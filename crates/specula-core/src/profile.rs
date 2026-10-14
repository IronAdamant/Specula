//! CPU profiler: named, nested zones per thread, aggregated per frame (foundation F3).
//!
//! ```text
//! use specula_core::profile;
//! profile::set_enabled(true);
//! {
//!     let _z = profile::zone("sim");
//!     let _t = profile::zone("tick");   // nested: path "sim/tick"
//! }
//! let mut collector = profile::Collector::new(120);
//! let frame = collector.collect(1);   // every thread's zones since the last collect
//! ```
//!
//! **Cost.** Disabled, [`zone`] is one relaxed atomic load and the guard's drop a branch. Enabled,
//! a zone reads the clock twice and, as it ends, adds its time to the thread's zone tree under a
//! `try_lock` that only the collector ever contends. Once a thread is registered (its first zone,
//! or [`register_thread`]), recording never allocates: the zone stack, the tree and the trace ring
//! (allocated by [`set_tracing`], never while recording) have fixed capacities. Past them a sample
//! is dropped and counted.
//!
//! **Threads.** Each thread records into its own buffers. Recording never blocks: if the collector
//! holds a thread's buffers at that instant, the sample is dropped and counted, so the profiler is
//! safe on an audio thread. The first zone on a thread registers it (one short lock and a few
//! allocations): call [`register_thread`] before a real-time loop to do that up front.
//!
//! **Memory.** With [`crate::alloc::Counting`] installed, each zone also records the allocations
//! its thread made while it was open.
//!
//! **Frames.** [`Collector::collect`] (call it once per frame, from one place) takes every
//! thread's tree, merges threads of the same name (worker pools), and keeps a rolling history.

use crate::alloc;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::Instant;

/// Deepest zone nesting recorded per thread; deeper zones are dropped (and counted).
pub const MAX_DEPTH: usize = 64;
/// Distinct zone paths per thread per frame.
pub const MAX_NODES: usize = 512;
/// Trace events kept per thread between collects while tracing (oldest overwritten).
pub const TRACE_CAPACITY: usize = 16_384;

/// No parent (a top-level zone).
const NONE: u32 = u32::MAX;

static ENABLED: AtomicBool = AtomicBool::new(false);
static TRACING: AtomicBool = AtomicBool::new(false);
static NEXT_TID: AtomicU32 = AtomicU32::new(1);
static REGISTRY: Mutex<Vec<Arc<Shared>>> = Mutex::new(Vec::new());
static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Turns recording on or off for every thread.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Relaxed)
}

/// Also records every zone instance (start, duration) for a timeline (see [`TraceEvent`]).
/// Turning it on gives every registered thread its trace ring here (threads registering later get
/// theirs as they register), so recording itself never allocates; [`Collector::collect`] frees
/// the rings once tracing is off again.
pub fn set_tracing(on: bool) {
    epoch();
    TRACING.store(on, Relaxed);
    if on {
        let threads: Vec<Arc<Shared>> = lock(&REGISTRY).clone();
        for t in threads {
            t.reserve_trace();
        }
    }
}

pub fn tracing() -> bool {
    TRACING.load(Relaxed)
}

/// The instant trace timestamps count from.
pub fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

/// Opens a zone on this thread; it closes when the guard drops. `name` should be short and
/// fixed (`"sim"`, `"autosave.write"`); a zone's path is its ancestors' names joined with `/`.
#[inline]
pub fn zone(name: &'static str) -> Zone {
    if !ENABLED.load(Relaxed) {
        return Zone { id: 0, _thread_bound: PhantomData };
    }
    Zone { id: begin(name), _thread_bound: PhantomData }
}

/// An open zone. Keep it in a named binding (`let _z = ...`): `let _ = ...` closes it at once.
///
/// Bound to the thread that opened it (not `Send`). Guards dropped out of order are tolerated:
/// closing a zone also discards zones opened inside it that are still open (counted as dropped),
/// and their guards' later drops are ignored.
#[must_use = "the zone closes when this guard drops"]
pub struct Zone {
    /// This zone's id on its thread; 0 = not recording.
    id: u64,
    _thread_bound: PhantomData<*const ()>,
}

impl Drop for Zone {
    #[inline]
    fn drop(&mut self) {
        if self.id != 0 {
            end(self.id);
        }
    }
}

/// Registers this thread now (allocating its buffers) instead of at its first zone.
pub fn register_thread() {
    let _ = LOCAL.try_with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            if slot.is_none() {
                *slot = Some(register());
            }
        }
    });
}

#[derive(Clone, Copy)]
struct Open {
    id: u64,
    name: &'static str,
    start: Instant,
    allocs: (u64, u64),
    /// Cached node index in the tree of generation `generation`.
    node: u32,
    generation: u64,
}

struct Local {
    stack: Vec<Open>,
    shared: Arc<Shared>,
    /// Id of the next zone opened on this thread (never 0).
    next_id: u64,
}

thread_local! {
    static LOCAL: RefCell<Option<Local>> = const { RefCell::new(None) };
}

struct Shared {
    tid: u32,
    name: String,
    data: Mutex<Data>,
    /// Samples lost: zones too deep, a full tree, the collector holding the buffers, or zones
    /// discarded because a guard was dropped out of order.
    dropped: AtomicU64,
}

impl Shared {
    /// Gives the thread its trace ring, allocated outside its lock.
    fn reserve_trace(&self) {
        if lock(&self.data).trace.capacity() != 0 {
            return;
        }
        let ring = Vec::with_capacity(TRACE_CAPACITY);
        let mut data = lock(&self.data);
        if data.trace.capacity() == 0 {
            data.trace = ring;
        }
    }
}

struct Data {
    /// Bumped by every collect, invalidating the node indices cached in zone stacks.
    generation: u64,
    tree: Tree,
    /// The trace ring: allocated by [`set_tracing`] or registration, never while recording.
    trace: Vec<TraceEvent>,
    /// Next slot to overwrite once `trace` is full.
    trace_head: usize,
    /// Trace events overwritten (ring full) or not recorded (no ring yet).
    trace_overwritten: u64,
}

fn register() -> Local {
    let tid = NEXT_TID.fetch_add(1, Relaxed);
    let name = std::thread::current().name().map_or_else(|| format!("thread-{tid}"), str::to_owned);
    let shared = Arc::new(Shared {
        tid,
        name,
        data: Mutex::new(Data {
            generation: 1,
            tree: Tree::with_capacity(MAX_NODES),
            trace: Vec::new(),
            trace_head: 0,
            trace_overwritten: 0,
        }),
        dropped: AtomicU64::new(0),
    });
    lock(&REGISTRY).push(shared.clone());
    if TRACING.load(Relaxed) {
        shared.reserve_trace();
    }
    Local { stack: Vec::with_capacity(MAX_DEPTH), shared, next_id: 1 }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Opens a zone; returns its id (0 = not recorded).
fn begin(name: &'static str) -> u64 {
    LOCAL
        .try_with(|cell| {
            let Ok(mut slot) = cell.try_borrow_mut() else { return 0 };
            let local = slot.get_or_insert_with(register);
            if local.stack.len() >= MAX_DEPTH {
                local.shared.dropped.fetch_add(1, Relaxed);
                return 0;
            }
            let id = local.next_id;
            local.next_id += 1;
            let allocs = alloc::thread_counts();
            local.stack.push(Open { id, name, start: Instant::now(), allocs, node: NONE, generation: 0 });
            id
        })
        .unwrap_or(0)
}

fn end(id: u64) {
    let _ = LOCAL.try_with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else { return };
        let Some(Local { stack, shared, .. }) = slot.as_mut() else { return };
        // Normally the top. Not found: it was discarded when an enclosing zone closed first.
        let Some(at) = stack.iter().rposition(|o| o.id == id) else { return };
        if at + 1 < stack.len() {
            // Zones opened inside this one whose guards are still alive: discard them.
            shared.dropped.fetch_add((stack.len() - at - 1) as u64, Relaxed);
            stack.truncate(at + 1);
        }
        let top = stack[at];
        let now = Instant::now();
        let allocs = alloc::thread_counts();
        let mut data = match shared.data.try_lock() {
            Ok(d) => d,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) => {
                // The collector has the buffers right now: never wait (audio threads use this).
                shared.dropped.fetch_add(1, Relaxed);
                stack.pop();
                return;
            }
        };
        let node = resolve(stack, &mut data);
        stack.pop();
        let dur = now.saturating_duration_since(top.start).as_nanos() as u64;
        let Some(node) = node else {
            shared.dropped.fetch_add(1, Relaxed);
            return;
        };
        let n = &mut data.tree.nodes[node as usize];
        n.calls += 1;
        n.total_ns += dur;
        n.max_ns = n.max_ns.max(dur);
        n.alloc_count += allocs.0.wrapping_sub(top.allocs.0);
        n.alloc_bytes += allocs.1.wrapping_sub(top.allocs.1);
        if TRACING.load(Relaxed) {
            let start_ns = top.start.saturating_duration_since(epoch()).as_nanos() as u64;
            let event = TraceEvent { tid: shared.tid, name: top.name, start_ns, dur_ns: dur, depth: stack.len() as u16 };
            data.push_trace(event);
        }
    });
}

impl Data {
    /// Never allocates: without a ring (tracing just turned on) the event is counted as lost.
    fn push_trace(&mut self, event: TraceEvent) {
        let cap = self.trace.capacity().min(TRACE_CAPACITY);
        if cap == 0 {
            self.trace_overwritten += 1;
        } else if self.trace.len() < cap {
            self.trace.push(event);
        } else {
            self.trace[self.trace_head] = event;
            self.trace_head = (self.trace_head + 1) % cap;
            self.trace_overwritten += 1;
        }
    }
}

/// The tree node of the top zone of `stack`, creating nodes as needed and caching them in the
/// stack entries (valid until the next collect). `None` if the tree is full.
fn resolve(stack: &mut [Open], data: &mut Data) -> Option<u32> {
    let n = stack.len();
    let mut first = n;
    while first > 0 && !(stack[first - 1].generation == data.generation && stack[first - 1].node != NONE) {
        first -= 1;
    }
    let mut parent = if first == 0 { NONE } else { stack[first - 1].node };
    for open in &mut stack[first..] {
        parent = data.tree.find_or_add(parent, open.name)?;
        open.node = parent;
        open.generation = data.generation;
    }
    Some(parent)
}

/// One zone path's numbers for a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneStat {
    pub name: &'static str,
    /// Nesting depth (0 = top level).
    pub depth: u16,
    pub calls: u32,
    /// Time inside the zone, children included.
    pub total_ns: u64,
    /// `total_ns` minus the children's (filled in by the collector).
    pub self_ns: u64,
    /// Longest single call.
    pub max_ns: u64,
    /// Allocations by this thread while the zone was open, children included.
    pub alloc_count: u64,
    pub alloc_bytes: u64,
    /// Allocations not inside a child zone (filled in by the collector).
    pub self_alloc_count: u64,
    pub self_alloc_bytes: u64,
    parent: u32,
    first_child: u32,
    last_child: u32,
    next_sibling: u32,
}

impl ZoneStat {
    fn new(name: &'static str, parent: u32, depth: u16) -> ZoneStat {
        ZoneStat {
            name,
            depth,
            calls: 0,
            total_ns: 0,
            self_ns: 0,
            max_ns: 0,
            alloc_count: 0,
            alloc_bytes: 0,
            self_alloc_count: 0,
            self_alloc_bytes: 0,
            parent,
            first_child: NONE,
            last_child: NONE,
            next_sibling: NONE,
        }
    }

    /// Index of the parent zone in the same list.
    pub fn parent(&self) -> Option<usize> {
        (self.parent != NONE).then_some(self.parent as usize)
    }

    pub fn total_ms(&self) -> f64 {
        self.total_ns as f64 / 1e6
    }

    pub fn self_ms(&self) -> f64 {
        self.self_ns as f64 / 1e6
    }

    pub fn max_ms(&self) -> f64 {
        self.max_ns as f64 / 1e6
    }
}

/// Zone nodes in creation order (a parent always comes before its children), with sibling links.
#[derive(Clone, Debug, Default)]
struct Tree {
    nodes: Vec<ZoneStat>,
    first_root: u32,
    last_root: u32,
}

impl Tree {
    fn with_capacity(n: usize) -> Tree {
        Tree { nodes: Vec::with_capacity(n), first_root: NONE, last_root: NONE }
    }

    /// Copies `other` into this tree's existing buffer (no allocation once it is big enough).
    fn copy_from(&mut self, other: &Tree) {
        self.nodes.clear();
        self.nodes.extend_from_slice(&other.nodes);
        self.first_root = other.first_root;
        self.last_root = other.last_root;
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.first_root = NONE;
        self.last_root = NONE;
    }

    fn find_or_add(&mut self, parent: u32, name: &'static str) -> Option<u32> {
        let mut c = if parent == NONE { self.first_root } else { self.nodes[parent as usize].first_child };
        while c != NONE {
            let node = &self.nodes[c as usize];
            if std::ptr::eq(node.name, name) || node.name == name {
                return Some(c);
            }
            c = node.next_sibling;
        }
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let idx = self.nodes.len() as u32;
        let depth = if parent == NONE { 0 } else { self.nodes[parent as usize].depth + 1 };
        self.nodes.push(ZoneStat::new(name, parent, depth));
        let last = if parent == NONE { self.last_root } else { self.nodes[parent as usize].last_child };
        if last == NONE {
            if parent == NONE {
                self.first_root = idx;
            } else {
                self.nodes[parent as usize].first_child = idx;
            }
        } else {
            self.nodes[last as usize].next_sibling = idx;
        }
        if parent == NONE {
            self.last_root = idx;
        } else {
            self.nodes[parent as usize].last_child = idx;
        }
        Some(idx)
    }

    /// Indices in depth-first order (parents before children, siblings in creation order).
    fn ordered(&self, out: &mut Vec<usize>) {
        out.clear();
        let mut stack = vec![self.first_root];
        while let Some(c) = stack.pop() {
            if c == NONE {
                continue;
            }
            let node = &self.nodes[c as usize];
            out.push(c as usize);
            stack.push(node.next_sibling);
            stack.push(node.first_child);
        }
    }

    fn fill_self(&mut self) {
        for n in &mut self.nodes {
            n.self_ns = n.total_ns;
            n.self_alloc_count = n.alloc_count;
            n.self_alloc_bytes = n.alloc_bytes;
        }
        for i in 0..self.nodes.len() {
            let n = self.nodes[i];
            if n.parent != NONE {
                let p = &mut self.nodes[n.parent as usize];
                p.self_ns = p.self_ns.saturating_sub(n.total_ns);
                p.self_alloc_count = p.self_alloc_count.saturating_sub(n.alloc_count);
                p.self_alloc_bytes = p.self_alloc_bytes.saturating_sub(n.alloc_bytes);
            }
        }
    }
}

/// One zone instance, recorded while tracing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    /// The recording thread (see [`ThreadProfile::tids`]).
    pub tid: u32,
    pub name: &'static str,
    /// Since [`epoch`].
    pub start_ns: u64,
    pub dur_ns: u64,
    pub depth: u16,
}

/// The zones of every thread with one name, for one frame.
#[derive(Clone, Debug, Default)]
pub struct ThreadProfile {
    pub thread: String,
    /// Thread ids merged under this name (trace events carry them).
    pub tids: Vec<u32>,
    /// Samples lost this frame (see the module docs).
    pub dropped: u64,
    /// Trace events overwritten because the ring was full.
    pub trace_overwritten: u64,
    tree: Tree,
}

impl ThreadProfile {
    pub fn zones(&self) -> &[ZoneStat] {
        &self.tree.nodes
    }

    /// Zone indices in depth-first order.
    pub fn ordered(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.tree.nodes.len());
        self.tree.ordered(&mut out);
        out
    }

    /// `"frame/sim/tick"` for the zone at `index`.
    pub fn path(&self, index: usize) -> String {
        let mut parts = Vec::new();
        let mut i = Some(index);
        while let Some(at) = i {
            let z = &self.tree.nodes[at];
            parts.push(z.name);
            i = z.parent();
        }
        parts.reverse();
        parts.join("/")
    }

    /// The zone at `path` (`"frame/sim"`).
    pub fn find(&self, path: &str) -> Option<&ZoneStat> {
        let mut parent = NONE;
        let mut found = None;
        for part in path.split('/') {
            let mut c = if parent == NONE { self.tree.first_root } else { self.tree.nodes[parent as usize].first_child };
            found = None;
            while c != NONE {
                let z = &self.tree.nodes[c as usize];
                if z.name == part {
                    found = Some(c);
                    break;
                }
                c = z.next_sibling;
            }
            parent = found?;
        }
        found.map(|i| &self.tree.nodes[i as usize])
    }
}

/// Heap use over one frame (zeros and `tracking: false` without [`alloc::Counting`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemorySample {
    pub tracking: bool,
    pub current_bytes: u64,
    pub peak_bytes: u64,
    /// Allocations in the whole process since the previous collect.
    pub allocations: u64,
    pub allocated_bytes: u64,
    pub frees: u64,
}

/// Everything recorded between two collects.
#[derive(Clone, Debug, Default)]
pub struct FrameProfile {
    pub frame: u64,
    /// When this frame was collected.
    pub at: Option<Instant>,
    pub threads: Vec<ThreadProfile>,
    /// Trace events (only while tracing), oldest first per thread.
    pub events: Vec<TraceEvent>,
    pub memory: MemorySample,
}

impl FrameProfile {
    pub fn thread(&self, name: &str) -> Option<&ThreadProfile> {
        self.threads.iter().find(|t| t.thread == name)
    }

    /// The thread name for a trace event's `tid`.
    pub fn thread_of(&self, tid: u32) -> Option<&str> {
        self.threads.iter().find(|t| t.tids.contains(&tid)).map(|t| t.thread.as_str())
    }
}

/// Rolling statistics of one zone path over the collector's history.
#[derive(Clone, Debug, PartialEq)]
pub struct RollingZone {
    pub thread: String,
    pub path: String,
    pub depth: u16,
    /// Frames in the history where the zone ran.
    pub frames: usize,
    pub calls: u64,
    /// Per-frame total time over the frames it ran in.
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_frame_ms: f64,
    /// Longest single call.
    pub max_call_ms: f64,
    /// Per-frame mean of self time.
    pub self_mean_ms: f64,
    /// Per-frame means of allocations made in the zone (children excluded).
    pub self_allocs_mean: f64,
    pub self_alloc_bytes_mean: f64,
}

/// Collects every thread's zones once per frame and keeps the last `history` frames.
/// Use one collector per process: each collect takes what all threads recorded.
pub struct Collector {
    capacity: usize,
    history: VecDeque<FrameProfile>,
    scratch: Tree,
    trace_scratch: Vec<TraceEvent>,
    map: Vec<u32>,
    last_alloc: alloc::AllocStats,
}

impl Collector {
    pub fn new(history: usize) -> Collector {
        Collector {
            capacity: history.max(1),
            history: VecDeque::with_capacity(history.max(1)),
            scratch: Tree::with_capacity(MAX_NODES),
            trace_scratch: Vec::new(),
            map: Vec::with_capacity(MAX_NODES),
            last_alloc: alloc::stats(),
        }
    }

    /// Takes what every thread recorded since the last collect as frame `frame`.
    pub fn collect(&mut self, frame: u64) -> &FrameProfile {
        let mut fp = if self.history.len() >= self.capacity { self.history.pop_front().unwrap_or_default() } else { FrameProfile::default() };
        fp.frame = frame;
        fp.at = Some(Instant::now());
        fp.events.clear();
        // Reuse last time's thread profiles (and their buffers).
        let mut spare = std::mem::take(&mut fp.threads);
        let tracing = TRACING.load(Relaxed);
        {
            let mut registry = lock(&REGISTRY);
            for shared in registry.iter() {
                let mut freed_ring = Vec::new();
                let (overwritten, has_events) = {
                    let mut data = lock(&shared.data);
                    self.scratch.copy_from(&data.tree);
                    data.tree.clear();
                    data.generation += 1;
                    let head = data.trace_head;
                    self.trace_scratch.clear();
                    self.trace_scratch.extend_from_slice(&data.trace[head..]);
                    self.trace_scratch.extend_from_slice(&data.trace[..head]);
                    data.trace.clear();
                    data.trace_head = 0;
                    let overwritten = std::mem::take(&mut data.trace_overwritten);
                    if !tracing && data.trace.capacity() != 0 {
                        freed_ring = std::mem::take(&mut data.trace);
                    }
                    (overwritten, !self.trace_scratch.is_empty())
                };
                drop(freed_ring);
                let dropped = shared.dropped.swap(0, Relaxed);
                if self.scratch.nodes.is_empty() && !has_events && dropped == 0 && overwritten == 0 {
                    continue;
                }
                if tracing || has_events {
                    fp.events.extend_from_slice(&self.trace_scratch);
                }
                let tp = match fp.threads.iter().position(|t| t.thread == shared.name) {
                    Some(i) => &mut fp.threads[i],
                    None => {
                        let mut t = spare.pop().unwrap_or_default();
                        t.thread.clear();
                        t.thread.push_str(&shared.name);
                        t.tids.clear();
                        t.dropped = 0;
                        t.trace_overwritten = 0;
                        t.tree.clear();
                        fp.threads.push(t);
                        fp.threads.last_mut().expect("just pushed")
                    }
                };
                tp.tids.push(shared.tid);
                tp.dropped += dropped;
                tp.trace_overwritten += overwritten;
                // Merge (creation order guarantees parents are mapped before children).
                self.map.clear();
                for z in &self.scratch.nodes {
                    let parent = if z.parent == NONE { NONE } else { self.map[z.parent as usize] };
                    let dest = if parent == NONE && z.parent != NONE { None } else { tp.tree.find_or_add(parent, z.name) };
                    match dest {
                        Some(d) => {
                            let n = &mut tp.tree.nodes[d as usize];
                            n.calls += z.calls;
                            n.total_ns += z.total_ns;
                            n.max_ns = n.max_ns.max(z.max_ns);
                            n.alloc_count += z.alloc_count;
                            n.alloc_bytes += z.alloc_bytes;
                            self.map.push(d);
                        }
                        None => {
                            tp.dropped += u64::from(z.calls);
                            self.map.push(NONE);
                        }
                    }
                }
            }
            // Threads that have exited and whose last zones were just collected.
            registry.retain(|s| Arc::strong_count(s) > 1);
        }
        for t in &mut fp.threads {
            t.tree.fill_self();
        }
        let now = alloc::stats();
        fp.memory = MemorySample {
            tracking: now.installed,
            current_bytes: now.current_bytes,
            peak_bytes: now.peak_bytes,
            allocations: now.allocations.saturating_sub(self.last_alloc.allocations),
            allocated_bytes: now.allocated_bytes.saturating_sub(self.last_alloc.allocated_bytes),
            frees: now.frees.saturating_sub(self.last_alloc.frees),
        };
        self.last_alloc = now;
        self.history.push_back(fp);
        self.history.back().expect("just pushed")
    }

    pub fn latest(&self) -> Option<&FrameProfile> {
        self.history.back()
    }

    /// Leaves allocations the caller made for its own bookkeeping (e.g. recording a capture
    /// right after a collect) out of the next frame's [`MemorySample`].
    pub fn discount_allocations(&mut self, count: u64, bytes: u64) {
        self.last_alloc.allocations += count;
        self.last_alloc.allocated_bytes += bytes;
    }

    /// Oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &FrameProfile> + ExactSizeIterator {
        self.history.iter()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Per (thread, path) statistics over the history, in the latest frames' tree order.
    pub fn rolling(&self) -> Vec<RollingZone> {
        rolling(self.history.iter())
    }
}

/// Rolling statistics over any run of frames (see [`Collector::rolling`]).
pub fn rolling<'a>(frames: impl Iterator<Item = &'a FrameProfile>) -> Vec<RollingZone> {
    struct Acc {
        depth: u16,
        totals: Vec<f64>,
        calls: u64,
        max_call_ns: u64,
        self_ns: u64,
        self_allocs: u64,
        self_bytes: u64,
    }
    let mut order: Vec<(String, String)> = Vec::new();
    let mut acc: HashMap<(String, String), Acc> = HashMap::new();
    let mut idx = Vec::new();
    for fp in frames {
        for t in &fp.threads {
            t.tree.ordered(&mut idx);
            for &i in &idx {
                let z = &t.tree.nodes[i];
                let key = (t.thread.clone(), t.path(i));
                let a = acc.entry(key.clone()).or_insert_with(|| {
                    order.push(key);
                    Acc { depth: z.depth, totals: Vec::new(), calls: 0, max_call_ns: 0, self_ns: 0, self_allocs: 0, self_bytes: 0 }
                });
                a.totals.push(z.total_ms());
                a.calls += u64::from(z.calls);
                a.max_call_ns = a.max_call_ns.max(z.max_ns);
                a.self_ns += z.self_ns;
                a.self_allocs += z.self_alloc_count;
                a.self_bytes += z.self_alloc_bytes;
            }
        }
    }
    order
        .into_iter()
        .map(|key| {
            let a = acc.remove(&key).expect("every key was ordered once");
            let mut sorted = a.totals;
            sorted.sort_by(f64::total_cmp);
            let n = sorted.len();
            let pick = |q: f64| sorted[((q * (n - 1) as f64).round() as usize).min(n - 1)];
            let frames = n as f64;
            RollingZone {
                thread: key.0,
                path: key.1,
                depth: a.depth,
                frames: n,
                calls: a.calls,
                mean_ms: sorted.iter().sum::<f64>() / frames,
                p50_ms: pick(0.5),
                p95_ms: pick(0.95),
                max_frame_ms: sorted[n - 1],
                max_call_ms: a.max_call_ns as f64 / 1e6,
                self_mean_ms: a.self_ns as f64 / 1e6 / frames,
                self_allocs_mean: a.self_allocs as f64 / frames,
                self_alloc_bytes_mean: a.self_bytes as f64 / frames,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The profiler is process-wide: tests that collect run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        lock(&SERIAL)
    }

    fn me() -> String {
        std::thread::current().name().unwrap_or("?").to_owned()
    }

    #[test]
    fn zones_nest_into_a_tree() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(4);
        c.collect(0);
        {
            let _a = zone("a");
            for _ in 0..2 {
                let _b = zone("b");
            }
            {
                let _c = zone("c");
                let _b = zone("b");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let fp = c.collect(1);
        let t = fp.thread(&me()).expect("this thread recorded");
        let order: Vec<String> = t.ordered().into_iter().map(|i| t.path(i)).collect();
        assert_eq!(order, ["a", "a/b", "a/c", "a/c/b"]);
        let a = t.find("a").unwrap();
        let ab = t.find("a/b").unwrap();
        let acb = t.find("a/c/b").unwrap();
        assert_eq!((a.calls, ab.calls, acb.calls), (1, 2, 1));
        assert_eq!((a.depth, ab.depth, acb.depth), (0, 1, 2));
        assert!(acb.total_ns >= 2_000_000, "{acb:?}");
        assert!(a.total_ns >= t.find("a/c").unwrap().total_ns + ab.total_ns);
        assert_eq!(a.self_ns, a.total_ns - ab.total_ns - t.find("a/c").unwrap().total_ns);
        assert!(t.find("a/x").is_none());
        // The next frame starts empty.
        assert!(c.collect(2).thread(&me()).is_none());
        set_enabled(false);
    }

    #[test]
    fn threads_with_one_name_merge() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(4);
        c.collect(0);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::Builder::new()
                    .name("profile-test-worker".into())
                    .spawn(|| {
                        let _w = zone("work");
                        let _i = zone("inner");
                    })
                    .unwrap()
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let fp = c.collect(1);
        let t = fp.thread("profile-test-worker").expect("workers recorded");
        assert_eq!(t.tids.len(), 4);
        assert_eq!(t.find("work").unwrap().calls, 4);
        assert_eq!(t.find("work/inner").unwrap().calls, 4);
        // Exited threads are dropped from the registry once collected.
        let alive = lock(&REGISTRY).iter().filter(|s| s.name == "profile-test-worker").count();
        assert_eq!(alive, 0);
        set_enabled(false);
    }

    #[test]
    fn disabled_records_nothing() {
        let _s = serial();
        set_enabled(false);
        let mut c = Collector::new(2);
        c.collect(0);
        {
            let _z = zone("off");
        }
        assert!(c.collect(1).thread(&me()).is_none());
        // A zone opened while enabled still closes cleanly after disabling.
        set_enabled(true);
        let z = zone("straddle");
        set_enabled(false);
        drop(z);
        let fp = c.collect(2);
        assert_eq!(fp.thread(&me()).unwrap().find("straddle").unwrap().calls, 1);
    }

    #[test]
    fn too_deep_and_too_many_are_dropped_and_counted() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(2);
        c.collect(0);
        fn nest(n: usize) {
            if n == 0 {
                return;
            }
            let _z = zone("deep");
            nest(n - 1);
        }
        nest(MAX_DEPTH + 6);
        let fp = c.collect(1);
        let t = fp.thread(&me()).unwrap();
        assert_eq!(t.dropped, 6);
        assert_eq!(t.zones().len(), MAX_DEPTH);
        // More distinct paths than the tree holds.
        let names: Vec<&'static str> = (0..MAX_NODES + 3).map(|i| &*Box::leak(format!("z{i}").into_boxed_str())).collect();
        for name in &names {
            let _z = zone(name);
        }
        let fp = c.collect(2);
        let t = fp.thread(&me()).unwrap();
        assert_eq!((t.zones().len(), t.dropped), (MAX_NODES, 3));
        set_enabled(false);
    }

    #[test]
    fn trace_ring_keeps_the_newest_events() {
        let _s = serial();
        set_enabled(true);
        set_tracing(true);
        let mut c = Collector::new(2);
        c.collect(0);
        for _ in 0..TRACE_CAPACITY + 10 {
            let _z = zone("traced");
        }
        let fp = c.collect(1);
        let mine: Vec<&TraceEvent> = {
            let tids = &fp.thread(&me()).unwrap().tids;
            fp.events.iter().filter(|e| tids.contains(&e.tid)).collect()
        };
        assert_eq!(mine.len(), TRACE_CAPACITY);
        assert_eq!(fp.thread(&me()).unwrap().trace_overwritten, 10);
        assert!(mine.windows(2).all(|w| w[0].start_ns <= w[1].start_ns), "oldest first");
        assert_eq!(fp.thread_of(mine[0].tid), Some(me().as_str()));
        set_tracing(false);
        set_enabled(false);
    }

    fn my_ring_capacity() -> usize {
        let me = me();
        let shared = lock(&REGISTRY).iter().find(|s| s.name == me).cloned().expect("registered");
        let cap = lock(&shared.data).trace.capacity();
        cap
    }

    /// The trace ring is allocated when tracing turns on (not while recording) and freed by the
    /// first collect after it turns off.
    #[test]
    fn trace_ring_is_allocated_outside_recording_and_freed() {
        let _s = serial();
        set_enabled(true);
        register_thread();
        assert_eq!(my_ring_capacity(), 0);
        set_tracing(true);
        assert!(my_ring_capacity() >= TRACE_CAPACITY);
        let before = alloc::thread_counts();
        {
            let _z = zone("first_traced");
        }
        assert_eq!(alloc::thread_counts(), before, "the first traced zone allocated");
        let mut c = Collector::new(2);
        assert_eq!(c.collect(1).events.iter().filter(|e| e.name == "first_traced").count(), 1);
        assert!(my_ring_capacity() >= TRACE_CAPACITY, "kept while tracing");
        set_tracing(false);
        c.collect(2);
        assert_eq!(my_ring_capacity(), 0, "freed once tracing is off");
        set_enabled(false);
    }

    /// Guards dropped out of order: the enclosing zone closes and discards the inner one, whose
    /// later drop is ignored; the thread's stack stays consistent.
    #[test]
    fn out_of_order_guards_keep_the_stack_consistent() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(2);
        c.collect(0);
        let a = zone("outer");
        let b = zone("inner");
        drop(a);
        drop(b);
        {
            let _z = zone("after");
        }
        let fp = c.collect(1);
        let t = fp.thread(&me()).unwrap();
        assert_eq!(t.find("outer").unwrap().calls, 1);
        assert!(t.find("outer/inner").is_none_or(|z| z.calls == 0));
        assert_eq!(t.find("after").unwrap().calls, 1, "a later zone is top level again");
        assert_eq!(t.dropped, 1);
        set_enabled(false);
    }

    /// Once warm, recording a zone allocates nothing (the counting allocator is installed for
    /// this crate's tests; its counters are per thread, so other tests can't interfere).
    #[test]
    fn recording_does_not_allocate_when_warm() {
        let _s = serial();
        set_enabled(true);
        set_tracing(true);
        let mut c = Collector::new(2);
        let work = || {
            for _ in 0..100 {
                let _a = zone("warm");
                let _b = zone("warm.inner");
            }
        };
        work();
        c.collect(0);
        work();
        c.collect(1);
        let before = alloc::thread_counts();
        work();
        let after = alloc::thread_counts();
        assert!(alloc::installed());
        assert_eq!(after, before, "zone recording allocated");
        let fp = c.collect(2);
        let warm = fp.thread(&me()).unwrap().find("warm").unwrap();
        assert_eq!(warm.calls, 100);
        assert_eq!(warm.alloc_count, 0);
        set_tracing(false);
        set_enabled(false);
    }

    #[test]
    fn zones_attribute_allocations() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(2);
        c.collect(0);
        {
            let _outer = zone("allocates");
            let v: Vec<u8> = Vec::with_capacity(1000);
            {
                let _inner = zone("inner");
                let w: Vec<u8> = Vec::with_capacity(500);
                drop(w);
            }
            drop(v);
        }
        let fp = c.collect(1);
        let t = fp.thread(&me()).unwrap();
        let outer = t.find("allocates").unwrap();
        let inner = t.find("allocates/inner").unwrap();
        assert_eq!((inner.alloc_count, inner.alloc_bytes), (1, 500));
        assert_eq!((outer.alloc_count, outer.alloc_bytes), (2, 1500));
        assert_eq!((outer.self_alloc_count, outer.self_alloc_bytes), (1, 1000));
        assert!(fp.memory.tracking && fp.memory.allocations >= 2);
        set_enabled(false);
    }

    /// A thread ending a zone while the collector holds its buffers drops the sample instead of
    /// waiting (the audio-thread guarantee).
    #[test]
    fn recording_never_blocks_on_the_collector() {
        let _s = serial();
        set_enabled(true);
        let (to_worker, worker_rx) = std::sync::mpsc::channel::<()>();
        let (to_main, main_rx) = std::sync::mpsc::channel::<()>();
        let worker = std::thread::Builder::new()
            .name("profile-test-realtime".into())
            .spawn(move || {
                register_thread();
                let z = zone("callback");
                to_main.send(()).unwrap();
                worker_rx.recv().unwrap();
                drop(z); // the buffers are locked now
                to_main.send(()).unwrap();
                worker_rx.recv().unwrap();
            })
            .unwrap();
        main_rx.recv().unwrap();
        let shared = lock(&REGISTRY).iter().find(|s| s.name == "profile-test-realtime").cloned().unwrap();
        {
            let _held = lock(&shared.data);
            to_worker.send(()).unwrap();
            main_rx.recv_timeout(Duration::from_secs(5)).expect("the zone end blocked on the collector");
        }
        assert_eq!(shared.dropped.load(Relaxed), 1);
        to_worker.send(()).unwrap();
        worker.join().unwrap();
        let mut c = Collector::new(1);
        c.collect(0);
        set_enabled(false);
    }

    /// Cost of one zone (open + close), enabled and disabled. A measurement, not a check:
    /// `cargo test --release -p specula-core --lib zone_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn zone_cost() {
        let _s = serial();
        const N: u32 = 1_000_000;
        let mut c = Collector::new(2);
        let run = || {
            let t = Instant::now();
            for _ in 0..N / 1000 {
                let _f = zone("frame");
                for _ in 0..999 {
                    let _z = zone("phase");
                }
            }
            t.elapsed().as_nanos() as f64 / f64::from(N)
        };
        set_enabled(false);
        let off = run();
        set_enabled(true);
        run();
        c.collect(0);
        let on = run();
        set_tracing(true);
        let traced = run();
        set_tracing(false);
        set_enabled(false);
        c.collect(1);
        let t = Instant::now();
        let mut last = t;
        for _ in 0..N {
            last = std::hint::black_box(Instant::now());
        }
        let clock = (last - t).as_nanos() as f64 / f64::from(N);
        println!("zone cost: disabled {off:.1} ns, enabled {on:.1} ns, enabled+tracing {traced:.1} ns (Instant::now {clock:.1} ns)");
    }

    #[test]
    fn rolling_statistics_span_the_history() {
        let _s = serial();
        set_enabled(true);
        let mut c = Collector::new(3);
        c.collect(0);
        for f in 1..=5u64 {
            for _ in 0..f {
                let _z = zone("rolling");
            }
            c.collect(f);
        }
        assert_eq!(c.history().len(), 3);
        assert_eq!(c.history().next().unwrap().frame, 3);
        let r = c.rolling();
        let z = r.iter().find(|z| z.thread == me() && z.path == "rolling").unwrap();
        assert_eq!((z.frames, z.calls), (3, 3 + 4 + 5));
        assert!(z.p95_ms >= z.p50_ms && z.max_frame_ms >= z.p95_ms);
        set_enabled(false);
    }
}

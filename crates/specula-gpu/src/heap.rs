//! The image heap: device-local memory for images, carved out of big blocks that a worker thread
//! allocates ahead of need (M5 gpuwork, `docs/design/WORKERS.md`, "GPU memory off the engine
//! thread").
//!
//! On the Intel iGPU one `vkAllocateMemory` for a large image took 45-178 ms on a loaded machine
//! (the driver commits and zeroes the pages), and every image used to get its own: a new capture
//! size, a window resize, a model's texture or a GI volume made one on the engine thread. Now:
//!
//! - **Blocks.** Images are placed in blocks of [`BLOCK_BYTES`] (first fit, at the image's
//!   alignment; freed ranges coalesce). An image bigger than a block gets a block its size. Only
//!   optimal-tiling images live here (every image Specula makes), so `bufferImageGranularity`
//!   never applies: no linear resource shares a block. An image whose driver *requires* its own
//!   allocation (`VkMemoryDedicatedRequirements`) still gets one.
//! - **Ahead.** After every placement the heap keeps a free range of at least [`RESERVE_BYTES`]
//!   per memory type (a 2048 x 2048 RGBA16F capture image fits), counting a growth in flight: when there is less, a worker thread (`specula-image-heap`) allocates the
//!   next block, and [`ImageHeap::poll`] adopts it. The engine thread never waits for one
//!   unless it must place an image now and no block holds it (`gpu.heap_wait` when a growth in
//!   flight would hold it, else `gpu.heap_sync`: a block allocated on the caller, counted in
//!   [`ImageHeapStats::sync_blocks`]).
//! - **Defer.** Callers that can come back later (a capture's prewarm, a slice a pass) ask with
//!   `defer`: no room means "not yet", a growth sized for the image starts, and the next call
//!   places it ([`ImageHeapStats::deferred`]).
//! - **Frees.** A range goes back to its block when its image is destroyed. An empty block is
//!   kept while the type has no other room for [`RESERVE_BYTES`], else (and always for a block
//!   bigger than [`BLOCK_BYTES`]) its memory is freed on the worker (`vkFreeMemory` of a big
//!   block costs the driver milliseconds too).
//!
//! Every `vkAllocateMemory` and `vkFreeMemory` the heap makes off the caller's thread is counted
//! ([`ImageHeapStats::grown_ahead`], [`ImageHeapStats::freed_ahead`]); the ones on the caller's thread too
//! ([`ImageHeapStats::sync_blocks`]), so a check can require none in a run.

use crate::device::RawAllocator;
use crate::vk::*;
use std::sync::mpsc::{channel, Receiver, SendError, Sender};
use std::time::{Duration, Instant};

/// Size of an ordinary block.
pub const BLOCK_BYTES: u64 = 64 << 20;
/// The free room (one range) the heap keeps ahead per memory type: a 2048 x 2048 RGBA16F image
/// (32 MB) and its alignment fit. Bigger images are rare (a 4K window's HDR image); they wait for
/// a growth sized for them, or defer. The memory held for it is at most a block plus this.
pub const RESERVE_BYTES: u64 = 40 << 20;
/// Bigger blocks are rounded up to this.
const BIG_ROUND: u64 = 16 << 20;
/// After a failed growth (out of memory, typically), no new one for this long: callers place
/// synchronously meanwhile, which reports the real error.
const RETRY_AFTER: Duration = Duration::from_secs(1);

/// Where an image's memory is: a range of a heap block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapRange {
    pub(crate) block: u32,
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

/// What the image heap holds and did (perf_memory `image_heap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageHeapStats {
    /// Blocks now, the bytes they hold, the bytes images use in them, and those images.
    pub blocks: u32,
    pub block_bytes: u64,
    pub used_bytes: u64,
    pub images: u32,
    /// Blocks allocated ahead on the worker thread (and adopted), and growths that failed.
    pub grown_ahead: u64,
    pub grow_failures: u64,
    /// Blocks allocated on the caller's thread: an image had to be placed now and no block or
    /// growth in flight held it.
    pub sync_blocks: u64,
    /// Placements that waited for a growth already in flight (the caller's thread blocked).
    pub growth_waits: u64,
    /// Deferred placements ("not yet": a growth started for them).
    pub deferred: u64,
    /// Images that required their own allocation (made on the caller's thread).
    pub dedicated: u64,
    /// Empty blocks freed on the worker.
    pub freed_ahead: u64,
    /// Buffers whose memory was allocated ahead on the worker (offscreen readbacks, sight's ID
    /// buffers: `Device::create_buffer_ahead`).
    pub buffers_ahead: u64,
    /// A growth is in flight now.
    pub growing: bool,
}

/// The size of a block made to hold `bytes`.
fn block_for(bytes: u64) -> u64 {
    if bytes <= BLOCK_BYTES {
        BLOCK_BYTES
    } else {
        bytes.div_ceil(BIG_ROUND) * BIG_ROUND
    }
}

fn align_up(v: u64, align: u64) -> u64 {
    let a = align.max(1);
    v.div_ceil(a) * a
}

/// Free-range bookkeeping of one block (no Vulkan objects: unit tested on its own).
#[derive(Debug)]
pub(crate) struct Ranges {
    size: u64,
    /// Free ranges `(offset, bytes)`, sorted by offset, never adjacent.
    free: Vec<(u64, u64)>,
    used: u64,
    count: u32,
}

impl Ranges {
    pub(crate) fn new(size: u64) -> Ranges {
        Ranges { size, free: vec![(0, size)], used: 0, count: 0 }
    }

    /// The first free place for `bytes` at `align`: its offset.
    pub(crate) fn take(&mut self, bytes: u64, align: u64) -> Option<u64> {
        let i = self.free.iter().position(|&(at, n)| align_up(at, align) - at + bytes <= n)?;
        let (at, n) = self.free[i];
        let start = align_up(at, align);
        let end = start + bytes;
        let mut parts = [(at, start - at), (end, at + n - end)].into_iter().filter(|p| p.1 > 0);
        match (parts.next(), parts.next()) {
            (Some(a), Some(b)) => {
                self.free[i] = a;
                self.free.insert(i + 1, b);
            }
            (Some(a), None) => self.free[i] = a,
            _ => {
                self.free.remove(i);
            }
        }
        self.used += bytes;
        self.count += 1;
        Some(start)
    }

    /// Gives `bytes` at `at` back (a range [`Ranges::take`] returned).
    pub(crate) fn give(&mut self, at: u64, bytes: u64) {
        let i = self.free.partition_point(|&(o, _)| o < at);
        self.free.insert(i, (at, bytes));
        if i + 1 < self.free.len() && self.free[i].0 + self.free[i].1 == self.free[i + 1].0 {
            self.free[i].1 += self.free[i + 1].1;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].0 + self.free[i - 1].1 == self.free[i].0 {
            self.free[i - 1].1 += self.free[i].1;
            self.free.remove(i);
        }
        self.used = self.used.saturating_sub(bytes);
        self.count = self.count.saturating_sub(1);
    }

    /// The largest free range.
    pub(crate) fn largest(&self) -> u64 {
        self.free.iter().map(|f| f.1).max().unwrap_or(0)
    }

    pub(crate) fn empty(&self) -> bool {
        self.count == 0
    }
}

struct Block {
    memory: VkDeviceMemory,
    type_index: u32,
    ranges: Ranges,
}

enum Job {
    /// A block: `(type index, size)`.
    Alloc(u32, u64),
    /// An allocation of its own for a buffer made ahead: `(ticket, type index, size, device
    /// address, map)` ([`ImageHeap::alloc_own`]).
    Own(u64, u32, u64, bool, bool),
    Free(VkDeviceMemory),
}

/// The worker's answer to an [`Job::Alloc`]: `(type index, size, memory)`.
type Grown = (u32, u64, Result<VkDeviceMemory, VkResult>);

/// An allocation of its own, and where it is mapped (0: not mapped).
type OwnMemory = Result<(VkDeviceMemory, usize), VkResult>;

/// What the worker sends back.
enum Done {
    Block(Grown),
    Own(u64, OwnMemory),
}

/// Two threads. Askers' own allocations and frees go to one (`tx`); speculative block growth to
/// the other (`grow`). An asker may wait for its own allocation on the engine thread (a
/// target's readback, at its first render), so it must never be queued behind a 64 MB growth,
/// which takes 45-178 ms on a loaded iGPU (review R1 of m5-gpuwork, M4: one thread took both in
/// order). `vkAllocateMemory` needs no external synchronisation, so the two run side by side.
struct Worker {
    tx: Option<Sender<Job>>,
    grow: Option<Sender<Job>>,
    rx: Receiver<Done>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

/// An allocation of its own made ahead on the worker ([`ImageHeap::alloc_own`]).
enum Own {
    /// Asked for, not back yet.
    Waiting,
    /// Back, not taken yet.
    Back(OwnMemory),
    /// Its asker gave up: freed when it comes back.
    Abandoned,
}

// SAFETY: VkDeviceMemory handles are plain values; the worker only allocates and frees memory no
// image uses (see `ImageHeap::release`), and `vkAllocateMemory` / `vkFreeMemory` need no external
// synchronisation on the device (Vulkan spec, "Threading Behavior").
unsafe impl Send for Job {}

impl Worker {
    fn start(alloc: RawAllocator) -> Option<Worker> {
        let (tx, jobs) = channel::<Job>();
        let (grow, growths) = channel::<Job>();
        let (done, rx) = channel::<Done>();
        let mut threads = Vec::new();
        for (name, jobs, done) in [("specula-image-heap", jobs, done.clone()), ("specula-image-heap-grow", growths, done)] {
            // (If the second can't start, the first ends when its sender is dropped here.)
            threads.push(std::thread::Builder::new().name(name.into()).spawn(move || Worker::run(alloc, jobs, done)).ok()?);
        }
        Some(Worker { tx: Some(tx), grow: Some(grow), rx, threads })
    }

    fn run(alloc: RawAllocator, jobs: Receiver<Job>, done: Sender<Done>) {
        for job in jobs {
            // Memory nobody is listening for any more (the heap is gone) is freed.
            match job {
                Job::Alloc(type_index, size) => {
                    let r = alloc.allocate(size, type_index, false);
                    if let Err(SendError(Done::Block((_, _, Ok(m))))) = done.send(Done::Block((type_index, size, r))) {
                        alloc.free(m);
                    }
                }
                Job::Own(ticket, type_index, size, address, map) => {
                    // (A failed map hands the memory back unmapped: the caller maps it.)
                    let r = alloc.allocate(size, type_index, address).map(|m| (m, if map { alloc.map_whole(m).unwrap_or(0) } else { 0 }));
                    if let Err(SendError(Done::Own(_, Ok((m, _))))) = done.send(Done::Own(ticket, r)) {
                        alloc.free(m);
                    }
                }
                Job::Free(m) => alloc.free(m),
            }
        }
    }
}

/// See the module docs.
pub(crate) struct ImageHeap {
    alloc: RawAllocator,
    blocks: Vec<Option<Block>>,
    worker: Option<Worker>,
    /// Growths in flight, in the order sent: `(type index, size)`.
    growing: Vec<(u32, u64)>,
    failed_at: Option<Instant>,
    stats: ImageHeapStats,
    /// Allocations of their own asked for ahead, by ticket.
    own: std::collections::BTreeMap<u64, Own>,
    next_ticket: u64,
}

impl ImageHeap {
    pub(crate) fn new(alloc: RawAllocator) -> ImageHeap {
        ImageHeap {
            alloc,
            blocks: Vec::new(),
            worker: None,
            growing: Vec::new(),
            failed_at: None,
            stats: ImageHeapStats::default(),
            own: std::collections::BTreeMap::new(),
            next_ticket: 1,
        }
    }

    /// Asks the worker for an allocation of its own (a buffer's: `device_address` when the
    /// buffer has one): its ticket, or `None` when no worker could start (the caller allocates
    /// itself).
    pub(crate) fn alloc_own(&mut self, type_index: u32, size: u64, device_address: bool, map: bool) -> Option<u64> {
        if self.worker.is_none() {
            self.worker = Worker::start(self.alloc);
        }
        let ticket = self.next_ticket;
        let tx = self.worker.as_ref().and_then(|w| w.tx.as_ref())?;
        tx.send(Job::Own(ticket, type_index, size, device_address, map)).ok()?;
        self.next_ticket += 1;
        self.own.insert(ticket, Own::Waiting);
        self.stats.buffers_ahead += 1;
        Some(ticket)
    }

    /// The allocation `ticket` asked for, once back (`wait`: blocks until it is). `None` while
    /// it is still on the worker.
    pub(crate) fn take_own(&mut self, ticket: u64, wait: bool) -> Option<OwnMemory> {
        loop {
            self.poll();
            match self.own.get(&ticket) {
                Some(Own::Back(_)) => {
                    let Some(Own::Back(r)) = self.own.remove(&ticket) else { unreachable!("checked") };
                    return Some(r);
                }
                Some(Own::Waiting) if wait => {
                    let _z = specula_core::profile::zone("gpu.heap_wait");
                    let got = self.worker.as_ref().and_then(|w| w.rx.recv().ok());
                    match got {
                        Some(d) => self.done(d),
                        None => {
                            // The worker is gone: nothing will come back.
                            self.own.remove(&ticket);
                            return Some(Err(VK_ERROR_OUT_OF_HOST_MEMORY));
                        }
                    }
                }
                Some(Own::Waiting) => return None,
                Some(Own::Abandoned) | None => return Some(Err(VK_ERROR_OUT_OF_HOST_MEMORY)),
            }
        }
    }

    /// The asker of `ticket` gave up: its memory is freed (now if back, else when it comes).
    pub(crate) fn abandon_own(&mut self, ticket: u64) {
        match self.own.remove(&ticket) {
            Some(Own::Back(Ok((m, _)))) => self.free_memory(m),
            Some(Own::Waiting) => {
                self.own.insert(ticket, Own::Abandoned);
            }
            _ => {}
        }
    }

    fn done(&mut self, d: Done) {
        match d {
            Done::Block(g) => self.adopt(g),
            Done::Own(ticket, r) => match self.own.get(&ticket) {
                Some(Own::Waiting) => {
                    self.own.insert(ticket, Own::Back(r));
                }
                _ => {
                    self.own.remove(&ticket);
                    if let Ok((m, _)) = r {
                        self.free_memory(m);
                    }
                }
            },
        }
    }

    pub(crate) fn stats(&self) -> ImageHeapStats {
        let mut s = self.stats;
        s.blocks = 0;
        s.block_bytes = 0;
        s.used_bytes = 0;
        s.images = 0;
        for b in self.blocks.iter().flatten() {
            s.blocks += 1;
            s.block_bytes += b.ranges.size;
            s.used_bytes += b.ranges.used;
            s.images += b.ranges.count;
        }
        s.growing = !self.growing.is_empty();
        s
    }

    /// An image got an allocation of its own (its driver required one).
    pub(crate) fn count_dedicated(&mut self) {
        self.stats.dedicated += 1;
    }

    /// The memory of block `i`.
    pub(crate) fn memory(&self, i: u32) -> VkDeviceMemory {
        self.blocks.get(i as usize).and_then(Option::as_ref).map_or(VK_NULL_HANDLE, |b| b.memory)
    }

    /// Adopts the blocks the worker has allocated since (never waits).
    pub(crate) fn poll(&mut self) {
        loop {
            let got = match self.worker.as_ref() {
                Some(w) => w.rx.try_recv().ok(),
                None => None,
            };
            let Some(d) = got else { return };
            self.done(d);
        }
    }

    fn adopt(&mut self, (type_index, size, r): Grown) {
        if let Some(i) = self.growing.iter().position(|&(t, s)| t == type_index && s == size) {
            self.growing.remove(i);
        }
        match r {
            Ok(memory) => {
                self.insert(Block { memory, type_index, ranges: Ranges::new(size) });
                self.stats.grown_ahead += 1;
            }
            Err(_) => {
                self.stats.grow_failures += 1;
                self.failed_at = Some(Instant::now());
            }
        }
    }

    fn insert(&mut self, b: Block) -> u32 {
        match self.blocks.iter().position(Option::is_none) {
            Some(i) => {
                self.blocks[i] = Some(b);
                i as u32
            }
            None => {
                self.blocks.push(Some(b));
                (self.blocks.len() - 1) as u32
            }
        }
    }

    /// Places `bytes` at `align` in a block of `type_index` (polls first). Keeps room ahead.
    pub(crate) fn place(&mut self, type_index: u32, bytes: u64, align: u64) -> Option<HeapRange> {
        self.poll();
        let found = self.find(type_index, bytes, align);
        self.keep_ahead(type_index);
        found
    }

    /// Starts the first block of `type_index` ahead of any image (at device creation).
    pub(crate) fn warm(&mut self, type_index: u32) {
        self.keep_ahead(type_index);
    }

    fn find(&mut self, type_index: u32, bytes: u64, align: u64) -> Option<HeapRange> {
        for (i, b) in self.blocks.iter_mut().enumerate() {
            let Some(b) = b.as_mut().filter(|b| b.type_index == type_index) else { continue };
            if let Some(offset) = b.ranges.take(bytes, align) {
                return Some(HeapRange { block: i as u32, offset, size: bytes });
            }
        }
        None
    }

    /// The largest free range of `type_index`, plus a growth of it in flight.
    fn room(&self, type_index: u32) -> u64 {
        let free = self.blocks.iter().flatten().filter(|b| b.type_index == type_index).map(|b| b.ranges.largest()).max().unwrap_or(0);
        let growing = self.growing.iter().filter(|g| g.0 == type_index).map(|g| g.1).max().unwrap_or(0);
        free.max(growing)
    }

    /// Starts a growth when `type_index` has less room than [`RESERVE_BYTES`] (one in flight at a time).
    fn keep_ahead(&mut self, type_index: u32) {
        if self.room(type_index) < RESERVE_BYTES {
            self.grow(type_index, BLOCK_BYTES);
        }
    }

    /// Room for an image of `bytes` is wanted soon (a deferred placement): a growth sized for it
    /// starts, unless one of this type is already in flight (the next call starts a bigger one
    /// if that one doesn't hold it). False when no growth can start (one failed a moment ago, or
    /// no worker): the caller places synchronously instead, which reports the real error.
    pub(crate) fn need(&mut self, type_index: u32, bytes: u64) -> bool {
        if !self.growing.iter().any(|g| g.0 == type_index) && !self.grow(type_index, block_for(bytes)) {
            return false;
        }
        self.stats.deferred += 1;
        true
    }

    /// Sends a growth of `size` to the worker (started on first use); false when none was sent
    /// (one of this type is in flight, one failed a moment ago, or no worker).
    fn grow(&mut self, type_index: u32, size: u64) -> bool {
        if self.growing.iter().any(|g| g.0 == type_index) || self.failed_at.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
            return false;
        }
        if self.worker.is_none() {
            self.worker = Worker::start(self.alloc);
        }
        let sent = self.worker.as_ref().and_then(|w| w.grow.as_ref()).is_some_and(|tx| tx.send(Job::Alloc(type_index, size)).is_ok());
        if sent {
            self.growing.push((type_index, size));
        }
        sent
    }

    /// Places `bytes` now: in a block, else in a growth in flight that holds it (waited for:
    /// `gpu.heap_wait`), else in a block allocated here (`gpu.heap_sync`).
    pub(crate) fn place_now(&mut self, type_index: u32, bytes: u64, align: u64) -> Result<HeapRange, VkResult> {
        if let Some(r) = self.place(type_index, bytes, align) {
            return Ok(r);
        }
        while self.growing.iter().any(|&(t, s)| t == type_index && s >= bytes + align) {
            let _z = specula_core::profile::zone("gpu.heap_wait");
            let got = self.worker.as_ref().and_then(|w| w.rx.recv().ok());
            let Some(d) = got else {
                self.growing.clear(); // the worker is gone
                break;
            };
            self.stats.growth_waits += 1;
            self.done(d);
            if let Some(r) = self.find(type_index, bytes, align) {
                self.keep_ahead(type_index);
                return Ok(r);
            }
        }
        let size = block_for(bytes + align);
        let memory = {
            let _z = specula_core::profile::zone("gpu.heap_sync");
            self.alloc.allocate(size, type_index, false)?
        };
        self.stats.sync_blocks += 1;
        let i = self.insert(Block { memory, type_index, ranges: Ranges::new(size) });
        let b = self.blocks[i as usize].as_mut().expect("just inserted");
        let offset = b.ranges.take(bytes, align).expect("a fresh block holds it");
        self.keep_ahead(type_index);
        Ok(HeapRange { block: i, offset, size: bytes })
    }

    /// Gives an image's range back (the image is destroyed). An empty block is freed (on the
    /// worker) when its type has room for [`RESERVE_BYTES`] without it, or when it is oversized.
    pub(crate) fn release(&mut self, r: HeapRange) {
        let Some(Some(b)) = self.blocks.get_mut(r.block as usize) else { return };
        b.ranges.give(r.offset, r.size);
        if !b.ranges.empty() {
            return;
        }
        let (type_index, size) = (b.type_index, b.ranges.size);
        let other = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| *i != r.block as usize && b.as_ref().is_some_and(|b| b.type_index == type_index))
            .filter_map(|(_, b)| b.as_ref().map(|b| b.ranges.largest()))
            .max()
            .unwrap_or(0);
        if size > BLOCK_BYTES || other >= RESERVE_BYTES {
            if let Some(b) = self.blocks[r.block as usize].take() {
                self.free_memory(b.memory);
            }
        }
    }

    fn free_memory(&mut self, m: VkDeviceMemory) {
        let sent = self.worker.as_ref().and_then(|w| w.tx.as_ref()).map(|tx| tx.send(Job::Free(m)));
        match sent {
            Some(Ok(())) => self.stats.freed_ahead += 1,
            Some(Err(SendError(_))) | None => self.alloc.free(m),
        }
    }

    /// Frees every block (no image may use them any more) after the worker finished its jobs.
    pub(crate) fn destroy(&mut self) {
        if let Some(mut w) = self.worker.take() {
            w.tx = None;
            w.grow = None;
            for t in w.threads.drain(..) {
                let _ = t.join();
            }
            while let Ok(d) = w.rx.try_recv() {
                if let Done::Block((_, _, Ok(m))) | Done::Own(_, Ok((m, _))) = d {
                    self.alloc.free(m);
                }
            }
        }
        for (_, o) in std::mem::take(&mut self.own) {
            if let Own::Back(Ok((m, _))) = o {
                self.alloc.free(m);
            }
        }
        self.growing.clear();
        for b in self.blocks.drain(..).flatten() {
            self.alloc.free(b.memory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;

    // A device whose large allocations are slow, as a loaded iGPU's are (no real device).
    unsafe extern "system" fn slow_allocate(_: VkDevice, info: *const VkMemoryAllocateInfo, _: *const c_void, out: *mut VkDeviceMemory) -> VkResult {
        // SAFETY: `RawAllocator::allocate` passes a valid info and out-pointer.
        let size = unsafe { (*info).allocation_size };
        if size >= 1 << 20 {
            std::thread::sleep(Duration::from_millis(400));
        }
        // SAFETY: as above; the "handle" is only ever handed back to `no_free`.
        unsafe { *out = size.max(1) };
        VK_SUCCESS
    }
    unsafe extern "system" fn no_free(_: VkDevice, _: VkDeviceMemory, _: *const c_void) {}
    unsafe extern "system" fn no_map(_: VkDevice, _: VkDeviceMemory, _: VkDeviceSize, _: VkDeviceSize, _: VkFlags, _: *mut *mut c_void) -> VkResult {
        VK_SUCCESS
    }

    /// Review R1 of m5-gpuwork, M4: an asker's own allocation (a new target's readback, waited
    /// for by its first render on the engine thread) was queued behind a speculative 64 MB block
    /// growth on the one worker thread.
    #[test]
    fn an_askers_own_allocation_never_waits_behind_a_block_growth() {
        let mut w = Worker::start(RawAllocator::for_test(slow_allocate, no_free, no_map)).expect("the heap's threads");
        w.grow.as_ref().unwrap().send(Job::Alloc(0, 64 << 20)).unwrap();
        w.tx.as_ref().unwrap().send(Job::Own(7, 0, 256 << 10, false, false)).unwrap();
        // The order says it, whatever the machine's speed: asked second, back first.
        let first = w.rx.recv_timeout(Duration::from_secs(30)).ok();
        assert!(matches!(first, Some(Done::Own(7, Ok(_)))), "the asker's own memory comes back before the growth asked ahead of it");
        assert!(matches!(w.rx.recv_timeout(Duration::from_secs(30)), Ok(Done::Block((0, _, Ok(_))))), "and the growth comes back when it is done");
        w.tx = None;
        w.grow = None;
        for t in w.threads.drain(..) {
            t.join().unwrap();
        }
    }

    /// A fake `vkAllocateMemory` whose handle says how it was asked: 2 with the device-address
    /// flag chained on, 1 without.
    unsafe extern "system" fn telling_allocate(_: VkDevice, info: *const VkMemoryAllocateInfo, _: *const c_void, out: *mut VkDeviceMemory) -> VkResult {
        // SAFETY: `RawAllocator::allocate` passes a valid info and out-pointer.
        unsafe { *out = if (*info).p_next.is_null() { 1 } else { 2 } };
        VK_SUCCESS
    }

    /// Mutation testing (Probatio, 2026-11-13) changed a block's allocation to ask for a shader
    /// device address and no test noticed. Blocks hold images: they never ask for one (a device
    /// without the feature refuses the flag, and one with it pays for address space no image
    /// uses). An asker's own allocation carries exactly the flag it asked for.
    #[test]
    fn blocks_never_ask_for_a_device_address_and_own_allocations_ask_as_told() {
        let mut w = Worker::start(RawAllocator::for_test(telling_allocate, no_free, no_map)).expect("the heap's threads");
        w.grow.as_ref().unwrap().send(Job::Alloc(0, 64 << 20)).unwrap();
        assert!(matches!(w.rx.recv_timeout(Duration::from_secs(30)), Ok(Done::Block((0, _, Ok(1))))), "a block: no device address");
        w.tx.as_ref().unwrap().send(Job::Own(1, 0, 4096, true, false)).unwrap();
        assert!(matches!(w.rx.recv_timeout(Duration::from_secs(30)), Ok(Done::Own(1, Ok((2, _))))), "asked with an address");
        w.tx.as_ref().unwrap().send(Job::Own(2, 0, 4096, false, false)).unwrap();
        assert!(matches!(w.rx.recv_timeout(Duration::from_secs(30)), Ok(Done::Own(2, Ok((1, _))))), "asked without");
        w.tx = None;
        w.grow = None;
        for t in w.threads.drain(..) {
            t.join().unwrap();
        }
    }

    #[test]
    fn ranges_take_at_alignment_and_coalesce_when_given_back() {
        let mut r = Ranges::new(1000);
        let a = r.take(100, 1).unwrap();
        let b = r.take(50, 64).unwrap();
        assert_eq!((a, b), (0, 128));
        assert_eq!(r.free, vec![(100, 28), (178, 822)]);
        let c = r.take(20, 1).unwrap();
        assert_eq!(c, 100, "the pad left before an aligned range is used first");
        assert_eq!(r.used, 170);
        r.give(b, 50);
        r.give(a, 100);
        r.give(c, 20);
        assert!(r.empty());
        assert_eq!(r.free, vec![(0, 1000)], "every range coalesced");
        assert_eq!(r.used, 0);
    }

    #[test]
    fn ranges_refuse_what_does_not_fit() {
        let mut r = Ranges::new(256);
        assert!(r.take(257, 1).is_none());
        assert_eq!(r.take(200, 1), Some(0));
        assert!(r.take(56, 64).is_none(), "56 bytes free at 200, but none at a 64-byte boundary with room");
        assert_eq!(r.take(56, 8), Some(200));
        assert_eq!(r.largest(), 0);
    }

    #[test]
    fn blocks_are_sized_for_what_they_hold() {
        const { assert!(RESERVE_BYTES >= (2048 * 2048 * 8) + (1 << 16), "a 2048 x 2048 RGBA16F image fits the room kept ahead") };
        const { assert!(RESERVE_BYTES <= BLOCK_BYTES) };
        assert_eq!(block_for(1), BLOCK_BYTES);
        assert_eq!(block_for(BLOCK_BYTES + 1), BLOCK_BYTES + BIG_ROUND);
    }
}

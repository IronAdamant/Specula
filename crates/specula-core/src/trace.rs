//! Structured tracing: levelled, coded events with typed fields, spans that line up with the
//! profiler's zones, and a call context that ties everything a call causes together
//! (docs/design/TRACE.md).
//!
//! ```text
//! use specula_core::{trace, trace_event};
//! static SUB: trace::Subsystem = trace::Subsystem::new("write_policy");
//! trace_event!(&SUB, Info, "write.refused", rule = "world", code = "claimed");
//! {
//!     let _s = trace::span(&SUB, trace::Level::Debug, "commit"); // also a profile zone
//!     trace_event!(&SUB, Debug, "step", changes = 3u64);         // carries the span's id
//! }
//! let events = trace::read(|ring| ring.iter().cloned().collect::<Vec<_>>());
//! ```
//!
//! **Cost.** A [`Subsystem`] holds its maximum level in one atomic: a disabled event is a relaxed
//! load and a compare, its fields are never evaluated and nothing allocates. An enabled event
//! builds its fields (one allocation) and goes into its thread's bounded buffer under a
//! lock only a drain contends (to move it, briefly); buffers drain into the central [`Ring`] when full and on
//! [`flush`] (readers, and the engine's sink thread every 100 ms, call it).
//!
//! **Determinism.** Simulation code may emit, but nothing here feeds back: emitting returns
//! nothing. The caller never passes time; the trace layer stamps it (and the engine's tick and
//! frame, set by the engine with [`set_now`]).

use crate::json::Json;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::time::Instant;

/// Events a thread buffers before they move to the ring.
pub const THREAD_CAPACITY: usize = 512;
/// Events the central ring keeps by default (oldest overwritten).
pub const RING_CAPACITY: usize = 8192;
/// Fields kept per event (more are dropped, and the event says so).
pub const MAX_FIELDS: usize = 16;
/// Characters kept of a string field.
pub const MAX_TEXT: usize = 256;
/// Distinct runtime names ([`subsystem`], [`intern`]) kept; past it, the fallback is used.
pub const MAX_NAMES: usize = 1024;

/// How much an event matters. Lower is more severe; a subsystem at `Info` records `Error`,
/// `Warn` and `Info`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub const ALL: [Level; 5] = [Level::Error, Level::Warn, Level::Info, Level::Debug, Level::Trace];

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }

    pub fn parse(s: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.as_str() == s)
    }

    fn from_u8(v: u8) -> Option<Level> {
        Level::ALL.into_iter().find(|l| *l as u8 == v)
    }
}

/// A setting: a level, or off.
pub fn parse_setting(s: &str) -> Option<Option<Level>> {
    if s == "off" {
        Some(None)
    } else {
        Level::parse(s).map(Some)
    }
}

pub fn setting_str(l: Option<Level>) -> &'static str {
    l.map_or("off", Level::as_str)
}

const OFF: u8 = 0;
/// Not registered yet: the first check registers the subsystem (and reads its setting).
const UNSET: u8 = u8::MAX;

/// A named source of events with its own level. Declare it `static`; the first event registers
/// it so [`set_level`] and [`subsystems`] see it.
pub struct Subsystem {
    name: &'static str,
    level: AtomicU8,
}

impl Subsystem {
    pub const fn new(name: &'static str) -> Subsystem {
        Subsystem { name, level: AtomicU8::new(UNSET) }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether an event at `level` is recorded: one relaxed load (after the first call).
    #[inline]
    pub fn enabled(&'static self, level: Level) -> bool {
        let max = self.level.load(Relaxed);
        if max == UNSET {
            return level as u8 <= self.register();
        }
        level as u8 <= max
    }

    /// The level it records now (`None`: off).
    pub fn level(&'static self) -> Option<Level> {
        let mut max = self.level.load(Relaxed);
        if max == UNSET {
            max = self.register();
        }
        Level::from_u8(max)
    }

    #[cold]
    fn register(&'static self) -> u8 {
        let mut reg = lock(&REGISTRY);
        if !reg.subsystems.iter().any(|s| std::ptr::eq(*s, self)) {
            reg.subsystems.push(self);
        }
        let v = reg.overrides.get(self.name).copied().unwrap_or_else(|| DEFAULT.load(Relaxed));
        self.level.store(v, Relaxed);
        v
    }

    /// Records an event (call it behind [`Subsystem::enabled`]; [`trace_event!`] does).
    pub fn emit(&'static self, level: Level, code: &'static str, fields: Vec<(&'static str, Value)>) {
        record(self.name, level, code, EventKind::Event, 0, fields);
    }
}

/// An event with typed fields, if `sub` records `level`. Fields aren't evaluated otherwise.
///
/// `trace_event!(&SUB, Warn, "engine.stall", tool = name, ms = 120.5)`
#[macro_export]
macro_rules! trace_event {
    ($sub:expr, $level:ident, $code:expr $(, $key:ident = $val:expr)* $(,)?) => {{
        let sub: &'static $crate::trace::Subsystem = $sub;
        if sub.enabled($crate::trace::Level::$level) {
            sub.emit($crate::trace::Level::$level, $code, vec![$((stringify!($key), $crate::trace::Value::from($val))),*]);
        }
    }};
}

struct Registry {
    subsystems: Vec<&'static Subsystem>,
    /// Levels set by name (applied as subsystems register).
    overrides: BTreeMap<String, u8>,
    /// Runtime names (modules' subsystems and codes), leaked once each.
    names: BTreeMap<String, &'static str>,
    dynamic: BTreeMap<&'static str, &'static Subsystem>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry { subsystems: Vec::new(), overrides: BTreeMap::new(), names: BTreeMap::new(), dynamic: BTreeMap::new() });
static DEFAULT: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// The level subsystems without their own setting record (default `info`).
pub fn set_default(level: Option<Level>) {
    let v = level.map_or(OFF, |l| l as u8);
    let reg = lock(&REGISTRY);
    DEFAULT.store(v, Relaxed);
    for s in &reg.subsystems {
        if !reg.overrides.contains_key(s.name) {
            s.level.store(v, Relaxed);
        }
    }
}

pub fn default_level() -> Option<Level> {
    Level::from_u8(DEFAULT.load(Relaxed))
}

/// Sets subsystem `name`'s level (`None`: off), now or when it registers. `reset` instead
/// returns it to the default.
pub fn set_level(name: &str, level: Option<Level>) {
    let v = level.map_or(OFF, |l| l as u8);
    let mut reg = lock(&REGISTRY);
    if reg.overrides.len() >= MAX_NAMES && !reg.overrides.contains_key(name) {
        return;
    }
    reg.overrides.insert(name.to_owned(), v);
    for s in reg.subsystems.iter().filter(|s| s.name == name) {
        s.level.store(v, Relaxed);
    }
}

/// Subsystem `name` back to the default level.
pub fn reset_level(name: &str) {
    let mut reg = lock(&REGISTRY);
    reg.overrides.remove(name);
    let v = DEFAULT.load(Relaxed);
    for s in reg.subsystems.iter().filter(|s| s.name == name) {
        s.level.store(v, Relaxed);
    }
}

/// Every registered subsystem and its level, by name.
pub fn subsystems() -> Vec<(&'static str, Option<Level>, bool)> {
    let reg = lock(&REGISTRY);
    let mut out: Vec<(&'static str, Option<Level>, bool)> =
        reg.subsystems.iter().map(|s| (s.name, Level::from_u8(s.level.load(Relaxed)), reg.overrides.contains_key(s.name))).collect();
    out.sort_by_key(|s| s.0);
    out.dedup_by_key(|s| s.0);
    out
}

/// A runtime name kept for the process (leaked once): module subsystems and codes. `None` once
/// [`MAX_NAMES`] are kept.
pub fn intern(name: &str) -> Option<&'static str> {
    let mut reg = lock(&REGISTRY);
    if let Some(n) = reg.names.get(name) {
        return Some(n);
    }
    if reg.names.len() >= MAX_NAMES {
        return None;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    reg.names.insert(name.to_owned(), leaked);
    Some(leaked)
}

static OVERFLOW: Subsystem = Subsystem::new("dynamic");

/// A subsystem named at runtime (a module's `module:<name>`), made once per name. Past
/// [`MAX_NAMES`] names, the shared `dynamic` subsystem.
pub fn subsystem(name: &str) -> &'static Subsystem {
    {
        let reg = lock(&REGISTRY);
        if let Some(n) = reg.names.get(name) {
            if let Some(s) = reg.dynamic.get(n) {
                return s;
            }
        }
    }
    let Some(n) = intern(name) else { return &OVERFLOW };
    let mut reg = lock(&REGISTRY);
    if let Some(s) = reg.dynamic.get(n) {
        return s;
    }
    let s: &'static Subsystem = Box::leak(Box::new(Subsystem::new(n)));
    reg.dynamic.insert(n, s);
    s
}

/// A typed field value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    Str(&'static str),
    Text(Box<str>),
}

impl Value {
    /// A string field, cut at [`MAX_TEXT`] characters.
    pub fn text(s: &str) -> Value {
        if s.len() <= MAX_TEXT {
            return Value::Text(s.into());
        }
        let cut: String = s.chars().take(MAX_TEXT).collect();
        Value::Text(format!("{cut}...").into_boxed_str())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::U64(v) => Some(*v),
            Value::I64(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::U64(v) => Some(*v as f64),
            Value::I64(v) => Some(*v as f64),
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        match self {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::from(*b),
            Value::U64(v) => Json::from(*v),
            Value::I64(v) => Json::from(*v),
            Value::F64(v) if v.is_finite() => Json::from((*v * 1000.0).round() / 1000.0),
            Value::F64(_) => Json::Null,
            Value::Str(s) => Json::from(*s),
            Value::Text(s) => Json::from(&**s),
        }
    }

    /// From JSON (a module's fields): numbers, strings, booleans; anything else as its text.
    pub fn from_json(j: &Json) -> Value {
        match j {
            Json::Null => Value::Null,
            Json::Bool(b) => Value::Bool(*b),
            Json::Num(n) if n.fract() == 0.0 && *n >= 0.0 && *n < 9.0e15 => Value::U64(*n as u64),
            Json::Num(n) if n.fract() == 0.0 && *n < 0.0 && *n > -9.0e15 => Value::I64(*n as i64),
            Json::Num(n) => Value::F64(*n),
            Json::Str(s) => Value::text(s),
            other => Value::text(&other.to_string()),
        }
    }

    fn bytes(&self) -> usize {
        match self {
            Value::Text(s) => s.len(),
            _ => 0,
        }
    }
}

macro_rules! value_from {
    ($($t:ty => $v:ident as $c:ty),* $(,)?) => {$(
        impl From<$t> for Value {
            fn from(v: $t) -> Value {
                Value::$v(v as $c)
            }
        }
    )*};
}
value_from!(u8 => U64 as u64, u16 => U64 as u64, u32 => U64 as u64, u64 => U64 as u64, usize => U64 as u64,
            i8 => I64 as i64, i16 => I64 as i64, i32 => I64 as i64, i64 => I64 as i64, isize => I64 as i64,
            f32 => F64 as f64, f64 => F64 as f64);

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::text(v)
    }
}

impl From<&String> for Value {
    fn from(v: &String) -> Value {
        Value::text(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Value {
        if v.len() <= MAX_TEXT {
            Value::Text(v.into_boxed_str())
        } else {
            Value::text(&v)
        }
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Value {
        v.map_or(Value::Null, Into::into)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    /// A point event.
    Event,
    /// A span's close: its duration (the event's `span` is the span itself, `parent` its parent).
    Span { dur_us: u64 },
}

/// One recorded event.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Position in the ring: 1, 2, 3... in the order events reached it; never reused.
    pub id: u64,
    pub level: Level,
    pub subsystem: &'static str,
    pub code: &'static str,
    pub kind: EventKind,
    /// The call that caused it (0: none, e.g. the simulation's own work).
    pub call: u64,
    /// The span it happened in (for a span's close: the span itself).
    pub span: u64,
    /// For a span's close: its parent span (0: none).
    pub parent: u64,
    /// The engine's simulation tick and frame as it was recorded (see [`set_now`]).
    pub tick: u64,
    pub frame: u64,
    /// Microseconds since the trace epoch ([`epoch_unix_ms`] gives its wall time).
    pub time_us: u64,
    pub thread: u32,
    pub fields: Vec<(&'static str, Value)>,
    /// Order of emission across threads (the ring sorts each drain by it).
    seq: u64,
}

impl Event {
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn dur_us(&self) -> Option<u64> {
        match self.kind {
            EventKind::Span { dur_us } => Some(dur_us),
            EventKind::Event => None,
        }
    }

    /// Approximate memory held.
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Event>() + self.fields.len() * std::mem::size_of::<(&str, Value)>() + self.fields.iter().map(|(_, v)| v.bytes()).sum::<usize>()
    }

    /// As JSON: ids, level, subsystem, code, context and fields. Zero context values are left out.
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("id", Json::from(self.id)),
            ("level", Json::from(self.level.as_str())),
            ("sub", Json::from(self.subsystem)),
            ("code", Json::from(self.code)),
        ]);
        let mut put = |k: &str, v: u64| {
            if v != 0 {
                j.set(k, Json::from(v));
            }
        };
        put("call", self.call);
        put("span", self.span);
        put("parent", self.parent);
        put("tick", self.tick);
        put("frame", self.frame);
        j.set("t_ms", Json::from((self.time_us / 100) as f64 / 10.0));
        if let EventKind::Span { dur_us } = self.kind {
            j.set("dur_ms", Json::from((dur_us / 10) as f64 / 100.0));
        }
        if !self.fields.is_empty() {
            j.set("fields", Json::Obj(self.fields.iter().map(|(k, v)| ((*k).to_owned(), v.to_json())).collect()));
        }
        j
    }

    /// One line for humans: `warn engine.stall call=12 tool=sight_frame ms=120.5`.
    pub fn line(&self) -> String {
        use std::fmt::Write as _;
        let mut s = format!("{} {} [{}]", self.level.as_str(), self.code, self.subsystem);
        if self.call != 0 {
            let _ = write!(s, " call={}", self.call);
        }
        if let EventKind::Span { dur_us } = self.kind {
            let _ = write!(s, " dur_ms={:.2}", dur_us as f64 / 1000.0);
        }
        for (k, v) in &self.fields {
            let _ = write!(s, " {k}={}", v.to_json());
        }
        s
    }
}

// ── context ─────────────────────────────────────────────────────────────────

/// What the current thread's work is part of: a call (and a span within it). Copy it into a job
/// and [`Ctx::enter`] it on the worker so the job's events join the call's tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ctx {
    pub call: u64,
    pub span: u64,
}

thread_local! {
    static CTX: Cell<Ctx> = const { Cell::new(Ctx { call: 0, span: 0 }) };
    static THREAD_TICK: Cell<Option<u64>> = const { Cell::new(None) };
}

impl Ctx {
    /// This thread's context.
    pub fn current() -> Ctx {
        CTX.try_with(Cell::get).unwrap_or_default()
    }

    /// Makes this the thread's context until the guard drops (then the previous one is back).
    pub fn enter(self) -> CtxGuard {
        let prev = CTX.try_with(|c| c.replace(self)).unwrap_or_default();
        CtxGuard { prev, _thread_bound: std::marker::PhantomData }
    }
}

/// Restores the previous context when dropped.
#[must_use = "the context ends when this guard drops"]
pub struct CtxGuard {
    prev: Ctx,
    _thread_bound: std::marker::PhantomData<*const ()>,
}

impl Drop for CtxGuard {
    fn drop(&mut self) {
        let _ = CTX.try_with(|c| c.set(self.prev));
    }
}

static NEXT_CALL: AtomicU64 = AtomicU64::new(1);
static NEXT_SPAN: AtomicU64 = AtomicU64::new(1);

/// A new call id (1, 2, 3...).
pub fn new_call() -> u64 {
    NEXT_CALL.fetch_add(1, Relaxed)
}

static TICK: AtomicU64 = AtomicU64::new(0);
static FRAME: AtomicU64 = AtomicU64::new(0);

/// The engine's simulation tick and frame, stamped on every event from now on.
pub fn set_now(tick: u64, frame: u64) {
    TICK.store(tick, Relaxed);
    FRAME.store(frame, Relaxed);
}

pub fn set_tick(tick: u64) {
    TICK.store(tick, Relaxed);
}

pub fn set_frame(frame: u64) {
    FRAME.store(frame, Relaxed);
}

/// The engine's tick and frame as last set.
pub fn now() -> (u64, u64) {
    (TICK.load(Relaxed), FRAME.load(Relaxed))
}

/// This thread runs a simulation of its own (a headless `lex_simulate`, a balance sweep): its
/// events carry `tick` instead of the engine's (`None`: the engine's again).
pub fn set_thread_tick(tick: Option<u64>) {
    let _ = THREAD_TICK.try_with(|t| t.set(tick));
}

static EPOCH: OnceLock<(Instant, u64)> = OnceLock::new();

fn epoch() -> &'static (Instant, u64) {
    EPOCH.get_or_init(|| {
        let unix_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        (Instant::now(), unix_ms)
    })
}

/// The trace epoch's wall time (milliseconds since 1970): an event happened at
/// `epoch_unix_ms() + time_us / 1000`.
pub fn epoch_unix_ms() -> u64 {
    epoch().1
}

/// Microseconds since the trace epoch.
pub fn now_us() -> u64 {
    epoch().0.elapsed().as_micros() as u64
}

// ── caught panics ───────────────────────────────────────────────────────────

thread_local! {
    static CATCHING: Cell<u32> = const { Cell::new(0) };
}

/// `std::panic::catch_unwind`, marking the thread as catching while `f` runs: a panic hook asks
/// [`is_catching`] to tell a panic that will be caught (a structured error, the engine runs on)
/// from a crash, so caught ones don't push real crash reports out.
pub fn catch_unwind<R>(f: impl FnOnce() -> R + std::panic::UnwindSafe) -> std::thread::Result<R> {
    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            let _ = CATCHING.try_with(|c| c.set(c.get().saturating_sub(1)));
        }
    }
    let _ = CATCHING.try_with(|c| c.set(c.get() + 1));
    let _scope = Scope;
    std::panic::catch_unwind(f)
}

/// Whether a panic on this thread now would be caught by [`catch_unwind`].
pub fn is_catching() -> bool {
    CATCHING.try_with(|c| c.get() > 0).unwrap_or(false)
}

// ── spans ───────────────────────────────────────────────────────────────────

/// Opens a span (and a profile zone of the same name): events inside carry its id, and it closes
/// as one event with its duration when the guard drops. When `sub` doesn't record `level` it is
/// only the zone.
pub fn span(sub: &'static Subsystem, level: Level, name: &'static str) -> Span {
    let zone = crate::profile::zone(name);
    if !sub.enabled(level) {
        return Span { open: None, _zone: zone };
    }
    let id = NEXT_SPAN.fetch_add(1, Relaxed);
    let prev = Ctx::current();
    let _ = CTX.try_with(|c| c.set(Ctx { call: prev.call, span: id }));
    Span { open: Some(Box::new(OpenSpan { sub, level, name, id, prev, started: Instant::now(), fields: Vec::new() })), _zone: zone }
}

struct OpenSpan {
    sub: &'static Subsystem,
    level: Level,
    name: &'static str,
    id: u64,
    prev: Ctx,
    started: Instant,
    fields: Vec<(&'static str, Value)>,
}

/// An open span. Keep it in a named binding (`let _s = ...`).
#[must_use = "the span closes when this guard drops"]
pub struct Span {
    open: Option<Box<OpenSpan>>,
    _zone: crate::profile::Zone,
}

impl Span {
    /// Whether it records (its subsystem was enabled at its level).
    pub fn recording(&self) -> bool {
        self.open.is_some()
    }

    /// The span's id (0 when not recording).
    pub fn id(&self) -> u64 {
        self.open.as_ref().map_or(0, |o| o.id)
    }

    /// A field its close carries.
    pub fn field(&mut self, key: &'static str, value: impl Into<Value>) {
        if let Some(o) = &mut self.open {
            if o.fields.len() < MAX_FIELDS {
                o.fields.push((key, value.into()));
            }
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some(o) = self.open.take() else { return };
        let dur_us = o.started.elapsed().as_micros() as u64;
        let _ = CTX.try_with(|c| c.set(o.prev));
        record_with(o.sub.name, o.level, o.name, EventKind::Span { dur_us }, Ctx { call: o.prev.call, span: o.id }, o.prev.span, o.fields);
    }
}

// ── recording ───────────────────────────────────────────────────────────────

struct ThreadBuf {
    tid: u32,
    events: Mutex<Vec<Event>>,
}

struct Local {
    buf: Arc<ThreadBuf>,
}

impl Drop for Local {
    fn drop(&mut self) {
        // The thread is ending: what it buffered goes to the ring now.
        let mut g = lock(&self.buf.events);
        let events = std::mem::take(&mut *g);
        drop(g);
        push_ring(events);
    }
}

thread_local! {
    static LOCAL: RefCell<Option<Local>> = const { RefCell::new(None) };
}

static THREADS: Mutex<Vec<Arc<ThreadBuf>>> = Mutex::new(Vec::new());
static NEXT_TID: AtomicU32 = AtomicU32::new(1);
static SEQ: AtomicU64 = AtomicU64::new(1);

/// Records one event in the current context (the macros and [`Subsystem::emit`] call this).
pub fn record(subsystem: &'static str, level: Level, code: &'static str, kind: EventKind, parent: u64, fields: Vec<(&'static str, Value)>) {
    record_with(subsystem, level, code, kind, Ctx::current(), parent, fields);
}

fn record_with(subsystem: &'static str, level: Level, code: &'static str, kind: EventKind, ctx: Ctx, parent: u64, mut fields: Vec<(&'static str, Value)>) {
    if fields.len() > MAX_FIELDS {
        fields.truncate(MAX_FIELDS - 1);
        fields.push(("fields_dropped", Value::Bool(true)));
    }
    let tick = THREAD_TICK.try_with(Cell::get).ok().flatten().unwrap_or_else(|| TICK.load(Relaxed));
    let event = Event {
        id: 0,
        level,
        subsystem,
        code,
        kind,
        call: ctx.call,
        span: ctx.span,
        parent,
        tick,
        frame: FRAME.load(Relaxed),
        time_us: now_us(),
        thread: 0,
        fields,
        seq: SEQ.fetch_add(1, Relaxed),
    };
    let mut pending = Some(event);
    let _ = LOCAL.try_with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else { return };
        let local = slot.get_or_insert_with(register_thread);
        let tid = local.buf.tid;
        if let Some(e) = pending.as_mut() {
            e.thread = tid;
        }
        // Only a drain contends for the buffer, and only to move it (a few microseconds).
        let mut g = lock(&local.buf.events);
        let Some(e) = pending.take() else { return };
        if g.len() >= THREAD_CAPACITY {
            // Full: this thread moves its buffer to the ring itself, still holding the buffer, so
            // a drain can't put the newer event in first.
            let full = std::mem::replace(&mut *g, Vec::with_capacity(THREAD_CAPACITY));
            push_ring(full);
        }
        g.push(e);
    });
    // Not buffered (the thread is exiting and its locals are gone).
    if let Some(e) = pending {
        push_ring(vec![e]);
    }
}

fn register_thread() -> Local {
    let buf = Arc::new(ThreadBuf { tid: NEXT_TID.fetch_add(1, Relaxed), events: Mutex::new(Vec::with_capacity(THREAD_CAPACITY)) });
    lock(&THREADS).push(Arc::clone(&buf));
    Local { buf }
}

/// The central ring of events, in the order they reached it.
pub struct Ring {
    events: VecDeque<Event>,
    capacity: usize,
    next_id: u64,
    /// Events overwritten since the start (the ring was full).
    pub overwritten: u64,
    bytes: usize,
}

impl Ring {
    const fn new() -> Ring {
        Ring { events: VecDeque::new(), capacity: RING_CAPACITY, next_id: 1, overwritten: 0, bytes: 0 }
    }

    /// Adds `e`; an overwritten event goes to `evicted` (freed by the caller, outside the lock).
    fn push_evicting(&mut self, mut e: Event, evicted: &mut Vec<Event>) {
        e.id = self.next_id;
        self.next_id += 1;
        self.bytes += e.bytes();
        self.events.push_back(e);
        while self.events.len() > self.capacity {
            if let Some(old) = self.events.pop_front() {
                self.bytes -= old.bytes();
                self.overwritten += 1;
                evicted.push(old);
            }
        }
    }

    #[cfg(test)]
    fn push(&mut self, e: Event) {
        self.push_evicting(e, &mut Vec::new());
    }

    /// Oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Event> + ExactSizeIterator {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Approximate memory the kept events hold.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The id the next event will get (every id below it was handed out).
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// The oldest id still kept (`next_id` when empty).
    pub fn first_id(&self) -> u64 {
        self.events.front().map_or(self.next_id, |e| e.id)
    }

    /// Events with ids after `id`, oldest first.
    pub fn after(&self, id: u64) -> impl Iterator<Item = &Event> {
        let i = self.events.partition_point(|e| e.id <= id);
        self.events.range(i..)
    }

    pub fn get(&self, id: u64) -> Option<&Event> {
        let i = self.events.binary_search_by_key(&id, |e| e.id).ok()?;
        self.events.get(i)
    }
}

static RING: Mutex<Ring> = Mutex::new(Ring::new());

fn push_ring(mut events: Vec<Event>) {
    if events.is_empty() {
        return;
    }
    events.sort_by_key(|e| e.seq);
    let mut evicted = Vec::with_capacity(events.len());
    let mut ring = lock(&RING);
    for e in events {
        ring.push_evicting(e, &mut evicted);
    }
    drop(ring);
    drop(evicted);
}

/// How many events the ring keeps (at least 64).
pub fn set_ring_capacity(n: usize) {
    let mut ring = lock(&RING);
    ring.capacity = n.max(64);
    while ring.events.len() > ring.capacity {
        if let Some(old) = ring.events.pop_front() {
            ring.bytes -= old.bytes();
            ring.overwritten += 1;
        }
    }
}

/// Moves every thread's buffered events into the ring (threads that ended are forgotten).
pub fn flush() {
    let threads: Vec<Arc<ThreadBuf>> = {
        let mut t = lock(&THREADS);
        // Only the registry holds a thread's buffer once the thread has ended.
        t.retain(|b| Arc::strong_count(b) > 1 || !lock(&b.events).is_empty());
        t.clone()
    };
    for t in threads {
        // Moved while the buffer is held: once a flush has passed a thread, everything that
        // thread recorded before is in the ring (whichever flush moved it).
        let mut g = lock(&t.events);
        if !g.is_empty() {
            let events = std::mem::take(&mut *g);
            push_ring(events);
        }
    }
}

/// [`flush`]es, then reads the ring. `f` must not record events (the ring is locked while it
/// runs): copy out what it needs and emit after.
pub fn read<T>(f: impl FnOnce(&Ring) -> T) -> T {
    flush();
    f(&lock(&RING))
}

/// The newest `n` events without blocking (a panic hook's read: buffers or the ring held by a
/// panicking thread are skipped), oldest first.
pub fn last_nonblocking(n: usize) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    if let Ok(threads) = THREADS.try_lock() {
        for t in threads.iter() {
            if let Ok(g) = t.events.try_lock() {
                out.extend(g.iter().cloned());
            }
        }
    }
    match RING.try_lock() {
        Ok(ring) => out.extend(ring.events.iter().rev().take(n).cloned()),
        Err(TryLockError::Poisoned(p)) => out.extend(p.into_inner().events.iter().rev().take(n).cloned()),
        Err(TryLockError::WouldBlock) => {}
    }
    out.sort_by_key(|e| e.seq);
    let skip = out.len().saturating_sub(n);
    out.drain(..skip);
    out
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ── filters ─────────────────────────────────────────────────────────────────

/// Which events a query wants. Empty matches everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    /// At most this verbose (`Info`: errors, warnings and info).
    pub level: Option<Level>,
    /// Subsystem names (exact, or a prefix ending in `*`/`:`: `module:*`).
    pub subsystems: Vec<String>,
    /// Code prefixes (`engine.`, `write.refused`).
    pub codes: Vec<String>,
    /// Only these calls (an agent filter resolves to its calls). A set: a lookup per event is a
    /// log-time search, never a scan of the agent's calls.
    pub calls: Option<BTreeSet<u64>>,
    pub tick: Option<(u64, u64)>,
    pub frame: Option<(u64, u64)>,
    /// Microseconds since the trace epoch, inclusive.
    pub time_us: Option<(u64, u64)>,
    /// A substring of the code, the subsystem or a field's text.
    pub text: Option<String>,
}

impl Filter {
    pub fn matches(&self, e: &Event) -> bool {
        if self.level.is_some_and(|l| e.level > l) {
            return false;
        }
        if !self.subsystems.is_empty() && !self.subsystems.iter().any(|s| name_matches(s, e.subsystem)) {
            return false;
        }
        if !self.codes.is_empty() && !self.codes.iter().any(|c| e.code.starts_with(c.as_str())) {
            return false;
        }
        if let Some(calls) = &self.calls {
            if !calls.contains(&e.call) {
                return false;
            }
        }
        let within = |r: Option<(u64, u64)>, v: u64| r.is_none_or(|(a, b)| v >= a && v <= b);
        if !within(self.tick, e.tick) || !within(self.frame, e.frame) || !within(self.time_us, e.time_us) {
            return false;
        }
        if let Some(t) = &self.text {
            let hit = e.code.contains(t.as_str())
                || e.subsystem.contains(t.as_str())
                || e.fields.iter().any(|(k, v)| k.contains(t.as_str()) || v.as_str().is_some_and(|s| s.contains(t.as_str())));
            if !hit {
                return false;
            }
        }
        true
    }
}

/// `pattern` names `name`: exactly, or as a prefix when it ends in `*`.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

#[cfg(test)]
mod tests;

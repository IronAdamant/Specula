use super::*;

/// This test's events (tests share the ring, so each uses its own subsystem).
fn events_of(sub: &str) -> Vec<Event> {
    read(|r| r.iter().filter(|e| e.subsystem == sub).cloned().collect())
}

#[test]
fn levels_follow_the_default_and_per_subsystem_settings() {
    static SUB: Subsystem = Subsystem::new("test.levels");
    assert!(SUB.enabled(Level::Info), "info by default");
    assert!(!SUB.enabled(Level::Debug));
    set_level("test.levels", Some(Level::Trace));
    assert!(SUB.enabled(Level::Trace));
    set_level("test.levels", None);
    assert!(!SUB.enabled(Level::Error), "off records nothing");
    reset_level("test.levels");
    assert!(SUB.enabled(Level::Warn) && !SUB.enabled(Level::Debug));
    assert!(subsystems().iter().any(|(n, l, _)| *n == "test.levels" && *l == Some(Level::Info)));
    // A setting made before the subsystem's first event applies when it registers.
    set_level("test.levels.late", Some(Level::Debug));
    static LATE: Subsystem = Subsystem::new("test.levels.late");
    assert!(LATE.enabled(Level::Debug));
}

#[test]
fn events_carry_code_fields_and_context() {
    static SUB: Subsystem = Subsystem::new("test.fields");
    let call = new_call();
    {
        let _c = Ctx { call, span: 0 }.enter();
        trace_event!(&SUB, Warn, "test.stall", tool = "sight_frame", ms = 12.5, n = 3u32, ok = false);
        trace_event!(&SUB, Debug, "test.hidden", never = "evaluated");
    }
    trace_event!(&SUB, Info, "test.outside");
    let ev = events_of("test.fields");
    assert_eq!(ev.len(), 2, "{ev:?}");
    assert_eq!(ev[0].code, "test.stall");
    assert_eq!(ev[0].level, Level::Warn);
    assert_eq!(ev[0].call, call);
    assert_eq!(ev[0].field("tool").and_then(Value::as_str), Some("sight_frame"));
    assert_eq!(ev[0].field("n").and_then(Value::as_u64), Some(3));
    assert_eq!(ev[0].field("ms").and_then(Value::as_f64), Some(12.5));
    assert_eq!(ev[1].call, 0, "the context ended with its guard");
    let j = ev[0].to_json();
    assert_eq!(j.get("code").and_then(Json::as_str), Some("test.stall"));
    assert_eq!(j.get("fields").and_then(|f| f.get("ok")).and_then(Json::as_bool), Some(false));
    assert!(ev[0].line().starts_with("warn test.stall [test.fields] call="));
}

#[test]
fn long_text_and_many_fields_are_bounded() {
    static SUB: Subsystem = Subsystem::new("test.bounds");
    let long = "x".repeat(10_000);
    let fields: Vec<(&'static str, Value)> = (0..40).map(|_| ("k", Value::from(long.as_str()))).collect();
    SUB.emit(Level::Info, "test.big", fields);
    let ev = events_of("test.bounds");
    assert_eq!(ev[0].fields.len(), MAX_FIELDS);
    assert_eq!(ev[0].fields.last().map(|f| f.0), Some("fields_dropped"));
    assert!(ev[0].field("k").and_then(Value::as_str).is_some_and(|s| s.len() <= MAX_TEXT + 3));
}

#[test]
fn the_ring_keeps_the_newest_events() {
    static SUB: Subsystem = Subsystem::new("test.ring");
    let mut ring = Ring::new();
    ring.capacity = 100;
    for i in 0..250u64 {
        ring.push(Event {
            id: 0,
            level: Level::Info,
            subsystem: "test.ring",
            code: "test.n",
            kind: EventKind::Event,
            call: 0,
            span: 0,
            parent: 0,
            tick: i,
            frame: 0,
            time_us: 0,
            thread: 0,
            fields: vec![("n", Value::from(i))],
            seq: i,
        });
    }
    assert_eq!(ring.len(), 100);
    assert_eq!(ring.overwritten, 150);
    assert_eq!(ring.first_id(), 151);
    assert_eq!(ring.next_id(), 251);
    assert_eq!(ring.after(240).count(), 10);
    assert_eq!(ring.get(200).map(|e| e.tick), Some(199));
    assert!(ring.get(10).is_none(), "overwritten");
    let bytes = ring.bytes();
    assert!(bytes > 0 && bytes < 100 * 1024, "{bytes}");
    let _ = SUB.name();
}

#[test]
fn a_full_thread_buffer_moves_to_the_ring_and_an_ended_thread_keeps_its_events() {
    static SUB: Subsystem = Subsystem::new("test.buffers");
    std::thread::spawn(|| {
        for i in 0..(THREAD_CAPACITY as u64 + 10) {
            trace_event!(&SUB, Info, "test.n", i = i);
        }
        // The thread ends with ten events still buffered: they reach the ring as it exits.
    })
    .join()
    .unwrap();
    let ev = events_of("test.buffers");
    assert_eq!(ev.len(), THREAD_CAPACITY + 10);
    let order: Vec<u64> = ev.iter().filter_map(|e| e.field("i").and_then(Value::as_u64)).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "kept in order");
    assert!(ev.windows(2).all(|w| w[0].id < w[1].id));
}

#[test]
fn a_worker_entering_a_captured_context_joins_the_call() {
    static SUB: Subsystem = Subsystem::new("test.worker");
    let call = new_call();
    let _c = Ctx { call, span: 0 }.enter();
    let span = span(&SUB, Level::Info, "test.job");
    let job_ctx = Ctx::current();
    assert_eq!(job_ctx.span, span.id());
    let worker = std::thread::spawn(move || {
        let _w = job_ctx.enter();
        let _s = super::span(&SUB, Level::Info, "test.work");
        trace_event!(&SUB, Info, "test.worked");
    });
    worker.join().unwrap();
    drop(span);
    let ev = events_of("test.worker");
    assert_eq!(ev.len(), 3, "{ev:?}");
    assert!(ev.iter().all(|e| e.call == call), "everything carries the call");
    let job = ev.iter().find(|e| e.code == "test.job").unwrap();
    let work = ev.iter().find(|e| e.code == "test.work").unwrap();
    let worked = ev.iter().find(|e| e.code == "test.worked").unwrap();
    assert!(job.dur_us().is_some() && work.dur_us().is_some());
    assert_eq!(work.parent, job.span, "the worker's span hangs under the job's");
    assert_eq!(worked.span, work.span, "the worker's event is inside its span");
    assert_eq!(job.parent, 0);
}

#[test]
fn spans_off_are_only_zones_and_cost_no_ids() {
    static SUB: Subsystem = Subsystem::new("test.spans_off");
    let s = span(&SUB, Level::Debug, "test.quiet");
    assert!(!s.recording());
    assert_eq!(s.id(), 0);
    assert_eq!(Ctx::current().span, 0);
    drop(s);
    assert!(events_of("test.spans_off").is_empty());
}

#[test]
fn dynamic_subsystems_are_made_once() {
    let a = subsystem("module:test_dyn");
    let b = subsystem("module:test_dyn");
    assert!(std::ptr::eq(a, b));
    a.emit(Level::Info, "module.hi", vec![]);
    assert_eq!(events_of("module:test_dyn").len(), 1);
    assert_eq!(intern("test.code.x"), intern("test.code.x"));
}

#[test]
fn filters_select_by_every_field() {
    static SUB: Subsystem = Subsystem::new("test.filter");
    let call = new_call();
    {
        let _c = Ctx { call, span: 0 }.enter();
        trace_event!(&SUB, Error, "write.refused", rule = "world", path = "/a/b");
    }
    trace_event!(&SUB, Info, "engine.stall", tool = "sight_frame");
    let ev = events_of("test.filter");
    let n = |f: Filter| ev.iter().filter(|e| f.matches(e)).count();
    assert_eq!(n(Filter::default()), 2);
    assert_eq!(n(Filter { level: Some(Level::Warn), ..Filter::default() }), 1);
    assert_eq!(n(Filter { codes: vec!["write.".into()], ..Filter::default() }), 1);
    assert_eq!(n(Filter { calls: Some([call].into_iter().collect()), ..Filter::default() }), 1);
    assert_eq!(n(Filter { text: Some("/a/".into()), ..Filter::default() }), 1);
    assert_eq!(n(Filter { text: Some("stall".into()), ..Filter::default() }), 1);
    assert_eq!(n(Filter { subsystems: vec!["test.*".into()], ..Filter::default() }), 2);
    assert_eq!(n(Filter { subsystems: vec!["other".into()], ..Filter::default() }), 0);
    // By time: both ends count, and what is outside is left out. Said of the events' own times,
    // which may be one and the same microsecond (on CI's Linux machine they were, once in three
    // runs: "the second event's time selects one event" was the clock's doing, not the filter's).
    let (first, last) = (ev[0].time_us, ev[1].time_us);
    assert!(first <= last);
    let at = |t: u64| ev.iter().filter(|e| e.time_us == t).count();
    assert_eq!(n(Filter { time_us: Some((last, last)), ..Filter::default() }), at(last));
    assert_eq!(n(Filter { time_us: Some((first, first)), ..Filter::default() }), at(first));
    assert_eq!(n(Filter { time_us: Some((first, last)), ..Filter::default() }), 2);
    assert_eq!(n(Filter { time_us: Some((last + 1, u64::MAX)), ..Filter::default() }), 0);
    if first > 0 {
        assert_eq!(n(Filter { time_us: Some((0, first - 1)), ..Filter::default() }), 0);
    }
    // And of times that are given: an event one microsecond off either end is out.
    let mut moved = ev[1].clone();
    moved.time_us = 5_000;
    for (range, inside) in [((5_000, 5_000), true), ((4_999, 5_001), true), ((5_001, 9_000), false), ((0, 4_999), false)] {
        assert_eq!(Filter { time_us: Some(range), ..Filter::default() }.matches(&moved), inside, "{range:?}");
    }
}

#[test]
fn a_thread_tick_overrides_the_engines() {
    static SUB: Subsystem = Subsystem::new("test.tick");
    std::thread::spawn(|| {
        set_thread_tick(Some(77));
        trace_event!(&SUB, Info, "test.sim");
    })
    .join()
    .unwrap();
    assert_eq!(events_of("test.tick")[0].tick, 77);
}

#[test]
fn the_nonblocking_read_sees_buffered_events() {
    static SUB: Subsystem = Subsystem::new("test.nonblocking");
    trace_event!(&SUB, Error, "test.last");
    let last = last_nonblocking(RING_CAPACITY);
    assert!(last.iter().any(|e| e.subsystem == "test.nonblocking"));
}

#[test]
fn caught_panics_are_marked_while_they_unwind() {
    assert!(!is_catching());
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let inner = std::sync::Arc::clone(&seen);
    let r = catch_unwind(move || {
        inner.store(is_catching(), std::sync::atomic::Ordering::Relaxed);
        7
    });
    assert_eq!(r.ok(), Some(7));
    assert!(seen.load(std::sync::atomic::Ordering::Relaxed), "marked inside");
    assert!(!is_catching(), "unmarked after");
}

#[test]
fn settings_parse() {
    assert_eq!(parse_setting("off"), Some(None));
    assert_eq!(parse_setting("debug"), Some(Some(Level::Debug)));
    assert_eq!(parse_setting("loud"), None);
    assert_eq!(setting_str(None), "off");
}

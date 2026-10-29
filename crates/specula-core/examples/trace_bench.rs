//! Cost of tracing (docs/design/TRACE.md): a disabled event, a disabled span, an enabled event
//! and an enabled span, per call, on one thread and on four at once.
//!
//!     cargo run --release -p specula-core --example trace_bench

use specula_core::trace::{self, Level, Subsystem};
use specula_core::trace_event;
use std::hint::black_box;
use std::time::Instant;

static SUB: Subsystem = Subsystem::new("bench");

fn per_call(n: u64, f: impl Fn(u64)) -> f64 {
    let t = Instant::now();
    for i in 0..n {
        f(black_box(i));
    }
    t.elapsed().as_secs_f64() * 1e9 / n as f64
}

fn main() {
    trace::set_level("bench", Some(Level::Info));
    let n = 2_000_000;
    let off_event = per_call(n, |i| trace_event!(&SUB, Debug, "bench.off", i = i, s = "text"));
    let off_span = per_call(n, |_| {
        let _s = trace::span(&SUB, Level::Debug, "bench.off_span");
    });
    let n_on = 200_000;
    let on_event = per_call(n_on, |i| trace_event!(&SUB, Info, "bench.on", i = i, ms = 1.5, s = "text"));
    let on_span = per_call(n_on, |_| {
        let _s = trace::span(&SUB, Level::Info, "bench.on_span");
    });
    let flush = Instant::now();
    trace::flush();
    let flush_ms = flush.elapsed().as_secs_f64() * 1e3;
    let threads: Vec<_> = (0..4)
        .map(|_| std::thread::spawn(move || per_call(n_on, |i| trace_event!(&SUB, Info, "bench.on4", i = i))))
        .collect();
    let on4: Vec<f64> = threads.into_iter().map(|t| t.join().unwrap_or(f64::NAN)).collect();
    // What an enabled event is made of.
    let clock = per_call(n_on, |_| {
        black_box(Instant::now());
    });
    let fields = per_call(n_on, |i| {
        black_box(vec![("i", trace::Value::from(i)), ("ms", trace::Value::from(1.5)), ("s", trace::Value::from("text"))]);
    });
    println!("(parts: reading the clock {clock:.1} ns; building 3 fields {fields:.1} ns)");
    println!("disabled event: {off_event:.2} ns");
    println!("disabled span:  {off_span:.2} ns");
    println!("enabled event:  {on_event:.1} ns (3 fields)");
    println!("enabled span:   {on_span:.1} ns");
    println!("enabled event, 4 threads at once: {:.1} ns each", on4.iter().sum::<f64>() / on4.len() as f64);
    println!("flush after the single-thread runs: {flush_ms:.2} ms");
    println!("ring: {} events, about {} KB", trace::read(|r| r.len()), trace::read(|r| r.bytes()) / 1024);
}

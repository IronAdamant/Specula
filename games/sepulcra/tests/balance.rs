//! The balance lab on Sepulcra's Lex slice (docs/design/BALANCE.md): targets as data, sweeps
//! whose numbers don't depend on the thread count, and a tuned param that hits its target on
//! held-out seeds the tuner never saw.

#![allow(clippy::disallowed_methods, reason = "a test and benchmark harness: wall-clock timings and parallel runs, each run deterministic on its own")]

use specula_balance::sweep::{sweep, Point, SweepOptions};
use specula_balance::{Targets, TuneJob, TuneOptions, TuneParam};
use specula_core::Json;
use specula_lex::Document;

const SLICE: &str = include_str!("../lex/slice.lex.json");
const TARGETS: &str = include_str!("../lex/slice.targets.json");

fn doc() -> Document {
    Document::parse(SLICE).expect("the slice parses")
}

fn targets() -> Targets {
    Targets::parse_text(TARGETS).unwrap_or_else(|d| panic!("{:#?}", d))
}

fn opts(threads: usize) -> SweepOptions {
    SweepOptions { threads, deadline: None }
}

#[test]
fn the_slice_targets_validate_and_bind() {
    let t = targets();
    assert_eq!((t.train.len(), t.holdout.len()), (24, 24));
    assert!(t.train.iter().all(|s| !t.holdout.contains(s)), "held-out seeds are unseen");
    let rules = specula_lex::compile(&doc(), &Default::default()).expect("compiles").rules;
    let bound = t.bind(&rules).unwrap_or_else(|d| panic!("{d:#?}"));
    assert_eq!(bound.policies[0].steps.len(), 6);
    // A typo is caught with a suggestion, against the rules.
    let bad = TARGETS.replace("\"archetype\": \"runner\"", "\"archetype\": \"runer\"");
    let e = Targets::parse_text(&bad).unwrap().bind(&rules).unwrap_err();
    assert_eq!(e[0].code, "unknown_ref");
    assert!(e[0].suggestion.as_deref().unwrap_or("").contains("runner"), "{e:?}");
}

/// Sweeps give the same numbers on 1, 3 or 8 threads (each run is independent; results are
/// gathered by index).
#[test]
fn sweeps_do_not_depend_on_the_thread_count() {
    let (d, t) = (doc(), targets());
    let points: Vec<Point> = [30, 45, 60].iter().map(|hp| vec![("runner_hp".to_owned(), Json::from(*hp))]).collect();
    let seeds = &t.train[..8];
    let one = sweep(&d, &t, &points, seeds, &opts(1));
    for threads in [3, 8] {
        let many = sweep(&d, &t, &points, seeds, &opts(threads));
        assert_eq!(many.points, one.points, "{threads} threads");
    }
    assert!(one.unfinished.is_empty());
    // Different params are different rules (their hash says so).
    let hashes: Vec<_> = one.points.iter().flatten().map(|p| p.rules_hash).collect();
    assert!(hashes[0] != hashes[1] && hashes[1] != hashes[2]);
}

/// The proof: tune `runner_hp` toward the targets on the training seeds, then the best value
/// hits every target on 24 held-out seeds the tuner never saw.
#[test]
fn tuning_runner_hp_hits_the_targets_on_held_out_seeds() {
    let (d, t) = (doc(), targets());
    // A step wider than the range is refused, not overflowed.
    let wide = TuneJob::new(&d, &[TuneParam::new("runner_hp").range(40, 160).step(200)], TuneOptions { budget: 4, init: None, seed: 1 });
    assert!(wide.is_err_and(|ds| ds.iter().any(|x| x.path == "step")));
    let tune = [TuneParam::new("runner_hp").range(40, 160).step(1)];
    let mut job = TuneJob::new(&d, &tune, TuneOptions { budget: 14, init: None, seed: 7 }).unwrap();
    let base = sweep(&d, &t, &[job.point(&[job.space[0].base])], &t.holdout, &opts(8)).points.remove(0).unwrap();
    assert!(!base.hit, "the rules as written miss the targets (else there is nothing to tune)");
    let (n, cut) = job.advance(&d, &t, usize::MAX, &opts(8));
    assert_eq!((n, cut), (14, false));
    let best = job.best().unwrap().clone();
    for e in &job.evals {
        println!("  eval {} loss {:.4} hit {}", Json::Obj(job.point(&e.x)), e.loss, e.hit);
    }
    println!("best on train: {} loss {:.4}", Json::Obj(job.point(&best.x)), best.loss);
    assert!(best.hit, "the best point hits every target on the training seeds: {best:?}");
    let held = job.validate(&d, &t, &opts(8)).expect("validated");
    println!("held out: {}", held.to_json(false));
    assert!(held.hit, "on held-out seeds too: {}", held.to_json(true));
    assert!(held.loss < base.loss / 10.0, "and far closer than the rules as written ({} vs {})", held.loss, base.loss);

    // Resumable and deterministic: 6 evaluations, saved as data, read back and continued, give
    // exactly the same sequence as one uninterrupted job.
    let mut a = TuneJob::new(&d, &tune, TuneOptions { budget: 14, init: None, seed: 7 }).unwrap();
    a.advance(&d, &t, 6, &opts(3));
    let mut b = TuneJob::from_json(&a.to_json()).unwrap();
    b.advance(&d, &t, usize::MAX, &opts(5));
    let xs = |j: &TuneJob| j.evals.iter().map(|e| (e.x.clone(), e.loss.to_bits())).collect::<Vec<_>>();
    assert_eq!(xs(&b), xs(&job));
}

/// Throughput, release build: `cargo test -p sepulcra --release --test balance -- --ignored --nocapture throughput`.
#[test]
#[ignore]
fn throughput() {
    let (d, t) = (doc(), targets());
    let points: Vec<Point> = (0..8).map(|i| vec![("runner_hp".to_owned(), Json::from(30 + 5 * i))]).collect();
    for threads in [1, 2, 4, 8, 16] {
        let started = std::time::Instant::now();
        let res = sweep(&d, &t, &points, &t.train, &opts(threads));
        let runs: usize = res.points.iter().flatten().map(|p| p.runs).sum();
        let s = started.elapsed().as_secs_f64();
        println!("{threads:>2} threads: {runs} runs in {s:.3} s = {:.0} runs/s", runs as f64 / s);
    }
}

/// A look at the slice under the targets' policy across a few param values.
#[test]
#[ignore]
fn explore() {
    let d = doc();
    let t = match std::env::var("TARGETS") {
        Ok(path) => Targets::parse_text(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{e:#?}")),
        Err(_) => targets(),
    };
    let param = std::env::var("PARAM").unwrap_or_else(|_| "runner_hp".into());
    let values: Vec<f64> = std::env::var("VALUES").map_or(vec![20.0, 26.0, 32.0, 40.0, 50.0], |v| v.split(',').map(|x| x.parse().unwrap()).collect());
    let points: Vec<Point> = values.iter().map(|hp| vec![(param.clone(), Json::from(*hp))]).collect();
    let started = std::time::Instant::now();
    let res = sweep(&d, &t, &points, &t.train, &opts(8));
    let runs: usize = res.points.iter().flatten().map(|p| p.runs).sum();
    let s = started.elapsed().as_secs_f64();
    println!("{runs} runs in {s:.2} s: {:.0} runs/s", runs as f64 / s);
    for p in res.points.iter().flatten() {
        println!("{}", p.to_json(false));
    }
}

const STYLES: &str = include_str!("../lex/slice_styles.targets.json");

/// Several play styles at once (`policies`): every point is played by each style on the same
/// seeds, each style's numbers are reported, and a target hits only when it hits under every
/// style. The value tuned for one build order (`runner_hp` 118, above) is a close call for that
/// build but a loss for the brazier-first build: one policy hides that, three show it.
#[test]
fn targets_must_hold_across_play_styles() {
    let d = doc();
    let t = Targets::parse_text(STYLES).unwrap_or_else(|e| panic!("{e:#?}"));
    let names: Vec<&str> = t.policies.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["dust_gated", "braziers", "rusher"]);
    let points: Vec<Point> = [26, 118].iter().map(|hp| vec![("runner_hp".to_owned(), Json::from(*hp))]).collect();
    let one = sweep(&d, &t, &points, &t.train, &opts(1));
    let many = sweep(&d, &t, &points, &t.train, &opts(7));
    assert_eq!(one.points, many.points, "the thread count changes nothing");
    let tuned = one.points[1].as_ref().unwrap();
    assert_eq!(tuned.runs, 3 * t.train.len(), "each style on each seed");
    let style = |n: &str| tuned.policies.iter().find(|p| p.name == n).unwrap();
    println!("runner_hp 118: {}", tuned.to_json(false));
    assert!(style("dust_gated").hit, "the build it was tuned for still gets its close call");
    assert!(!style("braziers").hit && style("braziers").outcomes.iter().any(|(o, _)| o == "lost"), "the brazier build loses: {:?}", style("braziers"));
    assert!(!tuned.hit, "so the point misses: the targets don't hold across styles");
    let weighted: f64 = tuned.policies.iter().map(|p| p.weight * p.loss).sum();
    assert!((tuned.loss - weighted).abs() < 1e-9, "the loss is the styles' weighted sum");
    let wins = &tuned.stats[0];
    assert!(!wins.hit && wins.mean.is_some_and(|m| m < 1.0), "the pooled win rate shows the losses: {wins:?}");
    // The JSON lists each style.
    let j = tuned.to_json(false);
    assert_eq!(j.get("policies").and_then(Json::as_array).map(|a| a.len()), Some(3));
}

/// Tuning against all three styles at once: `cargo test -p sepulcra --release --test balance --
/// --ignored --nocapture tune_styles`.
#[test]
#[ignore]
fn tune_styles() {
    let d = doc();
    let t = Targets::parse_text(STYLES).unwrap();
    let tune = [TuneParam::new("runner_hp").range(20, 160).step(1), TuneParam::new("shambler_hp").range(20, 160).step(1)];
    let mut job = TuneJob::new(&d, &tune, TuneOptions { budget: 30, init: None, seed: 7 }).unwrap();
    job.advance(&d, &t, usize::MAX, &opts(8));
    for e in &job.evals {
        println!("  eval {} loss {:.4} hit {}", Json::Obj(job.point(&e.x)), e.loss, e.hit);
    }
    let held = job.validate(&d, &t, &opts(8)).unwrap();
    println!("held out: {}", held.to_json(false));
}

/// What a run did is data: its trace (each applied policy step with its tick) replayed as plain
/// timed commands, with no policy, gives the same run. This is what turns lab runs into
/// playtests in a game engine (scripts/balance_playtests.py).
#[test]
fn a_runs_trace_replays_it_without_the_policy() {
    use specula_balance::sweep::sweep_keeping;
    use specula_lex::{compile, Natives, Options, Sim};
    let d = doc();
    let t = Targets::parse_text(STYLES).unwrap();
    let point: Point = vec![("runner_hp".to_owned(), Json::from(61)), ("shambler_hp".to_owned(), Json::from(139))];
    let seeds = [10_001, 10_002, 10_003];
    let (res, runs) = sweep_keeping(&d, &t, std::slice::from_ref(&point), &seeds, &opts(4), true);
    assert!(res.unfinished.is_empty());
    let runs = &runs[0];
    assert_eq!(runs.len(), 3 * seeds.len());
    let rules = std::sync::Arc::new(compile(&d, &Options { overrides: point.clone(), ..Default::default() }).unwrap().rules);
    for r in runs {
        let policy = &t.policies[r.policy];
        let mut sim = Sim::new(std::sync::Arc::clone(&rules), r.seed, &Natives::new()).unwrap();
        let mut next = 0;
        while sim.tick() < r.ticks && sim.outcome().is_none() {
            while let Some(&(tick, step)) = r.trace.get(next) {
                if tick != sim.tick() {
                    break;
                }
                let s = &policy.steps[usize::from(step)];
                let call = sim.parse_command(&s.command, &s.args).unwrap();
                sim.apply(&call).unwrap_or_else(|e| panic!("{} seed {}: step {step} at {tick} refused: {e:?}", policy.name, r.seed));
                next += 1;
            }
            sim.step(&[]);
        }
        assert_eq!(next, r.trace.len(), "every traced step was applied");
        assert_eq!(sim.outcome().map(|(o, k)| (o.to_owned(), k)), r.outcome, "{} seed {}", policy.name, r.seed);
        assert_eq!(sim.tick(), r.ticks);
        let dust = r.resources.iter().find(|(id, _)| id == "dust").unwrap().1;
        assert_eq!(sim.resource("dust").unwrap() as f64, dust, "{} seed {}", policy.name, r.seed);
    }
}

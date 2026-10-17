//! The balance lab on Sepulcra's Lex slice (docs/design/BALANCE.md): targets as data, sweeps
//! whose numbers don't depend on the thread count, and a tuned param that hits its target on
//! held-out seeds the tuner never saw.

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
    assert_eq!(bound.steps.len(), 6);
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

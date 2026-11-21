//! No test passes or fails by how fast the machine was.
//!
//! An assertion that something took "under 50 ms" is true on a quiet machine and a matter of
//! luck in a full test run (sixteen test threads, engines sharing one GPU): four such tests
//! each failed about one gate run in ten on 2026-11-12, and a single run of the suite showed
//! none of them. The owner's rule (AGENTS.md, "The gate"): a gate must not pass by luck.
//!
//! So a test says what the code **did**: a counter that didn't move ("no layout was made"), an
//! order ("asked second, back first"), an output that doesn't depend on the tail of its input.
//! Where a speed is the point (a budget: "picking over 512 galleries takes under 20 ms"), the
//! bound is written `speed_budget(tight)` (`specula_core::speed_budget`): the tight bound when
//! the test runs alone in the gate's `speed` step, and a liveness bound ([`MIN_SECONDS`]: far
//! above anything a busy run adds) in every other run. A bare bound must be at least
//! [`MIN_SECONDS`] itself.
//!
//! This test reads every `.rs` file under `crates/` and `games/` and fails on an assertion
//! that puts a time under a shorter bare bound. It reads these forms, in an `assert` or a
//! `debug_assert`, on one line or several:
//! - anything `<` or `<=` a `Duration::from_...(n)` (and `Duration::from_...(n) >` anything);
//! - `elapsed()`, `.as_millis()`, `.as_micros()`, `.as_nanos()`, `.as_secs...()`, or a value
//!   named `ms`, `..._ms`, `..._us` or `..._micros`, `<` or `<=` a number or a name.
//!
//! A bound given by a name is listed in [`NAMED`] with its length; a value that isn't a
//! measured speed in [`NOT_A_SPEED`]; a test the gate never runs in [`QUIET_MACHINE_ONLY`].
//! **What it can't see:** a time copied into a plain number (`let n = t.as_millis(); assert!(n
//! < 50)`), and an assertion inside a helper that takes the bound as an argument. Reviews look
//! for those.
//!
//! **Nor the other way round:** a test that needs the machine to be slow ("still running after
//! a pump of 1 ms": the cause of CI's red on 2026-11-21, where a faster machine had finished),
//! and a test that sleeps a few milliseconds and expects a file's time to have moved (a file
//! system may keep times by the second). This guard reads upper bounds only; a second
//! reader's list of 2026-11-22 names `thread::sleep` between writes in `project.rs`'s tests
//! as places to look. Not yet a check.

use std::path::{Path, PathBuf};

/// The shortest bare upper bound a test may put on a time, in seconds.
const MIN_SECONDS: f64 = 5.0;

/// Upper bounds given by a name: (file, the bound as written, its value in seconds, what it
/// bounds).
const NAMED: &[(&str, &str, f64, &str)] = &[
    ("crates/specula-model/tests/hostile_recipes.rs", "TIME", 20.0, "a hostile recipe is refused or built within its budget, not in minutes"),
    ("crates/specula/tests/streaming_review.rs", "STUCK", 60.0, "streaming made no progress at all for a minute"),
];

/// `specula_core::speed_budget`, and each copy of it, with `pub` and whitespace dropped.
const SPEED_BUDGET: &str = "fn speed_budget(tight: std::time::Duration) -> std::time::Duration { if std::env::var_os(\"SPECULA_SPEED_TESTS\").is_some() { tight } else { tight.max(std::time::Duration::from_secs(5)) } }";

/// Values that read as times and may be bounded tightly, because they aren't how fast something
/// ran: (file, what the left side contains, what it is).
const NOT_A_SPEED: &[(&str, &str, &str)] = &[("crates/specula-audio/tests/backends.rs", "latency_ms", "the device's reported buffer length (frames over rate), a setting")];

/// Speed tests the gate never runs: (file, the bound as written, why it may stay). Each is
/// ignored in the builds the gate tests, and is for a quiet machine, run alone. Don't add to
/// this list to get a speed assertion past the guard: write `speed_budget(..)`.
const QUIET_MACHINE_ONLY: &[(&str, &str, &str)] =
    &[("crates/specula/tests/claims_steps.rs", "budget_ms", "ignored in debug builds (`cfg_attr(debug_assertions, ignore)`): a release run, alone, checks a 1000-op batch's cost")];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" && name != ".git" {
                rust_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// `code` with the insides of its string literals blanked (a message may say "took < 5 ms").
fn without_strings(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let (mut inside, mut escaped) = (false, false);
    for c in code.chars() {
        if inside {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                inside = false;
                out.push('"');
                continue;
            }
            out.push(' ');
        } else {
            inside = c == '"';
            out.push(c);
        }
    }
    out
}

/// The operand that starts at the head of `text`, up to the `,`, `)`, `;`, `&&` or `||` that
/// ends it (parentheses inside it balanced).
fn operand_after(text: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth == 0 => return text[..i].trim(),
            ')' | ']' => depth -= 1,
            ',' | ';' | '&' | '|' if depth == 0 => return text[..i].trim(),
            _ => {}
        }
    }
    text.trim()
}

/// The operand that ends at the tail of `text`, back to the `(`, `,`, `&&` or `||` that starts
/// it.
fn operand_before(text: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in text.char_indices().rev() {
        match c {
            ')' | ']' => depth += 1,
            '(' | '[' if depth == 0 => return text[i + 1..].trim(),
            '(' | '[' => depth -= 1,
            ',' | '&' | '|' | '!' if depth == 0 => return text[i + 1..].trim(),
            _ => {}
        }
    }
    text.trim()
}

fn number(text: &str) -> Option<f64> {
    let t = text.trim().replace('_', "");
    let t = ["u128", "u64", "u32", "usize", "f64", "f32"].iter().fold(t.as_str(), |t, s| t.strip_suffix(s).unwrap_or(t));
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    t.trim_end_matches('.').parse().ok()
}

/// A `Duration::from_...(n)`'s length in seconds: `None` when `text` isn't one, `Some(Err)`
/// when its argument isn't a number.
fn duration_literal(text: &str) -> Option<Result<f64, String>> {
    let t = text.trim();
    let t = t.strip_prefix("std::time::").or_else(|| t.strip_prefix("core::time::")).unwrap_or(t);
    let t = t.strip_prefix("Duration::from_")?;
    let (unit, rest) = t.split_once('(')?;
    let scale = match unit {
        "secs" | "secs_f64" | "secs_f32" => 1.0,
        "millis" => 1e-3,
        "micros" => 1e-6,
        "nanos" => 1e-9,
        _ => return None,
    };
    let inner = rest.strip_suffix(')')?;
    Some(number(inner).map(|n| n * scale).ok_or_else(|| text.trim().to_owned()))
}

/// Seconds per unit of `text`, when it reads as a time: a `Duration` (1.0), milliseconds...
fn time_unit(text: &str) -> Option<f64> {
    let mut t = text.trim();
    // `x.as_millis() as f64`
    if let Some((head, cast)) = t.rsplit_once(" as ") {
        if cast.chars().all(|c| c.is_ascii_alphanumeric()) {
            t = head.trim();
        }
    }
    for (tail, unit) in [(".as_millis()", 1e-3), (".as_micros()", 1e-6), (".as_nanos()", 1e-9), (".as_secs_f64()", 1.0), (".as_secs_f32()", 1.0), (".as_secs()", 1.0), ("elapsed()", 1.0)] {
        if t.ends_with(tail) {
            return Some(unit);
        }
    }
    let start = t.rfind(|c: char| !c.is_alphanumeric() && c != '_').map_or(0, |i| i + 1);
    let name = &t[start..];
    if name == "ms" || name.ends_with("_ms") {
        Some(1e-3)
    } else if name.ends_with("_us") || name.ends_with("_micros") {
        Some(1e-6)
    } else {
        None
    }
}

/// An upper bound on a time found in an assertion.
#[derive(Debug, PartialEq)]
struct Bound {
    /// What is bounded, as written.
    what: String,
    /// The bound, as written.
    written: String,
    /// Its length in seconds; `Err`: a name or an expression this guard can't read.
    seconds: Result<f64, String>,
    /// Written with `speed_budget(..)`.
    budget: bool,
}

/// Whether `bound` is a `speed_budget(..)` call and nothing else (`speed_budget(x) / 100` is a
/// bare bound again).
fn is_budget(bound: &str) -> bool {
    let b = bound.trim();
    let b = b.strip_prefix("specula_core::").or_else(|| b.strip_prefix("crate::")).unwrap_or(b);
    let Some(inside) = b.strip_prefix("speed_budget(") else { return false };
    let mut depth = 1i32;
    for (i, c) in inside.char_indices() {
        depth += i32::from(c == '(') - i32::from(c == ')');
        if depth == 0 {
            return i + 1 == inside.len();
        }
    }
    false
}

/// The upper bounds on times in one assertion (its whole statement, strings blanked).
fn upper_bounds(statement: &str) -> Vec<Bound> {
    let s = statement;
    let bytes = s.as_bytes();
    let mut found = Vec::new();
    for i in 1..bytes.len().saturating_sub(1) {
        let (op, prev) = (bytes[i], bytes[i - 1]);
        if (op != b'<' && op != b'>') || prev != b' ' {
            continue;
        }
        let after = if bytes[i + 1] == b'=' { i + 2 } else { i + 1 };
        if bytes.get(after) != Some(&b' ') {
            continue; // `<<`, `->`, `Vec<u8>`
        }
        let (left, right) = (operand_before(&s[..i - 1]), operand_after(&s[after..]));
        // `time < bound`, or `bound > time` the other way round.
        let (time, bound) = if op == b'<' { (left, right) } else { (right, left) };
        let budget = is_budget(bound);
        let seconds = if budget {
            Ok(f64::INFINITY)
        } else if bound.contains("speed_budget(") {
            // A budget inside an expression: the guard can't say how long that is.
            Err(bound.to_owned())
        } else if let Some(d) = duration_literal(bound) {
            d
        } else if let Some(unit) = time_unit(time) {
            match number(bound) {
                Some(n) => Ok(n * unit),
                // `a > b` between two times or names says nothing about which is the bound.
                None if op == b'>' => continue,
                None => Err(bound.to_owned()),
            }
        } else {
            continue;
        };
        found.push(Bound { what: time.to_owned(), written: bound.to_owned(), seconds, budget });
    }
    found
}

/// The assertions in a file: (the line each starts on, its statement with comments dropped and
/// strings blanked).
fn assertions(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<String> = text.lines().map(|l| without_strings(l).split("//").next().unwrap_or("").to_owned()).collect();
    let mut out = Vec::new();
    let mut n = 0;
    while n < lines.len() {
        let Some(at) = lines[n].find("assert") else {
            n += 1;
            continue;
        };
        let first = n;
        let mut statement = lines[n][at..].to_owned();
        let depth = |s: &str| s.chars().fold(0i32, |d, c| d + i32::from(c == '(') - i32::from(c == ')'));
        while depth(&statement) > 0 && n + 1 < lines.len() && n - first < 24 {
            n += 1;
            statement.push(' ');
            statement.push_str(lines[n].trim());
        }
        out.push((first + 1, statement));
        n += 1;
    }
    out
}

#[test]
fn the_scanner_reads_the_bounds_tests_write() {
    let read = |line: &str| upper_bounds(&without_strings(line)).into_iter().map(|b| b.seconds).collect::<Vec<_>>();
    // The direct forms.
    assert_eq!(read("assert!(t.elapsed() < Duration::from_millis(50));"), [Ok(0.05)]);
    assert_eq!(read("assert!(t.elapsed() < std::time::Duration::from_secs(5), \"{:?}\", t.elapsed());"), [Ok(5.0)]);
    assert_eq!(read("assert!(started.elapsed().as_millis() < 500, \"x\");"), [Ok(0.5)]);
    assert_eq!(read("assert!(t.elapsed().as_secs_f64() < 0.1);"), [Ok(0.1)]);
    assert_eq!(read("assert!(done && t.elapsed() < Duration::from_secs(5), \"x\");"), [Ok(5.0)]);
    assert_eq!(read("assert!(t.elapsed() < TIME, \"took\");"), [Err("TIME".to_owned())]);
    assert_eq!(read("assert!(a.elapsed() >= WAIT && a.elapsed() < WAIT * 4);"), [Err("WAIT * 4".to_owned())]);
    // A time kept in a variable (review R2 of m5-gpuwork: a dozen of these escaped the first
    // guard, which read only lines that held `elapsed()`).
    assert_eq!(read("assert!(spent < Duration::from_millis(20), \"picking took {spent:?}\");"), [Ok(0.02)]);
    assert_eq!(read("assert!(took.as_millis() < 20, \"{side} m: {took:?}\");"), [Ok(0.02)]);
    assert_eq!(read("assert!(warm.as_secs_f64() < 0.0002, \"x\");"), [Ok(0.0002)]);
    assert_eq!(read("assert!(checks[10] < Duration::from_micros(500), \"median {:?}\", checks[10]);"), [Ok(0.0005)]);
    assert_eq!(read("assert!(ms < 2000.0, \"adapter {}\", a.index);"), [Ok(2.0)]);
    assert_eq!(read("assert!(t.passes[0].gpu_ms > 0.0 && t.passes[0].gpu_ms < 60_000.0, \"x\");"), [Ok(60.0)]);
    assert_eq!(read("assert!(ms < budget_ms, \"x\");"), [Err("budget_ms".to_owned())]);
    assert!(matches!(read("assert!(took.as_micros() as f64 <= 800.0);")[..], [Ok(s)] if (s - 0.0008).abs() < 1e-12));
    // The other way round.
    assert_eq!(read("assert!(Duration::from_millis(20) > spent);"), [Ok(0.02)]);
    assert_eq!(read("assert!(20 >= took.as_millis());"), [Ok(0.02)]);
    // A budget, and what isn't an upper bound on a time.
    assert_eq!(read("assert!(spent < speed_budget(Duration::from_millis(20)), \"x\");"), [Ok(f64::INFINITY)]);
    assert!(upper_bounds("assert!(spent < specula_core::speed_budget(Duration::from_millis(20)));")[0].budget);
    // Review R3: a budget is the whole bound.
    assert_eq!(read("assert!(spent < speed_budget(Duration::from_millis(20)) / 100);"), [Err("speed_budget(Duration::from_millis(20)) / 100".to_owned())]);
    assert!(is_budget("crate::speed_budget(std::time::Duration::from_millis(1500))") && !is_budget("speed_budget(a).min(b)") && !is_budget("my_speed_budget(a)"));
    assert!(read("assert!(a.elapsed() >= WAIT);").is_empty(), "a lower bound");
    assert!(read("assert!(gpu_ms > 0.0 && took.as_millis() > 3);").is_empty(), "lower bounds");
    assert!(read("assert!(items < 3 && rooms <= 4);").is_empty(), "words that only end in ms");
    assert!(read("assert!(v.len() < 10, \"took < 5 ms\");").is_empty(), "a message is not code");
    assert!(read("let v: Vec<u8> = x; assert!(a << 2 == b);").is_empty());
}

#[test]
fn the_scanner_reads_assertions_over_several_lines() {
    let text = "fn t() {\n    assert!(\n        spent < Duration::from_millis(20), // a note\n        \"picking took {spent:?}\"\n    );\n    assert_eq!(a, b);\n}\n";
    let found = assertions(text);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].0, 2);
    assert_eq!(upper_bounds(&found[0].1).into_iter().map(|b| b.seconds).collect::<Vec<_>>(), [Ok(0.02)]);
    assert!(upper_bounds(&found[1].1).is_empty());
}

/// The crates that don't depend on `specula-core` carry a copy of `speed_budget`; a copy that
/// drifted (another variable, another floor) would quietly stop being a budget.
#[test]
fn every_copy_of_speed_budget_is_the_same() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("games"), &mut files);
    let mut copies = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        if rel.ends_with("tests/clock_guard.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            if !line.contains("fn speed_budget(") {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            let end = (n..lines.len()).find(|&k| lines[k].trim() == "}" && lines[k].len() - 1 == indent).unwrap_or(n);
            let body = lines[n..=end].iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
            let body = body.trim_start_matches("pub(crate) ").trim_start_matches("pub ");
            assert_eq!(body, SPEED_BUDGET, "{rel}:{}: this copy of speed_budget differs from specula_core's", n + 1);
            copies.push(rel.clone());
        }
    }
    assert!(copies.iter().any(|c| c == "crates/specula-core/src/lib.rs"), "specula_core::speed_budget is gone: {copies:?}");
}

#[test]
fn no_test_asserts_a_speed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("games"), &mut files);
    assert!(files.len() > 200, "found {} source files: has the layout changed?", files.len());
    let mut bad = Vec::new();
    let (mut seen, mut budgets) = (0, 0);
    let mut named_used = vec![false; NAMED.len()];
    let mut not_a_speed_used = vec![false; NOT_A_SPEED.len()];
    let mut quiet_used = vec![false; QUIET_MACHINE_ONLY.len()];
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        if rel.ends_with("tests/clock_guard.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for (line, statement) in assertions(&text) {
            for b in upper_bounds(&statement) {
                seen += 1;
                if b.budget {
                    budgets += 1;
                    continue;
                }
                if let Some(i) = NOT_A_SPEED.iter().position(|(f, w, _)| rel.ends_with(f) && b.what.contains(w)) {
                    not_a_speed_used[i] = true;
                    continue;
                }
                if let Some(i) = QUIET_MACHINE_ONLY.iter().position(|(f, w, _)| rel.ends_with(f) && *w == b.written) {
                    // It must still be one the gate doesn't run.
                    assert!(text.contains("cfg_attr(debug_assertions, ignore"), "{rel}: listed as a quiet-machine test, but nothing in it is ignored in debug builds");
                    quiet_used[i] = true;
                    continue;
                }
                match &b.seconds {
                    Ok(s) if *s >= MIN_SECONDS => {}
                    Ok(s) => bad.push(format!("{rel}:{line}: {} < {} ({s} s)", b.what, b.written)),
                    Err(name) => match NAMED.iter().position(|(f, nm, _, _)| rel.ends_with(f) && nm == name) {
                        Some(i) if NAMED[i].2 >= MIN_SECONDS => named_used[i] = true,
                        _ => bad.push(format!("{rel}:{line}: {} < {name} (a bound this guard can't read: write the number, or list it in NAMED with its length)", b.what)),
                    },
                }
            }
        }
    }
    assert!(seen >= 10, "the guard found {seen} bounds on times: has their form changed?");
    assert!(
        bad.is_empty(),
        "{} assertions put a time under a bare bound shorter than {MIN_SECONDS} s, which a busy test run breaks now and then\n\
         (a gate must not pass by luck). Say what the code did instead (a counter that didn't move, an order, an output);\n\
         if the speed is the point, write the bound as `speed_budget(Duration::from_millis(..))`: tight when the test runs\n\
         alone in the gate's speed step, {MIN_SECONDS} s everywhere else:\n{}",
        bad.len(),
        bad.join("\n")
    );
    for (i, used) in named_used.iter().enumerate() {
        assert!(used, "NAMED lists {} in {}, which no assertion uses any more", NAMED[i].1, NAMED[i].0);
    }
    for (i, used) in quiet_used.iter().enumerate() {
        assert!(used, "QUIET_MACHINE_ONLY lists {} in {}, which no assertion uses any more", QUIET_MACHINE_ONLY[i].1, QUIET_MACHINE_ONLY[i].0);
    }
    for (i, used) in not_a_speed_used.iter().enumerate() {
        assert!(used, "NOT_A_SPEED lists {} in {}, which no assertion bounds any more", NOT_A_SPEED[i].1, NOT_A_SPEED[i].0);
    }
    eprintln!("clock_guard: {seen} bounds on times read, {budgets} of them budgets");
}

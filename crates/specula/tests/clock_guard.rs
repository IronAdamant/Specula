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
//! **Nor an outcome a clock in the code decides.** A test that checks which way the code went
//! (`stopped == "frame_cap"`), where the code chose by its own time budget, bounds a speed
//! with no time in the check: `ui_engine::engine_tests::ui_act_stops_at_its_time_budget_
//! and_hands_back_the_rest` held that 160 fast frames fit `ui_act`'s 20 ms budget, and failed
//! once in a busy run (109 frames in 20.1 ms, 2026-11-24). Such a test holds the code's clock
//! (`ui_engine::hold_act_clock`, `models`' `NOW`), or checks only the outcome no machine's
//! speed can change. **Nor a wait for a worker against a deadline** (`Instant::now() <
//! deadline`, or `elapsed()` under a bound of minutes): a minute is legal here and still ran
//! out beside a release build (`specula-gpu`'s `wait_variants`, the same day). A wait for a
//! worker ends on its event (a join: `Gpu::wait_for_pipelines`, a channel, the build's own
//! completion); where a bound must stay so that a hang names itself, it is the one shared
//! `specula_core::HANG_BOUND` (ten minutes), with a message that says what was awaited, and
//! never what decides a result ([`NAMED_ANYWHERE`]). Neither of these is a check yet: a
//! review looks for them (the second lane of `m6-clock-tests` mended `light_tools`' model
//! wait, which a busy machine ended by its 10 s limit).
//!
//! **A fixed sleep as synchronisation** is read by a second check here
//! (`no_test_synchronises_by_sleeping`): a call of `std::thread::sleep` in test code (by its
//! path, spaces allowed, or by name where the file imports it) that no poll's loop encloses.
//! A poll's loop is `loop`, `while`, or a `for` its body can leave early; a sleep in it is the
//! poll's interval (what ends that wait is the condition it checks). A `for` over one item or
//! with no way out but its end, and a closure (whatever loop spawned it), are not polls. Test
//! code is a `tests/` file, a test module's own file (by name, or declared `#[cfg(test)] mod
//! name;`), or the items after a `#[cfg(test)]`; it reads `crates/`, `games/` and `tools/`.
//! A `for` leaves early only by a `break` of its own (one in a nested loop is that loop's,
//! unless labelled) or a `return` outside a closure's braces; a `while` that only counts
//! (`while n < 5 { ..; n += 1 }`, with no way out but the count) is a fixed wait too. Code
//! under `#[cfg(..)]` is test code only where the predicate needs `test` (`all(test, ..)`;
//! never inside a `not(..)`). Each sleep left is listed, with how many its file has, as
//! deciding nothing ([`SLEEPS_NOT_SYNC`]) or as found and not yet mended ([`SLEEPS_TO_MEND`]).
//! An entry is a file and a piece of the line, counted: **a new sleep with the same piece in
//! the same file can replace a mended one in the same commit** and the count still holds;
//! reviews look at a changed entry's lines. **Known blind spots:** a sleep in a helper a test
//! calls from another file; a `while` whose condition is a time (a fixed wait written as a
//! loop); a counted `loop` (`loop { n += 1; if n == 5 { break } sleep(d) }`: its `break` reads
//! as a poll's); a `return` in a closure with no braces (`|y| return y`) inside a `for`; and a
//! sleep renamed on import (`use std::thread::sleep as nap`). `std::thread` brought in under
//! another name (`use std::thread as th; th::sleep(d)`) is seen.
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
    ("crates/specula-sys/src/lib.rs", "STOPPED", 600.0, "a thread's own clock that never passed 20 ms of spinning (specula-sys has no specula-core for HANG_BOUND)"),
];

/// Upper bounds given by a name that any file may use: (the path, its value in seconds, what
/// it bounds). Written with its path, or by its last name where the file imports exactly that
/// path ([`names_the_path`]): a test's own `const HANG_BOUND` of 50 ms is not it.
const NAMED_ANYWHERE: &[(&str, f64, &str)] = &[("specula_core::HANG_BOUND", 600.0, "a wait for work a test is owed that has not ended: a hang that names itself, never what decides a result")];

/// Whether `name` (a bound as written) is `path`: the path itself (a leading `::` allowed), or
/// its last name where a `use` in `text` brings in exactly that path (`use a::B;`, or `B` in a
/// group `use a::{.., B, ..};`).
fn names_the_path(name: &str, path: &str, text: &str) -> bool {
    let name = squeezed(name);
    let name = name.trim_start_matches("::");
    if name == path {
        return true;
    }
    let Some((prefix, last)) = path.rsplit_once("::") else { return false };
    if name != last {
        return false;
    }
    let code = code_only(text);
    let mut at = 0;
    while let Some(k) = code[at..].find("use ") {
        let i = at + k;
        at = i + 4;
        if code[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let u = squeezed(code[i + 4..].split(';').next().unwrap_or(""));
        let u = u.trim_start_matches("::");
        if u == path {
            return true;
        }
        if let Some(group) = u.strip_prefix(&format!("{prefix}::{{")).and_then(|g| g.strip_suffix('}')) {
            if group.split(',').any(|item| item == last) {
                return true;
            }
        }
    }
    false
}

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

/// Sleeps in test code that no poll's loop encloses and that are not synchronisation, each
/// with why: (file, a piece of the sleep's line, how many such sleeps the file has, why the
/// time it sleeps decides nothing). A sleep that stands in for "the other thread has got there
/// by now" is never listed here: it waits on the event instead (a channel, a join, a hook the
/// code under test calls). Nor is one after which a slow machine passes the test without
/// testing it: that is [`SLEEPS_TO_MEND`]'s.
const SLEEPS_NOT_SYNC: &[(&str, &str, usize, &str)] = &[
    ("crates/specula/src/lex_live.rs", "from_millis(self.0)", 2, "a native hook's cost, inside the code under test: a busy machine only lengthens it"),
    ("crates/specula/src/steps.rs", "from_millis(1)", 1, "a job's slice made to cost time: a lower bound, which load only lengthens"),
    ("crates/specula/src/ui_engine.rs", "sleep(self.cost)", 1, "the Slow driver's frame cost: frames past the budget, which load only makes later"),
    ("crates/specula/src/text_window.rs", "KEY_GUARD_GAP", 1, "sleeps past the key guard's own window: a longer sleep is the same outcome"),
    ("crates/specula/src/trace_engine.rs", "from_millis(60)", 2, "a stall of at least 60 ms made on purpose (a lower bound), and a worker's hold of the lock; the lock's test asserts what the lock counted, made again until this thread did wait"),
    ("crates/specula/src/workers.rs", "from_millis(30)", 1, "a slow task; the order asserted is the serial lane's, which the design guarantees however long it sleeps"),
    ("crates/specula/tests/streaming_review.rs", "from_millis(1100)", 1, "past UPDATE_IDLE (1 s), so that a pass due by time runs first: a lower bound, which load only lengthens"),
    ("crates/specula-core/src/profile.rs", "from_millis(2)", 1, "a zone's cost: a lower bound"),
    ("crates/specula-gpu/src/heap.rs", "from_millis(400)", 1, "a driver's slow allocation simulated, inside the code under test"),
    ("crates/specula-sys/src/lib.rs", "from_millis(40)", 1, "the subject: time asleep must not read as a thread's running time"),
    // File times: two writes kept apart so the second has a later time (the other way round:
    // a file system that keeps times by the second needs more; this guard's header).
    ("crates/specula/src/disk.rs", "from_millis(30)", 2, "file times apart (not synchronisation; see the header on file times)"),
    ("crates/specula/src/gi.rs", "from_millis(30)", 1, "file times apart (not synchronisation; see the header on file times)"),
    ("crates/specula/src/models.rs", "from_millis(20)", 9, "file times apart (not synchronisation; see the header on file times)"),
    ("crates/specula/src/project.rs", "from_millis(20)", 5, "file times apart (not synchronisation; see the header on file times)"),
    ("crates/specula/tests/two_d_gpu.rs", "from_millis(30)", 1, "file times apart (not synchronisation; see the header on file times)"),
    ("crates/specula-logic/src/digest.rs", "from_millis(30)", 1, "file times apart (not synchronisation; see the header on file times)"),
];

/// Sleeps that stand in for an event, found and not yet mended: (file, a piece of the line,
/// how many, what is wrong and why it is left). Each passes on a quiet machine, and passes
/// without testing what it means to on a slow one (none of them can fail by load). The list
/// only keeps new ones out; mend one and lower its count (the guard says when a count is
/// more than the file has).
///
/// Left on 2026-11-24 (second lane of `m6-clock-tests`): each needs to see inside the server
/// (whether its stop has reached its wait for a running call; how many connections its accept
/// thread has handed to workers or queued for a 503), and these are `specula-protocol`'s
/// integration tests, which see only its public API. The mend is a test seam in the crate (its
/// own unit tests, or a hook the crate's tests reach), not a public accessor added for a test.
const SLEEPS_TO_MEND: &[(&str, &str, usize, &str)] = &[
    ("crates/specula-protocol/tests/mcp_http.rs", "from_millis(100)", 1, "the server is meant to have taken the silent and half-sent connections before it stops: if it has not, the stop is prompt without having had them"),
    ("crates/specula-protocol/tests/mcp_http.rs", "from_millis(300)", 2, "'the server waits for the call that is running' after 300 ms (a stop that had not reached its wait yet passes it); and the 256 held connections meant to have been handed to workers before the queued ones come"),
    ("crates/specula-protocol/tests/mcp_http.rs", "from_millis(500)", 1, "the call meant to have returned and begun its write: the test holds either side, so a slow machine tests the side it was not meant to"),
    ("crates/specula-protocol/tests/mcp_http.rs", "from_millis(200)", 1, "the 40 connections over the limit meant to be queued for their 503 before the stop"),
];

/// Whether the `r` at `i` begins a raw string's prefix: nothing of a name before it, or only
/// the `b` of `br".."`.
fn starts_word(c: &[char], i: usize) -> bool {
    let ident = |k: usize| c[k].is_alphanumeric() || c[k] == '_';
    i == 0 || !ident(i - 1) || (c[i - 1] == 'b' && (i == 1 || !ident(i - 2)))
}

/// `text` with what is not code blanked, every line kept: string literals (raw ones with their
/// `#`s too), character literals, `//` and `/* */` comments. A brace in any of them is not a
/// block's.
fn code_only(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let blank = |out: &mut String, ch: char| out.push(if ch == '\n' { '\n' } else { ' ' });
    let mut i = 0;
    while i < c.len() {
        let at = |k: usize| c.get(k).copied().unwrap_or('\0');
        if at(i) == '/' && at(i + 1) == '/' {
            while i < c.len() && c[i] != '\n' {
                blank(&mut out, c[i]);
                i += 1;
            }
        } else if at(i) == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < c.len() {
                if at(i) == '/' && at(i + 1) == '*' {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if at(i) == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    blank(&mut out, c[i]);
                    i += 1;
                }
            }
        } else if at(i) == 'r' && (at(i + 1) == '"' || at(i + 1) == '#') && starts_word(&c, i) {
            // A raw string: r"..", r#".."#, ..
            let mut k = i + 1;
            while at(k) == '#' {
                k += 1;
            }
            if at(k) != '"' {
                out.push(c[i]);
                i += 1;
                continue;
            }
            let hashes = k - i - 1;
            for ch in &c[i..=k] {
                blank(&mut out, *ch);
            }
            i = k + 1;
            while i < c.len() {
                if c[i] == '"' && (0..hashes).all(|h| at(i + 1 + h) == '#') {
                    for _ in 0..=hashes {
                        out.push(' ');
                    }
                    i += hashes + 1;
                    break;
                }
                blank(&mut out, c[i]);
                i += 1;
            }
        } else if at(i) == '"' {
            out.push(' ');
            i += 1;
            while i < c.len() {
                if c[i] == '\\' {
                    blank(&mut out, c[i]);
                    if i + 1 < c.len() {
                        blank(&mut out, c[i + 1]);
                    }
                    i += 2;
                    continue;
                }
                let end = c[i] == '"';
                blank(&mut out, c[i]);
                i += 1;
                if end {
                    break;
                }
            }
        } else if at(i) == '\'' && (at(i + 2) == '\'' || (at(i + 1) == '\\' && (at(i + 3) == '\'' || at(i + 2) == 'u'))) {
            // A character literal ('{', '\'', '\n', '\u{..}'); a lifetime has no closing quote.
            out.push(' ');
            i += 1;
            while i < c.len() && c[i] != '\'' {
                let step = if c[i] == '\\' { 2 } else { 1 };
                for ch in &c[i..(i + step).min(c.len())] {
                    blank(&mut out, *ch);
                }
                i += step;
            }
            out.push(' ');
            i += 1;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

/// `s` without its whitespace.
fn squeezed(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether the words of `s` include `w`.
fn has_word(s: &str, w: &str) -> bool {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|x| x == w)
}

/// Whether a `use` in `code` (code only) brings `std::thread::sleep` in by name, so that a bare
/// `sleep(` is it.
fn imports_sleep(code: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut at = 0;
    while let Some(k) = code[at..].find("use ") {
        let i = at + k;
        at = i + 4;
        if code[..i].chars().next_back().is_some_and(ident) {
            continue;
        }
        let u = squeezed(code[i + 4..].split(';').next().unwrap_or(""));
        let u = u.trim_start_matches("::");
        if (u.starts_with("std::thread::") || u.starts_with("thread::") || (u.starts_with("std::{") && u.contains("thread::"))) && has_word(u, "sleep") {
            return true;
        }
    }
    false
}

/// Whether a `for` loop's iterator (`iter`: the header after its ` in `) has one item: a
/// one-element array or `0..1` (a loop of one scenario waits as a straight line does).
fn one_item(iter: &str) -> bool {
    let s = squeezed(iter);
    if matches!(s.as_str(), "0..1" | "0..=0") {
        return true;
    }
    let Some(inner) = s.strip_prefix('[').or_else(|| s.strip_prefix("&[")).and_then(|r| r.strip_suffix(']')) else { return false };
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            // A second item, or `[x; n]`.
            ',' | ';' if depth == 0 => return false,
            _ => {}
        }
    }
    !inner.is_empty()
}

/// What a `{` opens.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Opens {
    /// A poll's loop: what ends it is a condition.
    Loop,
    /// A closure's body: its sleeps are its own, whatever loop made the closure.
    Closure,
    Other,
}

/// What a `{` whose `header` is the code before it (back to the last `;`, `{` or `}`) and whose
/// `body` is the code inside it opens. A closure's header ends with its `|`. A loop is a poll
/// only where what ends it is a condition: `loop` and `while`, and a `for` whose body can leave
/// it early (`break` or `return` of its own: [`leaves_early`]); a `for` over one item, or with
/// no way out but its end, and a `while` that only counts ([`counted_while`]), wait a fixed time
/// where they sleep. `for<'a>` (a bound) and `impl X for Y` are not loops.
fn opens(header: &str, body: &str) -> Opens {
    if squeezed(header).ends_with('|') {
        return Opens::Closure;
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let words: Vec<&str> = header.split(|c: char| !ident(c)).filter(|w| !w.is_empty()).collect();
    if words.contains(&"impl") {
        return Opens::Other;
    }
    if words.contains(&"while") && counted_while(header, body) {
        return Opens::Other;
    }
    if words.iter().any(|w| matches!(*w, "loop" | "while")) {
        return Opens::Loop;
    }
    let mut at = 0;
    while let Some(k) = header[at..].find("for") {
        let i = at + k;
        at = i + 3;
        let word = !header[..i].chars().next_back().is_some_and(ident) && !header[i + 3..].chars().next().is_some_and(ident);
        if !word || header[i + 3..].trim_start().starts_with('<') {
            continue;
        }
        let iter = header[i + 3..].split_once(" in ").map_or("", |(_, it)| it);
        if one_item(iter) {
            return Opens::Other;
        }
        return if leaves_early(body) { Opens::Loop } else { Opens::Other };
    }
    Opens::Other
}

/// Whether a block's `header` (the code before its `{`) starts a loop of any kind.
fn is_loop_header(header: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let words: Vec<&str> = header.split(|c: char| !ident(c)).filter(|w| !w.is_empty()).collect();
    !words.contains(&"impl") && words.iter().any(|w| matches!(*w, "loop" | "while" | "for")) && !squeezed(header).contains("for<")
}

/// Whether a loop's `body` (code only) can leave the loop before its end: a `return`, or a
/// `break` of its own. A `break` inside a nested loop is that loop's (unless it names a label),
/// and nothing inside a closure's braces leaves the loop (review 2 of `m6-clock-tests`).
fn leaves_early(body: &str) -> bool {
    let c: Vec<char> = body.chars().collect();
    let close = closings(&c);
    exits(&c, &close, 0, c.len(), false)
}

fn exits(c: &[char], close: &[usize], from: usize, to: usize, nested_loop: bool) -> bool {
    let (mut header, mut flat) = (String::new(), String::new());
    let mut i = from;
    while i < to {
        match c[i] {
            '{' => {
                let end = close[i].min(to);
                let closure = squeezed(&header).ends_with('|');
                if !closure && exits(c, close, i + 1, end, nested_loop || is_loop_header(&header)) {
                    return true;
                }
                header.clear();
                flat.push(' ');
                i = end + 1;
                continue;
            }
            ';' | '}' => {
                header.clear();
                flat.push(' ');
            }
            ch => {
                header.push(ch);
                flat.push(ch);
            }
        }
        i += 1;
    }
    has_word(&flat, "return") || if nested_loop { squeezed(&flat).contains("break'") } else { has_word(&flat, "break") }
}

/// Whether a `while` (its `header` and `body`) only counts: its condition compares a name with
/// a number (`n < 5`) and its body adds to that name, with no way out but the count. Such a
/// loop waits a fixed time where it sleeps, as a `for` over a range with no `break` does.
fn counted_while(header: &str, body: &str) -> bool {
    let s = squeezed(header);
    let Some((_, cond)) = s.split_once("while") else { return false };
    let Some(k) = cond.find(['<', '!']) else { return false };
    let (name, rest) = cond.split_at(k);
    let number = rest.trim_start_matches(['<', '!', '=']);
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    !name.is_empty() && name.chars().all(ident) && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '_') && squeezed(body).contains(&format!("{name}+=")) && !leaves_early(body)
}

/// Whether the `#[cfg(..)]` at the head of `line` (trimmed) holds only in test builds: its
/// predicate is `test`, an `all(..)` with one such part, or an `any(..)` of only such parts. A
/// `not(..)` never is (`#[cfg(not(any(test, feature = "x")))]` is production code).
fn cfg_is_test(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("#[cfg(") else { return false };
    let mut depth = 1;
    let end = rest.char_indices().find(|&(_, c)| {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        depth == 0
    });
    let Some((end, _)) = end else { return false };
    implies_test(&squeezed(&rest[..end]))
}

fn implies_test(pred: &str) -> bool {
    if pred == "test" {
        return true;
    }
    let parts = |inner: &str| -> Vec<String> {
        let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0);
        for c in inner.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(std::mem::take(&mut cur));
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    };
    if let Some(inner) = pred.strip_prefix("all(").and_then(|r| r.strip_suffix(')')) {
        return parts(inner).iter().any(|p| implies_test(p));
    }
    if let Some(inner) = pred.strip_prefix("any(").and_then(|r| r.strip_suffix(')')) {
        let ps = parts(inner);
        return !ps.is_empty() && ps.iter().all(|p| implies_test(p));
    }
    false
}

/// The names `std::thread` is brought in under by a `use .. as` (`use std::thread as th;`), so
/// that `th::sleep(` is the sleep.
fn thread_aliases(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(k) = code[at..].find("use ") {
        let i = at + k;
        at = i + 4;
        let u: Vec<&str> = code[i + 4..].split(';').next().unwrap_or("").split_whitespace().collect();
        if let [path, "as", alias] = u.as_slice() {
            if path.trim_start_matches("::") == "std::thread" {
                out.push((*alias).to_owned());
            }
        }
    }
    out
}

/// Whether the statement so far (`header`) has begun a closure with no braces, whose body the
/// code after it is: `move ||`, `(|x|`, `, ||`, `= |x|`.
fn closure_begun(header: &str) -> bool {
    let s = squeezed(header);
    s.contains("move|") || s.contains("(|") || s.contains(",|") || s.contains("=|")
}

/// For each `{` in `code`, the index of the `}` that closes it (`code.len()` when none does).
fn closings(code: &[char]) -> Vec<usize> {
    let mut close = vec![code.len(); code.len()];
    let mut open = Vec::new();
    for (i, c) in code.iter().enumerate() {
        match c {
            '{' => open.push(i),
            '}' => {
                if let Some(o) = open.pop() {
                    close[o] = i;
                }
            }
            _ => {}
        }
    }
    close
}

/// The lines (1-based) of the calls to `std::thread::sleep` in test code that no poll's loop
/// encloses. A call is `thread::sleep(` (spaces around the `::` allowed) or, where the file
/// brings it in by name (`use std::thread::sleep`), a bare `sleep(`. Test code is a whole file
/// when `whole` (a `tests/` file, a test module's own file), else the items after a
/// `#[cfg(test)]`. A sleep in a poll's loop is its interval (what ends the wait is the
/// condition the loop checks); one in a closure is the closure's own, whatever loop spawned it;
/// one anywhere else waits a fixed time, which is synchronisation by the machine's speed when
/// an assertion after it reads what another thread or process did meanwhile.
fn lone_sleeps(text: &str, whole: bool) -> Vec<usize> {
    let code_text = code_only(text);
    let bare = imports_sleep(&code_text);
    let aliases = thread_aliases(&code_text);
    let code: Vec<char> = code_text.chars().collect();
    let close = closings(&code);
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut found = Vec::new();
    // Per open brace: what it opened, and whether it opened a test region.
    let mut stack: Vec<(Opens, bool)> = Vec::new();
    let mut header = String::new();
    // A `#[cfg(test)]` waiting for its item, and the brackets opened since it: a `,` outside
    // them ends an item too (a field, a variant, an argument).
    let mut pending_test = false;
    let mut depth_since = 0i32;
    let (mut line, mut line_start) = (1, true);
    for i in 0..code.len() {
        let c = code[i];
        if line_start {
            let rest: String = code[i..].iter().take_while(|&&ch| ch != '\n').collect();
            let t = rest.trim();
            if cfg_is_test(t) {
                pending_test = true;
                depth_since = 0;
            }
            line_start = false;
        }
        if code[i..].starts_with(&['s', 'l', 'e', 'e', 'p']) && (i == 0 || !ident(code[i - 1])) && !code.get(i + 5).is_some_and(|&n| ident(n)) {
            let called = code[i + 5..].iter().find(|ch| !ch.is_whitespace()) == Some(&'(');
            let start = i.saturating_sub(40);
            let before = squeezed(&code[start..i].iter().collect::<String>());
            let via_alias = |a: &String| before.strip_suffix(&format!("{a}::")).is_some_and(|b| !b.chars().next_back().is_some_and(ident));
            let via_thread = before.ends_with("thread::") || aliases.iter().any(via_alias);
            let by_name = bare && !before.ends_with("::") && !before.ends_with('.') && !before.ends_with("fn");
            if called && (via_thread || by_name) {
                let in_test = whole || stack.iter().any(|&(_, test)| test);
                // The innermost loop or closure decides: a closure's sleep is its own.
                let polled = !closure_begun(&header) && stack.iter().rev().find(|&&(o, _)| o != Opens::Other).is_some_and(|&(o, _)| o == Opens::Loop);
                if in_test && !polled && found.last() != Some(&line) {
                    found.push(line);
                }
            }
        }
        match c {
            '\n' => {
                line += 1;
                line_start = true;
                header.push(' ');
            }
            '{' => {
                // (The body is read only for a `for` or a `while`: whether it can leave early.)
                let body: String = if has_word(&header, "for") || has_word(&header, "while") { code[i + 1..close[i]].iter().collect() } else { String::new() };
                stack.push((opens(&header, &body), pending_test));
                pending_test = false;
                header.clear();
            }
            '}' => {
                stack.pop();
                header.clear();
            }
            ';' => {
                header.clear();
                // `#[cfg(test)] use ..;` or `mod x;`: the attribute is spent.
                pending_test = false;
            }
            '(' | '[' | '<' => {
                depth_since += 1;
                header.push(c);
            }
            // (`->` and `=>` close nothing.)
            '>' if i > 0 && matches!(code[i - 1], '-' | '=') => header.push(c),
            ')' | ']' | '>' => {
                depth_since -= 1;
                header.push(c);
            }
            ',' => {
                if depth_since <= 0 {
                    pending_test = false;
                }
                header.push(c);
            }
            _ => header.push(c),
        }
    }
    found
}

/// For each line of `text` (1-based, index 0 unused), whether it is test code: inside an item
/// after a `#[cfg(test)]` (the same reading as [`lone_sleeps`]: an attribute is spent at its
/// item's `{`, `;` or a `,` outside brackets).
fn test_lines(text: &str) -> Vec<bool> {
    let code: Vec<char> = code_only(text).chars().collect();
    let mut out = vec![false, false];
    let mut stack: Vec<bool> = Vec::new();
    let (mut pending, mut depth, mut line_start) = (false, 0i32, true);
    for (i, &c) in code.iter().enumerate() {
        if line_start {
            let rest: String = code[i..].iter().take_while(|&&ch| ch != '\n').collect();
            let t = rest.trim();
            if cfg_is_test(t) {
                pending = true;
                depth = 0;
            }
            line_start = false;
        }
        let here = out.len() - 1;
        out[here] |= stack.iter().any(|&t| t) || pending;
        match c {
            '\n' => {
                out.push(false);
                line_start = true;
            }
            '{' => {
                stack.push(pending);
                pending = false;
            }
            '}' => {
                stack.pop();
            }
            ';' => pending = false,
            '(' | '[' | '<' => depth += 1,
            '>' if i > 0 && matches!(code[i - 1], '-' | '=') => {}
            ')' | ']' | '>' => depth -= 1,
            ',' if depth <= 0 => pending = false,
            _ => {}
        }
    }
    out
}

/// The test aids of `specula_core` (`HANG_BOUND`, `Watchdog`) are public only so that every
/// crate's tests can reach them: no engine or game code uses them (a hang bound or an abort in
/// the engine would be a behaviour of its own). Read in every source outside test code.
#[test]
fn the_test_aids_are_used_by_tests_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    for top in ["crates", "games", "tools"] {
        rust_files(&root.join(top), &mut paths);
    }
    let files: Vec<(String, String)> = paths.iter().filter_map(|p| Some((p.strip_prefix(&root).unwrap_or(p).to_string_lossy().replace('\\', "/"), std::fs::read_to_string(p).ok()?))).collect();
    let modules = test_module_files(&files);
    let mut bad = Vec::new();
    let mut defined = 0;
    for (rel, text) in &files {
        let name = rel.rsplit('/').next().unwrap_or("");
        if rel.contains("/tests/") || rel.contains("/benches/") || rel.contains("/examples/") || name == "tests.rs" || name.ends_with("_tests.rs") || modules.contains(rel) || rel.ends_with("tests/clock_guard.rs") {
            continue;
        }
        let tests = test_lines(text);
        let code = code_only(text);
        for (n, line) in code.lines().enumerate() {
            let n = n + 1;
            if tests.get(n).copied().unwrap_or(false) || !(has_word(line, "HANG_BOUND") || has_word(line, "Watchdog")) {
                continue;
            }
            if rel == "crates/specula-core/src/lib.rs" {
                // Their own definitions.
                defined += 1;
                continue;
            }
            bad.push(format!("{rel}:{n}: {}", line.trim()));
        }
    }
    assert!(defined > 0, "specula_core's HANG_BOUND and Watchdog are gone: take this check off");
    assert!(bad.is_empty(), "specula_core's test aids used outside test code:\n{}", bad.join("\n"));
}

#[test]
fn test_lines_follow_cfg_test() {
    let t = test_lines("fn a() {}\n#[cfg(test)]\nmod tests {\n    fn b() {}\n}\nstruct S {\n    #[cfg(test)]\n    n: u32,\n}\nfn c() {}\n");
    assert_eq!((t[1], t[3], t[4], t[8], t[10]), (false, true, true, true, false), "{t:?}");
}

/// The files only tests compile, beyond `tests/` folders and test-module names: those a
/// `#[cfg(test)] mod name;` declares, as `name.rs` or `name/mod.rs` beside the declaring
/// module (`specula-formae`'s `tests_solid.rs` and `tests_raise.rs` are two).
fn test_module_files(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, text) in files {
        let code = code_only(text);
        let lines: Vec<&str> = code.lines().map(str::trim).collect();
        let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel.as_str()));
        let own = if matches!(file, "lib.rs" | "main.rs" | "mod.rs") { dir.to_owned() } else { format!("{dir}/{}", file.trim_end_matches(".rs")) };
        for (k, l) in lines.iter().enumerate() {
            let Some(rest) = l.strip_prefix("#[cfg(test)]") else { continue };
            let decl = if rest.trim().is_empty() { lines.get(k + 1).copied().unwrap_or("") } else { rest.trim() };
            let decl = decl.strip_prefix("pub(crate) ").or_else(|| decl.strip_prefix("pub ")).unwrap_or(decl);
            if let Some(name) = decl.strip_prefix("mod ").and_then(|n| n.strip_suffix(';')) {
                out.push(format!("{own}/{}.rs", name.trim()));
                out.push(format!("{own}/{}/mod.rs", name.trim()));
            }
        }
    }
    out
}

#[test]
fn the_scanner_finds_sleeps_outside_loops() {
    let text = "fn real() {\n    std::thread::sleep(d);\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        loop {\n            std::thread::sleep(d);\n        }\n        while x {\n            if y { std::thread::sleep(d); }\n        }\n        let s = \"{\";\n        std::thread::sleep(d);\n        for _ in 0..3 { std::thread::sleep(d); if z { break } }\n    }\n    impl A for B {\n        fn f() { std::thread::sleep(d); }\n    }\n}\n";
    assert_eq!(lone_sleeps(text, false), [14, 18], "the sleep after the loops and the one in an impl: not the engine's, not a poll's");
    assert_eq!(lone_sleeps(text, true), [2, 14, 18], "a whole test file");
    // Braces in raw strings, character literals and comments are not blocks.
    let text = "#[cfg(all(test, windows))]\nmod t {\n    fn f() {\n        let j = r#\"{\"a\": [{\"b\": 1}]}\"#; let c = '{'; let q = '\\''; // }\n        /* } */ std::thread::sleep(d);\n        loop { let s = br\"}\"; std::thread::sleep(d); }\n    }\n}\n";
    assert_eq!(lone_sleeps(text, false), [5]);
    assert_eq!(opens("let status = loop ", ""), Opens::Loop);
    assert_eq!(opens(" 'outer: while let Some(x) = it.next() ", ""), Opens::Loop);
    assert_eq!(opens("impl Driver for Slow ", ""), Opens::Other);
    assert_eq!(opens("fn format_for(x: u32) ", ""), Opens::Other);
    assert_eq!(opens(" 'outer: for (i, x) in xs.iter().enumerate() ", "if i > 3 { break 'outer; }"), Opens::Loop);
    assert_eq!(opens("let f = move |x: u32| ", ""), Opens::Closure);
}

/// The cases a review found the first scanner blind to (review 1 of `m6-clock-tests`, its
/// `guardprobe` snippets), and its false positive: each is held here.
#[test]
fn the_scanner_sees_what_the_first_one_missed() {
    let t = |body: &str| format!("#[cfg(test)]\nmod tests {{\n{body}\n}}\n");
    // A sleep brought in by name, and one with spaces in its path.
    assert_eq!(lone_sleeps(&t(" use std::thread::sleep;\n fn t() {\n  sleep(d);\n }"), false), [5]);
    assert_eq!(lone_sleeps(&t(" use std::{thread::{self, sleep}, time::Duration};\n fn t() {\n  sleep(d);\n }"), false), [5], "in a nested group");
    assert_eq!(lone_sleeps(&t(" fn t() {\n  std::thread:: sleep(d);\n  thread ::sleep (d);\n }"), false), [4, 5]);
    // Not a sleep: a method of that name, and a file's own function where std's is not imported.
    assert!(lone_sleeps(&t(" fn sleep(x: u32) {}\n fn t() {\n  w.sleep(d);\n  sleep(3);\n  let sleep = 1;\n }"), false).is_empty());
    // A closure spawned in a loop sleeps on its own thread: the loop is not its poll.
    assert_eq!(lone_sleeps(&t(" fn t() {\n  for i in 0..4 {\n   s.spawn(move || { std::thread::sleep(d); go(i) });\n  }\n  assert!(x);\n }"), false), [5]);
    assert_eq!(lone_sleeps(&t(" fn t() {\n  loop {\n   s.spawn(move || std::thread::sleep(d));\n   if done() { break }\n  }\n }"), false), [5], "a closure with no braces too");
    // A loop over one scenario, and a `for` with no way out but its end, wait a fixed time.
    assert_eq!(lone_sleeps(&t(" fn t() {\n  for case in [a] {\n   start(case);\n   std::thread::sleep(d);\n   assert!(done(case));\n  }\n }"), false), [6]);
    assert_eq!(lone_sleeps(&t(" fn t() {\n  for case in [a, b] {\n   start(case);\n   std::thread::sleep(d);\n   assert!(done(case));\n  }\n }"), false), [6], "one fixed wait a scenario");
    assert!(lone_sleeps(&t(" fn t() {\n  for _ in 0..400 {\n   step();\n   if done() { break; }\n   std::thread::sleep(d);\n  }\n }"), false).is_empty(), "a poll by count: what ends it is its condition");
    // A bound `for<'a>` is not a loop.
    assert_eq!(lone_sleeps(&t(" fn t<F>(f: F) where F: for<'a> Fn(&'a u8) {\n  std::thread::sleep(d);\n  assert!(x);\n }"), false), [4]);
    // `#[cfg(test)]` on a struct field (or a variant, an argument) is spent at its comma: the
    // function after the struct is not test code.
    assert!(lone_sleeps("struct A {\n #[cfg(test)]\n n: u32,\n}\nfn prod() {\n std::thread::sleep(d);\n}\n", false).is_empty(), "a production function after a test-only field");
    assert!(lone_sleeps("struct A {\n #[cfg(test)]\n n: BTreeMap<u32, u64>,\n m: u8,\n}\nfn prod() {\n std::thread::sleep(d);\n}\n", false).is_empty());
    assert_eq!(lone_sleeps("#[cfg(test)]\nfn helper<A, B>(a: A, b: B) {\n std::thread::sleep(d);\n}\n", false), [3], "commas inside an item's own brackets do not spend it");
    // Lifetimes, escaped quotes and macros do not confuse it.
    assert_eq!(lone_sleeps(&t(" fn t<'a>(x: &'a str) {\n  let s = \"a\\\"{\";\n  let v = vec![1];\n  std::thread::sleep(d);\n }"), false), [6]);
    // A test module's own file, declared `#[cfg(test)] mod tests_solid;`.
    let files = [("crates/f/src/lib.rs".to_owned(), "#[cfg(test)]\nmod tests_solid;\n#[cfg(test)] mod inner;\nmod real;\n".to_owned()), ("crates/f/src/a.rs".to_owned(), "#[cfg(test)]\npub(crate) mod deep;\n".to_owned())];
    let mods = test_module_files(&files);
    assert!(mods.contains(&"crates/f/src/tests_solid.rs".to_owned()) && mods.contains(&"crates/f/src/inner.rs".to_owned()) && mods.contains(&"crates/f/src/a/deep.rs".to_owned()) && !mods.iter().any(|m| m.contains("real")), "{mods:?}");
}

/// The cases the second review found (review 2 of `m6-clock-tests`, its `scan/probe.rs`
/// snippets): the four it missed, its false positive, and a bound that only borrowed the name.
#[test]
fn the_scanner_sees_what_the_second_review_found() {
    let t = |body: &str| format!("#[cfg(test)]\nmod tests {{\n{body}\n}}\n");
    // (a) A `for` whose only `break` is a nested loop's waits a fixed time a scenario; a
    // labelled break, or a `return`, inside the nested loop still leaves it.
    assert_eq!(lone_sleeps(&t(" fn t() {\n  for case in [a, b] {\n   start(case);\n   std::thread::sleep(d);\n   for x in xs { if x { break } }\n   assert!(done(case));\n  }\n }"), false), [6]);
    assert!(lone_sleeps(&t(" fn t() {\n  'o: for case in [a, b] {\n   std::thread::sleep(d);\n   for x in xs { if x { break 'o; } }\n  }\n }"), false).is_empty());
    assert!(lone_sleeps(&t(" fn t() {\n  for case in [a, b] {\n   std::thread::sleep(d);\n   while y { if x { return; } }\n  }\n }"), false).is_empty());
    // (b) A `return` inside a closure in the body is the closure's.
    assert_eq!(lone_sleeps(&t(" fn t() {\n  for case in [a, b] {\n   let f = |y| { return y; };\n   std::thread::sleep(d);\n   assert!(f(case));\n  }\n }"), false), [6]);
    // (c) A counted `while` with no way out but its count waits a fixed time; one that can
    // leave early is a poll by count.
    assert_eq!(lone_sleeps(&t(" fn t() {\n  let mut n = 0;\n  while n < 5 {\n   std::thread::sleep(d);\n   n += 1;\n  }\n }"), false), [6]);
    assert!(lone_sleeps(&t(" fn t() {\n  while n < 400 {\n   if done() { break }\n   std::thread::sleep(d);\n   n += 1;\n  }\n }"), false).is_empty());
    assert!(lone_sleeps(&t(" fn t() {\n  while !done() {\n   std::thread::sleep(d);\n  }\n }"), false).is_empty(), "a poll's while");
    // (c) A counted `loop` is a known blind spot (the header): what ends it is a condition, and
    // the scanner does not tell a count from a poll there. When it learns to, this is [7].
    assert!(lone_sleeps(&t(" fn t() {\n  loop {\n   n += 1;\n   if n == 5 { break }\n   std::thread::sleep(d);\n  }\n }"), false).is_empty());
    // (c) `std::thread` brought in under another name.
    assert_eq!(lone_sleeps(&t(" use std::thread as th;\n fn t() {\n  th::sleep(d);\n  smooth::sleep(d);\n }"), false), [5]);
    // The false positive: `not(..)` around `any(test, ..)` is production code.
    assert!(lone_sleeps("#[cfg(not(any(test, feature = \"x\")))]\nfn prod() {\n std::thread::sleep(d);\n}\n", false).is_empty());
    let lines = test_lines("#[cfg(not(any(test, feature = \"x\")))]\nfn prod() {\n x();\n}\n#[cfg(all(test, windows))]\nfn t() {}\n");
    assert_eq!((lines[2], lines[6]), (false, true), "{lines:?}");
    assert!(cfg_is_test("#[cfg(test)]") && cfg_is_test("#[cfg(all(windows, test))] mod t;") && cfg_is_test("#[cfg(any(test, all(test, unix)))]"));
    assert!(!cfg_is_test("#[cfg(not(test))]") && !cfg_is_test("#[cfg(any(test, feature = \"x\"))]") && !cfg_is_test("#[cfg(feature = \"test\")]") && !cfg_is_test("#[cfg_attr(test, ignore)]"));
    // A bound named `HANG_BOUND` is the core's only by its path or an import of exactly it.
    let core = "specula_core::HANG_BOUND";
    assert!(names_the_path("specula_core::HANG_BOUND", core, "") && names_the_path("::specula_core::HANG_BOUND", core, ""));
    assert!(!names_the_path("HANG_BOUND", core, "const HANG_BOUND: Duration = Duration::from_millis(50);"), "a test's own constant");
    assert!(!names_the_path("my::HANG_BOUND", core, ""));
    assert!(names_the_path("HANG_BOUND", core, "use specula_core::HANG_BOUND;") && names_the_path("HANG_BOUND", core, "use specula_core::{speed_budget, HANG_BOUND};"));
    assert!(!names_the_path("HANG_BOUND", core, "use other::HANG_BOUND;"));
}

/// No test waits a fixed time for another thread or process to have done something: it waits
/// on the event. (`launch_run`'s test slept 300 ms and then dropped a lock "while the command
/// waits"; on a loaded machine the command had not reached the wait, found the folder free and
/// started an engine: `not_started` where `in_use` was meant, 2026-11-24.) Each entry of the
/// lists says how many sleeps it covers in its file, so that one more of the same length there
/// fails.
#[test]
fn no_test_synchronises_by_sleeping() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    for top in ["crates", "games", "tools"] {
        rust_files(&root.join(top), &mut paths);
    }
    let files: Vec<(String, String)> = paths
        .iter()
        .filter_map(|p| Some((p.strip_prefix(&root).unwrap_or(p).to_string_lossy().replace('\\', "/"), std::fs::read_to_string(p).ok()?)))
        .filter(|(rel, _)| !rel.ends_with("tests/clock_guard.rs"))
        .collect();
    assert!(files.iter().any(|(r, _)| r.starts_with("tools/")) && files.len() > 200, "found {} files: has the layout changed?", files.len());
    let modules = test_module_files(&files);
    let mut bad = Vec::new();
    let mut not_sync = vec![0usize; SLEEPS_NOT_SYNC.len()];
    let mut to_mend = vec![0usize; SLEEPS_TO_MEND.len()];
    for (rel, text) in &files {
        let name = rel.rsplit('/').next().unwrap_or("");
        let whole = rel.contains("/tests/") || rel.contains("/benches/") || name == "tests.rs" || name.ends_with("_tests.rs") || modules.contains(rel);
        let lines: Vec<&str> = text.lines().collect();
        for n in lone_sleeps(text, whole) {
            let line = lines[n - 1];
            let listed = |list: &[(&str, &str, usize, &str)]| list.iter().position(|(f, piece, _, _)| rel.ends_with(f) && line.contains(piece));
            match (listed(SLEEPS_NOT_SYNC), listed(SLEEPS_TO_MEND)) {
                (Some(i), _) => not_sync[i] += 1,
                (None, Some(i)) => to_mend[i] += 1,
                (None, None) => bad.push(format!("{rel}:{n}: {}", line.trim())),
            }
        }
    }
    for (list, name, seen) in [(SLEEPS_NOT_SYNC, "SLEEPS_NOT_SYNC", &not_sync), (SLEEPS_TO_MEND, "SLEEPS_TO_MEND", &to_mend)] {
        for (i, (file, piece, count, _)) in list.iter().enumerate() {
            if seen[i] > *count {
                bad.push(format!("{file}: {} sleeps match '{piece}', and {name} lists {count}: a new one", seen[i]));
            }
            assert!(seen[i] >= *count, "{name} lists {count} sleeps '{piece}' in {file}, and {} are there: mended? lower the count, or take the entry off at 0", seen[i]);
        }
    }
    assert!(
        bad.is_empty(),
        "{} sleeps in test code outside any poll's loop: a fixed wait for another thread or process passes on a quiet machine and fails on a busy one.\n\
         Wait on the event (a channel, a join, a hook the code calls); if the sleep decides nothing, list it in SLEEPS_NOT_SYNC with why:\n{}",
        bad.len(),
        bad.join("\n")
    );
    eprintln!("clock_guard: {} sleeps outside loops are listed as still to mend", SLEEPS_TO_MEND.iter().map(|e| e.2).sum::<usize>());
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
                    Err(name) if NAMED_ANYWHERE.iter().any(|(path, s, _)| *s >= MIN_SECONDS && names_the_path(name, path, &text)) => {}
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

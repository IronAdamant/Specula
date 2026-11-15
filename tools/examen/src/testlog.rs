//! What a `cargo test` log says. The gate reads whole logs: a pass line at the end does not hide
//! a failure in the middle, a run in which no test ran is not a pass, and what each failed test
//! printed is kept with its name.

use std::collections::BTreeMap;

/// How a step's process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// It outlived its limit and was stopped.
    TimedOut,
}

impl Exit {
    pub fn ok(self) -> bool {
        self == Exit::Code(0)
    }

    pub fn said(self) -> String {
        match self {
            Exit::Code(c) => format!("exit {c}"),
            Exit::TimedOut => "TIMED OUT".to_owned(),
        }
    }
}

/// What a cargo test log says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub passed: u64,
    pub failed: u64,
    /// The failed tests' names, sorted, each once.
    pub failed_tests: Vec<String>,
    /// Something did not compile, or a test binary died without its result line (8 at most).
    pub broken: Vec<String>,
    /// Tests that were still running when the log ends (8 at most).
    pub still_running: Vec<String>,
    /// How many times the Vulkan loader said a GPU driver failed to load in a test process. Not
    /// a failure of the code under test, but those tests ran without that adapter.
    pub drivers_not_loaded: usize,
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `test NAME ... ok|FAILED|ignored`: (name, outcome). The name is the shortest that fits.
fn test_line(line: &str) -> Option<(&str, &'static str)> {
    let rest = line.strip_prefix("test ")?;
    let mut from = rest.chars().next()?.len_utf8();
    while from <= rest.len() {
        let at = from + rest.get(from..)?.find(" ... ")?;
        let after = &rest[at + 5..];
        for outcome in ["ok", "FAILED", "ignored"] {
            if let Some(tail) = after.strip_prefix(outcome) {
                if tail.chars().next().is_none_or(|c| !word_char(c)) {
                    return Some((&rest[..at], outcome));
                }
            }
        }
        from = at + 1;
        while !rest.is_char_boundary(from) {
            from += 1;
        }
    }
    None
}

fn digits(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `test result: WORD. N passed; N failed; N ignored`: (passed, failed).
fn result_line(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("test result: ")?;
    let (word, rest) = rest.split_once(". ")?;
    if word.is_empty() || !word.chars().all(word_char) {
        return None;
    }
    let (passed, rest) = rest.split_once(" passed; ")?;
    let (failed, rest) = rest.split_once(" failed; ")?;
    let (ignored, _) = rest.split_once(" ignored")?;
    digits(ignored)?;
    Some((digits(passed)?, digits(failed)?))
}

/// `test NAME has been running for over N seconds`: the name.
fn running_line(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("test ")?;
    const MARK: &str = " has been running for over ";
    let mut from = rest.chars().next()?.len_utf8();
    while from <= rest.len() {
        let at = from + rest.get(from..)?.find(MARK)?;
        let after = &rest[at + MARK.len()..];
        let n = after.bytes().take_while(u8::is_ascii_digit).count();
        if n > 0 && after[n..].starts_with(" seconds") {
            return Some(&rest[..at]);
        }
        from = at + 1;
        while !rest.is_char_boundary(from) {
            from += 1;
        }
    }
    None
}

fn clip(line: &str, chars: usize) -> String {
    line.trim().chars().take(chars).collect()
}

pub fn parse(log: &str) -> Parsed {
    let mut p = Parsed::default();
    let mut names: Vec<String> = Vec::new();
    for line in log.lines() {
        if let Some((name, outcome)) = test_line(line) {
            if outcome == "FAILED" {
                names.push(name.to_owned());
            }
            p.still_running.retain(|r| r != name);
        }
        if let Some(name) = running_line(line) {
            if !p.still_running.iter().any(|r| r == name) {
                p.still_running.push(name.to_owned());
            }
        }
        if let Some((passed, failed)) = result_line(line) {
            p.passed += passed;
            p.failed += failed;
        }
        if line.starts_with("error: could not compile") || line.starts_with("error[E") {
            p.broken.push(clip(line, 160));
        }
        if line.starts_with("error: test failed, to rerun pass") && names.is_empty() {
            p.broken.push(clip(line, 160));
        }
        if line.contains("(exit code: 0x") || line.contains("process didn't exit successfully") {
            p.broken.push(clip(line, 160));
        }
        if line.contains("Failed loading library associated with ICD") {
            p.drivers_not_loaded += 1;
        }
    }
    names.sort();
    names.dedup();
    p.failed_tests = names;
    p.broken.truncate(8);
    p.still_running.truncate(8);
    p
}

/// A line's leading colour codes and blanks, removed.
fn skip_colour_and_blanks(mut s: &str) -> &str {
    loop {
        let t = s.trim_start();
        match colour_code(t) {
            Some(rest) => s = rest,
            None => return t,
        }
    }
}

/// `ESC [ digits-and-semicolons m` at the start of `s`: what follows it.
fn colour_code(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("\x1b[")?;
    let n = rest.bytes().take_while(|b| b.is_ascii_digit() || *b == b';').count();
    rest[n..].strip_prefix('m')
}

/// Cargo's line before each test binary's output (in colour on CI): the binary's name.
fn binary_line(line: &str) -> Option<&str> {
    let s = skip_colour_and_blanks(line);
    let mut rest = s.strip_prefix("Running").or_else(|| s.strip_prefix("Doc-tests"))?;
    while let Some(r) = colour_code(rest) {
        rest = r;
    }
    let rest = rest.strip_prefix(' ')?;
    let name = match rest.get(1..).and_then(|r| r.find(" (")) {
        Some(i) if rest.ends_with(')') => &rest[..i + 1],
        _ => rest,
    };
    (!name.is_empty()).then_some(name)
}

/// `---- NAME stdout ----`: the name.
fn said_line(line: &str) -> Option<&str> {
    let name = line.strip_prefix("---- ")?.strip_suffix(" stdout ----")?;
    (!name.is_empty()).then_some(name)
}

/// A line as ASCII: a character outside it is written as an escape (a message may quote any
/// text, and the gate's own output is ASCII).
fn ascii(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        let n = c as u32;
        if n < 0x80 {
            out.push(c);
        } else if n < 0x100 {
            out.push_str(&format!("\\x{n:02x}"));
        } else if n < 0x1_0000 {
            out.push_str(&format!("\\u{n:04x}"));
        } else {
            out.push_str(&format!("\\U{n:08x}"));
        }
    }
    out
}

/// What each failed test of a cargo test log printed (its panic message): (name, text) in the
/// log's order, at most `lines` lines of 300 characters each, in ASCII. A name that failed in
/// two test binaries of one log is two entries, each with its binary.
pub fn outputs(log: &str, lines: usize) -> Vec<(String, String)> {
    let mut found: Vec<((String, String), Vec<String>)> = Vec::new();
    let mut binary = String::new();
    let mut key: Option<usize> = None;
    for line in log.lines() {
        if let Some(b) = binary_line(line) {
            binary = b.to_owned();
            key = None;
        } else if let Some(name) = said_line(line) {
            let k = (binary.clone(), name.to_owned());
            key = Some(match found.iter().position(|(have, _)| *have == k) {
                Some(i) => i,
                None => {
                    found.push((k, Vec::new()));
                    found.len() - 1
                }
            });
        } else if line.trim() == "failures:" || line.starts_with("test result:") {
            key = None;
        } else if let Some(i) = key {
            if !line.trim().is_empty() && found[i].1.len() < lines {
                let clipped: String = line.trim_end().chars().take(300).collect();
                found[i].1.push(ascii(&clipped));
            }
        }
    }
    let count = |name: &str| found.iter().filter(|((_, n), _)| n == name).count();
    found
        .iter()
        .map(|((place, name), text)| {
            let shown = if count(name) == 1 { name.clone() } else { format!("{name} [{place}]") };
            (shown, text.join("\n"))
        })
        .collect()
}

/// Why a test run is not a pass (empty when it is one).
pub fn verdict(r: &Parsed, exit: Exit) -> Vec<String> {
    let mut why = Vec::new();
    if exit == Exit::TimedOut {
        let still = if r.still_running.is_empty() { "unknown (see the log)".to_owned() } else { format!("{:?}", r.still_running) };
        why.push(format!("timed out; still running: {still}"));
    }
    if r.failed > 0 || !r.failed_tests.is_empty() {
        let shown: Vec<&String> = r.failed_tests.iter().take(12).collect();
        why.push(format!("{} failed {shown:?}", r.failed));
    }
    if !r.broken.is_empty() {
        let shown: Vec<&String> = r.broken.iter().take(3).collect();
        why.push(format!("broken: {shown:?}"));
    }
    if let Exit::Code(c) = exit {
        if c != 0 && why.is_empty() {
            why.push(format!("exit {c} (see the log)"));
        }
    }
    if why.is_empty() && r.passed == 0 {
        why.push("no test ran (a filter that matches nothing, or nothing was built)".to_owned());
    }
    why
}

/// Tests that failed in some of `runs` and not in others, with how often.
pub fn flaky(runs: &[Parsed]) -> BTreeMap<String, usize> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for r in runs {
        for t in &r.failed_tests {
            *seen.entry(t.clone()).or_default() += 1;
        }
    }
    seen.retain(|_, n| *n < runs.len());
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK_LOG: &str = "running 2 tests\ntest a::b ... ok\ntest a::c ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    const BAD_LOG: &str = "running 3 tests\ntest a::b ... ok\ntest a::flaky ... FAILED\ntest a::c ... ok\n\nfailures:\n    a::flaky\n\n\
        test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n\ntest result: ok. 5 passed; 0 failed; 1 ignored; 0 measured\n";

    #[test]
    fn a_pass_is_a_pass_and_a_failure_in_the_middle_is_not() {
        let (a, b) = (parse(OK_LOG), parse(BAD_LOG));
        assert_eq!(a, Parsed { passed: 2, ..Parsed::default() });
        assert!(b.passed == 7 && b.failed == 1 && b.failed_tests == ["a::flaky"], "{b:?}");
        assert!(BAD_LOG.trim_end().ends_with("0 measured"), "the log's last line says ok");
        assert!(verdict(&a, Exit::Code(0)).is_empty());
        assert!(!verdict(&b, Exit::Code(101)).is_empty());
        assert_eq!(verdict(&a, Exit::Code(3)), ["exit 3 (see the log)"], "a non-zero exit with a clean log is not a pass");
    }

    #[test]
    fn flaky_is_failing_in_some_runs_and_not_in_others() {
        let (a, b) = (parse(OK_LOG), parse(BAD_LOG));
        assert_eq!(flaky(&[a.clone(), b.clone(), a]), BTreeMap::from([("a::flaky".to_owned(), 1)]));
        assert!(flaky(&[b.clone(), b]).is_empty(), "failing every time isn't flaky: it is plainly failing");
    }

    #[test]
    fn a_driver_that_did_not_load_is_counted() {
        let log = "t: [Warning] Loader Message loader_icd_scan: Failed loading library associated with ICD JSON x.dll. Ignoring this JSON\n";
        assert_eq!(parse(log).drivers_not_loaded, 1);
    }

    /// CI, 2026-11-15: a test failed once in three runs and the gate gave its name only; the
    /// logs went with the machine.
    #[test]
    fn what_a_failed_test_said_is_kept_with_its_name() {
        let told = "running 2 tests\ntest a::b ... ok\ntest a::flaky ... FAILED\n\nfailures:\n\n---- a::flaky stdout ----\n\n\
            thread 'a::flaky' (7) panicked at src/a.rs:3:5:\nassertion `left == right` failed: the folder \u{5893}\n  left: None\n right: Some(1)\n\
            note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n---- a::other stdout ----\nsecond\n\n\nfailures:\n    a::flaky\n    a::other\n\n\
            test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out\nnot a test's line\n";
        let o = outputs(told, 25);
        assert_eq!(o.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["a::flaky", "a::other"]);
        assert_eq!(o[1].1, "second");
        assert_eq!(o[0].1.lines().nth(1), Some("assertion `left == right` failed: the folder \\u5893"));
        assert!(o[0].1.contains("left: None"));
        let all: String = o.iter().map(|(_, t)| t.as_str()).collect();
        assert!(!all.contains("not a test's line") && !all.contains("a::flaky\n    a::other"), "{all}");
        assert_eq!(outputs(told, 2)[0].1.lines().count(), 2);
        assert!(outputs(OK_LOG, 25).is_empty());
    }

    /// Review R1 (m5-ci2): one name failing in two binaries of one log is two entries.
    #[test]
    fn one_name_in_two_binaries_is_two_entries() {
        let two = "     Running unittests src/lib.rs (target/debug/deps/a-1)\n---- tests::x stdout ----\nfirst\n\nfailures:\n    tests::x\n\
            \x1b[1m\x1b[92m     Running\x1b[0m tests/b.rs (target/debug/deps/b-2)\n---- tests::x stdout ----\nsecond\n---- tests::y stdout ----\nthird\n\nfailures:\n   \
            Doc-tests a\n---- src/lib.rs - f (line 3) stdout ----\nfourth\n";
        let want = [("tests::x [unittests src/lib.rs]", "first"), ("tests::x [tests/b.rs]", "second"), ("tests::y", "third"), ("src/lib.rs - f (line 3)", "fourth")];
        let got = outputs(two, 25);
        assert_eq!(got.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect::<Vec<_>>(), want);
    }

    /// Review R2 (m5-gpuwork): three ways the gate could say PASSED with nothing proven.
    #[test]
    fn nothing_proven_is_not_a_pass() {
        let c = parse("error[E0599]: no method named `x`\nerror: could not compile `specula` (lib test) due to 1 previous error\n");
        assert!(c.broken.len() == 2 && c.passed == 0 && !verdict(&c, Exit::Code(101)).is_empty(), "{c:?}");
        let d = parse("error: test failed, to rerun pass `-p specula --lib`\n");
        assert!(!d.broken.is_empty(), "a test binary that died without naming a test is not a pass");
        let none = parse("running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out\n");
        assert_eq!(verdict(&none, Exit::Code(0)), ["no test ran (a filter that matches nothing, or nothing was built)"]);
        let hung = parse("running 2 tests\ntest a::b ... ok\ntest a::slow has been running for over 60 seconds\ntest a::stuck has been running for over 60 seconds\ntest a::slow ... ok\n");
        assert_eq!(hung.still_running, ["a::stuck"]);
        assert!(verdict(&hung, Exit::TimedOut)[0].contains("a::stuck"));
    }

    /// Review R3: a doc-test's name has spaces; it is named like any other failure (so it can
    /// be reported as flaky).
    #[test]
    fn a_doc_tests_name_has_spaces() {
        let doc = parse("running 1 test\ntest crates/x/src/lib.rs - speed_budget (line 12) ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n");
        assert_eq!(doc.failed_tests, ["crates/x/src/lib.rs - speed_budget (line 12)"]);
    }

    #[test]
    fn lines_that_only_look_like_results_are_not_read() {
        assert_eq!(test_line("test a ... okay"), None, "the outcome is a whole word");
        assert_eq!(test_line("test a ... ok ... FAILED"), Some(("a", "ok")), "the shortest name");
        assert_eq!(test_line("test  ... ok"), None, "a name is at least one character");
        assert_eq!(test_line("test \u{e9}cole ... ok"), Some(("\u{e9}cole", "ok")));
        assert_eq!(result_line("test result: ok. x passed; 0 failed; 0 ignored"), None);
        assert_eq!(running_line("test a has been running for over many seconds"), None);
        assert_eq!(binary_line("     Running `rustc --crate-name x`"), Some("`rustc --crate-name x`"));
        assert_eq!(binary_line("Runningtests"), None);
        assert_eq!(said_line("----  stdout ----"), None);
    }
}

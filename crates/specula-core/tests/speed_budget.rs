//! `speed_budget` reads `SPECULA_SPEED_TESTS`. This is its own test binary, with one test, so
//! setting and removing the variable can never reach another test running beside it.

use std::time::Duration;

// (Named another way here so that the gate's speed step, which runs every function that calls
// it by this name, does not count this test as one that holds a budget.)
use specula_core::speed_budget as budget;

#[test]
fn the_budget_follows_the_gate_s_factor() {
    let tight = Duration::from_millis(200);
    let before = std::env::var_os("SPECULA_SPEED_TESTS");
    let cases: [(Option<&str>, Duration); 9] = [
        (None, Duration::from_secs(5)),
        (Some("1"), tight),
        (Some("2.5"), Duration::from_millis(500)),
        (Some(" 3.40 "), Duration::from_millis(680)),
        (Some("0.5"), tight),
        (Some("nonsense"), tight),
        (Some(""), tight),
        (Some("NaN"), tight),
        (Some("1e300"), tight),
    ];
    for (value, want) in cases {
        match value {
            Some(v) => std::env::set_var("SPECULA_SPEED_TESTS", v),
            None => std::env::remove_var("SPECULA_SPEED_TESTS"),
        }
        // (Within a microsecond: `mul_f64` goes through a float.)
        let got = budget(tight);
        assert!(got.as_nanos().abs_diff(want.as_nanos()) <= 1_000, "SPECULA_SPEED_TESTS={value:?}: {got:?}, not {want:?}");
    }
    // A tight bound longer than 5 s keeps its length when the variable is unset.
    std::env::remove_var("SPECULA_SPEED_TESTS");
    assert_eq!(budget(Duration::from_secs(9)), Duration::from_secs(9));
    match before {
        Some(v) => std::env::set_var("SPECULA_SPEED_TESTS", v),
        None => std::env::remove_var("SPECULA_SPEED_TESTS"),
    }
}

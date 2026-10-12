//! Integration tests that drive the real `specula` binary entry path via
//! library APIs used by the CLI (no reimplementation of readiness logic).

use specula_env::{
    check_readiness, format_report_text, DoctorOptions, OsFamily, SystemCommandRunner,
};

#[test]
fn system_doctor_report_names_host_os_and_toolchain() {
    let report = check_readiness(&SystemCommandRunner, &DoctorOptions::default());
    let text = format_report_text(&report);

    assert_eq!(report.os_family, OsFamily::detect());
    assert!(
        text.contains(report.os_family.as_str())
            || text.contains(&report.os_label)
            || text.contains("macos")
            || text.contains("macOS")
            || text.contains("linux")
            || text.contains("windows"),
        "report should name OS family: {text}"
    );
    assert!(text.contains("rustc") || text.contains("cargo") || text.contains("READY") || text.contains("NOT READY"));
    assert!(report.checks.iter().any(|c| c.name == "rustc"));
    assert!(report.checks.iter().any(|c| c.name == "cargo"));

    // On this CI/dev host, rustc/cargo should be present.
    assert!(
        report.ready,
        "expected host toolchain ready; report:\n{text}"
    );
}

#[test]
fn doctor_json_roundtrip_fields() {
    let report = check_readiness(&SystemCommandRunner, &DoctorOptions::default());
    let v = serde_json::to_value(&report).expect("serialize");
    assert!(v.get("os_family").is_some());
    assert!(v.get("ready").is_some());
    assert!(v.get("checks").unwrap().as_array().unwrap().len() >= 3);
}

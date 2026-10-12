//! Drive the shipped capture adapter against the real repo PNG fixture.

use specula_brp::{validate_png_header, CapturedImage};
use std::path::PathBuf;

#[test]
fn adapter_loads_repo_fixture_png() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/1x1.png")
        .canonicalize()
        .expect("fixtures/1x1.png");
    let img = CapturedImage::from_png_path(&fixture).expect("from_png_path");
    assert_eq!(img.mime_type, "image/png");
    assert!(img.byte_len > 0);
    assert_eq!(img.width_hint, Some(1));
    assert_eq!(img.height_hint, Some(1));
    let raw = std::fs::read(&fixture).unwrap();
    validate_png_header(&raw).unwrap();
    let blocks = img.to_mcp_content_blocks();
    assert_eq!(blocks[0]["type"], "image");
    assert!(!blocks[0]["data"].as_str().unwrap().is_empty());
    let text = blocks[1]["text"].as_str().expect("text block");
    assert!(text.contains("abs_path="), "capture text must include abs_path: {text}");
    assert!(text.contains("bytes="), "capture text must include size: {text}");
    assert!(
        text.contains(&img.byte_len.to_string()),
        "byte_len must appear in text"
    );
    // Fixture was canonicalize()'d — abs_path should match real disk location.
    let abs = img.absolute_path_display();
    assert!(
        abs.starts_with('/') || abs.contains(':'),
        "abs_path should be absolute: {abs}"
    );
}

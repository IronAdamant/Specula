//! The visual history gallery (`docs/gallery/`, written by `scripts/gallery.py`) stays small and
//! consistent: only README.md and entry folders at the top, every folder an entry with a valid
//! `entry.json` that lists exactly the images beside it, every image decodes fully and is thumbnail
//! sized, the size budgets hold (the repo is public, so its history must not fill up with full-size
//! captures), and the generated README links every entry.
//!
//! The rules and budgets match `cmd_check` and `IMAGE_BUDGET`, `ENTRY_BUDGET`, `TOTAL_BUDGET`,
//! `MAX_THUMB_HEIGHT` in `scripts/gallery.py`; every file under docs/gallery counts toward the total.

use specula_core::json::Json;
use std::path::{Path, PathBuf};

const IMAGE_BUDGET: u64 = 200 * 1024;
const ENTRY_BUDGET: u64 = 900 * 1024;
const TOTAL_BUDGET: u64 = 24 * 1024 * 1024;
const MAX_WIDTH: u32 = 1024;
const MAX_HEIGHT: u32 = 1600;

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gallery")
}

fn text<'a>(entry: &'a Json, key: &str, dir: &str) -> &'a str {
    entry.get(key).and_then(Json::as_str).unwrap_or_else(|| panic!("{dir}/entry.json: `{key}` must be a string"))
}

/// Exactly `YYYY-MM-DD` with a month 01-12 and a day 01-31 (as `valid_date` in gallery.py, which also
/// rejects impossible days such as 02-30).
fn date_ok(date: &str) -> bool {
    let b = date.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    let (month, day): (u32, u32) = (date[5..7].parse().unwrap(), date[8..10].parse().unwrap());
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// `<YYYY-MM-DD>-<slug>` with a slug of lowercase letters, digits and single dashes.
fn entry_name_ok(name: &str, date: &str) -> bool {
    let Some(slug) = name.strip_prefix(date).and_then(|s| s.strip_prefix('-')) else { return false };
    date_ok(date)
        && !slug.is_empty()
        && slug.split('-').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

fn tree_bytes(path: &Path) -> u64 {
    let mut total = 0;
    for entry in std::fs::read_dir(path).unwrap().flatten() {
        let p = entry.path();
        total += if p.is_dir() { tree_bytes(&p) } else { p.metadata().unwrap().len() };
    }
    total
}

#[test]
fn gallery_entries_are_consistent_and_within_budget() {
    let root = gallery();
    let Ok(listing) = std::fs::read_dir(&root) else {
        println!("no docs/gallery: nothing to check");
        return;
    };
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap_or_default();
    let mut problems = Vec::new();
    let mut dirs = Vec::new();
    for item in listing.flatten() {
        let (path, name) = (item.path(), item.file_name().to_string_lossy().into_owned());
        if name.starts_with('.') {
            problems.push(format!("{name}: a hidden file or folder (a staging leftover from an interrupted add?)"));
        } else if path.is_dir() {
            if path.join("entry.json").is_file() {
                dirs.push(path);
            } else {
                problems.push(format!("{name}/: a folder without entry.json"));
            }
        } else if name != "README.md" {
            problems.push(format!("{name}: only README.md and entry folders belong at the top of docs/gallery"));
        }
    }
    dirs.sort();
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let raw = std::fs::read_to_string(dir.join("entry.json")).unwrap_or_else(|e| panic!("{name}/entry.json: {e}"));
        let entry = Json::parse(&raw).unwrap_or_else(|e| panic!("{name}/entry.json: {e:?}"));
        let date = text(&entry, "date", &name);
        for key in ["title", "note", "subject", "topic"] {
            if text(&entry, key, &name).is_empty() {
                problems.push(format!("{name}: empty `{key}`"));
            }
        }
        if !entry_name_ok(&name, date) {
            problems.push(format!("{name}: folder must be <date>-<slug> with its own date {date:?}"));
        }
        if !readme.contains(&format!("({name}/entry.json)")) {
            problems.push(format!("{name}: not in docs/gallery/README.md (run python scripts/gallery.py index)"));
        }
        let images = entry.get("images").and_then(Json::as_array).unwrap_or_else(|| panic!("{name}: `images` must be an array"));
        if images.is_empty() {
            problems.push(format!("{name}: no images"));
        }
        let mut listed: Vec<String> = images.iter().map(|im| text(im, "file", &name).to_owned()).collect();
        listed.sort();
        let mut on_disk = Vec::new();
        for item in std::fs::read_dir(dir).unwrap().flatten() {
            let file = item.file_name().to_string_lossy().into_owned();
            if item.path().is_dir() {
                problems.push(format!("{name}/{file}/: entries hold no folders"));
            } else if file != "entry.json" {
                on_disk.push(file);
            }
        }
        on_disk.sort();
        if listed != on_disk {
            problems.push(format!("{name}: images listed {listed:?} but on disk {on_disk:?}"));
        }
        for file in &on_disk {
            let bytes = std::fs::read(dir.join(file)).unwrap();
            if bytes.len() as u64 > IMAGE_BUDGET {
                problems.push(format!("{name}/{file}: {} bytes, budget {IMAGE_BUDGET}", bytes.len()));
            }
            let decoded = if file.ends_with(".png") {
                specula_core::png::decode(&bytes).map_err(|e| e.to_string())
            } else if file.ends_with(".jpg") {
                specula_core::jpeg::decode(&bytes).map_err(|e| e.to_string())
            } else {
                Err("not a .jpg or .png".to_owned())
            };
            match decoded {
                Ok(img) if img.width > MAX_WIDTH || img.height > MAX_HEIGHT => {
                    problems.push(format!("{name}/{file}: {}x{} is not a thumbnail (max {MAX_WIDTH}x{MAX_HEIGHT})", img.width, img.height))
                }
                Ok(_) => {}
                Err(e) => problems.push(format!("{name}/{file}: doesn't decode: {e}")),
            }
        }
        let size = tree_bytes(dir);
        if size > ENTRY_BUDGET {
            problems.push(format!("{name}: {size} bytes, budget {ENTRY_BUDGET}"));
        }
    }
    let total = tree_bytes(&root);
    if total > TOTAL_BUDGET {
        problems.push(format!("gallery: {total} bytes, budget {TOTAL_BUDGET}"));
    }
    println!("{} entries, {total} bytes", dirs.len());
    assert!(problems.is_empty(), "gallery problems:\n{}", problems.join("\n"));
}

#[test]
fn entry_names_are_checked() {
    assert!(entry_name_ok("2026-10-18-sepulcra-now", "2026-10-18"));
    assert!(!entry_name_ok("2026-10-18-sepulcra-now", "2026-10-17"));
    assert!(!entry_name_ok("2026-10-18-x--y", "2026-10-18"));
    assert!(!entry_name_ok("2026-10-18-Up", "2026-10-18"));
    assert!(!entry_name_ok("2026-10-18-", "2026-10-18"));
    assert!(!entry_name_ok("20261018-x", "20261018"));
    assert!(!entry_name_ok("2026-13-01-x", "2026-13-01"));
    assert!(!date_ok("2026-1-018"));
}

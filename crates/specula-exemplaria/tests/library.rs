//! The library against its folder, and the same specimens on every machine.
//!
//! **The table of hashes** holds kinds and seeds to the hash of the recipe and anims text they
//! write (roles as named, no style). It was written on Windows; CI's Linux job runs this test, so
//! every push compares the two systems. A change to a plan or a kind that moves a specimen
//! changes this table on purpose, in the same commit, with the reason.

use specula_core::hash::StableHasher;
use specula_exemplaria::{expand, library, AsNamed};
use std::path::Path;

/// Moved on 2026-11-24 (review 1 of S0): every number's stream is seeded by
/// `stream_seed(seed, "<kind>/<number>")` (A.2's scheme) instead of `Rng::named`, whose streams
/// of one seed were related; and the kind file lost its `far` level (not built: refused now), so
/// its hash in every description moved. Nothing had shipped.
const TABLE: &[(&str, Option<u64>, u64)] = &[
    ("box_creature", None, 0x8459ff76aee86804),
    ("box_creature", Some(0), 0xdab6a382dca1e26d),
    ("box_creature", Some(1), 0x5d8b14cd05786ee3),
    ("box_creature", Some(7), 0x73840d4d41b98e35),
    ("box_creature", Some(42), 0x60ce7c934a224e82),
    ("box_creature", Some(123_456_789), 0x0d5efea26d1fb4d2),
    ("box_creature", Some(9_007_199_254_740_991), 0x93eb2b75420c98e7),
];

fn written(name: &str, seed: Option<u64>) -> u64 {
    let e = library::entry(name).unwrap();
    let x = expand(&e.resolved().unwrap(), seed, &AsNamed, &e.from()).unwrap();
    let mut h = StableHasher::with_seed(1);
    h.write_str(&x.recipe);
    h.write_str(x.anims.as_deref().unwrap_or(""));
    h.finish()
}

#[test]
fn specimens_are_the_same_bytes_on_every_machine() {
    let mut wrong = Vec::new();
    for (name, seed, want) in TABLE {
        let got = written(name, *seed);
        if got != *want {
            wrong.push(format!("(\"{name}\", {seed:?}, 0x{got:016x}),"));
        }
    }
    assert!(wrong.is_empty(), "specimens moved (the rows as they are now):\n{}", wrong.join("\n"));
}

/// Every file under `exemplaria/`, at any depth, is a kind built into the engine, and every kind
/// built in is a file there with the same bytes: the folder is the library's source and the
/// engine answers from what was built from it (nothing beside the executable is read or can be
/// swapped). A file of any other sort there (a note, a picture, a stray copy) fails, so it
/// cannot be mistaken for part of the library.
#[test]
fn every_file_of_the_folder_is_built_in_and_nothing_built_in_is_missing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../exemplaria");
    let mut on_disk = Vec::new();
    let mut todo = vec![root.clone()];
    while let Some(dir) = todo.pop() {
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            if f.is_dir() {
                todo.push(f);
                continue;
            }
            let rel = f.strip_prefix(&root).unwrap().components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/");
            let text = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("exemplaria/{rel} is not a kind file's text: {e}"));
            on_disk.push((rel, text));
        }
    }
    on_disk.sort();
    for (rel, _) in &on_disk {
        let parts: Vec<&str> = rel.split('/').collect();
        assert!(parts.len() == 2 && rel.ends_with(".kind.json"), "exemplaria/{rel}: the folder holds only <plan>/<name>.kind.json (a kind built into the engine)");
    }
    let mut table: Vec<(String, String)> = library::ENTRIES.iter().map(|e| (e.path.to_string(), e.text.to_string())).collect();
    table.sort();
    let paths = |v: &[(String, String)]| v.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&on_disk), paths(&table), "exemplaria/ and library::ENTRIES list the same kinds");
    for ((p, disk), (_, built)) in on_disk.iter().zip(&table) {
        // (A checkout may hold CRLF; the built text is the file's bytes at build time.)
        assert_eq!(disk.replace("\r\n", "\n"), built.replace("\r\n", "\n"), "{p}");
    }
}

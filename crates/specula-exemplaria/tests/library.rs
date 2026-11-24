//! The library against its folder, and the same specimens on every machine.
//!
//! **The table of hashes** holds kinds and seeds to the hash of the recipe and anims text they
//! write (roles as named, no style). It was written on Windows; CI's Linux job runs this test, so
//! every push compares the two systems. A change to a plan or a kind that moves a specimen
//! changes this table on purpose, in the same commit, with the reason.

use specula_core::hash::StableHasher;
use specula_exemplaria::{expand, library, AsNamed};
use std::path::Path;

/// Moved on 2026-11-25 (review 2 of S0), on purpose: every residual is now drawn from a normal
/// truncated to its window by the inverse of the distribution (one uniform a stream) instead of
/// Box–Muller clipped at 2.5 spreads, so every seed's numbers moved; and the box creature's write
/// changed so that no two parts share a face (shins into thigh and foot, the tail into the trunk,
/// the head behind and above the neck), so the mean's text moved too. Nothing had shipped.
/// (Before: moved on 2026-11-24 for review 1's streams.)
const TABLE: &[(&str, Option<u64>, u64)] = &[
    ("box_creature", None, 0x0a1027edc258978b),
    ("box_creature", Some(0), 0x3f639edfc4ea243e),
    ("box_creature", Some(1), 0x734b8c97fb9ba479),
    ("box_creature", Some(7), 0xbd4923aba48555e1),
    ("box_creature", Some(42), 0x9c62f0dd481a002e),
    ("box_creature", Some(123_456_789), 0x984671fd77be4ee5),
    ("box_creature", Some(9_007_199_254_740_991), 0x23ac80452470e66e),
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

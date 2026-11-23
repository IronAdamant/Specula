//! The library against its folder, and the same specimens on every machine.
//!
//! **The table of hashes** holds kinds and seeds to the hash of the recipe and anims text they
//! write (roles as named, no style). It was written on Windows; CI's Linux job runs this test, so
//! every push compares the two systems. A change to a plan or a kind that moves a specimen
//! changes this table on purpose, in the same commit, with the reason.

use specula_core::hash::StableHasher;
use specula_exemplaria::{expand, library, AsNamed};
use std::path::Path;

const TABLE: &[(&str, Option<u64>, u64)] = &[
    ("box_creature", None, 0xe9b8ff01e1e82ecc),
    ("box_creature", Some(0), 0x43511e09b65a71e8),
    ("box_creature", Some(1), 0x940b679efe571e92),
    ("box_creature", Some(7), 0x461d38e4b1080a1e),
    ("box_creature", Some(42), 0x6e1e2a1e1a1ed3f1),
    ("box_creature", Some(123_456_789), 0x31285e8cb1911802),
    ("box_creature", Some(9_007_199_254_740_991), 0x9fb4b55a581aa034),
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

#[test]
fn every_kind_file_has_its_line_and_every_line_its_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../exemplaria");
    let mut on_disk = Vec::new();
    for plan in std::fs::read_dir(&root).unwrap() {
        let plan = plan.unwrap().path();
        if !plan.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&plan).unwrap() {
            let f = f.unwrap().path();
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".kind.json") {
                let rel = format!("{}/{name}", plan.file_name().unwrap().to_string_lossy());
                let text = std::fs::read_to_string(&f).unwrap();
                on_disk.push((rel, text));
            }
        }
    }
    on_disk.sort();
    let mut table: Vec<(String, String)> = library::ENTRIES.iter().map(|e| (e.path.to_string(), e.text.to_string())).collect();
    table.sort();
    let paths = |v: &[(String, String)]| v.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&on_disk), paths(&table), "exemplaria/ and library::ENTRIES list the same kinds");
    for ((p, disk), (_, built)) in on_disk.iter().zip(&table) {
        // (A checkout may hold CRLF; the built text is the file's bytes at build time.)
        assert_eq!(disk.replace("\r\n", "\n"), built.replace("\r\n", "\n"), "{p}");
    }
}

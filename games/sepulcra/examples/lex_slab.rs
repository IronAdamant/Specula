//! Writes the slab's half of Sepulcra's Lex rule set: `lex/slab_01.lex.json`, the level as Lex
//! items (the slab itself as a `data` item for the movement hooks, and the sites the rules
//! name: build spots, wall spots, spawn points and the flyers' entries), from
//! `data/slab_01.json`. The rules half is `lex/sepulcra.lex.json`, written by hand.
//!
//! ```text
//! cargo run -p sepulcra --example lex_slab            # check the file is up to date
//! cargo run -p sepulcra --example lex_slab -- --write # rewrite it
//! ```
//!
//! `tests/lex_parity.rs` checks the file matches the map, so the two can't drift apart.

use std::path::PathBuf;

fn main() {
    let write = std::env::args().any(|a| a == "--write");
    let text = sepulcra::lex::slab_document_text(sepulcra::content::SLAB_01_JSON).unwrap_or_else(|e| {
        eprintln!("lex_slab: {e}");
        std::process::exit(1);
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lex").join("slab_01.lex.json");
    let current = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    if write {
        std::fs::write(&path, &text).unwrap_or_else(|e| panic!("can't write {}: {e}", path.display()));
        println!("wrote {} ({} bytes)", path.display(), text.len());
    } else if current == text {
        println!("{} is up to date", path.display());
    } else {
        eprintln!("{} is out of date: run with --write", path.display());
        std::process::exit(1);
    }
}

//! The table of every form in docs/design/FORMAE.md is the crate's own: this test makes the
//! table from the crate and holds the document's against it, so the document cannot go stale.
//! When a form is added, `cargo test -p specula-formae --test doc_table -- --ignored` writes the
//! table into the document again (between its two markers; nothing else in it is touched).

use specula_formae::{aliases, forms, Form, Set};

const BEGIN: &str = "<!-- formae-table: begin -->";
const END: &str = "<!-- formae-table: end -->";

fn document() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/FORMAE.md")
}

fn number(x: f64) -> String {
    format!("{x}")
}

/// One form's parameters: `name` unit default (least..greatest).
fn params(f: &Form) -> String {
    if f.params.is_empty() {
        return "none".to_string();
    }
    f.params.iter().map(|p| format!("`{}` {} {} ({}..{})", p.name, p.unit.name(), number(p.default), number(p.min), number(p.max))).collect::<Vec<_>>().join(", ")
}

/// The table, made from the crate: a heading for each kind and family in the crate's order, a
/// row for each form, then the aliases.
fn table() -> String {
    let mut out = String::new();
    let mut last = None;
    for f in forms() {
        let group = (f.kind.name(), f.family.name());
        if last != Some(group) {
            out.push_str(&format!("\n#### {} forms, {}\n\n| Form | What it is | Parameters |\n|---|---|---|\n", group.0, group.1));
            last = Some(group);
        }
        out.push_str(&format!("| `{}` | {} | {} |\n", f.name, f.about.replace('|', "/"), params(f)));
    }
    out.push_str("\n#### Aliases\n\n| Word | Kind | Is | With |\n|---|---|---|---|\n");
    for a in aliases() {
        let set: Vec<String> = a
            .set
            .iter()
            .map(|s| match s {
                Set::Fixed(n, v) => format!("`{n}` = {}", number(*v)),
                Set::Tie { name, targets, .. } => format!("`{name}` sets {}", targets.join(", ")),
            })
            .collect();
        out.push_str(&format!("| `{}` | {} | `{}` | {} |\n", a.name, a.kind.name(), a.of, if set.is_empty() { "-".to_string() } else { set.join(", ") }));
    }
    out
}

fn split(doc: &str) -> (&str, &str, &str) {
    let b = doc.find(BEGIN).expect("FORMAE.md has the table's first marker");
    let e = doc.find(END).expect("FORMAE.md has the table's last marker");
    (&doc[..b + BEGIN.len()], &doc[b + BEGIN.len()..e], &doc[e..])
}

#[test]
fn the_documents_table_is_the_crates() {
    let doc = std::fs::read_to_string(document()).expect("docs/design/FORMAE.md");
    let doc = doc.replace("\r\n", "\n");
    let (_, between, _) = split(&doc);
    let want = table();
    if between.trim() != want.trim() {
        let first = between.trim().lines().zip(want.trim().lines()).find(|(a, b)| a != b);
        panic!("docs/design/FORMAE.md's table is not the crate's (first difference: {first:?}): write it again with `cargo test -p specula-formae --test doc_table -- --ignored`");
    }
    // Every form and alias is in it by name.
    for f in forms() {
        assert!(between.contains(&format!("| `{}` |", f.name)), "{}", f.name);
    }
}

#[test]
#[ignore = "writes the table into docs/design/FORMAE.md (run when a form is added)"]
fn write_the_table() {
    let path = document();
    let doc = std::fs::read_to_string(&path).expect("docs/design/FORMAE.md").replace("\r\n", "\n");
    let (head, _, tail) = split(&doc);
    std::fs::write(&path, format!("{head}\n{}\n{tail}", table().trim())).expect("written");
}

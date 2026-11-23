//! One index of names across the lists an agent meets (docs/design/EXEMPLARIA.md, A.9): the
//! shape library (`formae_list`), the library of sample assets (`exemplar_list`) and, when it
//! is plugged in, what the project holds (its own catalogue). A name that no list holds is
//! answered with the nearest names **across all of them**, each with the call that shows it, so
//! an agent that asked the wrong list is one request from the right one; a word that names
//! things in two lists (`egg`, a shape and a kind) is answered with both.
//!
//! **A list plugs in** as a [`Source`]: what it calls itself, how many it holds, and its names
//! with a line each. [`sources`] has the two that are built; the project's catalogue is the third,
//! added there when it exists (its names come from the engine thread's store, so it is handed
//! in by the caller rather than read here).
//!
//! Pure, on the caller's thread: a word is cut to `specula_exemplaria::words::MAX_WORD` bytes
//! before it is measured, and each list's names are a few hundred.

use specula_core::Json;
use specula_exemplaria::words;

/// The most nearest names an answer gives.
pub const MAX_NEAREST: usize = 6;

/// A name in a list.
#[derive(Clone, Debug, PartialEq)]
pub struct Name {
    pub name: String,
    /// What it is: `shape`, `kind`, `project file`.
    pub what: &'static str,
    /// The call that shows it (`exemplar_get {"name": "box_creature"}`).
    pub call: String,
}

impl Name {
    pub fn to_json(&self) -> Json {
        Json::obj([("name", Json::from(self.name.as_str())), ("is", Json::from(self.what)), ("see", Json::from(self.call.as_str()))])
    }
}

/// A list of names an agent can be pointed at.
pub trait Source {
    /// The tool that lists it (`formae_list`).
    fn list(&self) -> &'static str;
    /// What its entries are, plural (`shapes`).
    fn plural(&self) -> &'static str;
    /// How many entries it holds (aliases not counted).
    fn count(&self) -> usize;
    /// Every name and alias it holds.
    fn names(&self) -> Vec<Name>;
}

/// The shape library: its forms and their aliases.
pub struct Shapes;

impl Source for Shapes {
    fn list(&self) -> &'static str {
        "formae_list"
    }
    fn plural(&self) -> &'static str {
        "shapes"
    }
    fn count(&self) -> usize {
        specula_formae::forms().len()
    }
    fn names(&self) -> Vec<Name> {
        let get = |n: &str| format!("formae_get {{\"name\": \"{n}\"}}");
        let mut out: Vec<Name> = specula_formae::forms().iter().map(|f| Name { name: f.name.to_string(), what: "shape", call: get(f.name) }).collect();
        out.extend(specula_formae::aliases().iter().map(|a| Name { name: a.name.to_string(), what: "shape", call: get(a.name) }));
        out
    }
}

/// The library of sample assets: its kinds and their aliases.
pub struct Kinds;

impl Source for Kinds {
    fn list(&self) -> &'static str {
        "exemplar_list"
    }
    fn plural(&self) -> &'static str {
        "kinds"
    }
    fn count(&self) -> usize {
        specula_exemplaria::library::ENTRIES.len()
    }
    fn names(&self) -> Vec<Name> {
        specula_exemplaria::library::names().into_iter().map(|(n, _)| Name { call: format!("exemplar_get {{\"name\": \"{n}\"}}"), name: n, what: "kind" }).collect()
    }
}

/// The lists built so far. The project's own catalogue joins them as a third [`Source`].
pub fn sources() -> [&'static dyn Source; 2] {
    [&Shapes, &Kinds]
}

/// Every entry of `sources` that `word` names exactly (a name or an alias).
#[cfg_attr(not(test), allow(dead_code, reason = "the project catalogue will ask for exact names when it plugs in"))]
pub fn exact(word: &str, sources: &[&dyn Source]) -> Vec<Name> {
    sources.iter().flat_map(|s| s.names()).filter(|n| n.name == word).collect()
}

/// Up to `n` names of `sources` nearest to `word`, nearest first; a name two lists hold is
/// given once for each.
pub fn nearest(word: &str, sources: &[&dyn Source], n: usize) -> Vec<Name> {
    let mut scored: Vec<(usize, Name)> = sources.iter().flat_map(|s| s.names()).filter_map(|x| words::nearness(word, &x.name).map(|d| (d, x))).collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)).then_with(|| a.1.what.cmp(b.1.what)));
    scored.dedup_by(|a, b| a.1.name == b.1.name && a.1.what == b.1.what);
    scored.into_iter().take(n).map(|(_, x)| x).collect()
}

/// The answer to a name no list holds: the nearest across every list, with their calls, or
/// where to look when nothing is near.
pub fn not_found(word: &str, sources: &[&dyn Source]) -> Json {
    let near = nearest(word, sources, MAX_NEAREST);
    let mut o = vec![("nearest", Json::arr(near.iter().map(Name::to_json)))];
    if near.is_empty() {
        o.push(("look", Json::from(also(sources, ""))));
    }
    Json::obj(o)
}

/// "also: 159 shapes (formae_list), 1 kinds (exemplar_list)": the lists other than `except`, for
/// the first line of each list's first answer.
pub fn also(sources: &[&dyn Source], except: &str) -> String {
    let lines: Vec<String> = sources.iter().filter(|s| s.list() != except).map(|s| format!("{} {} ({})", s.count(), s.plural(), s.list())).collect();
    lines.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_name_finds_the_nearest_in_every_list() {
        let all = sources();
        let n = nearest("box_creatur", &all, MAX_NEAREST);
        assert_eq!(n.first().map(|x| (x.name.as_str(), x.what)), Some(("box_creature", "kind")), "{n:?}");
        let n = nearest("gearr", &all, MAX_NEAREST);
        assert!(n.iter().any(|x| x.name == "gear" && x.what == "shape" && x.call.contains("formae_get")), "{n:?}");
        // A word that both lists know answers with both (a test list stands in for a kind).
        struct Eggs;
        impl Source for Eggs {
            fn list(&self) -> &'static str {
                "project_catalog"
            }
            fn plural(&self) -> &'static str {
                "project files"
            }
            fn count(&self) -> usize {
                1
            }
            fn names(&self) -> Vec<Name> {
                vec![Name { name: "egg".into(), what: "project file", call: "project_catalog {\"word\": \"egg\"}".into() }]
            }
        }
        let three: [&dyn Source; 3] = [&Shapes, &Kinds, &Eggs];
        let e = exact("egg", &three);
        assert!(e.iter().any(|x| x.what == "shape") && e.iter().any(|x| x.what == "project file"), "{e:?}");
        assert!(also(&three, "exemplar_list").contains("1 project files (project_catalog)"));
        assert!(not_found("qqqqqqqq", &all).get("look").is_some());
        // A long word costs what a short one does.
        assert!(nearest(&"z".repeat(1 << 20), &all, MAX_NEAREST).is_empty());
    }
}

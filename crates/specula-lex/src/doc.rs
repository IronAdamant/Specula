//! The text form: a rule set is one JSON document of items keyed by id.
//!
//! ```text
//! { "lex": 1, "name": "slice", "tick_hz": 30, "notes": "...",
//!   "items": { "dust": { "kind": "resource", "start": 150 }, ... } }
//! ```
//!
//! [`Document::to_json`] is canonical: items sorted by id and every object's keys sorted, so the
//! text diffs cleanly and [`Document::hash`] ignores layout and key order. `notes` (anywhere)
//! never changes the hash.

use std::collections::BTreeMap;

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::diag::{codes, did_you_mean, show, type_name, Diagnostic};

/// The newest document format this crate reads and writes.
pub const FORMAT_VERSION: u32 = 1;
/// The simulation rate when a document doesn't say.
pub const DEFAULT_TICK_HZ: u32 = 60;
/// Longest item id.
pub const MAX_ID_CHARS: usize = 64;

const HEADER_KEYS: [&str; 5] = ["lex", "name", "tick_hz", "notes", "items"];

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub version: u32,
    pub name: String,
    pub tick_hz: u32,
    pub notes: Option<String>,
    /// Item id → the item as written (an object with a `kind`).
    pub items: BTreeMap<String, Json>,
}

impl Document {
    pub fn new(name: impl Into<String>, tick_hz: u32) -> Document {
        Document { version: FORMAT_VERSION, name: name.into(), tick_hz, notes: None, items: BTreeMap::new() }
    }

    /// Parses document text. Syntax errors carry the byte offset.
    pub fn parse(text: &str) -> Result<Document, Vec<Diagnostic>> {
        let json = Json::parse(text).map_err(|e| {
            vec![Diagnostic::error(codes::SYNTAX, "", "", format!("not valid JSON at byte {}: {}", e.offset, e.message))
                .suggest("fix the JSON syntax there (a missing comma, quote or bracket is the usual cause)")]
        })?;
        Document::from_json(&json)
    }

    /// Reads the header and the items. Item contents are checked later by the compiler, so a
    /// document with broken items still loads (and can be fixed through the tools).
    pub fn from_json(json: &Json) -> Result<Document, Vec<Diagnostic>> {
        let mut errs = Vec::new();
        let Some(pairs) = json.as_object() else {
            return Err(vec![Diagnostic::error(codes::WRONG_TYPE, "", "", "a Lex document must be a JSON object")
                .expected("{\"lex\": 1, \"name\": ..., \"items\": {...}}")
                .got(type_name(json))]);
        };
        for (k, _) in pairs {
            if !HEADER_KEYS.contains(&k.as_str()) {
                errs.push(
                    Diagnostic::error(codes::UNKNOWN_FIELD, "", k.as_str(), format!("'{k}' is not a document field"))
                        .suggest_opt(did_you_mean(k, HEADER_KEYS)),
                );
            }
        }
        let version = match json.get("lex") {
            None => {
                errs.push(Diagnostic::error(codes::MISSING_FIELD, "", "lex", "the format version is missing").suggest(format!("add \"lex\": {FORMAT_VERSION}")));
                FORMAT_VERSION
            }
            Some(v) => match v.as_u64() {
                Some(n) if n >= 1 && n <= u64::from(FORMAT_VERSION) => n as u32,
                Some(n) if n > u64::from(FORMAT_VERSION) => {
                    errs.push(
                        Diagnostic::error(codes::UNSUPPORTED_VERSION, "", "lex", format!("format version {n} is newer than this engine reads ({FORMAT_VERSION})"))
                            .suggest("update the engine, or save the document from an engine that writes this version"),
                    );
                    FORMAT_VERSION
                }
                _ => {
                    errs.push(Diagnostic::error(codes::WRONG_TYPE, "", "lex", "the format version must be a positive whole number").got(show(v)).suggest(format!("\"lex\": {FORMAT_VERSION}")));
                    FORMAT_VERSION
                }
            },
        };
        let name = match json.get("name") {
            None => String::new(),
            Some(Json::Str(s)) => s.clone(),
            Some(v) => {
                errs.push(Diagnostic::error(codes::WRONG_TYPE, "", "name", "the name must be a string").got(show(v)));
                String::new()
            }
        };
        let tick_hz = match json.get("tick_hz") {
            None => DEFAULT_TICK_HZ,
            Some(v) => match v.as_u64() {
                Some(n) if (1..=1000).contains(&n) => n as u32,
                _ => {
                    errs.push(
                        Diagnostic::error(codes::OUT_OF_RANGE, "", "tick_hz", "tick_hz must be a whole number from 1 to 1000")
                            .got(show(v))
                            .suggest(format!("{DEFAULT_TICK_HZ} (the default) or 30")),
                    );
                    DEFAULT_TICK_HZ
                }
            },
        };
        let notes = match json.get("notes") {
            None => None,
            Some(Json::Str(s)) => Some(s.clone()),
            Some(v) => {
                errs.push(Diagnostic::error(codes::WRONG_TYPE, "", "notes", "notes must be a string").got(show(v)));
                None
            }
        };
        let mut items = BTreeMap::new();
        match json.get("items") {
            None => errs.push(Diagnostic::error(codes::MISSING_FIELD, "", "items", "the document has no items").suggest("add \"items\": {} and put items in it, keyed by id")),
            Some(Json::Obj(pairs)) => {
                for (id, item) in pairs {
                    if let Err(d) = check_id(id) {
                        errs.push(d);
                        continue;
                    }
                    if items.insert(id.clone(), item.clone()).is_some() {
                        errs.push(Diagnostic::error(codes::DUPLICATE, id.as_str(), "", format!("item '{id}' is defined twice")).suggest("rename or remove one of them"));
                    }
                }
            }
            Some(v) => errs.push(Diagnostic::error(codes::WRONG_TYPE, "", "items", "items must be an object keyed by item id").got(type_name(v))),
        }
        if errs.is_empty() {
            Ok(Document { version, name, tick_hz, notes, items })
        } else {
            Err(errs)
        }
    }

    /// The header fields without the items.
    pub fn header_json(&self) -> Json {
        let mut pairs = vec![("lex", Json::from(self.version)), ("name", Json::from(self.name.as_str())), ("tick_hz", Json::from(self.tick_hz))];
        if let Some(n) = &self.notes {
            pairs.push(("notes", Json::from(n.as_str())));
        }
        Json::obj(pairs)
    }

    /// Canonical JSON: header keys in a fixed order, items by id, object keys sorted.
    pub fn to_json(&self) -> Json {
        let mut j = self.header_json();
        j.set("items", Json::Obj(self.items.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()));
        j
    }

    /// A hash of the rules: canonical, without any `notes` and without the name.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x1e8);
        h.write_u32(self.version);
        h.write_u32(self.tick_hz);
        for (id, item) in &self.items {
            h.write_str(id);
            h.write_str(&canonical_without_notes(item).to_string());
        }
        h.finish()
    }
}

/// Item ids: `[a-z][a-z0-9_]*`, at most [`MAX_ID_CHARS`].
pub fn check_id(id: &str) -> Result<(), Diagnostic> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id.as_bytes()[0].is_ascii_lowercase()
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        return Ok(());
    }
    let fixed: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_start_matches(|c: char| !c.is_ascii_lowercase())
        .chars()
        .take(MAX_ID_CHARS)
        .collect();
    Err(Diagnostic::error(codes::INVALID, id, "", format!("'{id}' is not a valid item id"))
        .expected("lowercase letters, digits and '_', starting with a letter, at most 64 characters")
        .got(id)
        .suggest_opt((!fixed.is_empty()).then(|| format!("use '{fixed}'"))))
}

/// `v` with every object's keys sorted (recursively).
pub fn canonical(v: &Json) -> Json {
    match v {
        Json::Obj(pairs) => {
            let mut sorted: Vec<(String, Json)> = pairs.iter().map(|(k, v)| (k.clone(), canonical(v))).collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(sorted)
        }
        Json::Arr(items) => Json::Arr(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// Objects whose keys are names the author chose (an attribute or a site may be called
/// `notes`), so a `notes` key inside them is data, not a note.
const NAMED_MAPS: [&str; 10] = ["attributes", "points", "states", "args", "scale", "with", "cost", "items", "set", "value"];

fn canonical_without_notes(v: &Json) -> Json {
    strip_notes(v, false)
}

fn strip_notes(v: &Json, named: bool) -> Json {
    match v {
        Json::Obj(pairs) => {
            let mut sorted: Vec<(String, Json)> = pairs
                .iter()
                .filter(|(k, v)| named || k != "notes" || !matches!(v, Json::Str(_)))
                .map(|(k, v)| (k.clone(), strip_notes(v, !named && NAMED_MAPS.contains(&k.as_str()))))
                .collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(sorted)
        }
        Json::Arr(items) => Json::Arr(items.iter().map(|i| strip_notes(i, false)).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_canonical_form() {
        let d = Document::parse(r#"{"lex":1,"name":"t","tick_hz":30,"items":{"b":{"kind":"resource","start":1},"a":{"start":2,"kind":"resource"}}}"#).unwrap();
        assert_eq!(d.tick_hz, 30);
        assert_eq!(d.items.keys().collect::<Vec<_>>(), ["a", "b"]);
        let text = d.to_json().to_string();
        assert!(text.contains(r#""items":{"a":{"kind":"resource","start":2},"b":"#), "{text}");
        let again = Document::parse(&text).unwrap();
        assert_eq!(again.to_json().to_string(), text, "the canonical form round-trips");
        assert_eq!(again.hash(), d.hash());
    }

    #[test]
    fn hash_ignores_notes_layout_and_key_order() {
        let a = Document::parse(r#"{"lex":1,"items":{"a":{"kind":"resource","start":2}}}"#).unwrap();
        let b = Document::parse(r#"{ "lex": 1, "name": "other", "notes": "x", "items": { "a": { "start": 2, "kind": "resource", "notes": "hi" } } }"#).unwrap();
        assert_eq!(a.hash(), b.hash());
        let c = Document::parse(r#"{"lex":1,"items":{"a":{"kind":"resource","start":3}}}"#).unwrap();
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn header_errors_say_what_to_change() {
        let errs = Document::parse(r#"{"lex":9,"itemz":{},"tick_hz":0}"#).unwrap_err();
        let codes: Vec<&str> = errs.iter().map(|e| e.code).collect();
        assert!(codes.contains(&"unknown_field") && codes.contains(&"unsupported_version") && codes.contains(&"out_of_range"), "{codes:?}");
        assert!(errs.iter().any(|e| e.suggestion.as_deref() == Some("did you mean 'items'?")));
        let errs = Document::parse(r#"{"lex":1,"items":{"Bad Id":{}}}"#).unwrap_err();
        assert_eq!(errs[0].suggestion.as_deref(), Some("use 'bad_id'"));
        let errs = Document::parse(r#"{"lex":1,"items":{"a":{},"a":{}}}"#).unwrap_err();
        assert_eq!(errs[0].code, "duplicate");
        let errs = Document::parse("{\"lex\":1,").unwrap_err();
        assert_eq!(errs[0].code, "syntax");
    }
}

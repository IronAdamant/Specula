//! String tables (`specula.strings.v1`): one file per locale, keys to texts, parsed with
//! structured errors and bounded everywhere.
//!
//! ```text
//! {
//!   "format": "specula.strings.v1",
//!   "locale": "de",
//!   "name": "Deutsch",                  // the language's own name, for menus
//!   "status": "unreviewed",             // the default for plain entries: unreviewed | reviewed
//!   "number": {"decimal": ","},         // optional: overrides the built-in number format
//!   "plural": {"one": "i = 1 and v = 0"}, // optional: overrides the built-in plural rules
//!   "fonts": {"ui": "system:ja"},       // optional: a face per font slot for this locale
//!   "strings": {
//!     "hud.wave": "Welle {wave}/{waves}",
//!     "hud.enemies": {"plural": "n", "one": "{n} Feind", "other": "{n} Feinde"},
//!     "hud.menu": {"text": "Menü", "status": "reviewed", "src": "9c1f04a2b37de810",
//!                  "notes": "the pause button"}
//!   }
//! }
//! ```
//!
//! The source table (English) says `"source": true`; its entries' status is `source`. A
//! translated entry may carry `src`, the hash of the source text it was translated from
//! ([`source_hash`]): when the source changes, checks call the translation stale.

use crate::error::{echo, key_path, LinguaError};
use crate::message::placeholders;
use crate::number::NumberFormat;
use crate::plural::{Category, PluralRules};
use crate::Locale;
use specula_core::hash::StableHasher;
use specula_core::Json;
use std::collections::BTreeMap;

pub const FORMAT: &str = "specula.strings.v1";
/// Largest table file (the project file limit).
pub const MAX_TABLE_BYTES: usize = 1 << 20;
/// Most keys in one table.
pub const MAX_KEYS: usize = 4096;
/// Longest key.
pub const MAX_KEY_BYTES: usize = 96;
/// Longest text (one plural form counts alone).
pub const MAX_TEXT_BYTES: usize = 2048;
/// Longest translator's note.
pub const MAX_NOTES_BYTES: usize = 512;
/// Longest display name.
pub const MAX_NAME_BYTES: usize = 64;
/// Most font slots a table names.
pub const MAX_FONTS: usize = 8;

/// Who stands behind an entry's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// The source table's own words.
    Source,
    /// Translated (by an agent or a human) and not yet reviewed.
    Unreviewed,
    /// A reviewer approved it.
    Reviewed,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Source => "source",
            Status::Unreviewed => "unreviewed",
            Status::Reviewed => "reviewed",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "unreviewed" => Some(Status::Unreviewed),
            "reviewed" => Some(Status::Reviewed),
            _ => None,
        }
    }
}

/// An entry's words: one text, or plural forms picked by a placeholder's number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    One(String),
    Plural {
        /// The placeholder whose number picks the form.
        count: String,
        /// Indexed by [`Category::index`]; `other` is always there.
        forms: [Option<String>; 6],
    },
}

impl Text {
    /// Every text it holds (one, or each form present), with its category.
    pub fn texts(&self) -> Vec<(Option<Category>, &str)> {
        match self {
            Text::One(t) => vec![(None, t.as_str())],
            Text::Plural { forms, .. } => Category::ALL.iter().zip(forms).filter_map(|(c, f)| f.as_deref().map(|t| (Some(*c), t))).collect(),
        }
    }

    /// The text for category `c` (`other` when that form is missing).
    pub fn form(&self, c: Category) -> &str {
        match self {
            Text::One(t) => t,
            Text::Plural { forms, .. } => forms[c.index()].as_deref().or(forms[Category::Other.index()].as_deref()).unwrap_or(""),
        }
    }

    /// Bytes of text it holds.
    pub fn bytes(&self) -> usize {
        self.texts().iter().map(|(_, t)| t.len()).sum()
    }

    pub fn to_json(&self) -> Json {
        match self {
            Text::One(t) => Json::from(t.as_str()),
            Text::Plural { count, forms } => {
                let mut f = vec![("plural".to_owned(), Json::from(count.as_str()))];
                for (c, t) in Category::ALL.iter().zip(forms) {
                    if let Some(t) = t {
                        f.push((c.name().to_owned(), Json::from(t.as_str())));
                    }
                }
                Json::Obj(f)
            }
        }
    }
}

/// One key's entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub text: Text,
    pub status: Status,
    /// The [`source_hash`] of the source text this translates.
    pub src: Option<u64>,
    pub notes: Option<String>,
}

impl Entry {
    /// The entry as a table holds it: a plain string when it is just a text at the table's
    /// default status, else an object.
    pub fn to_json(&self, default: Status) -> Json {
        if self.status == default && self.src.is_none() && self.notes.is_none() {
            if let Text::One(t) = &self.text {
                return Json::from(t.as_str());
            }
        }
        let mut f: Vec<(String, Json)> = match self.text.to_json() {
            Json::Obj(f) => f,
            t => vec![("text".to_owned(), t)],
        };
        if self.status != default && self.status != Status::Source {
            f.push(("status".into(), Json::from(self.status.name())));
        }
        if let Some(h) = self.src {
            f.push(("src".into(), Json::from(hex(h))));
        }
        if let Some(n) = &self.notes {
            f.push(("notes".into(), Json::from(n.as_str())));
        }
        Json::Obj(f)
    }
}

/// A face for a font slot in this locale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSpec {
    /// A face the engine bundles (`ui`, `ui-bold`, `display`).
    Bundled(String),
    /// The system's fallback face for a locale (`system:ja`): CJK faces, read off the engine
    /// thread by whoever loads the table's fonts.
    System(Locale),
}

impl FontSpec {
    pub fn parse(s: &str, field: &str) -> Result<FontSpec, LinguaError> {
        if let Some(l) = s.strip_prefix("system:") {
            return Locale::parse(l).map(FontSpec::System).map_err(|e| LinguaError::new("invalid_value", field, e.message).allowed("system:<locale>, e.g. system:ja"));
        }
        if !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) {
            return Ok(FontSpec::Bundled(s.to_owned()));
        }
        Err(LinguaError::new("invalid_value", field, format!("\"{}\" is not a face", echo(s, 40))).allowed("a bundled face name (ui, ui-bold, display) or system:<locale>"))
    }

    pub fn name(&self) -> String {
        match self {
            FontSpec::Bundled(n) => n.clone(),
            FontSpec::System(l) => format!("system:{l}"),
        }
    }
}

/// A parsed string table.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub locale: Locale,
    /// The language's own name (`Deutsch`).
    pub name: String,
    /// The source table (its entries are the source words).
    pub source: bool,
    /// The status plain entries have.
    pub status: Status,
    /// The table's own plural rules (`None`: the built-in ones for its locale).
    pub plural: Option<PluralRules>,
    /// The number format: built-in for the locale, with the table's overrides.
    pub number: NumberFormat,
    /// Whether the table overrides the number format (`number` in the file).
    pub number_override: bool,
    pub fonts: Vec<(String, FontSpec)>,
    pub entries: BTreeMap<String, Entry>,
    pub notes: Option<String>,
}

/// A key: 1..96 bytes of letters, digits, `_`, `-` and `.`, starting with a letter; dots
/// separate namespaces (`hud.wave`, `stone.warding.name`).
pub fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= MAX_KEY_BYTES
        && k.as_bytes()[0].is_ascii_alphabetic()
        && k.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !k.ends_with('.')
        && !k.contains("..")
}

/// The hash of a source text that translations remember (`src`): stable on every platform.
pub fn source_hash(text: &Text) -> u64 {
    let mut h = StableHasher::with_seed(0x6c69_6e67_7561);
    match text {
        Text::One(t) => {
            h.write_u8(0);
            h.write_str(t);
        }
        Text::Plural { count, forms } => {
            h.write_u8(1);
            h.write_str(count);
            for f in forms {
                match f {
                    Some(t) => {
                        h.write_u8(1);
                        h.write_str(t);
                    }
                    None => h.write_u8(0),
                }
            }
        }
    }
    h.finish()
}

/// 16 lower-case hex digits.
pub fn hex(h: u64) -> String {
    format!("{h:016x}")
}

/// Notes are read by translators and reviewers: line breaks, no other control characters
/// (escape sequences would reach a terminal), nothing [`hidden`].
fn plain_note(s: &str) -> bool {
    !s.chars().any(|c| (c.is_control() && c != '\n') || hidden(c))
}

fn parse_hex(s: &str) -> Option<u64> {
    (s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| u64::from_str_radix(s, 16).ok()).flatten()
}

fn text_field(j: &Json, field: &str) -> Result<String, LinguaError> {
    let s = j.as_str().ok_or_else(|| LinguaError::new("bad_type", field, "a string"))?;
    if s.len() > MAX_TEXT_BYTES {
        return Err(LinguaError::new("too_large", field, format!("a text is at most {MAX_TEXT_BYTES} bytes, this one {}", s.len())));
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(LinguaError::new("invalid_value", field, "control characters other than a line break aren't allowed in a text"));
    }
    if let Some(c) = s.chars().find(|c| hidden(*c)) {
        return Err(hidden_error(field, c));
    }
    placeholders(s, field)?;
    Ok(s.to_owned())
}

/// Characters that show nothing and change what a reader sees, or carry words nobody sees:
/// - the bidirectional embeddings, overrides and isolates (U+202A..202E, U+2066..2069): text can
///   be made to read in another order than it is stored;
/// - the Unicode tag characters (U+E0000..E007F): invisible copies of ASCII, the usual way to
///   hide instructions for an agent that reads the table;
/// - the invisible operators and deprecated format characters (U+2060..2064, U+206A..206F), the
///   byte-order mark (U+FEFF), and the line and paragraph separators (U+2028, U+2029).
///
/// A reviewer approving a translation must be able to see what it says. What real text is typed
/// with is allowed: the zero-width space (U+200B: the word break of Thai, Lao, Khmer and
/// Burmese), the joiners and direction marks of Arabic, Persian, Hebrew and Indic text
/// (U+200C..200F, U+061C), the soft hyphen and emoji's variation selectors.
pub fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{E0000}'..='\u{E007F}' | '\u{2060}'..='\u{2064}' | '\u{206A}'..='\u{206F}' | '\u{FEFF}' | '\u{2028}' | '\u{2029}'
    )
}

fn hidden_error(field: &str, c: char) -> LinguaError {
    LinguaError::new("invalid_value", field, format!("U+{:04X} isn't allowed here: it shows nothing and can change the order or look of the words around it (bidirectional overrides, zero-width spaces)", c as u32))
}

fn unknown(field: &str, key: &str, allowed: &str) -> LinguaError {
    LinguaError::new("unknown_field", key_path(field, key), format!("\"{}\" is not a field here", echo(key, 40))).allowed(allowed.to_owned())
}

impl Table {
    /// Parses a table file's text.
    pub fn parse(text: &str) -> Result<Table, LinguaError> {
        if text.len() > MAX_TABLE_BYTES {
            return Err(LinguaError::new("too_large", "", format!("a string table is at most {MAX_TABLE_BYTES} bytes, this one {}", text.len())));
        }
        let j = Json::parse(text).map_err(|e| LinguaError::new("json", "", e.message).at(e.offset))?;
        Table::from_json(&j)
    }

    pub fn from_json(j: &Json) -> Result<Table, LinguaError> {
        let obj = j.as_object().ok_or_else(|| LinguaError::new("bad_type", "", "a string table is a JSON object"))?;
        let get = |k: &str| obj.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        for (i, (k, _)) in obj.iter().enumerate() {
            if obj[..i].iter().any(|(n, _)| n == k) {
                return Err(LinguaError::new("duplicate", key_path("", k), format!("\"{}\" is given twice", echo(k, 40))));
            }
            if !["format", "locale", "name", "source", "status", "number", "plural", "fonts", "strings", "notes"].contains(&k.as_str()) {
                return Err(unknown("", k, "format, locale, name, source, status, number, plural, fonts, strings, notes"));
            }
        }
        match get("format").and_then(Json::as_str) {
            Some(FORMAT) => {}
            Some(f) => return Err(LinguaError::new("format", "format", format!("expected \"{FORMAT}\", found \"{}\"", echo(f, 40)))),
            None => return Err(LinguaError::new("missing_field", "format", format!("\"format\": \"{FORMAT}\" is required"))),
        }
        let locale = Locale::parse(get("locale").and_then(Json::as_str).ok_or_else(|| LinguaError::new("missing_field", "locale", "\"locale\" is required (en, de, pt-BR...)"))?)?;
        if locale.is_pseudo() {
            return Err(LinguaError::new("invalid_value", "locale", format!("{} is the pseudo-locale: it is made from the source table, never written", crate::locale::PSEUDO)));
        }
        let name = match get("name") {
            None => locale.as_str().to_owned(),
            Some(n) => {
                let s = n.as_str().ok_or_else(|| LinguaError::new("bad_type", "name", "a string"))?;
                if let Some(c) = s.chars().find(|c| hidden(*c)) {
                    return Err(hidden_error("name", c));
                }
                if s.is_empty() || s.len() > MAX_NAME_BYTES || s.chars().any(char::is_control) {
                    return Err(LinguaError::new("invalid_value", "name", format!("the language's own name, 1..{MAX_NAME_BYTES} bytes")));
                }
                s.to_owned()
            }
        };
        let source = match get("source") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(|| LinguaError::new("bad_type", "source", "true or false"))?,
        };
        let status = match (get("status"), source) {
            (None, true) => Status::Source,
            (None, false) => Status::Unreviewed,
            (Some(_), true) => return Err(LinguaError::new("invalid_value", "status", "the source table's words are the source: it takes no status")),
            (Some(v), false) => v.as_str().and_then(Status::parse).ok_or_else(|| LinguaError::new("invalid_value", "status", "unreviewed or reviewed"))?,
        };
        let plural = match get("plural") {
            None => None,
            Some(p) => {
                let o = p.as_object().ok_or_else(|| LinguaError::new("bad_type", "plural", "an object {category: CLDR rule}"))?;
                let pairs: Vec<(&str, &str)> = o
                    .iter()
                    .map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)).ok_or_else(|| LinguaError::new("bad_type", key_path("plural", k), "a CLDR rule string")))
                    .collect::<Result<_, _>>()?;
                Some(PluralRules::parse(&pairs, "plural")?)
            }
        };
        let builtin = NumberFormat::builtin(&locale);
        let (number, number_override) = match get("number") {
            None => (builtin, false),
            Some(n) => (NumberFormat::with_json(&builtin, n, "number")?, true),
        };
        let mut fonts = Vec::new();
        if let Some(f) = get("fonts") {
            let o = f.as_object().ok_or_else(|| LinguaError::new("bad_type", "fonts", "an object {slot: face}"))?;
            if o.len() > MAX_FONTS {
                return Err(LinguaError::new("too_many", "fonts", format!("at most {MAX_FONTS} font slots")));
            }
            for (slot, face) in o {
                let field = key_path("fonts", slot);
                if slot.is_empty() || slot.len() > 32 || !slot.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) {
                    return Err(LinguaError::new("invalid_value", field, "a font slot name: 1..32 of letters, digits, - _"));
                }
                if fonts.iter().any(|(s, _): &(String, FontSpec)| s == slot) {
                    return Err(LinguaError::new("duplicate", field, "this slot is given twice"));
                }
                let spec = FontSpec::parse(face.as_str().ok_or_else(|| LinguaError::new("bad_type", &field, "a face name"))?, &field)?;
                fonts.push((slot.clone(), spec));
            }
        }
        let notes = match get("notes") {
            None => None,
            Some(n) => Some(n.as_str().filter(|s| s.len() <= 4096 && plain_note(s)).ok_or_else(|| LinguaError::new("invalid_value", "notes", "a string of at most 4096 bytes, with no control or hidden characters but line breaks"))?.to_owned()),
        };
        let strings = get("strings").ok_or_else(|| LinguaError::new("missing_field", "strings", "\"strings\" is required: an object of key: text"))?;
        let so = strings.as_object().ok_or_else(|| LinguaError::new("bad_type", "strings", "an object of key: text"))?;
        if so.len() > MAX_KEYS {
            return Err(LinguaError::new("too_many", "strings", format!("at most {MAX_KEYS} keys in one table, this one has {}", so.len())));
        }
        let mut entries = BTreeMap::new();
        for (key, v) in so {
            let field = key_path("strings", key);
            if !valid_key(key) {
                return Err(LinguaError::new("bad_key", &field, format!("\"{}\" is not a key", echo(key, 40))).allowed("1..96 of letters, digits, _ - . starting with a letter, e.g. hud.wave"));
            }
            if entries.contains_key(key) {
                return Err(LinguaError::new("duplicate", &field, "this key is given twice"));
            }
            entries.insert(key.clone(), Table::entry(v, &field, status)?);
        }
        Ok(Table { locale, name, source, status, plural, number, number_override, fonts, entries, notes })
    }

    fn entry(v: &Json, field: &str, default: Status) -> Result<Entry, LinguaError> {
        if v.as_str().is_some() {
            return Ok(Entry { text: Text::One(text_field(v, field)?), status: default, src: None, notes: None });
        }
        let o = v.as_object().ok_or_else(|| LinguaError::new("bad_type", field, "a text, or an object {text | plural + forms, status?, src?, notes?}"))?;
        let mut text = None;
        let mut count = None;
        let mut forms: [Option<String>; 6] = Default::default();
        let mut status = default;
        let mut src = None;
        let mut notes = None;
        for (i, (k, x)) in o.iter().enumerate() {
            let f = key_path(field, k);
            if o[..i].iter().any(|(n, _)| n == k) {
                return Err(LinguaError::new("duplicate", f, "given twice"));
            }
            match k.as_str() {
                "text" => text = Some(text_field(x, &f)?),
                "plural" => {
                    let c = x.as_str().filter(|c| crate::message::valid_name(c)).ok_or_else(|| LinguaError::new("invalid_value", &f, "the name of the placeholder whose number picks the form (n, count, waves...)"))?;
                    count = Some(c.to_owned());
                }
                "status" => {
                    if default == Status::Source {
                        return Err(LinguaError::new("invalid_value", f, "the source table's words are the source: they take no status"));
                    }
                    status = x.as_str().and_then(Status::parse).ok_or_else(|| LinguaError::new("invalid_value", &f, "unreviewed or reviewed"))?;
                }
                "src" => src = Some(x.as_str().and_then(parse_hex).ok_or_else(|| LinguaError::new("invalid_value", &f, "16 hex digits: the source text's hash (l10n_set writes it)"))?),
                "notes" => notes = Some(x.as_str().filter(|n| n.len() <= MAX_NOTES_BYTES && plain_note(n)).ok_or_else(|| LinguaError::new("invalid_value", &f, format!("a string of at most {MAX_NOTES_BYTES} bytes, with no control or hidden characters but line breaks")))?.to_owned()),
                c => match Category::parse(c) {
                    Some(cat) => forms[cat.index()] = Some(text_field(x, &f)?),
                    None => return Err(unknown(field, k, "text, plural, zero, one, two, few, many, other, status, src, notes")),
                },
            }
        }
        let has_forms = forms.iter().any(Option::is_some);
        let text = match (text, count, has_forms) {
            (Some(t), None, false) => Text::One(t),
            (None, Some(count), true) => {
                if forms[Category::Other.index()].is_none() {
                    return Err(LinguaError::new("missing_field", key_path(field, "other"), "plural forms need \"other\" (what every number no other form takes shows)"));
                }
                let total: usize = forms.iter().flatten().map(String::len).sum();
                if total > MAX_TEXT_BYTES * 2 {
                    return Err(LinguaError::new("too_large", field, format!("an entry's forms are at most {} bytes together", MAX_TEXT_BYTES * 2)));
                }
                Text::Plural { count, forms }
            }
            (None, None, true) => return Err(LinguaError::new("missing_field", key_path(field, "plural"), "plural forms need \"plural\": the placeholder whose number picks the form")),
            (None, Some(_), false) => return Err(LinguaError::new("missing_field", key_path(field, "other"), "\"plural\" needs forms: at least \"other\"")),
            (Some(_), _, _) => return Err(LinguaError::new("invalid_value", field, "an entry is a \"text\" or plural forms, not both")),
            (None, None, false) => return Err(LinguaError::new("missing_field", key_path(field, "text"), "an entry needs \"text\" (or plural forms)")),
        };
        Ok(Entry { text, status, src, notes })
    }

    /// Where this table's plural rules come from: `"table"` (its own `plural`), `"builtin"`
    /// (the built-in ones for its locale), or `"none"`: no rules are known for the language, so
    /// every number shows the `other` form.
    pub fn plural_rules_from(&self) -> &'static str {
        if self.plural.is_some() {
            "table"
        } else if PluralRules::builtin(&self.locale).is_some() {
            "builtin"
        } else {
            "none"
        }
    }

    /// The plural rules in force: the table's, else the built-in ones for its locale, else
    /// `other` only.
    pub fn plural_rules(&self) -> PluralRules {
        self.plural.clone().or_else(|| PluralRules::builtin(&self.locale)).unwrap_or_else(PluralRules::other_only)
    }

    /// A new, empty table for `locale` (what `l10n_set` starts a missing locale's file with).
    pub fn empty_json(locale: &Locale, name: &str) -> Json {
        Json::obj([("format", Json::from(FORMAT)), ("locale", Json::from(locale.as_str())), ("name", Json::from(name)), ("status", Json::from("unreviewed")), ("strings", Json::Obj(Vec::new()))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DE: &str = r#"{
        "format": "specula.strings.v1", "locale": "de", "name": "Deutsch",
        "strings": {
            "hud.wave": "Welle {wave}/{waves}",
            "hud.enemies": {"plural": "n", "one": "{n} Feind", "other": "{n} Feinde"},
            "hud.menu": {"text": "Menü", "status": "reviewed", "src": "9c1f04a2b37de810", "notes": "the pause button"}
        }
    }"#;

    #[test]
    fn a_table_parses() {
        let t = Table::parse(DE).unwrap();
        assert_eq!((t.locale.as_str(), t.name.as_str(), t.source, t.status), ("de", "Deutsch", false, Status::Unreviewed));
        assert_eq!(t.entries.len(), 3);
        let e = &t.entries["hud.enemies"];
        assert_eq!(e.text.form(Category::One), "{n} Feind");
        assert_eq!(e.text.form(Category::Few), "{n} Feinde", "a missing form is other");
        let m = &t.entries["hud.menu"];
        assert_eq!((m.status, m.src), (Status::Reviewed, Some(0x9c1f_04a2_b37d_e810)));
        assert_eq!(t.number.decimal, ',');
        assert_eq!(t.plural_rules().categories(), [Category::One, Category::Other]);
        // Entries write back as they were read.
        assert_eq!(m.to_json(t.status).get("status").and_then(Json::as_str), Some("reviewed"));
        assert_eq!(t.entries["hud.wave"].to_json(t.status), Json::from("Welle {wave}/{waves}"));
        let back = Table::entry(&e.to_json(t.status), "x", t.status).unwrap();
        assert_eq!(&back, e);
    }

    /// Review R2 of m5-l10n, minor 7: characters that show nothing and reorder or hide words
    /// (bidirectional overrides and isolates, zero-width spaces) are refused in texts, plural
    /// forms, the language's name and notes; the joiners and marks real Arabic, Persian and
    /// Hebrew text is typed with are not.
    #[test]
    fn hidden_characters_are_refused() {
        let table = |text: &str, name: &str, notes: &str| {
            let j = Json::obj([
                ("format", Json::from(FORMAT)),
                ("locale", Json::from("fa")),
                ("name", Json::from(name)),
                ("strings", Json::obj([("a", Json::obj([("text", Json::from(text)), ("notes", Json::from(notes))]))])),
            ]);
            Table::parse(&j.to_string())
        };
        assert!(table("plain", "Farsi", "a note\nover two lines").is_ok());
        for bad in ['\u{202E}', '\u{202A}', '\u{2066}', '\u{2069}', '\u{E0041}', '\u{E007F}', '\u{2060}', '\u{2062}', '\u{206A}', '\u{FEFF}', '\u{2028}'] {
            let hostile = format!("pay {bad}001$ to");
            let e = table(&hostile, "Farsi", "").unwrap_err();
            assert!(e.code == "invalid_value" && e.to_string().contains(&format!("U+{:04X}", bad as u32)), "{bad:?} in a text: {e}");
            assert!(table("plain", &hostile, "").is_err(), "{bad:?} in the name");
            assert!(table("plain", "Farsi", &hostile).is_err(), "{bad:?} in a note");
        }
        // Escape sequences and NUL in notes would reach a reviewer's terminal.
        assert!(table("plain", "Farsi", "\u{1b}[2Jgone").is_err());
        assert!(table("plain", "Farsi", "a\u{0}b").is_err());
        // What real text is typed with: Thai's word break (a zero-width space), a soft hyphen,
        // an emoji's variation selector.
        assert!(table("\u{e2a}\u{e27}\u{e31}\u{e2a}\u{200B}\u{e14}\u{e35} Schiff\u{AD}fahrt \u{2764}\u{FE0F}", "Thai", "").is_ok());
        // What Persian, Arabic and Hebrew are typed with: a zero-width non-joiner, a joiner, and
        // the direction marks.
        assert!(table("\u{645}\u{6cc}\u{200C}\u{62e}\u{648}\u{627}\u{647}\u{645} \u{200D}\u{200E}\u{200F}\u{61C}", "\u{641}\u{627}\u{631}\u{633}\u{6cc}", "").is_ok());
    }

    #[test]
    fn bad_tables_say_what_and_where() {
        let wrap = |strings: &str| format!(r#"{{"format": "specula.strings.v1", "locale": "de", "strings": {strings}}}"#);
        let cases: Vec<(String, &str, &str)> = vec![
            ("{".into(), "json", ""),
            ("[]".into(), "bad_type", ""),
            (r#"{"format": "v0", "locale": "de", "strings": {}}"#.into(), "format", "format"),
            (r#"{"locale": "de", "strings": {}}"#.into(), "missing_field", "format"),
            (r#"{"format": "specula.strings.v1", "strings": {}}"#.into(), "missing_field", "locale"),
            (r#"{"format": "specula.strings.v1", "locale": "deutsch", "strings": {}}"#.into(), "bad_locale", "locale"),
            (r#"{"format": "specula.strings.v1", "locale": "qps-ploc", "strings": {}}"#.into(), "invalid_value", "locale"),
            (r#"{"format": "specula.strings.v1", "locale": "de", "strings": {}, "extra": 1}"#.into(), "unknown_field", "extra"),
            (r#"{"format": "specula.strings.v1", "locale": "de", "locale": "fr", "strings": {}}"#.into(), "duplicate", "locale"),
            (r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "status": "reviewed", "strings": {}}"#.into(), "invalid_value", "status"),
            (wrap(r#"{"1x": "a"}"#), "bad_key", "strings.1x"),
            (wrap(r#"{"a..b": "a"}"#), "bad_key", "strings.\"a..b\""),
            (wrap(r#"{"a": "{x"}"#), "bad_placeholder", "strings.a"),
            (wrap(r#"{"a": 3}"#), "bad_type", "strings.a"),
            (wrap(r#"{"a": {"one": "x"}}"#), "missing_field", "strings.a.plural"),
            (wrap(r#"{"a": {"plural": "n", "one": "x"}}"#), "missing_field", "strings.a.other"),
            (wrap(r#"{"a": {"text": "x", "other": "y", "plural": "n"}}"#), "invalid_value", "strings.a"),
            (wrap(r#"{"a": {"text": "x", "status": "approved"}}"#), "invalid_value", "strings.a.status"),
            (wrap(r#"{"a": {"text": "x", "src": "12"}}"#), "invalid_value", "strings.a.src"),
            (wrap(r#"{"a": {"text": "x", "colour": 1}}"#), "unknown_field", "strings.a.colour"),
            (wrap(r#"{"a": "x", "a": "y"}"#), "duplicate", "strings.a"),
            (wrap(r#"{"a": "bell\u0007"}"#), "invalid_value", "strings.a"),
            (r#"{"format": "specula.strings.v1", "locale": "de", "plural": {"one": "n ="}, "strings": {}}"#.into(), "bad_rule", "plural.one"),
            (r#"{"format": "specula.strings.v1", "locale": "de", "number": {"decimal": "xx"}, "strings": {}}"#.into(), "invalid_value", "number.decimal"),
            (r#"{"format": "specula.strings.v1", "locale": "de", "fonts": {"ui": "C:/x.ttf"}, "strings": {}}"#.into(), "invalid_value", "fonts.ui"),
        ];
        for (text, code, field) in cases {
            let e = Table::parse(&text).unwrap_err();
            assert_eq!((e.code, e.field.as_str()), (code, field), "{text}: {e}");
        }
        let long = wrap(&format!(r#"{{"a": "{}"}}"#, "x".repeat(MAX_TEXT_BYTES + 1)));
        assert_eq!(Table::parse(&long).unwrap_err().code, "too_large");
        let huge = " ".repeat(MAX_TABLE_BYTES + 1);
        assert_eq!(Table::parse(&huge).unwrap_err().code, "too_large");
        let many: Vec<String> = (0..=MAX_KEYS).map(|i| format!("\"k{i}\": \"x\"")).collect();
        assert_eq!(Table::parse(&wrap(&format!("{{{}}}", many.join(",")))).unwrap_err().code, "too_many");
    }

    #[test]
    fn source_hashes_are_stable_and_tell_texts_apart() {
        let a = source_hash(&Text::One("Wave {wave}".into()));
        assert_eq!(a, source_hash(&Text::One("Wave {wave}".into())));
        assert_ne!(a, source_hash(&Text::One("Wave {wave} ".into())));
        assert_eq!(parse_hex(&hex(a)), Some(a));
        let fonts = Table::parse(r#"{"format": "specula.strings.v1", "locale": "ja", "fonts": {"ui": "system:ja", "title": "display"}, "strings": {}}"#).unwrap();
        assert_eq!(fonts.fonts[0].1, FontSpec::System(Locale::parse("ja").unwrap()));
        assert_eq!(fonts.fonts[1].1.name(), "display");
    }
}

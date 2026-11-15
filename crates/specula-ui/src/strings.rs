//! A UI's string table: the words of one locale (a [`specula_lingua::Catalog`]) parsed into
//! [`Template`]s once, its number format and plural rules, and the faces its font slots use.
//!
//! Screens refer to entries by key (`"text": "@hud.wave"`); games put entries in state with
//! [`crate::StateMap::set_msg`]. [`crate::Ui::set_strings`] switches the language of everything at
//! once; a key the table lacks shows as `@key`, so it is seen (and `ui_check` names it).

use crate::state::{Key, Template};
use specula_lingua::catalog::fmt_of;
use specula_lingua::{Catalog, Category, Fmt, FontSpec, Locale, NumberFormat, PluralRules, Table, Text};
use specula_text::Font;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Faces for font slots: `(slot, face)`.
pub type Faces = Vec<(String, Arc<Font>)>;

/// One key's parsed words.
#[derive(Clone, Debug)]
pub enum UiEntry {
    One(Template),
    Plural {
        /// The state key whose number picks the form.
        count: Key,
        /// How the forms show that number (the `other` form's format): `1` and `1.0` pick
        /// differently.
        fmt: Fmt,
        forms: Box<[Option<Template>; 6]>,
    },
}

impl UiEntry {
    /// Every template it holds.
    pub fn templates(&self) -> Vec<&Template> {
        match self {
            UiEntry::One(t) => vec![t],
            UiEntry::Plural { forms, .. } => forms.iter().flatten().collect(),
        }
    }

    /// The form for category `c` (`other` when missing).
    pub fn form(&self, c: Category) -> Option<&Template> {
        match self {
            UiEntry::One(t) => Some(t),
            UiEntry::Plural { forms, .. } => forms[c.index()].as_ref().or(forms[Category::Other.index()].as_ref()),
        }
    }
}

/// Faces read for locales, by spec name (`system:ja`, `display`): a CJK face is 10-20 MB, read
/// once per process. At most [`MAX_CACHED_FACES`] (the oldest goes).
static FACES: std::sync::Mutex<Vec<(String, Arc<Font>)>> = std::sync::Mutex::new(Vec::new());
const MAX_CACHED_FACES: usize = 6;
/// System faces looked for and not found, by spec name, with why: finding out reads every
/// candidate file whole, so it is done once per process too (a font installed meanwhile, or a
/// file that could not be read just then, is found at the next start). At most
/// [`MAX_CACHED_FACES`].
static MISSES: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

fn lock<T>(m: &'static std::sync::Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Keeps `value` under `name`, once (two callers may both have gone to look: the first one's
/// stays), the oldest entry going when there are more than [`MAX_CACHED_FACES`].
fn keep<T>(cache: &'static std::sync::Mutex<Vec<(String, T)>>, name: &str, value: T) {
    let mut c = lock(cache);
    if !c.iter().any(|(n, _)| n == name) {
        c.push((name.to_owned(), value));
        if c.len() > MAX_CACHED_FACES {
            c.remove(0);
        }
    }
}

/// The face named `name` from the process's cache, or from `load`. A face is kept; the reason
/// there was none is kept where `costly` says that finding out was (a system face; a bundled
/// name that is wrong costs nothing to find wrong again, and a table full of such names must
/// not push the costly answers out).
fn cached_face(name: &str, costly: bool, load: impl FnOnce() -> Result<Font, String>) -> Result<Arc<Font>, String> {
    if let Some(f) = lock(&FACES).iter().find(|(n, _)| n == name).map(|(_, f)| Arc::clone(f)) {
        return Ok(f);
    }
    if let Some(why) = lock(&MISSES).iter().find(|(n, _)| n == name).map(|(_, why)| why.clone()) {
        return Err(why);
    }
    match load() {
        Ok(f) => {
            let f = Arc::new(f);
            keep(&FACES, name, Arc::clone(&f));
            Ok(f)
        }
        Err(why) => {
            if costly {
                keep(&MISSES, name, why.clone());
            }
            Err(why)
        }
    }
}

/// The words a UI shows, in one locale.
pub struct Strings {
    catalog: Catalog,
    entries: BTreeMap<String, UiEntry>,
    /// The source table, for the pseudo-locale ([`Strings::pseudo`]).
    source: Option<Arc<Table>>,
    /// Faces for font slots in this locale (`ui` → a CJK face).
    fonts: Vec<(String, Arc<Font>)>,
    /// What didn't load or parse: `(where, why)`.
    pub problems: Vec<(String, String)>,
    /// The pseudo-locale made from `source`, once ([`Strings::pseudo`]).
    pseudo: std::sync::OnceLock<Option<Arc<Strings>>>,
}

impl std::fmt::Debug for Strings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Strings").field("locale", &self.catalog.locale).field("entries", &self.entries.len()).field("fonts", &self.fonts.iter().map(|(s, _)| s).collect::<Vec<_>>()).finish()
    }
}

impl Strings {
    /// The catalog's texts parsed as templates (a text that doesn't parse shows as it is and is
    /// listed in `problems`), `source` kept for the pseudo-locale, and `fonts` (from
    /// [`Strings::load_fonts`]).
    pub fn new(catalog: Catalog, source: Option<Arc<Table>>, fonts: Vec<(String, Arc<Font>)>) -> Strings {
        let mut entries = BTreeMap::new();
        let mut problems = Vec::new();
        for (key, r) in catalog.entries() {
            let mut parse = |t: &str, form: &str| {
                Template::parse_text(t, key).unwrap_or_else(|e| {
                    problems.push((format!("{key}{form}"), e.message.clone()));
                    Template::literal(t)
                })
            };
            let e = match &r.text {
                Text::One(t) => UiEntry::One(parse(t, "")),
                Text::Plural { count, forms } => {
                    let other = forms[Category::Other.index()].as_deref().unwrap_or("");
                    let fmt = fmt_of(other, count);
                    let mut parsed: [Option<Template>; 6] = Default::default();
                    for (c, f) in Category::ALL.iter().zip(forms) {
                        if let Some(t) = f {
                            parsed[c.index()] = Some(parse(t, &format!(".{}", c.name())));
                        }
                    }
                    UiEntry::Plural { count: Key::parse(count).unwrap_or(Key::Global(count.clone())), fmt, forms: Box::new(parsed) }
                }
            };
            entries.insert(key.to_owned(), e);
        }
        Strings { catalog, entries, source, fonts, problems, pseudo: std::sync::OnceLock::new() }
    }

    /// Strings for `locale` from `tables` (one of them the source), with no faces of their own.
    pub fn build(locale: &Locale, tables: &[Arc<Table>]) -> Result<Strings, specula_lingua::LinguaError> {
        let refs: Vec<&Table> = tables.iter().map(|t| &**t).collect();
        let catalog = Catalog::build(locale, &refs)?;
        let source = tables.iter().find(|t| t.source).cloned();
        Ok(Strings::new(catalog, source, Vec::new()))
    }

    /// The faces `catalog`'s font slots name: bundled ones by name, `system:<locale>` from the
    /// system's fallback faces for that locale. **Reads font files (10-20 MB for CJK): never on
    /// the engine thread.** Returns the faces and, per slot that didn't load, why.
    pub fn load_fonts(catalog: &Catalog) -> (Faces, Vec<(String, String)>) {
        let mut fonts: Vec<(String, Arc<Font>)> = Vec::new();
        let mut problems = Vec::new();
        for (slot, spec) in &catalog.fonts {
            let loaded = cached_face(&spec.name(), matches!(spec, FontSpec::System(_)), || match spec {
                FontSpec::Bundled(n) => match specula_text::Bundled::from_name(n) {
                    Some(b) => b.load().map_err(|e| format!("bundled face {n}: {e}")),
                    None => Err(format!("no bundled face \"{n}\" (ui, ui-bold, display)")),
                },
                // (The locale's own face or a problem said: never the symbol face that ends
                // the fallback chain, which a machine with no CJK font still has. And one that
                // has the interface's digits and Latin words too: a slot's face is the one face
                // for all its text, and a CJK fallback face with no Latin left every digit of
                // the HUD missing.)
                FontSpec::System(l) => specula_text::load_cjk_face_with(l.as_str(), specula_text::INTERFACE_ASCII)
                    .map(|(_, f)| f)
                    .ok_or_else(|| format!("no system face for {l} on this machine that also has the digits and Latin letters an interface shows")),
            });
            match loaded {
                Ok(f) => fonts.push((slot.clone(), f)),
                Err(e) => problems.push((format!("fonts.{slot}"), e)),
            }
        }
        (fonts, problems)
    }

    /// The pseudo-locale made from the source table (`None` without one), with this locale's
    /// faces. Made once and kept: making it reads every key of the table (tens of milliseconds
    /// for a big one), so whoever builds a `Strings` off the engine thread calls
    /// [`Strings::warm_pseudo`] there, and a UI switching to the pseudo-locale only takes the
    /// reference (review R2 of m5-l10n: `ui_check {pseudo}` rebuilt it at every step).
    pub fn pseudo(&self) -> Option<Arc<Strings>> {
        self.pseudo
            .get_or_init(|| {
                let src = self.source.as_ref()?;
                Some(Arc::new(Strings::new(Catalog::pseudo(src), Some(Arc::clone(src)), self.fonts.clone())))
            })
            .clone()
    }

    /// Makes the pseudo-locale now (see [`Strings::pseudo`]): call it where the strings are built.
    pub fn warm_pseudo(&self) {
        let _ = self.pseudo();
    }

    /// Whether the pseudo-locale is already made (a switch to it costs nothing).
    pub fn pseudo_ready(&self) -> bool {
        self.pseudo.get().is_some()
    }

    /// How many font slots have a face read for this locale.
    pub fn fonts_len(&self) -> usize {
        self.fonts.len()
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn locale(&self) -> &Locale {
        &self.catalog.locale
    }

    pub fn number(&self) -> &NumberFormat {
        &self.catalog.number
    }

    pub fn rules(&self) -> &PluralRules {
        &self.catalog.rules
    }

    pub fn entry(&self, key: &str) -> Option<&UiEntry> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The faces for this locale's font slots.
    pub fn fonts(&self) -> &[(String, Arc<Font>)] {
        &self.fonts
    }

    /// `key` filled from a state map, as a label would show it (toasts, window titles, text the
    /// game draws itself). An unknown key writes `@key`.
    pub fn format(&self, key: &str, args: &crate::StateMap, out: &mut String) {
        let scope = crate::state::Scope { state: args, item: None, env: None, strings: Some(self) };
        Template::reference(key).render(&scope, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A review of the interface's system face (2026-11-15): on a machine with no face that will
    /// do, every call looked again, reading every candidate file whole each time. (Nothing else
    /// in this crate's own tests touches these caches.)
    #[test]
    fn a_face_is_looked_for_once_whatever_the_answer() {
        let pid = std::process::id();
        let (hit, miss, cheap) = (format!("test:hit-{pid}"), format!("test:miss-{pid}"), format!("test:cheap-{pid}"));
        let asked = std::cell::Cell::new(0);
        let none = || {
            asked.set(asked.get() + 1);
            Err("no such face".to_owned())
        };
        assert_eq!(cached_face(&miss, true, none).err().as_deref(), Some("no such face"));
        let again = cached_face(&miss, true, || -> Result<Font, String> { panic!("looked for a face it knew was not there") });
        assert_eq!(again.err().as_deref(), Some("no such face"));
        let first = cached_face(&hit, true, || {
            asked.set(asked.get() + 1);
            Ok(crate::testfont::box_font_parsed())
        })
        .unwrap();
        let second = cached_face(&hit, false, || -> Result<Font, String> { panic!("read a face it had") }).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(asked.get(), 2);
        // What costs nothing to find out again is asked again, and takes no place in the cache:
        // as many wrong names as the cache holds leave the costly answer where it was.
        for _ in 0..2 {
            assert!(cached_face(&cheap, false, none).is_err());
        }
        assert_eq!(asked.get(), 4);
        for i in 0..=MAX_CACHED_FACES {
            assert!(cached_face(&format!("test:wrong-{pid}-{i}"), false, || Err("no bundled face".to_owned())).is_err());
        }
        let still = cached_face(&miss, true, || -> Result<Font, String> { panic!("the costly answer was pushed out") });
        assert_eq!(still.err().as_deref(), Some("no such face"));
        // Kept once: a second keeper of the same name (two callers that both went to look)
        // adds nothing and changes nothing.
        keep(&MISSES, &miss, "another caller's answer".to_owned());
        let kept: Vec<String> = lock(&MISSES).iter().filter(|(n, _)| *n == miss).map(|(_, why)| why.clone()).collect();
        assert_eq!(kept, ["no such face"]);
    }
}

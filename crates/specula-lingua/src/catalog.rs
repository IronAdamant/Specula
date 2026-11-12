//! A catalog: the strings one locale shows, resolved through its fallback chain (the locale, its
//! language, then the source table), with the plural rules, number format and fonts in force.
//! Lookups never fail for a source key: a missing translation shows the source's words.

use crate::error::{echo, LinguaError};
use crate::message::{format as fill, pieces, pseudo, Arg, Fmt, Piece};
use crate::number::{auto_ascii, fixed_ascii, percent_ascii, Ascii, NumberFormat};
use crate::plural::{Category, Operands, PluralRules};
use crate::table::{source_hash, FontSpec, Status, Table, Text};
use crate::Locale;
use specula_core::dmath;
use std::collections::BTreeMap;

/// Most tables a catalog is built from (a chain is at most a few long).
pub const MAX_CHAIN: usize = 8;

/// One key's words in a catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub text: Text,
    /// The table it came from (the source's locale when no translation had it).
    pub from: Locale,
    pub status: Status,
    /// The translation was made from other source words than the source has now.
    pub stale: bool,
}

/// The strings of one locale.
#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub locale: Locale,
    /// The language's own name.
    pub name: String,
    pub source: Locale,
    pub rules: PluralRules,
    pub number: NumberFormat,
    /// Faces per font slot (the most specific table naming a slot wins).
    pub fonts: Vec<(String, FontSpec)>,
    entries: BTreeMap<String, Resolved>,
}

impl Catalog {
    /// The catalog for `locale` from `tables` (the source table, marked `source`, and any
    /// translations; tables outside the locale's chain are ignored). The pseudo-locale is made
    /// from the source. A locale with no table of its own (nor its language's) is
    /// `unknown_locale`, naming those there are.
    pub fn build(locale: &Locale, tables: &[&Table]) -> Result<Catalog, LinguaError> {
        let sources: Vec<&&Table> = tables.iter().filter(|t| t.source).collect();
        let source = match sources.as_slice() {
            [s] => **s,
            [] => return Err(LinguaError::new("no_source", "", "no string table says \"source\": true (the source language, English)")),
            _ => {
                let names: Vec<&str> = sources.iter().map(|t| t.locale.as_str()).collect();
                return Err(LinguaError::new("duplicate", "source", format!("more than one source table: {}", names.join(", "))));
            }
        };
        if locale.is_pseudo() {
            return Ok(Catalog::pseudo(source));
        }
        let chain = locale.chain(&source.locale);
        let mut found: Vec<&Table> = Vec::new();
        for l in &chain {
            let mut same = tables.iter().filter(|t| &t.locale == l);
            if let Some(t) = same.next() {
                if same.next().is_some() {
                    return Err(LinguaError::new("duplicate", "locale", format!("two string tables for {l}")));
                }
                found.push(t);
            }
        }
        found.truncate(MAX_CHAIN);
        if found.len() == 1 && *locale != source.locale && locale.language() != source.locale.language() {
            let mut have: Vec<&str> = tables.iter().map(|t| t.locale.as_str()).collect();
            have.sort_unstable();
            // A table for the bare language that the chain doesn't reach is in another script
            // (zh for zh-Hant, sr for sr-Latn): say so, or the refusal reads as a mistake.
            let other_script = tables.iter().any(|t| t.locale.as_str() == locale.language());
            let why = if other_script {
                format!("no string table for {}: the {} table is written in another script, so it isn't shown for it (add a table for {})", echo(locale.as_str(), 40), locale.language(), echo(locale.as_str(), 40))
            } else {
                format!("no string table for {} (nor its language)", echo(locale.as_str(), 40))
            };
            return Err(LinguaError::new("unknown_locale", "locale", why).allowed(format!("{}, {}", have.join(", "), crate::locale::PSEUDO)));
        }
        // Rules, numbers and the name from the most specific table that isn't the source (unless
        // the locale is the source's).
        let own = found[0];
        let rules = own.plural.clone().or_else(|| PluralRules::builtin(locale)).unwrap_or_else(PluralRules::other_only);
        let number = if own.number_override { own.number.clone() } else { NumberFormat::builtin(locale) };
        let mut fonts: Vec<(String, FontSpec)> = Vec::new();
        for t in &found {
            for (slot, f) in &t.fonts {
                if !fonts.iter().any(|(s, _)| s == slot) {
                    fonts.push((slot.clone(), f.clone()));
                }
            }
        }
        let mut entries = BTreeMap::new();
        for (key, src) in &source.entries {
            let hash = source_hash(&src.text);
            let hit = found.iter().find_map(|t| t.entries.get(key).map(|e| (t, e)));
            let r = match hit {
                Some((t, e)) => Resolved { text: e.text.clone(), from: t.locale.clone(), status: e.status, stale: !t.source && e.src.is_some_and(|h| h != hash) },
                None => Resolved { text: src.text.clone(), from: source.locale.clone(), status: Status::Source, stale: false },
            };
            entries.insert(key.clone(), r);
        }
        Ok(Catalog { locale: locale.clone(), name: own.name.clone(), source: source.locale.clone(), rules, number, fonts, entries })
    }

    /// The pseudo-locale made from `source`: every text accented and about 35% longer, in
    /// brackets, placeholders kept; the source's rules, numbers and fonts.
    pub fn pseudo(source: &Table) -> Catalog {
        let mut entries = BTreeMap::new();
        let p = |t: &str| {
            let mut out = String::with_capacity(t.len() * 2);
            pseudo(t, &mut out);
            out
        };
        for (key, e) in &source.entries {
            let text = match &e.text {
                Text::One(t) => Text::One(p(t)),
                Text::Plural { count, forms } => Text::Plural { count: count.clone(), forms: forms.clone().map(|f| f.map(|t| p(&t))) },
            };
            entries.insert(key.clone(), Resolved { text, from: source.locale.clone(), status: Status::Source, stale: false });
        }
        Catalog {
            locale: Locale::parse(crate::locale::PSEUDO).unwrap_or_else(|_| source.locale.clone()),
            name: "Pseudo".into(),
            source: source.locale.clone(),
            rules: source.plural_rules(),
            number: source.number.clone(),
            fonts: source.fonts.clone(),
            entries,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Resolved> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &Resolved)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The text `key` shows for `count` (its plural form; ignored for plain texts).
    pub fn text(&self, key: &str, count: Option<&Operands>) -> Option<&str> {
        let r = self.entries.get(key)?;
        Some(match (&r.text, count) {
            (Text::One(t), _) => t,
            (t @ Text::Plural { .. }, Some(o)) => t.form(self.rules.select(o)),
            (t @ Text::Plural { .. }, None) => t.form(Category::Other),
        })
    }

    /// `key` filled from `args` into `out` (cleared first): the plural form picked by the count
    /// placeholder's value as it is shown. An unknown key writes `key` itself, so it shows.
    pub fn format(&self, key: &str, args: &[(&str, Arg<'_>)], out: &mut String) {
        let Some(r) = self.entries.get(key) else {
            out.clear();
            out.push_str(key);
            return;
        };
        let text = match &r.text {
            Text::One(t) => t.as_str(),
            Text::Plural { count, forms } => {
                let other = forms[Category::Other.index()].as_deref().unwrap_or("");
                let ops = args.iter().find(|(n, _)| n == count).and_then(|(_, v)| operands(*v, fmt_of(other, count)));
                r.text.form(ops.map_or(Category::Other, |o| self.rules.select(&o)))
            }
        };
        fill(text, args, &self.number, out);
    }
}

/// The format placeholder `name` has in `text` (`Auto` when absent or malformed).
pub fn fmt_of(text: &str, name: &str) -> Fmt {
    pieces(text, "")
        .ok()
        .and_then(|p| {
            p.into_iter().find_map(|x| match x {
                Piece::Hole(h) if h.name == name => Some(h.fmt),
                _ => None,
            })
        })
        .unwrap_or(Fmt::Auto)
}

/// The plural operands of `v` as `fmt` shows it (`1` and `1.0` differ: CLDR's rules read the
/// visible digits). `None` for text.
pub fn operands(v: Arg<'_>, fmt: Fmt) -> Option<Operands> {
    let x = match v {
        Arg::Int(n) if matches!(fmt, Fmt::Auto | Fmt::Plain) => return Some(Operands::from_int(n)),
        Arg::Int(n) => n as f64,
        Arg::Num(x) => x,
        Arg::Str(_) => return None,
    };
    // On the stack: a HUD frame's plural forms allocate nothing.
    let mut s = Ascii::new();
    match fmt {
        Fmt::Fixed(d) if x.is_finite() && x.abs() < 1e15 => fixed_ascii(x, d, &mut s),
        Fmt::Percent => {
            let p = dmath::round_f64(x * 100.0);
            if !(p.is_finite() && p.abs() < 1e15) {
                return None;
            }
            percent_ascii(x, &mut s);
        }
        _ => auto_ascii(x, &mut s),
    }
    Operands::from_decimal(s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        Table::parse(text).unwrap()
    }

    fn tables() -> Vec<Table> {
        vec![
            table(r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "strings": {
                "hud.wave": "Wave {wave}/{waves}", "hud.menu": "Menu", "hud.only_en": "Only English",
                "hud.enemies": {"plural": "n", "one": "{n} enemy", "other": "{n} enemies"},
                "hud.dist": {"plural": "d", "one": "{d:.1} metre", "other": "{d:.1} metres"}}}"#),
            table(r#"{"format": "specula.strings.v1", "locale": "de", "name": "Deutsch", "strings": {
                "hud.wave": {"text": "Welle {wave}/{waves}", "src": "0000000000000000"}, "hud.menu": "Menü",
                "hud.enemies": {"plural": "n", "one": "{n} Feind", "other": "{n} Feinde"}}}"#),
            table(r#"{"format": "specula.strings.v1", "locale": "de-AT", "name": "Deutsch (Österreich)", "strings": {"hud.menu": "Menü (AT)"}}"#),
            table(r#"{"format": "specula.strings.v1", "locale": "ru", "name": "Русский", "strings": {
                "hud.enemies": {"plural": "n", "one": "{n} враг", "few": "{n} врага", "many": "{n} врагов", "other": "{n} врага"}}}"#),
        ]
    }

    fn cat(id: &str) -> Catalog {
        let t = tables();
        let refs: Vec<&Table> = t.iter().collect();
        Catalog::build(&Locale::parse(id).unwrap(), &refs).unwrap()
    }

    fn fmt(c: &Catalog, key: &str, args: &[(&str, Arg<'_>)]) -> String {
        let mut out = String::new();
        c.format(key, args, &mut out);
        out
    }

    #[test]
    fn lookups_walk_locale_then_language_then_source() {
        let at = cat("de-AT");
        assert_eq!(at.text("hud.menu", None), Some("Menü (AT)"));
        assert_eq!(at.get("hud.wave").unwrap().from.as_str(), "de");
        assert_eq!(at.get("hud.only_en").map(|r| (r.from.as_str(), r.status)), Some(("en", Status::Source)));
        assert_eq!(at.name, "Deutsch (Österreich)");
        assert!(at.get("hud.wave").unwrap().stale, "translated from other words than the source has now");
        assert!(!at.get("hud.menu").unwrap().stale, "no src: nothing to compare");
        assert_eq!(at.len(), 5);
        // Plurals, numbers and the rules of each locale.
        assert_eq!(fmt(&at, "hud.enemies", &[("n", Arg::Int(1))]), "1 Feind");
        assert_eq!(fmt(&at, "hud.enemies", &[("n", Arg::Int(1200))]), "1.200 Feinde");
        let ru = cat("ru");
        let got: Vec<String> = [1, 2, 5, 21, 11].iter().map(|n| fmt(&ru, "hud.enemies", &[("n", Arg::Int(*n))])).collect();
        assert_eq!(got, ["1 враг", "2 врага", "5 врагов", "21 враг", "11 врагов"]);
        let en = cat("en");
        assert_eq!(fmt(&en, "hud.dist", &[("d", Arg::Num(1.0))]), "1.0 metres", "1.0 is other in English: the shown digits pick");
        assert_eq!(fmt(&en, "hud.enemies", &[("n", Arg::Num(1.0))]), "1 enemy");
        assert_eq!(fmt(&en, "nope", &[]), "nope");
        assert_eq!(fmt(&en, "hud.enemies", &[]), "{n} enemies", "no count: other, and the hole shows");
    }

    #[test]
    fn unknown_locales_and_bad_sets_are_refused() {
        let t = tables();
        let refs: Vec<&Table> = t.iter().collect();
        let e = Catalog::build(&Locale::parse("fr").unwrap(), &refs).unwrap_err();
        assert_eq!(e.code, "unknown_locale");
        assert!(e.allowed.as_deref().unwrap().contains("de-AT"));
        // en-GB falls back to English: the source's language.
        assert_eq!(Catalog::build(&Locale::parse("en-GB").unwrap(), &refs).unwrap().text("hud.menu", None), Some("Menu"));
        assert_eq!(Catalog::build(&Locale::parse("de").unwrap(), &refs[1..]).unwrap_err().code, "no_source");
        let two = [refs[0], refs[0]];
        assert_eq!(Catalog::build(&Locale::parse("en").unwrap(), &two).unwrap_err().code, "duplicate");
        let de2 = [refs[0], refs[1], refs[1]];
        assert_eq!(Catalog::build(&Locale::parse("de").unwrap(), &de2).unwrap_err().code, "duplicate");
    }

    /// Probatio survivors: the most specific table's face wins a slot; words the source supplies
    /// are never stale; plural operands refuse what can't be shown exactly.
    #[test]
    fn faces_staleness_and_operands_hold_at_their_edges() {
        let mut t = tables();
        t[1] = table(r#"{"format": "specula.strings.v1", "locale": "de", "fonts": {"ui": "display", "title": "ui-bold"}, "strings": {"hud.menu": "Menü"}}"#);
        t[2] = table(r#"{"format": "specula.strings.v1", "locale": "de-AT", "fonts": {"ui": "ui-bold"}, "strings": {}}"#);
        let refs: Vec<&Table> = t.iter().collect();
        let at = Catalog::build(&Locale::parse("de-AT").unwrap(), &refs).unwrap();
        let fonts: Vec<(&str, String)> = at.fonts.iter().map(|(s, f)| (s.as_str(), f.name())).collect();
        assert_eq!(fonts, [("ui", "ui-bold".to_owned()), ("title", "ui-bold".to_owned())]);
        assert!(at.entries().filter(|(_, r)| r.from.as_str() == "en").all(|(_, r)| !r.stale && r.status == Status::Source));
        assert!(Catalog::pseudo(refs[0]).entries().all(|(_, r)| !r.stale));
        assert_eq!(operands(Arg::Num(1e15), Fmt::Fixed(1)), None, "past the exact range");
        assert_eq!(operands(Arg::Num(999.5), Fmt::Fixed(1)).map(|o| (o.i, o.v, o.f)), Some((999, 1, 5)));
        assert_eq!(operands(Arg::Num(1e13), Fmt::Percent), None, "1e15 percent is past the exact range");
        assert_eq!(operands(Arg::Num(0.5), Fmt::Percent).map(|o| o.i), Some(50));
        assert_eq!(operands(Arg::Str("x"), Fmt::Auto), None);
    }

    #[test]
    fn the_pseudo_locale_is_made_from_the_source() {
        let p = cat("qps-ploc");
        let w = p.text("hud.wave", None).unwrap();
        assert!(w.starts_with("[Wávé {wave}/{waves}"), "{w}");
        assert_eq!(fmt(&p, "hud.enemies", &[("n", Arg::Int(1))]), "[1 éñémý···]", "plural forms stay");
        assert_eq!(p.locale.as_str(), "qps-ploc");
    }
}

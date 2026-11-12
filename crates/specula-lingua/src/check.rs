//! Checks across a project's string tables: the source and its translations, and the keys
//! screens and code use. Each finding names its rule, the locale and key, and what to change.
//!
//! | Rule | Severity | What |
//! |---|---|---|
//! | `no_source` | error | no table says `"source": true`, or more than one does |
//! | `duplicate_locale` | error | two tables for one locale |
//! | `undefined` | error | a screen or the game uses a key the source table lacks |
//! | `placeholders` | error | a translation's placeholders aren't the source's (names) |
//! | `plural_count` | error | a translation's plural forms are picked by another placeholder than the source's |
//! | `empty` | error | a translation is empty where the source has words |
//! | `missing` | warning | a locale (and its language) lacks a source key: the source's words show |
//! | `unknown_key` | warning | a translation has a key the source doesn't |
//! | `placeholder_format` | warning | a placeholder is formatted differently from the source (`{t}` vs `{t:time}`) |
//! | `plural_forms` | warning | a plural lacks a form its locale's rules use (`other` shows instead) |
//! | `no_plural_rules` | warning | no plural rules are known for a table's language and it gives none: every number shows `other` |
//! | `stale` | warning | translated from other source words than the source has now |
//! | `unused` | warning | no screen or code uses a source key (only when the game declares its keys) |

use crate::message::placeholders;
use crate::plural::Category;
use crate::table::{source_hash, Status, Table, Text};
use crate::Locale;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    /// The locale it is about (empty for project-wide findings).
    pub locale: String,
    pub key: String,
    pub message: String,
}

impl Finding {
    pub fn to_json(&self) -> Json {
        let mut f = vec![("rule", Json::from(self.rule)), ("severity", Json::from(if self.severity == Severity::Error { "error" } else { "warning" }))];
        if !self.locale.is_empty() {
            f.push(("locale", Json::from(self.locale.as_str())));
        }
        if !self.key.is_empty() {
            f.push(("key", Json::from(self.key.as_str())));
        }
        f.push(("message", Json::from(self.message.as_str())));
        Json::obj(f)
    }
}

/// One locale's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocaleSummary {
    pub locale: String,
    pub name: String,
    pub source: bool,
    /// Its own keys.
    pub keys: usize,
    /// Source keys it (or its language) translates.
    pub translated: usize,
    pub missing: usize,
    pub unreviewed: usize,
    pub reviewed: usize,
    pub stale: usize,
}

impl LocaleSummary {
    pub fn to_json(&self) -> Json {
        let mut f = vec![("locale", Json::from(self.locale.as_str())), ("name", Json::from(self.name.as_str())), ("keys", Json::from(self.keys))];
        if self.source {
            f.push(("source", Json::Bool(true)));
        } else {
            f.extend([
                ("translated", Json::from(self.translated)),
                ("missing", Json::from(self.missing)),
                ("unreviewed", Json::from(self.unreviewed)),
                ("reviewed", Json::from(self.reviewed)),
                ("stale", Json::from(self.stale)),
            ]);
        }
        Json::obj(f)
    }
}

/// Keys screens and code use, and where (for `undefined` and `unused`).
#[derive(Clone, Debug, Default)]
pub struct Used {
    /// key → where it is used (`assets/ui/hud.json root.children[0]`, `game`).
    pub keys: BTreeMap<String, Vec<String>>,
    /// Every key the game's code may use is listed (else `unused` isn't checked).
    pub complete: bool,
}

impl Used {
    pub fn add(&mut self, key: &str, at: &str) {
        let v = self.keys.entry(key.to_owned()).or_default();
        if v.len() < 4 && !v.iter().any(|x| x == at) {
            v.push(at.to_owned());
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub locales: Vec<LocaleSummary>,
    /// The source table's locale.
    pub source: Option<String>,
}

fn names(t: &Text) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for (_, s) in t.texts() {
        for p in placeholders(s, "").map_err(|e| e.message)? {
            out.entry(p.name).or_insert_with(|| p.fmt.name());
        }
    }
    Ok(out)
}

/// Checks `tables` (each with the project path it came from) against each other and `used`.
pub fn check(tables: &[(&str, &Table)], used: Option<&Used>) -> Report {
    let mut r = Report::default();
    let mut push = |rule: &'static str, severity: Severity, locale: &str, key: &str, message: String| r.findings.push(Finding { rule, severity, locale: locale.to_owned(), key: key.to_owned(), message });
    let sources: Vec<&(&str, &Table)> = tables.iter().filter(|(_, t)| t.source).collect();
    let source = match sources.as_slice() {
        [s] => s.1,
        [] => {
            push("no_source", Severity::Error, "", "", "no string table says \"source\": true; mark the English table (assets/strings/en.json) as the source".into());
            return r;
        }
        many => {
            let p: Vec<&str> = many.iter().map(|(p, _)| *p).collect();
            push("no_source", Severity::Error, "", "", format!("more than one table says \"source\": true ({}): keep one", p.join(", ")));
            return r;
        }
    };
    let mut seen: BTreeMap<&Locale, &str> = BTreeMap::new();
    for (path, t) in tables {
        if let Some(other) = seen.insert(&t.locale, path) {
            push("duplicate_locale", Severity::Error, t.locale.as_str(), "", format!("{other} and {path} are both {}: delete one", t.locale));
        }
    }
    // A language nobody gave rules for shows "other" for every number ("1 enemies"): say so,
    // once per table (review R2 of m5-l10n: Lithuanian, Irish, Welsh passed unnoticed).
    for (_, t) in tables {
        if t.plural_rules_from() == "none" {
            push(
                "no_plural_rules",
                Severity::Warning,
                t.locale.as_str(),
                "",
                format!("no plural rules are built in for {} and the table gives none: every number shows the \"other\" form. Add the language's CLDR rules as the table's \"plural\" (for example {{\"one\": \"n = 1\"}}; {{}} if it has no plural forms)", t.locale),
            );
        }
    }
    // Source: its own soundness, and what code and screens use.
    let src_names: BTreeMap<&str, BTreeMap<String, String>> = source.entries.iter().map(|(k, e)| (k.as_str(), names(&e.text).unwrap_or_default())).collect();
    for (k, e) in &source.entries {
        if let Text::Plural { count, .. } = &e.text {
            if !src_names[k.as_str()].contains_key(count) {
                push("plural_count", Severity::Error, source.locale.as_str(), k, format!("the forms are picked by {{{count}}}, which none of them shows: name a placeholder they use"));
            }
        }
    }
    if let Some(u) = used {
        for (k, at) in &u.keys {
            if !source.entries.contains_key(k) {
                push("undefined", Severity::Error, source.locale.as_str(), k, format!("used by {} but not in the source table: add it to {} (l10n_set)", at.join(", "), source.locale));
            }
        }
        if u.complete {
            for k in source.entries.keys() {
                if !u.keys.contains_key(k) {
                    push("unused", Severity::Warning, source.locale.as_str(), k, "no screen or code uses it: delete it, or use it".into());
                }
            }
        }
    }
    r.source = Some(source.locale.to_string());
    r.locales.push(LocaleSummary { locale: source.locale.to_string(), name: source.name.clone(), source: true, keys: source.entries.len(), ..LocaleSummary::default() });
    // Translations, in locale order.
    let mut others: Vec<&(&str, &Table)> = tables.iter().filter(|(_, t)| !t.source).collect();
    others.sort_by(|a, b| a.1.locale.cmp(&b.1.locale));
    for (_, t) in others {
        let loc = t.locale.as_str();
        let rules = t.plural_rules();
        let cats = rules.categories();
        let mut s = LocaleSummary { locale: loc.to_owned(), name: t.name.clone(), keys: t.entries.len(), ..LocaleSummary::default() };
        // Tables earlier in this locale's chain (its language's) cover keys it lacks.
        let chain: Vec<&Table> = t.locale.chain(&source.locale).iter().filter_map(|l| tables.iter().find(|(_, x)| &x.locale == l && !x.source).map(|(_, x)| *x)).collect();
        for (k, se) in &source.entries {
            let Some(e) = chain.iter().find_map(|c| c.entries.get(k)) else {
                s.missing += 1;
                push("missing", Severity::Warning, loc, k, format!("no {loc} text: the source's words show (\"{}\")", crate::error::echo(se.text.form(Category::Other), 60)));
                continue;
            };
            s.translated += 1;
            if !t.entries.contains_key(k) {
                continue; // its language's table is checked on its own
            }
            match e.status {
                Status::Reviewed => s.reviewed += 1,
                _ => s.unreviewed += 1,
            }
            if e.src.is_some_and(|h| h != source_hash(&se.text)) {
                s.stale += 1;
                push("stale", Severity::Warning, loc, k, format!("translated from other source words than \"{}\": translate it again (l10n_set writes the new src)", crate::error::echo(se.text.form(Category::Other), 60)));
            }
            let mine = names(&e.text).unwrap_or_default();
            let theirs = &src_names[k.as_str()];
            let a: BTreeSet<&String> = mine.keys().collect();
            let b: BTreeSet<&String> = theirs.keys().collect();
            if a != b {
                let lack: Vec<String> = b.difference(&a).map(|n| format!("{{{n}}}")).collect();
                let extra: Vec<String> = a.difference(&b).map(|n| format!("{{{n}}}")).collect();
                let mut m = String::from("placeholders differ from the source's:");
                if !lack.is_empty() {
                    m.push_str(&format!(" missing {}", lack.join(" ")));
                }
                if !extra.is_empty() {
                    m.push_str(&format!(" not in the source {}", extra.join(" ")));
                }
                push("placeholders", Severity::Error, loc, k, m);
            } else {
                for (n, f) in &mine {
                    if theirs.get(n).is_some_and(|g| g != f) {
                        push("placeholder_format", Severity::Warning, loc, k, format!("{{{n}}} is formatted \"{f}\" here and \"{}\" in the source", theirs[n]));
                    }
                }
            }
            for (cat, text) in e.text.texts() {
                if text.trim().is_empty() && !se.text.form(cat.unwrap_or(Category::Other)).trim().is_empty() {
                    push("empty", Severity::Error, loc, k, format!("the {} text is empty", cat.map_or("", Category::name)));
                }
            }
            match (&se.text, &e.text) {
                (Text::Plural { count: sc, .. }, Text::Plural { count, forms }) => {
                    if count != sc {
                        push("plural_count", Severity::Error, loc, k, format!("the source picks forms by {{{sc}}}, this by {{{count}}}"));
                    }
                    let lacking: Vec<&str> = cats.iter().filter(|c| forms[c.index()].is_none()).map(|c| c.name()).collect();
                    if !lacking.is_empty() {
                        push("plural_forms", Severity::Warning, loc, k, format!("{loc} uses the forms {}; this lacks {} (\"other\" shows for those numbers)", cats.iter().map(|c| c.name()).collect::<Vec<_>>().join(", "), lacking.join(", ")));
                    }
                }
                (Text::Plural { .. }, Text::One(_)) if cats.len() > 1 => {
                    push("plural_forms", Severity::Warning, loc, k, format!("the source has plural forms and {loc} uses {}: give them too, or every number shows this one text", cats.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")));
                }
                _ => {}
            }
        }
        for k in t.entries.keys() {
            if !source.entries.contains_key(k) {
                push("unknown_key", Severity::Warning, loc, k, "the source table has no such key: delete it (or add it to the source)".into());
            }
        }
        r.locales.push(s);
    }
    r.findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.rule.cmp(b.rule)).then_with(|| a.locale.cmp(&b.locale)).then_with(|| a.key.cmp(&b.key)));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Table {
        Table::parse(s).unwrap()
    }

    /// Probatio survivors: where a key is used is kept at most 4 times, without repeats; the
    /// source's summary says it is the source.
    #[test]
    fn uses_are_bounded_and_the_source_says_so() {
        let mut u = Used::default();
        for at in ["a", "a", "b", "c", "d", "e"] {
            u.add("k", at);
        }
        assert_eq!(u.keys["k"], ["a", "b", "c", "d"]);
        let s = LocaleSummary { locale: "en".into(), source: true, ..LocaleSummary::default() };
        assert_eq!(s.to_json().get("source"), Some(&Json::Bool(true)));
        assert!(s.to_json().get("missing").is_none());
    }

    /// Review R2 of m5-l10n, major 4: a table for a language with no known plural rules is
    /// reported (it showed "other" for every number, with no finding); a table that gives its
    /// own rules, even none, and a language with built-in rules are not.
    #[test]
    fn a_language_without_plural_rules_is_reported() {
        let en = t(r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "strings": {"c": {"plural": "n", "one": "{n} enemy", "other": "{n} enemies"}}}"#);
        let table = |locale: &str, plural: &str| t(&format!(r#"{{"format": "specula.strings.v1", "locale": "{locale}"{plural}, "strings": {{"c": {{"plural": "n", "other": "{{n}} x"}}}}}}"#));
        let found = |tr: &Table| check(&[("en.json", &en), ("tr.json", tr)], None).findings.iter().filter(|f| f.rule == "no_plural_rules").map(|f| f.locale.clone()).collect::<Vec<_>>();
        let klingon = table("tlh", "");
        assert_eq!(klingon.plural_rules_from(), "none");
        assert_eq!(found(&klingon), ["tlh"]);
        let own = table("tlh", r#", "plural": {"one": "n = 1"}"#);
        assert_eq!(own.plural_rules_from(), "table");
        assert!(found(&own).is_empty());
        assert!(found(&table("tlh", r#", "plural": {}"#)).is_empty(), "a table may say its language has no plural forms");
        let lt = table("lt", "");
        assert_eq!(lt.plural_rules_from(), "builtin");
        assert!(found(&lt).is_empty());
        assert_eq!(en.plural_rules_from(), "builtin");
    }

    #[test]
    fn every_rule_fires_where_it_should() {
        let en = t(r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "strings": {
            "a": "Wave {wave}", "b": "Time {t:time}", "c": {"plural": "n", "one": "{n} enemy", "other": "{n} enemies"},
            "d": "Menu", "e": "Quit", "f": "Unused", "g": {"plural": "k", "one": "one", "other": "many"}}}"#);
        let hash = crate::table::hex(source_hash(&Text::One("Old menu".into())));
        let de = t(&format!(
            r#"{{"format": "specula.strings.v1", "locale": "de", "strings": {{
            "a": "Welle {{welle}}", "b": "Zeit {{t}}", "c": {{"plural": "n", "other": "{{n}} Feinde"}},
            "d": {{"text": "Menü", "src": "{hash}"}}, "e": " ", "zz": "Extra"}}}}"#
        ));
        let ru = t(r#"{"format": "specula.strings.v1", "locale": "ru", "strings": {"c": "{n} врагов", "a": {"text": "Волна {wave}", "status": "reviewed"}}}"#);
        let de_at = t(r#"{"format": "specula.strings.v1", "locale": "de-AT", "strings": {}}"#);
        let mut used = Used { complete: true, ..Used::default() };
        for k in ["a", "b", "c", "d", "e", "g"] {
            used.add(k, "assets/ui/hud.json");
        }
        used.add("nope", "game");
        let r = check(&[("en.json", &en), ("de.json", &de), ("ru.json", &ru), ("de-AT.json", &de_at)], Some(&used));
        let has = |rule: &str, loc: &str, key: &str| r.findings.iter().any(|f| f.rule == rule && f.locale == loc && f.key == key);
        assert!(has("undefined", "en", "nope"));
        assert!(has("unused", "en", "f"));
        assert!(has("plural_count", "en", "g"), "the source's forms don't show {{k}}");
        assert!(has("placeholders", "de", "a"));
        assert!(has("placeholder_format", "de", "b"));
        assert!(has("plural_forms", "de", "c"), "German uses one and other");
        assert!(has("stale", "de", "d"));
        assert!(has("empty", "de", "e"));
        assert!(has("unknown_key", "de", "zz"));
        assert!(has("missing", "de", "f"));
        assert!(has("plural_forms", "ru", "c"), "Russian uses one, few, many");
        assert!(!has("missing", "de-AT", "a"), "German covers Austrian German");
        assert!(has("missing", "de-AT", "f"));
        let ru_sum = r.locales.iter().find(|s| s.locale == "ru").unwrap();
        assert_eq!((ru_sum.translated, ru_sum.missing, ru_sum.reviewed, ru_sum.unreviewed), (2, 5, 1, 1));
        assert!(r.findings.first().is_some_and(|f| f.severity == Severity::Error), "errors first");
        // No source; two sources; two tables for one locale.
        assert_eq!(check(&[("de.json", &de)], None).findings[0].rule, "no_source");
        assert_eq!(check(&[("en.json", &en), ("en2.json", &en)], None).findings[0].rule, "no_source");
        assert!(check(&[("en.json", &en), ("de.json", &de), ("de2.json", &de)], None).findings.iter().any(|f| f.rule == "duplicate_locale"));
        // Without the game's full key list, unused isn't claimed.
        let partial = Used { complete: false, ..used.clone() };
        assert!(!check(&[("en.json", &en)], Some(&partial)).findings.iter().any(|f| f.rule == "unused"));
    }
}

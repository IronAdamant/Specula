//! Plural rules as data: CLDR's rule syntax (`v = 0 and i % 10 = 2..4 and i % 100 != 12..14`),
//! parsed with structured errors and evaluated on the operands of the number *as it is shown*
//! (`1` is "one" in English, `1.0` is "other"), with integer arithmetic only (deterministic, F2).
//!
//! Built in: CLDR 44's cardinal rules for the first languages ([`builtin`]). A table may give its
//! own (`"plural": {"one": "...", "few": "..."}`), which replaces the built-in ones.
//!
//! Operands (UTS 35): `n` the absolute value, `i` its integer digits, `v` the count of visible
//! fraction digits, `w` the same without trailing zeros, `f` the visible fraction digits as an
//! integer, `t` the same without trailing zeros, `e`/`c` the compact exponent (always 0 here).

use crate::error::{echo, LinguaError};
use std::fmt;

/// The plural categories, in the order rules are tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl Category {
    pub const ALL: [Category; 6] = [Category::Zero, Category::One, Category::Two, Category::Few, Category::Many, Category::Other];

    pub fn name(self) -> &'static str {
        match self {
            Category::Zero => "zero",
            Category::One => "one",
            Category::Two => "two",
            Category::Few => "few",
            Category::Many => "many",
            Category::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.name() == s)
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Digits kept of a number's integer and fraction parts (more are read but not kept: the integer
/// keeps its last 18 digits, which every modulus in the rules divides).
const MAX_DIGITS: u32 = 18;

/// The operands of a shown number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Operands {
    /// Integer digits (the last 18 when there are more: `big`).
    pub i: u64,
    /// More than 18 integer digits: `n` and `i` equal no value in a rule.
    pub big: bool,
    pub v: u32,
    pub w: u32,
    pub f: u64,
    pub t: u64,
}

impl Operands {
    pub fn from_int(n: i64) -> Operands {
        Operands { i: n.unsigned_abs() % 1_000_000_000_000_000_000, big: n.unsigned_abs() >= 1_000_000_000_000_000_000, ..Operands::default() }
    }

    /// From a number as shown: ASCII digits with an optional sign and one `.` (`"1.50"`,
    /// `"-3"`). Anything else (exponents, grouping, other characters) is `None`.
    pub fn from_decimal(s: &str) -> Option<Operands> {
        let s = s.strip_prefix(['-', '+']).unwrap_or(s);
        let (int, frac) = match s.split_once('.') {
            Some((a, b)) => (a, Some(b)),
            None => (s, None),
        };
        if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut o = Operands::default();
        let digits = int.trim_start_matches('0');
        o.big = digits.len() as u32 > MAX_DIGITS;
        let keep = &digits[digits.len().saturating_sub(MAX_DIGITS as usize)..];
        for b in keep.bytes() {
            o.i = o.i * 10 + u64::from(b - b'0');
        }
        if let Some(fr) = frac {
            if fr.is_empty() || !fr.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let fr = &fr[..fr.len().min(MAX_DIGITS as usize)];
            o.v = fr.len() as u32;
            for b in fr.bytes() {
                o.f = o.f * 10 + u64::from(b - b'0');
            }
            let trimmed = fr.trim_end_matches('0');
            o.w = trimmed.len() as u32;
            for b in trimmed.bytes() {
                o.t = o.t * 10 + u64::from(b - b'0');
            }
        }
        Some(o)
    }

    /// The value of operand `op` (`None`: not an integer, or too big to equal a rule's value).
    fn get(&self, op: Operand, modulus: Option<u64>) -> Option<u64> {
        let (value, whole) = match op {
            // n is an integer only when its fraction digits are all zero.
            Operand::N => (self.i, self.f == 0 && !self.big),
            Operand::I => (self.i, !self.big),
            Operand::V => (u64::from(self.v), true),
            Operand::W => (u64::from(self.w), true),
            Operand::F => (self.f, true),
            Operand::T => (self.t, true),
            Operand::E => (0, true),
        };
        match modulus {
            // The last 18 digits are kept, so i % m is exact for every m dividing 10^18; n with a
            // fraction stays a non-integer under %.
            Some(m) if matches!(op, Operand::N | Operand::I) => (op == Operand::I || self.f == 0).then_some(value % m),
            Some(m) => Some(value % m),
            None => whole.then_some(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operand {
    N,
    I,
    V,
    W,
    F,
    T,
    /// `e` and `c` (compact exponent): always 0.
    E,
}

#[derive(Clone, Debug, PartialEq)]
struct Relation {
    op: Operand,
    modulus: Option<u64>,
    negate: bool,
    ranges: Vec<(u64, u64)>,
}

impl Relation {
    fn holds(&self, o: &Operands) -> bool {
        let inside = o.get(self.op, self.modulus).is_some_and(|x| self.ranges.iter().any(|&(a, b)| (a..=b).contains(&x)));
        inside != self.negate
    }
}

/// One category's condition: relations joined by `and`, alternatives by `or`.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    any: Vec<Vec<Relation>>,
}

/// Longest rule text, and most relations and ranges in one.
pub const MAX_RULE_BYTES: usize = 512;
const MAX_RELATIONS: usize = 32;
const MAX_RANGES: usize = 16;

impl Condition {
    /// Parses a CLDR condition (samples after `@` are ignored). `field` names it in errors.
    pub fn parse(text: &str, field: &str) -> Result<Condition, LinguaError> {
        let err = |at: usize, m: String| {
            LinguaError::new("bad_rule", field, m).at(at).allowed("CLDR plural rule syntax: operand (n i v w f t e c) [% m] (= | !=) value, a..b, ... joined by and / or; e.g. \"v = 0 and i % 10 = 1 and i % 100 != 11\"")
        };
        if text.len() > MAX_RULE_BYTES {
            return Err(LinguaError::new("too_large", field, format!("a rule is at most {MAX_RULE_BYTES} bytes, this one {}", text.len())));
        }
        let body = text.split('@').next().unwrap_or("");
        let toks = tokens(body).map_err(|(at, m)| err(at, m))?;
        if toks.is_empty() {
            return Err(err(0, "an empty rule".into()));
        }
        let mut any = vec![Vec::new()];
        let mut k = 0;
        let mut count = 0;
        loop {
            // operand
            let (at, op) = match toks.get(k) {
                Some((at, Tok::Word(w))) => (
                    *at,
                    match w.as_str() {
                        "n" => Operand::N,
                        "i" => Operand::I,
                        "v" => Operand::V,
                        "w" => Operand::W,
                        "f" => Operand::F,
                        "t" => Operand::T,
                        "e" | "c" => Operand::E,
                        other => return Err(err(*at, format!("\"{}\" is not an operand", echo(other, 16)))),
                    },
                ),
                Some((at, t)) => return Err(err(*at, format!("expected an operand, found {t}"))),
                None => return Err(err(body.len(), "expected an operand at the end".into())),
            };
            k += 1;
            let mut modulus = None;
            if matches!(toks.get(k), Some((_, Tok::Mod))) {
                match toks.get(k + 1) {
                    Some((_, Tok::Num(m))) if *m > 0 => {
                        modulus = Some(*m);
                        k += 2;
                    }
                    Some((at, t)) => return Err(err(*at, format!("expected a modulus > 0, found {t}"))),
                    None => return Err(err(body.len(), "expected a modulus at the end".into())),
                }
            }
            let negate = match toks.get(k) {
                Some((_, Tok::Eq)) => false,
                Some((_, Tok::Ne)) => true,
                Some((at, t)) => return Err(err(*at, format!("expected = or !=, found {t}"))),
                None => return Err(err(body.len(), "expected = or != at the end".into())),
            };
            k += 1;
            let mut ranges = Vec::new();
            loop {
                let a = match toks.get(k) {
                    Some((_, Tok::Num(a))) => *a,
                    Some((at, t)) => return Err(err(*at, format!("expected a number, found {t}"))),
                    None => return Err(err(body.len(), "expected a number at the end".into())),
                };
                k += 1;
                let mut b = a;
                if matches!(toks.get(k), Some((_, Tok::Range))) {
                    b = match toks.get(k + 1) {
                        Some((_, Tok::Num(b))) => *b,
                        Some((at, t)) => return Err(err(*at, format!("expected a number after .., found {t}"))),
                        None => return Err(err(body.len(), "expected a number after ..".into())),
                    };
                    if b < a {
                        return Err(err(toks[k].0, format!("the range {a}..{b} is empty")));
                    }
                    k += 2;
                }
                ranges.push((a, b));
                if ranges.len() > MAX_RANGES {
                    return Err(err(toks.get(k).map_or(body.len(), |t| t.0), format!("more than {MAX_RANGES} ranges in one relation")));
                }
                if matches!(toks.get(k), Some((_, Tok::Comma))) {
                    k += 1;
                    continue;
                }
                break;
            }
            count += 1;
            if count > MAX_RELATIONS {
                return Err(err(at, format!("more than {MAX_RELATIONS} relations")));
            }
            if let Some(last) = any.last_mut() {
                last.push(Relation { op, modulus, negate, ranges });
            }
            match toks.get(k) {
                None => break,
                Some((_, Tok::And)) => k += 1,
                Some((_, Tok::Or)) => {
                    any.push(Vec::new());
                    k += 1;
                }
                Some((at, t)) => return Err(err(*at, format!("expected and / or, found {t}"))),
            }
        }
        Ok(Condition { any })
    }

    pub fn holds(&self, o: &Operands) -> bool {
        self.any.iter().any(|all| all.iter().all(|r| r.holds(o)))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Num(u64),
    Mod,
    Eq,
    Ne,
    Range,
    Comma,
    And,
    Or,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Word(w) => write!(f, "\"{}\"", echo(w, 16)),
            Tok::Num(n) => write!(f, "{n}"),
            Tok::Mod => f.write_str("%"),
            Tok::Eq => f.write_str("="),
            Tok::Ne => f.write_str("!="),
            Tok::Range => f.write_str(".."),
            Tok::Comma => f.write_str(","),
            Tok::And => f.write_str("and"),
            Tok::Or => f.write_str("or"),
        }
    }
}

fn tokens(s: &str) -> Result<Vec<(usize, Tok)>, (usize, String)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'%' => {
                out.push((i, Tok::Mod));
                i += 1;
            }
            b'=' => {
                out.push((i, Tok::Eq));
                i += 1;
            }
            b'!' if b.get(i + 1) == Some(&b'=') => {
                out.push((i, Tok::Ne));
                i += 2;
            }
            b'.' if b.get(i + 1) == Some(&b'.') => {
                out.push((i, Tok::Range));
                i += 2;
            }
            b',' => {
                out.push((i, Tok::Comma));
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                let mut v: u64 = 0;
                while i < b.len() && b[i].is_ascii_digit() {
                    v = v.checked_mul(10).and_then(|v| v.checked_add(u64::from(b[i] - b'0'))).ok_or((start, "a number too large".to_owned()))?;
                    i += 1;
                }
                out.push((start, Tok::Num(v)));
            }
            b'a'..=b'z' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_lowercase() {
                    i += 1;
                }
                let w = &s[start..i];
                out.push((
                    start,
                    match w {
                        "and" => Tok::And,
                        "or" => Tok::Or,
                        "mod" => Tok::Mod,
                        _ => Tok::Word(w.to_owned()),
                    },
                ));
            }
            _ => {
                let ch = s[i..].chars().next().unwrap_or('?');
                return Err((i, format!("unexpected '{ch}'")));
            }
        }
    }
    Ok(out)
}

/// A language's rules: a condition per category it uses (`other` needs none: it is what is left).
#[derive(Clone, Debug, PartialEq)]
pub struct PluralRules {
    rules: Vec<(Category, Condition)>,
}

impl PluralRules {
    /// `other` only (Japanese, Chinese, Korean...).
    pub fn other_only() -> PluralRules {
        PluralRules { rules: Vec::new() }
    }

    /// From `(category, CLDR condition)` pairs; `field` names them in errors (`plural`).
    pub fn parse(pairs: &[(&str, &str)], field: &str) -> Result<PluralRules, LinguaError> {
        let mut rules: Vec<(Category, Condition)> = Vec::new();
        for (name, text) in pairs {
            let f = format!("{field}.{name}");
            let cat = Category::parse(name).ok_or_else(|| LinguaError::new("invalid_value", &f, format!("\"{}\" is not a plural category", echo(name, 24))).allowed("zero, one, two, few, many"))?;
            if cat == Category::Other {
                return Err(LinguaError::new("invalid_value", &f, "\"other\" takes no rule: it is every number no other category takes"));
            }
            if rules.iter().any(|(c, _)| *c == cat) {
                return Err(LinguaError::new("duplicate", &f, format!("\"{name}\" is given twice")));
            }
            rules.push((cat, Condition::parse(text, &f)?));
        }
        rules.sort_by_key(|(c, _)| *c);
        Ok(PluralRules { rules })
    }

    /// The built-in rules for `locale` (its full id, then its language), or `None`.
    pub fn builtin(locale: &crate::Locale) -> Option<PluralRules> {
        let data = builtin(locale.as_str()).or_else(|| builtin(locale.language()))?;
        // The built-in data is tested to parse (`every_builtin_rule_parses`).
        PluralRules::parse(data, "plural").ok()
    }

    /// The categories this language uses (always ending with `other`).
    pub fn categories(&self) -> Vec<Category> {
        let mut c: Vec<Category> = self.rules.iter().map(|(c, _)| *c).collect();
        c.push(Category::Other);
        c
    }

    pub fn select(&self, o: &Operands) -> Category {
        self.rules.iter().find(|(_, c)| c.holds(o)).map_or(Category::Other, |(c, _)| *c)
    }
}

/// `e = 0 and i != 0 and i % 1000000 = 0 and v = 0 or e != 0..5`: CLDR's "many" for round
/// millions (French, Spanish, Italian, Portuguese).
const MILLIONS: &str = "e = 0 and i != 0 and i % 1000000 = 0 and v = 0 or e != 0..5";
/// Serbo-Croatian (hr, sr, bs): fractions count by their last digits too.
const SOUTH_SLAVIC: [(&str, &str); 2] = [
    ("one", "v = 0 and i % 10 = 1 and i % 100 != 11 or f % 10 = 1 and f % 100 != 11"),
    ("few", "v = 0 and i % 10 = 2..4 and i % 100 != 12..14 or f % 10 = 2..4 and f % 100 != 12..14"),
];
const EAST_SLAVIC: [(&str, &str); 3] = [
    ("one", "v = 0 and i % 10 = 1 and i % 100 != 11"),
    ("few", "v = 0 and i % 10 = 2..4 and i % 100 != 12..14"),
    ("many", "v = 0 and i % 10 = 0 or v = 0 and i % 10 = 5..9 or v = 0 and i % 100 = 11..14"),
];

/// CLDR 44 cardinal rules for the languages Specula's games start with, by locale or language.
/// A language that isn't here has no built-in rules: its table gives its own (`"plural"`), or
/// every number shows the `other` form, which `check` reports (`no_plural_rules`).
pub fn builtin(id: &str) -> Option<&'static [(&'static str, &'static str)]> {
    Some(match id {
        "en" | "de" | "nl" | "sv" | "fi" | "et" | "ur" | "sw" => &[("one", "i = 1 and v = 0")],
        "it" | "ca" | "pt-PT" => &[("one", "i = 1 and v = 0"), ("many", MILLIONS)],
        "es" => &[("one", "n = 1"), ("many", MILLIONS)],
        "fr" => &[("one", "i = 0,1"), ("many", MILLIONS)],
        "pt" => &[("one", "i = 0..1"), ("many", MILLIONS)],
        "ru" | "uk" => &EAST_SLAVIC,
        // Belarusian reads n, not i and v: 1.0 is "one", 2.0 "few" (CLDR 44).
        "be" => &[("one", "n % 10 = 1 and n % 100 != 11"), ("few", "n % 10 = 2..4 and n % 100 != 12..14"), ("many", "n % 10 = 0 or n % 10 = 5..9 or n % 100 = 11..14")],
        "pl" => &[
            ("one", "i = 1 and v = 0"),
            ("few", "v = 0 and i % 10 = 2..4 and i % 100 != 12..14"),
            ("many", "v = 0 and i != 1 and i % 10 = 0..1 or v = 0 and i % 10 = 5..9 or v = 0 and i % 100 = 12..14"),
        ],
        "cs" | "sk" => &[("one", "i = 1 and v = 0"), ("few", "i = 2..4 and v = 0"), ("many", "v != 0")],
        "ro" => &[("one", "i = 1 and v = 0"), ("few", "v != 0 or n = 0 or n != 1 and n % 100 = 1..19")],
        "da" => &[("one", "n = 1 or t != 0 and i = 0,1")],
        "nb" | "no" | "nn" | "el" | "hu" | "tr" | "bg" => &[("one", "n = 1")],
        "af" | "sq" | "eu" | "ka" | "kk" | "az" | "mn" | "ne" | "uz" | "ta" | "te" | "ml" | "mr" => &[("one", "n = 1")],
        "gl" => &[("one", "i = 1 and v = 0")],
        "hy" => &[("one", "i = 0,1")],
        "hi" | "bn" | "fa" | "gu" | "kn" | "am" => &[("one", "i = 0 or n = 1")],
        "lt" => &[("one", "n % 10 = 1 and n % 100 != 11..19"), ("few", "n % 10 = 2..9 and n % 100 != 11..19"), ("many", "f != 0")],
        "lv" => &[
            ("zero", "n % 10 = 0 or n % 100 = 11..19 or v = 2 and f % 100 = 11..19"),
            ("one", "n % 10 = 1 and n % 100 != 11 or v = 2 and f % 10 = 1 and f % 100 != 11 or v != 2 and f % 10 = 1"),
        ],
        "sl" => &[("one", "v = 0 and i % 100 = 1"), ("two", "v = 0 and i % 100 = 2"), ("few", "v = 0 and i % 100 = 3..4 or v != 0")],
        "hr" | "sr" | "bs" => &SOUTH_SLAVIC,
        "mk" => &[("one", "v = 0 and i % 10 = 1 and i % 100 != 11 or f % 10 = 1 and f % 100 != 11")],
        "is" => &[("one", "t = 0 and i % 10 = 1 and i % 100 != 11 or t % 10 = 1 and t % 100 != 11")],
        "ga" => &[("one", "n = 1"), ("two", "n = 2"), ("few", "n = 3..6"), ("many", "n = 7..10")],
        "cy" => &[("zero", "n = 0"), ("one", "n = 1"), ("two", "n = 2"), ("few", "n = 3"), ("many", "n = 6")],
        "mt" => &[("one", "n = 1"), ("two", "n = 2"), ("few", "n = 0 or n % 100 = 3..10"), ("many", "n % 100 = 11..19")],
        "he" => &[("one", "i = 1 and v = 0 or i = 0 and v != 0"), ("two", "i = 2 and v = 0")],
        "ar" => &[("zero", "n = 0"), ("one", "n = 1"), ("two", "n = 2"), ("few", "n % 100 = 3..10"), ("many", "n % 100 = 11..99")],
        "ja" | "zh" | "ko" | "th" | "vi" | "id" | "ms" | "lo" | "my" => &[],
        _ => return None,
    })
}

/// Languages with built-in rules (for docs and tool schemas).
pub const BUILTIN: [&str; 70] = [
    "en", "de", "nl", "sv", "fi", "et", "ur", "sw", "it", "ca", "pt-PT", "es", "fr", "pt", "ru", "uk", "be", "pl", "cs", "sk", "ro", "da", "nb", "no", "nn", "el", "hu", "tr", "bg", "he", "ar", "ja", "zh", "ko", "th", "vi", "id",
    "af", "sq", "eu", "ka", "kk", "az", "mn", "ne", "uz", "ta", "te", "ml", "mr", "gl", "hy", "hi", "bn", "fa", "gu", "kn", "am", "lt", "lv", "sl", "hr", "sr", "bs", "mk", "is", "ga", "cy", "mt", "ms",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    fn rules(id: &str) -> PluralRules {
        PluralRules::builtin(&Locale::parse(id).unwrap()).unwrap()
    }

    fn cat(r: &PluralRules, s: &str) -> &'static str {
        r.select(&Operands::from_decimal(s).unwrap()).name()
    }

    #[test]
    fn every_builtin_rule_parses() {
        for id in BUILTIN {
            let data = builtin(id).unwrap();
            PluralRules::parse(data, "plural").unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        assert!(builtin("xx").is_none());
        // ms, lo, my have rules too, outside the doc list.
        assert!(builtin("ms").is_some());
    }

    /// Review R2 of m5-l10n, major 4: Lithuanian, Latvian, Slovenian, Serbo-Croatian, Irish,
    /// Welsh, Maltese and others had no rules and silently showed `other` for every number.
    /// The expected categories are CLDR 44's own samples, not this code's output.
    #[test]
    fn more_cldr_languages_select_their_categories() {
        let all = |id: &str, want: &[(&str, &str)]| {
            let r = rules(id);
            for (n, c) in want {
                assert_eq!(cat(&r, n), *c, "{id}: {n}");
            }
        };
        all("lt", &[("1", "one"), ("21", "one"), ("101", "one"), ("1.0", "one"), ("2", "few"), ("9", "few"), ("22", "few"), ("0", "other"), ("10", "other"), ("11", "other"), ("19", "other"), ("20", "other"), ("0.1", "many"), ("10.5", "many")]);
        all("lv", &[("0", "zero"), ("10", "zero"), ("11", "zero"), ("19", "zero"), ("20", "zero"), ("0.11", "zero"), ("1", "one"), ("21", "one"), ("101", "one"), ("0.1", "one"), ("1.1", "one"), ("2", "other"), ("9", "other"), ("22", "other"), ("0.2", "other")]);
        all("sl", &[("1", "one"), ("101", "one"), ("2", "two"), ("102", "two"), ("3", "few"), ("4", "few"), ("104", "few"), ("1.5", "few"), ("0", "other"), ("5", "other"), ("100", "other")]);
        for id in ["hr", "sr", "bs"] {
            all(id, &[("1", "one"), ("21", "one"), ("0.1", "one"), ("2", "few"), ("24", "few"), ("0.2", "few"), ("0", "other"), ("5", "other"), ("11", "other"), ("12", "other"), ("0.11", "other"), ("1.0", "other")]);
        }
        all("mk", &[("1", "one"), ("21", "one"), ("0.1", "one"), ("0", "other"), ("2", "other"), ("11", "other")]);
        all("is", &[("1", "one"), ("21", "one"), ("1.0", "one"), ("0.1", "one"), ("0", "other"), ("2", "other"), ("11", "other"), ("0.2", "other")]);
        all("ga", &[("1", "one"), ("2", "two"), ("3", "few"), ("6", "few"), ("7", "many"), ("10", "many"), ("0", "other"), ("11", "other"), ("1.5", "other")]);
        all("cy", &[("0", "zero"), ("1", "one"), ("2", "two"), ("3", "few"), ("6", "many"), ("4", "other"), ("5", "other"), ("7", "other"), ("100", "other")]);
        all("mt", &[("1", "one"), ("2", "two"), ("0", "few"), ("3", "few"), ("10", "few"), ("103", "few"), ("11", "many"), ("19", "many"), ("111", "many"), ("20", "other"), ("101", "other")]);
        for id in ["hi", "bn", "fa", "gu", "kn", "am"] {
            all(id, &[("0", "one"), ("1", "one"), ("0.5", "one"), ("2", "other"), ("1.5", "other")]);
        }
        all("hy", &[("0", "one"), ("1", "one"), ("1.5", "one"), ("2", "other")]);
        all("gl", &[("1", "one"), ("0", "other"), ("1.0", "other"), ("2", "other")]);
        for id in ["af", "sq", "eu", "ka", "kk", "az", "mn", "ne", "uz", "ta", "te", "ml", "mr"] {
            all(id, &[("1", "one"), ("1.0", "one"), ("0", "other"), ("2", "other")]);
        }
    }

    #[test]
    fn cldr_samples_select_their_categories() {
        let en = rules("en");
        assert_eq!([cat(&en, "1"), cat(&en, "0"), cat(&en, "2"), cat(&en, "1.0"), cat(&en, "1.5"), cat(&en, "-1")], ["one", "other", "other", "other", "other", "one"]);
        let fr = rules("fr");
        assert_eq!([cat(&fr, "0"), cat(&fr, "1"), cat(&fr, "1.5"), cat(&fr, "2"), cat(&fr, "1000000"), cat(&fr, "2000000"), cat(&fr, "1000001")], ["one", "one", "one", "other", "many", "many", "other"]);
        let ru = rules("ru");
        let got: Vec<&str> = ["1", "21", "101", "11", "2", "3", "4", "22", "12", "14", "5", "0", "11", "19", "111", "1.5"].iter().map(|s| cat(&ru, s)).collect();
        assert_eq!(got, ["one", "one", "one", "many", "few", "few", "few", "few", "many", "many", "many", "many", "many", "many", "many", "other"]);
        let pl = rules("pl");
        let got: Vec<&str> = ["1", "2", "4", "22", "5", "12", "21", "0", "1.5"].iter().map(|s| cat(&pl, s)).collect();
        assert_eq!(got, ["one", "few", "few", "few", "many", "many", "many", "many", "other"]);
        let ar = rules("ar");
        let got: Vec<&str> = ["0", "1", "2", "3", "10", "11", "99", "100", "103", "111", "0.5"].iter().map(|s| cat(&ar, s)).collect();
        assert_eq!(got, ["zero", "one", "two", "few", "few", "many", "many", "other", "few", "many", "other"]);
        let cs = rules("cs");
        assert_eq!([cat(&cs, "1"), cat(&cs, "3"), cat(&cs, "5"), cat(&cs, "1.5")], ["one", "few", "other", "many"]);
        let ja = rules("ja");
        assert_eq!(ja.categories(), [Category::Other]);
        assert_eq!(cat(&ja, "1"), "other");
        let da = rules("da");
        assert_eq!([cat(&da, "1"), cat(&da, "0.1"), cat(&da, "2")], ["one", "one", "other"]);
        // Belarusian reads n (review of m5-l10n, finding 11): 1.0 is one, 2.0 few, 1.5 other.
        let be = rules("be");
        assert_eq!([cat(&be, "1.0"), cat(&be, "2.0"), cat(&be, "5"), cat(&be, "1.5"), cat(&be, "21")], ["one", "few", "many", "other", "one"]);
        // A locale falls back to its language's rules; pt-PT has its own.
        assert_eq!(cat(&rules("de-AT"), "1"), "one");
        assert_eq!([cat(&rules("pt-BR"), "0"), cat(&rules("pt-PT"), "0")], ["one", "other"]);
        // Big numbers keep their last 18 digits for modulus and equal no small value.
        let big = Operands::from_decimal("1000000000000000000001").unwrap();
        assert!(big.big);
        assert_eq!(en.select(&big), Category::Other);
        assert_eq!(ru.select(&big), Category::One, "i % 10 = 1 and i % 100 != 11 holds on the last digits");
    }

    #[test]
    fn operands_follow_uts_35() {
        let o = Operands::from_decimal("1.230").unwrap();
        assert_eq!((o.i, o.v, o.w, o.f, o.t), (1, 3, 2, 230, 23));
        assert_eq!(Operands::from_decimal("-12").unwrap().i, 12);
        assert_eq!(Operands::from_int(-7), Operands::from_decimal("7").unwrap());
        for bad in ["", ".", "1.", "a", "1e5", "1,000", "1.2.3", "--1"] {
            assert!(Operands::from_decimal(bad).is_none(), "{bad}");
        }
        assert_eq!(Operands::from_int(i64::MIN).i, i64::MIN.unsigned_abs() % 1_000_000_000_000_000_000);
    }

    #[test]
    fn bad_rules_are_structured_errors() {
        for (text, at) in [("", 0), ("x = 1", 0), ("n = ", 4), ("n == 1", 3), ("n = 3..1", 5), ("n % 0 = 1", 4), ("n = 1 xor n = 2", 6), ("n = 1 and", 9), ("n = 99999999999999999999", 4), ("n # 1", 2)] {
            let e = Condition::parse(text, "plural.one").unwrap_err();
            assert_eq!((e.code, e.offset), ("bad_rule", Some(at)), "{text}: {e}");
            assert_eq!(e.field, "plural.one");
        }
        assert_eq!(Condition::parse(&"n = 1 or ".repeat(60), "p").unwrap_err().code, "too_large");
        let many = (0..40).map(|k| format!("n = {k}")).collect::<Vec<_>>().join(" or ");
        assert!(Condition::parse(&many, "p").unwrap_err().message.contains("relations"));
        let ranges = (0..20).map(|k| k.to_string()).collect::<Vec<_>>().join(",");
        assert!(Condition::parse(&format!("n = {ranges}"), "p").unwrap_err().message.contains("ranges"));
        assert_eq!(PluralRules::parse(&[("other", "n = 1")], "plural").unwrap_err().code, "invalid_value");
        assert_eq!(PluralRules::parse(&[("one", "n = 1"), ("one", "n = 2")], "plural").unwrap_err().code, "duplicate");
        assert_eq!(PluralRules::parse(&[("lots", "n = 1")], "plural").unwrap_err().code, "invalid_value");
        // Samples are ignored, and `mod` is `%`.
        let c = Condition::parse("n mod 10 = 1 @integer 1, 21, 31", "p").unwrap();
        assert!(c.holds(&Operands::from_int(21)));
    }
}

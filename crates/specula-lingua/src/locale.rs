//! Locale ids: a BCP 47 subset (`en`, `de`, `pt-BR`, `zh-Hant-TW`, `es-419`, and the pseudo-locale
//! `qps-ploc`), normalised (language lower case, script title case, region upper case), and the
//! fallback chain a lookup walks: the locale, each shorter prefix of it, then the source table's
//! locale (English).

use crate::error::{echo, LinguaError};
use std::fmt;

/// Longest locale id accepted.
pub const MAX_LOCALE_BYTES: usize = 35;

/// The pseudo-locale: the source table's words accented and about 35% longer, in brackets, so
/// overflow shows before anyone translates (generated, never a file).
pub const PSEUDO: &str = "qps-ploc";

/// A normalised locale id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Locale(String);

impl Locale {
    /// Parses and normalises `s` (`EN-gb` → `en-GB`). Subtags: a language of 2-3 letters, then
    /// optionally a script of 4 letters, a region of 2 letters or 3 digits, and variants of 4-8
    /// letters or digits (4 only when it starts with a digit; at most 2), separated by `-` (or
    /// `_`).
    pub fn parse(s: &str) -> Result<Locale, LinguaError> {
        let bad = |why: &str| {
            LinguaError::new("bad_locale", "locale", format!("\"{}\" is not a locale id: {why}", echo(s, 40)))
                .allowed("a language of 2-3 letters, then optionally -Script (4 letters), -RG (2 letters or 3 digits) and up to 2 variants, e.g. en, de, pt-BR, zh-Hant-TW, es-419, qps-ploc")
        };
        if s.is_empty() || s.len() > MAX_LOCALE_BYTES || !s.is_ascii() {
            return Err(bad("1..35 ASCII characters"));
        }
        if s.replace('_', "-").eq_ignore_ascii_case(PSEUDO) {
            return Ok(Locale(PSEUDO.to_owned()));
        }
        let mut out = String::with_capacity(s.len());
        let mut stage = 0u8; // 0 language, 1 script, 2 region, 3 variants
        let mut variants = 0;
        for (i, part) in s.split(['-', '_']).enumerate() {
            if part.is_empty() {
                return Err(bad("an empty subtag"));
            }
            let alpha = part.bytes().all(|b| b.is_ascii_alphabetic());
            let digit = part.bytes().all(|b| b.is_ascii_digit());
            let alnum = part.bytes().all(|b| b.is_ascii_alphanumeric());
            if i == 0 {
                if !(alpha && (2..=3).contains(&part.len())) {
                    return Err(bad("the language is 2-3 letters"));
                }
                out.push_str(&part.to_ascii_lowercase());
                continue;
            }
            out.push('-');
            if stage < 1 && alpha && part.len() == 4 {
                let mut t = part.to_ascii_lowercase();
                t[..1].make_ascii_uppercase();
                out.push_str(&t);
                stage = 1;
            } else if stage < 2 && ((alpha && part.len() == 2) || (digit && part.len() == 3)) {
                out.push_str(&part.to_ascii_uppercase());
                stage = 2;
            } else if alnum && ((5..=8).contains(&part.len()) || (part.len() == 4 && part.as_bytes()[0].is_ascii_digit())) {
                variants += 1;
                if variants > 2 {
                    return Err(bad("at most 2 variants"));
                }
                out.push_str(&part.to_ascii_lowercase());
                stage = 3;
            } else {
                return Err(bad(&format!("subtag \"{part}\" is not a script, region or variant here")));
            }
        }
        Ok(Locale(out))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The language subtag (`pt` for `pt-BR`).
    pub fn language(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }

    pub fn is_pseudo(&self) -> bool {
        self.0 == PSEUDO
    }

    /// Where a lookup goes, in order: this locale, then each parent ([`Locale::parent`]: the
    /// shorter prefix, except where CLDR says otherwise), then `source` (when not already
    /// there). `pt-BR` reads `pt`; `zh-Hant-TW` reads `zh-Hant` and then the source, never `zh`
    /// (Simplified Chinese: another script). The pseudo-locale's chain is the source.
    pub fn chain(&self, source: &Locale) -> Vec<Locale> {
        let mut out: Vec<Locale> = Vec::new();
        if !self.is_pseudo() {
            let mut at = Some(self.clone());
            while let Some(l) = at {
                if out.contains(&l) || out.len() >= 8 {
                    break;
                }
                at = l.parent();
                out.push(l);
            }
        }
        if !out.contains(source) {
            out.push(source.clone());
        }
        out
    }

    /// The locale this one falls back to before the source (`None`: only the source is left).
    /// The shorter prefix, with CLDR's parent locales where a prefix would be wrong:
    /// - a script that isn't its language's usual one doesn't read the bare language
    ///   (`zh-Hant` is not `zh`, `sr-Latn` is not `sr`): its words would be in the other script;
    /// - Latin American Spanish reads `es-419`, and European and African Portuguese `pt-PT`,
    ///   before `es` and `pt` (which is Brazilian).
    pub fn parent(&self) -> Option<Locale> {
        let id = self.0.as_str();
        let (head, tail) = id.rsplit_once('-')?;
        let bare = !head.contains('-');
        if bare && OWN_SCRIPT.contains(&(head, tail)) {
            return None;
        }
        if bare && head == "es" && ES_419.contains(&tail) {
            return Some(Locale("es-419".to_owned()));
        }
        if bare && head == "pt" && PT_PT.contains(&tail) {
            return Some(Locale("pt-PT".to_owned()));
        }
        // Chinese as written in Taiwan, Hong Kong and Macau is Traditional, script named or not.
        if bare && head == "zh" && ["TW", "HK", "MO"].contains(&tail) {
            return Some(Locale("zh-Hant".to_owned()));
        }
        Some(Locale(head.to_owned()))
    }
}

/// `(language, script)` pairs whose words aren't the bare language's (CLDR parent locale `root`).
const OWN_SCRIPT: [(&str, &str); 14] = [
    ("zh", "Hant"),
    ("sr", "Latn"),
    ("az", "Cyrl"),
    ("az", "Arab"),
    ("uz", "Cyrl"),
    ("uz", "Arab"),
    ("pa", "Arab"),
    ("bs", "Cyrl"),
    ("yue", "Hans"),
    ("mn", "Mong"),
    ("kk", "Arab"),
    ("ha", "Arab"),
    ("ks", "Deva"),
    ("sd", "Deva"),
];
/// Regions whose Spanish reads `es-419` first (CLDR parent locales).
const ES_419: [&str; 22] = ["AR", "BO", "BR", "BZ", "CL", "CO", "CR", "CU", "DO", "EC", "GT", "HN", "MX", "NI", "PA", "PE", "PR", "PY", "SV", "US", "UY", "VE"];
/// Regions whose Portuguese reads `pt-PT` first.
const PT_PT: [&str; 10] = ["AO", "CH", "CV", "GQ", "GW", "LU", "MO", "MZ", "ST", "TL"];

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review R2 of m5-l10n, minor 6: Traditional Chinese fell back to `zh` (Simplified) and
    /// Latin-script Serbian to Cyrillic `sr`.
    #[test]
    fn the_chain_follows_cldr_parents() {
        let en = Locale::parse("en").unwrap();
        let chain = |id: &str| Locale::parse(id).unwrap().chain(&en).iter().map(|l| l.as_str().to_owned()).collect::<Vec<_>>();
        assert_eq!(chain("pt-BR"), ["pt-BR", "pt", "en"]);
        assert_eq!(chain("zh-Hant-TW"), ["zh-Hant-TW", "zh-Hant", "en"], "never zh: another script");
        assert_eq!(chain("zh-Hans-CN"), ["zh-Hans-CN", "zh-Hans", "zh", "en"]);
        assert_eq!(chain("zh-TW"), ["zh-TW", "zh-Hant", "en"], "Taiwan writes Traditional, script named or not");
        assert_eq!(chain("zh-HK"), ["zh-HK", "zh-Hant", "en"]);
        assert_eq!(chain("zh-CN"), ["zh-CN", "zh", "en"]);
        assert_eq!(chain("zh-SG"), ["zh-SG", "zh", "en"]);
        assert_eq!(chain("sr-Latn-RS"), ["sr-Latn-RS", "sr-Latn", "en"]);
        assert_eq!(chain("sr-Cyrl"), ["sr-Cyrl", "sr", "en"]);
        assert_eq!(chain("es-MX"), ["es-MX", "es-419", "es", "en"]);
        assert_eq!(chain("es-ES"), ["es-ES", "es", "en"]);
        assert_eq!(chain("pt-AO"), ["pt-AO", "pt-PT", "pt", "en"]);
        assert_eq!(chain("en-GB"), ["en-GB", "en"]);
        assert_eq!(chain("qps-ploc"), ["en"]);
        assert_eq!(chain("de"), ["de", "en"]);
    }

    #[test]
    fn locales_parse_and_normalise() {
        for (given, want) in [("en", "en"), ("EN-gb", "en-GB"), ("pt_br", "pt-BR"), ("zh-hant-tw", "zh-Hant-TW"), ("es-419", "es-419"), ("qps-PLOC", "qps-ploc"), ("sl-rozaj", "sl-rozaj")] {
            assert_eq!(Locale::parse(given).unwrap().as_str(), want, "{given}");
        }
        for bad in ["", "e", "english", "en--GB", "en-", "en-GB-x", "de-DE-1-2", "ja-日本", "en-GB-GB", "a-b-c-d-e-f-g-h-i-j-k-l-m-n-o-p-q-r", "en-Latn-US-abcde-fghij-klmno"] {
            let e = Locale::parse(bad).unwrap_err();
            assert_eq!(e.code, "bad_locale", "{bad}");
        }
    }

    #[test]
    fn the_chain_is_locale_then_language_then_source() {
        let en = Locale::parse("en").unwrap();
        let chain = |s: &str| Locale::parse(s).unwrap().chain(&en).into_iter().map(|l| l.0).collect::<Vec<_>>();
        assert_eq!(chain("de-AT"), ["de-AT", "de", "en"]);
        // (Traditional Chinese never reads `zh`, Simplified: see the_chain_follows_cldr_parents.)
        assert_eq!(chain("zh-Hans-SG"), ["zh-Hans-SG", "zh-Hans", "zh", "en"]);
        assert_eq!(chain("en-GB"), ["en-GB", "en"]);
        assert_eq!(chain("en"), ["en"]);
        assert_eq!(chain(PSEUDO), ["en"]);
        assert_eq!(Locale::parse("pt-BR").unwrap().language(), "pt");
    }
}

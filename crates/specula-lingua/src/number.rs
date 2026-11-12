//! Number formats as data: the decimal separator, the grouping separator and when grouping
//! starts (CLDR's minimum grouping digits), and the percent pattern. Built in for the first
//! languages ([`NumberFormat::builtin`]); a table may override them (`"number": {...}`).
//!
//! Formatting works on the ASCII text Rust's `Display` makes (exact and the same on every
//! platform), then localises it: no float maths beyond `std`'s formatting and `dmath` (F2).

use crate::error::{echo, LinguaError};
use specula_core::dmath;
use specula_core::Json;
use std::fmt::Write as _;

/// How a locale writes numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    pub decimal: char,
    /// `None`: never grouped.
    pub group: Option<char>,
    /// Grouping starts when the integer part has at least `3 + min_group` digits (CLDR
    /// `minimumGroupingDigits`: 1 groups 1,234; 2 leaves 1234 and groups 12 345).
    pub min_group: u8,
    /// The percent pattern: `#` is the number (`"#%"`, `"#\u{a0}%"`, `"%#"`).
    pub percent: String,
}

impl Default for NumberFormat {
    fn default() -> NumberFormat {
        NumberFormat::english()
    }
}

const NBSP: char = '\u{a0}';

impl NumberFormat {
    pub fn english() -> NumberFormat {
        NumberFormat { decimal: '.', group: Some(','), min_group: 1, percent: "#%".into() }
    }

    fn of(decimal: char, group: char, min_group: u8, percent: &str) -> NumberFormat {
        NumberFormat { decimal, group: Some(group), min_group, percent: percent.into() }
    }

    /// CLDR's formats for `locale` (its full id, then its language); English when unknown.
    /// French groups with a no-break space (CLDR's narrow one, U+202F, is missing from many
    /// interface faces).
    pub fn builtin(locale: &crate::Locale) -> NumberFormat {
        NumberFormat::lookup(locale.as_str()).or_else(|| NumberFormat::lookup(locale.language())).unwrap_or_else(NumberFormat::english)
    }

    fn lookup(id: &str) -> Option<NumberFormat> {
        Some(match id {
            "en" | "ja" | "zh" | "ko" | "th" | "he" | "ur" | "sw" => NumberFormat::english(),
            "tr" => NumberFormat::of(',', '.', 1, "%#"),
            "de" | "nl" | "id" | "da" | "el" | "ro" | "vi" => NumberFormat::of(',', '.', 1, if id == "de" || id == "da" || id == "el" || id == "ro" { "#\u{a0}%" } else { "#%" }),
            "de-CH" => NumberFormat::of('.', '\u{2019}', 1, "#%"),
            "it" | "pt" => NumberFormat::of(',', '.', 1, "#%"),
            "pt-PT" => NumberFormat::of(',', NBSP, 2, "#%"),
            "es" => NumberFormat::of(',', '.', 2, "#\u{a0}%"),
            "fr" => NumberFormat::of(',', NBSP, 1, "#\u{a0}%"),
            "ru" | "uk" | "be" | "cs" | "sk" | "fi" | "sv" | "nb" | "no" | "nn" | "bg" | "et" | "hu" => NumberFormat::of(',', NBSP, 1, "#\u{a0}%"),
            "pl" => NumberFormat::of(',', NBSP, 2, "#%"),
            "ar" => NumberFormat::of('.', ',', 1, "#%"),
            _ => return None,
        })
    }

    /// Reads a table's `number` override on top of `base`: `{decimal, group (a character, or ""
    /// for none), min_group (1..4), percent (a pattern with one #, at most 16 bytes)}`.
    pub fn with_json(base: &NumberFormat, j: &Json, field: &str) -> Result<NumberFormat, LinguaError> {
        let obj = j.as_object().ok_or_else(|| LinguaError::new("bad_type", field, "an object {decimal?, group?, min_group?, percent?}"))?;
        let mut out = base.clone();
        let one_char = |v: &Json, f: &str, empty_ok: bool| -> Result<Option<char>, LinguaError> {
            let s = v.as_str().ok_or_else(|| LinguaError::new("bad_type", f, "a string of one character"))?;
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (None, _) if empty_ok => Ok(None),
                (Some(c), None) if !c.is_ascii_digit() && !c.is_control() && c != '#' && c != '-' => Ok(Some(c)),
                _ => Err(LinguaError::new("invalid_value", f, format!("\"{}\" is not one character (not a digit, '-' or '#')", echo(s, 16)))),
            }
        };
        for (k, v) in obj {
            let f = format!("{field}.{k}");
            match k.as_str() {
                "decimal" => out.decimal = one_char(v, &f, false)?.unwrap_or('.'),
                "group" => out.group = one_char(v, &f, true)?,
                "min_group" => {
                    out.min_group = v.as_u64().filter(|n| (1..=4).contains(n)).ok_or_else(|| LinguaError::new("invalid_value", &f, "a whole number 1..4"))? as u8;
                }
                "percent" => {
                    let s = v.as_str().ok_or_else(|| LinguaError::new("bad_type", &f, "a pattern string"))?;
                    if s.len() > 16 || s.matches('#').count() != 1 || s.chars().any(|c| c.is_control()) {
                        return Err(LinguaError::new("invalid_value", &f, format!("\"{}\" must hold exactly one # (the number), at most 16 bytes", echo(s, 24))).allowed("#%, # %, %#"));
                    }
                    out.percent = s.to_owned();
                }
                other => return Err(LinguaError::new("unknown_field", &f, format!("\"{}\" is not a number setting", echo(other, 32))).allowed("decimal, group, min_group, percent")),
            }
        }
        if out.group == Some(out.decimal) {
            return Err(LinguaError::new("invalid_value", field, "the group and decimal separators must differ"));
        }
        Ok(out)
    }

    /// Localises an ASCII number as Rust writes it (`-1234.5`, `1.000e300`): the decimal point
    /// becomes this locale's, and the integer digits are grouped by three. Text that isn't a
    /// number is copied as it is.
    pub fn localise(&self, ascii: &str, out: &mut String) {
        let (sign, rest) = match ascii.strip_prefix('-') {
            Some(r) => ("-", r),
            None => ("", ascii),
        };
        let int_len = rest.bytes().take_while(u8::is_ascii_digit).count();
        if int_len == 0 {
            out.push_str(ascii);
            return;
        }
        out.push_str(sign);
        let digits = &rest[..int_len];
        match self.group {
            Some(g) if int_len >= 3 + usize::from(self.min_group) => {
                for (k, c) in digits.char_indices() {
                    if k > 0 && (int_len - k) % 3 == 0 {
                        out.push(g);
                    }
                    out.push(c);
                }
            }
            _ => out.push_str(digits),
        }
        for c in rest[int_len..].chars() {
            out.push(if c == '.' { self.decimal } else { c });
        }
    }

    /// An integer.
    pub fn write_int(&self, n: i64, out: &mut String) {
        let mut s = Ascii::new();
        let _ = write!(s, "{n}");
        self.localise(s.as_str(), out);
    }

    /// A number with up to 2 decimals (trailing zeros trimmed; integers as integers).
    pub fn write_auto(&self, x: f64, out: &mut String) {
        let mut s = Ascii::new();
        auto_ascii(x, &mut s);
        self.localise(s.as_str(), out);
    }

    /// A number with exactly `decimals` decimals (at most 6).
    pub fn write_fixed(&self, x: f64, decimals: u8, out: &mut String) {
        let mut s = Ascii::new();
        fixed_ascii(x, decimals, &mut s);
        self.localise(s.as_str(), out);
    }

    /// A 0..1 ratio as a whole percentage (`0.456` → `46%`).
    pub fn write_percent(&self, ratio: f64, out: &mut String) {
        let mut s = Ascii::new();
        percent_ascii(ratio, &mut s);
        let (a, b) = self.percent.split_once('#').unwrap_or(("", "%"));
        out.push_str(a);
        self.localise(s.as_str(), out);
        out.push_str(b);
    }
}

/// A number's ASCII text on the stack: formatting a number allocates nothing (HUD frames, F3).
/// Numbers are at most 32 bytes here (`{:.6}` below 1e15, `{:.3e}`, i64); more is cut.
pub struct Ascii {
    buf: [u8; 40],
    len: usize,
}

impl Ascii {
    pub fn new() -> Ascii {
        Ascii { buf: [0; 40], len: 0 }
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII is written (write_str cuts on a character boundary).
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    fn pop_while(&mut self, b: u8) {
        while self.len > 0 && self.buf[self.len - 1] == b {
            self.len -= 1;
        }
    }
}

impl Default for Ascii {
    fn default() -> Ascii {
        Ascii::new()
    }
}

impl std::fmt::Write for Ascii {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = self.buf.len() - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// [`write_auto_ascii`] onto the stack.
pub fn auto_ascii(x: f64, out: &mut Ascii) {
    if !x.is_finite() || x.abs() >= 1e15 {
        let _ = write!(out, "{x:.3e}");
    } else if dmath::fract_f64(x) == 0.0 {
        let _ = write!(out, "{}", x as i64);
    } else {
        let _ = write!(out, "{x:.2}");
        out.pop_while(b'0');
        out.pop_while(b'.');
    }
}

/// `x` with exactly `decimals` decimals (at most 6), in ASCII.
pub fn fixed_ascii(x: f64, decimals: u8, out: &mut Ascii) {
    if !x.is_finite() || x.abs() >= 1e15 {
        let _ = write!(out, "{x:.3e}");
    } else {
        let _ = write!(out, "{x:.*}", usize::from(decimals.min(6)));
    }
}

/// A 0..1 ratio as a whole percentage, in ASCII digits (no sign).
pub fn percent_ascii(ratio: f64, out: &mut Ascii) {
    let p = dmath::round_f64(ratio * 100.0);
    if p.is_finite() && p.abs() < 1e15 {
        let _ = write!(out, "{}", p as i64);
    } else {
        let _ = write!(out, "{p:.3e}");
    }
}

/// `x` as Specula's interfaces print a number by default, in ASCII: integers as integers, others
/// with up to 2 decimals (trailing zeros trimmed); huge or non-finite ones as `1.000e300`.
pub fn write_auto_ascii(x: f64, out: &mut String) {
    if !x.is_finite() || x.abs() >= 1e15 {
        let _ = write!(out, "{x:.3e}");
    } else if dmath::fract_f64(x) == 0.0 {
        let _ = write!(out, "{}", x as i64);
    } else {
        let start = out.len();
        let _ = write!(out, "{x:.2}");
        while out.len() > start && out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    fn fmt(id: &str) -> NumberFormat {
        NumberFormat::builtin(&Locale::parse(id).unwrap())
    }

    fn s(f: impl FnOnce(&mut String)) -> String {
        let mut out = String::new();
        f(&mut out);
        out
    }

    #[test]
    fn numbers_follow_the_locale() {
        let en = fmt("en");
        assert_eq!(s(|o| en.write_int(1234567, o)), "1,234,567");
        assert_eq!(s(|o| en.write_int(-1234, o)), "-1,234");
        assert_eq!(s(|o| en.write_int(999, o)), "999");
        assert_eq!(s(|o| en.write_auto(2.5, o)), "2.5");
        assert_eq!(s(|o| en.write_auto(3.0, o)), "3");
        assert_eq!(s(|o| en.write_fixed(1234.5, 1, o)), "1,234.5");
        assert_eq!(s(|o| en.write_percent(0.456, o)), "46%");
        let de = fmt("de-AT");
        assert_eq!(s(|o| de.write_int(1234, o)), "1.234");
        assert_eq!(s(|o| de.write_fixed(2.25, 2, o)), "2,25");
        assert_eq!(s(|o| de.write_percent(0.5, o)), "50\u{a0}%");
        let es = fmt("es");
        assert_eq!(s(|o| es.write_int(1234, o)), "1234", "minimum grouping digits 2");
        assert_eq!(s(|o| es.write_int(12345, o)), "12.345");
        let fr = fmt("fr");
        assert_eq!(s(|o| fr.write_auto(1234.5, o)), "1\u{a0}234,5");
        assert_eq!(s(|o| fmt("xx").write_int(1000, o)), "1,000", "unknown locales write English");
        assert_eq!(s(|o| fmt("tr").write_percent(0.25, o)), "%25", "Turkish puts the sign first");
        assert_eq!(s(|o| en.write_auto(f64::NAN, o)), "NaN");
        assert_eq!(s(|o| en.write_fixed(1e300, 2, o)), "1.000e300");
    }

    #[test]
    fn overrides_are_checked() {
        let en = NumberFormat::english();
        let j = Json::parse(r#"{"decimal": ",", "group": "", "min_group": 2, "percent": "%#"}"#).unwrap();
        let f = NumberFormat::with_json(&en, &j, "number").unwrap();
        assert_eq!((f.decimal, f.group, f.min_group), (',', None, 2));
        assert_eq!(s(|o| f.write_percent(0.25, o)), "%25");
        assert_eq!(s(|o| f.write_int(123456, o)), "123456");
        for (bad, code) in [(r#"{"decimal": ""}"#, "invalid_value"), (r#"{"decimal": "ab"}"#, "invalid_value"), (r#"{"decimal": "5"}"#, "invalid_value"), (r#"{"percent": "%"}"#, "invalid_value"), (r#"{"min_group": 9}"#, "invalid_value"), (r#"{"radix": 2}"#, "unknown_field"), (r#"{"group": "."}"#, "invalid_value"), ("[]", "bad_type")] {
            let e = NumberFormat::with_json(&en, &Json::parse(bad).unwrap(), "number").unwrap_err();
            assert_eq!(e.code, code, "{bad}");
        }
    }
}

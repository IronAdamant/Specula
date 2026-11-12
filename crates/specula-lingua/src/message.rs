//! Messages: text with placeholders, in the same syntax as `specula-ui`'s templates, so a string
//! table's text is a screen's template: `{wave}`, `{next:time}`, `{ratio:pct}`, `{dist:.1}`,
//! `{seed:plain}`, `{item.name}`; `{{` and `}}` are literal braces.
//!
//! [`placeholders`] reads them (for checks: a translation must use the source's placeholders),
//! [`format`] fills them for code outside a UI (dialogue lines, messages), and [`pseudo`] makes
//! the pseudo-locale's text (placeholders kept exactly).

use crate::error::{echo, LinguaError};
use crate::number::{write_auto_ascii, NumberFormat};
use std::fmt::Write as _;

/// How a placeholder's value is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    /// Integers as integers, other numbers with up to 2 decimals; grouped and with the locale's
    /// decimal separator.
    Auto,
    /// `{key:.N}`: exactly N decimals (N ≤ 6).
    Fixed(u8),
    /// `{key:time}`: seconds as `m:ss`.
    Time,
    /// `{key:pct}`: a 0..1 ratio as a whole percentage, in the locale's pattern.
    Percent,
    /// `{key:plain}`: digits as they are (ids, seeds, years): never grouped.
    Plain,
}

impl Fmt {
    pub fn parse(s: &str) -> Option<Fmt> {
        match s {
            "time" => Some(Fmt::Time),
            "pct" => Some(Fmt::Percent),
            "plain" => Some(Fmt::Plain),
            _ => {
                let n: u8 = s.strip_prefix('.')?.parse().ok()?;
                (n <= 6).then_some(Fmt::Fixed(n))
            }
        }
    }

    pub fn name(self) -> String {
        match self {
            Fmt::Auto => String::new(),
            Fmt::Fixed(n) => format!(".{n}"),
            Fmt::Time => "time".into(),
            Fmt::Percent => "pct".into(),
            Fmt::Plain => "plain".into(),
        }
    }
}

/// One `{name:fmt}` in a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placeholder {
    pub name: String,
    pub fmt: Fmt,
    /// Byte offset of its `{`.
    pub offset: usize,
}

/// Most placeholders one text may hold.
pub const MAX_PLACEHOLDERS: usize = 16;

/// A placeholder name: letters, digits, `_`, `-`, `.` (at most 64 bytes).
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// One piece of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Literal text (braces unescaped).
    Text(String),
    Hole(Placeholder),
}

/// Splits `text` into literal text and placeholders. `field` names it in errors.
pub fn pieces(text: &str, field: &str) -> Result<Vec<Piece>, LinguaError> {
    let err = |at: usize, m: String| LinguaError::new("bad_placeholder", field, m).at(at).allowed("{name}, {name:.N}, {name:time}, {name:pct}, {name:plain}; {{ and }} for braces");
    let mut out: Vec<Piece> = Vec::new();
    let mut lit = String::new();
    let b = text.as_bytes();
    let mut i = 0;
    let mut holes = 0;
    while i < b.len() {
        match b[i] {
            b'{' if b.get(i + 1) == Some(&b'{') => {
                lit.push('{');
                i += 2;
            }
            b'}' if b.get(i + 1) == Some(&b'}') => {
                lit.push('}');
                i += 2;
            }
            b'{' => {
                let end = text[i + 1..].find('}').ok_or_else(|| err(i, format!("unclosed '{{' at byte {i}")))?;
                let inner = text[i + 1..i + 1 + end].trim();
                let (name, fmt) = match inner.split_once(':') {
                    Some((n, f)) => (n.trim(), Fmt::parse(f.trim()).ok_or_else(|| err(i, format!("unknown format \"{}\" in {{{}}}", echo(f, 16), echo(inner, 40))))?),
                    None => (inner, Fmt::Auto),
                };
                if !valid_name(name) {
                    return Err(err(i, format!("bad placeholder name \"{}\": letters, digits, _ - . (1..64)", echo(name, 40))));
                }
                holes += 1;
                if holes > MAX_PLACEHOLDERS {
                    return Err(err(i, format!("more than {MAX_PLACEHOLDERS} placeholders in one text")));
                }
                if !lit.is_empty() {
                    out.push(Piece::Text(std::mem::take(&mut lit)));
                }
                out.push(Piece::Hole(Placeholder { name: name.to_owned(), fmt, offset: i }));
                i += end + 2;
            }
            b'}' => return Err(err(i, format!("unmatched '}}' at byte {i} (write }}}} for a brace)"))),
            _ => {
                let ch = text[i..].chars().next().map_or(1, char::len_utf8);
                lit.push_str(&text[i..i + ch]);
                i += ch;
            }
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Text(lit));
    }
    Ok(out)
}

/// The placeholders of `text`, in order.
pub fn placeholders(text: &str, field: &str) -> Result<Vec<Placeholder>, LinguaError> {
    Ok(pieces(text, field)?
        .into_iter()
        .filter_map(|p| match p {
            Piece::Hole(h) => Some(h),
            Piece::Text(_) => None,
        })
        .collect())
}

/// A value for [`format`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arg<'a> {
    Int(i64),
    Num(f64),
    Str(&'a str),
}

/// Writes `v` as `fmt` says, in `nf`'s format.
pub fn write_arg(v: Arg<'_>, fmt: Fmt, nf: &NumberFormat, out: &mut String) {
    match (v, fmt) {
        (Arg::Str(s), _) => out.push_str(s),
        (Arg::Int(n), Fmt::Plain) => {
            let _ = write!(out, "{n}");
        }
        (Arg::Num(x), Fmt::Plain) => write_auto_ascii(x, out),
        (Arg::Int(n), Fmt::Auto) => nf.write_int(n, out),
        (Arg::Num(x), Fmt::Auto) => nf.write_auto(x, out),
        (v, Fmt::Fixed(d)) => nf.write_fixed(num(v), d, out),
        (v, Fmt::Percent) => nf.write_percent(num(v), out),
        (v, Fmt::Time) => write_time(num(v), out),
    }
}

fn num(v: Arg<'_>) -> f64 {
    match v {
        Arg::Int(n) => n as f64,
        Arg::Num(x) => x,
        Arg::Str(_) => 0.0,
    }
}

/// Seconds as `m:ss` (rounded up; negative is 0).
pub fn write_time(seconds: f64, out: &mut String) {
    let s = if seconds.is_finite() { specula_core::dmath::ceil_f64(seconds.max(0.0)).min(1e15) as i64 } else { 0 };
    let _ = write!(out, "{}:{:02}", s / 60, s % 60);
}

/// Fills `text`'s placeholders from `args` into `out` (cleared first). A placeholder `args`
/// lacks prints as `{name}`, so it shows. Malformed text is copied as it is.
pub fn format(text: &str, args: &[(&str, Arg<'_>)], nf: &NumberFormat, out: &mut String) {
    out.clear();
    let Ok(pieces) = pieces(text, "") else {
        out.push_str(text);
        return;
    };
    for p in pieces {
        match p {
            Piece::Text(t) => out.push_str(&t),
            Piece::Hole(h) => match args.iter().find(|(n, _)| *n == h.name) {
                Some((_, v)) => write_arg(*v, h.fmt, nf, out),
                None => {
                    out.push('{');
                    out.push_str(&h.name);
                    out.push('}');
                }
            },
        }
    }
}

/// Text in the pseudo-locale: letters accented (Latin-1, which every interface face has), about
/// 35% longer (`·` padding), the whole in brackets; placeholders and escapes kept as they are.
/// Text without letters (separators, numbers, only placeholders) is left as it is.
pub fn pseudo(text: &str, out: &mut String) {
    let Ok(pieces) = pieces(text, "") else {
        pseudo_plain(text, out);
        return;
    };
    let letters: usize = pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.chars().filter(|c| c.is_alphabetic()).count(),
            Piece::Hole(_) => 0,
        })
        .sum();
    if letters == 0 {
        out.push_str(text);
        return;
    }
    let chars: usize = pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.chars().count(),
            Piece::Hole(_) => 0,
        })
        .sum();
    out.push('[');
    for p in &pieces {
        match p {
            Piece::Text(t) => {
                for c in t.chars() {
                    match c {
                        '{' => out.push_str("{{"),
                        '}' => out.push_str("}}"),
                        c => out.push(accent(c)),
                    }
                }
            }
            Piece::Hole(h) => {
                out.push('{');
                out.push_str(&h.name);
                if h.fmt != Fmt::Auto {
                    out.push(':');
                    out.push_str(&h.fmt.name());
                }
                out.push('}');
            }
        }
    }
    for _ in 0..(chars * 35).div_ceil(100).max(1) {
        out.push('·');
    }
    out.push(']');
}

/// [`pseudo`] for text that is shown as it is (no placeholders): every character is literal.
pub fn pseudo_plain(text: &str, out: &mut String) {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    if letters == 0 {
        out.push_str(text);
        return;
    }
    out.push('[');
    for c in text.chars() {
        out.push(accent(c));
    }
    for _ in 0..(text.chars().count() * 35).div_ceil(100).max(1) {
        out.push('·');
    }
    out.push(']');
}

fn accent(c: char) -> char {
    match c {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        'y' => 'ý',
        'n' => 'ñ',
        'c' => 'ç',
        'A' => 'Á',
        'E' => 'É',
        'I' => 'Í',
        'O' => 'Ó',
        'U' => 'Ú',
        'Y' => 'Ý',
        'N' => 'Ñ',
        'C' => 'Ç',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_read_the_ui_template_syntax() {
        let p = placeholders("Wave {wave}/{waves} next {next_wave_s:time} {r:pct} {d:.1} {seed:plain} {{x}} {item.name}", "t").unwrap();
        let names: Vec<(&str, Fmt)> = p.iter().map(|h| (h.name.as_str(), h.fmt)).collect();
        assert_eq!(names, [("wave", Fmt::Auto), ("waves", Fmt::Auto), ("next_wave_s", Fmt::Time), ("r", Fmt::Percent), ("d", Fmt::Fixed(1)), ("seed", Fmt::Plain), ("item.name", Fmt::Auto)]);
        assert_eq!(p[0].offset, 5);
        for (bad, at) in [("{dust", 0), ("dust}", 4), ("{du st}", 0), ("{dust:hex}", 0), ("{}", 0), ("a {b:.7}", 2)] {
            let e = placeholders(bad, "strings.x").unwrap_err();
            assert_eq!((e.code, e.offset, e.field.as_str()), ("bad_placeholder", Some(at), "strings.x"), "{bad}");
        }
        let many = "{a}".repeat(MAX_PLACEHOLDERS + 1);
        assert!(placeholders(&many, "t").unwrap_err().message.contains("more than"));
        let p = pieces("a{{b}}c{x}d", "t").unwrap();
        assert_eq!(p[0], Piece::Text("a{b}c".to_owned()));
        assert_eq!(p[2], Piece::Text("d".to_owned()));
    }

    #[test]
    fn format_fills_and_localises() {
        let mut out = String::new();
        let de = crate::NumberFormat::builtin(&crate::Locale::parse("de").unwrap());
        format("{n} Feinde, {t:time}, {r:pct}, {x:.1}, Seed {s:plain}, {who}, {missing}", &[("n", Arg::Int(1200)), ("t", Arg::Num(61.2)), ("r", Arg::Num(0.5)), ("x", Arg::Num(2.25)), ("s", Arg::Int(123456)), ("who", Arg::Str("Ada"))], &de, &mut out);
        assert_eq!(out, "1.200 Feinde, 1:02, 50\u{a0}%, 2,2, Seed 123456, Ada, {missing}");
        format("broken {", &[], &de, &mut out);
        assert_eq!(out, "broken {");
    }

    #[test]
    fn pseudo_keeps_placeholders() {
        let mut out = String::new();
        pseudo("Wave {wave}/{waves:plain} {{ok}}", &mut out);
        assert!(out.starts_with("[Wávé {wave}/{waves:plain} {{ók}}"), "{out}");
        assert!(out.ends_with("·]"));
        assert_eq!(placeholders(&out, "t").unwrap().len(), 2);
        out.clear();
        pseudo("{dust}", &mut out);
        assert_eq!(out, "{dust}");
        out.clear();
        pseudo_plain("Menu", &mut out);
        assert_eq!(out, "[Méñú··]");
    }
}

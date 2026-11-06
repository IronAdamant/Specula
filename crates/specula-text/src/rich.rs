//! Rich text markup: a small tag language agents and screens write, turned into plain text plus
//! [`Span`]s for [`crate::layout_rich`].
//!
//! | markup | meaning |
//! |---|---|
//! | `[b]…[/b]` | bold (the font set's bold face) |
//! | `[color=#rgb]…[/color]`, `#rrggbb`, `#rrggbbaa` | colour (sRGB) |
//! | `[icon=name]` | an inline icon one em wide (e.g. a controller prompt, `[icon=pad_a]`) |
//! | `[icon=name w=1.5]` | an inline icon `w` ems wide (0.25–4) |
//! | `[[` | a literal `[` |
//!
//! Tags nest (inner colours win; bold accumulates) up to [`MAX_DEPTH`]. Malformed markup is a
//! structured [`RichError`] with the byte offset and what to change, never a panic; sizes are
//! capped ([`MAX_MARKUP_BYTES`], [`MAX_SPANS`]).

use crate::layout::{Span, INLINE_OBJECT};
use std::fmt;

/// Longest markup accepted.
pub const MAX_MARKUP_BYTES: usize = 64 * 1024;
/// Most spans one text makes.
pub const MAX_SPANS: usize = 1024;
/// Deepest tag nesting.
pub const MAX_DEPTH: usize = 16;
/// Longest icon name.
pub const MAX_ICON_NAME: usize = 32;

/// Plain text and its styled spans (non-overlapping, in order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rich {
    pub text: String,
    pub spans: Vec<Span>,
}

/// Why markup was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichError {
    /// Byte offset in the markup.
    pub at: usize,
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for RichError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}: {}", self.code, self.at, self.message)
    }
}

impl std::error::Error for RichError {}

fn err(at: usize, code: &'static str, message: impl Into<String>) -> RichError {
    RichError { at, code, message: message.into() }
}

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` into sRGB 0..1 with alpha.
pub fn parse_color(s: &str) -> Option<[f32; 4]> {
    let h = s.strip_prefix('#')?;
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok().map(|x| if n == 1 { x * 17 } else { x } as f32 / 255.0);
    match h.len() {
        3 => Some([v(0, 1)?, v(1, 1)?, v(2, 1)?, 1.0]),
        6 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?, 1.0]),
        8 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?]),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    Bold,
    Color([f32; 4]),
}

/// Parses `markup` (see the module docs).
pub fn parse(markup: &str) -> Result<Rich, RichError> {
    if markup.len() > MAX_MARKUP_BYTES {
        return Err(err(MAX_MARKUP_BYTES, "too_long", format!("markup is {} bytes; at most {MAX_MARKUP_BYTES}", markup.len())));
    }
    let mut out = Rich::default();
    let mut stack: Vec<(Tag, usize)> = Vec::new();
    let style = |stack: &[(Tag, usize)]| {
        let bold = stack.iter().any(|(t, _)| *t == Tag::Bold);
        let color = stack.iter().rev().find_map(|(t, _)| if let Tag::Color(c) = t { Some(*c) } else { None });
        (bold, color)
    };
    let push_text = |out: &mut Rich, s: &str, (bold, color): (bool, Option<[f32; 4]>)| -> Result<(), RichError> {
        if s.is_empty() {
            return Ok(());
        }
        let start = out.text.len();
        out.text.push_str(s);
        if !bold && color.is_none() {
            return Ok(());
        }
        if let Some(last) = out.spans.last_mut() {
            if last.bytes.end == start && last.bold == bold && last.color == color && last.icon.is_none() {
                last.bytes.end = out.text.len();
                return Ok(());
            }
        }
        if out.spans.len() >= MAX_SPANS {
            return Err(err(start, "too_many_spans", format!("at most {MAX_SPANS} styled runs")));
        }
        out.spans.push(Span { bytes: start..out.text.len(), bold, color, icon: None });
        Ok(())
    };
    let mut i = 0usize;
    let bytes = markup.as_bytes();
    let mut text_from = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        push_text(&mut out, &markup[text_from..i], style(&stack))?;
        if bytes.get(i + 1) == Some(&b'[') {
            push_text(&mut out, "[", style(&stack))?;
            i += 2;
            text_from = i;
            continue;
        }
        let Some(close) = markup[i..].find(']').map(|k| i + k) else {
            return Err(err(i, "unclosed_tag", "a '[' starts a tag that never ends with ']' (write '[[' for a literal '[')"));
        };
        let tag = &markup[i + 1..close];
        match tag {
            "b" => {
                if stack.len() >= MAX_DEPTH {
                    return Err(err(i, "too_deep", format!("tags nest at most {MAX_DEPTH} deep")));
                }
                stack.push((Tag::Bold, i));
            }
            "/b" => match stack.pop() {
                Some((Tag::Bold, _)) => {}
                _ => return Err(err(i, "mismatched_tag", "'[/b]' doesn't close the innermost open tag")),
            },
            "/color" => match stack.pop() {
                Some((Tag::Color(_), _)) => {}
                _ => return Err(err(i, "mismatched_tag", "'[/color]' doesn't close the innermost open tag")),
            },
            t if t.starts_with("color=") => {
                let c = parse_color(&t[6..]).ok_or_else(|| err(i, "bad_color", format!("'{}' is not #rgb, #rrggbb or #rrggbbaa", &t[6..])))?;
                if stack.len() >= MAX_DEPTH {
                    return Err(err(i, "too_deep", format!("tags nest at most {MAX_DEPTH} deep")));
                }
                stack.push((Tag::Color(c), i));
            }
            t if t.starts_with("icon=") => {
                let mut parts = t[5..].split(' ').filter(|p| !p.is_empty());
                let name = parts.next().unwrap_or("");
                if name.is_empty() || name.len() > MAX_ICON_NAME || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                    return Err(err(i, "bad_icon", format!("icon names are 1..={MAX_ICON_NAME} of a-z, 0-9, '_' and '-'")));
                }
                let mut width = 1.0f32;
                for p in parts {
                    match p.strip_prefix("w=").and_then(|w| w.parse::<f32>().ok()) {
                        Some(w) if (0.25..=4.0).contains(&w) => width = w,
                        _ => return Err(err(i, "bad_icon", format!("'{p}': the only icon option is w=0.25..4 (ems)"))),
                    }
                }
                if out.spans.len() >= MAX_SPANS {
                    return Err(err(i, "too_many_spans", format!("at most {MAX_SPANS} styled runs")));
                }
                let start = out.text.len();
                out.text.push(INLINE_OBJECT);
                let (bold, color) = style(&stack);
                out.spans.push(Span { bytes: start..out.text.len(), bold, color, icon: Some((name.to_owned(), width)) });
            }
            other => {
                let shown: String = other.chars().take(24).collect();
                return Err(err(i, "unknown_tag", format!("'[{shown}]' is not a tag: use [b], [color=#rrggbb], [icon=name] or '[[' for '['")));
            }
        }
        i = close + 1;
        text_from = i;
    }
    push_text(&mut out, &markup[text_from..], style(&stack))?;
    if let Some((_, at)) = stack.last() {
        return Err(err(*at, "unclosed_tag", "a tag is never closed ([/b] or [/color])"));
    }
    Ok(out)
}

/// The markup with every tag removed (what a screen reader or a plain-text check reads); `[[`
/// becomes `[`, icons become their names in brackets. Malformed markup comes back unchanged.
pub fn plain(markup: &str) -> String {
    match parse(markup) {
        Ok(r) => {
            let mut s = String::with_capacity(r.text.len());
            let mut icons = r.spans.iter().filter_map(|sp| sp.icon.as_ref().map(|(n, _)| (sp.bytes.start, n)));
            let mut next = icons.next();
            for (i, c) in r.text.char_indices() {
                match next {
                    Some((at, name)) if at == i && c == INLINE_OBJECT => {
                        s.push('[');
                        s.push_str(name);
                        s.push(']');
                        next = icons.next();
                    }
                    _ => s.push(c),
                }
            }
            s
        }
        Err(_) => markup.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_make_flat_spans() {
        let r = parse("Press [icon=pad_a] to [b]build [color=#3fe0d0]a ward[/color][/b] now [[ok]").unwrap();
        assert_eq!(r.text, "Press \u{FFFC} to build a ward now [ok]");
        assert_eq!(r.spans.len(), 3, "{:?}", r.spans);
        assert_eq!(r.spans[0].icon, Some(("pad_a".into(), 1.0)));
        assert_eq!(&r.text[r.spans[1].bytes.clone()], "build ");
        assert!(r.spans[1].bold && r.spans[1].color.is_none());
        assert_eq!(&r.text[r.spans[2].bytes.clone()], "a ward");
        assert!(r.spans[2].bold);
        let c = r.spans[2].color.unwrap();
        assert!((c[0] - 0x3f as f32 / 255.0).abs() < 1e-6 && c[3] == 1.0);
        assert_eq!(plain("a [icon=pad_x w=2] b [b]c[/b]"), "a [pad_x] b c");
        assert_eq!(parse_color("#fff"), Some([1.0; 4]));
        assert_eq!(parse_color("#00000080").map(|c| c[3]), Some(128.0 / 255.0));
        assert_eq!(parse_color("fff"), None);
        assert_eq!(parse_color("#ggg"), None);
    }

    #[test]
    fn malformed_markup_is_a_structured_error() {
        let cases = [
            ("a [b]b", "unclosed_tag", 2),
            ("a [b", "unclosed_tag", 2),
            ("[b]x[/color]", "mismatched_tag", 4),
            ("[color=red]x[/color]", "bad_color", 0),
            ("[size=3]", "unknown_tag", 0),
            ("[icon=]", "bad_icon", 0),
            ("[icon=a w=9]", "bad_icon", 0),
            ("[icon=a/b]", "bad_icon", 0),
            ("x[/b]", "mismatched_tag", 1),
        ];
        for (m, code, at) in cases {
            let e = parse(m).unwrap_err();
            assert_eq!((e.code, e.at), (code, at), "{m}: {e}");
        }
        let deep = "[b]".repeat(MAX_DEPTH + 1);
        assert_eq!(parse(&deep).unwrap_err().code, "too_deep");
        let many = "[icon=a]".repeat(MAX_SPANS + 1);
        assert_eq!(parse(&many).unwrap_err().code, "too_many_spans");
        assert_eq!(parse(&"x".repeat(MAX_MARKUP_BYTES + 1)).unwrap_err().code, "too_long");
    }

    #[test]
    fn hostile_markup_never_panics() {
        let alphabet = ["[", "]", "b", "/", "color=", "#", "f", "icon=", " w=", "1", "é", "[[", "x", "\u{FFFC}"];
        let mut seed = 7u32;
        for _ in 0..2000 {
            let n = (seed % 12) as usize;
            let mut s = String::new();
            for _ in 0..n {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                s.push_str(alphabet[(seed >> 16) as usize % alphabet.len()]);
            }
            if let Ok(r) = parse(&s) {
                for sp in &r.spans {
                    assert!(sp.bytes.end <= r.text.len() && r.text.is_char_boundary(sp.bytes.start) && r.text.is_char_boundary(sp.bytes.end), "{s:?}");
                }
            }
            let _ = plain(&s);
        }
    }
}

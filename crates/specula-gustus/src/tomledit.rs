//! Sets one key of a TOML file by its dotted path (`stage.game.key_color`), keeping everything
//! else byte for byte: comments, order, blank lines and line endings. A key that exists has its
//! value replaced (a trailing comment stays); a missing key is added at the end of its table; a
//! missing table is appended. The result is parsed again before it is returned.

use crate::{Result, TasteError};

fn header(line: &str) -> Option<String> {
    let t = line.trim();
    // A header is `[a.b]` or `[[a.b]]` alone on its line (a comment may follow); `[0.1, 0.2],`
    // inside an array value is not one.
    let t = t.split('#').next().unwrap_or("").trim_end();
    if !t.starts_with('[') || !t.ends_with(']') || t.contains(',') || t.contains('=') {
        return None;
    }
    if t.starts_with("[[") {
        return Some(format!("[[{}]]", t.trim_start_matches('[').split(']').next().unwrap_or("").trim()));
    }
    let inner = t.strip_prefix('[')?.split(']').next()?;
    Some(inner.split('.').map(|s| s.trim().trim_matches('"')).collect::<Vec<_>>().join("."))
}

/// What a line is, read with the state of the lines before it (multi-line strings and arrays
/// spanning lines are never headers or keys).
#[derive(Clone, Debug, PartialEq)]
enum Line {
    Header(String),
    /// A `key = value` line at the top level of its table (the key as written).
    Key(String),
    Other,
}

fn classify(lines: &[&str]) -> Vec<Line> {
    let mut out = Vec::with_capacity(lines.len());
    // Inside a multi-line string (its delimiter), and the open bracket depth of a value.
    let mut in_string: Option<&'static str> = None;
    let mut depth = 0i32;
    for l in lines {
        if let Some(delim) = in_string {
            if l.contains(delim) {
                in_string = None;
                depth += bracket_depth(l.split(delim).last().unwrap_or(""));
            }
            out.push(Line::Other);
            continue;
        }
        if depth > 0 {
            depth += bracket_depth(l);
            out.push(Line::Other);
            continue;
        }
        if let Some(h) = header(l) {
            out.push(Line::Header(h));
            continue;
        }
        let t = l.trim_start();
        let Some(eq) = t.find('=').filter(|_| !t.starts_with('#')) else {
            out.push(Line::Other);
            continue;
        };
        let value = &t[eq + 1..];
        for delim in ["\"\"\"", "'''"] {
            if value.matches(delim).count() % 2 == 1 {
                in_string = Some(delim);
            }
        }
        if in_string.is_none() {
            depth = bracket_depth(value).max(0);
        }
        out.push(Line::Key(t[..eq].trim().to_owned()));
    }
    out
}

/// Open `[`/`{` minus closing ones outside strings and comments.
fn bracket_depth(s: &str) -> i32 {
    let (mut d, mut q) = (0i32, None::<char>);
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (q, c) {
            (None, '#') => break,
            (None, '"' | '\'') => q = Some(c),
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some(x), c) if c == x => q = None,
            (None, '[' | '{') => d += 1,
            (None, ']' | '}') => d -= 1,
            _ => {}
        }
    }
    d
}

/// The value at a dotted path of a parsed document.
fn at<'a>(j: &'a specula_core::Json, path: &str) -> Option<&'a specula_core::Json> {
    path.split('.').try_fold(j, |j, k| j.get(k))
}

/// The byte where a value ends (before any trailing comment).
fn value_end(rest: &str) -> usize {
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
        let q = b[i];
        i += 1;
        while i < b.len() && b[i] != q {
            if b[i] == b'\\' && q == b'"' {
                i += 1;
            }
            i += 1;
        }
        return (i + 1).min(b.len());
    }
    rest.find('#').map_or(rest.trim_end().len(), |h| rest[..h].trim_end().len())
}

/// `text` with `path` set to the TOML literal `literal`.
pub fn set(text: &str, path: &str, literal: &str) -> Result<String> {
    let (table, key) = match path.rsplit_once('.') {
        Some((t, k)) => (t, k),
        None => ("", path),
    };
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(TasteError::new("bad_key", format!("'{path}': a dotted path of bare keys")));
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut current = String::new();
    let (mut start, mut end) = (if table.is_empty() { Some(0) } else { None }, None);
    let mut found = None;
    for (i, kind) in classify(&lines).into_iter().enumerate() {
        match kind {
            Line::Header(h) => {
                if start.is_some() && end.is_none() && current == table {
                    end = Some(i);
                }
                current = h;
                if current == table && start.is_none() {
                    start = Some(i + 1);
                }
            }
            Line::Key(k) if current == table && found.is_none() && k == key => found = Some(i),
            _ => {}
        }
    }
    let mut out = String::with_capacity(text.len() + literal.len() + 16);
    if let Some(i) = found {
        for (k, l) in lines.iter().enumerate() {
            if k != i {
                out.push_str(l);
                continue;
            }
            let eq = l.find('=').expect("found an =");
            let (head, rest) = l.split_at(eq + 1);
            let end = value_end(rest);
            out.push_str(head);
            out.push(' ');
            out.push_str(literal);
            out.push_str(&rest[end..]);
        }
    } else if let Some(s) = start {
        // The end of the table: before the next header, after its last line that says something.
        let stop = end.unwrap_or(lines.len());
        let mut at = s;
        for (k, l) in lines.iter().enumerate().take(stop).skip(s) {
            if !l.trim().is_empty() {
                at = k + 1;
            }
        }
        for (k, l) in lines.iter().enumerate() {
            if k == at {
                out.push_str(&format!("{key} = {literal}{eol}"));
            }
            out.push_str(l);
            if k + 1 == lines.len() && k + 1 == at {
                if !l.ends_with('\n') {
                    out.push_str(eol);
                }
                out.push_str(&format!("{key} = {literal}{eol}"));
            }
        }
        if lines.is_empty() {
            out.push_str(&format!("{key} = {literal}{eol}"));
        }
    } else {
        out.push_str(text);
        if !text.is_empty() && !text.ends_with('\n') {
            out.push_str(eol);
        }
        if !text.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&format!("[{table}]{eol}{key} = {literal}{eol}"));
    }
    let parsed = specula_core::toml::parse(&out).map_err(|e| TasteError::new("bad_toml", format!("setting {path} would leave the file unparseable: {e}")))?;
    // What was meant must be what the file now says (a key written as a dotted key or inside an
    // inline table isn't found as a line, and setting it beside them would mean something else).
    let want = specula_core::toml::parse(&format!("v = {literal}\n")).map_err(|e| TasteError::new("bad_toml", format!("{literal} isn't a TOML value: {e}")))?;
    if at(&parsed, path) != want.get("v") {
        return Err(TasteError::new("unsupported_toml", format!("{path} is written in a form this editor doesn't change (a dotted key or an inline table): edit it by hand, or as [{table}] {key} = ..."))
            .at(path.to_owned()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# rules\n[motion]\ncontact_m = 0.025\n\n[stage.game]\n# the moon\nkey_color = \"#d0d9f0\"  # low and blue\nkey_intensity = 0.55\n\n[[budget]]\nmax_triangles = 3000\n";

    #[test]
    fn keys_are_replaced_added_and_tables_appended() {
        let t = set(TEXT, "stage.game.key_color", "\"#c0c8e8\"").unwrap();
        assert_eq!(t, TEXT.replace("\"#d0d9f0\"", "\"#c0c8e8\""), "only the value changes; the comment stays");
        let t = set(TEXT, "stage.game.ambient_intensity", "1.4").unwrap();
        assert!(t.contains("key_intensity = 0.55\nambient_intensity = 1.4\n\n[[budget]]"), "{t}");
        let t = set(TEXT, "ui.cant_afford", "\"#8a7a60\"").unwrap();
        assert!(t.starts_with(TEXT) && t.ends_with("\n[ui]\ncant_afford = \"#8a7a60\"\n"), "{t}");
        let j = specula_core::toml::parse(&t).unwrap();
        assert_eq!(j.get("ui").and_then(|u| u.get("cant_afford")).and_then(specula_core::Json::as_str), Some("#8a7a60"));
        // CRLF stays CRLF; a key in a different table of the same name isn't touched.
        let crlf = TEXT.replace('\n', "\r\n");
        let t = set(&crlf, "motion.contact_m", "0.03").unwrap();
        assert!(t.contains("contact_m = 0.03\r\n") && !t.contains("\r\r"));
        let t = set("[a]\nx = 1\n[b]\nx = 2\n", "b.x", "3").unwrap();
        assert_eq!(t, "[a]\nx = 1\n[b]\nx = 3\n");
        assert!(set(TEXT, "stage.game.bad key", "1").is_err());
        // Review probes: a table header inside a multi-line string, arrays of arrays across
        // lines, a dotted key and an inline table.
        let hostile = "[motion]\ndoc = \"\"\"\n[stage.game]\n\"\"\"\n";
        let t = set(hostile, "stage.game.key_color", "\"#112233\"").unwrap();
        let j = specula_core::toml::parse(&t).unwrap();
        assert_eq!(at(&j, "stage.game.key_color").and_then(specula_core::Json::as_str), Some("#112233"));
        assert!(at(&j, "motion.key_color").is_none(), "{t}");
        let arr = "[stage.game]\npalette = [\n  [0.1, 0.2],\n  [0.3]\n]\nkey_color = \"#000000\"\n";
        let t = set(arr, "stage.game.key_color", "\"#112233\"").unwrap();
        assert_eq!(t, arr.replace("#000000", "#112233"));
        assert!(set("[stage]\ngame.key_color = \"#000000\"\n", "stage.game.key_color", "\"#112233\"").is_err(), "refused, never silently wrong");
        assert!(set("[stage]\ngame = { key_color = \"#000000\" }\n", "stage.game.key_color", "\"#112233\"").is_err());
        assert_eq!(set(TEXT, "stage.game.key_color", "\"unterminated").unwrap_err().code, "bad_toml");
    }
}

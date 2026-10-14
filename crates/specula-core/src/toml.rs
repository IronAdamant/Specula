//! TOML subset parser for configuration files (`perf.toml`, later `style.toml`).
//!
//! Supported (TOML 1.0 semantics): comments; key/value pairs with bare, quoted and dotted keys;
//! tables `[a.b]`; arrays of tables `[[a]]`; basic and literal strings, single- and multi-line,
//! with every TOML 1.0 escape (`\b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX`, line-ending backslash);
//! integers (decimal, `0x`, `0o`, `0b`, underscores); floats (fraction, exponent, `inf`, `nan`);
//! booleans; arrays (multi-line, trailing comma, comments inside); inline tables.
//! Not supported: dates and times (an `unsupported_datetime` error).
//!
//! Strict: a key or table defined twice, a value redefined as a table, a table extended in a way
//! TOML forbids, or anything malformed is an error with a machine-readable `code`, the 1-based
//! `line` and `column` (in characters) and the byte `offset`. Hostile input never panics: nesting
//! is limited to [`MAX_DEPTH`].
//!
//! The result is a [`Json`] object (tables keep their key order). Numbers become `f64`, so integers
//! beyond ±2^53 are rejected (`integer_precision`) rather than silently rounded.
//!
//! ```text
//! let cfg = specula_core::toml::parse("[budgets]\nframe_ms = 16.7\n")?;
//! assert_eq!(cfg.get("budgets").and_then(|b| b.get("frame_ms")).and_then(Json::as_f64), Some(16.7));
//! ```

use crate::Json;
use std::collections::HashMap;
use std::fmt;

/// Deepest nesting of tables, arrays and inline tables (keys and values together).
pub const MAX_DEPTH: usize = 128;

/// Largest integer an `f64` holds exactly (2^53).
const MAX_SAFE_INT: i64 = 1 << 53;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TomlError {
    /// Machine-readable reason, e.g. `duplicate_key`, `invalid_number`, `unterminated_string`.
    pub code: &'static str,
    pub message: String,
    /// 1-based line.
    pub line: usize,
    /// 1-based column, counted in characters.
    pub column: usize,
    /// Byte offset into the text.
    pub offset: usize,
}

impl TomlError {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("error", Json::from(self.code)),
            ("message", Json::from(self.message.as_str())),
            ("line", Json::from(self.line)),
            ("column", Json::from(self.column)),
            ("offset", Json::from(self.offset)),
        ])
    }
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at line {}, column {} ({})", self.message, self.line, self.column, self.code)
    }
}

impl std::error::Error for TomlError {}

/// Parses a TOML document into a JSON object.
pub fn parse(text: &str) -> Result<Json, TomlError> {
    let mut p = Parser { s: text, b: text.as_bytes(), i: 0, root: Table::new(Kind::Header), current: Vec::new() };
    p.document()?;
    Ok(p.root.into_json())
}

/// How a table came to exist; decides what may still be added to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Defined by a `[header]` (or the root).
    Header,
    /// Created as the parent of a header (`[a.b]` creates `a`); a later `[a]` may define it.
    Implicit,
    /// Created by a dotted key (`a.b = 1` creates `a`); sub-tables only, never a `[a]` header.
    Dotted,
    /// An inline table: complete as written.
    Inline,
}

enum Item {
    Value(Json),
    Table(Table),
    /// `[[name]]`
    TableArray(Vec<Table>),
    /// A literal array (may hold inline tables).
    Array(Vec<Item>),
}

impl Item {
    fn into_json(self) -> Json {
        match self {
            Item::Value(v) => v,
            Item::Table(t) => t.into_json(),
            Item::TableArray(ts) => Json::Arr(ts.into_iter().map(Table::into_json).collect()),
            Item::Array(items) => Json::Arr(items.into_iter().map(Item::into_json).collect()),
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            Item::Value(_) => "a value",
            Item::Table(_) => "a table",
            Item::TableArray(_) => "an array of tables",
            Item::Array(_) => "an array",
        }
    }
}

struct Table {
    entries: Vec<(String, Item)>,
    index: HashMap<String, usize>,
    kind: Kind,
}

impl Table {
    fn new(kind: Kind) -> Table {
        Table { entries: Vec::new(), index: HashMap::new(), kind }
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut Item> {
        let i = *self.index.get(key)?;
        Some(&mut self.entries[i].1)
    }

    /// Inserts a new key (the caller checked it is absent) and returns its item.
    fn insert(&mut self, key: &str, item: Item) -> &mut Item {
        self.index.insert(key.to_owned(), self.entries.len());
        self.entries.push((key.to_owned(), item));
        &mut self.entries.last_mut().expect("just pushed").1
    }

    fn freeze(&mut self) {
        self.kind = Kind::Inline;
        for (_, item) in &mut self.entries {
            freeze_item(item);
        }
    }

    fn into_json(self) -> Json {
        Json::Obj(self.entries.into_iter().map(|(k, v)| (k, v.into_json())).collect())
    }
}

fn freeze_item(item: &mut Item) {
    match item {
        Item::Table(t) => t.freeze(),
        Item::Array(items) => items.iter_mut().for_each(freeze_item),
        Item::TableArray(ts) => ts.iter_mut().for_each(Table::freeze),
        Item::Value(_) => {}
    }
}

/// A key part and where it starts (for error positions).
struct Key {
    name: String,
    at: usize,
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
    root: Table,
    /// Key path of the current `[table]` / `[[table]]`.
    current: Vec<String>,
}

fn err_at(s: &str, offset: usize, code: &'static str, message: impl Into<String>) -> TomlError {
    let before = &s[..offset.min(s.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |n| n + 1);
    let column = before[line_start..].chars().count() + 1;
    TomlError { code, message: message.into(), line, column, offset }
}

fn is_bare(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

fn is_control(c: char) -> bool {
    (c < ' ' && c != '\t') || c == '\u{7f}'
}

impl Parser<'_> {
    fn err(&self, code: &'static str, message: impl Into<String>) -> TomlError {
        err_at(self.s, self.i, code, message)
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn peek_at(&self, n: usize) -> Option<u8> {
        self.b.get(self.i + n).copied()
    }

    fn starts_with(&self, pat: &str) -> bool {
        self.b[self.i..].starts_with(pat.as_bytes())
    }

    /// Spaces and tabs.
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    /// A newline (`\n` or `\r\n`) if one is next.
    fn newline(&mut self) -> Result<bool, TomlError> {
        match self.peek() {
            Some(b'\n') => {
                self.i += 1;
                Ok(true)
            }
            Some(b'\r') if self.peek_at(1) == Some(b'\n') => {
                self.i += 2;
                Ok(true)
            }
            Some(b'\r') => Err(self.err("bare_carriage_return", "a carriage return must be followed by a newline")),
            _ => Ok(false),
        }
    }

    /// A `#` comment up to (not including) the end of the line, if one is next.
    fn comment(&mut self) -> Result<(), TomlError> {
        if self.peek() != Some(b'#') {
            return Ok(());
        }
        self.i += 1;
        while let Some(c) = self.s[self.i..].chars().next() {
            if c == '\n' || (c == '\r' && self.peek_at(1) == Some(b'\n')) {
                break;
            }
            if is_control(c) {
                return Err(self.err("control_character", "control characters are not allowed in comments"));
            }
            self.i += c.len_utf8();
        }
        Ok(())
    }

    /// Whitespace, newlines and comments (inside arrays).
    fn blank(&mut self) -> Result<(), TomlError> {
        loop {
            self.ws();
            self.comment()?;
            if !self.newline()? {
                return Ok(());
            }
        }
    }

    /// After a header or key/value: optional whitespace and comment, then a newline or the end.
    fn end_of_line(&mut self) -> Result<(), TomlError> {
        self.ws();
        self.comment()?;
        if self.i < self.b.len() && !self.newline()? {
            return Err(self.err("expected_newline", "expected the end of the line"));
        }
        Ok(())
    }

    fn document(&mut self) -> Result<(), TomlError> {
        loop {
            self.ws();
            match self.peek() {
                None => return Ok(()),
                Some(b'#') => {
                    self.comment()?;
                    continue;
                }
                Some(b'\n' | b'\r') => {
                    self.newline()?;
                    continue;
                }
                Some(b'[') => {
                    if self.peek_at(1) == Some(b'[') {
                        self.table_array_header()?;
                    } else {
                        self.table_header()?;
                    }
                }
                Some(_) => {
                    let keys = self.key()?;
                    let depth = self.current.len() + keys.len();
                    self.ws();
                    if self.peek() != Some(b'=') {
                        return Err(self.err("expected_equals", "expected '=' after a key"));
                    }
                    self.i += 1;
                    self.ws();
                    let value = self.value(depth)?;
                    let path = self.current.clone();
                    let table = table_at(&mut self.root, &path);
                    insert_dotted(self.s, table, keys, value)?;
                }
            }
            self.end_of_line()?;
        }
    }

    fn header_keys(&mut self) -> Result<Vec<Key>, TomlError> {
        self.ws();
        let keys = self.key()?;
        self.ws();
        Ok(keys)
    }

    /// `[a.b.c]`
    fn table_header(&mut self) -> Result<(), TomlError> {
        self.i += 1;
        let keys = self.header_keys()?;
        if self.peek() != Some(b']') {
            return Err(self.err("expected_bracket", "expected ']' to close the table header"));
        }
        self.i += 1;
        let s = self.s;
        let mut t = &mut self.root;
        let last = keys.len() - 1;
        for (n, key) in keys.iter().enumerate() {
            t = header_step(s, t, key, n == last)?;
        }
        self.current = keys.into_iter().map(|k| k.name).collect();
        Ok(())
    }

    /// `[[a.b.c]]`
    fn table_array_header(&mut self) -> Result<(), TomlError> {
        self.i += 2;
        let keys = self.header_keys()?;
        if !self.starts_with("]]") {
            return Err(self.err("expected_bracket", "expected ']]' to close the array-of-tables header"));
        }
        self.i += 2;
        let s = self.s;
        let mut t = &mut self.root;
        let (last, parents) = keys.split_last().expect("a key has at least one part");
        for key in parents {
            t = header_step(s, t, key, false)?;
        }
        match slot(t, &last.name) {
            Slot::Absent => {
                t.insert(&last.name, Item::TableArray(vec![Table::new(Kind::Header)]));
            }
            Slot::TableArray => match t.get_mut(&last.name) {
                Some(Item::TableArray(ts)) => ts.push(Table::new(Kind::Header)),
                _ => unreachable!("checked by slot()"),
            },
            Slot::Table(_) => return Err(conflict(s, last, "a table", "an array of tables")),
            Slot::Other(what) => return Err(conflict(s, last, what, "an array of tables")),
        }
        self.current = keys.into_iter().map(|k| k.name).collect();
        Ok(())
    }

    /// A dotted key: `a`, `"a b".c`, `'x'.y . z`.
    fn key(&mut self) -> Result<Vec<Key>, TomlError> {
        let mut keys = Vec::new();
        loop {
            let at = self.i;
            let name = match self.peek() {
                Some(b'"') => {
                    if self.starts_with("\"\"\"") {
                        return Err(self.err("invalid_key", "keys cannot be multi-line strings"));
                    }
                    self.basic_string()?
                }
                Some(b'\'') => {
                    if self.starts_with("'''") {
                        return Err(self.err("invalid_key", "keys cannot be multi-line strings"));
                    }
                    self.literal_string()?
                }
                Some(c) if is_bare(c) => {
                    while self.peek().is_some_and(is_bare) {
                        self.i += 1;
                    }
                    self.s[at..self.i].to_owned()
                }
                _ => return Err(self.err("invalid_key", "expected a key (letters, digits, _ and -, or a quoted string)")),
            };
            keys.push(Key { name, at });
            if keys.len() > MAX_DEPTH {
                return Err(err_at(self.s, at, "too_deep", format!("keys may have at most {MAX_DEPTH} parts")));
            }
            let save = self.i;
            self.ws();
            if self.peek() == Some(b'.') {
                self.i += 1;
                self.ws();
            } else {
                self.i = save;
                return Ok(keys);
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Item, TomlError> {
        if depth > MAX_DEPTH {
            return Err(self.err("too_deep", format!("nesting deeper than {MAX_DEPTH}")));
        }
        match self.peek() {
            Some(b'"') if self.starts_with("\"\"\"") => self.multiline_basic().map(|s| Item::Value(Json::Str(s))),
            Some(b'"') => self.basic_string().map(|s| Item::Value(Json::Str(s))),
            Some(b'\'') if self.starts_with("'''") => self.multiline_literal().map(|s| Item::Value(Json::Str(s))),
            Some(b'\'') => self.literal_string().map(|s| Item::Value(Json::Str(s))),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.inline_table(depth),
            Some(b't') if self.starts_with("true") && !self.peek_at(4).is_some_and(is_bare) => {
                self.i += 4;
                Ok(Item::Value(Json::Bool(true)))
            }
            Some(b'f') if self.starts_with("false") && !self.peek_at(5).is_some_and(is_bare) => {
                self.i += 5;
                Ok(Item::Value(Json::Bool(false)))
            }
            Some(c) if c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'i' | b'n') => self.number(),
            None => Err(self.err("expected_value", "expected a value, found the end of the file")),
            Some(b'\n' | b'\r') => Err(self.err("expected_value", "expected a value on the same line as its key")),
            Some(_) => Err(self.err("expected_value", "expected a value (string, number, boolean, array or inline table)")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Item, TomlError> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.blank()?;
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(Item::Array(items));
            }
            items.push(self.value(depth + 1)?);
            self.blank()?;
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Item::Array(items));
                }
                _ => return Err(self.err("expected_comma", "expected ',' or ']' in an array")),
            }
        }
    }

    fn inline_table(&mut self, depth: usize) -> Result<Item, TomlError> {
        self.i += 1;
        let mut table = Table::new(Kind::Dotted);
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            table.freeze();
            return Ok(Item::Table(table));
        }
        loop {
            self.ws();
            if self.peek() == Some(b'}') {
                return Err(self.err("trailing_comma", "inline tables may not end with a comma"));
            }
            let keys = self.key()?;
            self.ws();
            if self.peek() != Some(b'=') {
                return Err(self.err("expected_equals", "expected '=' after a key"));
            }
            self.i += 1;
            self.ws();
            let value = self.value(depth + keys.len())?;
            insert_dotted(self.s, &mut table, keys, value)?;
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    table.freeze();
                    return Ok(Item::Table(table));
                }
                Some(b'\n' | b'\r') => {
                    return Err(self.err("expected_brace", "inline tables must be on one line"));
                }
                _ => return Err(self.err("expected_comma", "expected ',' or '}' in an inline table")),
            }
        }
    }

    fn number(&mut self) -> Result<Item, TomlError> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'+' | b'-' | b'.' | b':')) {
            self.i += 1;
        }
        let tok = &self.s[start..self.i];
        let bad = |code: &'static str, msg: String| err_at(self.s, start, code, msg);
        let looks_like_date = {
            let b = tok.as_bytes();
            tok.contains(':') || (b.len() >= 5 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-')
        };
        if looks_like_date {
            return Err(bad("unsupported_datetime", format!("dates and times are not supported ('{tok}')")));
        }
        let (sign, body) = match tok.as_bytes().first() {
            Some(b'+') => (1.0, &tok[1..]),
            Some(b'-') => (-1.0, &tok[1..]),
            _ => (0.0, tok),
        };
        match body {
            "inf" => return Ok(Item::Value(Json::Num(if sign < 0.0 { f64::NEG_INFINITY } else { f64::INFINITY }))),
            "nan" => return Ok(Item::Value(Json::Num(f64::NAN))),
            _ => {}
        }
        let radix = match body.get(..2) {
            Some("0x") => Some((16, "hexadecimal")),
            Some("0o") => Some((8, "octal")),
            Some("0b") => Some((2, "binary")),
            _ => None,
        };
        if let Some((radix, what)) = radix {
            let digits = &body[2..];
            if sign != 0.0 {
                return Err(bad("invalid_number", format!("{what} integers cannot have a sign ('{tok}')")));
            }
            if !underscored(digits, |c| (c as char).is_digit(radix)) {
                return Err(bad("invalid_number", format!("invalid {what} integer '{tok}'")));
            }
            let clean: String = digits.chars().filter(|&c| c != '_').collect();
            let n = u64::from_str_radix(&clean, radix)
                .ok()
                .filter(|&n| n <= i64::MAX as u64)
                .ok_or_else(|| bad("integer_overflow", format!("'{tok}' does not fit in a 64-bit integer")))?;
            return int_value(n as i64).map_err(|m| bad("integer_precision", m));
        }
        // Decimal: int part, optional fraction, optional exponent.
        let (mantissa, exponent) = match body.find(['e', 'E']) {
            Some(e) => (&body[..e], Some(&body[e + 1..])),
            None => (body, None),
        };
        let (int_part, frac) = match mantissa.find('.') {
            Some(d) => (&mantissa[..d], Some(&mantissa[d + 1..])),
            None => (mantissa, None),
        };
        let digit = |c: u8| c.is_ascii_digit();
        let int_ok = underscored(int_part, digit) && (int_part == "0" || !int_part.starts_with('0'));
        let frac_ok = frac.is_none_or(|f| underscored(f, digit));
        let exp_ok = exponent.is_none_or(|e| {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            underscored(e, digit)
        });
        if !(int_ok && frac_ok && exp_ok) {
            let msg = if int_ok && int_part.is_empty() { "missing digits" } else { "malformed number" };
            return Err(bad("invalid_number", format!("{msg}: '{tok}'")));
        }
        let clean: String = tok.chars().filter(|&c| c != '_').collect();
        if frac.is_none() && exponent.is_none() {
            let n: i64 = clean
                .parse()
                .map_err(|_| bad("integer_overflow", format!("'{tok}' does not fit in a 64-bit integer")))?;
            return int_value(n).map_err(|m| bad("integer_precision", m));
        }
        let f: f64 = clean.parse().map_err(|_| bad("invalid_number", format!("malformed float '{tok}'")))?;
        Ok(Item::Value(Json::Num(f)))
    }

    /// `"..."` with escapes; `self.i` at the opening quote.
    fn basic_string(&mut self) -> Result<String, TomlError> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.s[self.i..].chars().next() else {
                return Err(self.err("unterminated_string", "unterminated string"));
            };
            match c {
                '"' => {
                    self.i += 1;
                    return Ok(out);
                }
                '\\' => out.push(self.escape()?),
                '\n' | '\r' => return Err(self.err("unterminated_string", "strings in \"...\" must end on the same line")),
                c if is_control(c) => return Err(self.err("control_character", "control characters must be escaped")),
                c => {
                    out.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }

    /// One escape sequence; `self.i` at the backslash.
    fn escape(&mut self) -> Result<char, TomlError> {
        let at = self.i;
        self.i += 1;
        let c = match self.peek() {
            Some(b'b') => '\u{8}',
            Some(b't') => '\t',
            Some(b'n') => '\n',
            Some(b'f') => '\u{c}',
            Some(b'r') => '\r',
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'u') | Some(b'U') => {
                let len = if self.peek() == Some(b'u') { 4 } else { 8 };
                let hex = self.s.get(self.i + 1..self.i + 1 + len).filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()));
                let ch = hex.and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32);
                let Some(ch) = ch else {
                    return Err(err_at(self.s, at, "invalid_escape", format!("\\{} needs {len} hex digits of a Unicode scalar value", self.peek().unwrap_or(b'u') as char)));
                };
                self.i += 1 + len;
                return Ok(ch);
            }
            _ => return Err(err_at(self.s, at, "invalid_escape", "unknown escape sequence")),
        };
        self.i += 1;
        Ok(c)
    }

    /// `'...'`, no escapes; `self.i` at the opening quote.
    fn literal_string(&mut self) -> Result<String, TomlError> {
        self.i += 1;
        let start = self.i;
        loop {
            let Some(c) = self.s[self.i..].chars().next() else {
                return Err(self.err("unterminated_string", "unterminated string"));
            };
            match c {
                '\'' => {
                    let out = self.s[start..self.i].to_owned();
                    self.i += 1;
                    return Ok(out);
                }
                '\n' | '\r' => return Err(self.err("unterminated_string", "strings in '...' must end on the same line")),
                c if is_control(c) => return Err(self.err("control_character", "control characters are not allowed in literal strings")),
                c => self.i += c.len_utf8(),
            }
        }
    }

    /// Skips the newline right after an opening `"""` / `'''`.
    fn skip_first_newline(&mut self) -> Result<(), TomlError> {
        self.newline().map(|_| ())
    }

    /// At a run of `quote` characters inside a multi-line string: the string ends here if the run
    /// is 3..=5 long (the extra one or two belong to the content). Returns the content quotes.
    fn closing_run(&mut self, quote: u8) -> Result<Option<usize>, TomlError> {
        let run = self.b[self.i..].iter().take_while(|&&c| c == quote).count();
        if run < 3 {
            return Ok(None);
        }
        if run > 5 {
            return Err(self.err("unterminated_string", "too many quotes in a row in a multi-line string"));
        }
        self.i += run;
        Ok(Some(run - 3))
    }

    fn multiline_basic(&mut self) -> Result<String, TomlError> {
        self.i += 3;
        self.skip_first_newline()?;
        let mut out = String::new();
        loop {
            let Some(c) = self.s[self.i..].chars().next() else {
                return Err(self.err("unterminated_string", "unterminated multi-line string"));
            };
            match c {
                '"' => match self.closing_run(b'"')? {
                    Some(extra) => {
                        out.extend(std::iter::repeat_n('"', extra));
                        return Ok(out);
                    }
                    None => {
                        out.push('"');
                        self.i += 1;
                    }
                },
                '\\' => {
                    // A line-ending backslash trims the newline and all whitespace after it.
                    let after = &self.b[self.i + 1..];
                    let spaces = after.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
                    let rest = &after[spaces..];
                    if rest.starts_with(b"\n") || rest.starts_with(b"\r\n") {
                        self.i += 1 + spaces;
                        loop {
                            self.ws();
                            if !self.newline()? {
                                break;
                            }
                        }
                    } else {
                        out.push(self.escape()?);
                    }
                }
                '\r' | '\n' => {
                    self.newline()?;
                    out.push('\n');
                }
                c if is_control(c) => return Err(self.err("control_character", "control characters must be escaped")),
                c => {
                    out.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }

    fn multiline_literal(&mut self) -> Result<String, TomlError> {
        self.i += 3;
        self.skip_first_newline()?;
        let mut out = String::new();
        loop {
            let Some(c) = self.s[self.i..].chars().next() else {
                return Err(self.err("unterminated_string", "unterminated multi-line string"));
            };
            match c {
                '\'' => match self.closing_run(b'\'')? {
                    Some(extra) => {
                        out.extend(std::iter::repeat_n('\'', extra));
                        return Ok(out);
                    }
                    None => {
                        out.push('\'');
                        self.i += 1;
                    }
                },
                '\r' | '\n' => {
                    self.newline()?;
                    out.push('\n');
                }
                c if is_control(c) => return Err(self.err("control_character", "control characters are not allowed in literal strings")),
                c => {
                    out.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }
}

/// Digits matching `digit`, with single underscores only between digits.
fn underscored(s: &str, digit: impl Fn(u8) -> bool) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && digit(b[0])
        && digit(b[b.len() - 1])
        && b.iter().all(|&c| c == b'_' || digit(c))
        && !s.contains("__")
}

fn int_value(n: i64) -> Result<Item, String> {
    if !(-MAX_SAFE_INT..=MAX_SAFE_INT).contains(&n) {
        return Err(format!("{n} is beyond ±2^53, which a JSON number cannot hold exactly"));
    }
    Ok(Item::Value(Json::Num(n as f64)))
}

/// The table a `[header]` path names (it exists: the header created it).
fn table_at<'t>(root: &'t mut Table, path: &[String]) -> &'t mut Table {
    let mut t = root;
    for key in path {
        t = match t.get_mut(key) {
            Some(Item::Table(t)) => t,
            Some(Item::TableArray(ts)) => ts.last_mut().expect("arrays of tables are never empty"),
            _ => unreachable!("headers create their tables"),
        };
    }
    t
}

/// What a key holds in a table, without borrowing it.
enum Slot {
    Absent,
    Table(Kind),
    TableArray,
    Other(&'static str),
}

fn slot(t: &Table, key: &str) -> Slot {
    match t.index.get(key).map(|&i| &t.entries[i].1) {
        None => Slot::Absent,
        Some(Item::Table(tab)) => Slot::Table(tab.kind),
        Some(Item::TableArray(_)) => Slot::TableArray,
        Some(other) => Slot::Other(other.describe()),
    }
}

/// The table under `key` (the last one of an array of tables); the caller checked it exists.
fn child<'t>(t: &'t mut Table, key: &str) -> &'t mut Table {
    match t.get_mut(key) {
        Some(Item::Table(tab)) => tab,
        Some(Item::TableArray(ts)) => ts.last_mut().expect("arrays of tables are never empty"),
        _ => unreachable!("checked by slot()"),
    }
}

fn add_table<'t>(t: &'t mut Table, key: &str, kind: Kind) -> &'t mut Table {
    match t.insert(key, Item::Table(Table::new(kind))) {
        Item::Table(t) => t,
        _ => unreachable!("just inserted a table"),
    }
}

fn frozen(s: &str, key: &Key) -> TomlError {
    err_at(s, key.at, "inline_table_frozen", format!("'{}' is an inline table: it cannot be extended", key.name))
}

fn conflict(s: &str, key: &Key, is: &str, wanted: &str) -> TomlError {
    err_at(s, key.at, "key_conflict", format!("'{}' is already {is}, so it cannot be {wanted} here", key.name))
}

/// One step of a header path (`last`: the table the header defines): enters the existing table,
/// or creates it.
fn header_step<'t>(s: &str, t: &'t mut Table, key: &Key, last: bool) -> Result<&'t mut Table, TomlError> {
    match (slot(t, &key.name), last) {
        (Slot::Absent, _) => return Ok(add_table(t, &key.name, if last { Kind::Header } else { Kind::Implicit })),
        (Slot::Table(Kind::Inline), _) => return Err(frozen(s, key)),
        (Slot::Table(Kind::Header), true) => {
            return Err(err_at(s, key.at, "duplicate_table", format!("table '{}' is defined twice", key.name)));
        }
        (Slot::Table(Kind::Dotted), true) => {
            let msg = format!("table '{}' was already defined by dotted keys", key.name);
            return Err(err_at(s, key.at, "duplicate_table", msg));
        }
        (Slot::Table(_), _) | (Slot::TableArray, false) => {}
        (Slot::TableArray, true) => return Err(conflict(s, key, "an array of tables", "a table")),
        (Slot::Other(what), _) => return Err(conflict(s, key, what, "a table")),
    }
    let tab = child(t, &key.name);
    if last {
        tab.kind = Kind::Header; // it was implicit
    }
    Ok(tab)
}

/// Inserts `keys = value` (dotted keys create tables) into `table`.
fn insert_dotted(s: &str, table: &mut Table, keys: Vec<Key>, value: Item) -> Result<(), TomlError> {
    let (last, parents) = keys.split_last().expect("a key has at least one part");
    let mut t = table;
    for key in parents {
        t = match slot(t, &key.name) {
            Slot::Absent => add_table(t, &key.name, Kind::Dotted),
            Slot::Table(Kind::Dotted) => child(t, &key.name),
            Slot::Table(Kind::Inline) => return Err(frozen(s, key)),
            Slot::Table(_) => return Err(conflict(s, key, "a table defined by a [header]", "extended by dotted keys")),
            Slot::TableArray => return Err(conflict(s, key, "an array of tables", "extended by dotted keys")),
            Slot::Other(what) => return Err(conflict(s, key, what, "a table")),
        };
    }
    if !matches!(slot(t, &last.name), Slot::Absent) {
        return Err(err_at(s, last.at, "duplicate_key", format!("key '{}' is defined twice", last.name)));
    }
    t.insert(&last.name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn ok(text: &str) -> Json {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    fn code(text: &str) -> &'static str {
        match parse(text) {
            Ok(v) => panic!("{text:?} parsed as {v}"),
            Err(e) => e.code,
        }
    }

    #[test]
    fn key_values_and_types() {
        let v = ok("# config\na = 1\nb = -2.5 # trailing\nc = true\nd = false\ne = \"hi\"\nf = 'raw\\n'\n");
        assert_eq!(v.to_string(), r#"{"a":1,"b":-2.5,"c":true,"d":false,"e":"hi","f":"raw\\n"}"#);
        assert_eq!(ok("").to_string(), "{}");
        assert_eq!(ok("\r\n\r\n# only comments\r\n").to_string(), "{}");
        assert_eq!(ok("x=1").to_string(), r#"{"x":1}"#);
    }

    #[test]
    fn keys_bare_quoted_dotted() {
        let v = ok("bare-key_1 = 1\n\"quoted key\" = 2\n'lit.key' = 3\n1234 = 4\na . b.\"c d\" = 5\n\"\" = 6\n");
        assert_eq!(v.to_string(), r#"{"bare-key_1":1,"quoted key":2,"lit.key":3,"1234":4,"a":{"b":{"c d":5}},"":6}"#);
        assert_eq!(ok("3.14159 = 'pi'").to_string(), r#"{"3":{"14159":"pi"}}"#);
        assert_eq!(code("a b = 1"), "expected_equals");
        assert_eq!(code("= 1"), "invalid_key");
        assert_eq!(code("a = "), "expected_value");
        assert_eq!(code("a =\n1"), "expected_value");
        assert_eq!(code("a = 1 b = 2"), "expected_newline");
        assert_eq!(code("\"\"\"a\"\"\" = 1"), "invalid_key");
    }

    #[test]
    fn tables_and_arrays_of_tables() {
        let v = ok("top = 0\n[a]\nx = 1\n[a.b]\ny = 2\n[ c . d ]\nz = 3\n[[e]]\nn = 1\n[[e]]\nn = 2\n[e.sub]\nq = 1\n");
        assert_eq!(
            v.to_string(),
            r#"{"top":0,"a":{"x":1,"b":{"y":2}},"c":{"d":{"z":3}},"e":[{"n":1},{"n":2,"sub":{"q":1}}]}"#
        );
        // A parent created implicitly may be defined later, once.
        assert_eq!(ok("[x.y.z]\n[x]\na = 1").to_string(), r#"{"x":{"y":{"z":{}},"a":1}}"#);
        // Sub-tables of dotted-key tables are fine.
        assert_eq!(ok("[f]\napple.color = 'red'\n[f.apple.texture]\nsmooth = true").to_string(),
            r#"{"f":{"apple":{"color":"red","texture":{"smooth":true}}}}"#);
        // Nested arrays of tables.
        assert_eq!(ok("[[a]]\n[[a.b]]\nx=1\n[[a.b]]\nx=2\n[[a]]").to_string(), r#"{"a":[{"b":[{"x":1},{"x":2}]},{}]}"#);
    }

    #[test]
    fn duplicates_and_conflicts_are_rejected() {
        assert_eq!(code("a = 1\na = 2"), "duplicate_key");
        assert_eq!(code("[a]\n[a]"), "duplicate_table");
        assert_eq!(code("[a]\nb = 1\n[a.b]"), "key_conflict");
        assert_eq!(code("a.b = 1\n[a]"), "duplicate_table");
        assert_eq!(code("a = 1\na.b = 2"), "key_conflict");
        assert_eq!(code("[a.b]\n[a]\nb.c = 1"), "key_conflict");
        assert_eq!(code("a = {b = 1}\na.c = 2"), "inline_table_frozen");
        assert_eq!(code("a = {b = 1}\n[a.c]"), "inline_table_frozen");
        assert_eq!(code("a = [1]\n[[a]]"), "key_conflict");
        assert_eq!(code("[[a]]\n[a]"), "key_conflict");
        assert_eq!(code("[a]\n[[a]]"), "key_conflict");
        assert_eq!(code("a = {b = 1, b = 2}"), "duplicate_key");
        assert_eq!(code("[a]\nx = 1\n[b]\n[a]"), "duplicate_table");
        assert_eq!(code("\"a\" = 1\na = 2"), "duplicate_key");
    }

    #[test]
    fn strings_and_escapes() {
        let v = ok(r#"s = "tab\tnl\nq\"bs\\u\u00e9U\U0001F600b\bf\fr\r""#);
        assert_eq!(v.get("s").and_then(Json::as_str), Some("tab\tnl\nq\"bs\\u\u{e9}U\u{1F600}b\u{8}f\u{c}r\r"));
        assert_eq!(ok("s = 'C:\\path\\x'").get("s").and_then(Json::as_str), Some("C:\\path\\x"));
        assert_eq!(ok("s = \"\u{e9}t\u{e9} \u{2603}\"").get("s").and_then(Json::as_str), Some("\u{e9}t\u{e9} \u{2603}"));
        let ml = ok("s = \"\"\"\nline one\nline \\\n    two\\\n\n   three \"\" \"\"\"\"\"");
        assert_eq!(ml.get("s").and_then(Json::as_str), Some("line one\nline twothree \"\" \"\""));
        let ml = ok("s = '''\nraw \\n ''\nend'''''");
        assert_eq!(ml.get("s").and_then(Json::as_str), Some("raw \\n ''\nend''"));
        assert_eq!(ok("s = \"\"\"\r\nwin\r\n\"\"\"").get("s").and_then(Json::as_str), Some("win\n"));
        assert_eq!(code(r#"s = "\x41""#), "invalid_escape");
        assert_eq!(code(r#"s = "\uD800""#), "invalid_escape");
        assert_eq!(code(r#"s = "\u12""#), "invalid_escape");
        assert_eq!(code("s = \"open"), "unterminated_string");
        assert_eq!(code("s = \"two\nlines\""), "unterminated_string");
        assert_eq!(code("s = 'open"), "unterminated_string");
        assert_eq!(code("s = \"\"\"open"), "unterminated_string");
        assert_eq!(code("s = \"\"\"a\"\"\"\"\"\""), "unterminated_string");
        assert_eq!(code("s = \"a\u{1}b\""), "control_character");
        assert_eq!(code("s = 'a\u{7f}'"), "control_character");
        assert_eq!(code("# bad \u{0} comment"), "control_character");
        assert_eq!(code("a = 1\rb = 2"), "bare_carriage_return");
    }

    #[test]
    fn numbers() {
        let v = ok("a = +99\nb = -17\nc = 0\nd = 1_000\ne = 0xDEAD_beef\nf = 0o755\ng = 0b1101\nh = 3.25\ni = -0.01\n\
                    j = 5e+22\nk = 1e06\nl = -2E-2\nm = 6.626e-34\nn = 224_617.445_991\no = inf\np = -inf\nq = nan\nr = -0.0\n");
        let f = |k: &str| v.get(k).and_then(Json::as_f64).unwrap();
        assert_eq!([f("a"), f("b"), f("c"), f("d")], [99.0, -17.0, 0.0, 1000.0]);
        assert_eq!([f("e"), f("f"), f("g")], [3_735_928_559.0, 493.0, 13.0]);
        assert_eq!([f("h"), f("i"), f("j"), f("k"), f("l")], [3.25, -0.01, 5e22, 1e6, -0.02]);
        assert_eq!([f("m"), f("n")], [6.626e-34, 224_617.445_991]);
        assert_eq!(v.get("o"), Some(&Json::Num(f64::INFINITY)));
        assert_eq!(v.get("p"), Some(&Json::Num(f64::NEG_INFINITY)));
        assert!(matches!(v.get("q"), Some(Json::Num(n)) if n.is_nan()));
        assert!(matches!(v.get("r"), Some(Json::Num(n)) if *n == 0.0 && n.is_sign_negative()));
        for bad in ["01", "1__0", "1_", "1.", ".5", "1.e5", "1e", "0x", "+0x10", "0xG", "1.2.3", "--1", "0b2", "1e_5", "+", "-"] {
            assert_eq!(code(&format!("a = {bad}")), "invalid_number", "{bad}");
        }
        assert_eq!(code("a = 9223372036854775808"), "integer_overflow");
        assert_eq!(code("a = 0xFFFFFFFFFFFFFFFFFF"), "integer_overflow");
        assert_eq!(code("a = 9007199254740993"), "integer_precision");
        assert_eq!(ok("a = 9007199254740992").get("a").and_then(Json::as_f64), Some(9_007_199_254_740_992.0));
        assert_eq!(code("a = 1979-05-27"), "unsupported_datetime");
        assert_eq!(code("a = 07:32:00"), "unsupported_datetime");
        assert_eq!(code("a = truthy"), "expected_value");
        assert_eq!(code("a = _1"), "expected_value");
        assert_eq!(code("a = infinity"), "invalid_number");
    }

    #[test]
    fn arrays_and_inline_tables() {
        let v = ok("a = [ 1, 2, 3 ]\nb = [\n  'x', # first\n  \"y\",\n\n]\nc = [[1, 2], ['a'], []]\nd = [{x = 1}, {y.z = 2}]\n\
                    e = { name = 'n', point = { x = 1, y = 2 }, list = [1] }\nf = {}\n");
        assert_eq!(
            v.to_string(),
            r#"{"a":[1,2,3],"b":["x","y"],"c":[[1,2],["a"],[]],"d":[{"x":1},{"y":{"z":2}}],"e":{"name":"n","point":{"x":1,"y":2},"list":[1]},"f":{}}"#
        );
        assert_eq!(code("a = [1 2]"), "expected_comma");
        assert_eq!(code("a = [1,,2]"), "expected_value");
        assert_eq!(code("a = [1"), "expected_comma");
        assert_eq!(code("a = {x = 1,}"), "trailing_comma");
        assert_eq!(code("a = {x = 1\n}"), "expected_brace");
        assert_eq!(code("a = {x = 1 y = 2}"), "expected_comma");
    }

    #[test]
    fn errors_carry_line_and_column() {
        let e = parse("a = 1\n[t]\n  b = \u{e9}\u{e9}").unwrap_err();
        assert_eq!((e.code, e.line, e.column), ("expected_value", 3, 7));
        let e = parse("x = 1\ny = 2\nx = 3").unwrap_err();
        assert_eq!((e.code, e.line, e.column, e.offset), ("duplicate_key", 3, 1, 12));
        let j = e.to_json();
        assert_eq!(j.get("error"), Some(&Json::from("duplicate_key")));
        assert_eq!(j.get("line"), Some(&Json::from(3)));
        assert!(e.to_string().contains("line 3, column 1"), "{e}");
    }

    #[test]
    fn nesting_is_limited() {
        let deep = format!("a = {}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert_eq!(code(&deep), "too_deep");
        let deep = format!("a = {}", "{b = ".repeat(10_000));
        assert_eq!(code(&deep), "too_deep");
        let header = format!("[{}]", vec!["a"; 10_000].join("."));
        assert_eq!(code(&header), "too_deep");
        let fine = format!("a = {}1{}", "[".repeat(100), "]".repeat(100));
        assert!(parse(&fine).is_ok());
    }

    /// Random bytes and mutations of valid documents never panic or hang.
    #[test]
    fn fuzz_never_panics() {
        let corpus = [
            "a = 1\n[t]\nb = 'x'\n[[arr]]\nc = [1, {d = 2}]\n",
            "s = \"\"\"\nml \\\n  x\"\"\"\nl = '''q'''\nf = -1.5e3\nh = 0xff\n",
            "[a.b.\"c\"]\nk.l = true\n# comment\nm = { n = [ [], [1,2,], ] }\n",
        ];
        let alphabet = b"[]{}=,.\"'\\#\n\r\t abc019_-+eExob:uU";
        let mut rng = Rng::named(7, "toml-fuzz");
        for round in 0..20_000 {
            let text: Vec<u8> = if round % 2 == 0 {
                let base = corpus[round % corpus.len()].as_bytes();
                let mut t = base.to_vec();
                for _ in 0..1 + rng.below(4) {
                    let at = rng.below(t.len() as u32 + 1) as usize;
                    match rng.below(3) {
                        0 if at < t.len() => {
                            t.remove(at);
                        }
                        1 => t.insert(at, alphabet[rng.below(alphabet.len() as u32) as usize]),
                        _ if at < t.len() => t[at] = alphabet[rng.below(alphabet.len() as u32) as usize],
                        _ => {}
                    }
                }
                t
            } else {
                let len = rng.below(64) as usize;
                (0..len)
                    .map(|_| match rng.below(10) {
                        0 => rng.below(256) as u8,
                        _ => alphabet[rng.below(alphabet.len() as u32) as usize],
                    })
                    .collect()
            };
            let text = String::from_utf8_lossy(&text);
            if let Err(e) = parse(&text) {
                assert!(e.offset <= text.len() && e.line >= 1 && e.column >= 1, "{e:?}");
            }
        }
    }
}

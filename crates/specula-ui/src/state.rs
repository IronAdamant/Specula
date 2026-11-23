//! Game state the UI binds to: a [`StateMap`] of named values the game fills each frame,
//! [`Template`]s (`"Dust {dust}"`) and [`Cond`]itions (`"paused"`, `"dust >= 50"`).
//!
//! Everything reuses its storage: setting a key that already exists, refilling a list, or
//! rendering a template into a `String` with capacity does not allocate (the HUD's per-frame
//! budget, F3).

use crate::error::{ErrorCode, UiError};
use crate::strings::{Strings, UiEntry};
pub use specula_lingua::Fmt;
use specula_lingua::{Arg, Category, NumberFormat};
use std::fmt::Write as _;

/// One bound value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Num(f64),
    /// Words the game supplies as they are (a save's name, a key's glyph): no string table
    /// translates them (`ui_check`'s `untranslated` notes them in the pseudo-locale).
    Str(String),
    List(StateList),
    /// A string-table entry and its arguments ([`StateMap::set_msg`]): shown in the UI's
    /// language, like a screen's `@key` text.
    Msg(Box<Msg>),
}

/// A string-table entry with its own arguments (its placeholders read `args` only).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Msg {
    pub key: String,
    pub args: StateMap,
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Msg(_) => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n),
            Value::Str(_) | Value::List(_) | Value::Msg(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    key: String,
    value: Value,
    /// Cleared by [`StateMap::reset`] (keeps the storage for reuse).
    set: bool,
}

/// Named values, sorted by key (deterministic, binary-searched).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateMap {
    entries: Vec<Entry>,
}

impl StateMap {
    pub fn new() -> StateMap {
        StateMap::default()
    }

    fn slot(&mut self, key: &str) -> &mut Entry {
        let i = match self.entries.binary_search_by(|e| e.key.as_str().cmp(key)) {
            Ok(i) => i,
            Err(i) => {
                self.entries.insert(i, Entry { key: key.to_owned(), value: Value::Bool(false), set: false });
                i
            }
        };
        let e = &mut self.entries[i];
        e.set = true;
        e
    }

    pub fn set_bool(&mut self, key: &str, v: bool) -> &mut StateMap {
        self.slot(key).value = Value::Bool(v);
        self
    }

    pub fn set_int(&mut self, key: &str, v: i64) -> &mut StateMap {
        self.slot(key).value = Value::Int(v);
        self
    }

    pub fn set_num(&mut self, key: &str, v: f64) -> &mut StateMap {
        self.slot(key).value = Value::Num(v);
        self
    }

    /// Sets a string, reusing the previous string's storage.
    pub fn set_str(&mut self, key: &str, v: &str) -> &mut StateMap {
        let e = self.slot(key);
        match &mut e.value {
            Value::Str(s) => {
                s.clear();
                s.push_str(v);
            }
            other => *other = Value::Str(v.to_owned()),
        }
        self
    }

    /// Sets a string-table entry (`msg_key`, e.g. `stone.warding.name`) and returns its
    /// arguments, emptied, to fill (`set_msg("hint", "hint.cost").set_int("cost", 60)`). The
    /// previous message's storage is reused.
    pub fn set_msg(&mut self, key: &str, msg_key: &str) -> &mut StateMap {
        let e = self.slot(key);
        if !matches!(e.value, Value::Msg(_)) {
            e.value = Value::Msg(Box::default());
        }
        match &mut e.value {
            Value::Msg(m) => {
                m.key.clear();
                m.key.push_str(msg_key);
                m.args.reset();
                &mut m.args
            }
            _ => unreachable!("just made a message"),
        }
    }

    /// A list (created empty if missing or of another type) to refill: `clear()` then `push()`.
    pub fn list_mut(&mut self, key: &str) -> &mut StateList {
        let e = self.slot(key);
        if !matches!(e.value, Value::List(_)) {
            e.value = Value::List(StateList::default());
        }
        match &mut e.value {
            Value::List(l) => l,
            _ => unreachable!("just made a list"),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        let i = self.entries.binary_search_by(|e| e.key.as_str().cmp(key)).ok()?;
        let e = &self.entries[i];
        e.set.then_some(&e.value)
    }

    pub fn list(&self, key: &str) -> Option<&StateList> {
        match self.get(key)? {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn remove(&mut self, key: &str) {
        if let Ok(i) = self.entries.binary_search_by(|e| e.key.as_str().cmp(key)) {
            self.entries[i].set = false;
        }
    }

    /// Unset every key, keeping storage for reuse.
    pub fn reset(&mut self) {
        for e in &mut self.entries {
            e.set = false;
        }
    }

    /// Keys that are set, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter(|e| e.set).map(|e| e.key.as_str())
    }
}

/// Rows of [`StateMap`]s (threat arrows, binding lists). Rows are kept when cleared so refilling
/// does not allocate.
#[derive(Clone, Debug, Default)]
pub struct StateList {
    rows: Vec<StateMap>,
    len: usize,
}

impl PartialEq for StateList {
    fn eq(&self, other: &StateList) -> bool {
        self.len == other.len && self.rows[..self.len] == other.rows[..other.len]
    }
}

impl StateList {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Appends a row (an empty map, reusing an old row's storage) and returns it.
    pub fn push(&mut self) -> &mut StateMap {
        if self.len == self.rows.len() {
            self.rows.push(StateMap::default());
        }
        let row = &mut self.rows[self.len];
        row.reset();
        self.len += 1;
        row
    }

    pub fn row(&self, i: usize) -> Option<&StateMap> {
        self.rows[..self.len].get(i)
    }

    pub fn rows(&self) -> &[StateMap] {
        &self.rows[..self.len]
    }
}

/// Where a bound key is looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// A top-level key of the state map.
    Global(String),
    /// `item.<field>`: a field of the current row inside a list `repeat`.
    Item(String),
    /// `item.index` / `item.number`: the row index from 0 / 1.
    Index { one_based: bool },
}

impl Key {
    pub fn parse(s: &str) -> Option<Key> {
        let valid = |n: &str| {
            !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        };
        match s.strip_prefix("item.") {
            Some("index") => Some(Key::Index { one_based: false }),
            Some("number") => Some(Key::Index { one_based: true }),
            Some(f) => valid(f).then(|| Key::Item(f.to_owned())),
            None => valid(s).then(|| Key::Global(s.to_owned())),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Key::Global(k) => k.clone(),
            Key::Item(k) => format!("item.{k}"),
            Key::Index { one_based: false } => "item.index".into(),
            Key::Index { one_based: true } => "item.number".into(),
        }
    }
}

/// The values bindings read: the game's map, inside a list `repeat` the current row, and the
/// UI's own `ui.*` keys (`ui.width`, `ui.height`, `ui.portrait`, `ui.narrow` (under 760 logical
/// px wide), `ui.short` (under 520 high), `ui.gamepad`, `ui.touch`, and `ui.open.<screen>` for
/// each loaded screen) for layouts that adapt to the screen, the device and what else shows.
/// `strings` is the UI's string table: `@key` texts and [`Value::Msg`] values read it, and
/// numbers take its locale's format (without one they print as plain ASCII, as before tables).
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub state: &'a StateMap,
    pub item: Option<(usize, &'a StateMap)>,
    pub env: Option<&'a StateMap>,
    pub strings: Option<&'a Strings>,
}

/// A looked-up value, borrowed or synthesised (indices).
pub enum Lookup<'a> {
    Value(&'a Value),
    Index(i64),
}

impl<'a> Scope<'a> {
    pub fn new(state: &'a StateMap) -> Scope<'a> {
        Scope { state, item: None, env: None, strings: None }
    }

    pub fn with_env(mut self, env: &'a StateMap) -> Scope<'a> {
        self.env = Some(env);
        self
    }

    pub fn with_strings(mut self, strings: Option<&'a Strings>) -> Scope<'a> {
        self.strings = strings;
        self
    }

    pub fn get(&self, key: &Key) -> Option<Lookup<'a>> {
        match key {
            Key::Global(k) if k.starts_with("ui.") => self.env.and_then(|e| e.get(k)).map(Lookup::Value),
            Key::Global(k) => self.state.get(k).map(Lookup::Value),
            Key::Item(k) => self.item.and_then(|(_, row)| row.get(k)).map(Lookup::Value),
            Key::Index { one_based } => self.item.map(|(i, _)| Lookup::Index(i as i64 + i64::from(*one_based))),
        }
    }

    pub fn truthy(&self, key: &Key) -> bool {
        match self.get(key) {
            Some(Lookup::Value(v)) => v.truthy(),
            Some(Lookup::Index(i)) => i != 0,
            None => false,
        }
    }

    pub fn num(&self, key: &Key) -> Option<f64> {
        match self.get(key)? {
            Lookup::Value(v) => v.as_f64(),
            Lookup::Index(i) => Some(i as f64),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Seg {
    Lit(String),
    Bound { key: Key, fmt: Fmt },
}

/// Longest rendered template text, in bytes. What it bounds is a label's cost each frame: the
/// text is rendered again, compared with the one last laid out, and drawn as one glyph quad a
/// character (twice with a shadow), so a screen of labels stays far under the frame's glyph
/// limit. It is a screenful, not a line: a game's end-of-day summary of sixteen lines joined by
/// newlines is 300 to 600 bytes, which 512 would cut (2026-11-23: five of ten builders' games
/// put such a summary into one kept text). 4 KiB is about fifty lines of eighty characters, more
/// than a 1080p screen holds at a body size.
pub const MAX_TEXT: usize = 4096;

/// How deep string-table entries may nest through [`Value::Msg`] arguments: a message, a
/// message in its arguments, and so on, [`MAX_DEPTH`] messages in all (a screen's own `@key`
/// counts as the first). Past it a message shows as `@key`.
pub const MAX_DEPTH: u8 = 4;

/// Text with `{key}` bindings (`{{` and `}}` are literal braces), or a string-table reference:
/// `"@hud.wave"` shows that entry of the UI's string table in its language (its placeholders
/// read the same state); `"@@..."` is literal text starting with `@`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Template {
    segs: Vec<Seg>,
    /// `@key`: a string-table entry.
    reference: Option<String>,
}

impl Template {
    /// A screen's text: a `@key` reference, or text with bindings ([`Template::parse_text`]).
    pub fn parse(text: &str, path: &str) -> Result<Template, UiError> {
        if let Some(rest) = text.strip_prefix('@') {
            if let Some(lit) = rest.strip_prefix('@') {
                let mut t = Template::parse_text(lit, path)?;
                match t.segs.first_mut() {
                    Some(Seg::Lit(l)) => l.insert(0, '@'),
                    _ => t.segs.insert(0, Seg::Lit("@".into())),
                }
                return Ok(t);
            }
            let key = rest.trim();
            if !specula_lingua::table::valid_key(key) {
                return Err(UiError::new(
                    ErrorCode::BadTemplate,
                    path,
                    format!("\"@{}\" is not a string-table key (1..96 of letters, digits, _ - . starting with a letter; @@ for a literal @)", specula_lingua::error::echo(key, 40)),
                ));
            }
            return Ok(Template::reference(key));
        }
        Template::parse_text(text, path)
    }

    /// A reference to string-table entry `key` (not checked).
    pub fn reference(key: &str) -> Template {
        Template { segs: Vec::new(), reference: Some(key.to_owned()) }
    }

    /// Text with `{key}` bindings only (a string table's own texts: a leading `@` is literal).
    pub fn parse_text(text: &str, path: &str) -> Result<Template, UiError> {
        let err = |m: String| UiError::new(ErrorCode::BadTemplate, path, m);
        let mut segs = Vec::new();
        let mut lit = String::new();
        let mut chars = text.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '{' if chars.peek().map(|p| p.1) == Some('{') => {
                    chars.next();
                    lit.push('{');
                }
                '}' if chars.peek().map(|p| p.1) == Some('}') => {
                    chars.next();
                    lit.push('}');
                }
                '{' => {
                    let rest = &text[i + 1..];
                    let end = rest.find('}').ok_or_else(|| err(format!("unclosed '{{' at character {i}")))?;
                    let inner = rest[..end].trim();
                    let (name, fmt) = match inner.split_once(':') {
                        Some((n, f)) => (n.trim(), Fmt::parse(f.trim()).ok_or_else(|| {
                            err(format!("unknown format \"{f}\" in {{{inner}}}; use .N, time, pct or plain"))
                        })?),
                        None => (inner, Fmt::Auto),
                    };
                    let key = Key::parse(name).ok_or_else(|| {
                        err(format!("bad key \"{name}\" in {{{inner}}}; keys are letters, digits, _ - ."))
                    })?;
                    if !lit.is_empty() {
                        segs.push(Seg::Lit(std::mem::take(&mut lit)));
                    }
                    segs.push(Seg::Bound { key, fmt });
                    // Skip the characters of the binding.
                    let stop = i + 1 + end;
                    while chars.peek().is_some_and(|p| p.0 <= stop) {
                        chars.next();
                    }
                }
                '}' => return Err(err(format!("unmatched '}}' at character {i} (write }}}} for a brace)"))),
                c => lit.push(c),
            }
        }
        if !lit.is_empty() {
            segs.push(Seg::Lit(lit));
        }
        Ok(Template { segs, reference: None })
    }

    pub fn literal(text: &str) -> Template {
        Template { segs: if text.is_empty() { Vec::new() } else { vec![Seg::Lit(text.to_owned())] }, reference: None }
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty() && self.reference.is_none()
    }

    /// True if the template has no bindings and is no string-table reference (its text never
    /// changes).
    pub fn is_static(&self) -> bool {
        self.reference.is_none() && self.segs.iter().all(|s| matches!(s, Seg::Lit(_)))
    }

    /// The string-table key this template shows (`@key`).
    pub fn key(&self) -> Option<&str> {
        self.reference.as_deref()
    }

    /// The words the screen writes itself (its literal segments joined; empty for `@key`):
    /// what `ui_check` calls hard-coded when the UI has a string table.
    pub fn literal_text(&self) -> String {
        self.segs
            .iter()
            .filter_map(|s| match s {
                Seg::Lit(l) => Some(l.as_str()),
                Seg::Bound { .. } => None,
            })
            .collect()
    }

    /// The state keys it reads (its own bindings; for `@key` see [`Template::keys_with`]).
    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.segs.iter().filter_map(|s| match s {
            Seg::Bound { key, .. } => Some(key),
            Seg::Lit(_) => None,
        })
    }

    /// The state keys it reads, through `strings` for a `@key` (every form's bindings and a
    /// plural's count).
    pub fn keys_with(&self, strings: Option<&Strings>) -> Vec<Key> {
        let mut out: Vec<Key> = self.keys().cloned().collect();
        if let (Some(k), Some(s)) = (&self.reference, strings) {
            if let Some(e) = s.entry(k) {
                if let UiEntry::Plural { count, .. } = e {
                    out.push(count.clone());
                }
                for t in e.templates() {
                    out.extend(t.keys().cloned());
                }
            }
        }
        out.dedup();
        out
    }

    /// Render into `out` (cleared first). Missing keys print as `{key}`, and a `@key` the string
    /// table lacks (or with no table) as `@key`, so they show in previews. Output is capped at
    /// [`MAX_TEXT`] bytes (ending in `…`), so hostile state can't make a label arbitrarily
    /// expensive.
    pub fn render(&self, scope: &Scope<'_>, out: &mut String) {
        out.clear();
        self.write_into(scope, out, 0, false, false);
        cap(out);
    }

    /// [`Template::render`] in a pseudo-locale (`ui_check {pseudo: true}`): the screen's own
    /// text (literal segments) comes out accented and about 35% longer, in brackets, as a
    /// translation might, so overflow shows before anyone translates (`@key` texts come from the
    /// scope's table, the pseudo-locale's while checking). Returns the first bound key whose
    /// value put words on screen as they are (a [`Value::Str`] with letters): text the game
    /// supplies, which no string table translates.
    pub fn render_pseudo(&self, scope: &Scope<'_>, out: &mut String) -> Option<String> {
        out.clear();
        let from_state = self.write_into(scope, out, 0, true, true);
        cap(out);
        from_state
    }

    /// Writes the text after what `out` holds; returns the first state key that supplied words
    /// as they are. Stops adding once `out` is past [`MAX_TEXT`] (nested messages included), so
    /// the work is bounded by the cap.
    fn write_into(&self, scope: &Scope<'_>, out: &mut String, depth: u8, pseudo: bool, flag: bool) -> Option<String> {
        if let Some(k) = &self.reference {
            return write_entry(k, scope, out, depth, flag);
        }
        let mut from_state = None;
        for s in &self.segs {
            if out.len() > MAX_TEXT {
                break;
            }
            match s {
                Seg::Lit(l) if pseudo => pseudo_text(l, out),
                Seg::Lit(l) => out.push_str(l),
                Seg::Bound { key, fmt } => match scope.get(key) {
                    Some(Lookup::Value(v)) => {
                        // Only the pseudo-locale asks (Key::name allocates; a HUD frame may not).
                        if flag && from_state.is_none() && matches!(v, Value::Str(s) if s.chars().filter(|c| c.is_alphabetic()).nth(1).is_some()) {
                            from_state = Some(key.name());
                        }
                        let inner = write_bound(v, *fmt, scope, out, depth, flag);
                        from_state = from_state.or(inner);
                    }
                    Some(Lookup::Index(i)) => write_value(&Value::Int(i), *fmt, scope.strings.map(Strings::number), out),
                    None if pseudo => out.push_str("{?}"),
                    None => {
                        // Written in place (Key::name allocates; a HUD frame may not).
                        out.push('{');
                        match key {
                            Key::Global(k) => out.push_str(k),
                            Key::Item(k) => {
                                out.push_str("item.");
                                out.push_str(k);
                            }
                            Key::Index { .. } => out.push_str("item.index"),
                        }
                        out.push('}');
                    }
                },
            }
        }
        from_state
    }

    /// Keys this template reads that `scope` doesn't have.
    pub fn missing<'s>(&'s self, scope: &'s Scope<'_>) -> impl Iterator<Item = &'s Key> + 's {
        self.keys().filter(move |k| scope.get(k).is_none())
    }
}

fn cap(out: &mut String) {
    if out.len() > MAX_TEXT {
        let mut cut = MAX_TEXT;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push('…');
    }
}

/// String-table entry `key` in `scope`'s table, its form picked by its count's value.
fn write_entry(key: &str, scope: &Scope<'_>, out: &mut String, depth: u8, flag: bool) -> Option<String> {
    let entry = scope.strings.filter(|_| depth < MAX_DEPTH).and_then(|s| s.entry(key).map(|e| (s, e)));
    let Some((strings, e)) = entry else {
        out.push('@');
        out.push_str(key);
        return None;
    };
    let t = match e {
        UiEntry::One(t) => Some(t),
        UiEntry::Plural { count, fmt, .. } => {
            let arg = match scope.get(count) {
                Some(Lookup::Value(Value::Int(n))) => Some(Arg::Int(*n)),
                Some(Lookup::Value(v)) => v.as_f64().map(Arg::Num),
                Some(Lookup::Index(i)) => Some(Arg::Int(i)),
                None => None,
            };
            let cat = arg.and_then(|a| specula_lingua::catalog::operands(a, *fmt)).map_or(Category::Other, |o| strings.rules().select(&o));
            e.form(cat)
        }
    };
    // The table's texts are already in the scope's language (the pseudo-locale's too): only
    // the bindings below may still bring words from state.
    t.and_then(|t| t.write_into(scope, out, depth + 1, false, flag))
}

/// A bound value: a message through the string table, else [`write_value`].
fn write_bound(v: &Value, fmt: Fmt, scope: &Scope<'_>, out: &mut String, depth: u8, flag: bool) -> Option<String> {
    match v {
        Value::Msg(m) => {
            // The entry's own text counts the level (write_entry): counting it here as well made
            // each message cost two, so a message three deep showed its key (review R2 of m5-l10n).
            let inner = Scope { state: &m.args, item: None, env: scope.env, strings: scope.strings };
            write_entry(&m.key, &inner, out, depth, flag)
        }
        v => {
            write_value(v, fmt, scope.strings.map(Strings::number), out);
            None
        }
    }
}

/// A pseudo-locale version of a screen's literal text into `out`: vowels and a few consonants
/// accented (Latin-1, which every UI font has), about 35% longer (`·` padding, at least one per
/// word-bearing segment), the whole in brackets. Text without letters (separators, numbers) is
/// left as it is.
pub fn pseudo_text(text: &str, out: &mut String) {
    specula_lingua::message::pseudo_plain(text, out);
}

/// A value as `fmt` says. With a locale's number format (`nf`: the UI has a string table),
/// numbers are grouped and use its decimal separator and percent pattern; without one they
/// print as plain ASCII.
fn write_value(v: &Value, fmt: Fmt, nf: Option<&NumberFormat>, out: &mut String) {
    // Writing to a String cannot fail.
    match (v, fmt) {
        (Value::Str(s), _) => out.push_str(s),
        (Value::Msg(m), _) => {
            // Only reached without a scope (write_bound handles messages): show the key.
            out.push('@');
            out.push_str(&m.key);
        }
        (Value::Bool(b), Fmt::Auto | Fmt::Plain) => out.push_str(if *b { "true" } else { "false" }),
        (Value::List(l), _) => {
            let _ = write!(out, "{}", l.len());
        }
        (Value::Int(i), Fmt::Plain) => {
            let _ = write!(out, "{i}");
        }
        (v, Fmt::Plain) => specula_lingua::number::write_auto_ascii(v.as_f64().unwrap_or(0.0), out),
        (v, Fmt::Time) => specula_lingua::message::write_time(v.as_f64().unwrap_or(0.0), out),
        (v, fmt) => {
            let arg = match v {
                Value::Int(i) => Arg::Int(*i),
                v => Arg::Num(v.as_f64().unwrap_or(0.0)),
            };
            // No table: ASCII as before tables (no grouping); made once (no allocation per value).
            static PLAIN: std::sync::OnceLock<NumberFormat> = std::sync::OnceLock::new();
            let nf = nf.unwrap_or_else(|| PLAIN.get_or_init(|| NumberFormat { group: None, ..NumberFormat::english() }));
            specula_lingua::message::write_arg(arg, fmt, nf, out);
        }
    }
}

/// Comparison operators in conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Num(f64),
    Str(String),
}

/// A visibility / enabled condition.
///
/// Grammar: `key`, `!key`, `key OP literal` (OP is `== != < <= > >=`; the literal is a number,
/// `true`/`false`, `'quoted'` or a bare word), joined with `&&` and `||` (`&&` binds tighter).
/// A JSON array of conditions means all of them. Missing keys are false.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Always,
    Truthy { key: Key, negate: bool },
    Cmp { key: Key, op: Op, lit: Lit },
    All(Vec<Cond>),
    Any(Vec<Cond>),
}

impl Cond {
    pub fn parse(text: &str, path: &str) -> Result<Cond, UiError> {
        let err = |m: String| UiError::new(ErrorCode::BadCondition, path, m);
        let toks = tokenize(text).map_err(err)?;
        if toks.is_empty() {
            return Err(err("empty condition".into()));
        }
        let mut p = CondParser { toks: &toks, i: 0 };
        let c = p.or().map_err(err)?;
        if p.i != toks.len() {
            return Err(err(format!("unexpected {} in \"{}\"", toks[p.i].describe(), text.trim())));
        }
        Ok(c)
    }

    pub fn eval(&self, scope: &Scope<'_>) -> bool {
        match self {
            Cond::Always => true,
            Cond::Truthy { key, negate } => scope.truthy(key) != *negate,
            Cond::Cmp { key, op, lit } => match lit {
                Lit::Num(n) => match scope.num(key) {
                    Some(v) => match op {
                        Op::Eq => v == *n,
                        Op::Ne => v != *n,
                        Op::Lt => v < *n,
                        Op::Le => v <= *n,
                        Op::Gt => v > *n,
                        Op::Ge => v >= *n,
                    },
                    None => false,
                },
                Lit::Str(s) => match scope.get(key) {
                    Some(Lookup::Value(Value::Str(v))) => (v == s) == (*op == Op::Eq),
                    _ => false,
                },
            },
            Cond::All(c) => c.iter().all(|c| c.eval(scope)),
            Cond::Any(c) => c.iter().any(|c| c.eval(scope)),
        }
    }

    pub fn keys(&self, out: &mut Vec<Key>) {
        match self {
            Cond::Always => {}
            Cond::Truthy { key, .. } | Cond::Cmp { key, .. } => out.push(key.clone()),
            Cond::All(c) | Cond::Any(c) => c.iter().for_each(|c| c.keys(out)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A key or a bare word (`dust`, `item.name`, `won`).
    Word(String),
    Num(f64),
    Str(String),
    Op(Op),
    Not,
    And,
    Or,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Word(w) => format!("\"{w}\""),
            Tok::Num(n) => format!("{n}"),
            Tok::Str(s) => format!("'{s}'"),
            Tok::Op(_) => "an operator".into(),
            Tok::Not => "\"!\"".into(),
            Tok::And => "\"&&\"".into(),
            Tok::Or => "\"||\"".into(),
        }
    }
}

/// Split a condition into tokens; quoted text may contain anything except `'`.
fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let two = |x: u8| b.get(i + 1) == Some(&x);
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'\'' => {
                let end = text[i + 1..].find('\'').ok_or_else(|| format!("unclosed quote at character {i}"))?;
                out.push(Tok::Str(text[i + 1..i + 1 + end].to_owned()));
                i += end + 2;
            }
            b'&' if two(b'&') => {
                out.push(Tok::And);
                i += 2;
            }
            b'|' if two(b'|') => {
                out.push(Tok::Or);
                i += 2;
            }
            b'=' if two(b'=') => {
                out.push(Tok::Op(Op::Eq));
                i += 2;
            }
            b'!' if two(b'=') => {
                out.push(Tok::Op(Op::Ne));
                i += 2;
            }
            b'<' | b'>' => {
                let eq = two(b'=');
                out.push(Tok::Op(match (c, eq) {
                    (b'<', true) => Op::Le,
                    (b'<', false) => Op::Lt,
                    (_, true) => Op::Ge,
                    _ => Op::Gt,
                }));
                i += 1 + usize::from(eq);
            }
            b'!' => {
                out.push(Tok::Not);
                i += 1;
            }
            c if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.' || c == b'+' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'-' | b'.' | b'+')) {
                    i += 1;
                }
                let w = &text[start..i];
                let numeric = w.as_bytes()[0].is_ascii_digit() || (w.len() > 1 && matches!(w.as_bytes()[0], b'-' | b'+' | b'.'));
                match w.parse::<f64>() {
                    Ok(n) if n.is_finite() && numeric => out.push(Tok::Num(n)),
                    Ok(_) if numeric || matches!(w.to_ascii_lowercase().as_str(), "nan" | "inf" | "infinity" | "-inf" | "+inf") => {
                        return Err(format!("\"{w}\" is not a finite number"))
                    }
                    _ if numeric => return Err(format!("bad number \"{w}\"")),
                    _ => out.push(Tok::Word(w.to_owned())),
                }
            }
            _ => {
                let ch = text[i..].chars().next().unwrap_or('?');
                return Err(format!("unexpected '{ch}' at character {i} (operators: == != < <= > >= ! && ||)"));
            }
        }
    }
    Ok(out)
}

struct CondParser<'t> {
    toks: &'t [Tok],
    i: usize,
}

impl CondParser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn or(&mut self) -> Result<Cond, String> {
        let mut parts = vec![self.and()?];
        while self.peek() == Some(&Tok::Or) {
            self.i += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 { parts.pop().unwrap_or(Cond::Always) } else { Cond::Any(parts) })
    }

    fn and(&mut self) -> Result<Cond, String> {
        let mut parts = vec![self.term()?];
        while self.peek() == Some(&Tok::And) {
            self.i += 1;
            parts.push(self.term()?);
        }
        Ok(if parts.len() == 1 { parts.pop().unwrap_or(Cond::Always) } else { Cond::All(parts) })
    }

    fn key(&mut self) -> Result<Key, String> {
        match self.toks.get(self.i) {
            Some(Tok::Word(w)) => {
                self.i += 1;
                Key::parse(w).ok_or_else(|| format!("bad key \"{w}\""))
            }
            Some(t) => Err(format!("expected a key, found {}", t.describe())),
            None => Err("expected a key at the end".into()),
        }
    }

    fn term(&mut self) -> Result<Cond, String> {
        if self.peek() == Some(&Tok::Not) {
            self.i += 1;
            let key = self.key()?;
            return Ok(Cond::Truthy { key, negate: true });
        }
        let key = self.key()?;
        let Some(Tok::Op(op)) = self.peek().cloned() else { return Ok(Cond::Truthy { key, negate: false }) };
        self.i += 1;
        let lit = match self.toks.get(self.i) {
            Some(Tok::Num(n)) => Lit::Num(*n),
            Some(Tok::Str(s)) => Lit::Str(s.clone()),
            Some(Tok::Word(w)) if w == "true" => Lit::Num(1.0),
            Some(Tok::Word(w)) if w == "false" => Lit::Num(0.0),
            Some(Tok::Word(w)) if w.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') => Lit::Str(w.clone()),
            Some(t) => return Err(format!("bad literal {} (number, true/false, 'text' or a word)", t.describe())),
            None => return Err("missing a value after the comparison".into()),
        };
        self.i += 1;
        if matches!(lit, Lit::Str(_)) && !matches!(op, Op::Eq | Op::Ne) {
            return Err("text can only be compared with == or !=".into());
        }
        Ok(Cond::Cmp { key, op, lit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(t: &str, s: &StateMap) -> String {
        let mut out = String::new();
        Template::parse(t, "t").unwrap().render(&Scope::new(s), &mut out);
        out
    }

    #[test]
    fn templates_interpolate_and_format() {
        let mut s = StateMap::new();
        s.set_int("dust", 150).set_num("next", 23.2).set_num("ratio", 0.456).set_str("name", "Warding");
        assert_eq!(render("Dust {dust}", &s), "Dust 150");
        assert_eq!(render("{name} ({dust})", &s), "Warding (150)");
        assert_eq!(render("next {next:time}", &s), "next 0:24");
        assert_eq!(render("{ratio:pct}", &s), "46%");
        assert_eq!(render("{ratio:.1}", &s), "0.5");
        assert_eq!(render("{ratio}", &s), "0.46");
        assert_eq!(render("{{literal}}", &s), "{literal}");
        assert_eq!(render("{missing} x", &s), "{missing} x");
        assert_eq!(render("{ dust }", &s), "150");
    }

    #[test]
    fn template_errors_are_structured() {
        for bad in ["{dust", "dust}", "{du st}", "{dust:hex}", "{}"] {
            let e = Template::parse(bad, "root.text").unwrap_err();
            assert_eq!(e.code, ErrorCode::BadTemplate, "{bad}");
            assert_eq!(e.path, "root.text");
        }
    }

    #[test]
    fn item_keys_read_the_current_row() {
        let mut s = StateMap::new();
        let list = s.list_mut("rows");
        list.push().set_str("label", "first");
        list.push().set_str("label", "second");
        let rows = s.list("rows").unwrap();
        let t = Template::parse("{item.number}. {item.label}", "t").unwrap();
        let mut out = String::new();
        t.render(&Scope { state: &s, item: Some((1, rows.row(1).unwrap())), env: None, strings: None }, &mut out);
        assert_eq!(out, "2. second");
    }

    #[test]
    fn conditions_evaluate() {
        let mut s = StateMap::new();
        s.set_bool("paused", true).set_int("dust", 40).set_str("outcome", "won");
        let ev = |c: &str| Cond::parse(c, "c").unwrap().eval(&Scope::new(&s));
        assert!(ev("paused"));
        assert!(!ev("!paused"));
        assert!(ev("dust < 50"));
        assert!(!ev("dust >= 50"));
        assert!(ev("outcome == won"));
        assert!(ev("outcome == 'won'"));
        assert!(ev("outcome != lost"));
        assert!(!ev("missing"));
        assert!(ev("!missing"));
        assert!(!ev("missing > 0"));
        assert!(ev("paused && dust == 40"));
        assert!(ev("missing || paused"));
        assert!(!ev("paused && missing"));
        assert!(ev("paused == true"));
    }

    #[test]
    fn quoted_text_may_contain_operators() {
        let mut s = StateMap::new();
        s.set_str("who", "a || b");
        let c = Cond::parse("who == 'a || b'", "c").unwrap();
        assert!(c.eval(&Scope::new(&s)));
        assert!(!Cond::parse("who == 'a' || who == 'b'", "c").unwrap().eval(&Scope::new(&s)));
        assert_eq!(Cond::parse("x >= -2.5", "c").unwrap(), Cond::Cmp { key: Key::Global("x".into()), op: Op::Ge, lit: Lit::Num(-2.5) });
    }

    #[test]
    fn huge_values_are_capped() {
        let mut s = StateMap::new();
        s.set_num("w", 1e300).set_str("long", &"x".repeat(MAX_TEXT + 1000));
        assert_eq!(render("{w:.6}", &s), "1.000e300");
        assert_eq!(render("{w}", &s), "1.000e300");
        let long = render("{long}", &s);
        assert!(long.len() <= MAX_TEXT + 3 && long.ends_with('…'));
    }

    #[test]
    fn condition_errors() {
        for bad in ["", "dust >= ", "a b c", "name < word", "!", "x == NaN", "x > inf", "a && && b", "a ||", "x = 5", "x == 'open", "(a)"] {
            let e = Cond::parse(bad, "root.visible").unwrap_err();
            assert_eq!(e.code, ErrorCode::BadCondition, "{bad:?}");
        }
    }

    #[test]
    fn refilling_lists_and_strings_reuses_storage() {
        let mut s = StateMap::new();
        s.set_str("msg", "a fairly long message to reserve space");
        for _ in 0..3 {
            let l = s.list_mut("threats");
            l.clear();
            l.push().set_num("x", 1.0);
            l.push().set_num("x", 2.0);
        }
        assert_eq!(s.list("threats").unwrap().len(), 2);
        let cap = match s.get("msg") {
            Some(Value::Str(m)) => m.capacity(),
            _ => 0,
        };
        s.set_str("msg", "short");
        match s.get("msg") {
            Some(Value::Str(m)) => assert_eq!(m.capacity(), cap),
            _ => panic!(),
        }
        s.remove("msg");
        assert!(s.get("msg").is_none());
    }
}

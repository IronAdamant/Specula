//! Game state the UI binds to: a [`StateMap`] of named values the game fills each frame,
//! [`Template`]s (`"Dust {dust}"`) and [`Cond`]itions (`"paused"`, `"dust >= 50"`).
//!
//! Everything reuses its storage: setting a key that already exists, refilling a list, or
//! rendering a template into a `String` with capacity does not allocate (the HUD's per-frame
//! budget, F3).

use crate::error::{ErrorCode, UiError};
use std::fmt::Write as _;

/// One bound value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    List(StateList),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => l.len() != 0,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n),
            Value::Str(_) | Value::List(_) => None,
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

/// The values bindings read: the game's map plus, inside a list `repeat`, the current row.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub state: &'a StateMap,
    pub item: Option<(usize, &'a StateMap)>,
}

/// A looked-up value, borrowed or synthesised (indices).
pub enum Lookup<'a> {
    Value(&'a Value),
    Index(i64),
}

impl<'a> Scope<'a> {
    pub fn new(state: &'a StateMap) -> Scope<'a> {
        Scope { state, item: None }
    }

    pub fn get(&self, key: &Key) -> Option<Lookup<'a>> {
        match key {
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

/// How a bound value is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    /// Integers as integers, other numbers with up to 2 decimals (trailing zeros trimmed).
    Auto,
    /// `{key:.N}`: exactly N decimals (N ≤ 6).
    Fixed(u8),
    /// `{key:time}`: seconds as `m:ss`.
    Time,
    /// `{key:pct}`: a 0..1 ratio as a whole percentage.
    Percent,
}

#[derive(Clone, Debug, PartialEq)]
enum Seg {
    Lit(String),
    Bound { key: Key, fmt: Fmt },
}

/// Text with `{key}` bindings (`{{` and `}}` are literal braces).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Template {
    segs: Vec<Seg>,
}

impl Template {
    pub fn parse(text: &str, path: &str) -> Result<Template, UiError> {
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
                        Some((n, f)) => (n.trim(), parse_fmt(f.trim()).ok_or_else(|| {
                            err(format!("unknown format \"{f}\" in {{{inner}}}; use .N, time or pct"))
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
        Ok(Template { segs })
    }

    pub fn literal(text: &str) -> Template {
        Template { segs: if text.is_empty() { Vec::new() } else { vec![Seg::Lit(text.to_owned())] } }
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// True if the template has no bindings.
    pub fn is_static(&self) -> bool {
        self.segs.iter().all(|s| matches!(s, Seg::Lit(_)))
    }

    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.segs.iter().filter_map(|s| match s {
            Seg::Bound { key, .. } => Some(key),
            Seg::Lit(_) => None,
        })
    }

    /// Render into `out` (cleared first). Missing keys print as `{key}` so they show in previews.
    pub fn render(&self, scope: &Scope<'_>, out: &mut String) {
        out.clear();
        for s in &self.segs {
            match s {
                Seg::Lit(l) => out.push_str(l),
                Seg::Bound { key, fmt } => match scope.get(key) {
                    Some(Lookup::Value(v)) => write_value(v, *fmt, out),
                    Some(Lookup::Index(i)) => write_value(&Value::Int(i), *fmt, out),
                    None => {
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
    }

    /// Keys this template reads that `scope` doesn't have.
    pub fn missing<'s>(&'s self, scope: &'s Scope<'_>) -> impl Iterator<Item = &'s Key> + 's {
        self.keys().filter(move |k| scope.get(k).is_none())
    }
}

fn parse_fmt(f: &str) -> Option<Fmt> {
    match f {
        "time" => Some(Fmt::Time),
        "pct" => Some(Fmt::Percent),
        _ => {
            let n: u8 = f.strip_prefix('.')?.parse().ok()?;
            (n <= 6).then_some(Fmt::Fixed(n))
        }
    }
}

fn write_value(v: &Value, fmt: Fmt, out: &mut String) {
    // Writing to a String cannot fail.
    match (v, fmt) {
        (Value::Str(s), _) => out.push_str(s),
        (Value::Bool(b), Fmt::Auto) => out.push_str(if *b { "true" } else { "false" }),
        (Value::List(l), _) => {
            let _ = write!(out, "{}", l.len());
        }
        (v, fmt) => {
            let n = v.as_f64().unwrap_or(0.0);
            match fmt {
                Fmt::Auto => match v {
                    Value::Int(i) => {
                        let _ = write!(out, "{i}");
                    }
                    _ if n.fract() == 0.0 && n.abs() < 1e15 => {
                        let _ = write!(out, "{}", n as i64);
                    }
                    _ => {
                        let start = out.len();
                        let _ = write!(out, "{n:.2}");
                        while out.len() > start && out.ends_with('0') {
                            out.pop();
                        }
                        if out.ends_with('.') {
                            out.pop();
                        }
                    }
                },
                Fmt::Fixed(d) => {
                    let _ = write!(out, "{n:.*}", usize::from(d));
                }
                Fmt::Time => {
                    let s = n.max(0.0).ceil() as i64;
                    let _ = write!(out, "{}:{:02}", s / 60, s % 60);
                }
                Fmt::Percent => {
                    let _ = write!(out, "{}%", (n * 100.0).round() as i64);
                }
            }
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
        let text = text.trim();
        if text.is_empty() {
            return Err(err("empty condition".into()));
        }
        if text.contains("||") {
            let parts = text.split("||").map(|p| Cond::parse(p, path)).collect::<Result<Vec<_>, _>>()?;
            return Ok(Cond::Any(parts));
        }
        if text.contains("&&") {
            let parts = text.split("&&").map(|p| Cond::parse(p, path)).collect::<Result<Vec<_>, _>>()?;
            return Ok(Cond::All(parts));
        }
        for (sym, op) in [("==", Op::Eq), ("!=", Op::Ne), ("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)] {
            if let Some((l, r)) = text.split_once(sym) {
                let key = Key::parse(l.trim()).ok_or_else(|| err(format!("bad key \"{}\"", l.trim())))?;
                let r = r.trim();
                let lit = if let Some(q) = r.strip_prefix('\'').and_then(|q| q.strip_suffix('\'')) {
                    Lit::Str(q.to_owned())
                } else if r == "true" {
                    Lit::Num(1.0)
                } else if r == "false" {
                    Lit::Num(0.0)
                } else if let Ok(n) = r.parse::<f64>() {
                    Lit::Num(n)
                } else if !r.is_empty() && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                    Lit::Str(r.to_owned())
                } else {
                    return Err(err(format!("bad literal \"{r}\" (number, true/false, 'text' or a word)")));
                };
                if matches!(lit, Lit::Str(_)) && !matches!(op, Op::Eq | Op::Ne) {
                    return Err(err(format!("text can only be compared with == or != (\"{text}\")")));
                }
                return Ok(Cond::Cmp { key, op, lit });
            }
        }
        let (negate, name) = match text.strip_prefix('!') {
            Some(n) => (true, n.trim()),
            None => (false, text),
        };
        let key = Key::parse(name).ok_or_else(|| err(format!("bad key \"{name}\"")))?;
        Ok(Cond::Truthy { key, negate })
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
        t.render(&Scope { state: &s, item: Some((1, rows.row(1).unwrap())) }, &mut out);
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
    fn condition_errors() {
        for bad in ["", "dust >= ", "a b c", "name < word", "!"] {
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

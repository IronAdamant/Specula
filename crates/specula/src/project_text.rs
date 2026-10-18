//! Text edits and diffs for project files (pure functions; see `project_tools`).
//!
//! - **Edits keep a file's formatting.** A JSON path edit (`parts[1].shape.size`) finds the byte
//!   span of that value in the text and replaces only those bytes; a new member is inserted after
//!   the object's last one, indented like it. `find`/`replace` and line ranges work on any text.
//! - **Diffs are bounded.** Line diffs trim the common head and tail, then run an LCS over what is
//!   left when that is small enough (else the rest is one hunk); JSON diffs compare values by
//!   path. Both stop after a fixed number of items.

use specula_core::Json;

/// One step of a JSON path: an object key or an array index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// Splits `a.b[2].c` into steps (`""` is the whole document).
pub fn parse_path(path: &str) -> Result<Vec<Step>, String> {
    let mut out = Vec::new();
    if path.is_empty() {
        return Ok(out);
    }
    for seg in path.split('.') {
        let (name, rest) = match seg.find('[') {
            Some(i) => (&seg[..i], &seg[i..]),
            None => (seg, ""),
        };
        if !name.is_empty() {
            out.push(Step::Key(name.to_owned()));
        } else if rest.is_empty() {
            return Err(format!("empty segment in JSON path {path:?} (two dots?)"));
        }
        let mut r = rest;
        while let Some(s) = r.strip_prefix('[') {
            let end = s.find(']').ok_or_else(|| format!("unclosed '[' in JSON path {path:?}"))?;
            let n: usize = s[..end].parse().map_err(|_| format!("'{}' is not an array index (JSON path {path:?})", &s[..end]))?;
            out.push(Step::Index(n));
            r = &s[end + 1..];
        }
        if !r.is_empty() {
            return Err(format!("unexpected '{r}' in JSON path {path:?}"));
        }
    }
    Ok(out)
}

/// `a.b[2]` from steps.
pub fn path_string(steps: &[Step]) -> String {
    let mut s = String::new();
    for st in steps {
        match st {
            Step::Key(k) => {
                if !s.is_empty() {
                    s.push('.');
                }
                s.push_str(k);
            }
            Step::Index(i) => s.push_str(&format!("[{i}]")),
        }
    }
    s
}

/// A value's place in the text, and for containers where their members are.
#[derive(Clone, Debug)]
struct Node {
    start: usize,
    end: usize,
    /// Objects: `(key, key start, value)`; arrays: `(None, item start, value)`.
    members: Vec<(Option<String>, usize, Node)>,
    object: bool,
    array: bool,
}

struct Scanner<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Scanner<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn err(&self, what: &str) -> String {
        format!("{what} at byte {}", self.i)
    }

    fn string(&mut self) -> Result<(), String> {
        // at the opening quote
        self.i += 1;
        while self.i < self.s.len() {
            match self.s[self.i] {
                b'"' => {
                    self.i += 1;
                    return Ok(());
                }
                b'\\' => self.i += 2,
                _ => self.i += 1,
            }
        }
        Err(self.err("unterminated string"))
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > 128 {
            return Err(self.err("nested too deeply"));
        }
        self.ws();
        let start = self.i;
        let mut node = Node { start, end: start, members: Vec::new(), object: false, array: false };
        match self.s.get(self.i) {
            Some(b'{') => {
                node.object = true;
                self.i += 1;
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        let ks = self.i;
                        if self.s.get(self.i) != Some(&b'"') {
                            return Err(self.err("expected a key"));
                        }
                        self.string()?;
                        let key = match Json::parse(&self.text[ks..self.i]) {
                            Ok(Json::Str(k)) => k,
                            _ => return Err(format!("bad key at byte {ks}")),
                        };
                        self.ws();
                        if self.s.get(self.i) != Some(&b':') {
                            return Err(self.err("expected ':'"));
                        }
                        self.i += 1;
                        let v = self.value(depth + 1)?;
                        node.members.push((Some(key), ks, v));
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b'}') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(self.err("expected ',' or '}'")),
                        }
                    }
                }
            }
            Some(b'[') => {
                node.array = true;
                self.i += 1;
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        let is = self.i;
                        let v = self.value(depth + 1)?;
                        node.members.push((None, is, v));
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b']') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(self.err("expected ',' or ']'")),
                        }
                    }
                }
            }
            Some(b'"') => self.string()?,
            Some(_) => {
                while self.i < self.s.len() && !matches!(self.s[self.i], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                    self.i += 1;
                }
                if self.i == start {
                    return Err(self.err("expected a value"));
                }
            }
            None => return Err(self.err("expected a value")),
        }
        node.end = self.i;
        Ok(node)
    }
}

fn scan(text: &str) -> Result<Node, String> {
    // The values' syntax is checked by the real parser; the scanner only finds spans.
    Json::parse(text).map_err(|e| format!("the file isn't valid JSON ({e}); fix it with a find/replace or lines edit first"))?;
    let mut sc = Scanner { s: text.as_bytes(), text, i: 0 };
    sc.value(0)
}

/// The leading whitespace of the line holding byte `at`.
fn indent_at(text: &str, at: usize) -> &str {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line_start..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// `v` written for a place indented `indent`: compact when short, else pretty with its lines
/// indented to match.
fn render(v: &Json, indent: &str) -> String {
    let compact = v.to_string();
    if compact.len() <= 100 || !matches!(v, Json::Obj(_) | Json::Arr(_)) {
        return compact;
    }
    v.to_pretty().replace('\n', &format!("\n{indent}"))
}

fn find<'n>(root: &'n Node, steps: &[Step]) -> Result<&'n Node, (usize, &'n Node)> {
    let mut cur = root;
    for (k, st) in steps.iter().enumerate() {
        let next = match st {
            Step::Key(key) if cur.object => cur.members.iter().find(|(n, _, _)| n.as_deref() == Some(key.as_str())).map(|m| &m.2),
            Step::Index(i) if cur.array => cur.members.get(*i).map(|m| &m.2),
            _ => None,
        };
        match next {
            Some(n) => cur = n,
            None => return Err((k, cur)),
        }
    }
    Ok(cur)
}

/// Sets the value at `path` in JSON `text`, changing only its bytes (a new object member is
/// added after the last one; an index one past the end appends; missing objects on the way are
/// created).
pub fn json_set(text: &str, path: &str, value: &Json) -> Result<String, String> {
    let steps = parse_path(path)?;
    let root = scan(text)?;
    let out = match find(&root, &steps) {
        Ok(node) => {
            let v = render(value, indent_at(text, node.start));
            format!("{}{v}{}", &text[..node.start], &text[node.end..])
        }
        Err((k, parent)) => {
            // Build what's missing below the deepest existing value.
            let mut inner = value.clone();
            for st in steps[k + 1..].iter().rev() {
                match st {
                    Step::Key(key) => inner = Json::obj([(key.clone(), inner)]),
                    Step::Index(_) => return Err(format!("{} doesn't exist, so its items can't be set (set the whole array)", path_string(&steps[..=k]))),
                }
            }
            match &steps[k] {
                Step::Key(key) if parent.object => insert(text, parent, Some(key), &inner),
                Step::Index(i) if parent.array && *i == parent.members.len() => insert(text, parent, None, &inner),
                Step::Index(i) if parent.array => return Err(format!("index {i} is past the end of {} (length {})", path_string(&steps[..k]), parent.members.len())),
                _ => return Err(format!("{} is not an {}", if k == 0 { "the document".to_owned() } else { path_string(&steps[..k]) }, if matches!(steps[k], Step::Key(_)) { "object" } else { "array" })),
            }
        }
    };
    Json::parse(&out).map_err(|e| format!("the edit would leave invalid JSON ({e})"))?;
    Ok(out)
}

/// Adds a member (`key`) or an item to container `node`, after its last one.
fn insert(text: &str, node: &Node, key: Option<&str>, value: &Json) -> String {
    let close = node.end - 1; // the closing bracket
    let entry = |indent: &str| match key {
        Some(k) => format!("{}: {}", Json::from(k), render(value, indent)),
        None => render(value, indent),
    };
    match node.members.last() {
        None => {
            let multiline = text[node.start..node.end].contains('\n');
            if multiline {
                let outer = indent_at(text, close);
                let indent = format!("{outer}  ");
                format!("{}\n{indent}{}\n{outer}{}", &text[..node.start + 1], entry(&indent), &text[close..])
            } else {
                format!("{}{}{}", &text[..node.start + 1], entry(""), &text[close..])
            }
        }
        Some((_, member_start, last)) => {
            let on_own_line = text[node.start..*member_start].contains('\n');
            if on_own_line {
                let indent = indent_at(text, *member_start).to_owned();
                format!("{},\n{indent}{}{}", &text[..last.end], entry(&indent), &text[last.end..])
            } else {
                format!("{}, {}{}", &text[..last.end], entry(""), &text[last.end..])
            }
        }
    }
}

/// Removes the member or item at `path` from JSON `text` (with its comma).
pub fn json_remove(text: &str, path: &str) -> Result<String, String> {
    let steps = parse_path(path)?;
    let Some((last, parent_steps)) = steps.split_last() else { return Err("can't remove the whole document (project_delete deletes the file)".into()) };
    let root = scan(text)?;
    let parent = find(&root, parent_steps).map_err(|(k, _)| format!("{} doesn't exist", path_string(&steps[..=k])))?;
    let idx = match last {
        Step::Key(key) if parent.object => parent.members.iter().position(|(n, _, _)| n.as_deref() == Some(key.as_str())),
        Step::Index(i) if parent.array => (*i < parent.members.len()).then_some(*i),
        _ => None,
    }
    .ok_or_else(|| format!("{path} doesn't exist"))?;
    let (_, start, node) = &parent.members[idx];
    let (cut_from, cut_to) = if idx + 1 < parent.members.len() {
        (*start, parent.members[idx + 1].1)
    } else if idx > 0 {
        (parent.members[idx - 1].2.end, node.end)
    } else {
        (*start, node.end)
    };
    let out = format!("{}{}", &text[..cut_from], &text[cut_to..]);
    Json::parse(&out).map_err(|e| format!("the edit would leave invalid JSON ({e})"))?;
    Ok(out)
}

/// Replaces the one occurrence of `find` (refused when it occurs 0 or several times).
pub fn find_replace(text: &str, find: &str, replace: &str) -> Result<String, String> {
    if find.is_empty() {
        return Err("'find' must not be empty".into());
    }
    let n = text.matches(find).count();
    match n {
        1 => Ok(text.replacen(find, replace, 1)),
        0 => Err("'find' text doesn't occur in the file (project_read shows it; whitespace counts)".into()),
        n => Err(format!("'find' text occurs {n} times; include more context so it occurs once")),
    }
}

/// Replaces lines `from..=to` (1-based; `to = from - 1` inserts before `from`) with `new`.
pub fn replace_lines(text: &str, from: usize, to: usize, new: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let n = lines.len();
    if from == 0 || from > n + 1 || to + 1 < from || to > n {
        return Err(format!("'lines' [{from}, {to}] is outside the file's {n} lines (1-based, inclusive; [k, k-1] inserts before line k)"));
    }
    let mut out = String::with_capacity(text.len() + new.len());
    for l in &lines[..from - 1] {
        out.push_str(l);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(new);
    let rest = &lines[to..];
    if !rest.is_empty() && !new.is_empty() && !new.ends_with('\n') {
        out.push('\n');
    }
    for l in rest {
        out.push_str(l);
    }
    Ok(out)
}

// ---- diffs ----

/// A run of changed lines: base lines `base[0]..base[1]` (0-based, half open) became `added`.
#[derive(Clone, Debug, PartialEq)]
pub struct Hunk {
    pub base: (usize, usize),
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// Most lines kept per side of a hunk, characters per line, hunks per diff, and the LCS table size.
const HUNK_LINES: usize = 12;
const LINE_CHARS: usize = 160;
const MAX_HUNKS: usize = 24;
const LCS_CELLS: usize = 1_000_000;

fn cut(l: &str) -> String {
    let l = l.trim_end_matches(['\n', '\r']);
    if l.chars().count() <= LINE_CHARS {
        l.to_owned()
    } else {
        format!("{}...", l.chars().take(LINE_CHARS).collect::<String>())
    }
}

/// The changes from `a` to `b` as line hunks, and whether the list was cut.
pub fn line_diff(a: &str, b: &str) -> (Vec<Hunk>, bool) {
    let x: Vec<&str> = a.split_inclusive('\n').collect();
    let y: Vec<&str> = b.split_inclusive('\n').collect();
    let head = x.iter().zip(&y).take_while(|(p, q)| p == q).count();
    let tail = x[head..].iter().rev().zip(y[head..].iter().rev()).take_while(|(p, q)| p == q).count();
    let (xm, ym) = (&x[head..x.len() - tail], &y[head..y.len() - tail]);
    if xm.is_empty() && ym.is_empty() {
        return (Vec::new(), false);
    }
    // Pairs of (x index, y index) that match, in order.
    let mut matched: Vec<(usize, usize)> = Vec::new();
    if xm.len().saturating_mul(ym.len()) <= LCS_CELLS && !xm.is_empty() && !ym.is_empty() {
        let (n, m) = (xm.len(), ym.len());
        let mut t = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                t[i * (m + 1) + j] = if xm[i] == ym[j] { t[(i + 1) * (m + 1) + j + 1] + 1 } else { t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if xm[i] == ym[j] {
                matched.push((i, j));
                i += 1;
                j += 1;
            } else if t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    matched.push((xm.len(), ym.len()));
    let mut hunks = Vec::new();
    let mut cut_off = false;
    let (mut pi, mut pj) = (0, 0);
    for (i, j) in matched {
        if i > pi || j > pj {
            if hunks.len() == MAX_HUNKS {
                cut_off = true;
                break;
            }
            let removed: Vec<String> = xm[pi..i].iter().take(HUNK_LINES).map(|l| cut(l)).collect();
            let added: Vec<String> = ym[pj..j].iter().take(HUNK_LINES).map(|l| cut(l)).collect();
            cut_off |= i - pi > HUNK_LINES || j - pj > HUNK_LINES;
            hunks.push(Hunk { base: (head + pi, head + i), removed, added });
        }
        (pi, pj) = (i + 1, j + 1);
    }
    (hunks, cut_off)
}

/// Whether two hunk lists (both from the same base) touch the same base lines (touching ends
/// count: two edits on neighbouring lines are worth a look).
pub fn hunks_overlap(a: &[Hunk], b: &[Hunk]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for h in a {
        for k in b {
            if h.base.0 <= k.base.1 && k.base.0 <= h.base.1 {
                out.push((h.base.0.min(k.base.0), h.base.1.max(k.base.1)));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// One JSON value that differs: its path, before and after (`None`: absent).
#[derive(Clone, Debug, PartialEq)]
pub struct JsonChange {
    pub path: String,
    pub before: Option<Json>,
    pub after: Option<Json>,
}

const MAX_JSON_CHANGES: usize = 48;

/// The values that differ between `a` and `b`, by path (leaves, or a whole value whose type
/// changed), and whether the list was cut.
pub fn json_diff(a: &Json, b: &Json) -> (Vec<JsonChange>, bool) {
    let mut out = Vec::new();
    let mut cut = false;
    walk(a, b, &mut Vec::new(), &mut out, &mut cut);
    (out, cut)
}

fn walk(a: &Json, b: &Json, at: &mut Vec<Step>, out: &mut Vec<JsonChange>, cut: &mut bool) {
    if a == b {
        return;
    }
    fn push(out: &mut Vec<JsonChange>, cut: &mut bool, at: &[Step], before: Option<&Json>, after: Option<&Json>) {
        if out.len() == MAX_JSON_CHANGES {
            *cut = true;
        } else {
            out.push(JsonChange { path: path_string(at), before: before.cloned(), after: after.cloned() });
        }
    }
    match (a, b) {
        (Json::Obj(x), Json::Obj(y)) => {
            for (k, v) in x {
                at.push(Step::Key(k.clone()));
                match y.iter().find(|(n, _)| n == k) {
                    Some((_, w)) => walk(v, w, at, out, cut),
                    None => push(out, cut, at, Some(v), None),
                }
                at.pop();
            }
            for (k, w) in y {
                if !x.iter().any(|(n, _)| n == k) {
                    at.push(Step::Key(k.clone()));
                    push(out, cut, at, None, Some(w));
                    at.pop();
                }
            }
        }
        (Json::Arr(x), Json::Arr(y)) => {
            for i in 0..x.len().max(y.len()) {
                at.push(Step::Index(i));
                match (x.get(i), y.get(i)) {
                    (Some(v), Some(w)) => walk(v, w, at, out, cut),
                    (v, w) => push(out, cut, at, v, w),
                }
                at.pop();
            }
        }
        _ => push(out, cut, at, Some(a), Some(b)),
    }
}

/// Paths both lists change: one is the other or contains it.
pub fn paths_overlap(a: &[JsonChange], b: &[JsonChange]) -> Vec<String> {
    let within = |p: &str, q: &str| p == q || q.is_empty() || (p.starts_with(q) && matches!(p.as_bytes().get(q.len()), Some(b'.') | Some(b'[')));
    let mut out: Vec<String> = Vec::new();
    for x in a {
        for y in b {
            if within(&x.path, &y.path) || within(&y.path, &x.path) {
                let p = if x.path.len() <= y.path.len() { &x.path } else { &y.path };
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "{\n  \"name\": \"t\",\n  \"parts\": [\n    { \"size\": [1, 2], \"color\": \"lichen\" },\n    { \"size\": [3, 4] }\n  ],\n  \"seed\": 7\n}\n";

    #[test]
    fn json_edits_change_only_their_bytes() {
        let out = json_set(DOC, "parts[1].size", &Json::arr([Json::from(5.0), Json::from(6.0)])).unwrap();
        assert_eq!(out, DOC.replace("[3, 4]", "[5,6]"));
        let out = json_set(DOC, "seed", &Json::from(8.0)).unwrap();
        assert_eq!(out, DOC.replace("\"seed\": 7", "\"seed\": 8"));
        // A new member goes after the last, on its own line, indented like it.
        let out = json_set(DOC, "notes", &Json::from("hi")).unwrap();
        assert!(out.contains("\"seed\": 7,\n  \"notes\": \"hi\"\n}"), "{out}");
        // On one line, it stays on that line.
        let out = json_set(DOC, "parts[1].color", &Json::from("bone")).unwrap();
        assert!(out.contains("{ \"size\": [3, 4], \"color\": \"bone\" }"), "{out}");
        // Missing objects on the way are made; appending at the end of an array works.
        let out = json_set(DOC, "budget.max_triangles", &Json::from(400.0)).unwrap();
        assert_eq!(Json::parse(&out).unwrap().get("budget").unwrap().get("max_triangles").unwrap().as_f64(), Some(400.0));
        let out = json_set(DOC, "parts[2]", &Json::obj([("size", Json::arr([]))])).unwrap();
        assert_eq!(Json::parse(&out).unwrap().get("parts").unwrap().as_array().unwrap().len(), 3);
        assert!(json_set(DOC, "parts[5]", &Json::Null).is_err());
        assert!(json_set(DOC, "name.x", &Json::Null).is_err(), "a string has no fields");
        assert!(json_set("{ bad", "a", &Json::Null).is_err());
    }

    #[test]
    fn json_removes_take_their_comma() {
        let out = json_remove(DOC, "parts[0].color").unwrap();
        assert!(out.contains("{ \"size\": [1, 2] }"), "{out}");
        let out = json_remove(DOC, "seed").unwrap();
        assert!(Json::parse(&out).unwrap().get("seed").is_none());
        assert!(out.ends_with("  ]\n}\n"), "{out}");
        let out = json_remove(DOC, "name").unwrap();
        assert!(out.starts_with("{\n  \"parts\""), "{out}");
        let out = json_remove(DOC, "parts[0]").unwrap();
        assert_eq!(Json::parse(&out).unwrap().get("parts").unwrap().as_array().unwrap().len(), 1);
        assert!(json_remove(DOC, "nope").is_err());
        assert!(json_remove(DOC, "").is_err());
    }

    #[test]
    fn text_edits() {
        assert_eq!(find_replace("a b a", "b", "c").unwrap(), "a c a");
        assert!(find_replace("a b a", "a", "c").unwrap_err().contains("2 times"));
        assert!(find_replace("a", "z", "c").is_err());
        let t = "one\ntwo\nthree\n";
        assert_eq!(replace_lines(t, 2, 2, "TWO").unwrap(), "one\nTWO\nthree\n");
        assert_eq!(replace_lines(t, 2, 1, "new\n").unwrap(), "one\nnew\ntwo\nthree\n");
        assert_eq!(replace_lines(t, 4, 3, "four\n").unwrap(), "one\ntwo\nthree\nfour\n");
        assert_eq!(replace_lines(t, 1, 3, "").unwrap(), "");
        assert!(replace_lines(t, 0, 1, "x").is_err());
        assert!(replace_lines(t, 2, 9, "x").is_err());
    }

    #[test]
    fn line_diffs_and_overlap() {
        let base = "a\nb\nc\nd\ne\n";
        let theirs = "a\nB\nc\nd\ne\n";
        let mine = "a\nb\nc\nd\nE\n";
        let (t, _) = line_diff(base, theirs);
        let (m, _) = line_diff(base, mine);
        assert_eq!(t, vec![Hunk { base: (1, 2), removed: vec!["b".into()], added: vec!["B".into()] }]);
        assert_eq!(m[0].base, (4, 5));
        assert!(hunks_overlap(&t, &m).is_empty());
        let (both, _) = line_diff(base, "a\nX\nc\nd\ne\n");
        assert_eq!(hunks_overlap(&t, &both), vec![(1, 2)]);
        let (ins, _) = line_diff("a\nb\n", "a\nnew\nb\n");
        assert_eq!(ins, vec![Hunk { base: (1, 1), removed: vec![], added: vec!["new".into()] }]);
        assert_eq!(line_diff(base, base).0, vec![]);
    }

    #[test]
    fn json_diffs_by_path() {
        let a = Json::parse(DOC).unwrap();
        let b = Json::parse(&json_set(DOC, "parts[1].size", &Json::arr([Json::from(5.0)])).unwrap()).unwrap();
        let c = Json::parse(&json_set(DOC, "seed", &Json::from(9.0)).unwrap()).unwrap();
        let (ab, _) = json_diff(&a, &b);
        let (ac, _) = json_diff(&a, &c);
        assert_eq!(ab.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["parts[1].size[0]", "parts[1].size[1]"]);
        assert_eq!(ac[0].path, "seed");
        assert!(paths_overlap(&ab, &ac).is_empty());
        let whole = vec![JsonChange { path: "parts".into(), before: None, after: None }];
        assert_eq!(paths_overlap(&ab, &whole), vec!["parts".to_owned()]);
    }
}

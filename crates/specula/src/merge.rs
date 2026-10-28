//! Suggested three-way merges of structured text data (M4 conflict help; docs/design/REVIEW.md).
//!
//! When two agents change the same data, M3's conflict help said what changed (`base`, `theirs`,
//! `yours`). Where the engine knows the data's shape (JSON: Lex items, recipes, UI screens,
//! entity props, project files), it can go one step further and suggest the merge:
//!
//! - objects merge key by key;
//! - arrays of objects are matched by identity: `id`, then `name` (when every element of all three
//!   versions has one, unique), so two agents adding different items to one list both land, and a
//!   change to one item merges with a change to another;
//! - arrays of strings that the kind says are sets (`tags`) merge as sets;
//! - other arrays merge position by position when all three have the same length, and are atomic
//!   otherwise;
//! - a value both sides changed differently is a **conflict**: listed with its JSON path and the
//!   three values; the merged result keeps `theirs` there (never silently overwriting someone's
//!   newer change) unless the caller prefers `yours`.
//!
//! Objects compare as maps (key order never matters), numbers by value. Pure and deterministic:
//! the same three inputs give the same merge. Bounded: depth [`MAX_DEPTH`], [`MAX_CONFLICTS`]
//! conflicts listed (all are counted).

use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};

/// Deepest nesting merged; deeper values are compared whole.
pub const MAX_DEPTH: usize = 64;
/// Conflicts listed (the rest are counted).
pub const MAX_CONFLICTS: usize = 256;
/// Largest side accepted by `merge_suggest`, as compact JSON.
pub const MAX_SIDE_BYTES: usize = 256 << 10;

/// What is merged: picks the identity keys and set-valued fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Any JSON: `id` / `name` identities only.
    Json,
    /// A Lex item (`specula-lex`): `tags` are sets.
    Lex,
    /// A procedural recipe (`specula-model`): bones and parts by `name`.
    Recipe,
    /// A UI screen or theme (`specula-ui`): widgets by `id`.
    Ui,
    /// Entity `props` (game data): `tags` are sets.
    Props,
}

impl Kind {
    pub const NAMES: [&'static str; 5] = ["json", "lex", "recipe", "ui", "props"];

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "json" => Kind::Json,
            "lex" => Kind::Lex,
            "recipe" => Kind::Recipe,
            "ui" => Kind::Ui,
            "props" => Kind::Props,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Json => "json",
            Kind::Lex => "lex",
            Kind::Recipe => "recipe",
            Kind::Ui => "ui",
            Kind::Props => "props",
        }
    }

    /// Whether the array under `field` is a set of strings.
    fn is_set(self, field: &str) -> bool {
        match self {
            Kind::Lex | Kind::Props | Kind::Json => field == "tags",
            Kind::Recipe => matches!(field, "tags" | "keywords"),
            Kind::Ui => field == "tags",
        }
    }

    /// Identity keys tried, in order, for arrays of objects.
    fn identity_keys(self) -> &'static [&'static str] {
        match self {
            Kind::Ui => &["id", "name"],
            Kind::Recipe => &["name", "id"],
            _ => &["id", "name"],
        }
    }
}

/// Which side a conflict keeps in the merged result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Prefer {
    #[default]
    Theirs,
    Yours,
}

/// A value both sides changed differently.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// JSON path: `a.b[3]`, `behaviours[id=fire].range`; empty for the whole value.
    pub path: String,
    /// `None`: absent on that side (not there yet, or deleted).
    pub base: Option<Json>,
    pub theirs: Option<Json>,
    pub yours: Option<Json>,
}

/// A conflict side bigger than this (compact JSON) is shown by its size only.
pub const MAX_SIDE_SHOWN: usize = 2048;

/// `v`, or `{omitted_bytes}` when it's bigger than [`MAX_SIDE_SHOWN`].
pub fn shown(v: &Json) -> Json {
    let n = v.to_string().len();
    if n <= MAX_SIDE_SHOWN {
        v.clone()
    } else {
        Json::obj([("omitted_bytes", Json::from(n))])
    }
}

impl Conflict {
    /// The conflict with each side capped at [`MAX_SIDE_SHOWN`] bytes.
    pub fn to_json(&self) -> Json {
        let side = |v: &Option<Json>| v.as_ref().map_or_else(|| Json::obj([("absent", Json::Bool(true))]), shown);
        Json::obj([
            ("path", Json::from(if self.path.is_empty() { "(whole value)" } else { self.path.as_str() })),
            ("base", side(&self.base)),
            ("theirs", side(&self.theirs)),
            ("yours", side(&self.yours)),
        ])
    }
}

/// A suggested merge.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    /// `None`: the merged value is absent (both deleted it, or one deleted it unchanged by the other).
    pub merged: Option<Json>,
    /// The first [`MAX_CONFLICTS`] conflicts, in document order.
    pub conflicts: Vec<Conflict>,
    pub conflicts_total: usize,
    /// Paths where only yours changed something (taken into the merge).
    pub yours_taken: usize,
    /// Paths where only theirs changed something (kept).
    pub theirs_kept: usize,
}

impl Merge {
    pub fn clean(&self) -> bool {
        self.conflicts_total == 0
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("clean", Json::Bool(self.clean())),
            ("merged", self.merged.clone().unwrap_or(Json::Null)),
            ("conflicts", Json::arr(self.conflicts.iter().map(Conflict::to_json))),
            ("yours_taken", Json::from(self.yours_taken)),
            ("theirs_kept", Json::from(self.theirs_kept)),
        ];
        if self.merged.is_none() {
            f.push(("merged_absent", Json::Bool(true)));
        }
        if self.conflicts_total > self.conflicts.len() {
            f.push(("conflicts_total", Json::from(self.conflicts_total)));
        }
        Json::obj(f)
    }
}

/// Whether two JSON values are the same data: objects as maps, numbers by value.
pub fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Obj(x), Json::Obj(y)) => {
            if x.len() != y.len() {
                return false;
            }
            let ym: BTreeMap<&str, &Json> = y.iter().map(|(k, v)| (k.as_str(), v)).collect();
            if ym.len() != y.len() {
                // Duplicate keys: compare as written.
                return x == y;
            }
            x.iter().all(|(k, v)| ym.get(k.as_str()).is_some_and(|w| same(v, w)))
        }
        (Json::Arr(x), Json::Arr(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        _ => a == b,
    }
}

fn same_opt(a: Option<&Json>, b: Option<&Json>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same(x, y),
        _ => false,
    }
}

/// Three-way merge of `base`, `theirs` and `yours` (each `None` when absent).
pub fn merge3(base: Option<&Json>, theirs: Option<&Json>, yours: Option<&Json>, kind: Kind, prefer: Prefer) -> Merge {
    merge3_under("", base, theirs, yours, kind, prefer)
}

/// [`merge3`] of a value that sits under the key `field` (so a top-level `tags` list merges as
/// a set; conflict paths stay relative to the value).
pub fn merge3_under(field: &str, base: Option<&Json>, theirs: Option<&Json>, yours: Option<&Json>, kind: Kind, prefer: Prefer) -> Merge {
    let mut m = Merger { kind, prefer, conflicts: Vec::new(), total: 0, yours_taken: 0, theirs_kept: 0 };
    let merged = m.value("", field, base, theirs, yours, 0);
    Merge { merged, conflicts: m.conflicts, conflicts_total: m.total, yours_taken: m.yours_taken, theirs_kept: m.theirs_kept }
}

struct Merger {
    kind: Kind,
    prefer: Prefer,
    conflicts: Vec<Conflict>,
    total: usize,
    yours_taken: usize,
    theirs_kept: usize,
}

fn child(path: &str, key: &str) -> String {
    let plain = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    match (path.is_empty(), plain) {
        (true, true) => key.to_owned(),
        (false, true) => format!("{path}.{key}"),
        (_, false) => format!("{path}[{}]", Json::from(key)),
    }
}

impl Merger {
    fn conflict(&mut self, path: &str, b: Option<&Json>, t: Option<&Json>, y: Option<&Json>) -> Option<Json> {
        self.total += 1;
        if self.conflicts.len() < MAX_CONFLICTS {
            self.conflicts.push(Conflict { path: path.to_owned(), base: b.cloned(), theirs: t.cloned(), yours: y.cloned() });
        }
        match self.prefer {
            Prefer::Theirs => t.cloned(),
            Prefer::Yours => y.cloned(),
        }
    }

    /// `field`: the key this value sits under (for the kind's set fields).
    fn value(&mut self, path: &str, field: &str, b: Option<&Json>, t: Option<&Json>, y: Option<&Json>, depth: usize) -> Option<Json> {
        if same_opt(t, y) {
            return t.cloned();
        }
        if same_opt(b, t) {
            self.yours_taken += 1;
            return y.cloned();
        }
        if same_opt(b, y) {
            self.theirs_kept += 1;
            return t.cloned();
        }
        if depth >= MAX_DEPTH {
            return self.conflict(path, b, t, y);
        }
        match (b, t, y) {
            (Some(Json::Obj(_)) | None, Some(Json::Obj(tv)), Some(Json::Obj(yv))) => {
                let empty = Vec::new();
                let bv = match b {
                    Some(Json::Obj(v)) => v,
                    _ => &empty,
                };
                Some(self.object(path, bv, tv, yv, depth))
            }
            (Some(Json::Arr(_)) | None, Some(Json::Arr(tv)), Some(Json::Arr(yv))) => {
                let empty = Vec::new();
                let bv = match b {
                    Some(Json::Arr(v)) => v,
                    _ => &empty,
                };
                self.array(path, field, bv, tv, yv, b.is_some(), depth)
            }
            _ => self.conflict(path, b, t, y),
        }
    }

    fn object(&mut self, path: &str, b: &[(String, Json)], t: &[(String, Json)], y: &[(String, Json)], depth: usize) -> Json {
        // Indexed (a duplicate key keeps its first value, like `Json::get`), so a big object merges
        // in n log n.
        let (bm, tm, ym) = (index(b), index(t), index(y));
        // Theirs' order, then keys new in yours (in yours' order); keys only in base were deleted.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut keys: Vec<&str> = Vec::with_capacity(t.len() + y.len());
        for (k, _) in t.iter().chain(y).chain(b) {
            if seen.insert(k.as_str()) {
                keys.push(k);
            }
        }
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let (bv, tv, yv) = (bm.get(k).copied(), tm.get(k).copied(), ym.get(k).copied());
            if let Some(v) = self.value(&child(path, k), k, bv, tv, yv, depth + 1) {
                out.push((k.to_owned(), v));
            }
        }
        Json::Obj(out)
    }

    /// The identity of each element under `key`, when every element of all three arrays is an
    /// object with a unique string or integer `key`.
    fn identity(&self, arrays: [&[Json]; 3]) -> Option<&'static str> {
        'keys: for key in self.kind.identity_keys() {
            for a in arrays {
                let mut seen = BTreeSet::new();
                for e in a {
                    match e.get(key).and_then(id_text) {
                        Some(id) if !seen.contains(&id) => {
                            seen.insert(id);
                        }
                        _ => continue 'keys,
                    }
                }
            }
            if arrays.iter().any(|a| !a.is_empty()) {
                return Some(key);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn array(&mut self, path: &str, field: &str, b: &[Json], t: &[Json], y: &[Json], had_base: bool, depth: usize) -> Option<Json> {
        if self.kind.is_set(field) && [b, t, y].iter().all(|a| a.iter().all(|e| matches!(e, Json::Str(_)))) {
            return Some(self.set(b, t, y));
        }
        if let Some(key) = self.identity([b, t, y]) {
            return Some(self.keyed(path, key, b, t, y, depth));
        }
        if had_base && b.len() == t.len() && t.len() == y.len() {
            let mut out = Vec::with_capacity(t.len());
            for i in 0..t.len() {
                if let Some(v) = self.value(&format!("{path}[{i}]"), "", Some(&b[i]), Some(&t[i]), Some(&y[i]), depth + 1) {
                    out.push(v);
                }
            }
            return Some(Json::Arr(out));
        }
        let (bj, tj, yj) = (had_base.then(|| Json::Arr(b.to_vec())), Json::Arr(t.to_vec()), Json::Arr(y.to_vec()));
        self.conflict(path, bj.as_ref(), Some(&tj), Some(&yj))
    }

    /// Strings as a set: theirs, minus what yours removed, plus what yours added (yours' order).
    fn set(&mut self, b: &[Json], t: &[Json], y: &[Json]) -> Json {
        let strs = |a: &[Json]| -> BTreeSet<String> { a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect() };
        let (bs, ys) = (strs(b), strs(y));
        let mut out: Vec<Json> = Vec::with_capacity(t.len() + y.len());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for v in t {
            let s = v.as_str().unwrap_or_default();
            if !(bs.contains(s) && !ys.contains(s)) && seen.insert(s.to_owned()) {
                out.push(v.clone());
            }
        }
        for v in y {
            let s = v.as_str().unwrap_or_default();
            if !bs.contains(s) && seen.insert(s.to_owned()) {
                out.push(v.clone());
            }
        }
        if out.len() != t.len() || !out.iter().zip(t).all(|(a, b)| a == b) {
            self.yours_taken += 1;
        }
        Json::Arr(out)
    }

    fn keyed(&mut self, path: &str, key: &str, b: &[Json], t: &[Json], y: &[Json], depth: usize) -> Json {
        let ids = |a: &[Json]| -> Vec<String> { a.iter().map(|e| e.get(key).and_then(id_text).unwrap_or_default()).collect() };
        let (bi, ti, yi) = (ids(b), ids(t), ids(y));
        let index = |ids: &[String], a: &'_ [Json]| -> BTreeMap<String, Json> { ids.iter().cloned().zip(a.iter().cloned()).collect() };
        let (bm, tm, ym) = (index(&bi, b), index(&ti, t), index(&yi, y));
        // Theirs' order; ids new in yours go right after the yours element before them that
        // theirs has (or first), so a list keeps yours' local order for what yours added.
        let mut after: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        let mut anchor: Option<String> = None;
        for id in &yi {
            if tm.contains_key(id) {
                anchor = Some(id.clone());
            } else {
                after.entry(anchor.clone()).or_default().push(id.clone());
            }
        }
        let mut order: Vec<String> = Vec::with_capacity(ti.len() + yi.len());
        order.extend(after.remove(&None).unwrap_or_default());
        for id in &ti {
            order.push(id.clone());
            order.extend(after.remove(&Some(id.clone())).unwrap_or_default());
        }
        // Deleted by both: still visited (and dropped).
        order.extend(bi.iter().filter(|id| !tm.contains_key(*id) && !ym.contains_key(*id)).cloned());
        let mut out = Vec::with_capacity(order.len());
        for id in &order {
            let p = format!("{path}[{key}={id}]");
            let (bv, tv, yv) = (bm.get(id).cloned(), tm.get(id).cloned(), ym.get(id).cloned());
            if let Some(v) = self.value(&p, "", bv.as_ref(), tv.as_ref(), yv.as_ref(), depth + 1) {
                out.push(v);
            }
        }
        Json::Arr(out)
    }
}

/// An object's keys indexed (a duplicate key keeps its first value, like `Json::get`).
fn index(v: &[(String, Json)]) -> BTreeMap<&str, &Json> {
    let mut m = BTreeMap::new();
    for (k, j) in v {
        m.entry(k.as_str()).or_insert(j);
    }
    m
}

fn id_text(v: &Json) -> Option<String> {
    match v {
        Json::Str(s) if !s.is_empty() => Some(s.clone()),
        Json::Num(n) if n.fract() == 0.0 && n.is_finite() => Some(format!("{n}")),
        _ => None,
    }
}

/// Applies Lex-style patch paths (`a.b`, `list[2].x`) to a copy of `item`: `set` values, then
/// `remove`. For merging a stale `lex_edit` patch: yours = the base with your patch applied.
pub fn apply_patch(item: &Json, set: &[(String, Json)], remove: &[String]) -> Result<Json, String> {
    let mut out = item.clone();
    for (p, v) in set {
        let slot = walk(&mut out, p, true)?;
        *slot = v.clone();
    }
    for p in remove {
        let (parent, last) = split_last(p)?;
        let target = if parent.is_empty() { &mut out } else { walk(&mut out, &parent, false)? };
        match (target, last) {
            (Json::Obj(pairs), Seg::Key(k)) if pairs.iter().any(|(x, _)| *x == k) => pairs.retain(|(x, _)| *x != k),
            (Json::Arr(a), Seg::Index(i)) if i < a.len() => {
                a.remove(i);
            }
            _ => return Err(format!("'{p}' isn't there to remove")),
        }
    }
    Ok(out)
}

enum Seg {
    Key(String),
    Index(usize),
}

fn segments(path: &str) -> Result<Vec<Seg>, String> {
    let mut out = Vec::new();
    for part in path.split('.') {
        if part.is_empty() {
            return Err(format!("'{path}' is not a field path like \"a.b[0].c\" (an empty segment)"));
        }
        let (name, mut rest) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        if !name.is_empty() {
            out.push(Seg::Key(name.to_owned()));
        }
        while let Some(r) = rest.strip_prefix('[') {
            let end = r.find(']').ok_or(format!("'{path}': unclosed '['"))?;
            let i: usize = r[..end].parse().map_err(|_| format!("'{path}': '[{}]' must be an index", &r[..end]))?;
            out.push(Seg::Index(i));
            rest = &r[end + 1..];
        }
        if !rest.is_empty() || (name.is_empty() && out.is_empty()) {
            return Err(format!("'{path}' is not a field path like \"a.b[0].c\""));
        }
    }
    Ok(out)
}

fn split_last(path: &str) -> Result<(String, Seg), String> {
    let mut segs = segments(path)?;
    let last = segs.pop().ok_or(format!("'{path}' is empty"))?;
    let mut parent = String::new();
    for s in segs {
        match s {
            Seg::Key(k) => {
                if !parent.is_empty() {
                    parent.push('.');
                }
                parent.push_str(&k);
            }
            Seg::Index(i) => parent.push_str(&format!("[{i}]")),
        }
    }
    Ok((parent, last))
}

fn walk<'a>(root: &'a mut Json, path: &str, create: bool) -> Result<&'a mut Json, String> {
    let mut cur = root;
    for s in segments(path)? {
        cur = match s {
            Seg::Key(k) => {
                if cur.is_null() && create {
                    *cur = Json::Obj(Vec::new());
                }
                let Json::Obj(pairs) = cur else { return Err(format!("'{path}': '{k}' is under something that isn't an object")) };
                let at = match pairs.iter().position(|(x, _)| *x == k) {
                    Some(i) => i,
                    None if create => {
                        pairs.push((k, Json::Null));
                        pairs.len() - 1
                    }
                    None => return Err(format!("'{path}': no '{k}'")),
                };
                &mut pairs[at].1
            }
            Seg::Index(i) => {
                let Json::Arr(a) = cur else { return Err(format!("'{path}': [{i}] is under something that isn't a list")) };
                let n = a.len();
                if create && i == n {
                    // Like Lex's patches: one past the end appends.
                    a.push(Json::Null);
                }
                a.get_mut(i).ok_or(format!("'{path}': index {i} is past the list's end ({n} items)"))?
            }
        };
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn j(s: &str) -> Json {
        Json::parse(s).unwrap()
    }

    fn m(b: &str, t: &str, y: &str, kind: Kind) -> Merge {
        merge3(Some(&j(b)), Some(&j(t)), Some(&j(y)), kind, Prefer::Theirs)
    }

    #[test]
    fn disjoint_changes_merge_cleanly() {
        let r = m(r#"{"hp": 10, "speed": 2, "name": "ghoul"}"#, r#"{"hp": 12, "speed": 2, "name": "ghoul"}"#, r#"{"name": "ghoul", "speed": 3, "hp": 10}"#, Kind::Lex);
        assert!(r.clean(), "{:?}", r.conflicts);
        assert!(same(r.merged.as_ref().unwrap(), &j(r#"{"hp": 12, "speed": 3, "name": "ghoul"}"#)));
        assert_eq!((r.yours_taken, r.theirs_kept), (1, 1));
        // Key order doesn't make a change.
        assert!(same(&j(r#"{"a":1,"b":2}"#), &j(r#"{"b":2,"a":1}"#)) && !same(&j("[1,2]"), &j("[2,1]")));
        assert!(same(&j("1.0"), &j("1")));
    }

    #[test]
    fn both_changing_one_value_is_a_conflict_that_keeps_theirs() {
        let r = m(r#"{"a": {"range": "6m"}}"#, r#"{"a": {"range": "8m"}}"#, r#"{"a": {"range": "7m"}}"#, Kind::Json);
        assert!(!r.clean());
        assert_eq!(r.conflicts[0].path, "a.range");
        assert_eq!(r.conflicts[0].yours, Some(j(r#""7m""#)));
        assert!(same(r.merged.as_ref().unwrap(), &j(r#"{"a": {"range": "8m"}}"#)));
        let y = merge3(Some(&j("1")), Some(&j("2")), Some(&j("3")), Kind::Json, Prefer::Yours);
        assert_eq!((y.merged, y.conflicts[0].path.as_str()), (Some(j("3")), ""));
        let out = r.to_json();
        assert_eq!(out.get("clean"), Some(&Json::Bool(false)));
    }

    #[test]
    fn lists_of_named_items_merge_by_identity() {
        let base = r#"{"behaviours": [{"id": "claw", "dmg": 3}, {"id": "bite", "dmg": 2}]}"#;
        let theirs = r#"{"behaviours": [{"id": "claw", "dmg": 4}, {"id": "bite", "dmg": 2}, {"id": "howl", "dmg": 0}]}"#;
        let yours = r#"{"behaviours": [{"id": "claw", "dmg": 3}, {"id": "spit", "dmg": 1}, {"id": "bite", "dmg": 5}]}"#;
        let r = m(base, theirs, yours, Kind::Lex);
        assert!(r.clean(), "{:?}", r.conflicts);
        let want = r#"{"behaviours": [{"id": "claw", "dmg": 4}, {"id": "spit", "dmg": 1}, {"id": "bite", "dmg": 5}, {"id": "howl", "dmg": 0}]}"#;
        assert!(same(r.merged.as_ref().unwrap(), &j(want)), "{}", r.merged.unwrap());
        // Deleted by them, unchanged by you: gone. Deleted by them, changed by you: a conflict.
        let r = m(r#"[{"name": "a"}, {"name": "b", "x": 1}]"#, r#"[{"name": "a"}]"#, r#"[{"name": "a"}, {"name": "b", "x": 1}]"#, Kind::Recipe);
        assert!(r.clean() && same(r.merged.as_ref().unwrap(), &j(r#"[{"name": "a"}]"#)));
        let r = m(r#"[{"name": "a"}, {"name": "b", "x": 1}]"#, r#"[{"name": "a"}]"#, r#"[{"name": "a"}, {"name": "b", "x": 2}]"#, Kind::Recipe);
        assert_eq!(r.conflicts[0].path, "[name=b]");
        assert_eq!(r.conflicts[0].theirs, None);
    }

    #[test]
    fn sets_positions_and_atomic_lists() {
        let r = m(r#"{"tags": ["a", "b"]}"#, r#"{"tags": ["a", "b", "c"]}"#, r#"{"tags": ["b", "d"]}"#, Kind::Props);
        assert!(r.clean());
        assert!(same(r.merged.as_ref().unwrap(), &j(r#"{"tags": ["b", "c", "d"]}"#)));
        // Same length: by position.
        let r = m("[1, 2, 3]", "[1, 9, 3]", "[1, 2, 4]", Kind::Json);
        assert!(r.clean() && same(r.merged.as_ref().unwrap(), &j("[1, 9, 4]")));
        // Different lengths, no identity: atomic.
        let r = m("[1, 2]", "[1, 2, 3]", "[0, 1, 2]", Kind::Json);
        assert_eq!(r.conflicts_total, 1);
        // Both sides added the same key: fine when equal, a conflict when not.
        assert!(m("{}", r#"{"k": 1}"#, r#"{"k": 1}"#, Kind::Json).clean());
        let r = m("{}", r#"{"k": {"a": 1}}"#, r#"{"k": {"b": 2}}"#, Kind::Json);
        assert!(r.clean() && same(r.merged.as_ref().unwrap(), &j(r#"{"k": {"a": 1, "b": 2}}"#)), "absent base merges objects");
    }

    /// Review finding: object merges were quadratic (20k keys took seconds).
    #[test]
    fn big_objects_merge_quickly_and_conflicts_stay_small() {
        let obj = |f: &dyn Fn(usize) -> i64| Json::Obj((0..20_000).map(|i| (format!("k{i}"), Json::from(f(i)))).collect());
        let (b, t, y) = (obj(&|_| 0), obj(&|i| i64::from(i % 2 == 0)), obj(&|i| 2 * i64::from(i % 3 == 0)));
        let r = merge3(Some(&b), Some(&t), Some(&y), Kind::Json, Prefer::Theirs);
        assert_eq!(r.conflicts.len(), MAX_CONFLICTS);
        assert!(r.conflicts_total > MAX_CONFLICTS);
        let big = Json::from("x".repeat(MAX_SIDE_SHOWN + 1));
        let c = Conflict { path: "a".into(), base: None, theirs: Some(big.clone()), yours: Some(big) };
        assert!(c.to_json().to_string().len() < 400);
    }

    #[test]
    fn deletions_and_depth() {
        // Both deleted: absent.
        let r = merge3(Some(&j("1")), None, None, Kind::Json, Prefer::Theirs);
        assert!(r.clean() && r.merged.is_none());
        let mut deep = Json::from(1);
        let mut deep2 = Json::from(2);
        for _ in 0..(MAX_DEPTH + 5) {
            deep = Json::obj([("x", deep)]);
            deep2 = Json::obj([("x", deep2)]);
        }
        let r = merge3(Some(&Json::obj::<&str>([])), Some(&deep), Some(&deep2), Kind::Json, Prefer::Theirs);
        assert_eq!(r.conflicts_total, 1, "past the depth limit a value is compared whole");
    }

    #[test]
    fn patches_apply_like_lex_paths() {
        let item = j(r#"{"behaviours": [{"range": "6m"}], "notes": "x"}"#);
        let out = apply_patch(&item, &[("behaviours[0].range".into(), j(r#""8m""#)), ("attributes.hp".into(), j(r#""30pt""#))], &["notes".into()]).unwrap();
        assert!(same(&out, &j(r#"{"behaviours": [{"range": "8m"}], "attributes": {"hp": "30pt"}}"#)), "{out}");
        assert!(apply_patch(&item, &[("behaviours[3].range".into(), Json::Null)], &[]).is_err());
        let appended = apply_patch(&item, &[("behaviours[1]".into(), j(r#"{"range": "2m"}"#))], &[]).unwrap();
        assert_eq!(appended.get("behaviours").unwrap().as_array().unwrap().len(), 2, "one past the end appends");
        assert!(apply_patch(&item, &[], &["nope".into()]).is_err());
        assert!(segments("a..b").is_err() && segments("a[x]").is_err() && segments("a[1]b").is_err());
    }
}

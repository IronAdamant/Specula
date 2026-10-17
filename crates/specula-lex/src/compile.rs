//! Document → [`Rules`]: validation with actionable diagnostics, then compilation.
//!
//! Two passes. The first collects names (resources, params with overrides, paths, sites,
//! archetypes with `extends` resolved, attributes and their units, tags, events). The second
//! compiles every item against those names. Errors are collected, not thrown: one call reports
//! as much as it can (capped at [`MAX_DIAGNOSTICS`]), each with the item, the field path, what
//! was expected, what was found and a suggestion.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::diag::{codes, did_you_mean, show, type_name, Diagnostic};
use crate::doc::Document;
use crate::expr::{self, Ex, ExprError, Me, Registry, Scope, Ty};
use crate::rules::*;
use crate::schema::{self, Spec};
use crate::units::{dec_from_f64, human, isqrt, scale, Unit};

/// Most diagnostics one validation returns.
pub const MAX_DIAGNOSTICS: usize = 200;
/// Largest counts and sizes, so a typo can't ask for a billion spawns.
pub const MAX_GROUP_COUNT: i64 = 100_000;
pub const MAX_ITEMS: usize = 10_000;
/// Largest item, as compact JSON: an item is stored as one world entity, whose props are
/// at most 16 KiB. Split big schedules over several `waves` items.
pub const MAX_ITEM_BYTES: usize = 16_000;
/// Most tags in one tag filter, and most tags an effect grants or blocks (tag checks scan
/// them for every candidate).
pub const MAX_FILTER_TAGS: usize = 16;
/// Most attributes and behaviours per archetype, modifiers per effect and payload entries.
pub const MAX_ATTRS_PER_ARCHETYPE: usize = 64;
pub const MAX_BEHAVIOURS: usize = 32;
pub const MAX_MODIFIERS: usize = 8;
pub const MAX_PAYLOAD: usize = 16;
/// Most waves in a rule set.
pub const MAX_WAVES: usize = 10_000;
/// Most spawns one wave schedules (they are all queued when it starts).
pub const MAX_WAVE_SPAWNS: i64 = 100_000;
/// Largest coordinate: 1e9 m (in mm), so squared distances never overflow.
pub const MAX_COORD_MM: i64 = 1_000_000_000_000;
/// Most sites, tags, attributes and RNG streams (they are indexed by u16, and u16::MAX is the
/// "no tag" mark).
pub const MAX_NAMES: usize = 60_000;

/// Compile options.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Param overrides, `(id, value)`, value in the param's human unit (a number or a literal
    /// string such as `"7.5m"`).
    pub overrides: Vec<(String, Json)>,
    /// Native hooks the host provides; `Some` reports declared hooks missing from it
    /// (`native_unbound` warnings). `None` doesn't check.
    pub natives: Option<Vec<String>>,
}

/// A successful compile: the rules and any warnings.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub rules: Rules,
    pub warnings: Vec<Diagnostic>,
}

/// Compiles a document. `Err` holds the diagnostics (errors first, then warnings), at most
/// [`MAX_DIAGNOSTICS`] of them.
pub fn compile(doc: &Document, opts: &Options) -> Result<Compiled, Vec<Diagnostic>> {
    compile_capped(doc, opts, true)
}

/// Every diagnostic, uncapped (for callers that compare two validations, like `lex_edit`).
pub fn validate_all(doc: &Document, opts: &Options) -> Vec<Diagnostic> {
    match compile_capped(doc, opts, false) {
        Ok(c) => c.warnings,
        Err(d) => d,
    }
}

fn compile_capped(doc: &Document, opts: &Options, cap: bool) -> Result<Compiled, Vec<Diagnostic>> {
    let mut cx = Cx::new(doc);
    cx.pass1(opts);
    let rules = cx.pass2(opts);
    let mut diags = cx.diags.into_inner();
    diags.sort_by_key(|d| d.severity);
    if cap && diags.len() > MAX_DIAGNOSTICS {
        let n = diags.len();
        diags.truncate(MAX_DIAGNOSTICS);
        diags.push(Diagnostic::warning(codes::TOO_MANY, "", "", format!("{n} diagnostics; only the first {MAX_DIAGNOSTICS} are shown")).suggest("fix these and validate again"));
    }
    if diags.iter().any(Diagnostic::is_error) {
        Err(diags)
    } else {
        Ok(Compiled { rules, warnings: diags })
    }
}

/// Every diagnostic for a document (empty = valid with no warnings).
pub fn validate(doc: &Document, opts: &Options) -> Vec<Diagnostic> {
    match compile(doc, opts) {
        Ok(c) => c.warnings,
        Err(d) => d,
    }
}

/// A JSON value inside an item, with its path for messages.
#[derive(Clone)]
struct V<'a> {
    item: &'a str,
    path: String,
    j: &'a Json,
}

impl<'a> V<'a> {
    fn key(&self, k: &str) -> Option<V<'a>> {
        let j = self.j.get(k)?;
        let path = if self.path.is_empty() { k.to_owned() } else { format!("{}.{k}", self.path) };
        Some(V { item: self.item, path, j })
    }

    fn index(&self, i: usize, j: &'a Json) -> V<'a> {
        V { item: self.item, path: format!("{}[{i}]", self.path), j }
    }

    fn sub(&self, name: &str, j: &'a Json) -> V<'a> {
        let path = if self.path.is_empty() { name.to_owned() } else { format!("{}.{name}", self.path) };
        V { item: self.item, path, j }
    }

    fn err(&self, code: &'static str, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::error(code, self.item, self.path.as_str(), msg)
    }

    fn warn(&self, code: &'static str, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::warning(code, self.item, self.path.as_str(), msg)
    }
}

/// Where actions are compiled: the expression scope and the machine `goto` switches.
#[derive(Clone, Copy)]
struct Act<'s> {
    scope: Scope<'s>,
    machine: Option<u16>,
}

struct Cx<'d> {
    doc: &'d Document,
    hz: u32,
    diags: RefCell<Vec<Diagnostic>>,
    reg: Registry,
    /// Item id → kind, for items with a valid kind.
    kinds: BTreeMap<String, &'static str>,
    /// Archetype id → its effective JSON (extends resolved).
    eff: BTreeMap<String, Json>,
    idx: BTreeMap<&'static str, BTreeMap<String, u16>>,
    paths: Vec<PathDef>,
    sites: Vec<SiteDef>,
    params: Vec<ParamDef>,
    resources: Vec<ResourceDef>,
    /// Archetype → (attr id, base) in attr-id order.
    arch_attrs: Vec<Vec<(u16, i64)>>,
    events: Vec<EventDef>,
    /// Event name → index (custom names are many: a linear search per name was quadratic).
    event_idx: BTreeMap<String, u16>,
    /// More custom events than [`MAX_NAMES`]: already one `too_many`, so the dropped names
    /// aren't each reported again.
    events_overflow: bool,
    machines: Vec<MachineDef>,
    natives: Vec<NativeDef>,
}

impl<'d> Cx<'d> {
    fn new(doc: &'d Document) -> Cx<'d> {
        Cx {
            doc,
            hz: doc.tick_hz,
            diags: RefCell::new(Vec::new()),
            reg: Registry { tick_hz: doc.tick_hz, ..Default::default() },
            kinds: BTreeMap::new(),
            eff: BTreeMap::new(),
            idx: BTreeMap::new(),
            paths: Vec::new(),
            sites: Vec::new(),
            params: Vec::new(),
            resources: Vec::new(),
            arch_attrs: Vec::new(),
            events: Vec::new(),
            event_idx: BTreeMap::new(),
            events_overflow: false,
            machines: Vec::new(),
            natives: Vec::new(),
        }
    }

    fn push(&self, d: Diagnostic) {
        self.diags.borrow_mut().push(d);
    }

    /// Once more diagnostics exist than a report shows, suggestions aren't worth computing.
    fn full(&self) -> bool {
        self.diags.borrow().len() >= MAX_DIAGNOSTICS
    }

    /// `did_you_mean`, until the diagnostics are [`Cx::full`].
    fn dym<'a>(&self, key: &str, candidates: impl IntoIterator<Item = &'a str> + Clone) -> Option<String> {
        if self.full() {
            None
        } else {
            did_you_mean(key, candidates)
        }
    }

    fn ids(&self, kind: &str) -> Vec<String> {
        self.kinds.iter().filter(|(_, k)| **k == kind).map(|(id, _)| id.clone()).collect()
    }

    fn index_of(&self, kind: &str, id: &str) -> Option<u16> {
        self.idx.get(kind).and_then(|m| m.get(id)).copied()
    }

    fn item(&self, id: &'d str) -> V<'d> {
        V { item: id, path: String::new(), j: &self.doc.items[id] }
    }

    // ---- pass 1: names ----

    fn pass1(&mut self, opts: &Options) {
        let doc = self.doc;
        if doc.items.len() > MAX_ITEMS {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "items", format!("{} items; at most {MAX_ITEMS}", doc.items.len())));
        }
        let kind_names: Vec<&str> = schema::KINDS.iter().map(|k| k.name).collect();
        for (id, item) in &doc.items {
            let v = V { item: id, path: String::new(), j: item };
            let size = item.to_string().len();
            if size > MAX_ITEM_BYTES {
                self.push(
                    v.err(codes::TOO_LARGE, format!("the item is {size} bytes; an item is at most {MAX_ITEM_BYTES} (it is stored as one world entity)"))
                        .suggest(match item.get("kind").and_then(Json::as_str) {
                            Some("waves") => "split it: several waves items join in id order (waves_01, waves_02, ...)",
                            Some("archetype") => "move shared parts into a base archetype that this one extends, or split its behaviours over archetypes",
                            Some("timer") | Some("rule") | Some("command") => "move long action lists into custom events: emit one, and handle it in rules",
                            Some("sites") => "split the points over several sites items",
                            Some("machine") => "split it into smaller machines, or move state actions into rules on custom events",
                            _ => "split it into several smaller items",
                        }),
                );
            }
            if !matches!(item, Json::Obj(_)) {
                self.push(v.err(codes::WRONG_TYPE, format!("an item must be an object, not {}", type_name(item))).suggest("e.g. {\"kind\": \"resource\", \"start\": 0}"));
                continue;
            }
            match item.get("kind") {
                None => self.push(
                    Diagnostic::error(codes::MISSING_FIELD, id.as_str(), "kind", "the item has no kind").expected(kind_names.join(", ")).suggest("add \"kind\": \"<one of these>\""),
                ),
                Some(Json::Str(k)) => match schema::kind(k) {
                    Some(spec) => {
                        self.kinds.insert(id.clone(), spec.name);
                    }
                    None => self.push(
                        Diagnostic::error(codes::UNKNOWN_REF, id.as_str(), "kind", format!("'{k}' is not an item kind"))
                            .expected(kind_names.join(", "))
                            .got(k.as_str())
                            .suggest_opt(self.dym(k, kind_names.iter().copied())),
                    ),
                },
                Some(other) => self.push(Diagnostic::error(codes::WRONG_TYPE, id.as_str(), "kind", "kind must be a string").got(show(other))),
            }
        }
        for kind in schema::KINDS.iter().map(|k| k.name) {
            let ids = self.ids(kind);
            let m: BTreeMap<String, u16> = ids.into_iter().enumerate().map(|(i, id)| (id, i as u16)).collect();
            self.idx.insert(kind, m);
        }
        self.reg.has_waves = !self.ids("waves").is_empty();

        // Params (overrides folded in), resources.
        for id in self.ids("param") {
            let v = self.item(&doc.items.get_key_value(&id).expect("listed").0[..]);
            let p = self.param(&v, opts);
            self.reg.params.insert(id.clone(), (p.value, p.unit));
            self.params.push(p);
        }
        for (name, _) in &opts.overrides {
            if !self.reg.params.contains_key(name) {
                self.push(
                    Diagnostic::error(codes::UNKNOWN_REF, "", format!("params.{name}"), format!("there is no param '{name}' to override"))
                        .suggest_opt(self.dym(name, self.reg.params.keys().map(String::as_str))),
                );
            }
        }
        for id in self.ids("resource") {
            let v = self.item(doc.items.get_key_value(&id).expect("listed").0);
            let r = self.resource(&v);
            self.reg.resources.insert(id.clone(), (self.resources.len() as u16, r.unit));
            self.resources.push(r);
        }
        // Paths and sites.
        for id in self.ids("path") {
            let v = self.item(doc.items.get_key_value(&id).expect("listed").0);
            let p = self.path(&v);
            self.paths.push(p);
        }
        let mut site_points: BTreeMap<String, (P3, String)> = BTreeMap::new();
        for id in self.ids("sites") {
            let v = self.item(doc.items.get_key_value(&id).expect("listed").0);
            self.fields(&v, schema::kind("sites").expect("kind"), &["kind"]);
            let Some(pts) = self.req(&v, "points") else { continue };
            let Some(pairs) = pts.j.as_object() else {
                self.push(pts.err(codes::WRONG_TYPE, "points must be an object of site name → [x, y, z]").got(type_name(pts.j)));
                continue;
            };
            for (name, p) in pairs {
                let pv = pts.sub(name, p);
                if let Some((_, other)) = site_points.get(name) {
                    self.push(pv.err(codes::DUPLICATE, format!("site '{name}' is also defined in '{other}'")).suggest("rename one"));
                    continue;
                }
                if let Some(pos) = self.point(&pv) {
                    site_points.insert(name.clone(), (pos, id.clone()));
                }
            }
        }
        if site_points.len() > MAX_NAMES {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "sites", format!("{} sites; at most {MAX_NAMES}", site_points.len())));
            site_points = site_points.into_iter().take(MAX_NAMES).collect();
        }
        for (i, (name, (pos, _))) in site_points.into_iter().enumerate() {
            self.reg.sites.insert(name.clone(), i as u16);
            self.sites.push(SiteDef { name, pos });
        }
        // Archetypes: extends, then tags and attributes.
        let archs = self.ids("archetype");
        for id in &archs {
            let mut chain = Vec::new();
            if let Some(e) = self.effective(id, &mut chain) {
                self.eff.insert(id.clone(), e);
            }
        }
        for (i, id) in archs.iter().enumerate() {
            self.reg.archetypes.insert(id.clone(), i as u16);
        }
        let mut tags: BTreeSet<String> = BTreeSet::new();
        for id in &archs {
            if let Some(e) = self.eff.get(id) {
                if let Some(Json::Arr(ts)) = e.get("tags") {
                    tags.extend(ts.iter().filter_map(|t| t.as_str().map(str::to_owned)));
                }
            }
        }
        for id in self.ids("effect") {
            if let Some(Json::Arr(ts)) = doc.items[&id].get("grant_tags") {
                tags.extend(ts.iter().filter_map(|t| t.as_str().map(str::to_owned)));
            }
        }
        if tags.len() > MAX_NAMES {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "tags", format!("{} distinct tags; at most {MAX_NAMES}", tags.len())));
            tags = tags.into_iter().take(MAX_NAMES).collect();
        }
        for (i, t) in tags.into_iter().enumerate() {
            self.reg.tags.insert(t, i as u16);
        }
        // Attributes: name → unit, consistent across archetypes.
        let mut attr_units: BTreeMap<String, (Unit, String)> = BTreeMap::new();
        let mut per_arch: Vec<Vec<(String, i64)>> = Vec::new();
        for id in &archs {
            let mut list = Vec::new();
            if let Some(e) = self.eff.get(id).cloned() {
                let v = V { item: doc.items.get_key_value(id).expect("listed").0, path: String::new(), j: &e };
                if let Some(attrs) = v.key("attributes") {
                    match attrs.j.as_object() {
                        None => self.push(attrs.err(codes::WRONG_TYPE, "attributes must be an object of name → value").got(type_name(attrs.j))),
                        Some(pairs) => {
                            for (name, val) in pairs {
                                let av = attrs.sub(name, val);
                                if let Err(d) = crate::doc::check_id(name) {
                                    self.push(av.err(codes::INVALID, format!("'{name}' is not a valid attribute name")).suggest_opt(d.suggestion));
                                    continue;
                                }
                                if ["alive", "archetype", "next", "path_remaining"].contains(&name.as_str()) {
                                    self.push(av.err(codes::INVALID, format!("'{name}' is a built-in entity field, not an attribute")).suggest("choose another name"));
                                    continue;
                                }
                                let Some((unit, base)) = self.attribute(&av) else { continue };
                                match attr_units.get(name) {
                                    Some((u, owner)) if *u != unit => self.push(
                                        av.err(codes::UNIT_MISMATCH, format!("attribute '{name}' is {} here but {} in '{owner}'", unit.name(), u.name()))
                                            .suggest("an attribute name has one unit across all archetypes; rename one of them"),
                                    ),
                                    Some(_) => {}
                                    None => {
                                        attr_units.insert(name.clone(), (unit, id.clone()));
                                    }
                                }
                                list.push((name.clone(), base));
                            }
                        }
                    }
                }
            }
            if list.len() > MAX_ATTRS_PER_ARCHETYPE {
                self.push(Diagnostic::error(codes::TOO_MANY, id.as_str(), "attributes", format!("{} attributes; at most {MAX_ATTRS_PER_ARCHETYPE} per archetype", list.len())));
                list.truncate(MAX_ATTRS_PER_ARCHETYPE);
            }
            per_arch.push(list);
        }
        if attr_units.len() > MAX_NAMES {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "attributes", format!("{} distinct attributes; at most {MAX_NAMES}", attr_units.len())));
            attr_units = attr_units.into_iter().take(MAX_NAMES).collect();
        }
        for (i, (name, (unit, _))) in attr_units.iter().enumerate() {
            self.reg.attrs.insert(name.clone(), (i as u16, *unit));
        }
        for list in per_arch {
            let mut v: Vec<(u16, i64)> = list.iter().filter_map(|(n, b)| self.reg.attrs.get(n).map(|(i, _)| (*i, *b))).collect();
            v.sort_by_key(|(i, _)| *i);
            self.reg.arch_attrs.push(v.iter().map(|(i, _)| *i).collect());
            self.arch_attrs.push(v);
        }
        // Events: built-ins, then every name an `emit` raises.
        self.events = BUILTIN_EVENTS.iter().map(|(n, u, _)| EventDef { name: (*n).to_owned(), unit: *u }).collect();
        let mut custom = BTreeSet::new();
        for item in doc.items.values() {
            collect_emits(item, &mut custom);
        }
        for name in custom {
            if !self.event_idx.contains_key(&name) && !BUILTIN_EVENTS.iter().any(|(n, _, _)| *n == name) {
                self.event_idx.insert(name.clone(), 0);
                self.events.push(EventDef { name, unit: Unit::Int });
            }
        }
        if self.events.len() > MAX_NAMES {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "", format!("{} distinct events; at most {MAX_NAMES}", self.events.len())));
            self.events.truncate(MAX_NAMES);
            self.events_overflow = true;
        }
        self.event_idx = self.events.iter().enumerate().map(|(i, e)| (e.name.clone(), i as u16)).collect();
    }

    fn effective(&self, id: &str, chain: &mut Vec<String>) -> Option<Json> {
        if chain.iter().any(|c| c == id) {
            chain.push(id.to_owned());
            self.push(Diagnostic::error(codes::CYCLE, chain[0].as_str(), "extends", format!("archetypes extend each other in a loop: {}", chain.join(" → "))).suggest("remove one extends"));
            return None;
        }
        chain.push(id.to_owned());
        let item = self.doc.items.get(id)?;
        let Some(parent) = item.get("extends") else { return Some(item.clone()) };
        let v = V { item: chain[0].as_str(), path: "extends".into(), j: parent };
        let Some(pid) = parent.as_str() else {
            self.push(v.err(codes::WRONG_TYPE, "extends must be an archetype id").got(show(parent)));
            return Some(item.clone());
        };
        if self.kinds.get(pid) != Some(&"archetype") {
            self.unknown_or_wrong(&v, pid, "archetype");
            return Some(item.clone());
        }
        let base = self.effective(pid, chain)?;
        Some(merge_archetype(&base, item))
    }

    fn param(&self, v: &V, opts: &Options) -> ParamDef {
        self.fields(v, schema::kind("param").expect("kind"), &["kind"]);
        let unit = match v.key("type") {
            None => Unit::Int,
            Some(t) => self.unit_name(&t).unwrap_or(Unit::Int),
        };
        let min = v.key("min").and_then(|m| self.const_num(&m, unit)).unwrap_or(i64::MIN);
        let max = v.key("max").and_then(|m| self.const_num(&m, unit)).unwrap_or(i64::MAX);
        let mut value = self.req(v, "value").and_then(|x| self.const_num(&x, unit)).unwrap_or(0);
        if min > max {
            self.push(v.err(codes::OUT_OF_RANGE, "min is greater than max").suggest("swap them"));
        }
        let range = || format!("{}..={}", show_val(min, unit, self.hz), show_val(max, unit, self.hz));
        if value < min || value > max {
            self.push(v.key("value").unwrap_or(v.clone()).err(codes::OUT_OF_RANGE, format!("value {} is outside {}", human(value, unit, self.hz), range())).expected(range()));
        }
        if let Some((_, o)) = opts.overrides.iter().find(|(n, _)| n == v.item) {
            let ov = V { item: v.item, path: format!("override {}", v.item), j: o };
            if let Some(x) = self.const_num(&ov, unit) {
                if x < min || x > max {
                    self.push(ov.err(codes::OUT_OF_RANGE, format!("override {} is outside the param's range {}", human(x, unit, self.hz), range())).expected(range()).got(show(o)));
                } else {
                    value = x;
                }
            }
        }
        let doc = v.key("doc").and_then(|d| d.j.as_str().map(str::to_owned)).unwrap_or_default();
        ParamDef { id: v.item.to_owned(), unit, value, min, max, doc }
    }

    fn resource(&self, v: &V) -> ResourceDef {
        self.fields(v, schema::kind("resource").expect("kind"), &["kind"]);
        let unit = v.key("unit").and_then(|u| self.unit_name(&u)).unwrap_or(Unit::Int);
        let start = self.req(v, "start").and_then(|x| self.const_num(&x, unit)).unwrap_or(0);
        let min = v.key("min").and_then(|m| self.const_num(&m, unit)).unwrap_or(i64::MIN);
        let max = v.key("max").and_then(|m| self.const_num(&m, unit)).unwrap_or(i64::MAX);
        if min > max {
            self.push(v.err(codes::OUT_OF_RANGE, "min is greater than max").suggest("swap them"));
        } else if start < min || start > max {
            self.push(v.key("start").unwrap_or(v.clone()).err(codes::OUT_OF_RANGE, "start is outside [min, max]").suggest("change start, min or max"));
        }
        ResourceDef { id: v.item.to_owned(), unit, start, min, max }
    }

    fn path(&self, v: &V) -> PathDef {
        self.fields(v, schema::kind("path").expect("kind"), &["kind"]);
        let mut points = Vec::new();
        if let Some(p) = self.req(v, "points") {
            match p.j.as_array() {
                Some(list) => {
                    for (i, item) in list.iter().enumerate() {
                        if let Some(pt) = self.point(&p.index(i, item)) {
                            points.push(pt);
                        }
                    }
                    if list.len() < 2 {
                        self.push(p.err(codes::INVALID, format!("a path needs at least 2 points, got {}", list.len())).suggest("add an end point"));
                    }
                }
                None => self.push(p.err(codes::WRONG_TYPE, "points must be an array of [x, y, z] (metres)").got(type_name(p.j))),
            }
        }
        let looped = v.key("loop").and_then(|l| self.bool_field(&l)).unwrap_or(false);
        let mut seg: Vec<i64> = points.windows(2).map(|w| dist(w[0], w[1])).collect();
        // A loop closes itself: the last segment runs back to the first point.
        if looped && points.len() >= 2 {
            seg.push(dist(points[points.len() - 1], points[0]));
        }
        for (i, s) in seg.iter().enumerate() {
            if *s == 0 {
                self.push(Diagnostic::error(codes::INVALID, v.item, format!("points[{}]", i + 1), "two consecutive points are the same").suggest("remove the duplicate point"));
            }
        }
        let mut after = vec![0i64; seg.len()];
        for i in (0..seg.len().saturating_sub(1)).rev() {
            after[i] = after[i + 1].saturating_add(seg[i + 1]);
        }
        PathDef { id: v.item.to_owned(), points, seg, after, looped }
    }

    fn point(&self, v: &V) -> Option<P3> {
        let Some(list) = v.j.as_array().filter(|l| l.len() == 3) else {
            self.push(v.err(codes::WRONG_TYPE, "a point is [x, y, z] in metres").got(show(v.j)));
            return None;
        };
        let mut out = [0i64; 3];
        for (i, c) in list.iter().enumerate() {
            let x = self.const_num(&v.index(i, c), Unit::Distance)?;
            if x.abs() > MAX_COORD_MM {
                self.push(v.index(i, c).err(codes::OUT_OF_RANGE, format!("{} is too far out: coordinates are at most 1,000,000 km", show(c))).got(show(c)));
                return None;
            }
            out[i] = x;
        }
        Some(out)
    }

    /// An attribute value: a bare number (int), a literal (`"7m"`: the unit from the literal) or
    /// `{"base": ..., "unit": ...}`.
    fn attribute(&self, v: &V) -> Option<(Unit, i64)> {
        match v.j {
            Json::Num(_) => Some((Unit::Int, self.const_num(v, Unit::Int)?)),
            Json::Str(s) => {
                let sc = Scope::new(&self.reg, "an attribute's base value");
                match expr::compile(s, &sc) {
                    Ok((Ex::Int(x), Ty::Num(u))) => {
                        self.drain_warnings(v, s);
                        Some((u, x))
                    }
                    Ok(_) => {
                        self.push(v.err(codes::INVALID, "an attribute's base value must be a constant number").got(s.as_str()).suggest("e.g. \"55pt\", \"0.8m/s\" or {\"base\": 55, \"unit\": \"points\"}"));
                        None
                    }
                    Err(e) => {
                        self.expr_err(v, s, e);
                        None
                    }
                }
            }
            Json::Obj(_) => {
                for (k, _) in v.j.as_object().unwrap_or_default() {
                    if !["base", "unit", "notes"].contains(&k.as_str()) {
                        self.push(v.sub(k, &Json::Null).err(codes::UNKNOWN_FIELD, format!("'{k}' is not an attribute field")).suggest_opt(self.dym(k, ["base", "unit"])));
                    }
                }
                let unit = match v.key("unit") {
                    Some(u) => self.unit_name(&u)?,
                    None => Unit::Int,
                };
                let base = self.req(v, "base").and_then(|b| self.const_num(&b, unit))?;
                Some((unit, base))
            }
            other => {
                self.push(v.err(codes::WRONG_TYPE, format!("an attribute is a number, a literal or {{base, unit}}, not {}", type_name(other))).suggest("e.g. 3, \"55pt\" or {\"base\": 55, \"unit\": \"points\"}"));
                None
            }
        }
    }

    // ---- field helpers ----

    fn fields(&self, v: &V, spec: &Spec, extra: &[&'static str]) -> bool {
        let Some(pairs) = v.j.as_object() else {
            self.push(v.err(codes::WRONG_TYPE, format!("{} must be an object, not {}", spec.name, type_name(v.j))));
            return false;
        };
        let names = spec.names(extra);
        for (k, _) in pairs {
            if !names.contains(&k.as_str()) {
                let p = if v.path.is_empty() { k.clone() } else { format!("{}.{k}", v.path) };
                self.push(
                    Diagnostic::error(codes::UNKNOWN_FIELD, v.item, p, format!("'{k}' is not a field of {}", spec.name))
                        .expected(names.iter().filter(|n| **n != "notes").copied().collect::<Vec<_>>().join(", "))
                        .suggest_opt(self.dym(k, names.iter().copied())),
                );
            }
        }
        true
    }

    fn req<'a>(&self, v: &V<'a>, key: &str) -> Option<V<'a>> {
        let r = v.key(key);
        if r.is_none() {
            let p = if v.path.is_empty() { key.to_owned() } else { format!("{}.{key}", v.path) };
            self.push(Diagnostic::error(codes::MISSING_FIELD, v.item, p, format!("'{key}' is required")).suggest(format!("add \"{key}\"")));
        }
        r
    }

    fn unit_name(&self, v: &V) -> Option<Unit> {
        let names: Vec<&str> = Unit::ALL.iter().map(|u| u.name()).collect();
        match v.j.as_str().and_then(Unit::from_name) {
            Some(u) => Some(u),
            None => {
                self.push(
                    v.err(codes::UNKNOWN_REF, format!("{} is not a unit", show(v.j)))
                        .expected(names.join(", "))
                        .suggest_opt(v.j.as_str().and_then(|s| self.dym(s, names.iter().copied()))),
                );
                None
            }
        }
    }

    fn bool_field(&self, v: &V) -> Option<bool> {
        match v.j {
            Json::Bool(b) => Some(*b),
            other => {
                self.push(v.err(codes::WRONG_TYPE, "expected true or false").got(show(other)));
                None
            }
        }
    }

    fn string(&self, v: &V) -> Option<String> {
        match v.j {
            Json::Str(s) if !s.is_empty() => Some(s.clone()),
            other => {
                self.push(v.err(codes::WRONG_TYPE, "expected a non-empty string").got(show(other)));
                None
            }
        }
    }

    /// A number in `unit` that must be constant: a JSON number (human units) or a literal /
    /// constant expression string.
    fn const_num(&self, v: &V, unit: Unit) -> Option<i64> {
        match self.num_expr(v, unit, &Scope::new(&self.reg, "a constant"))? {
            Ex::Int(x) => Some(x),
            _ => {
                self.push(v.err(codes::INVALID, "this value must be a constant (numbers, literals and $params only)").got(show(v.j)));
                None
            }
        }
    }

    /// A number in `unit`: a JSON number (human units) or an expression.
    fn num_expr(&self, v: &V, unit: Unit, sc: &Scope) -> Option<Ex> {
        match v.j {
            Json::Num(n) => {
                let Some(d) = dec_from_f64(*n) else {
                    self.push(v.err(codes::OUT_OF_RANGE, "the number is not finite or too long").got(show(v.j)));
                    return None;
                };
                let Some(s) = scale(d, unit.per_human(self.hz), 1) else {
                    self.push(v.err(codes::OUT_OF_RANGE, "the number is too large").got(show(v.j)));
                    return None;
                };
                if !s.exact {
                    if unit == Unit::Int {
                        self.push(v.err(codes::WRONG_TYPE, format!("{} must be a whole number", show(v.j))).expected(unit.human()).suggest(format!("use {} or {}", s.below, s.above)));
                        return None;
                    }
                    self.push(
                        v.warn(codes::INEXACT, format!("{} {} is not a whole number of stored units; rounded to {}", show(v.j), unit.human(), human(s.value, unit, self.hz)))
                            .suggest(format!("use {} or {} for an exact value", human_number(s.below, unit, self.hz), human_number(s.above, unit, self.hz))),
                    );
                }
                Some(Ex::Int(s.value))
            }
            Json::Str(src) => match expr::compile_as(src, sc, Ty::Num(unit)) {
                Ok(e) => {
                    self.drain_warnings(v, src);
                    Some(e)
                }
                Err(e) => {
                    self.expr_err(v, src, e);
                    None
                }
            },
            other => {
                self.push(
                    v.err(codes::WRONG_TYPE, format!("expected {} (a number or an expression string), not {}", unit_desc(unit), type_name(other)))
                        .got(show(other))
                        .suggest(format!("e.g. {}", example(unit))),
                );
                None
            }
        }
    }

    fn bool_expr(&self, v: &V, sc: &Scope) -> Option<Ex> {
        match v.j {
            Json::Bool(b) => Some(Ex::Bool(*b)),
            Json::Str(src) => match expr::compile_as(src, sc, Ty::Bool) {
                Ok(e) => {
                    self.drain_warnings(v, src);
                    Some(e)
                }
                Err(e) => {
                    self.expr_err(v, src, e);
                    None
                }
            },
            other => {
                self.push(v.err(codes::WRONG_TYPE, "expected a condition (an expression string or true/false)").got(show(other)).suggest("e.g. \"res.dust >= 50\""));
                None
            }
        }
    }

    fn typed_src(&self, v: &V, src: &str, sc: &Scope, ty: Ty) -> Option<Ex> {
        match expr::compile_as(src, sc, ty) {
            Ok(e) => {
                self.drain_warnings(v, src);
                Some(e)
            }
            Err(e) => {
                self.expr_err(v, src, e);
                None
            }
        }
    }

    fn typed_expr(&self, v: &V, sc: &Scope, ty: Ty) -> Option<Ex> {
        match v.j {
            Json::Str(src) => self.typed_src(v, src, sc, ty),
            other => {
                self.push(v.err(codes::WRONG_TYPE, format!("expected {} (an expression string)", ty.describe())).got(show(other)).suggest(match ty {
                    Ty::Ent => "e.g. \"self\", \"target\" or \"arg.stone\"",
                    Ty::Arch => "e.g. an archetype id or \"self.next\"",
                    _ => "an expression string",
                }));
                None
            }
        }
    }

    fn expr_err(&self, v: &V, src: &str, e: ExprError) {
        self.drain_warnings(v, src);
        let mut d = if e.warning { v.warn(e.code, format!("{} (in \"{src}\")", e.message)) } else { v.err(e.code, format!("{} (in \"{src}\")", e.message)) };
        d.expected = e.expected;
        d.got = e.got.or_else(|| Some(src.to_owned()));
        d.suggestion = e.suggestion;
        d.column = Some(e.column);
        self.push(d);
    }

    fn drain_warnings(&self, v: &V, src: &str) {
        let ws: Vec<ExprError> = self.reg.warnings.borrow_mut().drain(..).collect();
        for e in ws {
            let mut d = v.warn(e.code, format!("{} (in \"{src}\")", e.message));
            d.suggestion = e.suggestion;
            d.column = Some(e.column);
            self.push(d);
        }
    }

    fn unknown_or_wrong(&self, v: &V, id: &str, want: &str) {
        if self.full() {
            // Past what a report shows: no id lists to build.
            self.push(v.err(codes::UNKNOWN_REF, format!("'{id}' is not {} {want}", article(want))));
            return;
        }
        match self.kinds.get(id) {
            Some(k) => self.push(
                v.err(codes::WRONG_KIND, format!("'{id}' is {} {k}, but {} {want} is expected here", article(k), article(want)))
                    .expected(format!("{} {want} id", article(want)))
                    .got(format!("{id} ({k})"))
                    .suggest_opt(self.dym(id, self.ids(want).iter().map(String::as_str)).or_else(|| Some(format!("{want}s: {}", self.ids(want).join(", "))))),
            ),
            None => {
                let ids = self.ids(want);
                self.push(
                    v.err(codes::UNKNOWN_REF, format!("there is no {want} '{id}'"))
                        .expected(format!("{} {want} id", article(want)))
                        .got(id)
                        .suggest_opt(self.dym(id, ids.iter().map(String::as_str)).or_else(|| Some(format!("add an item \"{id}\": {{\"kind\": \"{want}\", ...}}")))),
                )
            }
        }
    }

    /// An item reference of `kind`.
    fn ref_of(&self, v: &V, kind: &'static str) -> Option<u16> {
        let Some(id) = v.j.as_str() else {
            self.push(v.err(codes::WRONG_TYPE, format!("expected {} {kind} id (a string)", article(kind))).got(show(v.j)));
            return None;
        };
        match self.index_of(kind, id) {
            Some(i) => Some(i),
            None => {
                self.unknown_or_wrong(v, id, kind);
                None
            }
        }
    }

    fn tag_list(&self, v: &V, granted_only: bool) -> Vec<String> {
        let Some(list) = v.j.as_array() else {
            self.push(v.err(codes::WRONG_TYPE, "tags are an array of strings").got(show(v.j)).suggest("e.g. [\"enemy\", \"ground\"]"));
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, t) in list.iter().enumerate() {
            let tv = v.index(i, t);
            match t.as_str() {
                Some(s) => {
                    let bare = s.strip_prefix('!').unwrap_or(s);
                    let ok = !bare.is_empty() && bare.len() <= 64 && bare.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b));
                    if !ok || (granted_only && s.starts_with('!')) {
                        self.push(tv.err(codes::INVALID, format!("'{s}' is not a valid tag")).expected("1-64 of A-Z a-z 0-9 _ . : -").got(s));
                    } else {
                        out.push(s.to_owned());
                    }
                }
                None => self.push(tv.err(codes::WRONG_TYPE, "a tag is a string").got(show(t))),
            }
        }
        out
    }

    /// Effect and attack tag lists (bounded: tag checks scan them).
    fn tag_ids_capped(&self, v: &V) -> Vec<u16> {
        let ids = self.tag_ids(v);
        if ids.len() > MAX_FILTER_TAGS {
            self.push(v.err(codes::TOO_MANY, format!("{} tags; at most {MAX_FILTER_TAGS} here", ids.len())));
        }
        ids
    }

    fn tag_ids(&self, v: &V) -> Vec<u16> {
        let mut ids: Vec<u16> = self.tag_list(v, true).iter().filter_map(|t| self.reg.tags.get(t).copied()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn filter(&self, v: &V) -> TagFilter {
        let mut f = TagFilter::default();
        let list = match v.j {
            Json::Str(s) => vec![s.clone()],
            _ => self.tag_list(v, false),
        };
        if list.len() > MAX_FILTER_TAGS {
            self.push(v.err(codes::TOO_MANY, format!("{} tags in one filter; at most {MAX_FILTER_TAGS}", list.len())));
        }
        for t in list {
            let (neg, name) = match t.strip_prefix('!') {
                Some(n) => (true, n.to_owned()),
                None => (false, t.clone()),
            };
            let id = match self.reg.tags.get(&name) {
                Some(i) => *i,
                None => {
                    self.push(
                        v.warn(codes::UNKNOWN_REF, format!("no archetype or effect has the tag '{name}'"))
                            .suggest_opt(self.dym(&name, self.reg.tags.keys().map(String::as_str)).or_else(|| Some("so this filter never matches".into()))),
                    );
                    u16::MAX
                }
            };
            if neg {
                if id != u16::MAX {
                    f.none.push(id);
                }
            } else {
                f.all.push(id);
            }
        }
        f
    }

    // ---- pass 2 ----

    fn pass2(&mut self, opts: &Options) -> Rules {
        let doc = self.doc;
        for id in self.ids("native") {
            let n = self.native(&self.item(doc.items.get_key_value(&id).expect("listed").0));
            self.natives.push(n);
        }
        if let Some(bound) = &opts.natives {
            for n in &self.natives {
                if !bound.contains(&n.hook) {
                    self.push(
                        Diagnostic::warning(codes::NATIVE_UNBOUND, n.id.as_str(), "hook", format!("no module provides the native hook '{}' yet, so runs that use it will fail", n.hook))
                            .suggest(match self.dym(&n.hook, bound.iter().map(String::as_str)) {
                                Some(close) => format!("{close} (running hooks: {})", bound.iter().take(12).cloned().collect::<Vec<_>>().join(", ")),
                                None if bound.is_empty() => "no module with hooks is running: build and load one (logic_build), or replace the native with Lex building blocks".into(),
                                None => format!("running hooks: {}; or replace the native with Lex building blocks", bound.iter().take(12).cloned().collect::<Vec<_>>().join(", ")),
                            }),
                    );
                } else {
                    // Bound in the engine, but headless runs can't call module hooks.
                    self.push(
                        Diagnostic::warning(codes::NATIVE_HEADLESS, n.id.as_str(), "hook", format!("module hook '{}' is running, but lex_simulate runs headless and can't call module hooks, so simulating this set is refused", n.hook))
                            .suggest("test the hook with logic_hook; to simulate the set, replace the native with Lex building blocks (live Lex in the engine's tick will call module hooks)"),
                    );
                }
            }
        }
        for id in self.ids("machine") {
            let m = self.machine(&self.item(doc.items.get_key_value(&id).expect("listed").0));
            self.machines.push(m);
        }
        let mut archetypes = Vec::new();
        for (i, id) in self.ids("archetype").iter().enumerate() {
            archetypes.push(self.archetype(id, i as u16));
        }
        let mut effects = Vec::new();
        for id in self.ids("effect") {
            effects.push(self.effect(&self.item(doc.items.get_key_value(&id).expect("listed").0)));
        }
        // Several waves items join in id order into one schedule; the first sets start and rng.
        let wave_ids = self.ids("waves");
        let mut waves: Option<WavesDef> = None;
        for (k, id) in wave_ids.iter().enumerate() {
            let v = self.item(doc.items.get_key_value(id).expect("listed").0);
            if k > 0 {
                for key in ["start", "rng"] {
                    if let Some(x) = v.key(key) {
                        self.push(x.err(codes::INVALID, format!("only the first waves item ('{}') sets {key}", wave_ids[0])).suggest(format!("move \"{key}\" to '{}'", wave_ids[0])));
                    }
                }
            }
            let w = self.waves(&v, k + 1 == wave_ids.len());
            match &mut waves {
                None => waves = Some(w),
                Some(all) => all.waves.extend(w.waves),
            }
        }
        if let Some(w) = &waves {
            if w.waves.len() > MAX_WAVES {
                self.push(Diagnostic::error(codes::TOO_MANY, wave_ids[0].as_str(), "waves", format!("{} waves; at most {MAX_WAVES}", w.waves.len())));
            }
        }
        let mut timers = Vec::new();
        for id in self.ids("timer") {
            timers.push(self.timer(&self.item(doc.items.get_key_value(&id).expect("listed").0)));
        }
        let mut rules = Vec::new();
        for id in self.ids("rule") {
            if let Some(r) = self.rule(&self.item(doc.items.get_key_value(&id).expect("listed").0)) {
                rules.push(r);
            }
        }
        let mut commands = Vec::new();
        for id in self.ids("command") {
            commands.push(self.command(&self.item(doc.items.get_key_value(&id).expect("listed").0)));
        }
        let mut outcomes = Vec::new();
        for id in self.ids("outcome") {
            if let Some(o) = self.outcome(&self.item(doc.items.get_key_value(&id).expect("listed").0)) {
                outcomes.push(o);
            }
        }
        outcomes.sort_by(|a: &OutcomeDef, b: &OutcomeDef| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
        let mut h = StableHasher::with_seed(0x1e8_c0de);
        h.write_u64(doc.hash());
        let mut ov: Vec<(String, i64)> = self.params.iter().map(|p| (p.id.clone(), p.value)).collect();
        ov.sort();
        for (id, v) in ov {
            h.write_str(&id);
            h.write_i64(v);
        }
        if self.reg.streams.borrow().len() > MAX_NAMES {
            self.push(Diagnostic::error(codes::TOO_MANY, "", "", format!("more than {MAX_NAMES} random streams")));
        }
        let streams: BTreeMap<u16, String> = self.reg.streams.borrow().iter().map(|(n, i)| (*i, n.clone())).collect();
        let n_attrs = self.reg.attrs.len();
        for a in &mut archetypes {
            let mut slot = vec![u16::MAX; n_attrs];
            for (i, (attr, _)) in a.attrs.iter().enumerate() {
                slot[usize::from(*attr)] = i as u16;
            }
            a.slot = slot;
        }
        Rules {
            name: doc.name.clone(),
            tick_hz: self.hz,
            hash: h.finish(),
            attrs: self.reg.attrs.iter().map(|(n, (_, u))| AttrDef { name: n.clone(), unit: *u }).collect(),
            resources: std::mem::take(&mut self.resources),
            params: std::mem::take(&mut self.params),
            paths: std::mem::take(&mut self.paths),
            sites: std::mem::take(&mut self.sites),
            tags: self.reg.tags.keys().cloned().collect(),
            streams: streams.into_values().collect(),
            archetypes,
            effects,
            machines: std::mem::take(&mut self.machines),
            waves,
            timers,
            rules,
            commands,
            outcomes,
            natives: std::mem::take(&mut self.natives),
            events: std::mem::take(&mut self.events),
        }
    }

    fn native(&self, v: &V) -> NativeDef {
        self.fields(v, schema::kind("native").expect("kind"), &["kind"]);
        let hook = self.req(v, "hook").and_then(|h| self.string(&h)).unwrap_or_default();
        let reason = match v.key("reason") {
            Some(r) => self.string(&r).unwrap_or_default(),
            None => {
                self.push(v.err(codes::MISSING_FIELD, "a native hook needs a reason").suggest("add \"reason\": why no Lex building block fits (it becomes the feature backlog)"));
                String::new()
            }
        };
        let phase = match v.key("phase") {
            None => Phase::Any,
            Some(p) => match p.j.as_str() {
                Some("spawn") => Phase::Spawn,
                Some("move") => Phase::Move,
                Some("act") => Phase::Act,
                Some("effects") => Phase::Effects,
                Some("any") => Phase::Any,
                _ => {
                    self.push(p.err(codes::INVALID, format!("{} is not a phase", show(p.j))).expected("spawn, move, act, effects or any"));
                    Phase::Any
                }
            },
        };
        NativeDef { id: v.item.to_owned(), hook, reason, phase }
    }

    fn machine(&self, v: &V) -> MachineDef {
        self.fields(v, schema::kind("machine").expect("kind"), &["kind"]);
        let me = self.index_of("machine", v.item);
        let mut states = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let states_v = self.req(v, "states");
        if let Some(sv) = &states_v {
            match sv.j.as_object() {
                Some(pairs) => names = pairs.iter().map(|(k, _)| k.clone()).collect(),
                None => self.push(sv.err(codes::WRONG_TYPE, "states must be an object of state name → state").got(type_name(sv.j))),
            }
        }
        let initial = match self.req(v, "initial") {
            Some(iv) => match iv.j.as_str().and_then(|s| names.iter().position(|n| n == s)) {
                Some(i) => i as u16,
                None => {
                    self.push(iv.err(codes::UNKNOWN_REF, format!("{} is not one of the states", show(iv.j))).suggest_opt(iv.j.as_str().and_then(|s| self.dym(s, names.iter().map(String::as_str)))));
                    0
                }
            },
            None => 0,
        };
        let sc = Scope { me: Me::Any, other: true, ..Scope::new(&self.reg, "a machine") };
        let act = Act { scope: sc, machine: me };
        if let Some(sv) = &states_v {
            for (name, sj) in sv.j.as_object().unwrap_or_default() {
                let s = sv.sub(name, sj);
                let state_spec = Spec {
                    name: "state",
                    doc: "",
                    fields: &[
                        schema::Field { name: "behaviours", ty: "", required: false, doc: "" },
                        schema::Field { name: "on_enter", ty: "", required: false, doc: "" },
                        schema::Field { name: "on_exit", ty: "", required: false, doc: "" },
                        schema::Field { name: "transitions", ty: "", required: false, doc: "" },
                    ],
                };
                self.fields(&s, &state_spec, &[]);
                let behaviours = s.key("behaviours").map(|b| self.name_list(&b)).unwrap_or_default();
                let on_enter = s.key("on_enter").map(|a| self.actions(&a, act)).unwrap_or_default();
                let on_exit = s.key("on_exit").map(|a| self.actions(&a, act)).unwrap_or_default();
                let mut transitions = Vec::new();
                if let Some(tv) = s.key("transitions") {
                    for (i, t) in tv.j.as_array().unwrap_or_default().iter().enumerate() {
                        let t = tv.index(i, t);
                        let tspec = Spec {
                            name: "transition",
                            doc: "",
                            fields: &[
                                schema::Field { name: "to", ty: "", required: true, doc: "" },
                                schema::Field { name: "when", ty: "", required: false, doc: "" },
                                schema::Field { name: "after", ty: "", required: false, doc: "" },
                                schema::Field { name: "on", ty: "", required: false, doc: "" },
                            ],
                        };
                        if !self.fields(&t, &tspec, &[]) {
                            continue;
                        }
                        let to = match self.req(&t, "to") {
                            Some(x) => match x.j.as_str().and_then(|n| names.iter().position(|m| m == n)) {
                                Some(i) => i as u16,
                                None => {
                                    self.push(x.err(codes::UNKNOWN_REF, format!("{} is not a state of this machine", show(x.j))).suggest_opt(x.j.as_str().and_then(|n| self.dym(n, names.iter().map(String::as_str)))));
                                    continue;
                                }
                            },
                            None => continue,
                        };
                        let when = t.key("when").and_then(|w| self.bool_expr(&w, &sc));
                        let after = t.key("after").and_then(|a| self.num_expr(&a, Unit::Duration, &sc));
                        let on = t.key("on").and_then(|o| self.event_ref(&o));
                        if t.key("when").is_none() && t.key("after").is_none() && t.key("on").is_none() {
                            self.push(t.err(codes::MISSING_FIELD, "a transition needs when, after or on").suggest("e.g. {\"to\": \"idle\", \"after\": \"2s\"}"));
                        }
                        transitions.push(Transition { to, when, after, on });
                    }
                }
                states.push(StateDef { name: name.clone(), behaviours, on_enter, on_exit, transitions });
            }
        }
        if states.is_empty() && states_v.is_some() {
            self.push(v.err(codes::INVALID, "a machine needs at least one state"));
        }
        MachineDef { id: v.item.to_owned(), initial, states }
    }

    fn name_list(&self, v: &V) -> Vec<String> {
        match v.j.as_array() {
            Some(l) => l.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            None => {
                self.push(v.err(codes::WRONG_TYPE, "expected an array of names").got(show(v.j)));
                Vec::new()
            }
        }
    }

    fn event_ref(&self, v: &V) -> Option<u16> {
        let Some(name) = v.j.as_str() else {
            self.push(v.err(codes::WRONG_TYPE, "expected an event name").got(show(v.j)));
            return None;
        };
        match self.event_idx.get(name) {
            Some(i) => Some(*i),
            None if self.events_overflow => None,
            None => {
                self.push(
                    v.err(codes::UNKNOWN_REF, format!("no event '{name}' (built-in, or raised by some emit)"))
                        .suggest_opt(self.dym(name, self.events.iter().map(|e| e.name.as_str())).or_else(|| Some(format!("raise it somewhere with {{\"emit\": \"{name}\"}}")))),
                );
                None
            }
        }
    }

    fn archetype(&self, id: &str, a: u16) -> ArchDef {
        let item = self.doc.items.get_key_value(id).expect("listed").0.as_str();
        let empty = Json::Obj(Vec::new());
        let eff = self.eff.get(id).unwrap_or(&empty);
        let v = V { item, path: String::new(), j: eff };
        self.fields(&V { item, path: String::new(), j: &self.doc.items[id] }, schema::kind("archetype").expect("kind"), &["kind"]);
        let tags = v.key("tags").map(|t| self.tag_ids(&t)).unwrap_or_default();
        // `null` clears an inherited machine or next tier.
        let machine = v.key("machine").filter(|m| !m.j.is_null()).and_then(|m| self.ref_of(&m, "machine"));
        let next = v.key("next").filter(|n| !n.j.is_null()).and_then(|n| self.ref_of(&n, "archetype"));
        let abstract_ = v.key("abstract").and_then(|b| self.bool_field(&b)).unwrap_or(false);
        let sc = Scope { me: Me::Arch(a), ..Scope::new(&self.reg, "an archetype") };
        let act = Act { scope: sc, machine };
        let on_spawn = v.key("on_spawn").map(|x| self.actions(&x, act)).unwrap_or_default();
        let mut behaviours: Vec<Beh> = Vec::new();
        if let Some(bv) = v.key("behaviours") {
            match bv.j.as_array() {
                None => self.push(bv.err(codes::WRONG_TYPE, "behaviours must be an array").got(type_name(bv.j))),
                Some(list) => {
                    if list.len() > MAX_BEHAVIOURS {
                        self.push(bv.err(codes::TOO_MANY, format!("{} behaviours; at most {MAX_BEHAVIOURS} per archetype", list.len())));
                    }
                    for (i, b) in list.iter().enumerate() {
                        if let Some(beh) = self.behaviour(&bv.index(i, b), a, machine) {
                            if behaviours.iter().any(|x| x.name == beh.name) {
                                self.push(bv.index(i, b).err(codes::DUPLICATE, format!("two behaviours are named '{}'", beh.name)).suggest("give one a \"name\""));
                            }
                            behaviours.push(beh);
                        }
                    }
                }
            }
        }
        // Machine states gate behaviours by name.
        if let Some(m) = machine.and_then(|m| self.machines.get(usize::from(m))) {
            for s in &m.states {
                for n in &s.behaviours {
                    if !behaviours.iter().any(|b| &b.name == n) {
                        self.push(
                            v.key("machine")
                                .unwrap_or(v.clone())
                                .err(codes::UNKNOWN_REF, format!("machine '{}' state '{}' names behaviour '{n}', which this archetype doesn't have", m.id, s.name))
                                .suggest_opt(self.dym(n, behaviours.iter().map(|b| b.name.as_str()))),
                        );
                    }
                }
            }
            for b in &mut behaviours {
                let states: Vec<u16> = m.states.iter().enumerate().filter(|(_, s)| s.behaviours.contains(&b.name)).map(|(i, _)| i as u16).collect();
                let named_anywhere = m.states.iter().any(|s| s.behaviours.contains(&b.name));
                b.active_in = named_anywhere.then_some(states);
            }
        }
        let health = behaviours.iter().find_map(|b| match b.kind {
            BehKind::Health { attr, .. } => Some(attr),
            _ => None,
        });
        if behaviours.iter().filter(|b| matches!(b.kind, BehKind::Health { .. })).count() > 1 {
            self.push(v.err(codes::DUPLICATE, "an archetype has at most one health behaviour"));
        }
        ArchDef {
            id: id.to_owned(),
            tags,
            attrs: self.arch_attrs.get(usize::from(a)).cloned().unwrap_or_default(),
            slot: Vec::new(),
            behaviours,
            machine,
            next,
            abstract_,
            on_spawn,
            health,
        }
    }

    fn own_attr(&self, v: &V, a: u16) -> Option<u16> {
        let Some(name) = v.j.as_str() else {
            self.push(v.err(codes::WRONG_TYPE, "expected an attribute name").got(show(v.j)));
            return None;
        };
        let has: Vec<&str> = self.reg.arch_attrs[usize::from(a)].iter().map(|i| self.reg.attr_name(*i)).collect();
        match self.reg.attrs.get(name) {
            Some((id, _)) if self.reg.arch_attrs[usize::from(a)].contains(id) => Some(*id),
            _ => {
                self.push(v.err(codes::UNKNOWN_REF, format!("this archetype has no attribute '{name}'")).suggest_opt(self.dym(name, has).or_else(|| Some(format!("add \"{name}\" to attributes")))));
                None
            }
        }
    }

    fn any_attr(&self, v: &V) -> Option<(u16, Unit)> {
        let Some(name) = v.j.as_str() else {
            self.push(v.err(codes::WRONG_TYPE, "expected an attribute name").got(show(v.j)));
            return None;
        };
        self.attr_named(v, name)
    }

    fn attr_named(&self, v: &V, name: &str) -> Option<(u16, Unit)> {
        match self.reg.attrs.get(name) {
            Some(x) => Some(*x),
            None => {
                self.push(v.err(codes::UNKNOWN_REF, format!("no archetype has an attribute '{name}'")).suggest_opt(self.dym(name, self.reg.attrs.keys().map(String::as_str))));
                None
            }
        }
    }

    fn behaviour(&self, v: &V, a: u16, machine: Option<u16>) -> Option<Beh> {
        let types: Vec<&str> = schema::BEHAVIOURS.iter().map(|b| b.name).collect();
        let t = self.req(v, "type")?;
        let Some(spec) = t.j.as_str().and_then(schema::behaviour) else {
            self.push(t.err(codes::UNKNOWN_REF, format!("{} is not a behaviour type", show(t.j))).expected(types.join(", ")).suggest_opt(t.j.as_str().and_then(|s| self.dym(s, types.iter().copied()))));
            return None;
        };
        if !self.fields(v, spec, &["type", "name"]) {
            return None;
        }
        let name = v.key("name").and_then(|n| self.string(&n)).unwrap_or_else(|| spec.name.to_owned());
        let me = Scope { me: Me::Arch(a), ..Scope::new(&self.reg, "a behaviour") };
        let act = Act { scope: me, machine };
        let num = |key: &str, unit: Unit| self.req(v, key).and_then(|x| self.num_expr(&x, unit, &me));
        let opt_num = |key: &str, unit: Unit| v.key(key).and_then(|x| self.num_expr(&x, unit, &me));
        let acts = |key: &str, act: Act| v.key(key).map(|x| self.actions(&x, act)).unwrap_or_default();
        let kind = match spec.name {
            "health" => BehKind::Health {
                attr: self.req(v, "attribute").and_then(|x| self.own_attr(&x, a))?,
                on_death: acts("on_death", Act { scope: Scope { other: true, ..me }, machine }),
            },
            "follow_path" => {
                let pv = self.req(v, "path")?;
                let path = if pv.j.as_str() == Some("@spawn") { PathSel::Spawn } else { PathSel::Fixed(self.ref_of(&pv, "path")?) };
                BehKind::FollowPath { path, speed: num("speed", Unit::Speed)?, scale: opt_num("scale", Unit::Ratio), on_arrive: acts("on_arrive", act) }
            }
            "patrol" => {
                let path = self.req(v, "path").and_then(|p| self.ref_of(&p, "path"))?;
                let pingpong = match v.key("mode") {
                    None => false,
                    Some(m) => match m.j.as_str() {
                        Some("loop") => false,
                        Some("pingpong") => true,
                        _ => {
                            self.push(m.err(codes::INVALID, format!("{} is not a patrol mode", show(m.j))).expected("loop or pingpong"));
                            false
                        }
                    },
                };
                BehKind::Patrol { path, speed: num("speed", Unit::Speed)?, scale: opt_num("scale", Unit::Ratio), pingpong }
            }
            "chase" => BehKind::Chase {
                targets: self.req(v, "targets").map(|t| self.filter(&t)).unwrap_or_default(),
                range: num("range", Unit::Distance)?,
                speed: num("speed", Unit::Speed)?,
                scale: opt_num("scale", Unit::Ratio),
                reach: opt_num("reach", Unit::Distance).unwrap_or(Ex::Int(500)),
                on_reach: acts("on_reach", Act { scope: Scope { target: true, ..me }, machine }),
            },
            "attack" => BehKind::Attack(Box::new(self.attack(v, a)?)),
            "projectile" => BehKind::Projectile {
                speed: num("speed", Unit::Speed)?,
                homing: v.key("homing").and_then(|h| self.bool_field(&h)).unwrap_or(true),
                splash: opt_num("splash", Unit::Distance).unwrap_or(Ex::Int(0)),
            },
            "spawner" => {
                let arch = self.req(v, "archetype").and_then(|x| self.spawnable(&x))?;
                let every = num("every", Unit::Duration)?;
                BehKind::Spawner { arch, count: opt_num("count", Unit::Int).unwrap_or(Ex::Int(1)), start: opt_num("start", Unit::Duration).unwrap_or_else(|| every.clone()), every }
            }
            "lifetime" => BehKind::Lifetime { duration: num("duration", Unit::Duration)?, on_expire: acts("on_expire", act) },
            "native" => {
                let hv = self.req(v, "hook")?;
                let hook = self.ref_of(&hv, "native")?;
                if self.natives.get(usize::from(hook)).is_some_and(|n| n.phase == Phase::Any) {
                    self.push(hv.err(codes::INVALID, "a native used as a behaviour must declare its phase").suggest("add \"phase\": \"move\" (or spawn, act, effects) to the native item"));
                }
                BehKind::Native { hook, args: v.key("args").map(|x| self.native_args(&x, &me)).unwrap_or_default() }
            }
            _ => unreachable!("every behaviour spec is handled"),
        };
        Some(Beh { name, kind, active_in: None })
    }

    fn spawnable(&self, v: &V) -> Option<u16> {
        let a = self.ref_of(v, "archetype")?;
        let id = v.j.as_str().unwrap_or("");
        if self.eff.get(id).and_then(|e| e.get("abstract")).and_then(Json::as_bool) == Some(true) {
            self.push(v.err(codes::INVALID, format!("'{id}' is abstract, so it can't be spawned")).suggest("spawn an archetype that extends it"));
        }
        Some(a)
    }

    fn native_args(&self, v: &V, sc: &Scope) -> Vec<(String, Ex)> {
        let mut out = Vec::new();
        for (k, x) in v.j.as_object().unwrap_or_default() {
            let xv = v.sub(k, x);
            let ex = match x {
                Json::Num(_) => self.num_expr(&xv, Unit::Int, sc),
                Json::Str(src) => match expr::compile(src, sc) {
                    Ok((e, Ty::Num(_))) => Some(e),
                    Ok((_, t)) => {
                        self.push(xv.err(codes::WRONG_TYPE, format!("native args are numbers, not {}", t.describe())));
                        None
                    }
                    Err(e) => {
                        self.expr_err(&xv, src, e);
                        None
                    }
                },
                other => {
                    self.push(xv.err(codes::WRONG_TYPE, "native args are numbers or expressions").got(show(other)));
                    None
                }
            };
            if let Some(e) = ex {
                out.push((k.clone(), e));
            }
        }
        out
    }

    fn attack(&self, v: &V, a: u16) -> Option<AttackDef> {
        let me = Scope { me: Me::Arch(a), ..Scope::new(&self.reg, "an attack") };
        let hit = Scope { target: true, ..me };
        let targets = self.req(v, "targets").map(|t| self.filter(&t)).unwrap_or_default();
        let range = self.req(v, "range").and_then(|x| self.num_expr(&x, Unit::Distance, &me));
        let cooldown = self.req(v, "cooldown").and_then(|x| self.num_expr(&x, Unit::Duration, &me));
        let policy = match v.key("policy") {
            None => Policy::First,
            Some(p) => match p.j.as_str() {
                Some("first") => Policy::First,
                Some("last") => Policy::Last,
                Some("closest") => Policy::Closest,
                Some("strongest") => Policy::Strongest,
                Some("weakest") => Policy::Weakest,
                _ => {
                    let names = ["first", "last", "closest", "strongest", "weakest"];
                    self.push(p.err(codes::INVALID, format!("{} is not a targeting policy", show(p.j))).expected(names.join(", ")).suggest_opt(p.j.as_str().and_then(|s| self.dym(s, names))));
                    Policy::First
                }
            },
        };
        let projectile = match v.key("projectile") {
            None => None,
            Some(pv) => {
                let spec = Spec {
                    name: "projectile",
                    doc: "",
                    fields: &[schema::Field { name: "archetype", ty: "", required: true, doc: "" }, schema::Field { name: "with", ty: "", required: false, doc: "" }],
                };
                self.fields(&pv, &spec, &[]);
                let arch = self.req(&pv, "archetype").and_then(|x| self.spawnable(&x));
                if let (Some(pa), Some(x)) = (arch, pv.key("archetype")) {
                    let has = self.eff.get(x.j.as_str().unwrap_or("")).and_then(|e| e.get("behaviours")).and_then(Json::as_array).is_some_and(|bs| {
                        bs.iter().any(|b| b.get("type").and_then(Json::as_str) == Some("projectile"))
                    });
                    if !has {
                        self.push(x.err(codes::INVALID, format!("archetype '{}' has no projectile behaviour", self.ids("archetype")[usize::from(pa)])).suggest("add {\"type\": \"projectile\", \"speed\": ...} to its behaviours"));
                    }
                }
                let with = pv.key("with").map(|w| self.with_attrs(&w, &hit)).unwrap_or_default();
                arch.map(|pa| (pa, with))
            }
        };
        let delivery = match v.key("delivery") {
            None => {
                if projectile.is_some() {
                    Delivery::Projectile
                } else {
                    Delivery::Instant
                }
            }
            Some(d) => match d.j.as_str() {
                Some("instant") => Delivery::Instant,
                Some("pulse") => Delivery::Pulse,
                Some("projectile") => {
                    if v.key("projectile").is_none() {
                        self.push(d.err(codes::MISSING_FIELD, "projectile delivery needs a projectile").suggest("add \"projectile\": {\"archetype\": \"<a projectile archetype>\"}"));
                    }
                    Delivery::Projectile
                }
                _ => {
                    self.push(d.err(codes::INVALID, format!("{} is not a delivery", show(d.j))).expected("instant, pulse or projectile"));
                    Delivery::Instant
                }
            },
        };
        let muzzle = v.key("muzzle").and_then(|m| self.point(&m)).unwrap_or([0; 3]);
        let blocked_by = v.key("blocked_by").map(|b| self.tag_ids_capped(&b)).unwrap_or_default();
        let mut payload = Vec::new();
        if let Some(pv) = self.req(v, "payload") {
            if pv.j.as_array().is_some_and(|l| l.len() > MAX_PAYLOAD) {
                self.push(pv.err(codes::TOO_MANY, format!("at most {MAX_PAYLOAD} payload entries")));
            }
            match pv.j.as_array() {
                None => self.push(pv.err(codes::WRONG_TYPE, "payload must be an array").suggest("e.g. [{\"damage\": \"10pt\"}]")),
                Some(list) => {
                    for (i, p) in list.iter().enumerate() {
                        if let Some(x) = self.payload(&pv.index(i, p), &hit) {
                            payload.push(x);
                        }
                    }
                }
            }
        }
        Some(AttackDef { targets, range: range?, cooldown: cooldown?, policy, delivery, projectile, muzzle, blocked_by, payload })
    }

    fn with_attrs(&self, v: &V, sc: &Scope) -> Vec<(u16, Ex)> {
        let mut out = Vec::new();
        let Some(pairs) = v.j.as_object() else {
            self.push(v.err(codes::WRONG_TYPE, "with must be an object of attribute → value").got(type_name(v.j)));
            return out;
        };
        for (k, x) in pairs {
            let kv = v.sub(k, x);
            let Some((attr, unit)) = self.attr_named(&kv, k) else { continue };
            if let Some(e) = self.num_expr(&kv, unit, sc) {
                out.push((attr, e));
            }
        }
        out
    }

    fn payload(&self, v: &V, sc: &Scope) -> Option<Payload> {
        let verbs: Vec<&str> = schema::PAYLOADS.iter().map(|p| p.name).collect();
        let present: Vec<&str> = verbs.iter().copied().filter(|k| v.j.get(k).is_some()).collect();
        if present.len() != 1 {
            self.push(v.err(codes::INVALID, "a payload is {\"damage\": ...} or {\"effect\": ...}").got(show(v.j)));
            return None;
        }
        let spec = schema::payload(present[0]).expect("listed");
        self.fields(v, spec, &[]);
        match present[0] {
            "damage" => Some(Payload::Damage(self.num_expr(&v.key("damage")?, Unit::Points, sc)?)),
            _ => {
                let effect = self.ref_of(&v.key("effect")?, "effect")?;
                let munit = self.effect_magnitude_unit(effect);
                let magnitude = v.key("magnitude").and_then(|m| self.num_expr(&m, munit, sc));
                let duration = v.key("duration").and_then(|d| self.num_expr(&d, Unit::Duration, sc));
                Some(Payload::Effect { effect, magnitude, duration })
            }
        }
    }

    fn effect_magnitude_unit(&self, effect: u16) -> Unit {
        let id = &self.ids("effect")[usize::from(effect)];
        self.doc.items[id].get("magnitude").and_then(Json::as_str).and_then(Unit::from_name).unwrap_or(Unit::Ratio)
    }

    fn effect(&self, v: &V) -> EffectDef {
        self.fields(v, schema::kind("effect").expect("kind"), &["kind"]);
        let munit = match v.key("magnitude") {
            Some(m) => self.unit_name(&m).unwrap_or(Unit::Ratio),
            None => Unit::Ratio,
        };
        let sc = Scope { me: Me::Any, other: true, magnitude: Some(munit), ..Scope::new(&self.reg, "an effect") };
        let act = Act { scope: sc, machine: None };
        let duration = match v.key("duration") {
            None => EffDur::Instant,
            Some(d) => match d.j.as_str() {
                Some("instant") => EffDur::Instant,
                Some("infinite") => EffDur::Infinite,
                _ => self.num_expr(&d, Unit::Duration, &sc).map(EffDur::Ticks).unwrap_or(EffDur::Instant),
            },
        };
        let period = v.key("period").and_then(|p| self.num_expr(&p, Unit::Duration, &sc));
        if period.is_some() && matches!(duration, EffDur::Instant) {
            self.push(v.key("period").unwrap_or(v.clone()).err(codes::INVALID, "an instant effect can't have a period").suggest("give it a duration (or \"infinite\")"));
        }
        let mut modifiers = Vec::new();
        if let Some(mv) = v.key("modifiers") {
            if mv.j.as_array().is_some_and(|l| l.len() > MAX_MODIFIERS) {
                self.push(mv.err(codes::TOO_MANY, format!("at most {MAX_MODIFIERS} modifiers per effect")));
            }
            for (i, m) in mv.j.as_array().unwrap_or_default().iter().enumerate() {
                let m = mv.index(i, m);
                let spec = Spec {
                    name: "modifier",
                    doc: "",
                    fields: &[
                        schema::Field { name: "attribute", ty: "", required: true, doc: "" },
                        schema::Field { name: "op", ty: "", required: true, doc: "" },
                        schema::Field { name: "value", ty: "", required: true, doc: "" },
                    ],
                };
                if !self.fields(&m, &spec, &[]) {
                    continue;
                }
                let Some((attr, unit)) = self.req(&m, "attribute").and_then(|a| self.any_attr(&a)) else { continue };
                let op = match self.req(&m, "op").map(|o| (o.j.as_str().map(str::to_owned), o)) {
                    Some((Some(s), _)) if s == "add" => ModOp::Add,
                    Some((Some(s), _)) if s == "mul" => ModOp::Mul,
                    Some((Some(s), _)) if s == "set" => ModOp::Set,
                    Some((_, o)) => {
                        self.push(o.err(codes::INVALID, format!("{} is not a modifier op", show(o.j))).expected("add, mul or set"));
                        continue;
                    }
                    None => continue,
                };
                let vunit = if op == ModOp::Mul { Unit::Ratio } else { unit };
                let Some(value) = self.req(&m, "value").and_then(|x| self.num_expr(&x, vunit, &sc)) else { continue };
                modifiers.push(Modifier { attr, op, value });
            }
        }
        let stacking = match v.key("stacking") {
            None => Stacking::Strongest,
            Some(s) => match s.j.as_str() {
                Some("strongest") => Stacking::Strongest,
                Some("refresh") => Stacking::Refresh,
                Some("stack") => Stacking::Stack,
                Some("independent") => Stacking::Independent,
                Some("ignore") => Stacking::Ignore,
                _ => {
                    let names = ["strongest", "refresh", "stack", "independent", "ignore"];
                    self.push(s.err(codes::INVALID, format!("{} is not a stacking policy", show(s.j))).expected(names.join(", ")).suggest_opt(s.j.as_str().and_then(|x| self.dym(x, names))));
                    Stacking::Strongest
                }
            },
        };
        let max_stacks = v.key("max_stacks").and_then(|m| self.const_num(&m, Unit::Int)).unwrap_or(1);
        if !(1..=crate::sim::MAX_EFFECTS_PER_ENTITY as i64).contains(&max_stacks) {
            self.push(v.key("max_stacks").unwrap_or(v.clone()).err(codes::OUT_OF_RANGE, "max_stacks must be from 1 to 8 (an entity carries at most 8 effect instances)"));
        }
        EffectDef {
            id: v.item.to_owned(),
            duration,
            period,
            magnitude_unit: munit,
            modifiers,
            stacking,
            max_stacks: max_stacks.clamp(1, crate::sim::MAX_EFFECTS_PER_ENTITY as i64) as u16,
            grant_tags: v.key("grant_tags").map(|t| self.tag_ids_capped(&t)).unwrap_or_default(),
            block_tags: v.key("block_tags").map(|t| self.tag_ids_capped(&t)).unwrap_or_default(),
            on_apply: v.key("on_apply").map(|a| self.actions(&a, act)).unwrap_or_default(),
            on_period: v.key("on_period").map(|a| self.actions(&a, act)).unwrap_or_default(),
            on_expire: v.key("on_expire").map(|a| self.actions(&a, act)).unwrap_or_default(),
        }
    }

    /// One waves item (`last`: the schedule's last item, whose last wave may run until cleared).
    fn waves(&self, v: &V, last: bool) -> WavesDef {
        self.fields(v, schema::kind("waves").expect("kind"), &["kind"]);
        let sc = Scope::new(&self.reg, "a wave");
        let act = Act { scope: sc, machine: None };
        let ticks = |x: Option<V>| -> Option<u32> {
            let x = x?;
            let t = self.const_num(&x, Unit::Duration)?;
            if !(0..=i64::from(u32::MAX / 2)).contains(&t) {
                self.push(x.err(codes::OUT_OF_RANGE, "a duration here must be from 0 to about 2 years"));
                return None;
            }
            Some(t as u32)
        };
        let start = ticks(v.key("start")).unwrap_or(0);
        let stream = match v.key("rng") {
            None => self.reg.stream("waves"),
            Some(r) => self.string(&r).map(|s| self.reg.stream(&s)).unwrap_or(0),
        };
        let mut waves = Vec::new();
        if let Some(wv) = self.req(v, "waves") {
            let list = wv.j.as_array().unwrap_or_default();
            if list.is_empty() {
                self.push(wv.err(codes::INVALID, "waves needs at least one wave"));
            }
            for (i, w) in list.iter().enumerate() {
                let w = wv.index(i, w);
                let spec = Spec {
                    name: "wave",
                    doc: "",
                    fields: &[
                        schema::Field { name: "duration", ty: "", required: false, doc: "" },
                        schema::Field { name: "on_start", ty: "", required: false, doc: "" },
                        schema::Field { name: "scale", ty: "", required: false, doc: "" },
                        schema::Field { name: "groups", ty: "", required: true, doc: "" },
                    ],
                };
                if !self.fields(&w, &spec, &[]) {
                    continue;
                }
                let duration = ticks(w.key("duration"));
                if duration.is_none() && w.key("duration").is_none() && (i + 1 < list.len() || !last) {
                    self.push(w.err(codes::MISSING_FIELD, "every wave but the last needs a duration").suggest("add \"duration\": \"60s\""));
                }
                let on_start = w.key("on_start").map(|a| self.actions(&a, act)).unwrap_or_default();
                let mut scale_list = Vec::new();
                if let Some(s) = w.key("scale") {
                    for (k, x) in s.j.as_object().unwrap_or_default() {
                        let kv = s.sub(k, x);
                        if let (Some((attr, _)), Some(r)) = (self.attr_named(&kv, k), self.const_num(&kv, Unit::Ratio)) {
                            scale_list.push((attr, r));
                        }
                    }
                }
                let mut groups = Vec::new();
                if let Some(gv) = self.req(&w, "groups") {
                    for (j, g) in gv.j.as_array().unwrap_or_default().iter().enumerate() {
                        let g = gv.index(j, g);
                        let gspec = Spec {
                            name: "group",
                            doc: "",
                            fields: &[
                                schema::Field { name: "archetype", ty: "", required: true, doc: "" },
                                schema::Field { name: "count", ty: "", required: true, doc: "" },
                                schema::Field { name: "at", ty: "", required: true, doc: "" },
                                schema::Field { name: "start", ty: "", required: false, doc: "" },
                                schema::Field { name: "every", ty: "", required: false, doc: "" },
                            ],
                        };
                        if !self.fields(&g, &gspec, &[]) {
                            continue;
                        }
                        let arch = self.req(&g, "archetype").and_then(|x| self.spawnable(&x));
                        let count = self.req(&g, "count").and_then(|x| self.const_num(&x, Unit::Int));
                        if let Some(c) = count {
                            if !(0..=MAX_GROUP_COUNT).contains(&c) {
                                self.push(g.key("count").unwrap_or(g.clone()).err(codes::OUT_OF_RANGE, format!("count must be from 0 to {MAX_GROUP_COUNT}")).got(c.to_string()));
                            }
                        }
                        let mut at = Vec::new();
                        if let Some(av) = self.req(&g, "at") {
                            let names: Vec<(usize, &Json)> = match av.j {
                                Json::Str(_) => vec![(0, av.j)],
                                Json::Arr(l) => l.iter().enumerate().collect(),
                                _ => Vec::new(),
                            };
                            if names.is_empty() {
                                self.push(av.err(codes::INVALID, "at needs at least one path or site").suggest("e.g. [\"lane_mid\"]"));
                            }
                            for (k, n) in names {
                                let nv = if av.j.as_array().is_some() { av.index(k, n) } else { av.clone() };
                                match n.as_str() {
                                    Some(s) if self.index_of("path", s).is_some() => at.push(SpawnAt::Path(self.index_of("path", s).expect("checked"))),
                                    Some(s) if self.reg.sites.contains_key(s) => at.push(SpawnAt::Site(self.reg.sites[s])),
                                    Some(s) => {
                                        let mut all: Vec<String> = self.ids("path");
                                        all.extend(self.reg.sites.keys().cloned());
                                        self.push(nv.err(codes::UNKNOWN_REF, format!("'{s}' is neither a path nor a site")).suggest_opt(self.dym(s, all.iter().map(String::as_str))));
                                    }
                                    None => self.push(nv.err(codes::WRONG_TYPE, "a spawn point is a path id or a site name").got(show(n))),
                                }
                            }
                        }
                        let gstart = ticks(g.key("start")).unwrap_or(0);
                        let every = ticks(g.key("every")).unwrap_or(0);
                        if let (Some(arch), Some(count)) = (arch, count) {
                            if !at.is_empty() {
                                groups.push(GroupDef { arch, count: count.clamp(0, MAX_GROUP_COUNT) as u32, at, start: gstart, every });
                            }
                        }
                    }
                }
                let total: i64 = groups.iter().map(|g| i64::from(g.count)).sum();
                if total > MAX_WAVE_SPAWNS {
                    self.push(
                        w.err(codes::TOO_MANY, format!("wave {} spawns {total} entities; at most {MAX_WAVE_SPAWNS} per wave", i + 1))
                            .suggest("split it into more waves, or lower the groups' counts"),
                    );
                    groups.clear();
                }
                waves.push(WaveDef { duration, on_start, scale: scale_list, groups });
            }
        }
        WavesDef { start, stream, waves }
    }

    fn timer(&self, v: &V) -> TimerDef {
        self.fields(v, schema::kind("timer").expect("kind"), &["kind"]);
        let sc = Scope::new(&self.reg, "a timer");
        let t = |k: &str| v.key(k).and_then(|x| self.const_num(&x, Unit::Duration)).map(|x| x.clamp(0, i64::from(u32::MAX / 2)) as u32);
        let every = t("every");
        if every == Some(0) {
            self.push(v.key("every").unwrap_or(v.clone()).err(codes::OUT_OF_RANGE, "every must be at least 1 tick").suggest("e.g. \"1s\""));
        }
        TimerDef {
            id: v.item.to_owned(),
            start: t("start").unwrap_or(0),
            every: every.filter(|e| *e > 0),
            count: v.key("count").and_then(|x| self.const_num(&x, Unit::Int)).map(|c| c.clamp(0, i64::from(u32::MAX)) as u32),
            when: v.key("when").and_then(|w| self.bool_expr(&w, &sc)),
            actions: self.req(v, "do").map(|a| self.actions(&a, Act { scope: sc, machine: None })).unwrap_or_default(),
        }
    }

    fn rule(&self, v: &V) -> Option<RuleDef> {
        self.fields(v, schema::kind("rule").expect("kind"), &["kind"]);
        let on = self.req(v, "on").and_then(|o| self.event_ref(&o));
        let unit = on.map_or(Unit::Int, |e| self.events[usize::from(e)].unit);
        let sc = Scope { me: Me::Any, other: true, event: Some(unit), ..Scope::new(&self.reg, "a rule") };
        let tags = v.key("tags").map(|t| self.filter(&t)).unwrap_or_default();
        let when = v.key("when").and_then(|w| self.bool_expr(&w, &sc));
        let actions = self.req(v, "do").map(|a| self.actions(&a, Act { scope: sc, machine: None })).unwrap_or_default();
        Some(RuleDef { id: v.item.to_owned(), on: on?, tags, when, actions })
    }

    fn command(&self, v: &V) -> CommandDef {
        self.fields(v, schema::kind("command").expect("kind"), &["kind"]);
        let mut args = Vec::new();
        if let Some(av) = v.key("args") {
            match av.j.as_object() {
                None => self.push(av.err(codes::WRONG_TYPE, "args must be an object of name → type").got(type_name(av.j)).suggest("e.g. {\"site\": \"site\", \"stone\": {\"type\": \"archetype\", \"tags\": [\"stone\"]}}")),
                Some(pairs) => {
                    for (name, t) in pairs {
                        let tv = av.sub(name, t);
                        let (tname, tags) = match t {
                            Json::Str(s) => (Some(s.as_str()), None),
                            Json::Obj(_) => (t.get("type").and_then(Json::as_str), tv.key("tags")),
                            _ => (None, None),
                        };
                        let filter = tags.map(|tg| self.filter(&tg)).unwrap_or_default();
                        let ty = match tname {
                            Some("site") => ArgTy::Site,
                            Some("archetype") => ArgTy::Archetype(filter),
                            Some("entity") => ArgTy::Entity(filter),
                            Some(u) if Unit::from_name(u).is_some() => ArgTy::Num(Unit::from_name(u).expect("checked")),
                            _ => {
                                let mut names = vec!["site", "archetype", "entity"];
                                names.extend(Unit::ALL.iter().map(|u| u.name()));
                                self.push(tv.err(codes::INVALID, format!("{} is not an argument type", show(t))).expected(names.join(", ")).suggest_opt(tname.and_then(|s| self.dym(s, names.iter().copied()))));
                                continue;
                            }
                        };
                        args.push(ArgDef { name: name.clone(), ty });
                    }
                }
            }
        }
        let arg_tys: Vec<(String, Ty)> = args
            .iter()
            .map(|a| {
                (
                    a.name.clone(),
                    match a.ty {
                        ArgTy::Site => Ty::Site,
                        ArgTy::Archetype(_) => Ty::Arch,
                        ArgTy::Entity(_) => Ty::Ent,
                        ArgTy::Num(u) => Ty::Num(u),
                    },
                )
            })
            .collect();
        let sc = Scope { args: &arg_tys, ..Scope::new(&self.reg, "a command") };
        let mut cost = Vec::new();
        if let Some(cv) = v.key("cost") {
            for (res, x) in cv.j.as_object().unwrap_or_default() {
                let xv = cv.sub(res, x);
                match self.reg.resources.get(res) {
                    Some((r, unit)) => {
                        if let Some(e) = self.num_expr(&xv, *unit, &sc) {
                            cost.push((*r, e));
                        }
                    }
                    None => self.push(xv.err(codes::UNKNOWN_REF, format!("there is no resource '{res}'")).suggest_opt(self.dym(res, self.reg.resources.keys().map(String::as_str)))),
                }
            }
        }
        let mut require = Vec::new();
        if let Some(rv) = v.key("require") {
            for (i, r) in rv.j.as_array().unwrap_or_default().iter().enumerate() {
                let r = rv.index(i, r);
                let spec = Spec {
                    name: "requirement",
                    doc: "",
                    fields: &[
                        schema::Field { name: "when", ty: "", required: true, doc: "" },
                        schema::Field { name: "code", ty: "", required: true, doc: "" },
                        schema::Field { name: "message", ty: "", required: false, doc: "" },
                    ],
                };
                if !self.fields(&r, &spec, &[]) {
                    continue;
                }
                let cond = self.req(&r, "when").and_then(|w| self.bool_expr(&w, &sc));
                let code = self.req(&r, "code").and_then(|c| self.string(&c));
                let message = r.key("message").and_then(|m| self.string(&m));
                if let (Some(cond), Some(code)) = (cond, code) {
                    let message = message.unwrap_or_else(|| format!("refused: {code}"));
                    require.push(Require { cond, code, message });
                }
            }
        }
        let actions = self.req(v, "do").map(|a| self.actions(&a, Act { scope: sc, machine: None })).unwrap_or_default();
        CommandDef { id: v.item.to_owned(), args, cost, require, actions }
    }

    fn outcome(&self, v: &V) -> Option<OutcomeDef> {
        self.fields(v, schema::kind("outcome").expect("kind"), &["kind"]);
        let sc = Scope::new(&self.reg, "an outcome");
        let result = self.req(v, "result").and_then(|r| self.string(&r));
        let when = self.req(v, "when").and_then(|w| self.bool_expr(&w, &sc));
        let priority = v.key("priority").and_then(|p| self.const_num(&p, Unit::Int)).unwrap_or(0);
        let when = when?;
        let cheap = !expr::scans_entities(&when);
        Some(OutcomeDef { id: v.item.to_owned(), result: result?, when, priority, cheap })
    }

    // ---- actions ----

    fn actions(&self, v: &V, act: Act) -> Vec<Action> {
        let Some(list) = v.j.as_array() else {
            self.push(v.err(codes::WRONG_TYPE, "actions are an array").got(type_name(v.j)).suggest("wrap the action in [ ]"));
            return Vec::new();
        };
        list.iter().enumerate().filter_map(|(i, a)| self.action(&v.index(i, a), act)).collect()
    }

    /// An entity expression, or `self` when there is one.
    fn who(&self, v: Option<V>, at: &V, act: Act) -> Option<Ex> {
        match v {
            Some(w) => self.typed_expr(&w, &act.scope, Ty::Ent),
            None if act.scope.me != Me::None => Some(Ex::Who(expr::Who::Me)),
            None => {
                self.push(at.err(codes::MISSING_FIELD, "\"who\" is needed here (there is no self)").suggest("add \"who\": an entity expression"));
                None
            }
        }
    }

    fn action(&self, v: &V, act: Act) -> Option<Action> {
        let verbs: Vec<&str> = schema::ACTIONS.iter().map(|a| a.name).collect();
        if !matches!(v.j, Json::Obj(_)) {
            self.push(v.err(codes::WRONG_TYPE, format!("an action is an object, not {}", type_name(v.j))).suggest("e.g. {\"add\": \"gold\", \"amount\": 5}"));
            return None;
        }
        let present: Vec<&str> = verbs.iter().copied().filter(|k| v.j.get(k).is_some()).collect();
        let verb = match present.as_slice() {
            [one] => *one,
            [] => {
                let keys: Vec<&str> = v.j.as_object().unwrap_or_default().iter().map(|(k, _)| k.as_str()).collect();
                let hint = keys.first().and_then(|k| self.dym(k, verbs.iter().copied()));
                self.push(v.err(codes::INVALID, "an action needs one verb").expected(verbs.join(", ")).got(show(v.j)).suggest_opt(hint));
                return None;
            }
            many => {
                self.push(v.err(codes::INVALID, format!("an action has exactly one verb; this has {}", many.join(" and "))).suggest("split it into two actions"));
                return None;
            }
        };
        let spec = schema::action(verb).expect("listed");
        self.fields(v, spec, &[]);
        let sc = &act.scope;
        let key = |k: &str| v.key(k);
        let res = |k: &str| -> Option<(u16, Unit)> {
            let rv = key(k)?;
            let Some(name) = rv.j.as_str() else {
                self.push(rv.err(codes::WRONG_TYPE, "expected a resource id").got(show(rv.j)));
                return None;
            };
            match self.reg.resources.get(name) {
                Some(x) => Some(*x),
                None => {
                    self.unknown_or_wrong(&rv, name, "resource");
                    None
                }
            }
        };
        Some(match verb {
            "add" => {
                let (r, unit) = res("add")?;
                Action::AddRes { res: r, amount: self.req(v, "amount").and_then(|x| self.num_expr(&x, unit, sc))? }
            }
            "set" => {
                let (r, unit) = res("set")?;
                Action::SetRes { res: r, value: self.req(v, "value").and_then(|x| self.num_expr(&x, unit, sc))? }
            }
            "change" => {
                let cv = key("change")?;
                let Some(text) = cv.j.as_str() else {
                    self.push(cv.err(codes::WRONG_TYPE, "change names an attribute: \"<who>.<attribute>\"").got(show(cv.j)).suggest("e.g. \"self.invested\""));
                    return None;
                };
                let Some((who_src, attr)) = text.rsplit_once('.') else {
                    self.push(cv.err(codes::INVALID, format!("'{text}' names no entity")).suggest(format!("write self.{text}")));
                    return None;
                };
                let who = self.typed_src(&cv, who_src, sc, Ty::Ent)?;
                let (attr, unit) = self.attr_named(&cv, attr)?;
                if let (Me::Arch(a), "self") = (sc.me, who_src) {
                    if !self.reg.arch_attrs[usize::from(a)].contains(&attr) {
                        self.push(cv.err(codes::UNKNOWN_REF, format!("this archetype has no attribute '{}'", self.reg.attr_name(attr))));
                    }
                }
                let (op, val) = match (key("by"), key("to")) {
                    (Some(b), None) => (ChangeOp::By, b),
                    (None, Some(t)) => (ChangeOp::To, t),
                    _ => {
                        self.push(v.err(codes::INVALID, "change needs exactly one of by or to").suggest("e.g. {\"change\": \"self.invested\", \"by\": \"50\"}"));
                        return None;
                    }
                };
                Action::Change { who, attr, op, value: self.num_expr(&val, unit, sc)? }
            }
            "spawn" => {
                let av = key("spawn")?;
                let arch = match av.j.as_str() {
                    Some(id) if self.index_of("archetype", id).is_some() => Ex::Arch(self.spawnable(&av)?),
                    _ => self.typed_expr(&av, sc, Ty::Arch)?,
                };
                let atv = self.req(v, "at")?;
                let at = match atv.j.as_str() {
                    Some(s) if self.index_of("path", s).is_some() => At::Path(self.index_of("path", s).expect("checked")),
                    Some(s) if self.reg.sites.contains_key(s) => At::Site(Ex::Site(self.reg.sites[s])),
                    Some(s) => match expr::compile(s, sc) {
                        Ok((e, Ty::Site)) => At::Site(e),
                        Ok((e, Ty::Ent)) => At::Entity(e),
                        Ok((_, t)) => {
                            self.push(atv.err(codes::WRONG_TYPE, format!("at must be a path, a site or an entity, not {}", t.describe())).suggest("e.g. \"self\", \"arg.site\" or a path id"));
                            return None;
                        }
                        Err(e) => {
                            let mut all: Vec<String> = self.ids("path");
                            all.extend(self.reg.sites.keys().cloned());
                            let mut e = e;
                            if e.code == codes::UNKNOWN_REF {
                                e.suggestion = self.dym(s, all.iter().map(String::as_str).chain(["self", "target", "other"])).or(e.suggestion);
                            }
                            self.expr_err(&atv, s, e);
                            return None;
                        }
                    },
                    None => {
                        self.push(atv.err(codes::WRONG_TYPE, "at is a string: a path id, a site name or an entity expression").got(show(atv.j)));
                        return None;
                    }
                };
                let count = key("count").and_then(|c| self.num_expr(&c, Unit::Int, sc)).unwrap_or(Ex::Int(1));
                let with = key("with").map(|w| self.with_attrs(&w, sc)).unwrap_or_default();
                Action::Spawn { arch, at, count, with }
            }
            "despawn" => Action::Despawn(self.typed_expr(&key("despawn")?, sc, Ty::Ent)?),
            "kill" => Action::Kill(self.typed_expr(&key("kill")?, sc, Ty::Ent)?),
            "damage" => {
                let to = match key("to") {
                    Some(t) => self.typed_expr(&t, sc, Ty::Ent)?,
                    None if sc.target => Ex::Who(expr::Who::Target),
                    None => {
                        self.push(v.err(codes::MISSING_FIELD, "damage needs \"to\" here (there is no target)").suggest("add \"to\": \"self\" (or another entity)"));
                        return None;
                    }
                };
                Action::Damage { to, amount: self.num_expr(&key("damage")?, Unit::Points, sc)? }
            }
            "apply" => {
                let effect = self.ref_of(&key("apply")?, "effect")?;
                let to = match key("to") {
                    Some(t) => self.typed_expr(&t, sc, Ty::Ent)?,
                    None if sc.target => Ex::Who(expr::Who::Target),
                    None if sc.me != Me::None => Ex::Who(expr::Who::Me),
                    None => {
                        self.push(v.err(codes::MISSING_FIELD, "apply needs \"to\" here").suggest("add \"to\": an entity expression"));
                        return None;
                    }
                };
                let munit = self.effect_magnitude_unit(effect);
                Action::Apply {
                    effect,
                    to,
                    magnitude: key("magnitude").and_then(|m| self.num_expr(&m, munit, sc)),
                    duration: key("duration").and_then(|d| self.num_expr(&d, Unit::Duration, sc)),
                }
            }
            "emit" => {
                let ev = key("emit")?;
                let event = self.event_ref(&ev)?;
                if usize::from(event) < BUILTIN_EVENTS.len() {
                    self.push(ev.err(codes::INVALID, format!("'{}' is a built-in event; the engine raises it", self.events[usize::from(event)].name)).suggest("emit a custom event name instead"));
                }
                Action::Emit {
                    event,
                    subject: key("subject").and_then(|s| self.typed_expr(&s, sc, Ty::Ent)),
                    value: key("value").and_then(|x| self.num_expr(&x, Unit::Int, sc)),
                }
            }
            "become" => {
                let bv = key("become")?;
                let arch = match bv.j.as_str() {
                    Some(id) if self.index_of("archetype", id).is_some() => Ex::Arch(self.spawnable(&bv)?),
                    _ => self.typed_expr(&bv, sc, Ty::Arch)?,
                };
                let who = match key("who") {
                    Some(w) => self.typed_expr(&w, sc, Ty::Ent)?,
                    None => {
                        if sc.me == Me::None {
                            self.push(v.err(codes::MISSING_FIELD, "become needs \"who\" here (there is no self)").suggest("add \"who\": \"arg.stone\" (an entity expression)"));
                            return None;
                        }
                        Ex::Who(expr::Who::Me)
                    }
                };
                let mut keep = Vec::new();
                if let Some(kv) = key("keep") {
                    for (i, k) in kv.j.as_array().unwrap_or_default().iter().enumerate() {
                        if let Some((a, _)) = self.any_attr(&kv.index(i, k)) {
                            keep.push(a);
                        }
                    }
                }
                Action::Become { who, arch, keep }
            }
            "next_wave" => {
                if !self.reg.has_waves {
                    self.push(v.err(codes::NOT_AVAILABLE, "next_wave needs a waves item").suggest("add an item with \"kind\": \"waves\""));
                    return None;
                }
                Action::NextWave
            }
            "goto" => {
                let gv = key("goto")?;
                let Some(m) = act.machine else {
                    self.push(gv.err(codes::NOT_AVAILABLE, "goto needs a state machine here").suggest("use goto inside an archetype that has a machine, or inside the machine"));
                    return None;
                };
                let mdef_states: Vec<String> = self.doc.items[&self.ids("machine")[usize::from(m)]]
                    .get("states")
                    .and_then(Json::as_object)
                    .unwrap_or_default()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect();
                let Some(state) = gv.j.as_str().and_then(|s| mdef_states.iter().position(|x| x == s)) else {
                    self.push(gv.err(codes::UNKNOWN_REF, format!("{} is not a state of machine '{}'", show(gv.j), self.ids("machine")[usize::from(m)])).suggest_opt(gv.j.as_str().and_then(|s| self.dym(s, mdef_states.iter().map(String::as_str)))));
                    return None;
                };
                let who = self.who(key("who"), v, act)?;
                Action::Goto { who, machine: m, state: state as u16 }
            }
            "outcome" => Action::Outcome(self.string(&key("outcome")?)?),
            "if" => Action::If {
                cond: self.bool_expr(&key("if")?, sc)?,
                then: key("then").map(|t| self.actions(&t, act)).unwrap_or_default(),
                otherwise: key("else").map(|t| self.actions(&t, act)).unwrap_or_default(),
            },
            "native" => {
                let hook = self.ref_of(&key("native")?, "native")?;
                Action::Native { hook, args: key("args").map(|a| self.native_args(&a, sc)).unwrap_or_default() }
            }
            _ => unreachable!("every action spec is handled"),
        })
    }
}

fn collect_emits(j: &Json, out: &mut BTreeSet<String>) {
    match j {
        Json::Obj(pairs) => {
            for (k, v) in pairs {
                if k == "emit" {
                    if let Some(s) = v.as_str() {
                        out.insert(s.to_owned());
                    }
                }
                collect_emits(v, out);
            }
        }
        Json::Arr(items) => items.iter().for_each(|i| collect_emits(i, out)),
        _ => {}
    }
}

/// `child` over `base`: tags unioned, attributes by name, behaviours replaced by name (the name
/// defaults to the type), other fields replaced. `abstract` is never inherited.
fn merge_archetype(base: &Json, child: &Json) -> Json {
    let mut out: Vec<(String, Json)> = base.as_object().unwrap_or_default().iter().filter(|(k, _)| k != "abstract" && k != "extends").cloned().collect();
    let set = |out: &mut Vec<(String, Json)>, k: &str, v: Json| {
        if let Some(slot) = out.iter_mut().find(|(n, _)| n == k) {
            slot.1 = v;
        } else {
            out.push((k.to_owned(), v));
        }
    };
    for (k, v) in child.as_object().unwrap_or_default() {
        match k.as_str() {
            "extends" => {}
            "tags" => {
                let mut tags: Vec<Json> = base.get("tags").and_then(Json::as_array).unwrap_or_default().to_vec();
                for t in v.as_array().unwrap_or_default() {
                    if !tags.contains(t) {
                        tags.push(t.clone());
                    }
                }
                set(&mut out, "tags", Json::Arr(tags));
            }
            "attributes" => {
                let mut attrs: Vec<(String, Json)> = base.get("attributes").and_then(Json::as_object).unwrap_or_default().to_vec();
                for (n, a) in v.as_object().unwrap_or_default() {
                    if let Some(slot) = attrs.iter_mut().find(|(x, _)| x == n) {
                        slot.1 = a.clone();
                    } else {
                        attrs.push((n.clone(), a.clone()));
                    }
                }
                set(&mut out, "attributes", Json::Obj(attrs));
            }
            "behaviours" => {
                let name_of = |b: &Json| b.get("name").or_else(|| b.get("type")).and_then(Json::as_str).map(str::to_owned);
                let mut list: Vec<Json> = base.get("behaviours").and_then(Json::as_array).unwrap_or_default().to_vec();
                for b in v.as_array().unwrap_or_default() {
                    let n = name_of(b);
                    match list.iter_mut().find(|x| n.is_some() && name_of(x) == n) {
                        Some(slot) => *slot = b.clone(),
                        None => list.push(b.clone()),
                    }
                }
                set(&mut out, "behaviours", Json::Arr(list));
            }
            _ => set(&mut out, k, v.clone()),
        }
    }
    Json::Obj(out)
}

fn dist(a: P3, b: P3) -> i64 {
    let d2 = (0..3).fold(0u128, |acc, i| {
        let d = (i128::from(a[i]) - i128::from(b[i])).unsigned_abs();
        acc.saturating_add(d.saturating_mul(d))
    });
    isqrt(d2).min(i64::MAX as u128) as i64
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

fn unit_desc(u: Unit) -> String {
    match u {
        Unit::Int => "a whole number".into(),
        u => format!("a {} ({})", u.name(), u.human()),
    }
}

fn example(u: Unit) -> &'static str {
    match u {
        Unit::Int => "3",
        Unit::Points => "10 or \"10pt\"",
        Unit::Distance => "7 or \"7m\"",
        Unit::Speed => "0.8 or \"0.8m/s\"",
        Unit::Duration => "1.5 or \"1.5s\"",
        Unit::Ratio => "0.35 or \"35%\"",
    }
}

/// A stored value as the bare number to write in a field of that unit.
fn human_number(v: i64, unit: Unit, hz: u32) -> String {
    let per = unit.per_human(hz) as i64;
    if per == 1000 {
        let s = human(v, Unit::Distance, hz);
        s.trim_end_matches('m').to_owned()
    } else if unit == Unit::Duration {
        format!("\"{v}t\"")
    } else {
        v.to_string()
    }
}

fn show_val(v: i64, unit: Unit, hz: u32) -> String {
    match v {
        i64::MIN => "-inf".into(),
        i64::MAX => "inf".into(),
        v => human(v, unit, hz),
    }
}

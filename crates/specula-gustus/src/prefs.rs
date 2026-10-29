//! The preference store: every pick and rejection of every gallery, with its note, as records
//! agents query before proposing; and **suggestions** from repeated consistent picks of one
//! target (a project-file edit when the parameter says where its value lives, otherwise a prior
//! for later galleries). Suggestions wait for a human; nothing here writes anything.

use crate::colour::{self, Rgb};
use crate::gallery::{Gallery, Kind};
use crate::space::{Apply, Kind as PKind, Param, Value};
use crate::{r4, Result, TasteError};
use specula_core::{hash, Json};
use std::collections::BTreeMap;

pub const SUGGESTIONS_FORMAT: &str = "specula.taste.suggestions.v1";
pub const PRIORS_FORMAT: &str = "specula.taste.priors.v1";
/// Picks of one target that must agree before a suggestion is made.
pub const MIN_PICKS: usize = 3;
/// Agreement: numbers within this share of their range of the median; colours within this
/// Oklab distance of the mean.
pub const NUMBER_TOLERANCE: f64 = 0.12;
pub const COLOUR_TOLERANCE: f64 = 0.04;
/// Records per query page.
pub const MAX_PAGE: usize = 64;

/// One record: a decision (or a dislike) with what it was about.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub gallery: String,
    pub round: usize,
    /// `accept`, `pick` (an edit's pick), `none`, `dislike`, `back`.
    pub kind: &'static str,
    pub user: String,
    pub by: String,
    pub seq: u64,
    pub note: String,
    pub chips: Vec<String>,
    /// The subject's label (`recipe:recipes/tombstone.json`, `scene:overview`, `ui`, `stage`).
    pub subject: String,
    /// For picks, accepts and dislikes: the variant's values (param name, target, value).
    pub values: Vec<(String, String, Value)>,
    /// For "none": how many variants were turned down.
    pub rejected: usize,
}

impl Record {
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("gallery", Json::from(self.gallery.as_str())),
            ("round", Json::from(self.round)),
            ("kind", Json::from(self.kind)),
            ("user", Json::from(self.user.as_str())),
            ("subject", Json::from(self.subject.as_str())),
        ]);
        if self.by != format!("window:{}", self.user) {
            j.set("by", Json::from(self.by.as_str()));
        }
        if !self.note.is_empty() {
            j.set("note", Json::from(self.note.as_str()));
        }
        if !self.chips.is_empty() {
            j.set("chips", Json::arr(self.chips.iter().map(|c| Json::from(c.as_str()))));
        }
        if !self.values.is_empty() {
            j.set("values", Json::obj(self.values.iter().map(|(n, _, v)| (n.clone(), v.to_json()))));
            j.set("targets", Json::obj(self.values.iter().map(|(n, t, _)| (n.clone(), Json::from(t.as_str())))));
        }
        if self.rejected > 0 {
            j.set("rejected", Json::from(self.rejected));
        }
        j
    }
}

/// A subject's label for records and suggestions.
pub fn subject_label(subject: &Json) -> String {
    if let Some(r) = subject.get("recipe").and_then(Json::as_str) {
        return format!("recipe:{r}");
    }
    if let Some(s) = subject.get("scene") {
        return format!("scene:{}", s.get("camera").and_then(Json::as_str).unwrap_or("view"));
    }
    if subject.get("ui").is_some() {
        return "ui".into();
    }
    "stage".into()
}

/// Every record of one gallery, in decision order.
pub fn records(g: &Gallery) -> Vec<Record> {
    let subject = subject_label(&g.subject);
    let mut out = Vec::new();
    let vals = |r: &crate::gallery::Round, i: usize| -> Vec<(String, String, Value)> {
        r.variants.get(i).map_or_else(Vec::new, |v| g.space.params.iter().zip(&v.values).map(|(p, x)| (p.name.clone(), p.target.name(), x.clone())).collect())
    };
    for r in &g.rounds {
        for d in &r.dislikes {
            out.push(Record { gallery: g.id.clone(), round: r.n, kind: "dislike", user: d.user.clone(), by: d.by.clone(), seq: d.seq, note: d.note.clone(), chips: vec![], subject: subject.clone(), values: vals(r, d.variant), rejected: 0 });
        }
        let Some(d) = &r.decision else { continue };
        let base = Record { gallery: g.id.clone(), round: r.n, kind: "", user: d.user.clone(), by: d.by.clone(), seq: d.seq, note: String::new(), chips: vec![], subject: subject.clone(), values: vec![], rejected: 0 };
        out.push(match &d.kind {
            Kind::Accept { variant } => Record { kind: "accept", values: vals(r, *variant), ..base },
            Kind::Edit { variant, note, chips } => Record { kind: "pick", values: vals(r, *variant), note: note.clone(), chips: chips.clone(), ..base },
            Kind::None { note } => Record { kind: "none", note: note.clone(), rejected: r.variants.len() - usize::from(r.incumbent.is_some()), ..base },
            Kind::Back { to } => Record { kind: "back", note: format!("back to round {to}"), ..base },
            Kind::Superseded => continue,
        });
    }
    out.sort_by_key(|r| r.seq);
    out
}

/// A query over the store.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub subject: Option<String>,
    pub target: Option<String>,
    pub user: Option<String>,
    pub kind: Option<String>,
    /// Substring of the note (case-insensitive).
    pub text: Option<String>,
    pub gallery: Option<String>,
    /// Skip this many matching records (newest first).
    pub offset: usize,
    pub limit: usize,
}

impl Query {
    pub fn matches(&self, r: &Record) -> bool {
        self.subject.as_ref().is_none_or(|s| r.subject.contains(s.as_str()))
            && self.target.as_ref().is_none_or(|t| r.values.iter().any(|(_, x, _)| x == t))
            && self.user.as_ref().is_none_or(|u| &r.user == u)
            && self.kind.as_ref().is_none_or(|k| r.kind == k.as_str() || (k == "reject" && matches!(r.kind, "none" | "dislike")))
            && self.text.as_ref().is_none_or(|t| r.note.to_lowercase().contains(&t.to_lowercase()))
            && self.gallery.as_ref().is_none_or(|g| &r.gallery == g)
    }
}

/// [`query`] over records already built (the engine keeps them per gallery file version).
pub fn query_records<'a>(records: impl Iterator<Item = &'a Record>, q: &Query) -> (Vec<Record>, usize) {
    let mut all: Vec<&Record> = records.filter(|r| q.matches(r)).collect();
    // Newest first: by gallery (newest last in order), then decision order.
    all.reverse();
    let total = all.len();
    let page = all.into_iter().skip(q.offset).take(q.limit.clamp(1, MAX_PAGE)).cloned().collect();
    (page, total)
}

/// Matching records newest first (galleries newest first, then decision order reversed): a page
/// and the total.
pub fn query<'a>(galleries: impl Iterator<Item = &'a Gallery>, q: &Query) -> (Vec<Record>, usize) {
    let mut all: Vec<Record> = galleries.flat_map(records).filter(|r| q.matches(r)).collect();
    all.reverse();
    let total = all.len();
    let page = all.into_iter().skip(q.offset).take(q.limit.clamp(1, MAX_PAGE)).collect();
    (page, total)
}

/// A suggestion from repeated picks.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// Stable: the target, where it applies and the value.
    pub id: String,
    pub target: String,
    pub param: String,
    pub value: Value,
    /// Where it would be written (`None`: a prior).
    pub apply: Option<Apply>,
    /// The picks behind it: (gallery, round).
    pub evidence: Vec<(String, usize)>,
    pub spread: f64,
    pub user: String,
}

impl Suggestion {
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("id", Json::from(self.id.as_str())),
            ("target", Json::from(self.target.as_str())),
            ("param", Json::from(self.param.as_str())),
            ("value", self.value.to_json()),
            ("picks", Json::from(self.evidence.len())),
            ("spread", r4(self.spread)),
            ("user", Json::from(self.user.as_str())),
            ("evidence", Json::arr(self.evidence.iter().take(12).map(|(g, r)| Json::from(format!("{g}#{r}"))))),
        ]);
        match &self.apply {
            Some(a) => {
                j.set("kind", Json::from("edit"));
                j.set("apply", a.to_json());
                j.set("what", Json::from(format!("set {} to {}", a.describe(), self.value.toml_literal())));
            }
            None => {
                j.set("kind", Json::from("prior"));
                j.set("what", Json::from(format!("later galleries over {} start at {}", self.target, self.value.toml_literal())));
            }
        }
        j
    }
}

/// One pick of a parameter's value.
struct Pick<'a> {
    param: &'a Param,
    value: Value,
    gallery: String,
    round: usize,
    seq: u64,
}

/// Suggestions from every gallery's accepts and edit picks: per (user, target, apply), when at
/// least [`MIN_PICKS`] agree. `dismissed` ids are left out.
pub fn suggestions<'a>(galleries: impl Iterator<Item = &'a Gallery>, dismissed: &[String]) -> Vec<Suggestion> {
    // (user, target, where) -> picks, in decision order.
    let mut groups: BTreeMap<(String, String, String), Vec<Pick<'a>>> = BTreeMap::new();
    for g in galleries {
        for r in &g.rounds {
            let Some(d) = &r.decision else { continue };
            let Some(i) = d.kind.picked() else { continue };
            let Some(v) = r.variants.get(i) else { continue };
            for (p, x) in g.space.params.iter().zip(&v.values) {
                let at = p.apply.as_ref().map_or_else(String::new, Apply::describe);
                groups.entry((d.user.clone(), p.target.name(), at)).or_default().push(Pick { param: p, value: x.clone(), gallery: g.id.clone(), round: r.n, seq: d.seq });
            }
        }
    }
    let mut out = Vec::new();
    for ((user, target, _), picks) in groups {
        // Repeated picks of one value in one gallery (a tune round shows the pick again) count
        // once: the evidence is distinct (gallery, value) pairs (a review probe: one incumbent
        // picked three times made a "consistent" suggestion).
        let distinct: std::collections::BTreeSet<(String, String)> = picks.iter().map(|p| (p.gallery.clone(), p.value.to_json().to_string())).collect();
        if distinct.len() < MIN_PICKS {
            continue;
        }
        let Some((value, spread)) = agreement(&picks) else { continue };
        let last = picks.iter().max_by_key(|p| (p.gallery.clone(), p.seq)).map_or(picks[0].param, |p| p.param);
        let apply = last.apply.clone();
        let key = format!("{user}|{target}|{}|{}", apply.as_ref().map_or_else(String::new, Apply::describe), value.toml_literal());
        let id = format!("s{:08x}", hash::hash_str(&key) as u32);
        if dismissed.contains(&id) {
            continue;
        }
        out.push(Suggestion { id, target, param: last.name.clone(), value, apply, evidence: picks.iter().map(|p| (p.gallery.clone(), p.round)).collect(), spread, user });
    }
    out
}

/// The agreed value of picks and how far they spread, or `None` when they don't agree.
fn agreement(picks: &[Pick<'_>]) -> Option<(Value, f64)> {
    match &picks[0].param.kind {
        PKind::Number { min, max, step, .. } => {
            let mut v: Vec<f64> = picks.iter().filter_map(|p| p.value.as_num()).collect();
            v.sort_by(f64::total_cmp);
            let med = v[v.len() / 2];
            let range = (max - min).max(1e-9);
            let spread = v.iter().map(|x| (x - med).abs() / range).fold(0.0, f64::max);
            let med = match step {
                Some(s) if *s > 0.0 => min + specula_core::dmath::round_f64((med - min) / s) * s,
                _ => med,
            };
            (spread <= NUMBER_TOLERANCE).then_some((Value::Num(med), spread))
        }
        PKind::Colour { .. } => {
            let labs: Vec<[f64; 3]> = picks.iter().filter_map(|p| p.value.as_colour()).map(colour::srgb_to_oklab).collect();
            let n = labs.len() as f64;
            let mean = [labs.iter().map(|l| l[0]).sum::<f64>() / n, labs.iter().map(|l| l[1]).sum::<f64>() / n, labs.iter().map(|l| l[2]).sum::<f64>() / n];
            let spread = labs.iter().map(|l| colour::lab_distance(*l, mean)).fold(0.0, f64::max);
            let rgb: Rgb = colour::oklab_to_srgb(mean);
            (spread <= COLOUR_TOLERANCE).then_some((Value::Colour(rgb), spread))
        }
        PKind::Choice { .. } => {
            let mut counts: BTreeMap<(usize, String), usize> = BTreeMap::new();
            for p in picks {
                if let Value::Choice(i, s) = &p.value {
                    *counts.entry((*i, s.clone())).or_default() += 1;
                }
            }
            let ((i, s), c) = counts.into_iter().max_by_key(|(k, c)| (*c, std::cmp::Reverse(k.0)))?;
            (c >= MIN_PICKS && c * 4 >= picks.len() * 3).then_some((Value::Choice(i, s), 1.0 - c as f64 / picks.len() as f64))
        }
    }
}

/// What happened to suggestions (`taste/suggestions.json`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuggestionLog {
    /// id -> (state, by, detail): `dismissed`, `submitted` (detail: the review submission),
    /// `applied`.
    pub entries: BTreeMap<String, (String, String, String)>,
}

impl SuggestionLog {
    pub fn dismissed(&self) -> Vec<String> {
        self.entries.iter().filter(|(_, e)| e.0 == "dismissed" || e.0 == "applied").map(|(k, _)| k.clone()).collect()
    }

    pub fn state(&self, id: &str) -> Option<&str> {
        self.entries.get(id).map(|e| e.0.as_str())
    }

    pub fn to_text(&self) -> String {
        let j = Json::obj([
            ("format", Json::from(SUGGESTIONS_FORMAT)),
            ("entries", Json::obj(self.entries.iter().map(|(k, (s, by, d))| (k.clone(), Json::obj([("state", Json::from(s.as_str())), ("by", Json::from(by.as_str())), ("detail", Json::from(d.as_str()))]))))),
        ]);
        let mut t = j.to_pretty();
        t.push('\n');
        t
    }

    pub fn parse(text: &str) -> Result<SuggestionLog> {
        let j = Json::parse(text).map_err(|e| TasteError::new("invalid_json", e.to_string()))?;
        if j.get("format").and_then(Json::as_str) != Some(SUGGESTIONS_FORMAT) {
            return Err(TasteError::new("bad_format", format!("\"format\": \"{SUGGESTIONS_FORMAT}\"")).at("format"));
        }
        let mut entries = BTreeMap::new();
        for (k, v) in j.get("entries").and_then(Json::as_object).unwrap_or_default().iter().take(4096) {
            let s = |f: &str| v.get(f).and_then(Json::as_str).unwrap_or("").to_owned();
            if !matches!(s("state").as_str(), "dismissed" | "submitted" | "applied") {
                return Err(TasteError::new("bad_state", format!("{k}: state is dismissed, submitted or applied")).at(format!("entries.{k}.state")));
            }
            entries.insert(k.clone(), (s("state"), s("by"), s("detail")));
        }
        Ok(SuggestionLog { entries })
    }
}

/// Approved priors (`taste/priors.json`): target -> value; galleries over the target start there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Priors {
    pub by_target: BTreeMap<String, Json>,
}

impl Priors {
    pub fn to_text(&self) -> String {
        let mut t = Json::obj([("format", Json::from(PRIORS_FORMAT)), ("priors", Json::obj(self.by_target.iter().map(|(k, v)| (k.clone(), v.clone()))))]).to_pretty();
        t.push('\n');
        t
    }

    pub fn parse(text: &str) -> Result<Priors> {
        let j = Json::parse(text).map_err(|e| TasteError::new("invalid_json", e.to_string()))?;
        if j.get("format").and_then(Json::as_str) != Some(PRIORS_FORMAT) {
            return Err(TasteError::new("bad_format", format!("\"format\": \"{PRIORS_FORMAT}\"")).at("format"));
        }
        let mut by_target = BTreeMap::new();
        for (k, v) in j.get("priors").and_then(Json::as_object).unwrap_or_default().iter().take(512) {
            if crate::space::Target::parse(k).is_none() {
                return Err(TasteError::new("bad_target", format!("'{k}' is not a target ({})", crate::space::TARGETS)).at(format!("priors.{k}")));
            }
            by_target.insert(k.clone(), v.clone());
        }
        Ok(Priors { by_target })
    }

    /// The start point of `space` with priors applied (other parameters at their defaults).
    pub fn start(&self, space: &crate::space::Space) -> Option<Vec<f64>> {
        let mut vals = BTreeMap::new();
        for p in &space.params {
            if let Some(v) = self.by_target.get(&p.target.name()).and_then(|j| p.parse_value(j)) {
                vals.insert(p.name.clone(), v);
            }
        }
        (!vals.is_empty()).then(|| space.point_of(&vals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::{Gallery, Mode};
    use crate::space::Space;

    fn gallery(id: &str, picks: &[[f64; 4]]) -> Gallery {
        let space = Space::parse(
            &Json::parse(r##"[{"name":"moon","target":"light.key_color","base":"#d0d9f0","apply":{"file":"style.toml","toml":"stage.game.key_color"}},{"name":"fog","target":"fog.density","min":0,"max":0.04}]"##).unwrap(),
        )
        .unwrap();
        let mut g = Gallery::new(id, "t", Json::obj([("scene", Json::obj([("camera", Json::from("overview"))]))]), space, Mode::Tune, 3, 9, "lead").unwrap();
        for (k, p) in picks.iter().enumerate() {
            let mut plan = g.first_round(None);
            plan.after = k;
            plan.points[0] = p.to_vec();
            if k > 0 {
                plan.made = crate::gallery::Made::Tune;
            }
            g.push_round(plan, None, vec![]).unwrap();
            let kind = if k + 1 == picks.len() { Kind::Accept { variant: 0 } } else { Kind::Edit { variant: 0, note: format!("note {k}"), chips: vec![] } };
            g.decide(k + 1, kind, "window:owner", "owner").unwrap();
        }
        g
    }

    #[test]
    fn records_and_queries() {
        let g = gallery("moon", &[[0.4, 0.5, 0.5, 0.2], [0.42, 0.5, 0.5, 0.8]]);
        let recs = records(&g);
        assert_eq!(recs.iter().map(|r| r.kind).collect::<Vec<_>>(), vec!["pick", "accept"]);
        assert_eq!(recs[0].note, "note 0");
        let (page, total) = query([&g].into_iter(), &Query { kind: Some("accept".into()), limit: 10, ..Query::default() });
        assert_eq!((page.len(), total), (1, 1));
        let (page, _) = query([&g].into_iter(), &Query { text: Some("NOTE".into()), limit: 10, ..Query::default() });
        assert_eq!(page.len(), 1);
        let (page, _) = query([&g].into_iter(), &Query { target: Some("fog.density".into()), limit: 10, ..Query::default() });
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].to_json().get("values").and_then(|v| v.get("fog")).and_then(Json::as_f64), Some(0.032), "newest first");
    }

    #[test]
    fn consistent_picks_become_suggestions() {
        // Three picks with nearly the same moon colour; the fog spreads out.
        let g = gallery("moon", &[[0.40, 0.5, 0.5, 0.1], [0.42, 0.5, 0.5, 0.5], [0.41, 0.52, 0.5, 0.9]]);
        let s = suggestions([&g].into_iter(), &[]);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(s[0].target, "light.key_color");
        assert_eq!(s[0].evidence.len(), 3);
        assert!(s[0].to_json().get("what").and_then(Json::as_str).unwrap().starts_with("set style.toml stage.game.key_color to \"#"));
        // Dismissed suggestions stay away; too few picks suggest nothing.
        assert!(suggestions([&g].into_iter(), &[s[0].id.clone()]).is_empty());
        let few = gallery("few", &[[0.4, 0.5, 0.5, 0.1], [0.4, 0.5, 0.5, 0.1]]);
        assert!(suggestions([&few].into_iter(), &[]).is_empty());
        // Review probe: the same incumbent picked three times in one gallery is one piece of
        // evidence, not three.
        let same = gallery("same", &[[0.4, 0.5, 0.5, 0.1], [0.4, 0.5, 0.5, 0.1], [0.4, 0.5, 0.5, 0.1]]);
        assert!(suggestions([&same].into_iter(), &[]).is_empty(), "one value picked again and again");
        // ...but the same value picked in three galleries is.
        let (a, b, c) = (gallery("a", &[[0.4, 0.5, 0.5, 0.1]]), gallery("b", &[[0.4, 0.5, 0.5, 0.1]]), gallery("c", &[[0.4, 0.5, 0.5, 0.1]]));
        assert!(!suggestions([&a, &b, &c].into_iter(), &[]).is_empty());
        // The log and priors round-trip.
        let mut log = SuggestionLog::default();
        log.entries.insert(s[0].id.clone(), ("submitted".into(), "lead".into(), "review 4".into()));
        assert_eq!(SuggestionLog::parse(&log.to_text()).unwrap(), log);
        let mut pr = Priors::default();
        pr.by_target.insert("fog.density".into(), Json::from(0.02));
        let back = Priors::parse(&pr.to_text()).unwrap();
        let start = back.start(&g.space).unwrap();
        assert!((start[3] - 0.5).abs() < 1e-9);
    }
}

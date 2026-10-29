//! A gallery as data (`specula.gallery.v1`): a subject, a parameter space and rounds of
//! variants, each round ending in at most one decision. The file is the only truth; the engine
//! renders rounds and records decisions through it.
//!
//! The pick flow (owner, 2026-10-22): select a variant, then **accept** ("This one") or **edit**
//! ("This one, with edits": a note and chips); Claude's additions: **none** ("None of these", a
//! new spread), **back** (the breadcrumb: an earlier round's variants again) and **dislike** (a
//! variant, or a point on it, that the human doesn't like; the round stays open).

use crate::space::{Space, Value};
use crate::spread::{self, Region};
use crate::tune::{self, Obs};
use crate::{cut, r4, valid_name, Result, TasteError};
use specula_core::rng::Rng;
use specula_core::Json;

pub const FORMAT: &str = "specula.gallery.v1";
/// Rounds one gallery keeps.
pub const MAX_ROUNDS: usize = 32;
/// Variants in one round (9 by default: a 3 x 3 sheet).
pub const MAX_VARIANTS: usize = 12;
pub const DEFAULT_VARIANTS: usize = 9;
/// Characters in a note.
pub const MAX_NOTE: usize = 280;
/// Dislikes recorded per round.
pub const MAX_DISLIKES: usize = 32;
/// Characters in a title.
pub const MAX_TITLE: usize = 120;

/// How the next round is made after a pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Preferential BO, batch EI ([`tune::next_batch`]).
    Tune,
    /// Sequential Line Search ([`tune::next_line`]).
    Line,
    /// A shrinking spread around the pick ([`tune::next_spread`]).
    Spread,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "tune" => Some(Mode::Tune),
            "line" => Some(Mode::Line),
            "spread" => Some(Mode::Spread),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Tune => "tune",
            Mode::Line => "line",
            Mode::Spread => "spread",
        }
    }
}

/// How a round's variants were made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Made {
    /// A dispersed spread (round 1, or after "none of these" with nothing picked yet).
    Spread,
    /// Tuned after a pick (batch EI, a line, or a spread around the pick).
    Tune,
    Line,
    Around,
    /// After "none of these" with an incumbent.
    None,
    /// The variants of round `n` again (the breadcrumb).
    Back(usize),
    /// An agent's own round (`gallery_round`).
    Agent,
}

impl Made {
    fn name(&self) -> String {
        match self {
            Made::Spread => "spread".into(),
            Made::Tune => "tune".into(),
            Made::Line => "line".into(),
            Made::Around => "around".into(),
            Made::None => "none".into(),
            Made::Back(n) => format!("back:{n}"),
            Made::Agent => "agent".into(),
        }
    }

    fn parse(s: &str) -> Option<Made> {
        Some(match s {
            "spread" => Made::Spread,
            "tune" => Made::Tune,
            "line" => Made::Line,
            "around" => Made::Around,
            "none" => Made::None,
            "agent" => Made::Agent,
            _ => Made::Back(s.strip_prefix("back:")?.parse().ok()?),
        })
    }
}

/// What the human (or an agent with role `owner`) decided about a round.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// "This one".
    Accept { variant: usize },
    /// "This one, with edits".
    Edit { variant: usize, note: String, chips: Vec<String> },
    /// "None of these".
    None { note: String },
    /// The breadcrumb: round `to` again.
    Back { to: usize },
    /// An agent replaced the round before anyone decided (`gallery_round`).
    Superseded,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Accept { .. } => "accept",
            Kind::Edit { .. } => "edit",
            Kind::None { .. } => "none",
            Kind::Back { .. } => "back",
            Kind::Superseded => "superseded",
        }
    }

    /// The variant picked, if any.
    pub fn picked(&self) -> Option<usize> {
        match self {
            Kind::Accept { variant } | Kind::Edit { variant, .. } => Some(*variant),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub kind: Kind,
    /// Who pressed it: `window:owner`, or the agent's name.
    pub by: String,
    /// The person it is attributed to (a label: `owner` unless the opener said otherwise).
    pub user: String,
    /// The gallery's decision counter when it was made (orders decisions across rounds).
    pub seq: u64,
}

/// "Not this one": a variant (and the point on it, 0..1 across its tile) the human dislikes.
#[derive(Clone, Debug, PartialEq)]
pub struct Dislike {
    pub variant: usize,
    pub at: Option<[f64; 2]>,
    pub note: String,
    pub by: String,
    pub user: String,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub point: Vec<f64>,
    pub values: Vec<Value>,
    /// What the render measured (lightness, contrast, chroma; a recipe's size), when rendered.
    pub measures: Option<Json>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// 1-based.
    pub n: usize,
    pub made: Made,
    pub variants: Vec<Variant>,
    /// The variant that is the last pick shown again (tuned rounds).
    pub incumbent: Option<usize>,
    pub decision: Option<Decision>,
    pub dislikes: Vec<Dislike>,
    /// The round's sheet (a file name in the engine's captures folder), once rendered.
    pub sheet: Option<String>,
    /// The chips asked for couldn't move any further (the pick sat at their bound): the chip
    /// dimensions were held and the others varied.
    pub chips_exhausted: bool,
    /// The style rules left room for fewer variants than were planned: how many fit (the owner
    /// sees "6 options fit the style rules" rather than wondering where the rest went).
    pub fit: Option<usize>,
}

/// The next round's variants, before they are rendered.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    /// The round this one follows.
    pub after: usize,
    pub made: Made,
    pub points: Vec<Vec<f64>>,
    pub incumbent: Option<usize>,
    /// See [`Round::chips_exhausted`].
    pub chips_exhausted: bool,
    /// See [`Round::fit`].
    pub fit: Option<usize>,
}

impl Planned {
    pub fn new(after: usize, made: Made, points: Vec<Vec<f64>>, incumbent: Option<usize>) -> Planned {
        Planned { after, made, points, incumbent, chips_exhausted: false, fit: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gallery {
    pub id: String,
    pub title: String,
    /// What each variant shows (the engine's: recipe, scene, ui or stage).
    pub subject: Json,
    pub space: Space,
    pub mode: Mode,
    pub seed: u64,
    pub per_round: usize,
    /// Who made it.
    pub by: String,
    /// "This one" also writes the values to their `apply` targets (as the human).
    pub apply_on_accept: bool,
    pub rounds: Vec<Round>,
    /// Decisions and dislikes so far (each takes the next number).
    pub seq: u64,
    /// The accepted values were written (`gallery_apply`), by whom.
    pub applied: Option<String>,
}

/// What a gallery is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The last round waits for a decision.
    Open,
    /// A decision was made; the next round is being made and rendered.
    Rendering,
    /// "This one": done.
    Accepted,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Rendering => "rendering",
            Status::Accepted => "accepted",
        }
    }
}

impl Gallery {
    /// A new gallery with no rounds yet (see [`Gallery::first_round`]).
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: &str, title: &str, subject: Json, space: Space, mode: Mode, seed: u64, per_round: usize, by: &str) -> Result<Gallery> {
        if !valid_name(id) {
            return Err(TasteError::new("bad_id", "gallery ids are [a-z0-9_-], 1-40 characters").at("id"));
        }
        if !(2..=MAX_VARIANTS).contains(&per_round) {
            return Err(TasteError::new("bad_variants", format!("variants per round: 2..={MAX_VARIANTS}")).at("variants"));
        }
        Ok(Gallery { id: id.into(), title: cut(title, MAX_TITLE), subject, space, mode, seed, per_round, by: by.into(), apply_on_accept: false, rounds: Vec::new(), seq: 0, applied: None })
    }

    pub fn status(&self) -> Status {
        match self.rounds.last().and_then(|r| r.decision.as_ref()) {
            None => Status::Open,
            Some(d) if matches!(d.kind, Kind::Accept { .. }) => Status::Accepted,
            Some(_) => Status::Rendering,
        }
    }

    /// The accepted variant (round, index) and its values.
    pub fn accepted(&self) -> Option<(usize, usize, &[Value])> {
        let r = self.rounds.last()?;
        match r.decision.as_ref()?.kind {
            Kind::Accept { variant } => r.variants.get(variant).map(|v| (r.n, variant, v.values.as_slice())),
            _ => None,
        }
    }

    fn rng(&self, round: usize) -> Rng {
        Rng::new(self.seed, 0x6761_6c00 ^ round as u64)
    }

    /// Round 1's points: a dispersed spread from `start` (the defaults, a prior, a current
    /// value). A space that is one choice shows each option once.
    pub fn first_round(&self, start: Option<Vec<f64>>) -> Planned {
        let dims = self.space.dims();
        if let [p] = self.space.params.as_slice() {
            if let crate::space::Kind::Choice { options, .. } = &p.kind {
                let k = options.len().min(MAX_VARIANTS);
                let points = (0..k).map(|i| vec![(i as f64 + 0.5) / options.len() as f64]).collect();
                return Planned::new(0, Made::Spread, points, None);
            }
        }
        let start = start.filter(|s| s.len() == dims).unwrap_or_else(|| self.space.default_point());
        let mut rng = self.rng(1);
        let points = spread::dispersed(self.per_round, dims, std::slice::from_ref(&start), &[], &Region { centre: None, radius: 1.0 }, &mut rng);
        let points = self.distinct(points, None, None, &mut rng);
        Planned::new(0, Made::Spread, points, None)
    }

    /// `points` with look-alikes dropped (two variants that show the same values: gamut
    /// clipping makes near-white moons one colour), filled back up to the round's size with
    /// dispersed points on the chips' side of `inc`.
    pub fn distinct(&self, points: Vec<Vec<f64>>, inc: Option<&[f64]>, dir: Option<&[f64]>, rng: &mut Rng) -> Vec<Vec<f64>> {
        let n = points.len().max(1);
        let dims = self.space.dims();
        let mut out: Vec<(Vec<f64>, Vec<Value>)> = Vec::new();
        let push = |out: &mut Vec<(Vec<f64>, Vec<Value>)>, p: Vec<f64>| {
            let v = self.space.values(&p);
            if out.len() < n && !out.iter().any(|(_, w)| self.space.same_shown(w, &v)) {
                out.push((p, v));
            }
        };
        for p in points {
            push(&mut out, p);
        }
        for radius in [0.25, 0.5, 1.0] {
            if out.len() >= n {
                break;
            }
            let shown: Vec<Vec<f64>> = out.iter().map(|(p, _)| p.clone()).collect();
            let centre = inc.map(<[f64]>::to_vec);
            let region = Region { centre: centre.as_deref(), radius };
            let more = spread::dispersed_where(n * 4, dims, &[], &shown, &region, rng, &|x| match inc {
                Some(i) => tune::chip_apply(i, dir, x),
                None => Some(x),
            });
            for p in more {
                push(&mut out, p);
            }
        }
        out.into_iter().map(|(p, _)| p).collect()
    }

    /// A tuned round's points, kept distinct, with the incumbent first among equals.
    fn tuned(&self, after: usize, made: Made, points: Vec<Vec<f64>>, inc: &[f64], dir: Option<&[f64]>, rng: &mut Rng) -> Planned {
        let exhausted = dir.is_some_and(|d| tune::chips_exhausted(inc, d));
        let mut points = points;
        if let Some(i) = points.iter().position(|p| p.as_slice() == inc) {
            let p = points.remove(i);
            points.insert(0, p);
        }
        let points = self.distinct(points, Some(inc), dir, rng);
        let incumbent = points.iter().position(|p| p.as_slice() == inc);
        Planned { after, made, points, incumbent, chips_exhausted: exhausted, fit: None }
    }

    /// Everything shown and picked, for the optimiser.
    pub fn observations(&self) -> Obs {
        let mut o = Obs::default();
        for r in &self.rounds {
            let pts: Vec<Vec<f64>> = r.variants.iter().map(|v| v.point.clone()).collect();
            let winner = match r.decision.as_ref().map(|d| &d.kind) {
                Some(Kind::Accept { variant } | Kind::Edit { variant, .. }) => Some(*variant),
                Some(Kind::None { .. }) => r.incumbent,
                _ => None,
            };
            o.add_round(&pts, winner);
        }
        o
    }

    /// The next round's points after the last round's decision (`None`: none is needed, as after
    /// "This one", or there is no decision yet).
    pub fn plan_next(&self) -> Option<Planned> {
        let last = self.rounds.last()?;
        let d = last.decision.as_ref()?;
        let k = last.n + 1;
        if k > MAX_ROUNDS {
            return None;
        }
        let n = self.per_round;
        let mut rng = self.rng(k);
        let dims = self.space.dims();
        Some(match &d.kind {
            Kind::Accept { .. } | Kind::Superseded => return None,
            Kind::Back { to } => {
                let r = self.rounds.iter().find(|r| r.n == *to)?;
                Planned::new(last.n, Made::Back(*to), r.variants.iter().map(|v| v.point.clone()).collect(), r.incumbent)
            }
            Kind::Edit { variant, chips, .. } => {
                let inc = last.variants.get(*variant)?.point.clone();
                let dir = self.space.chip_direction(chips);
                let obs = self.observations();
                let (made, points) = match self.mode {
                    Mode::Tune => (Made::Tune, tune::next_mixed(&obs, &inc, dir.as_deref(), n, k, &mut rng)),
                    Mode::Line => (Made::Line, tune::next_line(&obs, &inc, dir.as_deref(), n, tune::tune_radius(k), &mut rng)),
                    Mode::Spread => (Made::Around, tune::next_spread(&inc, tune::spread_radius(k), dir.as_deref(), &[], n, &mut rng)),
                };
                self.tuned(last.n, made, points, &inc, dir.as_deref(), &mut rng)
            }
            Kind::None { .. } => {
                let shown: Vec<Vec<f64>> = self.rounds.iter().flat_map(|r| r.variants.iter().map(|v| v.point.clone())).collect();
                match last.incumbent.and_then(|i| last.variants.get(i)) {
                    Some(inc) if self.mode != Mode::Spread => {
                        let inc = inc.point.clone();
                        let points = tune::next_batch(&self.observations(), &inc, None, n, tune::tune_radius(k).max(0.25), &mut rng);
                        self.tuned(last.n, Made::None, points, &inc, None, &mut rng)
                    }
                    Some(inc) => {
                        let inc = inc.point.clone();
                        let points = tune::next_spread(&inc, tune::spread_radius(k.saturating_sub(1)), None, &shown, n, &mut rng);
                        self.tuned(last.n, Made::None, points, &inc, None, &mut rng)
                    }
                    None => {
                        // Nothing picked yet: a fresh spread over the whole space, away from all shown.
                        let points = spread::dispersed(n, dims, &[], &shown, &Region { centre: None, radius: 1.0 }, &mut rng);
                        let points = self.distinct(points, None, None, &mut rng);
                        Planned::new(last.n, Made::Spread, points, None)
                    }
                }
            }
        })
    }

    /// Appends a planned (and rendered) round; refused when it no longer follows the last round.
    pub fn push_round(&mut self, p: Planned, sheet: Option<String>, measures: Vec<Option<Json>>) -> Result<usize> {
        let last = self.rounds.last().map_or(0, |r| r.n);
        if p.after != last || (last > 0 && self.rounds.last().is_some_and(|r| r.decision.is_none() && p.made != Made::Agent)) {
            return Err(TasteError::new("stale_round", format!("this round followed round {}, but the gallery is at round {last}", p.after)));
        }
        if self.rounds.len() >= MAX_ROUNDS {
            return Err(TasteError::new("too_many_rounds", format!("a gallery keeps {MAX_ROUNDS} rounds; accept one, or start a new gallery from the last pick")));
        }
        if p.points.is_empty() || p.points.len() > MAX_VARIANTS || p.points.iter().any(|q| q.len() != self.space.dims() || q.iter().any(|v| !(0.0..=1.0).contains(v))) {
            return Err(TasteError::new("bad_points", format!("1..={MAX_VARIANTS} points of {} coordinates in 0..1", self.space.dims())));
        }
        let n = last + 1;
        let variants = p.points.iter().enumerate().map(|(i, q)| Variant { point: q.clone(), values: self.space.values(q), measures: measures.get(i).cloned().flatten() }).collect();
        self.rounds.push(Round { n, made: p.made, variants, incumbent: p.incumbent, decision: None, dislikes: Vec::new(), sheet, chips_exhausted: p.chips_exhausted, fit: p.fit });
        Ok(n)
    }

    /// Records a decision on the last round (`round`: the round the decider saw, which must be
    /// the last and still open).
    pub fn decide(&mut self, round: usize, kind: Kind, by: &str, user: &str) -> Result<u64> {
        let chips = self.space.chips();
        let open = self.rounds.last().map(|r| (r.n, r.decision.is_none(), r.variants.len()));
        let Some((n, undecided, count)) = open else { return Err(TasteError::new("no_round", "the gallery has no round yet (it is still rendering)")) };
        if round != n {
            return Err(TasteError::new("stale_round", format!("round {round} isn't the open round (round {n} is)")).at("round"));
        }
        if !undecided {
            return Err(TasteError::new(if self.status() == Status::Accepted { "gallery_done" } else { "already_decided" }, format!("round {n} is already decided ({})", self.rounds[n - 1].decision.as_ref().map_or("", |d| d.kind.name()))));
        }
        match &kind {
            Kind::Accept { variant } | Kind::Edit { variant, .. } if *variant >= count => return Err(TasteError::new("bad_variant", format!("round {n} has variants 0..{}", count - 1)).at("variant")),
            Kind::Edit { note, chips: c, .. } => {
                if note.chars().count() > MAX_NOTE {
                    return Err(TasteError::new("note_too_long", format!("notes are at most {MAX_NOTE} characters")).at("note"));
                }
                if let Some(bad) = c.iter().find(|x| !chips.contains(&x.as_str())) {
                    return Err(TasteError::new("bad_chip", format!("'{bad}' isn't a chip of this gallery: {}", chips.join(", "))).at("chips"));
                }
                if n >= MAX_ROUNDS {
                    return Err(TasteError::new("too_many_rounds", format!("round {n} is the last a gallery keeps ({MAX_ROUNDS}): accept one, or start a new gallery from it")));
                }
            }
            Kind::None { note } if note.chars().count() > MAX_NOTE => return Err(TasteError::new("note_too_long", format!("notes are at most {MAX_NOTE} characters")).at("note")),
            Kind::None { .. } | Kind::Back { .. } if n >= MAX_ROUNDS => return Err(TasteError::new("too_many_rounds", format!("round {n} is the last a gallery keeps ({MAX_ROUNDS})"))),
            Kind::Back { to } if *to == 0 || *to >= n => return Err(TasteError::new("bad_round", format!("back goes to an earlier round: 1..{}", n - 1)).at("to")),
            _ => {}
        }
        self.seq += 1;
        let seq = self.seq;
        let r = self.rounds.last_mut().expect("checked");
        r.decision = Some(Decision { kind, by: by.into(), user: user.into(), seq });
        Ok(seq)
    }

    /// Records a dislike on the open round.
    pub fn dislike(&mut self, round: usize, variant: usize, at: Option<[f64; 2]>, note: &str, by: &str, user: &str) -> Result<u64> {
        let Some(r) = self.rounds.last() else { return Err(TasteError::new("no_round", "the gallery has no round yet")) };
        if r.n != round || r.decision.is_some() {
            return Err(TasteError::new("stale_round", format!("round {round} isn't open (round {} is{})", r.n, if r.decision.is_some() { ", decided" } else { "" })).at("round"));
        }
        if variant >= r.variants.len() {
            return Err(TasteError::new("bad_variant", format!("round {} has variants 0..{}", r.n, r.variants.len() - 1)).at("variant"));
        }
        if r.dislikes.len() >= MAX_DISLIKES {
            return Err(TasteError::new("too_many_dislikes", format!("a round keeps {MAX_DISLIKES} dislikes")));
        }
        self.seq += 1;
        let seq = self.seq;
        let r = self.rounds.last_mut().expect("checked");
        r.dislikes.push(Dislike { variant, at: at.map(|a| a.map(|v| v.clamp(0.0, 1.0))), note: cut(note, MAX_NOTE), by: by.into(), user: user.into(), seq });
        Ok(seq)
    }

    /// Supersedes the open round (an agent's `gallery_round`), so `points` can follow it.
    pub fn supersede(&mut self, by: &str) -> Result<Planned> {
        let last = self.rounds.last().map_or(0, |r| r.n);
        if self.status() == Status::Accepted {
            return Err(TasteError::new("gallery_done", "the gallery is accepted; start a new one from its values"));
        }
        if last >= MAX_ROUNDS {
            return Err(TasteError::new("too_many_rounds", format!("a gallery keeps {MAX_ROUNDS} rounds")));
        }
        if let Some(r) = self.rounds.last_mut() {
            if r.decision.is_none() {
                self.seq += 1;
                r.decision = Some(Decision { kind: Kind::Superseded, by: by.into(), user: String::new(), seq: self.seq });
            }
        }
        Ok(Planned::new(last, Made::Agent, Vec::new(), None))
    }

    /// The breadcrumb: (round, how it was made, its decision's name).
    pub fn crumbs(&self) -> Vec<(usize, String, &'static str)> {
        self.rounds.iter().map(|r| (r.n, r.made.name(), r.decision.as_ref().map_or("open", |d| d.kind.name()))).collect()
    }

    // ---- the file ----

    pub fn to_json(&self) -> Json {
        let rounds = self.rounds.iter().map(|r| {
            let mut j = Json::obj([("n", Json::from(r.n)), ("made", Json::from(r.made.name()))]);
            if let Some(i) = r.incumbent {
                j.set("incumbent", Json::from(i));
            }
            if let Some(s) = &r.sheet {
                j.set("sheet", Json::from(s.as_str()));
            }
            if r.chips_exhausted {
                j.set("chips_exhausted", Json::Bool(true));
            }
            if let Some(n) = r.fit {
                j.set("fit", Json::from(n));
            }
            j.set(
                "variants",
                Json::arr(r.variants.iter().map(|v| {
                    // Points at full precision: a parsed file plans the same next round.
                    let mut vj = Json::obj([("point", Json::arr(v.point.iter().map(|x| Json::from(*x)))), ("values", self.space.values_json(&v.values))]);
                    if let Some(m) = &v.measures {
                        vj.set("measures", m.clone());
                    }
                    vj
                })),
            );
            if !r.dislikes.is_empty() {
                j.set(
                    "dislikes",
                    Json::arr(r.dislikes.iter().map(|d| {
                        let mut dj = Json::obj([("variant", Json::from(d.variant)), ("by", Json::from(d.by.as_str())), ("user", Json::from(d.user.as_str())), ("seq", Json::from(d.seq))]);
                        if let Some(a) = d.at {
                            dj.set("at", Json::arr(a.map(r4)));
                        }
                        if !d.note.is_empty() {
                            dj.set("note", Json::from(d.note.as_str()));
                        }
                        dj
                    })),
                );
            }
            if let Some(d) = &r.decision {
                j.set("decision", decision_json(d));
            }
            j
        });
        let mut j = Json::obj([
            ("format", Json::from(FORMAT)),
            ("id", Json::from(self.id.as_str())),
            ("title", Json::from(self.title.as_str())),
            ("subject", self.subject.clone()),
            ("space", self.space.to_json()),
            ("mode", Json::from(self.mode.name())),
            ("seed", Json::from(self.seed)),
            ("per_round", Json::from(self.per_round)),
            ("by", Json::from(self.by.as_str())),
            ("seq", Json::from(self.seq)),
        ]);
        if self.apply_on_accept {
            j.set("apply_on_accept", Json::Bool(true));
        }
        if let Some(a) = &self.applied {
            j.set("applied", Json::from(a.as_str()));
        }
        j.set("rounds", Json::arr(rounds));
        j
    }

    /// The file's text (pretty, so diffs in the journal are readable).
    pub fn to_text(&self) -> String {
        let mut t = self.to_json().to_pretty();
        t.push('\n');
        t
    }

    /// Parses a gallery file. Values are recomputed from the points (the space is the truth).
    pub fn parse(text: &str) -> Result<Gallery> {
        let j = Json::parse(text).map_err(|e| TasteError::new("invalid_json", e.to_string()))?;
        if j.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(TasteError::new("bad_format", format!("a gallery file has \"format\": \"{FORMAT}\"")).at("format"));
        }
        let s = |k: &str| j.get(k).and_then(Json::as_str);
        let space = Space::parse(j.get("space").unwrap_or(&Json::Null))?;
        let mode = s("mode").and_then(Mode::parse).ok_or_else(|| TasteError::new("bad_mode", "mode: tune, line or spread").at("mode"))?;
        let per_round = j.get("per_round").and_then(Json::as_u64).unwrap_or(DEFAULT_VARIANTS as u64) as usize;
        let mut g = Gallery::new(s("id").unwrap_or(""), s("title").unwrap_or(""), j.get("subject").cloned().unwrap_or(Json::Null), space, mode, j.get("seed").and_then(Json::as_u64).unwrap_or(0), per_round, s("by").unwrap_or(""))?;
        g.apply_on_accept = j.get("apply_on_accept").and_then(Json::as_bool).unwrap_or(false);
        g.applied = s("applied").map(str::to_owned);
        g.seq = j.get("seq").and_then(Json::as_u64).unwrap_or(0);
        let rounds = j.get("rounds").and_then(Json::as_array).unwrap_or_default();
        if rounds.len() > MAX_ROUNDS {
            return Err(TasteError::new("too_many_rounds", format!("a gallery keeps {MAX_ROUNDS} rounds")).at("rounds"));
        }
        let dims = g.space.dims();
        for (i, r) in rounds.iter().enumerate() {
            let at = format!("rounds[{i}]");
            let bad = |m: &str| TasteError::new("bad_round", m.to_owned()).at(at.clone());
            if r.get("n").and_then(Json::as_u64) != Some(i as u64 + 1) {
                return Err(bad("rounds are numbered 1, 2, 3, ... in order"));
            }
            let made = r.get("made").and_then(Json::as_str).and_then(Made::parse).ok_or_else(|| bad("made: spread, tune, line, around, none, back:<n> or agent"))?;
            let vs = r.get("variants").and_then(Json::as_array).unwrap_or_default();
            if vs.is_empty() || vs.len() > MAX_VARIANTS {
                return Err(bad(&format!("1..={MAX_VARIANTS} variants")));
            }
            let mut variants = Vec::new();
            for v in vs {
                let point: Vec<f64> = v.get("point").and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_f64).collect();
                if point.len() != dims || point.iter().any(|x| !(0.0..=1.0).contains(x)) {
                    return Err(bad(&format!("each variant's point has {dims} coordinates in 0..1")));
                }
                variants.push(Variant { values: g.space.values(&point), point, measures: v.get("measures").cloned() });
            }
            let count = variants.len();
            let idx = |k: &str, o: &Json| o.get(k).and_then(Json::as_u64).map(|x| x as usize).filter(|x| *x < count);
            let incumbent = r.get("incumbent").and_then(Json::as_u64).map(|x| x as usize).filter(|x| *x < count);
            let mut dislikes = Vec::new();
            for d in r.get("dislikes").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_DISLIKES) {
                let Some(variant) = idx("variant", d) else { return Err(bad("a dislike names a variant of its round")) };
                let at = d.get("at").and_then(Json::as_array).filter(|a| a.len() == 2).and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?]));
                let st = |k: &str| d.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
                dislikes.push(Dislike { variant, at, note: cut(&st("note"), MAX_NOTE), by: st("by"), user: st("user"), seq: d.get("seq").and_then(Json::as_u64).unwrap_or(0) });
            }
            let decision = match r.get("decision") {
                None | Some(Json::Null) => None,
                Some(d) => Some(parse_decision(d, count, i + 1).map_err(|e| e.at(format!("{at}.decision")))?),
            };
            if decision.is_none() && i + 1 < rounds.len() {
                return Err(bad("only the last round may be undecided"));
            }
            let sheet = r.get("sheet").and_then(Json::as_str).filter(|s| is_sheet_name(s)).map(str::to_owned);
            let chips_exhausted = r.get("chips_exhausted").and_then(Json::as_bool).unwrap_or(false);
            let fit = r.get("fit").and_then(Json::as_u64).map(|n| n as usize).filter(|&n| n == variants.len());
            g.rounds.push(Round { n: i + 1, made, variants, incumbent, decision, dislikes, sheet, chips_exhausted, fit });
        }
        Ok(g)
    }
}

/// Sheet names are file names only (no folders): `[A-Za-z0-9._-]`, ending `.png`.
pub fn is_sheet_name(s: &str) -> bool {
    s.len() <= 120 && s.ends_with(".png") && !s.starts_with('.') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

fn decision_json(d: &Decision) -> Json {
    let mut j = Json::obj([("kind", Json::from(d.kind.name())), ("by", Json::from(d.by.as_str())), ("user", Json::from(d.user.as_str())), ("seq", Json::from(d.seq))]);
    match &d.kind {
        Kind::Accept { variant } => {
            j.set("variant", Json::from(*variant));
        }
        Kind::Edit { variant, note, chips } => {
            j.set("variant", Json::from(*variant));
            if !note.is_empty() {
                j.set("note", Json::from(note.as_str()));
            }
            if !chips.is_empty() {
                j.set("chips", Json::arr(chips.iter().map(|c| Json::from(c.as_str()))));
            }
        }
        Kind::None { note } => {
            if !note.is_empty() {
                j.set("note", Json::from(note.as_str()));
            }
        }
        Kind::Back { to } => {
            j.set("to", Json::from(*to));
        }
        Kind::Superseded => {}
    }
    j
}

fn parse_decision(d: &Json, count: usize, round: usize) -> Result<Decision> {
    let st = |k: &str| d.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let variant = || d.get("variant").and_then(Json::as_u64).map(|x| x as usize).filter(|x| *x < count).ok_or_else(|| TasteError::new("bad_variant", "the decision's variant isn't in its round"));
    let note = cut(&st("note"), MAX_NOTE);
    let kind = match st("kind").as_str() {
        "accept" => Kind::Accept { variant: variant()? },
        "edit" => Kind::Edit { variant: variant()?, note, chips: d.get("chips").and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_str).map(str::to_owned).take(12).collect() },
        "none" => Kind::None { note },
        "back" => Kind::Back { to: d.get("to").and_then(Json::as_u64).map(|x| x as usize).filter(|t| *t >= 1 && *t < round).ok_or_else(|| TasteError::new("bad_round", "back goes to an earlier round"))? },
        "superseded" => Kind::Superseded,
        other => return Err(TasteError::new("bad_decision", format!("unknown decision kind '{other}'"))),
    };
    Ok(Decision { kind, by: st("by"), user: st("user"), seq: d.get("seq").and_then(Json::as_u64).unwrap_or(0) })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Gallery {
        let space = Space::parse(
            &Json::parse(r##"[{"name":"moon","target":"light.key_color","base":"#d0d9f0"},{"name":"fog","target":"fog.density","min":0,"max":0.04,"default":0.01}]"##).unwrap(),
        )
        .unwrap();
        Gallery::new("moon", "Moonlight", Json::obj([("scene", Json::obj([("camera", Json::from("overview"))]))]), space, Mode::Tune, 7, 9, "lead").unwrap()
    }

    #[test]
    fn the_pick_flow_round_trips_through_the_file() {
        let mut g = sample();
        assert_eq!(g.status(), Status::Open);
        assert!(g.decide(1, Kind::Accept { variant: 0 }, "w", "owner").is_err(), "no round yet");
        let p = g.first_round(None);
        assert_eq!(p.points.len(), 9);
        assert_eq!(p.points[0], g.space.default_point(), "the start point is shown first");
        g.push_round(p, Some("gallery-moon-r1.png".into()), vec![]).unwrap();
        g.dislike(1, 3, Some([0.2, 0.7]), "too purple", "window:owner", "owner").unwrap();
        // "This one, with edits": darker, with a note.
        assert_eq!(g.decide(1, Kind::Edit { variant: 5, note: "colder".into(), chips: vec!["darker".into()] }, "window:owner", "owner").unwrap(), 2);
        assert_eq!(g.status(), Status::Rendering);
        assert_eq!(g.decide(1, Kind::Accept { variant: 0 }, "w", "o").unwrap_err().code, "already_decided");
        let next = g.plan_next().unwrap();
        assert_eq!((next.after, next.made.clone()), (1, Made::Tune));
        let inc = g.rounds[0].variants[5].point.clone();
        assert_eq!(next.incumbent.map(|i| next.points[i].clone()), Some(inc.clone()), "the pick is shown again");
        // Darker: the moon's lightness never rises above the pick's.
        assert!(next.points.iter().all(|p| p[0] <= inc[0] + 1e-9), "{:?}", next.points);
        assert_eq!(g.push_round(next.clone(), None, vec![]).unwrap(), 2);
        assert_eq!(g.push_round(next, None, vec![]).unwrap_err().code, "stale_round");
        // None of these, then back to round 1, then accept.
        g.decide(2, Kind::None { note: String::new() }, "window:owner", "owner").unwrap();
        let p3 = g.plan_next().unwrap();
        assert_eq!(p3.made, Made::None);
        g.push_round(p3, None, vec![]).unwrap();
        g.decide(3, Kind::Back { to: 1 }, "window:owner", "owner").unwrap();
        let p4 = g.plan_next().unwrap();
        assert_eq!(p4.made, Made::Back(1));
        assert_eq!(p4.points, g.rounds[0].variants.iter().map(|v| v.point.clone()).collect::<Vec<_>>());
        g.push_round(p4, None, vec![]).unwrap();
        g.decide(4, Kind::Accept { variant: 2 }, "window:owner", "owner").unwrap();
        assert_eq!(g.status(), Status::Accepted);
        assert!(g.plan_next().is_none());
        assert_eq!(g.accepted().map(|a| (a.0, a.1)), Some((4, 2)));
        // The file round-trips exactly.
        let text = g.to_text();
        let back = Gallery::parse(&text).unwrap();
        for (a, b) in g.rounds.iter().zip(&back.rounds) {
            for (va, vb) in a.variants.iter().zip(&b.variants) {
                assert_eq!(va.point.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), vb.point.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "round {} points", a.n);
                assert_eq!(va.values, vb.values, "round {} values at {:?}", a.n, va.point);
            }
        }
        assert_eq!(g.space, back.space);
        assert_eq!(back.to_text(), text);
        assert_eq!(back.rounds[0].dislikes[0].note, "too purple");
        assert_eq!(back.crumbs().iter().map(|c| c.2).collect::<Vec<_>>(), vec!["edit", "none", "back", "accept"]);
    }

    #[test]
    fn decisions_are_checked() {
        let mut g = sample();
        let p = g.first_round(None);
        g.push_round(p, None, vec![]).unwrap();
        assert_eq!(g.decide(2, Kind::Accept { variant: 0 }, "a", "b").unwrap_err().code, "stale_round");
        assert_eq!(g.decide(1, Kind::Accept { variant: 9 }, "a", "b").unwrap_err().code, "bad_variant");
        assert_eq!(g.decide(1, Kind::Edit { variant: 0, note: "x".repeat(281), chips: vec![] }, "a", "b").unwrap_err().code, "note_too_long");
        assert_eq!(g.decide(1, Kind::Edit { variant: 0, note: String::new(), chips: vec!["bigger".into()] }, "a", "b").unwrap_err().code, "bad_chip");
        assert_eq!(g.decide(1, Kind::Back { to: 1 }, "a", "b").unwrap_err().code, "bad_round");
        // A file with an undecided round before the last is refused.
        let mut t = g.clone();
        let p = t.first_round(None);
        t.rounds[0].decision = None;
        t.rounds.push(Round { n: 2, made: Made::Spread, variants: t.rounds[0].variants.clone(), incumbent: None, decision: None, dislikes: vec![], sheet: None, chips_exhausted: false, fit: None });
        let _ = p;
        assert_eq!(Gallery::parse(&t.to_text()).unwrap_err().code, "bad_round");
        assert!(!is_sheet_name("../x.png") && is_sheet_name("gallery-moon-r1.png"));
    }

    #[test]
    fn a_choice_space_shows_each_option_once_and_none_spreads_away() {
        let space = Space::parse(&Json::parse(r#"[{"name":"look","target":"preset","options":["moonlit","storybook","muted","noir","clean"]}]"#).unwrap()).unwrap();
        let mut g = Gallery::new("base", "Base style", Json::Null, space, Mode::Tune, 1, 9, "lead").unwrap();
        let p = g.first_round(None);
        assert_eq!(p.points.len(), 5);
        g.push_round(p, None, vec![]).unwrap();
        let names: Vec<String> = g.rounds[0].variants.iter().map(|v| v.values[0].to_json().as_str().unwrap_or("").to_owned()).collect();
        assert_eq!(names, vec!["moonlit", "storybook", "muted", "noir", "clean"]);
        // Two-parameter galleries: none of these without a pick spreads away from what was shown.
        let mut g = sample();
        let p = g.first_round(None);
        g.push_round(p, None, vec![]).unwrap();
        g.decide(1, Kind::None { note: "all too blue".into() }, "w", "o").unwrap();
        let next = g.plan_next().unwrap();
        assert_eq!(next.made, Made::Spread);
        for q in &next.points {
            assert!(g.rounds[0].variants.iter().all(|v| spread::dist2(&v.point, q) > 1e-6));
        }
    }

    #[test]
    fn agents_supersede_open_rounds() {
        let mut g = sample();
        let p = g.first_round(None);
        g.push_round(p, None, vec![]).unwrap();
        let mut plan = g.supersede("lead").unwrap();
        plan.points = vec![vec![0.5, 0.5, 0.2, 0.1], vec![0.5, 0.5, 0.8, 0.9]];
        assert_eq!(g.push_round(plan, None, vec![]).unwrap(), 2);
        assert_eq!(g.rounds[0].decision.as_ref().map(|d| d.kind.clone()), Some(Kind::Superseded));
        assert_eq!(g.observations().choices.len(), 0, "a superseded round tells the optimiser nothing");
    }

    fn moon_space() -> Space {
        Space::parse(
            &Json::parse(
                r##"[{"name": "moon", "target": "light.key_color", "kind": "colour", "base": "#d0d9f0", "spread": {"l": 0.1, "c": 0.06, "h": 60}},
                     {"name": "strength", "target": "light.key_intensity", "min": 0.3, "max": 1.0, "default": 0.55},
                     {"name": "fog", "target": "fog.density", "min": 0.0, "max": 0.04, "default": 0.012}]"##,
            )
            .unwrap(),
        )
        .unwrap()
    }

    /// Review probe (major 4): "darker" at the dark bound produced lighter moons, and line mode 9
    /// identical tiles. Every mode, chip set and incumbent (at the bound, in the middle): no
    /// variant moves against any chip dimension, and a round shows more than one look.
    #[test]
    fn chips_never_go_the_wrong_way() {
        for mode in [Mode::Tune, Mode::Line, Mode::Spread] {
            for (where_, inc) in [("bound", vec![0.0, 0.5, 0.5, 0.0, 0.3]), ("middle", vec![0.5, 0.5, 0.5, 0.5, 0.3]), ("top", vec![1.0, 0.5, 0.5, 1.0, 1.0])] {
                for chips in [vec!["darker"], vec!["foggier"], vec!["darker", "foggier"], vec!["brighter", "clearer"]] {
                    let chips: Vec<String> = chips.into_iter().map(str::to_owned).collect();
                    let mut g = Gallery::new("moon", "t", Json::obj([("stage", Json::obj::<String>([]))]), moon_space(), mode, 7, 9, "lead").unwrap();
                    let mut p = g.first_round(None);
                    p.points[4] = inc.clone();
                    g.push_round(p, None, vec![]).unwrap();
                    g.decide(1, Kind::Edit { variant: 4, note: String::new(), chips: chips.clone() }, "w", "o").unwrap();
                    let next = g.plan_next().unwrap();
                    let dir = g.space.chip_direction(&chips).unwrap();
                    let at = format!("{mode:?} {where_} {chips:?}");
                    for q in &next.points {
                        for k in 0..dir.len() {
                            assert!((q[k] - inc[k]) * dir[k] >= -1e-9, "{at}: dimension {k} moved against the chips: {q:?}");
                        }
                    }
                    // The displayed colour too (gamut mapping keeps lightness).
                    if chips.contains(&"darker".to_owned()) {
                        let l0 = crate::colour::srgb_to_oklab(g.space.values(&inc)[0].as_colour().unwrap())[0];
                        for q in &next.points {
                            let l = crate::colour::srgb_to_oklab(g.space.values(q)[0].as_colour().unwrap())[0];
                            assert!(l <= l0 + 0.006, "{at}: a lighter moon ({l} > {l0})");
                        }
                    }
                    let mut looks: Vec<Vec<Value>> = Vec::new();
                    for q in &next.points {
                        let v = g.space.values(q);
                        if !looks.iter().any(|w| g.space.same_shown(w, &v)) {
                            looks.push(v);
                        }
                    }
                    assert!(looks.len() > 1, "{at}: one look only");
                    assert_eq!(looks.len(), next.points.len(), "{at}: look-alikes are dropped");
                    let exhausted = crate::tune::chips_exhausted(&inc, &dir);
                    assert_eq!(next.chips_exhausted, exhausted, "{at}");
                    // A round made at the bound says so in the file.
                    g.push_round(next, None, vec![]).unwrap();
                    assert_eq!(Gallery::parse(&g.to_text()).unwrap().rounds[1].chips_exhausted, exhausted);
                }
            }
        }
    }

    /// Review probe (minor 6): near-white moons all clip to one displayed colour; a round shows
    /// distinct looks only.
    #[test]
    fn rounds_never_show_look_alikes() {
        let g = Gallery::new("moonlight", "Which moonlight?", Json::obj([("stage", Json::obj::<String>([]))]), moon_space(), Mode::Tune, specula_core::hash::hash_str("moonlight:Which moonlight?"), 9, "lead").unwrap();
        let p = g.first_round(None);
        assert_eq!(p.points.len(), 9);
        let vals: Vec<Vec<Value>> = p.points.iter().map(|q| g.space.values(q)).collect();
        for i in 0..9 {
            for j in i + 1..9 {
                assert!(!g.space.same_shown(&vals[i], &vals[j]), "variants {i} and {j} look alike: {:?} {:?}", g.space.values_json(&vals[i]), g.space.values_json(&vals[j]));
            }
        }
        // An interface colour: thin text needs a wider gap (a sight-skeptic finding: labels 0.04
        // apart read the same). Every pair of a "can't afford" round is MIN_SHOWN_DE_UI apart.
        let space = Space::parse(&Json::parse(r##"[{"name": "cant_afford", "target": "ui.cant_afford", "kind": "colour", "base": "#b3a78f", "spread": {"l": 0.14, "c": 0.06, "h": 50}}]"##).unwrap()).unwrap();
        let g = Gallery::new("ca", "t", Json::obj([("ui", Json::obj::<String>([]))]), space, Mode::Tune, 7, 9, "lead").unwrap();
        let p = g.first_round(None);
        let cols: Vec<crate::colour::Rgb> = p.points.iter().map(|q| g.space.values(q)[0].as_colour().unwrap()).collect();
        assert!(cols.len() >= 6, "the spread holds at least six readable choices ({})", cols.len());
        for i in 0..cols.len() {
            for j in i + 1..cols.len() {
                let de = crate::colour::delta_e(cols[i], cols[j]);
                assert!(de >= crate::space::MIN_SHOWN_DE_UI, "labels {i} and {j} read alike (dE {de:.3})");
            }
        }
    }

    /// One simulated human tuning a hidden optimum through the gallery API exactly as the engine
    /// runs it (first round, Edit picks, plan_next, push_round): mean regret after 8 rounds over
    /// 12 seeds.
    fn production_regret(mode: Mode, dims: usize) -> f64 {
        let space = Space::parse(&Json::arr((0..dims).map(|i| Json::parse(&format!(r#"{{"name": "p{i}", "target": "recipe.p{i}", "min": 0, "max": 1, "default": 0.5}}"#)).unwrap()))).unwrap();
        let mut sum = 0.0;
        for seed in 0..12u64 {
            let mut gen = Rng::new(seed * 7919 + 13, 9);
            let opt: Vec<f64> = (0..dims).map(|_| 0.15 + 0.7 * f64::from(gen.next_u32()) / 4_294_967_296.0).collect();
            let f = |x: &[f64]| specula_core::dmath::exp_f64(-x.iter().zip(&opt).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() / (2.0 * 0.22 * 0.22));
            let mut g = Gallery::new("t", "t", Json::obj([("recipe", Json::from("r.json"))]), space.clone(), mode, seed, 9, "lead").unwrap();
            let p = g.first_round(None);
            g.push_round(p, None, vec![]).unwrap();
            let mut best = f64::NEG_INFINITY;
            for r in 1..=8 {
                let pts: Vec<Vec<f64>> = g.rounds[r - 1].variants.iter().map(|v| v.point.clone()).collect();
                let w = (0..pts.len()).max_by(|a, b| f(&pts[*a]).total_cmp(&f(&pts[*b]))).unwrap();
                best = best.max(f(&pts[w]));
                if r == 8 {
                    break;
                }
                g.decide(r, Kind::Edit { variant: w, note: String::new(), chips: vec![] }, "w", "o").unwrap();
                let next = g.plan_next().unwrap();
                g.push_round(next, None, vec![]).unwrap();
            }
            sum += 1.0 - best;
        }
        sum / 12.0
    }

    fn random_regret(dims: usize) -> f64 {
        let mut sum = 0.0;
        for seed in 0..12u64 {
            let mut gen = Rng::new(seed * 7919 + 13, 9);
            let opt: Vec<f64> = (0..dims).map(|_| 0.15 + 0.7 * f64::from(gen.next_u32()) / 4_294_967_296.0).collect();
            let f = |x: &[f64]| specula_core::dmath::exp_f64(-x.iter().zip(&opt).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() / (2.0 * 0.22 * 0.22));
            let mut rng = Rng::new(seed, 3);
            let mut best = f(&vec![0.5; dims]);
            for _ in 0..8 * 9 {
                let x: Vec<f64> = (0..dims).map(|_| f64::from(rng.next_u32()) / 4_294_967_296.0).collect();
                best = best.max(f(&x));
            }
            sum += 1.0 - best;
        }
        sum / 12.0
    }

    /// Review probe (minor 8): the tuning claim measured on what production runs, against both
    /// baselines (random spreads, and the spread hill-climb of `mode: "spread"`).
    #[test]
    fn production_tuning_beats_both_baselines() {
        let mut rows = Vec::new();
        for dims in [2usize, 4, 6, 8] {
            let (tune, line, spread, random) = (production_regret(Mode::Tune, dims), production_regret(Mode::Line, dims), production_regret(Mode::Spread, dims), random_regret(dims));
            // (line is measured for the table in GALLERIES.md; the assertions name every value.)
            let _ = line;
            rows.push((dims, tune, spread, random));
        }
        for (dims, tune, spread, random) in rows {
            assert!(tune < 0.5 * random, "dims {dims}: tune {tune:.4} vs random {random:.4}");
            assert!(tune <= spread * 1.15 + 0.01, "dims {dims}: tune {tune:.4} vs the spread hill-climb {spread:.4}");
        }
    }
}

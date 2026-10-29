//! `anim_check` and `anim_filmstrip` as step jobs (docs/design/WORKERS.md, C2 and C3): a check
//! (one clip, state or transition) or a filmstrip tile (one render and readback) per step, between
//! frames within the step jobs' share of the engine thread, and the sheet's encode and writes on a
//! worker. In one call, 256 clips took 1.2 s and a 12-row filmstrip 322 ms (class C, 2026-10-29).
//!
//! A child of `anim_tools` (it shares its helpers). The world may change between steps: an entity
//! filmstrip whose entity is deleted stops (`changed_during_job`); a recipe filmstrip holds its
//! build pinned on the GPU until it ends.

use super::*;
use crate::capture::Answer;
use crate::steps::{Step, StepJob, StepStart};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

/// One step's CPU time for checks (one subject at least).
const CHECK_STEP: Duration = Duration::from_millis(1);
/// Keys of filmstrips' GPU pins (apart from the studio's call ids).
static PIN_KEYS: AtomicU64 = AtomicU64::new(1 << 62);

/// What `anim_check` checks, in order: clips, then states, then transitions.
enum Subject {
    Clip(String),
    State(String, Vec<(String, Value)>),
    Transition(Vec<(String, Value)>, Vec<(String, Value)>, f32),
}

pub(super) struct CheckJob {
    source: String,
    /// The entity checked (its path), when it was an entity.
    entity: Option<String>,
    src: Source,
    rules: MotionRules,
    rig: String,
    graph: String,
    scale: f32,
    todo: VecDeque<Subject>,
    /// Subjects: (clips, states, transitions).
    counts: (usize, usize, usize),
    report: Report,
    max_checks: usize,
}

impl CheckJob {
    /// What to check (refusals as the answer, before any check runs).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(e: &mut Engine, source: Option<String>, key: Option<EntityKey>, graph_name: &str, clips: Option<Vec<String>>, states: Option<Vec<StateSubject>>, transitions: Vec<TransitionSubject>, scale: Option<f32>, max_checks: usize) -> Result<CheckJob, ToolOutput> {
        // An entity: what it plays now, at its own scale.
        let (source, entity) = match (source, key) {
            (Some(s), _) => (s, None),
            (None, Some(k)) => {
                let id = e.world.resolve(&k).map_err(|err| world_error(&err))?;
                let ent = e.world.get(id).expect("resolved").clone();
                let Some(m) = ent.model.as_ref() else { return Err(error("not_a_model", format!("{} is not a model entity", e.world.path_of(id)))) };
                (m.recipe.clone(), Some(ent))
            }
            (None, None) => return Err(error("invalid_argument", "anim_check takes 'recipe' or the entity")),
        };
        let src = e.anim_source(&source)?;
        let rules = e.motion_rules(&source)?;
        let rig = stem_of(&source);
        let set = Arc::clone(&src.set);
        let mut graph_used = graph_name.to_string();
        let mut state_list: Vec<(String, Vec<(String, Value)>)> = Vec::new();
        let mut clip_list: Vec<String> = Vec::new();
        let mut scale = scale.unwrap_or(1.0);
        match &entity {
            Some(ent) => {
                scale = crate::anim::entity_scale(ent);
                let a = ent.anim.as_ref();
                match a.and_then(|a| a.graph.as_deref()) {
                    Some(gp) => {
                        graph_used = gp.graph.clone();
                        if let Some(graph) = set.graph(&gp.graph) {
                            let mut sc = Scratch::default();
                            let timing = Timing { anchor: a.expect("graph").start.unwrap_or(0.0), speed: a.expect("graph").speed };
                            let now = aplay::state_at(Graphed { set: &set, graph, scale }, gp, timing, e.time_s(), &mut sc);
                            if let Some(top) = now.first().and_then(|l| l.stack.last()) {
                                state_list.push((top.state.clone(), gp.params.clone()));
                            }
                        }
                    }
                    None => {
                        if let Some(a) = a {
                            clip_list.push(a.clip.clone());
                        }
                    }
                }
            }
            None => {
                clip_list = clips.unwrap_or_else(|| set.clips().map(|(_, c)| c.name.clone()).collect());
                if let Some(graph) = set.graph(graph_name) {
                    let subjects = states.unwrap_or_else(|| graph.layers[0].states.iter().map(|s| StateSubject { state: s.name.clone(), params: None }).collect());
                    for s in subjects {
                        let p = params_for(graph, s.params.as_ref(), Some(&s.state))?;
                        state_list.push((s.state, p));
                    }
                } else if states.is_some() || !transitions.is_empty() {
                    return Err(error("unknown_graph", format!("{source} has no graph '{graph_name}' (graphs: {})", set.graphs().map(|(n, _)| n).collect::<Vec<_>>().join(", "))));
                }
            }
        }
        if let Some(c) = clip_list.iter().find(|c| set.id(c).is_none()) {
            return Err(error("unknown_clip", format!("{source} has no clip '{c}'")));
        }
        let mut todo: VecDeque<Subject> = clip_list.iter().cloned().map(Subject::Clip).collect();
        let mut n_transitions = 0;
        if let Some(graph) = set.graph(&graph_used) {
            todo.extend(state_list.iter().cloned().map(|(s, p)| Subject::State(s, p)));
            for t in &transitions {
                let from = params_for(graph, Some(&t.from), None)?;
                let to = params_for(graph, Some(&t.to), None)?;
                todo.push_back(Subject::Transition(from, to, t.settle_s));
                n_transitions += 1;
            }
        }
        let entity = entity.map(|ent| e.world.path_of(ent.id));
        Ok(CheckJob { source, entity, src, rules, rig, graph: graph_used, scale, todo, counts: (clip_list.len(), state_list.len(), n_transitions.max(transitions.len())), report: Report::default(), max_checks })
    }

    /// Checks the next subject; `Ok(true)` once none is left.
    fn next(&mut self) -> Result<bool, ToolOutput> {
        let Some(subject) = self.todo.pop_front() else { return Ok(true) };
        let set = &self.src.set;
        let mesh = self.src.built.model.lods.first();
        match subject {
            Subject::Clip(c) => {
                let id = set.id(&c).ok_or_else(|| error("unknown_clip", format!("{} has no clip '{c}'", self.source)))?;
                self.report.checks.extend(checks::check_clip(set, &self.rig, id, &self.rules, mesh).checks);
            }
            Subject::State(state, params) => {
                let graph = set.graph(&self.graph).ok_or_else(|| error("unknown_graph", format!("{} has no graph '{}'", self.source, self.graph)))?;
                let g = Graphed { set, graph, scale: self.scale };
                let r = checks::check_state(g, &self.rig, &self.graph, &state, &params, 3.0, &self.rules, mesh).map_err(|e| anim_error(&e, "states"))?;
                self.report.notes.extend(r.notes);
                self.report.checks.extend(r.checks);
            }
            Subject::Transition(from, to, settle_s) => {
                let graph = set.graph(&self.graph).ok_or_else(|| error("unknown_graph", format!("{} has no graph '{}'", self.source, self.graph)))?;
                let g = Graphed { set, graph, scale: self.scale };
                self.report.checks.extend(checks::check_transition(g, &self.rig, &self.graph, &from, &to, settle_s, &self.rules, mesh).checks);
            }
        }
        Ok(self.todo.is_empty())
    }

    fn answer(&self) -> ToolOutput {
        let mut out = self.report.to_json(self.max_checks);
        out.set("recipe", Json::from(self.source.as_str()));
        if let Some(p) = &self.entity {
            out.set("entity", Json::from(p.as_str()));
        }
        let (c, s, t) = self.counts;
        out.set("subjects", Json::obj([("clips", Json::from(c)), ("states", Json::from(s)), ("transitions", Json::from(t))]));
        out.set("counts", Json::obj([("checks", Json::from(self.report.checks.len())), ("failed", Json::from(self.report.failures().count()))]));
        out.set("rules", self.rules.to_json());
        if !self.src.problems.is_empty() {
            out.set("asset_problems", Json::arr(self.src.problems.iter().map(|p| Json::from(p.as_str()))));
        }
        ToolOutput::text(out.to_string())
    }

    /// Every check at once (an entity filmstrip checks the one state its entity plays).
    pub(super) fn run(mut self) -> ToolOutput {
        loop {
            match self.next() {
                Ok(true) => return self.answer(),
                Ok(false) => {}
                Err(out) => return out,
            }
        }
    }
}

impl StepJob for CheckJob {
    fn kind(&self) -> &'static str {
        "anim_check"
    }

    fn step(&mut self, _: &mut Engine, _: &jobs::Reply) -> Step {
        let t = Instant::now();
        loop {
            match self.next() {
                Ok(true) => return Step::Done(self.answer()),
                Err(out) => return Step::Done(out),
                Ok(false) if t.elapsed() < CHECK_STEP => {}
                Ok(false) => return Step::More,
            }
        }
    }
}

/// Where a filmstrip's sheet goes when its tiles are done.
struct Sheet {
    tile: u32,
    label_h: u32,
    sw: u32,
    sh: u32,
    pixels: Vec<u8>,
}

impl Sheet {
    fn new(tile: u32, cols: u32, rows: u32) -> Sheet {
        let label_h = 2 * film::CHAR_H + 6;
        let tile = fit_tile(tile, cols, rows, label_h);
        let (sw, sh) = (cols * tile, rows * (tile + label_h));
        let mut pixels = vec![0u8; (sw * sh * 4) as usize];
        film::fill(&mut pixels, sw, 0, 0, sw, sh, [14, 17, 26, 255]);
        Sheet { tile, label_h, sw, sh, pixels }
    }
}

/// A recipe filmstrip: rows of clips, states and transitions on the neutral stage.
pub(super) struct FilmJob {
    source: String,
    graph: String,
    src: Source,
    rules: MotionRules,
    rig: String,
    scale: f32,
    n: usize,
    tile: u32,
    with_checks: bool,
    rows_in: VecDeque<Row>,
    film: Vec<FilmRow>,
    /// Rows posed whose checks are still to run (a step each: a check can take tens of ms).
    checks_todo: VecDeque<(usize, Row)>,
    sheet: Option<Sheet>,
    /// Each row's framing (centre, radius) and the closest one's radius.
    fits: Vec<([f32; 3], f32)>,
    closest: f32,
    /// The tile being rendered: (row, frame).
    at: (usize, usize),
    tiles: Vec<Json>,
    lumas: Vec<Json>,
    out_rows: Vec<Json>,
    dark: Vec<String>,
    model: Vec<specula_anim::math::Affine>,
    skin: Vec<specula_anim::math::Affine>,
    session: SessionInfo,
    call: u64,
    pin: u64,
}

const FOV: f32 = 30.0;

impl FilmJob {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(e: &mut Engine, source: &str, graph_name: &str, rows: Vec<Row>, frames: u32, tile: u32, scale: Option<f32>, with_checks: bool, session: &SessionInfo, call: u64) -> Result<FilmJob, ToolOutput> {
        let src = e.anim_source(source)?;
        let rules = e.motion_rules(source)?;
        let rows: Vec<Row> = if rows.is_empty() { src.set.clips().take(MAX_ROWS).map(|(_, c)| Row::Clip(c.name.clone())).collect() } else { rows };
        let n = frames.max(2) as usize;
        if rows.len() * n > MAX_RENDERS {
            return Err(error("too_many_tiles", format!("{} rows of {n} frames is {} tiles; at most {MAX_RENDERS} per call (fewer rows or frames)", rows.len(), rows.len() * n)));
        }
        let pin = PIN_KEYS.fetch_add(1, Ordering::Relaxed);
        e.pin_for_job(pin, &session.id, src.built.id);
        Ok(FilmJob {
            source: source.to_owned(),
            graph: graph_name.to_owned(),
            rig: stem_of(source),
            src,
            rules,
            scale: scale.unwrap_or(1.0),
            n,
            tile,
            with_checks,
            rows_in: rows.into(),
            film: Vec::new(),
            checks_todo: VecDeque::new(),
            sheet: None,
            fits: Vec::new(),
            closest: f32::MAX,
            at: (0, 0),
            tiles: Vec::new(),
            lumas: Vec::new(),
            out_rows: Vec::new(),
            dark: Vec::new(),
            model: Vec::new(),
            skin: Vec::new(),
            session: session.clone(),
            call,
            pin,
        })
    }

    /// A row's motion checks (a step of their own: [`FilmJob::pose_row`] validated the row).
    fn check_row(&self, row: &Row) -> Report {
        let (set, graph_name, rig, rules) = (&self.src.set, &self.graph, &self.rig, &self.rules);
        let mesh = self.src.built.model.lods.first();
        match row {
            Row::Clip(name) => set.id(name).map(|id| checks::check_clip(set, rig, id, rules, mesh)).unwrap_or_default(),
            Row::State(st) => {
                let Some(graph) = set.graph(graph_name) else { return Report::default() };
                let Ok(params) = params_for(graph, st.params.as_ref(), Some(&st.state)) else { return Report::default() };
                let g = Graphed { set, graph, scale: self.scale };
                checks::check_state(g, rig, graph_name, &st.state, &params, 3.0, rules, mesh).unwrap_or_default()
            }
            Row::Transition(tr) => {
                let Some(graph) = set.graph(graph_name) else { return Report::default() };
                let (Ok(from), Ok(to)) = (params_for(graph, Some(&tr.from), None), params_for(graph, Some(&tr.to), None)) else { return Report::default() };
                let g = Graphed { set, graph, scale: self.scale };
                checks::check_transition(g, rig, graph_name, &from, &to, tr.settle_s, rules, mesh)
            }
        }
    }

    /// A row's poses (CPU), so a bad row fails before any rendering (its checks come later).
    fn pose_row(&self, row: &Row) -> Result<FilmRow, ToolOutput> {
        let (set, n, source, graph_name, rig) = (&self.src.set, self.n, &self.source, &self.graph, &self.rig);
        Ok(match row {
            Row::Clip(name) => {
                let Some(id) = set.id(name) else { return Err(error("unknown_clip", format!("{source} has no clip '{name}'"))) };
                let c = set.clip(id);
                let span = if c.looping == LoopMode::Loop { n as f32 } else { (n - 1) as f32 };
                let times: Vec<f64> = (0..n).map(|k| (c.duration * k as f32 / span) as f64).collect();
                let poses = times
                    .iter()
                    .map(|&t| {
                        let mut p = set.skeleton.rest_pose();
                        blend::sample(&set.skeleton, c, set.track_bones(id), t as f32, blend::Base::Rest, &mut p);
                        p
                    })
                    .collect();
                let report = None;
                let lift = vec![0.0; times.len()];
                FilmRow { label: format!("{} {:.2}S {}", c.name, c.duration, c.looping.name()), subject: format!("{rig}/{}", c.name), times, poses, report, lift }
            }
            Row::State(s) => {
                let Some(graph) = set.graph(graph_name) else { return Err(error("unknown_graph", format!("{source} has no graph '{graph_name}'"))) };
                let params = params_for(graph, s.params.as_ref(), Some(&s.state))?;
                if graph.layers[0].state(&s.state).is_none() {
                    return Err(error("unknown_state", format!("graph '{graph_name}' has no state '{}'", s.state)));
                }
                let g = Graphed { set, graph, scale: self.scale };
                let play = held_play(graph_name, &s.state, &params);
                let timing = Timing { anchor: 0.0, speed: 1.0 };
                let mut sc = Scratch::default();
                let rate = aplay::state_at(g, &play, timing, 0.0, &mut sc)[0].stack.last().map_or(1.0, |e| e.rate).max(1e-3);
                let cycle = (1.0 / rate).min(10.0);
                let times: Vec<f64> = (0..n).map(|k| cycle * k as f64 / n as f64).collect();
                let mut player = GraphPlayer::new();
                let poses = times.iter().map(|&t| player.evaluate(g, &play, timing, t, &Surroundings::default()).clone()).collect();
                let report = None;
                let shown: Vec<String> = params.iter().map(|(n, v)| format!("{n}={}", short(v))).collect();
                let lift: Vec<f32> = times.iter().map(|&t| aplay::root_motion(g, &play, timing, 0.0, t, &mut sc)[1]).collect();
                FilmRow { label: format!("{}.{} {}", graph_name, s.state, shown.join(" ").replace('"', "")), subject: format!("{rig}/{graph_name}.{}", s.state), times, poses, report, lift }
            }
            Row::Transition(tr) => {
                let Some(graph) = set.graph(graph_name) else { return Err(error("unknown_graph", format!("{source} has no graph '{graph_name}'"))) };
                let from = params_for(graph, Some(&tr.from), None)?;
                let to = params_for(graph, Some(&tr.to), None)?;
                let g = Graphed { set, graph, scale: self.scale };
                let mut before = GraphPlay::new(graph_name);
                for (k, v) in &from {
                    before.set_param(k, v.clone());
                }
                let t0 = Timing { anchor: 0.0, speed: 1.0 };
                let ts = tr.settle_s as f64;
                let mut sc = Scratch::default();
                let (mut after, _) = aplay::settle(g, &before, t0, ts, &mut sc);
                for (k, v) in &to {
                    after.set_param(k, v.clone());
                }
                let t1 = Timing { anchor: ts, speed: 1.0 };
                let blend_s = graph.layers.iter().flat_map(|l| l.transitions.iter().map(|t| t.blend)).fold(0.0f32, f32::max).min(2.0) as f64;
                let (w0, w1) = (ts - 0.15, ts + blend_s + 0.25);
                let times: Vec<f64> = (0..n).map(|k| w0 + (w1 - w0) * k as f64 / (n - 1) as f64).collect();
                let mut player = GraphPlayer::new();
                let poses = times
                    .iter()
                    .map(|&t| if t < ts { player.evaluate(g, &before, t0, t, &Surroundings::default()).clone() } else { player.evaluate(g, &after, t1, t, &Surroundings::default()).clone() })
                    .collect();
                let report = None;
                let name_of = |p: &GraphPlay, tm: Timing, t: f64, sc: &mut Scratch| aplay::state_at(g, p, tm, t, sc)[0].stack.last().map(|e| e.state.clone()).unwrap_or_default();
                let (a, b) = (name_of(&before, t0, ts, &mut sc), name_of(&after, t1, w1, &mut sc));
                FilmRow { label: format!("{graph_name}.{a}->{b}, switched at {:.2}S", ts), subject: format!("{rig}/{graph_name}.{a}->{b}"), times, poses, report, lift: vec![0.0; n] }
            }
        })
    }

    /// Every row's poses fitted (side view, from the creature's left). Each row is framed on its
    /// own (a death lying 7 m long would shrink every other row to a speck); a row framed wider
    /// than the closest one is labelled with its scale, so sizes still compare.
    fn lay_out(&mut self) {
        let skel = &self.src.set.skeleton;
        for row in &self.film {
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for (k, p) in row.poses.iter().enumerate() {
                skel.model_space(p, &mut self.model);
                for (b, m) in self.model.iter().enumerate() {
                    let tail = m.apply_point(specula_anim::math::math3::sub(skel.tails[b], skel.heads[b]));
                    for q in [m.t, tail] {
                        let q = [q[0], q[1] + row.lift[k], q[2]];
                        for i in 0..3 {
                            lo[i] = lo[i].min(q[i]);
                            hi[i] = hi[i].max(q[i]);
                        }
                    }
                }
            }
            lo[1] = lo[1].min(0.0);
            let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
            // Joints and bone ends lie inside the mesh: a margin for its skin.
            let radius = 0.58 * specula_anim::math::math3::length([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) + 0.1;
            self.fits.push((centre, radius));
        }
        self.closest = self.fits.iter().map(|f| f.1).fold(f32::MAX, f32::min);
        self.sheet = Some(Sheet::new(self.tile, self.n as u32, self.film.len() as u32));
    }

    /// Renders tile `self.at` into the sheet; after a row's last tile, its labels and facts.
    fn render_tile(&mut self, e: &mut Engine) -> Result<(), ToolOutput> {
        let (ri, k) = self.at;
        let sheet = self.sheet.as_mut().expect("laid out");
        let (tile, label_h, sw) = (sheet.tile, sheet.label_h, sheet.sw);
        let row = &self.film[ri];
        let y0 = ri as u32 * (tile + label_h);
        let (centre, radius) = self.fits[ri];
        let eye = film::frame_eye(centre, radius, 90.0, 12.0, FOV);
        let skel = &self.src.set.skeleton;
        skel.model_space(&row.poses[k], &mut self.model);
        skel.skin_matrices(&self.model, &mut self.skin);
        let mut draws = Vec::new();
        let mut bones = Vec::new();
        if let Some(gm) = e.meshes.get(Shape::Quad) {
            let g = Mat4::from_trs(Vec3::ZERO, specula_core::math::Quat::IDENTITY, Vec3::new(40.0, 1.0, 40.0));
            let mut d = DrawItem::new(gm, g.to_cols_array(), [0x30 as f32 / 255.0, 0x35 as f32 / 255.0, 0x3d as f32 / 255.0, 1.0]);
            d.surface = specula_gpu::Surface { roughness: 0.9, specular: 0.05, ..specula_gpu::Surface::default() };
            draws.push(d);
        }
        let place = Mat4::from_trs(Vec3::new(0.0, row.lift[k], 0.0), specula_core::math::Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
        e.models.posed_draws(&mut e.gpu, &self.src.built, &self.skin, &place, &mut bones, &mut draws).map_err(|m| error("render_unavailable", m))?;
        let x0 = k as u32 * tile;
        e.film_tile(Vec3::new(eye[0], eye[1], eye[2]), Vec3::new(centre[0], centre[1], centre[2]), FOV, &draws, &bones, tile, &mut sheet.pixels, sw, x0, y0).map_err(ToolOutput::error)?;
        let l = film::mean_luma(&sheet.pixels, sw, x0 + tile / 4, y0 + tile / 4, tile / 2, tile / 2);
        if l < DARK_LUMA {
            self.dark.push(format!("{} frame {k}", row.subject));
        }
        self.lumas.push(round(l as f64, 3));
        film::draw_text(&mut sheet.pixels, sw, x0 + 3, y0 + 3, &format!("{:.2}", row.times[k]), [200, 200, 190]);
        self.tiles.push(Json::arr([x0, y0, tile, tile].map(Json::from)));
        if k + 1 < row.poses.len() {
            self.at = (ri, k + 1);
            return Ok(());
        }
        // Labels: the subject, and the checks that failed (or "checks ok", with what the rules
        // exempt it from).
        let scale = self.closest / radius;
        let mut label = if scale < 0.995 { format!("{}  SCALE {scale:.2}X", row.label) } else { row.label.clone() };
        let rise = row.lift.iter().fold(0.0f32, |m, l| m.max(l.abs()));
        if rise > 0.01 {
            label.push_str(&format!("  ROOT RISE {rise:.2}M SHOWN"));
        }
        film::draw_text(&mut sheet.pixels, sw, 4, y0 + tile + 2, &label, [230, 226, 214]);
        let verdict = row.report.as_ref().map(verdict_of).unwrap_or_default();
        let colour = if row.report.as_ref().is_some_and(|r| !r.passed()) { [255, 120, 100] } else { [140, 220, 160] };
        film::draw_text(&mut sheet.pixels, sw, 4, y0 + tile + 4 + film::CHAR_H, &verdict, colour);
        let mut j = Json::obj([
            ("subject", Json::from(row.subject.as_str())),
            ("label", Json::from(row.label.as_str())),
            ("verdict", Json::from(verdict.as_str())),
            ("times_s", Json::arr(row.times.iter().map(|t| round(*t, 4)))),
            ("tiles", Json::Arr(std::mem::take(&mut self.tiles))),
            ("luma", Json::Arr(std::mem::take(&mut self.lumas))),
            ("framed_radius_m", round(radius as f64, 3)),
            ("scale", round(scale as f64, 3)),
            ("root_rise_m", Json::arr(row.lift.iter().map(|l| round(*l as f64, 3)))),
        ]);
        if let Some(r) = &row.report {
            j.set("checks", r.to_json(12));
        }
        self.out_rows.push(j);
        self.at = (ri + 1, 0);
        Ok(())
    }

    fn finish(&mut self, e: &mut Engine, reply: &jobs::Reply) -> Step {
        e.studio_done(self.pin);
        let failed: usize = self.film.iter().filter_map(|r| r.report.as_ref()).map(|r| r.failures().count()).sum();
        let sheet = self.sheet.take().expect("laid out");
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("anim_filmstrip")),
            ("recipe", Json::from(self.source.as_str())),
            ("view", Json::from("side (from the model's left, +X), 12 deg up, neutral stage with a ground plane at y = 0")),
            ("scale", Json::from("each row framed on its own; rows[].scale (and the label, when under 1) is its size against the closest-framed row")),
            ("frames", Json::from(self.n)),
            ("tile", Json::from(sheet.tile)),
            ("rows", Json::Arr(std::mem::take(&mut self.out_rows))),
            ("checks_failed", Json::from(failed)),
            ("rules", self.rules.to_json()),
        ]);
        if !self.dark.is_empty() {
            packet.set("warnings", Json::arr([Json::from(format!("too dark to judge (mean luma under {DARK_LUMA}): {}", self.dark.iter().take(8).cloned().collect::<Vec<_>>().join(", ")))]));
        }
        e.film_finish(&self.session, Answer { reply: reply.clone(), call: self.call }, sheet, packet);
        Step::Handed
    }
}

impl StepJob for FilmJob {
    fn kind(&self) -> &'static str {
        "anim_filmstrip"
    }

    fn step(&mut self, e: &mut Engine, reply: &jobs::Reply) -> Step {
        if let Some(row) = self.rows_in.pop_front() {
            return match self.pose_row(&row) {
                Ok(r) => {
                    self.film.push(r);
                    if self.with_checks {
                        self.checks_todo.push_back((self.film.len() - 1, row));
                    }
                    Step::More
                }
                Err(out) => {
                    e.studio_done(self.pin);
                    Step::Done(out)
                }
            };
        }
        if let Some((i, row)) = self.checks_todo.pop_front() {
            let report = self.check_row(&row);
            self.film[i].report = Some(report);
            return Step::More;
        }
        if self.sheet.is_none() {
            self.lay_out();
            return Step::More;
        }
        if self.at.0 < self.film.len() {
            return match self.render_tile(e) {
                Ok(()) => Step::More,
                Err(out) => {
                    e.studio_done(self.pin);
                    Step::Done(out)
                }
            };
        }
        self.finish(e, reply)
    }
}

/// An entity filmstrip: its play over time in the world, from three views.
pub(super) struct EntityFilmJob {
    id: u64,
    path: String,
    entity: crate::world::Entity,
    times: Vec<f64>,
    t0: f64,
    window_s: f32,
    with_checks: bool,
    views: [(f32, f32, &'static str, Vec3, f32); 3],
    yaw: f32,
    radius: f32,
    sheet: Sheet,
    at: (usize, usize),
    tiles: Vec<Json>,
    lumas: Vec<Json>,
    rows: Vec<Json>,
    dark: Vec<String>,
    session: SessionInfo,
    call: u64,
}

impl EntityFilmJob {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(e: &mut Engine, key: &EntityKey, frames: u32, tile: u32, window_s: f32, with_checks: bool, session: &SessionInfo, call: u64) -> Result<EntityFilmJob, ToolOutput> {
        let id = e.world.resolve(key).map_err(|err| world_error(&err))?;
        let path = e.world.path_of(id);
        let Some(ent) = e.world.get(id).cloned() else { return Err(error("not_found", "gone")) };
        if ent.anim.is_none() {
            return Err(error("not_animated", format!("{path} has no anim to film")));
        }
        // The game brings its presentation up to date first (as for captures).
        let _ = e.before_capture();
        let n = frames.max(2) as usize;
        let t0 = e.time_s();
        let times: Vec<f64> = (0..n).map(|k| t0 + window_s as f64 * k as f64 / (n - 1) as f64).collect();
        let (lo, hi) = capture::subtree_bounds(&e.world, id).unwrap_or((ent.world_position(), ent.world_position()));
        let centre = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(0.5) * 1.15;
        // The feet: the lowest third of the body, from the front and above knee-high clutter
        // (fences, rails), so a foot on a step and one below it both show.
        let feet_h = (hi.y - lo.y) * 0.34;
        let feet_centre = Vec3::new(centre.x, lo.y + feet_h * 0.5, centre.z);
        let feet_radius = ((hi.x - lo.x).max(hi.z - lo.z) * 0.5).max(feet_h).max(0.45) * 1.25;
        // Along its way (lanes run between raised blocks), high enough to see over them; then the
        // feet. Each view is lit from the camera too (the world's night leaves a creature's front
        // in the dark).
        let views = [(35.0f32, 24.0f32, "front quarter", centre, radius), (-145.0, 24.0, "back quarter", centre, radius), (8.0, 30.0, "feet (front)", feet_centre, feet_radius)];
        // The entity's own facing: views turn with it.
        let yaw = {
            let m = ent.model();
            let f = m.transform_point(Vec3::new(0.0, 0.0, 1.0)) - m.transform_point(Vec3::ZERO);
            f.x.atan2(f.z).to_degrees()
        };
        Ok(EntityFilmJob {
            id,
            path,
            entity: ent,
            times,
            t0,
            window_s,
            with_checks,
            views,
            yaw,
            radius,
            sheet: Sheet::new(tile, n as u32, 3),
            at: (0, 0),
            tiles: Vec::new(),
            lumas: Vec::new(),
            rows: Vec::new(),
            dark: Vec::new(),
            session: session.clone(),
            call,
        })
    }

    fn render_tile(&mut self, e: &mut Engine) -> Result<(), ToolOutput> {
        if e.world.get(self.id).is_none() {
            return Err(error("changed_during_job", format!("{} was deleted while it was filmed", self.path)));
        }
        let (vi, k) = self.at;
        let (az, el, name, c, r) = self.views[vi];
        let (tile, label_h, sw) = (self.sheet.tile, self.sheet.label_h, self.sheet.sw);
        let y0 = vi as u32 * (tile + label_h);
        let eye = film::frame_eye([c.x, c.y, c.z], r, az + self.yaw, el, 35.0);
        let eye = Vec3::new(eye[0], eye[1], eye[2]);
        let dist = (eye - c).length();
        let fill = specula_gpu::PointLight { position: [eye.x, eye.y, eye.z], color: [1.0, 0.95, 0.88], intensity: 0.7 * dist * dist, range: dist * 2.5 };
        let camera = Camera::new(eye, c, 35.0);
        let overlays = sight_overlay::Spec::default();
        let t = self.times[k];
        let opts = capture::ViewOpts { include_ui: false, lod: Some(0), time_s: t, overlays: &overlays, caption: None, ids: false, fill: Some(fill) };
        let rend = e.render_view(camera, tile, tile, &opts).map_err(ToolOutput::error)?;
        let x0 = k as u32 * tile;
        let px = &mut self.sheet.pixels;
        film::blit(px, sw, x0, y0, &rend.rgba, tile, tile);
        // How bright the subject came out (its screen box, else the tile's middle).
        let bx = rend.subjects.iter().find(|s| s.id == self.id).and_then(|s| s.json.get("screen")).and_then(Json::as_array).filter(|a| a.len() == 4).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0).max(0.0) as u32).collect::<Vec<_>>());
        let l = match bx {
            Some(b) if b[2] > 0 && b[3] > 0 => film::mean_luma(px, sw, x0 + b[0], y0 + b[1], b[2].min(tile), b[3].min(tile)),
            _ => film::mean_luma(px, sw, x0 + tile / 4, y0 + tile / 4, tile / 2, tile / 2),
        };
        if l < DARK_LUMA {
            self.dark.push(format!("{name} frame {k}"));
        }
        self.lumas.push(round(l as f64, 3));
        film::draw_text(px, sw, x0 + 3, y0 + 3, &format!("+{:.2}", t - self.t0), [200, 200, 190]);
        self.tiles.push(Json::arr([x0, y0, tile, tile].map(Json::from)));
        if k + 1 < self.times.len() {
            self.at = (vi, k + 1);
            return Ok(());
        }
        let scale = self.radius / r.max(1e-3);
        film::draw_text(px, sw, 4, y0 + tile + 2, &format!("{} {name}", self.path.rsplit('/').next().unwrap_or(&self.path)), [230, 226, 214]);
        film::draw_text(px, sw, 4, y0 + tile + 4 + film::CHAR_H, &format!("scale {scale:.2}x, lit from the camera"), [170, 175, 185]);
        self.rows.push(Json::obj([
            ("view", Json::from(name)),
            ("tiles", Json::Arr(std::mem::take(&mut self.tiles))),
            ("luma", Json::Arr(std::mem::take(&mut self.lumas))),
            ("framed_radius_m", round(r as f64, 3)),
            ("scale", round(scale as f64, 3)),
        ]));
        self.at = (vi + 1, 0);
        Ok(())
    }

    fn finish(&mut self, e: &mut Engine, reply: &jobs::Reply) -> Step {
        let id = self.id;
        let state = e.anim_state(&EntityKey::Id(id), Some(self.t0), self.window_s as f64);
        let state_json = state.content.iter().find_map(|c| match c {
            Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        });
        let n = self.times.len();
        let tile = self.sheet.tile;
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("anim_filmstrip")),
            ("entity", Json::from(self.path.as_str())),
            ("times_s", Json::arr(self.times.iter().map(|t| round(*t, 4)))),
            ("frames", Json::from(n)),
            ("tile", Json::from(tile)),
            ("rows", Json::Arr(std::mem::take(&mut self.rows))),
            ("lighting", Json::from("the world's lights plus a fill light at each camera")),
            ("note", Json::from("the entity's play at future times in the world as it is now (a moving entity is filmed in place)")),
        ]);
        if let Some(s) = state_json {
            packet.set("state", s);
        }
        // The verdict (as on stage sheets): the state's checks, then the feet on the real ground.
        let mut verdict: Vec<String> = Vec::new();
        let mut failed = false;
        if self.with_checks {
            let check = match CheckJob::new(e, None, Some(EntityKey::Id(id)), "creature", None, None, Vec::new(), None, 12) {
                Ok(job) => job.run(),
                Err(out) => out,
            };
            if let Some(Content::Text(t)) = check.content.first() {
                if let Ok(mut j) = Json::parse(t) {
                    if j.get("passed").and_then(Json::as_bool) == Some(true) {
                        let mut ex: Vec<String> = Vec::new();
                        for c in j.get("checks").and_then(Json::as_array).into_iter().flatten().filter(|c| c.get("exempt").and_then(Json::as_bool) == Some(true)) {
                            let name = c.get("check").and_then(Json::as_str).unwrap_or_default();
                            let n = match (name, c.get("bone").and_then(Json::as_str)) {
                                ("foot_sliding", Some(b)) => format!("{name}:{b}"),
                                _ => name.to_string(),
                            };
                            if !ex.contains(&n) {
                                ex.push(n);
                            }
                        }
                        let skipped: Vec<String> = j.get("skipped").and_then(Json::as_array).into_iter().flatten().filter_map(|s| s.as_str().map(|s| s.split(':').next().unwrap_or(s).to_string())).collect();
                        verdict.push(ok_verdict(&ex, &skipped));
                    } else if j.get("passed").is_some() {
                        failed = true;
                        let errs = j.get("errors").and_then(Json::as_array).into_iter().flatten().take(3);
                        let errs: Vec<String> = errs.map(|e| format!("FAIL {} {:.1}{}", e.get("check").and_then(Json::as_str).unwrap_or("?"), e.get("value").and_then(Json::as_f64).unwrap_or(0.0), e.get("unit").and_then(Json::as_str).unwrap_or(""))).collect();
                        verdict.push(errs.join("  "));
                    }
                    j.set("stage", Json::from("these checks play the entity's state on a flat stage; feet_in_world checks its feet on the world's own ground"));
                    packet.set("checks", j);
                }
            }
        }
        if let Some(f) = e.feet_in_world(&self.entity, &self.times) {
            if let Some(c) = f.get("check") {
                let ok = c.get("passed").and_then(Json::as_bool) == Some(true);
                failed |= !ok;
                verdict.push(format!("FEET ON GROUND {}", if ok { "OK".to_string() } else { format!("FAIL {:.1}CM", c.get("value").and_then(Json::as_f64).unwrap_or(0.0)) }));
            }
            packet.set("feet_in_world", f);
        }
        if !verdict.is_empty() {
            let v = verdict.join(", ");
            let colour = if failed { [255, 120, 100] } else { [140, 220, 160] };
            // Under the first row, after its scale note.
            film::draw_text(&mut self.sheet.pixels, self.sheet.sw, 4 + 36 * film::CHAR_W, tile + 4 + film::CHAR_H, &v, colour);
            packet.set("verdict", Json::from(v));
        }
        if !self.dark.is_empty() {
            packet.set("warnings", Json::arr([Json::from(format!("too dark to judge (mean luma under {DARK_LUMA}): {}", self.dark.iter().take(8).cloned().collect::<Vec<_>>().join(", ")))]));
        }
        let sheet = std::mem::replace(&mut self.sheet, Sheet { tile: 0, label_h: 0, sw: 0, sh: 0, pixels: Vec::new() });
        e.film_finish(&self.session, Answer { reply: reply.clone(), call: self.call }, sheet, packet);
        Step::Handed
    }
}

impl StepJob for EntityFilmJob {
    fn kind(&self) -> &'static str {
        "anim_filmstrip"
    }

    fn step(&mut self, e: &mut Engine, reply: &jobs::Reply) -> Step {
        if self.at.0 < self.views.len() {
            return match self.render_tile(e) {
                Ok(()) => Step::More,
                Err(out) => Step::Done(out),
            };
        }
        self.finish(e, reply)
    }
}

impl Engine {
    /// `anim_check` and `anim_filmstrip`: the job is set up now (refusals answered at once), then
    /// runs in steps.
    pub(crate) fn start_anim_job(&mut self, request: Request) {
        let Request { command: Command::Anim(call), session, reply, deadline, call: call_id, .. } = request else { unreachable!("only anim jobs come here") };
        let owner = jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        let start = |class| StepStart { owner: owner.clone(), session: session.id.clone(), call: call_id, reply: reply.clone(), deadline, class };
        let job: Result<(Box<dyn StepJob>, schedule::Class), ToolOutput> = match call {
            AnimCall::Check { source, key, graph, clips, states, transitions, scale, max_checks } => {
                CheckJob::new(self, source, key, &graph, clips, states, transitions, scale, max_checks).map(|j| (Box::new(j) as Box<dyn StepJob>, schedule::Class::Big))
            }
            AnimCall::Filmstrip { source, key, graph, rows, frames, tile, window_s, scale, checks } => {
                // Its sheet goes to a worker: with no slot free it is refused before it renders.
                if let Err(e) = self.workers_room(&owner, call_id, u64::from(MAX_SHEET_PX) * u64::from(MAX_SHEET_PX) * 4) {
                    reply.refuse(&e);
                    return;
                }
                match (key, source) {
                    (Some(k), _) => EntityFilmJob::new(self, &k, frames, tile, window_s, checks, &session, call_id).map(|j| (Box::new(j) as Box<dyn StepJob>, schedule::Class::Render)),
                    (None, Some(src)) => FilmJob::new(self, &src, &graph, rows, frames, tile, scale, checks, &session, call_id).map(|j| (Box::new(j) as Box<dyn StepJob>, schedule::Class::Render)),
                    (None, None) => Err(error("invalid_argument", "anim_filmstrip takes 'recipe' or the entity")),
                }
            }
            _ => unreachable!("only anim_check and anim_filmstrip are jobs"),
        };
        match job {
            Ok((job, class)) => self.step_job(start(class), job),
            Err(out) => {
                let _ = reply.send(out);
            }
        }
    }

    /// A filmstrip's sheet: numbered now, encoded and written (PNG and packet) on a worker, which
    /// answers.
    fn film_finish(&mut self, session: &SessionInfo, to: Answer, sheet: Sheet, mut packet: Json) {
        let png = self.next_capture_path();
        let frame = self.frames_captured;
        let (sw, sh) = (sheet.sw, sheet.sh);
        self.offload_capture(session, to, (sw, sh, sheet.pixels), png, "anim", move |png_bytes, saved, mut warnings| {
            let packet_path = saved.as_ref().map(|p| p.with_extension("json"));
            packet.set(
                "image",
                Json::obj([
                    ("width", Json::from(sw)),
                    ("height", Json::from(sh)),
                    ("png_bytes", Json::from(png_bytes.len())),
                    ("path", saved.as_ref().map(|p| p.display().to_string()).into()),
                    ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                ]),
            );
            packet.set("frame", Json::from(frame));
            if let Some(p) = &packet_path {
                if let Err(e) = crate::disk::book().write(crate::disk::Category::Captures, "anim", p, packet.to_pretty().as_bytes(), crate::disk::Need::Agent) {
                    warnings.push(Json::from(format!("packet not saved to disk: {e}")));
                }
            }
            if !warnings.is_empty() {
                let mut all = packet.get("warnings").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
                all.extend(warnings);
                packet.set("warnings", Json::Arr(all));
            }
            ToolOutput { content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_string())], is_error: false }
        });
    }
}

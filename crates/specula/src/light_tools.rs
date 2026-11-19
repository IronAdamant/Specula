//! `perf_light` (docs/design/LIGHT.md): what the light of a view costs on this machine.
//!
//! The engine has no "quality preset" to pick for a machine it has never seen: light is the
//! world's own settings (`world_env`'s shadows, ambient occlusion, bloom, anti-aliasing, fog,
//! sky and grading), the baked GI's tier and the point lights in view. What a machine can run
//! is not known from its name. This tool measures it where the engine runs: the view is drawn
//! offscreen as the world has it, then again with each feature that is on left out, in
//! alternating rounds, and the GPU's own timestamps say what each costs against a budget.
//! The engine reports; whoever builds the game decides (HANDOFF, the owner on 2026-11-14 and
//! 2026-11-16: the engine is a guide, and the agent on a machine sets its variables).
//!
//! It is a step job: a render is submitted in one step and landed in a later one, so the
//! GPU's work never holds the engine thread. Nothing of the world is changed and nothing is
//! written: the features are left out of the measuring renders alone ([`strip`]).

use super::*;
use crate::capture;
use crate::steps::{Step, StepJob, StepStart};
use crate::tools::{check_keys, int_arg, num_in, FrameSpec};
use specula_protocol::ToolDef;
use std::collections::BTreeMap;

/// Frames measured for each variant: at least, at most, and when none is asked for. (Three
/// were allowed and five were the default at first: by simulation the noise rule then called
/// one costless feature in six, and one in twenty, a cost. Its first review.)
pub const MIN_FRAMES: u32 = 5;
pub const MAX_FRAMES: u32 = 16;
pub const DEFAULT_FRAMES: u32 = 9;
/// How long a job waits for a view that is still loading before it measures it as it is.
const LOADING_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
/// A job ends its rounds this long before its call's time is out, with what it has.
const DEADLINE_MARGIN: std::time::Duration = std::time::Duration::from_secs(3);
/// Passes listed for the view as the world has it, the costliest first.
const MAX_PASSES: usize = 8;

/// A part of the light that a frame pays for and a game can do without.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
    Shadows,
    Ssao,
    Bloom,
    Fxaa,
    Fog,
    Sky,
    Grading,
    Gi,
    PointLights,
}

pub(crate) const FEATURES: [Feature; 9] = [Feature::Shadows, Feature::Ssao, Feature::Bloom, Feature::Fxaa, Feature::Fog, Feature::Sky, Feature::Grading, Feature::Gi, Feature::PointLights];

impl Feature {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Feature::Shadows => "shadows",
            Feature::Ssao => "ssao",
            Feature::Bloom => "bloom",
            Feature::Fxaa => "fxaa",
            Feature::Fog => "fog",
            Feature::Sky => "sky",
            Feature::Grading => "grading",
            Feature::Gi => "gi",
            Feature::PointLights => "point_lights",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Feature::Shadows => "the sun's or moon's shadows (cascaded shadow maps)",
            Feature::Ssao => "ambient occlusion from the depth buffer",
            Feature::Bloom => "glow round bright things",
            Feature::Fxaa => "anti-aliasing, a post pass",
            Feature::Fog => "height fog",
            Feature::Sky => "the analytic sky",
            Feature::Grading => "colour grading (and a grading LUT)",
            Feature::Gi => "baked global illumination (irradiance probes)",
            Feature::PointLights => "the point lights in view",
        }
    }

    /// How a game does without it, or with less of it.
    fn change(self) -> &'static str {
        match self {
            Feature::Shadows => "world_env {shadow: {enabled: false}}; or a lower resolution, fewer cascades, a shorter distance",
            Feature::Ssao => "world_env {ssao: {enabled: false}}; or fewer samples, a lower resolution",
            Feature::Bloom => "world_env {bloom: {enabled: false}}; or fewer levels",
            Feature::Fxaa => "world_env {fxaa: false}",
            Feature::Fog => "world_env {fog: {enabled: false}}",
            Feature::Sky => "world_env {sky: {enabled: false}}",
            Feature::Grading => "world_env {grading: {enabled: false}}, and no grading LUT",
            Feature::Gi => "gi_settings {quality: \"low\"} or \"off\" (a lead's to set)",
            Feature::PointLights => "fewer lights in view, or a shorter range for each",
        }
    }
}

/// The features a render leaves out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Without(u16);

impl Without {
    pub(crate) const NOTHING: Without = Without(0);

    fn bit(f: Feature) -> u16 {
        1 << FEATURES.iter().position(|g| *g == f).unwrap_or(0)
    }

    pub(crate) fn only(f: Feature) -> Without {
        Without(Without::bit(f))
    }

    pub(crate) fn and(self, f: Feature) -> Without {
        Without(self.0 | Without::bit(f))
    }

    pub(crate) fn has(self, f: Feature) -> bool {
        self.0 & Without::bit(f) != 0
    }

    fn count(self) -> u32 {
        self.0.count_ones()
    }
}

/// Takes `without` out of a frame about to be drawn. The world is not touched: the frame
/// alone is.
pub(crate) fn strip(frame: &mut specula_gpu::FrameDesc, without: Without) {
    let env = &mut frame.environment;
    if without.has(Feature::Shadows) {
        env.shadow.enabled = false;
    }
    if without.has(Feature::Ssao) {
        env.ssao.enabled = false;
    }
    if without.has(Feature::Bloom) {
        env.bloom.enabled = false;
    }
    if without.has(Feature::Fxaa) {
        env.fxaa = false;
    }
    if without.has(Feature::Fog) {
        env.fog.enabled = false;
    }
    if without.has(Feature::Sky) {
        env.sky.enabled = false;
    }
    if without.has(Feature::Grading) {
        env.grading.enabled = false;
        env.grading.lut = None;
    }
    if without.has(Feature::Gi) {
        frame.gi = Default::default();
    }
    if without.has(Feature::PointLights) {
        frame.lights = &[];
    }
}

/// The features of [`Feature`] a frame of this world pays for: the world's settings, whether
/// the render drew baked GI, and how many point lights it was given.
fn features_on(env: &specula_gpu::Environment, lut: bool, gi_drawn: bool, lights: usize) -> Without {
    let mut on = Without::NOTHING;
    for (f, is_on) in [
        (Feature::Shadows, env.shadow.enabled),
        (Feature::Ssao, env.ssao.enabled),
        (Feature::Bloom, env.bloom.enabled),
        (Feature::Fxaa, env.fxaa),
        (Feature::Fog, env.fog.enabled),
        (Feature::Sky, env.sky.enabled),
        // (As the renderer has it: grading is drawn when it is on and does something, and a
        // LUT counts only with it on. Listed as on with every value neutral, it cost nothing
        // and a variant a round. Its first review.)
        (Feature::Grading, env.grading.enabled && (env.grading.active() || lut)),
        (Feature::Gi, gi_drawn),
        (Feature::PointLights, lights > 0),
    ] {
        if is_on {
            on = on.and(f);
        }
    }
    on
}

/// `perf_light`'s arguments.
pub struct LightSpec {
    pub frame: FrameSpec,
    pub frames: u32,
    pub budget_ms: Option<f64>,
}

const KEYS: [&str; 12] = ["width", "height", "eye", "target", "fov_y_deg", "ortho", "view_2d", "camera", "camera_of", "include_ui", "frames", "budget_ms"];

pub fn parse(args: &Json) -> Result<LightSpec, String> {
    check_keys(args, "perf_light", &[&KEYS])?;
    // The view is `sight_frame`'s, by its own parser (its sizes and ranges hold here too).
    let mut frame = crate::tools::parse_frame(args)?;
    let camera = args.get("camera").filter(|v| !v.is_null()).is_some();
    if camera {
        // (`frame_extras` reads more than the camera; only the camera is this tool's.)
        let only = Json::obj([("camera", args.get("camera").cloned().unwrap_or(Json::Null)), ("camera_of", args.get("camera_of").cloned().unwrap_or(Json::Null))]);
        let mut named = crate::tools::parse_frame(&only)?;
        crate::sight_tools::frame_extras(&only, &mut named)?;
        frame.camera = named.camera;
    } else if args.get("camera_of").filter(|v| !v.is_null()).is_some() {
        return Err("'camera_of' names whose camera 'camera' is: give 'camera' with it".into());
    }
    frame.ids = false;
    Ok(LightSpec {
        frame,
        frames: int_arg(args, "frames", u64::from(MIN_FRAMES), u64::from(MAX_FRAMES))?.map_or(DEFAULT_FRAMES, |n| n as u32),
        budget_ms: num_in(args, "budget_ms", 0.1..=1000.0)?.map(f64::from),
    })
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![ToolDef {
        name: "perf_light".into(),
        description: "What the light of a view costs on this machine, measured here: the view is drawn offscreen as the world \
                      has it, then again with each light feature that is on left out (the sun's shadows, ambient occlusion, \
                      bloom, anti-aliasing, fog, the sky, grading, baked GI, the point lights in view), and once with all of \
                      them out. Answers the GPU time of each (the median of `frames` renders, by the adapter's own \
                      timestamps, in alternating rounds), each feature's cost and whether it is told from the noise (the \
                      frames' own scatter), whether \
                      the view holds `budget_ms`, and which features to give up, costliest first, when it does not. Also \
                      what the engine's light has and has not, whatever the machine (it never uses ray-tracing hardware). \
                      Changes nothing in the world and writes nothing; a render per step, between frames. The numbers are \
                      this adapter's, at this size, for this view: measure again where the game will run."
            .into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("width", t("integer", "Pixels, 64..2048 (default 960): measure at the size the game will show")),
                    ("height", t("integer", "Pixels, 64..2048 (default 540)")),
                    ("camera", t("string", "A named camera (camera_set); default: eye and target, or the whole world framed")),
                    ("camera_of", t("string", "Whose camera, when it is another agent's (1..64 characters)")),
                    ("eye", t("array", "[x, y, z] metres, with target")),
                    ("target", t("array", "[x, y, z] metres, with eye")),
                    ("fov_y_deg", t("number", "10..120 (default 50)")),
                    ("ortho", t("object", "{height_m 0.01..100000}: an orthographic view")),
                    ("view_2d", t("object", "{center: [x, y], height_m 0.01..100000, z}: a 2D view")),
                    ("include_ui", t("boolean", "Draw (and so measure) ui-layer entities and a hosted game's interface too (default false, as sight_frame)")),
                    ("frames", t("integer", "Renders measured for each variant, 5..16 (default 9); one more of each is drawn first and not counted")),
                    ("budget_ms", t("number", "The GPU time a frame may take, 0.1..1000 ms (default: this adapter's gpu_ms budget from perf.toml)")),
                ]),
            ),
            ("required", Json::arr(std::iter::empty::<Json>())),
        ]),
    }]
}

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn ms(v: f64) -> Json {
    Json::from((v * 1000.0).round() / 1000.0)
}

/// The middle of the samples (the mean of the middle two of an even count); 0 of none.
fn median(samples: &[f64]) -> f64 {
    let mut s = samples.to_vec();
    s.sort_by(f64::total_cmp);
    match s.len() {
        0 => 0.0,
        n if n % 2 == 1 => s[n / 2],
        n => (s[n / 2 - 1] + s[n / 2]) / 2.0,
    }
}

/// How far a variant's frames are from each other, as one number that one slow frame cannot
/// spoil: some times the middle of their distances from their median. (The least frame to
/// the most was tried first: of nine frames of Sepulcra's scene on the integrated GPU eight
/// were within 0.01 ms of each other and one was 0.3 ms late, and every cost under 0.3 ms was
/// called not told from the noise, the sun's shadows among them.)
///
/// How many times goes by how many frames there are, so that two variants that cost the same
/// are called apart about once in two hundred: three times for nine frames or more, four for
/// seven or eight, six for five or six (by simulation, 40,000 pairs of equal normal samples a
/// cell: 0.6%, 0.5% and 0.6%; three times at five frames was 4.9%, which its first review
/// found, and would have named a costless feature in a third of reports).
fn scatter(samples: &[f64]) -> f64 {
    let middle = median(samples);
    let off: Vec<f64> = samples.iter().map(|v| (v - middle).abs()).collect();
    let times = match samples.len() {
        0..=6 => 6.0,
        7..=8 => 4.0,
        _ => 3.0,
    };
    times * median(&off)
}

/// One way the view is drawn, and its measured frames.
struct Variant {
    without: Without,
    /// The one feature it leaves out (`None`: as the world has it, or everything out).
    feature: Option<Feature>,
    total: Vec<f64>,
    passes: BTreeMap<&'static str, Vec<f64>>,
}

impl Variant {
    fn new(without: Without, feature: Option<Feature>) -> Variant {
        Variant { without, feature, total: Vec::new(), passes: BTreeMap::new() }
    }

    fn json(&self) -> Json {
        let (lo, hi) = self.total.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        Json::obj([("gpu_ms", ms(median(&self.total))), ("min_ms", ms(if lo.is_finite() { lo } else { 0.0 })), ("max_ms", ms(if hi.is_finite() { hi } else { 0.0 })), ("frames", Json::from(self.total.len()))])
    }
}

/// The measured variants into the answer's numbers: pure, so that it is tested without a GPU.
/// `variants[0]` is the view as the world has it; one variant for each feature that is on;
/// and, when more than one is on, a last one with all of them out.
fn report(variants: &[Variant], on: Without, budget_ms: f64) -> Json {
    let as_is = &variants[0];
    let total = median(&as_is.total);
    let bare = variants.iter().find(|v| v.feature.is_none() && v.without != Without::NOTHING);
    let mut features: Vec<(f64, Json)> = Vec::new();
    let mut costs: Vec<(Feature, f64, bool)> = Vec::new();
    for f in FEATURES {
        let Some(v) = variants.iter().find(|v| v.feature == Some(f)) else {
            if !on.has(f) {
                features.push((f64::NEG_INFINITY, Json::obj([("name", Json::from(f.name())), ("what", Json::from(f.about())), ("on", Json::from(false))])));
            }
            continue;
        };
        let without = median(&v.total);
        let cost = total - without;
        // Two medians apart by less than either's own scatter are not told apart; nor by less
        // than a two-hundredth of the frame, whatever the scatter says (three frames alike to
        // the last digit have none).
        let noise = scatter(&as_is.total).max(scatter(&v.total)).max(total * 0.005);
        // (With fewer frames than the least asked for, a job that ended early: nothing is told.)
        let told = cost.abs() > noise && as_is.total.len() >= MIN_FRAMES as usize && v.total.len() >= MIN_FRAMES as usize;
        costs.push((f, cost, told));
        features.push((
            cost,
            Json::obj([
                ("name", Json::from(f.name())),
                ("what", Json::from(f.about())),
                ("on", Json::from(true)),
                ("cost_ms", ms(cost)),
                ("share", Json::from(if total > 0.0 { ((cost / total) * 1000.0).round() / 1000.0 } else { 0.0 })),
                ("without_ms", ms(without)),
                ("noise_ms", ms(noise)),
                ("told_from_noise", Json::from(told)),
                ("to_change", Json::from(f.change())),
            ]),
        ));
    }
    // The costliest first; the features that are off after them, in their own order.
    features.sort_by(|a, b| b.0.total_cmp(&a.0));
    let holds = total <= budget_ms;
    let mut out = Json::obj([
        ("budget_ms", ms(budget_ms)),
        ("holds_budget", Json::from(holds)),
        ("as_it_is", as_is.json()),
        ("features", Json::Arr(features.into_iter().map(|(_, j)| j).collect())),
    ]);
    if let Some(b) = bare {
        let mut j = b.json();
        j.set("what", Json::from("every feature that is on, left out: what is left is the scene's own drawing"));
        out.set("without_all", j);
    }
    let mut passes: Vec<(&'static str, f64)> = as_is.passes.iter().map(|(name, v)| (*name, median(v))).collect();
    passes.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    out.set("passes", Json::arr(passes.iter().take(MAX_PASSES).map(|(name, v)| Json::obj([("pass", Json::from(*name)), ("gpu_ms", ms(*v))]))));
    if !holds {
        // What to give up, the costliest first, until the sum of what was measured apart says
        // the budget holds. Costs measured apart do not add up exactly: it is an estimate, and
        // the floor is what was measured with everything out.
        let floor = bare.map_or(total - costs.iter().map(|c| c.1.max(0.0)).sum::<f64>(), |b| median(&b.total));
        costs.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut estimate = total;
        let mut give_up = Vec::new();
        for (f, cost, told) in &costs {
            // (The costliest first; done when the budget holds, or when what is left costs
            // nothing. A feature not told from the noise is passed over, not stopped at: a
            // dear and noisy one hid a cheaper one that would have held the budget, and the
            // note then said nothing would. Its first review.)
            if estimate <= budget_ms || *cost <= 0.0 {
                break;
            }
            if !*told {
                continue;
            }
            estimate = (estimate - cost).max(floor);
            give_up.push(Json::from(f.name()));
        }
        let enough = estimate <= budget_ms;
        out.set(
            "to_hold_budget",
            Json::obj([
                ("give_up", Json::Arr(give_up)),
                ("estimate_ms", ms(estimate)),
                ("enough", Json::from(enough)),
                (
                    "note",
                    Json::from(if enough {
                        "an estimate from costs measured apart: make the changes and measure again"
                    } else if floor > budget_ms {
                        "light is not what keeps this view over its budget: with every feature out it is still over. Draw less (fewer or simpler things in view, a smaller size), or allow more time"
                    } else {
                        "giving up every feature told from the noise is not enough by this estimate: measure again with them off, draw less, or allow more time"
                    }),
                ),
            ]),
        );
    }
    out
}

/// What the engine's light is, whatever the machine: said with every report, so that nobody
/// promises a game what the engine does not have.
fn engine_light() -> Json {
    Json::obj([
        (
            "has",
            Json::arr(
                [
                    "one sun or moon, with cascaded shadow maps (1 to 4 cascades)",
                    "up to 16 point lights a frame (they cast no shadows)",
                    "ambient occlusion from the depth buffer (ssao)",
                    "baked global illumination: irradiance probe volumes baked on the CPU (gi_bake), drawn at tier off, low or high",
                    "bloom, anti-aliasing (fxaa), height fog, an analytic sky, colour grading with LUTs",
                    "glossy surfaces reflect the sky and the ambient light, dimmed by the baked GI",
                ]
                .map(Json::from),
            ),
        ),
        (
            "has_not",
            Json::arr(
                [
                    "ray tracing on the GPU: the engine never uses ray-tracing hardware, on any adapter",
                    "bounce light that follows a moving light or object: the GI is baked, and is baked again when the scene changes",
                    "reflections of the scene itself (no screen-space reflections, no reflection probes)",
                    "shadows from point lights",
                ]
                .map(Json::from),
            ),
        ),
    ])
}

/// `perf_light`: the view drawn as the world has it and with each feature out, a render a step.
pub(crate) struct LightJob {
    camera: Camera,
    source: Json,
    warning: Option<String>,
    size: (u32, u32),
    include_ui: bool,
    frames: u32,
    budget_ms: f64,
    budget_from: &'static str,
    /// `[0]` is the view as the world has it; the rest are made when its first render lands
    /// and says which features this view pays for.
    variants: Vec<Variant>,
    on: Without,
    /// Round 0 is drawn and not counted.
    round: u32,
    next: usize,
    flying: bool,
    slot: std::rc::Rc<capture::flight::StepTile>,
    lights: usize,
    dropped_lights: usize,
    gi_error: Option<String>,
    renders: u32,
    started: Instant,
    /// When the call's time is out: the job ends its rounds a little before, with what it has.
    deadline: Instant,
    /// What the view was still waiting for when it was measured (0: nothing).
    loading: usize,
    /// It ended before its rounds were done.
    cut_short: bool,
}

impl LightJob {
    pub(crate) fn new(e: &mut Engine, spec: LightSpec, session: &SessionInfo, deadline: Instant) -> Result<LightJob, ToolOutput> {
        let _ = e.before_capture();
        let (camera, source, warning) = e.frame_camera(&spec.frame, session)?;
        let kind = perf::adapter_kind(e.gpu.adapter().kind);
        let (budget_ms, budget_from) = match spec.budget_ms {
            Some(b) => (b, "the call's budget_ms"),
            None => (e.perf_cfg.budgets(kind).gpu_ms, "perf.toml's gpu_ms for this adapter (or its default)"),
        };
        Ok(LightJob {
            camera,
            source,
            warning,
            size: (spec.frame.width, spec.frame.height),
            include_ui: spec.frame.include_ui,
            frames: spec.frames,
            budget_ms,
            budget_from,
            variants: vec![Variant::new(Without::NOTHING, None)],
            on: Without::NOTHING,
            round: 0,
            next: 0,
            flying: false,
            slot: std::rc::Rc::default(),
            lights: 0,
            dropped_lights: 0,
            gi_error: None,
            renders: 0,
            started: Instant::now(),
            deadline,
            loading: 0,
            cut_short: false,
        })
    }

    /// Submits the next render (no wait for the GPU: it lands in a later step).
    fn submit(&mut self, e: &mut Engine) -> Result<(), ToolOutput> {
        let without = self.variants[self.next].without;
        let overlays = crate::sight_overlay::Spec::default();
        let opts = capture::ViewOpts { include_ui: self.include_ui, lod: None, time_s: e.world.world_time(), overlays: &overlays, caption: None, ids: false, hud: true, fill: None };
        // Set for this one render, as a form view is.
        e.captures.light = Some(without);
        let submitted = e.render_view_submit(self.camera, self.size.0, self.size.1, &opts);
        e.captures.light = None;
        match submitted.map_err(ToolOutput::error)? {
            capture::Submitted::Done(r) => *self.slot.borrow_mut() = Some(Ok(*r)),
            capture::Submitted::Flying(v) => e.captures.step_flight = Some((std::rc::Rc::downgrade(&self.slot), v)),
        }
        self.flying = true;
        Ok(())
    }

    /// The render in flight once the GPU has finished it (`Ok(false)`: not yet).
    fn land(&mut self, e: &mut Engine) -> Result<bool, ToolOutput> {
        let taken = self.slot.borrow_mut().take();
        let landed = match taken {
            Some(r) => r,
            None => {
                if !e.captures.step_flight.as_ref().is_some_and(|(s, _)| std::ptr::eq(s.as_ptr(), std::rc::Rc::as_ptr(&self.slot))) {
                    return Err(error("render_unavailable", "a measuring render was lost in flight"));
                }
                match e.gpu.settle_offscreen_within(Some(std::time::Duration::ZERO)) {
                    Ok(false) => return Ok(false),
                    Ok(true) => {
                        let (_, v) = e.captures.step_flight.take().expect("checked above");
                        e.render_view_land(*v)
                    }
                    Err(err) => Err(capture::gpu_failure("render", &err, e.windowed)),
                }
            }
        };
        // (Its place in the flash watch is dropped unrun: a render with light left out is not
        // what the view shows.)
        let rend = landed.map_err(ToolOutput::error)?;
        self.flying = false;
        self.renders += 1;
        let Some(gpu) = rend.gpu else {
            return Err(error(
                "no_gpu_timings",
                "this adapter's queue gave no timestamps for the render, so what the light costs cannot be measured here (engine_info and perf_frame say whether timestamps are supported)",
            ));
        };
        if self.round == 0 && self.next == 0 {
            // The first render says what this view pays for.
            let env = e.world.env().light;
            let lut = !e.world.env().grading_lut.is_none();
            self.on = features_on(&env, lut, rend.gi_drawn, rend.lights);
            self.lights = rend.lights;
            self.dropped_lights = rend.dropped_lights;
            self.gi_error = rend.gi_error.clone();
            let mut all = Without::NOTHING;
            for f in FEATURES.into_iter().filter(|f| self.on.has(*f)) {
                self.variants.push(Variant::new(Without::only(f), Some(f)));
                all = all.and(f);
            }
            if all.count() > 1 {
                self.variants.push(Variant::new(all, None));
            }
        }
        if self.round > 0 {
            let v = &mut self.variants[self.next];
            v.total.push(gpu.total_ms);
            for p in &gpu.passes {
                v.passes.entry(p.name).or_default().push(p.gpu_ms);
            }
        }
        self.next += 1;
        if self.next == self.variants.len() {
            self.next = 0;
            self.round += 1;
        }
        Ok(true)
    }

    fn finish(&mut self, e: &mut Engine) -> ToolOutput {
        let adapter = e.gpu.adapter();
        let env = e.world.env().light;
        let mut out = report(&self.variants, self.on, self.budget_ms);
        out.set("budget_from", Json::from(self.budget_from));
        out.set("adapter", Json::obj([("name", Json::from(adapter.name.as_str())), ("kind", Json::from(perf::adapter_kind(adapter.kind)))]));
        out.set("size", Json::arr([self.size.0, self.size.1].map(Json::from)));
        out.set("camera", self.source.clone());
        out.set(
            "settings",
            Json::obj([
                ("shadow", Json::obj([("enabled", Json::from(env.shadow.enabled)), ("resolution", Json::from(env.shadow.resolution)), ("cascades", Json::from(env.shadow.cascades))])),
                ("ssao", Json::obj([("enabled", Json::from(env.ssao.enabled)), ("samples", Json::from(env.ssao.samples)), ("resolution", Json::from(f64::from(env.ssao.resolution)))])),
                ("bloom", Json::obj([("enabled", Json::from(env.bloom.enabled)), ("levels", Json::from(env.bloom.levels))])),
                ("gi", Json::obj([("tier", Json::from(e.gi.quality.name())), ("drawn", Json::from(self.on.has(Feature::Gi)))])),
                ("point_lights", Json::obj([("in_view", Json::from(self.lights)), ("left_out", Json::from(self.dropped_lights))])),
            ]),
        );
        out.set("renders", Json::from(self.renders));
        out.set("took_ms", Json::from((self.started.elapsed().as_secs_f64() * 1000.0).round()));
        out.set(
            "measured",
            Json::from(
                "GPU time of an offscreen frame of this view at this size, first pass start to last pass end, by the adapter's timestamps: the median of the frames counted, the variants drawn in alternating rounds, one round first that is not counted. The ID buffer is not drawn and no image is read back. Other programs use the same GPU: a cost inside noise_ms is not told from nothing. A window's frame also pays for presenting (perf_frame, whose `offscreen` record is now the last of these measuring renders, drawn without its light).",
            ),
        );
        out.set("engine_light", engine_light());
        let mut warnings = Vec::new();
        if let Some(w) = self.warning.take() {
            warnings.push(Json::from(w));
        }
        if let Some(g) = self.gi_error.take() {
            warnings.push(Json::from(format!("baked GI was not drawn: {g}")));
        }
        if self.loading > 0 {
            warnings.push(Json::from(format!("{} things in view were still loading when it was measured (models building, streamed cells, textures): measure again when they are in", self.loading)));
        }
        if self.cut_short {
            let counted = self.variants[0].total.len();
            warnings.push(Json::from(format!(
                "ended after {counted} of {} rounds: the call's time was nearly out{}",
                self.frames,
                if counted < MIN_FRAMES as usize { "; with so few frames no cost is said to be told from the noise" } else { "" }
            )));
        }
        if self.dropped_lights > 0 {
            warnings.push(Json::from(format!("{} point lights in view were left out of the frame: it draws {} at most", self.dropped_lights, specula_gpu::MAX_POINT_LIGHTS)));
        }
        if !warnings.is_empty() {
            out.set("warnings", Json::Arr(warnings));
        }
        ToolOutput::text(out.to_string())
    }
}

impl StepJob for LightJob {
    fn kind(&self) -> &'static str {
        "perf_light"
    }

    fn step(&mut self, e: &mut Engine, _reply: &crate::jobs::Reply) -> Step {
        if self.flying {
            return match self.land(e) {
                Ok(_) => Step::More,
                Err(out) => Step::Done(out),
            };
        }
        // The call's time nearly out, at the start of a round: what it has is answered (a
        // timed-out call answers nothing at all).
        if self.next == 0 && self.round >= 2 && self.round <= self.frames && Instant::now() + DEADLINE_MARGIN > self.deadline {
            self.cut_short = true;
            return Step::Done(self.finish(e));
        }
        if self.round <= self.frames {
            // Before its first render: what the view is still loading is waited for, as
            // `sight_frame` waits (a scene measured while its models build is not the scene),
            // for a while; then it is measured as it is, and the answer says so.
            if self.renders == 0 {
                self.loading = e.view_loading(self.camera, self.size.0, self.size.1, self.include_ui);
                if self.loading > 0 && self.started.elapsed() < LOADING_WAIT && Instant::now() + LOADING_WAIT < self.deadline {
                    return Step::Wait;
                }
            }
            // The subject cache a slice per step first, as the capture flight does.
            if !e.subject_cache.warm(&e.world, e.world_generation, std::time::Duration::from_millis(3)) {
                return Step::More;
            }
            return match self.submit(e) {
                Ok(()) => Step::More,
                Err(out) => Step::Done(out),
            };
        }
        Step::Done(self.finish(e))
    }
}

impl Engine {
    /// `perf_light`: set up now (a refusal is answered at once), then a render a step.
    pub(crate) fn start_light_job(&mut self, request: Request) {
        let Request { command: Command::PerfLight(spec), session, reply, deadline, call: call_id, .. } = request else { unreachable!("only perf_light comes here") };
        let owner = crate::jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        match LightJob::new(self, *spec, &session, deadline) {
            Ok(job) => self.step_job(StepStart { owner, session: session.id.clone(), call: call_id, reply, deadline, class: schedule::Class::Render }, Box::new(job)),
            Err(out) => {
                let _ = reply.send(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Json {
        Json::parse(s).unwrap()
    }

    fn variant(without: Without, feature: Option<Feature>, total: &[f64]) -> Variant {
        let mut v = Variant::new(without, feature);
        v.total = total.to_vec();
        v
    }

    #[test]
    fn its_arguments_are_bounded() {
        let s = parse(&args("{}")).unwrap();
        assert_eq!((s.frame.width, s.frame.height, s.frames, s.budget_ms), (960, 540, DEFAULT_FRAMES, None));
        assert!(!s.frame.include_ui && parse(&args(r#"{"include_ui": true}"#)).unwrap().frame.include_ui);
        assert!(!s.frame.ids, "no ID buffer is drawn for a measurement");
        let s = parse(&args(r#"{"width": 1280, "height": 720, "frames": 16, "budget_ms": 8.3, "eye": [0, 2, 6], "target": [0, 1, 0]}"#)).unwrap();
        assert_eq!((s.frame.width, s.frame.height, s.frames), (1280, 720, 16));
        assert!(s.budget_ms.is_some_and(|b| (b - 8.3).abs() < 1e-5));
        let s = parse(&args(r#"{"camera": "overview"}"#)).unwrap();
        assert_eq!(s.frame.camera.as_ref().map(|c| c.name.as_str()), Some("overview"));
        for bad in [
            r#"{"frames": 4}"#,
            r#"{"frames": 17}"#,
            r#"{"budget_ms": 0}"#,
            r#"{"budget_ms": 1001}"#,
            r#"{"width": 4096}"#,
            r#"{"eye": [0, 0, 0]}"#,
            r#"{"camera": "a", "eye": [0, 0, 1], "target": [0, 0, 0]}"#,
            r#"{"camera_of": "someone"}"#,
            r#"{"ids": true}"#,
            r#"{"lod": 1}"#,
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad} is refused");
        }
        // Every argument the tool reads is in its schema, with its range said.
        let def = &definitions()[0];
        let props = def.input_schema.get("properties").unwrap();
        for k in KEYS {
            assert!(props.get(k).is_some(), "{k} is in the schema");
        }
    }

    /// A frame with its light taken out has none of it left, and one feature out leaves the
    /// others alone.
    #[test]
    fn a_stripped_frame_has_none_of_what_was_taken() {
        let mut env = specula_gpu::Environment::default();
        (env.shadow.enabled, env.ssao.enabled, env.bloom.enabled, env.fxaa, env.fog.enabled, env.sky.enabled, env.grading.enabled) = (true, true, true, true, true, true, true);
        // (Grading that does something: a neutral one is not drawn, and is not on.)
        env.grading.saturation = 1.25;
        let lights = [specula_gpu::PointLight { position: [0.0, 2.0, 0.0], color: [1.0, 0.8, 0.6], intensity: 4.0, range: 6.0 }; 3];
        let on = features_on(&env, false, false, lights.len());
        assert!(FEATURES.into_iter().all(|f| on.has(f) == (f != Feature::Gi)));
        let frame = || specula_gpu::FrameDesc { environment: env, lights: &lights, ..Default::default() };
        let mut one = frame();
        strip(&mut one, Without::only(Feature::Bloom));
        assert!(!one.environment.bloom.enabled && one.environment.shadow.enabled && one.environment.fxaa && one.lights.len() == 3);
        let mut all = frame();
        strip(&mut all, FEATURES.into_iter().fold(Without::NOTHING, Without::and));
        let e = &all.environment;
        assert!(!e.shadow.enabled && !e.ssao.enabled && !e.bloom.enabled && !e.fxaa && !e.fog.enabled && !e.sky.enabled && !e.grading.enabled && e.grading.lut.is_none());
        assert!(all.lights.is_empty() && !all.gi.active());
        assert_eq!(features_on(e, false, false, 0), Without::NOTHING);
        // Grading that is on and does nothing is not drawn, and is not listed as on; a LUT
        // counts with grading on, and not without.
        let mut graded = specula_gpu::Environment::default();
        graded.grading.enabled = true;
        assert!(!graded.grading.active() && !features_on(&graded, false, false, 0).has(Feature::Grading));
        assert!(features_on(&graded, true, false, 0).has(Feature::Grading));
        graded.grading.enabled = false;
        assert!(!features_on(&graded, true, false, 0).has(Feature::Grading));
        // Nothing taken: the frame is as it was.
        let mut same = frame();
        strip(&mut same, Without::NOTHING);
        assert!(same.environment == env && same.lights.len() == 3);
    }

    /// The numbers of a report, from measurements given to it: costs, their order, what is
    /// told from the noise, and what to give up for a budget.
    #[test]
    fn a_report_says_what_each_feature_costs_and_what_to_give_up() {
        let on = Without::only(Feature::Shadows).and(Feature::Ssao).and(Feature::Fxaa);
        let all = on;
        let variants = [
            variant(Without::NOTHING, None, &[10.0, 10.2, 9.9, 10.1, 10.0]),
            variant(Without::only(Feature::Shadows), Some(Feature::Shadows), &[6.0, 6.1, 5.9, 6.0, 6.2]),
            variant(Without::only(Feature::Ssao), Some(Feature::Ssao), &[8.0, 8.1, 7.9, 8.0, 8.0]),
            // (Not told from the noise: 0.1 ms apart, where this variant's own frames are 0.1 ms
            // from their middle.)
            variant(Without::only(Feature::Fxaa), Some(Feature::Fxaa), &[9.9, 10.0, 9.8, 10.1, 9.9]),
            variant(all, None, &[3.0, 3.1, 2.9, 3.0, 3.0]),
        ];
        // Within the budget: nothing to give up is said.
        let r = report(&variants, on, 12.0);
        assert_eq!(r.get("holds_budget").and_then(Json::as_bool), Some(true));
        assert!(r.get("to_hold_budget").is_none());
        assert_eq!(r.get("as_it_is").and_then(|a| a.get("gpu_ms")).and_then(Json::as_f64), Some(10.0));
        assert_eq!(r.get("without_all").and_then(|a| a.get("gpu_ms")).and_then(Json::as_f64), Some(3.0));
        let features = r.get("features").and_then(Json::as_array).unwrap();
        let names: Vec<&str> = features.iter().filter_map(|f| f.get("name").and_then(Json::as_str)).collect();
        // The costliest first, then the ones that are off.
        assert_eq!(&names[..3], ["shadows", "ssao", "fxaa"]);
        assert_eq!(names.len(), FEATURES.len());
        let of = |name: &str| features.iter().find(|f| f.get("name").and_then(Json::as_str) == Some(name)).unwrap();
        assert_eq!(of("shadows").get("cost_ms").and_then(Json::as_f64), Some(4.0));
        assert_eq!(of("shadows").get("told_from_noise").and_then(Json::as_bool), Some(true));
        assert_eq!(of("fxaa").get("told_from_noise").and_then(Json::as_bool), Some(false));
        assert_eq!(of("bloom").get("on").and_then(Json::as_bool), Some(false));
        assert!(of("bloom").get("cost_ms").is_none(), "a feature that is off has no cost made up for it");
        // Over the budget: the costliest go first, as few as the estimate needs.
        let r = report(&variants, on, 7.0);
        let give = r.get("to_hold_budget").unwrap();
        let names: Vec<&str> = give.get("give_up").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).collect();
        assert_eq!(names, ["shadows"]);
        assert_eq!((give.get("estimate_ms").and_then(Json::as_f64), give.get("enough").and_then(Json::as_bool)), (Some(6.0), Some(true)));
        let r = report(&variants, on, 4.5);
        let give = r.get("to_hold_budget").unwrap();
        let names: Vec<&str> = give.get("give_up").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).collect();
        // (A feature not told from the noise is never named: fxaa stays.)
        assert_eq!(names, ["shadows", "ssao"]);
        assert_eq!(give.get("enough").and_then(Json::as_bool), Some(true));
        // A dear feature that is not told from the noise is passed over, and a cheaper one that
        // is told is named: here giving up ssao alone holds the budget (its first review: the
        // walk stopped at the noisy one, named nothing, and said nothing would be enough).
        let noisy = [
            variant(Without::NOTHING, None, &[10.0; 5]),
            variant(Without::only(Feature::Shadows), Some(Feature::Shadows), &[3.0, 5.0, 7.0, 9.0, 11.0]),
            variant(Without::only(Feature::Ssao), Some(Feature::Ssao), &[8.0; 5]),
            variant(Without::only(Feature::Shadows).and(Feature::Ssao), None, &[5.0; 5]),
        ];
        let r = report(&noisy, Without::only(Feature::Shadows).and(Feature::Ssao), 8.5);
        let give = r.get("to_hold_budget").unwrap();
        let names: Vec<&str> = give.get("give_up").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).collect();
        assert_eq!((names, give.get("estimate_ms").and_then(Json::as_f64), give.get("enough").and_then(Json::as_bool)), (vec!["ssao"], Some(8.0), Some(true)), "{give}");
        // With fewer frames than the least a job counts (one that ended early), nothing is told.
        let few = [variant(Without::NOTHING, None, &[10.0, 10.0]), variant(Without::only(Feature::Shadows), Some(Feature::Shadows), &[6.0, 6.0])];
        let r = report(&few, Without::only(Feature::Shadows), 12.0);
        assert_eq!(r.get("features").and_then(Json::as_array).unwrap()[0].get("told_from_noise").and_then(Json::as_bool), Some(false));
        // A budget under what the scene alone costs: light is not the cause, and it says so.
        let r = report(&variants, on, 2.0);
        let give = r.get("to_hold_budget").unwrap();
        assert_eq!(give.get("enough").and_then(Json::as_bool), Some(false));
        // (Shadows and ssao given up: 10 - 4 - 2. Fxaa, not told from the noise, is not counted on.)
        assert_eq!(give.get("estimate_ms").and_then(Json::as_f64), Some(4.0));
        assert!(give.get("note").and_then(Json::as_str).unwrap().contains("light is not what keeps"));
    }

    /// A view is measured with each of its features out, in rounds, and the world is left as
    /// it was: the same revision, the same settings, no frame compiling a pipeline. (A GPU;
    /// skips cleanly without one. What each costs is this machine's to say: the test holds
    /// what was drawn and counted, not a time.)
    #[test]
    fn a_view_is_measured_with_each_feature_out_and_the_world_is_left_as_it_was() {
        use crate::timeline::tests::{engine, session, spawn};
        let Some(mut e) = engine("a_view_is_measured_with_each_feature_out") else { return };
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            spawn(&mut e, name, i as f32 * 1.5);
        }
        let Ok(Command::Env(patch)) = crate::tools::parse("world_env", &args(r#"{"shadow": {"enabled": true}, "ssao": {"enabled": true}, "bloom": {"enabled": true}, "fxaa": true, "sky": {"enabled": false}, "fog": {"enabled": false}}"#)) else {
            panic!("world_env parses")
        };
        let alice = crate::journal::Writer { name: "alice", session: Some("aaaaaa11"), via: None };
        e.world.commit(&mut e.swarm, 0, alice, crate::journal::Kind::Env, crate::write_policy::Rule::World, |wd| wd.set_env(&patch)).unwrap();
        let (revision, env) = (e.world.revision(), e.world.env().light);
        let spec = parse(&args(r#"{"width": 320, "height": 180, "frames": 5, "budget_ms": 1000, "eye": [4, 3, 6], "target": [1.5, 0, 0]}"#)).unwrap();
        let far = Instant::now() + std::time::Duration::from_secs(600);
        let mut job = LightJob::new(&mut e, spec, &session("aaaaaa11"), far).unwrap();
        // A second job at once, of another view: each render's timings are its own (the GPU's
        // record of its last offscreen frame is another job's by the time a job asks).
        let other = parse(&args(r#"{"width": 256, "height": 256, "frames": 5, "budget_ms": 1000, "eye": [-3, 2, 5], "target": [1.5, 0, 0]}"#)).unwrap();
        let mut second = LightJob::new(&mut e, other, &session("bbbbbb22"), far).unwrap();
        let (reply2, _rx2) = crate::jobs::channel();
        let mut out2 = None;
        // (Its first render made, the engine is warm: nothing after it may compile.)
        let (reply, _rx) = crate::jobs::channel();
        let mut out = None;
        let mut warmed = false;
        let started = Instant::now();
        while out.is_none() {
            assert!(started.elapsed() < std::time::Duration::from_secs(120), "the report took over two minutes");
            match job.step(&mut e, &reply) {
                Step::Done(o) => out = Some(o),
                Step::More | Step::Wait => {}
                Step::Handed => panic!("it answers itself"),
            }
            if out2.is_none() {
                match second.step(&mut e, &reply2) {
                    Step::Done(o) => out2 = Some(o),
                    Step::More | Step::Wait => {}
                    Step::Handed => panic!("it answers itself"),
                }
            }
            if !warmed && job.renders >= 1 && second.renders >= 1 {
                e.gpu.mark_warm();
                warmed = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        while out2.is_none() {
            assert!(started.elapsed() < std::time::Duration::from_secs(120), "the second report took over two minutes");
            if let Step::Done(o) = second.step(&mut e, &reply2) {
                out2 = Some(o);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let out2 = out2.unwrap();
        assert!(!out2.is_error, "the second job: {:?}", out2.content);
        let out = out.unwrap();
        assert!(!out.is_error, "{:?}", out.content);
        let r = out.content.iter().find_map(|c| match c {
            specula_protocol::Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        });
        let r = r.expect("a report");
        // Four features are on: each measured alone, and all of them together; one round
        // more than was asked for is drawn and not counted.
        let features = r.get("features").and_then(Json::as_array).unwrap();
        let on: Vec<&str> = features.iter().filter(|f| f.get("on").and_then(Json::as_bool) == Some(true)).filter_map(|f| f.get("name").and_then(Json::as_str)).collect();
        let mut sorted = on.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ["bloom", "fxaa", "shadows", "ssao"], "{r}");
        assert_eq!(features.len(), FEATURES.len());
        assert_eq!(r.get("renders").and_then(Json::as_u64), Some((5 + 1) * 6));
        assert!(r.get("warnings").is_none(), "nothing loading, nothing cut short: {r}");
        for f in features.iter().filter(|f| f.get("on").and_then(Json::as_bool) == Some(true)) {
            assert!(f.get("cost_ms").and_then(Json::as_f64).is_some() && f.get("without_ms").and_then(Json::as_f64).is_some_and(|v| v > 0.0) && f.get("to_change").is_some(), "{f}");
        }
        assert_eq!(r.get("as_it_is").and_then(|a| a.get("frames")).and_then(Json::as_u64), Some(5));
        assert!(r.get("as_it_is").and_then(|a| a.get("gpu_ms")).and_then(Json::as_f64).is_some_and(|v| v > 0.0));
        assert_eq!(r.get("without_all").and_then(|a| a.get("frames")).and_then(Json::as_u64), Some(5));
        assert_eq!(r.get("holds_budget").and_then(Json::as_bool), Some(true), "a second a frame holds");
        assert!(r.get("to_hold_budget").is_none());
        assert_eq!(r.get("size"), Some(&Json::arr([320, 180].map(Json::from))));
        // The passes of the view as it is name the shadows' own.
        let passes: Vec<&str> = r.get("passes").and_then(Json::as_array).unwrap().iter().filter_map(|p| p.get("pass").and_then(Json::as_str)).collect();
        assert!(passes.contains(&specula_gpu::PASS_SHADOW) && passes.contains(&specula_gpu::PASS_SCENE), "{passes:?}");
        assert!(r.get("engine_light").and_then(|l| l.get("has_not")).and_then(Json::as_array).is_some_and(|l| l.iter().any(|x| x.as_str().is_some_and(|s| s.contains("ray-tracing hardware")))));
        // The world is as it was, and no frame compiled a pipeline.
        assert_eq!(e.world.revision(), revision);
        assert!(e.world.env().light == env && e.captures.light.is_none());
        assert_eq!(e.gpu.last_frame_stats().pipelines.created_after_warm, 0, "a measuring render compiled a pipeline");

        // A job whose call's time is nearly out answers with what it has, and says so: three
        // seconds before the end, at the start of a round.
        let spec = parse(&args(r#"{"width": 320, "height": 180, "frames": 16, "eye": [4, 3, 6], "target": [1.5, 0, 0]}"#)).unwrap();
        let mut hurried = LightJob::new(&mut e, spec, &session("aaaaaa11"), Instant::now() + DEADLINE_MARGIN + std::time::Duration::from_millis(40)).unwrap();
        let out = loop {
            assert!(started.elapsed() < std::time::Duration::from_secs(240));
            if let Step::Done(o) = hurried.step(&mut e, &reply) {
                break o;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(!out.is_error, "{:?}", out.content);
        let r = out.content.iter().find_map(|c| match c {
            specula_protocol::Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        });
        let r = r.unwrap();
        let counted = r.get("as_it_is").and_then(|a| a.get("frames")).and_then(Json::as_u64).unwrap();
        assert!(hurried.cut_short && (1..16).contains(&counted), "{r}");
        assert!(r.get("warnings").and_then(Json::as_array).is_some_and(|w| w.iter().any(|x| x.as_str().is_some_and(|s| s.contains("of 16 rounds")))), "{r}");
    }

    #[test]
    fn a_median_is_the_middle() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(&[]), 0.0);
        // Some times the middle distance from the median (six for so few frames, three for
        // nine): one late frame of nine moves nothing.
        assert_eq!(scatter(&[1.0, 4.0, 2.0]), 6.0);
        assert_eq!(scatter(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), 8.0);
        assert_eq!(scatter(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]), 6.0);
        assert_eq!(scatter(&[]), 0.0);
        let steady = [2.56, 2.56, 2.55, 2.56, 2.57, 2.56, 2.86, 2.56, 2.55];
        assert!(scatter(&steady) < 0.04, "{}", scatter(&steady));
    }
}

//! The engine side of taste direction (docs/design/GALLERIES.md): galleries kept as project
//! files under `assets/taste/` (the files are the only truth: the engine's copies are parsed from
//! them again after any change, an undo included), decisions from the window and from agents,
//! the gallery worker that plans and renders rounds, the preference store's queries and
//! suggestions, reference boards, and the style configurator.
//!
//! Every write is a project-file commit (`Engine::project_put`): taste state (galleries, picks,
//! suggestions, boards, onboarding) under the taste rule, which only these savers pass and which
//! writes nothing else; no agent's claim applies to it and nobody undoes another's entry in it.
//! Approved results (style.toml, ui colours, priors, notes) are written with the taste state that
//! records the decision in one commit (`Engine::project_put_all`) under the taste result rule,
//! which writes project files only and checks claims on the results (one tool call, one commit:
//! AGENTS rule 14). All of it is journaled with its author. Who may do what is one function,
//! [`taste_authority`]. Rounds are planned and rendered on the worker's thread (renders go
//! through the studio's queue, within its budget), never the engine's.

use crate::journal::Writer;
use crate::review::{Capture, Check, SubmitSpec, WINDOW_OWNER};
use crate::schedule::Class;
use crate::studio::gallery::{self as render, RoundJob};
use crate::tools::{Command, Request};
use crate::world::WorldError;
use crate::Engine;
use specula_core::Json;
use specula_gustus::gallery::{Gallery, Kind, Made, Planned, Status};
// (Named so: the engine-thread guard reads `preferences::` as a file-system call.)
use specula_gustus::prefs as preferences;
use preferences::{Priors, SuggestionLog};
use specula_gustus::space::{ApplyAt, Value};
use specula_gustus::{presets, TasteError};
use specula_protocol::{SessionInfo, ToolOutput};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// The writer of rendered rounds (reserved: no agent may take it).
pub const TASTE_AGENT: &str = crate::authority::TASTE_AGENT;
/// Taste state's rule: only the savers of galleries, suggestions, boards and onboarding pass it
/// (never derived from a path: taste review 2).
const TASTE_RULE: crate::write_policy::Rule = crate::write_policy::Rule::Taste;
/// A decision's results and the taste state that records it, in one commit (one tool call, one
/// commit: gallery_apply, an applied suggestion, an approved configurator proposal).
const DECIDED_RULE: crate::write_policy::Rule = crate::write_policy::Rule::TasteResult;
pub const GALLERIES: &str = "assets/taste/galleries/";
pub const SUGGESTIONS: &str = "assets/taste/suggestions.json";
/// Approved results are tracked (outside the private taste folder).
pub const PRIORS: &str = "assets/style/priors.json";
pub const ONBOARDING: &str = "assets/taste/onboarding.json";
pub const NOTES: &str = "assets/style/notes.md";
pub const STYLE_TOML: &str = "assets/style.toml";
/// Galleries a project keeps.
pub const MAX_GALLERIES: usize = 512;
/// Galleries the pump starts or looks at per second (each kind).
const PUMP_TAKES: usize = 4;
/// How long the pump waits for the world's models and streaming to settle before making rounds.
const SETTLE_WAIT: Duration = Duration::from_secs(20);
/// A round being made longer than this is given up (the next pump may start it again).
const RENDER_TIMEOUT: Duration = Duration::from_secs(120);
/// Review batches that write taste results when approved.
pub const SUGGESTION_BATCH: &str = "taste:";
pub const ONBOARDING_BATCH: &str = "taste_onboarding";
/// Who someone is to the taste tools, from the engine's one authority ([`crate::authority`]:
/// operator-granted, never self-declared). [`taste_authority`] is the only mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TasteAuthority {
    /// The human (the window, or an agent the operator granted owner): picks, applies, approves
    /// taste results, deletes any gallery.
    Owner,
    /// Puts work on the owner's screen (a granted lead or reviewer, the engine's
    /// "puts work on the owner's screen" authority): opens and closes the gallery screen,
    /// supersedes and deletes any gallery, dismisses suggestions, cancels the configurator.
    Opener,
    /// Any other agent ("art_director" and the like are job labels): makes galleries, and does
    /// to its own what an opener does.
    Agent,
}

impl TasteAuthority {
    pub fn name(self) -> &'static str {
        match self {
            TasteAuthority::Owner => crate::authority::Authority::Owner.name(),
            TasteAuthority::Opener => "opener",
            TasteAuthority::Agent => crate::authority::Authority::Agent.name(),
        }
    }

    /// Opens, closes, supersedes and deletes galleries that aren't its own.
    pub fn directs(self) -> bool {
        matches!(self, TasteAuthority::Owner | TasteAuthority::Opener)
    }
}

/// The taste tools' view of the engine's authority: Owner is the owner; a lead or a reviewer
/// (both granted by the operator; a reviewer already puts work on the owner's screen) opens and
/// directs galleries; anyone else is an agent.
pub fn taste_authority(a: crate::authority::Authority) -> TasteAuthority {
    use crate::authority::Authority;
    match a {
        Authority::Owner => TasteAuthority::Owner,
        Authority::Lead | Authority::Reviewer => TasteAuthority::Opener,
        Authority::Agent => TasteAuthority::Agent,
    }
}

/// A rendered sheet: its file, the tiles' boxes on it and its size.
pub type SheetLayout = (PathBuf, Vec<[u32; 4]>, (u32, u32));

/// Review batches the taste tools own.
pub fn is_taste_batch(batch: &str) -> bool {
    batch.starts_with("taste")
}

pub fn gallery_path(id: &str) -> String {
    format!("{GALLERIES}{id}.json")
}

pub fn board_path(board: &str) -> String {
    format!("assets/taste/boards/{board}.json")
}

/// Validates a `taste` project file by what it is (its place in the folder).
pub fn validate_file(path: &str, text: &str) -> Result<(), TasteError> {
    let rel = path.strip_prefix("assets/").unwrap_or(path);
    if let Some(name) = rel.strip_prefix("taste/galleries/") {
        let g = Gallery::parse(text)?;
        if name.strip_suffix(".json") != Some(g.id.as_str()) {
            return Err(TasteError::new("bad_id", format!("the gallery's id '{}' must be its file's name", g.id)).at("id"));
        }
        return Ok(());
    }
    if rel.starts_with("taste/boards/") {
        return specula_gustus::board::Board::parse(text).map(|_| ());
    }
    match rel {
        "taste/suggestions.json" => SuggestionLog::parse(text).map(|_| ()),
        "taste/onboarding.json" => presets::Onboarding::parse(text).map(|_| ()),
        _ => Json::parse(text).map(|_| ()).map_err(|e| TasteError::new("invalid_json", e.to_string())),
    }
}

/// Engine calls of the taste tools (see `taste_tools`).
pub enum TasteCall {
    Create { gallery: Box<Gallery>, start: Option<Json>, open: bool, user: String, wait: bool },
    Get { id: String, round: Option<usize> },
    Delete { id: String },
    List { status: Option<String>, after: Option<String>, limit: usize },
    Open { id: String, user: String },
    Close,
    Decide { id: String, round: usize, decision: Decision, user: String },
    Round { id: String, after: usize, points: Vec<Vec<f64>> },
    Apply { id: String },
    Query(preferences::Query),
    Suggest { action: String, id: Option<String> },
    BoardPut { board: String, text: String },
    BoardGet { board: String },
    Camera { name: String, of: Option<String> },
    /// `open`: `Some(true)` asks for the window (a try at it), `None` opens it only for a caller who may.
    Onboard { action: String, subject: Option<Json>, preset: Option<String>, user: String, open: Option<bool>, quote_notes: Option<bool> },
    /// The worker made a round.
    RoundReady { id: String, planned: Planned, result: Result<Rendered, Json>, rerender: bool },
    /// The window's "Try again" (or an agent's gallery_round): a failed round is made again.
    Retry { id: String },
}

/// A decision as the tools give it.
pub enum Decision {
    Kind(Kind),
    Dislike { variant: usize, at: Option<[f64; 2]>, note: String },
    /// The window's "Try again" on a round that couldn't be made.
    Retry,
}

/// A rendered round, as the worker sends it back.
pub struct Rendered {
    pub sheet: String,
    pub path: PathBuf,
    pub packet: PathBuf,
    pub measures: Vec<Option<Json>>,
    pub tiles: Vec<[u32; 4]>,
    pub size: (u32, u32),
    pub cost: Json,
    /// Tiles that render alike although their values differ.
    pub warning: Option<Json>,
}

impl TasteCall {
    pub fn class(&self) -> Class {
        match self {
            TasteCall::Get { .. } | TasteCall::List { .. } | TasteCall::Query(_) | TasteCall::BoardGet { .. } | TasteCall::Camera { .. } => Class::Read,
            _ => Class::Edit,
        }
    }
}

/// A round being made: which round it follows, since when.
struct Busy {
    after: usize,
    since: Instant,
}

/// Work for the gallery worker: a round to plan (or the one given) and render; `rerender`: the
/// gallery's last round again (its sheet is gone).
struct Work {
    gallery: Gallery,
    plan: Option<Planned>,
    rerender: bool,
}

/// A round that couldn't be made, and when it is tried again (`None`: not by itself).
pub(crate) struct Failure {
    pub(crate) error: Json,
    attempts: u32,
    retry_at: Option<Instant>,
}

/// Error codes a render won't recover from by trying again (the gallery or its subject is wrong:
/// an agent fixes it, or the human presses "Try again" after it is fixed).
const PERMANENT: [&str; 9] = ["bad_subject", "bad_points", "bad_target", "nothing_to_plan", "too_many_rounds", "invalid_recipe", "recipe_not_found", "bad_recipe", "too_many_renders"];
/// Attempts before a transient failure waits for a person.
const MAX_ATTEMPTS: u32 = 6;

/// When to try a failed round again: transient failures (the studio busy, a timed-out or
/// dropped render, a GPU hiccup) back off 2, 4, 8... seconds (at most 60) and give up after
/// [`MAX_ATTEMPTS`]; permanent ones wait for a person.
pub(crate) fn retry_after(code: &str, attempts: u32) -> Option<Duration> {
    (!PERMANENT.contains(&code) && attempts < MAX_ATTEMPTS).then(|| Duration::from_secs((2u64 << attempts.min(5)).min(60)))
}

/// The engine's taste state (see the module docs).
pub struct Taste {
    /// Galleries parsed from the store: id -> (file version, gallery, its preference records).
    cache: BTreeMap<String, (u64, Gallery, Vec<preferences::Record>)>,
    /// Gallery files that don't parse: path -> why (shown by gallery_list).
    bad: BTreeMap<String, String>,
    /// The store's files revision the cache was built at.
    rev: u64,
    busy: BTreeMap<String, Busy>,
    /// The last failure making a gallery's round.
    failures: BTreeMap<String, Failure>,
    /// Sheets looked for on disk and not found, and when (looked for again after a while).
    missing: BTreeMap<(String, usize), Instant>,
    /// Sheets being read back from their packets on a worker.
    loading: std::collections::BTreeSet<(String, usize)>,
    /// Rendered sheets: (id, round) -> (path, tile boxes, size), and what rendering cost.
    pub(crate) sheets: BTreeMap<(String, usize), SheetLayout>,
    costs: BTreeMap<String, Json>,
    /// Open suggestions as of the cached galleries (cleared when any file changes).
    suggestions: Option<Vec<preferences::Suggestion>>,
    /// A round whose tiles render alike: (gallery, round) -> the warning ([`crate::studio::gallery`]).
    warnings: BTreeMap<String, (usize, Json)>,
    /// The moment a studio call's scene tiles are drawn at: (call, window time).
    pub(crate) scene_time: Option<(u64, f64)>,
    worker: Option<mpsc::Sender<Work>>,
    /// The engine's request channel and the studio's state, for the worker's own tools.
    to_engine: Option<mpsc::Sender<Request>>,
    studio: Option<Arc<crate::studio::Shared>>,
    /// The window's gallery screen.
    pub(crate) screen: crate::gallery_screen::GalleryUi,
    /// When rounds waiting for their render were last looked for.
    last_pump: Option<Instant>,
    /// Since when the world has been building models or streaming (the pump waits for it).
    unsettled_since: Option<Instant>,
}

impl Default for Taste {
    fn default() -> Self {
        Taste {
            cache: BTreeMap::new(),
            bad: BTreeMap::new(),
            rev: u64::MAX,
            busy: BTreeMap::new(),
            failures: BTreeMap::new(),
            missing: BTreeMap::new(),
            loading: std::collections::BTreeSet::new(),
            sheets: BTreeMap::new(),
            costs: BTreeMap::new(),
            suggestions: None,
            warnings: BTreeMap::new(),
            scene_time: None,
            worker: None,
            to_engine: None,
            studio: None,
            screen: crate::gallery_screen::GalleryUi::default(),
            last_pump: None,
            unsettled_since: None,
        }
    }
}

impl Taste {
    /// Where the worker sends its renders and results (set once the MCP server's channel exists).
    pub(crate) fn connect(&mut self, to_engine: mpsc::Sender<Request>, studio: Arc<crate::studio::Shared>) {
        self.to_engine = Some(to_engine);
        self.studio = Some(studio);
    }

    fn worker(&mut self) -> Option<&mpsc::Sender<Work>> {
        if self.worker.is_none() {
            let (to_engine, shared) = (self.to_engine.clone()?, Arc::clone(self.studio.as_ref()?));
            let (tx, rx) = mpsc::channel::<Work>();
            let spawned = std::thread::Builder::new().name("specula-gallery".into()).spawn(move || {
                let tools = crate::tools::Tools::new(to_engine, Vec::new()).with_studio(Arc::clone(&shared));
                let session = render::worker_session();
                while let Ok(w) = rx.recv() {
                    let id = w.gallery.id.clone();
                    let rerender = w.rerender;
                    let planned = match w.plan {
                        Some(p) => Some(p),
                        None if w.gallery.rounds.is_empty() => None,
                        None => w.gallery.plan_next(),
                    };
                    let Some(planned) = planned else {
                        tools.post(&session, Command::Taste(TasteCall::RoundReady { id, planned: Planned::new(usize::MAX, Made::Agent, vec![], None), result: Err(Json::obj([("error", Json::from("nothing_to_plan"))])), rerender }));
                        continue;
                    };
                    let tile = w.gallery.subject.get("tile").and_then(Json::as_u64).map_or(render::DEFAULT_TILE, |t| t as u32);
                    // Look-alike renders are replaced only in rounds the engine planned and no
                    // chips steer (a re-render keeps what the round already holds).
                    let chips = w.gallery.rounds.last().and_then(|r| r.decision.as_ref()).is_some_and(|d| matches!(&d.kind, specula_gustus::gallery::Kind::Edit { chips, .. } if !chips.is_empty()));
                    let planner = matches!(planned.made, Made::Spread | Made::Tune | Made::Line | Made::Around | Made::None);
                    let replace_alike = planner && !chips && !rerender;
                    let job = RoundJob { n: planned.after + 1, points: planned.points.clone(), incumbent: planned.incumbent, tile, gallery: w.gallery, replace_alike, exact: rerender };
                    let mut planned = planned;
                    let result = render::render_round(&tools, &shared, &session, &job).map(|o| {
                        // What was rendered is what the round holds (palette rules may have
                        // pulled a colour back).
                        planned.points = o.points;
                        planned.incumbent = o.incumbent;
                        planned.fit = o.fit;
                        Rendered { sheet: o.sheet, path: o.path, packet: o.packet, measures: o.measures, tiles: o.tiles, size: o.size, cost: o.cost, warning: o.warning }
                    });
                    if !tools.post(&session, Command::Taste(TasteCall::RoundReady { id, planned, result, rerender })) {
                        break;
                    }
                }
            });
            match spawned {
                Ok(_) => self.worker = Some(tx),
                Err(e) => specula_core::trace_event!(&crate::trace_sink::JOBS, Warn, "gallery.no_worker", error = e.to_string()),
            }
        }
        self.worker.as_ref()
    }

    /// Every gallery, oldest first (the cache must be fresh: `Engine::taste_refresh`).
    pub(crate) fn galleries(&self) -> impl DoubleEndedIterator<Item = &Gallery> {
        self.cache.values().map(|(_, g, _)| g)
    }

    /// Every gallery's preference records (built once per file version).
    pub(crate) fn records(&self) -> impl Iterator<Item = &preferences::Record> {
        self.cache.values().flat_map(|(_, _, r)| r.iter())
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Gallery> {
        self.cache.get(id).map(|(_, g, _)| g)
    }

    /// Parses gallery files that changed since revision `rev` was last seen: every file whose
    /// version differs from the cached one (a gallery just saved is cached at its new version,
    /// so it isn't parsed again; any other change is).
    pub(crate) fn refresh(&mut self, files: &BTreeMap<String, crate::project::ProjectFile>, rev: u64) {
        if rev == self.rev {
            return;
        }
        self.rev = rev;
        self.suggestions = None;
        let mut seen = Vec::new();
        for (path, f) in files.range::<str, _>((std::ops::Bound::Included(GALLERIES), std::ops::Bound::Unbounded)) {
            if !path.starts_with(GALLERIES) {
                break;
            }
            let Some(id) = path.strip_prefix(GALLERIES).and_then(|n| n.strip_suffix(".json")) else { continue };
            seen.push(id.to_owned());
            if self.cache.get(id).is_some_and(|(v, _, _)| *v == f.version) {
                continue;
            }
            match Gallery::parse(&f.text) {
                Ok(g) if g.id == id => {
                    self.bad.remove(path);
                    let records = preferences::records(&g);
                    self.cache.insert(id.to_owned(), (f.version, g, records));
                }
                Ok(g) => {
                    self.bad.insert(path.clone(), format!("its id '{}' isn't its file name", g.id));
                    self.cache.remove(id);
                }
                Err(e) => {
                    self.bad.insert(path.clone(), e.to_string());
                    self.cache.remove(id);
                }
            }
        }
        self.cache.retain(|id, _| seen.contains(id));
        self.bad.retain(|p, _| p.strip_prefix(GALLERIES).and_then(|n| n.strip_suffix(".json")).is_some_and(|id| seen.iter().any(|s| s == id)));
    }

    /// What the pump looks at, chosen without cloning a gallery (taste review 2: cloning every
    /// open gallery once a second cost 62 ms at 110 maxed galleries, up to 910 ms at 512): at
    /// most [`PUMP_TAKES`] galleries whose round should be made, and at most [`PUMP_TAKES`] open
    /// ones whose sheet isn't known and isn't being read back.
    pub(crate) fn pump_candidates(&self, due: &[String], _now: Instant) -> (Vec<String>, Vec<String>) {
        let waiting = |id: &str| self.busy.contains_key(id) || self.failures.get(id).is_some_and(|f| !due.iter().any(|d| d == id) || f.retry_at.is_none());
        let want = self.galleries().filter(|g| (g.rounds.is_empty() || g.status() == Status::Rendering) && !waiting(&g.id)).take(PUMP_TAKES).map(|g| g.id.clone()).collect();
        let open = self
            .galleries()
            .filter(|g| g.status() == Status::Open && !g.rounds.is_empty() && !waiting(&g.id))
            .filter(|g| {
                let key = (g.id.clone(), g.rounds.len());
                !self.sheets.contains_key(&key) && !self.loading.contains(&key)
            })
            .take(PUMP_TAKES)
            .map(|g| g.id.clone())
            .collect();
        (want, open)
    }

    /// Drops what this run knows about a deleted gallery (its sheets, costs, failures).
    pub(crate) fn forget(&mut self, id: &str) {
        self.sheets.retain(|(g, _), _| g != id);
        self.missing.retain(|(g, _), _| g != id);
        self.loading.retain(|(g, _)| g != id);
        self.busy.remove(id);
        self.failures.remove(id);
        self.costs.remove(id);
        self.warnings.remove(id);
    }

    /// A gallery just written at `version` (taken as written: no parse).
    pub(crate) fn saved(&mut self, g: &Gallery, version: u64) {
        self.cache.insert(g.id.clone(), (version, g.clone(), preferences::records(g)));
        self.suggestions = None;
    }

    pub(crate) fn is_busy(&self, id: &str) -> bool {
        self.busy.contains_key(id)
    }

    pub(crate) fn failure(&self, id: &str) -> Option<&Json> {
        self.failures.get(id).map(|f| &f.error)
    }

    /// A round's sheet as known (rendered or read this run); never reads the disk.
    pub(crate) fn sheet(&self, g: &Gallery, n: usize) -> Option<SheetLayout> {
        self.sheets.get(&(g.id.clone(), n)).cloned()
    }

    /// Reads a round's sheet layout back from its packet (a sheet of an earlier run in this
    /// captures folder).
    fn load_sheet(g: &Gallery, n: usize, captures: &std::path::Path) -> Option<SheetLayout> {
        let r = g.rounds.get(n.checked_sub(1)?)?;
        let path = captures.join(r.sheet.as_ref()?);
        // Read the layout back from the packet next to it (sheets of earlier runs).
        let packet = Json::parse(&std::fs::read_to_string(path.with_extension("json")).ok()?).ok()?;
        // Only this round's sheet: another project sharing the captures folder may have written
        // a sheet of the same name (taste review 2), so the tiles' values must be the round's.
        let tiles = packet.get("tiles")?.as_array()?;
        if packet.get("gallery").and_then(Json::as_str) != Some(g.id.as_str())
            || tiles.len() != r.variants.len()
            || tiles.iter().zip(&r.variants).any(|(t, v)| t.get("values") != Some(&g.space.values_json(&v.values)))
        {
            return None;
        }
        let size = packet.get("size")?.as_array()?;
        let size = (size.first()?.as_u64()? as u32, size.get(1)?.as_u64()? as u32);
        let tiles = packet.get("tiles")?.as_array()?.iter().filter_map(|t| {
            let b = t.get("box")?.as_array()?;
            Some([b.first()?.as_u64()? as u32, b.get(1)?.as_u64()? as u32, b.get(2)?.as_u64()? as u32, b.get(3)?.as_u64()? as u32])
        }).collect();
        path.is_file().then_some((path, tiles, size))
    }
}

/// What the configurator's proposal says about the owner's notes: quoted into the tracked
/// notes.md only by opt-in, and then shown here, word for word, before the owner approves
/// (taste review 2: notes were quoted without being shown).
fn notes_line(steps: &[presets::StepResult], quote: bool) -> String {
    let notes: Vec<&String> = steps.iter().flat_map(|s| s.notes.iter()).filter(|n| !n.is_empty()).collect();
    match (notes.is_empty(), quote) {
        (true, _) => "There were no notes.".into(),
        (false, false) => format!("Your {} note{} stay private (in taste/): notes.md says what was chosen, never your words.", notes.len(), if notes.len() == 1 { "" } else { "s" }),
        (false, true) => format!("Your notes are quoted into it, word for word (quote_notes): {}", notes.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join("; ")),
    }
}

fn fail(e: &WorldError) -> ToolOutput {
    crate::world_error(e)
}

fn taste_err(e: TasteError) -> WorldError {
    let mut w = WorldError::new(e.code, e.message);
    if let Some(f) = e.field {
        w = w.with("field", f.as_str());
    }
    w
}

impl Engine {
    /// Parses gallery files that changed since the last look (after any write, import or undo).
    pub(crate) fn taste_refresh(&mut self) {
        let rev = self.world.files_revision();
        self.taste.refresh(self.world.files(), rev);
    }

    /// Writes a gallery's file as `w`. The cache takes the gallery as written (no parse: a
    /// maxed gallery took 22 ms to parse on every decision); every other file that changed is
    /// parsed again by the refresh (taste review 2: setting the revision here hid other
    /// galleries' changes until a restart).
    fn taste_save(&mut self, g: &Gallery, w: Writer) -> Result<Json, WorldError> {
        let text = g.to_text();
        let out = self.project_put(&gallery_path(&g.id), |_| Ok(text), w, w.name, TASTE_RULE)?;
        self.taste_saved(g);
        Ok(out)
    }

    /// The cache takes gallery `g` as just written (see [`Engine::taste_save`]).
    fn taste_saved(&mut self, g: &Gallery) {
        if let Some(v) = self.world.files().get(&gallery_path(&g.id)).map(|f| f.version) {
            self.taste.saved(g, v);
        }
        self.taste_refresh();
    }

    /// Gallery `g`, marked applied by `w`, written with its results `by_file` in **one** commit
    /// (one tool call, one commit: the files and the record that they were applied stand or fall
    /// together). The written files' `{file, version}`.
    fn taste_save_applied(&mut self, g: &mut Gallery, by_file: FileEdits, w: Writer) -> Result<Vec<Json>, WorldError> {
        let before = g.applied.replace(w.name.to_owned());
        let text = g.to_text();
        let mut puts: Vec<(String, crate::project_engine::PutEdit<'_>)> = by_file.into_iter().map(|(file, edits)| (file, Box::new(move |old: Option<&str>| apply_edits(old, &edits)) as crate::project_engine::PutEdit<'_>)).collect();
        let record = gallery_path(&g.id);
        puts.push((record.clone(), Box::new(move |_| Ok(text))));
        match self.project_put_all(puts, w, w.name, DECIDED_RULE, &[record.as_str()]) {
            Ok(mut out) => {
                out.pop();
                self.taste_saved(g);
                Ok(out.into_iter().map(|a| Json::obj([("file", a.get("path").cloned().unwrap_or(Json::Null)), ("version", a.get("version").cloned().unwrap_or(Json::Null))])).collect())
            }
            Err(e) => {
                g.applied = before;
                Err(e)
            }
        }
    }

    /// A round's sheet by gallery id: known this run, or read back from its packet on a worker
    /// (looked for again at most every 2 s while missing: the window asks every frame).
    fn taste_sheet_of(&mut self, id: &str, n: usize) -> Option<SheetLayout> {
        let key = (id.to_owned(), n);
        if let Some(s) = self.taste.sheets.get(&key) {
            return Some(s.clone());
        }
        if self.taste.loading.contains(&key) || self.taste.missing.get(&key).is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return None;
        }
        self.taste_load_sheet(key);
        None
    }

    /// Reads a round's sheet layout back from its packet on a worker (a sheet of an earlier run
    /// in this captures folder): the engine thread never touches the file. Until it answers the
    /// sheet counts as loading; a sheet not found is `missing`, which the pump renders again.
    fn taste_load_sheet(&mut self, key: (String, usize)) {
        let Some(g) = self.taste.get(&key.0).cloned() else { return };
        let captures = self.capture_dir.clone();
        let n = key.1;
        self.taste.loading.insert(key.clone());
        let job = crate::workers::Offload::engine(crate::workers::JobKind::SheetRead);
        let then = move |e: &mut Engine, r: Result<Option<SheetLayout>, crate::workers::Failed>| {
            e.taste.loading.remove(&key);
            match r.ok().flatten() {
                Some(s) => {
                    e.taste.missing.remove(&key);
                    e.taste.sheets.insert(key, s);
                    e.taste.screen.changed();
                }
                None => {
                    e.taste.missing.insert(key, Instant::now());
                }
            }
        };
        if self.offload(job, move |_| Taste::load_sheet(&g, n, &captures), then).is_err() {
            // (The engine's own jobs aren't refused; nothing to do if one were.)
        }
    }

    /// Notes a failure making a round: transient ones are tried again by the pump.
    fn taste_failed(&mut self, id: &str, error: Json) {
        let code = error.get("error").and_then(Json::as_str).unwrap_or("").to_owned();
        let attempts = self.taste.failures.get(id).map_or(0, |f| f.attempts) + 1;
        let retry_at = retry_after(&code, attempts - 1).map(|d| Instant::now() + d);
        specula_core::trace_event!(&crate::trace_sink::JOBS, Warn, "gallery.round_failed", gallery = id, code = code.as_str(), attempt = attempts, retry = retry_at.is_some());
        self.taste.failures.insert(id.to_owned(), Failure { error, attempts, retry_at });
        self.taste.screen.changed();
    }

    /// Asks the worker for a gallery's next round (`plan`: an agent's points; else planned from
    /// the gallery's last decision, or round 1 when it has none).
    fn taste_enqueue(&mut self, g: &Gallery, plan: Option<Planned>) {
        let after = g.rounds.len();
        if self.taste.busy.get(&g.id).is_some_and(|b| b.after == after && b.since.elapsed() < RENDER_TIMEOUT) {
            return;
        }
        let plan = match plan {
            Some(p) => Some(p),
            None if g.rounds.is_empty() => {
                let priors = self.world.files().get(PRIORS).and_then(|f| Priors::parse(&f.text).ok());
                let start = g.subject.get("start").and_then(|s| g.space.parse_values(s).ok()).map(|v| g.space.point_of(&v)).or_else(|| priors.and_then(|p| p.start(&g.space)));
                Some(g.first_round(start))
            }
            None => None,
        };
        let Some(tx) = self.taste.worker() else {
            self.taste_failed(&g.id, Json::obj([("error", Json::from("no_worker")), ("message", Json::from("the gallery worker isn't running (headless engines without a GPU can't render galleries)"))]));
            return;
        };
        if tx.send(Work { gallery: g.clone(), plan, rerender: false }).is_ok() {
            self.taste.busy.insert(g.id.clone(), Busy { after, since: Instant::now() });
        }
    }

    /// Renders a gallery's open round again (its sheet is gone: a fresh captures folder).
    fn taste_rerender(&mut self, g: &Gallery) {
        let Some(r) = g.rounds.last() else { return };
        let mut plan = Planned::new(r.n - 1, r.made.clone(), r.variants.iter().map(|v| v.point.clone()).collect(), r.incumbent);
        plan.chips_exhausted = r.chips_exhausted;
        let Some(tx) = self.taste.worker() else { return };
        if tx.send(Work { gallery: g.clone(), plan: Some(plan), rerender: true }).is_ok() {
            self.taste.busy.insert(g.id.clone(), Busy { after: r.n, since: Instant::now() });
        }
    }

    /// Once per loop (cheap): galleries whose next round isn't being made start it (after a
    /// restart, or a render that failed or timed out); a round whose sheet is gone (an earlier
    /// run's captures were tidied) is rendered again.
    pub(crate) fn taste_pump(&mut self) {
        if self.taste.last_pump.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.taste.last_pump = Some(Instant::now());
        // Rounds made again after a restart wait until the world's models are built and its
        // cells streamed (a sight-skeptic finding: the first scene tiles after a restart were
        // drawn before the graveyard's models existed), but never longer than SETTLE_WAIT.
        let settling = self.library.lock().in_flight() > 0 || self.stream.busy();
        let since = *self.taste.unsettled_since.get_or_insert_with(Instant::now);
        if settling && since.elapsed() < SETTLE_WAIT {
            return;
        }
        if !settling {
            self.taste.unsettled_since = None;
        }
        self.taste_refresh();
        self.taste.busy.retain(|_, b| b.since.elapsed() < RENDER_TIMEOUT);
        let now = Instant::now();
        // Failures whose retry is due are cleared (and so tried again below).
        let due: Vec<String> = self.taste.failures.iter().filter(|(_, f)| f.retry_at.is_some_and(|t| t <= now)).map(|(k, _)| k.clone()).collect();
        let (want, open) = self.taste.pump_candidates(&due, now);
        for id in want {
            if let Some(g) = self.taste.get(&id).cloned() {
                self.taste_enqueue(&g, None);
            }
        }
        // Open rounds whose sheet is gone (a restart on a fresh captures folder) render again.
        // A sheet never looked for is read back first (on a worker); one looked for and not
        // found is rendered again.
        for id in open {
            let n = self.taste.get(&id).map_or(0, |g| g.rounds.len());
            let key = (id.clone(), n);
            if self.taste.missing.contains_key(&key) {
                if let Some(g) = self.taste.get(&id).cloned() {
                    self.taste_rerender(&g);
                }
            } else {
                self.taste_load_sheet(key);
            }
        }
    }

    /// A round the worker made: appended to its gallery (if it still follows the last round).
    fn taste_round_ready(&mut self, id: String, planned: Planned, result: Result<Rendered, Json>, rerender: bool) -> ToolOutput {
        self.taste.busy.remove(&id);
        self.taste_refresh();
        let r = match result {
            Ok(r) => r,
            Err(e) => {
                self.taste_failed(&id, e);
                return ToolOutput::text("{}");
            }
        };
        self.taste.failures.remove(&id);
        let Some(mut g) = self.taste.get(&id).cloned() else { return ToolOutput::text("{}") };
        let n = planned.after + 1;
        if rerender {
            // The same round's sheet, made again: only if that round is still the one shown.
            let same = g.rounds.get(n - 1).is_some_and(|rd| rd.variants.iter().map(|v| &v.point).eq(planned.points.iter()));
            if same {
                self.taste.missing.remove(&(id.clone(), n));
                self.taste.sheets.insert((id, n), (r.path, r.tiles, r.size));
                self.taste.screen.changed();
            }
            return ToolOutput::text("{}");
        }
        if let Err(e) = g.push_round(planned, Some(r.sheet.clone()), r.measures.clone()) {
            // Someone decided or replaced the round meanwhile: this render is stale.
            if e.code != "stale_round" {
                self.taste_failed(&id, e.to_json());
            }
            return ToolOutput::text("{}");
        }
        match self.taste_save(&g, Writer::new(TASTE_AGENT, None)) {
            Ok(_) => {
                self.taste.sheets.insert((id.clone(), n), (r.path, r.tiles, r.size));
                self.taste.costs.insert(id.clone(), r.cost);
                match r.warning {
                    Some(wn) => self.taste.warnings.insert(id.clone(), (n, wn)),
                    None => self.taste.warnings.remove(&id),
                };
                self.taste.screen.changed();
            }
            Err(e) => {
                specula_core::trace_event!(&crate::trace_sink::JOBS, Warn, "gallery.round_not_saved", gallery = id.as_str(), round = n, error = e.message.as_str());
                self.taste_failed(&id, e.to_json());
            }
        }
        ToolOutput::text("{}")
    }

    /// Clears a gallery's failure and makes its round again (the window's "Try again").
    pub(crate) fn taste_retry(&mut self, id: &str) -> Result<Json, WorldError> {
        let g = self.taste.get(id).cloned().ok_or_else(|| WorldError::new("gallery_not_found", format!("there is no gallery '{id}'")).with("id", id))?;
        self.taste.failures.remove(id);
        self.taste.busy.remove(id);
        match g.status() {
            Status::Open if !g.rounds.is_empty() => self.taste_rerender(&g),
            Status::Accepted => {}
            _ => self.taste_enqueue(&g, None),
        }
        self.taste.screen.changed();
        Ok(Json::obj([("id", Json::from(id)), ("status", Json::from("rendering"))]))
    }

    /// Records a decision (the window's, or an agent's with role `owner`) and starts the next
    /// round. "This one" on an onboarding step starts the next step; with `apply_on_accept` it
    /// writes the values too.
    pub(crate) fn taste_decide(&mut self, id: &str, round: usize, d: Decision, w: Writer, user: &str) -> Result<Json, WorldError> {
        self.taste_refresh();
        let mut g = self.taste.get(id).cloned().ok_or_else(|| WorldError::new("gallery_not_found", format!("there is no gallery '{id}' (gallery_list)")).with("id", id))?;
        let seq = match d {
            Decision::Kind(k) => g.decide(round, k, w.name, user).map_err(taste_err)?,
            Decision::Dislike { variant, at, note } => g.dislike(round, variant, at, &note, w.name, user).map_err(taste_err)?,
            Decision::Retry => return self.taste_retry(id),
        };
        let status = g.status();
        let mut j = Json::obj([("id", Json::from(id)), ("seq", Json::from(seq)), ("status", Json::from(status.name()))]);
        // "This one" with apply_on_accept: the decision and the values it writes are one commit
        // (one tool call, one commit). When the values can't be written the decision still is.
        let mut saved = false;
        if status == Status::Accepted && g.apply_on_accept {
            match apply_plan(&g, id).and_then(|(round, variant, by_file)| Ok((round, variant, self.taste_save_applied(&mut g, by_file, w)?))) {
                Ok((round, variant, files)) => {
                    saved = true;
                    j.set("applied", Json::obj([("round", Json::from(round)), ("variant", Json::from(variant)), ("files", Json::Arr(files))]));
                }
                Err(e) => {
                    j.set("apply_problem", e.to_json());
                }
            }
        }
        // A configurator step accepted: the decision and what follows it (the next step's
        // gallery and onboarding.json, or after the last step the proposal's state) are one
        // commit, as the decider (the M4 final review: the follow-up was written as `taste`, a
        // second commit the counter didn't see). When the follow-up can't be made the decision
        // is still written, alone.
        if status == Status::Accepted && !saved {
            if let Some(follow) = self.onboard_after_accept(&g, w) {
                match follow {
                    Ok(next) => {
                        saved = true;
                        j.set("onboarding", next);
                    }
                    Err(e) => {
                        j.set("onboarding", e.to_json());
                    }
                }
            }
        }
        if !saved {
            self.taste_save(&g, w)?;
        }
        if status == Status::Rendering {
            self.taste_enqueue(&g, None);
        }
        self.taste.screen.changed();
        Ok(j)
    }

    /// Writes an accepted gallery's values to their `apply` targets as `w`, with the gallery's
    /// record that they were applied: one commit, all or nothing.
    pub(crate) fn taste_apply(&mut self, id: &str, w: Writer) -> Result<Json, WorldError> {
        self.taste_refresh();
        let mut g = self.taste.get(id).cloned().ok_or_else(|| WorldError::new("gallery_not_found", format!("there is no gallery '{id}'")).with("id", id))?;
        let (round, variant, by_file) = apply_plan(&g, id)?;
        let written = self.taste_save_applied(&mut g, by_file, w)?;
        Ok(Json::obj([("round", Json::from(round)), ("variant", Json::from(variant)), ("files", Json::Arr(written))]))
    }

    /// The window's gallery screen: one frame of input. Modal while open.
    pub(crate) fn gallery_frame(&mut self, size: (u32, u32), under_review: bool) -> bool {
        let open = self.taste.screen.is_open();
        // Pads are polled even without a hosted game (LB + RB + X opens it); under the review
        // screen the gallery gets no input at all. Input and pads are borrowed, never copied, and
        // the shown gallery is read in place (taste review 2: three clones a frame).
        if !open {
            // Closed: only its toggle is read (F8, or LB + RB + X).
            let pads: &[specula_platform::gamepad::GamepadState] = if under_review { &[] } else { self.logic.pad_states(true) };
            let events: &[specula_platform::Event] = if under_review { &[] } else { &self.input };
            if !self.taste.screen.toggle_pressed(events, pads) {
                return false;
            }
            self.taste_refresh();
            let id = self
                .taste
                .screen
                .gallery()
                .filter(|id| self.taste.get(id).is_some_and(|g| g.status() != Status::Accepted))
                .map(str::to_owned)
                .or_else(|| self.taste.galleries().rfind(|g| g.status() != Status::Accepted).map(|g| g.id.clone()));
            let user = self.taste.screen.user().to_owned();
            // The human's own F8 (or pad combo): the owner's action, seen all the same.
            let seen = crate::inputguard::done(crate::inputguard::Channel::Screen, "gallery_screen.toggle", &crate::inputguard::Actor::Human, id.as_deref().unwrap_or(""));
            match id {
                Some(id) => self.taste.screen.open(&id, &user, seen),
                None => self.taste.screen.open_empty(seen),
            }
            // The press that opened it isn't the game's.
            self.input.clear();
            return true;
        }
        self.taste_refresh();
        let id = match self.taste.screen.gallery().filter(|id| self.taste.cache.contains_key(*id)) {
            Some(id) => Some(id.to_owned()),
            None => self.taste.galleries().rfind(|g| g.status() != Status::Accepted).map(|g| g.id.clone()),
        };
        let n = id.as_deref().and_then(|id| self.taste.get(id)).map_or(0, |g| g.rounds.len());
        let sheet = id.as_deref().and_then(|id| self.taste_sheet_of(id, n));
        let busy = id.as_deref().is_some_and(|id| self.taste.is_busy(id));
        let failure = id.as_deref().and_then(|id| self.taste.failure(id).cloned());
        let pads: &[specula_platform::gamepad::GamepadState] = if under_review { &[] } else { self.logic.pad_states(true) };
        let events: &[specula_platform::Event] = if under_review { &[] } else { &self.input };
        let g = id.as_deref().and_then(|id| self.taste.cache.get(id)).map(|(_, g, _)| g);
        let (acts, modal) = self.taste.screen.frame(events, pads, size, g, sheet, busy, failure);
        for a in acts {
            let crate::gallery_screen::Act { gallery, round, decision } = a;
            if matches!(decision, Decision::Retry) {
                let result = self.taste_retry(&gallery);
                self.taste.screen.report(&result);
                continue;
            }
            let user = self.taste.screen.user().to_owned();
            // The human decides, but what an agent-made gallery writes (its apply targets) is the
            // maker's choice: journaled as the window's with the maker as `via`, never bare owner
            // attribution (input sweep R1).
            let maker = self.taste.get(&gallery).map(|g| g.by.clone()).filter(|m| !m.is_empty() && m != WINDOW_OWNER);
            let w = Writer::new(WINDOW_OWNER, Some(WINDOW_OWNER)).via(maker.as_deref());
            let result = self.taste_decide(&gallery, round, decision, w, &user);
            self.taste.screen.report(&result);
        }
        if modal {
            self.input.clear();
        }
        modal
    }

    /// The gallery screen's quads (after the game's, before the review screen's).
    pub(crate) fn gallery_overlay(&mut self, size: (u32, u32), capture: bool, out: &mut Vec<specula_gpu::OverlayQuad>) {
        if !self.taste.screen.is_open() && !self.taste.screen.has_textures() {
            return;
        }
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        self.taste.screen.overlay(&mut self.gpu, size, capture, room, out);
    }

    /// A review submission was approved: taste results it carries are written as the decider.
    /// A taste submission was decided: approved, its result is written as the decider; turned
    /// down, the suggestion is dismissed (the configurator's proposal goes back to running). Only
    /// the submission the taste tools made counts (its id is recorded with it).
    pub(crate) fn taste_review_decided(&mut self, batch: Option<&str>, submission: u64, approved: bool, w: Writer) -> Option<Json> {
        let b = batch?;
        if let Some(sid) = b.strip_prefix(SUGGESTION_BATCH) {
            let log = self.suggestion_log();
            let ours = log.entries.get(sid).is_some_and(|(state, _, detail)| state == "submitted" && detail == &format!("review {submission}"));
            if !ours {
                return Some(WorldError::new("not_this_submission", format!("suggestion {sid} wasn't submitted as review {submission}; nothing was written")).to_json());
            }
            return Some(if approved {
                self.suggestion_apply(sid, w).unwrap_or_else(|e| e.to_json())
            } else {
                self.suggestion_log_set(sid, "dismissed", w, &format!("turned down in review {submission}")).map(|_| Json::obj([("dismissed", Json::from(sid))])).unwrap_or_else(|e| e.to_json())
            });
        }
        if b != ONBOARDING_BATCH {
            return None;
        }
        let o = self.onboarding().filter(|o| o.state == "proposed" && o.submission == Some(submission));
        let Some(mut o) = o else {
            return Some(WorldError::new("not_this_submission", format!("review {submission} isn't the style configurator's proposal; nothing was written")).to_json());
        };
        Some(if approved {
            self.onboard_write(w).unwrap_or_else(|e| e.to_json())
        } else {
            // Turned down: the configurator goes on (pick again, then finish).
            o.state = "running".into();
            o.submission = None;
            o.proposal = None;
            self.onboard_save(&o, w).map(|_| Json::obj([("onboarding", Json::from("running"))])).unwrap_or_else(|e| e.to_json())
        })
    }

    /// Answers a taste call on the engine thread.
    pub(crate) fn handle_taste(&mut self, call: TasteCall, session: &SessionInfo) -> ToolOutput {
        let by = self.swarm.agent_name(session);
        let key = self.swarm.key(session);
        let w = Writer::new(&by, Some(&key));
        let named = self.swarm.introduced(&session.id);
        // The operator's grants only (a lead nobody granted puts nothing on the human's window).
        let auth = taste_authority(crate::authority::trusted(&self.swarm, &session.id));
        // The owner or a granted lead (only they may take the window while the human types).
        let operator = crate::authority::trusted(&self.swarm, &session.id) >= crate::authority::Authority::Lead;
        // The human's in-progress note is the owner's alone (input sweep R3).
        let owner = auth == TasteAuthority::Owner;
        let actor = crate::inputguard::Actor::of(&self.swarm, session);
        let hello = |what: &str| fail(&WorldError::new("hello_required", format!("say agent_hello first: {what} are attributed to agents by name")));
        self.taste_refresh();
        match call {
            TasteCall::RoundReady { id, planned, result, rerender } => self.taste_round_ready(id, planned, result, rerender),
            TasteCall::Retry { id } => match self.taste_retry(&id) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Onboard { ref action, .. } if !named && action != "status" => hello("galleries, applied values, boards and onboarding"),
            TasteCall::Create { .. } | TasteCall::Round { .. } | TasteCall::Apply { .. } | TasteCall::BoardPut { .. } | TasteCall::Delete { .. } if !named => hello("galleries, applied values, boards and onboarding"),
            TasteCall::Create { gallery, start, open, user, wait: _ } => match self.taste_create(*gallery, start, session) {
                Ok(g) => {
                    let mut j = self.gallery_json(&g, None, owner);
                    if open {
                        j.set("screen", self.taste_open(&g.id, &user, auth, &actor, operator).unwrap_or_else(|e| e.to_json()));
                    }
                    ToolOutput::text(j.to_string())
                }
                Err(e) => fail(&e),
            },
            TasteCall::Get { id, round } => match self.taste.get(&id).cloned() {
                Some(g) => ToolOutput::text(self.gallery_json(&g, round, owner).to_string()),
                None => fail(&WorldError::new("gallery_not_found", format!("there is no gallery '{id}' (gallery_list)")).with("id", id.as_str())),
            },
            TasteCall::List { status, after, limit } => ToolOutput::text(self.gallery_list(status.as_deref(), after.as_deref(), limit).to_string()),
            TasteCall::Open { id, user } => match self.taste_open(&id, &user, auth, &actor, operator) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Close => {
                if !auth.directs() && self.taste.screen.gallery().and_then(|id| self.taste.get(id)).is_none_or(|g| g.by != by) {
                    return fail(&WorldError::new("not_allowed", "only the gallery's maker, the owner, or a granted lead or reviewer closes the human's gallery screen").with("authority", auth.name()));
                }
                self.taste.screen.close();
                ToolOutput::text(self.taste.screen.status_json(owner).to_string())
            }
            TasteCall::Decide { .. } if auth != TasteAuthority::Owner => fail(&WorldError::new(
                "owner_only",
                "picks are the human's taste: they decide at the window (gallery_open), or an agent speaking for them says the operator granted owner (--owner NAME)",
            )
            .with("authority", auth.name())),
            TasteCall::Delete { id } => match self.taste_delete(&id, auth, w) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Decide { id, round, decision, user } => match self.taste_decide(&id, round, decision, w, &user) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Round { id, after, points } => {
                let Some(mut g) = self.taste.get(&id).cloned() else { return fail(&WorldError::new("gallery_not_found", format!("there is no gallery '{id}'"))) };
                // Replacing a gallery's round is its maker's, or the owner's or an opener's
                // (taste review 2: any agent superseded the lead's open round).
                if !auth.directs() && g.by != by {
                    return fail(&WorldError::new("not_allowed", format!("gallery '{id}' is {}'s: its maker, the owner, or a granted lead or reviewer replaces its round", g.by)).with("authority", auth.name()));
                }
                if g.rounds.len() != after {
                    return fail(&WorldError::new("stale_round", format!("the gallery moved on to round {} meanwhile; read it again", g.rounds.len())).with("round", g.rounds.len()));
                }
                let mut plan = match g.supersede(&by) {
                    Ok(p) => p,
                    Err(e) => return fail(&taste_err(e)),
                };
                plan.points = points;
                if let Err(e) = self.taste_save(&g, w) {
                    return fail(&e);
                }
                self.taste.busy.remove(&id);
                self.taste_enqueue(&g, Some(plan));
                self.taste.screen.changed();
                ToolOutput::text(Json::obj([("id", Json::from(id.as_str())), ("status", Json::from("rendering")), ("round", Json::from(after + 1))]).to_string())
            }
            TasteCall::Apply { id } => match self.taste_apply(&id, w) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Query(q) => {
                let pending = self.taste_suggestions().len();
                let (page, total) = preferences::query_records(self.taste.records(), &q);
                let priors = self.world.files().get(PRIORS).and_then(|f| Priors::parse(&f.text).ok()).unwrap_or_default();
                let mut j = Json::obj([
                    ("records", Json::arr(page.iter().map(preferences::Record::to_json))),
                    ("total", Json::from(total)),
                    ("galleries", Json::from(self.taste.cache.len())),
                    ("priors", Json::obj(priors.by_target.iter().map(|(k, v)| (k.clone(), v.clone())))),
                    ("suggestions", Json::from(pending)),
                ]);
                if q.offset + page.len() < total {
                    j.set("next_offset", Json::from(q.offset + page.len()));
                }
                ToolOutput::text(j.to_string())
            }
            TasteCall::Suggest { action, id } => match self.suggest(&action, id.as_deref(), w, auth, named) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::BoardPut { board, text } => match self.project_put(&board_path(&board), |_| Ok(text), w, &by, TASTE_RULE) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::BoardGet { board } => match self.world.files().get(&board_path(&board)) {
                Some(f) => ToolOutput::text(Json::obj([("text", Json::from(&*f.text)), ("version", Json::from(f.version))]).to_string()),
                None => fail(&WorldError::new("board_not_found", format!("board '{board}' hasn't been measured: board_measure first")).with("board", board.as_str())),
            },
            TasteCall::Camera { name, of } => match self.taste_camera(&name, of.as_deref(), session) {
                Ok(c) => ToolOutput::text(render::camera_json(&c).to_string()),
                Err(e) => fail(&e),
            },
            TasteCall::Onboard { action, subject, preset, user, open, quote_notes } => match self.onboard(&action, subject, preset, &user, open, quote_notes, w, auth, session) {
                Ok(j) => ToolOutput::text(j.to_string()),
                Err(e) => fail(&e),
            },
        }
    }

    /// A named camera (the caller's, or agent `of`'s), or `window` (the human's view now).
    fn taste_camera(&mut self, name: &str, of: Option<&str>, session: &SessionInfo) -> Result<crate::sight::Camera, WorldError> {
        if name == "window" {
            return Ok(self.window_camera.unwrap_or_else(|| crate::sight::auto_camera(&self.world, 50.0)));
        }
        let owner = of.map_or_else(|| self.swarm.agent_name(session), str::to_owned);
        self.cameras.resolve(&owner, name, of, &self.world).map(|r| r.camera).map_err(|e| WorldError::new(e.code, e.message).with("camera", name))
    }

    /// Resolves a subject's named cameras into `{eye, target, fov_y_deg}` (kept in the file).
    fn taste_resolve_subject(&mut self, subject: &mut Json, session: &SessionInfo) -> Result<(), WorldError> {
        for key in ["scene", "ui"] {
            let Some(mut part) = subject.get(key).cloned() else { continue };
            let cam = if key == "scene" { Some(part.clone()) } else { part.get("camera").cloned() };
            let Some(cam) = cam else { continue };
            let resolved = match cam.get("camera").and_then(Json::as_str).or_else(|| cam.as_str()) {
                Some(name) => {
                    let of = cam.get("of").and_then(Json::as_str).map(str::to_owned);
                    let c = self.taste_camera(name, of.as_deref(), session)?;
                    let mut j = render::camera_json(&c);
                    j.set("camera", Json::from(name));
                    j
                }
                None => cam,
            };
            if key == "scene" {
                part = resolved;
            } else {
                part.set("camera", resolved);
            }
            subject.set(key, part);
        }
        Ok(())
    }

    fn taste_create(&mut self, mut g: Gallery, start: Option<Json>, session: &SessionInfo) -> Result<Gallery, WorldError> {
        if self.taste.cache.len() >= MAX_GALLERIES {
            return Err(WorldError::new("too_many_galleries", format!("the project keeps {MAX_GALLERIES} galleries; delete old ones with gallery_delete (yours, or any as the owner or a granted lead or reviewer)")));
        }
        if g.id.is_empty() {
            let mut n = self.taste.cache.len() + 1;
            while self.taste.cache.contains_key(&format!("g{n}")) || self.world.files().contains_key(&gallery_path(&format!("g{n}"))) {
                n += 1;
            }
            g.id = format!("g{n}");
        }
        if self.taste.cache.contains_key(&g.id) || self.world.files().contains_key(&gallery_path(&g.id)) {
            return Err(WorldError::new("gallery_exists", format!("gallery '{}' exists: pick another id, or continue it", g.id)).with("id", g.id.as_str()));
        }
        self.taste_resolve_subject(&mut g.subject, session)?;
        let subject = render::parse_subject(&g.subject).map_err(|m| WorldError::new("bad_subject", m).with("field", "subject"))?;
        render::check_targets(&subject, &g.space).map_err(|m| WorldError::new("bad_target", m).with("field", "space"))?;
        if let Some(s) = start {
            g.space.parse_values(&s).map_err(taste_err)?;
            g.subject.set("start", s);
        }
        let by = self.swarm.agent_name(session);
        let key = self.swarm.key(session);
        g.by = by.clone();
        self.taste_save(&g, Writer::new(&by, Some(&key)))?;
        self.taste_enqueue(&g, None);
        Ok(g)
    }

    /// Deletes a gallery (`gallery_delete`): its maker's to delete, or the owner's (their
    /// private history) or an opener's. Taste state, so under the taste rule: journaled, and
    /// undoable by whoever deleted it (taste review 2: the cap's advice, project_delete, is
    /// refused for taste state).
    fn taste_delete(&mut self, id: &str, auth: TasteAuthority, w: Writer) -> Result<Json, WorldError> {
        let g = self.taste.get(id).ok_or_else(|| WorldError::new("gallery_not_found", format!("there is no gallery '{id}'")).with("id", id))?;
        if !auth.directs() && g.by != w.name {
            return Err(WorldError::new("not_allowed", format!("gallery '{id}' is {}'s: its maker, the owner, or a granted lead or reviewer deletes it", g.by)).with("authority", auth.name()));
        }
        if self.onboarding().is_some_and(|o| (o.state == "running" || o.state == "proposed") && o.galleries.iter().any(|x| x == id)) {
            return Err(WorldError::new("onboarding_running", format!("gallery '{id}' is a step of the running style configurator: cancel it first (style_onboard {{action: \"cancel\"}})")).with("id", id));
        }
        let out = self.project_remove(&gallery_path(id), w, w.name, TASTE_RULE)?;
        self.taste.forget(id);
        if self.taste.screen.gallery() == Some(id) {
            self.taste.screen.close();
        }
        self.taste_refresh();
        Ok(Json::obj([("id", Json::from(id)), ("deleted", Json::Bool(true)), ("journal", out.get("journal").cloned().unwrap_or(Json::Null))]))
    }

    fn taste_open(&mut self, id: &str, user: &str, auth: TasteAuthority, actor: &crate::inputguard::Actor, operator: bool) -> Result<Json, WorldError> {
        use crate::inputguard::{declined, done, refused, Channel};
        if self.taste.get(id).is_none() {
            return Err(WorldError::new("gallery_not_found", format!("there is no gallery '{id}'")).with("id", id));
        }
        // Putting a gallery on the human's window is the owner's, a granted lead's or a
        // reviewer's: being its maker isn't enough (input sweep R1: the human's click became an
        // owner decision writing what the maker chose). A refused try is a security incident
        // (docs/design/SECURITY.md).
        if !auth.directs() {
            let e = crate::authority::refusal(
                crate::authority::Site::WindowScreen,
                "putting a gallery on the human's window needs the owner, a granted lead or a reviewer (its maker asks one of them: gallery_create without open, then tools_request or a note); a refused try freezes your session (docs/design/SECURITY.md)",
            )
            .with("authority", auth.name());
            refused(Channel::Screen, "gallery_open", actor, e.code, id);
            return Err(e);
        }
        // The human is typing into a text the operator pointed their keyboard at: only the
        // operator takes the window from them (review R4 of m5-text: keys meant for the text
        // picked a gallery variant).
        if !operator && self.human_typing() && self.taste.screen.gallery() != Some(id) {
            let e = crate::text_window::typing_busy("gallery_open");
            // The human's typing refused it, not the caller's choice: seen, never counted.
            declined(Channel::Screen, "gallery_open", actor, e.code, id);
            return Err(e);
        }
        if self.taste.screen.closed_by_human_recently() {
            return Err(WorldError::new("owner_closed", "the human closed the gallery screen moments ago; agents can't reopen it for 30 s (humans are never locked in)").with("retry_after_s", 30));
        }
        if !specula_gustus::valid_name(user) {
            return Err(WorldError::new("bad_user", "user is a label [a-z0-9_-] (\"owner\" by default)").with("field", "user"));
        }
        // The monitor sees it first: a lead's or a reviewer's open is an alert the owner is told of.
        let seen = done(Channel::Screen, "gallery_open", actor, id);
        self.taste.screen.open(id, user, seen);
        Ok(self.taste.screen.status_json(actor.authority() == crate::authority::Authority::Owner))
    }

    /// A gallery as data: its rounds, the shown round's variants (values and measures), the
    /// breadcrumb, the chips, its file and what it is doing.
    /// (`owner`: the caller is the owner, who alone sees the human's in-progress note.)
    pub(crate) fn gallery_json(&self, g: &Gallery, round: Option<usize>, owner: bool) -> Json {
        let n = round.unwrap_or(g.rounds.len()).min(g.rounds.len());
        let mut j = Json::obj([
            ("id", Json::from(g.id.as_str())),
            ("title", Json::from(g.title.as_str())),
            ("status", Json::from(if self.taste.is_busy(&g.id) && g.status() != Status::Accepted { "rendering" } else { g.status().name() })),
            ("subject", Json::from(preferences::subject_label(&g.subject))),
            ("mode", Json::from(g.mode.name())),
            ("rounds", Json::from(g.rounds.len())),
            ("seq", Json::from(g.seq)),
            ("file", Json::from(gallery_path(&g.id))),
            ("chips", Json::arr(g.space.chips().into_iter().map(Json::from))),
            ("crumbs", Json::arr(g.crumbs().into_iter().map(|(n, made, d)| Json::obj([("round", Json::from(n)), ("made", Json::from(made)), ("decision", Json::from(d))])))),
        ]);
        if let Some(r) = n.checked_sub(1).and_then(|i| g.rounds.get(i)) {
            let mut rj = Json::obj([("n", Json::from(r.n)), ("made", Json::from(match &r.made {
                Made::Back(k) => format!("back:{k}"),
                m => format!("{m:?}").to_lowercase(),
            }))]);
            if let Some(i) = r.incumbent {
                rj.set("incumbent", Json::from(i));
            }
            rj.set(
                "variants",
                Json::arr(r.variants.iter().enumerate().map(|(i, v)| {
                    let mut vj = Json::obj([("variant", Json::from(i)), ("values", g.space.values_json(&v.values))]);
                    if let Some(m) = &v.measures {
                        vj.set("measures", m.clone());
                    }
                    vj
                })),
            );
            if let Some(d) = &r.decision {
                let mut dj = Json::obj([("kind", Json::from(d.kind.name())), ("by", Json::from(d.by.as_str())), ("user", Json::from(d.user.as_str()))]);
                if let Some(v) = d.kind.picked() {
                    dj.set("variant", Json::from(v));
                }
                match &d.kind {
                    Kind::Edit { note, chips, .. } => {
                        dj.set("note", Json::from(note.as_str()));
                        dj.set("chips", Json::arr(chips.iter().map(|c| Json::from(c.as_str()))));
                    }
                    Kind::None { note } => {
                        dj.set("note", Json::from(note.as_str()));
                    }
                    Kind::Back { to } => {
                        dj.set("to", Json::from(*to));
                    }
                    _ => {}
                }
                rj.set("decision", dj);
            }
            if let Some(n) = r.fit {
                rj.set("fit", Json::from(n));
                rj.set("fit_note", Json::from(render::fit_note(n)));
            }
            if !r.dislikes.is_empty() {
                rj.set("dislikes", Json::arr(r.dislikes.iter().map(|d| Json::obj([("variant", Json::from(d.variant)), ("note", Json::from(d.note.as_str()))]))));
            }
            if let Some((path, _, _)) = self.taste.sheet(g, r.n) {
                rj.set("sheet", Json::from(path.display().to_string()));
                rj.set("packet", Json::from(path.with_extension("json").display().to_string()));
            }
            j.set("round", rj);
        }
        if let Some((_, _, vals)) = g.accepted() {
            j.set("accepted", g.space.values_json(vals));
            if let Some(a) = &g.applied {
                j.set("applied_by", Json::from(a.as_str()));
            }
            let applies: Vec<Json> = g.space.params.iter().filter_map(|p| p.apply.as_ref().map(|a| Json::from(format!("{}: {}", p.name, a.describe())))).collect();
            if !applies.is_empty() {
                j.set("applies_to", Json::Arr(applies));
            }
        }
        if let Some(f) = self.taste.failure(&g.id) {
            j.set("problem", f.clone());
        }
        if let Some(c) = self.taste.costs.get(&g.id) {
            j.set("render_cost", c.clone());
        }
        if let Some((_, wn)) = self.taste.warnings.get(&g.id).filter(|(n, _)| *n == g.rounds.len()) {
            j.set("warning", wn.clone());
        }
        if self.taste.screen.gallery() == Some(g.id.as_str()) {
            j.set("screen", self.taste.screen.status_json(owner));
        }
        j
    }

    fn gallery_list(&self, status: Option<&str>, after: Option<&str>, limit: usize) -> Json {
        let mut items = Vec::new();
        let mut more = None;
        for g in self.taste.galleries().rev() {
            if after.is_some_and(|a| g.id.as_str() >= a) {
                continue;
            }
            let st = if self.taste.is_busy(&g.id) && g.status() != Status::Accepted { "rendering" } else { g.status().name() };
            if status.is_some_and(|s| s != st) {
                continue;
            }
            if items.len() >= limit.clamp(1, 32) {
                more = Some(g.id.clone());
                break;
            }
            items.push(Json::obj([("id", Json::from(g.id.as_str())), ("title", Json::from(g.title.as_str())), ("status", Json::from(st)), ("rounds", Json::from(g.rounds.len())), ("subject", Json::from(preferences::subject_label(&g.subject))), ("by", Json::from(g.by.as_str()))]));
        }
        let mut j = Json::obj([("galleries", Json::Arr(items)), ("total", Json::from(self.taste.cache.len()))]);
        if let Some(m) = more {
            j.set("next_after", Json::from(m));
        }
        if !self.taste.bad.is_empty() {
            j.set("unreadable", Json::obj(self.taste.bad.iter().take(16).map(|(p, e)| (p.clone(), Json::from(e.as_str())))));
        }
        j
    }

    // ---- suggestions ----

    fn suggestion_log(&self) -> SuggestionLog {
        self.world.files().get(SUGGESTIONS).and_then(|f| SuggestionLog::parse(&f.text).ok()).unwrap_or_default()
    }

    /// Open suggestions, worked out once per files revision (taste review 2: rebuilt on every
    /// taste_query, 59-300 ms over a full store).
    fn taste_suggestions(&mut self) -> Vec<preferences::Suggestion> {
        self.taste_refresh();
        if self.taste.suggestions.is_none() {
            let log = self.suggestion_log();
            self.taste.suggestions = Some(preferences::suggestions(self.taste.galleries(), &log.dismissed()));
        }
        self.taste.suggestions.clone().unwrap_or_default()
    }

    fn suggest(&mut self, action: &str, id: Option<&str>, w: Writer, auth: TasteAuthority, named: bool) -> Result<Json, WorldError> {
        let log = self.suggestion_log();
        let all = self.taste_suggestions();
        if action == "list" {
            return Ok(Json::obj([
                ("suggestions", Json::arr(all.iter().take(32).map(|s| {
                    let mut j = s.to_json();
                    if let Some(st) = log.state(&s.id) {
                        j.set("state", Json::from(st));
                    }
                    j
                }))),
                ("min_picks", Json::from(preferences::MIN_PICKS)),
                ("note", Json::from("suggestions wait for the human: submit puts one on the review queue (approving it writes it); apply needs the owner; dismiss records no")),
            ]));
        }
        if !named {
            return Err(WorldError::new("hello_required", "say agent_hello first: suggestions are submitted, applied and dismissed by name"));
        }
        let id = id.ok_or_else(|| WorldError::new("missing_id", format!("{action} needs a suggestion id (taste_suggest {{action: \"list\"}})")))?;
        let s = all.iter().find(|s| s.id == id).cloned().ok_or_else(|| WorldError::new("suggestion_not_found", format!("no open suggestion '{id}' (it may be applied or dismissed already)")).with("id", id))?;
        match action {
            "submit" => {
                if log.state(id) == Some("submitted") {
                    return Err(WorldError::new("already_submitted", format!("suggestion {id} is already on the review queue")).with("id", id));
                }
                let tick = self.clock.tick();
                let mut captures = Vec::new();
                for (gid, n) in s.evidence.iter().rev().take(2) {
                    if let Some((path, _, _)) = self.taste.get(gid).and_then(|g| self.taste.sheet(g, *n)) {
                        captures.push(Capture { path: path.display().to_string(), kind: "gallery_round".into(), packet: Some(path.with_extension("json").display().to_string()), caption: format!("{gid} round {n}"), recorded: None });
                    }
                }
                let checks = vec![Check {
                    kind: "taste".into(),
                    name: "consistent_picks".into(),
                    passed: crate::review::Fact::Engine(true),
                    severity: "note".into(),
                    metric: Some("picks".into()),
                    value: Some(s.evidence.len() as f64),
                    limit: Some(preferences::MIN_PICKS as f64),
                    detail: format!("{} picks of {} agree within {:.3} (galleries {})", s.evidence.len(), s.target, s.spread, s.evidence.iter().map(|(g, r)| format!("{g}#{r}")).collect::<Vec<_>>().join(", ")),
                    recorded: None,
                }];
                let what = s.to_json().get("what").and_then(Json::as_str).unwrap_or("").to_owned();
                let spec = SubmitSpec { kind: "changes".into(), title: specula_gustus::cut(&format!("Taste: {what}"), 200), summary: format!("From repeated picks in galleries ({} agreeing). Approving writes it as the decider; rejecting dismisses it.", s.evidence.len()), batch: Some(format!("{SUGGESTION_BATCH}{id}")), captures, checks, ..SubmitSpec::default() };
                let sub = self.swarm.review.submit(w, tick, spec)?.id;
                self.review_ui.changed(&self.swarm.review);
                self.suggestion_log_set(id, "submitted", w, &format!("review {sub}"))?;
                Ok(Json::obj([("id", Json::from(id)), ("submission", Json::from(sub)), ("state", Json::from("submitted"))]))
            }
            "apply" if auth != TasteAuthority::Owner => Err(WorldError::new("owner_only", "applying a suggestion writes the project's style: the human approves it on the review queue (submit), or an agent the operator granted owner applies it").with("authority", auth.name())),
            "apply" => self.suggestion_apply(id, w),
            "dismiss" if !auth.directs() => Err(WorldError::new("not_allowed", "dismissing is the owner's, or a granted lead's or reviewer's").with("authority", auth.name())),
            "dismiss" => {
                self.suggestion_log_set(id, "dismissed", w, "")?;
                Ok(Json::obj([("id", Json::from(id)), ("state", Json::from("dismissed"))]))
            }
            other => Err(WorldError::new("bad_action", format!("action '{other}': list, submit, apply or dismiss")).with("field", "action")),
        }
    }

    fn suggestion_log_set(&mut self, id: &str, state: &str, w: Writer, detail: &str) -> Result<Json, WorldError> {
        let mut log = self.suggestion_log();
        log.entries.insert(id.to_owned(), (state.to_owned(), w.name.to_owned(), detail.to_owned()));
        let text = log.to_text();
        self.project_put(SUGGESTIONS, |_| Ok(text), w, w.name, TASTE_RULE)
    }

    /// Writes a suggestion (a project-file edit, or a prior) as `w`.
    fn suggestion_apply(&mut self, id: &str, w: Writer) -> Result<Json, WorldError> {
        let s = self.taste_suggestions().into_iter().find(|s| s.id == id).ok_or_else(|| WorldError::new("suggestion_not_found", format!("no open suggestion '{id}'")).with("id", id))?;
        // The result and the log entry that records it: one commit.
        let result: (String, crate::project_engine::PutEdit<'_>) = match &s.apply {
            Some(a) => {
                if specula_gustus::space::is_taste_file(&a.file) {
                    return Err(WorldError::new("reserved", format!("suggestion '{id}' applies to taste state ({}): suggestions apply to the game's files only", a.file)).with("id", id));
                }
                let edits = vec![(a.at.clone(), s.value.clone())];
                (format!("assets/{}", a.file), Box::new(move |old: Option<&str>| apply_edits(old, &edits)))
            }
            None => {
                let mut priors = self.world.files().get(PRIORS).and_then(|f| Priors::parse(&f.text).ok()).unwrap_or_default();
                priors.by_target.insert(s.target.clone(), s.value.to_json());
                let text = priors.to_text();
                (PRIORS.to_owned(), Box::new(move |_| Ok(text)))
            }
        };
        let mut log = self.suggestion_log();
        log.entries.insert(id.to_owned(), ("applied".to_owned(), w.name.to_owned(), s.to_json().get("what").and_then(Json::as_str).unwrap_or("").to_owned()));
        let log_text = log.to_text();
        let mut out = self.project_put_all(vec![result, (SUGGESTIONS.to_owned(), Box::new(move |_| Ok(log_text)))], w, w.name, DECIDED_RULE, &[SUGGESTIONS])?;
        let written = if out.is_empty() { Json::Null } else { out.swap_remove(0) };
        Ok(Json::obj([("id", Json::from(id)), ("state", Json::from("applied")), ("written", written)]))
    }

    // ---- onboarding ----

    fn onboarding(&self) -> Option<presets::Onboarding> {
        self.world.files().get(ONBOARDING).and_then(|f| presets::Onboarding::parse(&f.text).ok())
    }

    fn onboard_save(&mut self, o: &presets::Onboarding, w: Writer) -> Result<Json, WorldError> {
        let text = o.to_text();
        self.project_put(ONBOARDING, |_| Ok(text), w, w.name, TASTE_RULE)
    }

    /// Starts onboarding step `k`'s gallery (as `w`): the gallery and the configurator state
    /// that names it, one commit.
    fn onboard_step(&mut self, o: &mut presets::Onboarding, k: usize, w: Writer) -> Result<String, WorldError> {
        let g = self.onboard_step_gallery(o, k, w.name)?;
        let (gtext, otext) = (g.to_text(), o.to_text());
        if let Err(e) = self.project_put_all(vec![(gallery_path(&g.id), Box::new(move |_| Ok(gtext))), (ONBOARDING.to_owned(), Box::new(move |_| Ok(otext)))], w, w.name, TASTE_RULE, &[]) {
            o.galleries.pop();
            return Err(e);
        }
        self.taste_saved(&g);
        self.taste_enqueue(&g, None);
        Ok(g.id)
    }

    /// Onboarding step `k`'s gallery, made by `maker` and named in `o` (nothing written).
    fn onboard_step_gallery(&mut self, o: &mut presets::Onboarding, k: usize, maker: &str) -> Result<Gallery, WorldError> {
        let preset = presets::find(&o.preset).unwrap_or(&presets::PRESETS[0]);
        let (title, subject, space) = presets::step_gallery(k, preset, &o.subject).ok_or_else(|| WorldError::new("no_step", format!("onboarding has {} steps", presets::STEPS.len())))?;
        let space = specula_gustus::space::Space::parse(&space).map_err(taste_err)?;
        let mut n = 1;
        let id = loop {
            let id = format!("onboarding{n}_{}", presets::STEPS[k]);
            if !self.taste.cache.contains_key(&id) && !self.world.files().contains_key(&gallery_path(&id)) {
                break id;
            }
            n += 1;
        };
        // The owner sees where they are in the configurator (a sight-skeptic finding).
        let title = format!("Step {} of {}: {title}", k + 1, presets::STEPS.len());
        let mut g = Gallery::new(&id, &title, subject, space, specula_gustus::gallery::Mode::Tune, specula_core::hash::hash_str(&id), specula_gustus::gallery::DEFAULT_VARIANTS, maker).map_err(taste_err)?;
        g.subject.set("onboarding_step", Json::from(presets::STEPS[k]));
        let subject = render::parse_subject(&g.subject).map_err(|m| WorldError::new("bad_subject", m))?;
        render::check_targets(&subject, &g.space).map_err(|m| WorldError::new("bad_target", m))?;
        o.galleries.push(id);
        Ok(g)
    }

    #[allow(clippy::too_many_arguments)]
    fn onboard(&mut self, action: &str, subject: Option<Json>, preset: Option<String>, user: &str, open: Option<bool>, quote_notes: Option<bool>, w: Writer, auth: TasteAuthority, session: &SessionInfo) -> Result<Json, WorldError> {
        match action {
            "status" => Ok(self.onboard_status()),
            "start" => {
                if self.onboarding().is_some_and(|o| o.state == "running" || o.state == "proposed") {
                    return Err(WorldError::new("onboarding_running", "the style configurator is already running (style_onboard {action: \"status\"}); cancel it first"));
                }
                let mut subject = subject.unwrap_or_else(|| Json::obj([("stage", Json::obj::<String>([]))]));
                self.taste_resolve_subject(&mut subject, session)?;
                render::parse_subject(&subject).map_err(|m| WorldError::new("bad_subject", m).with("field", "subject"))?;
                let preset = preset.unwrap_or_else(|| if self.world.files().contains_key(STYLE_TOML) { "moonlit".into() } else { "clean".into() });
                let mut o = presets::Onboarding::new(subject, &preset, w.name).map_err(taste_err)?;
                o.quote_notes = quote_notes.unwrap_or(false);
                let id = self.onboard_step(&mut o, 0, w)?;
                let mut j = self.onboard_status();
                // Putting the first gallery on the window is the default for a caller who may
                // (the owner, a granted lead or reviewer). Anyone else starts the onboarding
                // without it: the default is no try at the human's window, so it is never
                // counted (review R2 of m5-inputguard, major 3: a builder's plain start froze
                // it). Only an explicit `open: true` asks for the window.
                if open.is_none() && !auth.directs() {
                    j.set("screen", Json::obj([("opened", Json::Bool(false)), ("note", Json::from("not put on the window: that needs the owner, a granted lead or a reviewer (gallery_open); the onboarding runs without it"))]));
                } else if open.unwrap_or(true) {
                    let actor = crate::inputguard::Actor::of(&self.swarm, session);
                    j.set("screen", self.taste_open(&id, user, auth, &actor, crate::authority::trusted(&self.swarm, &session.id) >= crate::authority::Authority::Lead).unwrap_or_else(|e| e.to_json()));
                }
                Ok(j)
            }
            "finish" => {
                let mut o = self.onboarding().filter(|o| o.state == "running").ok_or_else(|| WorldError::new("not_running", "the style configurator isn't running (style_onboard {action: \"start\"})"))?;
                if let Some(q) = quote_notes {
                    o.quote_notes = q;
                }
                self.onboard_propose(&mut o, w, w, None)
            }
            "cancel" => {
                let mut o = self.onboarding().filter(|o| o.state == "running" || o.state == "proposed").ok_or_else(|| WorldError::new("not_running", "the style configurator isn't running"))?;
                if o.by != w.name && !auth.directs() {
                    return Err(WorldError::new("not_allowed", "the agent that started it, or the owner, lead or director, cancels the configurator"));
                }
                o.state = "cancelled".into();
                self.onboard_save(&o, w)?;
                Ok(self.onboard_status())
            }
            other => Err(WorldError::new("bad_action", format!("action '{other}': start, status, finish or cancel")).with("field", "action")),
        }
    }

    fn onboard_status(&self) -> Json {
        let Some(o) = self.onboarding() else { return Json::obj([("state", Json::from("none")), ("steps", Json::arr(presets::STEPS.map(Json::from)))]) };
        let steps: Vec<Json> = presets::STEPS
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let g = o.galleries.get(k).and_then(|id| self.taste.get(id));
                Json::obj([("step", Json::from(*s)), ("gallery", o.galleries.get(k).map_or(Json::Null, |g| Json::from(g.as_str()))), ("status", Json::from(g.map_or("waiting", |g| g.status().name())))])
            })
            .collect();
        let mut j = Json::obj([("state", Json::from(o.state.as_str())), ("preset", Json::from(o.preset.as_str())), ("steps", Json::Arr(steps))]);
        if let Some(s) = o.submission {
            j.set("submission", Json::from(s));
        }
        j
    }

    /// After gallery `g` is accepted (not written yet): if it is the onboarding's current step,
    /// the next step starts (the base style's pick decides the preset); after the last, the
    /// result is proposed. Either way `g` is written in the same commit, as `w` (`None`: not a
    /// current configurator step; nothing written).
    fn onboard_after_accept(&mut self, g: &Gallery, w: Writer) -> Option<Result<Json, WorldError>> {
        let mut o = self.onboarding().filter(|o| o.state == "running")?;
        let k = o.galleries.iter().position(|x| *x == g.id)?;
        if k + 1 != o.galleries.len() {
            return None;
        }
        if k == 0 {
            if let Some((_, _, vals)) = g.accepted() {
                if let Some(Value::Choice(_, name)) = vals.first() {
                    o.preset = name.clone();
                }
            }
        }
        Some(if k + 1 < presets::STEPS.len() {
            self.onboard_next(g, &mut o, k + 1, w)
        } else {
            // The proposal is the configurator's (submitted as `taste`, so the owner can approve
            // it); its state is written with the decision.
            self.onboard_propose(&mut o, Writer::new(TASTE_AGENT, None), w, Some(g))
        })
    }

    /// Step `k`'s gallery after `g`'s acceptance: `g`, the new gallery and onboarding.json in one
    /// commit as `w`; the human continues on the window with it.
    fn onboard_next(&mut self, g: &Gallery, o: &mut presets::Onboarding, k: usize, w: Writer) -> Result<Json, WorldError> {
        let next = self.onboard_step_gallery(o, k, TASTE_AGENT)?;
        let (gt, nt, ot) = (g.to_text(), next.to_text(), o.to_text());
        let puts: Vec<(String, crate::project_engine::PutEdit<'_>)> =
            vec![(gallery_path(&g.id), Box::new(move |_| Ok(gt))), (gallery_path(&next.id), Box::new(move |_| Ok(nt))), (ONBOARDING.to_owned(), Box::new(move |_| Ok(ot)))];
        self.project_put_all(puts, w, w.name, TASTE_RULE, &[])?;
        self.taste_saved(g);
        self.taste_saved(&next);
        self.taste_enqueue(&next, None);
        if self.taste.screen.gallery() == Some(g.id.as_str()) {
            let user = self.taste.screen.user().to_owned();
            // The engine carries on the human's own acceptance on the screen they have open.
            let seen = crate::inputguard::done(crate::inputguard::Channel::Screen, "gallery_open.next_step", &crate::inputguard::Actor::Engine("onboarding"), &next.id);
            self.taste.screen.open(&next.id, &user, seen);
        }
        Ok(Json::obj([("next_step", Json::from(presets::STEPS[k])), ("gallery", Json::from(next.id.as_str()))]))
    }

    /// The accepted steps' values, notes and rounds (`decided`: a gallery as just decided, not
    /// written yet, read in place of its file).
    fn onboard_results(&self, o: &presets::Onboarding, decided: Option<&Gallery>) -> Vec<presets::StepResult> {
        o.galleries
            .iter()
            .enumerate()
            .filter_map(|(k, id)| {
                let g = decided.filter(|d| d.id == *id).or_else(|| self.taste.get(id))?;
                let (round, _, vals) = g.accepted()?;
                let values = g.space.params.iter().zip(vals).filter_map(|(p, v)| match &p.apply.as_ref()?.at {
                    ApplyAt::Toml(key) if p.apply.as_ref()?.file == "style.toml" => Some((key.clone(), v.toml_literal(), p.name.clone())),
                    _ => None,
                });
                let notes = g.rounds.iter().filter_map(|r| match &r.decision.as_ref()?.kind {
                    Kind::Edit { note, .. } | Kind::None { note } => Some(note.clone()),
                    _ => None,
                });
                Some(presets::StepResult { step: presets::STEPS[k], gallery: id.clone(), round, values: values.collect(), notes: notes.collect() })
            })
            .collect()
    }

    /// Composes the result and puts it on the review queue as `submitter` (nothing is written
    /// until approved); the configurator's state, with `decided` (the last step's gallery as just
    /// accepted), is one commit as `w`.
    fn onboard_propose(&mut self, o: &mut presets::Onboarding, submitter: Writer, w: Writer, decided: Option<&Gallery>) -> Result<Json, WorldError> {
        let steps = self.onboard_results(o, decided);
        if steps.is_empty() {
            return Err(WorldError::new("nothing_chosen", "no onboarding step is accepted yet: the human picks \"This one\" in at least the first gallery"));
        }
        let preset = presets::find(&o.preset).unwrap_or(&presets::PRESETS[0]);
        let existing = self.world.files().get(STYLE_TOML).map(|f| f.text.to_string());
        let project = if self.game_name.is_empty() { "The game".to_owned() } else { self.game_name.clone() };
        let (toml, notes) = presets::compose(preset, existing.as_deref(), &steps, &project, o.quote_notes).map_err(taste_err)?;
        let captures: Vec<Capture> = steps
            .iter()
            .filter_map(|s| {
                let g = decided.filter(|d| d.id == s.gallery).or_else(|| self.taste.get(&s.gallery))?;
                let (path, _, _) = self.taste.sheet(g, s.round)?;
                Some(Capture { path: path.display().to_string(), kind: "gallery_round".into(), packet: Some(path.with_extension("json").display().to_string()), caption: format!("{}: accepted in round {}", s.step, s.round), recorded: None })
            })
            .take(8)
            .collect();
        let checks = steps
            .iter()
            .map(|s| Check { kind: "taste".into(), name: format!("{}_accepted", s.step), passed: crate::review::Fact::Engine(true), severity: "note".into(), metric: Some("round".into()), value: Some(s.round as f64), limit: None, detail: s.values.iter().map(|v| format!("{} = {}", v.0, v.1)).collect::<Vec<_>>().join(", "), recorded: None })
            .collect();
        let tick = self.clock.tick();
        let spec = SubmitSpec {
            kind: "changes".into(),
            title: "Style configurator: the look (style.toml and notes)".into(),
            summary: specula_gustus::cut(&format!("Approving writes {STYLE_TOML} ({} keys from {} steps, base style {}) and {NOTES} (tracked). {}", steps.iter().map(|s| s.values.len()).sum::<usize>(), steps.len(), preset.label, notes_line(&steps, o.quote_notes)), 2000),
            batch: Some(ONBOARDING_BATCH.into()),
            captures,
            checks,
            ..SubmitSpec::default()
        };
        let sub = self.swarm.review.submit(submitter, tick, spec)?.id;
        self.review_ui.changed(&self.swarm.review);
        o.state = "proposed".into();
        o.submission = Some(sub);
        o.proposal = Some((toml, notes));
        let otext = o.to_text();
        let mut puts: Vec<(String, crate::project_engine::PutEdit<'_>)> = vec![(ONBOARDING.to_owned(), Box::new(move |_| Ok(otext)))];
        if let Some(g) = decided {
            let gt = g.to_text();
            puts.insert(0, (gallery_path(&g.id), Box::new(move |_| Ok(gt))));
        }
        self.project_put_all(puts, w, w.name, TASTE_RULE, &[])?;
        if let Some(g) = decided {
            self.taste_saved(g);
        }
        Ok(Json::obj([("state", Json::from("proposed")), ("submission", Json::from(sub)), ("note", Json::from("the result is on the review queue: approving it (the window's review screen, F9) writes style.toml and the notes"))]))
    }

    /// The approved result, written as the decider (recomposed from the accepted galleries
    /// against style.toml as it is now, so edits made meanwhile are kept).
    fn onboard_write(&mut self, w: Writer) -> Result<Json, WorldError> {
        let mut o = self.onboarding().filter(|o| o.state == "proposed").ok_or_else(|| WorldError::new("not_proposed", "no style configurator result is waiting"))?;
        let steps = self.onboard_results(&o, None);
        let preset = presets::find(&o.preset).unwrap_or(&presets::PRESETS[0]);
        let project = if self.game_name.is_empty() { "The game".to_owned() } else { self.game_name.clone() };
        let quote = o.quote_notes;
        // style.toml, the notes and the configurator's state: one commit, all or nothing (the
        // notes are composed with style.toml, so the first edit hands them to the second).
        let notes_out = std::cell::RefCell::new(String::new());
        o.state = "approved".into();
        let otext = o.to_text();
        let puts: Vec<(String, crate::project_engine::PutEdit<'_>)> = vec![
            (
                STYLE_TOML.to_owned(),
                Box::new(|old: Option<&str>| {
                    let (toml, notes) = presets::compose(preset, old, &steps, &project, quote).map_err(taste_err)?;
                    *notes_out.borrow_mut() = notes;
                    Ok(toml)
                }),
            ),
            (NOTES.to_owned(), Box::new(|_| Ok(std::mem::take(&mut *notes_out.borrow_mut())))),
            (ONBOARDING.to_owned(), Box::new(move |_| Ok(otext))),
        ];
        let mut out = self.project_put_all(puts, w, w.name, DECIDED_RULE, &[ONBOARDING])?.into_iter();
        let (a, b) = (out.next().unwrap_or(Json::Null), out.next().unwrap_or(Json::Null));
        Ok(Json::obj([("style_toml", a), ("notes", b)]))
    }
}

/// Each file's edits (`assets/<file>`: where and what), as a gallery applies them.
type FileEdits = BTreeMap<String, Vec<(ApplyAt, Value)>>;

/// What accepted gallery `g` writes: its round and variant, and each file's edits (refused for a
/// configurator step, taste state, or a gallery with no `apply` targets).
fn apply_plan(g: &Gallery, id: &str) -> Result<(usize, usize, FileEdits), WorldError> {
    let Some((round, variant, values)) = g.accepted().map(|(r, v, vals)| (r, v, vals.to_vec())) else {
        return Err(WorldError::new("not_accepted", format!("gallery '{id}' has no accepted variant yet: the human picks \"This one\" first")).with("id", id));
    };
    // A configurator step's values reach style.toml only through the reviewed proposal.
    if g.subject.get("onboarding_step").is_some() {
        return Err(WorldError::new("onboarding_step", format!("gallery '{id}' is a style configurator step: its values are written when the configurator's proposal is approved on the review queue")).with("id", id));
    }
    let mut by_file: FileEdits = BTreeMap::new();
    for (p, v) in g.space.params.iter().zip(&values) {
        if let Some(a) = &p.apply {
            // (Space::parse refuses these; a gallery file written before it did is refused here.)
            if specula_gustus::space::is_taste_file(&a.file) {
                return Err(WorldError::new("reserved", format!("gallery '{id}' applies to taste state ({}): galleries apply to the game's files only", a.file)).with("id", id));
            }
            by_file.entry(format!("assets/{}", a.file)).or_default().push((a.at.clone(), v.clone()));
        }
    }
    if by_file.is_empty() {
        return Err(WorldError::new("nothing_to_apply", format!("no parameter of gallery '{id}' says where its value lives (apply: {{file, toml|json}}); read the values with gallery_get")).with("id", id));
    }
    Ok((round, variant, by_file))
}

/// `old` (a TOML or JSON file's text) with each value set at its place.
fn apply_edits(old: Option<&str>, edits: &[(ApplyAt, Value)]) -> Result<String, WorldError> {
    let mut text = old.map_or_else(String::new, str::to_owned);
    for (at, v) in edits {
        text = match at {
            ApplyAt::Toml(key) => specula_gustus::tomledit::set(&text, key, &v.toml_literal()).map_err(taste_err)?,
            ApplyAt::Json(path) => {
                if text.is_empty() {
                    text = "{}\n".into();
                }
                crate::project_text::json_set(&text, path, &v.to_json()).map_err(|m| WorldError::new("bad_apply", format!("{path}: {m}")))?
            }
        };
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taste_files_are_validated_by_place() {
        let g = Gallery::new("moon", "t", Json::obj([("stage", Json::obj::<String>([]))]), specula_gustus::space::Space::parse(&Json::parse(r#"[{"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}]"#).unwrap()).unwrap(), specula_gustus::gallery::Mode::Tune, 1, 9, "maker").unwrap();
        assert!(validate_file("assets/taste/galleries/moon.json", &g.to_text()).is_ok());
        assert_eq!(validate_file("assets/taste/galleries/sun.json", &g.to_text()).unwrap_err().code, "bad_id");
        assert!(validate_file("assets/taste/suggestions.json", "{}").is_err());
        assert!(validate_file("assets/taste/notes.json", "{\"a\": 1}").is_ok());
        assert!(validate_file("assets/taste/notes.json", "{").is_err());
    }

    /// Review probe (major 5): a round that failed once was never made again. Transient codes
    /// back off and are retried; permanent ones wait for a person.
    #[test]
    fn transient_failures_are_retried_and_permanent_ones_wait() {
        assert_eq!(retry_after("studio_busy", 0), Some(Duration::from_secs(2)));
        assert_eq!(retry_after("timed_out", 1), Some(Duration::from_secs(4)));
        assert_eq!(retry_after("render_unavailable", 5), Some(Duration::from_secs(60)));
        assert_eq!(retry_after("studio_busy", MAX_ATTEMPTS), None, "gives up after a while; Try again restarts it");
        assert_eq!(retry_after("bad_subject", 0), None);
        assert_eq!(retry_after("nothing_to_plan", 0), None);
    }

    /// Review probe (majors 1 and 2): taste state is a path the project tools' rule may not
    /// write, and taste batches are the taste tools' only.
    #[test]
    fn taste_state_and_batches_are_reserved() {
        assert!(crate::project::is_taste_state(&crate::project::journal_path("assets/taste/galleries/g1.json")));
        assert!(crate::project::is_taste_state(&crate::project::journal_path("assets/taste/onboarding.json")));
        assert!(!crate::project::is_taste_state(&crate::project::journal_path(PRIORS)), "approved priors are tracked results");
        assert!(!crate::project::is_taste_state(&crate::project::journal_path(NOTES)));
        assert!(!crate::project::is_taste_state(&crate::project::journal_path(STYLE_TOML)));
        // Folded: assets/Taste/ is the same folder on Windows and macOS (taste review 2).
        assert!(crate::project::is_taste_state(&crate::project::journal_path("assets/Taste/galleries/g1.json")));
        assert!(crate::project::is_taste_state("/project/assets/taste"), "the folder itself (a claim on it)");
        assert!(specula_gustus::space::is_taste_file("taste/galleries/vic.json") && specula_gustus::space::is_taste_file("./TASTE/x.json") && !specula_gustus::space::is_taste_file("style.toml") && !specula_gustus::space::is_taste_file("ui/taste.json"));
        assert!(is_taste_batch(ONBOARDING_BATCH) && is_taste_batch("taste:s1234") && !is_taste_batch("crypt_props"));
    }

    fn files_of(gs: &[(&Gallery, u64)]) -> BTreeMap<String, crate::project::ProjectFile> {
        gs.iter().map(|(g, v)| (gallery_path(&g.id), crate::project::ProjectFile::new(crate::project::FileKind::Taste, std::sync::Arc::from(g.to_text()), *v, std::sync::Arc::from("t")))).collect()
    }

    fn moon(id: &str, by: &str) -> Gallery {
        let space = specula_gustus::space::Space::parse(&Json::parse(r#"[{"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}]"#).unwrap()).unwrap();
        Gallery::new(id, "t", Json::obj([("stage", Json::obj::<String>([]))]), space, specula_gustus::gallery::Mode::Tune, 1, 9, by).unwrap()
    }

    /// Taste review 2 (1b): saving one gallery set the files revision, so another gallery's change
    /// (an undo, a disk edit) went unparsed until a restart.
    #[test]
    fn every_changed_gallery_file_is_parsed_again() {
        let (a, b) = (moon("a", "maker"), moon("b", "maker"));
        let mut t = Taste::default();
        t.refresh(&files_of(&[(&a, 1), (&b, 1)]), 1);
        assert_eq!(t.get("b").unwrap().by, "maker");
        // b changes (another writer), then a is saved; the refresh at the new revision sees both.
        let b2 = moon("b", "mallory");
        let mut a2 = a.clone();
        a2.title = "saved".into();
        t.saved(&a2, 2);
        t.refresh(&files_of(&[(&a2, 2), (&b2, 2)]), 3);
        assert_eq!(t.get("b").unwrap().by, "mallory", "the other gallery's change is parsed");
        assert_eq!(t.get("a").unwrap().title, "saved");
    }

    /// Taste review 2 (3): the pump cloned every open gallery once a second (62 ms at 110, up to
    /// 910 ms at 512). It now picks at most four of each kind, by id, and clones only those.
    #[test]
    fn the_pump_looks_at_four_galleries_without_cloning_the_rest() {
        let mut gs = Vec::new();
        for i in 0..512 {
            let mut g = moon(&format!("g{i:03}"), "maker");
            if i % 2 == 0 {
                let p = g.first_round(None);
                g.push_round(p, Some(format!("gallery-g{i:03}-r1.png")), vec![]).unwrap();
            }
            gs.push(g);
        }
        let mut t = Taste::default();
        t.refresh(&files_of(&gs.iter().map(|g| (g, 1)).collect::<Vec<_>>()), 1);
        let started = Instant::now();
        let (want, open) = t.pump_candidates(&[], Instant::now());
        let spent = started.elapsed();
        assert_eq!((want.len(), open.len()), (PUMP_TAKES, PUMP_TAKES));
        assert!(spent < specula_core::speed_budget(std::time::Duration::from_millis(20)), "picking took {spent:?} over 512 galleries");
        // Known sheets are not looked for again.
        for id in &open {
            t.sheets.insert((id.clone(), 1), (PathBuf::new(), vec![], (0, 0)));
        }
        let (_, again) = t.pump_candidates(&[], Instant::now());
        assert!(again.iter().all(|id| !open.contains(id)));
    }

    /// Taste review 2 (part B): two projects sharing a captures folder wrote sheets of the same
    /// name; after a restart one project showed the other's sheet. A sheet counts only when its
    /// packet's tiles are the round's.
    #[test]
    fn a_sheet_counts_only_when_its_tiles_are_the_rounds() {
        let dir = std::env::temp_dir().join(format!("specula-sheet-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut g = moon("g1", "maker");
        let p = g.first_round(None);
        g.push_round(p, Some("gallery-g1-r1.png".into()), vec![]).unwrap();
        std::fs::write(dir.join("gallery-g1-r1.png"), b"png").unwrap();
        let packet = |values: Vec<Json>| {
            let tiles: Vec<Json> = values.into_iter().map(|v| Json::obj([("box", Json::arr([0u32, 0, 10, 10].map(Json::from))), ("values", v)])).collect();
            Json::obj([("gallery", Json::from("g1")), ("size", Json::arr([Json::from(10), Json::from(10)])), ("tiles", Json::Arr(tiles))]).to_pretty()
        };
        let mine: Vec<Json> = g.rounds[0].variants.iter().map(|v| g.space.values_json(&v.values)).collect();
        std::fs::write(dir.join("gallery-g1-r1.json"), packet(mine.clone())).unwrap();
        assert!(Taste::load_sheet(&g, 1, &dir).is_some(), "its own sheet");
        let other: Vec<Json> = mine.iter().map(|_| Json::obj([("fog", Json::from(0.5))])).collect();
        std::fs::write(dir.join("gallery-g1-r1.json"), packet(other)).unwrap();
        assert!(Taste::load_sheet(&g, 1, &dir).is_none(), "another project's sheet of the same name is not this round's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn taste_authority_is_one_function() {
        use crate::authority::Authority;
        assert_eq!(taste_authority(Authority::Owner), TasteAuthority::Owner);
        assert_eq!(taste_authority(Authority::Lead), TasteAuthority::Opener);
        assert_eq!(taste_authority(Authority::Reviewer), TasteAuthority::Opener, "a reviewer already puts work on the owner's screen");
        assert_eq!(taste_authority(Authority::Agent), TasteAuthority::Agent, "a job label (art_director, artist) grants nothing");
        assert!(TasteAuthority::Owner.directs() && TasteAuthority::Opener.directs() && !TasteAuthority::Agent.directs());
    }

    #[test]
    fn edits_apply_to_toml_and_json() {
        let toml = apply_edits(Some("[stage.game]\nkey_color = \"#d0d9f0\"\n"), &[(ApplyAt::Toml("stage.game.key_color".into()), Value::Colour([0.75, 0.8, 0.9]))]).unwrap();
        assert_eq!(toml, "[stage.game]\nkey_color = \"#bfcce6\"\n");
        let json = apply_edits(Some("{\n  \"colors\": {\n    \"fire\": \"#ffd27a\"\n  }\n}\n"), &[(ApplyAt::Json("colors.cant_afford".into()), Value::Colour([0.5, 0.5, 0.5]))]).unwrap();
        assert!(json.contains("\"cant_afford\": \"#808080\"") && json.contains("\"fire\": \"#ffd27a\""), "{json}");
    }
}

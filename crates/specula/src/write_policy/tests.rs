//! The write policy's regression table (docs/design/WRITE_POLICY.md): every write path of the
//! audit, each tried against another agent's claimed entities or reserved data, with what must
//! happen. Rows marked `M3` are the bypasses M3's reviews found (each once a probe that got
//! through); the exempt rows are the documented exemptions, which journal under their own names.

use super::*;
use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
use crate::world::{Body, BodyKind, EntityKey, Op, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::SessionInfo;

const ALICE: Writer = Writer { name: "alice", session: Some("aaaaaa11") };
const MALLORY: Writer = Writer { name: "mallory", session: Some("bbbbbb22") };

fn session(id: &str) -> SessionInfo {
    SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

fn cube(name: &str, pos: Vec3) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Cube);
    s.name = Some(name.into());
    s.position = pos;
    s
}

fn group(path: &str) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Group);
    let (parent, name) = path.rsplit_once('/').unwrap();
    s.name = Some(name.into());
    s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
    s
}

fn rope(name: &str, a: u64, b: u64) -> SpawnSpec {
    let mut j = SpawnSpec::new(Shape::Group);
    j.name = Some(name.into());
    j.position = Vec3::new(40.0, 0.0, 40.0);
    j.props = Json::parse(&format!(r#"{{"joint": {{"kind": "distance", "a": {a}, "b": {b}, "length": [0, 0.6]}}}}"#)).unwrap();
    j
}

fn joint_onto(name: &str, b: u64) -> SpawnSpec {
    let mut j = SpawnSpec::new(Shape::Group);
    j.name = Some(name.into());
    j.position = Vec3::new(50.0, 0.0, 50.0);
    j.props = Json::parse(&format!(r#"{{"joint": {{"kind": "distance", "a": null, "b": {b}, "anchor_a": [0, 8, 0], "length": [0, 0.5]}}}}"#)).unwrap();
    j
}

/// A small burst every quarter second (vfx_check's `calm`): within 3 flashes a second as it is,
/// at `size` 0.05 m; five times bigger and four times as often, it strobes (at 0.01 m it still
/// doesn't).
fn calm(size: f32) -> Json {
    Json::parse(&format!(r##"{{"emitters": [{{"bursts": [{{"count": 1, "cycles": 0, "interval": 0.25}}], "lifetime": 0.05, "size": {size}, "color": "#ffffff", "alpha": [[0, 1], [1, 1]], "blend": "additive"}}]}}"##)).unwrap()
}

/// A world effect `/vfx/<name>` as vfx_set writes it.
fn effect(name: &str, size: f32) -> SpawnSpec {
    let mut s = group(&format!("/vfx/{name}"));
    s.tags.insert(crate::vfx::EFFECT_TAG.into());
    s.props = Json::obj([(crate::vfx::EFFECT_PROP, calm(size))]);
    s
}

/// An entity playing `effect` with `rate` and `size` params.
fn playing(name: &str, effect: &str, rate: f32, size: f32) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Group);
    s.name = Some(name.into());
    s.position = Vec3::new(60.0, 1.0, 60.0);
    let mut v = specula_scintilla::VfxSpec::new(effect);
    (v.params.rate, v.params.size) = (rate, size);
    s.vfx = Some(v);
    s
}

fn moved(x: f32) -> UpdateSpec {
    UpdateSpec { position: Some(Vec3::new(x, 0.5, 0.0)), ..UpdateSpec::default() }
}

/// The world every row starts from: alice's dynamic box `/abox` (her path claim) and `/rbox`
/// (inside her region claim); mallory's group `/mg` with three kinematic hooks, the first two
/// roped to the box (`rope`, `rope2`, made through physics_joint's rule). Before alice claimed,
/// mallory moved the box (`edit`), removed `rope2` (`unjoint`) and deleted `hook3` with its rope
/// (`unhook`): undoing those now is undoing onto alice's claim. The effect library holds alice's
/// `/vfx/glow` (her path claim) and mallory's smaller `/vfx/spark`, which mallory's `/lamp` plays
/// five times bigger and four times as often (calm still, at that size).
struct Fix {
    live: Live,
    sw: Swarm,
    abox: u64,
    rbox: u64,
    hook: u64,
    edit: u64,
    unjoint: u64,
    unhook: u64,
    /// Alice's entry writing taste state (the window's or the worker's, as far as undo goes).
    taste: u64,
}

fn fixture() -> Fix {
    let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
    let sessions = vec![s1.clone(), s2.clone()];
    let mut sw = Swarm::default();
    sw.hello(&s1, "alice".into(), "builder".into(), None, &sessions).unwrap();
    sw.hello(&s2, "mallory".into(), "builder".into(), None, &sessions).unwrap();
    let mut live = Live::new(World::default());
    let spawn = |live: &mut Live, sw: &mut Swarm, w: Writer, rule: Rule, s: SpawnSpec| {
        let by = w.name.to_owned();
        live.commit(sw, 0, w, Kind::Spawn, rule, |wd| wd.spawn(s, &by).map(|e| e.id)).unwrap()
    };
    let mut abox = cube("abox", Vec3::new(0.0, 0.5, 0.0));
    abox.body = Some(Body::new(BodyKind::Dynamic, 5.0));
    let abox = spawn(&mut live, &mut sw, ALICE, Rule::World, abox).0;
    let rbox = spawn(&mut live, &mut sw, ALICE, Rule::World, cube("rbox", Vec3::new(5.0, 0.5, 5.0))).0;
    let mut mg = group("/mg");
    mg.position = Vec3::new(20.0, 0.0, 20.0);
    let mg = spawn(&mut live, &mut sw, MALLORY, Rule::World, mg).0;
    let hook = |name: &str| {
        let mut h = cube(name, Vec3::new(0.0, 6.5, -20.0));
        h.parent = Some(EntityKey::Id(mg));
        h.body = Some(Body::new(BodyKind::Kinematic, 1.0));
        h
    };
    let hook1 = spawn(&mut live, &mut sw, MALLORY, Rule::World, hook("hook")).0;
    let hook2 = spawn(&mut live, &mut sw, MALLORY, Rule::World, hook("hook2")).0;
    let hook3 = spawn(&mut live, &mut sw, MALLORY, Rule::World, hook("hook3")).0;
    spawn(&mut live, &mut sw, MALLORY, Rule::Joint, rope("rope", hook1, abox));
    let rope2 = spawn(&mut live, &mut sw, MALLORY, Rule::Joint, rope("rope2", hook2, abox)).0;
    spawn(&mut live, &mut sw, MALLORY, Rule::Joint, rope("rope3", hook3, abox));
    let edit = live.commit(&mut sw, 0, MALLORY, Kind::Set, Rule::World, |wd| wd.update(&EntityKey::Id(abox), moved(0.25), "mallory").map(|_| ())).unwrap().1.unwrap();
    let unjoint = live.commit(&mut sw, 0, MALLORY, Kind::Delete, Rule::Joint, |wd| wd.delete(&EntityKey::Id(rope2), None).map(|_| ())).unwrap().1.unwrap();
    let unhook = live.commit(&mut sw, 0, MALLORY, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Id(hook3), None).map(|_| ())).unwrap().1.unwrap();
    spawn(&mut live, &mut sw, ALICE, Rule::Vfx, group("/vfx"));
    spawn(&mut live, &mut sw, ALICE, Rule::Vfx, effect("glow", 0.05));
    spawn(&mut live, &mut sw, MALLORY, Rule::Vfx, effect("spark", 0.01));
    spawn(&mut live, &mut sw, MALLORY, Rule::World, playing("lamp", "spark", 4.0, 5.0));
    sw.take_claim(&live, &s1, ClaimSpec { scope: Scope::path("/vfx/glow").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    sw.take_claim(&live, &s1, ClaimSpec { scope: Scope::path("/abox").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let region = Scope::region(Vec3::new(3.0, -1.0, 3.0), Vec3::new(7.0, 4.0, 7.0)).unwrap();
    sw.take_claim(&live, &s1, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    // A project file (read from disk) and a clock: kept even if a writer replaces the world.
    live.disk(&mut sw, 0, |wd| {
        wd.put_file("assets/recipes/fixture.json", Some(crate::project::ProjectFile::new(crate::project::FileKind::Recipe, Arc::from("{}"), 1, Arc::from("disk"))), true);
        // Taste state (the human's picks), as a rows' target.
        wd.put_file("assets/taste/onboarding.json", Some(crate::project::ProjectFile::new(crate::project::FileKind::Taste, Arc::from("{}"), 1, Arc::from("disk"))), true);
        Ok(())
    })
    .unwrap();
    let taste = live
        .commit(&mut sw, 0, ALICE, Kind::File, Rule::Taste, |wd| {
            wd.put_file("assets/taste/galleries/a1.json", Some(crate::project::ProjectFile::new(crate::project::FileKind::Taste, Arc::from("{}"), 1, Arc::from("alice"))), false);
            Ok(())
        })
        .unwrap()
        .1
        .unwrap();
    live.set_clock_s(12.5);
    Fix { live, sw, abox, rbox, hook: hook1, edit, unjoint, unhook, taste }
}

/// What a row must come to.
#[derive(Debug, PartialEq)]
enum Expect {
    /// Refused with this code; the world as before, nothing pending, nothing journaled.
    Refused(&'static str),
    /// A documented exemption: allowed, journaled under this name.
    Exempt(&'static str),
    /// Allowed, journaled under the writer.
    Allowed,
    /// The writer replaced the live world wholesale (a bug): reported and counted, and the world
    /// it left is live consistently (marked, recording, unsealed, the clock, project files and id
    /// counter kept, the journal started over from it).
    Replaced,
}

type Write = fn(&mut Fix) -> Result<(), WorldError>;

/// What the world holds (entities by id with their version, path and content, the environment,
/// project files): a refused write leaves it exactly as it was, while the id counter may move on
/// (ids a refused write used are never handed out again).
type Contents = (Vec<(u64, u64, String, u64)>, String, Vec<(String, u64)>);

fn contents(w: &World) -> Contents {
    let entities = w.entities().map(|e| (e.id, e.version, w.path_of(e.id), e.content_hash())).collect();
    let files = w.files().iter().map(|(p, f)| (p.clone(), f.version)).collect();
    (entities, w.env().to_json_exact().to_string(), files)
}

fn agent(f: &mut Fix, w: Writer, kind: Kind, rule: Rule, command: impl FnOnce(&mut dyn WorldWrite) -> Result<(), WorldError>) -> Result<(), WorldError> {
    f.live.commit(&mut f.sw, 1, w, kind, rule, command).map(|_| ())
}

fn set_abox(f: &mut Fix, rule: Rule) -> Result<(), WorldError> {
    let id = f.abox;
    agent(f, MALLORY, Kind::Set, rule, |wd| wd.update(&EntityKey::Id(id), moved(9.0), "mallory").map(|_| ()))
}

/// Mallory writes project file `path` under `rule` (the taste tools commit through
/// `Engine::project_put` exactly so).
fn taste_file(f: &mut Fix, rule: Rule, path: &'static str) -> Result<(), WorldError> {
    agent(f, MALLORY, Kind::File, rule, |wd| {
        wd.put_file(path, Some(crate::project::ProjectFile::new(crate::project::FileKind::Taste, Arc::from("{}"), 1, Arc::from("mallory"))), false);
        Ok(())
    })
}

/// Mallory writes `paths` in one commit under the taste result rule (gallery_apply, an applied
/// suggestion or an approved configurator proposal: the results with the taste state that
/// records them), after alice claims `claim` when given.
fn taste_result(f: &mut Fix, claim: Option<&'static str>, paths: &'static [&'static str]) -> Result<(), WorldError> {
    if let Some(c) = claim {
        let s1 = session("aaaaaa11");
        f.sw.take_claim(&f.live, &s1, ClaimSpec { scope: Scope::path(c).unwrap(), lease: DEFAULT_LEASE, note: String::new() })?;
    }
    agent(f, MALLORY, Kind::File, Rule::TasteResult, |wd| {
        for p in paths {
            wd.put_file(p, Some(crate::project::ProjectFile::new(crate::project::FileKind::Taste, Arc::from("{}"), 1, Arc::from("mallory"))), false);
        }
        Ok(())
    })
}

/// Alice claims the assets folder; mallory writes taste state under the taste rule, as the
/// window and the gallery worker do.
fn taste_under_claim(f: &mut Fix) -> Result<(), WorldError> {
    let s1 = session("aaaaaa11");
    f.sw.take_claim(&f.live, &s1, ClaimSpec { scope: Scope::path("/project/assets").unwrap(), lease: DEFAULT_LEASE, note: String::new() })?;
    taste_file(f, Rule::Taste, "assets/taste/galleries/g1.json")
}

/// Mallory claims the taste folder (spelled in another case).
fn claim_taste(f: &mut Fix) -> Result<(), WorldError> {
    let s2 = session("bbbbbb22");
    f.sw.take_claim(&f.live, &s2, ClaimSpec { scope: Scope::path("/project/assets/Taste").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).map(|_| ())
}

fn spawn_at(f: &mut Fix, w: Writer, path: &'static str) -> Result<(), WorldError> {
    agent(f, w, Kind::Spawn, Rule::World, |wd| wd.spawn(group(path), w.name).map(|_| ()))
}

fn undo(f: &mut Fix, seq: u64) -> Result<(), WorldError> {
    f.live.undo(&mut f.sw, 1, MALLORY, &UndoRequest { ids: Some(vec![seq]), count: 1, force: true }).map(|_| ())
}

/// A game tool (or a module call, or a live Lex command an agent made: they share
/// `game_commit`) doing `call` for mallory.
fn for_mallory(f: &mut Fix, game: &str, call: impl FnOnce(&mut crate::game::GameWorld)) -> Result<(), WorldError> {
    f.live.game_tool(&mut f.sw, 1, game, MALLORY, call)
}

fn module_hook(f: &mut Fix, call: impl FnOnce(&mut dyn WorldWrite)) -> Result<(), WorldError> {
    // As the module host commits an agent's hook call (logic::tools::handle).
    f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
        let mark = wd.mark();
        call(wd);
        let mut commit = module_agent_commit(sw, 1, MALLORY);
        commit(wd, mark, "module:nexus", Phase::Hook)
    })
    .and_then(|r| r)
}

fn module_request_at(f: &mut Fix, path: &'static str) -> Result<(), WorldError> {
    // A module's spawn request during its own tick: the request is checked for reserved data,
    // the call journaled as the module's.
    f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
        let mark = wd.mark();
        let spawned = wd.spawn(group(path), "module:nexus").map(|_| ());
        let out = spawned.and_then(|()| module_request(wd, mark));
        let mut commit = module_tick_commit(sw, 1);
        commit(wd, mark, "module:nexus", Phase::Tick).unwrap();
        out
    })
    .and_then(|r| r)
}

/// A module's spawn request (during its own tick) for `spec`.
fn module_request_spawn(f: &mut Fix, spec: SpawnSpec) -> Result<(), WorldError> {
    f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
        let mark = wd.mark();
        let out = wd.spawn(spec, "module:nexus").map(|_| ()).and_then(|()| module_request(wd, mark));
        let mut commit = module_tick_commit(sw, 1);
        commit(wd, mark, "module:nexus", Phase::Tick).unwrap();
        out
    })
    .and_then(|r| r)
}

fn exempt_move(f: &mut Fix, why: Exempt<'static>) -> Result<(), WorldError> {
    let id = f.abox;
    f.live.exempt(&mut f.sw, 1, why, |wd| wd.update(&EntityKey::Id(id), moved(3.0), why.by()).map(|_| ()))
}

/// What `procgen_scatter` sends for three placements at `at` (one `world_batch` of model spawns,
/// under `parent` if given), committed as mallory's world_batch.
fn procgen_scatter(f: &mut Fix, at: [f32; 3], parent: Option<&str>) -> Result<(), WorldError> {
    let items = vec![Json::parse(r#"{"model": {"recipe": "recipes/tombstone.json", "params": {"tier": [1, 3]}}}"#).unwrap()];
    let places: Vec<specula_model::scatter::Placement> =
        (0..3).map(|i| specula_model::scatter::Placement { position: [i as f32 * 0.5, 0.0, 0.0], rotation_deg: [0.0, 30.0 * i as f32, 0.0], scale: 1.0, item: 0, variant: i }).collect();
    let spawn = Json::parse(&parent.map_or("{}".to_string(), |p| format!(r#"{{"parent": "{p}", "name": "grave"}}"#))).unwrap();
    let crate::tools::Command::Batch(ops) = crate::procgen_tools::spawn_command(&items, &[], &places, at, &spawn).unwrap() else { panic!("procgen_scatter sends one world_batch") };
    agent(f, MALLORY, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "mallory").map(|_| ()).map_err(|(_, e)| e))
}

/// What `procgen_terrain` sends to put a heightfield collider on `path`, committed as mallory's
/// world_set.
fn procgen_collider(f: &mut Fix, path: &str) -> Result<(), WorldError> {
    let crate::tools::Command::Set(key, change) = crate::procgen_tools::collider_command(path, 3, 3, [1.0, 1.0], &[0.0; 9], [0.0; 3]).unwrap() else { panic!("procgen_terrain sends one world_set") };
    agent(f, MALLORY, Kind::Set, Rule::World, |wd| wd.update(&key, change, "mallory").map(|_| ()))
}

/// terrain_edit's commit (project_engine::file_commit): the recipe at its new text and, in the
/// same entry, the collider procgen_terrain's command would set on `collider_on`, as mallory.
/// `recipe_claimed`: alice claims the recipe first.
fn terrain_edit(f: &mut Fix, collider_on: &str, recipe_claimed: bool) -> Result<(), WorldError> {
    if recipe_claimed {
        let s1 = session("aaaaaa11");
        f.sw.take_claim(&f.live, &s1, ClaimSpec { scope: Scope::path("/project/assets/recipes/fixture.json").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    }
    let crate::tools::Command::Set(key, change) = crate::procgen_tools::collider_command(collider_on, 3, 3, [1.0, 1.0], &[0.5; 9], [0.0; 3]).unwrap() else { panic!("one world_set") };
    let file = crate::project::ProjectFile::new(crate::project::FileKind::Recipe, Arc::from("{\"edited\": true}"), 2, Arc::from("mallory"));
    agent(f, MALLORY, Kind::File, Rule::Project, |wd| crate::project_engine::file_commit(wd, "assets/recipes/fixture.json", Some(file), Some(Box::new((key, change))), "mallory"))
}

/// terrain_edit's recipe and collider are one journal entry: one undo takes both back (review of
/// a89b5eb, probe6: an undo used to revert only the collider, leaving it 8 m off the drawn
/// surface).
#[test]
fn terrain_edit_undoes_recipe_and_collider_together() {
    let mut f = fixture();
    let hook = f.live.find_path("/mg").unwrap();
    let before = (f.live.files().get("assets/recipes/fixture.json").map(|x| x.version), f.live.get(hook).map(|e| e.collider.is_some()));
    terrain_edit(&mut f, "/mg", false).unwrap();
    let seq = f.sw.journal().last_seq();
    let entry = f.sw.journal().get(seq).unwrap();
    assert!(entry.changes.iter().any(|c| matches!(c, crate::world::Change::File { .. })) && entry.touches(hook), "one entry holds both");
    assert_eq!((f.live.files().get("assets/recipes/fixture.json").map(|x| x.version), f.live.get(hook).map(|e| e.collider.is_some())), (Some(2), Some(true)));
    f.live.undo(&mut f.sw, 1, MALLORY, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
    let after = (f.live.files().get("assets/recipes/fixture.json").map(|x| x.version), f.live.get(hook).map(|e| e.collider.is_some()));
    assert_eq!(after.1, before.1, "the collider is back");
    assert_ne!(after.0, Some(2), "and so is the recipe (a new version of the old text)");
    assert_eq!(f.live.files().get("assets/recipes/fixture.json").map(|x| x.text.to_string()).as_deref(), Some("{}"));
}

/// The review queue's promotion (review_engine): the roots at `path` move under `to` in one
/// Rule::World batch, as the decider (mallory here).
fn review_promote(f: &mut Fix, path: &str, to: &str) -> Result<(), WorldError> {
    let id = f.live.find_path(path)?;
    let (stamp, entities) = crate::review::stamp(&f.live, id, crate::review::MAX_SUBTREE);
    let root = crate::review::Root { path: path.into(), id, version: f.live.get(id).map_or(0, |e| e.version), stamp, entities };
    let p = crate::review::Promote { to: to.into(), replace: false, remove_tag: None };
    let (ops, _) = crate::review_engine::promotion_ops(&f.live, &[root], &p)?;
    agent(f, MALLORY, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "mallory").map(|_| ()).map_err(|(_, e)| e))
}

/// A rejection's revert (review_engine): refused unless it takes back only the submission's own
/// work, then the policy's undo as the decider (mallory here).
/// The submission is the entry's own author's, recorded in this journal's epoch.
fn review_revert(f: &mut Fix, seq: u64) -> Result<(), WorldError> {
    let author = f.sw.journal().get(seq).map(|e| (e.by.to_string(), e.session.as_deref().map(str::to_owned)));
    let (name, key) = author.unwrap_or_default();
    review_revert_as(f, crate::board::Owner { name, key }, seq, None, None)
}

/// A revert of submission 99 covering `seq`, submitted by `by`: `epoch` and `ids` default to the
/// live journal's epoch and the entities the entry touches.
fn review_revert_as(f: &mut Fix, by: crate::board::Owner, seq: u64, epoch: Option<&str>, ids: Option<&[u64]>) -> Result<(), WorldError> {
    let live_epoch = f.sw.journal().epoch().to_owned();
    let touched: Vec<u64> = f.sw.journal().get(seq).map(|e| e.ids().to_vec()).unwrap_or_default();
    let of = crate::review_engine::RevertOf { id: 99, journal: &[seq], epoch: epoch.unwrap_or(&live_epoch), by: &by, ids: Some(ids.unwrap_or(&touched)) };
    let req = crate::review_engine::revert_request(&f.sw, &f.live, of)?;
    f.live.undo(&mut f.sw, 1, MALLORY, &req).map(|_| ())
}

fn owner(w: Writer) -> crate::board::Owner {
    crate::board::Owner { name: w.name.to_owned(), key: w.session.map(str::to_owned) }
}

const ROWS: &[(&str, Write, Expect)] = &[
    // The procgen tools (procgen_tools.rs): exactly what they send, as the calling agent.
    ("procgen_scatter's spawn batch into a claimed region", |f| procgen_scatter(f, [5.0, 0.0, 5.0], None), Expect::Refused("claimed")),
    ("procgen_scatter's spawn batch under a claimed parent", |f| procgen_scatter(f, [90.0, 0.0, 90.0], Some("/abox")), Expect::Refused("claimed")),
    ("procgen_scatter's spawn batch on free ground", |f| procgen_scatter(f, [90.0, 0.0, 90.0], None), Expect::Allowed),
    ("procgen_terrain's collider on a region-claimed entity", |f| procgen_collider(f, "/rbox"), Expect::Refused("claimed")),
    ("procgen_terrain's collider on a free entity", |f| procgen_collider(f, "/mg"), Expect::Allowed),
    // terrain_edit (terrain_tools.rs, review of a89b5eb): the recipe and the collider in one
    // Rule::Project entry, claims checked on both; a refusal leaves both as they were.
    ("terrain_edit's recipe with its collider on a claimed entity", |f| terrain_edit(f, "/rbox", false), Expect::Refused("claimed")),
    ("terrain_edit on a claimed recipe", |f| terrain_edit(f, "/mg", true), Expect::Refused("claimed")),
    ("terrain_edit's recipe and collider on free ones", |f| terrain_edit(f, "/mg", false), Expect::Allowed),
    // Agents' tools (Live::commit).
    ("world_set on a path-claimed entity", |f| set_abox(f, Rule::World), Expect::Refused("claimed")),
    ("world_set on a region-claimed entity", |f| {
        let id = f.rbox;
        agent(f, MALLORY, Kind::Set, Rule::World, |wd| wd.update(&EntityKey::Id(id), UpdateSpec { color: Some([1.0, 0.0, 0.0]), ..UpdateSpec::default() }, "mallory").map(|_| ()))
    }, Expect::Refused("claimed")),
    ("world_spawn into a claimed region", |f| agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(cube("squat", Vec3::new(5.0, 0.5, 4.0)), "mallory").map(|_| ())), Expect::Refused("claimed")),
    ("world_batch, mixed ops: a free spawn, then a claimed set (M3)", |f| {
        let id = f.abox;
        let ops = vec![Op::Spawn(cube("free", Vec3::new(90.0, 0.5, 90.0))), Op::Set(EntityKey::Id(id), moved(7.0))];
        agent(f, MALLORY, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "mallory").map(|_| ()).map_err(|(_, e)| e))
    }, Expect::Refused("claimed")),
    ("world_batch, mixed ops: a free spawn, then one in /lex (M3)", |f| {
        let ops = vec![Op::Spawn(cube("free", Vec3::new(90.0, 0.5, 90.0))), Op::Spawn(group("/lex"))];
        agent(f, MALLORY, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "mallory").map(|_| ()).map_err(|(_, e)| e))
    }, Expect::Refused("reserved")),
    ("world_batch, mixed ops: a rename carrying a claimed entity out (M1)", |f| {
        let ops = vec![Op::Spawn(group("/abox_parking")), Op::Set(EntityKey::Path("/abox".into()), UpdateSpec { name: Some("gone".into()), ..UpdateSpec::default() })];
        agent(f, MALLORY, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "mallory").map(|_| ()).map_err(|(_, e)| e))
    }, Expect::Refused("claimed")),
    ("world_delete of a jointed body whose other body is claimed (M3)", |f| {
        let id = f.hook;
        agent(f, MALLORY, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Id(id), None).map(|_| ()))
    }, Expect::Refused("claimed")),
    ("world_delete of the group holding a jointed body (M3)", |f| agent(f, MALLORY, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Path("/mg".into()), None).map(|_| ())), Expect::Refused("claimed")),
    ("world_set giving a jointed body another kind of body (M3)", |f| {
        let id = f.hook;
        agent(f, MALLORY, Kind::Set, Rule::World, |wd| wd.update(&EntityKey::Id(id), UpdateSpec { body: Some(None), ..UpdateSpec::default() }, "mallory").map(|_| ()))
    }, Expect::Refused("claimed")),
    ("world_set moving a jointed body (allowed: dragging is physics, not a write)", |f| {
        let id = f.hook;
        agent(f, MALLORY, Kind::Set, Rule::World, |wd| wd.update(&EntityKey::Id(id), UpdateSpec { position: Some(Vec3::new(1.0, 6.5, -20.0)), ..UpdateSpec::default() }, "mallory").map(|_| ()))
    }, Expect::Allowed),
    ("world_spawn of a joint (props.joint is physics_joint's) (M3)", |f| {
        let id = f.abox;
        agent(f, ALICE, Kind::Spawn, Rule::World, |wd| wd.spawn(joint_onto("tie", id), "alice").map(|_| ()))
    }, Expect::Refused("reserved")),
    ("world_spawn in /lex", |f| spawn_at(f, MALLORY, "/lex"), Expect::Refused("reserved")),
    ("world_spawn in /lex_live", |f| spawn_at(f, MALLORY, "/lex_live"), Expect::Refused("reserved")),
    ("world_spawn in /nav", |f| spawn_at(f, MALLORY, "/nav"), Expect::Refused("reserved")),
    ("world_spawn in /project", |f| spawn_at(f, MALLORY, "/project"), Expect::Refused("reserved")),
    // Owning a reserved kind never lifts claims.
    ("physics_joint's rule on a claimed entity", |f| set_abox(f, Rule::Joint), Expect::Refused("claimed")),
    ("the nav tools' rule on a claimed entity", |f| set_abox(f, Rule::Nav), Expect::Refused("claimed")),
    ("the Lex tools' rule on a claimed entity", |f| set_abox(f, Rule::Lex), Expect::Refused("claimed")),
    ("lex_live's rule on a claimed entity", |f| set_abox(f, Rule::LexLive), Expect::Refused("claimed")),
    ("the project tools' rule on a claimed entity", |f| set_abox(f, Rule::Project), Expect::Refused("claimed")),
    // Binary project files (project_upload) are project files: their pointer goes through the
    // project tools' rule like any file, and no other rule may put one.
    ("project_upload's binary file under the project tools' rule", |f| {
        let text = crate::blobs::pointer(crate::blobs::BlobId(1, 2, 0), 64);
        agent(f, MALLORY, Kind::File, Rule::Project, |wd| {
            wd.put_file("assets/textures/up.png", Some(crate::project::ProjectFile::new(crate::project::FileKind::Png, Arc::from(text), 1, Arc::from("mallory"))), false);
            Ok(())
        })
    }, Expect::Allowed),
    ("a binary file put under world_set's rule", |f| {
        let text = crate::blobs::pointer(crate::blobs::BlobId(1, 2, 0), 64);
        agent(f, MALLORY, Kind::Set, Rule::World, |wd| {
            wd.put_file("assets/textures/sneak.png", Some(crate::project::ProjectFile::new(crate::project::FileKind::Png, Arc::from(text), 1, Arc::from("mallory"))), false);
            Ok(())
        })
    }, Expect::Refused("reserved")),
    ("vfx_set's rule on a claimed entity", |f| set_abox(f, Rule::Vfx), Expect::Refused("claimed")),
    // Taste state (M4, taste lane review: a builder forged the human's pick with project_write,
    // and gallery_apply wrote it into style.toml): only the taste rule writes /project/assets/taste,
    // and it writes nothing else.
    ("the project tools' rule writing taste state (a forged pick)", |f| taste_file(f, Rule::Project, "assets/taste/galleries/g1.json"), Expect::Refused("reserved")),
    ("the project tools' rule deleting taste state", |f| {
        agent(f, MALLORY, Kind::File, Rule::Project, |wd| {
            wd.put_file("assets/taste/onboarding.json", None, false);
            Ok(())
        })
    }, Expect::Refused("reserved")),
    ("the taste tools' rule writing taste state (project_put, the window, the gallery worker)", |f| taste_file(f, Rule::Taste, "assets/taste/galleries/g1.json"), Expect::Allowed),
    ("the taste tools' rule writing outside taste state", |f| taste_file(f, Rule::Taste, "assets/style.toml"), Expect::Refused("reserved")),
    ("the taste tools' rule on a claimed entity", |f| set_abox(f, Rule::Taste), Expect::Refused("reserved")),
    // Taste review 2: a builder's claim locked the human's picks out (F5), and a forced
    // world_undo erased the owner's pick. No agent claims taste state, claims on what holds it
    // don't apply to the taste rule, and nobody undoes another's taste entry, even by force.
    ("the taste tools' rule under another agent's claim on the assets folder (the window's pick)", taste_under_claim, Expect::Allowed),
    ("a claim on taste state (in another case)", claim_taste, Expect::Refused("reserved")),
    ("world_undo of another's taste entry, forced (an erased pick)", |f| undo(f, f.taste), Expect::Refused("not_yours")),
    ("the project tools' rule writing taste state in another case (assets/Taste)", |f| taste_file(f, Rule::Project, "assets/Taste/galleries/g1.json"), Expect::Refused("reserved")),
    // M4 final audit: gallery_apply and taste_suggest {apply} committed their results and then
    // the gallery (or the log) separately: two commits for one tool call (AGENTS rule 14). The
    // taste result rule writes both in one commit: results claim-checked, taste state not.
    ("the taste result rule writing a result with the gallery that records it (one commit)", |f| taste_result(f, None, &["assets/style.toml", "assets/taste/galleries/g1.json"]), Expect::Allowed),
    ("the taste result rule on a claimed entity", |f| set_abox(f, Rule::TasteResult), Expect::Refused("reserved")),
    ("the taste result rule writing a result another agent claims", |f| taste_result(f, Some("/project/assets"), &["assets/style.toml", "assets/taste/galleries/g1.json"]), Expect::Refused("claimed")),
    ("the taste result rule's taste state under another agent's claim on the assets folder", |f| taste_result(f, Some("/project/assets"), &["assets/taste/suggestions.json"]), Expect::Allowed),
    ("the GI tools' rule on a claimed entity", |f| set_abox(f, Rule::Gi), Expect::Refused("claimed")),
    // Baked GI volumes (/gi, props.gi_volume: only gi_bake).
    ("world_spawn in /gi", |f| spawn_at(f, MALLORY, "/gi"), Expect::Refused("reserved")),
    ("world_spawn of GI volume settings outside /gi", |f| {
        let mut s = group("/sneak");
        s.props = Json::obj([(crate::gi::PROP, Json::obj([("min", Json::arr([0.0, 0.0, 0.0].map(Json::from))), ("max", Json::arr([1.0, 1.0, 1.0].map(Json::from)))]))]);
        agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(s, "mallory").map(|_| ()))
    }, Expect::Refused("reserved")),
    ("a game tool writing /gi", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(group("/gi"), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("the GI tools' rule adding /gi", |f| agent(f, MALLORY, Kind::Spawn, Rule::Gi, |wd| wd.spawn(group("/gi"), "mallory").map(|_| ())), Expect::Allowed),
    ("the GI tools' rule placing a volume in a claimed region", |f| agent(f, MALLORY, Kind::Spawn, Rule::Gi, |wd| {
        let mut g = group("/gi");
        g.position = Vec3::new(5.0, 0.5, 5.0);
        wd.spawn(g, "mallory").map(|_| ())
    }), Expect::Refused("claimed")),
    // The effect library (/vfx, the vfx_effect tag: only vfx_set) and the effect rules
    // (crate::vfx::write_rules: effects named must exist; nothing may flash more than 3 times a
    // second) on every checked path.
    ("world_spawn in /vfx", |f| spawn_at(f, MALLORY, "/vfx/sneak"), Expect::Refused("reserved")),
    ("world_spawn tagging an effect outside /vfx", |f| {
        let mut s = effect("glow", 0.05);
        s.parent = None;
        agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(s, "mallory").map(|_| ()))
    }, Expect::Refused("reserved")),
    ("world_set moving an effect out of /vfx", |f| agent(f, MALLORY, Kind::Set, Rule::World, |wd| {
        wd.update(&EntityKey::Path("/vfx/spark".into()), UpdateSpec { parent: Some(None), ..UpdateSpec::default() }, "mallory").map(|_| ())
    }), Expect::Refused("reserved")),
    ("world_delete of an effect", |f| agent(f, MALLORY, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Path("/vfx/spark".into()), None).map(|_| ())), Expect::Refused("reserved")),
    ("vfx_set's rule on a claimed effect", |f| agent(f, MALLORY, Kind::Set, Rule::Vfx, |wd| {
        wd.update(&EntityKey::Path("/vfx/glow".into()), UpdateSpec { props: Some(Json::obj([(crate::vfx::EFFECT_PROP, calm(0.02))])), ..UpdateSpec::default() }, "mallory").map(|_| ())
    }), Expect::Refused("claimed")),
    ("vfx_set's rule adding an effect", |f| agent(f, MALLORY, Kind::Spawn, Rule::Vfx, |wd| wd.spawn(effect("ember", 0.05), "mallory").map(|_| ())), Expect::Allowed),
    ("vfx_set's rule changing an effect so an instance playing it flashes", |f| agent(f, MALLORY, Kind::Set, Rule::Vfx, |wd| {
        wd.update(&EntityKey::Path("/vfx/spark".into()), UpdateSpec { props: Some(Json::obj([(crate::vfx::EFFECT_PROP, calm(0.05))])), ..UpdateSpec::default() }, "mallory").map(|_| ())
    }), Expect::Refused("photosensitive")),
    ("world_spawn playing an effect there isn't", |f| agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(playing("odd", "nowhere", 1.0, 1.0), "mallory").map(|_| ())), Expect::Refused("unknown_effect")),
    ("world_spawn playing an effect with params that flash", |f| agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(playing("loud", "glow", 4.0, 20.0), "mallory").map(|_| ())), Expect::Refused("photosensitive")),
    ("world_spawn playing an effect calmly", |f| agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(playing("quiet", "spark", 1.0, 1.0), "mallory").map(|_| ())), Expect::Allowed),
    ("world_set turning an instance's params up until it flashes", |f| agent(f, MALLORY, Kind::Set, Rule::World, |wd| {
        let mut v = specula_scintilla::VfxSpec::new("glow");
        (v.params.rate, v.params.size) = (4.0, 20.0);
        wd.update(&EntityKey::Path("/lamp".into()), UpdateSpec { vfx: Some(Some(v)), ..UpdateSpec::default() }, "mallory").map(|_| ())
    }), Expect::Refused("photosensitive")),
    // world_undo (Live::undo), forced (conflicts aside, claims always apply).
    ("world_undo of a change to a claimed entity", |f| undo(f, f.edit), Expect::Refused("claimed")),
    ("world_undo of a joint's removal onto a claimed body (M3)", |f| undo(f, f.unjoint), Expect::Refused("claimed")),
    ("world_undo restoring a jointed body (M3)", |f| undo(f, f.unhook), Expect::Refused("claimed")),
    // What agents ask the hosted game, a module or a live Lex run to do (game_commit).
    ("a game tool renaming a claimed entity (M1)", |f| for_mallory(f, "sepulcra", |wd| {
        wd.update(&EntityKey::Path("/abox".into()), UpdateSpec { name: Some("razed".into()), ..UpdateSpec::default() }, "sepulcra").unwrap();
    }), Expect::Refused("claimed")),
    ("a game tool writing /project (M3)", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(group("/project"), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("a game tool writing /lex (M3)", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(group("/lex"), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("a game tool writing /lex_live (M3)", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(group("/lex_live"), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("a game tool writing /nav", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(group("/nav"), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("a game tool writing /vfx", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(effect("forge", 0.05), "sepulcra").unwrap();
    }), Expect::Refused("reserved")),
    ("a game tool an agent called playing an effect with params that flash", |f| for_mallory(f, "sepulcra", |wd| {
        wd.spawn(playing("loud", "glow", 4.0, 20.0), "sepulcra").unwrap();
    }), Expect::Refused("photosensitive")),
    ("a live Lex command's hooks playing an effect there isn't", |f| for_mallory(f, "lex:siege", |wd| {
        wd.spawn(playing("odd", "nowhere", 1.0, 1.0), "lex:siege").unwrap();
    }), Expect::Refused("unknown_effect")),
    ("a module hook an agent called tying a joint onto a claimed body (M3)", |f| {
        let id = f.abox;
        module_hook(f, |wd| {
            wd.spawn(joint_onto("haul", id), "module:nexus").unwrap();
        })
    }, Expect::Refused("claimed")),
    ("a module hook an agent called deleting a claimed entity", |f| module_hook(f, |wd| {
        wd.delete(&EntityKey::Path("/abox".into()), None).unwrap();
    }), Expect::Refused("claimed")),
    ("a live Lex command's hooks deleting a claimed entity (M3)", |f| for_mallory(f, "lex:siege", |wd| {
        wd.delete(&EntityKey::Path("/rbox".into()), None).unwrap();
    }), Expect::Refused("claimed")),
    ("a live Lex command's hooks writing /lex (M3)", |f| for_mallory(f, "lex:siege", |wd| {
        wd.spawn(group("/lex"), "lex:siege").unwrap();
    }), Expect::Refused("reserved")),
    // A module's own requests: reserved data (claims don't hold the game's rules back).
    ("a module request writing /project (M3)", |f| module_request_at(f, "/project"), Expect::Refused("reserved")),
    ("a module request writing /lex (M3)", |f| module_request_at(f, "/lex"), Expect::Refused("reserved")),
    ("a module request writing /lex_live (M3)", |f| module_request_at(f, "/lex_live"), Expect::Refused("reserved")),
    ("a module request writing /nav", |f| module_request_at(f, "/nav"), Expect::Refused("reserved")),
    ("a module request writing /vfx", |f| module_request_at(f, "/vfx/forge"), Expect::Refused("reserved")),
    ("a module request playing an effect with params that flash", |f| module_request_spawn(f, playing("loud", "glow", 4.0, 20.0)), Expect::Refused("photosensitive")),
    ("a module request playing an effect there isn't", |f| module_request_spawn(f, playing("odd", "nowhere", 1.0, 1.0)), Expect::Refused("unknown_effect")),
    // The documented exemptions: the simulation's own writers move a claimed entity, journaled
    // under their names.
    ("the hosted game's tick (exempt)", |f| exempt_move(f, Exempt::Game { name: "sepulcra", kind: Kind::GameTick }), Expect::Exempt("sepulcra")),
    ("the hosted game's start (exempt)", |f| exempt_move(f, Exempt::GameStart { name: "sepulcra" }), Expect::Exempt("sepulcra")),
    ("physics' pose write-back (exempt)", |f| exempt_move(f, Exempt::Physics), Expect::Exempt("physics")),
    ("navigation's steering (exempt)", |f| exempt_move(f, Exempt::Nav), Expect::Exempt("nav")),
    ("an anim reload resettling graphs (exempt)", |f| exempt_move(f, Exempt::AnimReload), Expect::Exempt("anim_reload")),
    ("a module's tick (exempt)", |f| {
        let id = f.abox;
        f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
            let mark = wd.mark();
            wd.update(&EntityKey::Id(id), moved(4.0), "module:nexus").map(|_| ())?;
            module_tick_commit(sw, 1)(wd, mark, "module:nexus", Phase::Tick)
        })
        .and_then(|r| r)
    }, Expect::Exempt("module:nexus")),
    ("a live Lex run's own tick (exempt)", |f| {
        let id = f.abox;
        f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, sw| {
            wd.update(&EntityKey::Id(id), moved(5.0), "lex:siege").map(|_| ())?;
            journal_exempt(wd, sw, 1, Exempt::LexLive { agent: "lex:siege" });
            Ok(())
        })
        .and_then(|r| r)
    }, Expect::Exempt("lex:siege")),
    ("files read from disk (exempt: a human's edit)", |f| f.live.disk(&mut f.sw, 1, |wd| {
        wd.put_file("assets/recipes/t.json", Some(crate::project::ProjectFile::new(crate::project::FileKind::Recipe, Arc::from("{}"), 1, Arc::from("disk"))), true);
        Ok(())
    }).map(|_| ()), Expect::Exempt("disk")),
    // The review queue (M4): promotion is a Rule::World batch as the decider; a revert is the
    // policy's undo as the decider, after the queue's own refusals.
    ("review promotion moving work into a claimed path (M4)", |f| review_promote(f, "/mg", "/abox"), Expect::Refused("claimed")),
    ("review promotion of free work (M4)", |f| {
        agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(group("/game"), "mallory").map(|_| ()))?;
        review_promote(f, "/mg", "/game")
    }, Expect::Allowed),
    ("review revert of work onto a claimed entity (M4)", |f| review_revert(f, f.edit), Expect::Refused("claimed")),
    ("review revert of work someone changed since (M4)", |f| {
        let abox = f.abox;
        let spawned = f.sw.journal().entries().find(|e| e.touches(abox)).map_or(0, |e| e.seq);
        review_revert(f, spawned)
    }, Expect::Refused("revert_conflict")),
    // The M4 integration audit (probe_revert.py): a submission saved with its world named journal
    // seqs of an earlier run; after a restart the journal counts from 1 again, and the revert
    // undid another agent's entities. Refs carry their epoch, and each entry must be the
    // submitter's and touch only what it touched at submit.
    ("review revert of journal refs from an earlier run (M4 audit)", |f| {
        let seq = f.unhook;
        review_revert_as(f, owner(MALLORY), seq, Some("0123456789abcdef.0.1"), None)
    }, Expect::Refused("journal_from_earlier_run")),
    ("review revert of journal refs saved before epochs (M4 audit)", |f| {
        let seq = f.unhook;
        review_revert_as(f, owner(MALLORY), seq, Some(""), None)
    }, Expect::Refused("journal_from_earlier_run")),
    ("review revert of another agent's entry (M4 audit)", |f| {
        let seq = f.unhook;
        review_revert_as(f, owner(ALICE), seq, None, None)
    }, Expect::Refused("not_submitters_entry")),
    ("review revert touching entities not recorded at submit (M4 audit)", |f| {
        let seq = f.unhook;
        review_revert_as(f, owner(MALLORY), seq, None, Some(&[]))
    }, Expect::Refused("revert_scope_changed")),
    ("review revert of the submitter's own free entry (M4 audit)", |f| {
        agent(f, MALLORY, Kind::Spawn, Rule::World, |wd| wd.spawn(group("/junk"), "mallory").map(|_| ()))?;
        let seq = f.sw.journal().last_seq();
        review_revert(f, seq)
    }, Expect::Allowed),
    // Replacing the world (world_load, save_read, startup).
    ("world_load while another agent holds claims (M1)", |f| may_replace(&f.sw, MALLORY, &crate::playtest_engine::Playtests::default()).map(|_| ()), Expect::Refused("claimed")),
    ("world_load under a playtest (M3)", |f| may_replace(&f.sw, ALICE, &crate::playtest_engine::Playtests::pretend_running()).map(|_| ()), Expect::Refused("playtest_running")),
    // Replacing the live world wholesale (review round 2, probe2.rs: a game tool's
    // `*wd = World::default()` wiped claimed boxes unnoticed). No writer can any more: games get
    // a GameWorld and every other writer a `dyn WorldWrite`, never `&mut World` (compile_fail
    // doctests on both). The backstop is tested with a writer that holds the world anyway.
    ("an exempt writer replacing the live world (*w = World::default())", |f| {
        f.live.raw_writer_for_test(&mut f.sw, 1, None, |wd| *wd = World::default());
        Ok(())
    }, Expect::Replaced),
    ("an exempt writer swapping the live world out (mem::swap)", |f| {
        f.live.raw_writer_for_test(&mut f.sw, 1, None, |wd| {
            let mut other = World::default();
            std::mem::swap(wd, &mut other)
        });
        Ok(())
    }, Expect::Replaced),
    ("an exempt writer taking the live world (mem::take)", |f| {
        f.live.raw_writer_for_test(&mut f.sw, 1, None, |wd| drop(std::mem::take(wd)));
        Ok(())
    }, Expect::Replaced),
    ("a session putting a copy in the live world's place (clone)", |f| {
        f.live.raw_writer_for_test(&mut f.sw, 1, None, |wd| *wd = wd.clone());
        Ok(())
    }, Expect::Replaced),
];

/// GI bakes (review of the GI lane, finding 5): a bake whose settings are unchanged writes
/// nothing at its start, yet it occupies the baker and lands its result in the volume, so the
/// volume's claims hold there too; a running bake is cancelled only by its starter or by an agent
/// who may write the volume, naming it. (Checks, not writes: so tests beside the table.)
#[test]
fn gi_bakes_and_their_cancels_hold_claims() {
    use crate::gi_tools::{may_cancel, may_write_volume};
    let mut f = fixture();
    let mut yard = group("/gi/yard");
    yard.position = Vec3::new(5.0, 0.5, 5.0); // inside Alice's region claim
    let mut free = group("/gi/free");
    free.position = Vec3::new(90.0, 0.5, 90.0);
    agent(&mut f, ALICE, Kind::Batch, Rule::Gi, |wd| {
        wd.spawn(group("/gi"), "alice")?;
        wd.spawn(yard, "alice")?;
        wd.spawn(free, "alice").map(|_| ())
    })
    .unwrap();
    assert_eq!(may_write_volume(&f.sw, &f.live, "yard", MALLORY).unwrap_err().code, "claimed", "an unchanged bake of a claimed volume");
    assert!(may_write_volume(&f.sw, &f.live, "yard", ALICE).is_ok());
    assert!(may_write_volume(&f.sw, &f.live, "free", MALLORY).is_ok());
    assert!(may_write_volume(&f.sw, &f.live, "new", MALLORY).is_ok(), "a new volume: its spawn is checked when written");
    // Alice's bake of the yard.
    let alice = ALICE.session.unwrap();
    assert_eq!(may_cancel(&f.sw, &f.live, "yard", alice, "yard", MALLORY, false).unwrap_err().code, "not_job_owner", "another agent's bake needs force");
    assert_eq!(may_cancel(&f.sw, &f.live, "yard", alice, "yard", MALLORY, true).unwrap_err().code, "force_needs_lead", "and force the lead (authority::may_force)");
    assert_eq!(may_cancel(&f.sw, &f.live, "yard", alice, "free", ALICE, false).unwrap_err().code, "wrong_volume", "the volume named must be the one baking");
    assert!(may_cancel(&f.sw, &f.live, "yard", alice, "yard", ALICE, false).is_ok());
    // Alice's bake of a free volume: still hers (force, the lead's, for anyone else).
    assert_eq!(may_cancel(&f.sw, &f.live, "free", alice, "free", MALLORY, true).unwrap_err().code, "force_needs_lead");
}

/// Every row from the same fixture: refused rows leave the world and journal exactly as they
/// were (nothing pending), allowed and exempt ones journal under the right name, and the journal
/// replays to the live world afterwards.
#[test]
fn every_write_path_holds_the_policy() {
    for (name, write, expect) in ROWS {
        let mut f = fixture();
        let (before, last, next_id) = (contents(&f.live), f.sw.journal().last_seq(), f.live.next_id());
        let (time, files, revision) = (f.live.world_time(), f.live.files().len(), f.live.revision());
        let out = if *expect == Expect::Replaced {
            // (A debug build asserts once it has recovered.)
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write(&mut f)));
            assert_eq!(caught.is_err(), cfg!(debug_assertions), "{name}: reported");
            Ok(())
        } else {
            write(&mut f)
        };
        assert!(!f.live.has_unjournaled(), "{name}: nothing left pending");
        match expect {
            Expect::Refused(code) => {
                let e = out.expect_err(name);
                assert_eq!(e.code, *code, "{name}: {}", e.message);
                assert_eq!(contents(&f.live), before, "{name}: the world is as before (only the id counter moved on)");
                // (A module call whose request was refused still journals the ids it used.)
                assert!(f.sw.journal().entries().filter(|e| e.seq > last).all(|e| e.changes.is_empty()), "{name}: no change journaled");
            }
            Expect::Exempt(by) => {
                out.unwrap_or_else(|e| panic!("{name}: {e}"));
                let entry = f.sw.journal().entries().last().unwrap();
                assert!(entry.seq > last, "{name}: journaled");
                assert_eq!((&*entry.by, entry.via.as_deref()), (*by, None), "{name}: under its own name");
            }
            Expect::Allowed => {
                out.unwrap_or_else(|e| panic!("{name}: {e}"));
                let entry = f.sw.journal().entries().last().unwrap();
                assert!(entry.seq > last && entry.is_by(MALLORY), "{name}: journaled as the agent's");
            }
            Expect::Replaced => {
                out.unwrap();
                assert_eq!((f.live.strays(), f.live.replacements().0), (1, 1), "{name}: reported and counted");
                assert!(f.live.take_replaced(), "{name}: the engine is told (its caches start over)");
                assert!(f.live.is_marked_live() && f.live.recording() == Recording::Full, "{name}: live and recording");
                assert_eq!((f.live.world_time(), f.live.files().len()), (time, files), "{name}: the clock and project files are kept");
                assert!(f.live.revision() > revision, "{name}: no revision repeats");
                assert!(f.sw.journal().stats_json().get("base").and_then(Json::as_str).is_some_and(|b| b.starts_with("engine:replaced_by:")), "{name}: the journal starts over");
            }
        }
        assert!(f.live.next_id() >= next_id, "{name}: ids are never handed out again");
        let replayed = f.sw.journal().replay().expect("replayable");
        assert_eq!(replayed.state_hash(), f.live.state_hash(), "{name}: the journal replays to the live world");
    }
}

/// The table covers every writer the policy knows (a new variant needs a row, or this fails to
/// compile until it has one).
#[test]
fn the_table_names_every_writer() {
    let names: Vec<&str> = ROWS.iter().map(|(n, _, _)| *n).collect();
    let has = |s: &str| names.iter().any(|n| n.contains(s));
    for rule in [Rule::World, Rule::Joint, Rule::Nav, Rule::Lex, Rule::LexLive, Rule::Project, Rule::Vfx, Rule::Taste, Rule::TasteResult, Rule::Gi] {
        let row = match rule {
            Rule::World => "world_set",
            Rule::Joint => "physics_joint's rule",
            Rule::Nav => "the nav tools' rule",
            Rule::Lex => "the Lex tools' rule",
            Rule::LexLive => "lex_live's rule",
            Rule::Project => "the project tools' rule",
            Rule::Vfx => "vfx_set's rule",
            Rule::Taste => "the taste tools' rule",
            Rule::TasteResult => "the taste result rule",
            Rule::Gi => "the GI tools' rule",
        };
        assert!(has(row), "{rule:?} has a row");
    }
    for why in [Exempt::Game { name: "g", kind: Kind::GameTick }, Exempt::GameStart { name: "g" }, Exempt::Module { agent: "m", kind: Kind::GameTick }, Exempt::LexLive { agent: "l" }, Exempt::Physics, Exempt::Nav, Exempt::AnimReload] {
        let row = match why {
            Exempt::Game { .. } => "the hosted game's tick",
            Exempt::GameStart { .. } => "the hosted game's start",
            Exempt::Module { .. } => "a module's tick",
            Exempt::LexLive { .. } => "a live Lex run's own tick",
            Exempt::Physics => "physics' pose write-back",
            Exempt::Nav => "navigation's steering",
            Exempt::AnimReload => "an anim reload",
        };
        assert!(has(row), "{why:?} has a row");
    }
    for opener in [Opener::Modules, Opener::LexLive, Opener::LexTools] {
        let row = match opener {
            Opener::Modules => "a module hook",
            Opener::LexLive => "a live Lex",
            // The Lex tools commit with Rule::Lex.
            Opener::LexTools => "the Lex tools' rule",
        };
        assert!(has(row), "{opener:?} has a row");
    }
}

/// A writer that changes the live world without journaling it is caught when its entry point
/// ends, and fails closed: the changes are rolled back (full changes carry their state before),
/// reported and counted (and a debug assertion).
#[test]
fn a_writer_that_leaves_changes_unjournaled_is_caught() {
    let mut f = fixture();
    let before = contents(&f.live);
    let stray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, _| {
            wd.spawn(cube("forgotten", Vec3::new(70.0, 0.5, 70.0)), "lex:siege").unwrap();
        });
    }));
    assert_eq!(stray.is_err(), cfg!(debug_assertions), "a debug build stops there");
    assert!(!f.live.has_unjournaled());
    assert_eq!(f.live.strays(), 1);
    assert!(f.live.find_path("/forgotten").is_err(), "rolled back");
    assert_eq!(contents(&f.live), before);
    assert_eq!(f.sw.journal().replay().unwrap().state_hash(), f.live.state_hash());
}

macro_rules! call0 {
    ($w:expr, $m:ident) => {
        $w.$m()
    };
}

/// Review round 3 (probes/evasion.rs): writes hidden or the world replaced inside an agent's
/// commit, every spelling the text guard can't see. No handle can spell them any more (round 4), so
/// they run as a command holding the world itself (the backstop): sealed (taking changes or
/// switching recording does nothing), marked (replacements lack it), moves counted. Each must
/// leave alice's claimed box where it was (or the world consistent), be
/// counted, refuse, and replay.
#[test]
fn hiding_or_replacing_inside_a_commit_is_caught_however_spelled() {
    type Evasion = fn(&mut World, u64);
    let rows: &[(&str, Evasion, &str)] = &[
        ("E1: World::take_changes(wd) in path form", |wd, id| {
            wd.update(&EntityKey::Id(id), moved(9.0), "mallory").unwrap();
            World::take_changes(wd);
        }, "claimed"),
        ("E5: a macro calling $w.$m()", |wd, id| {
            wd.update(&EntityKey::Id(id), moved(9.0), "mallory").unwrap();
            let _ = call0!(wd, take_changes);
        }, "claimed"),
        ("recording switched off first", |wd, id| {
            wd.set_recording(Recording::Off);
            wd.update(&EntityKey::Id(id), moved(9.0), "mallory").unwrap();
        }, "claimed"),
        ("E2: mem::swap with a world built unrecorded", |wd, _| {
            let mut other = World::default();
            std::mem::swap(wd, &mut other);
        }, "unjournaled_write"),
        ("E2b: through a renamed reference (let t = wd; *t = ..)", |wd, _| {
            let target = wd;
            *target = World::default();
        }, "unjournaled_write"),
        ("E2c: clone_from", |wd, _| wd.clone_from(&World::default()), "unjournaled_write"),
        ("E2d: swap imported under another name", |wd, _| {
            use std::mem::swap as exchange;
            let mut other = World::default();
            exchange(wd, &mut other);
        }, "unjournaled_write"),
        ("E10: a modified clone put in place", |wd, id| {
            let mut c = wd.clone();
            let _ = c.update(&EntityKey::Id(id), moved(21.0), "mallory");
            *wd = c;
        }, "unjournaled_write"),
    ];
    for (name, evade, code) in rows {
        let mut f = fixture();
        let (id, at) = (f.abox, f.live.get(f.abox).unwrap().position);
        let mut out = None;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            out = Some(f.live.raw_writer_for_test(&mut f.sw, 1, Some(MALLORY), |wd| evade(wd, id)));
        }));
        if *code == "claimed" {
            // The seal kept the write recorded: it reached the claims and was refused.
            assert!(caught.is_ok(), "{name}: refused, not a crash");
            assert_eq!(out, Some(false), "{name}: refused");
            assert_eq!(f.live.get(id).unwrap().position, at, "{name}: alice's box didn't move");
        } else {
            // A replaced world: reported (a debug build asserts once it recovered), refused.
            assert_eq!(caught.is_err(), cfg!(debug_assertions), "{name}: reported");
            assert_ne!(out, Some(true), "{name}: refused");
            assert!(f.live.replacements().0 >= 1, "{name}: counted as a replacement");
            assert!(f.live.get(id).is_none_or(|e| e.position == at), "{name}: no claimed change got through as a journaled write");
        }
        assert!(f.live.strays() >= 1, "{name}: counted");
        assert!(f.live.is_marked_live() && f.live.is_sealed() && !f.live.has_unjournaled(), "{name}: the live world is consistent");
        assert_eq!(f.sw.journal().replay().unwrap().state_hash(), f.live.state_hash(), "{name}: replay stays exact");
    }
}

/// The hosted game can't hide its writes: it gets a GameWorld, which has no way to switch
/// recording or take the recorded changes (compile_fail doctests on GameWorld; review round 1's
/// probe1.rs switched to Snapshot). Writers in the crate that hold `&mut World` run sealed:
/// trying either changes nothing, the writes are journaled as usual, and the attempt is reported
/// and counted.
#[test]
fn a_sealed_writer_cant_hide_its_writes() {
    let mut f = fixture();
    let id = f.abox;
    let last = f.sw.journal().last_seq();
    let at = f.live.get(id).unwrap().position;
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.live.raw_writer_for_test(&mut f.sw, 1, None, |wd| {
            wd.set_recording(Recording::Off);
            wd.update(&EntityKey::Id(id), moved(6.0), "physics").unwrap();
            assert!(wd.take_changes().is_empty(), "nothing to take while sealed");
            wd.set_recording(Recording::Snapshot);
        })
    }));
    assert_eq!(caught.is_err(), cfg!(debug_assertions), "reported");
    // Sealed: the write stayed recorded (full), so the backstop rolled it back.
    assert_eq!(f.live.get(id).unwrap().position, at);
    assert!(f.sw.journal().last_seq() == last, "nothing journaled");
    assert!(f.live.strays() >= 2 && f.live.recording() == Recording::Full, "the break and the stray are counted");
    assert_eq!(f.sw.journal().replay().unwrap().state_hash(), f.live.state_hash());
}

/// Exempt writers record as they always did: per-tick writers one change per entity (cheap,
/// replay-only), the game's start and anim reloads full states; the live world records full
/// changes again afterwards.
#[test]
fn exempt_writers_record_as_documented() {
    let mut f = fixture();
    let id = f.abox;
    for why in [Exempt::Physics, Exempt::GameStart { name: "sepulcra" }] {
        f.live.exempt(&mut f.sw, 1, why, |wd| {
            let expected = if why == Exempt::Physics { Recording::Snapshot } else { Recording::Full };
            assert_eq!(wd.recording(), expected, "{why:?}");
            wd.update(&EntityKey::Id(id), moved(2.0), why.by()).unwrap();
            wd.update(&EntityKey::Id(id), moved(2.5), why.by()).unwrap();
        });
        assert_eq!(f.live.recording(), Recording::Full);
        let entry = f.sw.journal().entries().last().unwrap();
        let undoable = entry.undoable();
        assert_eq!((entry.changes.len(), undoable), if why == Exempt::Physics { (1, false) } else { (2, true) }, "{why:?}");
    }
}

/// The fast path: with no claim that could refuse the writer, an agent's batch looks at no
/// joint and checks no claim step (the joint index fast path stays O(changes)).
#[test]
fn with_no_blocking_claim_a_batch_does_no_claim_work() {
    let mut f = fixture();
    let before = crate::claims::joint_rule_work();
    let ops: Vec<Op> = (0..500).map(|i| Op::Spawn(cube(&format!("n{i}"), Vec3::new(i as f32, 0.5, 300.0)))).collect();
    // Alice holds the only claims: nothing can refuse her.
    agent(&mut f, ALICE, Kind::Batch, Rule::World, |wd| wd.apply_batch(ops, "alice").map(|_| ()).map_err(|(_, e)| e)).unwrap();
    assert_eq!(crate::claims::joint_rule_work() - before, 0);
}

/// Owner decision pending (coordinator, round 3): module and live Lex ticks stay exempt from
/// claims (deterministic), but a write one makes inside another agent's claim is told to the
/// claim's holder: a notice naming the module or run and the agent who started it.
#[test]
fn ticks_writing_inside_a_claim_notify_its_holder() {
    let mut f = fixture();
    let id = f.abox;
    f.sw.note_starter("module:nexus", "mallory");
    f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
        let mark = wd.mark();
        wd.update(&EntityKey::Id(id), moved(4.0), "module:nexus").unwrap();
        module_tick_commit(sw, 1)(wd, mark, "module:nexus", Phase::Tick).unwrap();
    })
    .unwrap();
    f.live.session(&mut f.sw, 2, Opener::LexLive, |wd, sw| {
        wd.update(&EntityKey::Path("/rbox".into()), moved(5.0), "lex:siege").unwrap();
        journal_exempt(wd, sw, 2, Exempt::LexLive { agent: "lex:siege" });
    })
    .unwrap();
    let notices = f.sw.claim_notices(Some("alice"), 20);
    let n = notices.as_array().unwrap();
    assert_eq!(n.len(), 2, "{notices}");
    assert_eq!((n[0].get("by").and_then(Json::as_str), n[0].get("started_by").and_then(Json::as_str), n[0].get("path").and_then(Json::as_str)), (Some("module:nexus"), Some("mallory"), Some("/abox")));
    assert_eq!(n[1].get("by").and_then(Json::as_str), Some("lex:siege"));
    assert!(f.sw.claim_notices(Some("mallory"), 20).as_array().unwrap().is_empty(), "only the holder is told");
    // Not refused: the ticks' writes are journaled as theirs.
    assert_eq!(f.live.get(id).unwrap().position, moved(4.0).position.unwrap());
}

/// The hosted game's own calls writing reserved data are reported and counted (not refused).
#[test]
fn the_games_own_reserved_writes_are_reported() {
    let mut f = fixture();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.live.exempt(&mut f.sw, 1, Exempt::Game { name: "sepulcra", kind: Kind::GameTick }, |wd| {
            wd.spawn(group("/lex"), "sepulcra").unwrap();
        });
    }));
    assert_eq!(caught.is_err(), cfg!(debug_assertions), "a debug build stops there");
    assert_eq!(f.live.game_reserved_writes(), 1);
    assert!(f.live.find_path("/lex").is_ok(), "journaled, not refused");
    assert_eq!(f.sw.journal().replay().unwrap().state_hash(), f.live.state_hash());
}

// Review round 4's probes (review-policy-a8ae760), each a permanent test. The handle makes most of
// them unspellable (compile_fail doctests on `WorldWrite` and `Swarm::journal`); what a writer
// in the crate can still reach (`recording_as`) and the runtime backstop (a writer holding the
// world itself) are tested here.

/// Runs `f`, catching the debug assertion a reported bug raises: whether it was raised.
fn reported(f: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}

fn replay_matches(f: &Fix) -> bool {
    f.sw.journal().replay().unwrap().state_hash() == f.live.state_hash()
}

/// P1, P1b: an agent's command switching recording (Off, Snapshot) to hide a write into a claim.
/// Inside a command recording stays full, so the write reaches the claims and is refused; the
/// attempt is reported and counted.
#[test]
fn p1_a_command_cant_switch_recording_to_hide_a_write() {
    for mode in [Recording::Off, Recording::Snapshot] {
        let mut f = fixture();
        let (id, at, s0) = (f.abox, f.live.get(f.abox).unwrap().position, f.live.strays());
        let out = f.live.commit(&mut f.sw, 1, MALLORY, Kind::Set, Rule::World, |wd| recording_as(wd, mode, |w| w.update(&EntityKey::Id(id), moved(9.0), "mallory").map(|_| ())));
        assert_eq!(out.err().map(|e| e.code), Some("claimed"), "{mode:?}: refused by the claims");
        assert_eq!(f.live.get(id).unwrap().position, at, "{mode:?}: alice's box didn't move");
        assert!(f.live.strays() > s0, "{mode:?}: the attempt is counted");
        assert!(replay_matches(&f), "{mode:?}");
    }
}

/// P1c, P1d, P1e: exempt writers and sessions may record one change per entity, but a write made
/// with recording off is hidden: found whatever else the writer journaled or left pending (the
/// counter journaling never clears), reported, and the journal starts over so replay stays exact.
/// The pending write in P1e is rolled back; the session answers with a refusal.
#[test]
fn p1_hidden_writes_in_exempt_writers_and_sessions_are_caught() {
    // P1c: an exempt writer.
    let mut f = fixture();
    let (id, s0) = (f.abox, f.live.strays());
    assert_eq!(reported(|| f.live.exempt(&mut f.sw, 1, Exempt::Physics, |wd| recording_as(wd, Recording::Off, |w| {
                w.update(&EntityKey::Id(id), moved(9.0), "physics").unwrap();
            }))), cfg!(debug_assertions));
    assert!(f.live.strays() > s0 && replay_matches(&f), "P1c");
    // P1d: a session that journals after hiding.
    let mut f = fixture();
    let s0 = f.live.strays();
    let mut out = None;
    assert_eq!(
        reported(|| {
            out = Some(f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, sw| {
                recording_as(wd, Recording::Off, |w| {
                w.update(&EntityKey::Id(id), moved(9.0), "lex:x").unwrap();
            });
                journal_exempt(wd, sw, 1, Exempt::LexLive { agent: "lex:x" });
            }))
        }),
        cfg!(debug_assertions)
    );
    assert!(f.live.strays() > s0 && replay_matches(&f), "P1d");
    if let Some(o) = out {
        assert_eq!(o.err().map(|e| e.code), Some("unjournaled_write"), "P1d: refused");
    }
    // P1e: hidden plus pending.
    let mut f = fixture();
    let s0 = f.live.strays();
    let reported_e = reported(|| {
        let _ = f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, _| {
            recording_as(wd, Recording::Off, |w| {
                w.update(&EntityKey::Id(id), moved(9.0), "lex:x").unwrap();
            });
            wd.spawn(cube("forgotten", Vec3::new(70.0, 0.5, 70.0)), "lex:x").unwrap();
        });
    });
    assert_eq!(reported_e, cfg!(debug_assertions));
    assert!(f.live.find_path("/forgotten").is_err(), "P1e: the pending spawn was rolled back");
    assert!(f.live.strays() > s0 && !f.live.has_unjournaled() && replay_matches(&f), "P1e");
}

/// P1f: the journal starting over after a hidden write is deterministic (same world, ids,
/// revision and journal base every run), and undo still works afterwards.
#[test]
fn p1f_starting_over_after_a_hidden_write_is_deterministic() {
    let run = || {
        let mut f = fixture();
        let id = f.abox;
        reported(|| {
            let _ = f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, _| recording_as(wd, Recording::Off, |w| {
                w.update(&EntityKey::Id(id), moved(9.0), "lex:x").unwrap();
            }));
        });
        assert!(replay_matches(&f));
        (f.live.state_hash(), f.live.next_id(), f.live.revision(), f.sw.journal().base_hash())
    };
    assert_eq!(run(), run());
}

/// P2 (backstop): a project-tool command taking the project files (no handle can: `take_files`
/// isn't on it). The hidden write is found, the files go back as they were (the disk sync would
/// have taken them for deletions), the command is refused.
#[test]
fn p2_files_taken_inside_a_project_command_are_put_back() {
    let mut f = fixture();
    let (files, revision) = (f.live.files().clone(), f.live.files_revision());
    let mut out = None;
    assert_eq!(reported(|| out = Some(f.live.raw_commit_for_test(&mut f.sw, 1, MALLORY, Rule::Project, |w| drop(w.take_files())))), cfg!(debug_assertions));
    assert_ne!(out, Some(true), "refused");
    assert_eq!(f.live.files(), &files, "the files are as they were");
    assert!(f.live.files_revision() > revision, "and the files revision moved past the old one");
    assert!(replay_matches(&f));
}

/// P3 (backstop, the data-loss case): a project-tool command or a disk import replacing the
/// world (no handle can). Recovery keeps the project files they held (not the empty store of
/// the world put in its place) and moves the files revision past the old one, so the disk sync
/// deletes nothing.
#[test]
fn p3_a_world_replaced_in_a_project_command_or_disk_import_keeps_the_files() {
    for disk in [false, true] {
        let mut f = fixture();
        let (files, revision) = (f.live.files().clone(), f.live.files_revision());
        assert!(!files.is_empty(), "the fixture has a file");
        let mut out = None;
        let replace = |w: &mut World| *w = World::default();
        let was_reported = reported(|| out = Some(if disk { f.live.raw_disk_for_test(&mut f.sw, 1, replace) } else { f.live.raw_commit_for_test(&mut f.sw, 1, MALLORY, Rule::Project, replace) }));
        assert_eq!(was_reported, cfg!(debug_assertions), "disk={disk}");
        assert_ne!(out, Some(true), "disk={disk}: refused");
        assert_eq!(f.live.files(), &files, "disk={disk}: the files are kept");
        assert!(f.live.files_revision() > revision, "disk={disk}: the files revision moved past the old one");
        assert!(f.live.replacements().0 == 1 && replay_matches(&f), "disk={disk}");
    }
}

/// P4, P4b (backstop): the engine's settings moved inside a command (no handle can): the id
/// counter jumping on is noted so replay ends past it; the shape budgets and the clock are put
/// back; reported and refused.
#[test]
fn p4_engine_settings_moved_in_a_command_are_put_back() {
    let mut f = fixture();
    assert!(f.live.raw_commit_for_test(&mut f.sw, 1, MALLORY, Rule::World, |w| w.replay_next_id(500_000)), "an id jump is no write");
    assert!(replay_matches(&f), "P4: replay ends past the jump");
    let (limits, time) = (f.live.shape_limits(), f.live.world_time());
    let clean = f.live.raw_commit_for_test(&mut f.sw, 1, MALLORY, Rule::World, |w| {
        w.set_shape_limits(crate::two_d::ShapeLimits { persistent: usize::MAX, transient: usize::MAX });
        w.adopt_clock(-1000.0);
    });
    assert!(!clean, "P4b: refused");
    assert_eq!((f.live.shape_limits(), f.live.world_time()), (limits, time), "P4b: put back");
    assert!(replay_matches(&f));
}

/// The claim notices are bounded per holder (one holder's flood never pushes another's out),
/// and what was dropped or never examined is counted.
#[test]
fn claim_notices_are_bounded_per_holder() {
    let mut f = fixture();
    let abox = f.abox;
    // Mallory claims her group; alice holds abox.
    let s2 = session("bbbbbb22");
    f.sw.take_claim(&f.live, &s2, ClaimSpec { scope: Scope::path("/mg").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let tick_write = |f: &mut Fix, key: EntityKey, x: f32, tick: u64| {
        f.live
            .session(&mut f.sw, tick, Opener::LexLive, |wd, sw| {
                wd.update(&key, moved(x), "lex:siege").unwrap();
                journal_exempt(wd, sw, tick, Exempt::LexLive { agent: "lex:siege" });
            })
            .unwrap()
    };
    tick_write(&mut f, EntityKey::Path("/mg".into()), 1.0, 1);
    for t in 0..(crate::swarm::MAX_CLAIM_NOTICES as u64 + 10) {
        tick_write(&mut f, EntityKey::Id(abox), 2.0 + t as f32, 2 + t);
    }
    assert_eq!(f.sw.claim_notices(Some("alice"), 1000).as_array().unwrap().len(), crate::swarm::MAX_CLAIM_NOTICES);
    assert_eq!(f.sw.claim_notices_dropped("alice").0, 10, "alice's dropped notices are counted");
    assert_eq!(f.sw.claim_notices(Some("mallory"), 1000).as_array().unwrap().len(), 1, "mallory's notice survived alice's flood");
}

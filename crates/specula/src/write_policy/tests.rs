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

fn moved(x: f32) -> UpdateSpec {
    UpdateSpec { position: Some(Vec3::new(x, 0.5, 0.0)), ..UpdateSpec::default() }
}

/// The world every row starts from: alice's dynamic box `/abox` (her path claim) and `/rbox`
/// (inside her region claim); mallory's group `/mg` with three kinematic hooks, the first two
/// roped to the box (`rope`, `rope2`, made through physics_joint's rule). Before alice claimed,
/// mallory moved the box (`edit`), removed `rope2` (`unjoint`) and deleted `hook3` with its rope
/// (`unhook`): undoing those now is undoing onto alice's claim.
struct Fix {
    live: Live,
    sw: Swarm,
    abox: u64,
    rbox: u64,
    hook: u64,
    edit: u64,
    unjoint: u64,
    unhook: u64,
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
    sw.take_claim(&live, &s1, ClaimSpec { scope: Scope::path("/abox").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let region = Scope::region(Vec3::new(3.0, -1.0, 3.0), Vec3::new(7.0, 4.0, 7.0)).unwrap();
    sw.take_claim(&live, &s1, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    Fix { live, sw, abox, rbox, hook: hook1, edit, unjoint, unhook }
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

fn agent(f: &mut Fix, w: Writer, kind: Kind, rule: Rule, command: impl FnOnce(&mut World) -> Result<(), WorldError>) -> Result<(), WorldError> {
    f.live.commit(&mut f.sw, 1, w, kind, rule, command).map(|_| ())
}

fn set_abox(f: &mut Fix, rule: Rule) -> Result<(), WorldError> {
    let id = f.abox;
    agent(f, MALLORY, Kind::Set, rule, |wd| wd.update(&EntityKey::Id(id), moved(9.0), "mallory").map(|_| ()))
}

fn spawn_at(f: &mut Fix, w: Writer, path: &'static str) -> Result<(), WorldError> {
    agent(f, w, Kind::Spawn, Rule::World, |wd| wd.spawn(group(path), w.name).map(|_| ()))
}

fn undo(f: &mut Fix, seq: u64) -> Result<(), WorldError> {
    f.live.undo(&mut f.sw, 1, MALLORY, &UndoRequest { ids: Some(vec![seq]), count: 1, force: true }).map(|_| ())
}

/// A game tool (or a module call, or a live Lex command an agent made: they share
/// `game_commit`) doing `call` for mallory.
fn for_mallory(f: &mut Fix, game: &str, call: impl FnOnce(&mut World)) -> Result<(), WorldError> {
    f.live.game_tool(&mut f.sw, 1, game, MALLORY, call)
}

fn module_hook(f: &mut Fix, call: impl FnOnce(&mut World)) -> Result<(), WorldError> {
    // As the module host commits an agent's hook call (logic::tools::handle).
    f.live.session(&mut f.sw, 1, Opener::Modules, |wd, sw| {
        let mark = wd.mark();
        call(wd);
        let mut commit = module_agent_commit(sw, 1, MALLORY);
        commit(wd, mark, "module:nexus", Phase::Hook)
    })
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
}

fn exempt_move(f: &mut Fix, why: Exempt<'static>) -> Result<(), WorldError> {
    let id = f.abox;
    f.live.exempt(&mut f.sw, 1, why, |wd| wd.update(&EntityKey::Id(id), moved(3.0), why.by()).map(|_| ()))
}

const ROWS: &[(&str, Write, Expect)] = &[
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
    }, Expect::Exempt("module:nexus")),
    ("a live Lex run's own tick (exempt)", |f| {
        let id = f.abox;
        f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, sw| {
            wd.update(&EntityKey::Id(id), moved(5.0), "lex:siege").map(|_| ())?;
            journal_exempt(wd, sw, 1, Exempt::LexLive { agent: "lex:siege" });
            Ok(())
        })
    }, Expect::Exempt("lex:siege")),
    ("files read from disk (exempt: a human's edit)", |f| f.live.disk(&mut f.sw, 1, |wd| {
        wd.put_file_raw("assets/recipes/t.json", Some(crate::project::ProjectFile::new(crate::project::FileKind::Recipe, Arc::from("{}"), 1, Arc::from("disk"))), true);
        Ok(())
    }).map(|_| ()), Expect::Exempt("disk")),
    // Replacing the world (world_load, save_read, startup).
    ("world_load while another agent holds claims (M1)", |f| may_replace(&f.sw, MALLORY, false).map(|_| ()), Expect::Refused("claimed")),
    ("world_load under a playtest (M3)", |f| may_replace(&f.sw, ALICE, true).map(|_| ()), Expect::Refused("playtest_running")),
];

/// Every row from the same fixture: refused rows leave the world and journal exactly as they
/// were (nothing pending), allowed and exempt ones journal under the right name, and the journal
/// replays to the live world afterwards.
#[test]
fn every_write_path_holds_the_policy() {
    for (name, write, expect) in ROWS {
        let mut f = fixture();
        let (before, last, next_id) = (contents(&f.live), f.sw.journal.last_seq(), f.live.next_id());
        let out = write(&mut f);
        assert!(!f.live.has_unjournaled(), "{name}: nothing left pending");
        match expect {
            Expect::Refused(code) => {
                let e = out.expect_err(name);
                assert_eq!(e.code, *code, "{name}: {}", e.message);
                assert_eq!(contents(&f.live), before, "{name}: the world is as before (only the id counter moved on)");
                // (A module call whose request was refused still journals the ids it used.)
                assert!(f.sw.journal.entries().filter(|e| e.seq > last).all(|e| e.changes.is_empty()), "{name}: no change journaled");
            }
            Expect::Exempt(by) => {
                out.unwrap_or_else(|e| panic!("{name}: {e}"));
                let entry = f.sw.journal.entries().last().unwrap();
                assert!(entry.seq > last, "{name}: journaled");
                assert_eq!((&*entry.by, entry.via.as_deref()), (*by, None), "{name}: under its own name");
            }
            Expect::Allowed => {
                out.unwrap_or_else(|e| panic!("{name}: {e}"));
                let entry = f.sw.journal.entries().last().unwrap();
                assert!(entry.seq > last && entry.is_by(MALLORY), "{name}: journaled as the agent's");
            }
        }
        assert!(f.live.next_id() >= next_id, "{name}: ids are never handed out again");
        let replayed = f.sw.journal.replay().expect("replayable");
        assert_eq!(replayed.state_hash(), f.live.state_hash(), "{name}: the journal replays to the live world");
    }
}

/// The table covers every writer the policy knows (a new variant needs a row, or this fails to
/// compile until it has one).
#[test]
fn the_table_names_every_writer() {
    let names: Vec<&str> = ROWS.iter().map(|(n, _, _)| *n).collect();
    let has = |s: &str| names.iter().any(|n| n.contains(s));
    for rule in [Rule::World, Rule::Joint, Rule::Nav, Rule::Lex, Rule::LexLive, Rule::Project] {
        let row = match rule {
            Rule::World => "world_set",
            Rule::Joint => "physics_joint's rule",
            Rule::Nav => "the nav tools' rule",
            Rule::Lex => "the Lex tools' rule",
            Rule::LexLive => "lex_live's rule",
            Rule::Project => "the project tools' rule",
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
/// ends: a debug assertion, and in release the changes are journaled as `engine:stray` (replay
/// stays exact) and counted.
#[test]
fn a_writer_that_leaves_changes_unjournaled_is_caught() {
    let mut f = fixture();
    let stray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.live.session(&mut f.sw, 1, Opener::LexLive, |wd, _| {
            wd.spawn(cube("forgotten", Vec3::new(70.0, 0.5, 70.0)), "lex:siege").unwrap();
        });
    }));
    if cfg!(debug_assertions) {
        assert!(stray.is_err(), "a debug build stops there");
    } else {
        assert!(stray.is_ok());
    }
    assert!(!f.live.has_unjournaled());
    assert_eq!(f.live.strays(), 1);
    let entry = f.sw.journal.entries().last().unwrap();
    assert_eq!(&*entry.by, STRAY_AGENT);
    assert_eq!(f.sw.journal.replay().unwrap().state_hash(), f.live.state_hash());
}

/// A hosted game (external code holding the live world in a game tool or its own calls) that
/// takes the world's recorded changes itself, or stops recording, would hide its writes from the
/// journal and the claims: the policy sees the world's seal move and reports it (a debug
/// assertion; counted in release).
#[test]
fn a_game_that_hides_its_writes_is_caught() {
    for how in ["take", "off"] {
        let mut f = fixture();
        let id = f.abox;
        let hid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = f.live.game_tool(&mut f.sw, 1, "sepulcra", MALLORY, |wd| {
                if how == "off" {
                    wd.set_recording(Recording::Off);
                }
                wd.update(&EntityKey::Id(id), moved(8.0), "sepulcra").unwrap();
                if how == "take" {
                    wd.take_changes();
                }
            });
        }));
        assert_eq!(hid.is_err(), cfg!(debug_assertions), "{how}");
        assert_eq!(f.live.strays(), 1, "{how}");
        assert_eq!(f.live.recording(), Recording::Full, "{how}: recording again");
    }
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
        let entry = f.sw.journal.entries().last().unwrap();
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

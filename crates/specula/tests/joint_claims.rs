//! Joints and claims (review of the ragdoll lane, findings 1 and 2): a joint moves the bodies
//! it links, so joint data (`props.joint`) is written only by `physics_joint`, which needs the
//! right to change both bodies, path and region claims alike. Each test is a reviewer probe
//! (scratchpad review-ragdoll-4a48609/live/claims.py, claims2.py) that used to get through.

use specula::claims::{body_change, ClaimSpec, Scope, DEFAULT_LEASE};
use specula::journal::{Kind, Writer};
use specula::swarm::Swarm;
use specula::world::{EntityKey, Op, Recording, Shape, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::SessionInfo;
use std::time::Instant;

const ALICE: Writer = Writer { name: "alice", session: Some("aaaaaa11"), via: None };
const MALLORY: Writer = Writer { name: "mallory", session: Some("bbbbbb22"), via: None };

fn session(id: &str) -> SessionInfo {
    SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

struct Setup {
    sw: Swarm,
    w: World,
    abox: u64,
    rbox: u64,
}

fn spawn(sw: &mut Swarm, w: &mut World, who: Writer, s: SpawnSpec) -> Result<u64, String> {
    let by = who.name.to_string();
    sw.commit(w, 0, who, Kind::Spawn, |w| w.spawn(s, &by).map(|e| e.id)).map(|(id, _)| id).map_err(|e| e.code.to_string())
}

fn cube(name: &str, pos: Vec3) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Cube);
    s.name = Some(name.into());
    s.position = pos;
    s
}

fn joint_props(b: u64) -> Json {
    Json::parse(&format!(r#"{{"joint": {{"kind": "distance", "a": null, "b": {b}, "anchor_a": [0, 8, 0], "anchor_b": [0, 0, 0], "length": [0, 0.5]}}}}"#)).unwrap()
}

/// Alice's two boxes: /abox under her path claim, /rbox inside her region claim.
fn setup() -> Setup {
    let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
    let live = vec![s1.clone(), s2.clone()];
    let mut sw = Swarm::default();
    sw.hello(&s1, "alice".into(), "builder".into(), None, &live).unwrap();
    sw.hello(&s2, "mallory".into(), "builder".into(), None, &live).unwrap();
    let mut w = World::default();
    w.set_recording(Recording::Full);
    let abox = spawn(&mut sw, &mut w, ALICE, cube("abox", Vec3::new(0.0, 0.5, 0.0))).unwrap();
    let rbox = spawn(&mut sw, &mut w, ALICE, cube("rbox", Vec3::new(5.0, 0.5, 5.0))).unwrap();
    sw.take_claim(&w, &s1, ClaimSpec { scope: Scope::path("/abox").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let region = Scope::region(Vec3::new(3.0, -1.0, 3.0), Vec3::new(7.0, 4.0, 7.0)).unwrap();
    sw.take_claim(&w, &s1, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    Setup { sw, w, abox, rbox }
}

/// Finding 1 (blocker): a props.joint written by world tools was simulated unchecked; now joint
/// data is reserved for physics_joint, whoever writes it.
#[test]
fn world_tools_cant_write_joints() {
    let mut s = setup();
    let mj = |b: u64| {
        let mut j = SpawnSpec::new(Shape::Group);
        j.name = Some("mj".into());
        j.position = Vec3::new(20.0, 0.0, 20.0);
        j.props = joint_props(b);
        j
    };
    assert_eq!(spawn(&mut s.sw, &mut s.w, MALLORY, mj(s.abox)), Err("reserved".into()), "world_spawn of a joint on a claimed body");
    // Even on one's own bodies, and even by the claim holder: joints go through physics_joint.
    assert_eq!(spawn(&mut s.sw, &mut s.w, ALICE, mj(s.abox)), Err("reserved".into()));
    // Nor by world_set onto an existing entity, nor in a batch.
    let mut g = SpawnSpec::new(Shape::Group);
    g.name = Some("mg".into());
    g.position = Vec3::new(20.0, 0.0, 20.0);
    let mg = spawn(&mut s.sw, &mut s.w, MALLORY, g).unwrap();
    let set = UpdateSpec { props: Some(joint_props(s.abox)), ..Default::default() };
    let r = s.sw.commit(&mut s.w, 0, MALLORY, Kind::Set, |w| w.update(&EntityKey::Id(mg), set, "mallory").map(|e| e.id));
    assert_eq!(r.err().map(|e| e.code), Some("reserved"));
    let r = s.sw.commit(&mut s.w, 0, MALLORY, Kind::Batch, |w| w.apply_batch(vec![Op::Spawn(mj(s.rbox))], "mallory").map_err(|(_, e)| e));
    assert_eq!(r.err().map(|e| e.code), Some("reserved"));
    assert!(s.w.entities().all(|e| e.props.get("joint").is_none()), "no joint got in");
    // physics_joint's own commits may.
    let mut ok = SpawnSpec::new(Shape::Group);
    ok.name = Some("hinge".into());
    ok.props = joint_props(s.abox);
    let by = "alice".to_string();
    assert!(s.sw.commit_joint(&mut s.w, 0, ALICE, Kind::Spawn, |w| w.spawn(ok, &by).map(|e| e.id)).is_ok());
}

/// Removing a joint with world tools lets go of its bodies: allowed only with the right to
/// change both.
#[test]
fn deleting_a_joint_needs_the_right_to_its_bodies() {
    let mut s = setup();
    let mut j = SpawnSpec::new(Shape::Group);
    j.name = Some("hold".into());
    j.props = joint_props(s.abox);
    let by = "mallory".to_string();
    // (Made through physics_joint's path; the tool itself refuses Mallory, see below.)
    let jid = s.sw.commit_joint(&mut s.w, 0, MALLORY, Kind::Spawn, |w| w.spawn(j, &by).map(|e| e.id)).unwrap().0;
    let del = |s: &mut Setup, who: Writer| s.sw.commit(&mut s.w, 0, who, Kind::Delete, |w| w.delete(&EntityKey::Id(jid), None).map(|_| ())).err().map(|e| e.code);
    assert_eq!(del(&mut s, MALLORY), Some("claimed"), "Mallory may not let Alice's box go");
    assert_eq!(del(&mut s, ALICE), None, "Alice may");
}

/// Finding 2: physics_joint checked bodies with a pose change, which region claims never see.
/// The check it makes now (a change in place at the body) sees both kinds of claim.
#[test]
fn joint_body_checks_see_path_and_region_claims() {
    let s = setup();
    let now = Instant::now();
    let check = |who: Writer, id: u64| s.sw.claims.check(&s.w, &[body_change(&s.w, id).unwrap()], who, now).err().map(|e| e.code);
    assert_eq!(check(MALLORY, s.abox), Some("claimed"), "path claim");
    assert_eq!(check(MALLORY, s.rbox), Some("claimed"), "region claim");
    assert_eq!(check(ALICE, s.abox), None);
    assert_eq!(check(ALICE, s.rbox), None);
}

/// Review round 2, finding 3 (probe undo.py): undoing a joint's removal ties the joint's bodies
/// again, so it needs the right to change them like physics_joint does: mallory can't undo her
/// way onto alice's claimed box.
#[test]
fn undo_cant_tie_a_joint_onto_claimed_bodies() {
    use specula::journal::UndoRequest;
    let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
    let live = vec![s1.clone(), s2.clone()];
    let mut sw = Swarm::default();
    sw.hello(&s1, "alice".into(), "builder".into(), None, &live).unwrap();
    sw.hello(&s2, "mallory".into(), "builder".into(), None, &live).unwrap();
    let mut w = World::default();
    w.set_recording(Recording::Full);
    let abox = spawn(&mut sw, &mut w, ALICE, cube("abox", Vec3::new(0.0, 0.5, 0.0))).unwrap();
    let mut j = SpawnSpec::new(Shape::Group);
    j.name = Some("hold".into());
    j.props = joint_props(abox);
    let by = "mallory".to_string();
    let jid = sw.commit_joint(&mut w, 0, MALLORY, Kind::Spawn, |w| w.spawn(j, &by).map(|e| e.id)).unwrap().0;
    let (_, removed) = sw.commit_joint(&mut w, 0, MALLORY, Kind::Delete, |w| w.delete(&EntityKey::Id(jid), None).map(|_| ())).unwrap();
    let removed = removed.expect("journaled");
    sw.take_claim(&w, &s1, ClaimSpec { scope: Scope::path("/abox").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let r = sw.undo(&mut w, 0, MALLORY, &UndoRequest { ids: Some(vec![removed]), count: 1, force: false });
    assert_eq!(r.err().map(|e| e.code), Some("claimed"));
    assert!(w.entities().all(|e| e.props.get("joint").is_none()), "the joint stayed removed");
    // Once alice lets the box go, the undo goes through.
    sw.release(ALICE, None).unwrap();
    let r = sw.undo(&mut w, 0, MALLORY, &UndoRequest { ids: Some(vec![removed]), count: 1, force: false });
    assert!(r.is_ok(), "{:?}", r.err());
    assert!(w.entities().any(|e| e.props.get("joint").is_some()));
}

/// Mallory's kinematic hook in her own group, roped (a distance joint) to alice's box before
/// alice claimed it: returns (swarm, world, hook, box, the hook's group).
fn roped() -> (Swarm, World, u64, u64, u64) {
    use specula::world::{Body, BodyKind};
    let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
    let live = vec![s1.clone(), s2.clone()];
    let mut sw = Swarm::default();
    sw.hello(&s1, "alice".into(), "builder".into(), None, &live).unwrap();
    sw.hello(&s2, "mallory".into(), "builder".into(), None, &live).unwrap();
    let mut w = World::default();
    w.set_recording(Recording::Full);
    let mut abox = cube("abox", Vec3::new(0.0, 6.0, 0.0));
    abox.body = Some(Body::new(BodyKind::Dynamic, 5.0));
    let abox = spawn(&mut sw, &mut w, ALICE, abox).unwrap();
    let mut mg = SpawnSpec::new(Shape::Group);
    mg.name = Some("mg".into());
    mg.position = Vec3::new(20.0, 0.0, 20.0);
    let mg = spawn(&mut sw, &mut w, MALLORY, mg).unwrap();
    let mut hook = cube("hook", Vec3::new(0.0, 6.5, -20.0));
    hook.parent = Some(EntityKey::Id(mg));
    hook.body = Some(Body::new(BodyKind::Kinematic, 1.0));
    let hook = spawn(&mut sw, &mut w, MALLORY, hook).unwrap();
    let mut j = SpawnSpec::new(Shape::Group);
    j.name = Some("rope".into());
    j.parent = Some(EntityKey::Id(mg));
    j.props = Json::parse(&format!(r#"{{"joint": {{"kind": "distance", "a": {hook}, "b": {abox}, "length": [0, 0.6]}}}}"#)).unwrap();
    let by = "mallory".to_string();
    sw.commit_joint(&mut w, 0, MALLORY, Kind::Spawn, |w| w.spawn(j, &by).map(|e| e.id)).unwrap();
    (sw, w, hook, abox, mg)
}

fn claim_abox(sw: &mut Swarm, w: &World) {
    sw.take_claim(w, &session("aaaaaa11"), ClaimSpec { scope: Scope::path("/abox").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
}

/// Review round 3, finding 4 (probe revive.py): deleting a body a joint links lets go of the
/// joint's other bodies, and restoring it pulls them again: both need the right to change them.
#[test]
fn deleting_or_restoring_a_jointed_body_needs_the_other_bodies() {
    use specula::journal::UndoRequest;
    // Deleting her hook (or the group holding it) while alice holds the box: refused.
    let (mut sw, mut w, hook, _, mg) = roped();
    claim_abox(&mut sw, &w);
    let del = |sw: &mut Swarm, w: &mut World, id: u64| sw.commit(w, 0, MALLORY, Kind::Delete, |w| w.delete(&EntityKey::Id(id), None).map(|_| ())).map(|r| r.1).map_err(|e| e.code);
    assert_eq!(del(&mut sw, &mut w, hook), Err("claimed"));
    assert_eq!(del(&mut sw, &mut w, mg), Err("claimed"), "nor through its group");
    assert!(w.get(hook).is_some());
    // Deleted before alice's claim, then restored by an undo after it: refused too.
    let (mut sw, mut w, hook, _, _) = roped();
    let removed = del(&mut sw, &mut w, hook).unwrap().expect("journaled");
    claim_abox(&mut sw, &w);
    let r = sw.undo(&mut w, 0, MALLORY, &UndoRequest { ids: Some(vec![removed]), count: 1, force: false });
    assert_eq!(r.err().map(|e| e.code), Some("claimed"));
    assert!(w.get(hook).is_none(), "the hook stayed deleted");
    // Without a claim on the box both go through.
    let (mut sw, mut w, hook, _, _) = roped();
    let removed = del(&mut sw, &mut w, hook).unwrap().unwrap();
    assert!(sw.undo(&mut w, 0, MALLORY, &UndoRequest { ids: Some(vec![removed]), count: 1, force: false }).is_ok());
    assert!(w.get(hook).is_some());
}

/// Review round 3, probe drag.py, decided: moving your own hook is allowed even though the rope
/// drags alice's claimed box after it, as pushing it through contacts is (claims are about
/// writes; PHYSICS.md "Joints and claims"). So are rescaling or reparenting it or an ancestor
/// (round 4, probe ancestor.py: those are moves too, and the rule is the same at every level).
/// Changing what the hook is (giving it another kind of body, or none) is a joint change and
/// needs the box.
#[test]
fn moving_a_jointed_body_is_allowed_changing_what_it_is_is_not() {
    use specula::world::{Body, BodyKind};
    let (mut sw, mut w, hook, _, mg) = roped();
    claim_abox(&mut sw, &w);
    let set = |sw: &mut Swarm, w: &mut World, id: u64, u: UpdateSpec| sw.commit(w, 0, MALLORY, Kind::Set, |w| w.update(&EntityKey::Id(id), u, "mallory").map(|_| ())).map(|_| ()).map_err(|e| e.code);
    assert_eq!(set(&mut sw, &mut w, hook, UpdateSpec { position: Some(Vec3::new(0.0, 6.5, -61.0)), ..UpdateSpec::default() }), Ok(()), "a move");
    assert_eq!(set(&mut sw, &mut w, hook, UpdateSpec { scale: Some(Vec3::new(2.0, 2.0, 2.0)), ..UpdateSpec::default() }), Ok(()), "a rescale");
    assert_eq!(set(&mut sw, &mut w, mg, UpdateSpec { scale: Some(Vec3::new(3.0, 3.0, 3.0)), ..UpdateSpec::default() }), Ok(()), "an ancestor's rescale");
    assert_eq!(set(&mut sw, &mut w, hook, UpdateSpec { body: Some(Some(Body::new(BodyKind::Dynamic, 1.0))), ..UpdateSpec::default() }), Err("claimed"), "a new kind of body");
    assert_eq!(set(&mut sw, &mut w, hook, UpdateSpec { body: Some(None), ..UpdateSpec::default() }), Err("claimed"), "no body: the joint lets go");
}

/// Review round 4, finding 2 (probe jointcost.py), as a work count: the joint rules look only at
/// the joints a change touches (the world keeps an index by body), never at every entity, so a
/// 1,000-spawn batch under someone's claim looks at no joint at all, and deleting a jointed body
/// at exactly its joints. They used to scan the world and parse every joint per change (1.5 s
/// for the batch, a bystander blocked 2.2 s).
#[test]
fn the_joint_rules_do_work_by_the_joints_touched_not_the_world() {
    use specula::claims::joint_rule_work;
    use specula::world::{Body, BodyKind};
    let (mut sw, mut w, hook, _, _) = roped();
    for i in 0..2000 {
        let mut c = cube(&format!("b{i}"), Vec3::new((i % 50) as f32 * 2.0, 1.0, 100.0 + (i / 50) as f32 * 2.0));
        c.body = Some(Body::new(BodyKind::Dynamic, 1.0));
        spawn(&mut sw, &mut w, MALLORY, c).unwrap();
    }
    claim_abox(&mut sw, &w);
    assert_eq!(w.joints_on(hook).count(), 1);
    let before = joint_rule_work();
    let ops: Vec<Op> = (0..1000)
        .map(|i| {
            let mut c = cube(&format!("n{i}"), Vec3::new(i as f32, 1.0, 300.0));
            c.body = Some(Body::new(BodyKind::Dynamic, 1.0));
            Op::Spawn(c)
        })
        .collect();
    let by = "mallory".to_string();
    sw.commit(&mut w, 0, MALLORY, Kind::Batch, |w| w.apply_batch(ops, &by).map(|_| ()).map_err(|(_, e)| e)).unwrap();
    assert_eq!(joint_rule_work() - before, 0, "no joint names a new body");
    let before = joint_rule_work();
    let r = sw.commit(&mut w, 0, MALLORY, Kind::Delete, |w| w.delete(&EntityKey::Id(hook), None).map(|_| ()));
    assert_eq!(r.err().map(|e| e.code), Some("claimed"));
    assert_eq!(joint_rule_work() - before, 1, "the hook's one joint");
}

/// Review round 3, finding 3 (probe modjoint.py): a module or game writes through
/// check_game_changes, which now applies the joint rules: a joint spawned onto alice's claimed
/// box is refused, and the world rolled back.
#[test]
fn modules_and_games_follow_the_joint_rules() {
    let (mut sw, mut w, _, abox, _) = roped();
    claim_abox(&mut sw, &w);
    let r = sw.game_tool(&mut w, 1, "nexus", MALLORY, |w| {
        let mut j = SpawnSpec::new(Shape::Group);
        j.name = Some("haul".into());
        j.position = Vec3::new(30.0, 0.0, 30.0);
        j.props = joint_props(abox);
        w.spawn(j, "nexus").map(|e| e.id)
    });
    assert_eq!(r.err().map(|e| e.code), Some("claimed"));
    assert!(w.find_path("/haul").is_err(), "rolled back");
    // On bodies nobody claimed, a module may make joints.
    let mut free = cube("free", Vec3::new(40.0, 1.0, 40.0));
    free.body = Some(specula::world::Body::new(specula::world::BodyKind::Dynamic, 1.0));
    let free = spawn(&mut sw, &mut w, MALLORY, free).unwrap();
    let ok = sw.game_tool(&mut w, 2, "nexus", MALLORY, |w| {
        let mut j = SpawnSpec::new(Shape::Group);
        j.name = Some("haul2".into());
        j.props = joint_props(free);
        w.spawn(j, "nexus").map(|e| e.id)
    });
    assert!(ok.is_ok(), "{:?}", ok.err());
}

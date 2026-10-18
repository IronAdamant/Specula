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

const ALICE: Writer = Writer { name: "alice", session: Some("aaaaaa11") };
const MALLORY: Writer = Writer { name: "mallory", session: Some("bbbbbb22") };

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

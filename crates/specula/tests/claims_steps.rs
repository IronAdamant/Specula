//! Claims are checked at every step of a command (review round 2): a later op must not hide what
//! an earlier one did. Each test is a reviewer probe (P1–P5, P8) that used to get through.

use specula::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
use specula::journal::{Kind, UndoRequest, Writer};
use specula::swarm::Swarm;
use specula::world::{EntityKey, Op, Recording, Shape, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_protocol::SessionInfo;

const ANN: Writer = Writer { name: "ann", session: Some("aaaaaa11") };
const BOB: Writer = Writer { name: "bob", session: Some("bbbbbb22") };

fn session(id: &str) -> SessionInfo {
    SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

fn spec(path: &str, pos: Vec3) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Cube);
    let (parent, name) = path.rsplit_once('/').unwrap();
    s.name = Some(name.into());
    s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
    s.position = pos;
    s
}

fn pclaim(p: &str) -> ClaimSpec {
    ClaimSpec { scope: Scope::path(p).unwrap(), lease: DEFAULT_LEASE, note: String::new() }
}

fn region(min: Vec3, max: Vec3) -> ClaimSpec {
    ClaimSpec { scope: Scope::region(min, max).unwrap(), lease: DEFAULT_LEASE, note: String::new() }
}

fn key(p: &str) -> EntityKey {
    EntityKey::Path(p.into())
}

fn rename(n: &str) -> UpdateSpec {
    UpdateSpec { name: Some(n.into()), ..Default::default() }
}

fn moved(x: f32) -> UpdateSpec {
    UpdateSpec { position: Some(Vec3::new(x, 0.0, 0.0)), ..Default::default() }
}

struct Setup {
    sw: Swarm,
    w: World,
    s1: SessionInfo,
}

fn setup() -> Setup {
    let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
    let live = vec![s1.clone(), s2.clone()];
    let mut sw = Swarm::default();
    sw.hello(&s1, "ann".into(), "builder".into(), &live).unwrap();
    sw.hello(&s2, "bob".into(), "builder".into(), &live).unwrap();
    let mut w = World::default();
    w.set_recording(Recording::Full);
    Setup { sw, w, s1 }
}

impl Setup {
    fn spawn(&mut self, who: Writer, path: &str, pos: Vec3) -> u64 {
        let by = who.name.to_string();
        self.sw.commit(&mut self.w, 0, who, Kind::Spawn, |w| w.spawn(spec(path, pos), &by).map(|e| e.id)).unwrap().0
    }

    fn batch(&mut self, who: Writer, ops: Vec<Op>) -> Result<(), String> {
        let by = who.name.to_string();
        // Everything but the id counter (ids a rejected command used are never reused).
        let snapshot = |w: &World| w.entities().map(|e| (e.id, e.version, w.path_of(e.id))).collect::<Vec<_>>();
        let before = snapshot(&self.w);
        let r = self.sw.commit(&mut self.w, 0, who, Kind::Batch, |w| w.apply_batch(ops, &by).map_err(|(_, e)| e));
        if r.is_err() {
            assert_eq!(snapshot(&self.w), before, "a rejected batch changes nothing");
            assert!(self.w.pending_changes().is_empty());
        }
        r.map(|_| ()).map_err(|e| e.code.to_string())
    }

    fn claim(&mut self, c: ClaimSpec) {
        let s1 = self.s1.clone();
        self.sw.take_claim(&self.w, &s1, c).unwrap();
    }

    fn replays(&self) {
        assert_eq!(self.sw.journal.replay().unwrap().state_hash(), self.w.state_hash(), "the journal still replays");
    }
}

/// P1: a rename then a delete in one batch deleted the claimed subtree.
#[test]
fn p1_rename_then_delete_in_a_batch() {
    let mut s = setup();
    s.spawn(BOB, "/a", Vec3::ZERO);
    s.spawn(BOB, "/a/b", Vec3::ZERO);
    s.claim(pclaim("/a/b"));
    s.spawn(ANN, "/a/b/wall", Vec3::ZERO);
    assert_eq!(s.batch(BOB, vec![Op::Delete(key("/a"), None)]), Err("claimed".into()));
    assert_eq!(s.batch(BOB, vec![Op::Set(key("/a"), rename("z"))]), Err("claimed".into()));
    let r = s.batch(BOB, vec![Op::Set(key("/a"), rename("z")), Op::Delete(key("/z"), None)]);
    assert_eq!(r, Err("claimed".into()));
    assert!(s.w.find_path("/a/b/wall").is_ok());
    s.replays();
}

/// P2: a rename then moving the claimed child out under a new name stole the subtree.
#[test]
fn p2_rename_then_reparent_the_child_out() {
    let mut s = setup();
    s.spawn(BOB, "/a", Vec3::ZERO);
    s.spawn(BOB, "/a/b", Vec3::ZERO);
    s.claim(pclaim("/a/b"));
    s.spawn(ANN, "/a/b/wall", Vec3::ZERO);
    let steal = UpdateSpec { parent: Some(None), name: Some("stolen".into()), ..Default::default() };
    let r = s.batch(BOB, vec![Op::Set(key("/a"), rename("z")), Op::Set(key("/z/b"), steal)]);
    assert_eq!(r, Err("claimed".into()));
    assert!(s.w.find_path("/a/b/wall").is_ok() && s.w.find_path("/stolen").is_err());
    s.replays();
}

/// P3: undoing two entries (a rename and a spawn) deleted the claimed entity.
#[test]
fn p3_multi_entry_undo() {
    let mut s = setup();
    s.spawn(BOB, "/a", Vec3::ZERO);
    s.spawn(BOB, "/a/b", Vec3::ZERO);
    let by = "bob".to_string();
    s.sw.commit(&mut s.w, 0, BOB, Kind::Set, |w| w.update(&key("/a"), rename("z"), &by).map(|_| ())).unwrap();
    s.claim(pclaim("/z/b"));
    let hash = s.w.state_hash();
    for count in [1, 2, 3] {
        let r = s.sw.undo(&mut s.w, 0, BOB, &UndoRequest { ids: None, count, force: false });
        assert_eq!(r.unwrap_err().code, "claimed", "undo count={count}");
        assert_eq!(s.w.state_hash(), hash);
    }
    let forced = s.sw.undo(&mut s.w, 0, BOB, &UndoRequest { ids: None, count: 2, force: true });
    assert_eq!(forced.unwrap_err().code, "claimed", "force never overrides claims");
    assert!(s.w.find_path("/z/b").is_ok());
    s.replays();
}

/// P4: two moves of a parent in one batch hid where its child started (inside the region).
#[test]
fn p4_two_moves_hide_the_start() {
    let mut s = setup();
    s.spawn(BOB, "/g", Vec3::new(-10.0, 0.0, 0.0));
    s.spawn(BOB, "/g/kid", Vec3::new(11.0, 0.0, 0.0));
    s.claim(region(Vec3::new(0.0, -1.0, -1.0), Vec3::new(5.0, 1.0, 1.0)));
    let mv = |x: f32| Op::Set(key("/g"), moved(x));
    assert_eq!(s.batch(BOB, vec![mv(-100.0)]), Err("claimed".into()), "one move is caught");
    assert_eq!(s.batch(BOB, vec![mv(-100.0), mv(-200.0)]), Err("claimed".into()));
    s.replays();
}

/// P5: a move then reparenting the child hid that the child left the region.
#[test]
fn p5_move_then_reparent() {
    let mut s = setup();
    s.spawn(BOB, "/g", Vec3::new(-10.0, 0.0, 0.0));
    s.spawn(BOB, "/g/kid", Vec3::new(11.0, 0.0, 0.0));
    s.claim(region(Vec3::new(0.0, -1.0, -1.0), Vec3::new(5.0, 1.0, 1.0)));
    let ops = vec![Op::Set(key("/g"), moved(-100.0)), Op::Set(key("/g/kid"), UpdateSpec { parent: Some(None), ..Default::default() })];
    assert_eq!(s.batch(BOB, ops), Err("claimed".into()));
    s.replays();
}

/// Steps that pass each check on their own still pass together, and the batch lands whole.
#[test]
fn harmless_multi_step_batches_still_apply() {
    let mut s = setup();
    s.spawn(BOB, "/a", Vec3::ZERO);
    s.spawn(BOB, "/a/b", Vec3::ZERO);
    s.claim(pclaim("/elsewhere"));
    let ops = vec![
        Op::Set(key("/a"), rename("z")),
        Op::Spawn(spec("/z/c", Vec3::ZERO)),
        Op::Set(key("/z/b"), moved(3.0)),
        Op::Delete(key("/z/c"), None),
        Op::Set(key("/z"), moved(-4.0)),
    ];
    assert_eq!(s.batch(BOB, ops), Ok(()));
    assert!(s.w.find_path("/z/b").is_ok());
    s.replays();
}

/// A rejected command still uses up the ids it handed out: they are never reused, and the
/// journal's replay ends at the same id counter.
#[test]
fn rejected_commands_never_reuse_ids() {
    let mut s = setup();
    s.claim(pclaim("/q"));
    let next = s.w.next_id();
    let ops = vec![Op::Spawn(spec("/fine", Vec3::ZERO)), Op::Spawn(spec("/q", Vec3::ZERO))];
    assert_eq!(s.batch(BOB, ops), Err("claimed".into()));
    assert!(s.w.next_id() > next, "the rejected spawns' ids stay used");
    let id = s.spawn(BOB, "/later", Vec3::ZERO);
    assert!(id > next + 1);
    s.replays();
}

/// P8: snapshot recording through spawn/delete/rename swaps replays exactly.
#[test]
fn p8_snapshot_edge_cases_replay() {
    let mut s = setup();
    s.spawn(BOB, "/p", Vec3::ZERO);
    s.spawn(BOB, "/q", Vec3::ZERO);
    s.spawn(BOB, "/r", Vec3::ZERO);
    let w = &mut s.w;
    w.set_recording(Recording::Snapshot);
    w.update(&key("/q"), moved(1.0), "game").unwrap();
    w.update(&key("/q"), rename("q2"), "game").unwrap();
    w.update(&key("/r"), rename("r2"), "game").unwrap();
    w.update(&key("/r2"), moved(4.0), "game").unwrap();
    w.delete(&key("/p"), None).unwrap();
    w.spawn(spec("/p", Vec3::new(9.0, 0.0, 0.0)), "game").unwrap();
    w.spawn(spec("/p/c", Vec3::ZERO), "game").unwrap();
    w.update(&key("/q2"), rename("tmp"), "game").unwrap();
    w.update(&key("/r2"), rename("q2"), "game").unwrap();
    w.update(&key("/tmp"), rename("r2"), "game").unwrap();
    w.set_recording(Recording::Full);
    s.sw.game_changes(&mut s.w, 1, "sepulcra", Kind::GameTick);
    s.w.set_recording(Recording::Snapshot);
    s.w.update(&key("/p"), moved(2.0), "game").unwrap();
    s.w.delete(&key("/p"), None).unwrap();
    s.w.spawn(spec("/p", Vec3::new(3.0, 0.0, 0.0)), "game").unwrap();
    s.w.set_recording(Recording::Full);
    s.sw.game_changes(&mut s.w, 2, "sepulcra", Kind::GameTick);
    let replayed = s.sw.journal.replay().unwrap();
    assert_eq!(replayed.state_hash(), s.w.state_hash());
    assert_eq!(replayed.find_path("/p").ok(), s.w.find_path("/p").ok());
    assert_eq!(replayed.find_path("/q2").ok(), s.w.find_path("/q2").ok());
}

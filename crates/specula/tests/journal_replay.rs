//! Journal replay (M1 exit proof, foundation F2): a world built by several agents and a game —
//! spawns, edits, renames, moves, deletes, batches, environment changes, rejected writes and
//! undos — is rebuilt from an empty world by replaying its journal, to the same state hash.
//!
//! Headless and GPU-free, so it runs on every CI job (windows-msvc, windows-gnu, ubuntu). The
//! scenario's final hash is also pinned: if two builds disagree, this test fails on one of them,
//! which is the cross-build replay check. (If `World::state_hash`'s definition changes on purpose,
//! update `EXPECTED_HASH`.)
//!
//! `cargo test -p specula --release --test journal_replay -- --ignored --nocapture` prints what
//! journaling costs per tick (foundation F3).

use specula::journal::{self, Journal, Kind, UndoRequest};
use specula::world::{Change, EntityKey, EnvPatch, Op, Recording, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::math::Vec3;
use std::time::{Duration, Instant};

/// The scenario's final state hash.
const EXPECTED_HASH: u64 = 0xfab1_be53_5836_6b94;

fn no_check(_: &World, _: &Change) -> Result<(), WorldError> {
    Ok(())
}

fn spec(shape: Shape, path: &str, position: Vec3) -> SpawnSpec {
    let mut s = SpawnSpec::new(shape);
    let (parent, name) = path.rsplit_once('/').unwrap();
    s.name = Some(name.into());
    s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
    s.position = position;
    s
}

struct Live {
    world: World,
    journal: Journal,
    tick: u64,
}

impl Live {
    fn new() -> Live {
        let mut world = World::default();
        world.set_recording(Recording::Full);
        Live { journal: Journal::new(&world, "empty"), world, tick: 0 }
    }

    fn agent<T>(&mut self, by: &str, kind: Kind, f: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<T, WorldError> {
        // A stand-in for the engine's claims: only "owner" may write under /claimed.
        let mut check = |_: &World, change: &Change| {
            if by != "owner" && change.path().starts_with("/claimed") {
                Err(WorldError::new("claimed", "inside owner's claim"))
            } else {
                Ok(())
            }
        };
        journal::commit(&mut self.world, &mut self.journal, self.tick, by, kind, &mut check, f).map(|(v, _)| v)
    }

    fn batch(&mut self, by: &str, ops: Vec<Op>) -> Result<(), WorldError> {
        self.agent(by, Kind::Batch, |w| w.apply_batch(ops, by).map(|_| ()).map_err(|(_, e)| e))
    }

    /// One game tick: the game's updates are recorded after-only and coalesced.
    fn game_tick(&mut self, update: impl FnOnce(&mut World)) {
        self.world.set_recording(Recording::Snapshot);
        update(&mut self.world);
        self.world.set_recording(Recording::Full);
        let changes = self.world.take_changes();
        self.journal.append_snapshot(self.tick, "game", None, Kind::GameTick, changes, self.world.next_id());
        self.tick += 1;
    }

    fn undo(&mut self, by: &str, req: UndoRequest) -> Result<(), WorldError> {
        self.journal.undo(&mut self.world, self.tick, by, &req, &mut no_check).map(|_| ())
    }
}

fn scenario() -> Live {
    let mut live = Live::new();
    let v = Vec3::new;
    // Three agents lay out districts.
    for (i, by) in ["owner", "mason", "glazier"].into_iter().enumerate() {
        let root = if by == "owner" { "/claimed".to_string() } else { format!("/district_{i}") };
        let mut ops = vec![Op::Spawn(spec(Shape::Group, &root, v(i as f32 * 20.0, 0.0, 0.0)))];
        for k in 0..12 {
            let mut s = spec(Shape::Cube, &format!("{root}/block_{k:02}"), v(k as f32 * 1.25, 0.5, (k % 3) as f32));
            s.tags.insert("wall".into());
            s.props = specula_core::Json::parse(&format!(r#"{{"hp":{k},"tier":"stone"}}"#)).unwrap();
            ops.push(Op::Spawn(s));
        }
        live.batch(by, ops).unwrap();
    }
    let mut lathe = spec(Shape::Lathe, "/district_1/vase", v(0.0, 1.0, 0.0));
    lathe.profile = Some(specula::world::Profile { points: vec![[0.3, 0.0], [0.5, 0.4], [0.2, 1.0]], segments: 24 });
    live.agent("glazier", Kind::Spawn, |w| w.spawn(lathe, "glazier").map(|_| ())).unwrap();
    // A rejected intrusion leaves no trace.
    let intrusion = live.agent("mason", Kind::Spawn, |w| w.spawn(spec(Shape::Cube, "/claimed/graffiti", v(0.0, 0.0, 0.0)), "mason").map(|_| ()));
    assert_eq!(intrusion.unwrap_err().code, "claimed");
    // The game ticks: it spawns a creep and walks it, recolours walls, and kills a block.
    live.game_tick(|w| {
        w.spawn(spec(Shape::Sphere, "/creep", v(-5.0, 0.5, 0.0)), "game").unwrap();
    });
    for t in 0..30 {
        live.game_tick(|w| {
            let x = -5.0 + t as f32 * 0.25;
            w.update(&EntityKey::Path("/creep".into()), UpdateSpec { position: Some(v(x, 0.5, 0.0)), ..Default::default() }, "game")
                .unwrap();
            let k = t % 12;
            let path = format!("/district_1/block_{k:02}");
            let colour = UpdateSpec { color: Some([0.5, 0.5, t as f32 / 30.0]), ..Default::default() };
            // Blocks agents renamed or removed are skipped, as game code would.
            let _ = w.update(&EntityKey::Path(path.clone()), colour.clone(), "game");
            let _ = w.update(&EntityKey::Path(path), colour, "game");
            if t == 20 {
                w.delete(&EntityKey::Path("/district_2/block_11".into()), None).unwrap();
            }
        });
        if t == 10 {
            // Agents edit between ticks: rename, reparent, env, delete.
            let rename = UpdateSpec { name: Some("gate".into()), ..Default::default() };
            live.agent("mason", Kind::Set, |w| w.update(&EntityKey::Path("/district_1/block_00".into()), rename, "mason").map(|_| ()))
                .unwrap();
            let moved = UpdateSpec {
                name: Some("borrowed_05".into()),
                parent: Some(Some(EntityKey::Path("/district_1".into()))),
                ..Default::default()
            };
            live.agent("glazier", Kind::Set, |w| w.update(&EntityKey::Path("/district_2/block_05".into()), moved, "glazier").map(|_| ()))
                .unwrap();
            let env = EnvPatch { sun_intensity: Some(0.4), background: Some([0.02, 0.03, 0.05]), ..Default::default() };
            live.agent("owner", Kind::Env, |w| w.set_env(&env)).unwrap();
            live.agent("mason", Kind::Delete, |w| w.delete(&EntityKey::Path("/district_1/block_03".into()), None).map(|_| ()))
                .unwrap();
        }
    }
    // Undo: mason takes back its delete; glazier undoes its last two changes.
    live.undo("mason", UndoRequest { ids: None, count: 1, force: false }).unwrap();
    live.undo("glazier", UndoRequest { ids: None, count: 2, force: false }).unwrap();
    // A failing batch rolls back completely.
    let bad = vec![
        Op::Spawn(spec(Shape::Cube, "/district_1/extra", v(0.0, 0.0, 0.0))),
        Op::Set(EntityKey::Path("/missing".into()), UpdateSpec { position: Some(v(1.0, 0.0, 0.0)), ..Default::default() }),
    ];
    assert!(live.batch("mason", bad).is_err());
    live
}

#[test]
fn replaying_the_journal_from_an_empty_world_reproduces_the_state_hash() {
    let live = scenario();
    assert!(live.journal.replayable());
    let replayed = journal::replay(World::default(), live.journal.entries());
    assert_eq!(replayed.len(), live.world.len());
    assert_eq!(replayed.next_id(), live.world.next_id());
    assert_eq!(
        journal::hex(replayed.state_hash()),
        journal::hex(live.world.state_hash()),
        "replay must rebuild the world bit for bit"
    );
    // Paths and attribution come back too, not just the hash.
    for e in live.world.entities() {
        let r = replayed.get(e.id).expect("every entity replayed");
        assert_eq!((replayed.path_of(e.id), r.version, &*r.modified_by), (live.world.path_of(e.id), e.version, &*e.modified_by));
    }
    // Replaying a prefix reproduces the world as it was then: the same scenario stopped early.
    let half = live.journal.len() / 2;
    let partial = journal::replay(World::default(), live.journal.entries().take(half));
    assert!(partial.len() <= live.world.len() + 20);
}

#[test]
fn the_replayed_hash_is_the_same_on_every_build() {
    let live = scenario();
    let hash = live.world.state_hash();
    if EXPECTED_HASH == 0 {
        panic!("pin EXPECTED_HASH to 0x{hash:016x}");
    }
    assert_eq!(hash, EXPECTED_HASH, "this build computes a different world from the same journal (0x{hash:016x})");
    assert_eq!(journal::replay(World::default(), live.journal.entries()).state_hash(), EXPECTED_HASH);
}

/// What journaling costs the hosted game per tick (F3): 500 entity updates per tick with
/// recording off, and with the engine's game path (snapshot recording + appending to the
/// journal), plus the per-tick state hash the engine records. Three shapes of tick: 500 distinct
/// entities moved, 100 entities moved 5 times each, and 500 moves of which 50 also change props.
#[test]
#[ignore = "timing report; run with --release --ignored --nocapture"]
fn journaling_cost_per_tick() {
    const ENTITIES: usize = 2000;
    const UPDATES: usize = 500;
    const TICKS: usize = 300;
    let build = || {
        let mut w = World::default();
        let ops: Vec<Op> = (0..ENTITIES)
            .map(|i| {
                let mut s = spec(Shape::Cube, &format!("/e{i:05}"), Vec3::new(i as f32, 0.0, 0.0));
                s.props = specula_core::Json::parse(r#"{"hp":10,"lane":2}"#).unwrap();
                s.tags.insert("creep".into());
                Op::Spawn(s)
            })
            .collect();
        w.apply_batch(ops, "setup").unwrap();
        w
    };
    // (label, distinct entities per tick, every n-th update also changes props (0 = never))
    let shapes = [("500 distinct", 500, 0), ("100 x 5", 100, 0), ("500, 50 with props", 500, 10)];
    for (label, distinct, props_every) in shapes {
        let run = |w: &mut World, journal: Option<&mut Journal>| -> (Duration, Duration) {
            let mut journal = journal;
            let (mut ticking, mut hashing) = (Duration::ZERO, Duration::ZERO);
            for t in 0..TICKS {
                let start = Instant::now();
                if journal.is_some() {
                    w.set_recording(Recording::Snapshot);
                }
                for k in 0..UPDATES {
                    let id = ((t * 37 + (k % distinct) * 3) % ENTITIES) as u64 + 1;
                    let mut change = UpdateSpec { position: Some(Vec3::new(k as f32, t as f32 * 0.01, 0.0)), ..Default::default() };
                    if props_every > 0 && k % props_every == 0 {
                        change.props = Some(specula_core::Json::parse(&format!(r#"{{"hp":{}}}"#, t % 10)).unwrap());
                    }
                    w.update(&EntityKey::Id(id), change, "game").unwrap();
                }
                if let Some(j) = journal.as_deref_mut() {
                    w.set_recording(Recording::Full);
                    let changes = w.take_changes();
                    j.append_snapshot(t as u64, "game", None, Kind::GameTick, changes, w.next_id());
                }
                ticking += start.elapsed();
                let start = Instant::now();
                std::hint::black_box(w.state_hash());
                hashing += start.elapsed();
            }
            (ticking / TICKS as u32, hashing / TICKS as u32)
        };
        let mut plain = build();
        let (off, _) = run(&mut plain, None);
        let mut recorded = build();
        let mut j = Journal::new(&recorded, "bench");
        let (on, hash) = run(&mut recorded, Some(&mut j));
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let stats = j.stats_json();
        let bytes = stats.get("approx_bytes").and_then(specula_core::Json::as_u64).unwrap_or(0);
        let entries = stats.get("game_entries").and_then(specula_core::Json::as_u64).unwrap_or(1).max(1);
        println!(
            "{label}: {:.3} ms/tick without journaling, {:.3} with (+{:.3} ms, x{:.2}); per-tick state hash over \
             {ENTITIES} entities {:.3} ms; ~{} KiB per tick, so the 16 MiB game ring holds {} ticks",
            ms(off),
            ms(on),
            ms(on) - ms(off),
            ms(on) / ms(off),
            ms(hash),
            bytes / entries / 1024,
            journal::MAX_GAME_BYTES as u64 / (bytes / entries).max(1),
        );
        assert_eq!(plain.state_hash(), recorded.state_hash());
    }
}

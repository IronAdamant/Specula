//! A game tool an agent calls is claim-checked as that agent; when a claim rejects it, the engine
//! rolls the world back and restores the game from its checkpoint, so game and world stay
//! consistent (Journal lane review round 2, probes P6 and P7). Ids the rejected tool used are
//! never handed out again, so the game can't write through recycled ids into others' claims.

use sepulcra::Content;
use sepulcra_app::SepulcraGame;
use specula::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
use specula::game::{Clock, Ctx, Game};
use specula::journal::{Kind, Writer};
use specula::swarm::Swarm;
use specula::world::{EntityKey, Recording, Shape, SpawnSpec, World, WorldError};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolOutput};
use std::sync::Arc;

const ANN: Writer = Writer { name: "ann", session: Some("aaaaaa11") };
const BOB: Writer = Writer { name: "bob", session: Some("bbbbbb22") };

fn session(id: &str) -> SessionInfo {
    SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

fn claim(p: &str) -> ClaimSpec {
    ClaimSpec { scope: Scope::path(p).unwrap(), lease: DEFAULT_LEASE, note: String::new() }
}

fn spec(path: &str) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Cube);
    let (parent, name) = path.rsplit_once('/').unwrap();
    s.name = Some(name.into());
    s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
    s
}

/// The engine's hosting of a game tool, without the GPU: checkpoint, claim-checked call, and
/// restore on rejection.
struct Host {
    game: SepulcraGame,
    world: World,
    swarm: Swarm,
    clock: Clock,
    ann: SessionInfo,
}

impl Host {
    fn new() -> Host {
        Host::new_with_seed(1)
    }

    fn new_with_seed(seed: u64) -> Host {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), seed);
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let clock = Clock::new(game.tick_hz());
        let mut swarm = Swarm::default();
        swarm.reserve(game.name());
        world.set_recording(Recording::Snapshot);
        game.start(&mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false });
        world.set_recording(Recording::Full);
        swarm.game_changes(&mut world, 0, "sepulcra", Kind::GameStart);
        let (ann, bob) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![ann.clone(), bob.clone()];
        swarm.hello(&ann, "ann".into(), "builder".into(), None, &live).unwrap();
        swarm.hello(&bob, "bob".into(), "builder".into(), None, &live).unwrap();
        Host { game, world, swarm, clock, ann }
    }

    fn tool(&mut self, who: Writer, name: &str, args: Json) -> Result<ToolOutput, WorldError> {
        let Host { game, world, swarm, clock, .. } = self;
        let checkpoint = if swarm.claims.is_empty() { None } else { game.checkpoint() };
        let call = |w: &mut World| game.call_tool(&mut Ctx { world: w, clock, agent: "sepulcra", windowed: false }, name, &args);
        let result = swarm.game_tool(world, clock.tick(), "sepulcra", who, call);
        if result.is_err() {
            game.restore(checkpoint.expect("sepulcra checkpoints"));
        }
        result
    }

    fn tick(&mut self) {
        let Host { game, world, swarm, clock, .. } = self;
        world.set_recording(Recording::Snapshot);
        game.tick(&mut Ctx { world, clock, agent: "sepulcra", windowed: false });
        world.set_recording(Recording::Full);
        swarm.game_changes(world, clock.tick(), "sepulcra", Kind::GameTick);
        clock.finish_tick();
    }

    fn claim(&mut self, path: &str) {
        let ann = self.ann.clone();
        self.swarm.take_claim(&self.world, &ann, claim(path)).unwrap();
    }

    fn ann_builds(&mut self, root: &str, count: usize) -> Vec<u64> {
        (0..count)
            .map(|i| {
                let p = if i == 0 { root.to_string() } else { format!("{root}/h{i:03}") };
                self.swarm.commit(&mut self.world, 0, ANN, Kind::Spawn, |w| w.spawn(spec(&p), "ann").map(|e| e.id)).unwrap().0
            })
            .collect()
    }
}

/// P6: a rejected `place` used to leave the stone in the game (dust spent) with its entity id
/// cached; a later `sell` then deleted whatever the recycled id became (ann's entity).
#[test]
fn a_rejected_place_leaves_game_and_world_as_they_were() {
    let mut h = Host::new();
    h.claim("/units/stones");
    let (game_hash, dust, next_id) = (h.game.state().hash(), h.game.state().dust(), h.world.next_id());
    let place = Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]);
    let err = h.tool(BOB, "play_command", place).unwrap_err();
    assert_eq!((err.code, err.detail("game")), ("claimed", Some(&Json::from("sepulcra"))));
    assert_eq!((h.game.state().hash(), h.game.state().dust()), (game_hash, dust), "the game went back too");
    assert_eq!(h.game.state().stones().count(), 0);
    assert!(h.world.next_id() >= next_id, "ids are never reused");
    // Ann's claim ends; she builds elsewhere; bob's sell finds no stone and touches nothing.
    h.swarm.release(ANN, None).unwrap();
    let ann = h.ann_builds("/ann", 40);
    let sell = Json::obj([("action", Json::from("sell")), ("cell", Json::from("top:12,9"))]);
    let out = h.tool(BOB, "play_command", sell);
    assert!(out.map_or(true, |o| o.is_error), "nothing to sell");
    assert!(ann.iter().all(|id| h.world.get(*id).is_some()), "ann's entities are intact");
    assert_eq!(h.swarm.journal.replay().unwrap().state_hash(), h.world.state_hash());
}

/// P7: a rejected `play_new` left the game on a new scene whose entities had been rolled back;
/// its ids were then reissued to ann, and thousands of game ticks wrote into her claim.
#[test]
fn a_rejected_new_game_keeps_ticks_out_of_others_claims() {
    let mut h = Host::new();
    h.claim("/units/stones");
    let game_hash = h.game.state().hash();
    let err = h.tool(BOB, "play_new", Json::obj([("seed", Json::from(2))])).unwrap_err();
    assert_eq!(err.code, "claimed");
    assert_eq!(h.game.state().hash(), game_hash, "still the old game");
    h.claim("/ann");
    let ann = h.ann_builds("/ann", 400);
    let versions: Vec<u64> = ann.iter().map(|id| h.world.get(*id).unwrap().version).collect();
    // Enemies, bolts and flashes are transient (never saved, ids of their own), so count what
    // the game had alive at once.
    let enemies = h.world.find_path("/live/enemies").unwrap();
    let mut most = 0;
    for _ in 0..6000 {
        h.tick();
        most = most.max(h.world.children_of(Some(enemies)).count());
    }
    assert!(most > 5, "the game spawned plenty while ticking ({most} enemies at once at most)");
    let touched = ann.iter().zip(&versions).filter(|(id, v)| h.world.get(**id).is_none_or(|e| e.version != **v)).count();
    let mut foreign = 0;
    h.world.walk(Some(ann[0]), |e, _| foreign += usize::from(&*e.created_by != "ann"));
    assert_eq!((touched, foreign), (0, 0), "game ticks stayed out of ann's claim");
    assert_eq!(h.swarm.journal.replay().map(|w| w.state_hash()), h.swarm.journal.replay().map(|_| h.world.state_hash()));
}

/// Without claims nothing is checkpointed and tools apply as before.
#[test]
fn tools_apply_normally_without_claims() {
    let mut h = Host::new();
    let place = Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]);
    let out = h.tool(BOB, "play_command", place).unwrap();
    assert!(!out.is_error);
    assert_eq!(h.game.state().stones().count(), 1);
    let entry = h.swarm.journal.entries().last().unwrap();
    assert_eq!((entry.kind, entry.via.as_deref()), (Kind::GameTool, Some("bob")));
}

/// The game's name is reserved: no agent can write as the game.
#[test]
fn agents_cannot_take_the_games_name() {
    let mut h = Host::new();
    let s = session("cccccc33");
    let err = h.swarm.hello(&s, "sepulcra".into(), "builder".into(), None, std::slice::from_ref(&s)).unwrap_err();
    assert_eq!(err.code, "name_reserved");
}

/// Review round 3 (R7): rejected tools leave `play_state` byte-identical, and the run goes on
/// exactly as a run that never made those calls (simulation and everything the game shows).
#[test]
fn rejected_tools_leave_no_trace_in_the_game() {
    let new_host = || {
        let mut h = Host::new_with_seed(7);
        for _ in 0..300 {
            h.tick();
        }
        h.claim("/units/stones");
        h
    };
    let (mut a, mut b) = (new_host(), new_host());
    let all = || Json::obj([("include", Json::arr(["enemies", "stones", "walls", "waves", "log"].map(Json::from)))]);
    let text = |o: ToolOutput| format!("{:?}", o.content);
    let before = text(a.tool(BOB, "play_state", all()).unwrap());
    let place = || Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]);
    assert_eq!(a.tool(BOB, "play_command", place()).unwrap_err().code, "claimed");
    let multi = Json::obj([("commands", Json::arr([Json::obj([("action", Json::from("call_wave"))]), place()]))]);
    assert!(a.tool(BOB, "play_command", multi).is_err());
    assert!(a.tool(BOB, "play_new", Json::obj([("seed", Json::from(3))])).is_err());
    assert_eq!(text(a.tool(BOB, "play_state", all()).unwrap()), before, "play_state is unchanged");
    for _ in 0..2000 {
        a.tick();
        b.tick();
    }
    assert_eq!(a.game.state().hash(), b.game.state().hash(), "the simulation didn't diverge");
    let view = |h: &Host| {
        let mut v: Vec<_> = h
            .world
            .entities()
            .filter(|e| &*e.created_by == "sepulcra")
            .map(|e| {
                let p = e.world_position();
                (h.world.path_of(e.id), e.version, [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(view(&a), view(&b), "nor did what it shows");
}

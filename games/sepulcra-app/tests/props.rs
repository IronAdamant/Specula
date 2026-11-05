//! Map 1's props in the game (the owner's rule, 2026-11-06): every prop of the map data stands in
//! the world under `/props/graveyard`, a stone on a big piece is refused with the prop named (as
//! data and in words), and small dressing under a stone is hidden, then shown again when the
//! stone is sold. A world that already holds the props (the approved M4 world) keeps them.

use sepulcra::Content;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Game, GameWorld};
use specula::world::World;
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::sync::Arc;

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
}

impl Host {
    fn new() -> Host {
        Host::with_world(World::default())
    }

    fn with_world(mut world: World) -> Host {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), 7);
        let clock = Clock::new(game.tick_hz());
        game.start(&mut Ctx { world: GameWorld::new(&mut world), clock: &clock, agent: "sepulcra", windowed: false });
        Host { game, world, clock }
    }

    fn call(&mut self, name: &str, args: &str) -> ToolOutput {
        let Host { game, world, clock } = self;
        game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, name, &Json::parse(args).unwrap())
    }

    fn tick(&mut self) {
        let Host { game, world, clock } = self;
        game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
        self.clock.finish_tick();
    }

    fn visible(&self, name: &str) -> bool {
        let id = self.world.find_path(&format!("/props/graveyard/{name}")).unwrap_or_else(|_| panic!("no prop {name}"));
        self.world.get(id).unwrap().visible
    }
}

fn text(out: &ToolOutput) -> Json {
    out.content
        .iter()
        .find_map(|c| match c {
            specula_protocol::Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        })
        .unwrap_or(Json::Null)
}

#[test]
fn every_prop_stands_in_the_world_where_the_map_says() {
    let h = Host::new();
    let content = Content::builtin();
    let map = &content.map;
    assert_eq!(map.props.len(), 33);
    for p in &map.props {
        let id = h.world.find_path(&format!("/props/graveyard/{}", p.name)).unwrap_or_else(|_| panic!("{} is in the world", p.name));
        let e = h.world.get(id).unwrap();
        assert_eq!(e.model.as_ref().map(|m| m.recipe.as_str()), Some(format!("recipes/{}.json", p.recipe).as_str()));
        let [x, _, z] = p.at.to_f32();
        assert!((e.position.x - x).abs() < 1e-4 && (e.position.z - z).abs() < 1e-4, "{} at {:?}", p.name, e.position);
        assert!((0.0..=0.8).contains(&e.position.y), "{} stands on its fragment: y {}", p.name, e.position.y);
        assert!((e.rotation_deg.y - p.yaw_deg()).abs() < 1e-3);
        assert!(e.visible, "{} is drawn at the start", p.name);
        assert_eq!(e.props.get("blocks_build"), Some(&Json::Bool(p.blocks_build)), "{}: agents read the rule from the entity", p.name);
        assert_eq!(e.props.get("cells").and_then(Json::as_array).map(|c| c.len()), Some(p.cells.len()));
        assert!(!e.transient, "props are saved, journaled and hashed with the world");
    }
}

#[test]
fn a_stone_on_a_big_prop_is_refused_with_the_prop_named() {
    let mut h = Host::new();
    let out = h.call("play_command", r#"{"action": "place", "cell": "top:8,2", "stone": "warding"}"#);
    assert!(out.is_error, "the grave angel covers top:8,2");
    let j = text(&out);
    assert_eq!(j.get("error").and_then(Json::as_str), Some("rejected"), "{j}");
    assert_eq!(j.get("code").and_then(Json::as_str), Some("prop_blocks"), "{j}");
    assert_eq!(j.get("prop").and_then(Json::as_str), Some("statue_angel"), "{j}");
    assert_eq!(j.get("what").and_then(Json::as_str), Some("a grave angel"), "{j}");
    let msg = j.get("message").and_then(Json::as_str).unwrap_or_default();
    assert!(msg.ends_with("a grave angel stands here"), "the HUD's words: {msg}");
    // A batch says the same per command, and refuses nothing else.
    let out = h.call("play_command", r#"{"commands": [{"action": "place", "cell": "top:9,1", "stone": "warding"}, {"action": "place", "cell": "top:8,0", "stone": "warding"}]}"#);
    let j = text(&out);
    let results = j.get("results").and_then(Json::as_array).unwrap();
    assert_eq!(results[0].get("code").and_then(Json::as_str), Some("prop_blocks"));
    assert_eq!(results[0].get("prop").and_then(Json::as_str), Some("mausoleum_domed"));
    assert_eq!(results[1].get("accepted"), Some(&Json::Bool(true)), "the cell beside the mausoleum is free");
}

#[test]
fn dressing_under_a_stone_is_hidden_and_comes_back_when_it_is_sold() {
    let mut h = Host::new();
    assert!(h.visible("bone_pile"));
    let out = h.call("play_command", r#"{"action": "place", "cell": "top:2,15", "stone": "warding"}"#);
    assert!(!out.is_error, "dressing doesn't block: {:?}", text(&out));
    h.tick();
    assert!(!h.visible("bone_pile"), "the bones under the stone are hidden, never drawn through it");
    assert!(h.visible("ribcage_buried") && h.visible("statue_angel"), "only the dressing under the stone");
    let out = h.call("play_command", r#"{"action": "sell", "cell": "top:2,15"}"#);
    assert!(!out.is_error);
    h.tick();
    assert!(h.visible("bone_pile"), "sold: the bones show again");
    // Hiding is a pure function of the stones: a rebuild (new game) shows everything again.
    h.call("play_command", r#"{"action": "place", "cell": "top:1,14", "stone": "warding"}"#);
    h.tick();
    assert!(!h.visible("bone_pile"));
    let out = h.call("play_new", r#"{"seed": 7}"#);
    assert!(!out.is_error, "{:?}", text(&out));
    h.tick();
    assert!(h.visible("bone_pile"));
}

/// The approved M4 world: the owner's promotion was each prop's last write, and its entities
/// predate the facts. Adopting them writes nothing (the approval stays their last write); only a
/// prop away from its data place is moved back, with the data's facts.
#[test]
fn approved_props_keep_the_owners_last_write() {
    use specula::world::{EntityKey, UpdateSpec};
    let mut a = Host::new();
    let angel = a.world.find_path("/props/graveyard/statue_angel").unwrap();
    let fence = a.world.find_path("/props/graveyard/fence_iron_corner").unwrap();
    // As the approved world holds them: no facts, written by window:owner; the fence at (2, 10).
    a.world.update(&EntityKey::Id(angel), UpdateSpec { props: Some(Json::obj([("m4", Json::Bool(true))])), ..UpdateSpec::default() }, "window:owner").unwrap();
    let pos = a.world.get(fence).unwrap().position;
    let moved = specula_core::math::Vec3::new(2.0, pos.y, 10.0);
    a.world.update(&EntityKey::Id(fence), UpdateSpec { position: Some(moved), props: Some(Json::obj([("m4", Json::Bool(true))])), ..UpdateSpec::default() }, "window:owner").unwrap();
    let v = a.world.get(angel).unwrap().version;
    assert!(!a.call("play_new", r#"{"seed": 3}"#).is_error);
    let e = a.world.get(angel).unwrap();
    assert_eq!((&*e.modified_by, e.version), ("window:owner", v), "the approval stays the angel's last write");
    let f = a.world.get(fence).unwrap();
    assert!((f.position.x - 3.0).abs() < 1e-4 && (f.position.z - 11.0).abs() < 1e-4, "the fence goes back to its data place: {:?}", f.position);
    assert_eq!(f.props.get("blocks_build"), Some(&Json::Bool(true)), "with the data's facts");
    assert_eq!(f.props.get("m4"), Some(&Json::Bool(true)), "its own props kept");
    // Facts that contradict the data are corrected, never left to mislead an agent.
    a.world.update(&EntityKey::Id(angel), UpdateSpec { props: Some(Json::obj([("blocks_build", Json::Bool(false))])), ..UpdateSpec::default() }, "bob").unwrap();
    assert!(!a.call("play_new", r#"{"seed": 3}"#).is_error);
    assert_eq!(a.world.get(angel).unwrap().props.get("blocks_build"), Some(&Json::Bool(true)));
}

#[test]
fn a_world_that_holds_the_props_keeps_them() {
    let mut a = Host::new();
    let id = a.world.find_path("/props/graveyard/statue_angel").unwrap();
    let version = a.world.get(id).unwrap().version;
    // A new game over the same world adopts every prop: same entities, nothing rewritten.
    let out = a.call("play_new", r#"{"seed": 3}"#);
    assert!(!out.is_error);
    let again = a.world.find_path("/props/graveyard/statue_angel").unwrap();
    assert_eq!((again, a.world.get(again).unwrap().version), (id, version), "adopted, not respawned or rewritten");
    let n = a.world.children_of(Some(a.world.find_path("/props/graveyard").unwrap())).count();
    assert_eq!(n, 33);
}

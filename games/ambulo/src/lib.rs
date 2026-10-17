//! Ambulo (Latin "I walk"): the example game module. It adds a playable character, a capsule
//! that walks with the `move` action and jumps with `jump`, on a small ground slab with a few
//! crates to climb.
//!
//! It shows the module rules (see `docs/design/MODULES.md`):
//! - **stateless code:** the character's vertical speed lives in the player entity's `props`
//!   (visible to agents in `world_query`, saved with the world) and the jump count in the
//!   engine-owned state store, so a rebuild reloads without losing anything;
//! - **actions, not keys:** `actions.json` binds keys, the gamepad and agents (`logic_input`);
//! - **deterministic:** the tick is the only clock and `dmath` does the trigonometry;
//! - **controller behind an interface:** `move_character` is the engine's character
//!   controller (a kinematic placeholder until physics P2 replaces it under the same call).
//!
//! Edit a constant, call `logic_build {"path": "games/ambulo"}`, and the running world picks it
//! up at the next tick.

use specula_sdk::{dmath, Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};

/// Walking speed, metres per second.
const WALK_SPEED: f32 = 4.0;
/// Take-off speed of a jump, metres per second (about 1 m high with this gravity).
const JUMP_SPEED: f32 = 6.0;
const GRAVITY: f32 = 18.0;
/// Fastest fall, metres per second.
const TERMINAL_SPEED: f32 = 30.0;
const RADIUS: f32 = 0.3;
const HEIGHT: f32 = 1.8;

const ROOT: &str = "/ambulo";
const PLAYER: &str = "/ambulo/player";

#[derive(Default)]
pub struct Ambulo;

specula_sdk::export_module!(Ambulo);

impl Module for Ambulo {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "ambulo",
            version: env!("CARGO_PKG_VERSION"),
            description: "A walking, jumping capsule (/ambulo/player) on a slab with crates; actions move and jump.",
            order: 0,
            actions: include_str!("../actions.json"),
            hooks: vec![HookDef {
                name: "launch",
                doc: "Throws the player straight up at `speed` m/s (default 8), even in mid-air. Answers the new vertical speed.",
                args: r#"{"type": "object", "properties": {"speed": {"type": "number", "minimum": 0, "maximum": 50}}}"#,
            }],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        // After a reload everything below already exists: find it, build nothing twice.
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("ambulo"))?;
        }
        if cx.query(Some("ground"), None, 1)?.is_empty() {
            cx.spawn(
                Spawn::new("cube")
                    .name("ground")
                    .parent(ROOT)
                    .position(Vec3::new(0.0, -0.1, 0.0))
                    .scale(Vec3::new(24.0, 0.2, 24.0))
                    .color("#4a5560")
                    .tags(&["ground"])
                    .field("collider", Value::map().with("shape", "box")),
            )?;
            for (i, (x, z, h)) in [(3.0f32, -2.0f32, 0.6f32), (5.0, -2.0, 1.2), (-4.0, 3.0, 0.9)].into_iter().enumerate() {
                cx.spawn(
                    Spawn::new("cube")
                        .name(&format!("crate_{}", i + 1))
                        .parent(ROOT)
                        .position(Vec3::new(x, h / 2.0, z))
                        .scale(Vec3::new(1.2, h, 1.2))
                        .color("#8a6a48")
                        .tags(&["ground", "crate"])
                        .field("collider", Value::map().with("shape", "box")),
                )?;
            }
        }
        if cx.find(PLAYER)?.is_none() {
            spawn_player(cx)?;
        }
        let loads = cx.state("loads")?.as_u64().unwrap_or(0) + 1;
        cx.set_state("loads", loads)?;
        cx.log(&format!("ambulo {} started (load {loads}, reloaded: {})", env!("CARGO_PKG_VERSION"), cx.reloaded()))
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        // An agent may have deleted the player: put a new one back.
        let id = match cx.find(PLAYER)? {
            Some(id) => id,
            None => spawn_player(cx)?,
        };
        let me = cx.entity(id)?.ok_or_else(|| SdkError::new("not_found", "the player vanished mid-tick"))?;
        let mut vy = me.prop("vy").as_f32().unwrap_or(0.0);
        let grounded = me.prop("grounded").as_bool().unwrap_or(false);
        let dt = cx.dt();

        let stick = cx.action("move").value;
        let mut walk = Vec3::new(stick[0], 0.0, -stick[1]);
        let len = walk.length();
        if len > 1.0 {
            walk = walk * (1.0 / len);
        }
        walk = walk * WALK_SPEED;

        if cx.action("jump").pressed && grounded {
            vy = JUMP_SPEED;
            let jumps = cx.state("jumps")?.as_u64().unwrap_or(0) + 1;
            cx.set_state("jumps", jumps)?;
            cx.emit("jumped", Value::map().with("jumps", jumps))?;
        }
        vy = (vy - GRAVITY * dt).max(-TERMINAL_SPEED);

        let moved = cx.move_character(id, Vec3::new(walk.x * dt, vy * dt, walk.z * dt))?;
        if moved.grounded && vy < 0.0 {
            vy = 0.0;
        }
        let mut patch = Patch::new().props(Value::map().with("vy", vy).with("grounded", moved.grounded));
        if len > 0.05 {
            // Face the way we walk: yaw about +Y, 0° looking down +Z.
            let yaw = dmath::atan2(walk.x, walk.z).to_degrees();
            patch = patch.rotation_deg(Vec3::new(0.0, yaw, 0.0));
        }
        cx.set(id, patch)?;
        Ok(())
    }

    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        match name {
            "launch" => {
                let speed = args.get("speed").and_then(Value::as_f32).unwrap_or(8.0);
                if !(0.0..=50.0).contains(&speed) {
                    return Err(SdkError::new("invalid_field", "'speed' must be 0..50 m/s"));
                }
                let id = cx.find(PLAYER)?.ok_or_else(|| SdkError::new("not_found", "no /ambulo/player to launch"))?;
                cx.set(id, Patch::new().props(Value::map().with("vy", speed).with("grounded", false)))?;
                Ok(Value::map().with("vy", speed))
            }
            other => Err(SdkError::new("unknown_hook", format!("ambulo has no hook '{other}' (it has: launch)"))),
        }
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        // A reload keeps everything; unloading takes the module's things with it.
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
        }
        Ok(())
    }
}

fn spawn_player(cx: &mut Cx) -> Result<Id, SdkError> {
    // A capsule as a lathe profile: feet at the entity's origin, head at HEIGHT.
    let mut profile = Vec::new();
    const CAP: usize = 6;
    for i in 0..=CAP {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / CAP as f32;
        profile.push(Value::from([(RADIUS * dmath::sin(a)).max(0.0), RADIUS - RADIUS * dmath::cos(a)]));
    }
    for i in 0..=CAP {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / CAP as f32;
        // cos(π/2) rounds to a hair below zero: radii must not.
        profile.push(Value::from([(RADIUS * dmath::cos(a)).max(0.0), HEIGHT - RADIUS + RADIUS * dmath::sin(a)]));
    }
    cx.spawn(
        Spawn::new("lathe")
            .name("player")
            .parent(ROOT)
            .position(Vec3::new(0.0, 0.0, 0.0))
            .color("#5ec8e0")
            .tags(&["player", "character"])
            .props(Value::map().with("vy", 0.0f32).with("grounded", true))
            .field("profile", profile)
            .field("segments", 16)
            .field(
                "collider",
                Value::map()
                    .with("shape", "capsule")
                    .with("radius", RADIUS)
                    .with("height", HEIGHT)
                    .with("offset", Vec3::new(0.0, HEIGHT / 2.0, 0.0)),
            ),
    )
}

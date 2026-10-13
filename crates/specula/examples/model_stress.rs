//! Foundation F3 stress: a hosted game with 500 persistent model entities (warding stones,
//! three tiers, two seeds: six builds) and 500 transient model enemies that it moves every tick
//! and eases every frame. `perf_frame` reports the costs (`sim_tick`, `game_frame`,
//! `draw_list`, GPU per pass); `engine_info` game status reports the spawn time.
//!
//! ```text
//! cargo run -p specula --example model_stress -- --assets games/sepulcra/assets --port 7492
//! cargo run -p specula --example model_stress -- --assets games/sepulcra/assets -- --count 1000
//! ```
//! (`scripts/model_stress.py` runs it and prints the numbers.)

use specula::game::{Ctx, Frame, Game};
use specula::sight::Camera;
use specula::world::{EntityKey, EnvPatch, ModelSpec, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::Vec3;
use specula_core::Json;
use std::time::Instant;

#[derive(Default)]
struct Stress {
    count: usize,
    stones: usize,
    enemies: Vec<u64>,
    spawn_ms: f64,
    failures: usize,
}

impl Stress {
    /// Where enemy `i` is at simulated time `t` (seconds): a slow walk around its own circle.
    fn enemy_at(&self, i: usize, t: f32) -> (Vec3, f32) {
        let n = self.count.max(1) as f32;
        let side = (n.sqrt().ceil()) as usize;
        let (gx, gz) = ((i % side) as f32, (i / side) as f32);
        let phase = i as f32 * 0.37 + t * 0.6;
        let p = Vec3::new(gx * 3.0 - side as f32 * 1.5 + phase.cos() * 0.8, 0.0, gz * 3.0 - side as f32 * 1.5 + phase.sin() * 0.8);
        (p, -phase.to_degrees())
    }
}

impl Game for Stress {
    fn name(&self) -> &str {
        "model_stress"
    }

    fn configure(&mut self, args: &[String]) -> Result<(), String> {
        self.count = 500;
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--count" => {
                    self.count = it.next().and_then(|v| v.parse().ok()).filter(|n| (1..=20_000).contains(n)).ok_or("--count needs 1..20000")?
                }
                other => return Err(format!("unknown argument '{other}' (model_stress takes --count N)")),
            }
        }
        Ok(())
    }

    fn start(&mut self, ctx: &mut Ctx) {
        let started = Instant::now();
        let w = &mut *ctx.world;
        let night = EnvPatch {
            sun_direction: Some(Vec3::new(0.43, 0.5, 0.75)),
            sun_color: Some([0.79, 0.84, 1.0]),
            sun_intensity: Some(1.3),
            ambient_color: Some([0.165, 0.204, 0.282]),
            ambient_intensity: Some(2.2),
            exposure: Some(1.1),
            tonemap: Some(true),
            ..Default::default()
        };
        w.set_env(&night).expect("valid environment");
        for (name, transient) in [("stones", false), ("enemies", false)] {
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some(name.into());
            g.transient = transient;
            w.spawn(g, ctx.agent).expect("group");
        }
        let side = (self.count as f32).sqrt().ceil() as usize;
        for i in 0..self.count {
            let spec = ModelSpec::new("recipes/warding_stone.json").with_param("tier", (1 + i % 3) as f64).with_seed((i / 3 % 2) as u64);
            let mut s = SpawnSpec::model(spec);
            s.parent = Some(EntityKey::Path("/stones".into()));
            s.position = Vec3::new((i % side) as f32 * 3.0 - side as f32 * 1.5 + 1.5, 0.0, (i / side) as f32 * 3.0 - side as f32 * 1.5 + 1.5);
            match w.spawn(s, ctx.agent) {
                Ok(_) => self.stones += 1,
                Err(e) => {
                    self.failures += 1;
                    eprintln!("model_stress: stone {i}: {e}");
                }
            }
        }
        for i in 0..self.count {
            let recipe = if i % 2 == 0 { "recipes/shambler.json" } else { "recipes/skeleton_runner.json" };
            let mut s = SpawnSpec::model(ModelSpec::new(recipe));
            s.parent = Some(EntityKey::Path("/enemies".into()));
            s.transient = true;
            let (p, yaw) = self.enemy_at(i, 0.0);
            s.position = p;
            s.rotation_deg = Vec3::new(0.0, yaw, 0.0);
            match w.spawn(s, ctx.agent) {
                Ok(e) => self.enemies.push(e.id),
                Err(e) => {
                    self.failures += 1;
                    eprintln!("model_stress: enemy {i}: {e}");
                }
            }
        }
        self.spawn_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!("model_stress: spawned {} stones and {} transient enemies in {:.1} ms", self.stones, self.enemies.len(), self.spawn_ms);
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        let t = ctx.clock.seconds() as f32;
        for (i, &id) in self.enemies.iter().enumerate() {
            let (p, yaw) = self.enemy_at(i, t);
            let change = UpdateSpec { position: Some(p), rotation_deg: Some(Vec3::new(0.0, yaw, 0.0)), ..UpdateSpec::default() };
            if ctx.world.update(&EntityKey::Id(id), change, ctx.agent).is_err() {
                self.failures += 1;
            }
        }
    }

    fn frame(&mut self, ctx: &mut Ctx, frame: &mut Frame) {
        // Ease every enemy between ticks (what a game's presentation does each frame).
        let t = ctx.clock.seconds() as f32 + frame.alpha / ctx.clock.hz() as f32;
        for (i, &id) in self.enemies.iter().enumerate() {
            let (p, yaw) = self.enemy_at(i, t);
            let change = UpdateSpec { position: Some(p), rotation_deg: Some(Vec3::new(0.0, yaw, 0.0)), ..UpdateSpec::default() };
            if ctx.world.update(&EntityKey::Id(id), change, ctx.agent).is_err() {
                self.failures += 1;
            }
        }
        if frame.camera.is_none() {
            let side = (self.count as f32).sqrt().ceil() * 3.0;
            *frame.camera = Some(Camera { eye: Vec3::new(0.0, side * 0.55, side * 0.75), target: Vec3::ZERO, fov_y_deg: 50.0 });
        }
    }

    fn status(&self) -> Json {
        Json::obj([
            ("stones", Json::from(self.stones)),
            ("enemies", Json::from(self.enemies.len())),
            ("spawn_ms", Json::from(self.spawn_ms)),
            ("failures", Json::from(self.failures)),
        ])
    }
}

fn main() {
    if let Err(e) = specula::run_game(Box::new(Stress::default())) {
        eprintln!("model_stress: {e}");
        std::process::exit(1);
    }
}

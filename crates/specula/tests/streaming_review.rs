//! Streaming review findings on a real GPU (each test skips cleanly without Vulkan):
//! agent-written clips reach drawing with streaming on; a capture waits for a game's transient
//! creatures; a capture never draws a changed cell's old HLOD proxy; residency follows size on
//! screen (far big models stay, orthographic views too).

use specula::anim::{ASSET_PROP, ASSET_TAG};
use specula::hlod;
use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, Library, LodChoice, LodStats, SharedLibrary};
use specula::sight::{self, Camera, Meshes, Projection, View};
use specula::streaming::{StreamTable, Streamer};
use specula::world::{EntityKey, ModelSource, ModelSpec, Shape, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_gpu::Gpu;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A world edit the tests make between captures.
type Edit = Box<dyn Fn(&mut World)>;

fn gpu_or_skip(test: &str) -> Option<Gpu> {
    if !specula_gpu::is_available() {
        eprintln!("SKIPPED {test}: Vulkan is not available");
        return None;
    }
    match Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("SKIPPED {test}: {e}");
            None
        }
    }
}

fn library() -> Arc<SharedLibrary> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    Arc::new(SharedLibrary::new(Library::new(&assets, None)))
}

fn world_of(shared: &Arc<SharedLibrary>) -> World {
    let mut world = World::default();
    world.set_model_source(Some(Arc::clone(shared) as Arc<dyn ModelSource>));
    world
}

fn model(recipe: &str, pos: Vec3, scale: f32) -> SpawnSpec {
    let mut s = SpawnSpec::model(ModelSpec::new(recipe));
    s.position = pos;
    s.scale = Vec3::new(scale, scale, scale);
    s
}

fn wait_builds(shared: &Arc<SharedLibrary>, world: &mut World) {
    let deadline = Instant::now() + Duration::from_secs(180);
    while shared.lock().in_flight() > 0 && Instant::now() < deadline {
        let (_, changed) = shared.lock().finish(Some(Duration::from_millis(50)));
        world.refresh_model_handles(&changed, false);
    }
}

fn streamer(gpu: &mut Gpu, models: &mut GpuModels, patch: &[(&str, Json)]) -> Streamer {
    let mut stream = Streamer::new(StreamTable::default(), "integrated");
    let mut all = vec![("settle_s", Json::from(0.0)), ("upload_ms", Json::from(50.0)), ("upload_mb", Json::from(64.0))];
    all.extend(patch.iter().cloned());
    stream.set(&Json::obj(all), "test", gpu, models).unwrap();
    stream
}

/// One window frame (headless: no window camera) and its flush.
fn tick(stream: &mut Streamer, gpu: &mut Gpu, models: &mut GpuModels, shared: &Arc<SharedLibrary>, world: &World) {
    stream.sync_models(models, gpu, shared, world, 0);
    stream.begin_frame();
    stream.frame(gpu, models, shared, world, 0, true);
    gpu.flush_uploads().unwrap();
}

/// A capture's streaming step: its view becomes a source; what it still waits for.
fn capture(stream: &mut Streamer, gpu: &mut Gpu, models: &mut GpuModels, shared: &Arc<SharedLibrary>, world: &World, cam: Camera) -> usize {
    stream.sync_models(models, gpu, shared, world, 0);
    stream.capture_source(cam, 640, 360, None);
    stream.frame(gpu, models, shared, world, 0, false);
    gpu.flush_uploads().unwrap();
    stream.waiting_in(&[(cam, 640, 360, None)], world, models)
}

/// What streaming has done so far: proxy builds and uploads, model uploads, passes. A wait that
/// sees none of it change for [`STUCK`] fails: the work is stuck, not slow.
fn progress(stream: &Streamer, gpu: &Gpu, models: &GpuModels) -> String {
    let j = stream.json(gpu, models);
    let h = j.get("hlod").cloned().unwrap_or(Json::Null);
    let pick = |v: &Json, k: &str| v.get(k).map_or_else(String::new, Json::to_string);
    let totals = j.get("totals").cloned().unwrap_or(Json::Null);
    ["built", "uploaded", "building", "ready_to_upload", "current"].iter().map(|k| pick(&h, k)).chain([pick(&totals, "uploads"), pick(&j, "passes")]).collect::<Vec<_>>().join(",")
}

/// How long a wait may see no progress at all before it calls the work stuck (generous: other
/// sessions share this machine's CPU and GPU).
const STUCK: Duration = Duration::from_secs(60);

/// Captures (with a frame between each, as the engine runs them) until `done`; fails only when
/// nothing progresses for [`STUCK`] (or after 600 s in all).
#[allow(clippy::too_many_arguments)]
fn wait_until(
    stream: &mut Streamer,
    gpu: &mut Gpu,
    models: &mut GpuModels,
    shared: &Arc<SharedLibrary>,
    world: &World,
    cam: Camera,
    what: &str,
    mut done: impl FnMut(&mut Streamer, &GpuModels) -> bool,
) {
    let start = Instant::now();
    let (mut last, mut since) = (String::new(), Instant::now());
    loop {
        tick(stream, gpu, models, shared, world);
        capture(stream, gpu, models, shared, world, cam);
        if done(stream, models) {
            return;
        }
        let now = progress(stream, gpu, models);
        if now != last {
            (last, since) = (now, Instant::now());
        }
        assert!(
            since.elapsed() < STUCK && start.elapsed() < Duration::from_secs(600),
            "{what}: stuck (no progress for {:?}, {:?} in all): {}",
            since.elapsed(),
            start.elapsed(),
            stream.json(gpu, models)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn draw(world: &World, models: &GpuModels, shapes: &ShapeMeshes, cam: Camera) -> LodStats {
    models.begin_frame(1.0);
    let lathes = LatheMeshes::default();
    let meshes = Meshes { shapes, lathes: &lathes, models };
    let mut view = View { camera: cam, viewport_h: 360, include_ui: false, lod: LodChoice::Fresh };
    let mut out = Vec::new();
    sight::draw_list(world, &meshes, &mut view, &mut out, None)
}

/// Review finding 1: a world anim asset (an agent's `anim_set` clip) reaches the anim set the
/// streamed path draws with, also when someone else read the changed assets first.
#[test]
fn agent_clips_reach_drawing_with_streaming_on() {
    let Some(mut gpu) = gpu_or_skip("agent_clips_reach_drawing_with_streaming_on") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    let id = world.spawn(model("recipes/shambler.json", Vec3::ZERO, 1.0), "t").unwrap().id;
    let clip = |bone_deg: f32| {
        Json::parse(&format!(
            r#"{{"kind":"keys","duration":1.2,"loop":"loop","keys":[{{"time":0,"pose":{{"upperarm_r":{{"rot":[{bone_deg},0,-20]}}}}}},{{"time":0.6,"pose":{{"upperarm_r":{{"rot":[{bone_deg},0,20]}}}}}}]}}"#
        ))
        .unwrap()
    };
    let asset = |name: &str, data: Json| {
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some(name.into());
        s.tags.insert(ASSET_TAG.into());
        s.props = Json::obj([(ASSET_PROP, Json::obj([("for", Json::from("recipes/shambler.json")), ("kind", Json::from("clip")), ("data", data)]))]);
        s
    };
    world.spawn(asset("wave", clip(-120.0)), "t").unwrap();
    wait_builds(&shared, &mut world);
    let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[]);
    tick(&mut stream, &mut gpu, &mut models, &shared, &world);
    let set = models.anim_set_of(handle).expect("a rigged model has a set");
    assert!(set.id("wave").is_some(), "the world's clip is in the set the streamed path draws with");
    // A second clip, read first by another reader (a tool refreshing the assets): still seen.
    world.spawn(asset("wave2", clip(-60.0)), "t").unwrap();
    models.refresh_anim_env(&shared, &world, 0);
    tick(&mut stream, &mut gpu, &mut models, &shared, &world);
    let set = models.anim_set_of(handle).unwrap();
    assert!(set.id("wave").is_some() && set.id("wave2").is_some(), "both clips");
    // The plain path keeps them too.
    let mut plain = GpuModels::default();
    plain.sync(&mut gpu, &shared, &world, 0);
    assert!(plain.anim_set_of(handle).unwrap().id("wave2").is_some());
}

/// Review finding 2: a capture waits for a game's transient creature in view whose build isn't
/// on the GPU, which is uploaded first, without waiting for a pass.
#[test]
fn captures_wait_for_transient_creatures() {
    let Some(mut gpu) = gpu_or_skip("captures_wait_for_transient_creatures") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    let mut s = model("recipes/bone_colossus.json", Vec3::new(0.0, 0.0, 19.0), 1.0);
    s.transient = true;
    world.spawn(s, "game").unwrap();
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    // Uploads limited to one item a frame: the creature takes several.
    let mut stream = streamer(&mut gpu, &mut models, &[("upload_ms", Json::from(0.1)), ("upload_mb", Json::from(0.25))]);
    let cam = Camera::new(Vec3::new(0.0, 6.0, 0.0), Vec3::new(0.0, 3.0, 19.0), 50.0);
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let first = capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
    assert!(first > 0, "the capture waits for the creature");
    let mut waited = 0;
    while capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam) > 0 {
        waited += 1;
        assert!(waited < 200, "the creature arrives");
        tick(&mut stream, &mut gpu, &mut models, &shared, &world);
    }
    let stats = draw(&world, &models, &shapes, cam);
    assert_eq!(stats.not_resident, 0, "drawn once the capture stops waiting");
    assert_eq!(stats.coarser, 0, "at the finest level");
}

/// Review finding 3: an edit in a cell a capture draws as a proxy; the next capture from the
/// same camera never draws the old proxy (its members draw), and a later one draws the new one.
#[test]
fn captures_never_draw_a_changed_cells_old_proxy() {
    let Some(mut gpu) = gpu_or_skip("captures_never_draw_a_changed_cells_old_proxy") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    let mut ids = Vec::new();
    for i in 0..16 {
        let pos = Vec3::new(300.0 + (i % 4) as f32 * 6.0, 0.0, (i / 4) as f32 * 6.0);
        // Seeded: a build each (a build many entities use is drawn instanced, never merged).
        let mut spec = model("recipes/tombstone.json", pos, 4.0);
        spec.model = Some(ModelSpec { recipe: "recipes/tombstone.json".into(), params: Vec::new(), seed: Some(100 + i) });
        ids.push(world.spawn(spec, "t").unwrap().id);
    }
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("hlod_min_m", Json::from(0.0))]);
    let cam = Camera::new(Vec3::new(0.0, 30.0, 10.0), Vec3::new(300.0, 0.0, 10.0), 40.0);
    let covered = |stream: &mut Streamer, id: u64| {
        let mut out = Vec::new();
        stream.draw_capture(&cam, 360, &mut out);
        hlod::covers(&stream.hlod.capture_cover, id)
    };
    let first = ids[0];
    wait_until(&mut stream, &mut gpu, &mut models, &shared, &world, cam, "the far cell gets a proxy", |s, _| covered(s, first));
    for &id in &ids {
        world.update(&EntityKey::Id(id), UpdateSpec { color: Some([0.2, 0.2, 1.0]), ..Default::default() }, "t").unwrap();
    }
    // At once, from the same camera (no new view, no pass due): not the old proxy.
    capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
    assert!(!covered(&mut stream, ids[0]), "the old proxy (old colour) is not drawn");
    // The new one comes.
    wait_until(&mut stream, &mut gpu, &mut models, &shared, &world, cam, "a new proxy is built", |s, _| covered(s, first));
}

/// Integrator's follow-up: a proxy was built and then never uploaded (`ready_to_upload: 1`,
/// `deferred` 1 every frame). A frame whose own streaming work (filing, the wanted set) had used
/// its upload budget uploaded nothing, and that frame after frame; and captures with no frame
/// between them shared one budget forever. Now a frame's first item always goes, and a capture
/// long after the last frame starts a budget of its own. Deterministic: the budget is spent by
/// a charge, not by timing.
#[test]
fn uploads_progress_when_the_budget_is_spent_before_the_pump() {
    let Some(mut gpu) = gpu_or_skip("uploads_progress_when_the_budget_is_spent_before_the_pump") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    for i in 0..12 {
        let mut spec = model("recipes/tombstone.json", Vec3::new((i % 4) as f32 * 3.0, 0.0, 20.0 + (i / 4) as f32 * 3.0), 1.0);
        spec.model = Some(ModelSpec { recipe: "recipes/tombstone.json".into(), params: Vec::new(), seed: Some(500 + i) });
        world.spawn(spec, "t").unwrap();
    }
    wait_builds(&shared, &mut world);
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let cam = Camera::new(Vec3::new(4.0, 6.0, 0.0), Vec3::new(4.0, 0.0, 24.0), 50.0);
    let spent = |ms: f64, items: u32| specula::two_d::Used { items, bytes: 0, ms, deferred: 0 };
    let uploads = |stream: &Streamer, gpu: &Gpu, models: &GpuModels| stream.json(gpu, models).get("totals").and_then(|t| t.get("uploads")).and_then(Json::as_u64).unwrap_or(0);

    // Frames whose other work used the whole budget: each still uploads its first item.
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("upload_ms", Json::from(2.0))]);
    stream.set_window(Some((cam, 1280, 720)));
    // (Frames until it all arrived; a frame uploads nothing only while every upload batch is
    // still in flight on a busy GPU, so the cap is time, generous.)
    let deadline = Instant::now() + STUCK;
    while Instant::now() < deadline {
        stream.sync_models(&mut models, &mut gpu, &shared, &world, 0);
        stream.begin_frame();
        stream.charge(spent(1.0e6, 0));
        stream.frame(&mut gpu, &mut models, &shared, &world, 0, true);
        gpu.flush_uploads().unwrap();
        if draw(&world, &models, &shapes, cam).not_resident == 0 && stream.json(&gpu, &models).get("pending_builds").and_then(Json::as_u64) == Some(0) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(uploads(&stream, &gpu, &models) > 0, "one item a frame goes: {}", stream.json(&gpu, &models));
    assert_eq!(draw(&world, &models, &shapes, cam).not_resident, 0, "everything arrives: {}", stream.json(&gpu, &models));

    // Captures with no frame between them, the budget (and the first item) spent long ago: a
    // capture after FRAME_STALE has a budget of its own.
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("upload_ms", Json::from(2.0))]);
    stream.begin_frame();
    stream.charge(spent(1.0e6, 1));
    let deadline = Instant::now() + STUCK;
    while Instant::now() < deadline {
        std::thread::sleep(specula::streaming::FRAME_STALE + Duration::from_millis(20));
        if capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam) == 0 {
            break;
        }
    }
    assert_eq!(draw(&world, &models, &shapes, cam).not_resident, 0, "captures alone upload: {}", stream.json(&gpu, &models));
}

/// Review finding 4: residency follows size on screen: a big model beyond radius_m stays on the
/// GPU (HLOD off, so nothing merges), a small one there doesn't; an orthographic view wants
/// what is in it whatever the eye's distance.
#[test]
fn big_far_models_stay_resident() {
    let Some(mut gpu) = gpu_or_skip("big_far_models_stay_resident") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    let mut big = model("recipes/tombstone.json", Vec3::new(0.0, 0.0, 507.0), 20.0);
    big.name = Some("big".into());
    world.spawn(big, "t").unwrap();
    let mut small = model("recipes/sky_obelisk.json", Vec3::new(40.0, 0.0, 507.0), 0.02);
    small.name = Some("small".into());
    world.spawn(small, "t").unwrap();
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("hlod", Json::from(false)), ("radius_m", Json::from(384.0))]);
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let cam = Camera::new(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 507.0), 40.0);
    for _ in 0..50 {
        if capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam) == 0 {
            break;
        }
    }
    let stats = draw(&world, &models, &shapes, cam);
    let j = stream.state_json(&gpu, &models, &world, None, 8).to_string();
    assert_eq!(stats.not_resident, 1, "the big far one is drawn, the tiny one isn't: {j}");
    // Orthographic, 60 m tall, the eye 900 m away: the big one is wanted (the eye's distance
    // doesn't shrink anything in an orthographic view).
    let mut world2 = world_of(&shared);
    world2.spawn(model("recipes/tombstone.json", Vec3::new(0.0, 0.0, 0.0), 4.0), "t").unwrap();
    wait_builds(&shared, &mut world2);
    let mut models2 = GpuModels::default();
    let mut stream2 = streamer(&mut gpu, &mut models2, &[("hlod", Json::from(false)), ("radius_m", Json::from(100.0))]);
    let ortho = Camera { projection: Projection::Ortho { height_m: 60.0 }, ..Camera::new(Vec3::new(0.0, 900.0, 1.0), Vec3::ZERO, 50.0) };
    for _ in 0..50 {
        if capture(&mut stream2, &mut gpu, &mut models2, &shared, &world2, ortho) == 0 {
            break;
        }
    }
    let stats = draw(&world2, &models2, &shapes, ortho);
    assert_eq!(stats.not_resident, 0, "in an orthographic view: {}", stream2.state_json(&gpu, &models2, &world2, None, 8));
}

/// Review of the fixes: captures don't run whole passes for changes they can't see. An edit out
/// of the capture's view (and a deleted cell's stale key) leave repeated captures from a known
/// view without a pass; an edit in view is redone out of turn, also without a whole pass.
#[test]
fn captures_redo_only_changed_cells_in_view() {
    let Some(mut gpu) = gpu_or_skip("captures_redo_only_changed_cells_in_view") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    let mut ids = Vec::new();
    for i in 0..64 {
        let pos = Vec3::new((i % 8) as f32 * 40.0, 0.0, (i / 8) as f32 * 40.0);
        ids.push(world.spawn(model("recipes/tombstone.json", pos, 2.0), "t").unwrap().id);
    }
    // Behind the camera: out of view.
    let behind = world.spawn(model("recipes/tombstone.json", Vec3::new(0.0, 0.0, -300.0), 2.0), "t").unwrap().id;
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[]);
    let cam = Camera::new(Vec3::new(140.0, 40.0, -60.0), Vec3::new(140.0, 0.0, 140.0), 50.0);
    for _ in 0..20 {
        if capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam) == 0 {
            break;
        }
    }
    let passes = |stream: &Streamer, gpu: &Gpu, models: &GpuModels| stream.json(gpu, models).get("passes").and_then(Json::as_u64).unwrap();
    // Quickly (passes otherwise come every UPDATE_EVERY after a change): a few captures each.
    let edits: [(&str, Edit); 3] = [
        (
            "an edit out of view",
            Box::new(move |w: &mut World| {
                w.update(&EntityKey::Id(behind), UpdateSpec { color: Some([1.0, 0.0, 0.0]), ..Default::default() }, "t").unwrap();
            }),
        ),
        (
            "a deleted cell",
            Box::new(move |w: &mut World| {
                w.delete(&EntityKey::Id(behind), None).unwrap();
            }),
        ),
        (
            "an edit in view",
            Box::new(move |w: &mut World| {
                w.update(&EntityKey::Id(ids[9]), UpdateSpec { color: Some([0.0, 0.0, 1.0]), ..Default::default() }, "t").unwrap();
            }),
        ),
    ];
    for (what, edit) in &edits {
        std::thread::sleep(Duration::from_millis(1100));
        // A pass due by time (UPDATE_IDLE) runs first, then the check.
        capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
        edit(&mut world);
        let t = Instant::now();
        let before = passes(&stream, &gpu, &models);
        for _ in 0..3 {
            capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
        }
        let after = passes(&stream, &gpu, &models);
        if t.elapsed() < Duration::from_millis(90) {
            assert_eq!(after, before, "{what}: no whole pass");
        } else {
            eprintln!("{what}: too slow to tell ({:?})", t.elapsed());
        }
    }
}

/// Integrator's m2 run: a build many entities use is drawn instanced (one draw), so no proxy
/// merges it: a far field of identical models keeps drawing as instances.
#[test]
fn instanced_builds_are_not_merged() {
    let Some(mut gpu) = gpu_or_skip("instanced_builds_are_not_merged") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    for i in 0..16 {
        let pos = Vec3::new(300.0 + (i % 4) as f32 * 6.0, 0.0, (i / 4) as f32 * 6.0);
        world.spawn(model("recipes/tombstone.json", pos, 4.0), "t").unwrap();
    }
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("hlod_min_m", Json::from(0.0))]);
    let cam = Camera::new(Vec3::new(0.0, 30.0, 10.0), Vec3::new(300.0, 0.0, 10.0), 40.0);
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(2) {
        capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut out = Vec::new();
    stream.draw_capture(&cam, 360, &mut out);
    assert!(out.is_empty(), "no proxy for an instanced build: {}", stream.state_json(&gpu, &models, &world, None, 8));
    let stats = draw(&world, &models, &shapes, cam);
    assert_eq!(stats.not_resident, 0, "all 16 drawn as instances");
}

/// Integrator's round 2: hidden copies aren't drawn instanced, so they don't count towards
/// INSTANCED_MIN: 7 shown + 9 hidden copies of one far build still merge into a proxy.
#[test]
fn hidden_copies_dont_count_as_instanced() {
    let Some(mut gpu) = gpu_or_skip("hidden_copies_dont_count_as_instanced") else { return };
    let shared = library();
    let mut world = world_of(&shared);
    for i in 0..16 {
        let pos = Vec3::new(300.0 + (i % 4) as f32 * 6.0, 0.0, (i / 4) as f32 * 6.0);
        let mut s = model("recipes/tombstone.json", pos, 4.0);
        s.visible = i < 7;
        world.spawn(s, "t").unwrap();
    }
    wait_builds(&shared, &mut world);
    let mut models = GpuModels::default();
    let mut stream = streamer(&mut gpu, &mut models, &[("hlod_min_m", Json::from(0.0))]);
    let cam = Camera::new(Vec3::new(0.0, 30.0, 10.0), Vec3::new(300.0, 0.0, 10.0), 40.0);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        capture(&mut stream, &mut gpu, &mut models, &shared, &world, cam);
        let mut out = Vec::new();
        stream.draw_capture(&cam, 360, &mut out);
        if !out.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "7 shown copies merge: {}", stream.state_json(&gpu, &models, &world, None, 8));
        std::thread::sleep(Duration::from_millis(20));
    }
}

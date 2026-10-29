//! Drawn facts come from the frame record (`docs/design/AUTHORITY.md`, class B), checked on a
//! real GPU (skips cleanly without Vulkan): each way an entity can fail to reach the image is
//! knocked out in turn (behind the camera, hidden, an unbuilt model, occluded, the ID pass off),
//! and the record's `drawn` must agree with an independent readback of the ID image (which
//! entity is the nearest opaque surface at each pixel).
//!
//! The M4 finding it pins: `sight_query` said `drawn: true` for an entity behind the camera,
//! because its subjects came from the world, not from what the frame drew.

use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, LodChoice};
use specula::sight::{self, Camera, FrameRecord, Meshes, View};
use specula::world::{ModelSpec, Shape, SpawnSpec, World};
use specula_core::math::Vec3;
use std::collections::BTreeSet;

fn gpu_or_skip() -> Option<specula_gpu::Gpu> {
    if !specula_gpu::is_available() {
        eprintln!("SKIPPED frame_record_gpu: Vulkan is not available");
        return None;
    }
    match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("SKIPPED frame_record_gpu: {e}");
            None
        }
    }
}

fn cube(world: &mut World, name: &str, pos: Vec3, scale: Vec3, visible: bool) -> u64 {
    let mut s = SpawnSpec::new(Shape::Cube);
    s.name = Some(name.into());
    s.position = pos;
    s.scale = scale;
    s.visible = visible;
    world.spawn(s, "t").unwrap().id
}

#[test]
fn drawn_facts_match_an_independent_readback() {
    let Some(mut gpu) = gpu_or_skip() else { return };
    let mut world = World::default();
    let front = cube(&mut world, "front", Vec3::new(-2.0, 0.0, 0.0), Vec3::ONE, true);
    let behind = cube(&mut world, "behind", Vec3::new(0.0, 0.0, 20.0), Vec3::ONE, true);
    let hidden = cube(&mut world, "hidden", Vec3::new(0.0, 0.0, 0.0), Vec3::ONE, false);
    let wall = cube(&mut world, "wall", Vec3::new(2.5, 0.0, 4.0), Vec3::new(3.0, 3.0, 0.2), true);
    let occluded = cube(&mut world, "occluded", Vec3::new(2.5, 0.0, -2.0), Vec3::new(0.5, 0.5, 0.5), true);
    // A model whose recipe doesn't exist: never built, so never drawn.
    let unbuilt = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/no_such_recipe.json")), "t").unwrap().id;
    let camera = Camera::new(Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 0.0), 60.0);
    let (w, h) = (160u32, 120u32);
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let lathes = LatheMeshes::default();
    let models = GpuModels::default();
    models.begin_frame(0.0);
    let meshes = Meshes { shapes: &shapes, lathes: &lathes, models: &models };
    let mut view = View { camera, viewport_h: h, include_ui: false, lod: LodChoice::Fresh };
    let (mut draws, mut owners) = (Vec::new(), Vec::new());
    sight::draw_list(&world, &meshes, &mut view, &mut draws, Some(&mut owners));
    let view_proj = camera.view_proj(w, h);
    let (ids, entities) = sight::dense_ids(&owners);
    let frame = sight::frame(&world, &view_proj, &draws, &[], 0.0);
    let target = gpu.create_target(w, h).unwrap();
    gpu.render_with_ids(target, &frame, &ids).unwrap();
    let counts = gpu.id_coverage(target, entities.len() as u32).unwrap();
    let image = gpu.read_ids(target).unwrap();
    // The independent readback: entities that are the nearest opaque surface somewhere.
    let seen: BTreeSet<u64> = image.ids.iter().filter(|&&i| i > 0).map(|&i| entities[i as usize - 1]).collect();

    let mut record = FrameRecord::new(&owners);
    record.set_ids(&entities, &counts.per_id);
    let drawn = |id: u64| record.drawn(id).drawn();
    // Every entity the readback saw is drawn; an entity the record says isn't drawn never
    // shows in the readback.
    for id in [front, behind, hidden, wall, occluded, unbuilt] {
        if seen.contains(&id) {
            assert_eq!(drawn(id), Some(true), "entity {id} is in the image but the record says otherwise");
        }
        if drawn(id) == Some(false) {
            assert!(!seen.contains(&id), "entity {id}: the record says not drawn, yet the image shows it");
        }
    }
    assert_eq!(drawn(front), Some(true));
    assert!(seen.contains(&front) && seen.contains(&wall));
    // Behind the camera: submitted (the GPU culls it), but it never reaches the image.
    assert!(record.submitted().contains(&behind), "the draw list submits it; the GPU culls it");
    assert_eq!(drawn(behind), Some(false), "M4 finding: drawn: true for an entity behind the camera");
    // Hidden, and the unbuilt model: never submitted.
    assert_eq!(drawn(hidden), Some(false));
    assert_eq!(drawn(unbuilt), Some(false));
    assert!(!record.submitted().contains(&hidden) && !record.submitted().contains(&unbuilt));
    // Occluded: rasterized, hidden by the wall (drawn, but not in the readback).
    assert!(!seen.contains(&occluded), "the wall hides it");
    let mut j = specula_core::Json::Obj(Vec::new());
    record.drawn(occluded).write(&mut j);
    if counts.per_id.iter().all(|c| c.rasterized.is_some()) {
        assert_eq!(drawn(occluded), Some(true), "{j}");
        assert!(j.get("reason").and_then(specula_core::Json::as_str).is_some_and(|r| r.contains("occluded")), "{j}");
    }
    // The ID pass off: nothing about the image is claimed.
    let unmeasured = FrameRecord::new(&owners);
    assert_eq!(unmeasured.drawn(front).drawn(), None, "without the ID pass, drawn is unmeasured");
    let mut j = specula_core::Json::Obj(Vec::new());
    unmeasured.drawn(front).write(&mut j);
    assert_eq!(j.get("measured_by").and_then(specula_core::Json::as_str), Some("draw_list"));
    gpu.destroy_target(target);
}

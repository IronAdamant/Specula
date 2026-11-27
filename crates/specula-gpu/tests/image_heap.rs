//! The image heap (`src/heap.rs`) on real GPUs under validation: images of many sizes are placed
//! in blocks the worker made ahead, an image bigger than a block gets a block of its own that goes
//! (on the worker) when the image does, ranges come back when images go, and nothing is reported
//! by the validation layer (binding at offsets, aliasing-free placement). Skips cleanly without
//! Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, MemoryKind, TextureFormat, IMAGE_BLOCK_BYTES};
use std::time::{Duration, Instant};

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap_or_default() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

/// Polls (`flush_uploads` adopts what the worker grew) until `done`; `HANG_BOUND` only names a hang.
fn wait_for(gpu: &mut specula_gpu::Gpu, test: &str, what: &str, done: impl Fn(&specula_gpu::MemoryStats) -> bool) {
    let started = Instant::now();
    loop {
        gpu.flush_uploads().unwrap();
        let s = gpu.memory_stats();
        if done(&s) {
            return;
        }
        assert!(started.elapsed() < specula_core::HANG_BOUND, "{test}: still waiting for {what} after {:?}: {:?}", specula_core::HANG_BOUND, s.image_heap);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn images_are_placed_in_blocks_made_ahead_and_given_back() {
    every_adapter("images_are_placed_in_blocks_made_ahead_and_given_back", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        // The first block is made ahead at device creation, on the worker.
        wait_for(&mut gpu, test, "the first block never came", |s| s.image_heap.grown_ahead >= 1);
        let start = gpu.memory_stats();
        assert_eq!(start.image_heap.sync_blocks, 0, "{test}: {:?}", start.image_heap);

        // Many sizes and formats: all in blocks, none allocated on this thread.
        let mut textures = Vec::new();
        for (i, &(w, h)) in [(1u32, 1u32), (7, 3), (64, 64), (256, 128), (1000, 10), (513, 777), (1024, 1024)].iter().enumerate() {
            let fmt = if i % 2 == 0 { TextureFormat::Rgba8Srgb } else { TextureFormat::R8 };
            let px = vec![(i * 37) as u8; (w * h) as usize * if i % 2 == 0 { 4 } else { 1 }];
            textures.push(gpu.upload_texture(w, h, fmt, &px).unwrap());
        }
        let placed = gpu.memory_stats();
        assert_eq!(placed.image_heap.images, start.image_heap.images + textures.len() as u32, "{test}: {:?}", placed.image_heap);
        assert_eq!(placed.allocations - placed.image_heap.blocks, start.allocations - start.image_heap.blocks, "{test}: textures make no allocation of their own");
        assert!(placed.bytes(MemoryKind::Textures) >= 1024 * 1024 * 4, "{test}");
        assert!(placed.image_heap.used_bytes <= placed.image_heap.block_bytes);
        assert_eq!(placed.bytes(MemoryKind::Reserve), placed.image_heap.block_bytes - placed.image_heap.used_bytes, "{test}: the reserve is the blocks' unused room");

        // Bigger than a block: a block of its own, freed (on the worker) once the image goes.
        let (bw, bh) = (4096u32, (IMAGE_BLOCK_BYTES / 4 / 4096) as u32 + 64);
        let big = gpu.upload_texture(bw, bh, TextureFormat::Rgba8Srgb, &vec![200u8; (bw * bh * 4) as usize]).unwrap();
        let with_big = gpu.memory_stats();
        assert!(with_big.image_heap.block_bytes >= placed.image_heap.block_bytes + IMAGE_BLOCK_BYTES, "{test}: {:?}", with_big.image_heap);
        gpu.destroy_texture(big);
        for t in textures {
            gpu.destroy_texture(t);
        }
        let after = gpu.memory_stats();
        assert_eq!(after.image_heap.images, start.image_heap.images, "{test}: every range came back: {:?}", after.image_heap);
        assert_eq!(after.bytes(MemoryKind::Textures), start.bytes(MemoryKind::Textures), "{test}");
        assert!(after.image_heap.freed_ahead >= 1, "{test}: the oversized block was freed on the worker: {:?}", after.image_heap);
        assert!(after.image_heap.block_bytes < with_big.image_heap.block_bytes, "{test}: {:?}", after.image_heap);

        // Targets come and go at many sizes (a capture's colour image): ranges reused.
        for &(w, h) in &[(320u32, 180u32), (1280, 720), (640, 640), (1280, 720)] {
            let t = gpu.create_target(w, h).unwrap();
            gpu.destroy_target(t);
        }
        let end = gpu.memory_stats();
        assert_eq!(end.image_heap.images, start.image_heap.images, "{test}: {:?}", end.image_heap);
        eprintln!("{test}: {:?}", end.image_heap);
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: validation errors: {errors:?}");
    });
}

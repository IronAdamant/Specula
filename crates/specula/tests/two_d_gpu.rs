//! 2D entities on a real GPU, through the engine's `TwoDGpu`: textures load on the loader
//! thread, a missing PNG shows up once it is written, an edited PNG replaces the old one, and a
//! warm frame of sheet sprites, tile maps and shapes allocates nothing on the engine thread.
//! Skips cleanly (prints why, passes) without Vulkan.

use specula::two_d::{self, TwoD, TwoDGpu};
use specula_core::deflate::Level;
use specula_core::math::{Mat4, Vec3};
use specula_core::{png, Json};
use specula_gpu::{Draw2d, FrameDesc, Gpu, GpuOptions, Validation};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: forwards every call to the system allocator unchanged; only counts (this thread's).
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: same contract as our caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as our caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: same contract as our caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations_during(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    COUNTING.with(|on| on.set(true));
    f();
    COUNTING.with(|on| on.set(false));
    COUNT.with(|c| c.get())
}

fn gpu_or_skip(test: &str) -> Option<Gpu> {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED - Vulkan is not available");
        return None;
    }
    match Gpu::new(&GpuOptions { validation: Validation::Auto, ..Default::default() }) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("{test}: SKIPPED - {e}");
            None
        }
    }
}

fn temp_assets(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("specula-two-d-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("s")).unwrap();
    dir
}

fn write_png(path: &Path, w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) {
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&f(x, y));
        }
    }
    std::fs::write(path, png::encode_rgba8(w, h, &px, Level::Fastest)).unwrap();
}

fn wait_loaded(t: &mut TwoDGpu, gpu: &mut Gpu) {
    let start = Instant::now();
    loop {
        t.pump(gpu);
        if t.loading() == 0 || start.elapsed() > Duration::from_secs(10) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 4 x 4 m orthographic view of the XY plane around the origin (reversed Z, down -Z).
fn view() -> [f32; 16] {
    let proj = Mat4::orthographic_vk_reverse_z(-2.0, 2.0, -2.0, 2.0, 0.05, 1000.0);
    (proj * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 100.0), Vec3::ZERO, Vec3::Y)).to_cols_array()
}

fn centre_pixel(gpu: &mut Gpu, draws: &[Draw2d]) -> [u8; 4] {
    let target = gpu.create_target(32, 32).unwrap();
    gpu.render(target, &FrameDesc { view_proj: view(), reverse_z: true, draws_2d: draws, ..Default::default() }).unwrap();
    let px = gpu.read_rgba8(target).unwrap();
    gpu.destroy_target(target);
    let i = (16 * 32 + 16) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn sprite(text: &str) -> TwoD {
    two_d::parse("sprite", &Json::parse(text).unwrap()).unwrap()
}

#[test]
fn textures_load_appear_and_reload() {
    let test = "textures_load_appear_and_reload";
    let Some(mut gpu) = gpu_or_skip(test) else { return };
    let dir = temp_assets("reload");
    let file = dir.join("s/a.png");
    let mut t = TwoDGpu::new(&dir);
    let s = sprite(r#"{"texture":"s/a.png","size":[4,4]}"#);
    let mut out = Vec::new();
    let draw = |t: &mut TwoDGpu, gpu: &mut Gpu, out: &mut Vec<Draw2d>| {
        out.clear();
        t.begin_frame(gpu);
        t.draws(gpu, 1, &s, &Mat4::IDENTITY, [1.0; 4], 0.0, out);
    };
    // Missing: nothing drawn, the problem says why.
    draw(&mut t, &mut gpu, &mut out);
    wait_loaded(&mut t, &mut gpu);
    draw(&mut t, &mut gpu, &mut out);
    assert!(out.is_empty());
    assert!(t.frame_problems.iter().any(|p| p.contains("s/a.png")), "{:?}", t.frame_problems);
    // Written: picked up by the file poll, drawn red.
    write_png(&file, 2, 2, |_, _| [255, 0, 0, 255]);
    t.poll_files(true);
    wait_loaded(&mut t, &mut gpu);
    draw(&mut t, &mut gpu, &mut out);
    assert_eq!(out.len(), 1, "{:?}", t.frame_problems);
    assert!(t.frame_problems.is_empty() && t.errors.is_empty(), "problems clear once it loads: {:?}", t.errors);
    assert_eq!(&centre_pixel(&mut gpu, &out)[..3], &[255, 0, 0]);
    // Edited (same size, so only the time stamp changes): blue replaces red.
    std::thread::sleep(Duration::from_millis(30));
    write_png(&file, 2, 2, |_, _| [0, 0, 255, 255]);
    t.poll_files(true);
    wait_loaded(&mut t, &mut gpu);
    draw(&mut t, &mut gpu, &mut out);
    assert_eq!(&centre_pixel(&mut gpu, &out)[..3], &[0, 0, 255]);
    // Broken: the last good texture stays, the problem is reported.
    std::fs::write(&file, b"not a png").unwrap();
    t.poll_files(true);
    wait_loaded(&mut t, &mut gpu);
    draw(&mut t, &mut gpu, &mut out);
    assert_eq!(out.len(), 1);
    assert!(t.frame_problems.iter().any(|p| p.contains("PNG")), "{:?}", t.frame_problems);
    t.collect(&mut gpu, &[]);
    assert!(t.errors.is_empty(), "problems go with the texture");
    let _ = std::fs::remove_dir_all(&dir);
    let errors = gpu.close().into_iter().filter(|m| m.severity == specula_gpu::Severity::Error).count();
    assert_eq!(errors, 0, "{test}: validation errors");
}

#[test]
fn warm_two_d_frames_allocate_nothing() {
    let test = "warm_two_d_frames_allocate_nothing";
    let Some(mut gpu) = gpu_or_skip(test) else { return };
    let dir = temp_assets("alloc");
    write_png(&dir.join("s/sheet.png"), 64, 16, |x, _| [(x * 4) as u8, 100, 200, 255]);
    write_png(&dir.join("s/tiles.png"), 32, 16, |x, y| [(x * 8) as u8, (y * 16) as u8, 50, 255]);
    let mut t = TwoDGpu::new(&dir);
    let items: Vec<TwoD> = vec![
        sprite(r#"{"texture":"s/sheet.png","sheet":{"frame_px":[16,16]},"anim":{"count":4,"fps":8}}"#),
        sprite(r#"{"texture":"s/sheet.png","sheet":{"frame_px":[16,16]},"frame":2,"billboard":"spherical"}"#),
        two_d::parse(
            "tilemap",
            &Json::parse(r#"{"tileset":"s/tiles.png","tile_px":[16,16],"size":[64,64],"animations":[{"tile":1,"frames":2,"frame_ms":100}],"layers":[{"rows":["1*64","2*64"]}]}"#).unwrap(),
        )
        .unwrap(),
        two_d::parse("shape2d", &Json::parse(r##"{"items":[{"circle":[0,0,1],"fill":"#ff0000","stroke":{"width":0.1}}]}"##).unwrap()).unwrap(),
    ];
    let mut out = Vec::with_capacity(64);
    let target = gpu.create_target(64, 64).unwrap();
    let frame = |t: &mut TwoDGpu, gpu: &mut Gpu, out: &mut Vec<Draw2d>, time_s: f32| {
        out.clear();
        t.begin_frame(gpu);
        for (i, item) in items.iter().enumerate() {
            t.draws(gpu, i as u64 + 1, item, &Mat4::IDENTITY, [1.0; 4], time_s, out);
        }
        gpu.render(target, &FrameDesc { view_proj: view(), reverse_z: true, draws_2d: out, time_s, ..Default::default() }).unwrap();
    };
    frame(&mut t, &mut gpu, &mut out, 0.0);
    wait_loaded(&mut t, &mut gpu);
    for k in 0..5 {
        frame(&mut t, &mut gpu, &mut out, k as f32 * 0.1);
    }
    assert!(t.frame_problems.is_empty(), "{:?}", t.frame_problems);
    assert_eq!(out.len(), 2 + 2 + 1, "two sprites, two tile chunks, one shape");
    let n = allocations_during(|| {
        for k in 0..20 {
            frame(&mut t, &mut gpu, &mut out, 1.0 + k as f32 * 0.07);
        }
    });
    eprintln!("{test}: {n} allocations in 20 warm frames");
    assert_eq!(n, 0, "warm 2D frames allocate");
    gpu.destroy_target(target);
    let _ = std::fs::remove_dir_all(&dir);
}

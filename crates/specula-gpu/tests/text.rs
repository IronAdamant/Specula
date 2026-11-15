//! In-world text on real GPUs ([`Draw2d::Text`], Slug coverage): the GPU port of the coverage
//! matches the CPU reference (`specula_text::slug`) pixel for pixel within a small tolerance, text
//! stays sharp under perspective at a grazing angle, the glyph store is copied incrementally, and
//! sight's ID buffer counts the glyphs a run shows. Every test skips cleanly without Vulkan and
//! requires zero validation errors. Captures go to `$SPECULA_TEST_OUT` as `text_*.png`.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, Draw2d, FrameDesc, GlyphInstance, Gpu, TextFrame, TextRun};
use specula_text::{layout, slug, Bundled, Font, SlugStore, TextStyle};

const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Orthographic, reversed Z, looking down −Z at the XY plane: `w` × `h` metres centred on 0.
fn ortho(w: f32, h: f32) -> [f32; 16] {
    let (near, far) = (0.05f32, 100.0f32);
    let d = far - near;
    let proj = [2.0 / w, 0.0, 0.0, 0.0, 0.0, -2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0 / d, 0.0, 0.0, 0.0, far / d, 1.0];
    mul(&proj, &translation([0.0, 0.0, -50.0]))
}

/// `text` laid out at `px` metres per em with its top-left at (`x0`, `top`) in the run's plane:
/// the glyphs appended to `glyphs`, stored in `store`.
fn lay_out(font: &Font, store: &mut SlugStore, text: &str, px: f32, x0: f32, top: f32, glyphs: &mut Vec<GlyphInstance>) -> [f32; 4] {
    let l = layout(font, text, &TextStyle::new(px));
    for g in &l.glyphs {
        let Some(s) = store.glyph(font, g.glyph).unwrap() else { continue };
        glyphs.push(GlyphInstance { word: s.word, origin: [x0 + g.x, top - g.y], size: px, bounds: s.bounds, color: [1.0; 4] });
    }
    [x0, top - l.height, x0 + l.width, top]
}

/// The CPU reference image: each glyph's coverage at every pixel centre, composited "over".
fn reference(store: &SlugStore, glyphs: &[GlyphInstance], w: u32, h: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            // Plane coordinates of the pixel centre (1 m = 1 px, origin at the image centre).
            let p = [x as f32 + 0.5 - w as f32 / 2.0, h as f32 / 2.0 - (y as f32 + 0.5)];
            let mut a = 0.0f32;
            for g in glyphs {
                let em = [(p[0] - g.origin[0]) / g.size, (p[1] - g.origin[1]) / g.size];
                let c = slug::coverage_from_words(store.words(), g.word, em, [g.size, g.size]);
                a += c * (1.0 - a);
            }
            out[(y * w + x) as usize] = a;
        }
    }
    out
}

fn finish(test: &str, gpu: Gpu) {
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation errors", errors.len());
}

#[test]
fn gpu_coverage_matches_the_reference() {
    let font = Bundled::UiRegular.load().unwrap();
    let display = Bundled::Display.load().unwrap();
    for adapter in [AdapterPreference::HighPerformance, AdapterPreference::LowPower] {
        let test = format!("text_reference[{adapter:?}]");
        let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
        let (w, h) = (320u32, 120u32);
        let mut store = SlugStore::new(slug::DEFAULT_STORE_BYTES);
        let mut glyphs = Vec::new();
        let r1 = lay_out(&font, &mut store, "Specula Жλ@", 40.0, -150.0, 55.0, &mut glyphs);
        let first2 = glyphs.len() as u32;
        let r2 = lay_out(&display, &mut store, "Ωmega ß 7", 30.0, -150.0, 5.0, &mut glyphs);
        let runs = [
            Draw2d::Text(TextRun { model: IDENTITY, rect: r1, first: 0, count: first2, color: [1.0; 4], layer: 0, lit: false }),
            Draw2d::Text(TextRun { model: IDENTITY, rect: r2, first: first2, count: glyphs.len() as u32 - first2, color: [1.0; 4], layer: 0, lit: false }),
        ];
        let frame = FrameDesc {
            view_proj: ortho(w as f32, h as f32),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            reverse_z: true,
            draws_2d: &runs,
            text: TextFrame { store: store.id(), words: store.words(), glyphs: &glyphs },
            ..Default::default()
        };
        let target = gpu.create_target(w, h).unwrap();
        gpu.render(target, &frame).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        write_png(&format!("text_reference_{adapter:?}.png"), &rgba, w, h);
        let want = reference(&store, &glyphs, w, h);
        let (mut sum, mut worst, mut covered) = (0.0f32, 0.0f32, 0u32);
        for i in 0..(w * h) as usize {
            let got = srgb_to_linear(rgba[i * 4 + 1] as f32 / 255.0);
            let d = (got - want[i]).abs();
            sum += d;
            worst = worst.max(d);
            covered += (want[i] > 0.5) as u32;
        }
        let mean = sum / (w * h) as f32;
        assert!(covered > 2000, "{test}: the reference covers {covered} pixels");
        assert!(mean < 0.01, "{test}: mean difference {mean}");
        assert!(worst < 0.2, "{test}: worst difference {worst}");
        let stats = gpu.last_frame_stats().draws;
        assert_eq!(stats.draw_calls_2d, 1, "{test}: both runs' glyphs batch into one draw");
        let (bytes, copied, rebuilds, failure) = gpu.glyph_stats();
        assert!(failure.is_none(), "{test}: {failure:?}");
        assert_eq!((copied, rebuilds), (store.words().len() as u64, 1), "{test}: one upload");
        assert!(bytes >= store.bytes() as u64);
        // The same store again: nothing copied. More glyphs: only the new words.
        gpu.render(target, &frame).unwrap();
        assert_eq!(gpu.glyph_stats().1, copied, "{test}: an unchanged store copies nothing");
        let before = store.words().len();
        let r3 = lay_out(&font, &mut store, "Qz", 40.0, 60.0, 55.0, &mut glyphs);
        let runs3 = [runs[0], runs[1], Draw2d::Text(TextRun { model: IDENTITY, rect: r3, first: glyphs.len() as u32 - 2, count: 2, color: [1.0; 4], layer: 0, lit: false })];
        let frame3 = FrameDesc {
            view_proj: ortho(w as f32, h as f32),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            reverse_z: true,
            draws_2d: &runs3,
            text: TextFrame { store: store.id(), words: store.words(), glyphs: &glyphs },
            ..Default::default()
        };
        gpu.render(target, &frame3).unwrap();
        let (_, copied3, rebuilds3, _) = gpu.glyph_stats();
        assert_eq!(copied3 - copied, (store.words().len() - before) as u64, "{test}: only the new glyphs' words");
        assert_eq!(rebuilds3, 1, "{test}: appended in place");
        finish(&test, gpu);
    }
}

/// Review of cd954bd, major 1: a parked capture uploads the glyph store a piece at a time
/// ahead of its frame (one buffer at the store's cap, then appends), so the frame copies only
/// what is left, never the whole store at once.
#[test]
fn glyph_words_preloaded_in_pieces_leave_the_frame_little_to_copy() {
    let font = Bundled::UiRegular.load().unwrap();
    let test = "text_preload";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let (w, h) = (320u32, 120u32);
    let mut store = SlugStore::new(slug::DEFAULT_STORE_BYTES);
    let mut glyphs = Vec::new();
    let rect = lay_out(&font, &mut store, "Specula Жλ@ Ωmega ß 7 Qz", 30.0, -150.0, 55.0, &mut glyphs);
    let words = store.words().len();
    let piece = words / 3 + 1;
    let reserve = slug::DEFAULT_STORE_BYTES / 4;
    for _ in 0..2 {
        gpu.preload_glyphs(store.id(), store.words(), reserve, piece);
    }
    let (bytes, copied, rebuilds, failure) = gpu.glyph_stats();
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!((copied, rebuilds), ((2 * piece) as u64, 1), "two pieces, one buffer");
    assert!(bytes >= (reserve * 4) as u64, "made at the store's cap");
    let run = [Draw2d::Text(TextRun { model: IDENTITY, rect, first: 0, count: glyphs.len() as u32, color: [1.0; 4], layer: 0, lit: false })];
    let frame = FrameDesc {
        view_proj: ortho(w as f32, h as f32),
        clear_color: [0.0, 0.0, 0.0, 1.0],
        reverse_z: true,
        draws_2d: &run,
        text: TextFrame { store: store.id(), words: store.words(), glyphs: &glyphs },
        ..Default::default()
    };
    let target = gpu.create_target(w, h).unwrap();
    gpu.render(target, &frame).unwrap();
    let (_, copied2, rebuilds2, _) = gpu.glyph_stats();
    assert_eq!((copied2 - copied, rebuilds2), ((words - 2 * piece) as u64, 1), "the frame copies only the rest, in place");
    let rgba = gpu.read_rgba8(target).unwrap();
    assert!(rgba.chunks(4).filter(|p| p[1] > 128).count() > 500, "the text draws");
    finish(test, gpu);
}

#[test]
fn grazing_perspective_text_is_drawn_and_counted_by_sight() {
    let font = Bundled::Display.load().unwrap();
    for adapter in [AdapterPreference::HighPerformance, AdapterPreference::LowPower] {
        let test = format!("text_grazing[{adapter:?}]");
        let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
        let (w, h) = (256u32, 160u32);
        let mut store = SlugStore::new(slug::DEFAULT_STORE_BYTES);
        let mut glyphs = Vec::new();
        // An inscription 4 m long on the ground (the XZ plane), seen from 1.2 m up: a grazing view.
        let rect = lay_out(&font, &mut store, "HIC IACET", 0.5, -1.3, 0.5, &mut glyphs);
        let flat = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let run = Draw2d::Text(TextRun { model: flat, rect, first: 0, count: glyphs.len() as u32, color: [0.93, 0.9, 0.84, 1.0], layer: 0, lit: false });
        let view = mul(&perspective_vk_reverse_z(0.9, w as f32 / h as f32, 0.05), &look_at_rh([0.0, 1.2, 3.0], [0.0, 0.0, -0.5], [0.0, 1.0, 0.0]));
        let runs = [run];
        let frame = FrameDesc {
            view_proj: view,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            reverse_z: true,
            draws_2d: &runs,
            text: TextFrame { store: store.id(), words: store.words(), glyphs: &glyphs },
            ..Default::default()
        };
        let target = gpu.create_target(w, h).unwrap();
        gpu.render_with_ids(target, &frame, &[7]).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        write_png(&format!("text_grazing_{adapter:?}.png"), &rgba, w, h);
        let lit = rgba.as_chunks::<4>().0.iter().filter(|p| p[1] > 128).count();
        let soft = rgba.as_chunks::<4>().0.iter().filter(|p| p[1] > 8 && p[1] <= 128).count();
        assert!(lit > 150, "{test}: {lit} bright pixels");
        assert!(soft > 50 && soft < lit * 3, "{test}: anti-aliased edges, not a smear ({soft} soft, {lit} bright)");
        let counts = gpu.id_coverage(target, 7).unwrap();
        let c = counts.per_id[6];
        if counts.blended_counted {
            assert!(c.blended_pixels as usize >= lit / 2, "{test}: sight counts the text's pixels: {c:?} vs {lit} bright");
            assert_eq!(c.rasterized, Some(true));
        }
        let (_, two_d) = gpu.last_colour_draws();
        assert!(two_d.contains(&0), "{test}: the frame record lists the run");
        finish(&test, gpu);
    }
}

#[test]
fn bad_runs_are_refused_and_no_store_draws_nothing() {
    let Some(mut gpu) = gpu_or_skip("text_bad_runs", AdapterPreference::LowPower) else { return };
    let target = gpu.create_target(64, 64).unwrap();
    let run = TextRun { model: IDENTITY, rect: [0.0, 0.0, 1.0, 1.0], first: 0, count: 3, color: [1.0; 4], layer: 0, lit: false };
    let runs = [Draw2d::Text(run)];
    let frame = FrameDesc { view_proj: ortho(64.0, 64.0), reverse_z: true, draws_2d: &runs, ..Default::default() };
    let e = gpu.render(target, &frame).unwrap_err();
    assert!(e.to_string().contains("text run"), "{e}");
    // Glyphs pointing anywhere in an empty store: nothing resident, nothing drawn, no fault.
    let glyphs = [GlyphInstance { word: 123_456, origin: [0.0; 2], size: 20.0, bounds: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4] }; 3];
    let frame = FrameDesc { text: TextFrame { store: 9, words: &[], glyphs: &glyphs }, ..frame };
    gpu.render(target, &frame).unwrap();
    assert!(gpu.read_rgba8(target).unwrap().as_chunks::<4>().0.iter().all(|p| p[1] == 0));
    // Garbage words: every read is clamped inside the store; whatever shows, the GPU survives.
    let words = vec![0xFFFF_FFFFu32; 64];
    let frame = FrameDesc { text: TextFrame { store: 10, words: &words, glyphs: &glyphs }, ..frame };
    gpu.render(target, &frame).unwrap();
    finish("text_bad_runs", gpu);
}

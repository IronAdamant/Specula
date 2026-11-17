//! The form views (`FormView`): a frame in which a lit surface shows nothing but its shape. Run
//! on every adapter; each test skips cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, Environment, FormView, FrameDesc, Gpu, Vertex};

const SIZE: u32 = 128;

/// A ball of radius 1 about the origin: `rings` bands of `2 * rings` faces, smooth normals.
fn ball(rings: u32) -> (Vec<Vertex>, Vec<u32>) {
    let around = rings * 2;
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for i in 0..=rings {
        let lat = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=around {
            let lon = std::f32::consts::TAU * j as f32 / around as f32;
            let p = [lat.sin() * lon.cos(), lat.cos(), lat.sin() * lon.sin()];
            vertices.push(Vertex::with_color(p, p, [255, 255, 255, 255]));
        }
    }
    for i in 0..rings {
        for j in 0..around {
            let (a, b) = (i * (around + 1) + j, (i + 1) * (around + 1) + j);
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    (vertices, indices)
}

/// A square of side 2 in the plane z = 0, facing +z.
fn wall() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 0.0, 1.0];
    let white = [255, 255, 255, 255];
    let corners = [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
    (corners.iter().map(|p| Vertex::with_color(*p, n, white)).collect(), vec![0, 1, 2, 0, 2, 3])
}

struct Shot<'a> {
    form: FormView,
    scale: f32,
    color: [f32; 4],
    environment: Environment,
    mesh: &'a (Vec<Vertex>, Vec<u32>),
}

/// One frame of one mesh at the origin, seen from +z, as rgba bytes.
fn shoot(gpu: &mut Gpu, shot: &Shot) -> Vec<u8> {
    let mesh = gpu.upload_mesh(&shot.mesh.0, &shot.mesh.1).unwrap();
    let target = gpu.create_target(SIZE, SIZE).unwrap();
    let view_proj = mul(&perspective_vk(40f32.to_radians(), 1.0, 0.1, 20.0), &look_at_rh([0.0, 0.0, 4.0], [0.0; 3], [0.0, 1.0, 0.0]));
    let draws = [DrawItem::new(mesh, common::translation([0.0; 3]), shot.color)];
    let frame = FrameDesc { view_proj, draws: &draws, environment: shot.environment, form: shot.form, form_scale: shot.scale, ..Default::default() };
    gpu.render(target, &frame).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    gpu.destroy_target(target);
    gpu.destroy_mesh(mesh);
    rgba
}

fn grey(rgba: &[u8], x: u32, y: u32) -> u8 {
    let [r, g, b, _] = pixel(rgba, SIZE, x, y);
    assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1, "grey at {x},{y}: {r},{g},{b}");
    g
}

fn on_adapter(adapter: AdapterPreference, label: &str) {
    let test = format!("form_views[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    let (ball, wall) = (ball(48), wall());
    let plain = Environment::default();
    let shot = |form, scale, mesh| Shot { form, scale, color: [1.0; 4], environment: plain, mesh };
    let mid = SIZE / 2;

    // Normals: the facing as colour, exactly. A wall facing the camera is (0, 0, 1); the ball's
    // left, right, top and bottom lean their own ways.
    let rgba = shoot(&mut gpu, &shot(FormView::Normals, 0.05, &wall));
    let [r, g, b, _] = pixel(&rgba, SIZE, mid, mid);
    assert!(r.abs_diff(128) <= 1 && g.abs_diff(128) <= 1 && b == 255, "{test}: a wall facing +z: {r},{g},{b}");
    let rgba = shoot(&mut gpu, &shot(FormView::Normals, 0.05, &ball));
    write_png(&format!("form_normals_{label}.png"), &rgba, SIZE, SIZE);
    let (left, right) = (pixel(&rgba, SIZE, mid - 30, mid), pixel(&rgba, SIZE, mid + 30, mid));
    let (top, bottom) = (pixel(&rgba, SIZE, mid, mid - 30), pixel(&rgba, SIZE, mid, mid + 30));
    assert!(left[0] < 80 && right[0] > 175 && left[1].abs_diff(128) <= 3, "{test}: x as red: {left:?} {right:?}");
    assert!(top[1] > 175 && bottom[1] < 80 && top[0].abs_diff(128) <= 3, "{test}: y as green: {top:?} {bottom:?}");

    // The world's light and the draw's own colour change nothing in a form view.
    let mut dark = plain;
    dark.sun_direction = [0.0, -1.0, 0.0];
    dark.sun_intensity = 3.0;
    dark.ambient_intensity = 0.0;
    dark.exposure = 2.5;
    for form in [FormView::Clay, FormView::Normals, FormView::Curvature, FormView::Stripes] {
        let a = shoot(&mut gpu, &shot(form, 1.0, &ball));
        let b = shoot(&mut gpu, &Shot { form, scale: 1.0, color: [0.9, 0.1, 0.1, 1.0], environment: dark, mesh: &ball });
        let worst = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        assert!(worst <= 1, "{test}: {} is the same under any light and colour (worst byte {worst})", form.name());
    }
    // (And the world is not: the same two frames differ.)
    let a = shoot(&mut gpu, &shot(FormView::World, 1.0, &ball));
    let b = shoot(&mut gpu, &Shot { form: FormView::World, scale: 1.0, color: [0.9, 0.1, 0.1, 1.0], environment: dark, mesh: &ball });
    assert!(a.iter().zip(&b).any(|(x, y)| x.abs_diff(*y) > 40), "{test}: the world's frame follows its light");

    // Clay: lit from the upper left, so the ball is lighter there than at the lower right, and
    // nowhere black or burnt out across its face.
    let rgba = shoot(&mut gpu, &shot(FormView::Clay, 0.05, &ball));
    write_png(&format!("form_clay_{label}.png"), &rgba, SIZE, SIZE);
    let (lit, shade, centre) = (grey_ish(&rgba, mid - 22, mid - 22), grey_ish(&rgba, mid + 22, mid + 22), grey_ish(&rgba, mid, mid));
    assert!(lit > shade + 40 && shade > 30 && lit < 250 && centre > shade && centre < lit, "{test}: clay's light: {lit} {centre} {shade}");

    // Curvature: a flat wall is mid grey; a ball of the scale's radius is halfway from mid
    // grey to white, one of ten times that radius a little lighter than flat, and one a tenth
    // of it nearly white (the turn is eased, never cut off).
    let rgba = shoot(&mut gpu, &shot(FormView::Curvature, 0.05, &wall));
    let flat = grey(&rgba, mid, mid);
    assert!(flat.abs_diff(128) <= 1, "{test}: a flat wall is mid grey: {flat}");
    let rgba = shoot(&mut gpu, &shot(FormView::Curvature, 1.0, &ball));
    write_png(&format!("form_curvature_{label}.png"), &rgba, SIZE, SIZE);
    let full = grey(&rgba, mid, mid);
    assert!((186..=197).contains(&full), "{test}: a ball of the scale's radius is halfway to white: {full}");
    let rgba = shoot(&mut gpu, &shot(FormView::Curvature, 10.0, &ball));
    let fine = grey(&rgba, mid, mid);
    assert!((238..=250).contains(&fine), "{test}: a ball a tenth of the scale's radius is nearly white: {fine}");
    let rgba = shoot(&mut gpu, &shot(FormView::Curvature, 0.1, &ball));
    let tenth = grey(&rgba, mid, mid);
    assert!(tenth > 132 && tenth < 150, "{test}: a ball ten scales wide turns a tenth as fast: {tenth}");

    // Stripes: level bands down the ball, each a clean dark or light, several of them.
    let rgba = shoot(&mut gpu, &shot(FormView::Stripes, 0.05, &ball));
    write_png(&format!("form_stripes_{label}.png"), &rgba, SIZE, SIZE);
    let column: Vec<bool> = (mid - 36..mid + 36).map(|y| grey(&rgba, mid, y) > 128).collect();
    let bands = column.windows(2).filter(|w| w[0] != w[1]).count();
    assert!((6..=40).contains(&bands), "{test}: {bands} band edges down the ball");
    let row: Vec<u8> = (mid - 20..mid + 20).map(|x| grey(&rgba, x, mid - 12)).collect();
    assert!(row.iter().all(|v| *v < 60) || row.iter().all(|v| *v > 200) || row.windows(2).filter(|w| (w[0] > 128) != (w[1] > 128)).count() <= 2, "{test}: bands run level");

    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

/// A pixel's lightness, for a surface that is nearly grey (clay is a warm grey).
fn grey_ish(rgba: &[u8], x: u32, y: u32) -> u8 {
    let [r, g, b, _] = pixel(rgba, SIZE, x, y);
    assert!(r >= g && g >= b && r - b < 40, "clay is a warm grey at {x},{y}: {r},{g},{b}");
    g
}

#[test]
fn a_form_view_shows_shape_alone_on_every_adapter() {
    if !specula_gpu::is_available() {
        eprintln!("a_form_view_shows_shape_alone_on_every_adapter: SKIPPED — Vulkan is not available");
        return;
    }
    let adapters = specula_gpu::enumerate_adapters().expect("enumerate_adapters");
    if adapters.is_empty() {
        eprintln!("a_form_view_shows_shape_alone_on_every_adapter: SKIPPED — no adapters");
    }
    for a in &adapters {
        on_adapter(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

#[test]
fn form_views_have_names() {
    for name in FormView::NAMES {
        assert_eq!(FormView::parse(name).unwrap().name(), name);
    }
    assert_eq!(FormView::parse("wireframe"), None);
    assert_eq!(FormView::default(), FormView::World);
}

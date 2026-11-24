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

    // A form view's pipelines are a set of their own, built on a worker when asked for: a
    // frame never compiles one (the first form capture held the engine thread 440 ms doing
    // so: second review of the form views). Before they exist a frame drawn as a form view
    // draws no mesh; asked for and waited for, it draws, and no pipeline was made on this thread.
    gpu.mark_warm();
    assert!(!gpu.pipeline_variants().form, "{test}: the form set is not built at start");
    let made = gpu.last_frame_stats().pipelines.created;
    let rgba = shoot(&mut gpu, &shot(FormView::Normals, 0.05, &wall));
    let early = gpu.last_frame_stats();
    assert_eq!(pixel(&rgba, SIZE, mid, mid), pixel(&rgba, SIZE, 2, 2), "{test}: no mesh is drawn before the form set exists");
    assert_eq!((early.pipelines.created, early.pipelines.created_after_warm), (made, 0), "{test}: the frame compiled nothing");
    gpu.request_form_pipelines();
    // (Until the worker ends: a join, not a deadline.)
    let v = gpu.wait_for_pipelines();
    assert!(v.form && v.failed.is_none(), "{test}: the form set is built: {v:?}");
    gpu.request_form_pipelines(); // (again: nothing to do)

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

    // What the review of the form views found, each as it should be:
    // Clay's light comes with the camera whichever way it looks. Two balls seen from straight
    // above are lit alike (taken from the world's up and each pixel's own direction, each was
    // lit from another side): each is lighter toward the picture's upper left than its lower right.
    let ball_mesh = gpu.upload_mesh(&ball.0, &ball.1).unwrap();
    let target = gpu.create_target(SIZE, SIZE).unwrap();
    let down = mul(&perspective_vk(40f32.to_radians(), 1.0, 0.1, 30.0), &look_at_rh([0.0, 9.0, 0.0], [0.0; 3], [0.0, 0.0, -1.0]));
    let pair = [DrawItem::new(ball_mesh, common::translation([-1.6, 0.0, 0.0]), [1.0; 4]), DrawItem::new(ball_mesh, common::translation([1.6, 0.0, 0.0]), [1.0; 4])];
    gpu.render(target, &FrameDesc { view_proj: down, draws: &pair, form: FormView::Clay, ..Default::default() }).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    write_png(&format!("form_clay_down_{label}.png"), &rgba, SIZE, SIZE);
    for x in [-1.6f32, 1.6] {
        let (cx, cy) = project(&down, [x, 0.0, 0.0], SIZE, SIZE);
        let (cx, cy) = (cx.round() as u32, cy.round() as u32);
        let (lit, shade) = (grey_ish(&rgba, cx - 8, cy - 8), grey_ish(&rgba, cx + 8, cy + 8));
        assert!(lit > shade + 40, "{test}: the ball at x = {x}, seen from above, is lit from the upper left: {lit} against {shade}");
    }
    // A ball turned inside out (every normal pointing in) is not the same picture as the right
    // one: clay stains it red, and its normals are the opposite colours.
    let inside_out: Vec<Vertex> = ball.0.iter().map(|v| Vertex::with_color(v.position, [-v.normal[0], -v.normal[1], -v.normal[2]], [255; 4])).collect();
    let wrong = (inside_out, ball.1.clone());
    let rgba = shoot(&mut gpu, &shot(FormView::Clay, 0.05, &wrong));
    let [r, g, b, _] = pixel(&rgba, SIZE, mid - 10, mid - 10);
    assert!(r > 90 && r > g.saturating_add(50) && r > b.saturating_add(50), "{test}: an inside-out ball is stained red in clay: {r},{g},{b}");
    let [r, g, b, _] = pixel(&shoot(&mut gpu, &shot(FormView::Clay, 0.05, &ball)), SIZE, mid - 10, mid - 10);
    assert!(r < g.saturating_add(30), "{test}: and a right one is not: {r},{g},{b}");
    let right = shoot(&mut gpu, &shot(FormView::Normals, 0.05, &ball));
    let turned = shoot(&mut gpu, &shot(FormView::Normals, 0.05, &wrong));
    let (a, b) = (pixel(&right, SIZE, mid + 30, mid), pixel(&turned, SIZE, mid + 30, mid));
    assert!(a[0] > 175 && b[0] < 80, "{test}: its normals show as its own, pointing in: {a:?} against {b:?}");
    // Curvature is of smooth shading: the same ball shaded flat (each face its own normal)
    // reads flat, and normals shows its faces as steps of colour. (Said so in the views'
    // description: a hard edge is looked for in normals and in the asset's form facts.)
    let coarse = self::ball(6);
    let mut faces = (Vec::new(), Vec::new());
    for t in coarse.1.chunks(3) {
        let p = [coarse.0[t[0] as usize].position, coarse.0[t[1] as usize].position, coarse.0[t[2] as usize].position];
        let n = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])));
        // (The ball's triangles wind inward-facing first: take the normal that points out.)
        let n = if dot(n, p[0]) + dot(n, p[1]) + dot(n, p[2]) < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
        for q in p {
            faces.1.push(faces.0.len() as u32);
            faces.0.push(Vertex::with_color(q, n, [255; 4]));
        }
    }
    let rgba = shoot(&mut gpu, &shot(FormView::Curvature, 1.0, &faces));
    assert!(grey(&rgba, mid + 4, mid + 3).abs_diff(128) <= 1, "{test}: a flat-shaded ball reads flat in curvature");
    let rgba = shoot(&mut gpu, &shot(FormView::Normals, 1.0, &faces));
    let row: Vec<[u8; 4]> = (mid - 30..mid + 30).map(|x| pixel(&rgba, SIZE, x, mid + 3)).collect();
    let steps = row.windows(2).filter(|w| w[0][0].abs_diff(w[1][0]) > 12).count();
    let flats = row.windows(2).filter(|w| w[0] == w[1]).count();
    assert!((2..=8).contains(&steps) && flats > 40, "{test}: its faces are steps of colour in normals: {steps} steps, {flats} of 59 neighbours alike");
    // A shadow's disc is no surface: a form view leaves it out.
    let quad = (vec![[-1.0f32, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, -1.0], [-1.0, 0.0, -1.0]].into_iter().map(|p| Vertex::with_color(p, [0.0, 1.0, 0.0], [255; 4])).collect::<Vec<_>>(), vec![0u32, 1, 2, 0, 2, 3]);
    let quad_mesh = gpu.upload_mesh(&quad.0, &quad.1).unwrap();
    let mut disc = DrawItem::new(quad_mesh, common::translation([0.0; 3]), [0.0, 0.0, 0.0, 0.8]);
    disc.material = specula_gpu::Material::BlobShadow;
    let clear = [0.2f32, 0.4, 0.6, 1.0];
    for (form, drawn) in [(FormView::World, true), (FormView::Normals, false)] {
        gpu.render(target, &FrameDesc { view_proj: down, draws: std::slice::from_ref(&disc), clear_color: clear, form, ..Default::default() }).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        let (centre, corner) = (pixel(&rgba, SIZE, mid, mid), pixel(&rgba, SIZE, 2, 2));
        assert_eq!(centre != corner, drawn, "{test}: a shadow's disc in {}: {centre:?} against {corner:?}", form.name());
    }
    gpu.destroy_target(target);

    // Under an orthographic camera every pixel is seen along the camera's own direction: a
    // ball's outline is not stained as facing away (taken from the camera's place, the middle
    // of its near plane, a ring of 258 pixels round a correct ball was red: second review).
    let target = gpu.create_target(SIZE, SIZE).unwrap();
    let mut ortho = [0.0f32; 16];
    // (Vulkan clip space: x and y of +-1.5 m to +-1, y down; depth 0 at 0.1 m, 1 at 20 m.)
    (ortho[0], ortho[5], ortho[10], ortho[14], ortho[15]) = (1.0 / 1.5, -1.0 / 1.5, -1.0 / 19.9, -0.1 / 19.9, 1.0);
    let flat = mul(&ortho, &look_at_rh([0.0, 0.0, 4.0], [0.0; 3], [0.0, 1.0, 0.0]));
    let coarse = self::ball(6);
    for mesh in [&ball, &coarse] {
        let id = gpu.upload_mesh(&mesh.0, &mesh.1).unwrap();
        let draws = [DrawItem::new(id, common::translation([0.0; 3]), [1.0; 4])];
        gpu.render(target, &FrameDesc { view_proj: flat, draws: &draws, form: FormView::Clay, ..Default::default() }).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        write_png(&format!("form_clay_ortho_{label}_{}.png", mesh.1.len()), &rgba, SIZE, SIZE);
        // (Clay is a warm grey, red over green about 1.06 as the eye's values; the stain, 1.8.)
        let red = rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 40 && u32::from(p[0]) * 10 > u32::from(p[1]) * 14 + 100).count();
        let drawn = rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 60).count();
        assert!(drawn > 4_000 && red == 0, "{test}: a ball of {} triangles under an orthographic camera: {red} of its {drawn} pixels are stained", mesh.1.len() / 3);
        gpu.destroy_mesh(id);
    }
    gpu.destroy_target(target);

    // And through all of it, no frame compiled a pipeline.
    let s = gpu.last_frame_stats();
    assert_eq!(s.pipelines.created_after_warm, 0, "{test}: {:?}", s.pipelines);

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

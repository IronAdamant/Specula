//! The bake's plan, worked out on the caller's thread before the engine renders anything: which
//! clips and frames, which directions, the orthographic camera fitted to every posed vertex of
//! every frame from every direction (one scale and one pivot for the whole sheet), the atlas's
//! layout and its ceilings. Then, per rendered cell, the pure steps after the GPU: alpha from the
//! ID buffer, the supersampled resolve, the pixel-art palette snap and the colour bleed.
//!
//! Nothing here touches the engine, the GPU or the disk: every function is a test away.

use crate::models::Built;
use specula_anim::math::Affine;
use specula_anim::{blend, AnimSet, LoopMode};
use specula_core::math::{Mat4, Vec3};

/// Cell sides in pixels (each of width and height).
pub const MIN_CELL_PX: u32 = 8;
pub const MAX_CELL_PX: u32 = 512;
/// The most pixels a side one cell renders at (the cell times the supersample): one render
/// and its readback is one step on the engine thread.
pub const MAX_RENDER_SIDE: u32 = 512;
/// Directions one bake takes at most.
pub const MAX_DIRECTIONS: usize = 16;
/// Clips one bake takes at most.
pub const MAX_CLIPS: usize = 16;
/// Frames one clip gives at most (a longer clip at the rate asked is refused, with the frames
/// it would have been).
pub const MAX_FRAMES_PER_CLIP: u32 = 64;
/// The highest frame rate.
pub const MAX_FPS: f32 = 60.0;
/// Cells (renders) one bake makes at most: a call answers within the engine's call deadline.
pub const MAX_CELLS: usize = 512;
/// The atlas: at most this many pixels a side and in all. 7 Mi pixels is 28 MiB of RGBA, so
/// even a PNG that does not compress at all stays under a binary project file's 32 MiB.
pub const MAX_ATLAS_SIDE: u32 = 4096;
pub const MAX_ATLAS_PIXELS: u64 = 7 << 20;
/// Transparent pixels kept round the model inside every cell (so a filtered sprite never reads
/// its neighbour's pixels, and a limb at the extreme of its swing is inside, not on the edge).
pub const MARGIN_PX: u32 = 2;
/// How far the colour of the model's edge is carried into the transparent pixels around it
/// (alpha stays 0 there): a sprite drawn with linear filtering then shows no dark fringe.
pub const BLEED_PX: u32 = 2;

/// The camera's elevation by name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Level with the model (a platformer's side view).
    Side,
    /// 30 degrees down (the usual three-quarter 2D view).
    ThreeQuarter,
    /// Straight down.
    Top,
}

impl View {
    pub const ALL: [(&'static str, View); 3] = [("side", View::Side), ("three_quarter", View::ThreeQuarter), ("top", View::Top)];

    pub fn name(self) -> &'static str {
        View::ALL.iter().find(|v| v.1 == self).map_or("?", |v| v.0)
    }

    pub fn elevation_deg(self) -> f32 {
        match self {
            View::Side => 0.0,
            View::ThreeQuarter => 30.0,
            View::Top => 90.0,
        }
    }
}

/// One direction the model is shown facing, on the screen: 0 degrees faces the viewer ("s"),
/// 90 faces right ("e"), 180 away ("n"), 270 left ("w"). The model's own front is +Z.
#[derive(Clone, Debug, PartialEq)]
pub struct Direction {
    pub name: String,
    pub facing_deg: f32,
}

/// The sixteen compass names, from facing the viewer round through facing right.
const WINDS: [&str; 16] = ["s", "sse", "se", "ese", "e", "ene", "ne", "nne", "n", "nnw", "nw", "wnw", "w", "wsw", "sw", "ssw"];

/// `n` directions evenly round (1, 4, 8 or 16), the first facing right when there is one
/// (`n` = 1: a side-scroller's one direction) and facing the viewer otherwise.
pub fn directions_around(n: u32) -> Vec<Direction> {
    if n == 1 {
        return vec![Direction { name: "e".into(), facing_deg: 90.0 }];
    }
    let step = 16 / n as usize;
    (0..n as usize).map(|i| Direction { name: WINDS[i * step].into(), facing_deg: 360.0 * i as f32 / n as f32 }).collect()
}

/// A direction at any facing: its compass name when it is one of the sixteen, else its degrees.
pub fn direction_at(facing_deg: f32) -> Direction {
    let f = facing_deg.rem_euclid(360.0);
    let k = f / 22.5;
    let name = if (k - k.round()).abs() < 1e-4 { WINDS[(k.round() as usize) % 16].to_owned() } else { format!("{}deg", (f * 10.0).round() / 10.0) };
    Direction { name, facing_deg: f }
}

/// The camera for a direction at an elevation: where it looks from (unit), and the screen's
/// right and up in model space. The model's front (+Z) faces `facing_deg` on the screen.
pub fn camera_axes(facing_deg: f32, elevation_deg: f32) -> (Vec3, Vec3, Vec3) {
    // The camera's azimuth about +Y is minus the facing (facing right puts the camera on -X).
    let (a, e) = ((-facing_deg).to_radians(), elevation_deg.clamp(0.0, 90.0).to_radians());
    let d = Vec3::new(e.cos() * a.sin(), e.sin(), e.cos() * a.cos());
    let right = Vec3::new(a.cos(), 0.0, -a.sin());
    let up = d.cross(right).normalize();
    (d, right, up)
}

/// How the frames of a clip are chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rate {
    /// So many frames over the clip, whatever its length.
    Frames(u32),
    /// So many frames a second (a clip's frames are its duration times this, at least 1).
    Fps(f32),
}

/// The clips asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Clips {
    /// A still: the model as it stands (bind pose).
    Still,
    /// Every clip of the model.
    All,
    Named(Vec<String>),
}

/// A foot is down in a frame when it is within this of its ground (its clip's 15th-percentile
/// height, as `specula_anim::measure::foot_contact` finds it).
pub const FOOT_DOWN_M: f32 = 0.02;
/// Samples over a clip that find a foot's ground.
const FOOT_SAMPLES: usize = 64;

/// One strip: a clip (or the still) from one direction, its frames consecutive in the atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    /// `None`: the still.
    pub clip: Option<String>,
    pub direction: usize,
    /// The sheet frame index (left to right, top to bottom) of its first frame.
    pub first: u32,
    pub count: u32,
    pub fps: f32,
    pub looping: bool,
    pub duration_s: f32,
    /// Metres the clip travels a cycle (its `distance`; 0 in place).
    pub distance_m: f32,
    /// Events of the clip: (name, the frame showing it).
    pub events: Vec<(String, u32)>,
    /// The clip's feet (as `anim_check` finds them: a graph's `feet`, the clip's footstep bones,
    /// or bones named foot*; empty: none) and, per frame, which are down (indices into `feet`).
    pub feet: Vec<String>,
    pub feet_down: Vec<Vec<u8>>,
    /// Index of its first pose in [`Plan::poses`] (its frames' poses follow in order).
    pub pose0: usize,
}

/// One cell of the atlas: what it shows and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub strip: usize,
    pub frame: u32,
    /// `[x, y, w, h]` in the atlas.
    pub rect: [u32; 4],
    pub time_s: f32,
    pub pose: usize,
}

/// The orthographic camera of one direction (column-major view-projection).
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub view_proj: [f32; 16],
    pub eye: Vec3,
    /// The camera's azimuth (degrees about +Y, 0 = on +Z): the light turns with it.
    pub azimuth_deg: f32,
}

/// Everything the engine needs to render the sheet, and what the frame data says of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub cell: [u32; 2],
    pub supersample: u32,
    pub columns: u32,
    pub atlas: [u32; 2],
    pub metres_per_pixel: f32,
    /// The model's origin on its ground, in every cell (pixels from the cell's top left).
    pub pivot: [u32; 2],
    pub directions: Vec<Direction>,
    pub elevation_deg: f32,
    pub strips: Vec<Strip>,
    pub cells: Vec<Cell>,
    pub cameras: Vec<Camera>,
    /// Skinning matrices per pose (empty for a model without a rig).
    pub poses: Vec<Vec<Affine>>,
    /// The fitted extent over every pose and direction (screen metres, from the pivot):
    /// left, right, bottom, top.
    pub extent_m: [f32; 4],
    /// How far each clip's root wanders sideways over its frames (metres; 0 = in place), as
    /// the clip has it (with `in_place` it was then taken out).
    pub root_drift_m: Vec<(String, f32)>,
    /// The shadow blob's cell, when asked for.
    pub shadow: Option<Shadow>,
}

/// The soft shadow blob: one cell of its own after the strips, alpha only, an ellipse centred
/// on the pivot (the model's footprint seen at the sheet's elevation).
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    /// Its sheet frame index and its rectangle in the atlas.
    pub frame: u32,
    pub rect: [u32; 4],
    /// Its own pivot (its middle), in pixels from its cell's top left: drawn there, it is under
    /// the model's pivot.
    pub pivot: [u32; 2],
    /// The ellipse's half-width and half-height in pixels.
    pub radii_px: [f32; 2],
}

/// How dark the blob is at its middle (its alpha there is this times 255).
pub const SHADOW_ALPHA: f32 = 0.6;

/// The blob's pixels for a cell: black, alpha falling smoothly from [`SHADOW_ALPHA`] at the
/// pivot to 0 at the ellipse's edge; pixel art: one alpha, half of it, inside 85% of the ellipse.
pub fn shadow_blob(cell: [u32; 2], pivot: [u32; 2], radii: [f32; 2], pixel_art: bool) -> Vec<u8> {
    let (w, h) = (cell[0] as usize, cell[1] as usize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - pivot[0] as f32) / radii[0].max(0.5);
            let dy = (y as f32 + 0.5 - pivot[1] as f32) / radii[1].max(0.5);
            let d2 = dx * dx + dy * dy;
            let a = if pixel_art {
                if d2 < 0.85 * 0.85 {
                    128
                } else {
                    0
                }
            } else if d2 < 1.0 {
                (SHADOW_ALPHA * (1.0 - d2) * (1.0 - d2) * 255.0).round() as u8
            } else {
                0
            };
            out[(y * w + x) * 4 + 3] = a;
        }
    }
    out
}

/// One palette colour drawn round the silhouette (pixel art's outline): every transparent pixel
/// with a visible pixel beside it (left, right, above, below) takes `colour`, fully opaque.
/// Pixels within `margin` of the cell's edge are never drawn on (the fit keeps them clear).
pub fn outline(rgba: &mut [u8], cell: [u32; 2], colour: [u8; 3], margin: u32) {
    let (w, h, m) = (cell[0] as i64, cell[1] as i64, i64::from(margin));
    let seen: Vec<bool> = rgba.chunks_exact(4).map(|p| p[3] > 0).collect();
    for y in m..h - m {
        for x in m..w - m {
            let i = (y * w + x) as usize;
            if seen[i] {
                continue;
            }
            let beside = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                nx >= 0 && ny >= 0 && nx < w && ny < h && seen[(ny * w + nx) as usize]
            });
            if beside {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[colour[0], colour[1], colour[2], 255]);
            }
        }
    }
}

/// The darkest colour of a palette (Oklab lightness): pixel art's outline colour.
pub fn darkest(palette: &[[u8; 3]]) -> Option<[u8; 3]> {
    palette.iter().copied().min_by(|a, b| {
        let l = |c: [u8; 3]| specula_studio::colour::oklab(specula_studio::colour::from8(c))[0];
        l(*a).total_cmp(&l(*b))
    })
}

/// Which of a clip's `feet` are down at each of `times` (indices into `feet`): a foot within
/// [`FOOT_DOWN_M`] of its ground, its 15th-percentile height over the clip.
fn feet_down(set: &AnimSet, id: specula_anim::ClipId, feet: &[usize], times: &[f32], in_place_root: Option<[f32; 3]>) -> Vec<Vec<u8>> {
    let c = set.clip(id);
    let height = |t: f32| -> Vec<f32> {
        let mut p = set.skeleton.rest_pose();
        blend::sample(&set.skeleton, c, set.track_bones(id), t, blend::Base::Rest, &mut p);
        if let (Some(r), Some(root)) = (in_place_root, p.local.first_mut()) {
            root.t[0] = r[0];
            root.t[2] = r[2];
        }
        let mut model = Vec::new();
        set.skeleton.model_space(&p, &mut model);
        feet.iter().map(|&f| model.get(f).map_or(0.0, |m| m.t[1])).collect()
    };
    let track: Vec<Vec<f32>> = (0..=FOOT_SAMPLES).map(|k| height(c.duration * k as f32 / FOOT_SAMPLES as f32)).collect();
    let ground: Vec<f32> = (0..feet.len())
        .map(|f| {
            let mut ys: Vec<f32> = track.iter().map(|h| h[f]).collect();
            ys.sort_by(f32::total_cmp);
            ys[ys.len() * 15 / 100]
        })
        .collect();
    times.iter().map(|&t| height(t).iter().enumerate().filter(|(f, y)| (**y - ground[*f]).abs() <= FOOT_DOWN_M).map(|(f, _)| f as u8).collect()).collect()
}

/// What was asked, as far as the plan needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub clips: Clips,
    pub directions: Vec<Direction>,
    pub elevation_deg: f32,
    pub rate: Rate,
    pub cell: [u32; 2],
    pub supersample: u32,
    /// Each frame's root put back over the origin (its sideways wander taken out): a 2D game
    /// moves the sprite itself.
    pub in_place: bool,
    /// A soft shadow blob in a cell of its own (alpha only, centred on the pivot).
    pub shadow_blob: bool,
}

/// Why a plan was refused, before anything was rendered: a code, words, and the numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
    pub would_be: Option<[u64; 3]>,
}

fn refuse(code: &'static str, message: String) -> Refusal {
    Refusal { code, message, would_be: None }
}

/// The frame times of a clip: a loop's `n` frames split its cycle (the last frame is not the
/// first again), a clip that plays once shows its first and last pose.
pub fn frame_times(duration: f32, looping: bool, n: u32) -> Vec<f32> {
    let n = n.max(1);
    if n == 1 {
        return vec![0.0];
    }
    let span = if looping { n as f32 } else { (n - 1) as f32 };
    (0..n).map(|k| duration * k as f32 / span).collect()
}

/// Frames of a clip at a rate, or the refusal: a clip longer than the ceiling allows.
pub fn frames_for(clip: &str, duration: f32, rate: Rate) -> Result<u32, Refusal> {
    let n = match rate {
        Rate::Frames(n) => n,
        Rate::Fps(fps) => {
            let want = (duration.max(0.0) * fps).round();
            if want > MAX_FRAMES_PER_CLIP as f32 {
                return Err(refuse(
                    "clip_too_long",
                    format!("clip '{clip}' is {duration:.3} s: at {fps} frames a second that is {want} frames, and a clip gives at most {MAX_FRAMES_PER_CLIP} (a lower fps, or frames: N)"),
                ));
            }
            want.max(1.0) as u32
        }
    };
    Ok(n.clamp(1, MAX_FRAMES_PER_CLIP))
}

/// The atlas's columns, rows and size for strips of `counts` frames of `cell` pixels: each strip
/// begins a row and wraps onto the next ones, so the sheet frames of a strip are consecutive
/// (the 2D path's `sheet` + `anim {first, count}` plays them as they are). The refusal names the
/// size it would have been.
pub fn layout(counts: &[u32], cell: [u32; 2]) -> Result<(u32, Vec<u32>, [u32; 2]), Refusal> {
    let widest = counts.iter().copied().max().unwrap_or(1).max(1);
    let columns = widest.min((MAX_ATLAS_SIDE / cell[0]).max(1));
    let mut firsts = Vec::with_capacity(counts.len());
    let mut rows = 0u32;
    for &n in counts {
        firsts.push(rows * columns);
        rows += n.max(1).div_ceil(columns);
    }
    let (w, h) = (u64::from(columns) * u64::from(cell[0]), u64::from(rows) * u64::from(cell[1]));
    let px = w * h;
    if h > u64::from(MAX_ATLAS_SIDE) || px > MAX_ATLAS_PIXELS {
        return Err(Refusal {
            code: "atlas_too_large",
            message: format!(
                "the atlas would be {w}x{h} pixels ({px} pixels, {} bytes of RGBA); at most {MAX_ATLAS_SIDE} a side and {MAX_ATLAS_PIXELS} pixels: fewer clips, directions or frames, or a smaller cell",
                px * 4
            ),
            would_be: Some([w, h, px * 4]),
        });
    }
    Ok((columns, firsts, [w as u32, h as u32]))
}

/// Skinning matrices of a pose.
fn skin_of(set: &AnimSet, pose: &specula_anim::Pose) -> Vec<Affine> {
    let (mut model, mut skin) = (Vec::new(), Vec::new());
    set.skeleton.model_space(pose, &mut model);
    set.skeleton.skin_matrices(&model, &mut skin);
    skin
}

/// The positions of LOD 0 posed by `skin` (unchanged without skinning data or a skin).
fn posed_points(built: &Built, skin: &[Affine], out: &mut Vec<[f32; 3]>) {
    out.clear();
    let Some(mesh) = built.model.lods.first() else { return };
    let skinned = !skin.is_empty() && mesh.joints.len() == mesh.positions.len() && mesh.weights.len() == mesh.positions.len();
    for (i, &p) in mesh.positions.iter().enumerate() {
        if !skinned {
            out.push(p);
            continue;
        }
        let (j, w) = (mesh.joints[i], mesh.weights[i]);
        let mut q = [0.0f32; 3];
        for k in 0..4 {
            if w[k] == 0 {
                continue;
            }
            let m = skin.get(j[k] as usize).copied().unwrap_or(Affine::IDENTITY);
            let r = m.apply_point(p);
            let f = f32::from(w[k]) / 255.0;
            for a in 0..3 {
                q[a] += r[a] * f;
            }
        }
        out.push(q);
    }
}

/// The scale (metres per pixel) and pivot (pixels from a cell's top left) that keep a screen
/// extent `[left, right, bottom, top]` (metres from the pivot) inside a cell with `margin`
/// pixels clear on every side. The pivot is a whole pixel corner, the same in every cell.
pub fn fit(extent: [f32; 4], cell: [u32; 2], margin: u32) -> (f32, [u32; 2]) {
    let [l, r, b, t] = extent;
    // Two spare pixels a side's span: rounding the pivot to a whole pixel never eats the margin.
    let room = |side: u32| (side.saturating_sub(2 * margin + 2)).max(1) as f32;
    let s = ((r - l).max(1e-6) / room(cell[0])).max((t - b).max(1e-6) / room(cell[1]));
    let place = |lo: f32, hi: f32, side: u32| -> u32 {
        let span = (hi - lo) / s;
        let slack = side as f32 - 2.0 * margin as f32 - span;
        ((margin as f32 - lo / s).ceil() + (slack / 2.0 - 1.0).max(0.0).floor()) as u32
    };
    // x: the pivot sits `-left / s` pixels in from the content's left; y counts down from the top.
    let px = place(l, r, cell[0]);
    let py = place(-t, -b, cell[1]);
    (s, [px, py])
}

/// The orthographic camera of a direction, from the scale and pivot of the whole sheet.
pub fn camera(dir: &Direction, elevation_deg: f32, s: f32, pivot: [u32; 2], cell: [u32; 2], radius: f32) -> Camera {
    let (d, _right, up) = camera_axes(dir.facing_deg, elevation_deg);
    let dist = radius + 1.0;
    let eye = d * dist;
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, up);
    let (left, right) = (-(pivot[0] as f32) * s, (cell[0] - pivot[0]) as f32 * s);
    let (top, bottom) = (pivot[1] as f32 * s, -((cell[1] - pivot[1]) as f32) * s);
    let proj = Mat4::orthographic_vk_reverse_z(left, right, bottom, top, (dist - radius - 0.5).max(0.01), dist + radius + 0.5);
    Camera { view_proj: (proj * view).to_cols_array(), eye, azimuth_deg: -dir.facing_deg }
}

/// The plan for `built` (its anim set, when it has one: `set`), or why not.
pub fn plan(built: &Built, set: Option<&AnimSet>, ask: &Ask) -> Result<Plan, Refusal> {
    if built.model.lods.first().is_none_or(|m| m.positions.is_empty()) {
        return Err(refuse("empty_model", format!("{} has no vertices to draw", built.model.name)));
    }
    if ask.directions.is_empty() || ask.directions.len() > MAX_DIRECTIONS {
        return Err(refuse("invalid_argument", format!("1 to {MAX_DIRECTIONS} directions")));
    }
    // The clips, each with its frames.
    struct Want {
        clip: Option<String>,
        times: Vec<f32>,
        fps: f32,
        looping: bool,
        duration: f32,
        distance: f32,
        events: Vec<(String, f32)>,
        feet: Vec<String>,
        feet_down: Vec<Vec<u8>>,
    }
    let names: Vec<String> = match (&ask.clips, set) {
        (Clips::Still, _) => Vec::new(),
        (Clips::All, Some(set)) => set.clips().map(|(_, c)| c.name.clone()).collect(),
        (Clips::All, None) => Vec::new(),
        (Clips::Named(n), None) => {
            return Err(refuse("no_clips", format!("{} has no clips (no rig or no animation set): leave 'clips' out for a still; asked for {}", built.model.name, n.join(", "))))
        }
        (Clips::Named(n), Some(_)) => n.clone(),
    };
    if names.len() > MAX_CLIPS {
        return Err(refuse("too_many_clips", format!("{} clips; at most {MAX_CLIPS} a bake (bake the rest into another sheet)", names.len())));
    }
    let mut wants: Vec<Want> = Vec::new();
    if names.is_empty() {
        wants.push(Want { clip: None, times: vec![0.0], fps: 1.0, looping: false, duration: 0.0, distance: 0.0, events: Vec::new(), feet: Vec::new(), feet_down: Vec::new() });
    }
    let rest_root: Option<[f32; 3]> = set.and_then(|s| s.skeleton.rest_pose().local.first().map(|t| t.t));
    let in_place_root = if ask.in_place { rest_root } else { None };
    for name in &names {
        let set = set.expect("clips come from a set");
        let Some(id) = set.id(name) else {
            let known: Vec<&str> = set.clips().map(|(_, c)| c.name.as_str()).take(24).collect();
            return Err(refuse("unknown_clip", format!("{} has no clip '{name}' (clips: {})", built.model.name, known.join(", "))));
        };
        let c = set.clip(id);
        let looping = c.looping == LoopMode::Loop;
        let n = frames_for(name, c.duration, ask.rate)?;
        let times = frame_times(c.duration, looping, n);
        let span = if looping { n as f32 } else { (n.max(2) - 1) as f32 };
        let fps = if c.duration > 0.0 && n > 1 { span / c.duration } else { 1.0 };
        let events = c.events.iter().map(|e| (e.name.clone(), e.time)).collect();
        // The feet as anim_check finds them (a graph's `feet`, else the clip's footstep bones,
        // else bones named foot*), and which are down in each frame.
        let graph = set.graphs().map(|(_, g)| g).find(|g| !g.feet.is_empty());
        let feet = specula_anim::checks::feet_of(set, graph, Some(id));
        let feet_down = if feet.is_empty() { Vec::new() } else { feet_down(set, id, &feet, &times, in_place_root) };
        let feet = feet.iter().map(|&b| set.skeleton.names.get(b).cloned().unwrap_or_else(|| format!("bone{b}"))).collect();
        wants.push(Want { clip: Some(name.clone()), times, fps, looping, duration: c.duration, distance: c.distance, events, feet, feet_down });
    }
    let cells_total: usize = wants.iter().map(|w| w.times.len()).sum::<usize>() * ask.directions.len();
    if cells_total > MAX_CELLS {
        return Err(Refusal {
            code: "too_many_cells",
            message: format!("{} clip(s) x {} direction(s) is {cells_total} cells; at most {MAX_CELLS} a bake (fewer frames, clips or directions, or several sheets)", wants.len(), ask.directions.len()),
            would_be: None,
        });
    }
    let mut counts: Vec<u32> = ask.directions.iter().flat_map(|_| wants.iter().map(|w| w.times.len() as u32)).collect();
    if ask.shadow_blob {
        // The blob: one cell of its own, beginning the last row.
        counts.push(1);
    }
    let (columns, firsts, atlas) = layout(&counts, ask.cell)?;

    // Poses: per clip frame (directions share them).
    let mut poses: Vec<Vec<Affine>> = Vec::new();
    let mut pose0 = Vec::new();
    let mut root_drift_m = Vec::new();
    let bind: Vec<Affine> = match (set, built.model.rig.as_ref()) {
        (Some(set), _) => skin_of(set, &set.skeleton.rest_pose()),
        (None, Some(rig)) => vec![Affine::IDENTITY; rig.len()],
        (None, None) => Vec::new(),
    };
    for w in &wants {
        pose0.push(poses.len());
        let (Some(name), Some(set)) = (&w.clip, set) else {
            poses.push(bind.clone());
            continue;
        };
        let id = set.id(name).expect("checked");
        let c = set.clip(id);
        let mut drift = 0.0f32;
        let mut first_root: Option<[f32; 3]> = None;
        for &t in &w.times {
            let mut p = set.skeleton.rest_pose();
            blend::sample(&set.skeleton, c, set.track_bones(id), t, blend::Base::Rest, &mut p);
            let mut model = Vec::new();
            set.skeleton.model_space(&p, &mut model);
            if let Some(root) = model.first() {
                let r = root.t;
                let f = *first_root.get_or_insert(r);
                drift = drift.max(((r[0] - f[0]).powi(2) + (r[2] - f[2]).powi(2)).sqrt());
            }
            // In place: the root over its rest place sideways (its height kept).
            if let (Some(r), Some(root)) = (in_place_root, p.local.first_mut()) {
                root.t[0] = r[0];
                root.t[2] = r[2];
            }
            poses.push(skin_of(set, &p));
        }
        root_drift_m.push((name.clone(), drift));
    }

    // The extent: every posed vertex of every pose, from every direction.
    let axes: Vec<(Vec3, Vec3)> = ask.directions.iter().map(|d| {
        let (_, r, u) = camera_axes(d.facing_deg, ask.elevation_deg);
        (r, u)
    }).collect();
    let mut ext = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
    let mut radius = 0.0f32;
    let mut pts = Vec::new();
    // The footprint (for the shadow blob): the first pose's vertices in its lowest fifth of
    // height, their farthest from the vertical axis.
    let mut footprint = 0.0f32;
    for (k, skin) in poses.iter().enumerate() {
        posed_points(built, skin, &mut pts);
        if k == 0 && ask.shadow_blob {
            let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
            let band = lo + (hi - lo) * 0.2;
            footprint = pts.iter().filter(|p| p[1] <= band).map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt()).fold(0.0, f32::max);
        }
        for p in &pts {
            let v = Vec3::new(p[0], p[1], p[2]);
            radius = radius.max(v.length());
            for (r, u) in &axes {
                let (x, y) = (v.dot(*r), v.dot(*u));
                ext = [ext[0].min(x), ext[1].max(x), ext[2].min(y), ext[3].max(y)];
            }
        }
    }
    if !ext.iter().all(|x| x.is_finite()) {
        return Err(refuse("empty_model", format!("{} has no finite vertices to frame", built.model.name)));
    }
    // The pivot (the origin) is always inside the frame: a sprite's ground point is on it.
    ext = [ext[0].min(0.0), ext[1].max(0.0), ext[2].min(0.0), ext[3].max(0.0)];
    let (s, pivot) = fit(ext, ask.cell, MARGIN_PX);
    let cameras = ask.directions.iter().map(|d| camera(d, ask.elevation_deg, s, pivot, ask.cell, radius.max(0.05))).collect();

    let mut strips = Vec::new();
    let mut cells = Vec::new();
    let mut k = 0usize;
    for (di, _) in ask.directions.iter().enumerate() {
        for (wi, w) in wants.iter().enumerate() {
            let first = firsts[k];
            k += 1;
            let frame_of = |t: f32| if w.times.len() <= 1 || w.duration <= 0.0 { 0 } else { ((t * w.fps).floor() as u32).min(w.times.len() as u32 - 1) };
            strips.push(Strip {
                clip: w.clip.clone(),
                direction: di,
                first,
                count: w.times.len() as u32,
                fps: w.fps,
                looping: w.looping,
                duration_s: w.duration,
                distance_m: w.distance,
                events: w.events.iter().map(|(n, t)| (n.clone(), frame_of(*t))).collect(),
                feet: w.feet.clone(),
                feet_down: w.feet_down.clone(),
                pose0: pose0[wi],
            });
            for (f, &t) in w.times.iter().enumerate() {
                let index = first + f as u32;
                let (col, row) = (index % columns, index / columns);
                cells.push(Cell { strip: strips.len() - 1, frame: f as u32, rect: [col * ask.cell[0], row * ask.cell[1], ask.cell[0], ask.cell[1]], time_s: t, pose: pose0[wi] + f });
            }
        }
    }
    let shadow = ask.shadow_blob.then(|| {
        let frame = firsts[k];
        let (col, row) = (frame % columns, frame / columns);
        // Its own pivot, the cell's middle (the model's pivot is near its cell's foot, with no
        // room below it). Seen from the sheet's elevation: as wide as the footprint, as tall as
        // it is foreshortened; inside the cell's clear margin.
        let centre = [ask.cell[0] / 2, ask.cell[1] / 2];
        let rx = (footprint.max(0.05) / s).min(centre[0].saturating_sub(MARGIN_PX).max(1) as f32);
        let ry = (rx * ask.elevation_deg.to_radians().sin()).max(1.0).min(centre[1].saturating_sub(MARGIN_PX).max(1) as f32);
        Shadow { frame, rect: [col * ask.cell[0], row * ask.cell[1], ask.cell[0], ask.cell[1]], pivot: centre, radii_px: [rx, ry] }
    });
    Ok(Plan {
        shadow,
        cell: ask.cell,
        supersample: ask.supersample,
        columns,
        atlas,
        metres_per_pixel: s,
        pivot,
        directions: ask.directions.clone(),
        elevation_deg: ask.elevation_deg,
        strips,
        cells,
        cameras,
        poses,
        extent_m: ext,
        root_drift_m,
    })
}

// ---------------------------------------------------------------------------------------------
// After the GPU: one cell's pixels

/// One cell from its render: `colour` (sRGB RGBA, the render's alpha ignored) and the opaque
/// `ids` of the same render, `ss` × `ss` samples a pixel. A pixel's alpha is the share of its
/// samples the model covers in the ID buffer (exactly 0 where none does, exactly 255 where all
/// do: never a colour key); its colour is the mean of the covered samples' colours alone, in
/// linear light, so no background colour reaches it (no halo). Uncovered pixels are (0,0,0,0).
pub fn resolve(colour: &[u8], ids: &[u32], cell: [u32; 2], ss: u32) -> Vec<u8> {
    let (cw, ch) = (cell[0] as usize, cell[1] as usize);
    let (ss, rw) = (ss.max(1) as usize, cell[0] as usize * ss.max(1) as usize);
    let lin = specula_studio::colour::linear8();
    let mut out = vec![0u8; cw * ch * 4];
    for y in 0..ch {
        for x in 0..cw {
            let (mut n, mut acc) = (0usize, [0.0f32; 3]);
            for sy in 0..ss {
                for sx in 0..ss {
                    let i = (y * ss + sy) * rw + x * ss + sx;
                    if ids.get(i).copied().unwrap_or(0) != 0 {
                        n += 1;
                        for c in 0..3 {
                            acc[c] += lin[colour[i * 4 + c] as usize];
                        }
                    }
                }
            }
            if n == 0 {
                continue;
            }
            let o = (y * cw + x) * 4;
            for c in 0..3 {
                out[o + c] = to_srgb8(acc[c] / n as f32);
            }
            out[o + 3] = ((n * 255 + ss * ss / 2) / (ss * ss)) as u8;
        }
    }
    out
}

/// Linear light back to the nearest 8-bit sRGB value (searched in the same table that decoded
/// it, so a lone sample's colour comes back exactly).
pub fn to_srgb8(v: f32) -> u8 {
    let lin = specula_studio::colour::linear8();
    let i = lin.partition_point(|x| *x < v);
    if i == 0 {
        return 0;
    }
    if i >= 256 {
        return 255;
    }
    if (lin[i] - v) < (v - lin[i - 1]) {
        i as u8
    } else {
        (i - 1) as u8
    }
}

/// Snaps every visible pixel's colour to the nearest palette colour (Oklab), and makes alpha
/// all or nothing (half covered or more: shown). Pixel art has no soft edge.
pub fn snap_to_palette(rgba: &mut [u8], palette: &[[u8; 3]]) {
    let labs: Vec<[f32; 3]> = palette.iter().map(|c| specula_studio::colour::oklab(specula_studio::colour::from8(*c))).collect();
    for px in rgba.chunks_exact_mut(4) {
        if px[3] < 128 {
            px.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        px[3] = 255;
        if labs.is_empty() {
            continue;
        }
        let lab = specula_studio::colour::oklab(specula_studio::colour::from8([px[0], px[1], px[2]]));
        let d = |l: &[f32; 3]| (l[0] - lab[0]).powi(2) + (l[1] - lab[1]).powi(2) + (l[2] - lab[2]).powi(2);
        let mut best = 0;
        for (i, l) in labs.iter().enumerate() {
            if d(l) < d(&labs[best]) {
                best = i;
            }
        }
        px[..3].copy_from_slice(&palette[best]);
    }
}

/// Carries the colour of visible pixels `px` pixels out into the transparent ones round them
/// (each takes the mean colour of its visible or already-filled neighbours, ring by ring);
/// alpha is never changed.
pub fn bleed(rgba: &mut [u8], cell: [u32; 2], px: u32) {
    let (w, h) = (cell[0] as i64, cell[1] as i64);
    let mut filled: Vec<bool> = rgba.chunks_exact(4).map(|p| p[3] > 0).collect();
    for _ in 0..px {
        let mut next = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if filled[i] {
                    continue;
                }
                let (mut n, mut acc) = (0u32, [0u32; 3]);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let j = (ny * w + nx) as usize;
                    if filled[j] {
                        n += 1;
                        for c in 0..3 {
                            acc[c] += u32::from(rgba[j * 4 + c]);
                        }
                    }
                }
                if n > 0 {
                    next.push((i, acc.map(|a| ((a + n / 2) / n) as u8)));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        for (i, c) in next {
            rgba[i * 4..i * 4 + 3].copy_from_slice(&c);
            filled[i] = true;
        }
    }
}

/// Whether anything of the model is on a cell's outermost pixels (it would be cut off there).
pub fn touches_edge(rgba: &[u8], cell: [u32; 2]) -> bool {
    let (w, h) = (cell[0] as usize, cell[1] as usize);
    let a = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3] > 0;
    (0..w).any(|x| a(x, 0) || a(x, h - 1)) || (0..h).any(|y| a(0, y) || a(w - 1, y))
}

/// `src` (a cell) into the atlas at `rect`.
pub fn blit(atlas: &mut [u8], atlas_w: u32, rect: [u32; 4], src: &[u8]) {
    let (x0, y0, w, h) = (rect[0] as usize, rect[1] as usize, rect[2] as usize, rect[3] as usize);
    for y in 0..h {
        let d = ((y0 + y) * atlas_w as usize + x0) * 4;
        atlas[d..d + w * 4].copy_from_slice(&src[y * w * 4..(y + 1) * w * 4]);
    }
}

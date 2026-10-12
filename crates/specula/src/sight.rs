//! Agent sight: cameras, draw lists and the subject list that accompanies every captured frame.
//!
//! Subjects come from the engine's own transforms and the exact view-projection used to render —
//! agents never have to guess where something is on screen.

use crate::meshes::ShapeMeshes;
use crate::world::{color_hex, vec3_json, World};
use specula_core::math::{project_to_screen, Mat4, Vec3};
use specula_core::Json;
use specula_gpu::DrawItem;

pub const SCHEMA: &str = "specula.sight/v1";

/// sRGB background for every view.
pub const CLEAR_COLOR: [f32; 4] = [0.10, 0.11, 0.13, 1.0];

const NEAR: f32 = 0.05;
const FAR: f32 = 2000.0;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov_y_deg: f32,
}

impl Camera {
    pub fn view_proj(&self, width: u32, height: u32) -> Mat4 {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let proj = Mat4::perspective_vk(self.fov_y_deg.to_radians(), aspect, NEAR, FAR);
        proj * Mat4::look_at_rh(self.eye, self.target, Vec3::Y)
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("eye", vec3_json(self.eye)),
            ("target", vec3_json(self.target)),
            ("fov_y_deg", Json::from(self.fov_y_deg)),
        ])
    }
}

/// Three-quarter view from above that fits everything in the world.
pub fn auto_camera(world: &World, fov_y_deg: f32) -> Camera {
    let (center, radius) = match world.bounds() {
        Some((lo, hi)) => ((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(0.5)),
        None => (Vec3::ZERO, 1.5),
    };
    let distance = radius / (fov_y_deg.to_radians() * 0.5).sin() * 1.15;
    let dir = Vec3::new(0.55, 0.45, 0.7).normalize();
    Camera { eye: center + dir * distance, target: center, fov_y_deg }
}

pub fn draw_list(world: &World, meshes: &ShapeMeshes) -> Vec<DrawItem> {
    world
        .entities()
        .iter()
        .map(|e| DrawItem {
            mesh: meshes.get(e.shape),
            model: e.model().to_cols_array(),
            color: [e.color[0], e.color[1], e.color[2], 1.0],
        })
        .collect()
}

/// One entry per entity: identity plus where it landed in the image.
///
/// `screen` is the pixel rectangle `[x, y, w, h]` (origin top-left) of the projected bounding box,
/// clipped to the image, or null when the entity is entirely behind the camera or off-image.
pub fn subjects(world: &World, view_proj: &Mat4, width: u32, height: u32) -> Vec<Json> {
    world
        .entities()
        .iter()
        .map(|e| {
            let projected: Vec<(f32, f32, f32)> = e
                .world_corners()
                .iter()
                .filter_map(|&p| project_to_screen(view_proj, p, width, height))
                .collect();
            let screen = screen_rect(&projected, width, height);
            let distance = projected.iter().map(|p| p.2).fold(f32::INFINITY, f32::min);
            Json::obj([
                ("id", Json::from(e.id)),
                ("name", Json::from(e.name.as_str())),
                ("shape", Json::from(e.shape.name())),
                ("color", Json::from(color_hex(e.color))),
                ("position", vec3_json(e.position)),
                ("created_by", Json::from(e.created_by.as_str())),
                ("on_screen", Json::from(screen.is_some())),
                ("screen", screen.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))),
                ("nearest_depth", if distance.is_finite() { Json::from(distance) } else { Json::Null }),
            ])
        })
        .collect()
}

/// Clip the projected points' bounding rectangle to the image. Partially-behind-camera boxes use
/// the corners in front; that can under-report the extent, which M2 fixes with proper clipping.
fn screen_rect(points: &[(f32, f32, f32)], width: u32, height: u32) -> Option<[u32; 4]> {
    if points.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &(x, y, _) in points {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (w, h) = (width as f32, height as f32);
    let (cx0, cy0, cx1, cy1) = (x0.max(0.0), y0.max(0.0), x1.min(w), y1.min(h));
    if cx0 >= cx1 || cy0 >= cy1 {
        return None;
    }
    let (px, py) = (cx0.floor() as u32, cy0.floor() as u32);
    Some([px, py, (cx1.ceil() as u32).saturating_sub(px).max(1), (cy1.ceil() as u32).saturating_sub(py).max(1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_rect_clips_and_rejects() {
        assert_eq!(screen_rect(&[], 100, 100), None);
        assert_eq!(screen_rect(&[(10.0, 20.0, 0.5), (30.5, 40.2, 0.5)], 100, 100), Some([10, 20, 21, 21]));
        assert_eq!(screen_rect(&[(-50.0, 10.0, 0.5), (20.0, 30.0, 0.5)], 100, 100), Some([0, 10, 20, 20]));
        assert_eq!(screen_rect(&[(150.0, 10.0, 0.5), (200.0, 30.0, 0.5)], 100, 100), None);
    }
}

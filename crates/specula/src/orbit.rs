//! `sight_orbit`: a turntable of N views around an entity or a point, rendered as one contact
//! sheet (a grid of tiles, row by row) so an agent sees every side in a single image.
//!
//! Azimuth 0 looks from +Z towards −Z (from the south: north is up the screen, east to the
//! right), and views go round anticlockwise seen from above (towards +X first, i.e. the camera
//! walks east, then north, then west). Elevation is the camera's angle above the target's
//! horizontal plane.

use crate::sight::Camera;
use specula_core::math::Vec3;

/// Views per orbit, and the largest contact sheet side.
pub const VIEWS: std::ops::RangeInclusive<u32> = 4..=12;
pub const MAX_SHEET: u32 = 2048;
/// Gutter between tiles, pixels.
pub const GAP: u32 = 4;

/// The camera of view `i` of `n` around `center` at `distance`, `elevation_deg` up, starting at
/// `start_deg` of azimuth.
pub fn view(center: Vec3, distance: f32, elevation_deg: f32, start_deg: f32, i: u32, n: u32, fov_y_deg: f32) -> (Camera, f32) {
    let az = start_deg + 360.0 * i as f32 / n as f32;
    let (a, e) = (az.to_radians(), elevation_deg.to_radians());
    let dir = Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos());
    (Camera::new(center + dir * distance, center, fov_y_deg), az.rem_euclid(360.0))
}

/// The distance at which a sphere of `radius` fills about 80% of the view's height.
pub fn fit_distance(radius: f32, fov_y_deg: f32) -> f32 {
    let half = (fov_y_deg.to_radians() * 0.5).sin().max(1e-3);
    (radius / half * 1.25).max(radius + 0.1)
}

/// The grid: `(columns, rows)` for `n` tiles, as square as possible, wider than tall.
pub fn grid(n: u32) -> (u32, u32) {
    let cols = (n as f32).sqrt().ceil() as u32;
    (cols, n.div_ceil(cols))
}

/// Tile size and sheet size for `n` views of `tile` pixels, shrunk (keeping the aspect) until
/// the sheet fits [`MAX_SHEET`]. Returns `(tile_w, tile_h, sheet_w, sheet_h)`.
pub fn layout(n: u32, tile: (u32, u32)) -> (u32, u32, u32, u32) {
    let (cols, rows) = grid(n);
    let (mut tw, mut th) = tile;
    let fits = |tw: u32, th: u32| tw * cols + GAP * (cols - 1) <= MAX_SHEET && th * rows + GAP * (rows - 1) <= MAX_SHEET;
    if !fits(tw, th) {
        let sx = (MAX_SHEET - GAP * (cols - 1)) as f32 / (tw * cols) as f32;
        let sy = (MAX_SHEET - GAP * (rows - 1)) as f32 / (th * rows) as f32;
        let s = sx.min(sy);
        tw = ((tw as f32 * s).floor() as u32).max(64);
        th = ((th as f32 * s).floor() as u32).max(64);
    }
    (tw, th, tw * cols + GAP * (cols - 1), th * rows + GAP * (rows - 1))
}

/// Top-left pixel of tile `i`.
pub fn tile_origin(i: u32, n: u32, tw: u32, th: u32) -> (u32, u32) {
    let (cols, _) = grid(n);
    ((i % cols) * (tw + GAP), (i / cols) * (th + GAP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_circle_the_target() {
        let c = Vec3::new(1.0, 2.0, 3.0);
        for n in [4, 7, 12] {
            for i in 0..n {
                let (cam, az) = view(c, 10.0, 30.0, 0.0, i, n, 50.0);
                assert!(((cam.eye - c).length() - 10.0).abs() < 1e-3);
                assert!(((cam.eye.y - c.y) - 5.0).abs() < 1e-3, "30 degrees up: sin 30 * 10");
                assert!((az - 360.0 * i as f32 / n as f32).abs() < 1e-3);
                assert_eq!(cam.target, c);
            }
        }
        // Azimuth 0 is from +Z (south); 90 from +X (east).
        let (south, _) = view(Vec3::ZERO, 5.0, 0.0, 0.0, 0, 4, 50.0);
        let (east, _) = view(Vec3::ZERO, 5.0, 0.0, 0.0, 1, 4, 50.0);
        assert!((south.eye - Vec3::new(0.0, 0.0, 5.0)).length() < 1e-4);
        assert!((east.eye - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn sheets_are_bounded_grids() {
        assert_eq!(grid(4), (2, 2));
        assert_eq!(grid(6), (3, 2));
        assert_eq!(grid(8), (3, 3));
        assert_eq!(grid(12), (4, 3));
        assert_eq!(layout(8, (320, 240)), (320, 240, 3 * 320 + 2 * GAP, 3 * 240 + 2 * GAP));
        for n in VIEWS {
            for tile in [(320, 240), (2048, 2048), (1000, 64)] {
                let (tw, th, sw, sh) = layout(n, tile);
                assert!(sw <= MAX_SHEET && sh <= MAX_SHEET, "{n} {tile:?}: {sw}x{sh}");
                assert!(tw >= 64 && th >= 64);
                let (x, y) = tile_origin(n - 1, n, tw, th);
                assert!(x + tw <= sw && y + th <= sh);
            }
        }
        assert_eq!(tile_origin(4, 6, 100, 50), (100 + GAP, 50 + GAP));
        assert!(fit_distance(1.0, 50.0) > 2.0);
    }
}

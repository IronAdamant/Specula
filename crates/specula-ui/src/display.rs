//! The surface a UI lays out for: its size, pixel density, the operating system's scale for
//! the monitor, safe-area insets and whether touch is a primary input (foundation F6: nothing
//! assumes a desktop, a keyboard or a 16:9 screen).
//!
//! A game and the engine's `ui_*` tools build the [`Viewport`] the same way
//! ([`Display::viewport`]), so what an agent measures is what the player sees.
//!
//! - **Scale:** [`crate::theme::ScaleRule::scale_with`]: by size against the reference (the
//!   Deck), by density, the OS's scale as a floor, times the player's UI-scale setting.
//! - **Safe area:** insets (notches, rounded corners, TV overscan) in physical px; screens that
//!   aren't `full_bleed` anchor inside it.
//! - **Aspect:** on screens wider than the theme's `scale.max_aspect`, the safe area becomes a
//!   centred band of that aspect (ultra-wide monitors keep the HUD's corners in view).

use crate::theme::Theme;
use crate::ui::Viewport;
use specula_core::json::Json;

/// Largest image side a display may have (16K), so hostile sizes can't make layouts huge.
pub const MAX_SIDE: u32 = 16_384;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    /// Physical pixels.
    pub size: [u32; 2],
    /// Physical pixel density, if known.
    pub ppi: Option<f32>,
    /// The operating system's scale for this monitor (Windows: DPI / 96), if known.
    pub os_scale: Option<f32>,
    /// Safe-area insets, physical px `[top, right, bottom, left]`.
    pub safe_area: [f32; 4],
    /// Touch is a primary input.
    pub touch: bool,
}

impl Display {
    /// A display of `size` with nothing else known.
    pub fn new(size: [u32; 2]) -> Display {
        Display { size, ppi: None, os_scale: None, safe_area: [0.0; 4], touch: false }
    }

    /// The size clamped to 1..=[`MAX_SIDE`] per side.
    pub fn clamped_size(&self) -> [u32; 2] {
        self.size.map(|v| v.clamp(1, MAX_SIDE))
    }

    /// The viewport a UI with `theme` lays out in at the player's `user_scale`: the scale rule,
    /// the safe area (each inset at most 40% of its side) and the theme's widest aspect.
    pub fn viewport(&self, theme: &Theme, user_scale: f32) -> Viewport {
        let size = self.clamped_size();
        let scale = theme.scale.scale_with(size, self.ppi, self.os_scale, user_scale);
        let (w, h) = (size[0] as f32, size[1] as f32);
        let ok = |v: f32, side: f32| if v.is_finite() { v.clamp(0.0, side * 0.4) } else { 0.0 };
        let mut s = [ok(self.safe_area[0], h), ok(self.safe_area[1], w), ok(self.safe_area[2], h), ok(self.safe_area[3], w)];
        if let Some(a) = theme.scale.max_aspect {
            let (sw, sh) = (w - s[1] - s[3], h - s[0] - s[2]);
            let widest = sh * a;
            if sw > widest + 0.5 {
                let extra = (sw - widest) * 0.5;
                s[1] += extra;
                s[3] += extra;
            }
        }
        Viewport::new(size, scale).with_safe_area(s)
    }

    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("size", Json::arr(self.size.map(|v| Json::Num(f64::from(v))))),
            ("ppi", self.ppi.map_or(Json::Null, |p| Json::Num(f64::from(p)))),
            ("os_scale", self.os_scale.map_or(Json::Null, |p| Json::Num((f64::from(p) * 1000.0).round() / 1000.0))),
            ("safe_area", Json::arr(self.safe_area.map(|v| Json::Num(f64::from(v))))),
        ];
        if self.touch {
            pairs.push(("touch", Json::Bool(true)));
        }
        Json::obj(pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(extra: &str) -> Theme {
        let t = crate::theme::tests::MINI.replacen(r#""name": "mini","#, &format!(r#""name": "mini",{extra}"#), 1);
        Theme::from_json_str(&t).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_os_scale_is_a_floor() {
        let t = theme("");
        let mut d = Display::new([1920, 1080]);
        assert_eq!(d.viewport(&t, 1.0).scale, 1.35);
        d.os_scale = Some(1.5);
        assert_eq!(d.viewport(&t, 1.0).scale, 1.5, "a 150% laptop gets at least 150%");
        d.os_scale = Some(1.0);
        assert_eq!(d.viewport(&t, 1.5).scale, 1.35 * 1.5, "the player's setting multiplies");
        d.os_scale = Some(f32::NAN);
        assert_eq!(d.viewport(&t, 1.0).scale, 1.35);
    }

    #[test]
    fn the_boost_is_capped_at_what_the_screens_were_checked_at() {
        let t = theme(r#" "scale": {"max_boost": 1.5},"#);
        // A 1280x800 window on a 175% laptop: by size 1.0, the OS asks 1.75; capped at 1.5.
        let d = Display { os_scale: Some(1.75), ..Display::new([1280, 800]) };
        assert_eq!(d.viewport(&t, 1.0).scale, 1.5);
        // The player's setting multiplies after the cap: it is their choice.
        assert_eq!(d.viewport(&t, 1.5).scale, 2.25);
        // Full screen on that laptop: by size 2.0 wins, the OS floor is below it.
        let full = Display { os_scale: Some(1.75), ..Display::new([2560, 1600]) };
        assert_eq!(full.viewport(&t, 1.0).scale, 2.0);
        assert_eq!(full.viewport(&t, 1.5).scale, 3.0);
        let bad = crate::theme::tests::MINI.replacen(r#""name": "mini","#, r#""name": "mini", "scale": {"max_boost": 0.5},"#, 1);
        assert_eq!(Theme::from_json_str(&bad).unwrap_err().path, "scale.max_boost");
    }

    /// Review R1 of m5-uitools, finding 6: on a 1280x720 window at OS 175% (and 150%) every UI
    /// scale setting gave 1.35, because max_boost capped the product. Each setting is a size now.
    #[test]
    fn every_ui_scale_setting_changes_the_size_on_a_high_dpi_laptop() {
        let t = theme(r#" "scale": {"max_boost": 1.5},"#);
        for os in [1.75, 1.5] {
            let d = Display { os_scale: Some(os), ..Display::new([1280, 720]) };
            let scales: Vec<f32> = [0.8, 0.9, 1.0, 1.25, 1.5].iter().map(|&u| d.viewport(&t, u).scale).collect();
            assert!(scales.windows(2).all(|w| w[1] > w[0]), "os {os}: {scales:?}");
            // The default is the OS floor, capped at 1.5x what 720p gives (0.9).
            assert!((d.viewport(&t, 1.0).scale - os.min(1.35)).abs() < 1e-5, "os {os}");
        }
    }

    /// The largest settings stop where the window has no more room (the theme's min_logical),
    /// never below what the size or density gives alone; 0.8, 1.0 and 1.5 still differ on the
    /// 175% laptop's 1280x720 window (review R1 of m5-uitools, finding 6).
    #[test]
    fn min_logical_stops_the_largest_settings_at_the_windows_room() {
        let t = theme(r#" "scale": {"max_boost": 1.5, "min_logical": [800, 480]},"#);
        let d = Display { os_scale: Some(1.75), ..Display::new([1280, 720]) };
        let s: Vec<f32> = [0.8, 1.0, 1.5].iter().map(|&u| d.viewport(&t, u).scale).collect();
        assert!((s[0] - 1.08).abs() < 1e-4 && (s[1] - 1.35).abs() < 1e-4 && (s[2] - 1.5).abs() < 1e-4, "{s:?}");
        let v = d.viewport(&t, 3.0);
        assert!(v.size[1] as f32 / v.scale >= 480.0 - 1e-3, "never under 480 logical px high");
        // A dense phone's own scale isn't cut by it.
        let phone = Display { ppi: Some(460.0), ..Display::new([1080, 2400]) };
        let no_floor = theme("");
        assert_eq!(phone.viewport(&t, 1.0).scale, phone.viewport(&no_floor, 1.0).scale);
        for bad in [r#"[50, 480]"#, r#"[1300, 480]"#, r#"[800]"#] {
            let j = crate::theme::tests::MINI.replacen(r#""name": "mini","#, &format!(r#""name": "mini", "scale": {{"min_logical": {bad}}},"#), 1);
            assert!(Theme::from_json_str(&j).unwrap_err().path.starts_with("scale.min_logical"), "{bad}");
        }
    }

    #[test]
    fn ultra_wide_keeps_the_safe_area_to_the_widest_aspect() {
        let t = theme(r#" "scale": {"max_aspect": 1.7778},"#);
        let v = Display::new([3440, 1440]).viewport(&t, 1.0);
        let safe = v.safe();
        assert!((safe.w / safe.h - 1.7778).abs() < 0.01, "{safe:?}");
        assert!((safe.x - (3440.0 - safe.w) / 2.0).abs() < 0.5, "centred: {safe:?}");
        // 16:9 and narrower keep the whole width.
        let v = Display::new([1920, 1080]).viewport(&t, 1.0);
        assert_eq!(v.safe().w, 1920.0);
        let v = Display::new([1024, 768]).viewport(&t, 1.0);
        assert_eq!(v.safe().w, 1024.0);
        // Theme validation names the field.
        let bad = crate::theme::tests::MINI.replacen(r#""name": "mini","#, r#""name": "mini", "scale": {"max_aspect": 0.5},"#, 1);
        let e = Theme::from_json_str(&bad).unwrap_err();
        assert_eq!(e.path, "scale.max_aspect", "{e}");
    }

    #[test]
    fn hostile_insets_and_sizes_are_bounded() {
        let t = theme("");
        let d = Display { safe_area: [f32::INFINITY, -5.0, 1e9, f32::NAN], ..Display::new([0, u32::MAX]) };
        let v = d.viewport(&t, 1.0);
        assert_eq!(v.size, [1, MAX_SIDE]);
        let s = v.safe();
        assert!(s.w > 0.0 && s.h > 0.0, "{s:?}");
    }
}

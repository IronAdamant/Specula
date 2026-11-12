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
    /// The physical panel the display is on, when the platform knows it (its native pixels and
    /// diagonal): handheld profiles read it ([`Display::handheld`]). It never changes the scale
    /// rule (that reads `ppi`), so a desktop's interface is the same whether or not it is known.
    pub panel: Option<Panel>,
}

/// A physical screen: native pixels and diagonal in inches (from the monitor's own report).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    pub size: [u32; 2],
    pub diagonal_in: f32,
    /// A built-in touch screen.
    pub touch: bool,
}

impl Panel {
    /// Pixels per inch along the diagonal.
    pub fn ppi(&self) -> f32 {
        let (w, h) = (self.size[0] as f32, self.size[1] as f32);
        (w * w + h * h).sqrt() / self.diagonal_in.max(0.1)
    }

    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("size", Json::arr(self.size.map(|v| Json::Num(f64::from(v))))),
            ("diagonal_in", Json::Num((f64::from(self.diagonal_in) * 100.0).round() / 100.0)),
            ("ppi", Json::Num(f64::from(self.ppi()).round())),
        ];
        if self.touch {
            pairs.push(("touch", Json::Bool(true)));
        }
        Json::obj(pairs)
    }
}

/// A handheld PC the display is on ([`Display::handheld`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handheld {
    /// "Steam Deck", "ROG Ally", "Legion Go", "Legion Go S", or "handheld" for an unknown small
    /// touch panel.
    pub name: &'static str,
    /// Pixels per inch of its panel.
    pub ppi: f32,
}

/// Known handhelds: name, native pixels (landscape) and diagonal range in inches.
pub const HANDHELDS: [(&str, [u32; 2], [f32; 2]); 4] = [
    // LCD (7.0") and OLED (7.4").
    ("Steam Deck", [1280, 800], [6.8, 7.6]),
    // Also the MSI Claw's 7" 1080p panel: the same size of text suits both.
    ("ROG Ally", [1920, 1080], [6.8, 7.2]),
    ("Legion Go", [2560, 1600], [8.6, 9.0]),
    ("Legion Go S", [1920, 1200], [7.8, 8.2]),
];

/// The diagonals, in inches, that count as a handheld when the model is unknown (smaller is a
/// phone, larger a tablet or laptop). The largest handheld PCs are 8.8 inches (the Legion Go);
/// Windows tablets and 2-in-1s start at about 10 (a Surface Go is 10.5), and they are held
/// further away: they keep the desktop default (review R1 of m5-a11y).
pub const HANDHELD_DIAGONAL_IN: [f32; 2] = [5.5, 9.0];

/// How large handhelds' text should be by default, physically, relative to the reference device
/// (the Deck) at UI scale 1: at arm's length on a 7" panel the reference size is the floor Valve
/// sets, not a comfortable default (owner, 2026-11-07: legible defaults on handhelds; players may
/// still choose smaller).
pub const HANDHELD_COMFORT: f32 = 1.15;

/// One UI-scale setting on a display: what it gives and whether it changes anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleOption {
    /// The player's setting (1 = as designed).
    pub user: f32,
    /// The scale the interface lays out at with it (physical px per logical px).
    pub effective: f32,
    /// Why it changes nothing on this display (a setting nearer 1 gives the same size):
    /// `limited_by_window` above 1, `smallest` below. `None`: it changes the size.
    pub limited: Option<&'static str>,
}

impl Display {
    /// A display of `size` with nothing else known.
    pub fn new(size: [u32; 2]) -> Display {
        Display { size, ppi: None, os_scale: None, safe_area: [0.0; 4], touch: false, panel: None }
    }

    /// The handheld PC this display is on, from its panel: a known model by native size and
    /// diagonal, else any panel of [`HANDHELD_DIAGONAL_IN`] with a built-in touch screen. `None`
    /// without a panel (headless, or a monitor that reports no size).
    pub fn handheld(&self) -> Option<Handheld> {
        let p = self.panel.filter(|p| p.diagonal_in.is_finite() && p.diagonal_in > 0.0)?;
        let native = [p.size[0].max(p.size[1]), p.size[0].min(p.size[1])];
        let d = p.diagonal_in;
        if let Some((name, _, _)) = HANDHELDS.iter().find(|(_, s, r)| *s == native && d >= r[0] && d <= r[1]) {
            return Some(Handheld { name, ppi: p.ppi() });
        }
        (p.touch && (HANDHELD_DIAGONAL_IN[0]..=HANDHELD_DIAGONAL_IN[1]).contains(&d)).then(|| Handheld { name: "handheld", ppi: p.ppi() })
    }

    /// The smallest of `options` (UI-scale settings, ascending) that makes text on this handheld
    /// at least [`HANDHELD_COMFORT`] times the reference's physical size (within 2%), never under 1
    /// (a profile only raises the size; smaller is the player's choice); the largest when none
    /// does. `None` when the display isn't a handheld.
    pub fn handheld_ui_scale(&self, theme: &Theme, options: &[f32]) -> Option<f32> {
        let h = self.handheld()?;
        // Physical size of a logical px, relative to the reference device's at scale 1.
        let physical = |user: f32| self.viewport(theme, user).scale * theme.scale.reference_ppi / h.ppi.max(1.0);
        options.iter().copied().filter(|&u| u >= 1.0).find(|&u| physical(u) >= HANDHELD_COMFORT * 0.98).or(options.last().copied())
    }

    /// Each UI-scale setting in `options` on this display: its effective scale, and whether it
    /// changes nothing because a setting nearer 1 already gives the same size (the window has no
    /// more room, or the theme's smallest scale is reached). A settings menu greys those out and
    /// says why; ask again when the display changes.
    pub fn scale_options(&self, theme: &Theme, options: &[f32]) -> Vec<ScaleOption> {
        let eff: Vec<f32> = options.iter().map(|&u| self.viewport(theme, u).scale).collect();
        options
            .iter()
            .enumerate()
            .map(|(i, &user)| {
                let nearer = options.iter().enumerate().any(|(j, &o)| j != i && (o - 1.0).abs() < (user - 1.0).abs() && (eff[j] - eff[i]).abs() < 1e-3);
                let limited = nearer.then_some(if user > 1.0 { "limited_by_window" } else { "smallest" });
                ScaleOption { user, effective: eff[i], limited }
            })
            .collect()
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
        if let Some(p) = &self.panel {
            pairs.push(("panel", p.to_json()));
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

    fn on_panel(size: [u32; 2], diagonal_in: f32, touch: bool) -> Display {
        Display { panel: Some(Panel { size, diagonal_in, touch }), ..Display::new(size) }
    }

    #[test]
    fn handhelds_are_known_by_their_panel() {
        assert_eq!(on_panel([1280, 800], 7.0, true).handheld().map(|h| h.name), Some("Steam Deck"));
        assert_eq!(on_panel([1280, 800], 7.4, true).handheld().map(|h| h.name), Some("Steam Deck"), "the OLED");
        // A portrait-native panel reports either way round.
        assert_eq!(on_panel([800, 1280], 7.0, false).handheld().map(|h| h.name), Some("Steam Deck"));
        assert_eq!(on_panel([1920, 1080], 7.0, true).handheld().map(|h| h.name), Some("ROG Ally"));
        assert_eq!(on_panel([2560, 1600], 8.8, true).handheld().map(|h| h.name), Some("Legion Go"));
        // An unknown small touch panel is a handheld; the same without touch, a laptop or a
        // desktop monitor, or no panel at all, isn't.
        assert_eq!(on_panel([1600, 900], 8.0, true).handheld().map(|h| h.name), Some("handheld"));
        assert_eq!(on_panel([1600, 900], 8.0, false).handheld(), None);
        assert_eq!(on_panel([1920, 1080], 15.6, true).handheld(), None);
        // A Windows tablet or 2-in-1 (a 10.1 or 10.5 inch touch panel) is held further away than a
        // handheld: it keeps the desktop default (review R1 of m5-a11y). 9 inches is the limit.
        assert_eq!(on_panel([1920, 1280], 10.5, true).handheld(), None, "a Surface Go class tablet");
        assert_eq!(on_panel([1920, 1200], 10.1, true).handheld(), None);
        assert_eq!(on_panel([1920, 1200], 9.0, true).handheld().map(|h| h.name), Some("handheld"));
        assert_eq!(on_panel([1280, 800], 24.0, false).handheld(), None, "a 1280x800 desktop monitor");
        assert_eq!(Display::new([1280, 800]).handheld(), None);
        assert_eq!(on_panel([1280, 800], f32::NAN, true).handheld(), None);
    }

    #[test]
    fn handhelds_get_a_legible_default_and_desktops_none() {
        let t = theme(r#" "scale": {"max_boost": 1.5, "min_logical": [800, 480]},"#);
        let options = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5];
        // The Deck is the reference: 1.0 is the floor, a size up is the comfortable default.
        assert_eq!(on_panel([1280, 800], 7.0, true).handheld_ui_scale(&t, &options), Some(1.15));
        // The Ally's denser panel at Windows' 150%: whatever it picks is physically comfortable.
        let ally = Display { os_scale: Some(1.5), ..on_panel([1920, 1080], 7.0, true) };
        let s = ally.handheld_ui_scale(&t, &options).unwrap();
        let physical = ally.viewport(&t, s).scale * t.scale.reference_ppi / ally.handheld().unwrap().ppi;
        assert!(physical >= HANDHELD_COMFORT * 0.98, "{s}: {physical}");
        // The Legion Go's larger panel is comfortable as designed.
        assert_eq!(on_panel([2560, 1600], 8.8, true).handheld_ui_scale(&t, &options), Some(1.0));
        assert_eq!(Display::new([1920, 1080]).handheld_ui_scale(&t, &options), None);
    }

    /// The owner's rule (2026-11-07): every UI-scale option shows its effective scale and is
    /// greyed out when it changes nothing on the current display.
    #[test]
    fn scale_options_say_which_change_nothing() {
        let t = theme(r#" "scale": {"max_boost": 1.5, "min_logical": [800, 480]},"#);
        let options = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5];
        // A big monitor: every option changes the size.
        let big = Display::new([2560, 1440]).scale_options(&t, &options);
        assert!(big.iter().all(|o| o.limited.is_none()), "{big:?}");
        assert!(big.windows(2).all(|w| w[1].effective > w[0].effective));
        // A small window: the largest options stop at the window's room.
        let small = Display { os_scale: Some(1.75), ..Display::new([1280, 720]) }.scale_options(&t, &options);
        let limited: Vec<f32> = small.iter().filter(|o| o.limited == Some("limited_by_window")).map(|o| o.user).collect();
        assert!(!limited.is_empty(), "{small:?}");
        assert!(limited.iter().all(|&u| u > 1.0));
        for o in small.iter().filter(|o| o.limited.is_some()) {
            assert!(small.iter().any(|p| p.limited.is_none() && (p.effective - o.effective).abs() < 1e-3), "a greyed option has an enabled twin: {o:?}");
        }
        assert_eq!(small[2].limited, None, "1 is never greyed");
        // The theme's smallest scale: options under 1 that give the same size as a nearer one say so.
        let tiny = Display::new([640, 400]).scale_options(&t, &options);
        for o in tiny.iter().filter(|o| o.user < 1.0 && o.limited.is_some()) {
            assert_eq!(o.limited, Some("smallest"), "{tiny:?}");
        }
    }
}

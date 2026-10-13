//! Themes as data (`specula.ui.theme.v1`): a palette, text roles, box styles, button styles and
//! the look of the special widgets, plus the scale rule and the thresholds `ui_check` enforces.
//!
//! Anywhere a colour is expected, a theme or screen may write `"#rrggbb[aa]"` or a palette name
//! (`"text"`, `"accent"`). Screens refer to roles, boxes and buttons by name, so swapping the
//! theme (high contrast, colour-vision variants) restyles every screen.

use crate::color::{parse_hex, Rgba, TRANSPARENT};
use crate::error::{bad, parse_json, suggest, wrong, ErrorCode, Obj, UiError};
use specula_core::json::Json;

pub const THEME_FORMAT: &str = "specula.ui.theme.v1";

/// A text role: font slot, size and colour (`body`, `small`, `title`, `value`, `button`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextRole {
    /// Font slot name in the [`crate::FontSet`] (falls back to the first font).
    pub font: String,
    /// Logical pixels per em.
    pub size: f32,
    pub color: Rgba,
    /// Multiplier on the font's line height.
    pub line_height: f32,
    /// A 1-logical-px drop shadow under the text, if set.
    pub shadow: Option<Rgba>,
}

/// A soft shadow under a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub offset: [f32; 2],
    pub blur: f32,
    pub color: Rgba,
}

/// A carved bevel: a ring just inside the border, light at the top fading to dark at the bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bevel {
    pub width: f32,
    pub light: Rgba,
    pub dark: Rgba,
}

/// How a box (panel, button body, keycap, tooltip) is drawn. Lengths are logical px.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxStyle {
    /// Vertical gradient `[top, bottom]` (equal for a flat fill).
    pub fill: [Rgba; 2],
    pub border: f32,
    pub border_color: [Rgba; 2],
    pub radius: f32,
    pub bevel: Option<Bevel>,
    pub shadow: Option<Shadow>,
    /// Default padding `[top, right, bottom, left]` for widgets drawn with this box.
    pub padding: [f32; 4],
}

impl BoxStyle {
    pub fn none() -> BoxStyle {
        BoxStyle {
            fill: [TRANSPARENT; 2],
            border: 0.0,
            border_color: [TRANSPARENT; 2],
            radius: 0.0,
            bevel: None,
            shadow: None,
            padding: [0.0; 4],
        }
    }

    /// The colour text inside this box sits on (the fill at mid-height), for contrast checks.
    pub fn mid_fill(&self) -> Rgba {
        crate::color::mix(self.fill[0], self.fill[1], 0.5)
    }

    pub fn is_visible(&self) -> bool {
        self.fill[0][3] > 0.0 || self.fill[1][3] > 0.0 || (self.border > 0.0 && self.border_color[0][3] > 0.0)
    }
}

/// Box and text colour for one button state.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonLook {
    pub boxed: usize,
    pub text: Rgba,
}

/// A button style: a look per state, padding and the text role.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonStyle {
    pub normal: ButtonLook,
    pub hover: ButtonLook,
    pub pressed: ButtonLook,
    pub focused: ButtonLook,
    pub disabled: ButtonLook,
    pub role: usize,
    pub padding: [f32; 4],
    /// Minimum height in logical px (touch targets).
    pub min_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    pub color: Rgba,
    pub width: f32,
    /// Gap between the widget and the ring.
    pub gap: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarStyle {
    pub track: usize,
    pub fill: Rgba,
    /// Fill colour when the value is at or below `low_below` of the maximum.
    pub low: Rgba,
    pub low_below: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialStyle {
    pub wedge: Rgba,
    pub wedge_hot: Rgba,
    pub wedge_disabled: Rgba,
    pub edge: Rgba,
    pub edge_hot: Rgba,
    pub centre: usize,
    /// Gap between wedges, logical px.
    pub gap: f32,
    pub text: Rgba,
    pub text_disabled: Rgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThreatStyle {
    pub color: Rgba,
    /// A dark disc behind each arrow, so it reads over bright scenery.
    pub halo: Rgba,
    /// Arrow length, logical px.
    pub size: f32,
    /// Distance from the safe-area edge to the arrow tip, logical px.
    pub inset: f32,
    pub pulse_hz: f32,
    pub label: usize,
    pub label_box: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToastStyle {
    pub boxed: usize,
    pub role: usize,
    pub info: Rgba,
    pub good: Rgba,
    pub warn: Rgba,
    pub threat: Rgba,
    pub duration_s: f32,
    pub fade_s: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TooltipStyle {
    pub boxed: usize,
    pub role: usize,
    pub delay_s: f32,
    pub max_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeycapStyle {
    pub boxed: usize,
    pub role: usize,
    pub min_width: f32,
}

/// How the UI scale follows the screen: `max(min(w / ref_w, h / ref_h), ppi / ref_ppi)` times
/// the player's UI-scale setting, clamped. At the reference (the Deck, 1280×800, ~215 ppi) the
/// scale is 1, so logical px = Deck px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleRule {
    pub reference: [f32; 2],
    pub reference_ppi: f32,
    pub min: f32,
    pub max: f32,
}

impl ScaleRule {
    pub fn scale(&self, size: [u32; 2], ppi: Option<f32>, user: f32) -> f32 {
        let by_size = (size[0] as f32 / self.reference[0]).min(size[1] as f32 / self.reference[1]);
        let by_ppi = ppi.filter(|p| p.is_finite() && *p > 0.0).map_or(0.0, |p| p / self.reference_ppi);
        let user = if user.is_finite() && user > 0.0 { user } else { 1.0 };
        (by_size.max(by_ppi) * user).clamp(self.min, self.max)
    }
}

/// Thresholds for `ui_check`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckRules {
    /// Minimum capital height in physical px at the reference ppi (Steam Deck guidance: 9 px at 800p).
    pub min_cap_px: f32,
    /// Minimum touch target side, logical px (44 ≈ 7 mm at the reference ppi).
    pub min_touch_px: f32,
    /// WCAG contrast for body text, and for large text.
    pub contrast_body: f32,
    pub contrast_large: f32,
    /// Text at least this many logical px per em counts as large.
    pub large_text_px: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub colors: Vec<(String, Rgba)>,
    pub roles: Vec<(String, TextRole)>,
    pub boxes: Vec<(String, BoxStyle)>,
    pub buttons: Vec<(String, ButtonStyle)>,
    /// Named lengths (`"$m"` in screens), logical px.
    pub spacing: Vec<(String, f32)>,
    pub focus: FocusRing,
    pub bar: BarStyle,
    pub radial: RadialStyle,
    pub threat: ThreatStyle,
    pub toast: ToastStyle,
    pub tooltip: TooltipStyle,
    pub keycap: KeycapStyle,
    /// Backdrop drawn under modal screens.
    pub dim: Rgba,
    pub scale: ScaleRule,
    pub check: CheckRules,
    /// Colours the world behind un-backed UI can be (darkest and brightest typical), for contrast.
    pub scene: Vec<Rgba>,
}

const TOP_FIELDS: &[&str] = &[
    "format", "name", "colors", "roles", "boxes", "buttons", "spacing", "focus", "bar", "radial", "threat", "toast",
    "tooltip", "keycap", "dim", "scale", "check", "scene", "notes",
];

impl Theme {
    pub fn from_json_str(text: &str) -> Result<Theme, UiError> {
        Theme::from_json(&parse_json(text)?)
    }

    pub fn from_json(json: &Json) -> Result<Theme, UiError> {
        let mut o = Obj::new(json, "")?;
        let format = o.req_str("format")?;
        if format != THEME_FORMAT {
            return Err(UiError::new(ErrorCode::Format, "format", format!("expected \"{THEME_FORMAT}\", found \"{format}\"")));
        }
        o.str("notes")?;
        let name = o.str("name")?.unwrap_or("theme").to_owned();

        // Palette first: everything else may refer to it.
        let mut colors = Vec::new();
        if let Some(j) = o.get("colors") {
            let pairs = j.as_object().ok_or_else(|| wrong("colors", "an object", j))?;
            for (k, v) in pairs {
                let path = format!("colors.{k}");
                let s = v.as_str().ok_or_else(|| wrong(&path, "a \"#rrggbb\" string", v))?;
                let c = parse_hex(s).ok_or_else(|| bad(&path, format!("\"{s}\" is not #rgb, #rrggbb or #rrggbbaa")))?;
                colors.push((k.clone(), c));
            }
        }
        let mut theme = Theme {
            name,
            colors,
            roles: Vec::new(),
            boxes: vec![("none".into(), BoxStyle::none())],
            buttons: Vec::new(),
            spacing: Vec::new(),
            focus: FocusRing { color: [0.25, 0.88, 0.82, 1.0], width: 2.0, gap: 2.0 },
            bar: BarStyle { track: 0, fill: [0.25, 0.88, 0.82, 1.0], low: [1.0, 0.48, 0.12, 1.0], low_below: 0.0, height: 10.0 },
            radial: RadialStyle {
                wedge: [0.17, 0.19, 0.22, 0.92],
                wedge_hot: [0.1, 0.56, 0.62, 0.95],
                wedge_disabled: [0.12, 0.13, 0.15, 0.8],
                edge: [0.35, 0.38, 0.44, 1.0],
                edge_hot: [0.72, 1.0, 0.96, 1.0],
                centre: 0,
                gap: 3.0,
                text: [0.93, 0.9, 0.84, 1.0],
                text_disabled: [0.6, 0.6, 0.6, 1.0],
            },
            threat: ThreatStyle { color: [0.61, 1.0, 0.31, 1.0], halo: [0.02, 0.03, 0.06, 0.8], size: 28.0, inset: 10.0, pulse_hz: 1.5, label: 0, label_box: 0 },
            toast: ToastStyle {
                boxed: 0,
                role: 0,
                info: [0.93, 0.9, 0.84, 1.0],
                good: [0.25, 0.88, 0.82, 1.0],
                warn: [1.0, 0.82, 0.48, 1.0],
                threat: [0.61, 1.0, 0.31, 1.0],
                duration_s: 3.0,
                fade_s: 0.4,
            },
            tooltip: TooltipStyle { boxed: 0, role: 0, delay_s: 0.45, max_width: 280.0 },
            keycap: KeycapStyle { boxed: 0, role: 0, min_width: 22.0 },
            dim: [0.0, 0.0, 0.0, 0.6],
            scale: ScaleRule { reference: [1280.0, 800.0], reference_ppi: 215.0, min: 0.5, max: 6.0 },
            check: CheckRules { min_cap_px: 9.0, min_touch_px: 44.0, contrast_body: 4.5, contrast_large: 3.0, large_text_px: 24.0 },
            scene: Vec::new(),
        };

        if let Some(j) = o.get("spacing") {
            let pairs = j.as_object().ok_or_else(|| wrong("spacing", "an object", j))?;
            for (k, v) in pairs {
                let path = format!("spacing.{k}");
                let n = v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(|| wrong(&path, "a length ≥ 0", v))?;
                theme.spacing.push((k.clone(), n as f32));
            }
        }

        let roles = o.req("roles")?;
        let pairs = roles.as_object().ok_or_else(|| wrong("roles", "an object", roles))?;
        for (k, v) in pairs {
            let role = theme.parse_role(v, &format!("roles.{k}"))?;
            theme.roles.push((k.clone(), role));
        }
        if theme.role_index("body").is_none() {
            return Err(UiError::new(ErrorCode::MissingField, "roles.body", "a theme needs a \"body\" role (it defines 1em)"));
        }

        if let Some(j) = o.get("boxes") {
            let pairs = j.as_object().ok_or_else(|| wrong("boxes", "an object", j))?;
            for (k, v) in pairs {
                let b = theme.parse_box(v, &format!("boxes.{k}"))?;
                match theme.boxes.iter_mut().find(|(n, _)| n == k) {
                    Some(slot) => slot.1 = b,
                    None => theme.boxes.push((k.clone(), b)),
                }
            }
        }

        if let Some(j) = o.get("buttons") {
            let pairs = j.as_object().ok_or_else(|| wrong("buttons", "an object", j))?;
            for (k, v) in pairs {
                let b = theme.parse_button(v, &format!("buttons.{k}"))?;
                theme.buttons.push((k.clone(), b));
            }
        }
        if theme.button_index("default").is_none() {
            return Err(UiError::new(ErrorCode::MissingField, "buttons.default", "a theme needs a \"default\" button style"));
        }

        if let Some(j) = o.get("focus") {
            let mut f = Obj::new(j, "focus")?;
            theme.focus.color = theme.color_field(&mut f, "color")?.unwrap_or(theme.focus.color);
            theme.focus.width = f.num("width")?.unwrap_or(theme.focus.width);
            theme.focus.gap = f.num("gap")?.unwrap_or(theme.focus.gap);
            f.finish(&["color", "width", "gap"])?;
        }
        if let Some(j) = o.get("bar") {
            let mut f = Obj::new(j, "bar")?;
            if let Some(n) = f.str("track")? {
                theme.bar.track = theme.box_ref(n, &f.field_path("track"))?;
            }
            theme.bar.fill = theme.color_field(&mut f, "fill")?.unwrap_or(theme.bar.fill);
            theme.bar.low = theme.color_field(&mut f, "low")?.unwrap_or(theme.bar.low);
            theme.bar.low_below = f.num("low_below")?.unwrap_or(theme.bar.low_below);
            theme.bar.height = f.num("height")?.unwrap_or(theme.bar.height);
            f.finish(&["track", "fill", "low", "low_below", "height"])?;
        }
        if let Some(j) = o.get("radial") {
            let mut f = Obj::new(j, "radial")?;
            let r = &mut theme.radial.clone();
            r.wedge = theme.color_field(&mut f, "wedge")?.unwrap_or(r.wedge);
            r.wedge_hot = theme.color_field(&mut f, "wedge_hot")?.unwrap_or(r.wedge_hot);
            r.wedge_disabled = theme.color_field(&mut f, "wedge_disabled")?.unwrap_or(r.wedge_disabled);
            r.edge = theme.color_field(&mut f, "edge")?.unwrap_or(r.edge);
            r.edge_hot = theme.color_field(&mut f, "edge_hot")?.unwrap_or(r.edge_hot);
            r.text = theme.color_field(&mut f, "text")?.unwrap_or(r.text);
            r.text_disabled = theme.color_field(&mut f, "text_disabled")?.unwrap_or(r.text_disabled);
            if let Some(n) = f.str("centre")? {
                r.centre = theme.box_ref(n, &f.field_path("centre"))?;
            }
            r.gap = f.num("gap")?.unwrap_or(r.gap);
            f.finish(&["wedge", "wedge_hot", "wedge_disabled", "edge", "edge_hot", "text", "text_disabled", "centre", "gap"])?;
            theme.radial = r.clone();
        }
        if let Some(j) = o.get("threat") {
            let mut f = Obj::new(j, "threat")?;
            let mut t = theme.threat.clone();
            t.color = theme.color_field(&mut f, "color")?.unwrap_or(t.color);
            t.halo = theme.color_field(&mut f, "halo")?.unwrap_or(t.halo);
            t.size = f.num("size")?.unwrap_or(t.size);
            t.inset = f.num("inset")?.unwrap_or(t.inset);
            t.pulse_hz = f.num("pulse_hz")?.unwrap_or(t.pulse_hz);
            if let Some(n) = f.str("label")? {
                t.label = theme.role_ref(n, &f.field_path("label"))?;
            }
            if let Some(n) = f.str("label_box")? {
                t.label_box = theme.box_ref(n, &f.field_path("label_box"))?;
            }
            f.finish(&["color", "halo", "size", "inset", "pulse_hz", "label", "label_box"])?;
            theme.threat = t;
        }
        if let Some(j) = o.get("toast") {
            let mut f = Obj::new(j, "toast")?;
            let mut t = theme.toast.clone();
            if let Some(n) = f.str("box")? {
                t.boxed = theme.box_ref(n, &f.field_path("box"))?;
            }
            if let Some(n) = f.str("role")? {
                t.role = theme.role_ref(n, &f.field_path("role"))?;
            }
            t.info = theme.color_field(&mut f, "info")?.unwrap_or(t.info);
            t.good = theme.color_field(&mut f, "good")?.unwrap_or(t.good);
            t.warn = theme.color_field(&mut f, "warn")?.unwrap_or(t.warn);
            t.threat = theme.color_field(&mut f, "threat")?.unwrap_or(t.threat);
            t.duration_s = f.num("duration_s")?.unwrap_or(t.duration_s);
            t.fade_s = f.num("fade_s")?.unwrap_or(t.fade_s);
            f.finish(&["box", "role", "info", "good", "warn", "threat", "duration_s", "fade_s"])?;
            theme.toast = t;
        }
        if let Some(j) = o.get("tooltip") {
            let mut f = Obj::new(j, "tooltip")?;
            if let Some(n) = f.str("box")? {
                theme.tooltip.boxed = theme.box_ref(n, &f.field_path("box"))?;
            }
            if let Some(n) = f.str("role")? {
                theme.tooltip.role = theme.role_ref(n, &f.field_path("role"))?;
            }
            theme.tooltip.delay_s = f.num("delay_s")?.unwrap_or(theme.tooltip.delay_s);
            theme.tooltip.max_width = f.num("max_width")?.unwrap_or(theme.tooltip.max_width);
            f.finish(&["box", "role", "delay_s", "max_width"])?;
        }
        if let Some(j) = o.get("keycap") {
            let mut f = Obj::new(j, "keycap")?;
            if let Some(n) = f.str("box")? {
                theme.keycap.boxed = theme.box_ref(n, &f.field_path("box"))?;
            }
            if let Some(n) = f.str("role")? {
                theme.keycap.role = theme.role_ref(n, &f.field_path("role"))?;
            }
            theme.keycap.min_width = f.num("min_width")?.unwrap_or(theme.keycap.min_width);
            f.finish(&["box", "role", "min_width"])?;
        }
        if let Some(j) = o.get("dim") {
            theme.dim = theme.color_value(j, "dim")?;
        }
        if let Some(j) = o.get("scale") {
            let mut f = Obj::new(j, "scale")?;
            if let Some(r) = f.get("reference") {
                theme.scale.reference = num2(r, "scale.reference")?;
            }
            theme.scale.reference_ppi = f.num("reference_ppi")?.unwrap_or(theme.scale.reference_ppi);
            theme.scale.min = f.num("min")?.unwrap_or(theme.scale.min);
            theme.scale.max = f.num("max")?.unwrap_or(theme.scale.max);
            f.finish(&["reference", "reference_ppi", "min", "max"])?;
            if theme.scale.reference[0] <= 0.0 || theme.scale.reference[1] <= 0.0 || theme.scale.reference_ppi <= 0.0 {
                return Err(bad("scale", "reference size and ppi must be positive"));
            }
        }
        if let Some(j) = o.get("check") {
            let mut f = Obj::new(j, "check")?;
            let c = &mut theme.check;
            c.min_cap_px = f.num("min_cap_px")?.unwrap_or(c.min_cap_px);
            c.min_touch_px = f.num("min_touch_px")?.unwrap_or(c.min_touch_px);
            c.contrast_body = f.num("contrast_body")?.unwrap_or(c.contrast_body);
            c.contrast_large = f.num("contrast_large")?.unwrap_or(c.contrast_large);
            c.large_text_px = f.num("large_text_px")?.unwrap_or(c.large_text_px);
            f.finish(&["min_cap_px", "min_touch_px", "contrast_body", "contrast_large", "large_text_px"])?;
        }
        if let Some(j) = o.get("scene") {
            let arr = j.as_array().ok_or_else(|| wrong("scene", "an array of colours", j))?;
            for (i, c) in arr.iter().enumerate() {
                theme.scene.push(theme.color_value(c, &format!("scene[{i}]"))?);
            }
        }
        o.finish(TOP_FIELDS)?;
        Ok(theme)
    }

    // ---- lookups ----

    pub fn color(&self, name: &str) -> Option<Rgba> {
        self.colors.iter().find(|(n, _)| n == name).map(|(_, c)| *c)
    }

    pub fn role_index(&self, name: &str) -> Option<usize> {
        self.roles.iter().position(|(n, _)| n == name)
    }

    pub fn box_index(&self, name: &str) -> Option<usize> {
        self.boxes.iter().position(|(n, _)| n == name)
    }

    pub fn button_index(&self, name: &str) -> Option<usize> {
        self.buttons.iter().position(|(n, _)| n == name)
    }

    pub fn role(&self, i: usize) -> &TextRole {
        &self.roles[i.min(self.roles.len() - 1)].1
    }

    pub fn boxed(&self, i: usize) -> &BoxStyle {
        &self.boxes[i.min(self.boxes.len() - 1)].1
    }

    pub fn button(&self, i: usize) -> &ButtonStyle {
        &self.buttons[i.min(self.buttons.len() - 1)].1
    }

    /// 1em in logical px: the body role's size.
    pub fn em(&self) -> f32 {
        self.role_index("body").map_or(16.0, |i| self.roles[i].1.size)
    }

    pub fn spacing(&self, name: &str) -> Option<f32> {
        self.spacing.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    pub(crate) fn role_ref(&self, name: &str, path: &str) -> Result<usize, UiError> {
        self.role_index(name).ok_or_else(|| {
            unknown(path, "text role", name, self.roles.iter().map(|(n, _)| n.as_str()))
        })
    }

    pub(crate) fn box_ref(&self, name: &str, path: &str) -> Result<usize, UiError> {
        self.box_index(name).ok_or_else(|| unknown(path, "box style", name, self.boxes.iter().map(|(n, _)| n.as_str())))
    }

    pub(crate) fn button_ref(&self, name: &str, path: &str) -> Result<usize, UiError> {
        self.button_index(name)
            .ok_or_else(|| unknown(path, "button style", name, self.buttons.iter().map(|(n, _)| n.as_str())))
    }

    /// `"#hex"` or a palette name.
    pub fn resolve_color(&self, s: &str, path: &str) -> Result<Rgba, UiError> {
        if s.starts_with('#') {
            return parse_hex(s).ok_or_else(|| bad(path, format!("\"{s}\" is not #rgb, #rgba, #rrggbb or #rrggbbaa")));
        }
        self.color(s).ok_or_else(|| unknown(path, "colour", s, self.colors.iter().map(|(n, _)| n.as_str())))
    }

    pub(crate) fn color_value(&self, j: &Json, path: &str) -> Result<Rgba, UiError> {
        let s = j.as_str().ok_or_else(|| wrong(path, "a colour (\"#rrggbb\" or a palette name)", j))?;
        self.resolve_color(s, path)
    }

    fn color_field(&self, o: &mut Obj<'_>, key: &str) -> Result<Option<Rgba>, UiError> {
        let path = o.field_path(key);
        match o.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(j) => self.color_value(j, &path).map(Some),
        }
    }

    /// One colour or `[top, bottom]`.
    fn gradient(&self, j: &Json, path: &str) -> Result<[Rgba; 2], UiError> {
        match j {
            Json::Arr(a) if a.len() == 2 => {
                Ok([self.color_value(&a[0], &format!("{path}[0]"))?, self.color_value(&a[1], &format!("{path}[1]"))?])
            }
            Json::Str(_) => {
                let c = self.color_value(j, path)?;
                Ok([c, c])
            }
            _ => Err(wrong(path, "a colour or [top, bottom] colours", j)),
        }
    }

    fn parse_role(&self, j: &Json, path: &str) -> Result<TextRole, UiError> {
        let mut o = Obj::new(j, path)?;
        let font = o.str("font")?.unwrap_or("ui").to_owned();
        let size = o.num("size")?.ok_or_else(|| UiError::new(ErrorCode::MissingField, o.field_path("size"), "\"size\" is required"))?;
        if !(1.0..=512.0).contains(&size) {
            return Err(bad(&o.field_path("size"), format!("size {size} is outside 1..=512")));
        }
        let color = self.color_field(&mut o, "color")?.unwrap_or([0.93, 0.9, 0.84, 1.0]);
        let line_height = o.num("line_height")?.unwrap_or(1.0);
        if !(0.5..=4.0).contains(&line_height) {
            return Err(bad(&o.field_path("line_height"), "line_height must be within 0.5..=4"));
        }
        let shadow = self.color_field(&mut o, "shadow")?;
        o.finish(&["font", "size", "color", "line_height", "shadow"])?;
        Ok(TextRole { font, size, color, line_height, shadow })
    }

    fn parse_box(&self, j: &Json, path: &str) -> Result<BoxStyle, UiError> {
        let mut o = Obj::new(j, path)?;
        let mut b = BoxStyle::none();
        if let Some(f) = o.get("fill") {
            b.fill = self.gradient(f, &format!("{path}.fill"))?;
        }
        b.border = o.num("border")?.unwrap_or(0.0).max(0.0);
        if let Some(f) = o.get("border_color") {
            b.border_color = self.gradient(f, &format!("{path}.border_color"))?;
        }
        b.radius = o.num("radius")?.unwrap_or(0.0).max(0.0);
        if let Some(bv) = o.get("bevel") {
            let bp = format!("{path}.bevel");
            let mut v = Obj::new(bv, &bp)?;
            let width = v.num("width")?.unwrap_or(1.0);
            let light = self.color_field(&mut v, "light")?.unwrap_or([1.0, 1.0, 1.0, 0.15]);
            let dark = self.color_field(&mut v, "dark")?.unwrap_or([0.0, 0.0, 0.0, 0.3]);
            v.finish(&["width", "light", "dark"])?;
            b.bevel = Some(Bevel { width, light, dark });
        }
        if let Some(sj) = o.get("shadow") {
            let sp = format!("{path}.shadow");
            let mut s = Obj::new(sj, &sp)?;
            let offset = match s.get("offset") {
                Some(j) => num2(j, &format!("{sp}.offset"))?,
                None => [0.0, 2.0],
            };
            let blur = s.num("blur")?.unwrap_or(4.0).max(0.0);
            let color = self.color_field(&mut s, "color")?.unwrap_or([0.0, 0.0, 0.0, 0.5]);
            s.finish(&["offset", "blur", "color"])?;
            b.shadow = Some(Shadow { offset, blur, color });
        }
        if let Some(p) = o.get("padding") {
            b.padding = parse_px4(p, &format!("{path}.padding"))?;
        }
        o.finish(&["fill", "border", "border_color", "radius", "bevel", "shadow", "padding"])?;
        Ok(b)
    }

    fn parse_look(&self, j: &Json, path: &str, base: Option<&ButtonLook>) -> Result<ButtonLook, UiError> {
        let mut o = Obj::new(j, path)?;
        let boxed = match o.str("box")? {
            Some(n) => self.box_ref(n, &o.field_path("box"))?,
            None => match base {
                Some(b) => b.boxed,
                None => return Err(UiError::new(ErrorCode::MissingField, o.field_path("box"), "\"box\" is required")),
            },
        };
        let text = match self.color_field(&mut o, "text")? {
            Some(c) => c,
            None => base.map_or([0.93, 0.9, 0.84, 1.0], |b| b.text),
        };
        o.finish(&["box", "text"])?;
        Ok(ButtonLook { boxed, text })
    }

    fn parse_button(&self, j: &Json, path: &str) -> Result<ButtonStyle, UiError> {
        let mut o = Obj::new(j, path)?;
        let normal = self.parse_look(o.req("normal")?, &format!("{path}.normal"), None)?;
        let state = |o: &mut Obj<'_>, key: &str| -> Result<ButtonLook, UiError> {
            match o.get(key) {
                Some(j) => self.parse_look(j, &format!("{path}.{key}"), Some(&normal)),
                None => Ok(normal.clone()),
            }
        };
        let hover = state(&mut o, "hover")?;
        let pressed = state(&mut o, "pressed")?;
        let focused = state(&mut o, "focused")?;
        let disabled = state(&mut o, "disabled")?;
        let role = match o.str("role")? {
            Some(n) => self.role_ref(n, &o.field_path("role"))?,
            None => self.role_index("button").or_else(|| self.role_index("body")).unwrap_or(0),
        };
        let padding = match o.get("padding") {
            Some(p) => parse_px4(p, &format!("{path}.padding"))?,
            None => [8.0, 16.0, 8.0, 16.0],
        };
        let min_height = o.num("min_height")?.unwrap_or(0.0);
        o.finish(&["normal", "hover", "pressed", "focused", "disabled", "role", "padding", "min_height"])?;
        Ok(ButtonStyle { normal, hover, pressed, focused, disabled, role, padding, min_height })
    }
}

pub(crate) fn unknown<'a>(path: &str, what: &str, name: &str, known: impl Iterator<Item = &'a str> + Clone) -> UiError {
    let hint = suggest(name, known.clone());
    let list: Vec<&str> = known.collect();
    UiError::new(
        ErrorCode::UnknownName,
        path,
        format!("unknown {what} \"{name}\"{hint}; the theme has: {}", list.join(", ")),
    )
}

pub(crate) fn num2(j: &Json, path: &str) -> Result<[f32; 2], UiError> {
    match j.as_array() {
        Some([a, b]) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Ok([a as f32, b as f32]),
            _ => Err(wrong(path, "two numbers", j)),
        },
        _ => Err(wrong(path, "[x, y]", j)),
    }
}

/// `n`, `[vertical, horizontal]` or `[top, right, bottom, left]`, logical px.
pub(crate) fn parse_px4(j: &Json, path: &str) -> Result<[f32; 4], UiError> {
    let n = |j: &Json| j.as_f64().filter(|v| v.is_finite() && *v >= 0.0 && *v < 1e5).map(|v| v as f32);
    let err = || wrong(path, "a length ≥ 0, [v, h] or [top, right, bottom, left]", j);
    match j {
        Json::Num(_) => n(j).map(|v| [v; 4]).ok_or_else(err),
        Json::Arr(a) => {
            let v: Option<Vec<f32>> = a.iter().map(n).collect();
            match v.as_deref() {
                Some([v, h]) => Ok([*v, *h, *v, *h]),
                Some([t, r, b, l]) => Ok([*t, *r, *b, *l]),
                _ => Err(err()),
            }
        }
        _ => Err(err()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const MINI: &str = r##"{
      "format": "specula.ui.theme.v1",
      "name": "mini",
      "colors": { "stone": "#2b3038", "text": "#ece6d6", "accent": "#3fe0d0" },
      "roles": { "body": { "size": 15, "color": "text" }, "title": { "size": 32, "font": "title" } },
      "boxes": { "stone": { "fill": ["#323843", "stone"], "border": 1, "border_color": "#5a6270", "radius": 6,
                            "bevel": { "width": 1 }, "shadow": { "offset": [0, 2], "blur": 6 }, "padding": [6, 10] } },
      "buttons": { "default": { "normal": { "box": "stone", "text": "text" }, "hover": { "text": "accent" } } }
    }"##;

    #[test]
    fn parses_a_minimal_theme() {
        let t = Theme::from_json_str(MINI).unwrap();
        assert_eq!(t.em(), 15.0);
        let b = t.boxed(t.box_index("stone").unwrap());
        assert_eq!(b.padding, [6.0, 10.0, 6.0, 10.0]);
        assert_eq!(b.fill[1], t.color("stone").unwrap());
        let btn = t.button(0);
        assert_eq!(btn.hover.boxed, btn.normal.boxed);
        assert_eq!(btn.hover.text, t.color("accent").unwrap());
        assert_eq!(t.role(t.role_index("title").unwrap()).font, "title");
    }

    #[test]
    fn theme_errors_name_the_path() {
        let cases = [
            (MINI.replace("\"#2b3038\"", "\"#2b3038f\""), ErrorCode::BadValue, "colors.stone"),
            (MINI.replace("\"color\": \"text\"", "\"color\": \"txet\""), ErrorCode::UnknownName, "roles.body.color"),
            (MINI.replace("\"radius\": 6", "\"radius\": 6, \"radios\": 2"), ErrorCode::UnknownField, "boxes.stone.radios"),
            (MINI.replace("\"box\": \"stone\"", "\"box\": \"ston\""), ErrorCode::UnknownName, "buttons.default.normal.box"),
            (MINI.replace("specula.ui.theme.v1", "v0"), ErrorCode::Format, "format"),
            (MINI.replace("\"body\"", "\"prose\""), ErrorCode::MissingField, "roles.body"),
        ];
        for (text, code, path) in cases {
            let e = Theme::from_json_str(&text).unwrap_err();
            assert_eq!((e.code, e.path.as_str()), (code, path), "{e}");
        }
        let e = Theme::from_json_str(&MINI.replace("\"color\": \"text\"", "\"color\": \"txet\"")).unwrap_err();
        assert!(e.message.contains("did you mean \"text\""), "{e}");
    }

    #[test]
    fn scale_rule_follows_screen_and_ppi() {
        let t = Theme::from_json_str(MINI).unwrap();
        let s = t.scale;
        assert_eq!(s.scale([1280, 800], Some(215.0), 1.0), 1.0);
        assert_eq!(s.scale([1920, 1080], Some(92.0), 1.0), 1.35);
        assert_eq!(s.scale([2560, 1080], None, 1.0), 1.35);
        // A phone in portrait: the size rule says 0.84, its density says ~1.95.
        let phone = s.scale([1080, 2400], Some(420.0), 1.0);
        assert!((phone - 420.0 / 215.0).abs() < 1e-5);
        assert_eq!(s.scale([1280, 800], None, 1.25), 1.25);
        assert_eq!(s.scale([1280, 800], None, f32::NAN), 1.0);
    }
}

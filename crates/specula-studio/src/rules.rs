//! `style.toml`: a game's measurable style rules (docs/design/ASSET_STUDIO.md).
//!
//! The studio reads its own tables (`budget`, `scale`, `palette`, `naming`, `taste`, `stage`,
//! `shots`, `bible`) and rejects unknown keys inside them; other tables (`[motion]`, read by
//! `specula-anim`) belong to other systems and are left alone. Every limit remembers where it came
//! from (`budget[2].max_triangles`), so a finding can say which rule it broke.

use crate::colour::{parse_hex, Rgb};
use crate::StyleError;
use specula_core::Json;

/// Which assets a rule applies to: a category, a name glob (`*` matches anything), and recipe
/// parameters that must have these values. Empty fields match everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selector {
    pub category: Option<String>,
    pub name: Option<String>,
    pub when: Vec<(String, f64)>,
}

impl Selector {
    pub fn matches(&self, name: &str, category: Option<&str>, params: &[(String, f64)]) -> bool {
        self.category.as_deref().is_none_or(|c| category == Some(c))
            && self.name.as_deref().is_none_or(|g| glob(g, name))
            && self.when.iter().all(|(k, v)| params.iter().any(|(pk, pv)| pk == k && (pv - v).abs() < 1e-9))
    }

    fn to_json(&self) -> Json {
        let mut j = Json::obj::<&str>([]);
        if let Some(c) = &self.category {
            j.set("category", Json::from(c.as_str()));
        }
        if let Some(n) = &self.name {
            j.set("name", Json::from(n.as_str()));
        }
        if !self.when.is_empty() {
            j.set("when", Json::obj(self.when.iter().map(|(k, v)| (k.clone(), Json::from(*v)))));
        }
        j
    }
}

/// `*` matches any run of characters (case-sensitive).
pub fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// A limit and the rule it came from (`budget[1].max_triangles`).
#[derive(Clone, Debug, PartialEq)]
pub struct Limit<T> {
    pub value: T,
    pub rule: String,
}

/// One `[[budget]]` entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BudgetRule {
    pub select: Selector,
    pub max_triangles: Option<u32>,
    pub min_triangles: Option<u32>,
    /// Most triangles of LOD 1, 2, ... (one per later level).
    pub max_lod_triangles: Option<Vec<u32>>,
    pub max_texture_kb: Option<f64>,
    pub max_materials: Option<u32>,
}

/// The budget in effect for one asset: later matching entries override earlier ones key by key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Budget {
    pub max_triangles: Option<Limit<u32>>,
    pub min_triangles: Option<Limit<u32>>,
    pub max_lod_triangles: Option<Limit<Vec<u32>>>,
    pub max_texture_kb: Option<Limit<f64>>,
    pub max_materials: Option<Limit<u32>>,
}

/// One `[[scale]]` entry: ranges in metres, and the aspect (height over the larger footprint side).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScaleRule {
    pub select: Selector,
    pub height_m: Option<[f32; 2]>,
    pub width_m: Option<[f32; 2]>,
    pub depth_m: Option<[f32; 2]>,
    pub aspect: Option<[f32; 2]>,
    /// How high its lowest point may be above the ground (a hovering flyer's band).
    pub float_m: Option<[f32; 2]>,
    /// How deep its lowest point may reach below the ground (a cliff's roots, a sunk footing).
    pub sink_m: Option<[f32; 2]>,
}

/// How high an asset's lowest point may float above the ground when no `[[scale]]` entry says.
pub const DEFAULT_MAX_FLOAT_M: f32 = 0.05;
/// How deep it may sink below the ground when no `[[scale]]` entry says.
pub const DEFAULT_MAX_SINK_M: f32 = 0.05;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scale {
    pub height_m: Option<Limit<[f32; 2]>>,
    pub width_m: Option<Limit<[f32; 2]>>,
    pub depth_m: Option<Limit<[f32; 2]>>,
    pub aspect: Option<Limit<[f32; 2]>>,
    pub float_m: Option<Limit<[f32; 2]>>,
    pub sink_m: Option<Limit<[f32; 2]>>,
}

/// `[palette]`: how the style's palette (in `style.json`) may be used.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteRules {
    /// Named groups of palette names (`player = ["rune_glow", ...]`); every other palette colour
    /// is neutral.
    pub families: Vec<(String, Vec<String>)>,
    /// Oklab distance from the nearest palette blend beyond which a colour is off the palette.
    pub max_delta_e: f32,
    /// Share of the surface allowed off the palette (and, separately, over-saturated neutral).
    pub max_off_palette: f32,
    /// Oklch chroma a neutral colour may have (0.05 is about a "25% saturated" mid-tone).
    pub neutral_max_chroma: f32,
    /// Neutral palette names allowed above it, each with its reason.
    pub exempt: Vec<(String, String)>,
    /// Category → families its assets must not use.
    pub forbid: Vec<(String, Vec<String>)>,
    /// Family → the only assets (name globs) that may use it (STYLE.md: fire is for braziers only).
    pub reserve: Vec<(String, Vec<String>)>,
    /// Material presets that may glow (empty: any).
    pub emissive: Vec<String>,
}

impl Default for PaletteRules {
    fn default() -> Self {
        PaletteRules { families: Vec::new(), max_delta_e: 0.05, max_off_palette: 0.02, neutral_max_chroma: 0.05, exempt: Vec::new(), forbid: Vec::new(), reserve: Vec::new(), emissive: Vec::new() }
    }
}

/// `[naming]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NamingRules {
    /// `snake`: lower case letters, digits and single underscores, starting with a letter.
    pub snake_case: bool,
    pub max_len: Option<usize>,
    /// The recipe file is `<name>.json`.
    pub file_matches_name: bool,
    /// Allowed categories (`None`: any, but a category is then optional).
    pub categories: Option<Vec<String>>,
    /// Every top-level part has a name.
    pub parts_named: bool,
}

/// `[taste]`: thresholds for the taste checks.
#[derive(Clone, Debug, PartialEq)]
pub struct TasteRules {
    /// Thumbnail side for silhouettes (pixels).
    pub thumb_px: u32,
    /// Siblings whose silhouettes overlap more than this (IoU, mean over the angles) read alike.
    pub silhouette_max_iou: f32,
    /// Share of the thumbnail silhouette that survives a one-pixel opening (thin parts don't).
    pub thumb_min_solidity: f32,
    /// Subject height in pixels from the game camera at the gameplay viewport and on a phone.
    pub min_px_gameplay: f32,
    pub min_px_phone: f32,
    /// Oklab lightness difference between the subject and the ring of background around it.
    pub value_contrast_min: f32,
    pub value_contrast_worst_min: f32,
    /// Ring width around the silhouette, in pixels of the shot.
    pub ring_px: u32,
    /// Lightness spread (90th - 10th percentile) inside the silhouette under the game light.
    pub flat_min_range: f32,
    /// The shape language the style asks of an outline ([`crate::shape`]): round, square or
    /// pointed. `None`: measured and reported, not checked.
    pub shape_language: Option<crate::shape::Kind>,
    /// The share of an outline's turning that must be of that kind.
    pub shape_min_share: f32,
    /// Deliberate style: subject glob → checks it is exempt from (with the reason in comments).
    pub allow: Vec<(String, Vec<String>)>,
}

impl Default for TasteRules {
    fn default() -> Self {
        TasteRules {
            thumb_px: 64,
            silhouette_max_iou: 0.9,
            thumb_min_solidity: 0.6,
            min_px_gameplay: 16.0,
            min_px_phone: 8.0,
            value_contrast_min: 0.05,
            value_contrast_worst_min: 0.03,
            ring_px: 3,
            flat_min_range: 0.06,
            shape_language: None,
            shape_min_share: 0.5,
            allow: Vec::new(),
        }
    }
}

impl TasteRules {
    pub fn allows(&self, subject: &str, check: &str) -> bool {
        self.allow.iter().any(|(g, checks)| glob(g, subject) && checks.iter().any(|c| c == check))
    }
}

/// `[form]`: limits on what [`crate::form`] measures of a mesh's shape. A limit not given is
/// not checked (the measure is still in the asset's facts); only loose pieces are checked
/// unless the style says more, since a piece that touches nothing is a fault in any style.
#[derive(Clone, Debug, PartialEq)]
pub struct FormRules {
    /// Pieces that touch no other piece.
    pub max_loose_pieces: Option<u32>,
    /// The share of the creases' length that is razor-sharp (unbevelled).
    pub max_razor_share: Option<f32>,
    /// Length of the shading seams, as a share of the shape's size.
    pub max_seam_share: Option<f32>,
    /// Length of the creases shaded as round, as a share of the creases' length.
    pub max_smeared_share: Option<f32>,
    /// The share of the shape's relief that is small.
    pub max_small_relief: Option<f32>,
    /// The turn between neighbouring faces of a shape the recipe names round, degrees (a
    /// cylinder of 12 sides turns 30), where its facets show at the asset's size.
    pub max_facet_deg: Option<f32>,
    /// Deliberate style: subject glob → checks it is exempt from.
    pub allow: Vec<(String, Vec<String>)>,
}

impl Default for FormRules {
    fn default() -> Self {
        FormRules { max_loose_pieces: Some(0), max_razor_share: None, max_seam_share: None, max_smeared_share: None, max_small_relief: None, max_facet_deg: None, allow: Vec::new() }
    }
}

impl FormRules {
    pub fn allows(&self, subject: &str, check: &str) -> bool {
        self.allow.iter().any(|(g, checks)| glob(g, subject) && checks.iter().any(|c| c == check))
    }
}

/// A light rig: key light relative to the camera, ambient and exposure.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub key_color: Rgb,
    pub key_intensity: f32,
    pub key_elevation_deg: f32,
    /// Degrees from the camera's direction, positive to the camera's right (180: from behind the
    /// subject).
    pub key_azimuth_deg: f32,
    pub ambient_color: Rgb,
    pub ambient_intensity: f32,
    pub exposure: f32,
    pub bloom: bool,
    pub shadows: bool,
    /// Backdrop (clear) and ground colours.
    pub background: Rgb,
    pub ground: Rgb,
}

impl Rig {
    pub fn to_json(&self) -> Json {
        use crate::colour::hex;
        Json::obj([
            ("key_color", Json::from(hex(self.key_color))),
            ("key_intensity", Json::from(self.key_intensity)),
            ("key_elevation_deg", Json::from(self.key_elevation_deg)),
            ("key_azimuth_deg", Json::from(self.key_azimuth_deg)),
            ("ambient_color", Json::from(hex(self.ambient_color))),
            ("ambient_intensity", Json::from(self.ambient_intensity)),
            ("exposure", Json::from(self.exposure)),
            ("bloom", Json::from(self.bloom)),
            ("shadows", Json::from(self.shadows)),
            ("background", Json::from(hex(self.background))),
            ("ground", Json::from(hex(self.ground))),
        ])
    }
}

/// The studio's neutral rig: a white key from the camera's front-left over 18% grey.
pub fn neutral_rig() -> Rig {
    let grey = parse_hex("#767676").expect("valid");
    Rig {
        key_color: [1.0; 3],
        key_intensity: 1.6,
        key_elevation_deg: 40.0,
        key_azimuth_deg: -45.0,
        ambient_color: [1.0; 3],
        ambient_intensity: 0.45,
        exposure: 1.0,
        bloom: false,
        shadows: true,
        background: grey,
        ground: parse_hex("#5e5e5e").expect("valid"),
    }
}

/// `[stage]`: the game rig, and how much darker the worst-case rig's ambient is.
#[derive(Clone, Debug, PartialEq)]
pub struct StageRules {
    pub game: Rig,
    /// The worst rig: the game rig lit from behind the subject with ambient × this.
    pub worst_ambient_scale: f32,
}

impl Default for StageRules {
    fn default() -> Self {
        StageRules { game: neutral_rig(), worst_ambient_scale: 0.6 }
    }
}

impl StageRules {
    pub fn worst(&self) -> Rig {
        let mut r = self.game.clone();
        r.key_azimuth_deg += 180.0;
        r.ambient_intensity *= self.worst_ambient_scale;
        r
    }
}

/// The game's default camera.
#[derive(Clone, Debug, PartialEq)]
pub struct GameCamera {
    pub pitch_deg: f32,
    pub yaw_deg: f32,
    pub distance_m: f32,
    pub fov_deg: f32,
    pub viewport: [u32; 2],
}

/// `[shots]`: the game camera and the viewports of the phone and iGPU shots.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotRules {
    pub gameplay: GameCamera,
    pub phone_viewport: [u32; 2],
    pub igpu_viewport: [u32; 2],
}

impl Default for ShotRules {
    fn default() -> Self {
        ShotRules {
            gameplay: GameCamera { pitch_deg: 40.0, yaw_deg: 35.0, distance_m: 30.0, fov_deg: 45.0, viewport: [1920, 1080] },
            phone_viewport: [854, 480],
            igpu_viewport: [1280, 720],
        }
    }
}

/// The parsed rules of one `style.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleRules {
    /// The file (relative to the assets root) or `"defaults"`.
    pub source: String,
    pub budgets: Vec<BudgetRule>,
    pub scales: Vec<ScaleRule>,
    pub palette: PaletteRules,
    pub naming: NamingRules,
    pub taste: TasteRules,
    pub form: FormRules,
    pub stage: StageRules,
    pub shots: ShotRules,
    /// Category → the bible's rules as (aspect, text), in file order.
    pub bible: Vec<(String, Vec<(String, String)>)>,
    /// Top-level tables the studio doesn't read (other systems' own).
    pub other_tables: Vec<String>,
}

const MAX_RULES: usize = 512;
const MAX_TEXT: usize = 2000;

fn err(path: &str, code: &'static str, message: impl Into<String>) -> StyleError {
    StyleError { code, path: path.to_string(), message: message.into(), line: None, column: None }
}

fn keys(v: &Json, path: &str, allowed: &[&str]) -> Result<(), StyleError> {
    let obj = v.as_object().ok_or_else(|| err(path, "wrong_type", format!("[{path}] is a table")))?;
    for (k, _) in obj {
        if !allowed.contains(&k.as_str()) {
            return Err(err(&format!("{path}.{k}"), "unknown_key", format!("'{k}' is not a key of [{path}] ({})", allowed.join(", "))));
        }
    }
    Ok(())
}

fn num(v: &Json, path: &str, lo: f64, hi: f64) -> Result<f64, StyleError> {
    v.as_f64().filter(|x| x.is_finite() && (lo..=hi).contains(x)).ok_or_else(|| err(path, "out_of_range", format!("'{path}' must be a number in {lo}..{hi}")))
}

fn int(v: &Json, path: &str, lo: u64, hi: u64) -> Result<u32, StyleError> {
    v.as_f64().filter(|x| x.fract() == 0.0 && *x >= lo as f64 && *x <= hi as f64).map(|x| x as u32).ok_or_else(|| err(path, "out_of_range", format!("'{path}' must be a whole number in {lo}..{hi}")))
}

fn text(v: &Json, path: &str) -> Result<String, StyleError> {
    v.as_str().filter(|s| s.len() <= MAX_TEXT).map(str::to_string).ok_or_else(|| err(path, "wrong_type", format!("'{path}' must be a string of at most {MAX_TEXT} bytes")))
}

fn bool_of(v: &Json, path: &str) -> Result<bool, StyleError> {
    v.as_bool().ok_or_else(|| err(path, "wrong_type", format!("'{path}' must be true or false")))
}

fn range(v: &Json, path: &str, lo: f64, hi: f64) -> Result<[f32; 2], StyleError> {
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(path, "wrong_type", format!("'{path}' must be [low, high]")))?;
    let l = num(&a[0], &format!("{path}[0]"), lo, hi)?;
    let h = num(&a[1], &format!("{path}[1]"), l, hi)?;
    Ok([l as f32, h as f32])
}

fn names(v: &Json, path: &str) -> Result<Vec<String>, StyleError> {
    let a = v.as_array().filter(|a| a.len() <= MAX_RULES).ok_or_else(|| err(path, "wrong_type", format!("'{path}' must be a list of names")))?;
    a.iter().enumerate().map(|(i, x)| text(x, &format!("{path}[{i}]"))).collect()
}

/// A colour: `#rrggbb` or a palette name (resolved through `palette`).
fn colour(v: &Json, path: &str, palette: &dyn Fn(&str) -> Option<Rgb>) -> Result<Rgb, StyleError> {
    let s = v.as_str().ok_or_else(|| err(path, "wrong_type", format!("'{path}' must be \"#rrggbb\" or a palette name")))?;
    parse_hex(s).or_else(|| palette(s)).ok_or_else(|| err(path, "unknown_colour", format!("'{path}': '{s}' is neither \"#rrggbb\" nor a palette name")))
}

fn selector(v: &Json, path: &str) -> Result<Selector, StyleError> {
    let mut s = Selector::default();
    if let Some(c) = v.get("category") {
        s.category = Some(text(c, &format!("{path}.category"))?);
    }
    if let Some(n) = v.get("name") {
        s.name = Some(text(n, &format!("{path}.name"))?);
    }
    if let Some(w) = v.get("when") {
        let obj = w.as_object().ok_or_else(|| err(&format!("{path}.when"), "wrong_type", "'when' is a table of parameter values, like { tier = 3 }"))?;
        for (k, x) in obj {
            s.when.push((k.clone(), num(x, &format!("{path}.when.{k}"), -1e9, 1e9)?));
        }
    }
    Ok(s)
}

fn entries<'a>(v: &'a Json, path: &str) -> Result<&'a [Json], StyleError> {
    v.as_array().filter(|a| a.len() <= MAX_RULES).ok_or_else(|| err(path, "wrong_type", format!("[[{path}]] is an array of tables (at most {MAX_RULES})")))
}

impl StyleRules {
    /// The defaults (a game without a `style.toml`).
    pub fn defaults() -> StyleRules {
        StyleRules { source: "defaults".into(), ..StyleRules::default() }
    }

    /// Parses a `style.toml`. `palette` resolves palette names (from the game's `style.json`) for
    /// the stage colours and checks the names the palette rules list.
    pub fn parse(toml: &str, source: &str, palette: &dyn Fn(&str) -> Option<Rgb>) -> Result<StyleRules, StyleError> {
        let doc = specula_core::toml::parse(toml).map_err(|e| StyleError { code: e.code, path: String::new(), message: format!("{source}: {}", e.message), line: Some(e.line), column: Some(e.column) })?;
        let mut r = StyleRules { source: source.to_string(), ..StyleRules::default() };
        let top = doc.as_object().ok_or_else(|| err("", "wrong_type", "style.toml must be a table"))?;
        for (k, v) in top {
            match k.as_str() {
                "budget" => {
                    for (i, e) in entries(v, "budget")?.iter().enumerate() {
                        let p = format!("budget[{i}]");
                        keys(e, &p, &["category", "name", "when", "max_triangles", "min_triangles", "max_lod_triangles", "max_texture_kb", "max_materials"])?;
                        let mut b = BudgetRule { select: selector(e, &p)?, ..BudgetRule::default() };
                        if let Some(x) = e.get("max_triangles") {
                            b.max_triangles = Some(int(x, &format!("{p}.max_triangles"), 1, 10_000_000)?);
                        }
                        if let Some(x) = e.get("min_triangles") {
                            b.min_triangles = Some(int(x, &format!("{p}.min_triangles"), 0, 10_000_000)?);
                        }
                        if let Some(x) = e.get("max_lod_triangles") {
                            let a = x.as_array().filter(|a| a.len() <= 8).ok_or_else(|| err(&format!("{p}.max_lod_triangles"), "wrong_type", "a list of triangle counts for LOD 1, 2, ..."))?;
                            b.max_lod_triangles = Some(a.iter().enumerate().map(|(j, n)| int(n, &format!("{p}.max_lod_triangles[{j}]"), 1, 10_000_000)).collect::<Result<_, _>>()?);
                        }
                        if let Some(x) = e.get("max_texture_kb") {
                            b.max_texture_kb = Some(num(x, &format!("{p}.max_texture_kb"), 0.0, 1e7)?);
                        }
                        if let Some(x) = e.get("max_materials") {
                            b.max_materials = Some(int(x, &format!("{p}.max_materials"), 1, 256)?);
                        }
                        r.budgets.push(b);
                    }
                }
                "scale" => {
                    for (i, e) in entries(v, "scale")?.iter().enumerate() {
                        let p = format!("scale[{i}]");
                        keys(e, &p, &["category", "name", "when", "height_m", "width_m", "depth_m", "aspect", "float_m", "sink_m"])?;
                        let mut s = ScaleRule { select: selector(e, &p)?, ..ScaleRule::default() };
                        for (key, slot) in [("height_m", &mut s.height_m), ("width_m", &mut s.width_m), ("depth_m", &mut s.depth_m), ("aspect", &mut s.aspect), ("float_m", &mut s.float_m), ("sink_m", &mut s.sink_m)] {
                            if let Some(x) = e.get(key) {
                                *slot = Some(range(x, &format!("{p}.{key}"), 0.0, 1e5)?);
                            }
                        }
                        r.scales.push(s);
                    }
                }
                "palette" => {
                    keys(v, "palette", &["families", "max_delta_e", "max_off_palette", "neutral_max_chroma", "exempt", "forbid", "reserve", "emissive"])?;
                    let pr = &mut r.palette;
                    if let Some(f) = v.get("families") {
                        for (name, list) in f.as_object().ok_or_else(|| err("palette.families", "wrong_type", "a table of family = [palette names]"))? {
                            let p = format!("palette.families.{name}");
                            let members = names(list, &p)?;
                            for (j, m) in members.iter().enumerate() {
                                if palette(m).is_none() {
                                    return Err(err(&format!("{p}[{j}]"), "unknown_colour", format!("'{m}' is not in the style's palette")));
                                }
                            }
                            pr.families.push((name.clone(), members));
                        }
                    }
                    if let Some(x) = v.get("max_delta_e") {
                        pr.max_delta_e = num(x, "palette.max_delta_e", 0.0, 1.0)? as f32;
                    }
                    if let Some(x) = v.get("max_off_palette") {
                        pr.max_off_palette = num(x, "palette.max_off_palette", 0.0, 1.0)? as f32;
                    }
                    if let Some(x) = v.get("neutral_max_chroma") {
                        pr.neutral_max_chroma = num(x, "palette.neutral_max_chroma", 0.0, 1.0)? as f32;
                    }
                    if let Some(x) = v.get("exempt") {
                        for (name, why) in x.as_object().ok_or_else(|| err("palette.exempt", "wrong_type", "a table of palette name = \"reason\""))? {
                            let p = format!("palette.exempt.{name}");
                            if palette(name).is_none() {
                                return Err(err(&p, "unknown_colour", format!("'{name}' is not in the style's palette")));
                            }
                            pr.exempt.push((name.clone(), text(why, &p)?));
                        }
                    }
                    if let Some(x) = v.get("forbid") {
                        for (cat, list) in x.as_object().ok_or_else(|| err("palette.forbid", "wrong_type", "a table of category = [families]"))? {
                            pr.forbid.push((cat.clone(), names(list, &format!("palette.forbid.{cat}"))?));
                        }
                    }
                    if let Some(x) = v.get("reserve") {
                        for (fam, list) in x.as_object().ok_or_else(|| err("palette.reserve", "wrong_type", "a table of family = [asset name globs that may use it]"))? {
                            if !pr.families.iter().any(|(n, _)| n == fam) {
                                return Err(err(&format!("palette.reserve.{fam}"), "unknown_family", format!("'{fam}' is not one of palette.families")));
                            }
                            pr.reserve.push((fam.clone(), names(list, &format!("palette.reserve.{fam}"))?));
                        }
                    }
                    if let Some(x) = v.get("emissive") {
                        pr.emissive = names(x, "palette.emissive")?;
                    }
                    for (cat, fams) in &pr.forbid {
                        for (j, f) in fams.iter().enumerate() {
                            if !pr.families.iter().any(|(n, _)| n == f) {
                                return Err(err(&format!("palette.forbid.{cat}[{j}]"), "unknown_family", format!("'{f}' is not one of palette.families")));
                            }
                        }
                    }
                }
                "naming" => {
                    keys(v, "naming", &["case", "max_len", "file_matches_name", "categories", "parts_named"])?;
                    let n = &mut r.naming;
                    if let Some(x) = v.get("case") {
                        match x.as_str() {
                            Some("snake") => n.snake_case = true,
                            Some("any") => n.snake_case = false,
                            _ => return Err(err("naming.case", "out_of_range", "'naming.case' is \"snake\" or \"any\"")),
                        }
                    }
                    if let Some(x) = v.get("max_len") {
                        n.max_len = Some(int(x, "naming.max_len", 1, 256)? as usize);
                    }
                    if let Some(x) = v.get("file_matches_name") {
                        n.file_matches_name = bool_of(x, "naming.file_matches_name")?;
                    }
                    if let Some(x) = v.get("categories") {
                        n.categories = Some(names(x, "naming.categories")?);
                    }
                    if let Some(x) = v.get("parts_named") {
                        n.parts_named = bool_of(x, "naming.parts_named")?;
                    }
                }
                "taste" => {
                    keys(v, "taste", &["thumb_px", "silhouette_max_iou", "thumb_min_solidity", "min_px_gameplay", "min_px_phone", "value_contrast_min", "value_contrast_worst_min", "ring_px", "flat_min_range", "shape_language", "shape_min_share", "allow"])?;
                    let t = &mut r.taste;
                    if let Some(x) = v.get("thumb_px") {
                        t.thumb_px = int(x, "taste.thumb_px", 32, 256)?;
                    }
                    for (key, slot, hi) in [
                        ("silhouette_max_iou", &mut t.silhouette_max_iou, 1.0),
                        ("thumb_min_solidity", &mut t.thumb_min_solidity, 1.0),
                        ("min_px_gameplay", &mut t.min_px_gameplay, 4096.0),
                        ("min_px_phone", &mut t.min_px_phone, 4096.0),
                        ("value_contrast_min", &mut t.value_contrast_min, 1.0),
                        ("value_contrast_worst_min", &mut t.value_contrast_worst_min, 1.0),
                        ("flat_min_range", &mut t.flat_min_range, 1.0),
                        ("shape_min_share", &mut t.shape_min_share, 1.0),
                    ] {
                        if let Some(x) = v.get(key) {
                            *slot = num(x, &format!("taste.{key}"), 0.0, hi)? as f32;
                        }
                    }
                    if let Some(x) = v.get("ring_px") {
                        t.ring_px = int(x, "taste.ring_px", 1, 32)?;
                    }
                    if let Some(x) = v.get("shape_language") {
                        let kind = x.as_str().and_then(crate::shape::Kind::parse);
                        t.shape_language = Some(kind.ok_or_else(|| err("taste.shape_language", "unknown_value", "one of \"round\", \"square\", \"pointed\""))?);
                    }
                    if let Some(x) = v.get("allow") {
                        for (subject, list) in x.as_object().ok_or_else(|| err("taste.allow", "wrong_type", "a table of \"subject glob\" = [checks]"))? {
                            let p = format!("taste.allow.{subject}");
                            let checks = names(list, &p)?;
                            for (j, c) in checks.iter().enumerate() {
                                if !crate::taste::CHECKS.contains(&c.as_str()) {
                                    return Err(err(&format!("{p}[{j}]"), "unknown_check", format!("'{c}' is not a taste check ({})", crate::taste::CHECKS.join(", "))));
                                }
                            }
                            t.allow.push((subject.clone(), checks));
                        }
                    }
                }
                "form" => {
                    keys(v, "form", &["max_loose_pieces", "max_razor_share", "max_seam_share", "max_smeared_share", "max_small_relief", "max_facet_deg", "allow"])?;
                    let f = &mut r.form;
                    if let Some(x) = v.get("max_loose_pieces") {
                        f.max_loose_pieces = Some(int(x, "form.max_loose_pieces", 0, 100_000)?);
                    }
                    for (key, slot, hi) in [
                        ("max_razor_share", &mut f.max_razor_share, 1.0),
                        ("max_seam_share", &mut f.max_seam_share, 1000.0),
                        ("max_smeared_share", &mut f.max_smeared_share, 1.0),
                        ("max_small_relief", &mut f.max_small_relief, 1.0),
                        ("max_facet_deg", &mut f.max_facet_deg, 180.0),
                    ] {
                        if let Some(x) = v.get(key) {
                            *slot = Some(num(x, &format!("form.{key}"), 0.0, hi)? as f32);
                        }
                    }
                    if let Some(x) = v.get("allow") {
                        for (subject, list) in x.as_object().ok_or_else(|| err("form.allow", "wrong_type", "a table of \"subject glob\" = [checks]"))? {
                            let p = format!("form.allow.{subject}");
                            let checks = names(list, &p)?;
                            for (j, c) in checks.iter().enumerate() {
                                if !crate::form::CHECKS.contains(&c.as_str()) {
                                    return Err(err(&format!("{p}[{j}]"), "unknown_check", format!("'{c}' is not a form check ({})", crate::form::CHECKS.join(", "))));
                                }
                            }
                            f.allow.push((subject.clone(), checks));
                        }
                    }
                }
                "stage" => {
                    keys(v, "stage", &["game", "worst_ambient_scale"])?;
                    if let Some(g) = v.get("game") {
                        keys(g, "stage.game", &["key_color", "key_intensity", "key_elevation_deg", "key_azimuth_deg", "ambient_color", "ambient_intensity", "exposure", "bloom", "shadows", "background", "ground"])?;
                        let rig = &mut r.stage.game;
                        for (key, slot) in [("key_color", &mut rig.key_color), ("ambient_color", &mut rig.ambient_color), ("background", &mut rig.background), ("ground", &mut rig.ground)] {
                            if let Some(x) = g.get(key) {
                                *slot = colour(x, &format!("stage.game.{key}"), palette)?;
                            }
                        }
                        for (key, slot, lo, hi) in [
                            ("key_intensity", &mut rig.key_intensity, 0.0, 100.0),
                            ("key_elevation_deg", &mut rig.key_elevation_deg, -90.0, 90.0),
                            ("key_azimuth_deg", &mut rig.key_azimuth_deg, -360.0, 360.0),
                            ("ambient_intensity", &mut rig.ambient_intensity, 0.0, 100.0),
                            ("exposure", &mut rig.exposure, 0.01, 100.0),
                        ] {
                            if let Some(x) = g.get(key) {
                                *slot = num(x, &format!("stage.game.{key}"), lo, hi)? as f32;
                            }
                        }
                        if let Some(x) = g.get("bloom") {
                            rig.bloom = bool_of(x, "stage.game.bloom")?;
                        }
                        if let Some(x) = g.get("shadows") {
                            rig.shadows = bool_of(x, "stage.game.shadows")?;
                        }
                    }
                    if let Some(x) = v.get("worst_ambient_scale") {
                        r.stage.worst_ambient_scale = num(x, "stage.worst_ambient_scale", 0.0, 1.0)? as f32;
                    }
                }
                "shots" => {
                    keys(v, "shots", &["gameplay", "phone", "igpu"])?;
                    let viewport = |x: &Json, p: &str| -> Result<[u32; 2], StyleError> {
                        let a = x.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(p, "wrong_type", format!("'{p}' is [width, height] in pixels")))?;
                        Ok([int(&a[0], &format!("{p}[0]"), 64, 8192)?, int(&a[1], &format!("{p}[1]"), 64, 8192)?])
                    };
                    if let Some(g) = v.get("gameplay") {
                        keys(g, "shots.gameplay", &["pitch_deg", "yaw_deg", "distance_m", "fov_deg", "viewport"])?;
                        let c = &mut r.shots.gameplay;
                        for (key, slot, lo, hi) in [("pitch_deg", &mut c.pitch_deg, -89.0, 89.0), ("yaw_deg", &mut c.yaw_deg, -360.0, 360.0), ("distance_m", &mut c.distance_m, 0.5, 1e4), ("fov_deg", &mut c.fov_deg, 5.0, 120.0)] {
                            if let Some(x) = g.get(key) {
                                *slot = num(x, &format!("shots.gameplay.{key}"), lo, hi)? as f32;
                            }
                        }
                        if let Some(x) = g.get("viewport") {
                            c.viewport = viewport(x, "shots.gameplay.viewport")?;
                        }
                    }
                    for (key, slot) in [("phone", &mut r.shots.phone_viewport), ("igpu", &mut r.shots.igpu_viewport)] {
                        if let Some(t) = v.get(key) {
                            let p = format!("shots.{key}");
                            keys(t, &p, &["viewport"])?;
                            if let Some(x) = t.get("viewport") {
                                *slot = viewport(x, &format!("{p}.viewport"))?;
                            }
                        }
                    }
                }
                "bible" => {
                    for (cat, rules) in v.as_object().ok_or_else(|| err("bible", "wrong_type", "[bible.<category>] tables of aspect = \"rule text\""))? {
                        let p = format!("bible.{cat}");
                        let obj = rules.as_object().ok_or_else(|| err(&p, "wrong_type", format!("[{p}] is a table of aspect = \"rule text\"")))?;
                        let list = obj.iter().map(|(k, t)| Ok((k.clone(), text(t, &format!("{p}.{k}"))?))).collect::<Result<Vec<_>, StyleError>>()?;
                        r.bible.push((cat.clone(), list));
                    }
                }
                other => r.other_tables.push(other.to_string()),
            }
        }
        Ok(r)
    }

    /// The budget in effect for an asset.
    pub fn budget_for(&self, name: &str, category: Option<&str>, params: &[(String, f64)]) -> Budget {
        let mut b = Budget::default();
        for (i, e) in self.budgets.iter().enumerate().filter(|(_, e)| e.select.matches(name, category, params)) {
            let at = |k: &str| format!("budget[{i}].{k}");
            if let Some(v) = e.max_triangles {
                b.max_triangles = Some(Limit { value: v, rule: at("max_triangles") });
            }
            if let Some(v) = e.min_triangles {
                b.min_triangles = Some(Limit { value: v, rule: at("min_triangles") });
            }
            if let Some(v) = &e.max_lod_triangles {
                b.max_lod_triangles = Some(Limit { value: v.clone(), rule: at("max_lod_triangles") });
            }
            if let Some(v) = e.max_texture_kb {
                b.max_texture_kb = Some(Limit { value: v, rule: at("max_texture_kb") });
            }
            if let Some(v) = e.max_materials {
                b.max_materials = Some(Limit { value: v, rule: at("max_materials") });
            }
        }
        b
    }

    /// The scale ranges in effect for an asset.
    pub fn scale_for(&self, name: &str, category: Option<&str>, params: &[(String, f64)]) -> Scale {
        let mut s = Scale::default();
        for (i, e) in self.scales.iter().enumerate().filter(|(_, e)| e.select.matches(name, category, params)) {
            let at = |k: &str| format!("scale[{i}].{k}");
            if let Some(v) = e.height_m {
                s.height_m = Some(Limit { value: v, rule: at("height_m") });
            }
            if let Some(v) = e.width_m {
                s.width_m = Some(Limit { value: v, rule: at("width_m") });
            }
            if let Some(v) = e.depth_m {
                s.depth_m = Some(Limit { value: v, rule: at("depth_m") });
            }
            if let Some(v) = e.aspect {
                s.aspect = Some(Limit { value: v, rule: at("aspect") });
            }
            if let Some(v) = e.float_m {
                s.float_m = Some(Limit { value: v, rule: at("float_m") });
            }
            if let Some(v) = e.sink_m {
                s.sink_m = Some(Limit { value: v, rule: at("sink_m") });
            }
        }
        s
    }

    /// The bible's rules for a category.
    pub fn bible_for(&self, category: Option<&str>) -> &[(String, String)] {
        category.and_then(|c| self.bible.iter().find(|(k, _)| k == c)).map_or(&[], |(_, v)| v.as_slice())
    }

    /// The family a palette name belongs to (`None`: neutral).
    pub fn family_of(&self, palette_name: &str) -> Option<&str> {
        self.palette.families.iter().find(|(_, m)| m.iter().any(|n| n == palette_name)).map(|(f, _)| f.as_str())
    }

    /// A bounded summary for tool answers.
    pub fn to_json(&self) -> Json {
        let t = &self.taste;
        Json::obj([
            ("source", Json::from(self.source.as_str())),
            ("budgets", Json::from(self.budgets.len())),
            ("scales", Json::from(self.scales.len())),
            (
                "palette",
                Json::obj([
                    ("families", Json::obj(self.palette.families.iter().map(|(f, m)| (f.clone(), Json::arr(m.iter().map(|n| Json::from(n.as_str()))))))),
                    ("max_delta_e", Json::from(self.palette.max_delta_e)),
                    ("max_off_palette", Json::from(self.palette.max_off_palette)),
                    ("neutral_max_chroma", Json::from(self.palette.neutral_max_chroma)),
                ]),
            ),
            (
                "taste",
                Json::obj([
                    ("thumb_px", Json::from(t.thumb_px)),
                    ("silhouette_max_iou", Json::from(t.silhouette_max_iou)),
                    ("thumb_min_solidity", Json::from(t.thumb_min_solidity)),
                    ("min_px_gameplay", Json::from(t.min_px_gameplay)),
                    ("min_px_phone", Json::from(t.min_px_phone)),
                    ("value_contrast_min", Json::from(t.value_contrast_min)),
                    ("value_contrast_worst_min", Json::from(t.value_contrast_worst_min)),
                    ("flat_min_range", Json::from(t.flat_min_range)),
                    ("shape_language", t.shape_language.map_or(Json::Null, |k| Json::from(k.name()))),
                    ("shape_min_share", Json::from(t.shape_min_share)),
                ]),
            ),
            (
                "form",
                Json::obj(
                    [
                        ("max_loose_pieces", self.form.max_loose_pieces.map(Json::from)),
                        ("max_razor_share", self.form.max_razor_share.map(Json::from)),
                        ("max_facet_deg", self.form.max_facet_deg.map(Json::from)),
                        ("max_seam_share", self.form.max_seam_share.map(Json::from)),
                        ("max_smeared_share", self.form.max_smeared_share.map(Json::from)),
                        ("max_small_relief", self.form.max_small_relief.map(Json::from)),
                    ]
                    .into_iter()
                    .filter_map(|(k, v)| v.map(|v| (k, v))),
                ),
            ),
        ])
    }

    /// Every rule entry, for `detailed` answers.
    pub fn entries_json(&self) -> Json {
        Json::obj([
            ("budget", Json::arr(self.budgets.iter().map(|b| b.select.to_json()))),
            ("scale", Json::arr(self.scales.iter().map(|s| s.select.to_json()))),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal(n: &str) -> Option<Rgb> {
        match n {
            "rune_glow" => parse_hex("#3fe0d0"),
            "grave_glow" => parse_hex("#9cff4f"),
            "moss" => parse_hex("#4f6b3a"),
            "moon" => parse_hex("#c9d6ff"),
            _ => None,
        }
    }

    #[test]
    fn globs() {
        assert!(glob("*", "") && glob("ward*", "warding_stone") && glob("*_stone", "warding_stone") && glob("w*g*e", "warding_stone"));
        assert!(!glob("ward", "warding_stone") && !glob("*x", "warding_stone"));
    }

    #[test]
    fn later_entries_override_key_by_key() {
        let t = r#"
[[budget]]
category = "stone"
max_triangles = 1500
max_texture_kb = 64
[[budget]]
category = "stone"
when = { tier = 3 }
max_triangles = 3000
"#;
        let r = StyleRules::parse(t, "style.toml", &pal).unwrap();
        let b1 = r.budget_for("warding_stone", Some("stone"), &[("tier".into(), 1.0)]);
        assert_eq!(b1.max_triangles.as_ref().map(|l| (l.value, l.rule.as_str())), Some((1500, "budget[0].max_triangles")));
        let b3 = r.budget_for("warding_stone", Some("stone"), &[("tier".into(), 3.0)]);
        assert_eq!(b3.max_triangles.as_ref().map(|l| (l.value, l.rule.as_str())), Some((3000, "budget[1].max_triangles")));
        assert_eq!(b3.max_texture_kb.unwrap().value, 64.0);
        assert!(r.budget_for("shambler", Some("enemy"), &[]).max_triangles.is_none());
    }

    #[test]
    fn errors_say_where() {
        let e = StyleRules::parse("[[budget]]\nmax_triangle = 3\n", "s", &pal).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), ("unknown_key", "budget[0].max_triangle"));
        let e = StyleRules::parse("[palette]\nfamilies = { player = [\"nope\"] }\n", "s", &pal).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), ("unknown_colour", "palette.families.player[0]"));
        let e = StyleRules::parse("[palette]\nfamilies = { player = [\"rune_glow\"] }\nforbid = { enemy = [\"enemy\"] }\n", "s", &pal).unwrap_err();
        assert_eq!(e.code, "unknown_family");
        let e = StyleRules::parse("[taste]\nallow = { \"x\" = [\"foot_sliding\"] }\n", "s", &pal).unwrap_err();
        assert_eq!(e.code, "unknown_check");
        let e = StyleRules::parse("[scale\n", "s", &pal).unwrap_err();
        assert!(e.line == Some(1), "{e:?}");
        let e = StyleRules::parse("[[scale]]\nheight_m = [2, 1]\n", "s", &pal).unwrap_err();
        assert_eq!(e.path, "scale[0].height_m[1]");
        // A shape language is one of three, and its share a share.
        let r = StyleRules::parse("[taste]\nshape_language = \"pointed\"\nshape_min_share = 0.3\n", "s", &pal).unwrap();
        assert_eq!((r.taste.shape_language, r.taste.shape_min_share), (Some(crate::shape::Kind::Pointed), 0.3));
        assert_eq!(StyleRules::defaults().taste.shape_language, None);
        let e = StyleRules::parse("[taste]\nshape_language = \"spiky\"\n", "s", &pal).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), ("unknown_value", "taste.shape_language"));
        let e = StyleRules::parse("[taste]\nshape_language = 3\n", "s", &pal).unwrap_err();
        assert_eq!(e.path, "taste.shape_language");
        assert!(StyleRules::parse("[taste]\nshape_min_share = 1.5\n", "s", &pal).is_err());
    }

    #[test]
    fn form_limits_parse_and_refuse() {
        let pal = |_: &str| None;
        let d = StyleRules::defaults();
        assert_eq!(d.form, FormRules::default());
        assert_eq!(d.form.max_loose_pieces, Some(0));
        let r = StyleRules::parse("[form]
max_loose_pieces = 2
max_razor_share = 0.25
max_facet_deg = 20
max_seam_share = 0.5
max_smeared_share = 0.1
max_small_relief = 0.4
allow = { \"wraith*\" = [\"loose_pieces\"] }
", "s", &pal).unwrap();
        assert_eq!((r.form.max_loose_pieces, r.form.max_razor_share, r.form.max_facet_deg), (Some(2), Some(0.25), Some(20.0)));
        assert_eq!((r.form.max_seam_share, r.form.max_smeared_share, r.form.max_small_relief), (Some(0.5), Some(0.1), Some(0.4)));
        assert!(r.form.allows("wraith tier=2", "loose_pieces") && !r.form.allows("wraith", "razor_edges") && !r.form.allows("stone", "loose_pieces"));
        let j = r.to_json();
        assert_eq!(j.get("form").and_then(|f| f.get("max_razor_share")).and_then(Json::as_f64), Some(0.25));
        assert!(d.to_json().get("form").is_some_and(|f| f.get("max_razor_share").is_none() && f.get("max_loose_pieces").is_some()));
        for (bad, code) in [
            ("[form]
max_razor_share = 1.5
", "out_of_range"),
            // (Faceting is a form check again, from what a recipe names round and not from
            // the mesh: its limit is degrees, and it may be allowed like the others.)
            ("[form]
max_facet_deg = 200
", "out_of_range"),
            ("[form]
max_facet_deg = -1
", "out_of_range"),
            ("[form]
max_seam_share = -1
", "out_of_range"),
            ("[form]
max_loose_pieces = 0.5
", "out_of_range"),
            ("[form]
facets = 3
", "unknown_key"),
            ("[form]
allow = { \"x\" = [\"flat_lighting\"] }
", "unknown_check"),
        ] {
            let e = StyleRules::parse(bad, "s", &pal).unwrap_err();
            assert_eq!(e.code, code, "{bad}: {e:?}");
        }
    }

    #[test]
    fn motion_and_unknown_tables_are_left_alone() {
        let r = StyleRules::parse("[motion]\ncontact_m = 0.025\n[review]\nx = 1\n", "s", &pal).unwrap();
        assert_eq!(r.other_tables, vec!["motion".to_string(), "review".to_string()]);
    }

    #[test]
    fn stage_colours_resolve_through_the_palette() {
        let r = StyleRules::parse("[stage.game]\nkey_color = \"moon\"\nbackground = \"#0b1020\"\nkey_azimuth_deg = 25\n", "s", &pal).unwrap();
        assert_eq!(crate::colour::hex(r.stage.game.key_color), "#c9d6ff");
        assert_eq!(r.stage.worst().key_azimuth_deg, 205.0);
        assert!(StyleRules::parse("[stage.game]\nkey_color = \"sun\"\n", "s", &pal).is_err());
    }
}

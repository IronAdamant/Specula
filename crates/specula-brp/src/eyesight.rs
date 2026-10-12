//! Agent eyesight: packets, crops, motion strips, black-frame checks, packs.
//!
//! Sensory helpers for agents (not a human editor). Schema: `specula.sight/v1`.

use crate::{capture_viewport_image, BrpClient, CapturedImage};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// Eyesight packet schema version string.
pub const EYESIGHT_SCHEMA: &str = "specula.sight/v1";

/// Pre-rename schema id (Grok-Bevy); still accepted when validating saved packets.
pub const LEGACY_EYESIGHT_SCHEMA: &str = "grok-bevy.eyesight/v1";

/// Default max frames in a motion strip (A3: 8–12, capped at 12).
pub const DEFAULT_MOTION_FRAMES: u32 = 8;

/// Default delay between motion frames.
pub const DEFAULT_MOTION_INTERVAL_MS: u64 = 70;

/// Default fovea half-size in pixels when cropping around a point.
pub const DEFAULT_CROP_HALF: u32 = 96;

/// Default max subjects in filtered packets (S0: 24 for agent attention).
pub const DEFAULT_MAX_SUBJECTS: usize = 24;

/// Exact Names preferred as primary (S0.1 tier 1).
pub const PRIMARY_EXACT: &[&str] = &[
    "Player",
    "MainCamera",
    "StrategyCamera",
    "WaterBody",
    "Ground",
    "DerelictStation",
];

/// Name prefixes preferred after exact match (S0.1 tier 2).
pub const PRIMARY_PREFIXES: &[&str] = &[
    "Nebula",
    "RockOutcrop",
    "TreeScrub",
    "CliffRidge",
    "FieldScrap",
    "Asteroid",
    "Shield",
    "Fuel",
    "Beacon",
    "Rescue",
    "Relay",
    "Supply",
    "Debris",
    "IceField",
    "AshPlateau",
    "RidgeOutcrop",
    // D1/D2 dogfood features (2D+3D sight plan)
    "CometFragment",
    "SignalSat",
    "MineDrone",
    "WatchPost",
    "OreSilo",
    "PipeJunction",
    "TerrainFlat",
    "TerrainHill",
    "TerrainPeak",
    "HeightTerrain",
    // R1/R2 debt-plan dogfood
    "SolarFlareBuoy",
    "WarpGateRing",
    "RadarDome",
    "TerrainSaddle",
];

/// Acuity milestone label for 20/20-candidate packets.
pub const ACUITY_LABEL: &str = "20/20-candidate";

/// Name onboarding rule (R0): every new dogfood Name stem must score
/// `gameplay_subject_score(name) > 0` so it survives `gameplay_prefer` filter.
/// Add stems here (and to GAMEPLAY_NAME_HINTS) before dogfood claims green.
pub const DOGFOOD_NAME_STEMS: &[&str] = &[
    // Prior dogfood
    "Player",
    "BeaconBuoy",
    "RescuePod",
    "DebrisRing",
    "CometFragment",
    "SignalSat",
    "WaterBody",
    "Ground",
    "StrategyCamera",
    "WatchPost",
    "OreSilo",
    "TerrainFlat",
    "TerrainHill_N",
    "TerrainPeak_N",
    "RelayTower",
    "SupplyCrate",
    "AshPlateau",
    // R1 Crystal Drift debt plan
    "SolarFlareBuoy",
    "WarpGateRing",
    // R2 Iron Feud debt plan
    "RadarDome",
    "TerrainSaddle",
    // H2/H3 hardening craft dogfood
    "PulseMine",
    "MineDrone",
    "LoadingBay",
    "PipeJunction",
    // F1/F2 fidelity craft dogfood
    "CargoPod",
    "RepairDrone",
    "OreCrusher",
    "SignalRelay",
];

/// Gameplay name prefixes / substrings preferred in subject filter (A4).
pub const GAMEPLAY_NAME_HINTS: &[&str] = &[
    "Player",
    "Camera",
    "Water",
    "Rock",
    "Tree",
    "Cliff",
    "Scrap",
    "Station",
    "Nebula",
    "Ground",
    "Crystal",
    "Asteroid",
    "Enemy",
    "Shield",
    "Fuel",
    "Ore",
    "Furnace",
    "Drill",
    "Chest",
    "Sun",
    "Light",
    "Hud",
    "HUD",
    // S0/S1/S2 dogfood Names — must score >0 to survive gameplay_prefer filter
    "Beacon",
    "Buoy",
    "Rescue",
    "Pod",
    "Debris",
    "Relay",
    "Tower",
    "Supply",
    "Crate",
    "Ash",
    "Plateau",
    "Ridge",
    "Ice",
    // D1 Crystal Drift (2D) dogfood Names — agent sight 2D+3D plan
    "Comet",
    "Fragment",
    "Signal",
    "Sat",
    "Mine",
    "Drone",
    // D2 Iron Feud (3D) dogfood Names + height terrain bands
    "Watch",
    "Post",
    "Silo",
    "Pipe",
    "Junction",
    "Terrain",
    "Flat",
    "Hill",
    "Peak",
    "Height",
    "Mountain",
    // R1/R2 debt plan — SolarFlareBuoy, WarpGateRing, RadarDome, TerrainSaddle
    "Solar",
    "Flare",
    "Warp",
    "Gate",
    "Ring",
    "Radar",
    "Dome",
    "Saddle",
    "Mist",
    "Basin",
    // H2/H3 hardening
    "Pulse",
    "Loading",
    "Bay",
];

/// Role of a capture within an eyesight packet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureRole {
    Full,
    Crop,
    Frame,
    Baseline,
    After,
    Unlit,
    Top,
    Side,
    Strip,
    Diff,
}

impl CaptureRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Crop => "crop",
            Self::Frame => "frame",
            Self::Baseline => "baseline",
            Self::After => "after",
            Self::Unlit => "unlit",
            Self::Top => "top",
            Self::Side => "side",
            Self::Strip => "strip",
            Self::Diff => "diff",
        }
    }
}

/// One capture entry in an eyesight packet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureEntry {
    pub role: String,
    pub abs_path: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black_frame_warning: Option<bool>,
}

impl CaptureEntry {
    pub fn from_path(role: CaptureRole, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).with_context(|| format!("read capture {}", path.display()))?;
        crate::validate_png_header(&bytes)?;
        let (w, h) = crate::read_png_ihdr_size_pub(&bytes)?;
        let meta = fs::metadata(path)?;
        let abs = abs_path_string(path);
        let black = is_mostly_black_png(&bytes, 0.04)?;
        Ok(Self {
            role: role.as_str().to_string(),
            abs_path: abs,
            bytes: meta.len(),
            width: w,
            height: h,
            note: None,
            black_frame_warning: if black { Some(true) } else { None },
        })
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Named subject grounding pixels to ECS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EyesightSubject {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<[f64; 3]>,
    /// Parent entity id when BRP exposes ChildOf/Parent (T1 structural demotion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_entity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_screen_estimate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_screen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_xy: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_aabb: Option<[u32; 4]>,
    /// How many entities shared this Name before collapse (S0.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_count: Option<u32>,
}

/// Subject list filter mode (A4).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubjectFilterMode {
    All,
    #[default]
    GameplayPrefer,
    NamesOnly,
}

impl SubjectFilterMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "all" => Self::All,
            "names_only" | "names" => Self::NamesOnly,
            _ => Self::GameplayPrefer,
        }
    }
}

/// Projection mode for world→screen (A1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// Orthographic 2D (Crystal Drift): world XY, cam XY, visible half extents.
    #[default]
    Ortho2d,
    /// Top-down strategy cam (Iron Feud): world XZ mapped to screen, cam XZ.
    TopDown3d,
}

/// Stimulus applied before temporal capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StimulusInfo {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

/// Full agent eyesight packet (`specula.sight/v1`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EyesightPacket {
    pub schema: String,
    pub subject_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_state: Option<String>,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acuity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subjects_truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_path: Option<String>,
    pub captures: Vec<CaptureEntry>,
    #[serde(default)]
    pub subjects: Vec<EyesightSubject>,
    #[serde(default)]
    pub stimulus: StimulusInfo,
    pub agent_must: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

impl EyesightPacket {
    pub fn new(subject_class: impl Into<String>, intent: impl Into<String>) -> Self {
        Self {
            schema: EYESIGHT_SCHEMA.into(),
            subject_class: subject_class.into(),
            app_state: None,
            intent: intent.into(),
            style_intent: None,
            acuity: Some(ACUITY_LABEL.into()),
            primary_subject: None,
            subject_filter: None,
            subjects_truncated: None,
            views: None,
            baseline_path: None,
            captures: Vec::new(),
            subjects: Vec::new(),
            stimulus: StimulusInfo {
                kind: "none".into(),
                detail: None,
            },
            agent_must: vec!["open_and_read_each_capture_image".into()],
            target: None,
            port: None,
            pack: None,
            warnings: None,
        }
    }

    pub fn to_json_value(&self) -> Result<Value> {
        Ok(serde_json::to_value(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_pretty_json()?)?;
        Ok(path.to_path_buf())
    }

    pub fn push_warning(&mut self, w: impl Into<String>) {
        self.warnings.get_or_insert_with(Vec::new).push(w.into());
    }

    /// Validate schema + non-empty captures with positive byte counts.
    pub fn validate(&self) -> Result<()> {
        if self.schema != EYESIGHT_SCHEMA && self.schema != LEGACY_EYESIGHT_SCHEMA {
            return Err(anyhow!(
                "unexpected schema '{}' (want {})",
                self.schema,
                EYESIGHT_SCHEMA
            ));
        }
        if self.captures.is_empty() {
            return Err(anyhow!("eyesight packet has no captures"));
        }
        for c in &self.captures {
            if c.bytes == 0 {
                return Err(anyhow!("capture {} has zero bytes", c.abs_path));
            }
            if c.abs_path.is_empty() {
                return Err(anyhow!("capture missing abs_path"));
            }
        }
        Ok(())
    }
}

/// Absolute path display helper.
pub fn abs_path_string(path: &Path) -> String {
    match path.canonicalize() {
        Ok(p) => p.display().to_string(),
        Err(_) if path.is_absolute() => path.display().to_string(),
        Err(_) => std::env::current_dir()
            .map(|cwd| cwd.join(path).display().to_string())
            .unwrap_or_else(|_| path.display().to_string()),
    }
}

/// Ensure captures/eyesight (or custom) directory exists; return joined path.
pub fn eyesight_path(base: impl AsRef<Path>, file: &str) -> PathBuf {
    let dir = base.as_ref().join("captures").join("eyesight");
    let _ = fs::create_dir_all(&dir);
    dir.join(file)
}

// ── PNG decode / crop / black-frame / strip ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA8
}

impl RgbaImage {
    pub fn decode_png(bytes: &[u8]) -> Result<Self> {
        crate::validate_png_header(bytes)?;
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().context("png read_info")?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).context("png next_frame")?;
        let width = info.width;
        let height = info.height;
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => {
                let rgb = &buf[..info.buffer_size()];
                let mut out = Vec::with_capacity((width * height * 4) as usize);
                for chunk in rgb.chunks(3) {
                    out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                }
                out
            }
            png::ColorType::Grayscale => {
                let g = &buf[..info.buffer_size()];
                let mut out = Vec::with_capacity((width * height * 4) as usize);
                for &v in g {
                    out.extend_from_slice(&[v, v, v, 255]);
                }
                out
            }
            png::ColorType::GrayscaleAlpha => {
                let ga = &buf[..info.buffer_size()];
                let mut out = Vec::with_capacity((width * height * 4) as usize);
                for chunk in ga.chunks(2) {
                    out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
                }
                out
            }
            other => return Err(anyhow!("unsupported PNG color type: {other:?}")),
        };
        Ok(Self {
            width,
            height,
            pixels: rgba,
        })
    }

    pub fn encode_png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().context("png write_header")?;
            writer
                .write_image_data(&self.pixels)
                .context("png write_image_data")?;
        }
        Ok(out)
    }

    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.encode_png()?)?;
        Ok(path.to_path_buf())
    }
}

/// Axis-aligned crop in pixel space (inclusive min, exclusive max clamped).
pub fn crop_rgba(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Result<RgbaImage> {
    if w == 0 || h == 0 {
        return Err(anyhow!("crop size must be non-zero"));
    }
    let x1 = x.min(img.width.saturating_sub(1));
    let y1 = y.min(img.height.saturating_sub(1));
    let x2 = (x1 + w).min(img.width);
    let y2 = (y1 + h).min(img.height);
    let cw = x2 - x1;
    let ch = y2 - y1;
    if cw == 0 || ch == 0 {
        return Err(anyhow!("crop rectangle empty after clamp"));
    }
    let mut pixels = Vec::with_capacity((cw * ch * 4) as usize);
    for row in y1..y2 {
        let start = ((row * img.width + x1) * 4) as usize;
        let end = start + (cw as usize) * 4;
        pixels.extend_from_slice(&img.pixels[start..end]);
    }
    Ok(RgbaImage {
        width: cw,
        height: ch,
        pixels,
    })
}

/// Crop a full-frame PNG bytes to a rect; write result.
pub fn crop_png_file(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<PathBuf> {
    let bytes = fs::read(src.as_ref())?;
    let img = RgbaImage::decode_png(&bytes)?;
    let cropped = crop_rgba(&img, x, y, w, h)?;
    cropped.save_png(dest)
}

/// Crop centered on (cx, cy) with half extents.
pub fn crop_png_around(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    cx: u32,
    cy: u32,
    half_w: u32,
    half_h: u32,
) -> Result<PathBuf> {
    let x = cx.saturating_sub(half_w);
    let y = cy.saturating_sub(half_h);
    crop_png_file(src, dest, x, y, half_w.saturating_mul(2), half_h.saturating_mul(2))
}

/// True if the frame is *empty* black (minimized / no draw), not merely a dark scene.
///
/// Space games have very low **mean** luminance (black backdrop). We only flag empty
/// when there is also no significant bright content: low max luminance **and** almost
/// no pixels above a mid-bright bar (HUD, sprites, stars).
pub fn is_mostly_black_png(bytes: &[u8], threshold: f32) -> Result<bool> {
    let img = RgbaImage::decode_png(bytes)?;
    if img.pixels.is_empty() {
        return Ok(true);
    }
    let mut sum: f64 = 0.0;
    let mut max_l: f64 = 0.0;
    let mut n: f64 = 0.0;
    let mut bright: f64 = 0.0;
    // Dense-enough sample: every 4th pixel (RGBA stride * 4).
    for chunk in img.pixels.chunks(4).step_by(4) {
        if chunk.len() < 3 {
            continue;
        }
        let r = chunk[0] as f64;
        let g = chunk[1] as f64;
        let b = chunk[2] as f64;
        let l = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        sum += l;
        if l > max_l {
            max_l = l;
        }
        // Any mid-bright content (sprites, HUD, stars, UI) counts as "not empty".
        if l > 0.18 {
            bright += 1.0;
        }
        n += 1.0;
    }
    if n == 0.0 {
        return Ok(true);
    }
    let mean = (sum / n) as f32;
    let bright_frac = (bright / n) as f32;
    // Empty black only when mean is dark, peak is dim, AND almost no bright pixels.
    // Dark-but-alive scenes (space + ship) have bright_frac > ~0.001 or max_l high.
    Ok(mean < threshold && (max_l as f32) < 0.35 && bright_frac < 0.0005)
}

/// Horizontal montage of equal-height frames (scaled to min height).
pub fn montage_horizontal(frames: &[RgbaImage], gap: u32) -> Result<RgbaImage> {
    if frames.is_empty() {
        return Err(anyhow!("montage requires at least one frame"));
    }
    let h = frames.iter().map(|f| f.height).min().unwrap();
    let mut scaled: Vec<RgbaImage> = Vec::with_capacity(frames.len());
    for f in frames {
        if f.height == h {
            scaled.push(f.clone());
        } else {
            // Nearest-neighbor scale to height h.
            let scale = h as f32 / f.height as f32;
            let nw = ((f.width as f32) * scale).max(1.0) as u32;
            scaled.push(scale_nearest(f, nw, h)?);
        }
    }
    let total_w: u32 = scaled.iter().map(|f| f.width).sum::<u32>()
        + gap * (scaled.len().saturating_sub(1) as u32);
    let mut pixels = vec![0u8; (total_w * h * 4) as usize];
    let mut x_off = 0u32;
    for f in &scaled {
        for row in 0..h {
            let src_start = (row * f.width * 4) as usize;
            let src_end = src_start + (f.width * 4) as usize;
            let dst_start = ((row * total_w + x_off) * 4) as usize;
            let dst_end = dst_start + (f.width * 4) as usize;
            pixels[dst_start..dst_end].copy_from_slice(&f.pixels[src_start..src_end]);
        }
        x_off += f.width + gap;
    }
    Ok(RgbaImage {
        width: total_w,
        height: h,
        pixels,
    })
}

fn scale_nearest(img: &RgbaImage, new_w: u32, new_h: u32) -> Result<RgbaImage> {
    if new_w == 0 || new_h == 0 {
        return Err(anyhow!("scale target must be non-zero"));
    }
    let mut pixels = vec![0u8; (new_w * new_h * 4) as usize];
    for y in 0..new_h {
        let sy = (y as u64 * img.height as u64 / new_h as u64) as u32;
        for x in 0..new_w {
            let sx = (x as u64 * img.width as u64 / new_w as u64) as u32;
            let si = ((sy * img.width + sx) * 4) as usize;
            let di = ((y * new_w + x) * 4) as usize;
            pixels[di..di + 4].copy_from_slice(&img.pixels[si..si + 4]);
        }
    }
    Ok(RgbaImage {
        width: new_w,
        height: new_h,
        pixels,
    })
}

/// Simple abs-diff montage hint: average absolute channel delta as f32 0..1.
pub fn mean_abs_diff(a: &RgbaImage, b: &RgbaImage) -> Result<f32> {
    if a.width != b.width || a.height != b.height {
        return Err(anyhow!("diff requires equal dimensions"));
    }
    let n = a.pixels.len();
    if n == 0 {
        return Ok(0.0);
    }
    let mut sum: f64 = 0.0;
    for i in 0..n {
        sum += (a.pixels[i] as i16 - b.pixels[i] as i16).unsigned_abs() as f64;
    }
    Ok((sum / (n as f64) / 255.0) as f32)
}

/// Write a crude RGB abs-diff PNG (same size).
pub fn write_diff_png(a_path: &Path, b_path: &Path, dest: &Path) -> Result<(PathBuf, f32)> {
    let a = RgbaImage::decode_png(&fs::read(a_path)?)?;
    let b = RgbaImage::decode_png(&fs::read(b_path)?)?;
    // If sizes differ, scale b to a.
    let b = if b.width != a.width || b.height != a.height {
        scale_nearest(&b, a.width, a.height)?
    } else {
        b
    };
    let mut pixels = vec![0u8; a.pixels.len()];
    for i in (0..a.pixels.len()).step_by(4) {
        for c in 0..3 {
            let d = (a.pixels[i + c] as i16 - b.pixels[i + c] as i16).unsigned_abs() as u8;
            pixels[i + c] = d.saturating_mul(3).min(255); // boost visibility
        }
        pixels[i + 3] = 255;
    }
    let diff = RgbaImage {
        width: a.width,
        height: a.height,
        pixels,
    };
    let score = mean_abs_diff(&a, &b)?;
    let path = diff.save_png(dest)?;
    Ok((path, score))
}

// ── BRP subject parsing ──────────────────────────────────────────────────────

/// Parse subjects from a BRP `world.query` result (Name + Transform).
pub fn subjects_from_query(query: &Value) -> Vec<EyesightSubject> {
    let mut out = Vec::new();
    let rows = match query {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .or_else(|| o.get("entities").and_then(|e| e.as_array()).cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    for row in rows {
        let entity = row
            .get("entity")
            .and_then(|e| e.as_u64())
            .or_else(|| row.get("id").and_then(|e| e.as_u64()));
        let comps = row
            .get("components")
            .cloned()
            .unwrap_or_else(|| row.clone());
        let name = extract_name(&comps).unwrap_or_else(|| "unnamed".into());
        let translation = extract_translation(&comps);
        let parent_entity = extract_parent_entity(&comps);
        out.push(EyesightSubject {
            name,
            entity,
            translation,
            parent_entity,
            on_screen_estimate: None,
            on_screen: None,
            screen_xy: None,
            screen_aabb: None,
            duplicate_count: None,
        });
    }
    out
}

/// Rank best primary subject Name for fovea (S0.1). Never prefers Star/OreCrystal/Menu when better exist.
pub fn rank_primary_subject(subjects: &[EyesightSubject]) -> Option<String> {
    if subjects.is_empty() {
        return None;
    }
    // Tier 1: exact
    for exact in PRIMARY_EXACT {
        if subjects.iter().any(|s| s.name == *exact) {
            return Some((*exact).to_string());
        }
    }
    // Tier 2: prefix
    for pref in PRIMARY_PREFIXES {
        if let Some(s) = subjects.iter().find(|s| s.name.starts_with(pref) || s.name.contains(pref))
        {
            return Some(s.name.clone());
        }
    }
    // Tier 3: best gameplay score, excluding noise
    let mut best: Option<(&str, i32)> = None;
    for s in subjects {
        if is_noise_name(&s.name) {
            continue;
        }
        let sc = gameplay_subject_score(&s.name);
        if sc <= 0 {
            continue;
        }
        match best {
            None => best = Some((s.name.as_str(), sc)),
            Some((_, b)) if sc > b => best = Some((s.name.as_str(), sc)),
            _ => {}
        }
    }
    if let Some((n, _)) = best {
        return Some(n.to_string());
    }
    // Last resort: first non-noise
    subjects
        .iter()
        .find(|s| !is_noise_name(&s.name))
        .map(|s| s.name.clone())
        .or_else(|| subjects.first().map(|s| s.name.clone()))
}

fn is_noise_name(name: &str) -> bool {
    name.starts_with("Star")
        || name.contains("Particle")
        || name.starts_with("OreCrystal")
        || name.contains("Menu")
        || name == "unnamed"
        || name == "OwnershipFlag"
        // Child mesh parts only (not WaterBody / SignalSat / etc.)
        || is_child_mesh_part(name)
}

/// Top-level gameplay / dogfood Names that must never be demoted as multiparts (T1).
pub fn is_protected_top_level_name(name: &str) -> bool {
    matches!(
        name,
        "OreCrusher"
            | "LoadingBay"
            | "CargoPod"
            | "PulseMine"
            | "SupplyCrate"
            | "AshPlateau"
            | "TerrainSaddle"
            | "FieldScrap"
            | "WatchPost"
            | "OreSilo"
            | "RelayTower"
            | "RadarDome"
            | "Player"
            | "WaterBody"
            | "Ground"
            | "StrategyCamera"
            | "MainCamera"
            | "SolarFlareBuoy"
            | "WarpGateRing"
            | "SignalSat"
            | "BeaconBuoy"
            | "RescuePod"
            | "CometFragment"
            | "DebrisRing"
    ) || name.starts_with("FieldScrap")
        || name.starts_with("RockOutcrop")
        || name.starts_with("CliffRidge")
        || name.starts_with("Terrain")
        || name.starts_with("Nebula")
}

/// Structural demotion (T1): BRP parent child, co-located multipart, or string fallback.
pub fn is_structural_child_of_named_root(s: &EyesightSubject, all: &[EyesightSubject]) -> bool {
    if is_protected_top_level_name(&s.name) {
        return false;
    }
    if let Some(pid) = s.parent_entity {
        if all.iter().any(|p| {
            p.entity == Some(pid)
                && !is_child_mesh_part(&p.name)
                && p.name != "unnamed"
                && !p.name.starts_with("Star")
        }) {
            return true;
        }
    }
    if let Some(t) = s.translation {
        let my_score = gameplay_subject_score(&s.name);
        for p in all {
            if p.entity == s.entity {
                continue;
            }
            if !is_protected_top_level_name(&p.name) {
                continue;
            }
            let Some(pt) = p.translation else {
                continue;
            };
            let dx = t[0] - pt[0];
            let dz = t[2] - pt[2];
            let dist_xz = (dx * dx + dz * dz).sqrt();
            if dist_xz < 4.0 && gameplay_subject_score(&p.name) > my_score + 20 {
                if is_child_mesh_part(&s.name) || looks_like_multipart_suffix(&s.name) {
                    return true;
                }
            }
        }
    }
    is_child_mesh_part(&s.name)
}

/// Noise for filter: name noise OR structural child (needs full subject list).
pub fn is_noise_subject(s: &EyesightSubject, all: &[EyesightSubject]) -> bool {
    is_noise_name(&s.name) || is_structural_child_of_named_root(s, all)
}

fn looks_like_multipart_suffix(name: &str) -> bool {
    name.ends_with("Chassis")
        || name.ends_with("Hopper")
        || name.ends_with("JawL")
        || name.ends_with("JawR")
        || name.contains("Shard")
        || name.starts_with("Crusher")
        || name.starts_with("Belt")
}

/// Local-space child mesh Names — string fallback when BRP parent missing.
pub fn is_child_mesh_part(name: &str) -> bool {
    if is_protected_top_level_name(name) {
        return false;
    }
    const PARTS: &[&str] = &[
        "WatchPostLegs",
        "WatchPostDeck",
        "WatchPostCabin",
        "OreSiloBody",
        "OreSiloCap",
        "RelayMast",
        "RelayDish",
        "TreeTrunk",
        "TreeCanopy",
        "InserterArm",
        "DrillBit",
        "AsmRotor",
        "Turbine",
        // F2 multiparts
        "CrusherChassis",
        "CrusherHopper",
        "CrusherFlywheel",
        "CrusherChute",
        "CliffFace",
        "CliffLedge",
        "CliffButtress",
        "CliffCap",
        "CrateLid",
        "CrateBand",
        "AshBerm",
        "AshMound",
        "SaddleAbutL",
        "SaddleAbutR",
        "SaddleCrown",
        "ScrapShardA",
        "ScrapShardB",
        "ScrapPipe",
        "RockPeak",
        "RockShelf",
    ];
    PARTS.iter().any(|p| name == *p)
        || name.starts_with("CrusherJaw")
        || name.starts_with("CrateFoot")
        || name.starts_with("ChestFoot")
        || name.starts_with("LoadingBayPost")
        || (name.starts_with("LoadingBay") && name != "LoadingBay")
        || (name.starts_with("Drill") && name != "Drill" && !name.contains("Tower"))
        || name.starts_with("Belt")
        || name.starts_with("Accu")
        || name.starts_with("Asm")
        || name.starts_with("SolarStand")
        || name.starts_with("SolarFrame")
        || name.starts_with("SolarGlass")
        || name.starts_with("SolarGrid")
        || name.starts_with("Furnace")
        || name.starts_with("Boiler")
        || name.starts_with("Engine")
        || name.starts_with("Pump")
        || name.starts_with("Inserter")
        || name.starts_with("Chest")
        || name.starts_with("Elec")
        || name.starts_with("Burner")
        || name.ends_with("Legs")
        || name.ends_with("Deck")
        || name.ends_with("Cabin")
        || name.ends_with("Mast")
        || name.ends_with("Dish")
        || name.ends_with("Trunk")
        || name.ends_with("Canopy")
        || name.ends_with("Rotor")
        || name.ends_with("Turbine")
        || name.ends_with("Shell")
        || name.ends_with("Base")
        || name.ends_with("Hopper")
        || name.ends_with("Chassis")
        || name.ends_with("Flywheel")
        || name.ends_with("Chute")
        || name.ends_with("Ledge")
        || name.ends_with("Buttress")
        || name.ends_with("Berm")
        || name.ends_with("Mound")
        || name.ends_with("Crown")
        || name.ends_with("Pipe")
        || name.ends_with("ShardA")
        || name.ends_with("ShardB")
        || name.ends_with("Shelf")
        || name.ends_with("Peak") && name.starts_with("Rock")
        // "…Body" / "…Cap" only when compound (e.g. OreSiloBody), not WaterBody alone
        || (name.ends_with("Body") && name != "WaterBody" && name.len() > "Body".len())
        || (name.ends_with("Cap") && name != "Cap" && name.contains("Silo") || name.ends_with("SiloCap"))
        || name.ends_with("Foot0")
        || name.ends_with("Foot1")
        || name.ends_with("Foot2")
        || name.ends_with("Foot3")
}

/// Collapse identical Names; keep first entity, set `duplicate_count` (S0.2).
pub fn collapse_duplicate_names(subjects: Vec<EyesightSubject>) -> Vec<EyesightSubject> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, EyesightSubject> =
        std::collections::HashMap::new();
    for s in subjects {
        let key = s.name.clone();
        if let Some(existing) = map.get_mut(&key) {
            let prev = existing.duplicate_count.unwrap_or(1);
            existing.duplicate_count = Some(prev + 1);
        } else {
            order.push(key.clone());
            let mut s = s;
            s.duplicate_count = Some(1);
            map.insert(key, s);
        }
    }
    order
        .into_iter()
        .filter_map(|k| map.remove(&k))
        .map(|mut s| {
            if s.duplicate_count == Some(1) {
                s.duplicate_count = None;
            }
            s
        })
        .collect()
}

/// Known multi-view pack names (2D + 3D + shared).
pub const KNOWN_PACKS: &[&str] = &[
    "entity_craft",
    "landscape",
    "water",
    "physics_jump",
    "lighting",
    "diagnostic",
    // D0 2D-specific packs
    "hud",
    "env_2d",
];

/// Height-band Name stems used for 3D landscape readability notes (D0.3 / D2).
pub const HEIGHT_BAND_NAME_HINTS: &[&str] = &[
    "TerrainFlat",
    "TerrainHill",
    "TerrainPeak",
    "HeightTerrain",
    "Terrain",
];

/// Named region presets for pixel crops (HUD, horizon band, etc.). Pure geometry — no BRP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionPreset {
    /// Top-left HUD strip (score/fuel/objective chrome).
    HudTopLeft,
    /// Full top HUD bar.
    HudTopBar,
    /// Upper third horizon/sky band (2D parallax / 3D ridgeline).
    HorizonBand,
    /// Lower third ground/surface band.
    GroundBand,
    /// Center half of the frame (general fovea-ish region without entity).
    CenterHalf,
}

impl RegionPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace('-', "_").as_str() {
            "hud" | "hud_top_left" | "hud_tl" => Some(Self::HudTopLeft),
            "hud_top" | "hud_bar" | "hud_top_bar" => Some(Self::HudTopBar),
            "horizon" | "horizon_band" | "sky" => Some(Self::HorizonBand),
            "ground" | "ground_band" | "surface" => Some(Self::GroundBand),
            "center" | "center_half" => Some(Self::CenterHalf),
            _ => None,
        }
    }

    /// Pixel rect `(x, y, w, h)` for a given frame size.
    pub fn rect(self, screen_w: u32, screen_h: u32) -> (u32, u32, u32, u32) {
        let w = screen_w.max(1);
        let h = screen_h.max(1);
        match self {
            Self::HudTopLeft => {
                let rw = (w / 3).max(64).min(w);
                let rh = (h / 5).max(48).min(h);
                (0, 0, rw, rh)
            }
            Self::HudTopBar => {
                let rh = (h / 6).max(40).min(h);
                (0, 0, w, rh)
            }
            Self::HorizonBand => {
                let rh = (h / 3).max(1);
                (0, 0, w, rh)
            }
            Self::GroundBand => {
                let rh = (h / 3).max(1);
                let y = h.saturating_sub(rh);
                (0, y, w, rh)
            }
            Self::CenterHalf => {
                let rw = (w / 2).max(1);
                let rh = (h / 2).max(1);
                let x = w.saturating_sub(rw) / 2;
                let y = h.saturating_sub(rh) / 2;
                (x, y, rw, rh)
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::HudTopLeft => "hud_top_left",
            Self::HudTopBar => "hud_top_bar",
            Self::HorizonBand => "horizon_band",
            Self::GroundBand => "ground_band",
            Self::CenterHalf => "center_half",
        }
    }
}

/// Resolve a region preset name to pixel rect for `see_region` (unit-testable pure helper).
pub fn region_preset_rect(preset: &str, screen_w: u32, screen_h: u32) -> Option<(u32, u32, u32, u32)> {
    RegionPreset::parse(preset).map(|p| p.rect(screen_w, screen_h))
}

/// Subjects that look like height-band terrain Names (flat / hill / peak).
pub fn height_band_subjects(subjects: &[EyesightSubject]) -> Vec<String> {
    subjects
        .iter()
        .filter(|s| {
            HEIGHT_BAND_NAME_HINTS
                .iter()
                .any(|h| s.name == *h || s.name.starts_with(h) || s.name.contains(h))
        })
        .map(|s| s.name.clone())
        .collect()
}

/// Note for landscape packs when height-band Names are present (D0.3).
pub fn height_readability_note(subjects: &[EyesightSubject]) -> Option<String> {
    let bands = height_band_subjects(subjects);
    if bands.is_empty() {
        return None;
    }
    Some(format!(
        "height_bands present in subjects ({}); landscape full+alt should show relief when terrain has hills/mountains",
        bands.join(", ")
    ))
}

/// True if pack name is a known eyesight pack (including D0 2D packs).
pub fn is_known_pack(pack: &str) -> bool {
    let p = pack.to_ascii_lowercase();
    KNOWN_PACKS.iter().any(|k| *k == p)
        || p == "entity"
        || p == "physics"
}

/// Stems that must score >0 (Name onboarding). Used by unit tests + skill docs.
pub fn dogfood_stems_needing_positive_score() -> &'static [&'static str] {
    DOGFOOD_NAME_STEMS
}

/// True when every dogfood stem scores >0 under the shipped scorer.
pub fn all_dogfood_stems_score_positive() -> bool {
    DOGFOOD_NAME_STEMS
        .iter()
        .all(|s| gameplay_subject_score(s) > 0)
}

/// Pure helper: camera translation nudge for landscape/water alt views (R0 / R3 debt).
/// Top-down 3D uses **large side XZ offset + lift** so alt differs from game more often than pure Y-lift.
/// Returns (x, y, z) absolute translation.
pub fn alt_camera_nudge_translation(
    cam: [f64; 3],
    topdown3d: bool,
) -> [f64; 3] {
    if topdown3d {
        // Aggressive side/orbit so mean_abs_diff is clearly non-trivial for height maps
        [
            cam[0] + 22.0,
            (cam[1] + 32.0).max(36.0),
            cam[2] + 18.0,
        ]
    } else {
        // 2D ortho: pan + slight lift in world units
        [cam[0] + 220.0, cam[1] + 100.0, cam[2]]
    }
}

/// Threshold for perceptual multi-view similarity (mean abs channel delta 0..1).
/// Below this, alt ≈ game — warn `views_similar` even when file hashes differ slightly.
pub const VIEWS_SIMILAR_MEAN_ABS_MAX: f32 = 0.02;

/// Minimum non-black fraction for a "readable" full-frame Playing capture (CD env).
pub const FULL_FRAME_NONBLACK_MIN: f32 = 0.08;
/// Entity fovea crop: craft-readable nonblack fraction (with [`FOVEA_CRAFT_LUMA_MIN`]).
/// Deep-shadow OreCrusher crops measure ~0.28 at thr=30 but still unreadable — use
/// stricter craft luma + mean gate (S3 skeptic gap).
pub const FOVEA_CROP_NONBLACK_MIN: f32 = 0.35;
/// Luma threshold for "craft-visible" pixels in fovea (stricter than env nonblack thr 30).
pub const FOVEA_CRAFT_LUMA_MIN: u8 = 48;
/// Mean Rec.601 luma below this → fovea_dark (deep shadow / underexposed crop).
pub const FOVEA_CROP_MEAN_LUMA_MIN: f32 = 50.0;

/// True-magenta plate gate: sprites must have zero such pixels.
/// Strict: R>200, B>200, G<40 (excludes purple craft with moderate G).
pub const TRUE_MAGENTA_MAX_ON_SPRITE: u32 = 0;

/// Mean Rec.601 luma 0..255 over opaque-ish pixels (all pixels if no alpha gate).
pub fn png_mean_luma(png_bytes: &[u8]) -> Result<f32> {
    let img = RgbaImage::decode_png(png_bytes)?;
    let n = (img.width as usize).saturating_mul(img.height as usize);
    if n == 0 {
        return Ok(0.0);
    }
    let mut sum = 0u64;
    for i in (0..img.pixels.len()).step_by(4) {
        let r = img.pixels[i] as u64;
        let g = img.pixels[i + 1] as u64;
        let b = img.pixels[i + 2] as u64;
        sum += (54 * r + 183 * g + 19 * b) / 256;
    }
    Ok(sum as f32 / n as f32)
}

/// Pure S3 gate: fovea crop too dark/shadowed to judge craft (even if aim is correct).
/// Uses craft luma fraction **and** mean luma so thr=30 "0.28 nonblack" shadow still warns.
pub fn fovea_crop_is_too_dark(png_bytes: &[u8]) -> Result<bool> {
    let frac = png_nonblack_fraction(png_bytes, FOVEA_CRAFT_LUMA_MIN)?;
    let mean = png_mean_luma(png_bytes)?;
    Ok(frac < FOVEA_CROP_NONBLACK_MIN || mean < FOVEA_CROP_MEAN_LUMA_MIN)
}

pub fn path_fovea_crop_is_too_dark(path: impl AsRef<std::path::Path>) -> Result<bool> {
    fovea_crop_is_too_dark(&fs::read(path.as_ref())?)
}

/// Fraction of pixels that are not near-black (luma > threshold).
/// Drives full-frame env readability gates (Names ≠ pixels).
pub fn png_nonblack_fraction(png_bytes: &[u8], luma_min: u8) -> Result<f32> {
    let img = RgbaImage::decode_png(png_bytes)?;
    let n = (img.width as usize).saturating_mul(img.height as usize);
    if n == 0 {
        return Ok(0.0);
    }
    let mut bright = 0usize;
    for i in (0..img.pixels.len()).step_by(4) {
        let r = img.pixels[i] as u32;
        let g = img.pixels[i + 1] as u32;
        let b = img.pixels[i + 2] as u32;
        // Rec.601-ish luma
        let luma = (54 * r + 183 * g + 19 * b) / 256;
        if luma > luma_min as u32 {
            bright += 1;
        }
    }
    Ok(bright as f32 / n as f32)
}

/// Count opaque true-magenta plate pixels (keying failures / purple squares).
/// Strict: R>200, G<40, B>200, A>200 — not soft purple craft.
pub fn png_true_magenta_pixel_count(png_bytes: &[u8]) -> Result<u32> {
    let img = RgbaImage::decode_png(png_bytes)?;
    let mut n = 0u32;
    for i in (0..img.pixels.len()).step_by(4) {
        let r = img.pixels[i];
        let g = img.pixels[i + 1];
        let b = img.pixels[i + 2];
        let a = img.pixels[i + 3];
        if a > 200 && r > 200 && g < 40 && b > 200 {
            n = n.saturating_add(1);
        }
    }
    Ok(n)
}

/// Path convenience for sprite inventory audit.
pub fn path_true_magenta_pixel_count(path: &Path) -> Result<u32> {
    png_true_magenta_pixel_count(&fs::read(path)?)
}

/// Path convenience for full-frame nonblack gate.
pub fn path_nonblack_fraction(path: &Path, luma_min: u8) -> Result<f32> {
    png_nonblack_fraction(&fs::read(path)?, luma_min)
}

/// True when two PNGs are perceptually nearly identical (exact hash OR mean abs < threshold).
pub fn captures_look_similar(path_a: &Path, path_b: &Path) -> Result<bool> {
    if file_content_hash(path_a)? == file_content_hash(path_b)? {
        return Ok(true);
    }
    let a = RgbaImage::decode_png(&fs::read(path_a)?)?;
    let b_raw = RgbaImage::decode_png(&fs::read(path_b)?)?;
    let b = if b_raw.width != a.width || b_raw.height != a.height {
        scale_nearest(&b_raw, a.width, a.height)?
    } else {
        b_raw
    };
    let score = mean_abs_diff(&a, &b)?;
    Ok(score < VIEWS_SIMILAR_MEAN_ABS_MAX)
}

/// Prefer a pre-placed secondary camera Name if present (S2 multi-view upgrade path).
/// Returns entity id + translation when a dedicated side view camera exists in subjects.
pub fn find_dedicated_view_camera(
    subjects: &[EyesightSubject],
) -> Option<(u64, [f64; 3])> {
    const NAMES: &[&str] = &[
        "AgentSightSideCamera",
        "SideCamera",
        "SightSideCamera",
        "MultiViewCamera",
    ];
    for prefer in NAMES {
        if let Some(s) = subjects.iter().find(|s| s.name == *prefer) {
            if let (Some(e), Some(t)) = (s.entity, s.translation) {
                return Some((e, t));
            }
        }
    }
    None
}

/// Pure helper: aggressive **side/orbit** camera translation when first alt is still similar.
/// Places camera well off-axis so height maps should differ under top-down strategy views.
/// High strategy Y gets a lower orbit and larger lateral offset (F0 fidelity residual).
pub fn side_orbit_camera_translation(cam: [f64; 3], topdown3d: bool) -> [f64; 3] {
    if topdown3d {
        let high = cam[1] > 30.0;
        let lat = if high { 42.0 } else { 28.0 };
        let z_off = if high { 38.0 } else { 26.0 };
        // Drop well below strategy altitude for parallax on heightfields.
        let y = if high {
            (cam[1] * 0.45).clamp(14.0, 22.0)
        } else {
            (cam[1] + 8.0).max(18.0).min(cam[1].max(18.0))
        };
        [cam[0] - lat, y, cam[2] + z_off]
    } else {
        [cam[0] - 320.0, cam[1] + 40.0, cam[2]]
    }
}

/// Tall prop / peak Names get a larger fovea half (R0 / R4 debt).
pub const TALL_ENTITY_PREFIXES: &[&str] = &[
    "WatchPost",
    "OreSilo",
    "Relay",
    "TerrainPeak",
    "RadarDome",
    "TerrainSaddle",
    "CliffRidge",
    "WarpGate",
    "OreCrusher",
    "SignalRelay",
    "LoadingBay",
];

/// Default crop half for entity fovea; larger for tall silhouettes.
pub fn crop_half_for_entity(entity_name: &str, requested_half: u32) -> u32 {
    let base = if requested_half == 0 {
        DEFAULT_CROP_HALF
    } else {
        requested_half
    };
    // Only inflate when caller used default-ish half (≤ DEFAULT_CROP_HALF)
    if base > DEFAULT_CROP_HALF {
        return base;
    }
    if TALL_ENTITY_PREFIXES
        .iter()
        .any(|p| entity_name.starts_with(p) || entity_name.contains(p))
    {
        base.saturating_mul(3).saturating_div(2).max(base + 32) // ~1.5×, min +32
    } else {
        base
    }
}

/// Apply named game profile defaults into SeeOptions (S0.3). Explicit non-empty waits already set are kept if non-empty.
pub fn apply_game_profile(opts: &mut SeeOptions, profile: &str) {
    match profile.to_ascii_lowercase().as_str() {
        "crystal-drift" | "crystal_drift" | "cd" | "ortho2d" | "2d" => {
            opts.projection = ProjectionMode::Ortho2d;
            // Half-extents match common 1280×720 ortho arenas (full width/height ≈ 1280×720 world units).
            opts.visible_half_w = 640.0;
            opts.visible_half_h = 360.0;
            opts.require_playing = false;
            if opts.wait_for_subjects.is_empty() {
                opts.wait_for_subjects = vec!["Player".into()];
            }
            opts.subject_class = if opts.subject_class == "scene" {
                "scene".into()
            } else {
                opts.subject_class.clone()
            };
        }
        "iron-feud" | "iron_feud" | "if" | "topdown3d" | "topdown" | "3d" => {
            opts.projection = ProjectionMode::TopDown3d;
            // Slightly wider half-extents so tall props (WatchPost ~3u) stay in fovea projection (R0.3).
            opts.visible_half_w = 22.0;
            opts.visible_half_h = 22.0;
            opts.require_playing = true;
            if opts.wait_for_subjects.is_empty() {
                opts.wait_for_subjects = vec![
                    "StrategyCamera".into(),
                    "WaterBody".into(),
                    "Ground".into(),
                ];
            }
        }
        _ => {
            // default: leave as-is for projection/wait unless empty defaults
            if opts.visible_half_w <= 0.0 {
                opts.visible_half_w = 640.0;
            }
            if opts.visible_half_h <= 0.0 {
                opts.visible_half_h = 360.0;
            }
        }
    }
}

/// Diagnostic / env allowlist when no ranked Player (S0.8). Prefer env over OreCrystal.
pub fn diagnostic_primary_name(subjects: &[EyesightSubject]) -> String {
    if let Some(r) = rank_primary_subject(subjects) {
        return r;
    }
    for n in [
        "WaterBody",
        "StrategyCamera",
        "FieldScrap_A",
        "Ground",
        "HeightTerrain",
        "TerrainFlat",
        "DerelictStation",
        "Player",
        "BeaconBuoy",
        "RelayTower",
        "WatchPost",
        "CometFragment",
        "SignalSat",
    ] {
        if subjects.iter().any(|s| s.name == n || s.name.contains(n)) {
            return n.to_string();
        }
    }
    subjects
        .first()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Player".into())
}

/// File hash for multi-view comparison (S0.7).
pub fn file_content_hash(path: &Path) -> Result<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let bytes = fs::read(path)?;
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    Ok(h.finish())
}

/// Score for gameplay preference sort (higher first).
pub fn gameplay_subject_score(name: &str) -> i32 {
    let mut score = 0i32;
    // Exact primary Names get a hard boost so they survive max_subjects caps.
    for exact in PRIMARY_EXACT {
        if name == *exact {
            score += 200;
        }
    }
    // Exact dogfood feature stems (F1/F2 new Names) outrank multiparts / generic chests.
    for stem in DOGFOOD_NAME_STEMS {
        if name == *stem {
            score += 120;
        } else if name.starts_with(stem) {
            // FieldScrap_A etc. — keep parent scrap piles ranked.
            score += 90;
        }
    }
    for hint in GAMEPLAY_NAME_HINTS {
        if name == *hint {
            score += 100;
        } else if name.contains(hint) {
            score += 40;
        }
    }
    // Terrain height-band Names must survive IF landscape filter.
    for h in HEIGHT_BAND_NAME_HINTS {
        if name == *h || name.starts_with(h) {
            score += 60;
        }
    }
    if name.starts_with("Star") || name.contains("Particle") || name.contains("parallax") {
        score -= 80;
    }
    // OreCrystal* is gameplay noise for sight primary/filter (contains "Ore"+"Crystal").
    if name.starts_with("OreCrystal") {
        score -= 160;
    }
    // Child mesh parts (local 0,0,0) crowd subject slots — demote hard.
    if is_child_mesh_part(name) || name == "OwnershipFlag" {
        score -= 200;
    }
    // Machine display Names with spaces are useful but must not outrank OreCrusher dogfood.
    if name.contains(' ') {
        score -= 30;
    }
    // DrillTower is a child tower part name, not a top-level feature.
    if name == "DrillTower" {
        score -= 150;
    }
    if name == "unnamed" {
        score -= 50;
    }
    score
}

/// Filter and cap subjects (A4).
pub fn filter_subjects(
    subjects: Vec<EyesightSubject>,
    mode: SubjectFilterMode,
    max: usize,
) -> (Vec<EyesightSubject>, bool) {
    let max = max.max(1);
    match mode {
        SubjectFilterMode::All => {
            let truncated = subjects.len() > max;
            (subjects.into_iter().take(max).collect(), truncated)
        }
        SubjectFilterMode::NamesOnly => {
            let mut v: Vec<_> = subjects
                .into_iter()
                .filter(|s| s.name != "unnamed" && !s.name.is_empty())
                .collect();
            v.sort_by(|a, b| {
                gameplay_subject_score(&b.name).cmp(&gameplay_subject_score(&a.name))
            });
            let truncated = v.len() > max;
            v.truncate(max);
            (v, truncated)
        }
        SubjectFilterMode::GameplayPrefer => {
            let all_ctx = subjects.clone();
            let mut preferred: Vec<_> = subjects
                .iter()
                .filter(|s| {
                    !is_noise_subject(s, &all_ctx) && gameplay_subject_score(&s.name) > 0
                })
                .cloned()
                .collect();
            preferred.sort_by(|a, b| {
                gameplay_subject_score(&b.name).cmp(&gameplay_subject_score(&a.name))
            });
            if preferred.is_empty() {
                let mut all: Vec<_> = subjects
                    .into_iter()
                    .filter(|s| !is_noise_subject(s, &all_ctx))
                    .collect();
                all.sort_by(|a, b| {
                    gameplay_subject_score(&b.name).cmp(&gameplay_subject_score(&a.name))
                });
                let truncated = all.len() > max;
                all.truncate(max);
                return (all, truncated);
            }
            let truncated = preferred.len() > max;
            preferred.truncate(max);
            (preferred, truncated)
        }
    }
}

/// Infer rough app state from subject names (A0).
pub fn infer_app_state_from_subjects(subjects: &[EyesightSubject]) -> Option<String> {
    let names: Vec<&str> = subjects.iter().map(|s| s.name.as_str()).collect();
    let menu = names.iter().any(|n| n.contains("Menu"));
    let playing = names.iter().any(|n| {
        *n == "Player"
            || *n == "StrategyCamera"
            || n.contains("WaterBody")
            || n.contains("Ground")
            || n.contains("Nebula")
    });
    if playing && !menu {
        Some("Playing".into())
    } else if menu && !playing {
        Some("MainMenu".into())
    } else if playing {
        Some("Playing".into())
    } else {
        None
    }
}

/// True if subjects look menu-only (A0 fail-fast signal).
pub fn subjects_look_menu_only(subjects: &[EyesightSubject]) -> bool {
    if subjects.is_empty() {
        return false;
    }
    let has_menu = subjects.iter().any(|s| s.name.contains("Menu"));
    let has_play = subjects.iter().any(|s| {
        s.name == "Player"
            || s.name == "StrategyCamera"
            || s.name.contains("WaterBody")
            || s.name.contains("RockOutcrop")
            || s.name.contains("Ground")
    });
    has_menu && !has_play
}

/// Annotate subjects with screen projection (A1).
pub fn annotate_subjects_projection(
    subjects: &mut [EyesightSubject],
    cam: [f64; 3],
    mode: ProjectionMode,
    visible_half_w: f64,
    visible_half_h: f64,
    screen_w: u32,
    screen_h: u32,
    half_extent_px: u32,
) {
    for s in subjects.iter_mut() {
        let Some(t) = s.translation else { continue };
        let (sx, sy) = match mode {
            ProjectionMode::Ortho2d => world_to_screen_ortho(
                t[0],
                t[1],
                cam[0],
                cam[1],
                visible_half_w,
                visible_half_h,
                screen_w,
                screen_h,
            ),
            ProjectionMode::TopDown3d => world_to_screen_ortho(
                t[0],
                t[2],
                cam[0],
                cam[2],
                visible_half_w,
                visible_half_h,
                screen_w,
                screen_h,
            ),
        };
        let on = sx > 0 && sy > 0 && sx < screen_w.saturating_sub(1) && sy < screen_h.saturating_sub(1);
        // margin: treat near-edge as on-screen
        let on = on
            || (sx as i32 - half_extent_px as i32) < screen_w as i32
                && (sy as i32 - half_extent_px as i32) < screen_h as i32;
        s.screen_xy = Some([sx, sy]);
        let he = half_extent_px;
        let x0 = sx.saturating_sub(he);
        let y0 = sy.saturating_sub(he);
        s.screen_aabb = Some([x0, y0, he.saturating_mul(2), he.saturating_mul(2)]);
        s.on_screen = Some(on);
        s.on_screen_estimate = Some(on);
    }
}

/// Pick camera translation from subjects (MainCamera / StrategyCamera / *Camera*).
pub fn find_camera_translation(subjects: &[EyesightSubject]) -> Option<[f64; 3]> {
    for prefer in ["MainCamera", "StrategyCamera"] {
        if let Some(s) = subjects.iter().find(|s| s.name == prefer) {
            if let Some(t) = s.translation {
                return Some(t);
            }
        }
    }
    subjects
        .iter()
        .find(|s| s.name.contains("Camera") && !s.name.contains("Menu"))
        .and_then(|s| s.translation)
}

/// Wait until BRP subjects include any of `expected` (A0).
pub fn wait_for_subject_names(
    client: &BrpClient,
    expected: &[String],
    timeout: Duration,
) -> Result<Vec<EyesightSubject>> {
    if expected.is_empty() {
        return Ok(query_all_subjects(client));
    }
    let start = std::time::Instant::now();
    let mut last = Vec::new();
    while start.elapsed() < timeout {
        last = query_all_subjects(client);
        let ok = expected.iter().any(|e| {
            last.iter()
                .any(|s| s.name == *e || s.name.contains(e.as_str()))
        });
        if ok {
            return Ok(last);
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(anyhow!(
        "wait_for_subjects timeout after {:?}; expected one of {:?}; last names sample: {:?}",
        timeout,
        expected,
        last.iter().map(|s| s.name.as_str()).take(12).collect::<Vec<_>>()
    ))
}

pub fn query_all_subjects(client: &BrpClient) -> Vec<EyesightSubject> {
    let comps = [
        "bevy_ecs::name::Name",
        "bevy_transform::components::transform::Transform",
    ];
    let mut subjects = if let Ok(q) = client.query(&comps) {
        subjects_from_query(&q)
    } else if let Ok(q) = client.query(&["Name", "Transform"]) {
        subjects_from_query(&q)
    } else {
        Vec::new()
    };
    // S1: ChildOf is often AND-filtered out of multi-component queries — merge from a
    // second query (live IF: ChildOf-only returns ~100 rows; Name+Transform+ChildOf → 0).
    merge_childof_parents(client, &mut subjects);
    subjects
}

/// Merge parent ids from a ChildOf-only BRP query into subjects by entity id (S1).
fn merge_childof_parents(client: &BrpClient, subjects: &mut [EyesightSubject]) {
    let child_queries = [
        ["bevy_ecs::hierarchy::ChildOf"].as_slice(),
        ["ChildOf"].as_slice(),
    ];
    for comps in child_queries {
        if let Ok(q) = client.query(comps) {
            let map = parent_map_from_childof_query(&q);
            if map.is_empty() {
                continue;
            }
            for s in subjects.iter_mut() {
                if s.parent_entity.is_some() {
                    continue;
                }
                if let Some(eid) = s.entity {
                    if let Some(&pid) = map.get(&eid) {
                        s.parent_entity = Some(pid);
                    }
                }
            }
            return;
        }
    }
}

/// Pure helper: entity_id → parent_id from a ChildOf-only `world.query` result (S1).
pub fn parent_map_from_childof_query(query: &Value) -> std::collections::HashMap<u64, u64> {
    let mut map = std::collections::HashMap::new();
    let rows = match query {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .or_else(|| o.get("entities").and_then(|e| e.as_array()).cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for row in rows {
        let entity = row
            .get("entity")
            .and_then(|e| e.as_u64())
            .or_else(|| row.get("id").and_then(|e| e.as_u64()));
        let Some(eid) = entity else { continue };
        let comps = row
            .get("components")
            .cloned()
            .unwrap_or_else(|| row.clone());
        if let Some(pid) = extract_parent_entity(&comps) {
            map.insert(eid, pid);
        } else if let Some(obj) = comps.as_object() {
            // Bare ChildOf:u64 under component key
            for (k, v) in obj {
                if k.contains("ChildOf") || k.ends_with("Parent") {
                    if let Some(pid) = v.as_u64() {
                        map.insert(eid, pid);
                    }
                }
            }
        }
    }
    map
}

/// Draw a simple 1px colored rectangle outline (diagnostic bounds, A6).
pub fn draw_rect_outline(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, rgba: [u8; 4]) {
    if w == 0 || h == 0 {
        return;
    }
    let x2 = (x + w).min(img.width);
    let y2 = (y + h).min(img.height);
    let x1 = x.min(img.width.saturating_sub(1));
    let y1 = y.min(img.height.saturating_sub(1));
    for px in x1..x2 {
        set_px(img, px, y1, rgba);
        if y2 > 0 {
            set_px(img, px, y2.saturating_sub(1), rgba);
        }
    }
    for py in y1..y2 {
        set_px(img, x1, py, rgba);
        if x2 > 0 {
            set_px(img, x2.saturating_sub(1), py, rgba);
        }
    }
}

fn set_px(img: &mut RgbaImage, x: u32, y: u32, rgba: [u8; 4]) {
    if x >= img.width || y >= img.height {
        return;
    }
    let i = ((y * img.width + x) * 4) as usize;
    if i + 3 < img.pixels.len() {
        img.pixels[i..i + 4].copy_from_slice(&rgba);
    }
}

fn extract_name(comps: &Value) -> Option<String> {
    if let Some(obj) = comps.as_object() {
        for (k, v) in obj {
            if k.ends_with("Name") || k == "Name" || k.contains("name::Name") {
                if let Some(s) = v.as_str() {
                    return Some(s.to_string());
                }
                if let Some(s) = v.get("name").and_then(|n| n.as_str()) {
                    return Some(s.to_string());
                }
                if let Some(s) = v.get("0").and_then(|n| n.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

fn extract_translation(comps: &Value) -> Option<[f64; 3]> {
    let obj = comps.as_object()?;
    for (k, v) in obj {
        if k.ends_with("Transform") || k == "Transform" || k.contains("transform::Transform") {
            let t = v.get("translation")?;
            if let Some(arr) = t.as_array() {
                if arr.len() >= 3 {
                    return Some([
                        arr[0].as_f64().unwrap_or(0.0),
                        arr[1].as_f64().unwrap_or(0.0),
                        arr[2].as_f64().unwrap_or(0.0),
                    ]);
                }
            }
            // {x,y,z}
            let x = t.get("x").and_then(|v| v.as_f64())?;
            let y = t.get("y").and_then(|v| v.as_f64())?;
            let z = t.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0);
            return Some([x, y, z]);
        }
    }
    None
}

/// Extract parent entity from ChildOf / Parent when BRP reflects them (T1).
pub fn extract_parent_entity(comps: &Value) -> Option<u64> {
    let obj = comps.as_object()?;
    for (k, v) in obj {
        let key = k.as_str();
        if !(key.ends_with("ChildOf")
            || key.ends_with("Parent")
            || key.contains("hierarchy::ChildOf")
            || key.contains("hierarchy::Parent")
            || key == "ChildOf"
            || key == "Parent")
        {
            continue;
        }
        if let Some(id) = v.as_u64() {
            return Some(id);
        }
        for field in ["0", "parent", "entity", "Parent", "ChildOf"] {
            if let Some(id) = v.get(field).and_then(|x| x.as_u64()) {
                return Some(id);
            }
        }
        if let Some(id) = v.get("index").and_then(|x| x.as_u64()) {
            return Some(id);
        }
    }
    None
}

/// Pure helper: pack path restore payload for original camera translation (T2).
pub fn pack_camera_restore_value(original: [f64; 3]) -> Value {
    translation_value_for_brp(&json!({
        "x": original[0],
        "y": original[1],
        "z": original[2],
    }))
}

/// Dedicated side/orbit placement when strategy alt + side-orbit still similar (T3).
pub fn dedicated_side_camera_translation(cam: [f64; 3], topdown3d: bool) -> [f64; 3] {
    if topdown3d {
        [
            cam[0] - 55.0,
            (cam[1] * 0.35).clamp(10.0, 16.0),
            cam[2] + 48.0,
        ]
    } else {
        [cam[0] - 480.0, cam[1] - 20.0, cam[2]]
    }
}

/// Map world XY to screen for a simple orthographic camera centered at (cam_x, cam_y).
pub fn world_to_screen_ortho(
    world_x: f64,
    world_y: f64,
    cam_x: f64,
    cam_y: f64,
    visible_half_w: f64,
    visible_half_h: f64,
    screen_w: u32,
    screen_h: u32,
) -> (u32, u32) {
    let nx = ((world_x - cam_x) / (2.0 * visible_half_w) + 0.5).clamp(0.0, 1.0);
    let ny = (0.5 - (world_y - cam_y) / (2.0 * visible_half_h)).clamp(0.0, 1.0);
    let sx = (nx * screen_w as f64) as u32;
    let sy = (ny * screen_h as f64) as u32;
    (sx.min(screen_w.saturating_sub(1)), sy.min(screen_h.saturating_sub(1)))
}

// ── High-level see_* operations ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SeeOptions {
    pub out_dir: PathBuf,
    pub subject_class: String,
    pub intent: String,
    pub style_intent: Option<String>,
    pub app_state: Option<String>,
    pub target_name: Option<String>,
    pub port: Option<u16>,
    /// A4 subject filter.
    pub subject_filter: SubjectFilterMode,
    pub max_subjects: usize,
    /// A0: wait until these name substrings appear.
    pub wait_for_subjects: Vec<String>,
    pub wait_timeout_secs: u64,
    /// Require Playing-like subjects (fail if menu-only).
    pub require_playing: bool,
    /// A1 projection.
    pub projection: ProjectionMode,
    pub visible_half_w: f64,
    pub visible_half_h: f64,
    /// A5: after capture, also write diff vs this baseline.
    pub compare_baseline: Option<PathBuf>,
    /// A5: copy scene full to this path as session baseline.
    pub save_baseline_as: Option<PathBuf>,
    /// A1: also write 2× zoom crop.
    pub zoom_ladder: bool,
    /// A6: draw bounds on diagnostic capture.
    pub diagnostic_bounds: bool,
    /// Game profile name applied (crystal-drift | iron-feud | default).
    pub profile: Option<String>,
    /// When true, see_scene also attaches primary fovea (+ zoom).
    pub include_primary_fovea: bool,
    /// Motion: optional entity id to mutate before strip.
    pub motion_mutate_entity: Option<u64>,
    /// Motion: translation value JSON object {x,y,z}.
    pub motion_mutate_translation: Option<Value>,
    /// If save_baseline requested with no path, use default eyesight baseline path.
    pub auto_baseline: bool,
}

impl Default for SeeOptions {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from("."),
            subject_class: "scene".into(),
            intent: "verify scene appearance".into(),
            style_intent: None,
            app_state: None,
            target_name: None,
            port: None,
            subject_filter: SubjectFilterMode::GameplayPrefer,
            max_subjects: DEFAULT_MAX_SUBJECTS,
            wait_for_subjects: Vec::new(),
            wait_timeout_secs: 15,
            require_playing: false,
            projection: ProjectionMode::Ortho2d,
            visible_half_w: 640.0,
            visible_half_h: 360.0,
            compare_baseline: None,
            save_baseline_as: None,
            zoom_ladder: true,
            diagnostic_bounds: false,
            profile: None,
            include_primary_fovea: false,
            motion_mutate_entity: None,
            motion_mutate_translation: None,
            auto_baseline: false,
        }
    }
}

fn apply_subject_pipeline(
    mut raw: Vec<EyesightSubject>,
    opts: &SeeOptions,
    screen_w: u32,
    screen_h: u32,
) -> (Vec<EyesightSubject>, bool, Option<String>) {
    let cam = find_camera_translation(&raw).unwrap_or([0.0, 0.0, 0.0]);
    annotate_subjects_projection(
        &mut raw,
        cam,
        opts.projection,
        opts.visible_half_w,
        opts.visible_half_h,
        screen_w,
        screen_h,
        DEFAULT_CROP_HALF / 2,
    );
    let inferred = opts
        .app_state
        .clone()
        .or_else(|| infer_app_state_from_subjects(&raw));
    let collapsed = collapse_duplicate_names(raw);
    let (filtered, truncated) =
        filter_subjects(collapsed, opts.subject_filter, opts.max_subjects);
    (filtered, truncated, inferred)
}

/// E0: full-frame capture + filtered subjects → packet (A0/A4/A5 + S0 ranking).
pub fn see_scene(client: &BrpClient, opts: &SeeOptions) -> Result<EyesightPacket> {
    let mut opts = opts.clone();
    if let Some(ref p) = opts.profile.clone() {
        apply_game_profile(&mut opts, p);
    }
    if !opts.wait_for_subjects.is_empty() {
        wait_for_subject_names(
            client,
            &opts.wait_for_subjects,
            Duration::from_secs(opts.wait_timeout_secs.max(1)),
        )?;
    }

    let full_path = eyesight_path(&opts.out_dir, "scene_full.png");
    let img = capture_viewport_image(client, &full_path)?;
    let entry = CaptureEntry::from_path(CaptureRole::Full, &img.path)?
        .with_note("S0 full frame");

    let (w, h) = (
        entry.width.unwrap_or(1280),
        entry.height.unwrap_or(720),
    );

    let mut packet = EyesightPacket::new(&opts.subject_class, &opts.intent);
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    packet.subject_filter = Some(format!("{:?}", opts.subject_filter).to_ascii_lowercase());
    packet.captures.push(entry);
    packet.views = Some(vec!["full".into()]);

    let raw = query_all_subjects(client);
    if opts.require_playing && subjects_look_menu_only(&raw) {
        return Err(anyhow!(
            "require_playing: subjects look MainMenu-only (MenuCamera/MenuLight). \
             Set IRON_FEUD_AUTO_PLAY=1 or press Enter before claiming environment sight."
        ));
    }
    let (filtered, truncated, inferred) = apply_subject_pipeline(raw, &opts, w, h);
    packet.subjects = filtered;
    packet.subjects_truncated = if truncated { Some(true) } else { None };
    packet.app_state = inferred;
    packet.primary_subject = rank_primary_subject(&packet.subjects);
    if subjects_look_menu_only(&packet.subjects) {
        packet.push_warning(
            "subjects look menu-only — env claims invalid; wait for Playing / AUTO_PLAY",
        );
    }

    if packet
        .captures
        .iter()
        .any(|c| c.black_frame_warning == Some(true))
    {
        packet.push_warning(
            "black_frame: window may be minimized, camera wrong, no lights, or still Loading",
        );
    }

    let baseline_path = opts.save_baseline_as.clone().or_else(|| {
        if opts.auto_baseline {
            Some(eyesight_path(&opts.out_dir, "baseline_scene.png"))
        } else {
            None
        }
    });
    if let Some(ref base) = baseline_path {
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&img.path, base)?;
        packet.baseline_path = Some(abs_path_string(base));
    }
    let compare = opts.compare_baseline.clone().or_else(|| {
        let def = eyesight_path(&opts.out_dir, "baseline_scene.png");
        if opts.compare_baseline.is_none() && def.is_file() && opts.auto_baseline {
            Some(def)
        } else {
            None
        }
    });
    // Explicit compare_baseline only (not auto re-diff every time)
    if let Some(ref base) = opts.compare_baseline {
        let diff_path = eyesight_path(&opts.out_dir, "scene_vs_baseline_diff.png");
        if let Ok((p, score)) = write_diff_png(base, &img.path, &diff_path) {
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Diff, &p)?
                    .with_note(format!("S0 vs baseline mean={score:.4}")),
            );
            packet.baseline_path = Some(abs_path_string(base));
        }
    }
    let _ = compare;

    // Optional primary fovea attach
    if opts.include_primary_fovea {
        if let Some(ref primary) = packet.primary_subject.clone() {
            if let Ok(ent) = see_entity(client, &opts, primary, None, None, DEFAULT_CROP_HALF) {
                for c in ent.captures {
                    if c.role == "crop" {
                        let c = c;
                        if c.note.as_ref().map(|n| n.contains("zoom")).unwrap_or(false) {
                            if let Some(v) = packet.views.as_mut() {
                                if !v.iter().any(|x| x == "fovea_zoom") {
                                    v.push("fovea_zoom".into());
                                }
                            }
                        } else if let Some(v) = packet.views.as_mut() {
                            if !v.iter().any(|x| x == "fovea") {
                                v.push("fovea".into());
                            }
                        }
                        packet.captures.push(c);
                    }
                }
            }
        }
    }

    let json_path = eyesight_path(&opts.out_dir, "scene_packet.json");
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

/// S0.4 one-shot verify: scene + primary fovea (+ zoom).
pub fn see_verify(client: &BrpClient, opts: &SeeOptions) -> Result<EyesightPacket> {
    let mut opts = opts.clone();
    if let Some(ref p) = opts.profile.clone() {
        apply_game_profile(&mut opts, p);
    }
    opts.include_primary_fovea = true;
    opts.zoom_ladder = true;
    let mut packet = see_scene(client, &opts)?;
    packet.intent = format!("verify: {}", opts.intent);
    let json_path = eyesight_path(&opts.out_dir, "verify_packet.json");
    packet.write_json(json_path)?;
    Ok(packet)
}

/// Pick subjects for entity fovea: exact Name first, then contains; prefer higher gameplay score.
/// Avoids picking an unrelated first BRP row when names are ambiguous.
pub fn match_subjects_for_entity<'a>(
    subjects: &'a [EyesightSubject],
    entity_name: &str,
) -> Vec<&'a EyesightSubject> {
    if entity_name == "*" {
        return subjects.iter().collect();
    }
    let mut exact: Vec<&EyesightSubject> = subjects
        .iter()
        .filter(|s| s.name == entity_name)
        .collect();
    if !exact.is_empty() {
        exact.sort_by(|a, b| {
            gameplay_subject_score(&b.name).cmp(&gameplay_subject_score(&a.name))
        });
        return exact;
    }
    let mut partial: Vec<&EyesightSubject> = subjects
        .iter()
        .filter(|s| s.name.contains(entity_name))
        .filter(|s| !is_child_mesh_part(&s.name))
        .collect();
    partial.sort_by(|a, b| {
        gameplay_subject_score(&b.name).cmp(&gameplay_subject_score(&a.name))
    });
    partial
}

/// E1 true fovea: project entity → screen, crop (+ optional zoom ladder).
pub fn see_entity(
    client: &BrpClient,
    opts: &SeeOptions,
    entity_name: &str,
    screen_x: Option<u32>,
    screen_y: Option<u32>,
    half: u32,
) -> Result<EyesightPacket> {
    let half = crop_half_for_entity(entity_name, half);
    let full_path = eyesight_path(&opts.out_dir, "entity_full.png");
    let img = capture_viewport_image(client, &full_path)?;
    let full = CaptureEntry::from_path(CaptureRole::Full, &img.path)?;

    let (w, h) = (
        full.width.unwrap_or(1280),
        full.height.unwrap_or(720),
    );

    let mut raw = query_all_subjects(client);
    let cam = find_camera_translation(&raw).unwrap_or([0.0, 0.0, 0.0]);
    annotate_subjects_projection(
        &mut raw,
        cam,
        opts.projection,
        opts.visible_half_w,
        opts.visible_half_h,
        w,
        h,
        half,
    );

    let matched = match_subjects_for_entity(&raw, entity_name);

    let (cx, cy, proj_note) = if let (Some(sx), Some(sy)) = (screen_x, screen_y) {
        (sx, sy, "explicit screen coords".to_string())
    } else if let Some(hit) = matched.first() {
        if let Some(xy) = hit.screen_xy {
            let mode_note = match opts.projection {
                ProjectionMode::TopDown3d => "topdown3d XZ",
                ProjectionMode::Ortho2d => "ortho2d XY",
            };
            (
                xy[0],
                xy[1],
                format!(
                    "world→screen projection ({mode_note}) name='{}'",
                    hit.name
                ),
            )
        } else {
            (
                w / 2,
                h / 2,
                "fallback center (matched entity has no screen_xy)".to_string(),
            )
        }
    } else {
        (
            w / 2,
            h / 2,
            "fallback center (no projection for entity)".to_string(),
        )
    };

    let crop_path = eyesight_path(
        &opts.out_dir,
        &format!("entity_{}_crop.png", sanitize_name(entity_name)),
    );
    crop_png_around(&img.path, &crop_path, cx, cy, half, half)?;
    let crop = CaptureEntry::from_path(CaptureRole::Crop, &crop_path)?
        .with_note(format!("A1 fovea '{entity_name}' @ ({cx},{cy}) half={half} ({proj_note})"));

    let mut packet = EyesightPacket::new("entity", &opts.intent);
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    packet.primary_subject = Some(entity_name.into());
    packet.app_state = infer_app_state_from_subjects(&raw);
    packet.captures.push(full);
    if proj_note.contains("fallback") {
        packet.push_warning(format!(
            "fovea used center fallback for '{entity_name}' — pass screen_x/y or ensure Name+Transform"
        ));
    }
    // S3: dark/unreadable fovea — aim may be correct but craft not judgeable
    if let Ok(bytes) = fs::read(&crop_path) {
        if fovea_crop_is_too_dark(&bytes).unwrap_or(false) {
            let frac = png_nonblack_fraction(&bytes, FOVEA_CRAFT_LUMA_MIN).unwrap_or(0.0);
            let mean = png_mean_luma(&bytes).unwrap_or(0.0);
            packet.push_warning(format!(
                "fovea_dark: crop craft_nonblack={frac:.3} (min {FOVEA_CROP_NONBLACK_MIN} @ luma>={FOVEA_CRAFT_LUMA_MIN}) mean_luma={mean:.1} (min {FOVEA_CROP_MEAN_LUMA_MIN}) for '{entity_name}' @ ({cx},{cy}) — do not claim craft identity from this crop alone"
            ));
        }
    }

    if opts.zoom_ladder {
        let half2 = half / 2;
        if half2 >= 16 {
            let zpath = eyesight_path(
                &opts.out_dir,
                &format!("entity_{}_crop_zoom2x.png", sanitize_name(entity_name)),
            );
            crop_png_around(&img.path, &zpath, cx, cy, half2, half2)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Crop, &zpath)?
                    .with_note(format!("A1 zoom ladder 2× half={half2}")),
            );
        }
    }

    if opts.diagnostic_bounds {
        if let Ok(bytes) = fs::read(&crop_path) {
            if let Ok(mut rgba) = RgbaImage::decode_png(&bytes) {
                let dw = rgba.width.saturating_sub(4);
                let dh = rgba.height.saturating_sub(4);
                draw_rect_outline(&mut rgba, 2, 2, dw, dh, [0, 255, 80, 255]);
                let dpath = eyesight_path(
                    &opts.out_dir,
                    &format!("entity_{}_diagnostic_bounds.png", sanitize_name(entity_name)),
                );
                rgba.save_png(&dpath)?;
                packet.captures.push(
                    CaptureEntry::from_path(CaptureRole::Crop, &dpath)?
                        .with_note("A6 diagnostic bounds outline on fovea crop"),
                );
            }
        }
    }
    packet.captures.push(crop);

    if matched.is_empty() {
        packet.subjects.push(EyesightSubject {
            parent_entity: None,
            name: entity_name.into(),
            entity: None,
            translation: None,
            on_screen_estimate: Some(true),
            on_screen: Some(true),
            screen_xy: Some([cx, cy]),
            screen_aabb: Some([
                cx.saturating_sub(half),
                cy.saturating_sub(half),
                half.saturating_mul(2),
                half.saturating_mul(2),
            ]),
            duplicate_count: None,
        });
    } else {
        packet.subjects = matched.into_iter().cloned().collect();
    }

    let json_path = eyesight_path(
        &opts.out_dir,
        &format!("entity_{}_packet.json", sanitize_name(entity_name)),
    );
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

/// E1 region crop by explicit pixel rect.
pub fn see_region(
    client: &BrpClient,
    opts: &SeeOptions,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    label: &str,
) -> Result<EyesightPacket> {
    let full_path = eyesight_path(&opts.out_dir, "region_full.png");
    let img = capture_viewport_image(client, &full_path)?;
    let full = CaptureEntry::from_path(CaptureRole::Full, &img.path)?;
    let crop_path = eyesight_path(
        &opts.out_dir,
        &format!("region_{}_crop.png", sanitize_name(label)),
    );
    crop_png_file(&img.path, &crop_path, x, y, w, h)?;
    let crop = CaptureEntry::from_path(CaptureRole::Crop, &crop_path)?
        .with_note(format!("E1 region '{label}' rect=({x},{y},{w},{h})"));

    let mut packet = EyesightPacket::new(opts.subject_class.clone(), &opts.intent);
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    packet.captures.push(full);
    packet.captures.push(crop);
    packet.subjects.push(EyesightSubject {
        parent_entity: None,
        name: label.into(),
        entity: None,
        translation: None,
        on_screen_estimate: Some(true),
        on_screen: Some(true),
        screen_xy: Some([x + w / 2, y + h / 2]),
        screen_aabb: Some([x, y, w, h]),
    duplicate_count: None,
    });

    let json_path = eyesight_path(
        &opts.out_dir,
        &format!("region_{}_packet.json", sanitize_name(label)),
    );
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

/// E2/A3 temporal: N frames with optional key stimulus; montage strip + static note.
pub fn see_motion(
    client: &BrpClient,
    opts: &SeeOptions,
    frames: u32,
    interval_ms: u64,
    keys: Option<Vec<String>>,
) -> Result<EyesightPacket> {
    let frames = frames.clamp(2, 12);
    let mut packet = EyesightPacket::new(
        if opts.subject_class == "scene" {
            "physics_motion".into()
        } else {
            opts.subject_class.clone()
        },
        &opts.intent,
    );
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    packet.app_state = opts.app_state.clone();

    let mut opts = opts.clone();
    if let Some(ref p) = opts.profile.clone() {
        apply_game_profile(&mut opts, p);
    }

    if let Some(ref k) = keys {
        let params = json!({ "keys": k, "duration": 0.05 });
        let _ = client.call("brp_extras/send_keys", Some(params));
        packet.stimulus = StimulusInfo {
            kind: "keys".into(),
            detail: Some(json!({ "keys": k })),
        };
    } else if let (Some(ent), Some(ref tr)) =
        (opts.motion_mutate_entity, opts.motion_mutate_translation.clone())
    {
        let component = "bevy_transform::components::transform::Transform";
        let _ = client.mutate_components(ent, component, "translation", tr.clone());
        thread::sleep(Duration::from_millis(50));
        packet.stimulus = StimulusInfo {
            kind: "mutate".into(),
            detail: Some(json!({ "entity": ent, "translation": tr })),
        };
    } else {
        packet.stimulus = StimulusInfo {
            kind: "none".into(),
            detail: Some(json!({"note": "no stimulus — static scene possible"})),
        };
    }

    let mut decoded = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    for i in 0..frames {
        let path = eyesight_path(&opts.out_dir, &format!("motion_frame_{i:02}.png"));
        let img = capture_viewport_image(client, &path)?;
        let entry = CaptureEntry::from_path(CaptureRole::Frame, &img.path)?
            .with_note(format!("A3 frame {i}/{frames}"));
        sizes.push(entry.bytes);
        if entry.black_frame_warning == Some(true) {
            packet.push_warning(format!("black_frame on motion frame {i}"));
        }
        packet.captures.push(entry);
        if let Ok(bytes) = fs::read(&img.path) {
            if let Ok(rgba) = RgbaImage::decode_png(&bytes) {
                decoded.push(rgba);
            }
        }
        if i + 1 < frames {
            thread::sleep(Duration::from_millis(interval_ms.max(16)));
        }
    }

    // A3: detect silent identical strips
    let all_same_size = sizes.len() >= 2 && sizes.windows(2).all(|w| w[0] == w[1]);
    let mut static_scene = all_same_size;
    if decoded.len() >= 2 {
        if let Ok(score) = mean_abs_diff(&decoded[0], &decoded[decoded.len() - 1]) {
            if score < 0.002 {
                static_scene = true;
            } else {
                static_scene = false;
            }
        }
        if let Ok(strip) = montage_horizontal(&decoded, 2) {
            let strip_path = eyesight_path(&opts.out_dir, "motion_strip.png");
            strip.save_png(&strip_path)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Strip, &strip_path)?
                    .with_note("A3 horizontal montage"),
            );
        }
    }
    if static_scene {
        packet.push_warning(
            "static_scene: motion frames nearly identical — apply keys/mutate stimulus or accept static",
        );
    }

    let (filtered, _, inferred) =
        apply_subject_pipeline(query_all_subjects(client), &opts, 1280, 720);
    packet.subjects = filtered;
    if packet.app_state.is_none() {
        packet.app_state = inferred;
    }

    let json_path = eyesight_path(&opts.out_dir, "motion_packet.json");
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

/// Normalize translation JSON to Bevy 0.19 BRP form: sequence of 3 f32 `[x,y,z]`.
/// Accepts `{x,y,z}` maps (historical) or arrays.
pub fn translation_value_for_brp(v: &Value) -> Value {
    if let Some(arr) = v.as_array() {
        if arr.len() >= 3 {
            return json!([
                arr[0].as_f64().unwrap_or(0.0),
                arr[1].as_f64().unwrap_or(0.0),
                arr[2].as_f64().unwrap_or(0.0)
            ]);
        }
    }
    if let Some(x) = v.get("x").and_then(|n| n.as_f64()) {
        let y = v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0);
        let z = v.get("z").and_then(|n| n.as_f64()).unwrap_or(0.0);
        return json!([x, y, z]);
    }
    v.clone()
}

/// Best-effort multi-view by mutating a camera Transform (A2).
/// **Always** attempts restore after capture (T2), even if capture fails mid-path.
fn capture_with_camera_nudge(
    client: &BrpClient,
    opts: &SeeOptions,
    role: CaptureRole,
    filename: &str,
    note: &str,
    entity: u64,
    component: &str,
    new_translation: Value,
    restore: Value,
) -> Result<(CaptureEntry, Option<String>)> {
    let mut warn: Option<String> = None;
    let new_t = translation_value_for_brp(&new_translation);
    let restore_t = translation_value_for_brp(&restore);
    let components = [
        component,
        "bevy_transform::components::transform::Transform",
        "Transform",
    ];
    let mut mutated = false;
    let mut last_err = String::new();
    for c in components {
        match client.mutate_components(entity, c, "translation", new_t.clone()) {
            Ok(_) => {
                mutated = true;
                break;
            }
            Err(e) => {
                last_err = format!("{c}: {e}");
            }
        }
    }
    if !mutated {
        warn = Some(format!(
            "camera_nudge_mutate_failed entity={entity}: {last_err}"
        ));
    }
    let capture_result = (|| -> Result<CaptureEntry> {
        thread::sleep(Duration::from_millis(if mutated { 180 } else { 40 }));
        let path = eyesight_path(&opts.out_dir, filename);
        let img = capture_viewport_image(client, &path)?;
        let mut note = note.to_string();
        if !mutated {
            note.push_str(" [mutate_failed]");
        } else {
            note.push_str(" [mutated]");
        }
        Ok(CaptureEntry::from_path(role, &img.path)?.with_note(note))
    })();

    let mut restored = false;
    for c in components {
        if client
            .mutate_components(entity, c, "translation", restore_t.clone())
            .is_ok()
        {
            restored = true;
            break;
        }
    }
    thread::sleep(Duration::from_millis(50));
    if mutated && !restored {
        let msg = format!("camera_restore_failed entity={entity} after nudge");
        warn = Some(match warn {
            Some(w) => format!("{w}; {msg}"),
            None => msg,
        });
    }

    match capture_result {
        Ok(mut entry) => {
            if mutated && restored {
                if let Some(ref mut n) = entry.note {
                    n.push_str(" [restored]");
                }
            } else if mutated && !restored {
                if let Some(ref mut n) = entry.note {
                    n.push_str(" [restore_failed]");
                }
            }
            Ok((entry, warn))
        }
        Err(e) => Err(e),
    }
}

/// E3: capture after using an existing baseline path; optional diff image.
pub fn see_diff(
    client: &BrpClient,
    opts: &SeeOptions,
    baseline_path: impl AsRef<Path>,
) -> Result<EyesightPacket> {
    let baseline = baseline_path.as_ref();
    if !baseline.is_file() {
        return Err(anyhow!("baseline not found: {}", baseline.display()));
    }
    let after_path = eyesight_path(&opts.out_dir, "diff_after.png");
    let img = capture_viewport_image(client, &after_path)?;

    let mut packet = EyesightPacket::new(&opts.subject_class, &opts.intent);
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    packet.captures.push(
        CaptureEntry::from_path(CaptureRole::Baseline, baseline)?
            .with_note("E3 baseline"),
    );
    packet.captures.push(
        CaptureEntry::from_path(CaptureRole::After, &img.path)?.with_note("E3 after"),
    );

    let diff_path = eyesight_path(&opts.out_dir, "diff_abs.png");
    if let Ok((p, score)) = write_diff_png(baseline, &img.path, &diff_path) {
        packet.captures.push(
            CaptureEntry::from_path(CaptureRole::Diff, &p)?
                .with_note(format!("E3 abs-diff mean={score:.4}")),
        );
    }

    let json_path = eyesight_path(&opts.out_dir, "diff_packet.json");
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

/// E4/A2 packs: entity_craft | landscape | water | physics_jump | lighting | diagnostic | hud | env_2d
pub fn see_pack(
    client: &BrpClient,
    opts: &SeeOptions,
    pack: &str,
) -> Result<EyesightPacket> {
    let pack = pack.to_ascii_lowercase();
    let mut packet = EyesightPacket::new(
        match pack.as_str() {
            "water" => "water",
            "landscape" => "landscape",
            "physics_jump" | "physics" => "physics_motion",
            "lighting" | "diagnostic" => "lighting",
            "hud" => "hud",
            "env_2d" => "env_2d",
            _ => "entity",
        },
        format!("pack:{pack} — {}", opts.intent),
    );
    packet.pack = Some(pack.clone());
    packet.style_intent = opts.style_intent.clone();
    packet.target = opts.target_name.clone();
    packet.port = Some(client.target.port);
    let mut views = Vec::new();

    match pack.as_str() {
        "entity_craft" | "entity" => {
            let scene = see_scene(client, opts)?;
            packet.subjects = scene.subjects.clone();
            packet.app_state = scene.app_state;
            packet.primary_subject = scene.primary_subject.clone();
            packet.captures.extend(scene.captures);
            views.push("game".into());
            // Fovea on primary gameplay subject (Player preferred for 2D craft after verify)
            let name = packet
                .primary_subject
                .clone()
                .or_else(|| packet.subjects.first().map(|s| s.name.clone()))
                .unwrap_or_else(|| "Player".into());
            if let Ok(ent) = see_entity(client, opts, &name, None, None, 128) {
                for c in ent.captures {
                    if c.role == "crop" {
                        packet.captures.push(c);
                    }
                }
                views.push("fovea".into());
            }
        }
        "hud" => {
            // 2D HUD craft: full + top-left + top-bar crops (region presets)
            let mut opts = opts.clone();
            if let Some(ref p) = opts.profile.clone() {
                apply_game_profile(&mut opts, p);
            }
            let full_path = eyesight_path(&opts.out_dir, "pack_hud_full.png");
            let img = capture_viewport_image(client, &full_path)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Full, &img.path)?
                    .with_note("hud pack full frame"),
            );
            views.push("game".into());
            let w = packet.captures[0].width.unwrap_or(1280);
            let h = packet.captures[0].height.unwrap_or(720);
            for preset in [RegionPreset::HudTopLeft, RegionPreset::HudTopBar] {
                let (x, y, rw, rh) = preset.rect(w, h);
                let crop_path =
                    eyesight_path(&opts.out_dir, &format!("pack_hud_{}.png", preset.label()));
                crop_png_file(&img.path, &crop_path, x, y, rw, rh)?;
                packet.captures.push(
                    CaptureEntry::from_path(CaptureRole::Crop, &crop_path)?.with_note(format!(
                        "hud region preset {} rect=({x},{y},{rw},{rh})",
                        preset.label()
                    )),
                );
                views.push(preset.label().into());
            }
            let subjects = query_all_subjects(client);
            let (filtered, _, inferred) = apply_subject_pipeline(subjects, &opts, w, h);
            packet.subjects = filtered;
            packet.app_state = inferred;
            packet.primary_subject = rank_primary_subject(&packet.subjects);
        }
        "env_2d" => {
            // 2D parallax env composition: full + horizon band + center station/debris crop
            let mut opts = opts.clone();
            if let Some(ref p) = opts.profile.clone() {
                apply_game_profile(&mut opts, p);
            } else {
                apply_game_profile(&mut opts, "crystal-drift");
            }
            // S4: wait briefly for env Names so nebulas/station paint before capture
            let env_wait = [
                "Nebula".to_string(),
                "WarpGate".to_string(),
                "Station".to_string(),
                "Player".to_string(),
            ];
            let _ = wait_for_subject_names(client, &env_wait, Duration::from_secs(4));
            thread::sleep(Duration::from_millis(200));

            let full_path = eyesight_path(&opts.out_dir, "pack_env_2d_full.png");
            let mut img = capture_viewport_image(client, &full_path)?;
            // If first full is nearly black, wait and recapture once (load timing flake)
            if path_nonblack_fraction(&img.path, 30).unwrap_or(0.0) < FULL_FRAME_NONBLACK_MIN {
                thread::sleep(Duration::from_millis(400));
                img = capture_viewport_image(client, &full_path)?;
                if path_nonblack_fraction(&img.path, 30).unwrap_or(0.0) < FULL_FRAME_NONBLACK_MIN {
                    packet.push_warning(
                        "env_2d_dark: full frame nonblack below FULL_FRAME_NONBLACK_MIN after retry — do not claim env composition from this pack alone",
                    );
                } else {
                    packet.push_warning(
                        "env_2d: first full capture was dark; retry succeeded",
                    );
                }
            }
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Full, &img.path)?
                    .with_note("env_2d full (parallax/composition)"),
            );
            views.push("game".into());
            let w = packet.captures[0].width.unwrap_or(1280);
            let h = packet.captures[0].height.unwrap_or(720);
            let (hx, hy, hw, hh) = RegionPreset::HorizonBand.rect(w, h);
            let horizon_path = eyesight_path(&opts.out_dir, "pack_env_2d_horizon.png");
            crop_png_file(&img.path, &horizon_path, hx, hy, hw, hh)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Crop, &horizon_path)?
                    .with_note("env_2d horizon band"),
            );
            views.push("horizon".into());
            let (cx, cy, cw, ch) = RegionPreset::CenterHalf.rect(w, h);
            let center_path = eyesight_path(&opts.out_dir, "pack_env_2d_station_or_debris.png");
            crop_png_file(&img.path, &center_path, cx, cy, cw, ch)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Crop, &center_path)?
                    .with_note("env_2d center (station/debris/craft)"),
            );
            views.push("center".into());
            let subjects = query_all_subjects(client);
            let (filtered, _, inferred) = apply_subject_pipeline(subjects, &opts, w, h);
            packet.subjects = filtered;
            packet.app_state = inferred;
            packet.primary_subject = rank_primary_subject(&packet.subjects);
        }
        "landscape" | "water" => {
            let mut opts = opts.clone();
            if let Some(ref p) = opts.profile.clone() {
                apply_game_profile(&mut opts, p);
            }
            // View 1: game camera
            let game_name = if pack == "water" {
                "pack_water_view_game.png"
            } else {
                "pack_landscape_view_game.png"
            };
            let full_path = eyesight_path(&opts.out_dir, game_name);
            let img = capture_viewport_image(client, &full_path)?;
            let mut game_note = format!("{pack} view=game");
            if pack == "landscape" {
                game_note.push_str(
                    " — height readability: full+alt should show relief when terrain has hills/mountains",
                );
            }
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Full, &img.path)?.with_note(game_note),
            );
            views.push("game".into());
            let w = packet.captures[0].width.unwrap_or(1280);
            let h = packet.captures[0].height.unwrap_or(720);
            let crop_path = eyesight_path(
                &opts.out_dir,
                &format!("pack_{}_surface_or_horizon.png", pack),
            );
            if pack == "water" {
                crop_png_file(&img.path, &crop_path, w / 4, h / 4, w / 2, h / 2)?;
            } else {
                crop_png_file(&img.path, &crop_path, 0, h / 6, w, h / 3)?;
            }
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Crop, &crop_path)?
                    .with_note(format!("{pack} crop from game view")),
            );

            let subjects = query_all_subjects(client);
            // H0: first alt nudge; if still similar, second **side-orbit** capture path
            if let Some(cam_s) = subjects.iter().find(|s| {
                s.name == "StrategyCamera"
                    || s.name == "MainCamera"
                    || (s.name.contains("Camera") && !s.name.contains("Menu"))
            }) {
                if let (Some(entity), Some(t)) = (cam_s.entity, cam_s.translation) {
                    let component = "bevy_transform::components::transform::Transform";
                    let restore = json!({ "x": t[0], "y": t[1], "z": t[2] });
                    let topdown = matches!(opts.projection, ProjectionMode::TopDown3d)
                        || cam_s.name.contains("Strategy");
                    let n = alt_camera_nudge_translation([t[0], t[1], t[2]], topdown);
                    let nudged = json!({ "x": n[0], "y": n[1], "z": n[2] });
                    let role = CaptureRole::Side;
                    let fname = format!("pack_{}_view_alt.png", pack);
                    if let Ok((entry, mut_warn)) = capture_with_camera_nudge(
                        client,
                        &opts,
                        role,
                        &fname,
                        &format!(
                            "{pack} view=alt (camera nudge side-xz topdown={topdown})"
                        ),
                        entity,
                        component,
                        nudged,
                        restore.clone(),
                    ) {
                        if let Some(w) = mut_warn {
                            packet.push_warning(w);
                        }
                        let similar = captures_look_similar(&img.path, Path::new(&entry.abs_path))
                            .unwrap_or(false);
                        packet.captures.push(entry);
                        views.push("alt".into());
                        if similar {
                            // Second path: aggressive side-orbit (H0 true multi-view attempt)
                            let o = side_orbit_camera_translation([t[0], t[1], t[2]], topdown);
                            let orbit = json!({ "x": o[0], "y": o[1], "z": o[2] });
                            let fname2 = format!("pack_{}_view_side.png", pack);
                            if let Ok((entry2, mut_warn2)) = capture_with_camera_nudge(
                                client,
                                &opts,
                                CaptureRole::Side,
                                &fname2,
                                &format!(
                                    "{pack} view=side-orbit (second path after views_similar)"
                                ),
                                entity,
                                component,
                                orbit,
                                restore.clone(),
                            ) {
                                if let Some(w) = mut_warn2 {
                                    packet.push_warning(w);
                                }
                                if captures_look_similar(&img.path, Path::new(&entry2.abs_path))
                                    .unwrap_or(false)
                                {
                                    let d = dedicated_side_camera_translation(
                                        [t[0], t[1], t[2]],
                                        topdown,
                                    );
                                    let dedicated = json!({ "x": d[0], "y": d[1], "z": d[2] });
                                    let fname3 = format!("pack_{}_view_dedicated.png", pack);
                                    if let Ok((entry3, mut_warn3)) = capture_with_camera_nudge(
                                        client,
                                        &opts,
                                        CaptureRole::Side,
                                        &fname3,
                                        &format!(
                                            "{pack} view=dedicated-side (T3 after side-orbit still similar)"
                                        ),
                                        entity,
                                        component,
                                        dedicated,
                                        restore.clone(),
                                    ) {
                                        if let Some(w) = mut_warn3 {
                                            packet.push_warning(w);
                                        }
                                        if captures_look_similar(
                                            &img.path,
                                            Path::new(&entry3.abs_path),
                                        )
                                        .unwrap_or(false)
                                        {
                                            packet.push_warning(
                                                "views_similar: alt, side-orbit, and dedicated-side still nearly match game — do not claim multi-angle insight",
                                            );
                                        } else {
                                            packet.push_warning(
                                                "multi_view: dedicated-side differs from game after alt/side-orbit were similar",
                                            );
                                        }
                                        packet.captures.push(entry3);
                                        views.push("dedicated_side".into());
                                    } else {
                                        packet.push_warning(
                                            "views_similar: alt and side-orbit still nearly match game — dedicated-side capture failed; do not claim multi-angle insight",
                                        );
                                    }
                                } else {
                                    packet.push_warning(
                                        "multi_view: side-orbit differs from game after first alt was similar",
                                    );
                                }
                                packet.captures.push(entry2);
                                views.push("side_orbit".into());
                            } else {
                                packet.push_warning(
                                    "views_similar: alt view nearly matches game — side-orbit capture failed; do not claim multi-angle insight",
                                );
                            }
                        }
                    }
                }
            } else {
                let alt_path = eyesight_path(&opts.out_dir, &format!("pack_{}_view_alt_crop.png", pack));
                crop_png_file(&img.path, &alt_path, 0, h / 2, w, h / 2)?;
                packet.captures.push(
                    CaptureEntry::from_path(CaptureRole::Side, &alt_path)?
                        .with_note(format!("{pack} view=alt crop (no camera entity to nudge)")),
                );
                views.push("alt_crop".into());
            }

            let (filtered, _, inferred) = apply_subject_pipeline(subjects, &opts, w, h);
            packet.subjects = filtered;
            packet.app_state = inferred;
            packet.primary_subject = rank_primary_subject(&packet.subjects);
            if pack == "landscape" {
                if let Some(note) = height_readability_note(&packet.subjects) {
                    packet.push_warning(note);
                }
            }
        }
        "physics_jump" | "physics" => {
            let mut motion_opts = opts.clone();
            motion_opts.subject_class = "physics_motion".into();
            let m = see_motion(client, &motion_opts, DEFAULT_MOTION_FRAMES, DEFAULT_MOTION_INTERVAL_MS, None)?;
            packet.captures = m.captures;
            packet.stimulus = m.stimulus;
            packet.subjects = m.subjects;
            packet.warnings = m.warnings;
            views.push("motion".into());
        }
        "lighting" => {
            let full_path = eyesight_path(&opts.out_dir, "pack_lighting.png");
            let img = capture_viewport_image(client, &full_path)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Full, &img.path)?
                    .with_note("lighting lit capture (unlit is game-side opt-in)"),
            );
            packet.push_warning(
                "unlit diagnostic not automatic; use pack=diagnostic for bounds overlay",
            );
            views.push("lit".into());
        }
        "diagnostic" => {
            let mut o = opts.clone();
            if let Some(ref p) = o.profile.clone() {
                apply_game_profile(&mut o, p);
            }
            o.diagnostic_bounds = true;
            let full_path = eyesight_path(&opts.out_dir, "pack_diagnostic_full.png");
            let img = capture_viewport_image(client, &full_path)?;
            packet.captures.push(
                CaptureEntry::from_path(CaptureRole::Full, &img.path)?
                    .with_note("S0 diagnostic full (beauty still)"),
            );
            let raw = query_all_subjects(client);
            let (filtered, _, _) = apply_subject_pipeline(
                raw,
                &o,
                packet.captures[0].width.unwrap_or(1280),
                packet.captures[0].height.unwrap_or(720),
            );
            let name = diagnostic_primary_name(&filtered);
            packet.primary_subject = Some(name.clone());
            if let Ok(ent) = see_entity(client, &o, &name, None, None, 120) {
                packet.captures.extend(ent.captures.into_iter().filter(|c| {
                    c.note
                        .as_ref()
                        .map(|n| n.contains("diagnostic") || n.contains("fovea") || n.contains("zoom"))
                        .unwrap_or(false)
                }));
                packet.subjects = ent.subjects;
            } else {
                packet.subjects = filtered;
            }
            packet.push_warning(
                "diagnostic pack: bounds overlay on crops; primary from ranker/allowlist (not Player-only)",
            );
            views.push("diagnostic".into());
        }
        other => {
            return Err(anyhow!(
                "unknown pack '{other}' (entity_craft|landscape|water|physics_jump|lighting|diagnostic|hud|env_2d)"
            ));
        }
    }

    packet.views = Some(views);
    if packet
        .captures
        .iter()
        .any(|c| c.black_frame_warning == Some(true))
    {
        packet.push_warning("black_frame detected in pack captures");
    }

    let json_path = eyesight_path(&opts.out_dir, &format!("pack_{}_packet.json", sanitize_name(&pack)));
    packet.write_json(json_path)?;
    packet.validate()?;
    Ok(packet)
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Build MCP text+image content blocks from a packet (first capture image + full JSON).
pub fn packet_to_mcp_content(packet: &EyesightPacket) -> Result<Value> {
    let mut content = Vec::new();
    // Prefer strip or crop for fovea, else first capture.
    let prefer = packet
        .captures
        .iter()
        .find(|c| c.role == "strip" || c.role == "crop" || c.role == "diff")
        .or_else(|| packet.captures.first());
    if let Some(c) = prefer {
        if let Ok(img) = CapturedImage::from_png_path(&c.abs_path) {
            if let Some(arr) = img.to_mcp_content_blocks().as_array() {
                content.extend(arr.iter().cloned());
            }
        }
    }
    content.push(json!({
        "type": "text",
        "text": packet.to_pretty_json()?
    }));
    Ok(json!({ "content": content, "isError": false }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine};

    fn fixture_png_bytes() -> Vec<u8> {
        let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        B64.decode(b64).expect("fixture b64")
    }

    fn solid_png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let img = RgbaImage {
            width: w,
            height: h,
            pixels: rgba.repeat((w * h) as usize),
        };
        img.encode_png().unwrap()
    }

    #[test]
    fn packet_schema_validate_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        fs::write(&png, solid_png(8, 8, [10, 20, 30, 255])).unwrap();
        let mut p = EyesightPacket::new("scene", "test");
        p.captures
            .push(CaptureEntry::from_path(CaptureRole::Full, &png).unwrap());
        p.validate().unwrap();
        let v = p.to_json_value().unwrap();
        assert_eq!(v["schema"], EYESIGHT_SCHEMA);
        assert!(v["captures"][0]["bytes"].as_u64().unwrap() > 0);
        let back: EyesightPacket = serde_json::from_value(v).unwrap();
        assert_eq!(back.schema, EYESIGHT_SCHEMA);
    }

    #[test]
    fn packet_validate_accepts_legacy_schema_only() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        fs::write(&png, solid_png(8, 8, [10, 20, 30, 255])).unwrap();
        let mut p = EyesightPacket::new("scene", "test");
        p.captures
            .push(CaptureEntry::from_path(CaptureRole::Full, &png).unwrap());
        p.schema = LEGACY_EYESIGHT_SCHEMA.into();
        p.validate().unwrap();
        p.schema = "other/v1".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn crop_rgba_center_region() {
        let img = RgbaImage {
            width: 10,
            height: 10,
            pixels: {
                let mut p = vec![0u8; 10 * 10 * 4];
                // mark (5,5) red
                let i = (5 * 10 + 5) * 4;
                p[i] = 255;
                p[i + 3] = 255;
                p
            },
        };
        let c = crop_rgba(&img, 4, 4, 3, 3).unwrap();
        assert_eq!(c.width, 3);
        assert_eq!(c.height, 3);
        // center of crop is original (5,5)
        let i = (1 * 3 + 1) * 4;
        assert_eq!(c.pixels[i], 255);
    }

    #[test]
    fn crop_png_file_writes_smaller() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.png");
        fs::write(&src, solid_png(32, 32, [40, 80, 120, 255])).unwrap();
        let dest = dir.path().join("crop.png");
        crop_png_file(&src, &dest, 4, 4, 8, 8).unwrap();
        let out = RgbaImage::decode_png(&fs::read(&dest).unwrap()).unwrap();
        assert_eq!(out.width, 8);
        assert_eq!(out.height, 8);
    }

    #[test]
    fn black_frame_detects_near_black() {
        let black = solid_png(16, 16, [0, 0, 0, 255]);
        assert!(is_mostly_black_png(&black, 0.04).unwrap());
        let bright = solid_png(16, 16, [200, 200, 200, 255]);
        assert!(!is_mostly_black_png(&bright, 0.04).unwrap());
    }

    fn sub(name: &str) -> EyesightSubject {
        EyesightSubject {
            name: name.into(),
            parent_entity: None,
            ..Default::default()
        }
    }

    #[test]
    fn rank_primary_prefers_player_over_crystal() {
        let subs = vec![sub("Crystal"), sub("Crystal"), sub("Player"), sub("Star1")];
        assert_eq!(rank_primary_subject(&subs).as_deref(), Some("Player"));
    }

    #[test]
    fn rank_primary_prefers_water_over_ore_crystal() {
        let subs = vec![
            sub("OreCrystal0"),
            sub("OreCrystal1"),
            sub("WaterBody"),
            sub("Ground"),
        ];
        assert_eq!(rank_primary_subject(&subs).as_deref(), Some("WaterBody"));
    }

    #[test]
    fn filter_prefers_waterbody_and_terrain_over_ore_and_children() {
        let mut subs = vec![
            sub("WaterBody"),
            sub("Ground"),
            sub("StrategyCamera"),
            sub("WatchPost"),
            sub("OreSilo"),
            sub("TerrainFlat"),
            sub("TerrainHill_N"),
            sub("TerrainPeak_W"),
            sub("WatchPostLegs"),
            sub("OreSiloBody"),
            sub("OreSiloCap"),
        ];
        for i in 0..12 {
            subs.push(sub(&format!("OreCrystal{i}")));
        }
        let (out, _) = filter_subjects(subs, SubjectFilterMode::GameplayPrefer, 24);
        assert!(out.iter().any(|s| s.name == "WaterBody"), "WaterBody must survive filter");
        assert!(out.iter().any(|s| s.name == "WatchPost"));
        assert!(out.iter().any(|s| s.name == "TerrainPeak_W"));
        assert!(!out.iter().any(|s| s.name.starts_with("OreCrystal")));
        assert!(
            !out.iter().any(|s| s.name == "WatchPostLegs" || s.name == "OreSiloBody"),
            "child mesh parts must not crowd filter"
        );
        // StrategyCamera is PRIMARY_EXACT tier 1 ahead of WaterBody/Ground
        assert_eq!(rank_primary_subject(&out).as_deref(), Some("StrategyCamera"));
    }

    /// F2: multiparts must not crowd out OreCrusher under max_subjects=24.
    #[test]
    fn filter_keeps_ore_crusher_when_multiparts_crowd() {
        let mut subs = vec![
            sub("StrategyCamera"),
            sub("Ground"),
            sub("WaterBody"),
            sub("TerrainFlat"),
            sub("TerrainHill_N"),
            sub("TerrainPeak_N"),
            sub("OreCrusher"),
            sub("LoadingBay"),
            sub("SupplyCrate"),
            sub("AshPlateau"),
            sub("TerrainSaddle"),
            sub("FieldScrap_A"),
            sub("CliffRidge_West"),
            sub("WatchPost"),
            sub("RadarDome"),
        ];
        // Flood with F2 child mesh parts that previously crowded the subject cap.
        for n in [
            "ScrapPipe",
            "ScrapBase",
            "ScrapShardA",
            "SaddleDeck",
            "SaddleAbutL",
            "SaddleAbutR",
            "SaddleCrown",
            "CliffFace",
            "CliffLedge",
            "CliffButtress",
            "CliffCap",
            "CrateBody",
            "CrateLid",
            "CrateBand",
            "CrateFoot0",
            "AshBase",
            "AshBerm",
            "AshMound",
            "CrusherChassis",
            "CrusherHopper",
            "CrusherJawL",
            "CrusherFlywheel",
            "RockPeak",
            "RockShelf",
            "DrillBit",
            "BeltDeck",
            "OwnershipFlag",
        ] {
            subs.push(sub(n));
        }
        let (out, _) = filter_subjects(subs, SubjectFilterMode::GameplayPrefer, 24);
        assert!(
            out.iter().any(|s| s.name == "OreCrusher"),
            "OreCrusher must survive multiparts; got {:?}",
            out.iter().map(|s| s.name.as_str()).collect::<Vec<_>>()
        );
        assert!(!out.iter().any(|s| s.name == "ScrapPipe" || s.name == "SaddleAbutL"));
        assert!(!out.iter().any(|s| s.name == "CrusherChassis" || s.name == "CliffFace"));
        assert!(gameplay_subject_score("OreCrusher") > 0);
        assert!(is_noise_name("ScrapPipe") || gameplay_subject_score("ScrapPipe") <= 0);
        // Dogfood stem beats generic placed machines with spaces.
        assert!(
            gameplay_subject_score("OreCrusher") > gameplay_subject_score("Wooden Chest"),
            "OreCrusher={} Wooden Chest={}",
            gameplay_subject_score("OreCrusher"),
            gameplay_subject_score("Wooden Chest")
        );
        assert!(
            gameplay_subject_score("OreCrusher") > gameplay_subject_score("DrillTower"),
            "OreCrusher must beat DrillTower child"
        );
    }

    #[test]
    fn collapse_duplicate_names_counts() {
        let subs = vec![
            sub("OreCrystal0"),
            sub("OreCrystal0"),
            sub("OreCrystal0"),
            sub("Player"),
        ];
        let out = collapse_duplicate_names(subs);
        assert_eq!(out.len(), 2);
        let ore = out.iter().find(|s| s.name == "OreCrystal0").unwrap();
        assert_eq!(ore.duplicate_count, Some(3));
        let p = out.iter().find(|s| s.name == "Player").unwrap();
        assert!(p.duplicate_count.is_none());
    }

    #[test]
    fn apply_profile_iron_feud_sets_require_playing() {
        let mut o = SeeOptions::default();
        apply_game_profile(&mut o, "iron-feud");
        assert!(o.require_playing);
        assert!(matches!(o.projection, ProjectionMode::TopDown3d));
        assert!(o.wait_for_subjects.iter().any(|s| s == "WaterBody"));
        assert_eq!(o.visible_half_w, 22.0);
        assert_eq!(o.visible_half_h, 22.0);
    }

    #[test]
    fn dogfood_stems_all_score_positive_including_r1_r2() {
        assert!(
            all_dogfood_stems_score_positive(),
            "every DOGFOOD_NAME_STEMS entry must score >0"
        );
        for name in [
            "SolarFlareBuoy",
            "WarpGateRing",
            "RadarDome",
            "TerrainSaddle",
        ] {
            assert!(
                gameplay_subject_score(name) > 0,
                "{name} score={}",
                gameplay_subject_score(name)
            );
        }
        // Survive filter with OreCrystal spam
        let mut subs: Vec<_> = [
            "SolarFlareBuoy",
            "WarpGateRing",
            "RadarDome",
            "TerrainSaddle",
            "WaterBody",
            "Player",
        ]
        .iter()
        .map(|n| sub(n))
        .collect();
        for i in 0..16 {
            subs.push(sub(&format!("OreCrystal{i}")));
        }
        let (out, _) = filter_subjects(subs, SubjectFilterMode::GameplayPrefer, 24);
        assert!(out.iter().any(|s| s.name == "SolarFlareBuoy"));
        assert!(out.iter().any(|s| s.name == "WarpGateRing"));
        assert!(out.iter().any(|s| s.name == "RadarDome"));
        assert!(out.iter().any(|s| s.name == "TerrainSaddle"));
        assert!(out.iter().any(|s| s.name == "WaterBody"));
        assert!(!out.iter().any(|s| s.name.starts_with("OreCrystal")));
    }

    #[test]
    fn alt_camera_nudge_topdown_uses_side_xz() {
        let cam = [3.0, 28.0, 10.0];
        let n = alt_camera_nudge_translation(cam, true);
        assert!(n[0] > cam[0], "side X offset");
        assert!(n[2] > cam[2], "side Z offset");
        assert!(n[1] >= 28.0, "Y lift retained");
        let n2 = alt_camera_nudge_translation([0.0, 0.0, 0.0], false);
        assert!((n2[0] - 220.0).abs() < 0.01);
    }

    #[test]
    fn png_nonblack_and_true_magenta_gates() {
        let black = solid_png(16, 16, [0, 0, 0, 255]);
        let frac = png_nonblack_fraction(&black, 30).unwrap();
        assert!(frac < 0.05, "black frame nonblack={frac}");
        let bright = solid_png(16, 16, [200, 180, 160, 255]);
        let frac_b = png_nonblack_fraction(&bright, 30).unwrap();
        assert!(frac_b > 0.9, "bright frame nonblack={frac_b}");
        let magenta = solid_png(8, 8, [255, 0, 255, 255]);
        assert!(png_true_magenta_pixel_count(&magenta).unwrap() > 0);
        // Clean craft: cyan opaque on transparent — not true magenta
        let mut craft = RgbaImage {
            width: 8,
            height: 8,
            pixels: [0u8, 0, 0, 0].repeat(64),
        };
        for y in 2..6 {
            for x in 2..6 {
                let i = (y * 8 + x) * 4;
                craft.pixels[i] = 40;
                craft.pixels[i + 1] = 200;
                craft.pixels[i + 2] = 220;
                craft.pixels[i + 3] = 255;
            }
        }
        let bytes = craft.encode_png().unwrap();
        assert_eq!(png_true_magenta_pixel_count(&bytes).unwrap(), 0);
        assert!(png_nonblack_fraction(&bytes, 30).unwrap() > 0.1);
    }

    #[test]
    fn side_orbit_differs_from_primary_nudge() {
        let cam = [3.0, 28.0, 10.0];
        let a = alt_camera_nudge_translation(cam, true);
        let b = side_orbit_camera_translation(cam, true);
        assert!((a[0] - b[0]).abs() > 5.0 || (a[2] - b[2]).abs() > 5.0);
        assert!(b[1] < a[1] || b[2] != a[2]);
    }

    #[test]
    fn translation_value_for_brp_is_f32_sequence() {
        let map = json!({ "x": 1.5, "y": 2.0, "z": 3.25 });
        let v = translation_value_for_brp(&map);
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 3);
        assert!((arr[0].as_f64().unwrap() - 1.5).abs() < 1e-9);
        let already = json!([9.0, 8.0, 7.0]);
        let v2 = translation_value_for_brp(&already);
        assert_eq!(v2.as_array().unwrap().len(), 3);
    }

    #[test]
    fn h2_h3_name_stems_score_positive() {
        for name in ["PulseMine", "MineDrone", "LoadingBay", "PipeJunction"] {
            assert!(
                gameplay_subject_score(name) > 0,
                "{name} score={}",
                gameplay_subject_score(name)
            );
            assert!(DOGFOOD_NAME_STEMS.iter().any(|s| *s == name));
        }
        assert!(all_dogfood_stems_score_positive());
    }

    #[test]
    fn f1_f2_name_stems_score_positive() {
        for name in ["CargoPod", "RepairDrone", "OreCrusher", "SignalRelay"] {
            assert!(
                gameplay_subject_score(name) > 0,
                "{name} score={}",
                gameplay_subject_score(name)
            );
            assert!(DOGFOOD_NAME_STEMS.iter().any(|s| *s == name));
        }
        assert!(all_dogfood_stems_score_positive());
    }

    #[test]
    fn side_orbit_high_strategy_y_drops_and_offsets() {
        let high = [10.0, 48.0, 12.0];
        let o = side_orbit_camera_translation(high, true);
        assert!(o[1] < high[1], "orbit Y should drop for high strategy cam");
        assert!((o[0] - high[0]).abs() >= 28.0);
        assert!((o[2] - high[2]).abs() >= 26.0);
    }

    #[test]
    fn captures_look_similar_detects_near_identical_pngs() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        let c = dir.path().join("c.png");
        fs::write(&a, solid_png(32, 32, [40, 80, 120, 255])).unwrap();
        fs::write(&b, solid_png(32, 32, [40, 80, 120, 255])).unwrap();
        fs::write(&c, solid_png(32, 32, [200, 20, 20, 255])).unwrap();
        assert!(captures_look_similar(&a, &b).unwrap());
        assert!(!captures_look_similar(&a, &c).unwrap());
        // Near-identical: one pixel differ still under threshold for 32x32
        let near = solid_png(32, 32, [40, 80, 120, 255]);
        // decode, flip one pixel, re-encode via RgbaImage
        let mut img = RgbaImage::decode_png(&near).unwrap();
        img.pixels[0] = 41;
        img.save_png(&dir.path().join("near.png")).unwrap();
        assert!(
            captures_look_similar(&a, &dir.path().join("near.png")).unwrap(),
            "tiny channel delta must still be views_similar"
        );
    }

    #[test]
    fn crop_half_inflates_tall_entities() {
        let def = DEFAULT_CROP_HALF;
        assert_eq!(crop_half_for_entity("Player", def), def);
        let tall = crop_half_for_entity("WatchPost", def);
        assert!(tall > def, "WatchPost half {tall} > {def}");
        assert!(crop_half_for_entity("RadarDome", def) > def);
        assert!(crop_half_for_entity("TerrainPeak_N", def) > def);
        // Explicit large half not reduced
        assert_eq!(crop_half_for_entity("WatchPost", 200), 200);
    }

    #[test]
    fn apply_profile_crystal_drift_ortho2d() {
        let mut o = SeeOptions::default();
        apply_game_profile(&mut o, "crystal-drift");
        assert!(!o.require_playing);
        assert!(matches!(o.projection, ProjectionMode::Ortho2d));
        assert!(o.wait_for_subjects.iter().any(|s| s == "Player"));
        assert_eq!(o.visible_half_w, 640.0);
        assert_eq!(o.visible_half_h, 360.0);
    }

    #[test]
    fn region_preset_hud_top_left_rect() {
        let (x, y, w, h) = region_preset_rect("hud_top_left", 1280, 720).unwrap();
        assert_eq!((x, y), (0, 0));
        assert!(w >= 64 && w <= 1280);
        assert!(h >= 48 && h <= 720);
        assert!(w <= 1280 / 2 + 1); // roughly left third
        let (x2, y2, w2, h2) = RegionPreset::HudTopBar.rect(1280, 720);
        assert_eq!((x2, y2), (0, 0));
        assert_eq!(w2, 1280);
        assert!(h2 < 720);
    }

    #[test]
    fn region_preset_horizon_and_center() {
        let (x, y, w, h) = RegionPreset::HorizonBand.rect(1280, 720);
        assert_eq!((x, y, w), (0, 0, 1280));
        assert!(h <= 720 / 2);
        let (cx, cy, cw, ch) = RegionPreset::CenterHalf.rect(1280, 720);
        assert!(cx > 0 && cy > 0);
        assert_eq!(cw, 640);
        assert_eq!(ch, 360);
    }

    #[test]
    fn d1_d2_name_hints_score_above_zero() {
        for name in [
            "CometFragment",
            "SignalSat",
            "MineDrone",
            "WatchPost",
            "OreSilo",
            "PipeJunction",
            "TerrainFlat",
            "TerrainHill_N",
            "TerrainPeak_A",
            "HeightTerrain",
        ] {
            assert!(
                gameplay_subject_score(name) > 0,
                "{name} must survive gameplay_prefer (score={})",
                gameplay_subject_score(name)
            );
        }
        // Stars still demoted
        assert!(gameplay_subject_score("Star12") < 0 || gameplay_subject_score("Star12") <= 0);
    }

    #[test]
    fn height_band_subjects_and_note() {
        let subs = vec![
            sub("OreCrystal0"),
            sub("TerrainFlat"),
            sub("TerrainHill_E"),
            sub("TerrainPeak_N"),
            sub("WatchPost"),
        ];
        let bands = height_band_subjects(&subs);
        assert!(bands.iter().any(|n| n == "TerrainFlat"));
        assert!(bands.iter().any(|n| n.contains("Hill")));
        assert!(bands.iter().any(|n| n.contains("Peak")));
        let note = height_readability_note(&subs).expect("note when bands present");
        assert!(note.contains("height_bands"));
        assert!(height_readability_note(&[sub("Player"), sub("WaterBody")]).is_none());
    }

    #[test]
    fn known_packs_include_2d_and_3d() {
        assert!(is_known_pack("landscape"));
        assert!(is_known_pack("water"));
        assert!(is_known_pack("hud"));
        assert!(is_known_pack("env_2d"));
        assert!(is_known_pack("entity_craft"));
        assert!(is_known_pack("diagnostic"));
        assert!(!is_known_pack("livestream"));
        assert!(!is_known_pack("taste_scorer"));
    }

    #[test]
    fn filter_keeps_new_dogfood_names() {
        let subs = vec![
            sub("Star0"),
            sub("Star1"),
            sub("CometFragment"),
            sub("SignalSat"),
            sub("WatchPost"),
            sub("OreSilo"),
            sub("TerrainPeak_W"),
            sub("Player"),
        ];
        let (out, _) = filter_subjects(subs, SubjectFilterMode::GameplayPrefer, 24);
        assert!(out.iter().any(|s| s.name == "CometFragment"));
        assert!(out.iter().any(|s| s.name == "SignalSat"));
        assert!(out.iter().any(|s| s.name == "WatchPost"));
        assert!(out.iter().any(|s| s.name == "OreSilo"));
        assert!(out.iter().any(|s| s.name.contains("TerrainPeak")));
        assert!(out.iter().any(|s| s.name == "Player"));
        assert!(!out.iter().any(|s| s.name.starts_with("Star")));
    }

    #[test]
    fn diagnostic_primary_allowlist() {
        let subs = vec![sub("OreCrystal0"), sub("FieldScrap_A")];
        // WaterBody exact not present; rank may pick FieldScrap via prefix FieldScrap
        let n = diagnostic_primary_name(&subs);
        assert!(n.contains("FieldScrap") || n == "FieldScrap_A");
    }

    #[test]
    fn filter_subjects_gameplay_prefer_caps_stars() {
        let mut subs = vec![
            EyesightSubject {
                parent_entity: None,
                name: "Star".into(),
                entity: Some(1),
                translation: None,
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: None,
                screen_aabb: None,
            duplicate_count: None,
            },
            EyesightSubject {
                parent_entity: None,
                name: "Player".into(),
                entity: Some(2),
                translation: Some([0.0, 0.0, 0.0]),
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: None,
                screen_aabb: None,
            duplicate_count: None,
            },
            EyesightSubject {
                parent_entity: None,
                name: "WaterBody".into(),
                entity: Some(3),
                translation: None,
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: None,
                screen_aabb: None,
            duplicate_count: None,
            },
        ];
        for i in 0..20 {
            subs.push(EyesightSubject {
                parent_entity: None,
                name: format!("Star{i}"),
                entity: Some(10 + i),
                translation: None,
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: None,
                screen_aabb: None,
            duplicate_count: None,
            });
        }
        let (out, _) = filter_subjects(subs, SubjectFilterMode::GameplayPrefer, 10);
        assert!(out.iter().any(|s| s.name == "Player"));
        assert!(out.iter().any(|s| s.name.contains("Water")));
        assert!(!out.iter().any(|s| s.name.starts_with("Star")));
    }

    #[test]
    fn infer_playing_vs_menu() {
        let playing = vec![EyesightSubject {
            parent_entity: None,
            name: "Player".into(),
            entity: None,
            translation: None,
            on_screen_estimate: None,
            on_screen: None,
            screen_xy: None,
            screen_aabb: None,
        duplicate_count: None,
        }];
        assert_eq!(infer_app_state_from_subjects(&playing).as_deref(), Some("Playing"));
        let menu = vec![EyesightSubject {
            parent_entity: None,
            name: "MenuCamera".into(),
            entity: None,
            translation: None,
            on_screen_estimate: None,
            on_screen: None,
            screen_xy: None,
            screen_aabb: None,
        duplicate_count: None,
        }];
        assert!(subjects_look_menu_only(&menu));
        assert!(!subjects_look_menu_only(&playing));
    }

    #[test]
    fn world_to_screen_and_annotate() {
        let mut subs = vec![EyesightSubject {
            parent_entity: None,
            name: "Player".into(),
            entity: Some(1),
            translation: Some([0.0, 0.0, 0.0]),
            on_screen_estimate: None,
            on_screen: None,
            screen_xy: None,
            screen_aabb: None,
        duplicate_count: None,
        }];
        annotate_subjects_projection(
            &mut subs,
            [0.0, 0.0, 0.0],
            ProjectionMode::Ortho2d,
            640.0,
            360.0,
            1280,
            720,
            32,
        );
        assert!(subs[0].screen_xy.is_some());
        assert_eq!(subs[0].on_screen, Some(true));
    }

    #[test]
    fn black_frame_allows_dark_scene_with_bright_sprite() {
        // Mostly black "space" with a bright 4x4 ship in the center — not empty.
        let mut img = RgbaImage {
            width: 64,
            height: 64,
            pixels: [0u8, 0, 0, 255].repeat(64 * 64),
        };
        for y in 30..34 {
            for x in 30..34 {
                let i = ((y * 64 + x) * 4) as usize;
                img.pixels[i] = 240;
                img.pixels[i + 1] = 240;
                img.pixels[i + 2] = 255;
                img.pixels[i + 3] = 255;
            }
        }
        let bytes = img.encode_png().unwrap();
        assert!(
            !is_mostly_black_png(&bytes, 0.04).unwrap(),
            "dark space with bright sprite must not be flagged empty"
        );
    }

    #[test]
    fn montage_horizontal_width_sums() {
        let a = RgbaImage {
            width: 4,
            height: 4,
            pixels: [255, 0, 0, 255].repeat(16),
        };
        let b = RgbaImage {
            width: 4,
            height: 4,
            pixels: [0, 255, 0, 255].repeat(16),
        };
        let m = montage_horizontal(&[a, b], 2).unwrap();
        assert_eq!(m.width, 4 + 2 + 4);
        assert_eq!(m.height, 4);
    }

    #[test]
    fn subjects_from_query_parses_name_transform() {
        let q = json!([
            {
                "entity": 7,
                "components": {
                    "bevy_ecs::name::Name": "Player",
                    "bevy_transform::components::transform::Transform": {
                        "translation": { "x": 1.0, "y": 2.0, "z": 3.0 }
                    }
                }
            }
        ]);
        let s = subjects_from_query(&q);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "Player");
        assert_eq!(s[0].entity, Some(7));
        assert_eq!(s[0].translation, Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn world_to_screen_ortho_center() {
        let (sx, sy) = world_to_screen_ortho(0.0, 0.0, 0.0, 0.0, 10.0, 10.0, 100, 100);
        assert!((sx as i32 - 50).abs() <= 1);
        assert!((sy as i32 - 50).abs() <= 1);
    }

    /// Regression: IF fovea must use XZ + iron-feud half extents, not ortho XY (centers everything).
    #[test]
    fn topdown3d_projects_ore_crusher_off_center() {
        // StrategyCamera [3, 28, 10], OreCrusher [14.5, 0, 7.5], half 22 (iron-feud profile).
        let cam = [3.0, 28.0, 10.0];
        let crusher = [14.5, 0.0, 7.5];
        let (sx, sy) = world_to_screen_ortho(
            crusher[0],
            crusher[2],
            cam[0],
            cam[2],
            22.0,
            22.0,
            2560,
            1440,
        );
        // Expect ~1949, ~801 (not near image center 1280,720).
        assert!(
            sx > 1800 && sx < 2100,
            "topdown X should place crusher right of center, got sx={sx}"
        );
        assert!(
            (sy as i32 - 801).abs() < 40,
            "topdown Z→sy near 801, got sy={sy}"
        );

        // Bug path: ortho2d using Y (height) + half 640 → near center (~1303,776).
        let (bad_x, bad_y) = world_to_screen_ortho(
            crusher[0],
            crusher[1],
            cam[0],
            cam[1],
            640.0,
            360.0,
            2560,
            1440,
        );
        assert!(
            (bad_x as i32 - 1303).abs() < 30,
            "documents ortho-misaim sx≈1303, got {bad_x}"
        );
        assert!((bad_y as i32 - 776).abs() < 30, "documents ortho-misaim sy≈776, got {bad_y}");
        assert!(sx != bad_x, "topdown and ortho must differ for this pose");
    }

    #[test]
    fn annotate_topdown_uses_xz_not_xy() {
        let mut subs = vec![
            EyesightSubject {
                parent_entity: None,
                name: "OreCrusher".into(),
                entity: Some(1),
                translation: Some([14.5, 0.0, 7.5]),
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: None,
                screen_aabb: None,
                duplicate_count: None,
            },
        ];
        let cam = [3.0, 28.0, 10.0];
        annotate_subjects_projection(
            &mut subs,
            cam,
            ProjectionMode::TopDown3d,
            22.0,
            22.0,
            2560,
            1440,
            96,
        );
        let xy = subs[0].screen_xy.expect("screen_xy");
        assert!(xy[0] > 1800, "got {:?}", xy);
    }

    #[test]
    fn match_subjects_prefers_exact_name() {
        let subs = vec![
            EyesightSubject {
                parent_entity: None,
                name: "OreSilo".into(),
                entity: Some(1),
                translation: Some([2.5, 0.0, 8.5]),
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: Some([100, 100]),
                screen_aabb: None,
                duplicate_count: None,
            },
            EyesightSubject {
                parent_entity: None,
                name: "OreCrusher".into(),
                entity: Some(2),
                translation: Some([14.5, 0.0, 7.5]),
                on_screen_estimate: None,
                on_screen: None,
                screen_xy: Some([1949, 801]),
                screen_aabb: None,
                duplicate_count: None,
            },
        ];
        let m = match_subjects_for_entity(&subs, "OreCrusher");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "OreCrusher");
        assert_eq!(m[0].screen_xy, Some([1949, 801]));
    }

    #[test]
    fn structural_child_via_parent_entity_demotes() {
        let parent = EyesightSubject {
            name: "OreCrusher".into(),
            entity: Some(10),
            translation: Some([14.5, 0.0, 7.5]),
            parent_entity: None,
            ..Default::default()
        };
        let child = EyesightSubject {
            name: "CrusherChassis".into(),
            entity: Some(11),
            translation: Some([14.5, 0.3, 7.5]),
            parent_entity: Some(10),
            ..Default::default()
        };
        let all = vec![parent.clone(), child.clone()];
        assert!(is_structural_child_of_named_root(&child, &all));
        assert!(!is_structural_child_of_named_root(&parent, &all));
        let (out, _) = filter_subjects(all, SubjectFilterMode::GameplayPrefer, 24);
        assert!(out.iter().any(|s| s.name == "OreCrusher"));
        assert!(!out.iter().any(|s| s.name == "CrusherChassis"));
    }

    #[test]
    fn solar_flare_buoy_not_demoted_as_solar_prefix() {
        assert!(!is_child_mesh_part("SolarFlareBuoy"));
        assert!(is_protected_top_level_name("SolarFlareBuoy"));
        assert!(gameplay_subject_score("SolarFlareBuoy") > 0);
    }

    #[test]
    fn pack_camera_restore_value_is_f32_sequence() {
        let v = pack_camera_restore_value([3.0, 28.0, 10.0]);
        let a = v.as_array().expect("array");
        assert_eq!(a.len(), 3);
        assert!((a[0].as_f64().unwrap() - 3.0).abs() < 1e-9);
        assert!((a[1].as_f64().unwrap() - 28.0).abs() < 1e-9);
    }

    #[test]
    fn dedicated_side_is_more_aggressive_than_side_orbit() {
        let cam = [3.0, 48.0, 10.0];
        let s = side_orbit_camera_translation(cam, true);
        let d = dedicated_side_camera_translation(cam, true);
        assert!((d[0] - cam[0]).abs() >= (s[0] - cam[0]).abs());
        assert!(d[1] <= s[1]);
    }

    #[test]
    fn extract_parent_entity_from_child_of() {
        let comps = json!({
            "bevy_ecs::hierarchy::ChildOf": { "0": 42 }
        });
        assert_eq!(extract_parent_entity(&comps), Some(42));
    }

    #[test]
    fn parent_map_from_childof_bare_u64() {
        // Live IF shape: ChildOf-only query returns bare parent entity id.
        let q = json!([
            {
                "entity": 100,
                "components": { "bevy_ecs::hierarchy::ChildOf": 10 }
            },
            {
                "entity": 101,
                "components": { "bevy_ecs::hierarchy::ChildOf": 10 }
            }
        ]);
        let map = parent_map_from_childof_query(&q);
        assert_eq!(map.get(&100), Some(&10));
        assert_eq!(map.get(&101), Some(&10));
    }

    #[test]
    fn find_dedicated_view_camera_prefers_named() {
        let subs = vec![
            EyesightSubject {
                name: "StrategyCamera".into(),
                entity: Some(1),
                translation: Some([3.0, 28.0, 10.0]),
                parent_entity: None,
                ..Default::default()
            },
            EyesightSubject {
                name: "AgentSightSideCamera".into(),
                entity: Some(2),
                translation: Some([-20.0, 12.0, 30.0]),
                parent_entity: None,
                ..Default::default()
            },
        ];
        let (e, t) = find_dedicated_view_camera(&subs).expect("side cam");
        assert_eq!(e, 2);
        assert!((t[0] + 20.0).abs() < 1e-6);
    }

    #[test]
    fn fovea_crop_nonblack_min_is_strict() {
        assert!(FOVEA_CROP_NONBLACK_MIN > FULL_FRAME_NONBLACK_MIN);
        assert!(FOVEA_CRAFT_LUMA_MIN > 30);
        assert!(FOVEA_CROP_MEAN_LUMA_MIN > 30.0);
    }

    /// S3 real path: shadow/deep-green crops must trip fovea_dark; bright craft must not.
    #[test]
    fn fovea_crop_is_too_dark_on_shadow_fixture() {
        // Matches live OreCrusher deep-shadow profile: mean luma ~31, thr=30 nonblack ~0.28
        // would NOT warn under the old min=0.05 @ thr=30.
        let shadow = solid_png(48, 48, [28, 32, 28, 255]);
        let frac30 = png_nonblack_fraction(&shadow, 30).unwrap();
        // solid 28 luma is below thr 30 → frac0; mix some brighter shadow pixels
        let mut img = RgbaImage::decode_png(&shadow).unwrap();
        // paint ~28% of pixels at luma ~40 (above thr 30, below craft thr 48)
        let step = 4usize;
        for y in (0..img.height as usize).step_by(step) {
            for x in 0..(img.width as usize / 4) {
                let i = (y * img.width as usize + x) * 4;
                img.pixels[i] = 40;
                img.pixels[i + 1] = 42;
                img.pixels[i + 2] = 38;
            }
        }
        let mixed = img.encode_png().unwrap();
        let frac30m = png_nonblack_fraction(&mixed, 30).unwrap();
        assert!(
            frac30m > 0.05,
            "fixture must exceed old thr=30 min=0.05 gate, got {frac30m}"
        );
        assert!(
            fovea_crop_is_too_dark(&mixed).unwrap(),
            "shadow/mixed crop must warn fovea_dark (mean low / craft nonblack low)"
        );
        let bright = solid_png(48, 48, [120, 110, 90, 255]);
        assert!(
            !fovea_crop_is_too_dark(&bright).unwrap(),
            "well-lit crop must not warn fovea_dark"
        );
        // Mean luma alone catches pure near-black
        assert!(fovea_crop_is_too_dark(&solid_png(16, 16, [10, 10, 10, 255])).unwrap());
        let _ = frac30;
    }

    #[test]
    fn capture_nudge_source_restores_after_capture() {
        // Structural contract: restore mutate runs after capture attempt (T2).
        let src = include_str!("eyesight.rs");
        let fn_start = src.find("fn capture_with_camera_nudge").expect("fn");
        let body = &src[fn_start..fn_start + 2500];
        assert!(body.contains("restore_t"));
        assert!(
            body.find("capture_viewport_image").unwrap()
                < body.rfind("mutate_components(entity, c, \"translation\", restore_t").unwrap()
                || body.contains("Always") && body.contains("restore"),
            "restore must follow capture in finally-style path"
        );
    }

    #[test]
    fn write_diff_png_score_positive_when_different() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        let d = dir.path().join("d.png");
        fs::write(&a, solid_png(8, 8, [0, 0, 0, 255])).unwrap();
        fs::write(&b, solid_png(8, 8, [255, 255, 255, 255])).unwrap();
        let (_p, score) = write_diff_png(&a, &b, &d).unwrap();
        assert!(score > 0.5);
        assert!(d.is_file());
    }

    #[test]
    fn sanitize_and_eyesight_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = eyesight_path(dir.path(), "x.png");
        assert!(p.ends_with("captures/eyesight/x.png") || p.to_string_lossy().contains("eyesight"));
        assert_eq!(sanitize_name("Player Ship!"), "Player_Ship_");
    }

    #[test]
    fn fixture_png_still_valid() {
        validate_roundtrip_fixture();
    }

    fn validate_roundtrip_fixture() {
        let bytes = fixture_png_bytes();
        let img = RgbaImage::decode_png(&bytes).unwrap();
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
    }
}

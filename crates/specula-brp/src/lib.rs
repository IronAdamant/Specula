//! Bevy Remote Protocol (BRP) HTTP client and screenshot/image helpers.
//!
//! Integrates with apps that enable Bevy's `RemotePlugin` + `RemoteHttpPlugin`
//! (or `bevy_brp_extras::BrpExtrasPlugin`, which sets those up). Full agent
//! tool surface is provided by `bevy_brp_mcp`; this crate is the thin client
//! layer Specula uses for CLI, MCP tools, and tests.

pub mod eyesight;

pub use eyesight::{
    all_dogfood_stems_score_positive, alt_camera_nudge_translation, annotate_subjects_projection,
    apply_game_profile, captures_look_similar, collapse_duplicate_names, crop_half_for_entity,
    crop_png_around, crop_png_file, crop_rgba, dedicated_side_camera_translation,
    diagnostic_primary_name, dogfood_stems_needing_positive_score, draw_rect_outline,
    extract_parent_entity, eyesight_path, file_content_hash, filter_subjects,
    find_dedicated_view_camera, gameplay_subject_score, height_band_subjects,
    height_readability_note, infer_app_state_from_subjects, is_child_mesh_part, is_known_pack,
    is_mostly_black_png, is_noise_subject, is_protected_top_level_name,
    is_structural_child_of_named_root, match_subjects_for_entity, mean_abs_diff,
    montage_horizontal, packet_to_mcp_content, pack_camera_restore_value,
    parent_map_from_childof_query, path_nonblack_fraction, path_true_magenta_pixel_count,
    png_nonblack_fraction, png_true_magenta_pixel_count, query_all_subjects, rank_primary_subject,
    region_preset_rect, see_diff, see_entity, see_motion, see_pack, see_region, see_scene,
    see_verify, side_orbit_camera_translation, subjects_from_query, subjects_look_menu_only,
    translation_value_for_brp, wait_for_subject_names, world_to_screen_ortho, write_diff_png,
    CaptureEntry, CaptureRole, EyesightPacket, EyesightSubject, ProjectionMode, RegionPreset,
    RgbaImage, SeeOptions, StimulusInfo, SubjectFilterMode, ACUITY_LABEL, DEFAULT_CROP_HALF,
    DEFAULT_MAX_SUBJECTS, DEFAULT_MOTION_FRAMES, DEFAULT_MOTION_INTERVAL_MS, DOGFOOD_NAME_STEMS,
    EYESIGHT_SCHEMA, FOVEA_CROP_NONBLACK_MIN, FULL_FRAME_NONBLACK_MIN, GAMEPLAY_NAME_HINTS,
    HEIGHT_BAND_NAME_HINTS, KNOWN_PACKS, TALL_ENTITY_PREFIXES, TRUE_MAGENTA_MAX_ON_SPRITE,
    VIEWS_SIMILAR_MEAN_ABS_MAX,
};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Default BRP HTTP port used by Bevy / bevy_brp_extras.
pub const DEFAULT_PORT: u16 = 15702;

static REQ_ID: AtomicU64 = AtomicU64::new(1);

/// Named target for multi-instance control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrpTarget {
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl BrpTarget {
    pub fn new(name: impl Into<String>, port: u16) -> Self {
        Self {
            name: name.into(),
            host: "127.0.0.1".into(),
            port,
        }
    }

    pub fn default_local() -> Self {
        Self::new("default", DEFAULT_PORT)
    }

    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// JSON-RPC error returned by BRP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrpRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Result of a BRP call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrpCallResult {
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<BrpRpcError>,
}

impl BrpCallResult {
    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }

    pub fn into_result(self) -> Result<Value> {
        if let Some(err) = self.error {
            Err(anyhow!("BRP error {}: {}", err.code, err.message))
        } else {
            Ok(self.result.unwrap_or(Value::Null))
        }
    }
}

/// HTTP JSON-RPC client for a Bevy Remote Protocol endpoint.
#[derive(Debug, Clone)]
pub struct BrpClient {
    pub target: BrpTarget,
    timeout: Duration,
}

impl BrpClient {
    pub fn new(target: BrpTarget) -> Self {
        Self {
            target,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_port(port: u16) -> Self {
        Self::new(BrpTarget::new("default", port))
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Low-level JSON-RPC call.
    pub fn call(&self, method: &str, params: Option<Value>) -> Result<BrpCallResult> {
        let id = REQ_ID.fetch_add(1, Ordering::Relaxed);
        let mut body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": id,
        });
        if let Some(p) = params {
            body["params"] = p;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .context("build HTTP client")?;

        let url = self.target.base_url();
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("POST {url} method={method}"))?;

        let status = response.status();
        let text = response.text().context("read BRP response body")?;
        if !status.is_success() {
            return Err(anyhow!("HTTP {status} from BRP at {url}: {text}"));
        }

        let value: Value = serde_json::from_str(&text)
            .with_context(|| format!("parse BRP JSON: {text}"))?;

        Ok(BrpCallResult {
            id: value.get("id").cloned().unwrap_or(Value::Null),
            result: value.get("result").cloned(),
            error: value
                .get("error")
                .cloned()
                .map(|e| serde_json::from_value(e))
                .transpose()
                .context("parse BRP error object")?,
        })
    }

    /// Wait until the BRP port accepts `rpc.discover` (or any successful call).
    pub fn wait_until_ready(&self, overall: Duration) -> Result<Value> {
        let start = Instant::now();
        let mut last_err = None;
        while start.elapsed() < overall {
            match self.call("rpc.discover", None) {
                Ok(r) if r.is_ok() => return r.into_result(),
                Ok(r) => {
                    last_err = Some(format!("{:?}", r.error));
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(anyhow!(
            "BRP at {} not ready after {:?}: {}",
            self.target.base_url(),
            overall,
            last_err.unwrap_or_else(|| "unknown".into())
        ))
    }

    pub fn query(&self, components: &[&str]) -> Result<Value> {
        let comps: Vec<Value> = components.iter().map(|c| json!(c)).collect();
        self.call(
            "world.query",
            Some(json!({
                "data": {
                    "components": comps,
                },
                "strict": false
            })),
        )?
        .into_result()
    }

    pub fn list_resources(&self) -> Result<Value> {
        self.call("world.list_resources", None)?.into_result()
    }

    pub fn spawn_entity(&self, components: Value) -> Result<Value> {
        self.call(
            "world.spawn_entity",
            Some(json!({ "components": components })),
        )?
        .into_result()
    }

    pub fn insert_components(&self, entity: u64, components: Value) -> Result<Value> {
        self.call(
            "world.insert_components",
            Some(json!({
                "entity": entity,
                "components": components
            })),
        )?
        .into_result()
    }

    pub fn mutate_components(
        &self,
        entity: u64,
        component: &str,
        path: &str,
        value: Value,
    ) -> Result<Value> {
        self.call(
            "world.mutate_components",
            Some(json!({
                "entity": entity,
                "component": component,
                "path": path,
                "value": value
            })),
        )?
        .into_result()
    }

    pub fn reparent_entities(&self, entities: &[u64], parent: Option<u64>) -> Result<Value> {
        let mut params = json!({ "entities": entities });
        if let Some(p) = parent {
            params["parent"] = json!(p);
        }
        self.call("world.reparent_entities", Some(params))?
            .into_result()
    }

    /// Request an in-engine screenshot via `brp_extras/screenshot`.
    ///
    /// Requires `bevy_brp_extras` in the target app and Bevy `png` feature.
    pub fn screenshot(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create screenshot dir {}", parent.display()))?;
        }
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let path_str = abs.to_string_lossy().to_string();

        let result = self
            .call(
                "brp_extras/screenshot",
                Some(json!({ "path": path_str })),
            )?
            .into_result()?;

        // Extras returns only after the PNG is published; still verify on disk.
        let published = result
            .get("path")
            .and_then(|p| p.as_str())
            .map(PathBuf::from)
            .unwrap_or(abs);

        wait_for_file(&published, Duration::from_secs(15))?;
        Ok(published)
    }

    pub fn get_diagnostics(&self) -> Result<Value> {
        self.call("brp_extras/get_diagnostics", None)?.into_result()
    }
}

fn wait_for_file(path: &Path, overall: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < overall {
        if path.is_file() {
            let meta = fs::metadata(path)?;
            if meta.len() > 8 {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(anyhow!(
        "screenshot file not ready at {} after {:?}",
        path.display(),
        overall
    ))
}

/// Validated PNG image bytes plus metadata for MCP image content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedImage {
    pub path: PathBuf,
    pub width_hint: Option<u32>,
    pub height_hint: Option<u32>,
    pub byte_len: usize,
    pub mime_type: String,
    pub png_base64: String,
}

impl CapturedImage {
    /// Load a PNG from disk and prepare MCP-friendly payload fields.
    pub fn from_png_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Self::from_png_bytes(path.to_path_buf(), bytes)
    }

    /// Validate PNG magic header and encode for transport.
    pub fn from_png_bytes(path: PathBuf, bytes: Vec<u8>) -> Result<Self> {
        validate_png_header(&bytes)?;
        let (w, h) = read_png_ihdr_size(&bytes).unwrap_or((None, None));
        Ok(Self {
            path,
            width_hint: w,
            height_hint: h,
            byte_len: bytes.len(),
            mime_type: "image/png".into(),
            png_base64: B64.encode(&bytes),
        })
    }

    /// Absolute path for agents when chat UI truncates the image payload.
    pub fn absolute_path_display(&self) -> String {
        match self.path.canonicalize() {
            Ok(p) => p.display().to_string(),
            Err(_) if self.path.is_absolute() => self.path.display().to_string(),
            Err(_) => std::env::current_dir()
                .map(|cwd| cwd.join(&self.path).display().to_string())
                .unwrap_or_else(|_| self.path.display().to_string()),
        }
    }

    /// Text metadata always returned with captures (path + size survive UI truncation).
    pub fn metadata_text(&self) -> String {
        let dims = match (self.width_hint, self.height_hint) {
            (Some(w), Some(h)) => format!(" dims={w}x{h}"),
            _ => String::new(),
        };
        format!(
            "Captured PNG bytes={}{} path={} abs_path={} mime={} \
             note=if the chat UI truncates image bytes, open abs_path on disk",
            self.byte_len,
            dims,
            self.path.display(),
            self.absolute_path_display(),
            self.mime_type
        )
    }

    /// MCP tool content block (image) plus text with **absolute path + size**.
    pub fn to_mcp_content_blocks(&self) -> Value {
        json!([
            {
                "type": "image",
                "data": self.png_base64,
                "mimeType": self.mime_type,
            },
            {
                "type": "text",
                "text": self.metadata_text()
            }
        ])
    }
}

/// Capture via BRP extras and return image payload.
pub fn capture_viewport_image(client: &BrpClient, path: impl AsRef<Path>) -> Result<CapturedImage> {
    let published = client.screenshot(path)?;
    CapturedImage::from_png_path(published)
}

/// PNG signature validation (real shipped path used by capture tools).
pub fn validate_png_header(bytes: &[u8]) -> Result<()> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    if bytes.len() < 24 {
        return Err(anyhow!(
            "PNG too small ({} bytes); expected a real screenshot",
            bytes.len()
        ));
    }
    if bytes[..8] != SIG {
        return Err(anyhow!("not a PNG: missing PNG signature"));
    }
    Ok(())
}

fn read_png_ihdr_size(bytes: &[u8]) -> Result<(Option<u32>, Option<u32>)> {
    read_png_ihdr_size_pub(bytes)
}

/// Public IHDR width/height reader (used by eyesight packets).
pub fn read_png_ihdr_size_pub(bytes: &[u8]) -> Result<(Option<u32>, Option<u32>)> {
    // IHDR is the first chunk: 8 sig + 4 len + 4 type + data
    if bytes.len() < 24 {
        return Ok((None, None));
    }
    if &bytes[12..16] != b"IHDR" {
        return Ok((None, None));
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok((Some(w), Some(h)))
}

/// Registry of named BRP targets (multiple running instances).
#[derive(Debug, Default, Clone)]
pub struct TargetRegistry {
    targets: Vec<BrpTarget>,
}

impl TargetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, target: BrpTarget) {
        if let Some(existing) = self.targets.iter_mut().find(|t| t.name == target.name) {
            *existing = target;
        } else {
            self.targets.push(target);
        }
    }

    pub fn get(&self, name: &str) -> Option<&BrpTarget> {
        self.targets.iter().find(|t| t.name == name)
    }

    pub fn list(&self) -> &[BrpTarget] {
        &self.targets
    }

    pub fn resolve(&self, name: Option<&str>, port: Option<u16>) -> BrpTarget {
        if let Some(n) = name {
            if let Some(t) = self.get(n) {
                return t.clone();
            }
        }
        BrpTarget::new(name.unwrap_or("default"), port.unwrap_or(DEFAULT_PORT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid 1x1 PNG (real bytes, not a mock of the unit under test).
    fn fixture_png_bytes() -> Vec<u8> {
        // Precomputed 1x1 red PNG
        let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        B64.decode(b64).expect("fixture b64")
    }

    #[test]
    fn validate_png_header_accepts_real_png() {
        let bytes = fixture_png_bytes();
        validate_png_header(&bytes).unwrap();
        let (w, h) = read_png_ihdr_size(&bytes).unwrap();
        assert_eq!(w, Some(1));
        assert_eq!(h, Some(1));
    }

    #[test]
    fn validate_png_header_rejects_garbage() {
        assert!(validate_png_header(b"not-a-png-file!!!!").is_err());
        assert!(validate_png_header(b"short").is_err());
    }

    #[test]
    fn captured_image_from_bytes_sets_mcp_fields() {
        let bytes = fixture_png_bytes();
        let img =
            CapturedImage::from_png_bytes(PathBuf::from("fixture.png"), bytes.clone()).unwrap();
        assert_eq!(img.mime_type, "image/png");
        assert_eq!(img.byte_len, bytes.len());
        assert!(!img.png_base64.is_empty());
        assert_eq!(img.width_hint, Some(1));
        assert_eq!(img.height_hint, Some(1));

        let blocks = img.to_mcp_content_blocks();
        let arr = blocks.as_array().unwrap();
        assert_eq!(arr[0]["type"], "image");
        assert_eq!(arr[0]["mimeType"], "image/png");
        assert_eq!(arr[0]["data"], img.png_base64);
        assert_eq!(arr[1]["type"], "text");
        let text = arr[1]["text"].as_str().unwrap();
        assert!(text.contains("bytes="));
        assert!(text.contains("abs_path="));
        assert!(text.contains(&format!("{}", img.byte_len)));
    }

    #[test]
    fn captured_image_from_path_reads_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.png");
        fs::write(&path, fixture_png_bytes()).unwrap();
        let img = CapturedImage::from_png_path(&path).unwrap();
        assert_eq!(img.path, path);
        let meta = img.metadata_text();
        assert!(meta.contains("abs_path="));
        assert!(meta.contains("bytes="));
        // abs_path should be absolute after canonicalize
        let abs = img.absolute_path_display();
        assert!(PathBuf::from(&abs).is_absolute() || abs.contains("scene.png"));
        assert!(img.byte_len > 0);
        validate_png_header(&B64.decode(&img.png_base64).unwrap()).unwrap();
    }

    #[test]
    fn target_registry_resolve_and_register() {
        let mut reg = TargetRegistry::new();
        reg.register(BrpTarget::new("game", 15702));
        reg.register(BrpTarget::new("editor", 15703));
        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.get("editor").unwrap().port, 15703);
        let t = reg.resolve(Some("game"), None);
        assert_eq!(t.port, 15702);
        let t2 = reg.resolve(None, Some(16000));
        assert_eq!(t2.port, 16000);
    }

    #[test]
    fn brp_target_base_url() {
        let t = BrpTarget::new("x", 15702);
        assert_eq!(t.base_url(), "http://127.0.0.1:15702");
    }
}

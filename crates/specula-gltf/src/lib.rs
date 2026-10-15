//! glTF 2.0 import and export for Specula (zero dependencies: our own JSON, base64, PNG and
//! JPEG decoders).
//!
//! # Import
//!
//! ```text
//! let asset = specula_gltf::import_file(Path::new("assets/knight.glb"), &ImportOptions::default())?;
//! asset.model            // specula_model::Model: lods[0] = every mesh of the scene, merged by material
//! asset.skeleton         // Option<specula_anim::Skeleton> (rest pose from the nodes)
//! asset.clips            // Vec<specula_anim::Clip>, rest-relative, ready for an AnimSet
//! asset.materials        // full glTF material data + the derived Specula preset
//! asset.textures, asset.images   // decoded RGBA8 images (PNG, JPEG) and samplers
//! asset.warnings         // what was ignored, approximated or repaired
//! asset.report()         // a bounded JSON summary for tool responses
//! ```
//!
//! **Supported:** `.gltf` (JSON + external `.bin`/images resolved inside the asset folder only)
//! and `.glb`; base64 data URIs; every accessor component type, `normalized`, byte strides,
//! matrix column padding and sparse accessors; TRIANGLES, TRIANGLE_STRIP and TRIANGLE_FAN
//! (converted to triangles); POSITION, NORMAL (generated flat when missing, as the spec says),
//! TEXCOORD_0, COLOR_0, JOINTS_0/1 + WEIGHTS_0/1 (top four influences kept), TANGENT (kept in
//! `MeshData::tangents`, moved with the nodes and the skin's bind-to-rest bake); node hierarchies with TRS or matrices;
//! scenes; materials (metallic-roughness factors and textures, normal/occlusion/emissive
//! textures, alpha modes, double-sided, `KHR_materials_emissive_strength`); textures, samplers
//! and PNG/JPEG images; skins; animations (translation/rotation/scale with LINEAR, STEP and
//! CUBICSPLINE); `KHR_mesh_quantization`.
//!
//! **Not supported:** POINTS and LINES primitives (error), morph targets and `weights`
//! animation (ignored with a warning), cameras, lights and other extensions (ignored with a
//! warning; *required* unsupported extensions are an error: Draco, meshopt, KTX2/Basis, WebP),
//! texture transforms, more than [`specula_model::rig::MAX_BONES`] (255) bones after pruning
//! unused leaves.
//!
//! **Images** are decoded only when a material slot uses them, each distinct source (buffer
//! view, file, data URI) once; duplicates point at the first ([`ImportedImage::duplicate_of`]).
//! Missing, unreadable or undecodable images are warnings; size limits are errors.
//!
//! **Memory:** every [`Limits`] budget is checked before the memory it guards is committed:
//! buffers read only their declared `byteLength`, buffers and encoded images share
//! `max_total_bytes`, decoded images share `max_image_bytes` (from their headers), animation
//! keys are counted before key data is read, and vertex counts are estimated per primitive mode.
//!
//! # Conventions
//!
//! glTF is right-handed, +Y up, metres, counter-clockwise front faces, and so is Specula: no
//! axis or winding conversion. Nodes are baked into model space (mirroring transforms flip the
//! winding back to counter-clockwise). glTF colours are linear and Specula's vertex colours and
//! presets are sRGB; conversion goes through [`color`]. UVs keep glTF's top-left origin.
//!
//! # Rigs and clips
//!
//! Bones are every skin joint and every animated node, plus their ancestors (so the hierarchy is
//! exact), in scene order; a synthetic `root` is added when there are several top-level bones
//! or meshes outside every bone. Skinned vertices are baked into the **rest pose**
//! (`Σ w · node_world · inverseBind`), and the skeleton's inverse binds are the inverse rest
//! transforms, so skinning at rest is the identity and Specula skinning reproduces glTF's
//! exactly whenever a vertex's joints agree on the bind-to-rest matrix (true for files whose
//! node pose is the bind pose; otherwise a warning reports how many vertices deviate). Meshes
//! under a bone are bound rigidly to it. glTF clip values are absolute; they are converted to
//! Specula's rest-relative values (`t − rest.t`, `rest.r⁻¹ · r`, `s / rest.s`). Linear rotation
//! keys more than 20° apart get slerp-sampled in-between keys (Specula interpolates with
//! nlerp); CUBICSPLINE channels whose tangents are Catmull–Rom (ours) import as `cubic` keys,
//! others are resampled deterministically to linear keys at [`ImportOptions::resample_hz`].
//!
//! # Export
//!
//! [`export_glb`] / [`export_gltf`] write a Specula model (+ skeleton + clips) as glTF, with
//! Specula specifics in `extras.specula` (material presets, bone tails, clip loop modes,
//! distances, events and curves) so export → import round-trips them.

pub mod accessor;
pub mod color;
pub mod doc;
pub mod error;
mod export;
mod import;

use std::path::Path;

use specula_anim::{AnimError, AnimSet, Clip, LoopMode, Skeleton};
use specula_core::Json;
use specula_model::{ColliderKind, MaterialPreset, Model};

pub use doc::{AlphaMode, Document, Sampler};
pub use error::{GltfError, GltfErrorCode, Warning};
pub use export::{export_glb, export_gltf, ExportInput};

/// Hard caps checked before allocating (foundation F3: hostile files can't exhaust memory).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Largest `.glb`/`.gltf` file, external file or data URI payload.
    pub max_file_bytes: u64,
    /// Most bytes read for buffers and (encoded) images together.
    pub max_total_bytes: u64,
    /// Largest JSON text (`.gltf` file or GLB JSON chunk); parsed JSON takes several times this.
    pub max_json_bytes: u64,
    /// Most entries in any top-level array (nodes, meshes, accessors, ...) or nested list.
    pub max_items: usize,
    /// Most elements in one accessor.
    pub max_accessor_count: u64,
    /// Most vertices / triangles in the imported model (all instances baked).
    pub max_vertices: usize,
    pub max_triangles: usize,
    /// Deepest node hierarchy.
    pub max_node_depth: usize,
    /// Most animation keys over every imported clip (after resampling).
    pub max_total_keys: usize,
    /// Largest decoded image (`width × height`).
    pub max_image_pixels: u64,
    /// Most RGBA8 bytes all decoded images may hold together (checked from their headers).
    pub max_image_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_file_bytes: 256 << 20,
            max_total_bytes: 512 << 20,
            max_json_bytes: 64 << 20,
            max_items: 1 << 16,
            max_accessor_count: 1 << 22,
            max_vertices: 1 << 22,
            max_triangles: 1 << 23,
            max_node_depth: 128,
            max_total_keys: 1 << 22,
            max_image_pixels: 1 << 26,
            max_image_bytes: 256 << 20,
        }
    }
}

/// Import settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportOptions {
    pub limits: Limits,
    /// Scene to import (default: the document's `scene`, else scene 0, else every root node).
    pub scene: Option<usize>,
    /// Decode PNG/JPEG images (off: textures keep their references, `images[i].rgba` is empty).
    pub decode_images: bool,
    /// Multiply the base colour texture (sampled at each vertex's UV) into the vertex colours,
    /// so untextured rendering still shows the asset's colours. Recorded per material.
    pub bake_texture_colors: bool,
    pub collider: ColliderKind,
    /// Sample rate for resampled channels (non-Catmull–Rom CUBICSPLINE, too many keys).
    pub resample_hz: f32,
    /// Loop mode for clips without a Specula loop mode in `extras`.
    pub default_loop: LoopMode,
    /// Model name (default: the file stem, else the scene name, else `gltf`).
    pub name: Option<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            limits: Limits::default(),
            scene: None,
            decode_images: true,
            bake_texture_colors: true,
            collider: ColliderKind::Aabb,
            resample_hz: 60.0,
            default_loop: LoopMode::Once,
            name: None,
        }
    }
}

/// A texture slot of a material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureRef {
    /// Index into [`Import::textures`].
    pub texture: usize,
    /// Which TEXCOORD set (only 0 is kept in `MeshData::uvs`).
    pub tex_coord: u32,
    /// `scale` (normal map) or `strength` (occlusion); 1 for other slots.
    pub scale: f32,
}

/// A glTF material, plus the Specula preset derived from it (or restored from our extras).
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedMaterial {
    /// The preset name the mesh parts use (unique; the glTF name when it has one).
    pub name: String,
    pub preset: MaterialPreset,
    /// Linear RGBA.
    pub base_color: [f32; 4],
    pub base_color_texture: Option<TextureRef>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_texture: Option<TextureRef>,
    pub normal_texture: Option<TextureRef>,
    pub occlusion_texture: Option<TextureRef>,
    /// Linear RGB, before `emissive_strength`.
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub emissive_texture: Option<TextureRef>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// The preset came from `extras.specula` (a Specula export), exactly.
    pub from_specula: bool,
    /// The base colour texture was multiplied into the vertex colours.
    pub texture_baked: bool,
}

/// A texture: an image and how to sample it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedTexture {
    pub name: Option<String>,
    /// Index into [`Import::images`] (`None` when the source is only in an extension).
    pub image: Option<usize>,
    pub sampler: Sampler,
}

/// A decoded image.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedImage {
    pub name: Option<String>,
    /// `image/png` or `image/jpeg` (by content), or the declared type when undecoded.
    pub mime: String,
    pub width: u32,
    pub height: u32,
    /// RGBA8, top row first; empty when not decoded (see `error`).
    pub rgba: Vec<u8>,
    /// Why the image wasn't decoded (unused, unreadable, unsupported format, decoder error,
    /// decoding off).
    pub error: Option<String>,
    /// This image has the same source (buffer view, file or data URI) as an earlier one, whose
    /// pixels textures use instead; `rgba` stays empty here.
    pub duplicate_of: Option<usize>,
}

/// Counts for reports and budgets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub nodes: usize,
    pub meshes: usize,
    pub primitives: usize,
    /// Mesh instances baked (a mesh used by several nodes counts once per node).
    pub instances: usize,
    pub skins: usize,
    pub animations: usize,
    /// Triangles dropped as degenerate (repeated vertex or zero area).
    pub degenerate_triangles: usize,
    /// Bones pruned (unused leaves) to fit the bone limit.
    pub pruned_bones: usize,
}

/// An imported asset.
#[derive(Clone, Debug)]
pub struct Import {
    pub model: Model,
    pub skeleton: Option<Skeleton>,
    pub clips: Vec<Clip>,
    pub materials: Vec<ImportedMaterial>,
    pub textures: Vec<ImportedTexture>,
    pub images: Vec<ImportedImage>,
    pub warnings: Vec<Warning>,
    pub stats: Stats,
    /// `asset.generator` of the file.
    pub generator: Option<String>,
}

impl Import {
    /// An [`AnimSet`] of the skeleton with every clip (`None` for unrigged assets).
    pub fn anim_set(&self) -> Option<Result<AnimSet, AnimError>> {
        let skel = self.skeleton.clone()?;
        let mut set = AnimSet::new(skel);
        for c in &self.clips {
            if let Err(e) = set.add(c.clone()) {
                return Some(Err(e));
            }
        }
        Some(Ok(set))
    }

    /// A bounded summary (counts, materials, clips, at most 32 warnings) for tool responses.
    pub fn report(&self) -> Json {
        let m = &self.model;
        let mesh = &m.lods[0];
        let s = &self.stats;
        Json::obj([
            ("name", short(&m.name, 128)),
            ("generator", self.generator.as_deref().map_or(Json::Null, |g| short(g, 128))),
            ("vertices", Json::from(mesh.vertex_count())),
            ("triangles", Json::from(mesh.triangle_count())),
            ("bounds", Json::obj([("min", Json::arr(m.bounds.min.iter().map(|v| Json::from(*v)))), ("max", Json::arr(m.bounds.max.iter().map(|v| Json::from(*v))))])),
            (
                "counts",
                Json::obj([
                    ("nodes", Json::from(s.nodes)),
                    ("meshes", Json::from(s.meshes)),
                    ("primitives", Json::from(s.primitives)),
                    ("instances", Json::from(s.instances)),
                    ("skins", Json::from(s.skins)),
                    ("animations", Json::from(s.animations)),
                    ("degenerate_triangles", Json::from(s.degenerate_triangles)),
                    ("pruned_bones", Json::from(s.pruned_bones)),
                ]),
            ),
            (
                "materials",
                Json::arr(self.materials.iter().take(64).map(|mt| {
                    Json::obj([
                        ("name", short(&mt.name, 128)),
                        ("alpha", Json::Str(format!("{:?}", mt.alpha_mode).to_lowercase())),
                        ("textured", Json::Bool(mt.base_color_texture.is_some())),
                        ("from_specula", Json::Bool(mt.from_specula)),
                    ])
                })),
            ),
            ("images", Json::arr(self.images.iter().take(64).map(|i| Json::obj([("mime", short(&i.mime, 64)), ("width", Json::from(i.width)), ("height", Json::from(i.height)), ("decoded", Json::Bool(!i.rgba.is_empty()))])))),
            ("bones", Json::from(self.skeleton.as_ref().map_or(0, |s| s.len()))),
            ("clips", Json::arr(self.clips.iter().take(64).map(|c| Json::obj([("name", short(&c.name, 128)), ("duration", Json::from(c.duration)), ("tracks", Json::from(c.tracks.len()))])))),
            ("warnings", Json::arr(self.warnings.iter().take(32).map(|w| Json::obj([("code", Json::Str(w.code.into())), ("path", short(&w.path, 128)), ("message", short(&w.message, 256))])))),
            ("warning_count", Json::from(self.warnings.len())),
        ])
    }
}

/// At most `max` characters of `s` (an ellipsis marks a cut), so reports stay bounded.
fn short(s: &str, max: usize) -> Json {
    match s.char_indices().nth(max) {
        None => Json::Str(s.to_string()),
        Some((at, _)) => Json::Str(format!("{}…", &s[..at])),
    }
}

/// Imports a `.gltf` or `.glb` file; external files resolve inside its folder.
pub fn import_file(path: &Path, opts: &ImportOptions) -> Result<Import, GltfError> {
    let meta = std::fs::metadata(path).map_err(|e| GltfError::new(GltfErrorCode::Io, "", format!("reading '{}': {e}", path.display())))?;
    if meta.len() > opts.limits.max_file_bytes {
        return Err(GltfError::new(GltfErrorCode::Limit, "", format!("'{}' is {} bytes; the limit is {}", path.display(), meta.len(), opts.limits.max_file_bytes)).with_numbers(meta.len() as f64, opts.limits.max_file_bytes as f64));
    }
    let bytes = std::fs::read(path).map_err(|e| GltfError::new(GltfErrorCode::Io, "", format!("reading '{}': {e}", path.display())))?;
    let base = path.parent().map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p });
    let mut opts = opts.clone();
    if opts.name.is_none() {
        opts.name = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    }
    import_bytes(&bytes, base, &opts)
}

/// Imports a `.glb` or `.gltf` held in memory. `base` is the folder external URIs resolve in
/// (`None`: only GLB chunks and data URIs are allowed).
pub fn import_bytes(bytes: &[u8], base: Option<&Path>, opts: &ImportOptions) -> Result<Import, GltfError> {
    let mut warnings = Vec::new();
    let loaded = doc::load(bytes, base, &opts.limits, &mut warnings)?;
    import::convert(&loaded, opts, warnings)
}

/// Parses and validates a file's structure without converting it (for inspection).
pub fn load_document(bytes: &[u8], base: Option<&Path>, limits: &Limits) -> Result<(Document, Vec<Warning>), GltfError> {
    let mut warnings = Vec::new();
    let loaded = doc::load(bytes, base, limits, &mut warnings)?;
    Ok((loaded.doc, warnings))
}

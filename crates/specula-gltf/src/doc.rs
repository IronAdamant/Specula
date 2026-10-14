//! The glTF document: container parsing (`.gltf` JSON or `.glb`), strict reading of the JSON
//! into typed objects with every reference checked, and buffer/URI loading with path
//! containment.

use std::path::{Path, PathBuf};

use specula_core::{base64, Json};

use crate::error::{gerr, push_warning, GltfError, GltfErrorCode as C, Warning};
use crate::Limits;

/// GLB magic `glTF`, chunk types `JSON` and `BIN\0`.
pub(crate) const GLB_MAGIC: u32 = 0x4654_6C67;
pub(crate) const CHUNK_JSON: u32 = 0x4E4F_534A;
pub(crate) const CHUNK_BIN: u32 = 0x004E_4942;

/// Extensions the importer understands well enough to be *required*.
pub const SUPPORTED_REQUIRED: [&str; 2] = ["KHR_materials_emissive_strength", "KHR_mesh_quantization"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentType {
    I8,
    U8,
    I16,
    U16,
    U32,
    F32,
}

impl ComponentType {
    pub fn from_code(c: u64) -> Option<ComponentType> {
        Some(match c {
            5120 => ComponentType::I8,
            5121 => ComponentType::U8,
            5122 => ComponentType::I16,
            5123 => ComponentType::U16,
            5125 => ComponentType::U32,
            5126 => ComponentType::F32,
            _ => return None,
        })
    }

    pub fn code(self) -> u32 {
        match self {
            ComponentType::I8 => 5120,
            ComponentType::U8 => 5121,
            ComponentType::I16 => 5122,
            ComponentType::U16 => 5123,
            ComponentType::U32 => 5125,
            ComponentType::F32 => 5126,
        }
    }

    pub fn size(self) -> usize {
        match self {
            ComponentType::I8 | ComponentType::U8 => 1,
            ComponentType::I16 | ComponentType::U16 => 2,
            ComponentType::U32 | ComponentType::F32 => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ComponentType::I8 => "BYTE",
            ComponentType::U8 => "UNSIGNED_BYTE",
            ComponentType::I16 => "SHORT",
            ComponentType::U16 => "UNSIGNED_SHORT",
            ComponentType::U32 => "UNSIGNED_INT",
            ComponentType::F32 => "FLOAT",
        }
    }
}

/// Element shape: `SCALAR`, `VEC2..4`, `MAT2..4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemType {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
    Mat2,
    Mat3,
    Mat4,
}

impl ElemType {
    fn parse(s: &str) -> Option<ElemType> {
        Some(match s {
            "SCALAR" => ElemType::Scalar,
            "VEC2" => ElemType::Vec2,
            "VEC3" => ElemType::Vec3,
            "VEC4" => ElemType::Vec4,
            "MAT2" => ElemType::Mat2,
            "MAT3" => ElemType::Mat3,
            "MAT4" => ElemType::Mat4,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            ElemType::Scalar => "SCALAR",
            ElemType::Vec2 => "VEC2",
            ElemType::Vec3 => "VEC3",
            ElemType::Vec4 => "VEC4",
            ElemType::Mat2 => "MAT2",
            ElemType::Mat3 => "MAT3",
            ElemType::Mat4 => "MAT4",
        }
    }

    /// Components per element.
    pub fn dims(self) -> usize {
        match self {
            ElemType::Scalar => 1,
            ElemType::Vec2 => 2,
            ElemType::Vec3 => 3,
            ElemType::Vec4 | ElemType::Mat2 => 4,
            ElemType::Mat3 => 9,
            ElemType::Mat4 => 16,
        }
    }

    /// Matrix columns (0 for vectors).
    pub fn columns(self) -> usize {
        match self {
            ElemType::Mat2 => 2,
            ElemType::Mat3 => 3,
            ElemType::Mat4 => 4,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {
    pub byte_length: usize,
    pub uri: Option<String>,
}

#[derive(Clone, Debug)]
pub struct View {
    pub buffer: usize,
    pub offset: usize,
    pub length: usize,
    pub stride: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Sparse {
    pub count: usize,
    pub idx_view: usize,
    pub idx_offset: usize,
    pub idx_type: ComponentType,
    pub val_view: usize,
    pub val_offset: usize,
}

#[derive(Clone, Debug)]
pub struct Accessor {
    pub view: Option<usize>,
    pub offset: usize,
    pub ctype: ComponentType,
    pub normalized: bool,
    pub count: usize,
    pub ty: ElemType,
    pub sparse: Option<Sparse>,
}

#[derive(Clone, Debug)]
pub struct Primitive {
    /// `(semantic, accessor)` in document order.
    pub attributes: Vec<(String, usize)>,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    pub mode: u32,
    pub has_targets: bool,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub name: Option<String>,
    pub primitives: Vec<Primitive>,
}

/// A node's local transform as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Local {
    Trs { t: [f32; 3], r: [f32; 4], s: [f32; 3] },
    /// Column-major 4×4.
    Matrix([f32; 16]),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Option<String>,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub local: Local,
    /// `extras.specula.tail` (written by our exporter for rig bones).
    pub tail: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub name: Option<String>,
    pub nodes: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexRef {
    pub texture: usize,
    pub tex_coord: u32,
    /// `scale` (normal maps) or `strength` (occlusion); 1 otherwise.
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: Option<String>,
    pub base_color: [f32; 4],
    pub base_color_texture: Option<TexRef>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_texture: Option<TexRef>,
    pub normal_texture: Option<TexRef>,
    pub occlusion_texture: Option<TexRef>,
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub emissive_texture: Option<TexRef>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// `extras.specula` (a Specula material preset written by our exporter).
    pub specula: Option<Json>,
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub name: Option<String>,
    pub source: Option<usize>,
    pub sampler: Option<usize>,
}

/// Sampler filters and wraps (GL enums; `None` = unspecified).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    pub mag_filter: Option<u32>,
    pub min_filter: Option<u32>,
    pub wrap_s: u32,
    pub wrap_t: u32,
}

impl Default for Sampler {
    fn default() -> Self {
        Sampler { mag_filter: None, min_filter: None, wrap_s: 10497, wrap_t: 10497 }
    }
}

#[derive(Clone, Debug)]
pub struct ImageDef {
    pub name: Option<String>,
    pub uri: Option<String>,
    pub view: Option<usize>,
    pub mime: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub name: Option<String>,
    pub joints: Vec<usize>,
    pub inverse_bind: Option<usize>,
    pub skeleton: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path3 {
    Translation,
    Rotation,
    Scale,
    Weights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

#[derive(Clone, Debug)]
pub struct AnimChannel {
    pub sampler: usize,
    /// `None` when the target has no node (extensions such as KHR_animation_pointer).
    pub node: Option<usize>,
    pub path: Option<Path3>,
    pub raw_path: String,
}

#[derive(Clone, Debug)]
pub struct AnimSampler {
    pub input: usize,
    pub output: usize,
    pub interpolation: Interpolation,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: Option<String>,
    pub channels: Vec<AnimChannel>,
    pub samplers: Vec<AnimSampler>,
    pub specula: Option<Json>,
}

/// A parsed, reference-checked glTF document.
#[derive(Clone, Debug, Default)]
pub struct Document {
    pub version: String,
    pub generator: Option<String>,
    pub extensions_used: Vec<String>,
    pub extensions_required: Vec<String>,
    pub buffers: Vec<Buffer>,
    pub views: Vec<View>,
    pub accessors: Vec<Accessor>,
    pub meshes: Vec<Mesh>,
    pub nodes: Vec<Node>,
    pub scenes: Vec<Scene>,
    pub scene: Option<usize>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub samplers: Vec<Sampler>,
    pub images: Vec<ImageDef>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
}

/// The document plus the bytes of every buffer (`buffers[i].len() == byte_length`).
pub struct Loaded {
    pub doc: Document,
    pub buffers: Vec<Vec<u8>>,
    pub base: Option<PathBuf>,
    pub is_glb: bool,
    /// Bytes read so far (buffers; images add theirs), against `Limits::max_total_bytes`.
    pub budget: ByteBudget,
}

/// A running count of bytes read from URIs and chunks, checked before each read.
#[derive(Clone, Copy, Debug)]
pub struct ByteBudget {
    pub used: u64,
    pub limit: u64,
}

impl ByteBudget {
    /// Charges `n` bytes, or fails with a `Limit` error at `path`.
    pub fn charge(&mut self, n: u64, path: &str) -> Result<(), GltfError> {
        let used = self.used.saturating_add(n);
        if used > self.limit {
            return Err(gerr(C::Limit, path, format!("{used} bytes of buffers and images; the limit is {}", self.limit)).with_numbers(used as f64, self.limit as f64));
        }
        self.used = used;
        Ok(())
    }
}

/// Where a URI's bytes are, before anything is read.
pub(crate) enum UriSource<'a> {
    /// The base64 payload of a `data:` URI and its offset in the URI.
    Data { payload: &'a str, offset: usize },
    /// A canonical path inside the asset folder, and the file's size.
    File { real: PathBuf, len: u64 },
}

// ---------------------------------------------------------------------------------------------
// JSON reading helpers. glTF permits unknown properties (extensions, extras, future fields), so
// they are ignored; types, ranges and references are strict.

fn wrong(path: &str, what: &str) -> GltfError {
    gerr(C::WrongType, path, format!("expected {what}"))
}

fn field(path: &str, k: &str) -> String {
    if path.is_empty() {
        k.to_string()
    } else {
        format!("{path}.{k}")
    }
}

fn obj<'a>(v: &'a Json, path: &str) -> Result<&'a Json, GltfError> {
    match v {
        Json::Obj(_) => Ok(v),
        _ => Err(wrong(path, "an object")),
    }
}

fn arr<'a>(o: &'a Json, k: &str, path: &str, max: usize) -> Result<&'a [Json], GltfError> {
    match o.get(k) {
        None => Ok(&[]),
        Some(Json::Arr(a)) => {
            if a.len() > max {
                return Err(gerr(C::Limit, &field(path, k), format!("{} entries; at most {max}", a.len())).with_numbers(a.len() as f64, max as f64));
            }
            Ok(a)
        }
        Some(_) => Err(wrong(&field(path, k), "an array")),
    }
}

fn uint(o: &Json, k: &str, path: &str) -> Result<Option<u64>, GltfError> {
    match o.get(k) {
        None => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| wrong(&field(path, k), "a non-negative integer")),
    }
}

fn req_uint(o: &Json, k: &str, path: &str) -> Result<u64, GltfError> {
    uint(o, k, path)?.ok_or_else(|| gerr(C::MissingField, &field(path, k), "required"))
}

fn usize_of(v: u64, path: &str) -> Result<usize, GltfError> {
    usize::try_from(v).map_err(|_| gerr(C::OutOfRange, path, format!("{v} is too large")))
}

/// An index property that must refer to one of `len` objects named `what`.
fn idx(o: &Json, k: &str, path: &str, len: usize, what: &str) -> Result<Option<usize>, GltfError> {
    match uint(o, k, path)? {
        None => Ok(None),
        Some(i) if (i as u128) < len as u128 => Ok(Some(i as usize)),
        Some(i) => Err(gerr(C::BadReference, &field(path, k), format!("{what} {i} doesn't exist ({len} defined)"))),
    }
}

fn req_idx(o: &Json, k: &str, path: &str, len: usize, what: &str) -> Result<usize, GltfError> {
    idx(o, k, path, len, what)?.ok_or_else(|| gerr(C::MissingField, &field(path, k), "required"))
}

fn index_value(v: &Json, path: &str, len: usize, what: &str) -> Result<usize, GltfError> {
    match v.as_u64() {
        Some(i) if (i as u128) < len as u128 => Ok(i as usize),
        Some(i) => Err(gerr(C::BadReference, path, format!("{what} {i} doesn't exist ({len} defined)"))),
        None => Err(wrong(path, "an index")),
    }
}

fn string<'a>(o: &'a Json, k: &str, path: &str) -> Result<Option<&'a str>, GltfError> {
    match o.get(k) {
        None => Ok(None),
        Some(Json::Str(s)) => Ok(Some(s)),
        Some(_) => Err(wrong(&field(path, k), "a string")),
    }
}

fn boolean(o: &Json, k: &str, path: &str, default: bool) -> Result<bool, GltfError> {
    match o.get(k) {
        None => Ok(default),
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(wrong(&field(path, k), "a boolean")),
    }
}

fn number(o: &Json, k: &str, path: &str, default: f32, lo: f32, hi: f32) -> Result<f32, GltfError> {
    match o.get(k) {
        None => Ok(default),
        Some(v) => {
            let p = field(path, k);
            let x = v.as_f64().ok_or_else(|| wrong(&p, "a number"))? as f32;
            if !x.is_finite() || x < lo || x > hi {
                return Err(gerr(C::OutOfRange, &p, format!("{x} is outside {lo}..={hi}")));
            }
            Ok(x)
        }
    }
}

fn vecn<const N: usize>(o: &Json, k: &str, path: &str, default: [f32; N]) -> Result<[f32; N], GltfError> {
    match o.get(k) {
        None => Ok(default),
        Some(v) => {
            let p = field(path, k);
            let a = v.as_array().filter(|a| a.len() == N).ok_or_else(|| wrong(&p, &format!("an array of {N} numbers")))?;
            let mut out = [0f32; N];
            for (i, (o, x)) in out.iter_mut().zip(a).enumerate() {
                let f = x.as_f64().ok_or_else(|| wrong(&format!("{p}[{i}]"), "a number"))? as f32;
                if !f.is_finite() {
                    return Err(gerr(C::InvalidData, &format!("{p}[{i}]"), "not a finite f32"));
                }
                *o = f;
            }
            Ok(out)
        }
    }
}

fn name(o: &Json, path: &str) -> Result<Option<String>, GltfError> {
    Ok(string(o, "name", path)?.map(str::to_string))
}

/// `extras.specula` of an object, if it's an object.
fn specula_extras(o: &Json) -> Option<Json> {
    o.get("extras").and_then(|e| e.get("specula")).filter(|s| s.as_object().is_some()).cloned()
}

// ---------------------------------------------------------------------------------------------

/// Splits a GLB into its JSON text and optional BIN chunk.
pub(crate) fn split_glb(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), GltfError> {
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (Some(magic), Some(version), Some(length)) = (u32_at(0), u32_at(4), u32_at(8)) else {
        return Err(gerr(C::Container, "", "GLB header is shorter than 12 bytes").at_offset(0));
    };
    if magic != GLB_MAGIC {
        return Err(gerr(C::Container, "", "missing GLB magic 'glTF'").at_offset(0));
    }
    if version != 2 {
        return Err(gerr(C::Unsupported, "", format!("GLB version {version} (only 2 is supported)")).at_offset(4));
    }
    if length as usize != bytes.len() {
        return Err(gerr(C::Container, "", format!("GLB header says {length} bytes, the data is {}", bytes.len())).at_offset(8));
    }
    let mut pos = 12usize;
    let mut json: Option<&[u8]> = None;
    let mut bin: Option<&[u8]> = None;
    let mut index = 0;
    while pos < bytes.len() {
        let (Some(len), Some(kind)) = (u32_at(pos), u32_at(pos + 4)) else {
            return Err(gerr(C::Container, "", "truncated chunk header").at_offset(pos as u64));
        };
        let start = pos + 8;
        let Some(data) = start.checked_add(len as usize).and_then(|end| bytes.get(start..end)) else {
            return Err(gerr(C::Bounds, "", format!("chunk {index} of {len} bytes runs past the end of the file")).at_offset(pos as u64));
        };
        match (index, kind) {
            (0, CHUNK_JSON) => json = Some(data),
            (0, _) => return Err(gerr(C::Container, "", format!("first chunk has type {kind:#010x}, expected JSON")).at_offset(pos as u64 + 4)),
            (1, CHUNK_BIN) => bin = Some(data),
            (_, CHUNK_JSON) => return Err(gerr(C::Container, "", "a second JSON chunk").at_offset(pos as u64 + 4)),
            (_, CHUNK_BIN) => return Err(gerr(C::Container, "", "the BIN chunk must be the second chunk").at_offset(pos as u64 + 4)),
            _ => {} // unknown chunk types are skipped
        }
        // Chunks are 4-byte aligned.
        pos = start + (len as usize).div_ceil(4) * 4;
        index += 1;
    }
    let json = json.ok_or_else(|| gerr(C::Container, "", "GLB has no JSON chunk").at_offset(12))?;
    Ok((json, bin))
}

/// Parses `bytes` (a `.glb` or `.gltf` file) and loads every buffer. External URIs resolve
/// against `base` (the file's folder) and must stay inside it.
pub fn load(bytes: &[u8], base: Option<&Path>, limits: &Limits, warnings: &mut Vec<Warning>) -> Result<Loaded, GltfError> {
    if bytes.len() as u64 > limits.max_file_bytes {
        return Err(gerr(C::Limit, "", format!("file is {} bytes; the limit is {}", bytes.len(), limits.max_file_bytes)).with_numbers(bytes.len() as f64, limits.max_file_bytes as f64));
    }
    let is_glb = bytes.starts_with(b"glTF");
    let (json_bytes, bin) = if is_glb { split_glb(bytes)? } else { (bytes, None) };
    if json_bytes.len() as u64 > limits.max_json_bytes {
        return Err(gerr(C::Limit, "", format!("JSON is {} bytes; the limit is {}", json_bytes.len(), limits.max_json_bytes)).with_numbers(json_bytes.len() as f64, limits.max_json_bytes as f64));
    }
    let json_start = if is_glb { 20 } else { 0 };
    let text = std::str::from_utf8(json_bytes).map_err(|e| gerr(C::Json, "", format!("JSON is not UTF-8: {e}")).at_offset((json_start + e.valid_up_to()) as u64))?;
    let text = match text.strip_prefix('\u{feff}') {
        Some(t) => {
            push_warning(warnings, Warning { code: "byte_order_mark", path: String::new(), message: "the JSON starts with a byte-order mark (not allowed by glTF; ignored)".into() });
            t
        }
        None => text,
    };
    // GLB JSON chunks may be padded with spaces (already whitespace) or, wrongly, NULs.
    let text = text.trim_end_matches('\0');
    let json = Json::parse(text).map_err(|e| gerr(C::Json, "", e.message).at_offset((json_start + e.offset) as u64))?;
    let doc = parse(&json, limits, warnings)?;
    let base = base.map(Path::to_path_buf);
    let declared: u64 = doc.buffers.iter().map(|b| b.byte_length as u64).sum();
    if declared > limits.max_total_bytes {
        return Err(gerr(C::Limit, "buffers", format!("buffers declare {declared} bytes; the limit is {}", limits.max_total_bytes)).with_numbers(declared as f64, limits.max_total_bytes as f64));
    }
    let mut budget = ByteBudget { used: 0, limit: limits.max_total_bytes };
    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(doc.buffers.len());
    // External files already read (canonical path → buffer index): each is read once.
    let mut files: Vec<(PathBuf, usize)> = Vec::new();
    for (i, b) in doc.buffers.iter().enumerate() {
        let path = format!("buffers[{i}]");
        let want = b.byte_length;
        let short = |have: usize| gerr(C::Bounds, &format!("{path}.byteLength"), format!("{want} bytes declared, {have} available")).with_numbers(have as f64, want as f64);
        budget.charge(want as u64, &path)?;
        let data = match (&b.uri, i, bin) {
            (Some(uri), _, _) => {
                let up = format!("{path}.uri");
                match resolve_uri(uri, base.as_deref(), &up, limits)? {
                    UriSource::Data { payload, offset } => {
                        let mut d = decode_data(payload, offset, &up)?;
                        if d.len() < want {
                            return Err(short(d.len()));
                        }
                        d.truncate(want);
                        d.shrink_to_fit();
                        d
                    }
                    UriSource::File { real, len } => {
                        if len < want as u64 {
                            return Err(short(len as usize));
                        }
                        match files.iter().find(|(p, k)| *p == real && buffers[*k].len() >= want) {
                            Some(&(_, k)) => buffers[k][..want].to_vec(),
                            None => {
                                let d = read_file(&real, Some(want as u64), &up)?;
                                files.push((real, i));
                                d
                            }
                        }
                    }
                }
            }
            (None, 0, Some(bin)) if is_glb => {
                if bin.len() > want + 3 {
                    push_warning(warnings, Warning { code: "bin_chunk_padding", path: path.clone(), message: format!("BIN chunk is {} bytes for a {want}-byte buffer", bin.len()) });
                }
                if bin.len() < want {
                    return Err(short(bin.len()));
                }
                bin[..want].to_vec()
            }
            (None, _, _) => return Err(gerr(C::MissingField, &format!("{path}.uri"), "only the first buffer of a GLB may omit its uri")),
        };
        buffers.push(data);
    }
    // Every view must lie inside its buffer.
    for (i, v) in doc.views.iter().enumerate() {
        let end = v.offset.checked_add(v.length);
        if end.is_none_or(|e| e > buffers[v.buffer].len()) {
            return Err(gerr(C::Bounds, &format!("bufferViews[{i}]"), format!("bytes {}..+{} exceed buffer {} ({} bytes)", v.offset, v.length, v.buffer, buffers[v.buffer].len())).at_offset(v.offset as u64));
        }
    }
    Ok(Loaded { doc, buffers, base, is_glb, budget })
}

/// Resolves a buffer or image URI without reading it: a base64 `data:` URI, or a relative path
/// that must stay inside `base` (links followed).
///
/// Containment is checked on the canonical path, and that canonical path is what gets read, so
/// a link swapped *between* this check and the read (a time-of-check/time-of-use race by
/// someone who can write inside the asset folder while the import runs) could still redirect
/// the read. Importing from a folder an attacker can modify concurrently is out of scope.
pub(crate) fn resolve_uri<'a>(uri: &'a str, base: Option<&Path>, path: &str, limits: &Limits) -> Result<UriSource<'a>, GltfError> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let comma = rest.find(',').ok_or_else(|| gerr(C::Uri, path, "data URI without a comma"))?;
        if !rest[..comma].ends_with(";base64") {
            return Err(gerr(C::Unsupported, path, "only base64 data URIs are supported"));
        }
        let payload = &rest[comma + 1..];
        let size = payload.len() as u64 / 4 * 3;
        if size > limits.max_file_bytes {
            return Err(gerr(C::Limit, path, format!("data URI holds about {size} bytes; the limit is {}", limits.max_file_bytes)).with_numbers(size as f64, limits.max_file_bytes as f64));
        }
        return Ok(UriSource::Data { payload, offset: 5 + comma + 1 });
    }
    let rel = relative_path(uri, path)?;
    let Some(base) = base else {
        return Err(gerr(C::Uri, path, format!("'{uri}' is an external file, but no base folder was given (load from a file path)")));
    };
    let full = base.join(&rel);
    let io = |e: std::io::Error, what: &str| gerr(C::Io, path, format!("{what} '{}': {e}", full.display()));
    let base_real = base.canonicalize().map_err(|e| io(e, "resolving the asset folder for"))?;
    let real = full.canonicalize().map_err(|e| io(e, "resolving"))?;
    if !real.starts_with(&base_real) {
        return Err(gerr(C::Uri, path, format!("'{uri}' resolves outside the asset folder (links may not leave it)")));
    }
    let meta = std::fs::metadata(&real).map_err(|e| io(e, "reading"))?;
    if !meta.is_file() {
        return Err(gerr(C::Uri, path, format!("'{uri}' is not a file")));
    }
    if meta.len() > limits.max_file_bytes {
        return Err(gerr(C::Limit, path, format!("'{uri}' is {} bytes; the limit is {}", meta.len(), limits.max_file_bytes)).with_numbers(meta.len() as f64, limits.max_file_bytes as f64));
    }
    Ok(UriSource::File { real, len: meta.len() })
}

/// Decodes a data URI's base64 payload (`offset` = its position in the URI, for errors).
pub(crate) fn decode_data(payload: &str, offset: usize, path: &str) -> Result<Vec<u8>, GltfError> {
    base64::decode(payload).map_err(|e| gerr(C::Uri, path, format!("bad base64: {}", e.message)).at_offset((offset + e.offset) as u64))
}

/// Reads at most `want` bytes (all of it when `None`) of a resolved file.
pub(crate) fn read_file(real: &Path, want: Option<u64>, path: &str) -> Result<Vec<u8>, GltfError> {
    use std::io::Read;
    let io = |e: std::io::Error| gerr(C::Io, path, format!("reading '{}': {e}", real.display()));
    let f = std::fs::File::open(real).map_err(io)?;
    let mut out = Vec::with_capacity(want.unwrap_or(0).min(1 << 30) as usize);
    match want {
        Some(n) => f.take(n).read_to_end(&mut out),
        None => (&f).read_to_end(&mut out),
    }
    .map_err(io)?;
    if want.is_some_and(|n| (out.len() as u64) < n) {
        return Err(gerr(C::Bounds, path, format!("'{}' shrank while reading: {} of {} bytes", real.display(), out.len(), want.unwrap_or(0))));
    }
    Ok(out)
}

/// A URI as a safe relative path: percent-decoded, `/`-separated, no scheme, no absolute
/// path, no `..`, no backslashes, colons or NULs.
pub(crate) fn relative_path(uri: &str, path: &str) -> Result<PathBuf, GltfError> {
    let bad = |why: &str| gerr(C::Uri, path, format!("'{uri}': {why}"));
    if uri.is_empty() {
        return Err(bad("empty URI"));
    }
    // A scheme ("http:", "file:", or a drive letter "C:") before the first '/'.
    if uri.split('/').next().is_some_and(|first| first.contains(':')) {
        return Err(bad("absolute or remote URIs are not allowed (relative paths and data URIs only)"));
    }
    let mut bytes = Vec::with_capacity(uri.len());
    let raw = uri.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let hex = raw.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
            bytes.push(hex.ok_or_else(|| bad("bad percent-escape"))?);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(bytes).map_err(|_| bad("not UTF-8 after percent-decoding"))?;
    if decoded.starts_with('/') || decoded.contains('\\') || decoded.contains('\0') || decoded.contains(':') {
        return Err(bad("absolute paths, backslashes, colons and NULs are not allowed"));
    }
    let mut out = PathBuf::new();
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(bad("'..' may not leave the asset folder")),
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(bad("names no file"));
    }
    Ok(out)
}

/// Reads the root JSON into a [`Document`], checking types, ranges and references.
pub fn parse(json: &Json, limits: &Limits, warnings: &mut Vec<Warning>) -> Result<Document, GltfError> {
    obj(json, "")?;
    let max = limits.max_items;
    let mut d = Document::default();
    let asset = json.get("asset").ok_or_else(|| gerr(C::MissingField, "asset", "required"))?;
    obj(asset, "asset")?;
    d.version = string(asset, "version", "asset")?.ok_or_else(|| gerr(C::MissingField, "asset.version", "required"))?.to_string();
    let major = d.version.split('.').next().unwrap_or("");
    if major != "2" {
        let min = string(asset, "minVersion", "asset")?;
        return Err(gerr(C::Unsupported, "asset.version", format!("glTF version {} (minVersion {:?}); only 2.x is supported", d.version, min)));
    }
    d.generator = string(asset, "generator", "asset")?.map(str::to_string);
    for (k, list) in [("extensionsUsed", &mut d.extensions_used), ("extensionsRequired", &mut d.extensions_required)] {
        for (i, e) in arr(json, k, "", 256)?.iter().enumerate() {
            list.push(e.as_str().ok_or_else(|| wrong(&format!("{k}[{i}]"), "a string"))?.to_string());
        }
    }
    for (i, e) in d.extensions_required.iter().enumerate() {
        if !SUPPORTED_REQUIRED.contains(&e.as_str()) {
            return Err(gerr(C::Unsupported, &format!("extensionsRequired[{i}]"), format!("required extension {e} is not supported (supported: {})", SUPPORTED_REQUIRED.join(", "))));
        }
    }
    for e in &d.extensions_used {
        if !SUPPORTED_REQUIRED.contains(&e.as_str()) && !d.extensions_required.contains(e) {
            push_warning(warnings, Warning { code: "ignored_extension", path: "extensionsUsed".into(), message: format!("{e} is not supported; its data is ignored") });
        }
    }

    // Buffers and views.
    for (i, b) in arr(json, "buffers", "", max)?.iter().enumerate() {
        let p = format!("buffers[{i}]");
        obj(b, &p)?;
        let byte_length = usize_of(req_uint(b, "byteLength", &p)?, &p)?;
        d.buffers.push(Buffer { byte_length, uri: string(b, "uri", &p)?.map(str::to_string) });
    }
    for (i, v) in arr(json, "bufferViews", "", max)?.iter().enumerate() {
        let p = format!("bufferViews[{i}]");
        obj(v, &p)?;
        let buffer = req_idx(v, "buffer", &p, d.buffers.len(), "buffer")?;
        let offset = usize_of(uint(v, "byteOffset", &p)?.unwrap_or(0), &p)?;
        let length = usize_of(req_uint(v, "byteLength", &p)?, &p)?;
        if length == 0 {
            return Err(gerr(C::OutOfRange, &field(&p, "byteLength"), "must be at least 1"));
        }
        let stride = match uint(v, "byteStride", &p)? {
            None => None,
            Some(s) if (4..=252).contains(&s) && s % 4 == 0 => Some(s as usize),
            Some(s) => return Err(gerr(C::OutOfRange, &field(&p, "byteStride"), format!("{s} (must be a multiple of 4 in 4..=252)"))),
        };
        d.views.push(View { buffer, offset, length, stride });
    }
    for (i, a) in arr(json, "accessors", "", max)?.iter().enumerate() {
        d.accessors.push(parse_accessor(a, &format!("accessors[{i}]"), &d, limits)?);
    }

    // Images, samplers, textures, materials.
    for (i, im) in arr(json, "images", "", max)?.iter().enumerate() {
        let p = format!("images[{i}]");
        obj(im, &p)?;
        let uri = string(im, "uri", &p)?.map(str::to_string);
        let view = idx(im, "bufferView", &p, d.views.len(), "bufferView")?;
        if uri.is_some() == view.is_some() {
            return Err(gerr(C::MissingField, &p, "an image has exactly one of uri or bufferView"));
        }
        let mime = string(im, "mimeType", &p)?.map(str::to_string);
        if view.is_some() && mime.is_none() {
            return Err(gerr(C::MissingField, &field(&p, "mimeType"), "required with bufferView"));
        }
        d.images.push(ImageDef { name: name(im, &p)?, uri, view, mime });
    }
    for (i, s) in arr(json, "samplers", "", max)?.iter().enumerate() {
        let p = format!("samplers[{i}]");
        obj(s, &p)?;
        let pick = |k: &str, ok: &[u64]| -> Result<Option<u32>, GltfError> {
            match uint(s, k, &p)? {
                None => Ok(None),
                Some(v) if ok.contains(&v) => Ok(Some(v as u32)),
                Some(v) => Err(gerr(C::OutOfRange, &field(&p, k), format!("{v} is not a valid value ({ok:?})"))),
            }
        };
        let wraps = [33071, 33648, 10497];
        d.samplers.push(Sampler {
            mag_filter: pick("magFilter", &[9728, 9729])?,
            min_filter: pick("minFilter", &[9728, 9729, 9984, 9985, 9986, 9987])?,
            wrap_s: pick("wrapS", &wraps)?.unwrap_or(10497),
            wrap_t: pick("wrapT", &wraps)?.unwrap_or(10497),
        });
    }
    for (i, t) in arr(json, "textures", "", max)?.iter().enumerate() {
        let p = format!("textures[{i}]");
        obj(t, &p)?;
        let source = idx(t, "source", &p, d.images.len(), "image")?;
        if source.is_none() && t.get("extensions").is_some() {
            push_warning(warnings, Warning { code: "texture_without_source", path: p.clone(), message: "texture images only through extensions (e.g. KTX2/WebP) are not supported; ignored".into() });
        }
        d.textures.push(Texture { name: name(t, &p)?, source, sampler: idx(t, "sampler", &p, d.samplers.len(), "sampler")? });
    }
    for (i, m) in arr(json, "materials", "", max)?.iter().enumerate() {
        d.materials.push(parse_material(m, &format!("materials[{i}]"), d.textures.len(), warnings)?);
    }

    // Meshes, skins, nodes, scenes, animations.
    for (i, m) in arr(json, "meshes", "", max)?.iter().enumerate() {
        let p = format!("meshes[{i}]");
        obj(m, &p)?;
        let prims = arr(m, "primitives", &p, max)?;
        if prims.is_empty() {
            return Err(gerr(C::MissingField, &field(&p, "primitives"), "a mesh needs at least one primitive"));
        }
        let mut primitives = Vec::with_capacity(prims.len());
        for (j, pr) in prims.iter().enumerate() {
            let pp = format!("{p}.primitives[{j}]");
            obj(pr, &pp)?;
            let attrs = pr.get("attributes").ok_or_else(|| gerr(C::MissingField, &field(&pp, "attributes"), "required"))?;
            let ap = field(&pp, "attributes");
            let pairs = attrs.as_object().ok_or_else(|| wrong(&ap, "an object"))?;
            let mut attributes = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                attributes.push((k.clone(), index_value(v, &field(&ap, k), d.accessors.len(), "accessor")?));
            }
            let mode = match uint(pr, "mode", &pp)? {
                None => 4,
                Some(m) if m <= 6 => m as u32,
                Some(m) => return Err(gerr(C::OutOfRange, &field(&pp, "mode"), format!("{m} (0..=6)"))),
            };
            primitives.push(Primitive {
                attributes,
                indices: idx(pr, "indices", &pp, d.accessors.len(), "accessor")?,
                material: idx(pr, "material", &pp, d.materials.len(), "material")?,
                mode,
                has_targets: pr.get("targets").and_then(Json::as_array).is_some_and(|t| !t.is_empty()),
            });
        }
        d.meshes.push(Mesh { name: name(m, &p)?, primitives });
    }
    let nodes = arr(json, "nodes", "", max)?;
    for (i, s) in arr(json, "skins", "", max)?.iter().enumerate() {
        let p = format!("skins[{i}]");
        obj(s, &p)?;
        let mut joints = Vec::new();
        for (j, v) in arr(s, "joints", &p, max)?.iter().enumerate() {
            joints.push(index_value(v, &format!("{p}.joints[{j}]"), nodes.len(), "node")?);
        }
        if joints.is_empty() {
            return Err(gerr(C::MissingField, &field(&p, "joints"), "a skin needs at least one joint"));
        }
        d.skins.push(Skin {
            name: name(s, &p)?,
            joints,
            inverse_bind: idx(s, "inverseBindMatrices", &p, d.accessors.len(), "accessor")?,
            skeleton: idx(s, "skeleton", &p, nodes.len(), "node")?,
        });
    }
    for (i, n) in nodes.iter().enumerate() {
        let p = format!("nodes[{i}]");
        obj(n, &p)?;
        let mut children = Vec::new();
        for (j, c) in arr(n, "children", &p, max)?.iter().enumerate() {
            children.push(index_value(c, &format!("{p}.children[{j}]"), nodes.len(), "node")?);
        }
        let local = if n.get("matrix").is_some() {
            if ["translation", "rotation", "scale"].iter().any(|k| n.get(k).is_some()) {
                return Err(gerr(C::InvalidData, &p, "a node has either matrix or translation/rotation/scale, not both"));
            }
            let m: [f32; 16] = vecn(n, "matrix", &p, [0.0; 16])?;
            if m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 || m[15] != 1.0 {
                return Err(gerr(C::InvalidData, &field(&p, "matrix"), "the last row must be 0, 0, 0, 1 (an affine transform)"));
            }
            Local::Matrix(m)
        } else {
            let r: [f32; 4] = vecn(n, "rotation", &p, [0.0, 0.0, 0.0, 1.0])?;
            let l2 = r.iter().map(|x| x * x).sum::<f32>();
            if l2.is_nan() || l2 <= 1e-12 {
                return Err(gerr(C::InvalidData, &field(&p, "rotation"), "a zero quaternion is not a rotation"));
            }
            let r = if (l2 - 1.0).abs() > 1e-6 {
                if (l2 - 1.0).abs() > 1e-2 {
                    push_warning(warnings, Warning { code: "rotation_not_unit", path: field(&p, "rotation"), message: format!("|q|^2 = {l2}; normalised") });
                }
                let k = 1.0 / l2.sqrt();
                r.map(|x| x * k)
            } else {
                r
            };
            Local::Trs { t: vecn(n, "translation", &p, [0.0; 3])?, r, s: vecn(n, "scale", &p, [1.0; 3])? }
        };
        let tail = match n.get("extras").and_then(|e| e.get("specula")) {
            Some(sp) if sp.get("tail").is_some() => Some(vecn(sp, "tail", &format!("{p}.extras.specula"), [0.0; 3])?),
            _ => None,
        };
        d.nodes.push(Node {
            name: name(n, &p)?,
            children,
            mesh: idx(n, "mesh", &p, d.meshes.len(), "mesh")?,
            skin: idx(n, "skin", &p, d.skins.len(), "skin")?,
            local,
            tail,
        });
    }
    for (i, s) in arr(json, "scenes", "", max)?.iter().enumerate() {
        let p = format!("scenes[{i}]");
        obj(s, &p)?;
        let mut list = Vec::new();
        for (j, v) in arr(s, "nodes", &p, max)?.iter().enumerate() {
            list.push(index_value(v, &format!("{p}.nodes[{j}]"), d.nodes.len(), "node")?);
        }
        d.scenes.push(Scene { name: name(s, &p)?, nodes: list });
    }
    d.scene = idx(json, "scene", "", d.scenes.len(), "scene")?;
    for (i, a) in arr(json, "animations", "", max)?.iter().enumerate() {
        let p = format!("animations[{i}]");
        obj(a, &p)?;
        let mut samplers = Vec::new();
        for (j, s) in arr(a, "samplers", &p, max)?.iter().enumerate() {
            let sp = format!("{p}.samplers[{j}]");
            obj(s, &sp)?;
            let interpolation = match string(s, "interpolation", &sp)? {
                None | Some("LINEAR") => Interpolation::Linear,
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::CubicSpline,
                Some(o) => return Err(gerr(C::OutOfRange, &field(&sp, "interpolation"), format!("'{o}' (LINEAR, STEP or CUBICSPLINE)"))),
            };
            samplers.push(AnimSampler {
                input: req_idx(s, "input", &sp, d.accessors.len(), "accessor")?,
                output: req_idx(s, "output", &sp, d.accessors.len(), "accessor")?,
                interpolation,
            });
        }
        let mut channels = Vec::new();
        for (j, c) in arr(a, "channels", &p, max)?.iter().enumerate() {
            let cp = format!("{p}.channels[{j}]");
            obj(c, &cp)?;
            let sampler = req_idx(c, "sampler", &cp, samplers.len(), "sampler")?;
            let target = c.get("target").ok_or_else(|| gerr(C::MissingField, &field(&cp, "target"), "required"))?;
            let tp = field(&cp, "target");
            obj(target, &tp)?;
            let raw_path = string(target, "path", &tp)?.ok_or_else(|| gerr(C::MissingField, &field(&tp, "path"), "required"))?.to_string();
            let path = match raw_path.as_str() {
                "translation" => Some(Path3::Translation),
                "rotation" => Some(Path3::Rotation),
                "scale" => Some(Path3::Scale),
                "weights" => Some(Path3::Weights),
                _ => None,
            };
            channels.push(AnimChannel { sampler, node: idx(target, "node", &tp, d.nodes.len(), "node")?, path, raw_path });
        }
        d.animations.push(Animation { name: name(a, &p)?, channels, samplers, specula: specula_extras(a) });
    }
    Ok(d)
}

fn parse_accessor(a: &Json, p: &str, d: &Document, limits: &Limits) -> Result<Accessor, GltfError> {
    obj(a, p)?;
    let view = idx(a, "bufferView", p, d.views.len(), "bufferView")?;
    let offset = usize_of(uint(a, "byteOffset", p)?.unwrap_or(0), p)?;
    let code = req_uint(a, "componentType", p)?;
    let ctype = ComponentType::from_code(code).ok_or_else(|| gerr(C::OutOfRange, &field(p, "componentType"), format!("{code} is not a glTF component type")))?;
    let normalized = boolean(a, "normalized", p, false)?;
    if normalized && matches!(ctype, ComponentType::F32 | ComponentType::U32) {
        return Err(gerr(C::InvalidData, &field(p, "normalized"), format!("{} accessors can't be normalized", ctype.name())));
    }
    let count = req_uint(a, "count", p)?;
    if count == 0 {
        return Err(gerr(C::OutOfRange, &field(p, "count"), "must be at least 1"));
    }
    let ty_s = string(a, "type", p)?.ok_or_else(|| gerr(C::MissingField, &field(p, "type"), "required"))?;
    let ty = ElemType::parse(ty_s).ok_or_else(|| gerr(C::OutOfRange, &field(p, "type"), format!("'{ty_s}' is not an accessor type")))?;
    let comps = count.saturating_mul(ty.dims() as u64);
    if count > limits.max_accessor_count || comps > limits.max_accessor_count * 4 {
        return Err(gerr(C::Limit, &field(p, "count"), format!("{count} elements; the limit is {}", limits.max_accessor_count)).with_numbers(count as f64, limits.max_accessor_count as f64));
    }
    if offset > 0 && view.is_none() {
        return Err(gerr(C::InvalidData, &field(p, "byteOffset"), "byteOffset without a bufferView"));
    }
    let sparse = match a.get("sparse") {
        None => None,
        Some(s) => {
            let sp = field(p, "sparse");
            obj(s, &sp)?;
            let n = req_uint(s, "count", &sp)?;
            if n == 0 || n > count {
                return Err(gerr(C::OutOfRange, &field(&sp, "count"), format!("{n} (1..={count})")));
            }
            let ip = field(&sp, "indices");
            let ix = s.get("indices").ok_or_else(|| gerr(C::MissingField, &ip, "required"))?;
            obj(ix, &ip)?;
            let icode = req_uint(ix, "componentType", &ip)?;
            let idx_type = match ComponentType::from_code(icode) {
                Some(t @ (ComponentType::U8 | ComponentType::U16 | ComponentType::U32)) => t,
                _ => return Err(gerr(C::OutOfRange, &field(&ip, "componentType"), format!("{icode} (sparse indices are UNSIGNED_BYTE, UNSIGNED_SHORT or UNSIGNED_INT)"))),
            };
            let vp = field(&sp, "values");
            let vx = s.get("values").ok_or_else(|| gerr(C::MissingField, &vp, "required"))?;
            obj(vx, &vp)?;
            Some(Sparse {
                count: n as usize,
                idx_view: req_idx(ix, "bufferView", &ip, d.views.len(), "bufferView")?,
                idx_offset: usize_of(uint(ix, "byteOffset", &ip)?.unwrap_or(0), &ip)?,
                idx_type,
                val_view: req_idx(vx, "bufferView", &vp, d.views.len(), "bufferView")?,
                val_offset: usize_of(uint(vx, "byteOffset", &vp)?.unwrap_or(0), &vp)?,
            })
        }
    };
    Ok(Accessor { view, offset, ctype, normalized, count: count as usize, ty, sparse })
}

fn parse_texref(m: &Json, k: &str, p: &str, textures: usize, scale_key: Option<&str>, warnings: &mut Vec<Warning>) -> Result<Option<TexRef>, GltfError> {
    let Some(t) = m.get(k) else { return Ok(None) };
    let tp = field(p, k);
    obj(t, &tp)?;
    let texture = req_idx(t, "index", &tp, textures, "texture")?;
    let tex_coord = uint(t, "texCoord", &tp)?.unwrap_or(0).min(u32::MAX as u64) as u32;
    let scale = match scale_key {
        Some(sk) => number(t, sk, &tp, 1.0, -1e6, 1e6)?,
        None => 1.0,
    };
    if t.get("extensions").and_then(|e| e.get("KHR_texture_transform")).is_some() {
        push_warning(warnings, Warning { code: "ignored_extension", path: tp.clone(), message: "KHR_texture_transform is not applied".into() });
    }
    Ok(Some(TexRef { texture, tex_coord, scale }))
}

fn parse_material(m: &Json, p: &str, textures: usize, warnings: &mut Vec<Warning>) -> Result<Material, GltfError> {
    obj(m, p)?;
    let (mut base_color, mut metallic, mut roughness) = ([1.0f32; 4], 1.0, 1.0);
    let (mut base_color_texture, mut metallic_roughness_texture) = (None, None);
    if let Some(pbr) = m.get("pbrMetallicRoughness") {
        let pp = field(p, "pbrMetallicRoughness");
        obj(pbr, &pp)?;
        base_color = vecn(pbr, "baseColorFactor", &pp, [1.0; 4])?;
        if base_color.iter().any(|c| !(0.0..=1.0).contains(c)) {
            return Err(gerr(C::OutOfRange, &field(&pp, "baseColorFactor"), "components are 0..=1"));
        }
        metallic = number(pbr, "metallicFactor", &pp, 1.0, 0.0, 1.0)?;
        roughness = number(pbr, "roughnessFactor", &pp, 1.0, 0.0, 1.0)?;
        base_color_texture = parse_texref(pbr, "baseColorTexture", &pp, textures, None, warnings)?;
        metallic_roughness_texture = parse_texref(pbr, "metallicRoughnessTexture", &pp, textures, None, warnings)?;
    }
    let emissive: [f32; 3] = vecn(m, "emissiveFactor", p, [0.0; 3])?;
    if emissive.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return Err(gerr(C::OutOfRange, &field(p, "emissiveFactor"), "components are 0..=1"));
    }
    let emissive_strength = match m.get("extensions").and_then(|e| e.get("KHR_materials_emissive_strength")) {
        Some(e) => number(e, "emissiveStrength", &format!("{p}.extensions.KHR_materials_emissive_strength"), 1.0, 0.0, 1e6)?,
        None => 1.0,
    };
    let alpha_mode = match string(m, "alphaMode", p)? {
        None | Some("OPAQUE") => AlphaMode::Opaque,
        Some("MASK") => AlphaMode::Mask,
        Some("BLEND") => AlphaMode::Blend,
        Some(o) => return Err(gerr(C::OutOfRange, &field(p, "alphaMode"), format!("'{o}' (OPAQUE, MASK or BLEND)"))),
    };
    Ok(Material {
        name: name(m, p)?,
        base_color,
        base_color_texture,
        metallic,
        roughness,
        metallic_roughness_texture,
        normal_texture: parse_texref(m, "normalTexture", p, textures, Some("scale"), warnings)?,
        occlusion_texture: parse_texref(m, "occlusionTexture", p, textures, Some("strength"), warnings)?,
        emissive,
        emissive_strength,
        emissive_texture: parse_texref(m, "emissiveTexture", p, textures, None, warnings)?,
        alpha_mode,
        alpha_cutoff: number(m, "alphaCutoff", p, 0.5, 0.0, 1e6)?,
        double_sided: boolean(m, "doubleSided", p, false)?,
        specula: specula_extras(m),
    })
}

//! glTF document → Specula model, skeleton, clips, materials and images.

use std::collections::{BTreeMap, BTreeSet};

use specula_anim::math::{qconj, qdot, qfrom_cols, qmul, qneg, qnorm, Transform, QID};
use specula_anim::{Channel, Clip, Interp, Keys, Skeleton, Track};
use specula_core::hash::StableHasher;
use specula_core::{dmath, jpeg, png, Json};
use specula_model::math3::{self, Affine};
use specula_model::mesh::MIN_TRIANGLE_AREA;
use specula_model::style::parse_hex;
use specula_model::{collider, Bone, Collider, MaterialKind, MaterialPreset, MeshData, Model, Part, Rig};

use crate::accessor::{expect_type, floats, uints};
use crate::color::{linear_to_srgb8, linear_to_srgb_unit, srgb8_to_linear, unorm8};
use crate::doc::{AlphaMode, ComponentType, Document, ElemType, Interpolation, Loaded, Local, Path3, TexRef};
use crate::error::{gerr, push_warning, GltfError, GltfErrorCode as C, Warning};
use crate::{Import, ImportOptions, ImportedImage, ImportedMaterial, ImportedTexture, Stats, TextureRef};

/// Linear rotation keys further apart than this get slerp-sampled keys in between.
const MAX_ROTATION_STEP_RAD: f32 = 0.349_065_85; // 20°

fn warn(w: &mut Vec<Warning>, code: &'static str, path: impl Into<String>, message: impl Into<String>) {
    push_warning(w, Warning { code, path: path.into(), message: message.into() });
}

/// A node's local transform as an affine matrix.
fn local_affine(l: &Local) -> Affine {
    match l {
        Local::Trs { t, r, s } => Transform { t: *t, r: *r, s: *s }.to_affine(),
        Local::Matrix(m) => Affine { cols: [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]], t: [m[12], m[13], m[14]] },
    }
}

/// A node's local transform as TRS (matrices decomposed; `false` if it had shear).
fn local_trs(l: &Local) -> (Transform, bool) {
    match l {
        Local::Trs { t, r, s } => (Transform { t: *t, r: *r, s: *s }, true),
        Local::Matrix(_) => {
            let m = local_affine(l);
            let mut s = [math3::length(m.cols[0]), math3::length(m.cols[1]), math3::length(m.cols[2])];
            if m.determinant() < 0.0 {
                s[0] = -s[0];
            }
            let r = if s.iter().all(|v| v.abs() > 1e-12) { qfrom_cols([math3::scale(m.cols[0], 1.0 / s[0]), math3::scale(m.cols[1], 1.0 / s[1]), math3::scale(m.cols[2], 1.0 / s[2])]) } else { QID };
            let tr = Transform { t: m.t, r, s };
            let back = tr.to_affine();
            let scale = 1.0 + s.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let exact = (0..3).all(|c| (0..3).all(|k| (back.cols[c][k] - m.cols[c][k]).abs() <= 1e-4 * scale));
            (tr, exact)
        }
    }
}

fn max_abs_diff(a: &Affine, b: &Affine) -> f32 {
    let mut d = 0.0f32;
    for c in 0..3 {
        for k in 0..3 {
            d = d.max((a.cols[c][k] - b.cols[c][k]).abs());
        }
        d = d.max((a.t[c] - b.t[c]).abs());
    }
    d
}

fn affine_scale(a: &Affine) -> f32 {
    let mut s = 1.0f32;
    for c in 0..3 {
        for k in 0..3 {
            s = s.max(a.cols[c][k].abs());
        }
        s = s.max(a.t[c].abs());
    }
    s
}

/// Unique names: the first use keeps the name, later ones get `_2`, `_3`, ...
fn unique(names: &mut BTreeSet<String>, want: &str) -> String {
    let base = if want.is_empty() { "unnamed" } else { want };
    if names.insert(base.to_string()) {
        return base.to_string();
    }
    let mut k = 2;
    loop {
        let n = format!("{base}_{k}");
        if names.insert(n.clone()) {
            return n;
        }
        k += 1;
    }
}

// ---------------------------------------------------------------------------------------------
// Images and textures.

/// The dimensions a PNG (IHDR) or JPEG (SOF) declares, read without decoding.
fn declared_size(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) && bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let wd = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as u64;
        let ht = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as u64;
        return Some((wd, ht));
    }
    jpeg::info(bytes).ok().map(|i| (i.width as u64, i.height as u64))
}

/// Where an image's bytes come from; equal sources are read and decoded once.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ImageSource {
    View(usize),
    File(std::path::PathBuf),
    /// A data URI, by the stable hash of its text (equal text confirmed before sharing).
    Data(u64),
}

/// Decodes every image a material slot uses (through a texture), each distinct source once.
/// Size limits are errors, checked from the PNG/JPEG header before decoding; unreadable or
/// undecodable images are warnings.
fn decode_images(l: &Loaded, opts: &ImportOptions, w: &mut Vec<Warning>) -> Result<Vec<ImportedImage>, GltfError> {
    let doc = &l.doc;
    let lim = &opts.limits;
    let mut used_textures = BTreeSet::new();
    for m in &doc.materials {
        for t in [m.base_color_texture, m.metallic_roughness_texture, m.normal_texture, m.occlusion_texture, m.emissive_texture].into_iter().flatten() {
            used_textures.insert(t.texture);
        }
    }
    let needed: BTreeSet<usize> = used_textures.iter().filter_map(|&t| doc.textures[t].source).collect();
    let mut budget = l.budget;
    let mut decoded_bytes: u64 = 0;
    let mut sources: BTreeMap<ImageSource, usize> = BTreeMap::new();
    let mut out = Vec::with_capacity(doc.images.len());
    for (i, im) in doc.images.iter().enumerate() {
        let path = format!("images[{i}]");
        let mut img = ImportedImage { name: im.name.clone(), mime: im.mime.clone().unwrap_or_default(), width: 0, height: 0, rgba: Vec::new(), error: None, duplicate_of: None };
        if !needed.contains(&i) {
            img.error = Some("not used by any material".into());
            out.push(img);
            continue;
        }
        // Resolve the source; failures to reach it are warnings.
        let source = match (&im.uri, im.view) {
            (Some(uri), _) => match crate::doc::resolve_uri(uri, l.base.as_deref(), &format!("{path}.uri"), lim) {
                Ok(crate::doc::UriSource::File { real, .. }) => Ok(ImageSource::File(real)),
                Ok(crate::doc::UriSource::Data { .. }) => Ok(ImageSource::Data(specula_core::hash::hash_str(uri))),
                Err(e) if e.code == C::Limit => return Err(e),
                Err(e) => Err(e),
            },
            (None, Some(v)) => Ok(ImageSource::View(v)),
            (None, None) => unreachable!("checked when parsing"),
        };
        let source = match source {
            Ok(s) => s,
            Err(e) => {
                warn(w, "image_unreadable", e.path.clone(), format!("{:?}: {}", e.code, e.message));
                img.error = Some(e.to_string());
                out.push(img);
                continue;
            }
        };
        // Equal data URIs (same hash and text) are the same source too.
        let same = sources.get(&source).copied().filter(|&k| !matches!(source, ImageSource::Data(_)) || doc.images[k].uri == im.uri);
        if let Some(k) = same {
            img.duplicate_of = Some(k);
            img.mime = out[k].mime.clone();
            img.width = out[k].width;
            img.height = out[k].height;
            img.error = out[k].error.clone();
            out.push(img);
            continue;
        }
        if !opts.decode_images {
            img.error = Some("image decoding is off".into());
            sources.entry(source.clone()).or_insert(i);
            out.push(img);
            continue;
        }
        let owned;
        let bytes: &[u8] = match (&source, &im.uri) {
            (ImageSource::View(v), _) => {
                let v = &doc.views[*v];
                &l.buffers[v.buffer][v.offset..v.offset + v.length]
            }
            (ImageSource::File(real), _) => {
                let len = std::fs::metadata(real).map(|m| m.len()).unwrap_or(0);
                budget.charge(len, &format!("{path}.uri"))?;
                owned = match crate::doc::read_file(real, None, &format!("{path}.uri")) {
                    Ok(b) => b,
                    Err(e) => {
                        warn(w, "image_unreadable", e.path.clone(), e.message.clone());
                        img.error = Some(e.to_string());
                        sources.entry(source.clone()).or_insert(i);
                        out.push(img);
                        continue;
                    }
                };
                &owned
            }
            (ImageSource::Data(_), Some(uri)) => {
                let up = format!("{path}.uri");
                let Ok(crate::doc::UriSource::Data { payload, offset }) = crate::doc::resolve_uri(uri, None, &up, lim) else { unreachable!("resolved above") };
                budget.charge(payload.len() as u64 / 4 * 3, &up)?;
                owned = match crate::doc::decode_data(payload, offset, &up) {
                    Ok(b) => b,
                    Err(e) => {
                        warn(w, "image_unreadable", e.path.clone(), e.message.clone());
                        img.error = Some(e.to_string());
                        sources.entry(source.clone()).or_insert(i);
                        out.push(img);
                        continue;
                    }
                };
                &owned
            }
            (ImageSource::Data(_), None) => unreachable!("data sources have a uri"),
        };
        sources.entry(source.clone()).or_insert(i);
        // Limits from the header, before any pixel memory is committed.
        if let Some((wd, ht)) = declared_size(bytes) {
            let px = wd.saturating_mul(ht);
            if px > lim.max_image_pixels {
                return Err(gerr(C::Limit, &path, format!("{wd}x{ht} image exceeds {} pixels", lim.max_image_pixels)).with_numbers(px as f64, lim.max_image_pixels as f64));
            }
            decoded_bytes = decoded_bytes.saturating_add(px.saturating_mul(4));
            if decoded_bytes > lim.max_image_bytes {
                return Err(gerr(C::Limit, &path, format!("decoded images total {decoded_bytes} bytes; the limit is {}", lim.max_image_bytes)).with_numbers(decoded_bytes as f64, lim.max_image_bytes as f64));
            }
        }
        let result = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            img.mime = "image/png".into();
            png::decode(bytes).map_err(|e| e.message)
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            img.mime = "image/jpeg".into();
            jpeg::decode_with_limits(bytes, &jpeg::Limits { max_pixels: lim.max_image_pixels }).map_err(|e| e.to_string())
        } else {
            Err(format!("unsupported image format (declared {:?}); PNG and JPEG are supported", im.mime))
        };
        match result {
            Ok(p) => {
                img.width = p.width;
                img.height = p.height;
                img.rgba = p.rgba;
            }
            Err(e) => {
                warn(w, "image_not_decoded", path, e.clone());
                img.error = Some(e);
            }
        }
        out.push(img);
    }
    Ok(out)
}

fn wrap_coord(i: i64, n: usize, mode: u32) -> usize {
    let n = n as i64;
    match mode {
        33071 => i.clamp(0, n - 1) as usize,
        33648 => {
            let p = i.rem_euclid(2 * n);
            (if p < n { p } else { 2 * n - 1 - p }) as usize
        }
        _ => i.rem_euclid(n) as usize,
    }
}

/// Bilinear sample of an sRGB texture at `uv`, in linear RGB (alpha linear).
fn sample(img: &ImportedImage, s: &crate::Sampler, uv: [f32; 2]) -> [f32; 4] {
    let (w, h) = (img.width as usize, img.height as usize);
    let x = uv[0] * w as f32 - 0.5;
    let y = uv[1] * h as f32 - 0.5;
    if !x.is_finite() || !y.is_finite() || x.abs() > 1e9 || y.abs() > 1e9 {
        return [1.0; 4];
    }
    let (x0, y0) = (dmath::floor(x), dmath::floor(y));
    let (fx, fy) = (x - x0, y - y0);
    let texel = |xi: i64, yi: i64| -> [f32; 4] {
        let (px, py) = (wrap_coord(xi, w, s.wrap_s), wrap_coord(yi, h, s.wrap_t));
        let p = &img.rgba[(py * w + px) * 4..(py * w + px) * 4 + 4];
        [srgb8_to_linear(p[0]), srgb8_to_linear(p[1]), srgb8_to_linear(p[2]), p[3] as f32 / 255.0]
    };
    let (xi, yi) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (texel(xi, yi), texel(xi + 1, yi), texel(xi, yi + 1), texel(xi + 1, yi + 1));
    let mut out = [0.0; 4];
    for k in 0..4 {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        out[k] = top + (bot - top) * fy;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Materials.

struct Materials {
    by_gltf: Vec<Option<usize>>,
    default: Option<usize>,
    list: Vec<ImportedMaterial>,
    names: BTreeSet<String>,
}

fn texref(t: Option<TexRef>) -> Option<TextureRef> {
    t.map(|t| TextureRef { texture: t.texture, tex_coord: t.tex_coord, scale: t.scale })
}

/// A preset from `extras.specula`, if it is complete and valid.
fn specula_preset(x: &Json) -> Option<(String, MaterialPreset)> {
    let num = |k: &str, hi: f32| x.get(k).and_then(Json::as_f64).map(|v| v as f32).filter(|v| (0.0..=hi).contains(v));
    let kind = match x.get("kind")?.as_str()? {
        "standard" => MaterialKind::Standard,
        "glass" => MaterialKind::Glass,
        _ => return None,
    };
    let preset = MaterialPreset {
        kind,
        color: parse_hex(x.get("color")?.as_str()?)?,
        roughness: num("roughness", 1.0)?,
        metallic: num("metallic", 1.0)?,
        emissive: num("emissive", 100.0)?,
        specular: num("specular", 1.0)?,
        opacity: num("opacity", 1.0)?,
    };
    Some((x.get("preset")?.as_str()?.to_string(), preset))
}

impl Materials {
    fn new(doc: &Document) -> Materials {
        Materials { by_gltf: vec![None; doc.materials.len()], default: None, list: Vec::new(), names: BTreeSet::new() }
    }

    fn get(&mut self, doc: &Document, m: Option<usize>, w: &mut Vec<Warning>) -> usize {
        if let Some(k) = m.map_or(self.default, |i| self.by_gltf[i]) {
            return k;
        }
        let k = self.list.len();
        let mat = match m {
            None => {
                let preset = MaterialPreset { kind: MaterialKind::Standard, color: [1.0; 4], roughness: 1.0, metallic: 1.0, emissive: 0.0, specular: 0.25, opacity: 1.0 };
                ImportedMaterial {
                    name: unique(&mut self.names, "default"),
                    preset,
                    base_color: [1.0; 4],
                    base_color_texture: None,
                    metallic: 1.0,
                    roughness: 1.0,
                    metallic_roughness_texture: None,
                    normal_texture: None,
                    occlusion_texture: None,
                    emissive: [0.0; 3],
                    emissive_strength: 1.0,
                    emissive_texture: None,
                    alpha_mode: AlphaMode::Opaque,
                    alpha_cutoff: 0.5,
                    double_sided: false,
                    from_specula: false,
                    texture_baked: false,
                }
            }
            Some(i) => {
                let g = &doc.materials[i];
                let restored = g.specula.as_ref().and_then(specula_preset);
                if g.specula.is_some() && restored.is_none() {
                    warn(w, "bad_specula_extras", format!("materials[{i}].extras.specula"), "not a valid Specula preset; derived from the glTF factors instead");
                }
                let from_specula = restored.is_some();
                let (want, preset) = restored.unwrap_or_else(|| {
                    let b = g.base_color;
                    let e = g.emissive.iter().fold(0.0f32, |a, v| a.max(*v)) * g.emissive_strength;
                    let bmax = b[0].max(b[1]).max(b[2]).max(1.0 / 255.0);
                    let preset = MaterialPreset {
                        kind: if g.alpha_mode == AlphaMode::Blend { MaterialKind::Glass } else { MaterialKind::Standard },
                        color: [linear_to_srgb_unit(b[0]), linear_to_srgb_unit(b[1]), linear_to_srgb_unit(b[2]), 1.0],
                        roughness: g.roughness,
                        metallic: g.metallic,
                        emissive: if e > 0.0 { (e / bmax).min(100.0) } else { 0.0 },
                        specular: 0.25,
                        opacity: if g.alpha_mode == AlphaMode::Blend { b[3] } else { 1.0 },
                    };
                    (g.name.clone().unwrap_or_else(|| format!("material_{i}")), preset)
                });
                ImportedMaterial {
                    name: unique(&mut self.names, &want),
                    preset,
                    base_color: g.base_color,
                    base_color_texture: texref(g.base_color_texture),
                    metallic: g.metallic,
                    roughness: g.roughness,
                    metallic_roughness_texture: texref(g.metallic_roughness_texture),
                    normal_texture: texref(g.normal_texture),
                    occlusion_texture: texref(g.occlusion_texture),
                    emissive: g.emissive,
                    emissive_strength: g.emissive_strength,
                    emissive_texture: texref(g.emissive_texture),
                    alpha_mode: g.alpha_mode,
                    alpha_cutoff: g.alpha_cutoff,
                    double_sided: g.double_sided,
                    from_specula,
                    texture_baked: false,
                }
            }
        };
        self.list.push(mat);
        match m {
            Some(i) => self.by_gltf[i] = Some(k),
            None => self.default = Some(k),
        }
        k
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry.

/// One mesh instance read in the node's local space.
struct Inst {
    node: usize,
    skin: Option<usize>,
    mesh: MeshData,
    /// Per vertex: up to 4 `(joint index in the skin, weight)`, weights normalised (skinned only).
    infl: Vec<[(u32, f32); 4]>,
}

/// Up to eight influences → the nonzero ones in their order (the four largest, stable, when
/// there are more), normalised; all-zero → `None`.
fn top4(js: &[u32], ws: &[f32]) -> Option<[(u32, f32); 4]> {
    let mut v: Vec<(u32, f32)> = js.iter().copied().zip(ws.iter().copied()).filter(|(_, w)| *w > 0.0).collect();
    if v.len() > 4 {
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(4);
    }
    let sum: f32 = v.iter().map(|x| x.1).sum();
    if sum.is_nan() || sum <= 0.0 {
        return None;
    }
    let mut out = [(0u32, 0.0f32); 4];
    for (o, (j, w)) in out.iter_mut().zip(v) {
        *o = (j, w / sum);
    }
    Some(out)
}

/// Normalised weights → unorm8 summing to exactly 255 (largest remainders, ties to the first;
/// zero weights stay zero).
fn quantize(w: [f32; 4]) -> [u8; 4] {
    let mut q = [0u32; 4];
    let mut rem = [-1.0f32; 4];
    for k in 0..4 {
        if w[k] > 0.0 {
            let x = w[k].min(1.0) * 255.0;
            q[k] = dmath::floor(x) as u32;
            rem[k] = x - q[k] as f32;
        }
    }
    let mut left = 255u32.saturating_sub(q.iter().sum());
    while left > 0 {
        let mut best = 0;
        for k in 1..4 {
            if rem[k] > rem[best] {
                best = k;
            }
        }
        if rem[best] < 0.0 {
            // Every remainder used: the largest weight takes the rest.
            best = (1..4).fold(0, |b, k| if w[k] > w[b] { k } else { b });
        }
        q[best] += 1;
        rem[best] = -1.0;
        left -= 1;
    }
    [q[0] as u8, q[1] as u8, q[2] as u8, q[3] as u8]
}

/// Triangle corner indices of a primitive (strips and fans converted).
fn triangles(mode: u32, idx: &[u32], path: &str) -> Result<Vec<u32>, GltfError> {
    match mode {
        4 => {
            if idx.len() % 3 != 0 {
                return Err(gerr(C::InvalidData, path, format!("TRIANGLES with {} indices (not a multiple of 3)", idx.len())));
            }
            Ok(idx.to_vec())
        }
        5 => {
            let mut t = Vec::with_capacity(idx.len().saturating_sub(2) * 3);
            for k in 0..idx.len().saturating_sub(2) {
                if k % 2 == 0 {
                    t.extend_from_slice(&[idx[k], idx[k + 1], idx[k + 2]]);
                } else {
                    t.extend_from_slice(&[idx[k + 1], idx[k], idx[k + 2]]);
                }
            }
            Ok(t)
        }
        6 => {
            let mut t = Vec::with_capacity(idx.len().saturating_sub(2) * 3);
            for k in 1..idx.len().saturating_sub(1) {
                t.extend_from_slice(&[idx[k], idx[k + 1], idx[0]]);
            }
            Ok(t)
        }
        m => {
            let what = ["POINTS", "LINES", "LINE_LOOP", "LINE_STRIP"].get(m as usize).copied().unwrap_or("unknown");
            Err(gerr(C::Unsupported, path, format!("primitive mode {m} ({what}) is not supported; only triangles (TRIANGLES, TRIANGLE_STRIP, TRIANGLE_FAN)")))
        }
    }
}

struct GeoCtx<'a> {
    doc: &'a Document,
    bufs: &'a [Vec<u8>],
    opts: &'a ImportOptions,
    images: &'a [ImportedImage],
    textures: &'a [ImportedTexture],
}

/// Reads mesh `mi` for one instance (node `node`, skin `skin`).
fn read_mesh(g: &GeoCtx, mats: &mut Materials, node: usize, mi: usize, skin: Option<usize>, w: &mut Vec<Warning>, stats: &mut Stats) -> Result<Inst, GltfError> {
    let doc = g.doc;
    let mut inst = Inst { node, skin, mesh: MeshData::default(), infl: Vec::new() };
    let mut has_uv = false;
    // Vertex sets already appended for (attributes, colour source) keys: shared accessors share vertices.
    let mut cache: BTreeMap<Vec<u64>, u32> = BTreeMap::new();
    let mut parts: Vec<Part> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    for (pi, prim) in doc.meshes[mi].primitives.iter().enumerate() {
        let path = format!("meshes[{mi}].primitives[{pi}]");
        if prim.has_targets {
            warn(w, "morph_targets_ignored", &path, "morph targets are not supported; the base shape is used");
        }
        let attr = |name: &str| prim.attributes.iter().find(|(k, _)| k == name).map(|(_, a)| *a);
        let Some(pos_acc) = attr("POSITION") else {
            warn(w, "no_positions", &path, "primitive without POSITION skipped");
            continue;
        };
        let ap = |name: &str| format!("{path}.attributes.{name}");
        expect_type(doc, pos_acc, &ap("POSITION"), &[ElemType::Vec3])?;
        let pos = floats(doc, g.bufs, pos_acc, &ap("POSITION"))?;
        let n = pos.len() / 3;
        let count_ok = |a: usize, name: &str| -> Result<(), GltfError> {
            let c = doc.accessors[a].count;
            if c != n {
                return Err(gerr(C::InvalidData, &ap(name), format!("{c} elements, POSITION has {n}")));
            }
            Ok(())
        };
        for (k, a) in &prim.attributes {
            count_ok(*a, k)?;
        }
        let idx: Vec<u32> = match prim.indices {
            Some(ia) => {
                let ipath = format!("{path}.indices");
                expect_type(doc, ia, &ipath, &[ElemType::Scalar])?;
                let v = uints(doc, g.bufs, ia, &ipath)?;
                if let Some((k, bad)) = v.iter().enumerate().find(|(_, &x)| x as usize >= n) {
                    return Err(gerr(C::InvalidData, &ipath, format!("index {k} is {bad}, but there are {n} vertices")));
                }
                v
            }
            None => (0..n as u32).collect(),
        };
        let tris = triangles(prim.mode, &idx, &format!("{path}.mode"))?;
        stats.primitives += 1;
        let mat = mats.get(doc, prim.material, w);
        let material = &mats.list[mat];
        let from_specula = material.from_specula;
        // Optional attributes.
        let normals = match attr("NORMAL") {
            Some(a) => {
                expect_type(doc, a, &ap("NORMAL"), &[ElemType::Vec3])?;
                Some(floats(doc, g.bufs, a, &ap("NORMAL"))?)
            }
            None => None,
        };
        if let Some(a) = attr("TANGENT") {
            expect_type(doc, a, &ap("TANGENT"), &[ElemType::Vec4])?;
            floats(doc, g.bufs, a, &ap("TANGENT"))?;
        }
        let uv = match attr("TEXCOORD_0") {
            Some(a) => {
                expect_type(doc, a, &ap("TEXCOORD_0"), &[ElemType::Vec2])?;
                Some(floats(doc, g.bufs, a, &ap("TEXCOORD_0"))?)
            }
            None => None,
        };
        let (col, col_dims) = match attr("COLOR_0") {
            Some(a) => {
                expect_type(doc, a, &ap("COLOR_0"), &[ElemType::Vec3, ElemType::Vec4])?;
                (Some(floats(doc, g.bufs, a, &ap("COLOR_0"))?), doc.accessors[a].ty.dims())
            }
            None => (None, 0),
        };
        // Skinning attributes (only meaningful on a node with a skin).
        let mut infl: Option<Vec<[(u32, f32); 4]>> = None;
        let joints0 = attr("JOINTS_0");
        if let (Some(s), Some(j0)) = (skin, joints0) {
            let w0 = attr("WEIGHTS_0").ok_or_else(|| gerr(C::MissingField, &ap("WEIGHTS_0"), "JOINTS_0 without WEIGHTS_0"))?;
            let mut js: Vec<Vec<u32>> = Vec::new();
            let mut ws: Vec<Vec<f32>> = Vec::new();
            for set in 0..2 {
                let (jn, wn) = (format!("JOINTS_{set}"), format!("WEIGHTS_{set}"));
                let (Some(ja), Some(wa)) = (if set == 0 { Some(j0) } else { attr(&jn) }, if set == 0 { Some(w0) } else { attr(&wn) }) else { continue };
                expect_type(doc, ja, &ap(&jn), &[ElemType::Vec4])?;
                expect_type(doc, wa, &ap(&wn), &[ElemType::Vec4])?;
                let wt = doc.accessors[wa].ctype;
                if !(wt == ComponentType::F32 || (doc.accessors[wa].normalized && matches!(wt, ComponentType::U8 | ComponentType::U16))) {
                    return Err(gerr(C::WrongType, &ap(&wn), "weights are FLOAT or normalized UNSIGNED_BYTE/SHORT"));
                }
                let jv = uints(doc, g.bufs, ja, &ap(&jn))?;
                let joints_len = doc.skins[s].joints.len();
                if let Some((k, bad)) = jv.iter().enumerate().find(|(_, &x)| x as usize >= joints_len) {
                    return Err(gerr(C::InvalidData, &ap(&jn), format!("vertex {} uses joint {bad}; skin {s} has {joints_len}", k / 4)));
                }
                let wv = floats(doc, g.bufs, wa, &ap(&wn))?;
                if wv.iter().any(|x| *x < 0.0) {
                    return Err(gerr(C::InvalidData, &ap(&wn), "negative weight"));
                }
                js.push(jv);
                ws.push(wv);
            }
            if attr("JOINTS_2").is_some() {
                warn(w, "extra_influences", &path, "JOINTS_2 and later sets are ignored (the four largest influences are kept)");
            }
            let mut v = Vec::with_capacity(n);
            let mut unweighted = 0;
            for i in 0..n {
                let jj: Vec<u32> = js.iter().flat_map(|j| j[4 * i..4 * i + 4].iter().copied()).collect();
                let ww: Vec<f32> = ws.iter().flat_map(|x| x[4 * i..4 * i + 4].iter().copied()).collect();
                v.push(top4(&jj, &ww).unwrap_or_else(|| {
                    unweighted += 1;
                    [(u32::MAX, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]
                }));
            }
            if unweighted > 0 {
                warn(w, "unweighted_vertices", &path, format!("{unweighted} vertices have no weight; bound to the root"));
            }
            infl = Some(v);
        } else if skin.is_some() {
            return Err(gerr(C::MissingField, &ap("JOINTS_0"), format!("mesh {mi} is skinned by node {node} but has no JOINTS_0")));
        } else if joints0.is_some() {
            warn(w, "skin_attributes_ignored", &path, "JOINTS_0/WEIGHTS_0 on a node without a skin are ignored");
        }
        // Colour: COLOR_0 × base colour factor × (optionally) the base colour texture.
        let mult = if from_specula { [1.0; 4] } else { material.base_color };
        let tex = material.base_color_texture.filter(|_| g.opts.bake_texture_colors && !from_specula).and_then(|t| {
            if t.tex_coord != 0 {
                warn(w, "texcoord_set_ignored", &path, format!("base colour texture uses TEXCOORD_{}; only TEXCOORD_0 is kept", t.tex_coord));
                return None;
            }
            let tx = g.textures.get(t.texture)?;
            let img = g.images.get(tx.image?)?;
            (!img.rgba.is_empty() && uv.is_some()).then_some((t.texture, img, tx.sampler))
        });
        if tex.is_some() {
            mats.list[mat].texture_baked = true;
        }
        let preset_rgba = mats.list[mat].preset.color;
        let color_of = |i: usize| -> [u8; 4] {
            if from_specula && col.is_none() {
                return [unorm8(preset_rgba[0]), unorm8(preset_rgba[1]), unorm8(preset_rgba[2]), unorm8(preset_rgba[3])];
            }
            let mut c = match &col {
                Some(cv) if col_dims == 4 => [cv[4 * i], cv[4 * i + 1], cv[4 * i + 2], cv[4 * i + 3]],
                Some(cv) => [cv[3 * i], cv[3 * i + 1], cv[3 * i + 2], 1.0],
                None => [1.0; 4],
            };
            for k in 0..4 {
                c[k] *= mult[k];
            }
            if let (Some((_, img, sampler)), Some(uvv)) = (&tex, &uv) {
                let t = sample(img, sampler, [uvv[2 * i], uvv[2 * i + 1]]);
                for k in 0..4 {
                    c[k] *= t[k];
                }
            }
            [linear_to_srgb8(c[0]), linear_to_srgb8(c[1]), linear_to_srgb8(c[2]), unorm8(c[3])]
        };
        // Append the vertices (shared when another primitive used the same accessors).
        let first_index = indices.len() as u32;
        if let Some(nv) = &normals {
            let mut key: Vec<u64> = ["POSITION", "NORMAL", "TEXCOORD_0", "COLOR_0", "JOINTS_0", "WEIGHTS_0", "JOINTS_1", "WEIGHTS_1"].iter().map(|k| attr(k).map_or(u64::MAX, |a| a as u64)).collect();
            key.extend(mult.iter().map(|m| m.to_bits() as u64));
            key.push(tex.as_ref().map_or(u64::MAX, |t| t.0 as u64));
            let base = match cache.get(&key) {
                Some(&b) => b,
                None => {
                    let b = inst.mesh.positions.len() as u32;
                    let mut bad = 0;
                    for i in 0..n {
                        inst.mesh.positions.push([pos[3 * i], pos[3 * i + 1], pos[3 * i + 2]]);
                        let nn = [nv[3 * i], nv[3 * i + 1], nv[3 * i + 2]];
                        let l = math3::length(nn);
                        // Unit normals keep their exact bits; others are normalised.
                        inst.mesh.normals.push(if (l - 1.0).abs() <= 1e-5 {
                            nn
                        } else if l > 1e-12 {
                            math3::scale(nn, 1.0 / l)
                        } else {
                            [0.0; 3]
                        });
                        if l.is_nan() || l <= 1e-12 {
                            bad += 1;
                        }
                        inst.mesh.colors.push(color_of(i));
                        uvs.push(uv.as_ref().map_or([0.0; 2], |u| [u[2 * i], u[2 * i + 1]]));
                        if let Some(v) = &infl {
                            inst.infl.push(v[i]);
                        }
                    }
                    if bad > 0 {
                        // Zero normals: the area-weighted normal of the triangles that use them.
                        let mut acc = vec![[0.0f32; 3]; n];
                        for t in tris.chunks_exact(3) {
                            let (a, b2, c) = (inst.mesh.positions[b as usize + t[0] as usize], inst.mesh.positions[b as usize + t[1] as usize], inst.mesh.positions[b as usize + t[2] as usize]);
                            let fnorm = math3::tri_normal(a, b2, c);
                            for &v in t {
                                acc[v as usize] = math3::add(acc[v as usize], fnorm);
                            }
                        }
                        for (slot, a) in inst.mesh.normals[b as usize..].iter_mut().zip(&acc) {
                            if *slot == [0.0; 3] {
                                *slot = if math3::length(*a) > 1e-20 { math3::normalize(*a) } else { [0.0, 1.0, 0.0] };
                            }
                        }
                        warn(w, "zero_normals", format!("{path}.attributes.NORMAL"), format!("{bad} zero-length normals replaced"));
                    }
                    cache.insert(key, b);
                    b
                }
            };
            indices.extend(tris.iter().map(|&i| base + i));
        } else {
            // No normals: flat shading, one vertex per triangle corner (glTF requires flat normals).
            for t in tris.chunks_exact(3) {
                let p = |k: usize| [pos[3 * t[k] as usize], pos[3 * t[k] as usize + 1], pos[3 * t[k] as usize + 2]];
                let fnorm = math3::tri_normal(p(0), p(1), p(2));
                let nn = if math3::length(fnorm) > 1e-20 { math3::normalize(fnorm) } else { [0.0, 1.0, 0.0] };
                for (k, &i) in t.iter().enumerate() {
                    let i = i as usize;
                    indices.push(inst.mesh.positions.len() as u32);
                    inst.mesh.positions.push(p(k));
                    inst.mesh.normals.push(nn);
                    inst.mesh.colors.push(color_of(i));
                    uvs.push(uv.as_ref().map_or([0.0; 2], |u| [u[2 * i], u[2 * i + 1]]));
                    if let Some(v) = &infl {
                        inst.infl.push(v[i]);
                    }
                }
            }
        }
        has_uv |= uv.is_some();
        let count = indices.len() as u32 - first_index;
        if count > 0 {
            parts.push(Part { material: mats.list[mat].name.clone(), first_index, index_count: count });
        }
        if inst.mesh.positions.len() > g.opts.limits.max_vertices {
            return Err(gerr(C::Limit, &path, format!("more than {} vertices", g.opts.limits.max_vertices)).with_numbers(inst.mesh.positions.len() as f64, g.opts.limits.max_vertices as f64));
        }
    }
    inst.mesh.indices = indices;
    inst.mesh.parts = parts;
    if has_uv {
        inst.mesh.uvs = uvs;
    }
    Ok(inst)
}

/// Drops triangles that repeat a vertex or have (near) zero area; returns how many.
fn drop_degenerate(m: &mut MeshData) -> usize {
    let mut indices = Vec::with_capacity(m.indices.len());
    let mut parts = Vec::with_capacity(m.parts.len());
    let mut dropped = 0;
    for p in &m.parts {
        let first = indices.len() as u32;
        for t in m.indices[p.first_index as usize..(p.first_index + p.index_count) as usize].chunks_exact(3) {
            let ok = t[0] != t[1] && t[1] != t[2] && t[0] != t[2] && {
                let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
                let area = 0.5 * math3::length(math3::tri_normal(a, b, c));
                area >= MIN_TRIANGLE_AREA
            };
            if ok {
                indices.extend_from_slice(t);
            } else {
                dropped += 1;
            }
        }
        let count = indices.len() as u32 - first;
        if count > 0 {
            parts.push(Part { material: p.material.clone(), first_index: first, index_count: count });
        }
    }
    m.indices = indices;
    m.parts = parts;
    dropped
}

// ---------------------------------------------------------------------------------------------
// Animation.

/// Catmull–Rom tangent at key `k`, exactly as `specula_anim::Keys::sample` computes it.
fn cr_tangent(times: &[f32], vals: &[[f32; 4]], k: usize, wrap: Option<f32>) -> [f32; 4] {
    let n = times.len();
    let closed = wrap.filter(|d| n >= 3 && times[0] == 0.0 && times[n - 1] == *d);
    let (tp, vp) = if k >= 1 {
        (times[k - 1], vals[k - 1])
    } else if let Some(d) = closed {
        (times[n - 2] - d, vals[n - 2])
    } else {
        (times[k], vals[k])
    };
    let (tn, vn) = if k + 1 < n {
        (times[k + 1], vals[k + 1])
    } else if let Some(d) = closed {
        (times[1] + d, vals[1])
    } else {
        (times[k], vals[k])
    };
    let inv = 1.0 / (tn - tp);
    [(vn[0] - vp[0]) * inv, (vn[1] - vp[1]) * inv, (vn[2] - vp[2]) * inv, (vn[3] - vp[3]) * inv]
}

/// glTF CUBICSPLINE value at `t` (clamped to the keys).
fn hermite(times: &[f32], v: &[[f32; 4]], a: &[[f32; 4]], b: &[[f32; 4]], t: f32, rotation: bool) -> [f32; 4] {
    let n = times.len();
    if t <= times[0] {
        return v[0];
    }
    if t >= times[n - 1] {
        return v[n - 1];
    }
    let k = times.partition_point(|&x| x <= t) - 1;
    let td = times[k + 1] - times[k];
    let s = (t - times[k]) / td;
    let (s2, s3) = (s * s, s * s * s);
    let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
    let mut out = [0.0; 4];
    for c in 0..4 {
        out[c] = h00 * v[k][c] + h10 * td * b[k][c] + h01 * v[k + 1][c] + h11 * td * a[k + 1][c];
    }
    if rotation {
        qnorm(out)
    } else {
        out
    }
}

/// Spherical interpolation of unit quaternions (`b` already in `a`'s hemisphere).
fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let d = qdot(a, b).clamp(-1.0, 1.0);
    let th = dmath::acos(d);
    let s = dmath::sin(th);
    if s < 1e-6 {
        return qnorm([a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u, a[3] + (b[3] - a[3]) * u]);
    }
    let (wa, wb) = (dmath::sin((1.0 - u) * th) / s, dmath::sin(u * th) / s);
    qnorm([a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb, a[2] * wa + b[2] * wb, a[3] * wa + b[3] * wb])
}

/// A uniform grid from the first to the last key time with at most `max` keys.
fn grid(first: f32, last: f32, hz: f32, max: usize) -> Vec<f32> {
    let span = last - first;
    let n = ((span * hz.max(1.0)).ceil() as usize + 1).clamp(2, max.max(2));
    let mut out: Vec<f32> = Vec::with_capacity(n);
    for i in 0..n {
        let t = if i + 1 == n { last } else { first + span * (i as f32 / (n - 1) as f32) };
        if out.last().is_none_or(|&p| t > p) {
            out.push(t);
        }
    }
    out
}

struct RawChannel {
    bone: usize,
    channel: Channel,
    interp: Interpolation,
    times: Vec<f32>,
    /// Absolute values (`[x, y, z, 0]` vectors, `[x, y, z, w]` rotations); for cubic splines
    /// also the in- and out-tangents.
    values: Vec<[f32; 4]>,
    in_tan: Vec<[f32; 4]>,
    out_tan: Vec<[f32; 4]>,
    path: String,
}

/// Reads one channel's keys (absolute values; tangents for cubic splines).
#[allow(clippy::too_many_arguments)]
fn read_channel(doc: &Document, bufs: &[Vec<u8>], a: &crate::doc::Animation, apath: &str, bone: usize, channel: Channel, si: usize, cp: String, times_of: &BTreeMap<usize, Vec<f32>>) -> Result<RawChannel, GltfError> {
    let s = &a.samplers[si];
    let sp = format!("{apath}.samplers[{si}]");
    let times = times_of[&s.input].clone();
    let rotation = channel == Channel::Rotation;
    expect_type(doc, s.output, &format!("{sp}.output"), &[if rotation { ElemType::Vec4 } else { ElemType::Vec3 }])?;
    let out = floats(doc, bufs, s.output, &format!("{sp}.output"))?;
    let dims = if rotation { 4 } else { 3 };
    let n = times.len();
    let per = if s.interpolation == Interpolation::CubicSpline { 3 } else { 1 };
    if out.len() != n * dims * per {
        return Err(gerr(C::InvalidData, &format!("{sp}.output"), format!("{} values for {n} keys ({} expected)", out.len() / dims, n * per)));
    }
    if per == 3 && n < 2 {
        return Err(gerr(C::InvalidData, &format!("{sp}.input"), "CUBICSPLINE needs at least two keys"));
    }
    let vec4 = |k: usize| -> [f32; 4] {
        let o = &out[k * dims..k * dims + dims];
        if rotation {
            [o[0], o[1], o[2], o[3]]
        } else {
            [o[0], o[1], o[2], 0.0]
        }
    };
    let (mut values, mut in_tan, mut out_tan) = (Vec::with_capacity(n), Vec::new(), Vec::new());
    for k in 0..n {
        if per == 3 {
            in_tan.push(vec4(3 * k));
            values.push(vec4(3 * k + 1));
            out_tan.push(vec4(3 * k + 2));
        } else {
            values.push(vec4(k));
        }
    }
    if rotation {
        for (k, q) in values.iter_mut().enumerate() {
            let l2 = qdot(*q, *q);
            if l2.is_nan() || l2 <= 1e-12 {
                return Err(gerr(C::InvalidData, &format!("{sp}.output"), format!("key {k} is a zero quaternion")));
            }
            if (l2 - 1.0).abs() > 1e-6 {
                *q = qnorm(*q);
            }
        }
    }
    Ok(RawChannel { bone, channel, interp: s.interpolation, times, values, in_tan, out_tan, path: cp })
}

#[allow(clippy::too_many_arguments)]
fn build_clip(doc: &Document, bufs: &[Vec<u8>], ai: usize, skel: &Skeleton, bone_of: &[Option<usize>], in_scene: &[bool], names: &mut BTreeSet<String>, keys_total: &mut [usize; 2], opts: &ImportOptions, w: &mut Vec<Warning>) -> Result<Option<Clip>, GltfError> {
    let a = &doc.animations[ai];
    let apath = format!("animations[{ai}]");
    // Pass 1: which channels to import, and their raw key count against the budget, before
    // any key data is read. Channels are then read and converted one at a time.
    let mut plan: Vec<(usize, Channel, usize, String)> = Vec::new();
    let mut seen: BTreeSet<(usize, u8)> = BTreeSet::new();
    let mut morph_warned = false;
    for (ci, c) in a.channels.iter().enumerate() {
        let cp = format!("{apath}.channels[{ci}]");
        let channel = match c.path {
            Some(Path3::Translation) => Channel::Translation,
            Some(Path3::Rotation) => Channel::Rotation,
            Some(Path3::Scale) => Channel::Scale,
            Some(Path3::Weights) => {
                if !morph_warned {
                    warn(w, "morph_animation_ignored", &cp, "morph target weight channels are ignored");
                    morph_warned = true;
                }
                continue;
            }
            None => {
                warn(w, "channel_ignored", &cp, format!("target path '{}' is not supported", c.raw_path));
                continue;
            }
        };
        let Some(node) = c.node else {
            warn(w, "channel_ignored", &cp, "target has no node (extension-defined targets are not supported)");
            continue;
        };
        if !in_scene[node] {
            warn(w, "channel_ignored", &cp, format!("node {node} is not in the imported scene"));
            continue;
        }
        let Some(bone) = bone_of[node] else {
            warn(w, "channel_ignored", &cp, format!("node {node}'s bone was pruned (unused leaf)"));
            continue;
        };
        if !seen.insert((node, channel as u8)) {
            return Err(gerr(C::InvalidData, &cp, format!("a second {} channel for node {node} in one animation", channel.name())));
        }
        let s = &a.samplers[c.sampler];
        let sp = format!("{apath}.samplers[{}]", c.sampler);
        expect_type(doc, s.input, &format!("{sp}.input"), &[ElemType::Scalar])?;
        if doc.accessors[s.input].ctype != ComponentType::F32 {
            return Err(gerr(C::WrongType, &format!("{sp}.input"), "key times are FLOAT"));
        }
        let per = if s.interpolation == Interpolation::CubicSpline { 3 } else { 1 };
        keys_total[1] = keys_total[1].saturating_add(doc.accessors[s.input].count.saturating_mul(per));
        if keys_total[1] > opts.limits.max_total_keys {
            return Err(gerr(C::Limit, &cp, format!("animations hold more than {} keys (counting every channel)", opts.limits.max_total_keys)).with_numbers(keys_total[1] as f64, opts.limits.max_total_keys as f64));
        }
        plan.push((bone, channel, c.sampler, cp));
    }
    if plan.is_empty() {
        warn(w, "empty_animation", &apath, "no usable channels; skipped");
        return Ok(None);
    }
    // Key times, read once per input accessor.
    let mut times_of: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for (_, _, si, _) in &plan {
        let input = a.samplers[*si].input;
        if times_of.contains_key(&input) {
            continue;
        }
        let ip = format!("{apath}.samplers[{si}].input");
        let times = floats(doc, bufs, input, &ip)?;
        for (k, t) in times.iter().enumerate() {
            if *t < 0.0 || (k > 0 && times[k - 1] >= *t) {
                return Err(gerr(C::InvalidData, &ip, format!("key {k} at {t} s: times must be non-negative and strictly increasing")));
            }
        }
        times_of.insert(input, times);
    }
    let last = times_of.values().map(|t| *t.last().expect("at least one key")).fold(0.0f32, f32::max);
    if last > specula_anim::clip::MAX_DURATION_S {
        return Err(gerr(C::Limit, &apath, format!("{last} s long; clips are at most {} s", specula_anim::clip::MAX_DURATION_S)).with_numbers(last as f64, specula_anim::clip::MAX_DURATION_S as f64));
    }
    // Specula exports record the clip's own duration (it may run past the last key).
    let declared = a.specula.as_ref().and_then(|x| x.get("duration")).and_then(Json::as_f64).map(|d| d as f32);
    let duration = declared.filter(|d| d.is_finite() && *d >= last && *d <= specula_anim::clip::MAX_DURATION_S).unwrap_or(last).max(1e-3);
    // Clip metadata: Specula extras (loop, distance, events, curves), validated by the clip parser.
    let want_name = doc.animations[ai].name.clone().unwrap_or_else(|| format!("animation_{ai}"));
    let name = unique(names, &want_name);
    let mut meta = vec![("name", Json::Str(name.clone())), ("duration", Json::Num(duration as f64))];
    if let Some(x) = &a.specula {
        for k in ["loop", "distance", "events", "curves"] {
            if let Some(v) = x.get(k) {
                meta.push((k, v.clone()));
            }
        }
    } else {
        meta.push(("loop", Json::Str(opts.default_loop.name().into())));
    }
    let mut clip = Clip::from_json(&Json::obj(meta), "").map_err(|e| gerr(C::InvalidData, &format!("{apath}.extras.specula{}", if e.path.is_empty() { String::new() } else { format!(".{}", e.path) }), e.message))?;
    let wrap = clip.wrap();
    let max_keys = specula_anim::clip::MAX_KEYS;
    for (bone, channel, si, cp) in plan {
        let r = read_channel(doc, bufs, a, &apath, bone, channel, si, cp, &times_of)?;
        let rest = skel.rest[r.bone];
        let rotation = r.channel == Channel::Rotation;
        // Absolute keys → (interp, times, absolute values) in Specula's interpolation terms.
        let (interp, times, values) = match r.interp {
            Interpolation::Step | Interpolation::Linear if r.times.len() > max_keys => {
                warn(w, "keys_resampled", &r.path, format!("{} keys resampled to {max_keys}", r.times.len()));
                let ks = Keys { interp: if r.interp == Interpolation::Step { Interp::Step } else { Interp::Linear }, times: r.times.clone(), values: r.values.clone() };
                let t = grid(r.times[0], *r.times.last().unwrap_or(&0.0), opts.resample_hz, max_keys);
                let v = t.iter().map(|&x| ks.sample(x, rotation, None)).collect();
                (ks.interp, t, v)
            }
            Interpolation::Step => (Interp::Step, r.times, r.values),
            Interpolation::Linear if rotation => {
                // Specula blends rotations with nlerp; add slerp keys where steps are large.
                let mut t = vec![r.times[0]];
                let mut v = vec![r.values[0]];
                for k in 1..r.times.len() {
                    let a0 = r.values[k - 1];
                    let b0 = if qdot(a0, r.values[k]) < 0.0 { qneg(r.values[k]) } else { r.values[k] };
                    let angle = 2.0 * dmath::acos(qdot(a0, b0).clamp(-1.0, 1.0));
                    if angle > MAX_ROTATION_STEP_RAD {
                        let m = (angle / MAX_ROTATION_STEP_RAD).ceil() as usize;
                        if t.len() + m < max_keys {
                            for i in 1..m {
                                let u = i as f32 / m as f32;
                                let ti = r.times[k - 1] + (r.times[k] - r.times[k - 1]) * u;
                                if ti > *t.last().unwrap_or(&f32::MIN) && ti < r.times[k] {
                                    t.push(ti);
                                    v.push(slerp(a0, b0, u));
                                }
                            }
                        }
                    }
                    t.push(r.times[k]);
                    v.push(r.values[k]);
                }
                (Interp::Linear, t, v)
            }
            Interpolation::Linear => (Interp::Linear, r.times, r.values),
            Interpolation::CubicSpline => {
                let n = r.times.len();
                let tol = |m: f32| 1e-4 * (1.0 + m.abs());
                let catmull_rom = (0..n).all(|k| {
                    let m = cr_tangent(&r.times, &r.values, k, wrap);
                    (0..4).all(|c| (r.in_tan[k][c] - m[c]).abs() <= tol(m[c]) && (r.out_tan[k][c] - m[c]).abs() <= tol(m[c]))
                });
                if catmull_rom && n <= max_keys {
                    (Interp::Cubic, r.times, r.values)
                } else {
                    let t = grid(r.times[0], r.times[n - 1], opts.resample_hz, max_keys);
                    let mut v: Vec<[f32; 4]> = Vec::with_capacity(t.len());
                    for &x in &t {
                        let mut q = hermite(&r.times, &r.values, &r.in_tan, &r.out_tan, x, rotation);
                        if rotation {
                            if let Some(p) = v.last() {
                                if qdot(*p, q) < 0.0 {
                                    q = qneg(q);
                                }
                            }
                        }
                        v.push(q);
                    }
                    warn(w, "cubic_resampled", &r.path, format!("CUBICSPLINE tangents aren't Catmull-Rom; resampled to {} linear keys", t.len()));
                    (Interp::Linear, t, v)
                }
            }
        };
        // Absolute → rest-relative.
        let mut rel = Vec::with_capacity(values.len());
        for v in values {
            rel.push(match r.channel {
                Channel::Translation => [v[0] - rest.t[0], v[1] - rest.t[1], v[2] - rest.t[2], 0.0],
                Channel::Rotation => {
                    if rest.r == QID {
                        v
                    } else {
                        qnorm(qmul(qconj(rest.r), v))
                    }
                }
                Channel::Scale => {
                    if rest.s.iter().any(|s| s.abs() < 1e-12) {
                        return Err(gerr(C::InvalidData, &r.path, "scale channel on a bone whose rest scale is 0"));
                    }
                    [v[0] / rest.s[0], v[1] / rest.s[1], v[2] / rest.s[2], 0.0]
                }
            });
        }
        // Values Specula clips can't hold (huge offsets, negative or huge scales) drop the track.
        let max_off = specula_anim::clip::MAX_OFFSET_M;
        let bad = rel.iter().position(|v| match r.channel {
            Channel::Translation => v[..3].iter().any(|x| x.abs() > max_off),
            Channel::Scale => v[..3].iter().any(|x| !(0.0..=100.0).contains(x)),
            Channel::Rotation => false,
        });
        if let Some(k) = bad {
            warn(w, "channel_out_of_range", &r.path, format!("key {k} is {:?} relative to the rest pose; Specula clips hold offsets within {max_off} m and scales 0..=100; channel dropped", &rel[k][..3]));
            continue;
        }
        keys_total[0] += times.len();
        if keys_total[0] > opts.limits.max_total_keys {
            return Err(gerr(C::Limit, &r.path, format!("more than {} animation keys in total", opts.limits.max_total_keys)).with_numbers(keys_total[0] as f64, opts.limits.max_total_keys as f64));
        }
        clip.tracks.push(Track { bone: skel.names[r.bone].clone(), channel: r.channel, keys: Keys { interp, times, values: rel } });
    }
    clip.validate().map_err(|e| gerr(C::InvalidData, &format!("{apath} (clip '{}'{})", clip.name, if e.path.is_empty() { String::new() } else { format!(", {}", e.path) }), e.message))?;
    Ok(Some(clip))
}

// ---------------------------------------------------------------------------------------------

pub(crate) fn convert(l: &Loaded, opts: &ImportOptions, mut warnings: Vec<Warning>) -> Result<Import, GltfError> {
    let doc = &l.doc;
    let bufs = &l.buffers;
    let lim = &opts.limits;
    let w = &mut warnings;
    let n = doc.nodes.len();
    let mut stats = Stats { nodes: n, meshes: doc.meshes.len(), skins: doc.skins.len(), animations: doc.animations.len(), ..Stats::default() };

    // Hierarchy: each node has at most one parent.
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (i, node) in doc.nodes.iter().enumerate() {
        for (k, &c) in node.children.iter().enumerate() {
            if c == i || parent[c].is_some() {
                return Err(gerr(C::InvalidData, &format!("nodes[{i}].children[{k}]"), format!("node {c} {}", if c == i { "is its own child" } else { "has two parents" })));
            }
            parent[c] = Some(i);
        }
    }
    let scene = match opts.scene {
        Some(s) if s >= doc.scenes.len() => return Err(gerr(C::BadReference, "scene", format!("scene {s} doesn't exist ({} defined)", doc.scenes.len()))),
        Some(s) => Some(s),
        None => doc.scene.or(if doc.scenes.is_empty() { None } else { Some(0) }),
    };
    let (roots, scene_name): (Vec<usize>, Option<String>) = match scene {
        Some(s) => (doc.scenes[s].nodes.clone(), doc.scenes[s].name.clone()),
        None => {
            if n > 0 {
                warn(w, "no_scene", "", "no scenes; every root node is imported");
            }
            ((0..n).filter(|&i| parent[i].is_none()).collect(), None)
        }
    };
    let mut in_scene = vec![false; n];
    let mut world = vec![Affine::IDENTITY; n];
    let mut order = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (k, &r) in roots.iter().enumerate().rev() {
        if parent[r].is_some() {
            return Err(gerr(C::InvalidData, &format!("scenes[{}].nodes[{k}]", scene.unwrap_or(0)), format!("node {r} is a child of node {}, not a root", parent[r].unwrap_or(0))));
        }
        stack.push((r, 0));
    }
    while let Some((i, depth)) = stack.pop() {
        if in_scene[i] {
            return Err(gerr(C::InvalidData, "scenes", format!("node {i} is listed twice")));
        }
        if depth >= lim.max_node_depth {
            return Err(gerr(C::Limit, &format!("nodes[{i}]"), format!("hierarchy deeper than {}", lim.max_node_depth)).with_numbers(depth as f64, lim.max_node_depth as f64));
        }
        in_scene[i] = true;
        order.push(i);
        let local = local_affine(&doc.nodes[i].local);
        world[i] = match parent[i] {
            Some(p) => world[p].then_after(&local),
            None => local,
        };
        for &c in doc.nodes[i].children.iter().rev() {
            stack.push((c, depth + 1));
        }
    }

    let images = decode_images(l, opts, w)?;
    // Textures point at the image that holds the pixels (duplicates share the first copy).
    let textures: Vec<ImportedTexture> = doc
        .textures
        .iter()
        .map(|t| ImportedTexture { name: t.name.clone(), image: t.source.map(|k| images[k].duplicate_of.unwrap_or(k)), sampler: t.sampler.map(|s| doc.samplers[s]).unwrap_or_default() })
        .collect();

    // Bones: skin joints and animated nodes, closed under ancestors.
    let insts: Vec<usize> = order.iter().copied().filter(|&i| doc.nodes[i].mesh.is_some()).collect();
    let mut is_bone = vec![false; n];
    for &i in &insts {
        if let Some(s) = doc.nodes[i].skin {
            for (k, &j) in doc.skins[s].joints.iter().enumerate() {
                if !in_scene[j] {
                    return Err(gerr(C::InvalidData, &format!("skins[{s}].joints[{k}]"), format!("joint node {j} is not in the imported scene")));
                }
                is_bone[j] = true;
            }
        }
    }
    for a in &doc.animations {
        for c in &a.channels {
            if let (Some(nd), Some(Path3::Translation | Path3::Rotation | Path3::Scale)) = (c.node, c.path) {
                if in_scene[nd] {
                    is_bone[nd] = true;
                }
            }
        }
    }
    for &i in &order {
        if is_bone[i] {
            let mut p = parent[i];
            while let Some(q) = p {
                is_bone[q] = true;
                p = parent[q];
            }
        }
    }
    // Vertex/triangle budget before reading anything big.
    let mut est_v = 0usize;
    let mut est_t = 0usize;
    for &i in &insts {
        let m = doc.nodes[i].mesh.unwrap_or(0);
        for pr in &doc.meshes[m].primitives {
            let pv = pr.attributes.iter().find(|(k, _)| k == "POSITION").map_or(0, |(_, a)| doc.accessors[*a].count);
            let ic = pr.indices.map_or(pv, |a| doc.accessors[a].count);
            // Triangles by mode (strips and fans make one per index after the first two); without
            // normals every triangle corner becomes its own vertex.
            let tris = if pr.mode == 4 { ic / 3 } else { ic.saturating_sub(2) };
            est_v = est_v.saturating_add(if pr.attributes.iter().any(|(k, _)| k == "NORMAL") { pv } else { tris.saturating_mul(3) });
            est_t = est_t.saturating_add(tris);
        }
    }
    if est_v > lim.max_vertices || est_t > lim.max_triangles {
        return Err(gerr(C::Limit, "meshes", format!("the scene bakes to about {est_v} vertices / {est_t} triangles; limits {} / {}", lim.max_vertices, lim.max_triangles)).with_numbers(est_v as f64, lim.max_vertices as f64));
    }

    // Read every instance in its local space.
    let g = GeoCtx { doc, bufs, opts, images: &images, textures: &textures };
    let mut mats = Materials::new(doc);
    let mut raw_insts = Vec::with_capacity(insts.len());
    for &i in &insts {
        let node = &doc.nodes[i];
        let mi = node.mesh.unwrap_or(0);
        raw_insts.push(read_mesh(&g, &mut mats, i, mi, node.skin, w, &mut stats)?);
        stats.instances += 1;
    }

    // Prune unused leaf bones if over the limit.
    let nearest_bone = |mut k: usize, is_bone: &[bool]| -> Option<usize> {
        loop {
            if is_bone[k] {
                return Some(k);
            }
            k = parent[k]?;
        }
    };
    let mut used = vec![false; n];
    for inst in &raw_insts {
        match inst.skin {
            Some(s) => {
                for v in &inst.infl {
                    for &(j, wt) in v {
                        if wt > 0.0 && j != u32::MAX {
                            used[doc.skins[s].joints[j as usize]] = true;
                        }
                    }
                }
            }
            None => {
                if let Some(b) = nearest_bone(inst.node, &is_bone) {
                    used[b] = true;
                }
            }
        }
    }
    // Bone order: skin joint order first (so a parents-first joint list keeps its indices),
    // then the remaining bones in scene order; parents are always placed before children.
    let mut rank = vec![usize::MAX; n];
    let mut next = 0;
    for inst in &raw_insts {
        if let Some(s) = inst.skin {
            for &j in &doc.skins[s].joints {
                if rank[j] == usize::MAX {
                    rank[j] = next;
                    next += 1;
                }
            }
        }
    }
    for &i in &order {
        if rank[i] == usize::MAX {
            rank[i] = next;
            next += 1;
        }
    }
    let bone_nodes = |is_bone: &[bool]| -> Vec<usize> {
        let mut by_rank: Vec<usize> = order.iter().copied().filter(|&i| is_bone[i]).collect();
        by_rank.sort_by_key(|&i| rank[i]);
        let mut placed = vec![false; n];
        let mut out = Vec::with_capacity(by_rank.len());
        for b in by_rank {
            // Place missing ancestors first (outermost first).
            let mut chain = Vec::new();
            let mut k = Some(b);
            while let Some(x) = k {
                if placed[x] {
                    break;
                }
                chain.push(x);
                k = parent[x];
            }
            for &x in chain.iter().rev() {
                placed[x] = true;
                out.push(x);
            }
        }
        out
    };
    let mut bones = bone_nodes(&is_bone);
    let top_level = bones.iter().filter(|&&b| parent[b].is_none()).count();
    let loose = raw_insts.iter().any(|r| r.skin.is_none() && nearest_bone(r.node, &is_bone).is_none());
    let synthetic = !bones.is_empty() && (top_level > 1 || loose);
    let max_bones = specula_model::rig::MAX_BONES;
    if bones.len() + usize::from(synthetic) > max_bones {
        let mut pruned = Vec::new();
        for &b in order.iter().rev() {
            if bones.len() - pruned.len() + usize::from(synthetic) <= max_bones {
                break;
            }
            let has_bone_child = doc.nodes[b].children.iter().any(|&c| is_bone[c]);
            if is_bone[b] && !used[b] && !has_bone_child {
                is_bone[b] = false;
                pruned.push(b);
            }
        }
        bones = bone_nodes(&is_bone);
        stats.pruned_bones = pruned.len();
        if !pruned.is_empty() {
            let names: Vec<String> = pruned.iter().map(|&b| doc.nodes[b].name.clone().unwrap_or_else(|| format!("node{b}"))).collect();
            warn(w, "bones_pruned", "skins", format!("{} unused leaf bones removed to fit {max_bones}: {}", pruned.len(), names.join(", ")));
        }
        if bones.len() + usize::from(synthetic) > max_bones {
            let have = bones.len() + usize::from(synthetic);
            return Err(gerr(C::Limit, "skins", format!("{have} bones after pruning unused leaves; a Specula rig has at most {max_bones}")).with_numbers(have as f64, max_bones as f64));
        }
    }

    // Skeleton.
    let mut bone_of: Vec<Option<usize>> = vec![None; n];
    let skeleton = if bones.is_empty() {
        None
    } else {
        let mut names = BTreeSet::new();
        let mut bnames = Vec::new();
        let mut parents: Vec<Option<u16>> = Vec::new();
        let mut rest = Vec::new();
        if synthetic {
            bnames.push(unique(&mut names, "root"));
            parents.push(None);
            rest.push(Transform::IDENTITY);
        }
        // Names first so the synthetic root keeps "root" only if no node claims it first.
        for &b in &bones {
            bone_of[b] = Some(bnames.len());
            bnames.push(unique(&mut names, &doc.nodes[b].name.clone().unwrap_or_else(|| format!("node{b}"))));
            parents.push(match parent[b] {
                Some(p) => Some(bone_of[p].expect("ancestors of bones are bones") as u16),
                None if synthetic => Some(0),
                None => None,
            });
            let (t, exact) = local_trs(&doc.nodes[b].local);
            if !exact {
                warn(w, "matrix_shear", format!("nodes[{b}].matrix"), "the matrix has shear; the bone uses its closest translation/rotation/scale");
            }
            rest.push(t);
        }
        let mut s = Skeleton::new(bnames, parents, rest).map_err(|e| gerr(C::InvalidData, "nodes", format!("skeleton: {e}")))?;
        // Tails: our exporter's extras, else the only child bone's head, else the head.
        for (k, &b) in bones.iter().enumerate() {
            let bi = k + usize::from(synthetic);
            let kids: Vec<usize> = doc.nodes[b].children.iter().filter_map(|&c| bone_of[c]).collect();
            s.tails[bi] = doc.nodes[b].tail.unwrap_or(if kids.len() == 1 { s.heads[kids[0]] } else { s.heads[bi] });
        }
        Some(s)
    };

    // Bake each instance into model space and bind it.
    let mut model_space: Vec<Affine> = Vec::new();
    if let Some(s) = &skeleton {
        s.model_space(&s.rest_pose(), &mut model_space);
    }
    let mut mesh = MeshData::default();
    let mut ibm_cache: BTreeMap<usize, Vec<Affine>> = BTreeMap::new();
    let mut mismatched = 0usize;
    for mut inst in raw_insts {
        let path = format!("nodes[{}]", inst.node);
        match (inst.skin, &skeleton) {
            (Some(si), Some(_)) => {
                let skin = &doc.skins[si];
                if let std::collections::btree_map::Entry::Vacant(e) = ibm_cache.entry(si) {
                    let mut list = vec![Affine::IDENTITY; skin.joints.len()];
                    if let Some(a) = skin.inverse_bind {
                        let p = format!("skins[{si}].inverseBindMatrices");
                        expect_type(doc, a, &p, &[ElemType::Mat4])?;
                        if doc.accessors[a].count < skin.joints.len() {
                            return Err(gerr(C::InvalidData, &p, format!("{} matrices for {} joints", doc.accessors[a].count, skin.joints.len())));
                        }
                        let m = floats(doc, bufs, a, &p)?;
                        for (k, slot) in list.iter_mut().enumerate() {
                            let c = &m[16 * k..16 * k + 16];
                            if c[3] != 0.0 || c[7] != 0.0 || c[11] != 0.0 || (c[15] - 1.0).abs() > 1e-6 {
                                return Err(gerr(C::InvalidData, &p, format!("matrix {k} is not affine (last row {:?})", [c[3], c[7], c[11], c[15]])));
                            }
                            *slot = Affine { cols: [[c[0], c[1], c[2]], [c[4], c[5], c[6]], [c[8], c[9], c[10]]], t: [c[12], c[13], c[14]] };
                        }
                    }
                    e.insert(list);
                }
                let ibm = &ibm_cache[&si];
                // Bind-to-rest matrix of each joint: rest model transform · inverse bind.
                let c: Vec<Option<Affine>> = skin.joints.iter().zip(ibm).map(|(&j, ib)| bone_of[j].map(|b| model_space[b].then_after(ib))).collect();
                let identity = c.iter().flatten().all(|m| max_abs_diff(m, &Affine::IDENTITY) <= 1e-5 * affine_scale(m));
                let mut joints = Vec::with_capacity(inst.infl.len());
                let mut weights = Vec::with_capacity(inst.infl.len());
                let mut all_det_neg = !identity;
                for (v, infl) in inst.infl.iter().enumerate() {
                    if infl[0].0 == u32::MAX {
                        joints.push([0u8; 4]);
                        weights.push([255u8, 0, 0, 0]);
                        all_det_neg = false;
                        continue;
                    }
                    let q = quantize([infl[0].1, infl[1].1, infl[2].1, infl[3].1]);
                    let mut jb = [0u8; 4];
                    for k in 0..4 {
                        if q[k] > 0 {
                            let b = bone_of[skin.joints[infl[k].0 as usize]].unwrap_or(0);
                            jb[k] = b as u8;
                        }
                    }
                    joints.push(jb);
                    weights.push(q);
                    if identity {
                        continue;
                    }
                    let ms: Vec<(Affine, f32)> = infl.iter().filter(|x| x.1 > 0.0).map(|&(j, wt)| (c[j as usize].unwrap_or(Affine::IDENTITY), wt)).collect();
                    let first = ms[0].0;
                    let uniform = ms.iter().all(|(m, _)| max_abs_diff(m, &first) <= 1e-4 * affine_scale(&first));
                    let m = if uniform {
                        first
                    } else {
                        mismatched += 1;
                        let mut acc = Affine { cols: [[0.0; 3]; 3], t: [0.0; 3] };
                        for (mm, wt) in &ms {
                            for k in 0..3 {
                                acc.cols[k] = math3::add(acc.cols[k], math3::scale(mm.cols[k], *wt));
                            }
                            acc.t = math3::add(acc.t, math3::scale(mm.t, *wt));
                        }
                        acc
                    };
                    all_det_neg &= m.determinant() < 0.0;
                    inst.mesh.positions[v] = m.apply_point(inst.mesh.positions[v]);
                    inst.mesh.normals[v] = m.apply_normal(inst.mesh.normals[v]);
                }
                if all_det_neg && !inst.mesh.positions.is_empty() {
                    for t in inst.mesh.indices.chunks_exact_mut(3) {
                        t.swap(1, 2);
                    }
                }
                inst.mesh.joints = joints;
                inst.mesh.weights = weights;
            }
            _ => {
                // An exact identity is skipped so coordinates keep their bits (and -0.0 signs).
                if world[inst.node] != Affine::IDENTITY {
                    inst.mesh.transform(&world[inst.node]);
                }
                if skeleton.is_some() {
                    let b = nearest_bone(inst.node, &is_bone).map_or(0, |b| bone_of[b].unwrap_or(0));
                    inst.mesh.joints = vec![[b as u8, 0, 0, 0]; inst.mesh.positions.len()];
                    inst.mesh.weights = vec![[255, 0, 0, 0]; inst.mesh.positions.len()];
                }
            }
        }
        // Degenerate transforms (zero scale) leave non-finite normals: give them a valid one.
        for nrm in &mut inst.mesh.normals {
            if nrm.iter().any(|v| !v.is_finite()) || (math3::length(*nrm) - 1.0).abs() > 1e-3 {
                let l = math3::length(*nrm);
                *nrm = if l.is_finite() && l > 1e-12 { math3::scale(*nrm, 1.0 / l) } else { [0.0, 1.0, 0.0] };
            }
        }
        let dropped = drop_degenerate(&mut inst.mesh);
        if dropped > 0 {
            stats.degenerate_triangles += dropped;
            warn(w, "degenerate_triangles", path, format!("{dropped} degenerate triangles dropped"));
        }
        mesh.merge(&inst.mesh);
        if mesh.positions.len() > lim.max_vertices || mesh.triangle_count() > lim.max_triangles {
            return Err(gerr(C::Limit, "meshes", format!("the scene bakes to more than {} vertices or {} triangles", lim.max_vertices, lim.max_triangles)));
        }
    }
    if mismatched > 0 {
        warn(w, "bind_pose_mismatch", "skins", format!("{mismatched} vertices have joints whose inverse bind matrices disagree with the node rest pose; they were baked with blended matrices and may deviate when animated"));
    }
    mesh.group_by_material();
    if mesh.triangle_count() == 0 {
        warn(w, "no_geometry", "meshes", "the scene has no triangles");
    }
    mesh.validate().map_err(|e| gerr(C::InvalidData, "meshes", format!("imported mesh failed validation: {e}")))?;

    // Materials in part order.
    let mut materials = Vec::new();
    let mut presets = Vec::new();
    for p in &mesh.parts {
        if let Some(m) = mats.list.iter().find(|m| m.name == p.material) {
            if !presets.iter().any(|(n, _): &(String, MaterialPreset)| n == &m.name) {
                presets.push((m.name.clone(), m.preset.clone()));
                materials.push(m.clone());
            }
        }
    }

    // Clips.
    let mut clips = Vec::new();
    if let Some(s) = &skeleton {
        let mut names = BTreeSet::new();
        let mut keys_total = [0usize; 2];
        for ai in 0..doc.animations.len() {
            if let Some(c) = build_clip(doc, bufs, ai, s, &bone_of, &in_scene, &mut names, &mut keys_total, opts, w)? {
                clips.push(c);
            }
        }
    } else if !doc.animations.is_empty() {
        warn(w, "animations_ignored", "animations", "no animated node is in the imported scene");
    }

    let rig = skeleton.as_ref().map(|s| Rig {
        bones: (0..s.len()).map(|i| Bone { name: s.names[i].clone(), parent: s.parents[i].map(|p| p as usize), head: s.heads[i], tail: s.tails[i] }).collect(),
    });
    if let Some(r) = &rig {
        r.validate().map_err(|e| gerr(C::InvalidData, "skins", format!("rig: {e}")))?;
    }
    let (collider, note) = collider::fit(opts.collider, &mesh.positions);
    let mut notes: Vec<String> = note.into_iter().collect();
    notes.extend(w.iter().take(64).map(|x| x.to_string()));
    let mut h = StableHasher::new();
    h.write_u64(mesh.content_hash());
    match &collider {
        Collider::None => h.write_u8(0),
        Collider::Aabb { min, max } => {
            h.write_u8(1);
            min.iter().chain(max).for_each(|v| h.write_f32(*v));
        }
        Collider::Capsule { a, b, radius } => {
            h.write_u8(2);
            a.iter().chain(b).for_each(|v| h.write_f32(*v));
            h.write_f32(*radius);
        }
        Collider::ConvexHull { points, indices } => {
            h.write_u8(3);
            points.iter().flatten().for_each(|v| h.write_f32(*v));
            indices.iter().for_each(|i| h.write_u32(*i));
        }
    }
    if let Some(r) = &rig {
        h.write_u8(1);
        r.hash_into(&mut h);
    }
    let name = opts.name.clone().or(scene_name).unwrap_or_else(|| "gltf".to_string());
    let model = Model {
        name,
        bounds: mesh.bounds(),
        lods: vec![mesh],
        lod_ratios: vec![1.0],
        collider,
        materials: presets,
        hash: h.finish(),
        params: Vec::new(),
        seed: 0,
        budget: (None, None),
        notes,
        rig,
    };
    Ok(Import { model, skeleton, clips, materials, textures, images, warnings, stats, generator: doc.generator.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_quantize_to_255() {
        for w in [[1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [0.333, 0.333, 0.334, 0.0], [0.25; 4], [0.7, 0.2, 0.06, 0.04]] {
            let q = quantize(w);
            assert_eq!(q.iter().map(|&x| x as u32).sum::<u32>(), 255, "{w:?} -> {q:?}");
        }
        assert_eq!(quantize([0.5, 0.5, 0.0, 0.0]), [128, 127, 0, 0]);
        let t = top4(&[1, 2, 3, 4, 5, 6, 7, 8], &[0.1, 0.0, 0.3, 0.05, 0.2, 0.15, 0.1, 0.1]).unwrap();
        assert_eq!(t.map(|x| x.0), [3, 5, 6, 1]);
        // Four or fewer keep their slots (minus zeros).
        let t = top4(&[7, 2, 9, 4], &[0.25, 0.0, 0.75, 0.0]).unwrap();
        assert_eq!(t, [(7, 0.25), (9, 0.75), (0, 0.0), (0, 0.0)]);
        assert!(top4(&[0, 1, 2, 3], &[0.0; 4]).is_none());
    }

    #[test]
    fn strips_and_fans() {
        assert_eq!(triangles(5, &[0, 1, 2, 3], "").unwrap(), vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(triangles(6, &[0, 1, 2, 3], "").unwrap(), vec![1, 2, 0, 2, 3, 0]);
        assert_eq!(triangles(1, &[0, 1], "").unwrap_err().code, C::Unsupported);
        assert_eq!(triangles(4, &[0, 1], "").unwrap_err().code, C::InvalidData);
    }

    #[test]
    fn matrix_decomposes() {
        let tr = Transform { t: [1.0, 2.0, 3.0], r: specula_anim::math::qeuler([10.0, 20.0, 30.0]), s: [2.0, 1.0, 0.5] };
        let a = tr.to_affine();
        let m = [a.cols[0][0], a.cols[0][1], a.cols[0][2], 0.0, a.cols[1][0], a.cols[1][1], a.cols[1][2], 0.0, a.cols[2][0], a.cols[2][1], a.cols[2][2], 0.0, a.t[0], a.t[1], a.t[2], 1.0];
        let (back, exact) = local_trs(&Local::Matrix(m));
        assert!(exact);
        assert!(max_abs_diff(&back.to_affine(), &a) < 1e-5);
        let sheared = [1.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(!local_trs(&Local::Matrix(sheared)).1);
    }

    #[test]
    fn slerp_midpoint() {
        let a = QID;
        let b = specula_anim::math::qaxis([0.0, 1.0, 0.0], 120.0);
        let m = slerp(a, b, 0.5);
        let want = specula_anim::math::qaxis([0.0, 1.0, 0.0], 60.0);
        assert!(qdot(m, want) > 1.0 - 1e-6);
    }
}

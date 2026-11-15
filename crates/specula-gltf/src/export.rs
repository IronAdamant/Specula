//! Specula model (+ skeleton + clips) → glTF 2.0 (`.glb` or `.gltf` + `.bin` / data URI).
//!
//! Layout: one mesh with one primitive per material part, all sharing the vertex accessors
//! (POSITION, NORMAL, COLOR_0 as linear floats, TEXCOORD_0 when present, JOINTS_0/WEIGHTS_0 as
//! unsigned bytes); one node per bone carrying its rest TRS; one skin over every bone with the
//! skeleton's inverse binds; one animation per clip with absolute values (`rest ∘ clip value`).
//! Specula specifics travel in `extras.specula`: material presets, bone tails, clip loop modes,
//! durations, distances, events and curves.
//!
//! A part whose material has a baked texture set (`model.extras.textures`) gets that set as
//! PNG images in the file: base colour (its alpha is the cut-out's coverage: `alphaMode` MASK
//! with the set's cutoff), normal, metallic-roughness and, when the preset emits, emissive.
//! Such a part's vertices are written white: the engine draws textured parts from the texture
//! alone, and glTF multiplies `COLOR_0` into the base colour.
//!
//! A vertex colour's alpha is the wind's weight in Specula, never opacity. glTF reads
//! `COLOR_0`'s alpha as opacity, so it is written as 1, and the weights (when any is below 1)
//! go in the custom attribute `_WIND`, which other programs ignore and the importer reads back.

use specula_anim::math::{qdot, qmul, qneg, qnorm, QID};
use specula_anim::{Channel, Clip, Interp, Skeleton};
use specula_core::{base64, Json};
use specula_model::{MaterialKind, MaterialPreset, Model};

use crate::color::srgb8_to_linear;
use crate::doc::{CHUNK_BIN, CHUNK_JSON, GLB_MAGIC};
use crate::error::{gerr, GltfError, GltfErrorCode as C};

/// What to export.
#[derive(Clone, Copy, Debug)]
pub struct ExportInput<'a> {
    pub model: &'a Model,
    /// Which LOD to write.
    pub lod: usize,
    /// The skeleton of a rigged model; `None` derives it from `model.rig` (recipe rigs).
    pub skeleton: Option<&'a Skeleton>,
    pub clips: &'a [Clip],
}

impl<'a> ExportInput<'a> {
    pub fn new(model: &'a Model) -> ExportInput<'a> {
        ExportInput { model, lod: 0, skeleton: None, clips: &[] }
    }
}

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
/// The vertex attribute that carries the wind's weights (0..1), when any is below 1.
pub const WIND_ATTRIBUTE: &str = "_WIND";

/// A material's textures in the file: texture indices.
#[derive(Clone, Copy)]
struct TextureSlots {
    base_color: usize,
    normal: Option<usize>,
    metallic_roughness: usize,
    emissive: Option<usize>,
    alpha_cutoff: f32,
}

#[derive(Default)]
struct Out {
    bin: Vec<u8>,
    views: Vec<Json>,
    accessors: Vec<Json>,
}

impl Out {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut v = vec![("buffer", Json::from(0u32)), ("byteOffset", Json::from(self.bin.len())), ("byteLength", Json::from(bytes.len()))];
        if let Some(t) = target {
            v.push(("target", Json::from(t)));
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(Json::obj(v));
        self.views.len() - 1
    }

    #[allow(clippy::too_many_arguments)]
    fn accessor(&mut self, view: usize, offset: usize, ctype: u32, count: usize, ty: &str, normalized: bool, minmax: Option<(Vec<f32>, Vec<f32>)>) -> usize {
        let mut a = vec![("bufferView", Json::from(view))];
        if offset > 0 {
            a.push(("byteOffset", Json::from(offset)));
        }
        a.push(("componentType", Json::from(ctype)));
        if normalized {
            a.push(("normalized", Json::Bool(true)));
        }
        a.push(("count", Json::from(count)));
        a.push(("type", Json::Str(ty.into())));
        if let Some((lo, hi)) = minmax {
            a.push(("min", Json::arr(lo.into_iter().map(Json::from))));
            a.push(("max", Json::arr(hi.into_iter().map(Json::from))));
        }
        self.accessors.push(Json::obj(a));
        self.accessors.len() - 1
    }

    /// A tightly packed f32 accessor of `dims`-vectors (with min/max when asked).
    fn floats(&mut self, data: &[f32], dims: usize, ty: &str, target: Option<u32>, minmax: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, target);
        let mm = minmax.then(|| {
            let mut lo = vec![f32::MAX; dims];
            let mut hi = vec![f32::MIN; dims];
            for e in data.chunks_exact(dims) {
                for k in 0..dims {
                    lo[k] = lo[k].min(e[k]);
                    hi[k] = hi[k].max(e[k]);
                }
            }
            (lo, hi)
        });
        self.accessor(view, 0, 5126, data.len() / dims, ty, false, mm)
    }
}

fn hex(c: [f32; 4]) -> String {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]), b(c[3]))
}

fn material_json(name: &str, p: &MaterialPreset, tex: Option<TextureSlots>, ext_strength: &mut bool) -> Json {
    let lin = |x: f32| srgb8_to_linear((x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    let glass = p.kind == MaterialKind::Glass;
    let slot = |index: usize| Json::obj([("index", Json::from(index))]);
    // Vertex colours carry the colour; the factor only carries glass opacity. With a baked
    // texture set the textures carry everything, as the engine draws it: factors of 1.
    let mut pbr = match tex {
        Some(t) => vec![("baseColorTexture", slot(t.base_color)), ("metallicFactor", Json::from(1.0f32)), ("roughnessFactor", Json::from(1.0f32)), ("metallicRoughnessTexture", slot(t.metallic_roughness))],
        None => vec![("metallicFactor", Json::from(p.metallic)), ("roughnessFactor", Json::from(p.roughness))],
    };
    if glass {
        pbr.insert(0, ("baseColorFactor", Json::arr([1.0f32, 1.0, 1.0, p.opacity].map(Json::from))));
    }
    let mut m = vec![("name", Json::Str(name.into())), ("pbrMetallicRoughness", Json::obj(pbr))];
    if let Some(t) = tex {
        if let Some(n) = t.normal {
            m.push(("normalTexture", slot(n)));
        }
        if let Some(e) = t.emissive.filter(|_| p.emissive > 0.0) {
            m.push(("emissiveTexture", slot(e)));
        }
        // A cut-out (foliage cards): the base colour's alpha is coverage.
        if t.alpha_cutoff > 0.0 && !glass {
            m.push(("alphaMode", Json::Str("MASK".into())));
            m.push(("alphaCutoff", Json::from(t.alpha_cutoff)));
        }
    }
    if p.emissive > 0.0 {
        let e = [lin(p.color[0]) * p.emissive, lin(p.color[1]) * p.emissive, lin(p.color[2]) * p.emissive];
        let peak = e[0].max(e[1]).max(e[2]);
        if peak > 1.0 {
            m.push(("emissiveFactor", Json::arr(e.map(|x| Json::from(x / peak)))));
            m.push(("extensions", Json::obj([("KHR_materials_emissive_strength", Json::obj([("emissiveStrength", Json::from(peak))]))])));
            *ext_strength = true;
        } else if peak > 0.0 {
            m.push(("emissiveFactor", Json::arr(e.map(Json::from))));
        }
    }
    if glass {
        m.push(("alphaMode", Json::Str("BLEND".into())));
    }
    let kind = if glass { "glass" } else { "standard" };
    m.push((
        "extras",
        Json::obj([(
            "specula",
            Json::obj([
                ("preset", Json::Str(name.into())),
                ("kind", Json::Str(kind.into())),
                ("color", Json::Str(hex(p.color))),
                ("roughness", Json::from(p.roughness)),
                ("metallic", Json::from(p.metallic)),
                ("emissive", Json::from(p.emissive)),
                ("specular", Json::from(p.specular)),
                ("opacity", Json::from(p.opacity)),
            ]),
        )]),
    ));
    Json::obj(m)
}

/// The glTF JSON and binary buffer (without the buffer's `uri`).
fn build(input: &ExportInput) -> Result<(Json, Vec<u8>), GltfError> {
    let model = input.model;
    let mesh = model.lods.get(input.lod).ok_or_else(|| gerr(C::BadReference, "lod", format!("LOD {} doesn't exist ({} LODs)", input.lod, model.lods.len())))?;
    mesh.validate().map_err(|e| gerr(C::InvalidData, "mesh", format!("the mesh is invalid: {e}")))?;
    if mesh.triangle_count() == 0 {
        // glTF needs at least one primitive per mesh and no empty buffer views.
        return Err(gerr(C::InvalidData, "mesh", "the mesh has no triangles; there is nothing to export"));
    }
    let derived;
    let skel = match (input.skeleton, &model.rig) {
        (Some(s), _) => Some(s),
        (None, Some(rig)) => {
            derived = Skeleton::from_rig(rig).map_err(|e| gerr(C::InvalidData, "rig", e.to_string()))?;
            Some(&derived)
        }
        (None, None) => None,
    };
    if mesh.is_skinned() && skel.is_none() {
        return Err(gerr(C::MissingField, "skeleton", "the mesh is skinned but the model has no rig and no skeleton was given"));
    }
    if let Some(s) = skel {
        if let Some(bad) = mesh.joints.iter().flatten().find(|&&j| j as usize >= s.len()) {
            return Err(gerr(C::InvalidData, "mesh.joints", format!("joint {bad} but the skeleton has {} bones", s.len())));
        }
    }
    let mut out = Out::default();
    let n = mesh.vertex_count();

    // Vertex attributes, shared by every primitive.
    let mut attrs = Vec::new();
    let pos: Vec<f32> = mesh.positions.iter().flatten().copied().collect();
    attrs.push(("POSITION", Json::from(out.floats(&pos, 3, "VEC3", Some(ARRAY_BUFFER), true))));
    let nrm: Vec<f32> = mesh.normals.iter().flatten().copied().collect();
    attrs.push(("NORMAL", Json::from(out.floats(&nrm, 3, "VEC3", Some(ARRAY_BUFFER), false))));
    if !mesh.uvs.is_empty() {
        let uv: Vec<f32> = mesh.uvs.iter().flatten().copied().collect();
        attrs.push(("TEXCOORD_0", Json::from(out.floats(&uv, 2, "VEC2", Some(ARRAY_BUFFER), false))));
    }
    // Tangents only when every vertex has one (glTF requires unit tangents throughout).
    if mesh.has_tangents() {
        let t: Vec<f32> = mesh.tangents.iter().flatten().copied().collect();
        attrs.push(("TANGENT", Json::from(out.floats(&t, 4, "VEC4", Some(ARRAY_BUFFER), false))));
    }
    // Vertices of parts drawn from a baked texture set are white (the texture is the colour).
    let set_of = |material: &str| model.extras.textures.iter().find(|(key, _)| key == material).map(|(_, set)| set);
    let mut textured = vec![false; n];
    for p in mesh.parts.iter().filter(|p| set_of(&p.material).is_some()) {
        for &i in &mesh.indices[p.first_index as usize..(p.first_index + p.index_count) as usize] {
            textured[i as usize] = true;
        }
    }
    // Alpha is the wind's weight, never opacity: 1 in COLOR_0, the weights in their own attribute.
    let col: Vec<f32> = mesh.colors.iter().zip(&textured).flat_map(|(c, white)| if *white { [1.0; 4] } else { [srgb8_to_linear(c[0]), srgb8_to_linear(c[1]), srgb8_to_linear(c[2]), 1.0] }).collect();
    attrs.push(("COLOR_0", Json::from(out.floats(&col, 4, "VEC4", Some(ARRAY_BUFFER), false))));
    if mesh.colors.iter().any(|c| c[3] != 255) {
        let wind: Vec<f32> = mesh.colors.iter().map(|c| c[3] as f32 / 255.0).collect();
        attrs.push((WIND_ATTRIBUTE, Json::from(out.floats(&wind, 1, "SCALAR", Some(ARRAY_BUFFER), false))));
    }
    if mesh.is_skinned() {
        let j: Vec<u8> = mesh.joints.iter().flatten().copied().collect();
        let v = out.view(&j, Some(ARRAY_BUFFER));
        attrs.push(("JOINTS_0", Json::from(out.accessor(v, 0, 5121, n, "VEC4", false, None))));
        let w: Vec<u8> = mesh.weights.iter().flatten().copied().collect();
        let v = out.view(&w, Some(ARRAY_BUFFER));
        attrs.push(("WEIGHTS_0", Json::from(out.accessor(v, 0, 5121, n, "VEC4", true, None))));
    }
    let attributes = Json::obj(attrs);

    // Indices: one view, one accessor per part.
    let wide = n >= 65535;
    let ibytes: Vec<u8> = if wide { mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect() } else { mesh.indices.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect() };
    let iview = out.view(&ibytes, Some(ELEMENT_ARRAY_BUFFER));
    let isz = if wide { 4 } else { 2 };
    let mut materials: Vec<Json> = Vec::new();
    let mut mat_names: Vec<&str> = Vec::new();
    let mut ext_strength = false;
    let mut primitives = Vec::new();
    // Baked texture sets as PNG images, each set once (several materials may share one).
    let mut images: Vec<Json> = Vec::new();
    let mut textures: Vec<Json> = Vec::new();
    let mut sets: Vec<(u64, TextureSlots)> = Vec::new();
    // (A set's emissive image is written only when a preset that uses the set emits: the
    // engine draws no emission otherwise, and an image no material names is dead weight.)
    let emits = |set: &specula_model::texgraph::TextureSet| {
        mesh.parts.iter().any(|p| set_of(&p.material).is_some_and(|s| s.hash == set.hash) && model.materials.iter().any(|(name, preset)| name == &p.material && preset.emissive > 0.0))
    };
    let mut slots_of = |out: &mut Out, set: &specula_model::texgraph::TextureSet| -> TextureSlots {
        if let Some((_, slots)) = sets.iter().find(|(hash, _)| *hash == set.hash) {
            return *slots;
        }
        let mut image = |out: &mut Out, rgba: &[u8], what: &str| -> usize {
            let png = specula_core::png::encode_rgba8(set.size, set.size, rgba, specula_core::deflate::Level::Default);
            let view = out.view(&png, None);
            images.push(Json::obj([("name", Json::Str(format!("{}_{what}", set.name))), ("mimeType", Json::Str("image/png".into())), ("bufferView", Json::from(view))]));
            textures.push(Json::obj([("sampler", Json::from(0u32)), ("source", Json::from(images.len() - 1))]));
            textures.len() - 1
        };
        let slots = TextureSlots {
            base_color: image(out, &set.base_color, "base_color"),
            normal: set.normal.as_deref().map(|n| image(out, n, "normal")),
            metallic_roughness: image(out, &set.metallic_roughness, "metallic_roughness"),
            emissive: set.emissive.as_deref().filter(|_| emits(set)).map(|e| image(out, e, "emissive")),
            alpha_cutoff: set.alpha_cutoff,
        };
        sets.push((set.hash, slots));
        slots
    };
    for p in &mesh.parts {
        let mi = match mat_names.iter().position(|m| *m == p.material) {
            Some(i) => i,
            None => {
                let preset = model.materials.iter().find(|(name, _)| name == &p.material).map(|(_, p)| p).ok_or_else(|| gerr(C::MissingField, "materials", format!("no preset for part material '{}'", p.material)))?;
                let tex = set_of(&p.material).map(|set| slots_of(&mut out, set));
                materials.push(material_json(&p.material, preset, tex, &mut ext_strength));
                mat_names.push(&p.material);
                mat_names.len() - 1
            }
        };
        let acc = out.accessor(iview, p.first_index as usize * isz, if wide { 5125 } else { 5123 }, p.index_count as usize, "SCALAR", false, None);
        primitives.push(Json::obj([("attributes", attributes.clone()), ("indices", Json::from(acc)), ("material", Json::from(mi)), ("mode", Json::from(4u32))]));
    }

    // Nodes: the mesh node, then one node per bone.
    let mut mesh_node = vec![("name", Json::Str(model.name.clone())), ("mesh", Json::from(0u32))];
    let mut nodes = Vec::new();
    let mut scene_nodes = vec![Json::from(0u32)];
    let mut skins = Vec::new();
    let mut animations = Vec::new();
    if let Some(s) = skel {
        if mesh.is_skinned() {
            mesh_node.push(("skin", Json::from(0u32)));
        }
        nodes.push(Json::obj(mesh_node));
        for i in 0..s.len() {
            let r = &s.rest[i];
            let mut b = vec![("name", Json::Str(s.names[i].clone()))];
            let kids: Vec<Json> = (0..s.len()).filter(|&c| s.parents[c] == Some(i as u16)).map(|c| Json::from(c + 1)).collect();
            if !kids.is_empty() {
                b.push(("children", Json::Arr(kids)));
            }
            if r.t != [0.0; 3] {
                b.push(("translation", Json::arr(r.t.map(Json::from))));
            }
            if r.r != QID {
                b.push(("rotation", Json::arr(r.r.map(Json::from))));
            }
            if r.s != [1.0; 3] {
                b.push(("scale", Json::arr(r.s.map(Json::from))));
            }
            b.push(("extras", Json::obj([("specula", Json::obj([("tail", Json::arr(s.tails[i].map(Json::from)))]))])));
            nodes.push(Json::obj(b));
        }
        scene_nodes.push(Json::from(1u32));
        let ibm: Vec<f32> = s.inverse_bind.iter().flat_map(|m| m.to_cols_array()).collect();
        let ibm_acc = out.floats(&ibm, 16, "MAT4", None, false);
        skins.push(Json::obj([("inverseBindMatrices", Json::from(ibm_acc)), ("skeleton", Json::from(1u32)), ("joints", Json::arr((0..s.len()).map(|i| Json::from(i + 1))))]));
        // A clip without tracks (a neutral additive-layer base) has no channels to write: glTF
        // animations need at least one.
        for (ci, clip) in input.clips.iter().enumerate().filter(|(_, c)| !c.tracks.is_empty()) {
            animations.push(animation_json(&mut out, s, clip, ci)?);
        }
    } else {
        nodes.push(Json::obj(mesh_node));
        if !input.clips.is_empty() {
            return Err(gerr(C::MissingField, "clips", "clips need a skeleton (the model has no rig)"));
        }
    }

    let mut root = vec![("asset", Json::obj([("version", Json::Str("2.0".into())), ("generator", Json::Str("Specula specula-gltf".into()))]))];
    if ext_strength {
        root.push(("extensionsUsed", Json::arr([Json::Str("KHR_materials_emissive_strength".into())])));
    }
    root.push(("scene", Json::from(0u32)));
    root.push(("scenes", Json::arr([Json::obj([("name", Json::Str(model.name.clone())), ("nodes", Json::Arr(scene_nodes))])])));
    root.push(("nodes", Json::Arr(nodes)));
    let mut mesh_json = vec![("name", Json::Str(model.name.clone())), ("primitives", Json::Arr(primitives))];
    // The wind term (foliage): this engine's own, kept for the way back (other programs
    // ignore extras).
    if let Some(w) = model.wind {
        let wind = Json::obj([("amount", Json::from(w.amount)), ("height", Json::from(w.height)), ("painted", Json::Bool(w.painted))]);
        mesh_json.push(("extras", Json::obj([("specula", Json::obj([("wind", wind)]))])));
    }
    root.push(("meshes", Json::arr([Json::obj(mesh_json)])));
    root.push(("materials", Json::Arr(materials)));
    if !images.is_empty() {
        // (Trilinear, repeating: how the engine samples a baked set.)
        root.push(("samplers", Json::arr([Json::obj([("magFilter", Json::from(9729u32)), ("minFilter", Json::from(9987u32)), ("wrapS", Json::from(10497u32)), ("wrapT", Json::from(10497u32))])])));
        root.push(("images", Json::Arr(images)));
        root.push(("textures", Json::Arr(textures)));
    }
    if !skins.is_empty() {
        root.push(("skins", Json::Arr(skins)));
    }
    if !animations.is_empty() {
        root.push(("animations", Json::Arr(animations)));
    }
    root.push(("accessors", Json::Arr(std::mem::take(&mut out.accessors))));
    root.push(("bufferViews", Json::Arr(std::mem::take(&mut out.views))));
    while out.bin.len() % 4 != 0 {
        out.bin.push(0);
    }
    Ok((Json::obj(root), out.bin))
}

/// Catmull–Rom tangent of key `k` (as `specula_anim::Keys::sample` computes it).
fn cr(times: &[f32], v: &[[f32; 4]], k: usize, wrap: Option<f32>) -> [f32; 4] {
    let n = times.len();
    let closed = wrap.filter(|d| n >= 3 && times[0] == 0.0 && times[n - 1] == *d);
    let (tp, vp) = if k >= 1 {
        (times[k - 1], v[k - 1])
    } else if let Some(d) = closed {
        (times[n - 2] - d, v[n - 2])
    } else {
        (times[k], v[k])
    };
    let (tn, vn) = if k + 1 < n {
        (times[k + 1], v[k + 1])
    } else if let Some(d) = closed {
        (times[1] + d, v[1])
    } else {
        (times[k], v[k])
    };
    let inv = 1.0 / (tn - tp);
    [(vn[0] - vp[0]) * inv, (vn[1] - vp[1]) * inv, (vn[2] - vp[2]) * inv, (vn[3] - vp[3]) * inv]
}

fn animation_json(out: &mut Out, s: &Skeleton, clip: &Clip, ci: usize) -> Result<Json, GltfError> {
    let mut channels = Vec::new();
    let mut samplers = Vec::new();
    let wrap = clip.wrap();
    for (ti, t) in clip.tracks.iter().enumerate() {
        let b = s.find(&t.bone).ok_or_else(|| gerr(C::BadReference, &format!("clips[{ci}].tracks[{ti}].bone"), format!("no bone '{}' in the skeleton", t.bone)))?;
        let rest = s.rest[b];
        let rotation = t.channel == Channel::Rotation;
        // Rest-relative → absolute; rotations made hemisphere-continuous (same rotations).
        let mut abs: Vec<[f32; 4]> = Vec::with_capacity(t.keys.values.len());
        for v in &t.keys.values {
            let a = match t.channel {
                Channel::Translation => [rest.t[0] + v[0], rest.t[1] + v[1], rest.t[2] + v[2], 0.0],
                Channel::Rotation => {
                    let q = if rest.r == QID { *v } else { qmul(rest.r, *v) };
                    // glTF validators want unit quaternions (Specula accepts |q|² within 1e-3).
                    let q = if (qdot(q, q) - 1.0).abs() > 1e-6 { qnorm(q) } else { q };
                    match abs.last() {
                        Some(p) if qdot(*p, q) < 0.0 => qneg(q),
                        _ => q,
                    }
                }
                Channel::Scale => [rest.s[0] * v[0], rest.s[1] * v[1], rest.s[2] * v[2], 0.0],
            };
            abs.push(a);
        }
        let dims = if rotation { 4 } else { 3 };
        let times = &t.keys.times;
        let cubic = t.keys.interp == Interp::Cubic && times.len() >= 2;
        let mut data: Vec<f32> = Vec::with_capacity(abs.len() * dims * if cubic { 3 } else { 1 });
        for (k, a) in abs.iter().enumerate() {
            if cubic {
                let m = cr(times, &abs, k, wrap);
                data.extend_from_slice(&m[..dims]);
                data.extend_from_slice(&a[..dims]);
                data.extend_from_slice(&m[..dims]);
            } else {
                data.extend_from_slice(&a[..dims]);
            }
        }
        let input = out.floats(times, 1, "SCALAR", None, true);
        let output = out.floats(&data, dims, if rotation { "VEC4" } else { "VEC3" }, None, false);
        let interp = match t.keys.interp {
            Interp::Step => "STEP",
            Interp::Cubic if cubic => "CUBICSPLINE",
            _ => "LINEAR",
        };
        samplers.push(Json::obj([("input", Json::from(input)), ("output", Json::from(output)), ("interpolation", Json::Str(interp.into()))]));
        channels.push(Json::obj([("sampler", Json::from(samplers.len() - 1)), ("target", Json::obj([("node", Json::from(b + 1)), ("path", Json::Str(t.channel.name().into()))]))]));
    }
    let full = clip.to_json();
    let mut extras = vec![("loop", Json::Str(clip.looping.name().into())), ("duration", Json::from(clip.duration)), ("distance", Json::from(clip.distance))];
    for k in ["events", "curves"] {
        if let Some(v) = full.get(k).filter(|v| v.as_array().is_some_and(|a| !a.is_empty())) {
            extras.push((k, v.clone()));
        }
    }
    Ok(Json::obj([
        ("name", Json::Str(clip.name.clone())),
        ("channels", Json::Arr(channels)),
        ("samplers", Json::Arr(samplers)),
        ("extras", Json::obj([("specula", Json::obj(extras))])),
    ]))
}

/// A binary glTF (`.glb`) of the model, its skeleton and clips.
pub fn export_glb(input: &ExportInput) -> Result<Vec<u8>, GltfError> {
    let (mut json, bin) = build(input)?;
    json.set("buffers", Json::arr([Json::obj([("byteLength", Json::from(bin.len()))])]));
    let mut text = json.to_string().into_bytes();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    let total32 = u32::try_from(total).map_err(|_| gerr(C::Limit, "", format!("{total} bytes is too large for a GLB")))?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total32.to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&text);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

/// A `.gltf` JSON text and its binary buffer. With `bin_uri` the JSON refers to that file
/// (write the bytes next to it); without, the buffer is embedded as a base64 data URI and the
/// returned bytes are empty.
pub fn export_gltf(input: &ExportInput, bin_uri: Option<&str>) -> Result<(String, Vec<u8>), GltfError> {
    let (mut json, bin) = build(input)?;
    let uri = match bin_uri {
        Some(u) => u.to_string(),
        None => format!("data:application/octet-stream;base64,{}", base64::encode(&bin)),
    };
    json.set("buffers", Json::arr([Json::obj([("byteLength", Json::from(bin.len())), ("uri", Json::Str(uri))])]));
    Ok((json.to_pretty(), if bin_uri.is_some() { bin } else { Vec::new() }))
}

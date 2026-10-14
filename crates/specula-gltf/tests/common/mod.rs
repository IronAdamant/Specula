//! A tiny glTF writer for hand-built test files (edge cases the exporter never produces).
#![allow(dead_code)]

use specula_core::{base64, Json};

pub const F32: u32 = 5126;
pub const U8: u32 = 5121;
pub const U16: u32 = 5123;
pub const U32: u32 = 5125;
pub const I16: u32 = 5122;

pub struct Gltf {
    pub root: Json,
    pub bin: Vec<u8>,
}

impl Default for Gltf {
    fn default() -> Self {
        Gltf::new()
    }
}

impl Gltf {
    pub fn new() -> Gltf {
        Gltf { root: Json::obj([("asset", Json::obj([("version", Json::Str("2.0".into()))]))]), bin: Vec::new() }
    }

    /// Appends `v` to the top-level array `key`; returns its index.
    pub fn push(&mut self, key: &str, v: Json) -> usize {
        if self.root.get(key).is_none() {
            self.root.set(key, Json::Arr(Vec::new()));
        }
        let Json::Obj(pairs) = &mut self.root else { unreachable!() };
        let slot = pairs.iter_mut().find(|(k, _)| k == key).unwrap();
        let Json::Arr(a) = &mut slot.1 else { panic!("{key} is not an array") };
        a.push(v);
        a.len() - 1
    }

    pub fn set(&mut self, key: &str, v: Json) {
        self.root.set(key, v);
    }

    /// A buffer view over `bytes` (4-byte aligned) with an optional stride.
    pub fn view(&mut self, bytes: &[u8], stride: Option<usize>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let mut v = vec![("buffer", Json::from(0u32)), ("byteOffset", Json::from(self.bin.len())), ("byteLength", Json::from(bytes.len()))];
        if let Some(s) = stride {
            v.push(("byteStride", Json::from(s)));
        }
        self.bin.extend_from_slice(bytes);
        self.push("bufferViews", Json::obj(v))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn accessor(&mut self, view: Option<usize>, offset: usize, ctype: u32, count: usize, ty: &str, normalized: bool) -> usize {
        let mut a = Vec::new();
        if let Some(v) = view {
            a.push(("bufferView", Json::from(v)));
        }
        if offset > 0 {
            a.push(("byteOffset", Json::from(offset)));
        }
        a.push(("componentType", Json::from(ctype)));
        if normalized {
            a.push(("normalized", Json::Bool(true)));
        }
        a.push(("count", Json::from(count)));
        a.push(("type", Json::Str(ty.into())));
        self.push("accessors", Json::obj(a))
    }

    pub fn floats(&mut self, data: &[f32], ty: &str) -> usize {
        let dims = match ty {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => panic!("{ty}"),
        };
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let v = self.view(&bytes, None);
        self.accessor(Some(v), 0, F32, data.len() / dims, ty, false)
    }

    pub fn u16s(&mut self, data: &[u16]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let v = self.view(&bytes, None);
        self.accessor(Some(v), 0, U16, data.len(), "SCALAR", false)
    }

    /// A mesh with one primitive; returns the mesh index.
    pub fn mesh(&mut self, attributes: &[(&str, usize)], indices: Option<usize>, material: Option<usize>, mode: Option<u32>) -> usize {
        let mut p = vec![("attributes", Json::obj(attributes.iter().map(|(k, a)| (k.to_string(), Json::from(*a)))))];
        if let Some(i) = indices {
            p.push(("indices", Json::from(i)));
        }
        if let Some(m) = material {
            p.push(("material", Json::from(m)));
        }
        if let Some(m) = mode {
            p.push(("mode", Json::from(m)));
        }
        self.push("meshes", Json::obj([("primitives", Json::arr([Json::obj(p)]))]))
    }

    pub fn node(&mut self, fields: Vec<(&str, Json)>) -> usize {
        self.push("nodes", Json::obj(fields))
    }

    pub fn scene(&mut self, nodes: &[usize]) {
        self.push("scenes", Json::obj([("nodes", Json::arr(nodes.iter().map(|n| Json::from(*n))))]));
        self.set("scene", Json::from(0u32));
    }

    fn with_buffer(&self, uri: Option<String>) -> Json {
        let mut root = self.root.clone();
        if !self.bin.is_empty() {
            let mut b = vec![("byteLength", Json::from(self.bin.len()))];
            if let Some(u) = uri {
                b.push(("uri", Json::Str(u)));
            }
            root.set("buffers", Json::arr([Json::obj(b)]));
        }
        root
    }

    pub fn glb(&self) -> Vec<u8> {
        let mut text = self.with_buffer(None).to_string().into_bytes();
        while text.len() % 4 != 0 {
            text.push(b' ');
        }
        let mut bin = self.bin.clone();
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let total = 12 + 8 + text.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&text);
        if !bin.is_empty() {
            out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
            out.extend_from_slice(&bin);
        }
        out
    }

    /// `.gltf` text with the buffer as a data URI.
    pub fn embedded(&self) -> String {
        self.with_buffer(Some(format!("data:application/octet-stream;base64,{}", base64::encode(&self.bin)))).to_string()
    }

    /// `.gltf` text whose buffer is the external file `uri`.
    pub fn external(&self, uri: &str) -> String {
        self.with_buffer(Some(uri.to_string())).to_string()
    }
}

/// A unit quad in the XY plane facing +Z: positions, normals, indices.
pub fn quad(g: &mut Gltf) -> (usize, usize, usize) {
    let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], "VEC3");
    let n = g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], "VEC3");
    let i = g.u16s(&[0, 1, 2, 0, 2, 3]);
    (p, n, i)
}

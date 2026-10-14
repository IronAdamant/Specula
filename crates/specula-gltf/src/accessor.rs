//! Reading accessors: every component type, `normalized`, byte strides, matrix column padding
//! and sparse substitution, with bounds checked against the buffer view before any read.

use crate::doc::{Accessor, ComponentType as T, Document, ElemType};
use crate::error::{gerr, GltfError, GltfErrorCode as C};

/// Bytes of one stored element (matrix columns are 4-byte aligned) and the column layout
/// `(column bytes, columns)`; `columns == 0` for vectors.
fn layout(a: &Accessor) -> (usize, usize, usize) {
    let cs = a.ctype.size();
    match a.ty.columns() {
        0 => (a.ty.dims() * cs, 0, 0),
        n => {
            let col = (n * cs).div_ceil(4) * 4;
            (n * col, col, n)
        }
    }
}

/// Copies element bytes from `src` (stored layout) into `dst` (packed, no column padding).
fn unpad(a: &Accessor, src: &[u8], dst: &mut [u8]) {
    let (_, col, n) = layout(a);
    if n == 0 {
        dst.copy_from_slice(&src[..dst.len()]);
    } else {
        let run = n * a.ctype.size();
        for c in 0..n {
            dst[c * run..(c + 1) * run].copy_from_slice(&src[c * col..c * col + run]);
        }
    }
}

/// The accessor's elements as packed little-endian bytes (`count × dims × component size`),
/// sparse values applied. `path` names the property that referenced the accessor.
fn gather(doc: &Document, bufs: &[Vec<u8>], ai: usize, path: &str) -> Result<Vec<u8>, GltfError> {
    let a = &doc.accessors[ai];
    let cs = a.ctype.size();
    let packed = a.ty.dims() * cs;
    let (elem, _, _) = layout(a);
    let total = a.count.checked_mul(packed).ok_or_else(|| gerr(C::Limit, path, format!("accessor {ai} is too large")))?;
    let mut out = vec![0u8; total];
    let at = |what: &str| format!("{path} (accessor {ai}{what})");
    if let Some(vi) = a.view {
        let v = &doc.views[vi];
        let data = &bufs[v.buffer][v.offset..v.offset + v.length];
        let stride = v.stride.unwrap_or(elem);
        if stride < elem {
            return Err(gerr(C::InvalidData, &at(""), format!("bufferView {vi} byteStride {stride} is smaller than the {elem}-byte element")));
        }
        let need = a.offset as u128 + stride as u128 * (a.count as u128 - 1) + elem as u128;
        if need > v.length as u128 {
            return Err(gerr(C::Bounds, &at(""), format!("{} elements of {elem} bytes (stride {stride}) from byte {} need {need} bytes; bufferView {vi} has {}", a.count, a.offset, v.length))
                .at_offset((v.offset + a.offset) as u64)
                .with_numbers(need as f64, v.length as f64));
        }
        for (i, dst) in out.chunks_exact_mut(packed).enumerate() {
            let s = a.offset + i * stride;
            unpad(a, &data[s..s + elem], dst);
        }
    }
    if let Some(sp) = &a.sparse {
        let iv = &doc.views[sp.idx_view];
        let isz = sp.idx_type.size();
        let need = sp.idx_offset as u128 + (sp.count * isz) as u128;
        if need > iv.length as u128 {
            return Err(gerr(C::Bounds, &at(".sparse.indices"), format!("{} indices need {need} bytes; bufferView {} has {}", sp.count, sp.idx_view, iv.length)).at_offset((iv.offset + sp.idx_offset) as u64));
        }
        let idata = &bufs[iv.buffer][iv.offset + sp.idx_offset..iv.offset + sp.idx_offset + sp.count * isz];
        let vv = &doc.views[sp.val_view];
        let need = sp.val_offset as u128 + (sp.count * elem) as u128;
        if need > vv.length as u128 {
            return Err(gerr(C::Bounds, &at(".sparse.values"), format!("{} values need {need} bytes; bufferView {} has {}", sp.count, sp.val_view, vv.length)).at_offset((vv.offset + sp.val_offset) as u64));
        }
        let vdata = &bufs[vv.buffer][vv.offset + sp.val_offset..vv.offset + sp.val_offset + sp.count * elem];
        let mut prev: Option<usize> = None;
        for k in 0..sp.count {
            let b = &idata[k * isz..(k + 1) * isz];
            let i = match sp.idx_type {
                T::U8 => b[0] as usize,
                T::U16 => u16::from_le_bytes([b[0], b[1]]) as usize,
                _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize,
            };
            if i >= a.count || prev.is_some_and(|p| i <= p) {
                return Err(gerr(C::InvalidData, &at(".sparse.indices"), format!("index {k} is {i}: sparse indices must strictly increase and stay below count {}", a.count)));
            }
            prev = Some(i);
            unpad(a, &vdata[k * elem..(k + 1) * elem], &mut out[i * packed..(i + 1) * packed]);
        }
    }
    Ok(out)
}

/// Checks the accessor's element type against what the property allows.
pub fn expect_type(doc: &Document, ai: usize, path: &str, allowed: &[ElemType]) -> Result<(), GltfError> {
    let ty = doc.accessors[ai].ty;
    if !allowed.contains(&ty) {
        let names: Vec<&str> = allowed.iter().map(|t| t.name()).collect();
        return Err(gerr(C::WrongType, path, format!("accessor {ai} is {}, expected {}", ty.name(), names.join(" or "))));
    }
    Ok(())
}

/// Reads an accessor as floats (`count × dims`): FLOAT as is (must be finite), normalized
/// integers mapped to 0..1 / −1..1 as the spec says, other integers as their value
/// (KHR_mesh_quantization).
pub fn floats(doc: &Document, bufs: &[Vec<u8>], ai: usize, path: &str) -> Result<Vec<f32>, GltfError> {
    let a = &doc.accessors[ai];
    let raw = gather(doc, bufs, ai, path)?;
    let dims = a.ty.dims();
    let mut out = Vec::with_capacity(a.count * dims);
    match (a.ctype, a.normalized) {
        (T::F32, _) => {
            for (k, b) in raw.chunks_exact(4).enumerate() {
                let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                if !v.is_finite() {
                    return Err(gerr(C::InvalidData, &format!("{path} (accessor {ai})"), format!("element {} component {} is {v}", k / dims, k % dims)));
                }
                out.push(v);
            }
        }
        (T::U8, true) => out.extend(raw.iter().map(|&b| b as f32 / 255.0)),
        (T::I8, true) => out.extend(raw.iter().map(|&b| (b as i8 as f32 / 127.0).max(-1.0))),
        (T::U16, true) => out.extend(raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0)),
        (T::I16, true) => out.extend(raw.chunks_exact(2).map(|b| (i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0).max(-1.0))),
        (T::U8, false) => out.extend(raw.iter().map(|&b| b as f32)),
        (T::I8, false) => out.extend(raw.iter().map(|&b| b as i8 as f32)),
        (T::U16, false) => out.extend(raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as f32)),
        (T::I16, false) => out.extend(raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)),
        (T::U32, _) => out.extend(raw.chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32)),
    }
    Ok(out)
}

/// Reads an unsigned-integer accessor (indices, joints): UNSIGNED_BYTE/SHORT/INT, not
/// normalized.
pub fn uints(doc: &Document, bufs: &[Vec<u8>], ai: usize, path: &str) -> Result<Vec<u32>, GltfError> {
    let a = &doc.accessors[ai];
    if a.normalized || !matches!(a.ctype, T::U8 | T::U16 | T::U32) {
        return Err(gerr(C::WrongType, path, format!("accessor {ai} is {}{}, expected unsigned integers", if a.normalized { "normalized " } else { "" }, a.ctype.name())));
    }
    let raw = gather(doc, bufs, ai, path)?;
    Ok(match a.ctype {
        T::U8 => raw.iter().map(|&b| b as u32).collect(),
        T::U16 => raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32).collect(),
        _ => raw.chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect(),
    })
}

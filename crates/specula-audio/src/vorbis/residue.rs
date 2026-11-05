//! Vorbis residues (spec section 8): the spectral detail under the floor, VQ-coded in
//! partitions over up to eight passes. Types 0 and 1 code each channel's vector on its own
//! (interleaved or in order); type 2 codes all channels as one interleaved vector.

use super::bits::Bits;
use super::codebook::Codebook;
use super::header_err;
use crate::error::Result;

/// Most partitions one residue decode may classify (bounds work and scratch memory).
const MAX_PARTITIONS: usize = 1 << 16;

#[derive(Clone, Debug)]
pub(crate) struct Residue {
    pub kind: u8,
    begin: u32,
    end: u32,
    partition_size: u32,
    classifications: u32,
    classbook: u8,
    /// `[class][pass]`: codebook + 1 (0 = none).
    books: Vec<[u16; 8]>,
}

#[derive(Default)]
pub(crate) struct ResidueScratch {
    classes: Vec<u32>,
    interleaved: Vec<f32>,
}

impl Residue {
    pub fn read(b: &mut Bits, index: usize, books: &[Codebook]) -> Result<Residue> {
        let path = || format!("setup.residues[{index}]");
        let kind = b.read(16);
        if kind > 2 {
            return Err(header_err(path(), "unknown residue type").with_numbers(kind as f64, 2.0));
        }
        let begin = b.read(24);
        let end = b.read(24);
        let partition_size = b.read(24) + 1;
        let classifications = b.read(6) + 1;
        let classbook = b.read(8) as usize;
        if classbook >= books.len() {
            return Err(header_err(path(), "residue classbook doesn't exist").with_numbers(classbook as f64, books.len() as f64));
        }
        let cb = &books[classbook];
        if cb.dims == 0 {
            return Err(header_err(path(), "residue classbook has no dimensions"));
        }
        let mut cascade = Vec::with_capacity(classifications as usize);
        for _ in 0..classifications {
            let low = b.read(3);
            let high = if b.bit() { b.read(5) } else { 0 };
            cascade.push(high * 8 + low);
        }
        let mut class_books = vec![[0u16; 8]; classifications as usize];
        for (c, &bits) in cascade.iter().enumerate() {
            for (pass, slot) in class_books[c].iter_mut().enumerate() {
                if bits & (1 << pass) != 0 {
                    let book = b.read(8) as usize;
                    if book >= books.len() {
                        return Err(header_err(path(), "residue book doesn't exist").with_numbers(book as f64, books.len() as f64));
                    }
                    if !books[book].has_lookup() || books[book].dims == 0 {
                        return Err(header_err(path(), "a residue book needs a VQ lookup table"));
                    }
                    *slot = book as u16 + 1;
                }
            }
        }
        if b.eop() {
            return Err(header_err(path(), "setup header ends inside a residue"));
        }
        Ok(Residue { kind: kind as u8, begin, end, partition_size, classifications, classbook: classbook as u8, books: class_books })
    }

    /// Decodes into `vectors` (one per channel of the submap, each `n` long, already zeroed).
    /// `skip[i]` marks channels not to decode.
    pub fn decode(&self, b: &mut Bits, books: &[Codebook], vectors: &mut [&mut [f32]], skip: &[bool], n: usize, s: &mut ResidueScratch) {
        let ch = vectors.len();
        if ch == 0 {
            return;
        }
        if self.kind == 2 {
            if skip.iter().all(|&x| x) {
                return;
            }
            let total = n * ch;
            s.interleaved.clear();
            s.interleaved.resize(total, 0.0);
            let mut one = [&mut s.interleaved[..]];
            Self::decode_vectors(self, b, books, &mut one, &[false], total, 1, &mut s.classes);
            for (k, frame) in s.interleaved.chunks_exact(ch).enumerate() {
                for (c, &x) in frame.iter().enumerate() {
                    vectors[c][k] = x;
                }
            }
        } else {
            Self::decode_vectors(self, b, books, vectors, skip, n, self.kind, &mut s.classes);
        }
    }

    /// The common partition loop. `format` 0 interleaves a vector's values over the partition
    /// with stride `partition / dims`; 1 (and 2, after interleaving) writes them in order.
    #[allow(clippy::too_many_arguments)]
    fn decode_vectors(&self, b: &mut Bits, books: &[Codebook], vectors: &mut [&mut [f32]], skip: &[bool], size: usize, format: u8, classes: &mut Vec<u32>) {
        let ch = vectors.len();
        let begin = (self.begin as usize).min(size);
        let end = (self.end as usize).min(size);
        if end <= begin {
            return;
        }
        let psize = self.partition_size as usize;
        let parts = ((end - begin) / psize).min(MAX_PARTITIONS);
        if parts == 0 {
            return;
        }
        let classbook = &books[self.classbook as usize];
        let per_word = classbook.dims;
        classes.clear();
        classes.resize(ch * parts, 0);
        for pass in 0..8 {
            let mut p = 0;
            while p < parts {
                if pass == 0 {
                    for j in 0..ch {
                        if skip[j] {
                            continue;
                        }
                        let Some(mut temp) = classbook.decode(b) else { return };
                        for i in (0..per_word).rev() {
                            if p + i < parts {
                                classes[j * parts + p + i] = temp % self.classifications;
                            }
                            temp /= self.classifications;
                        }
                    }
                }
                for _ in 0..per_word {
                    if p >= parts {
                        break;
                    }
                    for j in 0..ch {
                        if skip[j] {
                            continue;
                        }
                        let class = classes[j * parts + p] as usize;
                        let book = self.books[class][pass];
                        if book == 0 {
                            continue;
                        }
                        let cb = &books[book as usize - 1];
                        let offset = begin + p * psize;
                        let v = &mut vectors[j][offset..offset + psize];
                        let ok = if format == 0 { partition0(b, cb, v) } else { partition1(b, cb, v) };
                        if !ok {
                            return;
                        }
                    }
                    p += 1;
                }
            }
        }
    }
}

/// Format 0: value `j` of the `i`-th vector goes to `i + j * step`.
fn partition0(b: &mut Bits, cb: &Codebook, v: &mut [f32]) -> bool {
    let dims = cb.dims;
    let step = v.len() / dims;
    for i in 0..step {
        let Some(vals) = cb.decode_vector(b) else { return false };
        for (j, &x) in vals.iter().enumerate() {
            v[i + j * step] += x;
        }
    }
    true
}

/// Formats 1 and 2: values in order.
fn partition1(b: &mut Bits, cb: &Codebook, v: &mut [f32]) -> bool {
    let mut i = 0;
    while i < v.len() {
        let Some(vals) = cb.decode_vector(b) else { return false };
        for &x in vals {
            if i >= v.len() {
                break;
            }
            v[i] += x;
            i += 1;
        }
    }
    true
}

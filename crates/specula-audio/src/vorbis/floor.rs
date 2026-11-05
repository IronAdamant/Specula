//! Vorbis floors (spec sections 6 and 7): the spectral envelope each channel's residue is
//! multiplied by. Floor 1 (piecewise-linear, in every encoder in use) and floor 0 (LSP, from the
//! early encoders).

use super::bits::{ilog, Bits};
use super::codebook::Codebook;
use super::header_err;
use crate::error::Result;
use specula_core::dmath;

#[derive(Clone, Debug)]
pub(crate) enum Floor {
    Zero(Floor0),
    One(Floor1),
}

#[derive(Clone, Debug)]
pub(crate) struct Floor0 {
    order: usize,
    rate: u32,
    bark_map_size: u32,
    amplitude_bits: u32,
    amplitude_offset: u32,
    books: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) struct Floor1 {
    partition_class: Vec<u8>,
    class_dims: Vec<u8>,
    class_subclasses: Vec<u8>,
    class_masterbook: Vec<u8>,
    /// `[class][subclass]`: codebook + 1 (0 = none).
    subclass_books: Vec<[u16; 8]>,
    multiplier: u32,
    range: u32,
    /// X positions in header order.
    xs: Vec<u32>,
    /// Indices of `xs` sorted by X.
    order: Vec<usize>,
    /// Low and high neighbours of each point (spec 9.2.4 / 9.2.5).
    neighbors: Vec<(usize, usize)>,
}

/// A decoded floor: a curve (in the scratch buffer), or unused for this packet.
pub(crate) enum FloorOut {
    Unused,
    Used,
}

impl Floor {
    pub fn read(b: &mut Bits, index: usize, books: &[Codebook]) -> Result<Floor> {
        let path = || format!("setup.floors[{index}]");
        let kind = b.read(16);
        match kind {
            0 => {
                let order = b.read(8) as usize;
                let rate = b.read(16);
                let bark_map_size = b.read(16);
                let amplitude_bits = b.read(6);
                let amplitude_offset = b.read(8);
                let n = b.read(4) as usize + 1;
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    let book = b.read(8) as usize;
                    if book >= books.len() {
                        return Err(header_err(path(), "floor 0 names a codebook that doesn't exist").with_numbers(book as f64, books.len() as f64));
                    }
                    list.push(book as u8);
                }
                if b.eop() || order == 0 || rate == 0 || bark_map_size == 0 || amplitude_bits > 32 {
                    return Err(header_err(path(), "bad floor 0 header"));
                }
                Ok(Floor::Zero(Floor0 { order, rate, bark_map_size, amplitude_bits, amplitude_offset, books: list }))
            }
            1 => {
                let partitions = b.read(5) as usize;
                let mut partition_class = Vec::with_capacity(partitions);
                let mut max_class = -1i32;
                for _ in 0..partitions {
                    let c = b.read(4) as u8;
                    max_class = max_class.max(c as i32);
                    partition_class.push(c);
                }
                let classes = (max_class + 1) as usize;
                let mut class_dims = vec![0u8; classes];
                let mut class_subclasses = vec![0u8; classes];
                let mut class_masterbook = vec![0u8; classes];
                let mut subclass_books = vec![[0u16; 8]; classes];
                for c in 0..classes {
                    class_dims[c] = (b.read(3) + 1) as u8;
                    class_subclasses[c] = b.read(2) as u8;
                    if class_subclasses[c] > 0 {
                        let m = b.read(8) as usize;
                        if m >= books.len() {
                            return Err(header_err(path(), "floor 1 masterbook doesn't exist").with_numbers(m as f64, books.len() as f64));
                        }
                        class_masterbook[c] = m as u8;
                    }
                    for slot in subclass_books[c].iter_mut().take(1usize << class_subclasses[c]) {
                        let book = b.read(8) as usize;
                        if book > books.len() {
                            return Err(header_err(path(), "floor 1 subclass book doesn't exist").with_numbers(book as f64 - 1.0, books.len() as f64));
                        }
                        *slot = book as u16;
                    }
                }
                let multiplier = b.read(2) + 1;
                let rangebits = b.read(4);
                let mut xs = vec![0u32, 1u32 << rangebits];
                for &c in &partition_class {
                    for _ in 0..class_dims[c as usize] {
                        xs.push(b.read(rangebits));
                    }
                }
                if b.eop() {
                    return Err(header_err(path(), "setup header ends inside floor 1"));
                }
                if xs.len() > 65 {
                    return Err(header_err(path(), "floor 1 has more than 65 points").with_numbers(xs.len() as f64, 65.0));
                }
                let mut order: Vec<usize> = (0..xs.len()).collect();
                order.sort_by_key(|&i| xs[i]);
                if order.windows(2).any(|w| xs[w[0]] == xs[w[1]]) {
                    return Err(header_err(path(), "floor 1 has two points at the same X"));
                }
                let mut neighbors = vec![(0, 0); xs.len()];
                // X values are unique, xs[0] = 0 and every later X is below xs[1], so both
                // neighbours always exist.
                for i in 2..xs.len() {
                    let lo = (0..i).filter(|&j| xs[j] < xs[i]).max_by_key(|&j| xs[j]).unwrap_or(0);
                    let hi = (0..i).filter(|&j| xs[j] > xs[i]).min_by_key(|&j| xs[j]).unwrap_or(1);
                    neighbors[i] = (lo, hi);
                }
                let range = [256, 128, 86, 64][(multiplier - 1) as usize];
                Ok(Floor::One(Floor1 { partition_class, class_dims, class_subclasses, class_masterbook, subclass_books, multiplier, range, xs, order, neighbors }))
            }
            other => Err(header_err(path(), "unknown floor type").with_numbers(other as f64, 1.0)),
        }
    }

    /// Decodes this packet's floor for one channel into `out[..n]` (n = blocksize / 2).
    /// `scratch` holds integer work space.
    pub fn decode(&self, b: &mut Bits, books: &[Codebook], n: usize, out: &mut [f32], scratch: &mut FloorScratch) -> FloorOut {
        match self {
            Floor::One(f) => f.decode(b, books, n, out, scratch),
            Floor::Zero(f) => f.decode(b, books, n, out, scratch),
        }
    }
}

#[derive(Default)]
pub(crate) struct FloorScratch {
    y: Vec<i32>,
    final_y: Vec<i32>,
    step2: Vec<bool>,
    coeffs: Vec<f32>,
    map: Vec<i32>,
}

fn render_point(x0: i32, y0: i32, x1: i32, y1: i32, x: i32) -> i32 {
    // Hostile Y values reach 2^21 and X 2^15: work in i64.
    let dy = y1 as i64 - y0 as i64;
    let adx = x1 as i64 - x0 as i64;
    if adx == 0 {
        return y0;
    }
    let err = dy.abs() * (x as i64 - x0 as i64);
    let off = err / adx;
    let v = if dy < 0 { y0 as i64 - off } else { y0 as i64 + off };
    v.clamp(i32::MIN as i64 / 4, i32::MAX as i64 / 4) as i32
}

fn render_line(x0: i32, y0: i32, x1: i32, y1: i32, n: usize, out: &mut [f32]) {
    let dy = y1 - y0;
    let adx = x1 - x0;
    if adx <= 0 {
        return;
    }
    let base = dy / adx;
    let sy = if dy < 0 { base - 1 } else { base + 1 };
    let ady = dy.abs() - base.abs() * adx;
    let (mut x, mut y, mut err) = (x0, y0, 0);
    let put = |x: i32, y: i32, out: &mut [f32]| {
        if (x as usize) < n {
            out[x as usize] = INVERSE_DB[y.clamp(0, 255) as usize];
        }
    };
    put(x, y, out);
    x += 1;
    while x < x1 {
        err += ady;
        if err >= adx {
            err -= adx;
            y += sy;
        } else {
            y += base;
        }
        put(x, y, out);
        x += 1;
    }
}

impl Floor1 {
    fn decode(&self, b: &mut Bits, books: &[Codebook], n: usize, out: &mut [f32], s: &mut FloorScratch) -> FloorOut {
        if !b.bit() {
            return FloorOut::Unused;
        }
        let values = self.xs.len();
        s.y.clear();
        s.y.resize(values, 0);
        let rbits = ilog(self.range - 1);
        s.y[0] = b.read(rbits) as i32;
        s.y[1] = b.read(rbits) as i32;
        let mut offset = 2;
        for &class in &self.partition_class {
            let c = class as usize;
            let cdim = self.class_dims[c] as usize;
            let cbits = self.class_subclasses[c] as u32;
            let csub = (1u32 << cbits) - 1;
            let mut cval = 0u32;
            if cbits > 0 {
                match books[self.class_masterbook[c] as usize].decode(b) {
                    Some(v) => cval = v,
                    None => return FloorOut::Unused,
                }
            }
            for j in 0..cdim {
                let book = self.subclass_books[c][(cval & csub) as usize];
                cval >>= cbits;
                s.y[offset + j] = if book > 0 {
                    match books[book as usize - 1].decode(b) {
                        Some(v) => v as i32,
                        None => return FloorOut::Unused,
                    }
                } else {
                    0
                };
            }
            offset += cdim;
        }
        if b.eop() {
            return FloorOut::Unused;
        }
        // Amplitude synthesis (step 1 of curve computation).
        let range = self.range as i32;
        s.final_y.clear();
        s.final_y.resize(values, 0);
        s.step2.clear();
        s.step2.resize(values, false);
        s.final_y[0] = s.y[0];
        s.final_y[1] = s.y[1];
        s.step2[0] = true;
        s.step2[1] = true;
        for i in 2..values {
            let (lo, hi) = self.neighbors[i];
            let predicted = render_point(self.xs[lo] as i32, s.final_y[lo], self.xs[hi] as i32, s.final_y[hi], self.xs[i] as i32);
            let val = s.y[i];
            let highroom = range - predicted;
            let lowroom = predicted;
            let room = if highroom < lowroom { highroom * 2 } else { lowroom * 2 };
            if val != 0 {
                s.step2[lo] = true;
                s.step2[hi] = true;
                s.step2[i] = true;
                s.final_y[i] = if val >= room {
                    if highroom > lowroom {
                        val - lowroom + predicted
                    } else {
                        predicted - val + highroom - 1
                    }
                } else if val & 1 == 1 {
                    predicted - (val + 1) / 2
                } else {
                    predicted + val / 2
                };
            } else {
                s.final_y[i] = predicted;
            }
        }
        // Curve synthesis (step 2): lines between the flagged points, in X order.
        let m = self.multiplier as i32;
        let first = self.order[0];
        // Valid Y values are below 256; hostile ones are clamped so the maths can't overflow.
        let fy = |v: i32| v.clamp(-65_536, 65_536) * m;
        let (mut lx, mut ly) = (0i32, fy(s.final_y[first]));
        for &i in &self.order[1..] {
            if s.step2[i] {
                let hx = self.xs[i] as i32;
                let hy = fy(s.final_y[i]);
                render_line(lx, ly, hx, hy, n, out);
                lx = hx;
                ly = hy;
            }
        }
        if (lx as usize) < n {
            render_line(lx, ly, n as i32, ly, n, out);
        }
        FloorOut::Used
    }
}

/// The Bark scale (spec 6.2.3).
fn bark(x: f64) -> f64 {
    13.1 * dmath::atan_f64(0.00074 * x) + 2.24 * dmath::atan_f64(0.000_000_018_5 * x * x) + 0.0001 * x
}

impl Floor0 {
    fn decode(&self, b: &mut Bits, books: &[Codebook], n: usize, out: &mut [f32], s: &mut FloorScratch) -> FloorOut {
        let amplitude = b.read(self.amplitude_bits);
        if amplitude == 0 || b.eop() {
            return FloorOut::Unused;
        }
        let book_n = b.read(ilog(self.books.len() as u32)) as usize;
        let Some(&book) = self.books.get(book_n) else { return FloorOut::Unused };
        let cb = &books[book as usize];
        if !cb.has_lookup() || cb.dims == 0 {
            return FloorOut::Unused;
        }
        s.coeffs.clear();
        let mut last = 0f32;
        while s.coeffs.len() < self.order {
            let Some(v) = cb.decode_vector(b) else { return FloorOut::Unused };
            for &x in v {
                s.coeffs.push(x + last);
            }
            last = *s.coeffs.last().expect("a value");
        }
        s.coeffs.truncate(self.order);
        // The bark map for this block size.
        let bms = self.bark_map_size as f64;
        let scale = bms / bark(0.5 * self.rate as f64);
        s.map.clear();
        for i in 0..n {
            let v = dmath::floor_f64(bark(self.rate as f64 * i as f64 / (2.0 * n as f64)) * scale) as i32;
            s.map.push(v.min(self.bark_map_size as i32 - 1));
        }
        let cos_coef: Vec<f64> = s.coeffs.iter().map(|&c| dmath::cos_f64(c as f64)).collect();
        let max_amp = ((1u64 << self.amplitude_bits) - 1) as f64;
        let mut i = 0;
        while i < n {
            let w = std::f64::consts::PI * s.map[i] as f64 / bms;
            let cw = dmath::cos_f64(w);
            let (mut p, mut q);
            if self.order % 2 == 1 {
                p = 1.0 - cw * cw;
                q = 0.25;
                for j in 0..self.order.saturating_sub(1) / 2 {
                    let d = cos_coef[2 * j + 1] - cw;
                    p *= 4.0 * d * d;
                }
                for j in 0..=(self.order - 1) / 2 {
                    let d = cos_coef[2 * j] - cw;
                    q *= 4.0 * d * d;
                }
            } else {
                p = (1.0 - cw) / 2.0;
                q = (1.0 + cw) / 2.0;
                for j in 0..self.order / 2 {
                    let d1 = cos_coef[2 * j + 1] - cw;
                    p *= 4.0 * d1 * d1;
                    let d0 = cos_coef[2 * j] - cw;
                    q *= 4.0 * d0 * d0;
                }
            }
            let root = (p + q).sqrt();
            let v = if root > 0.0 {
                let db = amplitude as f64 * self.amplitude_offset as f64 / (max_amp * root) - self.amplitude_offset as f64;
                dmath::exp_f64(0.115_129_25 * db)
            } else {
                0.0
            };
            let v = if v.is_finite() { v as f32 } else { 0.0 };
            let cond = s.map[i];
            while i < n && s.map[i] == cond {
                out[i] = v;
                i += 1;
            }
        }
        FloorOut::Used
    }
}

/// The floor 1 inverse dB table (spec 10.1), digit for digit as the spec prints it.
#[allow(clippy::excessive_precision)]
static INVERSE_DB: [f32; 256] = [
    1.0649863e-07, 1.1341951e-07, 1.2079015e-07, 1.2863978e-07, 1.3699951e-07, 1.4590251e-07, 1.5538408e-07, 1.6548181e-07, 1.7623575e-07, 1.8768855e-07,
    1.9988561e-07, 2.1287530e-07, 2.2670913e-07, 2.4144197e-07, 2.5713223e-07, 2.7384213e-07, 2.9163793e-07, 3.1059021e-07, 3.3077411e-07, 3.5226968e-07,
    3.7516214e-07, 3.9954229e-07, 4.2550680e-07, 4.5315863e-07, 4.8260743e-07, 5.1396998e-07, 5.4737065e-07, 5.8294187e-07, 6.2082472e-07, 6.6116941e-07,
    7.0413592e-07, 7.4989464e-07, 7.9862701e-07, 8.5052630e-07, 9.0579828e-07, 9.6466216e-07, 1.0273513e-06, 1.0941144e-06, 1.1652161e-06, 1.2409384e-06,
    1.3215816e-06, 1.4074654e-06, 1.4989305e-06, 1.5963394e-06, 1.7000785e-06, 1.8105592e-06, 1.9282195e-06, 2.0535261e-06, 2.1869758e-06, 2.3290978e-06,
    2.4804557e-06, 2.6416497e-06, 2.8133190e-06, 2.9961443e-06, 3.1908506e-06, 3.3982101e-06, 3.6190449e-06, 3.8542308e-06, 4.1047004e-06, 4.3714470e-06,
    4.6555282e-06, 4.9580707e-06, 5.2802740e-06, 5.6234160e-06, 5.9888572e-06, 6.3780469e-06, 6.7925283e-06, 7.2339451e-06, 7.7040476e-06, 8.2047000e-06,
    8.7378876e-06, 9.3057248e-06, 9.9104632e-06, 1.0554501e-05, 1.1240392e-05, 1.1970856e-05, 1.2748789e-05, 1.3577278e-05, 1.4459606e-05, 1.5399272e-05,
    1.6400004e-05, 1.7465768e-05, 1.8600792e-05, 1.9809576e-05, 2.1096914e-05, 2.2467911e-05, 2.3928002e-05, 2.5482978e-05, 2.7139006e-05, 2.8902651e-05,
    3.0780908e-05, 3.2781225e-05, 3.4911534e-05, 3.7180282e-05, 3.9596466e-05, 4.2169667e-05, 4.4910090e-05, 4.7828601e-05, 5.0936773e-05, 5.4246931e-05,
    5.7772202e-05, 6.1526565e-05, 6.5524908e-05, 6.9783085e-05, 7.4317983e-05, 7.9147585e-05, 8.4291040e-05, 8.9768747e-05, 9.5602426e-05, 0.00010181521,
    0.00010843174, 0.00011547824, 0.00012298267, 0.00013097477, 0.00013948625, 0.00014855085, 0.00015820453, 0.00016848555, 0.00017943469, 0.00019109536,
    0.00020351382, 0.00021673929, 0.00023082423, 0.00024582449, 0.00026179955, 0.00027881276, 0.00029693158, 0.00031622787, 0.00033677814, 0.00035866388,
    0.00038197188, 0.00040679456, 0.00043323036, 0.00046138411, 0.00049136745, 0.00052329927, 0.00055730621, 0.00059352311, 0.00063209358, 0.00067317058,
    0.00071691700, 0.00076350630, 0.00081312324, 0.00086596457, 0.00092223983, 0.00098217216, 0.0010459992, 0.0011139742, 0.0011863665, 0.0012634633,
    0.0013455702, 0.0014330129, 0.0015261382, 0.0016253153, 0.0017309374, 0.0018434235, 0.0019632195, 0.0020908006, 0.0022266726, 0.0023713743,
    0.0025254795, 0.0026895994, 0.0028643847, 0.0030505286, 0.0032487691, 0.0034598925, 0.0036847358, 0.0039241906, 0.0041792066, 0.0044507950,
    0.0047400328, 0.0050480668, 0.0053761186, 0.0057254891, 0.0060975636, 0.0064938176, 0.0069158225, 0.0073652516, 0.0078438871, 0.0083536271,
    0.0088964928, 0.0094746370, 0.010090352, 0.010746080, 0.011444421, 0.012188144, 0.012980198, 0.013823725, 0.014722068, 0.015678791,
    0.016697687, 0.017782797, 0.018938423, 0.020169149, 0.021479854, 0.022875735, 0.024362330, 0.025945531, 0.027631618, 0.029427276,
    0.031339626, 0.033376252, 0.035545228, 0.037855157, 0.040315199, 0.042935108, 0.045725273, 0.048696758, 0.051861348, 0.055231591,
    0.058820850, 0.062643361, 0.066714279, 0.071049749, 0.075666962, 0.080584227, 0.085821044, 0.091398179, 0.097337747, 0.10366330,
    0.11039993, 0.11757434, 0.12521498, 0.13335215, 0.14201813, 0.15124727, 0.16107617, 0.17154380, 0.18269168, 0.19456402,
    0.20720788, 0.22067342, 0.23501402, 0.25028656, 0.26655159, 0.28387361, 0.30232132, 0.32196786, 0.34289114, 0.36517414,
    0.38890521, 0.41417847, 0.44109412, 0.46975890, 0.50028648, 0.53279791, 0.56742212, 0.60429640, 0.64356699, 0.68538959,
    0.72993007, 0.77736504, 0.82788260, 0.88168307, 0.9389798, 1.0,
];

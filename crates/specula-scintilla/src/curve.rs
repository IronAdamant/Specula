//! Values over a particle's life (`u` = age / lifetime, 0..1): scalar curves (size, alpha) and
//! colour gradients (sRGB keys, interpolated in linear light).

use crate::error::{read, Path, Result, VfxError};
use specula_core::{dmath, Json};

/// Most keys per curve or gradient.
pub const MAX_KEYS: usize = 16;

/// Piecewise-linear `(u, value)` keys, `u` ascending in 0..1; constant before the first and
/// after the last.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub keys: Vec<[f32; 2]>,
}

impl Curve {
    pub fn constant(v: f32) -> Curve {
        Curve { keys: vec![[0.0, v]] }
    }

    /// From a number, `[[u, v], ...]` or `{"keys": [[u, v], ...]}`, values in `lo..=hi`.
    pub fn parse(v: &Json, at: &Path, lo: f32, hi: f32) -> Result<Curve> {
        if v.as_f64().is_some() {
            return Ok(Curve::constant(read::num(v, at, lo, hi)?));
        }
        let (list, at) = keys_of(v, at)?;
        let mut keys = Vec::with_capacity(list.len());
        for (i, k) in list.iter().enumerate() {
            let p = at.index(i);
            match k.as_array() {
                Some([u, x]) => keys.push([read::num(u, &p.index(0), 0.0, 1.0)?, read::num(x, &p.index(1), lo, hi)?]),
                _ => return Err(VfxError::new("invalid_value", p.as_str(), format!("'{p}' must be [u, value] (u from 0 to 1)")).value(k)),
            }
        }
        ascending(keys.iter().map(|k| k[0]), &at)?;
        Ok(Curve { keys })
    }

    pub fn eval(&self, u: f32) -> f32 {
        let k = &self.keys;
        if u <= k[0][0] {
            return k[0][1];
        }
        for w in k.windows(2) {
            if u <= w[1][0] {
                let span = w[1][0] - w[0][0];
                let f = if span > 0.0 { (u - w[0][0]) / span } else { 1.0 };
                return w[0][1] + (w[1][1] - w[0][1]) * f;
            }
        }
        k[k.len() - 1][1]
    }

    /// The largest value anywhere.
    pub fn max(&self) -> f32 {
        self.keys.iter().map(|k| k[1]).fold(f32::NEG_INFINITY, f32::max)
    }

    /// The mean value over life (trapezoids between keys, constant ends).
    pub fn mean(&self) -> f32 {
        let k = &self.keys;
        let mut area = k[0][0] * k[0][1] + (1.0 - k[k.len() - 1][0]) * k[k.len() - 1][1];
        for w in k.windows(2) {
            area += (w[1][0] - w[0][0]) * 0.5 * (w[0][1] + w[1][1]);
        }
        area
    }

    pub fn to_json(&self) -> Json {
        if self.keys.len() == 1 && self.keys[0][0] == 0.0 {
            return Json::from(self.keys[0][1]);
        }
        Json::obj([("keys", Json::arr(self.keys.iter().map(|k| Json::arr([Json::from(k[0]), Json::from(k[1])]))))])
    }
}

/// Colour keys: sRGB as given (for output), linear for interpolation.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// `(u, sRGB)` as given.
    pub keys: Vec<(f32, [f32; 3])>,
    linear: Vec<(f32, [f32; 3])>,
}

impl Gradient {
    pub fn solid(srgb: [f32; 3]) -> Gradient {
        Gradient { keys: vec![(0.0, srgb)], linear: vec![(0.0, srgb.map(srgb_to_linear))] }
    }

    /// From a colour, `[[u, colour], ...]` or `{"keys": [...]}` (colours `"#rrggbb"` or
    /// `[r, g, b]`).
    pub fn parse(v: &Json, at: &Path) -> Result<Gradient> {
        if v.as_str().is_some() || v.as_array().is_some_and(|a| a.len() == 3 && a.iter().all(|x| x.as_f64().is_some())) {
            return Ok(Gradient::solid(read::color(v, at)?));
        }
        let (list, at) = keys_of(v, at)?;
        let mut keys = Vec::with_capacity(list.len());
        for (i, k) in list.iter().enumerate() {
            let p = at.index(i);
            match k.as_array() {
                Some([u, c]) => keys.push((read::num(u, &p.index(0), 0.0, 1.0)?, read::color(c, &p.index(1))?)),
                _ => return Err(VfxError::new("invalid_value", p.as_str(), format!("'{p}' must be [u, colour] (u from 0 to 1)")).value(k)),
            }
        }
        ascending(keys.iter().map(|k| k.0), &at)?;
        let linear = keys.iter().map(|&(u, c)| (u, c.map(srgb_to_linear))).collect();
        Ok(Gradient { keys, linear })
    }

    /// Linear RGB at `u`.
    pub fn eval(&self, u: f32) -> [f32; 3] {
        let k = &self.linear;
        if u <= k[0].0 {
            return k[0].1;
        }
        for w in k.windows(2) {
            if u <= w[1].0 {
                let span = w[1].0 - w[0].0;
                let f = if span > 0.0 { (u - w[0].0) / span } else { 1.0 };
                let (a, b) = (w[0].1, w[1].1);
                return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
            }
        }
        k[k.len() - 1].1
    }

    /// The sRGB keys' colours.
    pub fn colors(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.keys.iter().map(|k| k.1)
    }

    pub fn to_json(&self) -> Json {
        let hex = |c: [f32; 3]| Json::from(hex_of(c));
        if self.keys.len() == 1 && self.keys[0].0 == 0.0 {
            return hex(self.keys[0].1);
        }
        Json::obj([("keys", Json::arr(self.keys.iter().map(|&(u, c)| Json::arr([Json::from(u), hex(c)]))))])
    }
}

/// `"#rrggbb"` of an sRGB colour in 0..1.
pub fn hex_of(c: [f32; 3]) -> String {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// The keys list of `[...]` or `{"keys": [...]}`, with its path.
fn keys_of<'a>(v: &'a Json, at: &Path) -> Result<(&'a [Json], Path)> {
    let (list, at) = match v {
        Json::Arr(a) => (a.as_slice(), at.clone()),
        Json::Obj(_) => {
            read::keys(v, at, &["keys"])?;
            let k = at.key("keys");
            let a = v.get("keys").and_then(Json::as_array).ok_or_else(|| VfxError::new("missing_field", k.as_str(), format!("'{k}' is required: [[u, value], ...]")))?;
            (a, k)
        }
        _ => return Err(VfxError::new("bad_type", at.as_str(), format!("'{at}' must be a value, [[u, value], ...] or {{keys: [...]}}")).value(v)),
    };
    if list.is_empty() || list.len() > MAX_KEYS {
        return Err(VfxError::new("too_many", at.as_str(), format!("'{at}' needs 1 to {MAX_KEYS} keys (got {})", list.len())).allowed(format!("1..={MAX_KEYS}")));
    }
    Ok((list, at))
}

fn ascending(us: impl Iterator<Item = f32>, at: &Path) -> Result<()> {
    let mut last = f32::NEG_INFINITY;
    for (i, u) in us.enumerate() {
        if u < last {
            return Err(VfxError::new("invalid_value", at.index(i).as_str(), format!("'{at}' keys must be in ascending u (key {i} goes back to {u})")));
        }
        last = u;
    }
    Ok(())
}

/// sRGB-encoded channel → linear (IEC 61966-2-1), clamped to 0..1.
pub fn srgb_to_linear(c: f32) -> f32 {
    let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
    if c <= 0.04045 {
        c / 12.92
    } else {
        dmath::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// Linear → sRGB-encoded channel, clamped to 0..1.
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * dmath::pow(c, 1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_interpolate_and_hold_their_ends() {
        let c = Curve::parse(&Json::parse("[[0.2, 1], [0.6, 3]]").unwrap(), &Path::root("size"), 0.0, 10.0).unwrap();
        assert_eq!(c.eval(0.0), 1.0);
        assert_eq!(c.eval(0.4), 2.0);
        assert_eq!(c.eval(0.9), 3.0);
        assert_eq!(c.max(), 3.0);
        // 0.2 × 1 + 0.4 × 2 + 0.4 × 3
        assert!((c.mean() - 2.2).abs() < 1e-6);
        let back = Curve::parse(&c.to_json(), &Path::root("size"), 0.0, 10.0).unwrap();
        assert_eq!(back, c);
        assert_eq!(Curve::parse(&Json::from(0.5), &Path::root("a"), 0.0, 1.0).unwrap().to_json(), Json::from(0.5));
    }

    #[test]
    fn bad_curves_are_structured_errors() {
        let p = Path::root("alpha");
        let e = Curve::parse(&Json::parse("[[0.5, 1], [0.2, 1]]").unwrap(), &p, 0.0, 1.0).unwrap_err();
        assert_eq!(e.field, "alpha[1]");
        let e = Curve::parse(&Json::parse("[]").unwrap(), &p, 0.0, 1.0).unwrap_err();
        assert_eq!(e.code, "too_many");
        let e = Curve::parse(&Json::parse(r#"{"keys": [[0, 2]]}"#).unwrap(), &p, 0.0, 1.0).unwrap_err();
        assert_eq!(e.field, "alpha.keys[0][1]");
    }

    #[test]
    fn gradients_blend_in_linear_light() {
        let g = Gradient::parse(&Json::parse(r##"[[0, "#000000"], [1, "#ffffff"]]"##).unwrap(), &Path::root("color")).unwrap();
        let mid = g.eval(0.5);
        assert!((mid[0] - 0.5).abs() < 1e-6, "linear midpoint {mid:?}");
        assert_eq!(hex_of([1.0, 0.5, 0.0]), "#ff8000");
        let back = Gradient::parse(&g.to_json(), &Path::root("color")).unwrap();
        assert_eq!(back.keys, g.keys);
        assert!((linear_to_srgb(srgb_to_linear(0.42)) - 0.42).abs() < 1e-5);
    }
}

//! Ease curves: how a value travels between two keys. Every curve maps 0 to 0 and 1 to 1 and is
//! built from `+ − × ÷` only (the same bits on every platform).

use crate::error::{Path, Result, TimelineError};
use specula_core::Json;

/// How a segment between two keys is travelled (the curve on the key it arrives at).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ease {
    /// Constant speed.
    #[default]
    Linear,
    /// Starts slow (quadratic).
    In,
    /// Ends slow (quadratic).
    Out,
    /// Starts and ends slow (cubic smoothstep: zero speed at both ends).
    InOut,
    /// Smootherstep: zero speed and zero acceleration at both ends (the gentlest camera curve).
    Smooth,
    /// Holds the previous key's value, then jumps at this key's time (a cut).
    Step,
}

pub const NAMES: [&str; 6] = ["linear", "in", "out", "in_out", "smooth", "step"];

impl Ease {
    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::In => "in",
            Ease::Out => "out",
            Ease::InOut => "in_out",
            Ease::Smooth => "smooth",
            Ease::Step => "step",
        }
    }

    pub fn parse(v: &Json, at: &Path) -> Result<Ease> {
        let s = v.as_str().unwrap_or("");
        Ok(match s {
            "linear" => Ease::Linear,
            "in" => Ease::In,
            "out" => Ease::Out,
            "in_out" => Ease::InOut,
            "smooth" => Ease::Smooth,
            "step" => Ease::Step,
            _ => return Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' must be an ease name")).allowed(NAMES.join(", ")).value(v)),
        })
    }

    /// The eased fraction of `u` (clamped to 0..1).
    pub fn apply(self, u: f64) -> f64 {
        let u = if u.is_nan() { 0.0 } else { u.clamp(0.0, 1.0) };
        match self {
            Ease::Linear => u,
            Ease::In => u * u,
            Ease::Out => u * (2.0 - u),
            Ease::InOut => u * u * (3.0 - 2.0 * u),
            Ease::Smooth => u * u * u * (u * (u * 6.0 - 15.0) + 10.0),
            Ease::Step => {
                if u >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_meet_their_ends_and_rise() {
        for e in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut, Ease::Smooth, Ease::Step] {
            assert_eq!(e.apply(0.0), 0.0, "{e:?}");
            assert_eq!(e.apply(1.0), 1.0, "{e:?}");
            assert_eq!(e.apply(-3.0), 0.0);
            assert_eq!(e.apply(f64::NAN), 0.0);
            let mut last = 0.0;
            for i in 0..=100 {
                let v = e.apply(i as f64 / 100.0);
                assert!(v >= last, "{e:?} falls at {i}");
                last = v;
            }
            assert_eq!(Ease::parse(&Json::from(e.name()), &Path::root("e")).unwrap(), e);
        }
        assert!((Ease::InOut.apply(0.5) - 0.5).abs() < 1e-12);
        assert!(Ease::In.apply(0.5) < 0.5 && Ease::Out.apply(0.5) > 0.5);
        assert_eq!(Ease::Step.apply(0.999), 0.0);
        assert!(Ease::parse(&Json::from("bouncy"), &Path::root("e")).is_err());
    }
}

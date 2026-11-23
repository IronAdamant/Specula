//! Footfall patterns and dynamic similarity: a gait written in a plan's terms (a phase for each
//! foot, at a Froude number), so its stride and period come from the specimen's own legs
//! (docs/design/EXEMPLARIA.md, A.5; research C2).
//!
//! Legs are named `lh`, `lf`, `rh`, `rf` (left hind, left fore, right hind, right fore); phases
//! are the fraction of the cycle at which each foot lands, with the left hind at 0 (research C2's
//! convention).

use crate::draw::pow;

/// Gravity, m/s².
pub const G: f64 = 9.81;

/// A footfall pattern.
#[derive(Debug)]
pub struct Pattern {
    pub name: &'static str,
    /// Landing phase of each foot (`lh` at 0).
    pub phases: &'static [(&'static str, f64)],
    /// Where the phases came from.
    pub source: &'static str,
    /// A walk (the walking stride and duty equations) or a run.
    pub walk: bool,
}

/// The patterns a plan's gaits take.
pub const PATTERNS: &[Pattern] = &[
    Pattern {
        name: "lateral_walk",
        phases: &[("lh", 0.0), ("lf", 0.16), ("rh", 0.50), ("rf", 0.67)],
        source: "Maes et al. 2008, J Exp Biol 211:138, Table 2: Malinois lateral walk, 189 sequences (research C2 1a, converted to LH = 0)",
        walk: true,
    },
    Pattern {
        name: "trot",
        phases: &[("lh", 0.0), ("lf", 0.50), ("rh", 0.49), ("rf", 0.01)],
        source: "Maes et al. 2008, J Exp Biol 211:138, Table 2: Malinois trot, 180 sequences (research C2 1a, converted to LH = 0)",
        walk: false,
    },
];

/// The pattern of this name.
pub fn pattern(name: &str) -> Option<&'static Pattern> {
    PATTERNS.iter().find(|p| p.name == name)
}

/// A gait at a Froude number for hips `h` metres high: stride (m), period (s), duty factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    pub stride: f64,
    pub period: f64,
    pub duty: f64,
    pub speed: f64,
}

/// Where the stride and duty equations came from.
pub const SIMILARITY: &str = "Alexander and Jayes 1983, J Zool 201:135, via Hutchinson et al. 2006 Table 6 (research C2 3): walk stride/h = 2.4 Fr^0.34, duty 0.51 Fr^-0.18; run stride/h = 1.9 Fr^0.40 (cursorial), duty 0.53 Fr^-0.28";

/// Alexander and Jayes's dynamic similarity ([`SIMILARITY`]): speed from the Froude number,
/// stride and duty from the walking or running equations, the period from the two.
pub fn cycle(froude: f64, h: f64, walk: bool) -> Cycle {
    let speed = (froude * G * h).sqrt();
    let (k, e, d, de) = if walk { (2.4, 0.34, 0.51, -0.18) } else { (1.9, 0.40, 0.53, -0.28) };
    let stride = h * k * pow(froude, e);
    let duty = (d * pow(froude, de)).clamp(0.2, 0.9);
    Cycle { stride, period: stride / speed, duty, speed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probes_walk() {
        // The design note's probe: Fr 0.25, h 0.62 m gives 0.93 m, 0.75 s, duty 0.65.
        let c = cycle(0.25, 0.62, true);
        assert!((c.stride - 0.93).abs() < 0.005 && (c.period - 0.75).abs() < 0.005 && (c.duty - 0.65).abs() < 0.005, "{c:?}");
        for p in PATTERNS {
            assert_eq!(p.phases[0], ("lh", 0.0));
            assert!(p.phases.iter().all(|(_, x)| (0.0..1.0).contains(x)));
        }
    }
}

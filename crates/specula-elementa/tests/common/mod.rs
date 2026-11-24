//! Helpers shared by the tests.
#![allow(dead_code, reason = "each test file uses a part of the helpers")]

use specula_core::math::Vec3;
use specula_elementa::{Config, Elementa, NodeDesc};

pub const TICK_HZ: u32 = 60;

pub fn config(json: &str) -> Config {
    Config::parse(json).unwrap_or_else(|e| panic!("config: {e}"))
}

pub fn sim(json: &str, seed: u64) -> Elementa {
    Elementa::new(config(json), TICK_HZ, seed).unwrap()
}

pub fn mat(e: &Elementa, name: &str) -> u16 {
    e.config().material(name).unwrap_or_else(|| panic!("no material {name}"))
}

/// The smallest whole side of a square that holds `n`.
pub fn side(n: usize) -> usize {
    (0..).find(|c| c * c >= n).unwrap_or(0)
}

/// A forest of `n` things on a square of 1 m spacing in the XZ plane, grass and wood mixed, some
/// set off a little by a fixed integer pattern, ids from `first` in row order.
pub fn forest(e: &Elementa, n: usize, first: u64) -> Vec<NodeDesc> {
    let cols = side(n);
    let (grass, wood) = (mat(e, "grass"), mat(e, "wood"));
    (0..n)
        .map(|i| {
            let (x, z) = ((i % cols) as f32, (i / cols) as f32);
            let k = (i as u64).wrapping_mul(2_654_435_761) % 7;
            let jitter = k as f32 * 0.04;
            let m = if k < 3 { wood } else { grass };
            let half = if m == wood { Vec3::new(0.35, 1.5, 0.35) } else { Vec3::new(0.3, 0.2, 0.3) };
            NodeDesc::new(first + i as u64, m, Vec3::new(x + jitter, half.y, z - jitter), half)
        })
        .collect()
}

/// The centre of a forest of `n` things.
pub fn forest_centre(n: usize) -> Vec3 {
    let cols = side(n) as f32;
    Vec3::new(cols * 0.5, 0.5, cols * 0.5)
}

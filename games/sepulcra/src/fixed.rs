//! Fixed-point units: the simulation never touches a float.
//!
//! Integer maths is bit-identical on x86 and ARM with no maths library involved (F2 level D3, F6).
//! Units, used everywhere in the sim:
//!
//! | Quantity | Unit | Type |
//! |---|---|---|
//! | position, distance, range | millimetre | `i32` (`i64` for squares) |
//! | time | tick (1/[`TICK_HZ`] s) | `u32` |
//! | hit points, damage | milli-HP (1 HP = 1000) | `i32` |
//! | speed | mm per second (moved per tick with a remainder carry) | `i32` |
//! | ratios (slow, chance) | permille (1000 = 100 %) | `i32` |
//!
//! Content files speak metres, seconds and HP; [`mm`], [`ticks`] and [`milli`] convert once at
//! load time. Floats appear again only in presentation helpers (`world_pos_f32`).

/// Simulation ticks per second. 30 Hz is plenty for a tower defence (enemies move < 10 cm per
/// tick) and halves the cost of a 12-minute headless run against 60 Hz.
pub const TICK_HZ: u32 = 30;

/// Milli-HP per HP.
pub const HP: i32 = 1000;

/// A point in world space in millimetres: right-handed, +Y up (the engine's convention).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32, z: i32) -> Pos {
        Pos { x, y, z }
    }

    pub fn dist2(self, o: Pos) -> i64 {
        let dx = i64::from(self.x) - i64::from(o.x);
        let dy = i64::from(self.y) - i64::from(o.y);
        let dz = i64::from(self.z) - i64::from(o.z);
        dx * dx + dy * dy + dz * dz
    }

    /// Euclidean distance, rounded down.
    pub fn dist(self, o: Pos) -> i32 {
        isqrt(self.dist2(o) as u64) as i32
    }

    /// `a + (b - a) * num / den`, rounding toward zero. `den <= 0` gives `b`.
    pub fn lerp(a: Pos, b: Pos, num: i32, den: i32) -> Pos {
        if den <= 0 {
            return b;
        }
        let f = |a: i32, b: i32| a + ((i64::from(b) - i64::from(a)) * i64::from(num) / i64::from(den)) as i32;
        Pos { x: f(a.x, b.x), y: f(a.y, b.y), z: f(a.z, b.z) }
    }

    /// Moves up to `d` mm toward `target`. Returns the new point and whether it arrived.
    pub fn step_toward(self, target: Pos, d: i32) -> (Pos, bool) {
        let len = self.dist(target);
        if len <= d {
            (target, true)
        } else {
            (Pos::lerp(self, target, d, len), false)
        }
    }

    pub fn up(self, mm: i32) -> Pos {
        Pos { y: self.y + mm, ..self }
    }

    /// Metres as `f32`, for presentation only (never feed it back into the sim).
    pub fn to_f32(self) -> [f32; 3] {
        [self.x as f32 / 1000.0, self.y as f32 / 1000.0, self.z as f32 / 1000.0]
    }
}

/// Integer square root, rounded down (digit-by-digit; exact for every `u64`).
pub fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut num = n;
    let mut res = 0u64;
    let mut bit = 1u64 << ((63 - n.leading_zeros()) & !1);
    while bit != 0 {
        if num >= res + bit {
            num -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// Metres → mm (load time only).
pub fn mm(metres: f64) -> i32 {
    (metres * 1000.0).round() as i32
}

/// Seconds → ticks (load time only).
pub fn ticks(seconds: f64) -> u32 {
    (seconds * f64::from(TICK_HZ)).round() as u32
}

/// HP → milli-HP (load time only).
pub fn milli(v: f64) -> i32 {
    (v * 1000.0).round() as i32
}

/// A 0..1 ratio → permille (load time only).
pub fn permille(ratio: f64) -> i32 {
    (ratio * 1000.0).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_is_exact() {
        for n in 0..10_000u64 {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "isqrt({n}) = {r}");
        }
        assert_eq!(isqrt(u64::MAX), u64::from(u32::MAX));
        assert_eq!(isqrt(1 << 62), 1 << 31);
    }

    #[test]
    fn lerp_and_steps() {
        let a = Pos::new(0, 0, 0);
        let b = Pos::new(2000, 0, -1000);
        assert_eq!(Pos::lerp(a, b, 1, 2), Pos::new(1000, 0, -500));
        assert_eq!(Pos::lerp(a, b, 0, 0), b);
        assert_eq!(a.dist(Pos::new(3000, 4000, 0)), 5000);
        let (p, arrived) = a.step_toward(Pos::new(3000, 4000, 0), 1000);
        assert!(!arrived);
        assert_eq!(p, Pos::new(600, 800, 0));
        assert_eq!(p.step_toward(Pos::new(3000, 4000, 0), 4000), (Pos::new(3000, 4000, 0), true));
    }

    #[test]
    fn load_time_conversions() {
        assert_eq!(mm(2.5), 2500);
        assert_eq!(ticks(0.8), 24);
        assert_eq!(milli(12.5), 12500);
        assert_eq!(permille(0.35), 350);
    }
}

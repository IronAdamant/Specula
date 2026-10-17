//! Units: content speaks metres, seconds and percent; the simulation runs on integers.
//!
//! | Unit | Written as | Stored as | Literal |
//! |---|---|---|---|
//! | [`Unit::Int`] | whole numbers | the number | `5` |
//! | [`Unit::Points`] | HP, damage | milli-points | `10pt` |
//! | [`Unit::Distance`] | metres | mm | `7m` |
//! | [`Unit::Speed`] | metres per second | mm/s | `2.2m/s` |
//! | [`Unit::Duration`] | seconds | ticks | `0.8s`, `24t` |
//! | [`Unit::Ratio`] | 0..1, percent, multiplier | permille | `35%`, `1.35x` |
//!
//! Conversions are exact decimal arithmetic (no float ever touches the result): a JSON number is
//! printed in its shortest round-trip form and read back as a decimal, then scaled with integer
//! maths and rounded half away from zero. A value that needed rounding is reported as inexact
//! with both nearest exact values, so an agent sees "0.25 s is 7.5 ticks".

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unit {
    Int,
    Points,
    Distance,
    Speed,
    Duration,
    Ratio,
}

impl Unit {
    pub const ALL: [Unit; 6] = [Unit::Int, Unit::Points, Unit::Distance, Unit::Speed, Unit::Duration, Unit::Ratio];

    pub fn name(self) -> &'static str {
        match self {
            Unit::Int => "int",
            Unit::Points => "points",
            Unit::Distance => "distance",
            Unit::Speed => "speed",
            Unit::Duration => "duration",
            Unit::Ratio => "ratio",
        }
    }

    pub fn from_name(s: &str) -> Option<Unit> {
        Unit::ALL.into_iter().find(|u| u.name() == s)
    }

    /// What a bare number in a field of this unit means.
    pub fn human(self) -> &'static str {
        match self {
            Unit::Int => "a whole number",
            Unit::Points => "points (HP, damage; stored as milli-points)",
            Unit::Distance => "metres",
            Unit::Speed => "metres per second",
            Unit::Duration => "seconds",
            Unit::Ratio => "a ratio (0.35, \"35%\" or \"1.35x\")",
        }
    }

    /// The literal suffix that gives a number this unit in an expression.
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Int => "",
            Unit::Points => "pt",
            Unit::Distance => "m",
            Unit::Speed => "m/s",
            Unit::Duration => "s",
            Unit::Ratio => "%",
        }
    }

    /// The stored integer per one human unit (durations: per second, so `tick_hz`).
    pub fn per_human(self, tick_hz: u32) -> i128 {
        match self {
            Unit::Int => 1,
            Unit::Points | Unit::Distance | Unit::Speed | Unit::Ratio => 1000,
            Unit::Duration => i128::from(tick_hz),
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An exact decimal: `mant × 10^-scale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dec {
    pub mant: i128,
    pub scale: u32,
}

impl Dec {
    pub fn int(v: i64) -> Dec {
        Dec { mant: i128::from(v), scale: 0 }
    }

    pub fn is_integer(self) -> bool {
        self.scale == 0 || self.mant % 10i128.pow(self.scale) == 0
    }
}

/// Parses `[-]digits[.digits]` exactly. At most 30 significant digits.
pub fn parse_dec(s: &str) -> Option<Dec> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    if !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac = frac.trim_end_matches('0');
    let digits = format!("{int}{frac}");
    let digits = digits.trim_start_matches('0');
    if digits.len() > 30 {
        return None;
    }
    let mant: i128 = if digits.is_empty() { 0 } else { digits.parse().ok()? };
    Some(Dec { mant: if neg { -mant } else { mant }, scale: frac.len() as u32 })
}

/// A JSON number as an exact decimal (its shortest round-trip form; `None` for non-finite or
/// absurdly long values).
pub fn dec_from_f64(v: f64) -> Option<Dec> {
    if !v.is_finite() {
        return None;
    }
    parse_dec(&format!("{v}"))
}

/// The result of scaling a decimal to a stored integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scaled {
    pub value: i64,
    pub exact: bool,
    /// The nearest stored values below and above the exact result (equal when exact).
    pub below: i64,
    pub above: i64,
}

/// `d × num / den`, rounded half away from zero. `None` when it doesn't fit an `i64`.
pub fn scale(d: Dec, num: i128, den: i128) -> Option<Scaled> {
    let p = d.mant.checked_mul(num)?;
    let q = 10i128.checked_pow(d.scale)?.checked_mul(den)?;
    let floor = p.div_euclid(q);
    let rem = p.rem_euclid(q);
    let exact = rem == 0;
    let rounded = if exact {
        floor
    } else if rem * 2 > q || (rem * 2 == q && p > 0) {
        floor + 1
    } else {
        floor
    };
    let fit = |v: i128| i64::try_from(v).ok();
    Some(Scaled { value: fit(rounded)?, exact, below: fit(floor)?, above: fit(if exact { floor } else { floor + 1 })? })
}

/// A stored value in human units, for messages and summaries: `7000` mm → `7m`, `24` ticks at
/// 30 Hz → `0.8s`, `350` permille → `35%`.
pub fn human(value: i64, unit: Unit, tick_hz: u32) -> String {
    let thousandths = |v: i64| -> String {
        let neg = v < 0;
        let a = v.unsigned_abs();
        let (i, f) = (a / 1000, a % 1000);
        let mut s = if f == 0 { i.to_string() } else { format!("{i}.{:03}", f).trim_end_matches('0').to_owned() };
        if neg {
            s.insert(0, '-');
        }
        s
    };
    match unit {
        Unit::Int => value.to_string(),
        Unit::Points => format!("{}pt", thousandths(value)),
        Unit::Distance => format!("{}m", thousandths(value)),
        Unit::Speed => format!("{}m/s", thousandths(value)),
        Unit::Ratio => {
            // permille → percent with one decimal at most.
            if value % 10 == 0 {
                format!("{}%", value / 10)
            } else {
                format!("{}.{}%", value / 10, (value % 10).abs())
            }
        }
        Unit::Duration => {
            let hz = i64::from(tick_hz.max(1));
            if (value * 1000) % hz == 0 {
                format!("{}s", thousandths(value * 1000 / hz))
            } else {
                format!("{value}t")
            }
        }
    }
}

/// Literal suffixes in expressions: the unit they give and the stored integer per unit.
pub fn suffix_unit(suffix: &str, tick_hz: u32) -> Option<(Unit, i128, i128)> {
    Some(match suffix {
        "" => (Unit::Int, 1, 1),
        "pt" => (Unit::Points, 1000, 1),
        "m" => (Unit::Distance, 1000, 1),
        "m/s" => (Unit::Speed, 1000, 1),
        "s" => (Unit::Duration, i128::from(tick_hz), 1),
        "t" => (Unit::Duration, 1, 1),
        "%" => (Unit::Ratio, 10, 1),
        "x" => (Unit::Ratio, 1000, 1),
        _ => return None,
    })
}

pub const SUFFIXES: [&str; 7] = ["pt", "m", "m/s", "s", "t", "%", "x"];

/// Integer square root of a `u128`, rounded down (exact).
pub fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut num = n;
    let mut res = 0u128;
    let mut bit = 1u128 << ((127 - n.leading_zeros()) & !1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_parse_exactly() {
        assert_eq!(parse_dec("0.35"), Some(Dec { mant: 35, scale: 2 }));
        assert_eq!(parse_dec("-2.50"), Some(Dec { mant: -25, scale: 1 }));
        assert_eq!(parse_dec("007"), Some(Dec { mant: 7, scale: 0 }));
        assert_eq!(parse_dec(".5"), Some(Dec { mant: 5, scale: 1 }));
        for bad in ["", "-", "1e5", "1.2.3", "abc", "."] {
            assert_eq!(parse_dec(bad), None, "{bad}");
        }
        assert_eq!(dec_from_f64(0.1), Some(Dec { mant: 1, scale: 1 }), "shortest form, not 0.1000000000000000055");
        assert_eq!(dec_from_f64(1e21).map(|d| d.scale), Some(0));
    }

    #[test]
    fn scaling_rounds_half_away_from_zero_and_reports_inexact() {
        let s = scale(parse_dec("0.8").unwrap(), 30, 1).unwrap();
        assert_eq!((s.value, s.exact), (24, true));
        let s = scale(parse_dec("0.25").unwrap(), 30, 1).unwrap();
        assert_eq!((s.value, s.exact, s.below, s.above), (8, false, 7, 8));
        let s = scale(parse_dec("-0.25").unwrap(), 30, 1).unwrap();
        assert_eq!((s.value, s.below, s.above), (-8, -8, -7));
        assert_eq!(scale(parse_dec("19.98").unwrap(), 1000, 1).unwrap().value, 19_980);
        assert_eq!(scale(Dec::int(i64::MAX), 1000, 1), None);
    }

    #[test]
    fn human_forms() {
        assert_eq!(human(7000, Unit::Distance, 30), "7m");
        assert_eq!(human(2500, Unit::Distance, 30), "2.5m");
        assert_eq!(human(24, Unit::Duration, 30), "0.8s");
        assert_eq!(human(1, Unit::Duration, 30), "1t");
        assert_eq!(human(350, Unit::Ratio, 30), "35%");
        assert_eq!(human(-1500, Unit::Points, 30), "-1.5pt");
    }

    #[test]
    fn isqrt_is_exact() {
        for n in 0..5000u128 {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n);
        }
        assert_eq!(isqrt(u128::from(u64::MAX) * u128::from(u64::MAX)), u128::from(u64::MAX));
    }
}

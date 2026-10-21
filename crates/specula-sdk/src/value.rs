//! Values that cross the module boundary, and their byte encoding.
//!
//! Everything a module and the engine exchange (requests, answers, tick input, hook arguments,
//! errors) is a [`Value`] tree encoded to bytes. Maps are ordered ([`BTreeMap`]), so an encoding
//! is canonical: the same value always gives the same bytes (foundation F2). Floats travel as
//! their exact bits.
//!
//! Encoding (little-endian): one tag byte, then
//! - `0` null, `1` false, `2` true;
//! - `3` int: 8 bytes (`i64`); `4` num: 8 bytes (`f64` bits);
//! - `5` string: `u32` length + UTF-8;
//! - `6` list: `u32` count + items; `7` map: `u32` count + (`u32` length + UTF-8 key, value)
//!   with keys strictly increasing.

use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeMap;
use std::fmt;

/// Deepest nesting [`Value::decode`] accepts.
pub const MAX_DEPTH: usize = 64;

/// A tree of plain data.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

/// Why bytes are not a [`Value`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// Byte offset of the problem.
    pub offset: usize,
    /// Machine-readable: `truncated`, `bad_tag`, `bad_utf8`, `key_order`, `too_deep`, `trailing`.
    pub code: &'static str,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad value bytes at offset {}: {}", self.offset, self.code)
    }
}

impl std::error::Error for DecodeError {}

impl Value {
    /// An empty map, to build with [`Value::with`].
    pub fn map() -> Value {
        Value::Map(BTreeMap::new())
    }

    /// This map with `key` set (a non-map becomes a map first).
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Value {
        self.insert(key, value);
        self
    }

    /// Sets `key` in a map (a non-map becomes a map first).
    pub fn insert(&mut self, key: &str, value: impl Into<Value>) {
        if !matches!(self, Value::Map(_)) {
            *self = Value::map();
        }
        if let Value::Map(m) = self {
            m.insert(key.to_owned(), value.into());
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.get(key),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Ints and nums.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|v| v as f32)
    }

    /// Ints, and nums that are whole and in range.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Num(n) if specula_core::dmath::fract_f64(*n) == 0.0 && n.abs() < 9.0e18 => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().and_then(|i| u64::try_from(i).ok())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// A list of three numbers.
    pub fn as_vec3(&self) -> Option<Vec3> {
        match self.as_list()? {
            [x, y, z] => Some(Vec3::new(x.as_f32()?, y.as_f32()?, z.as_f32()?)),
            _ => None,
        }
    }

    /// The canonical bytes (see the module docs).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        self.encode_into(&mut out);
        out
    }

    pub fn encode_into(&self, out: &mut Vec<u8>) {
        match self {
            Value::Null => out.push(0),
            Value::Bool(false) => out.push(1),
            Value::Bool(true) => out.push(2),
            Value::Int(i) => {
                out.push(3);
                out.extend_from_slice(&i.to_le_bytes());
            }
            Value::Num(n) => {
                out.push(4);
                out.extend_from_slice(&n.to_bits().to_le_bytes());
            }
            Value::Str(s) => {
                out.push(5);
                put_str(out, s);
            }
            Value::List(items) => {
                out.push(6);
                out.extend_from_slice(&len32(items.len()).to_le_bytes());
                for item in items {
                    item.encode_into(out);
                }
            }
            Value::Map(m) => {
                out.push(7);
                out.extend_from_slice(&len32(m.len()).to_le_bytes());
                for (k, v) in m {
                    put_str(out, k);
                    v.encode_into(out);
                }
            }
        }
    }

    /// Reads one value that fills `bytes` exactly.
    pub fn decode(bytes: &[u8]) -> Result<Value, DecodeError> {
        let mut r = Reader { bytes, at: 0 };
        let v = r.value(0)?;
        if r.at != bytes.len() {
            return Err(DecodeError { offset: r.at, code: "trailing" });
        }
        Ok(v)
    }

    /// As JSON (ints above 2^53 lose precision there).
    pub fn to_json(&self) -> Json {
        match self {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Int(i) => Json::Num(*i as f64),
            Value::Num(n) if n.is_finite() => Json::Num(*n),
            // JSON has no NaN or infinity.
            Value::Num(_) => Json::Null,
            Value::Str(s) => Json::Str(s.clone()),
            Value::List(l) => Json::Arr(l.iter().map(Value::to_json).collect()),
            Value::Map(m) => Json::Obj(m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()),
        }
    }

    /// From JSON: whole numbers within ±2^53 become ints, the rest nums; duplicate object keys
    /// keep the last.
    pub fn from_json(json: &Json) -> Value {
        match json {
            Json::Null => Value::Null,
            Json::Bool(b) => Value::Bool(*b),
            Json::Num(n) if specula_core::dmath::fract_f64(*n) == 0.0 && n.abs() <= 9_007_199_254_740_992.0 => Value::Int(*n as i64),
            Json::Num(n) => Value::Num(*n),
            Json::Str(s) => Value::Str(s.clone()),
            Json::Arr(a) => Value::List(a.iter().map(Value::from_json).collect()),
            Json::Obj(o) => Value::Map(o.iter().map(|(k, v)| (k.clone(), Value::from_json(v))).collect()),
        }
    }
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).expect("a value part longer than 4 GiB")
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&len32(s.len()).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.bytes.len());
        let end = end.ok_or(DecodeError { offset: self.at, code: "truncated" })?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let n = self.u32()? as usize;
        let at = self.at;
        let b = self.take(n)?;
        std::str::from_utf8(b).map(str::to_owned).map_err(|_| DecodeError { offset: at, code: "bad_utf8" })
    }

    /// A count of items that each take at least one byte: more than the bytes left is a lie
    /// (and would otherwise allocate without bound).
    fn count(&mut self) -> Result<usize, DecodeError> {
        let at = self.at;
        let n = self.u32()? as usize;
        if n > self.bytes.len() - self.at {
            return Err(DecodeError { offset: at, code: "truncated" });
        }
        Ok(n)
    }

    fn value(&mut self, depth: usize) -> Result<Value, DecodeError> {
        if depth > MAX_DEPTH {
            return Err(DecodeError { offset: self.at, code: "too_deep" });
        }
        let at = self.at;
        let tag = self.take(1)?[0];
        Ok(match tag {
            0 => Value::Null,
            1 => Value::Bool(false),
            2 => Value::Bool(true),
            3 => Value::Int(self.u64()? as i64),
            4 => Value::Num(f64::from_bits(self.u64()?)),
            5 => Value::Str(self.string()?),
            6 => {
                let n = self.count()?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                Value::List(items)
            }
            7 => {
                let n = self.count()?;
                let mut m = BTreeMap::new();
                let mut last: Option<String> = None;
                for _ in 0..n {
                    let key_at = self.at;
                    let k = self.string()?;
                    if last.as_ref().is_some_and(|l| *l >= k) {
                        return Err(DecodeError { offset: key_at, code: "key_order" });
                    }
                    let v = self.value(depth + 1)?;
                    last = Some(k.clone());
                    m.insert(k, v);
                }
                Value::Map(m)
            }
            _ => return Err(DecodeError { offset: at, code: "bad_tag" }),
        })
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Value {
        Value::Int(i64::from(v))
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Value {
        Value::Int(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Value {
        Value::Int(i64::from(v))
    }
}
impl From<u64> for Value {
    /// Above `i64::MAX`: a num (rounded).
    fn from(v: u64) -> Value {
        i64::try_from(v).map_or(Value::Num(v as f64), Value::Int)
    }
}
impl From<usize> for Value {
    fn from(v: usize) -> Value {
        Value::from(v as u64)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Value {
        Value::Num(f64::from(v))
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Num(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Str(v.to_owned())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Str(v)
    }
}
impl From<Vec3> for Value {
    fn from(v: Vec3) -> Value {
        Value::List(vec![v.x.into(), v.y.into(), v.z.into()])
    }
}
impl From<[f32; 2]> for Value {
    fn from(v: [f32; 2]) -> Value {
        Value::List(vec![v[0].into(), v[1].into()])
    }
}
impl From<[f32; 3]> for Value {
    fn from(v: [f32; 3]) -> Value {
        Value::List(v.iter().map(|x| Value::from(*x)).collect())
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Value {
        Value::List(v)
    }
}
impl From<Vec<&str>> for Value {
    fn from(v: Vec<&str>) -> Value {
        Value::List(v.into_iter().map(Value::from).collect())
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Value {
        v.map_or(Value::Null, Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        Value::map()
            .with("op", "spawn")
            .with("position", Vec3::new(1.0, -2.5, 3.0))
            .with("count", 7)
            .with("big", u64::MAX)
            .with("nan", f64::NAN)
            .with("tags", vec!["a", "b"])
            .with("none", Value::Null)
            .with("nested", Value::map().with("ok", true).with("neg", -0.0f64).with("s", "é✓"))
    }

    #[test]
    fn round_trips_exactly() {
        let v = sample();
        let bytes = v.encode();
        let back = Value::decode(&bytes).unwrap();
        // NaN != NaN, so compare the canonical bytes (exact bits).
        assert_eq!(back.encode(), bytes);
        assert_eq!(back.get("op").and_then(Value::as_str), Some("spawn"));
        assert_eq!(back.get("position").and_then(Value::as_vec3), Some(Vec3::new(1.0, -2.5, 3.0)));
        assert_eq!(back.get("nested").and_then(|n| n.get("neg")).and_then(Value::as_f64).map(f64::to_bits), Some((-0.0f64).to_bits()));
    }

    #[test]
    fn encoding_is_canonical() {
        let a = Value::map().with("b", 1).with("a", 2);
        let b = Value::map().with("a", 2).with("b", 1);
        assert_eq!(a.encode(), b.encode());
    }

    #[test]
    fn every_truncation_and_corruption_is_an_error_not_a_panic() {
        let bytes = sample().encode();
        for n in 0..bytes.len() {
            assert!(Value::decode(&bytes[..n]).is_err(), "prefix {n}");
        }
        // Flip every byte to every interesting value: never panics, never allocates wildly.
        for i in 0..bytes.len() {
            for b in [0u8, 7, 8, 0x7f, 0xff] {
                let mut c = bytes.clone();
                c[i] = b;
                let _ = Value::decode(&c);
            }
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert_eq!(Value::decode(&extra).unwrap_err().code, "trailing");
    }

    #[test]
    fn huge_counts_and_deep_nesting_are_refused() {
        let mut lie = vec![6u8];
        lie.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Value::decode(&lie).unwrap_err().code, "truncated");
        let mut deep = Vec::new();
        for _ in 0..(MAX_DEPTH + 2) {
            deep.push(6u8);
            deep.extend_from_slice(&1u32.to_le_bytes());
        }
        deep.push(0);
        assert_eq!(Value::decode(&deep).unwrap_err().code, "too_deep");
    }

    #[test]
    fn unsorted_or_duplicate_keys_are_refused() {
        let mut b = vec![7u8];
        b.extend_from_slice(&2u32.to_le_bytes());
        for k in ["b", "a"] {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(k.as_bytes());
            b.push(0);
        }
        assert_eq!(Value::decode(&b).unwrap_err().code, "key_order");
    }

    #[test]
    fn json_conversion() {
        let j = Json::parse(r#"{"a": 1, "b": 2.5, "c": [true, null, "x"], "d": {"e": -3}}"#).unwrap();
        let v = Value::from_json(&j);
        assert_eq!(v.get("a"), Some(&Value::Int(1)));
        assert_eq!(v.get("b"), Some(&Value::Num(2.5)));
        assert_eq!(v.to_json(), j);
        assert_eq!(Value::Num(f64::INFINITY).to_json(), Json::Null);
    }
}

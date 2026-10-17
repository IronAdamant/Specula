//! `response_format`: concise or detailed answers from the heavier tools.
//!
//! Tokens are a cost (every agent in a swarm pays for every byte it reads), so the heavy tools
//! answer **concisely by default**: what an agent acts on (ids, paths, counts, status, problems),
//! without what it rarely needs (per-pass tables, full transforms, per-item metadata).
//! `response_format: "detailed"` gives the full answer, exactly as before. See
//! `docs/design/TOOLS.md` for the per-tool choices and measured savings.

use specula_core::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Concise,
    Detailed,
}

pub const KEY: &str = "response_format";

impl Format {
    /// `response_format` from a tool's arguments (`default` when absent).
    pub fn arg(args: &Json, default: Format) -> Result<Format, String> {
        match args.get(KEY) {
            None | Some(Json::Null) => Ok(default),
            Some(Json::Str(s)) if s == "concise" => Ok(Format::Concise),
            Some(Json::Str(s)) if s == "detailed" => Ok(Format::Detailed),
            Some(_) => Err(format!("'{KEY}' must be \"concise\" or \"detailed\"")),
        }
    }

    pub fn detailed(self) -> bool {
        self == Format::Detailed
    }

    /// The schema property every tool with a format uses.
    pub fn schema() -> Json {
        Json::obj([
            ("type", Json::from("string")),
            ("enum", Json::arr([Json::from("concise"), Json::from("detailed")])),
            ("description", Json::from("concise (default): what you act on, fewer tokens; detailed: everything")),
        ])
    }
}

/// Keeps only `keys` of an object (in its own order); anything else passes through.
pub fn keep(v: &Json, keys: &[&str]) -> Json {
    match v {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| keys.contains(&k.as_str())).cloned().collect()),
        other => other.clone(),
    }
}

/// Drops `keys` from an object; anything else passes through.
pub fn drop_keys(v: &Json, keys: &[&str]) -> Json {
    match v {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| !keys.contains(&k.as_str())).cloned().collect()),
        other => other.clone(),
    }
}

/// Numbers rounded to `places` decimals, recursively (concise answers don't need 17 digits).
pub fn round(v: &Json, places: i32) -> Json {
    let f = 10f64.powi(places);
    match v {
        Json::Num(n) if n.fract() != 0.0 => Json::Num((n * f).round() / f),
        Json::Arr(a) => Json::Arr(a.iter().map(|x| round(x, places)).collect()),
        Json::Obj(o) => Json::Obj(o.iter().map(|(k, x)| (k.clone(), round(x, places))).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_parse_and_shape() {
        let a = Json::parse(r#"{"response_format": "detailed"}"#).unwrap();
        assert_eq!(Format::arg(&a, Format::Concise), Ok(Format::Detailed));
        assert_eq!(Format::arg(&Json::Null, Format::Concise), Ok(Format::Concise));
        assert!(Format::arg(&Json::parse(r#"{"response_format": "short"}"#).unwrap(), Format::Concise).is_err());
        let v = Json::parse(r#"{"a": 1.23456, "b": [0.5, 2], "c": {"d": 1.0}}"#).unwrap();
        assert_eq!(round(&v, 2).to_string(), r#"{"a":1.23,"b":[0.5,2],"c":{"d":1}}"#);
        assert_eq!(keep(&v, &["c", "a"]).to_string(), r#"{"a":1.23456,"c":{"d":1}}"#);
        assert_eq!(drop_keys(&v, &["a", "b"]).to_string(), r#"{"c":{"d":1}}"#);
    }
}

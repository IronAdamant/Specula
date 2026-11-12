//! "Highlight and ask" (docs/design/EDITOR.md, "Highlight and ask"): the human highlights an
//! entity, a part of one or an area in the editor, types a request into the engine-owned "Ask the
//! agents…" box, and Send posts a board task carrying the request, the selection by identity
//! ([`crate::entity_ref::EntityRef`]) and **part descriptors** ([`Part`]): which part of what the
//! request is about. Agents claim the task, edit through the normal tools and submit to the
//! review queue; the human decides in the review screen.
//!
//! This module is the data: part descriptors as typed values with their limits, parsed from and
//! shown as JSON (`task_post {parts}`, `task_get`, `board.json`). The box itself is in
//! `editor_ask`.

use specula_core::Json;

/// The most part descriptors one task carries.
pub const MAX_PARTS: usize = 16;
/// A recipe part's name, a timeline's name: at most this many characters.
pub const MAX_NAME: usize = 128;
/// A text span's end, in bytes (the text component's own limit).
pub const MAX_SPAN_BYTES: u32 = crate::text_world::MAX_TEXT_BYTES as u32;
/// An area's radius and a rail's times: at most this much (metres, seconds).
pub const MAX_RADIUS_M: f32 = 10_000.0;
pub const MAX_RAIL_S: f32 = 86_400.0;
/// Coordinates: within ±this (metres; the world's own bound is far wider, this is a sanity cap).
pub const MAX_COORD_M: f32 = 1.0e7;
/// The request typed into the box: at most this many characters.
pub const MAX_REQUEST: usize = 2_000;
/// What a task made by the box is tagged with (agents find them with `task_list {tag: "ask"}`).
pub const TAG: &str = "ask";

/// Which part of what the request is about.
#[derive(Clone, Debug, PartialEq)]
pub enum PartKind {
    /// A span of a text entity's words: bytes `[start, end)`.
    TextSpan { start: u32, end: u32 },
    /// A named part of the entity's model recipe (`parts[].name`).
    RecipePart { name: String },
    /// A terrain or grass patch: a circle on the ground (world metres).
    Area { center: [f32; 3], radius: f32 },
    /// A stretch of a timeline's rail (the timeline's project name), seconds `[from, to]`.
    Rail { timeline: String, from_s: f32, to_s: f32 },
}

/// One part descriptor: its kind, and the entity it is a part of (an index into the task's
/// `entities`; `None` for an area or a rail, which need none).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub entity: Option<usize>,
    pub kind: PartKind,
}

impl PartKind {
    pub fn name(&self) -> &'static str {
        match self {
            PartKind::TextSpan { .. } => "text_span",
            PartKind::RecipePart { .. } => "recipe_part",
            PartKind::Area { .. } => "area",
            PartKind::Rail { .. } => "rail",
        }
    }
}

impl Part {
    pub fn to_json(&self) -> Json {
        let mut f: Vec<(&str, Json)> = vec![("kind", Json::from(self.kind.name()))];
        if let Some(e) = self.entity {
            f.push(("entity", Json::from(e)));
        }
        match &self.kind {
            PartKind::TextSpan { start, end } => {
                f.push(("start", Json::from(u64::from(*start))));
                f.push(("end", Json::from(u64::from(*end))));
            }
            PartKind::RecipePart { name } => f.push(("name", Json::from(name.as_str()))),
            PartKind::Area { center, radius } => {
                f.push(("center", Json::arr(center.map(|c| Json::from(f64::from(c))))));
                f.push(("radius", Json::from(f64::from(*radius))));
            }
            PartKind::Rail { timeline, from_s, to_s } => {
                f.push(("timeline", Json::from(timeline.as_str())));
                f.push(("from_s", Json::from(f64::from(*from_s))));
                f.push(("to_s", Json::from(f64::from(*to_s))));
            }
        }
        Json::obj(f)
    }

    /// In words, for the board's detail and the editor's lists.
    pub fn describe(&self) -> String {
        match &self.kind {
            PartKind::TextSpan { start, end } => format!("words (bytes {start}..{end})"),
            PartKind::RecipePart { name } => format!("part \"{name}\""),
            PartKind::Area { center, radius } => format!("area r={radius:.2} m at ({:.2}, {:.2}, {:.2})", center[0], center[1], center[2]),
            PartKind::Rail { timeline, from_s, to_s } => format!("rail of {timeline}, {from_s:.2}..{to_s:.2} s"),
        }
    }

    /// A rough size in bytes (board budgets).
    pub fn measure(&self) -> usize {
        48 + match &self.kind {
            PartKind::RecipePart { name } => name.len(),
            PartKind::Rail { timeline, .. } => timeline.len(),
            _ => 0,
        }
    }

    /// Reads one descriptor (`task_post {parts}` or a saved board). `entities`: how many entities
    /// the task names (an `entity` index must be below it). Errors say which field and why.
    pub fn parse(j: &Json, entities: usize) -> Result<Part, String> {
        let Json::Obj(fields) = j else { return Err("a part is an object {kind, ...}".into()) };
        let kind = j.get("kind").and_then(Json::as_str).ok_or("a part needs 'kind': text_span, recipe_part, area or rail")?;
        let allowed: &[&str] = match kind {
            "text_span" => &["kind", "entity", "start", "end"],
            "recipe_part" => &["kind", "entity", "name"],
            "area" => &["kind", "entity", "center", "radius"],
            "rail" => &["kind", "entity", "timeline", "from_s", "to_s"],
            other => return Err(format!("unknown part kind '{}': text_span, recipe_part, area or rail", crate::bounds::echo(other))),
        };
        if let Some((k, _)) = fields.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
            return Err(format!("a {kind} part has no field '{}' (it takes {})", crate::bounds::echo(k), allowed.join(", ")));
        }
        let entity = match j.get("entity") {
            None | Some(Json::Null) => None,
            Some(v) => {
                let i = v.as_u64().ok_or("'entity' is an index into the task's entities")? as usize;
                if i >= entities {
                    return Err(format!("'entity' {i} is past the task's {entities} entities"));
                }
                Some(i)
            }
        };
        let num = |k: &str| -> Result<f32, String> {
            let v = j.get(k).and_then(Json::as_f64).ok_or(format!("a {kind} part needs a number '{k}'"))?;
            if !v.is_finite() {
                return Err(format!("'{k}' must be finite"));
            }
            Ok(v as f32)
        };
        let name = |k: &str| -> Result<String, String> {
            let s = j.get(k).and_then(Json::as_str).ok_or(format!("a {kind} part needs a string '{k}'"))?;
            if s.trim().is_empty() || s.chars().count() > MAX_NAME || s.chars().any(char::is_control) {
                return Err(format!("'{k}' must be 1 to {MAX_NAME} characters, no control characters"));
            }
            Ok(s.to_owned())
        };
        let kind = match kind {
            "text_span" => {
                let (start, end) = (num("start")?, num("end")?);
                if start < 0.0 || end < start || end > MAX_SPAN_BYTES as f32 || start.fract() != 0.0 || end.fract() != 0.0 {
                    return Err(format!("a text span is whole bytes 0 <= start <= end <= {MAX_SPAN_BYTES}"));
                }
                if entity.is_none() {
                    return Err("a text span needs 'entity' (the text it is part of)".into());
                }
                PartKind::TextSpan { start: start as u32, end: end as u32 }
            }
            "recipe_part" => {
                if entity.is_none() {
                    return Err("a recipe part needs 'entity' (the model it is part of)".into());
                }
                PartKind::RecipePart { name: name("name")? }
            }
            "area" => {
                let c = j.get("center").and_then(Json::as_array).filter(|a| a.len() == 3).ok_or("'center' is [x, y, z]")?;
                let mut center = [0.0f32; 3];
                for (i, v) in c.iter().enumerate() {
                    let v = v.as_f64().filter(|v| v.is_finite() && v.abs() <= f64::from(MAX_COORD_M)).ok_or(format!("'center' components are finite, within ±{MAX_COORD_M} m"))?;
                    center[i] = v as f32;
                }
                let radius = num("radius")?;
                if radius <= 0.0 || radius > MAX_RADIUS_M {
                    return Err(format!("'radius' must be above 0 and at most {MAX_RADIUS_M} m"));
                }
                PartKind::Area { center, radius }
            }
            _ => {
                let (from_s, to_s) = (num("from_s")?, num("to_s")?);
                if from_s < 0.0 || to_s < from_s || to_s > MAX_RAIL_S {
                    return Err(format!("a rail stretch is 0 <= from_s <= to_s <= {MAX_RAIL_S} s"));
                }
                PartKind::Rail { timeline: name("timeline")?, from_s, to_s }
            }
        };
        Ok(Part { entity, kind })
    }

    /// Reads `parts` (an array, at most [`MAX_PARTS`]); errors name the index.
    pub fn parse_all(j: Option<&Json>, entities: usize) -> Result<Vec<Part>, String> {
        let Some(j) = j.filter(|j| !matches!(j, Json::Null)) else { return Ok(Vec::new()) };
        let a = j.as_array().ok_or("'parts' is an array of part descriptors")?;
        if a.len() > MAX_PARTS {
            return Err(format!("'parts' has {} items; at most {MAX_PARTS}", a.len()));
        }
        a.iter().enumerate().map(|(i, p)| Part::parse(p, entities).map_err(|e| format!("parts[{i}]: {e}"))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str, n: usize) -> Result<Part, String> {
        Part::parse(&Json::parse(s).unwrap(), n)
    }

    #[test]
    fn parts_round_trip_and_name_themselves() {
        for s in [
            r#"{"kind":"text_span","entity":0,"start":3,"end":9}"#,
            r#"{"kind":"recipe_part","entity":1,"name":"head"}"#,
            r#"{"kind":"area","center":[1.5,0,-2],"radius":3}"#,
            r#"{"kind":"rail","timeline":"intro","from_s":2,"to_s":4.5}"#,
        ] {
            let part = p(s, 2).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(Part::parse(&part.to_json(), 2).unwrap(), part, "{s}");
            assert!(!part.describe().is_empty());
        }
    }

    /// Hostile input is refused with a reason, never a panic, and caps hold.
    #[test]
    fn bad_parts_are_refused_with_a_reason() {
        for (s, why) in [
            (r#"{"kind":"text_span","entity":0,"start":9,"end":3}"#, "start <= end"),
            (r#"{"kind":"text_span","start":0,"end":3}"#, "needs 'entity'"),
            (r#"{"kind":"text_span","entity":0,"start":0,"end":99999}"#, "<= 2048"),
            (r#"{"kind":"text_span","entity":0,"start":0.5,"end":3}"#, "whole bytes"),
            (r#"{"kind":"recipe_part","entity":5,"name":"head"}"#, "past the task's"),
            (r#"{"kind":"recipe_part","entity":0,"name":""}"#, "1 to 128"),
            (r#"{"kind":"area","center":[0,0],"radius":1}"#, "[x, y, z]"),
            (r#"{"kind":"area","center":[0,0,1e30],"radius":1}"#, "within"),
            (r#"{"kind":"area","center":[0,0,0],"radius":0}"#, "above 0"),
            (r#"{"kind":"rail","timeline":"t","from_s":5,"to_s":1}"#, "from_s <= to_s"),
            (r#"{"kind":"blob"}"#, "unknown part kind"),
            (r#"{"kind":"area","center":[0,0,0],"radius":1,"extra":1}"#, "no field 'extra'"),
            ("[]", "an object"),
        ] {
            let e = p(s, 1).expect_err(s);
            assert!(e.contains(why), "{s}: {e}");
        }
        let many = Json::arr((0..=MAX_PARTS).map(|_| Json::parse(r#"{"kind":"area","center":[0,0,0],"radius":1}"#).unwrap()));
        assert!(Part::parse_all(Some(&many), 0).unwrap_err().contains("at most"));
        assert_eq!(Part::parse_all(None, 0), Ok(vec![]));
        let long = format!(r#"{{"kind":"recipe_part","entity":0,"name":"{}"}}"#, "x".repeat(MAX_NAME + 1));
        assert!(p(&long, 1).is_err());
    }
}

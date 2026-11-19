//! Critique against the bible, as data: the findings of one subject grouped into the taste
//! loop's passes (silhouette, form, value, colour, scale, budget, naming), each with its measures, a
//! verdict and the bible's rules for the subject's category, plus the revise hints in the order
//! the loop takes them (silhouette first: if it doesn't read as a silhouette, detail won't save
//! it).

use crate::rules::StyleRules;
use crate::Finding;
use specula_core::Json;

/// The passes, in loop order, and the checks each one owns.
pub const PASSES: [(&str, &[&str]); 7] = [
    ("silhouette", &["silhouette_distinct", "thumb_solidity", "px_gameplay", "px_phone", "shape_language"]),
    // (The mesh's own form, after the outline and before the light: a piece that floats or
    // an edge that cuts is mended in the shape, not in its colours.)
    ("form", &["loose_pieces", "razor_edges", "shading_seams", "smeared_creases", "small_relief", "faceting"]),
    ("value", &["value_contrast", "value_contrast_worst", "flat_lighting"]),
    ("colour", &["palette_distance", "neutral_saturation", "family_forbidden", "emissive_preset"]),
    ("scale", &["height", "width", "depth", "aspect"]),
    ("budget", &["triangles", "triangles_min", "lod_triangles", "texture_kb", "materials"]),
    ("naming", &["name_case", "name_length", "name_file", "category", "parts_named"]),
];

/// Which pass a bible aspect belongs to (by its key: `shape`, `silhouette`, `colour`, ...).
fn pass_of_aspect(aspect: &str) -> &'static str {
    match aspect {
        "shape" | "silhouette" | "outline" | "tier" => "silhouette",
        "form" | "edges" | "surface" => "form",
        "value" | "light" | "lighting" | "contrast" => "value",
        "colour" | "color" | "palette" | "emissive" | "material" | "materials" => "colour",
        "proportions" | "size" | "scale" => "scale",
        "budget" | "triangles" => "budget",
        _ => "general",
    }
}

/// The critique of `subject` (category `category`) from its findings.
pub fn critique(subject: &str, category: Option<&str>, findings: &[&Finding], rules: &StyleRules) -> Json {
    let bible = rules.bible_for(category);
    let mut passes = Vec::new();
    let mut next = Vec::new();
    for (pass, checks) in PASSES {
        let mine: Vec<&&Finding> = findings.iter().filter(|f| checks.contains(&f.check)).collect();
        let quotes: Vec<Json> = bible.iter().filter(|(k, _)| pass_of_aspect(k) == pass).map(|(k, t)| Json::obj([("rule", Json::from(k.as_str())), ("text", Json::from(t.as_str()))])).collect();
        if mine.is_empty() && quotes.is_empty() {
            continue;
        }
        let failed: Vec<&&&Finding> = mine.iter().filter(|f| f.is_error()).collect();
        for f in &failed {
            next.push(Json::from(format!("{pass}: {} ({}): {}", f.check, f.message, f.fix)));
        }
        let r = |x: f64| Json::from((x * 1000.0).round() / 1000.0);
        let measures = Json::arr(mine.iter().map(|f| {
            let mut j = Json::obj([("check", Json::from(f.check)), ("value", r(f.value)), ("limit", r(f.limit)), ("passed", Json::from(f.passed))]);
            if f.exempt {
                j.set("exempt", Json::from(true));
            }
            j
        }));
        let verdict = if mine.is_empty() { "not measured" } else if failed.is_empty() { "ok" } else { "revise" };
        let mut p = Json::obj([("pass", Json::from(pass)), ("verdict", Json::from(verdict)), ("measures", measures)]);
        if !quotes.is_empty() {
            p.set("bible", Json::Arr(quotes));
        }
        passes.push(p);
    }
    let general: Vec<Json> = bible.iter().filter(|(k, _)| pass_of_aspect(k) == "general").map(|(k, t)| Json::obj([("rule", Json::from(k.as_str())), ("text", Json::from(t.as_str()))])).collect();
    let mut j = Json::obj([
        ("subject", Json::from(subject)),
        ("category", category.map(Json::from).unwrap_or(Json::Null)),
        ("verdict", Json::from(if next.is_empty() { "ok" } else { "revise" })),
        ("passes", Json::Arr(passes)),
        ("next", Json::Arr(next)),
    ]);
    if !general.is_empty() {
        j.set("bible_general", Json::Arr(general));
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bound;

    #[test]
    fn passes_quote_the_bible_and_order_the_hints() {
        let rules = StyleRules::parse("[bible.stone]\nshape = \"Vertical monoliths; each type has a distinct top\"\ncolour = \"Carved grey; runes glow cyan\"\nmood = \"Board-game pieces\"\n", "style.toml", &|_| None).unwrap();
        let tri = Finding::measure("triangles", "s", 2000.0, 1500.0, Bound::Max, "triangles", "budget[0]".into(), "too many".into(), "fewer".into());
        let sil = Finding::measure("silhouette_distinct", "s", 0.95, 0.9, Bound::Max, "iou", "taste".into(), "alike".into(), "a new top".into());
        let ok = Finding::measure("value_contrast", "s", 0.2, 0.05, Bound::Min, "l", "taste".into(), String::new(), String::new());
        let c = critique("s", Some("stone"), &[&tri, &sil, &ok], &rules);
        assert_eq!(c.get("verdict").and_then(Json::as_str), Some("revise"));
        let next: Vec<&str> = c.get("next").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).collect();
        assert!(next[0].starts_with("silhouette:") && next[1].starts_with("budget:"), "{next:?}");
        let passes = c.get("passes").and_then(Json::as_array).unwrap();
        let sil_pass = &passes[0];
        assert_eq!(sil_pass.get("bible").and_then(Json::as_array).map(|b| b.len()), Some(1));
        let colour = passes.iter().find(|p| p.get("pass").and_then(Json::as_str) == Some("colour")).unwrap();
        assert_eq!(colour.get("verdict").and_then(Json::as_str), Some("not measured"));
        assert!(c.get("bible_general").is_some());
    }
}

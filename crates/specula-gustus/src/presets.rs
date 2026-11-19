//! Base styles and the first-run style configurator (owner, 2026-10-16): a new project starts
//! from a preset; galleries ask how the game should look (the base style, the key light, the
//! atmosphere, the interface colours including "can't afford"); the accepted values become a
//! `style.toml` and STYLE.md-style notes that a human approves before anything is written.

use crate::space::Value;
use crate::{tomledit, Result, TasteError};
use specula_core::Json;

pub const ONBOARDING_FORMAT: &str = "specula.taste.onboarding.v1";

/// A base style: the stage's game light, the atmosphere, the ground and sky, interface colours
/// and how colourful neutrals may be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub label: &'static str,
    pub about: &'static str,
    pub key_color: &'static str,
    pub key_intensity: f64,
    pub key_elevation_deg: f64,
    pub key_azimuth_deg: f64,
    pub ambient_color: &'static str,
    pub ambient_intensity: f64,
    pub exposure: f64,
    pub bloom: bool,
    pub shadows: bool,
    pub background: &'static str,
    pub ground: &'static str,
    pub fog_density: f64,
    pub fog_color: &'static str,
    pub panel: &'static str,
    pub text: &'static str,
    pub accent: &'static str,
    pub danger: &'static str,
    pub cant_afford: &'static str,
    pub neutral_max_chroma: f64,
    /// The shape language the style asks of an asset's outline, and the share of the
    /// outline's turning that must be of it (`[taste] shape_language`, `shape_min_share`:
    /// docs/design/FORM.md). `None`: the style does not say, and shapes are only reported.
    pub shape: Option<(&'static str, f64)>,
}

pub const PRESETS: [Preset; 5] = [
    Preset {
        name: "moonlit",
        label: "Moonlit",
        about: "Night under a low blue moon: a cool fill does most of the lighting, glows read, fog in the distance (Sepulcra's look).",
        key_color: "#d0d9f0",
        key_intensity: 0.55,
        key_elevation_deg: 30.0,
        key_azimuth_deg: 25.0,
        ambient_color: "#2f3850",
        ambient_intensity: 1.7,
        exposure: 1.0,
        bloom: true,
        shadows: true,
        background: "#0b1020",
        ground: "#3a3f48",
        fog_density: 0.012,
        fog_color: "#2a3448",
        panel: "#2b3038",
        text: "#ece6d6",
        accent: "#3fe0d0",
        danger: "#9cff4f",
        cant_afford: "#8e897d",
        neutral_max_chroma: 0.05,
        shape: None,
    },
    Preset {
        name: "storybook",
        label: "Storybook",
        about: "Bright, warm daylight with saturated colour and soft blue shade: friendly and readable.",
        key_color: "#fff1d6",
        key_intensity: 1.3,
        key_elevation_deg: 50.0,
        key_azimuth_deg: -35.0,
        ambient_color: "#9fc4ff",
        ambient_intensity: 0.8,
        exposure: 1.05,
        bloom: false,
        shadows: true,
        background: "#8fd0ff",
        ground: "#7cc46a",
        fog_density: 0.004,
        fog_color: "#cfe8ff",
        panel: "#fff6e6",
        text: "#2a2230",
        accent: "#ff7a59",
        danger: "#e5484d",
        cant_afford: "#9a8f99",
        neutral_max_chroma: 0.08,
        // (Friendly reads as round, and this preset asked for it until the measure's third
        // review: a ball with a string on it read as pointed, and a crate with wide bevels as
        // round. No preset asks for a shape until the measure is settled: FORM.md.)
        shape: None,
    },
    Preset {
        name: "muted",
        label: "Muted",
        about: "Overcast and low in chroma: earthy, calm, colour saved for what matters.",
        key_color: "#e8e4dc",
        key_intensity: 0.8,
        key_elevation_deg: 35.0,
        key_azimuth_deg: -20.0,
        ambient_color: "#b8bcc4",
        ambient_intensity: 0.9,
        exposure: 0.95,
        bloom: false,
        shadows: true,
        background: "#9aa0a8",
        ground: "#6f7266",
        fog_density: 0.01,
        fog_color: "#a8adb3",
        panel: "#3a3c40",
        text: "#f2f0ea",
        accent: "#c9a86a",
        danger: "#c0504d",
        cant_afford: "#7d7a74",
        neutral_max_chroma: 0.035,
        shape: None,
    },
    Preset {
        name: "noir",
        label: "Noir",
        about: "Hard white key light against near black: strong silhouettes, almost no colour.",
        key_color: "#ffffff",
        key_intensity: 1.6,
        key_elevation_deg: 25.0,
        key_azimuth_deg: 60.0,
        ambient_color: "#20232a",
        ambient_intensity: 0.35,
        exposure: 1.0,
        bloom: true,
        shadows: true,
        background: "#050608",
        ground: "#1a1b1e",
        fog_density: 0.02,
        fog_color: "#101216",
        panel: "#141518",
        text: "#f5f5f5",
        accent: "#e0b84a",
        danger: "#dd3333",
        cant_afford: "#6b6b6b",
        neutral_max_chroma: 0.02,
        shape: None,
    },
    Preset {
        name: "clean",
        label: "Clean",
        about: "Neutral daylight on grey: judge form and colour as they are (the studio's neutral rig).",
        key_color: "#ffffff",
        key_intensity: 1.0,
        key_elevation_deg: 45.0,
        key_azimuth_deg: -45.0,
        ambient_color: "#ffffff",
        ambient_intensity: 0.6,
        exposure: 1.0,
        bloom: false,
        shadows: true,
        background: "#c8ccd2",
        ground: "#9a9ea4",
        fog_density: 0.0,
        fog_color: "#c8ccd2",
        panel: "#f4f5f7",
        text: "#1d2026",
        accent: "#2f7cf6",
        danger: "#d64545",
        cant_afford: "#9aa1ab",
        neutral_max_chroma: 0.05,
        shape: None,
    },
];

pub fn find(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.name).collect()
}

impl Preset {
    /// The look as data (what the engine renders a preset with).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("name", Json::from(self.name)),
            ("label", Json::from(self.label)),
            ("about", Json::from(self.about)),
            ("key_color", Json::from(self.key_color)),
            ("key_intensity", Json::from(self.key_intensity)),
            ("key_elevation_deg", Json::from(self.key_elevation_deg)),
            ("key_azimuth_deg", Json::from(self.key_azimuth_deg)),
            ("ambient_color", Json::from(self.ambient_color)),
            ("ambient_intensity", Json::from(self.ambient_intensity)),
            ("exposure", Json::from(self.exposure)),
            ("bloom", Json::Bool(self.bloom)),
            ("shadows", Json::Bool(self.shadows)),
            ("background", Json::from(self.background)),
            ("ground", Json::from(self.ground)),
            ("fog_density", Json::from(self.fog_density)),
            ("fog_color", Json::from(self.fog_color)),
            ("ui", Json::obj([("panel", Json::from(self.panel)), ("text", Json::from(self.text)), ("accent", Json::from(self.accent)), ("danger", Json::from(self.danger)), ("cant_afford", Json::from(self.cant_afford))])),
            ("neutral_max_chroma", Json::from(self.neutral_max_chroma)),
            ("shape", self.shape.map_or(Json::Null, |(kind, share)| Json::obj([("language", Json::from(kind)), ("min_share", Json::from(share))]))),
        ])
    }

    /// The `style.toml` keys a preset sets (dotted path, TOML literal).
    pub fn toml_keys(&self) -> Vec<(String, String)> {
        let q = |s: &str| format!("\"{s}\"");
        let n = |v: f64| Value::Num(v).toml_literal();
        let mut keys = vec![
            ("stage.game.key_color".into(), q(self.key_color)),
            ("stage.game.key_intensity".into(), n(self.key_intensity)),
            ("stage.game.key_elevation_deg".into(), n(self.key_elevation_deg)),
            ("stage.game.key_azimuth_deg".into(), n(self.key_azimuth_deg)),
            ("stage.game.ambient_color".into(), q(self.ambient_color)),
            ("stage.game.ambient_intensity".into(), n(self.ambient_intensity)),
            ("stage.game.exposure".into(), n(self.exposure)),
            ("stage.game.bloom".into(), self.bloom.to_string()),
            ("stage.game.shadows".into(), self.shadows.to_string()),
            ("stage.game.background".into(), q(self.background)),
            ("stage.game.ground".into(), q(self.ground)),
            ("look.preset".into(), q(self.name)),
            ("look.fog_density".into(), n(self.fog_density)),
            ("look.fog_color".into(), q(self.fog_color)),
            ("ui.panel".into(), q(self.panel)),
            ("ui.text".into(), q(self.text)),
            ("ui.accent".into(), q(self.accent)),
            ("ui.danger".into(), q(self.danger)),
            ("ui.cant_afford".into(), q(self.cant_afford)),
        ];
        if let Some((kind, share)) = self.shape {
            keys.push(("taste.shape_language".into(), q(kind)));
            keys.push(("taste.shape_min_share".into(), n(share)));
        }
        keys
    }
}

/// The onboarding steps, in order.
pub const STEPS: [&str; 4] = ["base", "light", "atmosphere", "interface"];

/// One step's gallery: its title, subject (the project's own, with the look it starts from)
/// and parameter space.
pub fn step_gallery(step: usize, preset: &Preset, subject: &Json) -> Option<(String, Json, Json)> {
    let with_look = |s: &Json| {
        let mut s = s.clone();
        if let Json::Obj(_) = s {
            s.set("look", Json::from(preset.name));
        }
        s
    };
    let colour = |name: &str, target: &str, base: &str, l: f64, c: f64, h: f64, toml: &str| {
        Json::parse(&format!(r#"{{"name":"{name}","target":"{target}","kind":"colour","base":"{base}","spread":{{"l":{l},"c":{c},"h":{h}}},"apply":{{"file":"style.toml","toml":"{toml}"}}}}"#)).expect("valid")
    };
    let number = |name: &str, target: &str, min: f64, max: f64, default: f64, toml: &str| {
        Json::parse(&format!(r#"{{"name":"{name}","target":"{target}","kind":"number","min":{min},"max":{max},"default":{default},"apply":{{"file":"style.toml","toml":"{toml}"}}}}"#)).expect("valid")
    };
    Some(match *STEPS.get(step)? {
        "base" => {
            let options = Json::arr(names().into_iter().map(Json::from));
            let space = Json::arr([Json::obj([("name", Json::from("look")), ("target", Json::from("preset")), ("kind", Json::from("choice")), ("options", options), ("default", Json::from(preset.name)), ("apply", Json::obj([("file", Json::from("style.toml")), ("toml", Json::from("look.preset"))]))])]);
            ("Which base style is closest?".into(), subject.clone(), space)
        }
        "light" => (
            "The key light: colour and strength".into(),
            with_look(subject),
            Json::arr([
                colour("key", "light.key_color", preset.key_color, 0.08, 0.05, 45.0, "stage.game.key_color"),
                number("strength", "light.key_intensity", (preset.key_intensity * 0.4).max(0.05), preset.key_intensity * 1.8, preset.key_intensity, "stage.game.key_intensity"),
                number("fill", "light.ambient_intensity", (preset.ambient_intensity * 0.4).max(0.05), preset.ambient_intensity * 1.8, preset.ambient_intensity, "stage.game.ambient_intensity"),
            ]),
        ),
        "atmosphere" => (
            "The atmosphere: fog".into(),
            with_look(subject),
            Json::arr([number("fog", "fog.density", 0.0, (preset.fog_density * 3.0).max(0.03), preset.fog_density, "look.fog_density"), colour("fog_colour", "fog.color", preset.fog_color, 0.1, 0.04, 30.0, "look.fog_color")]),
        ),
        "interface" => {
            let mut ui = Json::obj([
                ("panel", Json::from(preset.panel)),
                (
                    "lines",
                    Json::arr([
                        Json::obj([("text", Json::from("Rune pillar  120")), ("color", Json::from(preset.text))]),
                        Json::obj([("text", Json::from("Not enough dust")), ("color", Json::from("$cant_afford"))]),
                        Json::obj([("text", Json::from("Selected")), ("color", Json::from("$accent"))]),
                    ]),
                ),
            ]);
            // Over the project's own scene, when it has one (its camera, as resolved).
            if let Some(c) = subject.get("scene") {
                ui.set("camera", c.clone());
            }
            let s = Json::obj([("ui", ui)]);
            (
                "Interface colours: the accent and \"can't afford\"".into(),
                s,
                Json::arr([colour("cant_afford", "ui.cant_afford", preset.cant_afford, 0.12, 0.05, 60.0, "ui.cant_afford"), colour("accent", "ui.accent", preset.accent, 0.08, 0.06, 30.0, "ui.accent")]),
            )
        }
        _ => return None,
    })
}

/// An onboarding session (`taste/onboarding.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct Onboarding {
    /// The subject every step shows (a recipe, a scene camera, or the stage).
    pub subject: Json,
    pub preset: String,
    /// The gallery of each step started so far.
    pub galleries: Vec<String>,
    /// Who started it.
    pub by: String,
    /// `running`, `proposed` (the result is on the review queue), `approved`, `cancelled`.
    pub state: String,
    /// The review submission of the proposal.
    pub submission: Option<u64>,
    /// The proposed files (style.toml text, notes), once finished.
    pub proposal: Option<(String, String)>,
    /// Quote the owner's notes into the tracked notes (an opt-in shown in the submission the
    /// owner approves; by default the notes say what was chosen, never the owner's words).
    pub quote_notes: bool,
}

impl Onboarding {
    pub fn new(subject: Json, preset: &str, by: &str) -> Result<Onboarding> {
        if find(preset).is_none() {
            return Err(TasteError::new("bad_preset", format!("'{preset}' is not a base style: {}", names().join(", "))).at("preset"));
        }
        Ok(Onboarding { subject, preset: preset.into(), galleries: Vec::new(), by: by.into(), state: "running".into(), submission: None, proposal: None, quote_notes: false })
    }

    pub fn to_text(&self) -> String {
        let mut j = Json::obj([
            ("format", Json::from(ONBOARDING_FORMAT)),
            ("subject", self.subject.clone()),
            ("preset", Json::from(self.preset.as_str())),
            ("galleries", Json::arr(self.galleries.iter().map(|g| Json::from(g.as_str())))),
            ("by", Json::from(self.by.as_str())),
            ("state", Json::from(self.state.as_str())),
        ]);
        if let Some(s) = self.submission {
            j.set("submission", Json::from(s));
        }
        if self.quote_notes {
            j.set("quote_notes", Json::Bool(true));
        }
        if let Some((toml, notes)) = &self.proposal {
            j.set("proposal", Json::obj([("style_toml", Json::from(toml.as_str())), ("notes", Json::from(notes.as_str()))]));
        }
        let mut t = j.to_pretty();
        t.push('\n');
        t
    }

    pub fn parse(text: &str) -> Result<Onboarding> {
        let j = Json::parse(text).map_err(|e| TasteError::new("invalid_json", e.to_string()))?;
        if j.get("format").and_then(Json::as_str) != Some(ONBOARDING_FORMAT) {
            return Err(TasteError::new("bad_format", format!("\"format\": \"{ONBOARDING_FORMAT}\"")).at("format"));
        }
        let s = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
        let mut o = Onboarding::new(j.get("subject").cloned().unwrap_or(Json::Null), &s("preset"), &s("by"))?;
        o.galleries = j.get("galleries").and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_str).map(str::to_owned).take(STEPS.len()).collect();
        o.state = s("state");
        if !matches!(o.state.as_str(), "running" | "proposed" | "approved" | "cancelled") {
            return Err(TasteError::new("bad_state", "state: running, proposed, approved or cancelled").at("state"));
        }
        o.submission = j.get("submission").and_then(Json::as_u64);
        o.quote_notes = j.get("quote_notes").and_then(Json::as_bool).unwrap_or(false);
        o.proposal = j.get("proposal").and_then(|p| Some((p.get("style_toml")?.as_str()?.to_owned(), p.get("notes")?.as_str()?.to_owned())));
        Ok(o)
    }
}

/// One step's outcome for [`compose`]: the step, its gallery, the accepted values (dotted
/// `style.toml` path, TOML literal, the parameter's name), the round it was accepted in, and
/// the notes written along the way.
#[derive(Clone, Debug, PartialEq)]
pub struct StepResult {
    pub step: &'static str,
    pub gallery: String,
    pub round: usize,
    pub values: Vec<(String, String, String)>,
    pub notes: Vec<String>,
}

/// The proposal: `style.toml` (the existing text with every key set, or a new file from the
/// preset) and the notes. The notes say what was chosen; the owner's own words only with
/// `quote_notes` (taste review 2: they were always quoted into the tracked file).
pub fn compose(preset: &Preset, existing: Option<&str>, steps: &[StepResult], project: &str, quote_notes: bool) -> Result<(String, String)> {
    let chosen = steps.iter().flat_map(|s| s.values.iter()).find(|v| v.0 == "look.preset").map(|v| v.1.trim_matches('"').to_owned());
    let base = chosen.as_deref().and_then(find).unwrap_or(preset);
    let mut text = existing.map_or_else(|| format!("# {project}'s measurable style rules (docs/design/ASSET_STUDIO.md, GALLERIES.md), from the style configurator.\n"), str::to_owned);
    // The preset's keys only where the file doesn't say otherwise yet (a new file gets them all).
    let parsed = specula_core::toml::parse(&text).map_err(|e| TasteError::new("bad_toml", format!("style.toml doesn't parse: {e}")))?;
    let has = |path: &str| path.split('.').try_fold(&parsed, |j, k| j.get(k)).is_some();
    for (k, v) in base.toml_keys() {
        if existing.is_none() || !has(&k) || k == "look.preset" {
            text = tomledit::set(&text, &k, &v)?;
        }
    }
    for s in steps {
        for (k, v, _) in &s.values {
            text = tomledit::set(&text, k, v)?;
        }
    }
    let mut notes = format!("# {project}: the look, as chosen\n\nWritten by the style configurator (docs/design/GALLERIES.md) from galleries the owner picked from. Picks and notes stay in `taste/`, which is private (ignored by git).\n\n## Base style: {}\n\n{}\n", base.label, base.about);
    for s in steps {
        notes.push_str(&format!("\n## {} (gallery `{}`, accepted in round {})\n\n", capital(s.step), s.gallery, s.round));
        if s.values.is_empty() {
            notes.push_str("- kept as the base style has it\n");
        }
        for (k, v, name) in &s.values {
            notes.push_str(&format!("- {name}: `{k} = {v}`\n"));
        }
        let written: Vec<&String> = s.notes.iter().filter(|n| !n.is_empty()).collect();
        if quote_notes {
            for n in &written {
                notes.push_str(&format!("- the owner's note: \"{n}\"\n"));
            }
        } else if !written.is_empty() {
            notes.push_str(&format!("- the owner wrote {} note{} along the way (kept private in `taste/`)\n", written.len(), if written.len() == 1 { "" } else { "s" }));
        }
    }
    Ok((text, notes))
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::Space;

    #[test]
    fn every_step_is_a_valid_gallery() {
        let subject = Json::obj([("scene", Json::obj([("camera", Json::from("overview"))]))]);
        for p in &PRESETS {
            for k in 0..STEPS.len() {
                let (title, s, space) = step_gallery(k, p, &subject).unwrap();
                assert!(!title.is_empty());
                let sp = Space::parse(&space).unwrap_or_else(|e| panic!("{} step {k}: {e}", p.name));
                assert!(sp.dims() >= 1);
                if k == 3 {
                    assert_eq!(s.get("ui").and_then(|u| u.get("camera")).and_then(|c| c.get("camera")).and_then(Json::as_str), Some("overview"));
                }
            }
        }
        assert!(step_gallery(4, &PRESETS[0], &subject).is_none());
    }

    #[test]
    fn the_proposal_keeps_the_file_and_sets_the_picks() {
        let existing = "# Sepulcra\n[motion]\ncontact_m = 0.025\n\n[stage.game]\nkey_color = \"#d0d9f0\"\nkey_intensity = 0.55\n";
        let steps = vec![
            StepResult { step: "base", gallery: "onboard_base".into(), round: 1, values: vec![("look.preset".into(), "\"moonlit\"".into(), "look".into())], notes: vec![] },
            StepResult { step: "light", gallery: "onboard_light".into(), round: 3, values: vec![("stage.game.key_color".into(), "\"#c4d0f2\"".into(), "key".into())], notes: vec!["colder, less purple".into()] },
            StepResult { step: "interface", gallery: "onboard_interface".into(), round: 2, values: vec![("ui.cant_afford".into(), "\"#857f72\"".into(), "cant_afford".into())], notes: vec![] },
        ];
        let (toml, notes) = compose(&PRESETS[1], Some(existing), &steps, "Sepulcra", false).unwrap();
        let j = specula_core::toml::parse(&toml).unwrap();
        let get = |a: &str, b: &str, c: Option<&str>| {
            let t = j.get(a).and_then(|x| x.get(b));
            match c {
                Some(c) => t.and_then(|x| x.get(c)),
                None => t,
            }
            .cloned()
        };
        assert_eq!(get("stage", "game", Some("key_color")).and_then(|v| v.as_str().map(str::to_owned)).as_deref(), Some("#c4d0f2"), "the pick wins");
        assert_eq!(get("stage", "game", Some("key_intensity")).and_then(|v| v.as_f64()), Some(0.55), "existing keys stay");
        assert_eq!(get("stage", "game", Some("exposure")).and_then(|v| v.as_f64()), Some(1.0), "the chosen preset (moonlit, not storybook) fills the gaps");
        assert_eq!(get("ui", "cant_afford", None).and_then(|v| v.as_str().map(str::to_owned)).as_deref(), Some("#857f72"));
        assert!(toml.starts_with("# Sepulcra\n[motion]\ncontact_m = 0.025\n"));
        assert!(!notes.contains("colder, less purple") && notes.contains("the owner wrote 1 note") && notes.contains("## Light (gallery `onboard_light`, accepted in round 3)"), "the owner's words stay private by default (taste review 2)");
        let (_, quoted) = compose(&PRESETS[1], Some(existing), &steps, "Sepulcra", true).unwrap();
        assert!(quoted.contains("the owner's note: \"colder, less purple\""), "quoted only by opt-in");
        // A new project gets the whole preset.
        let (fresh, _) = compose(&PRESETS[3], None, &[], "Noir game", false).unwrap();
        assert!(specula_core::toml::parse(&fresh).unwrap().get("look").and_then(|l| l.get("preset")).and_then(Json::as_str) == Some("noir"));
        // A preset that asks for a shape language writes it as a taste rule; one that does not
        // writes none (the existing file above, of a moonlit game, got none).
        assert!(!toml.contains("shape_language") && !fresh.contains("shape_language"));
        // (No shipped preset asks for one while the measure is unsettled: a preset that did
        // would write it so.)
        assert!(PRESETS.iter().all(|p| p.shape.is_none()));
        let asking = Preset { shape: Some(("round", 0.5)), ..*find("storybook").unwrap() };
        let (story, _) = compose(&asking, None, &[], "A friendly game", false).unwrap();
        let taste = specula_core::toml::parse(&story).unwrap().get("taste").cloned().unwrap();
        assert_eq!((taste.get("shape_language").and_then(Json::as_str), taste.get("shape_min_share").and_then(Json::as_f64)), (Some("round"), Some(0.5)));
        assert_eq!(asking.to_json().get("shape").and_then(|s| s.get("language")).and_then(Json::as_str), Some("round"));
        assert!(find("storybook").unwrap().to_json().get("shape").is_some_and(Json::is_null));
        assert!(find("clean").unwrap().to_json().get("shape").is_some_and(Json::is_null));
        let o = Onboarding::new(Json::Null, "muted", "lead").unwrap();
        assert_eq!(Onboarding::parse(&o.to_text()).unwrap(), o);
        assert_eq!(Onboarding::new(Json::Null, "neon", "x").unwrap_err().code, "bad_preset");
    }
}

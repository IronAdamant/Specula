//! The player's settings as data (`sepulcra.settings.v1`), saved per user by the engine
//! (`specula::user_settings`; docs/design/ACCESSIBILITY.md).
//!
//! - **Forgiving to read.** A file from another version or edited by hand never stops the game:
//!   a value out of range or of the wrong type falls back to its default and is named in
//!   [`Parsed::problems`]; keys this version doesn't know are kept as they were and written back
//!   (another version's settings survive this one), at most [`MAX_EXTRA`] of them.
//! - **Bounded.** The engine reads at most 64 KiB; the player's bindings are at most
//!   [`MAX_BINDINGS`] actions (the input crate validates them when they are applied).
//! - **"Auto"** for the interface and text size means "what this display wants": 1, or a
//!   handheld's legible default (`Display::handheld_ui_scale`). A player's own choice always wins,
//!   smaller included (owner, 2026-11-07: leave tiny UI to the player).

use specula_core::Json;
use specula_input::HoldMode;

pub const FORMAT: &str = "sepulcra.settings.v1";
/// Interface sizes (the settings list them all, with what each gives on this display).
pub const UI_SCALES: [f32; 6] = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5];

/// A setting's number as files and tools show it: the option as written (1.15), not its nearest
/// `f32` widened (1.149999976158142), so what is read back equals what was set.
pub fn shown(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}
/// Text sizes, on top of the interface size.
pub const TEXT_SIZES: [f32; 5] = [0.9, 1.0, 1.15, 1.3, 1.5];
/// Master volume steps.
pub const VOLUMES: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// Button-prompt styles (`Auto` follows the pad in use).
pub const GLYPHS: [&str; 4] = ["Auto", "Xbox", "PlayStation", "Steam Deck"];
/// Unknown keys kept from a file, at most.
pub const MAX_EXTRA: usize = 32;
/// Actions with the player's own bindings, at most.
pub const MAX_BINDINGS: usize = 64;

/// Colour themes for colour-vision differences: each moves the interface's meaning-carrying
/// colours (the player's magic, the dead's threat, warnings) to ones that stay apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vision {
    #[default]
    Standard,
    /// Red-green, green-weak (the most common).
    Deuteranopia,
    /// Red-green, red-weak.
    Protanopia,
    /// Blue-yellow.
    Tritanopia,
    /// Brighter text, darker panels, thicker focus.
    HighContrast,
}

impl Vision {
    pub const ALL: [Vision; 5] = [Vision::Standard, Vision::Deuteranopia, Vision::Protanopia, Vision::Tritanopia, Vision::HighContrast];

    pub fn name(self) -> &'static str {
        match self {
            Vision::Standard => "standard",
            Vision::Deuteranopia => "deuteranopia",
            Vision::Protanopia => "protanopia",
            Vision::Tritanopia => "tritanopia",
            Vision::HighContrast => "high_contrast",
        }
    }


    pub fn parse(s: &str) -> Option<Vision> {
        Vision::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// Subtitle and caption sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubtitleSize {
    Small,
    #[default]
    Medium,
    Large,
    Huge,
}

impl SubtitleSize {
    pub const ALL: [SubtitleSize; 4] = [SubtitleSize::Small, SubtitleSize::Medium, SubtitleSize::Large, SubtitleSize::Huge];

    pub fn name(self) -> &'static str {
        match self {
            SubtitleSize::Small => "small",
            SubtitleSize::Medium => "medium",
            SubtitleSize::Large => "large",
            SubtitleSize::Huge => "huge",
        }
    }

    /// Multiplies the subtitle roles' sizes.
    pub fn factor(self) -> f32 {
        match self {
            SubtitleSize::Small => 0.85,
            SubtitleSize::Medium => 1.0,
            SubtitleSize::Large => 1.3,
            SubtitleSize::Huge => 1.6,
        }
    }

    pub fn parse(s: &str) -> Option<SubtitleSize> {
        SubtitleSize::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// Everything the settings screens change.
/// The most the engine writes for a settings file (`specula::user_settings::MAX_BYTES`).
pub const MAX_FILE_BYTES: usize = specula::user_settings::MAX_BYTES;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Index into [`UI_SCALES`]; `None`: auto (1, or the handheld's default).
    pub ui_scale: Option<usize>,
    /// Index into [`TEXT_SIZES`]; `None`: auto (1).
    pub text_size: Option<usize>,
    /// Index into [`VOLUMES`].
    pub volume: usize,
    pub reduced_motion: bool,
    /// Index into [`GLYPHS`].
    pub glyphs: usize,
    /// Autosave at each wave's start; `None`: the default (on with a window).
    pub autosave: Option<bool>,
    pub vision: Vision,
    /// Cutscene subtitles (on by default).
    pub subtitles: bool,
    /// Captions for sounds (off by default).
    pub captions: bool,
    pub subtitle_size: SubtitleSize,
    /// A dark band behind subtitles and captions (on by default).
    pub subtitle_background: bool,
    /// The camera's mouse drags (orbit, pan): hold the button, or press once to start and once
    /// to stop.
    pub camera_drag: HoldMode,
    /// The player's bindings (`ActionMap::bindings_json` of the actions they changed);
    /// `None`: the game's own.
    pub bindings: Option<Json>,
    /// Keys this version doesn't know, kept as they were.
    pub extra: Vec<(String, Json)>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            ui_scale: None,
            text_size: None,
            volume: 3,
            reduced_motion: false,
            glyphs: 0,
            autosave: None,
            vision: Vision::Standard,
            subtitles: true,
            captions: false,
            subtitle_size: SubtitleSize::Medium,
            subtitle_background: true,
            camera_drag: HoldMode::Hold,
            bindings: None,
            extra: Vec::new(),
        }
    }
}

/// A settings file read: the settings, and what in it couldn't be used.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub settings: Settings,
    pub problems: Vec<String>,
}

/// The index of `v` among `options` (within 0.001).
fn index_of(options: &[f32], v: f64) -> Option<usize> {
    options.iter().position(|&o| (f64::from(o) - v).abs() < 1e-3)
}

const KNOWN: [&str; 15] = [
    "format",
    "ui_scale",
    "text_size",
    "volume",
    "reduced_motion",
    "glyphs",
    "autosave",
    "vision",
    "subtitles",
    "captions",
    "subtitle_size",
    "subtitle_background",
    "camera_drag",
    "bindings",
    "saved_by",
];

impl Settings {
    /// Reads a settings file; never fails (see the module docs).
    pub fn parse(text: &str) -> Parsed {
        let mut s = Settings::default();
        let mut problems = Vec::new();
        let json = match Json::parse(text) {
            Ok(j) => j,
            Err(e) => {
                problems.push(format!("not JSON ({}): defaults used", e.message));
                return Parsed { settings: s, problems };
            }
        };
        let Some(obj) = json.as_object() else {
            problems.push("not a JSON object: defaults used".into());
            return Parsed { settings: s, problems };
        };
        if json.get("format").and_then(Json::as_str) != Some(FORMAT) {
            problems.push(format!("format isn't {FORMAT}: read what matches"));
        }
        let mut bad = |key: &str, why: &str| problems.push(format!("{key}: {why} (default used)"));
        let auto_or = |key: &str, options: &[f32], bad: &mut dyn FnMut(&str, &str)| -> Option<usize> {
            match json.get(key) {
                None | Some(Json::Null) => None,
                Some(Json::Str(a)) if a == "auto" => None,
                Some(v) => match v.as_f64().and_then(|x| index_of(options, x)) {
                    Some(i) => Some(i),
                    None => {
                        bad(key, &format!("must be \"auto\" or one of {options:?}"));
                        None
                    }
                },
            }
        };
        s.ui_scale = auto_or("ui_scale", &UI_SCALES, &mut bad);
        s.text_size = auto_or("text_size", &TEXT_SIZES, &mut bad);
        if let Some(v) = json.get("volume") {
            match v.as_f64().and_then(|x| index_of(&VOLUMES, x)) {
                Some(i) => s.volume = i,
                None => bad("volume", "must be 0, 0.25, 0.5, 0.75 or 1"),
            }
        }
        let flag = |key: &str, out: &mut bool, bad: &mut dyn FnMut(&str, &str)| {
            if let Some(v) = json.get(key) {
                match v.as_bool() {
                    Some(b) => *out = b,
                    None => bad(key, "must be true or false"),
                }
            }
        };
        flag("reduced_motion", &mut s.reduced_motion, &mut bad);
        flag("subtitles", &mut s.subtitles, &mut bad);
        flag("captions", &mut s.captions, &mut bad);
        flag("subtitle_background", &mut s.subtitle_background, &mut bad);
        match json.get("autosave") {
            None | Some(Json::Null) => {}
            Some(v) => match v.as_bool() {
                Some(b) => s.autosave = Some(b),
                None => bad("autosave", "must be true, false or null"),
            },
        }
        let named = |key: &str, bad: &mut dyn FnMut(&str, &str)| -> Option<String> {
            let v = json.get(key)?;
            match v.as_str() {
                Some(t) => Some(t.to_owned()),
                None => {
                    bad(key, "must be a string");
                    None
                }
            }
        };
        if let Some(g) = named("glyphs", &mut bad) {
            match GLYPHS.iter().position(|n| *n == g) {
                Some(i) => s.glyphs = i,
                None => bad("glyphs", "must be Auto, Xbox, PlayStation or Steam Deck"),
            }
        }
        if let Some(v) = named("vision", &mut bad) {
            match Vision::parse(&v) {
                Some(v) => s.vision = v,
                None => bad("vision", "must be standard, deuteranopia, protanopia, tritanopia or high_contrast"),
            }
        }
        if let Some(v) = named("subtitle_size", &mut bad) {
            match SubtitleSize::parse(&v) {
                Some(v) => s.subtitle_size = v,
                None => bad("subtitle_size", "must be small, medium, large or huge"),
            }
        }
        if let Some(v) = named("camera_drag", &mut bad) {
            match HoldMode::parse(&v) {
                Some(m) => s.camera_drag = m,
                None => bad("camera_drag", "must be hold or toggle"),
            }
        }
        match json.get("bindings") {
            None | Some(Json::Null) => {}
            Some(b) => match b.get("bindings").and_then(Json::as_object) {
                Some(actions) if actions.len() <= MAX_BINDINGS => s.bindings = Some(b.clone()),
                Some(_) => bad("bindings", &format!("more than {MAX_BINDINGS} actions")),
                None => bad("bindings", "must be {\"format\": ..., \"bindings\": {action: [bindings]}}"),
            },
        }
        let unknown: Vec<&(String, Json)> = obj.iter().filter(|(k, _)| !KNOWN.contains(&k.as_str())).collect();
        if unknown.len() > MAX_EXTRA {
            problems.push(format!("{} unknown keys: the first {MAX_EXTRA} kept", unknown.len()));
        }
        s.extra = unknown.into_iter().take(MAX_EXTRA).cloned().collect();
        Parsed { settings: s, problems }
    }

    /// The settings file's JSON (unknown keys from the file read are written back after ours).
    pub fn to_json(&self) -> Json {
        let auto = |i: Option<usize>, options: &[f32]| i.and_then(|i| options.get(i)).map_or(Json::from("auto"), |&v| Json::from(shown(v)));
        let mut pairs: Vec<(String, Json)> = vec![
            ("format".into(), Json::from(FORMAT)),
            ("ui_scale".into(), auto(self.ui_scale, &UI_SCALES)),
            ("text_size".into(), auto(self.text_size, &TEXT_SIZES)),
            ("volume".into(), Json::from(shown(VOLUMES[self.volume.min(VOLUMES.len() - 1)]))),
            ("reduced_motion".into(), Json::from(self.reduced_motion)),
            ("glyphs".into(), Json::from(GLYPHS[self.glyphs.min(GLYPHS.len() - 1)])),
            ("autosave".into(), self.autosave.map_or(Json::Null, Json::from)),
            ("vision".into(), Json::from(self.vision.name())),
            ("subtitles".into(), Json::from(self.subtitles)),
            ("captions".into(), Json::from(self.captions)),
            ("subtitle_size".into(), Json::from(self.subtitle_size.name())),
            ("subtitle_background".into(), Json::from(self.subtitle_background)),
            ("camera_drag".into(), Json::from(self.camera_drag.name())),
            ("bindings".into(), self.bindings.clone().unwrap_or(Json::Null)),
        ];
        pairs.extend(self.extra.iter().cloned());
        Json::obj(pairs)
    }

    /// The text the engine saves: indented, for a person who opens it. The engine writes at most
    /// [`MAX_FILE_BYTES`]; only keys this version doesn't know (kept for the version that wrote
    /// them) can make the file that big, and an oversized file would fail every later save
    /// (review R1 of m5-a11y). So a file over the limit is written compact, and if that is
    /// still too big, without the unknown keys.
    pub fn to_text(&self) -> String {
        let pretty = self.to_json().to_pretty();
        if pretty.len() <= MAX_FILE_BYTES {
            return pretty;
        }
        let compact = self.to_json().to_string();
        if compact.len() <= MAX_FILE_BYTES {
            return compact;
        }
        Settings { extra: Vec::new(), ..self.clone() }.to_json().to_pretty()
    }

    pub fn volume(&self) -> f32 {
        VOLUMES[self.volume.min(VOLUMES.len() - 1)]
    }

    pub fn text_size(&self) -> f32 {
        self.text_size.and_then(|i| TEXT_SIZES.get(i)).copied().unwrap_or(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R1 finding 11 of m5-a11y: unknown keys another version wrote are kept, but they can't
    /// make the file too big to save (every later save would fail).
    #[test]
    fn the_saved_text_always_fits_what_the_engine_writes() {
        // Indented while it fits.
        let small = Settings::parse(r#"{"volume": 0.5, "from_a_later_version": [1, 2, 3]}"#).settings;
        assert!(small.to_text().contains('\n') && small.to_text().contains("from_a_later_version"));
        // A long array of numbers is small compact and several times that indented (a line each).
        let numbers = vec!["1"; 20_000].join(",");
        let big = Settings::parse(&format!(r#"{{"volume": 0.5, "from_a_later_version": [{numbers}]}}"#)).settings;
        assert!(big.to_json().to_pretty().len() > MAX_FILE_BYTES, "the indented form would not be written");
        let text = big.to_text();
        assert!(text.len() <= MAX_FILE_BYTES && text.contains("from_a_later_version"), "written compact, the unknown key kept ({} bytes)", text.len());
        assert_eq!(Settings::parse(&text).settings, big);
        // Too big even compact: our own settings are saved without the unknown keys.
        let huge = Settings::parse(&format!(r#"{{"volume": 0.5, "x": "{}"}}"#, "y".repeat(60_000))).settings;
        let mut huge = huge;
        huge.extra.push(("z".into(), Json::from("w".repeat(20_000))));
        let text = huge.to_text();
        assert!(text.len() <= MAX_FILE_BYTES, "{} bytes", text.len());
        let back = Settings::parse(&text).settings;
        assert!(back.extra.is_empty() && back.volume == huge.volume, "the player's own settings survive");
    }

    #[test]
    fn settings_round_trip_and_keep_what_they_dont_know() {
        let mut s = Settings { ui_scale: Some(3), text_size: Some(4), volume: 1, reduced_motion: true, glyphs: 2, autosave: Some(false), vision: Vision::Tritanopia, captions: true, subtitle_size: SubtitleSize::Huge, camera_drag: HoldMode::Toggle, ..Settings::default() };
        s.extra.push(("locale".into(), Json::from("de")));
        let back = Settings::parse(&s.to_text());
        assert_eq!(back.problems, Vec::<String>::new());
        assert_eq!(back.settings, s, "and the unknown locale key survives");
        assert_eq!(Settings::parse(&Settings::default().to_text()).settings, Settings::default());
    }

    #[test]
    fn bad_values_fall_back_one_by_one() {
        let p = Settings::parse(r#"{"format": "sepulcra.settings.v1", "ui_scale": 7, "text_size": "auto", "volume": 0.5, "vision": "plaid", "subtitles": "yes", "camera_drag": "toggle", "glyphs": "Steam Deck"}"#);
        assert_eq!(p.settings.ui_scale, None);
        assert_eq!(p.settings.volume, 2);
        assert_eq!(p.settings.vision, Vision::Standard);
        assert!(p.settings.subtitles, "the default");
        assert_eq!(p.settings.camera_drag, HoldMode::Toggle);
        assert_eq!(p.settings.glyphs, 3);
        assert_eq!(p.problems.len(), 3, "{:?}", p.problems);
        for bad in ["", "[]", "nul", "{\"format\": 3}"] {
            let p = Settings::parse(bad);
            assert!(!p.problems.is_empty(), "{bad}");
        }
    }

    #[test]
    fn hostile_files_stay_bounded() {
        // Hundreds of unknown keys: only MAX_EXTRA are kept.
        let keys: Vec<String> = (0..500).map(|i| format!("\"k{i}\": {i}")).collect();
        let p = Settings::parse(&format!("{{{}}}", keys.join(",")));
        assert_eq!(p.settings.extra.len(), MAX_EXTRA);
        // Too many bound actions: refused (the game's own bindings stay).
        let acts: Vec<String> = (0..MAX_BINDINGS + 1).map(|i| format!("\"a{i}\": []")).collect();
        let p = Settings::parse(&format!("{{\"bindings\": {{\"format\": \"specula.bindings.v1\", \"bindings\": {{{}}}}}}}", acts.join(",")));
        assert_eq!(p.settings.bindings, None);
        // Deep nesting is the JSON parser's to refuse; nothing panics.
        let deep = "[".repeat(10_000);
        assert!(!Settings::parse(&deep).problems.is_empty());
    }
}

//! Taste direction as data (docs/design/GALLERIES.md): "choose, don't describe".
//!
//! - [`space`]: a gallery's parameter space (numbers, Oklch colours, choices) mapped to the unit
//!   cube, its render targets, where accepted values apply in the project, and the edit chips.
//! - [`spread`]: dispersed spreads (Design Galleries): farthest-first points of a scrambled
//!   Halton set.
//! - [`gp`] and [`tune`]: preferential Bayesian optimisation from choices (Sequential Line
//!   Search): a Gaussian process on a latent goodness, fitted to Bradley-Terry-Luce choices by a
//!   Laplace approximation; expected improvement picks the line the next round shows.
//! - [`gallery`]: a gallery file (`specula.gallery.v1`): rounds, variants and decisions (accept,
//!   edit with a note and chips, none of these, back, dislike), and the next round's plan.
//! - [`prefs`]: the preference store over every gallery: queries, and suggestions from repeated
//!   consistent picks (`style.toml` edits or priors, for a human to approve).
//! - [`board`]: reference-board measures (value histogram, saturation, palette, contrast, edge
//!   density, shadow softness), targets, and checks of a scene against them.
//! - [`presets`]: base styles and the onboarding steps; `style.toml` text edited key by key.
//! - [`colour`]: sRGB, linear, Oklab and Oklch with `dmath` only.
//!
//! Everything is pure and deterministic: seeded streams, `dmath` for transcendentals, ordered
//! containers, no clocks (the determinism lints apply: the crate's `clippy.toml`).

#![forbid(unsafe_code)]

pub mod board;
pub mod colour;
pub mod gallery;
pub mod gp;
pub mod prefs;
/// The approved priors (`assets/style/priors.json`), for validating that file.
pub use prefs::Priors;
pub mod presets;
pub mod space;
pub mod spread;
pub mod tomledit;
pub mod tune;

use specula_core::Json;

/// A structured error: a machine-readable code, a message that says what to change, and the
/// field (JSON path) it is about.
#[derive(Clone, Debug, PartialEq)]
pub struct TasteError {
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
}

impl TasteError {
    pub fn new(code: &'static str, message: impl Into<String>) -> TasteError {
        TasteError { code, message: message.into(), field: None }
    }

    pub fn at(mut self, field: impl Into<String>) -> TasteError {
        self.field = Some(field.into());
        self
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))]);
        if let Some(f) = &self.field {
            j.set("field", Json::from(f.as_str()));
        }
        j
    }
}

impl std::fmt::Display for TasteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.field {
            Some(p) => write!(f, "{} ({p}): {}", self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

pub type Result<T> = std::result::Result<T, TasteError>;

/// Rounds to 4 decimals for files and answers (stable text, fewer tokens).
pub fn r4(x: f64) -> Json {
    Json::from(specula_core::dmath::round_f64(x * 10_000.0) / 10_000.0)
}

/// Rounds to 3 decimals.
pub fn r3(x: f64) -> Json {
    Json::from(specula_core::dmath::round_f64(x * 1000.0) / 1000.0)
}

/// `true` for names agents give galleries, boards and users: `[a-z0-9_-]{1,40}`.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Cuts `s` to at most `max` characters (on a char boundary).
pub fn cut(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].to_owned(),
        None => s.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_cuts() {
        assert!(valid_name("moon_light-2"));
        assert!(!valid_name("Moon") && !valid_name("") && !valid_name("a/b"));
        assert_eq!(cut("héllo", 2), "hé");
        assert_eq!(TasteError::new("bad", "no").at("space[0]").to_json().get("field").and_then(Json::as_str), Some("space[0]"));
    }
}

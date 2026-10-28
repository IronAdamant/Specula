//! The asset studio's measures (docs/design/ASSET_STUDIO.md): a game's measurable style rules
//! (`style.toml`), the facts of a built asset, and taste checks on rendered masks and images,
//! all reported as data like budget errors.
//!
//! - [`rules`]: `style.toml` parsed and validated (budgets, scale, palette, naming, taste
//!   thresholds, the stage's game light, the game camera, the bible's rule text).
//! - [`facts`]: what is measured about one build (triangles per LOD, size, materials, surface
//!   colours by area) from a [`specula_model::Model`].
//! - [`check`]: the rules on the facts: budgets, scale ranges, palette distance, neutral
//!   saturation, forbidden families, emissive presets, naming.
//! - [`image`]: masks from silhouette renders, IoU, rings, lightness statistics.
//! - [`taste`]: silhouette distinctness between siblings, thumbnail solidity, pixel size, value
//!   contrast, flat lighting.
//! - [`critique`]: the findings per pass with the bible's rules and revise hints.
//!
//! Everything here is pure and deterministic (fixed iteration orders, no clocks); the engine
//! renders, this crate measures. A rule broken is a [`Finding`] that failed.

#![forbid(unsafe_code)]

pub mod check;
pub mod colour;
pub mod critique;
pub mod facts;
pub mod image;
pub mod palette;
pub mod rules;
pub mod taste;

use specula_core::Json;
use std::fmt;

/// A `style.toml` that can't be used: a machine-readable `code`, the rule's `path`
/// (`budget[2].max_triangles`) and, for syntax errors, the line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleError {
    pub code: &'static str,
    pub path: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl StyleError {
    pub fn to_json(&self) -> Json {
        // (A palette too big is style.json's, not style.toml's.)
        let kind = if self.path.starts_with("style.json") { "invalid_style_json" } else { "invalid_style_toml" };
        let mut j = Json::obj([("error", Json::from(kind)), ("code", Json::from(self.code)), ("path", Json::from(self.path.as_str())), ("message", Json::from(self.message.as_str()))]);
        if let Some(l) = self.line {
            j.set("line", Json::from(l));
        }
        if let Some(c) = self.column {
            j.set("column", Json::from(c));
        }
        j
    }
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({} at '{}')", self.message, self.code, self.path)
    }
}

impl std::error::Error for StyleError {}

/// A game's rules with its palette: what every check needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bible {
    pub rules: rules::StyleRules,
    pub palette: palette::Palette,
}

impl Bible {
    /// From the game's style (`style.json`) and its `style.toml` text (`None`: the defaults).
    /// `source` names the file in findings (`style.toml`).
    pub fn load(style: &specula_model::Style, toml: Option<&str>, source: &str) -> Result<Bible, StyleError> {
        let lookup = |n: &str| style.palette.get(n).map(|c| [c[0], c[1], c[2]]);
        let rules = match toml {
            Some(t) => rules::StyleRules::parse(t, source, &lookup)?,
            None => rules::StyleRules::defaults(),
        };
        if style.palette.len() > palette::MAX_PALETTE {
            return Err(StyleError {
                code: "palette_too_big",
                path: "style.json palette".into(),
                message: format!("the palette has {} colours; the studio checks at most {} (the search for each surface colour's nearest blend grows with the square of the palette)", style.palette.len(), palette::MAX_PALETTE),
                line: None,
                column: None,
            });
        }
        let palette = palette::Palette::new(style.palette.iter().map(|(n, c)| (n.as_str(), [c[0], c[1], c[2]])), &rules);
        Ok(Bible { rules, palette })
    }

    /// The `style.toml` rules on one asset's facts.
    pub fn check(&self, facts: &facts::AssetFacts) -> Report {
        check::check(facts, &self.rules, &self.palette)
    }

    /// [`Bible::check`], stopping the palette search when `stop` says so (a call's time limit):
    /// the report is then [`Report::cut_short`] and its palette findings are left out.
    pub fn check_with(&self, facts: &facts::AssetFacts, stop: &dyn Fn() -> bool) -> Report {
        check::check_with(facts, &self.rules, &self.palette, stop)
    }
}

/// A taste check that passes within this share of its limit is reported `marginal` (adapters
/// differ by a few hundredths in lightness; a small edit can tip it).
pub const MARGIN: f64 = 0.15;

/// Which way a limit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// The value must be at most the limit.
    Max,
    /// At least the limit.
    Min,
}

/// One measured check: a value against a limit from a rule. Failed and not exempt = an error.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// `triangles`, `height`, `palette_distance`, `silhouette_distinct`, ...
    pub check: &'static str,
    /// The asset (`warding_stone tier=3`).
    pub subject: String,
    pub value: f64,
    pub limit: f64,
    pub bound: Bound,
    pub unit: &'static str,
    pub passed: bool,
    /// Failed, but `[taste.allow]` says it's deliberate.
    pub exempt: bool,
    /// A taste check that passed within [`MARGIN`] of its limit: another adapter or a small edit
    /// may tip it.
    pub marginal: bool,
    /// Where the limit came from (`style.toml budget[2].max_triangles`, or `defaults`).
    pub rule: String,
    pub message: String,
    /// What to change when it fails.
    pub fix: String,
    pub detail: Option<Json>,
}

impl Finding {
    /// A measured check; `passed` from the bound.
    #[allow(clippy::too_many_arguments)]
    pub fn measure(check: &'static str, subject: &str, value: f64, limit: f64, bound: Bound, unit: &'static str, rule: String, message: String, fix: String) -> Finding {
        let passed = match bound {
            Bound::Max => value <= limit + 1e-9,
            Bound::Min => value >= limit - 1e-9,
        };
        Finding { check, subject: subject.to_string(), value, limit, bound, unit, passed, exempt: false, marginal: false, rule, message, fix, detail: None }
    }

    /// Marks a passed check within [`MARGIN`] of its limit as `marginal` (the taste checks:
    /// rendered measures differ a little between adapters).
    pub fn with_margin(mut self) -> Finding {
        self.marginal = self.passed && self.limit != 0.0 && (self.value - self.limit).abs() <= MARGIN * self.limit.abs();
        self
    }

    /// A rule with no number (a name, a category): 1 for broken, 0 for kept.
    pub fn flag(check: &'static str, subject: &str, broken: bool, rule: String, message: String, fix: String) -> Finding {
        Finding { check, subject: subject.to_string(), value: broken as u8 as f64, limit: 0.0, bound: Bound::Max, unit: "", passed: !broken, exempt: false, marginal: false, rule, message, fix, detail: None }
    }

    pub fn with_detail(mut self, d: Json) -> Finding {
        self.detail = Some(d);
        self
    }

    /// An error: failed and not exempt.
    pub fn is_error(&self) -> bool {
        !self.passed && !self.exempt
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f64| Json::from((x * 10_000.0).round() / 10_000.0);
        let mut j = Json::obj([
            ("check", Json::from(self.check)),
            ("subject", Json::from(self.subject.as_str())),
            ("passed", Json::from(self.passed)),
            ("value", r(self.value)),
            (if self.bound == Bound::Max { "max" } else { "min" }, r(self.limit)),
            ("unit", Json::from(self.unit)),
            ("rule", Json::from(self.rule.as_str())),
        ]);
        if self.exempt {
            j.set("exempt", Json::from(true));
        }
        if self.marginal {
            j.set("marginal", Json::from(true));
        }
        if !self.passed {
            j.set("message", Json::from(self.message.as_str()));
            if !self.fix.is_empty() {
                j.set("fix", Json::from(self.fix.as_str()));
            }
        }
        if let Some(d) = &self.detail {
            j.set("detail", d.clone());
        }
        j
    }
}

/// Findings of one or more checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// What couldn't be measured, and why.
    pub notes: Vec<String>,
    /// The palette search was stopped (see [`Bible::check_with`]): its findings are missing.
    pub cut_short: bool,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.is_error())
    }

    pub fn passed(&self) -> bool {
        self.errors().next().is_none()
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Finding>) {
        self.findings.extend(other);
    }

    /// Passed checks within [`MARGIN`] of their limits.
    pub fn marginal(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.marginal)
    }

    /// `passed`, counts, the errors (at most `max_errors`), and with `all` every check (errors
    /// first, at most `max_checks`).
    pub fn to_json(&self, max_errors: usize, all: Option<usize>) -> Json {
        let errors: Vec<&Finding> = self.errors().collect();
        let mut j = Json::obj([
            ("passed", Json::from(errors.is_empty())),
            ("checks", Json::from(self.findings.len())),
            ("failed", Json::from(errors.len())),
            ("exempt", Json::from(self.findings.iter().filter(|f| f.exempt).count())),
            ("errors", Json::arr(errors.iter().take(max_errors).map(|f| f.to_json()))),
        ]);
        if errors.len() > max_errors {
            j.set("errors_not_listed", Json::from(errors.len() - max_errors));
        }
        let marginal: Vec<Json> = self.marginal().take(max_errors).map(|f| Json::from(format!("{} {} {} (limit {})", f.subject, f.check, (f.value * 1000.0).round() / 1000.0, (f.limit * 1000.0).round() / 1000.0))).collect();
        if !marginal.is_empty() {
            j.set("marginal", Json::Arr(marginal));
        }
        if !self.notes.is_empty() {
            j.set("not_measured", Json::arr(self.notes.iter().take(16).map(|n| Json::from(n.as_str()))));
        }
        if let Some(max) = all {
            let mut sorted: Vec<&Finding> = self.findings.iter().collect();
            sorted.sort_by_key(|f| !f.is_error());
            j.set("all", Json::arr(sorted.iter().take(max).map(|f| f.to_json())));
        }
        j
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_and_reports() {
        let ok = Finding::measure("triangles", "a", 900.0, 1500.0, Bound::Max, "triangles", "budget[0].max_triangles".into(), "m".into(), "f".into());
        let bad = Finding::measure("height", "a", 0.5, 1.0, Bound::Min, "m", "scale[0].height_m".into(), "too short".into(), "taller".into());
        let mut ex = bad.clone();
        ex.exempt = true;
        assert!(ok.passed && !bad.passed && bad.is_error() && !ex.is_error());
        let r = Report { findings: vec![ok, bad, ex], ..Report::default() };
        assert!(!r.passed());
        let j = r.to_json(10, Some(10));
        assert_eq!(j.get("failed").and_then(Json::as_u64), Some(1));
        assert_eq!(j.get("all").and_then(Json::as_array).map(|a| a[0].get("check").and_then(Json::as_str)), Some(Some("height")));
        assert!(Finding::flag("name_case", "A", true, "naming.case".into(), "m".into(), String::new()).is_error());
        // Marginal: a taste check passing within 15% of its limit (not a failure, not a rule).
        let near = Finding::measure("px_phone", "t", 9.0, 8.0, Bound::Min, "px", "taste".into(), String::new(), String::new());
        assert!(!near.marginal && near.clone().with_margin().marginal);
        let far = Finding::measure("px_phone", "t", 20.0, 8.0, Bound::Min, "px", "taste".into(), String::new(), String::new()).with_margin();
        let failed = Finding::measure("px_phone", "t", 7.9, 8.0, Bound::Min, "px", "taste".into(), String::new(), String::new()).with_margin();
        assert!(!far.marginal && !failed.marginal);
        let r = Report { findings: vec![near.with_margin()], notes: vec!["x: not measured".into()], ..Report::default() };
        let j = r.to_json(4, None);
        assert_eq!(j.get("marginal").and_then(Json::as_array).map(|a| a.len()), Some(1));
        assert!(j.get("not_measured").is_some());
    }
}

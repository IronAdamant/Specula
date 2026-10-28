//! `vfx_check`: budget and readability checks of an effect, as data. Each check says what was
//! measured, the limit, and what to change; errors are budget or safety failures, warnings are
//! readability concerns an agent should look at (with `vfx_preview`).

use crate::component::Params;
use crate::curve::srgb_to_linear;
use crate::effect::{Effect, Space};
use crate::eval::Track;
use specula_core::{dmath, Json};

/// What effects are checked against (perf.toml `[vfx]` and a reference view).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Most particles one instance may have alive at its peak.
    pub max_per_effect: f32,
    /// Estimated covered screens one instance may cost at the reference view.
    pub fill_cap: f32,
    /// Farthest a particle may travel from its emitter (metres).
    pub max_reach: f32,
    /// The reference view: camera distance (m), vertical field of view (degrees), image size.
    pub view_distance: f32,
    pub fov_y_deg: f32,
    pub width: u32,
    pub height: u32,
    /// The simulation tick rate (world-space trails are tracked per tick).
    pub tick_hz: u32,
    /// sRGB distance a palette check allows (0..1 per channel, Euclidean).
    pub palette_distance: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_per_effect: 1500.0,
            fill_cap: 1.0,
            max_reach: 25.0,
            view_distance: 8.0,
            fov_y_deg: 50.0,
            width: 1280,
            height: 720,
            tick_hz: 30,
            palette_distance: 0.22,
        }
    }
}

/// One check's verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub check: &'static str,
    pub ok: bool,
    /// `error` (budget or safety) or `warning` (readability).
    pub severity: &'static str,
    pub value: f64,
    pub limit: f64,
    /// The emitter it is about, when one.
    pub emitter: Option<String>,
    pub message: String,
}

impl Check {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("check", Json::from(self.check)),
            ("ok", Json::from(self.ok)),
            ("severity", Json::from(self.severity)),
            ("value", Json::from(round3(self.value))),
            ("limit", Json::from(round3(self.limit))),
        ];
        if let Some(e) = &self.emitter {
            f.push(("emitter", Json::from(e.as_str())));
        }
        f.push(("message", Json::from(self.message.as_str())));
        Json::obj(f)
    }
}

fn round3(v: f64) -> f64 {
    dmath::round_f64(v * 1000.0) / 1000.0
}

/// Pixels a particle `size` metres across covers at `distance` in the reference view.
pub fn particle_px(size: f32, distance: f32, fov_y_deg: f32, height: u32) -> f32 {
    let half = dmath::tan((fov_y_deg * 0.5).to_radians()).max(1e-3);
    size / (2.0 * distance.max(0.05) * half) * height as f32
}

/// Every check of `effect` played with `params`; `palette` (sRGB colours) checks that every
/// colour the effect uses lies near one of them (a style family).
pub fn check(effect: &Effect, params: &Params, limits: &Limits, palette: Option<&[[f32; 3]]>) -> Vec<Check> {
    let mut out = Vec::new();
    let peak = effect.peak() * params.rate;
    out.push(Check {
        check: "peak_particles",
        ok: peak <= limits.max_per_effect,
        severity: "error",
        value: peak as f64,
        limit: limits.max_per_effect as f64,
        emitter: None,
        message: format!("at most {peak:.0} particles alive at once (rate × longest lifetime plus bursts); lower rates or lifetimes past the limit"),
    });
    let reach = effect.reach() * params.speed.max(1.0) * params.size.max(1.0);
    out.push(Check {
        check: "reach",
        ok: reach <= limits.max_reach,
        severity: "warning",
        value: reach as f64,
        limit: limits.max_reach as f64,
        emitter: None,
        message: format!("particles can travel up to {reach:.1} m from the emitter: wide effects cost culling and fill; shorten speeds, lifetimes or noise if that isn't meant"),
    });
    // Fill at the reference view: streams at their mean of size² × alpha, bursts at their peak.
    let screen = (limits.width * limits.height) as f32;
    let px_per_m = particle_px(params.size, limits.view_distance, limits.fov_y_deg, limits.height);
    let fill: f32 = effect.emitters.iter().map(|e| e.fill_area(params.rate) * px_per_m * px_per_m).sum();
    let fill_screens = fill / screen;
    out.push(Check {
        check: "overdraw",
        ok: fill_screens <= limits.fill_cap,
        severity: "error",
        value: fill_screens as f64,
        limit: limits.fill_cap as f64,
        emitter: None,
        message: format!(
            "about {fill_screens:.2} screens of particle pixels at {} m ({}x{}, {}°): big, long-lived particles cost fill on the iGPU; fewer or smaller ones",
            limits.view_distance, limits.width, limits.height, limits.fov_y_deg
        ),
    });
    for e in &effect.emitters {
        // Too faint or too small to see at the reference view: work for nothing.
        let max_px = particle_px(e.size.max() * e.size_random[1] * params.size, limits.view_distance, limits.fov_y_deg, limits.height);
        let vis = e.alpha.max() * params.alpha.min(1.0);
        let seen = max_px * vis;
        out.push(Check {
            check: "visible",
            ok: seen >= 0.75,
            severity: "warning",
            value: seen as f64,
            limit: 0.75,
            emitter: Some(e.name.clone()),
            message: format!("its particles reach {max_px:.1} px at alpha {vis:.2} at {} m: below about a pixel they can't be seen", limits.view_distance),
        });
        // Additive colours far above white clip to white in the tone curve: the hue goes.
        if e.additive > 0.5 {
            let tint = params.tint.map(srgb_to_linear);
            let hottest = e.color.colors().map(|c| (0..3).map(|k| srgb_to_linear(c[k]) * tint[k]).fold(0.0, f32::max)).fold(0.0, f32::max);
            let blow = hottest * e.emissive * params.intensity * e.alpha.max();
            out.push(Check {
                check: "blowout",
                ok: blow <= 8.0,
                severity: "warning",
                value: blow as f64,
                limit: 8.0,
                emitter: Some(e.name.clone()),
                message: "additive brightness (colour × emissive × alpha, linear) far above white clips to white once particles overlap: the effect loses its colour family; lower emissive".into(),
            });
        }
        if e.space == Space::World {
            let tracked = Track::CAPACITY as f32 / limits.tick_hz.max(1) as f32;
            out.push(Check {
                check: "trail_track",
                ok: e.lifetime[1] <= tracked,
                severity: "warning",
                value: e.lifetime[1] as f64,
                limit: tracked as f64,
                emitter: Some(e.name.clone()),
                message: format!("world-space particles living past {tracked:.1} s (the emitter track's length at {} Hz) are placed at the track's oldest point", limits.tick_hz),
            });
        }
    }
    // WCAG 2.3.1: no more than three flashes in any second (particles, bursts, streams and the
    // light summed; brightening, darkening and red).
    let f = crate::flash::flash(effect, params);
    out.push(Check {
        check: "flash_rate",
        ok: f.hz <= crate::effect::MAX_FLASH_HZ,
        severity: "error",
        value: f.hz as f64,
        limit: crate::effect::MAX_FLASH_HZ as f64,
        emitter: f.emitter.and_then(|k| effect.emitters.get(k)).map(|e| e.name.clone()),
        message: format!(
            "{} (one instance at 3 m; the engine's flash watch checks the final image too): more than 3 flashes a second can trigger photosensitive seizures (WCAG 2.3.1)",
            if f.hz > 0.0 { f.describe() } else { "no flashes".into() }
        ),
    });
    for &k in &f.unchecked {
        out.push(Check {
            check: "flash_unchecked",
            ok: false,
            severity: "warning",
            value: 0.0,
            limit: 0.0,
            emitter: effect.emitters.get(k).map(|e| e.name.clone()),
            message: "a flipbook over an asset PNG animating faster than 3 frames a second: its frames' brightness isn't checked for flashing; keep neighbouring frames similar in brightness".into(),
        });
    }
    if let Some(pal) = palette.filter(|p| !p.is_empty()) {
        let tint = params.tint;
        let mut worst = (0.0f32, String::new(), [0.0f32; 3]);
        for e in &effect.emitters {
            for c in e.color.colors() {
                let c = [c[0] * tint[0], c[1] * tint[1], c[2] * tint[2]];
                let d = pal
                    .iter()
                    .map(|p| {
                        let (a, b, e) = (c[0] - p[0], c[1] - p[1], c[2] - p[2]);
                        (a * a + b * b + e * e).sqrt()
                    })
                    .fold(f32::INFINITY, f32::min);
                if d > worst.0 {
                    worst = (d, e.name.clone(), c);
                }
            }
        }
        out.push(Check {
            check: "palette",
            ok: worst.0 <= limits.palette_distance,
            severity: "warning",
            value: worst.0 as f64,
            limit: limits.palette_distance as f64,
            emitter: (!worst.1.is_empty()).then_some(worst.1),
            message: format!(
                "its farthest colour ({}) is {:.2} from the nearest palette colour: off its style family",
                crate::curve::hex_of(worst.2),
                worst.0
            ),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(text: &str) -> Effect {
        Effect::parse(&Json::parse(text).unwrap()).unwrap()
    }

    #[test]
    fn budgets_and_flashes_fail_as_errors() {
        let heavy = effect(r#"{"emitters": [{"rate": 2000, "lifetime": 2, "size": 2}]}"#);
        let r = check(&heavy, &Params::default(), &Limits::default(), None);
        let get = |n: &str| r.iter().find(|c| c.check == n).unwrap().clone();
        assert!(!get("peak_particles").ok && get("peak_particles").value == 4000.0);
        assert!(!get("overdraw").ok);
        // A strobing light is refused when parsed (WCAG 2.3.1); a slow flicker passes the check.
        let strobe = Effect::parse(&Json::parse(r#"{"emitters": [{"rate": 1}], "light": {"flicker": 0.8, "flicker_hz": 8}}"#).unwrap()).unwrap_err();
        assert_eq!((strobe.code, strobe.field.as_str()), ("photosensitive", "light.flicker_hz"));
        let slow = effect(r#"{"emitters": [{"rate": 1}], "light": {"flicker": 0.8, "flicker_hz": 3}}"#);
        let r = check(&slow, &Params::default(), &Limits::default(), None);
        assert!(r.iter().any(|c| c.check == "flash_rate" && c.ok));
    }

    #[test]
    fn a_spiking_size_counts_by_its_square() {
        // A size curve near 0.05 m that spikes to 4 m for a tenth of the life: its mean size is
        // small, but its mean of size² (and a burst's peak) isn't.
        let spike = effect(r##"{"emitters": [{"bursts": [{"count": 300}], "lifetime": 1, "speed": 0, "size": [[0, 0.05], [0.4, 0.05], [0.45, 4], [0.55, 4], [0.6, 0.05], [1, 0.05]], "alpha": [[0, 0], [0.1, 1], [0.9, 1], [1, 0]], "color": "#606060", "blend": "alpha"}]}"##);
        let e = &spike.emitters[0];
        let mean = e.size.mean();
        assert!(e.mean_area() > 4.0 * mean * mean, "{} vs {}", e.mean_area(), mean * mean);
        assert!(e.max_area() >= 15.0);
        let r = check(&spike, &Params::default(), &Limits::default(), None);
        assert!(r.iter().any(|c| c.check == "overdraw" && !c.ok), "the burst's peak is counted");
    }

    #[test]
    fn palette_flags_colours_off_the_family() {
        let cyan = [[0x3f as f32 / 255.0, 0xe0 as f32 / 255.0, 0xd0 as f32 / 255.0]];
        let fire = effect(r##"{"emitters": [{"rate": 5, "color": "#ff7a1f"}]}"##);
        let r = check(&fire, &Params::default(), &Limits::default(), Some(&cyan));
        assert!(r.iter().any(|c| c.check == "palette" && !c.ok));
        let pale = effect(r##"{"emitters": [{"rate": 5, "color": "#ffffff"}]}"##);
        let tinted = Params { tint: cyan[0], ..Params::default() };
        let r = check(&pale, &tinted, &Limits::default(), Some(&cyan));
        assert!(r.iter().any(|c| c.check == "palette" && c.ok), "a pale effect tinted cyan is cyan");
    }
}

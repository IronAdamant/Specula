//! A style's palette and its **blends**: where a recipe's colours may legitimately land.
//!
//! Recipes colour vertices with palette names, then gradients, bevel tints and noise tints blend
//! them, and ground shading darkens them. So a colour is on the palette when it lies near a
//! *blend*: a palette colour, a segment between two palette colours of the same group (one family,
//! or the neutrals), a segment between any two colours the recipe itself names (a wraith's shroud
//! fading from mist to grave green), or a segment from a palette colour towards black. Distance is
//! Oklab ΔE to the nearest point of the nearest blend (segments are taken in sRGB space, where the
//! recipe operations blend). A blend's family is the family of its nearer end (the shroud is
//! neutral at the mist end and the enemy's at the green end).
//!
//! *Why only named pairs across groups:* segments between every pair of palette colours cover so
//! much of colour space that a raw brown passes as "a fire tint".

use crate::colour::{nearest_on_segment, oklab, Rgb};
use crate::rules::StyleRules;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// The most colours a palette may have: the search for each surface colour's nearest blend
/// grows with the square of the palette (`Bible::load` refuses more).
pub const MAX_PALETTE: usize = 64;
/// Most answers [`Palette::nearest`] keeps (a call's assets share colours and named sets).
const MAX_CACHED: usize = 65_536;

/// [`Palette::nearest`]'s answers by (colour bits, the palette entries named).
type NearestCache = BTreeMap<([u32; 3], Vec<usize>), Option<Nearest>>;

/// The nearest blend to a colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Nearest {
    pub delta_e: f32,
    /// `carved_stone`, `carved_shadow-carved_stone` or `carved_stone shaded`.
    pub blend: String,
    /// The palette names the blend is made of (one or two).
    pub names: Vec<String>,
    /// The family of the nearer end (`None`: neutral).
    pub family: Option<String>,
    /// A family the blend involves at either end (the nearer one's when both are families):
    /// a neutral tinted towards a family colour reads as that family once it is colourful.
    pub touches: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    /// (name, colour, group): group 0 is neutral, k > 0 is `families[k - 1]`.
    entries: Vec<(String, Rgb, usize)>,
    /// Each entry's Oklab.
    labs: Vec<[f32; 3]>,
    families: Vec<String>,
    /// Answers so far, by (colour bits, the named colours that are on this palette).
    cache: RefCell<NearestCache>,
}

impl Palette {
    /// `colours` in a fixed order (a `style.json` palette); groups from the rules' families.
    pub fn new<'a>(colours: impl IntoIterator<Item = (&'a str, Rgb)>, rules: &StyleRules) -> Palette {
        let families: Vec<String> = rules.palette.families.iter().map(|(f, _)| f.clone()).collect();
        let entries: Vec<(String, Rgb, usize)> = colours
            .into_iter()
            .map(|(n, c)| {
                let g = rules.palette.families.iter().position(|(_, m)| m.iter().any(|x| x == n)).map_or(0, |i| i + 1);
                (n.to_string(), c, g)
            })
            .collect();
        let labs = entries.iter().map(|(_, c, _)| oklab(*c)).collect();
        Palette { entries, labs, families, cache: RefCell::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn colour(&self, name: &str) -> Option<Rgb> {
        self.entries.iter().find(|(n, _, _)| n == name).map(|(_, c, _)| *c)
    }

    fn family(&self, g: usize) -> Option<String> {
        (g > 0).then(|| self.families[g - 1].clone())
    }

    /// The nearest blend to `c` for an asset whose recipe names the colours `named`; `None` for
    /// an empty palette. Ties go to the earlier blend.
    pub fn nearest(&self, c: Rgb, named: &[String]) -> Option<Nearest> {
        // Cached by the colour and the palette entries named (names off the palette change nothing).
        let key = (c.map(f32::to_bits), self.entries.iter().enumerate().filter(|(_, e)| named.contains(&e.0)).map(|(i, _)| i).collect::<Vec<_>>());
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let answer = self.search(c, named);
        let mut cache = self.cache.borrow_mut();
        if cache.len() < MAX_CACHED {
            cache.insert(key, answer.clone());
        }
        answer
    }

    fn search(&self, c: Rgb, named: &[String]) -> Option<Nearest> {
        // (distance, blend, names, nearer end's group, a family group either end has)
        let mut best: Option<(f32, String, Vec<String>, usize, usize)> = None;
        let mut consider = |d: f32, blend: &dyn Fn() -> (String, Vec<String>), g: usize, touch: usize| {
            if best.as_ref().is_none_or(|b| d < b.0) {
                let (s, n) = blend();
                best = Some((d, s, n, g, touch));
            }
        };
        let black = [0.0f32; 3];
        let lab = oklab(c);
        let dist = |p: [f32; 3]| ((p[0] - lab[0]).powi(2) + (p[1] - lab[1]).powi(2) + (p[2] - lab[2]).powi(2)).sqrt();
        // Every blend, measured exactly: the nearest point in sRGB (where recipes blend),
        // converted to Oklab. Exhaustive on purpose: a filtered search (rank by the Oklab chord,
        // measure the closest few) misjudged colours near the threshold, and the studio's checks
        // run on the caller's thread, where this costs no frames.
        for (i, (na, a, ga)) in self.entries.iter().enumerate() {
            consider(dist(self.labs[i]), &|| (na.clone(), vec![na.clone()]), *ga, *ga);
            let (t, q) = nearest_on_segment(c, black, *a);
            if t < 1.0 {
                consider(dist(oklab(q)), &|| (format!("{na} shaded"), vec![na.clone()]), *ga, *ga);
            }
            let a_named = named.iter().any(|n| n == na);
            for (nb, b, gb) in &self.entries[i + 1..] {
                if gb != ga && !(a_named && named.iter().any(|n| n == nb)) {
                    continue;
                }
                let (t, q) = nearest_on_segment(c, *a, *b);
                if t > 0.0 && t < 1.0 {
                    let (near, far) = if t <= 0.5 { (*ga, *gb) } else { (*gb, *ga) };
                    let touch = if near > 0 { near } else { far };
                    consider(dist(oklab(q)), &|| (format!("{na}-{nb}"), vec![na.clone(), nb.clone()]), near, touch);
                }
            }
        }
        best.map(|(d, blend, names, g, touch)| Nearest { delta_e: d, blend, names, family: self.family(g), touches: self.family(touch) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::parse_hex;

    fn rules() -> StyleRules {
        let pal = |n: &str| match n {
            "rune_glow" | "carved_stone" | "carved_shadow" | "grave_glow" => Some([0.0; 3]),
            _ => None,
        };
        StyleRules::parse("[palette]\nfamilies = { player = [\"rune_glow\"], enemy = [\"grave_glow\"] }\n", "t", &pal).unwrap()
    }

    fn palette() -> Palette {
        let p = |h: &str| parse_hex(h).unwrap();
        Palette::new([("carved_stone", p("#8c96a3")), ("carved_shadow", p("#6b7480")), ("rune_glow", p("#3fe0d0")), ("grave_glow", p("#9cff4f"))], &rules())
    }

    #[test]
    fn blends_of_a_family_are_on_the_palette() {
        let p = palette();
        let exact = p.nearest(parse_hex("#8c96a3").unwrap(), &[]).unwrap();
        assert!(exact.delta_e < 1e-4 && exact.blend == "carved_stone" && exact.family.is_none());
        // Halfway between the two neutrals.
        let mid = p.nearest(parse_hex("#7c858f").unwrap(), &[]).unwrap();
        assert!(mid.delta_e < 0.01 && mid.family.is_none(), "{mid:?}");
        // Shaded towards black.
        let dark = p.nearest(parse_hex("#464b52").unwrap(), &[]).unwrap();
        assert!(dark.delta_e < 0.01 && dark.blend.ends_with("shaded"), "{dark:?}");
        // The glow is in its family.
        assert_eq!(p.nearest(parse_hex("#3fe0d0").unwrap(), &[]).unwrap().family.as_deref(), Some("player"));
    }

    #[test]
    fn blends_across_groups_need_the_recipe_to_name_both() {
        let p = palette();
        // A quarter of the way from rune cyan to grave green: off the palette, unless the recipe
        // names both (then the player's, the nearer end).
        let c = parse_hex("#56e8b3").unwrap();
        assert!(p.nearest(c, &[]).unwrap().delta_e > 0.05);
        let named = ["grave_glow".to_string(), "rune_glow".to_string()];
        let mix = p.nearest(c, &named).unwrap();
        assert!(mix.delta_e < 0.02 && mix.family.as_deref() == Some("player"), "{mix:?}");
        // Mostly carved stone with a little green, both named: neutral, touching the enemy family.
        let tint = p.nearest(parse_hex("#90a597").unwrap(), &["carved_stone".to_string(), "grave_glow".to_string()]).unwrap();
        assert!(tint.family.is_none() && tint.touches.as_deref() == Some("enemy"), "{tint:?}");
        // A saturated red is far from everything.
        assert!(p.nearest(parse_hex("#d02020").unwrap(), &named).unwrap().delta_e > 0.15);
        assert!(Palette::default().nearest([0.5; 3], &[]).is_none());
    }
}

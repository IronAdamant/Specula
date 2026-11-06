//! Words in the graveyard (M5 text lane): epitaphs carved into the headstones, and the map's
//! title cut into a stone by the graveyard. In-world `text` entities (drawn from glyph outlines,
//! lit by the moon like the stone they sit on), children of what they're carved into, so a
//! toppled headstone takes its epitaph with it.
//!
//! The face is the bundled display serif (Alegreya, SIL OFL; STYLE.md: "decorative serif only for
//! titles"), in a dark stone colour so the letters read as cut, not painted.

use crate::ents;
use specula::text_world::TextSpec;
use specula::two_d::TwoD;
use specula::game::GameWorld;
use specula::world::{EntityKey, Shape, SpawnSpec};
use specula_core::math::Vec3;
use specula_text::{Align, Bundled};

/// Cut letters: darker than the stone around them (STYLE.md graveyard neutrals).
const CUT: [f32; 3] = [0.22, 0.21, 0.2];

/// Epitaphs, Latin first (the setting), then the other scripts the bundled faces carry.
const EPITAPHS: [&str; 12] = [
    "HIC IACET\nADA",
    "REQUIESCAT\nIN PACE",
    "MEMENTO\nMORI",
    "Здесь покоится\nИван",
    "Ἐνθάδε κεῖται\nΝίκη",
    "HIC IACET\nBRAN",
    "VIXIT\nANNOS LX",
    "Requiem\naeternam",
    "Вечная\nпамять",
    "ΜΝΗΜΗ",
    "HIC IACET\nOSWIN",
    "SIT TIBI\nTERRA LEVIS",
];

/// `p` leaned as the recipes lean their stones: `rotate [-4 tilt, 0, 3 tilt]` degrees (x, then z)
/// about a pivot at height `pivot_y`.
fn lean(p: Vec3, pivot_y: f32, tilt: f32) -> Vec3 {
    let (rx, rz) = ((-4.0 * tilt).to_radians(), (3.0 * tilt).to_radians());
    let p = p - Vec3::new(0.0, pivot_y, 0.0);
    let (sx, cx) = specula_core::dmath::sin_cos(rx);
    let q = Vec3::new(p.x, p.y * cx - p.z * sx, p.y * sx + p.z * cx);
    let (sz, cz) = specula_core::dmath::sin_cos(rz);
    Vec3::new(q.x * cz - q.y * sz, q.x * sz + q.y * cz, q.z) + Vec3::new(0.0, pivot_y, 0.0)
}

/// Where a headstone's words go, in its own frame (before its scale): position, rotation
/// (degrees), width and size (m per em). `tombstone.json`: an arch 0.5 x 0.8 m with its front at
/// z = 0.075 and a raised cross from y = 0.40 up, so the words sit below the cross;
/// `headstone_carved.json` variant 1: a sunk panel 0.34 x 0.22 m from y = 0.2, its floor at
/// z = 0.06. Both lean by `tilt` (pivots at 0.05 and 0.07 m). Variants 2 and 3 (a ringed cross, a
/// broken obelisk) carry no words.
fn face(index: usize) -> Option<(Vec3, Vec3, f32, f32)> {
    let tilt = ((index * 37) % 13) as f32 / 6.0 - 1.0;
    let rot = Vec3::new(-4.0 * tilt, 0.0, 3.0 * tilt);
    match index % 4 {
        0 | 2 => Some((lean(Vec3::new(0.0, 0.25, 0.079), 0.05, tilt), rot, 0.36, 0.06)),
        1 => Some((lean(Vec3::new(0.0, 0.31, 0.0615), 0.07, tilt), rot, 0.3, 0.05)),
        _ => None,
    }
}

/// The words of headstone `index` (1-based in the world, 0-based here).
pub fn epitaph(index: usize) -> &'static str {
    EPITAPHS[index % EPITAPHS.len()]
}

fn carved(text: &str, font: Bundled, size: f32, width: f32, lines: u32) -> TextSpec {
    let mut s = TextSpec::new(text);
    s.font = font;
    s.size = size;
    s.align = Align::Center;
    s.max_width = Some(width);
    s.max_lines = Some(lines);
    s.lit = true;
    s.refresh().expect("plain text in its limits");
    s
}

/// Carves headstone `index`'s epitaph into entity `stone` (its child, `epitaph`).
pub fn carve(world: &mut GameWorld<'_>, agent: &str, stone: u64, index: usize) {
    let Some((pos, rot, width, size)) = face(index) else { return };
    if stone == 0 {
        return;
    }
    let mut spec = SpawnSpec::new(Shape::Text);
    spec.name = Some("epitaph".into());
    spec.parent = Some(EntityKey::Id(stone));
    spec.position = pos;
    spec.rotation_deg = rot;
    spec.color = CUT;
    spec.tags = ents::tags(&["epitaph"]);
    spec.two_d = Some(TwoD::Text(Box::new(carved(epitaph(index), Bundled::Display, size, width, 3))));
    ents::spawn(world, agent, spec);
}

/// The map's title stone: a low stone block at `at` (on the ground, facing +Z turned `yaw_deg`)
/// with the game's name and the map's cut into its face.
pub fn title_stone(world: &mut GameWorld<'_>, agent: &str, parent: u64, at: Vec3, yaw_deg: f32, title: &str) {
    let mut block = SpawnSpec::new(Shape::Cube);
    block.name = Some("title_stone".into());
    block.parent = (parent != 0).then_some(EntityKey::Id(parent));
    // A standing stele, tall enough for its words to clear the graveyard's fence.
    block.position = at + Vec3::new(0.0, 0.95, 0.0);
    block.rotation_deg = Vec3::new(0.0, yaw_deg, 0.0);
    block.scale = Vec3::new(2.6, 1.9, 0.35);
    block.color = [0.55, 0.57, 0.6];
    block.tags = ents::tags(&["title_stone"]);
    let id = ents::spawn(world, agent, block);
    if id == 0 {
        return;
    }
    // The words are children of the block but must not take its scale: a group carries them.
    let mut face = SpawnSpec::new(Shape::Group);
    face.name = Some("title_words".into());
    face.parent = (parent != 0).then_some(EntityKey::Id(parent));
    face.position = at + Vec3::new(0.0, 0.95, 0.0);
    face.rotation_deg = Vec3::new(0.0, yaw_deg, 0.0);
    face.tags = ents::tags(&["title_stone"]);
    let words = ents::spawn(world, agent, face);
    if words == 0 {
        return;
    }
    for (name, text, y, size, width) in [("title", "SEPULCRA", 0.62, 0.26, 2.4), ("subtitle", title, 0.34, 0.11, 2.4)] {
        let mut spec = SpawnSpec::new(Shape::Text);
        spec.name = Some(name.into());
        spec.parent = Some(EntityKey::Id(words));
        spec.position = Vec3::new(0.0, y, 0.176);
        spec.color = CUT;
        spec.tags = ents::tags(&["title_stone"]);
        spec.two_d = Some(TwoD::Text(Box::new(carved(text, Bundled::Display, size, width, 1))));
        ents::spawn(world, agent, spec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_epitaph_fits_its_stone_in_the_bundled_face() {
        for i in 0..EPITAPHS.len() * 4 {
            let Some((_, _, width, size)) = face(i) else { continue };
            let s = carved(epitaph(i), Bundled::Display, size, width, 3);
            let l = s.lay_out();
            assert!(l.missing.is_empty(), "{i}: {:?}", l.missing);
            assert_eq!(l.truncated, None, "{i}: '{}' is cut", epitaph(i));
            let (lo, hi) = s.bounds();
            assert!(hi.x - lo.x <= width + 1e-3 && hi.y - lo.y < 0.3, "{i}: {lo:?} {hi:?}");
        }
        assert!(face(3).is_none() && face(1).is_some());
    }
}

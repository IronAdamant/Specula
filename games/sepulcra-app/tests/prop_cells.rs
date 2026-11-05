//! The cells each prop of map 1 covers (`cells` in the map data) are checked against the props'
//! own meshes: a stone may stand on a cell only where it can't meet a prop that blocks building,
//! and dressing hides under a stone only if the stone's cell is listed. So for every prop, the
//! declared cells must be exactly the fragments whose stone footprint (the widest stone, tier III,
//! at its cell's own turn, as the scene places it) its mesh overlaps (lane review, finding 5: the
//! cells were declared by hand and several props reached cells they didn't name). A prop must
//! also keep off the lanes, where walls stand.

use sepulcra::map::{CellKind, Face};
use sepulcra::Content;
use specula_core::math::{Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Half the side of the widest stone's footprint (`brazier_cairn` tier III, 1.634 m), measured
/// from the recipes (style_check facts).
const STONE_HALF_M: f32 = 0.817;
/// A wall stands across its lane cell: this far either side of the lane's centre line (the
/// `wall_segment` recipe, about 1.2 m deep across the lane's 2 m).
const LANE_KEEP_OFF_M: f32 = 0.0;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets")
}

/// Projections of a convex polygon on an axis.
fn span(poly: &[[f32; 2]], axis: [f32; 2]) -> (f32, f32) {
    poly.iter().map(|p| p[0] * axis[0] + p[1] * axis[1]).fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)))
}

/// Whether two convex polygons overlap by more than `eps` (separating axis test).
fn overlap(a: &[[f32; 2]], b: &[[f32; 2]], eps: f32) -> bool {
    for poly in [a, b] {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let n = [q[1] - p[1], p[0] - q[0]];
            let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
            if len < 1e-9 {
                continue;
            }
            let n = [n[0] / len, n[1] / len];
            let (a0, a1) = span(a, n);
            let (b0, b1) = span(b, n);
            if a1 <= b0 + eps || b1 <= a0 + eps {
                return false;
            }
        }
    }
    true
}

fn square(cx: f32, cz: f32, half: f32, yaw_deg: f32) -> Vec<[f32; 2]> {
    let q = Quat::from_euler_xyz_deg(0.0, yaw_deg, 0.0);
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(sx, sz)| {
            let p = q.rotate(Vec3::new(sx * half, 0.0, sz * half));
            [cx + p.x, cz + p.z]
        })
        .collect()
}

/// Each prop's mesh (LOD 0) as world-space triangles on the ground plane.
fn footprints(content: &Content) -> BTreeMap<String, Vec<[[f32; 2]; 3]>> {
    let dir = assets().join("recipes");
    let style = specula_model::Style::parse(&std::fs::read_to_string(assets().join("style.json")).unwrap()).unwrap();
    let mut library = BTreeMap::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            library.insert(p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let mut out = BTreeMap::new();
    for p in &content.map.props {
        let text = &library[&p.recipe];
        let params = specula_model::BuildParams { library: Some(&library), ..specula_model::BuildParams::new(&style) };
        let model = specula_model::build(text, &params).unwrap_or_else(|e| panic!("{}: {e}", p.recipe));
        let mesh = &model.lods[0];
        let q = Quat::from_euler_xyz_deg(0.0, p.yaw_deg(), 0.0);
        let [x, _, z] = p.at.to_f32();
        let world: Vec<[f32; 2]> = mesh.positions.iter().map(|v| {
            let r = q.rotate(Vec3::new(v[0], v[1], v[2]));
            [x + r.x, z + r.z]
        }).collect();
        let tris = mesh.indices.chunks(3).map(|t| [world[t[0] as usize], world[t[1] as usize], world[t[2] as usize]]).collect();
        out.insert(p.name.clone(), tris);
    }
    out
}

/// The scene's turn for a stone on cell `id` (`Scene::sync_stones`).
fn stone_yaw(id: u16) -> f32 {
    ((u32::from(id) * 7919) % 11) as f32 - 5.0
}

#[test]
fn declared_cells_are_the_fragments_a_stone_would_meet() {
    let content = Content::builtin();
    let map = &content.map;
    let feet = footprints(&content);
    let top = map.face(Face::Top).copied().unwrap();
    let mut wrong = Vec::new();
    let mut on_lanes = Vec::new();
    for p in &map.props {
        let tris = &feet[&p.name];
        let mut met = BTreeSet::new();
        for r in 0..top.rows {
            for c in 0..top.cols {
                let id = map.at(Face::Top, c, r).unwrap();
                let cell = map.cell(id);
                let [cx, _, cz] = cell.world_pos_f32();
                match cell.kind {
                    CellKind::Fragment => {
                        let stone = square(cx, cz, STONE_HALF_M, stone_yaw(id.0));
                        if tris.iter().any(|t| overlap(t, &stone, 1e-3)) {
                            met.insert(id);
                        }
                    }
                    k if k.wallable() => {
                        let half = map.cell_mm as f32 / 2000.0 - LANE_KEEP_OFF_M;
                        let lane = square(cx, cz, half, 0.0);
                        if tris.iter().any(|t| overlap(t, &lane, 1e-3)) {
                            on_lanes.push(format!("{} reaches into lane {}", p.name, map.cell_name(id)));
                        }
                    }
                    _ => {}
                }
            }
        }
        let declared: BTreeSet<_> = p.cells.iter().copied().collect();
        if met != declared {
            let names = |s: &BTreeSet<sepulcra::CellId>| s.iter().map(|c| map.cell_name(*c)).collect::<Vec<_>>();
            wrong.push(format!("{}: declared {:?}, meets {:?}", p.name, names(&declared), names(&met)));
        }
    }
    assert!(wrong.is_empty() && on_lanes.is_empty(), "props whose cells differ from what their meshes cover:\n{}\nprops on lanes:\n{}", wrong.join("\n"), on_lanes.join("\n"));
}

/// DESIGN.md says how many fragments the big props block; it must be the data's count (lane
/// review, finding 6: it said 33 where the data blocked 32).
#[test]
fn design_md_counts_the_blocked_fragments_right() {
    let content = Content::builtin();
    let map = &content.map;
    let blocked = map.cells().iter().filter(|c| map.blocking_prop(c.id).is_some()).count();
    let design = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/DESIGN.md")).unwrap().replace("\r\n", "\n").replace('\n', " ");
    let said = design.split(" fragments in all").next().and_then(|s| s.rsplit(' ').next()).and_then(|n| n.parse::<usize>().ok());
    assert_eq!(said, Some(blocked), "DESIGN.md's \"N fragments in all\" (the props paragraph) must say {blocked}");
}

/// The test itself: the overlap test separates, touches and overlaps as it should.
#[test]
fn the_overlap_test_is_right() {
    let a = square(0.0, 0.0, 1.0, 0.0);
    assert!(overlap(&a, &square(1.9, 0.0, 1.0, 0.0), 1e-3));
    assert!(!overlap(&a, &square(2.0, 0.0, 1.0, 0.0), 1e-3), "touching edges don't count");
    assert!(!overlap(&a, &square(2.5, 0.0, 1.0, 45.0), 1e-3));
    assert!(overlap(&a, &square(2.3, 0.0, 1.0, 45.0), 1e-3), "a turned square's corner reaches further");
    let tri = [[0.5, 0.5], [3.0, 0.5], [0.5, 3.0]];
    assert!(overlap(&tri, &a, 1e-3) && !overlap(&[[1.2, 1.2], [3.0, 1.2], [1.2, 3.0]], &a, 1e-3));
}

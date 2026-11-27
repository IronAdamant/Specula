//! The document's test (review 2 of S0, item 6): a second plan written from EXEMPLARIA.md's "For
//! the plans' lanes" alone, without reading `box_creature.rs` again: a table, a top on four
//! legs, four numbers and two relations. It stays in the tests: it is not a library entry. What
//! the document left to guessing, and was mended there, is listed in the lane's handoff.

use crate::draw::Specimen;
use crate::error::Error;
use crate::kind::Kind;
use crate::plan::{resolve_among, MotionDef, NumberDef, OptionDef, Plan, RelationDef, RoleDef, SizeDef, TraitDef, Unit};
use crate::tests_build::{mesh_volume, no_shared_face, ByStyle, STYLE};
use crate::write::{expand, Writing};
use specula_model::{build, BuildParams, Style};

struct Table;
static TABLE: Table = Table;

/// Ground to the top's upper face. The write uses only the numbers (a relation cannot name the
/// size); the size scales them.
static SIZE: SizeDef = SizeDef { name: "height", unit: Unit::Metre, mean: 0.75, sd: 0.02, range: [0.06, 37.0], about: "ground to the table's top" };

/// The least of each: its smallest fixed difference of two faces is three steps (a leg's side
/// 0.1 of its girth inside the top's edge: 3 mm; the leg's top half the top's thickness into it:
/// 0.6 mm, so 1 mm); the top's sides keep a least girth apart by relation (2.2 girths and the
/// margin: 7 mm). The most: 60 m. The size: the girth's least at its share less the build's 2.5
/// spreads (0.003 / (0.06 x 0.875) = 0.057), to the length's most at its share (60 / 1.6).
static NUMBERS: [NumberDef; 5] = [
    NumberDef { name: "top_length", unit: Unit::Metre, share: 1.6, exponent: 1.0, traits: &[], range: [0.007, 60.0], about: "the top, end to end" },
    NumberDef { name: "top_width", unit: Unit::Metre, share: 1.0, exponent: 1.0, traits: &[], range: [0.007, 60.0], about: "the top, side to side" },
    NumberDef { name: "top_thickness", unit: Unit::Metre, share: 0.05, exponent: 1.0, traits: &[("build", 1.0)], range: [0.001, 60.0], about: "the top's thickness" },
    NumberDef { name: "leg_girth", unit: Unit::Metre, share: 0.06, exponent: 1.0, traits: &[("build", 1.0)], range: [0.003, 60.0], about: "a leg's width" },
    NumberDef { name: "leg_length", unit: Unit::Metre, share: 0.95, exponent: 1.0, traits: &[], range: [0.001, 60.0], about: "ground to the top's underside" },
];

/// A leg's centre is 0.6 girths inside the top's edge: two legs' inner faces are apart by the
/// top's width (or length) less 2.2 girths.
static RELATIONS: [RelationDef; 2] = [
    RelationDef { name: "legs_apart_across", terms: &[("top_width", 1.0), ("leg_girth", -2.2)], margin: 0.0003, yields: "leg_girth", about: "the two legs of an end do not meet" },
    RelationDef { name: "legs_apart_along", terms: &[("top_length", 1.0), ("leg_girth", -2.2)], margin: 0.0003, yields: "leg_girth", about: "the two legs of a side do not meet" },
];

static TRAITS: [TraitDef; 1] = [TraitDef { name: "build", spread: 0.05, about: "the top's thickness and the legs' girth" }];
static ROLES: [RoleDef; 1] = [RoleDef { name: "coat", hint: "#8a6a40", about: "the wood" }];

impl Plan for Table {
    fn name(&self) -> &'static str {
        "test_table"
    }
    fn about(&self) -> &'static str {
        "a table: a top on four legs (the document's test plan)"
    }
    fn size(&self) -> &'static SizeDef {
        &SIZE
    }
    fn numbers(&self) -> &'static [NumberDef] {
        &NUMBERS
    }
    fn traits(&self) -> &'static [TraitDef] {
        &TRAITS
    }
    fn roles(&self) -> &'static [RoleDef] {
        &ROLES
    }
    fn options(&self) -> &'static [OptionDef] {
        &[]
    }
    fn motions(&self) -> &'static [MotionDef] {
        &[]
    }
    fn relations(&self) -> &'static [RelationDef] {
        &RELATIONS
    }
    fn write(&self, s: &Specimen, w: &mut Writing) -> Result<(), Error> {
        let (tl, tw, tt, g, ll) = (s.get("top_length"), s.get("top_width"), s.get("top_thickness"), s.get("leg_girth"), s.get("leg_length"));
        w.bone("root", None, [0.0; 3], None);
        w.block("top", "coat", "root", [tw, tt, tl], [0.0, ll, 0.0])?;
        let (x, z) = (tw / 2.0 - 0.6 * g, tl / 2.0 - 0.6 * g);
        for (i, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)].into_iter().enumerate() {
            // Into the top by half its thickness: joined parts overlap.
            w.block(&format!("leg_{i}"), "coat", "root", [g, ll + 0.5 * tt, g], [sx * x, 0.0, sz * z])?;
        }
        w.fact("height_built", ll + tt, "m");
        Ok(())
    }
}

fn table(extra: &str) -> Result<crate::Resolved, Error> {
    let text = format!(r#"{{"kind": 1, "name": "desk", "plan": "test_table", "about": "a desk"{extra}}}"#);
    resolve_among(&Kind::parse(&text).unwrap(), None, &[&TABLE])
}

#[test]
fn a_table_written_from_the_document_builds_whole_and_refuses_what_it_cannot_make() {
    let style = Style::parse(STYLE).unwrap();
    let before = crate::draw::backstops::counts();
    let kinds = [
        table("").unwrap(),
        table(r#", "size": {"height": {"mean": 0.45, "sd": 0.03, "unit": "m", "own": "a coffee table"}}, "vary": {"build": {"spread": 0.1, "own": "x"}}"#).unwrap(),
        table(r#", "numbers": {"top_width": {"share": 0.2, "spread": 0.1, "own": "a narrow shelf"}, "leg_girth": {"share": 0.03, "own": "x"}}"#).unwrap(),
        table(r#", "size": {"height": {"mean": 0.07, "unit": "m", "own": "a doll's table"}}"#).unwrap(),
    ];
    for r in &kinds {
        for seed in 0..24u64 {
            let x = expand(r, Some(seed * 7_919), &ByStyle(&style), "t").unwrap();
            let m = build(&x.recipe, &BuildParams { seed: Some(seed), ..BuildParams::new(&style) }).unwrap_or_else(|e| panic!("{e}\n{}", x.recipe));
            assert_eq!(m.open_edges, 0, "{}", x.recipe);
            let boxes = no_shared_face(&x.recipe).unwrap_or_else(|e| panic!("{e}\n{}", x.recipe));
            let v = mesh_volume(&m.lods[0]);
            assert!((v - boxes).abs() <= 1e-4 * boxes, "{v} against {boxes}");
        }
    }
    assert_eq!(crate::draw::backstops::counts(), before, "no backstop acted");
    // Legs wider than half the top: refused by the relation, by name.
    let e = table(r#", "numbers": {"top_width": {"share": 0.2, "own": "x"}, "leg_girth": {"share": 0.1, "own": "x"}}"#).unwrap_err();
    assert_eq!((e.code, e.relation.as_ref().map(|b| b.relation.as_str())), ("outside_plan", Some("legs_apart_across")), "{e}");
    // Under a least: refused by its range.
    assert_eq!(table(r#", "numbers": {"leg_girth": {"mean": 0.002, "own": "x"}}"#).unwrap_err().path, "numbers.leg_girth.mean");
}

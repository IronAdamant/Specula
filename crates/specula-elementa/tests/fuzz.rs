//! A damaged section returns an error and never hangs, panics or allocates without bound; a
//! section that does load can be stepped.

mod common;

use common::{forest, forest_centre, sim};
use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_elementa::{Act, ActKind, Elementa, Target};

/// A small fire, saved mid-way with events still waiting.
fn saved() -> (Elementa, Vec<u8>) {
    let mut e = sim(r#"{"elements": 1, "materials": {"wood": {"embers": 0.5}}, "tags": {"tree": "wood"}}"#, 5);
    let water = e.config().material("water").unwrap();
    e.add_many(&forest(&e, 40, 1)).unwrap();
    e.add(specula_elementa::NodeDesc::new(500, water, Vec3::new(-3.0, 0.0, -3.0), Vec3::ONE)).unwrap();
    e.set_wind(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: forest_centre(40), radius: 1.5 }, amount: 1.0 }).unwrap();
    for _ in 0..90 {
        e.step();
    }
    assert!(!e.events().is_empty() && e.burning() > 0);
    let bytes = e.save();
    (e, bytes)
}

/// Whatever loaded must run: the state's checks hold the step's assumptions.
fn exercise(mut e: Elementa) {
    for t in 0..60 {
        if t == 10 {
            let _ = e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::ZERO, radius: 64.0 }, amount: 1.0 });
        }
        e.step();
    }
    e.take_events();
    let again = Elementa::load(&e.save()).expect("a state that ran saves and loads");
    assert_eq!(again.hash(), e.hash());
}

#[test]
fn every_cut_is_refused() {
    let (_, bytes) = saved();
    for len in 0..bytes.len() {
        let err = Elementa::load(&bytes[..len]).err().unwrap_or_else(|| panic!("a cut at {len} of {} loaded", bytes.len()));
        assert!(matches!(err.code, "bad_snapshot" | "snapshot_invalid" | "out_of_range" | "unsupported_version" | "limit" | "bad_config" | "too_large" | "bad_name" | "unknown_material"), "{err}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(Elementa::load(&longer).unwrap_err().code, "bad_snapshot", "trailing bytes");
}

#[test]
fn every_bit_flipped_is_refused_or_runs() {
    let (_, bytes) = saved();
    let mut loaded = 0;
    for i in 0..bytes.len() * 8 {
        let mut b = bytes.clone();
        b[i / 8] ^= 1 << (i % 8);
        if let Ok(e) = Elementa::load(&b) {
            loaded += 1;
            if i % 7 == 0 {
                exercise(e);
            }
        }
    }
    assert!(loaded > 0, "some flips (a float's low bits) still load");
}

#[test]
fn random_edits_with_extreme_values_are_refused_or_run() {
    let (_, bytes) = saved();
    let mut rng = Rng::named(9, "elementa-fuzz");
    let extremes: [u64; 6] = [0, 1, u64::MAX, u64::MAX >> 1, 1 << 31, 0x7f80_0000];
    for round in 0..3_000 {
        let mut b = bytes.clone();
        for _ in 0..1 + rng.below(3) {
            let width = [1usize, 2, 4, 8][rng.below(4) as usize];
            let at = rng.below(b.len().saturating_sub(width) as u32 + 1) as usize;
            let v = if rng.below(2) == 0 { extremes[rng.below(6) as usize] } else { rng.next_u64() };
            let end = (at + width).min(b.len());
            b[at..end].copy_from_slice(&v.to_le_bytes()[..end - at]);
        }
        if round % 5 == 0 {
            b.truncate(rng.below(b.len() as u32 + 1) as usize);
        }
        if let Ok(e) = Elementa::load(&b) {
            exercise(e);
        }
    }
}

#[test]
fn huge_counts_are_refused_before_anything_is_allocated() {
    // The first count in the section is the materials' (after the 4-byte version).
    let (_, bytes) = saved();
    let mut b = bytes.clone();
    b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let err = Elementa::load(&b).unwrap_err();
    assert_eq!(err.code, "bad_snapshot");
    assert_eq!(err.detail("decode").and_then(|v| v.as_str()), Some("too_long"));
    // A version from the future.
    let mut b = bytes.clone();
    b[0..4].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(Elementa::load(&b).unwrap_err().code, "unsupported_version");
    // Nothing at all.
    assert!(Elementa::load(&[]).is_err());
}

/// Where an 8-byte value sits in the section: found by its value, which must be there once.
fn find_u64(bytes: &[u8], v: u64) -> usize {
    let pat = v.to_le_bytes();
    let at: Vec<usize> = (0..=bytes.len() - 8).filter(|&i| bytes[i..i + 8] == pat).collect();
    assert_eq!(at.len(), 1, "{v} is in the section once");
    at[0]
}

#[test]
fn a_clock_or_a_count_at_its_end_is_refused_or_saturates() {
    // A state at a tick no other field holds, with a refusal count no other field holds.
    let mut e = sim(r#"{"elements": 1, "limits": {"nodes": 3, "burning": 3}}"#, 5);
    let wood = e.config().material("wood").unwrap();
    let d: Vec<specula_elementa::NodeDesc> =
        (0..1234u64).map(|i| specula_elementa::NodeDesc::new(i, wood, Vec3::new(i as f32 * 3.0, 0.0, 0.0), Vec3::ONE)).collect();
    e.add_many(&d).unwrap();
    for _ in 0..4321 {
        e.step();
    }
    let bytes = e.save();
    let tick_at = find_u64(&bytes, 4321);
    let refused_at = find_u64(&bytes, 1231);

    // The clock at its end, or past the latest tick a state may be at: refused.
    for t in [u64::MAX, specula_elementa::sim::MAX_TICK + 1] {
        let mut b = bytes.clone();
        b[tick_at..tick_at + 8].copy_from_slice(&t.to_le_bytes());
        let err = Elementa::load(&b).unwrap_err();
        assert_eq!(err.code, "snapshot_invalid", "tick {t}: {err}");
    }
    // At the latest tick: it loads and steps without wrapping.
    let mut b = bytes.clone();
    b[tick_at..tick_at + 8].copy_from_slice(&specula_elementa::sim::MAX_TICK.to_le_bytes());
    let mut l = Elementa::load(&b).unwrap();
    for _ in 0..12 {
        l.step();
    }
    assert_eq!(l.tick(), specula_elementa::sim::MAX_TICK);
    assert!(Elementa::load(&l.save()).is_ok());

    // The refusal count at its end: it loads, and a refused node leaves it there.
    let mut b = bytes.clone();
    b[refused_at..refused_at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut l = Elementa::load(&b).unwrap();
    assert_eq!(l.add(specula_elementa::NodeDesc::new(99_999, wood, Vec3::ZERO, Vec3::ONE)).unwrap_err().code, "limit");
    let r = l.add_many(&[specula_elementa::NodeDesc::new(99_998, wood, Vec3::ZERO, Vec3::ONE)]).unwrap();
    assert_eq!(r.left_out, 1);
    assert_eq!(l.refused(), u64::MAX);
}

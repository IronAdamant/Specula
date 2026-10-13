//! The per-tick path (advance with events, evaluate with a layer, skin) allocates nothing once
//! warmed up (F3: no per-tick heap churn for hundreds of creatures).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use specula_anim::math::Affine;
use specula_anim::{gen, AnimSet, Animator, LayerMode, Skeleton};
use specula_core::Json;
use specula_model::{Bone, Rig};

struct Count;
static N: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts calls.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        N.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same layout contract as the caller's.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: `p` came from `System.alloc` with layout `l`.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Count = Count;

#[test]
fn ticks_do_not_allocate() {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    let rig = Rig {
        bones: vec![
            b("root", None, [0.0; 3], [0.0; 3]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.3, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
            b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.05, 0.2]),
            b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
            b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.1, 0.0]),
            b("foot_r", Some(6), [-0.15, 0.1, 0.0], [-0.15, 0.05, 0.2]),
        ],
    };
    let s = Skeleton::from_rig(&rig).unwrap();
    let walk = gen::generate(
        &Json::parse(
            r#"{ "name": "walk", "kind": "gait", "duration": 0.2, "stride": 0.3, "pelvis": "hips",
      "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ] }"#,
        )
        .unwrap(),
        "",
        &s,
    )
    .unwrap();
    let mut set = AnimSet::new(s);
    let w = set.add(walk).unwrap();
    let mut a = Animator::new(&set, w);
    a.set_layer(0, w, LayerMode::Additive, 0.5, None, 0.0);
    let (mut m, mut k) = (Vec::<Affine>::new(), Vec::<Affine>::new());
    for _ in 0..100 {
        a.advance(&set, 1.0 / 60.0);
        a.evaluate(&set);
        a.skin(&set, &mut m, &mut k);
    }
    let before = N.load(Ordering::Relaxed);
    let mut events = 0;
    for i in 0..600 {
        a.advance(&set, 1.0 / 60.0);
        events += a.events().len();
        if i == 300 {
            // Crossfades (including an interrupting one) reuse buffers too.
            a.crossfade(&set, w, 0.2);
            a.crossfade(&set, w, 0.2);
        }
        a.evaluate(&set);
        a.skin(&set, &mut m, &mut k);
    }
    let allocs = N.load(Ordering::Relaxed) - before;
    assert!(events > 50, "{events} events fired");
    assert_eq!(allocs, 0, "{allocs} allocations in 600 ticks");
}

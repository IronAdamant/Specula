//! What a model build may spend, and how to stop one early.
//!
//! **Work units** are estimated nanoseconds of one core of the reference machine (the Intel
//! iGPU laptop Specula is measured on, release build). Each tier counts its own operations and
//! weighs them by what they cost there: a CSG classification, an SDF primitive or noise octave,
//! a terrain sample visit, a scatter candidate and the polygon edges it is tested against, a
//! texture pixel operation. Units are counted, never timed, so a build's result (and where it
//! runs out of budget) is the same on every machine; the weights only make the caps mean about
//! the same wall time everywhere a build can run out. A whole build gets [`MAX_BUILD`].
//!
//! **Cancelling:** the engine gives each build a flag ([`cancel_on`]); every tier's budget check
//! also looks at it, so a running build nobody wants any more stops within a few milliseconds.
//! Cancelling only discards work: it never changes what a finished build makes.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Most work units one model build may spend (≈ 1.5 s of one reference core), over all its
/// booleans, sculpts, generators and textures.
pub const MAX_BUILD: u64 = 1_500_000_000;

/// One CSG classification (a polygon against a plane: the bounding-sphere test, or a vertex's
/// side), with the splitting and bookkeeping around it.
pub const CSG_CLASSIFY: u64 = 200;
/// Extra for each orientation the float filter can't decide (the exact 256-bit predicate).
pub const CSG_EXACT: u64 = 100;
/// One terrain sample visit (a thermal cell); a noise octave is five, a droplet step fifteen.
pub const TERRAIN_VISIT: u64 = 10;
/// A scatter candidate (random point, Poisson neighbourhood), each polygon edge it is tested
/// against, each edge-cell visit building the polygon's grid, and each Poisson grid cell.
pub const SCATTER_CANDIDATE: u64 = 280;
pub const SCATTER_EDGE: u64 = 10;
pub const SCATTER_GRID_EDGE: u64 = 60;
pub const SCATTER_CELL: u64 = 10;
/// A texture pixel operation on one core (a bake may use up to four, but low-end devices have
/// one or two: the cap holds there).
pub const TEXTURE_OP: u64 = 20;
/// Tier-1 stages, per vertex: a displace (smooth normals plus noise; each octave more), weld,
/// cavity shading, noise tints and bands, transforms and plain colour ops; per output triangle:
/// subdivision and copies (array, radial, mirror); per vertex of the assembled mesh; per input
/// triangle of each LOD simplification (24 clustering passes); per point of a convex hull.
pub const TIER1_DISPLACE: u64 = 750;
pub const TIER1_OCTAVE: u64 = 50;
pub const TIER1_WELD: u64 = 650;
pub const TIER1_CAVITY: u64 = 3200;
pub const TIER1_TINT: u64 = 220;
pub const TIER1_POINT: u64 = 30;
pub const TIER1_SUBDIVIDE: u64 = 150;
pub const TIER1_COPY: u64 = 80;
pub const ASSEMBLE_VERTEX: u64 = 2500;
pub const SIMPLIFY_TRIANGLE: u64 = 3000;
pub const HULL_POINT: u64 = 600;

thread_local! {
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// While alive, builds on this thread stop (with a `Limit` error saying so) once `flag` is set.
pub struct CancelGuard {
    previous: Option<Arc<AtomicBool>>,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        let prev = self.previous.take();
        CANCEL.with(|c| *c.borrow_mut() = prev);
    }
}

/// Makes `flag` this thread's cancel flag until the guard is dropped.
pub fn cancel_on(flag: Arc<AtomicBool>) -> CancelGuard {
    let previous = CANCEL.with(|c| c.borrow_mut().replace(flag));
    CancelGuard { previous }
}

/// The current thread's flag (to hand to worker threads a build spawns).
pub fn current_flag() -> Option<Arc<AtomicBool>> {
    CANCEL.with(|c| c.borrow().clone())
}

/// Whether this thread's build was cancelled.
pub fn cancelled() -> bool {
    CANCEL.with(|c| c.borrow().as_ref().is_some_and(|f| f.load(Ordering::Relaxed)))
}

/// The message every tier gives when a build was cancelled.
pub const CANCELLED: &str = "the build was cancelled (nobody needs it any more)";

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
    use super::*;

    #[test]
    fn the_flag_is_per_thread_and_restored() {
        assert!(!cancelled());
        let f = Arc::new(AtomicBool::new(false));
        {
            let _g = cancel_on(Arc::clone(&f));
            assert!(!cancelled());
            f.store(true, Ordering::Relaxed);
            assert!(cancelled());
            // Another thread has no flag.
            assert!(!std::thread::spawn(cancelled).join().unwrap());
        }
        assert!(!cancelled());
    }
}

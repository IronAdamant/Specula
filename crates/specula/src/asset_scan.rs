//! The model library's disk scan, off the engine thread (M5 render follow-ups, the M4 exit run's
//! recipe-write stalls).
//!
//! **Why.** Every loop pass asked the library whether its recipe folders, styles, anim sets and
//! glTF files had changed (once a second), and every `project_write` of an asset forced the same
//! check at once: a `stat` of every cached file, and reads of what changed, on the engine thread
//! under the library's lock. With the M4 exit proof's 33 recipes being written that was 4-22 ms
//! per write (`engine.stall` in `worker_then`, growing with the folder), past the 4 ms edit budget.
//!
//! **Now.** A poll is three steps ([`crate::models::Library::scan_plan`], `ScanPlan::run`,
//! `Library::apply_scan`): the plan is copied from the caches under the lock (no disk), the scan
//! runs on a worker without the lock, and what it found is applied on the engine thread (only
//! what changed: forgotten recipes get their keys worked out again from cached texts). One scan
//! is in flight at a time; a forced one asked for meanwhile runs right after.
//!
//! **Project writes** don't wait for a scan: the store already holds the new texts, so recipes and
//! styles go into the library's caches at once, deleted ones leave them
//! ([`crate::models::Library::note_written`]), and the affected handles are re-keyed in memory;
//! tools that read recipes right after the write answers (`style_check`, `studio_check`) see the
//! new text. A scan planned before a note (or before a newer applied scan) is dropped: it read an
//! older disk. A forced scan follows on the worker for what the store doesn't describe (anim
//! sets, glTF and PNG files, folders not cached yet): those reach models a moment after the
//! write answers.

use crate::models;
use crate::Engine;
use std::sync::Arc;

/// Whether a scan runs on a worker, and whether another (forced) one was asked for meanwhile.
#[derive(Default)]
pub(crate) struct ScanState {
    in_flight: bool,
    again: bool,
}

impl Engine {
    /// The model library's poll for the loop: due once a second (`force`: now), its disk scan on
    /// a worker; what changed is applied when it lands. Sprite reloads and the library's garbage
    /// collection run every pass as before, and at once when forced (a project write: a sprite
    /// or tileset PNG it wrote is reloaded before its answer says so).
    pub(crate) fn poll_assets_async(&mut self, force: bool) {
        // A sliced re-key goes on first (models::REKEY_SLICE a pass).
        let more = self.library.lock().rekey_more();
        if more.is_some() {
            self.assets_reloaded(more);
        }
        self.assets_upkeep(force);
        if self.assets_scan.in_flight {
            self.assets_scan.again |= force;
            return;
        }
        let plan = {
            let mut lib = self.library.lock();
            if !force && !lib.poll_due() {
                return;
            }
            lib.scan_plan()
        };
        self.assets_scan.in_flight = true;
        let job = crate::workers::Offload::engine(crate::workers::JobKind::AssetScan).class(crate::schedule::Class::Disk);
        let then = move |e: &mut Engine, r: Result<models::Scanned, crate::workers::Failed>| {
            e.assets_scan.in_flight = false;
            match r {
                Ok(scanned) => {
                    let reload = {
                        let _z = specula_core::profile::zone("models.apply_scan");
                        e.library.lock().apply_scan(scanned, force)
                    };
                    e.assets_reloaded(reload);
                }
                Err(f) => specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "models.scan_failed", error = format!("{f:?}")),
            }
            if std::mem::take(&mut e.assets_scan.again) {
                e.poll_assets_async(true);
            }
        };
        if self.offload(job, move |_| plan.run(), then).is_err() {
            self.assets_scan.in_flight = false;
        }
    }

    /// Asset files the project store just wrote (paths under `assets/`, as the store names them):
    /// their texts reach the library's cached folders now and the affected models are re-keyed
    /// in memory; a forced scan on a worker follows for everything else. Returns whether the
    /// library was told anything (the write's answer says `models` were reloaded).
    pub(crate) fn assets_written(&mut self, changed: &[String]) -> bool {
        // Deleted files are noted too (None): they leave the cache now.
        let texts: Vec<(String, Option<Arc<str>>)> = changed
            .iter()
            .filter_map(|p| {
                let rel = p.strip_prefix("assets/")?;
                Some((rel.to_owned(), self.world.files().get(p).map(|f| Arc::clone(&f.text))))
            })
            .collect();
        let reload = {
            let _z = specula_core::profile::zone("models.note_written");
            let mut lib = self.library.lock();
            if lib.note_written(&texts) {
                lib.rekey(true)
            } else {
                None
            }
        };
        {
            let _z = specula_core::profile::zone("models.reloaded");
            self.assets_reloaded(reload);
        }
        self.poll_assets_async(true);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    /// Review R1 (minor 3): a forced poll (a project write) forces the upkeep too, as before the
    /// scan moved to a worker: sprite and tileset PNGs are checked and the library's garbage is
    /// collected at once, also while a scan is in flight; an unforced pass leaves both to their
    /// timers.
    #[test]
    fn a_forced_poll_forces_the_upkeep() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let long_ago = Instant::now().checked_sub(Duration::from_millis(50)).unwrap_or_else(Instant::now);
        e.last_gc = (long_ago, e.world.mesh_epoch());
        e.poll_assets_async(false);
        assert_eq!(e.last_gc.0, long_ago, "an unforced pass with nothing changed collects nothing");
        e.poll_assets_async(true);
        assert!(e.last_gc.0 > long_ago, "a forced pass collects at once");
        // Again while that scan is still in flight: still forced.
        e.last_gc = (long_ago, e.world.mesh_epoch());
        e.poll_assets_async(true);
        assert!(e.last_gc.0 > long_ago, "forced while a scan is in flight");
    }
}

//! The null backend: mixes in real time on its own thread and throws the result away. Used when
//! there is no audio device, for headless servers, and on platforms without a backend yet (F6).

use super::{set_state, Clock, ThreadBackend};
use crate::error::{AudioError, ErrorCode, Result};
use crate::mixer::{BackendState, Mixer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Block length of the null clock.
pub const BLOCK_MS: u32 = 10;

pub(crate) fn start(mut mixer: Mixer) -> Result<ThreadBackend> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let stats = mixer.stats().clone();
    set_state(&stats, BackendState::Null);
    stats.device_channels.store(2, Ordering::Relaxed);
    let rate = mixer.rate();
    stats.buffer_frames.store(rate * BLOCK_MS / 1000, Ordering::Relaxed);
    stats.latency_us.store(BLOCK_MS * 1000, Ordering::Relaxed);
    let join = std::thread::Builder::new()
        .name("specula-audio-null".into())
        .spawn(move || {
            let mut clock = Clock::new(rate, BLOCK_MS);
            while !stop2.load(Ordering::Relaxed) {
                clock.tick(&mut mixer, &stats, &stop2);
            }
            set_state(&stats, BackendState::Stopped);
        })
        .map_err(|e| AudioError::new(ErrorCode::ThreadGone, format!("cannot spawn the audio thread: {e}")))?;
    Ok(ThreadBackend { stop, join: Some(join) })
}

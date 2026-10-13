//! GPU pass timings from timestamp queries: one query pool per frame slot, read only after the
//! slot's fence signalled (never stalls).

use crate::device::Device;
use crate::vk::*;
use crate::{GpuError, GpuTiming, PassTiming};
use std::ffi::c_void;
use std::ptr;

/// Passes a frame can time.
pub const MAX_PASSES: usize = 8;
const QUERIES: u32 = 2 * MAX_PASSES as u32;

/// A query pool for one frame slot: a begin and end timestamp per pass.
pub struct PassTimer {
    pool: VkQueryPool,
    /// Passes written by the last recorded frame, and that frame's number.
    passes: Vec<&'static str>,
    frame: u64,
    /// Queries were reset and written by a submitted frame whose results are not read yet.
    pending: bool,
}

impl PassTimer {
    /// `Ok(None)` when the queue has no timestamp support.
    pub fn new(dev: &Device) -> Result<Option<PassTimer>, GpuError> {
        if dev.timestamp_valid_bits == 0 || dev.timestamp_period <= 0.0 {
            return Ok(None);
        }
        let info = VkQueryPoolCreateInfo {
            s_type: ST_QUERY_POOL_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            query_type: VK_QUERY_TYPE_TIMESTAMP,
            query_count: QUERIES,
            pipeline_statistics: 0,
        };
        let mut pool = VK_NULL_HANDLE;
        // SAFETY: valid device and create info; `pool` is a valid out-pointer.
        let r = unsafe { (dev.fns.create_query_pool)(dev.handle, &info, ptr::null(), &mut pool) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateQueryPool", r));
        }
        Ok(Some(PassTimer { pool, passes: Vec::new(), frame: 0, pending: false }))
    }

    /// Start recording `frame`: reset every query. Call outside a rendering instance, before any
    /// pass. Unread results of an earlier frame are dropped.
    pub fn begin_frame(&mut self, dev: &Device, cmd: VkCommandBuffer, frame: u64) {
        // SAFETY: `cmd` is recording outside a render pass; the pool has QUERIES queries.
        unsafe { (dev.fns.cmd_reset_query_pool)(cmd, self.pool, 0, QUERIES) };
        self.passes.clear();
        self.frame = frame;
        self.pending = false;
    }

    /// Timestamp the start of pass `name`; `None` once [`MAX_PASSES`] are in use (not timed).
    pub fn begin_pass(&mut self, dev: &Device, cmd: VkCommandBuffer, name: &'static str) -> Option<usize> {
        if self.passes.len() >= MAX_PASSES {
            return None;
        }
        let i = self.passes.len();
        self.passes.push(name);
        // SAFETY: `cmd` is recording; query 2i was reset in `begin_frame` and is written once.
        unsafe { (dev.fns.cmd_write_timestamp2)(cmd, VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, self.pool, 2 * i as u32) };
        Some(i)
    }

    /// Timestamp the end of the pass `begin_pass` returned.
    pub fn end_pass(&mut self, dev: &Device, cmd: VkCommandBuffer, pass: Option<usize>) {
        if let Some(i) = pass {
            // SAFETY: as in `begin_pass`, for query 2i + 1.
            unsafe { (dev.fns.cmd_write_timestamp2)(cmd, VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, self.pool, 2 * i as u32 + 1) };
        }
    }

    /// The frame's command buffer was submitted: its results become readable once it completes.
    pub fn submitted(&mut self) {
        self.pending = !self.passes.is_empty();
    }

    /// Results of the last submitted frame, if it has completed (the caller waited for its
    /// fence). Never waits; each frame's results are returned once.
    pub fn read(&mut self, dev: &Device) -> Option<GpuTiming> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        let n = 2 * self.passes.len();
        // (value, availability) pairs.
        let mut raw = vec![[0u64; 2]; n];
        // SAFETY: the queries were reset and written by a submitted command buffer; `raw` holds
        // `n` 16-byte (value, availability) results.
        let r = unsafe {
            (dev.fns.get_query_pool_results)(
                dev.handle,
                self.pool,
                0,
                n as u32,
                n * 16,
                raw.as_mut_ptr() as *mut c_void,
                16,
                VK_QUERY_RESULT_64_BIT | VK_QUERY_RESULT_WITH_AVAILABILITY_BIT,
            )
        };
        if r != VK_SUCCESS && r != VK_NOT_READY {
            return None;
        }
        timing_from_raw(self.frame, &self.passes, &raw, dev.timestamp_valid_bits, dev.timestamp_period)
    }

    pub fn destroy(&mut self, dev: &Device) {
        // SAFETY: the pool is idle (callers wait for the device) and destroyed once.
        unsafe { (dev.fns.destroy_query_pool)(dev.handle, self.pool, ptr::null()) };
        self.pool = VK_NULL_HANDLE;
    }
}

/// Turn (value, availability) pairs — begin and end per pass — into milliseconds. `None` if any
/// result is unavailable. Timestamps wrap at `valid_bits`.
pub fn timing_from_raw(frame: u64, passes: &[&'static str], raw: &[[u64; 2]], valid_bits: u32, period_ns: f32) -> Option<GpuTiming> {
    if passes.is_empty() || raw.len() < 2 * passes.len() || raw.iter().any(|q| q[1] == 0) {
        return None;
    }
    let mask = if valid_bits >= 64 { u64::MAX } else { (1u64 << valid_bits) - 1 };
    let ms = |a: u64, b: u64| ((b & mask).wrapping_sub(a & mask) & mask) as f64 * period_ns as f64 / 1e6;
    let timings = passes
        .iter()
        .enumerate()
        .map(|(i, &name)| PassTiming { name, gpu_ms: ms(raw[2 * i][0], raw[2 * i + 1][0]) })
        .collect();
    let total_ms = ms(raw[0][0], raw[2 * passes.len() - 1][0]);
    Some(GpuTiming { frame, total_ms, passes: timings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_converts_ticks_and_wraps() {
        let raw = [[1_000, 1], [3_000, 1], [3_500, 1], [9_000, 1]];
        let t = timing_from_raw(7, &["scene", "post"], &raw, 64, 83.333).unwrap();
        assert_eq!(t.frame, 7);
        assert!((t.passes[0].gpu_ms - 2_000.0 * 83.333e-6).abs() < 1e-6);
        assert_eq!(t.passes[1].name, "post");
        assert!((t.total_ms - 8_000.0 * 83.333e-6).abs() < 1e-6);
        // A 36-bit counter wrapping between begin and end.
        let wrap = [[(1u64 << 36) - 10, 1], [5, 1]];
        let t = timing_from_raw(1, &["scene"], &wrap, 36, 1.0).unwrap();
        assert!((t.passes[0].gpu_ms - 15e-6).abs() < 1e-12);
        // Garbage above the valid bits is ignored.
        let high = [[(0xffu64 << 40) | 100, 1], [(0x12u64 << 40) | 300, 1]];
        assert!((timing_from_raw(1, &["scene"], &high, 40, 1.0).unwrap().total_ms - 200e-6).abs() < 1e-12);
    }

    #[test]
    fn unavailable_results_give_none() {
        assert_eq!(timing_from_raw(1, &["scene"], &[[1, 1], [2, 0]], 64, 1.0), None);
        assert_eq!(timing_from_raw(1, &[], &[], 64, 1.0), None);
        assert_eq!(timing_from_raw(1, &["scene"], &[[1, 1]], 64, 1.0), None);
    }
}

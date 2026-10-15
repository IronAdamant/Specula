//! GPU pass timings from timestamp queries: one query pool per frame slot, read only after the
//! slot's fence signalled (never stalls).

use crate::device::Device;
use crate::vk::*;
use crate::{GpuError, GpuTiming, PassTiming};
use std::ffi::c_void;
use std::ptr;

/// Timed segments a frame can record (a pass interrupted by another, like the scene by its 2D
/// items, takes one per stretch; their times add up under the pass's name).
pub const MAX_PASSES: usize = 16;
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

    /// Write the results of the last submitted frame into `out` (reusing its storage), if it has
    /// completed (the caller waited for its fence). Never waits; each frame is read once.
    /// `Ok(true)`: `out` holds new timings. `Ok(false)`: nothing new (`out` untouched).
    /// `Err(VkResult)`: `vkGetQueryPoolResults` failed (e.g. `VK_ERROR_DEVICE_LOST`).
    pub fn read_into(&mut self, dev: &Device, out: &mut Option<GpuTiming>) -> Result<bool, VkResult> {
        if !self.pending {
            return Ok(false);
        }
        self.pending = false;
        let n = 2 * self.passes.len();
        // (value, availability) pairs.
        let mut raw = [[0u64; 2]; QUERIES as usize];
        // SAFETY: the queries were reset and written by a submitted command buffer; `raw` holds
        // at least `n` 16-byte (value, availability) results.
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
            return Err(r);
        }
        Ok(fill_timing(out, self.frame, &self.passes, &raw[..n], dev.timestamp_valid_bits, dev.timestamp_period))
    }

    pub fn destroy(&mut self, dev: &Device) {
        // SAFETY: the pool is idle (callers wait for the device) and destroyed once.
        unsafe { (dev.fns.destroy_query_pool)(dev.handle, self.pool, ptr::null()) };
        self.pool = VK_NULL_HANDLE;
    }
}

/// Turn (value, availability) pairs — begin and end per pass — into milliseconds in `out`,
/// reusing its pass list. Segments with the same name add up into one entry (in first-seen
/// order), so names stay unique. False (and `out` untouched) if any result is unavailable.
/// Timestamps wrap at `valid_bits`.
pub fn fill_timing(
    out: &mut Option<GpuTiming>,
    frame: u64,
    passes: &[&'static str],
    raw: &[[u64; 2]],
    valid_bits: u32,
    period_ns: f32,
) -> bool {
    if passes.is_empty() || raw.len() < 2 * passes.len() || raw.iter().any(|q| q[1] == 0) {
        return false;
    }
    let mask = if valid_bits >= 64 { u64::MAX } else { (1u64 << valid_bits) - 1 };
    let ms = |a: u64, b: u64| ((b & mask).wrapping_sub(a & mask) & mask) as f64 * period_ns as f64 / 1e6;
    let t = out.get_or_insert_with(|| GpuTiming { frame, total_ms: 0.0, passes: Vec::with_capacity(MAX_PASSES) });
    t.frame = frame;
    t.total_ms = ms(raw[0][0], raw[2 * passes.len() - 1][0]);
    t.passes.clear();
    for (i, &name) in passes.iter().enumerate() {
        let gpu_ms = ms(raw[2 * i][0], raw[2 * i + 1][0]);
        match t.passes.iter_mut().find(|p| p.name == name) {
            Some(p) => p.gpu_ms += gpu_ms,
            None => t.passes.push(PassTiming { name, gpu_ms }),
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing_from_raw(frame: u64, passes: &[&'static str], raw: &[[u64; 2]], valid_bits: u32, period_ns: f32) -> Option<GpuTiming> {
        let mut out = None;
        fill_timing(&mut out, frame, passes, raw, valid_bits, period_ns).then(|| out.expect("filled"))
    }

    #[test]
    fn filling_reuses_storage_and_leaves_old_timings_when_unavailable() {
        let mut out = None;
        assert!(fill_timing(&mut out, 1, &["scene", "post"], &[[0, 1], [10, 1], [10, 1], [30, 1]], 64, 1e6));
        let ptr = out.as_ref().unwrap().passes.as_ptr();
        assert!(fill_timing(&mut out, 2, &["scene"], &[[0, 1], [5, 1]], 64, 1e6));
        let t = out.as_ref().unwrap();
        assert_eq!((t.frame, t.passes.len(), t.passes.as_ptr()), (2, 1, ptr), "same allocation");
        assert!(!fill_timing(&mut out, 3, &["scene"], &[[0, 1], [5, 0]], 64, 1e6));
        assert_eq!(out.unwrap().frame, 2, "unavailable results keep the last timings");
    }

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
    fn segments_of_one_pass_add_up() {
        let raw = [[0, 1], [10, 1], [10, 1], [14, 1], [14, 1], [20, 1]];
        let t = timing_from_raw(1, &["scene", "sprites", "scene"], &raw, 64, 1e6).unwrap();
        let got: Vec<(&str, f64)> = t.passes.iter().map(|p| (p.name, p.gpu_ms)).collect();
        assert_eq!(got, [("scene", 16.0), ("sprites", 4.0)]);
        assert_eq!(t.total_ms, 20.0);
    }

    #[test]
    fn unavailable_results_give_none() {
        assert_eq!(timing_from_raw(1, &["scene"], &[[1, 1], [2, 0]], 64, 1.0), None);
        assert_eq!(timing_from_raw(1, &[], &[], 64, 1.0), None);
        assert_eq!(timing_from_raw(1, &["scene"], &[[1, 1]], 64, 1.0), None);
    }
}

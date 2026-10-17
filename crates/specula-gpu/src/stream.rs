//! Queued uploads and deferred frees: the streaming path (`docs/design/STREAMING.md`).
//!
//! [`crate::Gpu::upload_mesh_with`] and friends copy through a staging buffer and **wait** for the
//! copy (`run_sync`): fine at load time, a hitch when a world streams in during play (the wait
//! also drains every frame already queued). The queued path never waits:
//!
//! - `queue_mesh` / `queue_texture_levels` create the device-local resource, write the data into
//!   a **staging arena** (host-visible, persistently mapped, one per batch slot) and record the
//!   copy into the current **batch**: a command buffer of copies, each followed by a barrier that
//!   makes it visible to later vertex, index and fragment reads. The resource's id is valid at
//!   once.
//! - The batch is **submitted before the next frame** (`flush`, called by `present` and every
//!   offscreen render) on the same queue, so that frame already draws with the new data: a
//!   barrier's second scope reaches every command later in submission order.
//! - **Retiring** a mesh or texture frees it once no submitted frame can still read it, without a
//!   wait: it is freed when the first batch submitted after the retirement has finished (a fence
//!   signal covers everything submitted earlier on the queue, so every frame that could have used
//!   it is done by then). An empty batch is submitted for that when nothing else is queued.
//! - Batches cycle through [`MAX_SLOTS`] slots (command buffer, fence, arena). A slot is reused
//!   when its fence has signalled (polled, never waited) — only when every slot is still in
//!   flight does queuing wait, for the oldest (the GPU is that far behind).
//! - **Mesh blocks.** Queued meshes are carved out of device-local blocks of [`BLOCK_BYTES`]
//!   (first fit, freed ranges coalesce) instead of one `vkAllocateMemory` each: on the Intel iGPU
//!   an allocation costs about 0.3 ms and a free as much, which would cap streaming at a few
//!   meshes a frame (and `maxMemoryAllocationCount` is 4096 on some drivers). A block is freed
//!   once it is empty and another empty one is kept.
//!
//! Engine code bounds what it queues per frame (bytes and time, perf.toml `[streaming]`), so a
//! batch is a few MB at most; an item larger than the arena gets its own staging buffer.

use crate::device::{buffer_barrier, image_barrier, Buffer, Device, Image};
use crate::vk::*;
use crate::{GpuError, MemoryKind};
use std::collections::VecDeque;
use std::ptr;
use std::time::{Duration, Instant};

/// Batches that may be recorded or in flight at once.
pub const MAX_SLOTS: usize = 3;
/// Staging bytes each slot keeps mapped (items that don't fit get a buffer of their own).
pub const ARENA_BYTES: u64 = 8 << 20;
/// Time one poll spends freeing retired resources at most (freeing memory can take a driver a
/// while; what's left goes at the next poll). At least one is freed per poll.
pub const FREE_BUDGET: Duration = Duration::from_micros(250);

/// What the queued path did, for statistics (perf_frame `streaming.gpu`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadStats {
    /// Batches submitted so far (empty retire-only batches included).
    pub batches: u64,
    /// Meshes and textures queued so far.
    pub items: u64,
    /// Bytes copied through staging so far.
    pub bytes: u64,
    /// Batches recorded or in flight now.
    pub in_flight: u32,
    /// Times queuing had to wait for a slot (every slot in flight).
    pub slot_waits: u64,
    /// Retired resources not yet freed (a frame or batch in flight may still read them).
    pub retired_pending: u32,
    /// Resources freed after retirement so far.
    pub freed: u64,
    /// Items that did not fit a slot's arena and got their own staging buffer.
    pub dedicated_staging: u64,
    /// Device-local mesh blocks, the bytes they hold and the bytes meshes use in them.
    pub mesh_blocks: u32,
    pub mesh_block_bytes: u64,
    pub mesh_block_used: u64,
}

/// A resource to free once the GPU is past it.
pub(crate) enum Retired {
    Buffer(Buffer),
    /// An image and its bindless slot.
    Image(Image, u32),
    /// A mesh's range of a block: `(block, offset, bytes)`.
    Range(u32, u64, u64),
}

/// Device-local block size queued meshes are carved from (a bigger mesh gets a block its size).
pub const BLOCK_BYTES: u64 = 16 << 20;
/// Mesh ranges start on this alignment (index and vertex offsets stay aligned).
const RANGE_ALIGN: u64 = 256;

struct Block {
    buffer: Buffer,
    /// Free ranges `(offset, bytes)`, sorted by offset, never adjacent.
    free: Vec<(u64, u64)>,
    used: u64,
}

impl Block {
    fn take(&mut self, bytes: u64) -> Option<u64> {
        let i = self.free.iter().position(|&(_, n)| n >= bytes)?;
        let (at, n) = self.free[i];
        if n == bytes {
            self.free.remove(i);
        } else {
            self.free[i] = (at + bytes, n - bytes);
        }
        self.used += bytes;
        Some(at)
    }

    fn give(&mut self, at: u64, bytes: u64) {
        let i = self.free.partition_point(|&(o, _)| o < at);
        self.free.insert(i, (at, bytes));
        // Coalesce with the next, then the previous.
        if i + 1 < self.free.len() && self.free[i].0 + self.free[i].1 == self.free[i + 1].0 {
            self.free[i].1 += self.free[i + 1].1;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].0 + self.free[i - 1].1 == self.free[i].0 {
            self.free[i - 1].1 += self.free[i].1;
            self.free.remove(i);
        }
        self.used = self.used.saturating_sub(bytes);
    }
}

/// The mesh blocks (see the module docs).
#[derive(Default)]
pub(crate) struct Blocks {
    blocks: Vec<Option<Block>>,
}

impl Blocks {
    /// A range of `bytes` for a mesh: `(block, offset, its buffer, the range's size)`.
    pub fn alloc(&mut self, dev: &Device, bytes: u64) -> Result<(u32, u64, VkBuffer, u64), GpuError> {
        let size = bytes.div_ceil(RANGE_ALIGN) * RANGE_ALIGN;
        for (i, b) in self.blocks.iter_mut().enumerate() {
            if let Some(b) = b {
                if let Some(at) = b.take(size) {
                    return Ok((i as u32, at, b.buffer.handle, size));
                }
            }
        }
        let block_bytes = size.max(BLOCK_BYTES);
        let buffer = crate::gpu::create_mesh_buffer(dev, block_bytes)?;
        let mut block = Block { buffer, free: vec![(0, block_bytes)], used: 0 };
        let at = block.take(size).expect("a fresh block holds it");
        let handle = block.buffer.handle;
        let i = match self.blocks.iter().position(Option::is_none) {
            Some(i) => {
                self.blocks[i] = Some(block);
                i
            }
            None => {
                self.blocks.push(Some(block));
                self.blocks.len() - 1
            }
        };
        Ok((i as u32, at, handle, size))
    }

    /// Gives a range back (no frame or batch may still use it); an empty block is freed when
    /// another empty one exists.
    pub fn free(&mut self, dev: &Device, block: u32, at: u64, bytes: u64) {
        let Some(Some(b)) = self.blocks.get_mut(block as usize) else { return };
        b.give(at, bytes);
        if b.used == 0 {
            let empty = self.blocks.iter().flatten().filter(|b| b.used == 0).count();
            if empty > 1 {
                if let Some(b) = self.blocks[block as usize].take() {
                    dev.destroy_buffer(&b.buffer);
                }
            }
        }
    }

    /// `(blocks, bytes held, bytes used)`.
    pub fn usage(&self) -> (u32, u64, u64) {
        let live = self.blocks.iter().flatten();
        let (mut n, mut held, mut used) = (0, 0, 0);
        for b in live {
            n += 1;
            held += b.buffer.bytes;
            used += b.used;
        }
        (n, held, used)
    }

    fn destroy(&mut self, dev: &Device) {
        for b in self.blocks.drain(..).flatten() {
            dev.destroy_buffer(&b.buffer);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SlotState {
    Free,
    Recording,
    /// Submitted as batch `serial`.
    InFlight(u64),
}

struct Slot {
    cmd: VkCommandBuffer,
    fence: VkFence,
    arena: Option<Buffer>,
    used: u64,
    /// Dedicated staging buffers of oversized items (freed with the slot).
    extra: Vec<Buffer>,
    state: SlotState,
}

/// The queued-upload state of a GPU (see the module docs).
#[derive(Default)]
pub(crate) struct Uploads {
    slots: Vec<Slot>,
    recording: Option<usize>,
    /// Batch serials handed out (the last submitted) and known finished.
    submitted: u64,
    completed: u64,
    /// Retired since the last submission: freed after the next batch.
    retired_now: Vec<Retired>,
    /// `(serial that must finish, resource)`.
    retired: VecDeque<(u64, Retired)>,
    stats: UploadStats,
    /// Ids of the meshes and textures recorded into batches not yet known finished, with the
    /// serial (`u64::MAX`: still recording) — a synchronous destroy of one must wait for it.
    pending_ids: Vec<(u32, u64)>,
    /// Bindless slots of freed images, for the caller to give back (`take_freed_slots`).
    freed_slots: Vec<u32>,
    /// Where queued meshes live.
    pub blocks: Blocks,
}

/// Where staged bytes go: the buffer, the offset in it, and a pointer to write them at.
pub(crate) struct Staged {
    pub buffer: VkBuffer,
    pub offset: u64,
    pub dst: *mut u8,
}

impl Uploads {
    pub fn stats(&self) -> UploadStats {
        let mut s = self.stats;
        s.in_flight = self.slots.iter().filter(|s| s.state != SlotState::Free).count() as u32;
        s.retired_pending = (self.retired.len() + self.retired_now.len()) as u32;
        (s.mesh_blocks, s.mesh_block_bytes, s.mesh_block_used) = self.blocks.usage();
        s
    }

    /// Whether `id` (a mesh or texture) was queued in a batch that may not have finished.
    pub fn is_pending(&self, id: u32) -> bool {
        self.pending_ids.iter().any(|&(i, _)| i == id)
    }

    /// The command buffer of the batch being recorded (one is started when none is).
    pub fn recording_cmd(&mut self, dev: &Device, pool: VkCommandPool) -> Result<VkCommandBuffer, GpuError> {
        if let Some(i) = self.recording {
            return Ok(self.slots[i].cmd);
        }
        {
            let _z = specula_core::profile::zone("gpu.poll");
            self.poll(dev);
        }
        let i = match self.slots.iter().position(|s| s.state == SlotState::Free) {
            Some(i) => i,
            None if self.slots.len() < MAX_SLOTS => {
                let cmd = dev.allocate_command_buffer(pool)?;
                // A command buffer left behind by a failed fence is freed with the pool.
                let fence = dev.create_fence(false)?;
                self.slots.push(Slot { cmd, fence, arena: None, used: 0, extra: Vec::new(), state: SlotState::Free });
                self.slots.len() - 1
            }
            None => {
                // Every slot is in flight: wait for the oldest (the GPU is MAX_SLOTS batches behind).
                let oldest = (0..self.slots.len())
                    .min_by_key(|&i| match self.slots[i].state {
                        SlotState::InFlight(s) => s,
                        _ => u64::MAX,
                    })
                    .expect("slots exist");
                self.stats.slot_waits += 1;
                let _z = specula_core::profile::zone("gpu.slot_wait");
                if dev.wait_fence(self.slots[oldest].fence).is_err() {
                    dev.wait_idle();
                }
                self.poll(dev);
                self.slots.iter().position(|s| s.state == SlotState::Free).unwrap_or(oldest)
            }
        };
        let slot = &mut self.slots[i];
        // A slot taken over after a failed wait (device lost) may still hold its old state: free it.
        if let SlotState::InFlight(_) = slot.state {
            dev.wait_idle();
            Self::release_slot(dev, slot);
        }
        {
            let _z = specula_core::profile::zone("gpu.begin");
            dev.begin(slot.cmd)?;
        }
        slot.state = SlotState::Recording;
        self.recording = Some(i);
        Ok(slot.cmd)
    }

    /// Space for `bytes` (4-byte aligned) in the recording batch's arena, or a buffer of its own.
    pub fn stage(&mut self, dev: &Device, pool: VkCommandPool, bytes: u64) -> Result<Staged, GpuError> {
        self.recording_cmd(dev, pool)?;
        let i = self.recording.expect("recording");
        let slot = &mut self.slots[i];
        let aligned = bytes.div_ceil(16) * 16;
        if aligned <= ARENA_BYTES {
            if slot.arena.is_none() {
                let _z = specula_core::profile::zone("gpu.arena");
                let host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
                slot.arena = Some(dev.create_buffer(ARENA_BYTES, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, host, 0, MemoryKind::Staging, "upload arena")?);
                slot.used = 0;
            }
            if slot.used + aligned <= ARENA_BYTES {
                let arena = slot.arena.as_ref().expect("made above");
                let offset = slot.used;
                slot.used += aligned;
                // SAFETY: the arena is persistently mapped and `offset + bytes <= ARENA_BYTES`.
                let dst = unsafe { (arena.mapped as *mut u8).add(offset as usize) };
                return Ok(Staged { buffer: arena.handle, offset, dst });
            }
        }
        let host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
        let own = dev.create_buffer(bytes.max(4), VK_BUFFER_USAGE_TRANSFER_SRC_BIT, host, 0, MemoryKind::Staging, "upload staging")?;
        let staged = Staged { buffer: own.handle, offset: 0, dst: own.mapped as *mut u8 };
        slot.extra.push(own);
        self.stats.dedicated_staging += 1;
        Ok(staged)
    }

    /// Notes a queued item (`id`, `bytes` staged).
    pub fn queued(&mut self, id: u32, bytes: u64) {
        self.stats.items += 1;
        self.stats.bytes += bytes;
        self.pending_ids.push((id, u64::MAX));
    }

    /// Frees `r` once every frame and batch submitted so far has finished (see the module docs).
    pub fn retire(&mut self, r: Retired) {
        self.retired_now.push(r);
    }

    /// Submits the batch being recorded (with its closing barrier), or an empty one when
    /// resources were retired since the last submission. Frees what finished batches release.
    pub fn flush(&mut self, dev: &Device, pool: VkCommandPool) -> Result<(), GpuError> {
        self.poll(dev);
        if self.recording.is_none() && self.retired_now.is_empty() {
            return Ok(());
        }
        // An empty batch when only retirements are waiting (each copy carries its own barrier).
        self.recording_cmd(dev, pool)?;
        let i = self.recording.take().expect("recording");
        let slot = &mut self.slots[i];
        let ended = dev.end(slot.cmd).and_then(|()| dev.submit(slot.cmd, None, None, slot.fence));
        if let Err(e) = ended {
            // Nothing was queued: whatever the batch copies into is unusable, so wait for the GPU
            // and free the staging; the retired resources wait for the next batch.
            dev.wait_idle();
            Self::release_slot(dev, slot);
            slot.state = SlotState::Free;
            // A failed `end` leaves the command buffer in the invalid state; `begin` resets it.
            return Err(e);
        }
        self.submitted += 1;
        let serial = self.submitted;
        slot.state = SlotState::InFlight(serial);
        self.stats.batches += 1;
        for p in &mut self.pending_ids {
            if p.1 == u64::MAX {
                p.1 = serial;
            }
        }
        // Serials grow, so the queue stays in the order its batches finish.
        self.retired.extend(self.retired_now.drain(..).map(|r| (serial, r)));
        Ok(())
    }

    /// Takes finished batches (fences polled, never waited) and frees what they release.
    pub fn poll(&mut self, dev: &Device) {
        for slot in &mut self.slots {
            if let SlotState::InFlight(serial) = slot.state {
                if fence_signaled(dev, slot.fence) {
                    // Reset now, so a `Free` slot always holds an unsignalled fence.
                    let _ = dev.reset_fence(slot.fence);
                    Self::release_slot(dev, slot);
                    slot.state = SlotState::Free;
                    self.completed = self.completed.max(serial);
                }
            }
        }
        let done = self.completed;
        self.pending_ids.retain(|&(_, s)| s == u64::MAX || s > done);
        self.free_finished(dev, Some(FREE_BUDGET));
    }

    /// Frees retired resources whose batch finished, oldest first, within `budget` (`None`: all).
    fn free_finished(&mut self, dev: &Device, budget: Option<Duration>) {
        let start = Instant::now();
        while self.retired.front().is_some_and(|(after, _)| *after <= self.completed) {
            if budget.is_some_and(|b| start.elapsed() >= b) {
                break;
            }
            let (_, r) = self.retired.pop_front().expect("checked");
            match r {
                Retired::Range(block, at, bytes) => self.blocks.free(dev, block, at, bytes),
                r => free(dev, &r, &mut self.freed_slots),
            }
            self.stats.freed += 1;
        }
    }

    /// Submits and waits for everything queued, and frees every retired resource whose batch
    /// finished (freed images' slots: `take_freed_slots`).
    pub fn finish(&mut self, dev: &Device, pool: VkCommandPool) -> Result<(), GpuError> {
        let flushed = self.flush(dev, pool);
        for slot in &self.slots {
            if let SlotState::InFlight(_) = slot.state {
                if dev.wait_fence(slot.fence).is_err() {
                    dev.wait_idle();
                }
            }
        }
        self.poll(dev);
        self.free_finished(dev, None);
        flushed
    }

    /// Bindless slots of the images freed since the last call (the caller gives them back).
    pub fn take_freed_slots(&mut self) -> std::vec::Drain<'_, u32> {
        self.freed_slots.drain(..)
    }

    /// Destroys everything (the device must be idle). Command buffers go with the pool.
    pub fn destroy(&mut self, dev: &Device) {
        let mut slots = Vec::new();
        for r in self.retired.drain(..).map(|(_, r)| r).chain(self.retired_now.drain(..)) {
            free(dev, &r, &mut slots);
        }
        self.blocks.destroy(dev);
        for mut slot in self.slots.drain(..) {
            Self::release_slot(dev, &mut slot);
            if let Some(a) = slot.arena.take() {
                dev.destroy_buffer(&a);
            }
            // SAFETY: the device is idle; the fence belongs to it and is destroyed once.
            unsafe { (dev.fns.destroy_fence)(dev.handle, slot.fence, ptr::null()) };
        }
        self.recording = None;
        self.pending_ids.clear();
    }

    fn release_slot(dev: &Device, slot: &mut Slot) {
        for b in slot.extra.drain(..) {
            dev.destroy_buffer(&b);
        }
        slot.used = 0;
    }
}

fn free(dev: &Device, r: &Retired, freed_slots: &mut Vec<u32>) {
    match r {
        Retired::Buffer(b) => dev.destroy_buffer(b),
        Retired::Image(i, slot) => {
            dev.destroy_image(i);
            freed_slots.push(*slot);
        }
        // Blocks go with the pool.
        Retired::Range(..) => {}
    }
}

/// Whether `fence` has signalled, without waiting.
fn fence_signaled(dev: &Device, fence: VkFence) -> bool {
    // SAFETY: `fence` is a live fence of this device; a zero timeout only polls.
    let r = unsafe { (dev.fns.wait_for_fences)(dev.handle, 1, &fence, VK_TRUE, 0) };
    // VK_TIMEOUT: still running. Errors (device lost): treat as finished, nothing more will run.
    r != VK_TIMEOUT
}

/// Records the copy of a staged mesh (`total` bytes at `staged`) into `buffer` at `at`.
pub(crate) fn record_buffer_copy(dev: &Device, cmd: VkCommandBuffer, staged: &Staged, buffer: VkBuffer, at: u64, total: u64) {
    let region = VkBufferCopy { src_offset: staged.offset, dst_offset: at, size: total };
    // SAFETY: recording; the staging range and the destination buffer hold at least `total` bytes.
    unsafe { (dev.fns.cmd_copy_buffer)(cmd, staged.buffer, buffer, 1, &region) };
    // Every later vertex and index read (frames submitted after this batch) sees the copy.
    dev.barrier(
        cmd,
        &[buffer_barrier_range(
            buffer,
            at,
            total,
            VK_PIPELINE_STAGE_2_COPY_BIT,
            VK_ACCESS_2_TRANSFER_WRITE_BIT,
            VK_PIPELINE_STAGE_2_VERTEX_ATTRIBUTE_INPUT_BIT | VK_PIPELINE_STAGE_2_INDEX_INPUT_BIT,
            VK_ACCESS_2_VERTEX_ATTRIBUTE_READ_BIT | VK_ACCESS_2_INDEX_READ_BIT,
        )],
        &[],
    );
}

/// Records the copies of a staged texture's levels (`regions`, offsets relative to the staged
/// range) into `image`: UNDEFINED → TRANSFER_DST → copies → SHADER_READ_ONLY.
pub(crate) fn record_image_copy(dev: &Device, cmd: VkCommandBuffer, staged: &Staged, image: VkImage, regions: &mut [VkBufferImageCopy]) {
    dev.barrier(
        cmd,
        &[],
        &[image_barrier(
            image,
            VK_IMAGE_ASPECT_COLOR_BIT,
            VK_PIPELINE_STAGE_2_NONE,
            VK_ACCESS_2_NONE,
            VK_PIPELINE_STAGE_2_COPY_BIT,
            VK_ACCESS_2_TRANSFER_WRITE_BIT,
            VK_IMAGE_LAYOUT_UNDEFINED,
            VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        )],
    );
    for r in regions.iter_mut() {
        r.buffer_offset += staged.offset;
    }
    // SAFETY: recording; every region lies inside the staged range and its mip level; the image
    // is in TRANSFER_DST_OPTIMAL.
    unsafe {
        (dev.fns.cmd_copy_buffer_to_image)(cmd, staged.buffer, image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, regions.len() as u32, regions.as_ptr())
    };
    dev.barrier(
        cmd,
        &[],
        &[image_barrier(
            image,
            VK_IMAGE_ASPECT_COLOR_BIT,
            VK_PIPELINE_STAGE_2_COPY_BIT,
            VK_ACCESS_2_TRANSFER_WRITE_BIT,
            VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
            VK_ACCESS_2_SHADER_SAMPLED_READ_BIT,
            VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
            VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
        )],
    );
}

/// [`buffer_barrier`] for one range of a buffer.
fn buffer_barrier_range(buffer: VkBuffer, offset: u64, size: u64, src_stage: VkFlags64, src_access: VkFlags64, dst_stage: VkFlags64, dst_access: VkFlags64) -> VkBufferMemoryBarrier2 {
    VkBufferMemoryBarrier2 { offset, size, ..buffer_barrier(buffer, src_stage, src_access, dst_stage, dst_access) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(bytes: u64) -> Block {
        let buffer = Buffer { handle: VK_NULL_HANDLE, memory: VK_NULL_HANDLE, mapped: ptr::null_mut(), bytes, kind: MemoryKind::Meshes };
        Block { buffer, free: vec![(0, bytes)], used: 0 }
    }

    #[test]
    fn block_ranges_fit_first_and_coalesce() {
        let mut b = block(1024);
        let (a, c, d) = (b.take(256).unwrap(), b.take(512).unwrap(), b.take(256).unwrap());
        assert_eq!((a, c, d), (0, 256, 768));
        assert_eq!(b.take(1), None, "full");
        b.give(c, 512);
        assert_eq!(b.take(300), Some(256), "first fit reuses the hole");
        b.give(256, 300);
        b.give(a, 256);
        assert_eq!(b.free, vec![(0, 768)], "neighbours merge");
        b.give(d, 256);
        assert_eq!(b.free, vec![(0, 1024)]);
        assert_eq!(b.used, 0);
    }
}

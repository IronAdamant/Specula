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
//!
//! Engine code bounds what it queues per frame (bytes and time, perf.toml `[streaming]`), so a
//! batch is a few MB at most; an item larger than the arena gets its own staging buffer.

use crate::device::{buffer_barrier, image_barrier, Buffer, Device, Image};
use crate::vk::*;
use crate::{GpuError, MemoryKind};
use std::ptr;

/// Batches that may be recorded or in flight at once.
pub const MAX_SLOTS: usize = 3;
/// Staging bytes each slot keeps mapped (items that don't fit get a buffer of their own).
pub const ARENA_BYTES: u64 = 8 << 20;

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
}

/// A resource to free once the GPU is past it.
pub(crate) enum Retired {
    Buffer(Buffer),
    /// An image and its bindless slot.
    Image(Image, u32),
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
    retired: Vec<(u64, Retired)>,
    stats: UploadStats,
    /// Ids of the meshes and textures recorded into batches not yet known finished, with the
    /// serial (`u64::MAX`: still recording) — a synchronous destroy of one must wait for it.
    pending_ids: Vec<(u32, u64)>,
    /// Bindless slots of freed images, for the caller to give back (`take_freed_slots`).
    freed_slots: Vec<u32>,
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
        self.poll(dev);
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
        dev.begin(slot.cmd)?;
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
        let before = self.retired.len();
        let freed_slots = &mut self.freed_slots;
        self.retired.retain(|(after, r)| {
            if *after <= done {
                free(dev, r, freed_slots);
                false
            } else {
                true
            }
        });
        self.stats.freed += (before - self.retired.len()) as u64;
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
    }
}

/// Whether `fence` has signalled, without waiting.
fn fence_signaled(dev: &Device, fence: VkFence) -> bool {
    // SAFETY: `fence` is a live fence of this device; a zero timeout only polls.
    let r = unsafe { (dev.fns.wait_for_fences)(dev.handle, 1, &fence, VK_TRUE, 0) };
    // VK_TIMEOUT: still running. Errors (device lost): treat as finished, nothing more will run.
    r != VK_TIMEOUT
}

/// Records the copy of a staged mesh (`total` bytes at `staged`) into `buffer`.
pub(crate) fn record_buffer_copy(dev: &Device, cmd: VkCommandBuffer, staged: &Staged, buffer: VkBuffer, total: u64) {
    let region = VkBufferCopy { src_offset: staged.offset, dst_offset: 0, size: total };
    // SAFETY: recording; the staging range and the destination buffer hold at least `total` bytes.
    unsafe { (dev.fns.cmd_copy_buffer)(cmd, staged.buffer, buffer, 1, &region) };
    // Every later vertex and index read (frames submitted after this batch) sees the copy.
    dev.barrier(
        cmd,
        &[buffer_barrier(
            buffer,
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

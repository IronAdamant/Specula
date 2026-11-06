//! The resident Slug glyph store ([`crate::TextFrame`]): the words of the glyphs in-world text
//! draws (`specula_text::slug`), copied to a device-local buffer the glyph shader reads through its
//! address (`shaders/glyph.glsl`).
//!
//! The store on the CPU only ever grows (glyphs are appended, never moved) until it starts over
//! under a new id, so the upload is incremental: while a frame names the same store, only the
//! words past what is resident are copied, into the same buffer when they fit; a bigger buffer
//! (at least twice the size) takes the whole store otherwise, and the old one is retired (freed
//! once no submitted frame reads it). A store with a new id is uploaded from scratch.
//!
//! Words already uploaded are never written again, so frames in flight that read them are never
//! raced; the new words go where no submitted frame looks.

use crate::device::{buffer_barrier, Buffer, Device};
use crate::stream::{Retired, Uploads};
use crate::vk::*;
use crate::{GpuError, Limit, MemoryKind, TextFrame};
use std::ptr;

/// Most words of one store (64 MiB): far above any font's needs, a guard against a runaway caller.
pub const MAX_GLYPH_WORDS: usize = 16 << 20;
/// Smallest buffer made (words).
const MIN_WORDS: usize = 16 << 10;

/// The device copy of the frame's glyph store.
#[derive(Default)]
pub struct GlyphBuffer {
    store: u64,
    /// Words resident (a prefix of the store).
    uploaded: usize,
    capacity: usize,
    buffer: Option<Buffer>,
    address: u64,
    /// Words copied since start-up, and buffers made (a cache that re-sent everything every frame
    /// would show here).
    pub words_copied: u64,
    pub rebuilds: u64,
    /// The store (id, length) that couldn't be made resident, and why: its glyphs aren't drawn and
    /// it isn't retried until either changes.
    pub failure: Option<((u64, usize), String)>,
    /// Words a new buffer is made for at least (the CPU store's cap, when uploads come ahead of
    /// frames in pieces: one buffer, then appends).
    pub reserve: usize,
}

/// Checks `text` (sizes only; offsets are clamped by the shader).
pub fn check(text: &TextFrame) -> Result<(), GpuError> {
    if text.words.len() > MAX_GLYPH_WORDS {
        return Err(GpuError::limit(Limit::GlyphWords, text.words.len() as u64, MAX_GLYPH_WORDS as u64));
    }
    Ok(())
}

impl GlyphBuffer {
    /// The address the glyph shader reads (0: no store resident, glyphs aren't drawn).
    pub fn address(&self) -> u64 {
        if self.buffer.is_some() {
            self.address
        } else {
            0
        }
    }

    /// Words resident: the shader clamps every read below this.
    pub fn resident_words(&self) -> usize {
        self.uploaded
    }

    /// The store whose words are resident.
    pub fn store_id(&self) -> u64 {
        self.store
    }

    /// Whether the resident words are `text`'s whole store (what its glyph records may read).
    pub fn holds(&self, text: &TextFrame) -> bool {
        self.buffer.is_some() && self.store == text.store && self.uploaded >= text.words.len()
    }

    pub fn bytes(&self) -> u64 {
        self.buffer.as_ref().map_or(0, |b| b.bytes)
    }

    /// Whether `text` needs words copied before it draws.
    pub fn stale(&self, text: &TextFrame) -> bool {
        !text.words.is_empty() && (self.store != text.store || self.uploaded < text.words.len() || self.buffer.is_none()) && !self.failed(text)
    }

    fn failed(&self, text: &TextFrame) -> bool {
        self.failure.as_ref().is_some_and(|(k, _)| *k == (text.store, text.words.len()))
    }

    /// Makes `text`'s store resident. `run` records into a command buffer and waits for it (the
    /// copy is small: only new glyphs, which appear once). Errors leave the old state (and are
    /// remembered: [`GlyphBuffer::failure`]).
    pub fn sync(
        &mut self,
        dev: &Device,
        uploads: &mut Uploads,
        text: &TextFrame,
        run: impl FnOnce(&Device, &dyn Fn(&Device, VkCommandBuffer)) -> Result<(), GpuError>,
    ) -> Result<(), GpuError> {
        if !self.stale(text) {
            return Ok(());
        }
        let result = self.try_sync(dev, uploads, text, run);
        if let Err(e) = &result {
            self.failure = Some(((text.store, text.words.len()), e.to_string()));
        }
        result
    }

    fn try_sync(
        &mut self,
        dev: &Device,
        uploads: &mut Uploads,
        text: &TextFrame,
        run: impl FnOnce(&Device, &dyn Fn(&Device, VkCommandBuffer)) -> Result<(), GpuError>,
    ) -> Result<(), GpuError> {
        check(text)?;
        let same = self.store == text.store && self.buffer.is_some() && self.uploaded <= text.words.len();
        let from = if same { self.uploaded } else { 0 };
        let fits = same && text.words.len() <= self.capacity;
        let new_words = &text.words[from..];
        // A new buffer takes the whole store; an append copies only the new words.
        let (copy_from, copy) = if fits { (from, new_words) } else { (0, text.words) };
        // Growing the same store: at least double (appends stay cheap). A new store: sized from
        // its own words (half again for growth), never from the old buffer, so stores that
        // start over don't ratchet the buffer up (review of 3eefaa2).
        let wanted = if same { text.words.len().max(self.capacity.saturating_mul(2)) } else { text.words.len().saturating_add(text.words.len() / 2) };
        let wanted = wanted.max(self.reserve);
        let capacity = if fits { self.capacity } else { wanted.clamp(MIN_WORDS, MAX_GLYPH_WORDS).max(text.words.len()) };
        let bytes = (copy.len() * 4) as u64;
        let staging = dev.create_buffer(bytes.max(4), VK_BUFFER_USAGE_TRANSFER_SRC_BIT, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, 0, MemoryKind::Staging, "glyph staging")?;
        // SAFETY: `mapped` is coherent host memory of at least `bytes` bytes nothing else uses;
        // `copy` holds exactly `bytes` bytes.
        unsafe { ptr::copy_nonoverlapping(copy.as_ptr() as *const u8, staging.mapped as *mut u8, bytes as usize) };
        let target = if fits {
            None
        } else {
            let usage = VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT;
            match dev.create_buffer((capacity * 4) as u64, usage, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, MemoryKind::Glyphs, "glyph store") {
                Ok(b) => Some(b),
                Err(e) => {
                    dev.destroy_buffer(&staging);
                    return Err(e);
                }
            }
        };
        let dst = target.as_ref().or(self.buffer.as_ref()).map(|b| b.handle).expect("a buffer: fits implies one");
        let record = |dev: &Device, cmd: VkCommandBuffer| {
            let region = VkBufferCopy { src_offset: 0, dst_offset: (copy_from * 4) as u64, size: bytes };
            if bytes > 0 {
                // SAFETY: recording; both buffers are live; the region lies inside each (the
                // destination holds `capacity` ≥ `copy_from + copy.len()` words).
                unsafe { (dev.fns.cmd_copy_buffer)(cmd, staging.handle, dst, 1, &region) };
            }
            dev.barrier(cmd, &[buffer_barrier(dst, VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_WRITE_BIT, VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, VK_ACCESS_2_SHADER_STORAGE_READ_BIT)], &[]);
        };
        let ran = run(dev, &record);
        dev.destroy_buffer(&staging);
        if let Err(e) = ran {
            if let Some(b) = target {
                dev.destroy_buffer(&b);
            }
            return Err(e);
        }
        if let Some(b) = target {
            if let Some(old) = self.buffer.take() {
                uploads.retire(Retired::Buffer(old));
            }
            self.address = dev.buffer_address(&b);
            self.buffer = Some(b);
            self.capacity = capacity;
            self.rebuilds += 1;
        }
        self.store = text.store;
        self.uploaded = text.words.len();
        self.words_copied += copy.len() as u64;
        self.failure = None;
        Ok(())
    }

    /// Frees the buffer now (the device is idle: shutdown).
    pub fn destroy(&mut self, dev: &Device) {
        if let Some(b) = self.buffer.take() {
            dev.destroy_buffer(&b);
        }
        self.uploaded = 0;
    }
}

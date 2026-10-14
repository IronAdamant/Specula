//! The one global descriptor set every pipeline binds (set 0):
//!
//! - binding 0: the sun's shadow map (`sampler2DArrayShadow`, comparison sampler);
//! - binding 1: the bindless texture array (`sampler2D textures[MAX_TEXTURE_SLOTS]`): uploaded
//!   textures and the post chain's intermediate images, each in a slot.
//!
//! Both bindings are update-after-bind and partially bound, so a slot can be written while frames
//! that don't use it are in flight, and unwritten slots are fine as long as no shader reads them.

use crate::device::Device;
use crate::vk::*;
use crate::{GpuError, Limit};
use std::ptr;

/// Slots in the bindless texture array.
pub const MAX_TEXTURE_SLOTS: u32 = 4096;

const BINDING_SHADOW: u32 = 0;
const BINDING_TEXTURES: u32 = 1;

pub struct Descriptors {
    pub layout: VkDescriptorSetLayout,
    pool: VkDescriptorPool,
    pub set: VkDescriptorSet,
    /// Bilinear, clamp to edge: textures and post inputs.
    pub linear: VkSampler,
    /// Depth comparison with a white border: bilinear (hardware 2×2 PCF) where the device can
    /// filter D16 linearly, nearest otherwise.
    pub shadow: VkSampler,
    /// The view binding 0 points at (null until a frame first renders shadows).
    pub shadow_view: VkImageView,
    free: Vec<u32>,
    next: u32,
}

fn create_sampler(dev: &Device, compare: bool, linear: bool) -> Result<VkSampler, GpuError> {
    let address = if compare { VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER } else { VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE };
    let filter = if linear { VK_FILTER_LINEAR } else { VK_FILTER_NEAREST };
    let info = VkSamplerCreateInfo {
        s_type: ST_SAMPLER_CREATE_INFO,
        p_next: ptr::null(),
        flags: 0,
        mag_filter: filter,
        min_filter: filter,
        mipmap_mode: VK_SAMPLER_MIPMAP_MODE_NEAREST,
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mip_lod_bias: 0.0,
        anisotropy_enable: VK_FALSE,
        max_anisotropy: 1.0,
        compare_enable: compare as VkBool32,
        compare_op: VK_COMPARE_OP_LESS_OR_EQUAL,
        min_lod: 0.0,
        max_lod: 0.0,
        border_color: VK_BORDER_COLOR_FLOAT_OPAQUE_WHITE,
        unnormalized_coordinates: VK_FALSE,
    };
    let mut sampler = VK_NULL_HANDLE;
    // SAFETY: valid device and create info.
    let r = unsafe { (dev.fns.create_sampler)(dev.handle, &info, ptr::null(), &mut sampler) };
    if r != VK_SUCCESS {
        return Err(GpuError::vk("vkCreateSampler", r));
    }
    Ok(sampler)
}

impl Descriptors {
    /// `shadow_linear`: the device filters `D16_UNORM` linearly (else the shadow sampler is nearest).
    pub fn new(dev: &Device, shadow_linear: bool) -> Result<Descriptors, GpuError> {
        let mut d = Descriptors {
            layout: VK_NULL_HANDLE,
            pool: VK_NULL_HANDLE,
            set: VK_NULL_HANDLE,
            linear: VK_NULL_HANDLE,
            shadow: VK_NULL_HANDLE,
            shadow_view: VK_NULL_HANDLE,
            free: Vec::new(),
            next: 0,
        };
        match d.init(dev, shadow_linear) {
            Ok(()) => Ok(d),
            Err(e) => {
                d.destroy(dev);
                Err(e)
            }
        }
    }

    fn init(&mut self, dev: &Device, shadow_linear: bool) -> Result<(), GpuError> {
        self.linear = create_sampler(dev, false, true)?;
        self.shadow = create_sampler(dev, true, shadow_linear)?;
        let stages = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;
        let bindings = [
            VkDescriptorSetLayoutBinding {
                binding: BINDING_SHADOW,
                descriptor_type: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
                stage_flags: stages,
                p_immutable_samplers: ptr::null(),
            },
            VkDescriptorSetLayoutBinding {
                binding: BINDING_TEXTURES,
                descriptor_type: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
                descriptor_count: MAX_TEXTURE_SLOTS,
                stage_flags: stages,
                p_immutable_samplers: ptr::null(),
            },
        ];
        let flags = [VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT
            | VK_DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING_BIT
            | VK_DESCRIPTOR_BINDING_PARTIALLY_BOUND_BIT; 2];
        let binding_flags = VkDescriptorSetLayoutBindingFlagsCreateInfo {
            s_type: ST_DESCRIPTOR_SET_LAYOUT_BINDING_FLAGS_CREATE_INFO,
            p_next: ptr::null(),
            binding_count: flags.len() as u32,
            p_binding_flags: flags.as_ptr(),
        };
        let info = VkDescriptorSetLayoutCreateInfo {
            s_type: ST_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
            p_next: &binding_flags as *const _ as *const std::ffi::c_void,
            flags: VK_DESCRIPTOR_SET_LAYOUT_CREATE_UPDATE_AFTER_BIND_POOL_BIT,
            binding_count: bindings.len() as u32,
            p_bindings: bindings.as_ptr(),
        };
        // SAFETY: valid device; `info` and everything it points to live for the call.
        let r = unsafe { (dev.fns.create_descriptor_set_layout)(dev.handle, &info, ptr::null(), &mut self.layout) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateDescriptorSetLayout", r));
        }
        let size = VkDescriptorPoolSize { ty: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, descriptor_count: MAX_TEXTURE_SLOTS + 1 };
        let pool_info = VkDescriptorPoolCreateInfo {
            s_type: ST_DESCRIPTOR_POOL_CREATE_INFO,
            p_next: ptr::null(),
            flags: VK_DESCRIPTOR_POOL_CREATE_UPDATE_AFTER_BIND_BIT,
            max_sets: 1,
            pool_size_count: 1,
            p_pool_sizes: &size,
        };
        // SAFETY: valid device and create info.
        let r = unsafe { (dev.fns.create_descriptor_pool)(dev.handle, &pool_info, ptr::null(), &mut self.pool) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateDescriptorPool", r));
        }
        let alloc = VkDescriptorSetAllocateInfo {
            s_type: ST_DESCRIPTOR_SET_ALLOCATE_INFO,
            p_next: ptr::null(),
            descriptor_pool: self.pool,
            descriptor_set_count: 1,
            p_set_layouts: &self.layout,
        };
        // SAFETY: the pool has room for this one set of this layout.
        let r = unsafe { (dev.fns.allocate_descriptor_sets)(dev.handle, &alloc, &mut self.set) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkAllocateDescriptorSets", r));
        }
        Ok(())
    }

    /// A free texture slot, or a structured error when all [`MAX_TEXTURE_SLOTS`] are taken.
    pub fn alloc_slot(&mut self) -> Result<u32, GpuError> {
        if let Some(slot) = self.free.pop() {
            return Ok(slot);
        }
        if self.next >= MAX_TEXTURE_SLOTS {
            return Err(GpuError::limit(Limit::Textures, MAX_TEXTURE_SLOTS as u64 + 1, MAX_TEXTURE_SLOTS as u64));
        }
        self.next += 1;
        Ok(self.next - 1)
    }

    /// Give a slot back. No pending frame may still read it.
    pub fn free_slot(&mut self, slot: u32) {
        debug_assert!(slot < self.next && !self.free.contains(&slot));
        self.free.push(slot);
    }

    /// Point `slot` at `view` (in `SHADER_READ_ONLY_OPTIMAL`), sampled bilinearly. No pending
    /// frame may read the slot (update-unused-while-pending allows every other slot to be in use).
    pub fn write_texture(&self, dev: &Device, slot: u32, view: VkImageView) {
        self.write(dev, BINDING_TEXTURES, slot, self.linear, view);
    }

    /// Point binding 0 at the shadow map's array view. No pending frame may use it.
    pub fn write_shadow(&mut self, dev: &Device, view: VkImageView) {
        self.write(dev, BINDING_SHADOW, 0, self.shadow, view);
        self.shadow_view = view;
    }

    fn write(&self, dev: &Device, binding: u32, element: u32, sampler: VkSampler, view: VkImageView) {
        let image = VkDescriptorImageInfo { sampler, image_view: view, image_layout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL };
        let write = VkWriteDescriptorSet {
            s_type: ST_WRITE_DESCRIPTOR_SET,
            p_next: ptr::null(),
            dst_set: self.set,
            dst_binding: binding,
            dst_array_element: element,
            descriptor_count: 1,
            descriptor_type: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            p_image_info: &image,
            p_buffer_info: ptr::null(),
            p_texel_buffer_view: ptr::null(),
        };
        // SAFETY: valid set, binding and element; `image` lives for the call; the caller
        // guarantees no pending command buffer reads this descriptor.
        unsafe { (dev.fns.update_descriptor_sets)(dev.handle, 1, &write, 0, ptr::null()) };
    }

    /// Bind the set for graphics with `layout` (any pipeline layout built on [`Descriptors::layout`]).
    pub fn bind(&self, dev: &Device, cmd: VkCommandBuffer, layout: VkPipelineLayout) {
        // SAFETY: `cmd` is recording; the set is compatible with `layout` (set 0).
        unsafe { (dev.fns.cmd_bind_descriptor_sets)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, layout, 0, 1, &self.set, 0, ptr::null()) };
    }

    pub fn destroy(&mut self, dev: &Device) {
        // SAFETY: the device is idle; each object belongs to `dev` and is destroyed once (the set
        // is freed with its pool; null handles are ignored).
        unsafe {
            (dev.fns.destroy_descriptor_pool)(dev.handle, self.pool, ptr::null());
            (dev.fns.destroy_descriptor_set_layout)(dev.handle, self.layout, ptr::null());
            (dev.fns.destroy_sampler)(dev.handle, self.linear, ptr::null());
            (dev.fns.destroy_sampler)(dev.handle, self.shadow, ptr::null());
        }
        self.pool = VK_NULL_HANDLE;
        self.layout = VK_NULL_HANDLE;
        self.set = VK_NULL_HANDLE;
        self.linear = VK_NULL_HANDLE;
        self.shadow = VK_NULL_HANDLE;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn slot_ids_fit_the_shader_encoding() {
        // Overlay quads store `slot + 1` (0 = no texture) in a u32: plenty of room.
        const { assert!(super::MAX_TEXTURE_SLOTS < u32::MAX) };
    }
}

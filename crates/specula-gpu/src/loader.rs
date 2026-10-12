//! Runtime loading of the Vulkan loader library (never linked).
//!
//! The library is opened once per process and kept loaded for the process lifetime: every Vulkan
//! function pointer handed out anywhere in this crate stays valid.

use crate::vk::{self, EntryFns, PfnGetInstanceProcAddr, VkInstance};
use crate::GpuError;
use specula_sys::dynlib::Library;
use std::ffi::CStr;
use std::sync::OnceLock;

/// Global (pre-instance) Vulkan entry points.
pub struct Entry {
    /// Keeps `vulkan-1` mapped; all function pointers below come from it.
    _library: Library,
    pub get_instance_proc_addr: PfnGetInstanceProcAddr,
    pub fns: EntryFns,
    /// Packed instance-level API version reported by the loader.
    pub instance_version: u32,
}

impl Entry {
    /// Resolve an instance-level entry point (`instance` may be null for global commands).
    pub fn proc_addr(&self, instance: VkInstance, name: &CStr) -> vk::PfnVoidFunction {
        // SAFETY: `get_instance_proc_addr` is the loader's `vkGetInstanceProcAddr`; `name` is
        // NUL-terminated and `instance` is null or a live instance created through this loader.
        unsafe { (self.get_instance_proc_addr)(instance, name.as_ptr()) }
    }
}

static ENTRY: OnceLock<Result<Entry, GpuError>> = OnceLock::new();

/// The process-wide Vulkan entry, loading `vulkan-1` on first use.
pub fn entry() -> Result<&'static Entry, GpuError> {
    match ENTRY.get_or_init(load) {
        Ok(entry) => Ok(entry),
        Err(e) => Err(e.clone()),
    }
}

#[cfg(windows)]
const LIBRARY_NAME: &str = "vulkan-1.dll";

fn load() -> Result<Entry, GpuError> {
    #[cfg(not(windows))]
    {
        Err(GpuError::new("Vulkan is unsupported on this platform (specula-gpu only loads vulkan-1.dll on Windows so far)"))
    }
    #[cfg(windows)]
    {
        let library = Library::open(LIBRARY_NAME)
            .map_err(|e| GpuError::new(format!("Vulkan loader not available: {e}")))?;
        let raw = library
            .symbol("vkGetInstanceProcAddr")
            .ok_or_else(|| GpuError::new(format!("{LIBRARY_NAME} does not export vkGetInstanceProcAddr")))?;
        // SAFETY: `raw` is the non-null address of `vkGetInstanceProcAddr` exported by the Vulkan
        // loader, whose C prototype is `PFN_vkVoidFunction (VkInstance, const char*)`; `library`
        // is stored in the returned `Entry` (and that in a process-lifetime static), so the code
        // stays mapped.
        let get_instance_proc_addr = unsafe { std::mem::transmute::<*mut std::ffi::c_void, PfnGetInstanceProcAddr>(raw) };
        let global = |name: &CStr| {
            // SAFETY: global commands are queried with a NULL instance, as the spec requires.
            unsafe { get_instance_proc_addr(std::ptr::null_mut(), name.as_ptr()) }
        };
        let fns = EntryFns::load(global)
            .map_err(|name| GpuError::new(format!("Vulkan loader is missing global command {name}")))?;
        let instance_version = match global(c"vkEnumerateInstanceVersion") {
            None => vk::make_api_version(1, 0, 0),
            Some(f) => {
                // SAFETY: the loader returned `vkEnumerateInstanceVersion`, prototype
                // `VkResult (uint32_t*)`.
                let f = unsafe { std::mem::transmute::<unsafe extern "system" fn(), vk::PfnEnumerateInstanceVersion>(f) };
                let mut v = 0u32;
                // SAFETY: `v` is a valid out-pointer for the duration of the call.
                let r = unsafe { f(&mut v) };
                if r != vk::VK_SUCCESS {
                    return Err(GpuError::vk("vkEnumerateInstanceVersion", r));
                }
                v
            }
        };
        Ok(Entry { _library: library, get_instance_proc_addr, fns, instance_version })
    }
}

//! Hand-declared COM: the `ole32` imports (this crate owns `ole32`, see `specula-sys` docs) and
//! the WASAPI interface vtables, in declaration order from `mmdeviceapi.h` / `audioclient.h`.
//!
//! A COM interface pointer points at an object whose first field points at a vtable of
//! `extern "system"` functions, each taking the object as `this`. The vtables below list every
//! method up to the last one we call, in exact order; the layout tests pin their offsets.

#![allow(non_snake_case, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::ptr::NonNull;

pub type HRESULT = i32;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Guid {
        Guid { data1, data2, data3, data4 }
    }
}

pub const CLSID_MM_DEVICE_ENUMERATOR: Guid = Guid::new(0xBCDE0395, 0xE52F, 0x467C, [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E]);
pub const IID_IMM_DEVICE_ENUMERATOR: Guid = Guid::new(0xA95664D2, 0x9614, 0x4F35, [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6]);
pub const IID_IAUDIO_CLIENT: Guid = Guid::new(0x1CB9AD4C, 0xDBFA, 0x4C32, [0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2]);
pub const IID_IAUDIO_RENDER_CLIENT: Guid = Guid::new(0xF294ACFC, 0x3146, 0x4483, [0xA7, 0xBF, 0xAD, 0xDC, 0xA7, 0xC2, 0x60, 0xE2]);
pub const KSDATAFORMAT_SUBTYPE_PCM: Guid = Guid::new(0x00000001, 0x0000, 0x0010, [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);
pub const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: Guid = Guid::new(0x00000003, 0x0000, 0x0010, [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);

pub const COINIT_MULTITHREADED: u32 = 0;
pub const CLSCTX_ALL: u32 = 0x1 | 0x2 | 0x4 | 0x10;
pub const E_RENDER: i32 = 0;
pub const E_CONSOLE: i32 = 0;
pub const AUDCLNT_SHAREMODE_SHARED: i32 = 0;
pub const AUDCLNT_STREAMFLAGS_EVENTCALLBACK: u32 = 0x0004_0000;
pub const AUDCLNT_STREAMFLAGS_NOPERSIST: u32 = 0x0008_0000;
pub const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;
pub const WAVE_FORMAT_PCM: u16 = 1;
pub const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
pub const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

pub const S_OK: HRESULT = 0;
pub const S_FALSE: HRESULT = 1;
/// No device of the requested kind (`HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`).
pub const E_NOTFOUND: u32 = 0x8007_0490;
pub const AUDCLNT_E_DEVICE_INVALIDATED: u32 = 0x8889_0004;
pub const AUDCLNT_E_SERVICE_NOT_RUNNING: u32 = 0x8889_0010;
pub const AUDCLNT_E_ENDPOINT_CREATE_FAILED: u32 = 0x8889_000F;
pub const AUDCLNT_E_DEVICE_IN_USE: u32 = 0x8889_000A;

/// `WAVEFORMATEX` (packed to 1 byte in `mmreg.h`: 18 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct WaveFormatEx {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub cb_size: u16,
}

/// `WAVEFORMATEXTENSIBLE` (40 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct WaveFormatExtensible {
    pub format: WaveFormatEx,
    /// `wValidBitsPerSample` (union with `wSamplesPerBlock`).
    pub valid_bits: u16,
    pub channel_mask: u32,
    pub sub_format: Guid,
}

#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    pub fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> HRESULT;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(clsid: *const Guid, outer: *mut c_void, cls_context: u32, iid: *const Guid, out: *mut *mut c_void) -> HRESULT;
    pub fn CoTaskMemFree(pv: *mut c_void);
}

type This = *mut c_void;

#[repr(C)]
pub struct IUnknownVtbl {
    pub QueryInterface: unsafe extern "system" fn(This, *const Guid, *mut *mut c_void) -> HRESULT,
    pub AddRef: unsafe extern "system" fn(This) -> u32,
    pub Release: unsafe extern "system" fn(This) -> u32,
}

#[repr(C)]
pub struct IMMDeviceEnumeratorVtbl {
    pub base: IUnknownVtbl,
    pub EnumAudioEndpoints: unsafe extern "system" fn(This, i32, u32, *mut *mut c_void) -> HRESULT,
    pub GetDefaultAudioEndpoint: unsafe extern "system" fn(This, i32, i32, *mut *mut c_void) -> HRESULT,
    pub GetDevice: unsafe extern "system" fn(This, *const u16, *mut *mut c_void) -> HRESULT,
    pub RegisterEndpointNotificationCallback: unsafe extern "system" fn(This, *mut c_void) -> HRESULT,
    pub UnregisterEndpointNotificationCallback: unsafe extern "system" fn(This, *mut c_void) -> HRESULT,
}

#[repr(C)]
pub struct IMMDeviceVtbl {
    pub base: IUnknownVtbl,
    pub Activate: unsafe extern "system" fn(This, *const Guid, u32, *mut c_void, *mut *mut c_void) -> HRESULT,
    pub OpenPropertyStore: unsafe extern "system" fn(This, u32, *mut *mut c_void) -> HRESULT,
    pub GetId: unsafe extern "system" fn(This, *mut *mut u16) -> HRESULT,
    pub GetState: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
}

#[repr(C)]
pub struct IAudioClientVtbl {
    pub base: IUnknownVtbl,
    pub Initialize: unsafe extern "system" fn(This, i32, u32, i64, i64, *const WaveFormatEx, *const Guid) -> HRESULT,
    pub GetBufferSize: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    pub GetStreamLatency: unsafe extern "system" fn(This, *mut i64) -> HRESULT,
    pub GetCurrentPadding: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    pub IsFormatSupported: unsafe extern "system" fn(This, i32, *const WaveFormatEx, *mut *mut WaveFormatEx) -> HRESULT,
    pub GetMixFormat: unsafe extern "system" fn(This, *mut *mut WaveFormatEx) -> HRESULT,
    pub GetDevicePeriod: unsafe extern "system" fn(This, *mut i64, *mut i64) -> HRESULT,
    pub Start: unsafe extern "system" fn(This) -> HRESULT,
    pub Stop: unsafe extern "system" fn(This) -> HRESULT,
    pub Reset: unsafe extern "system" fn(This) -> HRESULT,
    pub SetEventHandle: unsafe extern "system" fn(This, *mut c_void) -> HRESULT,
    pub GetService: unsafe extern "system" fn(This, *const Guid, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
pub struct IAudioRenderClientVtbl {
    pub base: IUnknownVtbl,
    pub GetBuffer: unsafe extern "system" fn(This, u32, *mut *mut u8) -> HRESULT,
    pub ReleaseBuffer: unsafe extern "system" fn(This, u32, u32) -> HRESULT,
}

/// An owned COM interface pointer whose vtable is `V`; released on drop.
pub struct ComPtr<V> {
    ptr: NonNull<*const V>,
}

impl<V> ComPtr<V> {
    /// Takes ownership of one reference.
    ///
    /// # Safety
    /// `raw` must be null or a live COM interface pointer whose vtable layout starts with `V`.
    pub unsafe fn from_raw(raw: *mut c_void) -> Option<ComPtr<V>> {
        NonNull::new(raw as *mut *const V).map(|ptr| ComPtr { ptr })
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.ptr.as_ptr() as *mut c_void
    }

    /// The vtable.
    pub fn vtbl(&self) -> &V {
        // SAFETY: `from_raw`'s contract: the object's first field points at a vtable of type `V`,
        // which lives as long as the object (we hold a reference).
        unsafe { &**self.ptr.as_ptr() }
    }
}

impl<V> Drop for ComPtr<V> {
    fn drop(&mut self) {
        // SAFETY: every COM vtable starts with IUnknown; we own exactly one reference.
        unsafe {
            let unknown = &**(self.ptr.as_ptr() as *mut *const IUnknownVtbl);
            (unknown.Release)(self.as_raw());
        }
    }
}

/// `true` for `SUCCEEDED(hr)`.
pub fn succeeded(hr: HRESULT) -> bool {
    hr >= 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    const P: usize = size_of::<usize>();

    #[test]
    fn struct_layouts_match_the_windows_headers() {
        assert_eq!(size_of::<Guid>(), 16);
        assert_eq!(size_of::<WaveFormatEx>(), 18);
        assert_eq!(size_of::<WaveFormatExtensible>(), 40);
        assert_eq!(offset_of!(WaveFormatExtensible, valid_bits), 18);
        assert_eq!(offset_of!(WaveFormatExtensible, channel_mask), 20);
        assert_eq!(offset_of!(WaveFormatExtensible, sub_format), 24);
    }

    #[test]
    fn vtable_slots_are_in_header_order() {
        assert_eq!(size_of::<IUnknownVtbl>(), 3 * P);
        assert_eq!(offset_of!(IMMDeviceEnumeratorVtbl, GetDefaultAudioEndpoint), 4 * P);
        assert_eq!(size_of::<IMMDeviceEnumeratorVtbl>(), 8 * P);
        assert_eq!(offset_of!(IMMDeviceVtbl, Activate), 3 * P);
        assert_eq!(offset_of!(IMMDeviceVtbl, GetId), 5 * P);
        assert_eq!(size_of::<IMMDeviceVtbl>(), 7 * P);
        assert_eq!(offset_of!(IAudioClientVtbl, Initialize), 3 * P);
        assert_eq!(offset_of!(IAudioClientVtbl, GetMixFormat), 8 * P);
        assert_eq!(offset_of!(IAudioClientVtbl, Start), 10 * P);
        assert_eq!(offset_of!(IAudioClientVtbl, SetEventHandle), 13 * P);
        assert_eq!(offset_of!(IAudioClientVtbl, GetService), 14 * P);
        assert_eq!(size_of::<IAudioClientVtbl>(), 15 * P);
        assert_eq!(offset_of!(IAudioRenderClientVtbl, GetBuffer), 3 * P);
        assert_eq!(size_of::<IAudioRenderClientVtbl>(), 5 * P);
    }
}

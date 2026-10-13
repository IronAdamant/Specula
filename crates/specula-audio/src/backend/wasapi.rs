//! WASAPI shared-mode, event-driven output on a dedicated audio thread.
//!
//! - Opens the default render endpoint (console role) at its **mix format** (any rate, any
//!   channel count, float or integer PCM); the mixer renders stereo at that rate and this module
//!   up/down-mixes into the device's channels.
//! - Waits on an event the audio engine signals each device period, then fills
//!   `buffer - padding` frames. Requested buffer ≈ the configured latency (default 30 ms).
//! - **Device loss** (unplugged, disabled, audio service restart: any failing `HRESULT` from the
//!   render loop) and **default-device changes** (polled by a watcher thread every 500 ms) close
//!   the stream and reopen on the current default. While no device is available, the mixer keeps
//!   running against the wall clock (backend state `reconnecting`) and retries with back-off.
//! - `kernel32` is owned by `specula-sys`, so `CreateEventW` / `WaitForSingleObject` are looked up
//!   at runtime; `avrt` (MMCSS "Pro Audio" scheduling) is optional and also runtime-loaded.

use super::com::*;
use super::{set_state, Clock, ThreadBackend};
use crate::error::{AudioError, ErrorCode, Result};
use crate::mixer::{BackendState, Mixer, SharedStats};
use specula_sys::dynlib::Library;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WAIT_TIMEOUT: u32 = 0x102;
const WAIT_FAILED: u32 = 0xFFFF_FFFF;
/// Event waits that time out in a row (100 ms each) before the device counts as lost.
const MAX_TIMEOUTS: u32 = 20;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Options {
    pub latency_ms: u32,
    /// Fail `start` if no device opens (instead of running on the clock and retrying).
    pub require_device: bool,
}

fn hr_error(call: &str, hr: HRESULT) -> AudioError {
    let u = hr as u32;
    let code = match u {
        // No endpoint, or the Windows audio service is off: either way there is no device to use.
        E_NOTFOUND | AUDCLNT_E_SERVICE_NOT_RUNNING => ErrorCode::NoDevice,
        AUDCLNT_E_DEVICE_INVALIDATED | AUDCLNT_E_ENDPOINT_CREATE_FAILED | AUDCLNT_E_DEVICE_IN_USE => ErrorCode::DeviceLost,
        _ => ErrorCode::Os,
    };
    AudioError::at(code, call, format!("{call} failed")).with_hresult(u)
}

fn check(call: &str, hr: HRESULT) -> Result<()> {
    if succeeded(hr) {
        Ok(())
    } else {
        Err(hr_error(call, hr))
    }
}

/// `CoInitializeEx` (multithreaded) for the current thread; balanced on drop.
struct ComInit;

impl ComInit {
    fn new() -> Result<ComInit> {
        // SAFETY: plain call with a null reserved pointer.
        let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED) };
        if hr == S_OK || hr == S_FALSE {
            Ok(ComInit)
        } else {
            Err(hr_error("CoInitializeEx", hr))
        }
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx on this thread.
        unsafe { CoUninitialize() };
    }
}

type CreateEventW = unsafe extern "system" fn(*mut c_void, i32, i32, *const u16) -> *mut c_void;
type WaitForSingleObject = unsafe extern "system" fn(*mut c_void, u32) -> u32;
type AvSetMmThreadCharacteristicsW = unsafe extern "system" fn(*const u16, *mut u32) -> *mut c_void;
type AvRevertMmThreadCharacteristics = unsafe extern "system" fn(*mut c_void) -> i32;

struct Kernel32 {
    _lib: Library,
    create_event: CreateEventW,
    wait: WaitForSingleObject,
}

impl Kernel32 {
    fn load() -> Result<Kernel32> {
        let lib = Library::open("kernel32.dll").map_err(|e| AudioError::new(ErrorCode::Os, e.to_string()))?;
        let create = lib.symbol("CreateEventW");
        let wait = lib.symbol("WaitForSingleObject");
        match (create, wait) {
            // SAFETY: the symbols are kernel32's documented exports with exactly these
            // signatures; `_lib` keeps the module loaded for as long as the pointers are used.
            (Some(c), Some(w)) => unsafe {
                Ok(Kernel32 {
                    create_event: std::mem::transmute::<*mut c_void, CreateEventW>(c),
                    wait: std::mem::transmute::<*mut c_void, WaitForSingleObject>(w),
                    _lib: lib,
                })
            },
            _ => Err(AudioError::new(ErrorCode::Os, "kernel32 lacks CreateEventW/WaitForSingleObject")),
        }
    }
}

/// MMCSS registration for the audio thread (optional: skipped if `avrt.dll` is missing).
struct Mmcss {
    lib: Option<Library>,
    handle: *mut c_void,
}

impl Mmcss {
    fn join() -> Mmcss {
        let Ok(lib) = Library::open("avrt.dll") else {
            return Mmcss { lib: None, handle: std::ptr::null_mut() };
        };
        let mut handle = std::ptr::null_mut();
        if let Some(f) = lib.symbol("AvSetMmThreadCharacteristicsW") {
            let name = specula_sys::wide("Pro Audio");
            let mut task = 0u32;
            // SAFETY: documented avrt export with this signature; `name` is NUL-terminated and
            // outlives the call; `task` is writable.
            handle = unsafe { std::mem::transmute::<*mut c_void, AvSetMmThreadCharacteristicsW>(f)(name.as_ptr(), &mut task) };
        }
        Mmcss { lib: Some(lib), handle }
    }
}

impl Drop for Mmcss {
    fn drop(&mut self) {
        if let (Some(lib), false) = (&self.lib, self.handle.is_null()) {
            if let Some(f) = lib.symbol("AvRevertMmThreadCharacteristics") {
                // SAFETY: `handle` came from AvSetMmThreadCharacteristicsW on this thread.
                unsafe { std::mem::transmute::<*mut c_void, AvRevertMmThreadCharacteristics>(f)(self.handle) };
            }
        }
    }
}

struct EventHandle(*mut c_void);

impl Drop for EventHandle {
    fn drop(&mut self) {
        // SAFETY: a handle from CreateEventW, closed exactly once.
        unsafe { specula_sys::kernel32::CloseHandle(self.0) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SampleKind {
    F32,
    I16,
    I24,
    I32,
}

#[derive(Clone, Copy, Debug)]
struct OutFormat {
    kind: SampleKind,
    channels: usize,
    block_align: usize,
    rate: u32,
}

/// Reads a device mix format.
///
/// # Safety
/// `p` must point at a valid `WAVEFORMATEX` (and, when tagged extensible, the extended tail).
unsafe fn parse_format(p: *const WaveFormatEx) -> Result<OutFormat> {
    let wf = std::ptr::read_unaligned(p);
    let (tag, channels, rate, align, bits) = (wf.format_tag, wf.channels, wf.samples_per_sec, wf.block_align, wf.bits_per_sample);
    let float = match tag {
        WAVE_FORMAT_IEEE_FLOAT => true,
        WAVE_FORMAT_PCM => false,
        WAVE_FORMAT_EXTENSIBLE if wf.cb_size >= 22 => {
            let ext = std::ptr::read_unaligned(p as *const WaveFormatExtensible);
            let sub = ext.sub_format;
            if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                true
            } else if sub == KSDATAFORMAT_SUBTYPE_PCM {
                false
            } else {
                return Err(AudioError::new(ErrorCode::DeviceFormat, "device mix format has an unknown sub-format").with_numbers(tag as f64, bits as f64));
            }
        }
        _ => return Err(AudioError::new(ErrorCode::DeviceFormat, "device mix format is not PCM or float").with_numbers(tag as f64, bits as f64)),
    };
    let kind = match (float, bits) {
        (true, 32) => SampleKind::F32,
        (false, 16) => SampleKind::I16,
        (false, 24) => SampleKind::I24,
        (false, 32) => SampleKind::I32,
        _ => return Err(AudioError::new(ErrorCode::DeviceFormat, "unsupported device sample size").with_numbers(bits as f64, 32.0)),
    };
    let channels = channels as usize;
    let container = match kind {
        SampleKind::I16 => 2,
        SampleKind::I24 => 3,
        _ => 4,
    };
    if channels == 0 || align as usize != channels * container || rate < 8_000 {
        return Err(AudioError::new(ErrorCode::DeviceFormat, "inconsistent device mix format").with_numbers(align as f64, (channels * container) as f64));
    }
    Ok(OutFormat { kind, channels, block_align: align as usize, rate })
}

/// Writes stereo `src` into a device buffer: mono devices get (L+R)/2, others get L and R on the
/// front pair and silence on the rest.
fn write_frames(dst: &mut [u8], fmt: &OutFormat, src: &[f32]) {
    let ch = fmt.channels;
    let bytes = fmt.block_align / ch;
    for (frame, lr) in dst.chunks_exact_mut(fmt.block_align).zip(src.chunks_exact(2)) {
        for c in 0..ch {
            let v = if ch == 1 {
                0.5 * (lr[0] + lr[1])
            } else if c < 2 {
                lr[c]
            } else {
                0.0
            };
            let out = &mut frame[c * bytes..(c + 1) * bytes];
            match fmt.kind {
                SampleKind::F32 => out.copy_from_slice(&v.to_le_bytes()),
                SampleKind::I16 => out.copy_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes()),
                SampleKind::I24 => out.copy_from_slice(&((v.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32).to_le_bytes()[..3]),
                SampleKind::I32 => out.copy_from_slice(&((v.clamp(-1.0, 1.0) as f64 * 2_147_483_647.0).round() as i32).to_le_bytes()),
            }
        }
    }
}

fn enumerator() -> Result<ComPtr<IMMDeviceEnumeratorVtbl>> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: valid GUID pointers and out-pointer; COM is initialised on this thread.
    let hr = unsafe { CoCreateInstance(&CLSID_MM_DEVICE_ENUMERATOR, std::ptr::null_mut(), CLSCTX_ALL, &IID_IMM_DEVICE_ENUMERATOR, &mut raw) };
    check("CoCreateInstance(MMDeviceEnumerator)", hr)?;
    // SAFETY: on success `raw` is an IMMDeviceEnumerator with one reference for us.
    unsafe { ComPtr::from_raw(raw) }.ok_or_else(|| AudioError::new(ErrorCode::Os, "null device enumerator"))
}

fn default_device(e: &ComPtr<IMMDeviceEnumeratorVtbl>) -> Result<ComPtr<IMMDeviceVtbl>> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: `e` is live; out-pointer valid.
    let hr = unsafe { (e.vtbl().GetDefaultAudioEndpoint)(e.as_raw(), E_RENDER, E_CONSOLE, &mut raw) };
    check("IMMDeviceEnumerator::GetDefaultAudioEndpoint", hr)?;
    // SAFETY: on success `raw` is an IMMDevice we own.
    unsafe { ComPtr::from_raw(raw) }.ok_or_else(|| AudioError::new(ErrorCode::NoDevice, "no default render device"))
}

/// The default render device's id string, or `None` if there is none.
fn default_id(e: &ComPtr<IMMDeviceEnumeratorVtbl>) -> Option<Vec<u16>> {
    let dev = default_device(e).ok()?;
    let mut s: *mut u16 = std::ptr::null_mut();
    // SAFETY: `dev` is live; on success `s` is a CoTaskMemAlloc'd NUL-terminated string we free.
    unsafe {
        if !succeeded((dev.vtbl().GetId)(dev.as_raw(), &mut s)) || s.is_null() {
            return None;
        }
        let mut len = 0;
        while *s.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let id = std::slice::from_raw_parts(s, len).to_vec();
        CoTaskMemFree(s as *mut c_void);
        Some(id)
    }
}

/// Polls the default device id every 500 ms on its own thread; bumps `generation` on change.
struct Watcher {
    generation: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
    fn start() -> Watcher {
        let generation = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (g, s) = (generation.clone(), stop.clone());
        let join = std::thread::Builder::new()
            .name("specula-audio-watch".into())
            .spawn(move || {
                let Ok(_com) = ComInit::new() else { return };
                let Ok(e) = enumerator() else { return };
                let mut last = default_id(&e);
                while !s.load(Ordering::Relaxed) {
                    for _ in 0..10 {
                        if s.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    let now = default_id(&e);
                    if now != last {
                        last = now;
                        g.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .ok();
        Watcher { generation, stop, join }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

struct Device {
    client: ComPtr<IAudioClientVtbl>,
    render: ComPtr<IAudioRenderClientVtbl>,
    event: EventHandle,
    format: OutFormat,
    buffer_frames: u32,
    latency_us: u32,
}

enum Exit {
    Stop,
    Lost(u32),
    DefaultChanged,
}

impl Device {
    fn open(k32: &Kernel32, latency_ms: u32) -> Result<Device> {
        let e = enumerator()?;
        let dev = default_device(&e)?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: `dev` is live; null activation params are allowed for IAudioClient.
        let hr = unsafe { (dev.vtbl().Activate)(dev.as_raw(), &IID_IAUDIO_CLIENT, CLSCTX_ALL, std::ptr::null_mut(), &mut raw) };
        check("IMMDevice::Activate(IAudioClient)", hr)?;
        // SAFETY: on success `raw` is an IAudioClient we own.
        let client: ComPtr<IAudioClientVtbl> = unsafe { ComPtr::from_raw(raw) }.ok_or_else(|| AudioError::new(ErrorCode::Os, "null IAudioClient"))?;

        let mut fmt_ptr: *mut WaveFormatEx = std::ptr::null_mut();
        // SAFETY: `client` is live; on success `fmt_ptr` is CoTaskMemAlloc'd and freed below.
        let hr = unsafe { (client.vtbl().GetMixFormat)(client.as_raw(), &mut fmt_ptr) };
        check("IAudioClient::GetMixFormat", hr)?;
        // SAFETY: GetMixFormat returned a valid format block.
        let parsed = unsafe { parse_format(fmt_ptr) };
        let hns = latency_ms.clamp(10, 500) as i64 * 10_000;
        let hr = match parsed {
            // SAFETY: `fmt_ptr` is the device's own mix format, valid for Initialize.
            Ok(_) => unsafe {
                (client.vtbl().Initialize)(
                    client.as_raw(),
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_NOPERSIST,
                    hns,
                    0,
                    fmt_ptr,
                    std::ptr::null(),
                )
            },
            Err(_) => S_OK,
        };
        // SAFETY: frees the block GetMixFormat allocated, exactly once.
        unsafe { CoTaskMemFree(fmt_ptr as *mut c_void) };
        let format = parsed?;
        check("IAudioClient::Initialize", hr)?;

        let mut buffer_frames = 0u32;
        let mut stream_latency = 0i64;
        // SAFETY: `client` is initialised; out-pointers valid.
        unsafe {
            check("IAudioClient::GetBufferSize", (client.vtbl().GetBufferSize)(client.as_raw(), &mut buffer_frames))?;
            let _ = (client.vtbl().GetStreamLatency)(client.as_raw(), &mut stream_latency);
        }
        // SAFETY: null security attributes / name; auto-reset, initially unsignalled.
        let handle = unsafe { (k32.create_event)(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if handle.is_null() {
            return Err(AudioError::new(ErrorCode::Os, "CreateEventW failed"));
        }
        let event = EventHandle(handle);
        // SAFETY: `client` initialised with EVENTCALLBACK; the event outlives the client's use of
        // it (the Device drops the client first: field order).
        check("IAudioClient::SetEventHandle", unsafe { (client.vtbl().SetEventHandle)(client.as_raw(), event.0) })?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: `client` initialised; out-pointer valid.
        check("IAudioClient::GetService(IAudioRenderClient)", unsafe { (client.vtbl().GetService)(client.as_raw(), &IID_IAUDIO_RENDER_CLIENT, &mut raw) })?;
        // SAFETY: on success `raw` is an IAudioRenderClient we own.
        let render = unsafe { ComPtr::from_raw(raw) }.ok_or_else(|| AudioError::new(ErrorCode::Os, "null IAudioRenderClient"))?;
        let latency_us = (buffer_frames as u64 * 1_000_000 / format.rate as u64 + stream_latency.max(0) as u64 / 10) as u32;
        Ok(Device { client, render, event, format, buffer_frames, latency_us })
    }

    /// Fills `frames` frames of the device buffer from the mixer.
    fn fill(&self, mixer: &mut Mixer, scratch: &mut [f32], frames: u32) -> std::result::Result<(), u32> {
        let mut data: *mut u8 = std::ptr::null_mut();
        // SAFETY: `render` is live; `frames` ≤ buffer - padding, as WASAPI requires.
        let hr = unsafe { (self.render.vtbl().GetBuffer)(self.render.as_raw(), frames, &mut data) };
        if !succeeded(hr) || data.is_null() {
            return Err(hr as u32);
        }
        let n = frames as usize;
        mixer.render(&mut scratch[..n * 2]);
        // SAFETY: GetBuffer returned a writable region of `frames * block_align` bytes that stays
        // valid until ReleaseBuffer.
        let dst = unsafe { std::slice::from_raw_parts_mut(data, n * self.format.block_align) };
        write_frames(dst, &self.format, &scratch[..n * 2]);
        // SAFETY: releases exactly the frames we got.
        let hr = unsafe { (self.render.vtbl().ReleaseBuffer)(self.render.as_raw(), frames, 0) };
        if succeeded(hr) {
            Ok(())
        } else {
            Err(hr as u32)
        }
    }

    fn run(&self, k32: &Kernel32, mixer: &mut Mixer, stats: &SharedStats, stop: &AtomicBool, watcher: &Watcher, generation: u64) -> Exit {
        // Pre-fill with silence so the first period doesn't glitch.
        let mut data = std::ptr::null_mut();
        // SAFETY: the whole (empty) buffer is available before Start.
        unsafe {
            if succeeded((self.render.vtbl().GetBuffer)(self.render.as_raw(), self.buffer_frames, &mut data)) {
                (self.render.vtbl().ReleaseBuffer)(self.render.as_raw(), self.buffer_frames, AUDCLNT_BUFFERFLAGS_SILENT);
            }
        }
        // SAFETY: `client` is initialised.
        let hr = unsafe { (self.client.vtbl().Start)(self.client.as_raw()) };
        if !succeeded(hr) {
            return Exit::Lost(hr as u32);
        }
        let _mmcss = Mmcss::join();
        // Allocated before the loop: the loop itself never allocates.
        let mut scratch = vec![0.0f32; self.buffer_frames as usize * 2];
        let mut timeouts = 0;
        let mut first = true;
        let exit = loop {
            // SAFETY: a live event handle.
            let r = unsafe { (k32.wait)(self.event.0, 100) };
            if stop.load(Ordering::Relaxed) {
                break Exit::Stop;
            }
            if watcher.generation() != generation {
                break Exit::DefaultChanged;
            }
            if r == WAIT_FAILED {
                break Exit::Lost(0);
            }
            if r == WAIT_TIMEOUT {
                timeouts += 1;
                if timeouts >= MAX_TIMEOUTS {
                    break Exit::Lost(0);
                }
                continue;
            }
            timeouts = 0;
            let mut padding = 0u32;
            // SAFETY: `client` is live; out-pointer valid.
            let hr = unsafe { (self.client.vtbl().GetCurrentPadding)(self.client.as_raw(), &mut padding) };
            if !succeeded(hr) {
                break Exit::Lost(hr as u32);
            }
            if padding == 0 && !first {
                stats.underruns.fetch_add(1, Ordering::Relaxed);
            }
            first = false;
            let frames = self.buffer_frames.saturating_sub(padding);
            if frames == 0 {
                continue;
            }
            if let Err(hr) = self.fill(mixer, &mut scratch, frames) {
                break Exit::Lost(hr);
            }
        };
        // SAFETY: `client` is live; stopping a stream that failed is harmless.
        unsafe { (self.client.vtbl().Stop)(self.client.as_raw()) };
        exit
    }
}

pub(crate) fn start(mixer: Mixer, options: Options) -> Result<ThreadBackend> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<()>>(1);
    let join = std::thread::Builder::new()
        .name("specula-audio".into())
        .spawn(move || thread_main(mixer, &stop2, options, tx))
        .map_err(|e| AudioError::new(ErrorCode::ThreadGone, format!("cannot spawn the audio thread: {e}")))?;
    let mut backend = ThreadBackend { stop, join: Some(join) };
    match rx.recv() {
        Ok(Ok(())) => Ok(backend),
        Ok(Err(e)) => {
            backend.shutdown();
            Err(e)
        }
        Err(_) => {
            backend.shutdown();
            Err(AudioError::new(ErrorCode::ThreadGone, "the audio thread exited during start-up"))
        }
    }
}

fn thread_main(mut mixer: Mixer, stop: &AtomicBool, options: Options, tx: std::sync::mpsc::SyncSender<Result<()>>) {
    let stats = mixer.stats().clone();
    let _guard = super::PanicGuard(stats.clone());
    let mut tx = Some(tx);
    let report = |tx: &mut Option<std::sync::mpsc::SyncSender<Result<()>>>, r: Result<()>| {
        if let Some(t) = tx.take() {
            let _ = t.send(r);
        }
    };
    let setup = ComInit::new().and_then(|c| Kernel32::load().map(|k| (c, k)));
    let (_com, k32) = match setup {
        Ok(v) => v,
        Err(e) => {
            if options.require_device {
                report(&mut tx, Err(e));
                return;
            }
            // No COM: behave as the null backend.
            set_state(&stats, BackendState::Null);
            report(&mut tx, Ok(()));
            let mut clock = Clock::new(mixer.rate(), 10);
            while !stop.load(Ordering::Relaxed) {
                clock.tick(&mut mixer, &stats, stop);
            }
            set_state(&stats, BackendState::Stopped);
            return;
        }
    };
    let watcher = Watcher::start();
    let mut backoff = Duration::from_millis(250);
    while !stop.load(Ordering::Relaxed) {
        let generation = watcher.generation();
        match Device::open(&k32, options.latency_ms) {
            Ok(dev) => {
                backoff = Duration::from_millis(250);
                mixer.set_rate(dev.format.rate);
                stats.device_channels.store(dev.format.channels as u32, Ordering::Relaxed);
                stats.buffer_frames.store(dev.buffer_frames, Ordering::Relaxed);
                stats.latency_us.store(dev.latency_us, Ordering::Relaxed);
                set_state(&stats, BackendState::Device);
                // Report only after the stats are published, so `start` returns a settled state.
                report(&mut tx, Ok(()));
                match dev.run(&k32, &mut mixer, &stats, stop, &watcher, generation) {
                    Exit::Stop => break,
                    Exit::Lost(hr) => {
                        stats.last_hresult.store(hr, Ordering::Relaxed);
                        stats.reconnects.fetch_add(1, Ordering::Relaxed);
                    }
                    Exit::DefaultChanged => {
                        stats.reconnects.fetch_add(1, Ordering::Relaxed);
                    }
                }
                set_state(&stats, BackendState::Reconnecting);
            }
            Err(e) => {
                if tx.is_some() && options.require_device {
                    report(&mut tx, Err(e));
                    return;
                }
                stats.last_hresult.store(e.hresult.unwrap_or(0), Ordering::Relaxed);
                set_state(&stats, BackendState::Reconnecting);
                report(&mut tx, Ok(()));
                // Keep voices moving in real time until the next attempt (or a device appears).
                let mut clock = Clock::new(mixer.rate(), 10);
                let until = Instant::now() + backoff;
                while Instant::now() < until && !stop.load(Ordering::Relaxed) && watcher.generation() == generation {
                    clock.tick(&mut mixer, &stats, stop);
                }
                backoff = (backoff * 2).min(Duration::from_secs(2));
            }
        }
    }
    set_state(&stats, BackendState::Stopped);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(kind: SampleKind, channels: usize) -> OutFormat {
        let c = match kind {
            SampleKind::I16 => 2,
            SampleKind::I24 => 3,
            _ => 4,
        };
        OutFormat { kind, channels, block_align: channels * c, rate: 48_000 }
    }

    #[test]
    fn writes_every_device_layout() {
        let src = [0.5f32, -0.5, 1.0, 0.25];
        let f = fmt(SampleKind::F32, 6);
        let mut dst = vec![0xAAu8; 2 * f.block_align];
        write_frames(&mut dst, &f, &src);
        let read = |b: &[u8], i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((read(&dst, 0), read(&dst, 1), read(&dst, 2), read(&dst, 5)), (0.5, -0.5, 0.0, 0.0));
        assert_eq!((read(&dst, 6), read(&dst, 7)), (1.0, 0.25));

        let f = fmt(SampleKind::I16, 1);
        let mut dst = vec![0u8; 4];
        write_frames(&mut dst, &f, &src);
        assert_eq!(i16::from_le_bytes([dst[0], dst[1]]), 0);
        assert_eq!(i16::from_le_bytes([dst[2], dst[3]]), (0.625f32 * 32767.0).round() as i16);

        let f = fmt(SampleKind::I24, 2);
        let mut dst = vec![0u8; 12];
        write_frames(&mut dst, &f, &src);
        let s24 = |b: &[u8]| (i32::from_le_bytes([b[0], b[1], b[2], 0]) << 8) >> 8;
        assert_eq!(s24(&dst[0..3]), 4_194_304);
        assert_eq!(s24(&dst[3..6]), -4_194_304);
        assert_eq!(s24(&dst[6..9]), 8_388_607);

        let f = fmt(SampleKind::I32, 2);
        let mut dst = vec![0u8; 16];
        write_frames(&mut dst, &f, &src);
        assert_eq!(i32::from_le_bytes(dst[8..12].try_into().unwrap()), i32::MAX);
    }

    #[test]
    fn parses_mix_formats() {
        let ext = WaveFormatExtensible {
            format: WaveFormatEx { format_tag: WAVE_FORMAT_EXTENSIBLE, channels: 8, samples_per_sec: 44_100, avg_bytes_per_sec: 0, block_align: 32, bits_per_sample: 32, cb_size: 22 },
            valid_bits: 32,
            channel_mask: 0x63F,
            sub_format: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };
        // SAFETY: a valid local WAVEFORMATEXTENSIBLE.
        let f = unsafe { parse_format(&ext as *const _ as *const WaveFormatEx) }.unwrap();
        assert_eq!((f.kind, f.channels, f.rate), (SampleKind::F32, 8, 44_100));
        let pcm = WaveFormatEx { format_tag: WAVE_FORMAT_PCM, channels: 2, samples_per_sec: 48_000, avg_bytes_per_sec: 0, block_align: 4, bits_per_sample: 16, cb_size: 0 };
        // SAFETY: as above.
        assert_eq!(unsafe { parse_format(&pcm) }.unwrap().kind, SampleKind::I16);
        let bad = WaveFormatEx { block_align: 3, ..pcm };
        // SAFETY: as above.
        assert_eq!(unsafe { parse_format(&bad) }.unwrap_err().code, ErrorCode::DeviceFormat);
        let adpcm = WaveFormatEx { format_tag: 2, ..pcm };
        // SAFETY: as above.
        assert_eq!(unsafe { parse_format(&adpcm) }.unwrap_err().code, ErrorCode::DeviceFormat);
    }
}

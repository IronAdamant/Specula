//! Window smoke test: a bare Win32 window, a spinning lit cube presented through the swapchain,
//! one programmatic resize and one minimize/restore, then exit. Prints any validation messages and
//! exits non-zero on validation errors.
//!
//! `cargo run -p specula-gpu --example present -- [--adapter N] [--frames N] [--features]`
//!
//! `--features` adds a ground plane and turns on the M2 features in stages while the window is
//! resized and minimized: shadows, sky and fog from the start, then bloom, then FXAA with bloom
//! levels changed, plus a textured overlay panel; the window's post images are recreated on the
//! way.
//!
//! The Win32 calls are declared here on purpose (raw-dylib `user32`): specula-gpu must not depend
//! on specula-platform, and nothing this example links imports user32. `kernel32` comes from
//! `specula-sys`, its only owner.

#[cfg(not(windows))]
fn main() {
    eprintln!("present: this example needs Windows (Win32 window + VK_KHR_win32_surface)");
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use specula_gpu::{
        AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, GpuOptions, OverlayKind, OverlayQuad, Severity, TextureFormat,
        Validation, Vertex,
    };
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    type Hwnd = *mut c_void;

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        wnd_proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: *mut c_void,
        icon: *mut c_void,
        cursor: *mut c_void,
        background: *mut c_void,
        menu_name: *const u16,
        class_name: *const u16,
        icon_small: *mut c_void,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32", kind = "raw-dylib")]
    extern "system" {
        fn RegisterClassExW(class: *const WndClassExW) -> u16;
        fn UnregisterClassW(class_name: *const u16, instance: *mut c_void) -> i32;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> Hwnd;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn PeekMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    }

    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
    const PM_REMOVE: u32 = 1;
    const WM_DESTROY: u32 = 0x0002;
    const WM_SIZE: u32 = 0x0005;
    const WM_CLOSE: u32 = 0x0010;
    const SWP_NOMOVE: u32 = 0x2;
    const SWP_NOZORDER: u32 = 0x4;
    const SW_MINIMIZE: i32 = 6;
    const SW_RESTORE: i32 = 9;

    static WIDTH: AtomicU32 = AtomicU32::new(0);
    static HEIGHT: AtomicU32 = AtomicU32::new(0);
    static CLOSED: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn wnd_proc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
        match msg {
            WM_SIZE => {
                WIDTH.store((lparam & 0xffff) as u32, Ordering::Relaxed);
                HEIGHT.store(((lparam >> 16) & 0xffff) as u32, Ordering::Relaxed);
                0
            }
            WM_CLOSE | WM_DESTROY => {
                CLOSED.store(true, Ordering::Relaxed);
                0
            }
            // SAFETY: forwarding the unmodified message to the default window procedure.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn pump() {
        let mut msg = Msg { hwnd: ptr::null_mut(), message: 0, wparam: 0, lparam: 0, time: 0, pt: [0; 2], private: 0 };
        // SAFETY: `msg` is a valid MSG out-struct; messages for this thread's windows only.
        unsafe {
            while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    // --- tiny column-major math (specula-core math is implemented separately) ---

    fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
        let mut o = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
            }
        }
        o
    }

    fn perspective_vk(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
        let f = 1.0 / (fov_y / 2.0).tan();
        let d = near - far;
        [f / aspect, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, far / d, -1.0, 0.0, 0.0, near * far / d, 0.0]
    }

    /// Camera at (0, 1.5, 3.5) looking at the origin.
    fn view() -> [f32; 16] {
        let eye = [0.0f32, 1.5, 3.5];
        let len = (eye[1] * eye[1] + eye[2] * eye[2]).sqrt();
        let f = [0.0, -eye[1] / len, -eye[2] / len]; // forward
        let s = [1.0, 0.0, 0.0]; // right = f × up (normalized)
        let u = [0.0, -f[2], f[1]]; // up = s × f
        let d = |a: [f32; 3]| a[0] * eye[0] + a[1] * eye[1] + a[2] * eye[2];
        [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0, -d(s), -d(u), d(f), 1.0]
    }

    fn rotation(yaw: f32, pitch: f32) -> [f32; 16] {
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let ry = [cy, 0.0, -sy, 0.0, 0.0, 1.0, 0.0, 0.0, sy, 0.0, cy, 0.0, 0.0, 0.0, 0.0, 1.0];
        let rx = [1.0, 0.0, 0.0, 0.0, 0.0, cp, sp, 0.0, 0.0, -sp, cp, 0.0, 0.0, 0.0, 0.0, 1.0];
        mul(&ry, &rx)
    }

    fn cube() -> (Vec<Vertex>, Vec<u32>) {
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        let (mut v, mut i) = (Vec::new(), Vec::new());
        for (n, a, b) in faces {
            let base = v.len() as u32;
            for (sa, sb) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = |k: usize| 0.5 * (n[k] + sa * a[k] + sb * b[k]);
                v.push(Vertex::new([p(0), p(1), p(2)], n));
            }
            i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        (v, i)
    }

    fn arg(name: &str) -> Option<usize> {
        let args: Vec<String> = std::env::args().collect();
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
    }

    pub fn run() -> i32 {
        if !specula_gpu::is_available() {
            eprintln!("present: Vulkan is not available; nothing to do");
            return 0;
        }
        let frames = arg("--frames").unwrap_or(120);
        let features = std::env::args().any(|a| a == "--features");
        let adapter = arg("--adapter").map_or(AdapterPreference::HighPerformance, AdapterPreference::Index);

        let class_name = specula_sys::wide("SpeculaGpuPresentExample");
        let title = specula_sys::wide("specula-gpu present");
        // SAFETY: null = the module that created the process.
        let hinstance = unsafe { specula_sys::kernel32::GetModuleHandleW(ptr::null()) };
        let class = WndClassExW {
            cb_size: std::mem::size_of::<WndClassExW>() as u32,
            style: 0,
            wnd_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance: hinstance,
            icon: ptr::null_mut(),
            cursor: ptr::null_mut(),
            background: ptr::null_mut(),
            menu_name: ptr::null(),
            class_name: class_name.as_ptr(),
            icon_small: ptr::null_mut(),
        };
        // SAFETY: `class` and its strings are valid for the call.
        if unsafe { RegisterClassExW(&class) } == 0 {
            eprintln!("present: RegisterClassExW failed");
            return 1;
        }
        // SAFETY: registered class name, valid title, default position; no parent/menu.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                800,
                600,
                ptr::null_mut(),
                ptr::null_mut(),
                hinstance,
                ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            eprintln!("present: CreateWindowExW failed");
            return 1;
        }
        pump();
        let mut rect = Rect::default();
        // SAFETY: live window, valid out-struct.
        unsafe { GetClientRect(hwnd, &mut rect) };
        let (mut w, mut h) = ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32);

        let code = match render_loop(hinstance, hwnd, adapter, frames, features, &mut w, &mut h) {
            Ok(errors) => {
                if errors == 0 {
                    0
                } else {
                    1
                }
            }
            Err(e) => {
                eprintln!("present: {e}");
                1
            }
        };
        // SAFETY: our window and class; the Gpu (and its surface) was dropped in render_loop.
        unsafe {
            DestroyWindow(hwnd);
            UnregisterClassW(class_name.as_ptr(), hinstance);
        }
        code
    }

    /// Returns the number of validation errors plus failed statistics checks.
    fn render_loop(
        hinstance: *mut c_void,
        hwnd: Hwnd,
        adapter: AdapterPreference,
        frames: usize,
        features: bool,
        w: &mut u32,
        h: &mut u32,
    ) -> Result<usize, specula_gpu::GpuError> {
        let mut gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter, ..Default::default() })?;
        println!("present: {} — validation {}", gpu.adapter().name, if gpu.validation_enabled() { "on" } else { "off" });
        let (vertices, indices) = cube();
        let mesh = gpu.upload_mesh(&vertices, &indices)?;
        // A 2×2 checker texture for the overlay.
        let texture = gpu.upload_texture(2, 2, TextureFormat::R8, &[255, 64, 64, 255])?;
        gpu.attach_window(hinstance, hwnd, *w, *h)?;
        println!("present: attached {w}x{h}");
        // Loading is done: every pipeline the loop needs must exist already.
        gpu.mark_warm();

        let start = std::time::Instant::now();
        let mut presented = 0usize;
        let mut sizes = vec![(*w, *h)];
        for frame in 0..frames {
            match frame {
                // Programmatic resize: WM_SIZE arrives during SetWindowPos.
                f if f == frames / 2 => {
                    // SAFETY: live window; size-only change.
                    unsafe { SetWindowPos(hwnd, ptr::null_mut(), 0, 0, 640, 480, SWP_NOMOVE | SWP_NOZORDER) };
                }
                f if f == frames * 3 / 4 => {
                    // SAFETY: live window.
                    unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
                }
                f if f == frames * 3 / 4 + 5 => {
                    // SAFETY: live window.
                    unsafe { ShowWindow(hwnd, SW_RESTORE) };
                }
                _ => {}
            }
            pump();
            if CLOSED.load(Ordering::Relaxed) {
                break;
            }
            let (nw, nh) = (WIDTH.load(Ordering::Relaxed), HEIGHT.load(Ordering::Relaxed));
            if (nw, nh) != (*w, *h) {
                (*w, *h) = (nw, nh);
                gpu.resize_window(nw, nh)?;
                sizes.push((nw, nh));
            }
            let t = frame as f32 / 60.0;
            let aspect = if *h > 0 { *w as f32 / *h as f32 } else { 1.0 };
            let view_proj = mul(&perspective_vk(50f32.to_radians(), aspect, 0.1, 100.0), &view());
            let mut ground = [0.0f32; 16];
            (ground[0], ground[5], ground[10], ground[13], ground[15]) = (6.0, 0.05, 6.0, -0.8, 1.0);
            let mut glow = DrawItem::new(mesh, rotation(t * 1.5, 0.35), [0.85, 0.55, 0.25, 1.0]);
            if features {
                glow.surface.emissive = 1.5;
            }
            let all = [glow, DrawItem::new(mesh, ground, [0.4, 0.45, 0.5, 1.0])];
            let draws = if features { &all[..] } else { &all[..1] };
            let mut environment = Environment::default();
            let mut overlay = Vec::new();
            if features {
                // Stages: shadows, sky and fog throughout; bloom from a quarter in; FXAA (and
                // fewer bloom levels) from the resize on.
                environment.shadow.enabled = true;
                environment.sky.enabled = true;
                environment.fog.enabled = true;
                environment.tonemap = true;
                environment.bloom.enabled = frame >= frames / 4;
                environment.fxaa = frame >= frames / 2;
                environment.bloom.levels = if frame >= frames / 2 { 3 } else { 5 };
                overlay.push(OverlayQuad { radius: 8.0, ..OverlayQuad::solid([16.0, 16.0, 220.0, 60.0], [0.17, 0.19, 0.22, 0.85]) });
                overlay.push(OverlayQuad::textured([28.0, 28.0, 36.0, 36.0], [0.0, 0.0, 1.0, 1.0], texture, OverlayKind::AlphaMask, [0.93, 0.9, 0.84, 1.0]));
            }
            gpu.present(&FrameDesc {
                view_proj,
                clear_color: [0.10, 0.20, 0.30, 1.0],
                draws,
                reverse_z: false,
                environment,
                overlay: &overlay,
                ..Default::default()
            })?;
            if *w > 0 && *h > 0 {
                presented += 1;
            }
        }
        let elapsed = start.elapsed();
        println!(
            "present: {presented} frames in {:.2}s ({:.1} fps), client sizes {sizes:?}",
            elapsed.as_secs_f64(),
            presented as f64 / elapsed.as_secs_f64()
        );
        // Window-path statistics: GPU timings come from an earlier, completed frame.
        let mut problems = 0;
        match gpu.last_frame_stats_of(specula_gpu::FramePath::Window) {
            None => {
                println!("present: PROBLEM no window frame statistics");
                problems += 1;
            }
            Some(s) => {
                let gpu_t = s.gpu.as_ref().map(|g| (g.frame, g.total_ms));
                println!(
                    "present: frame {} {}x{}: gpu {gpu_t:?} ms, cpu wait {:.3} record {:.3} submit {:.3} ms, {} draw call(s), pipelines {} ({} after warm)",
                    s.frame, s.width, s.height, s.cpu.wait_ms, s.cpu.record_ms, s.cpu.submit_ms, s.draws.draw_calls,
                    s.pipelines.created, s.pipelines.created_after_warm
                );
                let lag_ok = s.gpu.as_ref().is_some_and(|g| g.frame < s.frame && g.frame + 4 >= s.frame && g.total_ms > 0.0);
                if s.timestamps_supported && !lag_ok {
                    println!("present: PROBLEM window GPU timings missing or stale");
                    problems += 1;
                }
                // With --features the ground is the same mesh, so it joins the cube's instanced call.
                if s.pipelines.created_after_warm != 0 || s.draws.draw_calls != 1 {
                    println!("present: PROBLEM pipelines created after warm-up or wrong draw count");
                    problems += 1;
                }
            }
        }
        gpu.destroy_mesh(mesh);
        gpu.destroy_texture(texture);
        let messages = gpu.close(); // includes teardown messages
        let errors = messages.iter().filter(|m| m.severity == Severity::Error).count();
        for m in &messages {
            println!("present: [{:?}] {} {}", m.severity, m.id_name, m.message);
        }
        println!("present: {} validation message(s), {errors} error(s)", messages.len());
        Ok(errors + problems)
    }
}

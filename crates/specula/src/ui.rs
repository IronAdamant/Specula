//! The human's window, on its own thread.
//!
//! Win32 runs a modal loop while a window is dragged or resized, blocking whichever thread pumps
//! its messages. Pumping on a dedicated thread keeps the engine thread rendering and answering
//! agents meanwhile. The engine thread presents to the window through its raw handles and reads
//! its size with [`specula_platform::client_size_of`]; the window itself never leaves this thread.
//! Input events are forwarded to the engine thread, which drains them once per frame.

use specula_platform::{Event, RawWindow, Window, WindowDesc};
use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the window thread pumps messages when idle.
const PUMP_INTERVAL: Duration = Duration::from_millis(8);

enum ToUi {
    SetTitle(String),
    Close,
}

pub struct UiThread {
    pub raw: RawWindow,
    close_requests: Receiver<()>,
    /// A close was requested (checked many times per frame, so it is remembered).
    closing: Cell<bool>,
    /// Keyboard, mouse and focus events, in order.
    events: Receiver<Event>,
    to_ui: Sender<ToUi>,
    handle: Option<JoinHandle<()>>,
}

impl UiThread {
    /// Create the window on a new thread; returns once it exists (or failed to).
    pub fn spawn(desc: WindowDesc) -> Result<UiThread, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<RawWindow, String>>();
        let (close_tx, close_requests) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let (to_ui, from_engine) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("specula-window".into())
            .spawn(move || {
                let mut window = match Window::new(&desc) {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(window.raw()));
                pump(&mut window, &close_tx, &events_tx, &from_engine);
                // `window` drops here, on its own thread, after the engine released its surface.
            })
            .map_err(|e| format!("cannot start the window thread: {e}"))?;
        match ready_rx.recv() {
            Ok(Ok(raw)) => Ok(UiThread { raw, close_requests, closing: Cell::new(false), events, to_ui, handle: Some(handle) }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err("the window thread exited during start-up".into()),
        }
    }

    /// True once the user asked to close the window (close button, Alt+F4), and from then on.
    pub fn close_requested(&self) -> bool {
        if !self.closing.get() && self.close_requests.try_recv().is_ok() {
            self.closing.set(true);
        }
        self.closing.get()
    }

    /// Input events since the last call, oldest first. Appends to `out` so a frame can reuse it.
    pub fn drain_events(&self, out: &mut Vec<Event>) {
        out.extend(self.events.try_iter());
    }

    pub fn set_title(&self, title: &str) {
        let _ = self.to_ui.send(ToUi::SetTitle(title.to_owned()));
    }

    /// Destroy the window and join its thread. Call after the GPU surface is gone.
    pub fn close(mut self) {
        let _ = self.to_ui.send(ToUi::Close);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for UiThread {
    fn drop(&mut self) {
        let _ = self.to_ui.send(ToUi::Close);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn pump(window: &mut Window, close_tx: &Sender<()>, events_tx: &Sender<Event>, from_engine: &Receiver<ToUi>) {
    loop {
        for event in window.poll_events() {
            match event {
                Event::CloseRequested => {
                    let _ = close_tx.send(());
                }
                // The engine reads the size directly (it stays current inside modal loops).
                Event::Resized { .. } => {}
                other => {
                    let _ = events_tx.send(other);
                }
            }
        }
        loop {
            match from_engine.try_recv() {
                Ok(ToUi::SetTitle(title)) => window.set_title(&title),
                Ok(ToUi::Close) | Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        std::thread::sleep(PUMP_INTERVAL);
    }
}

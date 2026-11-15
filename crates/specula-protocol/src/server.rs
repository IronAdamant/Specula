//! Sockets and threads: an accept thread plus one thread per connection.
//!
//! Shutdown: set `stopping`, self-connect to wake the blocked `accept`, then the accept thread
//! waits for every request that is running to be answered, shuts down every open connection
//! socket (unblocking their reads) and joins the workers.
//!
//! A request that began before the stop is answered: what its tool did was done, and its
//! caller is told. (The sockets were shut first, and the workers joined after: the server
//! waited for a running call all the same and then had nowhere to write what it said. A call
//! refused because the engine's start failed lost its refusal that way whenever its thread
//! was the slower of the two.) A request that is whole only after the stop is not begun,
//! and is told so (503) where its socket still takes it.
//! The stop waits for a handler as long as it takes, which is what joining its thread always
//! waited, and for the write of an answer no longer than [`STOP_WRITE_TIMEOUT`]: a caller
//! that does not read does not hold a stopping server.

use crate::http::{self, ReadError, Request, Response};
use crate::mcp::{self, State};
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Largest accepted request body.
pub(crate) const MAX_BODY: usize = 16 * 1024 * 1024;
/// A connection with no request bytes for this long is closed (keeps threads from hanging).
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Granularity at which blocked reads re-check the shutdown flag.
const POLL_STEP: Duration = Duration::from_millis(100);
/// A client that stops reading its response for this long is dropped.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a stopping server waits for answers to be written, once every handler has
/// returned. An answer goes into the socket at once unless its reader has stopped reading
/// (on Windows the loopback took 12 MB unread), so this is the wait for a caller that does not
/// read; then its socket is shut, as every socket was before answers were waited for.
const STOP_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an erroring connection keeps draining client input before closing.
const LINGER: Duration = Duration::from_secs(2);
const LINGER_STEP: Duration = Duration::from_millis(250);
/// Concurrent connections beyond this are answered 503 and closed.
const MAX_CONNECTIONS: usize = 256;
/// Rejected connections waiting for their 503 (beyond this they are closed outright).
const REJECT_QUEUE: usize = 64;
/// How long a rejected connection's close lingers while the client finishes sending.
const REJECT_LINGER: Duration = Duration::from_millis(500);

pub(crate) struct Shared {
    pub state: State,
    /// (Set with `running` held, so that a request either began before the stop and is
    /// counted, or does not begin.)
    stopping: AtomicBool,
    /// Requests being answered now: (with the handler, having their answer written).
    running: Mutex<(usize, usize)>,
    /// A request moved on (to its write, or out).
    moved: Condvar,
    /// Socket handles of live connections, so shutdown can unblock their reads.
    conns: Mutex<HashMap<u64, TcpStream>>,
}

impl Shared {
    pub fn new(state: State) -> Shared {
        Shared { state, stopping: AtomicBool::new(false), running: Mutex::new((0, 0)), moved: Condvar::new(), conns: Mutex::new(HashMap::new()) }
    }

    fn running(&self) -> std::sync::MutexGuard<'_, (usize, usize)> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A request begins: counted until its answer is written (the value's drop). `None` once
    /// the server is stopping: the request is not begun.
    fn begin(&self) -> Option<Running<'_>> {
        let mut running = self.running();
        if self.stopping.load(Ordering::SeqCst) {
            return None;
        }
        running.0 += 1;
        Some(Running { shared: self, writing: false })
    }

    /// The stop: no request begins from here on.
    fn stop_beginning(&self) {
        let _held = self.running();
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// Waits until every request that began is answered: for its handler as long as that
    /// takes, then for the writes at most `writes`.
    fn wait_answered(&self, writes: Duration) {
        let mut running = self.running();
        while running.0 > 0 {
            running = self.moved.wait(running).unwrap_or_else(|e| e.into_inner());
        }
        let since = Instant::now();
        while running.1 > 0 {
            let Some(left) = writes.checked_sub(since.elapsed()).filter(|d| !d.is_zero()) else { break };
            running = self.moved.wait_timeout(running, left).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    fn conns(&self) -> std::sync::MutexGuard<'_, HashMap<u64, TcpStream>> {
        self.conns.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub(crate) fn spawn_accept(listener: TcpListener, shared: Arc<Shared>) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new().name("specula-mcp-accept".into()).spawn(move || accept_loop(listener, shared))
}

/// Stops the server whose accept thread is `accept`, listening on `addr`.
pub(crate) fn stop(shared: &Shared, addr: SocketAddr, accept: JoinHandle<()>) {
    shared.stop_beginning();
    // Wake the blocked accept(); retry briefly in case the backlog is momentarily full.
    for _ in 0..50 {
        if accept.is_finished() || TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = accept.join();
    shared.state.clear_sessions();
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    let mut next_id: u64 = 0;
    // Over the limit, connections get their 503 on a separate thread: answering properly means
    // reading what the client already sent (or closing resets the connection and the client sees
    // an error instead of the 503), and that must not hold up accepting.
    let (reject, rejected) = mpsc::sync_channel::<TcpStream>(REJECT_QUEUE);
    let rejecter = thread::Builder::new()
        .name("specula-mcp-reject".into())
        .spawn({
            let shared = Arc::clone(&shared);
            // Shutting down: queued connections are just closed.
            move || rejected.into_iter().for_each(|stream| answer_busy(&shared.stopping, stream))
        })
        .ok();
    for incoming in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            break;
        }
        let stream = match incoming {
            Ok(stream) => stream,
            Err(e) => {
                // Transient (ECONNABORTED, EMFILE…): back off instead of spinning.
                eprintln!("specula-protocol: accept failed: {e}");
                thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        workers.retain(|h| !h.is_finished());
        if workers.len() >= MAX_CONNECTIONS {
            if let Err(TrySendError::Full(stream) | TrySendError::Disconnected(stream)) = reject.try_send(stream) {
                // Swamped: answer without lingering.
                let _ = (&stream).write_all(&busy_response().to_bytes(false));
            }
            continue;
        }
        let Ok(handle) = stream.try_clone() else { continue };
        let id = next_id;
        next_id += 1;
        shared.conns().insert(id, handle);
        let worker_shared = Arc::clone(&shared);
        let spawned = thread::Builder::new().name(format!("specula-mcp-conn-{id}")).spawn(move || {
            let _guard = ConnGuard { shared: &worker_shared, id };
            serve_connection(&worker_shared, stream);
        });
        match spawned {
            Ok(h) => workers.push(h),
            Err(e) => {
                eprintln!("specula-protocol: could not spawn connection thread: {e}");
                shared.conns().remove(&id);
            }
        }
    }
    drop(reject);
    if let Some(r) = rejecter {
        let _ = r.join();
    }
    // Stopping. The requests that are running are answered first; then every connection
    // thread is unblocked (a write to a caller that is not reading with them) and waited for.
    shared.wait_answered(STOP_WRITE_TIMEOUT);
    let open: Vec<TcpStream> = shared.conns().drain().map(|(_, s)| s).collect();
    for s in open {
        let _ = s.shutdown(Shutdown::Both);
    }
    for h in workers {
        let _ = h.join();
    }
}

fn busy_response() -> Response {
    mcp::error_response(503, -32000, "Service Unavailable: too many connections; retry shortly").header("Retry-After", "1")
}

/// What a request that was whole only after the stop began is told, where it can still be
/// told (review R3: it was closed on with nothing said): it was not begun.
fn stopping_response() -> Response {
    mcp::error_response(503, -32000, "Service Unavailable: the server is stopping; the request was not begun and nothing was applied")
}

/// Answers a connection over the limit with 503, then closes it gently (see [`send_http_error`]).
fn answer_busy(stopping: &AtomicBool, stream: TcpStream) {
    if stopping.load(Ordering::SeqCst) {
        return;
    }
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    // Give the request a moment to arrive, so the response follows it rather than racing it.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let mut head = [0u8; 16 * 1024];
    let _ = (&stream).read(&mut head);
    if !send(&stream, &busy_response(), false) {
        return;
    }
    linger(&stream, REJECT_LINGER, Some(stopping));
}

/// A request that began and is not yet answered ([`Shared::begin`]): with its handler, then
/// (after [`Running::answered`]) having its answer written.
struct Running<'a> {
    shared: &'a Shared,
    writing: bool,
}

impl Running<'_> {
    /// The handler returned: what is left is the write. (One step under the lock: a request
    /// is in one count or the other, never in neither.)
    fn answered(&mut self) {
        let mut running = self.shared.running();
        running.0 -= 1;
        running.1 += 1;
        self.writing = true;
        self.shared.moved.notify_all();
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        let mut running = self.shared.running();
        if self.writing {
            running.1 -= 1;
        } else {
            running.0 -= 1;
        }
        self.shared.moved.notify_all();
    }
}

/// Deregisters a connection when its thread ends, even by panic.
struct ConnGuard<'a> {
    shared: &'a Shared,
    id: u64,
}

impl Drop for ConnGuard<'_> {
    fn drop(&mut self) {
        self.shared.conns().remove(&self.id);
    }
}

/// The connection's read side. Reads poll in short steps so a blocked read notices shutdown
/// (on Windows, `shutdown()` from another thread does not wake a blocked `recv`) and so a
/// silent peer is dropped after [`READ_TIMEOUT`].
struct ConnReader<'a> {
    stream: TcpStream,
    shared: &'a Shared,
}

impl Read for ConnReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = Instant::now();
        loop {
            if self.shared.stopping.load(Ordering::SeqCst) {
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "server shutting down"));
            }
            match self.stream.read(buf) {
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {
                    if start.elapsed() >= READ_TIMEOUT {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "connection idle"));
                    }
                }
                other => return other,
            }
        }
    }
}

fn serve_connection(shared: &Shared, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(POLL_STEP));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(ConnReader { stream, shared });
    while !shared.stopping.load(Ordering::SeqCst) {
        let head = match http::read_head(&mut reader) {
            Ok(Some(head)) => head,
            Ok(None) | Err(ReadError::Closed) => return,
            Err(ReadError::Http(e)) => return send_http_error(&reader.get_ref().stream, e),
        };
        let framing = match http::framing(&head, MAX_BODY) {
            Ok(f) => f,
            Err(e) => return send_http_error(&reader.get_ref().stream, e),
        };
        let stream = &reader.get_ref().stream;
        if head.expects_continue() && framing != http::Framing::Length(0) {
            let mut w: &TcpStream = stream;
            if w.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
                return;
            }
        }
        let body = match http::read_body(&mut reader, framing, MAX_BODY) {
            Ok(body) => body,
            Err(ReadError::Closed) => return,
            Err(ReadError::Http(e)) => return send_http_error(&reader.get_ref().stream, e),
        };
        let req = Request { head, body };
        // (Whole only after the stop: not begun, nothing was applied. It is told so if its
        // socket still takes the words, and the connection closes.)
        let Some(mut running) = shared.begin() else {
            send(&reader.get_ref().stream, &stopping_response(), false);
            return;
        };
        let response = shared.state.handle(&req);
        running.answered();
        let keep_alive = req.head.keep_alive() && !shared.stopping.load(Ordering::SeqCst);
        let sent = send(&reader.get_ref().stream, &response, keep_alive);
        drop(running);
        if !sent || !keep_alive {
            return;
        }
    }
}

fn send(mut stream: &TcpStream, response: &Response, keep_alive: bool) -> bool {
    stream.write_all(&response.to_bytes(keep_alive)).and_then(|_| stream.flush()).is_ok()
}

/// Answers a framing-level error and closes: the rest of the stream cannot be trusted.
///
/// The close "lingers": after the response we half-close and discard what the client is still
/// sending (e.g. the rest of an oversized body) for a moment. Closing a socket with unread input
/// makes the OS send RST, which can destroy the response before the client reads it.
fn send_http_error(stream: &TcpStream, e: http::HttpError) {
    let resp = mcp::error_response(e.status, -32000, format!("{}: {}", http::reason(e.status), e.message));
    if !send(stream, &resp, false) {
        return;
    }
    linger(stream, LINGER, None);
}

/// Half-closes `stream` and discards what the client still sends for up to `how_long`, so the
/// final close doesn't reset the connection before the client has read the response.
fn linger(mut stream: &TcpStream, how_long: Duration, stopping: Option<&AtomicBool>) {
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(LINGER_STEP));
    let deadline = Instant::now() + how_long;
    let mut sink = [0u8; 16 * 1024];
    while Instant::now() < deadline && !stopping.is_some_and(|s| s.load(Ordering::SeqCst)) {
        match stream.read(&mut sink) {
            Ok(0) => break,
            Err(e) if !matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServerConfig, SessionInfo, ToolDef, ToolHandler, ToolOutput};
    use specula_core::Json;

    struct NoTools;

    impl ToolHandler for NoTools {
        fn tools(&self) -> Vec<ToolDef> {
            Vec::new()
        }

        fn call(&self, _: &SessionInfo, _: &str, _: &Json) -> ToolOutput {
            ToolOutput::text("")
        }
    }

    fn shared() -> Arc<Shared> {
        let config = ServerConfig { bind: "127.0.0.1:0".into(), server_name: "t".into(), server_version: "0".into(), instructions: None };
        Arc::new(Shared::new(State::new(config, Arc::new(NoTools))))
    }

    /// What a stop waits for: a request with its handler, without limit (here until the
    /// request is let go, however long the wait was told to give writes); a request whose
    /// answer is being written, the limit it was given and no longer; and nothing that had not
    /// begun when the stop came.
    #[test]
    fn a_stop_waits_for_handlers_and_gives_writes_their_limit() {
        let s = shared();
        let handled = s.begin().expect("the server runs: a request begins");
        s.stop_beginning();
        assert!(s.begin().is_none(), "nothing begins once the server is stopping");
        // With its handler: the wait ends when the request does, not at the writes' limit.
        let (done_tx, done) = mpsc::channel();
        let waiter = thread::spawn({
            let s = Arc::clone(&s);
            move || {
                s.wait_answered(Duration::ZERO);
                done_tx.send(()).unwrap();
            }
        });
        assert!(done.recv_timeout(Duration::from_millis(200)).is_err(), "a handler is waited for, whatever the writes' limit");
        drop(handled);
        done.recv_timeout(Duration::from_secs(30)).expect("the wait ends when the request does");
        waiter.join().unwrap();
        // Being written: counted, and waited for the limit only.
        let s = shared();
        let mut written = s.begin().unwrap();
        written.answered();
        assert_eq!(*s.running(), (0, 1), "in one count or the other");
        let asked = Instant::now();
        s.wait_answered(Duration::from_millis(50));
        assert!(asked.elapsed() >= Duration::from_millis(50), "the write was given its limit: {:?}", asked.elapsed());
        drop(written);
        assert_eq!(*s.running(), (0, 0));
        // Nothing running: no wait at all, whatever the limit.
        s.wait_answered(Duration::from_secs(3600));
        // What a request that came too late is told.
        let late = stopping_response();
        assert_eq!(late.status, 503);
        assert!(String::from_utf8_lossy(&late.body).contains("not begun"), "{}", String::from_utf8_lossy(&late.body));
    }
}

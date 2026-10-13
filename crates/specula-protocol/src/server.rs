//! Sockets and threads: an accept thread plus one thread per connection.
//!
//! Shutdown: set `stopping`, self-connect to wake the blocked `accept`, then the accept thread
//! shuts down every open connection socket (unblocking their reads) and joins the workers.

use crate::http::{self, ReadError, Request, Response};
use crate::mcp::{self, State};
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
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
    stopping: AtomicBool,
    /// Socket handles of live connections, so shutdown can unblock their reads.
    conns: Mutex<HashMap<u64, TcpStream>>,
}

impl Shared {
    pub fn new(state: State) -> Shared {
        Shared { state, stopping: AtomicBool::new(false), conns: Mutex::new(HashMap::new()) }
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
    shared.stopping.store(true, Ordering::SeqCst);
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
    // Stopping: unblock every connection thread, then wait for them.
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
        let response = shared.state.handle(&req);
        let keep_alive = req.head.keep_alive() && !shared.stopping.load(Ordering::SeqCst);
        if !send(&reader.get_ref().stream, &response, keep_alive) || !keep_alive {
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

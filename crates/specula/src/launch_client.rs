//! A small MCP client for the command line (docs/design/LAUNCH.md): `specula call`, `specula
//! tools` and `specula open` speak to a running engine with it, so an agent that has a shell and
//! no MCP client of its own can use every tool.
//!
//! It speaks what the engine's own server serves (`specula-protocol`): HTTP/1.1 on the loopback
//! address only, one request per connection, a JSON-RPC message per request, the session id in
//! `Mcp-Session-Id`. Standard library only. Every read is bounded: an answer larger than
//! [`MAX_ANSWER_BYTES`] is refused, and a silent server is given up on after the caller's wait.

use specula_core::Json;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// The largest answer read (the engine's own answers have ceilings far below this).
pub const MAX_ANSWER_BYTES: usize = 32 * 1024 * 1024;
/// The MCP revision asked for (the server answers with the one it speaks).
const PROTOCOL: &str = "2025-11-25";

/// Why a call to the engine failed before it had an answer.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientError {
    /// `no_engine` (nothing answers on the port), `timeout`, `bad_answer` (not HTTP, not JSON,
    /// too large), `http` (a status that isn't 200 or 202), `rpc` (a JSON-RPC error).
    pub code: &'static str,
    pub message: String,
}

impl ClientError {
    fn new(code: &'static str, message: impl Into<String>) -> ClientError {
        ClientError { code, message: message.into() }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))])
    }
}

/// A tool's answer: its text (the tool's JSON, as text) and whether the tool called it an error.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolAnswer {
    pub text: String,
    pub is_error: bool,
}

/// A session with an engine on this machine.
pub struct Client {
    port: u16,
    /// Asks for every tool (`?tools=all`), as the repo's scripts do, or the server's default.
    all_tools: bool,
    session: Option<String>,
    next_id: u64,
    wait: Duration,
}

/// One HTTP exchange's answer.
struct Http {
    status: u16,
    session: Option<String>,
    body: Vec<u8>,
}

/// Splits a whole HTTP/1.1 response (the connection was read to its end, or to the body's
/// declared length) into its status, its session header and its body.
fn parse_response(raw: &[u8]) -> Result<Http, ClientError> {
    let bad = |why: &str| ClientError::new("bad_answer", format!("the engine's answer is not HTTP as its server speaks it ({why})"));
    let head_end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| bad("no end of headers"))?;
    let head = std::str::from_utf8(&raw[..head_end]).map_err(|_| bad("headers are not text"))?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line.strip_prefix("HTTP/1.").and_then(|r| r.split(' ').nth(1)).and_then(|s| s.parse::<u16>().ok()).ok_or_else(|| bad("no status line"))?;
    let (mut session, mut length) = (None, None);
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("mcp-session-id") {
            session = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.parse::<usize>().map_err(|_| bad("a Content-Length that is not a number"))?);
        }
    }
    let body = &raw[head_end + 4..];
    let body = match length {
        Some(n) if n <= body.len() => &body[..n],
        Some(n) => return Err(bad(&format!("the body is {} bytes of the {n} it declares", body.len()))),
        None => body,
    };
    Ok(Http { status, session, body: body.to_vec() })
}

/// The JSON-RPC message of an answer's body: the body itself, or (a server may answer as an
/// event stream) the last `data:` line that carries an id.
fn rpc_message(body: &[u8]) -> Result<Json, ClientError> {
    let text = std::str::from_utf8(body).map_err(|_| ClientError::new("bad_answer", "the engine's answer is not UTF-8"))?;
    fn not_json(e: impl std::fmt::Display) -> ClientError {
        ClientError::new("bad_answer", format!("the engine's answer is not JSON ({e})"))
    }
    let first = text.trim_start();
    if !(first.starts_with("event:") || first.starts_with("data:") || first.starts_with("id:") || first.starts_with(':')) {
        return Json::parse(text).map_err(not_json);
    }
    let mut last = None;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data:").map(str::trim).filter(|d| !d.is_empty()) {
            let j = Json::parse(data).map_err(not_json)?;
            if j.get("id").is_some() || last.is_none() {
                last = Some(j);
            }
        }
    }
    last.ok_or_else(|| ClientError::new("bad_answer", "the engine's event stream carried no message"))
}

impl Client {
    /// A client for the engine on `port` of this machine. `wait`: how long one call may take.
    /// Nothing is sent until the first request ([`Client::connect`]).
    pub fn new(port: u16, all_tools: bool, wait: Duration) -> Client {
        Client { port, all_tools, session: None, next_id: 1, wait }
    }

    fn exchange(&mut self, method: &str, body: Option<&str>) -> Result<Http, ClientError> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2).min(self.wait))
            .map_err(|e| ClientError::new("no_engine", format!("nothing answers on 127.0.0.1:{} ({e}): is an engine running there? `specula list` shows the ones that are", self.port)))?;
        let io = |e: std::io::Error| {
            let timed_out = matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock);
            ClientError::new(if timed_out { "timeout" } else { "bad_answer" }, format!("the engine on port {} {} ({e})", self.port, if timed_out { "did not answer in time" } else { "closed the connection" }))
        };
        stream.set_read_timeout(Some(self.wait)).map_err(io)?;
        stream.set_write_timeout(Some(self.wait)).map_err(io)?;
        let target = if self.all_tools { "/mcp?tools=all" } else { "/mcp" };
        let mut head = format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\n", self.port);
        if let Some(s) = &self.session {
            head.push_str(&format!("Mcp-Session-Id: {s}\r\n"));
        }
        let body = body.unwrap_or("");
        if method == "POST" {
            head.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body.as_bytes())).map_err(io)?;
        // One request per connection: the answer is everything up to the connection's end.
        let mut raw = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&chunk[..n]);
                    if raw.len() > MAX_ANSWER_BYTES {
                        return Err(ClientError::new("bad_answer", format!("the engine's answer is over {MAX_ANSWER_BYTES} bytes")));
                    }
                }
                // (A server that keeps the connection open after a complete answer: done.)
                Err(_) if parse_response(&raw).is_ok() => break,
                Err(e) => return Err(io(e)),
            }
        }
        let answer = parse_response(&raw)?;
        if self.session.is_none() {
            self.session = answer.session.clone();
        }
        Ok(answer)
    }

    fn post(&mut self, message: &Json) -> Result<Http, ClientError> {
        let answer = self.exchange("POST", Some(&message.to_string()))?;
        if answer.status != 200 && answer.status != 202 {
            let said = String::from_utf8_lossy(&answer.body).chars().take(300).collect::<String>();
            return Err(ClientError::new("http", format!("the engine on port {} answered HTTP {}: {said}", self.port, answer.status)));
        }
        Ok(answer)
    }

    fn request(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        let message = Json::obj([("jsonrpc", Json::from("2.0")), ("id", Json::from(id)), ("method", Json::from(method)), ("params", params)]);
        let reply = rpc_message(&self.post(&message)?.body)?;
        if let Some(e) = reply.get("error") {
            let said = e.get("message").and_then(Json::as_str).map_or_else(|| e.to_string(), str::to_owned);
            return Err(ClientError::new("rpc", format!("{method}: {said}")));
        }
        reply.get("result").cloned().ok_or_else(|| ClientError::new("bad_answer", format!("{method}: the engine's answer has no result")))
    }

    /// Opens the session (`initialize`, then `notifications/initialized`). Returns what the
    /// server says it is (`serverInfo`: its name and version).
    pub fn connect(&mut self, client_name: &str) -> Result<Json, ClientError> {
        let params = Json::obj([
            ("protocolVersion", Json::from(PROTOCOL)),
            ("capabilities", Json::Obj(Vec::new())),
            ("clientInfo", Json::obj([("name", Json::from(client_name)), ("version", Json::from(env!("CARGO_PKG_VERSION")))])),
        ]);
        let result = self.request("initialize", params)?;
        self.post(&Json::obj([("jsonrpc", Json::from("2.0")), ("method", Json::from("notifications/initialized"))]))?;
        Ok(result.get("serverInfo").cloned().unwrap_or(Json::Null))
    }

    /// Calls a tool. A tool's own refusal is an answer (`is_error`), not a [`ClientError`].
    pub fn call(&mut self, tool: &str, arguments: Json) -> Result<ToolAnswer, ClientError> {
        let result = self.request("tools/call", Json::obj([("name", Json::from(tool)), ("arguments", arguments)]))?;
        let text = result.get("content").and_then(Json::as_array).and_then(|c| c.iter().find(|c| c.get("type").and_then(Json::as_str) == Some("text"))).and_then(|c| c.get("text")).and_then(Json::as_str);
        Ok(ToolAnswer { text: text.unwrap_or("").to_owned(), is_error: result.get("isError").and_then(Json::as_bool).unwrap_or(false) })
    }

    /// The tools this session lists (`tools/list`): their definitions.
    pub fn tools(&mut self) -> Result<Vec<Json>, ClientError> {
        let result = self.request("tools/list", Json::Obj(Vec::new()))?;
        Ok(result.get("tools").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default())
    }

    /// Ends the session (its name is free at once). Failing to is nothing: the server's own
    /// sweep ends it.
    pub fn close(&mut self) {
        if self.session.is_some() {
            let _ = self.exchange("DELETE", None);
            self.session = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_answer_is_split_into_status_session_and_body() {
        let a = parse_response(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: abc123\r\nContent-Length: 7\r\nConnection: close\r\n\r\n{\"a\":1}").unwrap();
        assert_eq!((a.status, a.session.as_deref(), a.body.as_slice()), (200, Some("abc123"), b"{\"a\":1}".as_slice()));
        // Header names in any case; bytes after the declared length are not the body.
        let a = parse_response(b"HTTP/1.1 202 Accepted\r\nmcp-session-id: s\r\ncontent-length: 2\r\n\r\nokEXTRA").unwrap();
        assert_eq!((a.status, a.session.as_deref(), a.body.as_slice()), (202, Some("s"), b"ok".as_slice()));
        // No length: the body is what came before the connection closed.
        assert_eq!(parse_response(b"HTTP/1.0 404 Not Found\r\n\r\nnope").unwrap().body, b"nope");
        for bad in [&b"garbage"[..], b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort", b"HTTP/1.1 OK\r\n\r\n", b"HTTP/1.1 200 OK\r\nContent-Length: x\r\n\r\n", b"SMTP 220\r\n\r\n"] {
            assert_eq!(parse_response(bad).map(|_| ()).unwrap_err().code, "bad_answer", "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn the_rpc_message_is_the_body_or_the_event_streams_last_answer() {
        assert_eq!(rpc_message(br#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).unwrap().get("id").and_then(Json::as_u64), Some(1));
        let stream = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n\n";
        assert_eq!(rpc_message(stream).unwrap().get("id").and_then(Json::as_u64), Some(7));
        assert_eq!(rpc_message(b"not json").unwrap_err().code, "bad_answer");
        assert_eq!(rpc_message(b"event: ping\n\n").unwrap_err().code, "bad_answer");
        assert_eq!(rpc_message(&[0xff, 0xfe]).unwrap_err().code, "bad_answer");
    }

    #[test]
    fn a_port_with_no_engine_says_so_with_what_to_do() {
        // (A listener that is bound and never accepts would hang a connect on some systems; a
        // port taken from the system and released is free for the moment this test needs it,
        // and whoever takes it meanwhile is not an engine: the call fails either way.)
        let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        let mut c = Client::new(port, true, Duration::from_millis(500));
        let e = c.connect("test").unwrap_err();
        assert!(matches!(e.code, "no_engine" | "bad_answer" | "timeout" | "http"), "{e:?}");
        assert_eq!(ClientError::new("no_engine", "x").to_json().to_string(), r#"{"error":"no_engine","message":"x"}"#);
    }

    /// The client against a server speaking the engine's framing: a session id is taken from
    /// the first answer and sent on the next requests; a tool's error is an answer.
    #[test]
    fn a_session_is_opened_a_tool_called_and_the_session_closed() {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for stream in listener.incoming().take(5) {
                let mut stream = stream.unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let (mut first, mut length, mut session) = (String::new(), 0usize, None);
                reader.read_line(&mut first).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.strip_prefix("Content-Length: ") {
                        length = v.parse().unwrap();
                    }
                    if let Some(v) = line.strip_prefix("Mcp-Session-Id: ") {
                        session = Some(v.to_owned());
                    }
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let answer = if first.starts_with("DELETE") {
                    String::new()
                } else if body.contains("\"initialize\"") {
                    r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"specula","version":"9"}}}"#.to_owned()
                } else if body.contains("notifications/initialized") {
                    String::new()
                } else if body.contains("world_list") {
                    r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"entities\":0}"}]}}"#.to_owned()
                } else {
                    r#"{"jsonrpc":"2.0","id":3,"result":{"isError":true,"content":[{"type":"text","text":"{\"error\":\"unknown_tool\"}"}]}}"#.to_owned()
                };
                seen.push((first.trim_end().to_owned(), session));
                let status = if answer.is_empty() { "202 Accepted" } else { "200 OK" };
                write!(stream, "HTTP/1.1 {status}\r\nMcp-Session-Id: sess-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
            }
            seen
        });
        let mut c = Client::new(port, true, Duration::from_secs(10));
        let info = c.connect("test").unwrap();
        assert_eq!(info.get("name").and_then(Json::as_str), Some("specula"));
        let ok = c.call("world_list", Json::Obj(Vec::new())).unwrap();
        assert_eq!(ok, ToolAnswer { text: r#"{"entities":0}"#.into(), is_error: false });
        let refused = c.call("nope", Json::Null).unwrap();
        assert!(refused.is_error && refused.text.contains("unknown_tool"));
        c.close();
        let seen = server.join().unwrap();
        assert_eq!(seen[0], ("POST /mcp?tools=all HTTP/1.1".to_owned(), None), "the first request has no session yet");
        assert!(seen[1..].iter().all(|(_, s)| s.as_deref() == Some("sess-1")), "{seen:?}");
        assert_eq!(seen[4].0, "DELETE /mcp?tools=all HTTP/1.1");
    }
}

//! MCP over Streamable HTTP: request routing, JSON-RPC 2.0 framing, lifecycle and tools.
//!
//! [`State::handle`] maps one complete HTTP request to one HTTP response; it knows nothing about
//! sockets, which keeps the protocol logic testable and the connection loop small.

use crate::http::{Request, Response};
use crate::{is_valid_tool_name, Content, ServerConfig, SessionInfo, ToolHandler, ToolOutput};
use crate::{LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS};
use specula_core::Json;
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The MCP endpoint path.
pub(crate) const ENDPOINT: &str = "/mcp";

/// Open sessions beyond this evict the oldest (a local client looping on `initialize` cannot
/// grow memory without bound).
const MAX_SESSIONS: usize = 1024;

/// Sessions unused for this long are left out of [`State::sessions`] (agent listings), so agents
/// that went away without `DELETE` stop being shown. They stay valid: an agent returning after a
/// long pause carries on in the same session. (Memory stays bounded by [`MAX_SESSIONS`].)
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

// JSON-RPC 2.0 error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
/// Implementation-defined server error used for transport-level rejections (as the reference
/// TypeScript SDK does), so dual-era clients do not mistake them for modern MCP errors.
const TRANSPORT_ERROR: i64 = -32000;
/// Unknown / expired session (same code as the reference SDK).
const SESSION_NOT_FOUND: i64 = -32001;

pub(crate) struct State {
    config: ServerConfig,
    handler: Arc<dyn ToolHandler>,
    sessions: Mutex<Sessions>,
    /// Tool names already reported as invalid (warn once, not on every `tools/list`).
    warned_names: Mutex<HashSet<String>>,
    id_counter: AtomicU64,
}

#[derive(Default)]
struct Sessions {
    /// id → (creation sequence, info, last request).
    map: HashMap<String, (u64, SessionInfo, Instant)>,
    next_seq: u64,
}

/// A classified JSON-RPC message.
enum Message<'a> {
    Request { id: &'a Json, method: &'a str, params: Option<&'a Json> },
    Notification,
    Response,
}

/// Rejection of a message that is not valid JSON-RPC; `id` is echoed when it could be read.
struct Invalid {
    id: Json,
    message: String,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Handlers run under catch_unwind, so poisoning only follows a bug in this crate; the data
    // (a session map) stays consistent either way.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl State {
    pub fn new(config: ServerConfig, handler: Arc<dyn ToolHandler>) -> State {
        State {
            config,
            handler,
            sessions: Mutex::new(Sessions::default()),
            warned_names: Mutex::new(HashSet::new()),
            id_counter: AtomicU64::new(0),
        }
    }

    /// Sessions used within [`SESSION_IDLE_TIMEOUT`], oldest first.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        let sessions = lock(&self.sessions);
        let now = Instant::now();
        let mut list: Vec<&(u64, SessionInfo, Instant)> =
            sessions.map.values().filter(|(_, _, used)| now.duration_since(*used) < SESSION_IDLE_TIMEOUT).collect();
        list.sort_by_key(|(seq, _, _)| *seq);
        list.into_iter().map(|(_, info, _)| info.clone()).collect()
    }

    pub fn clear_sessions(&self) {
        lock(&self.sessions).map.clear();
    }

    /// Handles one HTTP request.
    pub fn handle(&self, req: &Request) -> Response {
        if let Some(origin) = req.header("origin") {
            if !origin_allowed(origin) {
                return error_response(
                    403,
                    TRANSPORT_ERROR,
                    format!("Forbidden: Origin {origin:?} is not a localhost origin"),
                );
            }
        }
        if req.head.path() != ENDPOINT {
            return error_response(404, TRANSPORT_ERROR, format!("Not Found: the MCP endpoint is {ENDPOINT}"));
        }
        match req.head.method.as_str() {
            "POST" => self.post(req),
            "DELETE" => self.delete(req),
            _ => error_response(
                405,
                TRANSPORT_ERROR,
                "Method Not Allowed: use POST (and DELETE to end a session); no SSE stream is offered",
            )
            .header("Allow", "POST, DELETE"),
        }
    }

    fn delete(&self, req: &Request) -> Response {
        let Some(id) = req.header("mcp-session-id") else {
            return error_response(400, TRANSPORT_ERROR, "Bad Request: DELETE needs an Mcp-Session-Id header");
        };
        match lock(&self.sessions).map.remove(id) {
            Some(_) => Response::new(200),
            None => error_response(404, SESSION_NOT_FOUND, "Session not found"),
        }
    }

    fn post(&self, req: &Request) -> Response {
        if let Some(ct) = req.header("content-type") {
            if !media_type_is(ct, "application/json") {
                return error_response(
                    415,
                    TRANSPORT_ERROR,
                    format!("Unsupported Media Type: Content-Type must be application/json, got {ct:?}"),
                );
            }
        }
        if let Some(accept) = req.header("accept") {
            if !accepts_json(accept) {
                return error_response(406, TRANSPORT_ERROR, "Not Acceptable: client must accept application/json");
            }
        }
        let Ok(text) = std::str::from_utf8(&req.body) else {
            return error_response(400, PARSE_ERROR, "Parse error: body is not valid UTF-8");
        };
        let msg = match Json::parse(text) {
            Ok(msg) => msg,
            Err(e) => return error_response(400, PARSE_ERROR, format!("Parse error: {e}")),
        };
        if msg.as_array().is_some() {
            return error_response(
                400,
                INVALID_REQUEST,
                "Invalid Request: JSON-RPC batches are not supported; send one message per POST",
            );
        }
        let message = match classify(&msg) {
            Ok(m) => m,
            Err(Invalid { id, message }) => {
                return json_response(400, &rpc_error(id, INVALID_REQUEST, &format!("Invalid Request: {message}")))
            }
        };

        if let Message::Request { id, method: "initialize", params } = message {
            return self.initialize(id, params);
        }
        let session = match self.session_for(req) {
            Ok(s) => s,
            Err(resp) => return resp,
        };
        match message {
            Message::Notification | Message::Response => Response::new(202),
            Message::Request { id, method, params } => {
                let reply = match self.dispatch(&session, method, params) {
                    Ok(result) => rpc_result(id.clone(), result),
                    Err((code, message)) => rpc_error(id.clone(), code, &message),
                };
                json_response(200, &reply)
            }
        }
    }

    /// Session and protocol-version checks for everything but `initialize`.
    fn session_for(&self, req: &Request) -> Result<SessionInfo, Response> {
        let Some(id) = req.header("mcp-session-id") else {
            return Err(error_response(
                400,
                TRANSPORT_ERROR,
                "Bad Request: Mcp-Session-Id header is required (send initialize first)",
            ));
        };
        let session = {
            let mut sessions = lock(&self.sessions);
            match sessions.map.get_mut(id) {
                Some((_, info, used)) => {
                    *used = Instant::now();
                    info.clone()
                }
                None => return Err(error_response(404, SESSION_NOT_FOUND, "Session not found")),
            }
        };
        if let Some(v) = req.header("mcp-protocol-version") {
            if !SUPPORTED_PROTOCOL_VERSIONS.contains(&v) {
                return Err(error_response(
                    400,
                    TRANSPORT_ERROR,
                    format!(
                        "Bad Request: unsupported MCP-Protocol-Version {v:?} (supported: {})",
                        SUPPORTED_PROTOCOL_VERSIONS.join(", ")
                    ),
                ));
            }
        }
        Ok(session)
    }

    fn initialize(&self, id: &Json, params: Option<&Json>) -> Response {
        let empty = Json::obj::<&str>([]);
        let params = params.unwrap_or(&empty);
        let Some(requested) = params.get("protocolVersion").and_then(Json::as_str) else {
            let reply = rpc_error(id.clone(), INVALID_PARAMS, "initialize: params.protocolVersion (string) is required");
            return json_response(200, &reply);
        };
        let negotiated = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) { requested } else { LATEST_PROTOCOL_VERSION };
        let client = params.get("clientInfo");
        let field = |k: &str| client.and_then(|c| c.get(k)).and_then(Json::as_str).unwrap_or("").to_owned();
        let info = SessionInfo {
            id: self.new_session_id(),
            client_name: field("name"),
            client_version: field("version"),
            protocol_version: negotiated.to_owned(),
        };
        {
            let mut sessions = lock(&self.sessions);
            if sessions.map.len() >= MAX_SESSIONS {
                let oldest = sessions.map.iter().min_by_key(|(_, (seq, _, _))| *seq).map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    sessions.map.remove(&oldest);
                }
            }
            let seq = sessions.next_seq;
            sessions.next_seq += 1;
            sessions.map.insert(info.id.clone(), (seq, info.clone(), Instant::now()));
        }

        let mut result = Json::obj([
            ("protocolVersion", Json::from(negotiated)),
            ("capabilities", Json::obj([("tools", Json::obj([("listChanged", Json::from(false))]))])),
            (
                "serverInfo",
                Json::obj([
                    ("name", Json::from(self.config.server_name.as_str())),
                    ("version", Json::from(self.config.server_version.as_str())),
                ]),
            ),
        ]);
        if let Some(instructions) = &self.config.instructions {
            result.set("instructions", Json::from(instructions.as_str()));
        }
        json_response(200, &rpc_result(id.clone(), result)).header("Mcp-Session-Id", info.id)
    }

    fn dispatch(&self, session: &SessionInfo, method: &str, params: Option<&Json>) -> Result<Json, (i64, String)> {
        match method {
            "ping" => Ok(Json::obj::<&str>([])),
            "tools/list" => self.tools_list(),
            "tools/call" => self.tools_call(session, params),
            _ => Err((METHOD_NOT_FOUND, format!("Method not found: {method}"))),
        }
    }

    /// Valid tool definitions; invalid names are dropped with a one-time stderr warning.
    fn valid_tools(&self) -> Result<Vec<crate::ToolDef>, (i64, String)> {
        let tools = panic::catch_unwind(AssertUnwindSafe(|| self.handler.tools()))
            .map_err(|p| (INTERNAL_ERROR, format!("Internal error: tool list panicked: {}", panic_message(&*p))))?;
        let mut warned = lock(&self.warned_names);
        Ok(tools
            .into_iter()
            .filter(|t| {
                let ok = is_valid_tool_name(&t.name);
                if !ok && warned.insert(t.name.clone()) {
                    eprintln!(
                        "specula-protocol: omitting tool {:?}: names must match ^[a-zA-Z0-9_-]{{1,64}}$",
                        t.name
                    );
                }
                ok
            })
            .collect())
    }

    fn tools_list(&self) -> Result<Json, (i64, String)> {
        let tools = self.valid_tools()?;
        let list = tools.into_iter().map(|t| {
            Json::obj([
                ("name", Json::from(t.name)),
                ("description", Json::from(t.description)),
                ("inputSchema", t.input_schema),
            ])
        });
        Ok(Json::obj([("tools", Json::arr(list))]))
    }

    fn tools_call(&self, session: &SessionInfo, params: Option<&Json>) -> Result<Json, (i64, String)> {
        let params = params.ok_or((INVALID_PARAMS, "tools/call: params are required".to_owned()))?;
        let name = params
            .get("name")
            .and_then(Json::as_str)
            .ok_or((INVALID_PARAMS, "tools/call: params.name (string) is required".to_owned()))?;
        let empty = Json::obj::<&str>([]);
        let arguments = match params.get("arguments") {
            None | Some(Json::Null) => &empty,
            Some(args @ Json::Obj(_)) => args,
            Some(_) => return Err((INVALID_PARAMS, "tools/call: params.arguments must be an object".to_owned())),
        };
        if !self.valid_tools()?.iter().any(|t| t.name == name) {
            return Err((INVALID_PARAMS, format!("Unknown tool: {name}")));
        }
        let output = panic::catch_unwind(AssertUnwindSafe(|| self.handler.call(session, name, arguments)))
            .unwrap_or_else(|p| ToolOutput::error(format!("tool {name:?} panicked: {}", panic_message(&*p))));
        Ok(tool_output_json(output))
    }

    /// 32 lowercase hex chars (128 bits) from two randomly keyed SipHash instances over the time,
    /// a counter and the process id — unguessable enough for a loopback-only server.
    fn new_session_id(&self) -> String {
        let count = self.id_counter.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let mut id = String::with_capacity(32);
        for lane in 0u8..2 {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(nanos);
            h.write_u64(count);
            h.write_u8(lane);
            h.write_u32(std::process::id());
            h.write_usize(&count as *const u64 as usize);
            let _ = write!(id, "{:016x}", h.finish());
        }
        id
    }
}

fn tool_output_json(output: ToolOutput) -> Json {
    let content = output.content.into_iter().map(|c| match c {
        Content::Text(text) => Json::obj([("type", Json::from("text")), ("text", Json::from(text))]),
        Content::Image { mime_type, data_base64 } => Json::obj([
            ("type", Json::from("image")),
            ("data", Json::from(data_base64)),
            ("mimeType", Json::from(mime_type)),
        ]),
    });
    Json::obj([("content", Json::arr(content)), ("isError", Json::from(output.is_error))])
}

fn classify(msg: &Json) -> Result<Message<'_>, Invalid> {
    let invalid = |id: Json, message: &str| Invalid { id, message: message.to_owned() };
    if msg.as_object().is_none() {
        return Err(invalid(Json::Null, "message must be a JSON object"));
    }
    // MCP ids are strings or numbers (never null).
    let id = match msg.get("id") {
        None => None,
        Some(Json::Num(n)) if !n.is_finite() => return Err(invalid(Json::Null, "id must be a finite number")),
        Some(id @ (Json::Str(_) | Json::Num(_))) => Some(id),
        Some(_) => return Err(invalid(Json::Null, "id must be a string or a number")),
    };
    let echo_id = id.cloned().unwrap_or(Json::Null);
    if msg.get("jsonrpc").and_then(Json::as_str) != Some("2.0") {
        return Err(invalid(echo_id, "jsonrpc must be \"2.0\""));
    }
    match msg.get("method") {
        Some(Json::Str(method)) => {
            let params = msg.get("params");
            if let Some(p) = params {
                if p.as_object().is_none() {
                    return Err(invalid(echo_id, "params must be an object"));
                }
            }
            Ok(match id {
                Some(id) => Message::Request { id, method, params },
                None => Message::Notification,
            })
        }
        Some(_) => Err(invalid(echo_id, "method must be a string")),
        None => {
            let has_result = msg.get("result").is_some();
            let has_error = msg.get("error").is_some();
            if id.is_some() && has_result != has_error {
                Ok(Message::Response)
            } else {
                Err(invalid(echo_id, "not a request, notification or response"))
            }
        }
    }
}

/// `http(s)://localhost|127.0.0.1|[::1]` with an optional port, nothing else.
pub(crate) fn origin_allowed(origin: &str) -> bool {
    let origin = origin.trim();
    let lower = origin.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("http://").or_else(|| lower.strip_prefix("https://")) else {
        return false;
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = if let Some(after) = rest.strip_prefix("[::1]") {
        ("[::1]", after)
    } else {
        match rest.find(':') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        }
    };
    let port_ok = match port.strip_prefix(':') {
        None => port.is_empty(),
        Some(p) => !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()),
    };
    port_ok && matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

fn media_type_is(value: &str, want: &str) -> bool {
    value.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case(want)
}

fn accepts_json(accept: &str) -> bool {
    accept.split(',').any(|range| {
        let t = range.split(';').next().unwrap_or("").trim();
        ["application/json", "application/*", "*/*"].iter().any(|w| t.eq_ignore_ascii_case(w))
    })
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn rpc_result(id: Json, result: Json) -> Json {
    Json::obj([("jsonrpc", Json::from("2.0")), ("id", id), ("result", result)])
}

fn rpc_error(id: Json, code: i64, message: &str) -> Json {
    Json::obj([
        ("jsonrpc", Json::from("2.0")),
        ("id", id),
        ("error", Json::obj([("code", Json::from(code)), ("message", Json::from(message))])),
    ])
}

fn json_response(status: u16, body: &Json) -> Response {
    Response::with_body(status, "application/json", body.to_string())
}

/// HTTP error whose body is a JSON-RPC error without a usable id.
pub(crate) fn error_response(status: u16, code: i64, message: impl AsRef<str>) -> Response {
    json_response(status, &rpc_error(Json::Null, code, message.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        for ok in [
            "http://localhost",
            "http://localhost:3000",
            "https://127.0.0.1:7471",
            "http://[::1]:8080",
            "HTTP://LOCALHOST",
            "http://localhost/",
        ] {
            assert!(origin_allowed(ok), "{ok}");
        }
        for bad in [
            "null",
            "http://evil.com",
            "http://localhost.evil.com",
            "http://127.0.0.1.nip.io",
            "http://localhost:",
            "http://localhost:80x",
            "file://localhost",
            "http://[::1]evil",
            "http://localhost@evil.com",
            "",
        ] {
            assert!(!origin_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn accept_and_content_type() {
        assert!(accepts_json("application/json, text/event-stream"));
        assert!(accepts_json("text/event-stream;q=1, */*;q=0.1"));
        assert!(!accepts_json("text/event-stream"));
        assert!(media_type_is("Application/JSON; charset=utf-8", "application/json"));
        assert!(!media_type_is("text/plain", "application/json"));
    }

    #[test]
    fn classification() {
        let parse = |s: &str| Json::parse(s).unwrap();
        let m = parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        assert!(matches!(classify(&m), Ok(Message::Request { method: "ping", .. })));
        let m = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(matches!(classify(&m), Ok(Message::Notification)));
        let m = parse(r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
        assert!(matches!(classify(&m), Ok(Message::Response)));
        for bad in [
            r#"{"id":1,"method":"ping"}"#,
            r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":1e999,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":1,"method":5}"#,
            r#"{"jsonrpc":"2.0","id":1,"method":"x","params":[1]}"#,
            r#"{"jsonrpc":"2.0","id":1}"#,
            r#""text""#,
        ] {
            assert!(classify(&parse(bad)).is_err(), "{bad}");
        }
        let Err(e) = classify(&parse(r#"{"jsonrpc":"1.0","id":7,"method":"ping"}"#)) else { panic!() };
        assert_eq!(e.id, Json::from(7));
    }

    #[test]
    fn session_ids_are_32_hex_and_distinct() {
        struct NoTools;
        impl ToolHandler for NoTools {
            fn tools(&self) -> Vec<crate::ToolDef> {
                Vec::new()
            }
            fn call(&self, _: &SessionInfo, _: &str, _: &Json) -> ToolOutput {
                ToolOutput::text("")
            }
        }
        let config = ServerConfig {
            bind: "127.0.0.1:0".into(),
            server_name: "t".into(),
            server_version: "0".into(),
            instructions: None,
        };
        let state = State::new(config, Arc::new(NoTools));
        let ids: HashSet<String> = (0..1000).map(|_| state.new_session_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())));
    }
}

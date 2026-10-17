//! End-to-end tests over real sockets, with a minimal std-only HTTP/1.1 client.

use specula_core::Json;
use specula_protocol::{
    Content, Server, ServerConfig, SessionInfo, ToolDef, ToolHandler, ToolOutput, LATEST_PROTOCOL_VERSION,
    SUPPORTED_PROTOCOL_VERSIONS,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ── test handler ─────────────────────────────────────────────────────────────

struct TestTools;

impl ToolHandler for TestTools {
    fn tools(&self) -> Vec<ToolDef> {
        let schema = |props: Json| Json::obj([("type", Json::from("object")), ("properties", props)]);
        vec![
            ToolDef {
                name: "echo".into(),
                description: "Returns its text".into(),
                input_schema: schema(Json::obj([("text", Json::obj([("type", Json::from("string"))]))])),
            },
            ToolDef {
                name: "picture".into(),
                description: "Returns an image".into(),
                input_schema: schema(Json::obj::<&str>([])),
            },
            ToolDef { name: "boom".into(), description: "Panics".into(), input_schema: schema(Json::obj::<&str>([])) },
            ToolDef { name: "whoami".into(), description: "Session".into(), input_schema: schema(Json::obj::<&str>([])) },
            ToolDef {
                name: "bad.name".into(),
                description: "Invalid name; must be omitted".into(),
                input_schema: schema(Json::obj::<&str>([])),
            },
        ]
    }

    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput {
        match name {
            "echo" => ToolOutput::text(arguments.get("text").and_then(Json::as_str).unwrap_or("")),
            "picture" => ToolOutput {
                content: vec![
                    Content::Text("a picture".into()),
                    Content::Image { mime_type: "image/png".into(), data_base64: "iVBORw0KGgo=".into() },
                ],
                is_error: false,
            },
            "boom" => panic!("kaboom {}", 42),
            "whoami" => ToolOutput::text(format!("{} {}", session.client_name, session.protocol_version)),
            other => ToolOutput::error(format!("unexpected {other}")),
        }
    }
}

fn start() -> Server {
    let config = ServerConfig {
        bind: "127.0.0.1:0".into(),
        server_name: "specula-test".into(),
        server_version: "9.9.9".into(),
        instructions: Some("Be nice.".into()),
    };
    Server::start(config, Arc::new(TestTools)).expect("server starts")
}

// ── minimal HTTP client ──────────────────────────────────────────────────────

#[derive(Debug)]
struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Resp {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Json {
        Json::parse(std::str::from_utf8(&self.body).expect("utf-8 body")).expect("JSON body")
    }

    fn error_code(&self) -> Option<i64> {
        self.json().get("error")?.get("code")?.as_i64()
    }
}

struct Client {
    reader: BufReader<TcpStream>,
}

impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).expect("connect");
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Client { reader: BufReader::new(stream) }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.reader.get_mut().write_all(bytes).expect("write");
    }

    fn request(&mut self, method: &str, headers: &[(&str, &str)], body: &[u8]) -> Resp {
        let mut head = format!("{method} /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        for (k, v) in headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        if !body.is_empty() || method == "POST" {
            head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        self.write(&bytes);
        self.read_response()
    }

    fn read_response(&mut self) -> Resp {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("status line");
        let status: u16 = line.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or_else(|| panic!("bad status {line:?}"));
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            self.reader.read_line(&mut h).expect("header");
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            let (k, v) = h.split_once(':').expect("header colon");
            headers.push((k.trim().to_owned(), v.trim().to_owned()));
        }
        let len: usize = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.parse().expect("content-length"))
            .expect("every response carries Content-Length");
        let mut body = vec![0; len];
        self.reader.read_exact(&mut body).expect("body");
        Resp { status, headers, body }
    }

    /// True if the server closed the connection.
    fn is_closed(&mut self) -> bool {
        let mut buf = [0u8; 1];
        matches!(self.reader.read(&mut buf), Ok(0) | Err(_))
    }
}

/// POSTs one JSON-RPC message with the usual MCP headers.
fn post(c: &mut Client, session: Option<&str>, body: &str) -> Resp {
    let mut headers = vec![("Content-Type", "application/json"), ("Accept", "application/json, text/event-stream")];
    if let Some(s) = session {
        headers.push(("Mcp-Session-Id", s));
    }
    c.request("POST", &headers, body.as_bytes())
}

fn initialize(c: &mut Client, version: &str, client_name: &str) -> (String, Resp) {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{version}","capabilities":{{}},"clientInfo":{{"name":"{client_name}","version":"1.0"}}}}}}"#
    );
    let resp = post(c, None, &body);
    assert_eq!(resp.status, 200, "{:?}", String::from_utf8_lossy(&resp.body));
    let id = resp.header("Mcp-Session-Id").expect("session header").to_owned();
    (id, resp)
}

/// A ready session: initialize + notifications/initialized.
fn session(c: &mut Client) -> String {
    let (id, _) = initialize(c, "2025-06-18", "tester");
    let resp = post(c, Some(&id), r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert_eq!(resp.status, 202);
    id
}

fn call(c: &mut Client, session: &str, id: u32, tool: &str, args: &str) -> Resp {
    let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{tool}","arguments":{args}}}}}"#);
    post(c, Some(session), &body)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[test]
fn full_flow_on_one_keep_alive_connection() {
    let server = start();
    let mut c = Client::connect(server.local_addr());

    let (sid, init) = initialize(&mut c, "2025-06-18", "tester");
    assert_eq!(sid.len(), 32);
    assert!(sid.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(init.header("Content-Type"), Some("application/json"));
    let v = init.json();
    assert_eq!(v.get("id").and_then(Json::as_u64), Some(1));
    let result = v.get("result").unwrap();
    assert_eq!(result.get("protocolVersion").and_then(Json::as_str), Some("2025-06-18"));
    assert_eq!(result.get("capabilities").unwrap().to_string(), r#"{"tools":{"listChanged":false}}"#);
    assert_eq!(result.get("serverInfo").unwrap().to_string(), r#"{"name":"specula-test","version":"9.9.9"}"#);
    assert_eq!(result.get("instructions").and_then(Json::as_str), Some("Be nice."));

    let resp = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert_eq!((resp.status, resp.body.len()), (202, 0));

    let list = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":"list","method":"tools/list","params":{}}"#);
    assert_eq!(list.status, 200);
    let v = list.json();
    assert_eq!(v.get("id").and_then(Json::as_str), Some("list"));
    let tools = v.get("result").unwrap().get("tools").unwrap().as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.get("name").unwrap().as_str().unwrap()).collect();
    assert_eq!(names, ["echo", "picture", "boom", "whoami"], "invalid names omitted");
    assert_eq!(tools[0].get("description").and_then(Json::as_str), Some("Returns its text"));
    assert_eq!(tools[0].get("inputSchema").unwrap().get("type").and_then(Json::as_str), Some("object"));

    let echo = call(&mut c, &sid, 3, "echo", r#"{"text":"héllo \"world\""}"#);
    assert_eq!(
        echo.json().get("result").unwrap().to_string(),
        r#"{"content":[{"type":"text","text":"héllo \"world\""}],"isError":false}"#
    );

    let pic = call(&mut c, &sid, 4, "picture", "{}");
    assert_eq!(
        pic.json().get("result").unwrap().to_string(),
        r#"{"content":[{"type":"text","text":"a picture"},{"type":"image","data":"iVBORw0KGgo=","mimeType":"image/png"}],"isError":false}"#
    );

    let who = call(&mut c, &sid, 5, "whoami", "{}");
    let text = who.json().get("result").unwrap().get("content").unwrap().as_array().unwrap()[0].get("text").cloned();
    assert_eq!(text, Some(Json::from("tester 2025-06-18")));

    let ping = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#);
    assert_eq!(ping.json().to_string(), r#"{"jsonrpc":"2.0","id":6,"result":{}}"#);

    let sessions = server.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, sid);
    assert_eq!(sessions[0].client_name, "tester");
    assert_eq!(sessions[0].client_version, "1.0");
    server.shutdown();
}

#[test]
fn version_negotiation() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    for (asked, answered) in [
        ("2025-11-25", "2025-11-25"),
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", LATEST_PROTOCOL_VERSION),
        ("1999-01-01", LATEST_PROTOCOL_VERSION),
        // The session-less revision isn't served: a client asking for it is offered ours.
        ("2026-07-28", "2025-11-25"),
    ] {
        let (_, resp) = initialize(&mut c, asked, "v");
        let got = resp.json().get("result").unwrap().get("protocolVersion").and_then(Json::as_str).map(str::to_owned);
        assert_eq!(got.as_deref(), Some(answered), "asked {asked}");
    }
    let resp = post(&mut c, None, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    assert_eq!(resp.error_code(), Some(-32602));
    assert_eq!(server.sessions().len(), 5);
}

/// The MCP revisions are pinned: moving to another (2026-07-28 drops `initialize` and sessions) is
/// the owner's decision, so it must be a deliberate change to this test too.
#[test]
fn supported_revisions_are_pinned() {
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS, ["2025-11-25", "2025-06-18", "2025-03-26"]);
    assert_eq!(LATEST_PROTOCOL_VERSION, "2025-11-25");
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS[0], LATEST_PROTOCOL_VERSION);
}

#[test]
fn session_header_rules() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    let missing = post(&mut c, None, ping);
    assert_eq!(missing.status, 400);
    assert!(missing.json().get("error").is_some());

    let unknown = post(&mut c, Some("0123456789abcdef0123456789abcdef"), ping);
    assert_eq!(unknown.status, 404);

    let note = post(&mut c, None, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert_eq!(note.status, 400, "notifications need a session too");

    let sid = session(&mut c);
    let ok = c.request("POST", &[("Mcp-Session-Id", sid.as_str()), ("MCP-Protocol-Version", "2025-06-18")], ping.as_bytes());
    assert_eq!(ok.status, 200);
    let bad = c.request("POST", &[("Mcp-Session-Id", sid.as_str()), ("mcp-protocol-version", "2099-01-01")], ping.as_bytes());
    assert_eq!(bad.status, 400);
    assert!(String::from_utf8_lossy(&bad.body).contains("2099-01-01"));
}

#[test]
fn origin_checks() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;
    let evil = c.request("POST", &[("Origin", "http://evil.example")], init.as_bytes());
    assert_eq!(evil.status, 403);
    assert!(evil.header("Mcp-Session-Id").is_none());
    let rebinding = c.request("POST", &[("Origin", "http://localhost.evil.example:7471")], init.as_bytes());
    assert_eq!(rebinding.status, 403);
    assert!(server.sessions().is_empty());

    for origin in ["http://localhost:5173", "http://127.0.0.1", "https://[::1]:7471"] {
        let ok = c.request("POST", &[("Origin", origin)], init.as_bytes());
        assert_eq!(ok.status, 200, "{origin}");
    }
}

#[test]
fn notifications_and_responses_are_accepted() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);
    for msg in [
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#,
        r#"{"jsonrpc":"2.0","method":"anything/at/all"}"#,
        r#"{"jsonrpc":"2.0","id":9,"result":{}}"#,
        r#"{"jsonrpc":"2.0","id":9,"error":{"code":-1,"message":"no"}}"#,
    ] {
        let resp = post(&mut c, Some(&sid), msg);
        assert_eq!((resp.status, resp.body.len()), (202, 0), "{msg}");
        assert_eq!(resp.header("Content-Length"), Some("0"));
    }
}

#[test]
fn json_rpc_errors() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);

    let unknown_method = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#);
    assert_eq!(unknown_method.status, 200);
    assert_eq!(unknown_method.error_code(), Some(-32601));
    assert_eq!(unknown_method.json().get("id").and_then(Json::as_u64), Some(2));

    let unknown_tool = call(&mut c, &sid, 3, "nope", "{}");
    assert_eq!(unknown_tool.error_code(), Some(-32602));
    let invalid_name_tool = call(&mut c, &sid, 3, "bad.name", "{}");
    assert_eq!(invalid_name_tool.error_code(), Some(-32602), "omitted tools are not callable");

    let bad_args = call(&mut c, &sid, 4, "echo", "[1]");
    assert_eq!(bad_args.error_code(), Some(-32602));
    let no_name = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{}}"#);
    assert_eq!(no_name.error_code(), Some(-32602));
    let no_args = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"echo"}}"#);
    assert_eq!(no_args.json().get("result").unwrap().get("isError"), Some(&Json::Bool(false)));

    let malformed = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":7,"method":"#);
    assert_eq!(malformed.status, 400);
    assert_eq!(malformed.error_code(), Some(-32700));
    assert!(malformed.json().get("id").unwrap().is_null());

    let not_utf8 = c.request("POST", &[("Mcp-Session-Id", sid.as_str())], b"\xff\xfe{}");
    assert_eq!(not_utf8.error_code(), Some(-32700));

    let batch = post(&mut c, Some(&sid), r#"[{"jsonrpc":"2.0","id":8,"method":"ping"}]"#);
    assert_eq!((batch.status, batch.error_code()), (400, Some(-32600)));

    let invalid = post(&mut c, Some(&sid), r#"{"jsonrpc":"1.0","id":9,"method":"ping"}"#);
    assert_eq!((invalid.status, invalid.error_code()), (400, Some(-32600)));
    assert_eq!(invalid.json().get("id").and_then(Json::as_u64), Some(9));

    // The connection survives all of the above.
    let ping = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#);
    assert_eq!(ping.status, 200);
}

#[test]
fn panicking_tool_reports_is_error_and_server_survives() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);
    let resp = call(&mut c, &sid, 2, "boom", "{}");
    assert_eq!(resp.status, 200);
    let result = resp.json().get("result").cloned().unwrap();
    assert_eq!(result.get("isError"), Some(&Json::Bool(true)));
    let text = result.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap().to_owned();
    assert!(text.contains("kaboom 42"), "{text}");

    let after = call(&mut c, &sid, 3, "echo", r#"{"text":"still here"}"#);
    assert!(String::from_utf8_lossy(&after.body).contains("still here"));
    let mut fresh = Client::connect(server.local_addr());
    session(&mut fresh);
}

#[test]
fn http_level_routing() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    assert_eq!(c.request("GET", &[], b"").status, 405);
    let options = c.request("OPTIONS", &[], b"");
    assert_eq!(options.status, 405);
    assert_eq!(options.header("Allow"), Some("POST, DELETE"));

    c.write(b"POST /other HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}");
    assert_eq!(c.read_response().status, 404);

    let wrong_type = c.request("POST", &[("Content-Type", "text/plain")], b"{}");
    assert_eq!(wrong_type.status, 415);
    let wrong_accept = c.request("POST", &[("Accept", "text/event-stream")], b"{}");
    assert_eq!(wrong_accept.status, 406);

    let mut raw = Client::connect(server.local_addr());
    raw.write(b"THIS IS NOT HTTP\r\n\r\n");
    let resp = raw.read_response();
    assert_eq!(resp.status, 400);
    assert_eq!(resp.header("Connection"), Some("close"));
    assert!(raw.is_closed());
}

#[test]
fn oversized_body_is_413() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    c.write(b"POST /mcp HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 16777217\r\n\r\n");
    let resp = c.read_response();
    assert_eq!(resp.status, 413);
    assert!(c.is_closed());

    // Chunked bodies are limited too, and the 413 survives a client that keeps sending.
    let mut c = Client::connect(server.local_addr());
    c.write(b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n");
    let chunk = vec![b'x'; 1 << 20];
    let mut sent = 0;
    while sent <= 16 << 20 {
        let mut piece = format!("{:x}\r\n", chunk.len()).into_bytes();
        piece.extend_from_slice(&chunk);
        piece.extend_from_slice(b"\r\n");
        if c.reader.get_mut().write_all(&piece).is_err() {
            break;
        }
        sent += chunk.len();
    }
    assert_eq!(c.read_response().status, 413);

    // Exactly at the limit is accepted at the HTTP layer (then fails JSON parsing).
    let mut c = Client::connect(server.local_addr());
    let body = vec![b' '; 16 << 20];
    let resp = c.request("POST", &[], &body);
    assert_eq!((resp.status, resp.error_code()), (400, Some(-32700)));
}

#[test]
fn chunked_request_body() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);
    let body = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"echo","arguments":{"text":"chunky"}}}"#;
    let (a, b) = body.split_at(20);
    let raw = format!(
        "POST /mcp HTTP/1.1\r\nMcp-Session-Id: {sid}\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{a}\r\n{:x};ext=1\r\n{b}\r\n0\r\n\r\n",
        a.len(),
        b.len()
    );
    c.write(raw.as_bytes());
    let resp = c.read_response();
    assert_eq!(resp.status, 200);
    assert!(String::from_utf8_lossy(&resp.body).contains("chunky"));
}

#[test]
fn expect_continue_and_pipelining() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    c.write(
        format!("POST /mcp HTTP/1.1\r\nMcp-Session-Id: {sid}\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n", body.len())
            .as_bytes(),
    );
    let interim = c.read_response_head_only();
    assert_eq!(interim, "HTTP/1.1 100 Continue");
    c.write(body.as_bytes());
    assert_eq!(c.read_response().status, 200);

    // Two requests in one write: both answered in order.
    let one = format!("POST /mcp HTTP/1.1\r\nMcp-Session-Id: {sid}\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    c.write(format!("{one}{}", one.replace("\"id\":1", "\"id\":2")).as_bytes());
    assert_eq!(c.read_response().json().get("id").and_then(Json::as_u64), Some(1));
    assert_eq!(c.read_response().json().get("id").and_then(Json::as_u64), Some(2));
}

impl Client {
    /// Reads an interim `1xx` response (status line + empty line) and returns its status line.
    fn read_response_head_only(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        let mut blank = String::new();
        self.reader.read_line(&mut blank).unwrap();
        assert_eq!(blank, "\r\n");
        line.trim_end().to_owned()
    }
}

#[test]
fn connection_close_is_honoured() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let resp = c.request("GET", &[("Connection", "close")], b"");
    assert_eq!(resp.status, 405);
    assert_eq!(resp.header("Connection"), Some("close"));
    assert!(c.is_closed());

    let mut c = Client::connect(server.local_addr());
    c.write(b"GET /mcp HTTP/1.0\r\n\r\n");
    assert_eq!(c.read_response().status, 405);
    assert!(c.is_closed(), "HTTP/1.0 defaults to close");
}

#[test]
fn delete_ends_session() {
    let server = start();
    let mut c = Client::connect(server.local_addr());
    let sid = session(&mut c);
    assert_eq!(server.sessions().len(), 1);

    assert_eq!(c.request("DELETE", &[], b"").status, 400);
    let resp = c.request("DELETE", &[("Mcp-Session-Id", sid.as_str())], b"");
    assert!(resp.status == 200 || resp.status == 204);
    assert!(server.sessions().is_empty());

    let after = post(&mut c, Some(&sid), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    assert_eq!(after.status, 404);
    assert_eq!(c.request("DELETE", &[("Mcp-Session-Id", sid.as_str())], b"").status, 404);
}

#[test]
fn concurrent_sessions_from_threads() {
    let server = start();
    let addr = server.local_addr();
    let workers: Vec<_> = (0..2)
        .map(|t| {
            std::thread::spawn(move || {
                let mut c = Client::connect(addr);
                let sid = session(&mut c);
                for i in 0..50 {
                    let text = format!("thread {t} call {i}");
                    let resp = call(&mut c, &sid, i, "echo", &format!(r#"{{"text":"{text}"}}"#));
                    assert!(String::from_utf8_lossy(&resp.body).contains(&text));
                }
                sid
            })
        })
        .collect();
    let ids: Vec<String> = workers.into_iter().map(|w| w.join().expect("worker")).collect();
    assert_ne!(ids[0], ids[1]);
    let mut open: Vec<String> = server.sessions().into_iter().map(|s| s.id).collect();
    open.sort();
    let mut expected = ids.clone();
    expected.sort();
    assert_eq!(open, expected);
}

#[test]
fn shutdown_is_prompt_with_idle_connections() {
    let server = start();
    let addr = server.local_addr();
    let mut idle = Client::connect(addr);
    session(&mut idle);
    let _silent = TcpStream::connect(addr).unwrap(); // never sends anything
    let mut half = TcpStream::connect(addr).unwrap();
    half.write_all(b"POST /mcp HTTP/1.1\r\nContent-Length: 100\r\n\r\n{").unwrap(); // mid-body

    std::thread::sleep(Duration::from_millis(100));
    let t = Instant::now();
    server.shutdown();
    assert!(t.elapsed() < Duration::from_secs(3), "shutdown took {:?}", t.elapsed());
    assert!(idle.is_closed());
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err() || {
        // Some stacks accept into a dead backlog briefly; the request must still fail.
        let mut c = Client::connect(addr);
        c.write(b"GET /mcp HTTP/1.1\r\n\r\n");
        c.is_closed()
    });
}

#[test]
fn rejects_non_loopback_bind() {
    for bind in ["0.0.0.0:0", "[::]:0", "8.8.8.8:0"] {
        let config = ServerConfig {
            bind: bind.into(),
            server_name: "x".into(),
            server_version: "0".into(),
            instructions: None,
        };
        let err = Server::start(config, Arc::new(TestTools)).err().expect("non-loopback bind must fail");
        assert!(err.message.contains("loopback"), "{err}");
    }
    let config =
        ServerConfig { bind: "not an address".into(), server_name: "x".into(), server_version: "0".into(), instructions: None };
    assert!(Server::start(config, Arc::new(TestTools)).is_err());
}

#[test]
fn drop_stops_the_server() {
    let addr = {
        let server = start();
        server.local_addr()
    };
    let refused = TcpStream::connect_timeout(&addr, Duration::from_millis(500));
    if let Ok(stream) = refused {
        let mut c = Client { reader: BufReader::new(stream) };
        c.reader.get_ref().set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        c.write(b"GET /mcp HTTP/1.1\r\n\r\n");
        assert!(c.is_closed());
    }
}

/// Swarm scale (M1): past the connection limit a client gets a proper `503` response (even when
/// it has already sent its request), never a reset; once connections free up, service resumes.
#[test]
fn connection_limit_answers_503_not_a_reset() {
    const LIMIT: usize = 256;
    let server = start();
    let addr = server.local_addr();
    let held: Vec<TcpStream> = (0..LIMIT).map(|_| TcpStream::connect(addr).expect("connect")).collect();
    // Let the accept thread hand every held connection to a worker.
    std::thread::sleep(Duration::from_millis(300));
    for attempt in 0..5 {
        let mut c = Client::connect(addr);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let resp = post(&mut c, None, body);
        assert_eq!(resp.status, 503, "attempt {attempt}: {:?}", String::from_utf8_lossy(&resp.body));
        assert_eq!(resp.error_code(), Some(-32000));
        assert!(c.is_closed(), "the rejected connection is closed");
    }
    drop(held);
    // Workers notice their clients left; then requests are served again.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut c = Client::connect(addr);
        let (_, resp) = initialize(&mut c, "2025-06-18", "after");
        if resp.status == 200 {
            break;
        }
        assert!(Instant::now() < deadline, "service did not resume");
        std::thread::sleep(Duration::from_millis(50));
    }
    server.shutdown();
}

/// Reviewer finding: shutdown waited for every queued rejection to linger (up to ~2.5 s each).
#[test]
fn shutdown_is_prompt_with_rejections_queued() {
    let server = start();
    let addr = server.local_addr();
    let held: Vec<TcpStream> = (0..256).map(|_| TcpStream::connect(addr).expect("connect")).collect();
    std::thread::sleep(Duration::from_millis(300));
    // Silent clients over the limit: each would linger until its timeout.
    let queued: Vec<TcpStream> = (0..40).map(|_| TcpStream::connect(addr).expect("connect")).collect();
    std::thread::sleep(Duration::from_millis(200));
    let t = Instant::now();
    server.shutdown();
    assert!(t.elapsed() < Duration::from_secs(3), "shutdown took {:?}", t.elapsed());
    drop((held, queued));
}

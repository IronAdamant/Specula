//! Specula agent protocol: MCP (Model Context Protocol) served over HTTP on localhost.
//!
//! Implements the MCP **Streamable HTTP** transport subset agents need, on `std::net` only:
//!
//! - `POST /mcp` with one JSON-RPC 2.0 message per request (`Content-Type: application/json`).
//!   Requests get a single `application/json` response; notifications and responses get `202 Accepted`.
//! - `initialize` creates a session and returns its id in the `Mcp-Session-Id` header; later requests
//!   must send it (missing → 400, unknown → 404). `DELETE /mcp` ends a session. `GET /mcp` → 405
//!   (no server-initiated SSE stream yet).
//! - Methods: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`.
//!   Unknown method → JSON-RPC `-32601`; bad params / unknown tool → `-32602`.
//! - Security: binds loopback only; requests whose `Origin` header is present and not a localhost
//!   origin are rejected with 403 (DNS-rebinding protection).
//!
//! Tool names must match `^[a-zA-Z0-9_-]{1,64}$` (Claude Code rejects others) — e.g. `world_spawn`.

//! Revision targeted: **2025-11-25** (the latest session-based revision; `2025-06-18` and
//! `2025-03-26` are also accepted). Revision 2026-07-28 drops `initialize` and sessions; per its
//! backward-compatibility rules a dual-era client that probes with a modern request gets a plain
//! `400` here and falls back to `initialize`.
//!
//! Limits: request bodies up to 16 MiB (`413` beyond); idle connections close after 60 s;
//! at most 256 concurrent connections (beyond that, a proper `503` with `Retry-After: 1`, never a
//! reset) and 1024 open sessions (oldest evicted).

mod http;
mod mcp;
mod server;

use specula_core::Json;
use std::fmt;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Protocol revisions accepted in `initialize` and the `MCP-Protocol-Version` header, newest first.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// Answered in `initialize` when the client asks for a revision we do not support.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// Whether `name` matches `^[a-zA-Z0-9_-]{1,64}$` (tools with other names are not listed).
pub fn is_valid_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// e.g. `127.0.0.1:7471`. Use port 0 in tests for an ephemeral port.
    pub bind: String,
    pub server_name: String,
    pub server_version: String,
    /// Sent in the `initialize` result to guide agents.
    pub instructions: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema object for the arguments.
    pub input_schema: Json,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    /// MCP image content: `{"type":"image","mimeType":…,"data":<base64>}`.
    Image { mime_type: String, data_base64: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<Content>,
    /// Tool-level failure (reported to the agent as `isError: true`, not a JSON-RPC error).
    pub is_error: bool,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> ToolOutput {
        ToolOutput { content: vec![Content::Text(text.into())], is_error: false }
    }

    pub fn error(text: impl Into<String>) -> ToolOutput {
        ToolOutput { content: vec![Content::Text(text.into())], is_error: true }
    }
}

/// One connected agent (created by `initialize`).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionInfo {
    pub id: String,
    pub client_name: String,
    pub client_version: String,
    /// Negotiated MCP protocol version.
    pub protocol_version: String,
}

/// Implemented by the engine. Called from server threads, possibly concurrently.
pub trait ToolHandler: Send + Sync + 'static {
    fn tools(&self) -> Vec<ToolDef>;
    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub message: String,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolError {}

/// Running server: an accept thread plus one thread per connection.
///
/// Dropping a `Server` shuts it down like [`Server::shutdown`].
pub struct Server {
    local_addr: SocketAddr,
    shared: Arc<server::Shared>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Binds `config.bind` (loopback addresses only — anything else is an error) and starts serving.
    pub fn start(config: ServerConfig, handler: Arc<dyn ToolHandler>) -> Result<Server, ProtocolError> {
        let err = |message: String| ProtocolError { message };
        let addrs: Vec<SocketAddr> = config
            .bind
            .to_socket_addrs()
            .map_err(|e| err(format!("invalid bind address {:?}: {e}", config.bind)))?
            .collect();
        if addrs.is_empty() {
            return Err(err(format!("bind address {:?} resolved to nothing", config.bind)));
        }
        if let Some(bad) = addrs.iter().find(|a| !a.ip().is_loopback()) {
            return Err(err(format!(
                "refusing to bind {bad} (from {:?}): the agent protocol serves loopback addresses only",
                config.bind
            )));
        }
        let listener = TcpListener::bind(&addrs[..]).map_err(|e| err(format!("bind {:?} failed: {e}", config.bind)))?;
        let local_addr = listener.local_addr().map_err(|e| err(format!("local_addr failed: {e}")))?;
        let shared = Arc::new(server::Shared::new(mcp::State::new(config, handler)));
        let accept = server::spawn_accept(listener, Arc::clone(&shared))
            .map_err(|e| err(format!("could not spawn the accept thread: {e}")))?;
        Ok(Server { local_addr, shared, accept: Some(accept) })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Sessions used in the last 30 minutes, oldest first (idle ones stay valid but are not listed).
    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.shared.state.sessions()
    }

    /// Stop accepting, close connections, join threads.
    ///
    /// Waits for tool calls already running to return (their responses are dropped).
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(accept) = self.accept.take() {
            server::stop(&self.shared, self.local_addr, accept);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names() {
        for ok in ["world_spawn", "sight-frame", "A1", &"a".repeat(64)] {
            assert!(is_valid_tool_name(ok), "{ok}");
        }
        for bad in ["", "world.spawn", "has space", "é", &"a".repeat(65)] {
            assert!(!is_valid_tool_name(bad), "{bad}");
        }
    }
}

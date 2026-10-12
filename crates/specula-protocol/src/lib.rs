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

use specula_core::Json;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

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
pub struct Server {
    _private: (),
}

impl Server {
    pub fn start(config: ServerConfig, handler: Arc<dyn ToolHandler>) -> Result<Server, ProtocolError> {
        let _ = (config, handler);
        todo!("specula-protocol: Server::start")
    }

    pub fn local_addr(&self) -> SocketAddr {
        todo!("specula-protocol: Server::local_addr")
    }

    /// Currently open sessions.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        todo!("specula-protocol: Server::sessions")
    }

    /// Stop accepting, close connections, join threads.
    pub fn shutdown(self) {
        todo!("specula-protocol: Server::shutdown")
    }
}

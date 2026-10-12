//! Minimal MCP server for trying the protocol by hand or from Claude Code.
//!
//! ```text
//! cargo run -p specula-protocol --example echo_server
//! claude mcp add --transport http specula-echo http://127.0.0.1:7471/mcp
//! ```
//!
//! Tools: `echo` returns its `text` argument; `red_square` returns an 8×8 red PNG.
//! Press Enter to shut down (with stdin closed, e.g. run in the background, it serves until killed).

use specula_core::Json;
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolDef, ToolHandler, ToolOutput};
use std::sync::Arc;

/// 8×8 opaque red RGB PNG (74 bytes).
const RED_SQUARE_PNG: [u8; 74] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, //
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, //
    0x08, 0x02, 0x00, 0x00, 0x00, 0x4b, 0x6d, 0x29, 0xdc, 0x00, 0x00, 0x00, //
    0x11, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0x80, //
    0x15, 0x31, 0x0c, 0x2d, 0x09, 0x00, 0x28, 0xff, 0x3f, 0xc1, 0xce, 0x77, //
    0xc8, 0x4f, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, //
    0x60, 0x82,
];

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| chunk.get(i).copied().unwrap_or(0) as u32;
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

struct EchoTools;

impl ToolHandler for EchoTools {
    fn tools(&self) -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "echo".into(),
                description: "Returns the given text unchanged.".into(),
                input_schema: Json::obj([
                    ("type", Json::from("object")),
                    (
                        "properties",
                        Json::obj([(
                            "text",
                            Json::obj([("type", Json::from("string")), ("description", Json::from("Text to echo"))]),
                        )]),
                    ),
                    ("required", Json::arr([Json::from("text")])),
                ]),
            },
            ToolDef {
                name: "red_square".into(),
                description: "Returns an 8x8 red PNG image.".into(),
                input_schema: Json::obj([
                    ("type", Json::from("object")),
                    ("additionalProperties", Json::from(false)),
                ]),
            },
        ]
    }

    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput {
        eprintln!("[{}] {name} {arguments}", session.client_name);
        match name {
            "echo" => match arguments.get("text").and_then(Json::as_str) {
                Some(text) => ToolOutput::text(text),
                None => ToolOutput::error("echo: argument 'text' (string) is required"),
            },
            "red_square" => ToolOutput {
                content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64(&RED_SQUARE_PNG) }],
                is_error: false,
            },
            other => ToolOutput::error(format!("no tool named {other}")),
        }
    }
}

fn main() {
    let config = ServerConfig {
        bind: "127.0.0.1:7471".into(),
        server_name: "specula-echo".into(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        instructions: Some("Test server: `echo` returns text, `red_square` returns a small red PNG.".into()),
    };
    let server = match Server::start(config, Arc::new(EchoTools)) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("echo_server: {e}");
            std::process::exit(1);
        }
    };
    println!("MCP echo server on http://{}/mcp — press Enter to stop", server.local_addr());
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    // Closed stdin (e.g. run in the background) returns at once; keep serving in that case.
    if line.is_empty() {
        loop {
            std::thread::park();
        }
    }
    println!("open sessions at shutdown: {}", server.sessions().len());
    server.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE=")];
        for (input, expected) in cases {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        assert_eq!(
            base64(&RED_SQUARE_PNG),
            "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR42mP4z8CAFTEMLQkAKP8/wc53yE8AAAAASUVORK5CYII="
        );
    }
}

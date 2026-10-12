//! Standard base64 (RFC 4648 §4, with `=` padding). MCP image content is base64.

pub fn encode(data: &[u8]) -> String {
    let _ = data;
    todo!("specula-core: base64::encode")
}

/// Rejects invalid characters, bad padding and non-canonical trailing bits.
pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    let _ = text;
    todo!("specula-core: base64::decode")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base64Error {
    /// Byte offset of the offending character (or input length for truncation).
    pub offset: usize,
    pub message: String,
}

impl std::fmt::Display for Base64Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "base64 error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for Base64Error {}

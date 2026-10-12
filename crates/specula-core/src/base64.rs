//! Standard base64 (RFC 4648 §4, with `=` padding). MCP image content is base64.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Marker in [`DECODE_TABLE`] for bytes outside the alphabet.
const INVALID: u8 = 0xFF;

/// Reverse lookup: byte → 6-bit value, or [`INVALID`].
static DECODE_TABLE: [u8; 256] = build_decode_table();

const fn build_decode_table() -> [u8; 256] {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
}

/// Encode bytes as padded standard base64.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Rejects invalid characters, bad padding and non-canonical trailing bits.
pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    let s = text.as_bytes();
    let err = |offset: usize, message: &str| Base64Error { offset, message: message.to_string() };
    if s.len() % 4 != 0 {
        // Report the first invalid character if there is one, otherwise the truncation.
        if let Some(i) = s.iter().position(|&c| c != b'=' && DECODE_TABLE[c as usize] == INVALID) {
            return Err(err(i, "invalid base64 character"));
        }
        return Err(err(s.len(), "input length is not a multiple of 4"));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let quads = s.len() / 4;
    for q in 0..quads {
        let base = q * 4;
        let quad = &s[base..base + 4];
        let last = q + 1 == quads;
        // Padding is only allowed at the end of the final quad, one or two characters long.
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 {
            return Err(err(base + 4 - pad, "too much padding"));
        }
        if pad > 0 && !last {
            return Err(err(base + 4 - pad, "padding before end of input"));
        }
        let mut n = 0u32;
        for (j, &c) in quad[..4 - pad].iter().enumerate() {
            let v = DECODE_TABLE[c as usize];
            if v == INVALID {
                let message = if c == b'=' { "misplaced padding" } else { "invalid base64 character" };
                return Err(err(base + j, message));
            }
            n |= (v as u32) << (18 - 6 * j);
        }
        match pad {
            0 => out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            1 => {
                if n & 0xFF != 0 {
                    return Err(err(base + 2, "non-zero trailing bits"));
                }
                out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8]);
            }
            _ => {
                if n & 0xFFFF != 0 {
                    return Err(err(base + 1, "non-zero trailing bits"));
                }
                out.push((n >> 16) as u8);
            }
        }
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    const RFC4648: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn rfc4648_vectors() {
        for (plain, coded) in RFC4648 {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn round_trips_all_byte_values() {
        let data: Vec<u8> = (0..=255u8).chain((0..=255u8).rev()).collect();
        for len in 0..data.len() {
            assert_eq!(decode(&encode(&data[..len])).unwrap(), &data[..len]);
        }
        assert_eq!(encode(&[0xFB, 0xFF, 0xBF]), "+/+/");
    }

    #[test]
    fn rejects_malformed_input_with_offsets() {
        assert_eq!(decode("Zm9v!mFy").unwrap_err().offset, 4);
        assert_eq!(decode("Zm9").unwrap_err().offset, 3); // truncated
        assert_eq!(decode("Zm=v").unwrap_err().offset, 2); // padding in the middle
        assert_eq!(decode("Zg==Zm9v").unwrap_err().offset, 2); // padding before the end
        assert_eq!(decode("Z===").unwrap_err().offset, 1); // three pad characters
        assert_eq!(decode("Zh==").unwrap_err().offset, 1); // non-zero trailing bits
        assert_eq!(decode("Zm9=").unwrap_err().offset, 2); // non-zero trailing bits
        assert_eq!(decode("Zm9v YmFy").unwrap_err().offset, 4);
        assert!(decode("Zm8=").is_ok());
    }
}

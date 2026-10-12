//! Minimal HTTP/1.1 server-side message parsing and response serialization (RFC 9112 subset).
//!
//! Reading is split into [`read_head`], [`framing`] and [`read_body`] so the connection loop can
//! answer `Expect: 100-continue` (curl sends it for bodies over 1 KiB) between head and body.
//! Everything is bounded: line length, header count, body size — hostile input cannot make the
//! server allocate without limit.

use std::io::{self, BufRead};

/// Longest request line (method + target + version).
const MAX_REQUEST_LINE: usize = 8 * 1024;
/// Longest single header line.
const MAX_HEADER_LINE: usize = 16 * 1024;
/// Total header bytes per request (and per chunked trailer section).
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_HEADERS: usize = 128;
/// Longest chunk-size line (hex size plus extensions).
const MAX_CHUNK_LINE: usize = 1024;

/// An HTTP-level failure that should be answered with `status` (then the connection closed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpError {
    pub status: u16,
    pub message: String,
}

impl HttpError {
    pub fn new(status: u16, message: impl Into<String>) -> HttpError {
        HttpError { status, message: message.into() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// Peer closed, I/O error or timeout: nothing sensible to answer, just drop the connection.
    Closed,
    /// Malformed or unacceptable request: answer with this status and close.
    Http(HttpError),
}

impl From<HttpError> for ReadError {
    fn from(e: HttpError) -> Self {
        ReadError::Http(e)
    }
}

/// Request line plus headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Head {
    pub method: String,
    /// Request target as sent (origin-form, e.g. `/mcp?x=1`).
    pub target: String,
    /// `0` for HTTP/1.0, `1` for HTTP/1.1.
    pub minor_version: u8,
    /// Header names as received; look them up with [`Head::header`].
    pub headers: Vec<(String, String)>,
}

impl Head {
    /// First header with this name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// All values of a header (case-insensitive), in order.
    pub fn headers_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers.iter().filter(move |(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// Path part of the target (before `?`).
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    /// Whether the client wants the connection kept open after this exchange.
    pub fn keep_alive(&self) -> bool {
        let has_token = |token: &str| {
            self.headers_named("connection")
                .flat_map(|v| v.split(','))
                .any(|t| t.trim().eq_ignore_ascii_case(token))
        };
        if has_token("close") {
            false
        } else if self.minor_version == 0 {
            has_token("keep-alive")
        } else {
            true
        }
    }

    /// `Expect: 100-continue` on an HTTP/1.1 request.
    pub fn expects_continue(&self) -> bool {
        self.minor_version >= 1
            && self.header("expect").is_some_and(|v| v.trim().eq_ignore_ascii_case("100-continue"))
    }
}

/// A complete request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub head: Head,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.header(name)
    }
}

/// How the request body is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Framing {
    Length(usize),
    Chunked,
}

/// Reads a request line and headers. `Ok(None)` means the peer closed cleanly between requests.
pub(crate) fn read_head<R: BufRead>(r: &mut R) -> Result<Option<Head>, ReadError> {
    // RFC 9112 §2.2: a server SHOULD ignore at least one empty line before the request line.
    let mut request_line = None;
    for _ in 0..4 {
        match read_line(r, MAX_REQUEST_LINE, 414)? {
            None => return Ok(None),
            Some(line) if line.is_empty() => continue,
            Some(line) => {
                request_line = Some(line);
                break;
            }
        }
    }
    let line = request_line.ok_or_else(|| HttpError::new(400, "missing request line"))?;
    let (method, target, minor_version) = parse_request_line(&line)?;

    let mut headers = Vec::new();
    let mut total = 0usize;
    loop {
        let line = match read_line(r, MAX_HEADER_LINE, 431)? {
            None => return Err(ReadError::Closed),
            Some(line) => line,
        };
        if line.is_empty() {
            break;
        }
        total += line.len();
        if total > MAX_HEADER_BYTES || headers.len() >= MAX_HEADERS {
            return Err(HttpError::new(431, "request header fields too large").into());
        }
        headers.push(parse_header_line(&line)?);
    }
    Ok(Some(Head { method, target, minor_version, headers }))
}

/// Determines the body framing, enforcing `limit` on declared lengths (413).
pub(crate) fn framing(head: &Head, limit: usize) -> Result<Framing, HttpError> {
    let te: Vec<&str> = head.headers_named("transfer-encoding").collect();
    let cl: Vec<&str> = head.headers_named("content-length").collect();
    if !te.is_empty() {
        if !cl.is_empty() {
            // Request smuggling vector (RFC 9112 §6.1): refuse rather than guess.
            return Err(HttpError::new(400, "both Transfer-Encoding and Content-Length present"));
        }
        if head.minor_version == 0 {
            return Err(HttpError::new(400, "Transfer-Encoding in an HTTP/1.0 request"));
        }
        let codings: Vec<String> = te
            .iter()
            .flat_map(|v| v.split(','))
            .map(|c| c.trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty())
            .collect();
        return match codings.as_slice() {
            [only] if only == "chunked" => Ok(Framing::Chunked),
            [.., last] if last == "chunked" => {
                Err(HttpError::new(501, "only the 'chunked' transfer coding is supported"))
            }
            _ => Err(HttpError::new(400, "Transfer-Encoding must end with 'chunked'")),
        };
    }
    let mut length: Option<u64> = None;
    for value in cl.iter().flat_map(|v| v.split(',')) {
        let value = value.trim();
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(HttpError::new(400, format!("invalid Content-Length {value:?}")));
        }
        // Over u64 range is certainly over the limit.
        let n = value.parse::<u64>().unwrap_or(u64::MAX);
        match length {
            Some(prev) if prev != n => return Err(HttpError::new(400, "conflicting Content-Length values")),
            _ => length = Some(n),
        }
    }
    let n = length.unwrap_or(0);
    if n > limit as u64 {
        return Err(HttpError::new(413, format!("request body of {n} bytes exceeds the {limit}-byte limit")));
    }
    Ok(Framing::Length(n as usize))
}

/// Reads the body according to `framing`. Chunked bodies are decoded; trailers are discarded.
pub(crate) fn read_body<R: BufRead>(r: &mut R, framing: Framing, limit: usize) -> Result<Vec<u8>, ReadError> {
    match framing {
        Framing::Length(n) => {
            let mut body = vec![0u8; n];
            read_exact(r, &mut body)?;
            Ok(body)
        }
        Framing::Chunked => read_chunked(r, limit),
    }
}

fn read_chunked<R: BufRead>(r: &mut R, limit: usize) -> Result<Vec<u8>, ReadError> {
    let mut body = Vec::new();
    loop {
        let line = read_line(r, MAX_CHUNK_LINE, 400)?.ok_or(ReadError::Closed)?;
        let size_text = line.split(|&b| b == b';').next().unwrap_or(&[]);
        let size_text = trim_ows(size_text);
        if size_text.is_empty() || size_text.len() > 16 || !size_text.iter().all(u8::is_ascii_hexdigit) {
            return Err(HttpError::new(400, "invalid chunk size").into());
        }
        // ≤ 16 hex digits fits in u64; the slice is ASCII.
        let size = u64::from_str_radix(std::str::from_utf8(size_text).unwrap_or("0"), 16).unwrap_or(u64::MAX);
        if size == 0 {
            break;
        }
        if body.len() as u64 + size > limit as u64 {
            return Err(HttpError::new(413, format!("chunked request body exceeds the {limit}-byte limit")).into());
        }
        let start = body.len();
        body.resize(start + size as usize, 0);
        read_exact(r, &mut body[start..])?;
        match read_line(r, MAX_CHUNK_LINE, 400)? {
            Some(rest) if rest.is_empty() => {}
            Some(_) => return Err(HttpError::new(400, "missing CRLF after chunk data").into()),
            None => return Err(ReadError::Closed),
        }
    }
    // Trailer section: header lines up to an empty line.
    let mut total = 0usize;
    loop {
        let line = read_line(r, MAX_HEADER_LINE, 431)?.ok_or(ReadError::Closed)?;
        if line.is_empty() {
            return Ok(body);
        }
        total += line.len();
        if total > MAX_HEADER_BYTES {
            return Err(HttpError::new(431, "chunked trailer section too large").into());
        }
    }
}

fn read_exact<R: BufRead>(r: &mut R, buf: &mut [u8]) -> Result<(), ReadError> {
    r.read_exact(buf).map_err(|_| ReadError::Closed)
}

/// Reads one line terminated by LF (CRLF or bare LF), without the terminator.
/// `Ok(None)` on EOF before any byte; EOF mid-line is `Closed`. Longer than `max` → `too_long` status.
fn read_line<R: BufRead>(r: &mut R, max: usize, too_long: u16) -> Result<Option<Vec<u8>>, ReadError> {
    let mut line = Vec::new();
    loop {
        let buf = match r.fill_buf() {
            Ok(buf) => buf,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ReadError::Closed),
        };
        if buf.is_empty() {
            return if line.is_empty() { Ok(None) } else { Err(ReadError::Closed) };
        }
        let (take, done) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        // +2 leaves room for the CRLF terminator itself.
        if line.len() + take > max + 2 {
            return Err(HttpError::new(too_long, "line too long").into());
        }
        line.extend_from_slice(&buf[..take]);
        r.consume(take);
        if done {
            line.pop(); // \n
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

fn parse_request_line(line: &[u8]) -> Result<(String, String, u8), HttpError> {
    let bad = || HttpError::new(400, "malformed request line");
    let text = std::str::from_utf8(line).map_err(|_| bad())?;
    let mut parts = text.split(' ');
    let (Some(method), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    if method.is_empty() || !method.bytes().all(is_tchar) {
        return Err(bad());
    }
    if target.is_empty() || !target.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(bad());
    }
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        v if v.len() == 8 && v.starts_with("HTTP/") && v.as_bytes()[6] == b'.' => {
            let digits = [v.as_bytes()[5], v.as_bytes()[7]];
            if digits.iter().all(u8::is_ascii_digit) {
                return Err(HttpError::new(505, format!("unsupported HTTP version {v}")));
            }
            return Err(bad());
        }
        _ => return Err(bad()),
    };
    Ok((method.to_owned(), target.to_owned(), minor))
}

fn parse_header_line(line: &[u8]) -> Result<(String, String), HttpError> {
    if line.first().is_some_and(|&b| b == b' ' || b == b'\t') {
        return Err(HttpError::new(400, "obsolete header line folding is not accepted"));
    }
    let colon = line.iter().position(|&b| b == b':').ok_or_else(|| HttpError::new(400, "header line without ':'"))?;
    let name = &line[..colon];
    // No whitespace allowed between field name and colon (RFC 9112 §5.1).
    if name.is_empty() || !name.iter().all(|&b| is_tchar(b)) {
        return Err(HttpError::new(400, "invalid header field name"));
    }
    let value = trim_ows(&line[colon + 1..]);
    if value.iter().any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return Err(HttpError::new(400, "control character in header value"));
    }
    // Names are tchar-only ASCII; values may carry obs-text, kept lossily.
    let name = String::from_utf8_lossy(name).into_owned();
    let value = String::from_utf8_lossy(value).into_owned();
    Ok((name, value))
}

fn trim_ows(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = s {
        s = rest;
    }
    s
}

/// RFC 9110 token character.
fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

// ── responses ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Response {
    pub status: u16,
    /// Extra headers; `Content-Length` and `Connection` are added by [`Response::to_bytes`].
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Vec::new() }
    }

    pub fn with_body(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> Response {
        Response { status, headers: vec![("Content-Type".into(), content_type.into())], body: body.into() }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Serialized message; always carries `Content-Length`.
    pub fn to_bytes(&self, keep_alive: bool) -> Vec<u8> {
        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, reason(self.status));
        for (k, v) in &self.headers {
            head.push_str(k);
            head.push_str(": ");
            head.push_str(v);
            head.push_str("\r\n");
        }
        head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        head.push_str(if keep_alive { "Connection: keep-alive\r\n" } else { "Connection: close\r\n" });
        head.push_str("\r\n");
        let mut out = head.into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}

pub(crate) fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 1024;

    fn parse(raw: &[u8]) -> Result<Option<Request>, ReadError> {
        let mut r = raw;
        parse_from(&mut r)
    }

    fn parse_from(r: &mut &[u8]) -> Result<Option<Request>, ReadError> {
        let Some(head) = read_head(r)? else { return Ok(None) };
        let f = framing(&head, LIMIT)?;
        let body = read_body(r, f, LIMIT)?;
        Ok(Some(Request { head, body }))
    }

    fn status(raw: &[u8]) -> u16 {
        match parse(raw) {
            Err(ReadError::Http(e)) => e.status,
            other => panic!("expected HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn parses_post_with_content_length() {
        let req = parse(b"POST /mcp?x=1 HTTP/1.1\r\nHost: localhost\r\ncontent-LENGTH: 5\r\nX-A:  v  \r\n\r\nhello")
            .unwrap()
            .unwrap();
        assert_eq!(req.head.method, "POST");
        assert_eq!(req.head.path(), "/mcp");
        assert_eq!(req.head.minor_version, 1);
        assert_eq!(req.header("Content-Length"), Some("5"));
        assert_eq!(req.header("x-a"), Some("v"));
        assert_eq!(req.body, b"hello");
        assert!(req.head.keep_alive());
    }

    #[test]
    fn eof_between_requests_is_clean() {
        assert_eq!(parse(b""), Ok(None));
        assert_eq!(parse(b"POST /mcp HTTP/1.1\r\nHost"), Err(ReadError::Closed));
        assert_eq!(parse(b"POST /mcp HTTP/1.1\r\nContent-Length: 9\r\n\r\nshort"), Err(ReadError::Closed));
    }

    #[test]
    fn pipelined_requests_and_bare_lf() {
        let raw: &[u8] = b"\r\nGET /a HTTP/1.1\nContent-Length: 2\n\nhiDELETE /b HTTP/1.0\r\n\r\n";
        let mut r = raw;
        let a = parse_from(&mut r).unwrap().unwrap();
        assert_eq!((a.head.method.as_str(), a.body.as_slice()), ("GET", &b"hi"[..]));
        let b = parse_from(&mut r).unwrap().unwrap();
        assert_eq!((b.head.method.as_str(), b.head.minor_version), ("DELETE", 0));
        assert!(!b.head.keep_alive());
        assert_eq!(parse_from(&mut r), Ok(None));
    }

    #[test]
    fn connection_tokens() {
        let head = |h: &str, v: u8| Head {
            method: "GET".into(),
            target: "/".into(),
            minor_version: v,
            headers: vec![("Connection".into(), h.into())],
        };
        assert!(!head("Close", 1).keep_alive());
        assert!(!head("upgrade, close", 1).keep_alive());
        assert!(head("Keep-Alive", 0).keep_alive());
        assert!(!head("", 0).keep_alive());
    }

    #[test]
    fn decodes_chunked_body_with_extensions_and_trailers() {
        let raw = b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4;ext=1\r\n{\"a\"\r\nA\r\n:123456789\r\n1 \r\n}\r\n0\r\nX-Trailer: t\r\n\r\n";
        let req = parse(raw).unwrap().unwrap();
        assert_eq!(req.body, b"{\"a\":123456789}");
    }

    #[test]
    fn chunked_errors() {
        assert_eq!(status(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n"), 400);
        assert_eq!(status(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nabc\r\n0\r\n\r\n"), 400);
        assert_eq!(status(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n401\r\n"), 413);
        assert_eq!(status(b"POST / HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n"), 501);
        assert_eq!(status(b"POST / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n"), 400);
        assert_eq!(
            status(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n"),
            400
        );
    }

    #[test]
    fn content_length_rules() {
        assert_eq!(status(b"POST / HTTP/1.1\r\nContent-Length: 1025\r\n\r\n"), 413);
        assert_eq!(status(b"POST / HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n"), 413);
        assert_eq!(status(b"POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n"), 400);
        assert_eq!(status(b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n"), 400);
        let same = parse(b"POST / HTTP/1.1\r\nContent-Length: 2, 2\r\n\r\nok").unwrap().unwrap();
        assert_eq!(same.body, b"ok");
    }

    #[test]
    fn malformed_heads() {
        assert_eq!(status(b"POST /mcp\r\n\r\n"), 400);
        assert_eq!(status(b"POST  /mcp HTTP/1.1\r\n\r\n"), 400);
        assert_eq!(status(b"PO(ST /mcp HTTP/1.1\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/2.0\r\n\r\n"), 505);
        assert_eq!(status(b"POST /mcp FTP/1.1\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/1.1\r\nNoColon\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/1.1\r\nBad Name: x\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/1.1\r\nName : x\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/1.1\r\nA: b\r\n  folded\r\n\r\n"), 400);
        assert_eq!(status(b"POST /mcp HTTP/1.1\r\nA: b\x01c\r\n\r\n"), 400);
        assert_eq!(status(b"\xff\xfe /mcp HTTP/1.1\r\n\r\n"), 400);
    }

    #[test]
    fn size_limits() {
        let long_target = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_REQUEST_LINE));
        assert_eq!(status(long_target.as_bytes()), 414);
        let long_header = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(MAX_HEADER_LINE));
        assert_eq!(status(long_header.as_bytes()), 431);
        let many: String = (0..=MAX_HEADERS).map(|i| format!("X-{i}: v\r\n")).collect();
        assert_eq!(status(format!("GET / HTTP/1.1\r\n{many}\r\n").as_bytes()), 431);
    }

    #[test]
    fn expect_continue_detection() {
        let req = parse(b"POST / HTTP/1.1\r\nExpect: 100-Continue\r\n\r\n").unwrap().unwrap();
        assert!(req.head.expects_continue());
        let old = parse(b"POST / HTTP/1.0\r\nExpect: 100-continue\r\n\r\n").unwrap().unwrap();
        assert!(!old.head.expects_continue());
    }

    #[test]
    fn response_always_has_content_length() {
        let bytes = Response::new(202).to_bytes(true);
        assert_eq!(bytes, b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n");
        let bytes = Response::with_body(200, "application/json", "{}").header("Mcp-Session-Id", "ab").to_bytes(false);
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(
            text,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: ab\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
        );
    }
}

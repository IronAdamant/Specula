//! The save game container (`*.specsave`, docs/design/SAVES.md): framing, checksums, versions and
//! migrations. Pure: no engine state, no files (see `save_tools` for those).
//!
//! ```text
//! "SPECSAVE"            8 bytes, the magic
//! version               u32 LE: the save format version (VERSION)
//! header length         u32 LE, at most MAX_HEADER_BYTES
//! header                UTF-8 JSON: metadata (tick, game, hashes...) and the section table
//! sections              each section's stored bytes, in the table's order
//! crc32                 u32 LE of everything before it
//! ```
//!
//! The header's `sections` table lists each section's `name`, `codec` (`zlib` or `raw`), `bytes`
//! (its decoded length), `stored` (its length in the file) and `crc32` (of the decoded bytes), so
//! a damaged file names the section that is damaged. Reading is bounded: the file, the header,
//! the number of sections and each decoded section have caps, decompression stops at the
//! declared length, and every failure is a structured [`SaveError`].
//!
//! **Versions.** The framing above never changes. What a version changes is the header's fields
//! and the sections' contents; each change comes with a pure migration from version N to N+1
//! ([`MIGRATIONS`]), and reading runs the chain, so an old save always loads forward. A save from
//! a newer build is refused (`unsupported_version`), never guessed at.

use specula_core::bytes::{ByteReader, ByteWriter};
use specula_core::checksum::crc32;
use specula_core::deflate::{self, Level};
use specula_core::Json;

pub const MAGIC: &[u8; 8] = b"SPECSAVE";
/// The save format version this build writes. Version 2: the world section's files are
/// `specula.world/v2` (each entity's identity lineage, `born`; docs/design/EDITOR.md).
pub const VERSION: u32 = 2;
pub const FORMAT: &str = "specula.save";
pub const EXTENSION: &str = "specsave";
/// Files read at most this big.
pub const MAX_FILE_BYTES: u64 = 1 << 30;
pub const MAX_HEADER_BYTES: usize = 1 << 20;
pub const MAX_SECTIONS: usize = 32;
/// A decoded section at most this big, and all of a save's sections together at most
/// [`MAX_DECODED_BYTES`] (checked from the table before anything is decompressed).
pub const MAX_SECTION_BYTES: usize = 512 << 20;
pub const MAX_DECODED_BYTES: u64 = 512 << 20;
/// Decoded bytes per byte of file, at most (plus [`MAX_RATIO_SLACK`]): world text compresses
/// about 20 to 1 and very repetitive worlds (many entities with the same big props) a few
/// hundred to 1, so this leaves room while refusing decompression bombs (DEFLATE reaches about
/// 1000 to 1). Decoding runs on a worker and the total is capped at [`MAX_DECODED_BYTES`] too.
pub const MAX_RATIO: u64 = 1024;
pub const MAX_RATIO_SLACK: u64 = 16 << 20;
/// Sections smaller than this are stored raw (compressing them saves nothing).
const RAW_BELOW: usize = 256;

/// Why a save couldn't be read or written: a machine-readable `code`, a message that says what to
/// do, and where (the section and byte offset) when known.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveError {
    pub code: &'static str,
    pub message: String,
    pub section: Option<String>,
    pub offset: Option<usize>,
    pub details: Vec<(&'static str, Json)>,
}

impl SaveError {
    pub fn new(code: &'static str, message: impl Into<String>) -> SaveError {
        SaveError { code, message: message.into(), section: None, offset: None, details: Vec::new() }
    }

    pub fn at(mut self, offset: usize) -> SaveError {
        self.offset = Some(offset);
        self
    }

    pub fn in_section(mut self, s: &str) -> SaveError {
        self.section = Some(s.to_owned());
        self
    }

    pub fn with(mut self, k: &'static str, v: impl Into<Json>) -> SaveError {
        self.details.push((k, v.into()));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        if let Some(s) = &self.section {
            f.push(("section", Json::from(s.as_str())));
        }
        if let Some(o) = self.offset {
            f.push(("offset", Json::from(o)));
        }
        f.extend(self.details.iter().cloned());
        Json::obj(f)
    }
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// One named part of a save (`world`, `game`, `physics`, `logic`...).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub data: Vec<u8>,
}

/// A decoded save: its version (after migrations, always [`VERSION`]), the header's metadata
/// (everything but the section table) and the sections.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveFile {
    pub version: u32,
    pub meta: Json,
    pub sections: Vec<Section>,
}

impl SaveFile {
    pub fn section(&self, name: &str) -> Option<&[u8]> {
        self.sections.iter().find(|s| s.name == name).map(|s| s.data.as_slice())
    }
}

/// A pure step from one version's save to the next one's.
pub type Migration = fn(SaveFile) -> Result<SaveFile, SaveError>;

/// `MIGRATIONS[i]` turns a version `i + 1` save into version `i + 2`. Reading a committed version
/// 1 save: `games/sepulcra-app/tests/saves.rs` (`crash_1_57.specsave`).
pub const MIGRATIONS: &[Migration] = &[v1_to_v2];

/// Version 1 to 2: nothing in the file changes. Its world files are `specula.world/v1`, which
/// still load: their entities get fresh identities in the loading world's lineage, so notes and
/// critiques on them from before detach (a version 1 save never recorded identities).
fn v1_to_v2(file: SaveFile) -> Result<SaveFile, SaveError> {
    Ok(SaveFile { version: 2, ..file })
}

/// Runs `table` from `file.version` up to `target`.
pub fn migrate_with(mut file: SaveFile, table: &[Migration], target: u32) -> Result<SaveFile, SaveError> {
    if file.version > target {
        return Err(SaveError::new(
            "unsupported_version",
            format!("the save is format version {}, newer than this build reads ({target}): load it with a newer engine", file.version),
        )
        .with("version", file.version)
        .with("supported", target));
    }
    if file.version == 0 {
        return Err(SaveError::new("invalid", "save format versions start at 1").with("version", 0));
    }
    while file.version < target {
        let Some(step) = table.get(file.version as usize - 1) else {
            return Err(SaveError::new("no_migration", format!("no migration from save format version {}", file.version)).with("version", file.version));
        };
        let from = file.version;
        file = step(file)?;
        if file.version != from + 1 {
            return Err(SaveError::new("internal", format!("the migration from version {from} produced version {}", file.version)));
        }
    }
    Ok(file)
}

/// The file bytes for `meta` (an object; its `sections` key is replaced) and `sections`.
pub fn encode(meta: &Json, sections: &[Section]) -> Vec<u8> {
    let compressed = encode_with(meta, sections, true);
    // A save so repetitive it compresses past the ratio loads refuse is stored raw instead.
    let declared: u64 = sections.iter().map(|s| s.data.len() as u64).sum();
    if declared > MAX_RATIO_SLACK + MAX_RATIO * compressed.len() as u64 {
        return encode_with(meta, sections, false);
    }
    compressed
}

fn encode_with(meta: &Json, sections: &[Section], compress: bool) -> Vec<u8> {
    let mut table = Vec::new();
    let mut body = Vec::new();
    for s in sections {
        let (codec, stored) = if s.data.len() < RAW_BELOW || !compress { ("raw", s.data.clone()) } else { ("zlib", deflate::zlib_compress(&s.data, Level::Fast)) };
        table.push(Json::obj([
            ("name", Json::from(s.name.as_str())),
            ("codec", Json::from(codec)),
            ("bytes", Json::from(s.data.len())),
            ("stored", Json::from(stored.len())),
            ("crc32", Json::from(format!("{:08x}", crc32(&s.data)))),
        ]));
        body.extend_from_slice(&stored);
    }
    let mut header = match meta {
        Json::Obj(f) => Json::Obj(f.iter().filter(|(k, _)| k != "sections").cloned().collect()),
        _ => Json::Obj(Vec::new()),
    };
    header.set("sections", Json::Arr(table));
    let text = header.to_string();
    let mut w = ByteWriter::with_capacity(16 + text.len() + body.len() + 4);
    w.raw(MAGIC);
    w.u32(VERSION);
    w.len_u32(text.len());
    w.raw(text.as_bytes());
    w.raw(&body);
    let crc = crc32(w.bytes());
    w.u32(crc);
    w.into_bytes()
}

fn framing(bytes: &[u8]) -> Result<(u32, Json, usize), SaveError> {
    if bytes.len() < 20 {
        return Err(SaveError::new("truncated", format!("{} bytes is too short to be a save", bytes.len())).at(0));
    }
    if &bytes[..8] != MAGIC {
        return Err(SaveError::new("not_a_save", "the file doesn't start with SPECSAVE: it isn't a Specula save game").at(0));
    }
    let mut r = ByteReader::new(&bytes[8..]);
    let version = r.u32("version").map_err(|e| SaveError::new("truncated", e.to_string()).at(8))?;
    let len = r.u32("header length").map_err(|e| SaveError::new("truncated", e.to_string()).at(12))? as usize;
    if len > MAX_HEADER_BYTES {
        return Err(SaveError::new("too_large", format!("the header claims {len} bytes; at most {MAX_HEADER_BYTES}")).at(12));
    }
    let start = 16;
    let Some(text) = bytes.get(start..start + len) else {
        return Err(SaveError::new("truncated", format!("the header claims {len} bytes but the file ends first")).at(start));
    };
    let text = std::str::from_utf8(text).map_err(|e| SaveError::new("invalid_header", format!("the header isn't UTF-8: {e}")).at(start))?;
    let header = Json::parse(text).map_err(|e| SaveError::new("invalid_header", format!("the header isn't JSON: {e}")).at(start))?;
    if header.as_object().is_none() {
        return Err(SaveError::new("invalid_header", "the header must be a JSON object").at(start));
    }
    Ok((version, header, start + len))
}

/// The header alone (for listings): checks the magic and the header, not the checksum or the
/// sections.
pub fn peek(bytes: &[u8]) -> Result<(u32, Json), SaveError> {
    framing(bytes).map(|(v, h, _)| (v, h))
}

/// One row of a save's section table, checked against the file (nothing decompressed).
#[derive(Clone, Debug, PartialEq)]
pub struct TableEntry {
    pub name: String,
    pub codec: String,
    /// Decoded length.
    pub bytes: u64,
    /// Length in the file.
    pub stored: u64,
    /// CRC-32 of the decoded bytes.
    pub crc32: u32,
}

/// Checks a header's section table against a file of `file_len` bytes whose sections start at
/// `at` and end at `end` (where the checksum starts): names, codecs, sizes, the decoded total
/// and ratio caps, and that the stored bytes exactly fill the space. Nothing is decompressed.
fn read_table(header: &Json, at: usize, end: usize, file_len: u64) -> Result<Vec<TableEntry>, SaveError> {
    if at > end {
        return Err(SaveError::new("truncated", "the header runs into the checksum").at(at));
    }
    let table_json = header.get("sections").and_then(Json::as_array).ok_or_else(|| SaveError::new("invalid_header", "the header has no 'sections' table").at(16))?;
    if table_json.len() > MAX_SECTIONS {
        return Err(SaveError::new("too_large", format!("{} sections; at most {MAX_SECTIONS}", table_json.len())).at(16));
    }
    let declared: u64 = table_json.iter().map(|s| s.get("bytes").and_then(Json::as_u64).unwrap_or(0)).fold(0u64, u64::saturating_add);
    if declared > MAX_DECODED_BYTES {
        return Err(SaveError::new("too_large", format!("the sections decode to {declared} bytes in all; a save is read up to {MAX_DECODED_BYTES}")).at(16));
    }
    // Real saves compress a few hundred to 1 at most: a file that claims far more is a bomb.
    let ratio_cap = MAX_RATIO_SLACK.saturating_add(MAX_RATIO.saturating_mul(file_len));
    if declared > ratio_cap {
        return Err(SaveError::new("too_large", format!("the sections claim {declared} bytes from a {file_len}-byte file; saves decode to at most {MAX_RATIO} times their size (plus {MAX_RATIO_SLACK} bytes)")).at(16));
    }
    let mut out: Vec<TableEntry> = Vec::with_capacity(table_json.len());
    let mut pos = at as u64;
    for (i, s) in table_json.iter().enumerate() {
        let bad = |m: String| SaveError::new("invalid_header", format!("sections[{i}]: {m}")).at(16);
        let name = s.get("name").and_then(Json::as_str).filter(|n| !n.is_empty() && n.len() <= 64).ok_or_else(|| bad("'name' must be 1..64 characters".into()))?;
        if out.iter().any(|x| x.name == name) {
            return Err(bad(format!("section '{name}' is listed twice")));
        }
        let codec = s.get("codec").and_then(Json::as_str).ok_or_else(|| bad("'codec' is missing".into()))?;
        let size = s.get("bytes").and_then(Json::as_u64).ok_or_else(|| bad("'bytes' must be a whole number".into()))?;
        let stored = s.get("stored").and_then(Json::as_u64).ok_or_else(|| bad("'stored' must be a whole number".into()))?;
        let crc = s.get("crc32").and_then(Json::as_str).and_then(|h| u32::from_str_radix(h, 16).ok()).ok_or_else(|| bad("'crc32' must be 8 hex digits".into()))?;
        if size > MAX_SECTION_BYTES as u64 {
            return Err(SaveError::new("too_large", format!("section '{name}' claims {size} bytes; at most {MAX_SECTION_BYTES}")).in_section(name));
        }
        if !matches!(codec, "raw" | "zlib") {
            return Err(SaveError::new("unknown_codec", format!("section '{name}' uses codec '{codec}'; this build reads raw and zlib")).in_section(name));
        }
        if codec == "raw" && stored != size {
            return Err(SaveError::new("corrupt_section", format!("raw section '{name}' stores {stored} bytes but lists {size}")).in_section(name));
        }
        if pos.checked_add(stored).is_none_or(|e| e > end as u64) {
            return Err(SaveError::new("truncated", format!("section '{name}' claims {stored} stored bytes but the file ends first")).in_section(name).at(pos as usize));
        }
        pos += stored;
        out.push(TableEntry { name: name.to_owned(), codec: codec.to_owned(), bytes: size, stored, crc32: crc });
    }
    if pos != end as u64 {
        return Err(SaveError::new("trailing", format!("{} bytes between the last section and the checksum", end as u64 - pos)).at(pos as usize));
    }
    Ok(out)
}

/// What [`inspect`] found: the version, the header's metadata and the section table.
#[derive(Clone, Debug, PartialEq)]
pub struct Inspected {
    pub version: u32,
    pub meta: Json,
    pub table: Vec<TableEntry>,
}

/// Checks a save **without decompressing anything**, streaming the file: the framing, the
/// header, the section table (sizes, caps, the stored bytes filling the file) and the file's
/// checksum over every stored byte. Memory is the header plus a small buffer, whatever the
/// file's size. What it can't see: a section whose stored bytes are intact but don't decode to
/// what the table says (that takes a load).
pub fn inspect(r: &mut impl std::io::Read, file_len: u64) -> Result<Inspected, SaveError> {
    let io = |e: std::io::Error| SaveError::new("truncated", format!("the file ends early: {e}"));
    if file_len < 20 {
        return Err(SaveError::new("truncated", format!("{file_len} bytes is too short to be a save")).at(0));
    }
    let mut head = [0u8; 16];
    r.read_exact(&mut head).map_err(io)?;
    if &head[..8] != MAGIC {
        return Err(SaveError::new("not_a_save", "the file doesn't start with SPECSAVE: it isn't a Specula save game").at(0));
    }
    let version = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    if len > MAX_HEADER_BYTES {
        return Err(SaveError::new("too_large", format!("the header claims {len} bytes; at most {MAX_HEADER_BYTES}")).at(12));
    }
    if 16 + len as u64 + 4 > file_len {
        return Err(SaveError::new("truncated", format!("the header claims {len} bytes but the file ends first")).at(16));
    }
    let mut text = vec![0u8; len];
    r.read_exact(&mut text).map_err(io)?;
    let mut crc = specula_core::checksum::crc32_update(specula_core::checksum::crc32_update(0, &head), &text);
    let mut framed = head.to_vec();
    framed.extend_from_slice(&text);
    framed.extend_from_slice(&[0, 0, 0, 0]);
    drop(text);
    let (version_again, header, at) = framing(&framed)?;
    debug_assert_eq!(version, version_again);
    if version > VERSION {
        return Err(SaveError::new("unsupported_version", format!("save format version {version}; this build reads up to {VERSION}: update Specula to load it")));
    }
    let end = usize::try_from(file_len - 4).map_err(|_| SaveError::new("too_large", "the file is too big for this machine"))?;
    let table = read_table(&header, at, end, file_len)?;
    // The checksum over every stored byte, streamed.
    let mut left = file_len - 4 - at as u64;
    let mut buf = vec![0u8; 1 << 16];
    while left > 0 {
        let n = (left as usize).min(buf.len());
        r.read_exact(&mut buf[..n]).map_err(io)?;
        crc = specula_core::checksum::crc32_update(crc, &buf[..n]);
        left -= n as u64;
    }
    let mut tail = [0u8; 4];
    r.read_exact(&mut tail).map_err(io)?;
    let stored_crc = u32::from_le_bytes(tail);
    if crc != stored_crc {
        return Err(SaveError::new("checksum", format!("the file's checksum is {crc:08x} but it says {stored_crc:08x}: the save is damaged (cut short or changed)"))
            .at(end)
            .with("expected", format!("{stored_crc:08x}"))
            .with("found", format!("{crc:08x}")));
    }
    let meta = match header {
        Json::Obj(f) => Json::Obj(f.into_iter().filter(|(k, _)| k != "sections").collect()),
        other => other,
    };
    Ok(Inspected { version, meta, table })
}

/// Reads a save: framing, checksum, every section (decompressed within its declared size and
/// checked against its CRC), then the migrations up to [`VERSION`].
pub fn decode(bytes: &[u8]) -> Result<SaveFile, SaveError> {
    decode_with(bytes, MIGRATIONS, VERSION)
}

fn decode_with(bytes: &[u8], table: &[Migration], target: u32) -> Result<SaveFile, SaveError> {
    let (version, header, mut at) = framing(bytes)?;
    if version > target {
        // Before the checksum: a newer layout may differ in more than the header.
        return migrate_with(SaveFile { version, meta: Json::Null, sections: Vec::new() }, table, target);
    }
    let end = bytes.len() - 4;
    let stored_crc = u32::from_le_bytes([bytes[end], bytes[end + 1], bytes[end + 2], bytes[end + 3]]);
    let crc = crc32(&bytes[..end]);
    if crc != stored_crc {
        return Err(SaveError::new("checksum", format!("the file's checksum is {crc:08x} but it says {stored_crc:08x}: the save is damaged (cut short or changed)"))
            .at(end)
            .with("expected", format!("{stored_crc:08x}"))
            .with("found", format!("{crc:08x}")));
    }
    let rows = read_table(&header, at, end, bytes.len() as u64)?;
    let mut sections: Vec<Section> = Vec::with_capacity(rows.len());
    for t in &rows {
        let name = t.name.as_str();
        let raw = &bytes[at..at + t.stored as usize];
        let data = match t.codec.as_str() {
            "raw" => raw.to_vec(),
            _ => deflate::zlib_decompress_with_limit(raw, t.bytes as usize)
                .map_err(|e| SaveError::new("corrupt_section", format!("section '{name}' doesn't decompress: {e:?}")).in_section(name).at(at))?,
        };
        if data.len() as u64 != t.bytes {
            return Err(SaveError::new("corrupt_section", format!("section '{name}' decodes to {} bytes, not the {} listed", data.len(), t.bytes)).in_section(name).at(at));
        }
        if crc32(&data) != t.crc32 {
            return Err(SaveError::new("corrupt_section", format!("section '{name}' fails its checksum")).in_section(name).at(at));
        }
        at += raw.len();
        sections.push(Section { name: name.to_owned(), data });
    }
    if at != end {
        return Err(SaveError::new("trailing", format!("{} bytes between the last section and the checksum", end - at)).at(at));
    }
    let meta = match header {
        Json::Obj(f) => Json::Obj(f.into_iter().filter(|(k, _)| k != "sections").collect()),
        other => other,
    };
    migrate_with(SaveFile { version, meta, sections }, table, target)
}

/// Files (name, text) packed into one section (the world's files).
pub fn pack_files(files: &[(String, String)]) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.len_u32(files.len());
    for (name, text) in files {
        w.str(name);
        w.str(text);
    }
    w.into_bytes()
}

/// Reads [`pack_files`]'s bytes.
pub fn unpack_files(bytes: &[u8]) -> Result<Vec<(String, String)>, SaveError> {
    let mut r = ByteReader::new(bytes);
    let err = |e: specula_core::bytes::DecodeError| SaveError::new("corrupt_section", format!("the world section is damaged: {e}")).in_section("world").at(e.offset);
    let n = r.count("files", 1 << 20, 8).map_err(err)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str("file.name", 512).map_err(err)?;
        let text = r.str("file.text", MAX_SECTION_BYTES).map_err(err)?;
        out.push((name, text));
    }
    r.finish().map_err(err)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Json, Vec<Section>) {
        let meta = Json::obj([("format", Json::from(FORMAT)), ("tick", Json::from(1234u64)), ("game", Json::from("sepulcra"))]);
        let big: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8 ^ (i / 97) as u8).collect();
        let sections = vec![
            Section { name: "world".into(), data: pack_files(&[("world.json".into(), "{\"format\": \"specula.world/v1\"}".into())]) },
            Section { name: "game".into(), data: big },
            Section { name: "empty".into(), data: Vec::new() },
        ];
        (meta, sections)
    }

    #[test]
    fn saves_round_trip() {
        let (meta, sections) = sample();
        let bytes = encode(&meta, &sections);
        let f = decode(&bytes).unwrap();
        assert_eq!(f.version, VERSION);
        assert_eq!(f.meta.get("tick").and_then(Json::as_u64), Some(1234));
        assert!(f.meta.get("sections").is_none());
        assert_eq!(f.sections, sections);
        let files = unpack_files(f.section("world").unwrap()).unwrap();
        assert_eq!(files[0].0, "world.json");
        // Encoding is deterministic.
        assert_eq!(encode(&meta, &sections), bytes);
        let (v, h) = peek(&bytes).unwrap();
        assert_eq!((v, h.get("game").and_then(Json::as_str)), (VERSION, Some("sepulcra")));
    }

    #[test]
    fn damage_is_named_never_a_panic() {
        let (meta, sections) = sample();
        let bytes = encode(&meta, &sections);
        // Every truncation.
        for n in 0..bytes.len() {
            let e = decode(&bytes[..n]).unwrap_err();
            assert!(["truncated", "checksum", "not_a_save", "invalid_header", "too_large"].contains(&e.code), "{n}: {e}");
        }
        // Not a save at all.
        assert_eq!(decode(b"PNG\x0d\x0a\x1a\x0a and some more bytes here").unwrap_err().code, "not_a_save");
        // One flipped bit anywhere is caught (by the checksum, or before it).
        for i in (0..bytes.len()).step_by(3) {
            let mut x = bytes.clone();
            x[i] ^= 0x10;
            assert!(decode(&x).is_err(), "flip at {i} went unnoticed");
        }
        // A consistent-looking file (fixed checksum) with a damaged section names the section.
        let mut x = bytes.clone();
        let (_, _, start) = framing(&x).unwrap();
        x[start + 3] ^= 0xff;
        let end = x.len() - 4;
        let crc = crc32(&x[..end]);
        x[end..].copy_from_slice(&crc.to_le_bytes());
        let e = decode(&x).unwrap_err();
        assert_eq!((e.code, e.section.as_deref()), ("corrupt_section", Some("world")), "{e}");
    }

    /// Fuzz: random mutations, insertions and cuts, with the checksum fixed up so the parser
    /// behind it is exercised too. Nothing panics, nothing allocates past the caps.
    #[test]
    fn fuzzed_saves_fail_cleanly() {
        let (meta, sections) = sample();
        let base = encode(&meta, &sections);
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut errors = std::collections::BTreeMap::new();
        for round in 0..3000 {
            let mut x = base.clone();
            let edits = 1 + next() % 4;
            for _ in 0..edits {
                let i = (next() as usize) % x.len().max(1);
                match next() % 5 {
                    0 => x[i] = next() as u8,
                    1 => x.truncate(i.max(1)),
                    2 => x.insert(i, next() as u8),
                    3 => {
                        // Rewrite a header digit (lengths and counts).
                        if let Some(p) = x.iter().skip(16).position(|b| b.is_ascii_digit()) {
                            let p = p + 16 + (next() as usize % 64);
                            if p < x.len() && x[p].is_ascii_digit() {
                                x[p] = b'0' + (next() % 10) as u8;
                            }
                        }
                    }
                    _ => {
                        let n = (next() % 8) as usize;
                        x.extend((0..n).map(|_| next() as u8));
                    }
                }
            }
            if x.len() >= 4 && round % 2 == 0 {
                let end = x.len() - 4;
                let crc = crc32(&x[..end]);
                x[end..].copy_from_slice(&crc.to_le_bytes());
            }
            if let Err(e) = decode(&x) {
                *errors.entry(e.code).or_insert(0) += 1;
                assert!(!e.message.is_empty());
            }
        }
        assert!(errors.len() >= 4, "the fuzzer should reach several checks: {errors:?}");
    }

    /// The review's bomb: a half-megabyte file whose world section inflates to hundreds of MiB
    /// is refused by its compression ratio, before anything is inflated.
    #[test]
    fn decompression_bombs_are_refused_by_ratio() {
        let data = vec![0u8; 200 << 20];
        let stored = deflate::zlib_compress(&data, Level::Fast);
        let table = Json::arr([Json::obj([
            ("name", Json::from("world")),
            ("codec", Json::from("zlib")),
            // More than the stored bytes could ever decode to (DEFLATE tops out near 1032:1).
            ("bytes", Json::from(500u64 << 20)),
            ("stored", Json::from(stored.len())),
            ("crc32", Json::from(format!("{:08x}", crc32(&data)))),
        ])]);
        drop(data);
        let header = Json::obj([("format", Json::from(FORMAT)), ("sections", table)]).to_string();
        let mut w = ByteWriter::new();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.len_u32(header.len());
        w.raw(header.as_bytes());
        w.raw(&stored);
        let crc = crc32(w.bytes());
        w.u32(crc);
        assert!(w.len() < 1 << 20, "a small file");
        let Err(e) = decode(w.bytes()) else { panic!("the bomb must be refused") };
        assert_eq!(e.code, "too_large", "{e}");
        assert!(e.message.contains("times their size"), "refused by the ratio, before inflating: {e}");
        let Err(e) = inspect(&mut std::io::Cursor::new(w.bytes()), w.len() as u64) else { panic!("inspect refuses it too") };
        assert_eq!(e.code, "too_large", "{e}");
    }

    /// Very repetitive data still loads (stored raw only past what loads accept).
    #[test]
    fn repetitive_saves_still_load() {
        let meta = Json::obj([("format", Json::from(FORMAT))]);
        let sections = vec![Section { name: "world".into(), data: vec![b'a'; 40 << 20] }];
        let bytes = encode(&meta, &sections);
        assert!(decode(&bytes).unwrap().sections == sections);
    }

    /// The review's case: 3,000 entities with the same 16 KB prop (48 MB of world text that
    /// compresses about 500 to 1) is written compressed, not raw: a small file that loads.
    #[test]
    fn repetitive_worlds_stay_compressed() {
        let big = "x".repeat(16_000);
        let mut text = String::new();
        for i in 0..3000 {
            text.push_str(&format!("{{\"name\": \"e{i}\", \"position\": [{i}, 0, 0], \"props\": {{\"p\": \"{big}\"}}}}\n"));
        }
        let sections = vec![Section { name: "world".into(), data: text.into_bytes() }];
        let bytes = encode(&Json::obj([("format", Json::from(FORMAT))]), &sections);
        assert!(bytes.len() < 2 << 20, "{} bytes for {} of text", bytes.len(), sections[0].data.len());
        assert!(decode(&bytes).unwrap().sections == sections);
    }

    /// `inspect` streams a file and checks its framing, table and checksum without inflating:
    /// it describes a good save, names damage, and passes a section whose stored bytes are
    /// intact even when they wouldn't decode (that takes a load).
    #[test]
    fn inspect_checks_without_inflating() {
        let (meta, sections) = sample();
        let bytes = encode(&meta, &sections);
        let i = inspect(&mut std::io::Cursor::new(&bytes), bytes.len() as u64).unwrap();
        assert_eq!(i.version, VERSION);
        assert_eq!(i.meta.get("tick").and_then(Json::as_u64), Some(1234));
        assert_eq!(i.table.iter().map(|t| (t.name.as_str(), t.bytes)).collect::<Vec<_>>(), sections.iter().map(|s| (s.name.as_str(), s.data.len() as u64)).collect::<Vec<_>>());
        // Damage anywhere is the checksum's (or the framing's).
        for at in [20, bytes.len() / 2, bytes.len() - 6] {
            let mut x = bytes.clone();
            x[at] ^= 0x40;
            assert!(inspect(&mut std::io::Cursor::new(&x), x.len() as u64).is_err(), "byte {at}");
        }
        for cut in [0, 10, 19, bytes.len() / 2, bytes.len() - 1] {
            assert!(inspect(&mut std::io::Cursor::new(&bytes[..cut]), cut as u64).is_err(), "cut at {cut}");
        }
        // A zlib section whose stream is garbage but whose stored bytes carry a good file
        // checksum: inspect (no inflating) passes it, a load refuses it.
        let (v, header, at) = framing(&bytes).unwrap();
        assert_eq!(v, VERSION);
        let game = i.table.iter().position(|t| t.name == "game").unwrap();
        let off = at + i.table[..game].iter().map(|t| t.stored as usize).sum::<usize>();
        let mut x = bytes.clone();
        for b in &mut x[off + 2..off + 40] {
            *b = 0xFF;
        }
        let end = x.len() - 4;
        let crc = crc32(&x[..end]);
        x[end..].copy_from_slice(&crc.to_le_bytes());
        assert!(header.get("sections").is_some());
        assert!(inspect(&mut std::io::Cursor::new(&x), x.len() as u64).is_ok());
        assert_eq!(decode(&x).map(|_| ()).unwrap_err().code, "corrupt_section");
    }

    /// A small file whose table promises huge sections is refused before anything is inflated.
    #[test]
    fn the_decoded_total_is_capped_before_decompressing() {
        let table: Vec<Json> = (0..4)
            .map(|i| {
                Json::obj([
                    ("name", Json::from(format!("s{i}"))),
                    ("codec", Json::from("zlib")),
                    ("bytes", Json::from(256u64 << 20)),
                    ("stored", Json::from(0u64)),
                    ("crc32", Json::from("00000000")),
                ])
            })
            .collect();
        let header = Json::obj([("format", Json::from(FORMAT)), ("sections", Json::Arr(table))]).to_string();
        let mut w = ByteWriter::new();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.len_u32(header.len());
        w.raw(header.as_bytes());
        let crc = crc32(w.bytes());
        w.u32(crc);
        let e = decode(w.bytes()).unwrap_err();
        assert_eq!(e.code, "too_large", "{e}");
    }

    #[test]
    fn versions_migrate_forward_and_newer_ones_are_refused() {
        fn v1_to_v2(mut f: SaveFile) -> Result<SaveFile, SaveError> {
            f.meta.set("renamed", Json::from(true));
            f.version = 2;
            Ok(f)
        }
        fn v2_to_v3(mut f: SaveFile) -> Result<SaveFile, SaveError> {
            f.sections.push(Section { name: "added_in_v3".into(), data: vec![3] });
            f.version = 3;
            Ok(f)
        }
        let table: &[Migration] = &[v1_to_v2, v2_to_v3];
        let (meta, sections) = sample();
        let bytes = as_version(&encode(&meta, &sections), 1);
        let f = decode_with(&bytes, table, 3).unwrap();
        assert_eq!(f.version, 3);
        assert_eq!(f.meta.get("renamed"), Some(&Json::Bool(true)));
        assert_eq!(f.section("added_in_v3"), Some(&[3u8][..]));
        // A gap in the chain is an error, not a guess.
        assert_eq!(decode_with(&bytes, &[v1_to_v2], 3).unwrap_err().code, "no_migration");
        // A save newer than the build is refused.
        let mut newer = bytes.clone();
        newer[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
        assert_eq!(decode(&newer).unwrap_err().code, "unsupported_version");
    }

    /// `bytes` relabelled as format version `v` (checksum updated).
    fn as_version(bytes: &[u8], v: u32) -> Vec<u8> {
        let mut out = bytes.to_vec();
        out[8..12].copy_from_slice(&v.to_le_bytes());
        let end = out.len() - 4;
        let crc = crc32(&out[..end]);
        out[end..].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// Version 2 (identities in the world files) reads version 1 saves unchanged: the migration
    /// only relabels them (their world files are specula.world/v1, which load with fresh
    /// identities).
    #[test]
    fn a_version_1_save_reads_as_version_2_with_its_sections_unchanged() {
        assert_eq!(VERSION, 2);
        let (meta, sections) = sample();
        let f = decode(&as_version(&encode(&meta, &sections), 1)).unwrap();
        assert_eq!(f.version, 2);
        assert_eq!(f.sections, sections);
        assert_eq!(f.meta.get("tick"), meta.get("tick"));
    }
}

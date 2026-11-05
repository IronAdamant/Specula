//! Binary project files through the protocol: `project_upload` (chunked, resumable, CRC-checked,
//! validated by kind), binary reads for `project_read` (metadata by default, bytes on request) and
//! the checks each kind gets. Design: docs/design/PROJECT_DATA.md ("Binary assets").
//!
//! A binary file is committed like a text file (the same `ProjectCall::Commit`: claims, versions,
//! the journal, undo, disk sync), with its pointer text ([`crate::blobs::pointer`]) as the text.
//! Its bytes go into the content store first, so the commit itself is small.

use crate::blobs::{self, BlobId, ContentStore, Upload, MAX_CHUNK_BYTES};
use crate::project::FileKind;
use crate::project_kinds::Problem;
use crate::project_tools::{snapshot, Committed, ProjectCall, Snap};
use crate::tools::{bool_arg, check_keys, string_arg, Command, Tools};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolOutput};
use std::sync::{mpsc, Arc, Mutex};

/// Largest image side and image (pixels) a PNG may have: decoding it to check it holds the inflated
/// scanlines and the RGBA image (4 bytes a pixel each) and the file, so this bounds the check's
/// peak memory: about 384 MiB at 8192 x 4096. One check runs at a time, and its result is kept
/// per content id (a read never decodes the same file twice).
pub const MAX_IMAGE_SIDE: u32 = 8192;
pub const MAX_IMAGE_PIXELS: u64 = 32 << 20;
/// Most bytes one `project_read {bytes}` returns (before base64).
pub const MAX_READ_BYTES: usize = 192 * 1024;
const DEFAULT_READ_BYTES: usize = 64 * 1024;
/// Tries before a commit gives up on a file others keep changing.
const COMMIT_ATTEMPTS: usize = 4;

/// One check of a binary file at a time (a PNG's check can take hundreds of MiB).
static CHECKING: Mutex<()> = Mutex::new(());
/// The most an `.ogg` upload may decode to, in samples (frames × channels): 30 minutes of stereo
/// at 48 kHz. Counting samples, not frames, keeps a file that claims 8 channels at 384 kHz from
/// costing 16 times more to check (review round 2: one held the check's lock for 17.6 s).
pub const MAX_OGG_SAMPLES: u64 = 30 * 60 * 48_000 * 2;
/// The longest an `.ogg` check may decode for (it holds the upload check's lock).
pub const OGG_CHECK_TIME: std::time::Duration = std::time::Duration::from_secs(20);

/// Decodes an `.ogg` whole, through one small buffer, refusing it past `max_samples` decoded
/// samples or `max_time` of decoding, when it can't be streamed (more samples a second than
/// streams take), or when it decodes to nothing. Returns the decoder and the frames decoded.
fn check_ogg(path: &str, bytes: &[u8], max_samples: u64, max_time: std::time::Duration) -> Result<(specula_audio::vorbis::Decoder, u64), Vec<Problem>> {
    let invalid = |e: specula_audio::AudioError| vec![problem(path, "ogg_invalid", e.to_string())];
    let mut d = specula_audio::vorbis::Decoder::new(bytes.to_vec()).map_err(invalid)?;
    let ch = d.channels();
    let rate = d.info().rate;
    let per_second = u64::from(rate) * ch as u64;
    if per_second > specula_audio::stream::MAX_STREAM_SAMPLES_PER_S {
        return Err(vec![problem(path, "ogg_too_dense", format!("{ch} channels at {rate} Hz is more than a stream plays ({} samples a second at most: 8 channels at 96 kHz)", specula_audio::stream::MAX_STREAM_SAMPLES_PER_S))]);
    }
    let start = std::time::Instant::now();
    // Streamed through one small buffer: nothing the size of the file's audio is held.
    let mut buf = vec![0f32; 4096 * ch];
    let mut frames = 0u64;
    loop {
        let n = d.read(&mut buf).map_err(invalid)?;
        if n == 0 {
            break;
        }
        frames += n as u64;
        if frames * ch as u64 > max_samples {
            return Err(vec![problem(path, "ogg_too_long", format!("decodes to more than {max_samples} samples (30 minutes of stereo at 48 kHz)"))]);
        }
        if start.elapsed() > max_time {
            return Err(vec![problem(path, "ogg_too_slow", format!("checking it took longer than {} s", max_time.as_secs()))]);
        }
    }
    // Valid headers over garbage decode to nothing: not an audio file (review probe).
    if frames == 0 {
        return Err(vec![problem(path, "ogg_empty", "the file's headers are valid but it decodes to no audio")]);
    }
    Ok((d, frames))
}
/// Checks done, by content id (and kind): `project_read` answers from here. Bounded.
/// A check's result: what the file is, or its problems.
type Checked = Result<Json, Vec<Problem>>;
static CHECKED: Mutex<Vec<((BlobId, FileKind), Checked)>> = Mutex::new(Vec::new());
const CHECKED_KEEP: usize = 256;

fn checked(id: BlobId, kind: FileKind) -> Option<Checked> {
    CHECKED.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(k, _)| *k == (id, kind)).map(|(_, r)| r.clone())
}

/// Forgets the checks of content `id` (pruned, or a damaged slot rewritten).
pub fn forget_checked(id: BlobId) {
    CHECKED.lock().unwrap_or_else(|p| p.into_inner()).retain(|(k, _)| k.0 != id);
}

fn remember(id: BlobId, kind: FileKind, r: &Checked) {
    let mut c = CHECKED.lock().unwrap_or_else(|p| p.into_inner());
    if c.iter().any(|(k, _)| *k == (id, kind)) {
        return;
    }
    if c.len() >= CHECKED_KEEP {
        drop(c.remove(0));
    }
    c.push(((id, kind), r.clone()));
}

/// Who the caller is for its uploads: its identity key, then its connection's session id
/// (`key\u{1f}session`). An upload belongs to the key it was started under; before agent_hello
/// that key is the session id, so the same connection keeps its uploads after it says hello, and
/// a resumed session (same key) does too.
pub fn owner(tools: &Tools, session: &SessionInfo) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let out = tools.send(session, Command::Project(ProjectCall::Owner { reply: tx }));
    if out.is_error {
        return None;
    }
    rx.recv_timeout(crate::project_tools::DEFERRED_WAIT).ok().map(|key| format!("{key}\u{1f}{}", session.id))
}

fn problem(path: &str, code: &str, message: impl Into<String>) -> Problem {
    Problem { file: path.to_owned(), severity: "error", code: code.into(), at: String::new(), message: message.into(), offset: None }
}

fn err(code: &'static str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn disk_err(e: &crate::disk::DiskError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_string())
}

/// Checks `bytes` as a file of `kind` and describes it: a PNG decodes (within
/// [`MAX_IMAGE_SIDE`] and [`MAX_IMAGE_PIXELS`]), a WAV parses, a GLB or glTF imports the way the
/// engine's model paths import it (a `.gltf` must be self-contained: data URIs, no external files).
/// An `.ogg` must decode whole as Vorbis, to at most [`MAX_OGG_SAMPLES`] samples.
pub fn inspect(path: &str, kind: FileKind, bytes: &[u8]) -> Result<Json, Vec<Problem>> {
    let _one = CHECKING.lock().unwrap_or_else(|p| p.into_inner());
    match kind {
        FileKind::Png => {
            const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
            if bytes.len() < 33 || bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
                return Err(vec![problem(path, "not_png", "not a PNG file (no PNG signature and IHDR chunk)")]);
            }
            let (w, h) = (u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]));
            if w == 0 || h == 0 || w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE || u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
                return Err(vec![problem(path, "image_too_large", format!("{w}x{h}: images are at most {MAX_IMAGE_SIDE} pixels a side and {MAX_IMAGE_PIXELS} pixels in all"))]);
            }
            let img = specula_core::png::decode(bytes).map_err(|e| vec![problem(path, "png_invalid", e.to_string())])?;
            Ok(Json::obj([("width", Json::from(img.width)), ("height", Json::from(img.height))]))
        }
        FileKind::Wav => {
            let w = specula_audio::wav::decode(bytes).map_err(|e| vec![problem(path, "wav_invalid", e.to_string())])?;
            let frames = w.frames();
            let seconds = if w.rate > 0 { (frames as f64 / f64::from(w.rate) * 1000.0).round() / 1000.0 } else { 0.0 };
            let mut j = Json::obj([("rate", Json::from(w.rate)), ("channels", Json::from(u64::from(w.channels))), ("frames", Json::from(frames)), ("seconds", Json::from(seconds))]);
            if w.truncated {
                j.set("truncated", Json::Bool(true));
            }
            Ok(j)
        }
        FileKind::Ogg => {
            let (d, frames) = check_ogg(path, bytes, MAX_OGG_SAMPLES, OGG_CHECK_TIME)?;
            let i = d.info().clone();
            let mut j = Json::obj([
                ("rate", Json::from(i.rate)),
                ("channels", Json::from(u64::from(i.channels))),
                ("frames", Json::from(frames)),
                ("seconds", Json::from((frames as f64 / f64::from(i.rate) * 1000.0).round() / 1000.0)),
                ("vendor", Json::from(crate::bounds::echo(&i.vendor))),
            ]);
            if let (Some(s), Some(e)) = (i.loop_start, i.loop_end) {
                j.set("loop", Json::obj([("start_frame", Json::from(s)), ("end_frame", Json::from(e))]));
            }
            if d.stats.bad_packets > 0 || d.stats.pages_skipped > 0 {
                j.set("damaged", Json::obj([("bad_packets", Json::from(d.stats.bad_packets)), ("pages_skipped", Json::from(d.stats.pages_skipped))]));
            }
            Ok(j)
        }
        FileKind::Glb | FileKind::Gltf => {
            let glb = bytes.len() >= 4 && &bytes[..4] == b"glTF";
            if (kind == FileKind::Glb) != glb {
                let why = if glb { "this is a binary GLB: name it .glb" } else { "not a binary GLB (no glTF header): a JSON glTF is named .gltf" };
                return Err(vec![problem(path, "wrong_container", why)]);
            }
            let imported = specula_gltf::import_bytes(bytes, None, &specula_gltf::ImportOptions::default()).map_err(|e| vec![problem(path, "gltf_invalid", e.to_string())])?;
            let s = &imported.stats;
            Ok(Json::obj([
                ("nodes", Json::from(s.nodes)),
                ("meshes", Json::from(s.meshes)),
                ("primitives", Json::from(s.primitives)),
                ("skins", Json::from(s.skins)),
                ("animations", Json::from(s.animations)),
                ("materials", Json::from(imported.materials.len())),
                ("warnings", Json::from(imported.warnings.len())),
            ]))
        }
        other => Err(vec![problem(path, "not_binary", format!("{} files are text: use project_write", other.name()))]),
    }
}

fn problems_json(p: &[Problem]) -> Json {
    Json::arr(p.iter().take(20).map(Problem::to_json))
}

fn store(tools: &Tools) -> Result<&Arc<ContentStore>, ToolOutput> {
    tools.content.as_ref().ok_or_else(|| err("no_content_store", "this engine has no content store (binary project files need one)"))
}

/// `project_read` of a binary file: metadata (kind, version, bytes, content id, what the check
/// says), and with `offset`/`length` (or `bytes: true`) a chunk of its bytes as base64.
pub fn read(tools: &Tools, path: &str, snap: &Snap, version: Option<u64>, args: &Json) -> Result<ToolOutput, String> {
    let Some(cur) = &snap.current else { return Ok(err("not_found", format!("there is no project file {path}"))) };
    let text: Arc<str> = match version {
        Some(v) if v != cur.version => match &snap.at_version {
            Some(t) => Arc::clone(t),
            None => return Ok(err("not_kept", format!("version {v} of {path} isn't in the journal any more (or never existed); the current version is {}", cur.version))),
        },
        _ => Arc::clone(&cur.text),
    };
    let shown = version.unwrap_or(cur.version);
    let Some((id, len)) = blobs::parse_pointer(&text) else { return Ok(err("not_a_pointer", format!("{path}'s entry doesn't name its content (store damaged?)"))) };
    let store = match store(tools) {
        Ok(s) => s,
        Err(out) => return Ok(out),
    };
    let mut out = vec![("path", Json::from(path)), ("kind", Json::from(cur.kind.name())), ("version", Json::from(shown)), ("bytes", Json::from(len)), ("content", Json::from(id.hex()))];
    if shown != cur.version {
        out.push(("current_version", Json::from(cur.version)));
    } else {
        out.push(("modified_by", Json::from(&*cur.modified_by)));
    }
    let want_bytes = bool_arg(args, "bytes")?.unwrap_or(false) || args.get("offset").is_some() || args.get("length").is_some();
    let want_problems = bool_arg(args, "problems")?.unwrap_or(true);
    if want_bytes {
        let offset = args.get("offset").map(|v| v.as_u64().ok_or("'offset' must be a byte offset (an integer)")).transpose()?.unwrap_or(0);
        let length = args.get("length").map(|v| v.as_u64().filter(|n| (1..=MAX_READ_BYTES as u64).contains(n)).ok_or(format!("'length' must be 1..{MAX_READ_BYTES}"))).transpose()?.map_or(DEFAULT_READ_BYTES, |n| n as usize);
        if offset > len {
            return Err(format!("'offset' {offset} is past the end ({len} bytes)"));
        }
        let chunk = match store.read_range(id, offset, length) {
            Ok(c) => c,
            Err(e) => return Ok(disk_err(&e)),
        };
        let end = offset + chunk.len() as u64;
        out.push(("offset", Json::from(offset)));
        out.push(("length", Json::from(chunk.len())));
        out.push(("crc32", Json::from(format!("{:08x}", specula_core::checksum::crc32(&chunk)))));
        out.push(("data_base64", Json::from(specula_core::base64::encode(&chunk))));
        if end < len {
            out.push(("next_offset", Json::from(end)));
        }
    } else if want_problems {
        // Checked once per content (review finding: every read decoded the whole file again).
        let result = match checked(id, cur.kind) {
            Some(r) => Some(r),
            None => match store.get(id) {
                Ok(bytes) => {
                    let r = inspect(path, cur.kind, &bytes);
                    remember(id, cur.kind, &r);
                    Some(r)
                }
                Err(e) => {
                    out.push(("problems", Json::arr([e.to_json()])));
                    None
                }
            },
        };
        match result {
            Some(Ok(info)) => out.push(("info", info)),
            Some(Err(p)) => out.push(("problems", problems_json(&p))),
            None => {}
        }
    }
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

fn crc_arg(args: &Json, key: &str) -> Result<Option<u32>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => u32::from_str_radix(s.trim_start_matches("0x"), 16).map(Some).map_err(|_| format!("'{key}' must be a CRC-32: 8 hex digits (\"1c291ca3\") or an integer")),
        Some(v) => v.as_u64().filter(|n| *n <= u64::from(u32::MAX)).map(|n| Some(n as u32)).ok_or(format!("'{key}' must be a CRC-32: 8 hex digits or an integer")),
    }
}

fn decode(args: &Json) -> Result<Option<Vec<u8>>, String> {
    let Some(d) = string_arg(args, "data")? else { return Ok(None) };
    if d.len() > MAX_CHUNK_BYTES / 3 * 4 + 8 {
        return Err(format!("'data' carries at most {MAX_CHUNK_BYTES} bytes per call (base64): send the file in chunks"));
    }
    specula_core::base64::decode(d).map(Some).map_err(|e| format!("'data' isn't base64: {e:?}"))
}

/// `project_upload` (see its schema in [`definitions`]).
pub fn upload_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "project_upload", &[&["path", "bytes", "crc32", "expect_version", "upload", "offset", "data", "chunk_crc32", "commit", "abort"]])?;
    let store = match store(tools) {
        Ok(s) => Arc::clone(s),
        Err(out) => return Ok(out),
    };
    let ups = &tools.uploads;
    // Uploads belong to the agent (its identity), not the connection: a resumed session continues them.
    let Some(by) = owner(tools, session) else { return Ok(err("no_answer", "the engine didn't say who you are")) };
    let commit = bool_arg(args, "commit")?.unwrap_or(false);
    let abort = bool_arg(args, "abort")?.unwrap_or(false);
    let data = decode(args)?;
    let id = match string_arg(args, "upload")? {
        Some(id) => id.to_owned(),
        None => {
            // A new upload: path, size and CRC-32 of the whole file.
            let path = crate::project_tools::path_arg(args)?;
            crate::project::check_path(&path).map_err(|e| e.message)?;
            let snap = match snapshot(tools, session, &path, None, false) {
                Ok(s) => s,
                Err(out) => return Ok(out),
            };
            match snap.kind {
                Some(k) if k.is_binary() => {}
                Some(k) => return Ok(err("not_binary", format!("{path} is a {} file (text): use project_write", k.name()))),
                None => {
                    return Ok(err(
                        "not_mirrored",
                        format!(
                            "{path} isn't a binary project file: PNG images under a folder named {}, GLB/glTF models under {}, WAV audio under {} (in the assets mount)",
                            crate::project::PNG_FOLDERS.join(" / "),
                            crate::project::MODEL_FOLDERS.join(" / "),
                            crate::project::WAV_FOLDERS.join(" / ")
                        ),
                    ))
                }
            }
            let bytes = match (args.get("bytes").and_then(Json::as_u64), &data) {
                (Some(b), _) => b,
                (None, Some(d)) => d.len() as u64,
                (None, None) => return Err("'bytes' is required to start an upload: the whole file's size (with 'crc32', its CRC-32)".into()),
            };
            let crc = match (crc_arg(args, "crc32")?, &data) {
                (Some(c), _) => c,
                (None, Some(d)) if d.len() as u64 == bytes => specula_core::checksum::crc32(d),
                _ => return Err("'crc32' is required to start a chunked upload: the whole file's CRC-32 (Python: zlib.crc32(data))".into()),
            };
            let expect = args.get("expect_version").map(|v| v.as_u64().ok_or("'expect_version' must be an integer")).transpose()?;
            match ups.begin(&store, &by, &path, bytes, crc, expect) {
                Ok(u) => {
                    if data.is_none() {
                        let mut j = u.to_json();
                        j.set("chunk_bytes", Json::from(blobs::CHUNK_BYTES));
                        j.set("max_chunk_bytes", Json::from(MAX_CHUNK_BYTES));
                        j.set("next", Json::from("send chunks: {upload, offset, data (base64)}; then {upload, commit: true}. Interrupted? {upload} answers next_offset"));
                        return Ok(ToolOutput::text(j.to_string()));
                    }
                    u.id
                }
                Err(e) => return Ok(disk_err(&e)),
            }
        }
    };
    if abort {
        return Ok(match ups.finish(&store, &by, &id) {
            Some(u) => ToolOutput::text(Json::obj([("aborted", Json::from(u.id.as_str())), ("path", Json::from(u.path.as_str()))]).to_string()),
            None => err("upload_not_found", format!("you have no open upload {id}")),
        });
    }
    let mut status = None;
    if let Some(d) = &data {
        // A chunk names where it goes: one resent after a lost answer is refused (bad_offset),
        // never appended twice.
        let offset = match (args.get("offset").and_then(Json::as_u64), args.get("upload").is_some()) {
            (Some(o), _) => o,
            (None, false) => 0,
            (None, true) => return Err("'offset' is required with a chunk: the upload's next_offset (bytes received so far)".into()),
        };
        match ups.chunk(&by, &id, offset, d, crc_arg(args, "chunk_crc32")?) {
            Ok(u) => status = Some(u),
            Err(e) => return Ok(disk_err(&e)),
        }
    }
    let u = match status {
        Some(u) => u,
        None => match ups.status(&by, &id) {
            Ok(u) => u,
            Err(e) => return Ok(disk_err(&e)),
        },
    };
    // A single-call upload commits once its bytes are all there.
    let single = args.get("upload").is_none() && u.received == u.bytes;
    if !(commit || single) {
        return Ok(ToolOutput::text(u.to_json().to_string()));
    }
    commit_upload(tools, session, &store, &by, &u)
}

fn commit_upload(tools: &Tools, session: &SessionInfo, store: &ContentStore, by: &str, u: &Upload) -> Result<ToolOutput, String> {
    let ups = &tools.uploads;
    let (u, bytes) = match ups.take(by, &u.id) {
        Ok(x) => x,
        Err(e) => return Ok(disk_err(&e)),
    };
    let path = u.path.clone();
    for _ in 0..COMMIT_ATTEMPTS {
        let snap = match snapshot(tools, session, &path, u.expect_version.filter(|v| *v > 0), false) {
            Ok(s) => s,
            Err(out) => return Ok(out),
        };
        let Some(kind) = snap.kind.filter(|k| k.is_binary()) else { return Ok(err("not_mirrored", format!("{path} isn't a binary project file any more"))) };
        let (cur_v, cur_h) = (snap.current.as_ref().map_or(0, |f| f.version), snap.current.as_ref().map_or(0, |f| f.hash));
        if let Some(e) = u.expect_version {
            if e != cur_v {
                let theirs = snap.current.as_ref().and_then(|f| blobs::parse_pointer(&f.text)).map(|(id, b)| Json::obj([("content", Json::from(id.hex())), ("bytes", Json::from(b))]));
                return Ok(ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("stale_version")),
                        ("message", Json::from(format!("{path} is at version {cur_v}, not {e}: someone changed it since you read it; nothing was changed (the upload stays open: commit again with expect_version via a new upload, or abort it)"))),
                        ("path", Json::from(path.as_str())),
                        ("expected_version", Json::from(e)),
                        ("version", Json::from(cur_v)),
                        ("conflict", Json::obj([("theirs", Json::obj([("version", Json::from(cur_v)), ("changes", Json::arr(snap.later.iter().take(8).cloned())), ("now", theirs.unwrap_or(Json::Null))])), ("yours", Json::obj([("content", Json::from(BlobId::of(&bytes).hex())), ("bytes", Json::from(bytes.len()))]))])),
                        ("upload", Json::from(u.id.as_str())),
                    ])
                    .to_string(),
                ));
            }
        }
        let checked_now = inspect(&path, kind, &bytes);
        let info = match checked_now.clone() {
            Ok(i) => i,
            Err(p) => {
                return Ok(ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("invalid")),
                        ("message", Json::from(format!("{path} is not a loadable {} file; nothing was changed (abort the upload, or fix the file and upload again)", kind.name()))),
                        ("path", Json::from(path.as_str())),
                        ("kind", Json::from(kind.name())),
                        ("problems", problems_json(&p)),
                        ("upload", Json::from(u.id.as_str())),
                    ])
                    .to_string(),
                ))
            }
        };
        let id = match store.put(&bytes) {
            Ok(id) => id,
            Err(e) => return Ok(disk_err(&e)),
        };
        remember(id, kind, &checked_now);
        let text = blobs::pointer(id, bytes.len() as u64);
        if snap.current.as_ref().is_some_and(|f| *f.text == *text) {
            ups.finish(store, by, &u.id);
            return Ok(ToolOutput::text(Json::obj([("path", Json::from(path.as_str())), ("version", Json::from(cur_v)), ("unchanged", Json::Bool(true)), ("content", Json::from(id.hex()))]).to_string()));
        }
        let (tx, rx) = mpsc::channel();
        let out = tools.send(session, Command::Project(ProjectCall::Commit { path: path.clone(), base: (cur_v, cur_h), file: Some((kind, Arc::from(text))), replace_unreadable: false, also: None, reply: tx }));
        if out.is_error {
            return Ok(out);
        }
        match rx.recv_timeout(crate::project_tools::DEFERRED_WAIT) {
            Ok(Committed::Changed) => continue,
            Ok(Committed::Refused(e)) => {
                let mut j = e.to_json();
                j.set("upload", Json::from(u.id.as_str()));
                j.set("note", Json::from("nothing was changed; the upload stays open (commit again once the problem is gone, or abort it)"));
                return Ok(ToolOutput::error(j.to_string()));
            }
            Ok(Committed::Done(mut j)) => {
                ups.finish(store, by, &u.id);
                j.set("kind", Json::from(kind.name()));
                j.set("bytes", Json::from(bytes.len()));
                j.set("content", Json::from(id.hex()));
                j.set("info", info);
                return Ok(ToolOutput::text(j.to_string()));
            }
            Err(_) => return Ok(err("no_answer", "the engine didn't answer the commit")),
        }
    }
    Ok(err("busy", format!("others kept changing {path} while the upload was committed ({COMMIT_ATTEMPTS} tries); claim_take /project/{path} first, then commit again")))
}

/// The schema of `project_upload` (its documentation for agents).
pub fn definitions() -> Vec<specula_protocol::ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![specula_protocol::ToolDef {
        name: "project_upload".into(),
        description: format!(
            "Writes a binary project file: a PNG image (under a folder named {}), a GLB/glTF model (under models/), or WAV or Ogg Vorbis audio (under audio/, sounds/ or music/) in the assets mount, up to 32 MiB. \
             Small files in one call: {{path, data (base64)}}. Larger ones in chunks: start with {{path, bytes, crc32}} (the whole file's size and CRC-32), send {{upload, offset, data}} chunks of up to {} bytes \
             (1 MiB suggested; chunk_crc32 checks each), then {{upload, commit: true}}. An interrupted upload resumes: {{upload}} answers next_offset. \
             Before anything changes the file is checked by kind (a PNG decodes, a WAV parses, an .ogg decodes whole as Vorbis, a model imports as model_build would import it) and its CRC-32 must match. \
             Then it is committed like project_write: claim-checked (/project/<path>), versioned (expect_version), journaled (the journal holds a content id, not the bytes; world_undo puts the old bytes back on disk), \
             written to disk atomically, and hot-reloaded (models rebuild). project_read shows it (metadata, or bytes with offset/length); project_delete removes it.",
            crate::project::PNG_FOLDERS.join(", "),
            MAX_CHUNK_BYTES
        ),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("path", t("string", "Starting an upload: the project path, e.g. \"assets/textures/stone.png\", \"assets/models/wraith.glb\", \"assets/sounds/bell.wav\".")),
                    ("bytes", t("integer", "Starting a chunked upload: the whole file's size.")),
                    ("crc32", Json::obj([("description", Json::from("Starting a chunked upload: the whole file's CRC-32 (8 hex digits or an integer; Python zlib.crc32)."))])),
                    ("expect_version", t("integer", "The version you last read; 0 = the file must not exist yet. A stale one gets stale_version and nothing changes.")),
                    ("upload", t("string", "The upload's id (from the first answer): continue, check, commit or abort it.")),
                    ("offset", t("integer", "Where this chunk goes: the upload's next_offset (bytes received so far).")),
                    ("data", t("string", "Base64 bytes: the whole file (one call) or a chunk.")),
                    ("chunk_crc32", Json::obj([("description", Json::from("This chunk's CRC-32, checked before it is staged."))])),
                    ("commit", t("boolean", "Check and commit the finished upload.")),
                    ("abort", t("boolean", "Drop the upload and its staged bytes.")),
                ]),
            ),
            ("required", Json::Arr(Vec::new())),
        ]),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real GLB: a Sepulcra recipe built and exported by the engine's own exporter.
    pub(crate) fn test_glb() -> Vec<u8> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let style = specula_model::Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
        let model = specula_model::build(&std::fs::read_to_string(root.join("recipes/grave_rock.json")).unwrap(), &specula_model::BuildParams::new(&style)).unwrap();
        specula_gltf::export_glb(&specula_gltf::ExportInput { model: &model, lod: 0, skeleton: None, clips: &[] }).unwrap()
    }

    /// Review probe: mutated WAV and GLB bytes (what project_upload checks from agents) never
    /// panic the checks.
    #[test]
    fn damaged_audio_and_models_never_panic_the_checks() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36u32 + 64).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 8000u32.to_le_bytes().to_vec(), 32000u32.to_le_bytes().to_vec(), 4u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
            wav.extend_from_slice(&v);
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&64u32.to_le_bytes());
        wav.extend_from_slice(&[7u8; 64]);
        let mut x = 0x1234_5678_9abc_def1u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let ogg = include_bytes!("../../specula-audio/tests/data/vorbis/sweep_mono_22k.ogg").to_vec();
        for (kind, seed) in [(FileKind::Wav, wav), (FileKind::Glb, test_glb()), (FileKind::Ogg, ogg)] {
            for _ in 0..if kind == FileKind::Ogg { 300 } else { 1500 } {
                let mut d = seed.clone();
                for _ in 0..1 + next() % 6 {
                    let i = (next() as usize) % d.len();
                    match next() % 4 {
                        0 => d[i] = next() as u8,
                        1 => d[i] = 0xff,
                        2 => d[i] = 0,
                        _ => d[i..(i + 4).min(seed.len())].fill(0xff),
                    }
                }
                let _ = inspect("assets/models/fuzz", kind, &d);
            }
        }
    }

    /// An .ogg upload is checked by decoding it whole: a real file passes with its facts; its
    /// headers over garbage (review probe: they decoded to 0 frames and passed) are refused.
    #[test]
    fn ogg_uploads_must_decode_to_audio() {
        let ogg = include_bytes!("../../specula-audio/tests/data/vorbis/stereo_48k.ogg");
        let info = inspect("assets/music/a.ogg", FileKind::Ogg, ogg).unwrap();
        assert_eq!(info.get("channels").and_then(Json::as_u64), Some(2));
        assert_eq!(info.get("frames").and_then(Json::as_u64), Some(28_800));
        // The three header pages, then junk.
        let mut r = specula_audio::ogg::PacketReader::new(ogg.to_vec().into()).unwrap();
        let mut p = Vec::new();
        for _ in 0..3 {
            r.next_packet(&mut p).unwrap();
        }
        let audio_at = r.next_packet(&mut p).unwrap().unwrap().page_offset;
        let mut junk = ogg[..audio_at].to_vec();
        junk.extend(std::iter::repeat_n(0x5au8, 4096));
        let e = inspect("assets/music/b.ogg", FileKind::Ogg, &junk).unwrap_err();
        assert_eq!(e[0].code, "ogg_empty", "{:?}", e[0].message);
        assert_eq!(inspect("assets/music/c.ogg", FileKind::Ogg, b"RIFF....WAVE").unwrap_err()[0].code, "ogg_invalid");
    }

    /// Review round 2 probe "hi8": a file claiming 8 channels at 384 kHz held the check's lock
    /// for 17.6 s. The check counts samples (frames × channels), refuses what a stream couldn't
    /// play, and stops at a time limit.
    #[test]
    fn ogg_checks_are_bounded_in_samples_and_time() {
        let mono = include_bytes!("../../specula-audio/tests/data/vorbis/sweep_mono_22k.ogg");
        let claiming = |channels: u8, rate: u32| {
            let mut b = mono.to_vec();
            let (body, page_len) = {
                let p = specula_audio::ogg::parse_page(&b, 0).unwrap();
                (p.offset + 27 + p.lacing.len(), p.len)
            };
            b[body + 11] = channels;
            b[body + 12..body + 16].copy_from_slice(&rate.to_le_bytes());
            b[22..26].fill(0);
            let c = specula_audio::ogg::crc32(&b[..page_len]);
            b[22..26].copy_from_slice(&c.to_le_bytes());
            b
        };
        let long = std::time::Duration::from_secs(60);
        let e = check_ogg("a.ogg", &claiming(8, 384_000), MAX_OGG_SAMPLES, long).err().unwrap();
        assert_eq!(e[0].code, "ogg_too_dense");
        // 8 channels at 48 kHz is streamable: it is checked, counting every channel's samples.
        let (_, frames) = check_ogg("b.ogg", &claiming(8, 48_000), MAX_OGG_SAMPLES, long).unwrap();
        assert_eq!(frames, 22_050);
        assert_eq!(check_ogg("c.ogg", &claiming(8, 48_000), 22_050 * 8 - 1, long).err().unwrap()[0].code, "ogg_too_long");
        assert_eq!(check_ogg("d.ogg", mono, MAX_OGG_SAMPLES, std::time::Duration::ZERO).err().unwrap()[0].code, "ogg_too_slow");
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        specula_core::png::encode_rgba8(w, h, &vec![200u8; (w * h * 4) as usize], specula_core::deflate::Level::Fastest)
    }

    /// Validation by kind: each kind's real files pass with their facts; damaged or mislabelled
    /// ones are refused with a code; a huge PNG header is refused before anything is decoded.
    #[test]
    fn binary_files_are_checked_by_kind() {
        let info = inspect("assets/textures/a.png", FileKind::Png, &png(4, 3)).unwrap();
        assert_eq!((info.get("width").and_then(Json::as_u64), info.get("height").and_then(Json::as_u64)), (Some(4), Some(3)));
        let mut bad = png(4, 3);
        let n = bad.len();
        bad[n - 20] ^= 0xff; // damage the image data (CRC no longer matches)
        assert_eq!(inspect("x.png", FileKind::Png, &bad).unwrap_err()[0].code, "png_invalid");
        assert_eq!(inspect("x.png", FileKind::Png, b"GIF89a").unwrap_err()[0].code, "not_png");
        let mut bomb = png(1, 1);
        bomb[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        assert_eq!(inspect("x.png", FileKind::Png, &bomb).unwrap_err()[0].code, "image_too_large", "refused from its header");
        // WAV: a tiny 16-bit mono file.
        let mut wav = Vec::new();
        let samples: Vec<i16> = (0..800).map(|i| ((i % 40) * 500) as i16).collect();
        let data_len = (samples.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for s in &samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        let info = inspect("assets/sounds/b.wav", FileKind::Wav, &wav).unwrap();
        assert_eq!(info.get("seconds").and_then(Json::as_f64), Some(0.1), "{info}");
        assert_eq!(inspect("x.wav", FileKind::Wav, b"RIFFxxxxAVI ").unwrap_err()[0].code, "wav_invalid");
        // GLB: the engine's own exporter makes a real one.
        let glb = test_glb();
        let info = inspect("assets/models/w.glb", FileKind::Glb, &glb).unwrap();
        assert!(info.get("meshes").and_then(Json::as_u64).is_some_and(|m| m >= 1), "{info}");
        assert_eq!(inspect("assets/models/w.gltf", FileKind::Gltf, &glb).unwrap_err()[0].code, "wrong_container");
        assert_eq!(inspect("assets/models/w.glb", FileKind::Glb, b"glTF\x02\x00\x00\x00garbage").unwrap_err()[0].code, "gltf_invalid");
    }
}

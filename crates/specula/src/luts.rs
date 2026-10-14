//! Grading LUTs: the world names a PNG strip under the assets root (`world_env` `grading.lut`);
//! the engine checks it before the change is committed (and keeps what it decoded), uploads it
//! as a texture and points the frame's grading at it. The file in use is checked for changes once
//! a second; a LUT that can't be loaded is reported as a structured warning (`sight_frame`,
//! `perf_frame`, `world_env`) and retried when its file changes or `world_env` names it again.
//!
//! A LUT strip is `N² × N` pixels (N = 2..=64; 32 → 1024 × 32): N slices of N × N side by side,
//! red across each slice, green down the rows, blue from slice to slice — the layout Unreal
//! exports. Its colours are display values (as a paint program shows them) and are used as they
//! are. Paths stay inside the assets root: relative, no `..`, and links that lead outside are
//! refused.

use crate::effects::AssetPath;
use crate::world::{Env, WorldError};
use specula_core::Json;
use specula_gpu::{FrameDesc, Gpu, TextureFormat, TextureId, MAX_LUT_SIZE};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Largest LUT file read (a 64³ strip is about 1 MB as PNG at worst).
const MAX_FILE_BYTES: u64 = 16 << 20;

/// A decoded, checked LUT strip.
#[derive(Debug)]
pub struct Lut {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn lut_error(code: &'static str, path: &AssetPath, message: String) -> WorldError {
    WorldError::new(code, message).with("field", "grading.lut").with("lut", path.as_str())
}

/// Read and check the LUT `path` under `root`: the file must be a regular file inside the root
/// and a PNG strip `N² × N`.
pub fn load(root: &Path, path: &AssetPath) -> Result<Lut, WorldError> {
    let not_found = || {
        lut_error("lut_not_found", path, format!("no LUT file {} under the assets root {}", path.as_str(), root.display()))
            .with("assets_root", root.display().to_string())
    };
    let canonical_root = root.canonicalize().map_err(|_| not_found())?;
    let full = root.join(path.as_str()).canonicalize().map_err(|_| not_found())?;
    if !full.starts_with(&canonical_root) {
        return Err(lut_error("lut_outside_assets", path, format!("{} leads outside the assets root", path.as_str())));
    }
    let meta = std::fs::metadata(&full).map_err(|_| not_found())?;
    if !meta.is_file() {
        return Err(not_found());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(lut_error("lut_invalid", path, format!("{} is {} bytes; a LUT file is at most {MAX_FILE_BYTES}", path.as_str(), meta.len())));
    }
    let bytes = std::fs::read(&full).map_err(|e| lut_error("lut_not_found", path, format!("{}: {e}", path.as_str())))?;
    let invalid = |e: String| lut_error("lut_invalid", path, format!("{}: {e}", path.as_str()));
    let (width, height) = header_size(&bytes).ok_or_else(|| invalid("not a PNG file".into()))?;
    check_size(path, width, height)?;
    let image = specula_core::png::decode(&bytes).map_err(|e| invalid(e.to_string()))?;
    check_size(path, image.width, image.height)?;
    Ok(Lut { width: image.width, height: image.height, rgba: image.rgba })
}

/// The strip's size from the PNG header, before anything is decoded: a small file can claim a
/// huge image (a 1 MB PNG of zeros decodes to gigabytes).
fn header_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    (bytes.len() >= 24 && bytes[..8] == *b"\x89PNG\r\n\x1a\n" && bytes[12..16] == *b"IHDR").then(|| (be(16), be(20)))
}

fn check_size(path: &AssetPath, width: u32, height: u32) -> Result<(), WorldError> {
    let n = height;
    if (2..=MAX_LUT_SIZE).contains(&n) && width == n * n {
        return Ok(());
    }
    Err(lut_error(
        "lut_invalid",
        path,
        format!("{} is {width}x{height}; a 3D LUT strip is N*N x N with N 2..={MAX_LUT_SIZE} (32: 1024x32)", path.as_str()),
    )
    .with("width", width)
    .with("height", height))
}

/// What identifies a version of a file: modification time and length (`None`: unreadable).
type Stamp = Option<(SystemTime, u64)>;

/// A version of a file: its stamp, and a hash of its contents (an edit that keeps the length
/// and the modification time still changes the hash). `None` hash: unreadable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Version {
    stamp: Stamp,
    hash: Option<u64>,
}

fn version(root: &Path, path: &AssetPath) -> Version {
    let file = root.join(path.as_str());
    let hash = std::fs::metadata(&file)
        .ok()
        .filter(|m| m.is_file() && m.len() <= MAX_FILE_BYTES)
        .and_then(|_| std::fs::read(&file).ok())
        .map(|bytes| {
            let mut h = specula_core::hash::StableHasher::new();
            h.write_bytes(&bytes);
            h.finish()
        });
    Version { stamp: stamp(root, path), hash }
}

fn stamp(root: &Path, path: &AssetPath) -> Stamp {
    let meta = std::fs::metadata(root.join(path.as_str())).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// How often the file in use is checked for changes.
const POLL: Duration = Duration::from_secs(1);

enum State {
    Loaded(TextureId),
    Failed(WorldError),
}

/// The LUT the frames use: which file, which version of it, and the result.
struct Current {
    path: AssetPath,
    version: Version,
    state: State,
}

/// The engine's LUT (one at a time: the world has one grading). Keyed on the path and the file's
/// version: every capture checks the file (modification time, length and a content hash), the
/// window's frames check its time and length once a second, so a LUT edited in place shows in the
/// next capture; a LUT that failed is retried when its file changes or a `world_env` names it
/// again.
pub struct Luts {
    root: PathBuf,
    current: Option<Current>,
    /// Decoded by `check` (a `world_env` naming it), uploaded by the next frame: one decode.
    pending: Option<(AssetPath, Version, Lut)>,
    last_poll: Instant,
}

impl Luts {
    pub fn new(root: &Path) -> Luts {
        Luts { root: root.to_path_buf(), current: None, pending: None, last_poll: Instant::now() }
    }

    /// Check a LUT a `world_env` call names before the change is committed. The decoded LUT is
    /// kept for the next frame (naming the same path again reloads it: an explicit retry).
    pub fn check(&mut self, path: &AssetPath) -> Result<(), WorldError> {
        if path.is_none() {
            return Ok(());
        }
        let version = version(&self.root, path);
        let lut = load(&self.root, path)?;
        self.pending = Some((*path, version, lut));
        Ok(())
    }

    /// Point `frame`'s grading at the texture of the world's LUT, loading it first if the path or
    /// the file changed (and freeing the old texture). A LUT that can't be loaded (a world file or
    /// a game naming a missing file, say) leaves the frame graded without it; [`Luts::warning`]
    /// says why. `capture`: a sight_frame, which checks the file's full version every time (one
    /// read of a small file); window frames check its time and length once a second.
    pub fn apply(&mut self, gpu: &mut Gpu, env: &Env, frame: &mut FrameDesc, capture: bool) {
        let want = env.grading_lut;
        if want.is_none() {
            self.drop_current(gpu);
            self.pending = None;
        } else if let Some((path, version, lut)) = self.pending.take().filter(|(p, _, _)| *p == want) {
            self.install(gpu, path, version, Ok(lut));
        } else if self.current.as_ref().is_none_or(|c| c.path != want) {
            self.reload(gpu, want);
        } else if capture {
            let changed = self.current.as_ref().is_some_and(|c| c.version != version(&self.root, &want));
            if changed {
                self.reload(gpu, want);
            }
        } else if self.last_poll.elapsed() >= POLL {
            self.last_poll = Instant::now();
            let changed = self.current.as_ref().is_some_and(|c| c.version.stamp != stamp(&self.root, &want));
            if changed {
                self.reload(gpu, want);
            }
        }
        frame.environment.grading.lut = match &self.current {
            Some(Current { path, state: State::Loaded(id), .. }) if *path == want => Some(*id),
            _ => None,
        };
    }

    /// Why the world's LUT isn't applied, if it isn't (a structured error as JSON).
    pub fn warning(&self, env: &Env) -> Option<Json> {
        match &self.current {
            Some(Current { path, state: State::Failed(e), .. }) if *path == env.grading_lut => Some(e.to_json()),
            _ => None,
        }
    }

    fn reload(&mut self, gpu: &mut Gpu, path: AssetPath) {
        let version = version(&self.root, &path);
        let lut = load(&self.root, &path);
        self.install(gpu, path, version, lut);
    }

    fn install(&mut self, gpu: &mut Gpu, path: AssetPath, version: Version, lut: Result<Lut, WorldError>) {
        self.drop_current(gpu);
        let uploaded = lut.and_then(|lut| {
            gpu.upload_texture(lut.width, lut.height, TextureFormat::Rgba8Unorm, &lut.rgba)
                .map_err(|e| lut_error("lut_upload_failed", &path, format!("{}: {e}", path.as_str())))
        });
        let state = match uploaded {
            Ok(id) => State::Loaded(id),
            Err(e) => {
                eprintln!("specula: grading LUT {}: {e} (grading without it)", path.as_str());
                State::Failed(e)
            }
        };
        self.current = Some(Current { path, version, state });
        self.last_poll = Instant::now();
    }

    fn drop_current(&mut self, gpu: &mut Gpu) {
        if let Some(Current { state: State::Loaded(id), .. }) = self.current.take() {
            gpu.destroy_texture(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-luts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("luts")).unwrap();
        dir
    }

    fn strip(n: u32) -> Vec<u8> {
        let px: Vec<u8> = (0..n * n * n).flat_map(|i| [(i % n) as u8, (i / (n * n)) as u8, (i / n % n) as u8, 255]).collect();
        specula_core::png::encode_rgba8(n * n, n, &px, specula_core::deflate::Level::Fastest)
    }

    #[test]
    fn loads_strips_and_refuses_everything_else() {
        let root = scratch("load");
        std::fs::write(root.join("luts/id.png"), strip(4)).unwrap();
        std::fs::write(root.join("luts/square.png"), specula_core::png::encode_rgba8(4, 4, &[0; 64], specula_core::deflate::Level::Fastest)).unwrap();
        std::fs::write(root.join("luts/junk.png"), b"not a png").unwrap();
        let p = |s: &str| AssetPath::new(s).unwrap();
        let lut = load(&root, &p("luts/id.png")).unwrap();
        assert_eq!((lut.width, lut.height, lut.rgba.len()), (16, 4, 16 * 4 * 4));
        assert_eq!(load(&root, &p("luts/square.png")).unwrap_err().code, "lut_invalid");
        assert_eq!(load(&root, &p("luts/junk.png")).unwrap_err().code, "lut_invalid");
        let e = load(&root, &p("luts/missing.png")).unwrap_err();
        assert_eq!((e.code, e.detail("field")), ("lut_not_found", Some(&specula_core::Json::from("grading.lut"))));
        std::fs::create_dir_all(root.join("luts/dir.png")).unwrap();
        assert_eq!(load(&root, &p("luts/dir.png")).unwrap_err().code, "lut_not_found", "a folder is not a LUT");
        let mut luts = Luts::new(&root);
        assert!(luts.check(&AssetPath::NONE).is_ok());
        assert!(luts.check(&p("luts/id.png")).is_ok());
        assert!(luts.pending.is_some(), "the checked LUT is kept for the next frame (decoded once)");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_oversized_strip_is_refused_from_its_header() {
        // A 1 MB file claiming a 16384 x 16384 image: refused before any decoding.
        let root = scratch("huge");
        let mut png = strip(2);
        png[16..20].copy_from_slice(&16384u32.to_be_bytes());
        png[20..24].copy_from_slice(&16384u32.to_be_bytes());
        png.resize(1 << 20, 0);
        std::fs::write(root.join("luts/huge.png"), png).unwrap();
        let start = Instant::now();
        let e = load(&root, &AssetPath::new("luts/huge.png").unwrap()).unwrap_err();
        assert_eq!((e.code, e.detail("width")), ("lut_invalid", Some(&Json::from(16384u32))));
        assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_rewritten_in_place_has_a_new_version() {
        let root = scratch("stamp");
        let p = AssetPath::new("luts/id.png").unwrap();
        assert_eq!(stamp(&root, &p), None, "missing");
        std::fs::write(root.join("luts/id.png"), strip(4)).unwrap();
        let first = stamp(&root, &p);
        assert!(first.is_some());
        let v1 = version(&root, &p);
        std::fs::write(root.join("luts/id.png"), strip(8)).unwrap();
        assert_ne!(stamp(&root, &p), first, "a different length is a different version");
        // Same length, the modification time put back: only the content hash tells.
        let mut bytes = strip(4);
        let v_len = bytes.len();
        std::fs::write(root.join("luts/id.png"), &bytes).unwrap();
        let file = std::fs::File::options().write(true).open(root.join("luts/id.png")).unwrap();
        let mtime = std::fs::metadata(root.join("luts/id.png")).unwrap().modified().unwrap();
        let before = version(&root, &p);
        assert_eq!(before.hash, v1.hash);
        bytes[v_len - 13] ^= 0xff; // inside the IEND/IDAT tail: same length
        std::fs::write(root.join("luts/id.png"), &bytes).unwrap();
        file.set_modified(mtime).unwrap();
        let after = version(&root, &p);
        assert_eq!(after.stamp, before.stamp, "same length and time");
        assert_ne!(after.hash, before.hash, "the content hash tells them apart");
        let _ = std::fs::remove_dir_all(&root);
    }
}

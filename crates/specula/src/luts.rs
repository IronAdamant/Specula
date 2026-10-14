//! Grading LUTs: the world names a PNG strip under the assets root (`world_env` `grading.lut`);
//! the engine checks it before the change is committed, loads it once, uploads it as a texture
//! and points the frame's grading at it.
//!
//! A LUT strip is `N² × N` pixels (N = 2..=64; 32 → 1024 × 32): N slices of N × N side by side,
//! red across each slice, green down the rows, blue from slice to slice — the layout Unreal
//! exports. Its colours are display values (as a paint program shows them) and are used as they
//! are. Paths stay inside the assets root: relative, no `..`, and links that lead outside are
//! refused.

use crate::effects::AssetPath;
use crate::world::{Env, WorldError};
use specula_gpu::{FrameDesc, Gpu, TextureFormat, TextureId, MAX_LUT_SIZE};
use std::path::{Path, PathBuf};

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
    let image = specula_core::png::decode(&bytes).map_err(|e| lut_error("lut_invalid", path, format!("{}: {e}", path.as_str())))?;
    let n = image.height;
    if !(2..=MAX_LUT_SIZE).contains(&n) || image.width != n * n {
        return Err(lut_error(
            "lut_invalid",
            path,
            format!(
                "{} is {}x{}; a 3D LUT strip is N*N x N with N 2..={MAX_LUT_SIZE} (32: 1024x32)",
                path.as_str(),
                image.width,
                image.height
            ),
        )
        .with("width", image.width)
        .with("height", image.height));
    }
    Ok(Lut { width: image.width, height: image.height, rgba: image.rgba })
}

/// The engine's loaded LUT (one at a time: the world has one grading).
pub struct Luts {
    root: PathBuf,
    loaded: Option<(AssetPath, TextureId)>,
    /// A path that failed to load (reported once, not retried every frame).
    failed: Option<AssetPath>,
}

impl Luts {
    pub fn new(root: &Path) -> Luts {
        Luts { root: root.to_path_buf(), loaded: None, failed: None }
    }

    /// Check a LUT a `world_env` call names before the change is committed.
    pub fn check(&self, path: &AssetPath) -> Result<(), WorldError> {
        if path.is_none() {
            return Ok(());
        }
        load(&self.root, path).map(|_| ())
    }

    /// Point `frame`'s grading at the texture of the world's LUT, loading it first if it changed
    /// (and freeing the old one). A LUT that can't be loaded (a world file naming a missing file)
    /// is reported once and the frame is graded without it.
    pub fn apply(&mut self, gpu: &mut Gpu, env: &Env, frame: &mut FrameDesc) {
        frame.environment.grading.lut = self.texture(gpu, &env.grading_lut);
    }

    fn texture(&mut self, gpu: &mut Gpu, want: &AssetPath) -> Option<TextureId> {
        match self.loaded {
            Some((path, id)) if path == *want => return Some(id),
            Some((_, id)) => {
                gpu.destroy_texture(id);
                self.loaded = None;
            }
            None => {}
        }
        if want.is_none() || self.failed == Some(*want) {
            return None;
        }
        let uploaded = load(&self.root, want)
            .map_err(|e| e.to_string())
            .and_then(|lut| gpu.upload_texture(lut.width, lut.height, TextureFormat::Rgba8Unorm, &lut.rgba).map_err(|e| e.to_string()));
        match uploaded {
            Ok(id) => {
                self.loaded = Some((*want, id));
                self.failed = None;
                Some(id)
            }
            Err(e) => {
                eprintln!("specula: grading LUT {}: {e} (grading without it)", want.as_str());
                self.failed = Some(*want);
                None
            }
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
        let luts = Luts::new(&root);
        assert!(luts.check(&AssetPath::NONE).is_ok());
        assert!(luts.check(&p("luts/id.png")).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}

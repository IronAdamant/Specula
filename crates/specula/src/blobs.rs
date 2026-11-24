//! The content store: the bytes of binary project files (PNG images, glTF/GLB models, WAV audio)
//! by content id, so the journal never holds megabytes. Design: docs/design/PROJECT_DATA.md
//! ("Binary assets").
//!
//! - A binary file's entry in the project store is a small **pointer** text
//!   (`{"blob": "<32 hex>[-n]", "bytes": N}`), so it gets everything text files have (claims,
//!   versions, the journal, undo, conflict help, disk sync) without the journal holding bytes.
//! - **Ids and collisions.** The id is the content's 128-bit stable hash, which is fast, not
//!   cryptographic: two different contents with one hash can be made on purpose (a review built
//!   such a pair of PNGs in milliseconds; they are this module's test data). So the hash only
//!   finds a **chain** of slots (`<hash>`, `<hash>-1`, `<hash>-2`...): a put compares bytes slot
//!   by slot and takes the slot holding exactly its bytes, or the next free one. Two contents never
//!   share a slot, a known collision never blocks another upload, and the disk sync compares bytes,
//!   never only ids. (The other choice, the OS's SHA-256 through bcrypt, would leave the other
//!   platforms without one; byte comparison works everywhere and needs no cryptography.)
//! - **Keyed per run.** The id is two of `std`'s keyed hashes (`RandomState`, SipHash with keys
//!   from the OS's randomness, new each engine run), not the stable hash: collisions can't be
//!   prepared offline (a seeded stable hash wasn't enough: the review's colliding pair still
//!   collided under other seeds), and filling a chain would need the running engine's keys.
//!   Content ids live only as long as their run: they never reach the world hash, world files or
//!   save games (project files are not world state, and the journal isn't kept across runs).
//! - **Damage is repaired.** A slot whose file no longer holds content of its hash (changed or
//!   cut short on disk) is rewritten by the next put of content with that hash.
//! - **Where:** one folder per engine run under the cache's `content` folder
//!   (`<cache>/content/run-<secs>-<pid>/<id>`). The journal lives only as long as its engine, so a
//!   finished run's folder is never needed again and is removed at the next start; another
//!   running engine's folder is never touched.
//! - **Bounded and pruned:** the disk budget counts it (category `content`) and, over budget,
//!   asks it for bytes back: entries no current file, kept journal entry or upload in progress
//!   needs ([`ContentStore::set_referenced`]), oldest use first, never one put in the last two
//!   minutes (an upload being committed).
//! - **Uploads** ([`Uploads`]) are staged in the run's `uploads/` folder chunk by chunk (offset
//!   checked, CRC-32 checked), so an interrupted upload resumes where it stopped.

use crate::disk::{self, Category, DiskError, Need};
use specula_core::hash::StableHasher;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The content store's folder under the engine's cache folder.
pub const CONTENT_DIR: &str = "content";
/// Largest binary project file.
pub const MAX_BLOB_BYTES: u64 = 32 << 20;
/// Entries put this recently are never pruned (an upload's bytes wait for their commit).
const GRACE: Duration = Duration::from_secs(120);
/// Uploads in progress, in all and per agent, and how long an idle one is kept.
pub const MAX_UPLOADS: usize = 16;
pub const MAX_UPLOADS_PER_AGENT: usize = 4;
pub const UPLOAD_IDLE: Duration = Duration::from_secs(15 * 60);
/// Slots one hash's chain may hold (contents colliding on purpose, see the module docs).
pub const MAX_CHAIN: u32 = 256;
/// Largest chunk one call carries (decoded bytes), and the size suggested.
pub const MAX_CHUNK_BYTES: usize = 4 << 20;
pub const CHUNK_BYTES: usize = 1 << 20;

/// A content id: the content's 128-bit stable hash (two seeded 64-bit hashes) and its slot in
/// that hash's chain (0 unless contents collide; see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobId(pub u64, pub u64, pub u32);

/// This run's keys for content ids (see the module docs): `std`'s keyed hashes, keyed from the
/// OS's randomness.
fn run_keys() -> &'static (std::collections::hash_map::RandomState, std::collections::hash_map::RandomState) {
    static KEYS: std::sync::OnceLock<(std::collections::hash_map::RandomState, std::collections::hash_map::RandomState)> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| (std::collections::hash_map::RandomState::new(), std::collections::hash_map::RandomState::new()))
}

impl BlobId {
    /// The id of `bytes` under this run's keys (slot 0: where its chain starts).
    pub fn of(bytes: &[u8]) -> BlobId {
        use std::hash::BuildHasher;
        let (a, b) = run_keys();
        BlobId(a.hash_one(bytes), b.hash_one((bytes.len() as u64, bytes)), 0)
    }

    /// The stable hash of `bytes` under `seed` (tests use it to reproduce a known collision).
    pub fn of_seeded(bytes: &[u8], seed: u64) -> BlobId {
        let mut a = StableHasher::with_seed(0x626c_6f62_2d61 ^ seed);
        let mut b = StableHasher::with_seed(0x626c_6f62_2d62 ^ seed.rotate_left(32));
        a.write_bytes(bytes);
        b.write_u64(bytes.len() as u64);
        b.write_bytes(bytes);
        BlobId(a.finish(), b.finish(), 0)
    }

    /// The same hash, another slot.
    pub fn slot(self, n: u32) -> BlobId {
        BlobId(self.0, self.1, n)
    }


    pub fn hex(self) -> String {
        if self.2 == 0 {
            format!("{:016x}{:016x}", self.0, self.1)
        } else {
            format!("{:016x}{:016x}-{}", self.0, self.1, self.2)
        }
    }

    pub fn parse(s: &str) -> Option<BlobId> {
        let (h, slot) = match s.split_once('-') {
            Some((h, n)) => (h, n.parse::<u32>().ok().filter(|n| (1..MAX_CHAIN).contains(n))?),
            None => (s, 0),
        };
        if h.len() != 32 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(BlobId(u64::from_str_radix(&h[..16], 16).ok()?, u64::from_str_radix(&h[16..], 16).ok()?, slot))
    }
}

/// A binary file's text in the project store: its content id and size.
pub fn pointer(id: BlobId, bytes: u64) -> String {
    format!("{{\"blob\": \"{}\", \"bytes\": {bytes}}}\n", id.hex())
}

/// The content id and size a pointer names (`None`: not a pointer).
pub fn parse_pointer(text: &str) -> Option<(BlobId, u64)> {
    let j = Json::parse(text).ok()?;
    Some((BlobId::parse(j.get("blob")?.as_str()?)?, j.get("bytes")?.as_u64()?))
}

#[derive(Clone, Debug)]
struct Meta {
    bytes: u64,
    /// Order of last use (put or read).
    used: u64,
    put_at: Instant,
}

#[derive(Default)]
struct Index {
    blobs: BTreeMap<BlobId, Meta>,
    referenced: BTreeSet<BlobId>,
    seq: u64,
}

/// See the module docs.
pub struct ContentStore {
    /// The content root (every run's folder), and this run's.
    root: PathBuf,
    dir: PathBuf,
    index: Mutex<Index>,
    /// `None`: ids from this run's keys ([`BlobId::of`]); tests may fix a stable-hash seed.
    seed: Option<u64>,
}

static TMP: AtomicU64 = AtomicU64::new(0);

fn err(code: &'static str, message: impl Into<String>) -> DiskError {
    DiskError { code, message: message.into(), detail: Vec::new() }
}

impl ContentStore {
    /// This run's store under `root` (made if needed); finished runs' folders are removed.
    pub fn open(root: &Path) -> Result<ContentStore, DiskError> {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let dir = root.join(format!("run-{started}-{}", std::process::id()));
        disk::book().make_dir(Category::Content, &dir, Need::Essential)?;
        let root = disk::canonical(root);
        let dir = disk::canonical(&dir);
        if let Ok(entries) = std::fs::read_dir(&root) {
            for e in entries.flatten() {
                let p = root.join(e.file_name());
                let dead = e.file_name().to_str().and_then(disk::parse_run_name).is_some_and(|(_, pid)| pid != std::process::id() && !specula_sys::process_is_running(pid));
                if dead && p != dir {
                    if let Ok((n, b)) = disk::remove_tree_inside(&root, &p) {
                        disk::book().wrote(Category::Content, "content", -(b as i64), -(n as i64));
                    }
                }
            }
        }
        Ok(ContentStore { root, dir, index: Mutex::new(Index::default()), seed: None })
    }

    /// The id `bytes` start their chain at, in this store.
    pub fn id_of(&self, bytes: &[u8]) -> BlobId {
        match self.seed {
            None => BlobId::of(bytes),
            Some(s) => BlobId::of_seeded(bytes, s),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn path_of(&self, id: BlobId) -> PathBuf {
        self.dir.join(id.hex())
    }

    /// The folder uploads are staged in.
    pub fn uploads_dir(&self) -> PathBuf {
        self.dir.join("uploads")
    }

    /// Stores `bytes` (once per content) and returns their id: the slot of their hash's chain
    /// holding exactly these bytes, a damaged slot (rewritten), or the next free one. Refused
    /// below the disk floor.
    pub fn put(&self, bytes: &[u8]) -> Result<BlobId, DiskError> {
        self.put_new(bytes).map(|(id, _)| id)
    }

    /// [`ContentStore::put`], and whether these bytes were new to the store: `Some(mark)` when
    /// this put wrote them, for [`ContentStore::withdraw`] to take back if what they were for
    /// is given up (a cancelled bake) before anything refers to them.
    pub fn put_new(&self, bytes: &[u8]) -> Result<(BlobId, Option<u64>), DiskError> {
        if bytes.len() as u64 > MAX_BLOB_BYTES {
            return Err(err("too_large", format!("{} bytes; binary project files are at most {MAX_BLOB_BYTES}", bytes.len())));
        }
        let base = self.id_of(bytes);
        let mut ix = self.lock();
        ix.seq += 1;
        let seq = ix.seq;
        let (mut damaged, mut free) = (None, None);
        // The whole chain is looked at for these exact bytes before a free slot is taken (a
        // pruned slot never makes the same bytes a second id); only slots of the same length
        // are read.
        for n in 0..MAX_CHAIN {
            let id = base.slot(n);
            let Some(m) = ix.blobs.get(&id) else {
                free = free.or(Some(id));
                continue;
            };
            if m.bytes != bytes.len() as u64 && self.path_of(id).is_file() {
                continue;
            }
            match std::fs::read(self.path_of(id)) {
                Ok(on_disk) if on_disk == bytes => {
                    if let Some(m) = ix.blobs.get_mut(&id) {
                        m.used = seq;
                        m.put_at = Instant::now();
                    }
                    return Ok((id, None));
                }
                // Another content with the same hash (a collision): the next slot.
                Ok(on_disk) if self.id_of(&on_disk) == base => {}
                // Damaged (changed or gone on disk): these bytes are this hash's content, so the
                // slot is repaired with them.
                _ => {
                    damaged = damaged.or(Some(id));
                }
            }
        }
        let Some(id) = damaged.or(free) else {
            return Err(err("hash_collision", format!("{MAX_CHAIN} different contents already share the hash {}; nothing was stored", base.hex())));
        };
        let path = self.path_of(id);
        let old = ix.blobs.get(&id).map_or(0, |m| m.bytes);
        // A repaired slot's old check no longer describes it.
        crate::project_binary::forget_checked(id);
        disk::book().admit(Category::Content, (bytes.len() as u64).saturating_sub(old), Need::Agent)?;
        let tmp = self.dir.join(format!(".{}.tmp-{}", id.hex(), TMP.fetch_add(1, Ordering::Relaxed)));
        let written = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            drop(f);
            std::fs::rename(&tmp, &path)
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(err("io", format!("content {} not stored: {e}", id.hex())));
        }
        let was = ix.blobs.insert(id, Meta { bytes: bytes.len() as u64, used: seq, put_at: Instant::now() });
        disk::book().wrote(Category::Content, "content", bytes.len() as i64 - old as i64, i64::from(was.is_none()));
        Ok((id, Some(seq)))
    }

    /// Takes back content [`ContentStore::put_new`] wrote (its `mark`), when nothing has used it
    /// since (no other put of the same bytes, no read) and nothing refers to it: what a cancelled
    /// bake stored is not left behind. Whether it went. Disk work: on a worker.
    pub fn withdraw(&self, id: BlobId, mark: u64) -> bool {
        let mut ix = self.lock();
        let untouched = ix.blobs.get(&id).is_some_and(|m| m.used == mark) && !ix.referenced.contains(&id);
        if !untouched || disk::remove_file_inside(&self.root, &self.path_of(id)).is_err() {
            return false;
        }
        if let Some(m) = ix.blobs.remove(&id) {
            disk::book().wrote(Category::Content, "content", -(m.bytes as i64), -1);
        }
        crate::project_binary::forget_checked(id);
        true
    }

    pub fn has(&self, id: BlobId) -> bool {
        self.lock().blobs.contains_key(&id)
    }

    /// The whole content of `id`, checked against its id (a damaged file is an error).
    pub fn get(&self, id: BlobId) -> Result<Vec<u8>, DiskError> {
        let bytes = self.read_range(id, 0, MAX_BLOB_BYTES as usize)?;
        if self.id_of(&bytes).slot(id.2) != id {
            return Err(err("content_damaged", format!("content {} changed on disk; its bytes no longer match", id.hex())));
        }
        Ok(bytes)
    }

    /// Up to `len` bytes of `id` from `offset`.
    pub fn read_range(&self, id: BlobId, offset: u64, len: usize) -> Result<Vec<u8>, DiskError> {
        {
            let mut ix = self.lock();
            ix.seq += 1;
            let seq = ix.seq;
            match ix.blobs.get_mut(&id) {
                Some(m) => m.used = seq,
                None => return Err(err("not_kept", format!("content {} isn't kept any more (or never was)", id.hex()))),
            }
        }
        let mut f = std::fs::File::open(self.path_of(id)).map_err(|e| err("not_kept", format!("content {}: {e}", id.hex())))?;
        f.seek(SeekFrom::Start(offset)).map_err(|e| err("io", e.to_string()))?;
        let mut out = Vec::new();
        f.take(len as u64).read_to_end(&mut out).map_err(|e| err("io", e.to_string()))?;
        Ok(out)
    }

    /// What must stay: every id a current file, a kept journal entry or an upload needs.
    pub fn set_referenced(&self, ids: BTreeSet<BlobId>) {
        self.lock().referenced = ids;
    }

    /// `(entries, bytes)` held.
    pub fn stats(&self) -> (usize, u64) {
        let ix = self.lock();
        (ix.blobs.len(), ix.blobs.values().map(|m| m.bytes).sum())
    }

    /// Gives back up to `need` bytes (at least one entry when anything is prunable): entries
    /// nothing references, oldest use first, never one put within the grace period. `dry_run`
    /// only counts. Returns `(entries, bytes)`.
    pub fn prune(&self, need: u64, dry_run: bool) -> (u64, u64) {
        let mut ix = self.lock();
        let mut cands: Vec<(u64, BlobId, u64)> =
            ix.blobs.iter().filter(|(id, m)| !ix.referenced.contains(id) && m.put_at.elapsed() >= GRACE).map(|(id, m)| (m.used, *id, m.bytes)).collect();
        cands.sort();
        let (mut n, mut b) = (0u64, 0u64);
        for (_, id, bytes) in cands {
            if b >= need && n > 0 {
                break;
            }
            if !dry_run {
                if disk::remove_file_inside(&self.root, &self.path_of(id)).is_err() {
                    continue;
                }
                ix.blobs.remove(&id);
                crate::project_binary::forget_checked(id);
            }
            n += 1;
            b += bytes;
        }
        (n, b)
    }

    /// A pruner for the disk budget (see [`disk::Pruner`]).
    pub fn pruner(self: &Arc<Self>) -> disk::Pruner {
        let me = Arc::clone(self);
        Arc::new(move |need, dry_run| me.prune(need, dry_run))
    }

    #[cfg(test)]
    fn age_for_test(&self) {
        for m in self.lock().blobs.values_mut() {
            m.put_at = Instant::now().checked_sub(GRACE * 2).unwrap_or(m.put_at);
        }
    }
}

/// One upload in progress.
#[derive(Clone, Debug)]
pub struct Upload {
    pub id: String,
    /// Who started it: the agent's identity key. Only that agent may continue, check, commit or
    /// abort it, from any of its sessions (a resumed one included, or after agent_hello renamed it).
    pub by: String,
    pub path: String,
    pub bytes: u64,
    pub crc32: u32,
    pub expect_version: Option<u64>,
    pub received: u64,
    running_crc: u32,
    file: PathBuf,
    last: Instant,
}

impl Upload {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("upload", Json::from(self.id.as_str())),
            ("path", Json::from(self.path.as_str())),
            ("bytes", Json::from(self.bytes)),
            ("received", Json::from(self.received)),
            ("complete", Json::Bool(self.received == self.bytes)),
            ("next_offset", Json::from(self.received)),
            ("idle_s", Json::from(self.last.elapsed().as_secs())),
        ])
    }
}

/// Uploads in progress (see the module docs).
#[derive(Default)]
pub struct Uploads {
    map: Mutex<BTreeMap<String, Upload>>,
    next: AtomicU64,
}

fn upload_err(code: &'static str, message: impl Into<String>, u: Option<&Upload>) -> DiskError {
    let mut e = err(code, message);
    if let Some(u) = u {
        e.detail = vec![("upload", Json::from(u.id.as_str())), ("received", Json::from(u.received)), ("next_offset", Json::from(u.received)), ("bytes", Json::from(u.bytes))];
    }
    e
}

impl Uploads {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Upload>> {
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Forgets uploads idle past [`UPLOAD_IDLE`] (their staged bytes are removed).
    fn expire(&self, store: &ContentStore) {
        let mut map = self.lock();
        let gone: Vec<String> = map.iter().filter(|(_, u)| u.last.elapsed() >= UPLOAD_IDLE).map(|(k, _)| k.clone()).collect();
        for k in gone {
            if let Some(u) = map.remove(&k) {
                remove_staged(store, &u);
            }
        }
    }

    /// Starts an upload of `bytes` bytes (CRC-32 `crc32`) to `path`.
    pub fn begin(&self, store: &ContentStore, by: &str, path: &str, bytes: u64, crc32: u32, expect_version: Option<u64>) -> Result<Upload, DiskError> {
        self.expire(store);
        if bytes == 0 || bytes > MAX_BLOB_BYTES {
            return Err(err("too_large", format!("'bytes' must be 1..{MAX_BLOB_BYTES} (binary project files are at most 32 MiB)")));
        }
        {
            let map = self.lock();
            let mine = map.values().filter(|u| Self::owns(by, &u.by)).count();
            if map.len() >= MAX_UPLOADS || mine >= MAX_UPLOADS_PER_AGENT {
                return Err(err("too_many_uploads", format!("at most {MAX_UPLOADS_PER_AGENT} uploads per agent and {MAX_UPLOADS} in all are open; finish or abort some (an idle one goes after 15 minutes)")));
            }
        }
        // Owned by the identity key (the first part of `by`).
        let by = by.split('\u{1f}').next().unwrap_or(by);
        // The whole upload is admitted up front: one refused at the floor fails before any bytes move.
        disk::book().admit(Category::Content, bytes, Need::Agent)?;
        let n = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let mut h = StableHasher::with_seed(n);
        h.write_u64(u64::from(std::process::id()));
        h.write_u64(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0));
        let id = format!("u{n}-{:012x}", h.finish() >> 16);
        let dir = store.uploads_dir();
        disk::book().make_dir(Category::Content, &dir, Need::Agent)?;
        let file = dir.join(format!("{id}.part"));
        std::fs::File::create(&file).map_err(|e| err("io", format!("could not stage the upload: {e}")))?;
        let u = Upload { id: id.clone(), by: by.to_owned(), path: path.to_owned(), bytes, crc32, expect_version, received: 0, running_crc: 0, file, last: Instant::now() };
        self.lock().insert(id, u.clone());
        Ok(u)
    }

    /// Upload `id` if the caller owns it (another agent's is answered like a missing one). `by` is
    /// the caller's identity key, optionally followed by `\u{1f}` and its session id (an upload
    /// started before agent_hello is owned by the session id it had as its key).
    fn owned<'a>(map: &'a mut BTreeMap<String, Upload>, by: &str, id: &str) -> Result<&'a mut Upload, DiskError> {
        map.get_mut(id).filter(|u| Self::owns(by, &u.by)).ok_or_else(|| err("upload_not_found", format!("you have no open upload {id} (finished, aborted, idle for 15 minutes, or another agent's): start again")))
    }

    fn owns(by: &str, owner: &str) -> bool {
        by.split('\u{1f}').any(|k| !k.is_empty() && k == owner)
    }

    pub fn status(&self, by: &str, id: &str) -> Result<Upload, DiskError> {
        Self::owned(&mut self.lock(), by, id).map(|u| u.clone())
    }

    /// Appends a chunk at `offset` (which must be what was received so far: a resumed upload
    /// asks status for it). `crc32`: the chunk's own CRC-32, checked when given.
    pub fn chunk(&self, by: &str, id: &str, offset: u64, data: &[u8], crc32: Option<u32>) -> Result<Upload, DiskError> {
        let mut map = self.lock();
        let u = Self::owned(&mut map, by, id)?;
        if offset != u.received {
            return Err(upload_err("bad_offset", format!("upload {id} has {} bytes; send the chunk at offset {} (nothing was written)", u.received, u.received), Some(u)));
        }
        if data.is_empty() || data.len() > MAX_CHUNK_BYTES {
            return Err(upload_err("bad_chunk", format!("chunks are 1..{MAX_CHUNK_BYTES} bytes"), Some(u)));
        }
        if u.received + data.len() as u64 > u.bytes {
            return Err(upload_err("too_long", format!("the chunk would take the upload past its {} bytes", u.bytes), Some(u)));
        }
        if let Some(c) = crc32 {
            let got = specula_core::checksum::crc32(data);
            if got != c {
                return Err(upload_err("bad_crc", format!("the chunk's CRC-32 is {got:08x}, not {c:08x}: it was damaged on the way; send it again"), Some(u)));
            }
        }
        let written = (|| -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new().write(true).open(&u.file)?;
            f.set_len(u.received)?;
            f.seek(SeekFrom::Start(u.received))?;
            f.write_all(data)?;
            f.sync_data()
        })();
        written.map_err(|e| upload_err("io", format!("could not stage the chunk: {e}"), Some(u)))?;
        disk::book().wrote(Category::Content, "upload", data.len() as i64, i64::from(u.received == 0));
        u.running_crc = specula_core::checksum::crc32_update(u.running_crc, data);
        u.received += data.len() as u64;
        u.last = Instant::now();
        Ok(u.clone())
    }

    /// The finished upload's bytes, checked (all received, the whole CRC-32 matches). The upload
    /// stays open (a refused commit can be retried) until [`Uploads::finish`].
    pub fn take(&self, by: &str, id: &str) -> Result<(Upload, Vec<u8>), DiskError> {
        let u = self.status(by, id)?;
        if u.received != u.bytes {
            return Err(upload_err("incomplete", format!("upload {id} has {} of {} bytes: send the rest from offset {}", u.received, u.bytes, u.received), Some(&u)));
        }
        if u.running_crc != u.crc32 {
            return Err(upload_err("bad_crc", format!("the upload's CRC-32 is {:08x}, not the {:08x} you declared: abort it and upload again", u.running_crc, u.crc32), Some(&u)));
        }
        let bytes = std::fs::read(&u.file).map_err(|e| err("io", format!("the staged upload can't be read: {e}")))?;
        if bytes.len() as u64 != u.bytes {
            return Err(upload_err("io", "the staged upload changed on disk; abort it and upload again", Some(&u)));
        }
        Ok((u, bytes))
    }

    /// Closes an upload (committed or aborted) and removes its staged bytes.
    pub fn finish(&self, store: &ContentStore, by: &str, id: &str) -> Option<Upload> {
        let mut map = self.lock();
        Self::owned(&mut map, by, id).ok()?;
        let u = map.remove(id)?;
        drop(map);
        remove_staged(store, &u);
        Some(u)
    }

    /// `by`'s open uploads (for listings), at most `limit`.
    pub fn list(&self, by: &str, limit: usize) -> Vec<Json> {
        self.lock().values().filter(|u| Self::owns(by, &u.by)).take(limit).map(Upload::to_json).collect()
    }
}

fn remove_staged(store: &ContentStore, u: &Upload) {
    let len = disk::file_len(&u.file).unwrap_or(0);
    if disk::remove_file_inside(&store.root, &disk::canonical(&u.file)).is_ok() {
        disk::book().wrote(Category::Content, "upload", -(len as i64), -1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> (PathBuf, ContentStore) {
        let base = std::env::temp_dir().join(format!("specula-blobs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let s = ContentStore::open(&base.join(CONTENT_DIR)).unwrap();
        (base, s)
    }

    /// A store keyed with the seed the review's colliding pair was built for.
    fn store_seed0(name: &str) -> (PathBuf, ContentStore) {
        let (base, mut s) = store(name);
        s.seed = Some(0);
        (base, s)
    }

    #[test]
    fn ids_pointers_and_round_trips() {
        let (base, s) = store("ids");
        let a = s.put(b"hello png").unwrap();
        assert_eq!(a, s.id_of(b"hello png"));
        assert_eq!(s.put(b"hello png").unwrap(), a, "stored once");
        assert_eq!(s.stats().0, 1);
        assert_eq!(s.get(a).unwrap(), b"hello png");
        assert_eq!(s.read_range(a, 6, 100).unwrap(), b"png");
        let p = pointer(a, 9);
        assert_eq!(parse_pointer(&p), Some((a, 9)));
        assert_eq!(BlobId::parse(&a.hex()), Some(a));
        assert!(parse_pointer("{\"blob\": \"zz\", \"bytes\": 1}").is_none());
        assert_eq!(BlobId::parse("zz"), None);
        assert_eq!(BlobId::parse(&a.slot(3).hex()), Some(a.slot(3)));
        // A damaged file is noticed, and repaired by the next put of the right bytes (review
        // finding: it was refused as a collision forever).
        std::fs::write(s.path_of(a), b"hello pnG").unwrap();
        assert_eq!(s.get(a).unwrap_err().code, "content_damaged");
        assert_eq!(s.put(b"hello png").unwrap(), a, "repaired in place");
        assert_eq!(s.get(a).unwrap(), b"hello png");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review finding: the 128-bit stable hash can be made to collide (these two PNGs, built by
    /// the review, share it). Both are stored, each in its own slot, both read back exactly, and a
    /// known collision never blocks the other content.
    #[test]
    fn colliding_contents_get_their_own_slots() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let a = std::fs::read(data.join("collide_a.png")).unwrap();
        let b = std::fs::read(data.join("collide_b.png")).unwrap();
        assert_ne!(a, b);
        assert_eq!(BlobId::of_seeded(&a, 0), BlobId::of_seeded(&b, 0), "the pair collides under the key it was built for (test data)");
        assert_ne!(BlobId::of(&a), BlobId::of(&b), "under this run's keyed hash it doesn't (round-2 finding: collisions were made offline)");
        assert!(specula_core::png::decode(&a).is_ok() && specula_core::png::decode(&b).is_ok(), "both are valid PNGs");
        let (base, s) = store_seed0("collide");
        let ib = s.put(&b).unwrap();
        let ia = s.put(&a).unwrap();
        assert_ne!(ia, ib, "two contents, two slots");
        assert_eq!((ib.2, ia.2), (0, 1));
        assert_eq!((s.get(ia).unwrap(), s.get(ib).unwrap()), (a.clone(), b.clone()));
        assert_eq!(s.put(&a).unwrap(), ia, "found again by bytes");
        assert_eq!(parse_pointer(&pointer(ia, a.len() as u64)), Some((ia, a.len() as u64)));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review probe (round 2): after slot 0 of a chain was pruned, a put of content held in slot 1
    /// took slot 0 (a second copy, a second id). The whole chain is searched first now.
    #[test]
    fn a_pruned_slot_never_gives_the_same_bytes_a_second_id() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let a = std::fs::read(data.join("collide_a.png")).unwrap();
        let b = std::fs::read(data.join("collide_b.png")).unwrap();
        let (base, s) = store_seed0("r2slots");
        let ib = s.put(&b).unwrap();
        let ia = s.put(&a).unwrap();
        s.age_for_test();
        s.set_referenced([ia].into());
        assert_eq!(s.prune(1, false).0, 1, "b pruned");
        assert!(!s.has(ib));
        assert_eq!(s.put(&a).unwrap(), ia, "the same bytes keep their id");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Pruning never removes what is referenced or just put; the rest goes oldest use first.
    #[test]
    fn pruning_spares_referenced_and_recent_content() {
        let (base, s) = store("prune");
        let a = s.put(&[1u8; 100]).unwrap();
        let b = s.put(&[2u8; 100]).unwrap();
        let c = s.put(&[3u8; 100]).unwrap();
        assert_eq!(s.prune(1000, false), (0, 0), "all within the grace period");
        s.age_for_test();
        s.set_referenced([b].into());
        let _ = s.read_range(a, 0, 1); // a used after c
        assert_eq!(s.prune(1, true), (1, 100), "a dry run counts");
        assert!(s.has(c));
        assert_eq!(s.prune(1, false), (1, 100));
        assert!(!s.has(c) && s.has(a) && s.has(b), "c (oldest use) went first; b is referenced");
        assert_eq!(s.prune(10_000, false), (1, 100));
        assert!(s.has(b));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Uploads: chunks at the right offset only, resumable from status, CRC-checked, bounded.
    #[test]
    fn uploads_resume_and_check_their_bytes() {
        let (base, s) = store("upload");
        let up = Uploads::default();
        let data: Vec<u8> = (0..3000u32).map(|i| (i * 7) as u8).collect();
        let crc = specula_core::checksum::crc32(&data);
        let u = up.begin(&s, "ann", "assets/textures/a.png", data.len() as u64, crc, None).unwrap();
        up.chunk("ann", &u.id, 0, &data[..1000], Some(specula_core::checksum::crc32(&data[..1000]))).unwrap();
        // Interrupted: a retry of the same chunk is refused with where to resume.
        let e = up.chunk("ann", &u.id, 0, &data[..1000], None).unwrap_err();
        assert_eq!(e.code, "bad_offset");
        assert_eq!(e.to_json().get("next_offset").and_then(Json::as_u64), Some(1000));
        assert_eq!(up.chunk("ann", &u.id, 1000, &data[1000..2000], Some(1)).unwrap_err().code, "bad_crc");
        assert_eq!(up.take("ann", &u.id).unwrap_err().code, "incomplete");
        // Review finding: any session could see, abort or commit another agent's upload.
        assert!(up.list("bob", 16).is_empty() && up.list("ann", 16).len() == 1, "listed to its owner only");
        assert_eq!(up.status("bob", &u.id).unwrap_err().code, "upload_not_found");
        assert!(up.finish(&s, "bob", &u.id).is_none(), "bob can't abort it");
        assert_eq!(up.chunk("bob", &u.id, 1000, &data[1000..], None).unwrap_err().code, "upload_not_found");
        assert_eq!(up.take("bob", &u.id).unwrap_err().code, "upload_not_found");
        // Review finding (round 2): an upload started before agent_hello (key = the session id)
        // stays its connection's after the hello gives it a new key.
        let pre = up.begin(&s, "sess-1\u{1f}sess-1", "assets/textures/p.png", 3, 0, None).unwrap();
        assert!(up.status("key-ann\u{1f}sess-1", &pre.id).is_ok(), "same connection, new key");
        assert!(up.status("key-bob\u{1f}sess-2", &pre.id).is_err());
        let st = up.status("ann", &u.id).unwrap();
        up.chunk("ann", &u.id, st.received, &data[1000..], None).unwrap();
        let (_, bytes) = up.take("ann", &u.id).unwrap();
        assert_eq!(bytes, data);
        assert!(up.finish(&s, "ann", &u.id).is_some());
        assert_eq!(up.status("ann", &u.id).unwrap_err().code, "upload_not_found");
        // A wrong declared CRC fails the take.
        let v = up.begin(&s, "ann", "assets/textures/b.png", 3, 0xdead_beef, None).unwrap();
        up.chunk("ann", &v.id, 0, b"abc", None).unwrap();
        assert_eq!(up.take("ann", &v.id).unwrap_err().code, "bad_crc");
        for _ in 1..MAX_UPLOADS_PER_AGENT {
            up.begin(&s, "ann", "assets/textures/c.png", 3, 0, None).unwrap();
        }
        assert_eq!(up.begin(&s, "ann", "assets/textures/d.png", 3, 0, None).unwrap_err().code, "too_many_uploads");
        assert_eq!(up.begin(&s, "bob", "assets/textures/d.png", MAX_BLOB_BYTES + 1, 0, None).unwrap_err().code, "too_large");
        let _ = std::fs::remove_dir_all(&base);
    }
}

//! Resource leases: ports, GPU time and scratch folders handed out per agent, so swarms that run
//! their own engines (headless playtests) don't collide.
//!
//! Earlier sessions' lanes clobbered a shared scratch folder, killed each other's engines and
//! collided on ports. A lease is the engine's answer to "which port / folder / share of the GPU is
//! mine": an agent takes it before starting its own engine or writing scratch files, renews it
//! while it works, and releases it after.
//!
//! - **Ports**: `count` free ports from the pool (`perf.toml` `[swarm] lease_ports`, default
//!   7500-7599, never this engine's own port). A port is handed out only when no live lease has it,
//!   nothing listens on it (the OS's TCP listener table, read without opening a socket) *and* it
//!   can be bound on 127.0.0.1 right now. Nothing is ever bound on a wildcard address.
//! - **GPU time**: a share in milliseconds of GPU time per second, out of `gpu_ms_per_s` (1000 =
//!   one whole GPU). Advisory: engines don't enforce it, but agents that lease before rendering
//!   heavily stay within what the machine can give (the window keeps its frames).
//! - **Scratch folders**: a fresh folder of its own under the scratch root, never reused by a
//!   later lease. `lease_release {delete: true}` removes it with everything in it (links inside
//!   are removed as links, never followed). Each folder carries a marker (`.specula-lease`: its
//!   lease and when it ends, rewritten on renew and release); the disk budget's walker removes a
//!   folder whose lease ended more than `[disk] scratch_grace_s` ago (default an hour: its owner
//!   may still be reading it), unless it was released with `keep: true`. Folders without a marker
//!   are never touched.
//!
//! Leases run for a time limit (like claims), belong to an identity key (a resumed agent keeps
//! them) and are live coordination only: not saved with the world. Wall-clock time is fine here:
//! nothing in the simulation depends on them.

use crate::journal::Writer;
use crate::world::WorldError;
use specula_core::Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// `now + left` in unix seconds (what a scratch folder's marker records).
fn ends_unix(left: Duration) -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).saturating_add(left.as_secs())
}

pub const DEFAULT_LEASE: Duration = Duration::from_secs(600);
pub const MAX_PER_AGENT: usize = 32;
pub const MAX_LEASES: usize = 1024;
/// Ports one lease may hold.
pub const MAX_PORTS: u32 = 16;

#[derive(Clone, Debug, PartialEq)]
pub enum Resource {
    Ports(Vec<u16>),
    Gpu { ms_per_s: u32 },
    Scratch(PathBuf),
}

impl Resource {
    pub fn kind(&self) -> &'static str {
        match self {
            Resource::Ports(_) => "port",
            Resource::Gpu { .. } => "gpu",
            Resource::Scratch(_) => "scratch",
        }
    }
}

/// What `lease_take` asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Want {
    Ports(u32),
    Gpu(u32),
    Scratch,
}

/// The pools leases come from.
#[derive(Clone, Debug)]
pub struct Pools {
    pub ports: (u16, u16),
    /// Never handed out (the engine's own port).
    pub reserved_port: Option<u16>,
    pub gpu_ms_per_s: u32,
    pub scratch_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Lease {
    pub id: u64,
    pub owner: String,
    pub key: String,
    pub resource: Resource,
    pub note: String,
    pub expires: Instant,
}

impl Lease {
    pub fn held_by(&self, w: Writer) -> bool {
        self.owner == w.name && w.session == Some(self.key.as_str())
    }

    pub fn to_json(&self, now: Instant) -> Json {
        let mut f = vec![("id", Json::from(self.id)), ("owner", Json::from(self.owner.as_str())), ("kind", Json::from(self.resource.kind()))];
        match &self.resource {
            Resource::Ports(p) => f.push(("ports", Json::arr(p.iter().map(|x| Json::from(u64::from(*x)))))),
            Resource::Gpu { ms_per_s } => f.push(("gpu_ms_per_s", Json::from(u64::from(*ms_per_s)))),
            Resource::Scratch(d) => f.push(("dir", Json::from(d.display().to_string()))),
        }
        if !self.note.is_empty() {
            f.push(("note", Json::from(self.note.as_str())));
        }
        f.push(("expires_in_s", Json::from((self.expires.saturating_duration_since(now).as_secs_f64() * 10.0).round() / 10.0)));
        Json::obj(f)
    }
}

/// Whether `port` is free: nothing listens on it (the OS's listener table, IPv4 and IPv6, any
/// address: `listening`), and it can be bound on 127.0.0.1 right now. Nothing is ever bound on
/// a wildcard address (that would leave localhost, and makes firewalls ask). Without the table
/// (other platforms) only the loopback bind is tried, which on Windows misses a listener bound to
/// another address on the same port.
pub fn port_free(port: u16, listening: Option<&std::collections::BTreeSet<u16>>) -> bool {
    !listening.is_some_and(|l| l.contains(&port)) && std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// See the module docs.
pub struct Leases {
    leases: BTreeMap<u64, Lease>,
    next: u64,
    /// Makes scratch folder names unique across engine runs.
    run: String,
}

impl Default for Leases {
    fn default() -> Leases {
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Leases { leases: BTreeMap::new(), next: 0, run: format!("{started}-{}", std::process::id()) }
    }
}

fn not_found(id: u64) -> WorldError {
    WorldError::new("lease_not_found", format!("there is no live lease {id} (expired or released?)")).with("lease", id)
}

/// A folder-name-safe form of an agent name (also how the disk budget checks a lease folder's name).
pub(crate) fn folder_safe(name: &str) -> String {
    safe(name)
}

/// A folder-name-safe form of an agent name.
fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(40).collect();
    if s.is_empty() {
        "agent".into()
    } else {
        s
    }
}

/// What `Leases::release` did: the leases released, the folders deleted, and where each deleted
/// folder went (the trash, emptied on a worker).
pub type Released = (Vec<u64>, Vec<String>, Vec<std::path::PathBuf>);

impl Leases {
    pub fn len(&self) -> usize {
        self.leases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }

    pub fn expire(&mut self, now: Instant) {
        self.leases.retain(|_, l| l.expires > now);
    }

    fn live(&self, now: Instant) -> impl Iterator<Item = &Lease> + '_ {
        self.leases.values().filter(move |l| l.expires > now)
    }

    /// GPU time per second leased now.
    pub fn gpu_leased(&self, now: Instant) -> u32 {
        self.live(now).map(|l| if let Resource::Gpu { ms_per_s } = l.resource { ms_per_s } else { 0 }).sum()
    }

    /// `lease_take`: what, a note, and for how long. `free` says whether a port can be used now (see [`port_free`]).
    pub fn take(
        &mut self,
        w: Writer,
        (want, note, lease): (Want, String, Duration),
        pools: &Pools,
        free: &dyn Fn(u16) -> bool,
        now: Instant,
    ) -> Result<&Lease, WorldError> {
        self.expire(now);
        let mine = self.leases.values().filter(|l| l.held_by(w)).count();
        if mine >= MAX_PER_AGENT || self.leases.len() >= MAX_LEASES {
            return Err(WorldError::new(
                "too_many_leases",
                format!("at most {MAX_PER_AGENT} leases per agent and {MAX_LEASES} in all; release some first"),
            )
            .with("yours", mine));
        }
        let resource = match want {
            Want::Ports(count) => {
                let taken: Vec<u16> = self
                    .live(now)
                    .flat_map(|l| match &l.resource {
                        Resource::Ports(p) => p.clone(),
                        _ => Vec::new(),
                    })
                    .collect();
                let (first, last) = pools.ports;
                let ports: Vec<u16> = (first..=last)
                    .filter(|p| Some(*p) != pools.reserved_port && !taken.contains(p) && free(*p))
                    .take(count as usize)
                    .collect();
                if ports.len() < count as usize {
                    return Err(WorldError::new(
                        "no_free_ports",
                        format!(
                            "only {} of the {count} port(s) asked for are free in the pool {first}-{last}; release port leases, \
                             or widen [swarm] lease_ports in perf.toml",
                            ports.len()
                        ),
                    )
                    .with("pool", Json::arr([Json::from(u64::from(first)), Json::from(u64::from(last))]))
                    .with("leased", taken.len())
                    .with("free", ports.len()));
                }
                Resource::Ports(ports)
            }
            Want::Gpu(ms) => {
                let used = self.gpu_leased(now);
                if used + ms > pools.gpu_ms_per_s {
                    let holders: Vec<Json> = self
                        .live(now)
                        .filter(|l| matches!(l.resource, Resource::Gpu { .. }))
                        .take(10)
                        .map(|l| l.to_json(now))
                        .collect();
                    return Err(WorldError::new(
                        "gpu_busy",
                        format!(
                            "{used} of {} GPU ms per second are leased; {ms} more doesn't fit. Ask for less, or wait for a release",
                            pools.gpu_ms_per_s
                        ),
                    )
                    .with("leased_ms_per_s", used)
                    .with("capacity_ms_per_s", pools.gpu_ms_per_s)
                    .with("holders", Json::Arr(holders)));
                }
                Resource::Gpu { ms_per_s: ms }
            }
            Want::Scratch => {
                let dir = pools.scratch_root.join(format!("{}-{}-{}", safe(w.name), self.next + 1, self.run));
                // Through the disk budget: refused below the free-space floor, like any agent's write.
                crate::disk::book().make_dir(crate::disk::Category::Scratch, &dir, crate::disk::Need::Agent).map_err(|e| e.to_world())?;
                crate::disk::write_lease_marker(&dir, self.next + 1, w.name, ends_unix(lease), false).map_err(|e| {
                    WorldError::new("io", format!("could not create the scratch folder {}: {e}", dir.display())).with("dir", dir.display().to_string())
                })?;
                Resource::Scratch(dir)
            }
        };
        self.next += 1;
        let id = self.next;
        let l = Lease { id, owner: w.name.to_owned(), key: w.session.unwrap_or_default().to_owned(), resource, note, expires: now + lease };
        self.leases.insert(id, l);
        Ok(&self.leases[&id])
    }

    fn owned(&self, w: Writer, id: u64, now: Instant) -> Result<&Lease, WorldError> {
        let l = self.leases.get(&id).filter(|l| l.expires > now).ok_or_else(|| not_found(id))?;
        if !l.held_by(w) {
            return Err(WorldError::new("not_owner", format!("lease {id} belongs to {}, not {}", l.owner, w.name)).with("lease", id).with("owner", l.owner.as_str()));
        }
        Ok(l)
    }

    /// `lease_renew`: one lease, or all of `w`'s.
    pub fn renew(&mut self, w: Writer, id: Option<u64>, lease: Duration, now: Instant) -> Result<Vec<Json>, WorldError> {
        let ids: Vec<u64> = match id {
            Some(id) => vec![self.owned(w, id, now)?.id],
            None => self.live(now).filter(|l| l.held_by(w)).map(|l| l.id).collect(),
        };
        let mut out = Vec::new();
        for id in ids {
            if let Some(l) = self.leases.get_mut(&id) {
                l.expires = now + lease;
                if let Resource::Scratch(dir) = &l.resource {
                    let _ = crate::disk::write_lease_marker(dir, l.id, &l.owner, ends_unix(lease), false);
                }
                out.push(l.to_json(now));
            }
        }
        Ok(out)
    }

    /// `lease_release`: one lease, or all of `w`'s; `delete` removes scratch folders too (only
    /// folders inside `scratch_root`, never through a link); `keep` keeps a released folder from
    /// the disk budget's cleanup. Returns the ids released, the folders deleted, and where each
    /// went: a deleted folder is moved into the scratch root's trash in one rename (gone from its
    /// path at once), and the caller empties the trash on a worker (deleting 20,000 files took
    /// 8.5 s of the engine thread: class C, 2026-10-29).
    pub fn release(&mut self, w: Writer, id: Option<u64>, (delete, keep): (bool, bool), scratch_root: &Path, now: Instant) -> Result<Released, WorldError> {
        let ids: Vec<u64> = match id {
            Some(id) => vec![self.owned(w, id, now)?.id],
            None => self.leases.values().filter(|l| l.held_by(w)).map(|l| l.id).collect(),
        };
        let mut deleted = Vec::new();
        let mut trash = Vec::new();
        for id in &ids {
            if let Some(l) = self.leases.remove(id) {
                let Resource::Scratch(dir) = &l.resource else { continue };
                if delete {
                    let root = crate::disk::canonical(scratch_root);
                    if let Ok(t) = crate::disk::move_to_trash(&root, &crate::disk::canonical(dir)) {
                        trash.push(t);
                        deleted.push(dir.display().to_string());
                    }
                } else {
                    // Ended now: the walker removes it after the grace period, unless kept.
                    let _ = crate::disk::write_lease_marker(dir, l.id, &l.owner, ends_unix(Duration::ZERO), keep);
                }
            }
        }
        Ok((ids, deleted, trash))
    }

    /// Live leases, filtered, one page in id order.
    pub fn list(&self, kind: Option<&str>, owner: Option<&str>, after: Option<u64>, limit: usize, now: Instant) -> Json {
        let matching = self.leases.values().filter(move |l| l.expires > now).filter(|l| kind.is_none_or(|k| l.resource.kind() == k) && owner.is_none_or(|o| l.owner == o));
        let total = matching.clone().count();
        let (items, next) = crate::board::page(matching.filter(|l| after.is_none_or(|a| l.id > a)).map(|l| (l.id, l.to_json(now))), limit);
        Json::obj([("total", Json::from(total)), ("leases", Json::Arr(items)), ("next_cursor", Json::from(next))])
    }

    /// The folders of live scratch leases (the disk budget never cleans them up).
    pub fn scratch_dirs(&self, now: Instant) -> Vec<PathBuf> {
        self.live(now).filter_map(|l| if let Resource::Scratch(d) = &l.resource { Some(d.clone()) } else { None }).collect()
    }

    /// Live leases held by `name`, for roll-ups.
    pub fn of(&self, name: &str, now: Instant) -> usize {
        self.live(now).filter(|l| l.owner == name).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: Writer = Writer { name: "ann", session: Some("k1") };
    const BOB: Writer = Writer { name: "bob/x", session: Some("k2") };

    fn pools(root: &Path) -> Pools {
        Pools { ports: (7500, 7505), reserved_port: Some(7501), gpu_ms_per_s: 1000, scratch_root: root.to_path_buf() }
    }

    #[test]
    fn ports_are_handed_out_once_and_skip_busy_and_reserved_ones() {
        let now = Instant::now();
        let root = std::env::temp_dir();
        let mut l = Leases::default();
        let busy = |p: u16| p != 7502; // someone else is listening on 7502
        let a = l.take(ANN, (Want::Ports(2), "engine".into(), DEFAULT_LEASE), &pools(&root), &busy, now).unwrap();
        assert_eq!(a.resource, Resource::Ports(vec![7500, 7503]));
        let b = l.take(BOB, (Want::Ports(2), String::new(), DEFAULT_LEASE), &pools(&root), &busy, now).unwrap();
        assert_eq!(b.resource, Resource::Ports(vec![7504, 7505]));
        let e = l.take(BOB, (Want::Ports(1), String::new(), DEFAULT_LEASE), &pools(&root), &busy, now).unwrap_err();
        assert_eq!(e.code, "no_free_ports");
        // Released or expired ports come back.
        l.release(ANN, Some(1), (false, false), &root, now).unwrap();
        assert!(l.take(BOB, (Want::Ports(1), String::new(), DEFAULT_LEASE), &pools(&root), &busy, now).is_ok());
        assert_eq!(l.release(ANN, Some(2), (false, false), &root, now).unwrap_err().code, "not_owner");
        let later = now + DEFAULT_LEASE;
        assert!(l.take(ANN, (Want::Ports(2), String::new(), DEFAULT_LEASE), &pools(&root), &busy, later).is_ok());
    }

    /// Review findings: a listener on another address left the loopback bind free on Windows,
    /// and probing with a wildcard bind left localhost (and made the firewall ask). The listener
    /// table sees every listener without opening a socket.
    #[test]
    fn a_port_someone_listens_on_is_not_free() {
        let lo = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = lo.local_addr().unwrap().port();
        assert!(!port_free(port, None), "the loopback bind fails");
        let other = port.wrapping_add(1).max(1);
        let table: std::collections::BTreeSet<u16> = [other].into();
        assert!(!port_free(other, Some(&table)), "a port in the table is busy whatever the bind says");
        match specula_sys::net::tcp_listening_ports() {
            Ok(ports) => assert!(!port_free(port, Some(&ports)) && ports.contains(&port)),
            Err(e) => eprintln!("listener table unavailable here: {e}"),
        }
    }

    #[test]
    fn gpu_time_is_shared_out_of_a_capacity() {
        let now = Instant::now();
        let root = std::env::temp_dir();
        let mut l = Leases::default();
        let free = |_| true;
        l.take(ANN, (Want::Gpu(600), String::new(), DEFAULT_LEASE), &pools(&root), &free, now).unwrap();
        let e = l.take(BOB, (Want::Gpu(500), String::new(), DEFAULT_LEASE), &pools(&root), &free, now).unwrap_err();
        assert_eq!((e.code, e.detail("leased_ms_per_s")), ("gpu_busy", Some(&Json::from(600u32))));
        l.take(BOB, (Want::Gpu(400), String::new(), DEFAULT_LEASE), &pools(&root), &free, now).unwrap();
        assert_eq!(l.gpu_leased(now), 1000);
        // Renewing keeps it past the first expiry.
        l.renew(ANN, None, DEFAULT_LEASE * 2, now).unwrap();
        assert_eq!(l.gpu_leased(now + DEFAULT_LEASE), 600);
    }

    #[test]
    fn scratch_folders_are_fresh_and_deleted_only_on_request() {
        let now = Instant::now();
        let root = std::env::temp_dir().join(format!("specula-lease-test-{}", std::process::id()));
        let mut l = Leases::default();
        let free = |_| true;
        let a = l.take(ANN, (Want::Scratch, String::new(), DEFAULT_LEASE), &pools(&root), &free, now).unwrap().clone();
        let b = l.take(BOB, (Want::Scratch, String::new(), DEFAULT_LEASE), &pools(&root), &free, now).unwrap().clone();
        let (Resource::Scratch(da), Resource::Scratch(db)) = (&a.resource, &b.resource) else { panic!() };
        assert_ne!(da, db);
        assert!(da.is_dir() && db.is_dir());
        assert!(db.file_name().unwrap().to_string_lossy().starts_with("bob_x-"), "names are folder-safe");
        std::fs::write(da.join("f.txt"), "x").unwrap();
        assert!(da.join(crate::disk::LEASE_MARKER).is_file(), "marked with its lease");
        l.release(ANN, Some(a.id), (false, true), &root, now).unwrap();
        assert!(da.is_dir(), "kept without delete");
        let marker = std::fs::read_to_string(da.join(crate::disk::LEASE_MARKER)).unwrap();
        assert!(marker.contains("\"keep\":true"), "{marker}");
        let (_, deleted, trash) = l.release(BOB, None, (true, false), &root, now).unwrap();
        assert_eq!(deleted.len(), 1);
        assert!(!db.exists(), "gone from its path at once");
        assert_eq!(trash.len(), 1);
        let (files, _) = crate::disk::remove_tree_inside(&crate::disk::canonical(&root), &trash[0]).unwrap();
        assert!(files >= 1 && !trash[0].exists(), "the trash is emptied by the caller");
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(l.list(None, None, None, 10, now).get("total"), Some(&Json::from(0)));
    }
}

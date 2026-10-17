//! Resource leases: ports, GPU time and scratch folders handed out per agent, so swarms that run
//! their own engines (headless playtests) don't collide.
//!
//! Earlier sessions' lanes clobbered a shared scratch folder, killed each other's engines and
//! collided on ports. A lease is the engine's answer to "which port / folder / share of the GPU is
//! mine": an agent takes it before starting its own engine or writing scratch files, renews it
//! while it works, and releases it after.
//!
//! - **Ports**: `count` free ports from the pool (`perf.toml` `[swarm] lease_ports`, default
//!   7500-7599, never this engine's own port). A port is handed out only when no live lease has it
//!   *and* it can be bound on 127.0.0.1 right now (another program may be using it).
//! - **GPU time**: a share in milliseconds of GPU time per second, out of `gpu_ms_per_s` (1000 =
//!   one whole GPU). Advisory: engines don't enforce it, but agents that lease before rendering
//!   heavily stay within what the machine can give (the window keeps its frames).
//! - **Scratch folders**: a fresh folder of its own under the scratch root, never reused by a
//!   later lease. `lease_release {delete: true}` removes it with everything in it; a lease that
//!   just expires leaves its folder alone (its owner may still be writing there).
//!
//! Leases run for a time limit (like claims), belong to an identity key (a resumed agent keeps
//! them) and are live coordination only: not saved with the world. Wall-clock time is fine here:
//! nothing in the simulation depends on them.

use crate::journal::Writer;
use crate::world::WorldError;
use specula_core::Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// Whether a port can be bound on loopback right now.
pub fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
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

/// A folder-name-safe form of an agent name.
fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(40).collect();
    if s.is_empty() {
        "agent".into()
    } else {
        s
    }
}

impl Leases {
    pub fn len(&self) -> usize {
        self.leases.len()
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

    /// `lease_take`. `free` says whether a port can be used now (see [`port_free`]).
    pub fn take(
        &mut self,
        w: Writer,
        want: Want,
        note: String,
        lease: Duration,
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
                std::fs::create_dir_all(&dir).map_err(|e| {
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
                out.push(l.to_json(now));
            }
        }
        Ok(out)
    }

    /// `lease_release`: one lease, or all of `w`'s; `delete` removes scratch folders too (only
    /// folders under `scratch_root`). Returns the ids released and the folders deleted.
    pub fn release(&mut self, w: Writer, id: Option<u64>, delete: bool, scratch_root: &Path, now: Instant) -> Result<(Vec<u64>, Vec<String>), WorldError> {
        let ids: Vec<u64> = match id {
            Some(id) => vec![self.owned(w, id, now)?.id],
            None => self.leases.values().filter(|l| l.held_by(w)).map(|l| l.id).collect(),
        };
        let mut deleted = Vec::new();
        for id in &ids {
            if let Some(l) = self.leases.remove(id) {
                if let (true, Resource::Scratch(dir)) = (delete, &l.resource) {
                    if dir.starts_with(scratch_root) && std::fs::remove_dir_all(dir).is_ok() {
                        deleted.push(dir.display().to_string());
                    }
                }
            }
        }
        Ok((ids, deleted))
    }

    /// Live leases, filtered, one page in id order.
    pub fn list(&self, kind: Option<&str>, owner: Option<&str>, after: Option<u64>, limit: usize, now: Instant) -> Json {
        let matching = self.leases.values().filter(move |l| l.expires > now).filter(|l| kind.is_none_or(|k| l.resource.kind() == k) && owner.is_none_or(|o| l.owner == o));
        let total = matching.clone().count();
        let (items, next) = crate::board::page(matching.filter(|l| after.is_none_or(|a| l.id > a)).map(|l| (l.id, l.to_json(now))), limit);
        Json::obj([("total", Json::from(total)), ("leases", Json::Arr(items)), ("next_cursor", Json::from(next))])
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
        let a = l.take(ANN, Want::Ports(2), "engine".into(), DEFAULT_LEASE, &pools(&root), &busy, now).unwrap();
        assert_eq!(a.resource, Resource::Ports(vec![7500, 7503]));
        let b = l.take(BOB, Want::Ports(2), String::new(), DEFAULT_LEASE, &pools(&root), &busy, now).unwrap();
        assert_eq!(b.resource, Resource::Ports(vec![7504, 7505]));
        let e = l.take(BOB, Want::Ports(1), String::new(), DEFAULT_LEASE, &pools(&root), &busy, now).unwrap_err();
        assert_eq!(e.code, "no_free_ports");
        // Released or expired ports come back.
        l.release(ANN, Some(1), false, &root, now).unwrap();
        assert!(l.take(BOB, Want::Ports(1), String::new(), DEFAULT_LEASE, &pools(&root), &busy, now).is_ok());
        assert_eq!(l.release(ANN, Some(2), false, &root, now).unwrap_err().code, "not_owner");
        let later = now + DEFAULT_LEASE;
        assert!(l.take(ANN, Want::Ports(2), String::new(), DEFAULT_LEASE, &pools(&root), &busy, later).is_ok());
    }

    #[test]
    fn gpu_time_is_shared_out_of_a_capacity() {
        let now = Instant::now();
        let root = std::env::temp_dir();
        let mut l = Leases::default();
        let free = |_| true;
        l.take(ANN, Want::Gpu(600), String::new(), DEFAULT_LEASE, &pools(&root), &free, now).unwrap();
        let e = l.take(BOB, Want::Gpu(500), String::new(), DEFAULT_LEASE, &pools(&root), &free, now).unwrap_err();
        assert_eq!((e.code, e.detail("leased_ms_per_s")), ("gpu_busy", Some(&Json::from(600u32))));
        l.take(BOB, Want::Gpu(400), String::new(), DEFAULT_LEASE, &pools(&root), &free, now).unwrap();
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
        let a = l.take(ANN, Want::Scratch, String::new(), DEFAULT_LEASE, &pools(&root), &free, now).unwrap().clone();
        let b = l.take(BOB, Want::Scratch, String::new(), DEFAULT_LEASE, &pools(&root), &free, now).unwrap().clone();
        let (Resource::Scratch(da), Resource::Scratch(db)) = (&a.resource, &b.resource) else { panic!() };
        assert_ne!(da, db);
        assert!(da.is_dir() && db.is_dir());
        assert!(db.file_name().unwrap().to_string_lossy().starts_with("bob_x-"), "names are folder-safe");
        std::fs::write(da.join("f.txt"), "x").unwrap();
        l.release(ANN, Some(a.id), false, &root, now).unwrap();
        assert!(da.is_dir(), "kept without delete");
        let (_, deleted) = l.release(BOB, None, true, &root, now).unwrap();
        assert_eq!(deleted.len(), 1);
        assert!(!db.exists());
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(l.list(None, None, None, 10, now).get("total"), Some(&Json::from(0)));
    }
}

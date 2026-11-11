//! Who may load a module, and how a builder proposes one (docs/design/MODULES.md, "Who may load
//! a module").
//!
//! A module is native code: once loaded it runs inside the engine's process with the owner's
//! user rights, and no source check can make that safe (safe Rust can start programs and write
//! files). So `logic_build`, `logic_reload` and `logic_unload` need the owner or an
//! operator-granted lead ([`crate::authority::may_load_native`]).
//!
//! Any other agent's `logic_build` becomes a **proposal**: the engine records the module's folder,
//! who proposed it and the digest of every file in the folder ([`specula_logic::digest`]), and
//! answers `lead_only` with the proposal. The owner or lead lists proposals in `logic_status`,
//! reviews the files and runs `logic_build {path, approve: <id>}`, which builds only when the
//! folder still has the proposed digest, before the build and after it (`sources_changed`
//! otherwise). Nothing is built or loaded for a proposal until then.
//!
//! Refused loads, reloads and unloads are reported through [`report_refusal`], the seam for
//! the security monitor (m5-inputguard): a builder proposing is ordinary work, but loading a
//! library or unloading another agent's module without authority is an attempt worth an alert.

use specula_core::Json;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Proposals kept at most (the oldest go first). One per (agent, folder): a newer one replaces it.
pub const MAX_PROPOSALS: usize = 32;

/// A builder's request to load its module, waiting for the owner or a granted lead.
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub dir: PathBuf,
    /// The proposer's agent name and identity key.
    pub by: String,
    pub key: String,
    pub tick: u64,
    /// The folder's digest when proposed ([`specula_logic::digest::Digest::hex`]).
    pub digest: String,
    pub files: usize,
}

impl Proposal {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("id", Json::from(self.id)),
            ("path", Json::from(self.dir.display().to_string())),
            ("by", Json::from(self.by.as_str())),
            ("tick", Json::from(self.tick)),
            ("digest", Json::from(self.digest.as_str())),
            ("files", Json::from(self.files)),
        ])
    }
}

/// The proposals waiting (engine thread; part of the module host, not of the simulation).
#[derive(Default)]
pub struct Proposals {
    seq: u64,
    list: VecDeque<Proposal>,
}

impl Proposals {
    /// Records a proposal (replacing the same agent's earlier one for the same folder).
    pub fn propose(&mut self, dir: &Path, by: &str, key: &str, tick: u64, digest: &specula_logic::digest::Digest) -> &Proposal {
        self.list.retain(|p| !(p.key == key && p.dir == dir));
        while self.list.len() >= MAX_PROPOSALS {
            self.list.pop_front();
        }
        self.seq += 1;
        self.list.push_back(Proposal { id: self.seq, dir: dir.to_owned(), by: by.to_owned(), key: key.to_owned(), tick, digest: digest.hex.clone(), files: digest.files });
        self.list.back().expect("just pushed")
    }

    /// Takes proposal `id` for an approval of `dir` whose folder hashes `digest` now.
    pub fn approve(&mut self, id: u64, dir: &Path, digest: &str) -> Result<Proposal, crate::world::WorldError> {
        use crate::world::WorldError;
        let Some(at) = self.list.iter().position(|p| p.id == id) else {
            return Err(WorldError::new("no_proposal", format!("no proposal {id} is waiting (logic_status lists them)")).with("approve", id));
        };
        let p = &self.list[at];
        if p.dir != dir {
            return Err(WorldError::new("no_proposal", format!("proposal {id} is for {}, not this path", p.dir.display())).with("approve", id).with("proposal_path", p.dir.display().to_string()));
        }
        if p.digest != digest {
            return Err(WorldError::new(
                "sources_changed",
                format!("the module folder changed since {} proposed it (proposal {id}): review the files again; {} must propose again", p.by, p.by),
            )
            .with("approve", id)
            .with("proposed_digest", p.digest.as_str())
            .with("digest", digest));
        }
        Ok(self.list.remove(at).expect("found above"))
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn json(&self) -> Json {
        Json::arr(self.list.iter().map(Proposal::to_json))
    }
}

/// A native-code call refused for lack of authority: what the security monitor gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRefusal {
    /// A builder's `logic_build`, filed as proposal `proposal` (ordinary work: not an incident).
    Proposed { dir: PathBuf, proposal: Option<u64> },
    /// `logic_build` by an agent without authority that wasn't filed: an `approve`, or a folder
    /// that can't be digested.
    Build { dir: PathBuf },
    /// `logic_reload`: loading a library without authority.
    Reload,
    /// `logic_unload` of a module without authority.
    Unload { module: String },
}

impl NativeRefusal {
    pub fn kind(&self) -> &'static str {
        match self {
            NativeRefusal::Proposed { .. } => "proposed",
            NativeRefusal::Build { .. } => "build",
            NativeRefusal::Reload => "reload",
            NativeRefusal::Unload { .. } => "unload",
        }
    }

    /// Whether the security monitor should treat it as an attempt (everything but a proposal).
    pub fn is_attempt(&self) -> bool {
        !matches!(self, NativeRefusal::Proposed { .. })
    }
}

/// The seam to the security monitor: every refused native-code call comes here (`by` is the
/// agent's name, `key` its identity). Today a trace event (`logic.native_refused`); when the
/// m5-inputguard monitor merges, attempts ([`NativeRefusal::is_attempt`]) go to its
/// `inputguard::refused` as well.
pub fn report_refusal(by: &str, key: &str, r: &NativeRefusal) {
    let _ = key;
    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "logic.native_refused", by = by, kind = r.kind(), attempt = r.is_attempt());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(hex: &str) -> specula_logic::digest::Digest {
        specula_logic::digest::Digest { hex: hex.into(), files: 2, bytes: 10 }
    }

    #[test]
    fn proposals_are_bounded_replaced_and_pinned() {
        let mut p = Proposals::default();
        let dir = PathBuf::from("games/m");
        let a = p.propose(&dir, "ann", "ka", 1, &digest("aa")).id;
        let b = p.propose(&dir, "ann", "ka", 2, &digest("bb")).id;
        assert_eq!(p.len(), 1, "ann's newer proposal for the same folder replaces the older");
        assert_eq!(p.approve(a, &dir, "bb").unwrap_err().code, "no_proposal");
        assert_eq!(p.approve(b, Path::new("games/other"), "bb").unwrap_err().code, "no_proposal", "another folder");
        assert_eq!(p.approve(b, &dir, "cc").unwrap_err().code, "sources_changed", "the folder changed since");
        assert_eq!(p.approve(b, &dir, "bb").unwrap().by, "ann");
        assert_eq!(p.len(), 0, "an approval takes it");
        for i in 0..(MAX_PROPOSALS as u64 + 10) {
            p.propose(&PathBuf::from(format!("games/m{i}")), "bob", "kb", i, &digest("dd"));
        }
        assert_eq!(p.len(), MAX_PROPOSALS, "bounded");
    }

    #[test]
    fn only_attempts_are_incidents() {
        assert!(!NativeRefusal::Proposed { dir: PathBuf::new(), proposal: Some(1) }.is_attempt());
        assert!(NativeRefusal::Reload.is_attempt());
        assert!(NativeRefusal::Unload { module: "m".into() }.is_attempt());
        assert!(NativeRefusal::Build { dir: PathBuf::new() }.is_attempt());
    }
}

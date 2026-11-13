//! `disk_status` and `disk_prune`: the disk footprint budget as data (see [`crate::disk`] and
//! docs/design/DISK.md). Both run on the calling agent's protocol thread (the book is shared);
//! only the lead check of a forced prune asks the engine.

use crate::disk;
use crate::tools::{bool_arg, check_keys, int_arg, Command, Tools};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// Uploads listed by disk_status.
const LIST_UPLOADS: usize = 16;

/// Answers a `disk_*` call.
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let out = match name {
        "disk_status" => status(tools, session, args),
        "disk_prune" => prune(tools, session, args),
        other => Err(format!("unknown disk tool '{other}'")),
    };
    out.unwrap_or_else(|m| ToolOutput::error(crate::tools::argument_error(&m).to_string()))
}

fn status(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "disk_status", &[&["detailed", "measure"]])?;
    let detailed = bool_arg(args, "detailed")?.unwrap_or(false);
    let measured = bool_arg(args, "measure")?.unwrap_or(false) && disk::book().measure_for_agent();
    let mut j = disk::book().report(detailed);
    if bool_arg(args, "measure")?.unwrap_or(false) && !measured {
        j.set("measure", Json::from("skipped: measured less than 10 s ago (these are the background walker's numbers)"));
    }
    if let Some(c) = &tools.content {
        let (n, b) = c.stats();
        j.set("content_store", Json::obj([("entries", Json::from(n)), ("mb", Json::from((b as f64 / 1048576.0 * 10.0).round() / 10.0))]));
    }
    // Only the caller's own uploads (an upload id is the agent's to continue, commit or abort).
    if let Some(owner) = crate::project_binary::owner(tools, session) {
        let ups = tools.uploads.list(&owner, LIST_UPLOADS);
        if !ups.is_empty() {
            j.set("uploads", Json::Arr(ups));
        }
    }
    Ok(ToolOutput::text(j.to_string()))
}

fn prune(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "disk_prune", &[&["dry_run", "free_mb"]])?;
    let dry_run = bool_arg(args, "dry_run")?.unwrap_or(false);
    let free_mb = int_arg(args, "free_mb", 1, 1 << 24)?;
    // Freeing more than the budget needs deletes captures nobody asked to lose: the lead decides.
    if free_mb.is_some() && !dry_run {
        let gate = tools.send(session, Command::DiskGate);
        if gate.is_error {
            return Ok(gate);
        }
    }
    let at_least = free_mb.unwrap_or(0) << 20;
    let by = format!("disk_prune ({})", session.client_name);
    Ok(match disk::book().prune_for_agent(dry_run, &by, at_least) {
        Ok(j) if j.get("error").is_some() => ToolOutput::error(j.to_string()),
        Ok(j) => ToolOutput::text(j.to_string()),
        Err(e) => ToolOutput::error(e.to_json().to_string()),
    })
}

impl crate::Engine {
    /// The lead check before a forced prune.
    pub(crate) fn disk_gate(&self, session: &SessionInfo) -> ToolOutput {
        let have = crate::authority::authority(&self.swarm, &session.id);
        match crate::authority::require(
            have,
            crate::authority::Authority::Lead,
            crate::authority::Site::DiskPrune,
            "free_mb (it deletes more than the budget needs, the oldest captures first; anyone may prune what the budget needs, without free_mb, or dry_run)",
        ) {
            Ok(()) => ToolOutput::text("{}"),
            Err(e) => ToolOutput::error(e.to_json().to_string()),
        }
    }

    /// `engine_info.disk`: the book's summary, plus what only the engine knows (the journal's
    /// binary history).
    pub(crate) fn disk_json(&self) -> Json {
        // (As the book holds it: the engine thread never asks the system for free space.)
        let mut j = disk::book().report_as_read(false);
        let (held, budget) = self.swarm.journal().file_blobs();
        j.set("journal_binary_history_mb", Json::obj([("held", Json::from(held >> 20)), ("budget", Json::from(budget >> 20))]));
        j
    }

    /// Once in a while (and after file changes): the project store's size for the budget, and the
    /// content the store, the journal and the uploads still need (never pruned).
    pub(crate) fn disk_pump(&mut self) {
        // Review evidence the queue dropped goes (cheap when the queue didn't change).
        self.evidence_gc();
        let Some(content) = self.content.clone() else { return };
        let key = (self.world.files_revision(), self.swarm.journal().last_seq());
        if self.disk_seen == Some(key) && self.disk_last.elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        if self.disk_last.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }
        self.disk_seen = Some(key);
        self.disk_last = std::time::Instant::now();
        let files = self.world.files();
        let text: u64 = files.values().filter(|f| !f.kind.is_binary()).map(|f| f.text.len() as u64).sum();
        let binary: u64 = files.values().map(crate::project::binary_bytes).sum();
        disk::book().set_project(text + binary, files.len() as u64);
        let mut ids: std::collections::BTreeSet<crate::blobs::BlobId> = files.values().filter(|f| f.kind.is_binary()).filter_map(|f| crate::blobs::parse_pointer(&f.text)).map(|(id, _)| id).collect();
        for p in self.swarm.journal().blob_pointers() {
            if let Some((id, _)) = crate::blobs::parse_pointer(&p) {
                ids.insert(id);
            }
        }
        content.set_referenced(ids);
        // Live scratch leases are never cleaned up, whatever their markers say.
        let live = self.swarm.leases.scratch_dirs(std::time::Instant::now()).iter().map(|d| disk::canonical(d)).collect();
        disk::book().set_live_scratch(live);
    }
}

/// The tools' schemas.
pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let obj = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::Arr(Vec::new()))]);
    vec![
        ToolDef {
            name: "disk_status".into(),
            description: "The disk footprint budget as data: free space on each volume the engine writes to, the engine's own use by category (captures, cache, content, modules, scratch, saves, worlds, project) against perf.toml [disk] (total_mb, per-category caps, free_floor_mb), the state (ok, warn, over_budget, below_floor), the current warning, and what was pruned or refused. Over budget the engine prunes the oldest captures (older than [disk] capture_min_age_s), then content nothing needs, then expired scratch, never more than helps; it never prunes saves, world files, journals, project files or review evidence (review_submit and critique_add keep copies of their captures beside the world's queue). Agents' writes are refused below the free-space floor (disk_floor, with what to delete).".into(),
            input_schema: obj(vec![
                ("detailed", t("boolean", "Also each category's folders and when it was measured, bytes written this run by writer (sight, studio, vfx, ...), and the prune log.")),
                ("measure", t("boolean", "Measure every folder now instead of using the background walker's numbers (bounded; slower).")),
            ]),
        },
        ToolDef {
            name: "disk_prune".into(),
            description: "Prunes now what the budget allows, in order: the oldest captures (this run's and finished runs', older than [disk] capture_min_age_s, never another running engine's; reviews keep their own copies), then content no file or kept journal entry needs, then scratch folders whose lease ended more than [disk] scratch_grace_s ago. Without free_mb it frees only what the budget needs, capped at what is prunable (and expired scratch). dry_run lists what would go. A real prune at most once per 10 s per engine (too_soon); busy while another runs.".into(),
            input_schema: obj(vec![
                ("dry_run", t("boolean", "Report what would go; delete nothing.")),
                ("free_mb", t("integer", "Free at least this much even under budget (lead only, unless dry_run).")),
            ]),
        },
    ]
}

//! Results the engine recorded itself: what `style_check` and `studio_check` found and the
//! captures the engine wrote (`sight_frame`, the studio's sheets ...), kept by id so a review
//! submission can reference them (`review_submit {results: [{id, epoch}]}`) instead of retyping
//! them as reported checks (docs/design/AUTHORITY.md, "Provenance by type").
//!
//! - **Who made it.** Each record keeps the session that ran the tool; a submission may
//!   reference only its submitter's own records (`foreign_result` names who made it otherwise),
//!   like a perf capture belongs to its owner.
//! - **Which run.** Ids count from 1 in each engine run; `epoch` names the run, so an id from an
//!   earlier run (or another engine) is refused (`stale_result`) instead of naming another
//!   result.
//! - **The bytes the engine made, by SHA-256.** Agents can write files (captures, recipes,
//!   `style.toml`), so a record vouches only through a digest from the OS (`specula_sys::digest`,
//!   never hand-rolled; lane review: the unkeyed evidence name was forged with one solved word).
//!   A capture record keeps the SHA-256 of the PNG and of the packet the engine wrote; a check
//!   record keeps, per subject, the SHA-256 of everything it judged: the recipe's text, the
//!   recipes it includes, its params and seed, and the style files (`style.json`, and the
//!   `style.toml` whose rules it applied). Where the OS has no digest yet, nothing is recorded
//!   and the answer says so (`not_recorded`).
//! - **Checked on the agent's connection.** `review_submit`'s references are verified on the
//!   caller's thread ([`prepare`] and [`verify`]): a check's inputs read again and digested; a
//!   capture's evidence copy (the bytes the queue keeps) digested. Each verified reference leaves
//!   a short-lived token for that session; the engine thread accepts a reference only with its
//!   token (a playtest step's call, which doesn't come through there, can't reference results).
//! - **Bounded.** At most [`MAX_RECORDS`] records and [`MAX_BYTES`] of them; the oldest go
//!   first (`result_not_found` says so). Subjects, errors and tokens are capped.
//! - **Not simulation state.** Kept in memory only, never saved, never hashed.

use crate::review::{Recorded, ResultRef};
use crate::world::{ModelSpec, WorldError};
use specula_core::Json;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Records kept (the oldest go first), and their size in all.
pub const MAX_RECORDS: usize = 4096;
pub const MAX_BYTES: usize = 8 << 20;
/// References one submission may make.
pub const MAX_REFS: usize = 16;
/// Subjects one check record keeps (a style_check takes at most 96), and errors per subject.
pub const MAX_SUBJECTS: usize = 96;
pub const MAX_ERRORS: usize = 8;
/// Longest epoch accepted in a reference.
pub const MAX_EPOCH: usize = 40;
/// Verified references waiting for the engine thread, and how long one stays good.
const MAX_TOKENS: usize = 256;
const TOKEN_LIFE: Duration = Duration::from_secs(60);
/// Largest file a capture's verification reads (the evidence cap).
const MAX_FILE: u64 = crate::evidence::MAX_FILE;

pub type Digest = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    StyleCheck,
    StudioCheck,
    Capture,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::StyleCheck => "style_check",
            Kind::StudioCheck => "studio_check",
            Kind::Capture => "capture",
        }
    }
}

/// One subject a check looked at.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub name: String,
    pub spec: ModelSpec,
    /// SHA-256 of what it judged ([`inputs_digest`]); `None`: it can't be vouched for (a glTF
    /// asset, the recipe changed during the check, or no OS digest here).
    pub inputs: Option<Digest>,
    pub passed: bool,
    /// The failed checks' names, at most [`MAX_ERRORS`].
    pub errors: Vec<String>,
    /// How many failed in all.
    pub failed: usize,
}

/// A capture the engine wrote, with the SHA-256 of the bytes it wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureFile {
    pub png: PathBuf,
    pub png_sha: Digest,
    pub packet: Option<(PathBuf, Digest)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: u64,
    pub kind: Kind,
    /// The tool that made it (`style_check`, `studio_check`, `sight`, `studio` ...).
    pub tool: String,
    /// The session that ran it.
    pub session: String,
    pub subjects: Vec<Subject>,
    /// Subjects beyond [`MAX_SUBJECTS`] (counted, not kept: such a record can't back a pass).
    pub more_subjects: usize,
    pub capture: Option<CaptureFile>,
    size: usize,
}

/// A spec as people read it: `recipes/statue_angel.json{wings=0.85}#7`.
pub fn spec_label(spec: &ModelSpec) -> String {
    let mut s = spec.recipe.clone();
    if !spec.params.is_empty() {
        s.push('{');
        s.push_str(&spec.params.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(","));
        s.push('}');
    }
    if let Some(seed) = spec.seed {
        s.push_str(&format!("#{seed}"));
    }
    s
}

impl Record {
    /// Whether every subject passed (and none was left out).
    pub fn passed(&self) -> bool {
        self.more_subjects == 0 && self.subjects.iter().all(|s| s.passed)
    }

    pub fn failed(&self) -> usize {
        self.subjects.iter().filter(|s| !s.passed).count()
    }

    /// The provenance a submission keeps: the record, and every subject it judged.
    pub fn provenance(&self, epoch: &str) -> Recorded {
        let subjects = self.subjects.iter().take(crate::review::MAX_RECORDED_SUBJECTS).map(|s| (spec_label(&s.spec), s.passed)).collect();
        let more = self.subjects.len().saturating_sub(crate::review::MAX_RECORDED_SUBJECTS) + self.more_subjects;
        Recorded { tool: self.tool.clone(), id: self.id, epoch: epoch.to_owned(), subjects, more_subjects: more }
    }

    fn measure(&self) -> usize {
        let subjects: usize = self.subjects.iter().map(|s| 128 + s.name.len() + s.spec.recipe.len() + 16 * s.spec.params.len() + s.errors.iter().map(|e| 16 + e.len()).sum::<usize>()).sum();
        let capture = self.capture.as_ref().map_or(0, |c| 128 + c.png.as_os_str().len() + c.packet.as_ref().map_or(0, |p| 32 + p.0.as_os_str().len()));
        128 + self.tool.len() + self.session.len() + subjects + capture
    }
}

/// A reference verified on the caller's thread, for the engine thread to accept.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub session: String,
    pub record: u64,
    /// For a capture: the evidence references (`evidence/<file>`) whose bytes were verified.
    pub png_ref: Option<String>,
    pub packet_ref: Option<String>,
    at: Instant,
}

/// The store: one per engine run, shared by the threads that record and the one that submits.
#[derive(Debug)]
pub struct Records {
    epoch: String,
    next: u64,
    list: VecDeque<Record>,
    bytes: usize,
    tokens: VecDeque<Token>,
}

impl Default for Records {
    fn default() -> Records {
        Records::with_epoch(format!("{:016x}", crate::journal::run_nonce()))
    }
}

/// `{id, epoch}`: what an answer says of the record it made.
pub fn answer_json(id: u64, epoch: &str) -> Json {
    Json::obj([("id", Json::from(id)), ("epoch", Json::from(epoch))])
}

/// What an answer says when nothing could be recorded: `{code, message}`.
pub fn not_recorded_json(code: &str, message: &str) -> Json {
    Json::obj([("code", Json::from(code)), ("message", Json::from(message))])
}

impl Records {
    pub fn with_epoch(epoch: String) -> Records {
        Records { epoch, next: 1, list: VecDeque::new(), bytes: 0, tokens: VecDeque::new() }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Keeps a record (its id is assigned here) and returns the id. Subjects past the cap are
    /// counted in `more_subjects`; errors past theirs are cut.
    pub fn add(&mut self, kind: Kind, tool: &str, session: &str, mut subjects: Vec<Subject>, capture: Option<CaptureFile>) -> u64 {
        let more_subjects = subjects.len().saturating_sub(MAX_SUBJECTS);
        subjects.truncate(MAX_SUBJECTS);
        for s in &mut subjects {
            s.errors.truncate(MAX_ERRORS);
        }
        let id = self.next;
        self.next += 1;
        let tool: String = tool.chars().take(64).collect();
        let session: String = session.chars().take(128).collect();
        let mut r = Record { id, kind, tool, session, subjects, more_subjects, capture, size: 0 };
        r.size = r.measure();
        self.bytes += r.size;
        self.list.push_back(r);
        while self.list.len() > MAX_RECORDS || (self.bytes > MAX_BYTES && self.list.len() > 1) {
            if let Some(old) = self.list.pop_front() {
                self.bytes -= old.size;
            }
        }
        id
    }

    /// The record `id` of this run, if still kept.
    pub fn get(&self, id: u64) -> Option<&Record> {
        let first = self.list.front()?.id;
        let i = id.checked_sub(first)? as usize;
        self.list.get(i).filter(|r| r.id == id)
    }

    /// The record a reference names: this run's, still kept. Who made it is checked by the
    /// caller (`foreign_result`), which knows identities.
    pub fn resolve(&self, i: usize, r: &ResultRef) -> Result<&Record, WorldError> {
        if r.epoch != self.epoch {
            return Err(WorldError::new(
                "stale_result",
                format!("results[{i}]: result {} was recorded by another engine run (epoch {}; this run is {}): ids name other results now, so run the check again", r.id, r.epoch, self.epoch),
            )
            .with("result", r.id)
            .with("epoch", r.epoch.as_str())
            .with("current_epoch", self.epoch.as_str()));
        }
        self.get(r.id).ok_or_else(|| {
            let why = if r.id < self.next { format!("only the newest {MAX_RECORDS} results are kept") } else { "no result has that id yet".to_owned() };
            WorldError::new("result_not_found", format!("results[{i}]: there is no result {} in this engine run ({why}): run the check again", r.id)).with("result", r.id)
        })
    }

    /// A verified reference, replacing the session's earlier one for the same record.
    fn put_token(&mut self, session: &str, record: u64, png_ref: Option<String>, packet_ref: Option<String>) {
        self.tokens.retain(|t| !(t.session == session && t.record == record) && t.at.elapsed() < TOKEN_LIFE);
        if self.tokens.len() >= MAX_TOKENS {
            self.tokens.pop_front();
        }
        self.tokens.push_back(Token { session: session.to_owned(), record, png_ref, packet_ref, at: Instant::now() });
    }

    /// Takes the session's verified token for `record` (fresh ones only).
    pub fn take_token(&mut self, session: &str, record: u64) -> Option<Token> {
        let i = self.tokens.iter().position(|t| t.session == session && t.record == record)?;
        self.tokens.remove(i).filter(|t| t.at.elapsed() < TOKEN_LIFE)
    }
}

fn refused(e: WorldError) -> specula_protocol::ToolOutput {
    specula_protocol::ToolOutput::error(e.to_json().to_string())
}

/// The texts a check judged (or would judge now).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckInputs {
    pub recipe: String,
    /// Included recipes, by name.
    pub library: BTreeMap<String, String>,
    /// `style.json` (palette and material presets) and `style.toml` (the rules), when found.
    pub style_json: Option<String>,
    pub style_toml: Option<String>,
}

/// The SHA-256 of everything a check of `spec` judged: tagged, length-prefixed pieces, so no two
/// different inputs make the same bytes.
pub fn inputs_digest(spec: &ModelSpec, inputs: &CheckInputs) -> Result<Digest, specula_sys::digest::DigestError> {
    let mut b: Vec<u8> = Vec::with_capacity(inputs.recipe.len() + inputs.library.values().map(String::len).sum::<usize>() + 4096);
    let mut piece = |tag: &str, bytes: &[u8]| {
        b.extend_from_slice(tag.as_bytes());
        b.push(0);
        b.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        b.extend_from_slice(bytes);
    };
    piece("specula.records.inputs.v1", b"");
    piece("spec", spec.key().as_bytes());
    piece("recipe", inputs.recipe.as_bytes());
    for (name, text) in &inputs.library {
        piece("include", name.as_bytes());
        piece("text", text.as_bytes());
    }
    piece("style.json", inputs.style_json.as_deref().unwrap_or("\u{0}none").as_bytes());
    piece("style.toml", inputs.style_toml.as_deref().unwrap_or("\u{0}defaults").as_bytes());
    specula_sys::digest::sha256(&b)
}

/// What a submission's references need checked on the caller's thread after the evidence is
/// copied: each record, and for a capture its entry in the arguments' captures.
#[derive(Debug, Default)]
pub struct Plan {
    items: Vec<(usize, Record, Option<usize>)>,
}

/// `review_submit`'s references, on the caller's thread before its evidence is copied: each one
/// names a record of this run, every subject of a check is read again and must digest to what
/// the check judged, and a capture's PNG (and packet) join the submission's captures, so they
/// are copied as evidence like any capture ([`verify`] then digests the copies). Whose records
/// they are is checked on the engine thread, which knows identities.
pub fn prepare(studio: Option<&crate::studio::Shared>, args: &Json) -> Result<(Json, Plan), specula_protocol::ToolOutput> {
    let refs = match args.get("results") {
        Some(Json::Arr(items)) if !items.is_empty() => items.clone(),
        _ => return Ok((args.clone(), Plan::default())),
    };
    let Some(studio) = studio else {
        return Err(refused(WorldError::new("no_records", "this engine keeps no records of results (no asset studio): leave 'results' out")));
    };
    let mut args = args.clone();
    let mut captures: Vec<Json> = match args.get("captures") {
        Some(Json::Arr(c)) => c.clone(),
        _ => Vec::new(),
    };
    let mut plan = Plan::default();
    for (i, item) in refs.iter().enumerate() {
        // Arguments were checked already (`review_tools`): ids and epochs are well formed.
        let r = ResultRef { id: item.get("id").and_then(Json::as_u64).unwrap_or(0), epoch: item.get("epoch").and_then(Json::as_str).unwrap_or("").to_owned() };
        let rec = studio.records().resolve(i, &r).cloned().map_err(refused)?;
        if rec.more_subjects > 0 {
            return Err(refused(WorldError::new("result_unverifiable", format!("results[{i}]: {} #{} checked more subjects than a record keeps ({MAX_SUBJECTS}): check fewer at a time", rec.tool, rec.id)).with("result", rec.id)));
        }
        for s in &rec.subjects {
            let Some(then) = s.inputs else {
                return Err(refused(
                    WorldError::new("result_unverifiable", format!("results[{i}]: {} #{} can't vouch for {} ({}): its inputs couldn't be digested when it ran (a glTF asset, a recipe changed during the check, or no OS digest here)", rec.tool, rec.id, s.name, spec_label(&s.spec)))
                        .with("result", rec.id)
                        .with("subject", spec_label(&s.spec).as_str()),
                ));
            };
            let now = studio.current_inputs(&s.spec).ok().and_then(|inp| inputs_digest(&s.spec, &inp).ok());
            if now != Some(then) {
                let what = if now.is_some() { "its recipe, an include or its style files (style.json, style.toml) changed since the check" } else { "its inputs can't be read now" };
                return Err(refused(
                    WorldError::new("stale_result", format!("results[{i}]: {} #{} checked {} ({}), and {what}: run the check again", rec.tool, rec.id, s.name, spec_label(&s.spec)))
                        .with("result", rec.id)
                        .with("subject", spec_label(&s.spec).as_str()),
                ));
            }
        }
        let mut at = None;
        if let Some(c) = &rec.capture {
            let path = c.png.display().to_string();
            let found = captures.iter().position(|x| x.get("path").and_then(Json::as_str).or(x.as_str()) == Some(path.as_str()));
            let index = match found {
                Some(k) => {
                    // The agent's own entry for the file: the recorded packet is the one kept.
                    let mut j = match &captures[k] {
                        Json::Str(s) => Json::obj([("path", Json::from(s.as_str()))]),
                        other => other.clone(),
                    };
                    match &c.packet {
                        Some((p, _)) => j.set("packet", Json::from(p.display().to_string())),
                        None => j.set("packet", Json::Null),
                    };
                    captures[k] = j;
                    k
                }
                None => {
                    if captures.len() >= crate::review::MAX_CAPTURES {
                        return Err(refused(
                            WorldError::new("too_many_captures", format!("results[{i}]: a submission holds at most {} captures, with the recorded ones", crate::review::MAX_CAPTURES)).with("limit", crate::review::MAX_CAPTURES),
                        ));
                    }
                    let kind = if rec.tool == "studio" { "contact_sheet" } else { "frame" };
                    let mut j = Json::obj([("path", Json::from(path)), ("kind", Json::from(kind)), ("caption", Json::from(format!("{} capture #{}", rec.tool, rec.id)))]);
                    if let Some((p, _)) = &c.packet {
                        j.set("packet", Json::from(p.display().to_string()));
                    }
                    captures.push(j);
                    captures.len() - 1
                }
            };
            at = Some(index);
        }
        plan.items.push((i, rec, at));
    }
    args.set("captures", Json::Arr(captures));
    Ok((args, plan))
}

/// Reads an evidence copy (`evidence/<file>` under `queue`) and digests it.
fn evidence_digest(queue: &Path, reference: &str) -> Option<Digest> {
    let file = reference.strip_prefix("evidence/")?;
    if file.is_empty() || file.contains(['/', '\\']) {
        return None;
    }
    let path = queue.join(crate::evidence::EVIDENCE_DIR).join(file);
    let len = std::fs::metadata(&path).ok()?.len();
    if len > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    specula_sys::digest::sha256(&bytes).ok()
}

/// After the evidence is copied (`prepared`: the arguments with evidence references): every
/// recorded capture's copies must digest to the bytes the engine wrote; then each reference
/// leaves its token for the engine thread.
pub fn verify(studio: Option<&crate::studio::Shared>, session: &str, queue: &Path, prepared: &Json, plan: &Plan) -> Result<(), specula_protocol::ToolOutput> {
    let Some(studio) = studio else { return Ok(()) };
    let tokens = checked_copies(queue, prepared, plan).map_err(refused)?;
    let mut records = studio.records();
    for (id, png, packet) in tokens {
        records.put_token(session, id, png, packet);
    }
    Ok(())
}

/// [`verify`]'s check: each recorded capture's evidence copies against the record's digests;
/// the tokens to leave (record, PNG and packet references).
#[allow(clippy::type_complexity)]
fn checked_copies(queue: &Path, prepared: &Json, plan: &Plan) -> Result<Vec<(u64, Option<String>, Option<String>)>, WorldError> {
    let caps = prepared.get("captures").and_then(Json::as_array).unwrap_or_default();
    let mut tokens = Vec::with_capacity(plan.items.len());
    for (i, rec, at) in &plan.items {
        let (mut png_ref, mut packet_ref) = (None, None);
        if let (Some(c), Some(k)) = (&rec.capture, at) {
            let entry = caps.get(*k);
            let png = entry.and_then(|e| e.get("path").and_then(Json::as_str).or(e.as_str())).unwrap_or("");
            if evidence_digest(queue, png) != Some(c.png_sha) {
                return Err(WorldError::new("stale_result", format!("results[{i}]: capture #{}'s PNG isn't the bytes the engine wrote (it changed on disk since): capture again", rec.id))
                    .with("result", rec.id)
                    .with("file", "png"));
            }
            png_ref = Some(png.to_owned());
            if let Some((_, sha)) = &c.packet {
                let packet = entry.and_then(|e| e.get("packet")).and_then(Json::as_str).unwrap_or("");
                if evidence_digest(queue, packet) != Some(*sha) {
                    return Err(WorldError::new("stale_result", format!("results[{i}]: capture #{}'s packet isn't the one the engine wrote (it changed on disk since): capture again", rec.id))
                        .with("result", rec.id)
                        .with("file", "packet"));
                }
                packet_ref = Some(packet.to_owned());
            }
        }
        tokens.push((rec.id, png_ref, packet_ref));
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(name: &str, passed: bool) -> Subject {
        Subject { name: name.into(), spec: ModelSpec::new(format!("recipes/{name}.json")), inputs: Some([7; 32]), passed, errors: if passed { vec![] } else { vec!["palette".into()] }, failed: usize::from(!passed) }
    }

    #[test]
    fn ids_count_up_and_references_name_this_run_only() {
        let mut r = Records::with_epoch("e1".into());
        let a = r.add(Kind::StyleCheck, "style_check", "s1", vec![subject("a", true), subject("b", false)], None);
        let b = r.add(Kind::StudioCheck, "studio_check", "s1", vec![subject("a", true)], None);
        assert_eq!((a, b), (1, 2));
        let rec = r.resolve(0, &ResultRef { id: 1, epoch: "e1".into() }).unwrap();
        assert_eq!((rec.passed(), rec.failed(), rec.kind), (false, 1, Kind::StyleCheck));
        let p = rec.provenance("e1");
        assert_eq!(p.subjects, vec![("recipes/a.json".to_owned(), true), ("recipes/b.json".to_owned(), false)], "the provenance names every subject");
        assert!(r.resolve(0, &ResultRef { id: 2, epoch: "e1".into() }).unwrap().passed());
        let e = r.resolve(3, &ResultRef { id: 1, epoch: "e0".into() }).unwrap_err();
        assert_eq!(e.code, "stale_result");
        assert!(e.message.starts_with("results[3]"), "{}", e.message);
        assert_eq!(r.resolve(0, &ResultRef { id: 9, epoch: "e1".into() }).unwrap_err().code, "result_not_found");
        assert_eq!(r.resolve(0, &ResultRef { id: 0, epoch: "e1".into() }).unwrap_err().code, "result_not_found");
    }

    #[test]
    fn the_store_is_bounded_oldest_first() {
        let mut r = Records::with_epoch("e".into());
        for _ in 0..MAX_RECORDS + 10 {
            r.add(Kind::Capture, "sight", "s", vec![], None);
        }
        assert_eq!(r.len(), MAX_RECORDS);
        let e = r.resolve(0, &ResultRef { id: 3, epoch: "e".into() }).unwrap_err();
        assert!(e.code == "result_not_found" && e.message.contains("newest"), "{}", e.message);
        assert!(r.get((MAX_RECORDS + 10) as u64).is_some());
        // Big records are bounded by bytes too.
        let many: Vec<Subject> = (0..200).map(|i| subject(&format!("s{i}"), true)).collect();
        let id = r.add(Kind::StyleCheck, "style_check", "s", many, None);
        let rec = r.get(id).unwrap();
        assert_eq!((rec.subjects.len(), rec.more_subjects), (MAX_SUBJECTS, 200 - MAX_SUBJECTS));
        assert!(!rec.passed(), "subjects left out can't back a pass");
        for _ in 0..(MAX_BYTES / 4096) {
            r.add(Kind::StyleCheck, "style_check", "s", (0..MAX_SUBJECTS).map(|i| subject(&format!("x{i}"), true)).collect(), None);
        }
        assert!(r.bytes <= MAX_BYTES, "{} bytes", r.bytes);
    }

    /// Lane review, finding 3: the rules judged (`style.toml`) and the palette (`style.json`) are
    /// part of what a check vouches for, as are the recipe, its includes, params and seed; and the
    /// pieces can't be shifted between each other to keep the digest.
    #[test]
    fn the_inputs_digest_covers_every_file_a_check_judged() {
        let spec = ModelSpec::new("recipes/a.json");
        let base = CheckInputs { recipe: "{r}".into(), library: BTreeMap::from([("inc".to_owned(), "{i}".to_owned())]), style_json: Some("{s}".into()), style_toml: Some("budget = 1".into()) };
        let Ok(d0) = inputs_digest(&spec, &base) else {
            // No OS digest on this platform yet: nothing to check (the code says so to agents).
            return;
        };
        let changed = [
            CheckInputs { style_toml: Some("budget = 2".into()), ..base.clone() },
            CheckInputs { style_toml: None, ..base.clone() },
            CheckInputs { style_json: Some("{t}".into()), ..base.clone() },
            CheckInputs { recipe: "{r }".into(), ..base.clone() },
            CheckInputs { library: BTreeMap::from([("inc".to_owned(), "{j}".to_owned())]), ..base.clone() },
            CheckInputs { library: BTreeMap::new(), ..base.clone() },
            // The same bytes moved from one piece to the next.
            CheckInputs { recipe: "{r}{".into(), style_json: Some("s}".into()), ..base.clone() },
        ];
        for c in &changed {
            assert_ne!(inputs_digest(&spec, c).unwrap(), d0, "{c:?}");
        }
        let mut with_param = spec.clone();
        with_param.set_param("tier", 2.0);
        assert_ne!(inputs_digest(&with_param, &base).unwrap(), d0, "params are part of it");
        assert_eq!(inputs_digest(&spec, &base.clone()).unwrap(), d0, "the same inputs digest the same");
    }

    /// Lane review, findings 1 and 2: a capture's evidence copy is checked by SHA-256 against the
    /// bytes the engine wrote, PNG and packet; another file in its place (a forgery with the same
    /// evidence name, or an edited packet) is refused as `stale_result`.
    #[test]
    fn a_capture_verifies_only_with_the_bytes_the_engine_wrote() {
        let (png, packet) = (b"\x89PNG engine bytes".to_vec(), b"{\"subjects\": []}".to_vec());
        let (Ok(png_sha), Ok(packet_sha)) = (specula_sys::digest::sha256(&png), specula_sys::digest::sha256(&packet)) else {
            // No OS digest on this platform yet: nothing to check (the code says so to agents).
            return;
        };
        let queue = std::env::temp_dir().join(format!("specula-records-{}-{}", std::process::id(), line!()));
        let ev = queue.join(crate::evidence::EVIDENCE_DIR);
        std::fs::create_dir_all(&ev).unwrap();
        let rec = Record {
            id: 5,
            kind: Kind::Capture,
            tool: "sight".into(),
            session: "s".into(),
            subjects: vec![],
            more_subjects: 0,
            capture: Some(CaptureFile { png: "c.png".into(), png_sha, packet: Some(("c.json".into(), packet_sha)) }),
            size: 0,
        };
        let plan = Plan { items: vec![(0, rec, Some(0))] };
        let args = Json::parse(r#"{"captures": [{"path": "evidence/a.png", "packet": "evidence/a.json"}]}"#).unwrap();
        let run = |png_bytes: &[u8], packet_bytes: &[u8]| {
            std::fs::write(ev.join("a.png"), png_bytes).unwrap();
            std::fs::write(ev.join("a.json"), packet_bytes).unwrap();
            checked_copies(&queue, &args, &plan)
        };
        let ok = run(&png, &packet).unwrap();
        assert_eq!(ok, vec![(5, Some("evidence/a.png".to_owned()), Some("evidence/a.json".to_owned()))]);
        let forged = run(b"\x89PNG other bytes", &packet).unwrap_err();
        assert_eq!((forged.code, forged.details.iter().find(|(k, _)| *k == "file").and_then(|(_, v)| v.as_str())), ("stale_result", Some("png")));
        let edited = run(&png, b"{\"subjects\": [], \"FORGED\": 1}").unwrap_err();
        assert_eq!((edited.code, edited.details.iter().find(|(k, _)| *k == "file").and_then(|(_, v)| v.as_str())), ("stale_result", Some("packet")));
        // A reference that isn't an evidence file of this queue is never read.
        let outside = Json::parse(r#"{"captures": [{"path": "evidence/../a.png", "packet": "evidence/a.json"}]}"#).unwrap();
        assert_eq!(checked_copies(&queue, &outside, &plan).unwrap_err().code, "stale_result");
        let _ = std::fs::remove_dir_all(&queue);
    }

    #[test]
    fn tokens_belong_to_one_session_and_record_and_are_used_once() {
        let mut r = Records::with_epoch("e".into());
        r.put_token("ann", 3, Some("evidence/a.png".into()), None);
        assert!(r.take_token("bob", 3).is_none(), "another session's token");
        assert!(r.take_token("ann", 4).is_none(), "another record's");
        assert_eq!(r.take_token("ann", 3).and_then(|t| t.png_ref).as_deref(), Some("evidence/a.png"));
        assert!(r.take_token("ann", 3).is_none(), "used once");
        for i in 0..(MAX_TOKENS as u64 + 10) {
            r.put_token("ann", i, None, None);
        }
        assert!(r.tokens.len() <= MAX_TOKENS);
    }
}

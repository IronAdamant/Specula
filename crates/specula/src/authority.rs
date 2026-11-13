//! Who may do what: one authority function, the reserved roles and writer names, and the
//! refusal sites (see `docs/design/AUTHORITY.md`).
//!
//! **Authority is never self-declared.** `agent_hello {role}` names a job ("builder",
//! "playtester", "artist"); the roles that carry authority ([`ROLE_OWNER`], [`ROLE_LEAD`],
//! [`ROLE_DIRECTOR`], [`ROLE_REVIEWER`]) are admitted only when the operator granted them
//! ([`Grants`]: `--owner NAME`, `--lead NAME`, `--reviewer NAME`), or through the one documented
//! fallback (the first `lead` hello when the operator named no lead). The human at the window
//! is the owner ([`WINDOW_OWNER`]). A granted name is bound to the identity that first takes it
//! (its resume token carries it), so another session can't take it over later.
//!
//! This is the groundwork for M6's per-agent tokens: then a grant names a token instead of a name,
//! and [`authority`] stays the one place that answers.
//!
//! The role literals live only here (`crates/specula/tests/authority_guard.rs` fails otherwise).

use crate::world::WorldError;
use specula_core::Json;
use std::collections::BTreeMap;

/// The owner: the human, at the window or granted by the operator. Never self-declared.
pub const ROLE_OWNER: &str = "owner";
/// The lead agent (the controller that deploys a swarm): frame floor, streaming budgets.
pub const ROLE_LEAD: &str = "lead";
/// Another word for the lead (task_plan's director): the same authority, the same grant.
pub const ROLE_DIRECTOR: &str = "director";
/// Decides review submissions and puts work on the owner's screen.
pub const ROLE_REVIEWER: &str = "reviewer";
/// The window's human, in attribution (the review screen's decisions, critiques).
pub const WINDOW_OWNER: &str = "window:owner";

/// What a caller may do, lowest first: each level includes the ones below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Authority {
    /// Any agent: builds, claims, submits, comments on its own work.
    #[default]
    Agent,
    /// Decides other agents' submissions, puts work on the owner's screen, closes critiques.
    Reviewer,
    /// Shared settings that shape everyone's run (frame floor, streaming budgets); a reviewer too.
    Lead,
    /// The human: everything, including what a lead or reviewer made.
    Owner,
}

impl Authority {
    pub const ALL: [Authority; 4] = [Authority::Agent, Authority::Reviewer, Authority::Lead, Authority::Owner];

    pub fn name(self) -> &'static str {
        match self {
            Authority::Agent => "agent",
            Authority::Reviewer => ROLE_REVIEWER,
            Authority::Lead => ROLE_LEAD,
            Authority::Owner => ROLE_OWNER,
        }
    }

    pub fn parse(s: &str) -> Option<Authority> {
        Authority::ALL.into_iter().find(|a| a.name() == s)
    }

    /// The authority a hello's role (or a critique's source) asks for (`None`: a job label, no
    /// authority). Compared by [`skeleton`]: `Lead`, `LEAD` or a Cyrillic look-alike ask too.
    pub fn of_role(role: &str) -> Option<Authority> {
        let sk = skeleton(role);
        if sk == skeleton(WINDOW_OWNER) {
            return Some(Authority::Owner);
        }
        match sk.as_str() {
            ROLE_OWNER => Some(Authority::Owner),
            ROLE_LEAD | ROLE_DIRECTOR => Some(Authority::Lead),
            ROLE_REVIEWER => Some(Authority::Reviewer),
            _ => None,
        }
    }

    /// The roles a caller of this authority may have said (the words in error messages).
    fn roles(self) -> &'static str {
        match self {
            Authority::Agent => "any role",
            Authority::Reviewer => "role reviewer, lead, director or owner",
            Authority::Lead => "role lead or director (or owner)",
            Authority::Owner => "the owner (the window, or --owner NAME)",
        }
    }
}

/// Where a refusal came from: one stable code per site, so tracing can hook every refusal at
/// [`refusal`] and agents can branch on the code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// agent_hello asked for a role the operator didn't grant.
    HelloRole,
    /// agent_hello took a name the operator granted to another identity.
    HelloGrantBound,
    /// agent_hello took a name an engine writer uses.
    HelloReservedName,
    /// swarm_settings frame_floor_fps.
    FrameFloor,
    /// stream_settings with changes.
    StreamSettings,
    /// review_decide.
    ReviewDecide,
    /// review_screen open/close/next/prev.
    ReviewScreen,
    /// critique_update by someone who may not change that critique.
    CritiqueUpdate,
    /// critique_add claiming a source only the owner may claim.
    CritiqueSource,
    /// A job that runs one at a time is busy (another agent's).
    JobBusy,
    /// Cancelling or replacing a job that isn't yours.
    JobNotYours,
    /// perf_zones switching the profiler while a capture records.
    ProfilerBusy,
    /// tools_scope: setting another agent's tool list.
    ToolsScope,
    /// agent_hello: a name with invisible (format) characters or edge spaces.
    HelloBadName,
    /// `force` on another agent's object (a note, a save, a playtest, journal entries).
    ForceOverride,
    /// world_save under another name, or over another world.
    WorldSave,
    /// disk_prune {free_mb}: deleting past what the budget needs.
    DiskPrune,
    /// gi_settings with changes (the GI tier shapes the human's window too).
    GiSettings,
    /// editor_input: simulated input acts as the window's human.
    EditorInput,
    /// gi_bake `threads` past the default (they slow the frame thread and other agents).
    GiThreads,
    /// audio_settings with changes (bus volumes and ducking are what everyone hears).
    AudioSettings,
    /// text_edit window_focus: pointing the human's keyboard (and clipboard) at a text entity.
    WindowFocus,
    /// security_incidents and security_clear: the input monitor's log and clearing it (lead).
    Security,
    /// security_clear of an incident the owner chose to keep frozen.
    SecurityKept,
    /// security_allow_lead: the owner's alone.
    SecurityAllow,
    /// Putting an engine screen (gallery) on the window: owner, lead or reviewer (input sweep R1).
    WindowScreen,
    /// ui_act on an engine screen (its buttons decide as the window's owner).
    ScreenAct,
    /// logic_input overriding an action the human is using at the window.
    HumanInput,
    /// Marking string-table entries reviewed (l10n_set review, or a table written with them).
    L10nReview,
    /// logic_build, logic_reload, logic_unload: native module code runs with the owner's user
    /// rights, so only the owner or an operator-granted lead loads it (docs/design/MODULES.md).
    NativeCode,
}

impl Site {
    /// The stable error code agents and tracing see.
    pub fn code(self) -> &'static str {
        match self {
            Site::HelloRole => "role_not_granted",
            Site::HelloGrantBound => "grant_bound",
            Site::HelloReservedName => "name_reserved",
            Site::FrameFloor | Site::StreamSettings | Site::ToolsScope | Site::DiskPrune | Site::WorldSave | Site::GiSettings | Site::GiThreads | Site::AudioSettings | Site::WindowFocus | Site::Security | Site::NativeCode => "lead_only",
            Site::SecurityKept => "owner_kept",
            Site::WindowScreen => "not_allowed",
            Site::HumanInput => "human_input",
            Site::HelloBadName => "name_invalid",
            Site::ForceOverride => "force_needs_lead",
            Site::ReviewDecide | Site::ReviewScreen | Site::L10nReview => "not_reviewer",
            Site::EditorInput | Site::SecurityAllow | Site::ScreenAct => "owner_only",
            Site::CritiqueUpdate => "not_yours",
            Site::CritiqueSource => "source_reserved",
            Site::JobBusy => "job_busy",
            Site::JobNotYours => "not_job_owner",
            Site::ProfilerBusy => "capture_in_progress",
        }
    }

    /// The call site's name (the `refused_at` field).
    pub fn name(self) -> &'static str {
        match self {
            Site::HelloRole => "agent_hello.role",
            Site::HelloGrantBound => "agent_hello.grant",
            Site::HelloReservedName => "agent_hello.name",
            Site::FrameFloor => "swarm_settings.frame_floor_fps",
            Site::StreamSettings => "stream_settings",
            Site::ReviewDecide => "review_decide",
            Site::ReviewScreen => "review_screen",
            Site::CritiqueUpdate => "critique_update",
            Site::CritiqueSource => "critique_add.source",
            Site::JobBusy => "jobs.start",
            Site::JobNotYours => "jobs.cancel",
            Site::ProfilerBusy => "perf_zones.enabled",
            Site::ToolsScope => "tools_scope",
            Site::HelloBadName => "agent_hello.name",
            Site::ForceOverride => "force",
            Site::WorldSave => "world_save.name",
            Site::DiskPrune => "disk_prune.free_mb",
            Site::GiSettings => "gi_settings",
            Site::GiThreads => "gi_bake.threads",
            Site::EditorInput => "editor_input",
            Site::AudioSettings => "audio_settings",
            Site::WindowFocus => "text_edit.window_focus",
            Site::Security => "security",
            Site::SecurityKept => "security_clear.kept",
            Site::SecurityAllow => "security_allow_lead",
            Site::WindowScreen => "window.screen",
            Site::ScreenAct => "ui_act.surface",
            Site::HumanInput => "logic_input.override",
            Site::L10nReview => "l10n.review",
            Site::NativeCode => "logic.native_code",
        }
    }
}

/// The one way an authority or ownership refusal is made: each is a trace event
/// (`authority.refused`, with its code and call site; docs/design/TRACE.md).
pub fn refusal(site: Site, message: impl Into<String>) -> WorldError {
    specula_core::trace_event!(&crate::trace_sink::WRITES, Debug, "authority.refused", code = site.code(), at = site.name());
    WorldError::new(site.code(), message).with("refused_at", site.name())
}

/// `have` must be at least `need` for `what` (refused at `site` otherwise).
pub fn require(have: Authority, need: Authority, site: Site, what: &str) -> Result<(), WorldError> {
    if have >= need {
        return Ok(());
    }
    Err(refusal(site, format!("{what} needs {} (you have {} authority; see agent_hello's reply)", need.roles(), have.name()))
        .with("authority", have.name())
        .with("needs", need.name()))
}

/// `force` on another agent's object: the object's own owner may always force their own; anyone
/// else needs lead authority (owner, 2026-10-29). `what` names the override in the refusal.
pub fn may_force(have: Authority, own: bool, what: &str) -> Result<(), WorldError> {
    if own || have >= Authority::Lead {
        return Ok(());
    }
    Err(refusal(Site::ForceOverride, format!("{what}: force on another agent's object needs lead authority (you have {}); ask its owner, or the lead", have.name()))
        .with("authority", have.name())
        .with("needs", Authority::Lead.name()))
}

/// Native module code (`logic_build`, `logic_reload`, `logic_unload`): a module is a library
/// the engine loads into its own process, running with the owner's user rights, so it needs the
/// owner or a lead **the operator granted** (`--lead NAME`, `--owner NAME`). `granted` is whether
/// the caller's identity holds an operator grant: asked for apart from `have`, so that no other
/// way to lead authority (there is none today) would ever load native code.
pub fn may_load_native(have: Authority, granted: bool, what: &str) -> Result<(), WorldError> {
    if have == Authority::Owner || (have >= Authority::Lead && granted) {
        return Ok(());
    }
    let why = if have >= Authority::Lead {
        "your lead authority isn't an operator grant: start the engine with --lead NAME to load native code"
    } else {
        "a module is native code that runs with the owner's user rights"
    };
    Err(refusal(Site::NativeCode, format!("{what} needs the owner or an operator-granted lead (--lead NAME): {why}"))
        .with("authority", have.name())
        .with("needs", Authority::Lead.name())
        .with("granted", granted))
}

/// Characters that show nothing (Unicode format characters and the like): dropped from
/// skeletons of free text (critique sources, roles).
fn invisible(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x034F | 0x061C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F | 0x200B..=0x200F | 0x202A..=0x202E
        | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFF8 | 0x1D173..=0x1D17A | 0xE0000..=0xE0FFF)
}

/// Text as it looks: lower case, invisible characters dropped, and the common Cyrillic, Greek
/// and full-width look-alikes of Latin letters mapped to them. Agent names (ASCII only, see
/// [`name_problem`]) compare by it case-blind (`Physics`, `DISK`); free text compared against
/// a role word (a critique's source, a hello's role) by the look-alike mapping too.
pub fn skeleton(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if invisible(c) {
            continue;
        }
        let c = match c {
            // Full-width ASCII.
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFF01 + 0x21).unwrap_or(c),
            _ => c,
        };
        let mapped = match c {
            'а' | 'А' | 'α' | 'Α' => 'a',
            'в' | 'В' | 'β' | 'Β' => 'b',
            'с' | 'С' | 'ϲ' => 'c',
            'ԁ' => 'd',
            'е' | 'Е' | 'ε' | 'Ε' | 'ё' => 'e',
            'һ' | 'Н' | 'Η' => 'h',
            'і' | 'І' | 'ι' | 'Ι' | 'ӏ' | 'Ӏ' | 'ı' => 'i',
            'ј' | 'Ј' => 'j',
            'к' | 'К' | 'κ' | 'Κ' => 'k',
            'м' | 'М' | 'Μ' => 'm',
            'п' | 'ո' => 'n',
            'Ν' | 'ν' => 'v',
            'о' | 'О' | 'ο' | 'Ο' | 'օ' => 'o',
            'р' | 'Р' | 'ρ' | 'Ρ' => 'p',
            'ԛ' => 'q',
            'г' => 'r',
            'ѕ' | 'Ѕ' => 's',
            'т' | 'Т' | 'τ' | 'Τ' => 't',
            'υ' | 'ս' => 'u',
            'ѵ' => 'v',
            'ԝ' | 'ա' => 'w',
            'х' | 'Х' | 'χ' | 'Χ' => 'x',
            'у' | 'У' | 'γ' | 'Υ' => 'y',
            'Ζ' => 'z',
            other => other,
        };
        out.extend(mapped.to_lowercase());
    }
    out
}

/// The characters an agent's name may hold: ASCII letters and digits, space, `_`, `.` and `-`.
/// A small, documented class (coordinator, 2026-10-29): no look-alike letters, colons, marks or
/// invisible characters can reach a name, so names compare as they read (case aside: see
/// [`skeleton`]), and no name can spell a writer prefix such as `module:`.
pub fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '.' | '-')
}

/// Why `name` can't be an agent's name at all (`None`: it can): 1..64 characters of
/// [`name_char`], no space at either end.
pub fn name_problem(name: &str) -> Option<WorldError> {
    let ok = !name.is_empty() && name.chars().count() <= 64 && name.trim() == name && name.chars().all(name_char);
    if !ok {
        return Some(
            refusal(Site::HelloBadName, "an agent's name is 1..64 characters of ASCII letters, digits, space, _ . and -, with no space at either end (names compare as they read)")
                .with("name", name.escape_default().to_string()),
        );
    }
    None
}

/// The galleries lane's taste writer (its pick-driven tuning), reserved ahead of its merge.
pub const TASTE_AGENT: &str = "taste";

/// Names agents may not take: the engine's own writers (each [`crate::write_policy::Exempt`]
/// variant's name, the disk, the window, the backstop) and the prefixes of in-process writers.
pub const RESERVED_PREFIXES: [&str; 4] = ["module:", "lex:", "engine:", "window:"];

/// The role words: no agent may be named one unless the operator granted that name.
pub const ROLE_WORDS: [&str; 4] = [ROLE_OWNER, ROLE_LEAD, ROLE_DIRECTOR, ROLE_REVIEWER];

/// Whether `name` is (or looks like) an engine writer's: agents can't say agent_hello with it.
pub fn reserved_writer(name: &str) -> bool {
    const NAMES: [&str; 6] = [crate::physics::AGENT, crate::nav::AGENT, crate::anim::RELOAD_AGENT, crate::project::DISK_AGENT, crate::write_policy::STRAY_AGENT, TASTE_AGENT];
    let sk = skeleton(name.trim());
    NAMES.iter().chain([&WINDOW_OWNER]).any(|n| skeleton(n) == sk) || RESERVED_PREFIXES.iter().any(|p| sk.starts_with(p))
}

/// The authority roles the operator granted, and who holds them. Names are keyed by their
/// [`skeleton`], so a look-alike of a granted name is that name.
#[derive(Clone, Debug, Default)]
pub struct Grants {
    /// Skeleton of an agent name → (the name as given, the authority the operator granted it).
    operator: BTreeMap<String, (String, Authority)>,
    /// Skeleton of a granted name → the identity key that took it first (it keeps it; resumes
    /// carry the key).
    bound: BTreeMap<String, String>,
}

/// Operator grants at most (flags are typed by a human; this bounds a script's mistake).
pub const MAX_GRANTS: usize = 64;

impl Grants {
    /// Grants `name` the authority `a` (an operator flag). The highest grant of a name wins.
    pub fn grant(&mut self, name: &str, a: Authority) -> Result<(), String> {
        if name.is_empty() || name.chars().count() > 64 || name_problem(name).is_some() {
            return Err(format!("a granted name must be 1..64 visible characters without edge spaces, not {name:?}"));
        }
        if reserved_writer(name) {
            return Err(format!("'{name}' is an engine writer's name; grant an agent's name"));
        }
        let sk = skeleton(name);
        if self.operator.len() >= MAX_GRANTS && !self.operator.contains_key(&sk) {
            return Err(format!("at most {MAX_GRANTS} granted names"));
        }
        let e = self.operator.entry(sk).or_insert_with(|| (name.to_owned(), Authority::Agent));
        e.1 = e.1.max(a);
        Ok(())
    }

    /// Whether the operator granted anyone lead (or owner) authority: without one, nobody in
    /// this run can force over claims, set tool scopes or clear security incidents.
    pub fn lead_granted(&self) -> bool {
        self.operator.values().any(|(_, a)| *a >= Authority::Lead)
    }

    /// Whether `name` (or a look-alike) is granted and bound to an identity other than `key`.
    pub fn bound_elsewhere(&self, name: &str, key: &str) -> bool {
        self.bound.get(&skeleton(name)).is_some_and(|k| k != key)
    }

    /// Whether the identity `key` holds a grant (never forgotten).
    pub fn holds(&self, key: &str) -> bool {
        self.granted_key(key)
    }

    /// Whether the identity `key` holds a name the operator granted.
    pub fn granted_key(&self, key: &str) -> bool {
        self.bound.values().any(|k| k == key)
    }

    /// Whether `name` (or a look-alike) is a role word the operator didn't grant as a name.
    pub fn role_word(&self, name: &str) -> bool {
        let sk = skeleton(name);
        ROLE_WORDS.iter().any(|w| skeleton(w) == sk) && !self.operator.contains_key(&sk)
    }

    /// The authority `name` (identity `key`) gets by saying `role`, or why it can't say it.
    /// Only a successful admission binds a granted name to `key`.
    pub fn admit(&mut self, name: &str, key: &str, role: &str) -> Result<Authority, WorldError> {
        if self.bound_elsewhere(name, key) {
            return Err(refusal(
                Site::HelloGrantBound,
                format!("the operator granted '{name}' a role, and another identity holds that name for this run: resume it with its resume_token, or pick another name"),
            )
            .with("name", name));
        }
        let sk = skeleton(name);
        let granted = self.operator.get(&sk).map(|g| g.1);
        let out = self.admit_role(name, key, role, granted)?;
        if granted.is_some() {
            self.bound.insert(sk, key.to_owned());
        }
        Ok(out)
    }

    fn admit_role(&mut self, name: &str, key: &str, role: &str, granted: Option<Authority>) -> Result<Authority, WorldError> {
        let Some(asked) = Authority::of_role(role) else { return Ok(Authority::Agent) };
        if granted.is_some_and(|g| g >= asked) {
            return Ok(asked);
        }
        // Nobody becomes lead by asking (owner, 2026-11-12: "if people or AI agents start with
        // no --lead, then it shouldn't have lead powers"). Until then the first agent to say
        // lead held it when the operator had granted nothing: a role an agent gave itself.
        let _ = key;
        let how = match asked {
            Authority::Owner => "the owner is the human: at the window, or granted with --owner NAME when the engine starts",
            Authority::Lead => "the operator names the lead when the engine starts (--lead NAME, repeatable). Without one this run has no lead: agents still build (claims expire on their own, and agents choose their own tools), but nobody can force over another agent's claims, set tool scopes or clear security incidents until the operator restarts with --lead NAME",
            Authority::Reviewer => "reviewers are granted by the operator (--reviewer NAME, or --lead NAME: a lead reviews too)",
            Authority::Agent => "",
        };
        Err(refusal(Site::HelloRole, format!("role '{role}' carries {} authority, which nobody grants themselves: {how}; say a role like builder", asked.name()))
            .with("role", role)
            .with("name", name))
    }

    /// For engine_info: who holds what, and how.
    pub fn json(&self) -> Json {
        let mut f: Vec<(&str, Json)> = vec![(
            "operator",
            Json::obj(self.operator.iter().map(|(sk, (n, a))| (n.clone(), Json::obj([("authority", Json::from(a.name())), ("bound", Json::from(self.bound.contains_key(sk)))])))),
        )];
        f.push(("lead_granted", Json::from(self.lead_granted())));
        Json::obj(f)
    }
}

/// The one authority function: what the session `session_id` may do.
pub fn authority(swarm: &crate::swarm::Swarm, session_id: &str) -> Authority {
    swarm.authority_of(session_id)
}

/// Sessions' authority as the engine thread last admitted it, for tools that run on the
/// caller's connection thread (no `&Swarm` there: `trace_*`, `taste_*`, the procgen and studio
/// tools): the same answer as [`authority`], published by the swarm on every hello, resume and
/// prune. At most [`MAX_PUBLISHED`] sessions (the oldest go first).
static PUBLISHED: std::sync::Mutex<BTreeMap<String, (u64, Authority, bool)>> = std::sync::Mutex::new(BTreeMap::new());
static PUBLISH_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Sessions [`of_session`] remembers.
pub const MAX_PUBLISHED: usize = 4096;

/// What session `session_id` may do, read from the caller thread (see [`PUBLISHED`]):
/// `Agent` for a session that never said hello (or was forgotten).
pub fn of_session(session_id: &str) -> Authority {
    PUBLISHED.lock().unwrap_or_else(|p| p.into_inner()).get(session_id).map_or(Authority::Agent, |(_, a, _)| *a)
}

/// The authority the operator's grants give: `a`, except that a lead nobody granted
/// (`granted` false) counts as a plain agent.
///
/// Lead authority comes only from `--lead NAME` (there is no first-come lead any more: owner,
/// 2026-11-12), so today this changes nothing. It stays as the second lock on what matters
/// most: the checks on the human's input (pointing the keyboard, putting screens on the window,
/// holding their actions), native code and the security monitor read [`trusted`] /
/// [`trusted_of_session`], which ask for the grant itself. When a self-declared lead existed,
/// it escaped the monitor's freezes by saying "lead" (review R2 of m5-inputguard, blocker 1).
pub fn trust(a: Authority, granted: bool) -> Authority {
    if a == Authority::Lead && !granted {
        Authority::Agent
    } else {
        a
    }
}

/// [`authority`] as the operator's grants alone give it ([`trust`]), on the engine thread.
pub fn trusted(swarm: &crate::swarm::Swarm, session_id: &str) -> Authority {
    swarm.trusted_authority_of(session_id)
}

/// [`of_session`] as the operator's grants alone give it ([`trust`]), on a caller thread.
pub fn trusted_of_session(session_id: &str) -> Authority {
    PUBLISHED.lock().unwrap_or_else(|p| p.into_inner()).get(session_id).map_or(Authority::Agent, |(_, a, g)| trust(*a, *g))
}

/// Tests publish a session's authority as an operator grant gives it.
#[cfg(test)]
pub(crate) fn publish(session_id: &str, a: Authority) {
    publish_as(session_id, a, true);
}

/// The swarm publishes a session's admitted authority (hello, resume), and whether its identity
/// holds an operator grant (`granted`).
pub(crate) fn publish_as(session_id: &str, a: Authority, granted: bool) {
    let seq = PUBLISH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut m = PUBLISHED.lock().unwrap_or_else(|p| p.into_inner());
    m.insert(session_id.to_owned(), (seq, a, granted));
    while m.len() > MAX_PUBLISHED {
        let Some(oldest) = m.iter().min_by_key(|(_, (s, _, _))| *s).map(|(k, _)| k.clone()) else { break };
        m.remove(&oldest);
    }
}

/// The swarm forgets a session (a resume moved its identity on, or it was pruned).
pub(crate) fn unpublish(session_id: &str) {
    PUBLISHED.lock().unwrap_or_else(|p| p.into_inner()).remove(session_id);
}

/// The authority of an attributed name in stored records (the window is the owner; agents'
/// stored authority is recorded with what they made).
pub fn of_writer_name(name: &str) -> Option<Authority> {
    (name == WINDOW_OWNER).then_some(Authority::Owner)
}

/// One tool call, one commit: the caller's journal entries per tool call (every request the
/// call sent counts toward it, sessions included). A call that commits more than once can fail
/// halfway, its first commits standing (the terrain_edit class); the dispatcher logs, counts and
/// debug-asserts it. Commands that commit once per independent item say so (`independent`).
#[derive(Default)]
pub struct CallCommits {
    /// Call id → commits so far (the newest [`MAX_CALLS_COUNTED`] calls).
    calls: BTreeMap<u64, u64>,
}

/// Tool calls whose commits are remembered.
pub const MAX_CALLS_COUNTED: usize = 512;

impl CallCommits {
    /// One request of call `call` (0: not from a tool call) made `made` commits: the call's total
    /// when that is more than one (a breach), else `None`.
    pub fn count(&mut self, call: u64, made: u64, independent: bool) -> Option<u64> {
        if made == 0 || independent {
            return None;
        }
        let total = if call == 0 {
            made
        } else {
            let n = self.calls.entry(call).or_default();
            *n += made;
            let n = *n;
            while self.calls.len() > MAX_CALLS_COUNTED {
                self.calls.pop_first();
            }
            n
        };
        (total > 1).then_some(total)
    }
}

/// A shared store whose items have owners: the store checks the caller on every change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    /// Claims: only the holder renews, shares or releases.
    Claims,
    /// The task board: the poster edits a task, the assignee works it (`not_poster`, `not_task_owner`).
    Board,
    /// Notes: the author deletes (others with `force`, which is answered as such).
    Notes,
    /// Intents: the declarer clears.
    Intents,
    /// Leases: the holder renews and releases.
    Leases,
    /// Named cameras: each agent's own names (another's are read with `of`).
    Cameras,
    /// Review submissions: the submitter withdraws; deciding is [`Access::Reviewer`].
    Review,
    /// Critiques: [`crate::review::Review::check_critique`].
    Critiques,
    /// A job that runs one at a time ([`crate::jobs::Slot`]): busy is refused naming the owner.
    Jobs,
    /// Save games: another agent's is replaced or deleted only with `force`.
    Saves,
    /// Playtests: only the starter cancels (others with `force`, logged on the board).
    Playtests,
    /// `lex_simulate`'s saved runs: only the session that saved one deletes it.
    LexRuns,
    /// Binary uploads in progress: only the uploader continues, aborts or commits one.
    Uploads,
    /// Trace incidents: resolved or reopened by an agent the incident names, a reviewer or the
    /// lead; anyone acks, notes or links.
    Incidents,
    /// Galleries: the maker opens, closes, replaces a round of and deletes its own; the owner, a
    /// lead or a reviewer any (`crate::taste::taste_authority`).
    Galleries,
    /// Timeline playbacks: their starter pauses, scrubs and stops them; another agent (or the
    /// game's own) with `force` and lead authority (docs/design/TIMELINE.md).
    Timelines,
}

/// What a tool may do, declared once per tool ([`access`]): the authority table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Reads only (or writes only its own new files, such as captures).
    ReadOnly,
    /// Writes the world through the write policy with the named rule (`world`, `joint`, `nav`,
    /// `lex`, `lex_live`, `project`, `vfx`: `write_policy::Rule`'s variants, named here because
    /// only their owners may spell them): claims as the caller.
    World(&'static str),
    /// Replaces the whole world: `may_replace` (no playtest, no other agent's claim).
    ReplacesWorld,
    /// Changes items that have owners; the store checks the caller.
    Owned(Store),
    /// Changes need lead authority (reads are open).
    Lead,
    /// Changes need reviewer authority.
    Reviewer,
    /// Changes need the owner.
    Owner,
    /// Anyone may, and why that's right.
    Open(&'static str),
}

/// The authority table: every engine tool's [`Access`] (`None`: not an engine tool). A hosted
/// game's tools are [`Access::World`] (`world`): they run through `Live::game_tool` as the
/// caller.
pub fn access(tool: &str) -> Option<Access> {
    use Access::*;
    Some(match tool {
        // Reads.
        "world_list" | "world_query" | "world_hash" | "world_changes" | "world_history" | "world_rollup" | "claim_list" | "camera_list" | "sight_frame" | "sight_orbit"
        | "sight_compare" | "sight_query" | "tilemap_get" | "task_get" | "task_list" | "task_progress" | "note_list" | "intent_list" | "lease_list" | "desync_trace"
        | "desync_compare" | "lex_schema" | "lex_list" | "lex_get" | "lex_validate" | "lex_export" | "nav_path" | "nav_point" | "nav_info" | "ai_check" | "ai_status"
        | "anim_get" | "anim_state" | "anim_check" | "anim_filmstrip" | "physics_joints" | "ragdoll_info" | "physics_raycast" | "physics_cast" | "physics_overlap"
        | "physics_contacts" | "physics_events" | "physics_info" | "physics_explain" | "physics_check" | "stream_state" | "save_list" | "project_list"
        | "project_read" | "studio_stage" | "studio_sheet" | "studio_lineup" | "studio_compare" | "studio_check" | "style_check" | "review_list" | "critique_list"
        | "merge_suggest" | "vfx_get" | "vfx_check" | "vfx_info" | "logic_status" | "perf_frame" | "perf_memory" | "engine_info" | "balance_targets"
        | "disk_status" | "tools_catalog" | "trace_query" | "trace_follow" | "trace_call" | "trace_crash" | "trace_incidents" | "terrain_info" | "gi_info"
        | "gi_check" | "editor_state" => ReadOnly,
        // The interface's sight tools: they lay it out (and put the player's interface back) and read it.
        "ui_tree" | "ui_check" | "ui_frame" => ReadOnly,
        // A player's input, synthesised: the frames' world changes commit as the caller's through
        // Live::game_tool, like a hosted game's tool (claims checked). The engine's own screens
        // (surface review or gallery) are the owner's: refused (owner_only) for anyone else.
        "ui_act" => World("world"),
        // Localisation (docs/design/LOCALISATION.md): string tables are project files, written
        // as the caller through the project store; marking entries reviewed needs reviewer
        // authority (Site::L10nReview, also when a table is written with project_*).
        "l10n_get" | "l10n_check" => ReadOnly,
        "l10n_set" => World("project"),
        // Posts one plan of board tasks (task_plan's store rules).
        "l10n_tasks" => Owned(Store::Board),
        // Hearing (docs/design/AUDIO.md): what played and how loud, from the engine's audio.
        "audio_played" | "audio_levels" => ReadOnly,
        "audio_preview" => Open("renders or decodes a sound offline and measures it; its optional WAV and picture are new files in the captures folder; nothing shared changes"),
        // The world, through the write policy.
        "world_spawn" | "world_set" | "world_delete" | "world_batch" | "world_env" | "world_undo" | "world_place" | "tilemap_set" | "text_edit" | "nav_agent" | "ai_set"
        | "model_collider" | "anim_set" | "ragdoll_set" | "physics_impulse" | "procgen_terrain" | "procgen_scatter" => World("world"),
        "camera_set" | "camera_delete" => Owned(Store::Cameras),
        "physics_joint" => World("joint"),
        "nav_build" => World("nav"),
        "lex_edit" | "lex_import" => World("lex"),
        "lex_live" => World("lex_live"),
        "project_write" | "project_patch" | "project_delete" => World("project"),
        // Its files land under /project/<name>: the caller's claims are checked there first.
        "project_mount" => World("project"),
        "vfx_set" => World("vfx"),
        // Timelines (docs/design/TIMELINE.md): files through the project store; playback writes
        // its tracks as the caller under the timeline rule (claims hold; a refused tick pauses it
        // as blocked); a playback is its starter's to pause, scrub or stop.
        "timeline_get" | "timeline_validate" | "timeline_preview" | "timeline_check" => ReadOnly,
        "timeline_set" => World("project"),
        // A scrub writes the state at t as the caller (and makes a paused playback when none
        // plays); another agent's playback needs force and the lead (Store::Timelines' rule).
        "timeline_play" | "timeline_scrub" => World("timeline"),
        "timeline_pause" | "timeline_stop" => Owned(Store::Timelines),
        // A bake writes /gi/<volume> as the caller (claims checked when it starts and when its
        // result lands); cancelling another agent's bake needs force, and force the lead
        // (authority::may_force); threads past the default are the lead's (Site::GiThreads).
        "gi_bake" => World("gi"),
        // A terrain recipe's strokes (and its collider, in the same journal entry) through the
        // project store, as the caller.
        "terrain_edit" => World("project"),
        "model_paint" | "model_path" | "model_export" => World("project"),
        // Their `force` discards every agent's unsaved changes: the lead's (force_needs_lead).
        "world_load" | "save_read" => ReplacesWorld,
        // Owned stores.
        "claim_take" | "claim_renew" | "claim_share" | "claim_release" => Owned(Store::Claims),
        "task_post" | "task_take" | "task_update" | "task_done" | "task_plan" => Owned(Store::Board),
        "note_add" | "note_delete" => Owned(Store::Notes),
        "intent_declare" | "intent_clear" => Owned(Store::Intents),
        "lease_take" | "lease_renew" | "lease_release" => Owned(Store::Leases),
        "review_submit" => Owned(Store::Review),
        "critique_add" | "critique_update" => Owned(Store::Critiques),
        "perf_capture" | "stream_fly" | "vfx_preview" => Owned(Store::Jobs),
        "save_write" | "save_delete" => Owned(Store::Saves),
        // playtest_report {cancel} stops a playtest: its starter only (others with force, logged).
        "playtest_run" | "playtest_report" => Owned(Store::Playtests),
        // world_undo is World: undoing others' entries or over their later changes needs force,
        // and force on another agent's object needs the lead (authority::may_force).
        // Native code in the engine's process, with the owner's user rights: the owner or an
        // operator-granted lead only (may_load_native). A
        // builder's logic_build files a proposal the lead approves (docs/design/MODULES.md).
        "logic_build" | "logic_reload" | "logic_unload" => Lead,
        "logic_hook" => Open("calls a hook of a module the owner or a granted lead loaded; its changes are checked against other agents' claims as the caller's, and every use is logged with its reason"),
        "lex_session" => Owned(Store::LexRuns),
        "project_upload" => Owned(Store::Uploads),
        "tools_request" => Owned(Store::Board),
        "trace_incident" => Owned(Store::Incidents),
        // Galleries (docs/design/GALLERIES.md): taste state through the taste rule; picks are the
        // owner's; results reach the game's files with the gallery that records them, in one commit
        // under the taste result rule.
        "gallery_get" | "gallery_list" | "gallery_wait" | "taste_query" | "board_check" => ReadOnly,
        "gallery_create" | "board_measure" => World("taste"),
        "gallery_open" | "gallery_close" | "gallery_round" | "gallery_delete" => Owned(Store::Galleries),
        "gallery_decide" => Owner,
        "gallery_apply" => World("taste_result"),
        "taste_suggest" => Open("lists suggestions and files them for the owner (submit puts one on the review queue, which only the owner decides); apply needs the owner and dismiss a lead or reviewer"),
        "style_onboard" => Open("starts the style configurator, whose galleries the owner picks from; its result reaches style.toml only when the owner approves it on the review queue; cancelling is its starter's, the owner's, a lead's or a reviewer's"),
        // Authority.
        // trace_settings: engine-wide changes are the lead's; scope "session" is the caller's own view.
        "swarm_settings" | "stream_settings" | "tools_scope" | "trace_settings" | "gi_settings" => Lead,
        // Bus volumes and duck rules change what every agent and the human hear: the lead's to
        // change (Site::AudioSettings); anyone reads them.
        "audio_settings" => Lead,
        "review_decide" | "review_screen" => Reviewer,
        // Simulated window input: the editor's edits are the window's human's (window:owner), so
        // only the owner may drive it (docs/design/EDITOR.md, "Agents driving the editor").
        "editor_input" => Owner,
        // The input monitor (docs/design/SECURITY.md): its log and clearing an incident are the
        // lead's and the owner's; allowing a lead to point the keyboard is the owner's alone.
        "security_incidents" | "security_clear" => Lead,
        "security_allow_lead" => Owner,
        // Open, with the reason.
        "agent_hello" => Open("names the caller's own session; engine writer names, granted names and authority roles are refused"),
        "time_control" => Open("the clock is shared on purpose: any agent pauses or steps it for its checks (a playtest owns it while one runs); the answer's set_by names who moved it last"),
        "window_view" => Open("points the human's window, which a human's input always takes back; nothing in the world changes"),
        "model_build" => Open("builds are derived data from recipes on disk: building changes no entity and no file"),
        "model_reload" => Open("re-reads recipes and anim sets from disk (the files are the truth); animation graphs that use a changed set resettle as the exempt writer anim_reload, journaled; nothing an agent made is replaced"),
        "world_save" => Open("a checkpoint of the shared world under its current name; another name (it renames the live world: autosave, evidence and board files), or overwrite over a world folder the caller didn't save this run, needs the lead (lead_only)"),
        "perf_zones" => Open("reads the profiler; switching it is refused while another agent's capture records (capture_in_progress)"),
        "logic_input" => Open("input to a running module, as a player's would be; the module's writes are checked as the caller's"),
        "lex_simulate" | "balance_sweep" | "balance_tune" => Open("simulates on its own copy of the rules; nothing shared changes (saved runs are the caller's)"),
        "procgen_texture" => Open("writes a new image file in the captures folder; nothing shared changes"),
        "tools_activate" | "tools_deactivate" => Open("changes only the caller's own tool list, within the cap the lead set (tools_scope)"),
        "tools_invoke" => Open("calls another tool by name, inside the caller's scope: that tool's own access applies"),
        "disk_prune" => Open("prunes only what the disk budget needs, never saves, worlds, journals or project files; free_mb, which deletes more, needs the lead"),
        _ => return None,
    })
}

impl Access {
    /// Short text for listings: `read`, `world:<rule>`, `replaces_world`, `owned:<store>`, `lead`,
    /// `reviewer`, `owner`, `open`.
    pub fn label(self) -> String {
        match self {
            Access::ReadOnly => "read".into(),
            Access::World(r) => format!("world:{r}"),
            Access::ReplacesWorld => "replaces_world".into(),
            Access::Owned(s) => format!("owned:{}", format!("{s:?}").to_ascii_lowercase()),
            Access::Lead => ROLE_LEAD.into(),
            Access::Reviewer => ROLE_REVIEWER.into(),
            Access::Owner => ROLE_OWNER.into(),
            Access::Open(_) => "open".into(),
        }
    }
}

/// The authority table as data (engine_info, detailed): tool → label, and each Open reason.
/// The table never changes while the engine runs: it is built once (about 1 ms for some 230
/// tools, which was half of every `engine_info` on the engine thread: performance pass,
/// 2026-11-14) and copied from then on. [`warm_access_json`] builds it at start-up.
pub fn access_json() -> Json {
    ACCESS_TABLE.get_or_init(build_access_json).clone()
}

static ACCESS_TABLE: std::sync::OnceLock<Json> = std::sync::OnceLock::new();

/// Builds the table now (start-up), so the first `engine_info` does not.
pub fn warm_access_json() {
    ACCESS_TABLE.get_or_init(build_access_json);
}

fn build_access_json() -> Json {
    let mut tools: Vec<(String, Json)> = Vec::new();
    for name in crate::tools::engine_tool_names() {
        let Some(a) = access(&name) else { continue };
        let v = match a {
            Access::Open(reason) => Json::obj([("access", Json::from("open")), ("reason", Json::from(reason))]),
            other => Json::from(other.label()),
        };
        tools.push((name, v));
    }
    Json::obj(tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table handed out is the table as it is built, every time (it is built once).
    #[test]
    fn the_access_table_is_built_once_and_stays_what_it_was() {
        let built = build_access_json();
        assert_eq!(access_json(), built);
        warm_access_json();
        assert_eq!(access_json(), built, "warming it changes nothing");
        let Json::Obj(tools) = &built else { panic!("an object") };
        assert!(tools.len() > 150 && tools.iter().any(|(name, _)| name == "engine_info"), "{} tools", tools.len());
    }

    #[test]
    fn one_call_one_commit() {
        let mut c = CallCommits::default();
        assert_eq!(c.count(7, 1, false), None, "one commit");
        assert_eq!(c.count(7, 1, false), Some(2), "a second request of the same call");
        assert_eq!(c.count(8, 2, false), Some(2), "two in one request");
        assert_eq!(c.count(9, 3, true), None, "independent items");
        assert_eq!(c.count(0, 1, false), None);
        assert_eq!(c.count(0, 1, false), None, "requests outside a call are counted alone");
        assert_eq!(c.count(0, 2, false), Some(2));
        for i in 100..(100 + MAX_CALLS_COUNTED as u64 + 10) {
            c.count(i, 1, false);
        }
        assert!(c.calls.len() <= MAX_CALLS_COUNTED, "bounded");
    }

    #[test]
    fn every_engine_tool_declares_its_access() {
        let missing: Vec<String> = crate::tools::engine_tool_names().into_iter().filter(|n| access(n).is_none()).collect();
        assert!(missing.is_empty(), "tools with no entry in authority::access (declare what each may do): {missing:?}");
        // Open entries say why.
        for n in crate::tools::engine_tool_names() {
            if let Some(Access::Open(reason)) = access(&n) {
                assert!(reason.len() > 30, "{n}: an Open entry needs its reason");
            }
        }
    }

    #[test]
    fn roles_carry_authority_only_when_granted() {
        let mut g = Grants::default();
        assert_eq!(g.admit("ann", "k1", "builder").unwrap(), Authority::Agent);
        assert_eq!(g.admit("carl", "k2", ROLE_OWNER).unwrap_err().code, "role_not_granted");
        assert_eq!(g.admit("bob", "k3", ROLE_REVIEWER).unwrap_err().code, "role_not_granted");
        // Owner, 2026-11-12: nobody becomes lead by asking, first or not, grants or none.
        for role in [ROLE_LEAD, ROLE_DIRECTOR] {
            let e = g.admit("lee", "k4", role).unwrap_err();
            assert_eq!((e.code, e.detail("refused_at")), ("role_not_granted", Some(&Json::from("agent_hello.role"))));
            assert!(e.message.contains("--lead NAME"), "the refusal says how: {}", e.message);
        }
        assert!(!g.lead_granted() && !g.holds("k4"));
        g.grant("lee", Authority::Lead).unwrap();
        assert!(g.lead_granted());
        assert_eq!(g.admit("lee", "k4", ROLE_LEAD).unwrap(), Authority::Lead, "the operator's grant");
    }

    #[test]
    fn operator_grants_bind_to_the_first_identity() {
        let mut g = Grants::default();
        g.grant("rev", Authority::Reviewer).unwrap();
        g.grant("boss", Authority::Owner).unwrap();
        g.grant("lead1", Authority::Lead).unwrap();
        assert_eq!(g.admit("rev", "k1", ROLE_REVIEWER).unwrap(), Authority::Reviewer);
        assert_eq!(g.admit("rev", "k1", "builder").unwrap(), Authority::Agent, "a granted name may say a plain role");
        assert_eq!(g.admit("rev", "k2", "builder").unwrap_err().code, "grant_bound", "another identity can't take the name");
        assert_eq!(g.admit("rev", "k1", ROLE_LEAD).unwrap_err().code, "role_not_granted", "a reviewer grant isn't a lead's");
        assert_eq!(g.admit("boss", "k3", ROLE_OWNER).unwrap(), Authority::Owner);
        assert_eq!(g.admit("boss", "k3", ROLE_LEAD).unwrap(), Authority::Lead, "an owner grant covers less");
        // Someone else asking for lead gets nothing.
        assert_eq!(g.admit("dave", "k4", ROLE_LEAD).unwrap_err().code, "role_not_granted");
        assert_eq!(g.admit("lead1", "k5", ROLE_DIRECTOR).unwrap(), Authority::Lead);
        assert!(g.grant("physics", Authority::Lead).is_err());
        assert!(g.grant("", Authority::Lead).is_err());
    }

    #[test]
    fn engine_writer_names_are_reserved() {
        for n in ["physics", "nav", "anim_reload", "disk", "taste", "Taste", "engine:stray", "window:owner", "module:x", "lex:waves", "engine:anything", "window:other"] {
            assert!(reserved_writer(n), "{n}");
        }
        for n in ["ann", "lexi", "modules", "physicsbot", "team:a"] {
            assert!(!reserved_writer(n), "{n}");
        }
    }

    #[test]
    fn require_names_what_is_missing() {
        assert!(require(Authority::Lead, Authority::Reviewer, Site::ReviewDecide, "deciding").is_ok());
        let e = require(Authority::Agent, Authority::Lead, Site::FrameFloor, "the frame floor").unwrap_err();
        assert_eq!(e.code, "lead_only");
        assert_eq!(e.detail("needs"), Some(&Json::from("lead")));
        assert_eq!(e.detail("refused_at"), Some(&Json::from("swarm_settings.frame_floor_fps")));
    }
}

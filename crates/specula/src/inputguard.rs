//! The input-integrity monitor (docs/design/SECURITY.md): one place that sees every change of the
//! window's keyboard focus, every system-clipboard access, every engine screen put on the window,
//! every simulated input that acts as the window's human, and every refused attempt at any of
//! these; classifies each; and on an incident freezes the offending identity and session (and
//! escalates to a swarm-wide pause when a second identity offends).
//!
//! **Every agent is untrusted** (ours and other vendors' alike): who an actor is comes only from
//! the operator's grants ([`crate::authority::authority`] / [`crate::authority::of_session`]) and
//! the swarm's identity key (what a resume token carries), never from a name or a role it gave.
//! A name is only a label: freezes hold the identity key and the session, so taking another
//! agent's name can't freeze it (review R1 of m5-inputguard, major 3).
//!
//! **Only explicit tries count.** A refusal caused by state the agent didn't choose (the human
//! typing, an engine screen open when the agent named no surface, the human taking a held action
//! back) is [`declined`]: recorded, never counted. A granted lead's refused try is an alert, never
//! an incident, and the owner's and the lead's `security_*` tools always answer: a lead-only swarm
//! can't lock itself out (review R1, blocker 1).
//!
//! **It can't be bypassed.** The functions that point the keyboard, reach the clipboard thread and
//! open an engine screen take a [`Seen`], which only this module makes, after recording the event
//! ([`done`]). `crates/specula/tests/inputguard_guard.rs` holds the rest (no other construction,
//! clipboard calls only on its thread, every refusal site reported).
//!
//! Global, like the incident store: the caller threads (`text_edit`'s check), the clipboard's
//! requests and the engine thread all feed it, and the gate ([`gate`]) runs on every connection
//! thread before a call is routed. The engine applies what an incident halts (claims, leases and
//! input holds released, the clock paused, the journal records) in `inputguard_engine`.

use crate::authority::{skeleton, Authority, WINDOW_OWNER};
use specula_core::trace::{Level, Subsystem, Value};
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// The monitor's trace events (`security.alert`, `security.incident`, `security.cleared`, ...).
pub static SECURITY: Subsystem = Subsystem::new("security");

/// Events the monitor keeps (the newest; the lead reads them with `security_incidents`): the
/// lead's feed (alerts, incidents, allowed leads, clears and allowances).
pub const MAX_LOG: usize = 512;
/// The fine and noted events kept beside it (`security_incidents {all}`). They have a log of
/// their own, so events nobody counts can't push an alert out of the lead's feed (review R2 of
/// m5-inputguard, minor 7: 600 plain refusals did).
pub const MAX_QUIET: usize = 512;
/// `security_incidents {wait_s}` calls waiting at once; past it a call answers at once.
pub const MAX_WAITERS: usize = 32;
/// Characters kept of an event's `what` and `detail`.
pub const MAX_DETAIL: usize = 200;
/// Longest `security_incidents` waits.
pub const MAX_WAIT_S: u64 = 60;
/// Events one `security_incidents` answer holds at most.
pub const MAX_LIMIT: usize = 100;
/// Leads allowed at once (past it the oldest allowance goes).
pub const MAX_ALLOWED: usize = 64;
/// Open incidents, frozen identities and frozen sessions past which the swarm pauses (the monitor
/// fails closed: it never forgets a freeze to make room). An offender's further offences count on
/// its open incident.
pub const MAX_OPEN: usize = 1024;
pub const MAX_FROZEN: usize = 1024;
/// Session → identity bindings kept for the caller threads (past it the oldest goes; a session it
/// forgets is its own identity again, as before its hello).
pub const MAX_BOUND: usize = 4096;
/// Alert toasts queued at most (merged per identity and channel; past it the oldest alert goes,
/// counted). Incident toasts are never dropped: they last while their incident is open.
pub const MAX_ALERT_TOASTS: usize = 16;
/// Offender numbers handed out (they tell identities with the same name apart in trace
/// incidents without showing their keys); past it they share number 0.
const MAX_ORDINALS: usize = 4096;

/// What an event was about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    /// The window's keyboard pointed at a world text (`text_edit {window_focus}`), or dropped.
    Keyboard,
    /// The system clipboard read (a paste) or written (a copy or a cut).
    Clipboard,
    /// An engine screen (gallery, review) put on the window.
    Screen,
    /// Simulated window input as the window's human (`editor_input`).
    EditorInput,
    /// An engine screen's buttons driven (`ui_act` on surface review or gallery).
    ScreenInput,
    /// An agent's hold on an action the human uses at the window (`logic_input`).
    ModuleInput,
    /// Native code loaded into the engine's process (`logic_build`, `logic_reload`,
    /// `logic_unload`): it could read the keyboard itself (the module lane reports refused tries).
    NativeCode,
    /// The monitor's own records: a clear, a lead allowed or no longer allowed, a flood.
    Monitor,
}

impl Channel {
    pub const ALL: [Channel; 8] = [Channel::Keyboard, Channel::Clipboard, Channel::Screen, Channel::EditorInput, Channel::ScreenInput, Channel::ModuleInput, Channel::NativeCode, Channel::Monitor];

    pub fn name(self) -> &'static str {
        match self {
            Channel::Keyboard => "keyboard",
            Channel::Clipboard => "clipboard",
            Channel::Screen => "screen",
            Channel::EditorInput => "editor_input",
            Channel::ScreenInput => "screen_input",
            Channel::ModuleInput => "module_input",
            Channel::NativeCode => "native_code",
            Channel::Monitor => "monitor",
        }
    }
}

/// How the monitor judged an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// The owner's own action (the human at the window, an `--owner` agent), or the engine
    /// carrying on one; or a focus that dropped.
    Fine,
    /// Refused (or ended) for state the agent didn't choose: the human typing, an engine screen
    /// open when the call named no surface, the human taking back an action an agent held.
    /// Recorded; never counted.
    Noted,
    /// A granted lead the owner allowed for its session.
    Allowed,
    /// A granted lead (any try, done or refused; or, for screens, a reviewer): the owner is
    /// told; nothing is halted.
    Alert,
    /// An explicit refused try, or input taken, by an agent below lead: its identity and session
    /// freeze.
    Incident,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Fine => "fine",
            Class::Noted => "noted",
            Class::Allowed => "allowed",
            Class::Alert => "alert",
            Class::Incident => "incident",
        }
    }
}

/// Who acted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Actor {
    /// The human at the window (`window:owner`): never frozen, never locked out (F5).
    Human,
    /// The engine itself, carrying on the human's own action (the next onboarding step).
    Engine(&'static str),
    /// An agent: its identity key (what claims, undo and a resume token carry; never shown), its
    /// protocol session when known, the name it said, and the authority the operator's grants
    /// give it ([`crate::authority::trust`]: a lead nobody granted is a plain agent here).
    Agent { session: Option<String>, key: String, name: String, authority: Authority },
}

impl Actor {
    /// An agent's session whose identity is the session itself (as before its hello; tests).
    pub fn agent(session: &str, name: &str, authority: Authority) -> Actor {
        Actor::keyed(session, session, name, authority)
    }

    /// An agent's session with its identity `key` (`name` as attributed): the window's own
    /// operator session is the human.
    pub fn keyed(session: &str, key: &str, name: &str, authority: Authority) -> Actor {
        if name == WINDOW_OWNER {
            return Actor::Human;
        }
        Actor::Agent { session: Some(session.to_owned()), key: key.to_owned(), name: name.to_owned(), authority }
    }

    /// An identity with no session at hand (a module lane's refusal names the key only): a
    /// freeze holds the identity, and with it whichever session carries it.
    pub fn identity(key: &str, name: &str, authority: Authority) -> Actor {
        if name == WINDOW_OWNER {
            return Actor::Human;
        }
        Actor::Agent { session: None, key: key.to_owned(), name: name.to_owned(), authority }
    }

    /// The caller of a tool on its own connection thread: named as `agent_hello` named it (or the
    /// session's default name), its identity as the engine bound it at hello, with the authority
    /// the engine last admitted it with.
    pub fn caller(session: &specula_protocol::SessionInfo) -> Actor {
        let name = crate::trace_sink::name_of(&session.id).map_or_else(|| crate::swarm::default_name(session), |(n, _)| n);
        let key = key_of(&session.id).unwrap_or_else(|| session.id.clone());
        Actor::keyed(&session.id, &key, &name, crate::authority::trusted_of_session(&session.id))
    }

    /// The caller of a request on the engine thread.
    pub fn of(swarm: &crate::swarm::Swarm, session: &specula_protocol::SessionInfo) -> Actor {
        Actor::keyed(&session.id, &swarm.key(session), &swarm.agent_name(session), crate::authority::trusted(swarm, &session.id))
    }

    pub fn name(&self) -> &str {
        match self {
            Actor::Human => WINDOW_OWNER,
            Actor::Engine(n) => n,
            Actor::Agent { name, .. } => name,
        }
    }

    pub fn authority(&self) -> Authority {
        match self {
            Actor::Human | Actor::Engine(_) => Authority::Owner,
            Actor::Agent { authority, .. } => *authority,
        }
    }

    /// The identity key (`None`: the human or the engine).
    pub fn key(&self) -> Option<&str> {
        match self {
            Actor::Agent { key, .. } => Some(key),
            _ => None,
        }
    }

    fn session(&self) -> Option<&str> {
        match self {
            Actor::Agent { session, .. } => session.as_deref(),
            _ => None,
        }
    }
}

/// Proof that the monitor saw an action before it happened: the functions that point the
/// keyboard, reach the clipboard thread and open an engine screen take one. Only this module
/// makes one (its field is private), and only after recording the event ([`done`]).
#[derive(Debug)]
#[must_use = "a Seen is the monitor's record of an action: hand it to the action"]
pub struct Seen {
    seq: u64,
}

impl Seen {
    /// The monitor's event this action is.
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

/// Tests elsewhere in the crate exercise the guarded actions directly.
#[cfg(test)]
pub(crate) fn seen_for_test() -> Seen {
    Seen { seq: 0 }
}

/// One thing the monitor saw.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub seq: u64,
    pub class: Class,
    pub channel: Channel,
    /// The call site (`text_edit.window_focus`, `gallery_open`, `paste`, ...).
    pub what: String,
    pub agent: String,
    pub authority: Authority,
    /// `None`: done; else the refusal's code.
    pub refused: Option<String>,
    /// From the call (an entity key, a gallery id): agent-chosen text, shown labelled.
    pub detail: String,
    /// What was halted: `session_frozen` or `swarm_paused`.
    pub halted: Option<&'static str>,
    /// The trace incident (`trace_incidents`, code `security.incident`).
    pub incident: Option<u64>,
    /// Its trace event (ring id).
    pub trace_event: Option<u64>,
    /// Journal entries recording what was halted (kind `security`).
    pub journal: Vec<u64>,
    pub at: crate::incidents::Stamp,
    /// The session and the identity key (never shown: either lets one act as that agent).
    session: Option<String>,
    key: Option<String>,
}

impl Event {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("seq", Json::from(self.seq)),
            ("class", Json::from(self.class.name())),
            ("channel", Json::from(self.channel.name())),
            ("what", Json::from(self.what.as_str())),
            ("agent", Json::from(self.agent.as_str())),
            ("authority", Json::from(self.authority.name())),
            ("outcome", Json::from(if self.refused.is_some() { "refused" } else { "done" })),
        ];
        if let Some(c) = &self.refused {
            f.push(("code", Json::from(c.as_str())));
        }
        if !self.detail.is_empty() {
            f.push(("detail", Json::from(self.detail.as_str())));
        }
        f.push(("halted", self.halted.map_or(Json::Null, Json::from)));
        if let Some(i) = self.incident {
            f.push(("incident", Json::from(i)));
        }
        if let Some(e) = self.trace_event {
            f.push(("trace_event", Json::from(e)));
        }
        if !self.journal.is_empty() {
            f.push(("journal", Json::arr(self.journal.iter().map(|j| Json::from(*j)))));
        }
        f.push(("t_ms", Json::from((self.at.time_us / 100) as f64 / 10.0)));
        f.push(("tick", Json::from(self.at.tick)));
        f.push(("frame", Json::from(self.at.frame)));
        Json::obj(f)
    }

    fn fine(what: &str, by: &str, authority: Authority, detail: &str, incident: Option<u64>) -> Event {
        Event {
            seq: 0,
            class: Class::Fine,
            channel: Channel::Monitor,
            what: what.to_owned(),
            agent: by.to_owned(),
            authority,
            refused: None,
            detail: cut(detail),
            halted: None,
            incident,
            trace_event: None,
            journal: Vec::new(),
            at: crate::incidents::Stamp::now(),
            session: None,
            key: None,
        }
    }
}

/// What an incident halted, for the engine to apply on its thread (`inputguard_engine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Release the identity's claims, leases and input holds; journal the freeze.
    Freeze { key: String, name: String, incident: u64, event: u64 },
    /// Pause the clock and journal the swarm pause.
    Pause { incident: u64, event: u64 },
    /// Journal a clear (and resume the clock when the pause stopped it).
    Cleared { by: String, incidents: Vec<u64>, unpaused: bool, event: u64 },
    /// Journal an allowance (or its revocation).
    Allowed { by: String, lead: String, revoked: bool, event: u64 },
}

/// An incident not cleared yet.
#[derive(Clone, Debug, Default)]
struct Open {
    /// Offenders: identity key → name.
    offenders: BTreeMap<String, String>,
    /// Their sessions.
    sessions: BTreeSet<String>,
    /// The owner chose "Keep frozen": only the owner clears it.
    kept_by_owner: bool,
    /// The newest event (kept here: the log may have moved on).
    last: Option<Event>,
    /// Offences counted on it.
    count: u64,
}

/// An alert waiting in the owner's toast: one per identity and channel (repeats merge).
#[derive(Clone, Debug)]
struct AlertToast {
    key: Option<String>,
    session: Option<String>,
    channel: Channel,
    /// The newest event.
    last: Event,
    count: u64,
}

#[derive(Default)]
struct State {
    seq: u64,
    /// The lead's feed, by seq (alerts, incidents, allowed leads, the monitor's own records).
    log: VecDeque<Event>,
    /// Fine and noted events, by seq (kept apart: see [`MAX_QUIET`]).
    quiet: VecDeque<Event>,
    /// The newest seq the feed has dropped (0: none): a reader whose cursor is before it missed
    /// events, and is told so.
    lost_through: u64,
    /// The same for the quiet log (what an `all` reader missed).
    lost_quiet_through: u64,
    /// `security_incidents` calls waiting now.
    waiters: usize,
    /// Session id → the incident that froze it.
    frozen_sessions: BTreeMap<String, u64>,
    /// Identity key → (its name, the incident): a resume in a new session carries the freeze.
    frozen_keys: BTreeMap<String, (String, u64)>,
    open: BTreeMap<u64, Open>,
    /// The swarm pause: the incident that escalated, its event.
    paused: Option<(u64, u64)>,
    /// Session id → the lead's name: allowed by the owner for that session.
    allowed: BTreeMap<String, String>,
    /// Session id → (bound when, identity key), from each hello (the caller threads' actors).
    bound: BTreeMap<String, (u64, String)>,
    bind_seq: u64,
    /// Identity key → its offender number (trace incidents are per identity).
    ordinals: BTreeMap<String, u64>,
    pending: VecDeque<Pending>,
    /// The owner's toast: open incidents (oldest first; never dropped) before alerts (merged per
    /// identity and channel; the oldest goes past [`MAX_ALERT_TOASTS`], counted).
    toast_incidents: VecDeque<u64>,
    toast_alerts: VecDeque<AlertToast>,
    toasts_dropped: u64,
    /// Counters by class.
    counts: BTreeMap<Class, u64>,
}

static STATE: Mutex<State> = Mutex::new(State {
    seq: 0,
    log: VecDeque::new(),
    quiet: VecDeque::new(),
    lost_through: 0,
    lost_quiet_through: 0,
    waiters: 0,
    frozen_sessions: BTreeMap::new(),
    frozen_keys: BTreeMap::new(),
    open: BTreeMap::new(),
    paused: None,
    allowed: BTreeMap::new(),
    bound: BTreeMap::new(),
    bind_seq: 0,
    ordinals: BTreeMap::new(),
    pending: VecDeque::new(),
    toast_incidents: VecDeque::new(),
    toast_alerts: VecDeque::new(),
    toasts_dropped: 0,
    counts: BTreeMap::new(),
});
static WAKE: Condvar = Condvar::new();
/// Something is frozen or paused: the gate looks (else it costs one atomic load per call).
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The engine has something to apply.
static PENDING: AtomicBool = AtomicBool::new(false);
/// The engine is shutting down: waiting calls answer now ([`stopping`]).
static STOPPING: AtomicBool = AtomicBool::new(false);

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn cut(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_DETAIL).collect()
}

impl State {
    fn refresh_flags(&self) {
        ACTIVE.store(!self.frozen_sessions.is_empty() || !self.frozen_keys.is_empty() || self.paused.is_some(), Relaxed);
        PENDING.store(!self.pending.is_empty(), Relaxed);
    }

    fn push(&mut self, mut e: Event) -> u64 {
        self.seq += 1;
        e.seq = self.seq;
        *self.counts.entry(e.class).or_default() += 1;
        match e.class {
            Class::Incident => {
                if let Some(id) = e.incident {
                    if let Some(o) = self.open.get_mut(&id) {
                        o.last = Some(e.clone());
                        o.count += 1;
                    }
                    if self.open.contains_key(&id) && !self.toast_incidents.contains(&id) {
                        self.toast_incidents.push_back(id);
                    }
                }
            }
            Class::Alert => {
                match self.toast_alerts.iter_mut().find(|a| a.key == e.key && a.channel == e.channel) {
                    Some(a) => {
                        a.last = e.clone();
                        a.count += 1;
                        a.session = e.session.clone();
                    }
                    None => {
                        if self.toast_alerts.len() >= MAX_ALERT_TOASTS {
                            self.toast_alerts.pop_front();
                            self.toasts_dropped += 1;
                        }
                        self.toast_alerts.push_back(AlertToast { key: e.key.clone(), session: e.session.clone(), channel: e.channel, last: e.clone(), count: 1 });
                    }
                }
            }
            _ => {}
        }
        if Self::in_feed(&e) {
            if self.log.len() >= MAX_LOG {
                if let Some(old) = self.log.pop_front() {
                    self.lost_through = old.seq;
                }
            }
            self.log.push_back(e);
        } else {
            if self.quiet.len() >= MAX_QUIET {
                if let Some(old) = self.quiet.pop_front() {
                    self.lost_quiet_through = old.seq;
                }
            }
            self.quiet.push_back(e);
        }
        self.seq
    }

    /// Whether `e` belongs to the lead's feed (else the quiet log).
    fn in_feed(e: &Event) -> bool {
        matches!(e.class, Class::Alert | Class::Incident | Class::Allowed) || e.channel == Channel::Monitor
    }

    fn event_mut(&mut self, seq: u64) -> Option<&mut Event> {
        // Both logs are in seq order.
        if let Ok(i) = self.log.binary_search_by_key(&seq, |e| e.seq) {
            return self.log.get_mut(i);
        }
        let i = self.quiet.binary_search_by_key(&seq, |e| e.seq).ok()?;
        self.quiet.get_mut(i)
    }

    /// The distinct offending identities of the open incidents.
    fn offenders(&self) -> BTreeSet<&str> {
        self.open.values().flat_map(|o| o.offenders.keys().map(String::as_str)).collect()
    }

    fn frozen_incident(&self, session: &str, key: Option<&str>) -> Option<u64> {
        self.frozen_sessions.get(session).copied().or_else(|| key.and_then(|k| self.frozen_keys.get(k).map(|(_, i)| *i)))
    }

    fn flooded(&self) -> bool {
        self.open.len() > MAX_OPEN || self.frozen_sessions.len() > MAX_FROZEN || self.frozen_keys.len() > MAX_FROZEN
    }

    /// Pauses the swarm for incident `id` (the event `seq` that escalated), once.
    fn pause(&mut self, id: u64, seq: u64) -> bool {
        if self.paused.is_some() {
            return false;
        }
        self.paused = Some((id, seq));
        self.pending.push_back(Pending::Pause { incident: id, event: seq });
        true
    }

    fn ordinal(&mut self, key: &str) -> u64 {
        if let Some(n) = self.ordinals.get(key) {
            return *n;
        }
        if self.ordinals.len() >= MAX_ORDINALS {
            return 0;
        }
        let n = self.ordinals.len() as u64 + 1;
        self.ordinals.insert(key.to_owned(), n);
        n
    }
}

/// How the event went, as the call site saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome<'a> {
    /// Done (the [`Seen`] goes to the action).
    Done,
    /// Refused: an explicit try (it named the human's input, and authority refused it).
    Refused(&'a str),
    /// Refused or ended for state the agent didn't choose: never counted.
    Declined(&'a str),
}

impl<'a> Outcome<'a> {
    fn code(self) -> Option<&'a str> {
        match self {
            Outcome::Done => None,
            Outcome::Refused(c) | Outcome::Declined(c) => Some(c),
        }
    }
}

/// How `actor`'s event on `channel` is classed.
fn classify(s: &State, channel: Channel, actor: &Actor, outcome: Outcome<'_>) -> Class {
    match actor {
        Actor::Human | Actor::Engine(_) => Class::Fine,
        Actor::Agent { authority: Authority::Owner, .. } => Class::Fine,
        Actor::Agent { .. } if matches!(outcome, Outcome::Declined(_)) => Class::Noted,
        // A granted lead is never frozen (a lead-only swarm must never lock itself out): its
        // refused tries are alerts, and so are its actions unless the owner allowed it.
        Actor::Agent { authority: Authority::Lead, session, .. } => {
            if outcome == Outcome::Done && session.as_ref().is_some_and(|s0| s.allowed.contains_key(s0)) {
                Class::Allowed
            } else {
                Class::Alert
            }
        }
        // Reviewers keep putting screens on the window (R4, until the owner answers): an alert.
        Actor::Agent { authority: Authority::Reviewer, .. } if channel == Channel::Screen && outcome == Outcome::Done => Class::Alert,
        Actor::Agent { .. } => Class::Incident,
    }
}

/// Records `actor` doing `what` on `channel` (before it happens) and hands back the proof the
/// action takes. A lead's action raises an alert; an action by an agent below what the channel
/// allows (it got past a check that should have refused it) is an incident.
pub fn done(channel: Channel, what: &str, actor: &Actor, detail: &str) -> Seen {
    Seen { seq: record(channel, what, actor, Outcome::Done, detail) }
}

/// Records an explicit refused attempt by `actor` (refused with `code`): a security incident for
/// an agent below lead, an alert for a granted lead, fine for the owner.
pub fn refused(channel: Channel, what: &str, actor: &Actor, code: &str, detail: &str) -> u64 {
    record(channel, what, actor, Outcome::Refused(code), detail)
}

/// Records a refusal (or an end) caused by state the agent didn't choose (the human typing, an
/// engine screen open when the call named no surface, the human taking back a held action): seen,
/// never counted, nothing halted.
pub fn declined(channel: Channel, what: &str, actor: &Actor, code: &str, detail: &str) -> u64 {
    record(channel, what, actor, Outcome::Declined(code), detail)
}

/// The module lane's seam: a refused try at loading native code by the identity `key` (named
/// `name`, with the authority its grant gives). Classed as [`refused`] is.
pub fn refused_identity(channel: Channel, what: &str, name: &str, key: &str, authority: Authority, code: &str, detail: &str) -> u64 {
    refused(channel, what, &Actor::identity(key, name, authority), code, detail)
}

/// Records something that needs no proof (a focus that dropped): the owner's side of things.
pub fn note(channel: Channel, what: &str, detail: &str) {
    record(channel, what, &Actor::Engine("engine"), Outcome::Done, detail);
}

fn record(channel: Channel, what: &str, actor: &Actor, outcome: Outcome<'_>, detail: &str) -> u64 {
    let (class, ordinal) = {
        let mut s = state();
        let class = classify(&s, channel, actor, outcome);
        let ordinal = match (class, actor.key()) {
            (Class::Incident, Some(k)) => s.ordinal(k),
            _ => 0,
        };
        (class, ordinal)
    };
    let at = crate::incidents::Stamp::now();
    let (what, detail) = (cut(what), cut(detail));
    let code = outcome.code();
    let mut event = Event {
        seq: 0,
        class,
        channel,
        what: what.clone(),
        agent: actor.name().to_owned(),
        authority: actor.authority(),
        refused: code.map(str::to_owned),
        detail: detail.clone(),
        halted: None,
        incident: None,
        trace_event: None,
        journal: Vec::new(),
        at,
        session: actor.session().map(str::to_owned),
        key: actor.key().map(str::to_owned),
    };
    let tried = match code {
        Some(code) => format!("tried {what} (refused: {code})"),
        None => format!("did {what}"),
    };
    // The trace incident first (its own lock): one per offending identity (its number tells
    // identities under one name apart; the key itself is never shown), so its repeats count on it.
    if class == Class::Incident {
        let id = crate::incidents::raise(crate::incidents::Raise {
            sub: &SECURITY,
            level: Level::Error,
            code: "security.incident",
            key: &format!("agent:{}#{ordinal}", skeleton(actor.name())),
            summary: format!("{} ({} authority) {tried} on the {}: its session is frozen", actor.name(), actor.authority().name(), channel.name()),
            fields: vec![
                ("agent", Value::from(actor.name())),
                ("channel", Value::from(channel.name())),
                ("what", Value::from(what.as_str())),
                ("code", Value::from(code.unwrap_or("taken"))),
            ],
            evidence: Json::obj([("channel", Json::from(channel.name())), ("what", Json::from(what.as_str())), ("code", Json::from(code)), ("detail", Json::from(detail.as_str()))]),
            call: crate::tools::current_call(),
            agent: Some(actor.name().to_owned()),
        });
        event.incident = Some(id);
        event.trace_event = crate::incidents::get(id).and_then(|i| i.events.back().copied());
    }
    let mut s = state();
    let seq = s.seq + 1;
    if let (Class::Incident, Some(id), Actor::Agent { session, key, name, .. }) = (class, event.incident, actor) {
        let others = s.offenders().into_iter().any(|o| o != key);
        let open = s.open.entry(id).or_default();
        open.offenders.insert(key.clone(), name.clone());
        if let Some(sess) = session {
            open.sessions.insert(sess.clone());
        }
        if let Some(sess) = session {
            s.frozen_sessions.insert(sess.clone(), id);
        }
        s.frozen_keys.insert(key.clone(), (name.clone(), id));
        s.pending.push_back(Pending::Freeze { key: key.clone(), name: name.clone(), incident: id, event: seq });
        event.halted = Some("session_frozen");
        // A second distinct identity while an incident is open pauses the whole swarm; so does a
        // flood past MAX_OPEN / MAX_FROZEN (it fails closed: never forgets a freeze to make room).
        if (others || s.flooded()) && s.pause(id, seq) {
            event.halted = Some("swarm_paused");
        }
    }
    let halted = event.halted;
    let seq = s.push(event);
    s.refresh_flags();
    drop(s);
    match class {
        Class::Incident => {
            specula_core::trace_event!(&SECURITY, Error, "security.frozen", agent = actor.name(), channel = channel.name(), what = what.as_str(), halted = halted.unwrap_or(""), event = seq);
        }
        Class::Alert => {
            specula_core::trace_event!(&SECURITY, Warn, "security.alert", agent = actor.name(), authority = actor.authority().name(), channel = channel.name(), what = what.as_str(), event = seq);
        }
        Class::Noted => {
            specula_core::trace_event!(&SECURITY, Info, "security.declined", agent = actor.name(), channel = channel.name(), what = what.as_str(), code = code.unwrap_or(""), event = seq);
        }
        Class::Allowed | Class::Fine => {
            specula_core::trace_event!(&SECURITY, Debug, "security.seen", agent = actor.name(), channel = channel.name(), what = what.as_str(), class = class.name(), event = seq);
        }
    }
    if matches!(class, Class::Alert | Class::Incident) {
        WAKE.notify_all();
    }
    seq
}

/// Why a call must be refused now (`None`: it may go on). The owner's and a granted lead's
/// `security_*` tools always answer (a lead-only swarm never locks itself out). Otherwise: its
/// session or identity is frozen, or the swarm is paused and it isn't the owner's or the lead's
/// read. `key`: the session's identity key; `tool`: the tool called (`None`: unknown, e.g. a
/// playtest's internal step); `read`: the call changes nothing.
pub fn refusal(session: &str, key: Option<&str>, authority: Authority, tool: Option<&str>, read: bool) -> Option<crate::world::WorldError> {
    if !ACTIVE.load(Relaxed) {
        return None;
    }
    let security = tool.is_some_and(|t| t.starts_with("security_"));
    if authority >= Authority::Lead && security {
        return None;
    }
    let s = state();
    if let Some(id) = s.frozen_incident(session, key) {
        let (what, agent) = s.open.get(&id).and_then(|o| o.last.as_ref()).map_or((String::new(), String::new()), |e| (e.what.clone(), e.agent.clone()));
        return Some(
            crate::world::WorldError::new(
                "session_frozen",
                format!(
                    "this session is frozen by security incident {id} ({agent} tried {what}): nothing more is accepted from it until the owner or the lead clears it \
                     (security_clear); agent_hello still answers. docs/design/SECURITY.md"
                ),
            )
            .with("incident", id)
            .with("retry", false),
        );
    }
    if let Some((id, _)) = s.paused {
        if authority >= Authority::Lead && read {
            return None;
        }
        return Some(
            crate::world::WorldError::new(
                "swarm_paused",
                format!(
                    "the swarm is paused by security incident {id} (a second agent took the window's input): every call is refused until the owner or the lead clears it \
                     (security_clear); the lead and the owner may still read and use the security_* tools. docs/design/SECURITY.md"
                ),
            )
            .with("incident", id)
            .with("retry_after_s", 30),
        );
    }
    None
}

/// The gate on a connection thread, before a call is routed (`ToolHandler::call`): `agent_hello`
/// always answers; everything else from a frozen session or identity, or during a swarm pause, is
/// refused (the owner's and the lead's `security_*` tools never are).
pub fn gate(session: &specula_protocol::SessionInfo, tool: &str, args: &Json) -> Option<crate::world::WorldError> {
    if !ACTIVE.load(Relaxed) || tool == "agent_hello" {
        return None;
    }
    // tools_invoke is judged by the tool it invokes.
    let tool = if tool == "tools_invoke" { args.get("tool").and_then(Json::as_str).unwrap_or(tool) } else { tool };
    let read = is_read(tool);
    let key = key_of(&session.id).unwrap_or_else(|| session.id.clone());
    refusal(&session.id, Some(&key), crate::authority::trusted_of_session(&session.id), Some(tool), read)
}

/// Whether `tool` only reads (or only changes the caller's own tool list).
pub fn is_read(tool: &str) -> bool {
    matches!(crate::authority::access(tool), Some(crate::authority::Access::ReadOnly)) || matches!(tool, "tools_catalog" | "tools_activate" | "tools_deactivate")
}

/// Whether anything is frozen or paused.
pub fn active() -> bool {
    ACTIVE.load(Relaxed)
}

/// What the engine must apply now (cheap when nothing is waiting).
pub fn take_pending() -> Vec<Pending> {
    if !PENDING.load(Relaxed) {
        return Vec::new();
    }
    let mut s = state();
    let out: Vec<Pending> = s.pending.drain(..).collect();
    s.refresh_flags();
    out
}

/// The engine journaled `entry` for event `event`.
pub fn note_journal(event: u64, entry: u64) {
    let mut s = state();
    if let Some(e) = s.event_mut(event) {
        if e.journal.len() < 8 {
            e.journal.push(entry);
        }
    }
}

/// The identity key session `session` said hello with (`None`: not bound, or forgotten).
pub fn key_of(session: &str) -> Option<String> {
    state().bound.get(session).map(|(_, k)| k.clone())
}

/// A session said agent_hello as the identity `key` (a resume carries the key): the caller
/// threads learn its identity, and a frozen identity freezes the session too. A flood of such
/// sessions past [`MAX_FROZEN`] pauses the swarm (fails closed). Returns the incident.
pub fn hello(session: &str, key: &str) -> Option<u64> {
    let mut s = state();
    s.bind_seq += 1;
    let at = s.bind_seq;
    s.bound.insert(session.to_owned(), (at, key.to_owned()));
    if s.bound.len() > MAX_BOUND {
        if let Some(oldest) = s.bound.iter().min_by_key(|(_, (n, _))| *n).map(|(k, _)| k.clone()) {
            s.bound.remove(&oldest);
        }
    }
    if !ACTIVE.load(Relaxed) {
        return None;
    }
    let (id, by_key) = match s.frozen_sessions.get(session) {
        Some(id) => (*id, false),
        None => (s.frozen_keys.get(key).map(|(_, i)| *i)?, true),
    };
    if by_key {
        // Past the limit no more sessions are kept: the identity's own freeze covers every
        // session that carries it (the gate reads the session's key), and the swarm pauses. The
        // monitor never forgets a freeze, and never grows without bound either.
        if s.frozen_sessions.len() <= MAX_FROZEN {
            s.frozen_sessions.insert(session.to_owned(), id);
            if let Some(o) = s.open.get_mut(&id) {
                o.sessions.insert(session.to_owned());
            }
        }
        if s.flooded() {
            let name = s.frozen_keys.get(key).map(|(n, _)| n.clone()).unwrap_or_default();
            let seq = s.seq + 1;
            if s.pause(id, seq) {
                let mut e = Event::fine("agent_hello", &name, Authority::Agent, "a frozen identity's sessions passed the monitor's limit: the swarm pauses (fails closed)", Some(id));
                e.class = Class::Incident;
                e.halted = Some("swarm_paused");
                s.push(e);
                specula_core::trace_event!(&SECURITY, Error, "security.flood", agent = name.as_str(), incident = id, sessions = s.frozen_sessions.len() as u64);
                WAKE.notify_all();
            }
        }
    }
    s.refresh_flags();
    Some(id)
}

/// Whether `session` (or its identity `key`) is frozen.
pub fn frozen(session: &str, key: Option<&str>) -> Option<u64> {
    if !ACTIVE.load(Relaxed) {
        return None;
    }
    state().frozen_incident(session, key)
}

/// Whether the identity `key` is frozen (the engine releases its input holds).
pub fn identity_frozen(key: &str) -> bool {
    ACTIVE.load(Relaxed) && state().frozen_keys.contains_key(key)
}

/// The swarm pause's incident, if paused.
pub fn paused() -> Option<u64> {
    if !ACTIVE.load(Relaxed) {
        return None;
    }
    state().paused.map(|(i, _)| i)
}

/// What a clear did.
#[derive(Debug)]
pub struct Cleared {
    pub incidents: Vec<u64>,
    pub unfrozen: Vec<String>,
    pub unpaused: bool,
    pub event: u64,
}

/// Clears incident `incident` (`None`: all open ones) by `by` (the owner or a granted lead; the
/// caller checked): its sessions and identities unfreeze, and the swarm pause lifts once fewer
/// than two offenders remain. An incident the owner kept frozen is the owner's alone to clear.
pub fn clear(incident: Option<u64>, by: &Actor, note: &str) -> Result<Cleared, crate::world::WorldError> {
    let mut s = state();
    let targets: Vec<u64> = match incident {
        Some(id) if s.open.contains_key(&id) => vec![id],
        Some(id) => {
            return Err(crate::world::WorldError::new("incident_not_open", format!("no open security incident {id} (security_incidents lists them)")).with("incident", id));
        }
        None => s.open.keys().copied().collect(),
    };
    if by.authority() < Authority::Owner {
        if let Some(id) = targets.iter().find(|id| s.open.get(id).is_some_and(|o| o.kept_by_owner)) {
            return Err(crate::authority::refusal(crate::authority::Site::SecurityKept, format!("the owner chose to keep incident {id} frozen: only the owner clears it (at the window, or an --owner agent)"))
                .with("incident", *id));
        }
    }
    let mut unfrozen = Vec::new();
    for id in &targets {
        if let Some(o) = s.open.remove(id) {
            for sess in &o.sessions {
                if s.frozen_sessions.get(sess) == Some(id) {
                    s.frozen_sessions.remove(sess);
                }
            }
            for (key, name) in &o.offenders {
                if s.frozen_keys.get(key).is_some_and(|(_, i)| i == id) {
                    s.frozen_keys.remove(key);
                    unfrozen.push(name.clone());
                }
            }
        }
    }
    // A clear-all clears every freeze (a flood's included).
    if incident.is_none() {
        s.frozen_sessions.clear();
        s.frozen_keys.clear();
    }
    let unpaused = s.paused.is_some() && (incident.is_none() || s.offenders().len() < 2);
    if unpaused {
        s.paused = None;
    }
    let detail = format!("cleared {} incident(s){}{}", targets.len(), if unpaused { "; the swarm pause lifted" } else { "" }, if note.is_empty() { String::new() } else { format!(": {note}") });
    let event = s.push(Event::fine("security_clear", by.name(), by.authority(), &detail, targets.first().copied()));
    s.pending.push_back(Pending::Cleared { by: by.name().to_owned(), incidents: targets.clone(), unpaused, event });
    // Their toasts are done.
    let cleared: BTreeSet<u64> = targets.iter().copied().collect();
    s.toast_incidents.retain(|i| !cleared.contains(i));
    s.refresh_flags();
    drop(s);
    for id in &targets {
        let _ = crate::incidents::act(*id, by.name(), crate::incidents::Action::Resolve, Some(&detail));
    }
    specula_core::trace_event!(&SECURITY, Warn, "security.cleared", by = by.name(), incidents = targets.len() as u64, unpaused = unpaused, event = event);
    WAKE.notify_all();
    Ok(Cleared { incidents: targets, unfrozen, unpaused, event })
}

/// The owner chose "Keep frozen" on incident `incident`: only the owner clears it now.
pub fn keep_frozen(incident: u64) -> bool {
    let mut s = state();
    let Some(o) = s.open.get_mut(&incident) else { return false };
    o.kept_by_owner = true;
    s.toast_incidents.retain(|i| *i != incident);
    drop(s);
    specula_core::trace_event!(&SECURITY, Warn, "security.kept_frozen", incident = incident);
    true
}

/// The owner allows (or, `revoke`, no longer allows) the lead named `lead` on its protocol
/// session `session`: its keyboard and screen actions there are `allowed`, not alerts.
pub fn allow_lead(session: &str, lead: &str, by: &str, revoke: bool) -> u64 {
    let mut s = state();
    if revoke {
        s.allowed.remove(session);
    } else {
        if s.allowed.len() >= MAX_ALLOWED && !s.allowed.contains_key(session) {
            s.allowed.pop_first();
        }
        s.allowed.insert(session.to_owned(), lead.to_owned());
    }
    // That lead's alerts are answered.
    s.toast_alerts.retain(|a| a.session.as_deref() != Some(session));
    let detail = format!("{} lead {lead} for its current session", if revoke { "no longer allowed:" } else { "allowed:" });
    let event = s.push(Event::fine(if revoke { "security_allow_lead.revoke" } else { "security_allow_lead" }, by, Authority::Owner, &detail, None));
    s.pending.push_back(Pending::Allowed { by: by.to_owned(), lead: lead.to_owned(), revoked: revoke, event });
    s.refresh_flags();
    drop(s);
    specula_core::trace_event!(&SECURITY, Info, "security.lead_allowed", lead = lead, revoked = revoke, event = event);
    event
}

/// The lead behind the alert toast of event `seq` (the toast's "Allow this lead" acts on it):
/// its session and name.
pub(crate) fn session_of(seq: u64) -> Option<(String, String)> {
    let s = state();
    s.toast_alerts.iter().find(|a| a.last.seq == seq && a.last.authority == Authority::Lead).and_then(|a| a.session.clone().map(|sess| (sess, a.last.agent.clone())))
}

/// Leads allowed now (names).
pub fn allowed_leads() -> Vec<String> {
    state().allowed.values().cloned().collect()
}

/// What the owner's toast shows now.
#[derive(Clone, Debug, PartialEq)]
pub struct ToastView {
    /// The newest event of the incident or alert shown.
    pub event: Event,
    /// How many times it happened (an incident's offences, an alert's merged repeats).
    pub repeats: u64,
    /// Other toasts waiting behind it.
    pub more: usize,
}

/// The toast to show now: the oldest open incident first (never dropped, until answered), then
/// the oldest alert (merged per identity and channel). Alerts older than `alert_for` are
/// answered by time.
pub fn toast(now_us: u64, alert_for: Duration) -> Option<ToastView> {
    let mut s = state();
    let limit = alert_for.as_micros() as u64;
    let st: &mut State = &mut s;
    st.toast_incidents.retain(|i| st.open.get(i).is_some_and(|o| !o.kept_by_owner && o.last.is_some()));
    st.toast_alerts.retain(|a| now_us.saturating_sub(a.last.at.time_us) < limit);
    let more = st.toast_incidents.len() + st.toast_alerts.len();
    if let Some(i) = st.toast_incidents.front() {
        let o = &st.open[i];
        return o.last.clone().map(|event| ToastView { event, repeats: o.count, more: more - 1 });
    }
    st.toast_alerts.front().map(|a| ToastView { event: a.last.clone(), repeats: a.count, more: more - 1 })
}

/// Dismisses the alert toast of event `seq` (an alert's "Dismiss").
pub fn dismiss(seq: u64) {
    state().toast_alerts.retain(|a| a.last.seq != seq);
}

/// The monitor's state as data (`security_incidents`, `engine_info`).
pub fn state_json() -> Json {
    let s = state();
    let frozen: Vec<Json> = s
        .frozen_keys
        .values()
        .map(|(name, id)| Json::obj([("agent", Json::from(name.as_str())), ("incident", Json::from(*id)), ("kept_by_owner", Json::from(s.open.get(id).is_some_and(|o| o.kept_by_owner)))]))
        .collect();
    let open: Vec<Json> = s
        .open
        .iter()
        .map(|(id, o)| {
            Json::obj([
                ("incident", Json::from(*id)),
                ("agents", Json::arr(o.offenders.values().map(|n| Json::from(n.as_str())))),
                ("kept_by_owner", Json::from(o.kept_by_owner)),
                ("event", o.last.as_ref().map_or(Json::Null, |e| Json::from(e.seq))),
                ("count", Json::from(o.count)),
            ])
        })
        .collect();
    Json::obj([
        ("frozen", Json::Arr(frozen)),
        ("frozen_sessions", Json::from(s.frozen_sessions.len())),
        ("open", Json::Arr(open)),
        ("swarm_paused", s.paused.map_or(Json::Null, |(i, e)| Json::obj([("incident", Json::from(i)), ("event", Json::from(e))]))),
        ("allowed_leads", Json::arr(s.allowed.values().map(|n| Json::from(n.as_str())))),
        ("toasts", Json::obj([("incidents", Json::from(s.toast_incidents.len())), ("alerts", Json::from(s.toast_alerts.len())), ("alerts_dropped", Json::from(s.toasts_dropped))])),
        ("counts", Json::obj(s.counts.iter().map(|(c, n)| (c.name(), Json::from(*n))))),
        ("last_seq", Json::from(s.seq)),
    ])
}

/// What a `security_incidents` call reads.
#[derive(Debug)]
pub struct Page {
    pub events: Vec<Json>,
    /// The cursor for the next call.
    pub next: u64,
    /// More events are waiting than `limit` took.
    pub more: bool,
    /// The feed dropped events after the caller's cursor before it read them (it holds the
    /// newest [`MAX_LOG`]): the journal (`kind: security`) and `trace_incidents` still have them.
    pub missed: bool,
}

/// The engine is shutting down: every waiting `security_incidents` answers now, and new ones
/// don't wait (a pending 60 s wait held the engine's shutdown for its whole length: review R2
/// of m5-inputguard, major 2).
pub fn stopping() {
    STOPPING.store(true, Relaxed);
    WAKE.notify_all();
}

/// Events after `after` (the lead's feed: alerts, incidents, allowed leads and the monitor's own
/// records; with `all`, the fine and noted events too), at most `limit`, waiting up to `wait`
/// for the first. The wait ends at once when the engine stops, and at most [`MAX_WAITERS`]
/// calls wait at a time (the rest answer at once).
pub fn wait_events(after: u64, all: bool, limit: usize, wait: Duration) -> Page {
    let deadline = std::time::Instant::now() + wait;
    let mut s = state();
    let any = |s: &State| s.log.back().is_some_and(|e| e.seq > after) || (all && s.quiet.back().is_some_and(|e| e.seq > after));
    if !any(&s) && !wait.is_zero() && s.waiters < MAX_WAITERS {
        s.waiters += 1;
        loop {
            if any(&s) || STOPPING.load(Relaxed) {
                break;
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                break;
            }
            // In short steps: a stop that raced the notify is still seen within one.
            let step = (deadline - now).min(Duration::from_millis(100));
            s = WAKE.wait_timeout(s, step).unwrap_or_else(PoisonError::into_inner).0;
        }
        s.waiters -= 1;
    }
    let mut matching: Vec<&Event> = s.log.iter().filter(|e| e.seq > after).collect();
    if all {
        matching.extend(s.quiet.iter().filter(|e| e.seq > after));
        matching.sort_by_key(|e| e.seq);
    }
    let more = matching.len() > limit;
    let page: Vec<&Event> = matching.into_iter().take(limit).collect();
    let next = page.last().map_or(after, |e| e.seq);
    let missed = after < s.lost_through || (all && after < s.lost_quiet_through);
    Page { events: page.into_iter().map(Event::to_json).collect(), next, more, missed }
}

/// Tests: forget everything (the monitor is global; tests that freeze clean up after them).
#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut s = state();
    *s = State::default();
    s.refresh_flags();
    STOPPING.store(false, Relaxed);
}

/// Tests that touch the global monitor run one at a time.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests;

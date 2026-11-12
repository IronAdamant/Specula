//! The input monitor's engine side (docs/design/SECURITY.md): what an incident halts is applied
//! here, on the engine thread (claims and leases released, the clock paused, the journal
//! records), requests from a frozen session or during a swarm pause are refused here too (calls
//! already in flight, playtest steps), and the owner is told in the window: an engine-drawn toast,
//! always on top (only the typing banner draws over it), never modal, with Clear, Keep frozen and
//! Allow this lead for this session; and the window's title.

use crate::authority::{self, Authority, WINDOW_OWNER};
use crate::inputguard::{self, Actor, Channel, Class, Pending};
use crate::journal::Writer;
use crate::tools::{Command, Request};
use crate::Engine;
use specula_core::Json;
use specula_gpu::{OverlayKind, OverlayQuad, TextureFormat, TextureId};
use specula_platform::{Event, MouseButton};
use specula_protocol::{SessionInfo, ToolOutput};
use specula_text::{AtlasConfig, GlyphAtlas, TextStyle};
use std::time::{Duration, Instant};

/// The writer the monitor's journal records are under (an engine writer: no agent may take it).
pub const WRITER: &str = "engine:security";
/// An alert's toast stays this long unless answered (an incident's stays until it is).
pub const ALERT_TOAST: Duration = Duration::from_secs(60);
/// The toast's text size (pixels), and the longest line it shows (characters).
const TOAST_PX: f32 = 15.0;
const TOAST_LINE_CHARS: usize = 72;
/// Characters of an event's detail the toast quotes (the detail can hold text an agent chose).
const TOAST_DETAIL_CHARS: usize = 48;
/// A toast's buttons act only this long after the toast last changed (what it shows, where it
/// is, its buttons): something appearing under the cursor can't take a click meant elsewhere.
pub const TOAST_SETTLE: Duration = Duration::from_millis(400);

/// The engine's side of the monitor.
#[derive(Default)]
pub struct Guard {
    /// The swarm pause stopped a running clock: the clear resumes it (unless someone moved the
    /// clock since).
    paused_clock: bool,
    toast: Toast,
}

/// What a toast button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    Clear,
    KeepFrozen,
    AllowLead,
    Dismiss,
}

impl Button {
    fn label(self) -> &'static str {
        match self {
            Button::Clear => "Clear",
            Button::KeepFrozen => "Keep frozen",
            Button::AllowLead => "Allow this lead for this session",
            Button::Dismiss => "Dismiss",
        }
    }
}

/// The toast as the last window frame drew it.
#[derive(Default)]
struct Toast {
    atlas: Option<GlyphAtlas>,
    pages: Vec<TextureId>,
    scratch: Vec<u8>,
    /// The event shown, its box and its buttons (window pixels), as drawn last.
    shown: Option<u64>,
    rect: Option<[f32; 4]>,
    buttons: Vec<([f32; 4], Button)>,
    /// The incident of the event shown.
    incident: Option<u64>,
    /// For the window's title.
    title: Option<String>,
    /// What the toast is about (the incident, or the alert's agent and channel) and when it,
    /// its box or its buttons last changed (engine clock, microseconds): the settle time.
    subject: Option<(Option<u64>, String, &'static str)>,
    changed_us: u64,
    /// A press the toast took, and the button it landed on once settled: the button acts when
    /// the same press is released on it.
    took_press: bool,
    armed: Option<Button>,
}

fn frozen_notice(incident: u64) -> Json {
    Json::obj([
        ("frozen", Json::Bool(true)),
        ("incident", Json::from(incident)),
        ("message", Json::from("this identity is frozen by a security incident: every other call is refused (session_frozen) until the owner or a granted lead clears it (docs/design/SECURITY.md)")),
    ])
}

/// Every hello binds the session to its identity `key` for the monitor (a resume carries the
/// key); a frozen identity's hello says so (it doesn't lift the freeze).
pub fn hello_reply(session: &str, key: &str, mut reply: Json) -> Json {
    if let Some(id) = inputguard::hello(session, key) {
        reply.set("security", frozen_notice(id));
    }
    reply
}

/// The answer to a hello from a session that is frozen (itself, or the identity it carries):
/// who it is and that it is frozen, and nothing else. **Its hello changes nothing**: no new
/// name, no new role, no other identity resumed. A frozen builder that said hello again as
/// "lead", or under a granted lead's name, got that authority and cleared its own incident
/// (review R2 of m5-inputguard, blocker 1).
pub fn frozen_hello(name: &str, authority: Authority, incident: u64) -> Json {
    Json::obj([
        ("agent", Json::from(name)),
        ("authority", Json::from(authority.name())),
        ("unchanged", Json::Bool(true)),
        ("note", Json::from("a frozen session's agent_hello changes nothing (name, role and identity stay as they were)")),
        ("security", frozen_notice(incident)),
    ])
}

/// Requests that only finish or clean up what an admitted call started (or say hello): never
/// refused by the monitor.
fn bookkeeping(c: &Command) -> bool {
    matches!(
        c,
        Command::Hello { .. } | Command::StudioDone(_) | Command::EvidenceRefused(_) | Command::Taste(crate::taste::TasteCall::RoundReady { .. })
    )
}

impl Engine {
    /// `agent_hello` from a session that isn't frozen (a frozen identity it resumes stays frozen
    /// in it: the answer says so).
    pub(crate) fn hello_unfrozen(&mut self, session: &SessionInfo, name: String, role: String, resume: Option<String>, live: &[SessionInfo]) -> ToolOutput {
        match self.swarm.hello(session, name, role, resume, live) {
            Ok(reply) => ToolOutput::text(hello_reply(&session.id, &self.swarm.key(session), reply).to_pretty()),
            Err(e) => crate::world_error(&e),
        }
    }

    /// Applies what the monitor's incidents, clears and allowances asked for (cheap when none).
    pub(crate) fn security_pump(&mut self) {
        let pending = inputguard::take_pending();
        if pending.is_empty() {
            return;
        }
        // The monitor's own work: no agent's call (its journal records aren't any call's).
        let _ctx = specula_core::trace::Ctx { call: 0, span: 0 }.enter();
        let tick = self.clock.tick();
        for p in pending {
            let (event, record) = match p {
                Pending::Freeze { key, name, incident, event } => {
                    let w = Writer::new(&name, Some(&key));
                    // Its claims and leases go at once, so the swarm isn't blocked by a frozen
                    // agent; and its input holds, so nothing it placed keeps pressing.
                    let claims = self.swarm.claims.release_all(w);
                    let leases = self.swarm.leases.release_for_delete(w, None, Instant::now()).map(|(ids, _)| ids).unwrap_or_default();
                    let holds = self.logic.release_holds_of(&key) + self.lex_live.release_holds_of(&key);
                    specula_core::trace_event!(&inputguard::SECURITY, Error, "security.session_frozen", agent = name.as_str(), incident = incident, claims = claims.len() as u64, leases = leases.len() as u64, holds = holds as u64);
                    (
                        event,
                        Json::obj([
                            ("event", Json::from("session_frozen")),
                            ("agent", Json::from(name.as_str())),
                            ("incident", Json::from(incident)),
                            ("monitor_event", Json::from(event)),
                            ("claims_released", Json::arr(claims.into_iter().take(64).map(Json::from))),
                            ("leases_released", Json::arr(leases.into_iter().take(64).map(Json::from))),
                            ("holds_released", Json::from(holds)),
                        ]),
                    )
                }
                Pending::Pause { incident, event } => {
                    // Ticks stop as time_control would stop them, by the monitor.
                    if !self.clock.paused() {
                        self.clock.set_paused(true);
                        self.security.paused_clock = true;
                    }
                    self.clock_set_by = Some((WRITER.to_owned(), "pause"));
                    specula_core::trace_event!(&inputguard::SECURITY, Error, "security.swarm_paused", incident = incident, event = event);
                    (event, Json::obj([("event", Json::from("swarm_paused")), ("incident", Json::from(incident)), ("monitor_event", Json::from(event)), ("clock_paused", Json::Bool(true))]))
                }
                Pending::Cleared { by, incidents, unpaused, event } => {
                    let mut resumed = false;
                    if unpaused {
                        // The clock goes on only if the pause stopped it and nobody moved it since.
                        let ours = self.clock_set_by.as_ref().is_some_and(|(b, op)| b == WRITER && *op == "pause");
                        if self.security.paused_clock && ours {
                            self.clock.set_paused(false);
                            self.clock_set_by = Some((WRITER.to_owned(), "resume"));
                            resumed = true;
                        }
                        self.security.paused_clock = false;
                    }
                    (
                        event,
                        Json::obj([
                            ("event", Json::from("cleared")),
                            ("by", Json::from(by.as_str())),
                            ("incidents", Json::arr(incidents.iter().take(64).map(|i| Json::from(*i)))),
                            ("swarm_unpaused", Json::Bool(unpaused)),
                            ("clock_resumed", Json::Bool(resumed)),
                            ("monitor_event", Json::from(event)),
                        ]),
                    )
                }
                Pending::Allowed { by, lead, revoked, event } => (
                    event,
                    Json::obj([
                        ("event", Json::from(if revoked { "lead_allowance_revoked" } else { "lead_allowed" })),
                        ("by", Json::from(by.as_str())),
                        ("agent", Json::from(lead.as_str())),
                        ("monitor_event", Json::from(event)),
                    ]),
                ),
            };
            let seq = crate::write_policy::security_event(&self.world, &mut self.swarm, tick, WRITER, record);
            inputguard::note_journal(event, seq);
        }
    }

    /// A request from a frozen session, or during a swarm pause (except the owner's and the lead's
    /// reads and security tools), is refused on the engine thread too: calls already in flight
    /// when the incident came, deferred steps, playtest steps. The window's own human never is.
    pub(crate) fn security_gate(&self, request: &Request) -> Option<ToolOutput> {
        if !inputguard::active() || bookkeeping(&request.command) {
            return None;
        }
        if self.swarm.agent_name(&request.session) == WINDOW_OWNER {
            return None;
        }
        let key = self.swarm.key(&request.session);
        let have = authority::trusted(&self.swarm, &request.session.id);
        let tool: Option<String> = if matches!(request.command, Command::Security(_)) {
            Some("security_clear".into())
        } else {
            (request.call != 0).then(|| crate::trace_sink::tool_of(request.call)).flatten().map(|t| t.to_string())
        };
        let read = tool.as_deref().is_some_and(inputguard::is_read) || crate::call_class(&request.command) == crate::schedule::Class::Read;
        inputguard::refusal(&request.session.id, Some(&key), have, tool.as_deref(), read).map(|e| ToolOutput::error(e.to_json().to_string()))
    }

    /// `logic_input` holding an action the human is using at the window would take their input
    /// over: the owner's goes on (fine), a lead's goes on with an alert, anyone else's is refused
    /// (`human_input`). That refusal is caused by the human's timing, not the agent's choice, so
    /// it is noted, never counted. Whoever holds, the window wins afterwards: the human's own
    /// input on a held action ends the hold (`take_tick`), reported by [`Engine::security_holds`].
    /// `None`: go on.
    pub(crate) fn security_logic_input(&mut self, call: &crate::logic::tools::LogicCall, session: &SessionInfo) -> Option<ToolOutput> {
        let crate::logic::tools::LogicCall::Input { module, actions, ticks } = call else { return None };
        // Releasing a hold (0 ticks) takes nothing from the human: it is never judged, so it
        // can't be used to ask whether they are pressing an action (review R3, minor 3). A hold
        // on an action they are using is still refused: what the human does in the game shows
        // in the world anyway, so that refusal tells an agent nothing the world doesn't.
        if *ticks == 0 {
            return None;
        }
        let using = self.logic.human_using(module, actions);
        if using.is_empty() {
            return None;
        }
        let actor = Actor::of(&self.swarm, session);
        let what = format!("logic_input over the human's {}", using.join(", "));
        if actor.authority() >= Authority::Lead {
            let _seen = inputguard::done(Channel::ModuleInput, "logic_input", &actor, &what);
            return None;
        }
        // (Which of the actions the human is pressing isn't said: an agent polling holds
        // would read the human's input from the refusals.)
        let e = authority::refusal(
            authority::Site::HumanInput,
            "the human is using one of these actions at the window: an agent's hold would take their input over, so it is refused (try again when they stop; docs/design/SECURITY.md)",
        );
        inputguard::declined(Channel::ModuleInput, "logic_input", &actor, e.code, &what);
        Some(ToolOutput::error(e.to_json().to_string()))
    }

    /// Holds the window's human took back this tick (their own input on a held action ends the
    /// hold: the window wins), reported under each hold's holder: never counted (the human chose
    /// it), but on the record.
    pub(crate) fn security_holds(&mut self) {
        for (module, action, by) in self.logic.take_holds_ended() {
            let by = by.unwrap_or(Actor::Engine("restored hold"));
            inputguard::declined(Channel::ModuleInput, "logic_input.hold_ended", &by, "human_input", &format!("{module}: {action}"));
        }
    }

    /// `security_clear` and `security_allow_lead` (see `inputguard_tools`).
    pub(crate) fn security_call(&mut self, call: crate::inputguard_tools::SecurityCall, session: &SessionInfo, live: &[SessionInfo]) -> ToolOutput {
        use crate::inputguard_tools::SecurityCall;
        // The operator's grants only: a lead nobody granted neither clears nor allows.
        let have = authority::trusted(&self.swarm, &session.id);
        let actor = Actor::of(&self.swarm, session);
        let fail = |e: &crate::world::WorldError| ToolOutput::error(e.to_json().to_string());
        let out = match call {
            SecurityCall::Clear { incident, note } => {
                if let Err(e) = authority::require(have, Authority::Lead, authority::Site::Security, "security_clear (unfreezing sessions and lifting a swarm pause)") {
                    return fail(&e);
                }
                match inputguard::clear(incident, &actor, &note) {
                    Ok(c) => Json::obj([
                        ("cleared", Json::arr(c.incidents.iter().map(|i| Json::from(*i)))),
                        ("unfrozen", Json::arr(c.unfrozen.iter().map(|n| Json::from(n.as_str())))),
                        ("swarm_unpaused", Json::Bool(c.unpaused)),
                        ("event", Json::from(c.event)),
                    ]),
                    Err(e) => return fail(&e),
                }
            }
            SecurityCall::AllowLead { lead, revoke } => {
                if let Err(e) = authority::require(have, Authority::Owner, authority::Site::SecurityAllow, "security_allow_lead (only the owner allows a lead to point the keyboard without an alert)") {
                    return fail(&e);
                }
                // The lead's current protocol session(s): a resume in another one asks again.
                let sessions: Vec<String> = live
                    .iter()
                    .filter(|s| self.swarm.agent_name(s) == lead && authority::trusted(&self.swarm, &s.id) >= Authority::Lead)
                    .map(|s| s.id.clone())
                    .take(4)
                    .collect();
                if sessions.is_empty() {
                    return fail(&crate::world::WorldError::new("no_live_lead", format!("no live session is the granted lead '{}' now (allowances are per session: allow it while it is connected)", crate::bounds::echo(&lead))).with("agent", crate::bounds::echo(&lead)));
                }
                let mut events = Vec::new();
                for s in &sessions {
                    events.push(Json::from(inputguard::allow_lead(s, &lead, actor.name(), revoke)));
                }
                Json::obj([("agent", Json::from(lead.as_str())), ("allowed", Json::Bool(!revoke)), ("sessions", Json::from(sessions.len())), ("events", Json::Arr(events))])
            }
        };
        // Journaled now, so the answer can name the records.
        self.security_pump();
        let mut j = out;
        j.set("state", inputguard::state_json());
        ToolOutput::text(j.to_string())
    }

    /// For engine_info's `window.security`: the toast as the last window frame drew it (its event,
    /// box and buttons in window pixels: what a human clicks), and the title it set.
    pub(crate) fn security_window_json(&self) -> Json {
        let t = &self.security.toast;
        let Some(seq) = t.shown else { return Json::Null };
        let rect = |r: &[f32; 4]| Json::arr(r.iter().map(|v| Json::from(f64::from(*v))));
        Json::obj([
            ("event", Json::from(seq)),
            ("incident", t.incident.map_or(Json::Null, Json::from)),
            ("rect", t.rect.as_ref().map_or(Json::Null, rect)),
            ("buttons", Json::arr(t.buttons.iter().map(|(r, b)| Json::obj([("label", Json::from(b.label())), ("rect", rect(r))])))),
            ("title", t.title.as_deref().map_or(Json::Null, Json::from)),
        ])
    }

    /// The window's title while the monitor wants the owner (`None`: nothing to say).
    pub(crate) fn security_title(&self) -> Option<String> {
        if let Some(t) = &self.security.toast.title {
            return Some(t.clone());
        }
        if !inputguard::active() {
            return None;
        }
        Some(match inputguard::paused() {
            Some(i) => format!("SECURITY: swarm paused (incident {i})"),
            None => "SECURITY: a session is frozen".to_owned(),
        })
    }

    /// Before anything reads a window frame's input: presses inside the toast are its own (they
    /// never fall through to what is under it). A button acts when a press and its release both
    /// land on it, and only once the toast has been still for [`TOAST_SETTLE`]; a release is the
    /// toast's only when it took the press (a drag that ends over a toast that just appeared is
    /// still released where it began).
    pub(crate) fn security_toast_input(&mut self) {
        let now = crate::incidents::Stamp::now().time_us;
        if let Some(b) = self.security.toast.take_input(&mut self.input, now) {
            self.security_toast_button(b);
        }
    }

    /// The human pressed a toast button: the owner's own answer.
    fn security_toast_button(&mut self, b: Button) {
        let (Some(seq), incident) = (self.security.toast.shown, self.security.toast.incident) else { return };
        specula_core::trace_event!(&inputguard::SECURITY, Warn, "security.toast_answered", button = b.label(), event = seq);
        match b {
            Button::Clear => {
                if let Some(i) = incident {
                    let _ = inputguard::clear(Some(i), &Actor::Human, "cleared at the window");
                }
            }
            Button::KeepFrozen => {
                if let Some(i) = incident {
                    inputguard::keep_frozen(i);
                }
            }
            Button::AllowLead => {
                if let Some((session, lead)) = inputguard::session_of(seq) {
                    inputguard::allow_lead(&session, &lead, WINDOW_OWNER, false);
                }
            }
            Button::Dismiss => inputguard::dismiss(seq),
        }
        // Gone from the next frame; its clicks don't fall through meanwhile.
        self.security.toast.shown = None;
        self.security.toast.armed = None;
        self.security_pump();
    }

    /// The toast for a window frame (after the engine's screens, before the typing banner).
    pub(crate) fn security_overlay(&mut self, size: (u32, u32), out: &mut Vec<OverlayQuad>) {
        let now = crate::incidents::Stamp::now().time_us;
        let shown = inputguard::toast(now, ALERT_TOAST);
        let t = &mut self.security.toast;
        let Some(inputguard::ToastView { event, repeats, more }) = shown.filter(|_| size.0 > 0 && size.1 > 0) else {
            t.shown = None;
            t.rect = None;
            t.buttons.clear();
            t.incident = None;
            t.title = None;
            t.subject = None;
            t.armed = None;
            return;
        };
        if t.shown != Some(event.seq) {
            specula_core::trace_event!(&inputguard::SECURITY, Info, "security.toast_shown", event = event.seq, class = event.class.name());
        }
        t.shown = Some(event.seq);
        t.incident = event.incident;
        let lead = event.authority == Authority::Lead;
        // An incident's toast never offers "Allow this lead" (a lead is never an incident's
        // offender, and the button there cleared nothing: review R1, minor 10).
        let buttons: Vec<Button> = match event.class {
            Class::Incident => vec![Button::Clear, Button::KeepFrozen],
            _ if lead => vec![Button::AllowLead, Button::Dismiss],
            _ => vec![Button::Dismiss],
        };
        let paused = inputguard::paused();
        let mut lines = toast_lines(&event, repeats, more);
        if let Some(i) = paused.filter(|_| event.halted != Some("swarm_paused")) {
            // Whatever the toast shows, a swarm pause is said on it too.
            lines.push(format!("The whole swarm is paused (ticks stopped) by incident {i}."));
        }
        let who = shown_name(&event.agent);
        t.title = Some(match (event.class, paused) {
            (_, Some(i)) => format!("SECURITY: swarm paused (incident {i}); {who} {}", if event.class == Class::Incident { "frozen" } else { "alert" }),
            (Class::Incident, None) => format!("SECURITY: {who} frozen"),
            _ => format!("SECURITY ALERT: {} by {who}", event.what),
        });
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        let mut quads = Vec::new();
        let before = (t.rect, t.buttons.clone());
        let subject = Some((event.incident, event.agent.clone(), event.channel.name()));
        match t.draw(&mut self.gpu, size, &lines, &buttons, event.class == Class::Incident, &mut quads) {
            Ok(()) if quads.len() <= room => out.extend_from_slice(&quads),
            Ok(()) => {
                // No room left: the screens' quads yield to it (it is the owner's warning).
                let keep = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(quads.len());
                out.truncate(keep);
                out.extend_from_slice(&quads);
            }
            Err(e) => {
                specula_core::trace_event!(&inputguard::SECURITY, Warn, "security.toast_failed", message = e.as_str());
                t.rect = None;
                t.buttons.clear();
            }
        }
        // Anything a click would land on changed: the settle time starts again, and a press
        // already down acts on nothing.
        if t.subject != subject || before != (t.rect, t.buttons.clone()) {
            t.changed_us = now;
            t.armed = None;
        }
        t.subject = subject;
    }
}

/// Characters of an agent's name the toast and the window title show.
const TOAST_NAME_CHARS: usize = 32;

/// An agent's name as the owner's toast and the window title show it: quoted, at most
/// [`TOAST_NAME_CHARS`] of the characters a name may have (anything else is left out: the
/// window's own operator and the engine's writers carry `:`, which stays).
fn shown_name(name: &str) -> String {
    let kept: String = name.chars().filter(|c| authority::name_char(*c) || *c == ':').take(TOAST_NAME_CHARS).collect();
    let more = name.chars().count() > kept.chars().count();
    format!("\"{kept}{}\"", if more { "\u{2026}" } else { "" })
}

/// The toast's words for `e` (it happened `repeats` times; `more` toasts wait behind it). The
/// event's detail can hold text an agent chose (an entity's name): it is shown short, in quotes
/// and labelled, so it can't read as the engine's own words to the owner.
fn toast_lines(e: &inputguard::Event, repeats: u64, more: usize) -> Vec<String> {
    let cut = |s: String| -> String {
        if s.chars().count() > TOAST_LINE_CHARS {
            let mut c: String = s.chars().take(TOAST_LINE_CHARS - 1).collect();
            c.push('\u{2026}');
            c
        } else {
            s
        }
    };
    let head = match e.class {
        Class::Incident => "SECURITY INCIDENT",
        _ => "SECURITY ALERT",
    };
    let mut lines = vec![if more > 0 { format!("{head} (+{more} more)") } else { head.to_owned() }];
    // What happened first, in the engine's own words; then who on a line of their own, by the
    // name they gave: short and in quotes, so a name can't push the rest off the line or read
    // as the engine speaking (review R2 of m5-inputguard, major 5).
    let tried = match &e.refused {
        Some(code) => format!("Refused ({code}): {}", e.what),
        None => format!("Done: {} ({})", e.what, e.channel.name()),
    };
    lines.push(cut(if repeats > 1 { format!("{tried} (x{repeats})") } else { tried }));
    lines.push(cut(format!("By {} ({})", shown_name(&e.agent), e.authority.name())));
    if !e.detail.is_empty() {
        let mut d: String = e.detail.chars().filter(|c| *c != '"').take(TOAST_DETAIL_CHARS).collect();
        if e.detail.chars().count() > TOAST_DETAIL_CHARS {
            d.push('\u{2026}');
        }
        lines.push(format!("Its call named: \"{d}\""));
    }
    lines.push(match e.halted {
        Some("swarm_paused") => "Halted: the whole swarm is paused (ticks stopped); its session is frozen".to_owned(),
        Some(_) => "Halted: its session is frozen (claims and leases released)".to_owned(),
        None => "Nothing was halted.".to_owned(),
    });
    lines
}

impl Toast {
    /// Takes the toast's own events out of a window frame's `input` (the engine clock is at
    /// `now_us`) and says which button, if any, a press and its release both landed on.
    fn take_input(&mut self, input: &mut Vec<Event>, now_us: u64) -> Option<Button> {
        let Some([x0, y0, w, h]) = self.rect else {
            self.took_press = false;
            self.armed = None;
            return None;
        };
        let settled = now_us.saturating_sub(self.changed_us) >= TOAST_SETTLE.as_micros() as u64;
        let buttons = self.buttons.clone();
        let (mut took, mut armed) = (self.took_press, self.armed);
        let inside = |x: i32, y: i32| (x as f32) >= x0 && (x as f32) < x0 + w && (y as f32) >= y0 && (y as f32) < y0 + h;
        let hit = |x: i32, y: i32| buttons.iter().find(|([bx, by, bw, bh], _)| (x as f32) >= *bx && (x as f32) < bx + bw && (y as f32) >= *by && (y as f32) < by + bh).map(|(_, b)| *b);
        let mut act = None;
        input.retain(|e| match e {
            Event::MouseDown { button, x, y } if inside(*x, *y) => {
                took = true;
                armed = if *button == MouseButton::Left && settled { hit(*x, *y) } else { None };
                false
            }
            Event::PointerDown { x, y, .. } if inside(*x, *y) => {
                took = true;
                armed = if settled { hit(*x, *y) } else { None };
                false
            }
            Event::MouseUp { x, y, .. } | Event::PointerUp { x, y, .. } if took => {
                if let Some(b) = armed.take().filter(|b| settled && hit(*x, *y) == Some(*b)) {
                    act = Some(b);
                }
                took = false;
                false
            }
            _ => true,
        });
        self.took_press = took;
        self.armed = armed;
        act
    }
}

const TOAST_BG: [f32; 4] = [0.06, 0.05, 0.06, 1.0];
/// A warning edge (not the fire colour, which is for fire only; not cyan, which marks selection).
const TOAST_EDGE_INCIDENT: [f32; 4] = [0.93, 0.30, 0.33, 1.0];
const TOAST_EDGE_ALERT: [f32; 4] = [0.93, 0.80, 0.35, 1.0];
const TOAST_INK: [f32; 4] = [0.95, 0.93, 0.88, 1.0];
const BUTTON_BG: [f32; 4] = [0.16, 0.15, 0.17, 1.0];

impl Toast {
    /// Draws the toast at the window's bottom right: opaque, so nothing under it mixes with its
    /// words. Records its box and its buttons' for the next frame's clicks.
    fn draw(&mut self, gpu: &mut specula_gpu::Gpu, size: (u32, u32), lines: &[String], buttons: &[Button], incident: bool, out: &mut Vec<OverlayQuad>) -> Result<(), String> {
        let font = &crate::text_world::faces().ui;
        let style = TextStyle::new(TOAST_PX);
        let layouts: Vec<specula_text::TextLayout> = lines.iter().map(|l| specula_text::layout(font, l, &style)).collect();
        let labels: Vec<specula_text::TextLayout> = buttons.iter().map(|b| specula_text::layout(font, b.label(), &style)).collect();
        let line_h = TOAST_PX * 1.4;
        let pad = 12.0;
        let btn_h = line_h + 8.0;
        let btn_w: Vec<f32> = labels.iter().map(|l| l.width.ceil() + 20.0).collect();
        let row_w = btn_w.iter().sum::<f32>() + 8.0 * btn_w.len().saturating_sub(1) as f32;
        let text_w = layouts.iter().map(|l| l.width.ceil()).fold(0.0f32, f32::max);
        let (sw, sh) = (size.0 as f32, size.1 as f32);
        let w = (text_w.max(row_w) + 2.0 * pad).min(sw - 16.0).max(1.0);
        let h = (layouts.len() as f32 * line_h + 8.0 + btn_h + 2.0 * pad).min(sh - 16.0).max(1.0);
        let (x, y) = ((sw - w - 8.0).max(0.0).round(), (sh - h - 8.0).max(0.0).round());
        let atlas = self.atlas.get_or_insert_with(|| GlyphAtlas::new(AtlasConfig { page_size: 512, max_pages: 2, padding: 1, evict: true }));
        atlas.begin_frame();
        let mut glyphs = Vec::new();
        for (i, l) in layouts.iter().enumerate() {
            l.append_quads(font, atlas, [x + pad, y + pad + i as f32 * line_h], TOAST_INK, &mut glyphs).map_err(|e| format!("toast glyphs: {e}"))?;
        }
        let mut rects = Vec::new();
        let mut bx = x + pad;
        let by = y + pad + layouts.len() as f32 * line_h + 8.0;
        for ((b, l), bw) in buttons.iter().zip(&labels).zip(&btn_w) {
            let r = [bx.round(), by.round(), *bw, btn_h];
            l.append_quads(font, atlas, [r[0] + 10.0, r[1] + 4.0], TOAST_INK, &mut glyphs).map_err(|e| format!("toast glyphs: {e}"))?;
            rects.push((r, *b));
            bx += bw + 8.0;
        }
        let page = atlas.page_size();
        let fresh = self.pages.len();
        while self.pages.len() < atlas.page_count() {
            let pixels = atlas.page_pixels(self.pages.len()).unwrap_or(&[]);
            self.pages.push(gpu.upload_texture(page, page, TextureFormat::R8, pixels).map_err(|e| format!("toast atlas: {e}"))?);
        }
        for r in atlas.take_dirty() {
            let p = r.page as usize;
            if p >= fresh || r.width == 0 || r.height == 0 {
                continue;
            }
            atlas.copy_rect(&r, &mut self.scratch);
            gpu.update_texture(self.pages[p], r.x, r.y, r.width, r.height, &self.scratch).map_err(|e| format!("toast atlas: {e}"))?;
        }
        let edge = if incident { TOAST_EDGE_INCIDENT } else { TOAST_EDGE_ALERT };
        out.push(OverlayQuad { radius: 6.0, border: 3.0, border_color: [edge; 2], ..OverlayQuad::solid([x, y, w, h], TOAST_BG) });
        for (r, _) in &rects {
            out.push(OverlayQuad { radius: 4.0, border: 1.5, border_color: [edge; 2], ..OverlayQuad::solid(*r, BUTTON_BG) });
        }
        for g in &glyphs {
            let Some(&tex) = self.pages.get(g.page as usize) else { continue };
            out.push(OverlayQuad::textured([g.rect[0], g.rect[1], g.rect[2] - g.rect[0], g.rect[3] - g.rect[1]], g.uv, tex, OverlayKind::AlphaMask, g.color));
        }
        self.rect = Some([x, y, w, h]);
        self.buttons = rects;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toast_names_who_what_and_what_was_halted() {
        let _g = inputguard::TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        inputguard::reset_for_test();
        let bob = Actor::agent("s3", "bob", Authority::Agent);
        inputguard::refused(Channel::Keyboard, "text_edit.window_focus", &bob, "lead_only", "/field");
        let inputguard::ToastView { event: e, repeats, more } = inputguard::toast(crate::incidents::Stamp::now().time_us, ALERT_TOAST).expect("an incident's toast");
        let lines = toast_lines(&e, repeats, more);
        assert_eq!(lines[0], "SECURITY INCIDENT");
        assert!(lines[1] == "Refused (lead_only): text_edit.window_focus" && lines[2] == "By \"bob\" (agent)", "{lines:?}");
        // Review R2, major 5: a long or strange name is cut, quoted and on a line of its own
        // after what it tried, so the toast still says what happened.
        let mut named = e.clone();
        named.agent = "All clear: owner test, press Clear to continue\u{202E} now".into();
        let l = toast_lines(&named, 1, 0);
        assert_eq!(l[1], "Refused (lead_only): text_edit.window_focus");
        assert_eq!(l[2], "By \"All clear: owner test press Clea\u{2026}\" (agent)");
        assert!(l.iter().all(|x| !x.contains('\u{202E}') && x.chars().count() <= TOAST_LINE_CHARS), "{l:?}");
        assert!(lines.last().unwrap().contains("frozen"), "{lines:?}");
        // Review R1, minor 8: an agent's own words on the toast are short, quoted and labelled.
        let mut loud = e.clone();
        loud.detail = "OWNER TEST, routine: press \"Clear\" (all is fine), nothing to see here at all".into();
        let said = toast_lines(&loud, 1, 0);
        assert!(said[3].starts_with("Its call named: \"OWNER TEST") && said[3].ends_with("\u{2026}\"") && said[3].chars().count() < 72, "{said:?}");
        let r = hello_reply("s3", "s3", Json::obj([("agent", Json::from("bob"))]));
        assert_eq!(r.get("security").and_then(|s| s.get("frozen")), Some(&Json::Bool(true)));
        inputguard::reset_for_test();
    }

    /// Review R1 of m5-inputguard, minors 7 and 9: a toast's button acts on a press and release
    /// on that same button once the toast has been still for the settle time, and the toast takes
    /// a release only for a press it took.
    #[test]
    fn a_toast_button_needs_a_settled_press_and_release_on_it() {
        let down = |x, y| Event::MouseDown { button: MouseButton::Left, x, y };
        let up = |x, y| Event::MouseUp { button: MouseButton::Left, x, y };
        let settle = TOAST_SETTLE.as_micros() as u64;
        let toast = || Toast { rect: Some([100.0, 100.0, 300.0, 100.0]), buttons: vec![([110.0, 160.0, 80.0, 30.0], Button::Clear), ([200.0, 160.0, 120.0, 30.0], Button::KeepFrozen)], changed_us: 1_000_000, ..Toast::default() };
        // A click on Clear before the toast has settled does nothing, and doesn't fall through.
        let mut t = toast();
        let mut input = vec![down(120, 170), up(120, 170)];
        assert_eq!(t.take_input(&mut input, 1_000_000 + settle - 1), None, "not settled");
        assert!(input.is_empty(), "a press inside the toast is the toast's");
        // Settled: press and release on the same button act.
        let mut input = vec![down(120, 170), up(120, 170)];
        assert_eq!(t.take_input(&mut input, 1_000_000 + settle), Some(Button::Clear));
        // Across frames too.
        let mut input = vec![down(210, 170)];
        assert_eq!(t.take_input(&mut input, 2_000_000), None, "a press alone acts on nothing");
        let mut input = vec![up(210, 170)];
        assert_eq!(t.take_input(&mut input, 2_100_000), Some(Button::KeepFrozen));
        // Pressed on one button and released on another, or outside: nothing.
        let mut input = vec![down(120, 170), up(210, 170)];
        assert_eq!(t.take_input(&mut input, 3_000_000), None);
        let mut input = vec![down(120, 170), up(5, 5)];
        assert_eq!(t.take_input(&mut input, 3_000_000), None);
        assert!(input.is_empty(), "the release of a press the toast took is the toast's, wherever it lands");
        // A drag that began outside and ends over the toast is released where it began: the
        // release goes on to whoever took the press.
        let mut input = vec![up(120, 170)];
        assert_eq!(t.take_input(&mut input, 3_000_000), None);
        assert_eq!(input.len(), 1, "a release the toast has no press for goes on");
        // The toast changed while the press was down (the overlay disarms it): the release acts on nothing.
        let mut input = vec![down(120, 170)];
        assert_eq!(t.take_input(&mut input, 4_000_000), None);
        t.armed = None;
        t.changed_us = 4_050_000;
        let mut input = vec![up(120, 170)];
        assert_eq!(t.take_input(&mut input, 4_100_000), None);
        // A right click never presses a button; presses outside go on.
        let mut input = vec![Event::MouseDown { button: MouseButton::Right, x: 120, y: 170 }, up(120, 170), down(5, 5)];
        assert_eq!(t.take_input(&mut input, 9_000_000), None);
        assert_eq!(input.len(), 1);
        // No toast: nothing is taken.
        let mut none = Toast::default();
        let mut input = vec![down(120, 170), up(120, 170)];
        assert_eq!(none.take_input(&mut input, 9_000_000), None);
        assert_eq!(input.len(), 2);
    }

    #[test]
    fn bookkeeping_is_never_refused() {
        assert!(bookkeeping(&Command::StudioDone(1)));
        assert!(bookkeeping(&Command::Hello { name: "x".into(), role: "builder".into(), resume: None }));
        assert!(!bookkeeping(&Command::Info));
    }
}

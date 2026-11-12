use super::*;

fn agent(session: &str, name: &str, a: Authority) -> Actor {
    Actor::agent(session, name, a)
}

/// `wait_events` as (events, next cursor, more).
fn page_of(after: u64, all: bool, limit: usize, wait: Duration) -> (Vec<Json>, u64, bool) {
    let p = wait_events(after, all, limit, wait);
    (p.events, p.next, p.more)
}

fn lock() -> MutexGuard<'static, ()> {
    let g = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    reset_for_test();
    g
}

fn gate_ok(session: &str, tool: &str) -> bool {
    gate_ok_as(session, tool, Authority::Agent)
}

fn gate_ok_as(session: &str, tool: &str, a: Authority) -> bool {
    let key = key_of(session).unwrap_or_else(|| session.to_owned());
    refusal(session, Some(&key), a, Some(tool), is_read(tool)).is_none()
}

fn classes_of(what: &str) -> Vec<String> {
    let (ev, _, _) = page_of(0, true, MAX_LIMIT, Duration::ZERO);
    ev.iter().filter(|e| e.get("what").and_then(Json::as_str) == Some(what)).map(|e| e.get("class").and_then(Json::as_str).unwrap().to_owned()).collect()
}

fn now() -> u64 {
    crate::incidents::Stamp::now().time_us
}

#[test]
fn the_owner_is_fine_a_lead_alerts_and_everyone_else_is_an_incident() {
    let _g = lock();
    let owner = agent("s0", "boss", Authority::Owner);
    let lead = agent("s1", "lee", Authority::Lead);
    let rev = agent("s2", "rev", Authority::Reviewer);
    let bob = agent("s3", "bob", Authority::Agent);
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &Actor::Human, "");
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &owner, "");
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &lead, "");
    let _ = done(Channel::Screen, "review_screen", &rev, "");
    let (ev, _, _) = page_of(0, true, 50, Duration::ZERO);
    let classes: Vec<&str> = ev.iter().map(|e| e.get("class").and_then(Json::as_str).unwrap()).collect();
    assert_eq!(classes, ["fine", "fine", "alert", "alert"]);
    assert!(gate_ok("s3", "world_set"), "nothing is frozen yet");
    refused(Channel::Keyboard, "text_edit.window_focus", &bob, "lead_only", "");
    assert!(!gate_ok("s3", "world_set"), "bob's session is frozen");
    assert!(gate_ok("s1", "world_set"), "one agent: only its session freezes");
    assert_eq!(paused(), None);
    // A reviewer's window input other than a screen is an incident.
    let _ = done(Channel::EditorInput, "editor_input", &rev, "");
    assert!(paused().is_some(), "a second identity's incident pauses the swarm");
    assert!(!gate_ok("s1", "world_set"), "even the lead's writes wait");
    assert!(gate_ok_as("s1", "world_query", Authority::Lead), "the lead reads");
    assert!(gate_ok_as("s1", "security_clear", Authority::Lead), "the lead's security tools");
    assert!(!gate_ok_as("s4", "world_query", Authority::Agent), "a plain agent reads nothing");
    let pend = take_pending();
    assert!(matches!(pend.as_slice(), [Pending::Freeze { name, .. }, Pending::Freeze { .. }, Pending::Pause { .. }] if name == "bob"), "{pend:?}");
    let c = clear(None, &lead, "seen").unwrap();
    assert!(c.unpaused && c.incidents.len() == 2);
    assert!(gate_ok("s3", "world_set") && paused().is_none() && !active());
}

/// Review R1 of m5-inputguard, blocker 1 (probe.py b): a lead-only swarm (no --owner) must
/// never lock itself out. The lead's refused tries (editor_input, ui_act on the review screen)
/// are alerts, never freezes; its security_clear always answers; a later builder's incident is
/// clearable by it.
#[test]
fn a_lead_only_swarm_never_locks_itself_out() {
    let _g = lock();
    let lee = agent("lee-1", "lee", Authority::Lead);
    refused(Channel::EditorInput, "editor_input", &lee, "owner_only", "1 frames");
    refused(Channel::ScreenInput, "ui_act", &lee, "owner_only", "review");
    assert_eq!(classes_of("editor_input"), ["alert"]);
    assert_eq!(classes_of("ui_act"), ["alert"]);
    assert!(frozen("lee-1", Some("lee-1")).is_none() && !active(), "a lead is never frozen");
    assert!(gate_ok_as("lee-1", "security_clear", Authority::Lead) && gate_ok_as("lee-1", "world_set", Authority::Lead));
    // Two builders offend: the swarm pauses, and the lead clears it.
    refused(Channel::Keyboard, "text_edit.window_focus", &agent("x-1", "x", Authority::Agent), "lead_only", "");
    refused(Channel::Keyboard, "text_edit.window_focus", &agent("y-1", "y", Authority::Agent), "lead_only", "");
    assert!(paused().is_some());
    assert!(gate_ok_as("lee-1", "security_clear", Authority::Lead) && gate_ok_as("lee-1", "world_query", Authority::Lead));
    let c = clear(None, &lee, "").unwrap();
    assert!(c.unpaused && c.incidents.len() == 2 && !active());
    // Even a frozen session that is a granted lead (it can't become one by offending, but the
    // check holds whatever got it there) reaches its security tools.
    let mut s = state();
    s.frozen_sessions.insert("lee-1".into(), 99);
    s.refresh_flags();
    drop(s);
    assert!(gate_ok_as("lee-1", "security_clear", Authority::Lead), "security_* for a lead is never refused");
    assert!(gate_ok_as("lee-1", "security_incidents", Authority::Owner));
    assert!(!gate_ok_as("lee-1", "security_clear", Authority::Agent), "a plain agent's security_* is the authority check's (refused here: frozen)");
    reset_for_test();
}

/// Review R1, major 2: refusals caused by state the agent didn't choose (the human typing, an
/// engine screen open when the call named no surface, a hold the human took back) are noted,
/// never counted: nobody freezes, the swarm never pauses.
#[test]
fn declined_refusals_are_never_counted() {
    let _g = lock();
    for (i, n) in ["bob", "carl", "dan"].iter().enumerate() {
        let a = agent(&format!("s{i}"), n, Authority::Agent);
        declined(Channel::ScreenInput, "ui_act", &a, "engine_screen_open", "review (no surface named)");
        declined(Channel::Screen, "review_screen", &a, "window_typing", "the human is typing");
        declined(Channel::ModuleInput, "logic_input", &a, "human_input", "jump");
    }
    declined(Channel::Screen, "review_screen", &agent("r", "rev", Authority::Reviewer), "window_typing", "");
    assert!(!active() && paused().is_none(), "nothing frozen, no pause");
    assert!(classes_of("ui_act").iter().all(|c| c == "noted"));
    let (ev, _, _) = page_of(0, false, 50, Duration::ZERO);
    assert!(ev.is_empty(), "the lead isn't woken by them");
    assert!(toast(now(), Duration::from_secs(60)).is_none(), "the owner isn't bothered by them");
}

/// Review R1, major 3 (probe.py P3): an agent that takes another's name can't get it frozen.
/// Freezes hold the identity key (what a resume carries) and the session, not the name.
#[test]
fn taking_a_name_freezes_only_the_taker() {
    let _g = lock();
    // vic says hello (identity key = its first session), then goes offline.
    assert_eq!(hello("vic-1", "vic-1"), None);
    // Mallory says hello as "vic" in her own session: a new identity.
    assert_eq!(hello("mal-1", "mal-1"), None);
    let mallory = Actor::keyed("mal-1", "mal-1", "vic", Authority::Agent);
    refused(Channel::Keyboard, "text_edit.window_focus", &mallory, "lead_only", "");
    // The real vic resumes in a new session with its token: its key, not frozen.
    assert_eq!(hello("vic-2", "vic-1"), None, "the real vic's resume is not frozen");
    assert!(gate_ok("vic-2", "world_set"));
    // Mallory's own resume (her key in a new session) is.
    let id = hello("mal-2", "mal-1").expect("mallory's resume is frozen");
    assert!(!gate_ok("mal-2", "world_set"));
    assert_eq!(frozen("mal-2", None), Some(id), "and stays frozen by session");
    // A freeze stays with the session whatever identity it says hello as afterwards.
    assert_eq!(hello("mal-2", "zed"), Some(id));
    assert!(!gate_ok("mal-2", "world_set"));
    assert!(paused().is_none(), "one identity, however many names and sessions: no pause");
    // The owner may keep it frozen; then the lead's clear is refused.
    assert!(keep_frozen(id));
    let e = clear(Some(id), &agent("l", "lee", Authority::Lead), "").unwrap_err();
    assert_eq!(e.code, "owner_kept");
    clear(Some(id), &Actor::Human, "").unwrap();
    assert!(frozen("mal-2", Some("mal-1")).is_none() && frozen("mal-1", None).is_none());
    assert_eq!(clear(Some(id), &Actor::Human, "").unwrap_err().code, "incident_not_open");
}

/// Review R1, minor 6 (probe.py P4): a frozen identity resuming session after session never
/// grows the monitor without bound: past MAX_FROZEN sessions the swarm pauses (fails closed).
#[test]
fn a_flood_of_resumes_pauses_rather_than_grows() {
    let _g = lock();
    let bob = Actor::keyed("b-0", "bob-key", "bob", Authority::Agent);
    refused(Channel::Keyboard, "x", &bob, "lead_only", "");
    // Sessions saying hello under the *name* alone are new identities: nothing grows.
    for i in 0..(MAX_FROZEN + 50) {
        assert_eq!(hello(&format!("named-{i}"), &format!("named-{i}")), None);
    }
    assert!(paused().is_none() && state().frozen_sessions.len() == 1);
    // Resumes of the frozen identity each freeze a session, up to the limit, then pause.
    for i in 0..MAX_FROZEN {
        assert!(hello(&format!("r-{i}"), "bob-key").is_some());
    }
    assert!(paused().is_some(), "past MAX_FROZEN sessions the swarm pauses");
    assert!(frozen("b-0", None).is_some() && frozen("r-0", None).is_some(), "nothing was forgotten");
    // More resumes are still told they are frozen (the identity is), and nothing more is kept.
    for i in 0..500 {
        assert!(hello(&format!("more-{i}"), "bob-key").is_some());
        assert!(frozen(&format!("more-{i}"), Some("bob-key")).is_some(), "the identity's freeze covers it");
    }
    assert!(state().frozen_sessions.len() <= MAX_FROZEN + 1, "{} sessions kept", state().frozen_sessions.len());
    assert!(state().bound.len() <= MAX_BOUND, "bindings are bounded");
    clear(None, &Actor::Human, "").unwrap();
    assert!(!active());
}

/// One offender opening session after session (each offending) never makes the monitor forget a
/// freeze: past MAX_FROZEN sessions the swarm pauses.
#[test]
fn a_flood_of_sessions_pauses_rather_than_forgets() {
    let _g = lock();
    for i in 0..MAX_FROZEN {
        refused(Channel::Keyboard, "x", &Actor::keyed(&format!("flood-{i}"), "bob-key", "bob", Authority::Agent), "lead_only", "");
    }
    assert!(paused().is_none(), "one identity, however many sessions: not yet a pause");
    refused(Channel::Keyboard, "x", &Actor::keyed("flood-last", "bob-key", "bob", Authority::Agent), "lead_only", "");
    assert!(paused().is_some());
    assert!(frozen("flood-0", None).is_some(), "nothing was forgotten");
    clear(None, &Actor::Human, "").unwrap();
}

/// Review R1, major 4 (probe_toast.py): alerts never push an open incident out of the owner's
/// toast. Incidents show first and are never dropped; alerts merge per identity and channel.
#[test]
fn alerts_never_push_an_incident_out_of_the_toast() {
    let _g = lock();
    let bob = agent("s3", "bob", Authority::Agent);
    refused(Channel::Keyboard, "text_edit.window_focus", &bob, "lead_only", "/field");
    let lead = agent("s1", "lee", Authority::Lead);
    for _ in 0..20 {
        let _ = done(Channel::Keyboard, "text_edit.window_focus", &lead, "/field");
    }
    for _ in 0..5 {
        let _ = done(Channel::Clipboard, "paste", &lead, "/field");
    }
    let t = toast(now(), Duration::from_secs(60)).expect("a toast");
    assert_eq!(t.event.class, Class::Incident, "the incident shows first");
    assert_eq!(t.event.agent, "bob");
    assert_eq!(t.more, 2, "the lead's 25 alerts merged into 2 (keyboard, clipboard)");
    // Many distinct leads' alerts: the alert queue drops its oldest, the incident stays.
    for i in 0..(MAX_ALERT_TOASTS + 10) {
        let _ = done(Channel::Screen, "gallery_open", &agent(&format!("l{i}"), &format!("lead{i}"), Authority::Lead), "");
    }
    let t = toast(now(), Duration::from_secs(60)).expect("a toast");
    assert_eq!((t.event.class, t.event.agent.as_str()), (Class::Incident, "bob"));
    assert_eq!(t.more, MAX_ALERT_TOASTS);
    // And it outlives the log: 600 noted events later it is still shown.
    for _ in 0..(MAX_LOG + 100) {
        note(Channel::Keyboard, "focus_dropped", "");
    }
    let t = toast(now(), Duration::from_secs(60)).expect("a toast");
    assert_eq!(t.event.agent, "bob");
    // Alerts expire by time; the incident doesn't.
    let t = toast(now() + 120_000_000, Duration::from_secs(60)).expect("the incident");
    assert_eq!((t.event.class, t.more), (Class::Incident, 0));
    clear(None, &Actor::Human, "").unwrap();
    assert!(toast(now() + 120_000_000, Duration::from_secs(60)).is_none());
}

#[test]
fn allowing_a_lead_answers_its_alerts_for_that_session_only() {
    let _g = lock();
    let lead = agent("s1", "lee", Authority::Lead);
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &lead, "");
    let t = toast(now(), Duration::from_secs(20)).expect("an alert's toast");
    assert_eq!(t.event.class, Class::Alert);
    assert_eq!(session_of(t.event.seq), Some(("s1".into(), "lee".into())));
    allow_lead("s1", "lee", WINDOW_OWNER, false);
    assert!(toast(now(), Duration::from_secs(20)).is_none(), "answered");
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &lead, "");
    let other = agent("s7", "lee", Authority::Lead);
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &other, "");
    assert_eq!(classes_of("text_edit.window_focus"), ["alert", "allowed", "alert"], "another session of the same lead asks again");
    // An allowed lead's refused try is still an alert (it tried something beyond a lead).
    refused(Channel::EditorInput, "editor_input", &lead, "owner_only", "");
    assert_eq!(classes_of("editor_input"), ["alert"]);
    assert!(take_pending().iter().any(|p| matches!(p, Pending::Allowed { lead, revoked: false, .. } if lead == "lee")));
}

#[test]
fn the_lead_wakes_on_each_event_and_the_log_is_bounded() {
    let _g = lock();
    let t = std::thread::spawn(|| page_of(0, false, 10, Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(50));
    let lead = agent("s1", "lee", Authority::Lead);
    let _ = done(Channel::Screen, "gallery_open", &lead, "");
    let (ev, next, _) = t.join().unwrap();
    assert_eq!(ev.len(), 1);
    let (none, same, _) = page_of(next, false, 10, Duration::from_millis(20));
    assert!(none.is_empty() && same == next, "nothing new: the cursor stays");
    for _ in 0..(MAX_LOG + 20) {
        note(Channel::Keyboard, "focus_dropped", "");
    }
    assert!(state().log.len() <= MAX_LOG);
    let (ev, _, more) = page_of(0, true, MAX_LIMIT, Duration::ZERO);
    assert!(ev.len() == MAX_LIMIT && more);
    // Details are cut, control characters dropped.
    let long = "x\u{7}".repeat(1000);
    let _ = done(Channel::Screen, &long, &lead, &long);
    let last = state().log.back().cloned().unwrap();
    assert!(last.what.chars().count() <= MAX_DETAIL && !last.detail.contains('\u{7}'));
}

/// Review R2 of m5-inputguard, blocker 1: the monitor trusts the operator's grants only. A lead
/// nobody granted (then: the first agent to say lead; since 2026-11-12 there is none, and
/// `trust` is the second lock) is a plain agent to it: its refused try is an incident, it is
/// frozen, and its `security_*` calls are refused like anyone's.
#[test]
fn a_lead_nobody_granted_is_a_plain_agent_to_the_monitor() {
    use crate::authority::trust;
    let _g = lock();
    assert_eq!(trust(Authority::Lead, false), Authority::Agent, "a lead nobody granted");
    assert_eq!(trust(Authority::Lead, true), Authority::Lead);
    assert_eq!(trust(Authority::Owner, false), Authority::Owner);
    assert_eq!(trust(Authority::Reviewer, false), Authority::Reviewer, "reviewers come from grants only");
    // What a call site builds for a lead without a grant: trust() of what it said.
    let first = Actor::keyed("s-first", "first-key", "first", trust(Authority::Lead, false));
    refused(Channel::EditorInput, "editor_input", &first, "owner_only", "");
    assert!(frozen("s-first", Some("first-key")).is_some(), "its refused try is an incident");
    for tool in ["security_clear", "security_incidents", "security_allow_lead", "world_query"] {
        assert!(refusal("s-first", Some("first-key"), trust(Authority::Lead, false), Some(tool), tool == "world_query").is_some(), "{tool} from a frozen fallback lead");
    }
    // A granted lead's same try is an alert, and its security tools answer whatever is frozen.
    let lee = Actor::keyed("s-lee", "lee-key", "lee", trust(Authority::Lead, true));
    refused(Channel::EditorInput, "editor_input", &lee, "owner_only", "");
    assert!(frozen("s-lee", Some("lee-key")).is_none());
    assert!(refusal("s-lee", Some("lee-key"), Authority::Lead, Some("security_clear"), false).is_none());
    clear(None, &Actor::Human, "").unwrap();
}

/// Review R2, minor 7: events nobody counts (noted, fine) have a log of their own, so they can't
/// push an alert out of the lead's feed; and when the feed itself drops events a reader hasn't
/// read, its answer says so.
#[test]
fn uncounted_events_never_push_the_feed_out_and_gaps_are_told() {
    let _g = lock();
    let lee = agent("s-lee", "lee", Authority::Lead);
    let _ = done(Channel::Keyboard, "text_edit.window_focus", &lee, "/field");
    let bob = agent("s-bob", "bob", Authority::Agent);
    for _ in 0..(MAX_LOG + MAX_QUIET + 50) {
        declined(Channel::ScreenInput, "ui_act", &bob, "engine_screen_open", "review");
    }
    let p = wait_events(0, false, 10, Duration::ZERO);
    assert_eq!(p.events.len(), 1, "the lead's alert is still in the feed");
    assert!(!p.missed);
    let kept = state().quiet.len();
    assert!(kept <= MAX_QUIET && frozen("s-bob", None).is_none());
    // With `all`, both logs in order; the quiet log dropped events of its own, and says so.
    let all = wait_events(0, true, MAX_LIMIT, Duration::ZERO);
    let seqs: Vec<u64> = all.events.iter().filter_map(|e| e.get("seq").and_then(Json::as_u64)).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]) && seqs.first() == Some(&1), "{seqs:?}");
    assert!(all.missed, "an `all` reader is told the uncounted events it missed");
    // (The cursor is read first: `state()` holds the monitor's lock for its whole statement.)
    let recent = state().seq - 5;
    assert!(!wait_events(recent, true, MAX_LIMIT, Duration::ZERO).missed);
    // The feed keeps its newest MAX_LOG: a reader behind what it dropped is told.
    for i in 0..(MAX_LOG + 5) {
        let _ = done(Channel::Keyboard, "text_edit.window_focus", &agent(&format!("s-l{i}"), "lee", Authority::Lead), "");
    }
    assert!(wait_events(0, false, 10, Duration::ZERO).missed, "the first alert was dropped unread");
    let newest = state().seq;
    assert!(!wait_events(newest - 3, false, 10, Duration::ZERO).missed);
    clear(None, &Actor::Human, "").unwrap();
    reset_for_test();
}

/// Review R2, major 2: a waiting `security_incidents` ends at once when the engine stops (a 60 s
/// wait held the shutdown for its whole length), and only so many calls wait at a time.
#[test]
fn waits_end_when_the_engine_stops_and_are_bounded() {
    let _g = lock();
    let t0 = std::time::Instant::now();
    let waiters: Vec<_> = (0..3).map(|_| std::thread::spawn(|| wait_events(0, false, 10, Duration::from_secs(60)))).collect();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(state().waiters, 3);
    stopping();
    for w in waiters {
        assert!(w.join().unwrap().events.is_empty());
    }
    assert!(t0.elapsed() < Duration::from_secs(5), "answered in {:?}", t0.elapsed());
    assert!(wait_events(0, false, 10, Duration::from_secs(60)).events.is_empty() && t0.elapsed() < Duration::from_secs(5), "and no new wait begins while stopping");
    reset_for_test();
    // Past MAX_WAITERS a call answers at once.
    state().waiters = MAX_WAITERS;
    let t1 = std::time::Instant::now();
    let _ = wait_events(0, false, 10, Duration::from_secs(60));
    assert!(t1.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(2)));
    reset_for_test();
}

#[test]
fn sessions_and_identity_keys_are_never_shown() {
    let _g = lock();
    let bob = Actor::keyed("secret-session-id", "secret-identity-key", "bob", Authority::Agent);
    refused(Channel::Keyboard, "x", &bob, "lead_only", "");
    let (ev, _, _) = page_of(0, true, 10, Duration::ZERO);
    let shown = format!("{}{}", Json::Arr(ev), state_json());
    assert!(!shown.contains("secret-session-id") && !shown.contains("secret-identity-key"), "{shown}");
    let inc = crate::incidents::get(take_incident()).expect("the trace incident");
    assert!(!inc.detail_json().to_string().contains("secret-"), "nor in the trace incident");
    clear(None, &Actor::Human, "").unwrap();
}

fn take_incident() -> u64 {
    state().open.keys().next().copied().expect("an open incident")
}

/// The module lane's seam: a refused native-code try by an identity (no session at hand)
/// freezes the identity, and so whichever session carries it.
#[test]
fn a_refused_module_load_freezes_the_identity() {
    let _g = lock();
    assert_eq!(hello("m-1", "mod-key"), None);
    refused_identity(Channel::NativeCode, "logic_reload", "modder", "mod-key", Authority::Agent, "lead_only", "walker");
    assert!(!gate_ok("m-1", "world_set"), "its session is frozen through its identity");
    assert!(matches!(take_pending().as_slice(), [Pending::Freeze { key, .. }] if key == "mod-key"));
    // A granted lead's refused load is an alert.
    refused_identity(Channel::NativeCode, "logic_build", "lee", "lee-key", Authority::Lead, "lead_only", "");
    assert_eq!(classes_of("logic_build"), ["alert"]);
    assert!(identity_frozen("mod-key") && !identity_frozen("lee-key"));
    clear(None, &Actor::Human, "").unwrap();
}

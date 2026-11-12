//! The input monitor's tools (family `security`; docs/design/SECURITY.md):
//! `security_incidents` (the lead's and the owner's: what the monitor saw, waiting for the next
//! thing), `security_clear` (the lead's and the owner's: unfreeze, lift a swarm pause) and
//! `security_allow_lead` (the owner's alone). `security_incidents` is answered on the caller's
//! connection thread (it waits); the other two by the engine.

use crate::authority::{self, Authority};
use crate::inputguard;
use crate::tools::{argument_error, check_keys};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::time::Duration;

/// A note on a clear, at most (characters).
pub const MAX_NOTE: usize = 512;

#[derive(Debug)]
pub enum SecurityCall {
    /// `incident: None`: every open one.
    Clear { incident: Option<u64>, note: String },
    AllowLead { lead: String, revoke: bool },
}

fn u64_arg(args: &Json, key: &str, max: u64) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n <= max => Ok(Some(n)),
            _ => Err(format!("'{key}' must be a whole number 0..{max}")),
        },
    }
}

fn flag(args: &Json, key: &str) -> Result<bool, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(false),
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("'{key}' must be true or false")),
    }
}

/// `security_clear` and `security_allow_lead` for the engine (`None`: not one of them).
pub fn parse(name: &str, args: &Json) -> Option<Result<SecurityCall, String>> {
    Some(match name {
        "security_clear" => (|| {
            check_keys(args, name, &[&["incident", "all", "note"]])?;
            let incident = u64_arg(args, "incident", u64::MAX >> 11)?;
            let all = flag(args, "all")?;
            if incident.is_some() == all {
                return Err("give 'incident' (one, from security_incidents) or 'all': true, not both".into());
            }
            let note = match args.get("note") {
                None | Some(Json::Null) => String::new(),
                Some(Json::Str(s)) if s.chars().count() <= MAX_NOTE => s.clone(),
                Some(_) => return Err(format!("'note' must be a string of at most {MAX_NOTE} characters")),
            };
            Ok(SecurityCall::Clear { incident, note })
        })(),
        "security_allow_lead" => (|| {
            check_keys(args, name, &[&["agent", "revoke"]])?;
            let lead = crate::tools::name_arg(args.get("agent"))?.ok_or("'agent' is required: the granted lead's agent name")?;
            Ok(SecurityCall::AllowLead { lead, revoke: flag(args, "revoke")? })
        })(),
        _ => return None,
    })
}

/// `security_incidents`, on the caller's connection thread: what the monitor saw after `after`,
/// waiting up to `wait_s` for something new (the lead's session polls it).
pub fn incidents(session: &SessionInfo, args: &Json) -> ToolOutput {
    let parsed = (|| -> Result<(u64, u64, usize, bool), String> {
        check_keys(args, "security_incidents", &[&["after", "wait_s", "limit", "all"]])?;
        let after = u64_arg(args, "after", u64::MAX >> 11)?.unwrap_or(0);
        let wait = u64_arg(args, "wait_s", inputguard::MAX_WAIT_S)?.unwrap_or(0);
        let limit = match u64_arg(args, "limit", inputguard::MAX_LIMIT as u64)? {
            None => 20,
            Some(0) => return Err(format!("'limit' must be 1..{}", inputguard::MAX_LIMIT)),
            Some(n) => n as usize,
        };
        Ok((after, wait, limit, flag(args, "all")?))
    })();
    let (after, wait, limit, all) = match parsed {
        Ok(p) => p,
        Err(m) => return ToolOutput::error(argument_error(&m).to_string()),
    };
    if let Err(e) = authority::require(authority::trusted_of_session(&session.id), Authority::Lead, authority::Site::Security, "security_incidents (the input monitor's log: the owner or an operator-granted lead)") {
        return ToolOutput::error(e.to_json().to_string());
    }
    let page = inputguard::wait_events(after, all, limit, Duration::from_secs(wait));
    let mut j = Json::obj([("events", Json::Arr(page.events)), ("next_after", Json::from(page.next)), ("state", inputguard::state_json())]);
    if page.more {
        j.set("more", Json::Bool(true));
    }
    if page.missed {
        j.set("missed", Json::Bool(true));
        j.set("missed_note", Json::from("events after your cursor were dropped before you read them (the feed keeps the newest 512). Freezes, pauses, clears and allowances are in world_history {kinds: [\"security\"]}, incidents in trace_incidents, alerts in the trace (trace_query {code: \"security.alert\"})"));
    }
    ToolOutput::text(j.to_string())
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(Vec::<Json>::new()))]);
    let int = |d: &str, max: u64| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(0)), ("maximum", Json::from(max)), ("description", Json::from(d))]);
    let boolean = |d: &str| Json::obj([("type", Json::from("boolean")), ("description", Json::from(d))]);
    let string = |d: &str, max: usize| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(max)), ("description", Json::from(d))]);
    vec![
        ToolDef {
            name: "security_incidents".into(),
            description: "The input monitor's log (docs/design/SECURITY.md), for the lead and the owner: every change of the window's \
                          keyboard focus, every system-clipboard access, every engine screen put on the window, every simulated input \
                          acting as the window's human, and every refused try at them. Each event: seq, class (fine: the owner's own; \
                          allowed: a lead the owner allowed; alert: a granted lead, or a reviewer's screen; incident: a refused try, or \
                          input taken by anyone else; noted: refused for state the agent didn't choose, never counted), channel \
                          (keyboard, clipboard, screen, editor_input, screen_input, module_input, native_code: a refused try at loading \
                          or unloading module code; monitor: clears and allowances), \
                          what, agent, authority, outcome (done or refused, with code), halted (session_frozen or swarm_paused), \
                          incident (the trace_incidents id, code security.incident), trace_event and journal (kind security records). \
                          state: frozen agents, open incidents, the swarm pause, allowed leads. after: the cursor (next_after of the \
                          last answer); wait_s 0..60 blocks until something new arrives (poll it while you work; it answers at \
                          once when the engine stops); limit 1..100 (default 20); all: also the fine and noted events (the human's \
                          own F8, focus drops, uncounted refusals). missed: true when events after your cursor were dropped before \
                          you read them. Who: the owner or an operator-granted lead (--lead NAME)."
                .into(),
            input_schema: schema(vec![
                ("after", int("Events after this seq (next_after of the last answer)", u64::MAX >> 11)),
                ("wait_s", int("Wait up to this many seconds for something new (0..60)", inputguard::MAX_WAIT_S)),
                ("limit", int("Events at most (1..100, default 20)", inputguard::MAX_LIMIT as u64)),
                ("all", boolean("Also the owner's own events (fine)")),
            ]),
        },
        ToolDef {
            name: "security_clear".into(),
            description: "Clears security incidents (the lead or the owner; docs/design/SECURITY.md): the sessions and identities they \
                          froze take calls again, and a swarm pause lifts once fewer than two offenders remain (the clock resumes if \
                          the pause stopped it and nobody moved it since). Give incident (one, from security_incidents) or all: true. \
                          An incident the owner chose to keep frozen (the toast's Keep frozen) is the owner's alone to clear \
                          (owner_kept). note: why, at most 512 characters (journaled as a kind security record)."
                .into(),
            input_schema: schema(vec![
                ("incident", int("The incident to clear (security_incidents' incident)", u64::MAX >> 11)),
                ("all", boolean("Clear every open incident")),
                ("note", string("Why (journaled)", MAX_NOTE)),
            ]),
        },
        ToolDef {
            name: "security_allow_lead".into(),
            description: "The owner allows the granted lead (agent: its name) to point the window's keyboard and put screens on the \
                          window without an alert, for its current protocol session (a resume in another session asks again); \
                          revoke: true takes it back. The owner's alone (the window's toast offers it too). Refused when the lead \
                          has no live session (no_live_lead). Journaled as a kind security record."
                .into(),
            input_schema: schema(vec![("agent", string("The granted lead's agent name", 64)), ("revoke", boolean("Take the allowance back"))]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_bounded_and_named() {
        let p = |s: &str| parse("security_clear", &Json::parse(s).unwrap()).unwrap();
        assert!(matches!(p(r#"{"incident": 3}"#), Ok(SecurityCall::Clear { incident: Some(3), .. })));
        assert!(matches!(p(r#"{"all": true, "note": "seen"}"#), Ok(SecurityCall::Clear { incident: None, .. })));
        assert!(p(r#"{}"#).is_err(), "one of incident or all");
        assert!(p(r#"{"incident": 3, "all": true}"#).is_err());
        assert!(p(&format!(r#"{{"all": true, "note": "{}"}}"#, "x".repeat(MAX_NOTE + 1))).is_err());
        assert!(p(r#"{"all": true, "nope": 1}"#).is_err());
        let a = parse("security_allow_lead", &Json::parse(r#"{"agent": "lee"}"#).unwrap()).unwrap();
        assert!(matches!(a, Ok(SecurityCall::AllowLead { ref lead, revoke: false }) if lead == "lee"));
        assert!(parse("security_allow_lead", &Json::parse(r#"{}"#).unwrap()).unwrap().is_err());
        assert!(parse("security_incidents", &Json::Null).is_none(), "answered on the caller's thread");
    }

    #[test]
    fn only_the_lead_or_the_owner_reads_the_log() {
        let s = SessionInfo { id: "inputguard-test-nobody".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let out = incidents(&s, &Json::parse(r#"{"wait_s": 0}"#).unwrap());
        assert!(out.is_error);
        let bad = incidents(&s, &Json::parse(r#"{"wait_s": 61}"#).unwrap());
        assert!(bad.is_error);
    }
}

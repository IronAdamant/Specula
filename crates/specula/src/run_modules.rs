//! `--run`: the game file's modules, built and loaded when the engine starts
//! (docs/design/FIRST_HOUR.md, slice 3; docs/design/MODULES.md, "Who may load a module").
//!
//! **The engine does not run a project's native code because its folder was opened.** A
//! module is a library loaded into the engine's own process, with the user's rights: opening a
//! project someone sent shows its screens and runs none of its code. An owner or a lead the
//! operator granted builds and loads modules with `logic_build`, as before.
//!
//! `--run` is the operator saying, on the command line that starts the engine, that this
//! project's code is theirs to run (`specula play NAME` says it for a project on their list).
//! Then the module folders the game file names are built and loaded, one after the other, by
//! the engine itself: through the same build service and the same checks as an agent's
//! `logic_build` (what a build may use, the SDK it trusts, the digest of what it compiled), as
//! the identity [`RUN_SESSION`], which no protocol session can be.
//!
//! **Only what an owner or a granted lead last built** ([`crate::approved`]; review R1 of M6
//! part 3, the critical finding). A builder may write a module's sources and the game file
//! through the project tools, and may only propose a build. Were `--run` to build whatever is
//! on disk, restarting the engine would be the builder's way past the review. So each module is
//! built at start only while its folder has the digest it had at its last authorised build;
//! anything else is said and left for an owner or lead to build. `--run-as-is` is the operator
//! saying that what is on disk now is theirs to run, whoever wrote it.
//!
//! The work is a caller's, on a thread of its own: it waits for the engine's loop like any
//! first call, asks the engine's gate, runs Cargo, and hands the engine the library. A module
//! that does not build or load is said in the trace (`run.module`) and to the hosted game
//! ([`crate::game::Game::told`]), which shows it; the others still load.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use specula_core::Json;
use specula_protocol::{Content, SessionInfo};

use crate::startgate::StartGate;
use crate::tools::Request;

/// The session of the operator's own start-up build: never a protocol session id (those are 32
/// hex characters), so no connection can speak as it.
pub const RUN_SESSION: &str = "operator:run";
/// Most module folders a game file's start builds (a game file names at most this many).
pub const MAX_MODULES: usize = specula_ludus::game::MAX_MODULES;
/// How long the start-up build waits for the engine's loop to begin.
const START_WAIT: Duration = Duration::from_secs(120);
/// Most characters of one module's failure told to the game.
const MAX_REPORT: usize = 400;

/// What did not build or load, for the hosted game to show: filled by the start-up build's
/// thread, taken by the engine before a frame.
pub type Reports = Arc<Mutex<Vec<String>>>;

fn session() -> SessionInfo {
    SessionInfo { id: RUN_SESSION.into(), client_name: "specula --run".into(), client_version: env!("CARGO_PKG_VERSION").into(), protocol_version: String::new() }
}

/// What to do about a module `--run` will not load, for a person or an agent.
const WHAT_NOW: &str = "build it as its owner or a lead the operator granted (logic_build: that is the review), or start with --run-as-is (specula play NAME --as-is) to run what is on disk as it is";

/// The line for a module that has no record of an authorised build here.
fn not_built(dir: &str) -> String {
    format!("{dir} was not loaded: no owner or granted lead has built it on this machine, so --run does not run it. To play it, {WHAT_NOW}")
}

/// One module's build answer as a line for a person: `None` when it built and loaded.
fn failure(dir: &str, is_error: bool, answer: &Json) -> Option<String> {
    // Held back by the record of what was built here: the module's own sources, or the
    // project root's build configuration (said at once, or after the build read it).
    let held = |j: &Json| (j.get("error").and_then(Json::as_str) == Some("not_approved")).then(|| j.get("what").and_then(Json::as_str).unwrap_or("sources").to_owned());
    if let Some(what) = held(answer).or_else(|| answer.get("reload_error").and_then(held)) {
        let changed = if what == "configuration" { "the project's build configuration (Cargo.toml, a toolchain file or .cargo/ at its root) has changed" } else { "its sources have changed" };
        return Some(format!("{dir} was not loaded: {changed} since an owner or a granted lead last built it, so --run does not run it. To play it, {WHAT_NOW}"));
    }
    // (`logic_build` answers an error for a refusal, a build that failed and a load that failed.)
    if !is_error {
        return None;
    }
    // A refusal says why in `message`; a load that failed in `reload_error`; a build that
    // failed has its first diagnostic.
    let said = |j: &Json| j.get("message").and_then(Json::as_str).map(str::to_owned);
    let why = said(answer)
        .or_else(|| answer.get("reload_error").and_then(said).map(|why| format!("it built and did not load: {why}")))
        .or_else(|| {
            let first = answer.get("diagnostics").and_then(Json::as_array).and_then(|d| d.first())?;
            let message = first.get("message").and_then(Json::as_str)?;
            let at = first.get("file").and_then(Json::as_str).map(|f| format!("{f}: ")).unwrap_or_default();
            Some(format!("{} error(s); the first: {at}{message}", answer.get("errors").and_then(Json::as_u64).unwrap_or(1)))
        })
        .unwrap_or_else(|| "it did not build (logic_status and the trace say more)".to_owned());
    let line = format!("{dir} was not loaded: {why}");
    Some(if line.chars().count() > MAX_REPORT { format!("{} ...", line.chars().take(MAX_REPORT).collect::<String>()) } else { line })
}

/// Builds and loads `modules` (folders below the engine's working directory, as the game file
/// names them) once the engine's loop has begun. Returns at once; `reports` gets a line for
/// each module that was not loaded.
/// `as_is`: what is on disk now, whoever built it last (`--run-as-is`); otherwise only the
/// sources an owner or a granted lead last built here ([`crate::approved`]).
pub(crate) fn start(to_engine: Sender<Request>, gate: Arc<StartGate>, modules: Vec<String>, reports: Reports, as_is: bool) {
    let spawned = std::thread::Builder::new().name("specula-run-modules".into()).spawn(move || {
        if gate.wait(START_WAIT).is_err() {
            return;
        }
        let session = session();
        for dir in modules.into_iter().take(MAX_MODULES) {
            // Held to what was last built by someone who may load native code: sources changed
            // since (an agent's edit through the project tools, an undo, an editor) are not
            // loaded because the engine was started again.
            let root = std::env::current_dir().unwrap_or_default();
            let approved = if as_is { None } else { Some(crate::approved::approved(&root, &root.join(&dir))) };
            if let Some(None) = approved {
                let why = not_built(&dir);
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "run.module", module = dir.as_str(), loaded = false, why = why.as_str());
                reports.lock().unwrap_or_else(|p| p.into_inner()).push(why);
                continue;
            }
            let expect = approved.flatten();
            let out = crate::logic::tools::build_expecting(&to_engine, &session, &Json::obj([("path", Json::from(dir.as_str()))]), expect.as_ref());
            let answer = out
                .content
                .iter()
                .find_map(|c| match c {
                    Content::Text(t) => Json::parse(t).ok(),
                    Content::Image { .. } => None,
                })
                .unwrap_or(Json::Null);
            match failure(&dir, out.is_error, &answer) {
                None => specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "run.module", module = dir.as_str(), loaded = true),
                Some(why) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "run.module", module = dir.as_str(), loaded = false, why = why.as_str());
                    reports.lock().unwrap_or_else(|p| p.into_inner()).push(why);
                }
            }
        }
    });
    if let Err(e) = spawned {
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "run.no_thread", why = e.to_string().as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_answer_is_read_as_loaded_or_as_why_not() {
        let j = |text: &str| Json::parse(text).unwrap();
        assert_eq!(failure("modules/gate", false, &j(r#"{"ok": true, "errors": 0, "reload": {"module": "gate"}}"#)), None);
        // It built, and the engine did not take the library.
        let not_loaded = failure("modules/gate", true, &j(r#"{"ok": true, "reload_error": {"error": "claimed", "message": "/gate is claimed by mason"}}"#)).unwrap();
        assert_eq!(not_loaded, "modules/gate was not loaded: it built and did not load: /gate is claimed by mason");
        // A build that failed: how many errors, and the first.
        let failed = failure("modules/gate", true, &j(r#"{"ok": false, "errors": 2, "diagnostics": [{"file": "src/lib.rs", "message": "cannot find value `hp`"}]}"#)).unwrap();
        assert_eq!(failed, "modules/gate was not loaded: 2 error(s); the first: src/lib.rs: cannot find value `hp`");
        // A refusal (the gate, the disk, a path): its own words.
        let refused = failure("modules/gate", true, &j(r#"{"error": "sdk_differs", "message": "the project's SDK is not this engine's: specula sdk writes it"}"#)).unwrap();
        assert!(refused.starts_with("modules/gate was not loaded: the project's SDK is not this engine's"), "{refused}");
        // An answer nobody can read still says something, and a long one is cut.
        assert!(failure("modules/gate", true, &Json::Null).unwrap().contains("did not build"));
        let long = failure("modules/gate", true, &Json::obj([("message", Json::from("x".repeat(5000)))])).unwrap();
        assert!(long.chars().count() <= MAX_REPORT + 4 && long.ends_with(" ..."));
        // The identity is not one a connection can have (protocol session ids are 32 hex characters).
        assert!(RUN_SESSION.contains(':') && RUN_SESSION.len() != 32);
    }
}

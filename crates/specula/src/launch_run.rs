//! Doing what the command line asked (docs/design/LAUNCH.md): `launch_cli` reads the words,
//! this runs them. Each command answers one JSON document on stdout and an exit code: 0 done,
//! 1 the engine's tool (or the command) refused, 2 the command could not reach an engine.
//!
//! This runs in the command's own short process, never in an engine: it may read the user's
//! per-user folder, wait, and start an engine process.

use crate::launch_cli::{CallArgs, Cli, Target, Which, MAX_ARGS_BYTES};
use crate::launch_client::{Client, ClientError};
use crate::launch_instances::{self as instances, Instance};
use crate::launch_list::{same_path, Found, Project, ProjectList};
use crate::launch_store::{self as store, Store};
use specula_core::Json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long `specula open` waits for the engine it started to serve (a cold start on a loaded
/// machine can take most of a minute).
pub const START_WAIT: Duration = Duration::from_secs(180);
/// How long `specula stop` waits for the engine to be gone (it saves its world first).
pub const STOP_WAIT: Duration = Duration::from_secs(90);
/// How long `specula open` waits for the engine that holds a project's lock to say where it is.
pub const LOCK_WAIT: Duration = Duration::from_secs(30);
/// How long after a start screen's engine is gone `specula open` still expects the engine it
/// became (it starts again in the project handed to it).
pub const HANDOVER_GONE_WAIT: Duration = Duration::from_secs(45);
/// The port an engine serves on when none is named: where a configured MCP client looks.
pub const USUAL_PORT: u16 = 7471;
/// How long one `specula call` may take.
pub const CALL_WAIT: Duration = Duration::from_secs(300);
/// Tools `specula tools` lists, and the characters of each one's description it shows.
const MAX_TOOLS_LISTED: usize = 600;
const DESCRIPTION_CHARS: usize = 160;
/// The folder new projects are made in, inside the user's home folder.
pub const PROJECTS_FOLDER: &str = "Specula Projects";
/// The name of the variable that holds the user's home folder on Windows. Spelt in halves, as
/// `user_settings::ROAMING` is (`tests/private_paths.rs`).
const WINDOWS_HOME: &str = concat!("USER", "PROFILE");

/// Where a command finds things.
pub struct Ctx {
    /// The engine's per-user folder (`--settings DIR`, or the operating system's).
    pub store: Store,
    /// The command named that folder itself (`--settings DIR`): an engine it starts is given
    /// the same one. Without it the engine finds the per-user folder as the command did.
    pub settings_given: bool,
    /// The user's home folder, if the environment names one (new projects go under it).
    pub home: Option<PathBuf>,
    /// This executable (`specula open` starts it again as an engine when a project names none).
    pub exe: PathBuf,
    /// Now: seconds since the Unix epoch, UTC.
    pub now: u64,
    /// Whether the engine an instance file names is serving now ([`instances::serving`]).
    pub alive: fn(&Instance) -> bool,
}

impl Ctx {
    /// From the environment. `settings`: the command's `--settings DIR`.
    pub fn from_env(settings: Option<PathBuf>) -> Result<Ctx, String> {
        let var = |k: &str| std::env::var(k).ok();
        let settings_given = settings.is_some();
        let dir = match settings {
            // (Absolute: an engine this command starts changes its working directory to the
            // project's folder, and must read the same folder this command does. Review R1-5.)
            Some(d) => std::path::absolute(&d).unwrap_or(d),
            None => crate::user_settings::per_user_base(var).ok_or("this machine's environment names no per-user folder for the project list: pass --settings FOLDER")?,
        };
        let home = var(if cfg!(windows) { WINDOWS_HOME } else { "HOME" }).filter(|h| !h.trim().is_empty()).map(PathBuf::from);
        let exe = std::env::current_exe().map_err(|e| format!("this executable's own path is not known ({e})"))?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Ok(Ctx { store: Store::new(dir), settings_given, home, exe, now, alive: instances::serving })
    }

    /// The engines serving now (each one's own process owns its port), newest first.
    fn engines(&self) -> Vec<Instance> {
        instances::list(&self.store.dir, self.alive).0
    }
}

fn refusal(code: &str, message: impl Into<String>) -> (i32, Json) {
    (1, Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]))
}

/// A call that got no tool answer: exit code 2 when no engine was reached (nothing answers,
/// it timed out, it answered something that is not the protocol), 1 when the engine answered
/// and refused the request itself (a tool it does not have).
fn unreachable_engine(e: &ClientError) -> (i32, Json) {
    (if e.code == "rpc" { 1 } else { 2 }, e.to_json())
}

/// One project as `specula projects` shows it: what the list holds, whether its folder is still
/// there, and the engine that has it open now.
fn project_json(p: &Project, engines: &[Instance], exists: impl Fn(&Path) -> bool) -> Json {
    let mut j = p.to_json();
    if !exists(&p.path) {
        j.set("missing", Json::from(true));
    }
    if let Some(i) = engines.iter().find(|i| i.project.as_ref().is_some_and(|(path, _)| same_path(path, &p.path))) {
        j.set("running", Json::obj([("port", Json::from(u32::from(i.port))), ("pid", Json::from(i.pid)), ("url", Json::from(i.url()))]));
    }
    j
}

/// The project a word means, or why none: not listed, or a name several folders share.
/// `specula sdk`: writes this engine's SDK into `folder` and says what it did. A folder whose
/// SDK folders hold anything else is refused, and nothing is written (the engine's own
/// repository holds its whole crates at those paths).
pub fn write_sdk(folder: &Path) -> (i32, Json) {
    use specula_logic::sdk_source;
    if !folder.is_dir() {
        return refusal("missing", format!("{} is not a folder", folder.display()));
    }
    let was = sdk_source::differs(folder);
    if was.is_empty() {
        let sdk = Json::obj([("folder", Json::from(folder.join("crates").to_string_lossy().as_ref())), ("version", Json::from(sdk_source::VERSION)), ("written", Json::from(0u64))]);
        return (0, Json::obj([("sdk", sdk), ("note", Json::from("the project has this engine's SDK already: nothing to write"))]));
    }
    match sdk_source::write_into(folder) {
        Ok(written) => {
            let sdk = Json::obj([("folder", Json::from(folder.join("crates").to_string_lossy().as_ref())), ("version", Json::from(sdk_source::VERSION)), ("written", Json::from(written.len()))]);
            let mut j = Json::obj([("sdk", sdk)]);
            // What was there before and was not this engine's (missing files are not news in a new project).
            let replaced: Vec<&String> = was.iter().filter(|w| w.ends_with("not the engine's text")).collect();
            if !replaced.is_empty() {
                j.set("replaced", Json::arr(replaced.iter().take(12).map(|w| Json::from(w.split(':').next().unwrap_or(w)))));
            }
            j.set("next", Json::from("a module depends on it with: specula-sdk = { path = \"../../crates/specula-sdk\" } (from modules/NAME), and builds with logic_build"));
            (0, j)
        }
        Err(why @ sdk_source::WriteRefused::OtherFiles(_)) => refusal("other_files", why.to_string()),
        Err(why) => refusal("not_written", why.to_string()),
    }
}

fn pick_project<'a>(list: &'a ProjectList, which: &Which) -> Result<&'a Project, (i32, Json)> {
    match which {
        Which::Last => list.last().ok_or_else(|| refusal("no_projects", "no project has been worked on yet: 'specula new NAME' makes one, 'specula open FOLDER' opens a folder as one")),
        Which::Word(word) => match list.find(word) {
            Found::One(p) => Ok(p),
            Found::None => Err(refusal("not_listed", format!("no project is called '{word}' and none is listed at that folder ('specula projects' lists them)"))),
            Found::Several(paths) => {
                let (code, mut j) = refusal("ambiguous", format!("{} projects are called '{word}': say which by its folder", paths.len()));
                j.set("folders", Json::arr(paths.iter().map(|p| Json::from(p.to_string_lossy().as_ref()))));
                Err((code, j))
            }
        },
    }
}

/// The running engine a command means, or why none.
pub(crate) fn pick_engine(target: &Target, engines: &[Instance], list: &ProjectList) -> Result<Instance, (i32, Json)> {
    let listed = || Json::arr(engines.iter().take(16).map(Instance::to_json));
    match target {
        Target::Port(p) => Ok(engines.iter().find(|i| i.port == *p).cloned().unwrap_or(Instance { launcher: false, pid: 0, port: *p, project: None, world: None, window: false, started: 0, version: String::new(), game: None })),
        Target::Any => match engines {
            [] => Err((2, Json::obj([("error", Json::from("no_engine")), ("message", Json::from("no engine is running on this machine: 'specula open --last' starts one on the project last worked on"))]))),
            [one] => Ok(one.clone()),
            _ => {
                let (code, mut j) = refusal("several_engines", format!("{} engines are running: say which with --port N or --project NAME", engines.len()));
                j.set("engines", listed());
                Err((code, j))
            }
        },
        Target::Project(word) => {
            let folder = match list.find(word) {
                Found::One(p) => Some(p.path.clone()),
                Found::None => None,
                Found::Several(paths) => {
                    let (code, mut j) = refusal("ambiguous", format!("{} projects are called '{word}': say which by its folder", paths.len()));
                    j.set("folders", Json::arr(paths.iter().map(|p| Json::from(p.to_string_lossy().as_ref()))));
                    return Err((code, j));
                }
            };
            // By the list's folder, or (a project opened from a folder the list has dropped) by
            // what the engine itself says it has open.
            let hit = engines.iter().find(|i| i.project.as_ref().is_some_and(|(path, name)| folder.as_ref().is_some_and(|f| same_path(f, path)) || name.eq_ignore_ascii_case(word) || same_path(path, Path::new(word))));
            hit.cloned().ok_or_else(|| {
                let (_, mut j) = refusal("not_running", format!("no running engine has '{word}' open: 'specula open {word}' starts one"));
                j.set("engines", listed());
                (2, j)
            })
        }
    }
}

/// Runs a command: its exit code and the JSON it answers.
pub fn run(cli: Cli, ctx: &Ctx) -> (i32, Json) {
    match cli {
        Cli::Help => (0, Json::obj([("help", Json::from(crate::launch_cli::HELP))])),
        Cli::Projects => {
            let (list, problem) = ctx.store.load();
            let engines = ctx.engines();
            let mut out = vec![("projects", Json::arr(list.projects.iter().map(|p| project_json(p, &engines, |f| f.is_dir())))), ("count", Json::from(list.projects.len()))];
            if let Some(last) = list.last() {
                out.push(("last", Json::from(last.name.as_str())));
            }
            if let Some(p) = problem {
                out.push(("problem", Json::from(p)));
            }
            (0, Json::obj(out))
        }
        Cli::List => {
            let engines = ctx.engines();
            (0, Json::obj([("engines", Json::arr(engines.iter().map(Instance::to_json))), ("count", Json::from(engines.len()))]))
        }
        Cli::New { name, parent, bare } => {
            let Some(parent) = parent.or_else(|| ctx.home.as_ref().map(|h| h.join(PROJECTS_FOLDER))) else {
                return refusal("no_home", "this machine's environment names no home folder to make projects in: say where with --in FOLDER");
            };
            let parent = std::path::absolute(&parent).unwrap_or(parent);
            // A project that builds and plays (docs/design/FIRST_HOUR.md, slice 7); `--bare`:
            // the folder, its assets folder and its marker, as before.
            let made = if bare { store::create(&parent, &name).map(|folder| (folder, None)) } else { crate::scaffold::create(&parent, &name, Some(&ctx.store.dir.join(crate::approved::FOLDER))).map(|(folder, made)| (folder, Some(made))) };
            match made {
                Err(why) => refusal("not_made", why),
                Ok((folder, made)) => {
                    let project = Project { path: folder, name, exe: None, last_world: None, last_opened: ctx.now, version: env!("CARGO_PKG_VERSION").to_owned() };
                    let listed = ctx.store.opened(project.clone());
                    let mut j = Json::obj([("project", project.to_json()), ("next", Json::from(format!("specula open \"{}\"", project.path.display())))]);
                    if let Some(made) = made {
                        j.set(
                            "made",
                            Json::obj([
                                ("files", Json::from(made.files)),
                                ("module", Json::from(format!("modules/{}", made.module))),
                                ("read_first", Json::from(format!("notes/how.md, game.json, modules/{0}/src/lib.rs, modules/{0}/actions.json, assets/ui/hud.json, assets/ui/settings.json and assets/strings/en.json, in one go: how this game is built and run, and the sample's game file, rules, inputs, two screens and words (an agent is given the start of the first at its agent_hello)", made.module))),
                            ]),
                        );
                        j.set("next", Json::from(format!("specula run \"{0}\" --ticks 40 --inputs 0:enter,5:tap  (builds it and plays it with no window; notes/how.md says the rest), or: specula play \"{0}\" --as-is  (its window)", project.path.display())));
                    }
                    if let Err(e) = listed {
                        j.set("warning", Json::from(format!("the project was made but could not be listed ({e})")));
                    }
                    (0, j)
                }
            }
        }
        Cli::Forget { which } => {
            let (list, _) = ctx.store.load();
            let folder = match pick_project(&list, &Which::Word(which)) {
                Ok(p) => p.path.clone(),
                Err(no) => return no,
            };
            match ctx.store.forget(&folder) {
                Ok(_) => (0, Json::obj([("forgotten", Json::from(folder.to_string_lossy().as_ref())), ("note", Json::from("its folder was not touched"))])),
                Err(e) => refusal("not_saved", format!("the list could not be written ({e})")),
            }
        }
        Cli::Sdk { which } => {
            // The folder: the one named (a folder that is there, else a listed project's name),
            // or the one the command is run in.
            let folder = match which {
                None => match std::env::current_dir() {
                    Ok(here) => here,
                    Err(e) => return refusal("no_folder", format!("the folder this was run in can't be read ({e}): name the project")),
                },
                Some(word) if Path::new(&word).is_dir() => PathBuf::from(word),
                Some(word) => match pick_project(&ctx.store.load().0, &Which::Word(word)) {
                    Ok(p) => p.path.clone(),
                    Err(no) => return no,
                },
            };
            write_sdk(&std::path::absolute(&folder).unwrap_or(folder))
        }
        Cli::Open { which, front, engine_args } => open(ctx, &which, front, &engine_args),
        Cli::Run(args) => run_game(ctx, &args),
        Cli::Tune(args) => tune_game(ctx, &args),
        Cli::Find(args) => match game_folder(ctx, &args.which) {
            Ok(folder) => run_in(ctx, &folder, &args, Kept::AsAsked),
            Err(no) => no,
        },
        Cli::Check(args) => check_game(ctx, &args),
        Cli::Stop { target } => {
            let (list, _) = ctx.store.load();
            let engine = match pick_engine(&target, &ctx.engines(), &list) {
                Ok(e) => e,
                Err(no) => return no,
            };
            // (An engine that keeps no instance file, `--settings off`, has no folder to be
            // asked through: it is stopped where it was started.)
            if engine.pid == 0 {
                return refusal("not_listed", format!("no engine listed here serves port {}: one started with --settings off (or with another --settings folder) is stopped where it was started", engine.port));
            }
            if let Err(e) = instances::request_stop(&ctx.store.dir, engine.pid) {
                return refusal("not_asked", format!("the stop request could not be written ({e})"));
            }
            // Stopped means gone: no longer serving, and its process ended (it saves its
            // world and frees the GPU after it stops serving).
            let deadline = Instant::now() + STOP_WAIT;
            while ctx.engines().iter().any(|i| i.pid == engine.pid) || specula_sys::process_is_running(engine.pid) {
                if Instant::now() > deadline {
                    return refusal("still_running", format!("the engine (pid {}) has not stopped within {} s (it may still be saving): 'specula list' shows when it has", engine.pid, STOP_WAIT.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            (0, Json::obj([("stopped", Json::from(true)), ("pid", Json::from(engine.pid)), ("port", Json::from(u32::from(engine.port)))]))
        }
        Cli::Tools { target, filter } => {
            let (list, _) = ctx.store.load();
            let engine = match pick_engine(&target, &ctx.engines(), &list) {
                Ok(e) => e,
                Err(no) => return no,
            };
            let mut client = Client::new(engine.port, true, CALL_WAIT);
            let listed = client.connect("specula-cli").and_then(|_| client.tools());
            client.close();
            match listed {
                Err(e) => unreachable_engine(&e),
                Ok(tools) => (0, tools_answer(engine.port, &tools, filter.as_deref())),
            }
        }
        Cli::Call { target, tool, args, as_name, role } => {
            // (A file's arguments are read before any engine is looked for: a bad file is the
            // command's own mistake, whatever is running.)
            let args = match args {
                CallArgs::Given(j) => j,
                CallArgs::File(file) => match args_file(&file) {
                    Ok(j) => j,
                    Err(message) => return refusal("usage", message),
                },
            };
            let (list, _) = ctx.store.load();
            let engine = match pick_engine(&target, &ctx.engines(), &list) {
                Ok(e) => e,
                Err(no) => return no,
            };
            call(ctx, &engine, &tool, args, as_name.as_deref(), role.as_deref())
        }
    }
}

/// `specula tools [FILTER]`'s answer. With a filter: the tools whose name or description has
/// it, each with the start of its description. With none: every tool's name, by family (the
/// name up to its first `_`), and how to see more. The measured comparison's pilot
/// (2026-11-14): the whole list with descriptions was 46,000 bytes, a builder asked for it
/// twice, and that was most of what its engine calls put in its context; the names alone are
/// a tenth of it, and a family's descriptions are one more command away.
fn tools_answer(port: u16, tools: &[Json], filter: Option<&str>) -> Json {
    let named = tools.iter().filter_map(|t| Some((t.get("name").and_then(Json::as_str)?, t.get("description").and_then(Json::as_str).unwrap_or(""))));
    let port = ("port", Json::from(u32::from(port)));
    let Some(wanted) = filter.map(str::to_lowercase) else {
        let mut families: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
        let mut count = 0;
        for (name, _) in named.take(MAX_TOOLS_LISTED) {
            families.entry(name.split('_').next().unwrap_or(name)).or_default().push(name);
            count += 1;
        }
        let families = Json::Obj(families.into_iter().map(|(f, names)| (f.to_owned(), Json::from(names.join(" ").as_str()))).collect());
        let next = "names only. 'specula tools NAME' shows one tool: what it does and the arguments it takes. 'specula tools WORD' lists the tools whose name or description has WORD, with what each does (a family: 'specula tools world_'); the tools_catalog tool lists them by job";
        return Json::obj([port, ("count", Json::from(count)), ("families", families), ("next", Json::from(next))]);
    };
    // A tool's own name: that tool, whole. What an agent with only a shell needs before its
    // first call is the arguments, and nothing else gave them for one tool (the comparison's
    // builders asked tools_catalog for them, and activated whole families to read schemas).
    if let Some(t) = tools.iter().find(|t| t.get("name").and_then(Json::as_str).is_some_and(|n| n.eq_ignore_ascii_case(&wanted))) {
        let also: Vec<Json> = tools
            .iter()
            .filter_map(|o| o.get("name").and_then(Json::as_str))
            .filter(|n| !n.eq_ignore_ascii_case(&wanted) && n.to_lowercase().contains(&wanted))
            .take(MAX_TOOLS_LISTED)
            .map(Json::from)
            .collect();
        let mut out = vec![port, ("tool", t.get("name").cloned().unwrap_or(Json::Null)), ("description", t.get("description").cloned().unwrap_or(Json::Null)), ("arguments", t.get("inputSchema").cloned().unwrap_or(Json::Null))];
        if !also.is_empty() {
            out.push(("also", Json::Arr(also)));
        }
        return Json::obj(out);
    }
    let rows: Vec<Json> = named
        .filter(|(name, description)| name.to_lowercase().contains(&wanted) || description.to_lowercase().contains(&wanted))
        .map(|(name, description)| Json::obj([("name", Json::from(name)), ("description", Json::from(description.chars().take(DESCRIPTION_CHARS).collect::<String>()))]))
        .take(MAX_TOOLS_LISTED)
        .collect();
    Json::obj([port, ("count", Json::from(rows.len())), ("tools", Json::Arr(rows))])
}

/// A call's arguments from `@FILE` (read against the folder the command was run in): one JSON
/// object, at most [`MAX_ARGS_BYTES`] (no more than that is read, whatever the file is).
fn args_file(file: &Path) -> Result<Json, String> {
    use std::io::Read;
    let opened = std::fs::File::open(file).map_err(|e| format!("@{}: {e}", file.display()))?;
    let mut bytes = Vec::new();
    opened.take(MAX_ARGS_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("@{}: {e}", file.display()))?;
    if bytes.len() > MAX_ARGS_BYTES {
        return Err(format!("@{}: the file is more than {MAX_ARGS_BYTES} bytes, the most a call's arguments may be", file.display()));
    }
    // (A byte order mark is what some editors and shells put first: not part of the JSON.)
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("@{}: the file is not UTF-8 text", file.display()))?;
    crate::launch_cli::call_args(text.strip_prefix('\u{feff}').unwrap_or(text)).map_err(|e| format!("@{}: {e}", file.display()))
}

/// `specula call`: one tool call in a session of its own. With `--as NAME` the session says who
/// it is first (`agent_hello`), resuming the identity an earlier call under that name left (its
/// resume token is kept beside the engine's instance file), so claims and undo carry over from
/// one command to the next. `--role ROLE` is the role it says: an identity's authority is what
/// its latest hello was admitted with, so a lead the operator granted (`--lead NAME`) says
/// `--role lead` on the commands that need it (a role nobody granted the name is refused).
fn call(ctx: &Ctx, engine: &Instance, tool: &str, args: Json, as_name: Option<&str>, role: Option<&str>) -> (i32, Json) {
    let mut client = Client::new(engine.port, true, CALL_WAIT);
    if let Err(e) = client.connect("specula-cli") {
        return unreachable_engine(&e);
    }
    let mut unkept = None;
    // What the project remembers (agent_hello's `project_memory`), for an identity that has
    // just been made: this command said the hello, so its caller never saw the answer.
    let mut memory = None;
    if let Some(name) = as_name.filter(|_| tool != "agent_hello") {
        let token_file = ctx.store.dir.join(instances::FOLDER).join(format!("{}.{name}.token", engine.pid));
        let token = std::fs::read_to_string(&token_file).ok().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty() && t.len() <= 256);
        let hello = |client: &mut Client, token: Option<&str>| {
            let mut f = vec![("name", Json::from(name))];
            if let Some(role) = role {
                f.push(("role", Json::from(role)));
            }
            if let Some(t) = token {
                f.push(("resume_token", Json::from(t)));
            }
            client.call("agent_hello", Json::obj(f))
        };
        let mut said = hello(&mut client, token.as_deref());
        // A token from an engine that has since restarted: a new identity under the name.
        if said.as_ref().is_ok_and(|a| a.is_error && a.text.contains("bad_resume_token")) {
            said = hello(&mut client, None);
        }
        match said {
            Err(e) => {
                client.close();
                return unreachable_engine(&e);
            }
            Ok(a) if a.is_error => {
                client.close();
                let mut j = Json::parse(&a.text).unwrap_or_else(|_| Json::from(a.text.as_str()));
                // The name is held by an identity this command has no token for: most often an
                // earlier command's, whose token could not be kept where this one looks.
                if token.is_none() && j.get("error").and_then(Json::as_str) == Some("grant_bound") {
                    j.set("hint", Json::from(not_kept(ctx, engine, name)));
                }
                return (1, j);
            }
            Ok(a) => {
                if token.is_none() {
                    memory = Json::parse(&a.text).ok().and_then(|j| j.get("project_memory").cloned());
                }
                if let Some(t) = Json::parse(&a.text).ok().and_then(|j| j.get("resume_token").and_then(Json::as_str).map(str::to_owned)) {
                    // Kept beside the engine's instance file, which is only there when this
                    // command reads the settings folder the engine was started with.
                    if engine.pid == 0 || store::write_small(&token_file, &t).is_err() {
                        unkept = Some(not_kept(ctx, engine, name));
                    }
                }
            }
        }
    }
    let answer = client.call(tool, args);
    // One command, one session: closed, named or not. A named agent's identity and claims
    // outlive its session (the next command resumes them with the token kept above); a session
    // left open would never end, and a swarm driven from a shell would fill the engine's
    // session table with them (review R1-8).
    client.close();
    match answer {
        Err(e) => unreachable_engine(&e),
        Ok(a) => {
            let mut j = Json::parse(&a.text).unwrap_or_else(|_| Json::from(a.text.as_str()));
            // Said with the answer, where the caller reads: the next command would otherwise be
            // refused (or be somebody new) with nothing to say why.
            if let (Some(why), Json::Obj(_)) = (unkept, &j) {
                j.set("identity_not_kept", Json::from(why));
            }
            // Given once, with the first answer a new identity gets: what the game is and how it
            // is built and checked (docs/design/FIRST_HOUR.md, slice 6).
            if let (Some(memory), Json::Obj(_)) = (memory, &j) {
                j.set("project_memory", memory);
            }
            (i32::from(a.is_error), j)
        }
    }
}

/// Why a command could not keep `name`'s identity for the next command, and what to do.
fn not_kept(ctx: &Ctx, engine: &Instance, name: &str) -> String {
    let folder = ctx.store.dir.display();
    if engine.pid == 0 {
        format!(
            "the engine on port {} is not listed in {folder} (it was started with another --settings folder, or with --settings off), so {name}'s resume token is not kept between commands: the next command as {name} is a new identity, or is refused when the operator granted the name. Give every command the engine's own folder (specula call ... --settings DIR), or say agent_hello yourself and pass its resume_token",
            engine.port
        )
    } else {
        format!("{name}'s resume token could not be written under {folder}: the next command as {name} is a new identity, or is refused when the operator granted the name")
    }
}

/// `specula open`: finds the engine that has the project open, or starts one on it and waits
/// until it serves. A word that is a folder and not listed is opened as a project (the engine
/// lists it).
fn open(ctx: &Ctx, which: &Which, front: bool, engine_args: &[String]) -> (i32, Json) {
    let (list, _) = ctx.store.load();
    let (folder, exe) = match (pick_project(&list, which), which) {
        (Ok(p), _) => (p.path.clone(), p.exe.clone()),
        // Not listed, but a folder that is there: opened as a project.
        (Err(_), Which::Word(word)) if Path::new(word).is_dir() => (std::path::absolute(Path::new(word)).unwrap_or_else(|_| PathBuf::from(word)), None),
        (Err(no), _) => return no,
    };
    if !folder.is_dir() {
        return refusal("missing", format!("{} is not there any more: 'specula forget \"{}\"' takes it off the list", folder.display(), folder.display()));
    }
    let answer = |i: &Instance, started: bool| {
        let mut j = i.to_json();
        j.set("started", Json::from(started));
        (0, j)
    };
    // (One folder however it is spelt: a junction or a short name is the same project.)
    let has_it = |i: &Instance| i.project.as_ref().is_some_and(|(path, _)| store::same_folder(path, &folder));
    if let Some(running) = ctx.engines().iter().find(|i| has_it(i)) {
        return answer(running, false);
    }
    // An engine holds the project's lock and is not listed yet (it is starting: another
    // command got here first), or never will be: found, never doubled (review R1-2).
    if crate::launch_engine::project_locked(&folder) {
        return found_or_in_use(ctx, &folder);
    }
    // A start screen that is waiting (the window a person opened with no project) becomes the
    // project, on its port, rather than a second engine beside it. Not when the command names
    // engine flags (a start screen keeps its own), not when another command's hand-over waits
    // there, and not when it does not take it (its screen was closed, or its world is in use):
    // then an engine is started as always.
    if engine_args.is_empty() {
        if let Some(waiting) = ctx.engines().into_iter().find(|i| i.launcher && i.project.is_none()) {
            if instances::request_open(&ctx.store.dir, waiting.pid, &folder, front).unwrap_or(false) {
                let deadline = Instant::now() + START_WAIT;
                let mut gone: Option<Instant> = None;
                loop {
                    let engines = ctx.engines();
                    if let Some(i) = engines.iter().find(|i| i.pid != waiting.pid && has_it(i)) {
                        let mut j = i.to_json();
                        j.set("started", Json::from(true));
                        j.set("from_start_screen", Json::from(true));
                        return (0, j);
                    }
                    if instances::take_refused(&ctx.store.dir, waiting.pid) {
                        break;
                    }
                    // Its engine is gone and nothing took its place with this project (it was
                    // closed, or took another): an engine is started as always.
                    if engines.iter().any(|i| i.pid == waiting.pid) {
                        gone = None;
                    } else if gone.get_or_insert_with(Instant::now).elapsed() > HANDOVER_GONE_WAIT && !crate::launch_engine::project_locked(&folder) {
                        break;
                    }
                    if Instant::now() > deadline {
                        return refusal("not_serving", format!("the start screen's engine (pid {}) was handed the project but none serves it after {} s; 'specula list' shows it once it does", waiting.pid, START_WAIT.as_secs()));
                    }
                    std::thread::sleep(Duration::from_millis(150));
                }
            }
        }
    }
    // Where the engine will say it is running: made (and admitted by the disk book) before it
    // starts, so the wait below reads a folder that is there.
    if let Err(e) = crate::disk::book().make_dir(crate::disk::Category::Settings, &ctx.store.dir.join(instances::FOLDER), crate::disk::Need::Essential) {
        return refusal("no_per_user_folder", format!("{} could not be made ({e})", ctx.store.dir.display()));
    }
    // A game's own executable, if that is what opened the project last and it is still there;
    // else this one. (Another copy of this same program is not "the game's": the one the
    // person ran is the one that opens it. Review R1-15.)
    let exe = exe.filter(|e| e.is_file() && e.file_stem() != ctx.exe.file_stem()).unwrap_or_else(|| ctx.exe.clone());
    let named_port = engine_args.iter().any(|a| a == "--port");
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut command = std::process::Command::new(&exe);
        command.arg("--project").arg(&folder);
        if ctx.settings_given {
            command.arg("--settings").arg(&ctx.store.dir);
        }
        // Its window opens behind the others and takes no keyboard, unless a person asked to see
        // it (`--front`): whoever is using this machine is not interrupted by an agent's engine.
        let windowed = !engine_args.iter().any(|a| a == "--headless");
        if windowed && !front && !engine_args.iter().any(|a| a == "--no-activate") {
            command.arg("--no-activate");
        }
        // The usual port when it is free (a configured client finds the engine there), else one
        // the system has free now. A second try (the usual port was taken between the look and
        // the engine's bind: two commands at once) is always on a free one.
        if !named_port && (attempt > 1 || instances::port_taken(USUAL_PORT)) {
            match free_port() {
                Some(port) => {
                    command.arg("--port").arg(port.to_string());
                }
                None => return refusal("no_port", "no free port for the engine"),
            }
        }
        command.args(engine_args);
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            // Its own process group and no console of ours: it outlives this command.
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0008 | 0x0000_0200);
        }
        // The engine outlives this command: it must not hold this command's output open (whoever
        // reads the answer would wait for the engine to end).
        specula_sys::process::keep_std_handles();
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => return refusal("not_started", format!("{} could not be started ({e})", exe.display())),
        };
        let pid = child.id();
        let deadline = Instant::now() + START_WAIT;
        let status = loop {
            let engines = ctx.engines();
            if let Some(i) = engines.iter().find(|i| i.pid == pid) {
                return answer(i, true);
            }
            if let Ok(Some(status)) = child.try_wait() {
                break status;
            }
            if Instant::now() > deadline {
                return refusal("not_serving", format!("the engine (pid {pid}) has not served within {} s; 'specula list' shows it once it does", START_WAIT.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(150));
        };
        // It ended before it served. Another command's engine took the project first (ours was
        // refused the lock): that one is the answer.
        if crate::launch_engine::project_locked(&folder) || ctx.engines().iter().any(&has_it) {
            return found_or_in_use(ctx, &folder);
        }
        if attempt == 1 && !named_port {
            continue;
        }
        return refusal("engine_exited", format!("the engine stopped before it served ({status}): run it in a terminal to see why: \"{}\" --project \"{}\"", exe.display(), folder.display()));
    }
}

/// An engine holds the project's lock: the one listed here that has it open, once it says so
/// (it may still be starting); else that it is in use by one that is not listed here.
fn found_or_in_use(ctx: &Ctx, folder: &Path) -> (i32, Json) {
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        if let Some(i) = ctx.engines().into_iter().find(|i| i.project.as_ref().is_some_and(|(path, _)| store::same_folder(path, folder))) {
            let mut j = i.to_json();
            j.set("started", Json::from(false));
            return (0, j);
        }
        if !crate::launch_engine::project_locked(folder) || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    refusal(
        "in_use",
        format!("{} is open in an engine that is not listed here (one started with --settings off or with another --settings folder), or in one that stopped this moment: 'specula list' shows the engines found here", folder.display()),
    )
}

/// The folder new projects are made in when none is named: "Specula Projects" in the user's
/// home folder, if the environment names one.
pub fn default_parent(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    var(if cfg!(windows) { WINDOWS_HOME } else { "HOME" }).filter(|h| !h.trim().is_empty()).map(|h| PathBuf::from(h).join(PROJECTS_FOLDER))
}

/// A port the system has free now (loopback).
/// How long a headless run may take before its engine is ended (its first build of a module
/// compiles the SDK).
const RUN_WALL: Duration = Duration::from_secs(1800);
/// Most characters of what a run's engine said that a refusal carries.
const RUN_SAID: usize = 600;
/// How long a headless run waits for a project that an unlisted engine has open (another run
/// of the same project ends within seconds; its first build of a module takes longer).
const RUN_LOCK_WAIT: Duration = Duration::from_secs(90);

/// `specula run`: an engine of this run's own plays the script with no window and ends; its
/// answer is read from a file in a folder of this run's own, which goes when it is read.
fn run_game(ctx: &Ctx, args: &crate::launch_cli::RunArgs) -> (i32, Json) {
    let folder = match game_folder(ctx, &args.which) {
        Ok(folder) => folder,
        Err(no) => return no,
    };
    // One parameter with several values: the run is played once for each, a line each.
    let Some(swept) = args.set.iter().position(|(_, values)| values.len() > 1) else {
        let (code, mut answer) = run_in(ctx, &folder, args, Kept::AsAsked);
        if let (0, Some(out)) = (code, &args.sounds) {
            match write_sounds(&folder, out) {
                Ok(written) => drop(answer.set(RUN_SOUNDS, Json::Arr(written))),
                Err(why) => return why,
            }
        }
        return (code, answer);
    };
    let mut lines = Vec::new();
    let mut failed = false;
    for value in &args.set[swept].1 {
        let mut one = args.clone();
        one.set[swept].1 = vec![*value];
        let (code, answer) = run_in(ctx, &folder, &one, Kept::AsAsked);
        failed |= code != 0;
        let set = Json::obj([(args.set[swept].0.as_str(), Json::Num(*value))]);
        match (code, answer.get(RUN_AT).and_then(Json::as_array)) {
            (0, Some(states)) => {
                lines.extend(states.iter().map(|state| Json::obj([("set", set.clone()), ("state", state.clone())])));
                if answer.get(RUN_AT_IS_ALL).is_none() {
                    lines.push(Json::obj([("set", set), ("state", answer.get(RUN_REPORT).cloned().unwrap_or(Json::Null))]));
                }
            }
            (0, None) => lines.push(Json::obj([("set", set), ("state", answer.get(RUN_REPORT).cloned().unwrap_or(Json::Null))])),
            _ => {
                // (A run that could not be made says why, and the values after it are not tried:
                // a build that fails fails for every one.)
                lines.push(Json::obj([("set", set), ("error", answer)]));
                break;
            }
        }
    }
    (i32::from(failed), Json::obj([(CHECK_LINES, Json::Arr(lines))]))
}

/// Most sounds one run writes out.
const MAX_SOUNDS_WRITTEN: usize = 64;
/// The key a run's answer has the sounds it wrote under.
const RUN_SOUNDS: &str = "sounds_written";

/// `--sounds DIR`: every sound of the game (`assets/sounds/NAME.json`, a recipe) rendered and
/// written as `DIR/NAME.wav`, 16 bits, as the game would play it. No engine is started for it:
/// a brief that asks for each sound as a file cost a builder of the comparison three requests
/// and an engine left running (`audio_preview`), and the files landed in a captures folder.
/// Returns a line for each file written.
fn write_sounds(project: &Path, out: &Path) -> Result<Vec<Json>, (i32, Json)> {
    use specula_audio::{synth, wav};
    let from = project.join("assets").join("sounds");
    let out = std::path::absolute(out).unwrap_or_else(|_| out.to_path_buf());
    let mut recipes: Vec<PathBuf> = match std::fs::read_dir(&from) {
        Ok(entries) => entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json") && p.is_file()).collect(),
        Err(_) => Vec::new(),
    };
    recipes.sort();
    if recipes.is_empty() {
        return Err(refusal("no_sounds", format!("{} has no sound to write: a sound is a recipe, NAME.json, there (notes/world.md)", from.display())));
    }
    if recipes.len() > MAX_SOUNDS_WRITTEN {
        return Err(refusal("too_many_sounds", format!("{} has {} recipes; --sounds writes at most {MAX_SOUNDS_WRITTEN}", from.display(), recipes.len())));
    }
    crate::disk::book().make_dir(crate::disk::Category::Captures, &out, crate::disk::Need::Essential).map_err(|e| refusal("no_sounds", format!("{}: {e}", out.display())))?;
    let mut written = Vec::new();
    for path in recipes {
        let bad = |why: String| refusal("bad_sound", format!("{}: {why}", path.display()));
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > crate::module_sounds::MAX_RECIPE_BYTES {
            return Err(bad(format!("{size} bytes; a sound's recipe is at most {} KiB", crate::module_sounds::MAX_RECIPE_BYTES >> 10)));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| bad(e.to_string()))?;
        let recipe = synth::Recipe::parse(&text).map_err(|e| bad(e.to_string()))?;
        if recipe.rendered_seconds() > crate::module_sounds::MAX_SOUND_S {
            return Err(bad(format!("{:.1} s long; a game's sound is at most {} s", recipe.rendered_seconds(), crate::module_sounds::MAX_SOUND_S)));
        }
        let sound = synth::render(&recipe, &synth::RenderOptions::default()).map_err(|e| bad(e.to_string()))?;
        let bytes = wav::encode(&sound.samples, sound.channels, sound.rate, wav::WavFormat::Pcm16).map_err(|e| bad(e.to_string()))?;
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let file = out.join(format!("{name}.wav"));
        crate::disk::book().write(crate::disk::Category::Captures, "sounds", &file, &bytes, crate::disk::Need::Essential).map_err(|e| bad(format!("{}: {e}", file.display())))?;
        let seconds = sound.samples.len() as f64 / (f64::from(sound.channels.max(1)) * f64::from(sound.rate.max(1)));
        written.push(Json::obj([("sound", Json::from(name.as_str())), ("file", Json::from(file.to_string_lossy().as_ref())), ("seconds", Json::Num((seconds * 1000.0).round() / 1000.0))]));
    }
    Ok(written)
}

/// `specula tune`: the value of one parameter at which a condition begins to hold, looked for
/// by halving the range. Each try is a whole run of its own, from the game's start, with that
/// value: nothing of one try is in the next, and a value found is one a plain `specula run
/// --set` plays again. It needs the condition to hold at one end of the range and not at the
/// other; where it holds at both, or at neither, it says so with the two states, and looks no
/// further (a condition that comes and goes inside the range is not one a halving finds).
fn tune_game(ctx: &Ctx, args: &crate::launch_cli::RunArgs) -> (i32, Json) {
    let folder = match game_folder(ctx, &args.which) {
        Ok(folder) => folder,
        Err(no) => return no,
    };
    let (Some((name, low, high)), Some(until)) = (args.vary.clone(), args.until.clone()) else { return refusal("usage", "specula tune needs --vary and --until") };
    let tries = args.tries.unwrap_or(DEFAULT_TUNE_TRIES).max(2);
    let mut lines = Vec::new();
    let at = |value: f64, lines: &mut Vec<Json>| -> Result<bool, Json> {
        let mut one = args.clone();
        one.vary = None;
        one.until = None;
        one.set.push((name.clone(), vec![value]));
        let state = match run_in(ctx, &folder, &one, Kept::AsAsked) {
            (0, answer) => {
                // A number no rule reads decides nothing: every try would be the same run.
                let unread = crate::script_run::unread_words(&name);
                if answer.get("problems").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).any(|p| p.starts_with(&unread)) {
                    return Err(refusal("unread_parameter", format!("{unread}: --vary takes a name the rules read with cx.param(\"{name}\", default)")).1);
                }
                answer.get(RUN_REPORT).cloned().unwrap_or(Json::Null)
            }
            (_, why) => return Err(why),
        };
        if let Some(line) = until.unread(&state).first() {
            let mut e = refusal("unknown_line", format!("'{line}' is read by --until and the game's report has no such line (game.json, \"report\")")).1;
            e.set("state", state);
            return Err(e);
        }
        let holds = until.holds(&state);
        lines.push(Json::obj([("set", Json::obj([(name.as_str(), Json::Num(value))])), ("holds", Json::Bool(holds)), ("state", state)]));
        Ok(holds)
    };
    let ends = at(low, &mut lines).and_then(|a| at(high, &mut lines).map(|b| (a, b)));
    let (mut holds_at, mut fails_at) = match ends {
        Err(why) => {
            lines.push(why);
            return (1, Json::obj([(CHECK_LINES, Json::Arr(lines))]));
        }
        Ok((true, false)) => (low, high),
        Ok((false, true)) => (high, low),
        Ok((both, _)) => {
            let words = if both { "holds at both ends of the range: every value tried is one that does" } else { "holds at neither end of the range: widen it, or the condition is not one this number decides" };
            lines.push(Json::obj([("found", Json::Bool(false)), ("until", Json::from(until.text().as_str())), ("why", Json::from(format!("the condition {words}").as_str())), ("tries", Json::from(lines.len() as u64))]));
            return (i32::from(!both), Json::obj([(CHECK_LINES, Json::Arr(lines))]));
        }
    };
    while (lines.len() as u64) < tries {
        let middle = (holds_at + fails_at) / 2.0;
        match at(middle, &mut lines) {
            Ok(true) => holds_at = middle,
            Ok(false) => fails_at = middle,
            Err(why) => {
                lines.push(why);
                return (1, Json::obj([(CHECK_LINES, Json::Arr(lines))]));
            }
        }
    }
    lines.push(Json::obj([
        ("found", Json::Bool(true)),
        ("until", Json::from(until.text().as_str())),
        ("parameter", Json::from(name.as_str())),
        ("holds_at", Json::Num(holds_at)),
        ("fails_at", Json::Num(fails_at)),
        ("tries", Json::from(lines.len() as u64)),
        ("note", Json::from("the condition begins to hold between the two: holds_at is the nearest value tried at which it does")),
    ]));
    (0, Json::obj([(CHECK_LINES, Json::Arr(lines))]))
}

/// The runs `specula tune` makes when it is not told how many.
const DEFAULT_TUNE_TRIES: u64 = 10;

/// Where a headless run's player settings are read and kept.
#[derive(Clone, Copy)]
enum Kept<'a> {
    /// Where the command itself named (`--settings DIR`), or nowhere: `specula run`.
    AsAsked,
    /// Nowhere, whatever the command was told: a check that names no settings.
    Nowhere,
    /// This folder: checks that name the same settings.
    At(&'a Path),
}

/// How long `specula check` goes on starting runs (each run has its own limit besides).
const CHECK_WALL: Duration = Duration::from_secs(3600);

/// Where a check's frame is written: `frame` (a path below the project's checks folder, as the
/// checks file's reading holds) under `project`, with no step of it a link and no file there
/// when the run writes it. The file is the project's data and may be someone else's: a
/// junction named `checks/out` would lead a frame anywhere this user can write, and a file
/// that is there already may be a second name of a file elsewhere (a hard link, which nothing
/// tells from a file): a frame written before is removed, never written into.
fn frame_place(project: &Path, frame: &Path) -> Result<PathBuf, String> {
    let root = crate::disk::canonical(project);
    let mut at = root.clone();
    let steps: Vec<_> = frame.components().collect();
    for (i, step) in steps.iter().enumerate() {
        at.push(step);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if crate::disk::is_link(&m) => return Err(format!("{} is a link: a check's frame is written below the project's own checks folder and nowhere a link leads", at.display())),
            Ok(m) if i + 1 < steps.len() && !m.is_dir() => return Err(format!("{} is not a folder", at.display())),
            Ok(m) if i + 1 == steps.len() && !m.is_file() => return Err(format!("{} is there and is not a file", at.display())),
            Ok(_) if i + 1 == steps.len() => {
                crate::disk::remove_file_inside(&root, &at).map_err(|e| format!("{}: the frame written before could not be removed ({e})", at.display()))?;
            }
            // (What is not there yet is made by the run, as a folder or a file of its own.)
            Ok(_) | Err(_) => {}
        }
    }
    Ok(at)
}

/// The folder of the project a headless command names (by name, by folder, or the folder it
/// is run in): one that has a game file.
fn game_folder(ctx: &Ctx, which: &Option<String>) -> Result<PathBuf, (i32, Json)> {
    let folder = match which {
        None => std::env::current_dir().map_err(|e| refusal("no_folder", format!("the folder this was run in can't be read ({e}): name the project")))?,
        Some(word) if Path::new(word).is_dir() => PathBuf::from(word),
        Some(word) => pick_project(&ctx.store.load().0, &Which::Word(word.clone()))?.path.clone(),
    };
    let folder = std::path::absolute(&folder).unwrap_or(folder);
    if !folder.join(specula_ludus::game::FILE).is_file() {
        return Err(refusal("no_game", format!("{} has no {}: a headless run plays a project's game ('specula new NAME' makes a project that has one)", folder.display(), specula_ludus::game::FILE)));
    }
    Ok(folder)
}

/// Most characters of a state that a check's line carries when the check did not hold.
const CHECK_STATE: usize = 4000;
/// The key a `specula check` answer has its lines under ([`answer`] prints them, a line each).
const CHECK_LINES: &str = "check_lines";

/// `specula check`: every run the project's checks file lists, each played as `specula run`
/// plays one and held against what the check expects. A line a check, then a line that counts.
fn check_game(ctx: &Ctx, args: &crate::launch_cli::CheckArgs) -> (i32, Json) {
    let folder = match game_folder(ctx, &args.which) {
        Ok(folder) => folder,
        Err(no) => return no,
    };
    let file = args.file.clone().unwrap_or_else(|| folder.join(crate::run_checks::FILE));
    let shown = file.display().to_string();
    let text = match std::fs::metadata(&file) {
        Ok(m) if m.len() > crate::run_checks::MAX_FILE => return refusal("bad_checks", format!("{shown} is larger than a checks file is ({} bytes at most)", crate::run_checks::MAX_FILE)),
        Ok(_) => match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) => return refusal("no_checks", format!("{shown}: {e}")),
        },
        Err(e) => return refusal("no_checks", format!("{shown}: {e} (a project's checks are runs of its game and what each must print: notes/how.md says how one is written)")),
    };
    let mut checks = match crate::run_checks::parse(&text) {
        Ok(checks) => checks,
        Err(why) => return refusal("bad_checks", format!("{shown}: {why}")),
    };
    if let Some(only) = &args.only {
        let names: Vec<String> = checks.iter().map(|c| c.name.clone()).collect();
        checks.retain(|c| c.name == *only);
        if checks.is_empty() {
            let (code, mut j) = refusal("no_check", format!("{shown} has no check named '{only}'"));
            j.set("checks", Json::arr(names.iter().map(|n| Json::from(n.as_str()))));
            return (code, j);
        }
    }
    // The settings folders the checks name are this command's own, made new and gone with it:
    // what one check sets a later one finds, and nothing of an earlier command's is there.
    let temp = crate::disk::canonical(&std::env::temp_dir());
    let scratch = temp.join(format!("specula-check-{}", crate::swarm::random_hex()));
    if checks.iter().any(|c| c.settings.is_some()) {
        if scratch.exists() {
            return refusal("no_scratch", format!("{} is there already", scratch.display()));
        }
        if let Err(e) = crate::disk::book().make_dir(crate::disk::Category::Scratch, &scratch, crate::disk::Need::Essential) {
            return refusal("no_scratch", format!("{}: {e}", scratch.display()));
        }
    }
    let (mut lines, mut failed): (Vec<Json>, Vec<Json>) = (Vec::new(), Vec::new());
    let mut problems: Option<Json> = None;
    let began = Instant::now();
    for check in &checks {
        // (The whole command has a limit too: what is left when it is reached is not played,
        // and the count says how many.)
        if began.elapsed() > CHECK_WALL {
            break;
        }
        // A frame's place is the project's, wherever the command is run: held again here,
        // against what is on the disk now (no link on the way, nothing but a file at its end).
        let frame = match check.frame.as_ref().map(|f| frame_place(&folder, f)) {
            None => None,
            Some(Ok(place)) => Some(place),
            Some(Err(why)) => {
                let (_, error) = refusal("bad_frame", why);
                lines.push(Json::obj([("check", Json::from(check.name.as_str())), ("ok", Json::Bool(false)), ("error", error)]));
                failed.push(Json::from(check.name.as_str()));
                continue;
            }
        };
        let run = crate::launch_cli::RunArgs { which: None, ticks: Some(check.ticks), seed: check.seed, inputs: check.inputs.clone(), set: check.set.iter().map(|(name, value)| (name.clone(), vec![*value])).collect(), frame, size: check.size, ..Default::default() };
        // A check's settings are the folder it names, or none: never the folder the command
        // itself was given, which is some player's own.
        let named = check.settings.as_ref().map(|name| scratch.join(name));
        let kept = named.as_deref().map_or(Kept::Nowhere, Kept::At);
        let (code, answer) = run_in(ctx, &folder, &run, kept);
        let mut line = Json::obj([("check", Json::from(check.name.as_str()))]);
        let state = answer.get(RUN_REPORT);
        let off = match state {
            Some(state) if code == 0 => crate::run_checks::differs(&check.expect, state),
            _ => Vec::new(),
        };
        let ok = code == 0 && state.is_some() && off.is_empty();
        line.set("ok", Json::Bool(ok));
        match state {
            Some(state) if code == 0 => {
                if !off.is_empty() {
                    line.set("differs", Json::Obj(off.into_iter().map(|(key, is, expected)| (key, Json::obj([("is", is), ("expected", expected)]))).collect()));
                }
                // The state itself where it is what was asked for (a check that expects
                // nothing is a run to read) or what explains a check that did not hold.
                if !ok || check.expect.is_empty() {
                    let whole = state.to_string();
                    match whole.char_indices().nth(CHECK_STATE) {
                        Some((at, _)) => line.set("state_begins", Json::from(&whole[..at])),
                        None => line.set("state", state.clone()),
                    };
                }
            }
            _ => {
                line.set("error", answer.clone());
            }
        }
        if !ok {
            failed.push(Json::from(check.name.as_str()));
            // (What the rules logged in a run that did not hold is beside it: once. A run
            // that could not be made has it in its error.)
            if let Some(log) = answer.get("log").filter(|_| line.get("error").is_none()) {
                line.set("log", log.clone());
            }
        }
        // What the game says is wrong with its own files is the same in every run: said once.
        // A number this check set and no rule read is this check's own: said on its line.
        let (own, shared): (Vec<Json>, Vec<Json>) = answer.get("problems").and_then(Json::as_array).unwrap_or(&[]).iter().cloned().partition(|p| p.as_str().is_some_and(|p| p.starts_with(crate::script_run::UNREAD_BEGINS)));
        if !own.is_empty() {
            line.set("problems", Json::Arr(own));
        }
        if problems.is_none() && !shared.is_empty() {
            problems = Some(Json::Arr(shared));
        }
        lines.push(line);
        // A game that does not build, or a game file that does not load, fails every check
        // the same way: said once.
        // (And an engine that did not answer: the next would wait as long.)
        let fatal = ["build_failed", "load_failed", "bad_game_file", "no_game", "project_open", "no_answer"];
        if answer.get("error").and_then(Json::as_str).is_some_and(|e| fatal.contains(&e)) {
            break;
        }
    }
    if scratch.is_dir() {
        let _ = crate::disk::remove_tree_inside(&temp, &scratch);
    }
    let played = lines.len();
    let mut summary = Json::obj([("checks", Json::from(checks.len() as u64)), ("passed", Json::from((played - failed.len()) as u64))]);
    if !failed.is_empty() {
        summary.set("failed", Json::Arr(failed.clone()));
    }
    if played < checks.len() {
        summary.set("not_played", Json::from((checks.len() - played) as u64));
    }
    if let Some(problems) = problems {
        summary.set("problems", problems);
    }
    lines.push(summary);
    (i32::from(!failed.is_empty() || played < checks.len()), Json::obj([(CHECK_LINES, Json::Arr(lines))]))
}

/// One headless run of the game in `folder`. `kept`: where this run's player settings are
/// read and kept (`None`: where the command itself named, or nowhere).
fn run_in(ctx: &Ctx, folder: &Path, args: &crate::launch_cli::RunArgs, kept: Kept) -> (i32, Json) {
    let folder = folder.to_path_buf();
    // One engine has a project at a time. An engine that is listed has it: said at once, with
    // where it is. One that is not listed may be another run of this command, which ends by
    // itself: waited for, a while.
    let holder = |ctx: &Ctx| ctx.engines().into_iter().find(|i| i.project.as_ref().is_some_and(|(path, _)| store::same_folder(path, &folder)));
    let open_in = |i: &Instance| {
        let (code, mut j) = refusal("project_open", format!("{} is open in an engine (port {}, process {}): a headless run needs the project to itself. 'specula stop --port {}' stops that engine", folder.display(), i.port, i.pid, i.port));
        j.set("engine", i.to_json());
        (code, j)
    };
    if let Some(i) = holder(ctx) {
        return open_in(&i);
    }
    let waited = Instant::now();
    while crate::launch_engine::project_locked(&folder) {
        if let Some(i) = holder(ctx) {
            return open_in(&i);
        }
        if waited.elapsed() > RUN_LOCK_WAIT {
            return refusal("project_open", format!("{} has been open in an engine for {} s, one that is not listed here (started with --settings off or with another --settings folder: it is stopped where it was started; another 'specula run' of this project ends by itself). A headless run needs the project to itself", folder.display(), RUN_LOCK_WAIT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // (As the disk book names folders: the one spelling its removal takes.)
    let temp = crate::disk::canonical(&std::env::temp_dir());
    // A folder of this run's own, under a name nobody could have made first (the temporary
    // folder is shared on some systems: what the engine is asked, and its answer, are read
    // from here).
    let scratch = temp.join(format!("specula-run-{}", crate::swarm::random_hex()));
    if scratch.exists() {
        return refusal("no_scratch", format!("{} is there already", scratch.display()));
    }
    if let Err(e) = crate::disk::book().make_dir(crate::disk::Category::Scratch, &scratch, crate::disk::Need::Essential) {
        return refusal("no_scratch", format!("{}: {e}", scratch.display()));
    }
    let done = |answer: (i32, Json)| {
        // (This run's own folder, inside the temporary folder, never through a link.)
        let _ = crate::disk::remove_tree_inside(&temp, &scratch);
        answer
    };
    let absolute = |p: &PathBuf| std::path::absolute(p).unwrap_or_else(|_| p.clone());
    let spec = crate::script_run::Spec {
        ticks: args.ticks.unwrap_or(0),
        // (A run is the same run every time it is made, unless it is told to be another.)
        seed: Some(args.seed.unwrap_or(crate::script_run::DEFAULT_SEED)),
        inputs: args.inputs.clone(),
        frame: args.frame.as_ref().map(absolute),
        size: args.size.unwrap_or((960, 540)),
        follow: args.follow,
        at: args.at.clone(),
        keys: args.keys.clone(),
        set: args.set.iter().map(|(name, values)| (name.clone(), values[0])).collect(),
        find: args.until.clone().filter(|_| !args.words.is_empty()).map(|until| crate::script_run::Find {
            until,
            words: args.words.clone(),
            rise: args.rise.clone(),
            cells: args.cells.clone(),
            step: args.step.unwrap_or(crate::script_run::DEFAULT_STEP),
            tries: args.tries.unwrap_or(crate::script_run::DEFAULT_FIND_TRIES),
            never: args.never.clone(),
            seconds: args.seconds.unwrap_or(crate::script_run::DEFAULT_FIND_SECONDS),
        }),
        out: scratch.join("answer.json"),
    };
    if let Some(parent) = spec.frame.as_ref().and_then(|f| f.parent()).filter(|p| !p.as_os_str().is_empty()) {
        if let Err(e) = crate::disk::book().make_dir(crate::disk::Category::Captures, parent, crate::disk::Need::Essential) {
            return done(refusal("no_frame", format!("{}: {e}", parent.display())));
        }
    }
    let spec_file = scratch.join("spec.json");
    if let Err(e) = crate::disk::book().write(crate::disk::Category::Scratch, "run", &spec_file, spec.to_json().to_string().as_bytes(), crate::disk::Need::Essential) {
        return done(refusal("no_scratch", format!("{}: {e}", spec_file.display())));
    }
    // The engine takes the project's lock itself, after it has started: two runs of one project
    // begun in the same moment both find it free here, and one engine is refused it. That one
    // says so in its answer, and its run waits for the other and starts again.
    let (ended, said, answer) = loop {
        let Some(port) = free_port() else { return done(refusal("no_port", "no free port for the run's engine")) };
        let mut command = std::process::Command::new(&ctx.exe);
        command.arg("--project").arg(&folder).arg("--script").arg(&spec_file).arg("--port").arg(port.to_string());
        command.arg("--captures").arg(scratch.join("captures")).args(["--trace-file", "off"]);
        // (A search keeps the situations it reached as saves: in this run's own folder, gone with it.)
        if spec.find.is_some() {
            command.arg("--saves").arg(scratch.join("saves"));
        }
        // Where this run's player settings are read and kept: the folder the command named, or
        // nowhere (and then the engine is not listed either: it is this command's own).
        match kept {
            Kept::At(dir) => {
                command.arg("--settings").arg(dir);
            }
            Kept::AsAsked if ctx.settings_given => {
                command.arg("--settings").arg(&ctx.store.dir);
            }
            Kept::AsAsked | Kept::Nowhere => {
                command.args(["--settings", "off"]);
            }
        }
        // What the engine says of itself is kept, its last lines: why a run that did not answer did not.
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        if spec.follow {
            // A followed run's steps are read by the engine itself, from this command's own
            // input. Its states come back on the engine's standard error, each a line that
            // begins with a mark, and are passed on below as this command's output: never on
            // the engine's standard output, where what a module prints would land between them.
            command.stdin(std::process::Stdio::inherit());
        }
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => return done(refusal("not_started", format!("{} could not be started ({e})", ctx.exe.display()))),
        };
        // (Set when a state could not be passed on: whoever followed the run has gone.)
        let gone = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let said = child.stderr.take().map(|mut pipe| {
            let (follow, gone) = (spec.follow, gone.clone());
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let keep = |tail: &mut Vec<u8>, bytes: &[u8]| {
                    tail.extend_from_slice(bytes);
                    let over = tail.len().saturating_sub(4 * RUN_SAID);
                    tail.drain(..over);
                };
                let (mut tail, mut line, mut part) = (Vec::new(), Vec::new(), [0u8; 4096]);
                // (Whether `line` is a state's: begun by the mark, wherever in a line of the
                // engine's own words the mark came. A module that writes to standard error
                // and does not end its line leaves words in front of the next state: they
                // are the engine's, and the state is still a state. The review's finding:
                // read by its first byte alone, such a state was lost, and whoever played
                // waited for it.)
                let mut state = false;
                while let Ok(n @ 1..) = pipe.read(&mut part) {
                    if !follow {
                        keep(&mut tail, &part[..n]);
                        continue;
                    }
                    for &byte in &part[..n] {
                        if byte == crate::script_run::STATE_MARK && !state {
                            keep(&mut tail, &line);
                            line.clear();
                            state = true;
                            continue;
                        }
                        line.push(byte);
                        if byte == b'\n' && state {
                            let mut out = std::io::stdout().lock();
                            if out.write_all(&line).and_then(|()| out.flush()).is_err() {
                                gone.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            line.clear();
                            state = false;
                        } else if byte == b'\n' || line.len() > RUN_STATE_LINE {
                            // (The engine's own words; or a line longer than any state is.)
                            keep(&mut tail, &line);
                            line.clear();
                            state = false;
                        }
                    }
                }
                keep(&mut tail, &line);
                String::from_utf8_lossy(&tail).into_owned()
            })
        });
        let began = std::time::Instant::now();
        let ended = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if began.elapsed() < RUN_WALL && !gone.load(std::sync::atomic::Ordering::Relaxed) => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        // (The engine has ended: its last words, and its last state, are read to their end
        // before this command says anything of its own.)
        let said = said.and_then(|s| s.join().ok()).unwrap_or_default();
        if gone.load(std::sync::atomic::Ordering::Relaxed) {
            return done(refusal("no_reader", "the run's output could not be written: whoever followed it has gone, and the run was ended"));
        }
        let answer = std::fs::read_to_string(&spec.out).ok().and_then(|t| Json::parse(&t).ok());
        let refused_the_lock = answer.as_ref().and_then(|a| a.get("error")).and_then(|e| e.get("error")).and_then(Json::as_str) == Some(crate::script_run::PROJECT_OPEN);
        if !refused_the_lock || waited.elapsed() > RUN_LOCK_WAIT {
            break (ended, said, answer);
        }
        // (The refused engine's answer goes before the next one is started: a stale answer is
        // never read as the next engine's.)
        let _ = crate::disk::remove_file_inside(&temp, &spec.out);
        while crate::launch_engine::project_locked(&folder) && waited.elapsed() <= RUN_LOCK_WAIT {
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let Some(answer) = answer else {
        // No answer: the engine did not get as far as the run (its own words say why).
        let said: String = said.trim().chars().rev().take(RUN_SAID).collect::<Vec<_>>().into_iter().rev().collect();
        let why = match ended {
            None => format!("the run did not end within {} minutes and was stopped", RUN_WALL.as_secs() / 60),
            Some(status) => format!("the run's engine ended ({status}) before it answered"),
        };
        let (code, mut j) = refusal("no_answer", why);
        j.set("engine_said", Json::from(said.as_str()));
        return done((code, j));
    };
    if answer.get("ok").and_then(Json::as_bool) != Some(true) {
        return done((1, answer.get("error").cloned().unwrap_or_else(|| refusal("no_answer", "the run's answer could not be read").1)));
    }
    let mut out = Json::obj([(RUN_REPORT, answer.get("report").cloned().unwrap_or(Json::Null))]);
    // (A search that found nothing made its run and has no script: an answer, and exit code 1.)
    let unfound = answer.get("searched").is_some() && answer.get("report").and_then(|r| r.get("found")).and_then(Json::as_bool) != Some(true);
    for beside in ["problems", "log", RUN_FOLLOWED, RUN_AT, RUN_AT_IS_ALL] {
        if let Some(value) = answer.get(beside) {
            out.set(beside, value.clone());
        }
    }
    done((i32::from(unfound), out))
}

/// The key a headless run's answer has its report under ([`answer`] prints the report alone,
/// as one line: it is the game's own state, for whoever checks it, not this command's words).
const RUN_REPORT: &str = "run_report";
/// The key that says a run was followed: its states were printed as it went, by the engine.
const RUN_FOLLOWED: &str = "followed";
/// The states at the ticks `--at` named, in order; and whether the last of them is the run's
/// last state (then that one is not printed twice).
const RUN_AT: &str = "at";
const RUN_AT_IS_ALL: &str = "at_is_all";

/// The longest line of a followed run's engine that is still read as one (a state is far
/// shorter: a module keeps a bounded number of bounded values).
const RUN_STATE_LINE: usize = 8 << 20;

/// What a module logged (`cx.log`), as a line beside a run's answer: at the script's tick,
/// as an input's tick is counted (the game's own clock stands at the title and in a pause).
fn log_line(line: &Json) -> String {
    let word = |key: &str| line.get(key).map_or_else(String::new, |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned));
    let when = match line.get(crate::script_run::RUN_TICK) {
        Some(tick) => format!("tick {tick}"),
        None => format!("the game's tick {} (before this run's first)", word("tick")),
    };
    format!("specula run: {} logged at {when}: {}", word("module"), word("message"))
}

/// A command's answer as text: indented for a person at a terminal, compact (one line, no
/// spaces between its parts) for a reader through a pipe. The same JSON either way.
fn answer_text(json: &Json, for_a_person: bool) -> String {
    if for_a_person {
        json.to_pretty()
    } else {
        json.to_string()
    }
}

/// A line of a command's answer, on its standard output. That it could not be written (its
/// reader has gone) is nothing this command can say to anyone.
fn put(line: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stdout().lock(), "{line}");
}

/// A port of this machine that is free now.
pub fn free_port() -> Option<u16> {
    std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()).ok().map(|a| a.port())
}

/// A command line that is not one: the answer is JSON like every other, exit code 1 (the one
/// other place a command prints).
pub fn usage(message: &str) -> i32 {
    put(&refusal("usage", message).1.to_pretty());
    1
}

/// [`usage`], as one line: a headless run's answer is a line whatever it is.
pub fn usage_line(message: &str) -> i32 {
    put(&refusal("usage", message).1.to_string());
    1
}

/// The arguments an engine that showed the start screen starts again with, in `folder`: its
/// own, without `--launcher` and any `--project`, and the project chosen. `quiet`: the project
/// was handed over by a command, not chosen at the window: the window opens behind the others
/// (`--no-activate`), as every engine a command starts does.
pub fn respawn_args(argv: &[String], folder: &Path, quiet: bool) -> Vec<std::ffi::OsString> {
    let mut out: Vec<std::ffi::OsString> = vec!["--project".into(), folder.as_os_str().to_owned()];
    let own = &argv[..argv.iter().position(|a| a == "--").unwrap_or(argv.len())];
    if quiet && !own.iter().any(|a| a == "--no-activate") {
        out.push("--no-activate".into());
    }
    let mut words = argv.iter();
    while let Some(a) = words.next() {
        match a.as_str() {
            "--launcher" => {}
            "--project" => {
                words.next();
            }
            // (A hosted game's own arguments: kept as they are.)
            "--" => {
                out.push(a.into());
                out.extend(words.by_ref().map(Into::into));
            }
            _ => out.push(a.into()),
        }
    }
    out
}

/// Starts this executable again in the project chosen on the start screen, after the engine
/// that showed it has shut down (its port and its window are free). It runs on in this
/// process's console, in its place.
pub fn respawn(argv: &[String], folder: &Path, quiet: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("this executable's own path is not known ({e})"))?;
    std::process::Command::new(&exe).args(respawn_args(argv, folder, quiet)).spawn().map(|_| ()).map_err(|e| format!("{} could not be started again in {} ({e})", exe.display(), folder.display()))
}

/// The command line's whole answer: prints the JSON (stdout, or stderr for a failure to reach
/// an engine) and returns the exit code. The one place the commands print.
pub fn answer(cli: Cli, settings: Option<PathBuf>) -> i32 {
    let a_run = matches!(cli, Cli::Run(_) | Cli::Check(_) | Cli::Tune(_) | Cli::Find(_));
    let (code, json) = match Ctx::from_env(settings) {
        Ok(ctx) => run(cli, &ctx),
        Err(why) => refusal("no_per_user_folder", why),
    };
    // A project's checks: a line a check, and the line that counts them.
    if let Some(lines) = json.get(CHECK_LINES).and_then(Json::as_array) {
        for line in lines {
            put(&line.to_string());
        }
        return code;
    }
    // A headless run's report is the game's state: one line, alone on the output, and what the
    // game says is wrong with its own files beside it, where a reader of the state is not.
    if let Some(report) = json.get(RUN_REPORT) {
        for problem in json.get("problems").and_then(Json::as_array).unwrap_or(&[]) {
            eprintln!("specula run: {}", problem.as_str().map_or_else(|| problem.to_string(), str::to_owned));
        }
        for line in json.get("log").and_then(Json::as_array).unwrap_or(&[]) {
            eprintln!("{}", log_line(line));
        }
        // (The sounds written, beside the state: a line each, where the game's problems are said.)
        for sound in json.get(RUN_SOUNDS).and_then(Json::as_array).unwrap_or(&[]) {
            eprintln!("specula run: wrote {} ({} s)", sound.get("file").and_then(Json::as_str).unwrap_or("?"), sound.get("seconds").and_then(Json::as_f64).unwrap_or(0.0));
        }
        // The states on the way (`--at`), a line each, in order.
        for state in json.get(RUN_AT).and_then(Json::as_array).unwrap_or(&[]) {
            put(&state.to_string());
        }
        // (A followed run printed each state as it went: its last is not said twice. Nor is
        // the last of `--at` when the run ended there.)
        if json.get(RUN_FOLLOWED).is_none() && json.get(RUN_AT_IS_ALL).is_none() {
            put(&report.to_string());
        }
        return code;
    }
    // A run that could not be made, or failed: its answer is one line too, where the state
    // would have been (whoever reads a run reads its last line).
    if a_run && code == 1 {
        put(&json.to_string());
        return code;
    }
    // (`help` is words for a person: printed as text.)
    // An answer is indented for a person at a terminal and compact for whoever reads it through
    // a pipe: an agent pays for every space, and a measured comparison's builders stripped them
    // by hand and read refusals as "isnotafieldof" (2026-11-21). The JSON is the same.
    let for_a_person = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let text = json.get("help").and_then(Json::as_str).map_or_else(|| answer_text(&json, for_a_person), str::to_owned);
    if code == 2 {
        eprintln!("{text}");
    } else {
        put(&text);
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An answer through a pipe is one line with no spaces between its parts, and the same
    /// JSON as a person at a terminal is shown.
    #[test]
    fn an_answer_is_compact_through_a_pipe_and_indented_for_a_person() {
        let answer = Json::obj([("message", Json::from("'ops' is not a field of 'extrude'")), ("lods", Json::arr([Json::obj([("triangles", Json::from(288.0))])])), ("closed", Json::Bool(true))]);
        let (piped, shown) = (answer_text(&answer, false), answer_text(&answer, true));
        assert_eq!(piped, r#"{"message":"'ops' is not a field of 'extrude'","lods":[{"triangles":288}],"closed":true}"#, "compact, and the words inside a string keep their spaces");
        assert!(!piped.contains('\n') && shown.contains('\n') && shown.len() > piped.len());
        assert_eq!(Json::parse(&piped).unwrap(), Json::parse(&shown).unwrap());
    }

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-launch-run-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A check's frame is written below the project's own checks folder: where a step of its
    /// path is a link, or something other than a folder on the way or a file at the end, it
    /// is refused before a run writes there.
    #[test]
    fn a_checks_frame_is_written_nowhere_a_link_leads() {
        let dir = scratch("frame-place");
        let (project, outside) = (dir.join("game"), dir.join("outside"));
        std::fs::create_dir_all(project.join("checks").join("out")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(project.join("checks").join("runs.json"), "{}").unwrap();
        std::fs::write(project.join("checks").join("out").join("old.png"), "png").unwrap();
        let place = |frame: &str| frame_place(&project, Path::new(frame));
        // What is not there yet is the run's to make. A picture written before is removed, to
        // be written anew: never written into (it may be a second name of a file elsewhere).
        assert_eq!(place("checks/out/new/title.png"), Ok(crate::disk::canonical(&project).join("checks").join("out").join("new").join("title.png")));
        assert!(place("checks/out/old.png").is_ok() && !project.join("checks").join("out").join("old.png").exists());
        // (A hard link: the frame's name goes, and the file it was a second name of is as it was.)
        std::fs::write(outside.join("kept.txt"), "precious").unwrap();
        if std::fs::hard_link(outside.join("kept.txt"), project.join("checks").join("out").join("hard.png")).is_ok() {
            assert!(place("checks/out/hard.png").is_ok() && !project.join("checks").join("out").join("hard.png").exists());
            assert_eq!(std::fs::read_to_string(outside.join("kept.txt")).unwrap(), "precious");
        }
        // A folder where the file would be, and a file where a folder would be.
        assert!(place("checks/out").unwrap_err().contains("is not a file"));
        assert!(place("checks/runs.json/title.png").unwrap_err().contains("is not a folder"));
        // A link on the way (made where this user may make one: said and passed over where not).
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, project.join("checks").join("evil"));
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(&outside, project.join("checks").join("evil"));
        match linked {
            Ok(()) => {
                assert!(place("checks/evil/title.png").unwrap_err().contains("is a link"));
                assert!(place("checks/evil").unwrap_err().contains("is a link"));
            }
            Err(e) => eprintln!("skipped the link's part: this user can't make a link here ({e})"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ctx(dir: &Path) -> Ctx {
        // (An engine here is a port of the test's own: "serving" is "something answers there".)
        Ctx { store: Store::new(dir.join("user")), settings_given: true, home: Some(dir.join("home")), exe: PathBuf::from("specula-test-exe"), now: 1_800_000_000, alive: |i| instances::answers(i.port) }
    }

    fn engine(pid: u32, port: u16, project: Option<(&str, &str)>) -> Instance {
        Instance { launcher: false, pid, port, project: project.map(|(p, n)| (PathBuf::from(p), n.to_owned())), world: None, window: true, started: u64::from(pid), version: "0.3.0".into(), game: None }
    }

    /// `specula tools` with no filter answers with names by family, never every description
    /// (46,000 bytes in the comparison's pilot, asked for twice by one builder); a filter
    /// lists what matches with what each does.
    #[test]
    fn an_unfiltered_tools_listing_is_names_by_family() {
        let tool = |name: &str, about: &str| Json::obj([("name", Json::from(name)), ("description", Json::from(about))]);
        let long = "x".repeat(400);
        let tools = [tool("world_spawn", "Adds an entity."), tool("sight_frame", &long), tool("world_set", "Changes an entity's components."), tool("engine_info", "What is running.")];
        let all = tools_answer(7500, &tools, None);
        assert_eq!(all.get("count").and_then(Json::as_u64), Some(4));
        assert_eq!(all.get("families").map(Json::to_string).as_deref(), Some(r#"{"engine":"engine_info","sight":"sight_frame","world":"world_spawn world_set"}"#));
        assert!(all.get("tools").is_none() && !all.to_string().contains("Adds an entity"), "no descriptions without a filter: {all}");
        assert!(all.get("next").and_then(Json::as_str).is_some_and(|n| n.contains("specula tools WORD") && n.contains("tools_catalog")));
        // The engine's real list, names only, stays small: about 20 bytes a tool.
        let many: Vec<Json> = (0..300).map(|i| tool(&format!("family{}_tool_number_{i}", i % 30), &long)).collect();
        assert!(tools_answer(7500, &many, None).to_string().len() < 300 * 30, "names only");
        // A filter matches names and descriptions, whatever the case, and cuts long descriptions.
        let world = tools_answer(7500, &tools, Some("WORLD_"));
        let names: Vec<&str> = world.get("tools").and_then(Json::as_array).unwrap().iter().filter_map(|t| t.get("name").and_then(Json::as_str)).collect();
        assert_eq!(names, ["world_spawn", "world_set"]);
        let by_text = tools_answer(7500, &tools, Some("components"));
        assert_eq!(by_text.get("count").and_then(Json::as_u64), Some(1));
        let frame = tools_answer(7500, &tools, Some("sight"));
        let about = frame.get("tools").and_then(Json::as_array).unwrap()[0].get("description").and_then(Json::as_str).unwrap().len();
        assert_eq!(about, DESCRIPTION_CHARS);
        assert_eq!(tools_answer(7500, &tools, Some("nothing-has-this")).get("count").and_then(Json::as_u64), Some(0));
        // A tool's own name shows that tool whole: its description uncut and its arguments,
        // and the names of the others the word is in.
        let schema = Json::parse(r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#).unwrap();
        let mut with_schema = tools.to_vec();
        with_schema.push(Json::obj([("name", Json::from("sight_frame_wide")), ("description", Json::from("A wide one."))]));
        with_schema[1] = Json::obj([("name", Json::from("sight_frame")), ("description", Json::from(long.as_str())), ("inputSchema", schema.clone())]);
        let one = tools_answer(7500, &with_schema, Some("Sight_Frame"));
        assert_eq!(one.get("tool").and_then(Json::as_str), Some("sight_frame"));
        assert_eq!(one.get("description").and_then(Json::as_str).map(str::len), Some(400), "the whole description");
        assert_eq!(one.get("arguments"), Some(&schema));
        assert_eq!(one.get("also").map(Json::to_string).as_deref(), Some(r#"["sight_frame_wide"]"#));
        assert!(one.get("tools").is_none());
        assert!(all.get("next").and_then(Json::as_str).is_some_and(|n| n.contains("specula tools NAME")));
    }

    /// `specula call TOOL @FILE` (the measured comparison, 2026-11-14: a JSON object quoted for
    /// a shell is the hard part of calling a tool from a terminal): the file's one object is
    /// the call's arguments; a file that is missing, too large, not text or not one object is
    /// the command's own mistake, said before any engine is looked for.
    #[test]
    fn a_calls_arguments_are_read_from_a_file() {
        let dir = scratch("args-file");
        let file = |name: &str, bytes: &[u8]| {
            let f = dir.join(name);
            std::fs::write(&f, bytes).unwrap();
            f
        };
        let good = file("good.json", b"{\n  \"path\": \"/a b\",\n  \"limit\": 5\n}\n");
        assert_eq!(args_file(&good).map(|j| j.to_string()), Ok(r#"{"path":"/a b","limit":5}"#.to_owned()));
        // (What an editor or a shell's redirection may put first.)
        let marked = file("marked.json", b"\xef\xbb\xbf{\"limit\": 5}");
        assert_eq!(args_file(&marked).map(|j| j.to_string()), Ok(r#"{"limit":5}"#.to_owned()));
        assert!(args_file(&dir.join("none.json")).unwrap_err().starts_with(&format!("@{}", dir.join("none.json").display())));
        assert!(args_file(&file("list.json", b"[1]")).unwrap_err().contains("one JSON object"));
        assert!(args_file(&file("bad.json", b"{not json")).unwrap_err().contains("not JSON"));
        assert!(args_file(&file("bytes.json", b"{\"a\": \"\xff\"}")).unwrap_err().contains("not UTF-8"));
        // One byte past the most a call may carry: refused by its size, and no more is read.
        let mut big = b"{\"t\": \"".to_vec();
        big.resize(crate::launch_cli::MAX_ARGS_BYTES - 1, b'x');
        big.extend_from_slice(b"\"}");
        assert_eq!(big.len(), crate::launch_cli::MAX_ARGS_BYTES + 1);
        assert!(args_file(&file("big.json", &big)).unwrap_err().contains("more than"));
        big.remove(10);
        assert!(args_file(&file("fits.json", &big)).is_ok(), "exactly the most is taken");
        // Through the command: a bad file is `usage` (exit 1) with no engine running, where a
        // good one gets as far as finding none (exit 2).
        let c = ctx(&dir);
        let call = |f: PathBuf| run(Cli::Call { target: Target::Any, tool: "world_list".into(), args: CallArgs::File(f), as_name: None, role: None }, &c);
        let (code, j) = call(dir.join("none.json"));
        assert!(code == 1 && j.get("error").and_then(Json::as_str) == Some("usage"), "{j}");
        let (code, j) = call(good);
        assert_eq!(code, 2, "{j}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The owner's words, 2026-11-13: no project before, an empty list; a project worked on is
    /// listed; the last one worked on is first.
    #[test]
    fn projects_are_listed_most_recent_first_and_an_unused_machine_lists_none() {
        let dir = scratch("projects");
        let c = ctx(&dir);
        let (code, j) = run(Cli::Projects, &c);
        assert_eq!((code, j.to_string()), (0, r#"{"projects":[],"count":0}"#.to_owned()));
        // `new` makes the folder under the home's projects folder and lists it.
        let (code, j) = run(Cli::New { name: "crypt".into(), parent: None, bare: true }, &c);
        assert_eq!(code, 0, "{j}");
        let crypt = dir.join("home").join(PROJECTS_FOLDER).join("crypt");
        assert!(crypt.join("assets").is_dir() && crypt.join(crate::launch_list::MARKER).is_file());
        let (_, j) = run(Cli::New { name: "frost".into(), parent: Some(dir.join("elsewhere")), bare: true }, &c);
        assert!(dir.join("elsewhere").join("frost").is_dir(), "{j}");
        let names = |j: &Json| j.get("projects").and_then(Json::as_array).unwrap().iter().map(|p| p.get("name").and_then(Json::as_str).unwrap().to_owned()).collect::<Vec<_>>();
        let (_, j) = run(Cli::Projects, &c);
        assert_eq!(names(&j), ["frost", "crypt"]);
        assert_eq!(j.get("last").and_then(Json::as_str), Some("frost"));
        // A name in use is refused; a folder that is gone is still listed, as missing.
        let (code, j) = run(Cli::New { name: "crypt".into(), parent: None, bare: true }, &c);
        assert!(code == 1 && j.to_string().contains("not empty"), "{j}");
        std::fs::remove_dir_all(dir.join("elsewhere")).unwrap();
        let (_, j) = run(Cli::Projects, &c);
        let frost = &j.get("projects").and_then(Json::as_array).unwrap()[0];
        assert_eq!(frost.get("missing").and_then(Json::as_bool), Some(true));
        let (code, j) = run(Cli::Open { which: Which::Word("frost".into()), front: false, engine_args: vec![] }, &c);
        assert!(code == 1 && j.get("error").and_then(Json::as_str) == Some("missing"), "{j}");
        // Forgetting takes it off the list; the other stays, and is the last now.
        assert_eq!(run(Cli::Forget { which: "frost".into() }, &c).0, 0);
        let (_, j) = run(Cli::Projects, &c);
        assert_eq!((names(&j), j.get("last").and_then(Json::as_str)), (vec!["crypt".to_owned()], Some("crypt")));
        assert_eq!(run(Cli::Forget { which: "frost".into() }, &c).1.get("error").and_then(Json::as_str), Some("not_listed"));
        // `sdk` writes this engine's SDK into a project named by its name or its folder; a
        // second time there is nothing to write; a folder whose SDK folders hold anything else
        // is refused whole.
        let (code, j) = run(Cli::Sdk { which: Some("crypt".into()) }, &c);
        assert_eq!(code, 0, "{j}");
        assert_eq!(j.get("sdk").and_then(|s| s.get("written")).and_then(Json::as_u64), Some(specula_logic::sdk_source::FILES.len() as u64), "{j}");
        assert!(specula_logic::sdk_source::is_the_engines(&crypt));
        let (code, j) = run(Cli::Sdk { which: Some(crypt.to_string_lossy().into_owned()) }, &c);
        assert!(code == 0 && j.get("note").is_some() && j.get("sdk").and_then(|s| s.get("written")).and_then(Json::as_u64) == Some(0), "{j}");
        std::fs::write(crypt.join("crates/specula-sdk/src/mine.rs"), "// someone's").unwrap();
        let (code, j) = run(Cli::Sdk { which: Some("crypt".into()) }, &c);
        assert!(code == 1 && j.get("error").and_then(Json::as_str) == Some("other_files") && crypt.join("crates/specula-sdk/src/mine.rs").exists(), "{j}");
        assert_eq!(run(Cli::Sdk { which: Some("no-such-project".into()) }, &c).1.get("error").and_then(Json::as_str), Some("not_listed"));
        // Nothing to resume on a machine that has no project: said, with what to do.
        let empty = ctx(&dir.join("fresh"));
        let (code, j) = run(Cli::Open { which: Which::Last, front: false, engine_args: vec![] }, &empty);
        assert!(code == 1 && j.get("error").and_then(Json::as_str) == Some("no_projects"), "{j}");
        let (code, j) = run(Cli::List, &empty);
        assert_eq!((code, j.to_string()), (0, r#"{"engines":[],"count":0}"#.to_owned()));
        // Nothing to stop, and an engine that is not listed here, are said.
        let (code, j) = run(Cli::Stop { target: Target::Any }, &empty);
        assert!(code == 2 && j.get("error").and_then(Json::as_str) == Some("no_engine"), "{j}");
        let (code, j) = run(Cli::Stop { target: Target::Port(7999) }, &empty);
        assert!(code == 1 && j.get("error").and_then(Json::as_str) == Some("not_listed"), "{j}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_finds_its_engine_by_port_project_or_being_the_only_one() {
        let mut list = ProjectList::default();
        for (path, name, at) in [("C:/games/crypt", "crypt", 1), ("C:/a/demo", "demo", 2), ("C:/b/demo", "demo", 3)] {
            list.opened(Project { path: PathBuf::from(path), name: name.into(), exe: None, last_world: None, last_opened: at, version: String::new() });
        }
        let (crypt, plain) = (engine(10, 7471, Some(("C:/games/crypt", "crypt"))), engine(20, 7500, None));
        let code = |r: Result<Instance, (i32, Json)>| r.map(|i| i.port).map_err(|(c, j)| (c, j.get("error").and_then(Json::as_str).unwrap_or("").to_owned()));
        // No engine, one engine, several.
        assert_eq!(code(pick_engine(&Target::Any, &[], &list)), Err((2, "no_engine".into())));
        assert_eq!(code(pick_engine(&Target::Any, std::slice::from_ref(&plain), &list)), Ok(7500));
        assert_eq!(code(pick_engine(&Target::Any, &[crypt.clone(), plain.clone()], &list)), Err((1, "several_engines".into())));
        // By port: the engine listed there, or the port as given (the call says if none answers).
        assert_eq!(code(pick_engine(&Target::Port(7500), &[crypt.clone(), plain.clone()], &list)), Ok(7500));
        assert_eq!(code(pick_engine(&Target::Port(9000), std::slice::from_ref(&crypt), &list)), Ok(9000));
        // By project: its name, its folder, whatever the case; not running, and a shared name.
        for word in ["crypt", "CRYPT", "C:/games/crypt", "C:\\games\\crypt\\"] {
            assert_eq!(code(pick_engine(&Target::Project(word.into()), &[plain.clone(), crypt.clone()], &list)), Ok(7471), "{word}");
        }
        assert_eq!(code(pick_engine(&Target::Project("crypt".into()), std::slice::from_ref(&plain), &list)), Err((2, "not_running".into())));
        assert_eq!(code(pick_engine(&Target::Project("demo".into()), std::slice::from_ref(&crypt), &list)), Err((1, "ambiguous".into())));
        // A project the list has dropped is still found by what its engine says it has open.
        let stray = engine(30, 7600, Some(("D:/stray", "stray")));
        assert_eq!(code(pick_engine(&Target::Project("stray".into()), &[stray], &list)), Ok(7600));
    }

    #[test]
    fn open_hands_the_project_to_a_waiting_start_screen_and_starts_an_engine_when_refused() {
        let dir = scratch("handover");
        let c = ctx(&dir);
        let (_, j) = run(Cli::New { name: "crypt".into(), parent: None, bare: true }, &c);
        let crypt = PathBuf::from(j.get("project").and_then(|p| p.get("path")).and_then(Json::as_str).unwrap());
        // A start screen and, later, the engine it became: "serving" is a port of this test's.
        let (screen, after) = (std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap(), std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap());
        let port = |l: &std::net::TcpListener| l.local_addr().unwrap().port();
        let waiting = Instance { launcher: true, ..engine(901, port(&screen), None) };
        let open = |engine_args: Vec<String>| Cli::Open { which: Which::Word("crypt".into()), front: false, engine_args };
        // `specula open crypt` while the start screen's side does `then` with the hand-over.
        let handed = |then: &(dyn Fn() + Sync)| {
            instances::write(&c.store.dir, &waiting).unwrap();
            std::thread::scope(|s| {
                let call = s.spawn(|| run(open(vec![]), &c));
                let deadline = Instant::now() + specula_core::speed_budget(Duration::from_secs(5));
                // (As the engine's watcher waits: for a hand-over that is written. The file is
                // made, then written; taken in between it is empty and names nothing, which is
                // what this test did once in three runs on a busy machine.)
                while !instances::open_waits(&c.store.dir, 901) {
                    assert!(Instant::now() < deadline, "the hand-over never arrived");
                    std::thread::sleep(Duration::from_millis(10));
                }
                assert_eq!(instances::take_open(&c.store.dir, 901), Some((crypt.clone(), false)), "the project's folder is what is handed over");
                then();
                call.join().unwrap()
            })
        };
        let error = |j: &Json| j.get("error").and_then(Json::as_str).map(str::to_owned);
        // Refused (its screen was closed, or its world is in use): the command starts an engine
        // itself. This test's executable is not one: that it was tried is the proof.
        let (code, j) = handed(&|| instances::refuse_open(&c.store.dir, 901));
        assert_eq!((code, error(&j)), (1, Some("not_started".into())), "{j}");
        assert!(!instances::refused_file(&c.store.dir, 901).exists(), "the refusal was read");
        // Taken: the start screen's engine goes and one with the project serves; the command
        // answers with that engine and starts none.
        let became = engine(902, port(&after), Some((crypt.to_str().unwrap(), "crypt")));
        let (code, j) = handed(&|| {
            instances::remove(&c.store.dir, 901);
            instances::write(&c.store.dir, &became).unwrap();
        });
        assert_eq!(code, 0, "{j}");
        assert_eq!((j.get("pid").and_then(Json::as_u64), j.get("from_start_screen").and_then(Json::as_bool), j.get("started").and_then(Json::as_bool)), (Some(902), Some(true), Some(true)));
        // With engine flags the start screen is not asked (it keeps its own): nothing is handed.
        instances::remove(&c.store.dir, 902);
        instances::write(&c.store.dir, &waiting).unwrap();
        let (code, j) = run(open(vec!["--headless".into()]), &c);
        assert_eq!((code, error(&j)), (1, Some("not_started".into())), "{j}");
        assert!(!instances::open_file(&c.store.dir, 901).exists());
        // An engine with a project, or one that shows no start screen, is never handed one.
        instances::write(&c.store.dir, &Instance { launcher: false, ..waiting.clone() }).unwrap();
        let (_, j) = run(open(vec![]), &c);
        assert!(error(&j) == Some("not_started".into()) && !instances::open_file(&c.store.dir, 901).exists(), "{j}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review R1-2 and R1-15.
    #[test]
    fn a_project_an_engine_holds_is_found_never_opened_twice() {
        let dir = scratch("held");
        let c = ctx(&dir);
        let (_, j) = run(Cli::New { name: "crypt".into(), parent: None, bare: true }, &c);
        let crypt = PathBuf::from(j.get("project").and_then(|p| p.get("path")).and_then(Json::as_str).unwrap());
        let open = || Cli::Open { which: Which::Word("crypt".into()), front: false, engine_args: vec!["--headless".into()] };
        let error = |j: &Json| j.get("error").and_then(Json::as_str).map(str::to_owned);
        // Nobody has it: the command starts an engine (this test's executable is not one).
        assert_eq!(error(&run(open(), &c).1), Some("not_started".into()));
        if !crate::launch_engine::locks_here() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        // An engine holds it and says so a moment later (it was starting): that engine is the
        // answer, and none is started.
        let held = crate::launch_engine::lock(&crypt).unwrap();
        let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let holder = engine(903, port.local_addr().unwrap().port(), Some((crypt.to_str().unwrap(), "crypt")));
        let (code, j) = std::thread::scope(|s| {
            let call = s.spawn(|| run(open(), &c));
            std::thread::sleep(Duration::from_millis(300));
            instances::write(&c.store.dir, &holder).unwrap();
            call.join().unwrap()
        });
        assert_eq!((code, j.get("pid").and_then(Json::as_u64), j.get("started").and_then(Json::as_bool)), (0, Some(903), Some(false)), "{j}");
        // The same folder by another spelling is the same project (review R2-1: opened through
        // a junction, it waited out the lock and said the engine was "not listed here").
        let link = dir.join("link-to-crypt");
        let linked = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&crypt).output().is_ok_and(|o| o.status.success());
        if linked {
            assert!(!same_path(&link, &crypt) && store::same_folder(&link, &crypt));
            let (code, j) = run(Cli::Open { which: Which::Word(link.to_string_lossy().into_owned()), front: false, engine_args: vec!["--headless".into()] }, &c);
            assert_eq!((code, j.get("pid").and_then(Json::as_u64), j.get("started").and_then(Json::as_bool)), (0, Some(903), Some(false)), "{j}");
            let _ = std::fs::remove_dir(&link);
        }
        // The holder is not listed here and its lock goes while the command waits: said, and
        // nothing is started behind the person's back.
        instances::remove(&c.store.dir, 903);
        let (code, j) = std::thread::scope(|s| {
            let call = s.spawn(|| run(open(), &c));
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
            call.join().unwrap()
        });
        assert_eq!((code, error(&j)), (1, Some("in_use".into())), "{j}");
        // The executable that opened a project last is used only when it is a game's own (another
        // name than this program's): a copy of this program elsewhere is not.
        let (mut list, _) = c.store.load();
        let own_copy = dir.join("elsewhere").join("specula-test-exe");
        std::fs::create_dir_all(own_copy.parent().unwrap()).unwrap();
        std::fs::write(&own_copy, "").unwrap();
        list.projects[0].exe = Some(own_copy);
        c.store.save(&list).unwrap();
        let (_, j) = run(open(), &c);
        assert!(j.to_string().contains("specula-test-exe could not be started") && !j.to_string().contains("elsewhere"), "{j}");
        crate::launch_engine::forget_lock_for_test(&crypt);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_engine_starts_again_with_its_own_flags_in_the_project_chosen() {
        let with = |argv: &[&str], quiet: bool| respawn_args(&argv.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(), Path::new("D:/games/crypt"), quiet).into_iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let args = |argv: &[&str]| with(argv, false);
        // Handed over by a command (review R1-13): the window opens behind the others, once.
        assert_eq!(with(&[], true), ["--project", "D:/games/crypt", "--no-activate"]);
        assert_eq!(with(&["--no-activate", "--port", "7663"], true), ["--project", "D:/games/crypt", "--no-activate", "--port", "7663"]);
        assert_eq!(with(&["--", "--no-activate"], true), ["--project", "D:/games/crypt", "--no-activate", "--", "--no-activate"], "a game's own flag is not the engine's");
        assert_eq!(args(&[]), ["--project", "D:/games/crypt"]);
        assert_eq!(args(&["--launcher", "--port", "7663", "--settings", "D:/u", "--no-activate"]), ["--project", "D:/games/crypt", "--port", "7663", "--settings", "D:/u", "--no-activate"]);
        // Its own --project (it showed the start screen all the same) is replaced, not doubled.
        assert_eq!(args(&["--project", "D:/old", "--launcher", "--adapter", "integrated"]), ["--project", "D:/games/crypt", "--adapter", "integrated"]);
        assert_eq!(args(&["--project"]), ["--project", "D:/games/crypt"], "a flag with no value ends the list");
        // What follows -- is a hosted game's: never read as the engine's flags.
        assert_eq!(args(&["--launcher", "--", "--launcher", "--project", "x"]), ["--project", "D:/games/crypt", "--", "--launcher", "--project", "x"]);
        let env = |k: &str| (k == "HOME" || k == WINDOWS_HOME).then(|| "C:/people/ann".to_owned());
        assert_eq!(default_parent(env), Some(PathBuf::from("C:/people/ann").join(PROJECTS_FOLDER)));
        assert_eq!(default_parent(|_| Some("  ".into())), None);
        assert_eq!(default_parent(|_| None), None);
    }

    #[test]
    fn exit_codes_tell_no_engine_from_an_engines_refusal() {
        let code = |c: &'static str| unreachable_engine(&ClientError { code: c, message: "m".into() }).0;
        assert_eq!((code("no_engine"), code("timeout"), code("bad_answer"), code("http")), (2, 2, 2, 2));
        assert_eq!(code("rpc"), 1, "the engine answered: it has no such tool");
        assert_eq!(refusal("missing", "x").0, 1);
    }

    #[test]
    fn a_listed_project_says_when_its_engine_is_running_and_when_its_folder_is_gone() {
        let p = Project { path: PathBuf::from("C:/games/crypt"), name: "crypt".into(), exe: None, last_world: Some("night".into()), last_opened: 5, version: "0.3.0".into() };
        let running = [engine(10, 7471, Some(("C:/games/crypt/", "crypt")))];
        let here = project_json(&p, &running, |_| true);
        assert_eq!(here.get("running").and_then(|r| r.get("port")).and_then(Json::as_u64), Some(7471));
        assert!(here.get("missing").is_none());
        let gone = project_json(&p, &[], |_| false);
        assert!(gone.get("running").is_none() && gone.get("missing").and_then(Json::as_bool) == Some(true));
    }
}

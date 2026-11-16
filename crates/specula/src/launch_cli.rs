//! The command line's words (docs/design/LAUNCH.md): what `specula projects`, `specula open`,
//! `specula list`, `specula tools` and `specula call` mean, read from the arguments. An argument
//! list that starts with a flag (or is empty) is the engine's own, as before.
//!
//! Reading only: this module decides what was asked and says what is wrong with it; the engine's
//! start-up code (`launch_run`) does it.

use specula_core::Json;
use std::path::PathBuf;

/// The longest tool name, and the largest arguments, a `call` takes from the command line.
pub const MAX_TOOL_CHARS: usize = 64;
pub const MAX_ARGS_BYTES: usize = 1024 * 1024;
pub const MAX_ROLE_CHARS: usize = 32;

/// Which project a word means.
#[derive(Clone, Debug, PartialEq)]
pub enum Which {
    /// The one last worked on (the first of the list).
    Last,
    /// By name, or by its folder.
    Word(String),
}

/// Which running engine a command speaks to.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// The only one running (several: the command says which there are).
    Any,
    Port(u16),
    /// The one that has this project open (by name or folder).
    Project(String),
}

/// What `specula run` was asked.
#[derive(Clone, Debug, PartialEq)]
pub struct RunArgs {
    /// The project's name or folder (`None`: the folder the command is run in).
    pub which: Option<String>,
    pub ticks: Option<u64>,
    pub seed: Option<u64>,
    pub inputs: Vec<(u64, String)>,
    pub frame: Option<PathBuf>,
    pub size: Option<(u32, u32)>,
    /// After the script, steps are read from standard input (`--follow`).
    pub follow: bool,
}

/// What `specula check` was asked.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckArgs {
    /// The project's name or folder (`None`: the folder the command is run in).
    pub which: Option<String>,
    /// The checks file (`None`: the project's `checks/runs.json`).
    pub file: Option<PathBuf>,
    /// Only the check of this name.
    pub only: Option<String>,
}

/// One command.
#[derive(Clone, Debug, PartialEq)]
pub enum Cli {
    /// The projects worked on, most recent first.
    Projects,
    /// The engines running on this machine.
    List,
    /// Asks a running engine to stop (it shuts down cleanly, as on Ctrl+C).
    Stop { target: Target },
    /// Makes a project folder and lists it.
    /// `bare`: only the folder, its assets folder and its marker (what a new project was before
    /// it built and played: docs/design/FIRST_HOUR.md, slice 7).
    New { name: String, parent: Option<PathBuf>, bare: bool },
    /// Opens a project in an engine (or finds the engine that has it open). `front`: its
    /// window opens in front and takes the keyboard (a person asked to see it); else behind
    /// the other windows, taking nothing from whoever is using the machine.
    Open { which: Which, front: bool, engine_args: Vec<String> },
    /// Takes a project off the list (its folder is not touched).
    Forget { which: String },
    /// Writes this engine's SDK into a project (`which`: its name or folder; `None`: the
    /// folder the command is run in), so that its modules build against the engine that runs
    /// them (docs/design/FIRST_HOUR.md).
    Sdk { which: Option<String> },
    /// Plays a project's game with no window, by a script of inputs, and prints how it stands
    /// (docs/design/FIRST_HOUR.md, "A headless run").
    Run(RunArgs),
    Check(CheckArgs),
    /// The tools a running engine lists.
    Tools { target: Target, filter: Option<String> },
    /// Calls one tool of a running engine.
    Call { target: Target, tool: String, args: CallArgs, as_name: Option<String>, role: Option<String> },
    Help,
}

/// A call's arguments: given on the command line, or in a file the command reads (`@FILE`).
#[derive(Clone, Debug, PartialEq)]
pub enum CallArgs {
    Given(Json),
    File(PathBuf),
}

/// A call's arguments from their text: one JSON object, at most [`MAX_ARGS_BYTES`].
pub fn call_args(text: &str) -> Result<Json, String> {
    if text.len() > MAX_ARGS_BYTES {
        return Err(format!("the arguments are {} bytes; at most {MAX_ARGS_BYTES}", text.len()));
    }
    let hint = "pass one object, quoted for your shell, like '{\"limit\": 5}', or put it in a file: specula call TOOL @args.json";
    let j = Json::parse(text).map_err(|e| format!("the arguments are not JSON ({e}): {hint}"))?;
    if !matches!(j, Json::Obj(_)) {
        return Err("the arguments must be one JSON object, like '{\"limit\": 5}'".to_owned());
    }
    Ok(j)
}

pub const HELP: &str = "\
specula                          the start screen: your projects, most recent first
specula projects                 the projects worked on (JSON), most recent first
specula open --last              open the project last worked on (or find the engine that has it open)
specula open NAME|FOLDER [--front] [-- engine flags]
                                 (its window opens behind the others; --front: in front)
specula play NAME|FOLDER|--last [--as-is] [-- engine flags]
                                 open a project to play it: its window in front, and the
                                 modules its game file names built and loaded as their owner
                                 or a granted lead last built them on this machine (--run).
                                 --as-is: whatever is on disk now, whoever wrote it
                                 (--run-as-is): say it only for code that is yours to run. A
                                 module is native code with your user's rights
specula run [NAME|FOLDER] --ticks N [--inputs T:WORD,T:WORD,... | --inputs @FILE] [--seed N]
            [--settings FOLDER] [--frame FILE.png [--size WxH]] [--follow]
                                 play a project's game with no window, from its start, and
                                 print how it stands as one JSON object (the game file's
                                 \"report\", or everything). Its modules are built first, as
                                 they are on disk (as play --as-is: your code to run). An
                                 input is a word at a tick: up down left right enter escape,
                                 an action of a module (held until the next input; an axis
                                 with its value: throttle=0.5, move=1/0), words joined by +
                                 (held together), or none. A source (Key.Enter) presses the
                                 interface only. --seed: 1 when left out (a run is the same
                                 run every time). --settings: where this run's player
                                 settings are read and kept (none: kept nowhere). A module
                                 whose call failed is an error (which, at which tick), never
                                 a state, and what the modules logged is said beside it.
                                 --follow: after the script (--ticks may be left out), the
                                 state is printed and steps are read from standard input, a
                                 line each: TICKS, or TICKS WORD (10 up+shoot: the input,
                                 then ten ticks); the state is printed after each, and the
                                 run ends with the input: a player that reads the state and
                                 answers it plays a whole game in one run. A run ends after
                                 30 minutes at most
specula check [NAME|FOLDER] [--only NAME] [--file FILE]
                                 play every run the project's checks/runs.json lists, each as
                                 `run` plays one (ticks, inputs, seed, a settings folder by
                                 name that checks share, a frame), and hold each state
                                 against the check's \"expect\": a line a check, ok or what
                                 differs with the state, then a line that counts them. Exit
                                 code 1 when one did not hold. A check with no \"expect\"
                                 prints its state: many runs read in one command. A check's
                                 settings are the folder it names (the command's own, gone
                                 with it) or none, never --settings' (which only says where
                                 the list of projects is); its frame is a PNG below the
                                 project's checks folder. --only: that check alone
specula new NAME [--in FOLDER] [--bare]
                                 make a project that builds and plays (a game file, screens,
                                 words, a module, the SDK, notes) and list it; --bare: only
                                 the folder, its assets folder and its marker
specula forget NAME|FOLDER       take a project off the list (its folder stays)
specula sdk [NAME|FOLDER]        write this engine's SDK into a project (no word: the folder
                                 you are in), so its modules build against this engine
specula list                     the engines running on this machine (JSON)
specula stop [--port N | --project NAME]
                                 ask a running engine to stop (it saves and shuts down cleanly)
specula tools [FILTER] [--port N | --project NAME]
                                 (no FILTER: every tool's name, by family; a tool's name:
                                 what it does and the arguments it takes; a word: the
                                 tools that match, and what each does)
specula call TOOL ['{json}' | @FILE] [--port N | --project NAME] [--as NAME [--role ROLE]]
                                 (@FILE: the arguments, one JSON object, read from that file;
                                 --role: the role NAME says, such as lead when the engine was
                                 started with --lead NAME)
specula --help                   the engine's own flags";

const WORDS: [&str; 13] = ["projects", "open", "play", "run", "check", "new", "forget", "sdk", "list", "stop", "tools", "call", "help"];

/// The flags a command takes after its own words: `--port N`, `--project NAME`, `--as NAME`,
/// `--in FOLDER`, `--last`. Returns what is left (the command's own words, in order).
struct Flags {
    front: bool,
    port: Option<u16>,
    project: Option<String>,
    as_name: Option<String>,
    role: Option<String>,
    parent: Option<PathBuf>,
    last: bool,
    bare: bool,
    as_is: bool,
    rest: Vec<String>,
}

fn flags(words: &[String], allowed: &[&str]) -> Result<Flags, String> {
    let mut f = Flags { front: false, port: None, project: None, as_name: None, role: None, parent: None, last: false, bare: false, as_is: false, rest: Vec::new() };
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        if !w.starts_with("--") {
            f.rest.push(w.to_owned());
            i += 1;
            continue;
        }
        if !allowed.contains(&w) {
            return Err(format!("'{w}' is not a flag of this command (it takes: {})", if allowed.is_empty() { "none".to_owned() } else { allowed.join(", ") }));
        }
        if w == "--as-is" {
            f.as_is = true;
            i += 1;
            continue;
        }
        if w == "--bare" {
            f.bare = true;
            i += 1;
            continue;
        }
        if w == "--last" || w == "--front" {
            if w == "--last" {
                f.last = true;
            } else {
                f.front = true;
            }
            i += 1;
            continue;
        }
        let value = words.get(i + 1).filter(|v| !v.starts_with("--")).ok_or_else(|| format!("'{w}' needs a value"))?.clone();
        match w {
            "--port" => f.port = Some(value.parse::<u16>().ok().filter(|p| *p != 0).ok_or_else(|| format!("'--port' is a port number, 1..65535 (got '{value}')"))?),
            "--project" => f.project = Some(value),
            "--as" => {
                // (An agent's name, as agent_hello takes it: plain ASCII, short.)
                if value.is_empty() || value.len() > 64 || !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
                    return Err(format!("'--as' is an agent's name: 1..64 of A-Z a-z 0-9 _ - . (got '{value}')"));
                }
                f.as_name = Some(value);
            }
            "--role" => {
                // (A role, as agent_hello takes it: a short word.)
                if value.is_empty() || value.len() > MAX_ROLE_CHARS || !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')) {
                    return Err(format!("'--role' is a role's word, like lead or builder: 1..{MAX_ROLE_CHARS} of A-Z a-z 0-9 _ - (got '{value}')"));
                }
                f.role = Some(value);
            }
            "--in" => f.parent = Some(PathBuf::from(value)),
            _ => unreachable!("allowed lists only these"),
        }
        i += 2;
    }
    if f.port.is_some() && f.project.is_some() {
        return Err("give '--port' or '--project', not both".to_owned());
    }
    Ok(f)
}

fn target(f: &Flags) -> Target {
    match (&f.port, &f.project) {
        (Some(p), _) => Target::Port(*p),
        (None, Some(n)) => Target::Project(n.clone()),
        (None, None) => Target::Any,
    }
}

/// Takes a command's `--settings FOLDER` out of its arguments (anywhere before a `--`): the
/// per-user folder the command reads the project list and the running engines from, as the
/// engine's own `--settings` names where it keeps them. `off` is the engine's word for "keep
/// nothing": a command has nothing to read then, so it is refused here.
pub fn take_settings(args: &[String]) -> Result<(Vec<String>, Option<PathBuf>), String> {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let Some(at) = args[..end].iter().position(|a| a == "--settings") else { return Ok((args.to_vec(), None)) };
    let value = args[..end].get(at + 1).filter(|v| !v.starts_with("--")).ok_or("'--settings' needs a folder")?;
    if value == "off" {
        return Err("'--settings off' keeps no project list and no running engines' files, so a command has nothing to read: give the folder, or leave the flag out".to_owned());
    }
    let mut rest = args.to_vec();
    rest.drain(at..at + 2);
    Ok((rest, Some(PathBuf::from(value))))
}

/// Reads the arguments after the executable's name. `Ok(None)`: they are the engine's own (no
/// arguments, or the first is a flag). An unknown first word is an error that lists the words.
pub fn parse(args: &[String]) -> Result<Option<Cli>, String> {
    let Some(first) = args.first() else { return Ok(None) };
    if first.starts_with('-') {
        return Ok(None);
    }
    let rest = &args[1..];
    let exactly = |f: &Flags, n: usize, what: &str| if f.rest.len() == n { Ok(()) } else { Err(format!("'specula {first}' takes {what} (got {} word(s))", f.rest.len())) };
    let cli = match first.as_str() {
        "projects" => {
            exactly(&flags(rest, &[])?, 0, "no words")?;
            Cli::Projects
        }
        "list" => {
            exactly(&flags(rest, &[])?, 0, "no words")?;
            Cli::List
        }
        "help" => Cli::Help,
        "stop" => {
            let f = flags(rest, &["--port", "--project"])?;
            exactly(&f, 0, "no words: --port N or --project NAME says which engine")?;
            Cli::Stop { target: target(&f) }
        }
        "new" => {
            let f = flags(rest, &["--in", "--bare"])?;
            exactly(&f, 1, "one word: the new project's name")?;
            if !crate::launch_list::valid_name(&f.rest[0]) {
                return Err(format!("'{}' is not a project name: 1..{} characters, none of / \\ : * ? \" < > |", f.rest[0], crate::launch_list::MAX_NAME_CHARS));
            }
            Cli::New { name: f.rest[0].clone(), parent: f.parent, bare: f.bare }
        }
        "forget" => {
            let f = flags(rest, &[])?;
            exactly(&f, 1, "one word: the project's name or folder")?;
            Cli::Forget { which: f.rest[0].clone() }
        }
        // A headless run of a project's game by a script of inputs (docs/design/FIRST_HOUR.md).
        "run" => {
            let mut run = RunArgs { which: None, ticks: None, seed: None, inputs: Vec::new(), frame: None, size: None, follow: false };
            let mut i = 0;
            while i < rest.len() {
                let w = rest[i].as_str();
                if !w.starts_with("--") {
                    if run.which.replace(w.to_owned()).is_some() {
                        return Err("'specula run' takes one project: its name or its folder (none: the folder you are in)".to_owned());
                    }
                    i += 1;
                    continue;
                }
                if w == "--follow" {
                    run.follow = true;
                    i += 1;
                    continue;
                }
                const TAKES: [&str; 5] = ["--ticks", "--seed", "--inputs", "--frame", "--size"];
                if !TAKES.contains(&w) {
                    return Err(format!("'{w}' is not a flag of this command (it takes: {}, --follow, --settings)", TAKES.join(", ")));
                }
                let value = rest.get(i + 1).ok_or_else(|| format!("'{w}' needs a value"))?.as_str();
                match w {
                    "--ticks" => run.ticks = Some(value.parse::<u64>().ok().filter(|t| *t <= crate::script_run::MAX_TICKS).ok_or_else(|| format!("'--ticks' is a whole number up to {} (got '{value}')", crate::script_run::MAX_TICKS))?),
                    "--seed" => run.seed = Some(value.parse::<u64>().ok().filter(|s| *s <= crate::script_run::MAX_SEED).ok_or_else(|| format!("'--seed' is a whole number up to {} (got '{value}')", crate::script_run::MAX_SEED))?),
                    "--inputs" => {
                        // (`@FILE`: the script in a file, an input a line or with commas.)
                        let text = match value.strip_prefix('@') {
                            Some(file) => std::fs::read_to_string(file).map_err(|e| format!("'--inputs @{file}': {e}"))?,
                            None => value.to_owned(),
                        };
                        run.inputs = crate::script_run::parse_inputs(&text).map_err(|e| format!("'--inputs': {e}"))?;
                    }
                    "--frame" => run.frame = Some(PathBuf::from(value)),
                    _ => {
                        let side = |s: &str| s.parse::<u32>().ok().filter(|n| crate::script_run::FRAME_SIDE.contains(n));
                        run.size = Some(value.split_once(['x', 'X']).and_then(|(w, h)| Some((side(w)?, side(h)?))).ok_or_else(|| format!("'--size' is WIDTHxHEIGHT, each 64 to 2048 (got '{value}')"))?);
                    }
                }
                i += 2;
            }
            // (A followed run's ticks are its steps': the script before them may be none.)
            if run.follow && run.ticks.is_none() {
                run.ticks = Some(run.inputs.last().map_or(0, |(at, _)| *at));
            }
            if run.ticks.is_none() {
                return Err("'specula run' needs --ticks N: how many ticks to play (0: only the start), or --follow: steps read from standard input".to_owned());
            }
            if let Some(ticks) = run.ticks {
                crate::script_run::check_span(&run.inputs, ticks).map_err(|e| format!("'--inputs': {e}"))?;
            }
            if run.size.is_some() && run.frame.is_none() {
                return Err("'--size' is a frame's: give --frame FILE.png with it".to_owned());
            }
            Cli::Run(run)
        }
        // A project's own checks: runs of its game and what each must print.
        "check" => {
            let mut check = CheckArgs { which: None, file: None, only: None };
            let mut i = 0;
            while i < rest.len() {
                let w = rest[i].as_str();
                if !w.starts_with("--") {
                    if check.which.replace(w.to_owned()).is_some() {
                        return Err("'specula check' takes one project: its name or its folder (none: the folder you are in)".to_owned());
                    }
                    i += 1;
                    continue;
                }
                let value = match w {
                    "--only" | "--file" => rest.get(i + 1).ok_or_else(|| format!("'{w}' needs a value"))?.clone(),
                    _ => return Err(format!("'{w}' is not a flag of this command (it takes: --only NAME, --file FILE, --settings FOLDER)")),
                };
                match w {
                    "--only" => check.only = Some(value),
                    _ => check.file = Some(PathBuf::from(value)),
                }
                i += 2;
            }
            Cli::Check(check)
        }
        "sdk" => {
            let f = flags(rest, &[])?;
            if f.rest.len() > 1 {
                return Err("'specula sdk' takes at most one word: the project's name or folder (none: the folder you are in)".to_owned());
            }
            Cli::Sdk { which: f.rest.first().cloned() }
        }
        "open" => {
            // Everything after `--` is the engine's (`specula open crypt -- --headless`).
            let split = rest.iter().position(|w| w == "--").unwrap_or(rest.len());
            let engine_args: Vec<String> = rest.get(split + 1..).unwrap_or(&[]).to_vec();
            if let Some(bad) = engine_args.iter().find(|a| matches!(a.as_str(), "--project" | "--settings")) {
                return Err(format!("'{bad}' is set by 'specula open' itself: leave it out"));
            }
            let f = flags(&rest[..split], &["--last", "--front"])?;
            let which = match (f.last, f.rest.len()) {
                (true, 0) => Which::Last,
                (false, 1) => Which::Word(f.rest[0].clone()),
                (false, 0) => return Err("'specula open' needs a project: its name, its folder, or --last (the one last worked on; 'specula projects' lists them)".to_owned()),
                _ => return Err("'specula open' takes one project: a name or a folder, or --last".to_owned()),
            };
            Cli::Open { which, front: f.front, engine_args }
        }
        // `open`, in front, with the project's code run: the operator's word that the modules
        // its game file names are theirs to run (docs/design/MODULES.md).
        "play" => {
            let split = rest.iter().position(|w| w == "--").unwrap_or(rest.len());
            let mut engine_args: Vec<String> = rest.get(split + 1..).unwrap_or(&[]).to_vec();
            if let Some(bad) = engine_args.iter().find(|a| matches!(a.as_str(), "--project" | "--settings")) {
                return Err(format!("'{bad}' is set by 'specula play' itself: leave it out"));
            }
            let f = flags(&rest[..split], &["--last", "--as-is"])?;
            let which = match (f.last, f.rest.len()) {
                (true, 0) => Which::Last,
                (false, 1) => Which::Word(f.rest[0].clone()),
                (false, 0) => return Err("'specula play' needs a project: its name, its folder, or --last (the one last worked on; 'specula projects' lists them)".to_owned()),
                _ => return Err("'specula play' takes one project: a name or a folder, or --last".to_owned()),
            };
            // `--as-is`: what is on disk now, whoever built it last; otherwise only what an owner
            // or a granted lead last built on this machine (crate::approved).
            let flag = if f.as_is { "--run-as-is" } else { "--run" };
            if !engine_args.iter().any(|a| a == "--run" || a == "--run-as-is") {
                engine_args.push(flag.to_owned());
            }
            Cli::Open { which, front: true, engine_args }
        }
        "tools" => {
            let f = flags(rest, &["--port", "--project"])?;
            if f.rest.len() > 1 {
                return Err("'specula tools' takes at most one word: a filter".to_owned());
            }
            Cli::Tools { target: target(&f), filter: f.rest.first().cloned() }
        }
        "call" => {
            let f = flags(rest, &["--port", "--project", "--as", "--role"])?;
            if f.role.is_some() && f.as_name.is_none() {
                return Err("'--role' says the role of a named agent: give its name too (--as NAME --role lead)".to_owned());
            }
            let Some(tool) = f.rest.first() else { return Err("'specula call' needs a tool's name ('specula tools' lists them)".to_owned()) };
            if tool.is_empty() || tool.len() > MAX_TOOL_CHARS || !tool.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return Err(format!("'{tool}' is not a tool's name (A-Z a-z 0-9 _ -, at most {MAX_TOOL_CHARS})"));
            }
            // (`--as` says agent_hello for the command, with `--role` in it: calling agent_hello
            // itself under `--as` dropped both without a word. Review R1 of m5-leverage.)
            if tool == "agent_hello" && f.as_name.is_some() {
                return Err("'--as NAME' says agent_hello for you (with --role ROLE in it): call another tool with it, or call agent_hello without --as and give 'name' and 'role' in its arguments".to_owned());
            }
            let args = match f.rest.len() {
                1 => CallArgs::Given(Json::Obj(Vec::new())),
                2 => match f.rest[1].strip_prefix('@') {
                    // (No JSON begins with '@'. The file is read when the command runs.)
                    Some("") => return Err("'@' needs a file's name: specula call TOOL @args.json".to_owned()),
                    Some(file) => CallArgs::File(PathBuf::from(file)),
                    None => CallArgs::Given(call_args(&f.rest[1])?),
                },
                _ => return Err("'specula call' takes a tool's name and at most one JSON object (quote it for your shell, or put it in a file: @args.json)".to_owned()),
            };
            Cli::Call { target: target(&f), tool: tool.clone(), args, as_name: f.as_name, role: f.role }
        }
        other => return Err(format!("'{other}' is not a command (the commands: {}; flags start the engine: specula --help)", WORDS.join(", "))),
    };
    Ok(Some(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: &str) -> Result<Option<Cli>, String> {
        parse(&line.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
    }

    #[test]
    fn check_takes_a_project_and_which_checks() {
        let p = |line: &str| parse(&line.split_whitespace().map(str::to_owned).collect::<Vec<_>>());
        assert_eq!(p("check"), Ok(Some(Cli::Check(CheckArgs { which: None, file: None, only: None }))));
        assert_eq!(p("check crypt --only won --file other.json"), Ok(Some(Cli::Check(CheckArgs { which: Some("crypt".into()), file: Some(PathBuf::from("other.json")), only: Some("won".into()) }))));
        assert!(p("check a b").unwrap_err().contains("one project"));
        assert!(p("check --ticks 5").unwrap_err().contains("not a flag"));
        assert!(p("check --only").unwrap_err().contains("needs a value"));
    }

    #[test]
    fn run_takes_ticks_a_script_and_a_frame() {
        let Ok(Some(Cli::Run(run))) = p("run crypt --ticks 600 --seed 3 --inputs 0:enter,5:up,90:none --frame out/f.png --size 800x480") else { panic!("not a run") };
        assert_eq!((run.which.as_deref(), run.ticks, run.seed), (Some("crypt"), Some(600), Some(3)));
        assert_eq!(run.inputs, vec![(0, "enter".to_owned()), (5, "up".to_owned()), (90, "none".to_owned())]);
        assert_eq!((run.frame, run.size), (Some(PathBuf::from("out/f.png")), Some((800, 480))));
        // No project: the folder the command is run in. No script: the game from its start.
        let Ok(Some(Cli::Run(bare))) = p("run --ticks 0") else { panic!("not a run") };
        assert_eq!((bare.which, bare.ticks, bare.inputs.len(), bare.frame), (None, Some(0), 0, None));
        let no = |line: &str| p(line).unwrap_err();
        assert!(no("run crypt").contains("--ticks"), "how many ticks is always said");
        // A followed run's ticks are its steps': the script before them may be none, and
        // when there is one it runs to its last input.
        let Ok(Some(Cli::Run(followed))) = p("run crypt --follow") else { panic!("not a run") };
        assert_eq!((followed.follow, followed.ticks), (true, Some(0)));
        let Ok(Some(Cli::Run(followed))) = p("run --follow --inputs 0:enter,4:up crypt") else { panic!("not a run") };
        assert_eq!((followed.follow, followed.ticks, followed.which.as_deref()), (true, Some(4), Some("crypt")));
        assert!(!run.follow);
        assert!(no("run crypt --ticks 5 --size 800x480").contains("--frame"));
        assert!(no("run crypt --ticks 5 --frame f.png --size 10x480").contains("64 to 2048"));
        assert!(no("run crypt other --ticks 5").contains("one project"));
        assert!(no("run crypt --ticks 5 --port 7000").contains("not a flag"));
        assert!(no("run crypt --ticks 2000000").contains("whole number up to"));
        assert!(no("run crypt --ticks 5 --inputs 9:up,3:down").contains("order"));
        assert!(no("run crypt --ticks").contains("needs a value"));
    }

    #[test]
    fn no_arguments_and_flags_are_the_engines_own() {
        assert_eq!(p(""), Ok(None));
        assert_eq!(p("--port 7471 --headless"), Ok(None));
        assert_eq!(p("-h"), Ok(None));
        // A word that is no command is said, with the commands: never run as an engine.
        let e = p("opne crypt").unwrap_err();
        assert!(e.contains("'opne' is not a command") && e.contains("projects, open, play, run, check, new"), "{e}");
    }

    #[test]
    fn the_project_commands_read_their_words() {
        assert_eq!(p("projects"), Ok(Some(Cli::Projects)));
        assert_eq!(p("list"), Ok(Some(Cli::List)));
        assert_eq!(p("help"), Ok(Some(Cli::Help)));
        assert!(p("projects extra").unwrap_err().contains("no words"));
        // An agent's name: 1 to 64 safe characters (mutation testing, 2026-11-14: the bounds
        // could be changed and no test noticed).
        let named = |name: &str| parse(&["call".to_owned(), "world_list".to_owned(), "--as".to_owned(), name.to_owned()]);
        assert!(matches!(named(&"a".repeat(64)), Ok(Some(Cli::Call { as_name: Some(n), .. })) if n.len() == 64));
        assert!(named(&"a".repeat(65)).unwrap_err().contains("'--as' is an agent's name"));
        assert!(named("").unwrap_err().contains("'--as' is an agent's name"));
        assert!(named("a").is_ok() && named("a.b_c-9").is_ok());
        for bad in ["a b", "a/b", "..\\x", "a:b", "ann\u{e9}"] {
            assert!(named(bad).unwrap_err().contains("'--as' is an agent's name"), "{bad}");
        }
        assert_eq!(p("stop"), Ok(Some(Cli::Stop { target: Target::Any })));
        assert_eq!(p("stop --project crypt"), Ok(Some(Cli::Stop { target: Target::Project("crypt".into()) })));
        assert_eq!(p("stop --port 7500"), Ok(Some(Cli::Stop { target: Target::Port(7500) })));
        assert!(p("stop crypt").unwrap_err().contains("no words"));
        assert!(p("list --port 1").unwrap_err().contains("not a flag of this command"));
        assert_eq!(p("new crypt"), Ok(Some(Cli::New { name: "crypt".into(), parent: None, bare: false })));
        assert_eq!(p("new crypt --in D:/games"), Ok(Some(Cli::New { name: "crypt".into(), parent: Some(PathBuf::from("D:/games")), bare: false })));
        assert_eq!(p("new crypt --bare"), Ok(Some(Cli::New { name: "crypt".into(), parent: None, bare: true })));
        // Play is open, in front, with the project's code run: said once, however it is written.
        let run = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(p("play crypt"), Ok(Some(Cli::Open { which: Which::Word("crypt".into()), front: true, engine_args: run(&["--run"]) })));
        assert_eq!(p("play --last -- --size 800x600"), Ok(Some(Cli::Open { which: Which::Last, front: true, engine_args: run(&["--size", "800x600", "--run"]) })));
        assert_eq!(p("play crypt -- --run"), Ok(Some(Cli::Open { which: Which::Word("crypt".into()), front: true, engine_args: run(&["--run"]) })));
        // As it is on disk, whoever built it last, only when said.
        assert_eq!(p("play crypt --as-is"), Ok(Some(Cli::Open { which: Which::Word("crypt".into()), front: true, engine_args: run(&["--run-as-is"]) })));
        assert_eq!(p("play crypt --as-is -- --run-as-is"), Ok(Some(Cli::Open { which: Which::Word("crypt".into()), front: true, engine_args: run(&["--run-as-is"]) })));
        assert!(p("open crypt --as-is").unwrap_err().contains("not a flag of this command"));
        assert!(p("play").unwrap_err().contains("needs a project"));
        assert!(p("play crypt --front").unwrap_err().contains("not a flag of this command"), "play is in front already");
        assert!(p("play crypt -- --project D:/other").unwrap_err().contains("set by 'specula play' itself"));
        assert!(p("open crypt --bare").unwrap_err().contains("not a flag of this command"));
        assert!(p("new").unwrap_err().contains("one word"));
        assert!(p("new a/b").unwrap_err().contains("not a project name"));
        assert!(p("new crypt --in").unwrap_err().contains("needs a value"));
        assert_eq!(p("forget crypt"), Ok(Some(Cli::Forget { which: "crypt".into() })));
        assert_eq!(p("sdk"), Ok(Some(Cli::Sdk { which: None })));
        assert_eq!(p("sdk crypt"), Ok(Some(Cli::Sdk { which: Some("crypt".into()) })));
        assert!(p("sdk a b").unwrap_err().contains("at most one word"));
        assert_eq!(p("open --last"), Ok(Some(Cli::Open { which: Which::Last, front: false, engine_args: vec![] })));
        assert_eq!(p("open crypt --front"), Ok(Some(Cli::Open { which: Which::Word("crypt".into()), front: true, engine_args: vec![] })));
        assert_eq!(
            p("open D:/games/crypt -- --headless --port 7500"),
            Ok(Some(Cli::Open { which: Which::Word("D:/games/crypt".into()), front: false, engine_args: vec!["--headless".into(), "--port".into(), "7500".into()] }))
        );
        assert!(p("open").unwrap_err().contains("needs a project"));
        assert!(p("open a b").unwrap_err().contains("one project"));
        assert!(p("open --last crypt").unwrap_err().contains("one project"));
        assert!(p("open crypt -- --project D:/other").unwrap_err().contains("set by 'specula open' itself"));
        assert!(p("open crypt -- --settings off").unwrap_err().contains("set by 'specula open' itself"));
    }

    #[test]
    fn tools_and_call_say_which_engine_and_carry_one_json_object() {
        assert_eq!(p("tools"), Ok(Some(Cli::Tools { target: Target::Any, filter: None })));
        assert_eq!(p("tools terrain --port 7471"), Ok(Some(Cli::Tools { target: Target::Port(7471), filter: Some("terrain".into()) })));
        assert_eq!(p("tools --project crypt"), Ok(Some(Cli::Tools { target: Target::Project("crypt".into()), filter: None })));
        assert!(p("tools a b").unwrap_err().contains("at most one word"));
        assert!(p("tools --port 0").unwrap_err().contains("1..65535"));
        assert!(p("tools --port 7471 --project crypt").unwrap_err().contains("not both"));
        assert_eq!(p("call world_list"), Ok(Some(Cli::Call { target: Target::Any, tool: "world_list".into(), args: CallArgs::Given(Json::Obj(Vec::new())), as_name: None, role: None })));
        // A named agent says its role (a lead the operator granted takes the role by saying it).
        assert!(matches!(p("call logic_build --as mason --role lead"), Ok(Some(Cli::Call { as_name: Some(n), role: Some(r), .. })) if n == "mason" && r == "lead"));
        assert!(p("call logic_build --role lead").unwrap_err().contains("--as NAME --role lead"));
        assert!(p("call logic_build --as mason --role").unwrap_err().contains("needs a value"));
        assert!(p("call logic_build --as mason --role a.b").unwrap_err().contains("a role's word"));
        assert!(p(&format!("call logic_build --as mason --role {}", "r".repeat(MAX_ROLE_CHARS + 1))).unwrap_err().contains("a role's word"));
        assert!(p("tools --role lead").unwrap_err().contains("not a flag of this command"));
        assert!(p("call agent_hello --as mason --role lead").unwrap_err().contains("says agent_hello for you"));
        assert!(matches!(p("call agent_hello"), Ok(Some(Cli::Call { as_name: None, .. }))));
        // The arguments from a file: named here, read when the command runs.
        assert_eq!(p("call world_spawn @args.json"), Ok(Some(Cli::Call { target: Target::Any, tool: "world_spawn".into(), args: CallArgs::File(PathBuf::from("args.json")), as_name: None, role: None })));
        assert!(p("call world_spawn @").unwrap_err().contains("needs a file's name"));
        assert!(p("call world_spawn @a.json @b.json").unwrap_err().contains("at most one JSON object"));
        assert!(call_args(&format!("{{\"t\": \"{}\"}}", "x".repeat(MAX_ARGS_BYTES))).unwrap_err().contains("at most"));
        assert!(call_args("{not json").unwrap_err().contains("@args.json"));
        let call = |args: &[&str]| parse(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>());
        assert_eq!(
            call(&["call", "world_query", r#"{"limit": 5, "path_prefix": "/a b"}"#, "--as", "ann", "--port", "7500"]),
            Ok(Some(Cli::Call { target: Target::Port(7500), tool: "world_query".into(), args: CallArgs::Given(Json::parse(r#"{"limit": 5, "path_prefix": "/a b"}"#).unwrap()), as_name: Some("ann".into()), role: None }))
        );
        assert!(call(&["call"]).unwrap_err().contains("needs a tool's name"));
        assert!(call(&["call", "world.list"]).unwrap_err().contains("not a tool's name"));
        assert!(call(&["call", "world_list", "{not json"]).unwrap_err().contains("not JSON"));
        assert!(call(&["call", "world_list", "[1]"]).unwrap_err().contains("one JSON object"));
        assert!(call(&["call", "world_list", "{}", "{}"]).unwrap_err().contains("at most one JSON object"));
        assert!(call(&["call", "world_list", "--as", "a b"]).unwrap_err().contains("agent's name"));
        assert!(call(&["call", "world_list", "--last"]).unwrap_err().contains("not a flag of this command"));
    }

    #[test]
    fn a_commands_settings_folder_is_taken_out_of_its_words() {
        let words = |line: &str| line.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(take_settings(&words("projects")), Ok((words("projects"), None)));
        assert_eq!(take_settings(&words("call world_list --settings D:/u --port 7471")), Ok((words("call world_list --port 7471"), Some(PathBuf::from("D:/u")))));
        // After `--` it is the engine's own flag: left where it is (and `open` refuses it there).
        assert_eq!(take_settings(&words("open crypt -- --settings X")), Ok((words("open crypt -- --settings X"), None)));
        assert!(take_settings(&words("projects --settings")).unwrap_err().contains("needs a folder"));
        assert!(take_settings(&words("projects --settings --port")).unwrap_err().contains("needs a folder"));
        assert!(take_settings(&words("projects --settings off")).unwrap_err().contains("nothing to read"));
    }

    #[test]
    fn the_help_names_every_command() {
        for w in WORDS.iter().filter(|w| **w != "help") {
            assert!(HELP.contains(&format!("specula {w}")), "{w}");
        }
    }
}

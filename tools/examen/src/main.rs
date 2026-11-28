//! The repository's own tooling, in Rust (owner, 2026-11-16: "The only programming language used
//! here should be Rust"): the gate, the live checks' runner, and what a change needs verified.
//!
//! ```text
//! cargo run -p specula-examen -- gate [--all]     what a change passes before it is reported
//! cargo run -p specula-examen -- live             every live check, one after the other
//! cargo run -p specula-examen -- changed          which tier this checkout's change is, and why
//! cargo run -p specula-examen -- corpus tables F M   the body's tables, computed from their source
//! ```
//!
//! A tool's answer is its standard output, in ASCII. It never links the engine: a check
//! reaches an engine through the protocol, as an agent does.
#![forbid(unsafe_code)]

mod changed;
mod cli;
mod corpus;
mod gate;
mod kept;
mod leverage;
mod leverage_arms;
#[cfg(test)]
mod leverage_arms_tests;
mod leverage_report;
mod leverage_trace;
#[cfg(test)]
mod leverage_tests;
mod live;
mod mensura;
mod native;
mod passed;
mod pin;
mod run;
mod testlog;

const HELP: &str = "\
examen: the repository's own tooling

  examen gate [--all | --tier docs|tests|engine | --only STEP,STEP] [--base REF]
              [--jobs 8] [--repeat 5] [--threads 16] [--slow-workers-ms 30] [--package specula]
              [--no-gpu] [--out DIR]
      What a change must pass. Sized to what changed since --base (origin/main) unless told
      otherwise; --all runs everything. Steps: docs, tests, repeat, slow, speed, clippy, builds.

  examen live [--out DIR] [--port 7720] [--only NAME,NAME] [--skip NAME,NAME] [--list] [--plan]
              [--no-build]
      Every live check, one after the other, each on its own ports.

  examen changed [--base REF] [--github-output]
      The tier of this checkout's change since --base (origin/main), and why. With
      --github-output, also `tier=NAME` and `code=true|false` appended to the file
      $GITHUB_OUTPUT names (a workflow's job reads them).

  examen kept --built-at FILE [--clear FOLDER] | --mark FILE
      For CI: keeps a build folder between two runs. --mark writes the commit a passing build
      was of; --built-at, before the next build, gives the files that have not changed since
      that commit an old time, so that cargo builds only what changed.

  examen leverage --check [--data FOLDER]
  examen leverage prompt [--data FOLDER] [--job JOB] [--arm ARM] [--out ROOT] [--exe [KEY=]PATH ...]
  examen leverage prepare JOB ARM N --out ROOT [--data FOLDER] [--exe [KEY=]SPECULA_EXE ...]
              [--port P] [--from RUN] [--build NAME]
  examen leverage engine start|stop RUN | call RUN TOOL ['{json}' | @FILE] | tools RUN [FILTER]
  examen leverage run RUN [--judge [--again WHY]] [--note TEXT] | finish RUN
  examen leverage record RUN --tokens N --tool-uses N --duration-s N [--usage TRANSCRIPT]
              [--by-kind I,W,R,O] [--ended done|wall|stopped] [--note TEXT] [--judge]
  examen leverage usage TRANSCRIPT | judge RUN [--verdict FILE [--again WHY]]
  examen leverage report ROOT | export ROOT DEST | where RUN [RUN ...] [--json]
  examen leverage trace RUN [RUN ...] [--judge] [--json]   (every request: what it read, wrote and cost)
      The measured comparison's harness (docs/design/LEVERAGE.md): the same game jobs built
      through the engine and without it, each run in a folder of its own, what it cost kept
      apart from what its judge sees. --data FOLDER (default evals/leverage) names another
      comparison's settings and jobs, its arms among them; a run keeps the folder it came from.

  examen corpus tables FEMALE.csv MALE.csv
      Prints crates/specula-corpus/src/ansur_data.rs: how each of a body's dimensions is spread
      and follows stature and weight, computed from the two public data files of ANSUR II
      (tools/examen/src/corpus.rs says where they are; they are never in this repository).
";

/// `examen changed`: says the tier; for a workflow, writes it where the next job reads it.
fn changed_main(mut args: cli::Args) -> i32 {
    let parsed = (|| -> Result<_, String> {
        let base = args.value("--base")?.unwrap_or_else(|| "origin/main".to_owned());
        let github = args.flag("--github-output");
        args.done()?;
        Ok((base, github))
    })();
    let (base, github) = match parsed {
        Ok(p) => p,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    let change = changed::since(&base, &run::root());
    println!("{}", change.said());
    if github {
        let Some(path) = std::env::var_os("GITHUB_OUTPUT") else {
            println!("--github-output: GITHUB_OUTPUT names no file");
            return 2;
        };
        let lines = format!("tier={}\ncode={}\n", change.tier.name(), change.tier > changed::Tier::Docs);
        let written = std::fs::OpenOptions::new().append(true).create(true).open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, lines.as_bytes()));
        if let Err(e) = written {
            println!("--github-output: {e}");
            return 2;
        }
    }
    0
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let command = if args.is_empty() { String::new() } else { args.remove(0) };
    let args = cli::Args::new(args);
    let code = match command.as_str() {
        "gate" => gate::main(args),
        "live" => live::main(args),
        "changed" => changed_main(args),
        "kept" => kept::main(args),
        "leverage" => leverage::main(args),
        "corpus" => corpus::main(args),
        "" | "help" | "--help" | "-h" => {
            print!("{HELP}");
            0
        }
        other => {
            println!("unknown command {other:?}\n\n{HELP}");
            2
        }
    };
    std::process::exit(code);
}

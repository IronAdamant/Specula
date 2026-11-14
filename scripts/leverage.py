#!/usr/bin/env python3
"""The measured comparison (docs/design/LEVERAGE.md): the same game jobs built through the
engine and without it, by the same model on the same budget, checked against one list by
someone who built neither.

This script is the harness's plain part: it validates the data, makes a run's folder and its
brief, starts and stops the engine arm's engine, logs every call made through it, collects
what a run left, packs what the judge gets, and writes the report. It never starts an agent:
whoever runs the comparison hands each brief to their own agent (any vendor's) and tells this
script what that agent's harness reported (tokens, tool uses, time). Where an agent has a command
line that takes a prompt and runs to its end, `run` does all of that: it starts the agent as
a process of its own in the run's folder (settings.json's `runner`), stops it at the time
limit, and records what it used.

    python scripts/leverage.py --check
    python scripts/leverage.py prepare JOB ARM N --out ROOT [--exe SPECULA_EXE] [--port P] [--from RUN]
    python scripts/leverage.py engine start|stop RUN
    python scripts/leverage.py call RUN TOOL ['{json}' | @FILE]
    python scripts/leverage.py tools RUN [FILTER]
    python scripts/leverage.py run RUN [--judge]
    python scripts/leverage.py finish RUN
    python scripts/leverage.py record RUN --tokens N --tool-uses N --duration-s N [--usage TRANSCRIPT] [--ended done|wall|stopped] [--note TEXT]
    python scripts/leverage.py usage TRANSCRIPT
    python scripts/leverage.py judge RUN [--verdict FILE]
    python scripts/leverage.py report ROOT
    python scripts/leverage.py export ROOT DEST
    python scripts/leverage.py where RUN [RUN ...] [--json]

A run's folder (ROOT/<job>-<arm>-<n>/) holds what the builder and the judge see: BRIEF.md,
work/ (the game), and for the engine arm engine/ (its settings and trace). What a run cost is
kept apart, in ROOT/costs/, so the judge never reads it.

Output is ASCII. Standard library only.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DATA = REPO / "evals" / "leverage"
ARMS = ("engine", "plain")
KINDS = ("behaviour", "checkable")
# What a run's size counts leave out: build output, the engine's own files, the SDK copy.
SKIP_DIRS = {"target", "specula-sdk", "specula-core", ".specula-cache", "captures", "worlds", "saves", "__pycache__", ".git"}
CODE = {".rs", ".py", ".js", ".ts", ".html", ".css", ".json", ".toml", ".lex", ".md", ".txt", ".glsl"}


# A run's tokens by kind: sent and not cached, written to the cache (new in the context),
# read again from the cache (the context so far, re-read at every request), written out.
TOKEN_KINDS = ("input", "cache_write", "cache_read", "output")


# Words a brief may not hold: it is one text for both arms.
BRIEF_NAMES_NO = ("Specula", "engine", "Python", "Rust", "cargo")


class Bad(Exception):
    """What was asked cannot be done; the message says why."""


# ---- data -------------------------------------------------------------------------------

def load_json(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise Bad(f"{path}: {e}") from e


def settings(data=DATA):
    s = load_json(Path(data) / "settings.json")
    problems = check_settings(s)
    if problems:
        raise Bad("settings.json: " + "; ".join(problems))
    return s


def check_settings(s):
    out = []
    if s.get("leverage") != 1:
        out.append("leverage must be 1 (the format's version)")
    if not isinstance(s.get("runs_per_cell"), int) or not 1 <= s["runs_per_cell"] <= 20:
        out.append("runs_per_cell must be a whole number from 1 to 20")
    for k in ("builder_model", "judge_model"):
        if not isinstance(s.get(k), str) or not s[k]:
            out.append(f"{k} must name a model")
    arms = s.get("arms")
    if arms != list(ARMS):
        out.append(f"arms must be {list(ARMS)}: the comparison is between the two")
    jobs = s.get("jobs")
    if not isinstance(jobs, dict) or not jobs:
        out.append("jobs must map a job's id to its limits")
        jobs = {}
    for name, lim in list(jobs.items()) + [("judge", s.get("judge"))]:
        if not isinstance(lim, dict):
            out.append(f"{name}: limits must be an object")
            continue
        for k, least in (("token_cap", 10_000), ("wall_s", 60)):
            if not isinstance(lim.get(k), int) or lim[k] < least:
                out.append(f"{name}.{k} must be a whole number, at least {least:,}")
    ports = s.get("ports")
    if not (isinstance(ports, list) and len(ports) == 2 and all(isinstance(p, int) for p in ports) and 1024 <= ports[0] <= ports[1] <= 65535):
        out.append("ports must be [first, last]")
    w = s.get("token_weights")
    if not isinstance(w, dict) or set(w) != set(TOKEN_KINDS) or not all(isinstance(v, (int, float)) and v >= 0 for v in w.values()):
        out.append(f"token_weights must give a number for each of {list(TOKEN_KINDS)}")
    run = s.get("runner")
    if not isinstance(run, dict) or not isinstance(run.get("command"), list) or not all(isinstance(c, str) for c in run["command"]) or "{prompt}" not in run["command"]:
        out.append("runner.command must be a command line as a list, with {prompt} and {model} where they go")
    elif "{model}" not in run["command"] or len(run["command"]) < 2:
        out.append("runner.command must be a command line as a list, with {prompt} and {model} where they go")
    if not isinstance(s.get("rules"), list) or not all(isinstance(r, str) for r in s.get("rules", [])):
        out.append("rules must be a list of lines (what every builder is told about the machine)")
    else:
        for must in ("Download nothing", "127.0.0.1", "pointer"):
            if not any(must in r for r in s["rules"]):
                out.append(f"rules must tell a builder what a person's machine needs of it (a line with '{must}')")
    return out


def job(name, data=DATA):
    j = load_json(Path(data) / "jobs" / f"{name}.json")
    problems = check_job(j, name)
    if problems:
        raise Bad(f"jobs/{name}.json: " + "; ".join(problems))
    return j


def check_job(j, name):
    out = []
    if j.get("leverage") != 1:
        out.append("leverage must be 1")
    if j.get("id") != name:
        out.append(f"id must be '{name}' (the file's name)")
    if not isinstance(j.get("title"), str) or not j["title"]:
        out.append("title is missing")
    brief = j.get("brief")
    if not isinstance(brief, list) or not brief or not all(isinstance(b, str) for b in brief):
        out.append("brief must be a list of lines")
    acc = j.get("acceptance")
    if not isinstance(acc, list) or not acc:
        out.append("acceptance must be a list")
        acc = []
    seen = set()
    for a in acc:
        if not isinstance(a, dict) or not isinstance(a.get("id"), str) or not isinstance(a.get("text"), str) or a.get("kind") not in KINDS:
            out.append(f"an acceptance item needs id, text and kind ({' or '.join(KINDS)}): {str(a)[:60]}")
            continue
        if a["id"] in seen:
            out.append(f"acceptance id {a['id']} is used twice")
        seen.add(a["id"])
    if "change_of" in j and (not isinstance(j["change_of"], str) or j["change_of"] == name):
        out.append("change_of must name another job")
    # (What crates/specula/tests/leverage_data.rs holds, so that the two never disagree.)
    text = "\n".join(b for b in brief if isinstance(b, str)) if isinstance(brief, list) else ""
    if len(text) < 400:
        out.append("the brief says what the game does (it is under 400 characters)")
    for word in BRIEF_NAMES_NO:
        if word in text:
            out.append(f"the brief is the same for both arms and names no tool ({word})")
    if not ("headless" in text and "frame" in text and ("change_of" in j or ("PNG" in text and "JSON" in text))):
        out.append("the brief asks both arms for the two checks (a headless run printing JSON, a frame as a PNG)")
    if "change_of" in j and "built by someone else" not in text:
        out.append("a change's brief says its builder has no memory of the build")
    good = [a for a in acc if isinstance(a, dict) and isinstance(a.get("id"), str) and isinstance(a.get("text"), str)]
    if len(good) < 8:
        out.append("a list worth judging has at least 8 items")
    for a in good:
        i = a["id"]
        if not (len(i) == 3 and i[0] == name[:1] and i[1:].isdigit()):
            out.append(f"{i}: an item's id is the job's first letter and two digits")
        if len(a["text"]) < 30 or not a["text"].endswith("."):
            out.append(f"{i}: an item is one sentence that is true or false of a run")
    checkable = sum(1 for a in good if a.get("kind") == "checkable")
    if good and not (checkable >= 1 and checkable * 4 <= len(good)):
        out.append("the checks themselves are a small, marked part of the list (at least one item, at most a quarter)")
    return out


def check_all(data=DATA):
    """Every problem in the data files, as lines (none: the data is good)."""
    out = []
    try:
        s = load_json(Path(data) / "settings.json")
    except Bad as e:
        return [str(e)]
    out += [f"settings.json: {p}" for p in check_settings(s)]
    files = sorted((Path(data) / "jobs").glob("*.json"))
    names = [f.stem for f in files]
    for f in files:
        try:
            j = load_json(f)
        except Bad as e:
            out.append(str(e))
            continue
        out += [f"jobs/{f.name}: {p}" for p in check_job(j, f.stem)]
        if isinstance(j.get("change_of"), str) and j["change_of"] not in names:
            out.append(f"jobs/{f.name}: change_of names no job ({j['change_of']})")
    ids = {}
    for f in files:
        try:
            for a in load_json(f).get("acceptance") or []:
                if isinstance(a, dict) and isinstance(a.get("id"), str):
                    ids.setdefault(a["id"], []).append(f.name)
        except Bad:
            pass
    out += [f"{i} names one item across all jobs (it is in {', '.join(where)})" for i, where in sorted(ids.items()) if len(where) > 1]
    if not any("change_of" in (load_json(f) if f.is_file() else {}) for f in files):
        out.append("the change round is one of the jobs")
    for name in (s.get("jobs") or {}):
        if name not in names:
            out.append(f"settings.json: jobs.{name} has no jobs/{name}.json")
    for name in names:
        if name not in (s.get("jobs") or {}):
            out.append(f"jobs/{name}.json has no limits in settings.json")
    return out


# ---- a run's folder ---------------------------------------------------------------------

def run_file(run):
    f = Path(run) / "run.json"
    if not f.is_file():
        raise Bad(f"{run}: not a run's folder (no run.json; 'prepare' makes one)")
    return f


def read_run(run):
    return load_json(run_file(run))


def write_run(run, r):
    (Path(run) / "run.json").write_text(json.dumps(r, indent=1) + "\n", encoding="utf-8")


def costs_dir(run):
    d = Path(run).resolve().parent / "costs"
    d.mkdir(parents=True, exist_ok=True)
    return d


def cost_file(run):
    return costs_dir(run) / (Path(run).resolve().name + ".json")


def calls_file(run):
    return costs_dir(run) / (Path(run).resolve().name + ".calls.jsonl")


def read_cost(run):
    f = cost_file(run)
    return load_json(f) if f.is_file() else {}


def write_cost(run, c):
    cost_file(run).write_text(json.dumps(c, indent=1) + "\n", encoding="utf-8")


def me(run):
    """How a builder's brief names this script and its run."""
    return f'python "{Path(__file__).resolve().as_posix()}"', f'"{Path(run).resolve().as_posix()}"'


def arm_text(arm, run, r):
    py, rd = me(run)
    if arm == "plain":
        return [
            "What you have",
            "- You build it without a game engine, with what is already on this machine and needs no download: the Rust toolchain (the standard library only; `cargo --offline`) and Python 3 (its standard library only). You choose.",
            f"- Do not use or read Specula (the repository at {REPO.as_posix()}), its tools or its documents.",
        ]
    return [
        "What you have",
        "- You build it with the Specula engine. Your work folder is a Specula project, and a headless engine serves it on this machine.",
        f"- Start or stop that engine with `{py} engine start {rd}` and `{py} engine stop {rd}` (start it first; restart it this way if you need to). Never run the engine's executable yourself.",
        f"- Call a tool with `{py} call {rd} TOOL '{{json}}'`; arguments too long or awkward for a command line go in a file: `{py} call {rd} TOOL @args.json`. `{py} tools {rd} [FILTER]` lists tools, and the `tools_catalog` tool describes them.",
        f"- The engine's documents are in {(REPO / 'docs').as_posix()} (design notes for each area in `docs/design/`), with {(REPO / 'README.md').as_posix()}. Read what you need. An example game module is in {(REPO / 'games' / 'ambulo').as_posix()}.",
        "- Game logic can be a Rust module the engine builds and loads (your work folder is a Cargo workspace that already holds the SDK it depends on, in `crates/specula-sdk`: add your module's crate to its members), or rules as data. You choose. A game built this way has none of the engine's interface screens: what it shows, it draws in the world.",
        "- The two checks the brief asks for may be small scripts of yours that call the engine as above and print or write what is asked.",
        "- Whoever checks the game starts from a stopped engine and an empty world: the commands in your README must work from there (starting the engine and loading the game included).",
    ]


def brief_text(j, arm, run, r, s):
    lim = s["jobs"][j["id"]]
    lines = [f"# {j['title']}", ""] + list(j["brief"]) + [""] + arm_text(arm, run, r) + [
        "",
        "Limits and rules",
        f"- Budget: about {lim['token_cap']:,} tokens and {lim['wall_s'] // 60} minutes. Hand in what works when you are done or the budget is nearly spent; a smaller game that works is worth more than a larger one that does not.",
        f"- Your work folder is {(Path(run).resolve() / 'work').as_posix()}. Write nowhere else.",
    ] + [f"- {line}" for line in s["rules"]] + [
        "- When you finish, answer with: what you built, the commands for the two checks, what you checked yourself, and what does not work.",
    ]
    return "\n".join(lines) + "\n"


def sdk_into(work):
    """The engine arm's work folder as a Cargo workspace holding the SDK a module depends on
    (a module's crates must lie under the engine's working directory: docs/design/MODULES.md)."""
    # (In `crates/`: that is where the build service looks for the SDK it trusts without a
    # source check, under a project as under the engine's own repository.)
    sdk = work / "crates"
    for crate in ("specula-core", "specula-sdk"):
        shutil.copytree(REPO / "crates" / crate, sdk / crate, ignore=shutil.ignore_patterns("target", "tests", "benches", "examples"))
    root = (REPO / "Cargo.toml").read_text(encoding="utf-8")

    def section(name):
        lines, on = [], False
        for line in root.splitlines():
            if line.strip().startswith("["):
                on = line.strip() == f"[{name}]"
            if on:
                lines.append(line)
        while lines and not lines[-1].strip():
            lines.pop()
        return "\n".join(lines)

    manifest = "\n\n".join([
        "# This project's Cargo workspace: the Specula SDK (crates/specula-sdk, crates/specula-core), and your game's module crate(s).\n"
        "# Add a module's folder to `members`; it depends on `specula-sdk = { workspace = true }`.\n"
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/specula-core\", \"crates/specula-sdk\"]",
        section("workspace.package"),
        section("workspace.lints.rust"),
        section("workspace.lints.clippy"),
        "[workspace.dependencies]\nspecula-core = { path = \"crates/specula-core\" }\nspecula-sdk = { path = \"crates/specula-sdk\" }",
        "[profile.dev]\ndebug = 0\nstrip = \"debuginfo\"",
    ]) + "\n"
    (work / "Cargo.toml").write_text(manifest, encoding="utf-8")


def engine_build():
    """Which engine a run was made with: the repository's commit, marked when the tree has
    changes the commit does not."""
    try:
        head = subprocess.run(["git", "rev-parse", "--short=10", "HEAD"], capture_output=True, text=True, cwd=str(REPO), timeout=30, encoding="utf-8", errors="replace").stdout.strip()
        dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], capture_output=True, text=True, cwd=str(REPO), timeout=30, encoding="utf-8", errors="replace").stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    return (head or "unknown") + ("+changes" if dirty else "")


def free_port(s, root):
    used = set()
    for f in Path(root).glob("*/run.json"):
        try:
            used.add(json.loads(f.read_text(encoding="utf-8")).get("port"))
        except ValueError:
            pass
    for p in range(s["ports"][0], s["ports"][1] + 1):
        if p not in used:
            return p
    raise Bad("no port left in settings.json's range")


def prepare(a):
    s = settings()
    j = job(a.job)
    if a.arm not in s["arms"]:
        raise Bad(f"arm must be one of {s['arms']}")
    root = Path(a.out).resolve()
    run = root / f"{a.job}-{a.arm}-{a.n}"
    if run.exists():
        raise Bad(f"{run} exists already: a run is prepared once")
    work = run / "work"
    # (Checked before anything is made: a refused prepare leaves no folder that refuses the next.)
    exe = Path(a.exe).resolve() if a.exe else None
    if a.arm == "engine" and (not exe or not exe.is_file()):
        raise Bad("the engine arm needs --exe: the specula executable (a release build)")
    r = {"leverage": 1, "job": a.job, "arm": a.arm, "n": a.n, "phase": "build", "prepared": time.strftime("%Y-%m-%dT%H:%M:%S"),
         "settings": {"runs_per_cell": s["runs_per_cell"], "builder_model": s["builder_model"], "judge_model": s["judge_model"], "limits": s["jobs"][a.job], "judge": s["judge"],
                      "token_weights": s["token_weights"]}}
    if "change_of" in j:
        if not a.from_run:
            raise Bad(f"the {a.job} job changes a finished {j['change_of']} game: --from RUN names it")
        src = read_run(a.from_run)
        if src["job"] != j["change_of"] or src["arm"] != a.arm:
            raise Bad(f"--from must be a {j['change_of']} run of the {a.arm} arm (it is {src['job']} / {src['arm']})")
        run.mkdir(parents=True)
        # The game as it was handed in; never its builder's costs or the judge's verdict.
        shutil.copytree(Path(a.from_run) / "work", work, ignore=shutil.ignore_patterns("target", ".specula-cache"))
        r["from"] = Path(a.from_run).resolve().name
    else:
        work.mkdir(parents=True)
        if a.arm == "engine":
            sdk_into(work)
            (work / "specula.project.json").write_text(json.dumps({"schema": "specula.project.v1", "name": run.name, "assets": "assets"}) + "\n", encoding="utf-8")
            (work / "assets").mkdir()
    if a.arm == "engine":
        r["exe"] = exe.as_posix()
        r["engine_build"] = a.build or engine_build()
        r["port"] = a.port or free_port(s, root)
        (run / "engine").mkdir()
    write_run(run, r)
    (run / "BRIEF.md").write_text(brief_text(j, a.arm, run, r, s), encoding="utf-8")
    print(json.dumps({"run": run.as_posix(), "brief": (run / "BRIEF.md").as_posix(), "port": r.get("port"),
                      "model": s["builder_model"], "token_cap": s["jobs"][a.job]["token_cap"], "wall_s": s["jobs"][a.job]["wall_s"]}))


# ---- the engine arm's engine ------------------------------------------------------------

def specula(r, run, words, timeout=180):
    """One command of the engine's own command line, with this run's settings folder."""
    cmd = [r["exe"]] + words + ["--settings", str(Path(run).resolve() / "engine" / "settings")]
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout, cwd=str(Path(run).resolve()))


def engine_cmd(a):
    r = read_run(a.run)
    if r["arm"] != "engine":
        raise Bad("this run's arm has no engine")
    run = Path(a.run).resolve()
    if a.what == "start":
        flags = ["--headless", "--port", str(r["port"]), "--lead", "builder", "--lead", "judge", "--trace", str(run / "engine" / "trace"), "--captures", str(run / "work" / "captures")]
        cmd = [r["exe"], "open", str(run / "work"), "--settings", str(run / "engine" / "settings"), "--"] + flags
        p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300, cwd=str(run))
    else:
        p = specula(r, run, ["stop", "--port", str(r["port"])])
    sys.stdout.write(p.stdout or p.stderr)
    log_call(a.run, r, f"engine.{a.what}", 0, len(p.stdout or ""), p.returncode, 0.0)
    return p.returncode


def stop_engine(r, run):
    """Stops the run's engine and says whether none is left: stopped now, or none was running
    (`specula stop` refuses both a stop that failed and an engine that is not there with exit
    code 1: its answer tells them apart)."""
    if r["arm"] != "engine":
        return True
    try:
        p = specula(r, run, ["stop", "--port", str(r["port"])])
    except subprocess.TimeoutExpired:
        return False
    try:
        answer = json.loads(p.stdout or "{}")
    except ValueError:
        answer = {}
    return bool(answer.get("stopped")) or answer.get("error") in ("not_listed", "no_engine")


def log_call(run, r, tool, args_bytes, answer_bytes, code, ms):
    row = {"t": round(time.time(), 3), "phase": r.get("phase", "build"), "tool": tool, "args_bytes": args_bytes, "answer_bytes": answer_bytes, "exit": code, "ms": round(ms, 1)}
    with calls_file(run).open("a", encoding="utf-8") as f:
        f.write(json.dumps(row) + "\n")


def call_cmd(a):
    r = read_run(a.run)
    if r["arm"] != "engine":
        raise Bad("this run's arm has no engine")
    words = ["call", a.tool]
    args = ""
    if a.args:
        args = a.args
        if args.startswith("@"):
            f = Path(args[1:])
            if not f.is_absolute():
                f = Path.cwd() / f
            try:
                args = f.read_text(encoding="utf-8")
            except OSError as e:
                raise Bad(f"{f}: {e}") from e
        try:
            # (Sent on one line: what a file holds may be laid out over many.)
            args = json.dumps(json.loads(args), separators=(",", ":"))
        except ValueError as e:
            raise Bad(f"the arguments are not JSON ({e}); in a shell, quoting is easier from a file: TOOL @args.json") from e
        # (Handed over in a file, as `specula call TOOL @FILE` takes them: a command line
        # holds about 32,000 characters, and a recipe or a module's source is more.)
        handed = Path(a.run).resolve() / "engine" / f"args-{os.getpid()}.json"
        handed.write_text(args, encoding="utf-8")
        words.append(f"@{handed}")
    # (The run's builder, and later its judge, are the leads of their own engine: a module
    # is native code, which only a lead the operator named may build and load.)
    words += ["--port", str(r["port"]), "--as", "judge" if r.get("phase") == "judge" else "builder", "--role", "lead"]
    t0 = time.perf_counter()
    try:
        p = specula(r, a.run, words)
    finally:
        if a.args:
            handed.unlink(missing_ok=True)
    out = p.stdout or p.stderr
    log_call(a.run, r, a.tool, len(args.encode("utf-8")), len(out.encode("utf-8")), p.returncode, (time.perf_counter() - t0) * 1000.0)
    sys.stdout.write(out)
    return p.returncode


def tools_cmd(a):
    r = read_run(a.run)
    if r["arm"] != "engine":
        raise Bad("this run's arm has no engine")
    t0 = time.perf_counter()
    p = specula(r, a.run, ["tools"] + ([a.filter] if a.filter else []) + ["--port", str(r["port"])])
    out = p.stdout or p.stderr
    log_call(a.run, r, "(tools)", len((a.filter or "").encode("utf-8")), len(out.encode("utf-8")), p.returncode, (time.perf_counter() - t0) * 1000.0)
    sys.stdout.write(out)
    return p.returncode


# ---- what a run left --------------------------------------------------------------------

def sizes(work):
    files, size, lines, by_ext = 0, 0, 0, {}
    for dirpath, dirs, names in os.walk(work):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for n in names:
            p = Path(dirpath) / n
            try:
                b = p.stat().st_size
            except OSError:
                continue
            files += 1
            size += b
            ext = p.suffix.lower()
            if ext in CODE and b < 4_000_000:
                try:
                    k = sum(1 for _ in p.open("r", encoding="utf-8", errors="replace"))
                except OSError:
                    k = 0
                lines += k
                by_ext[ext] = by_ext.get(ext, 0) + k
    return {"files": files, "bytes": size, "text_lines": lines, "lines_by_kind": dict(sorted(by_ext.items()))}


def call_totals(run, phase):
    rows = []
    f = calls_file(run)
    if f.is_file():
        for line in f.read_text(encoding="utf-8").splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                continue
            if row.get("phase") == phase:
                rows.append(row)
    by_tool = {}
    for row in rows:
        by_tool[row["tool"]] = by_tool.get(row["tool"], 0) + 1
    # (Starting and stopping the engine are the harness's own commands, not tool calls.)
    rows = [r for r in rows if not r["tool"].startswith("engine.")]
    return {"calls": len(rows), "refused": sum(1 for r in rows if r["exit"] != 0), "args_bytes": sum(r["args_bytes"] for r in rows),
            "answer_bytes": sum(r["answer_bytes"] for r in rows), "by_tool": dict(sorted(by_tool.items(), key=lambda kv: (-kv[1], kv[0])))}


def finish(a):
    r = read_run(a.run)
    c = read_cost(a.run)
    if r["arm"] == "engine":
        c["engine_stopped"] = stop_engine(r, a.run)
        c["engine"] = call_totals(a.run, "build")
    c["left"] = sizes(Path(a.run) / "work")
    c["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S")
    write_cost(a.run, c)
    r["phase"] = "judge"
    write_run(a.run, r)
    print(json.dumps({"left": c["left"], "engine": c.get("engine")}))


def transcript_usage(path):
    """A run's tokens by kind, summed over the requests of a transcript that keeps each
    request's usage (Claude Code's: one JSON object a line, `message.usage`, a request's
    lines sharing `message.id`). Nothing of the transcript's text is kept."""
    tot, seen, last = dict.fromkeys(TOKEN_KINDS, 0), set(), 0
    names = {"input": "input_tokens", "cache_write": "cache_creation_input_tokens", "cache_read": "cache_read_input_tokens", "output": "output_tokens"}
    try:
        lines = Path(path).open(encoding="utf-8", errors="replace")
    except OSError as e:
        raise Bad(f"{path}: {e}") from e
    with lines:
        for line in lines:
            try:
                m = json.loads(line).get("message") or {}
            except (ValueError, AttributeError):
                continue
            u = m.get("usage")
            if not isinstance(u, dict) or m.get("id") in seen:
                continue
            seen.add(m.get("id"))
            for kind, name in names.items():
                tot[kind] += u.get(name) or 0
            last = sum(u.get(names[k]) or 0 for k in ("input", "cache_write", "cache_read"))
    if not seen:
        raise Bad(f"{path}: no request with a usage record (this reads Claude Code's transcripts; for another harness give the figures to 'record' yourself)")
    return {**tot, "requests": len(seen), "context_at_end": last}


def weighted(u, weights):
    """One figure for a run's tokens: each kind times its weight (settings.json's
    `token_weights`: relative prices, data like every other size)."""
    return round(sum(u[k] * weights[k] for k in TOKEN_KINDS))


def usage_cmd(a):
    print(json.dumps(transcript_usage(a.transcript)))


BUILDER_PROMPT = (
    "You are a builder in a measured comparison. Your whole task is in the file BRIEF.md in the folder you were started in: "
    "read it and build what it asks, in the work folder it names, within the limits and rules it states. "
    "When you finish, answer with what the brief asks for."
)
JUDGE_PROMPT = (
    "You are the judge of one game in a measured comparison. You built none of it. Your whole task is in the file JUDGE.md in the folder "
    "you were started in: read it and do what it says, for the game in the work folder beside it. Rules of this machine: {rules} "
    "Repair nothing in the game. Write only verdict.json beside JUDGE.md, and scratch files of your own in a new folder judge-scratch beside it. "
    "When you finish, answer in a few lines: how many items passed, which failed and why, and anything that got in your way as a judge."
)


def stream_usage(path):
    """What a run used, from the lines its process printed (Claude Code's `stream-json`: one
    JSON object a line; each request's message carries its usage, and the last line is the
    result with the totals and the price). A run stopped at its time limit has no last line:
    its requests are summed."""
    by, seen, tools, last, result = dict.fromkeys(TOKEN_KINDS, 0), set(), 0, 0, None
    names = {"input": "input_tokens", "cache_write": "cache_creation_input_tokens", "cache_read": "cache_read_input_tokens", "output": "output_tokens"}
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            j = json.loads(line)
        except ValueError:
            continue
        if not isinstance(j, dict):
            continue
        if j.get("type") == "result":
            result = j
            continue
        m = j.get("message") if isinstance(j.get("message"), dict) else {}
        if j.get("type") == "assistant" and isinstance(m.get("content"), list):
            tools += sum(1 for c in m["content"] if isinstance(c, dict) and c.get("type") == "tool_use")
        u = m.get("usage")
        if j.get("type") != "assistant" or not isinstance(u, dict):
            continue
        # (A request's message arrives in parts, each with the usage so far: the last counts.)
        mid = m.get("id")
        if mid in seen:
            for kind, name in names.items():
                by[kind] -= seen_usage[mid].get(name) or 0
        seen.add(mid)
        seen_usage[mid] = u
        for kind, name in names.items():
            by[kind] += u.get(name) or 0
        last = sum(u.get(names[k]) or 0 for k in ("input", "cache_write", "cache_read"))
    # (Without the result line the figures are what the lines so far carried: each request's
    # line holds its usage when it was written, which for the words written out is less than
    # the request came to. Such a run says its tokens are incomplete.)
    out = {"by_kind": {**by, "requests": len(seen), "context_at_end": last}, "tool_uses": tools, "complete": result is not None}
    if result:
        mu = result.get("modelUsage") or {}
        if mu:
            total = {"input": 0, "cache_write": 0, "cache_read": 0, "output": 0}
            for v in mu.values():
                total["input"] += v.get("inputTokens") or 0
                total["cache_write"] += v.get("cacheCreationInputTokens") or 0
                total["cache_read"] += v.get("cacheReadInputTokens") or 0
                total["output"] += v.get("outputTokens") or 0
            out["by_kind"].update(total)
            out["models"] = sorted(mu)
        out["cost_usd"] = result.get("total_cost_usd")
        out["turns"] = result.get("num_turns")
        out["said"] = result.get("result") or ""
        out["error"] = bool(result.get("is_error"))
    return out


seen_usage = {}


def run_cmd(a):
    """Starts the run's builder (or, with --judge, its judge) as a process of its own in the
    run's folder, stops it at its time limit, and records what it used."""
    import shutil as sh
    s = settings()
    r = read_run(a.run)
    run = Path(a.run).resolve()
    who = "judge" if a.judge else "builder"
    if a.judge:
        if r.get("phase") != "judge":
            raise Bad("'finish' the run first: the judge starts from what the builder handed in")
        had = read_cost(a.run)
        judged = bool(had.get("verdict") or had.get("judge"))
        if judged and not a.again:
            raise Bad("this run is judged already ('--judge --again WHY' judges it once more and keeps the first judging)")
        # (The judge's page first: it refuses when the game before a change has no verdict,
        # and nothing of an earlier judging is moved for a judging that will not happen.)
        page = judge_text(run, r)
        if judged:
            keep_judging(a.run, had, a.again)
        (run / "JUDGE.md").write_text(page, encoding="utf-8")
        prompt, model, wall = JUDGE_PROMPT.format(rules=" ".join(s["rules"])), s["judge_model"], s["judge"]["wall_s"]
    else:
        if r.get("phase") != "build" or read_cost(a.run).get("builder"):
            raise Bad("this run's builder has run already: a run is built once")
        prompt, model, wall = BUILDER_PROMPT, s["builder_model"], r["settings"]["limits"]["wall_s"]
    cmd = [c.replace("{prompt}", prompt).replace("{model}", model) for c in s["runner"]["command"]]
    exe = sh.which(cmd[0])
    if not exe:
        raise Bad(f"{cmd[0]}: no such command on this machine (settings.json's runner.command)")
    stream = costs_dir(a.run) / f"{run.name}.{who}.stream.jsonl"
    seen_usage.clear()
    t0 = time.time()
    ended = "done"
    def stop_agent(proc):
        # (The process and what it started, by its process id or its own process group:
        # nothing else on the machine.)
        if os.name == "nt":
            subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
        else:
            import signal
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except OSError:
                proc.kill()
        try:
            proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            pass

    try:
        with stream.open("w", encoding="utf-8") as out:
            # (A session of its own off Windows, so that stopping it stops what it started.)
            proc = subprocess.Popen([exe] + cmd[1:], cwd=str(run), stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT, start_new_session=os.name != "nt")
            try:
                proc.wait(timeout=wall)
            except subprocess.TimeoutExpired:
                ended = "wall"
                stop_agent(proc)
            except BaseException:
                # (Ctrl+C, or anything else that ends this script: the agent does not outlive it.)
                stop_agent(proc)
                raise
    finally:
        # Whatever the agent did or did not do, no engine of this run is left behind: an engine
        # is started by `specula open`, which exits, so it is not among the agent's children.
        engine_gone = stop_engine(r, run)
    took = time.time() - t0
    u = stream_usage(stream)
    c = read_cost(a.run)
    row = {"tokens": u["by_kind"]["context_at_end"], "tool_uses": u["tool_uses"], "duration_s": round(took, 1), "by_kind": u["by_kind"],
           "weighted": weighted(u["by_kind"], s["token_weights"]), "cost_usd": u.get("cost_usd"), "turns": u.get("turns"), "exit": proc.returncode,
           "runner": cmd[0], "model": model, "models": u.get("models"), "tokens_complete": u["complete"], "engine_stopped": engine_gone}
    if not a.judge:
        lim = r["settings"]["limits"]
        row.update({"ended": ended if not u.get("error") else "error", "over_token_cap": row["weighted"] > lim["token_cap"], "over_wall": ended == "wall", "note": a.note or ""})
    else:
        row["ended"] = ended
    c[who] = row
    write_cost(a.run, c)
    (costs_dir(a.run) / f"{run.name}.{who}.said.md").write_text(u.get("said") or "(the process ended without a last answer)\n", encoding="utf-8")
    if a.judge:
        v = run / "verdict.json"
        if v.is_file():
            a.verdict = str(v)
            try:
                judge(a)
            except Bad as e:
                print(json.dumps({"error": "bad_verdict", "message": str(e)}))
        else:
            print(json.dumps({"error": "no_verdict", "message": "the judge left no verdict.json"}))
    else:
        finish(a)
    print(json.dumps({k: row[k] for k in ("ended", "duration_s", "tool_uses", "weighted", "cost_usd", "turns", "exit")}))
    return 0


def record(a):
    r = read_run(a.run)
    c = read_cost(a.run)
    lim = r["settings"]["limits"]
    who = "judge" if a.judge else "builder"
    row = {"tokens": a.tokens, "tool_uses": a.tool_uses, "duration_s": a.duration_s}
    if a.usage:
        row["by_kind"] = transcript_usage(a.usage)
    elif a.by_kind:
        row["by_kind"] = dict(zip(TOKEN_KINDS, a.by_kind))
    if "by_kind" in row:
        row["weighted"] = weighted(row["by_kind"], settings()["token_weights"])
    if not a.judge:
        row.update({"ended": a.ended, "over_token_cap": row.get("weighted", a.tokens) > lim["token_cap"], "over_wall": a.duration_s > lim["wall_s"], "note": a.note or ""})
    c[who] = {**c.get(who, {}), **row}
    write_cost(a.run, c)
    print(json.dumps(c[who]))


# ---- judging ----------------------------------------------------------------------------

def acceptance_of(r):
    j = job(r["job"])
    items = list(j["acceptance"])
    return j, items


def judge_text(run, r):
    j, items = acceptance_of(r)
    py, rd = me(run)
    lines = [
        f"# Judging: {j['title']}", "",
        "You built none of this. Decide, item by item, whether the game in `work/` does what its brief asked. The brief the builder had is `BRIEF.md`, beside this file.", "",
        "How",
        "- Read the game's README.md (CHANGES.md for a change) for the commands of its headless run and its frame. Run them yourself, with inputs you choose: an item passes on what you saw the game do, never on what its builder wrote.",
        "- Open the frames (PNG) and look at them.",
        "- A check that cannot be run as the game's own documents say fails, and so does every item that needed it; say so once and go on.",
        "- You may read the game's files to choose inputs and to explain a failure. Do not repair anything.",
    ]
    if r["arm"] == "engine":
        lines += [f"- This game runs on the Specula engine. Start its engine with `{py} engine start {rd}` and stop it with `{py} engine stop {rd}` when you are done; calls go through `{py} call {rd} TOOL '{{json}}'` as its documents say. Never run the engine's executable yourself."]
    lines += ["", "The list"] + [f"- **{a['id']}** ({a['kind']}): {a['text']}" for a in items]
    if "change_of" in j:
        base = job(j["change_of"])
        before = read_cost(Path(run).resolve().parent / r["from"]).get("verdict")
        if not before:
            raise Bad(f"judge {r['from']} first: the change's judge is told what the game passed before the change")
        was = {i["id"]: i for i in before["items"]}
        lines += ["", "The list of the game before the change, as its own judge found it (read each with the change in mind). "
                  "The item that says nothing may be broken holds the changed game to the ones that PASSED then, and only to those:"]
        for a in base["acceptance"]:
            i = was.get(a["id"])
            then = "passed before the change" if i and i["pass"] else f"FAILED before the change (not held against it): {i['evidence'] if i else 'not judged'}"
            lines.append(f"- **{a['id']}**: {a['text']} [{then}]")
    lines += [
        "", "Your answer",
        "Write `verdict.json` beside this file, and nothing else outside `work/`:",
        "```json",
        '{"items": [{"id": "s01", "pass": true, "evidence": "what you ran and what it printed, in a line or two"}],',
        ' "defects": ["anything wrong that the list does not name, one line each"],',
        ' "could_not_check": ["an item\'s id and why, if you could not decide it"]}',
        "```",
        "Every item of the list is in `items` once. An item you could not decide is `\"pass\": false` and named in `could_not_check`.",
    ]
    return "\n".join(lines) + "\n"


def keep_judging(run, had, why):
    """An earlier judging goes into the record (`judged_before`) with why there is another,
    and its verdict file is kept under a number: a verdict is never written over."""
    run = Path(run).resolve()
    had.setdefault("judged_before", []).append({"why": why, "verdict": had.pop("verdict", None), "judge": had.pop("judge", None)})
    write_cost(run, had)
    old = run / "verdict.json"
    if old.is_file():
        old.replace(run / f"verdict.{len(had['judged_before'])}.json")


def judge(a):
    r = read_run(a.run)
    run = Path(a.run).resolve()
    if not a.verdict:
        if r.get("phase") != "judge":
            raise Bad("'finish' the run first: the judge starts from what the builder handed in")
        (run / "JUDGE.md").write_text(judge_text(run, r), encoding="utf-8")
        s = settings()
        print(json.dumps({"judge": (run / "JUDGE.md").as_posix(), "model": s["judge_model"], "token_cap": s["judge"]["token_cap"], "wall_s": s["judge"]["wall_s"]}))
        return 0
    v = load_json(a.verdict)
    _, items = acceptance_of(r)
    problems = check_verdict(v, items)
    if problems:
        raise Bad("verdict: " + "; ".join(problems))
    c = read_cost(a.run)
    if c.get("verdict"):
        # (By hand, on a run that has a verdict: kept, with why, or refused.)
        if not getattr(a, "again", None):
            raise Bad("this run has a verdict already ('judge RUN --verdict FILE --again WHY' records another and keeps the first)")
        kept = Path(a.verdict).resolve().read_text(encoding="utf-8")
        keep_judging(a.run, c, a.again)
        (run / "verdict.json").write_text(kept, encoding="utf-8")
        c = read_cost(a.run)
    c["verdict"] = score(v, items)
    c["judge"] = c.get("judge", {})
    if r["arm"] == "engine":
        c["judge"]["engine_stopped"] = stop_engine(r, a.run)
        c["judge"]["engine"] = call_totals(a.run, "judge")
    write_cost(a.run, c)
    print(json.dumps(c["verdict"]["totals"]))
    return 0


def check_verdict(v, items):
    out = []
    got = v.get("items")
    if not isinstance(got, list):
        return ["items must be a list"]
    ids = [i.get("id") for i in got if isinstance(i, dict)]
    for a in items:
        if ids.count(a["id"]) != 1:
            out.append(f"{a['id']} must be answered once (it is {ids.count(a['id'])} times)")
    for i in got:
        if not isinstance(i, dict) or not isinstance(i.get("pass"), bool) or not isinstance(i.get("evidence"), str) or not i["evidence"].strip():
            out.append(f"an item needs id, pass (true or false) and evidence: {str(i)[:60]}")
    for k in ("defects", "could_not_check"):
        if not isinstance(v.get(k, []), list):
            out.append(f"{k} must be a list")
    return out


def score(v, items):
    by = {i["id"]: i for i in v["items"]}
    rows = [{"id": a["id"], "kind": a["kind"], "pass": by[a["id"]]["pass"], "evidence": by[a["id"]]["evidence"]} for a in items]
    tot = {}
    for kind in KINDS:
        of = [r for r in rows if r["kind"] == kind]
        tot[kind] = {"passed": sum(1 for r in of if r["pass"]), "of": len(of)}
    tot["all"] = {"passed": sum(1 for r in rows if r["pass"]), "of": len(rows)}
    return {"items": rows, "totals": tot, "defects": list(v.get("defects", [])), "could_not_check": list(v.get("could_not_check", []))}


# ---- the report -------------------------------------------------------------------------

def fmt(n):
    return "-" if n is None else f"{n:,}"


def report(a):
    root = Path(a.root).resolve()
    runs = []
    for f in sorted(root.glob("*/run.json")):
        r = load_json(f)
        c = read_cost(f.parent)
        runs.append((f.parent.name, r, c))
    if not runs:
        raise Bad(f"{root}: no runs")
    out = ["# Measured leverage: the runs", "", f"Folder: `{root.name}`. Every run is here, failed ones included.", ""]
    seen = []
    for _, r, _ in runs:
        s = r["settings"]
        if r.get("engine_build"):
            built = f"- engine build of the {r['job']} runs: {r['engine_build']}"
            if built not in seen:
                seen.append(built)
        line = f"- {r['job']}: builder {s['builder_model']}, cap {s['limits']['token_cap']:,} tokens and {s['limits']['wall_s'] // 60} min; judge {s['judge_model']}; {s['runs_per_cell']} run(s) a cell"
        if line not in seen:
            seen.append(line)
    weights = {json.dumps(r["settings"].get("token_weights") or settings()["token_weights"], sort_keys=True) for _, r, _ in runs}
    w = json.loads(sorted(weights)[0])
    if len(weights) > 1:
        seen.append("- these runs were weighted with more than one set of weights (each run's file has its own); the first is described below")
    out += ["## Settings these runs used", ""] + seen + [
        "",
        "Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, "
        "re-read from the cache at every request), written out. \"Weighted\" is one figure for them: "
        + ", ".join(f"{k} x {w[k]:g}" for k in TOKEN_KINDS) + " (settings.json's `token_weights`: relative prices, change them for yours).", ""]
    out += ["## Runs", "",
            "| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |",
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    table = []
    for name, r, c in runs:
        b, v, e, left = c.get("builder", {}), c.get("verdict"), c.get("engine"), c.get("left", {})
        cap = r["settings"]["limits"]["token_cap"]
        over = b["weighted"] > cap if "weighted" in b else b.get("over_token_cap")
        ended = b.get("ended", "not recorded") + (" (past the token cap)" if over else "") + (" (past the time limit)" if b.get("over_wall") else "")
        checks = f"{v['totals']['all']['passed']} / {v['totals']['all']['of']}" if v else "not judged"
        checkable = f"{v['totals']['checkable']['passed']} / {v['totals']['checkable']['of']}" if v else "-"
        k = b.get("by_kind") or {}
        usd = f"{b['cost_usd']:.2f}" if isinstance(b.get("cost_usd"), (int, float)) else "-"
        partial = "" if b.get("tokens_complete", True) else " (incomplete: stopped before its totals)"
        out.append(f"| {name} | {fmt(b.get('weighted'))}{partial} | {usd} | {fmt(k['input'] + k['cache_write']) if k else '-'} | {fmt(k.get('cache_read'))} | {fmt(k.get('output'))} | {fmt(k.get('requests'))} | {fmt(b.get('tool_uses'))} | {('%.0f' % (b['duration_s'] / 60)) if 'duration_s' in b else '-'} | {ended} | "
                   f"{fmt(e['calls']) if e else '-'} | {(fmt(e['args_bytes']) + ' / ' + fmt(e['answer_bytes'])) if e else '-'} | {checks} | {checkable} | {len(v['defects']) if v else '-'} | {fmt(left.get('text_lines'))} |")
        table.append({"run": name, "job": r["job"], "arm": r["arm"], "n": r["n"], "from": r.get("from"), "engine_build": r.get("engine_build"), "builder": b, "engine": e, "left": left,
                      "verdict": v["totals"] if v else None, "defects": v["defects"] if v else None, "judge": c.get("judge"),
                      "failed": [{"id": i["id"], "evidence": i["evidence"]} for i in v["items"] if not i["pass"]] if v else None})
    out += ["", "## By job and arm (the mean of each cell's runs that were judged)", "",
            "| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |", "|---|---|---|---|---|---|---|---|"]
    cells = {}
    for t in table:
        cells.setdefault((t["job"], t["arm"], t["engine_build"] or ""), []).append(t)
    builds = {}
    for (jname, arm, build) in cells:
        builds.setdefault((jname, arm), set()).add(build)
    for (jname, arm, build), ts in sorted(cells.items()):
        # (An arm run on two engine builds is two cells: the build is part of what was run.)
        if len(builds[(jname, arm)]) > 1:
            arm = f"{arm}, build {build}"
        done = [t for t in ts if t["verdict"] and "weighted" in t["builder"] and t["builder"].get("tokens_complete", True)]
        if not done:
            out.append(f"| {jname} | {arm} | {len(ts)} | - | - | - | not judged | - |")
            continue
        tokens = sum(t["builder"]["weighted"] for t in done) / len(done)
        minutes = sum(t["builder"]["duration_s"] for t in done) / len(done) / 60
        passed = sum(t["verdict"]["all"]["passed"] for t in done) / len(done)
        of = done[0]["verdict"]["all"]["of"]
        per = f"{tokens / passed:,.0f}" if passed else "none passed"
        spread = f"{min(t['builder']['weighted'] for t in done):,} to {max(t['builder']['weighted'] for t in done):,}"
        out.append(f"| {jname} | {arm} | {len(done)} of {len(ts)} | {tokens:,.0f} | {spread} | {minutes:.0f} | {passed:.1f} / {of} | {per} |")
    def number(x):
        return x if isinstance(x, (int, float)) else 0

    last = [c.get("judge") or {} for _, _, c in runs]
    earlier = [b.get("judge") or {} for _, _, c in runs for b in c.get("judged_before") or []]
    judgings = [j for j in last + earlier if "weighted" in j]
    if judgings:
        out += ["", f"Judging: {len(judgings)} judging(s) of {sum(1 for j in last if 'weighted' in j)} run(s), {sum(j['weighted'] for j in judgings):,} weighted tokens in all"
                + (f" ({len([j for j in earlier if 'weighted' in j])} of them earlier judgings of runs judged again)." if earlier else ".")]
    built = sum(number(t["builder"].get("cost_usd")) for t in table)
    judged_usd, earlier_usd = sum(number(j.get("cost_usd")) for j in last), sum(number(j.get("cost_usd")) for j in earlier)
    if built or judged_usd or earlier_usd:
        out += ["", f"At the runner's list prices: {built:.2f} USD for the builds, {judged_usd:.2f} USD for the judging shown above"
                + (f", and {earlier_usd:.2f} USD for the earlier judgings of the runs judged again ({built + judged_usd + earlier_usd:.2f} USD in all)." if earlier else f" ({built + judged_usd:.2f} USD in all).")]
    again = [(name, c["judged_before"]) for name, _, c in runs if c.get("judged_before")]
    if again:
        out += ["", "## Judged more than once", "", "The figures above are the last judging. Before it:", ""]
        for name, before in again:
            for b in before:
                v0 = b.get("verdict")
                was = f"{v0['totals']['all']['passed']} / {v0['totals']['all']['of']}" if v0 else "no verdict"
                failed0 = ", ".join(i["id"] for i in v0["items"] if not i["pass"]) if v0 else ""
                out.append(f"- {name}: {was}" + (f" (failed: {failed0})" if failed0 else "") + f". Judged again because: {b.get('why')}")
    failed = [(t["run"], i) for t in table for i in (t["failed"] or [])]
    if failed:
        out += ["", "## Items that failed", ""] + [f"- {run} **{i['id']}**: {i['evidence']}" for run, i in failed]
    out += ["", "## What each judge found beyond the list", ""]
    for t in table:
        for d in t["defects"] or []:
            out.append(f"- {t['run']}: {d}")
    text = "\n".join(out) + "\n"
    (root / "report.md").write_text(text, encoding="utf-8")
    (root / "report.json").write_text(json.dumps({"leverage": 1, "runs": table}, indent=1) + "\n", encoding="utf-8")
    sys.stdout.write(text.encode("ascii", "replace").decode("ascii"))


def export(a):
    """What a repository keeps of a set of runs: the report, and one file a run with its
    settings, what it cost and its verdict. Never the games, the transcripts or a path of the
    machine they ran on (each is replaced by a name)."""
    root, dest = Path(a.root).resolve(), Path(a.dest).resolve()
    report(a)
    dest.mkdir(parents=True, exist_ok=True)
    import re
    places = [(root, "<runs>"), (REPO, "<repo>"), (Path.home(), "<home>")]
    try:
        # (A worktree's main checkout is a path of the machine too.)
        common = subprocess.run(["git", "rev-parse", "--git-common-dir"], capture_output=True, text=True, cwd=str(REPO), timeout=30, encoding="utf-8", errors="replace").stdout.strip()
        if common:
            main = (REPO / common).resolve().parent
            if main != REPO:
                places.append((main, "<repo>"))
    except (OSError, subprocess.TimeoutExpired):
        pass
    names = sorted(places, key=lambda kv: -len(kv[0].as_posix()))
    # The name of whoever ran it, wherever it stands (a path form not thought of here, a
    # URL, a prompt): never in what a repository keeps.
    # (Not inside a longer word; a percent-encoded separator before it, %5C or %2F, ends in a
    # letter and is not one.)
    user = re.compile(r"(?:(?<![A-Za-z0-9])|(?<=%5C)|(?<=%2F))" + re.escape(Path.home().name) + r"(?![A-Za-z0-9])", re.IGNORECASE) if len(Path.home().name) >= 3 else None

    def pattern(path):
        # A path as any tool writes it: C:/a/b, C:\a\b, C:\\a\\b (inside JSON), /c/a/b, in any case.
        posix = path.as_posix()
        drive, rest = (posix[0], posix[2:]) if len(posix) > 1 and posix[1] == ":" else ("", posix)
        parts = [re.escape(x) for x in rest.split("/") if x]
        # (Any run of separators: a path quoted from JSON inside JSON has four backslashes. The
        # last part is taken to its end, so that a longer name beginning with it leaks no tail.)
        sep = r"[/\\]+"
        head = f"(?:{re.escape(drive)}:|/{re.escape(drive)})" if drive else ""
        return re.compile(head + sep + sep.join(parts) + r"[A-Za-z0-9_.-]*", re.IGNORECASE)

    patterns = [(pattern(path), name) for path, name in names]

    def scrub(text):
        for pat, name in patterns:
            text = pat.sub(lambda _m, name=name: name, text)
        if user:
            text = user.sub("<user>", text)
        return text

    for name in ("report.md", "report.json"):
        (dest / name).write_text(scrub((root / name).read_text(encoding="utf-8")), encoding="utf-8", newline="\n")
    kept = 0
    for f in sorted(root.glob("*/run.json")):
        r = load_json(f)
        r.pop("exe", None)
        c = read_cost(f.parent)
        for who in ("builder", "judge"):
            (c.get(who) or {}).pop("runner", None)
        b = c.get("builder") or {}
        if "engine_stopped" in c and "engine_stopped" not in b:
            # (Recorded from `specula stop`'s exit code, which is 1 for an engine that had
            # stopped already as for one that would not: not known, so not said.)
            del c["engine_stopped"]
        if "weighted" in b:
            # (Held against the weighted figure, as the report holds it.)
            b["over_token_cap"] = b["weighted"] > r["settings"]["limits"]["token_cap"]
        try:
            c["where"] = where_of(f.parent)
        except Bad:
            pass
        text = json.dumps({"run": r, "costs": c}, indent=1) + "\n"
        (dest / f"{f.parent.name}.json").write_text(scrub(text), encoding="utf-8", newline="\n")
        kept += 1
    print(json.dumps({"kept": kept, "in": dest.as_posix()}))


# ---- where a builder's tokens went ------------------------------------------------------

def step_kind(name, inp):
    """What one step of a builder was, from the tool it used and on what (Claude Code's tools:
    Bash, Read, Write, Edit, Glob, Grep). A reading of a command's text: good enough to say
    where a run's steps went, not an exact account."""
    import re
    repo = REPO.as_posix().lower()

    def inside(text):
        return repo in str(text).replace("\\", "/").lower()

    if name in ("Write", "Edit"):
        return "write code"
    if name == "Read":
        target = str(inp.get("file_path", ""))
        return "look at a frame" if target.lower().endswith(".png") else ("read the engine's documents and source" if inside(target) else "read its own files")
    if name in ("Glob", "Grep"):
        return "read the engine's documents and source" if inside(str(inp.get("path", "")) + str(inp.get("pattern", ""))) else "read its own files"
    if name != "Bash":
        return "other"
    low = str(inp.get("command", "")).replace("\\", "/").lower()
    if "leverage.py" in low:
        if re.search(r"\bcall\b.*logic_build", low):
            return "build the module"
        if re.search(r"\btools\b|tools_catalog|tools_activate", low):
            return "find tools and their arguments"
        if re.search(r"\bengine (start|stop)\b", low) and not re.search(r"\bcall\b", low):
            return "start or stop the engine"
        return "call the engine"
    if re.search(r"\bcargo\b", low):
        return "build the module"
    if inside(low) and re.search(r"\b(cat|sed|head|tail|grep|rg|ls|find|wc|awk)\b", low):
        return "read the engine's documents and source"
    if re.search(r"<<\s*['\"]?\w+|\bcat\s*>|\btee\b", low):
        return "write code"
    if re.search(r"python[0-9.]*\s+[\w./\"-]*\.py|\.py\b.*(run|frame|--headless|--ticks)", low):
        return "run its own checks"
    if re.search(r"\b(cat|sed|head|tail|grep|ls|find|wc)\b", low):
        return "read its own files"
    return "other commands"


def where_of(run):
    """A builder's steps by kind, what each kind's answers put into the context and what the
    builder wrote for it (characters / 4, as tokens: a measure of share, beside the run's own
    exact counts by kind), and how many steps came before the first line of code."""
    run = Path(run).resolve()
    stream = costs_dir(run) / f"{run.name}.builder.stream.jsonl"
    if not stream.is_file():
        raise Bad(f"{run.name}: no builder's stream (only runs started by 'run' keep one)")
    import re
    kinds, pending, order, builds = {}, {}, [], {"steps": 0, "failed": 0}

    def row(kind):
        return kinds.setdefault(kind, {"steps": 0, "answered_tokens": 0, "written_tokens": 0})

    for line in stream.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            j = json.loads(line)
        except ValueError:
            continue
        m = j.get("message") if isinstance(j, dict) and isinstance(j.get("message"), dict) else {}
        for c in m.get("content") if isinstance(m.get("content"), list) else []:
            if not isinstance(c, dict):
                continue
            if j.get("type") == "assistant" and c.get("type") == "tool_use":
                kind = step_kind(c.get("name"), c.get("input") or {})
                pending[c.get("id")] = kind
                order.append(kind)
                row(kind)["steps"] += 1
                row(kind)["written_tokens"] += len(json.dumps(c.get("input") or {})) // 4
            elif j.get("type") == "assistant" and c.get("type") in ("text", "thinking"):
                row("words between steps")["written_tokens"] += len(c.get("text") or c.get("thinking") or "") // 4
            elif j.get("type") == "user" and c.get("type") == "tool_result":
                out = c.get("content")
                text = out if isinstance(out, str) else " ".join(x.get("text", "") for x in out or [] if isinstance(x, dict))
                kind = pending.get(c.get("tool_use_id"), "other")
                row(kind)["answered_tokens"] += len(text) // 4
                if kind == "build the module":
                    # (A build the builder asked for itself, and whether its answer says it failed.)
                    builds["steps"] += 1
                    failed = '"ok": false' in text or any(int(n) for n in re.findall(r'"errors": (\d+)', text)) or re.search(r"^error(\[E\d+\])?:", text, re.M)
                    builds["failed"] += 1 if failed else 0
    b = read_cost(run).get("builder") or {}
    k, w = b.get("by_kind") or {}, read_run(run)["settings"].get("token_weights") or settings()["token_weights"]
    split = {"read_again": round(k.get("cache_read", 0) * w["cache_read"]), "written_out": round(k.get("output", 0) * w["output"]),
             "new_in_context": round(k.get("cache_write", 0) * w["cache_write"] + k.get("input", 0) * w["input"])} if k else {}
    before = next((i for i, kind in enumerate(order) if kind == "write code"), len(order))
    return {"run": run.name, "requests": k.get("requests"), "steps": len(order), "steps_before_the_first_code": before, "weighted": b.get("weighted"), "weighted_by_kind": split,
            "builds": builds, "how": "weighted_by_kind is exact (the run's own token counts times the weights); a step's kind is a reading of its command's text, and answered_tokens / written_tokens are characters / 4",
            "by_step": dict(sorted(kinds.items(), key=lambda kv: -(kv[1]["answered_tokens"] + kv[1]["written_tokens"])))}


def where_cmd(a):
    out = [where_of(run) for run in a.runs]
    if a.json:
        print(json.dumps(out, indent=1))
        return 0
    for w in out:
        s3 = w["weighted_by_kind"]
        print(f"\n{w['run']}: {w['requests']} requests, {w['steps']} steps ({w['steps_before_the_first_code']} before the first code); weighted {fmt(w['weighted'])}"
              + (f" = read again {s3['read_again']:,} + written out {s3['written_out']:,} + new in the context {s3['new_in_context']:,}" if s3 else ""))
        print(f"  {'step':40} {'steps':>5} {'answers (~tokens)':>18} {'written (~tokens)':>18}")
        for kind, r in w["by_step"].items():
            print(f"  {kind:40} {r['steps']:5} {r['answered_tokens']:18,} {r['written_tokens']:18,}")
    return 0


# ---- the command line -------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser(description="The measured comparison's harness (docs/design/LEVERAGE.md).")
    ap.add_argument("--check", action="store_true", help="validate evals/leverage and exit")
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("prepare")
    p.add_argument("job")
    p.add_argument("arm")
    p.add_argument("n", type=int)
    p.add_argument("--out", required=True)
    p.add_argument("--exe")
    p.add_argument("--port", type=int)
    p.add_argument("--from", dest="from_run")
    p.add_argument("--build", help="a name for the engine build these runs use (default: the repository's commit)")
    p = sub.add_parser("engine")
    p.add_argument("what", choices=("start", "stop"))
    p.add_argument("run")
    p = sub.add_parser("call")
    p.add_argument("run")
    p.add_argument("tool")
    p.add_argument("args", nargs="?")
    p = sub.add_parser("tools")
    p.add_argument("run")
    p.add_argument("filter", nargs="?")
    p = sub.add_parser("finish")
    p.add_argument("run")
    p = sub.add_parser("record")
    p.add_argument("run")
    p.add_argument("--tokens", type=int, required=True)
    p.add_argument("--tool-uses", type=int, required=True)
    p.add_argument("--duration-s", type=float, required=True)
    p.add_argument("--ended", choices=("done", "wall", "stopped"), default="done")
    p.add_argument("--note")
    p.add_argument("--usage", help="the run's transcript, where the harness keeps one: its tokens are summed by kind")
    p.add_argument("--by-kind", type=int, nargs=4, metavar=("INPUT", "CACHE_WRITE", "CACHE_READ", "OUTPUT"), help="the run's tokens by kind, from a harness with no transcript")
    p.add_argument("--judge", action="store_true", help="these are the judge's figures, not the builder's")
    p = sub.add_parser("run")
    p.add_argument("run")
    p.add_argument("--judge", action="store_true", help="start the run's judge (after its builder has finished)")
    p.add_argument("--again", metavar="WHY", help="with --judge: judge a run that was judged, keeping the first judging and why")
    p.add_argument("--note")
    p.set_defaults(verdict=None)
    p = sub.add_parser("usage")
    p.add_argument("transcript")
    p = sub.add_parser("judge")
    p.add_argument("run")
    p.add_argument("--verdict")
    p.add_argument("--again", metavar="WHY", help="with --verdict: record another verdict for a judged run, keeping the first and why")
    p = sub.add_parser("report")
    p.add_argument("root")
    p = sub.add_parser("export")
    p.add_argument("root")
    p.add_argument("dest")
    p = sub.add_parser("where")
    p.add_argument("runs", nargs="+")
    p.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)
    try:
        if a.check:
            problems = check_all()
            for line in problems:
                print(line)
            print("leverage data: " + ("ok" if not problems else f"{len(problems)} problem(s)"))
            return 1 if problems else 0
        if not a.cmd:
            ap.print_help()
            return 1
        return {"prepare": prepare, "engine": engine_cmd, "call": call_cmd, "tools": tools_cmd, "finish": finish, "record": record, "run": run_cmd, "usage": usage_cmd, "judge": judge, "report": report, "export": export, "where": where_cmd}[a.cmd](a) or 0
    except Bad as e:
        print(json.dumps({"error": "leverage", "message": str(e)}))
        return 1
    except subprocess.TimeoutExpired as e:
        print(json.dumps({"error": "timeout", "message": f"the engine's command did not answer in {e.timeout:.0f} s"}))
        return 2


if __name__ == "__main__":
    sys.exit(main())

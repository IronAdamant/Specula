#!/usr/bin/env python3
"""Specula agent evals: realistic tasks with outcomes the engine verifies (docs/design/EVALS.md).

A task (evals/<set>/<id>.json) is data: the engine and game it needs, setup calls, the brief an
agent receives, the tools it may use, verifiers (tool calls whose answers must match, playtests
that must pass, files the engine wrote), a reference solution (scripted tool calls that pass) and a
broken one (that must fail). Each task runs in a **fresh** headless engine (`--headless --paused`).

Modes:

    python scripts/evals.py --check                     # validate every task file (no engine)
    python scripts/evals.py --list
    python scripts/evals.py --reference [--set public|heldout|all] [--task a,b] [--workers 3]
    python scripts/evals.py --broken                    # every task's broken solution must FAIL
    python scripts/evals.py --agent --task room_build --port 7493 --engine-port 7492
    python scripts/evals.py call --port 7493 --as builder world_query '{"path_prefix": "/room"}'
    python scripts/evals.py tools --port 7493 [--name world_query]

`--reference` is the tool-regression suite: run it after every tool change. Parallel runs reuse the
playtest swarm (scripts/playtest_swarm.py): a lead engine on --lead-port posts one task per eval on
its board, and each worker leases a port from the lead's pool (--pool, default 7493-7495) and a
scratch folder. Without Vulkan the engines can't start and every task is reported `skipped`, so CI
without a GPU stays green (exit 0).

`--agent` starts the task's engine on --engine-port and a logging MCP proxy on --port, prints the
brief, and waits until the agent calls the proxy's `eval_done` tool (or --timeout). The proxy
shows only the task's tools (others are refused with `tool_not_allowed`) and logs every call.
Agents without an MCP client can use `evals.py tools` (what the port offers) and `evals.py call`
(one call per run; the session is kept in a file per --as name, so claims and identity last between
calls).

Results: --out (default perf/evals/<stamp>-<mode>/): report.json (scores per task) and per task
transcript.jsonl (every MCP call: tool, arguments, bytes, ms, error code), journal.json (the engine's
world_history at the end) and copies of captures. Standard library only; output ASCII.
"""
import argparse
import contextlib
import fnmatch
import http.server
import json
import math
import os
import re
import shutil
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, server_up, url_for  # noqa: E402

EVAL_FORMAT = 1
EVALS_DIR = ROOT / "evals"
SETS = ("public", "heldout")
TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
EXE_SUFFIX = ".exe" if sys.platform == "win32" else ""
# The binary per hosted game (null: the bare engine).
GAME_EXES = {None: "specula", "sepulcra": "sepulcra"}

TASK_KEYS = {"eval", "id", "title", "set", "area", "summary", "game", "engine_args", "engine_cwd", "needs", "files",
             "setup", "actors", "brief", "tools", "limits", "verify", "reference", "broken", "cheats", "notes"}
STEP_KEYS = {"call", "args", "args_files", "as", "expect", "expect_files", "expect_error", "save", "select", "label",
             "wait_until", "write", "text", "text_file", "repeat", "every_s", "timeout_s", "note"}
VERIFY_KEYS = {"name", "call", "args", "args_files", "as", "expect", "expect_files", "expect_error", "save", "select",
               "playtest", "file", "transcript", "geometry", "world_diff", "series"}
# What a verifier does: one of these.
VERIFY_KINDS = ("call", "playtest", "file", "transcript", "geometry", "world_diff", "series")
CHEAT_KEYS = {"id", "note", "steps"}
GEOMETRY_KINDS = ("min_gap", "facing", "size", "world_size")
# Entity fields a world_diff never compares: bookkeeping, and what is derived from the rest.
DIFF_IGNORE = {"version", "modified_by", "created_by", "world_position", "bounds", "model_state", "journal", "children"}
# The most entities a snapshot reads (a task world is small; this bounds a runaway one).
MAX_SNAPSHOT_ENTITIES = 20000
ACTOR_KEYS = {"name", "watch", "do", "every_s", "once"}
LIMIT_KEYS = {"calls", "wall_s"}
MAX_CALLS_DEFAULT = 200
MAX_WALL_DEFAULT = 900
# What a transcript line quotes of arguments and answers.
ARGS_QUOTE = 2000
ANSWER_QUOTE = int(os.environ.get("EVALS_ANSWER_QUOTE", "600"))
# Transcript lines kept with their arguments and answers (the rest keep only their numbers).
MAX_FULL_LINES = 5000
# Tools every task may use (identity and reading the engine's own description).
ALWAYS_ALLOWED = ("agent_hello", "engine_info")
# The proxy's own tool in --agent mode.
DONE_TOOL = {
    "name": "eval_done",
    "description": "Call this once when you have finished the task (or given up). summary: one or two sentences on what you did. The run is then verified by the engine.",
    "inputSchema": {"type": "object", "properties": {"summary": {"type": "string", "description": "What you did, up to 1000 characters"}}, "required": []},
}


def ascii(s):
    return str(s).encode("ascii", "replace").decode()


def _path_pattern(path):
    """A regex for a folder as it appears in text: any separator (/ or any run of backslashes, as
    JSON escaping nests), any case."""
    parts = [re.escape(x) for x in re.split(r"[\\/]+", str(Path(path).resolve())) if x]
    # One character class with one quantifier: a nested one ((?:\\+|/)+) backtracks exponentially
    # on the long backslash runs of doubly escaped JSON.
    return re.compile(r"[\\/]+".join(parts), re.IGNORECASE)


# Fallbacks after the known folders: whatever absolute path, home folder, Claude project folder or
# session-like id is left (a path cut short by truncation no longer matches its folder's pattern).
_FALLBACKS = (
    (re.compile(r"\b[A-Za-z]:[\\/][^\s\"'<>|,;)]*"), "<abs>"),
    (re.compile(r"(?<![\w<.~])/(?:home|Users|tmp|var|private|mnt|root)/[^\s\"'<>|,;)]*"), "<abs>"),
    (re.compile(r"\b[A-Za-z]--[A-Za-z0-9][\w-]*"), "<folder>"),
    # What follows a project folder is a session's folder: mask it, however short a cut left it.
    (re.compile(r"(<folder>[\\/]+)[0-9a-fA-F][0-9a-fA-F-]*"), r"\1<id>"),
    (re.compile(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"), "<id>"),
    # An id cut short by a truncation ("12345678-90ab-cd... (40 bytes)") and a session id of any length.
    (re.compile(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F-]*(?=\.\.\.)"), "<id>"),
    (re.compile(r"\bsession_[A-Za-z0-9]{4,}"), "<session>"),
    (re.compile("App" + r"Data|Temp[\\/]+claude", re.IGNORECASE), "<private>"),
)


def scrub(text):
    """Reports and transcripts are committed (perf/evals): absolute folders become <target>, <repo>
    and ~, then anything still absolute, a Claude project folder or a session-like id is masked, so
    no machine's user name, layout or session lands in the repository."""
    for path, name in ((TARGET, "<target>"), (ROOT, "<repo>"), (Path.home(), "~")):
        text = _path_pattern(path).sub(name, text)
    for pattern, name in _FALLBACKS:
        text = pattern.sub(name, text)
    return text


def scrub_value(v):
    """`scrub` on every string inside a JSON value (before anything truncates it)."""
    if isinstance(v, str):
        return scrub(v)
    if isinstance(v, list):
        return [scrub_value(x) for x in v]
    if isinstance(v, dict):
        return {scrub(k): scrub_value(x) for k, x in v.items()}
    return v


# --------------------------------------------------------------------------------------------
# Matcher: the playtest runner's (crates/specula/src/playtest.rs), mirrored. Shared cases in
# evals/matcher_cases.json are checked against both (tests/evals_suite.rs and evals_test.py).

MATCH_KEYS = ("eq", "ne", "lt", "lte", "gt", "gte", "in", "exists", "contains", "near", "tol")


class SpecError(ValueError):
    pass


def parse_path(path):
    if path == "":
        return []
    segs = []
    for s in path.split("."):
        if s == "":
            raise SpecError(f"'{path}' has an empty segment (paths look like \"enemies.items.0.hp\")")
        if s == "#":
            segs.append(("len",))
        elif s.isdigit() and s.isascii() and len(s) <= 9:
            segs.append(("index", int(s)))
        else:
            segs.append(("key", s))
    return segs


_MISSING = object()


def lookup(v, segs):
    """`v` at `segs`, or _MISSING when the path leads nowhere."""
    cur = v
    for i, s in enumerate(segs):
        if s[0] == "len":
            if i + 1 != len(segs):
                return _MISSING
            if isinstance(cur, (list, dict, str)):
                return len(cur)
            return _MISSING
        if s[0] == "index":
            if isinstance(cur, list):
                if s[1] >= len(cur):
                    return _MISSING
                cur = cur[s[1]]
            elif isinstance(cur, dict):
                if str(s[1]) not in cur:
                    return _MISSING
                cur = cur[str(s[1])]
            else:
                return _MISSING
        else:
            if not isinstance(cur, dict) or s[1] not in cur:
                return _MISSING
            cur = cur[s[1]]
    return cur


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool)


def same(a, b):
    """JSON equality: numbers by value, bools never equal to numbers."""
    if is_num(a) and is_num(b):
        return float(a) == float(b)
    if isinstance(a, bool) or isinstance(b, bool):
        return isinstance(a, bool) and isinstance(b, bool) and a == b
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    return type(a) is type(b) and a == b


def parse_matcher(m):
    if not isinstance(m, dict):
        return [("eq", m)]
    if not m:
        raise SpecError("an empty matcher matches nothing: give a value, or {eq|ne|lt|lte|gt|gte|in|exists|contains|near: ...}")
    for k in m:
        if k not in MATCH_KEYS:
            raise SpecError(f"unknown key '{k}' (allowed: {', '.join(MATCH_KEYS)}) (to compare a whole object use {{\"eq\": {{...}}}})")
    ops = []
    for k, v in m.items():
        if k in ("lt", "lte", "gt", "gte", "near"):
            if not is_num(v):
                raise SpecError(f"'{k}' needs a number")
            if k == "near":
                tol = m.get("tol", 1e-6)
                if not is_num(tol):
                    raise SpecError("'tol' needs a number")
                ops.append(("near", float(v), abs(float(tol))))
            else:
                ops.append((k, float(v)))
        elif k == "in":
            if not isinstance(v, list):
                raise SpecError("'in' needs an array of values")
            ops.append(("in", v))
        elif k == "exists":
            if not isinstance(v, bool):
                raise SpecError("'exists' needs true or false")
            ops.append(("exists", v))
        elif k == "tol":
            if "near" not in m:
                raise SpecError("'tol' goes with 'near'")
        else:
            ops.append((k, v))
    return ops


def check_expect(path, matcher, answer):
    """(ok, got): whether `answer` at `path` matches `matcher`; got is None when missing."""
    segs, ops = parse_path(path), parse_matcher(matcher)
    got = lookup(answer, segs)
    for op in ops:
        kind = op[0]
        if kind == "exists":
            ok = (got is not _MISSING) == op[1]
        elif got is _MISSING:
            ok = False
        elif kind == "eq":
            ok = same(got, op[1])
        elif kind == "ne":
            ok = not same(got, op[1])
        elif kind in ("lt", "lte", "gt", "gte"):
            ok = is_num(got) and {"lt": got < op[1], "lte": got <= op[1], "gt": got > op[1], "gte": got >= op[1]}[kind]
        elif kind == "near":
            ok = is_num(got) and abs(got - op[1]) <= op[2]
        elif kind == "in":
            ok = any(same(got, x) for x in op[1])
        elif kind == "contains":
            x = op[1]
            if isinstance(got, str):
                ok = isinstance(x, str) and x in got
            elif isinstance(got, list):
                ok = any(same(e, x) for e in got)
            elif isinstance(got, dict):
                ok = isinstance(x, str) and x in got
            else:
                ok = False
        else:
            ok = False
        if not ok:
            return False, (None if got is _MISSING else got)
    return True, (None if got is _MISSING else got)


def check_all(expect, answer):
    """Failures of an `expect` object against an answer: [{path, want, got}]."""
    fails = []
    for path, m in (expect or {}).items():
        ok, got = check_expect(path, m, answer)
        if not ok:
            fails.append({"path": path, "want": m, "got": cut(scrub_value(got), 240)})
    return fails


def cut(v, n):
    """`v` for a report: scrubbed first (a cut path or id no longer matches its pattern), then cut
    to about n characters of JSON."""
    v = scrub_value(v)
    s = json.dumps(v)
    return v if len(s) <= n else s[:n] + f"... ({len(s)} bytes)"


# --------------------------------------------------------------------------------------------
# Variables: a step's `save: name` keeps its answer; "${name.dotted.path}" in later arguments is
# replaced by that value (a whole-string reference keeps the value's type).

VAR_RE = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)((?:\.[^.}]+)*)\}")


def var_value(vars_, name, rest):
    if name not in vars_:
        raise SpecError(f"unknown variable '{name}' (saved so far: {', '.join(sorted(vars_)) or 'none'})")
    v = lookup(vars_[name], parse_path(rest[1:]) if rest else [])
    if v is _MISSING:
        raise SpecError(f"'${{{name}{rest}}}' leads nowhere in the saved answer: {cut(vars_[name], 300)}")
    return v


def resolve(obj, vars_):
    if isinstance(obj, str):
        m = VAR_RE.fullmatch(obj)
        if m:
            return var_value(vars_, m.group(1), m.group(2))
        return VAR_RE.sub(lambda mm: str(var_value(vars_, mm.group(1), mm.group(2))) if not isinstance(
            var_value(vars_, mm.group(1), mm.group(2)), (dict, list)) else json.dumps(var_value(vars_, mm.group(1), mm.group(2))), obj)
    if isinstance(obj, list):
        return [resolve(x, vars_) for x in obj]
    if isinstance(obj, dict):
        return {k: resolve(v, vars_) for k, v in obj.items()}
    return obj


def select(answer, sel):
    """`select: {from: "items", where: {path: matcher, ...}, index?: 0}`: the matching element of
    an array in the answer (so expectations needn't know array order). None when none matches."""
    arr = lookup(answer, parse_path(sel.get("from", "")))
    if arr is _MISSING or not isinstance(arr, list):
        return None
    hits = [e for e in arr if not check_all(sel.get("where", {}), e)]
    if sel.get("count"):
        return {"count": len(hits), "items": hits[:50]}
    i = sel.get("index", 0)
    return hits[i] if i < len(hits) else None


# --------------------------------------------------------------------------------------------
# Tasks


def tool_allowed(name, patterns):
    return isinstance(name, str) and (name in ALWAYS_ALLOWED or any(fnmatch.fnmatchcase(name, p) for p in patterns))


def refusal(tool, args, allowed):
    """Why the task's tools don't allow this call ({error, message}), or None. A playtest's spec
    makes calls too, so its steps are held to the same list; specs from files aren't allowed (their
    calls can't be checked before they run)."""
    if not isinstance(tool, str) or not tool:
        return {"error": "bad_request", "message": "the tool name must be a string"}
    if not tool_allowed(tool, allowed):
        return {"error": "tool_not_allowed", "message": f"{tool} isn't one of this task's tools ({', '.join(allowed)})"}
    if tool == "playtest_run" and isinstance(args, dict):
        if args.get("path") is not None or args.get("replay") is not None:
            return {"error": "tool_not_allowed", "message": "in an eval, playtest_run takes its spec inline (spec), so its calls can be checked against the task's tools"}
        spec = args.get("spec")
        calls = []
        if isinstance(spec, dict):
            for key in ("steps", "end"):
                for st in spec.get(key) or []:
                    calls.append(st.get("call") if isinstance(st, dict) else None)
            if isinstance(spec.get("until"), dict):
                calls.append(spec["until"].get("call"))
        for c in calls:
            if not tool_allowed(c, allowed):
                return {"error": "tool_not_allowed", "message": f"the playtest calls {c}, which isn't one of this task's tools ({', '.join(allowed)})"}
    return None


class DuplicateKey(ValueError):
    pass


def _no_duplicates(pairs):
    seen = {}
    for k, v in pairs:
        if k in seen:
            raise DuplicateKey(f"duplicate key '{k}'")
        seen[k] = v
    return seen


def _no_constant(name):
    # NaN and Infinity aren't JSON (Python accepts them; the engine doesn't).
    raise ValueError(f"{name} isn't JSON")


def screen(body, allowed):
    """The proxy's check of one request body: (message to forward or None, error or None). Duplicate
    keys are refused (Python keeps the last, the engine's JSON the first, so a duplicate could show
    the proxy one tool and run another), batches and non-string methods too; what is forwarded is
    the checked message re-serialised, never the raw bytes."""
    try:
        msg = json.loads(body, object_pairs_hook=_no_duplicates, parse_constant=_no_constant)
    except DuplicateKey as e:
        return None, {"code": -32600, "message": f"refused by the eval proxy: {e}"}
    except ValueError:
        return None, {"code": -32700, "message": "refused by the eval proxy: not JSON"}
    if not isinstance(msg, dict):
        return None, {"code": -32600, "message": "refused by the eval proxy: send one JSON-RPC object (no batches)"}
    if not isinstance(msg.get("method"), str):
        return None, {"code": -32600, "message": "refused by the eval proxy: 'method' must be a string"}
    if msg["method"] == "tools/call":
        # A call without an id is a notification: it would run without an answer to log.
        if isinstance(msg.get("id"), bool) or not isinstance(msg.get("id"), (str, int)):
            return None, {"code": -32600, "message": "refused by the eval proxy: tools/call needs an id (a string or an integer)"}
        params = msg.get("params")
        if not isinstance(params, dict) or not isinstance(params.get("name"), str):
            return None, {"code": -32602, "message": "refused by the eval proxy: tools/call needs params.name (a string)"}
        if params.get("arguments") is not None and not isinstance(params["arguments"], dict):
            return None, {"code": -32602, "message": "refused by the eval proxy: params.arguments must be an object"}
    return msg, None


def validate_steps(steps, where, errors, verify=False):
    if not isinstance(steps, list):
        errors.append(f"{where}: must be an array of steps")
        return
    for i, s in enumerate(steps):
        w = f"{where}[{i}]"
        if not isinstance(s, dict):
            errors.append(f"{w}: must be an object")
            continue
        allowed = VERIFY_KEYS if verify else STEP_KEYS
        for k in s:
            if k not in allowed:
                errors.append(f"{w}: unknown key '{k}' (allowed: {', '.join(sorted(allowed))})")
        kinds = [k for k in (VERIFY_KINDS if verify else ("call", "write", "wait_until")) if k in s]
        if len(kinds) != 1:
            errors.append(f"{w}: needs exactly one of {', '.join(VERIFY_KINDS) if verify else 'call, write, wait_until'} (has {kinds or 'none'})")
        if "geometry" in s and (not isinstance(s["geometry"], dict) or s["geometry"].get("kind") not in GEOMETRY_KINDS or "query" not in s["geometry"]):
            errors.append(f"{w}.geometry: needs kind ({', '.join(GEOMETRY_KINDS)}) and query")
        if "transcript" in s:
            tr = s["transcript"]
            calls = (tr.get("sequence") or []) + tr.get("none", []) + tr.get("count", []) if isinstance(tr, dict) and any(k in tr for k in ("sequence", "none", "count")) else [tr]
            if not isinstance(calls, list) or not calls or not all(isinstance(c, dict) and isinstance(c.get("tool"), str) for c in calls):
                errors.append(f"{w}.transcript: needs tool (or sequence / none / count: [{{tool, args?, answer?}}, ...])")
        if "world_diff" in s:
            wd = s["world_diff"]
            if not isinstance(wd, dict) or not all(isinstance(r, dict) and isinstance(r.get("path"), str) for r in wd.get("allow", [])):
                errors.append(f"{w}.world_diff: allow is a list of {{path, ops?, fields?}}")
        if "series" in s:
            se = s["series"]
            if not isinstance(se, dict) or not isinstance(se.get("ticks"), int) or not isinstance(se.get("call"), dict) or not isinstance(se.get("value"), str):
                errors.append(f"{w}.series: needs ticks, call {{call, args}} and value (a dotted path)")
        for path, ref in (s.get("expect_files") or {}).items():
            if not isinstance(ref, dict) or not (ROOT / str(ref.get("file"))).is_file():
                errors.append(f"{w}.expect_files.{path}: needs file (an existing repository file) and at")
        if verify and not s.get("name"):
            errors.append(f"{w}: a verifier needs a 'name' (what it proves)")
        for key in ("expect",):
            if key in s:
                if not isinstance(s[key], dict):
                    errors.append(f"{w}.{key}: must be an object of path: matcher")
                    continue
                for p, m in s[key].items():
                    try:
                        parse_path(p)
                        parse_matcher(m)
                    except SpecError as e:
                        errors.append(f"{w}.expect.{p}: {e}")
        if "wait_until" in s:
            wu = s["wait_until"]
            if not isinstance(wu, dict) or "call" not in wu or "expect" not in wu:
                errors.append(f"{w}.wait_until: needs call and expect")
        if "write" in s and not ("text" in s or "text_file" in s):
            errors.append(f"{w}: write needs text or text_file")
        if "playtest" in s and not isinstance(s["playtest"], dict):
            errors.append(f"{w}.playtest: must be a playtest spec object")


def validate_task(t, path_name=None, root=ROOT):
    """Problems with a task document (empty when it is valid)."""
    errors = []
    if not isinstance(t, dict):
        return ["a task must be a JSON object"]
    for k in t:
        if k not in TASK_KEYS:
            errors.append(f"unknown key '{k}' (allowed: {', '.join(sorted(TASK_KEYS))})")
    if t.get("eval") != EVAL_FORMAT:
        errors.append(f"'eval' must be {EVAL_FORMAT} (the task format version)")
    tid = t.get("id")
    if not isinstance(tid, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,47}", tid or ""):
        errors.append("'id' must be [a-z][a-z0-9_]* (at most 48 characters)")
    elif path_name and path_name != tid:
        errors.append(f"the file is named {path_name}.json but its id is {tid}")
    if t.get("set") not in SETS:
        errors.append(f"'set' must be one of {', '.join(SETS)}")
    for k in ("title", "brief", "area"):
        if not isinstance(t.get(k), str) or not t.get(k).strip():
            errors.append(f"'{k}' is required (a non-empty string)")
    if t.get("game") not in GAME_EXES:
        errors.append(f"'game' must be one of {', '.join(str(g) for g in GAME_EXES)}")
    if t.get("engine_cwd", "run") not in ("run", "repo"):
        errors.append("'engine_cwd' must be \"run\" (a fresh folder, default) or \"repo\" (the checkout: module builds)")
    tools = t.get("tools")
    if not isinstance(tools, list) or not tools or not all(isinstance(p, str) and p for p in tools):
        errors.append("'tools' must be a non-empty array of tool names or patterns (\"world_*\")")
    for k in ("needs", "engine_args"):
        if k in t and not (isinstance(t[k], list) and all(isinstance(x, str) for x in t[k])):
            errors.append(f"'{k}' must be an array of strings")
    if "limits" in t:
        if not isinstance(t["limits"], dict) or any(k not in LIMIT_KEYS for k in t["limits"]):
            errors.append(f"'limits' takes {', '.join(sorted(LIMIT_KEYS))}")
    validate_steps(t.get("setup", []), "setup", errors)
    for k in ("reference", "broken"):
        if not t.get(k):
            errors.append(f"'{k}' is required: a scripted solution that {'passes' if k == 'reference' else 'must fail'} every/some verifier")
        else:
            validate_steps(t[k], k, errors)
            for i, s in enumerate(t[k]):
                for c in [s.get("call"), (s.get("wait_until") or {}).get("call")]:
                    if c and isinstance(tools, list) and not tool_allowed(c, tools):
                        errors.append(f"{k}[{i}]: calls {c}, which the task's 'tools' don't allow")
    for i, c in enumerate(t.get("cheats", []) if isinstance(t.get("cheats", []), list) else [None]):
        if not isinstance(c, dict) or any(k not in CHEAT_KEYS for k in c) or not re.fullmatch(r"[a-z][a-z0-9_]{0,31}", str(c.get("id"))) or not c.get("steps"):
            errors.append(f"cheats[{i}]: needs id ([a-z][a-z0-9_]*), note and steps (another broken solution the verifiers must catch)")
            continue
        validate_steps(c["steps"], f"cheats[{i}].steps", errors)
        for j, st in enumerate(c["steps"]):
            for cc in [st.get("call"), (st.get("wait_until") or {}).get("call")]:
                if cc and isinstance(tools, list) and not tool_allowed(cc, tools):
                    errors.append(f"cheats[{i}].steps[{j}]: calls {cc}, which the task's tools don't allow")
    if not t.get("verify"):
        errors.append("'verify' is required: at least one verifier")
    else:
        validate_steps(t["verify"], "verify", errors, verify=True)
        names = [v.get("name") for v in t["verify"] if isinstance(v, dict)]
        if len(set(names)) != len(names):
            errors.append("verifier names must be unique")
    for i, a in enumerate(t.get("actors", [])):
        if not isinstance(a, dict) or any(k not in ACTOR_KEYS for k in a) or "watch" not in a or "do" not in a or "name" not in a:
            errors.append(f"actors[{i}]: needs name, watch {{call, args, expect, select?}} and do [steps] (keys: {', '.join(sorted(ACTOR_KEYS))})")
        else:
            validate_steps(a["do"], f"actors[{i}].do", errors)
    # Files the task reads must exist.
    for where, steps in (("setup", t.get("setup", [])), ("reference", t.get("reference", [])), ("broken", t.get("broken", [])),
                         ("verify", t.get("verify", []))):
        for i, s in enumerate(steps if isinstance(steps, list) else []):
            if not isinstance(s, dict):
                continue
            files = list((s.get("args_files") or {}).values()) + ([s["text_file"]] if "text_file" in s else [])
            for f in files:
                if not (root / f).is_file():
                    errors.append(f"{where}[{i}]: file {f} doesn't exist (paths are relative to the repo root)")
    for f in t.get("files", []) if isinstance(t.get("files", []), list) else []:
        if not isinstance(f, dict) or "from" not in f or "to" not in f:
            errors.append("'files' entries are {from: repo path, to: path in the engine's folder}")
        elif not (root / f["from"]).exists():
            errors.append(f"files: {f['from']} doesn't exist")
    return errors


def load_tasks(sets=SETS, ids=None, root=EVALS_DIR):
    """{id: task}, sorted by set then id; raises SystemExit on invalid files."""
    tasks, problems = {}, []
    for s in sets:
        for p in sorted((root / s).glob("*.json")):
            try:
                t = json.loads(p.read_text(encoding="utf-8"), object_pairs_hook=_no_duplicates)
            except ValueError as e:
                problems.append(f"{p.name}: not JSON: {e}")
                continue
            errs = validate_task(t, p.stem)
            if t.get("set") != s:
                errs.append(f"it is in {s}/ but its set is {t.get('set')}")
            problems += [f"{s}/{p.name}: {e}" for e in errs]
            if not errs:
                tasks[t["id"]] = t
    if problems:
        raise SystemExit("invalid task files:\n  " + "\n  ".join(ascii(p) for p in problems))
    if ids:
        missing = [i for i in ids if i not in tasks]
        if missing:
            raise SystemExit(f"no such task: {', '.join(missing)} (have: {', '.join(tasks)})")
        tasks = {i: tasks[i] for i in ids}
    return tasks


# --------------------------------------------------------------------------------------------
# Transcripts and clients


class Transcript:
    """Every MCP call of a run, by agent: tool, arguments, bytes, ms, error code. Thread-safe."""

    def __init__(self, path=None):
        self.path, self.lock, self.lines, self.polls = path, threading.Lock(), [], 0
        self.t0 = time.perf_counter()

    def add(self, agent, phase, tool, args, req_bytes, resp_bytes, ms, error, answer):
        if phase == "actor_poll":
            # An actor's watch calls (several a second) are counted, not listed: what it did is.
            with self.lock:
                self.polls += 1
            return
        # Scrubbed before being cut: a cut path no longer matches its folder.
        line = {"t_s": round(time.perf_counter() - self.t0, 3), "agent": agent, "phase": phase, "tool": tool,
                "args": cut(scrub_value(args), ARGS_QUOTE), "req_bytes": req_bytes, "resp_bytes": resp_bytes, "ms": round(ms, 1)}
        if error:
            line["error"] = error
        line["answer"] = cut(scrub_value(answer), ANSWER_QUOTE)
        # Kept in memory (never written) for `transcript` verifiers.
        line["_args"], line["_answer"] = args, answer
        with self.lock:
            if len(self.lines) >= MAX_FULL_LINES:
                # A runaway agent (refused calls cost the engine nothing) can't grow the transcript
                # without bound: past the cap, lines keep their numbers but not their text.
                for k in ("args", "answer", "_args", "_answer"):
                    line.pop(k)
            self.lines.append(line)

    def stats(self, phase=None):
        ls = [x for x in self.lines if phase is None or x["phase"] == phase]
        req, resp = sum(x["req_bytes"] for x in ls), sum(x["resp_bytes"] for x in ls)
        by_tool = {}
        for x in ls:
            by_tool[x["tool"]] = by_tool.get(x["tool"], 0) + 1
        errors = {}
        for x in ls:
            if x.get("error"):
                errors[x["error"]] = errors.get(x["error"], 0) + 1
        return {"calls": len(ls), "req_bytes": req, "resp_bytes": resp,
                # A rough token count (about 4 bytes of JSON a token); real counts come from the agent.
                "tokens_est": (req + resp) // 4, "errors": sum(errors.values()), "error_codes": errors,
                "refused_not_allowed": errors.get("tool_not_allowed", 0), "by_tool": dict(sorted(by_tool.items()))}

    def write(self):
        if self.path:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            with open(self.path, "w") as f:
                for x in self.lines:
                    f.write(scrub(json.dumps({k: v for k, v in x.items() if not k.startswith("_")})) + "\n")


def answer_of(result):
    """The first text content parsed as JSON when it is (else the text), like playtest::answer_json."""
    text = next((c.get("text") for c in result.get("content", []) if c.get("type") == "text"), None)
    if text is None:
        return None
    try:
        return json.loads(text)
    except ValueError:
        return text


def error_code(answer):
    """An error answer's code (its `error`, or `code` when more specific), or 'error'."""
    if isinstance(answer, dict):
        for k in ("code", "error"):
            if isinstance(answer.get(k), str):
                return answer[k]
    return "error"


def error_codes(answer):
    out = []
    if isinstance(answer, dict):
        for k in ("error", "code"):
            if isinstance(answer.get(k), str):
                out.append(answer[k])
        cause = answer.get("cause")
        if isinstance(cause, dict):
            out += error_codes(cause)
    elif isinstance(answer, str):
        out.append(answer)
    return out


class Client:
    """One agent's MCP session on an engine, logged to a transcript, limited to allowed tools."""

    def __init__(self, url, agent, transcript, phase, allowed=None, detailed=True):
        self.mcp = Mcp(url, f"eval-{agent}", detailed=detailed)
        self.agent, self.transcript, self.phase, self.allowed = agent, transcript, phase, allowed

    def call(self, tool, args):
        """(is_error, answer). Tools the task doesn't allow are refused here (as the proxy would)."""
        refused = refusal(tool, args, self.allowed) if self.allowed is not None else None
        if refused:
            self.transcript.add(self.agent, self.phase, tool, args, 0, 0, 0, refused["error"], refused)
            return True, refused
        if self.mcp.detailed:
            from specula_mcp import detailed
            args = detailed(tool, args)
        body = json.dumps({"name": tool, "arguments": args})
        t0 = time.perf_counter()
        result = self.mcp.request("tools/call", {"name": tool, "arguments": args})
        ms = (time.perf_counter() - t0) * 1000
        resp = json.dumps(result)
        ans = answer_of(result)
        err = bool(result.get("isError"))
        self.transcript.add(self.agent, self.phase, tool, args, len(body), len(resp), ms, error_code(ans) if err else None, ans)
        if not err and isinstance(result.get("content"), list):
            ans_images = [c for c in result["content"] if c.get("type") == "image"]
            if ans_images and isinstance(ans, dict):
                ans.setdefault("_images", len(ans_images))
        return err, ans

    def close(self):
        self.mcp.close()


# --------------------------------------------------------------------------------------------
# Running steps


class StepFailed(Exception):
    pass


def step_args(step, vars_, root=ROOT):
    args = dict(step.get("args") or {})
    for key, f in (step.get("args_files") or {}).items():
        args[key] = json.loads((root / f).read_text(encoding="utf-8"))
    return resolve(args, vars_)


class Ctx:
    """What a task run's steps share: the engine URL, its folder, the transcript, the clients
    (one per agent name, so claims and identity belong to that agent), variables."""

    def __init__(self, url, workdir, transcript, allowed, deadline, max_calls):
        self.url, self.workdir, self.transcript, self.allowed = url, Path(workdir), transcript, allowed
        self.deadline, self.max_calls = deadline, max_calls
        self.clients, self.lock = {}, threading.Lock()
        self.snapshot = None
        self.vars = {}
        self.stop = threading.Event()

    def client(self, agent, phase, restricted):
        key = (agent, phase if phase in ("verify",) else "")
        with self.lock:
            c = self.clients.get(key)
            if c is None:
                c = Client(self.url, agent, self.transcript, phase, self.allowed if restricted else None)
                if agent not in ("verifier", "snapshot"):
                    err, ans = c.call("agent_hello", {"name": agent, "role": "builder" if restricted else "lead"})
                    if err:
                        raise StepFailed(f"agent_hello as {agent}: {cut(ans, 300)}")
                self.clients[key] = c
            c.phase = phase
            return c

    def close(self):
        for c in self.clients.values():
            c.close()


def run_call(ctx, step, phase, vars_, restricted, default_agent):
    agent = step.get("as", default_agent)
    c = ctx.client(agent, phase, restricted)
    if time.perf_counter() > ctx.deadline:
        raise StepFailed(f"the run's wall-time limit passed before {step['call']}")
    if restricted and ctx.transcript.stats("solution")["calls"] >= ctx.max_calls:
        raise StepFailed(f"the task's call limit ({ctx.max_calls}) is spent")
    err, ans = c.call(step["call"], step_args(step, vars_))
    return err, ans


def expand_expect_files(step):
    """`expect_files: {answer path: {file, at}}` adds `{answer path: {eq: <the file's value at 'at'>}}`
    to `expect`: an answer compared with a repository file (a rule set as shipped, say)."""
    if not step.get("expect_files"):
        return step
    expect = dict(step.get("expect") or {})
    for path, ref in step["expect_files"].items():
        doc = json.loads((ROOT / ref["file"]).read_text(encoding="utf-8"))
        v = lookup(doc, parse_path(ref.get("at", "")))
        if v is _MISSING:
            raise SpecError(f"expect_files: {ref['file']} has nothing at {ref.get('at')}")
        expect[path] = {"eq": v}
    return {**step, "expect": expect}


def judge(step, err, ans):
    """Failure text of a call step's answer against its expectations, or None."""
    want_err = step.get("expect_error")
    if want_err:
        if not err:
            return f"expected an error{'' if want_err is True else ' ' + str(want_err)}, got {cut(ans, 300)}"
        if want_err is not True and want_err not in error_codes(ans):
            return f"expected error {want_err}, got {cut(ans, 300)}"
        return None
    if err:
        return f"{step.get('call')} failed: {cut(ans, 400)}"
    step = expand_expect_files(step)
    target = ans
    if "select" in step:
        target = select(ans, step["select"])
        if target is None:
            return f"select {json.dumps(step['select'])[:200]} matched nothing in {cut(ans, 300)}"
    fails = check_all(step.get("expect"), target)
    if fails:
        return "; ".join(f"{f['path']}: want {json.dumps(f['want'])}, got {json.dumps(f['got'])[:200]}" for f in fails)
    return None


def with_vars(step, vars_):
    """`step` with variables replaced in what is judged (select, expect); args resolve when sent."""
    keys = [k for k in ("select", "expect", "file") if k in step]
    return {**step, **resolve({k: step[k] for k in keys}, vars_)} if keys else step


def run_steps(ctx, steps, phase, restricted, default_agent, vars_=None):
    """Runs solution/setup steps in order. Raises StepFailed on a step whose expectations fail."""
    vars_ = ctx.vars if vars_ is None else vars_
    for i, step in enumerate(steps):
        step = with_vars(step, vars_)
        if ctx.stop.is_set():
            raise StepFailed("stopped")
        label = step.get("label") or step.get("call") or ("write" if "write" in step else "wait_until")
        if "write" in step:
            dest = ctx.workdir / resolve(step["write"], vars_)
            if not dest.resolve().is_relative_to(ctx.workdir.resolve()):
                raise StepFailed(f"{phase}[{i}]: write outside the engine's folder: {dest}")
            dest.parent.mkdir(parents=True, exist_ok=True)
            text = step["text"] if "text" in step else (ROOT / step["text_file"]).read_text(encoding="utf-8")
            dest.write_text(resolve(text, vars_) if "text" in step else text, encoding="utf-8")
            ctx.transcript.add(step.get("as", default_agent), phase, "(write file)", {"path": str(step["write"])}, len(text), 0, 0, None, None)
            continue
        if "wait_until" in step:
            wu = with_vars(step["wait_until"], vars_)
            limit = time.perf_counter() + float(step.get("timeout_s", 30))
            while True:
                err, ans = run_call(ctx, {**wu, "as": step.get("as", default_agent)}, phase, vars_, restricted, default_agent)
                probs = judge(wu, err, ans)
                if probs is None:
                    break
                if time.perf_counter() > limit:
                    raise StepFailed(f"{phase}[{i}] {label}: waited {step.get('timeout_s', 30)} s: {probs}")
                time.sleep(float(step.get("every_s", 0.5)))
            if step.get("save"):
                vars_[step["save"]] = select(ans, wu["select"]) if "select" in wu else ans
            continue
        for _ in range(int(step.get("repeat", 1))):
            err, ans = run_call(ctx, step, phase, vars_, restricted, default_agent)
        probs = judge(step, err, ans)
        if probs is not None and phase != "solution":
            raise StepFailed(f"{phase}[{i}] {label}: {probs}")
        if probs is not None and (step.get("expect") or step.get("expect_error") or step.get("select")):
            # A scripted solution checks its own steps too: a reference that stops working says where.
            raise StepFailed(f"{phase}[{i}] {label}: {probs}")
        if step.get("save"):
            vars_[step["save"]] = select(ans, step["select"]) if "select" in step else ans


class Actor(threading.Thread):
    """A scripted participant (another agent in the world): polls `watch` and runs `do` once the
    watched answer matches; the matched element (with `select`) is the variable `match`."""

    def __init__(self, ctx, spec, log):
        super().__init__(daemon=True)
        self.ctx, self.spec, self.log = ctx, spec, log
        self.fired, self.error = 0, None

    def run(self):
        name, watch = self.spec["name"], self.spec["watch"]
        try:
            while not self.ctx.stop.is_set():
                c = self.ctx.client(name, "actor_poll", False)
                watch_now = with_vars(watch, self.ctx.vars)
                err, ans = c.call(watch["call"], step_args(watch, self.ctx.vars))
                target = select(ans, watch_now["select"]) if (not err and "select" in watch_now) else ans
                if not err and target is not None and not check_all(watch_now.get("expect"), target):
                    local = dict(self.ctx.vars, match=target)
                    run_steps(self.ctx, self.spec["do"], "actor", False, name, local)
                    self.fired += 1
                    if self.spec.get("once", True):
                        return
                self.ctx.stop.wait(float(self.spec.get("every_s", 0.5)))
        except Exception as e:  # reported with the task's result
            self.error = ascii(scrub(str(e)))[:500]


def run_playtest(ctx, client, spec, vars_):
    err, r = client.call("playtest_run", {"spec": resolve(spec, vars_), "wait_s": 20})
    if err:
        return err, r
    while isinstance(r, dict) and r.get("status") == "running":
        err, r = client.call("playtest_report", {"id": r["id"], "wait_s": 20})
        if err:
            return err, r
    return False, r


def transcript_check(ctx, spec):
    """`transcript: {tool, args?, answer?}`: the solution made this call and it succeeded, with
    arguments and answer matching (the agent did the step, not just reached its end state).
    `transcript: {sequence: [{tool, args?, answer?}, ...]}`: it made these calls in this order (others
    may come between)."""
    lines = [ln for ln in list(ctx.transcript.lines) if ln["phase"] == "solution" and not ln.get("error") and "_args" in ln]

    def matches(want, line):
        return line["tool"] == want["tool"] and not check_all(want.get("args"), line["_args"]) and not check_all(want.get("answer"), line["_answer"])

    # `none`: calls that must not have happened (successfully); `count`: exactly this many of a call.
    for want in spec.get("none", []):
        hit = next((ln for ln in lines if matches(want, ln)), None)
        if hit:
            return f"the solution called {want['tool']} with {cut(hit['_args'], 200)}, which this task rules out"
    for want in spec.get("count", []):
        n = sum(1 for ln in lines if matches(want, ln))
        if n != want["n"]:
            return f"the solution called {want['tool']} {json.dumps(want.get('args'))} {n} times; exactly {want['n']} wanted"
    if "sequence" not in spec and "tool" not in spec:
        return None
    wanted = spec["sequence"] if "sequence" in spec else [spec]
    found = []
    for line in lines:
        if len(found) == len(wanted):
            break
        if matches(wanted[len(found)], line):
            found.append(line)
    if len(found) < len(wanted):
        want = wanted[len(found)]
        done = f" (after {len(found)} of the {len(wanted)} calls in order)" if len(wanted) > 1 else ""
        return f"the solution never called {want['tool']} successfully with {json.dumps(want.get('args'))} answering {json.dumps(want.get('answer'))}{done}"
    # `equal: [{a, b, path}]`: two of the sequence's answers agree (the hash before and after a reload).
    for eq in spec.get("equal", []):
        va = lookup(found[eq["a"]]["_answer"], parse_path(eq["path"]))
        vb = lookup(found[eq["b"]]["_answer"], parse_path(eq["path"]))
        if va is _MISSING or not same(va, vb):
            return f"call {eq['a'] + 1} and call {eq['b'] + 1} of the sequence answered {eq['path']} differently: {cut(None if va is _MISSING else va, 80)} vs {cut(None if vb is _MISSING else vb, 80)}"
    return None


def rotation_matrix(rotation_deg):
    """The 3x3 matrix of a rotation_deg (about X, then Y, then Z: R = Rz Ry Rx), rows of columns."""
    a, b, c = (math.radians(d) for d in rotation_deg)
    rx = [[1, 0, 0], [0, math.cos(a), -math.sin(a)], [0, math.sin(a), math.cos(a)]]
    ry = [[math.cos(b), 0, math.sin(b)], [0, 1, 0], [-math.sin(b), 0, math.cos(b)]]
    rz = [[math.cos(c), -math.sin(c), 0], [math.sin(c), math.cos(c), 0], [0, 0, 1]]
    return _matmul(rz, _matmul(ry, rx))


def _matmul(m, n):
    return [[sum(m[i][k] * n[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def world_matrix(*transforms):
    """The 3x3 linear part of an entity's world transform: the product of R * diag(scale) over its
    ancestors (outermost first) and itself. Each transform is (rotation_deg, scale) or a bare
    rotation_deg; a scale is a number or [x, y, z]."""
    m = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    for t in transforms:
        rot, scale = (t, 1) if not isinstance(t, tuple) else t
        sc = [scale] * 3 if isinstance(scale, (int, float)) else list(scale or [1, 1, 1])
        r = rotation_matrix(rot or [0, 0, 0])
        m = _matmul(m, [[r[i][j] * sc[j] for j in range(3)] for i in range(3)])
    return m


def forward_xz(*transforms):
    """The X/Z direction of an entity's local +Z in the world (M * Z, normalised in X/Z), given its
    own transform after its ancestors' (outermost first): a yaw of -176 degrees may read
    [180, -4, 180], a parent turned 180 degrees turns its children's fronts, and a parent's
    non-uniform scale bends them."""
    m = world_matrix(*transforms)
    fx, fz = m[0][2], m[2][2]
    n = math.hypot(fx, fz) or 1.0
    return fx / n, fz / n


def world_size(*transforms):
    """The world lengths of a unit cube's three edges after the transforms (the columns of M)."""
    m = world_matrix(*transforms)
    return [math.sqrt(sum(m[i][j] ** 2 for i in range(3))) for j in range(3)]


def _box_gap(a, b):
    """Gap in X/Z between two bounds boxes (0 when they overlap)."""
    dx = max(a["min"][0] - b["max"][0], b["min"][0] - a["max"][0], 0.0)
    dz = max(a["min"][2] - b["max"][2], b["min"][2] - a["max"][2], 0.0)
    return math.hypot(dx, dz)


def geometry_check(client, spec):
    """Measurements a matcher can't make, computed here from the engine's own answers (bounds,
    positions, rotations): `min_gap` (pairwise X/Z gap between the queried entities' bounds),
    `facing` (each one's front, local +Z turned by its yaw, points at a target within max_deg), `size`
    (bounds extents, any quarter turn about Y)."""
    q = dict(spec["query"], fields=["path", "bounds", "world_position", "rotation_deg", "scale"], limit=500, response_format="detailed")
    err, ans = client.call("world_query", q)
    if err:
        return f"world_query failed: {cut(ans, 300)}"
    items = ans.get("items", [])
    if len(items) < spec.get("min_count", 1):
        return f"the query found {len(items)} entities, at least {spec.get('min_count', 1)} expected"
    kind = spec["kind"]
    if kind == "min_gap":
        worst = None
        for i, a in enumerate(items):
            for b in items[i + 1:]:
                g = _box_gap(a["bounds"], b["bounds"])
                if worst is None or g < worst[0]:
                    worst = (g, a["path"], b["path"])
        if worst and worst[0] < spec["min"]:
            return f"{worst[1]} and {worst[2]} are {worst[0]:.3f} m apart; at least {spec['min']} m wanted"
        return None
    if kind == "facing":
        err, t = client.call("world_query", {"path_prefix": spec["target"], "fields": ["path", "world_position"], "limit": 1, "response_format": "detailed"})
        if err or not t.get("items"):
            return f"no target {spec['target']}"
        tx, tz = t["items"][0]["world_position"][0], t["items"][0]["world_position"][2]
        for it in items:
            x, z = it["world_position"][0], it["world_position"][2]
            fx, fz = forward_xz(*_chain(client, it["path"]))
            dx, dz = tx - x, tz - z
            n = math.hypot(dx, dz)
            if n < 1e-6:
                continue
            ang = math.degrees(math.acos(max(-1.0, min(1.0, (fx * dx + fz * dz) / n))))
            if ang > spec["max_deg"]:
                return f"{it['path']} faces {ang:.1f} degrees away from {spec['target']}; at most {spec['max_deg']}"
        return None
    if kind == "size":
        want, tol = spec["size"], spec.get("tol", 0.02)
        for it in items:
            b = it["bounds"]
            ext = [b["max"][k] - b["min"][k] for k in range(3)]
            ok_y = abs(ext[1] - want[1]) <= tol
            ok_xz = all(abs(g - w) <= tol for g, w in zip(sorted([ext[0], ext[2]]), sorted([want[0], want[2]])))
            if not (ok_y and ok_xz):
                return f"{it['path']} measures {[round(e, 3) for e in ext]} m; {want} wanted (within {tol})"
        return None
    if kind == "world_size":
        # Edge lengths of the entity's unit shape in the world, through every ancestor's rotation
        # and scale (a child of a stretched parent is stretched too).
        want, tol = spec["size"], spec.get("tol", 0.02)
        for it in items:
            ext = world_size(*_chain(client, it["path"]))
            ok_y = abs(ext[1] - want[1]) <= tol
            ok_xz = all(abs(g - w) <= tol for g, w in zip(sorted([ext[0], ext[2]]), sorted([want[0], want[2]])))
            if not (ok_y and ok_xz):
                return f"{it['path']} is {[round(e, 3) for e in ext]} m in the world; {want} wanted (within {tol})"
        return None
    return f"unknown geometry kind {kind}"


def _chain(client, path):
    """(rotation_deg, scale) of every ancestor of `path` and of the entity itself, outermost first."""
    parts = path.strip("/").split("/")
    out = []
    for k in range(1, len(parts) + 1):
        p = "/" + "/".join(parts[:k])
        e, a = client.call("world_query", {"path_prefix": p, "fields": ["path", "rotation_deg", "scale"], "limit": 1, "response_format": "detailed"})
        it = a["items"][0] if not e and a.get("items") else {}
        out.append((it.get("rotation_deg") or [0, 0, 0], it.get("scale") or 1))
    return out


# --------------------------------------------------------------------------------------------
# world_diff: nothing changed but what the task allows


def world_snapshot(client):
    """Every persistent entity by path (whole, as world_list gives it) plus the environment as
    "<env>". Transient entities (a hosted game's presentation) are left out: agents can't touch them."""
    ents, cursor = {}, None
    while True:
        args = {"limit": 200, "response_format": "detailed", **({"cursor": cursor} if cursor else {})}
        err, a = client.call("world_list", args)
        if err:
            raise StepFailed(f"world_list for the snapshot failed: {cut(a, 300)}")
        for e in a.get("entities", []):
            if not e.get("transient"):
                ents[e["path"]] = e
        cursor = a.get("next_cursor")
        if not cursor:
            break
        if len(ents) > MAX_SNAPSHOT_ENTITIES:
            raise StepFailed(f"the world has over {MAX_SNAPSHOT_ENTITIES} entities: too many to diff")
    err, env = client.call("world_env", {})
    ents["<env>"] = {} if err else env
    return ents


def leaf_diff(a, b, prefix=""):
    """Dotted paths where two JSON values differ: objects and equal-length arrays are compared
    member by member, anything else as a whole."""
    if isinstance(a, dict) and isinstance(b, dict):
        out = []
        for k in sorted(set(a) | set(b)):
            if not prefix and k in DIFF_IGNORE:
                continue
            if k not in a or k not in b:
                out.append(prefix + k)
            else:
                out += leaf_diff(a[k], b[k], prefix + k + ".")
        return out
    if isinstance(a, list) and isinstance(b, list) and len(a) == len(b) and any(isinstance(x, (dict, list)) for x in a + b):
        out = []
        for i, (x, y) in enumerate(zip(a, b)):
            out += leaf_diff(x, y, f"{prefix}{i}.")
        return out
    return [] if same(a, b) else [prefix.rstrip(".")]


def world_diff(before, after, allow):
    """What changed between two snapshots that `allow` doesn't permit. Each rule: {path (a glob),
    ops: [added, removed, changed] (default all three), fields: [globs over dotted field paths]
    (default all)}. A path whose entity was deleted and re-created shows as a changed `id`."""
    def ok(op, path, field=None):
        for r in allow:
            if op not in r.get("ops", ["added", "removed", "changed"]) or not fnmatch.fnmatchcase(path, r["path"]):
                continue
            if field is None or any(fnmatch.fnmatchcase(field, f) for f in r.get("fields", ["*"])):
                return True
        return False

    problems = []
    for path in sorted(set(before) | set(after)):
        if path not in before:
            if not ok("added", path):
                problems.append(f"{path}: added")
        elif path not in after:
            if not ok("removed", path):
                problems.append(f"{path}: removed")
        else:
            for field in leaf_diff(before[path], after[path]):
                if not ok("changed", path, field):
                    problems.append(f"{path}: {field} changed")
    return problems


def series_check(ctx, client, spec, vars_):
    """`series: {setup: [calls], ticks, call, value, expect: {max, min, last}}`: run the setup calls,
    then step one tick at a time, reading `value` from `call` after each; judge the largest, the
    smallest and the last reading (a jump's peak, not just one sample of it)."""
    for st in spec.get("setup", []):
        err, ans = client.call(st["call"], step_args(st, vars_))
        if err:
            return f"{st['call']} failed: {cut(ans, 300)}"
    values = []
    for _ in range(int(spec["ticks"])):
        err, ans = client.call("time_control", {"action": "step", "ticks": 1})
        if err:
            return f"time_control failed: {cut(ans, 300)}"
        err, ans = client.call(spec["call"]["call"], step_args(spec["call"], vars_))
        v = lookup(ans, parse_path(spec["value"])) if not err else _MISSING
        if not is_num(v):
            return f"tick {len(values) + 1}: {spec['value']} isn't a number in {cut(ans, 200)}"
        values.append(v)
    summary = {"max": max(values), "min": min(values), "last": values[-1], "argmax": values.index(max(values)) + 1}
    fails = check_all(spec.get("expect"), summary)
    if fails:
        return "; ".join(f"{f['path']}: want {json.dumps(f['want'])}, got {json.dumps(f['got'])}" for f in fails) + f" (readings over {len(values)} ticks)"
    return None


def run_verify(ctx, task):
    """Each verifier's result: {name, passed, detail?}. Runs as the agent 'verifier', which never
    said hello (claims don't apply to reads)."""
    results = []
    vars_ = dict(ctx.vars)  # setup's variables; the solution's are its own
    for v in task["verify"]:
        name = v["name"]
        try:
            v = with_vars(v, vars_)
            if "playtest" in v:
                c = ctx.client(v.get("as", "verifier"), "verify", False)
                err, r = run_playtest(ctx, c, v["playtest"], vars_)
                ok = not err and isinstance(r, dict) and r.get("status") == "passed"
                detail = None if ok else cut({k: r.get(k) for k in ("status", "failures", "reason", "error", "message")} if isinstance(r, dict) else r, 1200)
                if ok and v.get("expect"):
                    fails = check_all(v["expect"], r)
                    ok, detail = not fails, (fails or None)
                results.append({"name": name, "passed": ok, **({"detail": detail} if detail else {})})
                continue
            if "transcript" in v:
                probs = transcript_check(ctx, v["transcript"])
                results.append({"name": name, "passed": probs is None, **({"detail": ascii(scrub(probs))[:1200]} if probs else {})})
                continue
            if "world_diff" in v:
                if ctx.snapshot is None:
                    raise StepFailed("no snapshot of the world after setup (world_diff)")
                after = world_snapshot(ctx.client("verifier", "verify", False))
                found = world_diff(ctx.snapshot, after, v["world_diff"].get("allow", []))
                probs = (f"{len(found)} changes the task doesn't allow: " + "; ".join(found[:12])) if found else None
                results.append({"name": name, "passed": probs is None, **({"detail": ascii(scrub(probs))[:1200]} if probs else {})})
                continue
            if "series" in v:
                probs = series_check(ctx, ctx.client(v.get("as", "verifier"), "verify", False), v["series"], vars_)
                results.append({"name": name, "passed": probs is None, **({"detail": ascii(scrub(probs))[:1200]} if probs else {})})
                continue
            if "geometry" in v:
                probs = geometry_check(ctx.client(v.get("as", "verifier"), "verify", False), v["geometry"])
                results.append({"name": name, "passed": probs is None, **({"detail": ascii(scrub(probs))[:1200]} if probs else {})})
                continue
            if "file" in v:
                p = ctx.workdir / resolve(v["file"], vars_)
                if not p.is_file():
                    results.append({"name": name, "passed": False, "detail": f"no file {v['file']} in the engine's folder"})
                    continue
                try:
                    ans = json.loads(p.read_text(encoding="utf-8"))
                except ValueError:
                    ans = p.read_text(encoding="utf-8", errors="replace")
                err = False
            else:
                c = ctx.client(v.get("as", "verifier"), "verify", False)
                err, ans = c.call(v["call"], step_args(v, vars_))
            probs = judge(v, err, ans)
            if v.get("save") and not err:
                vars_[v["save"]] = select(ans, v["select"]) if "select" in v else ans
            results.append({"name": name, "passed": probs is None, **({"detail": ascii(scrub(probs))[:1200]} if probs else {})})
        except (StepFailed, SpecError, RuntimeError, OSError, KeyError, TypeError) as e:
            results.append({"name": name, "passed": False, "detail": ascii(scrub(str(e)))[:1200]})
    return results


# --------------------------------------------------------------------------------------------
# A task run


def engine_exe(task, exe_dir):
    return Path(exe_dir) / (GAME_EXES[task.get("game")] + EXE_SUFFIX)


def prepare_workdir(task, run_dir):
    """The engine's working folder: a fresh one (default) or, for module builds (path crates must
    be under the engine's folder), the checkout itself with the task's files in target/evals-work."""
    run_dir.mkdir(parents=True, exist_ok=True)
    if task.get("engine_cwd") == "repo":
        # One folder per run and task: two runs from one checkout never share it.
        # (One folder level, as the fixtures' relative paths to the SDK expect.)
        work = ROOT / "target" / "evals-work" / f"{run_dir.parent.name}-{os.getpid()}-{run_dir.name}"
        if work.exists():
            shutil.rmtree(work, ignore_errors=True)
        work.mkdir(parents=True, exist_ok=True)
        cwd, files_root = ROOT, work
    else:
        cwd, files_root = run_dir / "engine", run_dir / "engine"
        cwd.mkdir(parents=True, exist_ok=True)
    for f in task.get("files", []):
        src, dst = ROOT / f["from"], files_root / f["to"]
        if src.is_dir():
            shutil.copytree(src, dst, dirs_exist_ok=True)
        else:
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dst)
    return cwd, files_root


def brief_text(task, files_root, port):
    """The brief an agent receives: the task's text plus where and with what."""
    rel = files_root.relative_to(ROOT) if files_root.is_relative_to(ROOT) else files_root
    tools = ", ".join(task["tools"])
    lines = [f"Specula agent eval: {task['title']} ({task['id']})", "", task["brief"].replace("{work}", str(rel).replace("\\", "/")), "",
             f"Engine: MCP at http://127.0.0.1:{port}/mcp (headless; the clock is paused: time_control steps it).",
             f"Tools you may use: {tools} (and agent_hello, engine_info).",
             f"Limits: {task.get('limits', {}).get('calls', MAX_CALLS_DEFAULT)} tool calls, {task.get('limits', {}).get('wall_s', MAX_WALL_DEFAULT)} s.",
             "Call eval_done when you have finished."]
    return "\n".join(lines)


def tool_names(url):
    m = Mcp(url, "eval-probe")
    try:
        return {t["name"] for t in m.request("tools/list", {})["tools"]}
    finally:
        m.close()


def run_task(task, mode, port, run_dir, exe_dir, log=print, cache=None, agent_port=None, timeout_s=None, on_brief=None, report_dir=None):
    """Runs one task in a fresh engine on `port`. mode: reference | broken | agent. The engine works
    in `run_dir` (captures, saves, logs); the transcript, journal and brief go to `report_dir`
    (default run_dir). The result: {id, mode, status: passed|failed|skipped|error, verifiers,
    score, solution stats, ...}."""
    report_dir = Path(report_dir or run_dir)
    report_dir.mkdir(parents=True, exist_ok=True)
    exe = engine_exe(task, exe_dir)
    result = {"id": task["id"], "set": task["set"], "area": task["area"], "mode": mode}
    if not exe.is_file():
        return {**result, "status": "error", "error": f"no engine binary {exe} (cargo build --release -p {'sepulcra-app' if task.get('game') else 'specula'})"}
    if run_dir.exists():
        shutil.rmtree(run_dir, ignore_errors=True)
    cwd, files_root = prepare_workdir(task, run_dir)
    args = ["--headless", "--paused", "--captures", str(run_dir / "captures"), "--saves", str(run_dir / "saves"),
            "--cache", str(cache or run_dir / "pipeline-cache"), *task.get("engine_args", [])]
    transcript = Transcript(report_dir / "transcript.jsonl")
    limits = task.get("limits", {})
    started = time.perf_counter()
    env = None
    if task.get("engine_cwd") == "repo":
        # Module builds get their own Cargo target folder: two module tasks running at once (same
        # crate name) would otherwise build into one target/debug/<name>.dll and load each other's
        # library (review round 3: a cheat ran the broken solution's build).
        env = dict(os.environ, CARGO_TARGET_DIR=str(run_dir / "cargo-target"))
    eng = Engine(exe, port, run_dir / "engine.log", *args, cwd=str(cwd), env=env)
    try:
        eng.__enter__()
    except RuntimeError as e:
        text = str(e)
        if "GPU init failed" in text or "Vulkan" in text or "vulkan" in text:
            return {**result, "status": "skipped", "reason": "no Vulkan: the engine can't start here"}
        return {**result, "status": "error", "error": ascii(text)[-800:]}
    proxy = None
    ctx = Ctx(url_for(port), files_root, transcript, task["tools"], time.perf_counter() + float(limits.get("wall_s", MAX_WALL_DEFAULT)),
              int(limits.get("calls", MAX_CALLS_DEFAULT)))
    # `${work}`: the task's folder, relative to the engine's working directory (logic_build paths).
    ctx.vars["work"] = os.path.relpath(files_root, cwd).replace("\\", "/")
    actors = []
    try:
        result["startup_s"] = round(eng.startup_s, 2)
        have = tool_names(url_for(port))
        missing = [n for n in task.get("needs", []) if n not in have]
        if missing:
            return {**result, "status": "skipped", "reason": f"needs tools this engine doesn't have: {', '.join(missing)}"}
        bad = [p for p in task["tools"] if not any(fnmatch.fnmatchcase(n, p) for n in have)]
        if bad:
            return {**result, "status": "error", "error": f"the task's tools name nothing this engine has: {', '.join(bad)}"}
        run_steps(ctx, task.get("setup", []), "setup", False, "setup")
        if any("world_diff" in v for v in task["verify"]):
            # The world as the agent finds it: world_diff verifiers compare the end state with it.
            ctx.snapshot = world_snapshot(ctx.client("snapshot", "setup", False))
        for a in task.get("actors", []):
            actor = Actor(ctx, a, log)
            actor.start()
            actors.append(actor)
        t_solution = time.perf_counter()
        solution_error = None
        if mode in ("reference", "broken"):
            try:
                run_steps(ctx, task[mode], "solution", True, "solver", vars_={**ctx.vars})
            except StepFailed as e:
                solution_error = ascii(scrub(str(e)))[:800]
        else:
            proxy = Proxy(agent_port, url_for(port), task["tools"], transcript, ctx, max_calls=int(limits.get("calls", MAX_CALLS_DEFAULT)))
            proxy.start()
            brief = brief_text(task, files_root, agent_port)
            (report_dir / "brief.txt").write_text(scrub(brief))
            if on_brief:
                on_brief(brief)
            limit = timeout_s or float(limits.get("wall_s", MAX_WALL_DEFAULT))
            done = proxy.done.wait(limit)
            result["agent_summary"] = proxy.summary
            if not done:
                solution_error = f"the agent didn't call eval_done within {limit:.0f} s"
            proxy.stop()
        result["solution_wall_s"] = round(time.perf_counter() - t_solution, 2)
        if mode == "agent" and result["solution_wall_s"] > float(limits.get("wall_s", MAX_WALL_DEFAULT)):
            # The brief's time limit is advisory for agents (--timeout is the hard stop); the report says so.
            result["over_wall_limit"] = True
        ctx.stop.set()
        for a in actors:
            a.join(5)
        result["actors"] = [{"name": a.spec["name"], "fired": a.fired, **({"error": a.error} if a.error else {})} for a in actors]
        if actors:
            result["actor_polls"] = transcript.polls
        verifiers = run_verify(ctx, task)
        result["verifiers"] = verifiers
        passed = sum(1 for v in verifiers if v["passed"])
        result["score"] = {"passed": passed, "of": len(verifiers), "fraction": round(passed / len(verifiers), 3)}
        if solution_error:
            result["solution_error"] = solution_error
        result["status"] = "passed" if passed == len(verifiers) else "failed"
        # The engine's journal of the run (who changed what), for transcripts.
        try:
            jc = ctx.client("verifier", "verify", False)
            err, j = jc.call("world_history", {"limit": 200, "response_format": "detailed"})
            (report_dir / "journal.json").write_text(scrub(json.dumps(j, indent=1)))
        except Exception as e:  # the journal is a transcript extra; its absence isn't a failure
            (report_dir / "journal.json").write_text(scrub(json.dumps({"error": ascii(scrub(str(e)))})))
    except (StepFailed, SpecError) as e:
        result["status"] = "error"
        result["error"] = ascii(scrub(str(e)))[:1200]
    except (RuntimeError, OSError) as e:
        result["status"] = "error"
        result["error"] = ascii(scrub(str(e)))[:1200]
    finally:
        ctx.stop.set()
        if proxy:
            proxy.stop()
        with contextlib.suppress(Exception):
            ctx.close()
        eng.__exit__(None, None, None)
        transcript.write()
    result["solution"] = transcript.stats("solution")
    result["wall_s"] = round(time.perf_counter() - started, 2)
    result["validation_errors"] = eng.validation_errors()
    caps = sorted((run_dir / "captures").rglob("*.png")) if (run_dir / "captures").exists() else []
    if caps:
        result["captures"] = [str(p).replace("\\", "/") for p in caps[:16]]
    if task.get("engine_cwd") == "repo":
        if mode == "agent" and task.get("files"):
            # What the agent wrote (module source) is part of its transcript: keep the task's text
            # files (not build output), at most 1 MB.
            kept, total = report_dir / "files", 0
            for p in sorted(files_root.rglob("*")):
                if p.is_file() and "target" not in p.relative_to(files_root).parts and p.suffix in (".rs", ".toml", ".json", ".md", ".txt"):
                    total += p.stat().st_size
                    if total > 1 << 20:
                        break
                    # Kept as .txt: a Cargo.toml or .rs in the repo must not look like a crate or code to tools.
                    dest = kept / (str(p.relative_to(files_root)) + ".txt")
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_text(scrub(p.read_text(encoding="utf-8", errors="replace")), encoding="utf-8")
        shutil.rmtree(files_root, ignore_errors=True)
        shutil.rmtree(run_dir / "cargo-target", ignore_errors=True)
    return result


# --------------------------------------------------------------------------------------------
# Agent mode: a logging MCP proxy in front of the task's engine


class Proxy:
    """Forwards MCP over HTTP to the engine, logs every tools/call, hides and refuses tools the
    task doesn't allow, and adds `eval_done`. Loopback only."""

    def __init__(self, port, upstream, allowed, transcript, ctx, max_calls=MAX_CALLS_DEFAULT):
        self.port, self.upstream, self.allowed, self.transcript, self.ctx = port, upstream, allowed, transcript, ctx
        # The brief's call limit: past it, calls are refused with call_limit (eval_done still works).
        self.max_calls, self.calls = max_calls, 0
        self.done, self.summary = threading.Event(), None
        self.names = {}  # session -> agent name (from agent_hello)
        proxy = self

        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *a):
                pass

            def reply(self, code, body, headers=()):
                self.send_response(code)
                for k, v in headers:
                    self.send_header(k, v)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def forward(self, method, body):
                headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
                sid = self.headers.get("Mcp-Session-Id")
                if sid:
                    headers["Mcp-Session-Id"] = sid
                req = urllib.request.Request(proxy.upstream, body, headers, method=method)
                try:
                    with urllib.request.urlopen(req, timeout=300) as r:
                        return r.status, r.read(), [(k, v) for k, v in r.headers.items() if k.lower() == "mcp-session-id"]
                except urllib.error.HTTPError as e:
                    return e.code, e.read(), []

            def do_DELETE(self):
                code, body, hdrs = self.forward("DELETE", None)
                self.reply(code, body, hdrs)

            def do_GET(self):
                self.reply(405, b"")

            def do_POST(self):
                body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
                msg, bad = screen(body, proxy.allowed)
                sid = self.headers.get("Mcp-Session-Id") or "-"
                agent = proxy.names.get(sid, sid[:8])
                if bad:
                    proxy.transcript.add(agent, "solution", "(refused request)", {"bytes": len(body)}, len(body), 0, 0, "bad_request", bad)
                    return self.reply(400, json.dumps({"jsonrpc": "2.0", "id": None, "error": bad}).encode())
                # Forward the message that was checked, never the raw bytes.
                body = json.dumps(msg).encode()
                if msg["method"] == "tools/call":
                    p = msg["params"]
                    tool, args = p["name"], p.get("arguments") or {}
                    if tool == "eval_done":
                        proxy.summary = str(args.get("summary", ""))[:1000]
                        proxy.transcript.add(agent, "solution", tool, args, len(body), 0, 0, None, None)
                        proxy.done.set()
                        out = {"jsonrpc": "2.0", "id": msg.get("id"), "result": {"content": [{"type": "text", "text": json.dumps({"done": True, "message": "Thanks: the engine verifies your work now."})}]}}
                        return self.reply(200, json.dumps(out).encode())
                    with proxy.ctx.lock:
                        proxy.calls += 1
                        over = proxy.calls > proxy.max_calls
                    if over:
                        ans = {"error": "call_limit", "message": f"this task allows {proxy.max_calls} tool calls; call eval_done"}
                        proxy.transcript.add(agent, "solution", tool, args, len(body), 0, 0, "call_limit", ans)
                        out = {"jsonrpc": "2.0", "id": msg.get("id"), "result": {"isError": True, "content": [{"type": "text", "text": json.dumps(ans)}]}}
                        return self.reply(200, json.dumps(out).encode())
                    ans = refusal(tool, args, proxy.allowed)
                    if ans:
                        proxy.transcript.add(agent, "solution", tool, args, len(body), 0, 0, ans["error"], ans)
                        out = {"jsonrpc": "2.0", "id": msg.get("id"), "result": {"isError": True, "content": [{"type": "text", "text": json.dumps(ans)}]}}
                        return self.reply(200, json.dumps(out).encode())
                    t0 = time.perf_counter()
                    code, rbody, hdrs = self.forward("POST", body)
                    ms = (time.perf_counter() - t0) * 1000
                    try:
                        reply = json.loads(rbody)
                    except ValueError:
                        reply = None
                    res = reply.get("result") if isinstance(reply, dict) else None
                    if code != 200 or not isinstance(res, dict) or (isinstance(reply, dict) and "error" in reply):
                        # It didn't run (or the answer can't be read): logged as an error, never a success.
                        rpc = reply.get("error") if isinstance(reply, dict) else None
                        ans = {"error": "not_run", "status": code, **({"rpc_error": rpc} if rpc is not None else {})}
                        err = True
                    else:
                        ans = answer_of(res)
                        err = bool(res.get("isError"))
                    if tool == "agent_hello" and not err and isinstance(args.get("name"), str):
                        proxy.names[sid] = args["name"]
                        agent = args["name"]
                    proxy.transcript.add(agent, "solution", tool, args, len(body), len(rbody), ms, error_code(ans) if err else None, ans)
                    return self.reply(code, rbody, hdrs)
                code, rbody, hdrs = self.forward("POST", body)
                if msg["method"] == "tools/list" and code == 200:
                    try:
                        out = json.loads(rbody)
                        tools = [t for t in out["result"]["tools"] if tool_allowed(t["name"], proxy.allowed)]
                        out["result"]["tools"] = tools + [DONE_TOOL]
                        rbody = json.dumps(out).encode()
                    except (ValueError, KeyError, TypeError):
                        pass
                self.reply(code, rbody, hdrs)

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self):
        self.thread.start()

    def stop(self):
        if self.thread.is_alive():
            self.server.shutdown()
            self.server.server_close()


# --------------------------------------------------------------------------------------------
# The `call` command: one MCP call per run, for agents without an MCP client


def session_file(port, name):
    d = Path(os.environ.get("TEMP") or os.environ.get("TMPDIR") or "/tmp") / "specula-evals-sessions"
    d.mkdir(parents=True, exist_ok=True)
    return d / f"{port}-{re.sub(r'[^A-Za-z0-9_-]', '_', name)}.txt"


def cli_call(port, name, tool, args_text):
    url = url_for(port)
    sf = session_file(port, name)
    mcp = None
    if sf.exists():
        mcp = Mcp.__new__(Mcp)
        mcp.url, mcp.session, mcp.next_id, mcp.detailed = url, sf.read_text().strip() or None, 1, False
    try:
        if mcp is None or not mcp.session:
            raise RuntimeError("no session")
        result = mcp.request("tools/call", {"name": tool, "arguments": args_text})
    except (RuntimeError, urllib.error.HTTPError, OSError):
        mcp = Mcp(url, f"evals-call-{name}", detailed=False)
        sf.write_text(mcp.session or "")
        result = mcp.request("tools/call", {"name": tool, "arguments": args_text})
    for c in result.get("content", []):
        if c.get("type") == "text":
            print(ascii(c["text"]))
        elif c.get("type") == "image":
            print(f"(image, {len(c.get('data', ''))} base64 bytes)")
    return 1 if result.get("isError") else 0


# --------------------------------------------------------------------------------------------
# Many tasks: one after another, or in parallel through the playtest swarm's lead and leases


def broken_variants(tasks):
    """--broken runs each task's broken solution and every cheat (another broken solution someone
    found passing once), each as its own case `<id>~<cheat>`; all must fail."""
    out = {}
    for tid, t in tasks.items():
        out[tid] = t
        for c in t.get("cheats", []):
            out[f"{tid}~{c['id']}"] = {**t, "id": f"{tid}~{c['id']}", "broken": c["steps"]}
    return out


def stamp():
    return time.strftime("%Y%m%d-%H%M%S")


def run_suite(tasks, mode, out, exe_dir, workers, lead_port, pool, log=print, work=None):
    """Runs every task; engines work under `work` (default <target>/evals-runs/<out's name>), reports
    go to `out`."""
    from playtest_swarm import Swarm

    work = Path(work or TARGET / "evals-runs" / out.name)
    work.mkdir(parents=True, exist_ok=True)
    results = {}
    if workers <= 1:
        port = pool[0]
        for tid, t in tasks.items():
            log(f"  {tid}: running ({mode}) on port {port}")
            r = run_task(t, mode, port, work / tid, exe_dir, log, cache=work / "pipeline-cache", report_dir=out / tid)
            results[tid] = r
            log(f"  {tid}: {describe(r, mode)}")
        return results, {"workers": 1}

    class EvalSwarm(Swarm):
        task_tag, task_word = "eval", "eval"

        def run_one(self, worker, name, spec, ports, scratch):
            log(f"  {name}: running ({mode}) on port {ports[0]} (worker {worker})")
            try:
                r = run_task(spec, mode, ports[0], work / name, exe_dir, log, cache=work / f"pipeline-cache-{worker}", report_dir=out / name)
            except Exception as e:  # the job is reported as an error, the swarm goes on
                r = {"id": name, "set": spec.get("set"), "area": spec.get("area"), "mode": mode, "status": "error", "error": ascii(scrub(str(e)))[:800]}
            r["worker"], r["name"] = worker, name
            r["passed"] = (r["status"] == "passed") if mode != "broken" else (r["status"] in ("failed",))
            log(f"  {name}: {describe(r, mode)}")
            return r

        def summary(self, res):
            return describe(res, mode)

    perf = work / "lead" / "perf.toml"
    perf.parent.mkdir(parents=True, exist_ok=True)
    perf.write_text(f"[swarm]\nlease_ports = [{pool[0]}, {pool[-1]}]\nscratch_root = \"{(work / 'lead' / 'scratch').as_posix()}\"\n")
    swarm = EvalSwarm(Path(exe_dir) / ("specula" + EXE_SUFFIX), work, workers=workers, lead_port=lead_port,
                      engine_args=["--perf", str(perf)], verify=False, log=log, ports_per_worker=1)
    report = swarm.run(tasks)
    for r in report["results"]:
        r.setdefault("id", r.get("name"))
        r.setdefault("status", "error")
        results[r["id"]] = r
    return results, {"workers": swarm.workers, "board": report.get("board"), "swarm_wall_s": report.get("wall_s")}


def describe(r, mode):
    s = r.get("status")
    if s in ("passed", "failed"):
        sc = r["score"]
        bad = [v["name"] for v in r.get("verifiers", []) if not v["passed"]]
        tail = f"; failed: {', '.join(bad)}" if bad else ""
        sol = r.get("solution", {})
        return f"{s.upper()} {sc['passed']}/{sc['of']} verifiers, {sol.get('calls')} calls, {sol.get('resp_bytes')} bytes back, {r.get('wall_s')} s{tail}"
    return f"{str(s).upper()}: {r.get('reason') or r.get('error')}"


def summarise(results, mode, extra):
    rs = list(results.values())
    ran = [r for r in rs if r.get("status") in ("passed", "failed")]
    skipped = [r for r in rs if r.get("status") == "skipped"]
    errors = [r for r in rs if r.get("status") not in ("passed", "failed", "skipped")]
    def rid(r):
        return r.get("id") or r.get("name") or "?"

    if mode == "broken":
        # A broken solution must fail at least one verifier.
        good = [r for r in ran if r["status"] == "failed"]
        bad = [rid(r) for r in ran if r["status"] == "passed"]
    else:
        good = [r for r in ran if r["status"] == "passed"]
        bad = [rid(r) for r in ran if r["status"] != "passed"]
    return {"mode": mode, "tasks": len(rs), "ran": len(ran), "as_expected": len(good), "unexpected": bad,
            "skipped": [{"id": rid(r), "reason": r.get("reason")} for r in skipped],
            "errors": [{"id": rid(r), "error": r.get("error")} for r in errors],
            "calls": sum(r.get("solution", {}).get("calls", 0) for r in ran),
            "resp_bytes": sum(r.get("solution", {}).get("resp_bytes", 0) for r in ran),
            "tokens_est": sum(r.get("solution", {}).get("tokens_est", 0) for r in ran), **extra}


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "tools":
        ap = argparse.ArgumentParser(prog="evals.py tools", description="List the tools a port offers (tools/list), or one tool's full schema.")
        ap.add_argument("tools")
        ap.add_argument("--port", type=int, required=True)
        ap.add_argument("--name", default=None, help="print this tool's description and input schema")
        a = ap.parse_args()
        mcp = Mcp(url_for(a.port), "evals-tools", detailed=False)
        try:
            listed = mcp.request("tools/list", {})["tools"]
        finally:
            mcp.close()
        for t in listed:
            if a.name is None:
                print(ascii(f"{t['name']}: {t.get('description', '')[:160]}"))
            elif t["name"] == a.name:
                print(ascii(json.dumps(t, indent=1)))
                return
        if a.name is not None:
            raise SystemExit(f"no tool {a.name} on port {a.port}")
        return
    if len(sys.argv) > 1 and sys.argv[1] == "call":
        ap = argparse.ArgumentParser(prog="evals.py call", description="One MCP tool call (prints the answer's text).")
        ap.add_argument("call")
        ap.add_argument("--port", type=int, required=True)
        ap.add_argument("--as", dest="name", default="agent", help="your agent name (keeps your session between calls)")
        ap.add_argument("tool")
        ap.add_argument("args", nargs="?", default="{}", help="the arguments as JSON, @file.json, or - for stdin (in PowerShell use @file or -)")
        a = ap.parse_args()
        text = a.args
        if text == "-":
            # From stdin: PowerShell strips the double quotes inside a single-quoted argument.
            text = sys.stdin.read()
        elif text.startswith("@"):
            text = Path(text[1:]).read_text(encoding="utf-8")
        sys.exit(cli_call(a.port, a.name, a.tool, json.loads(text)))

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="validate the task files and exit")
    mode.add_argument("--list", action="store_true")
    mode.add_argument("--reference", action="store_true", help="run every task's reference solution: all must pass")
    mode.add_argument("--broken", action="store_true", help="run every task's broken solution: all must fail")
    mode.add_argument("--agent", action="store_true", help="run one task for an external agent")
    ap.add_argument("--set", default="all", choices=("public", "heldout", "all"))
    ap.add_argument("--task", default="", help="comma-separated task ids")
    ap.add_argument("--tasks-dir", default=str(EVALS_DIR), help="a folder with public/ and heldout/ (e.g. a private held-out set)")
    ap.add_argument("--exe-dir", default=str(TARGET / "release"), help="folder with specula and sepulcra binaries")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--lead-port", type=int, default=7492)
    ap.add_argument("--pool", default="7493-7495", help="ports the workers' engines lease (first-last)")
    ap.add_argument("--port", type=int, default=7493, help="--agent: the port the agent connects to (the proxy)")
    ap.add_argument("--engine-port", type=int, default=7492, help="--agent: the engine's own port")
    ap.add_argument("--timeout", type=float, default=None, help="--agent: seconds to wait for eval_done")
    ap.add_argument("--out", default=None)
    a = ap.parse_args()

    sets = SETS if a.set == "all" else (a.set,)
    ids = [x for x in a.task.split(",") if x]
    tasks = load_tasks(sets, ids or None, Path(a.tasks_dir))
    if a.check:
        print(f"{len(tasks)} task files valid")
        return
    if a.list:
        for t in tasks.values():
            needs = f" (needs {', '.join(t['needs'])})" if t.get("needs") else ""
            print(f"  {t['set']:8} {t['area']:10} {t['id']:24} {ascii(t['title'])}{needs}")
        return
    m = "reference" if a.reference else "broken" if a.broken else "agent"
    if m == "broken":
        tasks = broken_variants(tasks)
    out = Path(a.out) if a.out else ROOT / "perf" / "evals" / f"{stamp()}-{m}"
    out.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    if m == "agent":
        if len(tasks) != 1:
            raise SystemExit("--agent runs one task: pass --task <id>")
        t = next(iter(tasks.values()))
        print(f"starting {t['id']}: engine on {a.engine_port}, agent port {a.port}", flush=True)
        r = run_task(t, "agent", a.engine_port, TARGET / "evals-runs" / out.name / t["id"], a.exe_dir, agent_port=a.port, timeout_s=a.timeout,
                     report_dir=out / t["id"],
                     on_brief=lambda b: print("\n----- BRIEF -----\n" + ascii(b) + "\n-----------------\n", flush=True))
        results, extra = {t["id"]: r}, {}
        print(f"  {t['id']}: {describe(r, m)}")
    else:
        lo, hi = (int(x) for x in a.pool.split("-"))
        pool = list(range(lo, hi + 1))
        print(f"{len(tasks)} tasks ({m}), {a.workers} worker(s), results in {out}", flush=True)
        results, extra = run_suite(tasks, m, out, a.exe_dir, max(1, a.workers), a.lead_port, pool)
    summary = summarise(results, m, {**extra, "wall_s": round(time.perf_counter() - started, 2)})
    report = {"format": "specula-evals", "version": EVAL_FORMAT, "summary": summary,
              "results": [results[i] for i in tasks if i in results]}
    (out / "report.json").write_text(scrub(json.dumps(report, indent=1)))
    print(f"{summary['as_expected']}/{summary['ran']} as expected ({m}); skipped {len(summary['skipped'])}, errors {len(summary['errors'])}; "
          f"{summary['calls']} calls; report {out / 'report.json'}")
    for e in summary["errors"]:
        print(f"  ERROR {e['id']}: {ascii(e['error'])[:300]}")
    ok = not summary["unexpected"] and not summary["errors"]
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

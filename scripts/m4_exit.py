#!/usr/bin/env python3
"""M4 exit proof: "A swarm builds a themed asset set (30+ props) that passes style.toml; it is
presented as one contact sheet in the review queue; the approved assets go straight into the game
world" (ROADMAP, M4). A debugger agent watches the whole run (docs/design/DEBUGGER.md).

Phase 1 (default): Sepulcra runs windowed with a fresh copy of its assets that lacks the 33 map-1
graveyard props (their recipes are the swarm's to write). Over MCP, concurrently:
  - director (operator-granted lead): task_plan turns the prop list into board tasks assigned by
    role, caps each builder's tools (tools_scope), tracks with task_progress, makes the one
    contact sheet (studio_sheet of every prop) and the one review submission (review_submit with
    promote), and puts it on the review screen (review_screen show);
  - 4 builders (?role=builder plus project, studio, review): take tasks, claim_take each recipe's
    project path, project_write the recipe, style_check and studio_check it until it passes, stage
    a model entity of it at its map position under /staging (out of the game's view), finish the
    task. Builders fix what their own checks and the art director's critiques find;
  - art director (operator-granted reviewer): gallery rounds for two look decisions, then a review
    of every prop with studio_sheet (worst light, phone) and studio_lineup (silhouette
    distinctness among siblings) against its own bar; weak props get critique_add + to_task;
  - sight-skeptic: checks the staged props in a sight_frame (each drawn, visible pixels) and the
    contact sheet's packet (every prop, every tile) before submission;
  - debugger (operator-granted reviewer): trace_incidents / trace_follow polls, triage with
    trace_incident + trace_call, files incidents as critiques with evidence, re-verifies.
The recipes' content was authored by Claude (games/sepulcra/assets/recipes, and first drafts in
scripts/m4_exit_drafts that the checks sent back); the script runs the swarm mechanics so claims,
the board, scoped tools, concurrency, critiques and the debugger are exercised for real.

Then it STOPS: the owner approves at the window. No agent approves, no forged window:owner, no
--owner grant. With --no-wait the script saves the world and exits once the submission is on the
screen; `--review` reopens the window on the saved world later and waits for the owner.

    cargo build -p sepulcra-app --release
    python scripts/m4_exit.py [--port 7871] [--adapter integrated] [--out DIR] [--no-wait]
    python scripts/m4_exit.py --review [--out DIR] [--exe PATH]   # the owner's window: approve or reject there
    python scripts/m4_exit.py --after-approval   # (phase 2) verify promotion into the game world

Output (summary.json, captures, packets, engine log, the world) goes to target/m4-exit/ unless
--out. Standard library only; output is ASCII.
"""
import argparse
import json
import os
import shutil
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, url_for  # noqa: E402

SEPULCRA_ASSETS = ROOT / "games" / "sepulcra" / "assets"
DRAFTS = Path(__file__).resolve().parent / "m4_exit_drafts"
EXE = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
WORLD = "m4_exit"
BATCH = "m4_graveyard"
STAGING = "/staging/m4_graveyard"
# The staging lot: 200 m east of the slab, out of the game camera's view. Each prop's local
# position under it is its place on map 1, so promotion (a re-parent under PROPS_ROOT, at the
# origin) puts it straight onto the slab.
STAGING_AT = [200.0, 0.0, 0.0]
PROPS_ROOT = "/props/graveyard"

# The 33 props: name, where on map 1 (x, z in metres; the slab's top face spans -20..20 both ways,
# 2 m cells), yaw in degrees. Dressing sits on the rim fragments, away from the lanes and the
# centre build spots: the old north cemetery, the gravedigger's west corner, the east memorial
# garden and the south graves.
PROPS = [
    # The old north cemetery (rows 0-2).
    ("mausoleum_domed", -0.5, -17.0, 0), ("statue_angel", -3.6, -15.0, 15), ("statue_mourner", 3.0, -15.0, -20),
    ("grave_obelisk", -4.4, -18.6, 0), ("lantern_dead", 4.0, -18.6, 0), ("chest_tomb", -7.4, -16.4, 90),
    ("column_broken", 7.2, -16.4, 30), ("gate_iron", -7.4, -19.2, 0), ("crypt_door", 8.2, -19.3, 0),
    ("pediment_fallen", 6.6, -14.8, -10),
    # The gravedigger's west corner (cols 0-5).
    ("candle_shrine", -15.0, -9.0, 60), ("grave_bell", -12.0, -5.0, 90), ("grave_mound", -12.0, -9.0, 0),
    ("gravedigger_tools", -9.4, -10.2, 20), ("wheelbarrow", -15.0, -5.6, 35), ("coffin_open", -17.4, -2.6, 0),
    ("coffin_closed", -14.6, -1.4, 80), ("lantern_post", -10.0, -2.6, 0), ("rune_obelisk", -18.0, -10.2, 20),
    # The east memorial garden (cols 14-19).
    ("brazier_tripod", 10.6, -8.0, 0), ("stone_bench", 16.0, -8.0, 180), ("urn_funeral", 12.6, -4.0, 0),
    ("urn_toppled", 17.0, -4.4, 40), ("candle_cluster", 14.6, -1.0, -15), ("wreath_stand", 11.0, 0.6, -30),
    ("headstone_skull", 18.0, 0.2, -10), ("grave_cross_iron", 16.0, 2.6, 5),
    # The south graves (rows 14-16).
    ("headstone_broken", -6.0, 10.0, 10), ("grave_marker_wood", -4.0, 12.4, -5), ("bone_pile", -16.0, 10.0, 30),
    ("ribcage_buried", -14.0, 12.6, 90), ("fence_iron_corner", 2.0, 10.0, 0), ("fence_iron_broken", 6.6, 10.2, 0),
]
NAMES = [p[0] for p in PROPS]
# Sibling groups for the builders' studio_check (what each prop must not look like) and the art
# director's line-ups (at most 12 a line-up).
GROUPS = {
    "monuments": ["mausoleum_domed", "statue_angel", "statue_mourner", "lantern_dead", "grave_obelisk", "column_broken",
                  "rune_obelisk", "crypt_door", "candle_shrine", "lantern_post", "gate_iron"],
    "grave_markers": ["grave_cross_iron", "grave_marker_wood", "headstone_broken", "headstone_skull", "urn_funeral",
                      "wreath_stand", "candle_cluster", "fence_iron_corner", "fence_iron_broken", "grave_bell", "brazier_tripod"],
    "low_pieces": ["chest_tomb", "coffin_closed", "coffin_open", "grave_mound", "bone_pile", "ribcage_buried", "urn_toppled",
                   "gravedigger_tools", "wheelbarrow", "pediment_fallen", "stone_bench"],
}
GROUP_OF = {n: g for g, ns in GROUPS.items() for n in ns}
# The art director's own bar, stricter than style.toml (taste.silhouette_max_iou 0.9,
# value_contrast_worst_min 0.03, min_px_phone 8): what a reviewer flags for a builder to improve.
# A line-up pair over the bar is filed on the pair's first prop in its group's order (the newer
# idea; the group lists the established shapes later).
ART_BAR = {"silhouette_iou": 0.75, "worst_contrast": 0.035, "phone_px": 10}
# Look decisions taken as gallery rounds (docs/design/GALLERIES.md). The pick is the art
# director's (an agent's), recorded here and in the summary with its reason; the owner's pick at
# the window overrides it. Galleries stay undecided until the owner decides them.
GALLERIES = [
    {"id": "m4_brazier_tripod", "title": "How should the graveyard's tripod brazier burn?", "recipe": "brazier_tripod",
     "param": "lit", "min": 0, "max": 2, "step": 1, "values": [0, 1, 2],
     "pick": 1, "reason": "embers: a warm glow at night without a second bowl of flames on the slab; lit flames (2) read as the "
                          "player's brazier cairn from the default camera (the bowl of fire is that stone's silhouette), cold ash (0) "
                          "loses the only warmth in the east garden"},
    {"id": "m4_rune_obelisk", "title": "What should the ancient rune obelisk's runes do?", "recipe": "rune_obelisk",
     "param": "runes", "min": 0, "max": 2, "step": 1, "values": [0, 1, 2],
     "pick": 0, "reason": "dormant: carved runes with lichen in the grooves. Rune cyan (2) would read as a player's stone; grave green "
                          "(1) says 'enemy' on something that never attacks. Colour stays meaning (STYLE.md)"},
    {"id": "m4_statue_angel", "title": "How high should the grave angel's wings be?", "recipe": "statue_angel",
     "param": "wings", "min": 0, "max": 1, "step": None, "values": [0, 0.25, 0.5, 0.75, 1],
     "pick": 0.85, "reason": "raised but not spread flat: the wings make the silhouette tell 'angel' at 57 px from the default "
                             "camera (folded wings read as a robed figure like the mourner's)"},
]
TOKENS = None  # set in main: out / "tokens"
FAILS = []
LOG_LOCK = threading.Lock()


def log(msg):
    with LOG_LOCK:
        print(msg, flush=True)


def check(cond, what):
    log(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


class Agent:
    """One agent's MCP session with per-agent statistics (calls, refusals, conflicts)."""

    def __init__(self, port, name, role, query, token_dir):
        self.name, self.role = name, role
        self.mcp = Mcp(url_for(port) + query, f"m4exit-{name}", detailed=False, tools=None)
        self.stats = {"calls": 0, "refusals": 0, "conflicts": 0, "busy_retries": 0, "codes": {}}
        self.hello = self.mcp.hello(name, role=role, token_file=str(token_dir / f"{name}.token"))

    def raw(self, tool, **args):
        """(answer json, error json or None); never raises on a tool error."""
        self.stats["calls"] += 1
        result = self.mcp.request("tools/call", {"name": tool, "arguments": args})
        text = next((c["text"] for c in result["content"] if c["type"] == "text"), "{}")
        try:
            body = json.loads(text)
        except ValueError:
            body = {"text": text}
        if result.get("isError"):
            code = body.get("error", "error") if isinstance(body, dict) else "error"
            self.stats["refusals"] += 1
            self.stats["codes"][code] = self.stats["codes"].get(code, 0) + 1
            if code in ("claimed", "stale_version", "version_conflict", "name_taken"):
                self.stats["conflicts"] += 1
            return None, body
        return body, None

    def call(self, tool, retries=40, **args):
        """A call that must succeed; busy studio or engine answers are waited out and counted."""
        for _ in range(retries):
            body, err = self.raw(tool, **args)
            if err is None:
                return body
            if err.get("error") in ("studio_busy", "engine_busy") or err.get("retry"):
                self.stats["busy_retries"] += 1
                time.sleep(min(float(err.get("retry_after_s", 1.0) or 1.0), 5.0))
                continue
            raise RuntimeError(f"{self.name}: {tool}: {json.dumps(err)[:600]}")
        raise RuntimeError(f"{self.name}: {tool}: still busy after {retries} tries")

    def close(self):
        self.mcp.close()


def recipe_text(name, draft):
    """The recipe a builder writes: its first draft (scripts/m4_exit_drafts/<name>.v1.json, the
    version the checks or the art director sent back) or the final one in the repo."""
    d = DRAFTS / f"{name}.v1.json"
    if draft and d.exists():
        return d.read_text(encoding="utf-8")
    return (SEPULCRA_ASSETS / "recipes" / f"{name}.json").read_text(encoding="utf-8")


def prepare_assets(out):
    """A fresh copy of Sepulcra's assets without the 33 props (and without taste history)."""
    dst = out / "assets"
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(SEPULCRA_ASSETS, dst, ignore=shutil.ignore_patterns("taste"))
    for n in NAMES:
        (dst / "recipes" / f"{n}.json").unlink(missing_ok=True)
    return dst


def engine_args(a, out, assets):
    # Authority is granted by the operator, never self-declared (docs/design/AUTHORITY.md): the
    # director leads, the art director and the debugger review. Nobody is granted owner.
    return [*(["--headless"] if a.headless else []), "--adapter", a.adapter, "--size", a.size, "--assets", str(assets), "--captures", str(out / "captures"),
            "--worlds", str(out / "worlds"), "--world", WORLD, "--saves", str(out / "saves"), "--trace", str(out / "trace"),
            "--lead", "director", "--reviewer", "artdir", "--reviewer", "debugger",
            "--", "--seed", str(a.seed), "--mute"]


# ---------------------------------------------------------------- the debugger

class Debugger(threading.Thread):
    """DEBUGGER.md's loop: watch, triage (ack, narrow with trace_call), report as a critique with
    the evidence and the fix owner, link, and re-verify at the end. Never fixes others' work."""

    def __init__(self, agent, stop):
        super().__init__(daemon=True)
        self.a, self.stop_event = agent, stop
        self.handled = {}
        self.warn_events = 0
        self.event_codes = {}
        self.errors = []

    def run(self):
        while not self.stop_event.is_set():
            try:
                self.poll()
            except Exception as e:  # noqa: BLE001 - the debugger reports, never takes the run down
                self.errors.append(str(e)[:300])
            self.stop_event.wait(3.0)

    def poll(self):
        # The engine keeps this session's cursor: each poll returns only new events.
        f, err = self.a.raw("trace_follow", level="warn", limit=200)
        if f:
            evs = f.get("events", [])
            self.warn_events += len(evs)
            for e in evs:
                code = e.get("code", "?")
                self.event_codes[code] = self.event_codes.get(code, 0) + 1
        inc, err = self.a.raw("trace_incidents")
        for i in (inc or {}).get("incidents", []):
            if i["id"] not in self.handled:
                self.triage(i)

    def triage(self, incident):
        start = self.a.stats["calls"]
        iid = incident["id"]
        self.a.raw("trace_incident", id=iid, action="ack", note="debugger: looking")
        full, _ = self.a.raw("trace_incident", id=iid, response_format="detailed")
        full = full or {}
        calls = full.get("calls") or []
        tree = None
        if calls:
            tree, _ = self.a.raw("trace_call", call=calls[-1])
        evidence = json.dumps(full.get("evidence") or {})[:700]
        agents = full.get("agents") or incident.get("agents") or []
        owner = agents[-1] if agents else None
        tool = ((tree or {}).get("call") or {}).get("tool")
        item = {"text": f"[incident {iid} {incident.get('code')}] seen {incident.get('count', 1)}x; last call {calls[-1] if calls else 'none'}"
                        f" ({tool or 'unknown tool'}). Evidence: {evidence}. Fix owner: {owner or 'unknown (no agent on the incident)'}.",
                "severity": "error" if incident.get("level") == "error" else "warning", "source": "debugger"}
        added, err = self.a.raw("critique_add", items=[item])
        critique = (added or {}).get("added", [None])[0]
        if critique is not None:
            self.a.raw("trace_incident", id=iid, action="link", critique=critique, note="filed as a critique")
        self.handled[iid] = {"code": incident.get("code"), "level": incident.get("level"), "count": incident.get("count"),
                             "agents": agents, "calls": calls[-3:], "tool": tool, "critique": critique,
                             "calls_used": self.a.stats["calls"] - start}
        log(f"debugger: incident {iid} {incident.get('code')} (x{incident.get('count')}, agents {agents}) -> critique {critique}")

    def reverify(self):
        """At the end: an incident still active (seen again since triage) stays open with its
        evidence; one that went quiet is resolved, citing the count that held."""
        out = {}
        for iid, h in self.handled.items():
            now, _ = self.a.raw("trace_incident", id=iid)
            now = now or {}
            same = now.get("count") == h.get("count")
            if same and now.get("status") not in ("resolved",):
                self.a.raw("trace_incident", id=iid, action="resolve", note=f"debugger: no recurrence since triage (count held at {h.get('count')})")
                if h.get("critique") is not None:
                    self.a.raw("critique_update", id=h["critique"], action="resolve", note="re-verified: no recurrence")
            out[iid] = {"count_then": h.get("count"), "count_now": now.get("count"), "resolved": same}
        return out


# ---------------------------------------------------------------- builders

class Builder(threading.Thread):
    def __init__(self, agent, director_name, positions, report):
        super().__init__(daemon=True)
        self.a, self.director_name, self.positions, self.report = agent, director_name, positions, report
        self.done, self.error, self.failed = [], None, []
        self.rounds = {}

    def run(self):
        try:
            idle = 0
            while idle < 20:
                # Builder tasks (tagged role:builder) and critique tasks the art director assigned.
                ready = [t for t in self.a.call("task_list", ready_for_me=True, limit=10).get("tasks", []) if t.get("assignee") == self.a.name]
                if not ready:
                    idle += 1
                    time.sleep(1.0)
                    continue
                idle = 0
                task = ready[0]
                taken, err = self.a.raw("task_take", id=task["id"])
                if err:
                    continue
                try:
                    # A critique's task (critique_update to_task) is titled "Critique: <text>".
                    if "critique" in task.get("tags", []) or task.get("title", "").startswith("Critique:"):
                        self.fix(task)
                    else:
                        self.build(task)
                except Exception as e:  # noqa: BLE001 - a failed task is reported, the builder goes on
                    self.failed.append({"task": task["id"], "title": task.get("title"), "error": str(e)[:600]})
                    log(f"{self.a.name}: task {task['id']} failed: {str(e)[:300]}")
                    self.a.raw("claim_release", all=True)
        except Exception as e:  # noqa: BLE001
            self.error = str(e)
            log(f"{self.a.name}: stopped: {e}")

    def prop_of(self, task):
        """The prop a task is about: the name in its title ("Build grave_mound (...)",
        "Critique: grave_mound: ...")."""
        words = task.get("title", "").replace(":", " ").split()
        return next((w for w in words if w in NAMES), None)

    def write(self, name, text):
        """A new recipe is written as is; a revision names the version it read (expect_version),
        so another agent's change in between is a stale_version refusal, never overwritten."""
        path = f"assets/recipes/{name}.json"
        args = {"path": path, "text": text}
        if name in self.report["written"]:
            cur = self.a.call("project_read", path=path, problems=False)
            args["expect_version"] = cur["version"]
        return self.a.call("project_write", **args)

    def checks(self, name):
        """style_check, then studio_check against the prop's sibling group: the builder's gate."""
        for _ in range(50):
            style, err = self.a.raw("style_check", subjects=[{"recipe": f"recipes/{name}.json"}])
            if err is None:
                break
            if err.get("error") != "recipe_not_found":
                raise RuntimeError(f"{self.a.name}: style_check: {json.dumps(err)[:600]}")
            # Rough edge (M4 exit run): project_write answers before the studio's tools can read
            # the new file from disk. Counted, and waited out.
            self.a.stats["write_then_check_races"] = self.a.stats.get("write_then_check_races", 0) + 1
            time.sleep(0.2)
        else:
            raise RuntimeError(f"{self.a.name}: style_check never found recipes/{name}.json after project_write")
        sib = [{"recipe": f"recipes/{s}.json"} for s in GROUPS[GROUP_OF[name]] if s != name and s in self.report["written"]][:11]
        for _ in range(50):
            taste, err = self.a.raw("studio_check", subject={"recipe": f"recipes/{name}.json"}, siblings=sib, image=False)
            if err is None:
                break
            if err.get("error") == "recipe_not_found":
                self.a.stats["write_then_check_races"] = self.a.stats.get("write_then_check_races", 0) + 1
                time.sleep(0.2)
            elif err.get("error") in ("studio_busy", "engine_busy"):
                self.a.stats["busy_retries"] += 1
                time.sleep(min(float(err.get("retry_after_s", 1.0) or 1.0), 5.0))
            else:
                raise RuntimeError(f"{self.a.name}: studio_check: {json.dumps(err)[:600]}")
        else:
            raise RuntimeError(f"{self.a.name}: studio_check of {name} kept failing")
        return style, taste

    def build(self, task):
        name = self.prop_of(task)
        claim = self.a.call("claim_take", path=f"/project/assets/recipes/{name}.json", lease_s=600)
        self.report["holding"][self.a.name] = name
        self.report["written_by"][name] = self.a.name
        rounds = []
        draft = True
        for attempt in range(3):
            self.write(name, recipe_text(name, draft))
            with LOG_LOCK:
                self.report["written"].add(name)
            style, taste = self.checks(name)
            ok = style.get("passed") and taste.get("passed")
            rounds.append({"draft": draft and (DRAFTS / f"{name}.v1.json").exists(), "style_passed": style.get("passed"),
                           "taste_passed": taste.get("passed"), "errors": [e.get("check") for e in taste.get("errors", [])] + [
                               e.get("check") for r in style.get("results", []) for e in r.get("errors", [])],
                           "capture": (taste.get("image") or {}).get("path")})
            if ok:
                break
            if not draft:
                break
            log(f"{self.a.name}: {name} failed its own checks ({rounds[-1]['errors']}): revising")
            draft = False
        self.rounds[name] = rounds
        x, z, yaw = self.positions[name]
        self.a.call("world_spawn", shape="model", recipe=f"recipes/{name}.json", name=name, parent=STAGING,
                    position=[x, self.positions["_y"].get(name, 0.0), z], rotation_deg=[0, yaw, 0], tags=["staging", "m4_exit", "prop"])
        self.report["holding"].pop(self.a.name, None)
        self.a.call("claim_release", id=claim["id"])
        passed = rounds[-1]["style_passed"] and rounds[-1]["taste_passed"]
        self.a.call("task_done", id=task["id"], summary=f"{name}: {len(rounds)} round(s), style.toml and taste checks {'pass' if passed else 'FAIL'}; staged at {STAGING}/{name}")
        self.done.append(name)
        log(f"{self.a.name}: {name} done ({len(rounds)} round(s), {'pass' if passed else 'FAIL'})")

    def fix(self, task):
        """A critique turned into a task: apply the revision, re-check, finish (the critique
        resolves itself with the summary)."""
        name = self.prop_of(task)
        claim = self.a.call("claim_take", path=f"/project/assets/recipes/{name}.json", lease_s=600)
        self.write(name, recipe_text(name, draft=False))
        style, taste = self.checks(name)
        ok = style.get("passed") and taste.get("passed")
        self.rounds.setdefault(name, []).append({"critique_fix": True, "style_passed": style.get("passed"), "taste_passed": taste.get("passed")})
        self.a.call("claim_release", id=claim["id"])
        self.a.call("task_done", id=task["id"], summary=f"{name} revised after the art director's critique; style.toml and taste checks {'pass' if ok else 'FAIL'}")
        log(f"{self.a.name}: fixed {name} after critique ({'pass' if ok else 'FAIL'})")


# ---------------------------------------------------------------- phase 1

def surface_heights(director):
    """Each prop's ground height: a ray down onto the slab where it stands (physics), else 0."""
    ys = {}
    for name, x, z, _ in PROPS:
        r, err = director.raw("physics_raycast", origin=[x, 6.0, z], direction=[0, -1, 0], max_distance=12)
        hits = (r or {}).get("hits") or []
        ys[name] = round(float(hits[0]["point"][1]), 3) if hits else 0.0
    return ys


def phase1(a, out, report):
    assets = prepare_assets(out)
    with Engine(a.exe or EXE, a.port, out / "engine.log", *engine_args(a, out, assets), cwd=str(out)) as eng:
        report["engine_pid"] = eng.proc.pid
        (out / "engine.pid").write_text(str(eng.proc.pid))
        tokens = out / "tokens"
        tokens.mkdir(exist_ok=True)
        director = Agent(a.port, "director", "director", "?role=director", tokens)
        builders = [Agent(a.port, f"builder-{i}", "builder", "?role=builder&tools=project,studio,review", tokens) for i in range(1, 5)]
        artdir = Agent(a.port, "artdir", "reviewer", "?role=art_director", tokens)
        skeptic = Agent(a.port, "skeptic", "sight-skeptic", "?role=sight_skeptic", tokens)
        debugger_agent = Agent(a.port, "debugger", "reviewer", "?role=debugger", tokens)
        agents = [director, *builders, artdir, skeptic, debugger_agent]
        report["listed_tools"] = {ag.name: len(ag.mcp.tools()) for ag in agents}
        log(f"tools listed per session: {report['listed_tools']}")
        stop = threading.Event()
        dbg = Debugger(debugger_agent, stop)
        dbg.start()

        # -- the director: scopes, staging, plan --
        for b in builders:
            director.call("tools_scope", agent=b.name, role="builder", tools=["project", "studio", "review"], cap=True)
        probe, err = builders[0].raw("swarm_settings", frame_floor_fps=30)
        check(err is not None and err.get("error") == "not_in_scope", f"a capped builder can't reach the lead's tools ({(err or {}).get('error')})")
        for path, kw in (("/staging", {}), (STAGING, {"position": STAGING_AT, "tags": ["staging"]}), ("/props", {}), (PROPS_ROOT, {})):
            parent, name = path.rsplit("/", 1)
            director.call("world_spawn", shape="group", name=name, **({"parent": parent} if parent else {}), **kw)
        ys = surface_heights(director)
        positions = {n: (x, z, yaw) for n, x, z, yaw in PROPS}
        positions["_y"] = ys
        report["ground_heights"] = ys
        items = [{"key": n, "title": f"Build {n} (map 1 graveyard prop)", "role": "builder",
                  "detail": f"Recipe recipes/{n}.json: claim it, write it, pass style_check and studio_check against the "
                            f"{GROUP_OF[n].replace('_', ' ')} siblings, stage it at {STAGING}/{n}. prop:{n}"} for n in NAMES]
        items.append({"key": "review", "title": "Review the 33 graveyard props", "role": "reviewer", "after": NAMES,
                      "detail": "studio_sheet under worst light and on a phone, line-ups for distinctness; critique what is weak"})
        plan = director.call("task_plan", items=items, assign="by_role")
        report["plan"] = {"id": plan.get("plan"), "tasks": len(plan.get("tasks", []))}
        check(len(plan.get("tasks", [])) == len(NAMES) + 1, f"task_plan posted {len(plan.get('tasks', []))} tasks for {len(NAMES)} props and the review")
        report["assigned"] = {t.get("title", "")[6:].split(" ")[0]: t.get("assignee") for t in plan.get("tasks", []) if t.get("title", "").startswith("Build ")}

        # -- the art director's look decisions, while the builders start --
        report["written"] = set()
        report["written_by"] = {}
        report["holding"] = {}
        workers = [Builder(b, director.name, positions, report) for b in builders]
        for w in workers:
            w.start()

        # -- a deliberate cross-agent probe: builder-2 tries to write a recipe builder-1 holds --
        conflict = None
        for _ in range(240):
            held = report["holding"].get("builder-1")
            if held:
                # A valid recipe that differs from what is there (the same prop's, with a note in
                # its description), so only the claim can refuse it. (A write of the very same
                # text answers `unchanged` without a claim check: a rough edge, see the report.)
                cur, cerr = builders[1].raw("project_read", path=f"assets/recipes/{held}.json", problems=False)
                if cerr:  # claimed but not written yet: look again
                    time.sleep(0.25)
                    continue
                probe = cur["text"].replace('"description": "', '"description": "(builder-2 probe) ', 1)
                same, same_err = builders[1].raw("project_write", path=f"assets/recipes/{held}.json", text=cur["text"])
                report["claim_probe_same_text"] = same_err or same
                _, conflict = builders[1].raw("project_write", path=f"assets/recipes/{held}.json", text=probe)
                if conflict is None or conflict.get("error") != "stale_version":
                    break
            time.sleep(0.25)
        check(conflict is not None and conflict.get("error") == "claimed", f"another builder's claimed recipe is refused ({(conflict or {}).get('error')})")
        report["claim_probe"] = conflict

        for w in workers:
            w.join()
        for w in workers:
            check(w.error is None and not w.failed, f"{w.a.name} finished its tasks ({w.error or w.failed or 'ok'}): {w.done}")
        report["builder_rounds"] = {n: r for w in workers for n, r in w.rounds.items()}
        prog = director.call("task_progress", plan=report["plan"]["id"])
        report["progress_after_build"] = prog
        staged = [e for e in director.call("world_query", path_prefix=STAGING, fields=["path"], limit=100).get("items", []) if e["path"].startswith(STAGING + "/")]
        check(len(staged) == len(NAMES), f"all {len(NAMES)} props are staged ({len(staged)})")

        # -- the art director's look decisions (gallery rounds on the built props), then its
        # review: critiques for builders to fix --
        report["galleries"] = galleries(artdir, out)
        crits = art_review(artdir, out, report)
        if crits:
            fixers = [Builder(b, director.name, positions, report) for b in builders]
            for f in fixers:
                f.start()
            for f in fixers:
                f.join()
            for f in fixers:
                for n, r in f.rounds.items():
                    report["builder_rounds"].setdefault(n, []).extend(r)
        open_crit = artdir.call("critique_list", status=["open", "assigned", "in_task"], source="artdir").get("critiques", [])
        resolved = artdir.call("critique_list", status="resolved", source="artdir").get("critiques", [])
        report["artdir_resolved"] = len(resolved)
        check(not open_crit and len(resolved) == len(crits), f"every art-director critique was fixed and resolved itself ({len(resolved)} resolved, {len(open_crit)} open)")

        # -- what is submitted is what the repo holds: every recipe the swarm wrote is the final one --
        differ = [n for n in NAMES if director.call("project_read", path=f"assets/recipes/{n}.json", problems=False).get("text") != recipe_text(n, draft=False)]
        check(not differ, f"every prop's recipe in the run is the committed final recipe ({differ or 'all 33'})")

        # -- final gate: every prop passes style.toml (engine-measured, all variants) --
        style = director.call("style_check", subjects=[{"recipe": f"recipes/{n}.json"} for n in NAMES], response_format="detailed")
        report["style_check"] = {"passed": style.get("passed"), "subjects": style.get("subjects"), "failed": style.get("failed")}
        check(style.get("passed") and style.get("failed") == 0, f"style_check: {style.get('subjects')} subjects (variants included), {style.get('failed')} failed")

        # -- the one contact sheet --
        sheet = director.call("studio_sheet", subjects=[{"recipe": f"recipes/{n}.json"} for n in NAMES],
                              views=["three_quarter", "gameplay"], rig="game", tile=176, response_format="detailed")
        sheet_png, sheet_json = Path(sheet["image"]["path"]), Path(sheet["image"]["packet_path"])
        shutil.copy(sheet_png, out / "contact_sheet.png")
        shutil.copy(sheet_json, out / "contact_sheet.json")
        report["contact_sheet"] = {"path": str(sheet_png), "packet": str(sheet_json), "width": sheet["image"].get("width"), "height": sheet["image"].get("height")}
        log(f"contact sheet: {out / 'contact_sheet.png'} ({sheet['image'].get('width')}x{sheet['image'].get('height')})")

        # -- the sight-skeptic, before submitting --
        report["skeptic"] = skeptic_pass(skeptic, out, sheet)

        # -- the submission --
        checks = []
        by_name = {r["subject"]: r for r in style.get("results", [])}
        for n in NAMES:
            r = by_name.get(n) or next((v for k, v in by_name.items() if k.startswith(n + " ")), {})
            tris = (r.get("facts") or {}).get("triangles", [0])[0]
            checks.append({"kind": "style", "name": f"{n}: style.toml", "passed": bool(r.get("passed")), "severity": "error",
                           "metric": "triangles", "value": tris, "detail": f"style_check {r.get('checks')} checks, {r.get('failed')} failed"})
        taste_ok = sum(1 for n in NAMES if (report["builder_rounds"].get(n) or [{}])[-1].get("taste_passed"))
        checks.append({"kind": "taste", "name": "studio_check (taste) on every prop", "passed": taste_ok == len(NAMES), "severity": "error",
                       "metric": "props_passing", "value": taste_ok, "limit": len(NAMES)})
        checks.append({"kind": "other", "name": "sight-skeptic: staged props drawn, sheet complete", "passed": report["skeptic"]["passed"],
                       "severity": "warning", "detail": report["skeptic"]["summary"][:300]})
        picks = "; ".join(f"{g['recipe']}.{g['param']} = {g['pick']} (art director's pick, gallery {g['id']})" for g in GALLERIES)
        summary = (f"33 map-1 graveyard props (Sepulcra, the dark graveyard): procedural recipes (CSG, SDF sculpts, lathes, sweeps) by a "
                   f"4-builder swarm, each through style_check and studio_check, reviewed by the art director, watched by a debugger. "
                   f"Approve to promote them from {STAGING} to {PROPS_ROOT}: they then stand on the slab. Look decisions: {picks}. "
                   f"Style proposals for the owner: a 'prop' category, fire for candles and lanterns, a 'wood' preset.")
        sub = director.call("review_submit", title="Map 1 graveyard props (33), M4 exit set", kind="asset", batch=BATCH, summary=summary[:2000],
                            paths=[f"{STAGING}/{n}" for n in NAMES], promote={"to": PROPS_ROOT},
                            captures=[{"path": str(sheet_png), "kind": "contact_sheet", "packet": str(sheet_json),
                                       "caption": "every prop: three-quarter (game light) and the gameplay shot at true pixel size"}],
                            checks=checks)
        report["submission"] = {"id": sub.get("id"), "status": sub.get("status"), "engine_checks": sub.get("checks")}
        check(sub.get("status") == "pending", f"one submission #{sub.get('id')} is pending with {len(checks)} checks and the contact sheet")

        # -- refusals that must hold: nobody but the owner decides it --
        _, e1 = builders[0].raw("review_decide", ids=[sub["id"]], verdict="approve")
        _, e2 = director.raw("review_decide", ids=[sub["id"]], verdict="approve", note="probe: self-review")
        check(e1 and e1.get("error") == "not_reviewer", f"a builder can't decide it ({(e1 or {}).get('error')})")
        check(e2 and e2.get("error") == "self_review", f"the submitter can't decide it ({(e2 or {}).get('error')})")
        _, e3 = builders[0].raw("agent_hello", name="window:owner", role="owner")
        check(e3 is not None, f"no agent can say hello as the window's owner ({(e3 or {}).get('error')})")

        prog = director.call("task_progress", plan=report["plan"]["id"])
        report["progress"] = prog
        stop.set()
        dbg.join(timeout=10)
        report["debugger"] = {"incidents": dbg.handled, "warn_events": dbg.warn_events, "warn_codes": dbg.event_codes, "errors": dbg.errors,
                              "reverify": dbg.reverify()}
        report["agents"] = {ag.name: ag.stats for ag in agents}
        show_on_screen(director, sub["id"], report, a.headless)
        director.call("world_save")
        (out / "submission.json").write_text(json.dumps({"id": sub["id"], "port": a.port, "adapter": a.adapter}, indent=1))
        write_summary(out, report)
        print_instructions(a, sub["id"], out)
        if not a.no_wait:
            wait_for_owner(director, sub["id"], report, out)
        for ag in agents:
            ag.close()


def galleries(artdir, out):
    """Two-plus look decisions as gallery rounds; the art director's pick is recorded (not made
    as the owner)."""
    res = []
    for g in GALLERIES:
        # A stepped number, not a `choice`: a choice on recipe.<param> always renders 0 (a rough
        # edge found here: gallery.rs passes `as_num()` of the choice, which is None).
        space = [{"name": g["param"], "target": f"recipe.{g['param']}", "kind": "number", "min": g["min"], "max": g["max"], "default": g["pick"],
                  **({"step": g["step"]} if g["step"] else {})}]
        gal, err = artdir.raw("gallery_create", id=g["id"], title=g["title"], subject={"recipe": f"recipes/{g['recipe']}.json", "rig": "game"},
                              space=space, variants=len(g["values"]), image=False)
        if err:
            if err.get("error") in ("exists", "gallery_exists", "id_taken"):
                gal, err = artdir.raw("gallery_get", id=g["id"])
            if err:
                res.append({"id": g["id"], "error": err})
                check(False, f"gallery {g['id']}: {json.dumps(err)[:200]}")
                continue
        got, _ = artdir.raw("gallery_get", id=g["id"], image=False)
        rnd = (got or {}).get("round") or {}
        variants = rnd.get("variants") or (got or {}).get("variants") or []
        values = sorted({v.get("values", {}).get(g["param"]) for v in variants if isinstance(v, dict)} - {None})
        sheet = rnd.get("sheet") or (got or {}).get("sheet")
        res.append({"id": g["id"], "recipe": g["recipe"], "param": g["param"], "values_shown": values, "sheet": sheet,
                    "art_director_pick": g["pick"], "reason": g["reason"], "decided_by_owner": False})
        check(len(values) >= 2, f"gallery {g['id']}: round 1 shows {values} for the owner (art director's pick {g['pick']})")
    return res


def art_review(artdir, out, report):
    """The art director's pass: sheets under worst light and on a phone, line-ups per sibling
    group; anything under ART_BAR gets a critique turned into a task for the builder who made it."""
    review = artdir.call("task_list", ready_for_me=True, tag="role:reviewer").get("tasks", [])
    if review:
        artdir.raw("task_take", id=review[0]["id"])
    findings = []
    made_by = report["written_by"]
    for gname, names in GROUPS.items():
        sh = artdir.call("studio_sheet", subjects=[{"recipe": f"recipes/{n}.json"} for n in names], views=["worst_light", "phone"],
                         response_format="detailed")
        shutil.copy(sh["image"]["path"], out / f"artdir_{gname}_worst_phone.png")
        packet = json.loads(Path(sh["image"]["packet_path"]).read_text())
        for t in packet.get("tiles", []):
            m = t.get("shot") or {}
            v = t.get("view")
            if v == "worst_light" and m.get("contrast") is not None and m["contrast"] < ART_BAR["worst_contrast"]:
                findings.append((t["subject"], f"reads poorly backlit: outline contrast {m['contrast']:.3f} under the worst light", {"name": "worst_contrast", "value": m["contrast"], "limit": ART_BAR["worst_contrast"]}))
            if v == "phone" and m.get("px_height") is not None and m["px_height"] < ART_BAR["phone_px"]:
                findings.append((t["subject"], f"too small on a phone: {m['px_height']:.0f} px tall", {"name": "phone_px", "value": m["px_height"], "limit": ART_BAR["phone_px"]}))
        ln = artdir.call("studio_lineup", subjects=[{"recipe": f"recipes/{n}.json"} for n in names], rig="game", response_format="detailed")
        shutil.copy(ln["image"]["path"], out / f"artdir_{gname}_lineup.png")
        lp = json.loads(Path(ln["image"]["packet_path"]).read_text())
        subs = [s.get("subject", s.get("name")) if isinstance(s, dict) else s for s in lp.get("subjects", [])] or names
        iou = lp.get("silhouette_iou") or []
        for i, row in enumerate(iou):
            for j, v in enumerate(row):
                if j > i and v is not None and v > ART_BAR["silhouette_iou"]:
                    findings.append((subs[i], f"silhouette too close to {subs[j]} (IoU {v:.2f})", {"name": "silhouette_iou", "value": v, "limit": ART_BAR["silhouette_iou"]}))
        report.setdefault("artdir_lineups", {})[gname] = {"subjects": subs, "max_iou": max([v for i, r in enumerate(iou) for j, v in enumerate(r) if j > i and v is not None] or [0])}
    report["artdir_findings"] = [{"subject": s, "text": t, "metric": m} for s, t, m in findings]
    crits = []
    per_prop = {}
    for subject, text, metric in findings:
        per_prop.setdefault(subject.split(" ")[0], []).append((text, metric))
    for name, items in per_prop.items():
        text = "; ".join(t for t, _ in items)
        added = artdir.call("critique_add", items=[{"text": f"{name}: {text}", "severity": "warning", "source": "artdir", "metric": items[0][1],
                                                    "path": f"/project/assets/recipes/{name}.json"}])
        cid = added["added"][0]
        owner = made_by.get(name) or "builder-1"
        t = artdir.call("critique_update", id=cid, action="to_task", assignee=owner)
        tid = t["results"][0]["task"]
        crits.append({"critique": cid, "task": tid, "prop": name, "assignee": owner, "text": text})
        log(f"artdir: critique {cid} on {name} -> task {tid} for {owner}: {text}")
    report["artdir_critiques"] = crits
    if review:
        artdir.raw("task_done", id=review[0]["id"], summary=f"reviewed {len(NAMES)} props: {len(crits)} critiques, {len(report.get('artdir_notes', []))} notes")
    return crits


def skeptic_pass(skeptic, out, sheet):
    """Every staged prop is drawn in a frame of the staging lot, and the contact sheet has every
    prop's tiles: the packets agree with what was submitted."""
    problems = []
    f = skeptic.call("sight_frame", eye=[STAGING_AT[0] + 2, 46, STAGING_AT[2] + 44], target=[STAGING_AT[0], 0, STAGING_AT[2] - 4],
                     fov_y_deg=55, width=1920, height=1080, max_subjects=64, response_format="detailed")
    shutil.copy(f["image"]["path"], out / "skeptic_staging.png")
    shutil.copy(f["image"]["packet_path"], out / "skeptic_staging.json")
    seen, hlod = {}, []
    positions = {n: (x, z) for n, x, z, _ in PROPS}
    for s in f.get("subjects", []):
        p = s.get("path", "")
        if p.startswith(STAGING + "/"):
            seen[p.split("/")[-1]] = s.get("visible_px") or 0
            if s.get("drawn_as") == "hlod":
                hlod.append(p.split("/")[-1])
    # Props in streaming cells far from the view are drawn merged in their cell's HLOD proxy
    # (visible_px null by design): each is looked at again from close by, where it is drawn
    # itself and its pixels are counted.
    close_frames = {}
    for n in NAMES:
        if seen.get(n, 0) > 0:
            continue
        x, z = positions[n]
        at = [STAGING_AT[0] + x, STAGING_AT[1] + 0.8, STAGING_AT[2] + z]
        g = skeptic.call("sight_frame", eye=[at[0] + 1.5, at[1] + 5, at[2] + 8], target=at, fov_y_deg=50, width=640, height=360,
                         max_subjects=16, response_format="detailed")
        for s in g.get("subjects", []):
            if s.get("path") == f"{STAGING}/{n}":
                seen[n] = s.get("visible_px") or 0
                close_frames[n] = {"visible_px": s.get("visible_px"), "drawn_as": s.get("drawn_as"), "image": g["image"]["path"]}
        shutil.copy(g["image"]["path"], out / f"skeptic_close_{n}.png")
    for n in NAMES:
        if seen.get(n, 0) <= 0:
            problems.append(f"{n}: not drawn in the staging frame ({seen.get(n)})")
    tiles = {}
    for t in sheet.get("subjects", []):
        tiles[t.get("subject")] = len(t.get("tiles", []))
    for n in NAMES:
        if tiles.get(n, 0) != 2:
            problems.append(f"{n}: {tiles.get(n, 0)} tiles on the contact sheet (2 expected)")
    if problems:
        skeptic.raw("critique_add", items=[{"text": p, "severity": "warning", "source": "sight-skeptic", "capture": str(out / "skeptic_staging.png")} for p in problems[:32]])
    summary = f"{sum(1 for n in NAMES if seen.get(n, 0) > 0)}/{len(NAMES)} staged props drawn; {sum(1 for n in NAMES if tiles.get(n) == 2)}/{len(NAMES)} on the sheet"
    check(not problems, f"sight-skeptic: {summary}")
    return {"passed": not problems, "summary": summary, "problems": problems, "visible_px": seen, "hlod_in_overview": hlod,
            "close_frames": close_frames}


def show_on_screen(director, sid, report, headless=False):
    st, err = director.raw("review_screen", action="show", id=sid)
    if headless:
        log(f"headless development run: the review screen answered {json.dumps(st or err)[:200]}")
        report["screen"] = st or err
        return
    st = st or {}
    check(st.get("open") and st.get("shown") == sid, f"the submission is on the review screen ({st.get('shown')}, {json.dumps(err)[:200] if err else 'ok'})")
    report["screen"] = st


def print_instructions(a, sid, out):
    log("")
    log("=" * 78)
    log(f"OWNER: submission #{sid} (33 graveyard props) is on Sepulcra's review screen.")
    log("  Look at the contact sheet on the screen (and full size at")
    log(f"  {out / 'contact_sheet.png'}).")
    log("  Approve: click Approve (or Enter on it; a gamepad's A). Reject: Reject, then a reason.")
    log("  The review screen opens and closes with F9 (a pad: LB + RB + Y).")
    log("  Agents cannot approve it: only the window's owner decides.")
    log("  Optional, after the review screen is closed: F8 opens the newest undecided gallery of three look")
    log("  decisions (the angel's wings, the rune obelisk's runes, the tripod brazier's fire; F8 closes it,")
    log("  and opening it again shows the newest one still undecided). The art director's picks are recorded;")
    log("  yours ('This one') override them.")
    log(f"  Later: python scripts/m4_exit.py --review --out \"{out}\" --exe \"{a.exe or EXE}\"")
    log("         (reopens the window on the saved world and waits for the decision)")
    log("=" * 78)


def wait_for_owner(director, sid, report, out):
    log("waiting for the owner's decision at the window (Ctrl+C to stop; resume with --review)...")
    while True:
        item = director.call("review_list", id=sid)
        if item.get("status") != "pending":
            report["decision"] = item.get("decision")
            log(f"decided: {item.get('status')} by {(item.get('decision') or {}).get('by')}")
            director.call("world_save")
            write_summary(out, report)
            return
        time.sleep(2.0)


def write_summary(out, report):
    def clean(v):
        if isinstance(v, set):
            return sorted(v)
        if isinstance(v, dict):
            return {str(k): clean(x) for k, x in v.items()}
        if isinstance(v, list):
            return [clean(x) for x in v]
        return v
    root = str(ROOT)
    text = json.dumps(clean(report), indent=1).replace(root.replace("\\", "\\\\"), "<repo>").replace(str(out).replace("\\", "\\\\"), "<out>")
    (out / "summary.json").write_text(text)


def review(a, out):
    """The owner's window on the saved world: show the submission and wait for the decision."""
    sub = json.loads((out / "submission.json").read_text())
    assets = out / "assets"
    with Engine(a.exe or EXE, a.port, out / "engine_review.log", *engine_args(a, out, assets), cwd=str(out)) as eng:
        director = Agent(a.port, "director", "director", "?role=director", out / "tokens")
        item = director.call("review_list", id=sub["id"])
        log(f"submission #{sub['id']}: {item.get('status')}")
        report = {"review_run": True}
        if item.get("status") == "pending":
            show_on_screen(director, sub["id"], report)
            print_instructions(a, sub["id"], out)
            wait_for_owner(director, sub["id"], report, out)
        director.close()
        _ = eng


# ---------------------------------------------------------------- phase 2

def game_eye(yaw_deg, pitch_deg=42.0, dist=56.0, target=(0.0, 0.0, 0.0)):
    """A camera like the game's default orbit (style.toml [shots.gameplay]: 42 degrees down, 56 m)."""
    import math
    y, p = math.radians(yaw_deg), math.radians(pitch_deg)
    return [target[0] + dist * math.sin(y) * math.cos(p), target[1] + dist * math.sin(p), target[2] + dist * math.cos(y) * math.cos(p)]


def props_drawn(mcp, out, label, report):
    """Every promoted prop drawn in the game's view: two overview frames from the game camera's
    orbit (opposite sides), then a close frame of any prop the overviews didn't measure (behind
    something, or merged in a distant streaming cell's HLOD proxy). Returns {prop: visible_px}."""
    seen, frames = {}, []
    for yaw in (35, 215):
        f = mcp.json("sight_frame", eye=game_eye(yaw), target=[0, 0, 0], fov_y_deg=45, width=1280, height=720, max_subjects=64,
                     response_format="detailed")
        name = f"{label}_overview_yaw{yaw}"
        shutil.copy(f["image"]["path"], out / f"{name}.png")
        shutil.copy(f["image"]["packet_path"], out / f"{name}.json")
        frames.append(name)
        # The answer lists the most relevant subjects only; the saved packet has every one.
        for s in json.loads(Path(f["image"]["packet_path"]).read_text()).get("subjects", []):
            p = s.get("path", "")
            if p.startswith(PROPS_ROOT + "/"):
                n = p.split("/")[-1]
                seen[n] = max(seen.get(n, 0), s.get("visible_px") or 0)
    pos = {e["path"].split("/")[-1]: e["position"] for e in report["promoted_entities"]}
    close = {}
    for n in NAMES:
        if seen.get(n, 0) > 0:
            continue
        at = [pos[n][0], pos[n][1] + 0.8, pos[n][2]]
        g = mcp.json("sight_frame", eye=[at[0] + 1.5, at[1] + 6, at[2] + 8], target=at, fov_y_deg=50, width=640, height=360, max_subjects=24,
                     response_format="detailed")
        shutil.copy(g["image"]["path"], out / f"{label}_close_{n}.png")
        shutil.copy(g["image"]["packet_path"], out / f"{label}_close_{n}.json")
        for s in json.loads(Path(g["image"]["packet_path"]).read_text()).get("subjects", []):
            if s.get("path") == f"{PROPS_ROOT}/{n}":
                seen[n] = s.get("visible_px") or 0
                close[n] = {"visible_px": s.get("visible_px"), "drawn_as": s.get("drawn_as")}
    report.setdefault("drawn", {})[label] = {"visible_px": seen, "overviews": frames, "close": close}
    return seen


def after_approval(a, out):
    """Phase 2: the approved props are in the game world (promoted by the window's owner), drawn in
    Sepulcra's view, and the game still plays to its end within the iGPU budget, with a verified
    replay and no validation messages (scripts/m3_exit.py's criterion, on this world)."""
    from m2_exit import PERF_TOML
    from m3_exit import measure
    from sepulcra_play import Run
    sub = json.loads((out / "submission.json").read_text())
    play = out / "play"
    if play.exists():
        shutil.rmtree(play)
    play.mkdir(parents=True)
    # Play on a copy of the approved world, so the game's own autosaves never overwrite it.
    shutil.copytree(out / "worlds" / WORLD, play / "worlds" / WORLD)
    args = [*(["--headless"] if a.headless else []), "--adapter", a.adapter, "--size", "1280x720", "--perf", str(PERF_TOML),
            "--assets", str(out / "assets"), "--captures", str(play / "captures"), "--worlds", str(play / "worlds"), "--world", WORLD,
            "--saves", str(play / "saves"), "--trace", str(play / "trace"), "--lead", "director",
            "--", "--seed", str(a.seed), "--mute", "--rules", "lex"]
    report = {"phase": 2, "started": time.strftime("%Y-%m-%d %H:%M"), "adapter": a.adapter, "submission": sub["id"]}
    measured = []
    with Engine(a.exe or EXE, a.port, play / "engine.log", *args, cwd=str(play)) as eng:
        mcp = eng.mcp("m4-exit-play")

        def call_raw(tool, **kw):
            from specula_mcp import detailed
            return mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, kw)})
        mcp.call_raw = call_raw
        for _ in range(120):  # the game loads its models first: the engine may answer busy a while
            r = mcp.request("tools/call", {"name": "agent_hello", "arguments": {"name": "director", "role": "director"}})
            if not r.get("isError"):
                break
            time.sleep(1.0)
        report["gpu"] = mcp.json("engine_info")["gpu"]["name"]

        # -- the decision and the promotion, as the queue recorded them --
        item = mcp.json("review_list", id=sub["id"], response_format="detailed")
        dec = item.get("decision") or {}
        moved = ((dec.get("promoted") or {}).get("moved")) or []
        report["decision"] = {"status": item.get("status"), "by": dec.get("by"), "journal": (dec.get("promoted") or {}).get("journal"),
                              "moved": len(moved), "journal_epoch": item.get("journal_epoch"), "journal_stale": item.get("journal_stale")}
        check(item.get("status") == "approved" and dec.get("by") == "window:owner", f"submission #{sub['id']} was approved at the window ({item.get('status')} by {dec.get('by')})")
        check(len(moved) == len(NAMES) and all(m["to"] == f"{PROPS_ROOT}/{m['from'].split('/')[-1]}" for m in moved),
              f"the approval promoted all {len(moved)} props from {STAGING} to {PROPS_ROOT}")

        # -- the props in the world, where map 1 has them, written by the decider --
        q = mcp.json("world_query", path_prefix=PROPS_ROOT, fields=["path", "shape", "position", "rotation_deg", "recipe", "tags", "version",
                                                                   "created_by", "modified_by"], limit=100)
        ents = [e for e in q.get("items", []) if e["path"].startswith(PROPS_ROOT + "/")]
        report["promoted_entities"] = ents
        want = {n: (x, z, yaw) for n, x, z, yaw in PROPS}
        # M5: the props are map data (games/sepulcra/data/slab_01.json). A prop the map data moved
        # from its approved place (the corner fence, so it doesn't block the lab's build spots) is
        # moved there by the game; every other one keeps the owner's approval as its last write.
        data = {p["name"]: (p["at"][0], p["at"][1]) for p in json.loads((ROOT / "games" / "sepulcra" / "data" / "slab_01.json").read_text()).get("props", [])}
        moved = {n for n, (x, z) in data.items() if n in want and (abs(want[n][0] - x) > 1e-3 or abs(want[n][1] - z) > 1e-3)}
        report["moved_by_map_data"] = sorted(moved)
        bad = []
        for e in ents:
            n = e["path"].split("/")[-1]
            x, z, yaw = want.get(n, (None, None, None))
            if n in moved:
                x, z = data[n]
            p = e.get("position", [0, 0, 0])
            if x is None or abs(p[0] - x) > 1e-3 or abs(p[2] - z) > 1e-3 or not (0.0 <= p[1] <= 0.8):
                bad.append((n, p))
            if (e.get("modified_by") != "window:owner" and n not in moved) or "staging" in (e.get("tags") or []):
                bad.append((n, e.get("modified_by"), e.get("tags")))
        check(len(ents) == len(NAMES) and not bad, f"{len(ents)} props stand in {PROPS_ROOT} at their map-1 places, last written by window:owner "
              f"(moved by the map data: {sorted(moved) or 'none'}), untagged staging ({bad[:3] or 'all'})")
        left = [e for e in mcp.json("world_query", path_prefix=STAGING, fields=["path"], limit=100).get("items", []) if e["path"].startswith(STAGING + "/")]
        check(not left, f"nothing is left in staging ({len(left)})")
        hist = mcp.json("world_history", path_prefix=PROPS_ROOT, limit=5, response_format="detailed")
        report["world_history_now"] = hist
        # The journal belongs to one run (REVIEW.md: journal refs belong to one epoch): the
        # promotion's entry was in the approval run's journal; here the world was loaded.
        report["journal_note"] = ("world_history in this run starts at the load; the promotion was journal entry "
                                  f"{report['decision']['journal']} of the approval run's epoch {report['decision']['journal_epoch']}, "
                                  "committed by the review queue as the decider (Rule::World through write_policy), and every "
                                  "promoted entity's modified_by is window:owner")

        # -- a new game on the slab with the props: drawn in the game's view --
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=a.seed)
        still = [e for e in mcp.json("world_query", path_prefix=PROPS_ROOT, fields=["path"], limit=100).get("items", []) if e["path"].startswith(PROPS_ROOT + "/")]
        check(len(still) == len(NAMES), f"a new game keeps the props on the slab ({len(still)})")
        seen = props_drawn(mcp, out / "play", "game_start", report)
        check(all(seen.get(n, 0) > 0 for n in NAMES), f"all 33 props are drawn in the game's view ({sum(1 for n in NAMES if seen.get(n, 0) > 0)}/33)")

        # -- play the whole game, measured as the M3 exit measures it --
        run = Run(mcp, out / "play")
        run.capture("build_phase", eye=[38, 27, 52], target=[0, -9, 1])
        busy = boss = False
        while True:
            st = run.state()
            if st["outcome"] != "running":
                break
            run.drive()
            st = run.state()
            if not busy and st["wave"] >= 6 and st["enemies"] >= 18:
                measured.append(measure(mcp, eng, "busy_wave", a.frames, st))
                run.capture("busy_wave", eye=[38, 27, 52], target=[0, -9, 1])
                run.capture("busy_wave_north", eye=game_eye(215), target=[0, 0, -8])
                busy = True
                continue
            if not boss and st["wave"] == 10 and st["enemies"] >= 1:
                measured.append(measure(mcp, eng, "boss_wave", a.frames, st))
                run.capture("boss_wave", eye=[38, 27, 52], target=[0, -9, 1])
                boss = True
                continue
            run.step(30)
        st = run.state()
        report["end"] = {k: st[k] for k in ("outcome", "tick", "wave", "tombstones", "state_hash")}
        rec = mcp.json("play_record", path="recordings/m4-exit.json", overwrite=True)
        rep = mcp.json("play_replay", path="recordings/m4-exit.json")
        lex = st.get("rules") or {}
        report["replay"] = {"status": rep.get("status"), "lex_parity": lex.get("parity"), "lex_hash_recorded": (rec.get("lex") or {}).get("end_hash"),
                            "lex_hash_replayed": (rep.get("lex") or {}).get("end_hash")}
        seen_end = props_drawn(mcp, out / "play", "game_end", report)
        check(all(seen_end.get(n, 0) > 0 for n in NAMES), f"all 33 props are still drawn at the game's end ({sum(1 for n in NAMES if seen_end.get(n, 0) > 0)}/33)")
        report["captures"] = run.captures
        log(f"end: {st['outcome']} at wave {st['wave']}; replay {rep.get('status')}, Lex parity {lex.get('parity')}")
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    report["measured"] = measured
    replay_ok = (report["replay"]["status"] == "verified" and report["replay"]["lex_parity"] == "ok"
                 and report["replay"]["lex_hash_recorded"] == report["replay"]["lex_hash_replayed"])
    check(report["end"]["outcome"] == "won", f"the game is won with the props placed ({report['end']['outcome']} at wave {report['end']['wave']}, {report['end']['tombstones']} tombstones)")
    check(replay_ok, f"the recording replays as verified, Lex parity ok ({report['replay']})")
    check(errors == 0, f"validation errors at shutdown: {errors}")
    check(len(measured) == 2 and all(m["passed"] for m in measured), "the busy and boss waves hold the frame-level budgets: " + "; ".join(
        f"{m['label']} frame p95 {(m.get('frame_ms') or {}).get('p95')} ms, GPU p95 {(m.get('gpu_ms') or {}).get('p95')} ms" for m in measured))
    report["fails"] = FAILS
    write_summary(out / "play", report)
    if FAILS:
        print(f"\n{len(FAILS)} check(s) failed")
        sys.exit(1)
    print("m4_exit phase 2: all checks passed")


def main():
    global TOKENS
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7871)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--size", default="1600x900")
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--out", default=str(ROOT / "target" / "m4-exit"))
    ap.add_argument("--no-wait", action="store_true", help="save and exit once the submission is on the screen")
    ap.add_argument("--frames", type=int, default=600, help="phase 2: window frames per perf capture (as m3_exit.py)")
    ap.add_argument("--exe", help="the sepulcra binary (default: release under CARGO_TARGET_DIR or target/)")
    ap.add_argument("--headless", action="store_true", help="development runs only: no window (the owner can't approve there)")
    ap.add_argument("--review", action="store_true", help="reopen the saved world's review screen for the owner")
    ap.add_argument("--after-approval", action="store_true", help="phase 2: verify the approved props in the game")
    a = ap.parse_args()
    out = Path(a.out)
    if a.after_approval:
        after_approval(a, out)
        return
    if a.review:
        review(a, out)
        return
    if (out / "worlds").exists():
        shutil.rmtree(out / "worlds")
    for d in ("tokens", "captures", "trace", "saves"):
        if (out / d).exists():
            shutil.rmtree(out / d)
    out.mkdir(parents=True, exist_ok=True)
    TOKENS = out / "tokens"
    report = {"started": time.strftime("%Y-%m-%d %H:%M"), "adapter": a.adapter, "props": NAMES}
    t0 = time.time()
    try:
        phase1(a, out, report)
    finally:
        report["wall_s"] = round(time.time() - t0, 1)
        report["fails"] = FAILS
        write_summary(out, report)
    if FAILS:
        print(f"\n{len(FAILS)} check(s) failed")
        sys.exit(1)
    print("m4_exit phase 1: all checks passed")


if __name__ == "__main__":
    main()

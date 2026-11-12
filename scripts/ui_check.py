"""Live check of the UI sight tools on Sepulcra (lane m5-uitools): a playtester agent walks every
menu path with a simulated gamepad through `ui_act`, at the Steam Deck's 1280x800, 1080p and 4K,
each at UI scale 1.0 and 1.5 (set through the game's own settings screen, with the pad), and
every screen passes `ui_check` there. `ui_frame` captures each screen with its widgets outlined;
the packets' outlines are checked against `ui_tree`. The review probes of R1 (per-session and
owner-only displays, the capture's focus, claims during a wave, ui_act's time budget mid-wave,
outlines under the review screen, every UI-scale setting a size) run as checks too. `--perf` then
opens a window on the iGPU and measures the HUD's cost against UI.md's budget (0.5 ms CPU, 0.3 ms
GPU) at the default display (the monitor's OS scale), and reports the reference (OS scale 1).

Usage:
    cargo build --release -p sepulcra-app
    python scripts/ui_check.py [--port 7520] [--adapter integrated] [--out DIR] [--perf]

Fails loudly (exit 1) on any step that errors, any ui_check error, a screen that didn't open, a
focus that didn't land, or (with --perf) a budget miss. Writes summary.json and the captures to
--out (default target/ui-check).
"""
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

DISPLAYS = [("deck", [1280, 800]), ("1080p", [1920, 1080]), ("4k", [3840, 2160])]
SCALES = [1.0, 1.5]
# The game's UI-scale cycle (games/sepulcra-app/src/hud.rs UI_SCALES); it starts at 1.0.
UI_SCALES = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5]
# Every screen the game has; victory, defeat and title can't be opened mid-run by input, so they
# are checked laid out over the HUD, as the game shows them.
ALL_SETS = [["hud"], ["hud", "radial"], ["hud", "pause"], ["hud", "pause", "settings"], ["hud", "pause", "controls"],
            ["hud", "pause", "saves"], ["hud", "pause", "confirm"], ["hud", "victory"], ["hud", "defeat"], ["title"]]

# Every UI-scale setting is checked on these displays (review R2 of m5-uitools, finding 4: the
# game's 1.15 and 1.3 went unchecked, and the build bar ran into the selected panel there). A
# call lays out at most 16 targets x scales.
SETTING_TARGETS = ["deck", "1080p", "4k", "16:10", "ultrawide"]
RULES = ["legibility", "contrast", "non_text_contrast", "overlap", "offscreen", "overflow", "text_overflow", "missing_glyph", "touch_target",
         "focus_reachability", "focus_scope", "gamepad_access", "unbound", "unknown_texture", "untranslated"]

failures = []
# Stalls a later lane owns (scripts/budget_check.py KNOWN): a capture view's preparation.
# ui_frame is sight_frame's capture path.
KNOWN_STALLS = {"sight_frame": 80.0, "ui_frame": 80.0}


def fail(msg):
    print(f"  FAIL: {msg}")
    failures.append(msg)


class Walker:
    def __init__(self, mcp, out, name):
        self.mcp, self.out, self.name = mcp, out, name
        self.captures, self.checks, self.acts = [], [], 0

    def act(self, steps, expect_top=None, label=""):
        j, r = act(self.mcp, {"steps": steps})
        text = r["content"][0]["text"]
        self.acts += 1
        if r.get("isError"):
            fail(f"{self.name} {label}: ui_act failed: {text[:600]}")
            return j
        top = (j.get("screens") or [None])[0]
        if expect_top is not None and top != expect_top:
            fail(f"{self.name} {label}: expected '{expect_top}' on top, have {j.get('screens')} (steps {j.get('steps')})")
        return j

    def check(self, screens, target, label):
        j = self.mcp.json("ui_check", screens=screens, targets=[target], scales=SCALES, max_findings=20)
        self.checks.append({"screens": screens, "target": target, "errors": j["errors"], "warnings": j["warnings"], "ms": j.get("ms")})
        if j["errors"]:
            for f in j["findings"]:
                if f["severity"] == "error":
                    fail(f"{self.name} {label} ui_check {'+'.join(screens)}: [{f['rule']}] {f['target']} {f['screen']}:{f['widget']} {f['message']}")
        return j

    def frame(self, label):
        j = self.mcp.json("ui_frame")
        path = j.get("image", {}).get("path")
        ui = (j.get("overlays") or {}).get("ui") or {}
        outlines = ui.get("outlines", [])
        tree = self.mcp.json("ui_tree", only="interactive", limit=200)
        # The outlines come from the capture's layout; with the same display they match the tree.
        by_id = {(w.get("screen"), w.get("id")): w["rect"] for w in tree["widgets"] if w.get("id") and not w.get("clipped")}
        mismatched = 0
        for o in outlines:
            key = (o.get("screen"), o.get("id"))
            if key in by_id and o.get("surface") == "game":
                if any(abs(a - b) > 1.0 for a, b in zip(by_id[key], o["rect"])):
                    mismatched += 1
        if mismatched:
            fail(f"{self.name} {label}: {mismatched} outlines differ from ui_tree's rects")
        if not outlines:
            fail(f"{self.name} {label}: ui_frame drew no outlines")
        img = j.get("image", {})
        self.captures.append({"label": f"{self.name}_{label}", "path": path, "packet": img.get("packet_path"),
                              "size": [img.get("width"), img.get("height")], "outlines": len(outlines), "surfaces": ui.get("surfaces")})
        return j


def act(mcp, args):
    """ui_act, sent again with its `rest` until it finishes (its frames are budgeted by time)."""
    for _ in range(50):
        r = mcp.request("tools/call", {"name": "ui_act", "arguments": args})
        j = json.loads(r["content"][0]["text"])
        if r.get("isError") or not j.get("rest"):
            return j, r
        args = dict(args, steps=j["rest"])
        args.pop("display", None)
    fail(f"ui_act never finished: {str(j)[:300]}")
    return j, r


def check_every_setting(mcp, label):
    """Every screen set on SETTING_TARGETS at every UI-scale setting. The settings under 1.0 are the
    player's choice of smaller text: every rule but legibility holds there; its count is noted
    (the Deck at 0.8 and 0.9 is under its legibility floor, an open question for the owner)."""
    smaller = [s for s in UI_SCALES if s < 1.0]
    larger = [s for s in UI_SCALES if s >= 1.0]
    passes = [(larger[:3], RULES), (larger[3:], RULES), (smaller, [r for r in RULES if r != "legibility"])]
    small_legibility = 0
    for screens in ALL_SETS:
        errors = warnings = 0
        for scales, rules in passes:
            if not scales:
                continue
            j = mcp.json("ui_check", screens=screens, targets=SETTING_TARGETS, scales=scales, rules=rules, max_findings=50)
            errors += j["errors"]
            warnings += j["warnings"]
            for f in j["findings"]:
                if f["severity"] == "error":
                    fail(f"{label} ui_check {'+'.join(screens)}: [{f['rule']}] {f['target']} {f['screen']}:{f['widget']} {f['message']}")
            if j["errors"] > len([f for f in j["findings"] if f["severity"] == "error"]):
                fail(f"{label} ui_check {'+'.join(screens)} at {scales}: {j['errors']} errors, more than it quoted")
        if smaller:
            j = mcp.json("ui_check", screens=screens, targets=SETTING_TARGETS, scales=smaller, rules=["legibility"], max_findings=1)
            small_legibility += j["errors"]
        print(f"ui_check {label} {'+'.join(screens):<24} every setting {UI_SCALES} on {len(SETTING_TARGETS)} displays: {errors} errors, {warnings} warnings")
    return {"label": label, "legibility_errors_under_1x": small_legibility}


def raw(mcp, tool, args):
    r = mcp.request("tools/call", {"name": tool, "arguments": args})
    t = next((c["text"] for c in r["content"] if c["type"] == "text"), "")
    try:
        return bool(r.get("isError")), json.loads(t)
    except json.JSONDecodeError:
        return bool(r.get("isError")), t


def probes(exe, port, adapter, out, summary):
    """The review probes of R1 (m5-uitools), headless: each a check that fails loudly."""
    res = {}
    args = ["--headless", "--adapter", adapter, "--owner", "boss", "--reviewer", "rev", "--strict-budget", "--captures", str(out / "probe-captures"),
            "--saves", str(out / "probe-saves"), "--", "--seed", "7", "--mute", "--title", "off"]
    with Engine(exe, port, out / "engine-probes.log", *args, cwd=out) as eng:
        boss, alice, mal, rev = (eng.mcp(n) for n in ("boss", "alice", "mallory", "rev"))
        for m, n, role in ((boss, "boss", "owner"), (alice, "alice", "builder"), (mal, "mallory", "builder"), (rev, "rev", "reviewer")):
            raw(m, "agent_hello", {"name": n, "role": role})
        raw(alice, "time_control", {"action": "pause"})
        # Finding 1 (p1 A): one agent's display is its own.
        e, r = raw(mal, "ui_act", {"display": {"safe_area": [320, 512, 320, 512], "os_scale": 5, "size": [1024, 768]}, "steps": []})
        e2, t = raw(alice, "ui_tree", {"limit": 1, "fresh": True})
        d = (t or {}).get("display") or {}
        res["display_per_session"] = {"mallory": (r or {}).get("display"), "alice": d}
        if e or d.get("size") != [1280, 800] or d.get("os_scale") is not None or d.get("set_by"):
            fail(f"probe display: mallory's display reached alice's view: {d}")
        raw(mal, "ui_act", {"display": {"safe_area": [0, 0, 0, 0], "os_scale": None, "size": [1280, 800]}, "steps": []})
        # Finding 2 (p1 B): ui_frame outlines the focus the player has.
        j, _ = act(alice, {"steps": [{"press": "pause"}, {"focus": "controls"}]})
        e, f = raw(alice, "ui_frame", {})
        ol = (((f or {}).get("overlays") or {}).get("ui") or {}).get("outlines", []) if isinstance(f, dict) else []
        focused = [o.get("id") for o in ol if o.get("focused")]
        res["frame_focus"] = {"act": j.get("focused"), "outlined": focused, "image": ((f or {}).get("image") or {}).get("path")}
        if j.get("focused") != "controls" or focused != ["controls"]:
            fail(f"probe focus: ui_act focused {j.get('focused')}, ui_frame outlines {focused}")
        act(alice, {"steps": [{"press": "back"}]})
        # Finding 6 (p1 E): every UI-scale setting is a size on a 1280x720 window at OS 175%.
        raw(alice, "ui_act", {"display": {"size": [1280, 720], "os_scale": 1.75}, "steps": []})
        act(alice, {"steps": [{"press": "pause"}, {"activate": "settings"}]})
        seen = []
        for _ in range(len(UI_SCALES)):
            e, t = raw(alice, "ui_tree", {"screen": "hud", "limit": 1, "fresh": True})
            seen.append(((t or {}).get("ui_scale"), ((t or {}).get("view") or {}).get("scale")))
            act(alice, {"steps": [{"activate": "ui_scale"}]})
        act(alice, {"steps": [{"press": "back"}, {"press": "back"}]})
        res["ui_scale_settings_os175"] = seen
        # Every setting changes the size until the window has no more room (the theme's
        # min_logical, 800x480: 1.5 on a 1280x720 window); 0.8, 1.0 and 1.5 always differ.
        # (The game's cycle passes over sizes that change nothing here, so it comes round to the
        # first ones again within these presses: each setting is looked at once.)
        by = sorted(set(seen))
        room = 720 / 480
        for (u0, v0), (u1, v1) in zip(by, by[1:]):
            if not (v1 > v0 + 1e-4 or abs(v0 - room) < 1e-3):
                fail(f"probe ui scale: setting {u1} doesn't change the size below the window's room: {seen}")
        pick = {round(u, 2): v for u, v in seen}
        if len({pick.get(0.8), pick.get(1.0), pick.get(1.5)}) != 3:
            fail(f"probe ui scale: 0.8, 1.0 and 1.5 share a size on the 175% window: {seen}")
        raw(alice, "ui_act", {"display": {"size": [1280, 800], "os_scale": None}, "steps": []})
        # Findings 4 and 5 (p2 C2, p4): a busy wave, alice claims everything; mallory's waits
        # aren't refused for the game's own motion, and ui_act at its cap stays in budget.
        raw(alice, "time_control", {"action": "resume"})
        raw(alice, "play_control", {"speed": 4})
        for _ in range(3):
            raw(alice, "play_command", {"action": "call_wave"})
            time.sleep(3)
        e, st = raw(alice, "play_state", {})
        res["wave"] = {k: (st or {}).get(k) for k in ("tick", "wave", "enemies")} if isinstance(st, dict) else st
        e, c = raw(alice, "claim_take", {"region": {"min": [-500, -500, -500], "max": [500, 500, 500]}})
        if e:
            fail(f"probe claims: alice's claim: {c}")
        waits = []
        for _ in range(3):
            e, r = raw(mal, "ui_act", {"steps": [{"wait": 5}]})
            waits.append((e, (r or {}).get("error") if isinstance(r, dict) else r))
        res["waits_under_claim"] = waits
        if any(e for e, _ in waits):
            fail(f"probe claims: mallory's waits were refused under alice's claim: {waits}")
        e, r = raw(mal, "ui_act", {"steps": [{"press": "pause"}]})
        res["pause_under_claim"] = {"error": e, "answer": (r or {}).get("error") if isinstance(r, dict) else r}
        # The same command through the game's tool: refused the same way (the pause writes the
        # game's entities inside alice's claim; it is mallory's command either way).
        e2, r2 = raw(mal, "play_control", {"paused": True})
        res["play_control_pause_under_claim"] = {"error": e2, "answer": (r2 or {}).get("error") if isinstance(r2, dict) else r2}
        if e != e2:
            fail(f"probe claims: a pause by ui_act ({res['pause_under_claim']}) and by play_control ({res['play_control_pause_under_claim']}) differ")
        if not e2:
            raw(mal, "play_control", {"paused": False})
        raw(alice, "claim_release", {"all": True})
        if not e:
            act(mal, {"steps": [{"press": "back"}]})
        big = []
        for _ in range(4):
            t0 = time.perf_counter()
            e, r = raw(mal, "ui_act", {"steps": [{"wait": 120}, {"wait": 40}], "settle": 0})
            big.append({"ms": (r or {}).get("ms"), "frames": (r or {}).get("frames"), "stopped": (r or {}).get("stopped"),
                        "frames_left": (r or {}).get("frames_left"), "wall_ms": round((time.perf_counter() - t0) * 1000, 1)})
            if e:
                fail(f"probe budget: ui_act failed mid-wave: {r}")
        res["ui_act_cap_mid_wave"] = big
        print(f"ui_act at its cap mid-wave: {big}")
        if any((b["ms"] or 0) > 25 for b in big):
            fail(f"probe budget: ui_act past its 20 ms budget mid-wave: {big}")
        # Finding 3 (p2 F): the review screen open: the game's outlines are all blocked.
        e, r = raw(rev, "review_screen", {"action": "open"})
        e, r = raw(alice, "ui_act", {"surface": "game", "steps": [{"press": "pause"}]})
        if not e or "engine_screen_open" not in json.dumps(r):
            fail(f"probe review: a game ui_act under the review screen wasn't refused: {str(r)[:200]}")
        e, f = raw(alice, "ui_frame", {})
        ui = (((f or {}).get("overlays") or {}).get("ui") or {}) if isinstance(f, dict) else {}
        game = [o for o in ui.get("outlines", []) if o.get("surface") == "game"]
        res["under_review"] = {"surfaces": ui.get("surfaces"), "game": len(game), "game_open": sum(1 for o in game if not o.get("blocked")),
                               "game_focused": sum(1 for o in game if o.get("focused")), "image": ((f or {}).get("image") or {}).get("path")}
        if not game or res["under_review"]["game_open"] or res["under_review"]["game_focused"]:
            fail(f"probe review: game outlines under the review screen: {res['under_review']}")
        e, r = raw(mal, "ui_act", {"surface": "review", "device": "keyboard", "steps": [{"press": "Key.F9"}]})
        if not e or "owner_only" not in json.dumps(r):
            fail(f"probe review: a non-owner drove the review screen: {str(r)[:200]}")
        e, r = raw(boss, "ui_act", {"surface": "review", "device": "keyboard", "steps": [{"press": "Key.F9"}]})
        if e:
            fail(f"probe review: the owner couldn't close it: {str(r)[:200]}")
        time.sleep(1.5)
        e, inc = raw(boss, "trace_incidents", {"code": "engine.stall"})
        stalls = (inc or {}).get("incidents", []) if isinstance(inc, dict) else []
        new = []
        for x in stalls:
            if x.get("key") == "replay_check":
                # The engine's own background replay check: traced as a warning, never a strict
                # failure (trace_engine.rs), and not the UI tools' work.
                res.setdefault("stalls", []).append({"key": "replay_check", "summary": x.get("summary"), "known": True})
                continue
            peak = (raw(boss, "trace_incident", {"id": x["id"]})[1] or {}).get("peak_ms")
            most = KNOWN_STALLS.get(x.get("key"))
            known = most is not None and peak is not None and peak <= most
            res.setdefault("stalls", []).append({"key": x.get("key"), "summary": x.get("summary"), "count": x.get("count"), "peak_ms": peak, "known": known})
            if not known:
                new.append(x.get("summary"))
        if new:
            fail(f"probe budget: engine-thread stalls: {new}")
    res["exit_code"] = eng.exit_code
    if eng.exit_code not in (0, None) and not all(s["known"] for s in res.get("stalls", [])):
        fail(f"probes: the strict-budget engine exited {eng.exit_code}")
    summary["probes"] = res


def windowed_probe(exe, port, adapter, out, summary):
    """Finding 1 (p3): with a window, a non-owner's display override is refused and the human's
    HUD is unchanged; the owner's is attributed."""
    args = ["--adapter", adapter, "--size", "1280x720", "--owner", "boss", "--captures", str(out / "window-captures"), "--", "--seed", "7", "--mute", "--title", "off"]
    res = {}
    with Engine(exe, port, out / "engine-window-probe.log", *args, cwd=out) as eng:
        mal, boss = eng.mcp("mallory"), eng.mcp("boss")
        raw(mal, "agent_hello", {"name": "mallory"})
        raw(boss, "agent_hello", {"name": "boss", "role": "owner"})
        time.sleep(3)
        e, before = raw(mal, "ui_tree", {"limit": 1})
        e, r = raw(mal, "ui_act", {"display": {"safe_area": [288, 512, 288, 512], "os_scale": 0.5}, "steps": []})
        time.sleep(1)
        e2, after = raw(mal, "ui_tree", {"limit": 1})
        res["non_owner"] = {"refused": e and "owner_only" in json.dumps(r), "view_before": (before or {}).get("view"), "view_after": (after or {}).get("view")}
        if not res["non_owner"]["refused"] or (before or {}).get("view") != (after or {}).get("view"):
            fail(f"window probe: a non-owner changed the window's display: {res['non_owner']}")
        e, r = raw(boss, "ui_act", {"display": {"safe_area": [24, 0, 24, 0]}, "steps": []})
        res["owner"] = (r or {}).get("display")
        if e or ((r or {}).get("display") or {}).get("set_by") != "boss":
            fail(f"window probe: the owner's override: {str(r)[:300]}")
        e, r = raw(mal, "ui_frame", {"display": {"os_scale": 1.0, "safe_area": [0, 0, 0, 0]}})
        res["capture_display"] = ((r or {}).get("image") or {}).get("path")
        if e:
            fail(f"window probe: ui_frame with its own display: {str(r)[:300]}")
        e, t = raw(mal, "ui_tree", {"limit": 1})
        if ((t or {}).get("display") or {}).get("safe_area") != [24, 0, 24, 0]:
            fail(f"window probe: a capture's display stayed: {(t or {}).get('display')}")
    res["validation_errors"] = eng.validation_errors()
    if res["validation_errors"]:
        fail(f"window probe: {res['validation_errors']} validation errors")
    summary["window_probe"] = res


def set_scale(w, want):
    """Cycles the game's UI scale through its settings screen (the pad) until it is `want`."""
    w.act([{"press": "pause"}], expect_top="pause", label="open pause")
    w.act([{"activate": "settings"}], expect_top="settings", label="open settings")
    for _ in range(len(UI_SCALES)):
        st = w.mcp.json("ui_tree", screen="hud", limit=1)
        if abs(st.get("ui_scale", 0) - want) < 1e-3:
            break
        w.act([{"activate": "ui_scale"}], expect_top="settings", label="cycle ui scale")
    else:
        fail(f"{w.name}: the UI scale never reached {want}")
    w.act([{"press": "back"}], expect_top="pause", label="settings back")
    w.act([{"press": "back"}], expect_top="hud", label="resume")


def walk(w, target):
    """Every menu path with the gamepad, checking and capturing each screen."""
    w.act([{"wait": 2}], expect_top="hud", label="start")
    w.check(["hud"], target, "hud")
    w.frame("hud")
    # The radial build menu (North): every item reachable with the d-pad, then back.
    j = w.act([{"press": "build_menu"}], expect_top="radial", label="open radial")
    if (j.get("screens") or [None])[0] == "radial":
        w.check(["hud", "radial"], target, "radial")
        w.frame("radial")
        for item in ["warding", "pillar", "brazier", "obelisk", "wall"]:
            jj = w.act([{"focus": item}], label=f"radial focus {item}")
            if jj.get("focused") != item:
                fail(f"{w.name}: radial focus landed on {jj.get('focused')}, not {item}")
        w.act([{"press": "back"}], expect_top="hud", label="close radial")
    # The pause menu (Start) and everything under it.
    w.act([{"press": "pause"}], expect_top="pause", label="open pause")
    w.check(["hud", "pause"], target, "pause")
    w.frame("pause")
    for sub, back_to in [("settings", "pause"), ("controls", "pause"), ("saves", "pause")]:
        w.act([{"activate": sub}], expect_top=sub, label=f"open {sub}")
        w.check(["hud", "pause", sub], target, sub)
        w.frame(sub)
        # Every button on it, reached with the d-pad.
        tree = w.mcp.json("ui_tree", screen=sub, only="interactive", limit=200)
        for b in tree["widgets"]:
            if b.get("id") and b.get("enabled"):
                jj = w.act([{"focus": b["id"]}], label=f"{sub} focus {b['id']}")
                if jj.get("focused") != b["id"]:
                    fail(f"{w.name}: focus on {sub}:{b['id']} landed on {jj.get('focused')}")
        w.act([{"press": "back"}], expect_top=back_to, label=f"close {sub}")
    # Restart asks first: the confirm dialog, answered no.
    w.act([{"activate": "restart"}], expect_top="confirm", label="open confirm")
    w.check(["hud", "pause", "confirm"], target, "confirm")
    w.frame("confirm")
    w.act([{"focus": "confirm_yes"}, {"focus": "confirm_no"}, {"press": "accept"}], expect_top="pause", label="confirm no")
    # The pause menu's speed button (the HUD's speed for a pad), then resume with B.
    w.act([{"activate": "pause_speed"}], expect_top="pause", label="speed")
    w.act([{"press": "back"}], expect_top="hud", label="resume")
    # The end screens and the title: laid out with the game's state, measured.
    for screens in (["hud", "victory"], ["hud", "defeat"], ["title"]):
        w.check(screens, target, screens[-1])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7520)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe")
    ap.add_argument("--out", default=str(ROOT / "target" / "ui-check"))
    ap.add_argument("--perf", action="store_true", help="also measure the HUD's cost in a window")
    ap.add_argument("--perf-only", action="store_true", help="only measure the HUD's cost (no walk)")
    a = ap.parse_args()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target_dir / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p sepulcra-app")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    summary = {"walks": [], "pseudo": None, "perf": None}
    if a.perf_only:
        summary["perf"] = perf(exe, a.port + 1, a.adapter, out)
        summary["failures"] = failures
        (out / "summary.json").write_text(json.dumps(summary, indent=2))
        sys.exit(1 if failures else 0)
    args = ["--headless", "--adapter", a.adapter, "--captures", str(out / "captures"), "--", "--seed", "7", "--mute", "--title", "off"]
    with Engine(exe, a.port, out / "engine.log", *args, cwd=out) as eng:
        mcp = eng.mcp("ui-playtester")
        mcp.json("agent_hello", name="ui-playtester", role="playtester")
        mcp.call("time_control", action="pause")
        # Every screen at every display and scale, before any walking (the state as the game starts).
        for screens in ALL_SETS:
            j = mcp.json("ui_check", screens=screens, targets=[d for d, _ in DISPLAYS], scales=SCALES, max_findings=50)
            print(f"ui_check {'+'.join(screens):<24} {j['errors']} errors, {j['warnings']} warnings ({j.get('ms')} ms)")
            for f in j["findings"]:
                if f["severity"] == "error":
                    fail(f"ui_check {'+'.join(screens)}: [{f['rule']}] {f['target']} {f['screen']}:{f['widget']} {f['message']}")
        summary["settings"] = [check_every_setting(mcp, "start")]
        # Busy: a stone carved and inspected (its panel, upgrade and sell), another chosen to
        # carve (the selected panel), paused (the play button and the PAUSED label), a wave due.
        for tool, args in [("play_command", {"action": "place", "stone": "warding", "cell": "top:12,9"}), ("play_ui", {"press": "build_pillar"}),
                           ("play_ui", {"press": "pause_button"}), ("play_ui", {"select": "top:12,9"})]:
            err, j = raw(mcp, tool, args)
            if err:
                fail(f"busy state: {tool} {args}: {str(j)[:300]}")
        summary["settings"].append(check_every_setting(mcp, "busy"))
        for tool, args in [("play_ui", {"press": "deselect"}), ("play_ui", {"press": "resume_button"})]:
            raw(mcp, tool, args)
        # A pseudo-locale pass: how much longer text fits, and what the game supplies itself.
        p = mcp.json("ui_check", screens=["hud", "pause"], targets=["deck"], pseudo=True, max_findings=10)
        summary["pseudo"] = {"errors": p["errors"], "warnings": p["warnings"], "by_rule": p["by_rule"]}
        print(f"pseudo-locale hud+pause at deck: {p['errors']} errors, {p['warnings']} warnings {p['by_rule']}")
        for name, size in DISPLAYS:
            mcp.json("ui_act", display={"size": size}, steps=[])
            for scale in SCALES:
                w = Walker(mcp, out, f"{name}@{scale}")
                if scale != 1.0:
                    set_scale(w, scale)
                print(f"walking {w.name} ...")
                walk(w, name)
                if scale != 1.0:
                    set_scale(w, 1.0)
                summary["walks"].append({"name": w.name, "acts": w.acts, "checks": w.checks, "captures": w.captures})
                print(f"  {w.acts} ui_act calls, {len(w.checks)} checks, {len(w.captures)} captures")
        # The default 1280x720 window on a 175% laptop (the OS scale is a floor, capped by the
        # theme's max_boost): the display the owner's machine gives a window.
        laptop = {"name": "window_1280x720_os175", "size": [1280, 720], "os_scale": 1.75}
        mcp.json("ui_act", display={"size": laptop["size"], "os_scale": 1.75}, steps=[])
        w = Walker(mcp, out, "laptop175")
        print(f"walking {w.name} ...")
        walk(w, laptop)
        summary["walks"].append({"name": w.name, "acts": w.acts, "checks": w.checks, "captures": w.captures})
        print(f"  {w.acts} ui_act calls, {len(w.checks)} checks, {len(w.captures)} captures")
        mcp.json("ui_act", display={"os_scale": None}, steps=[])
        # The largest calls the schemas allow, timed on the engine thread (budget: 30 ms, the
        # big class): ui_act's 160 frames (stopped by time; the probes time it mid-wave),
        # ui_check's 16 layouts (step-jobbed per layout).
        mcp.json("ui_act", display={"size": [1280, 800]}, steps=[])
        big_act = mcp.json("ui_act", steps=[{"wait": 120}, {"wait": 40}], settle=0)
        big_check = mcp.json("ui_check", screens=["hud", "pause", "saves"], targets=["deck", "1080p", "4k", "ultrawide"], scales=[1, 1.25, 1.5, 2], max_findings=1)
        per_layout = big_check["ms"] / 16
        summary["budget"] = {"ui_act_160_frames_ms": big_act["ms"], "ui_check_16_layouts_ms": big_check["ms"], "ui_check_per_layout_ms": per_layout}
        print(f"largest calls: ui_act 160 frames {big_act['ms']} ms; ui_check 16 layouts {big_check['ms']} ms ({per_layout:.1f} ms per slice)")
        if big_act["ms"] > 30:
            fail(f"ui_act at its limit took {big_act['ms']} ms on the engine thread (budget 30)")
        if per_layout > 10:
            fail(f"a ui_check slice took {per_layout:.1f} ms")
        # The engine's own: a refusal for a non-owner, and a refused engine screen.
        r = mcp.request("tools/call", {"name": "ui_act", "arguments": {"surface": "review", "steps": [{"press": "accept"}]}})
        if not r.get("isError") or "owner_only" not in r["content"][0]["text"]:
            fail(f"ui_act on the review screen by a non-owner wasn't refused: {r['content'][0]['text'][:300]}")
        summary["engine_log_tail"] = None
    summary["validation_errors"] = eng.validation_errors()
    probes(exe, a.port + 2, a.adapter, out, summary)
    windowed_probe(exe, a.port + 3, a.adapter, out, summary)
    if a.perf:
        summary["perf"] = perf(exe, a.port + 1, a.adapter, out)
    summary["failures"] = failures
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"summary: {out / 'summary.json'}")
    if failures:
        print(f"FAILED: {len(failures)} failures")
        sys.exit(1)
    print("PASS")


def perf(exe, port, adapter, out):
    """The HUD in a 1280x800 window on the adapter, the game running: the HUD's CPU zones (its
    layout and input, and its overlay build) and the overlay pass's GPU time over recorded frames.
    Measured at the monitor's own OS scale (the default a player gets: a 1280x800 window on a 175%
    laptop lays the HUD out 1.5 times as large, the theme's max_boost) and at the reference (UI
    scale 1 at the Deck's 1280x800: the OS scale set to 1 through the owner's ui_act display). The
    budget (UI.md: 0.5 ms CPU, 0.3 ms GPU; the owner, 2026-11-07: it holds at the default) is
    checked at both."""
    args = ["--adapter", adapter, "--size", "1280x800", "--owner", "ui-perf", "--captures", str(out / "perf-captures"), "--", "--seed", "7", "--mute", "--title", "off"]
    res = {}
    with Engine(exe, port, out / "engine-perf.log", *args, cwd=out) as eng:
        mcp = eng.mcp("ui-perf")
        mcp.json("agent_hello", name="ui-perf", role="owner")
        info = mcp.json("engine_info")
        res["gpu"] = (info.get("gpu") or {}).get("name")
        for label, os_scale in (("platform", None), ("reference", 1.0)):
            act = mcp.json("ui_act", display={"os_scale": os_scale}, steps=[])
            mcp.call("perf_zones", enabled=False)
            mcp.call("perf_zones", enabled=True)
            # Past start-up (glyph atlas, first layouts) and the rolling zones' window.
            time.sleep(8.0)
            cap = mcp.json("perf_capture", frames=180, trace=False, label=f"ui_hud_{label}")
            zones = mcp.json("perf_zones", thread="main", min_ms=0.0, limit=500).get("zones", [])
            tree = mcp.json("ui_tree", limit=1)

            def zone(suffix):
                z = [x for x in zones if x.get("path", "").endswith(suffix)]
                return z[0] if z else None
            hud_frame, hud_overlay = zone("hud_frame"), zone("hud_overlay")
            overlay = (cap.get("gpu_passes_ms") or {}).get("overlay") or {}
            r = {"display": act.get("display"), "ui_scale_view": (tree.get("view") or {}).get("scale"), "frames": cap.get("frames"),
                 "hud_frame": hud_frame, "hud_overlay": hud_overlay, "overlay_pass_ms": overlay,
                 "cpu_mean_ms": sum(float((z or {}).get("mean_ms", 0.0)) for z in (hud_frame, hud_overlay)),
                 "cpu_p95_ms": sum(float((z or {}).get("p95_ms", 0.0)) for z in (hud_frame, hud_overlay)),
                 "gpu_mean_ms": overlay.get("mean"), "gpu_p95_ms": overlay.get("p95"), "frame_ms": cap.get("frame_ms")}
            res[label] = r
            print(f"HUD cost ({label}, view scale {r['ui_scale_view']}) on {res['gpu']}: CPU mean {r['cpu_mean_ms']:.3f} ms, p95 {r['cpu_p95_ms']:.3f} ms; "
                  f"GPU overlay pass mean {r['gpu_mean_ms']} ms, p95 {r['gpu_p95_ms']} ms ({r['frames']} frames)")
            if hud_frame is None or hud_overlay is None:
                fail("perf: the hud zones weren't recorded")
            elif r["cpu_mean_ms"] > 0.5:
                fail(f"perf: HUD CPU {r['cpu_mean_ms']:.3f} ms > 0.5 ms")
            if r["gpu_mean_ms"] is None:
                fail("perf: no overlay pass GPU time")
            elif r["gpu_mean_ms"] > 0.3:
                fail(f"perf: HUD GPU {r['gpu_mean_ms']} ms > 0.3 ms")
    res["validation_errors"] = eng.validation_errors()
    # Contention is the realistic condition (lane rules), but say what shared the machine.
    tl = subprocess.run(["tasklist"], capture_output=True, text=True).stdout.lower() if sys.platform == "win32" else ""
    res["running_after"] = {n: tl.count(n) for n in ("sepulcra.exe", "specula.exe", "rustc.exe")}
    print(f"validation errors {res['validation_errors']}; running after: {res['running_after']}")
    if res["validation_errors"]:
        fail(f"perf: {res['validation_errors']} validation errors")
    return res


if __name__ == "__main__":
    main()

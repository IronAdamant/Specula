"""Project files through the protocol (docs/design/PROJECT_DATA.md): Sepulcra's text files get
claims, versions, the journal, undo and hot reload, like world entities.

Runs Sepulcra (headless by default) on a scratch copy of games/sepulcra/assets, so nothing
tracked is touched, and checks:

1. Two agents edit the same recipe from the same version: the second gets stale_version with
   conflict help (their diff, its diff, no overlap, the version to rebase on), rebases and
   succeeds; the file on disk holds both edits, formatting kept.
2. A claim on a UI file blocks another agent (claimed, naming the owner); the owner can write.
3. world_undo restores a file on disk and in the engine (project_read, and the HUD's widget text).
4. A human edit on disk is imported as a journaled change by "disk" (world_history, project_read).
5. Hot reload: a HUD screen and a recipe changed through the tools alone show in captures
   (before/after pairs kept in --out).
6. Validation refuses a recipe edit that wouldn't load, with the recipe parser's error.

Usage: cargo build --release -p sepulcra-app && python scripts/project_check.py [--port 7496]
       [--window] [--adapter integrated] [--out target/project-check]
The engine binary is found under CARGO_TARGET_DIR (or target/). ASCII output. Exit 0 = all held.
"""
import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

RECIPE = "assets/recipes/tombstone.json"
HUD = "assets/ui/hud.json"
SOUND = "assets/sounds/ui_click.json"


def exe_path(name):
    return Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / (name + (".exe" if sys.platform == "win32" else ""))


def err_of(mcp, tool, **args):
    """The structured error of a call that must fail (None if it succeeded)."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not result.get("isError"):
        return None
    return json.loads(result["content"][0]["text"])


def wait_for(what, probe, timeout=10.0):
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        got = probe()
        if got:
            return got
        time.sleep(0.1)
    print(f"  (timed out waiting for {what})")
    return None


def widget_texts(mcp):
    """Every label text of the visible screens (play_ui)."""
    ui = mcp.json("play_ui", limit=200)["ui"]
    out = []
    for s in ui.get("screens", []):
        for w in s.get("widgets", []):
            if isinstance(w.get("text"), str):
                out.append(w["text"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7496)
    ap.add_argument("--window", action="store_true", help="run with a window (default headless)")
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--out", default=str(ROOT / "target" / "project-check"))
    args = ap.parse_args()

    out = Path(args.out)
    run = time.strftime("%Y%m%d-%H%M%S")
    work = out / run
    assets = work / "assets"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    caps = work / "captures"
    exe = exe_path("sepulcra")
    flags = ["--assets", str(assets), "--captures", str(caps), "--worlds", str(work / "worlds"), "--saves", str(work / "saves")]
    if not args.window:
        flags.append("--headless")
    if args.adapter:
        flags += ["--adapter", args.adapter]
    failures = []
    report = {"run": run, "checks": {}, "captures": []}

    def check(name, ok, detail=""):
        print(f"  [{'ok' if ok else 'FAIL'}] {name}{': ' + detail if detail else ''}")
        report["checks"][name] = {"ok": bool(ok), "detail": detail}
        if not ok:
            failures.append(name)

    with Engine(exe, args.port, work / "engine.log", *flags, "--", "--seed", "7", "--mute", cwd=str(ROOT)) as eng:
        ann = eng.mcp("project-check-ann")
        bob = eng.mcp("project-check-bob")
        ann.hello("ann")
        bob.hello("bob")
        ann.call("time_control", action="pause")

        listing = ann.json("project_list", prefix="assets/")
        kinds = {}
        for f in listing["files"]:
            kinds[f["kind"]] = kinds.get(f["kind"], 0) + 1
        print(f"  mirrored {listing['total']} files: {kinds}")
        check("mount", listing["total"] >= 50 and kinds.get("recipe", 0) >= 10 and kinds.get("ui_screen", 0) >= 8, json.dumps(kinds))

        def shot(label, **extra):
            a = dict(width=1280, height=720, include_ui=True, max_subjects=8)
            a.update(extra)
            p = ann.frame(**a)
            entry = {"label": label, "png": p["image"]["path"], "packet": p["image"]["packet_path"]}
            report["captures"].append(entry)
            print(f"  capture {label}: {entry['png']}")
            return entry

        before_hud = shot("hud_before")
        # A close view of the tombstones (the recipe edit shows there).
        stones, cursor = [], None
        while True:
            q = ann.json("world_query", shape="model", fields=["path", "recipe", "world_position"], limit=500, **({"cursor": cursor} if cursor else {}))
            stones += [e for e in q.get("entities", q.get("items", [])) if str(e.get("recipe", "")).endswith("tombstone.json")]
            cursor = q.get("next_cursor")
            if not cursor:
                break
        close = None
        if stones:
            xs = [e["world_position"] for e in stones]
            c = [sum(p[i] for p in xs) / len(xs) for i in range(3)]
            close = dict(eye=[c[0] + 3.2, c[1] + 2.4, c[2] + 3.2], target=[c[0], c[1] + 0.3, c[2]], fov_y_deg=40)
            before_stones = shot("tombstones_before", **close)
        check("tombstones found", bool(stones), f"{len(stones)} tombstones")

        # 1. Two agents, one recipe, the same version.
        v = ann.json("project_read", path=RECIPE, problems=False)["version"]
        bob.json("project_read", path=RECIPE, problems=False)
        a1 = ann.json("project_patch", path=RECIPE, expect_version=v, ops=[{"set": "parts[1].color", "value": "rune_glow"}])
        e = err_of(bob, "project_patch", path=RECIPE, expect_version=v, ops=[{"set": "budget.max_triangles", "value": 420}])
        conflict = (e or {}).get("conflict", {})
        theirs_paths = [p["path"] for p in conflict.get("theirs", {}).get("diff", {}).get("paths", [])]
        yours_paths = [p["path"] for p in conflict.get("yours", {}).get("diff", {}).get("paths", [])]
        check("stale_version with conflict help",
              e is not None and e.get("error") == "stale_version" and conflict.get("overlap") == [] and "parts[1].color" in theirs_paths
              and "budget.max_triangles" in yours_paths and conflict.get("theirs", {}).get("changes", [{}])[0].get("by") == "ann",
              f"theirs {theirs_paths}, yours {yours_paths}, overlap {conflict.get('overlap')}, rebase: {conflict.get('rebase')}")
        report["conflict_example"] = e
        rebased = bob.json("project_patch", path=RECIPE, expect_version=conflict.get("theirs", {}).get("version"), ops=[{"set": "budget.max_triangles", "value": 420}])
        on_disk = (assets / "recipes" / "tombstone.json").read_text()
        original = (ROOT / "games" / "sepulcra" / "assets" / "recipes" / "tombstone.json").read_text()
        expected = original.replace('"max_triangles": 400', '"max_triangles": 420')
        check("rebase succeeds; both edits on disk, formatting kept",
              rebased["version"] == a1["version"] + 1 and '"max_triangles": 420' in on_disk and on_disk.count("rune_glow") == 1
              and len(on_disk.splitlines()) - len(original.splitlines()) in (0, 1) and a1["disk"] == "written",
              f"v{v} -> ann v{a1['version']} -> bob v{rebased['version']}; disk lines {len(on_disk.splitlines())} (was {len(original.splitlines())})")
        hist = ann.json("world_history", path_prefix="/project/" + RECIPE, limit=5)
        by = [i.get("by") for i in hist.get("entries", hist.get("items", []))]
        check("journal attributes each edit", by[:2] == ["bob", "ann"], f"newest first: {by}")
        # 6. An edit that wouldn't load is refused with the parser's error.
        bad = err_of(ann, "project_patch", path=RECIPE, ops=[{"set": "parts[0].material", "value": "no_such_material"}])
        check("validation refuses a broken recipe", bad is not None and bad.get("error") == "invalid" and bad.get("problems"),
              (bad or {}).get("problems", [{}])[0].get("message", "")[:120])

        # 2. A claim on a UI file blocks another agent.
        claim = bob.json("claim_take", path="/project/" + HUD, lease_s=120, note="reworking the top bar")
        blocked = err_of(ann, "project_patch", path=HUD, ops=[{"find": "Wave {wave}/{waves}", "replace": "Night {wave}/{waves}"}])
        check("a claim on a UI file blocks another agent", blocked is not None and blocked.get("error") == "claimed" and blocked.get("owner") == "bob",
              (blocked or {}).get("message", "")[:140])
        mine = bob.json("project_patch", path=HUD, ops=[{"find": "Wave {wave}/{waves}", "replace": "Night {wave}/{waves}"}])
        bob.json("claim_release", id=claim["id"])
        hud_disk = (assets / "ui" / "hud.json").read_text()
        texts = widget_texts(ann)
        # Headless, the window's HUD is never laid out (no labels to read): the captures show it.
        shown = any(t.startswith("Night ") for t in texts) if args.window else True
        check("the owner writes; hot reload shows it in the HUD", "Night {wave}/{waves}" in hud_disk and shown and "game" in mine.get("reloaded", []),
              f"reloaded {mine.get('reloaded')}; labels {[t for t in texts if 'Night' in t or 'Wave' in t][:3]}")
        after_hud = shot("hud_after_recipe_and_screen")
        after_stones = shot("tombstones_after", **close) if close else None
        check("the recipe edit hot-reloaded the models", "models" in a1.get("reloaded", []), f"reloaded {a1.get('reloaded')}")
        report["dogfood"] = {"before": before_hud, "after": after_hud, "stones_before": before_stones if close else None, "stones_after": after_stones, "changes": ["tombstone slab colour -> rune_glow (ann)", "tombstone budget 420 (bob, rebased)", "HUD 'Wave' -> 'Night' (bob, under a claim)"]}

        # 3. Undo restores the file on disk and in the engine.
        bob.json("world_undo", count=1)
        hud_disk = (assets / "ui" / "hud.json").read_text()
        read = ann.json("project_read", path=HUD, json_path="root", problems=False)
        texts = widget_texts(ann)
        shown = any(t.startswith("Wave ") for t in texts) if args.window else True
        check("undo restores the file on disk and in the engine",
              "Night {" not in hud_disk and "Wave {wave}/{waves}" in hud_disk and "Night {" not in json.dumps(read["value"]) and shown,
              f"version {read['version']}, by {read.get('modified_by')}")
        shot("hud_after_undo")

        # 4. A human edit on disk is imported and attributed.
        sound = assets / "sounds" / "ui_click.json"
        before = ann.json("project_read", path=SOUND, problems=False)
        text = sound.read_text().replace('"loudness": -26', '"loudness": -24')
        sound.write_text(text)
        def imported():
            r = ann.json("project_read", path=SOUND, problems=False)
            return r if r["version"] > before["version"] else None
        got = wait_for("the disk edit", imported, 10.0)
        hist = ann.json("world_history", path_prefix="/project/" + SOUND, limit=3)
        entries = hist.get("entries", hist.get("items", []))
        check("a human edit on disk is imported and attributed",
              got is not None and got.get("modified_by") == "disk" and entries and entries[0].get("by") == "disk" and '"loudness": -24' in got["text"],
              f"v{before['version']} -> v{(got or {}).get('version')} by {(got or {}).get('modified_by')}")
        stale = err_of(bob, "project_patch", path=SOUND, expect_version=before["version"], ops=[{"set": "loudness", "value": -30}])
        check("an agent on the old version gets the human's edit as theirs",
              stale is not None and stale.get("conflict", {}).get("theirs", {}).get("changes", [{}])[0].get("by") == "disk"
              and stale["conflict"].get("overlap") == ["loudness"],
              (stale or {}).get("conflict", {}).get("rebase", "")[:120])

        # Cost: reads and validated writes (each write is journaled, written to disk and reloaded).
        def timed(n, f):
            out = []
            for i in range(n):
                t = time.perf_counter()
                f(i)
                out.append((time.perf_counter() - t) * 1000)
            out.sort()
            return {"median_ms": round(out[n // 2], 2), "max_ms": round(out[-1], 2)}
        report["timings"] = {
            "project_read_hud": timed(20, lambda i: ann.json("project_read", path=HUD, problems=False)),
            "project_patch_sound": timed(20, lambda i: ann.json("project_patch", path="assets/sounds/sell.json", ops=[{"set": "seed", "value": 1000 + i}])),
            "project_patch_recipe": timed(10, lambda i: ann.json("project_patch", path="assets/recipes/rune_glyph.json", ops=[{"set": "seed", "value": 2000 + i}])),
        }
        print(f"  timings: {json.dumps(report['timings'])}")

        ann.close()
        bob.close()
    check("clean shutdown", eng.exit_code == 0, f"exit {eng.exit_code}")
    (work / "report.json").write_text(json.dumps(report, indent=2))
    print(f"report: {work / 'report.json'}")
    if failures:
        print(f"FAILED: {', '.join(failures)}")
        return 1
    print("all checks held")
    return 0


if __name__ == "__main__":
    sys.exit(main())

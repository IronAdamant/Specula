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
import subprocess
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


def module_workspace(ws):
    """A small Cargo workspace holding copies of specula-core, specula-sdk and ambulo (as
    mods/ambulo), so fix builds edit copies, never the repo's own module."""
    import re
    ignore = shutil.ignore_patterns("target")
    for c in ("specula-core", "specula-sdk"):
        shutil.copytree(ROOT / "crates" / c, ws / "crates" / c, ignore=ignore)
    shutil.copytree(ROOT / "games" / "ambulo", ws / "mods" / "ambulo", ignore=ignore)
    shutil.copytree(ROOT / ".cargo", ws / ".cargo")
    manifest = (ROOT / "Cargo.toml").read_text()
    manifest = re.sub(r"members = \[.*?\]", 'members = ["crates/specula-core", "crates/specula-sdk", "mods/*"]', manifest, flags=re.S)
    deps = re.search(r"\[workspace\.dependencies\]\n(.*?)\n\n", manifest, flags=re.S)
    if deps:
        kept = "\n".join(l for l in deps.group(1).splitlines() if l.startswith(("specula-core ", "specula-sdk ")) or not l.startswith("specula"))
        manifest = manifest.replace(deps.group(1), kept)
    (ws / "Cargo.toml").write_text(manifest)
    (ws / "emptyassets").mkdir()


def fix_checks(args, work, check):
    """logic_build {fix: true} on a mounted module (review round 2): the fix is the caller's
    journaled change (never `disk`), claims stop it, and a file another agent changes during the
    build keeps their text."""
    ws = work / "ws"
    module_workspace(ws)
    lib = ws / "mods" / "ambulo" / "src" / "lib.rs"
    orig = lib.read_text(encoding="utf-8")
    warn = orig + "\npub fn review_probe() -> i32 {\n    let mut x = 1;\n    x\n}\n"
    P = "modules/ambulo/src/lib.rs"
    exe = exe_path("sepulcra")
    env_target = os.environ.get("CARGO_TARGET_DIR")
    os.environ["CARGO_TARGET_DIR"] = str(work / "wstarget")
    try:
        with Engine(exe, args.port + 1, work / "engine-fix.log", "--headless", "--assets", str(ws / "emptyassets"), "--captures", str(work / "caps-fix"), "--worlds", str(work / "worlds-fix"), "--saves", str(work / "saves-fix"), "--", "--mute", cwd=str(ws)) as eng:
            ann = eng.mcp("fix-ann")
            bob = eng.mcp("fix-bob")
            ann.hello("ann")
            bob.hello("bob")
            m = ann.json("project_mount", module="mods/ambulo")
            ann.call("logic_build", path="mods/ambulo", check=True)  # warm (the first build compiles the SDK)
            # A: ann's fix is ann's journaled change; she can undo it.
            v = ann.json("project_read", path=P, problems=False)["version"]
            ann.json("project_write", path=P, text=warn, expect_version=v)
            r = ann.json("logic_build", path="mods/ambulo", fix=True, reload=False)
            time.sleep(2.5)  # past the watcher: nothing may come in as `disk`
            disk = lib.read_text(encoding="utf-8")
            hist = ann.json("world_history", path_prefix="/project/" + P, limit=3)["entries"]
            store = ann.json("project_read", path=P, problems=False)["text"]
            check("fix:true on a mounted module is the caller's journaled change",
                  "let x = 1" in disk and "let mut x" not in disk and store == disk and hist[0]["by"] == "ann" and hist[0]["kind"] == "file"
                  and P in r.get("fixes_journaled", {}).get("files", []),
                  f"{m.get('files')} files mounted; newest entry by {hist[0]['by']} ({hist[0]['kind']}); fixes {r.get('fixes_journaled')}")
            u = ann.json("world_undo", count=1)
            check("ann undoes her fix (no undo_conflict)", "let mut x" in lib.read_text(encoding="utf-8") and bool(u.get("undone")), f"undone {u.get('undone')}")
            # B: bob's claim on the file stops ann's fix.
            claim = bob.json("claim_take", path="/project/" + P, lease_s=120)
            before = lib.read_text(encoding="utf-8")
            e = err_of(ann, "logic_build", path="mods/ambulo", fix=True, reload=False)
            time.sleep(2.5)
            check("a claim on a module file stops fix:true", e is not None and lib.read_text(encoding="utf-8") == before, json.dumps(e)[:160] if e else "no error")
            bob.json("claim_release", id=claim["id"])
            # C: bob edits the file while ann's fix build runs: his text stands, never a broken mix.
            results = []
            for delay in (0.3, 0.8, 1.5):
                v = ann.json("project_read", path=P, problems=False)["version"]
                ann.json("project_write", path=P, text=warn, expect_version=v)
                res = {}
                th = __import__("threading").Thread(target=lambda: res.setdefault("r", ann.request("tools/call", {"name": "logic_build", "arguments": {"path": "mods/ambulo", "fix": True, "reload": False}})))
                th.start()
                time.sleep(delay)
                v = bob.json("project_read", path=P, problems=False)["version"]
                bobtext = "// bob's header, written while ann's fix build ran\n" + warn
                bob.json("project_write", path=P, text=bobtext, expect_version=v)
                th.join()
                time.sleep(2.2)
                disk = lib.read_text(encoding="utf-8")
                sane = disk in (bobtext, bobtext.replace("let mut x", "let x"))
                answer = json.loads(res["r"]["content"][0]["text"])
                results.append((delay, sane, bool(answer.get("fixes_journaled", {}).get("skipped"))))
                v = ann.json("project_read", path=P, problems=False)["version"]
                ann.json("project_write", path=P, text=orig, expect_version=v)
            check("a file changed during the fix build keeps the other agent's text", all(s for _, s, _ in results),
                  "; ".join(f"delay {d}s: {'sane' if s else 'BROKEN'}{', fix skipped' if k else ''}" for d, s, k in results))
            # D: a mount inside another (here the assets root) is refused.
            shutil.copytree(ws / "mods" / "ambulo", ws / "emptyassets" / "mod2", ignore=shutil.ignore_patterns("target"))
            e = err_of(ann, "project_mount", module="emptyassets/mod2")
            check("a mount inside another is refused", e is not None and e.get("error") == "mount_overlap", (e or {}).get("message", "")[:120])
            check("a module in the engine's own workspace mounts with a warning", str(m.get("warning", "")).startswith("workspace_member"), str(m.get("warning", ""))[:80])
            # Round 3 E: compile-time path evasions. The token scan refuses what it sees at
            # commit; macro indirection is caught after the build by rustc's dep-info.
            outside = work / "outside.txt"
            outside.write_text("host file\n")
            o = outside.as_posix()
            evasions = {
                "a '//' string earlier on the line": f'const S: &str = "//"; const W: &str = include_str!("{o}");\npub fn leak() -> usize {{ W.len() + S.len() }}\n',
                "use core::include_str as grab": f'use core::include_str as grab;\nconst W: &str = grab!("{o}");\npub fn leak() -> usize {{ W.len() }}\n',
                "a macro_rules wrapper": f'macro_rules! m {{ ($i:ident) => {{ $i!("{o}") }} }}\nconst W: &str = m!(include_str);\npub fn leak() -> usize {{ W.len() }}\n',
            }
            seen = []
            for name, code in evasions.items():
                v = ann.json("project_read", path=P, problems=False)["version"]
                e = err_of(ann, "project_write", path=P, text=orig + "\n" + code, expect_version=v)
                if e is not None:
                    seen.append(f"{name}: refused at commit ({e.get('error')})")
                    continue
                b = err_of(ann, "logic_build", path="mods/ambulo", reload=False)
                err = (b or {}).get("error")
                code = err.get("code") if isinstance(err, dict) else err
                ok = b is not None and code == "source_path" and not b.get("diagnostics") and not b.get("library")
                seen.append(f"{name}: {'refused after the build (dep-info)' if ok else 'BUILT: ' + json.dumps(b)[:120]}")
            v = ann.json("project_read", path=P, problems=False)["version"]
            ann.json("project_write", path=P, text=orig, expect_version=v)
            check("compile-time path evasions are refused (commit scan or the build's dep-info)", all("refused" in s for s in seen), "; ".join(seen))
            # Round 3 F: allow_errors doesn't store a crates.io dependency or a build script.
            cargo = ws / "mods" / "ambulo" / "Cargo.toml"
            corig = cargo.read_text(encoding="utf-8")
            PC = "modules/ambulo/Cargo.toml"
            results = []
            for bad in (corig.replace("[dependencies]", '[dependencies]\nserde = "1"'), corig.replace("publish = false", 'publish = false\nbuild = "src/b.rs"')):
                v = ann.json("project_read", path=PC, problems=False)["version"]
                e = err_of(ann, "project_write", path=PC, text=bad, expect_version=v, allow_errors=True)
                results.append(e is not None and e.get("error") == "invalid" and cargo.read_text(encoding="utf-8") == corig)
            check("allow_errors never stores a crates.io dependency or a build script", all(results), str(results))
            # Round 3 H and L: fix:true while the disk doesn't hold the store's text.
            v = ann.json("project_read", path=P, problems=False)["version"]
            ann.json("project_write", path=P, text=warn, expect_version=v)
            time.sleep(1.1)
            lib.write_text(warn + "\n" + ("// pad " + "x" * 90 + "\n") * 11000, encoding="utf-8")
            wait_for("the held lib.rs", lambda: ann.json("project_list", prefix=P, limit=1).get("disk_unreadable"), 10.0)
            e_held = err_of(ann, "logic_build", path="mods/ambulo", fix=True, reload=False)
            lib.write_text(warn, encoding="utf-8")
            wait_for("the release", lambda: not ann.json("project_list", prefix=P, limit=1).get("disk_unreadable"), 10.0)
            fh = open(lib, "rb")  # holds the file open without delete sharing: the engine's rename-over fails
            try:
                v = bob.json("project_read", path=P, problems=False)["version"]
                bob.json("project_write", path=P, text="// bob's header, written while the file was locked\n" + warn, expect_version=v)
                pending = ann.json("project_list", prefix=P, limit=1).get("disk_pending")
                e_pending = err_of(ann, "logic_build", path="mods/ambulo", fix=True, reload=False)
            finally:
                fh.close()
            time.sleep(2.5)
            synced = lib.read_text(encoding="utf-8") == ann.json("project_read", path=P, problems=False)["text"]
            check("fix:true is refused while a module file is held or pending (Cargo compiles the disk)",
                  e_held is not None and e_held.get("fixes_journaled", {}).get("refused", {}).get("error") == "disk_out_of_sync"
                  and bool(pending) and e_pending is not None and e_pending.get("fixes_journaled", {}).get("refused", {}).get("error") == "disk_out_of_sync" and synced,
                  f"held: {(e_held or {}).get('error')}; pending: {(e_pending or {}).get('error')}; disk synced after the unlock: {synced}")
            v = ann.json("project_read", path=P, problems=False)["version"]
            ann.json("project_write", path=P, text=orig, expect_version=v)
            ann.close()
            bob.close()
        check("fix engine shuts down cleanly", eng.exit_code == 0, f"exit {eng.exit_code}")
    finally:
        if env_target is None:
            os.environ.pop("CARGO_TARGET_DIR", None)
        else:
            os.environ["CARGO_TARGET_DIR"] = env_target


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7496)
    ap.add_argument("--window", action="store_true", help="run with a window (default headless)")
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--out", default=str(ROOT / "target" / "project-check"))
    ap.add_argument("--skip-fix", action="store_true", help="skip the fix:true checks (they build a module workspace with Cargo)")
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
        # The M4 final audit: a write of the same text answered "unchanged" past bob's claim.
        same = ann.json("project_read", path=HUD)["text"]
        noop = err_of(ann, "project_write", path=HUD, text=same)
        check("a write that changes nothing is refused under another's claim too", noop is not None and noop.get("error") == "claimed", json.dumps(noop)[:160])
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

        # Review round 1's probes, live.
        tomb = (assets / "recipes" / "tombstone.json").read_bytes()
        if b"\r\n" in tomb:
            check("edits keep a CRLF file's line endings", b"\n" not in tomb.replace(b"\r\n", b""), "no bare LF after the inserted member")
        e = err_of(ann, "project_mount", module="crates/specula")
        check("an engine crate can't be mounted", e is not None and e.get("error") == "not_a_module", (e or {}).get("message", "")[:100])
        e = err_of(ann, "project_mount", module=".")
        check("the project root can't be mounted", e is not None and e.get("error") == "not_a_module", (e or {}).get("message", "")[:100])
        m = ann.json("project_mount", module="games/ambulo")
        e = err_of(ann, "project_patch", path="modules/ambulo/Cargo.toml", dry_run=True, ops=[{"find": "[dependencies]", "replace": "[dependencies]\nserde = \"1\""}])
        check("a module mounts; a crates.io dependency is refused", m.get("files", 0) >= 2 and e is not None and e.get("error") == "invalid"
              and any(p.get("code") == "third_party_crate" for p in e.get("problems", [])), f"{m.get('files')} files")
        # A human saves a sound in another encoding: held, never overwritten, replaced only on purpose.
        sell = assets / "sounds" / "sell.json"
        human = sell.read_bytes().replace(b'"sound": 1', b'"sound": 1, "notes": "caf\xe9"', 1)
        time.sleep(0.05)
        sell.write_bytes(human)
        held = wait_for("the unreadable edit", lambda: [h for h in ann.json("project_list", prefix="assets/sounds/sell", limit=1).get("disk_unreadable", []) if h["path"].endswith("sell.json")], 10.0)
        e = err_of(bob, "project_patch", path="assets/sounds/sell.json", ops=[{"set": "loudness", "value": -12}])
        check("a non-UTF-8 outside edit is held, not overwritten", bool(held) and e is not None and e.get("error") == "disk_unreadable" and sell.read_bytes() == human,
              (held or [{}])[0].get("why", ""))
        r = bob.json("project_patch", path="assets/sounds/sell.json", replace_unreadable=True, ops=[{"set": "loudness", "value": -12}])
        backup = Path(r.get("backup", "")) if r.get("backup") else None
        check("replacing it on purpose backs the bytes up first", backup is not None and backup.read_bytes() == human and b'"loudness": -12' in sell.read_bytes(), str(backup))
        # Round 2: held again, then the disk put back to the store's text: released.
        stored = sell.read_bytes()
        time.sleep(0.05)
        sell.write_bytes(human)
        wait_for("held again", lambda: ann.json("project_list", prefix="assets/sounds/sell", limit=1).get("disk_unreadable"), 10.0)
        time.sleep(0.05)
        sell.write_bytes(stored)
        released = wait_for("the release", lambda: not ann.json("project_list", prefix="assets/sounds/sell", limit=1).get("disk_unreadable"), 10.0)
        # ...and replacing a held file with the very text the store has still replaces it.
        time.sleep(0.05)
        sell.write_bytes(human)
        wait_for("held a third time", lambda: ann.json("project_list", prefix="assets/sounds/sell", limit=1).get("disk_unreadable"), 10.0)
        same = bob.json("project_write", path="assets/sounds/sell.json", text=stored.decode("utf-8"), replace_unreadable=True)
        check("a held file is released when the disk gets the store's text back; replace works with the same text",
              bool(released) and not same.get("unchanged") and same.get("backup") and sell.read_bytes() == stored, f"replace answered version {same.get('version')}")
        # Delete and make again: an old expect_version never matches the new file.
        P = "assets/sounds/aba.json"
        ann.json("project_write", path=P, text=(assets / "sounds" / "ui_hover.json").read_text().replace("ui_hover", "aba"), expect_version=0)
        old_v = bob.json("project_read", path=P, problems=False)["version"]
        ann.json("project_delete", path=P)
        again = ann.json("project_write", path=P, text=(assets / "sounds" / "ui_hover.json").read_text().replace("ui_hover", "aba"), expect_version=0)
        e = err_of(bob, "project_patch", path=P, expect_version=old_v, ops=[{"set": "loudness", "value": -30}])
        check("versions never repeat across a delete (no ABA)", again["version"] > old_v and e is not None and e.get("error") == "stale_version", f"v{old_v} deleted, made again as v{again['version']}")
        # Two spellings of one Windows file.
        ann.json("project_write", path="assets/Ä/recipes/t.json", text=original, allow_errors=True)
        e = err_of(ann, "project_write", path="assets/ä/recipes/t.json", text=original, allow_errors=True)
        check("Unicode case clashes are refused", e is not None and e.get("error") == "case_clash", (e or {}).get("message", "")[:100])
        # A folder that is a link to elsewhere.
        outside = work / "outside"
        outside.mkdir()
        if sys.platform == "win32" and subprocess.run(["cmd", "/c", "mklink", "/J", str(assets / "jx"), str(outside)], capture_output=True).returncode == 0:
            e = err_of(ann, "project_write", path="assets/jx/recipes/evil.json", text=original, allow_errors=True)
            check("writes through a linked folder are refused before commit", e is not None and e.get("error") == "linked_folder" and not any(outside.rglob("*")), (e or {}).get("error", ""))
        # Deleting the style every recipe uses.
        e = err_of(ann, "project_delete", path="assets/style.json")
        check("deleting a style its recipes need is refused", e is not None and e.get("error") == "invalid" and (assets / "style.json").exists(),
              f"{len((e or {}).get('problems', []))} problems")
        # Big files: a 980 KB anim set written repeatedly stays fast, and world history survives.
        bob.json("world_spawn", shape="cube", name="bobs_cube", position=[0, -50, 0])
        big = lambda i: json.dumps({f"k{n:06d}": i for n in range(70000)})
        times = []
        for i in range(6):
            t = time.perf_counter()
            ann.json("project_write", path="assets/anims/big.json", text=big(i), allow_errors=True)
            times.append(time.perf_counter() - t)
        undo = bob.json("world_undo", count=1)
        verify = ann.json("world_hash", verify=True)
        check("980 KB writes stay fast and never evict world history",
              max(times[1:]) < 5.0 and undo.get("undone") and json.dumps(verify).find('"replayable": false') < 0,
              f"writes {[round(t, 2) for t in times]} s; bob's undo {undo.get('undone')}")
        report["big_writes_s"] = [round(t, 3) for t in times]

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
    if not args.skip_fix:
        fix_checks(args, work, check)
    (work / "report.json").write_text(json.dumps(report, indent=2))
    print(f"report: {work / 'report.json'}")
    if failures:
        print(f"FAILED: {', '.join(failures)}")
        return 1
    print("all checks held")
    return 0


if __name__ == "__main__":
    sys.exit(main())

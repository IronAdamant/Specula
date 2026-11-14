#!/usr/bin/env python3
"""Map 1's props in a fresh Sepulcra (M5, the owner's decision of 2026-11-06): the 33 props the
owner approved in M4 are map data (games/sepulcra/data/slab_01.json, `props`), each with a
`blocks_build` flag. Big pieces block building on the cells they cover; small dressing doesn't,
and is hidden while a stone stands on it.

A fresh `sepulcra` (release), windowed on the iGPU, its rules from Lex (`--rules lex`, the
hand-written sim kept as the reference and compared every tick):

1. the data: every prop on top-face fragments (no lane, spawn or graveyard cell: no wave path
   changes), the board's buildable cells before and after, the cells near each cliff lane's rim
   (where stones reach climbers) still free;
2. all 33 in the world at their data places (32 at the owner's approved places; the corner fence
   moved, see the map notes), and drawn in the game's view (the frame record's visible_px, from
   two overviews and close-ups of any the overviews didn't measure);
3. style_check passes on every prop, under the confirmed categories; its result is recorded by
   the engine, and a review submission references it (and an overview capture) by id;
4. a stone on a big prop is refused with the prop named (prop_blocks, Lex's own code too), a
   stone on dressing is taken and the dressing is hidden (not drawn), then shown when sold;
5. a new game plays to victory with the centre build order; the busy and boss waves are measured
   as m3_exit measures them, against M3's and M4's GPU p95; the recording replays verified with
   the Rust and the Lex rules (same Lex hash); the props are still drawn at the end;
6. zero validation messages at shutdown.

    cargo build -p sepulcra-app --release
    python scripts/map1_check.py [--port 7540] [--adapter integrated] [--out DIR]

Output (summary.json, captures, packets, engine log) goes to target/map1-check/ unless --out. Look
at every PNG. Standard library only; output is ASCII.
"""
import argparse
import json
import math
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, detailed  # noqa: E402

SLAB = ROOT / "games" / "sepulcra" / "data" / "slab_01.json"
PROPS_ROOT = "/props/graveyard"
# The owner's approved world (M4 exit), read only when present (this checkout's target/, or the
# main checkout's when run from a lane's worktree under .claude/worktrees): where each prop was approved.
APPROVED = next((p for p in [ROOT / "target" / "m4-exit-approved", *[q / "target" / "m4-exit-approved" for q in ROOT.parents[:3]]]
                 if (p / "worlds" / "m4_exit").exists()), ROOT / "target" / "m4-exit-approved") / "worlds" / "m4_exit"
# GPU p95 (ms) of earlier exits on the iGPU: M3 (Sepulcra alone) and M4 (with the approved props).
M3_GPU_P95 = {"busy_wave": 5.6, "boss_wave": 5.9}
M4_GPU_P95 = {"busy_wave": 7.1, "boss_wave": 8.4}
FAILS = []


def log(msg):
    print(msg, flush=True)


def check(cond, what):
    log(f"  {'ok  ' if cond else 'FAIL'} {what}")
    if not cond:
        FAILS.append(what)
    return cond


def cell_of(x, z):
    return math.floor((x + 20) / 2), math.floor((z + 20) / 2)


def game_eye(yaw_deg, pitch_deg=42.0, dist=56.0, target=(0.0, 0.0, 0.0)):
    y, p = math.radians(yaw_deg), math.radians(pitch_deg)
    return [target[0] + dist * math.sin(y) * math.cos(p), target[1] + dist * math.sin(p), target[2] + dist * math.cos(y) * math.cos(p)]


def data_checks(slab, report):
    """What the map data says, checked without the engine."""
    log("the map data:")
    top = slab["top"]
    props = slab["props"]
    check(len(props) == 33, f"33 props in the map data ({len(props)})")
    cells = lambda refs: [tuple(int(v) for v in r.split(":")[1].split(",")) for r in refs]  # noqa: E731
    on_lanes = [(p["name"], c) for p in props for c in cells(p["cells"]) if top[c[1]][c[0]] != "#"]
    check(not on_lanes, f"every prop stands on top-face fragments only, so no wave path changes ({on_lanes[:3] or 'none on a lane'})")
    blocked = sorted({c for p in props if p["blocks_build"] for c in cells(p["cells"])})
    buildable = [(c, r) for r, row in enumerate(top) for c, ch in enumerate(row) if ch == "#"]
    ledges = sum(row.count("#") for face in slab["cliffs"].values() for row in face)
    report["board"] = {"top_fragments": len(buildable), "cliff_ledges": ledges, "blocked_by_props": len(blocked),
                       "buildable_after": len(buildable) + ledges - len(blocked)}
    check(len(blocked) <= len(buildable) // 6, f"big props block {len(blocked)} of {len(buildable)} top-face fragments (at most a sixth)")
    # A stone reaches another face within 3 m of their rim: the cells by each cliff lane's rim.
    rim = {}
    for face, rows in slab["cliffs"].items():
        for col in [i for i, ch in enumerate(rows[0]) if ch == "|"]:
            if face == "north":
                near = [(c, r) for c in range(col - 2, col + 3) for r in (0, 1)]
            elif face == "east":
                near = [(c, r) for c in (18, 19) for r in range(col - 2, col + 3)]
            else:
                near = [(c, r) for c in (0, 1) for r in range(col - 2, col + 3)]
            free = [x for x in near if 0 <= x[0] < 20 and 0 <= x[1] < 20 and top[x[1]][x[0]] == "#" and x not in blocked]
            rim[f"{face}:{col}"] = len(free)
    report["free_cells_by_cliff_lanes"] = rim
    check(all(n >= 3 for n in rim.values()), f"every cliff lane keeps 3+ free cells within reach of its rim ({rim})")
    # The lab's play styles and the built-in build orders build only where they may.
    used = set()
    for f in (ROOT / "games" / "sepulcra" / "data" / "layouts").glob("*.json"):
        for e in json.loads(f.read_text(encoding="utf-8"))["commands"]:
            if "place" in e:
                used.add(e["at"])
    for f in (ROOT / "games" / "sepulcra" / "lex").glob("*.targets.json"):
        doc = json.loads(f.read_text(encoding="utf-8"))
        for pol in doc.get("policies") or [doc.get("policy") or {}]:
            for s in pol.get("steps", []):
                site = (s.get("args") or {}).get("site")
                if site:
                    used.add(site)
    hit = sorted(u for u in used if tuple(int(v) for v in u.split(":")[1].split(",")) in blocked)
    check(not hit, f"no build order (layouts, the balance lab's play styles) builds on a big prop ({hit or 'none'})")
    report["blocks_build"] = {p["name"]: p["blocks_build"] for p in props}
    return props


def approved_places():
    """{name: (x, z, yaw)} from the approved world, when it is on this machine."""
    f = next(iter(sorted(APPROVED.glob("props.*.json"))), None) if APPROVED.exists() else None
    if not f:
        return None
    out = {}
    for e in json.loads(f.read_text(encoding="utf-8"))["entities"]:
        if e["path"].startswith(PROPS_ROOT + "/"):
            out[e["path"].split("/")[-1]] = (e["position"][0], e["position"][2], e["rotation_deg"][1])
    return out


def props_drawn(mcp, out, label, props):
    """visible_px of each prop from the frame record: two overviews from the game camera's orbit,
    then a close frame of any prop they didn't measure."""
    seen, frames = {}, []
    for yaw in (35, 215):
        f = mcp.json("sight_frame", eye=game_eye(yaw), target=[0, 0, 0], fov_y_deg=45, width=1280, height=720, max_subjects=64, response_format="detailed")
        name = f"{label}_overview_yaw{yaw}"
        shutil.copy(f["image"]["path"], out / f"{name}.png")
        shutil.copy(f["image"]["packet_path"], out / f"{name}.json")
        frames.append({"name": name, "recorded": f["image"].get("recorded")})
        for s in json.loads(Path(f["image"]["packet_path"]).read_text(encoding="utf-8")).get("subjects", []):
            p = s.get("path", "")
            if p.startswith(PROPS_ROOT + "/"):
                n = p.split("/")[-1]
                seen[n] = max(seen.get(n, 0), s.get("visible_px") or 0)
    for p in props:
        n = p["name"]
        if seen.get(n, 0) > 0:
            continue
        x, z = p["at"]
        g = mcp.json("sight_frame", eye=[x + 1.5, 7, z + 8], target=[x, 0.8, z], fov_y_deg=50, width=640, height=360, max_subjects=24, response_format="detailed")
        shutil.copy(g["image"]["path"], out / f"{label}_close_{n}.png")
        shutil.copy(g["image"]["packet_path"], out / f"{label}_close_{n}.json")
        for s in json.loads(Path(g["image"]["packet_path"]).read_text(encoding="utf-8")).get("subjects", []):
            if s.get("path") == f"{PROPS_ROOT}/{n}":
                seen[n] = s.get("visible_px") or 0
    return seen, frames


def subject(packet_path, path):
    for s in json.loads(Path(packet_path).read_text(encoding="utf-8")).get("subjects", []):
        if s.get("path") == path:
            return s
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7540)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--frames", type=int, default=600, help="window frames per perf capture (as m3_exit.py)")
    ap.add_argument("--exe", help="the sepulcra binary (default: release under CARGO_TARGET_DIR or target/)")
    ap.add_argument("--out", default=str(ROOT / "target" / "map1-check"))
    a = ap.parse_args()
    from m2_exit import PERF_TOML
    from m3_exit import measure
    from sepulcra_play import EXE, Run
    out = Path(a.out)
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    report = {"started": time.strftime("%Y-%m-%d %H:%M"), "adapter": a.adapter, "seed": a.seed}
    slab = json.loads(SLAB.read_text(encoding="utf-8"))
    props = data_checks(slab, report)
    want = {p["name"]: p for p in props}
    approved = approved_places()
    if approved:
        moved = sorted(n for n, (x, z, _) in approved.items() if n in want and (abs(want[n]["at"][0] - x) > 1e-3 or abs(want[n]["at"][1] - z) > 1e-3))
        report["moved_from_approved"] = moved
        check(set(approved) == set(want) and moved == ["fence_iron_corner"], f"the map data holds the 33 approved props, 32 at their approved places (moved: {moved})")
    else:
        report["moved_from_approved"] = "the approved world isn't on this machine: not compared"
    args = ["--adapter", a.adapter, "--size", "1280x720", "--perf", str(PERF_TOML), "--captures", str(out / "captures"),
            "--worlds", str(out / "worlds"), "--saves", str(out / "saves"), "--trace", str(out / "trace"),
            "--", "--seed", str(a.seed), "--mute", "--rules", "lex"]
    measured = []
    with Engine(a.exe or EXE, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        mcp = eng.mcp("map1-check")

        def call_raw(tool, **kw):
            return mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, kw)})
        mcp.call_raw = call_raw
        for _ in range(120):  # the game loads its models first
            r = mcp.request("tools/call", {"name": "agent_hello", "arguments": {"name": "mapper", "role": "builder"}})
            if not r.get("isError"):
                break
            time.sleep(1.0)
        report["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        mcp.call("time_control", action="pause")

        log("the props in a fresh game:")
        q = mcp.json("world_query", path_prefix=PROPS_ROOT, fields=["path", "position", "rotation_deg", "props", "visible", "modified_by"], limit=100)
        ents = {e["path"].split("/")[-1]: e for e in q.get("items", []) if e["path"].startswith(PROPS_ROOT + "/")}
        bad = []
        for n, p in want.items():
            e = ents.get(n)
            if e is None:
                bad.append((n, "missing"))
                continue
            pos = e["position"]
            if abs(pos[0] - p["at"][0]) > 1e-3 or abs(pos[2] - p["at"][1]) > 1e-3 or not 0.0 <= pos[1] <= 0.8 or abs(e["rotation_deg"][1] - p["yaw"]) > 1e-3:
                bad.append((n, pos))
            if (e.get("props") or {}).get("blocks_build") != p["blocks_build"]:
                bad.append((n, "blocks_build"))
        check(len(ents) == 33 and not bad, f"all 33 props stand at their data places with their blocks_build ({bad[:3] or 'all'})")
        seen, frames = props_drawn(mcp, out, "start", props)
        report["drawn_start"] = seen
        check(all(seen.get(n, 0) > 0 for n in want), f"all 33 props are drawn in the game's view, from the frame record ({sum(1 for n in want if seen.get(n, 0) > 0)}/33)")

        log("style_check under the confirmed categories, recorded by the engine:")
        style = mcp.json("style_check", subjects=[f"recipes/{p.get('recipe', p['name'])}.json" for p in props], response_format="detailed")
        report["style_check"] = {"passed": style.get("passed"), "subjects": style.get("subjects"), "failed": style.get("failed"), "recorded": style.get("recorded")}
        # Recipes with params are checked at their variants too (33 recipes, more subjects).
        check(style.get("passed") is True and style.get("failed") == 0 and (style.get("subjects") or 0) >= 33,
              f"style_check passes on every prop and its variants ({style.get('failed')} failed of {style.get('subjects')} subjects)")
        cats = sorted({r.get("facts", {}).get("category") for r in style.get("results", [])} - {None})
        report["categories"] = cats
        check(set(cats) <= {"prop", "graveyard", "building", "ruin", "set", "terrain"}, f"categories used: {cats}")
        # The review evidence: the props exactly as they stand (no variants), so the recorded check
        # judges every model under the paths, and an overview capture.
        exact = mcp.json("style_check", subjects=[f"recipes/{p.get('recipe', p['name'])}.json" for p in props], variants=False)
        rec, cap = exact.get("recorded") or {}, (frames[0].get("recorded") or {})
        sub = mcp.call_raw("review_submit", kind="asset", title="Map 1's 33 props (blocks_build as data)", paths=[PROPS_ROOT],
                           results=[rec, cap], summary="The approved M4 props in the shipped map data; style_check and the overview are the engine's records.")
        body = json.loads(sub["content"][0]["text"])
        check(not sub.get("isError") and (body.get("checks", {}).get("engine", {}).get("recorded") == 1), f"review_submit references the engine's records ({json.dumps(body.get('checks') or body)[:200]})")
        if not sub.get("isError"):
            full = mcp.json("review_list", id=body.get("id"), response_format="detailed")
            gap = [c for c in full.get("checks") or [] if c.get("name") == "models_engine_checked"]
            caps = [c for c in full.get("captures") or [] if c.get("recorded")]
            check(len(gap) == 1 and gap[0].get("passed") is True and len(caps) == 1, f"the recorded check judged every prop model, and the overview is marked recorded ({json.dumps(gap)[:200]})")
        report["review"] = body

        log("building on props:")
        r = mcp.call_raw("play_command", action="place", stone="warding", cell="top:8,2")
        err = json.loads(r["content"][0]["text"])
        report["refused"] = err
        check(r.get("isError") and err.get("code") == "prop_blocks" and err.get("prop") == "statue_angel" and (err.get("lex") or {}).get("code") == "prop_blocks",
              f"a stone on the grave angel is refused by both rule sets: {err.get('code')}, {err.get('prop')}, lex {(err.get('lex') or {}).get('code')}")
        check(str(err.get("message", "")).endswith("a grave angel stands here"), f"the refusal's words: {err.get('message')}")
        at = want["bone_pile"]["at"]
        eye, target = [at[0] + 3.0, 5.5, at[1] + 6.0], [at[0], 0.6, at[1]]
        before = mcp.json("sight_frame", eye=eye, target=target, fov_y_deg=45, width=960, height=540, max_subjects=24, response_format="detailed")
        shutil.copy(before["image"]["path"], out / "dressing_before.png")
        shutil.copy(before["image"]["packet_path"], out / "dressing_before.json")
        r = mcp.call_raw("play_command", action="place", stone="warding", cell="top:2,15")
        check(not r.get("isError"), f"a stone on the pile of bones (dressing) is taken ({r['content'][0]['text'][:120]})")
        vis = mcp.json("world_query", path_prefix=f"{PROPS_ROOT}/bone_pile", fields=["path", "visible"], limit=2).get("items", [])
        after = mcp.json("sight_frame", eye=eye, target=target, fov_y_deg=45, width=960, height=540, max_subjects=24, response_format="detailed")
        shutil.copy(after["image"]["path"], out / "dressing_hidden.png")
        shutil.copy(after["image"]["packet_path"], out / "dressing_hidden.json")
        bones_before = subject(before["image"]["packet_path"], f"{PROPS_ROOT}/bone_pile")
        bones_after = subject(after["image"]["packet_path"], f"{PROPS_ROOT}/bone_pile")
        stone_after = subject(after["image"]["packet_path"], "/units/stones/top:2,15")
        report["dressing"] = {"bones_before": bones_before, "bones_after": bones_after, "stone_after": stone_after, "entity": vis}
        check((bones_before or {}).get("visible_px", 0) > 0, f"the bones are drawn before ({(bones_before or {}).get('visible_px')} px)")
        check(vis and vis[0].get("visible") is False and (bones_after is None or not bones_after.get("visible_px")), f"under the stone the bones are hidden, not drawn ({bones_after})")
        check((stone_after or {}).get("visible_px", 0) > 0, f"the stone is drawn there ({(stone_after or {}).get('visible_px')} px)")
        mcp.call_raw("play_command", action="sell", cell="top:2,15")
        vis = mcp.json("world_query", path_prefix=f"{PROPS_ROOT}/bone_pile", fields=["path", "visible"], limit=2).get("items", [])
        check(vis and vis[0].get("visible") is True, "sold: the bones show again")

        log("a whole game with the props:")
        mcp.json("play_new", seed=a.seed)
        run = Run(mcp, out)
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
        rec = mcp.json("play_record", path="recordings/map1.json", overwrite=True)
        rep = mcp.json("play_replay", path="recordings/map1.json")
        lex = st.get("rules") or {}
        report["replay"] = {"status": rep.get("status"), "lex_parity": lex.get("parity"), "lex_hash_recorded": (rec.get("lex") or {}).get("end_hash"),
                            "lex_hash_replayed": (rep.get("lex") or {}).get("end_hash")}
        seen_end, _ = props_drawn(mcp, out, "end", props)
        report["drawn_end"] = seen_end
        check(all(seen_end.get(n, 0) > 0 for n in want), f"all 33 props are still drawn at the game's end ({sum(1 for n in want if seen_end.get(n, 0) > 0)}/33)")
        report["captures"] = run.captures
        report["layout"] = {"applied": run.applied, "skipped": run.skipped}
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    report["measured"] = measured
    check(report["end"]["outcome"] == "won", f"the game is won with the props ({report['end']['outcome']} at wave {report['end']['wave']}, {report['end']['tombstones']} tombstones)")
    check(not run.skipped, f"the build order applied in full ({run.applied} applied, skipped {run.skipped})")
    rp = report["replay"]
    check(rp["status"] == "verified" and rp["lex_parity"] == "ok" and rp["lex_hash_recorded"] == rp["lex_hash_replayed"] and rp["lex_hash_recorded"],
          f"the recording replays verified with the Rust rules and the Lex rules agree ({rp})")
    check(errors == 0, f"validation errors at shutdown: {errors}")
    check(len(measured) == 2 and all(m["passed"] for m in measured), "the busy and boss waves hold the frame budgets")
    for m in measured:
        p95 = (m.get("gpu_ms") or {}).get("p95")
        log(f"  {m['label']}: GPU p95 {p95} ms (M3 {M3_GPU_P95.get(m['label'])}, M4 {M4_GPU_P95.get(m['label'])}), frame p95 {(m.get('frame_ms') or {}).get('p95')} ms")
    report["gpu_p95_vs"] = {m["label"]: {"now": (m.get("gpu_ms") or {}).get("p95"), "m3": M3_GPU_P95.get(m["label"]), "m4": M4_GPU_P95.get(m["label"])} for m in measured}
    report["fails"] = FAILS
    (out / "summary.json").write_text(json.dumps(report, indent=1, default=str), encoding="utf-8")
    log(f"summary: {out / 'summary.json'}")
    if FAILS:
        log(f"\n{len(FAILS)} check(s) failed")
        sys.exit(1)
    log("map1_check: all checks passed")


if __name__ == "__main__":
    main()

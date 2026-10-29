#!/usr/bin/env python3
"""Baked GI check (M4 "Baked global illumination"; docs/design/GI.md).

1. **Bake and determinism.** A fresh headless engine builds a yard: ground, a hut with 0.2 m walls,
   a door facing the moon and a brazier (point light, glowing bowl) at the door, plus a closed
   shed. `gi_bake` runs off the frame thread (progress from `gi_info`); a second bake of the same
   scene finishes at once from the store; a second engine on the other adapter, with its own
   store and one thread, bakes the same scene to the same hash.
2. **No leaks.** `gi_check` samples the closed shed's floor and walls (dark) and the hut's floor
   inside the door (lit by bounce); the octants that would leak without masks are counted.
3. **Staleness and the write policy.** Moving a wall makes the volume stale (geometry); moving it
   back makes it fresh; `/gi` and `props.gi_volume` are refused to world_spawn/world_set
   (`reserved`); undoing the bake's result leaves the volume unbaked, and baking again lands it
   from the store.
4. **Sight.** Captures of the yard with `gi` off, lit, irradiance and probes (PNG plus packet),
   lit at tier high and tier low side by side (`gi_settings`).
4b. **Bounce from a fire.** A second volume, `corner`: a brazier in the corner of two pale walls,
   at night. Baked with its light and again with the light dimmed to nothing, the difference at
   points on the floor and walls is the brazier's bounce alone; captures off, lit and irradiance.
5. **Sepulcra.** The game hosted on the chosen adapter (the Intel iGPU by default), windowed, its
   centre layout built; an agent places the crypt recipe by the graveyard and bakes the top face
   (`gi_bake`, spacing 1 m). Captures of the graveyard and braziers, the crypt's door and its
   inside, each with GI off and on (tier high), and lit at tier low beside them; then a wave runs in
   real time with `gi_settings quality` off, low and high in turn (twice), `perf_capture`'s
   scene pass and GPU time each time.
6. **Validation.** Every engine must shut down with zero validation errors.

Usage: cargo build --release -p specula -p sepulcra-app && python scripts/gi_check.py
       [--adapter integrated] [--port 7630] [--out DIR] [--skip-sepulcra]
Output (captures, packets, logs, summary.json) goes to target/gi-check/<adapter>/. Look at every
PNG. Standard library only; output ASCII.
"""
import argparse
import base64
import json
import shutil
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
EXE = TARGET / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
GAME = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
CENTRE = ROOT / "games" / "sepulcra" / "data" / "layouts" / "centre.json"
WALLS = ["top:10,7", "top:12,10", "top:9,12", "top:7,9"]


def rel(p):
    """A path for the summary: relative to the repository when inside it (no home folders)."""
    try:
        return str(Path(p).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return "<out>/" + Path(p).name


def tool(m, tool_name, **a):
    """(is_error, parsed or text) of a tool call: errors are data here."""
    r = m.request("tools/call", {"name": tool_name, "arguments": a})
    text = next(c["text"] for c in r["content"] if c["type"] == "text")
    try:
        body = json.loads(text)
    except ValueError:
        body = text
    return r.get("isError", False), body


def box(name, lo, hi, color="#8c96a3", **extra):
    c = [(lo[i] + hi[i]) / 2 for i in range(3)]
    s = [hi[i] - lo[i] for i in range(3)]
    return dict({"op": "spawn", "shape": "cube", "name": name, "position": c, "scale": s, "color": color}, **extra)


def yard_ops():
    ops = [box("ground", [-14, -1, -14], [14, 0, 14], "#5b6470")]
    # The hut: inside x, z in [0.2, 3.2], 2.4 m high; a 1.2 m door in its south wall (z = 0).
    ops += [
        box("hut_w", [0, 0, 0], [0.2, 2.4, 3.4]),
        box("hut_e", [3.2, 0, 0], [3.4, 2.4, 3.4]),
        box("hut_n", [0.2, 0, 3.2], [3.2, 2.4, 3.4]),
        box("hut_s1", [0.2, 0, 0], [1.1, 2.4, 0.2]),
        box("hut_s2", [2.3, 0, 0], [3.2, 2.4, 0.2]),
        box("hut_lintel", [1.1, 2.0, 0], [2.3, 2.4, 0.2]),
        box("hut_roof", [-0.1, 2.4, -0.1], [3.5, 2.6, 3.5], "#5b6470"),
    ]
    # The shed: closed on every side (no light gets in).
    ops += [
        box("shed_w", [-6, 0, -1], [-5.8, 2.0, 2]),
        box("shed_e", [-3.2, 0, -1], [-3, 2.0, 2]),
        box("shed_n", [-5.8, 0, 1.8], [-3.2, 2.0, 2]),
        box("shed_s", [-5.8, 0, -1], [-3.2, 2.0, -0.8]),
        box("shed_roof", [-6.1, 2.0, -1.1], [-2.9, 2.2, 2.1], "#5b6470"),
    ]
    # A brazier at the hut's door: a glowing bowl and its fire light.
    ops.append({"op": "spawn", "shape": "sphere", "name": "brazier", "position": [1.7, 0.5, -1.0], "scale": 0.5, "color": "#ff7a1f",
                "emissive": 3.0, "light": {"intensity": 2.2, "range": 4.5, "color": "#ff9a40", "offset": [0, 0.6, 0]}})
    return ops


# The corner: pale walls (north and east) around a brazier, in its own volume east of the yard.
CORNER_BAKE = {"volume": "corner", "min": [9, 0, -3], "max": [14, 3, 2], "spacing": 0.5, "rays": 256, "bounces": 2}
CORNER_LIGHT = {"intensity": 2.2, "range": 3.0, "color": "#ff9a40", "offset": [0, 0.6, 0]}


def corner_ops():
    return [
        box("corner_n", [10, 0, 1.0], [13.2, 2.2, 1.2], "#d8cfc0"),
        box("corner_e", [13.0, 0, -2.0], [13.2, 2.2, 1.0], "#d8cfc0"),
        {"op": "spawn", "shape": "sphere", "name": "brazier2", "position": [12.4, 0.4, 0.4], "scale": 0.5, "color": "#ff7a1f",
         "emissive": 3.0, "light": CORNER_LIGHT},
    ]


# Points where the corner's bounce shows: the floor in front of and beside the brazier, the walls
# away from it, and the floor just past the light's range (lit by bounce alone).
CORNER_POINTS = [
    {"p": [11.4, 0.0, -0.4], "n": [0, 1, 0]},
    {"p": [10.4, 0.0, 0.6], "n": [0, 1, 0]},
    {"p": [10.4, 1.2, 1.0], "n": [0, 0, -1]},
    {"p": [13.0, 1.2, -1.6], "n": [-1, 0, 0]},
    {"p": [9.6, 0.0, -2.4], "n": [0, 1, 0]},
]


def night(m):
    m.json("world_env", sun_direction=[-0.35, 0.55, -0.75], sun_color="#c9d6ff", sun_intensity=0.9, ambient_color="#2a3448",
           ambient_intensity=0.9, tonemap=True, exposure=1.1, background="#0b1020", shadow={"enabled": True}, bloom={"enabled": True})


BAKE = {"volume": "yard", "min": [-9, 0, -6], "max": [7, 4, 6], "spacing": 0.5, "rays": 256, "bounces": 2}


def wait_bake(m, timeout=600):
    start = time.time()
    while True:
        info = m.json("gi_info")
        if info["job"] is None:
            return info, time.time() - start
        if time.time() - start > timeout:
            raise RuntimeError(f"bake did not finish: {info['job']}")
        time.sleep(0.25)


def settle(m, name, timeout=60):
    """A volume's status once its bake has loaded (files load on a worker after a bake lands)."""
    start = time.time()
    while True:
        st = volume(m.json("gi_info", volume=name), name)["status"]
        if st != "loading" or time.time() - start > timeout:
            return st
        time.sleep(0.25)


def wait_pipelines(m, timeout=300):
    """Seconds until the GI pipelines exist (built on a worker from the bake's start)."""
    start = time.time()
    while not m.json("gi_info")["runtime"]["pipelines_ready"]:
        if time.time() - start > timeout:
            raise RuntimeError("GI pipelines never became ready")
        time.sleep(0.5)
    return round(time.time() - start, 2)


def capture(m, out, label, caps, **view):
    r = m.call("sight_frame", width=960, height=540, max_subjects=2, **view)
    png = next(c for c in r["content"] if c["type"] == "image")["data"]
    packet = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
    path = out / f"{label}.png"
    path.write_bytes(base64.b64decode(png))
    (out / f"{label}.json").write_text(json.dumps(packet, indent=1))
    caps.append({"label": label, "png": rel(path), "packet": rel(out / f"{label}.json"), "gi": packet.get("gi"), "warnings": packet.get("warnings")})
    print(f"  capture {label}: gi {json.dumps(packet.get('gi'))[:160]}")
    return packet


def volume(info, name):
    return next(v for v in info["volumes"] if v["name"] == name)


def part_yard(args, out):
    res = {}
    d = out / "yard"
    d.mkdir(parents=True, exist_ok=True)
    store = d / "cache_a"
    # A fresh store each run: a bake served from a warm store commits the volume and its result as
    # one entry, so the undo step below would remove the volume instead of unbaking it.
    shutil.rmtree(store, ignore_errors=True)
    # The operator grants the lead its name (agent_hello role lead needs the grant).
    with Engine(args.exe, args.port, d / "engine.log", "--headless", "--adapter", args.adapter, "--cache", str(store),
                "--captures", str(d / "captures"), "--lead", "gi-lead", cwd=d) as eng:
        m = eng.mcp("gi-check")
        m.json("agent_hello", name="gi-lead", role="lead")
        night(m)
        m.json("world_batch", ops=yard_ops())
        # Tier high for the main captures (the headless engine on the iGPU defaults to low).
        res["settings_default"] = m.json("gi_settings")
        m.json("gi_settings", quality="high")
        caps = []
        view = {"eye": [1.7, 2.2, -7.5], "target": [1.7, 0.9, 1.6], "fov_y_deg": 55}
        capture(m, d, "yard_before", caps, **view)
        started = m.json("gi_bake", **BAKE)
        res["bake_started"] = {k: started.get(k) for k in ("status", "estimate", "journal")}
        print(f"  bake: {json.dumps(started)[:300]}")
        info, wall_s = wait_bake(m)
        res["bake_wall_s"] = round(wall_s, 2)
        res["last_job"] = info["last_job"]
        v = volume(info, "yard")
        res["status"] = settle(m, "yard")
        res["hash"] = v["baked"]["hash"]
        res["pipelines_wait_s_after_bake"] = wait_pipelines(m)
        print(f"  baked in {wall_s:.1f} s: {v['status']} {v['baked']}")
        # The same scene again: from the store, at once.
        again = m.json("gi_bake", volume="yard")
        res["again"] = {"status": again["status"], "cached": again.get("cached"), "hash": (again.get("result") or {}).get("baked", {}).get("hash")}
        # No leaks: inside the shed dark, inside the hut by the door lit by bounce.
        pts = [
            {"p": [-4.5, 0.0, 0.5], "n": [0, 1, 0]},
            {"p": [-5.8, 1.0, 0.5], "n": [1, 0, 0]},
            {"p": [-3.2, 1.0, 0.5], "n": [-1, 0, 0]},
            {"p": [-4.5, 2.0, 0.5], "n": [0, -1, 0]},
            {"p": [1.7, 0.0, 0.8], "n": [0, 1, 0]},
            {"p": [1.7, 0.0, 2.8], "n": [0, 1, 0]},
            {"p": [6.0, 0.0, -3.0], "n": [0, 1, 0]},
        ]
        chk = m.json("gi_check", volume="yard", points=pts)
        res["check"] = chk["volumes"]
        lum = [p.get("luminance") for p in chk["points"]]
        res["luminance"] = {"shed_floor": lum[0], "shed_wall_w": lum[1], "shed_wall_e": lum[2], "shed_ceiling": lum[3], "hut_door": lum[4], "hut_back": lum[5], "open": lum[6]}
        open_l = lum[6] or 1e-6
        res["shed_dark"] = max(lum[:4]) < 0.05 * open_l
        res["hut_lit_by_bounce"] = (lum[4] or 0) > 0.1 * open_l and (lum[4] or 0) > 3 * max(lum[:4])
        print(f"  luminance: {res['luminance']}")
        # Sight: off, lit, irradiance, probes.
        for g in ("off", "lit", "irradiance", "probes"):
            capture(m, d, f"yard_{g}", caps, gi=g, **view)
        capture(m, d, "hut_inside_off", caps, gi="off", eye=[1.7, 1.2, 0.6], target=[1.7, 0.8, 3.2], fov_y_deg=80)
        capture(m, d, "hut_inside_lit", caps, eye=[1.7, 1.2, 0.6], target=[1.7, 0.8, 3.2], fov_y_deg=80)
        capture(m, d, "hut_inside_irradiance", caps, gi="irradiance", eye=[1.7, 1.2, 0.6], target=[1.7, 0.8, 3.2], fov_y_deg=80)
        # Tier low beside high: per-draw SH for the walls, boxes and brazier, per pixel for the ground.
        res["settings_low"] = m.json("gi_settings", quality="low")
        wait_pipelines(m)
        capture(m, d, "yard_lit_low", caps, **view)
        capture(m, d, "hut_inside_lit_low", caps, eye=[1.7, 1.2, 0.6], target=[1.7, 0.8, 3.2], fov_y_deg=80)
        capture(m, d, "yard_irradiance_low", caps, gi="irradiance", **view)
        m.json("gi_settings", quality="high"); wait_pipelines(m)
        res["captures"] = caps
        # Staleness: move a wall, then back.
        m.json("world_set", path="/hut_e", position=[3.5, 1.2, 1.7])
        res["stale_after_move"] = volume(m.json("gi_info", volume="yard"), "yard").get("stale")
        m.json("world_set", path="/hut_e", position=[3.3, 1.2, 1.7])
        res["fresh_after_move_back"] = volume(m.json("gi_info", volume="yard"), "yard")["status"]
        m.json("world_env", ambient_intensity=0.8)
        res["stale_after_env"] = volume(m.json("gi_info", volume="yard"), "yard").get("stale")
        m.json("world_env", ambient_intensity=0.9)
        # The write policy: /gi and props.gi_volume are gi_bake's.
        e1, b1 = tool(m, "world_spawn", shape="group", name="sneak", parent="/gi")
        e2, b2 = tool(m, "world_set", path="/ground", props={"gi_volume": {"min": [0, 0, 0], "max": [1, 1, 1]}})
        e3, b3 = tool(m, "world_delete", path="/gi/yard")
        res["reserved"] = {"spawn_in_gi": e1 and "reserved" in json.dumps(b1), "props_elsewhere": e2 and "reserved" in json.dumps(b2),
                           "delete_volume": e3 and "reserved" in json.dumps(b3)}
        # Undo the result: unbaked; bake again: from the store.
        hist = m.json("world_history", path_prefix="/gi", limit=10)
        entries = hist.get("entries") or hist.get("items") or []
        res["history_entries"] = len(entries)
        res["history"] = entries
        last = max(entries, key=lambda e: e.get("id", 0)) if entries else None
        if last:
            m.json("world_undo", ids=[last["id"]])
            res["after_undo"] = volume(m.json("gi_info", volume="yard"), "yard")["status"]
            redo = m.json("gi_bake", volume="yard")
            res["rebake_after_undo"] = {"status": redo["status"], "cached": redo.get("cached")}
            res["after_rebake"] = settle(m, "yard")
        # The corner: bounce from the brazier alone (baked with its light, then with it dimmed).
        m.json("world_batch", ops=corner_ops())
        m.json("gi_bake", **CORNER_BAKE)
        wait_bake(m)
        lit = m.json("gi_check", volume="corner", points=CORNER_POINTS)["points"]
        cview = {"eye": [9.4, 1.9, -2.8], "target": [12.2, 0.5, 0.5], "fov_y_deg": 60}
        for g in ("off", "lit", "irradiance"):
            capture(m, d, f"corner_{g}", caps, gi=g, **cview)
        m.json("gi_settings", quality="low"); wait_pipelines(m)
        capture(m, d, "corner_lit_low", caps, **cview)
        m.json("gi_settings", quality="high"); wait_pipelines(m)
        m.json("world_set", path="/brazier2", light=dict(CORNER_LIGHT, intensity=0.0), emissive=0.0)
        m.json("gi_bake", volume="corner")
        wait_bake(m)
        dark = m.json("gi_check", volume="corner", points=CORNER_POINTS)["points"]
        m.json("world_set", path="/brazier2", light=CORNER_LIGHT, emissive=3.0)
        m.json("gi_bake", volume="corner")
        wait_bake(m)
        res["corner"] = [{"p": pt["p"], "with_brazier": a.get("luminance"), "without": b.get("luminance"),
                          "bounce": round((a.get("luminance") or 0) - (b.get("luminance") or 0), 4)}
                         for pt, a, b in zip(CORNER_POINTS, lit, dark)]
        print(f"  corner bounce (luminance with - without the brazier): {json.dumps(res['corner'])}")
        res["info"] = m.json("gi_info")
    res["validation_errors"] = eng.validation_errors()
    # Another engine, the other adapter, its own store, one thread: the same bytes.
    other = "discrete" if args.adapter == "integrated" else "integrated"
    d2 = out / "yard_other"
    d2.mkdir(parents=True, exist_ok=True)
    with Engine(args.exe, args.port + 1, d2 / "engine.log", "--headless", "--adapter", other, "--cache", str(d2 / "cache_b"),
                "--captures", str(d2 / "captures"), "--lead", "gi-lead-2", cwd=d2) as eng2:
        m = eng2.mcp("gi-check-2")
        night(m)
        m.json("world_batch", ops=yard_ops())
        m.json("agent_hello", name="gi-lead-2", role="lead")
        m.json("gi_bake", threads=1, **BAKE)
        info, wall_s = wait_bake(m)
        # The GI pipelines build on a worker from the bake's start: let them finish before the
        # engine shuts down (a shutdown waits for them).
        wait_pipelines(m)
        res["other"] = {"adapter": other, "hash": volume(info, "yard")["baked"]["hash"], "wall_s": round(wall_s, 2), "threads": 1}
    res["validation_errors_other"] = eng2.validation_errors()
    res["same_bytes"] = res["other"]["hash"] == res["hash"]
    print(f"  same hash on {other} with one thread: {res['same_bytes']} ({res['hash']})")
    return res


class Game:
    """Drives Sepulcra: the centre layout plus walls on the graveyard gates (as vfx_check)."""

    def __init__(self, mcp):
        self.mcp = mcp
        self.entries = [{"wave": 0, "wall": w} for w in WALLS] + json.loads(CENTRE.read_text())["commands"]
        self.next = 0

    def state(self, **a):
        return self.mcp.json("play_state", **a)

    def drive(self):
        st = self.state()
        while self.next < len(self.entries):
            e = self.entries[self.next]
            if st["wave"] < e.get("wave", 0):
                return
            if "place" in e:
                a = {"action": "place", "stone": e["place"], "cell": e["at"]}
            elif "wall" in e:
                a = {"action": "wall", "cell": e["wall"]}
            elif "upgrade" in e:
                a = {"action": "upgrade", "cell": e["upgrade"]}
            elif "call_wave" in e:
                a = {"action": "call_wave"}
            else:
                self.next += 1
                continue
            r = self.mcp.request("tools/call", {"name": "play_command", "arguments": a})
            if r.get("isError") and "not_enough_dust" in r["content"][0]["text"]:
                return
            self.next += 1
            st = self.state()

    def step(self, n):
        while n > 0:
            k = min(n, 300)
            self.mcp.call("time_control", action="step", ticks=k)
            n -= k


def scene_ms(perf):
    win = perf.get("window") or {}
    passes = (win.get("gpu") or {}).get("passes") or []
    scene = [p for p in passes if isinstance(p, dict) and p.get("name") in ("scene", "scene_blend")]
    return {"passes": {p["name"]: p.get("p95_ms", p.get("gpu_ms")) for p in scene}, "gpu_total_ms": (win.get("gpu") or {}).get("total_ms"),
            "gpu_p95_ms": (win.get("gpu") or {}).get("p95_ms"), "frame_pacing": perf.get("frame_pacing")}


def part_sepulcra(args, out):
    d = out / "sepulcra"
    d.mkdir(parents=True, exist_ok=True)
    res = {}
    extra = ["--adapter", args.adapter, "--size", "1280x720", "--cache", str(d / "cache"), "--captures", str(d / "captures"), "--lead", "gi-lead",
             "--", "--seed", "7", "--mute"]
    with Engine(args.game, args.port + 2, d / "engine.log", *extra, cwd=d) as eng:
        m = eng.mcp("gi-sepulcra")
        # The game loads its models first: the engine may answer "busy" for a while.
        for _ in range(120):
            er, body = tool(m, "agent_hello", name="gi-lead", role="lead")
            if not er:
                break
            time.sleep(1.0)
        res["gpu"] = m.json("engine_info")["gpu"]["name"]
        m.call("time_control", action="pause")
        m.json("play_new", seed="7")
        g = Game(m)
        g.drive()
        g.step(20)
        # Build until a brazier burns (from wave 2) and wave 3 is on, the game paused between steps.
        while True:
            st = g.state()
            braziers = m.json("world_query", tags=["brazier"], limit=4)
            have = braziers.get("entities") or braziers.get("items") or []
            if st["outcome"] != "running" or (st["wave"] >= 3 and have) or st["wave"] >= 6:
                break
            g.drive()
            g.step(30)
        res["braziers"] = len(have)
        # The crypt: an agent places the recipe by the graveyard, facing it.
        q = m.json("world_query", path_prefix="/slab/graveyard/ground", fields=["position", "scale"], limit=4)
        items = q.get("entities") or q.get("items") or []
        ground = items[0] if items else {"position": [0, 0, 0], "scale": [8, 0.2, 8]}
        gp, gs = ground["position"], ground["scale"]
        top = gp[1] + gs[1] / 2
        crypt_at = [gp[0] + gs[0] / 2 + 3.0, top - 0.05, gp[2]]
        res["crypt_at"] = crypt_at
        # The crypt recipe is solid (its openings are recesses): an agent writes a hollow variant
        # through the project tools, so it has a room inside: sealed (the generator always fits a door
        # leaf), lit only by its glowing windows, which shows emissive bounce and no sky leaking in
        # (light coming in through a doorway is the yard hut's check). Built first (model_build
        # waits for it), so the bake sees its walls.
        text = (ROOT / "games" / "sepulcra" / "assets" / "recipes" / "crypt.json").read_text()
        text = text.replace('"op": "building",', '"op": "building", "hollow": true,', 1).replace('"max_triangles": 2500', '"max_triangles": 4000').replace('"name": "crypt"', '"name": "crypt_hollow"', 1)
        res["crypt_recipe"] = tool(m, "project_write", path="assets/recipes/crypt_hollow.json", text=text)[0] is False
        res["crypt_build"] = tool(m, "model_build", recipe="recipes/crypt_hollow.json")[0] is False
        er, body = tool(m, "world_spawn", shape="model", name="crypt", recipe="recipes/crypt_hollow.json", position=crypt_at, rotation_deg=[0, 90, 0])
        res["crypt_spawn"] = "ok" if not er else body
        time.sleep(1.0)
        caps = []
        views = {
            "graveyard": {"eye": [gp[0] - 9, top + 7, gp[2] - 11], "target": [gp[0], top, gp[2]], "fov_y_deg": 50},
            "crypt_door": {"eye": [crypt_at[0] - 4.5, top + 1.6, crypt_at[2] + 0.2], "target": [crypt_at[0], top + 1.1, crypt_at[2]], "fov_y_deg": 55},
            "crypt_inside": {"eye": [crypt_at[0] - 1.5, top + 1.3, crypt_at[2]], "target": [crypt_at[0] + 2, top + 0.9, crypt_at[2]], "fov_y_deg": 80},
            "overview": {"eye": [38, 27, 52], "target": [0, -9, 1], "fov_y_deg": 45},
        }
        for label, v in views.items():
            capture(m, d, f"{label}_before", caps, include_ui=False, **v)
        # Bake the top face: the slab's top plus a few metres above.
        bake = {"volume": "top", "min": [-22, top - 1.0, -22], "max": [22, top + 7.0, 22], "spacing": 1.0, "rays": 256, "bounces": 2, "sky_below": 0.6}
        started = m.json("gi_bake", **bake)
        res["bake_started"] = {k: started.get(k) for k in ("status", "estimate")}
        print(f"  sepulcra bake: {json.dumps(started)[:300]}")
        info, wall_s = wait_bake(m, timeout=1200)
        res["bake_wall_s"] = round(wall_s, 2)
        res["last_job"] = info["last_job"]
        settle(m, "top")
        res["volume"] = volume(m.json("gi_info", volume="top"), "top")
        print(f"  baked in {wall_s:.1f} s: {res['volume']['status']} {json.dumps(info['last_job'])[:300]}")
        res["pipelines_wait_s_after_bake"] = wait_pipelines(m)
        print(f"  GI pipelines ready {res['pipelines_wait_s_after_bake']} s after the bake")
        res["check"] = m.json("gi_check", volume="top")["volumes"]
        res["settings_default"] = m.json("gi_settings")
        m.json("gi_settings", quality="high"); wait_pipelines(m)
        for label, v in views.items():
            capture(m, d, f"{label}_off", caps, gi="off", **v)
            capture(m, d, f"{label}_lit", caps, **v)
        # Tier low (the iGPU's default) beside high: per draw and per grid, no per-pixel probes.
        m.json("gi_settings", quality="low"); wait_pipelines(m)
        for label in ("graveyard", "crypt_inside", "overview"):
            capture(m, d, f"{label}_lit_low", caps, **views[label])
        m.json("gi_settings", quality="high"); wait_pipelines(m)
        capture(m, d, "graveyard_irradiance", caps, gi="irradiance", **views["graveyard"])
        capture(m, d, "crypt_inside_irradiance", caps, gi="irradiance", **views["crypt_inside"])
        capture(m, d, "graveyard_probes", caps, gi="probes", **views["graveyard"])
        res["captures"] = caps
        # A wave in real time: every tier, alternated.
        m.json("play_command", action="call_wave")
        perf = {}
        m.call("time_control", action="resume")
        for q in ("off", "low", "high", "off", "low", "high"):
            m.json("gi_settings", quality=q)
            time.sleep(3.0)
            cap = m.json("perf_capture", frames=180)
            passes = cap.get("gpu_passes_ms") or {}
            perf.setdefault(q, []).append({"gpu_ms": cap.get("gpu_ms"), "frame_ms": cap.get("frame_ms"),
                                           "scene": passes.get("scene"), "scene_blend": passes.get("scene_blend"), "frames": cap.get("frames")})
            print(f"  quality {q}: gpu {json.dumps(cap.get('gpu_ms'))} scene {json.dumps(passes.get('scene'))}")
        m.call("time_control", action="pause")
        res["perf"] = perf
        res["perf_errors"] = m.json("perf_frame").get("errors")
        res["gi_info"] = m.json("gi_info")
        m.request("tools/call", {"name": "world_delete", "arguments": {"path": "/crypt"}})
        m.request("tools/call", {"name": "project_delete", "arguments": {"path": "assets/recipes/crypt_hollow.json"}})
    res["validation_errors"] = eng.validation_errors()
    # The variant was this run's: it never stays in the game's assets.
    leftover = ROOT / "games" / "sepulcra" / "assets" / "recipes" / "crypt_hollow.json"
    if leftover.exists():
        leftover.unlink()
    return res


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7630, help="uses this port and the next two")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--game", default=str(GAME))
    ap.add_argument("--out", default=None)
    ap.add_argument("--skip-sepulcra", action="store_true")
    ap.add_argument("--skip-yard", action="store_true")
    args = ap.parse_args()
    out = Path(args.out) if args.out else ROOT / "target" / "gi-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    summary = {"adapter": args.adapter}
    ok = True
    if not args.skip_yard:
        print("1-4. yard: bake, determinism, leaks, staleness, write policy, sight")
        y = summary["yard"] = part_yard(args, out)
        ok &= y["status"] == "fresh" and y["same_bytes"] and y["again"]["cached"] and y["again"]["hash"] == y["hash"]
        ok &= y["shed_dark"] and y["hut_lit_by_bounce"]
        ok &= y["stale_after_move"] == ["geometry"] and y["fresh_after_move_back"] == "fresh" and y["stale_after_env"] == ["environment"]
        ok &= all(y["reserved"].values())
        ok &= y.get("after_undo") == "unbaked" and (y.get("rebake_after_undo") or {}).get("cached") is True and y.get("after_rebake") == "fresh"
        ok &= y["validation_errors"] == 0 and y["validation_errors_other"] == 0
        # The brazier's bounce is there (on the floor and walls by it), and the iGPU defaults to tier low.
        ok &= max(c["bounce"] for c in y["corner"]) > 0.002
        ok &= y["settings_default"]["quality"] == ("low" if args.adapter == "integrated" else "high")
    if not args.skip_sepulcra:
        print("5. sepulcra")
        s = summary["sepulcra"] = part_sepulcra(args, out)
        ok &= s["volume"]["status"] == "fresh" and s["validation_errors"] == 0
    summary["ok"] = bool(ok)
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"summary: {rel(out / 'summary.json')}  ok={ok}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

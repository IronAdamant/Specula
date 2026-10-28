#!/usr/bin/env python3
"""Effects and decals check (M4 "Particle/VFX system" and decals; docs/design/VFX.md).

1. **Cosmetic.** Two fresh engines (paused, stepped) build the same world with effects and decals
   (a fire, a moving entity with a world-space trail, smoke, motes, decals) and run the same ticks,
   one with `--vfx on`, one with `--vfx off`; the effects engine also renders captures in between.
   Their `world_hash` must agree on every tick checked.
2. **Repeatable.** A third fresh engine runs the same script; its captures must equal the first
   engine's byte for byte (particles are a function of time; trails come from the stepped ticks).
3c. **Flash watch.** Two instances of an effect that passes the per-effect check alone, a quarter
   period apart, strobe 5 times a second together: the final image's flash watch must report it
   from captures (stepped, deterministic) and from a small window with a dim strobe; the calm
   Sepulcra session in part 4 must not trip it.
3b. **Bounds.** With the default budgets: a heavy `vfx_preview` stays within the work budget per
   frame, a burst whose size spikes is thinned to the fill cap after evaluation, an effect tagged
   outside `/vfx` is refused (claims on `/vfx` hold), particle strobes are refused (also through
   params), case aliases of one PNG share a texture and asset textures stop at 64, costly effects
   are thinned to `max_work`, lit decals stop at `decal_fill_cap`, and a stale `vfx_set` carries
   conflict help.
3. **Budgets.** With a tiny `[vfx]` budget (perf.toml written here), many heavy effects must be
   thinned to `max_particles` and the fill cap, reported as data (`over_particles`, `thinned`).
   Refusals and edges: an unknown effect name, a burst repeating faster than a frame, a strobing
   light (photosensitive); a decal with a float colour keeps `world_hash` through save and load; an
   effect whose PNG texture is missing draws once the file appears.
4. **Sepulcra.** The game hosted on the chosen adapter (the Intel iGPU by default), windowed: its
   effects (brazier fire and embers, rune motes, wraith trails, dust and soot, impacts, binding
   runes and cracks under walls) while a wave runs in real time; `perf_frame` GPU ms per pass
   (particles, decals, scene), the effects' budget use, frame pacing; captures with the `effects`
   overlay. Then two headless runs (`--vfx on` / `off`) of the same stepped game must keep the same
   world and game hashes.
5. **Validation.** Every engine must shut down with zero validation errors.

Usage: cargo build --release -p specula -p sepulcra-app && python scripts/vfx_check.py
       [--adapter integrated] [--port 7628] [--out DIR] [--skip-sepulcra]
Output (captures, packets, logs, summary.json) goes to target/vfx-check/<adapter>/. Look at every
PNG. Standard library only; output ASCII.
"""
import argparse
import base64
import hashlib
import json
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
    """A path for the summary: relative to the repository when it is inside it (privacy: no home
    folders in anything that may be committed)."""
    try:
        return str(Path(p).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return "<out>/" + Path(p).name


def world_ops():
    return [
        {"op": "spawn", "shape": "cube", "name": "ground", "position": [0, -0.5, 0], "scale": [40, 1, 40], "color": "#5b6470", "tags": ["ground"]},
        {"op": "spawn", "shape": "cube", "name": "pillar", "position": [-2, 1, 0], "scale": [1, 2, 1], "color": "#8c96a3"},
        {"op": "spawn", "shape": "group", "name": "fire", "position": [0, 0, 0], "vfx": "fire"},
        {"op": "spawn", "shape": "group", "name": "embers", "position": [0, 0.1, 0], "vfx": "embers"},
        {"op": "spawn", "shape": "group", "name": "smoke", "position": [0, 0.6, 0], "vfx": "smoke"},
        {"op": "spawn", "shape": "group", "name": "motes", "position": [-2, 1.5, 0], "vfx": {"effect": "motes", "params": {"tint": "#3fe0d0"}}},
        {"op": "spawn", "shape": "sphere", "name": "wisp", "position": [-6, 1.2, 3], "scale": 0.4, "color": "#9cff4f", "emissive": 2,
         "vfx": {"effect": "trail", "params": {"tint": "#9cff4f"}}},
        {"op": "spawn", "shape": "group", "name": "sparks", "position": [3, 1.5, -1], "vfx": "sparks"},
        {"op": "spawn", "shape": "group", "name": "scorch", "position": [2, 0, 1.5], "scale": [2, 1, 2], "decal": "scorch"},
        {"op": "spawn", "shape": "group", "name": "rune", "position": [-1.5, 0, 2.5], "scale": [2, 1, 2],
         "decal": {"texture": "rune_circle", "color": "#3fe0d0", "emissive": 2}},
        {"op": "spawn", "shape": "group", "name": "cracks", "position": [4, 0, 3], "scale": [2.5, 1, 2.5],
         "decal": {"texture": "cracks", "normal": "cracks_normal", "specular": 0.4, "roughness": 0.5}},
    ]


def scripted(mcp, out, capture, label):
    """The same world and ticks every time: returns {tick: hash} and the captures' digests."""
    mcp.json("world_env", tonemap=True, exposure=1.1, ambient_intensity=0.45, sun_intensity=0.6, background="#0b1020",
             bloom={"enabled": True}, shadow={"enabled": True})
    mcp.json("world_batch", ops=world_ops())
    hashes, digests = {}, []
    for k in range(6):
        # The wisp moves each step: its trail comes from the ticks (world space).
        mcp.json("world_set", path="/wisp", position=[-6 + 2.0 * k, 1.2 + 0.3 * (k % 2), 3])
        mcp.json("time_control", action="step", ticks=20)
        h = mcp.json("world_hash")
        hashes[h["tick"]] = h["state_hash"]
        if capture:
            r = mcp.call("sight_frame", width=640, height=400, eye=[5, 4, 9], target=[-0.5, 0.8, 0.5], fov_y_deg=55,
                         overlays=["effects"] if k == 5 else [])
            png = next(c for c in r["content"] if c["type"] == "image")["data"]
            packet = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
            raw = base64.b64decode(png)
            (out / f"{label}_{k}.png").write_bytes(raw)
            (out / f"{label}_{k}.json").write_text(json.dumps(packet, indent=1))
            digests.append(hashlib.sha256(raw).hexdigest())
    return hashes, digests


def part_cosmetic(args, out):
    res = {}
    runs = {}
    for label, flags in [("on", ["--vfx", "on"]), ("off", ["--vfx", "off"]), ("again", ["--vfx", "on"])]:
        d = out / f"world_{label}"
        d.mkdir(parents=True, exist_ok=True)
        with Engine(args.exe, args.port, d / "engine.log", "--headless", "--paused", "--adapter", args.adapter,
                    "--captures", str(d / "captures"), *flags, cwd=d) as eng:
            m = eng.mcp("vfx-check")
            hashes, digests = scripted(m, d, label != "off", label)
            info = m.json("vfx_info")
        runs[label] = {"hashes": hashes, "digests": digests, "validation_errors": eng.validation_errors(), "info": info}
        print(f"  world {label}: ticks {sorted(hashes)} validation errors {eng.validation_errors()}")
    res["hash_on_equals_off"] = runs["on"]["hashes"] == runs["off"]["hashes"]
    res["captures_repeat"] = runs["on"]["digests"] == runs["again"]["digests"] and len(runs["on"]["digests"]) == 6
    res["hashes"] = runs["on"]["hashes"]
    res["capture_info"] = runs["on"]["info"]["capture"]
    res["validation_errors"] = {k: v["validation_errors"] for k, v in runs.items()}
    pk = json.loads((out / "world_on" / "on_5.json").read_text())
    res["packet_effects"] = {"effects": len(pk.get("effects", {}).get("effects", [])), "decals": len(pk.get("effects", {}).get("decals", [])),
                             "overlay": pk.get("overlays", {}).get("effects")}
    print(f"  hash on == off: {res['hash_on_equals_off']}; captures repeat: {res['captures_repeat']}; packet {res['packet_effects']}")
    return res


def part_budgets(args, out):
    d = out / "budgets"
    d.mkdir(parents=True, exist_ok=True)
    perf = d / "perf.toml"
    perf.write_text("[vfx]\nmax_particles = 600\nfill_cap = 0.5\nmax_decals = 4\n")
    assets = d / "assets"
    (assets / "fx").mkdir(parents=True, exist_ok=True)
    late = assets / "fx" / "late.png"
    if late.exists():
        late.unlink()
    with Engine(args.exe, args.port, d / "engine.log", "--headless", "--paused", "--adapter", args.adapter,
                "--captures", str(d / "captures"), "--perf", str(perf), "--assets", str(assets), "--worlds", str(d / "worlds"), cwd=d) as eng:
        m = eng.mcp("vfx-budget")
        m.json("world_env", tonemap=True, background="#0b1020")
        heavy = {"emitters": [{"rate": 600, "lifetime": [1, 2], "speed": [0.5, 1.5], "spread_deg": 60, "size": 0.6, "color": "#ff7a1f",
                               "texture": "soft", "blend": "additive"}]}
        print("  vfx_set heavy:", m.text("vfx_set", name="heavy", effect=heavy)[:200])
        ops = [{"op": "spawn", "shape": "cube", "name": "ground", "position": [0, -0.5, 0], "scale": [40, 1, 40], "color": "#5b6470"}]
        for i in range(12):
            ops.append({"op": "spawn", "shape": "group", "name": f"h{i}", "position": [(i % 4) * 3 - 4.5, 0, (i // 4) * 3 - 3], "vfx": "heavy"})
        for i in range(10):
            ops.append({"op": "spawn", "shape": "group", "name": f"d{i}", "position": [i - 5, 0, 5], "decal": "scorch"})
        m.json("world_batch", ops=ops)
        m.json("time_control", action="step", ticks=90)
        r = m.call("sight_frame", width=640, height=400, eye=[0, 12, 16], target=[0, 0, 0])
        raw = base64.b64decode(next(c for c in r["content"] if c["type"] == "image")["data"])
        (d / "budget.png").write_bytes(raw)
        pk = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
        frame = pk["effects"]["frame"]
        check = json.loads(m.text("vfx_check", effect="heavy"))
        try:
            m.json("world_spawn", shape="group", name="bad", vfx="no_such_effect")
            refused = False
        except RuntimeError as e:
            refused = "unknown_effect" in str(e)
        edges = edge_probes(m, late)
    res = {
        "frame": frame,
        "drawn_within_budget": frame["particles"]["drawn"] <= 600,
        "thinned": frame["particles"]["thinned"] > 0 and (frame["over_particles"] or frame["over_fill"]),
        "decals_capped": frame["decals"] <= 4 and frame["decals_skipped"] >= 6,
        "check_flags_heavy": not check["ok"],
        "unknown_effect_refused": refused,
        "edges": edges,
        "validation_errors": eng.validation_errors(),
    }
    print(f"  budgets: drawn {frame['particles']['drawn']} (expected {frame['particles']['expected']}), thinned {frame['particles']['thinned']}, "
          f"over_particles {frame['over_particles']} over_fill {frame['over_fill']}, decals {frame['decals']} skipped {frame['decals_skipped']}; "
          f"vfx_check ok={check['ok']}; unknown effect refused={refused}")
    print(f"  edges: {edges}")
    return res


def refusal(m, tool, **kw):
    """The error code of a refused call (or "accepted")."""
    err, text = tool_call(m, tool, **kw)
    return error_code(text) if err else "accepted"


def error_code(text):
    try:
        return json.loads(text).get("error", "")
    except ValueError:
        return ""


def png_bytes(w, h, rgba):
    return png_image(w, h, lambda x, y: rgba)


def png_image(w, h, f):
    """An RGBA PNG of `f(x, y)` (4 values each)."""
    import struct, zlib
    raw = b"".join(b"\x00" + b"".join(bytes(f(x, y)) for x in range(w)) for y in range(h))
    chunk = lambda t, b: struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def edge_probes(m, late):
    # The review's edges, live: refusals, a save/load hash, a texture that appears late.
    res = {}
    dos = {"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 1e-7}], "lifetime": 10}]}
    res["fast_burst_refused"] = refusal(m, "vfx_set", name="dos", effect=dos) == "invalid_argument"
    strobe = {"emitters": [{"rate": 1}], "light": {"flicker": 0.8, "flicker_hz": 8}}
    res["strobe_refused"] = refusal(m, "vfx_set", name="strobe", effect=strobe) == "photosensitive"
    m.json("world_spawn", shape="group", name="tinted_decal", position=[0, 0, -6], decal={"texture": "rune_circle", "color": [0.3, 0.5, 0.7], "start": 0})
    before = m.json("world_hash")["state_hash"]
    m.call("world_save", name="vfx_edges", overwrite=True)
    m.json("world_load", name="vfx_edges")
    res["decal_colour_hash_kept"] = m.json("world_hash")["state_hash"] == before
    # The heavy effects would take the whole (tiny) particle budget.
    for i in range(12):
        m.call("world_delete", path=f"/h{i}")
    late_fx = {"emitters": [{"rate": 40, "lifetime": 1, "speed": 0.5, "size": 0.5, "texture": "fx/late.png", "color": "#ffffff"}]}
    m.json("vfx_set", name="late", effect=late_fx)
    m.json("world_spawn", shape="group", name="late_fx", position=[0, 1, 0], vfx="late")
    m.json("time_control", action="step", ticks=30)

    def drawn():
        r = m.call("sight_frame", width=320, height=200, eye=[0, 3, 6], target=[0, 1, 0])
        pk = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
        fx = [e for e in pk["effects"]["effects"] if e.get("effect") == "late"]
        return fx[0]["drawn"] if fx else 0, pk["effects"]["frame"].get("problems", [])

    missing, problems = drawn()
    late.write_bytes(png_bytes(8, 8, [255, 255, 255, 255]))
    appeared, _ = drawn()
    res["missing_texture_reported"] = missing == 0 and any("late.png" in p for p in problems)
    res["late_texture_drawn"] = appeared > 0
    # Textures the world names stay loaded however long they are out of view (each capture
    # scans; 10 scans without use frees a texture nothing names).
    loaded = m.json("vfx_info")["textures"]["loaded"]
    for _ in range(12):
        m.call("sight_frame", width=64, height=64, eye=[0, 3, 6], target=[0, 3, 20])
    res["named_textures_kept"] = m.json("vfx_info")["textures"]["loaded"] == loaded
    # A streak born out of view that reaches into it is drawn (its quad's stretch counts).
    streak = {"emitters": [{"rate": 400, "lifetime": 0.02, "speed": 20, "direction": [1, 0, 0], "size": 0.1, "stretch": 1,
                            "blend": "additive", "color": "#ffffff"}]}
    m.json("vfx_set", name="streak", effect=streak)
    m.json("world_spawn", shape="group", name="streak_fx", position=[-11.8, 0.5, 30], vfx="streak")
    m.json("time_control", action="step", ticks=2)
    r = m.call("sight_frame", width=320, height=180, eye=[0, 0.5, 40], target=[0, 0.5, 30])
    pk = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
    res["streak_into_view_drawn"] = any(e.get("effect") == "streak" and e["drawn"] > 0 for e in pk["effects"]["effects"])
    return res


class Game:
    """Drives Sepulcra: the centre layout plus walls on the graveyard gates."""

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



def tool_call(m, tool_name, **a):
    """(is_error, text) of a tool call (errors are data here, not exceptions)."""
    r = m.request("tools/call", {"name": tool_name, "arguments": a})
    return r.get("isError", False), next(c["text"] for c in r["content"] if c["type"] == "text")


tool = tool_call


def frame_of(m, **view):
    """A capture's effects report (vfx_info's `capture`: the packet leaves it out when empty)."""
    m.call("sight_frame", width=640, height=360, **view)
    return json.loads(m.text("vfx_info"))["capture"]


def part_bounds(args, out):
    """Work, fill, library, flashing, textures and conflicts, bounded as data (default [vfx]
    budgets): each a probe from the M4 review, checked by work counts where it can be."""
    d = out / "bounds"
    (d / "assets" / "fx").mkdir(parents=True, exist_ok=True)
    big = d / "assets" / "fx" / "big.png"
    big.write_bytes(png_bytes(64, 64, [255, 255, 255, 255]))
    for i in range(70):
        f = d / "assets" / "fx" / f"t{i}.png"
        if not f.exists():
            f.write_bytes(png_bytes(8, 8, [200, 200, 255, 255]))
    res = {}
    with Engine(args.exe, args.port, d / "engine.log", "--headless", "--paused", "--adapter", args.adapter, "--assets", str(d / "assets"),
                "--captures", str(d / "captures"), "--worlds", str(d / "worlds"), cwd=d) as eng:
        a = eng.mcp("vfx-bounds-a")
        b = eng.mcp("vfx-bounds-b")
        a.call("agent_hello", name="alpha")
        b.call("agent_hello", name="beta")
        budget = json.loads(a.text("vfx_info"))["budget"]
        max_work = budget["max_work"]
        a.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": "ground", "position": [0, -0.5, 0], "scale": [40, 1, 40], "tags": ["ground"]}])
        # 1. Previews are bounded like frames (noise + stick at 10x rate: once a 23-34 s stall).
        base = {"rate": 2000, "lifetime": 10, "size": 0.05, "speed": [0, 2], "shape": {"kind": "sphere", "radius": 2},
                "noise": {"strength": 1}, "collide": {"plane": -1, "mode": "stick"}, "gravity": [0, -10, 0]}
        noisy = {"emitters": [dict(base, name="a"), dict(base, name="b")]}
        t0 = time.perf_counter()
        err, txt = tool(a, "vfx_preview", definition=noisy, params={"rate": 10}, frames=16, tile=160)
        took = time.perf_counter() - t0
        frames = json.loads(txt)["frames"] if not err else []
        res["preview_work_bounded"] = bool(frames) and all(f["work"] <= 2 * max_work for f in frames)
        res["preview_seconds"] = round(took, 2)
        res["preview_fast"] = took < 5.0
        # 1b. Previews are queued and rendered a frame per step within the studio's budget (M4
        # audit: one call rendered every tile at once and stalled the window's frames). Two agents'
        # previews at once both answer, with the same facts as one alone (no state leaks between
        # interleaved steps).
        import threading
        alone = json.loads(tool(a, "vfx_preview", effect="fire", frames=6, tile=128)[1])["frames"]
        got = {}

        def run(m, key):
            got[key] = tool(m, "vfx_preview", effect="fire", frames=6, tile=128)

        ts = [threading.Thread(target=run, args=(m, k)) for m, k in ((a, "a"), (b, "b"))]
        for t in ts:
            t.start()
        for t in ts:
            t.join()
        res["previews_interleave"] = all(not got[k][0] and json.loads(got[k][1])["frames"] == alone for k in ("a", "b"))
        # 1c. A playtest's preview step still answers after the agent that started the playtest
        # closed its session (review of the M4 fixes: it failed with session_closed).
        st = eng.mcp("vfx-pt-starter")
        st.call("agent_hello", name="pt-starter", role="lead")
        spec = {"name": "preview_after_close", "ticks": 600, "steps": [
            {"at": 300, "call": "vfx_preview", "args": {"effect": "fire", "frames": 4, "tile": 96}},
            {"at": 300, "call": "world_list"}]}
        rid = st.json("playtest_run", spec=spec, wait_s=0).get("id")
        st.close()
        rep = {}
        for _ in range(300):
            rep = b.json("playtest_report", id=rid, response_format="detailed")
            if rep.get("status") not in ("running", None):
                break
            time.sleep(0.1)
        res["playtest_preview_outlives_session"] = rep.get("status") == "passed"
        if not res["playtest_preview_outlives_session"]:
            print("  playtest preview after close:", rep.get("status"), json.dumps(rep.get("failures"))[:400])
        # 2. Fill: a burst whose size spikes mid-life (the estimate sees its mean of size^2, the
        # evaluation sees the spike): thinned to the cap after evaluation.
        spike = {"emitters": [{"bursts": [{"count": 300, "cycles": 1}], "lifetime": 1, "speed": 0, "shape": {"kind": "sphere", "radius": 1},
                               "size": [[0, 0.05], [0.4, 0.05], [0.45, 4], [0.55, 4], [0.6, 0.05], [1, 0.05]], "alpha": [[0, 0], [0.1, 1], [0.9, 1], [1, 0]],
                               "color": "#606060", "blend": "alpha"}]}
        a.json("vfx_set", name="spike", effect=spike)
        now = frame_of(a, eye=[0, 3, 8], target=[0, 1, 0])["time_s"]
        a.json("world_spawn", shape="group", name="spiky", position=[0, 1, 0], vfx={"effect": "spike", "start": now - 0.5})
        f = frame_of(a, eye=[0, 3, 8], target=[0, 1, 0])
        cap = budget["fill_cap"]
        res["fill_capped_after_evaluation"] = f["fill_screens"] <= cap * 1.02 and f["particles"]["fill_thinned"] > 0
        res["fill"] = [f["fill_screens"], f["particles"]["fill_thinned"]]
        a.call("world_delete", path="/spiky")
        # 3. The library is /vfx's children, reserved data of the write policy: an effect tagged
        # elsewhere is refused (reserved), and can't shadow alpha's claimed one.
        a.json("vfx_set", name="glow", effect={"emitters": [{"rate": 5, "color": "#00ff00"}]})
        a.json("claim_take", path="/vfx")
        err1, t1 = tool(b, "world_spawn", shape="group", name="glow", tags=["vfx_effect"], props={"vfx_effect": {"emitters": [{"rate": 50}]}})
        err2, t2 = tool(b, "world_batch", ops=[{"op": "spawn", "shape": "group", "name": "b"},
                                                {"op": "spawn", "shape": "group", "name": "glow", "parent": "/b", "tags": ["vfx_effect"],
                                                 "props": {"vfx_effect": {"emitters": [{"rate": 50}]}}}])
        err3, _ = tool(b, "vfx_set", name="glow", effect={"emitters": [{"rate": 50, "color": "#ff00ff"}]})
        g = json.loads(a.text("vfx_get", name="glow"))
        res["library_tag_refused"] = err1 and error_code(t1) == "reserved" and err2 and error_code(t2) == "reserved"
        res["library_claim_holds"] = err3 and g.get("path") == "/vfx/glow" and g["effect"]["emitters"][0]["color"] == "#00ff00"
        a.json("claim_release", all=True)
        # 4. Photosensitivity covers particles: a 10 Hz particle strobe, and params that make a
        # calm effect strobe, are refused.
        strobe = {"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 2, "emissive": 6,
                                "color": "#ffffff", "alpha": [[0, 1], [1, 1]], "blend": "additive"}]}
        err, t = tool(a, "vfx_set", name="strobe", effect=strobe)
        calm = {"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.25}], "lifetime": 0.05, "size": 0.05, "color": "#ffffff",
                              "alpha": [[0, 1], [1, 1]], "blend": "additive"}]}
        a.json("vfx_set", name="calm", effect=calm)
        err4, t4 = tool(a, "world_spawn", shape="group", name="loud", vfx={"effect": "calm", "params": {"size": 20, "rate": 4}})
        res["particle_strobe_refused"] = err and error_code(t) == "photosensitive"
        res["params_strobe_refused"] = err4 and error_code(t4) == "photosensitive"
        # 5. Asset textures: case aliases of one file share one texture (Windows); past 64
        # distinct ones, the rest are refused as data.
        names = ["fx/big.png", "FX/big.png", "fx/BIG.png", "Fx/Big.png", "fX/bIg.png"]
        before = json.loads(a.text("vfx_info"))["textures"]["loaded"]
        ops = [{"op": "spawn", "shape": "group", "name": f"al{i}", "position": [i * 0.5 - 1, 0, 0], "decal": {"texture": n}} for i, n in enumerate(names)]
        a.json("world_batch", ops=ops)
        frame_of(a, eye=[0, 5, 5], target=[0, 0, 0])
        loaded = json.loads(a.text("vfx_info"))["textures"]["loaded"]
        res["aliases_share_one_texture"] = (loaded - before == 1) if sys.platform == "win32" else True
        a.json("world_batch", ops=[{"op": "delete", "path": f"/al{i}"} for i in range(len(names))])
        ops = [{"op": "spawn", "shape": "group", "name": f"tx{i}", "position": [(i % 10) - 5, 0, (i // 10) - 3], "decal": {"texture": f"fx/t{i}.png"}} for i in range(70)]
        a.json("world_batch", ops=ops)
        frame_of(a, eye=[0, 12, 8], target=[0, 0, 0])
        info = json.loads(a.text("vfx_info"))
        res["textures_capped"] = info["textures"]["loaded"] <= 64 + 8 and any("texture_budget" in p for p in info["capture"]["problems"])
        a.json("world_batch", ops=[{"op": "delete", "path": f"/tx{i}"} for i in range(70)])
        # 6. CPU work: 16 noise + stick instances are thinned to [vfx] max_work.
        dear = {"rate": 300, "lifetime": 5, "size": 0.05, "speed": [0, 2], "shape": {"kind": "sphere", "radius": 1.5},
                "noise": {"strength": 1}, "gravity": [0, -10, 0], "collide": {"plane": -0.9, "mode": "stick"}}
        a.json("vfx_set", name="dear", effect={"emitters": [dear]})
        a.json("world_batch", ops=[{"op": "spawn", "shape": "group", "name": f"dear{i}", "position": [(i % 4) - 1.5, 1, (i // 4) - 1.5], "vfx": "dear"} for i in range(16)])
        a.json("time_control", action="step", ticks=200)
        f = frame_of(a, eye=[4, 4, 6], target=[0, 0.5, 0])
        res["work_capped"] = f["work"]["done"] <= 2 * max_work and f["work"]["over"]
        res["work"] = f["work"]
        res["cpu_ms"] = f["cpu_ms"]
        a.json("world_batch", ops=[{"op": "delete", "path": f"/dear{i}"} for i in range(16)])
        # 7. Decal fill: big lit decals near the camera stop at decal_fill_cap (lit counts double).
        a.json("world_batch", ops=[{"op": "spawn", "shape": "group", "name": f"big{i}", "position": [0, 0, 0], "scale": [12, 1, 12],
                                    "decal": {"texture": "rune_circle", "mode": "lit"}} for i in range(12)])
        f = frame_of(a, eye=[0, 4, 4], target=[0, 0, 0])
        res["decal_fill_capped"] = f["decals"] <= 1 or f["decal_fill_screens"] <= budget["decal_fill_cap"] * 1.01
        res["decal_fill"] = [f["decals"], f["decal_fill_screens"]]
        # 9. Fill: one big near particle doesn't take the rest with it (skip, not stop).
        a.json("world_batch", ops=[{"op": "delete", "path": f"/big{i}"} for i in range(12)])
        a.json("vfx_set", name="puff", effect={"emitters": [{"bursts": [{"count": 1}], "lifetime": 10, "speed": 0, "size": 6, "color": "#808080", "blend": "alpha", "alpha": [[0, 0.5], [1, 0.5]]}]})
        a.json("vfx_set", name="dots", effect={"emitters": [{"rate": 200, "lifetime": 5, "speed": [0.5, 1], "size": 0.05, "color": "#ffaa00", "emissive": 2, "alpha": [[0, 0], [0.1, 1], [1, 0]]}]})
        now = frame_of(a, eye=[0, 1, 2.5], target=[0, 1, -6])["time_s"]
        a.json("world_batch", ops=[{"op": "spawn", "shape": "group", "name": "dots", "position": [0, 1, -6], "vfx": {"effect": "dots", "start": now - 6}},
                                   {"op": "spawn", "shape": "group", "name": "puff", "position": [0, 1, 0], "vfx": {"effect": "puff", "start": now - 1}}])
        f = frame_of(a, eye=[0, 1, 2.5], target=[0, 1, -6])
        res["fill_skips_one_big"] = f["particles"]["fill_thinned"] < 0.5 * f["particles"]["live"]
        res["fill_one_big"] = [f["particles"]["live"], f["particles"]["drawn"], f["particles"]["fill_thinned"], f["fill_screens"]]
        a.json("world_batch", ops=[{"op": "delete", "path": "/dots"}, {"op": "delete", "path": "/puff"}])
        # 10. A legal but costly effect is checked fast (flash() bounded; no re-parse per refresh).
        burst = {"count": 1, "cycles": 0, "interval": 0.008333334}
        costly = {"emitters": [{"name": f"e{k}", "bursts": [burst] * 8, "lifetime": 6, "size": 0.01, "color": "#101010"} for k in range(8)]}
        t0 = time.perf_counter()
        err, _ = tool(a, "vfx_set", name="costly", effect=costly)
        took = time.perf_counter() - t0
        res["costly_vfx_set_seconds"] = round(took, 3)
        res["costly_vfx_set_fast"] = (not err) and took < 1.0
        # 11. Params checked wherever they come from: vfx_set re-checks the instances playing an
        # effect; an instance loaded with flashing params isn't drawn, and the frame says so.
        calm = {"emitters": [{"rate": 1, "lifetime": 1, "size": 0.05, "color": "#202020"}]}
        small = {"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 0.15, "emissive": 6, "color": "#ffffff", "alpha": [[0, 1], [1, 1]]}]}
        a.json("vfx_set", name="e1", effect=calm)
        a.json("world_spawn", shape="group", name="inst", position=[0, 1, 0], vfx={"effect": "e1", "params": {"size": 10}})
        err, t = tool(a, "vfx_set", name="e1", effect=small)
        res["vfx_set_rechecks_instances"] = err and error_code(t) == "photosensitive"
        a.call("world_delete", path="/inst")
        a.json("vfx_set", name="small", effect=small)
        a.json("world_spawn", shape="group", name="inst2", position=[0, 1, 0], vfx={"effect": "small"})
        a.call("world_save", name="flashy", overwrite=True)
        wf = d / "worlds" / "flashy" / "world.json"
        text = wf.read_text(encoding="utf-8")
        edited = text.replace('"vfx":{"effect":"small"', '"vfx":{"effect":"small","params":{"size":10}', 1)
        wf.write_text(edited, encoding="utf-8")
        a.json("world_load", name="flashy", force=True)
        f = frame_of(a, eye=[0, 1, 3], target=[0, 1, 0])
        res["loaded_flashing_not_drawn"] = edited != text and f["flashing"] >= 1 and any("photosensitive" in p for p in f["problems"])
        # 12. A texture refused for the budget loads once there is room.
        ops = [{"op": "spawn", "shape": "group", "name": f"rt{i}", "position": [(i % 10) - 5, 0, (i // 10) - 3], "decal": {"texture": f"fx/t{i}.png"}} for i in range(70)]
        a.json("world_batch", ops=ops)
        frame_of(a, eye=[0, 12, 8], target=[0, 0, 0])
        a.json("world_batch", ops=[{"op": "delete", "path": f"/rt{i}"} for i in range(20)])
        for _ in range(15):
            f = frame_of(a, eye=[0, 12, 8], target=[0, 0, 0])
        res["refused_texture_retried"] = not any("texture_budget" in p for p in f["problems"])
        a.json("world_batch", ops=[{"op": "delete", "path": f"/rt{i}"} for i in range(20, 70)])
        # 8. Conflict help on a stale vfx_set, as world_set gives.
        v = json.loads(a.text("vfx_set", name="c1", effect={"emitters": [{"rate": 1}]}))["version"]
        a.text("vfx_set", name="c1", effect={"emitters": [{"rate": 2}]})
        err, t = tool(a, "vfx_set", name="c1", effect={"emitters": [{"rate": 3}]}, expect_version=v)
        res["conflict_help"] = err and "conflict" in t and "theirs" in t
    res["validation_errors"] = eng.validation_errors()
    print(f"  bounds: {res}")
    return res


def flash_gpu_ms(win):
    """GPU ms of the flash watch's passes in a window's perf (its state, blocks and copies)."""
    passes = (win.get("gpu") or {}).get("passes") or {}
    parts = [passes.get(k) for k in ("flash", "flash_cells", "flash_copy")]
    return round(sum(p for p in parts if p), 3) if any(parts) else None


def strobe_pair(m, name):
    """Two instances of one effect that passes alone (2.5 flashes a second each), a quarter
    period apart: together 5 a second, which only the final image shows."""
    pulse = {"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.4}], "lifetime": 0.12, "speed": 0, "size": 1.2,
                           "color": name["colour"], "emissive": name["emissive"], "alpha": [[0, 1], [1, 1]], "texture": "soft", "blend": "additive"}]}
    m.json("vfx_set", name="pulse", effect=pulse)
    now = json.loads(m.text("vfx_info"))["capture"]["time_s"]
    m.json("world_batch", ops=[
        {"op": "spawn", "shape": "cube", "name": "backdrop", "position": [0, 1, -0.6], "scale": [6, 4, 0.2], "color": "#101010"},
        {"op": "spawn", "shape": "group", "name": "p1", "position": [0, 1, 0], "vfx": {"effect": "pulse", "start": now}},
        {"op": "spawn", "shape": "group", "name": "p2", "position": [0, 1, 0], "vfx": {"effect": "pulse", "start": now + 0.2}},
    ])


def checker_sheet(path, a, b):
    """A 2-frame flipbook (128 x 64): an 8 px checkerboard of greys `a`/`b`, then inverted."""
    def px(x, y):
        on = ((x % 64) // 8 + y // 8) % 2 == 0
        if x >= 64:
            on = not on
        g = a if on else b
        return [g, g, g, 255]
    path.write_bytes(png_image(128, 64, px))


CHECKER = {"emitters": [{"bursts": [{"count": 1}], "lifetime": 10, "speed": 0, "size": 2.4, "color": "#ffffff", "texture": "fx/checker.png",
                         "flipbook": {"cols": 2, "rows": 1, "frames": 2, "fps": 10}, "blend": "alpha", "alpha": [[0, 1], [1, 1]]}]}


def part_watch(args, out):
    """The flash watch on the final image: a two-instance strobe (each instance passes the
    per-effect check) and an inverting checkerboard (an asset flipbook: its mean doesn't change)
    are reported from captures and from the window; strobing bars where the old halving blits
    were blind are reported from a 1280x720 window; the calm Sepulcra session (part 4) must not
    be. A machine whose window can't run the watch fails this part."""
    d = out / "watch"
    d.mkdir(parents=True, exist_ok=True)
    (d / "assets" / "fx").mkdir(parents=True, exist_ok=True)
    checker_sheet(d / "assets" / "fx" / "checker.png", 90, 150)
    res = {}
    # Captures: offscreen, stepped tick by tick at one camera (deterministic: their own pixels at
    # simulation time).
    with Engine(args.exe, args.port, d / "engine.log", "--headless", "--paused", "--adapter", args.adapter,
                "--captures", str(d / "captures"), cwd=d) as eng:
        m = eng.mcp("vfx-watch")
        m.json("world_env", tonemap=True, background="#000000")
        res["alone_passes"] = refusal(m, "vfx_check", definition={"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.4}], "lifetime": 0.12, "speed": 0, "size": 1.2, "color": "#ffffff", "emissive": 4, "alpha": [[0, 1], [1, 1]]}]}) == "accepted"
        strobe_pair(m, {"colour": "#ffffff", "emissive": 4})
        over_at = None
        for i in range(75):
            m.json("time_control", action="step", ticks=1)
            m.call("sight_frame", width=320, height=180, eye=[0, 1, 3], target=[0, 1, 0], max_subjects=1)
            cap = json.loads(m.text("vfx_info"))["flash"]["capture"]
            if cap["over"] and over_at is None:
                over_at = i
        watch = json.loads(m.text("vfx_info"))["flash"]
        res["capture_reported"] = over_at is not None and any(e["source"] == "capture" for e in watch["events"])
        res["capture_report"] = watch["capture"]
        res["capture_cpu_ms"] = watch["cpu_ms"]["capture"]
    res["validation_errors"] = eng.validation_errors()
    # Captures of an inverting checkerboard (an asset flipbook at 10 frames a second: the per-effect
    # check can't see its frames, and its mean never changes): the watch's pattern count reports it.
    with Engine(args.exe, args.port, d / "engine_checker.log", "--headless", "--paused", "--adapter", args.adapter,
                "--assets", str(d / "assets"), "--captures", str(d / "captures_checker"), cwd=d) as eng:
        m = eng.mcp("vfx-watch-checker")
        m.json("world_env", tonemap=True, background="#000000")
        m.json("vfx_set", name="checker", effect=CHECKER)
        m.json("world_spawn", shape="group", name="c", position=[0, 1, 0], vfx={"effect": "checker", "start": 0})
        best = {}
        for i in range(60):
            m.json("time_control", action="step", ticks=1)
            m.call("sight_frame", width=320, height=180, eye=[0, 1, 3], target=[0, 1, 0], max_subjects=1)
            cap = json.loads(m.text("vfx_info"))["flash"]["capture"]
            if cap["pattern_flashes"] >= best.get("pattern_flashes", 0):
                best = cap
        res["capture_checker_reported"] = bool(best.get("over")) and best.get("pattern_flashes", 0) > 3
        res["capture_checker"] = best
    res["validation_errors_checker"] = eng.validation_errors()
    # The window: a small window (320x180 on the screen) with a dim, low-contrast strobe for two
    # seconds (enough to be reported: a 10% luminance change over 2.8% of the view).
    with Engine(args.exe, args.port + 1, d / "engine_window.log", "--adapter", args.adapter, "--size", "320x180",
                "--captures", str(d / "captures_window"), cwd=d) as eng:
        m = eng.mcp("vfx-watch-window")
        m.json("world_env", tonemap=True, background="#000000")
        strobe_pair(m, {"colour": "#808080", "emissive": 1.2})
        m.call("window_view", eye=[0, 1, 3], target=[0, 1, 0])
        time.sleep(2.5)
        perf = m.json("perf_frame")
        watch = perf.get("flash") or {}
        win = perf.get("window") or {}
        res["window_reported"] = any(e.get("source") == "window" for e in watch.get("events") or []) or (watch.get("window") or {}).get("over", False)
        res["window_report"] = watch.get("window")
        res["window_unsupported"] = watch.get("window_unsupported")
        res["window_gpu_ms"] = flash_gpu_ms(win)
        res["window_cpu_ms"] = (watch.get("cpu_ms") or {}).get("window")
        res["flash_error_in_perf_frame"] = any(e.get("error") == "flash" for e in perf.get("errors") or [])
        m.json("world_batch", ops=[{"op": "delete", "path": "/p1"}, {"op": "delete", "path": "/p2"}])
    res["validation_errors_window"] = eng.validation_errors()
    # The window and the dim inverting checkerboard (320x180, two seconds).
    with Engine(args.exe, args.port + 1, d / "engine_window_checker.log", "--adapter", args.adapter, "--size", "320x180",
                "--assets", str(d / "assets"), "--captures", str(d / "captures_window_checker"), cwd=d) as eng:
        m = eng.mcp("vfx-watch-window-checker")
        m.json("world_env", tonemap=True, background="#000000")
        m.json("vfx_set", name="checker", effect=CHECKER)
        m.json("world_spawn", shape="group", name="c", position=[0, 1, 0], vfx={"effect": "checker"})
        m.call("window_view", eye=[0, 1, 3], target=[0, 1, 0])
        best = {}
        for _ in range(25):
            time.sleep(0.1)
            w = (m.json("perf_frame").get("flash") or {}).get("window") or {}
            if w.get("pattern_flashes", 0) >= best.get("pattern_flashes", 0):
                best = w
        res["window_checker_reported"] = bool(best.get("over")) and best.get("pattern_flashes", 0) > 3
        res["window_checker"] = best
        m.call("world_delete", path="/c")
    res["validation_errors_window_checker"] = eng.validation_errors()
    # The window at 1280x720 with dim bars strobing at 10 Hz in the rows the old halving blits
    # skipped (321-398, 5.8% of the view), and the same bars 16 rows lower.
    for shift in (0, 16):
        with Engine(args.exe, args.port + 1, d / f"engine_bars_{shift}.log", "--adapter", args.adapter, "--size", "1280x720",
                    "--captures", str(d / f"captures_bars_{shift}"), cwd=d) as eng:
            m = eng.mcp("vfx-watch-bars")
            m.json("world_env", tonemap=False, background="#000000", bloom={"enabled": False})
            half_h = 10 * 0.5773502691896257

            def y_of(row):
                return (360 - row) / 360 * half_h
            ops = []
            for k, (r0, r1) in enumerate([(320, 336), (352, 368), (384, 400)]):
                a, b = y_of(r0 + shift + 1), y_of(r1 + shift - 1)
                ops.append({"op": "spawn", "shape": "cube", "name": f"bar{k}", "position": [0, (a + b) / 2, 0], "scale": [30, a - b, 0.01], "color": "#808080", "emissive": 1})
            m.json("world_batch", ops=ops)
            m.call("window_view", eye=[0, 0, 10], target=[0, 0, 0], fov_y_deg=60)
            time.sleep(0.5)
            t0, on, over, best = time.time(), True, False, {}
            while time.time() - t0 < 2.0:
                on = not on
                m.json("world_batch", ops=[{"op": "set", "path": f"/bar{k}", "position": [0 if on else 100, ops[k]["position"][1], 0]} for k in range(3)])
                time.sleep(0.05)
                w = (m.json("perf_frame").get("flash") or {}).get("window") or {}
                if w.get("flashes", 0) >= best.get("flashes", 0):
                    best = w
                over |= bool(w.get("over"))
            res[f"bars_{shift}_reported"] = over
            res[f"bars_{shift}"] = best
        res[f"validation_errors_bars_{shift}"] = eng.validation_errors()
    print(f"  watch: {res}")
    return res

def part_sepulcra(args, out):
    d = out / "sepulcra"
    d.mkdir(parents=True, exist_ok=True)
    res = {}
    extra = ["--adapter", args.adapter, "--size", "1280x720", "--captures", str(d / "captures"), "--", "--seed", "7", "--mute"]
    with Engine(args.game, args.port + 1, d / "engine.log", *extra, cwd=d) as eng:
        m = eng.mcp("vfx-sepulcra")
        res["gpu"] = m.json("engine_info")["gpu"]["name"]
        m.call("time_control", action="pause")
        m.json("play_new", seed="7")
        g = Game(m)
        g.drive()
        caps = []

        def capture(label, eye, target, fov=45, overlays=()):
            pk = m.frame(width=1280, height=720, eye=eye, target=target, fov_y_deg=fov, max_subjects=4, overlays=list(overlays))
            fx = pk.get("effects", {})
            caps.append({"label": label, "png": rel(pk["image"]["path"]), "packet": rel(pk["image"]["packet_path"]),
                         "effects": len(fx.get("effects", [])), "decals": len(fx.get("decals", [])), "warnings": pk["warnings"]})
            print(f"  capture {label}: effects {caps[-1]['effects']} decals {caps[-1]['decals']}")

        # Build phase: braziers burning, binding runes under the new walls.
        g.step(20)
        capture("build", eye=[16, 14, 20], target=[1, 0, 1], fov=40)
        while True:
            st = g.state()
            if st["outcome"] != "running" or (st["wave"] >= 4 and st["enemies"] >= 10):
                break
            g.drive()
            g.step(30)
        # A wave in real time: the window draws every effect; then perf.
        m.call("time_control", action="resume")
        time.sleep(6.0)
        perf = m.json("perf_frame")
        info = m.json("vfx_info")
        m.call("time_control", action="pause")
        win = perf.get("window") or {}
        res["perf"] = {
            "gpu_passes_ms": (win.get("gpu") or {}).get("passes"),
            "gpu_total_ms": (win.get("gpu") or {}).get("total_ms"),
            "draws": win.get("draws"),
            "frame_pacing": perf.get("frame_pacing"),
            "vfx": perf.get("vfx"),
            "errors": perf.get("errors"),
        }
        res["vfx_window"] = info["window"]
        watch = perf.get("flash") or {}
        res["flash_watch"] = {"window": watch.get("window"), "events": watch.get("events"), "cpu_ms": watch.get("cpu_ms"),
                              "gpu_ms": flash_gpu_ms(win), "unsupported": watch.get("window_unsupported")}
        res["calm"] = not (watch.get("window") or {}).get("over") and not [e for e in (watch.get("events") or []) if e.get("source") == "window"]
        print(f"  flash watch (calm session): {json.dumps(res['flash_watch'])[:300]}")
        print(f"  perf on {res['gpu']}: gpu total {res['perf']['gpu_total_ms']} passes {res['perf']['gpu_passes_ms']}")
        print(f"  vfx window: {json.dumps(info['window'])[:400]}")
        st = g.state()
        capture("wave_overview", eye=[38, 27, 52], target=[0, -9, 1])
        capture("wave_close", eye=[14, 12, 20], target=[0, 0, 2], overlays=["effects"])
        capture("wave_close_plain", eye=[14, 12, 20], target=[0, 0, 2])
        enemies = g.state(include=["enemies"], limit=200).get("enemies", {}).get("items", [])
        wraith = next((e for e in enemies if e.get("kind") == "wraith"), None)
        if wraith:
            x, y, z = wraith["pos"]
            capture("wraith", eye=[x + 6, y + 4, z + 7], target=[x, y + 1, z], fov=50)
        res["captures"] = caps
        res["at"] = {"tick": st["tick"], "wave": st["wave"], "enemies": st["enemies"]}
    res["validation_errors"] = eng.validation_errors()
    # The same stepped game with effects on and off: the same hashes.
    hashes = {}
    for flag in ("on", "off"):
        dd = d / f"hash_{flag}"
        dd.mkdir(parents=True, exist_ok=True)
        with Engine(args.game, args.port + 2, dd / "engine.log", "--headless", "--paused", "--adapter", args.adapter, "--vfx", flag,
                    "--captures", str(dd / "captures"), "--", "--seed", "7", "--mute", cwd=dd) as e2:
            m = e2.mcp("vfx-hash")
            m.json("play_new", seed="7")
            g = Game(m)
            seq = []
            for _ in range(40):
                g.drive()
                g.step(30)
                st = g.state()
                seq.append((st["tick"], st["state_hash"], m.json("world_hash")["state_hash"]))
                if flag == "on" and len(seq) % 10 == 0:
                    m.frame(width=320, height=200, eye=[16, 14, 20], target=[1, 0, 1])
        hashes[flag] = seq
        res[f"validation_errors_hash_{flag}"] = e2.validation_errors()
    res["hash_on_equals_off"] = hashes["on"] == hashes["off"]
    res["hash_ticks"] = len(hashes["on"])
    print(f"  sepulcra world+game hashes on == off over {len(hashes['on'])} checkpoints: {res['hash_on_equals_off']}")
    return res


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7628, help="uses this port and the next two")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--game", default=str(GAME))
    ap.add_argument("--out", default=None)
    ap.add_argument("--skip-sepulcra", action="store_true")
    args = ap.parse_args()
    out = Path(args.out) if args.out else ROOT / "target" / "vfx-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    summary = {"adapter": args.adapter}
    print("1-2. cosmetic and repeatable")
    summary["world"] = part_cosmetic(args, out)
    print("3. budgets")
    summary["budgets"] = part_budgets(args, out)
    print("3b. bounds (work, fill, library, flashing, textures, conflicts)")
    summary["bounds"] = part_bounds(args, out)
    print("3c. flash watch (final image: captures and the window)")
    summary["watch"] = part_watch(args, out)
    if not args.skip_sepulcra:
        print("4. sepulcra")
        summary["sepulcra"] = part_sepulcra(args, out)
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    w, b, s, bo, wa = summary["world"], summary["budgets"], summary.get("sepulcra", {}), summary["bounds"], summary["watch"]
    errors = list(w["validation_errors"].values()) + [b["validation_errors"], bo["validation_errors"]] + [v for k, v in wa.items() if k.startswith("validation_errors")] + [v for k, v in s.items() if k.startswith("validation_errors")]
    bounds_ok = all(v for k, v in bo.items() if isinstance(v, bool))
    # A GPU machine whose window can't run the watch fails: the watch is the safety net.
    watch_ok = (wa["alone_passes"] and wa["capture_reported"] and wa["window_reported"] and not wa["window_unsupported"]
                and wa["capture_checker_reported"] and wa["window_checker_reported"] and wa["bars_0_reported"] and wa["bars_16_reported"])
    ok = (w["hash_on_equals_off"] and w["captures_repeat"] and b["drawn_within_budget"] and b["thinned"] and b["decals_capped"]
          and b["check_flags_heavy"] and b["unknown_effect_refused"] and all(b["edges"].values()) and bounds_ok and watch_ok and all(e == 0 for e in errors)
          and (args.skip_sepulcra or (s["hash_on_equals_off"] and s["calm"] and any(c["effects"] > 0 for c in s["captures"]))))
    print(f"validation errors: {errors}; summary in {rel(out / 'summary.json')}")
    print("VFX CHECK", "PASSED" if ok else "FAILED")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

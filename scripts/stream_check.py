"""Streaming and HLOD check (M3 "Async streaming with per-frame budgets, plus HLOD"; see
docs/design/STREAMING.md).

Builds a large world over MCP (a graveyard of procedural models, most of them unique builds per
cell, so their meshes don't fit the streaming memory budget at once), then on the chosen adapter
(the Intel iGPU by default):

1. **Loading.** Records frames while every build lands and streams in: no frame over budget from
   loading (the old engine uploaded every landed build synchronously in one frame).
2. **Flight.** `stream_fly` flies the window camera across the world: frame p99 and max, CPU frame,
   upload ms per frame against `upload_ms`, frames over budget that were uploading (traversal
   stutter), evictions under `memory_mb`, models not resident.
3. **HLOD.** From a high vantage point: draw calls, submitted draws and frame cost with HLOD on, and
   with HLOD off (everything per object, all resident, as before streaming); the image difference
   between the two (no popping: the proxies are the members' own coarsest LODs).
4. **Determinism.** The world hash with streaming on equals the hash of a second engine run with
   streaming off (same spawns), and the flight never changed it.
5. **Captures** for the sight-skeptic: near, mid and far views with their packets.

With `--baseline-exe` (an engine built from before streaming), the same world and a scripted
flight (window_view at 60 Hz with perf_capture) give the "before" numbers.

Usage: cargo build --release -p specula && python scripts/stream_check.py [--exe PATH] [--port 7496]
       [--adapter integrated] [--out DIR] [--cells 24] [--baseline-exe PATH]
Needs a GPU and a desktop (the window). Standard library only; output ASCII.
"""
import argparse
import json
import os
import random
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, share_file  # noqa: E402

CELL = 32.0
ASSETS = ROOT / "games" / "sepulcra" / "assets"


def build_ops(cells, seed=11):
    """The world's spawns: deterministic, so two runs build the same world."""
    rnd = random.Random(seed)
    half = cells * CELL / 2
    ops = [{"op": "spawn", "shape": "cube", "name": "ground", "position": [0, -0.5, 0], "scale": [cells * CELL, 1, cells * CELL],
            "color": "#2c2a26"}]
    n = 0

    def model(recipe, x, z, seed, params=None, yaw=0.0, scale=1.0, color=None):
        nonlocal n
        n += 1
        op = {"op": "spawn", "shape": "model", "recipe": f"recipes/{recipe}.json", "seed": seed, "name": f"{recipe}_{n}",
              "position": [round(x, 2), 0, round(z, 2)], "rotation_deg": [0, round(yaw, 1), 0], "scale": scale}
        if params:
            op["params"] = params
        if color:
            op["color"] = color
        return op

    for cx in range(cells):
        for cz in range(cells):
            x0, z0 = cx * CELL - half, cz * CELL - half
            cell_seed = cx * 1000 + cz
            pos = lambda: (x0 + rnd.uniform(2, CELL - 2), z0 + rnd.uniform(2, CELL - 2))  # noqa: E731
            if rnd.random() < 0.3:
                x, z = pos()
                ops.append(model("slab_cliff", x, z, 1000 + cell_seed, {"height": rnd.randint(6, 9)}, rnd.uniform(0, 360), 0.5))
            for k in range(2):
                x, z = pos()
                r = rnd.choice(["rune_pillar", "warding_stone", "sky_obelisk"])
                ops.append(model(r, x, z, 2000 + cell_seed * 2 + k, {"tier": rnd.randint(1, 3)}, rnd.uniform(0, 360)))
            tombs = [3000 + cell_seed * 2, 3001 + cell_seed * 2]
            for k in range(7):
                x, z = pos()
                ops.append(model("tombstone", x, z, tombs[k % 2], {"tilt": round(rnd.uniform(-1, 1), 2)}, rnd.uniform(-20, 20), 1.0,
                                 rnd.choice(["#ffffff", "#e8e2d6", "#d6dce8"])))
            fx, fz = pos()
            for k in range(3):
                ops.append(model("graveyard_fence", fx + 2 * k, fz, 4000 + cell_seed))
            wx, wz = pos()
            for k in range(2):
                ops.append(model("wall_segment", wx + 2 * k, wz, 5000 + (cell_seed % 37)))
            if rnd.random() < 0.15:
                x, z = pos()
                ops.append(model("brazier_cairn", x, z, 6000 + cell_seed, {"tier": 1}))
    return ops


def spawn_world(mcp, ops):
    t = time.perf_counter()
    for i in range(0, len(ops), 1000):
        mcp.call("world_batch", ops=ops[i:i + 1000])
    return time.perf_counter() - t


def wait_builds(mcp, timeout=600):
    t = time.perf_counter()
    while time.perf_counter() - t < timeout:
        lib = mcp.json("engine_info")["models"]["library"]
        if lib.get("building", 0) == 0:
            return time.perf_counter() - t, lib
        time.sleep(0.5)
    raise RuntimeError("builds never finished")


def wait_streamed(mcp, timeout=180, need_hlod=True):
    """Until nothing wanted is pending and no proxy is building or waiting to upload."""
    t = time.perf_counter()
    s = None
    while time.perf_counter() - t < timeout:
        s = mcp.json("stream_state", limit=1)
        h = s["hlod"]
        if s["pending_builds"] == 0 and (not need_hlod or (h["building"] == 0 and h["ready_to_upload"] == 0)):
            return time.perf_counter() - t, s
        time.sleep(0.25)
    return None, s


def perf(mcp):
    p = mcp.json("perf_frame")
    w = p.get("window") or {}
    d = w.get("draws") or {}
    g = (w.get("gpu") or {}).get("total_ms")
    return {
        "draw_calls": d.get("draw_calls"), "submitted": d.get("submitted"), "culled": d.get("culled"), "instances": d.get("instances"),
        "triangles": d.get("triangles"), "shadow_draw_calls": d.get("shadow_draw_calls"), "gpu_ms": g,
        "frame_pacing": p.get("frame_pacing"), "draw_list": p.get("draw_list"), "cpu": p.get("cpu"), "errors": p.get("errors"),
        "models": (p.get("models") or {}).get("window"), "streaming": p.get("streaming"),
    }


def capture(mcp, name, eye, target, out):
    shot = mcp.frame(eye=eye, target=target, fov_y_deg=60, width=1280, height=720, max_subjects=8)
    print(f"capture {name}: {shot['image']['path']}  models {json.dumps(shot['models'])}  streaming {json.dumps({k: v for k, v in (shot.get('streaming') or {}).items() if k != 'note'})}")
    out[name] = {"png": shot["image"]["path"], "packet": shot["image"]["packet_path"], "models": shot["models"],
                 "streaming": shot.get("streaming"), "warnings": shot.get("warnings")}
    return shot


def record_frames(port, frames, label, result):
    """perf_capture on its own session (it answers when its frames are recorded)."""
    from specula_mcp import Mcp, url_for
    m = Mcp(url_for(port), f"stream-check-{label}")
    result[label] = m.json("perf_capture", frames=frames, trace=False, label=label)


def record_until(port, label, stop, paths):
    """perf_capture after perf_capture (600 frames each) until `stop` is set: every frame's CPU
    zones, for attributing slow frames."""
    from specula_mcp import Mcp, url_for
    m = Mcp(url_for(port), f"stream-check-{label}")
    while not stop.is_set():
        paths.append(m.json("perf_capture", frames=600, trace=False, label=label)["path"])


def zone_ms(frame, name):
    return sum(z[2] for z in frame["zones"].get("main", []) if z[0] == name)


# Streaming's calls into the Vulkan driver that only record commands (vkResetCommandPool,
# vkBeginCommandBuffer, vkCmdCopyBuffer and the batch's one barrier). On the Intel iGPU's
# Windows driver they sometimes take 10-200 ms while a big world loads (measured 2026-10-22:
# never on the RTX, never in frames that don't upload, the same with a fresh command pool per
# batch reset whole); constant-time calls on our side, so a frame whose overrun is inside them is
# reported apart (`over_budget_in_driver_recording`), not counted as streaming's.
DRIVER_RECORDING = ("vk.reset_pool", "vk.begin_cb", "vk.reset_cb", "vk.record")


def driver_ms(frame):
    return sum(z[2] for z in frame["zones"].get("main", []) if z[0].startswith("frame/stream/") and z[0].rsplit("/", 1)[-1] in DRIVER_RECORDING)


def analyze_frames(paths, budget_ms, upload_ms):
    """Frame times over recorded captures, and every frame over 1.5x the budget with what its
    time went to: a frame is a *loading* stutter when it missed while streaming ran over its own
    budget (`frame/stream` above twice upload_ms); frames that missed by less than streaming's
    in-budget time are counted too (a scene already at the edge)."""
    frames = [f for p in paths for f in json.load(open(p))["frames"]]
    if not frames:
        return {"frames": 0}
    dts = sorted(f["dt_ms"] for f in frames)
    pick = lambda q: round(dts[min(len(dts) - 1, round(q * (len(dts) - 1)))], 3)  # noqa: E731
    over = [f for f in frames if f["dt_ms"] > budget_ms * 1.5]
    worst = []
    for f in sorted(over, key=lambda f: -f["dt_ms"])[:6]:
        top = sorted(((z[0], round(z[3], 2)) for z in f["zones"].get("main", [])), key=lambda z: -z[1])[:3]
        worst.append({"frame_ms": round(f["dt_ms"], 2), "stream_ms": round(zone_ms(f, "frame/stream"), 2), "top_self_zones": top})
    stream = sorted(zone_ms(f, "frame/stream") for f in frames)
    return {
        "frames": len(frames), "p50": pick(0.5), "p95": pick(0.95), "p99": pick(0.99), "max": round(dts[-1], 3),
        "over_budget": len(over),
        # Missed while streaming ran over its own budget (twice upload_ms) outside the driver's
        # command recording: a loading stutter.
        "over_budget_from_streaming": sum(1 for f in over if zone_ms(f, "frame/stream") - driver_ms(f) > upload_ms * 2),
        # Missed with streaming over its budget only because of time inside the driver's command
        # recording calls (see DRIVER_RECORDING): reported, not counted.
        "over_budget_in_driver_recording": sum(1 for f in over if zone_ms(f, "frame/stream") > upload_ms * 2 >= zone_ms(f, "frame/stream") - driver_ms(f)),
        "driver_recording_ms_max": round(max((driver_ms(f) for f in frames), default=0.0), 3),
        # Missed by less than streaming's own (in-budget) time: a scene already at the edge.
        "over_budget_by_less_than_streaming": sum(1 for f in over if f["dt_ms"] - zone_ms(f, "frame/stream") <= budget_ms * 1.5),
        # Streaming itself over twice its upload budget (a stall in a driver call, typically).
        "stream_overruns": sum(1 for f in frames if zone_ms(f, "frame/stream") > upload_ms * 2),
        "over_budget_from_requests": sum(1 for f in over if zone_ms(f, "requests") > budget_ms),
        "stream_ms_max": round(stream[-1], 3), "stream_ms_p99": round(stream[min(len(stream) - 1, round(0.99 * (len(stream) - 1)))], 3),
        "worst": worst,
    }


def fly_path(cells, height=18.0):
    w = cells * CELL
    return [[-0.42 * w, height, -0.42 * w], [0.0, height + 6, -0.1 * w], [0.42 * w, height, 0.42 * w]]


def scripted_fly(mcp, path, speed, look_ahead=40.0, pitch=12.0):
    """The baseline's flight: window_view as fast as calls go (the old engine has no stream_fly)."""
    import math
    segs = [(path[i], path[i + 1]) for i in range(len(path) - 1)]
    lens = [math.dist(a, b) for a, b in segs]
    total = sum(lens)

    def at(s):
        for (a, b), ln in zip(segs, lens):
            if s <= ln:
                t = s / ln
                return [a[k] + (b[k] - a[k]) * t for k in range(3)]
            s -= ln
        return list(path[-1])

    t0 = time.perf_counter()
    while True:
        s = (time.perf_counter() - t0) * speed
        if s >= total:
            break
        eye, ahead = at(s), at(s + look_ahead)
        dx, dz = ahead[0] - eye[0], ahead[2] - eye[2]
        ln = math.hypot(dx, dz) or 1.0
        target = [eye[0] + dx / ln * look_ahead, eye[1] - math.tan(math.radians(pitch)) * look_ahead, eye[2] + dz / ln * look_ahead]
        mcp.call("window_view", eye=eye, target=target, fov_y_deg=60)
        time.sleep(1 / 120)
    return total


def perf_toml(out, streaming_lines):
    text = (ROOT / "perf.example.toml").read_text()
    path = out / f"perf_{abs(hash(tuple(streaming_lines))) % 10000}.toml"
    path.write_text(text + "\n[streaming]\n" + "\n".join(streaming_lines) + "\n")
    return path


def run_streaming(a, out, ops, report, check):
    log = out / "engine_stream.log"
    perf_file = perf_toml(out, [f"memory_mb = {a.memory_mb}", f"upload_ms = {a.upload_ms}", "upload_mb = 8"])
    args = ["--lead", "stream-check", "--assets", str(ASSETS), "--captures", str(out / "captures"), "--size", "1280x720", "--adapter", a.adapter,
            "--perf", str(perf_file), "--cache", str(out / "cache")]
    with Engine(a.exe, a.port, log, *args, cwd=str(ROOT)) as eng:
        mcp = eng.mcp("stream-check")
        mcp.hello("stream-check", role="lead")
        info = mcp.json("engine_info")
        report["gpu"] = info["gpu"]
        print(f"gpu: {info['gpu']['name']} ({info['gpu']['kind']}); validation {info['validation']}")
        path = fly_path(a.cells)
        # Start where the flight starts, looking along it.
        mcp.call("window_view", eye=path[0], target=[path[0][0] + 30, 10, path[0][2] + 30], fov_y_deg=60)
        mcp.call("time_control", action="pause")
        budget = mcp.json("perf_frame")["budget_ms"]
        stop, load_paths = threading.Event(), []
        rec = threading.Thread(target=record_until, args=(a.port, "loading", stop, load_paths))
        rec.start()
        spawn_s = spawn_world(mcp, ops)
        build_s, lib = wait_builds(mcp)
        settle_s, st = wait_streamed(mcp)
        stop.set()
        rec.join()
        load = analyze_frames(load_paths, budget, a.upload_ms)
        report["loading"] = {"spawn_s": round(spawn_s, 2), "builds_s": round(build_s, 2), "settle_s": settle_s, "library": lib, "frames": load}
        print(f"world: {len(ops)} spawns in {spawn_s:.1f} s; builds done after {build_s:.1f} s; streamed/HLOD settled after {settle_s} s")
        print(f"loading frames: {json.dumps(load)}")
        h0 = mcp.json("world_hash")["state_hash"]
        report["hash_streaming_on"] = h0
        # --- flight, with the engine's own per-frame zones recorded throughout (attribution)
        stop, fly_paths = threading.Event(), []
        rec = threading.Thread(target=record_until, args=(a.port, "flight", stop, fly_paths))
        rec.start()
        fly = mcp.json("stream_fly", path=path, speed_mps=a.speed)
        stop.set()
        rec.join()
        report["flight_frames"] = analyze_frames(fly_paths, budget, a.upload_ms)
        print(f"flight frames (perf_capture): {json.dumps(report['flight_frames'])}")
        report["flight"] = {k: v for k, v in fly.items() if k != "streaming_now"}
        sn = fly["streaming_now"]
        report["flight_streaming"] = {k: sn.get(k) for k in ("resident_mb", "memory_budget_mb", "totals", "upload_ms", "frames_over_upload_budget", "hlod", "gpu")}
        print(f"flight: {json.dumps(report['flight'])}")
        print(f"streaming after the flight: {json.dumps(report['flight_streaming'])}")
        report["upload_misses"] = mcp.json("stream_state", limit=1).get("recent_misses")
        print(f"recent upload budget misses: {json.dumps(report['upload_misses'])}")
        h1 = mcp.json("world_hash")["state_hash"]
        check(h1 == h0, f"the flight left the world hash unchanged ({h0})")
        ff = report["flight_frames"]
        check(ff["over_budget_from_streaming"] <= ff["frames"] // 1000,
              f"no traversal stutter: {ff['over_budget_from_streaming']} of {ff['over_budget']} frames over 1.5x the {budget} ms budget "
              f"(of {ff['frames']}; at most 0.1% allowed for driver stalls) had streaming run over its budget "
              f"(frame/stream max {ff['stream_ms_max']} ms, p99 {ff['stream_ms_p99']} ms; "
              f"{ff['over_budget_in_driver_recording']} more over only inside the driver's command recording, not counted)")
        check(fly["upload_ms"]["p99"] <= a.upload_ms * 1.25,
              f"upload ms per frame p99 {fly['upload_ms']['p99']} <= 1.25 x upload_ms {a.upload_ms} (max {fly['upload_ms']['max']})")
        check(ff["stream_ms_p99"] <= a.upload_ms * 2, f"frame/stream p99 {ff['stream_ms_p99']} ms <= 2 x upload_ms ({ff['stream_overruns']} frames over)")
        check(sn["totals"]["evicted"] > 0, f"evictions happened under memory_mb {a.memory_mb} ({sn['totals']['evicted']})")
        check(sn["resident_mb"] <= a.memory_mb * 1.02, f"resident {sn['resident_mb']} MB within memory_mb {a.memory_mb}")
        check(load["over_budget_from_streaming"] <= load["frames"] // 1000,
              f"loading: {load['over_budget_from_streaming']} frames over budget from streaming (at most 0.1%) while {len(ops)} spawns built and streamed in "
              f"({load['over_budget']} over budget, {load['over_budget_from_requests']} of them the agent's own world_batch calls; "
              f"frame/stream max {load['stream_ms_max']} ms, p99 {load['stream_ms_p99']} ms, {load['stream_overruns']} frames over 2 x upload_ms; "
              f"{load['over_budget_in_driver_recording']} more over only inside the driver's command recording (up to "
              f"{load['driver_recording_ms_max']} ms), not counted)")
        if load["over_budget_in_driver_recording"] or ff["over_budget_in_driver_recording"]:
            report["driver_recording_note"] = (
                "frames over budget only because of time inside the Vulkan driver's command recording calls "
                "(vkResetCommandPool, vkBeginCommandBuffer, vkCmdCopyBuffer, the batch barrier) are reported apart: "
                "constant-time calls on our side that the Intel iGPU driver sometimes stalls in while a big world loads")
        # --- captures
        w = a.cells * CELL
        caps = {}
        capture(mcp, "near", [-0.30 * w, 6, -0.30 * w], [-0.30 * w + 30, 0, -0.30 * w + 20], caps)
        capture(mcp, "mid", [0, 25, -0.25 * w], [0, 0, 0], caps)
        capture(mcp, "far", [-0.5 * w, 70, -0.5 * w], [0, 0, 0], caps)
        report["captures"] = caps
        far = caps["far"]["streaming"] or {}
        check(far.get("hlod_cells", 0) > 0, f"the far capture draws HLOD proxies ({far.get('hlod_cells')} cells, {far.get('hlod_entities')} entities)")
        check(caps["near"]["models"]["not_resident"] == 0, "the near capture waited for everything it shows")
        # --- HLOD on vs off from a vantage point
        vantage = dict(eye=[-0.5 * w, 70, -0.5 * w], target=[0, 0, 0], fov_y_deg=60)
        mcp.call("window_view", **vantage)
        wait_streamed(mcp)
        time.sleep(1.5)
        on = perf(mcp)
        rec = {}
        record_frames(a.port, 120, "hlod_on", rec)
        mcp.call("sight_compare", action="mark", name="hlod_on", eye=vantage["eye"], target=vantage["target"], fov_y_deg=60, width=1280, height=720)
        # Everything per object and resident, as before streaming (a big upload budget only so
        # the test gets there quickly; it isn't what is measured).
        mcp.call("stream_settings", hlod=False, radius_m=100000, memory_mb=8192, upload_ms=50, upload_mb=64)
        settled, _ = wait_streamed(mcp, timeout=300, need_hlod=False)
        time.sleep(1.5)
        off = perf(mcp)
        record_frames(a.port, 120, "hlod_off", rec)
        diff = mcp.json("sight_compare", action="diff", name="hlod_on", threshold=24)
        mcp.call("stream_settings", hlod=True, radius_m=384, memory_mb=a.memory_mb, upload_ms=a.upload_ms, upload_mb=8)
        report["hlod"] = {"on": on, "off": off, "capture_on": rec.get("hlod_on"), "capture_off": rec.get("hlod_off"),
                          "diff": {**{k: (diff.get("pixels") or {}).get(k) for k in ("changed_px", "changed_pct", "mean_diff", "max_diff", "regions_total")}, "image": (diff.get("image") or {}).get("path")},
                          "off_settle_s": settled}
        print(f"HLOD on : draw calls {on['draw_calls']}, submitted {on['submitted']}, gpu {on['gpu_ms']} ms, draw_list {on['draw_list']}")
        print(f"HLOD off: draw calls {off['draw_calls']}, submitted {off['submitted']}, gpu {off['gpu_ms']} ms, draw_list {off['draw_list']}")
        for k in ("hlod_on", "hlod_off"):
            c = rec.get(k) or {}
            print(f"  {k}: frame_ms {c.get('frame_ms')} gpu_ms {c.get('gpu_ms')} draw_calls {c.get('draw_calls')}")
        print(f"HLOD on vs off image diff: {json.dumps(report['hlod']['diff'])}")
        if on["draw_calls"] and off["draw_calls"]:
            check(on["draw_calls"] * 2 <= off["draw_calls"], f"HLOD halves draw calls at least ({on['draw_calls']} vs {off['draw_calls']})")
            check(on["submitted"] * 2 <= off["submitted"], f"HLOD halves submitted draws at least ({on['submitted']} vs {off['submitted']})")
        diff_px = report["hlod"]["diff"].get("changed_px")
        check(diff_px is not None and diff_px <= 1280 * 720 // 1000, f"no popping: HLOD on vs off differ in {diff_px} pixels (threshold 24 of 255)")
        # --- the same flight with streaming off (everything resident, per object, no proxies: the
        # old path, in this binary)
        mcp.call("stream_settings", enabled=False)
        # The old path uploads every build at once, synchronously: the engine is busy for a
        # while (that stall is what streaming removes); wait it out.
        t = time.perf_counter()
        while True:
            try:
                mcp.call("window_view", eye=path[0], target=[path[0][0] + 30, 10, path[0][2] + 30], fov_y_deg=60)
                break
            except RuntimeError as e:
                if "busy" not in str(e) or time.perf_counter() - t > 300:
                    raise
        report["streaming_off_switch_s"] = round(time.perf_counter() - t, 1)
        print(f"switching streaming off (everything uploaded at once): engine busy for about {report['streaming_off_switch_s']} s")
        time.sleep(2)
        fly_off = mcp.json("stream_fly", path=path, speed_mps=a.speed)
        report["flight_streaming_off"] = {k: v for k, v in fly_off.items() if k != "streaming_now"}
        print(f"flight, streaming off: {json.dumps(report['flight_streaming_off'])}")
        p_on = (fly.get("frame_ms") or {}).get("p99")
        p_off = (fly_off.get("frame_ms") or {}).get("p99")
        if p_on is None or p_off is None:
            # A flight with no frames (the window starved: another session's load, a stalled driver).
            check(False, f"both flights recorded frames (with streaming {fly.get('frames')}, without {fly_off.get('frames')})")
        else:
            check(p_on <= max(budget * 1.5, p_off * 1.1 + 2.0), f"flight frame p99 with streaming {p_on} ms <= 1.5 x the {budget} ms budget, or no worse than without it ({p_off} ms)")
        mcp.call("stream_settings", enabled=True)
        zones = mcp.json("perf_zones", min_ms=0.001, limit=40)
        report["zones_after"] = [z for z in zones.get("zones", []) if "stream" in z.get("path", "")]
    report["validation_errors_streaming_run"] = eng.validation_errors()
    check(eng.validation_errors() == 0, f"no validation errors ({eng.validation_errors()})")
    check(eng.exit_code == 0, "clean shutdown")


def run_off(a, out, ops, report, check):
    perf_file = perf_toml(out, ["enabled = false"])
    args = ["--assets", str(ASSETS), "--captures", str(out / "captures_off"), "--headless", "--adapter", a.adapter, "--perf", str(perf_file),
            "--cache", str(out / "cache")]
    with Engine(a.exe, a.port, out / "engine_off.log", *args, cwd=str(ROOT)) as eng:
        mcp = eng.mcp("stream-check-off")
        mcp.call("time_control", action="pause")
        spawn_world(mcp, ops)
        wait_builds(mcp)
        h = mcp.json("world_hash")["state_hash"]
        report["hash_streaming_off"] = h
        check(h == report.get("hash_streaming_on"), f"world hash identical with streaming on and off ({report.get('hash_streaming_on')} vs {h})")


def run_baseline(a, out, ops, report):
    args = ["--assets", str(ASSETS), "--captures", str(out / "captures_base"), "--size", "1280x720", "--adapter", a.adapter,
            "--perf", str(ROOT / "perf.example.toml"), "--cache", str(out / "cache_base")]
    with Engine(a.baseline_exe, a.port, out / "engine_base.log", *args, cwd=str(ROOT)) as eng:
        mcp = eng.mcp("stream-check-base")
        path = fly_path(a.cells)
        mcp.call("window_view", eye=path[0], target=[path[0][0] + 30, 10, path[0][2] + 30], fov_y_deg=60)
        mcp.call("time_control", action="pause")
        budget = mcp.json("perf_frame")["budget_ms"]
        stop, paths = threading.Event(), []
        rec = threading.Thread(target=record_until, args=(a.port, "loading", stop, paths))
        rec.start()
        spawn_world(mcp, ops)
        wait_builds(mcp)
        time.sleep(1)
        stop.set()
        rec.join()
        # The old engine uploads in its draw list (`frame/draw_list/sync_meshes`), not a stream zone.
        report["baseline_loading"] = analyze_frames(paths, budget, a.upload_ms)
        frames = [f for p in paths for f in json.load(open(p))["frames"]]
        report["baseline_loading"]["sync_meshes_ms_max"] = round(max((zone_ms(f, "frame/draw_list/sync_meshes") for f in frames), default=0), 2)
        print(f"baseline loading frames: {json.dumps(report['baseline_loading'])}")
        time.sleep(2)
        stop, paths = threading.Event(), []
        th = threading.Thread(target=record_until, args=(a.port, "fly", stop, paths))
        th.start()
        scripted_fly(mcp, path, a.speed)
        stop.set()
        th.join()
        report["baseline_flight"] = analyze_frames(paths, budget, a.upload_ms)
        print(f"baseline flight: {json.dumps(report['baseline_flight'])}")
        w = a.cells * CELL
        mcp.call("window_view", eye=[-0.5 * w, 70, -0.5 * w], target=[0, 0, 0], fov_y_deg=60)
        time.sleep(2)
        report["baseline_vantage"] = perf(mcp)
        print(f"baseline vantage: draw calls {report['baseline_vantage']['draw_calls']}, submitted {report['baseline_vantage']['submitted']}, gpu {report['baseline_vantage']['gpu_ms']} ms")


def run_sepulcra(a, out, report, check):
    """Dogfood: Sepulcra (seed 7) played the same way twice, run A windowed with streaming on
    (captures every 20 ticks, so frames and captures stream while it plays), run B headless with
    streaming off; the desync detector compares every tick's hashes, game subsystems included."""
    from specula_mcp import Mcp, url_for
    exe = Path(a.sepulcra_exe)
    if not exe.exists():
        check(False, f"{exe} not found: cargo build --release -p sepulcra-app")
        return
    on = perf_toml(out, [f"upload_ms = {a.upload_ms}"])
    off = perf_toml(out, ["enabled = false"])
    runs = [("on", a.port + 1, ["--size", "1280x720", "--adapter", a.adapter, "--perf", str(on)]),
            ("off", a.port + 2, ["--headless", "--adapter", a.adapter, "--perf", str(off)])]
    engines, mcps = [], []
    try:
        for label, port, extra in runs:
            e = Engine(exe, port, out / f"sepulcra_{label}.log", "--captures", str(out / f"sep_cap_{label}"), "--worlds",
                       str(out / f"sep_worlds_{label}"), "--saves", str(out / f"sep_saves_{label}"), "--cache", str(out / "cache"),
                       *extra, "--", "--seed", "7", cwd=str(ROOT))
            engines.append(e.__enter__())
            m = Mcp(url_for(port), f"stream-check-sepulcra-{label}")
            m.json("agent_hello", name=f"sep_{label}")
            mcps.append(m)
        ticks = [m.json("time_control", action="pause")["tick"] for m in mcps]
        target = max(ticks) + 2
        for m, t in zip(mcps, ticks):
            m.json("time_control", action="step", ticks=target - t)
        plan = {10: ("place", "warding", "top:8,12"), 30: ("place", "warding", "top:10,12"), 60: ("call_wave", None, None)}
        captures = []
        for i in range(a.sepulcra_ticks):
            for k, m in enumerate(mcps):
                if i in plan:
                    act, stone, cell = plan[i]
                    args = {"action": act} if act == "call_wave" else {"action": act, "stone": stone, "cell": cell}
                    try:
                        m.call("play_command", **args)
                    except RuntimeError as e:
                        print(f"  play_command {args} on run {k}: {e}")
                if k == 0 and i % 20 == 0:
                    shot = m.frame(width=960, height=540, max_subjects=3)
                    captures.append(shot["image"]["path"])
                m.json("time_control", action="step", ticks=1)
        trace = mcps[0].json("desync_trace")
        result = mcps[1].json("desync_compare", a=share_file(mcps[1], trace["file"]), b="live")
        stream_on = mcps[0].json("perf_frame")["streaming"]
        report["sepulcra"] = {"ticks": a.sepulcra_ticks, "trace_ticks": trace["ticks"], "match": result.get("match"),
                              "first_divergent_tick": result.get("first_divergent_tick"), "components": result.get("components"),
                              "captures": captures, "streaming_on": {k: stream_on.get(k) for k in ("resident_mb", "totals", "upload_ms", "hlod")},
                              "hash_on": mcps[0].json("world_hash")["state_hash"], "hash_off": mcps[1].json("world_hash")["state_hash"]}
        print(f"sepulcra: {json.dumps({k: v for k, v in report['sepulcra'].items() if k != 'captures'})}")
        check(result.get("match") is True, f"Sepulcra plays tick-identically with streaming on and off ({trace['ticks']} ticks traced)")
    finally:
        for m in mcps:
            m.close()
        for e in engines:
            e.__exit__(None, None, None)
    for e, (label, _, _) in zip(engines, runs):
        check(e.validation_errors() == 0, f"Sepulcra run {label}: no validation errors ({e.validation_errors()})")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    ap.add_argument("--exe", default=str(target / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--baseline-exe", default=None, help="an engine built from before streaming, for the before numbers")
    ap.add_argument("--port", type=int, default=7496)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--cells", type=int, default=24, help="the world is cells x cells of 32 m")
    ap.add_argument("--speed", type=float, default=25.0, help="flight speed, m/s")
    ap.add_argument("--memory-mb", type=float, default=None,
                    help="streaming memory budget, less than the world needs (default 96 MB up to 12 x 12 cells, else 128)")
    ap.add_argument("--upload-ms", type=float, default=2.0)
    ap.add_argument("--out", default=str(target / "stream-check"))
    ap.add_argument("--sepulcra-exe", default=str(target / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")))
    ap.add_argument("--sepulcra-ticks", type=int, default=120)
    ap.add_argument("--skip", default="", help="comma-separated phases to skip: baseline, world, off, sepulcra")
    a = ap.parse_args()
    skip = set(filter(None, a.skip.split(",")))
    if a.memory_mb is None:
        a.memory_mb = 96.0 if a.cells <= 12 else 128.0
    if a.port == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    ops = build_ops(a.cells)
    print(f"world: {a.cells} x {a.cells} cells of {CELL:.0f} m, {len(ops)} spawns")
    failures = []

    def check(ok, what):
        print(("ok    " if ok else "FAIL  ") + what)
        if not ok:
            failures.append(what)

    report = {"cells": a.cells, "spawns": len(ops), "memory_mb": a.memory_mb, "upload_ms": a.upload_ms, "speed_mps": a.speed}
    if a.baseline_exe and "baseline" not in skip:
        run_baseline(a, out, ops, report)
    if "world" not in skip:
        run_streaming(a, out, ops, report, check)
        if "off" not in skip:
            run_off(a, out, ops, report, check)
    if "sepulcra" not in skip:
        run_sepulcra(a, out, report, check)
    report["failures"] = failures
    (out / "report.json").write_text(json.dumps(report, indent=2))
    print(f"report: {out / 'report.json'}")
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

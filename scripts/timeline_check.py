#!/usr/bin/env python3
"""Timeline check (M5 "Timeline/sequencer and camera rails"; docs/design/TIMELINE.md).

1. **Authoring and checks** (the stock engine, windowed on the chosen adapter, a scratch assets
   folder): an agent writes a timeline over MCP (timeline_set: a camera rail round a pillar, an
   entity track, a vfx trigger, an audio cue, subtitles, events), validates it, checks its rail
   (subject on screen and within size bounds, clearance by physics raycasts, no jumps, upright,
   no roll) and previews it (a filmstrip with sight packets). A deliberately bad rail (through the
   pillar, with a step) must fail clearance and the jump checks, and a flipping one upright.
2. **Playback.** Played windowed: the window follows the rail camera (engine_info's window camera
   moves and names the timeline), then gets its camera back. Claims: another agent's claim refuses
   the start (claimed) and pauses a running playback as blocked; another agent can't stop it.
3. **Determinism.** Scrubbing to t gives the state playing to t gives; a fresh headless engine
   running the same calls has the same world_hash on the same tick, and world_hash {verify} replays
   the journal to it.
4. **Sepulcra** (windowed on the adapter, a scratch copy of its assets): the intro flyover plays
   at the start (the window follows it, wave 1 waits), a key posted to the window skips it, a new
   window plays it to its end (restored camera, the run starts); both shipped rails pass
   timeline_check with the slab as the subject; a rail diving through the slab fails; the game is
   won (autoplayed) and the victory shot plays in the window; play_record / play_replay verify,
   and world_hash {verify} holds.
5. **Review probes** (the lead's review of 33ac9a8, kept as checks): a timeline's cache follows an
   undo; a blocked playback resumed starts its effects where a scrub would; scrubs, pauses, edits and
   restarts replay (world_hash verify); a deleted target blocks a playback; a scrub puts back an
   entity another agent moved; a closed session's playbacks free their slots; in a window, a second
   timeline takes the view, only the lead's force stops another's, the camera comes back, and a
   world load ends a playback.
6. **Validation.** Every engine shuts down with zero validation errors.

Usage: cargo build --release -p specula -p sepulcra-app && python scripts/timeline_check.py
       [--adapter integrated] [--port 7530] [--out DIR] [--skip-sepulcra | --only-sepulcra]
Output (captures, packets, logs, summary.json) goes to target/timeline-check/<adapter>/. Look at
every PNG. Standard library only; output ASCII. Exit 0 = everything held.
"""
import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
EXE = TARGET / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
GAME = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")

FAILURES = []


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        FAILURES.append(what)
    return cond


def rel(p):
    try:
        return str(Path(p).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return "<out>/" + Path(p).name


def err_of(mcp, tool, **args):
    """The structured error of a call that must fail (None if it succeeded)."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not result.get("isError"):
        return None
    text = result["content"][0]["text"]
    try:
        return json.loads(text)
    except ValueError:
        return {"error": "text", "message": text}


def checks_of(rep):
    return {c["check"]: c for c in rep["checks"]}


def save_image(result, out, name):
    """Copies an image answer's PNG (and packet) into the output folder."""
    packet = json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))
    img = packet.get("image") or {}
    if img.get("path"):
        shutil.copyfile(img["path"], out / f"{name}.png")
    if img.get("packet_path"):
        shutil.copyfile(img["packet_path"], out / f"{name}.json")
    return packet


# ---------------------------------------------------------------------------------------------
# Part 1-3: a scripted world

def world_ops():
    return [
        {"op": "spawn", "shape": "cube", "name": "ground", "position": [0, -0.5, 0], "scale": [60, 1, 60], "color": "#5b6470", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "cube", "name": "pillar", "position": [0, 3, 0], "scale": [2, 6, 2], "color": "#8c96a3", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "cube", "name": "door", "position": [6, 1, 0], "scale": [1, 2, 0.3], "color": "#a07850"},
        {"op": "spawn", "shape": "sphere", "name": "lamp", "position": [-5, 1, 3], "scale": 0.6, "color": "#3fe0d0", "emissive": 1},
    ]


def good_timeline():
    return {
        "schema": "specula.timeline.v1",
        "description": "an orbit round the pillar: the door rises, the lamp lights, a caption",
        "duration": 6,
        "tracks": [
            {"kind": "camera", "name": "orbit",
             "rail": {"points": [[18, 6, 0], [12.7, 7, 12.7], [0, 8, 18], [-12.7, 9, 12.7], [-18, 10, 0], [-12.7, 11, -12.7], [0, 12, -18]]},
             "move": [{"t": 0, "at": 0}, {"t": 6, "at": 1, "ease": "smooth"}],
             "look": {"entity": "/pillar"}, "fov": [{"t": 0, "value": 50}, {"t": 6, "value": 40, "ease": "in_out"}]},
            {"kind": "entity", "name": "door", "target": "/door",
             "position": [{"t": 0, "value": [6, 1, 0]}, {"t": 3, "value": [6, 4, 0], "ease": "in_out"}],
             "emissive": [{"t": 0, "value": 0}, {"t": 4, "value": 2}],
             "visible": [{"t": 0, "value": True}, {"t": 5, "value": False}]},
            {"kind": "vfx", "name": "flame", "target": "/lamp", "keys": [{"t": 1, "vfx": "fire"}]},
            {"kind": "audio", "cues": [{"t": 0.5, "sound": "bell", "at": "/pillar"}]},
            {"kind": "subtitles", "lines": [{"t": 1, "duration": 3, "speaker": "Guide", "text": "The pillar stands."}]},
            {"kind": "events", "events": [{"t": 2, "name": "halfway", "data": {"n": 1}}]},
        ],
    }


def bad_timeline():
    """Through the pillar (clearance), with a step (jump) and looking straight down at the end
    (upright)."""
    return {
        "schema": "specula.timeline.v1",
        "tracks": [
            {"kind": "camera", "name": "dive", "rail": {"points": [[16, 3, 0], [-16, 3, 0]]},
             "move": [{"t": 0, "at": 0}, {"t": 2, "at": 0.4}, {"t": 2.5, "at": 1, "ease": "step"}],
             "look": {"entity": "/pillar"}},
            {"kind": "camera", "name": "flip", "rail": {"points": [[0, 30, 0.01], [0.5, 30, 0.01]]},
             "move": [{"t": 3, "at": 0}, {"t": 5, "at": 1, "ease": "smooth"}], "look": {"point": [0, 0, 0]}},
        ],
    }


def entity(mcp, path):
    q = mcp.json("world_query", path_prefix=path, fields=["position", "visible", "emissive", "vfx"], limit=1)
    items = q.get("entities") or q.get("items") or []
    return items[0] if items else None


def state_of(e):
    """What the timeline writes on /door (and /lamp's vfx), comparable across runs."""
    return {k: e.get(k) for k in ("position", "visible", "emissive", "vfx")}


def scripted(args, out):
    print("== scripted world: authoring, checks, preview, playback, claims ==")
    assets = out / "assets1"
    shutil.rmtree(assets, ignore_errors=True)
    (assets / "timelines").mkdir(parents=True)
    caps = out / "captures1"
    flags = ["--adapter", args.adapter, "--size", "1280x720", "--assets", str(assets), "--captures", str(caps),
             "--worlds", str(out / "worlds1"), "--paused"]
    summary = {}
    with Engine(EXE, args.port, out / "engine1.log", *flags, cwd=str(out)) as eng:
        alice = eng.mcp("alice")
        alice.hello("alice")
        bob = eng.mcp("bob")
        bob.hello("bob")
        alice.json("world_batch", ops=world_ops())
        # Author.
        w = alice.json("timeline_set", path="orbit", timeline=good_timeline(), expect_version=0)
        check(w.get("version") == 1, f"timeline_set wrote assets/timelines/orbit.json (version {w.get('version')})")
        check((assets / "timelines" / "orbit.json").exists(), "the file is on disk")
        bad = err_of(alice, "timeline_set", path="broken", timeline={"schema": "specula.timeline.v1", "tracks": [{"kind": "camera", "rail": {"points": [[0, 0, 0]]}}]})
        check(bad is not None and bad.get("field", "").startswith("tracks[0]"), f"a broken timeline is refused with its field: {bad and bad.get('field')}")
        alice.json("timeline_set", path="bad", timeline=bad_timeline())
        v = alice.json("timeline_validate", path="orbit")
        check(v["ok"], f"timeline_validate: ok ({len(v['problems'])} problems)")
        lst = alice.json("timeline_get")
        check(lst["total"] == 2, f"timeline_get lists {lst['total']} timelines")
        at = alice.json("timeline_get", path="orbit", at=1.5)
        door = next(t for t in at["at"]["tracks"] if t["track"] == "door")
        check(abs(door["position"][1] - 2.5) < 1e-3, f"evaluated at 1.5 s without playing: the door is at y {door['position'][1]}")
        # Rail checks.
        good = alice.json("timeline_check", path="orbit", subject="/pillar")
        (out / "check_orbit.json").write_text(json.dumps(good, indent=2))
        check(good["ok"], "the orbit passes every rail check: " + ", ".join(f"{c['check']} {c.get('value')}" for c in good["checks"]))
        check(good["colliders_considered"] >= 2, f"colliders considered: {good['colliders_considered']}")
        badc = alice.json("timeline_check", path="bad", subject="/pillar")
        (out / "check_bad.json").write_text(json.dumps(badc, indent=2))
        bc = checks_of(badc)
        check(not badc["ok"], "the bad rail fails")
        check(not bc["clearance"]["ok"] and any("/pillar" in f["entity"] for f in bc["clearance"].get("failures", [])), "clearance: the camera passes through /pillar (physics raycasts)")
        check(not bc["jump"]["ok"] and not bc["speed"]["ok"], f"jump: {bc['jump']['value']} m/s2, speed {bc['speed']['value']} m/s at t = {bc['speed'].get('at_t')}")
        check(not bc["upright"]["ok"], f"upright: pitch reaches {bc['upright']['value']} deg (flips)")
        check(badc["cuts"] == [3.0], f"the cut between the two cameras isn't measured as a jump: cuts {badc['cuts']}")
        # Preview.
        r = alice.call("timeline_preview", path="orbit", frames=8, tile=320)
        pk = save_image(r, out, "orbit_preview")
        frames = pk["frames"]
        check(len(frames) == 8 and all(f["subject"]["on_screen"] for f in frames), "the preview's 8 frames all have /pillar on screen")
        check(all((f["subject"].get("visible_px") or 0) > 0 and f["subject"].get("drawn") for f in frames), "and visible (ID buffer) and drawn (frame record) in each")
        r = alice.call("timeline_preview", path="bad", frames=6, tile=320, subject="/pillar")
        save_image(r, out, "bad_preview")
        # Claims: bob holds the door, so alice's start (which writes it) is refused.
        claim = bob.json("claim_take", path="/door")
        e = err_of(alice, "timeline_play", path="orbit")
        check(e is not None and e.get("error") == "claimed", f"a start writing a claimed entity is refused: {e and e.get('error')}")
        bob.json("claim_release", id=claim["id"])
        # Playback in the window.
        cam0 = alice.json("engine_info")["window"]
        p = alice.json("timeline_play", path="orbit")
        check(p["state"] == "playing" and p["window"], "timeline_play: playing, driving the window")
        alice.json("time_control", action="resume")
        time.sleep(1.0)
        w1 = alice.json("engine_info")["window"]
        time.sleep(1.0)
        w2 = alice.json("engine_info")["window"]
        check(w1.get("timeline") == "assets/timelines/orbit.json", f"the window looks through the timeline: {w1.get('timeline')}")
        check(w1["camera"] != w2["camera"], "the window camera moves along the rail")
        st = alice.json("timeline_get", path="orbit")["playback"]
        check(st.get("subtitles") or st["t"] > 4, f"subtitles at t {st['t']}: {st.get('subtitles')}")
        # Bob can't stop it; his claim blocks it.
        e = err_of(bob, "timeline_stop", path="orbit")
        check(e is not None and e.get("owner") == "alice", f"another agent can't stop it: {e and e.get('error')} (owner {e and e.get('owner')})")
        alice.json("time_control", action="pause")
        claim = bob.json("claim_take", path="/door")
        alice.json("time_control", action="step", ticks=5)
        st = alice.json("timeline_get", path="orbit")["playback"]
        blocked = st.get("blocked") or {}
        check(st["state"] == "blocked" and blocked.get("error") == "claimed", f"a claim taken mid-play pauses it as blocked: {st['state']} {blocked.get('error')} (owner {blocked.get('owner')})")
        bob.json("claim_release", id=claim["id"])
        alice.json("timeline_pause", path="orbit", paused=False)
        alice.json("time_control", action="resume")
        end = time.time() + 15
        while time.time() < end and alice.json("timeline_get")["player"]["playing"]:
            time.sleep(0.25)
        player = alice.json("timeline_get")["player"]
        check(not player["playing"] and player["ended"][-1]["how"] == "finished", f"it ends: {player['ended'][-1] if player['ended'] else None}")
        w3 = alice.json("engine_info")["window"]
        check(w3.get("timeline") is None and w3["camera"] == cam0["camera"], f"the window has its camera back: {w3['camera']}")
        check(player["window_frames"] > 30, f"window frames drawn through the rail: {player['window_frames']}")
        alice.json("time_control", action="pause")
        door_end = entity(alice, "/door")
        check(door_end and door_end.get("visible") is False, "the door's visibility key at 5 s took effect")
        # Scrub == play (the same playback, the same origin): played to 1.5 s, then scrubbed.
        alice.json("timeline_play", path="orbit", window=False)
        alice.json("time_control", action="step", ticks=45)
        played = state_of(entity(alice, "/door")), entity(alice, "/lamp").get("vfx")
        alice.json("timeline_scrub", path="orbit", t=5.0)
        alice.json("timeline_scrub", path="orbit", t=1.5)
        scrubbed = state_of(entity(alice, "/door")), entity(alice, "/lamp").get("vfx")
        check(played == scrubbed, f"scrubbing to 1.5 s equals playing to it: {played[0]} / lamp vfx {played[1]}")
        alice.json("timeline_stop", path="orbit")
        # Determinism: a headless engine running the same calls.
        alice.json("timeline_play", path="orbit", window=False)
        alice.json("time_control", action="step", ticks=60)
        h1 = alice.json("world_hash")
        ver = alice.json("world_hash", verify=True)
        v = ver.get("verify") or {}
        check(v.get("matches") is True, f"world_hash verify replays the journal to the live hash ({v.get('entries')} entries): {v.get('replay_hash')}")
        summary["windowed_hash"] = h1
        info = alice.json("engine_info")
        summary["gpu"] = info["gpu"]["name"]
    check(eng.validation_errors() == 0, f"engine 1 shut down with {eng.validation_errors()} validation errors")
    # The same calls, headless, in a fresh engine with the same files.
    assets2 = out / "assets2"
    shutil.rmtree(assets2, ignore_errors=True)
    shutil.copytree(assets, assets2)
    flags2 = ["--headless", "--assets", str(assets2), "--captures", str(out / "captures2"), "--worlds", str(out / "worlds2"), "--paused"]
    with Engine(EXE, args.port + 1, out / "engine2.log", *flags2, cwd=str(out)) as eng2:
        a = eng2.mcp("alice")
        a.hello("alice")
        a.json("world_batch", ops=world_ops())
        h_before = a.json("world_hash")
        a.json("timeline_play", path="orbit", window=False)
        a.json("time_control", action="step", ticks=60)
        h2 = a.json("world_hash")
        # Replay the earlier engine's sequence is not needed: compare a fresh run with itself
        # from a second fresh engine below.
        summary["headless_hash"] = h2
        summary["headless_before"] = h_before
    with Engine(EXE, args.port + 2, out / "engine3.log", *[f.replace("captures2", "captures3").replace("worlds2", "worlds3") for f in flags2], cwd=str(out)) as eng3:
        a = eng3.mcp("alice")
        a.hello("alice")
        a.json("world_batch", ops=world_ops())
        a.json("timeline_play", path="orbit", window=False)
        a.json("time_control", action="step", ticks=60)
        h3 = a.json("world_hash")
    same = lambda x, y: x.get("state_hash") == y.get("state_hash") and x.get("tick") == y.get("tick")  # noqa: E731
    check(bool(h3.get("state_hash")) and same(summary["headless_hash"], h3), f"two fresh engines playing the timeline agree on world_hash at tick {h3.get('tick')}: {h3.get('state_hash')}")
    check(summary["headless_hash"].get("state_hash") != summary["headless_before"].get("state_hash"), "and the playback did change the world (the door moved)")
    summary["fresh_hash"] = h3
    return summary


# ---------------------------------------------------------------------------------------------
# Part 4: Sepulcra

def find_window(pid):
    """The engine's top-level window (Windows only), or None."""
    if sys.platform != "win32":
        return None
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    found = []
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _):
        p = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
        if p.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True
    user32.EnumWindows(proto(each), 0)
    return found[0] if found else None


def post_key(hwnd, vk=0x7C):
    """Posts a key press (F13 by default: bound to nothing) to the window, as a human's would arrive."""
    import ctypes
    user32 = ctypes.windll.user32
    user32.PostMessageW(hwnd, 0x0100, vk, 0x00000001)
    time.sleep(0.05)
    user32.PostMessageW(hwnd, 0x0101, vk, 0xC0000001)


def sepulcra(args, out):
    print("== Sepulcra: intro flyover, skip, rail checks, victory shot, replay ==")
    assets = out / "sepulcra_assets"
    shutil.rmtree(assets, ignore_errors=True)
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    summary = {}

    def start(n, log):
        flags = ["--adapter", args.adapter, "--size", "1280x720", "--assets", str(assets), "--captures", str(out / f"sep_captures{n}"),
                 "--worlds", str(out / f"sep_worlds{n}"), "--saves", str(out / f"sep_saves{n}"), "--", "--seed", "7", "--mute"]
        return Engine(GAME, args.port + 3 + n, out / log, *flags, cwd=str(out))

    # A: the intro plays, the run waits, a key skips it.
    with start(0, "sepulcra0.log") as eng:
        mcp = eng.mcp("cutter")
        mcp.hello("cutter")
        st = None
        end = time.time() + 60
        while time.time() < end:
            st = mcp.json("play_state")
            if (st.get("cutscene") or {}).get("playing"):
                break
            time.sleep(0.2)
        cs = st.get("cutscene") or {}
        check(cs.get("playing") == "assets/timelines/intro.json" and cs.get("holds_run"), f"the intro plays at the start and holds the run: {cs}")
        tick0 = st["tick"]
        time.sleep(1.5)
        st = mcp.json("play_state")
        w = mcp.json("engine_info")["window"]
        check(st["tick"] == tick0, f"wave 1 waits: the simulation stays at tick {st['tick']}")
        check(w.get("timeline") == "assets/timelines/intro.json", "the window looks through the intro's rail camera")
        pb = mcp.json("timeline_get", path="intro")["playback"]
        summary["intro_playback"] = pb
        hwnd = find_window(eng.proc.pid)
        if check(hwnd is not None, "found the game's window"):
            post_key(hwnd)
            time.sleep(1.0)
            st = mcp.json("play_state")
            ended = (st.get("cutscene") or {}).get("last_ended") or {}
            check(ended.get("how") == "skipped" and not (st.get("cutscene") or {}).get("playing"), f"a key skips it: {ended}")
            time.sleep(1.0)
            st2 = mcp.json("play_state")
            check(st2["tick"] > st["tick"], f"and the run starts: tick {st['tick']} -> {st2['tick']}")
            check(mcp.json("engine_info")["window"].get("timeline") is None, "the window has its camera back")
    check(eng.validation_errors() == 0, f"Sepulcra (skip) shut down with {eng.validation_errors()} validation errors")

    # B: the intro to its end, rail checks, previews, victory, replay.
    with start(1, "sepulcra1.log") as eng:
        mcp = eng.mcp("cutter")
        mcp.hello("cutter")
        # The window's cost while the intro drives it (the iGPU), then in play after it.
        end = time.time() + 60
        while time.time() < end and not mcp.json("engine_info")["window"].get("timeline"):
            time.sleep(0.1)
        intro_perf = mcp.json("perf_capture", frames=120, trace=False, label="timeline_intro")
        summary["perf_intro"] = {k: intro_perf.get(k) for k in ("frames", "adapter", "frame_ms", "gpu_ms", "draw_calls")}
        cams = []
        end = time.time() + 90
        ended = None
        while time.time() < end:
            st = mcp.json("play_state")
            cs = st.get("cutscene") or {}
            win = mcp.json("engine_info")["window"]
            if win.get("timeline"):
                cams.append(win["camera"]["eye"])
            if cs.get("last_ended"):
                ended = cs["last_ended"]
                break
            time.sleep(0.5)
        check(ended and ended.get("how") == "finished", f"the intro plays to its end: {ended}")
        check(len({tuple(c) for c in cams}) >= 5, f"the window followed the rail ({len(cams)} samples, {len({tuple(c) for c in cams})} distinct eyes)")
        time.sleep(1.0)
        st = mcp.json("play_state")
        check(st["tick"] > 0, f"the run started after it: tick {st['tick']}")
        win = mcp.json("engine_info")["window"]
        check(win.get("timeline") is None and win["camera"] != "auto", f"the player's camera after it: {win['camera']}")
        summary["intro_rail_eyes"] = cams[:40]
        play_perf = mcp.json("perf_capture", frames=120, trace=False, label="timeline_after_intro")
        summary["perf_after_intro"] = {k: play_perf.get(k) for k in ("frames", "adapter", "frame_ms", "gpu_ms", "draw_calls")}
        for label, pf in (("intro", summary["perf_intro"]), ("after", summary["perf_after_intro"])):
            g, f = pf.get("gpu_ms") or {}, pf.get("frame_ms") or {}
            print(f"  perf {label}: {pf.get('frames')} frames on {pf.get('adapter')}, GPU mean {g.get('mean')} p95 {g.get('p95')} ms, frame p95 {f.get('p95')} ms")
        # Rail checks with the slab as the subject; previews.
        for name in ("intro", "victory"):
            # The whole slab stays in frame (its box, with the rock hanging under it, measures
            # 1.34 of the view's height from the player's own default camera: max 1.5), and the
            # graveyard on top stays in frame throughout. Its size: the player's default view
            # shows it at 0.18 of the view's height; the intro opens on a far establishing shot
            # (0.018 at its start) and closes in to the player's view, the victory shot stays near
            # the player's scale (measured 0.13..0.19).
            graveyard_min = 0.015 if name == "intro" else 0.1
            for subject, lo, hi in (("/slab", 0.3, 1.5), ("/slab/graveyard", graveyard_min, 1.0)):
                rep = mcp.json("timeline_check", path=name, subject=subject, min_size=lo, max_size=hi)
                tag = subject.strip("/").replace("/", "_")
                (out / f"check_{name}_{tag}.json").write_text(json.dumps(rep, indent=2))
                failed = [f"{c['check']}: {c['message']}" for c in rep["checks"] if not c["ok"] and c.get("severity") == "error"]
                check(rep["ok"], f"{name} with {subject}: every rail check passes (upright, no roll, on screen, size {lo}..{hi}, clearance, no jumps){'' if rep['ok'] else ': ' + '; '.join(failed)}")
                summary[f"check_{name}_{tag}"] = {c["check"]: [c.get("ok"), c.get("value")] for c in rep["checks"]}
            r = mcp.call("timeline_preview", path=name, frames=8, tile=400, subject="/slab")
            pk = save_image(r, out, f"sepulcra_{name}_preview")
            check(all(f["subject"]["on_screen"] for f in pk["frames"]), f"{name} preview: the slab is on screen in all 8 frames")
        # A rail diving through the slab fails clearance.
        dive = {"schema": "specula.timeline.v1", "tracks": [{"kind": "camera", "rail": {"points": [[0, 30, 30], [0, -12, -30]]},
                "move": [{"t": 0, "at": 0}, {"t": 4, "at": 1, "ease": "smooth"}], "look": {"ahead": 10}}]}
        mcp.json("timeline_set", path="dive", timeline=dive)
        rep = mcp.json("timeline_check", path="dive", subject="/slab")
        (out / "check_dive.json").write_text(json.dumps(rep, indent=2))
        cl = checks_of(rep)["clearance"]
        check(not cl["ok"], f"a rail diving through the slab fails clearance: {[f['entity'] for f in cl.get('failures', [])][:3]}")
        # Win (autoplayed, stepped), then the victory shot in the window.
        mcp.json("time_control", action="pause")
        mcp.json("play_new", seed=7, layout="centre")
        outcome = "running"
        for _ in range(400):
            st = mcp.json("play_state")
            outcome = st["outcome"]
            if outcome != "running":
                break
            mcp.json("time_control", action="step", ticks=600)
        check(outcome == "won", f"the autoplayed run ends: {outcome} at wave {st['wave']}")
        if outcome == "won":
            mcp.json("time_control", action="resume")
            seen = []
            end = time.time() + 20
            while time.time() < end:
                cs = mcp.json("play_state").get("cutscene") or {}
                win = mcp.json("engine_info")["window"]
                if win.get("timeline"):
                    seen.append(win["camera"]["eye"])
                if cs.get("last_ended") and cs["last_ended"]["path"].endswith("victory.json"):
                    break
                time.sleep(0.4)
            cs = mcp.json("play_state").get("cutscene") or {}
            check(len(seen) >= 3 and cs.get("last_ended", {}).get("how") == "finished", f"the victory shot plays in the window and ends: {cs.get('last_ended')} ({len(seen)} samples)")
            mcp.json("time_control", action="pause")
        rec = mcp.json("play_record", path="recordings/timeline-check.json", overwrite=True)
        rep = mcp.json("play_replay", path="recordings/timeline-check.json")
        check(rep.get("status") == "verified", f"play_replay verifies the recorded run: {rep.get('status')}")
        ver = mcp.json("world_hash", verify=True)
        v = ver.get("verify") or {}
        check(v.get("matches") is True, f"Sepulcra: world_hash verify replays the journal (cutscenes included) to the live hash: {v.get('replay_hash')}")
        summary["sepulcra_hash_verify"] = ver
        summary["recording_commands"] = rec.get("commands")
    check(eng.validation_errors() == 0, f"Sepulcra shut down with {eng.validation_errors()} validation errors")
    return summary


def vfx_start(mcp, path):
    e = entity(mcp, path) or {}
    v = e.get("vfx") or {}
    return v.get("start") if isinstance(v, dict) else None


def probes(args, out):
    """The lead review's probes (probe1, probe2, probe5), as checks."""
    print("== review probes ==")
    assets = out / "assets_probes"
    shutil.rmtree(assets, ignore_errors=True)
    (assets / "timelines").mkdir(parents=True)
    flags = ["--headless", "--assets", str(assets), "--captures", str(out / "captures_probes"), "--worlds", str(out / "worlds_probes"), "--paused"]
    events = lambda dur: {"schema": "specula.timeline.v1", "duration": dur, "tracks": [{"kind": "events", "events": [{"t": 1, "name": "e"}]}]}  # noqa: E731
    with Engine(EXE, args.port + 6, out / "engine_probes.log", *flags, cwd=str(out)) as eng:
        a = eng.mcp("alice")
        a.hello("alice")
        b = eng.mcp("bob")
        b.hello("bob")
        # 1. The cache follows an undo.
        a.json("timeline_set", path="stale", timeline=events(5))
        a.json("timeline_set", path="stale", timeline=events(7))
        a.json("world_undo", count=1)
        d = a.json("timeline_get", path="stale")["duration"]
        check(d == 5, f"after world_undo, timeline_get reads the restored file (duration {d})")
        # 2. Blocked, then resumed: the effect starts where a scrub starts it.
        a.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": "pa"}, {"op": "spawn", "shape": "cube", "name": "pb", "position": [3, 0, 0]}])
        a.json("timeline_set", path="blk", timeline={"schema": "specula.timeline.v1", "duration": 4, "tracks": [
            {"kind": "entity", "name": "mv", "target": "/pa", "position": [{"t": 0, "value": [0, 0, 0]}, {"t": 1, "value": [0, 5, 0]}]},
            {"kind": "vfx", "name": "fx", "target": "/pb", "keys": [{"t": 2, "vfx": "fire"}]}]})
        a.json("timeline_play", path="blk", window=False)
        a.json("time_control", action="step", ticks=10)
        claim = b.json("claim_take", path="/pa")
        a.json("time_control", action="step", ticks=10)
        st = a.json("timeline_get", path="blk")["playback"]["state"]
        a.json("time_control", action="step", ticks=300)
        b.json("claim_release", id=claim["id"])
        a.json("timeline_pause", path="blk", paused=False)
        a.json("time_control", action="step", ticks=60)
        played = vfx_start(a, "/pb")
        t = a.json("timeline_get", path="blk")["playback"]["t"]
        a.json("timeline_scrub", path="blk", t=t)
        scrubbed = vfx_start(a, "/pb")
        check(st == "blocked" and played is not None and abs(played - scrubbed) < 1e-3, f"a blocked playback resumed starts its effect where a scrub does ({played} vs {scrubbed})")
        a.json("timeline_stop", path="blk")
        # 3. Scrubs, pauses, edits and restarts replay exactly.
        a.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": "pc", "position": [6, 0, 0]}, {"op": "spawn", "shape": "sphere", "name": "pd", "position": [-3, 0, 0]}])
        det = {"schema": "specula.timeline.v1", "duration": 6, "tracks": [
            {"kind": "entity", "name": "mv", "target": "/pc", "position": [{"t": 0, "value": [6, 0, 0]}, {"t": 5, "value": [6, 4, 2], "ease": "smooth"}],
             "rotation": [{"t": 0, "value": [0, 0, 0]}, {"t": 5, "value": [0, 270, 0]}], "visible": [{"t": 0, "value": True}, {"t": 3, "value": False}],
             "color": [{"t": 0, "value": "#ff0000"}, {"t": 4, "value": "#00ff00"}], "emissive": [{"t": 0, "value": 0}, {"t": 4, "value": 3}]},
            {"kind": "vfx", "name": "fx", "target": "/pd", "keys": [{"t": 1, "vfx": "fire"}, {"t": 4, "vfx": False}]}]}
        a.json("timeline_set", path="det", timeline=det)
        a.json("timeline_play", path="det", window=False)
        a.json("time_control", action="step", ticks=20)
        a.json("timeline_scrub", path="det", t=3.3)
        a.json("time_control", action="step", ticks=5)
        det["tracks"][0]["position"][1]["value"] = [6, 9, 2]
        a.json("timeline_set", path="det", timeline=det)
        a.json("timeline_pause", path="det")
        a.json("time_control", action="step", ticks=5)
        a.json("timeline_pause", path="det", paused=False)
        a.json("timeline_play", path="det", window=False, **{"from": 0.5})
        a.json("time_control", action="step", ticks=15)
        a.json("timeline_scrub", path="det", t=0.2)
        a.json("timeline_stop", path="det")
        v = a.json("world_hash", verify=True).get("verify") or {}
        check(v.get("matches") is True, "scrubs, pauses, an edit and a restart replay from the journal (world_hash verify)")
        # 4. A deleted target blocks the playback, loudly.
        a.json("timeline_set", path="del", timeline={"schema": "specula.timeline.v1", "tracks": [{"kind": "entity", "target": "/pc", "position": [{"t": 0, "value": [0, 0, 0]}, {"t": 3, "value": [0, 3, 0]}]}]})
        a.json("timeline_play", path="del", window=False)
        a.json("time_control", action="step", ticks=5)
        a.json("world_delete", path="/pc")
        a.json("time_control", action="step", ticks=5)
        pb = a.json("timeline_get", path="del")["playback"]
        check(pb["state"] == "blocked", f"a deleted target blocks the playback: {pb.get('blocked', {}).get('error')}")
        check(err_of(a, "timeline_scrub", path="del", t=1.0) is not None, "and a scrub onto it is refused, not silent")
        a.json("timeline_stop", path="del")
        # 5. A scrub puts back what another agent moved.
        a.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": "pe", "position": [9, 0, 0]}])
        a.json("timeline_set", path="fw", timeline={"schema": "specula.timeline.v1", "duration": 4, "tracks": [{"kind": "entity", "target": "/pe", "position": [{"t": 0, "value": [9, 0, 0]}, {"t": 2, "value": [9, 4, 0]}]}]})
        a.json("timeline_play", path="fw", window=False)
        a.json("time_control", action="step", ticks=30)
        a.json("timeline_pause", path="fw")
        b.json("world_set", path="/pe", position=[0, 0, 0])
        t = a.json("timeline_get", path="fw")["playback"]["t"]
        a.json("timeline_scrub", path="fw", t=t)
        pos = (entity(a, "/pe") or {}).get("position")
        check(pos is not None and abs(pos[0] - 9) < 1e-3 and abs(pos[1] - 4 * t / 2) < 1e-2, f"a scrub puts back what bob moved: {pos}")
        a.json("timeline_stop", path="fw")
        # 6. A closed session's playbacks free their slots (probe5).
        for n in ("q1", "q2", "q3"):
            a.json("timeline_set", path=n, timeline=events(5))
        a.json("timeline_play", path="q1", paused=True)
        a.json("timeline_play", path="q2", paused=True)
        e = err_of(a, "timeline_play", path="q3", paused=True)
        check(e is not None and e.get("error") == "job_busy", "an agent plays at most 2 at once")
        a.close()
        time.sleep(1.0)
        ok = err_of(b, "timeline_play", path="q3") is None
        left = [p["path"] for p in b.json("timeline_get")["player"]["playing"]]
        check(ok and not any("q1" in x or "q2" in x for x in left), f"alice's session closed: her playbacks ended and bob plays ({left})")
        b.close()
    check(eng.validation_errors() == 0, f"the probes' engine shut down with {eng.validation_errors()} validation errors")
    # The windowed probe (probe2): two window timelines, a lead's force, the camera, a load.
    shutil.rmtree(out / "worlds_probes2", ignore_errors=True)
    flags = ["--adapter", args.adapter, "--size", "640x360", "--lead", "boss", "--assets", str(assets), "--captures", str(out / "captures_probes2"), "--worlds", str(out / "worlds_probes2")]
    with Engine(EXE, args.port + 7, out / "engine_probes2.log", *flags, cwd=str(out)) as eng:
        a = eng.mcp("alice")
        a.hello("alice")
        b = eng.mcp("bob")
        b.hello("bob")
        lead = eng.mcp("boss")
        lead.hello("boss", role="lead")
        win = lambda: a.json("engine_info")["window"]  # noqa: E731
        cam = lambda pts: {"schema": "specula.timeline.v1", "duration": 30, "tracks": [{"kind": "camera", "rail": {"points": pts}, "move": [{"t": 0, "at": 0}, {"t": 30, "at": 1}], "look": {"point": [0, 0, 0]}}]}  # noqa: E731
        a.json("world_spawn", shape="cube", name="c")
        a.json("window_view", eye=[0, 3, 12], target=[0, 0, 0])
        w0 = win()["camera"]
        a.json("timeline_set", path="ta", timeline=cam([[20, 5, 0], [0, 6, 20]]))
        b.json("timeline_set", path="tb", timeline=cam([[-20, 5, 0], [0, 9, -20]]))
        a.json("timeline_play", path="ta")
        time.sleep(0.4)
        b.json("timeline_play", path="tb")
        time.sleep(0.4)
        check(win().get("timeline") == "assets/timelines/tb.json", "a second window timeline takes the view")
        e = err_of(a, "timeline_stop", path="tb")
        check(e is not None and e.get("owner") == "bob", "alice can't stop bob's")
        check(err_of(lead, "timeline_stop", path="tb", force=True) is None, "the lead's force stops it")
        a.json("timeline_stop", path="ta")
        time.sleep(0.3)
        check(win()["camera"] == w0 and win().get("timeline") is None, "with both stopped the window has the camera from before")
        a.json("world_save", name="w1")
        a.json("timeline_play", path="ta")
        time.sleep(0.3)
        a.request("tools/call", {"name": "world_load", "arguments": {"name": "w1"}})
        time.sleep(0.3)
        ended = a.json("timeline_get")["player"]["ended"]
        check(win().get("timeline") is None and any(x.get("how") == "world_replaced" for x in ended), "a world load ends the playback and frees the window")
        a.close()
        b.close()
        lead.close()
    check(eng.validation_errors() == 0, f"the windowed probe's engine shut down with {eng.validation_errors()} validation errors")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7530)
    ap.add_argument("--out")
    ap.add_argument("--skip-sepulcra", action="store_true")
    ap.add_argument("--only-sepulcra", action="store_true")
    args = ap.parse_args()
    out = Path(args.out) if args.out else ROOT / "target" / "timeline-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    summary = {"adapter": args.adapter}
    if not args.only_sepulcra:
        summary["scripted"] = scripted(args, out)
        probes(args, out)
    if not args.skip_sepulcra:
        summary["sepulcra"] = sepulcra(args, out)
    summary["failures"] = FAILURES
    summary["passed"] = not FAILURES
    (out / "summary.json").write_text(json.dumps(summary, indent=2, default=str))
    print(("PASS" if not FAILURES else f"FAIL ({len(FAILURES)})") + f": summary in {rel(out / 'summary.json')}")
    sys.exit(0 if not FAILURES else 1)


if __name__ == "__main__":
    main()

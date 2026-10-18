"""Save game check: a loaded save continues bit-identically, and the desync detector says so.

Engine A (Sepulcra, seed 7, headless by default) plays the "centre" build order with a pile of
physics crates tumbling beside the slab and the ambulo module walking its character (so the
world, the game's simulation, physics' hidden state and module state all move). At tick T it
writes save_write "proof", then runs N more ticks and exports its trace (desync_trace T+1..T+N).
Engine B starts fresh, loads ambulo, save_read "proof" (every saved hash must match on load),
runs the same N ticks and exports its trace. desync_compare of the two traces must report a match
over all N ticks, with the world hash and the subsystems sim, economy, enemies, defences,
projectiles, physics and logic all compared.

Also measured: save size and time (snapshot, encode, write), load time, and the same with a
world of --stress extra entities.

Usage: cargo build --release -p sepulcra-app && python scripts/save_check.py [--ticks 900]
       [--save-at 600] [--stress 10000] [--ports 7600 7601] [--window] [--adapter integrated]
       [--out perf/saves/NAME.json]
ASCII output. Exit code 0 = every check held.
"""
import argparse
import json
import os
import shutil
import struct
import sys
import tempfile
import threading
import time
import zlib
from pathlib import Path

from specula_mcp import ROOT, Engine, Mcp, url_for


def exe_path(name):
    return Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / (name + (".exe" if sys.platform == "win32" else ""))


def wait_for(what, probe, timeout=10.0):
    """Polls `probe` until it returns something truthy (background saves and loads answer
    later); returns it, or None after `timeout` seconds."""
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        got = probe()
        if got:
            return got
        time.sleep(0.05)
    print(f"  (timed out waiting for {what})")
    return None


def bomb(path, mib=300):
    """A small save whose world section inflates to `mib` MiB of zeros (the review's bomb)."""
    data = bytes(mib << 20)
    comp = zlib.compress(data, 6)
    hdr = {"format": "specula.save", "game": "sepulcra", "tick": 0, "tick_hz": 30,
           "sections": [{"name": "world", "codec": "zlib", "bytes": len(data), "stored": len(comp), "crc32": "%08x" % (zlib.crc32(data) & 0xffffffff)}]}
    t = json.dumps(hdr).encode()
    out = b"SPECSAVE" + struct.pack("<II", 1, len(t)) + t + comp
    out += struct.pack("<I", zlib.crc32(out) & 0xffffffff)
    path.write_bytes(out)
    return len(out)


def sections_of(b):
    """A save file's version, header and sections [name, decoded bytes]."""
    ver, hl = struct.unpack_from("<II", b, 8)
    hdr = json.loads(b[16:16 + hl])
    at, secs = 16 + hl, []
    for s in hdr["sections"]:
        raw = b[at:at + s["stored"]]
        at += s["stored"]
        secs.append([s["name"], zlib.decompress(raw) if s["codec"] == "zlib" else raw])
    return ver, hdr, secs


def build_save(ver, hdr, secs):
    """A save file from a header and sections (all compressed), checksums made right."""
    table, body = [], b""
    for name, data in secs:
        st = zlib.compress(data, 6)
        table.append({"name": name, "codec": "zlib", "bytes": len(data), "stored": len(st), "crc32": "%08x" % (zlib.crc32(data) & 0xffffffff)})
        body += st
    hdr = dict(hdr, sections=table)
    t = json.dumps(hdr, separators=(",", ":")).encode()
    out = b"SPECSAVE" + struct.pack("<II", ver, len(t)) + t + body
    return out + struct.pack("<I", zlib.crc32(out) & 0xffffffff)


def game_bomb(src, dst, mib=32):
    """The review's crafted save: `src` with a `mib` MiB recording in its game section (far over
    Sepulcra's cap), in a file of about a megabyte."""
    ver, hdr, secs = sections_of(src.read_bytes())
    g = dict(secs)["game"]
    sl = struct.unpack_from("<I", g, 4)[0]
    at = 8 + sl
    rl = struct.unpack_from("<I", g, at)[0]
    rec = b'{"x":[' + b"0," * (mib << 20 >> 1) + b'0]}'
    g2 = g[:at] + struct.pack("<I", len(rec)) + rec + g[at + 4 + rl:]
    dst.write_bytes(build_save(ver, hdr, [[n, g2 if n == "game" else d] for n, d in secs]))
    return dst.stat().st_size


def step(m, n):
    while n > 0:
        k = min(n, 18000)
        m.json("time_control", action="step", ticks=k)
        n -= k


def build_ambulo(m):
    result = m.request("tools/call", {"name": "logic_build", "arguments": {"path": "games/ambulo"}})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    if result.get("isError"):
        raise RuntimeError(f"logic_build failed: {text[:600]}")
    return json.loads(text)


def crates(m):
    ops = [{"op": "spawn", "shape": "group", "name": "crates", "position": [70, 0, 0]},
           {"op": "spawn", "shape": "cube", "parent": "/crates", "name": "floor", "position": [0, -0.5, 0], "scale": [40, 1, 40],
            "collider": {"shape": "box"}}]
    for i in range(24):
        ops.append({"op": "spawn", "shape": "sphere" if i % 4 == 0 else "cube", "parent": "/crates", "name": f"c{i}",
                    "position": [(i % 4) * 0.8 - 1.2, 0.6 + (i // 4) * 1.15, (i % 3) * 0.35 - 0.35],
                    "rotation_deg": [0, i * 13, 4],
                    "collider": {"shape": "sphere", "radius": 0.5} if i % 4 == 0 else {"shape": "box"},
                    "body": {"kind": "dynamic", "mass": 2 + i}})
    m.json("world_batch", ops=ops)


def stress(m, n):
    """n static cubes in a few groups (a big world to save)."""
    made = 0
    g = 0
    while made < n:
        ops = [{"op": "spawn", "shape": "group", "name": f"stress_{g}"}]
        for i in range(min(999, n - made)):
            k = made + i
            ops.append({"op": "spawn", "shape": "cube", "parent": f"/stress_{g}", "name": f"s{k}",
                        "position": [200 + (k % 100) * 1.5, 0, (k // 100) * 1.5], "color": "#808890"})
        m.json("world_batch", ops=ops)
        made += len(ops) - 1
        g += 1


def run(args, work):
    exe = exe_path("sepulcra")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p sepulcra-app")
    common = ["--saves", str(work / "saves")]
    if not args.window:
        common.append("--headless")
    if args.adapter:
        common += ["--adapter", args.adapter]
    problems, report = [], {}

    def check(ok, what):
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")
        if not ok:
            problems.append(what)

    def keep_shot(label, shot):
        entry = {"png": shot["image"]["path"], "packet": shot["image"]["packet_path"]}
        if args.keep:
            args.keep.mkdir(parents=True, exist_ok=True)
            for k in ("path", "packet_path"):
                dst = args.keep / f"{label}{Path(shot['image'][k]).suffix}"
                shutil.copy(shot["image"][k], dst)
                entry["kept_" + k] = str(dst)
        report.setdefault("captures", {})[label] = entry
        print(f"  capture {label}: {entry.get('kept_path', entry['png'])}")

    pa, pb = args.ports
    with Engine(exe, pa, work / "a.log", *common, "--captures", str(work / "cap-a"), "--worlds", str(work / "worlds-a"),
                "--", "--seed", "7", cwd=str(ROOT)):
        a = Mcp(url_for(pa), "save-check-a")
        a.json("agent_hello", name="saver", role="lead")
        a.json("time_control", action="pause")
        a.call("play_new", seed=7, layout="centre")
        crates(a)
        built = build_ambulo(a)
        print(f"A: ambulo loaded ({built.get('reload', {}).get('status')})")
        a.json("logic_input", module="ambulo", actions={"move": [1, 0.3]}, ticks=args.save_at + args.ticks)
        step(a, args.save_at // 2)
        a.call("play_command", action="call_wave")
        a.json("logic_input", module="ambulo", actions={"jump": True}, ticks=2)
        step(a, args.save_at - args.save_at // 2)
        t = a.json("time_control", action="info")["tick"]
        saved = a.json("save_write", name="proof", label="save_check")
        print(f"A: saved at tick {saved['tick']}: {saved['bytes']} bytes, sections {saved['sections']}, ms {saved['timings_ms']}")
        report["save"] = saved
        check(saved["tick"] == t, "the save is at the current tick")
        for name in ("sim", "physics", "logic"):
            check(name in saved["subsystems"], f"the save covers subsystem {name}")
        step(a, args.ticks)
        trace_a = a.json("desync_trace", from_tick=t + 1, to_tick=t + args.ticks)
        print(f"A: trace {trace_a['ticks']} ticks {trace_a['from_tick']}..{trace_a['to_tick']}")
        if args.stress:
            stress(a, args.stress)
            big = a.json("save_write", name="stress", label=f"{args.stress} extra entities")
            print(f"A: stress save ({big['sections']}): {big['bytes']} bytes, ms {big['timings_ms']}")
            report["stress_save"] = big
        a.close()

    with Engine(exe, pb, work / "b.log", *common, "--captures", str(work / "cap-b"), "--worlds", str(work / "worlds-b"),
                "--", "--seed", "99", cwd=str(ROOT)):
        b = Mcp(url_for(pb), "save-check-b")
        b.json("agent_hello", name="loader", role="lead")
        b.json("time_control", action="pause")
        build_ambulo(b)
        started = time.perf_counter()
        loaded = b.json("save_read", name="proof", force=True)
        wall_ms = (time.perf_counter() - started) * 1000
        print(f"B: loaded tick {loaded['tick']} match={loaded['match']} ms {loaded['timings_ms']} (call {wall_ms:.1f} ms)")
        report["load"] = loaded
        check(loaded["match"], "every saved hash matches right after the load")
        check(not loaded["notes"], f"no notes on load ({loaded['notes']})")
        check(b.json("time_control", action="info")["tick"] == t, "the clock is at the save's tick")
        step(b, args.ticks)
        trace_b = b.json("desync_trace", from_tick=t + 1, to_tick=t + args.ticks)
        cmp = b.json("desync_compare", a=trace_a["file"], b=trace_b["file"])
        report["compare"] = cmp
        print(f"desync_compare: match={cmp.get('match')} common_ticks={cmp.get('common_ticks')} "
              f"first_divergent_tick={cmp.get('first_divergent_tick')} components={cmp.get('components')}")
        check(cmp.get("match") is True, "the loaded run matches the saving run on every tick")
        check(cmp.get("common_ticks") == args.ticks, f"over all {args.ticks} ticks")
        rows = json.loads(Path(trace_b["file"]).read_text())["ticks"]
        names = sorted(rows[-1][2].keys()) if rows else []
        report["subsystems_compared"] = names
        for name in ("sim", "economy", "enemies", "defences", "projectiles", "physics", "logic"):
            check(name in names, f"subsystem {name} was compared")
        # The player's way: the pause menu's Save / Load screen (play_ui presses the buttons a
        # click, a key or a pad's accept would): slots, overwrite / load / delete confirmations.
        print("B: the player saves and loads from the Save / Load screen")
        b.json("play_ui", press="menu_button")
        keep_shot("menu", b.frame(width=1280, height=720, include_ui=True))
        pressed = b.json("play_ui", press="saves")
        check(pressed.get("action") == "open_saves", f"the pause menu opens the Save / Load screen ({pressed.get('action')})")

        def slot(name, want=True):
            def probe():
                listed = b.json("save_list")
                found = next((s for s in listed["saves"] if s["name"] == name), None)
                if want:
                    return found if found and not found.get("writing") else None
                return {"gone": True} if found is None else None
            return wait_for(name, probe)

        before = b.json("play_state")
        b.json("play_ui", press="save_1")
        s1 = slot("sepulcra_slot_1")
        check(s1 is not None and s1["created_by"] == "sepulcra" and s1["tick"] == b.json("time_control", action="info")["tick"],
              "Save on an empty slot writes it at once, as the game, at this tick")
        check("Wave" in (s1 or {}).get("label", ""), f"the slot says what it holds ({(s1 or {}).get('label')})")
        b.json("play_ui", press="save_2")
        check(slot("sepulcra_slot_2") is not None, "a second slot")
        wait_for("the screen's list", lambda: True)
        time.sleep(0.2)
        keep_shot("saves", b.frame(width=1280, height=720, include_ui=True))
        # Saving over a used slot asks first.
        b.json("play_ui", press="save_1")
        keep_shot("confirm_overwrite", b.frame(width=1280, height=720, include_ui=True))
        ui = b.json("play_ui")
        check("confirm" in json.dumps(ui), "saving over slot 1 asks first")
        b.json("play_ui", press="confirm_no")
        # Deleting asks too.
        b.json("play_ui", press="delete_2")
        b.json("play_ui", press="confirm_yes")
        check(slot("sepulcra_slot_2", want=False) is not None, "Delete (confirmed) removes slot 2")
        b.json("play_ui", press="saves_back")
        b.json("play_ui", press="resume")
        step(b, 300)
        moved = b.json("play_state")
        check(moved["tick"] > before["tick"], "the run went on after saving")
        b.json("play_ui", press="menu_button")
        b.json("play_ui", press="saves")
        time.sleep(0.2)
        b.json("play_ui", press="load_1")
        b.json("play_ui", press="confirm_yes")
        after = wait_for("the player's load", lambda: (lambda st: st if st["tick"] == before["tick"] else None)(b.json("play_state", include=["log"])))
        after = after or b.json("play_state", include=["log"])
        check(after["state_hash"] == before["state_hash"] and after["tick"] == before["tick"], "Load (confirmed) brought the saved run back")
        check(after["paused"] is True, "and it waits, paused under the menu")
        said = [m["message"] for m in after.get("log", {}).get("items", after.get("log", [])) if isinstance(m, dict)]
        check(any(m.startswith("Game loaded") for m in said), f"the player is told ({said[:3]})")
        # The wave-start autosave (off headless; Settings switches it on).
        b.json("play_ui", press="settings")
        b.json("play_ui", press="autosave")
        b.json("play_ui", press="settings_back")
        b.json("play_ui", press="resume")
        b.call("play_command", action="call_wave")
        step(b, 2)
        auto = wait_for("an autosave", lambda: next((s for s in b.json("save_list")["saves"] if s["name"].startswith("sepulcra_autosave_") and not s.get("writing")), None))
        log = [m["message"] for m in b.json("play_state", include=["log"]).get("log", {}).get("items", []) if isinstance(m, dict)]
        check(auto is not None and "Autosave" in auto.get("label", ""), f"a wave start autosaves ({(auto or {}).get('name')}; log {log[:3]})")
        # What the loaded world looks like (the slab and the stones adopted, enemies rebuilt).
        keep_shot("loaded_world", b.frame(width=1280, height=720, eye=[0, 38, 30], target=[0, 0, 0]))
        # Loads read and check off the engine thread: another agent's calls keep answering.
        other = Mcp(url_for(pb), "save-check-bystander")
        latencies, done = [], threading.Event()

        def poke():
            while not done.is_set():
                t0 = time.perf_counter()
                other.json("world_list", limit=1)
                latencies.append((time.perf_counter() - t0) * 1000)
                time.sleep(0.01)

        size = bomb(work / "saves" / "bomb.specsave")
        th = threading.Thread(target=poke)
        th.start()
        res = b.request("tools/call", {"name": "save_read", "arguments": {"name": "bomb", "force": True}})
        err = json.loads(res["content"][0]["text"])
        # 300 MiB of zeros is under the 512 MiB decoded cap and DEFLATE's ratio: it is inflated on
        # the load's worker (the engine thread never sees it), then refused (no world in it).
        check(res.get("isError") and err.get("error") in ("too_large", "corrupt_section"), f"a {size}-byte decompression bomb is refused, off the engine thread ({err.get('error')}: {err.get('message', '')[:90]})")
        # The review's game-section bomb: a 32 MiB recording in a ~1 MB file, refused by the game's
        # cap on the load's worker.
        gsize = game_bomb(work / "saves" / "sepulcra_slot_1.specsave", work / "saves" / "gamebomb.specsave")
        t0 = time.perf_counter()
        res = b.request("tools/call", {"name": "save_read", "arguments": {"name": "gamebomb", "force": True}})
        gms = (time.perf_counter() - t0) * 1000
        err = json.loads(res["content"][0]["text"])
        check(res.get("isError") and err.get("error") == "game_state" and "recording" in err.get("message", "") + json.dumps(err),
              f"a {gsize}-byte save with a 32 MiB recording is refused by the game's cap in {gms:.0f} ms ({err.get('message', '')[:100]})")
        # Inspections stream the file and inflate nothing; at most 4 run at once (busy beyond).
        target_name = "stress" if args.stress else "sepulcra_slot_1"
        answers = [None] * 8

        def look(i):
            c = Mcp(url_for(pb), f"save-check-inspect-{i}")
            r = c.request("tools/call", {"name": "save_read", "arguments": {"name": target_name, "inspect": True}})
            answers[i] = (r.get("isError"), json.loads(r["content"][0]["text"]))
            c.close()

        lookers = [threading.Thread(target=look, args=(i,)) for i in range(8)]
        for t in lookers:
            t.start()
        for t in lookers:
            t.join()
        ok = [a for a in answers if a and not a[0] and a[1].get("reads")]
        busy = [a for a in answers if a and a[0] and a[1].get("error") == "busy"]
        check(len(ok) + len(busy) == 8 and ok, f"8 inspections at once: {len(ok)} read the table and checksum, {len(busy)} busy (none failed)")
        if args.stress:
            started = time.perf_counter()
            big = b.json("save_read", name="stress", force=True)
            print(f"B: stress load: match={big['match']} ms {big['timings_ms']} (call {(time.perf_counter() - started) * 1000:.1f} ms)")
            report["stress_load"] = big
            check(big["match"], "the stress save loads with matching hashes")
        done.set()
        th.join()
        # A load while a playtest runs would move the clock and the world under it: refused, for
        # an agent (even with force) and for the player's Load.
        spec = {"name": "long", "ticks": 60000, "steps": [{"at": 0, "call": "play_state"}, {"at": 50000, "call": "play_state"}]}
        b.request("tools/call", {"name": "playtest_run", "arguments": {"spec": spec, "wait_s": 0}})
        res = other.request("tools/call", {"name": "save_read", "arguments": {"name": "sepulcra_slot_1", "force": True}})
        err = json.loads(res["content"][0]["text"])
        check(res.get("isError") and err.get("error") == "playtest_running", f"another agent's load during a playtest is refused ({err.get('error')})")
        for press in ("menu_button", "saves", "load_1", "confirm_yes"):
            other.request("tools/call", {"name": "play_ui", "arguments": {"press": press}})
        said = wait_for("the player's refused load", lambda: [m for m in (lambda st: [x.get("message", "") for x in st.get("log", {}).get("items", []) if isinstance(x, dict)])(other.json("play_state", include=["log"])) if "playtest" in m])
        check(bool(said), f"the player's Load during a playtest is refused and says why ({(said or [''])[0][:90]})")
        b.request("tools/call", {"name": "playtest_report", "arguments": {"cancel": True, "wait_s": 5}})
        lat = sorted(latencies)
        if lat:
            print(f"  bystander world_list during the loads: {len(lat)} calls, median {lat[len(lat) // 2]:.1f} ms, max {lat[-1]:.1f} ms")
            report["bystander_during_loads_ms"] = {"median": lat[len(lat) // 2], "max": lat[-1], "calls": len(lat)}
        other.close()
        b.close()
    return problems, report


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--ticks", type=int, default=900, help="ticks run after the save, compared")
    ap.add_argument("--save-at", type=int, default=600, help="ticks played before the save")
    ap.add_argument("--stress", type=int, default=10000, help="extra entities for the size/time measurement (0: skip)")
    ap.add_argument("--ports", type=int, nargs=2, default=[7600, 7601])
    ap.add_argument("--window", action="store_true", help="open the windows (default headless)")
    ap.add_argument("--adapter", default=None, help="discrete | integrated | <index>")
    ap.add_argument("--out", type=Path, help="save the report as JSON here")
    ap.add_argument("--keep", type=Path, help="copy the pause menu capture and its packet here")
    args = ap.parse_args()
    if 7471 in args.ports:
        sys.exit("never run on 7471 (the interactive engine's port)")
    work = Path(tempfile.mkdtemp(prefix="specula-saves-"))
    try:
        problems, report = run(args, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2) + "\n")
    for p in problems:
        print(f"  PROBLEM: {p}")
    print("PASS" if not problems else "FAIL")
    sys.exit(0 if not problems else 1)


if __name__ == "__main__":
    main()

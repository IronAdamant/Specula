"""World files end to end: build a nested world over MCP, save, restart the engine, load, compare.

Run 1 builds a small hierarchy with world_batch (groups, lathes, glass, a light, tags, props,
reserved physics components), checks versions and queries, saves with world_save, makes one more
edit (left for the shutdown autosave), captures a frame at a fixed camera and time, then stops the
engine with Ctrl+Break. Run 2 starts with the same --world, and must report the same state hash,
the same entities (versions and attribution included), the same subjects and the same PNG bytes.
world_load must then reload it unchanged.

Usage: cargo build -p specula && python scripts/world_roundtrip.py [--port 7481] [--out DIR]
Needs a GPU (Vulkan); runs headless.
"""
import argparse
import base64
import json
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Mcp:
    """Minimal MCP client over Streamable HTTP (JSON responses)."""

    def __init__(self, url, client_name):
        self.url, self.session, self.next_id = url, None, 1
        self.request("initialize", {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": client_name, "version": "0"},
        })
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, msg):
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        req = urllib.request.Request(self.url, json.dumps(msg).encode(), headers)
        with urllib.request.urlopen(req, timeout=60) as r:
            self.session = self.session or r.headers.get("Mcp-Session-Id")
            body = r.read()
        return json.loads(body) if body else None

    def request(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def raw(self, tool, **args):
        return self.request("tools/call", {"name": tool, "arguments": args})

    def call(self, tool, **args):
        result = self.raw(tool, **args)
        if result.get("isError"):
            raise RuntimeError(f"{tool}: {result['content'][0]['text']}")
        return json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))

    def fails(self, tool, **args):
        """The structured error of a call that must fail."""
        result = self.raw(tool, **args)
        if not result.get("isError"):
            raise RuntimeError(f"{tool} should have failed: {result['content'][0]['text'][:200]}")
        return json.loads(result["content"][0]["text"])

    def frame(self):
        result = self.raw("sight_frame", width=480, height=270, eye=[6.5, 5.0, 8.0], target=[0.0, 0.6, 0.0], time_s=2.5)
        if result.get("isError"):
            raise RuntimeError(f"sight_frame: {result['content'][0]['text']}")
        png = base64.b64decode(next(c["data"] for c in result["content"] if c["type"] == "image"))
        packet = json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))
        return png, packet

    def everything(self):
        """Every entity, page by page (small pages, to exercise the cursor)."""
        items, cursor = [], None
        while True:
            args = {"limit": 4}
            if cursor:
                args["cursor"] = cursor
            page = self.call("world_list", **args)
            items += page["entities"]
            cursor = page["next_cursor"]
            if not cursor:
                return page["total"], items


def server_up(url):
    try:
        urllib.request.urlopen(urllib.request.Request(url, method="GET"), timeout=1)
    except urllib.error.HTTPError:
        return True  # 405 from our server
    except OSError:
        return False
    return True


def start(exe, out, port, run):
    log_path = out / f"engine_run{run}.log"
    log = open(log_path, "w")
    proc = subprocess.Popen(
        [str(exe), "--headless", "--port", str(port), "--worlds", str(out / "worlds"), "--world", "e2e",
         "--captures", str(out / "captures")],
        stderr=log, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        cwd=str(ROOT),  # the walker's recipe path is relative to the repository
    )
    url = f"http://127.0.0.1:{port}/mcp"
    for _ in range(150):
        if proc.poll() is not None:
            sys.exit(f"run {run}: engine exited early:\n{log_path.read_text()}")
        if server_up(url):
            return proc, log, log_path
        time.sleep(0.2)
    proc.kill()
    sys.exit(f"run {run}: MCP server never came up")


def stop(proc, log, log_path, run):
    proc.send_signal(signal.CTRL_BREAK_EVENT)
    code = proc.wait(timeout=30)
    log.close()
    last = log_path.read_text().strip().splitlines()[-1]
    if code != 0 or "shut down" not in last:
        sys.exit(f"run {run}: unclean shutdown (exit {code}): {last}")


def build(m):
    hello = m.call("agent_hello", name="mason", role="builder")
    # (The reply also carries a resume_token and a note since resumable identities.)
    assert (hello["agent"], hello["role"]) == ("mason", "builder"), hello
    vase = [[0.0, 0.0], [0.12, 0.0], [0.16, 0.18], [0.07, 0.42], [0.09, 0.5]]
    ops = [
        {"op": "spawn", "shape": "group", "path": "/slab", "position": [0.1, 0.0, -0.2], "rotation_deg": [0, 12.5, 0]},
        {"op": "spawn", "shape": "quad", "path": "/slab/floor", "scale": [6, 1, 4], "color": "#4a4f57",
         "tags": ["ground"], "collider": {"shape": "box", "size": [1, 0.01, 1]}, "body": "static"},
        {"op": "spawn", "shape": "group", "path": "/slab/stones", "position": [-1.3, 0, 0.4]},
    ]
    for i in range(4):
        ops.append({"op": "spawn", "shape": "cube", "path": f"/slab/stones/ward_{i:02}",
                    "position": [i * 0.7, 0.35 + i / 30.0, -0.1 * i], "rotation_deg": [0, 1.0 / 3.0 * i * 10, 0],
                    "scale": [0.3, 0.7, 0.3], "color": [0.31, 0.62 - i * 0.05, 0.93],
                    "tags": ["ward", "stone"], "props": {"hp": 10 + i, "rune": f"r{i}"},
                    "collider": {"shape": "box"}, "body": {"kind": "static"}})
    ops += [
        {"op": "spawn", "shape": "group", "path": "/slab/lamp", "position": [1.4, 0, 0.6]},
        {"op": "spawn", "shape": "lathe", "path": "/slab/lamp/vase", "profile": vase, "segments": 28, "color": "#7a5a3a",
         "metallic": 0.3, "roughness": 0.35, "specular": 1.2},
        {"op": "spawn", "shape": "sphere", "path": "/slab/lamp/bulb", "position": [0, 0.62, 0], "scale": 0.16,
         "color": "#ffcf7a", "emissive": 3.0, "light": {"intensity": 1.6, "range": 4.0}},
        {"op": "spawn", "shape": "sphere", "path": "/slab/lamp/shade", "position": [0, 0.62, 0], "scale": 0.3,
         "material": "glass", "color": "#d8e8ff", "opacity": 0.2},
        {"op": "spawn", "shape": "cube", "path": "/pawn", "position": [2.2, 0.45, -1.1],
         "scale": [0.35, 0.9, 0.35], "color": "#b03a2e", "tags": ["unit"],
         "collider": {"shape": "capsule", "radius": 0.2, "height": 0.9, "layer": "units"},
         "body": {"kind": "kinematic", "mass": 70}},
        # An animated model (anim starts are world time: they must survive save, restart, load).
        {"op": "spawn", "shape": "model", "path": "/walker", "recipe": "games/sepulcra/assets/recipes/shambler.json",
         "position": [-2.2, 0, -1.0], "anim": {"clip": "walk", "time_offset": 0.3}},
        {"op": "set", "path": "/slab/stones/ward_03", "parent": "/slab", "rename": "ward_lonely", "add_tags": ["moved"]},
    ]
    result = m.call("world_batch", ops=ops)
    assert result["applied"] == len(ops), result
    assert result["results"][-1] == {"op": "set", "id": 7, "path": "/slab/ward_lonely", "version": 2}, result["results"][-1]

    # All or nothing: the bad last op leaves no trace of the first.
    err = m.fails("world_batch", ops=[{"op": "spawn", "shape": "cube", "path": "/ghost"},
                                      {"op": "delete", "path": "/nowhere"}])
    assert (err["error"], err["op_index"], err["cause"]["error"]) == ("batch_failed", 1, "not_found"), err
    assert m.fails("world_delete", path="/ghost")["error"] == "not_found"

    # Versions: a stale write is rejected with the current state.
    ward = m.call("world_set", path="/slab/stones/ward_01", expect_version=1, color="#5b8fd6")
    assert ward["version"] == 2 and ward["modified_by"] == "mason", ward
    stale = m.fails("world_set", path="/slab/stones/ward_01", expect_version=1, color="#ff0000")
    assert stale["error"] == "stale_version" and stale["current"]["color"] == "#5b8fd6", stale
    m.call("world_env", sun_direction=[0.3, 0.9, 0.4], ambient_intensity=0.3, exposure=1.1, tonemap=True,
           background="#20242c")
    # A crossfade started now (on world time): its start and fade source are saved with it.
    time.sleep(1.0)
    fade = m.call("world_set", path="/walker", anim={"clip": "attack", "crossfade": 0.5})
    assert fade["anim"]["start"] > 0 and fade["anim"]["from"]["clip"] == "walk", fade["anim"]

    q = m.call("world_query", tags=["ward"], fields=["path", "version", "props"])
    assert q["total"] == 4 and [i["path"] for i in q["items"]] == [
        "/slab/stones/ward_00", "/slab/stones/ward_01", "/slab/stones/ward_02", "/slab/ward_lonely"], q
    region = [i["path"] for i in m.call("world_query", region={"min": [1.0, 0.2, -1.5], "max": [3.0, 1.0, -0.8]})["items"]]
    assert "/pawn" in region and "/slab/floor" not in region and region == sorted(region), region


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--exe", type=Path, default=ROOT / "target" / "debug" / "specula.exe")
    ap.add_argument("--port", type=int, default=7481)
    ap.add_argument("--out", type=Path, default=ROOT / "target" / "world-roundtrip")
    args = ap.parse_args()
    if sys.platform != "win32":
        sys.exit("world_roundtrip: Windows only for now")
    if not args.exe.is_file():
        sys.exit(f"world_roundtrip: {args.exe} not found (cargo build -p specula)")
    url = f"http://127.0.0.1:{args.port}/mcp"
    if server_up(url):
        sys.exit(f"world_roundtrip: something is already serving on port {args.port}; stop it or pass --port")
    shutil.rmtree(args.out, ignore_errors=True)
    args.out.mkdir(parents=True)

    proc, log, log_path = start(args.exe, args.out, args.port, 1)
    try:
        m = Mcp(url, "roundtrip")
        build(m)
        saved = m.call("world_save")
        assert saved["saved"] == "e2e" and saved["entities"] == 13, saved
        # A change after the save, then the periodic autosave (worker thread, every 10 s) picks it up.
        m.call("world_set", path="/slab/stones/ward_00", props={"hp": 99})
        for _ in range(40):
            world = m.call("engine_info")["world"]
            if not world["unsaved_changes"]:
                break
            time.sleep(0.5)
        assert not world["unsaved_changes"] and world["last_autosave_error"] is None, world
        slab_files = sorted(p.name for p in (args.out / "worlds" / "e2e").glob("slab.*.json"))
        assert slab_files == ["slab.2.json"], slab_files
        # One more change: only the shutdown autosave can keep it.
        m.call("world_set", path="/pawn", rotation_deg=[0, 33.3, 0])
        info1 = m.call("engine_info")
        assert info1["world"]["unsaved_changes"] is True, info1["world"]
        assert any(a["name"] == "mason" and a["role"] == "builder" for a in info1["agents"]), info1["agents"]
        png1, packet1 = m.frame()
        total1, entities1 = m.everything()
    finally:
        if proc.poll() is None:
            stop(proc, log, log_path, 1)
    files = sorted(p.name for p in (args.out / "worlds" / "e2e").iterdir())
    print(f"run 1: {total1} entities, state {info1['world']['state_hash']}, files {files}")
    # The shutdown autosave only touched /pawn (in world.json): the slab subtree keeps generation 2.
    assert files == ["slab.2.json", "world.json"], files

    proc, log, log_path = start(args.exe, args.out, args.port, 2)
    problems = []
    try:
        m = Mcp(url, "roundtrip")
        info2 = m.call("engine_info")
        # A second engine on the same world must be turned away, not allowed to interleave saves.
        rival = subprocess.run(
            [str(args.exe), "--headless", "--port", str(args.port + 1), "--worlds", str(args.out / "worlds"),
             "--world", "e2e", "--captures", str(args.out / "captures")],
            capture_output=True, text=True, timeout=60,
        )
        if rival.returncode == 0 or "world_locked" not in rival.stderr:
            problems.append(f"a second engine on the same world was not refused: {rival.returncode} {rival.stderr[-300:]}")
        png2, packet2 = m.frame()
        total2, entities2 = m.everything()
        # world_load must restore a world that has really changed. Save a copy first, so autosave
        # now targets the copy and can never write the damage into e2e.
        m.call("world_save", name="e2e_copy")
        m.call("world_delete", path="/slab/lamp")
        damaged, _ = m.frame()
        if damaged == png1:
            problems.append("deleting the lamp did not change the frame")
        # The copy is now the current world: saving over e2e by name must be refused.
        clash = m.fails("world_save", name="e2e")
        if (clash["error"], clash["entities"]) != ("world_exists", 13):
            problems.append(f"world_save over another world: {clash}")
        refused = m.fails("world_load", name="e2e")
        if refused["error"] != "unsaved_changes":
            problems.append(f"world_load over unsaved changes: {refused}")
        reloaded = m.call("world_load", name="e2e", force=True)
        png3, _ = m.frame()
        if info2["world"]["state_hash"] != info1["world"]["state_hash"]:
            problems.append(f"state hash {info2['world']['state_hash']} != {info1['world']['state_hash']}")
        if reloaded["state_hash"] != info1["world"]["state_hash"]:
            problems.append(f"world_load state hash {reloaded['state_hash']}")
        if info2["world"]["unsaved_changes"]:
            problems.append("a freshly loaded world reports unsaved changes")
        if (total2, entities2) != (total1, entities1):
            problems.append("entity listings differ")
        if packet2["subjects"] != packet1["subjects"]:
            problems.append("sight subjects differ")
        if png2 != png1 or png3 != png1:
            problems.append(f"PNG bytes differ ({len(png1)} / {len(png2)} / {len(png3)})")
    finally:
        if proc.poll() is None:
            stop(proc, log, log_path, 2)
    (args.out / "run1.png").write_bytes(png1)
    (args.out / "run2.png").write_bytes(png2)
    print(f"run 2: {total2} entities, state {info2['world']['state_hash']}, "
          f"{len(packet2['subjects'])} subjects, PNG {len(png2)} bytes")
    for p in problems:
        print(f"  PROBLEM: {p}")
    print("IDENTICAL" if not problems else "DIFFERENT", f"- captures in {args.out}")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()

"""Recipe-write stall check: the M4 exit proof's builder load, smaller, against Sepulcra's window.

In the M4 exit run, windowed recipe writes and studio sheets raised frame-budget incidents on the
iGPU (ROADMAP, M4 follow-ups). This reproduces that load and fails on any of them:
  - 4 builders, concurrently, each with 8 or 9 of the 33 graveyard props: project_write the recipe,
    style_check and studio_check it, stage a model entity of it, then write a revision (another
    seed: every staged entity rebuilds and its new build lands in the window);
  - then one studio_sheet of all 33 (the director's contact sheet) and one under worst light;
  - Sepulcra runs windowed (default: the Intel iGPU) with its game going.
It reads every engine.stall and frame.over_budget event of the run from the trace and fails when
one names project_write, world_spawn, a studio tool or a window frame (the engine's own work
during the load), or when validation messages were logged.

Usage: cargo build --release -p sepulcra-app && python scripts/recipe_stall_check.py
       [--port 7552] [--adapter integrated] [--exe PATH] [--out DIR] [--headless]
Results: <target>/recipe-stall-check/report.json, the engine log and trace. ASCII output only.
"""
import argparse
import json
import os
import shutil
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, url_for  # noqa: E402
from m4_exit import NAMES, SEPULCRA_ASSETS  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
EXE = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
STAGING = "/staging/recipe_stall"
WATCHED = {"project_write", "world_spawn", "studio_check", "studio_sheet", "studio_stage", "style_check", "frame"}
LOCK = threading.Lock()


def log(msg):
    with LOCK:
        print(msg, flush=True)


def recipe_text(name, seed_bump=0):
    text = (SEPULCRA_ASSETS / "recipes" / f"{name}.json").read_text(encoding="utf-8")
    if seed_bump:
        j = json.loads(text)
        j["seed"] = int(j.get("seed", 1)) + seed_bump
        text = json.dumps(j, indent=1)
    return text


class Builder(threading.Thread):
    def __init__(self, port, name, props):
        super().__init__(daemon=True)
        self.port, self.name, self.props = port, name, props
        self.error, self.done = None, []

    def call(self, mcp, tool, **args):
        for _ in range(60):
            r = mcp.request("tools/call", {"name": tool, "arguments": args})
            text = next((c["text"] for c in r["content"] if c["type"] == "text"), "")
            if not r.get("isError"):
                return json.loads(text) if text.startswith(("{", "[")) else text
            if any(k in text for k in ("engine_busy", "not_found", "recipe_not_found", "building", "try again")):
                time.sleep(0.5)
                continue
            raise RuntimeError(f"{self.name}: {tool}: {text[:400]}")
        raise RuntimeError(f"{self.name}: {tool} kept failing")

    def run(self):
        try:
            mcp = Mcp(url_for(self.port), self.name)
            mcp.hello(self.name, role="builder")
            for i, name in enumerate(self.props):
                path = f"assets/recipes/{name}.json"
                self.call(mcp, "project_write", path=path, text=recipe_text(name))
                self.call(mcp, "style_check", subjects=[{"recipe": f"recipes/{name}.json"}])
                self.call(mcp, "studio_check", subject={"recipe": f"recipes/{name}.json"}, image=False)
                self.call(mcp, "world_spawn", shape="model", recipe=f"recipes/{name}.json", name=name, parent=STAGING,
                          position=[(i % 4) * 3.0, 0, (hash(self.name) % 7) * 3.0], tags=["staging"])
            # Revisions: every staged entity rebuilds and its new build lands in the window.
            for name in self.props:
                path = f"assets/recipes/{name}.json"
                cur = self.call(mcp, "project_read", path=path, problems=False)
                self.call(mcp, "project_write", path=path, text=recipe_text(name, seed_bump=1), expect_version=cur["version"])
                self.call(mcp, "studio_check", subject={"recipe": f"recipes/{name}.json"}, image=False)
                self.done.append(name)
            log(f"{self.name}: {len(self.done)} props written twice, checked and staged")
        except Exception as e:  # noqa: BLE001 - reported by the main thread
            self.error = str(e)
            log(f"{self.name}: stopped: {e}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7552)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=str(TARGET / "recipe-stall-check"))
    ap.add_argument("--headless", action="store_true")
    a = ap.parse_args()
    out = Path(a.out)
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    assets = out / "assets"
    shutil.copytree(SEPULCRA_ASSETS, assets, ignore=shutil.ignore_patterns("taste"))
    for n in NAMES:
        (assets / "recipes" / f"{n}.json").unlink(missing_ok=True)
    args = [*(["--headless"] if a.headless else []), "--adapter", a.adapter, "--size", "1600x900", "--assets", str(assets),
            "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--trace", str(out / "trace"),
            "--lead", "director", "--", "--seed", "7", "--mute"]
    report = {"adapter": a.adapter, "failures": []}
    fails = report["failures"]
    t0 = time.time()
    with Engine(Path(a.exe), a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        director = eng.mcp("director")
        director.hello("director", role="lead")
        report["gpu"] = director.json("engine_info")["gpu"]["name"]
        log(f"gpu: {report['gpu']}")

        def heap():
            """The image heap's counters (None on builds before it existed, M5 gpuwork)."""
            p = director.json("perf_frame")
            return (((p.get("window") or p.get("offscreen") or {}).get("memory") or {}).get("image_heap")) or None

        heap_before = heap()
        director.call("world_spawn", shape="group", name="staging", position=[0, 0, 0])
        director.call("world_spawn", shape="group", name="recipe_stall", parent="/staging", position=[200, 0, 0])
        builders = [Builder(a.port, f"builder-{i + 1}", NAMES[i::4]) for i in range(4)]
        rec = {}

        def record():
            # The engine thread's zones over the writes (perf_capture: 600 window frames).
            try:
                rec["out"] = eng.mcp("recipe-stall-perf").json("perf_capture", frames=600, trace=False, label="writes")
            except Exception as e:  # noqa: BLE001 - reported below
                rec["error"] = str(e)

        recorder = threading.Thread(target=record, daemon=True)
        recorder.start()
        # The load starts here: start-up (pipelines, the slab's builds) is not what this checks.
        report["load_started_unix_ms"] = load_started = time.time() * 1000
        for b in builders:
            b.start()
        for b in builders:
            b.join(timeout=900)
        for b in builders:
            if b.error or b.is_alive():
                fails.append(f"{b.name}: {b.error or 'timed out'}")
        recorder.join(timeout=120)
        cap = rec.get("out") or {}
        if cap.get("path"):
            worst = {}
            data = json.loads(Path(cap["path"]).read_text(encoding="utf-8"))
            for f in data["frames"]:
                for rows in f["zones"].values():
                    for row in rows:  # [path, calls, ms, self_ms, max_call_ms, ...]
                        leaf = row[0].split("/")[-1]
                        worst[leaf] = max(worst.get(leaf, 0.0), row[4])
            top = sorted(worst.items(), key=lambda kv: -kv[1])[:25]
            report["zones_worst_ms"] = top
            log("worst engine-thread zones over the writes: " + ", ".join(f"{k} {v:.1f}" for k, v in top[:15]))
        subjects = [{"recipe": f"recipes/{n}.json"} for n in NAMES]
        sheet = director.json("studio_sheet", subjects=subjects, views=["three_quarter", "gameplay"], rig="game", tile=176)
        log(f"contact sheet: {(sheet.get('image') or {}).get('path')}")
        director.json("studio_sheet", subjects=subjects[:24], views=["worst_light", "phone"])
        time.sleep(2.0)
        heap_after = heap()
        report["image_heap"] = {"before": heap_before, "after": heap_after}
        if heap_before and heap_after:
            sync = heap_after["sync_blocks"] - heap_before["sync_blocks"]
            log(f"image heap: {heap_after['blocks']} blocks ({heap_after['block_mb']} MB, {heap_after['images']} images), "
                f"{heap_after['grown_ahead'] - heap_before['grown_ahead']} grown ahead, {sync} on the engine thread")
            if sync > 0:
                fails.append(f"{sync} image heap block(s) were allocated on the engine thread during the load")
        report["seconds"] = round(time.time() - t0, 1)
    trace_files = sorted((out / "trace").glob("*.jsonl"))
    events = []
    for f in trace_files:
        for line in f.read_text(errors="replace", encoding="utf-8").splitlines():
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            if e.get("code") in ("engine.stall", "frame.over_budget") and e.get("unix_ms", 0) >= load_started:
                events.append(e)
    report["events"] = [{"code": e["code"], "t_ms": e.get("t_ms"), **{k: e["fields"].get(k) for k in ("tool", "zone", "ms", "budget_ms", "cpu_ms", "lock_ms", "cause", "gpu_ms")}} for e in events]
    stalls = [e for e in report["events"] if e["code"] == "engine.stall" and (e.get("tool") in WATCHED)]
    over = [e for e in report["events"] if e["code"] == "frame.over_budget"]
    log(f"engine.stall events: {len([e for e in report['events'] if e['code'] == 'engine.stall'])} ({len(stalls)} from this load); frame.over_budget: {len(over)}")
    for e in report["events"][:40]:
        log(f"  {e}")
    if stalls:
        # A stall whose thread ran for under half of it waited for the rest (a processor on a
        # busy machine, a lock, the GPU): still a late frame or call, and said as what it is.
        waited = [e for e in stalls if e.get("cpu_ms") is not None and e["cpu_ms"] < (e["ms"] or 0) * 0.5 and (e["ms"] or 0) - e["cpu_ms"] >= 2]
        locked = [e for e in waited if (e.get("lock_ms") or 0) >= 1]
        note = f"; in {len(waited)} of them the engine thread ran for under half the time and waited the rest ({len(locked)} for the model library's lock, which a worker held; the others for a processor, a disk or the GPU: is the machine busy?)" if waited else ""
        fails.append(f"{len(stalls)} engine-thread stalls from recipe writes, staging and studio calls (worst {max(e['ms'] or 0 for e in stalls)} ms){note}")
    if over:
        fails.append(f"{len(over)} frame.over_budget incidents (window GPU time past perf.toml)")
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    if errors:
        fails.append(f"validation errors: {errors}")
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    log(f"report: {out / 'report.json'}")
    if fails:
        for f in fails:
            log("  FAIL " + f)
        raise SystemExit("FAILED")
    log("PASSED: no stall or frame-budget incident from recipe writes and studio sheets")


if __name__ == "__main__":
    main()

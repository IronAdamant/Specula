#!/usr/bin/env python3
"""M3 exit proof: "The playable build holds budget on the integrated GPU" (ROADMAP, M3).

Plays a whole game of Sepulcra windowed at 1280x720 on the Intel iGPU (the reference device, F3),
with its rules running from Lex and its shamblers dying as ragdolls, and `perf.example.toml`'s
budgets. In real time, during the busiest wave and during the boss wave, `perf_capture` records
window frames and checks the budgets on them. The criterion passes when no frame-level budget is
missed (the same set as M2's criterion 3: the frame's GPU time, frame pacing, draw calls,
triangles, GPU memory, pipelines built after warm-up and the window's draw-list CPU time). Per-pass
GPU and other CPU budgets are reported beside it as notes.

It also requires the game to play to its end, the recording to replay as verified (Lex parity
included) and zero validation messages at shutdown. Other engines and compilers running at the
same time skew timings, so their count is recorded with each capture, and so are the desktop apps
running (the owner's machine runs Discord, Brave, Obsidian and Steam beside the engine: the
criterion must hold with them open) and, on Windows, every other process's share of the engine's
GPU while it measured (the 3D engine's utilisation from the OS's GPU counters; DWM composes the
engine's window there).

    cargo build -p sepulcra-app --release
    python scripts/m3_exit.py [--adapter integrated] [--port 7488] [--frames 600] [--out DIR] [--max-fps N] [--gi]

With `--gi` the game's top face gets a baked GI volume (`gi_bake`, rebaked at each measured wave),
and each measured wave is captured six times back to back with `gi_settings` alternating off, low,
high, off, low, high (the frame-level budgets then decide on the off and low captures: low is the
integrated GPU's default tier). Each capture also prints the scene pass's mean and p95.

Output (summary.json, captures, engine log) goes to target/m3-exit/<adapter>/. Keep output ASCII.
"""
import argparse
import collections
import json
import subprocess
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine, detailed
from sepulcra_play import EXE, Run
from m2_exit import PERF_TOML, is_frame_metric, other_engines


def compilers_running():
    """How many cargo/rustc processes are running: other sessions' builds skew the timings."""
    if sys.platform != "win32":
        return 0
    n = 0
    for image in ("cargo.exe", "rustc.exe"):
        r = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"],
                           capture_output=True, text=True)
        n += sum(1 for line in r.stdout.splitlines() if line.startswith('"'))
    return n


# Desktop apps that share the GPU on a typical machine (the owner's: Discord, Brave, Obsidian, Steam).
DESKTOP_APPS = ("discord", "brave", "chrome", "msedge", "firefox", "obsidian", "steam", "steamwebhelper",
                "spotify", "ms-teams", "teams", "slack", "zoom", "obs64", "vlc")


def desktop_apps():
    """Desktop apps running, by image name with their process counts ({} off Windows)."""
    if sys.platform != "win32":
        return {}
    r = subprocess.run(["tasklist", "/FO", "CSV", "/NH"], capture_output=True, text=True)
    n = collections.Counter()
    for line in r.stdout.splitlines():
        image = line.split(",")[0].strip('"').lower()
        if image.endswith(".exe") and image[:-4] in DESKTOP_APPS:
            n[image[:-4]] += 1
    return dict(sorted(n.items()))


# Prints every process's 3D-engine utilisation once a second (per GPU, by its LUID) as CSV lines.
GPU_SAMPLER = r"""
while ($true) {
    $t = (Get-Date).ToString("HHmmss")
    try { $s = (Get-Counter "\GPU Engine(*engtype_3D)\Utilization Percentage" -ErrorAction Stop).CounterSamples } catch { continue }
    foreach ($x in $s) { if ($x.CookedValue -gt 0.05) {
        $f = $x.InstanceName -split "_"
        $p = Get-Process -Id $f[1] -ErrorAction SilentlyContinue
        $line = "{0},{1},{2},{3}_{4},{5:F2}" -f $t, $p.ProcessName, $f[1], $f[3], $f[4], $x.CookedValue
        [Console]::Out.WriteLine($line)
    } }
    [Console]::Out.Flush()
}
"""


class GpuUsers:
    """Other processes' share of the engine's GPU while it measures (Windows' GPU counters)."""

    def __init__(self, engine_pid):
        self.pid, self.proc = str(engine_pid), None
        if sys.platform == "win32":
            self.proc = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_SAMPLER],
                                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)

    def stop(self):
        """{process: [mean %, max %]} on the engine's GPU over the samples, the engine's own share
        first (as "engine"); {} where it can't be told."""
        if self.proc is None:
            return {}
        self.proc.kill()
        out, _ = self.proc.communicate()
        rows = [line.split(",") for line in out.splitlines() if line.count(",") == 4]
        luid = collections.Counter(r[3] for r in rows if r[2] == self.pid).most_common(1)
        if not luid:
            return {}
        times = {r[0] for r in rows}
        total = collections.defaultdict(float)
        peak = collections.defaultdict(float)
        for _t, name, pid, gpu, util in rows:
            if gpu == luid[0][0]:
                key = "engine" if pid == self.pid else (name or pid)
                total[key] += float(util)
                peak[key] = max(peak[key], float(util))
        n = max(len(times), 1)
        order = sorted(total, key=lambda k: (k != "engine", -total[k]))
        return {k: [round(total[k] / n, 1), round(peak[k], 1)] for k in order[:8]}


def measure(mcp, eng, label, frames, st):
    """Records `frames` window frames in real time and checks the frame-level budgets."""
    mcp.call("time_control", action="resume")
    gpu_users = GpuUsers(eng.proc.pid)
    time.sleep(1.0)
    cap = mcp.json("perf_capture", frames=frames, trace=False, label=f"m3exit_{label}")
    mcp.call("time_control", action="pause")
    users = gpu_users.stop()
    errors = cap.get("budget_errors") or []
    required = [e for e in errors if is_frame_metric(e.get("metric", ""))]
    notes = [e for e in errors if not is_frame_metric(e.get("metric", ""))]
    result = {
        "label": label, "wave": st["wave"], "enemies": st["enemies"],
        "frames": cap.get("frames"), "adapter_kind": cap.get("adapter"),
        "frame_ms": cap.get("frame_ms"), "gpu_ms": cap.get("gpu_ms"),
        "gpu_passes_ms": cap.get("gpu_passes_ms"), "draw_calls": cap.get("draw_calls"),
        "triangles": cap.get("triangles"), "budgets_checked_on": cap.get("budgets_checked_on"),
        "budget_misses": required, "notes": notes,
        "other_engines_running": other_engines(eng.proc.pid), "compilers_running": compilers_running(),
        "desktop_apps": desktop_apps(), "gpu_3d_percent_mean_max": users,
    }
    gpu, fr = cap.get("gpu_ms") or {}, cap.get("frame_ms") or {}
    scene = (cap.get("gpu_passes_ms") or {}).get("scene") or {}
    print(f"  {label} (wave {st['wave']}, {st['enemies']} enemies): {cap.get('frames')} frames, "
          f"frame p95 {fr.get('p95')} ms, GPU mean {gpu.get('mean')} p95 {gpu.get('p95')} ms, "
          f"scene mean {scene.get('mean')} p95 {scene.get('p95')} ms; "
          f"other engines {result['other_engines_running']}, compilers {result['compilers_running']}")
    print(f"    desktop apps: {result['desktop_apps']}; GPU 3D % (mean/max) on the engine's GPU: {users}")
    for e in required:
        print(f"    budget miss: {e}")
    for e in notes:
        print(f"    note (per-pass or other CPU budget; doesn't decide the criterion): {e}")
    result["passed"] = bool(cap.get("frames", 0) >= frames // 2) and not required
    return result


def grass_entities(mcp):
    """Every grass patch in the world (the slab's and the roots'), by path."""
    paths, cursor = [], None
    for _ in range(50):
        args = {"fields": ["path", "recipe"], "limit": 200}
        if cursor:
            args["cursor"] = cursor
        page = mcp.json("world_query", **args)
        paths += [i["path"] for i in page.get("items", []) if "grass_patch" in str(i.get("recipe"))]
        cursor = page.get("next_cursor")
        if not cursor:
            break
    return paths


def measure_grass(mcp, label, frames):
    """The grass's own GPU cost in this game state: the clock paused (the same frame drawn), the
    window's frames with every grass patch shown and hidden, ABAB, perf_capture each."""
    mcp.call("time_control", action="pause")
    paths = grass_entities(mcp)
    runs = {"on": [], "off": []}
    for rnd in range(2):
        for shown in (True, False):
            for p in paths:
                mcp.call("world_set", path=p, visible=shown)
            time.sleep(0.5)
            cap = mcp.json("perf_capture", frames=frames, trace=False, label=f"m3grass_{'on' if shown else 'off'}{rnd}")
            passes = cap.get("gpu_passes_ms") or {}
            runs["on" if shown else "off"].append({
                "gpu_mean": (cap.get("gpu_ms") or {}).get("mean"), "gpu_p95": (cap.get("gpu_ms") or {}).get("p95"),
                "scene_mean": (passes.get("scene") or {}).get("mean"), "shadow_mean": (passes.get("shadow") or {}).get("mean"),
                "triangles": (cap.get("triangles") or {}).get("mean")})
    for p in paths:
        mcp.call("world_set", path=p, visible=True)

    def avg(key, side):
        vals = [r[key] for r in runs[side] if isinstance(r.get(key), (int, float))]
        return round(sum(vals) / len(vals), 3) if vals else None
    out = {"label": label, "patches": len(paths), "runs": runs}
    for k in ("gpu_mean", "gpu_p95", "scene_mean", "shadow_mean", "triangles"):
        on, off = avg(k, "on"), avg(k, "off")
        out[k] = {"on": on, "off": off, "delta": round(on - off, 3) if on is not None and off is not None else None}
    print(f"  {label}: the grass ({len(paths)} patches), clock paused, ABAB: " + ", ".join(
        f"{k} +{out[k]['delta']}" for k in ("gpu_mean", "gpu_p95", "scene_mean", "shadow_mean", "triangles")))
    return out


GI_CYCLE = ("off", "low", "high", "off", "low", "high")


def bake_top(mcp):
    """Bakes the slab's top face (and a few metres above it) as GI volume "top"; returns the bake's
    wall seconds and how long the GI pipelines took after it (the pipeline cache under --cache
    makes that near zero from the second run on)."""
    q = mcp.json("world_query", path_prefix="/slab/graveyard/ground", fields=["position", "scale"], limit=4)
    items = q.get("entities") or q.get("items") or []
    ground = items[0] if items else {"position": [0, 0, 0], "scale": [8, 0.2, 8]}
    top = ground["position"][1] + ground["scale"][1] / 2
    start = time.time()
    mcp.json("gi_bake", volume="top", min=[-22, top - 1.0, -22], max=[22, top + 7.0, 22], spacing=1.0,
             rays=256, bounces=2, sky_below=0.6)
    while mcp.json("gi_info")["job"] is not None:
        if time.time() - start > 1200:
            raise RuntimeError("the GI bake did not finish")
        time.sleep(0.25)
    baked = time.time() - start
    waited = wait_gi_pipelines(mcp)
    return {"bake_s": round(baked, 2), "pipelines_after_bake_s": waited}


def wait_gi_pipelines(mcp, timeout=900):
    """Seconds until the active tier's GI pipelines exist (a tier switch starts building its set
    on a worker; measuring before they exist measures the flat ambient)."""
    start = time.time()
    while not mcp.json("gi_info")["runtime"]["pipelines_ready"]:
        if time.time() - start > timeout:
            raise RuntimeError("the GI pipelines never became ready")
        time.sleep(0.25)
    return round(time.time() - start, 2)


def measure_gi(mcp, eng, label, frames, st):
    """Rebakes the top face, then GI off, low and high back to back, twice each (alternated)."""
    times = bake_top(mcp)
    print(f"  GI bake {times}")
    out = []
    for i, q in enumerate(GI_CYCLE):
        mcp.json("gi_settings", quality=q)
        waited = wait_gi_pipelines(mcp) if q != "off" else 0.0
        r = measure(mcp, eng, f"{label}_gi_{q}_{i // 3 + 1}", frames, st)
        r["gi"], r["gi_bake"], r["gi_pipelines_wait_s"] = q, times, waited
        out.append(r)
    mcp.json("gi_settings", quality="low")
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7488)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--seed", default="7")
    ap.add_argument("--frames", type=int, default=600, help="window frames per capture (default 600)")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=None)
    ap.add_argument("--max-fps", type=int, default=None, help="the window's frame cap (the engine's --max-fps)")
    ap.add_argument("--grass-ab", action="store_true", help="at each measured wave, also measure the grass's own GPU "
                    "cost: the clock paused, every grass patch shown and hidden, ABAB (reported, not a criterion)")
    ap.add_argument("--gi", action="store_true", help="bake GI and alternate its tiers off/low/high per measured wave")
    ap.add_argument("--water", default=None, help="a water preset (murk, ...): an agent spawns a pool round the slab's "
                    "foot (the `water` component; cosmetic, never simulation) to measure the water pass in play")
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("use another port than 7471 (reserved for your working engine)")
    exe = Path(args.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(args.out) if args.out else ROOT / "target" / "m3-exit" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    cap = ["--max-fps", str(args.max_fps)] if args.max_fps else []
    extra = [*cap, "--adapter", args.adapter, "--size", "1280x720", "--perf", str(PERF_TOML),
             "--captures", str(out / "captures"), "--lead", "m3-exit", "--", "--seed", str(args.seed), "--mute", "--rules", "lex", "--title", "off"]
    summary = {"adapter": args.adapter, "seed": args.seed, "rules": "lex", "frames_per_capture": args.frames,
               "max_fps": args.max_fps, "gi": args.gi}
    measured = []
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("m3-exit")

        def call_raw(tool, **a):
            return mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, a)})
        mcp.call_raw = call_raw
        summary["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        print(f"GPU: {summary['gpu']}")
        run = Run(mcp, out)
        if args.gi:
            # The game loads its models first: the engine may answer "busy" for a while.
            for attempt in range(120):
                r = mcp.request("tools/call", {"name": "agent_hello", "arguments": {"name": "m3-exit", "role": "lead"}})
                if not r.get("isError"):
                    break
                time.sleep(1.0)
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=args.seed)
        if args.water:
            # A still pool under the slab's top, showing in its broken corners and round its foot.
            mcp.json("world_spawn", shape="group", path="/pool", position=[0, -1.6, 0], water={"preset": args.water, "size": [70, 70]})
            summary["water"] = args.water
        busy = boss = False
        while True:
            st = run.state()
            if st["outcome"] != "running":
                break
            run.drive()
            st = run.state()
            if not busy and st["wave"] >= 6 and st["enemies"] >= 18:
                if args.gi:
                    measured.extend(measure_gi(mcp, eng, "busy_wave", args.frames, st))
                else:
                    measured.append(measure(mcp, eng, "busy_wave", args.frames, st))
                if args.grass_ab:
                    summary.setdefault("grass_ab", []).append(measure_grass(mcp, "busy_wave", args.frames // 2))
                run.capture("busy_wave", eye=[38, 27, 52], target=[0, -9, 1])
                busy = True
                continue
            if not boss and st["wave"] == 10 and st["enemies"] >= 1:
                if args.gi:
                    measured.extend(measure_gi(mcp, eng, "boss_wave", args.frames, st))
                else:
                    measured.append(measure(mcp, eng, "boss_wave", args.frames, st))
                if args.grass_ab:
                    summary.setdefault("grass_ab", []).append(measure_grass(mcp, "boss_wave", args.frames // 2))
                run.capture("boss_wave", eye=[38, 27, 52], target=[0, -9, 1])
                boss = True
                continue
            run.step(30)
        st = run.state()
        summary["end"] = {k: st[k] for k in ("outcome", "tick", "wave", "tombstones", "state_hash")}
        rec = mcp.json("play_record", path="recordings/m3-exit.json", overwrite=True)
        rep = mcp.json("play_replay", path="recordings/m3-exit.json")
        lex = st.get("rules") or {}
        summary["replay"] = {"status": rep.get("status"), "lex_parity": lex.get("parity"),
                             "lex_hash_recorded": (rec.get("lex") or {}).get("end_hash"),
                             "lex_hash_replayed": (rep.get("lex") or {}).get("end_hash")}
        summary["captures"] = run.captures
        print(f"end: {st['outcome']} at wave {st['wave']}; replay {rep.get('status')}, Lex parity {lex.get('parity')}")
    errors = eng.validation_errors()
    summary["validation_errors"] = errors
    summary["measured"] = measured
    replay_ok = (summary["replay"]["status"] == "verified" and summary["replay"]["lex_parity"] == "ok"
                 and summary["replay"]["lex_hash_recorded"] == summary["replay"]["lex_hash_replayed"])
    # With --gi the budgets decide on the default tiers (off, and low: the iGPU's); high is reported.
    deciding = [m for m in measured if m.get("gi", "off") in ("off", "low")]
    ok = len(deciding) == (8 if args.gi else 2) and all(m["passed"] for m in deciding) and errors == 0 and replay_ok
    summary["passed"] = ok
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"validation errors at shutdown: {errors}; replay verified: {replay_ok}")
    print(("PASS" if ok else "FAIL") + f": summary in {out / 'summary.json'}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

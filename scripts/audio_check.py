#!/usr/bin/env python3
"""Live check of audio (docs/design/AUDIO.md): Vorbis fixtures, Sepulcra's music and effects as
the engine hears them, loudness targets, starvation and clipping, and the GPU cost.

1. The decoder: `cargo test --release -p specula-audio --test vorbis --test streaming
   --test sepulcra_music` (six ffmpeg-made fixtures within 1e-4 of ffmpeg's own decode, sample-
   exact seeking, fuzzing, loops, ducking, crossfades, map 1's music at its target).
2. Sepulcra windowed (default: the integrated GPU) on a copy of its assets, with the real audio
   device. The fixtures are uploaded as project files (project_upload) and audio_preview decodes
   them in the engine: length and peak must match ffmpeg's reference decode. audio_preview of
   map1.json reports its loop and loudness.
3. A run with the centre build order at speed: audio_played must show map 1's theme streaming
   on the music bus and the game's effects; audio_levels must show the music bed near its
   target (-29 LUFS, a little lower when it ducks), the effects' loud moments 3 LU or more
   above it, no starvation, no clipped samples and no overs on the master output.
4. A run with no defences until it is lost: the defeat sting plays on the voice bus, the music
   ducks under it and crossfades into the defeat cue.
5. The same centre run with `--audio off`: GPU p95 (median of three captures) with audio may
   not be worse than without, past noise (the mixer runs on its own threads, drawing nothing).

    cargo build --release -p sepulcra-app
    python scripts/audio_check.py                         # integrated GPU, ports 7500-7501
    python scripts/audio_check.py --adapter discrete --port 7502

Output (engine logs, previews, summary.json) in target/audio-check/<adapter>/. Fails loudly.
Standard library only; keep output ASCII.
"""
import argparse
import base64
import json
import os
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
EXE = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
FIXTURES = ROOT / "crates" / "specula-audio" / "tests" / "data" / "vorbis"
ASSETS = ROOT / "games" / "sepulcra" / "assets"
AGENT = "audio-checker"

failures = []


def check(ok, what):
    print(("  ok    " if ok else "  FAIL  ") + what)
    if not ok:
        failures.append(what)
    return ok


def ref_wav(path):
    """(rate, channels, frames, peak) of a 16-bit PCM WAV (the fixtures' references)."""
    b = path.read_bytes()
    i, rate, ch, data = 12, None, None, None
    while i + 8 <= len(b):
        cid, size = b[i:i + 4], struct.unpack("<I", b[i + 4:i + 8])[0]
        if cid == b"fmt ":
            ch, rate = struct.unpack("<HI", b[i + 10:i + 16])
        elif cid == b"data":
            data = b[i + 8:i + 8 + size]
        i += 8 + size + (size & 1)
    samples = struct.unpack(f"<{len(data) // 2}h", data)
    return rate, ch, len(samples) // ch, max(abs(s) for s in samples) / 32768.0


def db(x):
    import math
    return 20 * math.log10(x) if x > 0 else float("-inf")


def upload(mcp, path, data):
    out = mcp.json("project_upload", path=path, data=base64.b64encode(data).decode())
    return out


def run_engine(adapter, port, out, assets, audio, label):
    args = ["--adapter", adapter, "--assets", str(assets), "--captures", str(out / f"captures-{label}"), "--lead", AGENT, "--max-fps", "60"]
    if audio:
        args += ["--audio", audio]
    return Engine(EXE, port, out / f"engine-{label}.log", *args, "--", "--seed", "7", "--title", "off", cwd=str(out))


def wait_for(fn, timeout, step=0.25):
    end = time.time() + timeout
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(step)
    return None


def centre_run(mcp, seconds, sample=False):
    """Starts a run with the centre build order and lets it play at speed 2 for `seconds`; with
    `sample`, reads the music and effects buses' momentary loudness every half second."""
    mcp.call("play_new", seed="7", layout="centre")
    mcp.call("time_control", action="resume")
    mcp.call("time_control", action="scale", scale=2)
    samples = []
    end = time.time() + seconds
    while time.time() < end:
        if sample:
            b = mcp.json("audio_levels", window_s=0.4, buses=["music", "sfx"])["buses"]
            samples.append((b["music"]["momentary_lufs"], b["sfx"]["momentary_lufs"]))
        time.sleep(0.5)
    return samples


def percentile(values, q):
    v = sorted(values)
    return v[min(len(v) - 1, int(q * len(v)))] if v else None


def measure_gpu(mcp, label, runs=3):
    """GPU ms over `runs` captures of 240 frames, a few seconds apart: the median of their p95s
    (the iGPU is shared with other engines and desktop apps; one capture is noise)."""
    p95s, means = [], []
    for i in range(runs):
        cap = mcp.json("perf_capture", frames=240, trace=False, label=f"audio_{label}_{i}")
        gpu = cap.get("gpu_ms") or {}
        if gpu.get("p95") is not None:
            p95s.append(gpu["p95"])
            means.append(gpu.get("mean"))
        time.sleep(2.0)
    return {"p95": percentile(p95s, 0.5), "p95s": p95s, "means": means}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7500)
    ap.add_argument("--seconds", type=float, default=45.0, help="real seconds the centre run plays before measuring")
    ap.add_argument("--skip-tests", action="store_true", help="skip step 1 (cargo test)")
    a = ap.parse_args()
    out = TARGET / "audio-check" / a.adapter
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    summary = {"adapter": a.adapter}

    print("1. decoder fixtures, streaming and music (cargo test --release)")
    if not a.skip_tests:
        r = subprocess.run(["cargo", "test", "--release", "-j", "4", "-p", "specula-audio", "--test", "vorbis", "--test", "streaming", "--test", "sepulcra_music"],
                           cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
        (out / "cargo-test.log").write_text(r.stdout + r.stderr, encoding="utf-8")
        passed = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("test result:")]
        check(r.returncode == 0 and passed and all(" 0 failed" in l for l in passed), f"cargo test: {'; '.join(passed) or 'no result'}")

    if not EXE.exists():
        check(False, f"{EXE} missing: cargo build --release -p sepulcra-app")
        return finish(out, summary)
    assets = out / "assets"
    shutil.copytree(ASSETS, assets)

    print("2-4. Sepulcra windowed with audio")
    with run_engine(a.adapter, a.port, out, assets, None, "audio") as eng:
        mcp = eng.mcp("audio_check")
        mcp.hello(AGENT, role="lead")
        settings = mcp.json("audio_settings")
        check(settings.get("enabled") is True, "the engine's audio is on (windowed default)")
        ducks = settings.get("ducks", [])
        check(len(ducks) == 3 and all(d.get("set_by") == "sepulcra" for d in ducks), f"Sepulcra set its duck rules: {[(d['source'], d['target'], d['depth_db']) for d in ducks]}")

        # Fixtures through the engine: upload, then decode and measure with audio_preview.
        previews = {}
        for name in ["sweep_mono_22k", "stereo_48k", "surround_6ch", "loud_32k_q10"]:
            path = f"assets/music/fixture_{name}.ogg"
            up = upload(mcp, path, (FIXTURES / f"{name}.ogg").read_bytes())
            p = mcp.json("audio_preview", path=path)
            rate, ch, frames, peak = ref_wav(FIXTURES / f"{name}.ref.wav")
            previews[name] = {"upload": up.get("info"), "preview": p}
            want_s = frames / rate
            got_s = p["seconds"]
            # ffmpeg ends some streams up to a block early (we keep exactly the last granule).
            check(abs(got_s - want_s) <= 2048 / rate + 1e-3 and p.get("bad_packets") == 0,
                  f"{name}: decoded {got_s:.3f} s in the engine (ffmpeg {want_s:.3f} s), no bad packets")
            if ch <= 2:
                # The reference is 16-bit: ffmpeg clips what goes past full scale; we don't.
                ours = min(0.0, p["metrics"]["peak_dbfs"])
                check(abs(ours - min(0.0, db(peak))) < 0.2,
                      f"{name}: peak {p['metrics']['peak_dbfs']} dBFS (ffmpeg's 16-bit decode {db(peak):.2f}, clipped at 0)")
        map1 = mcp.json("audio_preview", path="assets/music/map1.json", seconds=60, png=True)
        previews["map1"] = map1
        check(abs(map1["lap_s"] - 58.2) < 0.5 and abs(map1["loop_lufs"] - -29.0) < 0.3, f"map1: lap {map1['lap_s']} s, loop {map1['loop_lufs']} LUFS (target -29)")
        for cue in ["victory", "defeat"]:
            c = mcp.json("audio_preview", path=f"assets/music/{cue}.json", png=True)
            previews[cue] = c
            check(c["metrics"]["clipped"] == 0 and c["metrics"]["lufs"] is not None, f"{cue} cue: {c['seconds']} s, {c['metrics']['lufs']} LUFS, picture {c['files']['png']}")
        summary["previews"] = previews

        # 3. A centre run: music and effects as heard.
        mcp.call("play_new", seed="7", layout="centre")
        theme = wait_for(lambda: [p for p in mcp.json("audio_played", seconds=60, bus="music", response_format="detailed")["plays"] if p["name"] == "map1"], 60)
        check(bool(theme) and theme[0]["by"] == "sepulcra" and theme[0]["kind"] == "stream", f"map 1's theme streams on the music bus: {theme[:1]}")
        moments = centre_run(mcp, a.seconds, sample=True)
        played = mcp.json("audio_played", seconds=a.seconds, limit=200)
        sfx = {k: v for k, v in played["by_name"].items() if k not in ("map1",)}
        check(sum(sfx.values()) >= 20, f"the game's effects played: {played['matched']} plays, {sfx}")
        lv = mcp.json("audio_levels", window_s=min(30.0, a.seconds), spectrum=True, response_format="detailed")
        summary["levels_centre"] = lv
        music, effects, master = lv["buses"]["music"], lv["buses"]["sfx"], lv["buses"]["master_out"]
        mi, si = music["integrated_lufs"], effects["integrated_lufs"]
        check(mi is not None and -34.0 <= mi <= -27.0, f"music bus at {mi} LUFS integrated (piece target -29; ducking lowers it)")
        # The mix target: the music is a bed under the game. Effects come and go, so their
        # integrated loudness says little; their loud moments (momentary, 90th percentile of
        # half-second samples while they play) must stand above the music bed.
        sfx_moments = [s for m, s in moments if s is not None]
        music_moments = [m for m, s in moments if m is not None]
        loud = percentile(sfx_moments, 0.9)
        bed = percentile(music_moments, 0.5)
        summary["moments"] = {"sfx_p90": loud, "music_p50": bed, "samples": len(moments), "sfx_integrated": si}
        check(loud is not None and bed is not None and loud - bed >= 3.0, f"effects stand above the music: sfx momentary p90 {loud} vs music p50 {bed} LUFS (sfx integrated {si})")
        check(master["overs"] == 0 and lv["totals"]["clipped_samples"] == 0, f"no clipping: master overs {master['overs']}, clipped samples {lv['totals']['clipped_samples']}, limiter blocks {lv['totals']['limited_blocks']}")
        check(lv["totals"]["stream_starved_frames"] == 0 and all(s["starved_frames"] == 0 for s in lv["streams"]), f"no starvation: {[(s['name'], s['state'], s['starved_frames']) for s in lv['streams']]}")
        check(master["true_peak_dbtp"] is not None and master["true_peak_dbtp"] <= 0.5, f"master true peak {master['true_peak_dbtp']} dBTP")
        dev = lv["device"]
        summary["audio_cpu"] = {"mix_cpu_percent": dev.get("mix_cpu_percent"), "worker_cpu_percent": dev.get("worker_cpu_percent"), "mix_us_mean": dev.get("mix_us_mean")}
        cpu = (dev.get("mix_cpu_percent") or 0) + (dev.get("worker_cpu_percent") or 0)
        check(cpu < 5.0, f"audio's CPU: mixer {dev.get('mix_cpu_percent')} % and worker {dev.get('worker_cpu_percent')} % of one core (under 5 %)")
        gpu_on = measure_gpu(mcp, "on")
        summary["gpu_audio_on"] = gpu_on
        print(f"  GPU with audio: p95 median {gpu_on['p95']} ms (p95s {gpu_on['p95s']}, means {gpu_on['means']})")
        st = mcp.json("play_state")
        summary["state_centre"] = {"wave": st.get("wave"), "audio": st.get("audio")}

        # 4. Lose a run: the defeat sting on the voice bus, the music ducks and crossfades.
        mcp.call("play_new", seed="7", layout="none")
        mcp.call("time_control", action="resume")
        mcp.call("time_control", action="scale", scale=16)
        before = None
        lost = None
        end = time.time() + 240
        while time.time() < end:
            st = mcp.json("play_state")
            if st.get("outcome") not in (None, "playing", "running"):
                lost = st.get("outcome")
                break
            m = mcp.json("audio_levels", window_s=0.4, buses=["music"])["buses"]["music"]["momentary_lufs"]
            before = m if m is not None else before
            time.sleep(0.3)
        check(lost is not None, f"the undefended run ended: {lost}")
        samples = []
        for _ in range(12):
            lvl = mcp.json("audio_levels", window_s=0.4, buses=["music", "voice"])["buses"]
            samples.append((lvl["music"]["momentary_lufs"], lvl["voice"]["momentary_lufs"]))
            time.sleep(0.25)
        mcp.call("time_control", action="scale", scale=1)
        time.sleep(3.0)
        played = mcp.json("audio_played", seconds=15, limit=100, response_format="detailed")
        names = [(p["name"], p["bus"], p["kind"]) for p in played["plays"]]
        check(("defeat", "voice", "sound") in names, f"the defeat sting played on the voice bus: {names[:8]}")
        check(("defeat", "music", "stream") in names, "the music crossfaded into the defeat cue")
        lows = [m for m, v in samples if m is not None and v is not None]
        check(before is not None and lows and before - min(lows) >= 6.0, f"the music ducked under the sting: {before} LUFS before, {min(lows) if lows else None} under it")
        summary["defeat"] = {"before": before, "during": samples, "played": names[:20]}
        lv = mcp.json("audio_levels", window_s=60, response_format="detailed")
        check(lv["totals"]["stream_starved_frames"] == 0 and lv["totals"]["clipped_samples"] == 0, f"still no starvation or clipping: {lv['totals']}")
        mcp.close()
    check(eng.exit_code == 0, f"engine exited cleanly ({eng.exit_code})")

    print("5. the same run with --audio off (GPU cost)")
    with run_engine(a.adapter, a.port + 1, out, assets, "off", "off") as eng:
        mcp = eng.mcp("audio_check_off")
        mcp.hello(AGENT, role="lead")
        check(mcp.json("audio_settings").get("enabled") is False, "--audio off: no audio")
        centre_run(mcp, a.seconds)
        gpu_off = measure_gpu(mcp, "off")
        summary["gpu_audio_off"] = gpu_off
        print(f"  GPU without audio: p95 median {gpu_off['p95']} ms (p95s {gpu_off['p95s']}, means {gpu_off['means']})")
        mcp.close()
    on, off = gpu_on["p95"], gpu_off["p95"]
    # Audio mixes on its own threads and draws nothing: with it the GPU may not be slower than
    # without it, past noise (1.5 ms or 20% on a shared iGPU). Faster is noise too, not a fault.
    check(on is not None and off is not None and on <= off + max(1.5, 0.2 * off), f"GPU p95 with audio {on} ms vs without {off} ms (no worse, within 1.5 ms or 20%)")
    return finish(out, summary)


def finish(out, summary):
    summary["failures"] = failures
    (out / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"summary: {out / 'summary.json'}")
    if failures:
        print(f"FAILED: {len(failures)} check(s)")
        sys.exit(1)
    print("audio check passed")


if __name__ == "__main__":
    main()

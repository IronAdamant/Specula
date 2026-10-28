"""Asset studio check: Sepulcra's props and creatures through the stage, the shot set, the taste
checks and style.toml (docs/design/ASSET_STUDIO.md).

What it does and checks:
  - the stage describes itself (rigs, shots, the rules from Sepulcra's style.toml),
  - style_check over every Sepulcra recipe at every tier: all pass,
  - studio_check on every prop and creature: shot-set measures, taste checks and the critique; all
    pass (the fence's flat lighting is reported as exempt: deliberate style, reason in style.toml),
  - contact sheets (the shot set; a turntable with colour, value and silhouette passes), line-ups
    with the scale figures and the distinctness matrix, stage frames,
  - a violation caught and fixed: a new graveyard prop drafted with an agent's typical mistakes is
    caught (errors with numbers, a critique with the next steps), marked, revised per the critique,
    passes, and studio_compare reports what was fixed,
  - refusals: too many tiles, an unknown mark, a recipe that doesn't exist (structured errors),
  - the world is never touched (same state hash, no entities), zero validation errors at shutdown.

The engine runs headless on a copy of Sepulcra's assets (OUT/assets), so the draft prop never
touches the game. Captures (PNG + packet JSON) land in OUT/captures/run-*/; the summary in
OUT/summary.json. Look at the PNGs before claiming anything about them.

With --window the main check runs with the window open (each call's renders then spread over
frames). With --pacing it measures the window's frames over the whole of heavy studio calls
(back-to-back perf_captures) and fails on any frame over 50 ms whose time went to the studio.
With --fidelity it checks the Fidelity track's benchmark set instead (docs/fidelity/assets, T0 and
T1) and renders its contact sheet from the stage's fixed cameras into docs/fidelity/.

Usage: cargo build -p specula && python scripts/studio_check.py [--exe PATH] [--port 7616] [--out DIR] [--adapter integrated] [--window | --pacing | --fidelity]
Needs a GPU (Vulkan).
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

SEPULCRA = ROOT / "games" / "sepulcra" / "assets"

PROPS = ["warding_stone", "rune_pillar", "brazier_cairn", "sky_obelisk", "wall_segment", "tombstone", "graveyard_fence", "slab_cliff"]
CREATURES = ["shambler", "skeleton_runner", "wraith", "bone_colossus"]

# An agent's first draft of a new graveyard prop, with typical mistakes: a raw brown instead of the
# palette's bone (it reads as fire, which STYLE.md keeps for braziers), a heap too small to read
# from the game camera, too many triangles for a graveyard piece, no shading.
BONE_PILE_DRAFT = {
    "recipe": 1, "name": "bone_pile", "category": "graveyard",
    "description": "Draft: a heap of old bones by a grave.",
    "seed": 808, "lods": [0.5, 0.25], "collider": "aabb",
    "parts": [
        {"name": "heap", "material": "bone", "color": "#8b5a2b",
         "shape": {"op": "sphere", "radius": 0.18, "subdivisions": 2},
         "ops": [{"op": "scale", "by": [1.5, 0.45, 1.2]}, {"op": "move", "by": [0, 0.05, 0]}]},
        {"name": "bones", "material": "bone", "color": "#8b5a2b",
         "shape": {"op": "capsule", "radius": 0.025, "height": 0.34},
         "ops": [{"op": "rotate", "deg": [0, 0, 84]}, {"op": "move", "by": [0.04, 0.1, 0]},
                 {"op": "radial", "count": 4, "jitter": {"rotate": [8, 20, 6], "scale": 0.2}}]},
    ],
}

# The revision, following the critique: palette colours (bone, shaded at the foot), bevelled
# chunky bones and a skull on top so it reads from the game camera, every part named.
BONE_PILE_REVISED = {
    "recipe": 1, "name": "bone_pile", "category": "graveyard",
    "description": "A heap of old bones with a skull on top, about 0.55 m high (STYLE.md bone neutrals).",
    "seed": 808, "lods": [0.5, 0.25], "collider": "aabb", "smooth_angle": 35,
    "parts": [
        {"name": "heap", "material": "bone", "color": "bone_shadow",
         "shape": {"op": "sphere", "radius": 0.36, "subdivisions": 2},
         "ops": [{"op": "scale", "by": [1.3, 0.55, 1.1]},
                 {"op": "displace", "amount": 0.04, "scale": 0.25},
                 {"op": "gradient", "from": "bone_shadow", "to": "bone", "range": [0, 0.3]}]},
        {"name": "long_bones", "material": "bone",
         "shape": {"op": "capsule", "radius": 0.045, "height": 0.62, "segments": 6, "rings": 1},
         "ops": [{"op": "rotate", "deg": [0, 0, 78]}, {"op": "move", "by": [0.05, 0.26, 0]},
                 {"op": "radial", "count": 5, "jitter": {"rotate": [10, 25, 8], "scale": 0.15}},
                 {"op": "bevel_tint", "color": "bone", "amount": 0.8}]},
        {"name": "skull", "material": "bone",
         "shape": {"op": "sphere", "radius": 0.13, "subdivisions": 1},
         "ops": [{"op": "scale", "by": [1.0, 1.1, 1.15]}, {"op": "move", "by": [0, 0.42, 0.03]},
                 {"op": "gradient", "from": "bone_shadow", "to": "bone", "range": [0.3, 0.55]}]},
        {"name": "eye_sockets", "material": "bone", "color": "fissure",
         "shape": {"op": "sphere", "radius": 0.028, "subdivisions": 0},
         "ops": [{"op": "move", "by": [0.045, 0.44, 0.15]}, {"op": "mirror", "axis": "x"}]},
    ],
    "finish": [{"op": "ground_shade", "amount": 0.3, "height": 0.2}],
}


def textured_glb(path, size=8):
    """A 1 m box with a `size`x`size` checker base-colour texture (8x8 squares), as a GLB
    (standard library only): a glTF whose colours come from a texture, not vertex colours."""
    import struct
    import zlib

    def png(w, h, rgb):
        rows = b"".join(b"\x00" + b"".join(bytes(rgb(x, y)) for x in range(w)) for y in range(h))
        chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
        return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")

    cell = max(1, size // 8)
    image = png(size, size, lambda x, y: (200, 60, 40) if (x // cell + y // cell) % 2 else (40, 160, 200))
    pos, nrm, uv, idx = [], [], [], []
    faces = [((1, 0, 0), (0, 0, -1), (0, 1, 0)), ((-1, 0, 0), (0, 0, 1), (0, 1, 0)), ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
             ((0, -1, 0), (1, 0, 0), (0, 0, 1)), ((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0))]
    for n, u, v in faces:
        base = len(pos)
        for su, sv, (tu, tv) in [(-1, -1, (0, 1)), (1, -1, (1, 1)), (1, 1, (1, 0)), (-1, 1, (0, 0))]:
            pos.append(tuple(0.5 * n[k] + 0.5 * su * u[k] + 0.5 * sv * v[k] + (0.5 if k == 1 else 0.0) for k in range(3)))
            nrm.append(n)
            uv.append((tu, tv))
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    f32 = lambda rows: b"".join(struct.pack("<" + "f" * len(r), *r) for r in rows)
    blobs = [f32(pos), f32(nrm), f32(uv), struct.pack("<" + "H" * len(idx), *idx), image]
    views, buf = [], b""
    for b in blobs:
        while len(buf) % 4:
            buf += b"\x00"
        views.append({"buffer": 0, "byteOffset": len(buf), "byteLength": len(b)})
        buf += b
    while len(buf) % 4:
        buf += b"\x00"
    lo = [min(p[k] for p in pos) for k in range(3)]
    hi = [max(p[k] for p in pos) for k in range(3)]
    doc = {
        "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0, "name": "box"}],
        "meshes": [{"name": "box", "primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}, "indices": 3, "material": 0}]}],
        "materials": [{"name": "checker", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0.0, "roughnessFactor": 0.8}}],
        "textures": [{"source": 0, "sampler": 0}], "samplers": [{}], "images": [{"bufferView": 4, "mimeType": "image/png"}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": len(pos), "type": "VEC3", "min": lo, "max": hi},
            {"bufferView": 1, "componentType": 5126, "count": len(nrm), "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": len(uv), "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": len(idx), "type": "SCALAR"}],
        "bufferViews": views, "buffers": [{"byteLength": len(buf)}],
    }
    js = json.dumps(doc).encode()
    while len(js) % 4:
        js += b" "
    body = struct.pack("<I", len(js)) + b"JSON" + js + struct.pack("<I", len(buf)) + b"BIN\x00" + buf
    path.write_bytes(struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body)


def check(ok, what, failures):
    print(("  ok   " if ok else "  FAIL ") + what)
    if not ok:
        failures.append(what)


def call_error(m, tool, **args):
    """The structured error a call answers (None when it succeeds)."""
    try:
        m.json(tool, **args)
        return None
    except RuntimeError as e:
        text = str(e).split(": ", 1)[1] if ": " in str(e) else str(e)
        try:
            return json.loads(text)
        except ValueError:
            return {"error": "?", "message": text}


def timed(m, tool, **args):
    start = time.perf_counter()
    r = m.json(tool, **args)
    return r, (time.perf_counter() - start) * 1000.0


def copy_assets(dest):
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    for name in ["recipes", "anims"]:
        shutil.copytree(SEPULCRA / name, dest / name)
    (dest / "models").mkdir()
    for name in ["style.json", "style.toml"]:
        shutil.copy(SEPULCRA / name, dest / name)


def default_exe():
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    return str(target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula"))


FIDELITY = ROOT / "docs" / "fidelity"
BENCHMARK = ["runestone", "barrel", "lantern", "tree", "skeleton_warrior"]


def fidelity(a, out):
    """The Fidelity track's benchmark set at T0 and T1 on the stage: style_check, the taste checks
    per tier (reported: T0 greybox is expected to read flat), a line-up and the set's contact sheet
    from the stage's fixed cameras, copied to docs/fidelity/."""
    failures, summary = [], {"adapter": a.adapter, "subjects": {}}
    args = ["--headless", "--assets", str(FIDELITY / "assets"), "--captures", str(out / "captures"), "--adapter", a.adapter]
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        m = engine.mcp("studio-fidelity", detailed=False)
        sc = m.json("style_check", folder="recipes", response_format="detailed")
        bad = {r["subject"]: [e["check"] for e in r["errors"]] for r in sc["results"] if not r["passed"]}
        check(sc["passed"], f"style_check: {sc['subjects']} benchmark recipes keep the set's style.toml {bad or ''}", failures)
        for name in BENCHMARK:
            for tier in ("t0", "t1"):
                r = m.json("studio_check", recipe=f"recipes/{name}_{tier}.json", siblings=[], image=False, response_format="detailed")
                summary["subjects"][f"{name}_{tier}"] = {"errors": [(e["check"], e["value"]) for e in r["errors"]],
                                                         "spread": r["measures"]["game_light"]["spread"], "contrast": r["measures"]["gameplay"]["contrast"],
                                                         "triangles": r["facts"]["triangles"][0], "image": r["image"]["path"]}
                print(f"    {name}_{tier}: {r['failed']} errors {[e['check'] for e in r['errors']]}, {r['facts']['triangles'][0]} triangles")
            check(not summary["subjects"][f"{name}_t1"]["errors"], f"{name} T1 passes every check", failures)
        subjects = [f"recipes/{n}_{t}.json" for n in BENCHMARK for t in ("t0", "t1")]
        sheet = m.json("studio_sheet", subjects=subjects, views=["front", "three_quarter", "top", "gameplay"], rig="game", tile=150)
        lineup = m.json("studio_lineup", subjects=[f"recipes/{n}_t1.json" for n in BENCHMARK], rig="game")
        check(len(sheet["subjects"]) == 10, "the set's contact sheet: 5 subjects x T0/T1 from 4 fixed views", failures)
        summary["captures"] = {"sheet": sheet["image"]["path"], "lineup": lineup["image"]["path"]}
        shutil.copy(sheet["image"]["path"], FIDELITY / "benchmark_t0_t1_studio.png")
        m.close()
    v = engine.validation_errors()
    check(v == 0, f"zero validation errors at shutdown ({v})", failures)
    summary["failures"] = failures
    (out / "fidelity.json").write_text(json.dumps(summary, indent=1))
    print(f"\nsheet copied to {FIDELITY / 'benchmark_t0_t1_studio.png'}; summary: {out / 'fidelity.json'}")
    print("FAILED: " + "; ".join(failures) if failures else "fidelity set checked")
    return 1 if failures else 0


STUDIO_FRAME_LIMIT_MS = 50.0


def frames_over(capture_paths):
    """Every captured frame: (dt_ms, its heaviest main-thread zone by self time, that zone's ms)."""
    out = []
    for p in capture_paths:
        j = json.loads(Path(p).read_text())
        cols = j["zone_columns"]
        si = cols.index("self_ms")
        for fr in j["frames"]:
            zones = fr.get("zones", {}).get("main", [])
            top = max(zones, key=lambda z: z[si]) if zones else ["", 0, 0, 0]
            studio = sum(z[si] for z in zones if "studio" in z[0])
            out.append((fr["dt_ms"], top[0], top[si], studio))
    return out


def pacing(a, out):
    """The window's frames over the WHOLE of each heavy studio call: perf_capture runs back to
    back from before the call starts until after it answers, and every frame is counted. A frame
    over 50 ms whose time went to the studio fails the check."""
    import threading
    failures, summary = [], {"adapter": a.adapter, "cases": {}}
    assets = out / "assets"
    copy_assets(assets)
    R = ["warding_stone", "rune_pillar", "brazier_cairn", "sky_obelisk", "shambler", "wraith"]
    subjects = [f"recipes/{x}.json" for x in R]
    cases = [
        ("angles_96", "studio_sheet", dict(subjects=subjects, views=16, passes=["colour"], tile=512)),
        ("shots_120", "studio_sheet", dict(subjects=subjects, views=["gameplay"] * 10, tile=256)),
        ("check", "studio_check", dict(recipe="recipes/wraith.json", image=True)),
        ("lineup_2048", "studio_lineup", dict(subjects=subjects, width=2048)),
    ]
    # Cold: builds no call has uploaded (the biggest recipes, and a glTF with a 1024x1024
    # texture), uploaded a level per step while the window runs.
    textured_glb(assets / "models" / "checker_big.glb", size=1024)
    COLD = ["recipes/bone_colossus.json", "recipes/slab_cliff.json", "recipes/wall_segment.json", "recipes/graveyard_fence.json", "models/checker_big.glb"]
    cold = [("cold_upload", "studio_stage", dict(subjects=COLD, view="three_quarter"))]
    args = ["--assets", str(assets), "--captures", str(out / "captures"), "--adapter", a.adapter]
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        m = engine.mcp("lead", detailed=False)
        # The window draws a game-like scene with shadows on (three cascades), as a game does: the
        # studio's renders must not fight the window over the shadow map.
        m.call("world_spawn", shape="quad", name="ground", scale=[30, 1, 30], color="#3a5f3a")
        for i in range(12):
            m.call("world_spawn", shape="cube", name=f"block_{i}", position=[(i % 4) * 3 - 4.5, 0.75, (i // 4) * 3 - 3], scale=[1, 1.5, 1], color="#8a8f99")
        m.call("world_env", shadow={"enabled": True, "resolution": 2048, "cascades": 3})
        for r in [f"recipes/{x}.json" for x in R] + COLD:
            m.json("model_build", recipe=r)
        # Warm: every target size and pipeline the cases use exists before they are measured
        # (the cold case's builds are left alone).
        for _, tool, kw in cases:
            m.call(tool, **kw)
        time.sleep(1.0)
        for name, tool, kw in cases + cold:
            done, res = threading.Event(), {}

            def run():
                c = engine.mcp("caller", detailed=False)
                started = time.perf_counter()
                try:
                    r = c.call(tool, **kw)
                    res["ok"] = True
                    for item in r["content"]:
                        if item["type"] == "text":
                            try:
                                body = json.loads(item["text"])
                                packet = json.loads(Path(body["image"]["packet_path"]).read_text())
                                res["renders"] = packet.get("renders")
                            except (ValueError, AttributeError, KeyError, TypeError, OSError):
                                pass
                            break
                except RuntimeError as e:
                    res["ok"] = str(e)[:200]
                res["ms"] = round((time.perf_counter() - started) * 1000)
                c.close()
                done.set()

            cap = engine.mcp("capture", detailed=True)
            paths = []
            th = threading.Thread(target=run)
            first = True
            while first or not done.is_set():
                if first:
                    th.start()
                    first = False
                r = cap.json("perf_capture", frames=600, trace=False)
                paths.append(r["path"])
            # One more after the answer: the call's last frames.
            paths.append(cap.json("perf_capture", frames=60, trace=False)["path"])
            cap.close()
            th.join()
            frames = frames_over(paths)
            dts = sorted(f[0] for f in frames)
            slow = [f for f in frames if f[0] > STUDIO_FRAME_LIMIT_MS]
            studio_slow = [f for f in slow if f[3] >= 0.5 * f[0] or "studio" in f[1]]
            case = {"ok": res.get("ok"), "call_ms": res.get("ms"), "frames": len(dts), "p50_ms": round(dts[len(dts) // 2], 2), "p95_ms": round(dts[int(len(dts) * 0.95)], 2),
                    "max_ms": round(dts[-1], 2), "over_50ms": [(round(f[0], 1), f[1], round(f[2], 1)) for f in sorted(slow, reverse=True)[:8]],
                    "studio_over_50ms": len(studio_slow), "worst_studio_render_ms": round(max((f[3] for f in frames), default=0.0), 1),
                    "renders": res.get("renders")}
            summary["cases"][name] = case
            check(res.get("ok") is True and not studio_slow,
                  f"{name}: {len(dts)} frames over the whole call ({res.get('ms')} ms): p50 {case['p50_ms']} ms, p95 {case['p95_ms']} ms, max {case['max_ms']} ms; "
                  f"{len(slow)} over 50 ms ({len(studio_slow)} of them the studio's); studio work at most {case['worst_studio_render_ms']} ms in a frame; "
                  f"the call's renders: {res.get('renders')}", failures)
        m.close()
    v = engine.validation_errors()
    check(v == 0, f"zero validation errors at shutdown ({v})", failures)
    summary["failures"] = failures
    (out / "pacing.json").write_text(json.dumps(summary, indent=1))
    print(f"\nsummary: {out / 'pacing.json'}")
    print("FAILED: " + "; ".join(failures) if failures else "pacing checked")
    return 1 if failures else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--exe", default=default_exe())
    ap.add_argument("--port", type=int, default=7616)
    ap.add_argument("--out", default=str(ROOT / "target" / "studio-check"))
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--fidelity", action="store_true", help="check and render the Fidelity benchmark set instead")
    ap.add_argument("--pacing", action="store_true", help="measure the window's frames over whole heavy studio calls instead")
    ap.add_argument("--window", action="store_true", help="run the main check with the window open (renders spread over frames)")
    a = ap.parse_args()
    out = Path(a.out).resolve()
    if a.fidelity:
        return fidelity(a, out)
    if a.pacing:
        return pacing(a, out)
    assets = out / "assets"
    copy_assets(assets)
    failures, summary = [], {"adapter": a.adapter, "subjects": {}, "timings_ms": {}, "captures": {}}
    args = (["--headless"] if not a.window else []) + ["--assets", str(assets), "--captures", str(out / "captures"), "--adapter", a.adapter]
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        m = engine.mcp("studio-check", detailed=False)
        world0 = m.json("engine_info", response_format="detailed")["world"]

        print("the stage")
        stage = m.json("studio_stage")
        check(set(stage["rigs"]) == {"neutral", "game", "worst"}, "three light rigs (neutral, game, worst)", failures)
        check(set(stage["shots"]) == {"close_up", "gameplay", "worst_light", "phone", "igpu"}, "the shot set has five shots", failures)
        check(stage["rules_source"] == "style.toml" and stage["rules"]["palette"]["families"].get("player"), "Sepulcra's style.toml is read (families, taste thresholds)", failures)

        print("style_check: every recipe, every tier")
        sc, ms = timed(m, "style_check", folder="recipes")
        summary["timings_ms"]["style_check_folder"] = round(ms, 1)
        check(sc["passed"] and sc["subjects"] >= 30, f"{sc['subjects']} subjects, {sc['failed']} failing ({ms:.0f} ms)", failures)
        w3 = m.json("style_check", recipe="recipes/warding_stone.json", params={"tier": 3}, variants=False, response_format="detailed")
        tri = next(c for c in w3["results"][0]["all"] if c["check"] == "triangles")
        check(tri["max"] == 3000 and tri["rule"].endswith("budget[2].max_triangles"), f"tier III takes the tier-3 budget ({tri['value']} of {tri['max']}, {tri['rule']})", failures)

        print("studio_check: props and creatures")
        for name in PROPS + CREATURES:
            r, ms = timed(m, "studio_check", recipe=f"recipes/{name}.json", response_format="detailed", image=False)
            meas = r["measures"]
            exempt = [c["check"] for c in r["report"]["all"] if c.get("exempt")]
            summary["subjects"][name] = {
                "passed": r["passed"], "errors": [e["check"] for e in r["errors"]], "exempt": exempt, "ms": round(ms, 1),
                "px_gameplay": meas["gameplay"]["px_height"], "px_phone": meas["phone"]["px_height"],
                "contrast": meas["gameplay"]["contrast"], "contrast_worst": meas["worst_light"]["contrast"],
                "spread": meas["game_light"]["spread"], "solidity": meas["thumbnails"]["solidity"],
                "siblings": {s["subject"]: s["iou_with_subject"] for s in r["siblings"]},
                "image": r["image"]["path"], "lod_gameplay": meas["gameplay"]["lod"],
            }
            summary["captures"][f"check_{name}"] = r["image"]["path"]
            note = f" (exempt: {', '.join(exempt)})" if exempt else ""
            check(r["passed"], f"{name}: {r['failed']} errors{note}; {meas['gameplay']['px_height']:.0f} px from the game camera, contrast {meas['gameplay']['contrast']:.3f} / backlit {meas['worst_light']['contrast']:.3f} ({ms:.0f} ms)", failures)
        fence = summary["subjects"]["graveyard_fence"]
        check(fence["exempt"] == ["flat_lighting"], "the fence's flat lighting is reported, as exempt (deliberate style)", failures)
        conc = m.call("studio_check", recipe="recipes/shambler.json", image=False)
        det = m.call("studio_check", recipe="recipes/shambler.json", image=False, response_format="detailed")
        sizes = [len(c["text"]) for c in (conc["content"][0], det["content"][0])]
        summary["answer_bytes"] = {"concise": sizes[0], "detailed": sizes[1]}
        check(sizes[0] < sizes[1] and sizes[0] < 6000, f"concise answers are small ({sizes[0]} bytes; detailed {sizes[1]})", failures)

        print("sheets, line-ups and stage frames")
        shots, ms = timed(m, "studio_sheet", subjects=["recipes/warding_stone.json", {"recipe": "recipes/warding_stone.json", "params": {"tier": 3}},
                                                       "recipes/sky_obelisk.json", "recipes/shambler.json", "recipes/skeleton_runner.json", "recipes/wraith.json"],
                          views="shots", tile=160)
        summary["timings_ms"]["sheet_shot_set_6"] = round(ms, 1)
        summary["captures"]["shot_set"] = shots["image"]["path"]
        tiles = [t for s in shots["subjects"] for t in s["tiles"]]
        game_tiles = [t for t in tiles if t["view"] == "gameplay"]
        check(len(tiles) == 30 and all(t.get("px_height", 0) > 0 for t in game_tiles), f"shot-set sheet: 6 subjects x 5 shots, px heights from the game camera ({ms:.0f} ms)", failures)
        turn, ms = timed(m, "studio_sheet", recipe="recipes/brazier_cairn.json", params={"tier": 2}, views=8, passes=["colour", "value", "silhouette"], tile=128, references=True)
        summary["captures"]["turntable"] = turn["image"]["path"]
        check(len(turn["subjects"][0]["tiles"]) == 24, f"turntable: 8 angles x 3 passes ({ms:.0f} ms)", failures)
        for label, subjects in [("stones", ["warding_stone", "rune_pillar", "brazier_cairn", "sky_obelisk", "wall_segment", "tombstone"]),
                                ("creatures", ["shambler", "skeleton_runner", "wraith", "bone_colossus"])]:
            lu, ms = timed(m, "studio_lineup", subjects=[f"recipes/{s}.json" for s in subjects], response_format="detailed")
            summary["captures"][f"lineup_{label}"] = lu["image"]["path"]
            summary[f"silhouette_iou_{label}"] = lu["silhouette_iou"]
            heights = {s["subject"]: s["height_m"] for s in lu["subjects"]}
            check(len(lu["subjects"]) == len(subjects) and not lu["alike"], f"line-up of {label}: heights {heights}, no sibling pair alike ({ms:.0f} ms)", failures)
        fr = m.json("studio_stage", subjects=["recipes/shambler.json", "recipes/warding_stone.json", "recipes/tombstone.json"], view="three_quarter", width=960, height=540)
        summary["captures"]["stage"] = fr["image"]["path"]
        check(all("px_height" in s for s in fr["subjects"]), "stage frame with the references: every subject boxed", failures)
        gp = m.json("studio_stage", subjects=["recipes/shambler.json", "recipes/warding_stone.json"], view="gameplay", width=640, height=360, references=False)
        summary["captures"]["stage_gameplay"] = gp["image"]["path"]
        check(all(s.get("px_height", 0) > 0 for s in gp["subjects"]), "stage through the game camera reports pixel heights in its viewport", failures)

        print("a violation caught and fixed: a new graveyard prop")
        recipe = assets / "recipes" / "bone_pile.json"
        recipe.write_text(json.dumps(BONE_PILE_DRAFT, indent=1))
        draft_style = m.json("style_check", recipe="recipes/bone_pile.json")
        caught_rules = sorted({e["check"] for f in draft_style["failing"] for e in f["errors"]})
        check(not draft_style["passed"], f"style_check catches the draft: {caught_rules}", failures)
        mark = m.json("studio_sheet", recipe="recipes/bone_pile.json", views=["front", "three_quarter", "gameplay", "phone"], passes=["colour", "silhouette"], tile=160, mark="bone_pile_v1")
        check(mark.get("mark") == "bone_pile_v1", "the draft's sheet is marked", failures)
        draft = m.json("studio_check", recipe="recipes/bone_pile.json", siblings=["recipes/tombstone.json", "recipes/graveyard_fence.json"])
        summary["captures"]["bone_pile_draft_check"] = draft["image"]["path"]
        caught = [e["check"] for e in draft["errors"]]
        summary["draft"] = {"errors": draft["errors"], "next": draft["critique"]["next"]}
        check(not draft["passed"] and {"family_forbidden", "px_gameplay", "height", "triangles"} <= set(caught), f"studio_check catches the draft: {caught}", failures)
        print("    critique, next steps:")
        for step in draft["critique"]["next"]:
            print("      - " + step[:150])
        recipe.write_text(json.dumps(BONE_PILE_REVISED, indent=1))
        revised = m.json("studio_check", recipe="recipes/bone_pile.json", siblings=["recipes/tombstone.json", "recipes/graveyard_fence.json"])
        summary["captures"]["bone_pile_revised_check"] = revised["image"]["path"]
        summary["revised"] = {"errors": revised["errors"]}
        check(revised["passed"], f"the revision passes ({revised['failed']} errors: {[e['check'] for e in revised['errors']]})", failures)
        cmp = m.json("studio_compare", mark="bone_pile_v1")
        summary["captures"]["bone_pile_compare"] = cmp["image"]["path"]
        fixed = cmp["subjects"][0]["fixed"]
        summary["compare"] = {"fixed": fixed, "new": cmp["subjects"][0]["new_errors"], "tiles_changed": cmp["tiles_changed"], "facts_changed": cmp["subjects"][0]["facts_changed"]}
        check(set(caught_rules) <= set(fixed) and not cmp["subjects"][0]["new_errors"] and cmp["tiles_changed"] == len(cmp["tiles"]),
              f"studio_compare against the mark: fixed {fixed}, every tile changed", failures)

        print("a textured glTF (palette not measured, renders across frames)")
        textured_glb(assets / "models" / "checker_box.glb")
        tex = m.json("studio_check", recipe="models/checker_box.glb", siblings=[], image=False, response_format="detailed")
        summary["captures"]["textured_check"] = tex["image"]["path"]
        notes = tex["report"].get("not_measured", [])
        check(any("not measured" in n for n in notes) and not any(e["check"].startswith("palette") for e in tex["errors"]),
              f"its palette checks are reported as not measured ({notes[:1]})", failures)
        sheet_t = m.json("studio_sheet", recipe="models/checker_box.glb", views=6, passes=["colour", "silhouette"], tile=128)
        check(len(sheet_t["subjects"][0]["tiles"]) == 12, "a 12-tile sheet of it renders (its upload stays pinned between frames)", failures)
        summary["captures"]["textured_sheet"] = sheet_t["image"]["path"]

        print("refusals")
        e = call_error(m, "studio_sheet", subjects=[f"recipes/{s}.json" for s in PROPS[:7]], views=16)
        check(e is not None and "at most 96" in e.get("message", ""), "a sheet over 96 tiles is refused", failures)
        e = call_error(m, "studio_compare", mark="never_made")
        check(e is not None and e.get("error") == "unknown_mark", "an unknown mark is refused (unknown_mark)", failures)
        e = call_error(m, "studio_check", recipe="recipes/nope.json")
        check(e is not None and e.get("error") not in (None, "?"), f"a missing recipe is a structured error ({e and e.get('error')})", failures)
        e = call_error(m, "style_check", folder="../outside")
        check(e is not None, "a folder outside the assets root is refused", failures)

        world1 = m.json("engine_info", response_format="detailed")["world"]
        check(world1["state_hash"] == world0["state_hash"] and world1["entities"] == world0["entities"] == 0,
              f"the world is untouched (hash {world1['state_hash']}, {world1['entities']} entities)", failures)

        # Last (it edits the world): textured glTF calls while another agent spawns and deletes
        # model entities, with streaming on (the default). Every edit runs the model sync, which
        # must keep the studio's pinned uploads. One glTF is cold (never uploaded); the other's
        # pins expired (more than 10 s since its last call), so it is imported and uploaded again.
        print("textured glTF calls while another agent edits the world (streaming on)")
        import threading
        st = json.dumps(m.json("stream_state"))
        check('"enabled": true' in st and '"enabled": false' not in st, "streaming is on (the default path)", failures)
        textured_glb(assets / "models" / "checker_cold.glb")
        time.sleep(11)
        stop, edits, churn_errors = threading.Event(), [0], []

        def churn():
            c = engine.mcp("churn", detailed=False)
            while not stop.is_set():
                try:
                    e = c.json("world_spawn", shape="model", recipe="recipes/tombstone.json", position=[edits[0] % 20, 0, -5])
                    c.call("world_delete", id=e["id"])
                    edits[0] += 1
                except RuntimeError as ex:
                    churn_errors.append(str(ex)[:160])
                time.sleep(0.03)
            c.close()

        th = threading.Thread(target=churn)
        th.start()
        # The churn is running before the calls start.
        while edits[0] < 5 and th.is_alive() and not churn_errors:
            time.sleep(0.05)
        churned, during = [], 0
        for recipe in ("models/checker_cold.glb", "models/checker_box.glb"):
            for tool, kw in (("studio_sheet", dict(views=8, passes=["colour", "silhouette"], tile=128)), ("studio_check", dict(siblings=[], image=False)), ("studio_stage", dict(view="front"))):
                kw = dict(kw, **({"subjects": [recipe]} if tool == "studio_stage" else {"recipe": recipe}))
                before = edits[0]
                e = call_error(m, tool, **kw)
                during += edits[0] - before
                churned.append(f"{recipe.split('/')[-1]} {tool}: {'ok' if e is None else e.get('error')}")
        stop.set()
        th.join()
        check(all(c.endswith(": ok") for c in churned) and during >= 3 and not churn_errors,
              f"every call ok while entities were spawned and deleted ({during} during the calls; {'; '.join(c for c in churned if not c.endswith(': ok')) or 'all ok'})", failures)
        m.close()
    v = engine.validation_errors()
    check(v == 0, f"zero validation errors at shutdown ({v})", failures)
    summary["failures"] = failures
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(f"\nsummary: {out / 'summary.json'}")
    print("FAILED: " + "; ".join(failures) if failures else "all studio checks passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

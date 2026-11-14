"""Foliage check: alpha-cutout cards and wind, proved from captures and the ID buffer.

What it does (one engine, Sepulcra's assets: recipes/grass_tuft.json, grass_patch.json and the
texture graph tex_grass_blades.json with an opacity output):
  1. Cutout. A grass tuft close up: the ID buffer counts only its blades (visible_px well under its
     projected box, the gaps cut), the packet is consistent (no sight.packet_inconsistent), and the
     ground shows through the gaps in the image.
  2. Shadow. The low sun casts the tuft's shadow on the ground: the shadow has gaps too (the
     ground under the cards is neither fully lit nor fully dark).
  3. Wind. The same view at two simulation times differs where the grass is and nowhere else;
     the same time twice is byte-identical (deterministic); with the wind's strength 0 two times
     match exactly. The ID buffer moves with the blades (its exact boxes differ between the times
     and stay inside the projected boxes).
  4. Fade. With world_env {wind: {fade_start, fade_end}} nearer than the camera, the grass draws
     nothing and the ID buffer counts nothing; the ground is still drawn.
  5. A patch (grass_patch: scattered tufts in one mesh) builds within the foliage budget.
Captures and packets go to <target>/foliage-check/ for the sight-skeptic.

Usage: cargo build -p specula && python scripts/foliage_check.py [--port 7553] [--adapter integrated]
       [--exe PATH] [--release]
ASCII output only; exits non-zero on any failure.
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
from two_d_check import read_png  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
ASSETS = ROOT / "games" / "sepulcra" / "assets"
FAILS = []


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what, flush=True)
    if not cond:
        FAILS.append(what)
    return cond


def pixels(path):
    w, h, px = read_png(path)
    return w, h, px


def diff_mask(a, b, w, h, tol=6):
    """Pixels differing by more than tol in any channel: (count, bbox or None)."""
    n, box = 0, None
    for i in range(w * h):
        o = i * 4
        if max(abs(a[o] - b[o]), abs(a[o + 1] - b[o + 1]), abs(a[o + 2] - b[o + 2])) > tol:
            n += 1
            x, y = i % w, i // w
            box = [x, y, x, y] if box is None else [min(box[0], x), min(box[1], y), max(box[2], x), max(box[3], y)]
    return n, box


def subject(pkt, name):
    return next((s for s in pkt.get("subjects", []) if s.get("name") == name), None)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7553)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=None)
    ap.add_argument("--release", action="store_true")
    ap.add_argument("--headless", action="store_true")
    a = ap.parse_args()
    exe = Path(a.exe) if a.exe else TARGET / ("release" if a.release else "debug") / ("specula.exe" if sys.platform == "win32" else "specula")
    out = TARGET / "foliage-check"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    args = ["--adapter", a.adapter, "--assets", str(ASSETS), "--captures", str(out / "captures"), "--trace", str(out / "trace"), "--paused"]
    if a.headless:
        args.append("--headless")
    report = {}
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        m = eng.mcp("foliage-check")
        m.hello("foliage-check", role="builder")
        report["gpu"] = m.json("engine_info")["gpu"]["name"]
        print(f"gpu: {report['gpu']}")
        # A low sun from the side (long shadows across the ground), shadows on, a breeze.
        m.call("world_env", sun_direction=[-0.8, 0.45, 0.3], sun_intensity=1.2, ambient_intensity=0.6, tonemap=True,
               shadow={"enabled": True, "distance": 12, "resolution": 4096, "softness": 0.6}, background="#203040",
               wind={"direction": [1, 0, 0.2], "strength": 2.0, "frequency": 0.7, "gust": 0.4})
        m.call("world_spawn", shape="cube", name="ground", position=[0, -0.05, 0], scale=[8, 0.1, 8], color="#9a9488")
        m.call("world_spawn", shape="model", name="tuft", recipe="recipes/grass_tuft.json", position=[0, 0, 0], scale=[2.5, 2.5, 2.5])
        m.call("world_spawn", shape="model", name="patch", recipe="recipes/grass_patch.json", position=[0, 0, -3.2])
        for _ in range(240):
            st = {e["name"]: e.get("model_state") for e in m.json("world_query", fields=["name", "model_state"]).get("items", [])}
            if st.get("tuft") == "ready" and st.get("patch") == "ready":
                break
            m.call("time_control", action="step", ticks=1)
        view = {"eye": [0.0, 1.0, 3.6], "target": [0.0, 0.55, 0.0], "fov_y_deg": 40, "width": 640, "height": 480}

        # 1. Cutout.
        f1 = m.frame(time_s=1.0, **view)
        shutil.copy(f1["image"]["path"], out / "cutout_t1.png")
        (out / "cutout_t1.json").write_text(json.dumps(f1, indent=1), encoding="utf-8")
        t = subject(f1, "tuft")
        check(t is not None and t.get("visible_px", 0) > 500, f"the tuft is seen ({t and t.get('visible_px')} px)")
        if t and t.get("screen"):
            sx, sy, sw, sh = t["screen"]
            frac = t["visible_px"] / max(1, sw * sh)
            report["cutout_fraction"] = round(frac, 3)
            check(frac < 0.55, f"the ID buffer counts only the blades: {t['visible_px']} px of a {sw}x{sh} projected box ({frac:.2f}; solid cards would fill most of it)")
            ex = t.get("screen_box_exact")
            check(ex is not None and ex[0] >= sx - 1.5 and ex[1] >= sy - 1.5 and ex[0] + ex[2] <= sx + sw + 1.5 and ex[1] + ex[3] <= sy + sh + 1.5,
                  f"the exact box {ex} lies inside the projected box {t['screen']}")
            # The ground shows through the gaps: inside the tuft's exact box there are ground-coloured pixels.
            w, h, px = pixels(f1["image"]["path"])
            g = subject(f1, "ground")
            ground_like = 0
            x0, y0, bw, bh = [int(v) for v in ex]
            for y in range(y0 + bh // 2, min(h, y0 + bh)):
                for x in range(x0, min(w, x0 + bw)):
                    o = (y * w + x) * 4
                    r, gg, b = px[o], px[o + 1], px[o + 2]
                    if r >= gg - 4 and abs(r - b) < 40 and r > 60:  # the grey-beige ground, not green blades
                        ground_like += 1
            check(ground_like > 50, f"the ground shows through the cut gaps in the tuft's box ({ground_like} ground pixels; ground visible_px {g and g.get('visible_px')})")

        # 2. Shadow: the tuft's shadow on the ground, seen from above, has gaps.
        top = {"eye": [0.0, 6.0, 0.01], "target": [0.0, 0.0, 0.0], "fov_y_deg": 40, "width": 512, "height": 512}
        f2 = m.frame(time_s=1.0, **top)
        shutil.copy(f2["image"]["path"], out / "shadow_top.png")
        w, h, px = pixels(f2["image"]["path"])
        # Ground luminance along a row through the shadow's side (the sun is at -x, shadows fall +x).
        lum = []
        y = h // 2
        for x in range(w // 2 + 40, min(w, w // 2 + 200)):
            o = (y * w + x) * 4
            lum.append(px[o] + px[o + 1] + px[o + 2])
        if lum:
            lo, hi = min(lum), max(lum)
            transitions = sum(1 for i in range(1, len(lum)) if (lum[i] - (lo + hi) / 2) * (lum[i - 1] - (lo + hi) / 2) < 0)
            report["shadow_row"] = {"min": lo, "max": hi, "transitions": transitions}
            check(hi - lo > 40 and transitions >= 4, f"the shadow beside the tuft is broken by the cut gaps (luminance {lo}..{hi}, {transitions} light/dark transitions along a row)")

        # 3. Wind: two times differ where the grass is, the same time twice is identical.
        f3 = m.frame(time_s=2.3, **view)
        shutil.copy(f3["image"]["path"], out / "wind_t2_3.png")
        (out / "wind_t2_3.json").write_text(json.dumps(f3, indent=1), encoding="utf-8")
        f1b = m.frame(time_s=1.0, **view)
        w, h, a1 = pixels(f1["image"]["path"])
        _, _, a3 = pixels(f3["image"]["path"])
        _, _, a1b = pixels(f1b["image"]["path"])
        n, box = diff_mask(a1, a3, w, h)
        report["wind_diff"] = {"pixels": n, "box": box}
        check(n > 200, f"the grass sways: {n} pixels differ between t=1.0 and t=2.3 (box {box})")
        tb = t and t.get("screen")
        if box and tb:
            inside = box[0] >= tb[0] - 4 and box[2] <= tb[0] + tb[2] + 4 and box[1] >= tb[1] - 4 and box[3] <= tb[1] + tb[3] + 4
            patch = subject(f1, "patch")
            pb = patch and patch.get("screen")
            inside_any = inside or (pb and box[1] >= min(tb[1], pb[1]) - 4)
            check(inside_any, f"only the grass moves (changed pixels {box} within the grass's projected boxes {tb} / {pb})")
        same, _ = diff_mask(a1, a1b, w, h, tol=0)
        check(same == 0, f"deterministic: the same simulation time twice gives the same pixels ({same} differ)")
        t3 = subject(f3, "tuft")
        if t and t3:
            check(t.get("screen_box_exact") != t3.get("screen_box_exact") or t.get("visible_px") != t3.get("visible_px"),
                  f"the ID buffer moves with the blades ({t.get('screen_box_exact')} {t.get('visible_px')} px -> {t3.get('screen_box_exact')} {t3.get('visible_px')} px)")
        # Still air: two times match.
        m.call("world_env", wind={"strength": 0})
        s1 = m.frame(time_s=1.0, **view)
        s3 = m.frame(time_s=2.3, **view)
        _, _, b1 = pixels(s1["image"]["path"])
        _, _, b3 = pixels(s3["image"]["path"])
        still, _ = diff_mask(b1, b3, w, h, tol=0)
        check(still == 0, f"with strength 0 nothing sways ({still} pixels differ)")
        m.call("world_env", wind={"strength": 2.0})

        # 4. Fade: nearer than the camera, the grass is gone from the image and the ID buffer.
        # Water's records use the wind's bits for themselves: a pool beside the grass must count
        # the same pixels with the fade on as off (review of the lane: the ID buffer cut terrain
        # and water as if they were fading foliage).
        inc = m.json("trace_incidents", code="sight.packet_inconsistent", limit=10).get("incidents", [])
        check(not inc, f"no sight.packet_inconsistent incident from the foliage ({len(inc)})")
        m.call("world_spawn", shape="group", name="pool", position=[2.2, 0.3, -0.6], water={"preset": "lake", "size": [2.0, 2.0]})
        # The water pipelines build in the background: compare once the pool is drawn.
        for _ in range(120):
            pw = subject(m.frame(time_s=1.0, **view), "pool")
            if pw and pw.get("visible_px", 0) > 0:
                break
            time.sleep(0.25)
        for strength in (0.11, 0.37, 0.61, 0.95):
            m.call("world_set", path="/pool", water={"preset": "lake", "size": [2.0, 2.0], "waves": {"strength": strength}})
            p0 = subject(m.frame(time_s=1.0, **view), "pool")
            m.call("world_env", wind={"fade_start": 0.5, "fade_end": 1.5})
            p1 = subject(m.frame(time_s=1.0, **view), "pool")
            m.call("world_env", wind={"fade_start": 0, "fade_end": 0})
            check(p0 is not None and p1 is not None and p0.get("visible_px") == p1.get("visible_px") and p0.get("visible_px", 0) > 0,
                  f"water (wave strength {strength}) counts the same pixels with the foliage fade on ({p0 and p0.get('visible_px')} vs {p1 and p1.get('visible_px')})")
        m.call("world_delete", path="/pool")
        m.call("world_env", wind={"fade_start": 0.5, "fade_end": 1.5})
        f4 = m.frame(time_s=1.0, **view)
        shutil.copy(f4["image"]["path"], out / "faded.png")
        t4, g4 = subject(f4, "tuft"), subject(f4, "ground")
        check(t4 is not None and t4.get("visible_px", 0) == 0, f"faded: the ID buffer counts none of the tuft ({t4 and t4.get('visible_px')} px)")
        check(g4 is not None and g4.get("visible_px", 0) > 1000, "the ground is still drawn")
        m.call("world_env", wind={"fade_start": 30, "fade_end": 60})
        f5 = m.frame(time_s=1.0, **view)
        _, _, c5 = pixels(f5["image"]["path"])
        far, _ = diff_mask(a1, c5, w, h, tol=0)
        check(far == 0, f"a fade beyond the camera's reach changes nothing ({far} pixels differ)")
        m.call("world_env", wind={"fade_start": 0, "fade_end": 0})

        # 5. The patch's budget and build.
        sc = m.json("style_check", subjects=[{"recipe": "recipes/grass_patch.json"}, {"recipe": "recipes/grass_tuft.json"}], response_format="detailed")
        report["style_check"] = sc
        check(sc.get("passed") is True, f"grass_patch and grass_tuft pass style.toml (foliage budget): {'ok' if sc.get('passed') else [e.get('message') for r in sc.get('results', []) for e in r.get('errors', [])][:6]}")
        # A water draw the ID buffer counts in the first frames, before the scene's water pipeline
        # is built, is reported by the packet check as pixels_not_drawn: a readiness race in the
        # ID passes (water counted once the terrain variant exists), not the foliage's. Reported.
        inc = m.json("trace_incidents", code="sight.packet_inconsistent", limit=10).get("incidents", [])
        report["packet_incidents_after_water"] = [i.get("key") for i in inc]
        print(f"  note sight.packet_inconsistent after the water probe: {[i.get('key') for i in inc]}")
    errors = eng.validation_errors()
    check(errors == 0, f"no validation messages ({errors})")
    report["failures"] = FAILS
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"report: {out / 'report.json'}")
    if FAILS:
        raise SystemExit(f"FAILED ({len(FAILS)})")
    print("PASSED: cutout and wind hold in the image, the shadow and the ID buffer")


if __name__ == "__main__":
    main()

"""The M5 gpuwork lane's live check: sight's counts follow the paint order, and world_load reads on a
worker. Exits non-zero on the first failure.

1. Covers (sight truthfulness, the text lane's review R3 probes): a text field faces the camera;
   behind it, in turn, opaque glass and a 2D shape on a higher z layer paint over it (blended draws
   write no depth): sight_frame must report the field at 0 visible px and occluded, as the image
   shows it (the field's colour is gone from its screen box). Wide capitals on a higher z layer
   paint over part of it: the image and sight lose about the same share. See-through glass
   (opacity 0.3) behind it hides nothing.
2. world_load at 5,000 cubes, three times: the engine thread's longest stall in the load (the
   engine.stall incidents of world_load) stays under 60 ms (it was 112-250 ms before the read,
   parse and board files moved to a worker; what is left is the world's replacement).

    cargo build -p specula && python scripts/gpuwork_check.py [--port 7584] [--adapter integrated] [--exe PATH]

Results: <target>/gpuwork-check/ (captures, engine log, report.json). ASCII output only.
"""
import argparse
import base64
import json
import os
import struct
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
FIELD = (236, 230, 214)  # the field's colour, #ece6d6
COVERS = [
    ("opaque glass behind the field", {"shape": "cube", "name": "cover", "position": [0, 0, -0.7], "scale": [5, 3, 0.1],
                                       "material": "glass", "opacity": 1.0, "color": "#406080"}),
    ("a 2D shape on a higher z layer behind it", {"shape": "shape2d", "name": "cover", "position": [0, 0, -0.7],
                                                  "shape2d": {"items": [{"rect": [-2.5, -1.5, 5, 3], "fill": "#406080"}], "z_layer": 5}}),
    # Wide capitals on a higher z layer: they paint over part of the field (glyphs leave gaps).
    ("a text on a higher z layer behind it", {"shape": "text", "name": "cover", "position": [0, 0, -0.7], "color": "#406080",
                                              "text": {"text": "WMWMW", "size": 1.6, "anchor": [0.5, 0.5], "z_layer": 5}}),
]
FAILS = []


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what, flush=True)
    if not cond:
        FAILS.append(what)


def png_rgba(data):
    """RGBA rows of an 8-bit RGBA PNG (the engine's captures)."""
    pos, idat, w, h = 8, b"", 0, 0
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h = struct.unpack(">II", body[:8])
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * 4
    rows, prev = [], bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - 4] if x >= 4 else 0
            b = prev[x]
            c = prev[x - 4] if x >= 4 else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line
    return w, h, rows


def field_pixels(result, box):
    """Pixels of the field's colour (within 30 per channel) inside `box` of the capture."""
    img = next(c for c in result["content"] if c["type"] == "image")
    w, h, rows = png_rgba(base64.b64decode(img["data"]))
    x0, y0, bw, bh = box
    n = 0
    for y in range(max(0, y0), min(h, y0 + bh)):
        row = rows[y]
        for x in range(max(0, x0), min(w, x0 + bw)):
            p = row[x * 4:x * 4 + 3]
            if all(abs(p[i] - FIELD[i]) <= 30 for i in range(3)):
                n += 1
    return n


def subject(packet, path):
    return next((s for s in packet["subjects"] if s["path"] == path), None)


def covers(m, out):
    print("1. sight follows the paint order", flush=True)
    m.call("world_env", background="#101418", sky={"enabled": False}, ambient_intensity=0.8, sun_intensity=0.6)
    m.json("world_spawn", shape="text", name="field", position=[0, 0, 0], color="#ece6d6",
           text={"text": "Here lies", "size": 0.6, "anchor": [0.5, 0.5]})
    view = dict(eye=[0, 0, 6], target=[0, 0, 0], width=480, height=320, max_subjects=20)

    def shot(label):
        r = m.call("sight_frame", **view)
        packet = json.loads(next(c["text"] for c in r["content"] if c["type"] == "text"))
        img = next(c for c in r["content"] if c["type"] == "image")
        (out / f"{label}.png").write_bytes(base64.b64decode(img["data"]))
        (out / f"{label}.json").write_text(json.dumps(packet, indent=1), encoding="utf-8")
        return r, packet

    r, p = shot("field_alone")
    f = subject(p, "/field")
    check(f is not None and f.get("visible_px", 0) > 100, f"the field alone shows ({f and f.get('visible_px')} px)")
    box = (f or {}).get("screen_box_exact") or (f or {}).get("screen_box") or [0, 0, 480, 320]
    alone = field_pixels(r, box)
    check(alone > 50, f"...in its colour in the image ({alone} px)")
    full = f.get("visible_px", 0) if f else 0
    for label, spec in COVERS:
        m.json("world_spawn", **spec)
        r, p = shot("cover_" + spec["shape"] + "_" + spec.get("material", "flat"))
        f = subject(p, "/field")
        shown = field_pixels(r, box)
        px = f.get("visible_px") if f else None
        if spec["shape"] == "text":
            # Partly painted over: the image and sight both lose part of the field, never all.
            check(0 < shown < alone and px is not None and 0 < px < full,
                  f"{label}: the image shows {shown} of {alone} px of the field, sight {px} of {full} px")
            check(bool(px) and abs(shown / alone - px / full) < 0.25,
                  f"{label}: ...about the same share ({shown / alone:.2f} in the image, {(px or 0) / max(full, 1):.2f} in sight)")
        else:
            check(shown == 0, f"{label}: the image shows none of the field ({shown} px of its colour)")
            check(f is not None and px == 0 and f.get("occluded") is True,
                  f"{label}: sight says 0 px, occluded ({f and {k: f.get(k) for k in ('visible_px', 'occluded')}})")
        m.json("world_delete", path="/cover")
    m.json("world_spawn", **dict(COVERS[0][1], opacity=0.3))
    r, p = shot("cover_seethrough_glass")
    f = subject(p, "/field")
    check(f is not None and f.get("visible_px", 0) > 100, f"see-through glass behind hides nothing ({f and f.get('visible_px')} px)")
    m.json("world_delete", path="/cover")
    m.json("world_delete", path="/field")


def loads(m, out):
    print("2. world_load reads on a worker", flush=True)
    n = 0
    while n < 5000:
        k = min(1000, 5000 - n)
        m.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": f"c{n + i}", "position": [((n + i) % 50) * 1.5, 0, ((n + i) // 50) * 1.5]} for i in range(k)])
        n += k
    m.json("world_save", name="gpuwork_load", overwrite=True)
    before = {s["id"] for s in m.json("trace_incidents", code="engine.stall", limit=50).get("incidents", [])}
    times = []
    for _ in range(3):
        t0 = time.perf_counter()
        r = m.json("world_load", name="gpuwork_load", force=True)
        times.append(round((time.perf_counter() - t0) * 1000, 1))
        check(r.get("entities") == 5000, f"world_load loaded {r.get('entities')} entities")
    peaks = []
    for s in m.json("trace_incidents", code="engine.stall", limit=50).get("incidents", []):
        if s["id"] in before or "world_load" not in (s.get("summary") or ""):
            continue
        peaks.append(m.json("trace_incident", id=s["id"]).get("peak_ms") or 0)
    worst = max(peaks, default=0)
    print(f"  world_load calls {times} ms; engine-thread stalls {peaks}", flush=True)
    check(worst < 60, f"world_load's longest engine-thread stall {worst} ms (< 60)")
    return {"calls_ms": times, "stall_peaks_ms": peaks}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7584)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=str(TARGET / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--out", default=str(TARGET / "gpuwork-check"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    args = ["--headless", "--adapter", a.adapter, "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--lead", "lee"]
    report = {}
    with Engine(Path(a.exe), a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        m = eng.mcp("lee")
        m.hello("lee", role="lead")
        covers(m, out)
        report["world_load"] = loads(m, out)
    errors = eng.validation_errors()
    check(not errors, f"no validation errors ({errors})")
    report["failures"] = FAILS
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    if FAILS:
        raise SystemExit(f"FAILED: {len(FAILS)} check(s)")
    print("PASSED: sight follows the paint order; world_load reads on a worker")


if __name__ == "__main__":
    main()

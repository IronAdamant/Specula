"""Live check of `model_export` (docs/design/EXPORT.md): recipes written as .glb files that
other engines read, and read back by this one.

On a copy of Sepulcra's assets (nothing is written into the repository):

- a prop, a rigged creature with its clips, a terrain and a piece of foliage are exported;
- each file is a valid GLB by an independent reading here (the container, the JSON, every
  accessor inside its buffer, indices inside the vertex count, a skin whose joints are nodes,
  animations whose samplers hold times and values of the same count);
- each answer's numbers (triangles, vertices, bones, clips, bytes) are what the file holds;
- each file imports back through `model_build`, with the same triangles, bones and clips;
- the original and the re-imported model, spawned side by side and captured, cover the same
  screen area with the same colours (the picture is kept for a person to look at);
- the write is a project write: journaled as the agent's, refused over another agent's claim,
  taken back by `world_undo`, and `dry_run` writes nothing; an export that changes nothing
  says so, and never points at `world_undo`;
- a solid surface under a `MASK` material, as other tools' assets have them (a texture that is
  opaque everywhere), is drawn whole at every distance, as the same surface is under an
  `OPAQUE` material, on every adapter (review R1: keeping a cut-out's coverage through the
  mips once put holes in such surfaces, or made them vanish).

Usage: cargo build -p specula --release && python scripts/export_check.py [--exe PATH] [--port 7671]
"""
import argparse
import json
import os
import shutil
import struct
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

FAILS = []


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return bool(cond)


def refused(mcp, tool, **args):
    """A call expected to be refused: its error JSON ({} when it was not refused)."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    text = result["content"][0]["text"]
    if not result.get("isError"):
        return {}
    try:
        return json.loads(text)
    except ValueError:
        return {"error": "text", "message": text}


COMPONENTS = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}
WIDTHS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def read_glb(path):
    """A GLB read by its specification, independently of the engine: (the JSON, the binary
    chunk, a list of what is wrong)."""
    data = Path(path).read_bytes()
    wrong = []
    if len(data) < 20 or data[:4] != b"glTF":
        return {}, b"", ["not a GLB (no glTF magic)"]
    version, length = struct.unpack("<II", data[4:12])
    if version != 2 or length != len(data):
        wrong.append(f"header: version {version}, length {length} of {len(data)} bytes")
    pos, doc, binary = 12, {}, b""
    while pos + 8 <= len(data):
        n, kind = struct.unpack("<I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if n % 4:
            wrong.append(f"chunk {kind!r} is not padded to 4 bytes")
        if kind == b"JSON":
            doc = json.loads(body.decode("utf-8"))
        elif kind == b"BIN\x00":
            binary = body
        pos += 8 + n
    if doc.get("asset", {}).get("version") != "2.0":
        wrong.append("asset.version is not 2.0")
    views, accessors = doc.get("bufferViews", []), doc.get("accessors", [])
    for i, v in enumerate(views):
        if v.get("byteOffset", 0) + v["byteLength"] > len(binary):
            wrong.append(f"bufferView {i} runs past the buffer")
    for i, a in enumerate(accessors):
        v = views[a["bufferView"]]
        size = COMPONENTS[a["componentType"]] * WIDTHS[a["type"]]
        if a.get("byteOffset", 0) + a["count"] * size > v["byteLength"]:
            wrong.append(f"accessor {i} runs past its view")
    return doc, binary, wrong


def accessor_values(doc, binary, index):
    """An accessor's values as a flat list (floats or integers)."""
    a = doc["accessors"][index]
    v = doc["bufferViews"][a["bufferView"]]
    start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    n = a["count"] * WIDTHS[a["type"]]
    fmt = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}[a["componentType"]]
    return struct.unpack_from("<" + fmt * n, binary, start)


def facts(doc, binary):
    """What a GLB holds, counted from the file: triangles, vertices, bones, clips, and what
    is wrong inside it."""
    wrong = []
    triangles = vertices = 0
    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"]:
            count = doc["accessors"][prim["attributes"]["POSITION"]]["count"]
            vertices = max(vertices, count)
            for name, index in prim["attributes"].items():
                if doc["accessors"][index]["count"] != count:
                    wrong.append(f"{name} has another count than POSITION")
            indices = accessor_values(doc, binary, prim["indices"])
            if len(indices) % 3:
                wrong.append("an index count that is not a multiple of 3")
            if indices and max(indices) >= count:
                wrong.append("an index past the last vertex")
            triangles += len(indices) // 3
            if "NORMAL" in prim["attributes"]:
                normals = accessor_values(doc, binary, prim["attributes"]["NORMAL"])
                bad = sum(1 for i in range(0, len(normals), 3) if abs(sum(c * c for c in normals[i:i + 3]) - 1.0) > 0.02)
                if bad:
                    wrong.append(f"{bad} normals that are not unit length")
    bones = 0
    for skin in doc.get("skins", []):
        bones = len(skin["joints"])
        if any(j >= len(doc["nodes"]) for j in skin["joints"]):
            wrong.append("a skin joint that is no node")
        if doc["accessors"][skin["inverseBindMatrices"]]["count"] != bones:
            wrong.append("inverse bind matrices of another count than the joints")
    clips = []
    for anim in doc.get("animations", []):
        clips.append(anim.get("name", ""))
        for sampler in anim["samplers"]:
            times = doc["accessors"][sampler["input"]]
            values = doc["accessors"][sampler["output"]]
            if times["count"] == 0 or values["count"] % times["count"]:
                wrong.append(f"animation {anim.get('name')}: {values['count']} values for {times['count']} times")
    # Images are PNG files inside the buffer, and every texture a material names exists.
    for i, image in enumerate(doc.get("images", [])):
        v = doc["bufferViews"][image["bufferView"]]
        start = v.get("byteOffset", 0)
        if image.get("mimeType") != "image/png" or binary[start:start + 8] != b"\x89PNG\r\n\x1a\n":
            wrong.append(f"image {i} is not a PNG in the buffer")
    textures = doc.get("textures", [])
    for t in textures:
        if t["source"] >= len(doc.get("images", [])) or t.get("sampler", 0) >= len(doc.get("samplers", [])):
            wrong.append("a texture whose image or sampler is not in the file")
    for m in doc.get("materials", []):
        named = [m.get("pbrMetallicRoughness", {}).get(k) for k in ("baseColorTexture", "metallicRoughnessTexture")] + [m.get(k) for k in ("normalTexture", "emissiveTexture")]
        if any(t is not None and t["index"] >= len(textures) for t in named):
            wrong.append(f"material {m.get('name')} names a texture that is not in the file")
    # COLOR_0's alpha is opacity to every other program: always 1 here (the wind's weights
    # travel in _WIND).
    alpha_below_one = wind = 0
    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"][:1]:
            colour = doc["accessors"][prim["attributes"]["COLOR_0"]]
            if colour["type"] == "VEC4":
                values = accessor_values(doc, binary, prim["attributes"]["COLOR_0"])
                alpha_below_one += sum(1 for i in range(3, len(values), 4) if values[i] < 0.999)
            if "_WIND" in prim["attributes"]:
                wind = doc["accessors"][prim["attributes"]["_WIND"]]["count"]
    if alpha_below_one:
        wrong.append(f"{alpha_below_one} vertex colours with an alpha below 1 (other programs read it as opacity)")
    return {"triangles": triangles, "vertices": vertices, "bones": bones, "clips": clips, "materials": [m.get("name") for m in doc.get("materials", [])],
            "images": len(doc.get("images", [])), "wind_weights": wind, "cutouts": sum(1 for m in doc.get("materials", []) if m.get("alphaMode") == "MASK")}, wrong


def decode_png(path):
    """An RGB8 or RGBA8 non-interlaced PNG -> (width, height, bytes per pixel, the pixels)."""
    data = Path(path).read_bytes()
    pos, idat = 8, b""
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, _, ctype, _, _, _ = struct.unpack(">IIBBBBB", body)
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * bpp
    out, prev, i = bytearray(h * stride), bytearray(stride), 0
    for y in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                q = a + b - c
                qa, qb, qc = abs(q - a), abs(q - b), abs(q - c)
                line[x] = (line[x] + (a if qa <= qb and qa <= qc else (b if qb <= qc else c))) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, bpp, bytes(out)


def png_rgba(width, height, pixel):
    """A PNG (RGBA8) whose pixels are `pixel(x, y) -> (r, g, b, a)`."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw += bytes(pixel(x, y))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(raw), 6)) + chunk(b"IEND", b"")


def plane_glb(alpha_mode, cutoff):
    """A model as another tool writes one: an 8 m square on the ground with one textured
    material, the texture repeated 64 times and opaque everywhere (alpha 255), under
    `alpha_mode` ("OPAQUE" or "MASK" with `cutoff`)."""
    image = png_rgba(256, 256, lambda x, y: (150, 140, 120, 255) if ((x // 32) + (y // 32)) % 2 else (95, 110, 90, 255))
    positions = struct.pack("<12f", -4, 0, -4, -4, 0, 4, 4, 0, 4, 4, 0, -4)
    normals = struct.pack("<12f", *([0, 1, 0] * 4))
    uvs = struct.pack("<8f", 0, 0, 0, 64, 64, 64, 64, 0)
    indices = struct.pack("<6H", 0, 1, 2, 0, 2, 3)
    parts, views = [positions, normals, uvs, indices, image], []
    binary = b""
    for part in parts:
        binary += b"\x00" * (-len(binary) % 4)
        views.append({"buffer": 0, "byteOffset": len(binary), "byteLength": len(part)})
        binary += part
    binary += b"\x00" * (-len(binary) % 4)
    material = {"name": "ground", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0, "roughnessFactor": 0.9}}
    if alpha_mode == "MASK":
        material.update({"alphaMode": "MASK", "alphaCutoff": cutoff})
    doc = {
        "asset": {"version": "2.0", "generator": "export_check.py (an asset from another tool)"},
        "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0, "name": "plane"}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}, "indices": 3, "material": 0}]}],
        "materials": [material],
        "textures": [{"source": 0, "sampler": 0}], "images": [{"bufferView": 4, "mimeType": "image/png"}],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-4, 0, -4], "max": [4, 0, 4]},
            {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": 6, "type": "SCALAR"},
        ],
        "bufferViews": views, "buffers": [{"byteLength": len(binary)}],
    }
    text = json.dumps(doc).encode("utf-8")
    text += b" " * (-len(text) % 4)
    body = struct.pack("<I4s", len(text), b"JSON") + text + struct.pack("<I4s", len(binary), b"BIN\x00") + binary
    return struct.pack("<4sII", b"glTF", 2, 12 + len(body)) + body


PLANES = [("opaque", "OPAQUE", None), ("mask_033", "MASK", 0.33), ("mask_05", "MASK", 0.5), ("mask_09", "MASK", 0.9)]


def solid_planes(exe, out, adapter, port):
    """An opaque texture under a MASK material is a solid surface: drawn whole, from near and
    from far, as it is under an OPAQUE material."""
    assets = out / f"plane_assets_{adapter}"
    (assets / "models").mkdir(parents=True)
    for name, mode, cutoff in PLANES:
        (assets / "models" / f"plane_{name}.glb").write_bytes(plane_glb(mode, cutoff))
    args = ["--headless", "--adapter", adapter, "--assets", str(assets), "--captures", str(out / f"plane_captures_{adapter}"), "--worlds", str(out / f"plane_worlds_{adapter}"), "--cache", str(out / f"plane_cache_{adapter}")]
    try:
        engine = Engine(exe, port, out / f"engine_planes_{adapter}.log", *args, cwd=str(out))
        engine.__enter__()
    except RuntimeError as e:
        print(f"skip  solid planes on the {adapter} adapter: no such adapter here ({str(e).splitlines()[0][:80]})")
        return None
    seen = {}
    try:
        m = engine.mcp("planes")
        for i, (name, _, _) in enumerate(PLANES):
            m.json("world_spawn", shape="model", recipe=f"models/plane_{name}.glb", path=f"/plane_{name}", position=[i * 60.0, 0, 0])
        time.sleep(1.0)
        # From near, from the middle distance and from far and low (where the smallest mips are read).
        views = [("near", [0, 5, -9]), ("middle", [0, 9, -22]), ("far", [0, 2.5, -34])]
        for view, eye in views:
            for i, (name, _, _) in enumerate(PLANES):
                x = i * 60.0
                frame = m.frame(width=640, height=360, eye=[x + eye[0], eye[1], eye[2]], target=[x, 0, 0], response_format="detailed")
                packet = json.loads(Path(frame["image"]["packet_path"]).read_text(encoding="utf-8"))
                px = next((sub.get("visible_px", 0) for sub in packet.get("subjects", []) if sub.get("path") == f"/plane_{name}"), 0)
                seen[(name, view)] = px
                if name in ("opaque", "mask_09"):
                    shutil.copy(frame["image"]["path"], out / f"plane_{adapter}_{name}_{view}.png")
        m.close()
    finally:
        engine.__exit__(None, None, None)
    for view, _ in views:
        whole = seen[("opaque", view)]
        for name, _, cutoff in PLANES[1:]:
            px = seen[(name, view)]
            check(whole > 500 and abs(px - whole) <= max(8, whole // 200), f"{adapter} adapter, {view}: an opaque texture under MASK {cutoff} is drawn whole ({px} px; the same plane under OPAQUE: {whole} px)")
    return seen


def colour_of(image, box):
    """The mean colour of what is drawn inside a subject's screen box: its pixels that are not
    the background's colour (the image's top-left pixel)."""
    w, h, bpp, px = image
    back = px[0:3]
    x0, y0, bw, bh = (int(v) for v in box)
    total, n = [0, 0, 0], 0
    for y in range(max(y0, 0), min(y0 + bh, h)):
        row = y * w * bpp
        for x in range(max(x0, 0), min(x0 + bw, w)):
            at = row + x * bpp
            if abs(px[at] - back[0]) + abs(px[at + 1] - back[1]) + abs(px[at + 2] - back[2]) > 36:
                n += 1
                for k in range(3):
                    total[k] += px[at + k]
    return [round(t / max(n, 1)) for t in total], n


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--port", type=int, default=7671)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--out")
    a = ap.parse_args()
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula --release")
    out = Path(a.out) if a.out else ROOT / "target" / "export-check"
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    assets = out / "assets"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    args = ["--headless", "--adapter", a.adapter, "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--cache", str(out / "cache")]
    report = {}
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        ann, bob = eng.mcp("ann"), eng.mcp("bob")
        ann.hello("ann")
        bob.hello("bob")
        check("model_export" in ann.tools(), "model_export is in the tool list")

        # ---- dry_run: what the file would hold, and nothing written ----
        dry = ann.json("model_export", recipe="recipes/statue_angel.json", dry_run=True)
        check(dry.get("dry_run") is True and dry.get("triangles", 0) > 0 and dry.get("path") == "assets/models/exports/statue_angel.glb" and not (assets / "models").exists(),
              f"dry_run reports ({dry.get('triangles')} triangles, {dry.get('bytes')} bytes) and writes nothing")

        # ---- four kinds of asset ----
        subjects = [
            ("statue_angel", "a prop", {}),
            ("shambler", "a rigged creature with clips", {}),
            ("graveyard_ground", "a terrain", {}),
            ("grass_patch", "foliage", {}),
        ]
        for name, what, params in subjects:
            built = ann.json("model_build", recipe=f"recipes/{name}.json", params=params)
            answer = ann.json("model_export", recipe=f"recipes/{name}.json", params=params)
            path = assets / "models" / "exports" / f"{name}.glb"
            report[name] = answer
            check(path.is_file() and answer.get("version") == 1 and answer.get("bytes") == path.stat().st_size, f"{what} ({name}) is written: {answer.get('bytes')} bytes at {answer.get('path')}, version {answer.get('version')}")
            if not path.is_file():
                continue
            doc, binary, wrong = read_glb(path)
            held, inside = facts(doc, binary) if doc else ({}, ["no JSON chunk"])
            check(not wrong and not inside, f"  the file is a valid GLB by an independent reading ({(wrong + inside)[:3]})")
            check((held.get("triangles"), held.get("vertices"), held.get("bones"), len(held.get("clips", []))) == (answer.get("triangles"), answer.get("vertices"), answer.get("bones"), len(answer.get("clips", []))),
                  f"  the answer says what the file holds: {held.get('triangles')} triangles, {held.get('vertices')} vertices, {held.get('bones')} bones, {len(held.get('clips', []))} clips")
            lod0 = (built.get("lods") or [{}])[0]
            check(answer.get("triangles") == lod0.get("triangles") and answer.get("vertices") == lod0.get("vertices"), f"  and it is the model model_build builds ({lod0.get('triangles')} triangles, {lod0.get('vertices')} vertices at LOD 0)")
            check(isinstance(answer.get("not_carried"), list) and answer["not_carried"] and isinstance(answer.get("carried"), list), f"  the answer says what was and was not carried ({len(answer.get('not_carried', []))} notes)")
            check(held.get("images") == answer.get("images"), f"  and how many images are in it ({held.get('images')})")
            if name == "grass_patch":
                check(held.get("images", 0) >= 2 and held.get("cutouts") == 1, f"  the grass carries its baked cut-out texture ({held.get('images')} images, a MASK material)")
            # Read back by this engine, as another engine would read it.
            back = ann.json("model_build", recipe=f"models/exports/{name}.glb")
            back0 = (back.get("lods") or [{}])[0]
            check(back0.get("triangles") == answer.get("triangles"), f"  it imports back with the same triangles ({back0.get('triangles')})")
            if answer.get("bones"):
                clips_back = sorted(c.get("name") for c in back.get("clips") or [])
                check(back.get("bones") == answer.get("bones") and clips_back == sorted(answer.get("clips", [])), f"  with its {back.get('bones')} bones and its clips ({', '.join(clips_back)})")
                check(any("no tracks" in n for n in answer.get("not_carried", [])), "  and the answer says which clip is not in the file, and why")

        # ---- the original and the re-import, side by side ----
        # The same distance from the camera, the same light: what is drawn is the same picture
        # twice if the file carries the shape and the colours (a colour written in the wrong
        # space, linear for sRGB, would show as a lighter or darker copy).
        pairs = []
        for i, (name, _, _) in enumerate(subjects[:2] + subjects[3:]):
            x = i * 9.0
            ann.json("world_spawn", shape="model", recipe=f"recipes/{name}.json", path=f"/original_{name}", position=[x, 0, 0])
            ann.json("world_spawn", shape="model", recipe=f"models/exports/{name}.glb", path=f"/exported_{name}", position=[x + 4.0, 0, 0])
            pairs.append(name)
        time.sleep(1.0)
        frame = ann.frame(width=1280, height=720, eye=[11, 7, -17], target=[11, 1.2, 0], response_format="detailed")
        shutil.copy(frame["image"]["path"], out / "original_and_exported.png")
        shutil.copy(frame["image"]["packet_path"], out / "original_and_exported.json")
        packet = json.loads(Path(frame["image"]["packet_path"]).read_text(encoding="utf-8"))
        seen = {s.get("path"): s for s in packet.get("subjects", [])}
        image = decode_png(out / "original_and_exported.png")
        colours = {}
        for name in pairs:
            orig, exp = seen.get(f"/original_{name}", {}), seen.get(f"/exported_{name}", {})
            po, pe = orig.get("visible_px", 0), exp.get("visible_px", 0)
            check(po > 200 and pe > 200 and 0.75 < pe / max(po, 1) < 1.33, f"{name}: the original and the exported model are drawn at the same size ({po} and {pe} px)")
            if orig.get("screen_box_exact") and exp.get("screen_box_exact"):
                co, no = colour_of(image, orig["screen_box_exact"])
                ce, ne = colour_of(image, exp["screen_box_exact"])
                colours[name] = {"original": co, "exported": ce}
                check(no > 100 and ne > 100 and max(abs(a - b) for a, b in zip(co, ce)) <= 14, f"{name}: and in the same colours (mean {co} and {ce})")
        report["frame"] = {"png": str(out / "original_and_exported.png"), "subjects": {k: v.get("visible_px") for k, v in seen.items()}, "colours": colours}

        # The terrain too, each copy in a frame of its own from the same place relative to it
        # (a terrain is wider than the row above).
        ground = {}
        for which, recipe, x in (("original", "recipes/graveyard_ground.json", 400.0), ("exported", "models/exports/graveyard_ground.glb", 800.0)):
            ann.json("world_spawn", shape="model", recipe=recipe, path=f"/{which}_graveyard_ground", position=[x, 0, 0])
        time.sleep(1.0)
        for which, x in (("original", 400.0), ("exported", 800.0)):
            frame = ann.frame(width=960, height=540, eye=[x, 26, -34], target=[x, 0, 0], response_format="detailed")
            shutil.copy(frame["image"]["path"], out / f"terrain_{which}.png")
            packet = json.loads(Path(frame["image"]["packet_path"]).read_text(encoding="utf-8"))
            subject = next((sub for sub in packet.get("subjects", []) if sub.get("path") == f"/{which}_graveyard_ground"), {})
            box = subject.get("screen_box_exact") or [0, 0, 960, 540]
            ground[which] = (subject.get("visible_px", 0), colour_of(decode_png(out / f"terrain_{which}.png"), box)[0])
        (po, co), (pe, ce) = ground["original"], ground["exported"]
        check(po > 2000 and 0.95 < pe / max(po, 1) < 1.05, f"graveyard_ground: the original and the exported terrain are drawn at the same size ({po} and {pe} px)")
        check(max(abs(a - b) for a, b in zip(co, ce)) <= 10, f"graveyard_ground: and in the same colours (mean {co} and {ce})")
        report["terrain"] = {"original": ground["original"], "exported": ground["exported"]}

        # ---- a project write like any other ----
        history = ann.json("world_history", limit=40, response_format="detailed")
        mine = [e for e in history.get("entries", []) if e.get("by") == "ann" and "models/exports" in json.dumps(e)]
        check(len(mine) >= 1, f"the export is in the journal as ann's ({len(mine)} entries name models/exports)")
        again = ann.json("model_export", recipe="recipes/statue_angel.json")
        check(again.get("unchanged") is True and again.get("version") == 1, f"the same export again changes nothing (version {again.get('version')})")
        check("world_undo" not in again.get("next", "") and "nothing was written" in again.get("next", ""), "and its answer does not point at world_undo: there is nothing of that call's to undo")
        first = report["statue_angel"]
        check("world_undo takes this write back" in first.get("next", ""), "an export that wrote the file says how to take it back")
        stale = refused(ann, "model_export", recipe="recipes/statue_angel.json", expect_version=7)
        check(stale.get("error") == "stale_version", f"a stale expect_version is refused ({stale.get('error')})")
        bob.json("claim_take", path="/project/assets/models/exports")
        blocked = refused(ann, "model_export", recipe="recipes/tombstone.json")
        check(blocked.get("error") in ("claimed", "claim_conflict") and not (assets / "models" / "exports" / "tombstone.glb").exists(), f"another agent's claim on the folder refuses the write ({blocked.get('error')})")
        same = refused(ann, "model_export", recipe="recipes/statue_angel.json")
        check(same.get("error") in ("claimed", "claim_conflict") and "unchanged" not in same, f"under that claim an export that would change nothing is refused too, as a write would be ({same.get('error')})")
        bob.json("claim_release", all=True)
        wrong_place = refused(ann, "model_export", recipe="recipes/tombstone.json", path="assets/elsewhere/tombstone.glb")
        check(wrong_place.get("error") == "not_a_model_path", f"a path that is no model file of the project is refused before anything is built ({wrong_place.get('error')})")
        gltf = refused(ann, "model_export", recipe="models/exports/statue_angel.glb")
        check("glTF file already" in gltf.get("message", ""), "a .glb is not exported again: it is copied")
        lod = refused(ann, "model_export", recipe="recipes/statue_angel.json", lod=9)
        check(lod.get("error") == "no_such_lod", f"an LOD the model does not have is refused, with how many it has ({lod.get('error')})")
        # world_undo takes an export back like any project write: the file goes.
        made = ann.json("model_export", recipe="recipes/tombstone.json")
        tomb = assets / "models" / "exports" / "tombstone.glb"
        check(tomb.is_file() and made.get("version", 0) >= 1, f"with the claim released the export is written (version {made.get('version')}, {made.get('bytes')} bytes)")
        ann.json("world_undo")
        end = time.time() + 10
        while tomb.exists() and time.time() < end:
            time.sleep(0.1)
        check(not tomb.exists(), "world_undo takes the export back: the file is gone from the project")
    check(eng.validation_errors() == 0, f"the engine shut down cleanly with no validation errors ({eng.validation_errors()})")

    # ---- solid surfaces under MASK materials, on every adapter there is ----
    planes = {}
    for adapter in dict.fromkeys([a.adapter, "integrated", "discrete"]):
        seen = solid_planes(exe, out, adapter, a.port + 1)
        if seen is not None:
            planes[adapter] = {f"{name} {view}": px for (name, view), px in seen.items()}
    check(len(planes) >= 1, f"the solid planes were drawn on {len(planes)} adapter(s): {', '.join(planes)}")
    report["planes"] = planes
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"\nreport: {out / 'report.json'}\npicture: {out / 'original_and_exported.png'}")
    if FAILS:
        print(f"FAIL: {len(FAILS)} check(s) failed")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())

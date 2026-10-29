"""Live check of the disk footprint budget and binary assets through the protocol
(docs/design/DISK.md, docs/design/PROJECT_DATA.md "Binary assets").

Three headless engines on a scratch folder (never anything else on the machine):

1. Budget: a 2 MB captures cap ([disk] in a scratch perf.toml). An old finished capture run is
   seeded with files (oldest first), one of them named by a review submission, plus a junction
   inside the run that points at a "precious" folder outside the engine's roots. Sight frames push
   the captures past the soft line: the answer to the write that crossed it carries a structured
   disk_warning, once per session. The walker (and disk_prune) then prune the oldest captures
   first; the reviewed capture and everything behind the junction survive. A scratch lease
   released long ago is cleaned up; one released with keep: true stays. disk_status and
   engine_info.disk report it all.
2. The floor: a second engine whose free_floor_mb is 10 GB above the real free space. Agents'
   writes are refused with the numbers and what to delete (project_upload, lease_take scratch,
   save_write, logic_build); a sight frame still answers its image inline, with the refusal.
3. Binary assets: a PNG uploaded in chunks, interrupted (the session closes mid-upload) and resumed
   from next_offset by a new session, then committed; a bad offset and a bad chunk CRC refused;
   project_read metadata and bytes; a second version, then world_undo puts the first version's
   bytes back on disk; an outside edit (a human saving a PNG) is imported as `disk`; a damaged PNG
   is refused by its check; a WAV and a GLB (a cube) uploaded in one call each, the GLB built with
   model_build and drawn (a capture of the uploaded model).

Usage: cargo build -p specula && python scripts/disk_check.py [--port 7620] [--adapter integrated]
       [--exe PATH] [--out DIR]
Output (captures, logs, the scratch folders) goes to --out (default target/disk-check). Standard
library only; output is ASCII.
"""
import argparse
import base64
import io
import json
import os
import shutil
import struct
import subprocess
import sys
import time
import wave
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine, ROOT  # noqa: E402

FAILS = []
MB = 1 << 20


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


def body(result):
    return json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))


def raw(mcp, tool, **args):
    """(is_error, answer) without raising."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    return bool(result.get("isError")), body(result)


def png(w, h, rgb):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    rows = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def noisy_png(w, h, seed):
    """A PNG that doesn't compress (so chunked uploads have real bytes to move)."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    x = seed
    rows = bytearray()
    for _ in range(h):
        rows.append(0)
        for _ in range(w * 3):
            x = (x * 1103515245 + 12345) & 0x7FFFFFFF
            rows.append((x >> 16) & 0xFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(rows), 1)) + chunk(b"IEND", b"")


def wav_bytes(seconds=0.25, rate=22050):
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        frames = bytearray()
        for i in range(int(seconds * rate)):
            v = int(8000 * (((i * 440 * 2 // rate) % 2) * 2 - 1))
            frames += struct.pack("<h", v)
        w.writeframes(bytes(frames))
    return buf.getvalue()


def cube_glb(color=(0.25, 0.75, 0.85)):
    faces = [((1, 0, 0), (0, 1, 0), (0, 0, 1)), ((-1, 0, 0), (0, 1, 0), (0, 0, -1)), ((0, 1, 0), (0, 0, 1), (1, 0, 0)),
             ((0, -1, 0), (0, 0, -1), (1, 0, 0)), ((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0))]
    pos, nrm, idx = [], [], []
    for n, u, v in faces:
        base = len(pos)
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            pos.append(tuple(0.5 * n[k] + 0.5 * su * u[k] + 0.5 * sv * v[k] for k in range(3)))
            nrm.append(n)
        # Counter-clockwise seen from outside: u x v = n for each face above.
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    pb = b"".join(struct.pack("<3f", *p) for p in pos)
    nb = b"".join(struct.pack("<3f", *n) for n in nrm)
    ib = b"".join(struct.pack("<H", i) for i in idx)
    ib += b"\x00" * ((4 - len(ib) % 4) % 4)
    binary = pb + nb + ib
    doc = {
        "asset": {"version": "2.0", "generator": "disk_check.py"},
        "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0, "name": "cube"}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [*color, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.7}}],
        "buffers": [{"byteLength": len(binary)}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": len(pb), "target": 34962},
                        {"buffer": 0, "byteOffset": len(pb), "byteLength": len(nb), "target": 34962},
                        {"buffer": 0, "byteOffset": len(pb) + len(nb), "byteLength": len(idx) * 2, "target": 34963}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": len(pos), "type": "VEC3", "min": [-0.5] * 3, "max": [0.5] * 3},
                      {"bufferView": 1, "componentType": 5126, "count": len(nrm), "type": "VEC3"},
                      {"bufferView": 2, "componentType": 5123, "count": len(idx), "type": "SCALAR"}],
    }
    j = json.dumps(doc).encode()
    j += b" " * ((4 - len(j) % 4) % 4)
    total = 12 + 8 + len(j) + 8 + len(binary)
    return struct.pack("<4sII", b"glTF", 2, total) + struct.pack("<I4s", len(j), b"JSON") + j + struct.pack("<I4s", len(binary), b"BIN\x00") + binary


def junction(link, target):
    if sys.platform != "win32":
        try:
            os.symlink(target, link, target_is_directory=True)
            return True
        except OSError:
            return False
    r = subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(target)], capture_output=True)
    return r.returncode == 0


def in_evidence(ref, world_dir):
    """The file a review reference names, when it is the world's evidence (`evidence/<file>`,
    relative to the world: no home paths in world files, and a copied world keeps its evidence)."""
    if not isinstance(ref, str) or not ref.startswith("evidence/") or "/" in ref[len("evidence/"):]:
        return None
    return world_dir / "evidence" / ref[len("evidence/"):]


def same_dir(a, b):
    """Whether two folders are one (the engine answers verbatim \\\\?\\ paths)."""
    try:
        return os.path.samefile(a, b)
    except OSError:
        return False


def remove_tree(d):
    with os.scandir(d) as it:
        for e in it:
            if e.is_symlink() or e.is_junction():
                # A link to a folder (junction or directory symlink) is removed with rmdir: the link, not its target.
                (os.rmdir if e.is_junction() or os.path.isdir(e.path) else os.unlink)(e.path)
            elif e.is_dir(follow_symlinks=False):
                remove_tree(e.path)
            else:
                os.unlink(e.path)
    os.rmdir(d)


def write_perf(path, disk):
    lines = ["[disk]"] + [f"{k} = {v}" for k, v in disk.items()]
    path.write_text("\n".join(lines) + "\n")


def engine_args(work, perf, adapter):
    return ["--headless", "--adapter", adapter, "--captures", str(work / "captures"), "--cache", str(work / "cache"),
            "--assets", str(work / "assets"), "--worlds", str(work / "worlds"), "--saves", str(work / "saves"), "--perf", str(perf)]


def phase_budget(exe, port, adapter, work, out):
    print("== 1. budget, warnings, pruning order, protection, scratch cleanup")
    caps = work / "captures"
    old = caps / "run-1000-4000000003"
    old.mkdir(parents=True)
    precious = work / "precious"
    precious.mkdir()
    (precious / "thesis.docx").write_bytes(b"\x07" * 4096)
    seeded = []
    for i in range(4):
        p = old / f"old_{i}.png"
        p.write_bytes(noisy_png(190, 175, i + 1))  # ~100 KB each, real PNGs (a review names one)
        t = time.time() - 3600 * (10 - i)  # old_0 oldest
        os.utime(p, (t, t))
        seeded.append(p)
    made_junction = junction(old / "link_out", precious)
    check(made_junction, "a junction inside a capture run points outside the engine's roots")
    # Scratch: one lease released two hours ago, one kept.
    scratch = caps / "scratch"
    # Named as lease folders are (<owner>-<lease>-<secs>-<pid>), with markers saying the same; and
    # a forged marker in a folder of someone else's, which must never get it removed.
    for name, lease, keep in (("ann-1-1000-1", 1, False), ("ann-2-1000-1", 2, True), ("their-results", 3, False)):
        d = scratch / name
        d.mkdir(parents=True)
        (d / "out.bin").write_bytes(b"\x01" * 50_000)
        (d / ".specula-lease").write_text(json.dumps({"lease": lease, "owner": "ann", "ends_unix": int(time.time()) - 7200, "keep": keep, "pid": 1}))
    perf = work / "perf_budget.toml"
    write_perf(perf, {"captures_mb": 1, "free_floor_mb": 0, "warn_percent": 50, "scratch_grace_s": 60})
    with Engine(exe, port, out / "engine_budget.log", *engine_args(work, perf, adapter), cwd=str(ROOT)) as eng:
        lead = eng.mcp("disk-lead")
        lead.hello("lead", role="lead")
        a = eng.mcp("disk-ann")
        a.hello("ann")
        st = lead.json("disk_status", detailed=True, measure=True)
        print("   disk_status:", json.dumps({k: st.get(k) for k in ("state", "budget")}))
        check("categories" in st and "captures" in st["categories"], "disk_status reports the engine's use by category")
        vols = st.get("volumes") or []
        check(bool(vols) and vols[0].get("available_mb", 0) > 0, f"free space per volume from the OS ({vols[:1]})")
        info = lead.json("engine_info", response_format="detailed")
        check("disk" in info and "journal_binary_history_mb" in info["disk"], "engine_info.disk is there")
        # Startup pruning: the engine's first walk over the 2 MB cap already pruned.
        time.sleep(1.5)
        # A review names old_3 (the newest seeded one): never pruned.
        a.json("world_spawn", shape="group", name="staging")
        a.json("world_spawn", shape="cube", name="crate", parent="/staging")
        sub = a.json("review_submit", title="Crate", kind="asset", paths=["/staging/crate"], captures=[str(seeded[3])])
        check("id" in sub or "submission" in sub, f"a review submission names {seeded[3].name}")
        time.sleep(1.5)  # the engine's disk pump hands the review's files to the book
        warned = None
        for i in range(3):
            j = body(a.call("sight_frame", width=320, height=240))
            if "disk_warning" in j:
                warned = j["disk_warning"]
        check(warned is None, "small captures stay under the soft line: no warning")
        # One big capture (a baked texture strip saved to the captures) crosses it, and the cap.
        t = body(a.call("procgen_texture", file="recipes/tex_crypt_stone.json", size=256, save=True, name="crypt_stone"))
        big = Path(t.get("saved", ""))
        print(f"   saved {big.name}: {big.stat().st_size // 1024 if big.exists() else 0} KB")
        warned = t.get("disk_warning")
        check(warned is not None, f"the write that crossed the soft line answered a disk_warning ({warned and warned.get('message')})")
        if warned:
            check(warned.get("code") == "disk_budget" and "captures" in (warned.get("crossed") or []) and warned.get("biggest"),
                  "the warning is structured: code, what crossed, the biggest categories with what to do, a hint")
        j = body(a.call("sight_frame", width=64, height=64))
        check("disk_warning" not in j, "a session is told once")
        b = eng.mcp("disk-bob")
        jb = b.hello("bob")
        check("disk_warning" in jb, "every session hears it (bob's first call)")
        # More big captures push past the 1 MB cap: the walker prunes, oldest first.
        for n in range(5):
            body(a.call("procgen_texture", file="recipes/tex_crypt_stone.json", size=256, save=True, name=f"crypt_stone_{n}"))
        print("   captures now:", json.dumps(lead.json("disk_status")["categories"]["captures"]))
        dry = lead.json("disk_prune", dry_run=True)
        print("   dry run:", json.dumps({k: dry.get(k) for k in ("needed_mb", "files", "mb")}))
        check(all(p.exists() for p in seeded[1:]), "a dry run deletes nothing")
        # The walker prunes on its own (within its cooldown); disk_prune does the same at once.
        deadline = time.time() + 45
        while time.time() < deadline and seeded[0].exists():
            time.sleep(1)
        check(not seeded[0].exists(), "over budget, the walker pruned the oldest capture first")
        r = lead.json("disk_prune")
        print("   prune:", json.dumps({k: r.get(k) for k in ("needed_mb", "files", "mb", "by_category", "state")}))
        check(seeded[3].exists(), "the capture a review submission names is never pruned")
        check((precious / "thesis.docx").exists(), "nothing behind the junction was touched")
        check(not (scratch / "ann-1-1000-1").exists(), "a scratch folder whose lease ended past its grace period is cleaned up")
        check((scratch / "ann-2-1000-1").exists(), "one released with keep: true stays")
        check((scratch / "their-results").exists(), "a forged marker in a folder not named as a lease removes nothing")
        # Leases made now carry markers.
        lease = a.json("lease_take", kind="scratch", note="disk check")
        d = Path(lease["dir"])
        check((d / ".specula-lease").is_file(), "a new scratch lease carries its marker")
        rel = a.json("lease_release", id=lease["id"], delete=True)
        check(not d.exists() and rel.get("deleted_dirs"), "lease_release {delete: true} removes it")
        e, forced = raw(a, "disk_prune", free_mb=1)
        check(e and forced.get("error") == "lead_only", "only the lead may free more than the budget needs")
        # The real disk_prune above started the agents' 10 s wait; dry runs don't wait or start it.
        e, again = raw(lead, "disk_prune", dry_run=True)
        e2, again2 = raw(lead, "disk_prune")
        check(not e and e2 and again2.get("error") == "too_soon" and again2.get("retry_after_s"),
              "a real prune starts the agents' 10 s wait (too_soon, retry_after_s); a dry run doesn't")
        st2 = lead.json("disk_status", detailed=True)
        check(st2.get("pruned", {}).get("files", 0) > 0 and st2.get("prune_log"), "disk_status records what was pruned and why")
        print("   after:", json.dumps({"state": st2.get("state"), "captures": st2["categories"]["captures"], "pruned": st2.get("pruned")}))
        (out / "status_budget.json").write_text(json.dumps(st2, indent=2))
        for m in (lead, a, b):
            m.close()
    return vols[0]["available_mb"] if vols else None


def phase_floor(exe, port, adapter, work, out, available_mb):
    print("== 2. refused below the free-space floor")
    perf = work / "perf_floor.toml"
    floor = int(available_mb) + 10240
    write_perf(perf, {"free_floor_mb": floor})
    with Engine(exe, port + 1, out / "engine_floor.log", *engine_args(work, perf, adapter), cwd=str(ROOT)) as eng:
        a = eng.mcp("disk-floor")
        a.hello("ann")
        e, up = raw(a, "project_upload", path="assets/textures/floor.png", data=base64.b64encode(png(8, 8, (9, 9, 9))).decode())
        check(e and up.get("error") == "disk_floor", f"project_upload refused: {up.get('error')}")
        check(up.get("floor_mb") == floor and "available_mb" in up and up.get("what_to_delete") is not None, "with the numbers and what to delete")
        e, ls = raw(a, "lease_take", kind="scratch")
        check(e and ls.get("error") == "disk_floor", f"lease_take scratch refused: {ls.get('error')}")
        e, sv = raw(a, "save_write", name="floor")
        check(e and sv.get("error") == "disk_floor", f"save_write refused: {sv.get('error')}")
        e, lb = raw(a, "logic_build", path="games/ambulo", check=True)
        check(e and lb.get("error") == "disk_floor", f"logic_build refused before Cargo runs: {lb.get('error')}")
        # Review finding: world_save to new names was essential, so it went through the floor.
        a.json("world_spawn", shape="cube", name="box")
        for n in ("copy1", "copy2"):
            e, ws = raw(a, "world_save", name=n)
            check(e and ws.get("error") == "disk_floor" and not (work / "worlds" / n / "world.json").exists(), f"world_save {n} refused: {ws.get('error')}")
        f = a.call("sight_frame", width=64, height=64)
        j = body(f)
        warns = " ".join(str(w) for w in j.get("warnings", []))
        check(any(c["type"] == "image" for c in f["content"]) and "disk_floor" in warns and j["image"].get("path") is None,
              "a sight frame still answers its image inline, unsaved, saying why")
        st = a.json("disk_status")
        check(st.get("state") == "below_floor" and st.get("refused", 0) >= 4, f"disk_status: {st.get('state')}, refused {st.get('refused')}")
        (out / "status_floor.json").write_text(json.dumps(st, indent=2))
        a.close()


def upload_chunked(mcp, path, data, chunk=48 * 1024, stop_after=None, expect=None):
    args = dict(path=path, bytes=len(data), crc32=f"{zlib.crc32(data) & 0xFFFFFFFF:08x}")
    if expect is not None:
        args["expect_version"] = expect
    u = mcp.json("project_upload", **args)
    uid, off, sent = u["upload"], 0, 0
    while off < len(data):
        if stop_after is not None and sent >= stop_after:
            return uid, off
        piece = data[off:off + chunk]
        r = mcp.json("project_upload", upload=uid, offset=off, data=base64.b64encode(piece).decode(), chunk_crc32=zlib.crc32(piece) & 0xFFFFFFFF)
        off = r["next_offset"]
        sent += 1
    return uid, off


def phase_binary(exe, port, adapter, work, out):
    print("== 3. binary assets through the protocol")
    perf = work / "perf_binary.toml"
    write_perf(perf, {"free_floor_mb": 0})
    assets = work / "assets"
    with Engine(exe, port + 2, out / "engine_binary.log", *engine_args(work, perf, adapter), cwd=str(ROOT)) as eng:
        a = eng.mcp("disk-up-1")
        a.hello("ann", token_file=str(out / "ann.token"))
        v1 = noisy_png(160, 120, 7)
        path = "assets/textures/stone.png"
        uid, off = upload_chunked(a, path, v1, chunk=16 * 1024, stop_after=2)
        check(0 < off < len(v1), f"upload interrupted after {off} of {len(v1)} bytes")
        a.close()  # the connection drops mid-upload
        a = eng.mcp("disk-up-2")
        a.hello("ann", token_file=str(out / "ann.token"))
        st = a.json("project_upload", upload=uid)
        check(st["next_offset"] == off and not st["complete"], f"a new session resumes from next_offset {st['next_offset']}")
        e, bad = raw(a, "project_upload", upload=uid, offset=0, data=base64.b64encode(v1[:100]).decode())
        check(e and bad.get("error") == "bad_offset" and bad.get("next_offset") == off, "a chunk at the wrong offset is refused with where to resume")
        e, badcrc = raw(a, "project_upload", upload=uid, offset=off, data=base64.b64encode(v1[off:off + 100]).decode(), chunk_crc32=1)
        check(e and badcrc.get("error") == "bad_crc", "a damaged chunk (CRC-32) is refused")
        while off < len(v1):
            piece = v1[off:off + 48 * 1024]
            off = a.json("project_upload", upload=uid, offset=off, data=base64.b64encode(piece).decode())["next_offset"]
        done = a.json("project_upload", upload=uid, commit=True)
        print("   committed:", json.dumps({k: done.get(k) for k in ("path", "version", "journal", "bytes", "disk", "info")}))
        check(done.get("version") == 1 and done.get("disk") == "written", "committed: version 1, written to disk")
        check((assets / "textures" / "stone.png").read_bytes() == v1, "the file on disk is the uploaded bytes")
        check(done.get("info", {}).get("width") == 160, "checked by kind: the PNG decoded (160x120)")
        rd = a.json("project_read", path=path)
        check(rd.get("kind") == "png" and rd.get("bytes") == len(v1) and rd.get("info", {}).get("height") == 120 and "data_base64" not in rd,
              "project_read answers metadata by default")
        chunk = a.json("project_read", path=path, offset=1000, length=4096)
        check(base64.b64decode(chunk["data_base64"]) == v1[1000:5096] and chunk.get("next_offset") == 5096, "and bytes on request, chunked")
        lst = a.json("project_list", kind="png")
        check(any(f["path"] == path and f["bytes"] == len(v1) for f in lst["files"]), "project_list lists it with its byte size")
        hist = a.json("world_history", response_format="detailed")
        text = json.dumps(hist)
        check("file_create" in text and len(text) < 64 * 1024, "journaled (a pointer, never the bytes)")
        # Version 2, then undo puts version 1's bytes back on disk.
        v2 = noisy_png(160, 120, 99)
        uid2, _ = upload_chunked(a, path, v2, expect=1)
        a.json("project_upload", upload=uid2, commit=True)
        rd2 = a.json("project_read", path=path, problems=False)
        check(rd2.get("version") == 2 and (assets / "textures" / "stone.png").read_bytes() == v2, "version 2 written")
        e, stale = raw(a, "project_upload", path=path, data=base64.b64encode(v1).decode(), expect_version=1)
        check(e and stale.get("error") == "stale_version" and "conflict" in stale, "a stale expect_version is refused with conflict help")
        u = a.json("world_undo", count=1)
        time.sleep(0.5)
        check((assets / "textures" / "stone.png").read_bytes() == v1, f"world_undo put version 1's bytes back on disk ({u.get('undone', u)})")
        old = a.json("project_read", path=path, version=2, offset=0, length=64)
        check(base64.b64decode(old["data_base64"]) == v2[:64], "project_read {version} reads an earlier version's bytes")
        # A human saves a new PNG over it: imported as `disk`.
        human = png(32, 32, (200, 40, 40))
        time.sleep(0.3)
        (assets / "textures" / "stone.png").write_bytes(human)
        seen = None
        for _ in range(30):
            r = a.json("project_read", path=path)
            if r.get("modified_by") == "disk":
                seen = r
                break
            time.sleep(0.2)
        check(seen is not None and seen.get("info", {}).get("width") == 32, f"an outside edit was imported as disk ({seen and seen.get('version')})")
        # A damaged PNG is refused by its check; nothing changes.
        bad_png = bytearray(png(16, 16, (1, 2, 3)))
        bad_png[-20] ^= 0xFF
        e, inv = raw(a, "project_upload", path="assets/textures/broken.png", data=base64.b64encode(bytes(bad_png)).decode())
        check(e and inv.get("error") == "invalid" and inv.get("problems"), f"a damaged PNG is refused: {inv.get('problems', [{}])[0].get('code')}")
        check(not (assets / "textures" / "broken.png").exists(), "and nothing reached the disk")
        e, txt = raw(a, "project_write", path=path, text="hello")
        check(e and txt.get("error") == "binary_file", "project_write refuses binary files (project_upload writes them)")
        # Review findings: another agent could list, abort or commit an upload; every metadata read
        # decoded the whole file; damaged content could never be repaired.
        b = eng.mcp("disk-up-bob")
        b.hello("bob")
        mine = a.json("project_upload", path="assets/textures/mine.png", bytes=len(v1), crc32=zlib.crc32(v1) & 0xFFFFFFFF)
        check(not b.json("disk_status").get("uploads") and a.json("disk_status").get("uploads"), "disk_status lists only the caller's uploads")
        e, ab = raw(b, "project_upload", upload=mine["upload"], abort=True)
        e2, cm = raw(b, "project_upload", upload=mine["upload"], offset=0, data=base64.b64encode(v1[:10]).decode())
        check(e and e2 and ab.get("error") == "upload_not_found" and cm.get("error") == "upload_not_found", "another agent can't abort or continue it")
        a.json("project_upload", upload=mine["upload"], abort=True)
        t0 = time.time()
        a.json("project_read", path=path)
        first = time.time() - t0
        t1 = time.time()
        for _ in range(3):
            r = b.json("project_read", path=path)
        again = (time.time() - t1) / 3
        print(f"   metadata read: first {first * 1000:.0f} ms, then {again * 1000:.0f} ms each")
        check(r.get("info", {}).get("width") == 32 and again < 0.2, "metadata reads answer from the checks kept per content")
        small = png(8, 8, (5, 5, 5))
        r = a.json("project_upload", path="assets/textures/small.png", data=base64.b64encode(small).decode())
        runs = list((work / "cache" / "content").glob("run-*"))
        blob = next(rr / r["content"] for rr in runs if (rr / r["content"]).exists())
        blob.write_bytes(small[:-1] + b"X")
        e, dmg = raw(b, "project_read", path="assets/textures/small.png", problems=False, bytes=True)
        r2 = a.json("project_upload", path="assets/textures/small2.png", data=base64.b64encode(small).decode())
        check(r2.get("content") == r["content"] and blob.read_bytes() == small, "damaged content is repaired by the next upload of the same bytes")
        b.close()
        # WAV and GLB in one call each; the GLB builds through model_build and draws.
        w = wav_bytes()
        wv = a.json("project_upload", path="assets/sounds/bell.wav", data=base64.b64encode(w).decode())
        check(wv.get("info", {}).get("rate") == 22050, f"a WAV uploaded and checked ({wv.get('info')})")
        g = cube_glb()
        gl = a.json("project_upload", path="assets/models/cube.glb", data=base64.b64encode(g).decode())
        check(gl.get("info", {}).get("meshes") == 1, f"a GLB uploaded and imported ({gl.get('info')})")
        mb = a.json("model_build", recipe="models/cube.glb")
        check("error" not in mb, f"model_build loads the uploaded GLB through the model paths ({str(mb)[:120]})")
        a.json("world_spawn", shape="model", recipe="models/cube.glb", name="cube", position=[0, 0.5, 0], rotation_deg=[0, 30, 0])
        a.json("world_spawn", shape="cube", name="ground", position=[0, -0.05, 0], scale=[6, 0.1, 6], color=[0.35, 0.33, 0.3])
        f = a.call("sight_frame", width=640, height=420, eye=[2.2, 1.8, 2.6], target=[0, 0.4, 0])
        j = body(f)
        cap = j.get("image", {}).get("path")
        check(cap and Path(cap).exists(), f"captured the uploaded model: {cap}")
        if cap:
            shutil.copyfile(cap, out / "uploaded_cube.png")
            pk = Path(cap).with_suffix(".json")
            if pk.exists():
                shutil.copyfile(pk, out / "uploaded_cube.json")
        e, gone = raw(a, "project_delete", path="assets/sounds/bell.wav")
        check(not e and not (assets / "sounds" / "bell.wav").exists(), "project_delete removes a binary file on disk")
        st = a.json("disk_status", detailed=True)
        check(st.get("content_store", {}).get("entries", 0) >= 3, f"the content store holds the versions ({st.get('content_store')})")
        (out / "status_binary.json").write_text(json.dumps(st, indent=2))
        a.close()


def phase_restarts(exe, port, adapter, out):
    """Review probes A, B and C: protection across a restart at a low floor, the 20-run retention,
    and an engine over budget only because of what is never pruned."""
    print("== 4. restarts: what a review names survives; over budget for nothing prunes nothing")
    w = out / "work_restart"
    if w.exists():
        remove_tree(w)
    for d in ("assets", "captures", "cache", "worlds", "saves"):
        (w / d).mkdir(parents=True)
    old = w / "captures" / "run-1000-4000000005"
    old.mkdir(parents=True)
    oldpng = old / "reviewed_old.png"
    oldpng.write_bytes(png(16, 16, (1, 2, 3)))
    t = time.time() - 7200
    os.utime(oldpng, (t, t))
    p0 = w / "p0.toml"
    write_perf(p0, {"free_floor_mb": 0})
    with Engine(exe, port, out / "engine_restart1.log", *engine_args(w, p0, adapter), "--world", "revw", cwd=str(ROOT)) as eng:
        a = eng.mcp("disk-rev")
        a.hello("ann")
        a.json("world_spawn", shape="group", name="staging")
        a.json("world_spawn", shape="cube", name="crate", parent="/staging")
        frame = Path(body(a.call("sight_frame", width=64, height=64))["image"]["path"])
        a.json("review_submit", title="Crate", kind="asset", paths=["/staging/crate"], captures=[str(oldpng), str(frame)])
        a.close()
    check((w / "worlds" / "revw" / "review.json").exists(), "the review queue was saved with the world")
    # B: twenty newer finished runs: the start-up retention keeps the reviewed run whole.
    now = int(time.time())
    for k in range(20):
        (w / "captures" / f"run-{now - 100 + k}-{4000000100 + k}").mkdir()
    # A: restart on the same world with free space under twice the floor (pruning wanted at once).
    free = shutil.disk_usage(str(w)).free >> 20
    p1 = w / "p1.toml"
    write_perf(p1, {"free_floor_mb": int(free * 0.6), "capture_min_age_s": 0})
    with Engine(exe, port + 1, out / "engine_restart2.log", *engine_args(w, p1, adapter), "--world", "revw", cwd=str(ROOT)) as eng:
        b = eng.mcp("disk-rev2")
        b.hello("bob")
        time.sleep(4)
        st = b.json("disk_status", detailed=True)
        print("   restart:", json.dumps({"state": st.get("state"), "pruned": st.get("pruned")}))
        sub = b.json("review_list", id=1)
        named = [in_evidence(c["path"], w / "worlds" / "revw") for c in sub["captures"]]
        b.close()
    check(len(named) == 2 and all(p is not None and p.exists() for p in named),
          "after a restart at a low floor (and the run retention), both captures the pending review names are there, in the world's evidence")
    # C: over budget only because of world files: a fresh capture is never wiped.
    big = w / "worlds" / "big.bin"
    big.write_bytes(bytes(30 << 20))
    p2 = w / "p2.toml"
    write_perf(p2, {"free_floor_mb": 0, "total_mb": 20})
    with Engine(exe, port + 2, out / "engine_restart3.log", *engine_args(w, p2, adapter), cwd=str(ROOT)) as eng:
        c = eng.mcp("disk-over")
        c.hello("cat")
        fresh = Path(body(c.call("sight_frame", width=64, height=64))["image"]["path"])
        time.sleep(40)
        st = c.json("disk_status", detailed=True)
        check(fresh.exists(), f"over budget because of world files, a fresh capture survives 40 s (state {st.get('state')})")
        check(st.get("warning") is not None, "the engine warns instead")
        c.close()
    big.unlink()


def phase_evidence(exe, port, adapter, out):
    """Review evidence (round-2 review): submissions and critiques keep copies of their captures in
    the world's folder, so a world_load, another world's prune and the start-up run retention
    never take a reviewed capture away; old queues are migrated on load."""
    print("== 5. review evidence lives with the world")
    w = out / "work_evidence"
    if w.exists():
        remove_tree(w)
    for d in ("assets", "captures", "cache", "worlds", "saves"):
        (w / d).mkdir(parents=True)
    p0 = w / "p0.toml"
    write_perf(p0, {"free_floor_mb": 0, "capture_min_age_s": 0})
    ev_a = w / "worlds" / "alpha" / "evidence"
    with Engine(exe, port, out / "engine_ev1.log", *engine_args(w, p0, adapter), "--world", "alpha", cwd=str(ROOT)) as eng:
        lead = eng.mcp("ev-lead")
        lead.hello("lead", role="lead")
        a = eng.mcp("ev-ann")
        a.hello("ann")
        a.json("world_spawn", shape="group", name="staging")
        a.json("world_spawn", shape="cube", name="crate", parent="/staging")
        f = body(a.call("sight_frame", width=96, height=64))
        frame = Path(f["image"]["path"])
        packet = Path(f["image"]["packet_path"])
        s1 = a.json("review_submit", title="Crate", kind="asset", paths=["/staging/crate"], captures=[{"path": str(frame), "packet": str(packet)}])
        s2 = a.json("review_submit", title="Crate again", kind="asset", paths=["/staging/crate"], captures=[str(frame)], supersedes=s1["id"])
        crit = a.json("critique_add", items=[{"text": "too plain", "severity": "warning", "capture": str(frame), "path": "/staging/crate"}])
        sub = a.json("review_list", id=s2["id"])
        kept_ref = sub["captures"][0]["path"]
        kept = in_evidence(kept_ref, w / "worlds" / "alpha")
        check(kept is not None and kept.read_bytes() == frame.read_bytes(), f"the submission names a copy in the world's evidence folder, relative to the world ({kept_ref})")
        check(len([x for x in ev_a.iterdir() if x.suffix == ".png"]) == 1, "the same PNG submitted twice (and critiqued) is one file")
        e, bad = raw(a, "review_submit", title="x", kind="asset", paths=["/staging/crate"], captures=[str(ROOT / "docs" / "gallery" / "README.md")])
        check(e and bad.get("error") == "path_not_allowed", f"evidence must be a capture: {bad.get('error')}")
        # Round-3 review: never a world's own files, never a UNC name (refused before any lookup).
        for name in [str(w / "worlds" / "alpha" / "world.json"), r"\\nohost\share\x.png"]:
            e, bad = raw(a, "review_submit", title="x", kind="asset", paths=["/staging/crate"], captures=[name])
            check(e and bad.get("error") == "path_not_allowed", f"refused as evidence: {name[-30:]} ({bad.get('error')})")
        # Another world: alpha's capture source is pruned there, alpha's evidence is not.
        lead.json("world_save", name="alpha")
        a.close()
        lead.close()
    check((w / "worlds" / "alpha" / "review.json").exists(), "alpha's queue was saved")
    # Old queues pointed into the captures: rewrite alpha's to do so, as a pre-evidence queue would.
    old_run = w / "captures" / "run-1000-4000000009"
    old_run.mkdir(parents=True)
    legacy = old_run / "legacy.png"
    legacy_bytes = png(12, 12, (9, 8, 7))
    legacy.write_bytes(legacy_bytes)
    gone = old_run / "gone.png"  # named by the old queue, deleted before the load
    rj = w / "worlds" / "alpha" / "review.json"
    doc = json.loads(rj.read_text())
    for sub_ in doc["submissions"]:
        for c in sub_["captures"]:
            c["path"] = str(legacy)
            c.pop("packet", None)
    for c in doc["critiques"]:
        c["capture"] = str(gone)
    rj.write_text(json.dumps(doc, indent=2))
    # Twenty newer finished runs: the start-up retention removes the legacy run.
    now = int(time.time())
    for k in range(20):
        (w / "captures" / f"run-{now - 100 + k}-{4000000200 + k}").mkdir()
    with Engine(exe, port + 1, out / "engine_ev2.log", *engine_args(w, p0, adapter), "--world", "alpha", cwd=str(ROOT)) as eng:
        a = eng.mcp("ev-ann2")
        a.hello("ann")
        sub = a.json("review_list", id=s2["id"])
        migrated = in_evidence(sub["captures"][0]["path"], w / "worlds" / "alpha")
        check(migrated is not None and migrated.exists() and migrated.read_bytes() == legacy_bytes, "an old queue's capture was copied into the evidence on load (migration)")
        crits = a.json("critique_list")
        check(any(c.get("capture_missing") for c in crits.get("critiques", [])), "a reference whose file is gone is marked missing")
        check(not old_run.exists(), "the start-up run retention still removed the old run")
        # world_load into another world, then prune everything prunable: alpha's evidence stays.
        lead = eng.mcp("ev-lead2")
        lead.hello("lead2", role="lead")
        lead.json("world_save", name="beta")
        loaded = lead.json("world_load", name="alpha")
        check(any("missing" in str(x) for x in loaded.get("warnings", [])), f"world_load warns about missing review evidence ({loaded.get('warnings')})")
        time.sleep(1.5)
        r = lead.json("disk_prune", free_mb=4096)
        print("   prune:", json.dumps({k: r.get(k) for k in ("prunable_mb", "files", "mb")}))
        check(not frame.exists(), "the reviewed capture's source in the captures folder was pruned")
        sub = lead.json("review_list", id=s2["id"])
        ev = in_evidence(sub["captures"][0]["path"], w / "worlds" / "alpha")
        check(ev is not None and ev.exists() and not sub["captures"][0].get("missing"), "after world_load and a prune, the reviewed capture is still there (in alpha's evidence)")
        doc2 = json.loads((w / "worlds" / "alpha" / "review.json").read_text())
        refs = [c["path"] for s_ in doc2["submissions"] for c in s_["captures"]]
        check(refs and all(r.startswith("evidence/") for r in refs), f"the world's queue names its evidence relative to the world, no absolute paths ({refs[:2]})")
        a.close()
        lead.close()


def phase_robustness(exe, port, adapter, out):
    """Round-3 review probes: an unreadable queue never loses its evidence (P1), a copied world
    keeps its own (P2), and copying evidence never stalls the engine thread (P4/P5)."""
    print("== 6. evidence survives an unreadable queue and a copy; copies stay off the engine thread")
    w = out / "work_robust"
    if w.exists():
        remove_tree(w)
    for d in ("assets", "captures", "cache", "worlds", "saves"):
        (w / d).mkdir(parents=True)
    p0 = w / "p0.toml"
    write_perf(p0, {"free_floor_mb": 0, "capture_min_age_s": 0})
    with Engine(exe, port, out / "engine_rb1.log", *engine_args(w, p0, adapter), "--world", "w1", cwd=str(ROOT)) as eng:
        a = eng.mcp("rb-ann")
        a.hello("ann")
        a.json("world_spawn", shape="group", name="staging")
        frame = body(a.call("sight_frame", width=64, height=64))["image"]["path"]
        a.json("review_submit", title="Crate", kind="asset", paths=["/staging"], captures=[frame])
        a.json("world_save")
        a.close()
    wd = w / "worlds" / "w1"
    before = sorted(p.name for p in (wd / "evidence").iterdir())
    old = time.time() - 7200
    for p_ in (wd / "evidence").iterdir():
        os.utime(p_, (old, old))
    # P2: the world folder copied elsewhere keeps its evidence.
    w2 = w / "worlds2"
    shutil.copytree(wd, w2 / "w1")
    # P1: the queue becomes unreadable (a newer engine's, or damaged).
    doc = json.loads((wd / "review.json").read_text())
    doc["format"] = doc["format"] + "2"
    (wd / "review.json").write_text(json.dumps(doc))
    with Engine(exe, port + 1, out / "engine_rb2.log", *engine_args(w, p0, adapter), "--world", "w1", cwd=str(ROOT)) as eng:
        time.sleep(8)
    after = sorted(p.name for p in (wd / "evidence").iterdir())
    check(before and after == before and list(wd.glob("review.json.unreadable-*")), f"an unreadable queue set aside keeps all its evidence ({len(after)} of {len(before)})")
    args2 = engine_args(w, p0, adapter)
    args2[args2.index("--worlds") + 1] = str(w2)
    with Engine(exe, port + 2, out / "engine_rb3.log", *args2, "--world", "w1", cwd=str(ROOT)) as eng:
        b = eng.mcp("rb-bob")
        b.hello("bob")
        time.sleep(3)
        sub = b.json("review_list", id=1)
        ref = sub["captures"][0]["path"]
        f = in_evidence(ref, w2 / "w1")
        check(f is not None and f.exists() and not sub["captures"][0].get("missing"), f"a copied world folder names its own evidence ({ref})")
        b.close()
    # P4/P5: 32 critiques naming the same two 16 MiB files: copied once, off the engine thread.
    with Engine(exe, port + 3, out / "engine_rb4.log", *engine_args(w, p0, adapter), "--world", "big", cwd=str(ROOT)) as eng:
        a = eng.mcp("rb-ann2")
        a.hello("ann")
        run = Path(body(a.call("sight_frame", width=64, height=64))["image"]["path"]).parent
        big, big2 = run / "big.bin", run / "big2.bin"
        big.write_bytes(os.urandom(16 << 20))
        big2.write_bytes(os.urandom(16 << 20))
        b = eng.mcp("rb-bob2")
        b.hello("bob")
        lat, stop = [], [False]

        def poll():
            while not stop[0]:
                t0 = time.perf_counter()
                b.json("world_list")
                lat.append(time.perf_counter() - t0)
        import threading
        th = threading.Thread(target=poll)
        th.start()
        time.sleep(0.5)
        t0 = time.perf_counter()
        e, r = raw(a, "critique_add", items=[{"text": f"d{i}", "severity": "warning", "source": "ann", "packet": str(big), "capture": str(big2)} for i in range(32)])
        dt = time.perf_counter() - t0
        time.sleep(0.5)
        stop[0] = True
        th.join()
        print(f"   critique_add 32 x the same two 16 MiB files: {dt:.2f} s; another agent's world_list max {max(lat) * 1000:.0f} ms, median {sorted(lat)[len(lat) // 2] * 1000:.1f} ms (n={len(lat)})")
        check(not e and max(lat) < 1.0, "copying evidence never holds up another agent's call (max world_list under 1 s)")
        fresh3 = []
        for i in range(5):
            f3 = run / f"fresh{i}.bin"
            f3.write_bytes(os.urandom(15 << 20))
            fresh3.append(f3)
        e, r = raw(a, "critique_add", items=[{"text": f"x{i}", "severity": "warning", "source": "ann", "packet": str(f3)} for i, f3 in enumerate(fresh3)])
        check(e and r.get("error") == "evidence_budget", f"one call copies at most 64 MiB ({r.get('error')})")
        t0 = time.perf_counter()
        e, r = raw(a, "world_save", name="big2")
        print(f"   world_save under a new name (evidence copied on a worker): {time.perf_counter() - t0:.2f} s")
        check(not e, "world_save under a new name answers")
        a.close()
        b.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7620)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe")
    ap.add_argument("--out", default=str(ROOT / "target" / "disk-check"))
    a = ap.parse_args()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target_dir / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = Path(a.out)
    work = out / "work"
    # Only ever our own scratch folder (made by this script); links and junctions inside are
    # removed as links, never followed.
    if work.exists():
        remove_tree(work)
    for d in ("assets/textures", "assets/models", "assets/sounds", "assets/recipes", "captures", "cache", "worlds", "saves"):
        (work / d).mkdir(parents=True, exist_ok=True)
    sep = ROOT / "games" / "sepulcra" / "assets"
    shutil.copyfile(sep / "style.json", work / "assets" / "style.json")
    shutil.copyfile(sep / "recipes" / "tex_crypt_stone.json", work / "assets" / "recipes" / "tex_crypt_stone.json")
    out.mkdir(parents=True, exist_ok=True)
    available = phase_budget(exe, a.port, a.adapter, work, out)
    if available:
        phase_floor(exe, a.port, a.adapter, work, out, available)
    phase_binary(exe, a.port, a.adapter, work, out)
    phase_restarts(exe, a.port + 3, a.adapter, out)
    phase_evidence(exe, a.port + 6, a.adapter, out)
    phase_robustness(exe, a.port + 3, a.adapter, out)
    print()
    if FAILS:
        print(f"{len(FAILS)} check(s) failed:")
        for f in FAILS:
            print("  - " + f)
        sys.exit(1)
    print("disk check passed")


if __name__ == "__main__":
    main()

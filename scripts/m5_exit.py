#!/usr/bin/env python3
"""M5 exit proof (docs/ROADMAP.md, M5 "Exit proof"): assets are made once, saved, and are there
to be used again.

A project is made from a terminal (`specula new`), outside this repository. Agents put assets
into it through the tools alone, every one a project file: 3D (a prop recipe, a rigged creature
with its clips, the style they are built under), 2D (a tile sheet and a sprite sheet as PNGs, a
tile map and sprites that use them) and a sound. The world is saved and the engine stopped.
A second engine opens the same project and must find every asset as it was left: the saved
world loads to the same hash and draws the same pictures. Then the assets are reused: a second
world places them again, one agent (working from a terminal, with `specula call`) copies a
recipe and changes it, takes a change back with `world_undo`, a person's edit made on disk
comes in as a journaled change, and a model is written as a `.glb` that the engine reads back
with the same triangles. Every write names the agent that made it.

All of it runs on every adapter, headless, with zero validation messages. The pictures are
kept in --out for a person (and the sight-skeptic) to look at.

Usage: cargo build -p specula --release && python scripts/m5_exit.py [--exe PATH] [--port 7715]
       [--adapters integrated,discrete] [--out DIR]
Standard library only; ASCII output. Exit 0 = every check held.
"""
import argparse
import base64
import json
import os
import shutil
import struct
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
from launch_check import Cli  # noqa: E402
from specula_mcp import Engine  # noqa: E402
from two_d_check import hero, read_png, rows_2d, tileset  # noqa: E402

SEPULCRA = ROOT / "games" / "sepulcra" / "assets"
NAME = "hortus"
problems = []


def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what, flush=True)
    if not ok:
        problems.append(what)
    return ok


def source(rel):
    """An asset's text as another project keeps it (Sepulcra's): assets are data, brought in."""
    return (SEPULCRA / rel).read_text(encoding="utf-8")


def tool_error(fn, *a, **k):
    try:
        fn(*a, **k)
    except RuntimeError as e:
        return str(e)
    return None


def journal(m, limit=200):
    """The journal's entries as (who, kind, paths), newest first. `m` is a session that asks
    for concise answers: a concise entry lists the paths it changed."""
    return [(e.get("by"), e.get("kind"), list(e.get("paths") or [])) for e in m.json("world_history", limit=limit).get("entries", [])]


def picture_difference(a, b):
    """The mean difference of two PNGs' channels (0..255); None when their sizes differ."""
    (wa, ha, pa), (wb, hb, pb) = read_png(a), read_png(b)
    if (wa, ha) != (wb, hb):
        return None
    return sum(abs(x - y) for x, y in zip(pa, pb)) / max(1, len(pa))


def lit_share(path):
    """The share of a PNG's pixels that are not near black: a picture with something in it."""
    _, _, px = read_png(path)
    n = len(px) // 4
    return sum(1 for i in range(n) if px[i * 4] + px[i * 4 + 1] + px[i * 4 + 2] > 60) / max(1, n)


YARD = dict(width=960, height=600, eye=[4.2, 2.6, 5.4], target=[0.6, 1.0, 0.0], fov_y_deg=45, time_s=0.4)
# (32 pixels a metre: a 16-pixel tile is drawn at exactly twice its size, the hero at three times.)
BOARD = dict(width=1024, height=576, view_2d={"center": [400, 0], "height_m": 18}, time_s=0.0)
ANNEX = dict(width=960, height=600, eye=[104.5, 2.8, 5.6], target=[100.6, 1.1, 0.0], fov_y_deg=45, time_s=0.4)
# The creature alone, side on, for the pair of pictures that shows its clip.
WALKER = dict(width=480, height=480, eye=[-3.5, 1.0, 4.8], target=[-1.2, 0.9, 2.0], fov_y_deg=35, time_s=0.4)
# A candle's light, where the lantern's cage hangs (the recipe has no light of its own).
CANDLE = {"intensity": 5, "range": 6, "color": "#ffc98a", "offset": [0.55, 1.85, 0]}


def shot(m, out, name, **view):
    p = m.frame(**view)
    kept = out / f"{name}.png"
    shutil.copy(p["image"]["path"], kept)
    shutil.copy(p["image"]["packet_path"], out / f"{name}.json")
    return kept, p


def engine_args(adapter, project, settings, out):
    return ["--headless", "--adapter", adapter, "--project", str(project), "--settings", str(settings), "--lead", "director", "--trace", str(out / "trace")]


def first_session(exe, port, adapter, project, settings, out, cli):
    """Agents make the assets and the world; the engine is stopped."""
    art = out / "art"
    art.mkdir(parents=True, exist_ok=True)
    tileset(art / "tileset.png")
    hero(art / "hero.png")
    with Engine(exe, port, out / "engine-1.log", *engine_args(adapter, project, settings, out), cwd=str(out)) as eng:
        code, j = cli.run("list")
        mine = [e for e in j.get("engines", []) if e.get("port") == port]
        check(code == 0 and len(mine) == 1 and (mine[0].get("project") or {}).get("name") == NAME, f"the engine that has the project open is found from a terminal ({[e.get('port') for e in j.get('engines', [])]})")
        director, sculptor, painter, composer = (eng.mcp(n) for n in ("director", "sculptor", "painter", "composer"))
        auditor = eng.mcp("auditor", detailed=False)
        director.hello("director", role="lead")
        for m, n in ((sculptor, "sculptor"), (painter, "painter"), (composer, "composer"), (auditor, "auditor")):
            m.hello(n)
        info = director.json("engine_info")
        check((info.get("project") or {}).get("name") == NAME and info.get("entities") == 0, f"it is the project's engine, with an empty world ({(info.get('project') or {}).get('name')}, {info.get('entities')} entities)")

        # ---- 3D: a style, a prop, a rigged creature with its clips ----
        for rel in ("style.json", "recipes/lantern_post.json", "recipes/shambler.json", "anims/shambler.json"):
            sculptor.json("project_write", path=f"assets/{rel}", text=source(rel))
        lantern = sculptor.json("model_build", recipe="recipes/lantern_post.json")
        walker = sculptor.json("model_build", recipe="recipes/shambler.json")
        rig = sculptor.json("anim_get", recipe="recipes/shambler.json")
        clips = [c["name"] for c in rig.get("clips", [])]
        check(lantern["lods"][0]["triangles"] > 100 and walker["lods"][0]["triangles"] > 100 and rig.get("bones", 0) >= 10 and "walk" in clips,
              f"3D assets build in the new project: a prop of {lantern['lods'][0]['triangles']} triangles, a creature of {walker['lods'][0]['triangles']} with {rig.get('bones')} bones and clips {clips[:4]}")
        # Dusk, lit by the lanterns.
        director.call("world_env", sun_intensity=0.35, sun_direction=[0.5, 0.7, 0.4], ambient_intensity=0.16, tonemap=True, sky={"enabled": True}, shadow={"enabled": True}, bloom={"enabled": True})
        sculptor.call("world_spawn", shape="group", path="/yard")
        sculptor.call("world_spawn", shape="cube", path="/yard/ground", position=[0, -0.1, 0], scale=[14, 0.2, 10], color="#4f5a48")
        for k, x in enumerate((-1.6, 0.4, 2.4)):
            sculptor.call("world_spawn", shape="model", recipe="recipes/lantern_post.json", path=f"/yard/lantern_{k}", position=[x, 0, -1.0 + 0.5 * k], rotation_deg=[0, 40 * k, 0], light=CANDLE)
        sculptor.call("world_spawn", shape="model", recipe="recipes/shambler.json", path="/yard/walker", position=[-1.2, 0, 2.0], rotation_deg=[0, 50, 0], anim={"clip": "walk"})
        # The creature's pose is its clip's: the engine says which clip plays, and the same
        # creature from the same camera at the same moment is drawn differently under another.
        playing = sculptor.json("anim_state", path="/yard/walker", time_s=0.4)
        (out / "yard_walker_anim.json").write_text(json.dumps(playing, indent=1), encoding="utf-8")
        walk, _ = shot(sculptor, out, "walker_walk", **WALKER)
        sculptor.call("world_set", path="/yard/walker", anim={"clip": "idle"})
        resting = sculptor.json("anim_state", path="/yard/walker", time_s=0.4)
        (out / "walker_idle_anim.json").write_text(json.dumps(resting, indent=1), encoding="utf-8")
        idle, _ = shot(sculptor, out, "walker_idle", **WALKER)
        sculptor.call("world_set", path="/yard/walker", anim={"clip": "walk"})
        moved = picture_difference(walk, idle)
        check(playing.get("clip") == "walk" and resting.get("clip") == "idle" and moved is not None and moved > 0.25,
              f"the creature is drawn in its clip's pose: the engine says walk, then idle, and the two pictures differ (mean difference {moved:.2f} of 255)")

        # ---- 2D: two sheets as PNG files, a tile map and sprites that use them ----
        for f in ("tileset.png", "hero.png"):
            painter.json("project_upload", path=f"assets/textures/{f}", data=base64.b64encode((art / f).read_bytes()).decode("ascii"))
        ground, details, canopy = rows_2d()
        painter.call("world_spawn", shape="group", path="/board", position=[400, 0, 0])
        painter.call("world_spawn", shape="tilemap", path="/board/map", position=[-16, 9, 0], color=[1, 1, 1],
                     tilemap={"tileset": "textures/tileset.png", "tile_px": [16, 16], "size": [32, 18], "pixel_snap": True,
                              "animations": [{"tile": 3, "frames": 2, "frame_ms": 400}],
                              "layers": [{"name": "ground", "z_layer": 0, "rows": ground}, {"name": "details", "z_layer": 1, "rows": details}, {"name": "canopy", "z_layer": 3, "rows": canopy}]})
        sheet = {"texture": "textures/hero.png", "size": [1.5, 1.5], "pivot": [0.5, 0.0], "sheet": {"frame_px": [16, 16]}, "filter": "nearest", "blend": "cutout", "pixel_snap": True, "z_layer": 2}
        painter.call("world_spawn", shape="sprite", path="/board/hero", position=[0, 0, 0], color=[1, 1, 1], sprite=dict(sheet, anim={"count": 4, "fps": 6}))
        painter.call("world_spawn", shape="sprite", path="/board/villager", position=[5.0, 4.0, 0], color=[1, 0.8, 0.8], sprite=dict(sheet, frame=1))
        cells = painter.json("tilemap_get", path="/board/map", layer="ground")
        check(len(cells.get("rows", [])) == 18 and cells.get("region") == [0, 0, 32, 18], f"2D assets are in the world: a tile map of 32 x 18 cells in three layers, and two sprites, from uploaded sheets ({len(cells.get('rows', []))} rows read back)")

        # ---- a sound ----
        composer.json("project_write", path="assets/sounds/chime.json", text=source("sounds/bolt_hit.json"))
        heard = composer.json("audio_preview", path="assets/sounds/chime.json")
        target = json.loads(source("sounds/bolt_hit.json")).get("loudness")
        check(heard["metrics"]["clipped"] == 0 and abs(heard["metrics"]["lufs"] - target) <= 1.5, f"the sound renders at its loudness with no clipped sample ({heard['metrics']['lufs']} LUFS for {target})")

        # ---- pictures, the save, who made what ----
        yard, p = shot(sculptor, out, "yard_1", **YARD)
        seen = {s.get("path"): s for s in p.get("subjects", [])}
        drawn = [n for n in ("/yard/lantern_0", "/yard/lantern_1", "/yard/lantern_2", "/yard/walker") if (seen.get(n) or {}).get("visible_px", 0) > 200]
        check(len(drawn) == 4 and lit_share(yard) > 0.2, f"the 3D picture has the three props and the creature in it ({drawn})")
        board, _ = shot(painter, out, "board_1", **BOARD)
        check(lit_share(board) > 0.5, f"the 2D picture has the map in it ({lit_share(board):.2f} of it lit)")
        saved = director.json("world_save", name="yard")
        before = director.json("world_hash")
        check(saved.get("saved") == "yard" and before.get("entities", 0) >= 9, f"the world is saved as 'yard' ({before.get('entities')} entities, hash {before.get('state_hash')})")
        by = {}
        for who, _, paths in journal(auditor):
            for path in paths:
                by.setdefault(path, who)
        want = {"/project/assets/recipes/lantern_post.json": "sculptor", "/project/assets/anims/shambler.json": "sculptor", "/project/assets/textures/tileset.png": "painter",
                "/project/assets/sounds/chime.json": "composer", "/board/hero": "painter", "/yard/walker": "sculptor"}
        check(all(by.get(k) == v for k, v in want.items()), f"every asset and every entity names the agent that made it ({ {k.rsplit('/', 1)[-1]: by.get(k) for k in want} })")
        for m in (director, sculptor, painter, composer, auditor):
            m.close()
    check(eng.validation_errors() == 0, f"the first engine shut down with no validation message ({eng.validation_errors()})")
    code, j = cli.run("list")
    check(code == 0 and not [e for e in j.get("engines", []) if e.get("port") == port], "after the stop no engine is listed for the project")
    return before, lantern


def second_session(exe, port, adapter, project, settings, out, cli, before, lantern):
    """Another engine opens the project: everything is there, and is used again."""
    with Engine(exe, port, out / "engine-2.log", *engine_args(adapter, project, settings, out), cwd=str(out)) as eng:
        director, sculptor, auditor = eng.mcp("director"), eng.mcp("sculptor"), eng.mcp("auditor", detailed=False)
        director.hello("director", role="lead")
        sculptor.hello("sculptor")
        auditor.hello("auditor")
        listed = {f["path"]: f for f in director.json("project_list").get("files", [])}
        want = {"assets/style.json": "style", "assets/recipes/lantern_post.json": "recipe", "assets/recipes/shambler.json": "recipe", "assets/anims/shambler.json": "anim_set",
                "assets/textures/tileset.png": "png", "assets/textures/hero.png": "png", "assets/sounds/chime.json": "sound"}
        kinds = {k: (listed.get(k) or {}).get("kind") for k in want}
        check(all(k in listed for k in want), f"a new engine finds every asset in the project ({len(listed)} files; kinds {sorted(set(kinds.values()), key=str)})")
        check(director.json("engine_info").get("entities") == 0, "and starts with an empty world: nothing is loaded behind an agent's back")
        director.json("world_load", name="yard")
        after = director.json("world_hash")
        check(after.get("state_hash") == before.get("state_hash") and after.get("entities") == before.get("entities"),
              f"the saved world loads to the same hash ({after.get('state_hash')} = {before.get('state_hash')}, {after.get('entities')} entities)")
        again = sculptor.json("model_build", recipe="recipes/lantern_post.json")
        check(again["hash"] == lantern["hash"], f"the prop builds to the same model as before the restart ({again['hash']})")
        yard, _ = shot(sculptor, out, "yard_2", **YARD)
        board, _ = shot(sculptor, out, "board_2", **BOARD)
        d3, d2 = picture_difference(out / "yard_1.png", yard), picture_difference(out / "board_1.png", board)
        check(d3 is not None and d3 < 0.05 and d2 is not None and d2 < 0.05, f"and draws the same pictures, 3D and 2D (mean difference {d3:.3f} and {d2:.3f} of 255)")
        check(sculptor.json("anim_state", path="/yard/walker", time_s=0.4).get("clip") == "walk", "with the creature still playing its clip")

        # ---- reuse: a second world from the same assets, and a changed copy made from a terminal ----
        def terminal(tool, **args):
            f = out / "args.json"
            f.write_text(json.dumps(args), encoding="utf-8")
            return cli.run("call", tool, f"@{f}", "--port", port, "--as", "decorator")

        code, read = terminal("project_read", path="assets/recipes/lantern_post.json")
        text = read.get("text", "")
        check(code == 0 and '"name": "lantern_post"' in text and '"seed": 3115' in text, "an agent with only a terminal reads the prop's recipe (specula call ... @args.json)")
        # The copy is made of iron where the first is made of timber.
        iron = text.replace('"name": "lantern_post"', '"name": "lantern_iron"').replace('"material": "wood"', '"material": "iron"')
        check(iron.count('"material": "wood"') == 0 and text.count('"material": "wood"') >= 3, "the copy's timber parts are changed to iron in its text")
        code, wrote = terminal("project_write", path="assets/recipes/lantern_iron.json", text=iron)
        code2, built = terminal("model_build", recipe="recipes/lantern_iron.json")
        check(code == 0 and code2 == 0 and built.get("hash") not in (None, lantern["hash"]) and "wood" not in built.get("materials", ["wood"]) and "wood" in lantern.get("materials", []),
              f"copies it, changes it and builds the copy: another model, with no timber in it ({built.get('hash')} against {lantern['hash']}; {built.get('materials')})")
        # A change is taken back, by the one who made it.
        code, undone = terminal("world_undo", count=1)
        gone = not (project / "assets" / "recipes" / "lantern_iron.json").exists()
        code2, missing = terminal("project_read", path="assets/recipes/lantern_iron.json")
        check(code == 0 and gone and code2 == 1, f"world_undo takes the new file back, in the engine and on disk ({undone.get('undone')}, {missing.get('error')})")
        code, _ = terminal("project_write", path="assets/recipes/lantern_iron.json", text=iron)
        check(code == 0 and (project / "assets" / "recipes" / "lantern_iron.json").is_file(), "and it is written again")

        sculptor.call("world_spawn", shape="group", path="/annex", position=[100, 0, 0])
        sculptor.call("world_spawn", shape="cube", path="/annex/ground", position=[0, -0.1, 0], scale=[14, 0.2, 10], color="#5a5148")
        sculptor.call("world_spawn", shape="model", recipe="recipes/lantern_post.json", path="/annex/lantern", position=[-1.2, 0, 0], light=CANDLE)
        sculptor.call("world_spawn", shape="model", recipe="recipes/lantern_iron.json", path="/annex/lantern_iron", position=[0.6, 0, -0.4], light=CANDLE)
        sculptor.call("world_spawn", shape="model", recipe="recipes/shambler.json", path="/annex/walker", position=[2.2, 0, 0.8], rotation_deg=[0, 35, 0], anim={"clip": "idle"})
        (out / "annex_walker_anim.json").write_text(json.dumps(sculptor.json("anim_state", path="/annex/walker", time_s=0.4), indent=1), encoding="utf-8")
        sculptor.call("world_spawn", shape="sprite", path="/annex/sign", position=[-2.6, 0, 1.0], color=[1, 1, 1],
                      sprite={"texture": "textures/hero.png", "size": [1.2, 1.2], "pivot": [0.5, 0.0], "sheet": {"frame_px": [16, 16]}, "filter": "nearest", "blend": "cutout", "billboard": "cylindrical", "lit": True, "cast_shadows": True})
        annex, p = shot(sculptor, out, "annex", **ANNEX)
        seen = {s.get("path"): s for s in p.get("subjects", [])}
        drawn = [n for n in ("/annex/lantern", "/annex/lantern_iron", "/annex/walker", "/annex/sign") if (seen.get(n) or {}).get("visible_px", 0) > 150]
        check(len(drawn) == 4, f"a second place is built from the same assets: the prop, its copy in iron, the creature and a 2D sprite in a 3D scene ({drawn})")

        # ---- a person's edit on disk comes in, journaled ----
        chime = project / "assets" / "sounds" / "chime.json"
        edited = chime.read_text(encoding="utf-8").replace('"loudness": -21', '"loudness": -24')
        chime.write_text(edited, encoding="utf-8")
        # (The engine's watcher takes it in on its own thread; nobody has to read the file
        # for that. An agent that reads a file in the instant after it changed on disk has it
        # read on the engine thread instead: docs/ROADMAP.md, M5's open items.)
        took, outside = None, []
        t0 = time.time()
        while time.time() - t0 < 30 and took is None:
            outside = [e for e in journal(auditor, 50) if e[0] == "disk" and "/project/assets/sounds/chime.json" in e[2]]
            if outside:
                took = time.time() - t0
            else:
                time.sleep(0.25)
        seen_text = director.json("project_read", path="assets/sounds/chime.json").get("text", "")
        check(took is not None and '"loudness": -24' in seen_text, f"a file edited on disk by a person is taken in and journaled as theirs (in {took if took is None else round(took, 1)} s, {len(outside)} entry by 'disk')")
        quieter = director.json("audio_preview", path="assets/sounds/chime.json")
        check(abs(quieter["metrics"]["lufs"] + 24) <= 1.5, f"and the sound is the edited one ({quieter['metrics']['lufs']} LUFS)")

        # ---- an asset leaves as a .glb and is read back ----
        export = sculptor.json("model_export", recipe="recipes/lantern_iron.json")
        glb = next((project / "assets" / p for p in ("models/exports/lantern_iron.glb",) if (project / "assets" / p).is_file()), None)
        ok = False
        if glb:
            head = glb.read_bytes()[:12]
            magic, version, length = head[:4], *struct.unpack("<II", head[4:12])
            ok = magic == b"glTF" and version == 2 and length == glb.stat().st_size == export.get("bytes")
        back = sculptor.json("model_build", recipe="models/exports/lantern_iron.glb") if glb else {}
        check(ok and back.get("lods", [{}])[0].get("triangles") == export.get("triangles"),
              f"the changed copy is written as a .glb of {export.get('bytes')} bytes that the engine reads back with the same {export.get('triangles')} triangles")
        saved = director.json("world_save", name="annex")
        worlds = sorted(p.name for p in (project / "worlds").iterdir() if p.is_dir())
        check(saved.get("saved") == "annex" and {"yard", "annex"} <= set(worlds), f"both worlds are saved in the project ({worlds})")
        final = {f["path"]: f.get("by") for f in director.json("project_list").get("files", [])}
        check(final.get("assets/recipes/lantern_iron.json") == "decorator" and final.get("assets/sounds/chime.json") == "disk",
              f"the project says who last changed each file (the copy: {final.get('assets/recipes/lantern_iron.json')}; the sound: {final.get('assets/sounds/chime.json')})")
        for m in (director, sculptor, auditor):
            m.close()
    check(eng.validation_errors() == 0, f"the second engine shut down with no validation message ({eng.validation_errors()})")
    for n in (1, 2):
        log = (out / f"engine-{n}.log").read_text(encoding="utf-8", errors="replace")
        check("engine.stall" not in log and "panicked" not in log, f"engine {n} raised no stall and no panic ({log.count('engine.stall')} stalls)")
    return {"yard": str(yard), "board": str(board), "annex": str(annex)}


def main():
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap = argparse.ArgumentParser(description="M5 exit proof: assets made once, saved, reopened and reused, 2D and 3D.")
    ap.add_argument("--exe", default=str(target / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--port", type=int, default=7715, help="the first port; one more for each further adapter")
    ap.add_argument("--adapters", default="integrated,discrete")
    ap.add_argument("--out", default=str(target / "m5-exit"))
    a = ap.parse_args()
    exe = Path(a.exe)
    if not exe.is_file():
        print(f"no engine at {exe}: cargo build -p specula --release")
        return 2
    out_root = Path(a.out).resolve()
    report = {"exe": str(exe), "adapters": {}}
    for i, adapter in enumerate(x.strip() for x in a.adapters.split(",") if x.strip()):
        print(f"\n== adapter: {adapter} ==", flush=True)
        out = out_root / adapter
        if out.exists():
            shutil.rmtree(out)
        out.mkdir(parents=True)
        settings, port = out / "settings", a.port + i
        cli = Cli(exe, settings)
        code, j = cli.run("new", NAME, "--in", out / "projects")
        project = out / "projects" / NAME
        check(code == 0 and (project / "specula.project.json").is_file() and (project / "assets").is_dir(), f"specula new makes the project outside the repository ({code})")
        code, j = cli.run("projects")
        check(code == 0 and [p.get("name") for p in j.get("projects", [])][:1] == [NAME], "and lists it")
        n = len(problems)
        try:
            before, lantern = first_session(exe, port, adapter, project, settings, out, cli)
            pictures = second_session(exe, port, adapter, project, settings, out, cli, before, lantern)
        except Exception as e:  # a tool refused, or an engine did not start: said, and the next adapter runs
            check(False, f"{adapter}: stopped by {type(e).__name__}: {str(e)[:400]}")
            pictures = {}
        report["adapters"][adapter] = {"problems": problems[n:], "pictures": pictures}
    report["problems"] = problems
    (out_root / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"\nreport: {out_root / 'report.json'}")
    print("PASS" if not problems else f"FAILED: {len(problems)} check(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

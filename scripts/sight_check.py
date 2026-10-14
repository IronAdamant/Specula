#!/usr/bin/env python3
"""Sight check: builds a scene that exercises every sight tool and saves what they return.

The scene: a wall hiding a box, a sphere half hidden behind a low wall, a rigged Sepulcra creature
(skeleton_runner), two claimed districts (one region claim, one subtree claim) held by two agents.
Then: named cameras (fixed and follow, looked through by another agent), sight_frame with every
overlay, sight_orbit around the creature, sight_compare before/after a set of changes,
sight_query and the ID buffer's visible pixel counts.

    cargo build -p specula
    python scripts/sight_check.py               # port 7494, headless
    python scripts/sight_check.py --windowed

Everything goes to target/sight-check/: one PNG and one JSON per call, plus summary.json. LOOK
at every PNG, and have the sight-skeptic check the claims, before calling a visual change good.
Standard library only; output is ASCII.
"""
import argparse
import base64
import json
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

OUT = ROOT / "target" / "sight-check"


def save(name, result):
    """Writes a tool result's image (if any) and text to OUT; returns (json, png path or None)."""
    png = None
    text = None
    for c in result["content"]:
        if c["type"] == "image":
            png = OUT / f"{name}.png"
            png.write_bytes(base64.b64decode(c["data"]))
        elif c["type"] == "text":
            text = c["text"]
    data = json.loads(text)
    (OUT / f"{name}.json").write_text(json.dumps(data, indent=2))
    return data, png


def check(cond, msg, failures):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)


def subject(packet, path):
    return next((s for s in packet["subjects"] if s["path"] == path), None)


def build_scene(m):
    ops = [
        {"op": "spawn", "shape": "quad", "name": "ground", "scale": [30, 1, 30], "color": "#3a3f47"},
        # A tall wall with a box right behind it (hidden from the front camera).
        {"op": "spawn", "shape": "cube", "name": "wall", "position": [0, 1.5, 0], "scale": [6, 3, 0.4], "color": "#8a8f99"},
        {"op": "spawn", "shape": "cube", "name": "hidden_box", "position": [0, 0.5, -2.5], "color": "#d94f3d"},
        # A low wall whose top is level with the front camera's eye: the sphere's lower half hides.
        {"op": "spawn", "shape": "cube", "name": "low_wall", "position": [5, 0.5, 0], "scale": [2.4, 1, 0.3], "color": "#7d8590"},
        {"op": "spawn", "shape": "sphere", "name": "peeking_sphere", "position": [5, 1.0, -1.5], "color": "#e8c547"},
        # Out in the open.
        {"op": "spawn", "shape": "cube", "name": "marker", "position": [-3, 0.5, 4], "scale": 0.8, "color": "#4fa3e0"},
        # A rigged creature.
        {"op": "spawn", "shape": "model", "name": "runner", "recipe": "recipes/skeleton_runner.json", "position": [-6, 0, 2],
         "rotation_deg": [0, 30, 0]},
        # A district another agent claims by subtree.
        {"op": "spawn", "shape": "group", "name": "district_east", "position": [8, 0, 6]},
        # Glass behind glass, left of the wall: the back pane shows through the front one.
        {"op": "spawn", "shape": "cube", "name": "front_pane", "position": [-5.5, 1.5, 3], "scale": [1.6, 1.6, 0.05],
         "material": "glass", "color": "#cfe3ff"},
        {"op": "spawn", "shape": "cube", "name": "back_pane", "position": [-5.5, 1.5, 1.5], "scale": [0.7, 0.7, 0.05],
         "material": "glass", "color": "#ffd2a6"},
    ]
    for i in range(3):
        ops.append({"op": "spawn", "shape": "cube", "path": f"/district_east/house_{i}", "position": [i * 1.6 - 1.6, 0.6, 0],
                    "scale": [1.2, 1.2, 1.2], "color": "#b07a4f"})
    m.json("world_batch", ops=ops)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7494)
    ap.add_argument("--windowed", action="store_true")
    ap.add_argument("--adapter", help="discrete, integrated or an index (the engine's --adapter)")
    ap.add_argument("--exe", default=str(ROOT / "target" / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("sight_check: never on 7471 (that's the engine you work in)")
    OUT.mkdir(parents=True, exist_ok=True)
    # Fresh outputs every run (the engine's pipeline cache, .specula-cache, is kept).
    for f in OUT.iterdir():
        if f.is_file():
            f.unlink()
        elif f.name == "captures":
            shutil.rmtree(f)
    failures = []
    summary = {}
    engine_args = ["--assets", str(ROOT / "games" / "sepulcra" / "assets"), "--captures", str(OUT / "captures")]
    if not args.windowed:
        engine_args.append("--headless")
    if args.adapter:
        engine_args += ["--adapter", args.adapter]
    with Engine(args.exe, args.port, OUT / "engine.log", *engine_args, cwd=str(OUT)) as eng:
        builder = eng.mcp("sight-check")
        builder.json("agent_hello", name="builder_a", role="builder")
        other = eng.mcp("sight-check-b")
        other.json("agent_hello", name="builder_b", role="builder")
        reviewer = eng.mcp("sight-check-review")
        reviewer.json("agent_hello", name="reviewer", role="reviewer")
        build_scene(builder)
        builder.json("claim_take", region={"min": [-12, 0, -12], "max": [-2, 4, -2]}, note="west district")
        other.json("claim_take", path="/district_east", note="east district")

        print("cameras")
        cam = builder.json("camera_set", name="overview", eye=[4, 9, 16], target=[0, 0.5, 0], fov_y_deg=55)
        check(cam["version"] == 1 and cam["owner"] == "builder_a", "camera_set creates a camera owned by its caller", failures)
        builder.json("camera_set", name="front", eye=[0, 1, 12], target=[0, 1, -1], fov_y_deg=50)
        chase = builder.json("camera_set", name="chase", follow="/runner", offset=[0, 1.6, 4], look_offset=[0, 1, 0], local=True)
        check(chase["now"] is not None, "a follow camera resolves to its entity", failures)
        listed = builder.json("camera_list")
        check([c["name"] for c in listed["cameras"]] == ["chase", "front", "overview"], "camera_list lists them by name", failures)
        try:
            reviewer.json("camera_delete", name="overview")
            check(False, "another agent cannot delete my camera", failures)
        except RuntimeError as e:
            check("camera_not_found" in str(e), "another agent cannot delete my camera (it has none by that name)", failures)
        through, png = save("frame_reviewer_through_overview", reviewer.call("sight_frame", camera="overview", camera_of="builder_a",
                                                                           width=960, height=540, time_s=1))
        check(through["camera"]["source"] == "named" and through["camera"]["of"] == "builder_a",
              "a reviewer looks through builder_a's camera", failures)
        check(abs(through["camera"]["eye"][2] - 16) < 1e-4, "...and sees from its eye", failures)
        chase_pkt, _ = save("frame_chase", builder.call("sight_frame", camera="chase", width=640, height=480, time_s=1))
        runner = subject(chase_pkt, "/runner")
        check(runner is not None and runner["on_screen"], "the chase camera sees the runner", failures)
        builder.json("world_set", path="/runner", position=[-8, 0, 0])
        chase2, _ = save("frame_chase_after_move", builder.call("sight_frame", camera="chase", width=640, height=480, time_s=1))
        moved = [round(a - b, 3) for a, b in zip(chase2["camera"]["eye"], chase_pkt["camera"]["eye"])]
        check(moved == [-2.0, 0.0, -2.0], f"the chase camera moved with the runner {moved}", failures)
        check(through["camera"].get("camera_version") == 1 and len(through["camera"].get("owner_id", "")) == 8,
              "the packet names the owner identity and camera version", failures)
        # Reviewer finding: after a session ends, another taking its name is another identity.
        ann = eng.mcp("sight-check-ann")
        ann.json("agent_hello", name="ann")
        ann.json("camera_set", name="view", eye=[0, 5, 5], target=[0, 0, 0])
        first = reviewer.frame(camera="view", camera_of="ann", width=64, height=64)["camera"]
        ann.close()
        mal = eng.mcp("sight-check-mal")
        mal.json("agent_hello", name="ann")
        mal.json("camera_set", name="view", eye=[99, 5, 5], target=[0, 0, 0])
        second = reviewer.frame(camera="view", camera_of="ann", width=64, height=64)["camera"]
        check(first["owner_id"] != second["owner_id"], f"a new holder of the name shows a new owner_id ({first['owner_id']} -> {second['owner_id']})",
              failures)
        mal.close()
        summary["cameras"] = {"list": listed, "chase_moved": moved}

        print("overlays")
        ovl, ovl_png = save("frame_overlays_all", builder.call(
            "sight_frame", camera="overview", width=1280, height=720, time_s=1,
            overlays=["grid", "axes", "bounds", "labels", "claims", "skeletons", "lods"], max_subjects=20))
        drawn = ovl["overlays"]["drawn"]
        check(sorted(drawn) == sorted(["grid", "axes", "bounds", "labels", "claims", "skeletons", "lods"]),
              f"every overlay drawn: {drawn}", failures)
        check(ovl["overlays"]["claims"]["drawn"] == 2, "both claims drawn", failures)
        check(ovl["overlays"]["skeletons"]["entities"] == 1 and ovl["overlays"]["skeletons"]["bones"] >= 10,
              f"the runner's skeleton drawn ({ovl['overlays']['skeletons']})", failures)
        check(ovl["overlays"]["text"]["placed"] > 5, f"labels placed ({ovl['overlays']['text']})", failures)
        plain, _ = save("frame_plain", builder.call("sight_frame", camera="overview", width=1280, height=720, time_s=1))
        check("overlays" not in plain, "no overlays unless asked", failures)
        lods, _ = save("frame_overlays_lods_entities", builder.call(
            "sight_frame", camera="overview", width=960, height=540, time_s=1, overlays=["lods", "labels", "axes"],
            overlay_entities=["/runner", "/district_east"]))
        summary["overlays"] = ovl["overlays"]

        print("orbit")
        orb, orb_png = save("orbit_runner", builder.call("sight_orbit", target="/runner", views=8, elevation_deg=20,
                                                         overlays=["skeletons", "bounds"], time_s=1))
        check(len(orb["views"]) == 8 and orb["image"]["width"] <= 2048, f"8 views on one sheet {orb['image']['width']}x{orb['image']['height']}", failures)
        azs = [v["azimuth_deg"] for v in orb["views"]]
        check(azs == [0, 45, 90, 135, 180, 225, 270, 315], f"evenly spaced azimuths {azs}", failures)
        check(all(any(s["path"] == "/runner" for s in v["subjects"]) for v in orb["views"]), "the runner is in every view", failures)
        big = builder.call("sight_orbit", target="/runner", views=12, max_subjects=20, time_s=1,
                           overlays=["grid", "axes", "bounds", "labels", "claims", "skeletons", "lods"])
        size = len(next(c["text"] for c in big["content"] if c["type"] == "text"))
        check(size < 48 * 1024, f"a 12-view orbit with every overlay answers in {size} bytes", failures)
        summary["orbit"] = {k: orb[k] for k in ("image", "target", "distance_m")}

        print("compare")
        mark = builder.json("sight_compare", action="mark", name="before", camera="overview", width=960, height=540, time_s=1)
        check(mark["marked"] == "before", "mark stored", failures)
        builder.json("world_set", path="/marker", position=[-1, 0.5, 5])
        builder.json("world_spawn", shape="sphere", name="newcomer", position=[2, 0.6, 5], color="#5ad16a")
        builder.json("world_delete", path="/hidden_box")
        diff, diff_png = save("compare_diff", builder.call("sight_compare", action="diff", name="before"))
        subj = diff["subjects"]
        check(subj["added"]["items"] == ["/newcomer"], f"added: {subj['added']}", failures)
        check(subj["removed"]["items"] == ["/hidden_box"], f"removed: {subj['removed']}", failures)
        check([m_["path"] for m_ in subj["moved"]["items"]] == ["/marker"], f"moved: {subj['moved']}", failures)
        check(abs(subj["moved"]["items"][0]["world_moved_m"] - 5 ** 0.5) < 0.01, "...by sqrt(5) m (2 along x, 1 along z)", failures)
        check(diff["pixels"]["changed_pct"] > 0.1 and len(diff["pixels"]["regions"]) >= 2,
              f"changed pixels and regions: {diff['pixels']['changed_pct']}% in {diff['pixels']['regions_total']} regions", failures)
        same, _ = save("compare_diff_again", builder.call("sight_compare", action="diff", name="before"))
        check(same["pixels"]["changed_pct"] == diff["pixels"]["changed_pct"], "diffing twice is reproducible", failures)
        try:
            reviewer.json("sight_compare", action="diff", name="before")
            check(False, "marks are per agent", failures)
        except RuntimeError as e:
            check("mark_not_found" in str(e), "marks are per agent", failures)
        # Put the hidden box back, and the newcomer away (it stands in front of the low wall), for
        # the ID buffer checks.
        builder.json("world_spawn", shape="cube", name="hidden_box", position=[0, 0.5, -2.5], color="#d94f3d")
        builder.json("world_delete", path="/newcomer")
        summary["compare"] = {"pixels": diff["pixels"], "subjects": subj}

        print("visibility (ID buffer)")
        front, _ = save("frame_front_visible", builder.call("sight_frame", camera="front", width=1280, height=720, time_s=1,
                                                            overlays=["bounds", "labels"], max_subjects=50))
        hb, ps, mk = subject(front, "/hidden_box"), subject(front, "/peeking_sphere"), subject(front, "/marker")
        fp, bp = subject(front, "/front_pane"), subject(front, "/back_pane")
        summary["front"] = {"hidden_box": hb, "peeking_sphere": ps, "marker": mk, "front_pane": fp, "back_pane": bp}
        check(hb is not None and "visible_px" in hb, "captures carry the ID buffer's visible_px", failures)
        if hb is not None and "visible_px" in hb:
            check(hb["visible_px"] == 0 and hb.get("no_pixels") is True and hb.get("occluded") is True,
                  f"the box behind the wall: 0 visible px, no_pixels, occluded ({hb})", failures)
            # Glass behind glass: both panes count (the back one shows through the front one).
            check(fp["visible_px"] > 0 and bp["visible_px"] > 0 and "occluded" not in bp,
                  f"glass behind glass is visible (front {fp['visible_px']} px, back {bp['visible_px']} px)", failures)
            bx = bp["screen_box_exact"]
            check(bp["visible_px"] >= 0.9 * bx[2] * bx[3], "...counted over its whole face, once", failures)
            # Its top half shows over the low wall: about half a disc as wide as its exact box.
            d = ps["screen_box_exact"][2]
            ratio = ps["visible_px"] / (3.14159 * (d / 2) ** 2)
            check(0.4 < ratio < 0.6 and abs(ps["screen_box_exact"][3] - d / 2) <= 2,
                  f"the peeking sphere shows its top half ({ps['visible_px']} px = {ratio:.2f} of its disc, box {ps['screen_box_exact']})",
                  failures)
            mk_exact, mk_proj = mk["screen_box_exact"], mk["screen"]
            check(mk_exact[2] * mk_exact[3] <= mk_proj[2] * mk_proj[3], "exact boxes are never bigger than projected ones", failures)
            q = builder.json("sight_query", path="/hidden_box", camera="front")
            check(q["visible_px"] == 0 and q["occluded"] is True, "sight_query: the hidden box is occluded", failures)
            vis, _ = save("frame_front_visible_only", builder.call("sight_frame", camera="front", width=1280, height=720,
                                                                  time_s=1, subjects="visible"))
            check(all(s.get("visible_px", 0) > 0 for s in vis["subjects"]), "subjects: visible lists only visible ones", failures)
            check(not any(s["path"] == "/hidden_box" for s in vis["subjects"]), "...and not the hidden box", failures)
            summary["query_hidden_box"] = q
        off = builder.frame(camera="front", width=640, height=360, time_s=1, ids=False, max_subjects=50)
        check(off["id_buffer"] == {"off": True} and all("visible_px" not in s for s in off["subjects"]),
              "ids: false skips the ID buffer", failures)

        print("compare: visibility is not movement")
        builder.json("sight_compare", action="mark", name="screen_test", camera="front", width=640, height=360, time_s=1)
        builder.json("world_spawn", shape="cube", name="screen", position=[-1, 1, 7], scale=[2.5, 2.5, 0.2], color="#666a70")
        sd, _ = save("compare_screen", builder.call("sight_compare", action="diff", name="screen_test"))
        subj2 = sd["subjects"]
        check(subj2["moved"]["count"] == 0, f"nothing moved when a screen hid the marker ({subj2['moved']})", failures)
        check(subj2["added"]["items"] == ["/screen"] and "/marker" in subj2["became_hidden"]["items"],
              f"the screen was added and the marker became hidden ({subj2['added']}, {subj2['became_hidden']})", failures)
        builder.json("world_delete", path="/screen")

        print("ID buffer cost at 1280x720")
        costs = []
        for _ in range(6):
            costs.append(builder.frame(camera="front", width=1280, height=720, time_s=1, max_subjects=1)["id_buffer"])
        costs = costs[1:]  # the first warms caches
        gpu = sorted(c["gpu_ms"] or 0 for c in costs)
        cpu = sorted(c["readback_count_ms"] for c in costs)
        info = builder.json("engine_info")
        summary["id_buffer_cost"] = {"gpu": info["gpu"]["name"], "gpu_ms_median": gpu[len(gpu) // 2],
                                     "readback_count_ms_median": cpu[len(cpu) // 2], "samples": costs}
        print(f"  {info['gpu']['name']}: gpu {gpu[len(gpu) // 2]:.3f} ms, readback + count {cpu[len(cpu) // 2]:.3f} ms"
              f" (median of {len(costs)})")

        for m_ in (builder, other, reviewer):
            m_.close()
    log = (OUT / "engine.log").read_text(errors="replace")
    errors = eng.validation_errors()
    check(errors == 0, f"clean shutdown, {errors} validation errors", failures)
    summary["failures"] = failures
    (OUT / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\n{len(failures)} failure(s); outputs in {OUT}")
    if "panicked" in log:
        print("engine log mentions a panic")
        return 1
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

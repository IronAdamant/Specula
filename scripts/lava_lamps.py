"""Fidelity benchmark: three lava lamps at rising fidelity, built and captured over MCP.

  L1  basic       - opaque lathe shapes, glowing colour, wax spots; no transparency.
  L2  transparent - metal base and cap, glass shell over a tinted liquid shell, wax spheres
                    inside, a point light.
  L3  realistic   - the ray-marched lava_lamp material: refraction, absorbing and scattering
                    liquid, merging wax, glow halo, light spill on the table.

Usage: python scripts/lava_lamps.py [--port N] [--out DIR]
Starts nothing: run the engine first (cargo run -p specula). Prints the capture paths.
"""
import argparse
import shutil
import sys

from specula_mcp import Mcp


# Classic colours: red wax in yellow liquid.
WAX = "#ff3d12"
LIQUID = "#ffb31a"
METAL = "#8a8f98"

# Lamp dimensions (metres). The globe is 0.24 m tall, widest (r 0.055) 30% up.
R = 0.055
GLOBE_H = 0.24
BASE_H = 0.132
BASE = [[0.0, 0.0], [0.066, 0.0], [0.068, 0.006], [0.066, 0.012], [0.038, 0.125], [0.040, 0.128],
        [0.036, BASE_H], [0.0, BASE_H]]
CAP = [[0.0, 0.0], [0.026, 0.0], [0.026, 0.004], [0.009, 0.062], [0.0, 0.066]]


def globe(scale=1.0):
    """The vessel profile used by lava_lamp: radius 0.62R at the bottom, R at 30%, 0.42R on top,
    the shoulder rounded. Inset 1.5 mm at both ends so it never z-fights with the base or cap."""
    pts = [[0.62 * R, 0.0015], [0.92 * R, 0.24 * GLOBE_H], [0.985 * R, 0.28 * GLOBE_H], [0.985 * R, 0.32 * GLOBE_H],
           [0.94 * R, 0.36 * GLOBE_H], [0.42 * R, GLOBE_H - 0.0015]]
    return [[r * scale, y] for r, y in pts]


def clear_world(m):
    """Deletes everything. Paged engines (world_query) delete top-level entities, which takes their
    subtrees with them; older engines list everything in one call."""
    if "world_query" not in m.tools():
        for e in m.json("world_list")["entities"]:
            m.call("world_delete", id=e["id"])
        return
    while True:
        page = m.json("world_query", fields=["id", "path"], limit=500)
        top = [e for e in page["items"] if e["path"].count("/") == 1]
        if not top:
            return
        m.call("world_batch", ops=[{"op": "delete", "id": e["id"]} for e in top[:1000]])


# Renderer effects for the lamps (M2): real bloom spreads the glowing wax and bulbs, FXAA smooths
# the lathe silhouettes. Off with --plain, which reproduces the original 2026-10-13 benchmark
# frames exactly (a pixel regression check for the renderer).
EFFECTS = dict(bloom={"enabled": True, "threshold": 1.2, "knee": 0.6, "intensity": 0.55, "levels": 6}, fxaa=True)


def build(m, plain=False):
    clear_world(m)
    m.call("world_env", sun_direction=[-0.3, 0.8, 0.5], sun_color="#9fb4ff", sun_intensity=0.06,
           ambient_color="#6d7a99", ambient_intensity=0.035, exposure=1.3, tonemap=True, background="#050608",
           **({} if plain else EFFECTS))

    # Room: varnished walnut table and a dark wall.
    m.call("world_spawn", shape="quad", name="table", scale=[1.8, 1, 0.9], color="#4a3122",
           specular=0.35, roughness=0.35)
    m.call("world_spawn", shape="quad", name="wall", position=[0, 0.6, -0.4], rotation_deg=[90, 0, 0],
           scale=[1.8, 1, 1.2], color="#2a2c31")

    spot = lambda x, name: m.call("world_spawn", shape="quad", name=name, position=[x, 0.001, 0], scale=0.26,
                                  material="blob_shadow", opacity=0.55)
    base_and_cap = lambda x, tag, **look: (
        m.call("world_spawn", shape="lathe", name=f"{tag}_base", position=[x, 0, 0], profile=BASE, segments=48,
               color=METAL, **look),
        m.call("world_spawn", shape="lathe", name=f"{tag}_cap", position=[x, BASE_H + GLOBE_H, 0], profile=CAP,
               segments=48, color=METAL, **look),
    )
    y0 = BASE_H

    # L1 basic: everything opaque. The globe glows in the liquid colour; wax shows as spots.
    x = -0.42
    spot(x, "l1_shadow")
    base_and_cap(x, "l1")
    m.call("world_spawn", shape="lathe", name="l1_globe", position=[x, y0, 0], profile=globe(), segments=32,
           color=LIQUID, emissive=0.45)
    for i, (dx, dy, dz, s) in enumerate([(0.022, 0.05, 0.033, 0.040), (-0.03, 0.12, 0.022, 0.032),
                                         (0.008, 0.19, 0.016, 0.022)]):
        m.call("world_spawn", shape="sphere", name=f"l1_wax_{i}", position=[x + dx, y0 + dy, dz], scale=s,
               color=WAX, emissive=0.8)

    # L2 transparent: metal, glass over tinted liquid, wax inside, a warm light.
    x = 0.0
    spot(x, "l2_shadow")
    base_and_cap(x, "l2", metallic=0.85, roughness=0.42, specular=1.0)
    m.call("world_spawn", shape="lathe", name="l2_glass", position=[x, y0, 0], profile=globe(), segments=48,
           color="#ffffff", material="glass", opacity=0.05)
    m.call("world_spawn", shape="lathe", name="l2_liquid", position=[x, y0, 0], profile=globe(0.93), segments=48,
           color=LIQUID, material="glass", opacity=0.4, emissive=0.5)
    m.call("world_spawn", shape="sphere", name="l2_pool", position=[x, y0 + 0.012, 0], scale=[0.062, 0.03, 0.062],
           color=WAX, emissive=1.4)
    for i, (dx, dy, dz, s) in enumerate([(0.012, 0.065, 0.006, [0.03, 0.042, 0.03]), (-0.01, 0.13, -0.005, 0.026),
                                         (0.004, 0.2, 0.0, 0.017)]):
        m.call("world_spawn", shape="sphere", name=f"l2_wax_{i}", position=[x + dx, y0 + dy, dz], scale=s,
               color=WAX, emissive=1.2)
    m.call("world_spawn", shape="sphere", name="l2_bulb", position=[x, y0 + 0.06, 0], scale=0.001, color="#ffb050",
           light={"intensity": 0.03, "range": 2.0})

    # L3 realistic: the ray-marched lamp (cube proxy 3.2R wide, as tall as the globe).
    x = 0.42
    spot(x, "l3_shadow")
    base_and_cap(x, "l3", metallic=0.85, roughness=0.42, specular=1.0)
    m.call("world_spawn", shape="cube", name="l3_lamp", position=[x, y0 + GLOBE_H / 2, 0],
           scale=[3.2 * R, GLOBE_H, 3.2 * R], color=WAX, color2=LIQUID, material="lava_lamp", emissive=2.4, seed=1,
           light={"intensity": 0.03, "range": 2.0, "color": "#ff9a40", "offset": [0, -0.25, 0]})


SHOTS = [
    ("all", dict(eye=[0, 0.36, 1.05], target=[0, 0.19, 0], fov_y_deg=38, width=1600, height=900, time_s=8)),
    ("l1", dict(eye=[-0.42, 0.27, 0.62], target=[-0.42, 0.21, 0], fov_y_deg=42, width=900, height=1100, time_s=8)),
    ("l2", dict(eye=[0.0, 0.27, 0.62], target=[0.0, 0.21, 0], fov_y_deg=42, width=900, height=1100, time_s=8)),
    ("l3_t08", dict(eye=[0.42, 0.27, 0.62], target=[0.42, 0.21, 0], fov_y_deg=42, width=900, height=1100, time_s=8)),
    ("l3_t30", dict(eye=[0.42, 0.27, 0.62], target=[0.42, 0.21, 0], fov_y_deg=42, width=900, height=1100, time_s=30)),
    ("l3_t55", dict(eye=[0.42, 0.27, 0.62], target=[0.42, 0.21, 0], fov_y_deg=42, width=900, height=1100, time_s=55)),
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7471)
    ap.add_argument("--out", default=None, help="copy the captures here as <shot>.png")
    ap.add_argument("--no-build", action="store_true", help="capture the current world only")
    ap.add_argument("--plain", action="store_true", help="no renderer effects: the original benchmark frames")
    args = ap.parse_args()
    m = Mcp(f"http://127.0.0.1:{args.port}/mcp", "lava-lamps")
    if not args.no_build:
        build(m, args.plain)
    m.call("window_view", eye=[0, 0.36, 1.05], target=[0, 0.19, 0], fov_y_deg=38)
    for name, shot in SHOTS:
        packet = m.frame(max_subjects=1, **shot)
        path = packet["image"]["path"]
        if args.out:
            dest = f"{args.out}/{name}.png"
            shutil.copyfile(path, dest)
            path = dest
        print(f"{name}: {path} warnings={packet['warnings']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

# Fidelity track: sheets and numbers

The same five benchmark subjects (a carved runestone, a banded barrel, an iron lantern, a gnarled tree, a skeleton
warrior) are rebuilt at each fidelity tier, so the engine's visual progress can be measured
([ROADMAP](../ROADMAP.md), Fidelity track). Their recipes, style rules and stage dressing are in
[`assets/`](assets/).

| Sheet | Tiers | Made by |
|---|---|---|
| [benchmark_t0_t1_studio.png](benchmark_t0_t1_studio.png) | T0 greybox, T1 stylised flat (M2) | `python scripts/studio_check.py --fidelity --sheets t01` |
| [benchmark_t3_m4.png](benchmark_t3_m4.png) ([numbers](benchmark_t3_m4.json)) | T0, T1 and T3 detailed stylised (M4) | `python scripts/studio_check.py --fidelity --sheets t3` ([fidelity_t3.py](../../scripts/fidelity_t3.py)) |

## T3 (M4): detailed stylised

**What T3 adds, per subject** (all data: recipes `assets/recipes/*_t3.json`, texture graphs `tex_*_t3.json`, the stage
dressing `assets/stage_t3.json`, effects `assets/vfx/`, decal textures `assets/fx/`, the skeleton's motion
`assets/anims/skeleton_warrior_t3.json`):

| Subject | SDF sculpts | Normal-mapped texture graphs (128 px) | Decals | Particles | Light and GI |
|---|---|---|---|---|---|
| Runestone | weathered slab with the runes carved in (smooth difference), footing, moss cushions | weathered stone (grain, pits, cracks, moss in the pits) | moss and lichen on the front and side, moss on the ground | rune motes | GI 0.25 m |
| Barrel | (a lathe: an SDF wouldn't help) | stave wood (seams, grain, knots), rusted iron hoops (brass rivets) | grime at the foot and wear scuffs (multiply / lit), a ground stain (faint: packet evidence) | | GI 0.25 m |
| Lantern | (forged parts: an SDF wouldn't help) | rusted iron | soot on the wall (doesn't show: packet evidence only) | the candle's flame (a small warm tip in the close-up, partly behind a post) and embers out of the chimney | a point light; GI 0.25 m with two pale walls beside it |
| Tree | a leaning trunk of three blended strands (twisted in the recipe; at this resolution it reads as one column), root flares and two branches, a crown of six blended, noise-lumped masses | bark, leaf clumps | moss on the ground at the roots | leaves drifting down | GI 0.5 m |
| Skeleton warrior | every bone (knobbed limbs, ribcage and sternum, pelvis, skull with sockets, jaw; the vertebrae and nose are barely visible) | bone, rusted blade, shield wood | dirt on the ground (faint) | | GI 0.25 m; rigged (18 bones), shown in its idle clip at 6 s |

**How it's rendered.** Decals, particles, the lantern's light and baked GI are world features, and the asset studio's
stage is isolated (builds only), so the script rebuilds the studio's stage in a headless engine's world: the studio's
ground and backdrop per light rig, its `neutral`, `game` and `worst` rigs (read from `studio_stage`, the key light
turned with the camera as the studio turns it), its close-up framing and its game camera, cropped at true pixel size.
All three tiers render there through the same cameras and light; T0 and T1 with GI off and without dressing. The
clock is paused and every capture is taken at `time_s` 6, so particles and the skeleton's pose repeat. Every T3
subject also passes `style_check` and `studio_check` on the studio's own stage (0 errors), and every effect passes
`vfx_check` against its palette family (fire for the lantern, leaves for the tree).

**iGPU budget (F3).** Intel RaptorLake-S iGPU, release engine, headless, 1280×720 offscreen frames with the game
rig's shadows, bloom and FXAA, GI tier **low** (`gi_settings` reports `low` as the integrated adapter's default, and
every T3 packet says `quality: low`). Each figure is the lowest of 24 frames, alternating 8 times between the subject
(with its decals and effects) in place and moved away, so the difference is the subject's own cost; no other engine
was running on the GPU for the runestone; one or two other sessions' engines were for the others (the sheet and the JSON record how many per measurement), so read those as upper bounds (2026-10-30, run 7).

| T3 subject | Triangles (LOD 0 / later) | Textures | Materials | Gameplay view | Close-up filling the frame | Other engines |
|---|---|---|---|---|---|---|
| Runestone | 15,152 / 4,164 | 256 KB | 3 | 3.07 ms (+0.45) | 6.15 ms (+2.99) | 0 |
| Barrel | 3,320 / 1,640 | 512 KB | 4 | 2.90 ms (+0.28) | 5.81 ms (+2.43) | 1 |
| Lantern | 1,058 / 660 | 256 KB | 5 | 2.85 ms (+0.15) | 6.41 ms (+1.11) | 1-2 |
| Tree | 7,696 / 2,320 / 1,924 | 512 KB | 2 | 3.75 ms (+1.10) | 4.60 ms (+1.77) | 1-2 |
| Skeleton warrior | 9,608 / 2,774 | 768 KB | 6 | 2.81 ms (+0.20) | 4.40 ms (+1.30) | 1 |

In play (the game camera, 12 m away) each subject adds 0.15-1.1 ms of GPU to a 2.6 ms bare stage; filling the screen,
1.1-3 ms (a quiet machine in run 6b: 0.13-0.70 and 1.09-2.56 ms; the tree bakes at 0.5 m since run 7, so its
wide crown reads more probes per pixel), most of it the scene pass and, for the runestone and barrel, their decals (0.5-0.6 ms when a lit decal
covers much of the frame). The T3 budget in `assets/style.toml` (Claude's proposal): 16k triangles at LOD 0, 6k and
3k at the later levels, 1 MB of textures, 9 materials. Bakes: 384 to 2,184 probes, 0.4 to 0.7 s each on four threads.

**How to read the sheet** (the sight-skeptic's review, 2026-10-30). Each shot is the studio's own: the close-up is
framed to each tier's own bounds, and the game camera is centred on each tier's own box with its crop class chosen from
its size (the T0 lantern's gameplay crops are 96 px, T1's and T3's 128 px), so tiles in a group share direction and
light, not exact framing. The irradiance column is rendered with the decals moved away (decals are re-lit by that view,
so moss would read as light). In the barrel's close-up the ground stain is skipped by the iGPU's decal fill cap
(`[vfx] decal_fill_cap` 1 screen on integrated GPUs; the packet says `decals_skipped: 1`); the gameplay shots draw all
five. Small effects (motes, embers, falling leaves) are a few pixels at the game camera's distance; their packets list
them. The irradiance column shows each view stretched over its probes' 5th to 95th percentiles: what is darker than
that (contact under the barrel, the moss at the runestone's foot, one deep crease in the tree's crown, which also shows
faintly in the lit image) reads black there; it is a dim floor, not missing probes. The skeleton's open stage has
probes nearly alike, so its tile is pale and nearly flat. The T1 runestone's rune strokes float off the slab's left edge (its `array` jitter turns copies about the
origin): a T1 recipe issue, left as it is so the T0/T1 sheet stays the M2 record.

**What the bake adds.** Lit against GI off in the gameplay view, 0.6 to 9% of the pixels change (the tree and the
lantern's corner most, the skeleton least). The lantern's light bounces +0.06 to +0.07 probe luminance onto its walls and +0.19 onto the ground in the
corner (baked with the light, then without it).

**Not done at T3 (and why):**
- **Alpha foliage and wind** (the tree subject's list): texture graphs have no opacity output and meshes no vertex
  wind, so the crown is a sculpted, textured mass; the drifting leaves stand in for wind. Both need engine work (a
  texture-graph alpha output with the masked pipeline that exists for glTF, and a wind vertex term).
- **The lantern's flame is particles only:** an opaque flame mesh inside the glass hid the particles (depth) and
  shadowed the point light completely in the GI bake (a light inside a mesh lights nothing there); the wick sits below
  the light.
- **GI fixes this sheet found** (on main since run 7): at 0.5 m and 0.25 m the tree's crown showed black cells
  (surfaces where every probe read as hidden); the GI lane fixed it, and the tree bakes at 0.5 m again (one deep
  crease of the crown still reads black in the irradiance column and faintly dark in the lit image: below the view's
  floor, see "How to read the sheet"; reported to the GI lane). The
  irradiance view now stretches the probes' range for readability; an open stage whose probes are nearly alike
  (the skeleton's) was stretched 264 times into a white frame, so the stretch is now at least 8% of its top
  (`IRRADIANCE_VIEW_MIN_SPREAD`, this lane's fix with a test).
- **The skeleton is left out of the bake's geometry** (animated models are), and receives GI like any draw.
- **Moss amount:** `runestone_t3`'s `moss` parameter (0.75) is Claude's pick from a gallery round
  (`runestone_t3_moss`, six variants rendered through the studio); the owner may pick again.

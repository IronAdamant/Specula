# Sepulcra: style bible (draft)

Status: **Claude's proposal, 2026-10-13**, written for the owner to approve or change. Everything here is a
rule agents build to: shape language, palette, proportions, materials and lighting. When M4 lands, the hard
numbers move into `style.toml` and are checked automatically.

## The look in one sentence

**A moonlit graveyard on a broken slab of dark stone, floating over a sea of mist; your magic glows cyan,
the dead glow sickly green, and fire is the only warm light.**

Stylised, not realistic: chunky bevelled shapes, flat-ish colour with soft light, strong silhouettes.
Think carved board-game pieces lit by moonlight. It must read at a glance from the default camera
(pillar 4) and run on an Intel iGPU and a Steam Deck (F3).

## Three colour families, never mixed

Readability rule: **you can tell whose a thing is from its colour alone.**

| Family | Meaning | Colours (sRGB) |
|---|---|---|
| **Player (rune cyan)** | Your stones, runes, bolts, wall highlights, the cursor, friendly UI | glow `#3fe0d0`, deep `#1a8f9e`, pale `#b8fff6` |
| **Enemy (grave green)** | Undead eyes and wounds, necromantic magic, wraith trails, threat arrows | glow `#9cff4f`, deep `#3e7a1f`, sickly `#c8e66a` |
| **Fire (warm)** | Brazier cairn flames and splash only; the single warm accent in the scene | core `#ffd27a`, flame `#ff7a1f`, ember `#c2410c` |

Everything else is neutral and cool, so the three families pop:

| Neutral | Use | Colours |
|---|---|---|
| Slab stone | top face, cliffs, fragments | top `#5b6470`, cliff strata `#3d434d` / `#4a515c`, fissure floor `#262a31` |
| Carved stone | the player's stones (body, before runes) | `#8c96a3`, chamfer highlights `#aab4c0` |
| Moss and roots | nature, from wave 5; fragment edges | `#4f6b3a`, `#6d8a4a`, roots `#5a4632` |
| Bone | undead bodies | `#d8cfb8`, shadowed `#9c937e` |
| Graveyard | tombstones, fence | `#a7a39a`, lichen `#7d8a6a` |
| Night | sky, mist, background | sky `#0b1020`, mist `#2a3448`, moon `#c9d6ff` |

Rules:
- **Saturation belongs to the families.** Neutrals stay below ~25% saturation.
- **Emissive only for gameplay meaning.** Runes, eyes, bolts, fire. Nothing glows just to look nice.
- Never use green on the player side or cyan on the enemy side (accessibility: the two families also
  differ in brightness and shape, not just hue, for colour-blind players).

## Shape language

| Thing | Shape | Silhouette rule |
|---|---|---|
| Player stones | Vertical, rectangular monoliths with chamfered edges | Upright and stable. Each type has a distinct top: warding stone pointed, rune pillar flat with a ring, brazier cairn a bowl, sky obelisk a tall needle |
| Tiers I–III | More carved runes, then a capstone, then a floating rune ring | You can read the tier from the default camera without the UI |
| Walls | Low dry-stone rows, slightly irregular blocks | Knee to waist high; never hides an enemy behind it |
| Undead | Organic, hunched, broken outlines | Shambler wide and low; runner thin, leaning forward; wraith a tattered inverted cone with no feet; boss a giant ribcage and spine |
| Slab | Thick, layered stone with sharp fractures | Flat top, vertical cliffs with horizontal strata, jagged underside that fades into the mist |
| Lane-jump points | Cracked ledges with a pale chalk-like scar | Visible from above at a glance |
| Graveyard | Small, dense, pale tombstones inside a low fence | The pale centre of the slab is the "eye" of the composition |

Bevel everything: no hard 90° edges on anything you build (bevel about 4–8% of the smallest dimension).
It reads better at low resolution and catches the moonlight.

## Proportions (1 lane cell = 2 m)

| Thing | Size |
|---|---|
| Slab top face | about 40 × 40 m; cliffs about 15 m high |
| Warding stone tier I | 1.0 × 1.0 × 2.4 m (tier III about 3.2 m with the rune ring) |
| Sky obelisk | 0.8 × 0.8 × 4 m |
| Wall segment | 2 m long, 0.6 m deep, 0.9 m high |
| Shambler / runner / wraith | 1.7 m / 1.8 m / 2.2 m tall |
| Boss | about 8 m tall |
| Tombstone | 0.5 × 0.15 × 0.8 m |

Gameplay pieces are slightly oversized compared with reality, so they read from 30–60 m away.

## Light and atmosphere

- **Key light: the moon.** Cool `#c9d6ff`, low angle (about 30° up), so cliffs on one side are lit and
  the other side falls into blue shadow. The lit side changes as you rotate the slab, which helps you read
  which face you're looking at.
- **Ambient:** dim blue-violet `#2a3448`, about 0.2–0.3 of the key.
- **Local lights:** brazier fire (warm, flickering), stone runes (small cyan point lights at tier III only,
  to save light budget), wraiths (faint green).
- **Mist below the slab** (height fog, M2) hides the void and makes the slab float. Enemies climb up out
  of the mist, which is also the telegraph for a cliff attack.
- **Tone mapping:** ACES with exposure around 1.1; slight bloom on emissives only (M2 post).
- **Shadows:** one directional shadow map for the moon; the blob shadows already in the engine work for
  units if the budget is tight on the iGPU.

## Materials (presets)

All use the engine's `standard` material. Textures are optional; the base look must work with vertex
colour and roughness alone (procedural tier 1).

| Preset | Colour | Roughness | Metallic | Emissive | Notes |
|---|---|---|---|---|---|
| `slab_stone` | `#5b6470` | 0.9 | 0 | 0 | strata as vertex-colour bands |
| `carved_stone` | `#8c96a3` | 0.75 | 0 | 0 | |
| `rune_glow` | `#3fe0d0` | 0.4 | 0 | 2.5 | inset runes, bolts |
| `bone` | `#d8cfb8` | 0.7 | 0 | 0 | |
| `grave_glow` | `#9cff4f` | 0.5 | 0 | 2.0 | eyes, trails |
| `iron` | `#5c6168` | 0.45 | 0.9 | 0 | brazier bowl, fence |
| `fire` | `#ff7a1f` | 1.0 | 0 | 3.0 | additive particles later |
| `wraith` | `#9cff4f` | — | — | 0.8 | `glass`, opacity 0.35 |
| `moss` | `#4f6b3a` | 0.95 | 0 | 0 | |

## UI (M5, but the rules start now)

- Carved-stone panels (neutral `#2b3038`, bevelled edges), ivory text `#ece6d6`, player cyan for
  highlights and selection, grave green only for threats.
- A clean sans-serif text face for legibility on the Deck at 800p (minimum 9 px cap height at 800p).
  Decorative serif only for titles.
- Threat arrows at the screen edge use grave green and pulse; they point to the face under attack.

## Performance budget (F3)

| Item | Budget |
|---|---|
| Frame | 60 fps at 1080p on the Intel iGPU (16.6 ms), 60 fps at 800p on the Deck |
| Triangles per enemy | 300–800 (instanced; hundreds on screen) |
| Triangles per stone | about 1.5k (tier III up to 3k) |
| Point lights on screen | 16 maximum (engine limit); prefer emissive without lights |
| Draw calls | under 300 per frame after instancing |

## Open questions for the owner

- Do you like the **floating slab over mist**? The alternative is a slab sitting in a dark canyon.
- **Cyan magic vs green dead:** happy with these two families, or another pair?
- How dark should night be? This proposes "readable moonlight", not "pitch black with torches".

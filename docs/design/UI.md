# Design: UI and text

Status: researched 2026-10-13, not yet implemented. The UI toolkit and text land in M5; the editor is built on them
([ROADMAP.md](../ROADMAP.md)). Engine comparison: [research/ENGINE_SURVEY.md](../research/ENGINE_SURVEY.md).

**Goal:** one zero-dep UI toolkit that serves both the **game UI** (menus, HUD, dialogue, inventory) and the **human
editor** (panels, gizmos, outliner). It must be fast on integrated GPUs, fully usable with a gamepad, legible on a
handheld, and **as visible to agents as the 3D world is**.

## Architecture: immediate-mode API, retained layout

- **Immediate-mode API.** Code describes the UI each frame (`if button("Play") { … }`). There's no widget object graph
  to keep in sync with game state, which suits both hot-reloaded modules and agents writing code. Dear ImGui and egui
  proved the model for tools.
- **Flexbox-style layout pass.** Containers lay out children in rows or columns, with grow, shrink, padding, gaps,
  alignment and percentage or fixed sizes. Clay (Nic Barker, a single zero-dependency C file) showed this runs in
  microseconds with a per-frame arena and no allocation. We write our own in Rust, using the same ideas.
- **Retained ids.** Every element gets a stable id from its path plus a key, so focus, animation, scroll position and
  agent references survive across frames.
- **Output is draw data, not drawing.** Layout produces a flat list of primitives:
  - rounded rectangles with borders
  - images or nine-slices
  - glyph runs
  - clip rectangles

  One batched pipeline renders them with very few draw calls, which is cheap on low-end GPUs (F3). Rounded rects and
  shadows use signed-distance shaders, so no textures are needed.
- **UI as data where it helps.** Static screens (main menu, settings, HUD layout) can be written as text files that
  agents edit and diff like world files. Code fills in dynamic values. Themes are data too: colours, fonts, spacing,
  corner radii, from the game's style bible.

## Text

Text is the hardest part of UI, so it's built in stages:

| Stage | Contents |
|---|---|
| **T1** | **TrueType/OpenType parser** (`cmap`, `glyf`, `loca`, `hmtx`, `kern`, basic `GPOS` kerning). Quadratic outlines; CFF/cubic outlines later. |
| **T2** | **Glyph rendering.** Plan: CPU-rasterised glyph atlas for small UI text (exact coverage, cheapest on low-end GPUs), plus **Slug-style GPU outline rendering** for large, scaled, rotated or in-world 3D text. Slug evaluates glyph curves per pixel, so it's sharp at any scale or perspective with no atlas. Eric Lengyel dedicated the Slug patent to the public domain in March 2026 and published MIT reference shaders. We implement the algorithm ourselves. MSDF atlases are the fallback if Slug proves too costly on the integrated GPU. |
| **T3** | **Layout**: line breaking (Unicode line-break rules, simplified first), alignment, rich text (colour, bold, icons inline, e.g. controller glyphs), ellipsis, text input with caret and selection, IME for CJK input. |
| **T4** | **Scripts**: Latin, Cyrillic and Greek first. CJK: large fonts, atlas paging, fallback fonts. Then **complex shaping** (Arabic joining, bidirectional text, Indic reordering), a large job that HarfBuzz does for other engines; built only when a game needs those languages. |

## Input and focus

- **Gamepad-first navigation.** Every interactive element is reachable by d-pad or stick, with an explicit or
  automatic focus graph (nearest element in direction), plus focus scopes for modals. Unreal needed CommonUI for this;
  it should be built in from the start.
- **Correct glyphs.** Prompts show the active device's buttons (keyboard, Xbox, PlayStation, Deck), switching
  instantly when the player changes device. Steam Deck Verified requires controller access to all content with no
  settings change, and no non-controller glyphs when using the default configuration.
- **Legibility.** Steam's Deck guidance says interface text must be easily readable at 30 cm on an 800p screen. A UI
  scale setting plus `style.toml` minimum font sizes make this an automatic check.
- **Mouse, keyboard and touch** all map onto the same focus and activate model.

## Accessibility (built in, not bolted on)

- UI scale and text size.
- High-contrast and colour-vision themes (they're just themes).
- Subtitles and captions with speaker labels and background.
- Full input remapping, and hold-vs-toggle options.
- Reduced motion (disables UI animation and screen shake).
- **Screen reader** (later): expose the UI tree to Windows UI Automation. Godot 4.5 did this through AccessKit, a
  third-party crate. We implement the UIA provider ourselves. The UI tree is already structured data for agents (below),
  so most of the work is mapping it to UIA.

## Agent-facing tools: sight for UI

UI is easier to verify than 3D, because every element's rect, text and state is known exactly.

| Tool | Returns |
|---|---|
| `ui_tree` | The element tree: id, type, rect, text, enabled, focused, and clipped or overflowing flags |
| `ui_frame` | A screenshot with element outlines and ids overlaid (the UI version of `sight_frame`) |
| `ui_act` | Click, focus, navigate, type, by id. Drives headless UI tests. |
| `ui_check` | Measured checks: text overflow or truncation, minimum font size at the target resolution, contrast ratio, every element reachable by gamepad, untranslated strings |

These let a playtester agent run every menu path with a simulated gamepad, at Steam Deck resolution, in every
language, in parallel, without a human.

## The editor on the same toolkit

- **Panels:** outliner, inspector, journal timeline, claim map, review queue, cost dashboard. These are docked panels
  built with the toolkit.
- **3D viewport tools:**
  - click to select (using the M2 ID buffer for exact picking)
  - translate, rotate and scale gizmos with snapping
  - box select
  - painting (vertex colour, terrain sculpt and splat)
  - spline and path drawing
- **Every human edit is a protocol command.** It is journaled, attributed to the human, respects claims and is undoable,
  exactly like an agent edit (F5).

## Performance budgets (F3)

- A full HUD in under 0.5 ms CPU and 0.3 ms GPU on the reference integrated GPU.
- Glyph atlas memory capped, with least-recently-used eviction.
- Zero pipeline creation after load.

## References

- Clay layout library (Nic Barker): [overview](https://simonwillison.net/2024/Dec/21/clay-ui-library/). Flexbox-style
  layout in a single zero-dependency file.
- Eric Lengyel, Slug algorithm. Patent dedicated to the public domain March 2026 ("A Decade of Slug"; coverage:
  [Gigazine](https://gigazine.net/news/20260323-slug-font-text-rendering/)). Original paper: Lengyel, "GPU-Centered
  Font Rendering Directly from Glyph Outlines", JCGT 2017.
- Chlumsky, msdfgen / multi-channel signed distance fields (the fallback approach).
- Apple and Microsoft OpenType specifications (`cmap`, `glyf`, `GPOS`); Unicode UAX #14 (line breaking) and UAX #9
  (bidirectional text).
- [Steamworks: Steam Deck compatibility](https://partner.steamgames.com/doc/steamhardware/compat);
  [Godot 4.5 accessibility](https://godotengine.org/releases/4.5/).

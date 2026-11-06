# Design: UI and text

Status: researched 2026-10-13; the toolkit landed 2026-10-13 (`specula-ui`), the agent-facing tools and DPI, aspect
and safe-area handling in M5 (2026-11-06, below). The editor is built on them
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

### As built (M5, lane m5-uitools)

Code: `crates/specula-ui/src/agent.rs` (the toolkit's agent surface), `crates/specula-ui/src/display.rs`,
`crates/specula/src/ui_tools.rs` (arguments, schemas) and `crates/specula/src/ui_engine.rs` (the engine side).
Live proof: `scripts/ui_check.py` (with the review probes of R1 as checks) and `scripts/budget_check.py` step 5.
Permanent tests: `crates/specula-ui/tests/agent.rs`, `crates/specula/src/ui_engine/engine_tests.rs` (on a real engine),
the `ui_act` rows of `every_write_path_holds_the_policy`, `games/sepulcra-app/tests/ui_tools.rs`.

- **Surfaces.** The tools look at the hosted game's interface (`surface: "game"`, lent through `Game::ui`) and the
  engine's own screens (`review`, F9; `gallery`, F8). By default: the engine screen that is open, else the game's.
  Another toolkit user (the editor's panels) joins by adding a `Surface` variant and an arm in
  `Engine::with_ui` and `paint_ui_outlines`.
- **`ui_tree`** reads the last frame's layout when it was made for the current display (`layout: "last_frame"`),
  else lays out now with no input and puts the player's interface back exactly (`Ui::inspect`: layout, pointer,
  focus, hover, events, draw list and queued requests). Paged (`offset`, `limit` ≤ 200, `total`, `next_offset`);
  each widget carries `rect`, `text`, `enabled`, `focused`, `action`, `clipped` (+ `visible_rect`), `overflow`,
  `blocked` (a modal above takes its input) and its JSON `path`.
- **`ui_frame`** is `sight_frame` at the display's size (up to 4K), through the player's camera, with the interface
  drawn and the `ui` overlay: every widget with an id or that takes input outlined (cyan: takes input; yellow, thick:
  gamepad focus; red: disabled; grey: has an id; faint and unlabelled: under a modal) and labelled. The packet's `overlays.ui.outlines` come from the
  layout the capture drew (the game's capture UI, or an engine screen laid out for that size); an interface not laid
  out for the capture's size isn't listed. The capture UI takes the player's focus and list scroll
  (`Ui::copy_interaction_from`), and while the review or gallery screen holds the window every outline under it is
  `blocked` with no focus. `display` simulates another display (os_scale, safe_area, touch, size) for that capture
  alone: the game is told it for the capture and told its own again after.
- **`ui_act`** turns steps into synthetic input frames: `press` (UI directions, the game's input actions through
  their binding on the device, or raw sources such as `Pad.Start`), `focus` (the shortest d-pad route by the same
  focus rules a frame uses, re-planned after each press), `activate` (focus then accept; a HUD button's `shortcut`
  action on a gamepad; a click or tap at its centre for mouse and touch, refused as `covered` when something else is
  on top), `click`, `type`, `scroll`, `wait`. The game's frames run inside `Live::game_tool` as the caller, with
  `Frame::synthetic` set: the game makes only the writes the input causes (the commands a press sends), which commit
  as the agent's with claims checked, in one commit; what only presents (interpolating moving things, hover markers)
  waits for the window's next frame or a capture's `before_capture`, as the game's own writes (rows in
  `every_write_path_holds_the_policy`). The engine's screens are the owner's (`owner_only` for anyone else), and a
  game surface is refused (`engine_screen_open`) while one of them holds the window. At most 16 steps a call; the
  frames are budgeted by time (`ui_tools::BUDGET_MS`, 20 ms, and at most 160): before each unit that can't be cut
  (a frame, a press and its release, a click) the call's frames so far say what it would cost, and past the budget
  the answer has `stopped` (`time_budget` or `frame_cap`), `frames_left` (of an interrupted wait) and `rest`, the
  steps still to run, to send back. The first step unit of a call always runs (after a settle frame too), so every
  `rest` made progress. A playtest's `ui` step sends its rest itself at the same tick, so its input doesn't depend on
  the machine's speed; one still unfinished after 200 calls fails as `too_many_continues`, never counted done.
  Synthetic frames leave the game's presentation state alone: Sepulcra's "a window frame came lately" (which tells
  ticks to leave motion to frames), presentation time and the window title belong to real frames only (review R2:
  headless, a running clock's enemies stood still in the world while an agent called `ui_act`). The answer names each step's input, the actions the UI sent, refusals, the
  focus and the screens. The schedule counts `ui_act` as big work (it was a game tool's half millisecond).
- **Who may change the display.** Every override belongs to the session that set it and goes when the session
  closes (`Engine::ui_sweep`); answers name it (`display.set_by`). Headless, each session has its own virtual display
  (the game is told the caller's for its call's frames and the default one again after). A capture with no display
  of its own (`sight_frame {include_ui}`) draws the default one, never the last caller's. With a window, only the
  owner may override the window's display (the human sees it); other agents simulate displays with `ui_check` targets
  or `ui_frame {display}`. A call that fails or is refused keeps the display it had.
- **`ui_check`** runs `specula_ui::check_with` on a surface's screens at up to 8 targets (presets `deck`, `1080p`,
  `4k`, `ultrawide`, `16:10`, `4:3`, `1440p`, `phone`, or custom displays with ppi, OS scale, safe area and touch)
  times up to 4 UI scales (at most 16 layouts). Rules added in M5: `focus_scope` (a modal that traps the gamepad
  with nothing to press; a menu with no way out), `gamepad_access` accepts a HUD button whose action a menu screen
  a gamepad reaches offers (a menu shown now, or one whose `opened_by` actions have a gamepad binding or are sent
  by buttons on a reachable menu; a menu nothing opens, like a title screen never shown, is no path), and with
  `pseudo: true` every rule measures a pseudo-locale (the screens' own words accented and ~35%
  longer, in brackets) and `untranslated` warns where a label's words come from game state.
- **Playtests.** A step `{"ui": {...}}` is a `ui_act` call and `{"ui_check": {...}}` a `ui_check` call that expects
  `errors: 0` unless it gives its own `expect`.

### DPI, aspect ratio and safe areas (F6)

- `specula_ui::Display` is the surface a UI lays out for: size, ppi, the OS's scale for the monitor (Windows: DPI /
  96, from `specula_platform::os_scale_of`), safe-area insets and touch. `Display::viewport(theme, user_scale)` is
  the one way a game and the engine's tools compute a viewport, so agents measure what the player sees.
- **Scale:** `min(max(base, the OS scale), base x max_boost)` times the player's UI-scale setting (at most what
  keeps `min_logical`), where `base` is
  the scale by size against the Deck or by density. The OS scale is a floor: a player who asked Windows for 150%
  gets at least that (one OS logical pixel at desktop distance subtends about one Deck pixel at 30 cm). A theme's
  `scale.max_boost` caps how much the OS floor may enlarge the UI over `base`, at the largest boost its screens were
  checked at (Sepulcra: 1.5): a 1280x720 window on a 175% laptop gets 1.35 (948x533 logical, the compact layout),
  not 1.75 (731x411, smaller than any layout was made for). The player's setting multiplies after the cap: it is
  their explicit choice, and every step of it changes the size (with the cap on the product, every setting gave
  1.35 on that laptop). A theme's `scale.min_logical` is the smallest logical size its screens are made and checked
  for (Sepulcra: 800x480): the largest settings on a small window stop where it has no more room, instead of
  pushing menus off it (1280x720 at 175%: 0.8 gives 1.08, 1.0 gives 1.35, 1.5 gives 1.5, 853x480 logical).
- **Safe areas:** insets (each at most 40% of its side) shrink the area screens anchor in (`full_bleed` screens
  ignore them). Agents simulate TVs and phones with `ui_check` targets, `ui_frame {display}`, or headless their
  own `ui_act {display}`.
- **Aspect:** a theme's `scale.max_aspect` keeps the safe area to a centred band on wider screens (ultra-wide
  monitors keep the HUD's corners in view); 16:10 and 4:3 lay out by the smaller side.
- **The engine** tells the hosted game its display (`Game::display`): the window's (size and the monitor's OS scale,
  re-read four times a second, so moving the window to another monitor follows, with the owner's override), or
  headless the calling session's virtual one (the Deck's 1280x800 until its `ui_act {display}` sets another).

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

- A full HUD in under 0.5 ms CPU and 0.3 ms GPU on the reference integrated GPU, at the display a player gets
  by default (the owner, 2026-11-07: a 1280x800 window on the 175% laptop, 1.5x). How the overlay stays there:
  - quads grow by half their feather (they grew by `ceil(feather / 2) + 1` px: up to 40% more fragments on small
    glyphs, all blending nothing);
  - a rect whose middle draws nothing (a bevel, an outline) and a rect the next rect covers with opaque fill (a soft
    shadow under its panel) are drawn as whole-pixel bands around that middle (edge strips and corner squares around
    a plus shape; `specula_ui::gpu`), the
    same pixels with a fraction of the fragments (shadows and bevels were 0.12 ms each of 0.46 ms).
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

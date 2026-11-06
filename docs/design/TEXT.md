# Design: text (stages T2–T4)

Status: built in M5 (lane m5-text, 2026-11-06). The plan is in [UI.md](UI.md) ("Text"); this page
is how it was built, what was measured, and what is left. Code: `crates/specula-text`, the glyph
path in `crates/specula-gpu` (`sprite.rs`, `glyphs.rs`, `shaders/glyph.glsl`), in-world text in
`crates/specula/src/text_world.rs`, clipboard and IME in `crates/specula-platform/src/textinput.rs`.

## Fonts we ship

Owner's decision: SIL OFL only. Bundled (compiled in, `specula_text::Bundled`):

| name | face | role |
|---|---|---|
| `ui` | Alegreya Sans Regular | interface text (STYLE.md: a clean sans for the interface) |
| `ui-bold` | Alegreya Sans Bold | emphasis, rich text `[b]` |
| `display` | Alegreya (variable, default instance) | titles and carved inscriptions (STYLE.md: serif only for titles) |

All three cover Latin, Cyrillic and Greek with their extended sets, and Vietnamese, with `glyf`
outlines and GPOS pair kerning (which the parser already read). One family keeps sans and serif in
step. About 955 KB in all, byte for byte the upstream files (we only subset with our own code, and
there's no subsetter yet). `fonts/MANIFEST.toml` gives each file's source, upstream commit, version
(from its `name` table), size and SHA-256; each family's `OFL.txt` sits beside it. A unit test
checks sizes and licences; `scripts/text_check.py` checks the SHA-256 sums. This is the start of
M6's "licence metadata on every asset".

CJK isn't bundled: a pan-CJK face is 10–20 MB. Text that needs it falls back to the system's
faces (below). A game that ships CJK text will bundle a subset of an OFL face (Noto Sans CJK or
similar), which needs a subsetter of our own and a CFF parser (Noto CJK uses CFF outlines).

## T2: two renderers

- **CPU atlas** (unchanged, T1): exact-area coverage, quarter-pixel positioning, R8 pages with
  per-page LRU eviction under a cap. Small interface text: exact at its size and cheapest.
- **Slug** (new): glyph outlines evaluated per pixel. Large, scaled, rotated, perspective and
  in-world text, sharp at any distance with no atlas. Eric Lengyel's algorithm (JCGT 2017; patent
  dedicated to the public domain, March 2026), implemented here from the paper's description:
  - Per glyph: its quadratic curves (lines become quadratics) in em units, cut into up to 16
    horizontal and 16 vertical bands, each listing the curves that reach it, sorted by how far
    they reach so the per-pixel loop stops early.
  - Per pixel: a horizontal and a vertical ray. Which roots a ray crosses comes from the signs of
    the three control points (the `0x2E74` table), never from testing `t`; each crossing adds
    `saturate(x · ppem + 0.5)` (a one-pixel box filter); the rays are blended by their distance
    to the nearest crossing.
  - The reference is Rust (`SlugGlyph::coverage`, `coverage_from_words`); the shader
    (`shaders/glyph.glsl`) is a line-for-line port. Tests compare the reference with the
    exact-area rasteriser (mean difference under 0.03 at 48 px on every glyph tried) and the GPU
    with the reference (mean under 0.01, worst under 0.2, on both GPUs).
- **The store** (`SlugStore`, `glyphs.rs`): glyphs are appended to one word buffer, each prepared
  once; within a frame nothing moves, and the GPU copy grows by appending only new words (a steady
  scene copies nothing). It starts at 4 MiB (about 4,000 Latin glyphs, or about 1,090 CJK ones:
  Yu Gothic is about 960 words a glyph) and grows, doubling, up to 32 MiB (`MAX_STORE_BYTES`,
  about 16,000 CJK glyphs, as many as a frame draws). A glyph that didn't fit is **remembered**
  and never prepared again until the store has room. Below its maximum the store **grows** in
  place (same id: the GPU keeps what it has) as soon as a glyph doesn't fit. Between frames, a
  store that overflowed or is three quarters full of glyphs nobody used lately makes room: it
  grows while the last two frames' glyphs take more than half of it; else it
  **evicts** the least recently used glyphs (keeping the last two frames' and the most recently
  used others up to half the store), copying the kept words to their new places (`relocate`),
  never preparing them again, under a new id; else (a maximal store the scene fills) it stays,
  and the glyphs that didn't fit stay refused, so the scene converges instead of flickering
  (review of cd954bd: a refusal wasn't remembered and an overflow threw the whole store away, so
  24 paragraphs of CJK flickered and the last never drew). Preparing a glyph costs about 0.2 µs a
  word: window frames prepare at most 8,192 words of new glyphs (`WINDOW_PREP_WORDS`, about 2 ms);
  a capture that waits for its view (parked, like models and textures) prepares 16,384 words a
  pass of the engine loop (`CAPTURE_PREP_WORDS_PER_PASS`) until its text is ready, uploads the
  store's new words a piece at a time (`Gpu::preload_glyphs`, 4 MiB a pass, into one buffer at
  the store's cap) and snapshots the store once its glyphs are ready, so its frame copies little;
  at most
  32,768 more when it renders (`CAPTURE_PREP_WORDS`); what is left is drawn by a later capture
  and its packet says how many glyphs were not drawn yet (and how many were dropped). Every
  shader read is clamped inside the resident words, and a band walks at most 512 curves
  (`MAX_BAND_CURVES`, enforced when a glyph is prepared), so a hostile font can't read out of
  bounds or stall the GPU.
- **Where it draws:** glyphs are 2D items (`Draw2d::Text`), drawn in the scene pass with the
  sprites by a specialised copy of the blended sprite pipeline (`FEAT_GLYPH`): sprites keep their
  registers. Lit text takes the sun, ambient and point lights like a lit sprite. Sight's ID buffer
  draws glyphs with the same coverage (`ids_sprite.glsl`), so `visible_px` counts the letters, not
  their boxes, and the frame record lists each run's draw.

**MSDF** stays the documented fallback if Slug proves too costly on the iGPU. Measured cost is in
"Budgets" below.

## In-world text

The `text` component (shape `text`): text in the entity's local XY plane, like the other 2D
shapes. Fields: `text`, `font`, `size` (m per em), `align`, `max_width`, `line_height`,
`max_lines`, `anchor`, `lit`, `rich`, `z_layer`, `locale`, `editing`. The layout is made at a fixed
reference size (64 px per em) and scaled, so lines and bounds don't depend on the camera. It is
made where the component is set (the agent's connection thread for `world_set`, `world_spawn` and
`world_batch`) and kept in the component: frames, culling, sight packets and `text_edit`'s answer
all use that one layout, so what is drawn and what is measured can't disagree. The engine thread
lays a text out only when the window's typing changes it (`text_edit` lays out on the caller's
thread and the engine only commits). Texts a world file brings in (`world_load`, `save_read`,
start-up) aren't laid out where they load: the load thread lays them out (256 a batch), frames
take each batch up between loop turns, and a capture waits for those in its view (review of
cd954bd: 2000 texts took 1.95 s of layouts on the engine thread in `world_load`). A CJK text set while
its fallback face was still loading is laid out again by the loader thread once the face is in
(at most 65,536 texts wait). The engine takes those layouts up only between frames and calls,
and sight then reads those texts' subjects again, so within a frame or a capture and its sight
packet a text's box and glyphs come from one layout.
Sepulcra carves epitaphs into its headstones and its map title into a stone (`epitaphs.rs`).

## T3: layout and input

- **Line breaking:** a documented subset of UAX 14 (`linebreak.rs`): mandatory breaks, spaces,
  hyphens, no break before closing punctuation or small kana, opening punctuation and quotes kept
  with what follows, numbers with their signs and separators, breaks between ideographs. Not
  covered: Hebrew letters, regional indicator pairs, emoji modifier sequences, dictionary breaking
  for Thai/Lao/Khmer, the conditional Japanese starter (treated as strict). A word wider than the
  line breaks between characters, never inside a base character + combining mark cluster.
- **Overflow:** `max_lines` cuts the text and ends it with "…"; `wrap: false` keeps one line per
  paragraph (cut by the ellipsis, or reported as `overflow`). Layout reports every line's box,
  `truncated` (the byte where shown text stops), `overflow` and `missing` characters, for
  `ui_check`.
- **Rich text:** `[b]`, `[color=#rrggbb]`, `[icon=name]` (inline boxes the caller draws: controller
  prompts) and `[[` (`rich.rs`). Structured errors with byte offsets; `rich::plain` for screen
  readers.
- **Editing:** `TextEdit` (`edit.rs`): caret and selection on caret stops (a letter and its
  accents move together; CR LF is one stop), word and line movement, clipboard through a trait,
  undo, IME composition (shown at the caret, not text until committed), a byte limit. The same
  ops serve a keyboard, an on-screen keyboard (`OnScreenKeyboard`: gamepad and touch) and agents
  (`text_edit`). Agents get their own clipboard; only a human's window path uses the system one,
  on its own thread (below). `text_edit` reads the text from the engine, applies its ops and lays
  them out on the caller's connection thread, and the engine only commits the result (refused as
  `stale_version` if another writer changed it in between, when the ops are applied again to
  the new text, up to three times). `text_edit`'s undo steps back within
  one call (`world_undo` across calls); the window keeps its own history of the focused text
  (Ctrl+Z / Ctrl+Y). Line movement lays the text out only when it changed since the last
  layout; the call's cost is worked out first (below). `done: true` ends an edit (no caret
  drawn); Escape ends the window's.
- **Platform:** `specula-platform` declares `imm32` (the one crate that does): the composition
  becomes `ImeComposition`; the result becomes `ImeCommit` followed by its characters as `Char`
  events (so a game's text mode and the gallery's note get IME text too), then `ImeEnd`. AltGr
  (Ctrl+Alt) types characters, never commands. `Window::set_ime_position` exists but the engine
  doesn't call it yet (the window lives on the UI thread), and the system's composition window
  shows beside the inline one. The clipboard is `CF_UNICODETEXT` (global memory from
  `specula-sys`, which owns `kernel32`).

## The window's keyboard

The human's keyboard and clipboard are theirs (review of cd954bd: any agent could aim them at a
hidden text and read what was typed, a paste included). `text_window.rs`:

**The rule (review R3, lead direction; the owner approved window typing "so long as it's
needed"):** only the owner, a granted lead, or the human's own action in the editor directs the
window's typing, and a click never does. Three review rounds showed that working out on the CPU
what the human sees under a click drifts from what the renderer draws (R3: glass or a 2D item on
a higher `z_layer` *behind* a text paints over it, because texts are blended and write no
depth), so the surface is shrunk instead of the geometry made cleverer:

- **Who points it:** only the operator, through `text_edit {window_focus: true}` from an owner or
  lead grant (`lead_only` otherwise, `authority::Site::WindowFocus`; the authority drill probes
  it). `{window_focus: false}` or the human's Escape ends it.
- **A click never points it.** Any click or tap anywhere (any button) ends the typing, and the
  click goes to the game. There is no click-to-focus into a world text, whatever its `editing`
  says (R2: a builder's editable text behind a cube caught the click aimed at the cube; R3:
  glass, 2D shapes and texts behind an opened field caught it). **The human types into world
  texts through the editor** (F7): the inspector's **Text** field (`field:text`) types a text
  entity's words as the window's owner, one `world_set` with the inspected version as
  `expect_version` (an agent's change meanwhile refuses it, never lost).
- **Anyone else's write drops it.** When the focus begins, the versions of the text and its
  ancestors are recorded (`text_window::watch_of`). The operator's own `text_edit` and the
  window's typing move the text's on (only from the version recorded, so a foreign write just
  before them still counts); any other change to them (a builder's `world_set`, `text_edit`,
  undo, a parent moved) drops the focus at the next window frame, before its keys
  (`foreign_write`). A few lookups a frame: never a walk of the world (R3, major 2: the old
  occlusion walk cost 8-11 ms per request and 1.8 ms per held check at 100k entities).
- **When else it drops:** the text is deleted, stops being a text, is hidden, under half opaque
  (`MIN_FOCUS_OPACITY` 0.5), off screen or less than 4 px tall on screen; an engine screen (the
  editor, a gallery, the review screen) takes the window (`engine_screen`: the keys go there).
  A text that can't hold it can't take it (`not_visible`, with `reason`). What the frame drew
  over the text is **not** judged: no CPU occlusion test, the banner is the guarantee.
- **The banner:** while it holds, an opaque banner just above the text (below it when there is
  no room) says "Typing into /path — Esc or any click stops" (the bundled UI face, a cyan
  edge), and the window's title says so. It is the **last** of the window frame's overlay, so it
  draws over everything: the world, 2D layers, glass, the game's interface and the engine's
  screens. When the overlay is full, what was drawn before it yields its room
  (`text.focus_cue_room`). **No key lands until a frame has sent the banner last in its overlay**
  (measured on the overlay the frame sends: `WindowTyping::overlay_sent`); a frame that can't
  draw it drops the focus (`no_banner`). `engine_info`'s `window.typing` reports the text and the
  banner (`quads`, `last`, `frames`). Traces: `text.window_focus` (who took it),
  `text.window_focus_dropped` (why).
- **Only the real field shows a caret** (review R4): any agent may set `editing`, so in the
  window a caret, selection or composition is drawn only on the text the keyboard types into
  (`TextGpu::window_marks`); captures still draw every text's marks for agents. A builder can
  still paint a fake caret and a fake banner as world content, so **the window's title bar is the
  cue world content can't fake**: while the keyboard types into a text it reads
  "… — Typing into /path — Esc or any click stops", and without it no key reaches any text.
- **Screens agents open** (review R4): while the human types into a text, only the owner or a
  granted lead puts a gallery (`gallery_open`) or the review screen (`review_screen`) on the
  window (`window_typing` otherwise). When one does, the typing ends before that screen reads the
  frame's input, and while the key guard holds the keys go neither to the game nor to the screen
  (`text_window_screens`). Opening the editor (F7) is the human's own leaving: no guard.
- **Keys after a drop:** when the focus drops by itself (not the human's click), what the human
  goes on typing was meant for the text: keys, characters and IME events are withheld from the
  game and its modules while they keep coming (gaps under 0.6 s, at most 5 s; traced
  `text.window_keys_withheld`); a click or Escape ends that at once. A module binding every key
  never sees the rest of a password.
- **The clipboard** is read and written on its own thread (`specula-clipboard`): a delayed-
  rendering owner can block a read for as long as it likes. A paste or a cut waits for the
  thread's answer through a `jobs::Reply`, and the keys after it wait with it, in order; a cut
  deletes the selection only once the clipboard holds it; an answer that doesn't come within 2 s
  is given up on (`text.clipboard_failed`, and clipboard keys are refused until it answers).
  Failures are traced, never dropped.
- **Keys** past a frame's 256 ops or its layout share (`WINDOW_LAYOUT_CHARS`, two layouts of the
  longest text) wait for the next frame; past 4,096 waiting keys they are dropped and traced
  (`text.window_typing_dropped`, `queue_full`).
- **Siblings** (review R2's sweep; every agent is untrusted): `window_view`, named cameras,
  `stream_fly` and timeline rails move the window's camera only, and the human's first key, click,
  scroll or touch takes it back; they redirect no input. `ui_act` drives synthetic frames and never
  touches the window's input. The editor opens only from the human's keys or pad, and only the
  owner drives it (`editor_input`). The review screen (reviewers) and a gallery (its maker,
  `gallery_open`) take the window's input only as a visible engine screen; open questions there
  are listed in the lane's report. Games get keys through their modules and live Lex rule sets by
  design (F6), never while a text holds the keyboard or right after it dropped by itself.

## T4: scripts

- **Latin, Cyrillic, Greek:** the bundled faces, with `kern` and GPOS pair kerning.
- **Fallback per locale:** `system_fallback_candidates(locale)` orders the system's CJK faces by
  the locale (Han unification: Japanese text must find a Japanese face first); without a locale,
  the characters pick it (kana: Japanese, hangul: Korean, other Han: Simplified Chinese). Fallback
  faces load on their own thread; until one is in, its characters report as missing.
- **CJK atlas paging:** for interface text, the CPU atlas's pages are evicted
  least-recently-used under its cap (`AtlasConfig::max_pages`), never a page the current frame
  used. In-world text uses the Slug store instead (its cap and policy above).
- **Bounds and determinism:** text bounds depend on which fallback faces the machine has and when
  they finish loading, so they are presentation only: navigation treats text like models (its
  collider or origin, never its bounds), and nothing simulated reads them.

## Not built (design notes)

Complex shaping is built only when a game needs those languages:

- **Arabic joining:** per character, its joining type (`ArabicShaping.txt`: right, dual, causing,
  transparent) picks the isolated, initial, medial or final form, through the font's GSUB `init`,
  `medi`, `fina`, `isol` lookups (plus `rlig` and `calt`); then mark positioning (GPOS mark-to-base,
  mark-to-mark). Needs a GSUB reader (single and ligature substitution first), and line breaking
  must run on the shaped run.
- **Bidirectional text (UAX 9):** resolve embedding levels per paragraph, break lines on the
  logical order, then reorder each line's runs visually; carets move logically but draw at
  visual positions (a caret at a direction boundary has two places). The editing model's byte
  offsets stay logical, which keeps `TextEdit` unchanged.
- **Indic:** syllable clusters (consonant + virama + consonant…), reordering pre-base matras and
  reph, then GSUB features in order (`nukt`, `akhn`, `rphf`, `blwf`, `half`, `pstf`, `vatu`,
  `cjct`, then presentation forms). The largest of the three; HarfBuzz's tables are the reference
  behaviour.
- **Vertical CJK** (`vert`/`vrt2` substitution and vertical metrics) and **CFF outlines** (cubic
  charstrings, needed for Noto CJK and most macOS/Linux system CJK faces).

## Cost before work

Every path that lays out, shapes or rasterises agent-supplied text works out its cost from the
parsed input before doing any of it, refuses what is over budget with a structured error that
names what is too big, and never does more than the model said. Each has a test that feeds the
worst input the schema allows.

| Path | Cost model | Budget | Over it | Test |
|---|---|---|---|---|
| `text_edit` ops | layouts × longest text (bytes): one layout per line movement after a change, plus the refreshes; inserts and compositions grow the longest text, a paste counts as the longest allowed. On the caller's thread; the engine only commits | `EDIT_LAYOUT_CHARS` = 8192 (about 4 layouts of the longest text: 2 ms of Latin, under 1 ms of CJK, in release) | `limit` error (`field: ops`, `limit`, `got`) | `line_movement_costs_what_the_model_says_and_too_much_is_refused` |
| A frame of the window's typing (engine thread) | the same model over the frame's keys, each Ctrl+Z / Ctrl+Y one more layout | `WINDOW_LAYOUT_CHARS` = 4096 a frame (about 1 ms), at least one key | the rest wait for the next frame (never dropped below 4,096 waiting keys) | `keys_past_a_frame_are_carried_to_the_next_never_dropped_silently`, `a_paste_is_a_barrier_and_the_layout_budget_carries_the_rest` |
| The `text` component (`world_set`, `world_spawn`, `world_batch`) | one layout and one ink box per text, on the caller's thread; at most 2048 bytes, 64 lines. Ink boxes read each glyph's `glyf` header (ten bytes), never its outline | the schema caps | `invalid_value` naming the field | `the_largest_component_lays_out_once_within_its_cost`, `ink_boxes_come_from_glyph_headers_and_hold_the_ink`, `header_boxes_hold_the_ink_of_every_bundled_glyph` |
| World files' texts (`world_load`, `save_read`) | none where they load: the load thread lays them out | 0 layouts on the loading thread | | `a_world_files_texts_are_laid_out_by_the_load_thread_not_where_they_load` |
| Frames, culling, sight packets | none: they use the component's layout | 0 layouts per steady frame | | `frames_and_bounds_draw_the_components_own_layout` |
| Rich markup | linear: spans open and close as the bytes go by (it was characters × spans: 64 KiB with 1024 spans was 64 M checks a layout) | 64 KiB, 1024 spans, depth 16 | `RichError` with its byte | `the_largest_markup_lays_out_within_its_cost`, `the_span_sweep_matches_looking_each_span_up` |
| CPU atlas glyphs (HUD, UI) | the glyph's pixel box from its outline's bounds, before rasterising | a page (1024 px); sizes clamp to 1024 px | `glyph_too_large`, then the glyph is remembered as ink-less (no work every frame) | `oversized_glyphs_are_refused_before_rasterising_and_remembered` |
| Slug glyphs | curves per glyph and per band from the outline; words prepared, counted as they are made | 4096 curves a glyph, 512 a band, 16 bands a way; a store of 4 MiB growing to 32 MiB; 8,192 words of new glyphs a window frame, 16,384 a parked capture's pass, 32,768 a capture's render (each overshoots by one glyph at most); 16,384 glyphs a frame | `SlugError`; the glyph is left out (a refusal for want of room is remembered) and the packet says how many were not drawn yet or dropped | `slug.rs` tests (`a_scene_larger_than_a_full_store_converges_without_resets`, `a_growing_store_grows_in_place_then_evicts_what_went_unused`), `captures_prepare_glyphs_within_their_budget_and_say_what_waits`, `window_frames_of_a_scene_larger_than_the_store_converge`, the fuzz tests |
| Line breaking | linear in the text | | | `line_breaks_of_hostile_text_hold_their_invariants` (a 68,027-character text) |
| Font files (bundled; system CJK faces) | table directory, `glyf` points, GPOS index reads, bitmap sides all capped before allocating | `MAX_POINTS`, `MAX_GPOS_INDEX_READS`, `MAX_BITMAP_DIM`, `MAX_SDF_DIM` | `FontError` with table and offset | `fuzz_tests.rs` |

A CJK fallback face is read on its own thread; a read that fails for a reason other than a
missing file is tried again twice (`text.fallback_failed` traces each failure), so one bad moment
(a machine out of commit memory while builds run) doesn't leave text without its face for the
rest of the run.

## Budgets

Measured 2026-11-06 by `scripts/text_check.py` on the Intel iGPU (RaptorLake-S, ordinary desktop
apps open, six other lanes building):

| What | Budget | Measured |
|---|---|---|
| Sepulcra's HUD text, CPU (`game_overlay` zone, p95; four runs) | 0.5 ms | 0.05–0.14 ms |
| Sepulcra's HUD text, GPU (overlay pass, p95; four runs) | 0.3 ms | 0.19, 0.22, 0.79, 0.83 ms |
| Sepulcra's 2D pass with the epitaphs and title (sprites pass, p95) | | 0.06–0.73 ms |
| Sepulcra frame GPU p95, 300 frames | 14 ms | 5.5–7.8 ms |

The overlay pass is bimodal across runs: the two slow runs were also the ones whose whole frame
was slower (6.8 and 7.8 ms against 5.5 and 6.2) while other lanes' engines shared the iGPU; the HUD
itself only changed face (Alegreya Sans for Segoe UI). Re-measure on a quiet machine before
calling the 0.3 ms budget met or missed. The glyph store's GPU memory (`perf_memory`, `glyphs`)
is capped at 4 MiB of words; the text check's scene (six scripts and a CJK paragraph) uses 0.3 MB.

Slug is cheap enough on the iGPU that the MSDF fallback isn't needed. GPU coverage matches the
CPU reference within a mean of 0.01 on both GPUs (`crates/specula-gpu/tests/text.rs`).

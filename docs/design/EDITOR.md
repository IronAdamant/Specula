# Design: the human editor

Status: M5 core (2026-11-06): editor mode, docked panels, outliner, inspector, click and box select, gizmos with
snapping, undo and redo. Wave 2 adds the draw tools and the swarm panels (below, "Where wave 2 plugs in").
Plan: [ROADMAP.md](../ROADMAP.md) M5 "Human editor on the protocol"; toolkit: [UI.md](UI.md); rules:
[WRITE_POLICY.md](WRITE_POLICY.md), [AUTHORITY.md](AUTHORITY.md), [TOOLS.md](TOOLS.md).

## The principle (F5)

**Every human edit is a protocol command.** The editor is a client of the tools agents use. It has no private path into
the world: a move is a `world_set`, a delete a `world_batch`, an undo a `world_undo`, and each goes through the engine's
one request handler (`Engine::handle`), the write policy and its rule (`Rule::World`), exactly like an agent's call.
So each human edit is:

- **journaled and attributed** to the window's operator, `window:owner` (the name M4's review screen already uses);
- **subject to claims**: an agent's claim refuses the human's edit, and the editor says whose claim it is;
- **versioned**: every edit carries `expect_version` from the inspector's or the drag's start, so a change an agent
  made meanwhile is a `stale_version` conflict, shown as such, never silently overwritten;
- **undoable**: through `world_undo` (the human's own entries), and redone by undoing the undo.

Agents see human edits in `world_changes` and `world_history` like any other.

### The window's operator

The window's human is an identity the engine admits at start-up: a swarm agent whose session id is `window:operator`
(never a protocol session id, which is 32 hex characters) and whose name and identity key are `window:owner`. Its
authority is `Owner`, from `authority::authority` like any agent's: the human at the window is the owner
(`authority.rs`). It is never self-declared: `agent_hello` refuses the reserved name `window:owner`, and the identity
is not pruned with dead sessions. Claims still bind it: authority decides who may override *objects* (force), never
whose claim a write may cross.

### Agents driving the editor

Agents see the editor with `editor_state` (read only: open, mode, selection, gizmo handles on screen, panels' widget
boxes, the outliner page, the inspector, the last result). Until the uitools lane's `ui_act` lands, they drive it
with `editor_input`: simulated window input (pointer moves, presses, drags, keys, text) fed to the editor exactly as
the human's would be. Its edits are the window operator's, so `editor_input` needs **owner** authority (an operator
grant, `--owner NAME`): it acts as the human's hands. Each injection is traced (`editor.input`, with the agent's
name), and `editor_state` names the last agent that drove it. Its limits:

- **The editor's alone.** Injected frames go to the editor only, never to the review or gallery screens or the game,
  whose decisions would otherwise be recorded as the human's. They wait while another screen holds the window or the
  window is minimized; a call past its deadline has its remaining frames dropped, never run later.
- **One call, one commit** (AGENTS rule 14): an `editor_input` call makes at most one edit, charged to that tool
  call; a second edit in the same call is refused in the status line. Send each edit in a call of its own.

## The window

- **Toggle:** `F7`, or `LB + RB + Start` on a gamepad (F8 is the gallery, F9 the review screen). The review and
  gallery screens come first: while either is open the editor gets no input.
- **While open** the hosted game gets no window input and is told the window is modal (`GameCall::ModalChanged`), so
  Sepulcra pauses. The window camera is the editor's own (orbit, pan, zoom), started from the game's view; closing the
  editor hands the camera back. While it is open the editor holds the window as a human's input does: `window_view`
  is refused (`editor_open`), an agent's earlier view or a `stream_fly` lets go of it (`released_by: editor` in
  engine_info), so what the human clicks is what the window shows. The game's own interface (its HUD and menus) is not drawn while the editor is open:
  the viewport shows the world the human edits, and the pick render leaves the interface out too, so a click is never
  stopped by a HUD panel the human can't see.
- **Docked panels** (`crates/specula/ui/editor.json`, theme `editor_theme.json`: screens and themes are data):

```
+------------------------------------------------------------------------------+
| Move Rotate Scale | World/Local | Snap 0.25 m 15 deg x0.1 | Undo Redo | Close |  toolbar
+---------------+----------------------------------------------+---------------+
| Outliner      |                                              | Inspector     |
| search [   ]  |                                              | name, path    |
| > /slab       |              viewport                        | v12 by ann    |
|   > stones    |   (click select, box select, gizmos,         | claim: none   |
|     stone_1   |    orbit / pan / zoom)                       | position -x+  |
|   ...         |                                              | rotation -x+  |
| page 1/7 < >  |                                              | scale    -x+  |
|               |                                              | colour, shown |
+---------------+----------------------------------------------+---------------+
| status: what the last edit did, or why it was refused (whose claim)          |
+------------------------------------------------------------------------------+
```

Everything is reachable by gamepad focus navigation (toolbar, rows, inspector steppers); the viewport takes the
pointer and touch.

## Outliner

- The entity tree in path order, a page of rows at a time (`PAGE_ROWS` = 18), with expand and collapse per row.
  Pages walk the world lazily from a resumable cursor (a stack of `(parent, last child name)`, each step one ordered
  range read), so a page costs its rows, not the world: a world of a million entities pages like one of ten.
- **Search** (typed text): a flat list of matches by name, scanned for at most `SCAN_BUDGET` (0.5 ms) per frame,
  and at most `SCAN_MAX_STEPS` entities, then resumed the next frame, so a search over a huge world never stalls a
  frame. The walk (`outline.rs`) allocates nothing per entity: one ordered range of the world's child index per
  sibling run, and a name copied per level only where a frame's slice stops; it reads the clock every 256 entities.
  While a search runs without new matches the panel shows its progress at most ten times a second.
  `tests/outliner_alloc.rs` pins a 100k-entity search: no allocation per entity, at most 12 per slice, and slices
  that stop at their time budget. `#tag` matches entities carrying that tag, with the same budget.
- Each row shows the name (indented by depth), whether an agent's claim covers it (and whose), who last changed it,
  and `game` for the hosted game's transient entities (which no tool may edit).
- Rebuilt only when the world's revision, the claims, the filter, the page or the expanded set change, and at most
  four times a second.

## Inspector

- The primary selection's name, path, id, version, creator and last editor, its claim, and its components.
- **Typed editors:** position, rotation (degrees) and scale as three numbers each (step buttons by the snap step, or
  click and type a value), colour (three numbers, 0..1), visibility (a toggle) and the name (type a new one). Each
  commit is one `world_set` with `expect_version`.
- **An edit's version is the one it began at.** Typing into a field records the inspector as it is then (the entity's
  version and values); Enter sends that version as `expect_version` and the other components as they were. An agent's
  change made while the human typed refuses it (`stale_version`, naming who changed it): it is never overwritten
  silently. A step button, the visibility toggle and Delete begin and end with their press, so their base is what
  the inspector shows at the press; a gizmo drag records each entity's version when the drag begins.
- **Conflicts and refusals show plainly** in the status bar: `claimed` names the holder, the claim and its time left;
  `stale_version` says who changed it since; `transient` says the hosted game owns it.

## Selection

- **Click:** exact picking from the ID buffer. The click renders the window's view with IDs offscreen through the
  capture flight (`Job::Pick`: never a wait on the GPU on the engine thread) and reads the entity at the pixel (or the
  nearest within 3 px); a click on nothing clears the selection. Shift adds or removes.
- **Box select:** a drag from empty viewport space draws a rectangle; on release the same pick render takes every
  entity **mostly inside** it: at least half of its visible pixels in the box (`BOX_INSIDE`). Visible pixels only, so
  what the human sees is what they get, and the ground or a slab the box merely overlaps is not taken along (a drag
  would move it). Most pixels first, at most `MAX_SELECTION`.
- The cyan selection mark (STYLE.md's highlight rule: saturated `#3fe0d0`, never the pale tone) outlines each selected
  entity's box in the overlay.

## Gizmos

- **Translate** (W): three axis arrows and three plane handles. **Rotate** (E): three rings. **Scale** (R): three
  axis handles and a centre handle for uniform scale. Handles are picked in screen space (distance to the projected
  axis, ring or square), so they stay the same size in pixels at any distance.
- **Space** (L): world or local axes. **Snapping** (G toggles; Ctrl held inverts it for a drag): translation to
  `0.25 m`, rotation to `15` degrees, scale to `0.1`. The drag's delta is snapped once, so the preview is exactly what
  the release writes: a world-space move puts the gizmo's entity on the grid along the dragged axes, a scale puts its
  scale on the step, and the rest of the selection moves with it rigidly.
- **A drag is one edit.** While dragging, the window draws the selection at the preview transform: the frame's draw
  list is patched for the dragged entities and their children (their model matrices and bounds), so the real mesh
  moves on screen. The draws' owners come from the draw cache (`DrawCache::draw_list_owned`) only while a drag
  previews; other frames pay nothing. Nothing is written to the world, so the preview cannot flood the journal, break claims or show
  agents a half-done move. On release, **one** `world_batch` (one `set` per selected entity, each with
  `expect_version`) commits the whole drag: one journal entry, one commit per action. Escape or a right click cancels, and
  so does anything that interrupts the drag (another screen over the editor, the window losing focus or being
  minimized): an interrupted drag is never committed half done.
- **Claims before the drag:** the drag starts only when no other agent's claim covers a selected entity (a look-ahead,
  `Claims::blocking`, over each entity's path and position); otherwise the status says whose claim it is. The commit
  is checked again, authoritatively, by the write policy.
- **A drag never spans a world replacement.** It records the world generation it began in; a `world_load` or
  `save_read` cancels it (see "World replacement"), and a release in another generation writes nothing.

## Undo and redo

The editor keeps the journal ids of the human's own edits. Ctrl+Z is `world_undo {ids: [last]}`; Ctrl+Y (or
Ctrl+Shift+Z) undoes that undo (the journal's redo). A refused undo or redo is decided by the error's own `retry`
flag (every error answer carries it; `response::retryable`), never by a list of codes in the editor:

- **lasting** (`retry: false`: already undone, too old, not found, changed since, not yours) sets the entry aside, so
  Ctrl+Z (Ctrl+Y) goes on with the next one;
- **transient** (`retry: true`: another agent's claim, a busy engine) keeps it on top to try again.

Forcing over an agent's later change is a lead's or the owner's call, made with `world_undo force` explicitly.

## World replacement

A `world_load`, a `save_read` or a writer's bug the write policy recovered from replaces the world wholesale, and
entity ids restart: an id held from before names a different entity. Every such path goes through one hook,
`Engine::world_changed_wholesale`, which calls `editor_world_replaced`: the drag is cancelled, the selection,
inspector, outliner (cursors and expanded rows) and a pick in flight go, commands not yet sent are dropped, and undo
and redo start over (their journal ids are the old world's). The same hook resets the engine's other holders of ids
(merged HLOD proxies, the review screen's shown submission, live Lex runs, sight_compare's marks by generation);
`tests/wholesale_guard.rs` holds every `Engine` field to "reset in the hook", "keyed on the world generation", "holds
no ids" or "holds stable identities" (below), so a new field fails it until it is classified.

## Stable entity identity (references that outlive a world replacement)

Board notes, critiques and task-board `entities` are kept across a `save_read` (the board and the review queue stay
while the world is replaced), and with the world in its folder. A bare entity id can't name an entity across that:
ids are only unique within one run of the id counter, and loading an older save winds the counter back, so a later
spawn takes an id an old note holds. Review rounds R2 and R3 of this lane tried to fake identity by following
paths through the journal (renames, moves, re-attaching by path), and each round found another hole: a rename in
the same serve window, a delete, a module's rename, and a cost of every rename times every reference on the
engine thread. So a reference holds an identity instead, and paths are only shown.

- **The identity** of an entity is `(lineage, id)` (`world::EntityUid`). A world has a *spawn lineage*, a random
  64-bit value; an entity records the lineage it was born in (`Entity::born`) and keeps it for life: through
  renames, moves, any writer's changes (agents, the human, modules, the hosted game), saves and loads.
- **Never reused.** Within one lineage ids only grow: the counter never goes back (a failed batch rolls back its own
  spawns inside the call that made them, before anything could refer to them). Every way a counter can go back
  starts a new lineage: `World::default` and `World::clone` draw a fresh one (a loaded file, a save read, a journal
  replay, a playtest copy), and the write policy forks the live world's lineage whenever it is replaced (`world_load`,
  `save_read`, a writer's world recovered). Entities loaded from a file keep the lineage they were born in. So no
  newly spawned entity can ever take an old reference's identity, whatever save is read in whatever order.
- **Fresh values** come from `std`'s per-process random SipHash keys over a process-wide counter (not cryptography:
  only distinctness matters; two engines drawing the same 64-bit value is about as likely as a hash collision). 0
  means "unknown" and matches nothing.
- **Determinism: metadata, outside the simulation.** The lineage is in no hash (`content_hash`, `world_hash`), in no
  tool's entity JSON and never read by simulation code, so replays and `world_hash` are unchanged. Journal snapshots
  carry it, so an undone delete (or a replayed spawn) brings the same identity back.
- **Persistence.** World files save each entity's `born` (16 hex digits); the format is `specula.world/v2`. Version
  1 files still load: their entities get the loading world's fresh lineage (fresh identities). Saves embed world
  files, so the save format is version 2, with a v1 to v2 migration that changes nothing (its v1 world files load as
  above). Board and review files save a reference as `{id, born, path}`; one without `born` (an older file) is
  detached.
- **References** (`entity_ref::EntityRef`: id, lineage, last known path). Adding one is O(1) plus its path. It is
  *attached* while the world has an entity with that id born in that lineage, which is one map lookup; otherwise
  it is *detached* and shown as `{path: <last known>, detached: true}` with no id (the id may name another entity
  now). A delete detaches it; undoing the delete attaches it again; it never moves to whatever later takes its path.
- **Paths are looked up when shown.** Listings (`note_list`, `critique_list`, `task_get`) show an attached
  reference's current path, worked out for the page's items only; `path_prefix` filters resolve the prefix once and
  ask whether it is an ancestor (no allocation per reference). The last known path is refreshed then, when added,
  and once before a world replacement (O(references x (log n + depth)), with the load).
- **Nothing runs per loop or per write.** No journal walk, no hook in the write policy, nothing while the world is
  idle or the game ticks. Concurrent renames can't race a reference, because a rename doesn't change identity.

## How actions map onto tools

| Action | Tool and arguments |
|---|---|
| gizmo drag released | `world_batch {ops: [{op: set, id, position / rotation_deg / scale, expect_version}, ...]}` |
| inspector value | `world_set {id, position / rotation_deg / scale / color / visible / rename, expect_version}` |
| Delete | `world_batch {ops: [{op: delete, id, expect_version}, ...]}` |
| Ctrl+Z / Ctrl+Y | `world_undo {ids: [seq]}` |
| click, box select | none (a read: the pick render) |
| outliner page | none (a bounded read of the world's index) |

## Agents driving the editor (`editor_input`)

- Owner only (`owner_only`); its frames go to the editor alone, one per window frame.
- **Attribution.** An edit its frames make is the window's human's (author `window:owner`, undoable by the human),
  and it records the agent too: the journal entry's `via` is the agent and its `call` is the `editor_input` call
  (`world_history`, `trace_call`); the `write.committed` trace names `via`. One call makes at most one edit (one call,
  one commit).
- **Refusals are structured.** While the review (F9) or gallery (F8) screen holds the window, or it is minimized,
  the call is refused at once: `window_held` (`held_by` names the screen; `retry: true`). Frames that didn't run by
  the call's deadline are dropped, never run later as the human: `input_dropped` with `frames_run` and
  `frames_dropped`.

## Performance (F3)

- The panels are one `specula-ui` screen (its draw list is allocation-free once warm) drawn on the overlay after the
  review screen. `Ui::frame` prepares and lays the screen out every frame the editor is open (that is what reads the
  pointer); the screen's state (`fill`) is rebuilt only when the editor's data changed.
- The outliner and inspector are rebuilt on change, not every frame; the outliner's page and search are bounded.
- The pick render costs one offscreen view, only on a click or a box release.
- The gizmo is at most a few hundred overlay triangles.
- `editor_check.py` measures the window's frame time with the editor open against `perf.toml`'s budget on the iGPU.

## Where wave 2 plugs in

- **Panels** are screens in `editor.json` (or more screens loaded by `EditorUi::load`): the journal timeline, claim
  map, task board, intents and notes, and the cost and progress dashboard each add a dock panel whose state is filled
  from the engine's own stores (`Swarm::journal`, `claims`, `board`, `intents`) in `editor_engine.rs` on change, like
  the outliner. Panels toggle from the toolbar.
- **Tools** (`Tool` in `editor.rs`): a tool owns the viewport's pointer while active, like the gizmo does. The draw
  tools (vertex paint, splines and paths, terrain sculpt and splat) are tools whose strokes preview in the overlay and
  commit on release as one protocol command each (`world_set` of the entity's data, `terrain_edit`, `project_write`
  of a spline recipe), through the same `editor_engine::send` path, so they are journaled, claimed and undoable with
  no new write path.
- **Overlays** (claim map, journal "who built this" tints) are painted in `Engine::editor_overlay` with the same
  painter.

## Not yet

- Multi-entity inspector editing (the inspector edits the primary selection; gizmos move them all).
- Gamepad gizmo drags (the inspector's steppers are the gamepad's way to move things).
- Live preview of 2D entities (sprites, tilemaps) while dragging: their draws aren't patched; they move on release.

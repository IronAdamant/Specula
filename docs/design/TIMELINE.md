# Design: timelines, the sequencer and camera rails

Status: designed and built 2026-11-06 (M5 lane "timeline"). Code: `crates/specula-tempus` (the format, rails,
evaluation, motion checks), `crates/specula/src/timeline.rs` (playback on the tick, the window, the game's hooks),
`crates/specula/src/timeline_tools.rs` (the `timeline_*` tools, previews and rail checks), Sepulcra's
`games/sepulcra-app/src/cutscene.rs` and `games/sepulcra/assets/timelines/`. Proof: `scripts/timeline_check.py`.
Foundations **F1** (agents author cutscenes as data, with claims, versions, the journal and undo), **F2** (evaluated on
the fixed tick, scrubbing equals playing), **F3** (a binary search per track per tick, and a commit only on ticks
that change something; a timeline is parsed when its file is validated, off the engine thread) and **F5** (a human's
input always takes the window back).

**Tempus** (Latin: time) is the crate's name.

## What agents get

- **Timelines are project files**: `*.json` in a folder named `timelines` under the assets mount (kind `timeline`,
  [PROJECT_DATA.md](PROJECT_DATA.md)). They are written, patched, diffed, claimed, journaled and undone like every
  other project file; `timeline_set` is a validated `project_write`. A file is at most 256 KiB.
- **Tracks** (`specula.timeline.v1`):

  | Kind | What it holds | What playing it does |
  |---|---|---|
  | `camera` | a rail (2..64 points, open or closed), `move` keys (how far along, 0..1 of its arc length), `look` (a point, an entity, a second rail, or ahead along the rail), `fov` and `roll` (a number or keys) | drives the window camera (`window`), previews and checks |
  | `entity` | a target path and channels `position`, `rotation`, `scale`, `visible`, `color`, `emissive`, each keys `{t, value, ease}` | writes the entity each tick its values change |
  | `anim` | keys `{t, anim}`: what `world_set` takes for `anim` (a clip, `{clip, ...}`, `{graph, params}`, or `false`) | sets the entity's anim, starting at the key's own time |
  | `vfx` | keys `{t, vfx}`: what `world_set` takes for `vfx` | sets the entity's effect, starting at the key's own time |
  | `audio` | cues `{t, sound, volume, at (a point or an entity), bus}` | a typed cue for the audio service (the seam: the hosted game hears it now) |
  | `subtitles` | lines `{t, duration, text, speaker}` | shown in `timeline_get`; each line's start goes to the game |
  | `events` | `{t, name, data}` | events for the hosted game |

- **Eases** between keys (the curve on the key it arrives at): `linear` (default), `in`, `out`, `in_out`
  (smoothstep), `smooth` (smootherstep: zero speed and acceleration at both ends), `step` (a hold, then a jump).
- **Several cameras cut.** Each camera track is in use over its span (first to last `move` key); where spans
  overlap the last listed wins, between them the last to end holds. A change of track is a cut: checks don't measure
  across it.
- **Roll stays zero** unless a track asks for it; the engine camera gained `roll_deg` (0 for every other camera, which
  leaves their view matrices bit for bit as they were).
- **Errors are structured**: `{error, field: "tracks[0].move[3].t", message, allowed?, value?}`. Unknown keys name the
  closest known one; a key given twice is refused (`duplicate`), never guessed.

## Rails

Centripetal Catmull-Rom (alpha 0.5) through the points: it never loops or kinks between close points. Open rails get
mirrored end controls (the curve leaves the first point heading for the second); closed rails wrap. Rails are
**parameterised by arc length**: a table of 48 chords per segment is built once, `at = 0.5` is half the rail's length
in metres, and equal steps of `at` move equal distances (tested to 2%). A position is a binary search in the table and
an evaluation on the curve itself.

## Deterministic evaluation

- **Pure functions of time.** `specula_tempus::eval` gives the camera, entity values and the anim or vfx key in force
  at `t` from the data alone (and, for a camera looking at an entity, where the world puts it). IEEE `+ − × ÷`,
  `sqrt` and `dmath` only: the crate carries the determinism lints.
- **Positions are ticks.** A playback's position is a whole number of ticks; it moves one tick per simulation tick and
  never while the clock is paused (`time_control` steps it). Its time is `ticks / hz`.
- **Scrubbing equals playing.** Entity values are a function of `t`; anim and vfx keys start at the world time of the
  key (`origin + key.t`, where `origin` is the world time at position 0). A scrub anchors `origin` afresh and writes
  every anim and vfx track again from it; before a track's first key the entity gets back the anim or effect it had
  before the playback started. So the entity's state after scrubbing to `t` is its state after playing to `t`, from
  any position, backwards too (tests scrub to seven times from a fresh start and from a played state). Replaying a
  timeline (even an edited one) does the same: the entities' own anims and effects from before the first playback are
  kept by entity and kind, and the old playback's triggers are written again from the new start. Resuming a paused
  playback moves `origin` on by the pause, so keys reached later start when they are reached; effects already
  playing keep the start they had (they aged with world time during the pause). Things that
  happen once (events, cues, a subtitle line starting) fire only while playing, each exactly once (tested over a
  tick-by-tick run).

## Playback and the write policy

- `timeline_play` starts a playback **as the caller**: its state at the start is written by that call (its one
  commit). Each tick after, the values that changed are written in one commit under **`Rule::Timeline`**
  ([WRITE_POLICY.md](WRITE_POLICY.md)): no reserved data, claims checked as the agent who started it, journaled under
  that agent and undoable. Only changes are written (a timeline holding still writes nothing).
- **A scrub writes everything.** It writes every entity channel and every anim and vfx track again, in its one
  commit, so an entity another agent moved (or an undo put back) returns to the timeline's state.
- **Never fights claims silently.** A claim (or a deleted target) that refuses a tick's write pauses the playback as
  `blocked`: the refusal (code, message, the claim's owner) is in `timeline_get` (`blocked`) and the trace
  (`timeline.blocked`, warn). It keeps its place; `timeline_pause {paused: false}` tries again.
- **Who changes a playback.** Its starter pauses, scrubs and stops it; anyone else needs `force` and lead authority
  (`authority::access`: `Owned(Store::Timelines)`). Two playbacks never write the same entity (`job_busy` naming the
  owner), and every target must exist when it starts (`target_missing`).
- **Slots never stay taken.** Agents share 4 playbacks, at most 2 each; the hosted game has 2 of its own, so agents
  never crowd out its intro. The game's request for a path an agent is playing takes it over (the agent's ends as
  `replaced`), so an agent playing `intro.json` never blocks the game's intro. A playback ends when its starter's last session closes (`session_closed`, as step jobs
  do), and a paused or blocked one untouched for 120 s ends (`expired`). Resuming a paused or blocked playback anchors
  its keys from the time it resumes.
- **The game's timelines are presentation.** A hosted game plays timelines through `Game::timeline_requests`; those
  may not write entities (`game_timeline_writes`), so a game's cutscene never changes `world_hash`.
- **Loads.** Replacing the world ends every playback (`world_replaced`).

## The window

A playback started with `window` (default when a window is open) drives the human's window camera from its rail
every frame, at the frame's time between ticks (as the hosted game interpolates), with `agent_view` set so the game
neither adopts nor fights it. A human's key, click, scroll, touch or **gamepad button** (nothing assumes a keyboard,
F6) **skips** a skippable playback (`Ended {how: "skipped"}`) and otherwise just takes the view back (a pad button that
skipped is masked from the game until it is let go); `window_view` from an agent takes it back too. When it ends, the
camera the window had before it started comes back (`restore`, default on).

## The hosted game

`Game::timeline_event` hears, on the tick they happen: `Started`, `Event {name, data}`, `Cue` (the audio seam),
`Subtitle` (a line starting) and `Ended {how}`: `finished`, `skipped`, `stopped`, `replaced` (played again, or taken
over by the game: the path plays on), `refused`, `expired` (paused or blocked past the idle lease), `session_closed`
or `world_replaced`. Events are by path, so a game hears an agent's playback of its file too. A blocked playback hasn't ended, so the
game hears nothing then (its status and the trace say so). Both new trait methods have defaults, so other games are
unchanged.

## Tools

| Tool | What it does |
|---|---|
| `timeline_get` | the project's timelines (path, version, bytes, and the summary of those already parsed) and the playbacks; one timeline's tracks; with `at`, its state at a time without playing |
| `timeline_set` | writes a whole timeline, validated, through `project_write` |
| `timeline_validate` | the format, anim and vfx values as `world_set` takes them, and missing targets and look-at entities now |
| `timeline_play` | plays on the tick (`from`, `paused`, `window`, `skippable`, `restore`) |
| `timeline_pause` | pauses or resumes (and retries a blocked one) |
| `timeline_scrub` | moves to `t` and writes the state there (one commit) |
| `timeline_stop` | stops; entities keep the last values written |
| `timeline_preview` | a filmstrip from the rail camera with sight packets: per frame the pose, the subject's screen box, `visible_px`, `on_screen` and whether the frame record drew it, and the largest subjects |
| `timeline_check` | rail checks (below) |

**Off the engine thread.** `timeline_validate {definition}` parses and checks a definition on the caller's thread,
by the file rule (its saved text at most 256 KiB), and a file's problems come with its parse when it was validated or
read; the engine thread only looks paths up. `timeline_check` takes its samples a slice per step, after a step for
the subject's bounds and the look-at centres (resolved once per job) and one for the physics sync. Both are in
`scripts/budget_check.py` at their largest arguments.

Family `timeline` in the tool catalog. Every argument is bounded in the schema (times within 600 s, 2..16 preview
frames, 160..480 px tiles, sample rates 1..120 with at most 4,096 samples) and every answer is bounded before it is
built (failures listed at most 8 per check, 64 problems). Previews and checks are **step jobs**, run between
frames. A preview tile is submitted in one step and landed in a later one, once the GPU has finished: a step flight
that the capture flight lands before any other offscreen render, so the GPU's work never holds the engine thread. A
check runs 128 samples per step. The sheet is encoded and written on a worker.

### Rail checks

Measured on the camera path sampled at `rate` (default 30 a second, at most 4,096 samples):

| Check | Measures | Default limit |
|---|---|---|
| `subject_on_screen` | the subject's box centre in frame | every sample |
| `subject_size` | the subject's box height as a fraction of the view | 0.05..1 |
| `clearance` | physics raycasts both ways along every step of the camera: it never passes through a collider (the entity is named) | none |
| `line_of_sight` (warning) | colliders between the camera and the subject that aren't the subject | none |
| `speed` | m/s | 40 |
| `jump` | m/s²: a step or a kink between keys shows here | 80 |
| `turn_rate` | deg/s the view turns | 90 |
| `turn_jump` | deg/s² | 720 |
| `upright` | the view's angle above or below the horizon (90 flips a world-up camera) | 80 |
| `roll` | deg | 0.5 |
| `horizon_spin` | deg/s the picture's horizon turns (it whips round near straight up or down, where `turn_rate` barely moves) | the `turn_rate` limit |

Only geometry with colliders is seen by `clearance` (`colliders_considered` says how many). The subject is the given
path, else the first camera's look-at entity.

## Sepulcra

- **The intro flyover** (`timelines/intro.json`, 12.5 s): low over the mist north-west of the slab, round it and up
  to the player's default view, where it ends (so the hand-off is seamless). Captions ("Sepulcra"; the Warden's line),
  a cue and events. Played at the start of a windowed run; **wave 1 waits for it** (the simulation isn't stepped, as
  while paused, so no build time is lost). Any input skips it, and that input doesn't also act in the game; an agent's
  acting `play_*` call ends it; headless runs, `--intro off` and reduced motion have none.
- **The victory shot** (`timelines/victory.json`, 7 s): a slow rise round the east of the slab with the caption
  "Victory: the graveyard holds."; it doesn't hold anything (the run is over).
- **The camera rule.** Both rails pass `timeline_check` with the slab as the subject: upright (pitch at most 80
  degrees), no roll, the slab on screen and readable throughout, no jumps, and no pass through the slab's collider.
- **Replays.** The cutscenes write nothing to the world; holding wave 1 is a pause of the simulation, and a replay is
  by simulation tick, so `play_replay` and `world_hash {verify}` still verify.

## Later

- The engine's audio service binds cues to its sound library (the audio lane, note `m5/audio/api`); until then
  Sepulcra plays the cues it knows.
- Subtitles drawn by the engine's own text (the text lane) and the accessibility settings (size, background).
- Previews that apply entity tracks (a scratch world per frame), and curved `look` keys.
- Editor timeline UI (scrubbing with a mouse) on the same tools.

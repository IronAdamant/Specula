# Accessibility and the game shell

M5 lane m5-a11y, 2026-11-07. ROADMAP M5: "UI scale, colour-vision themes, subtitles, remapping,
hold/toggle, reduced motion; screen reader via Windows UI Automation (later)", and from HANDOFF:
"title screen, saved settings, rebinding, gamepad pause". [UI.md](UI.md) has the toolkit these
build on; this page is what Sepulcra ships and what the engine gives every game.

## The owner's rules

- **Leave tiny UI to the player** (2026-11-07). Defaults are legible; a player may still choose
  smaller. Handhelds (Steam Deck, ROG Ally, Legion Go) get legible defaults.
- **Every UI-scale option shows its effective scale**, and is greyed out with "limited by window
  size" when it changes nothing on the current display. It is checked again whenever the display
  changes.
- **Settings are saved per user, outside the repo.**
- **Gamepad pause.**
- Security: rebinding and the title take the human's real input; no agent may read input in
  progress.

## What the engine gives every game

| Piece | Where | What |
|---|---|---|
| Saved settings | `specula::user_settings`, `Game::use_settings` / `take_settings` | One file per game and user, read once before `start`, written off the engine thread |
| Quit | `Game::take_quit` | A title screen's Quit stops the engine, like the close button. Never honoured headless |
| Hold or toggle | `specula_input::HoldMode`, `Input::set_hold_mode` | Per action. A chord's modifiers latch (`Mouse.Right+Mouse.Motion`), and a button stays held until pressed again |
| Panels and handhelds | `specula_platform::monitor_of`, `specula_ui::Display::panel`, `Display::handheld`, `handheld_ui_scale` | The monitor's native size and diagonal (EDID raw DPI); known handhelds by panel; a legible default scale |
| Scale options | `Display::scale_options` | Each UI-scale setting's effective scale, and `limited_by_window` or `smallest` when another setting nearer 1 gives the same size |
| Colour vision | `specula_ui::vision` | Simulates protan, deutan and tritan vision (Machado 2009, full severity), measures CIE76 ΔE, and lists a theme's confusable pairs |
| Timeline cues | `specula::timeline_audio` | A timeline's audio cue plays on the engine's `AudioService` as `timeline:<path>`, on its bus and at its point or entity |
| Captions source | `AudioService::played_after` | The play log after a cursor (typed and bounded), so a game can caption what played |
| Displays for agents | `ui_*` `display.diagonal_in` | Gives a virtual display a physical panel (7 with 1280x800 is a Deck), so handheld profiles can be tested headless |

### Saved settings

- **Where.** `--settings DIR` uses that folder, and `--settings off` turns settings off. Otherwise:
  - a windowed engine uses the OS's per-user folder: `Specula\<game>` in Windows' roaming application-data folder,
    `~/Library/Application Support/Specula/<game>`, or `$XDG_CONFIG_HOME/specula/<game>`
    (`~/.config/...`);
  - a headless engine uses none. Agents' runs never read or overwrite a player's settings.
- **Whose changes are saved.**
  - In the per-user folder, only the player's own changes: those made with the window's real
    input.
  - An agent's change (`play_settings`, `play_ui`, or `ui_act`'s synthetic frames) applies for the
    session only. `play_settings` and `play_state` say `unsaved` when they differ.
  - A folder the operator names saves agents' changes too. Scripts and tests that check saving
    use that.
  - **What is saved is what the player sees.** Live and saved differ after an agent's session
    change. A player's toggle or cycle is worked out once from the live value and then set in
    both, so the file never holds the opposite of what is on screen. What the agent set and the
    player didn't touch stays out of the file. (`change_settings` takes changes that set values;
    debug builds refuse one that reads the settings it is given.)
  - **Scripts never run a game on the real folder.** `scripts/specula_mcp.py`'s `Engine` passes
    `--settings off` unless a script names a folder, and
    `crates/specula/tests/script_settings_guard.rs` fails when a script starts an engine without
    the flag. A script's posted keys and clicks count as the player's, so without this they would
    be written into the file of whoever ran the script.
- **Cost before work.**
  - Reading: at most 64 KiB, read once at start-up. A larger or non-UTF-8 file is left alone, and
    the game gets defaults and the problem.
  - Writing: on a worker, one write at a time, the newest text winning. Each write goes to a
    temporary file, synced, that is then renamed over the old one, through the disk book
    (category `settings`). At shutdown, after the workers drain, a change still waiting is
    written before the process ends (the last click before Quit is kept).
  - `engine_info` (detailed) shows `settings`: where they go, writes made, and a write that
    failed.
- **Forgiving format** (`sepulcra.settings.v1`):
  - A bad value falls back to its default and is named.
  - Unknown keys are kept and written back (at most 32), so other versions' settings survive.
    The file is written indented; if that would pass 64 KiB it is written compact, and if that
    is still too big, without the unknown keys (so one oversized key can't block every save).
  - Bindings are at most 64 actions, validated by the input crate when applied. A set that doesn't
    load leaves the game's own bindings.

### Timeline cues on the audio service

- A cue's sound is the recipe `sounds/<name>.json` under the assets root. It is rendered on a
  worker (`JobKind::TimelineSound`) and cached by name.
- The play is presentation only. A missing recipe, audio being off, a render error or a late
  render is skipped and traced (`timeline.cue_sound`), never an error, so runs hash the same with
  and without audio.
- **Bounds.**
  - Names: 1 to 64 of `a-z0-9_-`.
  - Recipes: at most 64 KiB, unread when larger.
  - Sounds: at most 30 s.
  - Cache: 64 sounds or 8 Mi samples.
  - In flight: 4 renders, 8 waiting cues each, and 16 cues per delivery.
- Sepulcra no longer plays its intro's cue itself. The game still hears every cue in
  `timeline_event`.
- Gameplay modules' `module_sounds` (lane m5-gmods) uses the same find, render and cache pattern.
  The two could share one bank once both are merged.

## Sepulcra's shell

### Title screen

- A windowed run starts on the title, which holds the run the way the intro does: the simulation
  isn't stepped.
- Buttons: New run (then the intro flyover, then wave 1's countdown), Continue (loads the newest
  save, enabled when there is one), Settings, Controls, Quit.
- Quit works only with the window's own input; an agent's press is refused and counted.
- Headless runs and `-- --title off` have no title.
- An agent's acting `play_*` call takes the title away, so agents are never held. A `play_ui`
  press of the title's own buttons drives it instead.

### Settings and accessibility

- **Settings:**
  - Interface size: it cycles, passing over sizes that change nothing on this display, and shows
    the effective scale.
  - Accessibility (opens its own screen), Volume, Button prompts, Reduced motion, Autosave.
- **Accessibility** (`ui/access.json`):
  - **Interface size:** Auto plus all six sizes, each labelled with what it gives on this
    display. Sizes limited here are disabled, with the reason as a tooltip. A line gives the size
    now and a handheld's suggestion.
  - **Text size:** 90-150%, on top of the interface size. It multiplies the theme's role sizes.
  - **Colours:** standard, red-green (deuteranopia), red-green (protanopia), blue-yellow
    (tritanopia), high contrast. These are colour patches in `vision.json` (beside `ui/`), applied to the
    theme's named colours. Each theme keeps the three meaning-carrying colours (rune: the player's
    magic and focus; grave: the dead; fire: warnings) at least ΔE 30 apart for the deficiency it is
    for. The standard palette's grave green and fire yellow are ΔE 16 apart for deutans, which is
    the confusion this fixes. `play_settings` reports it as `colours.pairs_apart`, and a test
    proves it.
  - **Subtitles** (on by default), **sound captions** (off), **subtitle size** (small to huge),
    **subtitle background** (on).
  - **Reduced motion:** no intro flyover, and the threat arrows stop pulsing (the theme's
    `pulse_hz` is 0).
  - **Camera drag:** hold or toggle. Toggle makes one press of the right (orbit) or middle (pan)
    mouse button start the drag and another stop it.
- **Handheld profiles.**
  - The window's monitor reports its native size and raw DPI. A known panel (the Deck at 7.0 or
    7.4", the Ally at 7", the Legion Go at 8.8", the Legion Go S at 8") or any 5.5-10.5" touch panel
    is a handheld.
  - "Auto" there is the smallest size whose text is physically at least 1.15 times the Deck's at
    100%, never under 100%: the Deck gets 115% and the Legion Go 100%.
  - The player may still pick smaller.

### Rebinding

- The controls screen lists each action with its keyboard-and-mouse and gamepad inputs. Pressing a
  row's button listens for the next input from that device: Esc or Start cancels.
- An input another action already uses is bound anyway, and both rows say "Also: ...". The
  player sees the conflict instead of being refused. Reset to defaults restores the game's own
  bindings.
- **Only the player can make the game listen.** An agent's press of a rebind button is refused:
  listening would hand the agent the player's next key. Agents bind by name with
  `play_settings {bind: [{action, device, input}]}`, which is checked on a copy first, so nothing
  changes when any entry is refused.
- **Only the player's devices answer.** While the game listens, an agent's `ui_act` frames are
  dropped whole by the input layer (`specula_input::Input::update_synthetic`): an agent's key
  neither becomes the binding nor cancels the capture. Dropped presses are counted
  (`play_settings`: `refused_agent_presses`).
- **The player's file holds the player's bindings.** A rebind is made in the player's own map (the
  saved one) and in the live one; an agent's session binds stay live only.
- **The ways into the menu are the player's to rebind.** `play_settings {bind}` refuses `cancel`
  and `pause` (`players_action`), so no call can take Escape or Start from the player. They
  rebind them on this screen, where Reset stays a click away.
- **Whose input a frame is.** A window frame is the player's; a `ui_act` frame is synthetic
  (`Frame::synthetic`). Synthetic frames are applied whatever the window's focus (focus is about
  the human's devices: an agent's gamepad press used to do nothing while another window was in
  front) and are never the player's: what they change isn't saved, and Quit and rebinding are
  refused. The engine tells the input monitor about those refusals (`players_only`: noted, never
  counted) and `ui_act` answers `refused_players_only`.

### Gamepad pause

- Start opens the pause menu and Start closes it again, from any screen the pause menu leads to.
- A pad that was in use and disconnects pauses the run with the menu open ("Controller
  disconnected: paused").
- Agents' synthetic frames, which carry no pads, are never counted as a pad going away.

### Subtitles and captions

- **Subtitles.** A timeline's subtitle lines are drawn by the `subtitles` screen: bottom centre,
  above the HUD and below menus, with the speaker, the band, and sizes from the settings. They also
  still go to the message log.
- **Captions** come from the engine's audio log. They are read each frame with a cursor and mapped
  through `assets/captions.json` (sound name to words).
  - Positioned sounds say where they came from, relative to the camera: left, right or behind.
  - The same sound again within 2 s counts up (×3) instead of adding a line.
  - At most 3 captions show, each for 3.5 s.
  - Captions need the engine's audio on (`--audio null` headless).

## Screen reader (design note, later)

Windows UI Automation (UIA), zero-dependency, as decided (UI.md):

1. **Provider.** `specula-platform` answers `WM_GETOBJECT` with a hand-written UIA provider: COM
   vtables in Rust behind `unsafe` FFI boundaries, with `uiautomationcore` loaded at run time
   through `dynlib`, never linked. The root fragment is the window. Its children are the visible
   screens, then their widgets.
2. **Tree source.** `Ui::tree()`, already structured data for agents, gives each widget's id, type,
   text, rect, enabled state and focus. It maps to UIA control types: button to Button, label to
   Text, list to List, radial item to MenuItem, toast to a live region. Screen notes become
   HelpText.
3. **Focus and events.** The UI's focus raises `UIA_AutomationFocusChangedEventId`. Toasts,
   subtitles and captions raise `UIA_LiveRegionChangedEventId` (polite; threats are assertive).
4. **Invoke.** `IInvokeProvider::Invoke` presses the widget through the same activation as a pad's
   accept. That path is the player's: it is real OS accessibility input, so it counts as the
   window's own input.
5. **Security.** Like the window keyboard, nothing an agent can call reads what the screen reader
   is told while the human uses it. The provider only exposes what is drawn.
6. **Cost.** The tree is built only when a client asks, from the last layout. It is bounded by the
   same widget caps as `ui_tree`.

Off Windows: NSAccessibility on macOS and AT-SPI on Linux, each later, with the same mapping.

## Follow-ups

- Sepulcra's world colours (the dead's green glow, the runes' cyan) under the colour themes. Only
  the interface changes today.
- Localisation: every word on these screens is a string-table key (English and German). What
  stays English: the detail an engine error carries when a rebind or a theme can't be applied
  (`rebind.cant`, `rebind.cant_bind`, `toast.settings_problem` show it after their own words).
- A handheld run below its panel's native resolution may not match its model, and a panel that
  reports no physical size is never a handheld: both keep the desktop default, and the player
  chooses a size. Unverified on hardware (no such device here).
- `ui_check` could gain a colour-vision rule built on `specula_ui::vision`. It is a crate
  function today, used by Sepulcra's tests and `play_settings`.

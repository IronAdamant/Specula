# Sepulcra: in-game test plan

For the owner, in the game's window. Tick each box as you go (Obsidian checklist). Anything that
fails or feels wrong: a note under the item is perfect.

Start it with:

```text
cargo run -p sepulcra-app --release -- --port 7486 -- --seed 7
```

The game's own arguments go after the second `--`: `--seed N` picks the run (the HUD's first message
says which), `--mute` turns sound off. Agents use the same game over MCP on the port.

## 1. First look

- [ ] The window opens on the slab, at night: a moonlit grey-stone slab floating over dark mist,
      the pale graveyard in the middle with tombstones inside a low iron fence.
- [ ] The HUD is there: wave, dust and tombstones in the top bar; the speed and pause buttons top
      right; the selected stone's card bottom left; the build bar (four stones and the wall, each with
      its cost and key) bottom centre; "Call wave" bottom right.
- [ ] You hear the night (a quiet ambience). `--mute` silences everything.
- [ ] The first message says "Seed 7: carve stones before wave 1".

## 2. Camera (mouse, then keyboard)

- [ ] Right-drag orbits; the wheel zooms; middle-drag (or WASD) pans.
- [ ] Q / E turn to the next face of the slab, smoothly.
- [ ] Arrow keys orbit too.

## 3. Building with the mouse

- [ ] Click a stone in the build bar (or press 1–5): it lights up cyan and the card bottom left shows
      what it does.
- [ ] Hover a raised fragment: a cyan cell outline and a dotted range ring. Hover a lane with a stone
      selected: a red outline (stones go on fragments).
- [ ] Click a fragment: the stone is carved there (a rune flash, a stone-on-stone sound), and the dust
      drops by its cost.
- [ ] Clicking a build-bar button never also builds on the slab behind it.
- [ ] A stone you can't afford shows its cost in the warm "fire" colour; trying to carve it says
      "need N dust" and buzzes.
- [ ] Select the wall (5) and click a top-face lane: a low stone wall across the lane.

## 4. A stone's panel (upgrade and sell)

- [ ] Click a stone you built: its panel opens on the right (name, tier, what it does now and what
      the next tier adds), with its range ring and a pale cell outline.
- [ ] "Carve +N" upgrades it: the model gains runes, then a capstone, then a floating rune ring at
      tier III. A chime plays. At tier III the button is gone ("Fully carved").
- [ ] "Sell +N" sells it for 70 %; the panel closes.
- [ ] Click a wall: its panel shows its HP and "Remove +N".
- [ ] The X in the panel's corner, or Escape, closes it. Escape with no panel open pauses.
- [ ] Keys still work while a panel is open: U upgrades and X sells the stone under the cursor.

## 5. Waves and the dead

- [ ] "Call wave" (or Space) starts the next wave early and pays the bonus it shows.
- [ ] Shamblers walk with a lurching, dragging gait; skeleton runners sprint; wraiths float and
      sway in the air around the rim; nobody slides around in a frozen pose.
- [ ] Runners leap at the cracked white ledges (a jump in a leap pose).
- [ ] Shamblers climb the cliffs on the north, east and west faces, facing the rock.
- [ ] A shambler blocked by walls stops and claws at a wall (attack swing) until it breaks.
- [ ] When something dies it plays a death (collapse, scatter, fade) where it fell, then goes.
- [ ] Bolts fly from the warding stones (cyan), the obelisks shoot pale needles at wraiths, braziers
      lob fire that bursts low and wide where it lands; rune pillars pulse a cyan ring and slow
      what's inside it (the slowed dead also animate slower).
- [ ] Sounds follow the action: shots, hits, deaths, the wave horn, a brazier's crackle when you're
      near one. Sounds near the camera are louder than far ones.
- [ ] From wave 5, moss and roots creep over the lanes around tier II+ stones.
- [ ] When the dead reach the graveyard, a tombstone topples and turns sickly green; the top bar's
      tombstone count drops.
- [ ] When a face you're not looking at is under attack, a green arrow at the screen edge points at
      it with its name ("North cliff (3)", "Wraiths (4)").

## 6. Pause, speed, menus

- [ ] P (or the pause button top right) is an **active pause**: "PAUSED" shows in the top bar, the
      dead freeze mid-stride, and you can still select, carve, upgrade and wall. P again (or the
      play button) goes on.
- [ ] Escape (with no panel open) or the pad's Start opens the pause menu, and pauses under it.
- [ ] Resume (or Escape / B) closes the menu and the game goes on.
- [ ] Settings: interface size (the HUD scales), volume (sound gets quieter/louder), button prompts
      (Xbox / PlayStation / Steam Deck glyphs), reduced motion.
- [ ] Controls lists the actions with their keys; F1 opens it too.
- [ ] "Restart run" asks first (No is the default), then restarts the same seed.
- [ ] The speed button (or F) cycles x1, x2, x4: everything moves and animates faster.

## 7. Gamepad (Xbox pad, or a Steam Deck)

- [ ] The left stick moves a cursor that snaps to cells; A carves the selected stone there.
- [ ] Y opens the radial build menu at the cursor; point with the stick, A carves that stone at the
      cell; B cancels.
- [ ] X upgrades and View sells the stone under the cursor; R3 removes a wall; L3 calls the wave.
- [ ] The right stick orbits, the triggers zoom, the bumpers turn to the next face.
- [ ] Start pauses; the menus are navigable with the d-pad and A / B, with pad glyphs on the prompts.
- [ ] Prompts switch between keyboard and pad glyphs as you switch devices.

## 8. Touch (a touch screen, if you have one)

- [ ] Tap carves; drag orbits; pinch zooms; two-finger drag pans; long-press opens the radial menu.
- [ ] The build bar and panel buttons are big enough to hit with a finger.

## 9. The end

- [ ] Winning wave 10 shows "The graves hold" with tombstones left, undead laid to rest and the time;
      "New run" starts the next seed, "Look at the slab" closes the card.
- [ ] Losing shows "The dead rise" with the wave you fell on; "Try again" starts a new run.

## 10. Window behaviour

- [ ] Resizing the window keeps the HUD anchored to its edges and readable; a small window switches
      to the narrow layout (a "Stones" button opens the radial menu).
- [ ] Minimizing and restoring is fine; the game keeps running behind it.
- [ ] Alt-tabbing away and back doesn't drop input.

## 11. Agents (optional, with Claude)

- [ ] `play_state` reports the seed, the HUD (screens, selection) and the audio (device, sounds
      played).
- [ ] `play_ui` lists the HUD's widgets and presses them by id (`build_pillar`, `call_wave`, ...);
      `play_ui {select: "top:12,9"}` opens a stone's panel for it.
- [ ] `sight_frame {include_ui: true}` captures show the HUD as you see it.

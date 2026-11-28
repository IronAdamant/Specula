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
- [ ] The graveyard props stand where you approved them in M4 (the north cemetery, the gravedigger's
      corner, the east memorial garden, the south graves).
- [ ] With a stone selected, hover the grave angel or the mausoleum: a red outline, and the hover line
      says what stands there "(no building here)". Click: "Can't place ...: a grave angel stands here".
- [ ] Carve a stone on the pile of bones (south-west): the bones disappear under it, never drawn
      through it. Sell the stone: the bones come back.

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
- [ ] "Save / Load" in the pause menu opens Saved games: three slots and two autosaves, each with what
  it holds (wave, tombstones), time played and when; empty ones say "Empty" and their Load is greyed.
- [ ] Save on an empty slot saves at once ("Saving..." under the title, then a "Game saved" toast).
  Save on a used slot asks first (No is the default); Yes replaces it.
- [ ] Load asks first (this run is lost), then brings the saved run back, paused under the menu, with
  "Game loaded: wave X of Y". The X deletes a slot, after asking.
- [ ] With Autosave on in Settings (it is by default in the window), each wave's start writes the next
  autosave in turn ("Autosaved" toast); both show up in Saved games and load like the slots.
- [ ] All of this works with the pad (d-pad, A, B), the mouse, touch and the keys; Back (Esc / B)
  returns to the pause menu.

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

## 12. The proving ground: the elemental stones (not a shipped map)

The five elemental stones (Rogus fire, Fulmen lightning, Gelu frost, Ventus wind, Saxum earth) and
the exploding dead (Tumidus) live only in the proving ground for now (DESIGN.md, "Elemental
stones"), so the shipped map's balance does not move; the owner has decided they go on the shipped
maps, which is the next round's work and this section's next revision. **Their looks are placeholders today** (glow spheres, the
old dust puff, scorch and cracks), made to show that each rule fired: the items marked *(look)*
below are for when the effects library (Effectus) and the elements (Elementa) have landed, and are
expected to fail until then. The build bar still shows the four shipped stones: place the
elemental ones with `play_command` (an agent) until the HUD has nine.

**Headless first (an agent, no window):**

```text
cargo run -p sepulcra --example headless --release -- --ground proving --layout fire --seed 1
cargo run -p sepulcra --example headless --release -- --ground proving --layout all --sweep 1000
```

**In the window:** start the game as above, then `play_new {"data": "games/sepulcra/data/proving",
"seed": 1, "layout": "<a file in games/sepulcra/data/proving/layouts/>"}`.

### 12.1 Headless checks (an agent can run all of these)

- [ ] `cargo test -p sepulcra` passes: the elements' tests (each stone does what its row says),
      the shipped map's replay and balance tests unchanged.
- [ ] The shipped map's sweep is unchanged: `--layout all --sweep 1000` prints, line for line, what
      it printed before the elements (the wall-time lines aside: DESIGN.md, "Balance so far"; the
      extra `all runs:` line is printed for elemental content only), and the final hashes of seeds
      1, 7, 42 and 999 for each built-in layout are the ones from before.
- [ ] `--ground proving --layout all --sweep 1000` strands nobody (exit code 0) and prints a row
      per layout; the table in DESIGN.md ("What the sweeps say") matches it.
- [ ] The same seed gives the same run: two `--journal` recordings of one proving layout and seed
      are byte for byte the same, and `--replay` verifies each.
- [ ] A save mid-fire (`play_record`, then `play_replay`, or `State::to_bytes` in a test) continues
      the same: the tests `an_exploder_bursts_on_a_wall...` and the proving save test hold it.
- [ ] Furor (DESIGN.md, "Furor"): no stock run reaches a wave's hour
      (`no_stock_run_reaches_a_waves_hour`), and every drawn content ends inside its stated length
      (`every_run_ends_inside_its_stated_length`; the release run:
      `cargo test -p sepulcra --release --lib -- --ignored every_run_ends_inside_its_stated_length_at_length`).
- [ ] Content the format takes that holds a lone one (Gelu with chill 1 and a 30 s freeze every
      tick, no damage), played in the window: the one held stands frozen until twenty minutes after
      its wave's last entry, then thaws and walks on, and nothing freezes it again.

### 12.2 Fire (Rogus) and fire's spread

- [ ] A Rogus stone lobs fire; where it lands the dead catch (*look:* each burning one carries
      flames and a flicker of light, and sounds it).
- [ ] Wave 1 (a packed column up the middle lane): fire jumps from one shambler to the one beside
      it; the column burns from the front back.
- [ ] Fire reaches the grass at the rock's edges and the dry grass laid on the middle lane and the
      west connector: *(look)* a front that advances cell by cell, char behind it, embers, smoke and
      light; nothing burns on bare stone, the lanes outside the dry stretches or the graveyard.
- [ ] A shambler walking past burning grass catches fire.
- [ ] A fire break holds: a gap of bare stone wider than 2.9 m stops grass spreading to grass.
      It does not stop fire carried by the dead: a burning one that walks across lights the grass
      on the far side (DESIGN.md, "What burns on the slab").
- [ ] Grass is drawn on every cell that burns, the dry stretches on the lanes included (a
      placeholder patch there): no fire on what looks like bare stone. **What is drawn now:** the
      proving ground's fuel is exactly the app's 72 patches (the rock's-edge grass less the
      graveyard's, and the two dry stretches), so the graveyard has **no grass** on the proving
      ground (it had 7 patches before the fuel filled the budget); *(look)* the graveyard's slabs
      bare, every grass patch elsewhere on fuel. The shipped map's grass is as it was.
- [ ] Content with more fuel than 72 cells loads (the rules hold no drawing budget): the app
      draws 72 of them and its trace says `sepulcra.grass_dropped` (a warning) with how many burning
      cells it left out.
- [ ] Fire ends: a cell burns about 8 s and is spent (char), a burning one of the dead burns out
      after its time.
- [ ] Headless: `elements:` in a single run's report counts ignitions, bursts and fuel cells caught
      (its bursts are the sweep's "bursts mean": a burst that went off, counted once); a
      `layout waits:` line says when the layout waited for dust or for one of the dead on a cell.

### 12.3 Lightning (Fulmen)

- [ ] A strike hits the first of the dead in range and jumps to the nearest others (two jumps at
      tier I, three at II, four at III), each weaker (*look:* a jagged bolt from the menhir, arcs
      between them).
- [ ] A thawed (wet) one takes half again.
- [ ] One staggered by Saxum ends the chain on itself (earth grounds lightning).
- [ ] A struck Tumidus bursts at once.
- [ ] **Photosensitivity:** one menhir never strikes more than three times a second (a cooldown
      of at least 10 ticks; the content refuses a shorter one). Several menhirs together are not
      held to it by the rules (how often lightning strikes is gameplay): what a person sees is held
      by the engine's flash governor (the effects library's stage 1c). **A dependency: until that
      governor exists, do not show the stones with lightning effects to a person**, and do not
      tick this item. Then: with three menhirs and Rogus firing together, the engine's flash watch
      (`vfx_info`, the frame's problems) reports no more than three flashes a second (*look:* the
      governor dims the extra ones).

### 12.4 Frost (Gelu)

- [ ] Frost bolts slow and chill; at full chill the target freezes and stands (*look:* rime over it,
      a crystalline crackle).
- [ ] The next hit shatters a frozen one for double damage (more at higher tiers).
- [ ] A frozen one thaws after its time and is wet for 4 s (it does not catch fire while wet).
- [ ] Fire on a frozen one thaws it at once (steam) instead of burning it, a Rogus lob included
      (its damage shatters the ice; it does not set the enemy alight, nor light a Tumidus's fuse).
- [ ] Frost on a burning one puts it out; rime on a cell stops the grass fire there.
- [ ] Frost with no damage loads (it is legal now) and may hold a lone one long; with the stock
      stones a lone one under frost is killed or walks on within minutes (DESIGN.md, "Gelu").
- [ ] The colossus barely chills (mass 8); Gelu is weak against it.

### 12.5 Wind (Ventus): no damage, through the push, then recharge

- [ ] With any of the dead in range, one alone included, a gust shoves them all back along their
      lane (runners farther, the colossus hardly at all); nobody loses a hit point to it.
- [ ] Each shove one of the dead takes is a quarter weaker than the one before (`shove_fade`
      0.25), from whichever cairn, and it is not shoved again until a shove has ended; a lone one
      held by several stock cairns reaches the graveyard within minutes.
- [ ] A cairn with nobody in range it can move (all still being shoved, in furor, or with their
      shoves faded to nothing) does not gust: it keeps its charge, and fans no fire.
- [ ] A leaper in mid-air is not stopped in the air: the gust's shove takes it when it lands.
- [ ] The cairn then recharges: no second gust before its cooldown (6 s at tier I), whatever is in
      range; *(look)* a charge the player sees filling.
- [ ] A shove never pushes one through a wall or off a lane, and stops at a spawn.
- [ ] A gust fans the fires of the burning dead it pushes, and leans the grass fire downwind: it
      runs faster with the wind than against it.
- [ ] A burning one pushed back into the column sets it alight.

### 12.6 Earth (Saxum)

- [ ] Stone spikes erupt along the target's lane (two cells at tier I, three at II, four at III):
      damage and a short stagger to all on them (*look:* spikes rising from the slab and sinking,
      a crack, a rumble, dust).
- [ ] The heave puts out the fire on its cells (sand on flames).
- [ ] From tier II, rubble rises on the farthest cell: a wall worth nothing that blocks and
      re-routes, that the dead break and Tumidus blasts, and that crumbles by itself (6 s, 8 s).
- [ ] A heave on standing rubble does not mend it: half-broken rubble stays half broken and
      crumbles when it was to.
- [ ] Where rubble has gone, no rubble rises again while any of the dead that were on the slab
      then still stand; once they have all gone it may.
- [ ] *Rubble's health bar:* place a Saxum of tier II or III by a lane and let a wave reach it. A
      bar shows over each rubble from the tick it rises, full: a dark back, a green fill,
      drawn in the interface over the rubble's place, never smaller than about 84 by 15 pixels at
      1080 high (56 by 10 at 720), above the rubble so that the dead at it stay in sight, and
      drawn over them; a thin stem runs from each bar down to a small mark on its own rubble.
- [ ] From the game's usual camera, with the whole slab in view, the bar's level can be read
      without enlarging: green from full to a half, orange under a half, bright red under a
      quarter, by day as by night.
- [ ] Rubble on neighbouring cells: no bar covers another's; a bar lifted clear of a neighbour's
      stays over its own rubble, its stem reaching down to that rubble's mark.
- [ ] The bar empties from the right as the dead claw the rubble (`play_state`'s walls give its
      hp and max_hp, and for rubble `life_ticks`, `life_left_ticks` and `bar_permille`: the fill
      is the lesser share).
- [ ] With nobody at it, the bar still empties as the rubble's life runs out (6 s at II, 8 s at
      III), and is gone the tick the rubble crumbles or breaks.
- [ ] The bar shows the lesser of health and life: rubble half broken with most of its life left
      shows half; rubble untouched near its end shows almost nothing.

### 12.7 The exploding dead (Tumidus)

- [ ] It walks to a wall on the lanes even while a way round is open, and bursts on it: every wall
      and rubble within 3 m goes (no refund), the dead around it are hurt and shoved.
- [ ] After a burst every one of the dead that was going round takes the opened lane.
- [ ] Killed, it bursts where it falls; set alight, it bursts when its 1.5 s fuse burns down
      (unless frost puts it out first); frozen when a strike, hit or blast reaches it, it cannot
      burst: if that kills it, it dies whole; if it lives, the ice is gone and the next strike or
      fire sets it off.
- [ ] A burst never goes off after the game is lost (nothing happens after `Lost`).
- [ ] Wind pushes it back from the wall it is walking to.

### 12.8 The low-end device (the integrated GPU) *(look)*

- [ ] Start with `--adapter integrated`. In wave 1 with fire on the grass and the column burning,
      `perf_frame` stays within the engine's budgets (`gpu_passes.particles`, the frame time); the
      particle and light budgets thin the far fires first (VFX.md).
- [ ] Wave 6 (everything at once): no frame over budget; `vfx_info` reports what was thinned.

### 12.9 Recording

- [ ] Record the engine's window only (`python scripts/window_recorder.py`), never the screen: one
      clip per stone (12.2 to 12.7), failures kept, and the frames put in the gallery.

### 12.10 Owed before stones are placed in a shipped map

- [ ] **Furor ported to Lex.** The Rust rules have furor only where the content has the elements
      (DESIGN.md, "Furor"); the Lex rules (`--rules lex`) have none. Before a shipped map has
      elemental stones, Lex gets furor (the wave's hour, nothing holds the one in furor, no
      rubble, no summons), with a parity test of a run walled in past a wave's hour, so the app's
      every-tick comparison never stops the game on it.

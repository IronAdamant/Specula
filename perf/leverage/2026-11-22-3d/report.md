# Measured leverage: the runs

Folder: `lv3e`. Every run is here, failed ones included.

## Settings these runs used

- 6 of these builders' briefs carry the lead's choice for the job (which of the engine's aids fit it, under "From the lead"): they are directed builders, not cold ones (change3d-engine-1, change3d-engine-2, medium3d-engine-1, medium3d-engine-2, small3d-engine-1, small3d-engine-2)
- engine build of the change3d runs: bafa9b7d43
- change3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium3d runs: bafa9b7d43
- medium3d: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small3d runs: bafa9b7d43
- small3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change3d-engine-1 | 231,227 | 0.47 | 55,192 | 795,179 | 16,546 | 21 | 22 | 4 | done | 30 | 4,256 / 20,126 | 11 / 11 | 1 / 1 | 6 | 1,541 |
| change3d-engine-2 | 188,062 | 0.37 | 42,412 | 689,479 | 13,222 | 21 | 23 | 6 | done | 18 | 2,825 / 9,812 | 11 / 11 | 1 / 1 | 5 | 1,204 |
| change3d-plain-1 | 241,494 | 0.49 | 52,253 | 745,225 | 20,333 | 20 | 25 | 3 | done | - | - | 11 / 11 | 1 / 1 | 3 | 1,513 |
| change3d-plain-2 | 281,921 | 0.57 | 63,489 | 919,849 | 22,117 | 21 | 26 | 3 | done | - | - | 11 / 11 | 1 / 1 | 3 | 2,016 |
| medium3d-engine-1 | 640,038 | 1.11 | 94,081 | 3,158,982 | 41,313 | 52 | 53 | 15 | done | 37 | 4,581 / 26,084 | 23 / 24 | 3 / 3 | 5 | 1,381 (834 at the start) |
| medium3d-engine-2 | 334,931 | 0.65 | 63,248 | 1,149,594 | 28,185 | 27 | 27 | 5 | done | 19 | 2,064 / 15,822 | 23 / 24 | 3 / 3 | 6 | 1,086 (834 at the start) |
| medium3d-plain-1 | 413,532 | 0.86 | 63,593 | 592,926 | 54,951 | 13 | 16 | 7 | done | - | - | 23 / 24 | 3 / 3 | 5 | 1,401 |
| medium3d-plain-2 | 694,428 | 1.35 | 94,534 | 1,786,435 | 79,526 | 27 | 34 | 10 | done | - | - | 23 / 24 | 3 / 3 | 4 | 1,820 |
| small3d-engine-1 | 137,046 | 0.29 | 35,783 | 347,087 | 11,523 | 13 | 18 | 6 | done | 9 | 951 / 4,752 | 15 / 15 | 3 / 3 | 3 | 932 (834 at the start) |
| small3d-engine-2 | 132,688 | 0.28 | 33,203 | 336,355 | 11,511 | 13 | 19 | 2 | done | 7 | 583 / 2,950 | 15 / 15 | 3 / 3 | 4 | 937 (834 at the start) |
| small3d-plain-1 | 103,776 | 0.23 | 19,200 | 89,289 | 14,170 | 6 | 6 | 2 | done | - | - | 14 / 15 | 3 / 3 | 3 | 538 |
| small3d-plain-2 | 123,684 | 0.26 | 21,308 | 184,540 | 15,720 | 10 | 10 | 2 | done | - | - | 14 / 15 | 3 / 3 | 3 | 510 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change3d | engine | 2 of 2 | 209,644 | 188,062 to 231,227 | 5 | 11.0 / 11 | 19,059 |
| change3d | plain | 2 of 2 | 261,708 | 241,494 to 281,921 | 3 | 11.0 / 11 | 23,792 |
| medium3d | engine | 2 of 2 | 487,484 | 334,931 to 640,038 | 10 | 23.0 / 24 | 21,195 |
| medium3d | plain | 2 of 2 | 553,980 | 413,532 to 694,428 | 8 | 23.0 / 24 | 24,086 |
| small3d | engine | 2 of 2 | 134,867 | 132,688 to 137,046 | 4 | 15.0 / 15 | 8,991 |
| small3d | plain | 2 of 2 | 113,730 | 103,776 to 123,684 | 2 | 14.0 / 15 | 8,124 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change3d | engine | 209,645 | 261,708 | 0.80 | 1.02 | 11.0 / 11 | 11.0 / 11 |
| medium3d | engine | 487,485 | 553,980 | 0.88 | 1.98 | 23.0 / 24 | 23.0 / 24 |
| small3d | engine | 134,867 | 113,730 | 1.19 | 1.63 | 15.0 / 15 | 14.0 / 15 |

Judging: 12 judging(s) of 12 run(s), 4,775,287 weighted tokens in all.

At the runner's list prices: 6.92 USD for the builds, 16.98 USD for the judging shown above (23.91 USD in all).

## Items that failed

- medium3d-engine-1 **m23**: Volume set to 0: sounds still listed (bounce, coin, fall). But the game code calls cx.sound(name, None, 1.0) and the README never says how volume scales playback. The only statement is the engine's own note work/notes/world.md:61 (master bus), which the builder did not write. Silence could not be heard in a headless run.
- medium3d-engine-2 **m23**: At volume 0 the sounds list still fills, but a headless run plays nothing aloud, so silence cannot be heard here. The README says nothing about volume scaling. The code calls cx.sound(name, None, 1.0) with a fixed gain. Only the engine note notes/world.md says the engine scales the master bus by volume.
- medium3d-plain-1 **m23**: sound.play returns at volume 0 and wav_bytes scales the amplitude by volume/10, and sounds is still listed at volume 0. But play calls winsound.PlaySound(bytes, SND_MEMORY|SND_ASYNC), which on this Python (3.13) raises 'RuntimeError: Cannot play asynchronously from memory' (tested with silent bytes). The error is swallowed, so the game plays no sound at any volume and the volume scaling never reaches the player.
- medium3d-plain-2 **m13**: I could not get any crate off the board: I tried a straight push right, a steering controller through the wall along 4 z lanes x 3 speeds, a push from the +z side, and a crate-chasing controller (3000 ticks). The crates moved at most ~2.3 m and the count stayed at 6, so crate removal below y=-5 was never seen.
- small3d-plain-1 **s14**: No frame shows score or lives: play.png, over.png (game over at 1200 ticks) and won.png (win at 1763 ticks) have no text at all, so there is no 'Game over' / 'You win', score or restart hint. The README admits this ('No score text in the PNG frame').
- small3d-plain-2 **s14**: No frame has any text: play.png, play2.png show no score or lives; over.png (lives 0, over true) and won.png (won true) show only the board, no 'Game over'/'You win', score or restart hint. render() in roll.py draws no HUD; text exists only in the unopened tkinter window.

## What each judge found beyond the list

- change3d-engine-1: README's file list still says strings are 'en.json, es.json: every word, both languages' (fr.json not named) and 'Both are kept between runs' for what are now three settings.
- change3d-engine-1: Language: menu_right/menu_left stop at the ends (en..fr) instead of wrapping; only Enter cycles fr back to en.
- change3d-engine-1: With the Light ball a sustained push cannot move the crate wall on board 2 at all (crate displacement 0.12 m in total), so board 2's middle coin route may be blocked for a light ball.
- change3d-engine-1: The README scripts for board 4 lose two lives on the way (found by search), so neither is a clean play-through.
- change3d-engine-1: After the lost screen the ball is left falling forever; the engine prints 'ball has fallen out of the world' on stderr.
- change3d-engine-1: Title frame shows the red ball peeking out beside the title panel (the ball spawned at board 1's start before play).
- change3d-engine-2: ball_kind in the state lags the Ball setting: on the title and Settings screens it keeps the previous run's value (Settings shows Ball: Heavy while ball_kind is normal) and only changes one tick after a run starts.
- change3d-engine-2: README's line under the board-3 script still says it ends at screen won, score 30; it now ends on board 4 in play.
- change3d-engine-2: README's Files section names only en.json and es.json; fr.json is not listed.
- change3d-engine-2: The ramp is a tilted slab with open space under its high end: a ball rolled back from +x wedges under it and rests at x 1.06 (solid, but it gets stuck there).
- change3d-engine-2: The board-4 clearing script given in the README loses two lives on the way (allowed by the brief, but no lossless win is offered).
- change3d-plain-1: Board 4 frame: the ball's ground shadow is drawn on top of the ramp at floor level (shows through the ramp under the ball).
- change3d-plain-1: A push held when a board ends stays held on the next board (a held down push carries into board 2; the builder noted it as pre-existing).
- change3d-plain-1: Coins on boards 3 and 4 render as dark brown discs, much darker than the gold coins of board 2.
- change3d-plain-2: Board 2: after the ball strikes the wall, the upper crates sink into the lower ones and stay there. In all three ball weights, pairs of crates end with centres 0.01-0.10 apart (cubes are 0.5), so the frame shows 3 boxes for 6 crates. This likely followed the builder's crate-wake change, since the crates now really move.
- change3d-plain-2: README file table still says sim.py holds 'texts in both languages' and font.py has 'accents for Spanish'; the game now has three languages.
- change3d-plain-2: The flying coin (2.2, 1.2, 0) is taken with the ball at (1.79, 1.01), 0.45 away, before the ball reaches the coin's x.
- medium3d-engine-1: Title frame shows the 3D ball as a red blob over the left edge of the menu panel (the ball is drawn while on the title).
- medium3d-engine-1: On the lost screen the ball is left falling out of the world; the engine prints a warning (ball has fallen out of the world) on stderr in every lost run.
- medium3d-engine-1: The README says the engine's find could not find a board-3 winning script; the script given was found by hand.
- medium3d-engine-1: One fixed camera for all boards, so board 1 is off-centre (README admits it).
- medium3d-engine-1: A ball that rolls onto the bridge moving at a speed other than the bridge's keeps that relative speed (the bridge passes on only its changes of speed); a ball at rest relative to the bridge is carried.
- medium3d-engine-2: With --set board=N and no input the run sits on the title showing board 1. The brief says the board option starts the run in play on that board; here it takes effect only after 0:enter.
- medium3d-engine-2: bridge_x is null at the tick play begins (tick 0) on board 3; it becomes a number from tick 1.
- medium3d-engine-2: A crate or ball that drops below y=-5 is removed or reset one tick later: it appears in the state once with y below -5.
- medium3d-engine-2: After a run ends and Enter returns to the title, the state still reports the old score, lives, ball and crates until Start is chosen. Start does reset them to score 0 and lives 3.
- medium3d-engine-2: On the settings screen texts also lists the title's texts beneath it (Roll, Start, Settings, Quit).
- medium3d-engine-2: Bounce plays at most once per tick (README admits it).
- medium3d-plain-1: Windowed game is silent: sound.play uses SND_MEMORY|SND_ASYNC, which winsound rejects with a RuntimeError that the game swallows, so no sound is ever heard.
- medium3d-plain-1: The ball can fall through a slab: landing is only tested when the previous y was >= 0.25, so a ball that dips even one tick over the small gap between the bridge edge and slab 2 then passes through the slab top (seen at x 3.2..3.6, y 0.17 -> -0.17). This makes crossing to board 3's second slab need frame-exact timing (1 of 40 start ticks tried worked).
- medium3d-plain-1: Ball landing on the bridge takes the bridge's velocity for free (velocity is not converted to the bridge frame when it lands).
- medium3d-plain-1: The settings screen hint '< >' draws as '? ?' because the bitmap font has no glyphs for < and >.
- medium3d-plain-1: Coins on board 2 are drawn in a dark brown close to the crate colour, so they are hard to see.
- medium3d-plain-2: Crates sink into each other: after a strike or a push from the +z side the top crates fall into the bottom ones and sit at almost the same centres (e.g. two crates at (0.01,0.25,-0.49)). Two crates fill the same space, against the brief's 'push one another'.
- medium3d-plain-2: The crates are very hard to move: the ball wedges against the pile and gets turned aside, so I could not push a crate off an edge with any input I tried.
- medium3d-plain-2: Without --settings the run reads and writes settings_data inside the game folder, so a settings change in an earlier no-folder run would carry over to later no-folder runs.
- medium3d-plain-2: In the frame the lost screen's title 'FIN DEL JUEGO' fills the width almost edge to edge.
- small3d-engine-1: On the end screen, inputs such as up/down move focus among the 'Play again'/'Title' buttons. After 'down' was held there, a later restart (Enter) did not restart the run (t1100 in my run), though the screen says 'press Enter to restart'.
- small3d-engine-1: `sounds` is not cleared on restart: it keeps the sounds of the earlier run, while the brief says 'every sound played so far in this run'. This is ambiguous, because the headless run is the same run.
- small3d-engine-1: After the third fall the ball stays frozen at y=1.2483, one physics step past the start, instead of at exactly (0,1.25,0). This is minor.
- small3d-engine-2: After a run ends, pressing down (S) moves the end screen's focus from 'Press Enter to restart' to 'Title', and a following restart no longer restarts: inputs 100:right,640:down,700:restart stays over at tick 720 (lives 0, score 10).
- small3d-engine-2: Restart in mid-run does nothing (150:restart in the README's own example leaves score 10, lives 2); the brief only asks for restart after the end, but the README's example implies otherwise.
- small3d-engine-2: The ball rests at y = 0.24993, slightly below 0.25 (within tolerance).
- small3d-engine-2: After the final fall the ball is left frozen below the board at y -5.28 instead of being reset; the engine prints a 'fallen out of the world' warning on stderr.
- small3d-plain-1: The PNG frame has no HUD at all: no score, no lives, and no end-of-run message (only the untested tkinter window draws text).
- small3d-plain-1: The interactive window mode was never run by its builder, and I did not open it (it would take the keyboard).
- small3d-plain-1: Once the ball's centre passes the board's edge, nothing stops it hitting the slab's side: there is no side collision, and the README says so.
- small3d-plain-2: sounds is not cleared on restart, so after a restart the printed sounds list still holds sounds from the previous run (brief: every sound played so far in this run).
- small3d-plain-2: On a loss the ball is left where it fell (y about -5.05, off screen) rather than visible; the game-over frame shows an empty board with no message.
- small3d-plain-2: The interactive window uses a separate simple polygon renderer; the README says it was never opened by the builder.

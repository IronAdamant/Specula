# Measured leverage: the runs

Folder: `lv3d`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change3d runs: 7f2167f8df
- change3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium3d runs: 7f2167f8df
- medium3d: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small3d runs: 7f2167f8df
- small3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change3d-engine-1 | 351,956 | 0.64 | 60,167 | 1,556,097 | 24,231 | 35 | 37 | 11 | done | 127 | 7,329 / 43,256 | 11 / 11 | 1 / 1 | 4 | 1,976 |
| change3d-engine-2 | 290,058 | 0.55 | 55,946 | 1,175,546 | 20,517 | 28 | 28 | 6 | done | 46 | 3,194 / 9,251 | 11 / 11 | 1 / 1 | 4 | 1,714 |
| change3d-plain-1 | 308,022 | 0.60 | 62,250 | 1,122,768 | 23,589 | 25 | 30 | 3 | done | - | - | 10 / 11 | 1 / 1 | 3 | 1,655 |
| change3d-plain-2 | 211,025 | 0.45 | 51,105 | 522,608 | 18,978 | 15 | 18 | 2 | done | - | - | 11 / 11 | 1 / 1 | 2 | 1,393 |
| medium3d-engine-1 | 721,729 | 1.22 | 92,854 | 3,621,047 | 48,717 | 56 | 56 | 15 | done | 118 | 6,294 / 49,481 | 22 / 24 | 3 / 3 | 6 | 1,794 (792 at the start) |
| medium3d-engine-2 | 751,694 | 1.28 | 94,889 | 3,657,615 | 53,470 | 58 | 63 | 16 | done | 123 | 9,286 / 57,950 | 22 / 24 | 3 / 3 | 5 | 1,562 (792 at the start) |
| medium3d-plain-1 | 483,335 | 0.96 | 67,974 | 1,107,686 | 57,522 | 23 | 26 | 8 | done | - | - | 23 / 24 | 3 / 3 | 3 | 1,530 |
| medium3d-plain-2 | 377,694 | 0.77 | 59,381 | 722,065 | 46,254 | 17 | 22 | 5 | done | - | - | 24 / 24 | 3 / 3 | 4 | 1,274 |
| small3d-engine-1 | 213,712 | 0.42 | 45,010 | 699,109 | 17,510 | 22 | 30 | 3 | done | 17 | 833 / 8,157 | 15 / 15 | 3 / 3 | 5 | 1,076 (792 at the start) |
| small3d-engine-2 | 227,286 | 0.45 | 44,395 | 704,078 | 20,279 | 21 | 32 | 5 | done | 30 | 2,087 / 13,642 | 15 / 15 | 3 / 3 | 4 | 939 (792 at the start) |
| small3d-plain-1 | 136,294 | 0.29 | 24,559 | 155,142 | 18,017 | 8 | 8 | 2 | done | - | - | 15 / 15 | 3 / 3 | 3 | 604 |
| small3d-plain-2 | 136,340 | 0.29 | 23,829 | 175,584 | 17,800 | 9 | 11 | 2 | done | - | - | 15 / 15 | 3 / 3 | 4 | 600 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change3d | engine | 2 of 2 | 321,007 | 290,058 to 351,956 | 8 | 11.0 / 11 | 29,182 |
| change3d | plain | 2 of 2 | 259,524 | 211,025 to 308,022 | 3 | 10.5 / 11 | 24,717 |
| medium3d | engine | 2 of 2 | 736,712 | 721,729 to 751,694 | 15 | 22.0 / 24 | 33,487 |
| medium3d | plain | 2 of 2 | 430,514 | 377,694 to 483,335 | 7 | 23.5 / 24 | 18,320 |
| small3d | engine | 2 of 2 | 220,499 | 213,712 to 227,286 | 4 | 15.0 / 15 | 14,700 |
| small3d | plain | 2 of 2 | 136,317 | 136,294 to 136,340 | 2 | 15.0 / 15 | 9,088 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change3d | engine | 321,007 | 259,524 | 1.24 | 1.58 | 11.0 / 11 | 10.5 / 11 |
| medium3d | engine | 736,712 | 430,515 | 1.71 | 2.85 | 22.0 / 24 | 23.5 / 24 |
| small3d | engine | 220,499 | 136,317 | 1.62 | 2.53 | 15.0 / 15 | 15.0 / 15 |

Judging: 12 judging(s) of 12 run(s), 4,360,565 weighted tokens in all.

At the runner's list prices: 7.92 USD for the builds, 15.83 USD for the judging shown above (23.75 USD in all).

## Items that failed

- change3d-plain-1 **c02**: Rolling up works: with "0:right,80:none" on board 4 the ball has y 0.519 at x -0.27 (tick 71) and y 0.975 at x 1.03 (tick 92). But the ramp is not solid from the side: --board 4 --ticks 351 --inputs "0:none,80:down,120:up,160:right,222:left,281:none,300:up,330:none" rolls the ball at y 0.25 through the ramp's side from z 1.2 to z -1.3 at x 0.74-0.8, where the ramp top is about 0.6. The state at tick 351 is ball (0.74, 0.25, -0.01), and the frame shows the ball hidden inside the ramp. A ball rolled -x at the ramp's 0.7-high end also passes straight through (sim._ramp only handles the top plane).
- medium3d-engine-1 **m10**: My controller over --follow took the 4 coins: each gave +10, removed that coin and added coin. At tick 511 (the 4th coin) board=2, clear was added, ball (-3,1.25,0) at rest, board 2's 3 coins listed, but crates was [] (empty). The six crates first appear at tick 512, so the state that changes board does not hold board 2's crates.
- medium3d-engine-1 **m23**: Could not hear or measure the sound headless. The game code calls cx.sound(name, None, 1.0) with a fixed gain. Only README line 72 says it is played with the volume's gain; it does not show where, and the engine does the scaling (notes/how.md).
- medium3d-engine-2 **m16**: The second half holds: '100:right,130:none' rolls off the first slab while the bridge is at 1.9 and falls into the gap (lives 3 -> 2). The first half fails. With '100:right,112:none' the ball waits at the slab edge and goes onto the bridge at tick 360. It is never carried at rest: from 360 to 540 it moves 3.48 m while bridge_x moves 2 (1.48 m off). With no input it then slides off the bridge's +x end at about 760 and falls (y -3.8 at 820). Pushing left on the bridge (385-393) sped it up to the right instead.
- medium3d-engine-2 **m23**: Not observed. The headless runs and the engine I started have audio off ('audio is off (--audio off; the headless default)'), so what is heard at volume 0 cannot be measured. The rules call cx.sound(name, None, 1.0) with no scaling. The README only says sounds are silent at 0. Only the engine note notes/how.md says the built-in volume setting 'is the sound's volume'.
- medium3d-plain-1 **m22**: Title, settings, paused and won frames show the Spanish texts, and the play HUD shows 'Board N of 3', score and lives. But the Spanish lost frame (es_lost.png) draws 'Fin del juego' too wide: its first and last letters are cut off at both edges of the 800-px frame.

## What each judge found beyond the list

- change3d-engine-1: The Language setting does not wrap around: Right at French stays French and Left at English stays English. It steps en<->es<->fr in order but never cycles back.
- change3d-engine-1: The README's 'How it is put together' section is partly stale: it says actions.json holds the 'board2'/'board3' start words and that the strings are en.json and es.json. It does not mention board4 or fr.json there, although later lines do.
- change3d-engine-1: Known and unchanged: the texts of a Settings screen opened from the title still list the title's words; 'sounds' lists sounds at volume 0.
- change3d-engine-1: With a Light ball held against the board-2 crate wall, the ball stalls at x=-0.48 and the crates move only 0.02. This fits the rule (same acceleration, smaller mass), but the wall then cannot be passed with a Light ball by holding right.
- change3d-engine-2: README still says the 'You win' screen comes after board 3 and 'bot on 3 ends in won' (What I checked), out of date with four boards; CHANGES.md says drive.py 3 now ends lost.
- change3d-engine-2: The bridge on board 3 still flings a ball that rides it (ground speed rose to ~4 m/s while the bot pushed left), the m16 defect, unchanged.
- change3d-engine-2: roll.py holds only one push at a time, so diagonal pushes cannot be scripted through it.
- change3d-engine-2: A plain push right from board 4's start misses the air coin (closest pass 0.67 m from it, pick-up radius 0.5); a run-up is needed.
- change3d-plain-1: Board 4 ramp only has a top plane: a ball enters it through the sides (|z|=1) and through the 0.7-high end face at x=1, and can sit hidden inside it.
- change3d-plain-1: A strong push on board 4 (right held 80+ ticks from the start) launches the ball over the ramp and off the far end of the slab, costing a life (it is a long jump, so this may be intended).
- change3d-plain-1: Pre-existing, noted by the builder: ball against slab sides is not modelled.
- change3d-plain-2: The 5x7 font draws the acute accent of E as a single dot above the letter (REGLAGES, TERMINEE, GAGNE in French frames), so it looks more like a dot accent than an acute.
- change3d-plain-2: A bounce sound plays when a rolling ball meets the foot of the ramp (seen around tick 61 with 0:right on board 4; CHANGES.md mentions it).
- medium3d-engine-1: After a board is cleared, the state on the tick of the change reports crates as [] although board 2 has six crates (one tick late).
- medium3d-engine-1: A push of 4 m/s2 moves the rolling ball only at about 2.86 m/s2 (0.359 m in 0.5 s, against 0.5 m expected).
- medium3d-engine-1: Settings texts also list the title's texts (Roll, Start, Settings, Quit) under the settings ones.
- medium3d-engine-1: The ball does not topple the crates: the struck column of two slides as a block.
- medium3d-engine-1: The bridge in the board-3 frame is drawn with a notch on its left side.
- medium3d-engine-1: After Quit to title, the state still reports board, score, lives, ball and crates instead of null.
- medium3d-engine-2: roll.py decodes the engine's UTF-8 output with the Windows default code page, so non-ASCII texts print garbled in the state: 'Idioma: EspaÃ±ol', 'Salir al tÃ\u00adtulo' (the frames are correct).
- medium3d-engine-2: On the bridge, pushes and landings add speed in the wrong way: a left push on the bridge sped the ball up to the right, and landing on the bridge at ~2 m/s threw it on at ~5-8 m/s, off the far end.
- medium3d-engine-2: A ball on the bridge moves at about twice the bridge's speed going +x: it is carried by the rule and also driven by the moving bridge.
- medium3d-engine-2: The won and lost screens report selected 'Quit to title' though their one button reads 'Press Enter to go back to the title'.
- medium3d-engine-2: Two headless runs that share one settings folder at the same time fail with project_open (the engine records itself in that folder).
- medium3d-plain-1: The Spanish Game-over heading 'Fin del juego' is drawn wider than the 800-px frame and clipped at both sides.
- medium3d-plain-1: After a strike on board 2, the top middle crate can come to rest (asleep) at (0.62, 0.75, 0) on a lower crate at x 0.92 whose top spans x 0.67-1.17. Its centre of mass is outside its support, so it should topple but hangs.
- medium3d-plain-1: The window and live sound (gui.py) were never run by the builder (stated in README). The judge did not open a window either.
- medium3d-plain-2: Crates never rotate: the brief asks that the ball topples them; here an upper crate only slides or drops (README admits it).
- medium3d-plain-2: On the bridge, friction of up to 3.5 m/s2 nearly cancels the 4 m/s2 push, so a ball on the bridge can barely be steered relative to it.
- medium3d-plain-2: A held push stays held through a fall reset, so the ball starts moving again at once (the player still holds the key, so this is arguable).
- medium3d-plain-2: Running the game creates work/__pycache__ (Python bytecode). The judge removed it after the runs.
- small3d-engine-1: The ball starts to fall once its centre passes |x| or |z| = 3, even though half of it is still over the board; there is no rolling over the edge.
- small3d-engine-1: The ball does not visibly roll or spin; it is one flat colour (the README says so).
- small3d-engine-1: Coins are flattened spheres, not coin shapes; the ball casts no shadow on the board.
- small3d-engine-1: After a restart, sounds keeps the earlier run's sounds (the README says so; the brief's 'in this run' is ambiguous).
- small3d-engine-1: The window play command (specula play . --as-is) in the README cannot be run through the allowed wrapper, so I did not check it.
- small3d-engine-2: The brief's input word `restart` does not restart after the end in the headless script; only `enter+restart` does (documented in README).
- small3d-engine-2: On restart the `sounds` list is not cleared (it keeps the earlier run's sounds); arguably fine since it is the headless run's list.
- small3d-engine-2: After the third fall the ball is shown reset at its start (0,1.25,0) rather than where it fell; harmless.
- small3d-engine-2: Sound regeneration command (engine start + audio_preview) not exercised by the judge; the shipped WAV files were used.
- small3d-plain-1: tick and sounds are not cleared by a restart (README says so); the brief speaks of sounds 'so far in this run', so this is arguable.
- small3d-plain-1: The ball has no collision with the slab's edge or side: it is held up only while its centre is over the board, so near the edge it stays level until the centre is past x=3.
- small3d-plain-1: The live tkinter window (keys, timing, sound playback) was not checked by the judge or the builder.
- small3d-plain-2: The 'sounds' list is not cleared on restart: after a restart (a fresh run) it still holds every sound from the earlier run, so it is not 'every sound played so far in this run'.
- small3d-plain-2: The ball does not collide with the slab's sides (the README admits this); the board supports the ball only while its centre is inside [-3,3].
- small3d-plain-2: The PNG HUD text is all in capitals ('YOU WIN' / 'GAME OVER'), not 'You win' / 'Game over' as the brief writes it.
- small3d-plain-2: Interactive play (tkinter + winsound) was not exercised by the builder or by me.

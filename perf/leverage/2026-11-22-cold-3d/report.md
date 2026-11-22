# Measured leverage: the runs

Folder: `lv3f`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change3d runs: 74007d71bc
- change3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the medium3d runs: 74007d71bc
- medium3d: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the small3d runs: 74007d71bc
- small3d: builder claude-sonnet-5-5, cap 1,500,000 tokens and 120 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change3d-engine-1 | 181,845 | 0.38 | 45,665 | 561,677 | 13,721 | 18 | 21 | 4 | done | 20 | 2,671 / 9,432 | 11 / 11 | 1 / 1 | 4 | 1,655 |
| change3d-engine-2 | 163,731 | 0.34 | 43,006 | 471,510 | 12,566 | 15 | 16 | 4 | done | 16 | 1,980 / 6,644 | 11 / 11 | 1 / 1 | 5 | 1,256 |
| change3d-engine-3 | 286,867 | 0.53 | 54,667 | 1,288,783 | 17,934 | 31 | 31 | 6 | done | 49 | 7,899 / 15,361 | 11 / 11 | 1 / 1 | 4 | 1,601 |
| change3d-engine-4 | 156,410 | 0.33 | 43,040 | 448,671 | 11,550 | 15 | 17 | 4 | done | 24 | 3,448 / 11,071 | 11 / 11 | 1 / 1 | 6 | 1,204 |
| change3d-engine-5 | 161,031 | 0.34 | 42,200 | 453,281 | 12,592 | 15 | 17 | 3 | done | 19 | 2,613 / 6,024 | 11 / 11 | 1 / 1 | 4 | 1,319 |
| change3d-plain-1 | 294,426 | 0.58 | 55,423 | 884,637 | 27,339 | 22 | 25 | 3 | done | - | - | 11 / 11 | 1 / 1 | 2 | 1,211 |
| change3d-plain-2 | 272,869 | 0.54 | 57,908 | 927,659 | 21,546 | 23 | 25 | 3 | done | - | - | 11 / 11 | 1 / 1 | 3 | 1,587 |
| change3d-plain-3 | 300,072 | 0.58 | 62,049 | 1,135,583 | 21,793 | 26 | 30 | 3 | done | - | - | 11 / 11 | 1 / 1 | 3 | 1,753 |
| change3d-plain-4 | 268,199 | 0.54 | 56,362 | 774,016 | 24,071 | 20 | 25 | 3 | done | - | - | 10 / 11 | 1 / 1 | 2 | 1,403 |
| change3d-plain-5 | 265,013 | 0.54 | 62,435 | 791,231 | 21,571 | 18 | 25 | 3 | done | - | - | 11 / 11 | 1 / 1 | 2 | 1,953 |
| medium3d-engine-1 | 284,962 | 0.55 | 52,885 | 1,037,528 | 23,023 | 25 | 26 | 6 | done | 16 | 1,860 / 7,066 | 21 / 24 | 3 / 3 | 4 | 1,492 (850 at the start) |
| medium3d-engine-2 | 434,899 | 0.79 | 72,603 | 1,897,637 | 30,880 | 36 | 36 | 9 | done | 31 | 2,915 / 21,808 | 22 / 24 | 3 / 3 | 6 | 1,131 (850 at the start) |
| medium3d-engine-3 | 628,612 | 1.13 | 99,598 | 2,773,706 | 45,353 | 43 | 45 | 10 | done | 30 | 4,034 / 28,191 | 24 / 24 | 3 / 3 | 4 | 1,469 (850 at the start) |
| medium3d-engine-4 | 429,119 | 0.81 | 79,223 | 1,706,763 | 31,886 | 33 | 47 | 7 | done | 27 | 3,383 / 18,549 | 23 / 24 | 3 / 3 | 6 | 1,106 (850 at the start) |
| medium3d-engine-5 | 505,870 | 0.91 | 82,837 | 2,241,638 | 35,636 | 40 | 42 | 8 | done | 32 | 3,557 / 27,094 | 22 / 24 | 2 / 3 | 5 | 1,208 (850 at the start) |
| medium3d-plain-1 | 327,088 | 0.68 | 51,881 | 482,731 | 42,794 | 13 | 19 | 5 | done | - | - | 24 / 24 | 3 / 3 | 4 | 1,056 |
| medium3d-plain-2 | 435,055 | 0.90 | 66,964 | 667,918 | 56,913 | 14 | 18 | 7 | done | - | - | 23 / 24 | 3 / 3 | 3 | 1,446 |
| medium3d-plain-3 | 562,645 | 1.08 | 74,939 | 1,555,403 | 62,689 | 29 | 32 | 8 | done | - | - | 23 / 24 | 3 / 3 | 4 | 1,623 |
| medium3d-plain-4 | 364,737 | 0.73 | 54,718 | 769,991 | 43,870 | 19 | 22 | 5 | done | - | - | 24 / 24 | 3 / 3 | 4 | 1,227 |
| medium3d-plain-5 | 571,317 | 1.13 | 80,063 | 1,347,150 | 67,307 | 24 | 32 | 8 | done | - | - | 24 / 24 | 3 / 3 | 4 | 1,787 |
| small3d-engine-1 | 157,217 | 0.32 | 35,753 | 517,102 | 12,165 | 18 | 23 | 4 | done | 9 | 989 / 3,424 | 15 / 15 | 3 / 3 | 3 | 947 (850 at the start) |
| small3d-engine-2 | 195,611 | 0.40 | 42,463 | 591,559 | 16,677 | 18 | 25 | 5 | done | 13 | 1,720 / 6,119 | 15 / 15 | 3 / 3 | 3 | 980 (850 at the start) |
| small3d-engine-3 | 140,153 | 0.30 | 35,682 | 380,526 | 11,501 | 14 | 20 | 12 | done | 7 | 1,100 / 2,402 | 15 / 15 | 3 / 3 | 4 | 961 (850 at the start) |
| small3d-engine-4 | 148,153 | 0.30 | 34,928 | 458,214 | 11,736 | 16 | 21 | 3 | done | 11 | 906 / 4,663 | 15 / 15 | 3 / 3 | 4 | 957 (850 at the start) |
| small3d-engine-5 | 125,961 | 0.27 | 34,305 | 306,653 | 10,484 | 12 | 16 | 2 | done | 9 | 908 / 4,780 | 14 / 15 | 3 / 3 | 3 | 1,097 (850 at the start) |
| small3d-plain-1 | 131,578 | 0.28 | 23,445 | 150,754 | 17,440 | 8 | 9 | 2 | done | - | - | 15 / 15 | 3 / 3 | 5 | 589 |
| small3d-plain-2 | 122,375 | 0.26 | 22,026 | 143,669 | 16,096 | 8 | 9 | 2 | done | - | - | 15 / 15 | 3 / 3 | 4 | 597 |
| small3d-plain-3 | 117,525 | 0.26 | 21,260 | 116,984 | 15,851 | 7 | 8 | 2 | done | - | - | 15 / 15 | 3 / 3 | 5 | 549 |
| small3d-plain-4 | 94,481 | 0.21 | 18,285 | 85,979 | 12,606 | 6 | 7 | 1 | done | - | - | 15 / 15 | 3 / 3 | 3 | 543 |
| small3d-plain-5 | 91,610 | 0.20 | 17,398 | 83,153 | 12,310 | 6 | 6 | 1 | done | - | - | 15 / 15 | 3 / 3 | 4 | 511 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change3d | engine | 5 of 5 | 189,977 | 156,410 to 286,867 | 4 | 11.0 / 11 | 17,271 |
| change3d | plain | 5 of 5 | 280,116 | 265,013 to 300,072 | 3 | 10.8 / 11 | 25,937 |
| medium3d | engine | 5 of 5 | 456,692 | 284,962 to 628,612 | 8 | 22.4 / 24 | 20,388 |
| medium3d | plain | 5 of 5 | 452,168 | 327,088 to 571,317 | 7 | 23.6 / 24 | 19,160 |
| small3d | engine | 5 of 5 | 153,419 | 125,961 to 195,611 | 5 | 14.8 / 15 | 10,366 |
| small3d | plain | 5 of 5 | 111,514 | 91,610 to 131,578 | 2 | 15.0 / 15 | 7,434 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change3d | engine | 189,977 | 280,116 | 0.68 | 0.86 | 11.0 / 11 | 10.8 / 11 |
| medium3d | engine | 456,692 | 452,168 | 1.01 | 1.79 | 22.4 / 24 | 23.6 / 24 |
| small3d | engine | 153,419 | 111,514 | 1.38 | 2.23 | 14.8 / 15 | 15.0 / 15 |

Judging: 30 judging(s) of 30 run(s), 10,889,788 weighted tokens in all.

At the runner's list prices: 16.22 USD for the builds, 39.19 USD for the judging shown above (55.41 USD in all).

## Items that failed

- change3d-plain-4 **c02**: Rolling up works: with --board 4 and '60:right', y>0.5 at x=-0.30 (tick 133) and in flight at x=1.03, y=0.97 (tick 157). The ramp is not solid from the side: I drove the ball to (0, 0.25, 1.6) and pushed it up (-z) into the ramp. At z=0.61 the ball sat at y=-0.17, inside the ramp and below the slab top. It kept falling and cost a life. CHANGES.md admits there is only top-face contact.
- medium3d-engine-1 **m12**: Pushed right from the start (0:right, and 80:right after resting), and from a 4 m/s run-up (70:left,95:right). Each time the ball bounces back off the wall and stops at x=-0.47, never reaching x=1, so it is slowed. But no crate moves more than 0.03 (largest displacement 0.033 at tick 400). The 'at least two crates more than 0.3' part fails.
- medium3d-engine-1 **m13**: No crate can be pushed off the board: strikes at about 2.7 and 4 m/s, and a constant push held for 500 ticks against the wall, move crates at most 0.03. The removal code exists in lib.rs (y < -5 deletes the crate), but no crate can reach the edge.
- medium3d-engine-1 **m23**: At volume 0, sounds are still listed ([bounce, bounce, coin]). The game's report does not expose how loud anything played (run.sounds_heard is not a report line; --keys run.sounds_heard was refused), so silence at volume 0 could not be observed. The code calls cx.sound(name, None, 1.0) and relies on the engine (notes/how.md) to multiply by volume. The README only mentions run.sounds_heard's design and says this was not verified.
- medium3d-engine-2 **m13**: I could not push a crate off an edge in 5 scripts: an x=0 lane, a z=-0.5 lane, a z=-0.45 lane, a re-strike after a fall, and a +z push on the z=0.5 column. Each time the ball slipped round the crates and fell. Crates got only as far as x 3.23 or z 0.8, so I never saw a crate fall or leave 'crates'.
- medium3d-engine-2 **m23**: The README says nothing about volume scaling the sounds, and lib.rs plays every sound as cx.sound(s, None, 1.0) with no reference to the volume. Only the engine's notes/how.md says the engine scales by volume. 'sounds' does list the sounds at volume 0, but I could not observe the heard gain because run.sounds_heard is not in the report.
- medium3d-engine-4 **m23**: At volume 0, 'sounds' still lists the sounds. Whether they are silent cannot be seen: the report has no run.sounds_heard and a check asking for it says 'not in the state'. README says nothing about volume scaling sounds. The rules call cx.sound(s, None, 1.0) with no volume. Only the engine's notes/how.md says gain times volume.
- medium3d-engine-5 **m01**: play --ticks N --inputs ... --settings F prints the brief's JSON object, but the starting-board option does not start the run in play: `play work --ticks 3 --set board=2` prints screen title, board null, and the engine warns 'the run set board and no rule read it'. Board 2 only begins after 0:enter. The README admits this ('I found no way for a run to skip the title screen'). The brief says the run then starts in play on that board.
- medium3d-engine-5 **m23**: Not shown. The headless run has audio off, and the report has no sounds_heard line (asking for run.sounds_heard is refused), so no gain at volume 0 could be observed. The code plays every sound at gain 1.0. The README only says '(the engine scales it)' in a list of things the builder did not check, and does not point to where.
- medium3d-plain-2 **m22**: Title, settings, paused, won frames show their Spanish texts and the play frame shows TABLERO 2 DE 3, PUNTOS, VIDAS; but the Spanish lost frame (es_lost.png) cuts the banner to N DE LA PARTI: Fin de la partida runs off both sides of the 800 px frame.
- medium3d-plain-3 **m13**: I could not push any crate off the board with inputs: about 15 scripted tries (strikes from several z lanes, pushes in +z/-z, repeated strikes) left all 6 crates on the board. The ball cannot move two crates with a held push. A diagnostic that set a crate moving at the edge showed it falls and is dropped from crates below y=-5 (6 -> 5), but no input did that.
- small3d-engine-5 **s10**: README win script: t680 score 40, coins [], won true and frozen through t850 despite inputs, but `over` is false (printed "over":false,"won":true at t680..t850). The item requires both won and over true; the rules set over only on the third fall (lib.rs:181).

## What each judge found beyond the list

- change3d-engine-1: README.md and CHANGES.md say a board-4 winning script is given in CHANGES.md, but no script is written there; I had to find one with the engine.
- change3d-engine-1: README 'What I checked' still says the Spanish HUD shows 'Tablero 1 de 3' (stale; the game now shows 'de 4').
- change3d-engine-1: On the won screen the ball is frozen in mid-air where the last coin was taken (harmless, but odd in the frame).
- change3d-engine-1: No checks/ folder: 'leverage check' has nothing of the game's own to play.
- change3d-engine-2: Board 2: the first impact of the ball on the crates is the same for every mass (ball rebound and crate positions identical to 1e-6 at ticks 130-150 for light, normal, heavy); mass only shows while the ball keeps pushing into the crates afterwards.
- change3d-engine-2: The ramp is 2 long along its slope, so its top face spans only about x -0.94..0.94 rather than -1..1 (the builder says so in CHANGES.md).
- change3d-engine-2: Language and Ball choices do not wrap around: menu_right on French stays French, menu_left on Light stays Light.
- change3d-engine-2: README 'Files' line says en.json and es.json hold 'every word in the three languages' (fr.json is separate); minor wording error.
- change3d-engine-2: On board 4 a ball that flies past x=4 leaves the fixed camera's view before it falls.
- change3d-engine-3: The ball's mass makes no difference at the moment of the strike: on board 2 with the push released before contact (0:enter,100:right,160:none), light, normal and heavy rebound identically to 1e-6 and no crate moves for any of them. Mass only matters while the push is held into the crates, so the brief's 'how hard the ball strikes the crates' holds only for a pressed ball.
- change3d-engine-3: README 'How it is put together' still says assets/strings has en.json and es.json 'in both languages' (fr.json is only mentioned in the 'Added later' note).
- change3d-engine-3: The ramp is 2.12 m long along its slope rather than 2 (deliberate and documented in CHANGES.md, so the top face spans x -1..1).
- change3d-engine-3: There is no checks/ folder, so 'leverage check' on the game has nothing of its own to run (CHANGES.md says so).
- change3d-engine-4: README's file list still says assets/strings holds en.json and es.json 'in both languages'; fr.json is not named there (the top line does mention fr).
- change3d-engine-4: On the title frame in French the 'Commencer' label fills its button to the edges (the button does not widen for the longer word).
- change3d-engine-4: On the first tick of a run (tick 0 with 0:enter) the HUD texts show unfilled templates such as 'Plateau {work.board} sur 4'; filled from the next tick.
- change3d-engine-4: Language does not wrap: menu_right at French and menu_left at English do nothing.
- change3d-engine-4: A ball on a moving bridge keeps any small own velocity forever (no rolling resistance), so it only rides the bridge when stopped almost exactly; same as before the change.
- change3d-engine-4: m23 (volume 0 silence) was not rechecked; it failed before the change and the rules still call cx.sound(s, None, 1.0).
- change3d-engine-5: The Language and Ball settings stop at the ends with Left/Right (French then Right stays French); they do not wrap around.
- change3d-engine-5: CHANGES.md and README give the board-4 winning script elided with '...', so it cannot be run as written.
- change3d-engine-5: README's file map still names only en.json and es.json for the words (fr.json is not listed there).
- change3d-engine-5: The run still opens on the title: --set board=N needs 0:enter first (m01 failing before the change, unchanged).
- change3d-plain-1: French won screen frame: 'VOUS AVEZ GAGNÉ' is wider than 800 px, its first and last letters are cut off at the edges.
- change3d-plain-1: The ramp's underside is not shown as distinct from its top in the frame; tilt is mostly read from the thickness face at the high end.
- change3d-plain-2: README says ready-made WAVs are in sounds/, but work/sounds holds only bounce.wav and clear.wav (coin.wav and fall.wav are missing); --all-wavs regenerates all four.
- change3d-plain-2: A push held when a board is cleared stays held on the next board (the input stays set until changed). This was already the game's behaviour and is harmless, but the next board starts with the push applied.
- change3d-plain-2: The ball gives 'bounce' sounds when it reaches the foot of the ramp (noted by the builder).
- change3d-plain-3: Without a settings folder the game would write saved/settings.json inside the game folder when a setting changes (default folder is next to roll.py) - by design, noted only.
- change3d-plain-3: On board 4 a held right push from the start carries the ball off the far edge (falls) after taking the air coin; the push must be released to stay on the board - not a brief requirement.
- change3d-plain-3: French 'Volume' and 'Score' are the same spelling as English; they are correct French words.
- change3d-plain-4: Board 4 ramp has no side, end or underside collision. A ball rolling into the ramp from the side (or from the high end at floor level) sinks through it and the slab beneath and falls out of the world, costing a life.
- change3d-plain-4: French 'FIN DE PARTIE' heading on the lost frame spans almost the full 800 px width (touches both margins).
- change3d-plain-5: Board 2: after the ball strikes the wall, crates end up overlapping each other (light ball tick 188: crate 4 at (0.09,0.27,-0.02) inside crate 1 at (0.10,0.25,0.00); normal ball: crates 2 and 5 0.07 apart; heavy ball: two crates 0.11 apart). They overlap with every ball mass, so the fault may predate the change.
- change3d-plain-5: Board 2 with a normal ball and right held: the ball stays pressed against the crate wall at x~-0.07 for 600 ticks. After the first knock the crates stop and do not move again however long the push is held, so the wall then acts like a fixed wall.
- medium3d-engine-1: The crates are practically immovable: the ball at up to about 4 m/s, or pushing steadily, moves them at most 0.03 m. The README's 'crates stand as a wall and fall/topple when hit' is not what the game does.
- medium3d-engine-1: --set board=N does not start the run in play on that board as the brief asks: without an enter at tick 0 the run stays on the title, and the engine warns that no rule read 'board'. The README says '--set board=N starts in play on board N'.
- medium3d-engine-1: Board 3: the ball can wedge in the gap between the first slab and the bridge (y about -0.06, ticks 300-360 of the winning script), giving a burst of 7 bounce sounds within about 30 ticks.
- medium3d-engine-1: The title menu focus does not wrap (menu_down on Quit stays on Quit). Minor, and not asked for.
- medium3d-engine-2: --set board=N alone leaves the run on the title. The script needs 0:enter first, although the brief says the run starts in play on that board (the README states this).
- medium3d-engine-2: 'selected' reports 'Back_to_title' and 'Quit_to_title' (ids with underscores), not the English words shown.
- medium3d-engine-2: The title-screen frame shows the red ball floating behind the title panel.
- medium3d-engine-2: Crates hardly react to impact: a ball at about 4 m/s bounces back off the wall and no crate moves. They only slide under a held push and never topple, so pushing one off an edge is very hard in practice.
- medium3d-engine-2: 'bounce' also plays when the ball rolls level from the first slab onto the bridge (no landing). The ball also gains speed (about 2.0 to 2.5 m/s) when it rolls onto the moving bridge.
- medium3d-engine-2: The report has an extra 'phase' key that the brief does not ask for (harmless).
- medium3d-engine-3: The board option alone (--set board=2 with no inputs) leaves the game on the title, and the engine warns that no rule read 'board'. A '0:enter' must be in the script; the README documents this, but the brief says the run then starts in play on that board.
- medium3d-engine-3: A crate or ball that drops below y = -5 is still reported for one tick: a crate at y -5.079 is in crates at tick 470 and gone at tick 471, and the ball at y -5.13 at tick 472 costs a life only at tick 473.
- medium3d-engine-3: A held push is lost through pause and resume: 'right' held from tick 60, then escape at 200 and Resume at 300; after resuming the ball coasts and stops instead of being pushed on.
- medium3d-engine-3: In the crate strike the ball bounces back off the wall and then shoves the middle stack whole. The top crate rides along without toppling, and the outer stacks stay where they stood.
- medium3d-engine-4: --set board=N alone leaves the run on the title. It needs 0:enter in the inputs, but the brief says the board option starts the run in play.
- medium3d-engine-4: On the pause menu, 'selected' reads 'Quit_to_title' and not its English words 'Quit to title'.
- medium3d-engine-4: On the settings screen, 'texts' still holds the title's texts (Roll/Start/Settings/Quit) as well as the settings texts.
- medium3d-engine-4: On Language, menu_left while English is chosen does nothing; menu_right cycles both ways.
- medium3d-engine-4: A crate that has fallen below y = -5 is still listed for one tick (state at tick 305 shows y -5.10).
- medium3d-engine-4: README checks are written against .../work while the judge's instructions run the parent folder. Both work.
- medium3d-engine-5: selected reads 'Quit_to_title', not the English words 'Quit to title' as the brief asks.
- medium3d-engine-5: A crate knocked from the top row can ride on top of the rolling ball (crate at y 0.78-0.80 above the ball at x 0.7-1.1).
- medium3d-engine-5: A ball wedged between the first slab's edge and the moving bridge was flung up to y 1.17 (board 3, 0:enter,250:right,280:none,365:left,396:none, t660-720).
- medium3d-engine-5: The won/lost screens list the play HUD texts as well as their own in texts; the lost HUD shows 'Lives: 0'.
- medium3d-engine-5: For board 1, crates is [] rather than null; harmless.
- medium3d-plain-1: Crates only translate, never rotate: the brief asks that the ball topple them; the README admits this.
- medium3d-plain-1: A strike only moves the crate column it hits; the stacked crate drops straight down instead of tipping.
- medium3d-plain-1: Ball vs slab sides not modelled (README admits): a ball past an edge just falls.
- medium3d-plain-1: Menus wrap around (menu_down on Quit returns to Start); harmless but not asked for.
- medium3d-plain-2: Spanish Game over banner (Fin de la partida) is drawn wider than the 800 px frame and is clipped on both sides.
- medium3d-plain-2: Coins are drawn as upright discs at a spinning angle, so one can show as a barely visible thin sliver in a frame (coin at (-1.5, 1.5) on board 2).
- medium3d-plain-2: On lost/won screens the state nulls ball, coins and crates, so the frozen board cannot be confirmed from the state alone (the code does stop stepping).
- medium3d-plain-3: Crates go into each other: after strikes on board 2, two crates rested at (0.82,0.25,0.00) and (0.80,0.25,0.01), and in another run a stacked pair was only 0.34 apart in y.
- medium3d-plain-3: A held push cannot move two crates: the ball stays pressed on the wall at x of about 0.66 for over 1000 ticks, and a top crate stays half over the edge of the one under it without toppling.
- medium3d-plain-3: Crates ignore the sides of slabs (the README says this).
- medium3d-plain-3: ball and board are null on the won and lost screens.
- medium3d-plain-4: Crates never rotate. In the strike only the middle column moves: the bottom crate is bulldozed ahead of the ball and the top one drops straight down. Crates never topple, as the README admits.
- medium3d-plain-4: Ground support is tested only at the ball's centre, and a ball up to 0.3 below the top surface (y > -0.05) is snapped back on top. A bouncing ball can therefore cross a 0.5 m gap to the bridge and pop up through its side (board 3 with 0:right clears without the bridge needing to be adjacent).
- medium3d-plain-4: The bitmap font is uppercase only, so the frames show texts in capitals rather than exactly as in texts.
- medium3d-plain-4: Ball-to-board landing only checks the centre, so a ball whose centre is just past an edge drops through, though its body overlaps the slab.
- medium3d-plain-5: The window game never plays a sound: audio.play calls winsound.PlaySound with SND_MEMORY | SND_ASYNC, which raises RuntimeError 'Cannot play asynchronously from memory' (checked with a silent buffer); the except swallows it.
- medium3d-plain-5: Crates pass into each other: after the board-2 strike ('0 right,60 none', 400 ticks) crates rest in pairs with centres about 0.06 apart at the same height, e.g. (0.134, 0.256, -0.471) and (0.189, 0.257, -0.493); such pairs then move and fall together.
- medium3d-plain-5: A ball rolling fast into the gap on board 3 can be thrown back up by the bridge's side face (y 0.23 in the gap, then 0.63 on the bridge, then a 1.16 hop onto the second slab) instead of falling.
- medium3d-plain-5: In the PNG font the letter V looks like W (LIWES, WOLUMEN, WIDAS).
- small3d-engine-1: A held push on the board is scaled by 7/5 to offset rolling (README admits this). Measured: about 4.1-4.3 m/s2 along the board, so close to the brief's 4 but not derived exactly.
- small3d-engine-1: The Enter/restart input given at tick T shows in the state only at tick T+1 (the state at T still shows the old run); minor.
- small3d-engine-1: Each headless run takes about 20 s because the module is rebuilt every time; slow but works.
- small3d-engine-2: Fall is detected one tick late: the reported state at t256 shows y = -5.118 with lives still 3 before the reset at t257.
- small3d-engine-2: At --ticks 0 the report prints score, lives, over, won and coins as null.
- small3d-engine-2: The win/lose panel covers the middle of the board, so the ball can be hidden behind it in the end frames.
- small3d-engine-3: The ball is reported one tick below y = -5 with lives unchanged (tick 256: y -5.118, lives 3) before the reset happens on the next tick.
- small3d-engine-3: Running the sounds command prints engine warnings that a screen shows {game.score} and {work.lives} that nothing keeps (the HUD in frames still shows the numbers correctly).
- small3d-engine-3: At tick 0 the report shows score, lives, over, won and coins as null (README states this).
- small3d-engine-3: The end screens show a 'Title' button, and the title/pause/settings screens are mostly the sample's (README states this).
- small3d-engine-4: Enter during a run (before the end) does nothing; brief only requires restart after the end, README admits it.
- small3d-engine-4: Sample title/pause/settings screens remain in the project unused (README admits it).
- small3d-engine-4: End screens show 'Play again'/'Quit' buttons inherited from the sample; harmless.
- small3d-engine-4: After the third fall the ball is left below the board at y=-5.17 rather than reset; allowed by the brief (nothing moves after the end).
- small3d-engine-5: On a win `over` stays false: the report gives won true, over false, so a reader of `over` alone would think the run is still going.
- small3d-engine-5: After the third fall the ball is left at y = -5.28 (not reset) and the engine prints a warning on stderr that /work/ball has fallen out of the world.
- small3d-engine-5: Coins are drawn as small flattened ovals, hard to read as coins (the README admits this).
- small3d-plain-1: After a restart, sounds is not cleared: the list keeps the sounds from before the restart.
- small3d-plain-1: Restart is accepted at any time, not only after the end (the game logic resets on 'restart' even mid-run).
- small3d-plain-1: The ball does not hit the slab's side faces; the README itself says contact only checks whether the centre is over the board.
- small3d-plain-1: At game over the ball is left frozen below the board (y=-5.05, x=9.6), so it is not drawn on the game-over frame.
- small3d-plain-1: The interactive 'play' window was not tested by the builder, and the judge did not test it either (no windows may be opened on this machine).
- small3d-plain-2: The sounds list is not cleared by a restart, so after a restart it still holds the sounds of the previous run (the brief asks for sounds played so far in this run).
- small3d-plain-2: After the third fall the ball is left where it was, below the board (x 10.14, y -5.07), rather than being put back at the start.
- small3d-plain-2: The bitmap font draws M as a shape that reads more like an H ('GAHE OVER'), though the text can still be read.
- small3d-plain-2: The README frame command writes frame.png into the work folder, and play writes a sounds/ folder there; the interactive tkinter window was not opened by the builder or by the judge.
- small3d-plain-3: The 3x5 pixel font W (rows 101,101,101,111,101) looks like an H, so the win banner reads close to 'YOU HIN'.
- small3d-plain-3: End banners are not centred: the big text is placed for 24 px per character, but each one is 32 px wide.
- small3d-plain-3: A restart keeps the old entries in 'sounds', so after a restart the list still holds the previous run's sounds. The README documents this.
- small3d-plain-3: The ball does not collide with the slab's side or underside. The README admits this.
- small3d-plain-3: The interactive tkinter mode was not run, either by the builder (as their README says) or by the judge.
- small3d-plain-4: The sounds list is not cleared on restart, so after a restart it still holds the previous run's sounds (the brief says sounds played so far in this run).
- small3d-plain-4: The ball does not collide with the board's side faces; once its centre passes |x|>3 it drops straight through where the slab edge is (README admits this).
- small3d-plain-4: Interactive play mode was not exercised by the judge (opening a window is forbidden here); only the headless core was judged.
- small3d-plain-5: In a headless run, sounds (and tick) keep building up across a restart, so after a restart sounds still lists the earlier run's sounds; the README says so, but the brief's 'every sound played so far in this run' suggests a fresh list.
- small3d-plain-5: The 3x5 pixel font's W is crude (it reads close to an H), so 'YOU WIN' is only just readable.
- small3d-plain-5: The interactive play window (tkinter) was not tested, by the builder or by me (it would take the keyboard); its look is a simpler polygon drawing than the PNG.
- small3d-plain-5: Inputs are exclusive: the play window maps held keys to one direction at a time, so diagonal pushes are not possible.

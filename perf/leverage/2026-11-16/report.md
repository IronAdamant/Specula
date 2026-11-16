# Measured leverage: the runs

Folder: `p3`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 2a0c5669
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 2a0c5669
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 2a0c5669
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 320,482 | 0.60 | 62,934 | 1,394,709 | 20,472 | 33 | 33 | 16 | done | 6,264 | 136,591 / 3,072,493 | 9 / 9 | 1 / 1 | 5 | 1,641 |
| change-engine-2 | 172,787 | 0.37 | 42,305 | 417,225 | 15,638 | 14 | 17 | 20 | done | 9,074 | 311,519 / 7,319,066 | 9 / 9 | 1 / 1 | 3 | 1,298 |
| change-plain-1 | 109,265 | 0.23 | 27,620 | 253,006 | 9,889 | 12 | 14 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 681 |
| change-plain-2 | 119,295 | 0.25 | 29,172 | 290,869 | 10,750 | 13 | 13 | 2 | done | - | - | 9 / 9 | 1 / 1 | 3 | 693 |
| medium-engine-1 | 1,055,414 | 1.77 | 136,692 | 5,436,350 | 68,189 | 61 | 62 | 16 | done | 3,290 | 70,735 / 1,646,613 | 18 / 19 | 2 / 2 | 5 | 1,503 (528 at the start) |
| medium-engine-2 | 802,492 | 1.37 | 112,919 | 4,040,632 | 51,461 | 50 | 54 | 14 | done | 3,464 | 118,070 / 5,732,481 | 19 / 19 | 2 / 2 | 5 | 1,187 (528 at the start) |
| medium-plain-1 | 204,994 | 0.44 | 34,750 | 218,457 | 27,943 | 9 | 12 | 3 | done | - | - | 19 / 19 | 2 / 2 | 3 | 583 |
| medium-plain-2 | 257,115 | 0.54 | 43,105 | 396,348 | 32,721 | 13 | 21 | 4 | done | - | - | 19 / 19 | 2 / 2 | 6 | 585 |
| small-engine-1 | 451,899 | 0.84 | 78,669 | 1,835,104 | 34,014 | 36 | 42 | 6 | done | 62 | 26,561 / 154,469 | 14 / 14 | 2 / 2 | 3 | 970 (528 at the start) |
| small-engine-2 | 905,193 | 1.48 | 111,559 | 5,007,779 | 53,000 | 68 | 70 | 21 | done (past the token cap) | 297 | 18,887 / 241,401 | 13 / 14 | 2 / 2 | 3 | 597 (528 at the start) |
| small-plain-1 | 102,714 | 0.22 | 18,618 | 118,602 | 13,517 | 8 | 9 | 2 | done | - | - | 14 / 14 | 2 / 2 | 4 | 274 |
| small-plain-2 | 96,839 | 0.21 | 17,822 | 95,604 | 13,001 | 7 | 8 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 258 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 2 of 2 | 246,634 | 172,787 to 320,482 | 18 | 9.0 / 9 | 27,404 |
| change | plain | 2 of 2 | 114,280 | 109,265 to 119,295 | 2 | 9.0 / 9 | 12,698 |
| medium | engine | 2 of 2 | 928,953 | 802,492 to 1,055,414 | 15 | 18.5 / 19 | 50,214 |
| medium | plain | 2 of 2 | 231,054 | 204,994 to 257,115 | 3 | 19.0 / 19 | 12,161 |
| small | engine | 2 of 2 | 678,546 | 451,899 to 905,193 | 14 | 13.5 / 14 | 50,263 |
| small | plain | 2 of 2 | 99,776 | 96,839 to 102,714 | 1 | 14.0 / 14 | 7,127 |

Judging: 12 judging(s) of 12 run(s), 2,847,572 weighted tokens in all.

At the runner's list prices: 8.32 USD for the builds, 10.14 USD for the judging shown above (18.46 USD in all).

## Items that failed

- medium-engine-1 **m14**: Pause works: escape at 11, state at tick 200 is still walker x 19.775 and guard 7.0, texts [Paused, Resume, Title]. Title works: down+enter gives the title. Resume does not continue from the same state. Escape at 11, enter at 100, 1 tick: walker at 19.575, but pausing at 11 plus 1 step should give 19.75 (7 extra steps). With up held it was 19.375 (15 extra steps) while the guard moved only 1 step. The headless script lets the engine clock run in real time after Resume, before it steps again.
- small-engine-2 **s12**: From a cold engine, the frame of seed 1, 330 ticks, 0:right,60:none is correct: 'Score: 0  Misses left: 1', paddle at x 9-12, and three pieces where the state puts them (40 px per unit). Run after any earlier run, or after an in-game restart (0:none,400:restart, 700 ticks), the same frame also draws a stale piece at (3.5,14.9) that is not in the state. One frame (play.png) showed six pieces against the state's three. The frame does not match the state.

## What each judge found beyond the list

- change-engine-1: checks/run_checks.py writes its scratch into work/.scratch, inside the game folder (I did not run it for that reason).
- change-engine-1: From reading tools/headless.py (not run): Left on Language sends the same cycle:language action as Right, so Left steps forward instead of back.
- change-engine-1: Pause/Resume still lets the engine clock run on after Resume (m14 from before), unchanged by this work.
- change-engine-1: README's 'Add a language' list starts with a step 0 that refers to things explained only later; minor.
- change-engine-1: l10n_check theme contrast error in assets/ui/theme.json (reported by the builder; not seen on any screen).
- change-engine-2: README's 'What I checked' section is stale: it still reports a win of 45 walkers for 450 points in 5420 ticks, from before the sixth wave.
- change-engine-2: A headless run takes real time (about 60 ticks/s, about 2 min for a full run; the autoplay bot took about 6 min), as the README admits.
- change-engine-2: Settings are kept only by the headless runner, not when the game is played in the engine window (stated in README/CHANGES).
- change-plain-1: French won screen frame: 'VOUS AVEZ GAGNÉ !' at scale 8 is 808 px wide, so the first V and the '!' are clipped at the 800 px edges of the PNG.
- change-plain-1: Difficulty and Language wrap around (Hard -> Easy on Right) instead of stopping at the ends; the brief does not forbid this.
- change-plain-2: README still says state 'language' is the code en/es; fr is not mentioned there.
- change-plain-2: When stdout is piped on this Windows machine, the headless JSON is written in the locale code page (cp1252), not UTF-8, because of print with ensure_ascii=False; a UTF-8 reader fails on accented texts. This is probably unchanged from before the change.
- change-plain-2: The tkinter play window is untested by the builder (stated in CHANGES.md), and the judge did not open it under the machine rules.
- medium-engine-1: Pause/Resume in a headless script is not deterministic: after Resume the engine clock runs in real time (7-15 uncounted walker steps, no held input) before the next scripted step. So the same seed and script with a pause can give different states depending on machine speed.
- medium-engine-1: Spanish end screens say 'Enter: título' (the key name 'Enter' is left in English; Spanish keyboards usually call it 'Intro').
- medium-engine-1: In a window (per README), Left/Right do nothing on Settings; volume only steps with Enter and wraps 10 -> 0. The brief asks for Left/Right.
- medium-engine-1: Quit cannot be chosen in the headless run (README: the engine refuses it to agents), so I could not observe what Quit does.
- medium-engine-1: Running the frame command leaves engine capture files in work/captures/ (written by the game's own sight_frame call).
- medium-engine-2: After leaving a run (pause -> Title, or Enter on won), the state still reports the old walkers/bolts on the title screen (e.g. 3 walkers on title after pause->Title; 2 bolts on title after won).
- medium-engine-2: On the won screen the leftover bolts stay frozen in the state and are drawn in the frame (won.png).
- medium-engine-2: Volume/language are saved only by the headless runner (settings.json written by tools/gate.py); a game played in the engine's window starts with defaults each time (README admits it).
- medium-engine-2: Quit on the title screen does nothing.
- medium-engine-2: Wave 2's first walker appears 181 ticks after the last walker of wave 1 is gone (one tick over 3 s); negligible.
- medium-plain-1: The Spanish end-screen string is "Enter: título", which keeps the English key name "Enter" (the usual Spanish is "Intro"). It is on the won/lost screens only, which m05 does not cover.
- medium-plain-1: The guard can stand exactly on y=0 or y=12, so half its sprite is off the field (at the top it sits under the HUD).
- medium-plain-1: The state's wave number changes to the next wave 179 ticks after the previous wave ends. The first walker appears at 180 ticks (one tick difference).
- medium-plain-2: The JSON is printed in the console code page when piped on Windows (cp1252 here), not UTF-8: Spanish output (e.g. 'Configuración') fails to decode as UTF-8; a reader must use the locale encoding.
- medium-plain-2: On won/lost screens the frame still draws the HUD (Score/Wave/Gate top bar), but 'texts' lists only the three overlay strings, so texts is not every string on screen.
- medium-plain-2: A bolt fired exactly 0.6 units off a lane (guard_y reported 4.6 after 14 down ticks) misses because of float drift; the 'within 0.6' edge is not inclusive in practice.
- medium-plain-2: Guard can sit at y = 0 or y = 12, so half of its sprite is drawn outside the field at the edges.
- medium-plain-2: README's first headless example ('5:down,6:enter,...') opens Settings rather than starting play, so its shoot/escape inputs do nothing; misleading as an example.
- medium-plain-2: The tkinter play window was not checked (the builder's README says it was never opened, and judge rules forbid opening it).
- small-engine-1: A piece is counted as missed one tick after it reaches the bottom edge. At tick 226 its centre is at y 0.5 (its bottom on the edge) with misses still 0, and the miss is counted at tick 227. Floating-point build-up of 4/60 makes the y <= 0.5 test miss by a hair.
- small-engine-1: README says a player opening the game sees the engine's title screen and must press Start first. The builder never played it with a window or keyboard, and I did not either.
- small-engine-1: Each headless run builds the module and starts and stops the engine, so even short runs take about 5 s.
- small-engine-2: Piece entities from a previous run are not removed when a run starts or restarts (the delete query in start does not remove them). They stay frozen on screen, with no matching piece in the state, until their id is reused.
- small-engine-2: README says that with no input the run ends at tick 372; the game is already over at tick 371 (minor documentation error).
- small-engine-2: The headless 'tick' field just echoes the -Ticks argument rather than reporting a count from the game; the state values I checked agree with it.
- small-plain-1: Float drift delays a miss by one tick: at tick 226 (seed 1 idle) the piece centre is shown at y=0.5, so its bottom edge is at 0, but the miss is only counted at tick 227.
- small-plain-1: Pieces near the top edge are drawn over the HUD text (score / misses left); the HUD has no backing.
- small-plain-1: The game-over panel hides pieces under it in the frame (cosmetic).
- small-plain-1: The interactive 'play' window was never opened by me (machine rules) nor, per its README, by the builder; it is unverified.
- small-plain-2: Float rounding: a piece whose centre is at y 0.5 (its bottom on the bottom edge) is still listed and not yet counted missed (seed 1 idle, ticks 225 and 369). The miss is counted one tick later.
- small-plain-2: In the frame a piece near the top is drawn under the HUD text and partly hidden by it (seed 1, tick 300, piece at y 14.7).
- small-plain-2: The PNG game-over panel labels the final score 'SCORE n', while the window code labels it 'Final score n', so the two views use different wording (cosmetic).

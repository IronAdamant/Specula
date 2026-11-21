# Measured leverage: the runs

Folder: `lv2d`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 7f2167f8df
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 7f2167f8df
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 7f2167f8df
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 115,400 | 0.24 | 31,596 | 356,968 | 8,043 | 14 | 15 | 3 | done | 15 | 1,485 / 3,801 | 8 / 9 | 1 / 1 | 3 | 1,300 |
| change-engine-2 | 128,399 | 0.27 | 32,895 | 353,820 | 10,381 | 14 | 16 | 7 | done | 16 | 1,099 / 2,143 | 9 / 9 | 1 / 1 | 4 | 1,271 |
| change-plain-1 | 100,774 | 0.22 | 26,195 | 241,962 | 8,768 | 12 | 17 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 729 |
| change-plain-2 | 106,475 | 0.23 | 27,745 | 254,451 | 9,271 | 12 | 17 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 782 |
| medium-engine-1 | 192,666 | 0.39 | 39,178 | 550,921 | 17,722 | 18 | 20 | 3 | done | 13 | 716 / 2,840 | 19 / 19 | 2 / 2 | 3 | 1,209 (792 at the start) |
| medium-engine-2 | 174,715 | 0.36 | 36,904 | 415,623 | 17,406 | 15 | 17 | 3 | done | 14 | 1,399 / 3,326 | 19 / 19 | 2 / 2 | 7 | 1,137 (792 at the start) |
| medium-plain-1 | 150,721 | 0.32 | 26,753 | 206,802 | 19,321 | 10 | 15 | 2 | done | - | - | 19 / 19 | 2 / 2 | 5 | 644 |
| medium-plain-2 | 151,131 | 0.33 | 27,224 | 153,699 | 20,347 | 8 | 13 | 2 | done | - | - | 19 / 19 | 2 / 2 | 4 | 697 |
| small-engine-1 | 127,371 | 0.26 | 28,111 | 326,689 | 11,914 | 14 | 19 | 3 | done | 14 | 3,908 / 2,225 | 14 / 14 | 2 / 2 | 3 | 841 (792 at the start) |
| small-engine-2 | 112,973 | 0.24 | 26,190 | 257,363 | 10,901 | 12 | 15 | 2 | done | 10 | 447 / 1,946 | 14 / 14 | 2 / 2 | 2 | 890 (792 at the start) |
| small-plain-1 | 70,903 | 0.15 | 13,308 | 90,011 | 9,054 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 5 | 361 |
| small-plain-2 | 64,546 | 0.14 | 12,288 | 100,355 | 7,831 | 8 | 8 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 289 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 2 of 2 | 121,900 | 115,400 to 128,399 | 5 | 8.5 / 9 | 14,341 |
| change | plain | 2 of 2 | 103,624 | 100,774 to 106,475 | 1 | 9.0 / 9 | 11,514 |
| medium | engine | 2 of 2 | 183,690 | 174,715 to 192,666 | 3 | 19.0 / 19 | 9,668 |
| medium | plain | 2 of 2 | 150,926 | 150,721 to 151,131 | 2 | 19.0 / 19 | 7,943 |
| small | engine | 2 of 2 | 120,172 | 112,973 to 127,371 | 2 | 14.0 / 14 | 8,584 |
| small | plain | 2 of 2 | 67,724 | 64,546 to 70,903 | 1 | 14.0 / 14 | 4,837 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change | engine | 121,900 | 103,625 | 1.18 | 1.17 | 8.5 / 9 | 9.0 / 9 |
| medium | engine | 183,691 | 150,926 | 1.22 | 1.83 | 19.0 / 19 | 19.0 / 19 |
| small | engine | 120,172 | 67,725 | 1.77 | 1.73 | 14.0 / 14 | 14.0 / 14 |

Judging: 12 judging(s) of 12 run(s), 2,483,776 weighted tokens in all.

At the runner's list prices: 3.15 USD for the builds, 9.58 USD for the judging shown above (12.73 USD in all).

## Items that failed

- change-engine-1 **c01**: Own --follow bot, 1-tick steps (seed 3, Normal): wave 6 sent 15 runners, kind runner, health 1, moving 3.0 units/s, but new runners appeared every 60 ticks (1 s), not 90 ticks (1.5 s). lib.rs has RUNNER_SPAWN_EVERY = 60, and CHANGES.md says the same.

## What each judge found beyond the list

- change-engine-1: Wave 6 runners spawn every 1 s (RUNNER_SPAWN_EVERY = 60), not every 1.5 s.
- change-engine-1: README Layout still says assets/strings/ holds (en, es); fr is not mentioned there.
- change-engine-1: Changing difficulty mid-run does not change walkers already on the field (CHANGES.md notes this).
- change-engine-2: CHANGES.md says walkers come every 89 ticks; I measured 90 (it is wrong about the game, not the game wrong).
- change-engine-2: Language Right stops at French and does not wrap, while Enter wraps to English (cycle by Enter is the documented way); the same holds for Difficulty.
- change-engine-2: README's check `python tools/bot.py` runs `play .` and so works only when started from the work folder; README does not say so.
- change-engine-2: Module description still says 'five waves' (not shown to players).
- change-plain-1: README still says 'Each kill is 10 points' (runners give 15) and its colour legend and Settings controls do not mention runners or the Difficulty row.
- change-plain-1: Spanish won/lost screens say 'Enter: título' with the English word Enter (pre-existing, not part of this change).
- change-plain-1: check/bot_test.py writes a settings file into the system temp folder, outside the work folder.
- change-plain-2: On the won/lost screens, bolts still in flight stay frozen in the state and the frame (cosmetic).
- change-plain-2: The HUD score text starts at x = 10, while the gate bar covers x 0-12, so the first letter's left edge overlaps the bar (cosmetic, pre-existing).
- change-plain-2: The Spanish 'enter_title' string is 'Enter: título' and keeps the English key name 'Enter' (pre-existing; French uses 'Entrée').
- medium-engine-1: texts lists strings of screens drawn underneath. In Settings it includes the title strings (Hold the gate, Start, Settings, Quit), though the settings frame does not show them, so texts is not exactly 'every string on screen now'.
- medium-engine-1: After a run ends and Enter returns to the title, the state still reports the old score (450), wave 5, guard_y and leftover bolts, instead of resetting.
- medium-engine-1: Before the first run the state reports wave 0 (README admits this).
- medium-engine-2: Before play starts, walkers and bolts are reported as null rather than empty lists (the README admits this).
- medium-engine-2: Settings screen 'texts' also lists the title screen's four strings, which sit hidden behind the settings panel.
- medium-engine-2: The title's Start button always has the blue 'primary' fill, so when another entry is focused two buttons look highlighted; only the focus ring tells which is chosen.
- medium-engine-2: Quit on the title does nothing in the headless run (screen stays title); its effect in a window was not checked.
- medium-engine-2: After a wave, the wave counter moves at +180 ticks but the first walker of the next wave appears at +181 ticks.
- medium-engine-2: The guard is clamped to y 0.5..11.5 rather than the full 0..12 field (documented as the body staying inside).
- medium-engine-2: Bolts still in flight stay in the state on the won screen.
- medium-plain-1: Stale field objects stay in the printed state off the play screen: the won screen still lists bolts, and the title screen after Pause->Title still lists the walker and bolts from the abandoned run.
- medium-plain-1: A walker that reaches x = 0.0 is still listed for one tick before the gate takes the hit (x <= 0 is checked on the following step).
- medium-plain-1: Volume is stored and shown but the game has no sound (the README says so).
- medium-plain-1: The Spanish end screens keep the English key name 'Enter' ('Enter: titulo').
- medium-plain-1: The window mode (tkinter) is untested by the builder and was not opened by me, as the rules required.
- medium-plain-2: Spanish won/lost hint is 'Enter: título' - keeps the English key name 'Enter' (arguably a key label, not translated).
- medium-plain-2: Volume is stored and shown but the game makes no sound (README says so).
- medium-plain-2: Default settings path is settings.json inside the game folder; the tkinter window mode was not checked (not opened, per machine rules; builder also says it is untested).
- medium-plain-2: The next wave's counter flips 179 ticks after the last walker is removed (first spawn at 180); within one tick of 3 s.
- small-engine-1: Float drift: a piece whose bottom is exactly at the edge (y 0.5000000000000349 at tick 226) is counted as missed one tick later than the bottom is reached.
- small-engine-1: Restart is accepted at any time, not only after the end (README states this; the brief only names restart after the end).
- small-engine-1: The sample's title, pause, won and lost screens are left in assets/ui unused (README says so).
- small-engine-2: The headless run does not accept the brief's script word `restart` (0:left,600:restart left the run over at tick 700); only `enter` restarts. README says this openly.
- small-engine-2: A piece at the top of the field is partly drawn under the score/misses HUD panel in the frame (the HUD covers the top-left of the field).
- small-plain-1: Float drift: a piece sits with its bottom at y 0 (state y 0.5) for one tick (e.g. seed 1 tick 226), and the miss is counted on the next tick.
- small-plain-1: If two pieces reached the bottom in the same step at the third miss, the loop would keep counting misses past 3. I did not see this happen, because spawns are 72 ticks apart.
- small-plain-1: README.md line 24 has two bullets run together on one line (the 'centre x = column + 0.5.' bullet and the 'Order inside a step' bullet).
- small-plain-1: The game-over PNG says 'SCORE: N' rather than 'Final score', unlike the window version. The final score is still shown.
- small-plain-1: I did not open the tkinter window (machine rules), so the interactive play path is unverified by me and by the builder.
- small-plain-2: When the run ends, all remaining pieces are deleted at once, so the Game over frame shows no pieces that were still falling.
- small-plain-2: A piece whose centre is at y=0.5 (bottom edge on y=0, tick 226 seed 1) is not counted as a miss until the next tick because of float rounding. That is one tick late.
- small-plain-2: In the Game over frame the restart hint reads 'ENTER RESTART' (blocky font, no 'press'), and the score line reads 'SCORE' and not 'final score'. Only the tkinter window has the full wording.

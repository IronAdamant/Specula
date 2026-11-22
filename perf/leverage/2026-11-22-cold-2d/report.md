# Measured leverage: the runs

Folder: `lv2f`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 74007d71bc
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the large runs: 74007d71bc
- large: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the medium runs: 74007d71bc
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the small runs: 74007d71bc
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 116,644 | 0.25 | 30,981 | 321,644 | 9,152 | 13 | 14 | 2 | done | 9 | 1,439 / 3,367 | 8 / 9 | 1 / 1 | 4 | 1,080 |
| change-engine-2 | 125,348 | 0.27 | 33,807 | 357,415 | 9,471 | 14 | 14 | 4 | done | 13 | 3,116 / 9,444 | 9 / 9 | 1 / 1 | 3 | 1,248 |
| change-engine-3 | 150,840 | 0.30 | 35,092 | 492,695 | 11,543 | 18 | 19 | 5 | done | 27 | 6,691 / 6,300 | 9 / 9 | 1 / 1 | 2 | 1,128 |
| change-engine-4 | 117,705 | 0.26 | 33,874 | 310,844 | 8,857 | 13 | 14 | 2 | done | 7 | 847 / 5,172 | 9 / 9 | 1 / 1 | 3 | 1,284 |
| change-engine-5 | 132,643 | 0.27 | 32,443 | 438,728 | 9,645 | 17 | 18 | 5 | done | 19 | 35,198 / 19,720 | 8 / 9 | 1 / 1 | 3 | 1,255 |
| change-plain-1 | 114,135 | 0.24 | 27,611 | 253,570 | 10,854 | 12 | 20 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 736 |
| change-plain-2 | 91,599 | 0.20 | 25,227 | 185,148 | 8,311 | 10 | 17 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 673 |
| change-plain-3 | 114,273 | 0.25 | 29,831 | 266,054 | 10,077 | 12 | 21 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 861 |
| change-plain-4 | 77,535 | 0.18 | 21,564 | 123,642 | 7,644 | 8 | 9 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 594 |
| change-plain-5 | 101,003 | 0.22 | 27,389 | 197,367 | 9,407 | 10 | 17 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 820 |
| large-engine-1 | 1,557,278 | 2.49 | 170,845 | 8,780,654 | 93,140 | 87 | 92 | 16 | done | 41 | 7,410 / 26,833 | 28 / 31 | 2 / 2 | 5 | 1,548 (850 at the start) |
| large-engine-2 | 2,097,197 | 3.23 | 220,438 | 12,974,929 | 104,841 | 97 | 105 | 19 | done | 41 | 11,348 / 25,984 | 28 / 31 | 2 / 2 | 6 | 4,467 (850 at the start) |
| large-engine-3 | 1,128,330 | 1.93 | 145,005 | 5,461,888 | 80,183 | 61 | 70 | 18 | done | 28 | 18,082 / 19,857 | 29 / 31 | 2 / 2 | 2 | 1,301 (850 at the start) |
| large-plain-1 | 290,941 | 0.61 | 46,787 | 442,037 | 37,652 | 13 | 17 | 4 | done | - | - | 31 / 31 | 2 / 2 | 3 | 1,099 |
| large-plain-2 | 210,352 | 0.45 | 35,210 | 267,648 | 27,916 | 10 | 12 | 3 | done | - | - | 31 / 31 | 2 / 2 | 3 | 890 |
| large-plain-3 | 306,759 | 0.62 | 46,873 | 599,920 | 37,637 | 18 | 25 | 4 | done | - | - | 31 / 31 | 2 / 2 | 2 | 1,395 |
| medium-engine-1 | 176,386 | 0.37 | 40,461 | 450,622 | 16,151 | 15 | 25 | 4 | done | 12 | 1,207 / 5,482 | 19 / 19 | 2 / 2 | 5 | 999 (850 at the start) |
| medium-engine-2 | 189,846 | 0.38 | 40,273 | 553,984 | 16,823 | 18 | 18 | 6 | done | 19 | 24,153 / 20,169 | 18 / 19 | 2 / 2 | 4 | 1,130 (850 at the start) |
| medium-engine-3 | 277,822 | 0.52 | 51,277 | 1,104,145 | 20,665 | 28 | 37 | 14 | done | 18 | 12,970 / 28,045 | 19 / 19 | 2 / 2 | 5 | 1,041 (850 at the start) |
| medium-engine-4 | 202,551 | 0.41 | 41,631 | 603,971 | 18,025 | 19 | 23 | 5 | done | 18 | 5,387 / 9,637 | 19 / 19 | 2 / 2 | 2 | 1,139 (850 at the start) |
| medium-engine-5 | 237,429 | 0.47 | 46,453 | 793,842 | 19,998 | 22 | 28 | 7 | done | 17 | 5,980 / 12,488 | 19 / 19 | 2 / 2 | 4 | 1,168 (850 at the start) |
| medium-plain-1 | 117,355 | 0.26 | 21,288 | 108,638 | 15,977 | 7 | 9 | 2 | done | - | - | 18 / 19 | 2 / 2 | 4 | 584 |
| medium-plain-2 | 144,041 | 0.31 | 24,766 | 198,489 | 18,648 | 10 | 13 | 2 | done | - | - | 19 / 19 | 2 / 2 | 3 | 607 |
| medium-plain-3 | 140,767 | 0.31 | 25,320 | 121,008 | 19,404 | 7 | 10 | 2 | done | - | - | 19 / 19 | 2 / 2 | 5 | 704 |
| medium-plain-4 | 117,801 | 0.25 | 21,468 | 155,855 | 15,077 | 9 | 11 | 2 | done | - | - | 19 / 19 | 2 / 2 | 5 | 520 |
| medium-plain-5 | 135,205 | 0.29 | 23,840 | 132,793 | 18,426 | 8 | 11 | 2 | done | - | - | 19 / 19 | 2 / 2 | 4 | 683 |
| small-engine-1 | 111,617 | 0.25 | 29,062 | 193,993 | 11,179 | 9 | 15 | 2 | done | 6 | 305 / 2,793 | 14 / 14 | 2 / 2 | 3 | 876 (850 at the start) |
| small-engine-2 | 101,814 | 0.22 | 26,920 | 206,091 | 9,512 | 10 | 16 | 2 | done | 4 | 278 / 1,730 | 14 / 14 | 2 / 2 | 2 | 881 (850 at the start) |
| small-engine-3 | 116,902 | 0.25 | 29,612 | 281,134 | 10,356 | 12 | 18 | 2 | done | 8 | 441 / 3,014 | 14 / 14 | 2 / 2 | 3 | 901 (850 at the start) |
| small-engine-4 | 110,173 | 0.24 | 27,907 | 247,249 | 10,114 | 11 | 19 | 3 | done | 11 | 1,264 / 3,109 | 13 / 14 | 2 / 2 | 1 | 914 (850 at the start) |
| small-engine-5 | 109,135 | 0.23 | 27,451 | 297,023 | 9,025 | 13 | 15 | 2 | done | 6 | 491 / 2,742 | 14 / 14 | 2 / 2 | 2 | 954 (850 at the start) |
| small-plain-1 | 68,206 | 0.15 | 13,897 | 70,329 | 8,761 | 6 | 10 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 314 |
| small-plain-2 | 62,958 | 0.14 | 12,402 | 69,834 | 8,095 | 6 | 6 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 292 |
| small-plain-3 | 66,688 | 0.15 | 12,886 | 70,780 | 8,701 | 6 | 6 | 1 | done | - | - | 14 / 14 | 2 / 2 | 5 | 306 |
| small-plain-4 | 63,993 | 0.14 | 12,318 | 68,981 | 8,340 | 6 | 6 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 280 |
| small-plain-5 | 61,552 | 0.13 | 12,208 | 69,001 | 7,879 | 6 | 6 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 291 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 5 of 5 | 128,636 | 116,644 to 150,840 | 3 | 8.6 / 9 | 14,958 |
| change | plain | 5 of 5 | 99,709 | 77,535 to 114,273 | 1 | 9.0 / 9 | 11,079 |
| large | engine | 3 of 3 | 1,594,268 | 1,128,330 to 2,097,197 | 17 | 28.3 / 31 | 56,268 |
| large | plain | 3 of 3 | 269,351 | 210,352 to 306,759 | 4 | 31.0 / 31 | 8,689 |
| medium | engine | 5 of 5 | 216,807 | 176,386 to 277,822 | 7 | 18.8 / 19 | 11,532 |
| medium | plain | 5 of 5 | 131,034 | 117,355 to 144,041 | 2 | 18.8 / 19 | 6,970 |
| small | engine | 5 of 5 | 109,928 | 101,814 to 116,902 | 2 | 13.8 / 14 | 7,966 |
| small | plain | 5 of 5 | 64,679 | 61,552 to 68,206 | 1 | 14.0 / 14 | 4,620 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change | engine | 128,636 | 99,709 | 1.29 | 1.44 | 8.6 / 9 | 9.0 / 9 |
| large | engine | 1,594,268 | 269,351 | 5.92 | 5.98 | 28.3 / 31 | 31.0 / 31 |
| medium | engine | 216,807 | 131,034 | 1.65 | 2.49 | 18.8 / 19 | 18.8 / 19 |
| small | engine | 109,928 | 64,679 | 1.70 | 1.83 | 13.8 / 14 | 14.0 / 14 |

Judging: 36 judging(s) of 36 run(s), 10,150,442 weighted tokens in all.

At the runner's list prices: 17.22 USD for the builds, 37.72 USD for the judging shown above (54.94 USD in all).

## Items that failed

- change-engine-1 **c01**: play --set gate=100 --seed 1, --at 8400..8700 step 10: wave 6 sends runners (kind runner, health 1 at Normal, x falls 0.5 per 10 ticks = 3 u/s), and the gate drops 55 -> 40 over wave 6 (15 runners). But they come one every 60 ticks (1 s): first runner at x 19.55 at tick 8510, second at 19.55 at tick 8570 (lib.rs RUNNER_GAP = 60). The item asks for one every 1.5 s.
- change-engine-5 **c05**: Seen: Easy walkers 1 and runners 1 (wave 6 reached on Easy), Normal walkers 2 and runners 1, Hard walkers 3. Not seen: Hard runners 2. No Hard run reached wave 6 (the winning script loses in wave 3 on Hard, and the engine's find gave up after 300 s). The code (lib.rs: base+1 on hard) says 2, but that is not something I saw the game do.
- large-engine-1 **l22**: Escape freezes play (enemy x frozen 50 ticks), shows Paused/Resume/Title, and Resume continues from the same x. But the reported screen stays 'play', never 'paused'. After Title the texts are a title menu while screen stays 'play' with the old enemies listed; Enter there starts a new run at once.
- large-engine-1 **l23**: saved became true when the shop opened. But a later run with the same --settings folder (sP) showed saved false and no Continue: the saved run is not kept between processes (README admits this).
- large-engine-1 **l26**: Won run: best 1540 on the won screen and the title ('Best: 1,540') in the same run. A later run with the same settings folder (sW) shows Best: 0: the best score is not kept between processes.
- large-engine-2 **l22**: Escape pauses (enemies and bolts frozen from tick 51 to 200) and Title returns to the title, but Enter on the focused Resume button (p_resume) does nothing: still paused 150 ticks later. Only a second Escape resumes. The button's action 'resume' is never received by the rules.
- large-engine-2 **l23**: saved becomes true when the shop opens, but a second headless run with the same --settings folder starts with saved false and no Continue; README admits the saved run is kept for the session only.
- large-engine-2 **l26**: A lost run ending at 530 shows 'Best: 530' on the lost screen, but if the headless run ends there, the next run's title shows 'Best: 0': best is only kept once Enter is pressed on the end screen. It works with Enter: 1540 was kept and shown later, and a later 530 did not replace it. Also 'best' reads 0 at tick 0 of a run with a kept best.
- large-engine-3 **l23**: saved becomes true when the shop opens, but a second play run on the same --settings folder shows saved false and title ['...','Best: 0','Start','Settings','Quit'] with no Continue. README admits the saved run is not kept between runs.
- large-engine-3 **l26**: In one run the won run set Best 1,620 on title/won screen and a later lost run with 0 left it 1,620 (shown on lost screen); but a later headless run on the same settings folder shows best 0 / 'Best: 0': the best score is not kept.
- medium-engine-2 **m10**: Seed 1, guard at y 4, one bolt at tick 30 hit the lane-4 walker (health 2->1, bolt gone). A second bolt fired at tick 60 in the same lane passed through it: tick 124 bolt x 16.933 / walker x 16.95, tick 125 bolt x 17.167 / walker x 16.925, walker still health 1, bolt flew on to x 19.5 and left. The swept check compares the walker's new x with the bolt's old x, so a crossing is missed (about 1 in 10 bolts). Kills otherwise give +10 (score 10 at tick 130 with continuous fire); a bolt in lane 6 passes a lane-4 walker.
- medium-plain-1 **m16**: Mid-field frame (seed 1, shoot 100 ticks) matches state exactly: guard y 6 at px 240, walkers (17.5,4) and (19.75,10), four bolts, gate bar, Score/Wave/Gate HUD. But with guard_y 12.0 (hold Up 1 s, the top edge) play.png shows no guard at all: the HUD bar is drawn over it; at y 0 it is half off the bottom.
- small-engine-4 **s14**: README explains play and gives both commands, but its headless command run as written from work/ (--ticks 600 ... 610:restart) prints {"error":"usage","message":"'--inputs': the input 'restart' is at tick 610, after the run's last tick (600)..."}. The frame command works.

## What each judge found beyond the list

- change-engine-1: Wave 6 runners come every 60 ticks (1 s), not every 1.5 s (lib.rs RUNNER_GAP).
- change-engine-1: The README 'How to play' still says 'A walker has 2 health, 10 points', with nothing on difficulty or on runners' 15 points and 1 health.
- change-engine-1: The Language and Difficulty settings stop at the ends instead of wrapping round (Right on French stays French).
- change-engine-1: The texts list on the settings screen still holds the title screen's strings, and on won/lost it holds the HUD's (already present before the change).
- change-engine-2: The bolt/walker swept collision is still the pre-change one (compares the walker's new x with the bolt's old x), so a crossing can be missed; with runners at 3 units/s the missed window is larger (about 0.05 of 0.283 per tick).
- change-engine-2: README still says notes/winning_inputs.txt ends on the Won screen with score 440; replayed now it ends in play at wave 5 of 6, score 440 (CHANGES.md admits it, README not updated). notes/frame_won.png and frame_play.png show the old 5-wave HUD.
- change-engine-2: README's 'Not checked / limits' still says Spanish was not checked on won/lost; minor stale text. The m10 failure (missed second hit) is not fixed, as allowed.
- change-engine-3: Language and Difficulty do not wrap: Right on French stays French, Left on Easy stays Easy (the order is right; it just does not loop back).
- change-engine-3: README still opens by calling it 'five waves of walkers (5, 7, 9, 11, 13)' and says a walker has 2 health and each kill is 10 points; the wave-6 and difficulty lines are added further down, so the intro is out of date.
- change-engine-4: Outside a run (title and settings screens), the reported 'difficulty' stays 'normal' even when the Difficulty setting is Hard or Easy. The rules only learn it when a run starts, so the state disagrees with the screen until Start.
- change-engine-4: CHANGES.md has the runner colours the wrong way round: runners with 1 health are drawn purple and those with 2 health (Hard) yellow, not 'yellow (purple when they have 2 health)'.
- change-engine-4: CHANGES.md says no frame image was taken and Easy was not tested by a run; the builder did not check these, though they work.
- change-engine-5: README 'How to play' still says 'Walkers have 2 health' and the Settings section says only volume and language are kept; it does not mention difficulty.
- change-engine-5: CHANGES.md says Easy/Hard runs were only checked at the first spawns, and Hard was never played to wave 6.
- change-engine-5: The Language setting does not wrap: Right on Français and Left on English do nothing. It is still the English-Spanish-French order.
- change-plain-1: French lost-screen title 'LA PORTE EST TOMBÉE !' is wider than 800 px at scale 7: the leading L and trailing ! are cut off in the frame (lost_fr.png).
- change-plain-1: Pre-existing (m16) still present: with guard_y 12 the guard is hidden under the HUD bar in the frame.
- change-plain-1: The game opens the interactive tkinter window when given --ticks 0 and no inputs, --script, --png or --headless (because ticks 0 is falsy), so a headless call with zero ticks is not headless.
- change-plain-2: README.md was not updated: it still says five waves of walkers, lists only en.json/es.json, and does not mention Difficulty in Settings (CHANGES.md says this was left on purpose).
- change-plain-2: Bolts left over from a finished run stay in the state after Enter returns to the title (not drawn, cosmetic).
- change-plain-2: The next wave starts 179 ticks after the clearing tick, not 180 (this was so before the change).
- change-plain-3: Difficulty and volume clamp at the ends while Language wraps around; consistent with the brief but differs between rows of the same menu (not a failure).
- change-plain-3: Window (tkinter) path and sound remain untested by builder and judge.
- change-plain-4: The cedilla in 'Français' (language name on the French settings screen) is drawn as C with an accent mark above it; CHANGES.md wrongly says no French string uses a cedilla.
- change-plain-4: README.md is partly stale after the change: says language codes are (en, es) and shows the saved settings JSON without difficulty.
- change-plain-4: Spanish 'Enter: título' keeps the English word Enter (pre-existing, not part of this change).
- change-plain-5: A bolt can pass through a walker in its lane without hitting it, because walkers move before the bolt sweep [b.x, b.x+14/60] is tested. I fired one bolt at the lane-4 walker at 90 different ticks and 9 passed through. Runners move faster (0.05 per tick), so the gap is wider for them. This is probably not new to the change: the collision code is not named in CHANGES.md.
- change-plain-5: README 'What I checked' still says the bot wins with score 450 (45 walkers x 10), but a full clear is now 675.
- change-plain-5: Volume changes nothing audible (the README says so).
- large-engine-1: The best score and the saved run are not kept between game processes at all, as README admits; only volume, language and difficulty are.
- large-engine-1: The report's screen never becomes 'paused'. After the pause's Title it stays 'play' with stale enemies while the engine's own title is shown.
- large-engine-1: Escape in the shop opens the pause menu (reported screen shop, texts Paused/Resume/Title); the brief says Escape does nothing there.
- large-engine-1: Buying Heavy bolts moves the shop's menu focus back to Quick hands, so a following Down/Enter lands on the wrong entry.
- large-engine-1: Without a saved run, Start on the title shown after pause-Title begins a run at once, without going through the game's own title state.
- large-engine-2: The pause menu's Resume button does nothing when chosen with Enter; only Escape resumes.
- large-engine-2: The saved run is not kept between launches (session only), against the brief's 'kept'.
- large-engine-2: The best score is written only when Enter is pressed on the won/lost screen; quitting there loses it. It is capped at 1700 and only multiples of 5 are kept (340 hidden buttons).
- large-engine-2: The JSON 'best' reads 0 at tick 0 of a run whose kept best is higher (the title text is right).
- large-engine-2: The shooting cooldown carries over across the shop, so the first shot of a new wave can wait up to 24 ticks.
- large-engine-2: Primary-styled buttons (Start, Next wave) are always filled blue, which can look selected next to the focused button's outline.
- large-engine-3: The best score and the saved run live only in the process: a new headless run (or a restarted game) always starts with best 0 and no saved run, against the brief's 'kept' requirement.
- large-engine-3: On the title screen the report still carries the last run's score, coins, wave and gate_health (e.g. score 1620, gate_health 14 on the title), not cleared values.
- large-plain-1: Spanish 'Enter: título' keeps the English key name 'Enter' (Spanish keyboards say 'Intro'). It is a common loanword, so l05 was not failed for it.
- large-plain-1: The window mode (tkinter) was never run by the builder, and the judge did not run it either (no windows allowed). It draws text with a different font from the PNG frame.
- large-plain-1: Volume is stored and shown, but the game has no sound.
- large-plain-2: Spanish won text 'Victoria!' lacks the opening '¡'; Spanish 'Enter: título' keeps the English key name.
- large-plain-2: Settings Left/Right wrap around for language and difficulty (Hard->Easy) rather than stopping; allowed by the brief but not stated.
- large-plain-2: The window mode (tkinter) was not exercised by the judge or, per README, by the builder.
- large-plain-3: Spanish lost/won screens say 'Enter: titulo': the key name stays English (Spanish keyboards label it Intro); French uses 'Entree'.
- large-plain-3: An enemy reaching x = 0 stays one tick at x 0.0 before the gate takes its damage (harmless).
- medium-engine-1: texts on the settings screen also lists the title screen's four strings (the title is drawn faintly beneath), and won/lost/paused texts include the HUD strings: texts is not only what the current screen shows.
- medium-engine-1: The guard is clamped with its centre at y = 0 and 12, so at the field edges half the guard (and bolts fired there) is drawn off the frame; the HUD panel also covers the top-left of the field near the guard's column.
- medium-engine-1: On the title screen the Start button is always drawn filled blue (primary style), which can look like a selection marker next to the real focus outline.
- medium-engine-1: Volume has no audible effect (the game has no sounds), as the README admits.
- medium-engine-1: The guard reaches y = 0 and 12 although no lane is there; the brief's 'inside the field' is met only by the centre.
- medium-engine-2: On the first tick after Start the HUD texts are raw placeholders: ['Score: {game.score}', 'Wave {work.wave} of 5', 'Gate: {work.gate}'] with score/wave null (tick 1 of a 1:enter run).
- medium-engine-2: Bolt-walker collision misses crossings (walker new x vs bolt old x), so a bolt can pass a walker in its own lane; this is why the builder's own winning script ends at 440, not 450.
- medium-engine-2: Guard is allowed to y 0 and y 12, i.e. half drawn outside the field at the edges.
- medium-engine-2: After returning to the title, the JSON still reports the last run's score/wave/walkers (README admits it).
- medium-engine-3: The HUD panel lies over the top-left of the field. In f/play.png the state had a bolt at (3.87, 12) that cannot be seen in the frame.
- medium-engine-3: The guard can go to y = 0 and y = 12, the field's border lines. There it is drawn half outside the field, and bolts fired there run along the edge.
- medium-engine-3: On the title screen the Start button is always filled blue (style primary), so the moved focus is marked only by a ring. That can be read as Start still being chosen.
- medium-engine-3: The README has the heading `What I checked` twice. It also says no full five-wave win was shown, though one is possible (I reached it: 450 points).
- medium-engine-3: The rules read a test option `--set waves=N` that the brief does not ask for (the default is 5).
- medium-engine-4: texts holds strings from screens under the current one: under Settings it lists the title screen's 4 words (the panel hides them in the frame), and under Pause, Won and Lost it lists the HUD's words. The README says so.
- medium-engine-4: The game has a test-only option --set gate=N that changes the gate's starting health (the README says so).
- medium-engine-5: After leaving play for the title (from pause), the state still holds the old walkers and guard_y, so the title-screen JSON reports walkers that are not in play.
- medium-engine-5: The settings frame shows a faint copy of the title's name panel behind the settings panel, so the title screen shows through.
- medium-engine-5: The Start button is always drawn in blue (primary style), so on the title the marked entry is shown only by an outline and is less clear at a glance.
- medium-engine-5: The README's commands contain placeholders (SETTINGS_FOLDER, C:/some/folder) that must be replaced before they run.
- medium-plain-1: With --ticks 0 and no --inputs/--script/--png/--headless, the program opens the interactive tkinter window instead of a headless run (a.ticks is 0, so it is falsy); I stopped the window I opened by mistake this way.
- medium-plain-1: Guard at the top edge (y 12) is fully hidden under the HUD bar, and at y 0 half off the frame.
- medium-plain-1: Settings screen shows a hard-coded '< >' hint; '<' is not in the font, so it is drawn as '? >'.
- medium-plain-1: Paused-then-resumed play drops a held input (Resume sets held to none), so holding a key across a pause needs the key again.
- medium-plain-2: A walker that reaches the gate is reported at x 0.0 for one tick (float error) before the gate loses health a tick later.
- medium-plain-2: Spanish 'Enter: título' keeps the key name 'Enter' in English (arguably a key label).
- medium-plain-2: Default settings file is settings.json inside the game folder, beside game.py.
- medium-plain-3: A walker sits at x=0.0 for one tick before it counts against the gate (it is removed only when x <= 0 after rounding error), so the gate damage lands one tick late.
- medium-plain-3: Spanish keeps the English key name in 'Enter: título' (arguably a key label, not a word).
- medium-plain-3: Lost and won states still list the remaining walkers in the walkers array.
- medium-plain-3: Without --settings, settings.json is written into the game folder itself.
- medium-plain-3: The paused overlay covers bolts and walkers behind it in the frame.
- medium-plain-4: The state's wave becomes 2 at exactly 180 ticks after the last walker, but the first walker of the new wave appears one tick later (181 ticks); negligible.
- medium-plain-4: Spanish 'Enter: titulo' keeps the key name 'Enter' in English (arguably a key label, not a word).
- medium-plain-4: Accented characters are drawn as base letter plus a small mark; acute and tilde look the same.
- medium-plain-4: README example settings path /tmp/s.json resolves to C:\tmp\s.json on Windows, a shared location that may already hold other settings (it held volume 6 here), so the README example does not start at defaults.
- medium-plain-4: Quit on the title does nothing visible in headless mode (documented).
- medium-plain-5: The Spanish won/lost hint is 'Enter: título', so the English key word 'Enter' stays on Spanish screens.
- medium-plain-5: Volume is stored and shown but there is no sound at all, as the README admits.
- medium-plain-5: Without --settings, the headless run writes to %APPDATA%\HoldTheGate\settings.json. The README example uses C:/tmp/s.json, which writes outside the game folder.
- medium-plain-5: The tkinter window was not checked by the judge. The builder says it did not open it either.
- small-engine-1: Sample title, pause, won and settings screens and their strings are left in the game, unused (README admits it).
- small-engine-1: Miss detection is subject to float rounding: piece expected to leave at tick 226 still listed at y 0.5000000000000349 at tick 226 (one tick late); harmless.
- small-engine-1: Enter also restarts a run still in play, which the brief does not ask for (README states it).
- small-engine-2: README's play line is garbled ('add `--seed N` is only for headless runs'), and it gives a bare `specula play . --as-is` window command that a checker cannot run through the documented leverage wrapper; I did not try it, because it would open a window.
- small-engine-2: A piece that has passed the paddle's top (bottom between 0 and 0.5) can still be caught if the paddle slides into it from the side. This is arguably 'touching', but it is lenient.
- small-engine-3: A restart input during a live run is ignored (restart at tick 400 of the README's own example run does nothing). The brief only asks for restart after the end, but the README's example command suggests otherwise.
- small-engine-3: The README's frame command writes frame.png into the game folder relative to the cwd. This works, but it overwrites the shipped example frame.
- small-engine-3: A piece whose bottom is exactly at the paddle's side edge (x+0.5 == paddle_x) counts as caught ('edges touching count'). This is generous but within 'touches'.
- small-engine-4: README headless-run command fails as written: its script puts a restart at tick 610 in a 600-tick run, which the engine rejects.
- small-engine-5: The HUD keeps the sample's Pause button and the Game over panel a 'Title' button that goes to the sample's unused title screen (README admits the title screen is unused).
- small-engine-5: At tick 0 (before any tick runs) the report prints null for score, misses, pieces and the other game keys, not the starting values.
- small-plain-1: The restart input uses up a whole tick doing nothing but the reset. The new run's first piece appears one tick after the restart tick, not at it.
- small-plain-1: Pieces near the top are drawn under the score/misses text. On the Game over frame, pieces behind the panel cannot be seen.
- small-plain-1: The Game over frame says 'SCORE n', not 'final score' (the interactive window says 'Final score'). This is cosmetic only.
- small-plain-1: The interactive play mode (tkinter window) was not exercised. The machine rules forbid opening a window that takes the keyboard.
- small-plain-2: In the PNG frame, a piece near the top edge is drawn under the HUD text (SCORE line overlaps the piece at y 15.1 in mid.png).
- small-plain-2: The PNG prints GAME OVER in upper case only, not "Game over" as written in the brief (the tkinter window uses "Game over").
- small-plain-2: The interactive tkinter window was not run by its builder and was not run by me (no windows allowed), so keyboard play is untested.
- small-plain-3: A miss counts one tick late: at tick 226 the piece centre is at y=0.5 (bottom edge at 0), but floating-point error keeps bottom just above 0, so the miss comes at tick 227.
- small-plain-3: The restart input uses up its tick without stepping, so a restarted run is one tick behind a fresh run (restart+N matches fresh tick N-1).
- small-plain-3: In the step where the third miss sets over, the remaining pieces are still checked, so a catch in that same step could still add points after the end.
- small-plain-3: The pieces are drawn under the HUD text and the Game-over box, so a piece near the top or in the middle can be hidden (seen in over.png).
- small-plain-3: The README says the tkinter window was never opened, so live keyboard play is untested by the builder. The judge did not open it either, per the machine rules.
- small-plain-4: On the third miss every piece still falling is cleared from the state and the frame (pieces: []), so the game-over screen shows no pieces. The brief does not ask for this.
- small-plain-4: The game-over text is drawn in uppercase ('GAME OVER') in the PNG frame. The window path uses 'Game over'.
- small-plain-4: In the headless script, restart is also accepted while a run is still in play, not only after the end. This is harmless and is documented.
- small-plain-5: A tick whose input is restart only resets the run. The fresh run's first piece appears on the next tick, not on the restart tick.
- small-plain-5: At game over the game clears all pieces still falling, so the end frame shows none (the brief does not ask for this).
- small-plain-5: The README's frame command writes out.png into the current folder; I ran it elsewhere so nothing was written into work/.
- small-plain-5: The window mode (tkinter) is untested: the builder did not open it and the judge rules forbid opening it.

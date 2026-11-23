# Measured leverage: the runs

Folder: `lev-props`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the kit runs: 4deccc36
- kit: builder claude-sonnet-5-5, cap 400,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the kit runs: 8b67796d

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| kit-found-1 | 509,455 | 0.87 | 87,163 | 2,823,386 | 23,637 | 44 | 52 | 8 | done (past the token cap) | 139 | 6,717 / 164,182 | 8 / 9 | 2 / 2 | 4 | 980 (689 at the start) |
| kit-found-2 | 663,346 | 1.11 | 99,963 | 3,652,779 | 34,628 | 52 | 58 | 8 | done (past the token cap) | 232 | 10,049 / 289,040 | 9 / 9 | 2 / 2 | 4 | 965 (689 at the start) |
| kit-none-1 | 249,331 | 0.49 | 60,517 | 991,158 | 14,916 | 23 | 30 | 3 | done | 1 | 0 / 0 | 0 / 9 | 0 / 2 | 4 | 810 (683 at the start) |
| kit-none-2 | 447,786 | 0.84 | 98,012 | 2,039,267 | 24,272 | 31 | 47 | 6 | done (past the token cap) | 50 | 1,955 / 98,723 | 9 / 9 | 2 / 2 | 4 | 976 (683 at the start) |
| kit-none-3 | 594,327 | 1.00 | 95,983 | 3,354,068 | 27,793 | 48 | 59 | 8 | done (past the token cap) | 158 | 8,069 / 207,869 | 9 / 9 | 2 / 2 | 4 | 1,312 (683 at the start) |
| kit-told-1 | 507,469 | 0.85 | 70,632 | 2,707,061 | 29,701 | 64 | 70 | 7 | done (past the token cap) | 130 | 3,740 / 239,107 | 9 / 9 | 2 / 2 | 6 | 965 (689 at the start) |
| kit-told-2 | 562,963 | 0.97 | 85,332 | 2,864,310 | 33,979 | 56 | 73 | 6 | done (past the token cap) | 173 | 6,530 / 232,697 | 9 / 9 | 2 / 2 | 4 | 914 (689 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| kit | found | 2 of 2 | 586,400 | 509,455 to 663,346 | 8 | 8.5 / 9 | 68,988 |
| kit | none | 3 of 3 | 430,481 | 249,331 to 594,327 | 6 | 6.0 / 9 | 71,747 |
| kit | told | 2 of 2 | 535,216 | 507,469 to 562,963 | 6 | 9.0 / 9 | 59,468 |

## Against the baseline arm, `none` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| kit | found | 586,401 | 430,481 | 1.36 | 1.41 | 8.5 / 9 | 6.0 / 9 |
| kit | told | 535,216 | 430,481 | 1.24 | 1.76 | 9.0 / 9 | 6.0 / 9 |

Judging: 7 judging(s) of 7 run(s), 677,887 weighted tokens in all.

At the runner's list prices: 6.12 USD for the builds, 2.85 USD for the judging shown above (8.97 USD in all).

## Items that failed

- kit-found-1 **k05**: The frame reads clearly as a heart, with two lobes on top and a point down. The edges are not rounded, though. heart.glb has a flat back cap at z=-0.025 (62 tris, nz=-1) and a flat front cap at z=0.025 (62 tris, nz=+1). The 18 vertex rings between them all sit within 0.1 mm of the outline (xmax 0.15000 at z=-0.025 vs 0.14990 at z=0.025), and 2048 of the 2300 triangles have nz=0. So the side is a vertical wall meeting both caps at a right angle: the 'pillow' raise used inset 0.0001, so it never domes.
- kit-none-1 **k01**: Ran README's `sh <runs>/kit-none-1/work/build.sh`: exit 1, no output. Its first step `examen leverage engine start` returns engine_exited (exit code 1); port 7680 is held by svchost PID 33736. No recipe was built. build.sh would also only print the engine's raw answer, not the {name, triangles, closed, bounds} JSON the brief asks for.
- kit-none-1 **k02**: README's frame command is a placeholder (`sight_frame '{...}'`, 'arguments are NOT settled') and needs the engine, which will not start. No PNG exists anywhere in work/.
- kit-none-1 **k03**: No frame or build could be made (see k01/k02), so the teeth and the hole were never seen.
- kit-none-1 **k04**: No build ran, so no bounds were reported (see k01).
- kit-none-1 **k05**: No heart frame exists (see k02).
- kit-none-1 **k06**: No star frame or bounds exist (see k01/k02).
- kit-none-1 **k07**: No coin frame or bounds exist (see k01/k02).
- kit-none-1 **k08**: The headless build never reported closedness for any recipe (see k01), and there are no frames to judge the shared style from.
- kit-none-1 **k09**: From a fresh start (no engine running), build.sh exits 1 with nothing printed, and the frame command is not a runnable command ('{...}').

## What each judge found beyond the list

- kit-found-1: Heart: the recipe's pillow raise uses inset 0.0001, so the 'rounded' rim is a straight vertical wall with square edges on front and back. README's 'gentle dome ... rim is soft' describes something the mesh does not have.
- kit-found-1: assets/style.json promises 'soft bevel on every rim' and 'a warm metal for the gear and coin', but no prop is bevelled and the gear and coin use `painted`, not `metal`.
- kit-found-1: Closedness is measured by the builder's own script on the exported .glb, not by the engine's model_build closed flag that the brief names (README says so).
- kit-found-1: Every frames run leaves another run folder of PNG/JSON files under work/captures, and these accumulate.
- kit-found-2: The README and style description say every token has a 'warm face colour', but the gear uses token_metal/steel (grey, #aeb8c4), which is not warm.
- kit-found-2: The coin's raised rim cannot be seen in its only frame (flat head-on lighting). The README admits this.
- kit-found-2: The star's points reach radius 0.1577, so its tip-circle diameter is 0.315. Only its x width is exactly 0.300; it is still about 0.3 across.
- kit-found-2: The build command writes .glb exports into assets/models/exports as a side effect of the watertight check.
- kit-none-1: README admits that nothing was ever built or framed by the engine; recipe op names, fields, orientation and tool arguments are guesses.
- kit-none-1: build.sh sends engine start's output to /dev/null, so when it fails it exits 1 silently with no reason shown.
- kit-none-1: build.sh does not reshape the engine's answers into the per-prop {name, triangles, closed, bounds} JSON the brief asks for.
- kit-none-1: No frame commands are given: the sight_frame arguments are left as '{...}'.
- kit-none-2: The coin's raised rim barely shows in its front frame (only a thin line); the relief is real in the geometry but the frame shows little depth.
- kit-none-2: The heart is 13,448 triangles, far more than the other props (36 to 576), because it is an sdf sculpt.
- kit-none-2: kit.py uses the work folder (<runs>/kit-none-2/work) as the engine project, not <runs>/kit-none-2 as the brief's engine commands do; it works, but it differs from the brief.
- kit-none-2: The build command takes about 1.5 minutes because it calls examen once per tool call.
- kit-none-3: The heart's bottom point is cut flat about 17 mm wide (outline points +-0.00841 at y = 0), not a true point. The README admits this.
- kit-none-3: The gear is a dull dark grey (metal/steel) in its frame. That does not fit the style's own 'one bold colour each' description.
- kit-none-3: Under 'Expected output', the README shows a text table, but the command actually prints JSON. It is a small documentation mismatch.
- kit-none-3: In the straight-on frame the coin's rim shows only as a faint ring, so the frame alone barely shows the raised rim.
- kit-told-1: The front frames cannot show the coin's rim or any prop's thickness. The coin's front frame is a flat brass disc, as the README admits.
- kit-told-1: The heart's bevel is only 3 segments, so the rounding is faceted. The heart outline is two arcs with straight sides, not a classic heart curve.
- kit-told-1: The gear teeth have straight flanks (not involute). That is fine for a token.
- kit-told-1: The star is 0.3 wide but 0.285 tall (tip to tip sideways).
- kit-told-1: The `closed` flag comes from the builder's own Python check on the exported glb, not from the closed flag the engine's model_build reports.
- kit-told-1: Python 3 is needed for both checks. It is not part of the engine.
- kit-told-2: The 'closed' flag is the kit's own edge test on the exported glTF, not the engine's report. The README admits it was never tried on a known-open mesh, and I did not test it against one either.
- kit-told-2: The heart's outline is hand-made (two circular lobes on a right-angled point) rather than the library heart. It reads fine but is a little stiff, as the README says. It is 0.274 tall for 0.3 wide.
- kit-told-2: The coin's front frame shows the rim only faintly (a thin ring). The raised rim is clear only from an angle, and the README's frames give no angled view.
- kit-told-2: The frames command leaves engine capture folders under work/captures each time it runs.

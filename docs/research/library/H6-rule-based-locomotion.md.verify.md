# H6 verification: rule-based locomotion

Verifier: Library Verify (adversarial re-check). Written 2026-11-16; finalised 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `H6-rule-based-locomotion.md`. H6 itself was not edited.

## Final status (2026-11-17, completes the 2026-11-16 partial pass)

**This section replaces the 2026-11-16 Summary and the "Section 2 parameter blocks" statuses below wherever they differ.** Times are AEDT (UTC+11).

### Version verified against
- Snapshot at the start of today's work: mtime **2026-11-16 10:25:38 AEDT** (unchanged).
- Motion Data edited H6 at **2026-11-17 08:56:26 AEDT** (42,849 bytes). The Changelog says "No values or licence calls change". My diff and number-diff confirm that **only access dates and the Changelog were added**, so no rows were re-checked because of that edit.
- Re-stat before writing: **2026-11-17 08:56:26 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

### Counts
- **§1 survey methods (13 rows):** 11 VERIFIED, 0 FIX, 2 UNVERIFIABLE. M9 Mordatch and M11 Ijspeert are still at abstract or metadata level. I could not open the Wayback copies of Mordatch's author PDF or Ijspeert's preprint from the box. "Drive level" in M11 is a paraphrase; a secondary quote of the review says "level of stimulation" of the MLR.
- **§2 parameter rows, audited cell by cell today (blocks 2a–2j, 52 rows as numbered in the table below):** **49 VERIFIED, 2 FIX, 1 UNVERIFIABLE.**
- **Total:** 65 rows, of which **60 VERIFIED, 2 FIX and 3 UNVERIFIABLE**.

### Worst problems
1. **FIX M2/§2b PD-gain scaling (Coros, Beaudoin & van de Panne 2010).** H6 says "kp scaled by √(M′/M)". The PDF page rendered as an image (p. 5) reads *"we scale the kp values by M′/M"*, which is **linear**. The square root applies only to kd: *"kd = 2·√kp"*.
   - **Correction:** "kp scaled by M′/M (linear); kd = 2√kp."
   - H6's note that "the square-root signs only appear in the raw text layer" is true only for the kd formula.
2. **FIX §2i Walknet rule 4.** H6 says it is "not described in the parts of the review I extracted". The review does describe it: *"Rule 4 is represented by a specific version of the Target-net (Dean 1990) that transmits the current position of the anterior leg which is used as the swing target by the receiving leg. In other words, rule 4 influences the AEP."*
   - **Correction:** "Rule 4 sends the anterior leg's current position to the leg behind as its swing target (sets the AEP); front-to-back, same side." H6's "Not found" bullet on rule 4 should go.
   - H6's "information from joint angles" is roughly right.
3. **§2f Overgrowth "13 keyframes": conflicting sources, not a FIX.**
   - The third-party transcript H6 cites has Rosen saying "even with only 13 keyframes".
   - Grok A's YouTube auto-caption excerpt counts "six locomotion keyframes and our two standing keyframes" (8). It found "13" only in community write-ups (Unity forum, Reddit).
   - The 6 + 2 breakdown agrees with H6. 13 is plausible if the ledge, fall and other keys are added. I did not watch the video myself, so the row stays as H6 has it (third-party transcript), with this conflict recorded.

**Dead links:** none among the H6 URLs re-checked. The Raibert & Hodgins ACM page returns 403, which H6 already notes.

### §2 row table (2026-11-17)

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 2a-1…12 | SIMBICON Table 1 (12 rows: walk/fast walk/etc. state times, swing-hip/knee/ankle targets, cd/cv) | VERIFIED | Every cell matches the PDF Table 1 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf |
| 2a-13 | Planar kp 300 / kd 30; ground 100000 N/m and 6000 Ns/m; friction 0.65; 0.0001 s; torque limits 1000/370/90 Nm | VERIFIED | PDF §5 | same |
| 2a-14 | 3D: ODE, 0.6, 0.005 s, friction 0.8, waist kp 1000, kd = 0.1kp | VERIFIED | PDF | same |
| 2a-15 | Robustness: 20 cm steps, ±6° slopes, 10 pushes at 5 s intervals, 350 N for 0.2 s; FEL 0.2 rad, 2.5/3.5/14 % | VERIFIED | PDF and project page | same ; https://www.cs.ubc.ca/~van/papers/Simbicon.htm |
| 2b-1 | Coros 2010 α=0.05; step-distance formula d; 0.6L; T 0.2–1.0 s; speeds −0.6 to 1.7 m/s | VERIFIED | PDF | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf |
| 2b-2 | **kp scaled by √(M′/M)**; kd = 2√kp | **FIX** | Linear M′/M (page image); kd = 2√kp is right | same (p. 5 rendered) |
| 2b-3 | ODE 0.0005 s, friction 0.8, torque limits 200/100 Nm, 37 DOF, 40 % | VERIFIED | PDF | same |
| 2c-1…6 | Coros 2011 quadruped (dog 34 kg; trot 3.0 vs 3.3 m/s, gallop 5.7 vs ~6.5 m/s; Table 1 pushes) | VERIFIED | PDF | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf |
| 2d-1…3 | Wampler & Popović 2009: σmax 50 N/cm²; biped 0.7 / 3.0 m/s | VERIFIED | PDF | https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf |
| 2e-1…4 | Hecker 2008 Spore: 10 %/120 %, 0.2 ms, 25 bodies, 1.7 GHz Pentium-M, 35 %, several hundred creatures / thousand animations, ~90 % | VERIFIED | PDF | https://www.cse.chalmers.se/edu/year/2011/course/TDA361/Advanced%20Computer%20Graphics/027-hecker%20copy.pdf |
| 2f-1 | 13 keyframes in total | UNVERIFIABLE | Third-party transcript only; conflict above | https://reducer.xgoose.org/items/8812 ; https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach |
| 2f-2…6 | 2 keys per gait (6 locomotion keys) + 2 standing keys; ledge 1 key + IK; fall 3 flail keys; reload 2–3 keys; half step time → quarter fall | VERIFIED | Transcript; the 6 + 2 count also matches Grok A's auto-caption quote. The fall-distance physics is correct (h ∝ t²). | same |
| 2g-1…3 | Wu & Popović 2003 Table 2, 11 links / 21 DOF, 2-DOF feathers, 2.8 GHz | VERIFIED | PDF | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf |
| 2h-1…4 | Geijtenbeek 2013: 20/10/5 ms; 100 s⁻¹; 0.02; 10 or 20 s; 500–3000 generations; 2–12 h; real time; Table 3 totals 130/53/132/29; "20 muscles" | VERIFIED | PDF. "20 muscles" is derived from 10 per leg, which is fair. | https://goatstream.com/research/papers/SA2013/SA2013.pdf |
| 2i-1 | Rule 1: delays swing while the sender swings plus a speed-dependent delay; ipsilateral back-to-front | VERIFIED | "…plus a given velocity-dependent delay… Rule 1 operates between ipsilateral legs from back to front." | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ (Europe PMC XML, CC BY) |
| 2i-2 | Rule 2: 270 ms after stance start, active for 50 ms; ipsilateral back-to-front and contralateral | VERIFIED | Exact | same |
| 2i-3 | Rule 3: shifts lift-off forward when the sender reaches a speed-dependent position | VERIFIED | "The critical position of the sender leg depends on velocity and is different for influences between ipsilateral legs and between contralateral legs." | same |
| 2i-4 | Rule 4 "not described" | **FIX** | It is described: a Target-net that sets the AEP. See problem 2. | same |
| 2i-5…6 | Rule 5 load sharing; rule 6 not implemented | VERIFIED | Review | same |
| 2i-7 | Rules 1–4 are enough; tripod ≥3, tetrapod ≥4, wave ≥5 legs on the ground | VERIFIED | Review | same |
| 2j-1 | DeepLoco: 2 Hz high-level and 30 Hz low-level; 7 s of mocap or one keyframed cycle | VERIFIED | PDF | https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf |

### Not found (additions)
- Ijspeert 2008 and Mordatch 2010 full texts: I could not open them from the box (archive.org SSL error 35 or 504 timeouts). Grok A reports that the Mordatch author PDF exists on Wayback (`/web/20110602193901if_/http://www.dgp.toronto.edu/~mdelasa/data/siggraph2010_slip.pdf`). I have not opened it, so M9 stays UNVERIFIABLE.
- H6's "Walknet rule 4" Not-found bullet is resolved (see the FIX above).

### Sources disagree (additions)
- **Overgrowth keyframe total:** the third-party transcript says 13; Grok A's YouTube auto-caption excerpt counts 6 locomotion + 2 standing; "13" is otherwise found only in community posts.

### Source table (re-checked 2026-11-17)

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| SIMBICON | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf | y | y (author copy; closed ACM paper) |
| Coros 2010 | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf | y | y |
| Coros 2011 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf | y | y |
| Walknet review (Schilling et al. 2013) | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ | y | y (CC BY) |
| Geijtenbeek 2013 | https://goatstream.com/research/papers/SA2013/SA2013.pdf | y | y |
| DeepLoco | https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf | y | y |
| Overgrowth talk | https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 | y | y (talk; third-party transcript flagged) |
| Mordatch 2010 | https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ (DOI 10.1145/1778765.1778808; ACM 403 to bots) | y | y (© ACM) |
| Ijspeert 2008 | https://doi.org/10.1016/j.neunet.2008.03.014 | y (paywall) | y |

### Grok passes
- 2026-11-16 10:47–10:56 AEDT: see below.
- **2026-11-17 08:49:22–08:57:54 AEDT ("Grok A", exit 0).** For H6 it covered the Mordatch author PDF (Wayback) and the Overgrowth spoken keyframe counts. I did not take a number from it on its own. The Overgrowth conflict is recorded rather than used to change the row.

---


## Version checked (2026-11-16 partial pass)

- Path: `/workspace/specula-research/H6-rule-based-locomotion.md`
- Size: 29,720 bytes, **244 lines**
- File mtime: **2026-11-16 10:25:38 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 11:04 AEDT**. The file was unchanged from that mtime while the check ran.
- Access date for live URLs: **2026-11-16**.

## Grok live-search pass

A Grok CLI pass was run after sign-in: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` with live search on, **10:47–10:56 AEDT**, exit 0. Numbers were not taken from Grok alone. URLs it gave were opened here.

**What it raised, then checked:**

1. **JSimbicon.zip — no licence.** Confirmed: project page https://www.cs.ubc.ca/~van/papers/Simbicon.htm has no licence grant; zip HTTP 200, 50,417 bytes; listing has no LICENSE/COPYING/README licence file; `src/Simbicon.java` header is a demo credit only. H6 is right: treat as all rights reserved.
2. **cartwheel-3d (M2) — `asf20` = Apache License 2.0.** Confirmed from https://storage.googleapis.com/google-code-archive/v2/code.google.com/cartwheel-3d/project.json (`"license": "asf20"`) and a GitHub clone LICENSE.txt (Apache 2.0). The archive HTML itself is a JS stub.
3. **Coros 2011 `binary.zip` — no licence.** Confirmed on the project page (Windows binary only, “see the included readme.txt for details on the interface”). Grok did not download the 3.2 MB zip. **This check did:** `readme.txt` is interface instructions only (no grant). The only `license.terms` files inside are **Tcl/Tk 8.4** bundled libraries, not a licence for the demo itself. H6 is right.
4. **Mordatch 2010.** Title, year, and abstract (SLIP stance, projectile flight, high-level user commands) confirmed from DOI metadata / indexed abstract. The ACM PDF URL Grok used did not open here (Cloudflare). No legal author PDF was opened. **Full text stays unreached.** H6 already marked M9 as abstract-only.
5. **Ijspeert 2008.** Title, year, journal, and the abstract sentence on “simple, low-dimensional, input signals” confirmed (Europe PMC / Crossref). Grok opened a Wayback preprint and reported that the exact phrase **“drive level” does not appear**; the preprint uses **drive *d* / MLR stimulation level**. I could not open that Wayback PDF (TLS) or a working Infoscience PDF (the handle returned an SPA HTML shell). So: abstract-level claim is fine; the wording “such as drive level” is a paraphrase I could not confirm in a PDF I opened.
6. **Raibert & Hodgins 1991.** Full text **does** exist at https://www.cs.princeton.edu/courses/archive/spring01/cs598b/papers/raibert91.pdf (PDF opened here, 1,715,242 bytes, title and SIGGRAPH 1991 masthead confirmed). H6 correctly left it unread. It is not one of the 13 survey methods.

No conflict with Overgrowth being a real GDC 2014 Vault talk, or with the transcript being a third-party copy.

## Summary (2026-11-16 partial pass — superseded by "Final status" above)

Unit of counting for the headline table: **one survey-table method (M1–M13)**. That is what the brief asked to re-check (paper, year, claimed inputs, code/binary licence). Section 2 parameter blocks were not given a second full cell-by-cell audit in this signed-in pass; they are reported separately below and are **not** folded into the 13.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **11** | Paper/year exist, claimed inputs match a source I opened (full text or, for M12, the abstract plus a reachable CC BY full text), and the licence note is right. |
| **FIX** | **0** | No wrong year, title, or licence on the 13 methods. |
| **UNVERIFIABLE** | **2** | M9 and M11: H6 already checked these only at abstract/metadata level; I also did not open a usable full text. Abstract-level title/year/inputs are consistent with the abstracts I did open. |
| **Total survey methods** | **13** | M1–M13. |

**Most serious problems**

1. **None of the 13 methods has a wrong year or a wrongly claimed open licence.** The two “no licence” calls (JSimbicon, Coros 2011 binary) are correct and important. Apache 2.0 on cartwheel-3d and Overgrowth is correct.
2. **M9 (Mordatch 2010) and M11 (Ijspeert 2008) stay abstract-level**, as H6 already says in “Not found.” I confirmed the abstracts. I did not confirm §1 input wording for M11 beyond “simple, low-dimensional input signals.” The extra phrase “such as drive level” is a reasonable paraphrase of CPG literature (Grok: “drive *d*” in a preprint I could not open) but it is **not in the abstract I opened**. Leave the row UNVERIFIABLE for anything past that abstract, and do not treat “drive level” as a quoted term.
3. **M12 (Ambe 2021) is CC BY and the PMC full text is reachable** (PMC8450536; a PDF was fetched in this check). H6 says “Abstract read.” That is conservative, not wrong. Survey inputs (phase oscillator per leg, phase resetting, emergent direct/retrograde/source-wave gaits) are in the abstract. I did not re-extract body parameters (H6 has no numeric §2 block for M12).
4. **Overgrowth (M6):** the GDC Vault page exists and names David Rosen, *Animation Bootcamp: An Indie Approach to Procedural Animation*, with Overgrowth in the blurb (`https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach`, opened). H6 correctly notes that the numeric counts come from a **third-party transcript** (`https://reducer.xgoose.org/items/8812`), not from slides. GitHub `WolfireGames/overgrowth` is Apache-2.0 (README / API).
5. **Section 2 parameter tables were not re-audited cell-by-cell in this pass.** Author PDFs for M1–M5, M7, M8, M10 and M13 were on disk from earlier in the check, and earlier spot-checks had not turned up invented numbers, but that is not a completed numeric verification. Do not treat §2a–2j as VERIFIED in the headline counts.
6. **Raibert 1991 full text is now reachable** (Princeton course PDF). H6’s “full text not read” is still accurate as a statement about what the researcher did.

**What is in good shape.** The survey’s paper/year identifications for M1–M13 match the author PDFs, Crossref, or abstracts. Licence notes in §4 are careful and, where I re-opened the sources, correct. “Not found” and “Sources disagree” are present (SIMBICON article number 105 vs 81 is a real citation split with a shared DOI; gain conventions really differ between M1 and M2). The researcher already refused to invent a licence for the two unlicensed demos.

## Row table — §1 survey methods

Access date 2026-11-16 throughout.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| M1 | SIMBICON, Yin, Loken & van de Panne, 2007; FSM + PD; Java demo, no licence | VERIFIED | Author PDF and project page opened. Zip has no licence file. | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/Simbicon.htm ; https://www.cs.ubc.ca/~van/papers/simbicon/JSimbicon.zip |
| M2 | Coros, Beaudoin & van de Panne, 2010; Vd, T, IPM, cartwheel-3d Apache 2.0 (`asf20`) | VERIFIED | Author PDF and project page opened. Google Code `project.json` licence `asf20`; clone LICENSE.txt is Apache 2.0. | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf ; https://storage.googleapis.com/google-code-archive/v2/code.google.com/cartwheel-3d/project.json |
| M3 | Coros et al. 2011 quadruped; gait graph, T, trajectories; `binary.zip`, no source licence | VERIFIED | Author PDF and page opened. `binary/readme.txt` has no licence grant. | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf ; https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/index.html |
| M4 | Wampler & Popović 2009, *Optimal gait and form…* | VERIFIED | Title/year/DOI; author PDF opened (grail.cs.washington.edu). Inputs as a kinematic tree + objective are consistent with the paper. §2d numbers not re-audited here. | https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf |
| M5 | Hecker et al. 2008 Spore; any number of legs; third-party Chalmers PDF | VERIFIED | DOI/year; Chalmers copy was the readable one (author site had been down). Third-party copy is correctly flagged. | https://www.cse.chalmers.se/edu/year/2011/course/TDA361/Advanced%20Computer%20Graphics/027-hecker%20copy.pdf |
| M6 | Rosen, GDC 2014 Overgrowth talk; 2 key poses per cycle etc. from a third-party transcript; code Apache-2.0 | VERIFIED | Vault page opened (title, Overgrowth). Transcript is third-party, as H6 says. Repo licence Apache-2.0. Talk numbers themselves were not re-timed against the video in this pass, but the *existence* of the talk and the licence are confirmed. | https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://github.com/WolfireGames/overgrowth |
| M7 | Wu & Popović 2003 bird flight | VERIFIED | Author PDF opened. §2g Table 2 not re-audited cell-by-cell here. | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf |
| M8 | Geijtenbeek et al. 2013 muscle-based bipeds | VERIFIED | Author PDF opened (goatstream). | https://goatstream.com/research/papers/SA2013/SA2013.pdf |
| M9 | Mordatch, de Lasa & Hertzmann 2010; SLIP + projectile + user commands | **UNVERIFIABLE** (full text) | Abstract agrees with H6’s input/output sketch (SLIP, projectile flight, high-level commands, walking/running/jumping emerge). ACM HTML/PDF did not open here. H6 already confined this row to the abstract. | https://doi.org/10.1145/1778765.1778808 ; SIGGRAPH History abstract page |
| M10 | Walknet, Schilling et al. 2013 review (CC BY); Cruse et al. 1998 © Elsevier | VERIFIED | Europe PMC full text of the 2013 review opened (PMC3755227). Licence CC BY, as H6 says. Coordination-rule wording in §2i was not re-audited line-by-line here. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ |
| M11 | Ijspeert 2008 CPG review; low-dimensional inputs, “such as drive level” | **UNVERIFIABLE** (full text) | Abstract confirmed: CPGs produce “high-dimensional rhythmic output signals while receiving only simple, low-dimensional, input signals.” Journal 21(4):642–653, 2008. I did not open a working PDF. The words **“drive level” are not in the abstract.** | https://api.crossref.org/works/10.1016/j.neunet.2008.03.014 ; Europe PMC PMID 18555958 |
| M12 | Ambe et al. 2021; phase oscillator per leg, phase resetting; CC BY | VERIFIED | Abstract matches the survey inputs/outputs. PMC8450536 is CC BY and a full-text PDF was fetched; H6’s “abstract read” is conservative. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ |
| M13 | DeepLoco, Peng et al. 2017; learned contrast, not rule-based | VERIFIED | Author PDF opened. Correctly marked as a learned contrast, not a rule-based method. | https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf |

## Section 2 parameter blocks (2026-11-16 — superseded by the §2 row table in "Final status")

| Block | Status | Note |
|---|---|---|
| 2a SIMBICON Table 1 / gains / robustness | Not re-audited cell-by-cell in this pass | Author PDF and project page were opened earlier in the check. H6 already says run/skip rows were not transcribed. |
| 2b–2c Coros 2010 / 2011 (α=0.05, T range, dog 34 kg, speeds 3.0 vs 3.3 and 5.7 vs 6.5 m/s, etc.) | Not re-audited cell-by-cell | Author PDFs opened; earlier spot-checks had not found invented figures. |
| 2d–2e, 2g–2h, 2j | Not re-audited cell-by-cell | PDFs on disk. |
| 2f Overgrowth talk numbers | **UNVERIFIABLE** as slide-accurate | They come from a third-party auto-transcript, which H6 already flags. Vault page confirms the talk, not the counts. |
| 2i Walknet rules | Not re-audited line-by-line | 2013 review full text is reachable (CC BY). |

## Not found (this check)

- A licence for JSimbicon or for the Coros 2011 quadruped binary (none found; H6 is right).
- A usable Ijspeert 2008 PDF (Wayback TLS error; Infoscience returned an HTML shell). A green OA copy is reported to exist.
- A Mordatch 2010 PDF that would open from this box.
- Numeric Overgrowth gains (stiffness, damping, surveyor-wheel radius) — also in H6’s own “Not found.”
- Kry 2009 “Modal locomotion” full text (H6 already lists it).

## Sources disagree

- **SIMBICON article number:** Crossref / later papers: *ACM TOG* 26(3) Article **105**. Some citations (H6 notes Mordatch’s list) give Article **81**. Same DOI. H6 already records this.
- **M1 vs M2 damping conventions** (kd = 0.1·kp vs kd = 2·√kp) are different on purpose; H6 already says not to mix them.
- **Ijspeert wording:** “drive level” is not a quoted abstract phrase; the abstract says “simple, low-dimensional, input signals.”
- **Real vs simulated dog speeds (M3):** authors state the gap themselves (H6 already lists it).

## Source table

| Key | URL | Resolves y/n | Licence correct y/n |
|---|---|---|---|
| M1 paper + page | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; Simbicon.htm | y | y (© ACM; demo has no licence) |
| M1 JSimbicon.zip | https://www.cs.ubc.ca/~van/papers/simbicon/JSimbicon.zip | y (200, 50,417 bytes) | y (no licence file) |
| M2 paper | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf | y | y (© ACM) |
| M2 cartwheel-3d | https://storage.googleapis.com/google-code-archive/v2/code.google.com/cartwheel-3d/project.json | y | y (asf20 / Apache 2.0) |
| M3 paper + binary | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/index.html ; binary.zip | y | y (no licence for the demo; Tcl/Tk licences inside the zip are not a grant for the app) |
| M4–M5, M7–M8, M13 author PDFs | URLs in H6 §6 | y (opened earlier / this check) | y (© ACM; facts cited) |
| M6 GDC Vault | https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach | y | y (talk © Informa; H6 says so) |
| M6 transcript | https://reducer.xgoose.org/items/8812 | y (fetched earlier) | y (correctly flagged third-party) |
| M6 Overgrowth code | https://github.com/WolfireGames/overgrowth | y | y (Apache-2.0) |
| M9 Mordatch | https://doi.org/10.1145/1778765.1778808 | y (metadata/abstract); n (ACM PDF here) | y (© ACM) |
| M10 Walknet 2013 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ | y | y (CC BY) |
| M11 Ijspeert | Crossref / Europe PMC / Infoscience | y (abstract/metadata); n (working PDF here) | y (© Elsevier; green copy exists but was not opened) |
| M12 Ambe 2021 | https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ | y (PMC PDF) | y (CC BY) |
| Raibert 1991 (not a survey row) | https://www.cs.princeton.edu/courses/archive/spring01/cs598b/papers/raibert91.pdf | y (full text opened) | n/a (© ACM; H6 did not use it) |


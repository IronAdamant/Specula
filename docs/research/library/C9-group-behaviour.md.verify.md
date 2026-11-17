# Verification: C9-group-behaviour.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/C9-group-behaviour.md`
Output path (this file only): `/workspace/specula-research/C9-group-behaviour.md.verify.md`

Private adversarial source-check. No number was taken from Grok, a search snippet, or a pirate mirror. Every figure below was opened in a publisher page, PMC/Europe PMC full text (JATS XML), an official or author/institutional PDF, an arXiv preprint PDF, or Crossref metadata. The C9 file itself was not edited.

## Summary

- **64 data rows** checked (sections 1–7). Rows 46–64 restate rows 1–45 in engine terms, as C9 says; they were checked as restatements, not as independent measurements.
- **58 VERIFIED.** Almost every C9 “Verified” number that I could open matches the source, with those units and that population. Several rows C9 had only as search excerpts or abstracts were confirmed in full text (Couzin 2002; Lukeman 2010 plus SI; Ginelli 2015 main text; Bennie 2014 author PDF; Katz 2011 full text).
- **1 number FIX:** row 25 (Tunstrøm 2013). “Swarm or transitional: 0.09–0.11 at all sizes” mixes two different quantities. The paper’s Fig. 4A text is: swarm state 0.09–0.11, **transition region 0.31–0.44**. Polar and milling fractions are correct. See close check 8.
- **5 UNVERIFIABLE,** the same IDs C9 already marked Unverified (rows 33, 34, 43, 52, 56). Row 33 is only partly so: packed density 1.3 sheep/m², spread of about 1 sheep per 23 m², 74% of sheep motionless, and the 0.41 m/s walk/run threshold are in the Ginelli 2015 **main text**. The walking mean 0.18 m/s, running mean 0.76 m/s, and time fractions 0.17/0.09 were not found in the main text; the SI PDF was not opened in this pass (PNAS SI URLs returned 403).
- **Rows skipped:** none of the 64.
- **Dead links:** none of the 35 unique URLs is a dead identifier. Scripted HEAD/GET to PNAS, APS, ACM, and the Taylor & Francis landing for `10.1071/MU12026` returned 403; OUP (`10.1093/beheco/arj020`) and Springer (`10.1007/BF00292770`) returned 500. The same works still resolve in Crossref, and legal copies were opened for every source that C9 marked Verified (author/institutional PDFs, PMC XML, arXiv, PLOS, UCL Discovery, the red3d Reynolds reprint, the Blumstein-lab Weston PDF).
- **Worst problem:** the only opened-source number error is row 25’s lumping of swarm time with the much larger transition-region share. No other opened source contained a wrong number.

One-line summary: C9 is solid on the models, starling tables, FID values and licences; fix the Tunstrøm swarm/transition split; treat the Ginelli main-text density and 0.41 m/s threshold as verified; leave Herbert-Read’s spacing numbers, Stankowich 2006/2008 and Partridge 1980 Unverified until those texts are opened.

## File snapshot

At the start of this check the C9 file was copied to `/workspace/lv-scratch/c9/C9-group-behaviour.md.start` (not `/tmp`). Snapshot: **36,914 bytes**, sha256 `6b9ea679d024fec19287533506b2ea8dc2b703563555b6072045befbef7ef9d8`, 199 lines (64 data rows), mtime 17 November 2026, 11:18:40 AEDT. That size and time match Creature Data’s report.

Immediately before this verify file was written (17 November 2026, 11:36:04 AEDT), `sha256sum` and `cmp` of the workspace file against that snapshot were identical. **No content change; no row re-check was required.** End snapshot: **36,914 bytes**, sha256 `6b9ea679d024fec19287533506b2ea8dc2b703563555b6072045befbef7ef9d8`.

## Close checks

### 1. arXiv-only rows: Ballerini, Cavagna, Vicsek, Nagy

**Vicsek et al. 1995**, *Phys. Rev. Lett.* 75:1226, doi `10.1103/PhysRevLett.75.1226`. Opened: official arXiv PDF `cond-mat/0611743v1` (29 Nov 2006 reprint; HTTP 200, 201,877 bytes, 21 pages). Semantic Scholar lists this arXiv file as the open-access PDF of the PRL. The APS landing returned 403 to scripts, so the printed VOR was not compared side by side. In the arXiv reprint: *r* = 1 and Δ*t* = 1 are the units; *v* = 0.03; noise is uniform in [−η/2, η/2]; η_c(∞) = 2.9 ± 0.05 at ρ = 0.4; β = 0.45 ± 0.07, δ = 0.35 ± 0.06; Fig. 1 uses *N* = 300, with *L* = 25, η = 0.1 (groups form) and *L* = 5, η = 0.1 (ordered). The paper notes that the “infinite temperature” limit is η = 2π, so quoting η_c ≈ 2.9 as radians (row 60) is fair. Rows 6 and 60 **VERIFIED (preprint)**. No VOR discrepancy was seen because the VOR PDF was not opened.

**Ballerini et al. 2008**, *PNAS* 105:1232, doi `10.1073/pnas.0711437105`. Opened: arXiv `0709.1916` (HTTP 200, 2,124,607 bytes, 25 pages, including SM). PMC2234121 was only an abstract-sized deposit in this pass. Text: *n_c* = 6.5 ± 0.9 SE; “6–7 neighbours”; *n_c* vs *r*₁, *n* = 10, *R*² = 0.00021, *P* = 0.97; *r_c* vs *r*₁, *R*² = 0.78, *P* = 0.00072 (figure caption uses *P* < 0.00072); *r*₁ from 0.68 m to 1.51 m; *r* ~ *r*₁ *n*^{1/3}. Rows 12–13 and 46–47 **VERIFIED (preprint)**.

**Ballerini et al. 2008**, *Anim. Behav.* 76:201, doi `10.1016/j.anbehav.2008.02.004`. Opened: arXiv `0802.1667` (HTTP 200, 2,795,376 bytes, 33 pages). Table 1 (ten events) matches row 14 exactly: *N* = 448–2631; density 0.04–0.80 birds/m³; *r*₁ 0.68–1.51 m; centre-of-mass speed 6.9–15.2 m/s. Recomputed mean speed 10.59 m/s, sample SD 2.26, which is C9’s 10.6 (SD 2.3). Thickness *I*₁ 5.29–19.00 m; *I*₂/*I*₁ = 2.8 ± 0.4 and *I*₃/*I*₁ = 5.6 ± 1.0 (95% CI); |*I*₁·*G*| = 0.93 ± 0.04; |*V*·*G*| = 0.13 ± 0.05. Methods: about 500 events over 40 sessions; 10 analysable (*N* > 400, sharp borders); wind never exceeded 12 m s⁻¹; cameras at 10 fps. Rows 14–16 and 49 **VERIFIED (preprint)**.

**Cavagna et al. 2010**, *PNAS* 107:11865, doi `10.1073/pnas.1005766107`. Opened: arXiv `0911.4393` (HTTP 200, 1,306,877 bytes, 24 pages, including Table S1; PDF metadata “cavagna-text-v3”, 23 Nov 2009). This is a preprint, not the VOR. Main text: Φ = 0.96 ± 0.03 (SD) over 24 flocks; *N* 122–4268; *L* 9.1–85.7 m; scale-free correlation length growing with *L*. Table S1 per-flock polarisation 0.844–0.995; speeds 6.8–19.1 m/s. Recomputed mean speed 11.59 m/s, sample SD 2.48, which is C9’s 11.6 (SD 2.5). Event-label disagreements with Ballerini Table 1 are real and are correctly recorded in C9’s “Sources disagree”: 16-05 is 2631 birds at 15.2 m/s vs 2941 at 15.2; 32-06 is 781 at 9.6 vs 809 at 9.8; 31-01 is 1856 at 6.9 vs 2126 at 6.8. Rows 17–19, 48, 50 and 63 **VERIFIED (preprint)**. Row 19 can be read in the preprint body and Fig. 2, not only the abstract.

**Nagy et al. 2010**, *Nature* 464:890, doi `10.1038/nature08891`. Opened: arXiv `1010.5394` (HTTP 200, 506,960 bytes, 8 pages). Semantic Scholar lists this arXiv file as the OA PDF of the Nature paper. The Nature landing returned 200 with a cookies interstitial; the VOR PDF was not opened. In the arXiv text: flocks of up to 10 homing pigeons, GPS; directional-correlation delay 0.37 s (± 0.27 SD) at *C*_min = 0.5 and 0.32 s (± 0.20 SD) at *C*_min = 0.9; example pair delays 0.07 s and 0.14 s in Fig. 3. The arXiv text says “homing pigeons” and does not use *Columba livia*; that binomial in row 20 is C9’s gloss (standard, but not in the cited text). Rows 20 and 57 **VERIFIED (preprint)** for the numbers.

### 2. PNAS rows that C9 took from search excerpts: Lukeman, Ginelli, Bennie

**Lukeman, Li and Edelstein-Keshet 2010**, *PNAS* 107:12576, doi `10.1073/pnas.1001763107`. Opened: author PDF and SI from Ryan Lukeman’s StFX page (`https://people.stfx.ca/rlukeman/PNASLukemanPub.pdf`, 877,348 bytes; `PNASLukemanSI.pdf`, 1,853,982 bytes). Fig. 4 caption: ranges −50% to +50% of ω_rep = 8.5, ω_att = 1.2, ω_al = 0.74, ω_front = 0.08, ω_ξ = 0.37. Results: mean NND 1.45 BL (SD 0.25 BL); mean speed 2.0 BL/s; preferred distance ≈ 1.5 BL; frontal-neighbour preference. ⚠ARR (PNAS) as C9 said. Rows 10, 21 and 58 **VERIFIED**, now from the full text rather than an excerpt. SI Table S1 fills C9’s stated gap on zone radii (not a fix of a C9 number): *r*_rep = 1.45 BL (data), *r*_al = 3 BL (data, estimated), *r*_att = 5 BL (chosen), frontal sector ±30°.

**Ginelli et al. 2015**, *PNAS* 112:12729, doi `10.1073/pnas.1503749112`. Opened: University of Aberdeen AURA publisher PDF `https://aura.abdn.ac.uk/bitstream/2164/5128/1/PNAS_2015_Ginelli_12729_34.pdf` (HTTP 200, 936,416 bytes). “Freely available online through the PNAS open access option”; no CC licence, so ⚠ARR is correct. Experimental set-up and model values in rows 11, 31 and 32 all match the main text (“Parameter Estimation…” and Results): 100 same-age ewes (Merino *Ovis aries*), 80 × 80 m, five 1-h trials, 1 frame/s from a 7-m tower; packing typically about every 15 min, started by a peripheral sheep running toward the centre; no audible vocalisation that could serve as an alarm; max individual speed about 1.5 m/s; group-speed error 0.03 m/s; power-law tail γ = −2.3(2); walking peak ≈ 0.1 m/s and a running shoulder in Fig. S4A (order-of-magnitude “running about 1 m/s” in row 32 is fair). Model: *v*₁ = 0.15 m/s, *v*₂ = 1.5 m/s, τ(1→0) = 8 s, τ(0→1) = 35 s, τ(0,1→2) = τ(2→0) = *N* s, *d*_S = 6.3 m, *d*_R = 31.6 m, *r*_e = *r*₀ = 1 m, β = 0.8, η = 0.13, *N* = 100; packing only when α > 5 and δ > 2. I did **not** find “= 5 *d*_S” or “η range tested 0.1–0.5” in the main text (5 × 6.3 = 31.5, so 31.6 is not exactly 5 *d*_S); those two asides stay unconfirmed without the SI. Rows 11, 31, 32 and 53 **VERIFIED** from the full text.

The same main text also contains several of row 33’s “SI-only” claims: density “from 1.3 individuals per square meter up to around 1 sheep in 23 m²”; “a large majority (74%) of sheep are actually motionless”; “~*v* ≃ 0.41 m/s, as a practical threshold to distinguish walking from running.” See close check 7.

**Bennie, Duffy, Inger and Gaston 2014**, *PNAS* 111:13727, doi `10.1073/pnas.1216063110`. Opened: author PDF on Kevin Gaston’s site, `https://kevingaston.com/wp-content/uploads/2017/05/Bennie-et-al-14-Biogeography-of-time-partitioning-in-mammals.pdf` (HTTP 200, 1,553,551 bytes). “69% in our dataset; 20% diurnal, 8.5% cathemeral, and 2.5% crepuscular.” “In total, 4,477 species were used in this analysis, with 612 nonfossorial species omitted due to lack of data on their temporal niche.” That completes the reason C9 said was cut off in the excerpt; the 4,477 and 612 figures are right. Methods also remove 184 highly/fully fossorial species (a different subtraction, not a contradiction). Pagel’s λ = 0.947, *P* < 0.001. Diurnal dominance at high altitude on the Tibetan Plateau and Andes; crepuscular aggregations in southern Africa, India and China; cathemeral richness in mid- to high-latitude ranges and the Arctic; nocturnal share highest in arid regions. ⚠ARR is correct. Rows 44, 45 and 62 **VERIFIED**, now from the full text. C9’s parenthetical about the cut-off excerpt can be replaced with the completed sentence.

### 3. Weston 2012 as a compiled source

**Weston, McLeod, Blumstein and Guay 2012**, *Emu* 112:269–286, doi `10.1071/MU12026`. Opened: Blumstein-lab author PDF (HTTP 200, 428,131 bytes). It is a **review compilation**: Appendix 1 lists species-level walker FIDs drawn from numbered source studies, not new primary measurements. C9’s status “Verified (review compilation of primary studies)” is the right framing. Every number in rows 40–42 matches Appendix 1 or the text:

- Emu 58.7 (SD 36.2, *n* 6); Black Swan 50.4 (35.8, 19); Australian Pelican 32.6 (25.4, 39); Great Cormorant (*Phalacrocorax carbo*) 32.3 (20.6, 34); Eastern Great Egret 39.9 (24.8, 79); Cattle Egret (*Ardea ibis*) 63.1 (46.8, 11); Australian White Ibis 32.8 (20.4, 48); Pacific Black Duck (*Anas superciliosa*) 38.9 (29.0, 50).
- Eastern Curlew 65.5 (41.6, 42) vs 126.1 (29.2, 22); Masked Lapwing 46.8 (30.5, 37) vs 62.6 (43.1, 55); Hooded Plover 54.4 (35.4, 30), 41.1 (17.1, 8), 26.3 (3.3, 4).
- Black Swan at Albert Park Lake 3.6 (SD 3.8, *n* 92) vs 50.4 in the general sample. “The maximum FID recorded for any Australian species to date is 196 m for the Eastern Curlew … (Glover et al. 2011).”

Licence ⚠ARR (CSIRO Publishing) is correct; the official T&F/CSIRO landing returned 403 to scripts. The study-to-study spread in C9’s “Sources disagree” #5 is faithfully taken from this appendix. Rows 40–42 and 55 **VERIFIED (review compilation)**.

### 4. Kelley & Ouellette licence (CC BY-NC-ND)

**Kelley and Ouellette 2013**, *Sci. Rep.* 3:1073, doi `10.1038/srep01073`, PMC3545223. Opened: Europe PMC JATS full text. The permissions block is: “This work is licensed under a Creative Commons Attribution-NonCommercial-NoDerivs 3.0 Unported License” (`creativecommons.org/licenses/by-nc-nd/3.0/`). C9’s ⚠NC ⚠ND (CC BY-NC-ND 3.0) is **correct**. Numbers also match: *p* ≤ 0.09 in every swarm; swarms typically 20–30 cm; *R*_s ∝ ⟨*N*⟩^{1/3}; MSD a power law up to *t* ≈ 0.5 s (in fact ⟨*d*²⟩ ~ *t*², ballistic); *Chironomus riparius*, 91-cm cubical enclosure, 23 °C. Wording niggle, not a number fix: the paper says midges behave “as if they are trapped in an **elastic** potential well (since the **effective force is linear** in position)” and that this potential is **quadratic**. C9’s “linear (spring-like) potential well” conflates a linear force with a linear potential. Rows 29–30, 50 and 59 **VERIFIED**.

### 5. Díaz 2013 flight distances

**Díaz et al. 2013**, *PLoS ONE* 8:e64634, doi `10.1371/journal.pone.0064634`, PMC3665823. Opened: Europe PMC JATS full text. Licence **CC BY 4.0** (`creativecommons.org/licenses/by/4.0/`), as C9 said. All of rows 36–39 match Results, Table 1 and Methods: mean FID 11.46 m (SE 1.03), back-transformed; population means 1–180 m; 12,495 FIDs, 714 populations, 159 species; rural 13.34 m (SE 1.05) vs urban 7.12 m (SE 1.05); paired *t* = −13.99, df = 229; effect size 27% after body mass and phylogeny. Body mass *r* = 0.603 (95% CI 0.554–0.648); habitat *r* = 0.524; latitude *r* = 0.219, with FID shorter further north; adjusted *R*² = 0.33; λ = 0.472. Protocol: observer walks at normal walking speed, counting 1 m steps; approaches start from at least 30 m; FID is the Euclidean (straight-line) distance including height. Nine rural–urban site pairs, Spain to Finland, 37–66°N, 2009–2010. Rows 36–39 and 55 **VERIFIED**.

(Table 1’s latitude CI is printed as 0.222–0.357 around a point estimate of 0.219, which does not contain the point estimate. That is a quirk in the source table; C9 quoted only the point estimate, so it is not a C9 error.)

### 6. Boids, Couzin and StarDisplay parameters

**Reynolds 1987**, author-hosted OCR reprint `https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf` (HTTP 200, 102,978 bytes). Three rules in decreasing precedence: collision avoidance, velocity matching, flock centring. Acceleration requests are combined by priority up to a maximum acceleration, “expressed as a fraction of the maximum speed.” Neighbourhood: a sphere with sensitivity an inverse exponential of distance, two parameters (radius and exponent); **no numeric values** for radius, exponent or rule weights. Linear distance weighting produced a “bouncy” flock; it was changed to inverse-square. Steer-to-avoid aims one body length beyond the obstacle silhouette edge. 80 boids, about 95 s per frame, O(*N*²); a 10 s, 300-frame test took about eight hours on a Symbolics 3600. Field of view about 300°, binocular overlap 10–15°, stated as a general remark about “real birds,” not a cited measurement (row 22’s caveat is right). ⚠ARR (ACM; author reprint) is correct. Rows 1–3 and 22 **VERIFIED**.

**Couzin et al. 2002**, *J. Theor. Biol.* 218:1–11, doi `10.1006/jtbi.2002.3065`. Opened: publicly posted journal PDF (identical copies from academic hosts, 371,851 bytes), including Table 1 and the Fig. 3 caption. Table 1 matches row 4 exactly: *N* 10–100; *r*_r = 1; Δ*r*_o 0–15; Δ*r*_a 0–15; α 200–360°; turning rate 10–100 °/s; *s* 1–5 units/s; error SD 0–11.5° (0–0.2 rad); τ 0.1 s. Fig. 3 caption: *N* = 100, *r*_r = 1, α = 270°, turning rate 40 °/s, *s* = 3, error = 0.05; means of 30 replicates; states swarm, torus, dynamic parallel, highly parallel; region (e) has a greater than 50% chance of fragmenting. The 0.1 s step is explicitly “corresponding to the response latency of fish (Partridge & Pitcher, 1980),” so the Couzin rationale in row 57 can be upgraded from unverified. The paper also says the model is analysed after it reaches a stable state, “always within 5000 time steps, equivalent to 8.3 min of real time”; C9 correctly did not put that Grok lead in a data row. ⚠ARR (Elsevier) is correct. Rows 4–5 and 61 **VERIFIED**, now from the full PDF rather than an excerpt.

**Hemelrijk and Hildenbrandt 2011**, *PLoS ONE* 6:e22479, doi `10.1371/journal.pone.0022479`, PMC3150374, CC BY (Attribution License in the PMC XML). Table 1 (“Default parameter values”) matches rows 7–9 in full: Δ*t* 5 ms; reaction time 50 ms; cruise speed 10 m/s; mass 0.08 kg; *C*_L/*C*_D 3.3; default lift 0.78 N; default drag = thrust 0.24 N; speed-control time constant 1 s; *n*_c = 6.5; *R*_max 100 m; hard sphere 0.2 m; separation radius 4 m; Gaussian σ = 1.37 m; weights 1 N, 1 N, 0.5 N, 0.01 N; rear blind angle 2×45°; banking *w*_in = 10, *w*_out = 1; interior if centrality < 0.35; roost radius 150 m; horizontal boundary 0.01 N/m; vertical 0.2 N. Rows 7–9 and 47 **VERIFIED (model parameter)**.

### 7. Unverified rows 33, 34, 43, 52 and 56

**Row 33 (Ginelli SI gait statistics).** As in close check 2, the main text already has packed density ≈ 1.3 sheep/m², spread ≈ 1 per 23 m², 74% motionless, and the 0.41 m/s threshold. Those four claims should be treated as **verified**. I did not find walking mean 0.18 m/s, running mean 0.76 m/s, or time shares 0.17 walking / 0.09 running in the main text (the text gives a walking *peak* of ≈ 0.1 m/s and a running *shoulder* around 0.66 m/s in Fig. S4A). The official SI PDF returned 403. Those three figures remain **UNVERIFIABLE** in this pass. The row as a whole should stay marked Unverified until the SI is opened, with the main-text subset upgraded.

**Row 34 (Herbert-Read 2011 spacing and speed).** The abstract (PMC3219133) matches row 28 and was opened. I could not legally open the full text or SI (PNAS 403; institutional copies 403/404). The NND mean 6.2 cm, median 3.8, mode 2.6, switch at about 6 cm, cruise 6.9 cm/s, and polarisation 0.84/0.71/0.63 were not confirmed. **UNVERIFIABLE**, as C9 said.

**Row 43 (Stankowich & Coss 2006).** The work exists: Crossref title, year and doi `10.1093/beheco/arj020` resolve. The OUP landing returned HTTP 500 to this crawler; no author PDF was opened. The FID quadratic, the 87 m peak at SD = 219 m, the 169 m alert distance and *n* = 78 were not read. **UNVERIFIABLE**, as C9 said.

**Row 52 (Partridge et al. 1980).** The work exists: Crossref, *Behav. Ecol. Sociobiol.* 6:277–288, doi `10.1007/BF00292770`. Springer returned HTTP 500; Semantic Scholar elides the abstract. I did not read the abstract or the species NND table. C9’s qualitative claims (herring more widely spaced than saithe or cod; NND falls with school size and speed) remain **UNVERIFIABLE**, as C9 said.

**Row 56 (Stankowich 2008).** The work exists and the doi resolves (Elsevier 200). Europe PMC returned the abstract, which is a meta-analysis of ungulate escape responses (including FID) and does **not** quote mean FIDs in metres or the claimed *r* = 0.21 / 0.31 effect sizes. Table 1 was not opened. **UNVERIFIABLE**, as C9 said. The abstract is consistent with C9’s qualitative point that this is an effect-size review, not a table of species-mean FIDs in metres.

### 8. Row 25 FIX (Tunstrøm 2013)

**Tunstrøm et al. 2013**, *PLoS Comput. Biol.* 9:e1002915, PMC3585391, CC BY (Attribution License). Full text opened. Setup (row 23), thresholds (row 24), 15.5 vs 15.6 cm (row 26), and the density-control experiment (row 64: 30 fish in 0.66 × 0.38 m, 4 replicates, milling not increased) all match. Polar time 0.57 → 0.26 → 0.18 → 0.05 and milling 0.03 → 0.18 → 0.36 → 0.45 for 30 → 70 → 150 → 300 fish also match.

The error is the last clause of row 25. The paper says, of Fig. 4A: “The group size has however little effect on the proportion of time spent in the swarm state or in the transition region, which varies between 0.09 and 0.11 (dark blue columns Fig. 4A), and 0.31 and 0.44 (brown columns in Fig. 4A), respectively.” C9’s “Swarm or transitional: 0.09–0.11 at all sizes” applies the swarm range to the transition region as well. **Corrected value:** swarm state 0.09–0.11 at all sizes; transition region 0.31–0.44. Source: Tunstrøm 2013 Results, text of Fig. 4A, `https://doi.org/10.1371/journal.pcbi.1002915`. (The numbers are stated in the text with Fig. 4A; Fig. 2 is the density-plot context C9 cited.) Row 51, which only restates the 57% / 45% polar and mill figures, is unaffected and stays **VERIFIED**.

## Row table

| Row | Claim | Status | Note | URL |
|---|---|---|---|---|
| 1 | Boids rule order: 3 rules, priority up to max acceleration (a fraction of max speed) | VERIFIED | Reynolds 1987, “Simulated Flocks” / “Geometric flight”; author PDF | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| 2 | Neighbourhood: inverse-exponential sphere; no numeric radius or weights; linear weighting was “bouncy”, then inverse-square | VERIFIED | “Simulated Perception”; no numbers found | same |
| 3 | Steer-to-avoid: 1 body length beyond silhouette; 80 boids ~95 s/frame; 10 s / 300 frames ~8 h on Symbolics 3600 | VERIFIED | “Environmental Obstacles” and “Notes on Implementation” | same |
| 4 | Couzin Table 1 ranges (*N* 10–100, *r*_r=1, Δ*r*_o/Δ*r*_a 0–15, α 200–360°, θ 10–100 °/s, *s* 1–5, error 0–0.2 rad, τ 0.1 s) | VERIFIED | Full journal PDF, Table 1; was excerpt-only in C9 | https://doi.org/10.1006/jtbi.2002.3065 |
| 5 | Fig. 3 defaults (*N*=100, α=270°, θ=40 °/s, *s*=3, error=0.05, 30 runs); four states; >50% split region | VERIFIED | Fig. 3 caption in the same PDF | same |
| 6 | Vicsek *v*=0.03, *r*=1, Δ*t*=1; η_c(∞)=2.9±0.05 at ρ=0.4; β=0.45±0.07; δ=0.35±0.06; *N*=300 examples | VERIFIED | arXiv cond-mat/0611743 reprint; APS VOR not opened | https://arxiv.org/abs/cond-mat/0611743 |
| 7 | StarDisplay timing/flight (5 ms, 50 ms, 10 m/s, 0.08 kg, L/D 3.3, 0.78 N, 0.24 N, 1 s) | VERIFIED | Hemelrijk 2011 Table 1, PMC full text, CC BY | https://doi.org/10.1371/journal.pone.0022479 |
| 8 | StarDisplay neighbour rules (*n*_c=6.5, 100 m, 0.2 m, 4 m, σ=1.37 m, weights, 2×45°, *w*_in/out, centrality 0.35) | VERIFIED | same Table 1 | same |
| 9 | Roost radius 150 m; horiz. 0.01 N/m; vert. 0.2 N | VERIFIED | same Table 1 | same |
| 10 | Lukeman weights 8.5 / 1.2 / 0.74 / 0.08 / 0.37, ±50% | VERIFIED | Author PDF, Fig. 4 caption (was excerpt) | https://doi.org/10.1073/pnas.1001763107 |
| 11 | Ginelli model *v*₁=0.15, *v*₂=1.5, τ’s, *d*_S=6.3, *d*_R=31.6, *r*_e=1 m, η=0.13, β=0.8, α>5, δ>2, *N*=100 | VERIFIED | AURA publisher PDF, “Parameter Estimation”. “= 5 *d*_S” and η range 0.1–0.5 not in the main text | https://doi.org/10.1073/pnas.1503749112 |
| 12 | *n*_c=6.5±0.9 SE; “6–7”; *R*²=0.00021 / 0.78 | VERIFIED | arXiv 0709.1916; PNAS VOR not opened | https://arxiv.org/abs/0709.1916 |
| 13 | *r*₁ 0.68–1.51 m; *r*≈*r*₁ *n*^{1/3} | VERIFIED | same preprint, plus Ballerini 2008 *Anim. Behav.* Table 1 | same |
| 14 | *N* 448–2631; density 0.04–0.80 m⁻³; *r*₁ 0.68–1.51 m; speed 6.9–15.2 m/s; mean 10.6 (SD 2.3) computed | VERIFIED | arXiv 0802.1667 Table 1; mean/SD recomputed, match | https://arxiv.org/abs/0802.1667 |
| 15 | *I*₁ 5.29–19.0 m; aspect 2.8±0.4 and 5.6±1.0 (95% CI); \|*I*₁·*G*\|=0.93±0.04; \|*V*·*G*\|=0.13±0.05 | VERIFIED | same Table 1 and Results | same |
| 16 | ~500 events, 40 sessions, 10 analysable (*N*>400); wind ≤12 m/s; 10 fps | VERIFIED | Methods | same |
| 17 | Φ=0.96±0.03 over 24 flocks; per-flock 0.844–0.995 | VERIFIED | arXiv 0911.4393 text and Table S1 | https://arxiv.org/abs/0911.4393 |
| 18 | Speed 6.8–19.1 m/s, mean 11.6 (SD 2.5); *L* 9.1–85.7 m | VERIFIED | Table S1; mean/SD recomputed, match | same |
| 19 | ξ grows linearly with *L* (scale-free) | VERIFIED | Preprint body and Fig. 2, not only the abstract | https://doi.org/10.1073/pnas.1005766107 |
| 20 | Delays 0.37±0.27 s (*C*_min=0.5) and 0.32±0.20 s (0.9); examples 0.07–0.14 s; flocks up to 10, GPS | VERIFIED | arXiv 1010.5394. “Homing pigeons”; *Columba livia* is not in the preprint text | https://arxiv.org/abs/1010.5394 |
| 21 | NND 1.45 BL (SD 0.25); speed 2.0 BL/s; preferred ~1.5 BL; frontal preference | VERIFIED | Lukeman author PDF, Results | https://doi.org/10.1073/pnas.1001763107 |
| 22 | ~300° field of view; 10–15° binocular overlap (Reynolds’ uncited remark) | VERIFIED | As quoted; not a measurement | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| 23 | Golden shiner set-up: ~5 cm (BL=5.2 cm); 30/70/150 ×7 and 300 ×3; 2.1×1.2 m, 5 cm, 56 min, 30 fps | VERIFIED | Tunstrøm PMC full text | https://doi.org/10.1371/journal.pcbi.1002915 |
| 24 | Polar: *O*_p>0.65 and *O*_r<0.35; mill: *O*_p<0.35 and *O*_r>0.65; swarm: both <0.35 | VERIFIED | same, Results/Methods | same |
| 25 | Polar 0.57→0.26→0.18→0.05; mill 0.03→0.18→0.36→0.45; **“swarm or transitional 0.09–0.11”** | FIX | Polar and mill are right. Swarm is 0.09–0.11; **transition region is 0.31–0.44** (Fig. 4A text). See close check 8 | same |
| 26 | Local-order radius 15.5 cm (Results) or 15.6 cm (~3 BL, Methods) | VERIFIED | Both values are in the paper, as C9 said | same |
| 27 | Katz rules: speed regulation dominates; alignment from attraction/repulsion; copy those ahead; no explicit orientation matching; three-body effects; persists at 10 and 30 | VERIFIED | Full text (UVM copy) as well as the abstract | https://doi.org/10.1073/pnas.1107583108 |
| 28 | Herbert-Read abstract: attraction; weak alignment; repulsion via speed; single nearest neighbour; *G. holbrooki* | VERIFIED | PMC abstract | https://doi.org/10.1073/pnas.1109355108 |
| 29 | Midge *p*≤0.09; *C. riparius*, 91 cm, 23 °C | VERIFIED | Kelley PMC full text; licence CC BY-NC-ND 3.0 | https://doi.org/10.1038/srep01073 |
| 30 | Swarms typically 20–30 cm; *R*_s ∝ ⟨*N*⟩^{1/3}; spring-like well; MSD power law to *t*≈0.5 s | VERIFIED | Potential is elastic/quadratic (linear *force*). Wording niggle only | same |
| 31 | 100 ewes, 80×80 m, five 1-h trials, 1 fps, 7 m tower; packing ~every 15 min; peripheral starter; no alarm calls | VERIFIED | Ginelli AURA PDF, Results | https://doi.org/10.1073/pnas.1503749112 |
| 32 | Max speed ~1.5 m/s; COM error ~0.03 m/s; γ=−2.3(2); walking ~0.1 m/s, running ~1 m/s (order of magnitude) | VERIFIED | same, Results / Fig. S4A mention | same |
| 33 | Packed 1.3 /m², spread 1 per 23 m²; walk 0.18 m/s; run 0.76 m/s; time 0.74/0.17/0.09; threshold 0.41 m/s | UNVERIFIABLE | Density 1.3 / 23 m², 74% motionless, and 0.41 m/s **are in the main text** and can be upgraded. Means 0.18 / 0.76 and shares 0.17 / 0.09 were not found without the SI | same |
| 34 | Mosquitofish NND 6.2 / 3.8 / 2.6 cm; switch ~6 cm; cruise 6.9 cm/s; polarisation 0.84 / 0.71 / 0.63 | UNVERIFIABLE | Full text and SI not opened; not in the abstract | https://doi.org/10.1073/pnas.1109355108 |
| 35 | 46 sheep, Kelpie, 3 trials; respond at ~70 m; cohesion settles at ~4 m | VERIFIED | UCL Discovery PDF, text and Fig. 1. Licence CC BY (no version) on the UCL record | https://discovery.ucl.ac.uk/id/eprint/1366736/ |
| 36 | Mean FID 11.46 m (SE 1.03); range 1–180 m; 12,495 / 714 / 159 | VERIFIED | Díaz 2013 PMC full text, Results | https://doi.org/10.1371/journal.pone.0064634 |
| 37 | Rural 13.34 (SE 1.05) vs urban 7.12 (SE 1.05); *t*=−13.99, df=229; 27% after mass and phylogeny | VERIFIED | Results | same |
| 38 | Mass *r*=0.603 (0.554–0.648); habitat 0.524; latitude 0.219; adj. *R*²=0.33; λ=0.472 | VERIFIED | Table 1 | same |
| 39 | Walk at normal speed, 1 m steps, from ≥30 m; FID includes height | VERIFIED | Methods: Euclidean distance including height | same |
| 40 | Australian large-bird walker FIDs (Emu 58.7 … Pacific Black Duck 38.9) | VERIFIED | Weston Appendix 1; compiled from source studies | https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf |
| 41 | Shorebird study-to-study spread (Eastern Curlew, Masked Lapwing, Hooded Plover) | VERIFIED | same appendix | same |
| 42 | Black Swan urban lake 3.6 (3.8, *n* 92); longest Australian FID 196 m (Eastern Curlew, Glover et al. 2011) | VERIFIED | Appendix 1 and text | same |
| 43 | Deer FID = −7.68 + 0.86·SD − 1.96×10⁻³·SD²; peak 87 m at SD=219 m; max alert 169 m; *n*=78 | UNVERIFIABLE | Paper exists (Crossref); text not opened (OUP 500) | https://doi.org/10.1093/beheco/arj020 |
| 44 | 69 / 20 / 8.5 / 2.5%; 4,477 species; 612 non-fossorial omitted | VERIFIED | Author PDF. Reason now complete: lack of temporal-niche data. 184 fossorial also removed | https://doi.org/10.1073/pnas.1216063110 |
| 45 | Geographic pattern; Pagel’s λ=0.947 | VERIFIED | same, Results | same |
| 46 | Use 6–7 topological neighbours (from #12) | VERIFIED | Restatement | https://arxiv.org/abs/0709.1916 |
| 47 | NND 0.7–1.5 m; 0.2 m hard-core (from #13, #8) | VERIFIED | Restatement | same |
| 48 | Speed about 7–19 m/s, typically 10–12 (from #14, #18) | VERIFIED | Restatement | https://arxiv.org/abs/0911.4393 |
| 49 | Shape 1 : 2.8 : 5.6, thin axis vertical (from #15) | VERIFIED | Restatement | https://arxiv.org/abs/0802.1667 |
| 50 | Polarisation ~0.96 (starlings) vs ≤0.09 (midges) (from #17, #29) | VERIFIED | Restatement | https://arxiv.org/abs/0911.4393 |
| 51 | 30-fish schools polarised 57% of the time; 300-fish mill 45% (from #25) | VERIFIED | Restatement; does not repeat the row-25 error | https://doi.org/10.1371/journal.pcbi.1002915 |
| 52 | Partridge: herring more spaced than saithe or cod; NND falls with size and speed | UNVERIFIABLE | Paper exists; abstract and table not opened | https://doi.org/10.1007/BF00292770 |
| 53 | Sheep grazing 0.1–0.15 m/s; packing runs up to ~1.5 m/s (from #11, #32) | VERIFIED | Restatement | https://doi.org/10.1073/pnas.1503749112 |
| 54 | Sheep alert to a dog ~70 m (from #35) | VERIFIED | Restatement | https://doi.org/10.1016/j.cub.2012.05.008 |
| 55 | Typical FID ~11 m in Europe; ~30–65 m for large Australian waterbirds/shorebirds; urban about half rural | VERIFIED | Restatement of #36–#41 | https://doi.org/10.1371/journal.pone.0064634 |
| 56 | Stankowich 2008: effect sizes *r*, not mean FIDs; e.g. fast vs slow *r*=0.21, direct vs tangential *r*=0.31 | UNVERIFIABLE | Abstract opened (meta-analysis of escape responses); Table 1 and the *r* values not opened | https://doi.org/10.1016/j.biocon.2008.06.026 |
| 57 | Pigeon delay ~0.3–0.4 s; StarDisplay 50 ms; Couzin 0.1 s step | VERIFIED | Values stand. Couzin’s “fish response latency” sentence **is** in the 2002 PDF (Partridge & Pitcher 1980) | https://arxiv.org/abs/1010.5394 |
| 58 | Surf scoter ~1.5 BL, ~2 BL/s (from #21) | VERIFIED | Restatement | https://doi.org/10.1073/pnas.1001763107 |
| 59 | Midge size ∝ *N*^{1/3}, about 20–30 cm (from #30) | VERIFIED | Restatement | https://doi.org/10.1038/srep01073 |
| 60 | Vicsek critical noise about 2.9 rad at ρ=0.4 (from #6) | VERIFIED | η=2π is the paper’s “infinite temperature” limit | https://arxiv.org/abs/cond-mat/0611743 |
| 61 | Couzin state recipe (swarm / torus / parallel) from Fig. 3, qualitative boundaries | VERIFIED | Restatement; boundaries are in the figure, not as numbers | https://doi.org/10.1006/jtbi.2002.3065 |
| 62 | Default mammal activity: nocturnal 69%, diurnal 20% (from #44) | VERIFIED | Restatement | https://doi.org/10.1073/pnas.1216063110 |
| 63 | Field flock-size range: starlings 122–4268; pigeons up to 10; sheep 46–100 | VERIFIED | Restatement | https://arxiv.org/abs/0911.4393 |
| 64 | 30 fish in 0.66×0.38 m (density of 300) did not increase milling (4 replicates) | VERIFIED | Tunstrøm Results; “one-tenth the area” is a correct computation from 2.1×1.2 m | https://doi.org/10.1371/journal.pcbi.1002915 |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Reynolds 1987 author PDF | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf | Yes (200, PDF) | Yes, ⚠ARR (ACM; author reprint). ACM doi landing 403 to scripts |
| Reynolds 1987 doi | https://doi.org/10.1145/37402.37406 | Identifier resolves; ACM 403 to scripts | ⚠ARR |
| Couzin 2002 doi | https://doi.org/10.1006/jtbi.2002.3065 | Yes (Elsevier 200 to linkinghub). Full text opened from academic PDF hosts | Yes, ⚠ARR (Elsevier) |
| Vicsek 1995 arXiv | https://arxiv.org/abs/cond-mat/0611743 | Yes (200) | Yes, ⚠ARR (APS); arXiv copy |
| Vicsek 1995 doi | https://doi.org/10.1103/PhysRevLett.75.1226 | Identifier resolves; APS 403 to scripts | ⚠ARR |
| Hemelrijk 2011 | https://doi.org/10.1371/journal.pone.0022479 · PMC3150374 | Yes (PLOS and PMC 200) | Yes, CC BY |
| Lukeman 2010 doi / author PDF | https://doi.org/10.1073/pnas.1001763107 · https://people.stfx.ca/rlukeman/PNASLukemanPub.pdf | PNAS 403 to scripts; author PDF and SI 200 | Yes, ⚠ARR (PNAS) |
| Ballerini 2008 PNAS arXiv / doi | https://arxiv.org/abs/0709.1916 · https://doi.org/10.1073/pnas.0711437105 | arXiv 200; PNAS 403 | Yes, ⚠ARR (PNAS) |
| Ballerini 2008 *Anim. Behav.* arXiv / doi | https://arxiv.org/abs/0802.1667 · https://doi.org/10.1016/j.anbehav.2008.02.004 | arXiv 200; Elsevier 200 to linkinghub | Yes, ⚠ARR (Elsevier) |
| Cavagna 2010 arXiv / doi | https://arxiv.org/abs/0911.4393 · https://doi.org/10.1073/pnas.1005766107 | arXiv 200; PNAS 403 | Yes, ⚠ARR (PNAS) |
| Nagy 2010 arXiv / doi | https://arxiv.org/abs/1010.5394 · https://doi.org/10.1038/nature08891 | arXiv 200; Nature 200 (cookies interstitial) | Yes, ⚠ARR (Nature) |
| Tunstrøm 2013 | https://doi.org/10.1371/journal.pcbi.1002915 · PMC3585391 | Yes (200) | Yes, CC BY |
| Katz 2011 doi | https://doi.org/10.1073/pnas.1107583108 | PNAS 403; full text opened from a university copy | Yes, ⚠ARR (PNAS OA option) |
| Herbert-Read 2011 doi | https://doi.org/10.1073/pnas.1109355108 | PNAS 403; abstract opened via PMC3219133 | Yes, ⚠ARR; full text not opened |
| Kelley 2013 | https://doi.org/10.1038/srep01073 · PMC3545223 | Yes (Nature 200; PMC 200) | Yes, CC BY-NC-ND 3.0 (⚠NC ⚠ND) |
| Ginelli 2015 doi / AURA PDF | https://doi.org/10.1073/pnas.1503749112 · https://aura.abdn.ac.uk/bitstream/2164/5128/1/PNAS_2015_Ginelli_12729_34.pdf | PNAS 403; AURA 200. SI 403 | Yes, ⚠ARR (PNAS OA option) |
| King 2012 UCL / doi | https://discovery.ucl.ac.uk/id/eprint/1366736/ · https://doi.org/10.1016/j.cub.2012.05.008 | Both 200 | Yes, CC BY with no version on the UCL record (C9 is right; Grok’s “3.0” was not used) |
| Díaz 2013 | https://doi.org/10.1371/journal.pone.0064634 · PMC3665823 | Yes (200) | Yes, CC BY 4.0 |
| Weston 2012 author PDF / doi | https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf · https://doi.org/10.1071/MU12026 | Author PDF 200; T&F/CSIRO 403 | Yes, ⚠ARR (CSIRO) |
| Bennie 2014 doi / author PDF | https://doi.org/10.1073/pnas.1216063110 · https://kevingaston.com/wp-content/uploads/2017/05/Bennie-et-al-14-Biogeography-of-time-partitioning-in-mammals.pdf | PNAS 403; author PDF 200; PMC landing 200 (full text was abstract-sized in PMC XML) | Yes, ⚠ARR (PNAS) |
| Stankowich & Coss 2006 doi | https://doi.org/10.1093/beheco/arj020 | Identifier in Crossref; OUP 500 to this crawler | ⚠ARR not checked in the text (not read) |
| Stankowich 2008 doi | https://doi.org/10.1016/j.biocon.2008.06.026 | Yes (Elsevier 200 to linkinghub); full text not opened | ⚠ARR not checked in the text (not read) |
| Partridge 1980 doi | https://doi.org/10.1007/BF00292770 | Identifier in Crossref; Springer 500 to this crawler | ⚠ARR not checked in the text (not read) |

C9’s file-level access date (all URLs accessed 2026-11-17) stands in for a per-row access date. The source list at the bottom of C9 is complete for the works cited; the three Grok-lead papers that were not read are listed as such.

## Gaps and disagreements

C9’s “Not found” and “Sources disagree” lists are fair. I confirmed the Ballerini vs Cavagna event-count/speed disagreements on the same labels, the Tunstrøm 15.5 vs 15.6 cm inconsistency, and the Weston same-species FID spread. Two gaps can now be filled without changing C9’s numbers: Lukeman SI Table S1 zone radii (*r*_rep = 1.45 BL, *r*_al = 3 BL, *r*_att = 5 BL), and Bennie’s reason for omitting 612 species (lack of temporal-niche data). Deer/wildebeest/bison NND, ungulate mean FIDs in metres, and the Partridge NND table remain unfound in this pass as well.

## Grok section

A live grok-4.6 pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file …`) ran **17 November 2026, 11:20:59–11:35:46 AEDT** (14 min 47 s, exit 0), under the 25-minute cap. Live search was on. The prompt is `/workspace/lv-scratch/c9/grok_prompt.txt`; the output is `/workspace/lv-scratch/c9/grok/grok_out.txt` (also copied to `grok_c9_verify.txt`). stderr recorded one mid-run compaction. No C9 number in this verify file was taken from Grok alone; every figure I marked VERIFIED or FIX was opened in a primary source first.

**Where Grok agreed with sources I opened:** Reynolds; Couzin Table 1 / Fig. 3 (including the 0.1 s “fish response latency” sentence and the unused 5000-step / 8.3 min remark); Vicsek arXiv numbers and the radian reading of η; StarDisplay Table 1; Lukeman weights, spacing, speed, and SI zone radii; Ballerini and Cavagna tables, including the 16-05 / 32-06 / 31-01 disagreements and the recomputed mean speeds 10.59 (SD 2.26) and 11.59 (SD 2.48); Nagy delays; Tunstrøm set-up, thresholds, polar/mill fractions, 15.5 vs 15.6 cm, and the density-control experiment; Katz rules; Kelley licence and numbers; Ginelli main-text parameters, 1.3 / 23 m² densities, 74% motionless and the 0.41 m/s threshold; King 70 m and ~4 m; Díaz statistics and the paper’s own latitude-CI inconsistency; Weston Appendix 1; Bennie percentages, 4,477 / 612, and λ = 0.947.

**Where I did not follow Grok:**

- **Row 25 (Tunstrøm).** Grok also wrote “swarm/transition 0.09–0.11” and called the item a MATCH. That repeats C9’s error. The Fig. 4A text I opened splits swarm (0.09–0.11) from the transition region (0.31–0.44). The FIX above stands.
- **Row 34 (Herbert-Read numbers).** Grok reported a MATCH from an IMT Lucca PDF and an SI extract (NND 6.2 / 3.8 / 2.6 cm; switch ~6 cm; cruise 6.9 cm/s in Fig. 2D; polarisation 0.84 / 0.71 / 0.63 in the SI, not Fig. 2D). The same IMT Lucca URL returned 403 to me, so I did not open that file. Those figures remain **UNVERIFIABLE** here; they are a Grok lead only.
- **Rows 43, 52 and 56.** Grok reported matches from a ResearchGate/SciSpace OCR of Stankowich & Coss 2006, a ResearchGate snippet of Stankowich 2008 Table 1 (*r* = 0.21 fast vs slow, *r* = 0.31 direct vs indirect), and an ADS/Springer abstract of Partridge 1980. I did not open those pages (OUP/Springer were 500 or a bot-challenge to me). I have not used those numbers. The three rows stay **UNVERIFIABLE**.
- **Ginelli “running about 1 m/s.”** Grok called this a mismatch because the main text’s Fig. S4A shoulder is ≈ 0.66 m/s. C9 already labelled it an order-of-magnitude reading (max recorded speed is about 1.5 m/s). I have not turned that into a number FIX.
- **King licence version.** Grok read “CC BY 3.0” on the Cell HTML. The UCL record I opened says CC BY with no version, which is what C9 used. A versioned Cell grant is a licence clarification, not a data fix.
- **Ginelli “*d*_R = 31.6 m (= 5 *d*_S).”** Grok treated the “= 5 *d*_S” gloss as a match. I did not find that phrase in the main text (5 × 6.3 = 31.5). The two values themselves match.

**Grok status:** completed successfully, live search on, sourced output written. Useful as a second pair of eyes on the papers I also opened; not used as the sole source of any number in the row table.


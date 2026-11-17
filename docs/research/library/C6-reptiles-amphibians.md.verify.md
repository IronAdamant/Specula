# Verification: C6-reptiles-amphibians.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/C6-reptiles-amphibians.md`
Output path (this file only): `/workspace/specula-research/C6-reptiles-amphibians.md.verify.md`

Private adversarial source-check. No number was taken from Grok, a search snippet, or a pirate mirror. Every figure below was opened in a publisher page, PMC/Europe PMC full text, Crossref metadata, or a legal author/institution PDF.

## Summary

- **107 data rows** checked (sections 1–6: 32 + 15 + 9 + 15 + 20 + 16).
- **93 VERIFIED.** Almost every C6 “Verified” / “Verified (abstract / excerpt / as quoted / computed)” number matches the opened source, with those units and that population.
- **1 FIX:** row 2.8, Hutchinson et al. 2019 RSF formula (the Table 1/2 RSF *values* in the same row are correct).
- **13 UNVERIFIABLE:** the remaining Unverified JEB/Wiley/CUP rows (Fieler & Jayne, McElroy et al., Chen et al., Ritter, Reilly & Elias, Renous et al., Livingston et al., Jayes & Alexander, Rivera et al., Marsh & John-Alder). Richards et al. 2017 (row 6.16), which C6 had marked Unverified, was opened on the official JEB HTML in this pass and is now VERIFIED. Official JEB/Wiley/CUP full texts were 403 or otherwise closed to this pass. I agree with the researcher and did not invent substitutes.
- **Rows skipped:** none of the 107.
- **Dead links:** no working citation used for a verified number was a dead DOI. `10.1242/jeb.144436` is a 404 (Grok’s earlier mis-ID of Porro 2017; C6 already rejected it). Official JEB, Wiley, and Royal Society landings often return 403/500 to scripted fetches; they still resolve at the DOI. PubMed HTML was a bot challenge (HTTP 203); NCBI eutils and Europe PMC were used instead.
- **Worst problem:** the RSF formula in row 2.8 is not the paper’s formula and is not dimensionless. Everything else that could be opened is clean, with only licence-metadata nuance on Mazouchova 2010 (close check 1) and a few wording/status notes that are not number errors.

One-line summary: C6 is solid; correct the Hutchinson RSF definition, keep the 14 Unverified rows Unverified, and treat Mazouchova 2010 as originally OA CC BY 2.0 (PMC2880067) with Crossref now listing CC BY 4.0.

## File snapshot

At the start of this check the file was copied to `/tmp/lv17c6/C6-reptiles-amphibians.md`. The first snapshot was **43,470 bytes**, mtime **17 November 2026, 09:36:03 AEDT**, 253 lines.

A box restart at about 09:59 AEDT restored `/workspace/specula-research` and wiped `/tmp`. After the restore the workspace file was still **43,470 bytes** and 253 lines, but its mtime became **09:59:59 AEDT**. Per parent instruction the restored file was treated as the baseline, compared by **size and content (not mtime)**, and recopied to `/tmp/lv17c6/` at 10:00:48 AEDT.

Identity of the restored file versus the 09:36:03 baseline: same size (43,470 bytes), same line count (253), same opening header and closing notes, md5 `10593f42d4b7e094ac181926877a7f5a`. Immediately before this verify file was written, `cmp` of the workspace file against the post-restore snapshot was identical. **No content change; no row re-check was required.** Access-date-only changes: none.

## Close checks

### 1. Mazouchova, Gravish, Savu and Goldman 2010 licence

The cited paper is *Biology Letters* 6:398–401, doi `10.1098/rsbl.2009.1041`, “Utilization of granular solidification during terrestrial locomotion of hatchling sea turtles.” Opened: Crossref work record; PMC2880067 (this *is* the Biol. Lett. article, not a different Interface paper); Goldman-lab / journal PDF (local copy 257,897 bytes; C6 URL `https://goldmanlab.gatech.edu/pages/publications/pdf/Biol.%20Lett.-2010-Mazouchova-rsbl.2009.1041.pdf`). Official Royal Society HTML/PDF was 403/500 to scripted clients. Key pages of the PDF were also rendered and read as images because OCR had turned degree signs into “8”.

- Crossref lists a CC BY 4.0 VOR+TDM licence with start date 2010-02-10 and delay 0. That start date is anachronistic: CC BY 4.0 did not exist until November 2013.
- The PMC2880067 deposit is licensed **CC BY 2.0**, with copyright statement “© 2010 The Royal Society,” which historically fits a 2010 Royal Society OA paper. C6 did not list the PMC deposit.
- The journal PDF prints only “This journal is © 2010 The Royal Society.” There is no Creative Commons line on the PDF. A copyright line is compatible with CC BY (copyright is asserted; the licence grants reuse). It is not ARR in the “all rights reserved, no reuse” sense.
- C6’s note (“treat as CC BY per Crossref, check before reuse beyond citing numbers”) is cautious but slightly overstates CC BY 4.0 as the original licence. The better contemporary deposit record is PMC CC BY 2.0; Crossref now states 4.0. Not a data-row FIX.

All numbers in rows 4.11–4.13 were confirmed from the PDF and from the page images (N = 26 hatchling *Caretta caretta*, 18 sand / 8 hard ground; stance DF 0.66 ± 0.05; body lifted 2.2 ± 0.9 mm; stride 4.0 ± 1.9 cm sand, 4.7 ± 2.9 cm hard ground; mean speed = stride × frequency; average speed on sand 28% lower, same maxima; shoulder 111 ± 17° sand, 114 ± 6° sandpaper; insertion depth 0.76 ± 0.13 cm).

### 2. Moreno-Rueda et al. 2020 — is it really CC BY-NC?

Yes. Crossref licence URL is CC BY-NC 4.0. PMC7319472 has `license-type="cc-by-nc"`. C6’s ⚠NC flag is correct. Rows 6.13–6.14: 6.13 is Herrel 2014 (verified separately); 6.14 is Moreno-Rueda, Requena-Blanco, Zamora-Camacho, Comas and Pascual 2020, *Curr. Zool.* 66:417–424, doi `10.1093/cz/zoz062`. n = 712, SVL 2.2–7.9 cm, jump distance levels off with size, relative hind-limb length explains 2.4% of variance. C6’s wording “longer relative femur and tibiofibula” is slightly tighter than the abstract’s “hind limbs”; both segments were measured. Not a FIX.

### 3. Schmidt 2016, Zani 2005, and Cieri 2020 opened directly

**Schmidt, Mehlhorn and Fischer 2016**, *J. Exp. Biol.* 219:2693, doi `10.1242/jeb.137059`, PubMed 27340203. Official JEB full-text HTML was opened via WebFetch (scripted PDF URLs 403). © 2016 Company of Biologists (⚠ARR, CoB user licence 1.1). Three adult *Testudo hermanni* (2♀, 1♂), 954–2318 g, biplanar X-ray. Lateral-sequence diagonal couplets in all trials. Duty factor 56.8–87.1%; individual means 69.1 ± 8.6, 77.9 ± 3.8, 77.4 ± 6.4%. Abstract: more than 30% / nearly 40% / roughly 50% as quoted for girdle contribution. Rows 4.5–4.6 verified; 4.5 can be upgraded from “publisher excerpt” to official full text. 4.6 remains fairly described as abstract-plus-full-text. No number error.

**Zani, Gottschall and Kram 2005**, *J. Exp. Biol.* 208:1489–1494, doi `10.1242/jeb.01554`, PubMed 15802673. Official JEB full-text HTML opened via WebFetch, plus the PubMed/Europe PMC / Crossref abstract. Five adult *Chelonoidis (Geochelone) elephantopus*, mean 142 kg (103–196). Speed 0.16 ± 0.052 m/s (mean ± 1 s.d.). Recovery 29.8 ± 3.77% (13–52%). KE 8.1 ± 3.98 vs GPE 22.7 ± 8.04 J/stride. Work 0.41 ± 0.092 J kg⁻¹ m⁻¹. Results: plastron about 10–20 cm. Average of 10 good trials per animal (Methods tried 20). LS diagonal couplets. Duty factor 80.5 ± 0.58% overall; hind 81.9 ± 0.71 vs fore 79.1 ± 0.69 (mean ± 1 s.d.). Rows 4.7–4.9 verified; 4.9 can be upgraded from excerpt to official full text. The official HTML also prints: “Their gait was a lateral sequence diagonal couplet (Fig. 3; Hildebrand, 1985).” (A Grok excerpt missed this sentence and only quoted the preceding Jayes & Alexander 1980 comparison; the phrase is in the page I opened.) Abstract/first Results give external work 0.41±0.092 J kg⁻¹ m⁻¹, which is what C6 quotes; a later Discussion sentence prints 0.42±0.083. Not a FIX.

**Cieri, Hatch, Capano and Brainerd 2020**, *Sci. Rep.* 10:7739, doi `10.1038/s41598-020-64140-y`, PMC7217971. CC BY 4.0 on the paper and on Crossref. Official article / PMC XML opened. Results ± s.e. for summed bucket + pump + caliper exactly as claimed: savannah monitors 8.1 ± 3.3, 14.8 ± 5.8, 12.7 ± 3.6; tegus 13 ± 3.4, 13.1 ± 4.9, 16.8 ± 5.5 (units: degrees of rib rotation, n = 3 *Varanus exanthematicus* and n = 3 *Salvator merianae*). Row 1.32 verified; C6’s “Verified (publisher excerpt)” can be upgraded to official full text. No number error.

### 4. Moo 2017 54° vs Porro 2017 Table 4 mean 26° (*Lithobates / Rana pipiens*)

Both numbers are real; they are not the same comparison.

**Moo, Peterson, Leonard, Kaya and Herzog 2017**, *PLoS ONE* 12:e0173415, doi `10.1371/journal.pone.0173415`, official PDF, CC BY 4.0. *Lithobates pipiens*. Data from **two frogs** (mean SVL 6.4 cm) and **15 jumps** in total (the paper also states ten frogs were studied and only these met the near-maximal inclusion cut). Take-off angle **54 ± 1°** (s.e.m.) and jump distance **59 ± 1 cm** (> 9 body lengths). Peak plantaris force 3.3 ± 0.2 N; the paper itself says “~13 times of body weight.” Rows 6.4–6.5 verified.

**Porro, Collings, Eberhard, Chadwick and Richards 2017**, *J. Exp. Biol.* 220:1882–1893. Printed and resolving DOI is `10.1242/jeb.155416` (UCL author PDF HTTP 200). `10.1242/jeb.144436` is a 404 and was already rejected in C6. Companion paper Richards, Porro and Collings 2017 is `10.1242/jeb.144279`. Four adult *Kassina maculata*, 28.4 ± 3.7 g, SVL 60.0 ± 1.2 mm, 50 jumps (13 low 0–20°, 24 intermediate 21–49°, 13 high 50–70°). All-trials means: 1.36 ± 0.28 m/s (22.6 ± 4.6 SVL/s), max 2.02 (33.1), min 0.67; angle 34 ± 19°, range 0.3–69°; acceleration 35.6 ± 12.1 m/s² (max 79.5); distance 0.19 ± 0.09 m (max 0.34); force/BW 3.03 ± 0.87. Peak force about 0.06 s and peak velocity about 0.01 s before take-off. Rows 6.6–6.10 verified from the author PDF. ⚠ARR, CoB user licence 1.1.

Table 4 is a **compiled literature table**, not *K. maculata* data (C6 already marks 6.11–6.12 “Verified (as quoted)”). For *Rana pipiens* it quotes mean take-off angle **26°** (range 16–42) and peak velocity 56 SVL/s, citing sources 10–12: Hirano and Rome 1984; Lutz and Rome 1996a; Astley and Roberts 2014. Other compiled means also match C6: *R. catesbeiana* 42° (about 10–60), vel 15 SVL/s; *Bufo* 31° (14–51); *R. esculenta* 40°; *R. temporaria* 34°; hylids 40°, vel 45–115 SVL/s; *Bombina* 31–43°; *Litoria* 52°.

The 54° vs 26° difference is therefore near-maximal selected jumps in two frogs (Moo) versus a compiled literature mean (Porro Table 4). Conditions are not the same. C6 already flags this in “Sources disagree #2.” Richards, Porro and Collings 2017 (row 6.16), doi `10.1242/jeb.144279`, was opened on the official JEB HTML (`https://journals.biologists.com/jeb/article/220/10/1894/17784/Kinematic-control-of-extreme-jump-angles-in-the`). Results: “The average minimum jump angle (minimum angle averaged across all frogs) was 7.61±6.68 deg and the maximum was 58.43±8.24 deg … peak resultant velocities ranged from 16.49±3.64 to 31.47±3.27 body lengths s⁻¹ (mean±s.d., N=4 frogs).” Take-off angles ranged “from 0.27 deg to nearly 70 deg.” ⚠ARR, CoB. C6’s “per frog” wording is a fair gloss of “minimum angle averaged across all frogs.” This row is VERIFIED (C6 had it Unverified because only a search-index excerpt had been opened).

### 5. Iijima, Kubo and Kobayashi 2018 limb proportions are size-corrected, not raw

PMC5882705, *R. Soc. Open Sci.* 5:171774, doi `10.1098/rsos.171774`, CC BY 4.0 on the paper (Crossref carries a Royal Society data-sharing/TDM URL as well as, or instead of, a clearly labelled article-licence field in some views; the PMC/paper licence is CC BY 4.0 and C6 is right). Sample: 110 individuals / 20 species, juveniles to adults, femur ≥ 50 mm; trunk (C3–D15) for 55 individuals / 17 species. Size-normalisation follows Elliott et al. 1995; raw measurements are in ESM Table S1 (not downloaded; not needed for the stated ranges). All listed subfamily ranges match Results / Figs 4–5. PGLS adjusted R² = 0.498, p = 0.001, 17 species. “Slender-snouted” taxa having relatively long stylopodia and short overall limbs is a direct match. Rows 3.1–3.6 verified. Row 3.6’s note that the values are allometrically size-normalised, not raw, is correct.

### 6. Fire salamander is Pierce et al. 2020; Karakasiliotis and Porro DOIs opened

**Pierce, Lamas, Pelligand, Schilling and Hutchinson 2020**, *Integr. Org. Biol.* 2:obaa015, doi `10.1093/iob/obaa015`, PMC7671131, CC BY 4.0, *is* the fire-salamander paper. `10.1093/iob/obaa017` is Usherwood 2020, “The Possibility of Zero Limb-Work Gaits…”, PMC7545857 — not a Butcher paper, and not this paper. Table 1 means are **s.e.m., not SD**, and match C6 exactly (fore n = 5: mass 24.4 (4.0) g, TL 146.0 (7.8) mm, SVL 87.6 (8.0) mm, stride 0.84 (0.06) s, DF 74.50 (1.03)%; hind n = 5: 22.2 (1.55) g, 156.4 (5.4) mm, 96.6 (4.2) mm, 1.04 (0.30) s, 73.82 (1.35)%). Lateral-sequence, diagonal-couplet footfall and standing-wave axial pattern confirmed. Table 4 compiled maxima: thigh protraction 40–60°, retraction 114–145°, knee extension 145–173°, confirmed across the six datasets. Discussion quotes also match: *Triturus* DF about 75%; *Dicamptodon* hind stride 0.97–1.14 s, DF 68–72%; *Taricha* 77%; *A. tigrinum* 0.71 / 0.76 s, DF 74 / 80%; *Pleurodeles* 1.52 s, DF about 77%. Rows 5.16–5.19 verified.

**Karakasiliotis, Schilling, Cabelguen and Ijspeert 2013**, doi `10.1007/s00422-012-0540-4` is correct (*Biol. Cybern.* 107:529–544; Springer ARR; EPFL repository copy, no open licence stated). `10.1007/s00422-013-0563-5` is Schilling et al. Walknet, CC BY 2.0 — a different paper. EPFL PDF opened. Numbers: 0.49 ± 0.12 SVL/s, cycle 1.52 ± 0.32 s, LS walk, stance about 77%, two animals, N = 23 cycles; Fig. 3 J1 14.9°(2.3) through J13 7.5°(0.9); standing wave almost perfect in J9–J15 and J19–J26, anti-phase. All match. Rows 5.13–5.15 verified.

Porro DOI: see close check 4. Printed / resolving DOI is `10.1242/jeb.155416`.

## The one FIX

**Row 2.8 — Hutchinson, Felkler, Houston, Chang, Brueggen, Kledzik and Vliet 2019, RSF formula.**

Paper: *Sci. Rep.* 9:19302, doi `10.1038/s41598-019-55768-6`, PMC6917812, CC BY 4.0.

C6 writes the quantity as `RSF = SF·(g·h)^−0.5`, h = extended hindlimb length, and gives the Table 1/2 values 0.30 ± 0.03 / 0.30 ± 0.02 / 0.41 ± 0.02.

The paper’s Methods, display formula 1 (MathML), is

`RSF = SF · (g · h^{−1})^{−0.5}`

which is `SF · √(h/g)`. That *is* dimensionless (g = 9.81 m s⁻², h in metres, SF in s⁻¹). C6’s `SF · (g·h)^{−0.5}` has units of s/m and is not the paper’s formula.

Corrected value: **RSF = SF·(g·h^{−1})^{−0.5} = SF·√(h/g)**. The numeric RSF values in the same row match Table 1 and do not need changing.

Source: https://www.ncbi.nlm.nih.gov/pmc/articles/PMC6917812/ and https://doi.org/10.1038/s41598-019-55768-6.

By contrast, row 2.12’s û = u·(g·h)^{−0.5} *is* the paper’s display formula 2 and is VERIFIED.

## Other notes (not FIXes)

- Cieri 1.32, Schmidt 4.5, and Zani 4.9 can be upgraded from “Verified (publisher excerpt)” to official full text. Row 6.16 can be upgraded from Unverified to Verified (official JEB HTML).
- Zani 2005 external work: C6 correctly quotes the Abstract / first Results value 0.41±0.092 J kg⁻¹ m⁻¹; the Discussion later prints 0.42±0.083. Flag both printed values.
- Porro 2017 husbandry (28.4±3.7 g, SVL 60.0±1.2 mm) sits in Methods (and Table S1), not Table 1 (which is force/velocity). C6 already cites Methods for row 6.6, so this is not a C6 location error. “50 jumps” is also called “50 trials” in the paper.
- Vega and Ashley-Ross 2021 (row 5.20): C6 omitted n = 3 *Ambystoma tigrinum*; the stated SVL, speed ranges, 86 trials, and standing- vs travelling-wave statements all stand (PMC8545788: “All video trials (n = 86) were used”).
- Walker 1971 pagination: C6 and PubMed use 195–214; Crossref lists 195–213. Abstract content was opened in Crossref’s official abstract record (Wiley HTML timed out / 403). Speed 28–51 mm/s, LS diagonal couplet, glenoids more than twice as far apart as the acetabula, all confirmed. Wiley VOR, ⚠ARR. Not a FIX.
- Ashley-Ross 1994a (row 5.11): three-foot support is the arithmetic remainder after the stated two-foot and four-foot figures (24.6% walk, 3.7% trot). C6 already marks this “Verified (computed).” The paper describes a shorter transition rather than always naming “3 feet.”
- Feiner, Jackson, Van der Cruyssen and Uller 2021 (row 3.7, PMC8220270): “For total fore- and hindlimb length, nearly all slopes were significantly larger than 1” confirms C6’s “limb-vs-SVL slopes mostly > 1.” r = 0.616 is the correlation of relative hindlimb length (p = 0.014); relative forelimb r = 0.361 (p = 0.187) is in the same sentence and is not in C6. Not a number error in the stated figure.

## Row table

| Row | Claim (short) | Status | Note | URL |
|---|---|---|---|---|
| 1.1–1.12 | Druelle, Goyens, Vasilopoulou-Kampitsi and Aerts 2019, *Sci. Rep.*: *A. boskianus* (7 adult ♂, 61 strides) and *P. muralis* (7 adult ♂, 51 strides) Table 1 speeds, frequencies, duty factors, stride lengths on flat and 25 mm-hemisphere terrain; CoM heights 17.15 ± 3.66 vs 9.21 ± 3.03 mm; qualitative posture | VERIFIED | PMC6856151 full text, Table 1 and Results / Fig. 4. CC BY 4.0. All listed means match. | https://pmc.ncbi.nlm.nih.gov/articles/PMC6856151/ |
| 1.13–1.21 | Jagnandan and Higham 2017, *Sci. Rep.*: *Eublepharis macularius* n = 10; speed 0.59–3.36 SVL/s; intact-tail Table 1 duty factor, stride, stance, humerus/elbow/wrist/femur/knee/ankle values (means + residuals ± s.e.m.); femur max depression falls to 23.01 ± 1.44 (restricted) and 20.79 ± 1.30 (autotomised) | VERIFIED | PMC5589804 full text, Table 1. **CC BY 4.0** (not a JEB paper). | https://pmc.ncbi.nlm.nih.gov/articles/PMC5589804/ |
| 1.22–1.26 | Jayne and Irschick 1999: 4 *Dipsosaurus dorsalis*, 62 ± 3.2 g, SVL 12.2 ± 0.1 cm (s.e.m.), 219 strides, 50–250 cm/s, ±30°, 35–40 °C; walk and run on all inclines; DF ≈ 50% between 100 and 150 cm/s on the level; pelvic rotation 36–52° (greater uphill, no speed effect); femur retraction max 38–55°, excursion 91–111° rises with speed; depression excursions 35.8 ± 0.8 / 41.2 ± 2.1 / 43.8 ± 2.5 (s.e.m.); downhill max 17–21°, min −14 to −21° | VERIFIED | Author-institution PDF opened. ⚠ARR, CoB (doi `10.1242/jeb.202.2.143`). DF ≈ 50% is in the Discussion in a level-locomotion context (p. 154). | https://webcentral.uc.edu/eProf/media/attachment/eprofmediafile_151.pdf |
| 1.27–1.28 | Fieler and Jayne 1998 duty-factor / stride / frequency regressions | UNVERIFIABLE | Official JEB full text not obtained (403). Agrees with C6 Unverified. C6 correctly refused a third-party host copy. | https://doi.org/10.1242/jeb.201.4.609 |
| 1.29 | McElroy, Hickey and Reilly 2008 multi-species DF / phase (e.g. *V. exanthematicus*) | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.015503 |
| 1.30 | Chen et al. 2006 *Hemidactylus garnotii* trot DF | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.01979 |
| 1.31 | Ritter 1992 axial wave type | UNVERIFIABLE | Official JEB full text not obtained; abstract not re-read. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.173.1.1 |
| 1.32 | Cieri et al. 2020 rib rotation, savannah vs tegu, bucket + pump + caliper, ± s.e. | VERIFIED | PMC7217971 / official *Sci. Rep.* text. Upgrade from “publisher excerpt.” CC BY 4.0. doi `10.1038/s41598-020-64140-y`. | https://www.nature.com/articles/s41598-020-64140-y · https://www.ncbi.nlm.nih.gov/pmc/articles/PMC7217971/ |
| 2.1–2.7, 2.9–2.12 | Hutchinson et al. 2019: 42 individuals, 15 species, 0.5–43.2 kg, ≤ about 2 m TL, 184 trials, 0.15–4.4 m/s; 42 asymmetrical trials from 17 individuals, all Crocodyloidea; no alligatoroid galloped or bounded; *Caiman crocodilus* 3.7 m/s trot; *C. acutus* 4.4 m/s diagonal-sequence running gait; young *A. mississippiensis* 2.0 m/s, DF 0.50; *Mecistops cataphractus* rotary gallop 2.36, bound 2.98; *Osteolaemus* bound 2.98 m/s; aerial phases in 25 individuals of 13 species; Table 1/2 DF values; mass-effect p = 0.026, 0.013; literature ceiling 4.7 m/s *C. johnstoni*, nothing above 5, Zug 18 m/s is an error for about 1.8; gait codes 1–7; û = u·(g·h)^{−0.5} | VERIFIED | PMC6917812, *Sci. Rep.* 9:19302, doi `10.1038/s41598-019-55768-6`, CC BY 4.0. Arithmetic and table values re-derived. | https://pmc.ncbi.nlm.nih.gov/articles/PMC6917812/ |
| 2.8 | RSF formula plus Table 1 values 0.30 ± 0.03 / 0.30 ± 0.02 / 0.41 ± 0.02 | FIX | **Correct formula: RSF = SF·(g·h^{−1})^{−0.5} = SF·√(h/g)** (display formula 1). C6’s `SF·(g·h)^{−0.5}` is not dimensionless and is not the paper’s TeX. Table values match. | https://pmc.ncbi.nlm.nih.gov/articles/PMC6917812/ |
| 2.13–2.14 | Reilly and Elias 1998 sprawl vs high-walk DF and femoral adduction | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.201.18.2559 |
| 2.15 | Renous et al. 2002 juvenile bound / min DF about 0.2 | UNVERIFIABLE | Official full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1017/S0952836902000353 |
| 3.1–3.6 | Iijima 2018 size-corrected subfamily limb-proportion ranges; n = 110 / 20 species; trunk n = 55 / 17; PGLS adj. R² 0.498, p = 0.001; slender-snouted statement; values are Elliott et al. 1995 size-normalised, not raw | VERIFIED | PMC5882705, CC BY 4.0. | https://pmc.ncbi.nlm.nih.gov/articles/PMC5882705/ |
| 3.7 | Feiner et al. 2021: 374 embryos/hatchlings of 15 *Anolis* species; 693 adults of 267 species (2.60 ± 1.58 per species); r = 0.616; limb-vs-SVL slopes mostly > 1 | VERIFIED | PMC8220270, *Proc. R. Soc. B* 288:20210226, doi `10.1098/rspb.2021.0226`, CC BY 4.0. Paper: “nearly all slopes were significantly larger than 1.” r = 0.616 is relative hindlimb length. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8220270/ |
| 3.8 | Fieler and Jayne 1998 hindlimb segment ratios | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. C6 correctly notes that SVL and mass are already in Jayne & Irschick 1999 (1.22). | https://doi.org/10.1242/jeb.201.4.609 |
| 3.9 | Livingston et al. 2009 crocodylian limb growth | UNVERIFIABLE | Official full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1002/ar.20912 |
| 4.1–4.4 | Ewart, Tickle, Nudds, Sellers, Crossley and Codd 2022: *T. graeca*, N = 5 captive-born 2-yr-old ♀, Tb 34.6 ± 0.4 °C; speed 1.2–6.0 m/min; DF 0.78 ± 0.01, no speed effect; stride length/frequency rise with speed and mass; mean CoT 6.93 J kg⁻¹ m⁻¹, minimum at 4–6 m/min | VERIFIED | PMC9312080, *Biology* 11:1052, doi `10.3390/biology11071052`, CC BY 4.0. | https://pmc.ncbi.nlm.nih.gov/articles/PMC9312080/ |
| 4.5–4.6 | Schmidt 2016 *T. hermanni*: LS diagonal couplets in all trials; DF 56.8–87.1% and the three individual means; girdle contribution > 30% / about 40% / about 50% | VERIFIED | Official JEB HTML (WebFetch), doi `10.1242/jeb.137059`. ⚠ARR, CoB. 4.5 upgrade from excerpt. | https://doi.org/10.1242/jeb.137059 · https://pubmed.ncbi.nlm.nih.gov/27340203/ |
| 4.7–4.9 | Zani 2005 *G. elephantopus*: n = 5, 142 kg (103–196); 0.16 ± 0.052 m/s; recovery 29.8 ± 3.77% (13–52%); KE / GPE / work; plastron 10–20 cm; about 10 good trials; LS diagonal couplets; DF 80.5 ± 0.58%, hind 81.9 ± 0.71, fore 79.1 ± 0.69 | VERIFIED | Official JEB HTML (WebFetch) + abstract, doi `10.1242/jeb.01554`. ⚠ARR, CoB. 4.9 upgrade from excerpt. | https://doi.org/10.1242/jeb.01554 · https://pubmed.ncbi.nlm.nih.gov/15802673/ |
| 4.10 | Walker 1971 *Chrysemys picta marginata*: 28–51 mm/s, LS diagonal couplet, glenoids > 2× acetabula | VERIFIED | Crossref official abstract of `10.1002/jmor.1051340205`. Pagination 195–214 (PubMed / C6) vs 195–213 (Crossref). Wiley VOR, ⚠ARR. | https://doi.org/10.1002/jmor.1051340205 |
| 4.11–4.13 | Mazouchova 2010 hatchling *C. caretta*: diagonal gait, DF 0.66 ± 0.05, body lifted 2.2 ± 0.9 mm; stride 4.0 ± 1.9 / 4.7 ± 2.9 cm; ⟨v⟩ = stride × frequency; 28% lower average speed on sand, same maxima; shoulder 111 ± 17° sand, 114 ± 6° sandpaper; insertion 0.76 ± 0.13 cm | VERIFIED | Journal PDF + page images (OCR had turned ° into 8). Licence: PMC2880067 CC BY 2.0; Crossref now CC BY 4.0; PDF © 2010 The Royal Society (close check 1). | https://doi.org/10.1098/rsbl.2009.1041 · https://www.ncbi.nlm.nih.gov/pmc/articles/PMC2880067/ |
| 4.14 | Jayes and Alexander 1980 tortoise DF | UNVERIFIABLE | Official full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1111/j.1469-7998.1980.tb01464.x |
| 4.15 | Rivera, Wyneken and Blob 2011 *Caretta* swimming kinematics | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.057364 |
| 5.1–5.12 | Ashley-Ross 1994a *Dicamptodon tenebrosus*: 5 animals, SVLs as listed, N = 15 strides per gait; walk 0.77 vs trot 2.90 SVL/s; Table 1 stride, cycle, contact, pelvic, pelvis–femur, knee, trunk angles; limb phase 43% / 50%; support 2 feet 40.6 / 91.5%, 4 feet 34.8 / 4.8% (three-foot remainder); walk standing wave, trot mixed | VERIFIED | Author-hosted PDF. ⚠ARR, CoB, doi `10.1242/jeb.193.1.255`. Remainder correctly computed (row 5.11). | https://users.wfu.edu/rossma/Ashley-Ross%20JEB%201994A.pdf |
| 5.13–5.15 | Karakasiliotis 2013 *Pleurodeles*: 0.49 ± 0.12 SVL/s, cycle 1.52 ± 0.32 s, LS walk, stance about 77%; Fig. 3 joint amplitudes; standing wave / anti-phase in J9–J15 and J19–J26 | VERIFIED | EPFL repository PDF. DOI `10.1007/s00422-012-0540-4` is this paper (Springer ARR). `…0563-5` is Schilling Walknet, a different paper. | https://doi.org/10.1007/s00422-012-0540-4 |
| 5.16–5.19 | Pierce 2020 fire salamander: Table 1 s.e.m. (not SD); LS diagonal-couplet + standing wave; Discussion quotes for *Triturus*, *Dicamptodon*, *Taricha*, *A. tigrinum*, *Pleurodeles*; Table 4 compiled maxima 40–60 / 114–145 / 145–173° | VERIFIED | PMC7671131, doi `10.1093/iob/obaa015`, CC BY 4.0. `obaa017` is Usherwood 2020, not this paper. | https://pmc.ncbi.nlm.nih.gov/articles/PMC7671131/ |
| 5.20 | Vega and Ashley-Ross 2021 *A. tigrinum*: SVL 9–14.5 cm; unrestricted 0.02–0.13 m/s, restricted 0.02–0.06 m/s; 86 trials; walking standing wave, swimming travelling wave | VERIFIED | PMC8545788, *Integr. Org. Biol.* 3:obab029, doi `10.1093/iob/obab029`, CC BY 4.0. C6 omitted n = 3; numbers stand. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8545788/ |
| 6.1–6.3 | Astley and Roberts 2012: take-off velocity 161 ± 30 (SD); implied distance at 45° 52.8 ± 11.4; plantaris fascicles shorten 7.8% (54% of total strain) before the joint moves; total fascicle shortening 14.6 ± 5.6% | VERIFIED | Author-lab PDF. doi `10.1098/rsbl.2011.0982`. ⚠ARR / bronze (Crossref data-sharing only, not CC BY). | author-lab PDF of https://doi.org/10.1098/rsbl.2011.0982 |
| 6.4–6.5 | Moo 2017 *L. pipiens*: 54 ± 1° (s.e.m.), 59 ± 1 cm; 2 frogs, 15 jumps, mean SVL 6.4 cm; peak plantaris 3.3 ± 0.2 N (~13× body weight) | VERIFIED | Official *PLoS ONE* PDF, CC BY 4.0. 54° is a selected near-maximal condition, not Porro Table 4’s compiled 26° mean. | https://doi.org/10.1371/journal.pone.0173415 |
| 6.6–6.12 | Porro 2017 *K. maculata*: 4 adults, 28.4 ± 3.7 g, SVL 60.0 ± 1.2 mm, 50 jumps (13 / 24 / 13); all-trials kinematics; Table 4 compiled anurans including *R. pipiens* mean 26° (16–42) and 56 SVL/s | VERIFIED | UCL author PDF. Printed DOI `10.1242/jeb.155416`. Table 4 *pipiens* sources: Hirano and Rome 1984; Lutz and Rome 1996a; Astley and Roberts 2014. 6.11–6.12 remain “as quoted.” ⚠ARR, CoB. | https://discovery.ucl.ac.uk/id/eprint/10071143/1/Porro_1882.full.pdf |
| 6.13 | Herrel, Vasilopoulou-Kampitsi and Bonneaud 2014 *Xenopus tropicalis*: 0.53 ± 0.26 N; SVL 38.74 ± 6.09 mm; mass 6.37 ± 2.85 g | VERIFIED | PMC4226644, *PeerJ* 2:e661, doi `10.7717/peerj.661`. CC BY 4.0 (PMC; Crossref licence field empty). C6 also states n = 125 (56♂, 69♀) in the supporting text; those counts match the paper. | https://doi.org/10.7717/peerj.661 · https://pmc.ncbi.nlm.nih.gov/articles/PMC4226644/ |
| 6.14 | Moreno-Rueda et al. 2020: n = 712, SVL 2.2–7.9 cm; jump distance levels off; longer relative femur and tibiofibula improve performance | VERIFIED | ⚠NC CC BY-NC 4.0 confirmed (PMC7319472). Wording “femur and tibiofibula” is slightly tighter than the paper’s “hind limbs.” | https://pmc.ncbi.nlm.nih.gov/articles/PMC7319472/ |
| 6.15 | Marsh and John-Alder 1994 hylid take-off / power | UNVERIFIABLE | Official JEB full text not obtained. Agrees with C6 Unverified. | https://doi.org/10.1242/jeb.188.1.131 |
| 6.16 | Richards, Porro and Collings 2017 *K. maculata* min/max jump angle 7.61 ± 6.68 / 58.43 ± 8.24° (N = 4) | VERIFIED | Official JEB HTML opened (this pass). Mean ± s.d.; “minimum angle averaged across all frogs” / maximum. ⚠ARR, CoB. C6 had this Unverified. | https://doi.org/10.1242/jeb.144279 · https://journals.biologists.com/jeb/article/220/10/1894/17784/Kinematic-control-of-extreme-jump-angles-in-the |

## Source table

| Key | URL | Resolves? | Licence correct? |
|---|---|---|---|
| H19 Hutchinson 2019 | https://doi.org/10.1038/s41598-019-55768-6 · PMC6917812 | Yes (PMC 200) | Yes — CC BY 4.0 |
| D19 Druelle 2019 | https://doi.org/10.1038/s41598-019-53329-5 · PMC6856151 | Yes (PMC 200) | Yes — CC BY 4.0 |
| JH17 Jagnandan and Higham 2017 | https://doi.org/10.1038/s41598-017-11484-7 · PMC5589804 | Yes (PMC 200) | Yes — CC BY 4.0 (this is *Sci. Rep.*, not JEB) |
| C20 Cieri 2020 | https://doi.org/10.1038/s41598-020-64140-y · PMC7217971 | Yes | Yes — CC BY 4.0 |
| JI99 Jayne and Irschick 1999 | https://doi.org/10.1242/jeb.202.2.143 · UC author PDF | Author PDF 200; official PDF often 403 | Yes — ⚠ARR, CoB user licence 1.1 |
| I18 Iijima 2018 | https://doi.org/10.1098/rsos.171774 · PMC5882705 | Yes (PMC 200) | Yes — CC BY 4.0 on the paper |
| F21 Feiner 2021 | https://doi.org/10.1098/rspb.2021.0226 · PMC8220270 | Yes (PMC 200) | Yes — CC BY 4.0 (PMC record, as C6) |
| E22 Ewart 2022 | https://doi.org/10.3390/biology11071052 · PMC9312080 | Yes (PMC 200) | Yes — CC BY 4.0 |
| S16 Schmidt 2016 | https://doi.org/10.1242/jeb.137059 · PubMed 27340203 | Official HTML opened via WebFetch; PDFs 403 | Yes — ⚠ARR, CoB user licence 1.1 |
| Z05 Zani 2005 | https://doi.org/10.1242/jeb.01554 · PubMed 15802673 | Official HTML opened via WebFetch | Yes — ⚠ARR, CoB |
| W71 Walker 1971 | https://doi.org/10.1002/jmor.1051340205 | DOI / Crossref abstract yes; Wiley HTML 403 / timeout | Yes — Wiley VOR, ⚠ARR |
| M10 Mazouchova 2010 | https://doi.org/10.1098/rsbl.2009.1041 · PMC2880067 · Goldman-lab PDF | Journal PDF 200 (257,897 B); RS publisher 403/500; PMC 200 | **Nuance, not a number error.** Crossref now CC BY 4.0 (start date 2010-02-10 is anachronistic). PMC deposit CC BY 2.0. PDF prints only © 2010 The Royal Society. C6 “CC BY per Crossref” slightly overstates 4.0 as the original licence. |
| AR94 Ashley-Ross 1994a | https://doi.org/10.1242/jeb.193.1.255 · WFU author PDF | Author PDF 200 | Yes — ⚠ARR, CoB user licence 1.1 |
| K13 Karakasiliotis 2013 | https://doi.org/10.1007/s00422-012-0540-4 · EPFL PDF | Author/IR PDF 200 | Yes — Springer ARR; no open licence stated. Do not confuse with `10.1007/s00422-013-0563-5` (Schilling, CC BY 2.0). |
| P20 Pierce 2020 | https://doi.org/10.1093/iob/obaa015 · PMC7671131 | Yes (PMC 200) | Yes — CC BY 4.0 |
| V21 Vega and Ashley-Ross 2021 | https://doi.org/10.1093/iob/obab029 · PMC8545788 | Yes (PMC 200) | Yes — CC BY 4.0 |
| AR12 Astley and Roberts 2012 | https://doi.org/10.1098/rsbl.2011.0982 · author-lab PDF | Author-lab PDF 200 | Yes — ⚠ARR / bronze (not CC BY) |
| Mo17 Moo 2017 | https://doi.org/10.1371/journal.pone.0173415 | Official PDF 200 | Yes — CC BY 4.0 |
| Po17 Porro 2017 | https://doi.org/10.1242/jeb.155416 · UCL PDF | Author PDF 200; `10.1242/jeb.144436` is 404 | Yes — ⚠ARR, CoB user licence 1.1 |
| He14 Herrel 2014 | https://doi.org/10.7717/peerj.661 · PMC4226644 | Yes (PMC 200) | Yes — CC BY 4.0 (PMC; Crossref licence empty) |
| MR20 Moreno-Rueda 2020 | https://doi.org/10.1093/cz/zoz062 · PMC7319472 | Yes (PMC 200) | Yes — ⚠NC CC BY-NC 4.0 |
| Unverified leads (10 remaining DOIs as listed in C6, excluding Richards 2017) | DOIs as in C6 full source list | DOIs resolve; full texts 403 or unopened here | ⚠ARR (JEB / Wiley / CUP) as C6 states |
| Richards, Porro and Collings 2017 | https://doi.org/10.1242/jeb.144279 · official JEB HTML | Official HTML opened via WebFetch; scripted PDFs 403 | Yes — ⚠ARR, CoB user licence 1.1 |
| IOB `obaa017` (not a C6 source) | https://doi.org/10.1093/iob/obaa017 | Yes — Usherwood 2020, PMC7545857 | n/a (wrong paper if used for fire salamander or Butcher) |

## Grok CLI live pass

A first Grok CLI live pass was started at 09:47:31 AEDT and was killed by the box restart at about 09:59 AEDT before it exited. The required **single replacement pass** was launched at **17 November 2026, 10:01:26 AEDT** and exited 0 at **10:17:27 AEDT** (16 min 1 s; 26,449 bytes of stdout). Command:

`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /tmp/lv17c6/grok_prompt.md --output-format plain`

No number in this verify file is taken from Grok alone. Every figure used above was opened in a publisher page, PMC/OAI, Crossref, or a legal author/institution PDF. Grok’s report is treated as leads.

**Agreement (after I opened the same or equivalent sources):** Mazouchova licence conflict (Crossref CC BY 4.0 with anachronistic 2010-02-10 start vs PMC/OAI CC BY 2.0 vs PDF © 2010 The Royal Society; do not treat 4.0 as the original licence). Moreno-Rueda is CC BY-NC 4.0. Schmidt, Cieri, Moo, Iijima (size-normalised, ranges, PGLS), Pierce Table 1 s.e.m. and obaa017 = Usherwood, Karakasiliotis DOI vs Walknet 0563-5, Porro Table 4 26° (16–42) from Hirano and Rome 1984 / Lutz and Rome 1996a / Astley and Roberts 2014, Porro DOI 155416 vs dead 144436, Hutchinson display formulae **RSF = SF·(g·h^{−1})^{−0.5}** and **û = u·(g·h)^{−0.5}**, and the remaining JEB/Wiley leads still blocked to scripted clients.

**Grok also opened, and I then confirmed on the official JEB HTML:** Richards et al. 2017 7.61±6.68 / 58.43±8.24° (mean ± s.d., N = 4), which upgrades row 6.16 from Unverified to VERIFIED.

**Where Grok was incomplete or off, and I opened the source anyway:**
- Zani gait label: Grok reported that “lateral sequence, diagonal couplets” was not in the opened excerpts and that Zani only compared limb phase to Jayes and Alexander 1980. The official JEB HTML I opened (`https://journals.biologists.com/jeb/article/208/8/1489/16026/Giant-Gala-pagos-tortoises-walk-without-inverted`) does print “Their gait was a lateral sequence diagonal couplet (Fig. 3; Hildebrand, 1985).” C6 is correct. Grok did correctly flag the later Discussion value 0.42±0.083 vs Abstract/Results 0.41±0.092.
- Porro husbandry vs Table 1: Grok is right that 28.4±3.7 g / 60.0±1.2 mm sit in Methods / Table S1, not Table 1. C6 already cites Methods for row 6.6, so this is not a C6 error.
- Grok warned about nearby wrong DOIs (Hutchinson 10.1038/s41598-019-55747-x is a *Drosophila* paper; Livingston 10.1002/ar.20900 is White 2009 lemur molars). C6 already uses the correct DOIs (`…55768-6` and `10.1002/ar.20912`).
- Grok listed “50 trials” rather than “50 jumps”; both wordings appear in Porro/Richards.

Full Grok stdout (including tool-trace preamble) is saved at `/tmp/lv17c6/logs/grok_out.txt` and is not copied into the library file.

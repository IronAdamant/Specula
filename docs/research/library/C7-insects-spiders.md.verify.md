# Verification: C7-insects-spiders.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/C7-insects-spiders.md`
Output path (this file only): `/workspace/specula-research/C7-insects-spiders.md.verify.md`

Private adversarial source-check. No number was taken from Grok, a search snippet, or a pirate mirror. Every figure below was opened in a publisher page, PMC/Europe PMC full text, Crossref metadata, or a legal author/institution PDF (including the official JEB PDF for Theunissen 2015 and the JHU author PDF for Zheng, Hedrick & Mittal 2013 JFM).

## Summary

- **81 data rows** checked (sections 1–4: 25 + 24 + 15 + 17).
- **69 VERIFIED.** Almost every C7 “Verified” number that I could open matches the source, with those units and that population. Two C7 “Unverified” rows are upgraded here: 2.18 (Wahl Fig. 5c percentages and category order) and 2.23 (Full & Tu 1991, official abstract).
- **0 number FIXES.** One **citation FIX** on an already-Unverified row: 1.7 uses the wrong Cruse 1976 DOI.
- **12 UNVERIFIABLE:** 1.7 (wrong DOI; correct paper not opened), 1.11 (Kram 1997; official abstract opened, Table 1 not), 1.14 (Ramdya supplement not opened), 1.18 (Silva-Pereyra Table 1 tarsus cells empty/garbled in the JATS XML; PDF blocked), 2.24 (Bender 2011, JEB, not OA), 3.14–3.15 (Biancardi 2011 and Weihmann 2012, JEB blocked; Biancardi identity confirmed), 4.11 (Dudley 1990 abstract not obtained in this pass), 4.13–4.17 (Ellington, Fry, Willmott, Baker, Wakeling; official abstracts where obtained do not contain the claimed numbers, or were not opened).
- **Rows skipped:** none of the 81.
- **Dead links:** no citation DOI used in C7 is a dead identifier. All 15 checked DOIs resolve at doi.org (HTTP 302). Official JEB HTML/PDF landings return 403 to scripted fetches; Springer link.springer.com returned 500 to scripts for the two 1976/1981 DOIs; the JHU author PDF for Zheng 2013 JFM returns 403 to `curl` but opened via a page-fetch of the same URL. PMC3102013 (Bender) and Europe PMC XML for PMC1312389 returned 500 to the XML API; Altshuler was read from the official PMC full-text extract already fetched live today, and the Table 1 numbers were re-checked there.
- **Worst problem:** row 1.7 cites `10.1007/BF00606538`, which is a 1976 crab-vision paper, not Cruse’s stick-insect Table 3. The numbers in that row were left Unverified and should stay that way until the correct paper is opened.

One-line summary: C7 is solid; correct the Cruse 1976 DOI, leave the remaining closed JEB/Springer rows Unverified, and note that Theunissen’s caption measures mass on 10 animals per species.

## File snapshot

At the start of this check the file was copied to `/workspace/lv-scratch/c78/C7-insects-spiders.md`. Snapshot: **32,856 bytes**, sha256 `7e7e710170924bde59365f20fbf760897f4dbeeb6ec65edb540c4514e9184ecb`, 220 lines (81 data rows), mtime recorded as 17 November 2026, 10:26:24 AEDT (mtime is not trusted after the 09:59 restore; identity is by size and hash).

Immediately before this verify file was written (10:59:44 AEDT), `sha256sum` and `cmp` of the workspace file against that snapshot were identical. **No content change; no row re-check was required.**

## Close checks

### 1. Theunissen, Bekemeier and Dürr 2015, Table 1 — column order

**Opened:** the official JEB Version of Record PDF, *J. Exp. Biol.* 218:340–352, doi `10.1242/jeb.114173`, © 2015 The Company of Biologists Ltd (⚠ARR; Crossref lists no open licence). Direct PDF URL `https://journals.biologists.com/jeb/article-pdf/218/3/340/1887112/340.pdf` (403 to `curl`; opened as the publisher PDF in this pass). Title and pagination confirmed on PubMed (PMID 25524984) and Crossref.

**Column order is hind → middle → front, as C7 said.** The Table 1 header, left to right after the body columns (Mass, Abd, T3, T2, T1, Head), is “Hind legs / Middle legs / Front legs / Antennae”, each as cox / fem / tib (then scp / ped / fla).

*Carausius morosus* ♀, N = 10, means ± s.d.:

- Front: 1.32 ± 0.16 / 15.84 ± 0.85 / 15.03 ± 0.61 mm
- Middle: 1.42 ± 0.13 / 11.60 ± 0.42 / 11.20 ± 0.34 mm
- Hind: 1.54 ± 0.16 / 13.87 ± 0.44 / 14.11 ± 0.57 mm
- Body: mass 0.955 ± 0.124 g; Abd 39.15 ± 1.21; T3 11.96 ± 0.56; T2 17.11 ± 0.43; T1 3.68 ± 0.29; head 4.21 ± 0.23 mm; antenna 1.54 ± 0.08 / 0.75 ± 0.09 / 30.87 ± 0.69 mm

*Medauroidea extradentata* ♀, N = 6: hind 2.07 / 27.48 / 33.87; middle 2.07 / 21.42 / 24.00; front 2.11 / 32.54 / 39.07; mass 1.673 ± 0.243 g (s.d. are in the table, as C7 said).

*Aretaon asperrimus* ♂, N = 13: hind 2.06 / 12.43 / 13.00; middle 1.86 / 9.79 / 9.67; front 1.91 / 10.56 / 10.65; mass 0.818 ± 0.068 g.

Sexes match Fig. 1 (adult female *Carausius* and *Medauroidea*; adult male *Aretaon*). Rows 1.1–1.6 verified, and the inferred column order is confirmed.

**Caption nuance, not a value error:** “The mean mass and s.d. are given in g and were measured from 10 animals per species.” Length N is 10 / 13 / 6 as in the header; mass n is 10 for all three species. C7 attached the header N to the mass values as well. The mass *values* are correct.

### 2. Hawkmoth 25 Hz and Painted Lady 22 Hz — n = 1

**(a) Zheng, Hedrick and Mittal 2013, *J. Fluid Mech.* 721:118–154, doi `10.1017/jfm.2013.46`.** Official author PDF at Johns Hopkins, `https://engineering.jhu.edu/fsag/wp-content/uploads/2013/11/2013-Lingxiao-JFM.pdf` (403 to `curl`; opened in this pass). © Cambridge University Press 2013 (⚠ARR). Section 3.2: “For the particular moth examined in this work … m = 1.39 g, R = 4.93 cm, … f = 1/T = 25 Hz, θA = 100.3° and β = 21.8°.” Methods: “This individual moth weighed 1.39 g”; the digitized sequence was “divided … into four complete phase-matched flapping cycles” and results are “phase-averaged over four consecutive flapping cycles.” **n = 1 moth, four cycles. Row 4.12 verified.**

**(b) Zheng, Hedrick and Mittal 2013, *PLoS ONE* 8:e53060, doi `10.1371/journal.pone.0053060`, CC BY.** Official printable PDF opened (HTTP 200, 882,164 bytes). Results: “R = 3.0 cm … f = 22.2 Hz”; “wingbeat amplitude w … is 120°.” Discussion: “the current study is based only a single wingbeat from a single butterfly specimen.” Table 1 prints wing length 3.01 cm; the text’s 3.0 cm is what C7 used. **n = 1 (one wingbeat). Row 4.10 verified.**

### 3. Fruit-fly top step frequency

**(a) Mendes, Bartos, Márka and Mann 2013, *eLife* 2:e00231, PMC3545443, CC BY 3.0.** Official eLife PDF opened. 71 videos; speeds 7.2–44.7 mm/s, “28 mm/s the most represented speed” (C7: “most common 28”). Fig. 2C text: step period “varies inversely with average speed up to approximately 30 mm/s, when step period reaches a steady state of about ∼60 ms, corresponding to a frequency of 16 cycles per s.” 1/0.060 s = 16.67 cycles/s, fairly described as ≈ 16. Table 1 gait shares match C7 exactly (slow / medium / fast tripod 31.37 / 51.63 / 64.98, tetrapod 25.45 / 15.90 / 7.26, pentapod 30.15 / 14.37 / 13.33). Slow-fly lag: body text `H Lag F = 0.505 * P_H + 27.6`; figure caption 27.627. Rows 2.1–2.6 verified.

**(b) Ramdya et al. 2017, *Nat. Commun.* 8:14494, PMC5321742, CC BY 4.0.** Official publisher PDF opened. **20 Hz is a model parameter, not a measured fruit-fly frequency:** Supplementary Movie 3: “The insect model is 2.5 mm long and is walking at 20Hz.” Supplementary Movie 5: “walking at 5Hz (25 mm model), or 2Hz (250 mm model).” Robot: “nearly 25% faster when using the bipod gait rather than the tripod gait (Fig. 5c,d, P < 0.001).” Rows 2.10–2.11 verified as model / robot results. Supplementary Table 2 (row 1.14) was not opened (supplement URL 403).

### 4. Licences — Bomphrey, Altshuler, JEB

**(a) Bomphrey, Nakata, Phillips and Walker 2017, *Nature* 544:92–95, doi `10.1038/nature21727`, PMC5412966.** The PMC copy is the UKPMC Funders **author manuscript** (EMS71723). Its licence-p restricts use to viewing, printing, copying and text/data-mining “for the purposes of academic research,” subject to Nature’s Conditions of use. The Version of Record is Nature copyright (⚠ARR). C7’s “author manuscript, academic use / numbers only” treatment is right. Numbers from the AM: male *Culex quinquefasciatus* “717 ± 59 (mean ± one s.d.) Hz and with amplitudes of just 39°±4°”; “425 wing beats, over 15 sequences from between 12 and 15 individuals.” Row 4.5 verified.

**(b) Altshuler, Dickson, Vance, Roberts and Dickinson 2005, *PNAS* 102:18213–18218, doi `10.1073/pnas.0506590102`, PMC1312389.** 2005 PNAS has no CC licence (⚠ARR), as C7 states. PMC id PMC1312389 is this paper. Table 1: normal-air means 229.8 ± 11.0 Hz, 90.8 ± 8.6°, Re 1,123 ± 81.3 (C7 rounds the last to 81); four bees, per-bee frequency 221.5–245.0 Hz and amplitude 81.3–96.7°. Heliox means 235.8 ± 11.7 Hz, 131.6 ± 16.6°. Discussion quotes Feuerbacher et al. for pollen-loading “Φ = 118°” and “n = 234 Hz.” Rows 4.1–4.4 verified (4.4 as quoted).

**(c) Company of Biologists / JEB papers.** Crossref prints `http://www.biologists.com/user-licence-1-1/` on Dudley 1990, Full & Tu 1991, Kram 1997, Willmott & Ellington 1997, Wakeling & Ellington 1997, and Fry et al. 2005. Theunissen 2015 and Biancardi 2011 have empty Crossref licence arrays but the VoR PDFs / pages are © The Company of Biologists (⚠ARR). None of these is CC BY. C7’s “numbers only” flag is right.

### 5. Le Roy et al. 2026, *PLoS Biology* — final published version

**Opened:** official printable PDF and Europe PMC full-text XML, PMC13349162. Final article: Le Roy, Bharathi, Engels et al., “Dipteran flight diversity is shaped by aerodynamic constraints, scaling, and evolutionary trade-offs,” *PLOS Biology* 24:e3003473, doi `10.1371/journal.pbio.3003473`, e-published 9 July 2026, CC BY 4.0. This is the published version, not a preprint.

Results: morphology in **133 species**; kinematics/CFD for a subset of **46 species**. “Among most Diptera taxa, wingbeat frequency is conserved within [a] relatively narrow range of f = 236 [201–270] Hz (mean [95% confidence interval]; Fig 3I).” Stroke amplitude “Aϕ = 117° [110°–124°].” Culicomorpha “f = 714 [536–924] Hz” and “Aϕ = 37° [32°–42°]”; Tipulomorpha “f = 70 [12–118] Hz; Fig 3C.” Rows 4.6–4.7 verified. (C7’s “Fig. 3” for the typical-fly means is fair; the frequency CI is labelled Fig. 3I in the paper.)

### 6. Grok corrections — identities

**(a) Nabawy, Sivalingam, Garwood, Crowther, Sellers et al. 2018, *Sci. Rep.* 8:7142, PMC5940701, doi of the article as opened, CC BY 4.0.** Table 1 of a female *Phidippus regius* micro-CT specimen matches C7 rows 1.22–1.24 exactly (totals 10.4 / 8.4 / 8.4 / 10.5 mm and all seven segment percentages). Results: appendages (legs, chelicerae, pedipalps) are “26% of total mass.” A **second** female, used for the jumps, is “body mass 150 mg, L = 15 mm”; Fig. 1 caption calls the CT animal “a second specimen.” C7’s “n = 1 each” is right. Row 1.25 verified.

**(b) Biancardi et al. 2011 is doi `10.1242/jeb.057471`.** Crossref and Europe PMC title: “Biomechanics of octopedal locomotion: kinematic and kinetic analysis of the spider *Grammostola mollicoma*.” Official abstract opened (JEB full text 403). The abstract confirms the species and an 18 g animal and discusses duty factor and gaits, but does not print `d = −0.531 v + 0.666` or `f = 14.4 v + 0.14` or 4.55 Hz. Row 3.14 stays UNVERIFIABLE. The DOI correction is confirmed.

**(c) Altshuler 2005 is PMC1312389.** Confirmed (close check 4).

### 7. New or changed since the restore

**(a) Ellington 1999, doi `10.1242/jeb.202.23.3439`.** Official Europe PMC abstract opened (JEB full text 403). The abstract does not contain 149 Hz, 114°, 0.175 g or 13.2 mm for *Bombus terrestris*. C7 is right to leave row 4.13 Unverified.

**(b) Added Seidl and Wehner 2008 stance-duration row, PMC2430559, *Front. Zool.* 5:8, doi `10.1186/1742-9994-5-8`, CC BY 2.0.** Official full text opened. Desert ant steps 10.1 ± 2.5 mm vs wood ant 7.9 ± 1.3 mm (Fig. 2); 876 steps from 263 sequences; “α ≈ 29 mm per m/s”; “γ ≈ 0.8 mm step length per 1 mm head width”; “slope … only in the hind leg (δ = −0.0063 ± 0.003 mm per degree inclination).” Swing 0.027 ± 0.004 s vs 0.028 ± 0.006 s; stance 0.032 ± 0.011 vs 0.039 ± 0.012 s. Maximum speeds: desert ants “0.4 m/s or about 40 body lengths per second” on level ground; wood ants “0.25 m/s, −30°.” Rows 2.19–2.22 verified. (C7’s source list should say CC BY 2.0, not a generic CC BY, if it does not already.)

## Citation FIX (not a number change)

**Row 1.7 — Cruse 1976 DOI.** C7 cites `https://doi.org/10.1007/BF00606538` for “Cruse 1976, Table 3” (*C. morosus*, n = 20). Crossref and OpenAlex identify that DOI as “The detection of real and apparent motion by the crab *Leptograpsus variegatus*: II. Electrophysiology,” *J. Comp. Physiol. A*, 1976 — the wrong paper. The stick-insect paper is Cruse, “The function of the legs in the free walking stick insect, *Carausius morosus*,” *J. Comp. Physiol. A* 112:235–262, **doi `10.1007/BF00606541`** (adjacent identifier). I did not open a legal full text of BF00606541, so the Table 3 numbers stay **UNVERIFIABLE**. Replace the DOI only.

## Notes that are not number errors

- **Row 2.18 (Wahl 2015, Fig. 5c).** C7 left this Unverified because the five-category order was unclear and the 20–25 mm/s percentages were cut off. The open full text (PMC4439428, CC BY) states the order as “tripod, tetrapod, wavegait, undefined gait, hexa support phase,” and prints I 5–10 mm/s: 27, 14, 29, 6, 23% and V 25–30 mm/s: 58, 7, 15, 13, 7%, matching C7. Group IV (20–25 mm/s) percentages are missing in the paper’s own text, not just in C7’s extract. I mark the two example lists **VERIFIED** and leave IV unfilled.
- **Row 2.23 (Full & Tu 1991).** Official abstract (doi `10.1242/jeb.156.1.215`): mass 0.83 g; tripod 0.44–1.0 m/s; aerial phases above 1 m/s; quadrupedal and bipedal running at 1.0–1.5 m/s; “Stride frequency approached the wing beat frequencies used during flight (27 Hz).” I mark this **VERIFIED (abstract)**. The “~27 Hz” wording is the abstract’s.
- **Row 1.18 (Silva-Pereyra tarsus).** The JATS XML has empty length/diameter cells; the four numbers C7 quoted (10 ± 0.3 / 0.8 ± 0.3 / 0.8 ± 0.4 / 10 ± 0.2) sit in the mass column and are internally inconsistent as lengths. The official PeerJ PDF was 403 in this pass. Agree with C7: Unverified pending a page image of Table 1.
- **DeAngelis event counts (row 2.7):** swing n = 58,000–59,000 per limb and stance n = 54,000–56,000 per limb. C7’s “~54,000–59,000 stance/swing events per leg” is a fair envelope, not a wrong number.
- **Guo 2018 (rows 1.12–1.13):** Table 1–2 match exactly. “Body parts … were weighed and photographed for 15 randomly selected ants.” Mass percentages re-derive: total 24.05 mg if each listed leg mass is one leg and the pair proportions are 5.00 / 4.12 / 5.82%. *C. fortis* kinematics/forces come from a previous study; the segments are model parameters, as C7 said.

## Row table

| Row | Claim (short) | Status | Note | URL |
|---|---|---|---|---|
| 1.1–1.4 | *C. morosus* ♀ N=10 segments and body | VERIFIED | Official JEB PDF Table 1; hind→middle→front header confirmed | https://doi.org/10.1242/jeb.114173 |
| 1.5 | *M. extradentata* ♀ N=6 legs; mass 1.673 ± 0.243 g | VERIFIED | Values match; mass n = 10 per caption | same |
| 1.6 | *A. asperrimus* ♂ N=13 legs; mass 0.818 ± 0.068 g | VERIFIED | Values match; mass n = 10 per caption | same |
| 1.7 | Cruse 1976 Table 3 front segments; body 73.2 ± 4.6 mm; n=20 | UNVERIFIABLE | **Citation FIX:** DOI is a crab paper; correct is 10.1007/BF00606541, not opened | https://doi.org/10.1007/BF00606538 (wrong) |
| 1.8–1.10 | Parle 2016 Table 2 locust and cockroach tibiae | VERIFIED | Table 2 and caption (hind tibia as quoted from Dirks & Taylor 2012) | https://pmc.ncbi.nlm.nih.gov/articles/PMC4972353/ |
| 1.11 | Kram 1997 Table 1 *B. discoidalis* segments | UNVERIFIABLE | Official abstract opened; Table 1 not. JEB ⚠ARR | https://doi.org/10.1242/jeb.200.13.1919 |
| 1.12–1.13 | Guo 2018 model segments and masses; 15 ants | VERIFIED | Tables 1–2; 15 ants in Methods; CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5795013/ |
| 1.14 | Ramdya Suppl. Table 2 *Drosophila* model segments | UNVERIFIABLE | Supplement not opened (403) | https://doi.org/10.1038/ncomms14494 |
| 1.15–1.17, 1.19–1.21 | Silva-Pereyra 2019 Tables 1–2 morphology, n=12 males | VERIFIED | XML Table 1/2 match; mass 219 ± 23 × 10⁻² g = 2.19 ± 0.23 g | https://pmc.ncbi.nlm.nih.gov/articles/PMC6766366/ |
| 1.18 | Tarantula tarsus lengths | UNVERIFIABLE | XML cells garbled/empty; PDF blocked | same |
| 1.22–1.25 | Nabawy 2018 *P. regius* Table 1 and 26% / 150 mg / 15 mm | VERIFIED | Sci. Rep. 8:7142; two females, n=1 each; CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5940701/ |
| 2.1–2.6 | Mendes 2013 speeds, Table 1 gaits, ~60 ms ≈ 16 cycles/s, lag | VERIFIED | CC BY 3.0; 27.6 in body text, 27.627 in Fig. 2D caption | https://pmc.ncbi.nlm.nih.gov/articles/PMC3545443/ |
| 2.7–2.9 | DeAngelis 2019 speeds, 24/13/7 mm/s, phases | VERIFIED | Fig. 2D 24/13/7 for 3/4/5 feet; phases 0.5 / ≈0.4 / ≈0.85; CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6598772/ |
| 2.10–2.11 | Ramdya 2017 20 Hz model; bipod ~25% faster | VERIFIED | 20 Hz is a model setting; P < 0.001 on the robot | https://pmc.ncbi.nlm.nih.gov/articles/PMC5321742/ |
| 2.12–2.17 | Wahl 2015 *C. fortis* N=388 strides, fits, DF thresholds | VERIFIED | Fig. 1–2, 6; 132 / 182 / 369 mm/s; ~370 mm/s aerial; CC BY | https://pmc.ncbi.nlm.nih.gov/articles/PMC4439428/ |
| 2.18 | Wahl slow-walk gait shares 5–10 and 25–30 mm/s | VERIFIED | Order now confirmed; 20–25 mm/s % missing in the paper itself | same |
| 2.19–2.22 | Seidl & Wehner 2008 step length, fits, stance, top speeds | VERIFIED | CC BY 2.0; −0.0063 ± 0.003 is in the Results text | https://pmc.ncbi.nlm.nih.gov/articles/PMC2430559/ |
| 2.23 | Full & Tu 1991 cockroach gaits and ~27 Hz | VERIFIED (abstract) | Official abstract matches; full text 403 | https://doi.org/10.1242/jeb.156.1.215 |
| 2.24 | Bender 2011 preferred speeds / Fr 0.4 | UNVERIFIABLE | JEB, not OA; Europe PMC XML 500 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3102013/ |
| 3.1–3.10 | Weihmann 2013 *C. salei* size, Tables 1–3, DF / no aerial | VERIFIED | “1st, 2nd and 4th legs at high speeds the mean duty factors sank below 0.5”; “Aerial phases were not observed”; CC BY | https://pmc.ncbi.nlm.nih.gov/articles/PMC3689776/ |
| 3.11–3.13 | Silva-Pereyra 2019 Table 3 locomotion, n=5 | VERIFIED | Speed 2.17–2.80 cm/s; DF 0.58–0.61; stride 3.04–3.53 cm (2.09–2.43 BL); f 0.62–0.74 s⁻¹; COM 1.34–1.70 cm; recovery 1.9–10.5% | https://pmc.ncbi.nlm.nih.gov/articles/PMC6766366/ |
| 3.14 | Biancardi 2011 *G. mollicoma* DF and frequency fits | UNVERIFIABLE | DOI identity confirmed; fits not in the abstract; JEB 403 | https://doi.org/10.1242/jeb.057471 |
| 3.15 | Weihmann, Günther & Blickhan 2012 hydraulics | UNVERIFIABLE | JEB 403; not opened | https://doi.org/10.1242/jeb.054585 |
| 4.1–4.4 | Altshuler 2005 Table 1 honeybee hover / heliox; pollen quoted | VERIFIED | PMC1312389; ⚠ARR (PNAS); pollen from Feuerbacher et al. | https://pmc.ncbi.nlm.nih.gov/articles/PMC1312389/ |
| 4.5 | Bomphrey 2017 mosquito 717 ± 59 Hz, 39 ± 4°, n details | VERIFIED | Author manuscript, academic use; VoR Nature ⚠ARR | https://pmc.ncbi.nlm.nih.gov/articles/PMC5412966/ |
| 4.6–4.7 | Le Roy 2026 typical Diptera and Culicomorpha / Tipulomorpha | VERIFIED | Final *PLoS Biol.* 24:e3003473, 9 Jul 2026, CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC13349162/ |
| 4.8–4.9 | Cheng & Sun 2016 *L. sativae* n=5 Tables 1–2 | VERIFIED | 259.3–288.4 Hz; Φ 172.4–182.4°; mass 0.17–0.32 mg; R 1.31–1.49 mm; CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4863373/ |
| 4.10 | Zheng 2013 PLoS *V. cardui* 22.2 Hz, 120°, 3.0 cm, n=1 | VERIFIED | Single wingbeat from a single butterfly; CC BY | https://doi.org/10.1371/journal.pone.0053060 |
| 4.11 | Dudley 1990 ~11 Hz, ~103° | UNVERIFIABLE | DOI resolves; official abstract not obtained (JEB 403, Europe PMC empty) | https://doi.org/10.1242/jeb.150.1.37 |
| 4.12 | Zheng 2013 JFM *M. sexta* 25 Hz, 100.3°, n=1 (four cycles) | VERIFIED | Official JHU author PDF, §3.2 and methods | https://doi.org/10.1017/jfm.2013.46 |
| 4.13 | Ellington 1999 bumblebee 149 Hz, 114° | UNVERIFIABLE | Official abstract opened; claimed numbers not in it; JEB 403 | https://doi.org/10.1242/jeb.202.23.3439 |
| 4.14 | Fry, Sayaman & Dickinson 2005 hover 218 ± 7 Hz, 140 ± 10° | UNVERIFIABLE | Official abstract opened; Table 1 not in it; JEB 403 | https://doi.org/10.1242/jeb.01612 |
| 4.15 | Willmott & Ellington 1997 24.8–26.5 Hz | UNVERIFIABLE | Official abstract opened; those figures not in it; JEB 403 | https://doi.org/10.1242/jeb.200.21.2705 |
| 4.16 | Baker, Gewecke & Cooter 1981 22.9 vs 19.8 Hz | UNVERIFIABLE | DOI resolves (Springer 500 to scripts); abstract not opened | https://doi.org/10.1007/BF01342669 |
| 4.17 | Wakeling & Ellington 1997 dragonfly vs damselfly | UNVERIFIABLE | Official abstract confirms damselfly frequencies about half the dragonfly’s, but not the 43.5 / 16 Hz figures (cited from Rüppell 1989) | https://doi.org/10.1242/jeb.200.3.557 |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Theunissen 2015 | https://doi.org/10.1242/jeb.114173 | Yes (302 → journals.biologists.com, HTML/PDF 403 to scripts; VoR PDF opened) | Yes — ⚠ARR (CoB © 2015) |
| Cruse 1976 (as cited) | https://doi.org/10.1007/BF00606538 | Yes, but **wrong paper** (crab electrophysiology) | n/a |
| Cruse 1976 (correct) | https://doi.org/10.1007/BF00606541 | Yes (Crossref / publisher landing) | ⚠ARR (Springer); not opened |
| Parle 2016 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4972353/ | Yes (Europe PMC XML 200) | Yes — CC BY 4.0 |
| Kram 1997 | https://doi.org/10.1242/jeb.200.13.1919 | Yes (JEB 403) | Yes — CoB user licence 1.1 (⚠ARR) |
| Guo 2018 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5795013/ | Yes | Yes — CC BY 4.0 |
| Ramdya 2017 | https://doi.org/10.1038/ncomms14494 · PMC5321742 | Yes (Nature PDF 200) | Yes — CC BY 4.0 |
| Silva-Pereyra 2019 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6766366/ | Yes (XML 200; PeerJ PDF 403) | Yes — CC BY 4.0 |
| Nabawy 2018 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5940701/ | Yes | Yes — CC BY 4.0 |
| Mendes 2013 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3545443/ | Yes (eLife PDF 200) | Yes — CC BY 3.0 (C7 “CC BY” is true; version is 3.0) |
| DeAngelis 2019 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6598772/ | Yes (eLife PDF 200) | Yes — CC BY 4.0 |
| Wahl 2015 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4439428/ | Yes | Yes — CC BY (Springer OA) |
| Seidl & Wehner 2008 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2430559/ | Yes | Yes — CC BY **2.0** |
| Full & Tu 1991 | https://doi.org/10.1242/jeb.156.1.215 | Yes (JEB 403; abstract via Europe PMC) | Yes — CoB user licence 1.1 (⚠ARR) |
| Bender 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3102013/ | Landing exists; XML 500; not OA | Yes — JEB ⚠ARR |
| Weihmann 2013 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3689776/ | Yes (PLoS PDF 200) | Yes — CC BY |
| Biancardi 2011 | https://doi.org/10.1242/jeb.057471 | Yes (JEB 403; abstract via Europe PMC) | Yes — ⚠ARR (CoB) |
| Weihmann et al. 2012 | https://doi.org/10.1242/jeb.054585 | Yes (JEB 403) | Yes — ⚠ARR (CoB) |
| Altshuler 2005 | https://pmc.ncbi.nlm.nih.gov/articles/PMC1312389/ | Yes (PMC full text; PDF API blocked) | Yes — ⚠ARR (PNAS) |
| Bomphrey 2017 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5412966/ | Yes (author manuscript XML 200) | Yes — AM academic-use; VoR Nature ⚠ARR |
| Le Roy 2026 | https://pmc.ncbi.nlm.nih.gov/articles/PMC13349162/ | Yes (PLoS PDF 200) | Yes — CC BY 4.0 |
| Cheng & Sun 2016 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4863373/ | Yes (Nature PDF 200) | Yes — CC BY 4.0 |
| Zheng 2013 PLoS | https://doi.org/10.1371/journal.pone.0053060 | Yes (PDF 200) | Yes — CC BY |
| Zheng 2013 JFM | https://doi.org/10.1017/jfm.2013.46 · JHU author PDF | DOI 200 (abstract); author PDF opened | Yes — ⚠ARR (Cambridge) |
| Dudley 1990 | https://doi.org/10.1242/jeb.150.1.37 | Yes (JEB 403) | Yes — CoB user licence 1.1 (⚠ARR) |
| Ellington 1999 | https://doi.org/10.1242/jeb.202.23.3439 | Yes (JEB 403; abstract via Europe PMC) | Yes — CoB user licence 1.1 (⚠ARR) |
| Fry et al. 2005 | https://doi.org/10.1242/jeb.01612 | Yes (JEB 403; abstract via Europe PMC) | Yes — CoB user licence 1.1 (⚠ARR) |
| Willmott & Ellington 1997 | https://doi.org/10.1242/jeb.200.21.2705 | Yes (JEB 403; abstract via Europe PMC) | Yes — CoB user licence 1.1 (⚠ARR) |
| Baker et al. 1981 | https://doi.org/10.1007/BF01342669 | DOI 302; Springer 500 to scripts | ⚠ARR (Springer TDM licence in Crossref) |
| Wakeling & Ellington 1997 | https://doi.org/10.1242/jeb.200.3.557 | Yes (JEB 403; abstract via Europe PMC) | Yes — CoB user licence 1.1 (⚠ARR) |

## Grok section

One live pass covering C7 and C8 was started at **10:46:04 AEDT** on 17 November 2026 with `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --no-alt-screen --prompt-file /workspace/lv-scratch/c78/grok/prompt.txt`. The process was still running when this C7 verify file was written (about 11:00 AEDT; ~14 minutes elapsed). **No number in this file was taken from that pass.** This file will not be held for it. If the pass later contradicts a row, that would need a separate note after C8.

Arithmetic re-derived where it mattered: 16 cycles/s from ~60 ms; Guo mass percentages from 24.05 mg; Silva body mass 219 ± 23 × 10⁻² g = 2.19 ± 0.23 g; Ramdya Δx-style movie-legend scale factors 20 Hz / 5 Hz / 2 Hz at 2.5 / 25 / 250 mm.

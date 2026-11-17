# I2-garment-fitting.md — Library Verify

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
I2 itself was not edited. URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at start: **30,457 bytes**, sha256 `df81ddf277b8d5ee0589e47b607adf843e50aa5e4ec7b1dbba2e253994f34879`.
- Re-stat before writing: same size and same sha256 as `/workspace/lv-scratch/i13/I2-garment-fitting.md`. No changed-row re-check needed.
- Box mtimes all read ~09:59 AEDT from the restore and were not used.

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **44** | All 16 KES values; all 10 Wang 2011 fabrics + cost; Wang 2010 timings (C2); KCD method + timings (G3, C1); TailorNet paper/README/licence (R5, C4, C7); PBNS README + no-licence (R11, C5); GarNet abstract (R8, C6); DrapeNet GPL-3.0 (R9); CAPE NC (R4); Robust Skin Weights MIT (R12); SMPL NC |
| **FIX** | **0** | Close checks all stand as I2 printed them |
| **UNVERIFIABLE** | **15** | Game/platform talks whose slides or transcripts were not re-opened this pass (G1–G2, G4–G11), plus research papers whose PDFs were not opened (R1–R3, R6–R7, R10, R14). Their URLs were checked. |
| **Total** | **59** | G1–G11 (11) + R1–R14 (14) + KES 16 + Wang 2011 10 + C1–C8 (8) |

**Worst problems**
1. None in the measured tables or the four close checks. Those all match the primary files, including the official KCD slide *images*.
2. Fifteen method-survey rows were not re-read in full (GDC videos/slides or ACM PDFs). ACM DOIs return 403 from this box; the Second Life fitted-mesh wiki returns 403. I2 already marks several research licences as “not checked”.
3. I2’s own Sources disagree items (TailorNet 1–2 ms vs ≈20 ms; Destiny surname; KCD “17-4770” OCR) are all correct.

**Dead links:** no true 404s among I2’s cited URLs.
- ACM / Wiley DOIs: 403 (paywall).
- `https://wiki.secondlife.com/wiki/Mesh/Rigging_Fitted_Mesh`: 403 from this box.
- `https://graphics.berkeley.edu/papers/Wang-…` fails default TLS (certificate name mismatch); the PDFs return 200 with `curl -k` and are the copies I read.
- I2’s cited KCD source is the Archive.org djvu OCR; the official 39-page slide PDF is also on that item (`GDCEU2015-Barak.pdf`) and is a better primary.

## Close checks
1. **KES units are “by convention”.** Ramaiah, Radhalakshmi & Nachane PDF (https://static.fibre2fashion.com/articleresources/PdfFiles/60/5963.pdf, 14 pages, created 2011-11-24): Table 3 prints **no units**. All 16 value rows match I2 exactly (including LC B = 0.3141). Population matches Table 2 (A knit 100% cotton innerwear; B knit 95/5 cotton/polyurethane — the PDF misspells “polyurethrane”; C woven shirting 100% cotton; D woven suiting 100% cotton, dyed, unfinished). “Each value represents the mean of 10 tests on five different test specimens.” Conditioning is printed as “65 ± 2 %, RH 27 ± 10 C”, which is garbled, as I2 says; no clean temperature was recovered. Kato Tech’s KES-FB2 datasheet (https://english.keskato.co.jp/contents/EN_FB2-A.FB2-S.FB2-SH.FB2-V.pdf) confirms the usual units I2 lists for bending: **B = gf·cm²/cm**, **2HB = gf·cm/cm**. Shear-angle labels “0.5 deg” / “5 deg” *are* in Table 3’s property names. Licence: no reuse licence on the fibre2fashion PDF; I2’s **[NO-LICENCE]** is right.

2. **TailorNet speed: 1–2 ms vs ≈20 ms.** Both figures are the authors’ own, in different places.
   - Paper (arXiv:2003.04583, §6.4): “On a gaming laptop with an NVIDIA GeForce GTX 1060 GPU and Intel i7 CPU, our approach runs from 1 to 2 ms per frame, which is 1000 times faster than PBS it is trained from.” Abstract also has “more than 1000 times faster”.
   - README: “In the paper, we report inference time to be 1-2 ms per frame … averaged … over the batch of 21 samples (20-40 ms per batch). Apologies for the ambiguity. Running each sample separately takes almost same time as batch - around 20 ms per frame for all garments. … TailorNet has 21 independent MLPs.”
   - I2 is right to tell a user to use **20 ms** for interactive single-character use. C4’s “Authors’ GPU” is the GTX 1060 (with an i7 also stated).

3. **Licences: CAPE, TailorNet and SMPL are NC; DrapeNet is GPL-3.0.**
   - CAPE https://cape.is.tue.mpg.de/license.html: “non-commercial scientific research, non-commercial education, or non-commercial artistic projects”; any commercial use “is prohibited”, including “production of other artefacts for commercial purposes”; commercial training banned.
   - TailorNet LICENSE.md (© 2019 Gerard Pons Moll, MPG): the same NC grant and commercial prohibition, plus a citation requirement.
   - SMPL https://smpl.is.tue.mpg.de/modellicense.html: the same MPI/MPG NC grant (“non-commercial scientific research, non-commercial education, or non-commercial artistic projects”); commercial use prohibited under the free licence.
   - DrapeNet: GitHub API `gpl-3.0`; repo LICENSE is the GPL-3.0 text; README requires the SMPL model from `smplify.is.tue.mpg.de`. I2’s “GPL-3.0 but still needs SMPL [NC]” is exact.
   - PBNS: GitHub API licence `null`, no LICENSE file (404). I2 **[NO-LICENCE]** is right.
   - Robust Skin Weights Transfer: MIT, © 2024 Rinat Abdrashitov, as I2 says.

4. **Kingdom Come timings 110 / 365 / 879 ms.** Confirmed on the **official slide images** (Archive.org item `GDCEU2015Barak`, file `GDCEU2015-Barak.pdf`, 39 pages, Tomas Barak / Warhorse, 2015), not only the djvu OCR I2 cited.
   - Slide “Performance / Simple setup”: **i7-4770 @ 3.4 GHz**; BODY, CLOTH; **207,312 rays; 110 ms; 1.88 M rays/s**.
   - Advanced: BODY, CLOTH, CHAIN; **417,400 rays; 365 ms; 1.14 M rays/s**.
   - Full: BODY, CLOTH, CHAIN, PLATE, DECO; **770,028 rays; 879 ms; 0.88 M rays/s**.
   - The djvu OCR reads the CPU as “17-4770”; the slide image is clearly **i7-4770**. I2 already says this.
   - Arithmetic: 207312/0.110 = 1.8847×10⁶ → 1.88; 417400/0.365 = 1.1436×10⁶ → 1.14; 770028/0.879 = 0.876×10⁶ → 0.88. All match.
   - Method (base + shrink morph, W=1 none / W=0 max, bind T-pose, n random barycentric samples, hash grid, alpha mask, vertex-edge smoothing, fat/slim variation morphs, layer order body → cloth → chain mail → plate → decoration, influence matrix) and the drawbacks (“only layered shapes”; one draw call per piece; “not instant” / “fraction of second”) are in the same deck.

## Row table

| Row | Claim (I2) | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| G3 / C1 | KCD adaptive clothing method and 110/365/879 ms timings; i7-4770 | VERIFIED | Official PDF slides (images) + djvu OCR. See close check 4. | https://archive.org/download/GDCEU2015Barak/GDCEU2015-Barak.pdf ; I2’s stream URL also 200 for the OCR |
| G1, G2, G4–G11 | Sims 4 DMAPs; Destiny wrap/blend-shapes dropped; Roblox cages; Second Life fitted mesh; MetaHuman; Cyberpunk garment support; LoL; APB; Brennan/Havok cloth; Ubisoft AC Unity cloth | **UNVERIFIABLE** | URLs mostly 200 (GDC Vault, YouTube, Roblox docs, Epic, redmodding, Brennan PDF, Vaisse OCR). Second Life wiki 403. I did not re-open the talk videos or slide bodies this pass, so the method wording was not re-checked. | as cited in I2 |
| R4 | CAPE [NC] | VERIFIED | Close check 3. | https://cape.is.tue.mpg.de/license.html |
| R5 / C4 / C7 | TailorNet [NC]; 1–2 ms paper vs ≈20 ms README; “more than 1000 times faster” | VERIFIED | Close check 2–3. | arXiv PDF ; GitHub README + LICENSE.md |
| R8 / C6 | GarNet: <1 cm mean point error, 100× faster than PBS | VERIFIED | arXiv v3 abstract, exact wording. Licence still “not checked”, as I2 says. | https://arxiv.org/abs/1811.10983 |
| R9 | DrapeNet GPL-3.0; SMPL [NC] dependency | VERIFIED | GitHub API + LICENSE + README. | https://github.com/liren2515/DrapeNet |
| R11 / C5 | PBNS [NO-LICENCE]; “over 14.000 FPS (for an outfit with 24K triangles!)”; “a few MBs”; “a few minutes, even without a GPU” | VERIFIED | README wording is exact. GitHub licence null; LICENSE 404. Hardware still not stated in the README, as I2 says. | https://github.com/hbertiche/PBNS |
| R12 | Robust Skin Weights Transfer code is MIT | VERIFIED | LICENSE is MIT, © 2024 Rinat Abdrashitov. ACM DOI 403. | https://github.com/rin-23/RobustSkinWeightsTransferCode |
| R13 / C2 | Wang 2010 wrinkle synthesis timings | VERIFIED | Author PDF Table 1: GPU wrinkle 0.009s / 0.008s; total 0.084s / 0.086s; 11.93 / 11.65 FPS; 500-frame averages; male 46k, female 34k vertices; Dell T7500 dual quad-core 2.26 GHz + Quadro FX 5800. Fig. 1: 25k-vertex reference “2-3 minutes per frame”. Table is with the timing text on p. 6 of the paper. | https://graphics.berkeley.edu/papers/Wang-EBW-2010-07/Wang-EBW-2010-07.pdf |
| R1–R3, R6, R7, R10, R14 | Other research method rows (ACM or arXiv; several “licence not checked”) | **UNVERIFIABLE** | ACM DOIs 403. I2 already leaves those licences unread. Claims not re-read from full text. | as cited |
| KES LT…W (16) | Table 3 values; no units in the table; mean of 10 tests on 5 specimens; 65±2% RH; [NO-LICENCE] | VERIFIED | All 16 numbers match. See close check 1. | fibre2fashion PDF |
| Wang 2011 (10 fabrics + range + 24 h cost) | Table 1 compositions, uses, kg/m², mm error; ~24 h = 2+1+2+19 h on four Xeon X7560; p. 7; no parameter-database licence | VERIFIED | Every Table 1 cell I2 prints matches the author PDF (article page 71:7). Range 0.113–0.324 kg/m² and 0.89–3.24 mm recomputes. Cost sentence is verbatim. Supplemental parameter tables were not opened, as I2 says. | https://graphics.berkeley.edu/papers/Wang-DDE-2011-08/Wang-DDE-2011-08.pdf |
| C3, C8 | Ubisoft cloth cost split; Roblox “same or better” performance | **UNVERIFIABLE** | Vaisse OCR and Roblox forum URLs resolve (200). Wording not re-checked against the OCR/post body this pass. | as cited |
| SD TailorNet | 1–2 ms vs ≈20 ms | VERIFIED | Close check 2. | README |
| SD Destiny surname | Shepherd (Polygon) vs Shepard (auto-transcript) | not re-opened | Left as I2 stated; not counted in the 59. | |
| SD KCD CPU | OCR “17-4770” = i7-4770 | VERIFIED | Slide image shows i7-4770. | official PDF p. 34–36 |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| KCD official slides | https://archive.org/download/GDCEU2015Barak/GDCEU2015-Barak.pdf | 200, 39-page PDF | Yes: [CLOSED] talk © Warhorse |
| KCD djvu OCR (I2’s URL) | https://archive.org/stream/GDCEU2015Barak/GDCEU2015-Barak_djvu.txt | 200 as text via /download/ | Yes; OCR of “i7” is “17”, already flagged |
| Ramaiah et al. KES PDF | fibre2fashion …/5963.pdf | 200 | Yes: [NO-LICENCE] |
| Kato Tech KES-FB2 (units check) | english.keskato.co.jp/…/EN_FB2-A…pdf | 200 | Manufacturer ©; used only for conventional units |
| Wang 2011 author PDF | graphics.berkeley.edu/…/Wang-DDE-2011-08.pdf | 200 with TLS workaround | Yes: paper © ACM; no dataset licence, as I2 says |
| Wang 2010 author PDF | graphics.berkeley.edu/…/Wang-EBW-2010-07.pdf | 200 with TLS workaround | Yes: paper © ACM |
| TailorNet paper | https://arxiv.org/pdf/2003.04583 | 200 | Paper © authors/IEEE; code is NC |
| TailorNet LICENSE.md / README | GitHub raw | 200 | Yes: [NC] |
| CAPE licence | https://cape.is.tue.mpg.de/license.html | 200 | Yes: [NC] |
| SMPL model licence | https://smpl.is.tue.mpg.de/modellicense.html | 200 | Yes: [NC] |
| DrapeNet repo | https://github.com/liren2515/DrapeNet | 200; API licence GPL-3.0 | Yes: [COPYLEFT] + SMPL [NC] |
| PBNS repo | https://github.com/hbertiche/PBNS | 200; no licence | Yes: [NO-LICENCE] |
| RSWT LICENSE | GitHub raw | 200 | Yes: MIT |
| GarNet arXiv | https://arxiv.org/abs/1811.10983 | 200 (v3) | Licence not checked, as I2 says |
| ACM DOIs in I2 | dl.acm.org/… | 403 | I2’s “Paper ACM” is the right default |
| Second Life wiki | wiki.secondlife.com/…Fitted_Mesh | 403 | I2 licence “platform feature” not re-checked |
| Roblox / Epic / GDC Vault / YouTube / Polygon | as cited | 200 unless noted | Game methods [CLOSED], as I2 says |

I2’s §6 licence summary is accurate: MPI/SMPL-family models are NC; DrapeNet is GPL-3.0 but still needs SMPL; KES and Wang 2011 numbers may be cited as facts but the PDF/dataset should not be copied into a repo; game methods are closed; no patent search was done.

## Grok
A live Grok CLI pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`, live search on) covering I1–I3 together was started at **10:17:18 AEDT**. This I2 file was written from sources I opened myself while that process was still running. No number above was taken from Grok. The pass will be summarised once it exits.

# Verification of A7-skeleton.md

Checked 2026-11-16 (AEDT, UTC+11) by Library Verify's checker. The researcher's file was not edited.

**File checked:** mtime 2026-11-16 10:30:00 +1100, 171 lines, 19383 bytes, recorded before checking. Re-measured when this report was written: 171 lines, 19383 bytes (unchanged = yes).

## Summary

**Rows: 65 total: 52 VERIFIED, 13 FIX, 0 UNVERIFIABLE.**

- **Kolesova female n — decision: 212 is right; "122" in the methods is a typo.** Evidence: (1) the abstract, the OJS landing page and the Table 1 header all say 212. (2) **Back-calculating the paper's own t statistics from Table 1 means/SDs fits n = 212, not 122.** With n_F = 212 the Welch t is bispinous 20.50, bituberous 20.66, biacetabular 10.66, mid-2 AP 7.33, mid-1 AP 3.85, outlet AP 4.27; the printed values are 20.85, 20.72, 10.63, 7.22, 3.87, 4.22. With n_F = 122 they drop to 17.6, 17.6, 9.1, 6.2, 3.3, 3.6. (3) The authors' 2017 paper on the same Gailezers archive and the same October 2009 – November 2010 window reports **211 females** aged 18–84 (mean 48.3, SD 18.3) and 181 males (Kolesova, Kolesovs & Vetra, Anat Cell Biol 50:265–274, https://doi.org/10.5115/acb.2017.50.4.265). Keep it under "Sources disagree", but note that 212 is the best-supported value. No erratum was found.
- Side finding (Kolesova, not in the file's numbers): the inlet AP t is printed 5.39, but it back-calculates to about 4.75 with either n. The inlet-transverse Levene F is printed 5.34* in Table 1 but given as 5.54 (p < .01) in the text. These are the paper's own inconsistencies; they don't change the file's values.
- **Page FIXes:** cervicale should be pp. 87–88, not 88–89. The A7-3 head and face rows also need correcting: head length 137–138, head breadth 133–134, head circumference 135–136, menton–sellion 165–166, tragion–top of head 207–208. In the A7-2c and A7-5 prose: bicristal 63–64, chest breadth 89–90, chest depth 93–94. Acromion, suprasternale, tenth rib, iliocristale, trochanterion, wrist, epicondyle, malleolus, bizygomatic, interpupillary and biacromial pages are already correct.
- One rounding FIX: tenth-rib/stature for females is 0.648, not 0.649.
- All values match: A7-2a (all 10 Kolesova rows against the PDF Table 1, p. 213), A7-2b (Mohd Ali Table 2 and every as-cited Table 4 row), A7-3 (cephalic index 77.9/77.4 and bizygomatic/head breadth 0.906/0.924 recomputed) and A7-4 (all 20 rows against Hona & Stephan Table 3, total weighted mean, n and s).
- **Licence findings:**
  - Mohd Ali 2020 is **CC BY-NC 4.0**. The file states this correctly (the PMC XML and Crossref agree).
  - **Hona & Stephan 2023 is CC BY 4.0** (Crossref licence metadata; Grok quoted the Springer PDF licence line). The file says "unconfirmed", so this can now be confirmed.
  - **Weaver 2014 carries no CC licence.** It is "bronze" OA with © Anatomical Society and Wiley terms (Unpaywall API and Crossref). The file says "not checked".
  - Kolesova has no licence statement on OJS or in Crossref. The file is correct.
- "Sources disagree" subpubic-angle sentence is imprecise. The AP-radiograph studies in Mohd Ali's Table 4 give M 75.8–101.5° and F 93.5–140.0° (Young & Ince 75.8/93.5 is an AP-radiograph study). The file's 94–106 M / 116–140 F range mixes in Oladipo's "projection images" and leaves out Young & Ince.
- Blocked links:
  - doi 10.12697/poa.2011.20.21 returns 500 to curl. The landing page opens via WebFetch and the OJS PDF opens (200).
  - doi 10.5115/acb.19.135 redirects (302) to acbjournal.org, which opens via WebFetch.
  - doi 10.1111/joa.12203 returns 403 (Wiley). The abstract is available via the EBI REST API; the PMC4111931 page opens (200).
- Structure check: rows have URLs; access date only in header. Not found / Sources disagree present.

### Grok live-search pass

Ran 2026-11-16 ~10:47–10:53 AEDT: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` (live web search on), single 10-question prompt; waited until the process exited (exit 0). A Grok answer counted only where it gave a URL that I then opened myself; no number was taken from Grok alone.

For A7: Grok confirmed the Kolesova PDF contains both 212 (abstract/table) and 122 (methods), found no erratum, and pointed to the 2017 follow-up (211 F / 181 M, same archive and period), which I opened and confirmed. It confirmed Mohd Ali CC BY-NC 4.0 and 68.6 ± 7.6 / 87.4 ± 6.5, n 50/50 (opened by me), Hona & Stephan CC BY 4.0 (I confirmed via Crossref), and Weaver bronze OA / no CC licence (I confirmed via Unpaywall API). **Conflicts raised: Hona licence (file: unconfirmed → CC BY 4.0) and Weaver licence (file: not checked → no CC).**

## Row table

L = line number in the researcher's file. Status: VERIFIED = re-read or recomputed from the source and matches; FIX = correction needed (corrected value given); UNVERIFIABLE = could not be re-read (reason given).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| L11 | Cervicale (spine of the 7th cervical vertebra, the most prominent neck bump) ; 139.57 ± 5.95 ; 151.73 ± 6.33 ;… | FIX | Values match the report. Page FIX: printed pp. 87–88, not pp. 88–89 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L12 | Acromion (lateral tip of the shoulder blade's acromial process) ; 133.51 ± 5.81 ; 144.07 ± 6.33 ; pp. 47–48 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L13 | Suprasternale (bottom of the jugular notch of the sternum) ; 132.97 ± 5.72 ; 143.86 ± 6.12 ; pp. 195–196 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L14 | Tenth rib (lowest point of the right 10th rib, the bottom edge of the rib cage) ; 105.63 ± 5.00 ; 112.07 ± 5.3… | FIX | Heights 105.63 ± 5.00 / 112.07 ± 5.36 ✓ (pp. 197–198 ✓). Per-person ratio F = **0.648** (0.6485), not 0.649; SD 0.011 ✓; M 0.638 (0.012) ✓. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L15 | Iliocristale (highest palpable point of the iliac crest) ; 99.57 ± 4.97 ; 106.16 ± 5.20 ; pp. 147–148 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L16 | Trochanterion (top of the femur's greater trochanter) ; 84.54 ± 4.47 ; 90.09 ± 4.92 ; pp. 209–210 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L17 | Stylion level, i.e. wrist height (bottom tip of the radius) ; 79.42 ± 3.82 ; 84.75 ± 4.17 ; pp. 229–230 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L18 | Lateral femoral epicondyle (knee pivot point) ; 46.59 ± 2.71 ; 49.17 ± 2.66 ; pp. 159–160 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L19 | Lateral malleolus (outer ankle bone) ; 6.27 ± 0.51 ; 7.29 ± 0.57 ; pp. 161–162 | VERIFIED | Heights match report; per-person ratios recomputed from CSVs match. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L31 | Inlet AP (top back of the pubic symphysis to the sacral promontory) ; 119.23 (10.16) ; 124.18 (10.26) ; F > M,… | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). | https://doi.org/10.12697/poa.2011.20.21 |
| L32 | Inlet transverse (widest span between the iliopectineal lines) ; 126.82 (7.49) ; 135.08 (8.49) ; F > M, p < .0… | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L33 | Mid-plane 1 AP (pubic symphysis to S2/S3) ; 127.78 (8.98) ; 131.46 (9.84) ; F > M | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L34 | Mid-plane 1 biacetabular (between acetabulum centres) ; 113.82 (7.21) ; 122.23 (8.33) ; F > M | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L35 | Mid-plane 2 AP (lower symphysis to S4/S5) ; 116.48 (7.49) ; 122.52 (8.74) ; F > M | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L36 | Bispinous (narrowest gap between the ischial spines) ; 93.64 (8.64) ; 112.31 (9.27) ; largest difference, t = … | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L37 | Outlet AP (lower symphysis to coccyx tip) ; 95.97 (7.38) ; 99.69 (9.78) ; F > M | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L38 | Bituberous (between the inner ischial tuberosities) ; 103.48 (9.33) ; 124.18 (10.39) ; t = 20.72 | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L39 | AP/transverse index: inlet ; 0.94 (0.08) ; 0.93 (0.10) ; not significant | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L40 | AP/transverse index: outlet ; 0.94 (0.11) ; 0.81 (0.11) ; F outlet relatively wider | VERIFIED | Matches Kolesova Table 1 (PDF p. 213). |  |
| L48 | Malaysian adults aged 20–79, 3D CT ; 50 / 50 ; 68.6 ± 7.6 (range 50.8–84.6) ; 87.4 ± 6.5 (range 74.0–106.5) | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). | https://doi.org/10.5115/acb.19.135 |
| L49 | Australian Caucasian, 3D CT (Franklin et al.) ; 200 / 200 ; 69.0 ± 7.3 ; 89.4 ± 7.5 | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). |  |
| L50 | Anatolian Caucasian, 3D CT (Karakas et al.) ; 66 / 43 ; 65.9 ± 7.2 ; 82.6 ± 7.7 | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). |  |
| L51 | American, 3D CT (Decker et al.) ; 40 / 60 ; 71.4 ± 7.8 ; 82.9 ± 5.7 | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). |  |
| L52 | Japanese cadavers, 3D CT (Torimitsu et al.) ; 104 / 104 ; 74.8 ± 9.58 ; 112.7 ± 10.0 | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). |  |
| L53 | American White, Hamann-Todd collection, projection (Tague) ; 50 / 50 ; 63.7 ± 7.8 ; 88.4 ± 12.3 | VERIFIED | Matches Mohd Ali Table 2 / Table 4 (PMC XML). |  |
| L67 | Head length (glabella to opisthocranion) ; 18.98 ± 0.74 ; 19.95 ± 0.70 ; pp. 138–139 | FIX | Values match the report. Page FIX: printed pp. 137–138, not pp. 138–139 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L68 | Head breadth (euryon to euryon) ; 14.78 ± 0.52 ; 15.43 ± 0.55 ; pp. 134–135 | FIX | Values match the report. Page FIX: printed pp. 133–134, not pp. 134–135 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L69 | Head circumference ; 56.11 ± 1.94 ; 57.44 ± 1.60 ; pp. 136–137 | FIX | Values match the report. Page FIX: printed pp. 135–136, not pp. 136–137 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L70 | Bizygomatic breadth (cheekbone width) ; 13.38 ± 0.56 ; 14.26 ± 0.62 ; pp. 73–74 | VERIFIED | Values match report / CSV recomputation. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L71 | Menton–sellion length (face height, chin to nasal root) ; 11.31 ± 0.61 ; 12.26 ± 0.67 ; pp. 166–167 | FIX | Values match the report. Page FIX: printed pp. 165–166, not pp. 166–167 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L72 | Interpupillary breadth ; 6.17 ± 0.36 ; 6.40 ± 0.34 ; pp. 149–150 | VERIFIED | Values match report / CSV recomputation. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L73 | Tragion to top of head ; 12.65 ± 0.65 ; 13.11 ± 0.62 ; pp. 208–209 | FIX | Values match the report. Page FIX: printed pp. 207–208, not pp. 208–209 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L74 | Cephalic index, 100 × breadth / length (computed) ; 77.9 (SD 3.7) ; 77.4 (SD 3.6) ; n/a | VERIFIED | Values match report / CSV recomputation. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L75 | Bizygomatic / head breadth (computed, mean) ; 0.906 ; 0.924 ; n/a | VERIFIED | Values match report / CSV recomputation. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L83 | Vertex (top of the skull) ; 5.0 ; 2272 ; 1.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L84 | Opisthocranion (back of the skull) ; 6.0 ; 2273 ; 2.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L85 | Glabella (between the brows) ; 5.5 ; 15,937 ; 1.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L86 | Nasion to sellion (root of the nose) ; 6.0 ; 16,300 ; 1.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L87 | Rhinion (end of the nasal bones) ; 3.0 ; 15,401 ; 1.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L88 | Subspinale to subnasale (below the nose) ; 13.5 ; 8897 ; 3.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L89 | Upper lip (prosthion to labrale superius) ; 12.0 ; 14,894 ; 3.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L90 | Lower lip (infradentale to labrale inferius) ; 13.5 ; 14,425 ; 3.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L91 | Supramentale (chin groove) ; 11.0 ; 15,238 ; 2.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L92 | Pogonion (front of the chin) ; 11.0 ; 17,075 ; 2.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L93 | Gnathion ; 7.5 ; 3259 ; 2.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L94 | Menton (under the chin) ; 7.0 ; 13,583 ; 2.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L95 | Mid-supraorbital (above the eye) ; 7.0 ; 8966 ; 2.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L96 | Mid-infraorbital (below the eye) ; 6.5 ; 8966 ; 3.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L97 | Zygion (cheekbone arch) ; 7.5 ; 10,399 ; 3.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L98 | Gonion (jaw angle) ; 12.5 ; 9827 ; 6.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L99 | Mid-ramus (side of the jaw) ; 19.5 ; 6344 ; 5.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L100 | Over the upper second molar (cheek) ; 26.0 ; 6203 ; 7.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L101 | Over the lower second molar ; 22.0 ; 4756 ; 6.5 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L102 | Mid-mandibular border ; 11.0 ; 4573 ; 4.0 | VERIFIED | Matches Hona & Stephan Table 3 (total weighted mean, n, s). |  |
| L27 (population) | Kolesova females n = 212 (abstract/table) vs 122 (methods) | VERIFIED | 212 best supported (t back-calculation + 2017 paper on same archive: 211 F). Also add female ages 18–84, mean 48.1 (SD 18.3) from methods; males 18–82, mean 43.6. | https://ojs.utlib.ee/index.php/PoA/article/download/poa.2011.20.21/654 |
| L7 (landmarks) | ANSUR Section 2.6, pp. 18–29; 6068 soldiers 17–58 | VERIFIED | Section 2.6 starts printed p. 18; the landmark list runs pp. 20–30 (zygofrontale etc. on p. 30). n 6068 ✓. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L55 (prose) | Mohd Ali: cut point 78.6°, 94 % correct; r ≈ −0.4 with age | VERIFIED | 78.6°, 94 %, r = −0.449 M / −0.385 F ✓; ages M 20–78, F 20–79 ✓. | https://doi.org/10.5115/acb.19.135 |
| L59 (prose) | Bicristal F 27.33 ± 2.23 / M 27.54 ± 1.75 (pp. 64–65); biacromial 36.53 ± 1.83 / 41.57 ± 1.92 (pp. 59–60); ratio 0.749 (0.062) / 0.663 (0.039) | FIX | All values ✓. Bicristal page FIX: printed pp. 63–64. Biacromial pages correct. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L108 (prose) | Chest breadth 26.93 ± 1.86 / 28.94 ± 1.83 (pp. 90–91); chest depth 24.74 ± 2.73 / 25.38 ± 2.62 (pp. 94–95); depth/breadth 0.918 (0.076) / 0.877 (0.075) | FIX | All values ✓. Page FIX: chest breadth pp. 89–90, chest depth pp. 93–94. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L109 (prose) | Rib cage spans 0.816–0.819 → 0.638–0.649 H | FIX | Use 0.638–0.648 (F tenth-rib ratio is 0.648); span ~0.17–0.18 unchanged. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L110 (prose) | Weaver 2014: CT of 339 subjects aged 0–100; qualitative shape trends | VERIFIED | Abstract confirms 339 subjects, 0–100 y; paraphrase consistent. | https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=DOI:10.1111/joa.12203&resultType=core&format=json |
| L104 (prose) | Sex differences in FSTT typically < 1 mm | VERIFIED | Paper: "typically in the order of < 1 mm". | https://link.springer.com/article/10.1007/s00414-023-03087-x |
| L144 (licence) | Hona & Stephan licence unconfirmed | FIX | CC BY 4.0 (Crossref licence metadata; Springer "Open access", CAUL-funded). | https://api.crossref.org/works/10.1007/s00414-023-03087-x |
| L145 (licence) | Weaver 2014 licence not checked | FIX | No CC licence: bronze OA, © 2014 Anatomical Society / Wiley terms (Unpaywall, Crossref). | https://api.unpaywall.org/v2/10.1111/joa.12203?email=unpaywall@impactstory.org |
| L168 (disagree) | AP radiograph subpubic ≈ 94–106 M / 116–140 F | FIX | Mohd Ali Table 4 AP-radiograph rows: M 75.8–101.5, F 93.5–140.0 (incl. Young & Ince 75.8/93.5); "projection images" rows (Oladipo) 100.1–105.6 M / 119.4–125 F are a separate method. | https://pmc.ncbi.nlm.nih.gov/articles/PMC7118257/ |

## Source table

| Key | URL | Resolves (y/n) | Licence statement correct (y/n) | Note |
|---|---|---|---|---|
| Gordon 2014 (ANSUR II report) | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf | y (206) | y | "Approved for public release; distribution is unlimited" confirmed on the report. |
| ANSUR II public CSVs | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv ; …MALE_Public.csv | y (206) | y | Downloaded to /workspace/tmp, recomputed, then deleted. CSVs = unweighted working sample, F 1986 / M 4082, ages 17–58. |
| OpenLab host page | https://www.openlab.psu.edu/ansur2/ | y (200) | n/a |  |
| Kolesova & Vētra 2011 DOI | https://doi.org/10.12697/poa.2011.20.21 | n (500 to curl); landing page y via WebFetch https://ojs.utlib.ee/index.php/PoA/article/view/poa.2011.20.21 | y | No licence statement found (correct). |
| Kolesova PDF (OJS) | https://ojs.utlib.ee/index.php/PoA/article/download/poa.2011.20.21/654 | y (200, PDF) | y |  |
| Kolesova et al. 2017 (cross-check, new) | https://doi.org/10.5115/acb.2017.50.4.265 | y (acbjournal.org via WebFetch) | n/a | Same archive and period; 211 F / 181 M. |
| Mohd Ali et al. 2020 | https://doi.org/10.5115/acb.19.135 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC7118257/ | y (302 → acbjournal.org; PMC 200) | y | CC BY-NC 4.0 – NON-COMMERCIAL, flagged correctly. |
| Hona & Stephan 2023 | https://doi.org/10.1007/s00414-023-03087-x ; …/tables/3 | y | n (file says unconfirmed; it is CC BY 4.0) | Vol 138, pp. 519–535 per Crossref (missing from file citation). |
| Weaver et al. 2014 | https://doi.org/10.1111/joa.12203 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4111931/ | DOI n (403 Wiley); PMC y | n (not checked; it is bronze OA, no CC) |  |

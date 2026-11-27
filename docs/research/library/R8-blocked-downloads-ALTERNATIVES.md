# Blocked downloads and substitutes (round 8)

Compiled 2026-11-27. There is one row for each listed source that was missing, unreadable, thin, or of unclear legal status, and one row for each substitute used in its place. These are the routes tried:
- the publisher page;
- author and university pages;
- Semantic Scholar and Crossref;
- arXiv;
- Internet Archive / Wayback (`id_` URLs; Wayback refused TLS from the box for part of this session).

No sign-ins, captchas, or pirate sites were used. Copies of unclear status are flagged and were read for content only, always alongside a clearly legal source.

Status key:
- **Exact**: the listed item was obtained.
- **Substitute**: a different free source covering the same content.
- **Unclear**: a copy was read but its legality is unclear.
- **Blocked**: no free copy was obtained.

## Section A

| Item | Listed source | Status | What was used instead (labelled in the file) |
|---|---|---|---|
| A1 S2a | Aichholzer & Aurenhammer, COCOON '96, LNCS 1090: 117-126 | Blocked (Springer) | **Substitute S2b**: the authors' own 14-page PostScript version (aa-ssgpf-98.ps.gz, via Wayback from Aurenhammer's TU Graz page). Its pages differ from the LNCS pages |
| A2 FO | Felkel & Obdržálek, SCCG '98: 210-218 | Unclear. The original cgg.cvut.cz URL is gone, with no Wayback snapshot. The readkong/phlatforum uploads are of unclear status and have no reliable page breaks | **Sub-1**: Eder, Held & Palfrader, SoCG 2020, LIPIcs (CC BY). **Sub-2**: Wang, MIT 6.849 report. FO's errors are also cross-checked against Huber's thesis (H11) |
| A2 | Yakersberg (2004), MSc thesis, Technion (cited by H11 p. 27) | Blocked | Not substituted. Known only through H11's citation |
| A3 L1 | Held (1998), CAD 30(4): 287-300 | Blocked (author links 404, no snapshot; Elsevier paywall) | **S2**: Held & Huber 2009, CAD (an author copy; legality unclear, flagged as WEAK). **S1**: Palfrader & Held 2015, CAD&A |
| A3 L2 | Held (2001), VRONI, CGTA 18(2): 95-123 | Blocked from the box (Elsevier open archive not reachable by script; the fetched "PDFs" were HTML error pages, kept as `*.FAILED-not-a-pdf.html`) | S2 (Held & Huber 2009) for the VRONI method; **S4**: W3C SVG 2 for the mitre limit |
| A4 L1 | Guibas & Marimont (1998), IJCGA 8(2): 157-176; SoCG '95 | Blocked (World Scientific / ACM paywalls) | Its properties are cited second-hand via de Berg-Halperin-Overmars (UU-CS-2004-055) and Halperin-Packer |
| A4 S2 | Halperin & Packer (2002), Iterated snap rounding, CGTA 23(2) | Unclear (University of Trier seminar copy) | **S2b**: Packer's MSc thesis, TAU (institutional copy) |
| A4 | Hershberger (2013), Stable snap rounding, CGTA 46(4) | Blocked | Not substituted. Named as the follow-up to the SR/ISR trade-off |
| A4 | Goodrich, Guibas, Hershberger & Tanenbaum (1997), SoCG | Blocked | Known only through de Berg et al. p. 2 |
| A4 (fold-over) | No listed source | n/a | **S5**: Garland PhD thesis, CMU (1999). **S8**: Floater (journal page = PDF + 684) |
| A6 L1 | Held (2001), FIST, Algorithmica 30(4): 563-596 | Blocked. Springer is paywalled; the project page's preprint links are commented out; algo01.ps.gz returns 404; 20 Wayback retries failed | **Sub-1**: Held, CGI'98, pp. 633-643 (same author and code). **Sub-2**: Eberly, "Triangulation by Ear Clipping" (CC BY 4.0) for hole bridging |
| A6 L4 | Lawson (1977), in *Mathematical Software III*: 161-194 | Blocked (book chapter) | **Sub-3**: Shewchuk, *Lecture Notes on Delaunay Mesh Generation* (2012), Theorems 5, 13-16 |
| A7 L1 | Ruppert, J. Algorithms version | Journal version not read | The NASA RNR-94-002 report was used (free, US government). Journal page numbers are not given |

## Section B

| Item | Listed source | Status | What was used instead (labelled in the file) |
|---|---|---|---|
| B1 L2 | Sherstyuk (1999), The Visual Computer 15(4) | Blocked. Springer is paywalled; the Monash PhD thesis (doi:10.26180/14876034) is "In Copyright" with its file embargoed | Kernel forms taken from L3 (Zanni et al., HAL), which restates them |
| B1 | SCALIS ray-tracing follow-up | Download truncated; not read | Not substituted |
| B1 Sub-1 | Hart (1996), Sphere tracing, Vis. Comput. 12(10) | Unclear (preprint on a Stanford course page) | Read for the Lipschitz bound only; flagged WEAK |
| B2 Sub-1 | Custódio et al. (2013), C&G 37(7) | Exact (author copy) | Added as the later bug report on MC33 tables |
| B4 L3 | Dionne & de Lasa (2013), SCA | Blocked (ACM; no open copy; a scispace mirror of unclear status returned HTML) | **Sub-1**: Autodesk Maya 2025 Help, Bind Skin Options (Geodesic Voxel) |

## Section C

| Item | Listed source | Status | What was used instead |
|---|---|---|---|
| C1 L1 | Niemeyer, Pietronero & Wiesmann (1984), PRL 52: 1033 | Blocked (APS paywall) | Restated via Kim-Lin 2007 and **S1** Sañudo et al. 1995 (NPG, CC BY-NC-SA 2.5) |
| C1 L2 | Reed & Wyvill (1994), SIGGRAPH 94: 359-364 | Blocked (ACM returned HTML; kept as reed-wyvill94.FAILED-not-a-pdf.html) | **S3** Glassner 2000 (CG&A, author site); **S2** Bickel et al. 2006 (ETH); **S4** Reis & Fernandes 2022 (UMinho) |
| C2 | Briggs originals (1969, 1975) | Not obtained | **L2** EPA ISC3 User's Guide Vol II (public domain) restates them; **S1** Cushman-Roisin ch. 8 |
| C3 L1 | Cetegen & Ahmed (1993), Combust. Flame 93: 157-184 | Blocked (Elsevier; OSTI has the abstract only) | **S2** Taha et al. 2024 (HAL hal-04338538); **S1** NIST FDS Validation Guide (public domain) |
| C3 L2 | Heskestad (1983) | Not read | Correlation taken from the FDS Validation Guide (S1). NUREG-1805 ch. 3 returned "Access Denied" (kept as nureg1805-ch3*.FAILED-not-a-pdf.html) |
| C4 L1 | Hulse-Smith, Mehdizadeh & Chandra (2005), JFS 50(1) | Blocked (ASTM paywall) | **L2** Attinger et al. 2013 (Iowa State repository) restates the results; **S1** Laan et al. 2014 (UvA-DARE) |
| C5 | ISO 9241-391 (source of the WCAG red-flash note) | Blocked (ISO sale) | WCAG 2.2 Understanding doc gives the working definition |
| C5 | Harding FPA / Trace PEAT internals | Not published | **S1** EA IRIS README (BSD-3-Clause), an open analyser's documented logic |

## Section D

| Item | Listed source | Status | What was used instead |
|---|---|---|---|
| D1 | Gibbs (1732), *Rules for Drawing* | Exact (Internet Archive scan of the 1732 first edition; public domain). The OCR drops fractions, so they were read from page images | **S1** Ware, *The American Vignola* Pt I (1902; 1920 printing; public domain) as a second baluster rule and for moulding names |
| D1 | "Any open measured account of the classical baluster's parts" | Not found (no measured survey located) | Pattern-book rules only (Gibbs, Ware), with their disagreement given |
| D2 | "An open account of a tower bell's profile" (nothing listed) | Chosen | **S1** Grimthorpe, *Clocks, Watches & Bells*, 8th ed. 1903 (Gutenberg #17576); **S2** EB 1911 "Bell" (Wikisource). An open modern true-harmonic profile table was not found |
| D3 | "Any open design or archaeological typology" (nothing listed) | Chosen | **S1** Met Collection API (CC0) object measurements; **S2** Parpinello et al. 2018 (CC BY; MDPI blocked direct, got via mdpi-res.com); **S3** ISO 3591 via Wikipedia (the standard itself is sold; the ANSI preview was behind a Cloudflare challenge); **S4** Attwood et al. 2012 (CC BY) |
| D3 | MPRG (1998), *Guide to the Classification of Medieval Ceramic Forms* | Blocked from the box: the MPRG site and the ADS returned bot-check pages, and Wayback refused TLS. Kept as mprg1998-forms.FAILED-not-a-pdf.html, ads-mprg.FAILED-cloudflare.html, mprg1998-forms.FAILED-empty.pdf | Not substituted; the Met data was used instead. The guide is free, so it could be fetched in a normal browser if wanted |
| D3 | Beaudry et al. (1983), POTS, *Historical Archaeology* 17(1): 18-39 | Blocked (Springer paywall) | Not substituted |

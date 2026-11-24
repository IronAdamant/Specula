# Verify: R7-3-primary-sources.md

Independent second pass, 2026-11-24. Method: every number in the file was re-matched as an exact string against the saved text of its source (PDF text layer, article XML or captured HTML), not against my notes; citations and licences were re-read from Crossref or the article; computed values were recomputed. Verdicts: VERIFIED (string found in the source as stated), FIX (error found and corrected in the file), WEAK (only a search-engine extract was seen), UNVERIFIABLE (source not readable). Rows marked Not found / Not attempted in the file carry no numbers and are not listed.

| Row | Verdict | What was checked |
|---|---|---|
| W1 | VERIFIED | AL21: "almost exactly 10 meters", "5.2 to 25.8 meters", "5.239–25.81 m (settled on 11.43 m)", "approximately 4.5 meters", "9.64 m (Witton". Licence CC BY-NC-ND 4.0 from Crossref. Page number: none in the HTML, so the file now says so (FIX of a guessed page range) |
| W2 | VERIFIED | "30–440 kg (Greenewalt", "5.25 m (Greenewalt" |
| W3 | VERIFIED | "200–250 kg (Witton and Habib", "at least 350 kg (Henderson et al", "150 kg (Padian et al" |
| W4 | VERIFIED | "544 kg (Henderson, Citation 2010)"; rero.ch link answered HTTP 410 |
| W8 | VERIFIED (citation) | Crossref: title, *Hist. Biol.* 16(2-4): 71-83, authors Mazzetta, Christiansen, Fariña |
| W10 | VERIFIED | BE14 Table 1 "Argentinosaurus huinculensis a 90,000", footnote "67,400–124,000 kg", "0.135 log 10"; the page's own wikitext cites Benson 2014 and Benson et al. 2018 (doi:10.1111/pala.12329) for "90 and 95 t" (my first draft assumed the 2018 paper; now confirmed from the page). Licence: CC0-type public-domain dedication in the XML |
| W11 | VERIFIED | GR16 "~60.4 metric tons", "~44.9 metric tons", "25.6% of calculated mass"; no "96.4" anywhere in the main-text XML |
| W14, W16, W17 | VERIFIED, FIX | HU11 Table 6 string "Sue 9502 0.801 −0.355 18489 0.572 −0.356 13996"; "Stan 5934", "MOR 5777", "Carnegie 7394"; abstract "6000–8000 kg"; "1790 kg/year", "more than twofold". FIX: author list lacked Claessens; "MOR 555" changed to "MOR" (the specimen number is not in the text I read). BE14 "Tyrannosaurus rex 7,700" |
| W18 | VERIFIED, FIX | LO26 "approximately 75 cm long", "2459 g (95% cPI: 1229–5623 g", "78 bird species", "2.5 kg or less and measured ~700 mm", "1.7 kg in the case of T. rex", "2389 g", "~70 cm". FIX: licence was "to confirm"; Crossref gives CC BY 4.0 |
| W20 to W28 | VERIFIED | Each value matched in the UR text layers, e.g. "Weight including cable 11.2 kg", "Wrist 1 ± 360° ± 360˚/s" (UR3e); "Footprint Ø151 mm", "20,7 kg" (UR5e); "33.3 kg", "Base ± 360° ± 120˚/s" (UR10e); "33,1 kg", "Reach 900 mm" (UR16e); "64 kg", "Elbow ± 360° ± 150˚/s", "± 210˚/s" (UR20); "63.5 kg" (UR30); "Weight: 11 kg", "Ø128 mm" (UR3); "18.4 kg", "Ø149 mm", "All joints: 180°/s" (UR5); "28.9 kg" (UR10). The June 2023 UR10e 180°/s figure is WEAK (search extract) |
| W29 | WEAK, FIX | "28 hydraulically-actuated degrees of freedom" seen in the search extract of the Wayback copy and on drc.csail.mit.edu; the file now says so |
| W30 | WEAK | "about 5' 9" ... weighs 180 lbs": search extract of the YouTube description, quoted identically by IEEE Spectrum, The Robot Report, GeekWire. Conversions recomputed: 1.753 m, 81.6 kg |
| W31, W32, W35 | VERIFIED | BD product pages captured in full: Atlas 1.9 m, 90 kg, 56 DoF, 2.3 m, 50/30 kg, 4 h, IP67; Spot 1100/500/191/610/520-700 mm, 33.8 kg, 1.6 m/s, ±30°, 300 mm, 20.43 kg, 564 Wh; Stretch "up to 50 pounds" |
| W33, W34 | WEAK | DARPA release text via phys.org (search extract) "28.3 mph for a 20-meter split"; Handle description via search extract, quoted the same by three outlets. 28.3 mph = 12.65 m/s recomputed |
| W42 | VERIFIED | legislation.gov.uk Annex I Part A table: "A | RRC ≤ 6,5 | RRC ≤ 5,5 | RRC ≤ 4,0", B to E rows as stated |
| W48 | VERIFIED | atan(100/98.1) = 45.55°; with 9.80665, 45.56° |
| W50 | WEAK | Benchmark speeds 4.292 / 6.024 m/s seen only in the search extract of the authors' preprint (the PDF gave HTTP 500); the page's 5.3 and 8 m/s confirmed in its wikitext as "in this example". Author page "approximately 6 m" sentence verified |
| W52 | VERIFIED | "Length 4,284 mm", "track 1,549 mm", "track 1,519 mm", "0.275", "2.21 m2" |
| W53 | VERIFIED, FIX | "Height 1,298 mm", "Unladen weight (DIN) 1,505 kg", "Track width, front 1,591 mm", "283 kW". FIX: a garbled sentence in the status cell rewritten |
| W54 | VERIFIED | Tesla table: 4,720 / 1,848 / 1,442 / 2,875 mm; tracks 1,584 / 1,584 and 1,582 / 1,560; curb 1760 (856/904), 1823, 1851 kg |
| W55 | VERIFIED | "283.75″ (7.2m)", "41′ 0″ (12.50m)", "40′ 2″ (12.24m)", "10′ 6″ (3.2m)" |
| W57 | VERIFIED | KNDS spec block: "up to 10.97 m", "3.77 m – 4.00 m", "2.64 m", "65 t", "1,500 HP / 1,100 kW", "≤ 70 km/h", "450 km". 1100/65 = 16.9 kW/t recomputed |
| W59 | VERIFIED | NASM: "Wingspan: 12.3 m (40 ft 4 in)", "Empty, 274 kg (605 lb)", "Gross, 341 kg (750 lb)", "12-horsepower", "Open Access (CCO)" |
| W60 | VERIFIED | 3A12: "Rated 160 Horsepower", "Never Exceed 163 Knots IAS (160 Knots CAS)", "MAC 58.8 inches", "July 29, 2015". The 172S 2,550 lb is WEAK (search extract) |
| W61 | VERIFIED | 2A13: "Maximum Weight 2150 lb.", "2700 r.p.m. (150 hp)", "Never exceed 171 mph (148 knots) CAS", "Revision 63" |
| W63 | VERIFIED | A-669: "Take-off 25,200 lbs.", "2700 rpm (1200 hp)", "2550 rpm (1050 hp)" in section II (DC3A-S1CG / S1C3G) |
| W65 | VERIFIED | ACAPS p. 2-4 table and "D6-58325-7 Rev C October 2025"; FAQ "112’ 7” (34.3 m)", "117’ 5” (35.8 m)" |
| W67 | VERIFIED, FIX | EASA TCDS "Span 25.00 m 26.40 m 26.58 m", "Wing area 16.31m² 16.67m2 16.67m2", "Max. Mass 750 kg" (section C), "Max. Mass 810 kg 850 kg" (sections E and F), "VNE 280 km/h", "VA 185 km/h". FIX: added that 850 kg also applies to the BLE 50R |
| W69 | VERIFIED | R22 POH "Diameter 25 feet 2 inches"; maker page "1370 lb (622 kg)"; FAA AC "206B-3 3,200 39.1 10.4 33.3/2", "UH-60L Blackhawk 22,000 64.8 16.8 53.8/4". Disc loadings recomputed: 2.75, 3.67, 9.68 lb/ft² |
| W81 | VERIFIED | SA08: "40 mm", "38 mm reported by Ayoub", subject counts and ages, "greatest when the handle diameter was 51 or 58 mm", "inner grip breadth", "197 mm"; Crossref 50(5): 734-744 |
| W82 | VERIFIED | Journal PDF text: "Open Access" and no licence text; three journal pages without licence text; Europe PMC isOpenAccess = N; Crossref no licence |

Fixes applied to the file: W1 page guess removed; W14/W16 author list and specimen label; W18 licence; W29 access note; W53 status sentence; W67 BLE 50R. No number changed value.

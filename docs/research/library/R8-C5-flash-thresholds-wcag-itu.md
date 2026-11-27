# R8-C5. Flash safety: WCAG 2.2 SC 2.3.1 definitions, ITU-R BT.1702, and how analysers work

Round 8, Section C, item 5. Private notes in my own words; no code from sources. These are HTML documents, so they are cited by section or note number, plus PDF page for the ITU text. **(mine)** marks my own arithmetic or inference.

## Sources

| Tag | Role | Citation | Free legal copy | Licence |
|---|---|---|---|---|
| L1 | Listed | W3C, *Web Content Accessibility Guidelines (WCAG) 2.2*, W3C Recommendation 12 December 2024. Eds. A. Campbell, C. Adams, R. Bradley Montgomery, M. Cooper, A. Kirkpatrick. https://www.w3.org/TR/2024/REC-WCAG22-20241212/ | Yes (link). Raw: raw/round8/wcag22.html and .txt | W3C document licence ("W3C liability, trademark and document use rules apply") |
| L1b | Companion to L1 | W3C WAI, *Understanding SC 2.3.1: Three Flashes or Below Threshold* (WCAG 2.2 Understanding Docs, informative). https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html ; techniques G15, G19 and G176 at https://www.w3.org/WAI/WCAG22/Techniques/general/G176 etc. | Yes. Raw: wcag22-understanding-2.3.1.html, wcag-tech-G15/G19/G176.html | W3C document licence |
| L1c | Errata check | W3C WAI, *WCAG 2.2 Errata*, https://www.w3.org/WAI/WCAG22/errata/ | Yes. Raw: wcag22-errata.html | W3C |
| L2 | Listed | ITU-R, *Recommendation ITU-R BT.1702-3 (11/2023): Guidance for the reduction of photosensitive epileptic seizures caused by television*, approved 2023-11-22, status in force. Earlier editions: 2005, 2018, 2019. | ITU free download: https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1702-3-202311-I!!PDF-E.pdf (12 PDF pages; printed page = PDF − 2). Raw: itu-bt1702-3.pdf | © ITU 2023, all rights reserved; free to download |
| S1 | Substitute for "an open description of analyser methods" | Electronic Arts, *IRIS* (photosensitivity video analysis library), README, https://github.com/electronicarts/IRIS (last push 2025-01-15) | Yes. Raw: iris-readme.md | BSD-3-Clause (repository licence). Only its documented behaviour is used here; no code was read or copied. |

The proprietary analysers WCAG points to (Harding FPA; Trace Center PEAT, per L1b "Related Resources") publish no algorithm specification that I found.

## C5-1. The success criterion (L1, SC 2.3.1, Level A)

A page passes if nothing on it flashes more than three times in any one-second period, *or* if the flashing stays below the general flash and red flash thresholds.

Because failing content can disrupt the whole page, every part of the page must meet this criterion, not only the parts used to meet other criteria (L1 SC 2.3.1 note; Conformance Requirement 5, Non-Interference).

## C5-2. Normative definitions (L1, Section 7 Glossary)

- **Flash:** a pair of opposing changes in relative luminance, which can cause seizures in some people if it is large enough and in the right frequency range.
- **General flash and red flash thresholds:** content passes if *either*:
  1. there are no more than three general flashes and/or no more than three red flashes in any one-second period; *or*
  2. all the flashes happening at the same time together cover no more than 0.006 steradians within any 10 degree visual field on the screen. The glossary also describes this as 25% of any 10 degree field at typical viewing distance.
- **General flash:** a pair of opposing changes in relative luminance of 10% or more of the maximum relative luminance (1.0), where the darker image's relative luminance is below 0.80. "Opposing" means an increase then a decrease, or the reverse.
- **Red flash:** any pair of opposing transitions involving a saturated red. The normative text does not define "saturated red".
- **Exception:** a fine, balanced pattern (white noise, or a checkerboard whose squares are smaller than 0.1 degree on a side) does not violate the thresholds.
- **Note 1 (pixel estimate):**
  - For general web content, a 341 x 256 pixel rectangle anywhere on a 1024 x 768 screen is a good estimate of a 10 degree field. This assumes a 15-17 inch screen viewed from 22-26 inches.
  - That is 75-85 ppi, lower and so more cautious than CSS's nominal 96 ppi.
  - Higher-resolution displays make the same content physically smaller, which is safer.
- **Note 2 (transition):** a transition is the change in relative luminance, or in luminance/colour for red, between adjacent peaks and valleys of the luminance-against-time plot. A flash is two opposing transitions.
- **Note 3 (new in WCAG 2.2, informative working definition of a saturated-red transition):** one of the two states has R/(R+G+B) of at least 0.8, *and* the two states differ by more than 0.2 in CIE 1976 UCS chromaticity (u', v'). This is credited to ISO 9241-391.
- **Note 4:** tools can analyse screen-captured video. No tool is needed when there are 3 or fewer flashes in every second.

## C5-3. Relative luminance (L1 glossary, "relative luminance")

- L = 0.2126 R + 0.7152 G + 0.0722 B, computed on linearised channels.
- Each 8-bit value is first divided by 255 to give C (between 0 and 1). If C is at most 0.04045, the linear value is C/12.92; otherwise it is ((C + 0.055)/1.055)^2.4.
- **Version note (L1 Note 2):** before May 2021 the threshold was 0.03928, taken from an older version of the sRGB specification. W3C says the change has no practical effect for these guidelines.
- Assume sRGB unless another colour space is known (Note 3).
- Dithering (Note 4): if it happens after delivery, use the source colour. If the colours are dithered at the source, use the average R, G and B.
- **Chromaticity for the red test (L1b, "Note (new in WCAG 2.2)"):**
  - u' = 4X/(X + 15Y + 3Z), v' = 9Y/(X + 15Y + 3Z).
  - The difference between two states is the Euclidean distance in (u', v').
  - X, Y, Z come from the linear R, G, B through the standard sRGB-to-XYZ matrix: first row 0.4124564, 0.3575761, 0.1804375; Y row 0.2126729, 0.7151522, 0.0721750; Z row 0.0193339, 0.1191920, 0.9503041.
  - **(mine)** The Y row matches the luminance weights to four decimals.

## C5-4. Area: 10 degree field, 0.006 sr, and pixels (L1 Note 1; L1b; G176)

- **Where the 10 degree field comes from (L1b Intent):** it comes from the original broadcast specifications and stands for central vision, where people are most susceptible. The criterion was adapted from UK television broadcast rules for closer desktop viewing.
- **CSS pixels (L1b Intent):** the 341 x 256 block can be read as CSS pixels, so the flashing area is 87,296 CSS px². L1b argues that a fixed CSS size takes up a similar share of the visual field across devices.
- **G176 (Formula 1):** with only one flashing area on screen, any contiguous flash smaller than 21,824 px² (any shape) at 1024 x 768 passes. **(mine)** That is 87,296 / 4.
- **G176 (Formula 2, known displays):**
  - The 10 degree field is approximated as a 4:3 rectangle, 10 degrees wide by 7.5 degrees high (75 square degrees).
  - Rectangle width = viewing distance x 0.1745; height = viewing distance x 0.1309.
  - Multiply by pixels per unit length to get pixels, then take width x height / 4.
  - G176 gives pixel-density shortcuts: 1080p gives 2203 divided by the diagonal in inches; 720p widescreen, which G176 takes as 1365 x 768, gives 1566 divided by the diagonal.
- **My check of 0.006 sr (mine):** a 10 x 7.5 degree rectangle is 0.02285 sr, a quarter of which is 0.00571 sr. A 10 degree circle is 0.02392 sr, a quarter of which is 0.00598 sr. So "0.006 sr" and "25% of a 10 degree field" agree to within about 5%.
- **Viewing distance caveat (L1b note):** the criterion cannot allow for how close a user actually sits. Closer viewers, and users of zoom or magnifiers, are affected by areas that formally pass. Viewers further away can tolerate more.
- **How to test (L1b Intent):**
  - Analyse at the largest scale the user might see, such as full screen for a video that can go full screen, at the browser's standard zoom.
  - For short looping clips such as GIFs, analyse while looping.
  - For non-sRGB or HDR video, test the version with the highest dynamic range.

## C5-5. ITU-R BT.1702-3 (L2) and how it lines up with WCAG

**L2 Annex 1, Guideline 1 (printed p. 3 = PDF 5):**
- **Darker image below 160 cd/m²:** a potentially harmful flash is a pair of opposing changes of 20 cd/m² or more. This applies to SDR and HDR.
- **Darker image at or above 160 cd/m²:** the change must exceed a Michelson contrast of 1/17, where Michelson contrast is (L_high − L_low)/(L_high + L_low). This applies to HDR only.
  - Note 6 (PDF 6): the two rules meet without a jump at 160, since 20/(180 + 160) = 1/17.
- **Red:** a transition to or from a saturated red is potentially harmful whatever the luminance.
- **When a sequence is not allowed:** only when *both* conditions hold:
  1. the flashes together cover more than 25% of the *displayed screen area*; *and*
  2. there are more than three flashes (six changes) in any one second.
- **Timing:** single, double or triple flashes are acceptable. Successive flashes whose leading edges are at least 360 ms apart (50 Hz) or 334 ms apart (60 Hz) are always acceptable, whatever their brightness or area.
- **Fast cuts** count as flashes if they make areas of the screen flash.
- **Duration:** a run of flashing longer than 5 seconds may be a risk even if it complies, because risk is thought to rise with duration.

**Display assumptions (L2 Note 3, PDF 5-6; Annex 2, PDF 8):**
- SDR peak white is assumed to be 200 cd/m²; HLG peak 1,000 cd/m²; PQ peak 10,000 cd/m².
- Table 1 (PDF 9) maps 10-bit code values to screen luminance. For BT.709 masters, V = (D − 64)/876.
- Notes to Fig. 3 (PDF 10): an SDR code value of 400 gives about 20.1 cd/m², and 863 gives about 160.4 cd/m².

**Patterns (Guideline 2 and Attachment 1, PDF 5 and 8):**
- More than five clearly visible light-dark stripe pairs are potentially harmful if the pattern covers more than 40% of the screen while still, or more than 25% while moving, flashing or reversing.
- The contrast test is the same as for flashes.
- Patterns that flow smoothly across, into or out of the screen in one direction are exempt.

**Receiver-side filtering (Annex 4, PDF 11-12):** an adaptive temporal filter should reduce frame-to-frame changes in the 10-30 Hz range, typically by 20 dB or more at 10 Hz and above.

**Environment (Annex 5, PDF 12):** risk rises in a dark room, with a brighter or larger screen, or sitting closer. Photosensitivity is most common under age 20. L2 suggests viewing in a well-lit room from at least two metres.

**Measurement specification (Annex 3, PDF 10):** L2 itself says a unified measurement specification "needs further study", and the saturated-red criterion needs more explicit definition. Note 5 recommends automatic analysers.

**How WCAG maps onto L2 (my inference, mine):**
- With SDR peak white at 200 cd/m², 20 cd/m² is 10% of peak and 160 cd/m² is 0.80 of peak. So WCAG's "10% of maximum relative luminance, darker image below 0.80" is the same rule in relative units. L1b also quotes the 20 and 160 cd/m² figures and the 1/17 Michelson rule for non-sRGB/HDR video, citing BT.1702.
- **Area: the sources differ.**
  - L2 uses 25% of the *whole displayed screen* (television at about 2 m).
  - WCAG uses 25% of any *10 degree field* at desktop distance (341 x 256 px at 1024 x 768).
  - For a web page these give very different pixel limits. On a 1024 x 768 screen, 25% of the screen is 196,608 px² against WCAG's 21,824 px². **WCAG is about 9 times stricter on area (mine).**
- **Counting: the sources differ.** WCAG is "no more than three flashes in any one-second period". L2 permits flashes spaced 360/334 ms apart, which allows up to about 2.8-3 per second. **(mine)** These are close but not identical rules at the boundary.

## C5-6. Analyser method (open description: S1, IRIS README)

S1 documents its pass/fail logic and per-frame outputs.

**Flash detection:**
- It tracks per-frame *average luminance difference accumulation*. A new luminance transition is counted when the accumulated change reaches +0.1 or −0.1 in relative luminance. **(mine)** This matches WCAG's 10%.
- Red uses a separate accumulated *red difference*, with a transition at ±20 on S1's own red-saturation scale. The README does not define that scale.
- It also records the flashing *area proportion* per frame.

**Verdicts:**
- **Flash failure:** more than 3 flashes in any second.
- **Extended flash failure:** 2-3 flashes per second (4-6 transitions) sustained over any 5 consecutive seconds. **(mine)** This mirrors L2's 5-second caution.
- **Warning:** more than 2 flashes per second.
- **Pattern failure:** a harmful spatial pattern present for 0.5 s or more.

**Limits:** S1 states it is not intended to guarantee, certify or validate compliance with legal or regulatory requirements. Its thresholds sit in a configurable settings file, and the README says the defaults follow publicly available guidance.

**(mine) A minimal analyser consistent with L1 and L2:**
1. Per frame, compute the relative-luminance map (C5-3) and a red-state map (Note 3 test).
2. Per pixel, track peaks and valleys over time. Count a transition when the swing since the last extreme reaches 0.1 with the darker side below 0.8, or meets the red-state test.
3. For each one-second window and each 341 x 256 (CSS px) window position, sum the area of pixels flashing at the same time. Fail if more than 3 flashes occur over more than 25% of that window.

This is my own design sketch, not taken from any source.

## Known problems and changes

- WCAG 2.2 errata (L1c): the only item touching these definitions is editorial, dated 2024-11-19 (#3756): "general flash and red flash thresholds" changed from an ordered to an unordered list. Nothing substantive.
- The 0.03928 to 0.04045 correction (May 2021), noted in L1 itself.
- "Saturated red" was undefined until WCAG 2.2's informative Note 3. L2 still calls for a more explicit red criterion.

## Not found or not verified

- ISO 9241-391 (the source of the red working definition): paywalled; not read.
- Harding FPA and PEAT algorithm details: not published.

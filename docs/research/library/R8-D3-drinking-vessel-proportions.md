# R8-D3. Drinking vessels: height-to-diameter ratios for tumbler, beaker, mug and goblet (bowl, stem, foot)

Round 8, section D, item 3. Prepared 2026-11-27. The brief named no source ("any open design or archaeological typology"), so every source here is chosen by me and labelled. **(mine)** marks my own computation.

## Sources

| Tag | Role | Citation | Free copy | Licence |
|---|---|---|---|---|
| S1 | Measured museum objects (open data), standing in for an archaeological typology | The Metropolitan Museum of Art, Collection API (object records: objectName, medium, date, measurements), queried 2026-11-27 for objectName = Tumbler, Beaker, Mug, Goblet, Wine glass (first 200 search hits per term). https://metmuseum.github.io/ | Yes. Extract saved as library/R8-D3-met-drinking-vessels-data.csv (129 objects; object URLs included); script raw/round8/met/met_ratios.py | Data CC0 (The Met's API page: "waived all copyright ... using the Creative Commons Zero license") |
| S2 | Measured modern stemware bowls | Parpinello G. P., Meglioli M., Ricci A., Versari A. (2018). "Effect of Different Glass Shapes and Size on the Time Course of Dissolved Oxygen in Wines during Simulated Tasting." *Beverages* 4(1): 3. doi:10.3390/beverages4010003 | MDPI PDF (fetched via the mdpi-res.com mirror; raw parpinello18.pdf, 7 pp.) | CC BY 4.0 |
| S3 | Standard tasting goblet (bowl, stem, foot) | ISO 3591:1977, *Sensory analysis — Apparatus — Wine-tasting glass* (confirmed 2022). **Standard not read** (sold by ISO; the free ANSI preview returned a Cloudflare challenge to the box). Dimensions taken second-hand from Wikipedia, "Wine glass", §ISO wine tasting glass (raw wp-Wine_glass.wiki, fetched 2026-11-27) | Wikipedia only | Wikipedia text CC BY-SA 4.0; the standard is © ISO |
| S4 | Failure case (perception) | Attwood A. S., Scott-Samuel N. E., Stothart G., Munafò M. R. (2012). "Glass shape influences consumption rate for alcoholic beverages." *PLoS ONE* 7(8): e43007. doi:10.1371/journal.pone.0043007 | Europe PMC full text PMC3422221 (raw attwood12.xml/.txt) | CC BY |
| (tried) | Typology definitions | Medieval Pottery Research Group (1998), *A Guide to the Classification of Medieval Ceramic Forms*, Occasional Paper 1 | Free PDF on medievalpottery.org.uk and the ADS (doi:10.5284/1059287). **Not obtained:** both sites returned bot-check pages to the box, and Wayback refused TLS. Failures kept as raw/round8/mprg1998-forms.FAILED-*.html | © MPRG |

## D3-1. Measured ratios from museum objects (S1)

**Method (mine):**
- Query each term and keep objects whose objectName matches it exactly.
- Read the "Overall" Height and Diameter from the structured measurements (cm) and take H/D.
- **Caveat:** the Met does not say whether "Diameter" is the rim, the greatest width or the foot. For goblets it is usually the rim or the bowl (my reading of the dimension strings). H is the overall height, so a goblet's H includes stem and foot.

| Type | n | median H/D | interquartile range | min to max | glass only (n, median) |
|---|---|---|---|---|---|
| Tumbler | 8 | 1.21 | 0.98 to 1.35 | 0.58 (silver, ca. 1691) to 1.40 | 7, 1.27 |
| Beaker | 63 | 1.27 | 1.16 to 1.41 | 0.65 to 3.35 | 42, 1.29 |
| Mug | 9 | 1.25 | 1.17 to 1.50 | 1.06 to 2.42 | 6, 1.38 |
| Goblet (overall) | 37 | 1.81 | 1.50 to 2.14 | 0.77 to 3.31 | 33, 1.86 |
| Wine glass (overall) | 12 | 2.47 | 1.96 to 4.31 | 1.09 to 4.31 | 12, 2.47 |

**Notes (mine):**
- Most of the sample is European decorative arts, 16th to 19th century; the wine-glass sample is mostly 20th-century design. These are not random samples of their types.
- Tumbler and beaker overlap almost completely (median about 1.2-1.3). In this sample the name tells you little about the proportion.
- The full per-object list (ID, date, medium, H, D, ratio, URL) is in the CSV beside this file.

## D3-2. Stemware: bowl, stem and foot

### ISO 3591 tasting glass (S3, second-hand)

| Part | Dimension |
|---|---|
| Rim | 46 mm |
| Calyx (bowl) height | 100 mm |
| Shoulder (bowl max) | 65 mm |
| Base (foot) diameter | 65 mm |
| "Height of the foot" | 55 mm, i.e. stem plus foot |
| Stem | 9 mm (Wikipedia lists this as "Foot diameter"; see Known problems) |
| Volume | 210-225 mL, for a 50 mL pour |

Lead crystal, 9% lead. ISO's public abstract (https://www.iso.org/standard/9002.html, read 2026-11-27; 3 pages; confirmed 2022-05-10) describes the shape as a cup like an "elongated egg" on a stem resting on a base, with the opening narrower than the convex part so as to concentrate the bouquet.

**Ratios (mine):**
- Bowl H/max D = 100/65 = 1.54.
- Total height 155 mm, so overall H/max D = 2.38.
- Stem plus foot = 55/155 = 35% of the height.
- Rim / max = 46/65 = 0.71.
- Foot diameter = bowl max diameter.

### Measured wine-glass bowls (S2 Table 1, PDF 3)

S2 measured six glasses following Hirson, Heymann and Ebeler (2012); one is the ISO glass (Bormioli).

| Glass | Opening (mm) | Max D (mm) | Height (mm) | H/max D (mine) | H/opening (mine) | max/opening (S2) |
|---|---|---|---|---|---|---|
| ISO | 45.2 | 63.9 | 94.5 | 1.48 | 2.09 | 1.41 |
| Fresh | 49.7 | 79.0 | 116.1 | 1.47 | 2.34 | 1.59 |
| Mature | 53.8 | 83.2 | 128.3 | 1.54 | 2.38 | 1.55 |
| Rich | 67.6 | 89.8 | 130.6 | 1.45 | 1.93 | 1.33 |
| Reserve | 73.0 | 95.8 | 142.5 | 1.49 | 1.95 | 1.31 |
| Super800 | 73.4 | 116.1 | 107.7 | 0.93 | 1.47 | 1.58 |

**(mine)**
- The "Height" column matches a bowl height. ISO measures 94.5 against 100 nominal, so I read it as bowl only, measured as S2/Hirson define it (the paper does not draw the datum).
- Five of the six bowls sit at **H/max D ≈ 1.45-1.54**, with max/opening 1.3-1.6. The wide "Super800" bowl is the outlier at 0.93.

### Where the sources disagree

- **ISO bowl height:** nominal 100 mm (S3) against 94.5 mm measured (S2). This is probably a different datum (inside against outside, or rim to bowl bottom). Both are given.
- **Overall goblet H/D:** about 1.8 for historical Met goblets against 2.4 for the ISO glass and 2.5 for the Met's (mostly modern) wine glasses. Historical goblets are squatter overall, or their D is a wider rim.

## D3-3. A failure case for shape choice (S4)

S4 (Bristol, n = 159) compared two 12 fl oz glasses: a straight-sided one, whose half-way point by height is also half by volume, and a curved flute.
- Lager was drunk **60% slower from the straight glass** when full; there was no effect for half-full glasses or soft drinks.
- People misjudged the half-way point of the curved glass more.

For a shape library this means a curved or flared profile makes the volume at a given fill height hard to judge by eye. If the engine ever shows fill levels, compute volume by integrating the profile, not by height (mine).

## Summary for the shape library (mine)

| Form | H/D to use | Basis |
|---|---|---|
| Tumbler / beaker (straight or slightly flared) | median 1.2-1.3; IQR 1.0-1.4 | S1 |
| Mug (handled) | median 1.25 (glass 1.4) | S1, small n |
| Goblet overall | about 1.8 (historical) to 2.4 (ISO) | S1, S3 |
| Goblet bowl | H/max D about 1.5; rim/max about 0.7 (tulip); stem plus foot about 35% of the total height; foot D ≈ bowl max D | S3, S2 |

## Known problems

- **Wikipedia's ISO list labels 9 mm as "Foot diameter"** while also giving a 65 mm "Diameter of the base". The 9 mm is evidently the stem diameter: vendor specification pages in my search results give "Stem diameter 9 mm", but those are not citable sources. Flagged WEAK.
- **S1's "Diameter" is not defined** per object, and the samples are small and biased to the Met's holdings.

## Not found

- MPRG 1998 Guide (blocked by bot checks), and Beaudry et al. 1983 POTS (paywalled; *Historical Archaeology* 17(1): 18-39). Both give typology definitions by height-to-diameter relations, but I did not read either, so no definition is quoted from them.

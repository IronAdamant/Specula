# R7-2C Birds: leg bones, neck, girth, downstroke, wing folding (Q12-Q16)

Round 7 library research. **Access date for every source in this file: 2026-11-24.** Values marked **computed** were worked out by me from published numbers.

## Sources used (licence and access)

| Key | Citation | Free to read? | Licence | Read |
|---|---|---|---|---|
| B1 | Böhmer C, Plateau O, Cornette R, Abourachid A (2019). "Correlated evolution of neck length and leg length in birds." *Royal Society Open Science* 6(5): 181588. doi:10.1098/rsos.181588. PMC6549945. Data: electronic supplementary material, figshare doi:10.6084/m9.figshare.c.4479884 | Yes | **CC BY 4.0** | Full text plus supplementary Word file (Tables S1-S4), taken from the Europe PMC supplementary bundle (the figshare download returned 403) |
| B2 | Marek RD, Falkingham PL, Benson RBJ, Gardiner JD, Maddox TW, Bates KT (2021). "Evolutionary versatility of the avian neck." *Proceedings of the Royal Society B* 288(1946): 20203150. doi:10.1098/rspb.2020.3150. PMC7934994 | Yes | **CC BY 4.0** | Full text; per-species data are in its ESM table S2, **not read** |
| B3 | Tobalske BW, Warrick DR, Clark CJ, Powers DR, Hedrick TL, Hyder GA, Biewener AA (2007). "Three-dimensional kinematics of hummingbird flight." *Journal of Experimental Biology* 210(13): 2368-2382. doi:10.1242/jeb.005686 | Yes (free to read) | © The Company of Biologists | Full text |
| B4 | Baliga VB, Szabo I, Altshuler DL (2019). "Range of motion in the avian wing is strongly associated with flight behavior and body mass." *Science Advances* 5(10): eaaw6670. doi:10.1126/sciadv.aaw6670. PMC6810231 | Yes | **CC BY-NC 4.0** | Full text (Methods). Numbers are in figures and data S1; the supplementary bundle I got was truncated, so **no numbers were read** |

## Q12 and Q13. Leg bones and neck length per species (B1)

**Method (B1):** one skeletal specimen per species from the Muséum National d'Histoire Naturelle, Paris (MNHN ZO; IDs below), 103 species from 34 orders. Leg bones (right side unless absent) and the centrum of every cervical vertebra were measured with a digital calliper (Mitutoyo, ±0.01 mm), straight along the long axis. Neck length = sum of all vertebral (centrum) lengths, **without intervertebral discs or cartilage**; the atlas (CV1) has no centrum and is not measured. Total leg length = femur + tibiotarsus + tarsometatarsus (toes are not included). The central vertebra is the one at the middle of the cervical count, usually the longest. Body mass is **not the specimen's**: B1 took species mean body mass from the literature (its ref 17).

**Units and back-transformation (computed):** Table S4 gives only log-transformed values and does not state the log base or units. I back-transformed them as log10 of metres for lengths and log10 of (body mass in kg)^(1/3) for mass. This is **computed with inferred units**. The check that supports it: for every row, femur + tibiotarsus + tarsometatarsus equals the "tll" column after back-transformation (e.g. *Amazona amazonica* 43 + 56 + 19 = 118 mm), and the ostrich femur comes out at 316 mm and mass at 128 kg, both realistic. n = 1 specimen per species, so there is no spread.

| Q | Species | Specimen | Body mass, literature mean (kg, **computed**) | Femur (mm) | Tibiotarsus (mm) | Tarsometatarsus (mm) | Total leg (mm) | Neck length (mm) | Central vertebra (mm) | Femur / tibiotarsus / tarsometatarsus as share of the leg (**computed**) | Neck / leg (**computed**) | Source |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Q12 Q13 | *Columba livia* | MNHN ZO 1876-642 | 0.27 | 56 | 79 | 43 | 178 | 115 | 12.0 | 0.31 / 0.44 / 0.24 | 0.65 | B1 Table S4 |
| Q12 Q13 | *Corvus corax* | MNHN ZO 1997-949 | 1.29 | 70 | 111 | 69 | 250 | 106 | 12.1 | 0.28 / 0.44 / 0.27 | 0.42 | B1 Table S4 |
| Q12 Q13 | *Larus glaucescens* | MNHN ZO 1993-346 | 0.08 | 70 | 123 | 75 | 267 | 172 | 16.8 | 0.26 / 0.46 / 0.28 | 0.64 | B1 Table S4 |
| Q12 Q13 | *Aquila chrysaetos* | MNHN ZO 2013-362 | 3.69 | 129 | 129 | 89 | 347 | 148 | 16.5 | 0.37 / 0.37 / 0.26 | 0.43 | B1 Table S4 |
| Q12 Q13 | *Bubo bubo* | MNHN ZO 2000-343 | 2.71 | 101 | 147 | 80 | 328 | 109 | 12.7 | 0.31 / 0.45 / 0.24 | 0.33 | B1 Table S4 |
| Q12 Q13 | *Tyto alba* | MNHN ZO 1997-494 | 0.32 | 49 | 82 | 59 | 190 | 54 | 5.8 | 0.26 / 0.43 / 0.31 | 0.29 | B1 Table S4 |
| Q12 Q13 | *Ardea cinerea* | MNHN ZO 1993-103 | 1.55 | 86 | 198 | 147 | 432 | 375 | 29.5 | 0.20 / 0.46 / 0.34 | 0.87 | B1 Table S4 |
| Q12 Q13 | *Anas acuta* | MNHN ZO 1986-35 | 0.85 | 41 | 68 | 40 | 149 | 154 | 13.0 | 0.28 / 0.46 / 0.27 | 1.03 | B1 Table S4 |
| Q12 Q13 | *Gallus gallus* | MNHN ZO 1893-232 | 1.06 | 103 | 150 | 109 | 362 | 204 | 18.9 | 0.28 / 0.41 / 0.30 | 0.56 | B1 Table S4 |
| Q12 Q13 | *Struthio camelus* | MNHN ZO 1908-160 | 128.0 | 316 | 551 | 473 | 1340 | 921 | 58.6 | 0.24 / 0.41 / 0.35 | 0.69 | B1 Table S4 |
| Q12 Q13 | *Dromaius novaehollandiae* | MNHN ZO 1897-89 | 57.5 | 207 | 376 | 350 | 933 | 552 | 32.8 | 0.22 / 0.40 / 0.38 | 0.59 | B1 Table S4 |
| Q12 Q13 | *Cygnus olor* | MNHN ZO MNHN1871-420 | 10.8 | 110 | 280 | 116 | 506 | 638 | 32.6 | 0.22 / 0.55 / 0.23 | 1.26 | B1 Table S4 |
| Q12 Q13 | *Phoenicopterus ruber* | MNHN ZO 1997-205 | 3.10 | 94 | 308 | 294 | 696 | 439 | 41.5 | 0.13 / 0.44 / 0.42 | 0.63 | B1 Table S4 |

All lengths in this table are **computed** (back-transformed, rounded to 1 mm; central vertebra to 0.1 mm).

### Q13. Cervical vertebra counts

| Q | Item | Value | n | Source |
|---|---|---|---|---|
| Q13 | All birds sampled: mean / max / min count | 13 / 23 / 10 | 103 taxa | B1 Table S3 |
| Q13 | Palaeognathae | 16 / 18 / 14 | 6 | B1 Table S3 |
| Q13 | Galloanseriformes (fowl and waterfowl) | 16 / 23 / 14 | 8 | B1 Table S3 |
| Q13 | Other Neoaves | 14 / 19 / 11 | 15 | B1 Table S3 |
| Q13 | Aequornithes (waterbirds) | 14 / 18 / 12 | 13 | B1 Table S3 |
| Q13 | Charadriiformes (shorebirds, gulls) | 13 / 14 / 12 | 10 | B1 Table S3 |
| Q13 | Telluraves (land birds) | 13 / 15 / 11 | 24 | B1 Table S3 |
| Q13 | Australaves (falcons, parrots, songbirds) | 11 / 13 / 10 | 27 | B1 Table S3 |
| Q13 | Named species | mute swan *Cygnus olor* 23 (the sample maximum, the only taxon with 23); emu 18; nandu (rhea) 15; chicken 14; blue-and-yellow macaw *Ara ararauna* 10 (the only taxon with 10); no bird in the sample had 20, 21 or 22 | 1 specimen each | B1 Results and Discussion |
| Q13 | Range in birds (literature) | "as few as 10 in parrots to as many as 26 in swans" (B1 Introduction); "between 10 and 26" (B2 Introduction, citing its refs 12-13) | | B1, B2 |
| Q13 | Flamingo | count not given per species in B1's text; neck length 439 mm and central vertebra 41.5 mm (**computed**, table above). B1 notes that a few elongated vertebrae in flamingos make a long neck, vs many short vertebrae in swans | | B1 |

B2 digitised every cervical vertebra except the atlas for 54 specimens (48 species) by CT scan; its per-species counts are in ESM table S2, which I did not read.

## Q14. Body girth, breadth and depth
Not found (see the list below).

## Q15. Share of the wingbeat spent in downstroke

| Q | Species | n | Value | Conditions | Source |
|---|---|---|---|---|---|
| Q15 | Rufous hummingbird *Selasphorus rufus*, 3 g | 5 birds | downstroke "increased with increasing flight velocity from 48% at 0 and 2 m s−1 to 53 ± 1% at 12 m s−1" | wind tunnel, 0-12 m/s, 500 Hz 3-D video | B3 Results |
| Q15 | Hummingbird vs zebra finch | | coefficient of variation in downstroke percentage 8% (hummingbird) vs 3% (zebra finch, from Tobalske et al. 1999) | | B3 Discussion |

## Q16. Wing folding angles

| Q | Item | Value | Source |
|---|---|---|---|
| Q16 | Method | wings from adult cadavers of 61 species (125 individuals) in 20 orders; extension and flexion at the elbow and the manus (wrist) were found by manually applying force at a joint until resistance; angles plotted on 0-180° elbow vs 0-180° manus axes | B4 Methods and Results |
| Q16 | Per-species angles | **not read** (in B4 figures and data S1) | B4 |

## Not found (this file)
- **Q12:** toe (digit and phalanx) lengths for any bird; any spread (B1 is one specimen per species).
- **Q13:** per-species cervical counts beyond the six named in B1's text (B1 Table S1 and B2 ESM table S2 hold them but were not read in full), including the flamingo count.
- **Q14:** body girth, breadth and depth for any bird. No open source found.
- **Q15:** downstroke percentage for any bird other than the hummingbird (Tobalske et al. 1999 on the zebra finch and Tobalske & Dial 1996 on the magpie and pigeon were not opened; a search extract says the zebra finch's percentage did not change with speed, not verified).
- **Q16:** any numeric wing-fold angle (B4 data S1 not read).

## Sources disagree (this file)
- **Maximum cervical count in swans:** B1's own sample has the mute swan at 23 (one specimen), but B1's Introduction and B2 both cite a maximum of 26 "in swans". The species behind 26 is not named in either; the difference may be between species or between counting conventions (B1 follows Romer: cervicals are all vertebrae in front of the first vertebra with freely movable ribs. Under Baumel et al.'s definition, the first thoracic is the first with a complete rib reaching the sternum, so the last rib-bearing vertebrae count as cervical. B1 says counts in the literature differ for this reason, and its convention gives the lower count).
- **Body mass in B1 Table S4:** *Larus glaucescens* back-transforms to 0.08 kg (**computed**), far too light for its skeleton (leg 267 mm, longer than the raven's at 1.29 kg). This looks like an error in B1's input mass; do not use that value.

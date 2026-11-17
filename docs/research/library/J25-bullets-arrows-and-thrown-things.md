# J25: Bullets, arrows and thrown things: speeds, masses, energies, recoil, drop

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 1d. Values marked "computed by me" are arithmetic. Firearm figures are for animation, sound and hit effects in a game; nothing here is handloading advice.

## J25-1. Cartridges (Wikipedia "Ballistic performance" infoboxes; each infobox names maker or test sources)

| Cartridge, load | Bullet mass | Muzzle velocity | Energy | Test barrel | Infobox sources | Page |
|---|---|---|---|---|---|---|
| .22 Long Rifle, 40 gr solid, high velocity | 40 gr (2.59 g) | 1,260 ft/s (384 m/s) | 131 ft·lbf as listed | 18.5 in | Federal Cartridge Co. | https://en.wikipedia.org/wiki/.22_Long_Rifle |
| 9×19mm Parabellum, Federal FMJ | 7.45 g (115 gr) | 1,180 ft/s (360 m/s) | 355 ft·lbf (481 J) | 118 mm | Sellier & Bellot, CIP, Norma, Underwood, RBCD | https://en.wikipedia.org/wiki/9×19mm_Parabellum |
| 9×19mm, Hornady XTP | 9.53 g (147 gr) | 975 ft/s (297 m/s) | 420 J | 118 mm | same | same |
| .45 ACP, Winchester FMJ | 230 gr (14.9 g) | 835 ft/s (255 m/s) | 356 ft·lbf | | maker data | https://en.wikipedia.org/wiki/.45_ACP |
| 7.62×39mm, 57N231 FMJ | 7.9 g | 730.3 m/s | 2,108 J | 520 mm | Wolf, Sellier & Bellot | https://en.wikipedia.org/wiki/7.62×39mm |
| 5.56×45mm NATO, XM193 | 3.56 g (55 gr) | 993 m/s | 1,755 J | 508 mm | NATO EPVAT, SAAMI, CIP | https://en.wikipedia.org/wiki/5.56×45mm_NATO |
| 5.56×45mm NATO, SS109 | 4 g (62 gr) | 948 m/s | 1,797 J | 508 mm | same | same |
| 7.62×51mm NATO, M80 ball | 147 gr (9.53 g) | 853 m/s | 3,469 J | 22 in | US Army TM 9-1005-298-12 (1969), TM 9-1005-224-10 (1985) | https://en.wikipedia.org/wiki/7.62×51mm_NATO |
| 7.62×51mm NATO, M118 long range | 175 gr | 792 m/s | 3,561 J | 24 in | same | same |
| .50 BMG, Speer 647 gr | 647 gr (41.9 g) | 3,044 ft/s (928 m/s) | 13,310 ft·lbf (18,046 J) | 45 in | Ammoguide.com | https://en.wikipedia.org/wiki/.50_BMG |

The .22 LR page itself carries an editor note that its energies look low against ½mv² (e.g. 191 J computed for 40 gr at 1,260 ft/s, against 131 ft·lbf = 178 J listed).

## J25-2. Derived game numbers (computed by me from J25-1)

Recoil speed is for a 4 kg gun, bullet momentum only (propellant gas adds more, often a third to a half again for rifles; not sourced, so not used). Drop is the vacuum drop over 100 m at muzzle speed; real drop is larger because the bullet slows.

| Load | Momentum (kg m/s) | Gun recoil speed, 4 kg (m/s) | Vacuum drop over 100 m (cm) | Faster than sound (343 m/s)? |
|---|---|---|---|---|
| .22 LR 40 gr | 1.00 | 0.25 | 33 | yes, just (crack) |
| 9 mm 7.45 g | 2.68 | 0.67 | 38 | yes, just |
| .45 ACP 230 gr | 3.79 | 0.95 | 76 | no (no crack, only muzzle report) |
| 7.62×39 7.9 g | 5.77 | 1.44 | 9 | yes |
| 5.56 SS109 | 3.79 | 0.95 | 5.5 | yes |
| 7.62×51 M80 | 8.13 | 2.03 | 6.7 | yes |
| .50 BMG 647 gr | 38.9 | 9.7 (why .50 rifles weigh 13 kg or more and use muzzle brakes) | 5.7 | yes |

## J25-3. Bows and crossbows

- Mary Rose replica longbow (Roy King, bowyer to the Mary Rose Trust): measured draw weight 465 N at 0.746 m draw, bow mass 0.794 kg, length 1.874 m; Kooi's model predicted it within 1% (B. W. Kooi, "An approach to the study of ancient archery using mathematical modelling", 1997, table of bows, https://www.bio.vu.nl/thb/users/kooi/kobe97.pdf, citing Hardy 1992). Same table: Turkish flight bow 690 N, 0.35 kg, 1.14 m; modern bow 126 N.
- Original Mary Rose bow draw weights estimated at 150-160 lbf at 30 in, full range 100-185 lbf (Hardy, *Longbow*, via Wikipedia "English longbow").
- Approximation bows of Mary Rose bow X1-3 shooting replica arrows (Aston and Spencer, "The Deane brothers' Mary Rose Bow X1-3", https://keskustelu.primitiivijousi.fi/uploads/short-url/jbxr7WWexChfC3RBxRL26IK9gxF.pdf): 101-141 lbf at 30 in; Westminster-type arrow (about 50 g) 227-255 yd; 61 g Mary Rose livery arrow 209-228 yd; AZ472/19 replica 203-241 yd.
- Mark Stretton's test (Wikipedia "English longbow", citing Soar, Gibbs, Jury and Stretton, *Secrets of the English War Bow*, Westholme 2010, pp. 127-151): 102 g quarrel-like arrow from a 144 lbf yew self bow at 32 in, 47.23 m/s, 113.76 J. Bane's and Williams' tests gave 73 and 80 J; a 160 lbf bow at short range gave 160 J.
- Compound bow efficiency: 70-85% of stored energy goes to the arrow (Wikipedia "Compound bow", citing its listed sources).

## J25-4. Thrown and slung things

- Atlatl darts: well-practised throwers rarely exceed about 35 m/s (Whittaker, Pettigrew and Grohsmeyer, "Atlatl Dart Velocity: Accurate Measurements and Implications for Paleoindian and Archaic Archaeology", *PaleoAmerica* 3(2), 2017, doi:10.1080/20555563.2017.1301133, abstract).
- Slings: Finney (2006) mean initial velocity 25.38 m/s, maximum 36.96 m/s; Vega and Craig (2009) mean range 66 m, adult males 78 m, one slinger over 100 m consistently and 130 m best; the reviewing author concludes experienced slingers throw beyond 100 m at 30-35 m/s (Eric Skov, "Sling Technology: Towards an understanding of capabilities", *The Nebraska Anthropologist* 26 (2011) 112-126, https://digitalcommons.unl.edu/cgi/viewcontent.cgi?article=1168&context=nebanthro).
- Trajectory check (computed by me, no drag): 35 m/s at 45° reaches 125 m; 25 m/s reaches 64 m.

## Sources disagree

- .22 LR energies: infobox values against ½mv² (see J25-1 note).
- Longbow draw weights: 450 N average from bows tested to destruction (degraded wood), 465 N measured replica, 670-710 N estimated originals (all as reported in Kooi 1997 and Wikipedia "English longbow").
- Sling speeds: 25-37 m/s (Finney) against chronograph tests by a hobbyist of 45-77 m/s seen in a forum post (not used as a source).

## Not found

- Ballistic coefficients (G1/G7) from maker pages for the loads above (not opened this session).
- Measured arrow speed for a named longbow and arrow pair from a primary paper (Stretton's 47.23 m/s is via Wikipedia).
- Crossbow bolt speeds and draw weights from a primary historical test (Payne-Gallwey 1903 is public domain but was not opened).
- Musket and cannon muzzle velocities from a primary source (Robins 1742 is cited in Wikipedia "Brown Bess" only in passing).
- Bullet penetration depths in wood, brick, steel and water from an open primary test (FBI ballistic gel protocols are summaries only).
- Javelin release speeds (World Athletics records give distances only).

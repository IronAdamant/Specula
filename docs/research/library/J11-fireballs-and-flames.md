# J11: Fireballs, flames and smoke rise

Fixed after the second check (2026-11-18):
- J11-4: the radiated fraction (0.15 to 0.60) cites Quintiere p. 58 (ATF endnote 15), not p. 111; the ignition and flashover thresholds cite Quintiere p. 61 (endnotes 12 and 13), not pp. 58-61; the point-source rule cites Quintiere p. 58 (endnote 14).
- Sources disagree: TNO's fireball is larger than CCPS's by about 1.5 to 12% (not 5 to 12%), always the larger.
- J11-5: the plume centreline constants (ΔT0 = 25 ..., u0 = 1.0 ...) and the two columns of Table J11-5a computed from them moved under "Unconfirmed".
- Added: MAPP gas (2,020 °C) from the same Flame table; the diffusion-flame table's authors (Schmidt and Symes 2008); where the 565 nm figure sits on the page.
- A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic on the cited formulas, never measurements.

## J11-1. Fireball size and duration against fuel mass (correlations, read directly)

| Quantity | Formula (M = flammable mass in kg) | Source |
|---|---|---|
| Maximum fireball diameter | D_max = 5.8 · M^(1/3) m | CCPS, *Guidelines for Chemical Process Quantitative Risk Analysis*, 2nd ed. (2000), Eq. 2.2.32, p. 207, as quoted by TekRisk PRO documentation "Fire Ball", section 3.1, https://docs.tekrisk.com/en/docs/risk-models/fire-ball (secondary; the CCPS book itself is not open) |
| Alternative diameter | D_max = 2 × 3.24 · M^0.325 m | TNO Yellow Book (CPR 14E) Eq. 6.119, as quoted by the same TekRisk page (secondary) |
| Diameter at ground level, before lift-off | D_ground = 1.3 · D_max | same TekRisk page, Eq. 2.2 (secondary) |
| Duration, M below 30,000 kg | t = 0.45 · M^(1/3) s | AIChE CCPS *CHEF Guide*, p. 199 (web page 239), text under Equation 14-5, https://www.aiche.org/sites/default/files/html/ccps/2979346/new/files/basic-html/page239.html; same values in TekRisk Eq. 2.3 citing CCPS QRA pp. 207-208 |
| Duration, M of 30,000 kg or more | t = 2.6 · M^(1/6) s | same two sources |
| Height of fireball centre | H = 0.75 · D_max (CCPS); Kakosimos uses H = 1.0 · D_max | TekRisk section 3.1 citing CCPS QRA p. 211 and Kakosimos (secondary) |
| Distance for severe burns | X = 2.64 · t^0.379 · M^0.307 m (based on LNG) | CCPS CHEF Guide p. 199, Equation 14-5 (same link) |

Observed case: San Juanico (Mexico, 1984) fireballs were estimated at 200-300 m across, lasting about 20 s; injuries at 300 m (Feyzin) and 400 m (San Juanico). Wikipedia "Boiling liquid expanding vapor explosion", section "BLEVE-fireball", citing Mannan (2012) p. 1541, https://en.wikipedia.org/wiki/Boiling_liquid_expanding_vapor_explosion (secondary).

### Table J11-1a. Fireball size against mass (computed by me from the formulas above)

| Fuel mass M | D_max (CCPS) | D_max (TNO) | Duration t | Centre height (0.75 D) |
|---|---|---|---|---|
| 1 kg | 5.8 m | 6.5 m | 0.45 s | 4.3 m |
| 10 kg | 12.5 m | 13.7 m | 0.97 s | 9.4 m |
| 100 kg | 26.9 m | 28.9 m | 2.1 s | 20.2 m |
| 1,000 kg | 58.0 m | 61.2 m | 4.5 s | 43.5 m |
| 10,000 kg | 125 m | 129 m | 9.7 s | 94 m |
| 30,000 kg | 180 m | 185 m | 14.5 s (switches to the M^(1/6) branch here) | 135 m |
| 100,000 kg | 269 m | 273 m | 17.7 s | 202 m |

Check against the observed case: 100 t gives 269 m and 17.7 s, close to San Juanico's 200-300 m and about 20 s (comparison by me; the San Juanico fuel mass was not read).

## J11-2. Flame temperatures by fuel (read directly, secondary)

All from Wikipedia "Flame", table "Flame temperatures for various common substances (in 20 °C air at 1 atm)", https://en.wikipedia.org/wiki/Flame. The rows carry no individual citations except butane (Pearlman 2000), so treat them as rough.

| Fuel or flame | Temperature |
|---|---|
| Butane cool flame in low gravity | about 300 °C |
| Charcoal fire | 750-1,200 °C |
| Methane (natural gas) | 900-1,500 °C |
| Bunsen burner | 900-1,600 °C, depending on the air valve |
| Candle flame | about 1,100 °C over most of the flame; hot spots 1,300-1,400 °C |
| Propane blowtorch | 1,200-1,700 °C |
| Backdraft flame peak | 1,700-1,950 °C |
| Magnesium | 1,900-2,300 °C |
| Hydrogen torch | up to about 2,000 °C |
| Acetylene blowlamp | up to about 2,300 °C |
| MAPP gas | 2,020 °C |
| Oxyacetylene | up to 3,300 °C |

Maximum diffusion-flame temperatures in air (same page, second table, citing Schmidt and Symes (2008), *The analysis of burned human remains*, pp. 2-4): animal fat 800-900 °C; kerosene 990 °C; gasoline 1,026 °C; wood 1,027 °C; methanol 1,200 °C; charcoal with forced draft 1,390 °C.

Adiabatic flame temperatures in air at constant pressure (Wikipedia "Adiabatic flame temperature", table "Adiabatic flame temperature (constant pressure) of common fuels", https://en.wikipedia.org/wiki/Adiabatic_flame_temperature): methane 1,963 °C and methanol 1,949 °C (both citing CRC Handbook 96th ed. p. 15-51); propane 1,980 °C; ethane 1,955 °C; hydrogen 2,254 °C; gasoline 2,138 °C; ethanol 2,082 °C; acetylene 2,500 °C in air and 3,480 °C in oxygen. These are theoretical ceilings; real open flames lose heat and run cooler (see the first table).

## J11-3. Flame colour (read directly, secondary)

From Wikipedia "Flame", section "Color", https://en.wikipedia.org/wiki/Flame:

- A sooty diffusion flame (Bunsen burner with the air inlet closed) is yellow, peak about 2,000 K; the yellow is incandescence of very fine soot.
- With enough premixed air there is no soot and the flame is blue; the blue comes from excited CH and C2 radicals, and almost all the light of a premixed butane flame is below about 565 nm (this figure is in the caption of a butane-torch image on the page, not in the body text).
- For flame colour against temperature for the soot glow, use the blackbody table in J12.

Metal-salt colours (images in `images/1-fire-smoke/`): sodium gives a strong yellow (photo caption, Søren Wedel Nielsen, Commons, CC BY-SA 3.0). Exact emission lines were not read from a primary line list in this pass (see Not found).

## J11-4. Flame height (correlations, read directly)

| Correlation | Formula (Q in kW, D in m, heights in m) | Source |
|---|---|---|
| Heskestad mean flame height | L = 0.235 · Q^(2/5) − 1.02 · D | Fire Safety Academy wiki "Heskestad plume", citing Heskestad (2002), *SFPE Handbook* 3rd ed., ch. 2-1, https://fire-safety.academy/wiki/Heskestad_plume; also NUREG-1805 ch. 3 (search-result extract only; nrc.gov blocked the box) |
| Same, simplified | L = 0.23 · Q^(2/5) − 1.02 · D, valid for 7 < Q^(2/5)/D < 700 kW^(2/5)/m; not for flames in a crosswind | Wiley appendix "Flame Radiation Review", Eq. A.6, doi:10.1002/9781118903117.app1 (search-result extract only); same 0.23 form in ATF paper below |
| McCaffrey persistent (continuous) flame | Z_c = 0.08 · Q^(2/5) | R. J. Schaal (ATF), "Field Application of Fire Dynamics Equations", flame-height section, citing Icove & DeHaan (2004) p. 81, https://www.cfitrainer.net/Programs/491262f3-f01d-4340-8a11-a1d072d7ff86/Document/fieldApplicationOfFireDynamicsEquations.pdf |
| McCaffrey intermittent flame tip | Z_i = 0.20 · Q^(2/5) | same |
| NFPA 921 with wall factor | H = 0.174 · (k · Q)^0.4, k = 1 open floor, 2 against one wall, 4 in a corner | same ATF paper, citing NFPA 921 (2004) p. 29 |
| Radiated fraction of fire power | 0.15 (low-soot fuels such as methane) to 0.60 (high-soot fuels such as polystyrene) | same ATF paper, endnote 15 ("Ibid (58)"): Quintiere p. 58 |
| Point-source radiant flux | q = X_r · Q / (4π R²), valid beyond about two flame diameters | same ATF paper, endnote 14: Quintiere p. 58 |
| Ignition thresholds | thin fuels ignite from about 10 kW/m², thick fuels from about 20 kW/m²; smoke layer at 500-600 °C gives 20-25 kW/m² at the floor (flashover onset) | same ATF paper, endnotes 12 and 13: Quintiere p. 61 |

ATF worked example (read directly): Q = 1,000 kW, D = 1 m gives persistent 1.3 m, intermittent 3.2 m, NFPA 2.8 m (open floor) or 4.8 m (corner), pool-fire 2.6 m.

### Table J11-4a. Flame heights (computed by me; Heskestad 0.235 form, McCaffrey 0.08 and 0.20)

| Fire | Q | D | Heskestad L | Persistent | Intermittent tip |
|---|---|---|---|---|---|
| Small waste bin (illustrative) | 50 kW | 0.1 m | 1.0 m | 0.4 m | 1.0 m |
| Armchair-sized (illustrative) | 500 kW | 0.5 m | 2.3 m | 1.0 m | 2.4 m |
| 1 MW pool, 1 m | 1,000 kW | 1 m | 2.7 m | 1.3 m | 3.2 m |
| 5 MW pool, 2 m | 5,000 kW | 2 m | 5.1 m | 2.4 m | 6.0 m |
| 20 MW pool, 4 m | 20,000 kW | 4 m | 8.3 m | 4.2 m | 10.5 m |

The Q values are chosen inputs, not measured item burn rates.

## J11-5. Smoke rise above the flames (plume, read directly)

Fire Safety Academy wiki "Heskestad plume" (citing Heskestad 2002, SFPE Handbook ch. 2-1), https://fire-safety.academy/wiki/Heskestad_plume, and the search-engine summary of NFPA 92 calculators:

- Virtual origin: z0 = 0.083 · Q^(2/5) − 1.02 · D (Q total, kW).
- Convective part: Q_c ≈ 0.7 Q is the usual assumption (0.8 for clean-burning methanol).
- Plume mass flow above the flames: m_p = 0.071 · Q_c^(1/3) · (z − z0)^(5/3) + 0.00192 · Q_c kg/s; inside the flames m_p = 0.0056 · Q_c · (z/L) (SFPE prints 0.0059 in one place; sources differ).
- Centreline temperature rise and centreline speed: the constants this file had are unconfirmed (see "Unconfirmed" below).
- Worked example on the wiki: 2,600 kW pallet stack, 1.21 m equivalent diameter, flame 4.23 m; at 10 m the plume carries about 39.1 kg/s.

### Table J11-5a. Plume above a 1 MW, 1 m fire (computed by me; Q_c = 700 kW, z0 = 0.30 m)

| Height z | Plume mass flow |
|---|---|
| 3 m (just above the 2.7 m flame) | 4.7 kg/s |
| 4 m | 6.9 kg/s |
| 6 m | 12.8 kg/s |
| 10 m | 29 kg/s |
| 20 m | 92 kg/s |

The table's centreline temperature and speed columns rest on the unconfirmed constants and are moved under "Unconfirmed" below.

## Sources disagree

- Flame-height constant: 0.235 (Heskestad via the wiki, NUREG-1805) against 0.23 (ATF, Wiley appendix). The difference is about 2%.
- Fireball diameter: CCPS 5.8 M^(1/3) against TNO 6.48 M^0.325; TNO is always the larger, by about 1.5-12% over 1 kg to 100 t: +11.7% at 1 kg, falling to +1.5% at 100 t (ratios 1.117, 1.096, 1.075, 1.055, 1.035, 1.025, 1.015 over the rows of Table J11-1a, as recomputed by the second checker; this file first said "within about 5-12%").
- Fireball duration branch: the CHEF Guide writes the second branch as "M>30,000", TekRisk as "≥" (a trivial difference).
- Fireball centre height: 0.75 D (CCPS) against 1.0 D (Kakosimos).
- Candle flame: about 1,100 °C (Flame table) against the 1,850 K colour temperature for candle light in J12 (physicallybased.info). They measure different things (gas temperature against the colour of the emitted light), so both can stand.

## Not found

- Fireball correlations read from an open primary (the CCPS and TNO books are paid; the box could not open NUREG-1805 at nrc.gov, HTTP 403).
- Wood, cloth and furniture burn rates (kW) from an open primary table.
- Metal-salt flame emission wavelengths (sodium, copper, strontium) from an open primary line list such as the NIST Atomic Spectra Database: not read in this pass.
- Smoke colour by fuel as measured values.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives the values has been opened (the checker suggests Heskestad, *SFPE Handbook* ch. 2-1, or NUREG-1805 ch. 9).

- Plume centreline temperature rise above the flame tip: ΔT0 = 25 · Q_c^(2/3) · (z − z0)^(−5/3) K; centreline speed u0 ≈ 1.0 · Q_c^(1/3) · (z − z0)^(−1/3) m/s (search extract of dgaconsulting.net and fire-safety.academy "Virtual origin"; normal air). The checker's reason: the coefficients 25 and 1.0 are on no page it opened; the live "Virtual origin" page (https://fire-safety.academy/wiki/Virtual_origin) gives only the exponents (−5/3, 5/3 and the falling velocity), not the coefficients.
- The columns of Table J11-5a that depend on them (computed by me; the arithmetic is right, the constants are not confirmed):

| Height z | Centreline ΔT | Centreline speed |
|---|---|---|
| 3 m | about 375 K | 6.4 m/s |
| 4 m | 222 K | 5.7 m/s |
| 6 m | 108 K | 5.0 m/s |
| 10 m | 45 K | 4.2 m/s |
| 20 m | 14 K | 3.3 m/s |

## Sources

- CCPS, *Guidelines for Chemical Process Quantitative Risk Analysis*, 2nd ed. (2000), Eq. 2.2.32 p. 207, pp. 207-208, p. 211 (paid; read as quoted by TekRisk). No link. Not opened.
- TNO Yellow Book (CPR 14E), Eq. 6.119 (read as quoted by TekRisk). No link. Not opened.
- Kakosimos (read as quoted by TekRisk). No link. Not opened.
- TekRisk PRO documentation, "Fire Ball", section 3.1. https://docs.tekrisk.com/en/docs/risk-models/fire-ball . Accessed 2026-11-17.
- AIChE CCPS, *CHEF Guide*, p. 199 (web page 239), Equation 14-5. https://www.aiche.org/sites/default/files/html/ccps/2979346/new/files/basic-html/page239.html . Accessed 2026-11-17.
- Wikipedia, "Boiling liquid expanding vapor explosion", section "BLEVE-fireball" (citing Mannan (2012) p. 1541). https://en.wikipedia.org/wiki/Boiling_liquid_expanding_vapor_explosion . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Flame" (tables and section "Color"; citing Pearlman 2000, and Schmidt and Symes (2008), *The analysis of burned human remains*, pp. 2-4). https://en.wikipedia.org/wiki/Flame . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Adiabatic flame temperature" (citing the CRC Handbook 96th ed. p. 15-51). https://en.wikipedia.org/wiki/Adiabatic_flame_temperature . CC BY-SA 4.0. Accessed 2026-11-17.
- Søren Wedel Nielsen, sodium flame test photograph, Wikimedia Commons (in `images/1-fire-smoke/flametest-na-swn.jpg`). CC BY-SA 3.0. Accessed 2026-11-17.
- Fire Safety Academy wiki, "Heskestad plume" (citing Heskestad (2002), *SFPE Handbook* 3rd ed., ch. 2-1). https://fire-safety.academy/wiki/Heskestad_plume . Accessed 2026-11-17.
- Fire Safety Academy wiki, "Virtual origin". https://fire-safety.academy/wiki/Virtual_origin . Accessed 2026-11-17.
- dgaconsulting.net (search-result extract only; no page opened). No link.
- NUREG-1805, ch. 3 (search-result extract only; nrc.gov refused the fetch). No link. Not opened.
- Wiley appendix, "Flame Radiation Review", Eq. A.6, doi:10.1002/9781118903117.app1 (search-result extract only). https://doi.org/10.1002/9781118903117.app1 . Not opened.
- R. J. Schaal (ATF), "Field Application of Fire Dynamics Equations" (citing Icove and DeHaan (2004) p. 81, NFPA 921 (2004) p. 29, and Quintiere pp. 58 and 61). https://www.cfitrainer.net/Programs/491262f3-f01d-4340-8a11-a1d072d7ff86/Document/fieldApplicationOfFireDynamicsEquations.pdf . Accessed 2026-11-17.
- NFPA 92 calculators (search-engine summary only). No link.
- J12 of this library (physicallybased.info candle colour temperature), for the candle-flame comparison.

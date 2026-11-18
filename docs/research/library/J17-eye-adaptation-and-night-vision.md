# J17: Eye adaptation, Purkinje shift, night-vision thresholds

Fixed after the second check (2026-11-18):
- J17-1: the mesopic range (0.01 to 3.0 cd/m²) and the street-lighting sentence cite Stockman and Sharpe 2006 and CIE Publication No. 41 (1978), not "CIE and Rea et al.".
- Sources disagree: the same page gives rods "about two hours" to reach maximum sensitivity (Passer and Smith 2008) against 30 to 45 minutes (Britannica).
- Added, from pages already cited: the eye's momentary contrast ratio of about 1,000; dark adaptation quicker and deeper in the young; rods insensitive beyond about 640 nm.
- Licence of Wikipedia stated; a "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Useful for an auto-exposure model, a night look, and how fast the view recovers after a flash.

## J17-1. Vision regimes by luminance (read directly)

| Regime | Luminance range | Source |
|---|---|---|
| Absolute threshold | about 1 µcd/m² (10⁻⁶) | Wikipedia "Orders of magnitude (illuminance)", luminance section, https://en.wikipedia.org/wiki/Orders_of_magnitude_(illuminance) (secondary) |
| Scotopic (rods only) | 10⁻³ down to 10⁻⁶ cd/m² | Wikipedia "Scotopic vision", https://en.wikipedia.org/wiki/Scotopic_vision, citing Pokorny et al. 1979 for 10⁻³; the 10⁻⁶ end is flagged "citation needed" (secondary) |
| Scotopic to mesopic boundary | about 5 mcd/m² (0.005) | Orders of magnitude (luminance) |
| Mesopic (rods and cones together) | about 0.01 to 3.0 cd/m²; "most nighttime outdoor and street lighting conditions" | Wikipedia "Mesopic vision", https://en.wikipedia.org/wiki/Mesopic_vision, citing Stockman and Sharpe 2006 and CIE Publication No. 41 (1978) for this range and the street-lighting sentence (Rea et al. 2004 is cited later on the page, only for the unified photometry system) (secondary) |
| Mesopic, other definition | 10⁻³ to 10^0.5 (about 3.2) cd/m² | Wikipedia "Scotopic vision" (flagged "citation needed") |
| Mesopic to photopic boundary | about 5 cd/m² | Orders of magnitude (luminance) |
| Whole working range of the eye | about nine orders of magnitude | Wikipedia "Adaptation (eye)", https://en.wikipedia.org/wiki/Adaptation_(eye) (secondary) |
| Contrast seen at any one moment | a contrast ratio of only about 1,000 | Wikipedia "Adaptation (eye)" (secondary; useful for tone mapping) |

Matching scenes (from J16): full-Moon scene about 1.4 mcd/m², so moonlit landscapes sit at the scotopic and mesopic edge; floodlit buildings (2 cd/m²) are mesopic; anything above about 5 cd/m² is full colour vision.

## J17-2. Spectral sensitivity and the Purkinje shift (read directly)

| Item | Value | Source |
|---|---|---|
| Cone (photopic) peak | 555 nm, 683 lm/W | Wikipedia "Mesopic vision"; CIE photopic table maximum found by me at 555 nm (see J12-4) |
| Rod (scotopic) peak | about 507 nm, about 1,700 lm/W | Wikipedia "Mesopic vision" and "Purkinje effect"; CIE scotopic table maximum found by me at 506 nm on its 1 nm grid |
| Rod pigment | rods most sensitive around 498 nm (blue-green) | Wikipedia "Scotopic vision", citing Bowmaker and Dartnall 1980, *J. Physiol.* 298, 501-511 |
| Purkinje shift | rhodopsin absorbs best at about 500 nm, the cone opsins that dominate in daylight at about 555 nm; peak sensitivity moves toward blue at low light as part of dark adaptation | Wikipedia "Purkinje effect", https://en.wikipedia.org/wiki/Purkinje_effect (secondary) |
| Red light keeps dark adaptation | rods are insensitive to long wavelengths, so red light does not bleach them; rods are insensitive to wavelengths longer than about 640 nm | Wikipedia "Adaptation (eye)"; the 640 nm figure from Wikipedia "Scotopic vision", citing Frisby |
| Moonlight looks blue | an illusion from the Purkinje effect; moonlight's spectrum has slightly more long-wavelength (red) light than daylight and is almost the same as the Sun's | Wikipedia "Moonlight", https://en.wikipedia.org/wiki/Moonlight |

Game use: at night, weight luminance by the scotopic curve (peak about 507 nm), desaturate below about 3 cd/m², and push the rendered tint slightly blue; reds go dark first.

## J17-3. Adaptation times (read directly, secondary)

All from Wikipedia "Adaptation (eye)" (citing Passer and Smith 2008; *Encyclopaedia Britannica* 1987; Hurley 2002, *J. Gen. Physiol.* 119(2) 125-128):

| Change | Time | Note |
|---|---|---|
| Bright sun to full darkness | about 20-30 minutes | eye becomes 10,000 to 1,000,000 times more sensitive than in full daylight |
| Cones reach maximum dark sensitivity | 9-10 minutes | |
| Rods reach maximum | 30-45 minutes (Britannica); "about two hours" (Passer and Smith 2008), on the same page | rods take over from cones after about 5 minutes; see Sources disagree |
| Age | dark adaptation is quicker and deeper in young people than in the elderly | same page |
| Rod pathway improves considerably | within 5-10 minutes | |
| Darkness to bright sunlight | about 5 minutes | light adaptation |

### Table J17-3a. A simple exposure model fitted to these times (computed by me, not a measurement)

Treating dark adaptation as a gain that rises by about 10⁴ to 10⁶ over 25 minutes gives roughly 0.16-0.24 decades (0.5-0.8 stops) per minute on average; light adaptation (about 5 minutes for a similar range) is about 5 times faster. A real curve has two branches: a fast cone branch in the first 5-10 minutes, then the slower rod branch.

## Sources disagree

- Mesopic range: 0.01-3 cd/m² ("Mesopic vision"), 0.005-5 cd/m² (thresholds in "Orders of magnitude (luminance)", which match the CIE 191:2010 mesopic system, not read directly this pass), 10⁻³-10^0.5 cd/m² ("Scotopic vision", uncited).
- Rods' time to maximum sensitivity: Wikipedia "Adaptation (eye)" says rods "do not reach their maximum sensitivity for about two hours" (citing Passer and Smith 2008) and, a few lines later, 30-45 minutes (citing *Encyclopaedia Britannica*). It matters for a long-night adaptation curve.
- Rod peak: 498 nm (rod pigment, Bowmaker and Dartnall), about 500 nm (rhodopsin, "Purkinje effect"), 507 nm ("Mesopic vision", scotopic luminosity function), 506 nm in the CIE 1 nm table. The pigment absorption and the whole-eye scotopic function differ because of the lens and eye media.

## Not found

- An open, citable dark-adaptation curve (threshold against time) with data points.
- Flash-blindness recovery time against flash luminance.
- Glare and veiling-luminance formulas from an open source.

## Sources

Wikipedia text is CC BY-SA 4.0; only facts are taken from it. The primaries Wikipedia cites were not opened.

- Wikipedia, "Orders of magnitude (illuminance)", luminance section. https://en.wikipedia.org/wiki/Orders_of_magnitude_(illuminance) . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Scotopic vision" (citing Pokorny et al. 1979; Bowmaker and Dartnall 1980, *J. Physiol.* 298, 501-511; Frisby). https://en.wikipedia.org/wiki/Scotopic_vision . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Mesopic vision" (citing Stockman and Sharpe 2006; CIE Publication No. 41 (1978); Rea et al. 2004). https://en.wikipedia.org/wiki/Mesopic_vision . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Adaptation (eye)" (citing Passer and Smith 2008; *Encyclopaedia Britannica* 1987; Hurley 2002, *J. Gen. Physiol.* 119(2) 125-128). https://en.wikipedia.org/wiki/Adaptation_(eye) . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Purkinje effect". https://en.wikipedia.org/wiki/Purkinje_effect . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Moonlight". https://en.wikipedia.org/wiki/Moonlight . CC BY-SA 4.0. Accessed 2026-11-17.
- CIE photopic and scotopic tables (CIE 018:2019, via J12-4 of this library; CC BY-SA 4.0). https://files.cie.co.at/Publications-datasets/CIE_sle_photopic.csv ; https://files.cie.co.at/Publications-datasets/CIE_sle_scotopic.csv . Accessed 2026-11-17.
- CIE 191:2010 (named in "Sources disagree"; not read). No link.
- J16 of this library (matching scenes).

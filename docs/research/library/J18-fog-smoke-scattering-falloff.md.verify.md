# Second check: J18-fog-smoke-scattering-falloff.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J18-fog-smoke-scattering-falloff.md

**30 rows: 29 VERIFIED, 0 FIX, 1 UNVERIFIABLE.**

- UNVERIFIABLE: J18-3 "large droplets scatter all colours equally, so fog looks white (Mie)". The file says it has no source; neither Wikipedia "Visibility" nor "Fog" states it in those words.
- Source upgrades (values unchanged):
  - WMO MOR row was "read via search extract". Now read directly from the PDF: definition on PDF p.1 (section 9.1.1), MOR = (1/σ)·ln(1/0.05) ≈ 3/σ on PDF p.5 (eq. 9.5). Drop the "search extract" note.
  - FDS rows were via PyroSim/fdsrun (secondary). The primary NIST FDS User's Guide (SP 1019, Sixth Edition, revision FDS-6.11.1), section 22.10.5 "Visibility and Obscuration", p.362, eq. 22.23, states S = C/K, C = 8 light-emitting, C = 3 light-reflecting (ref. [85] Mulholland, SFPE Handbook 3rd ed. 2002), VISIBILITY_FACTOR default 3, and K_m default 8,700 m²/kg for flaming wood and plastics. Cite this instead.
- Missing spread: FDS footnote 7 gives K_m = 8,700 ± 1,100 m²/kg at 633 nm for most flaming fuels (ref. [86] Mulholland & Croarkin, Fire and Materials 24 (2000) 227-230). Table J18-2a soot column therefore spans about ±13% (e.g. 1 m visibility: 306-395 mg/m³, computed by me).
- Missing disagreement: WMO (PDF p.3, section 9.1.2) says proposed contrast thresholds range from 0.0077 to 0.06 "or even 0.2", and aviation takes 0.05 because runway markings have low contrast. This is wider than Wikipedia's 0.018-0.03 and explains why WMO uses 5%.
- Gap: every Wikipedia "Visibility" sentence used here (0.018-0.03, 3.912, 13.2e-6 /m, 296 km, 0.1-1.0 µm, fog/mist/haze bands, zero under 100 m) carries no inline reference on the page. Values match the page but rest on Wikipedia alone. Fog < 1 km is also on Wikipedia "Fog", cited to the US Federal Meteorological Handbook No. 1 ch. 8.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 3.2 | Fog white (Mie) | UNVERIFIABLE | no source given or found |, |

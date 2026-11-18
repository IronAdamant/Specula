# Second check: J11-fireballs-and-flames.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J11-fireballs-and-flames.md

**Rows: 37 total: 34 VERIFIED, 2 FIX, 1 UNVERIFIABLE.**

- **FIX 1 (citation page, J11-4 radiated fraction):** in the ATF paper, the 0.15-0.60 radiated-fraction sentence carries endnote 15, which is "Ibid (58)", i.e. **Quintiere p. 58**, not p. 111. (Endnote 5, p. 111, belongs to the mass-loss-rate input.)
  - Also, the ignition thresholds and the 500-600 °C / 20-25 kW/m² flashover sentence carry endnotes 12 and 13, which are both **Quintiere p. 61**. The file's "pp. 58-61" over-spans.
  - The point-source "more than two flame diameters" sentence is endnote 14, Quintiere p. 58.
- **FIX 2 (Sources disagree, fireball diameter):** TNO/CCPS recomputes as **+11.7% at 1 kg, falling to +1.5% at 100 t** (1.117, 1.096, 1.075, 1.055, 1.035, 1.025, 1.015). So the file's "within about 5-12%" should read "about 1.5-12%, with TNO always larger".
- **UNVERIFIABLE (J11-5):** I couldn't find the plume centreline constants **ΔT0 = 25 · Q_c^(2/3)(z−z0)^(−5/3)** and **u0 = 1.0 · Q_c^(1/3)(z−z0)^(−1/3)** on any page I opened.
  - The file sources them to a "search extract" of dgaconsulting.net and fire-safety.academy "Virtual origin".
  - The live Virtual origin page (opened) gives only the exponents (−5/3, 5/3 and the falling velocity), not the coefficients 25 and 1.0.
  - The ΔT and speed columns of Table J11-5a are arithmetically correct, but they depend on these unverified constants. Read Heskestad's SFPE ch. 2-1 or NUREG-1805 ch. 9 before shipping.
- **Primary and secondary pages opened, all matching:**
  - TekRisk §3.1: Eq. 2.1 5.8 M^(1/3) citing CCPS Eq. 2.2.32 p. 207; TNO 2 × 3.24 M^0.325 (Eq. 6.119); D_ground 1.3 D_max (Eq. 2.2); 0.45 M^(1/3) / 2.6 M^(1/6) split at 30,000 kg (pp. 207-208); H 0.75 D (p. 211); Kakosimos 1.0 D.
  - The CCPS CHEF Guide p. 199 (web p. 239, opened via a web fetch, since curl gets 403): Eq. 14-5, the durations and "based on liquified natural gas" are all verbatim.
  - The CHEF text says "M>30,000" for the second branch and TekRisk says "≥". It's trivial.
  - Fire Safety Academy "Heskestad plume" has every coefficient verbatim (0.235, 0.083, 0.071, 0.00192, 0.0056, and the 0.0059 note), plus Q_c 0.7 (0.8 methanol) and the pallet example (2,600 kW, 1.21 m, 4.23 m, 39.1 kg/s).
  - The ATF/Schaal PDF has McCaffrey 0.08 and 0.20 (endnote 7, Icove p. 81); Heskestad .23 Q^.4 − 1.02 D; and NFPA .174 (WALL·Q)^.4 with wall factors 1, 2 and 4 (endnote 10, NFPA 921 p. 29).
  - Its worked example is 1.3, 3.2, 2.8 / 4.8 and 2.6 m.
- **Every "computed by me" table recomputes:**
  - Fireball table: all 35 cells match, to rounding.
  - Flame heights: 1.02, 2.31, 2.70, 5.05, 8.26 m and 0.38-4.20 / 0.96-10.51 m.
  - Plume: z0 0.295 m; ΔT 375, 222, 108, 44.6, 13.7; u 6.37, 5.74, 4.97, 4.16, 3.29; ṁ 4.65, 6.94, 12.83, 29.2, 92.0.
  - The pallet example's 39.19 kg/s matches the wiki's 39.1.
- **Wikipedia rows match verbatim:**
  - "Flame" table (only butane cited, Pearlman), the diffusion-flame table (Schmidt & Symes 2008 pp. 2-4) and the colour section (yellow ~2000 K closed inlet; CH and C2 bands; "below about 565 nanometers" is in a butane-torch image caption).
  - "Adiabatic flame temperature": methane 1,963 and methanol 1,949 (CRC 96th p. 15-51), propane 1,980, ethane 1,955, hydrogen 2,254, gasoline 2,138, ethanol 2,082, acetylene 2,500 / 3,480.
  - BLEVE: San Juanico 200-300 m, ~20 s, injuries at 300 m (Feyzin) and 400 m (San Juanico), Mannan 2012 p. 1541.
  - **Gap:** the Flame table also lists MAPP gas at 2,020 °C, which the file omits.
- **Image:** `images/1-fire-smoke/flametest-na-swn.jpg` exists. Commons confirms CC BY-SA 3.0, by Søren Wedel Nielsen (Swn).

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| J11-4 Radiated fraction | 0.15-0.60, "Quintiere p. 111" | **FIX** | The endnote is Quintiere p. 58 | same |
| J11-5 ΔT0, u0 | 25 and 1.0 coefficients | **UNVERIFIABLE** | Snippet only; not on any opened page | https://fire-safety.academy/wiki/Virtual_origin |
| Disagree CCPS vs TNO | "5-12%" | **FIX** | 1.5-12% |, |

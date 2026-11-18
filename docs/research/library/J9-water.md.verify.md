# Second check: J9-water.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J9-water.md

**Rows: 45 total: 44 VERIFIED, 1 FIX, 0 UNVERIFIABLE.** (Viscosity is counted as 17 rows holding 32 values.)

- **FIX (licence, header L3).** OpenStax UP1 is **CC BY-NC-SA 4.0** on all three live pages (§14.1, §14.7 and §17.2 each say "This book uses the Creative Commons Attribution-NonCommercial-ShareAlike License"), not CC BY 4.0. Flag ⚠NC.
- **J9-1:** all 7 water densities match Table 14.2 "Densities of Water" (9.17e2, 9.998e2, 1.000e3, 9.982e2, 9.584e2, 1.670e2, 1.030e3).
  - The researcher's steam note is right to flag the value. The source really prints 1.670 × 10² for steam at 100 °C and 101.3 kPa. My ideal-gas recomputation gives 0.588 kg/m³ (pM/RT, M = 0.018015 kg/mol, T = 373.15 K), so 167 is very likely a source error. Don't use it.
- **J9-2:** all 32 viscosity values match Table 14.4 "Coefficients of Viscosity of Various Fluids" (units ×10⁻³ Pa·s).
  - **Note on mercury 0.0450.** In the source it sits in the *gas* block (between hydrogen and oxygen), so it's presumably mercury vapour. Liquid mercury is far more viscous. The file's "(as printed)" is correct, but add "vapour, gas section" so nobody uses it as liquid mercury.
- The Reynolds number is right: N_R = 2ρvr/η (eq. 14.20). The source states the threshold directly: "below about 2000, flow is laminar … above about 3000, flow is turbulent". The file attributes it to a worked example; it's actually stated in the text, a minor tidy. The 2000-3000 unstable band is missing and worth adding.
- **J9-3:** all 17 sound speeds match Table 17.1 "Speed of Sound in Various Media". Eq. 17.7 v = 331 m/s·√(1 + T_C/273 °C) checks out, as do "331 m/s … 343 m/s". The gases are at 0 °C and the liquids at 20 °C; the solids are labelled "longitudinal or bulk" with no temperature.
- Cross-check recomputed: 1000/1.29 = 775 and 1.002/0.0181 = 55.4. The Hirt quote "800 times denser and 60 times more viscous" is verbatim (checked in the J7 verify).
- Dead links: none.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| Header | All OpenStax UP1, CC BY 4.0 | FIX | CC BY-NC-SA 4.0 | (three pages below) |

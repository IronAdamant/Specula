# Second check: J12-blackbody-and-light-colour.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J12-blackbody-and-light-colour.md

**Rows: 59 total: 55 VERIFIED, 2 FIX, 2 UNVERIFIABLE.**

- **FIX 1 (licence, J12-2 method):** the file says the CIE dataset licence is "not stated on the dataset page I read". The CIE metadata files next to each CSV (`CIE_xyz_1931_2deg.csv_metadata.json`, `CIE_sle_photopic.csv_metadata.json`, `CIE_sle_scotopic.csv_metadata.json`) all give **CC BY-SA 4.0** (rightsIdentifier).
  - The tables can therefore be redistributed with attribution and share-alike. That changes the "not copied here" decision.
  - The table numbers are confirmed in the same metadata: xyz is Table 6, photopic Table 1, scotopic Table 2 (CIE 018:2019).
- **FIX 2 (Sun, J12-3 notes):** the Sun's 128,000 lx is the database's **"Space"** variant, not illuminance on the ground.
  - The database's own description says: "The solar illuminance constant is 128 000 lux in space, but atmospheric extinction on earth brings it down to around 100 000 lux."
  - Its "Midday" variant is 400 / 100,000 / 100,000 lx (range and default) at 5,778 K, and its "Sunset" variant is 2,600 K at 3,000 lx.
  - The notes line "lx rows (Sun, Moon, Sky) are illuminance on the ground" is wrong for the Sun row. Use 100,000 lx for the ground at midday.
- **Missing spreads:** the file takes only the first variant of each light. The database also has:
  - **Sky:** Overcast Day 6,500 K, 100-2,000 lx (default 2,000), and Moonless Clear Night 6,500 K, 0.002 lx.
  - **Moon:** Crescent 0.05 lx.
  - **Arri M8:** Medium 31,671 lm and Spot 22,053 lm.
  - **Car headlight:** Halogen 3,400-4,000 K, 1,000-2,000 lm (default 1,500).
  - **Fluorescent:** 4,000 K and 6,500 K variants.
  - **LED:** White 10,596 K.
  - **Black-body:** 23 temperatures, and **Standard Illuminant** has 53 variants.
- **Car headlight reference:** the row shown is the LED variant, but the file names only the database's first reference, the Halogen chart. The database also cites the Bulb Facts "Aftermarket LED Kits Chart", which belongs to that row.
- **The constants match NIST exactly:** c2 1.438 776 877e-2 m K, c1 3.741 771 852e-16 W m², and b 2.897 771 955e-3 m K.
  - BIPM SI Brochure 9th ed.: "K_cd … is 683 lm/W" in the defining-constants list, and the candela definition is in §2.3.1.
- **Blackbody table (computed by me) recomputes exactly:** I independently integrated Planck's law against the CIE 1931 2° CSV (360-830 nm, 1 nm) using the IEC sRGB matrix.
  - All 23 rows match on peak λ, x, y, linear RGB and hex, to the last printed digit. (The higher-precision sRGB matrix shifts at most 0.001 in one channel, and the hex changes only at 6,000 K, #FFF3F1 vs #FFF3F0.)
  - The V(λ) peak is 555 nm and the V′(λ) peak 506 nm, confirmed from the CSVs. Wikipedia "Purkinje effect" says "maximum at about 500 nm", which also matches.
- **UNVERIFIABLE:**
  - The spot-check claim that 1,000 K "matches the usual Planckian-locus value to the fourth decimal" cites no source. The arithmetic itself is reproduced.
  - "Daylight on the ground is usually quoted nearer 5,500-6,500 K" cites no source.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| Method / CIE data | Table 6, DOI, licence "not stated" | **FIX** | CC BY-SA 4.0 in the metadata | https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv_metadata.json |
| Spot check | Matches the "usual" locus value | UNVERIFIABLE | No source |, |
| Notes | "Sun … illuminance on the ground" | **FIX** | 128,000 lx is space; ground ~100,000 lx (Midday variant) | https://api.physicallybased.info/v2/lightsources |
| Disagree Sun | Ground daylight 5,500-6,500 K | UNVERIFIABLE | Unsourced |, |

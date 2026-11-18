# Second check: J10-other-worlds.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J10-other-worlds.md

**Rows: 36 total: 36 VERIFIED, 0 FIX, 0 UNVERIFIABLE.**

- Every NSSDC value matches the live fact sheets exactly.
- Earth's escape speed is printed as 11.186 km/s on the Earth sheet itself and as 11.19 in the Mars comparison column. Both agree, and the file cites the latter correctly.
- The Earth sheet says 9.80665 is the "standard acceleration … (CODATA 2018) … (exact)".
- **Titan g recomputed:** 6.674e-11 × 1.3455e23 / (2.575e6)² = **1.3543 m/s²**, which matches the file's 1.354. The inputs (mass 1,345.5 ×10²⁰ kg, radius 2,575 km, density 1,880) match the Saturnian satellite sheet's Titan (VI) row.
- The Titan facts page matches word for word: "about 60 percent greater than on Earth", "about 50 feet (15 meters) below the surface", "10 times higher … (600 kilometers)", "nitrogen (about 95 percent) and methane (about 5 percent)", "-179 degrees Celsius".
- **Gaps worth adding (not errors):**
  - Venus's scale height is on the same sheet (15.9 km) but the file omits it.
  - Venus's composition (96.5% CO₂, 3.5% N₂) is omitted too.
  - Earth's diurnal range (283-293 K) and winds (0-100 m/s) are on the Earth sheet but not in the file.
  - The Mars sheet also gives the Viking 1 site pressure as 6.9-9 mb.
  - Titan's surface density stays an open gap, as the file says. I didn't find it on the two NASA pages, and I'm not filling it from memory.
- **Licence:** NSSDC and NASA Science pages are US government works. The file's statement is fine. NASA's image and media guidelines apply to the pictures, not to these numbers.
- Dead links: none. NSSDC times out from my computer's curl (connection timeout after 25 s), but the live pages loaded through a web fetch at check time.

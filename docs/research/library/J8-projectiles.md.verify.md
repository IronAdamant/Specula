# Second check: J8-projectiles.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J8-projectiles.md

**Rows: 14 total: 14 VERIFIED, 0 FIX, 0 UNVERIFIABLE.**

- This file mostly points to other files. Each pointer row matches the file it names:
  - the gravity values match J10-1 (checked live on the NSSDC Earth sheet; see J10's verify);
  - the drag formula, 0.45 and 0.07-0.5 match J4-1 and J4-2;
  - about 1.23 kg/m³ matches J4-4 (1.227 recomputed in J4's verify).
- **J8-1 recomputed.** I used the file's stated inputs: ρ 1.21, C 0.45, g 9.80665, and diameters from circumference via C/π, giving 0.2419 and 0.2196 m.
  - Tennis 24.28, basketball 21.68 and football 20.22 m/s match the file's 24.3 / 21.7 / 20.2.
  - The masses and diameters are the true midpoints of the J2-2 ranges.
- **Inherited caveat:** the OpenStax-derived inputs (ρ 1.21, C 0.45) carry OpenStax's **CC BY-NC-SA 4.0** licence (⚠NC), not CC BY. See the J4 verify. This file states no licence itself, so it's not counted as a FIX here.
- **Caveat on the method:** the 0.45 sphere value is for a smooth sphere in the subcritical range. A real soccer ball near 20 m/s is close to the drag crisis, and tennis-ball felt raises C. The file's "rough defaults" warning covers this.
- Gaps (Magnus lift, javelin and arrow drag) are listed honestly.

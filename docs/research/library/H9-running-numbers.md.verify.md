# Second check: H9-running-numbers.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker. The checker names this file H8-running-numbers.md.

### Round 3: H8-running-numbers.md

**Rows: 18 total: 17 VERIFIED, 1 FIX, 0 UNVERIFIABLE.**

- **Primary check:** I checked against Fukuchi, Fukuchi & Duarte 2017, *PeerJ* 5:e3298, full text via Europe PMC (PMC5426356, licence "cc by"). peerj.com returns 403 to curl, as before.
  - Every Table 4 mean in the file matches exactly.
  - The Table 4 caption confirms 28 subjects at 2.5, 3.5 and 4.5 m/s. The abstract calls them "28 regular runners"; the file says "recreational", a small wording difference.
- **FIX (spreads):** the file gives means only, but Table 4 prints mean ± SD for every row. Add them:
  - Stride length: 1.86±0.11, 2.46±0.15, 2.96±0.20.
  - Cadence: 80.82±4.63, 85.68±5.27, 91.74±6.69.
  - Hip flexion: 43.75±6.06, 52.76±5.75, 60.50±6.06.
  - Hip extension: −3.58±4.85, −7.95±4.58, −11.75±4.78.
  - Knee flexion: 93.52±10.36, 108.68±10.65, 119.12±10.37.
  - Ankle dorsiflexion: 26.36±2.93, 26.54±2.49, 26.79±2.51.
  - Ankle plantarflexion: −16.62±5.50, −20.47±4.71, −23.17±4.72.
  - Eversion: −4.91±2.74, −6.59±2.99, −7.81±3.59.
  - Hip flexion torque: 0.78±0.11, 1.15±0.14, 1.49±0.19.
  - Knee extension torque: 2.84±0.45, 3.18±0.50, 3.41±0.47.
  - Knee negative work: −1.50±0.18, −2.15±0.24, −3.03±0.30.
  - Vertical impulse: 5.04±0.09, 5.12±0.10, 5.19±0.10.
- **Computed values,** all recomputed correctly:
  - Stride time: 0.742, 0.700 and 0.654 s.
  - Steps per minute: 161.6, 171.4 and 183.5. The file's 162, 171 and 183 are fine.
  - Belt speed: 2.505, 3.513 and 4.526 m/s.
  - Stride +59.1%, cadence +13.5%, knee flexion +25.6°.
- **Gaps** (rows in Table 4 the file leaves out):
  - Stride width 0.10, 0.09, 0.08 m.
  - Hip extension torque −1.06, −1.37, −1.67.
  - Ankle plantarflexion torque 2.03, 2.23, 2.34.
  - Ankle dorsiflexion torque.
  - Hip, knee and ankle positive and negative work.
  - Braking and propulsive A-P impulses.
- **Licence:** CC BY 4.0 is stated correctly.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| Spreads | Means only | **FIX** | Add the SDs listed above | https://europepmc.org/article/PMC/PMC5426356 |

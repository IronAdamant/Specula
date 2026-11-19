# Second check: A15-skinfolds.md (2026-11-19)

The delivery's own checker re-opened the sources of the rows below (a separate pass from the one that gathered them; its scripts and page renders stayed with the checker). The rows are copied word for word from its report. **Fixes were applied in the file before it was delivered.** Nothing here was re-opened by the engine's builder: a number is still checked against its source as it is typed into the engine.

**3 rows checked: 1 FIX, 1 UNVERIFIABLE, 1 VERIFIED.**

| File | Value checked | Source re-opened | Status | Note |
|---|---|---|---|---|
| A15 | NHANES III triceps mean/median (men 20–39, women 20–39, BMI≥30 both) | re-parsed exam.dat at the SAS column positions | VERIFIED | 12.5/11.2, 22.3/20.8, 20.1/18.6, 33.0/33.3 |
| A15 | '% beyond caliper' column | recomputed | FIX | it is an unweighted share; header and note now say so |
| A15 | Code 5555 = beyond caliper | CDC codebook | UNVERIFIABLE | still inferred from data, as the file already says |

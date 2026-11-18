# Second check: J19-shape-drag-inertia-tumbling.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J19-shape-drag-inertia-tumbling.md

**53 rows: 49 VERIFIED, 3 FIX, 1 UNVERIFIABLE.**

- FIX (licence): OpenStax *University Physics Volume 1* is **CC BY-NC-SA 4.0**, not CC BY 4.0 (stated on the 10.5 page itself, checked 2026-11-17). Same error as J4/J15.
- FIX (attribution): J19-1 says the Wikipedia "Drag coefficient" table cites aerodyn.org and NASA. It also cites Engineering ToolBox (ref "tool", http://www.engineeringtoolbox.com/drag-coefficient-d_627.html, archived 2010-12-04), placed on the ski-jumper row; NASA backs the 3D plate row and aerodyn.org the table header. Add Engineering ToolBox.
- FIX (attribution): "50% in about 3 s, 90% in about 8 s" is not from Huang 1998; it is the Wikipedia page's own uncited example sentence after the Huang-cited 55 m/s. The numbers do recompute for v = v_t·tanh(g t / v_t) with v_t = 55 m/s: 3.1 s and 8.3 s (computed by me). Label as page arithmetic.
- UNVERIFIABLE: "a human (close to water's density) floats only with air in the lungs" has no source in the file.
- Missing spread / disagreement: Huang's own Physics Factbook essay says terminal speed "is often reported to be approximately 60 m/s", and its bibliography table runs 53-76 m/s (Exploratorium 53, USAFA 54, Ardley 55.6, Tipler 56, Bueche 76). Wikipedia rounds to 55. List 53-76 m/s as the spread.
- Gaps:
  - The Deep Blue handle https://hdl.handle.net/2027.42/62793 returned 403 to my check (may be bot-blocking); the nori-physics.org PDF opens and was used.
  - The Wikipedia drag table has no Reynolds number for most bluff-body rows; the file's "typical values only" caveat covers it.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 1.src | Table cites aerodyn.org and NASA | FIX | also Engineering ToolBox | same |
| 2.lic | OpenStax CC BY 4.0 | FIX | CC BY-NC-SA 4.0 | same |
| 4.3 | 50% in 3 s, 90% in 8 s, citing Huang | FIX | uncited page example; recomputes 3.1/8.3 s | Terminal velocity |
| 5.5 | Human floats only with air in lungs | UNVERIFIABLE | unsourced |, |

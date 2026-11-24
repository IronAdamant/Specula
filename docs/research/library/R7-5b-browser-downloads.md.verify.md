# Verify: R7-5b-browser-downloads.md

**Method:** every tabulated number was checked against pdftotext and, for Henderson Tables 1-4, Cruse Tables 3-4, Musgrave Plate 9 and the thermal/hydraulic formulae, against rendered page images. The Castillo s001 loadings were read from the docx table itself. Arithmetic marked **computed** in the main file was recomputed (544 x 9.81; 22.8 x (11.2/4.11)^3; 22.8 x (6.36/1.92)^3; 1623-544 and 1079/544; 2.244 x (5.0e-4/5.61e-7); Cordonnier Table 1 time/step ratios).

**Result:** the numbers in R7-5b match the files. No corrections. Flags that stand: Henderson Table 3 swaps the Table 2 masses of *Preondactylus* and *Jeholopterus*; Cruse p. 242 says 40 animals while Table 3 caption says n = 20; Cordonnier's printed "height in kilometers" rule h_max = 2.244 u/k needs a unit choice (see the main file); Mei has no default Ks/Kd/Kc/Ke; Musgrave does not state T in degrees; Castillo s001 has no millimetre lengths.

Date: 2026-11-24.

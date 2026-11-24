# Verify: R7-2A-Q8-footfalls.md (adversarial second check, 2026-11-24)

**Method:** a script (`vcheck.py`, kept in scratch) pulled every number of two or more digits out of the file and searched for it in the cached source texts (the F1 journal HTML text and the F2 Europe PMC text). Every number not found was then checked by hand. Duty factors were recomputed independently from the stance and stride means.

| Check | Result |
|---|---|
| F1 Table 2 and 3 rows (stride ± SD, N, stance ± SD for each limb) | All 8 rows match the source, read cell by cell. The table header in the cached text is shifted by one column; I re-read it as stride, N, then stance, swing and N for LF, RF, LH and RH |
| F1 Table 4 ratios and N | All 6 rows match |
| Duty factors (**computed**) | Recomputed; all 32 values match to 2 decimals |
| F2 quotes (0.25 limb phasing, duty 0.70-0.60, 0.55-0.30, 0.30 to 0.20; speeds 1.4-1.8, 2.5-6.5, 8.5-12.0, 9.0-15.0, 17.0-19.0 m/s; footfall orders) | All found word for word |
| Citations | F1 and F2 checked against Crossref (F1: 210(2): 187-197, doi:10.1242/jeb.02611; F2: sole author Vincelette; Hobbs and Clayton are academic editors, not authors) |
| Numbers not in the cached sources | duty factors and LH = 0 phases (computed, labelled); F3's 2.0-4.6 m/s and 200 ms (from a search-extract abstract, labelled "abstract only"); the 2026 J Neurophysiol figures (explicitly marked not verified and kept out of the tables) |

**Errors found and fixed:**
1. F3 was first cited as *Journal of Motor Behavior*. The ScienceDirect PII prefix (0091-6773) is *Behavioral Biology*; corrected, and volume and pages marked as unconfirmed.

**Remaining weaknesses (not errors):**
- The LH = 0 phases are an idealised pattern built from a review's limb phasing values, not measured means.
- F1 is Icelandic horses only (n = 8); other breeds may differ.
- Duty factors are ratios of means, so they carry no SD.
- The cat data rest on abstracts only.

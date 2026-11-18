# Second check: J26-swords-and-hand-weapons.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J26-swords-and-hand-weapons.md

**46 rows checked: 34 VERIFIED, 3 FIX, 9 UNVERIFIABLE.** All 20 Met objects in J26-1 match the live Collection API exactly: accession, date, culture, overall length, blade length and weight, all flagged isPublicDomain. I checked the Met's CC0 waiver against its Open Access README on GitHub (the policy page timed out). Both hobbyist cut studies match number for number.

FIX:
- Row 22 (the "reading the table" summary): the weight bands leave out two objects in the table.
  - The **ca. 1400 sword 32.75.225 weighs 1,673 g**, with an 81.3 cm blade. It is the heaviest object in J26-1 and fits neither the 0.85-1.2 kg single-hand band nor the 1.2-1.55 kg band.
  - The 1500 Italian sword 36.149.14 has a 66 cm blade, below the stated 75-82 cm.
  - The bands should either be widened to 0.85-1.67 kg or name these outliers.
- Row 35 (Gladius): the page doesn't give "blade 42.5-59 cm found". The infobox gives **blade (part_length) 37.5-65 cm**. The weight (1.2-1.6 kg) and width (3.6-9 cm) are right.
- Row 39 (Pike):
  - The "citation needed" tag sits on the sentence *before* the weight. The 2-6 kg weight carries a reference (renaissancewarfare.com) and cites Sir John Smythe.
  - The "head about 50 cm" belongs to the **Schefflin**, a related polearm, not the pike.
  - Missing spread: the page gives pike length as 3-7 m.

UNVERIFIABLE:
- Rows 24-31 (J26-2 for Sword, Saber, Dagger, Axe, Mace, Spear, Halberd, Katana): I couldn't reproduce these. The Met retired the v1 search endpoint on 2026-10-01, and v1.1 is paginated. Re-running "departmentId=4, q=<type>, first 40, public domain with a weight" on v1.1 gave the following.
  - Rapier matched exactly (row 23): 40 objects, median 1,247 g, 312-1,673 g, median 120.0 cm, 87.6-144.1 cm.
  - Mace: same count (21) and identical ranges, but median 1,094 g against 1,162.
  - Spear and Halberd: ranges match; medians are close but not equal.
  - Sword: weight range matches exactly (329-1,928 g), but the median is 1,049 g against 992, from 33 objects against 35.
  - Saber, Dagger and Axe differ widely: for example Dagger is 22 objects, median 332 g, 74-624 g.
  - Katana: only 1 object came back with a weight, against 3 in the file.
  - **The file should list the object IDs behind each group** so the medians can be checked.
- Row 43 (Iaido 17 m/s): seen in a search summary only, as the file says. Crossref confirms the paper exists: Okamoto and Adachi, *J. Mech. Eng. Autom.* 2016. The EUROCAST DOI also resolves: Kluwak et al., "Reference Datasets for Analysis of Traditional Japanese and German Martial Arts", LNCS 2022.

Missing spreads and context:
- Longsword: the article's prose gives about 2-3 kg and a 15-30 cm grip, against the infobox's 1.8-3 kg and 20-30 cm.
- Zweihänder: the prose gives "typically at least 140 cm and around 2 kg". The over-4 kg parade claim is tagged citation needed.
- Katana: by the late 16th century the average had returned to about 73 cm, after about 60 cm in the early 16th century.
- Science and Fiction gives a 1.2 kg sword mass and says useful fight cuts are "somewhat below 100 J".
- Ensis Sub Caelo: cut duration 0.5 s, peak user power 150 W.
- Plate armour: the 50 kg jousting armour is late 15th to 16th century.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 22 | Weight and blade bands | FIX | 1,673 g and 66 cm outliers | (computed) |
| 24-31 | Other groups | UNVERIFIABLE | not reproducible | v1.1 search |
| 35 | Gladius blade 42.5-59 cm | FIX | 37.5-65 cm | https://en.wikipedia.org/wiki/Gladius |
| 39 | Pike 2-6 kg (cn); head 50 cm | FIX | weight is cited; the 50 cm head is the Schefflin | https://en.wikipedia.org/wiki/Pike_(weapon) |
| 43 | Iaido 17 m/s | UNVERIFIABLE | snippet only | https://doi.org/10.17265/2159-5275/2016.04.004 |

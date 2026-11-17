# H5 verification: motion-capture dataset licences

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**This is not legal advice, and it is not a source of licence text for the engine.** It checks `H5-mocap-licences.md`. H5 itself was not edited. All URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at the start of the check: mtime **2026-11-16 10:33:15 AEDT**.
- Motion Data edited H5 at **2026-11-17 08:56:07 AEDT** (22,583 bytes). Diff and number-diff show that **only the access-date cells and the Changelog were added**, and "No licence calls were changed". No rows were re-checked because of that edit.
- Re-stat before writing: **2026-11-17 08:56:07 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **26** | I opened the licence text myself (live page, GitHub raw file, or a Wayback capture of the dataset's own page), and it matches H5's quote and its commercial call |
| **FIX** | **3** | D6 Human3.6M (wrong URL and "Not read"), D8 HDM05 (own page *is* readable through archive.org), D10 Berkeley MHAD (licence is BSD-2, not unknown) |
| **UNVERIFIABLE** | **0** | |
| **Total** | **29** | §1 main table 15 (D1–D14 + D2b); §2 AMASS subsets 14 |

**Worst problems**
1. **D10 Berkeley MHAD is BSD-2, which is a permissive licence.** H5 marks it ⚠️ unknown, "Not read". The dataset's own page has a **License** section, read through the Wayback capture of 2021-01-18: *"The database and accompanying code are released under BSD-2 license: Copyright (c) 2013, Regents of the University of California. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice… 2. Redistributions in binary form must reproduce the above copyright notice…"*
   - **Correction:** Commercial use ✅. Models, clips and parameters ✅, provided the copyright notice and disclaimer are kept.
   - Move MHAD from "Unknown, so ask the owners" in §3 to "usable with notice".
   - The live page is still down (no connection or 500).
   - Grok B reported that a 2016 capture has no licence text. That does not contradict the 2021 capture, which has it.
2. **D6 Human3.6M: H5's URL is wrong.** `vision.imar.ro/human36m/` does not work. The EULA lives at **https://vision.imar.ro/human3.6m/eula.php**, which returned 200 and which I read: *"1. GRANT OF LICENSE FREE OF CHARGE FOR ACADEMIC USE ONLY. Licenses free of charge are limited to academic use only. Provided you send the request from an academic address…"*. The licence call (❌ commercial) is right. Change "Not read: secondary only" to read on the own page.
3. **D8 HDM05: the own page *is* readable through archive.org.** Wayback capture 2023-12-11 (`/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/`) says: *"Motion Capture Database HDM05 by Motion Capture Database HDM05 is licensed under a Creative Commons Attribution-ShareAlike 3.0 Unported License."* The page also says the database aims to "supply free motion capture data for research purposes". H5's call (CC BY-SA, ✅ with share-alike) is **right**. Only "Own page not read" and the matching "Not found" bullet need changing. The live page still returns 403.
4. **Mixamo currency, not a FIX.** I could not open Adobe's current terms index (403 from the box). Grok B reports that the Adobe General Terms index (effective 2025-10-03) does not list a newer Mixamo Additional Terms PDF, and that the 2021-06-23 PDF is still hosted. I confirmed that the 2021 PDF is still live (200). H5's "Not found" note on currency stands.

**Dead links:** `http://vision.imar.ro/human36m/` and `…/human36m/eula.php` (wrong path; use `/human3.6m/`). `tele-immersion.citris-uc.org/berkeley_mhad` (live page down; Wayback works). `resources.mpi-inf.mpg.de/HDM05/` (403; Wayback works). `mocap.cs.cmu.edu` returns 500 to the box (WebFetch or Wayback works). AMASS `eyes_japan.html` and `eyesjapan.html` return 404, as H5 says.

## Exact licence clauses (the 11 the brief asked for)

| Dataset | Exact clause (quoted from the page I opened) | Where | URL |
|---|---|---|---|
| CMU | "This dataset of motions is free for all uses." … "You may include this data in commercially-sold products, but you may not resell this data directly, even in converted form." FAQ: "The motion capture data may be copied, modified, or redistributed without permission." | Home (Wayback 2025-01-07) and FAQ | http://mocap.cs.cmu.edu/ ; https://web.archive.org/web/20250107/http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php |
| 100STYLE | "This work is licensed under a Creative Commons Attribution 4.0 International License." | Project page | https://www.ianxmason.com/100style/ |
| AMASS | "License for non-commercial scientific research purposes" … "Any other use, in particular any use for commercial purposes, is prohibited. This includes, without limitation, incorporation in a commercial product, use in a commercial service, or production of other artefacts for commercial purposes." … "This license also prohibits the use of the Dataset to train methods/algorithms/neural networks/etc. for commercial use of any kind." No sharing "to any third party without Max-Planck's prior written permission". | license.html | https://amass.is.tue.mpg.de/license.html |
| SMPL | "This software/data and the associated license is for academic research purposes. The software/data is also available for commercial licensing through Meshcapade.com." | Model licence | https://smpl.is.tue.mpg.de/modellicense.html |
| Mixamo | FAQ ("Last updated on Sep 14, 2021"): "You can use both characters and animations royalty free for personal, commercial, and non-profit projects including … Create video games." Additional Terms ("Effective as of June 23, 2021. Replaces all prior versions"), §1 Restriction on AI/ML: "You will not … use the Services or Software (or any content, data, output, or other information received or derived from the Services or Software) to directly or indirectly create, train, test, or otherwise improve any machine learning algorithms or artificial intelligence systems…" | FAQ (WebFetch) and PDF | https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html ; https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Mixamo-Addl-Terms-en_US-20210623.pdf |
| Bandai Namco | README: "Bandai-Namco-Research-Motiondataset-1: CC BY-NC 4.0"; "…-2: CC BY-NC 4.0"; "Motion visualization on Blender: MIT". Both dataset LICENSE files begin "Attribution-NonCommercial 4.0 International". | GitHub README and LICENSE | https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset |
| LaFAN1 | README: "This dataset can be used under the Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International Public License". license.txt: "produce and reproduce, but not Share, Adapted Material for NonCommercial purposes only." | GitHub | https://github.com/ubisoft/ubisoft-laforge-animation-dataset ; https://raw.githubusercontent.com/ubisoft/ubisoft-laforge-animation-dataset/master/license.txt |
| ZeroEGGS | License.md: "Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International Public License". There is no separate code licence in the README. | GitHub | https://github.com/ubisoft/ubisoft-laforge-ZeroEGGS |
| Motorica | README: "free to use for research purposes by academic institutes, companies, and individuals. Use for commercial purposes is not permitted without prior written consent. This includes … training machine-learning algorithms for commercial purposes"; "Unauthorised redistribution … prohibited". LICENSE: "The motion in this database is (c) copyright Motorica AB, with all rights reserved." | GitHub | https://github.com/simonalexanderson/MotoricaDanceDataset |
| HDM05 | "Motion Capture Database HDM05 by Motion Capture Database HDM05 is licensed under a Creative Commons Attribution-ShareAlike 3.0 Unported License." | Own page via Wayback 2023-12-11 | https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ |
| Human3.6M | "1. GRANT OF LICENSE FREE OF CHARGE FOR ACADEMIC USE ONLY. Licenses free of charge are limited to academic use only. Provided you send the request from an academic address…" | Own EULA (live) | https://vision.imar.ro/human3.6m/eula.php |
| Berkeley MHAD | "The database and accompanying code are released under BSD-2 license: Copyright (c) 2013, Regents of the University of California…" | Own page via Wayback 2021-01-18 | https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad |

## Row table

| Row | Claim (H5) | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| D1 | CMU: free for all uses; commercial ✅; no resale "even in converted form" | VERIFIED | Quotes above | http://mocap.cs.cmu.edu/ (Wayback) ; http://mocap.cs.cmu.edu/faqs.php |
| D2 | AMASS non-commercial; commercial training banned; "other artefacts" banned | VERIFIED | Quotes above | https://amass.is.tue.mpg.de/license.html |
| D2b | SMPL academic; commercial through Meshcapade | VERIFIED | | https://smpl.is.tue.mpg.de/modellicense.html |
| D3 | Mixamo: commercial ✅ in finished products; AI/ML ❌ | VERIFIED | Currency of the 2021 PDF not confirmed (Adobe pages give 403). The PDF is still live. | Mixamo FAQ ; Mixamo-Addl-Terms-en_US-20210623.pdf |
| D4 | Bandai Namco 1 and 2: CC BY-NC 4.0; Blender script MIT | VERIFIED | Current README and LICENSE. See Sources disagree for the 2022 NC-ND wording. | GitHub repo |
| D5 | LaFAN1 CC BY-NC-ND 4.0 | VERIFIED | | GitHub repo and license.txt |
| D6 | Human3.6M: academic only; "Not read"; URL http://vision.imar.ro/human36m/ | **FIX** | **URL → https://vision.imar.ro/human3.6m/eula.php (200, read).** The licence call is right. | same |
| D7 | KIT: no licence grant; "freely available to the whole scientific community"; re3data "Copyrights" | VERIFIED | FAQ text matches; KIT-ML page says "large, open, and extensible" | https://motion-database.humanoids.kit.edu/faq/ ; https://motion-annotation.humanoids.kit.edu/dataset/ ; https://www.re3data.org/api/v1/repository/r3d100012184 |
| D8 | HDM05 CC BY-SA 3.0-style; "Own page not read" | **FIX** | Call is correct. **Own page is readable through Wayback** and states CC BY-SA 3.0 Unported. | https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ |
| D9 | SFU research only | VERIFIED | "free for research purposes. The data cannot be used for commercial products or resale" | https://mocap.cs.sfu.ca/ |
| D10 | MHAD ⚠️ unknown; "BSD-2" unverified; Not read | **FIX** | **BSD-2 confirmed on the own page (Wayback 2021-01-18). Commercial ✅ with the notice and disclaimer.** | https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad |
| D11 | Motorica: all rights reserved; research only; no commercial ML | VERIFIED | | GitHub repo |
| D12 | ZeroEGGS CC BY-NC-ND 4.0 | VERIFIED | | GitHub repo |
| D13 | 100STYLE CC BY 4.0 | VERIFIED | | https://www.ianxmason.com/100style/ |
| D14 | HumanML3D: MIT for code and annotations; motions follow AMASS | VERIFIED | GitHub API licence MIT; motions are rebuilt from AMASS and HumanAct12 | https://github.com/EricGuo5513/HumanML3D |
| S-ACCAD | CC "CCPL" text with no NC/SA/ND | VERIFIED | | https://download.is.tue.mpg.de/amass/licences/accad.html |
| S-BMLmovi | Non-commercial; "…3D models, pictures, movies, or video games" | VERIFIED | | …/bmlmovi.html |
| S-BMLrub | Same as BMLmovi; Troje | VERIFIED | | …/bmlrub.html |
| S-CMU | Acknowledgement only | VERIFIED | | …/cmu.html |
| S-DFaust | MPI non-commercial | VERIFIED | | …/dfaust.html |
| S-EKUT/KIT/WEIZMANN | KIT citation text only | VERIFIED | | …/ekut.html ; …/kit.html ; …/weizmann.html |
| S-HDM05 | CC BY-SA 3.0-style | VERIFIED | Matches the own page (D8) | …/hdm05.html |
| S-DanceDB | CC BY-SA 3.0-style | VERIFIED | The text is the CC BY-SA 3.0 (US-ported) legal code | …/dancedb.html |
| S-HumanEva/CNRS | Only the word "License" | VERIFIED | | …/humaneva.html ; …/cnrs.html |
| S-MPI subsets (MoSh, PosePrior, SSM, Transitions, GRAB, SOMA, MoYo) | MPI non-commercial | VERIFIED | All seven pages read | …/mosh.html etc. |
| S-SFU | Acknowledgement only | VERIFIED | | …/sfu.html |
| S-TCDHands | "licensed from … Trinity College Dublin" | VERIFIED | | …/tcdhands.html |
| S-TotalCapture | Acknowledgement only; source research-only, no redistribution | VERIFIED | cvssp: "free for research use only… should not be used for commercial purposes. The data should not be redistributed." | …/totalcapture.html ; https://cvssp.org/data/totalcapture/ |
| S-EyesJapan | AMASS page not found | VERIFIED | eyesjapan.html and eyes_japan.html both return 404 | https://download.is.tue.mpg.de/amass/licences/eyes_japan.html |

(…/ = https://download.is.tue.mpg.de/amass/licences/)

## Not found
- Mixamo: no proof that the 2021-06-23 Additional Terms are still the current version. Adobe pages are 403 from the box. Grok B found no newer Mixamo PDF on the terms index; that is unconfirmed by me.
- Live pages for HDM05 (403), MHAD (down) and CMU (500 from the box) were not reachable live; archive.org captures were used.
- No dataset licence says anything explicit about shipping **summary statistics** (H5's category c). This agrees with H5.

## Sources disagree
- **Bandai Namco licence history:** the current README and LICENSE say **CC BY-NC 4.0**. Grok B reports that a 2022 AUTOMATON article and GitHub issue #6 (May 2022) gave **CC BY-NC-ND 4.0**, which suggests the licence was relaxed later. I did not open those two pages myself. Use the current files, and record the date checked.
- **Berkeley MHAD:** the Wayback capture from 2021 has the BSD-2 licence section. Grok B says a 2016 capture showed none. The licence section may have been added after 2016. The 2021 text is the latest available.
- **HDM05 purpose vs licence:** the page says it aims to "supply free motion capture data for research purposes", but the licence it states is CC BY-SA 3.0, which permits commercial use with share-alike. The licence text governs.
- H5's existing disagreements (CMU free vs no resale; Mixamo FAQ vs AI/ML terms; AMASS vs source subsets; KIT; Motorica) were re-read and are accurate.

## Source table

| Key | URL | Resolves (2026-11-17) | Licence correct in H5 |
|---|---|---|---|
| U1/U2 CMU | http://mocap.cs.cmu.edu/ ; /faqs.php | y (box 500; WebFetch/Wayback y) | y |
| U3/U4 AMASS | https://amass.is.tue.mpg.de/license.html | y | y |
| U5 AMASS subsets | https://download.is.tue.mpg.de/amass/licences/*.html | y (Eyes Japan 404) | y |
| U6 SMPL | https://smpl.is.tue.mpg.de/modellicense.html | y | y |
| U7 Mixamo FAQ | https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html | y (WebFetch; box 403) | y |
| U8 Mixamo terms | …/Mixamo-Addl-Terms-en_US-20210623.pdf | y | y (currency unconfirmed) |
| U9 Adobe community post | https://community.adobe.com/questions-696/mixamo-faq-licensing-royalties-ownership-eula-and-tos-589400 | not re-opened | n/a |
| U10 Bandai Namco | GitHub repo | y | y |
| U11 LaFAN1 | GitHub repo | y | y |
| U12 Human3.6M | http://vision.imar.ro/human36m/ | **n (wrong path)** → https://vision.imar.ro/human3.6m/eula.php y | y (call), URL FIX |
| U14/U15/U16 KIT | FAQ, KIT-ML, re3data | y | y |
| U17 HDM05 | https://resources.mpi-inf.mpg.de/HDM05/ | n live (403); y via Wayback 20231211133855 | y |
| U19 HDM05 re3data | https://www.re3data.org/repository/r3d100011968 | y | y ("CC") |
| U20 SFU | https://mocap.cs.sfu.ca/ | y | y |
| U21 MHAD | https://tele-immersion.citris-uc.org/berkeley_mhad | n live; y via Wayback 20210118014158 | **n**: H5 says unknown; the page says BSD-2 |
| U23 Motorica | GitHub repo | y | y |
| U24 ZeroEGGS | GitHub repo | y | y |
| U25 100STYLE | https://www.ianxmason.com/100style/ | y | y |
| U26 HumanML3D | GitHub repo | y | y |
| U27 TotalCapture | https://cvssp.org/data/totalcapture/ | y | y |

## Grok pass
- **"Grok B"** (H5/H2/H4): `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file …`, run **2026-11-17 08:49:22–09:08:49 AEDT**, exit 0. Output is in `/workspace/tmp/hverify2/grokB/out.txt`.
- **It agreed on** CMU, 100STYLE, AMASS/SMPL, Mixamo (with no newer PDF found), Bandai Namco (current), LaFAN1, ZeroEGGS, Motorica, HDM05 (Wayback 2020), Human3.6M, SFU, KIT and TotalCapture.
- **Conflicts it raised:**
  - Bandai Namco's 2022 licence was NC-ND (not opened by me; recorded above).
  - MHAD 2016 Wayback has no licence. That is superseded by my reading of the 2021 capture, which does.
- Every licence clause above was opened by me. None is taken from Grok alone.

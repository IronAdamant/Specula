# H5: Motion-capture dataset licences

**Changelog**
- 2026-11-17 09:23 AEDT (second Library Verify pass, verify file 09:16; plus my Grok H5 cross-check): (1) D6/U12 Human3.6M URL corrected to https://vision.imar.ro/human3.6m/eula.php and the EULA read today (academic use only; call unchanged). (2) D8/U17 HDM05 own page read via Wayback 2023-12-11: CC BY-SA 3.0 Unported (call unchanged). (3) D10/U21 Berkeley MHAD: own page read via Wayback 2021-01-18 — **BSD-2** (2013 UC Regents); calls changed ⚠️ → ✅ with notice and disclaimer; moved out of "ask the owners" and into the usable list. The three matching Not-found bullets were removed. The Grok cross-check found no other conflict with H5's licence calls.
- 2026-11-17 (AEDT): per-row access dates added. Every table row now ends with a "Source URL · accessed" cell (or "Accessed" in the source list), and every numeric or licence bullet carries its URL and access date. Rows checked on 2026-11-16 carry 2026-11-16. No licence calls were changed by this step.


Lane H5 of `LIBRARY_RESEARCH_BRIEF.md`. Written 2026-11-16 (AEDT). All URLs were accessed on 2026-11-16.
Licence wording is quoted briefly from each dataset's own page (short quotes, for accuracy). Where the dataset's own page could not be opened from the box or through the fetch tool, this is said plainly.
**This is not legal advice.** "Derived results" are split three ways:
- **(a) trained models**
- **(b) retargeted or edited clips shipped as animation**
- **(c) extracted parameters**: summary numbers such as stride length ranges or joint-angle min/max, shipped inside a generator

## 1. Main table

Key:
- ✅ allowed
- ❌ not allowed
- ⚠️ unclear or conditional
- "own page" means the licence text was read on the dataset's own or official page on 2026-11-16

| ID | Dataset | Licence, as worded on its page | Commercial use | (a) Ship trained model | (b) Ship retargeted clips | (c) Ship extracted parameters | Page read? | Source URL · accessed |
|---|---|---|---|---|---|---|---|---|
| D1 | **CMU Graphics Lab Motion Capture Database** | Home: "This dataset of motions is free for all uses." … "You may include this data in commercially-sold products, but you may not resell this data directly, even in converted form." FAQ: "The motion capture data may be copied, modified, or redistributed without permission." | ✅ | ✅ (not forbidden) | ✅ inside a product. ❌ resold as a data pack, "even in converted form" | ✅ | Own page via the fetch tool (the box got 500) | http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php · 2026-11-16 |
| D2 | **AMASS** (the combined archive) | "License for non-commercial scientific research purposes" … "Any other use, in particular any use for commercial purposes, is prohibited. This includes … incorporation in a commercial product …" … "This license also prohibits the use of the Dataset to train methods/algorithms/neural networks/etc. for commercial use of any kind." … "may not be reproduced, modified and/or made available in any form to any third party". Commercial licensing: ps-license@tue.mpg.de. | ❌ (separate commercial licence on request) | ❌ for commercial use (stated explicitly) | ❌ | ⚠️ "production of other artefacts for commercial purposes" is also banned. A generator shipping AMASS-derived numbers is likely covered, so treat it as ❌ | Own page | https://amass.is.tue.mpg.de/license.html · 2026-11-16 |
| D2b | **SMPL body model** (AMASS motions are stored as SMPL parameters) | "This software/data and the associated license is for academic research purposes. The software/data is also available for commercial licensing through Meshcapade.com." | ❌ without a paid licence | ❌ | ❌ | ⚠️ | Own page | https://smpl.is.tue.mpg.de/modellicense.html · 2026-11-16 |
| D3 | **Mixamo** (Adobe) | FAQ: "You can use both characters and animations royalty free for personal, commercial, and non-profit projects including … Create video games." Mixamo Additional Terms (effective 23 Jun 2021), §1 "Restriction on AI/ML": you will not use "any content, data, output, or other information received or derived from the Services … to directly or indirectly create, train, test, or otherwise improve any machine learning algorithms or artificial intelligence systems". Adobe staff community FAQ: you cannot distribute the "raw character and animation files", for example asset packs or templates. | ✅ inside a finished game or film | ❌ (AI/ML clause) | ✅ inside a finished product. ❌ as raw files or packs | ⚠️ Probably ❌ if the parameters feed any learned system. For a hand-written rule generator the terms say nothing explicit, so ask Adobe | FAQ + PDF read. The FAQ page needed the fetch tool (403 from the box) | https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html ; https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Mixamo-Addl-Terms-en_US-20210623.pdf · 2026-11-16 |
| D4 | **Bandai Namco Research Motiondataset** (1 and 2) | README: "Bandai-Namco-Research-Motiondataset-1: CC BY-NC 4.0"; "…-2: CC BY-NC 4.0"; Blender script: MIT. The LICENSE file in dataset-1 begins "Attribution-NonCommercial 4.0 International". | ❌ | ❌ for commercial | ❌ commercial (adapted material may be shared non-commercially with credit) | ⚠️ NC applies to adapted material, so ❌ commercially | Own GitHub page | https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset · 2026-11-16 |
| D5 | **LaFAN1** (Ubisoft La Forge Animation Dataset) | README: "This dataset can be used under the Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International Public License". license.txt: "produce and reproduce, but not Share, Adapted Material for NonCommercial purposes only." | ❌ | ❌ | ❌ (no sharing of adapted material at all) | ❌ commercially | Own GitHub page | https://github.com/ubisoft/ubisoft-laforge-animation-dataset · 2026-11-16 |
| D6 | **Human3.6M** | Own EULA page (read 2026-11-17): "1. GRANT OF LICENSE FREE OF CHARGE FOR ACADEMIC USE ONLY … Licenses free of charge are limited to academic use only." The licensee "may not rent, lease, lend, sub-license or transfer the dataset". §6: commercial licensing by contacting the authors. | ❌ (free grant is academic only) | ❌ | ❌ | ❌ | Own page | https://vision.imar.ro/human3.6m/eula.php · 2026-11-17 |
| D7 | **KIT Whole-Body Human Motion Database** (and KIT Motion-Language) | FAQ: registration needed; "we aim to make content freely available to the whole scientific community". AMASS's KIT licence page asks only for citation. re3data lists the data licence as "Copyrights". The Motion-Language page calls itself "large, open, and extensible". **No named licence was found.** | ⚠️ no grant was found. Treat as all rights reserved | ⚠️ | ⚠️ | ⚠️ | Own pages | https://motion-database.humanoids.kit.edu/faq/ ; https://motion-annotation.humanoids.kit.edu/dataset/ · 2026-11-16 |
| D8 | **HDM05** | Own page, read via the Wayback capture of 2023-12-11 (the live page still returns 403): "Motion Capture Database HDM05 … is licensed under a Creative Commons Attribution-ShareAlike 3.0 Unported License." The AMASS mirror licence page carries matching CC ShareAlike legal text. | ✅ with credit and share-alike | ⚠️ whether a model is "adapted material" is unsettled. If it is, share-alike applies | ✅ with credit, **but the clips must be released under CC BY-SA** | ✅ facts with credit. ⚠️ share-alike if they count as adapted | Own page (Wayback) | https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ ; https://download.is.tue.mpg.de/amass/licences/hdm05.html · 2026-11-17 |
| D9 | **SFU Motion Capture Database** | "This dataset of motions is free for research purposes. The data cannot be used for commercial products or resale, unfortunately." | ❌ | ❌ for commercial | ❌ | ❌ | Own page | https://mocap.cs.sfu.ca/ · 2026-11-16 |
| D10 | **Berkeley MHAD** | Own page, License section, read via the Wayback capture of 2021-01-18 (live page gives 500): "The database and accompanying code are released under BSD-2 license: Copyright (c) 2013, Regents of the University of California" … "Redistribution and use in source and binary forms, with or without modification, are permitted provided that" the copyright notice, conditions and disclaimer are retained/reproduced. | ✅ (keep the copyright notice and disclaimer) | ✅ (not forbidden; keep notice) | ✅ with notice and disclaimer | ✅ | Own page (Wayback) | https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad · 2026-11-17 |
| D11 | **Motorica Dance Dataset** (KTH/Motorica) | LICENSE.txt: "The motion in this database is (c) copyright Motorica AB, with all rights reserved." README: "free to use for research purposes by academic institutes, companies, and individuals. Use for commercial purposes is not permitted without prior written consent. This includes … training machine-learning algorithms for commercial purposes." "Unauthorised redistribution … is prohibited". | ❌ | ❌ commercial | ❌ | ❌ | Own GitHub page | https://github.com/simonalexanderson/MotoricaDanceDataset · 2026-11-16 |
| D12 | **ZeroEGGS** (Ubisoft speech-gesture mocap) | License.md: "Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International Public License" | ❌ | ❌ | ❌ | ❌ commercially | Own GitHub page | https://github.com/ubisoft/ubisoft-laforge-ZeroEGGS · 2026-11-16 |
| D13 | **100STYLE** (Mason et al. 2022) | "This work is licensed under a Creative Commons Attribution 4.0 International License." "For creative or commercial work please credit in an appropriate way. E.g. 'The 100STYLE Dataset - Ian Mason'." | ✅ with credit | ✅ with credit | ✅ with credit | ✅ with credit | Own page | https://www.ianxmason.com/100style/ · 2026-11-16 |
| D14 | **HumanML3D** | The repo licence is MIT (GitHub API), but that covers the code and text annotations only. The motions are rebuilt from AMASS and HumanAct12, so D2's terms apply to the motion. | ❌ for motion (via AMASS) | ❌ | ❌ | ❌ | Own GitHub page | https://amass.is.tue.mpg.de/license.html ; https://github.com/EricGuo5513/HumanML3D · 2026-11-16 |

## 2. AMASS component datasets: licence pages hosted by AMASS

AMASS hosts one licence page per subset at `https://download.is.tue.mpg.de/amass/licences/<name>.html`. All were read on 2026-11-16. **The combined AMASS licence (D2) still applies to anything downloaded from AMASS**, even where the original subset is more open. To use an open subset commercially, get it from its original source, not from AMASS. (accessed 2026-11-16)

| Subset | What the AMASS-hosted page says | Original-source licence (if checked) | Commercial via the original source? | Source URL · accessed |
|---|---|---|---|---|
| ACCAD | Creative Commons "CCPL" legal text, with no NonCommercial, ShareAlike or NoDerivs terms. That fits CC BY (3.0-era text; the version number is not printed in the extracted text) | not re-checked at ACCAD | ✅ probably, with credit (verify on ACCAD's site) | https://download.is.tue.mpg.de/amass/licences/accad.html · 2026-11-16 |
| BMLmovi | "Data License For non-commercial and scientific research purposes" … "use for commercial purposes, is prohibited … including, for example, 3D models, pictures, movies, or video games" | – | ❌ | https://download.is.tue.mpg.de/amass/licences/bmlmovi.html · 2026-11-16 |
| BMLrub | Same wording as BMLmovi (non-commercial). Data owned by N. Troje | – | ❌ | https://download.is.tue.mpg.de/amass/licences/bmlrub.html · 2026-11-16 |
| CMU | Acknowledgement text only | D1: free for all uses, no resale | ✅ (from CMU directly) | https://download.is.tue.mpg.de/amass/licences/cmu.html ; http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php · 2026-11-16 |
| DFaust | MPI "non-commercial scientific research" licence | – | ❌ | https://download.is.tue.mpg.de/amass/licences/dfaust.html · 2026-11-16 |
| EKUT, KIT, WEIZMANN | KIT citation text only (no licence grant) | D7: no named licence | ⚠️ | https://download.is.tue.mpg.de/amass/licences/ekut.html ; https://download.is.tue.mpg.de/amass/licences/kit.html ; https://download.is.tue.mpg.de/amass/licences/weizmann.html ; https://motion-database.humanoids.kit.edu/faq/ ; https://motion-annotation.humanoids.kit.edu/dataset/ · 2026-11-16 |
| HDM05 | CC BY-SA (3.0-style text) | D8 | ✅ with share-alike | https://download.is.tue.mpg.de/amass/licences/hdm05.html ; https://resources.mpi-inf.mpg.de/HDM05/ · 2026-11-16 |
| DanceDB | CC BY-SA (3.0-style text) | not checked | ✅ with share-alike (verify) | https://download.is.tue.mpg.de/amass/licences/dancedb.html · 2026-11-16 |
| HumanEva, CNRS | Page shows only the word "License" (empty) | not checked | ⚠️ | https://download.is.tue.mpg.de/amass/licences/humaneva.html ; https://download.is.tue.mpg.de/amass/licences/cnrs.html · 2026-11-16 |
| MoSh, PosePrior, SSM, Transitions, GRAB, SOMA, MoYo | MPI "non-commercial scientific research purposes" licences | – | ❌ | https://download.is.tue.mpg.de/amass/licences/mosh.html ; https://download.is.tue.mpg.de/amass/licences/poseprior.html ; https://download.is.tue.mpg.de/amass/licences/ssm.html ; https://download.is.tue.mpg.de/amass/licences/transitions.html ; https://download.is.tue.mpg.de/amass/licences/grab.html · 2026-11-16 |
| SFU | Acknowledgement text only | D9: research only | ❌ | https://download.is.tue.mpg.de/amass/licences/sfu.html ; https://mocap.cs.sfu.ca/ · 2026-11-16 |
| TCDHands | "licensed from the Graphics, Vision and Visualisation Group, Trinity College Dublin" | not checked | ⚠️ | https://download.is.tue.mpg.de/amass/licences/tcdhands.html · 2026-11-16 |
| TotalCapture | Acknowledgement text only | cvssp.org: "free for research use only … should not be used for commercial purposes. The data should not be redistributed." | ❌ | https://download.is.tue.mpg.de/amass/licences/totalcapture.html ; https://cvssp.org/data/totalcapture/ · 2026-11-16 |
| Eyes Japan | AMASS page not found at the guessed URL. The mocapdata.com terms page returned 500 | not checked | ⚠️ | https://amass.is.tue.mpg.de/ (AMASS-hosted Eyes Japan licence page; exact page URL not recorded) · 2026-11-16 |

## 3. What can actually ship commercially (summary)

- **Usable with credit for a commercial seeded generator:** CMU (D1), 100STYLE (D13), Berkeley MHAD (D10, BSD-2: keep the 2013 UC Regents copyright notice and disclaimer). [http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php ; https://www.ianxmason.com/100style/ ; https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad · accessed 2026-11-16 / 2026-11-17 (MHAD)]
  - CMU allows commercial products but bans reselling the data "even in converted form". Do not ship CMU clips as a standalone asset pack. [http://mocap.cs.cmu.edu/ · accessed 2026-11-16]
- **Share-alike:** HDM05 (D8) and DanceDB. Clips derived from them must be released under CC BY-SA. Summary numbers taken as facts are lower risk. [https://resources.mpi-inf.mpg.de/HDM05/ ; https://download.is.tue.mpg.de/amass/licences/hdm05.html · accessed 2026-11-16]
- **Commercial use allowed but no ML:** Mixamo (D3). Fine inside a finished game. No machine-learning training, and no raw-file redistribution. [https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html ; https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Mixamo-Addl-Terms-en_US-20210623.pdf · accessed 2026-11-16]
- **Research or non-commercial only:**
  - Not usable for a shipped product: AMASS and the SMPL body model, BMLmovi and BMLrub, the MPI subsets, Bandai Namco (CC BY-NC), LaFAN1 and ZeroEGGS (CC BY-NC-ND), Human3.6M, SFU, TotalCapture, Motorica. [https://smpl.is.tue.mpg.de/modellicense.html ; https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset ; https://github.com/ubisoft/ubisoft-laforge-animation-dataset ; https://vision.imar.ro/human3.6m/eula.php ; https://mocap.cs.sfu.ca/ ; https://github.com/simonalexanderson/MotoricaDanceDataset ; https://github.com/ubisoft/ubisoft-laforge-ZeroEGGS ; https://cvssp.org/data/totalcapture/ · accessed 2026-11-16]
  - HumanML3D motions inherit AMASS terms. [https://github.com/EricGuo5513/HumanML3D · accessed 2026-11-16]
- **Unknown, so ask the owners:** KIT, HumanEva/CNRS, TCD Hands, Eyes Japan. [https://motion-database.humanoids.kit.edu/faq/ ; https://download.is.tue.mpg.de/amass/licences/humaneva.html ; https://download.is.tue.mpg.de/amass/licences/cnrs.html ; https://download.is.tue.mpg.de/amass/licences/tcdhands.html · accessed 2026-11-16]

## 4. Full source list (all accessed 2026-11-16)

| ID | Page | URL | Status 2026-11-16 | Accessed |
|---|---|---|---|---|
| U1 | CMU mocap home | http://mocap.cs.cmu.edu/ | Box 500. Read via the fetch tool | 2026-11-16 |
| U2 | CMU mocap FAQ | http://mocap.cs.cmu.edu/faqs.php | Box 500. Read via the fetch tool | 2026-11-16 |
| U3 | AMASS licence | https://amass.is.tue.mpg.de/license.html | 200, read | 2026-11-16 |
| U4 | AMASS home | https://amass.is.tue.mpg.de/ | 200 | 2026-11-16 |
| U5 | AMASS subset licence pages | https://download.is.tue.mpg.de/amass/licences/accad.html (and bmlmovi, bmlrub, cmu, dfaust, ekut, hdm05, humaneva, kit, mosh, poseprior, sfu, ssm, tcdhands, totalcapture, transitions, grab, soma, dancedb, cnrs, weizmann, moyo `.html`) | All 200. eyesjapan/eyes_japan gave 404 | 2026-11-16 |
| U6 | SMPL model licence | https://smpl.is.tue.mpg.de/modellicense.html | 200, read | 2026-11-16 |
| U7 | Mixamo FAQ | https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html | Box 403. Read via the fetch tool ("Last updated on Sep 14, 2021") | 2026-11-16 |
| U8 | Mixamo Additional Terms (PDF, effective 2021-06-23) | https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Mixamo-Addl-Terms-en_US-20210623.pdf | 200, read | 2026-11-16 |
| U9 | Adobe Community Mixamo licensing FAQ (Adobe staff post) | https://community.adobe.com/questions-696/mixamo-faq-licensing-royalties-ownership-eula-and-tos-589400 | 200 (content seen via search) | 2026-11-16 |
| U10 | Bandai Namco Research Motiondataset | https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset ; licence: https://raw.githubusercontent.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset/master/dataset/Bandai-Namco-Research-Motiondataset-1/LICENSE | 200, read | 2026-11-16 |
| U11 | LaFAN1 | https://github.com/ubisoft/ubisoft-laforge-animation-dataset ; https://raw.githubusercontent.com/ubisoft/ubisoft-laforge-animation-dataset/master/license.txt | 200, read | 2026-11-16 |
| U12 | Human3.6M | https://vision.imar.ro/human3.6m/eula.php (the earlier path http://vision.imar.ro/human36m/ was wrong) | 200, EULA read | 2026-11-17 |
| U13 | Human3.6M secondary | https://computervisiononline.com/explains/3d-human-pose-estimation/ | via search only | 2026-11-16 |
| U14 | KIT database FAQ | https://motion-database.humanoids.kit.edu/faq/ | 200, read | 2026-11-16 |
| U15 | KIT Motion-Language | https://motion-annotation.humanoids.kit.edu/dataset/ | 200, read | 2026-11-16 |
| U16 | KIT in re3data | https://www.re3data.org/api/v1/repository/r3d100012184 | 200 ("Copyrights") | 2026-11-16 |
| U17 | HDM05 | https://resources.mpi-inf.mpg.de/HDM05/ (live: 403) ; Wayback: https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ | Wayback 200, licence line read | 2026-11-17 |
| U18 | HDM05 secondary (VISGRAF tech report 01/2014) | https://www.visgraf.impa.br/Data/RefBib/PS_PDF/tr01-2014/tr-01-2014.pdf | via search | 2026-11-16 |
| U19 | HDM05 in re3data | https://www.re3data.org/repository/r3d100011968 | 200 ("Data license: CC") | 2026-11-16 |
| U20 | SFU mocap | https://mocap.cs.sfu.ca/ | 200, read | 2026-11-16 |
| U21 | Berkeley MHAD | https://tele-immersion.citris-uc.org/berkeley_mhad (live: 500) ; Wayback: https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad | Wayback 200, License section read | 2026-11-17 |
| U22 | MHAD secondary (*Sci Data* 2022 table) | https://www.nature.com/articles/s41597-022-01841-1/tables/3 | via search | 2026-11-16 |
| U23 | Motorica Dance Dataset | https://github.com/simonalexanderson/MotoricaDanceDataset | 200, README and LICENSE.txt read | 2026-11-16 |
| U24 | ZeroEGGS | https://github.com/ubisoft/ubisoft-laforge-ZeroEGGS | 200, License.md read | 2026-11-16 |
| U25 | 100STYLE | https://www.ianxmason.com/100style/ | 200, read | 2026-11-16 |
| U26 | HumanML3D | https://github.com/EricGuo5513/HumanML3D | 200 (repo licence MIT via the GitHub API) | 2026-11-16 |
| U27 | TotalCapture | https://cvssp.org/data/totalcapture/ | 200, read | 2026-11-16 |
| L1 | CC BY-NC-ND 4.0 | https://creativecommons.org/licenses/by-nc-nd/4.0/ | 200 | 2026-11-16 |
| L2 | CC BY-NC 4.0 | https://creativecommons.org/licenses/by-nc/4.0/ | 200 | 2026-11-16 |
| L3 | CC BY-SA 3.0 | https://creativecommons.org/licenses/by-sa/3.0/ | 200 | 2026-11-16 |
| L4 | CC BY 3.0 | https://creativecommons.org/licenses/by/3.0/ | 200 | 2026-11-16 |

## Not found
- **KIT:** no explicit licence. Only citation requests and "freely available to the scientific community".
- **Eyes Japan, HumanEva, CNRS, TCD Hands:** no usable licence text found.
- **ACCAD:** the CC version was not confirmed (only the 3.0-style legal text on the AMASS mirror). ACCAD's own site was not checked.
- **Mixamo:** I did not confirm whether the Additional Terms PDF dated 2021-06-23 is still the current version on Adobe's terms index.
- **No dataset says anything explicit** about shipping *summary statistics* (category c). The judgements in column (c) are my reading, not the owners' words.

## Sources disagree
- **CMU:** "free for all uses" and "may be copied, modified, or redistributed without permission" (FAQ) versus "you may not resell this data directly, even in converted form" (home page). Read together: free use and redistribution, but no selling the data itself. [http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php · accessed 2026-11-16]
- **Mixamo:** the FAQ lists commercial games as allowed with no mention of ML. The 2021 Additional Terms ban any ML or AI use of Mixamo content or anything "derived" from it. The terms are the binding text. [https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html ; https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Mixamo-Addl-Terms-en_US-20210623.pdf · accessed 2026-11-16]
- **AMASS versus its subsets:** CMU (free for all uses), ACCAD (CC BY) and HDM05 (CC BY-SA) are open at source. The AMASS repackaging of the same motions is non-commercial with no third-party redistribution. Which applies depends on where you downloaded from. [http://mocap.cs.cmu.edu/ ; http://mocap.cs.cmu.edu/faqs.php ; https://resources.mpi-inf.mpg.de/HDM05/ ; https://download.is.tue.mpg.de/amass/licences/hdm05.html · accessed 2026-11-16]
- **KIT:** re3data says "Copyrights" (all rights reserved). The KIT Motion-Language page says it is "open". There is no licence text to settle it. [https://motion-database.humanoids.kit.edu/faq/ ; https://motion-annotation.humanoids.kit.edu/dataset/ · accessed 2026-11-16]
- **Motorica:** "free to use for research purposes by … companies" versus "all rights reserved" and no commercial use. Companies may do research but not ship. [https://github.com/simonalexanderson/MotoricaDanceDataset · accessed 2026-11-16]

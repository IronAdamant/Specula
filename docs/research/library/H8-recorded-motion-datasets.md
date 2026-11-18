# Recorded motion and gait datasets: links only

Second check (2026-11-18): clean (12 of 12 datasets verified; the checker calls this file `motion-datasets.md`). Small additions:
- A caution on HDM05: its page frames the database as "for research purposes" beside its CC BY-SA 3.0 notice. Have the licence looked at before any HDM05 clip ships.
- A note that the CMU site is flaky (it returned an error to one fetch and loaded through another), not dead.
- Added a Sources list. The checker's summary is in `H8-recorded-motion-datasets.md.verify.md`.

Checked 2026-11-17. Nothing here was downloaded. Each licence is quoted in the dataset's own words from its own page (or a Wayback capture of it where the live page fails). Fuller licence analysis for more datasets is in the existing library file H5 (motion-capture licences).

Actions key: walk, run, jump, sit, climb, carry (the brief's list). "yes" means the dataset's own page lists it; "no" means not listed there.

## Usable in a commercial product (with conditions)

| Dataset | Licence, in its own words | Format | walk | run | jump | sit | climb | carry | Size and people | Link (checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|---|
| **100STYLE** (Mason, Starke, Komura 2022) | "This work is licensed under a Creative Commons Attribution 4.0 International License." "For creative or commercial work please credit in an appropriate way. E.g. 'The 100STYLE Dataset - Ian Mason'." | BVH, 60 fps, Xsens, 28-bone skeleton; FBX retarget by Daniel Holden linked | yes (forwards, backwards, sidestep) | yes (forwards, backwards, sidestep) | as styles only (for example "TwoFootJump" style of locomotion) | no | no | as styles only (for example walking stick) | over 4 million frames, 100 locomotion styles, one actor; idles and transitions | https://www.ianxmason.com/100style/ |
| **CMU Graphics Lab Motion Capture Database** | "This dataset of motions is free for all uses." and "This data is free for use in research projects. You may include this data in commercially-sold products, but you may not resell this data directly, even in converted form." | ASF/AMC and C3D (Vicon); BVH conversions by Bruce Hahn hosted at cgspeed (third party) | yes | yes | listed under categories (playground, sports) | listed under "common behaviors" | "Interaction with Environment: playground, uneven terrain" | not listed on the home page | 2,605 trials in 6 categories and 23 subcategories. Note from the page: finger and thumb joints are not captured; toe and hand joints are noisy | http://mocap.cs.cmu.edu/ |
| **HDM05** (MPI Informatik, Bonn, HdM Stuttgart, 2005) | "Motion Capture Database HDM05 by Motion Capture Database HDM05 is licensed under a Creative Commons Attribution-ShareAlike 3.0 Unported License." | C3D and ASF/AMC; Matlab parser provided; full takes and roughly 100 classes of cut clips | yes (scene 1-1) | yes (scene 1 "Walking, Running, Jumping") | yes | yes (scene 4-1 "Chair, table, floor") | no stairs listed | yes ("Locomotion with weights", "Grabbing and depositing") | more than 3 hours, more than 70 motion classes, 5 non-professional actors. Share-alike: clips derived from it must be released under CC BY-SA | https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ (live page returns 403) |
| **Berkeley MHAD** (UC Berkeley Teleimmersion Lab, 2013) | "The database and accompanying code are released under BSD-2 license: Copyright (c) 2013, Regents of the University of California" … "Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met" (keep the notice and disclaimer) | mocap as ASCII XYZ of 43 markers at 480 Hz (PhaseSpace); also video, Kinect depth, accelerometers, audio | no | no | yes ("jumping in place, jumping jacks") | yes ("sit down, stand up") | no | no (also bending, punching, waving, clapping, throwing) | 11 actions, 7 men and 5 women aged 23 to 30 plus one elderly subject, 5 repetitions, about 82 minutes | https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad (live page gives 500) |
| **Running biomechanics dataset** (Fukuchi, Fukuchi, Duarte 2017) | The paper says the data "can be downloaded as a single compressed file that is made available under the CC-BY license (https://creativecommons.org/licenses/by/4.0/)". Read on the paper page; figshare itself returns 403 to the box | C3D (markers and forces) plus tab-separated ASCII; processed file of time-normalised 3D hip, knee and ankle angles, torques, GRF, power | no | yes (2.5, 3.5, 4.5 m/s, treadmill) | no | no | no | no | 28 regular runners. Numbers from it are in library H7 | https://peerj.com/articles/3298/ ; data https://doi.org/10.6084/m9.figshare.4543435 |

**Caution on HDM05 (second check, 2026-11-18).** The same page that carries the CC BY-SA 3.0 notice opens with "It is the objective of our motion capture database HDM05 to supply free motion capture data for research purposes." The CC BY-SA licence is the operative grant and allows commercial use with share-alike, but the research-purpose framing means the licence should be looked at (a legal review) before HDM05 clips ship in a product.

**Note on CMU (second check).** The page has both "free for all uses" and "free for use in research projects. You may include this data in commercially-sold products, but you may not resell this data directly, even in converted form"; both are quoted above. The cgspeed BVH conversions are a third party's, and their own terms were not checked. http://mocap.cs.cmu.edu/ returned HTTP 500 once at the second check and loaded on another fetch: flaky, not dead.

## Research only, or no licence found (do not ship)

| Dataset | Licence, in its own words (from H5, read 2026-11-16/17) | Why it is listed | Link |
|---|---|---|---|
| AMASS | "License for non-commercial scientific research purposes" … "Any other use, in particular any use for commercial purposes, is prohibited." | Largest archive, but no commercial use, and its licence covers subsets downloaded from it | https://amass.is.tue.mpg.de/license.html |
| SFU Motion Capture Database | "This dataset of motions is free for research purposes. The data cannot be used for commercial products or resale, unfortunately." | good walks, runs, jumps; research only | https://mocap.cs.sfu.ca/ |
| LaFAN1 (Ubisoft) | "Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International Public License" | high-quality locomotion; non-commercial | https://github.com/ubisoft/ubisoft-laforge-animation-dataset |
| Bandai Namco Research Motiondataset 1 and 2 | "CC BY-NC 4.0" | stylised everyday motion; non-commercial | https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset |
| Human3.6M | "GRANT OF LICENSE FREE OF CHARGE FOR ACADEMIC USE ONLY" | academic only | https://vision.imar.ro/human3.6m/eula.php |
| Senden et al. 2024 walking gait (OSF t72cw) | no licence on the OSF project (`node_license` empty), per H1 | the walking joint-angle curves in H1 come from it; cite values as facts, do not redistribute the files | https://osf.io/t72cw/ |
| KIT Whole-Body Human Motion Database | no named licence found (H5) | treat as all rights reserved | https://motion-database.humanoids.kit.edu/faq/ |

## Best picks for Specula

1. **100STYLE** (CC BY 4.0): walks and runs in 100 styles, BVH, credit only. Best for locomotion variety.
2. **CMU** (free for all uses, no resale): broadest action range; fine inside a product, never as a clip pack.
3. **Fukuchi 2017 running data** (CC BY 4.0): the curves a test can hold a run against.
4. **HDM05** (CC BY-SA 3.0): sit, carry and walk-with-weights clips; share-alike applies to derived clips. Its page frames the data as "for research purposes": have the licence looked at before shipping (see the caution above).
5. **Berkeley MHAD** (BSD-2): jumps and sit-stand; keep the copyright notice.

## Not found

- An open, commercially usable quadruped (dog, horse) motion-capture dataset. None was confirmed in this pass; footfall timing tables are in library C2 instead.
- A commercially usable climbing (ladder, wall) dataset.

## Sources

Usable datasets (each page checked 2026-11-17):
- 100STYLE (Mason, Starke, Komura 2022), CC BY 4.0, https://www.ianxmason.com/100style/
- CMU Graphics Lab Motion Capture Database, http://mocap.cs.cmu.edu/
- HDM05, CC BY-SA 3.0, Wayback capture https://web.archive.org/web/20231211133855/https://resources.mpi-inf.mpg.de/HDM05/ (live page https://resources.mpi-inf.mpg.de/HDM05/ returns 403)
- Berkeley MHAD, BSD-2, Wayback capture https://web.archive.org/web/20210118014158/https://tele-immersion.citris-uc.org/berkeley_mhad (live page gives 500)
- Fukuchi, Fukuchi, Duarte (2017), *PeerJ* 5:e3298, https://peerj.com/articles/3298/; data CC BY 4.0, https://doi.org/10.6084/m9.figshare.4543435

Research only or no licence found (licences as read for H5, 2026-11-16/17):
- AMASS licence, https://amass.is.tue.mpg.de/license.html
- SFU Motion Capture Database, https://mocap.cs.sfu.ca/
- LaFAN1 (Ubisoft), https://github.com/ubisoft/ubisoft-laforge-animation-dataset
- Bandai Namco Research Motiondataset, https://github.com/BandaiNamcoResearchInc/Bandai-Namco-Research-Motiondataset
- Human3.6M EULA, https://vision.imar.ro/human3.6m/eula.php
- Senden et al. 2024, OSF t72cw, https://osf.io/t72cw/
- KIT Whole-Body Human Motion Database FAQ, https://motion-database.humanoids.kit.edu/faq/

# Second check: H8-recorded-motion-datasets.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff (the checker names this file motion-datasets.md); the checker's per-row table stayed with the checker.

### Round 2: motion-datasets.md

**Rows: 12 datasets: 12 VERIFIED, 0 FIX, 0 UNVERIFIABLE.** The 5 "best picks" lines restate the rows and are covered by them.

- Every licence quote I checked is verbatim on the dataset's own page, as are the size and people figures.
- **Caution for HDM05 (not a FIX):** the same page that carries the CC BY-SA 3.0 notice opens with "It is the objective of our motion capture database HDM05 to supply free motion capture data for research purposes."
  - The CC BY-SA licence is the operative grant and allows commercial use with share-alike, but the research-purpose framing is worth a line in the file.
  - Legal Desk should look at it before HDM05 clips ship.
- **Caution for CMU:** the page has both "free for all uses" and "free for use in research projects. You may include this data in commercially-sold products, but you may not resell this data directly, even in converted form". The file quotes both correctly.
  - The cgspeed BVH conversions are a third party's, and their own terms weren't checked. The file already marks them as third party.
  - Live status: http://mocap.cs.cmu.edu/ returned HTTP 500 to my computer's curl at check time but loaded through a web fetch. It's flaky, not dead.
- HDM05's live page still returns 403, so the 2023-12-11 Wayback capture was used, as the file says.
- Berkeley MHAD: the 2021-01-18 Wayback capture confirms these phrases verbatim:
  - "11 actions performed by 7 male and 5 female subjects in the range 23-30 years of age except for one elderly subject", "5 repetitions" and "about 82 minutes";
  - "43 LED markers", "480 Hz" and PhaseSpace Impulse;
  - the BSD-2 text.
  - The "ASCII XYZ" file format is on the omitted Collected Data section, which I didn't re-read.
- 100STYLE matches: "over 4 million frames", "100 different styles of locomotion", XSens at 60 fps, "28 bone skeleton", "one actor", the Daniel Holden FBX/BVH retarget, the TwoFootJump and WalkingStickLeft/Right styles, and the CC BY 4.0 wording and credit line.
- **Fukuchi 2017 CC-BY:** verified earlier this session from the paper's full text (PMC5426356; see the H7 verify). figshare and PeerJ return 403 to curl, as the file says.
- **Research-only rows:**
  - AMASS, SFU, LaFAN1 (license.txt), Bandai Namco (README links CC BY-NC 4.0 for datasets 1 and 2) and Human3.6M (EULA §1) are all verbatim.
  - The OSF t72cw API returns `node_license: null` (public project, "Normative 3D gait data of healthy subjects walking at three different speeds…"), as the file says.
  - The KIT FAQ names only the GNU GPL for the MMM *software*, with no data licence, so treating the data as all rights reserved is correct.
- The "not found" gaps (commercial quadruped mocap, climbing mocap) aren't contradicted by anything I saw.

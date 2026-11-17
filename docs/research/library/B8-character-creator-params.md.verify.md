# B8-character-creator-params.md — verification (Library Verify)

Checked 2026-11-17, 08:55–09:40 AEDT (Australia/Sydney). Every URL below was opened on 2026-11-17.
File checked: `B8-character-creator-params.md`, mtime 2026-11-16 11:06:15 AEDT, 22,054 bytes (recorded at start; re-stat before writing this note: unchanged, byte-identical to the start-of-run snapshot).

## Summary

- Rows checked: 34. **VERIFIED 31 · FIX 1 · UNVERIFIABLE 2.**
- The disputed points are settled:
  - **FLAME licence: as the file says.** The official licence page applies the non-commercial licence to "FLAME 2017, FLAME 2019, FLAME 2020, FLAME 2023, FLAME Blender Add-on (FLAME 2020)". A separate "Open Model License … applies ONLY to the FLAME 2023 Open model", and it links CC BY 4.0. The home page news item "11/25: Released FLAME 2023 Open Model with the Open Source License (CC-BY-4.0); previous versions are only available for non-commercial research purposes" confirms this.
  - **Anny bones: 104 by default, 163 for the full rig.** README: "By default, `anny.Anny()` uses the compact `anny` rig with 104 bones … `Anny(rig="makehuman")` exposes the full 163-bone MakeHuman rig." Grok's 163 is the full rig, not the default.
- The one FIX: Anny's local-change list has **254** entries (my count of `local_change_labels` printed in the shape docs, `local_changes="default"`). The file says "about 255".
- **Wiki-only rows:** every Part A game row (Skyrim, Fallout 4, Cyberpunk 2077, BG3, Elden Ring) rests on fan wikis only. No publisher documentation is cited, and the totals are counts made by the researcher and by me. Elden Ring's 57 is **UNVERIFIABLE** for me: eldenring.wiki.gg returns a Cloudflare challenge to curl, to its MediaWiki API and to archive.org (no snapshot), and WebFetch returned only the top of the page. Grok independently reported 57, but Grok alone does not count.
- Items closed since the researcher's draft:
  - MPFB2 `LICENSE.ASSETS.md` is the CC0 1.0 text, and LICENSE.md §C says the assets are CC0.
  - The ICT README confirms that its 53 = ARKit's 52 with `browInnerUp` and `cheekPuff` split into L/R.
  - The GHUM README does print 10,168 vertices (GHUM) and 3,194 (GHUML). The researcher had listed these as unconfirmed.
- Licence caveat to add: the Anny README warns "the free install may download non-commercial only assets when needed".
- Dead links: none. Every Part B URL returned 200. fallout.wiki returns 403 to curl but loads through WebFetch. eldenring.wiki.gg is blocked (see above).

## Row table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| A1 Skyrim controls | 7 tabs; 47 controls (Body 4, Head 6, Face 15, Eyes 8, Brow 4, Mouth 7, Hair 3) | VERIFIED (wiki-only) | My scripted count of "Name :" items = 47, same list. The page intro says "one of the 10 presets", while the Body tab says "20 presets, 10 for male and another 10 for female" (wiki inconsistency) | https://skyrim.wiki.fextralife.com/Character_Creation |
| A1 Skyrim 10 races; Weight raises muscle; extras; Surgery / vampirism | — | VERIFIED (wiki-only) | UESP §Race and §Appearance | https://en.uesp.net/wiki/Skyrim:Character_Creation |
| A2 FO4 sculpting, percentage extras, triangle body | — | VERIFIED (wiki-only) | "Most facial features can be manually sculpted and adjusted"; "changing the percentage value"; "triangle interface". The page does not literally say "no sliders" | https://fallout.wiki/wiki/Fallout_4_Character_Creation (WebFetch; 403 to curl) |
| A2 FO4 presets 15 F / 15 M | — | VERIFIED (wiki-only) | Gallery: Default female face 1–15, male 1–15 | https://fallout.fandom.com/api.php?action=parse&page=Fallout_4_character_creation&prop=wikitext&format=json |
| A2 FO4 SPECIAL 21 + 1 base, max 10 | — | VERIFIED (wiki-only) | Same page, §SPECIAL | same |
| A2 fallout.wiki CC BY-SA 4.0 | licence | UNVERIFIABLE | Footer not visible through WebFetch; curl 403 | — |
| A3 Cyberpunk appearance table (42 aspects) | all option counts and where to change | VERIFIED (wiki-only) | Every row matches the §Appearance wikitext table. "Post-2.2 content" is the researcher's label; the table itself does not give a patch version | https://cyberpunk.fandom.com/api.php?action=parse&page=Cyberpunk_2077_Character_Customization&prop=wikitext&format=json |
| A3 Only body type and voice are locked | — | VERIFIED (wiki-only) | Same | same |
| A4 BG3 controls (19 rows) | body type 4/2; identity 3; 8 voices; maturity; vitiligo; dragonborn crest; genitals 2; heterochromia; hair greying; facial hair restrictions; horns; Draconic Bloodline; tail 3 or none | VERIFIED (wiki-only) | §Appearance | https://bg3.wiki/wiki/Character_creation |
| A5 Elden Ring 57 face-structure sliders, 0–255; other controls | — | UNVERIFIABLE (wiki-only) | Cloudflare challenge (curl, API); no Wayback snapshot; WebFetch truncated. Grok: 57, all 0–255 (lead only) | https://eldenring.wiki.gg/wiki/Character_Creation |
| A6 BDO / DD2 / Sims 4 / MetaHuman not confirmed | absence | VERIFIED | Correctly left out | — |
| B MakeHuman licence | code AGPL-3.0+; assets CC0 1.0 (mesh, targets/modifiers, textures, clothes, poses, expressions) | VERIFIED | LICENSE.md §B, §C | https://github.com/makehumancommunity/makehuman/blob/master/LICENSE.md |
| B MPFB2 licence | code GPLv3; assets same setup as MakeHuman | VERIFIED | LICENSE.md §A–C: assets "released under CC0 1.0 Universal". LICENSE.ASSETS.md = CC0 1.0 legal code. **"Not read" can be closed** | https://github.com/makehumancommunity/mpfb2/blob/master/LICENSE.md ; …/LICENSE.ASSETS.md |
| B Anny mesh | 13,718 vertices, 27,420 triangles | VERIFIED | Shape tutorial output | https://naver.github.io/anny/build/shape_parameterization.html |
| B Anny rig | 104 default; 163 full | VERIFIED | README "Default anny rig" paragraph | https://github.com/naver/anny |
| B Anny phenotypes | 11 listed | VERIFIED | "gender, age, muscle, weight, height, proportions, cupsize, firmness, african, asian, caucasian" | shape tutorial |
| B Anny local changes | "about 255" | **FIX** | **254** labels (my count of the printed `local_change_labels`, default set) | shape tutorial |
| B Anny licences | Apache-2.0 code; MPFB2 and Face Units CC0; soma Apache-2.0; smplx topology NC | VERIFIED | README §License. Add caveat: "the free install may download non-commercial only assets when needed" | README |
| B MHR | 45 identity (20 body, 20 head, 5 hands); 72 expression (artist-sculpted, not PCA, not 1:1 AUs); 204 model params; LOD 0–6; Apache-2.0 | VERIFIED | README §Overview, §Model Parameters, §License | https://github.com/facebookresearch/MHR |
| B FLAME geometry and params | N = 5,023; K = 4; pose 3K+3; 300 shape / 100 expression | VERIFIED | Paper §3 (N, K, pose vector); viewer page "out of 300 … out of 100" | https://download.is.tue.mpg.de/flame/flame_paper.pdf ; https://flame.is.tue.mpg.de/interactivemodelviewer.html |
| B FLAME training data | 3,800 CAESAR heads (2,100 F / 1,700 M); > 33,000 scans; D3DFACS + own 4D | VERIFIED | Paper abstract and §6 | paper PDF |
| B FLAME licence | NC for 2017/2019/2020/2023; 2023 Open CC BY 4.0 (released 11/25) | VERIFIED | Licence page; home page news | https://flame.is.tue.mpg.de/modellicense.html ; https://flame.is.tue.mpg.de/ |
| B ICT-FaceKit | 26,719 v / 26,384 f; face 9,409 v; 100 PCA (Light), 200+ (full); 53 expressions; 4,000 scans; MIT; full model "different USC specific license" | VERIFIED | README tables and text | https://github.com/USC-ICT/ICT-FaceKit |
| B SMPL | 6,890 v; K = 23; 207×3×6,890 = 4,278,690; \|β\| = 300 as in Dyna; up to 300 shape components (site) | VERIFIED | Paper §3, §4 ("We take \|β\| = 300 principal components as in Dyna"); site | https://files.is.tue.mpg.de/black/papers/SMPL2015.pdf ; https://smpl.is.tue.mpg.de/ |
| B SMPL licences | Model NC; Body CC BY 4.0 | VERIFIED | Model licence "non-commercial scientific research"; body licence links CC BY 4.0 | https://smpl.is.tue.mpg.de/modellicense.html ; https://smpl.is.tue.mpg.de/bodylicense.html |
| B SMPL-X | N = 10,475; K = 54; 119 params = 75 + 24 + 10 + 10; full spaces released 20 Aug 2020 | VERIFIED | arXiv §3; site news "20 August 2020" | https://arxiv.org/pdf/1904.05866 ; https://smpl-x.is.tue.mpg.de/ |
| B SMPL-X licence | NC; no distribution | VERIFIED | Model licence: "non-commercial scientific research"; "No Distribution" clause | https://smpl-x.is.tue.mpg.de/modellicense.html |
| B MANO | ~1,000 scans of 31 subjects; NC | VERIFIED | Home page; licence page | https://mano.is.tue.mpg.de/ ; https://mano.is.tue.mpg.de/license.html |
| B STAR | 20% of SMPL's parameters; NC | VERIFIED | arXiv abstract; licence page. ("Extra 10,000 scans" not re-checked) | https://arxiv.org/abs/2008.08535 ; https://star.is.tue.mpg.de/license.html |
| B SUPR | NC (GitHub LICENSE) | VERIFIED | LICENSE: "non-commercial scientific research purposes" | https://github.com/ahmedosman/SUPR |
| B BFM 2009 | 53,490 v; 160,470 tris; 199 PCs; NC; no distribution | VERIFIED | Details page; downloads "Terms of Non-Commercial Use … NO DISTRIBUTION" | https://faces.dmi.unibas.ch/bfm/index.php?nav=1-1-0&id=details ; …nav=1-2&id=downloads |
| B BFM 2017 | NC terms | VERIFIED | "Terms of Non-Commercial Use" | https://faces.dmi.unibas.ch/bfm/bfm2017.html |
| B GHUM | 16-D shape, 4,329 scans; 20-D expression; 63 joints / 124 DOF; > 2 M mocap; > 4.8 K hand; request form; no licence in README | VERIFIED | README feature table; request-form link. README also gives 10,168 v (GHUM) and 3,194 v (GHUML) | https://github.com/google-research/google-research/blob/master/ghum/README.md |
| B ARKit | 52 locations, 0.0 neutral to 1.0 | VERIFIED | Doc JSON: 52 BlendShapeLocation identifiers (+ init); abstract range text | https://developer.apple.com/documentation/arkit/arfaceanchor/blendshapelocation |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Fextralife Skyrim | https://skyrim.wiki.fextralife.com/Character_Creation | y | n/a (wiki) |
| UESP | https://en.uesp.net/wiki/Skyrim:Character_Creation | y | n/a |
| fallout.wiki | https://fallout.wiki/wiki/Fallout_4_Character_Creation | y (WebFetch) / 403 curl | CC BY-SA 4.0 unverified |
| Fallout Fandom | fallout.fandom.com (API) | y | n/a |
| Cyberpunk Fandom | cyberpunk.fandom.com (API) | y | n/a |
| bg3.wiki | https://bg3.wiki/wiki/Character_creation | y | n/a |
| Eldenpedia | https://eldenring.wiki.gg/wiki/Character_Creation | **n (Cloudflare)** | n/a |
| MakeHuman LICENSE | github makehuman LICENSE.md | y | y |
| MPFB2 LICENSE / ASSETS | github mpfb2 | y | y |
| Anny | github naver/anny; docs | y | y (+ NC-download caveat) |
| MHR | github facebookresearch/MHR; arXiv 2511.15586 | y | y |
| momentum | github facebookresearch/momentum | y | MIT (not re-read) |
| FLAME | site, licence, viewer, paper PDF | y | y |
| TF_FLAME / FLAME_PyTorch READMEs | raw GitHub | y | (old CC BY wording; official page wins) |
| ICT-FaceKit | github USC-ICT | y | y |
| SMPL | site, licences, paper | y | y |
| SMPL-X | arXiv, site, licence | y | y |
| MANO | site, licence | y | y |
| STAR | arXiv, licence | y | y |
| SUPR | arXiv, GitHub | y | y |
| BFM 2009 / 2017 | unibas pages | y | y |
| GHUM | README | y | y |
| ARKit | Apple docs (JSON) | y | y |

## Grok section

- B8 was covered by the single B6–B8 Grok pass (prompt `/tmp/lv17/grok/B.txt`). It ran from 08:49:03 to 08:56:14 AEDT and exited with code 0.
- Grok's results:
  - **FLAME:** NC for 2017–2023 and CC BY 4.0 only for 2023 Open. Matches.
  - **Anny:** 104 default and 163 full, 13,718 / 27,420, 11 phenotypes. Matches. Its earlier "163 only" was the full rig.
  - **MHR, ICT, GHUM, SMPL-X, BFM, STAR, SUPR, MANO:** all match.
  - **Elden Ring and Skyrim:** Grok reported 57 sliders at 0–255, and the same Skyrim list. I matched Skyrim. Elden Ring stays UNVERIFIABLE because I could not open the page myself.
- No number in this note was taken from Grok alone.

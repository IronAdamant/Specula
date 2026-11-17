# I1-procedural-prior-art.md — Library Verify

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
I1 itself was not edited. All URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at start: **26,565 bytes**, sha256 `2f677bd675417b7944904c45afaaee04faeebc2a6e56094eb61a2a76c63b7486`.
- Re-stat before writing this file: same size and same sha256 as the start-of-check copy in `/workspace/lv-scratch/i13/`. No re-check of changed rows was needed.
- Box mtimes all read ~09:59 AEDT from the restore and were not used.

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **15** | P1–P12, P14, P17, and the patent/title disagreements |
| **FIX** | **1** | P13: “160,000 catalogued stars” should be “160,000 known star systems” |
| **UNVERIFIABLE** | **2** | P15 (Minecraft 1.18) and P16 (Spelunky HD) — YouTube auto-captions were blocked this pass; no other primary transcript was opened |
| **Total** | **18** | 17 survey rows (P1–P17) plus the Superformula patent status (also in P6) |

**Worst problems**
1. **P13 wording.** Polygon (4 Dec 2014) says Braben’s team started from “**160,000 known star systems** from our galaxy”, not “catalogued stars”. The Stellar Forge / “scientific basis” / “indestructible” quotes and the 80.lv precision claims stand.
2. **P15 and P16 rest only on YouTube auto-transcripts** that this check could not open (YouTube `timedtext` 429 or empty). They are not used as verified numbers.
3. Nothing else in the numbered survey failed a source check. The two close checks (HZD slides; Superformula patent expiry) both pass.

**Dead links:** no true 404s among I1’s cited URLs.
- `https://doi.org/10.1145/1360612.1360626` → ACM 403 (paywall). The Chalmers course copy cited beside it returns 200 and is the copy I read.
- `https://666uille.wordpress.com/2017/03/08/ghost-recon-wildlands-terrain-tools-and-technology/` returns 403 to curl from this box; WebFetch opened it.
- The 9 Jan 2024 Internet Archive capture of Compton’s essay failed TLS in curl; WebFetch opened the same capture. Live Tumblr is 403, as I1 says.

## Close checks
1. **Horizon Zero Dawn numbers are on Guerrilla’s own GDC slides**, not only in secondary write-ups. Official PDF (50 pages, title *GPU-based Procedural Placement in Horizon Zero Dawn*, Jaap van Muijden, created 2017-03-07): https://www.guerrilla-games.com/media/News/Files/GDC2017_VanMuijden_GPUBasedProceduralPlacementInHorizonZeroDawn.pdf
   - Slide 2 Motivation: “Deterministic”, “Locally stable”.
   - Slide 3: “Started traditional CPU” / “Moved to GPU based”.
   - Slide 5 Results: “500+ asset types”, “100.000+ objects in scene”, “~250µs avg busy load on GPU”.
   - Slide 8 WorldData: “~4MB/km2 exclusively”.
   - Slides 34–37: “Solving Collision” then three “Layered Dithering” slides.
   - Slide 41 Conclusion: “Unpolished areas in shippable quality”; “Nature assets created by 3 people”; “Ecotopes made by 1 person”; “250µs avg. busy load”.
   - Guerrilla’s page title is “GPU-Based Procedural Placement in Horizon Zero Dawn” (no “Run-Time”). GDC Vault lists “GPU-Based Run-Time Procedural Placement in 'Horizon: Zero Dawn'” (https://www.gdcvault.com/play/1024120/GPU-Based-Run-Time-Procedural). I1’s title note is right and I1 correctly uses Guerrilla’s title.
2. **US 9,317,627 B2 is listed “Expired - Fee Related” on Google Patents** (https://patents.google.com/patent/US9317627B2/en, opened 2026-11-17). Title, assignee Genicap Beheer BV, grant 2016-04-19 and priority 1999-05-10 all match. Google’s own caveat is on the page: “The legal status is an assumption and is not a legal conclusion.” I1 already carries that caveat and is not a freedom-to-operate search. Related US family members on the same page (US 7,620,527 B1; US 8,775,134 B2) are also listed Expired - Fee Related; other countries were not checked, as I1 says. Ars Technica (updated 25 July 2016) quotes Murray: the game “doesn’t actually use this ‘superformula’ thing or infringe a patent”, and quotes the 2015 New Yorker passage about Murray typing the Superformula into a test planet.

## FIX
- **P13 Elite Dangerous.** Change “~160,000 catalogued stars” to **“~160,000 known star systems”**. Source: Charlie Hall, Polygon, 4 Dec 2014: “Braben’s team started with as much information as they could gather about 160,000 known star systems from our galaxy.” https://www.polygon.com/2014/12/4/7333709/elite-dangerous-milky-way-stellar-forge-human-cruelty
  - The rest of P13 stands: Braben “We’re reasonably confident that everything we do has some sort of scientific basis”; pinning a newly found exoplanet “in a sort of indestructible way”; Doc Ross, 80.lv 2018, “double precision and dual-float precision” (64-bit / two 32-bit floats) and “our job is not done yet”.

## Row table

| Row | Claim (I1) | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| P1 | Spore SIGGRAPH 2008: morphology-independent form; particle IK; Limitations quotes (hug / rub your chin; intra-character collision “clearly a flaw”; “twist unrealistically”; anti-buckling “heavy-handed”; hours vs “weeks”; gait “with anticipation”); p. 10 of an 11-page TOG 27(3) art. 27 article | VERIFIED | Official DOI is 403 (ACM paywall). Read the cited 12-page course PDF: article pages are numbered 27:1–27:11; Limitations and Future Work are on **27:10**. Quotes match, with I1 slightly paraphrasing “no understanding of volume and boundaries” as “no notion of volume or boundaries”, and “within a few hours… it takes weeks to build up an intuition about which kinds of motions generalize”. ACM ©; do not redistribute the course copy. | https://www.cse.chalmers.se/edu/year/2011/course/TDA361/Advanced%20Computer%20Graphics/027-hecker%20copy.pdf ; https://doi.org/10.1145/1360612.1360626 (403) |
| P2 | Hecker GDC 2007 abstract: “most of the content in the game will be made after the game ships”; “two arms and one leg” / “no arms and seven legs, and two mouths”; “what worked and what didn’t” | VERIFIED | Internet Archive recording page, exact abstract text. | https://archive.org/details/GDC2007Hecker |
| P3 | Compton 22 Feb 2016: properties/constraints sentences; failure list; 18,446,744,073,709,551,616 planets; “10,000 Bowls of Oatmeal”; “Perceptual uniqueness”; undefinable “bad” content cannot be filtered | VERIFIED | Opened the cited 9 Jan 2024 IA capture via WebFetch. The source itself says “sets concrete properties” (no “of”); I1 transcribes that wording. Date on the page is “22 2 / 2016”. | https://web.archive.org/web/20240109201851/https://galaxykate0.tumblr.com/post/139774965871/so-you-want-to-build-a-generator |
| P4 | NMS McKendrick GDC 2017: voxel→polygonise→texture→populate at runtime; flat-to-sphere failure; Mercator / Peirce quincuncial; “literal edge cases”; “always know where up is”; team ~20 (about 10 programmers, 5 artists); “every asset… has to count” | VERIFIED | Polygon (2 Mar 2017) confirms the projection / “up” quotes. Team size and “every asset that the artists produce has to count” are in a third-party transcript of the official GDC talk (YouTube captions were 429 from this box): “we’re [effectively] at around 20 people… about 10 programmers code heavy and five artists”. The “engine does not separate generated from hand-authored content” line is I1’s paraphrase of the pipeline talk, not a verbatim quote I pinned in the transcript. | https://www.polygon.com/2017/3/2/14790028/no-mans-sky-was-flat-procedural-world-generation-maths ; https://www.youtube.com/watch?v=sCRzxEEcO2Y ; https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No (200); transcript used: https://gameindustrylibrary.com/documents/continuous-world-generation-in-no-man-s-sky |
| P5 | Murray GDC 2017: “about like three or four years” of noise work collapsed into “Uber Noise”; abs() for rounded hills vs sharp creases; “you were never within floating point accuracy… your Y is no longer up”; “testing the untestable… An infinite number of planets and permutations. You can’t possibly test them all” | VERIFIED | Same situation as P4: official video exists (200); captions blocked; a third-party transcript of that upload has all of the quoted lines, including “you just ABS everything that’s underneath” / “rounded Hills” / “sharp creases”. | https://www.youtube.com/watch?v=C9RyEiEzMiU ; https://gameindustrylibrary.com/documents/building-worlds-in-no-man-s-sky-using-math-s |
| P6 | Superformula dispute; US 9,317,627 B2 expired-fee-related; Murray “doesn’t actually use” quote; New Yorker test-planet passage | VERIFIED | See Close check 2. I1’s “Patent flag” and “not legal advice” wording are appropriate. | https://arstechnica.com/gaming/2016/07/no-mans-sky-faces-potential-patent-fight-over-use-of-superformula/ ; https://patents.google.com/patent/US9317627B2/en |
| P7 | HZD: ~4MB/km²; 500+ types; 100.000+ objects; ~250µs; Layered Dithering slides 34–37; 3 people / 1 person; Deterministic / Locally stable | VERIFIED | Every figure is on the official slide cited. See Close check 1. | Guerrilla page + PDF above |
| P8 | Far Cry 5 GDC 2018: 64×64 m sectors, 256×256 m sections; “604 824 entities in 1 km square”; “FC5 world is about 100 km square”; 2.5-year incoherent forests; density capped for AI/animals; pre-caching “a bad idea”; deterministic nightly rebake; Houdini in Dunia | VERIFIED | Official 145-page slide PDF with speaker notes, Étienne Carrier. Slide numbers match (5–6, 10–11, 34, 132, 139–141, 143). The “100 km square” ambiguity is correctly flagged under Sources disagree; the slide does not say which reading is meant. | https://media.gdcvault.com/gdc2018/presentations/ProceduralWorldGeneration.pdf |
| P9 | Wildlands: real elevation dropped; “Nothing is random…”; World Machine “not 100% deterministic”; 25 cm “overkill”, settled on 50 cm; mid-slope VT+triplanar “VERY expensive”; “magic button” | VERIFIED | 80.lv interview has the first five claims (“1pixel for 25cm… ended up with a 50cm precision”; elevation “from scratch in World Machine”). “This is VERY expensive” is **Guillaume Werlé, in a 10 Jan 2018 comment** on his GDC-talk blog, not in the 80.lv interview and not in the blog’s main post. Still the speaker’s own words on a URL I1 cites. | https://80.lv/articles/procedural-technology-in-ghost-recon-wildlands ; https://666uille.wordpress.com/2017/03/08/ghost-recon-wildlands-terrain-tools-and-technology/ |
| P10 | SpeedTree 2004 postmortem: “Graphic Artist Not On Board”; “A Vision Unshared”; “horrible idea”; trees “looked terrible”; “the occasional flood”; SpeedTreeCAD + SpeedTreeRT | VERIFIED | | https://www.gamedeveloper.com/art/middleware-postmortem-idv-inc-s-speedtreert |
| P11 | Dwarf Fortress: midpoint displacement; temperature / rainfall / orographic rain shadows; biomes “not the basics, they arise”; “57,283 other historical figures”; hoped-for plate tectonics | VERIFIED | Harris/Gamasutra 27 Feb 2008 and Hall/Polygon 23 Jul 2014, as cited. | those two URLs |
| P12 | Elite 1984: 8 galaxies × 256 systems; three 16-bit seeds, “twisted” | VERIFIED | bbcelite.com: “three 16-bit seeds”; “Each of the eight galaxies… 256 systems”; seeds “twisted”. | https://elite.bbcelite.com/deep_dives/twisting_the_system_seeds.html |
| P13 | Elite Dangerous: ~160,000 catalogued stars; Stellar Forge quotes; 64-bit / dual-float | **FIX** | **“160,000 known star systems”**, not “catalogued stars”. Other quotes verified (see FIX). | Polygon 2014 ; https://80.lv/articles/generating-the-universe-in-elite-dangerous |
| P14 | Starfield: 1,000 planets; Howard “no way we’re gonna go and handcraft an entire planet”; “too game-y” if too many finds | VERIFIED | Kotaku, 22 Jun 2023, exact quotes. | https://kotaku.com/starfield-rpg-planets-xbox-pc-gaming-todd-howard-1850565329 |
| P15 | Minecraft 1.18: continentalness / erosion / PV splines; “ended up changing almost everything” | **UNVERIFIABLE** | The cited YouTube video exists (200). Auto-captions were blocked (429/empty). No other transcript opened this pass. | https://www.youtube.com/watch?v=CSa5O6knuwI |
| P16 | Spelunky: 4×4 grid of 16 rooms, entrance-to-exit path, templates; HD “monster itself” | **UNVERIFIABLE** | Same caption block as P15. I1 already left the Spelunky book unread. | https://www.youtube.com/watch?v=RiDy6CgBKqs |
| P17 | Rain World: physics body + “paper doll” cosmetic layer; Jakobsson “care for itself… super difficult and frustrating”; Therrien “basic platformer”; ecosystem can make unbeatable situations | VERIFIED | All of those lines are in Priestman, Gamasutra/Game Developer, 28 Mar 2017. The article says “basically a paper doll”, not I1’s “sort of like a paper doll”. The GDC 2016 talk video exists (200); captions were not opened, so the talk itself was not re-checked. | https://www.gamedeveloper.com/design/crafting-the-complex-chaotic-ecosystem-of-i-rain-world-i- ; https://www.youtube.com/watch?v=sVntwsrjNe4 |
| Patent note | US 9,317,627 B2 Expired - Fee Related | VERIFIED | Close check 2. | Google Patents |
| SD1 | HZD talk title: GDC has “Run-Time”; Guerrilla does not | VERIFIED | Confirmed on both pages. | GDC Vault 1024120 ; Guerrilla page |
| SD2 | NMS Superformula: New Yorker vs Murray 2016 | VERIFIED | Both passages are in the cited Ars article. | Ars Technica |
| SD3 | FC5 “about 100 km square” is ambiguous | VERIFIED | Slide 132 wording is exactly that; no units clarification. | FC5 PDF |
| SD4 | Rain World fun-vs-freedom is a trade-off, not a contradiction | VERIFIED | Both quotes are in the same article, as I1 sets them. | Game Developer article |

Lessons in §2 are I1’s synthesis of the rows above, not independent source claims, and were not counted.

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| R1 Hecker TOG 2008 DOI | https://doi.org/10.1145/1360612.1360626 | 403 (ACM paywall) | Yes: ACM © |
| R1 Chalmers course copy | …/027-hecker%20copy.pdf | 200, 12-page PDF | Yes: “do not redistribute” is the right caution |
| R2 Hecker GDC 2007 IA | https://archive.org/details/GDC2007Hecker | 200 | Yes: talk © |
| R3 Compton IA 2024-01-09 | galaxykate0 tumblr capture | WebFetch 200; curl TLS fail | Yes: © Compton |
| R4–R5 NMS McKendrick / Polygon | GDC Vault, YouTube, Polygon | 200 | Yes: © |
| R6 Murray GDC 2017 | YouTube C9RyEiEzMiU | 200 (captions blocked) | Yes: © |
| R7 Ars Superformula | arstechnica.com/…/superformula/ | 200 | Yes: © |
| R8 US 9,317,627 B2 | Google Patents | 200 | Yes: public record |
| R9 HZD Guerrilla page + PDF | guerrilla-games.com/… | 200 (50-page official PDF) | Yes: © Guerrilla, published for reading |
| R10 FC5 slides | media.gdcvault.com/…/ProceduralWorldGeneration.pdf | 200, 145 pages | Yes: © Ubisoft/GDC |
| R11 Wildlands 80.lv | 80.lv/articles/procedural-technology-in-ghost-recon-wildlands | 200 | Yes: © |
| R12 Werlé blog | 666uille.wordpress.com/… | curl 403; WebFetch 200 | Yes: © author |
| R13 SpeedTree 2004 | gamedeveloper.com/…/speedtreert | 200 | Yes: © |
| R14–R15 DF interviews | Game Developer 2008; Polygon 2014 | 200 | Yes: © |
| R16 Elite seeds | elite.bbcelite.com/… | 200 | Yes: site © Moxon; game © Bell & Braben |
| R17–R18 Elite Dangerous | Polygon 2014; 80.lv 2018 | 200 | Yes: © |
| R19 Starfield Kotaku | kotaku.com/…1850565329 | 200 | Yes: © |
| R20–R22 YouTube talks (Minecraft, Spelunky, Rain World) | three watch URLs | 200; captions 429/empty | Yes: © |
| R23 Rain World article | gamedeveloper.com/…/rain-world | 200 | Yes: © |

I1’s general licence stance (talks/articles ©; facts and short quotes only; no code is open; not a patent FTO search) is correct.

## Grok
A live Grok CLI pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`, live search on) was started at **10:17:18 AEDT** on 2026-11-17, covering I1, I2 and I3 together. This I1 file was written from sources I opened myself, before that process had exited. No number in the table above was taken from Grok. The pass will be summarised in the final report and in this section once it exits.

# D3: Robots and machines: arm sizes and payloads, legged robot masses and speeds

Second check (2026-11-18): clean. All fifteen rows matched their pages, but every row rests on Wikipedia alone (secondary, CC BY-SA 4.0): no maker datasheet or press source was opened, by this file or by the checker. Small additions:
- UR3 and UR10, which the same Wikipedia table lists, added to D3-1.
- The 2013 Atlas row's citation corrected (Wikipedia cites "Atlas robot interactive 3D animation" for it); the Cheetah's cited headline says 28.3 mph.
- The 2024 electric Atlas and the UR datasheets added under "Not found"; licence stated; a "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 3, "Machines and robots". Ground vehicles are in J22, aircraft in J23, rocket engines in J24. Main-library D1 and D2 cover earlier machine topics.

All values here are read from Wikipedia (CC BY-SA 4.0), a secondary source; only facts are taken.

## D3-1. Collaborative robot arms (Wikipedia "Universal Robots" model table; row sources are maker product pages cited in the article)

https://en.wikipedia.org/wiki/Universal_Robots, table "Collaborative robots from Universal Robots" (references include universal-robots.com/cb3/ and trade-press launch reports).

| Model | Release | Payload (kg) | Base footprint (mm) | Arm mass (kg) | Reach (mm) | Payload / own mass (computed by me) |
|---|---|---|---|---|---|---|
| UR3e | 2018 | 3 | 128 | 11.2 | 500 | 0.27 |
| UR5 | 2008 | 5 | 149 | 18.4 | 850 | 0.27 |
| UR5e | 2018 | 5 | 149 | 20.6 | 850 | 0.24 |
| UR10e | 2018 | 12.5 | 190 | 33.5 | 1,300 | 0.37 |
| UR16e | 2019 | 16 | 190 | 33.1 | 900 | 0.48 |
| UR20 | 2022 | 20 | 245 | 64 | 1,750 | 0.31 |
| UR30 | 2024 | 30 | 245 | 64 | 1,300 | 0.47 |
| UR3 | 2015 | 3 | (not retyped) | 11 | 500 | 0.27 |
| UR10 | 2012 | 10 | (not retyped) | 28.9 | 1,300 | 0.35 |

The UR3 and UR10 rows were added after the second check, from the same Wikipedia table (payload, mass, reach and year as the checker relayed them); their ratios are computed by me (3 / 11 and 10 / 28.9). Wikipedia's column header is "Footprint (mm)".

Shape cue (computed by me from the table): a six-axis cobot carries a quarter to a half of its own mass.

## D3-2. Legged and humanoid robots (Wikipedia, each with its cited press or maker source)

| Robot | Figure | Source as cited |
|---|---|---|
| Boston Dynamics Atlas (2013 DARPA version) | 28 degrees of freedom in the limbs | Wikipedia "Atlas (robot)", citing "Atlas robot interactive 3D animation" (the *New York Times* and CNET, 11 July 2013, are cited on neighbouring sentences, not on this one) |
| Atlas, "The Next Generation" (2016) | 150 cm tall, 80 kg | same article, citing Boston Dynamics video 23 Feb 2016 and bostondynamics.com/atlas |
| Atlas (2021 hydraulic) | 1.5 m, 85 kg (190 lb), battery-powered, hydraulic, 20 degrees of freedom | same article, citing Boston Dynamics "Inside the lab: How does Atlas work?" video, 17 Aug 2021 |
| SpotMini (2016) | 25 kg (55 lb) | Wikipedia "Boston Dynamics", citing Yahoo Finance, 23 June 2016 |
| Cheetah (2012) | gallops at 28 mph (12.5 m/s, computed by me), a legged-robot speed record at the time | same, citing *The Verge*, 6 Sep 2012 (the cited headline says 28.3 mph) |
| Handle (2017) | 6.5 ft tall, 9 mph, jumps 4 ft vertically | same, citing CNN Money and *French Tribune*, 28 Feb 2017 |
| Stretch (2021) | 7-degree-of-freedom arm with suction gripper; boxes up to 23 kg | same, citing *The Verge*, 29 Mar 2021 |

## Sources disagree

- Atlas mass and height differ by version (80 kg in 2016, 85 kg in 2021); treat each as its own robot.
- Cheetah's speed: 28 mph in Wikipedia's text, 28.3 mph in the headline it cites.

## Not found

- Joint speed and range tables for the UR arms (the maker product page served no datasheet text to the fetcher). Wikipedia cites the maker's PDFs (for example universal-robots.com/media/240787/ur3_us.pdf), which could be read for the primary values; not opened.
- Figures for the all-electric Atlas of 2024 (Wikipedia mentions it, replacing the hydraulic Atlas, "retired" in 2024, but gives none that were retyped).
- Industrial (non-collaborative) arm figures from a maker datasheet (KUKA, ABB, FANUC).
- Hydraulic and electric actuator figures (pressure, force, power density) from a primary source; Wikipedia "Hydraulic cylinder" gave no cited working pressures.
- Three-view drawings of public-domain machines (not searched this session).

## Sources

- Wikipedia, "Universal Robots" (table "Collaborative robots from Universal Robots"). https://en.wikipedia.org/wiki/Universal_Robots . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Atlas (robot)". https://en.wikipedia.org/wiki/Atlas_(robot) . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Boston Dynamics". https://en.wikipedia.org/wiki/Boston_Dynamics . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Hydraulic cylinder" (named under "Not found"). https://en.wikipedia.org/wiki/Hydraulic_cylinder . CC BY-SA 4.0. Accessed 2026-11-17.
- The sources Wikipedia cites (not opened): universal-robots.com/cb3/ and trade-press launch reports; universal-robots.com/media/240787/ur3_us.pdf; "Atlas robot interactive 3D animation"; Boston Dynamics video, 23 Feb 2016, and bostondynamics.com/atlas; Boston Dynamics, "Inside the lab: How does Atlas work?", 17 Aug 2021; Yahoo Finance, 23 June 2016; *The Verge*, 6 Sep 2012 and 29 Mar 2021; CNN Money and *French Tribune*, 28 Feb 2017.

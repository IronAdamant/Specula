# I4: Buildings and architecture: stairs, handrails, guarding, ramps, bricks, roof pitch, storey height

Fixed after the second check (2026-11-18):
- ADK Table 1.1 ("Rise and going") is on p. 5, not p. 4; Diagram 3.1 ("Guarding design") is on p. 24, not p. 23. Every value matched the text.
- Licences stated: ADK is Crown copyright, published under the Open Government Licence (the PDF's own text does not print the notice); Wikipedia is CC BY-SA 4.0. A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 3, "Prior art and materials" (architecture). Values marked "computed by me" are arithmetic. Material properties are in the main library's I3.

Main source: HM Government, *Approved Document K: Protection from falling, collision and impact*, 2013 edition (England), Crown copyright, Open Government Licence (the standard licence for gov.uk Approved Documents; the PDF's text layer prints no OGL notice, so this rests on that standard term). https://assets.publishing.service.gov.uk/government/uploads/system/uploads/attachment_data/file/996860/Approved_Document_K.pdf (read as text on the box). Page numbers are the document's own.

## I4-1. Stair rise and going (ADK Table 1.1, p. 5)

| Stair category | Rise min-max (mm) | Going min-max (mm) | Steepest pitch from the limits (computed by me, max rise over min going) |
|---|---|---|---|
| Private stair (in a dwelling) | 150-220 | 220-300 | 45° (but the note caps it at 42°) |
| Utility stair | 150-190 | 250-400 | 37.2° |
| General access stair | 150-170 | 250-400 | 34.2° |

Notes to Table 1.1: maximum pitch for a private stair 42°; external tapered steps in dwellings going at least 280 mm; school buildings preferred going 280 mm with rise 150 mm; "twice the rise plus the going (2R + G) equals between 550mm and 700mm".

Check (computed by me): a 175 mm rise with a 250 mm going gives 2R+G = 600 mm and a pitch of 35.0°.

## I4-2. Headroom, widths, flight lengths, handrails (ADK Section 1)

| Item | Value | ADK reference |
|---|---|---|
| Minimum headroom over stairs and landings | 2 m | para 1.11, Diagram 1.3, p. 7 |
| Escape routes clear headroom | 2 m (except doorways) | para 1.12, p. 7 |
| Loft conversion reduced headroom | 1.9 m at the centre of the stair | Diagram 1.4, p. 8 |
| Width between handrails (flights in buildings other than dwellings) | at least 1,000 mm; flights wider than 2 m divided into flights of at least 1,000 mm | para 1.15 |
| Maximum risers per flight | utility stairs 16; general access 12 (exceptionally 16 in small premises) | para 1.18 b (buildings other than dwellings) |
| Change of direction | required if more than 36 risers in consecutive flights | para 1.17 |
| Open risers | a 100 mm sphere must not pass through | para 1.9 b |
| Handrail top height | 900-1,000 mm above the pitch line or floor | para 1.34 a |

## I4-3. Guarding heights (ADK Diagram 3.1, p. 24)

| Building and location | Guarding height |
|---|---|
| Single family dwellings: stairs, landings, ramps, edges of internal floors | 900 mm |
| Single family dwellings: external balconies (including Juliette) and roof edges | 1,100 mm |
| Factories and warehouses: stairs and ramps / landings and floor edges | 900 mm / 1,100 mm |
| Residential, institutional, educational, office and public buildings | 900 mm on flights, otherwise 1,100 mm |
| Assembly: within 530 mm in front of fixed seating / elsewhere | 800 mm / 900 mm on flights, else 1,100 mm |
| Opening windows (all buildings, except roof windows in loft extensions) | 800 mm |

## I4-4. Ramps (ADK para 2.3, Diagram 2.1, p. 18)

| Going of a ramp flight | Maximum gradient | Rise (from the diagram) | Angle (computed by me) |
|---|---|---|---|
| 2 m | 1:12 | 166 mm | 4.8° |
| 5 m | 1:15 | 333 mm | 3.8° |
| 10 m | 1:20 | 500 mm | 2.9° |

Interpolate between (1:14 at 4 m, 1:19 at 9 m). A floor at 1:20 or steeper counts as a ramp; ground-formed landings no steeper than 1:60.

## I4-5. Bricks

| Item | Value | Source |
|---|---|---|
| UK modern brick (from 1965) | 215 × 102.5 × 65 mm; with a nominal 10 mm mortar joint forms a unit of 225 × 112.5 × 75 mm (ratio 6:3:2) | Wikipedia "Brick", citing Scottish Brick History, "Brick sizes, variations and standardisation", https://www.scottishbrickhistory.co.uk/brick-sizes-variations-and-standardisation/ |
| English statute brick of 1625 | 9 × 4.5 × 3 in | same article, citing *Encyclopaedia Britannica* 1911, "Brick", vol. 4 p. 518 |
| Early Near East bricks | 400 × 150 × 100 mm | same article, citing Fiala 2019, IOP Conf. Ser. doi:10.1088/1757-899x/603/3/032097 |

Computed by me: courses per metre of height at 75 mm = 13.3; a 3.0 m storey (I4-6) is 40 courses.

## I4-6. Roof pitch and storey height

Historic named roof pitches (Wikipedia "Roof pitch", citing Knight, *Knight's American Mechanical Dictionary*, "Pitch" def. 2):

| Name | Rule | Angle |
|---|---|---|
| Greek | ridge height 1/9 to 1/7 of span | 12.5° to 16° |
| Roman | ridge height 2/9 to 1/3 of span | 24° to 34° |
| Common | rafter length 3/4 of span | about 48° |
| Gothic | rafters equal the span | 60° |
| Elizabethan | rafters longer than the span | over 60° |

Modern code: Canada's National Building Code defines a low slope as under 1 in 3 (4/12, 18.4° computed by me) (Wikipedia "Roof pitch"; the NBC itself was not opened).

Storey height: Wikipedia "Storey" gives about 10 ft (3.0 m) floor to floor, marked "citation needed"; treat as a rule of thumb only.

## Sources disagree

- Storey height has no primary source here; ADK sets only headroom (2 m) and not ceiling height.

## Not found

- Ceiling height minimums from a building code (ADK does not set them; other national codes not read).
- Door and window standard sizes from a primary standard (Wikipedia "Door" and "Window" gave no cited dimensions).
- Historic castle and cathedral dimensions (wall thickness, vault spans) from a primary source.

## Sources

- HM Government, *Approved Document K: Protection from falling, collision and impact*, 2013 edition (England). https://assets.publishing.service.gov.uk/government/uploads/system/uploads/attachment_data/file/996860/Approved_Document_K.pdf . Crown copyright, Open Government Licence. Accessed 2026-11-17.
- Wikipedia, "Brick". https://en.wikipedia.org/wiki/Brick . CC BY-SA 4.0. Accessed 2026-11-17.
- Scottish Brick History, "Brick sizes, variations and standardisation". https://www.scottishbrickhistory.co.uk/brick-sizes-variations-and-standardisation/ . Accessed 2026-11-17.
- *Encyclopaedia Britannica* (1911), "Brick", vol. 4 p. 518 (as cited by Wikipedia "Brick"). No link. Not opened.
- Fiala (2019), IOP Conf. Ser., doi:10.1088/1757-899x/603/3/032097 (as cited by Wikipedia "Brick"). https://doi.org/10.1088/1757-899x/603/3/032097 . Not opened.
- Wikipedia, "Roof pitch" (citing Knight, *Knight's American Mechanical Dictionary*, "Pitch" def. 2, and Canada's National Building Code; neither opened). https://en.wikipedia.org/wiki/Roof_pitch . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Storey". https://en.wikipedia.org/wiki/Storey . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Door" and "Window" (named under "Not found"). https://en.wikipedia.org/wiki/Door ; https://en.wikipedia.org/wiki/Window . CC BY-SA 4.0. Accessed 2026-11-17.

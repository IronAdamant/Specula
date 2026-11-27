# Specula: library research, round seven

Date: 2026-11-24.

## 0. What this is, and how to answer

**In five lines.**
1. Specula is a game engine written from scratch in Rust. It is getting a library of sample assets made by code: animals, fantasy creatures, machines, trees, plants, terrain, water and people.
2. Each asset kind is built from a **body plan**: a skeleton and the list of numbers that size it (a bone's length as a share of body height, a chest's girth, the timing of each foot in a gait).
3. Every number in a plan must come from a published measurement with its source, or be marked as the engine's own guess.
4. Six earlier rounds of research were delivered. Designing the library found numbers no delivered file holds (part 2), numbers taken from Wikipedia pages where a primary source is wanted (part 3), and numbers the earlier research itself marked unverified or garbled (part 4).
5. You do the web research and send back numbers with their sources. You do not need to know the engine: each request says what the number is for, so you can judge a substitute.

**How a number must come back.** These are the rules of the earlier rounds, unchanged.
1. **Every number with its source:** the paper (authors, year, title, journal, volume, pages, DOI), and the table, figure or page it is on. A number with no source is not used.
2. **What was measured, and on whom:** the species or population, sex and age where they matter, the sample size (n), and the mean with its spread (standard deviation, standard error or range: say which). Not the mean alone, where the source gives more.
3. **Units always**, and how the quantity was measured: between which landmarks, standing or lying, live animal or dry bone.
4. **The licence of every source, said plainly.** For each source say:
   - whether it is **free to read** (open access) or **paywalled**;
   - its licence: public domain or a government work; an open licence (name it: CC0, CC BY 4.0, CC BY-NC, CC BY-SA and so on); or "all rights reserved".
   - A number from a paywalled paper may be reported as a fact with its citation, but say that it is paywalled and whether you read the full text or only the abstract.
   - **Where two sources give the same number, prefer the open one.**
   - Say where a data set may not be redistributed or may not be used commercially.
5. **Data sets are named, never copied whole.** You may read a data set and report the values asked for (for example the mean and spread of one bone for six species), with the file and column they came from. Do not send the whole file back.
6. **Nothing from a book in copyright:** no pictures, no tables copied whole, no wording. Measurements and methods are facts; a book's drawings and text are not.
7. **Tables as plain text** (Markdown or CSV), with each request's number (for example "Q2") on the rows that answer it.
8. **Mark arithmetic as yours:** a ratio or conversion you computed from cited numbers is marked "computed", never presented as a measurement.
9. **Say what you could not find, and where two sources disagree.** A "Not found" list and a "Sources disagree" list at the end are as useful as the rest.
10. **Give an access date** for every web page and file you read.

Everything you send is checked against its source before it is used. The "where to look" hints below are guesses made without searching, offered as places to start: confirm that a source exists and says what is claimed before you rely on it, and prefer a better source where there is one.

## 1. The three data sets on Dryad

Dryad (datadryad.org) is a repository of research data. The earlier rounds named three data sets on it that would close several gaps, and could not read any of them. The owner asked, for each one: **is it free, is it open, does downloading it need an account or a sign-in, how large is it, and if it cannot be had freely, what open alternative holds the same measurements?**

What the earlier research recorded about access (2026-11-16 and 17; to be confirmed, not taken as fact):
- The Dryad API answered HTTP 401 with "must have current bearer token" for file downloads, and the web file download answered HTTP 403.
- The licence field in Dryad's metadata (readable without a download) said CC0 for the two that were checked.
- Two other Dryad data sets were obtained through official mirrors on Zenodo (zenodo.org records 4964743 and 4969945, both CC0), so a mirror sometimes exists. For Serio et al. 2024 none was found on Zenodo or figshare.

**Questions for every data set below (please answer each):**
- (a) Is it free to download, with no fee?
- (b) Is it open, and under which licence? The project's notes say CC0: **claimed, to be confirmed**. Please read the licence on the data set's own page.
- (c) Does downloading need an account, a sign-in or a token, by the web page and by the API? If a sign-in is needed, what kind (an ORCID login, a Dryad account, an institution's login), and is that account free for anyone?
- (d) How large is it: how many files, what formats, how many megabytes?
- (e) Is there an official mirror (Zenodo, figshare, the journal's supplement) that can be had without signing in?
- (f) Which columns or measurements does it actually hold, and for which species (a list, or the count and the groups)?
- (g) If it cannot be had freely: what open source holds the same measurements?
- (h) If you can read it: the values asked for under "What the project wants", for the species listed, with n and spread.

### DS1. Serio et al. 2024: limb bones and skull of living mammals

- **Paper:** Serio C, Brown RP, Clauss M, Meloro C (2024). *Palaeontology* 67(4): e12720. doi:10.1111/pala.12720. The project's notes give the paper's licence as CC BY 4.0 (to be confirmed). The paper's title is not in the project's files.
- **Data set:** Dryad, doi:10.5061/dryad.8gtht76sp. Licence: CC0 1.0 in Dryad's metadata as read on 2026-11-16 (**claimed, to be confirmed**).
- **What the project's notes say it holds:** humerus, radius, femur and tibia lengths, and greatest skull length, for 246 living species. Which species, and whether it gives one value or several specimens a species, is not known.
- **What the project wants, and for which plan:** the **quadruped** plan (dogs, wolves, cats, big cats, horses, deer, cattle, pigs, bears, elephants, rabbits, rats). For each of those species that it holds: humerus, radius, femur, tibia and greatest skull length in mm, with n, mean and spread, and body mass if given. The delivered research has bone-by-bone lengths for wolf, African wildcat, lion, two bears and a horse, but not for adult dogs, deer, adult pigs, cattle (beyond three bulls), elephants (beyond two), rats, or tiger and leopard hind limbs; and skull length for almost none (only wolf and rabbit).

### DS2. Skelevision: Weeks et al. 2025, bird skeletons

- **Paper:** Weeks BC, Zhou Z, Probst C, Berv J, et al. (2025). "Skeletal trait measurements for thousands of bird species." *Scientific Data*, article s41597-025-05234-y. doi:10.1038/s41597-025-05234-y. The paper's licence was checked by the earlier round as **CC BY-NC-ND 4.0** (non-commercial, no derivatives).
- **Data set:** Dryad, doi:10.5061/dryad.v41ns1s4c. Licence: CC0 on the Dryad landing page (**claimed, to be confirmed**). The project's notes say it holds 14,419 skeletons of 2,057 species, and call them passerine skeletons; whether it covers non-passerine birds too is not known. Its code is on GitHub; no mirror of the data was found.
- **What the earlier round read from the paper:** only Table 1 (the machine measurement's error against hand measurement: humerus 0.47 mm, ulna 0.96 mm, carpometacarpus 1.11 mm root-mean-square error, 100 specimens).
- **What the project wants, and for which plan:** the **bird** plan. Per species, with n and spread: the **leg bones** (femur, tibiotarsus, tarsometatarsus) and the wing bones (humerus, ulna, carpometacarpus), in mm, and any body size measure given beside them (mass, or a skull or keel length). No leg-bone lengths were delivered for any bird. Please say which measurements it holds before extracting: if it holds no leg bones, say so. Wanted species if present: a pigeon, a crow or raven, a sparrow or finch, a gull, a hawk or eagle, an owl, a heron, a duck, a chicken or other game bird; if it is passerines only, give five passerines of different sizes.

### DS3. Di Santo et al. 2021: the body wave of swimming fish

- **Paper:** Di Santo V, Goerig E, Wainwright DK, Akanyeti O, Liao JC, Castro-Santos T, Lauder GV (2021). "Convergence of undulatory swimming kinematics across a diversity of fishes." *PNAS* 118(49): e2113206118. doi:10.1073/pnas.2113206118. Free to read at PubMed Central (PMC8670443). Licence: "Published under the PNAS license" (free to read, not a Creative Commons licence), as the earlier round read it.
- **Data:** two things are named in the project's notes, and it is not clear whether they are the same: (1) **Dataset S2**, which appears to be part of the paper's supplementary information; (2) a Dryad deposit, doi:10.5061/dryad.bg79cnp9x, CC0 according to its DataCite record (**claimed, to be confirmed**). Please say what each holds and where each can be had.
- **Note on access:** the earlier round says it did **not download** these under its own rule against downloading data sets, not that the download failed. Whether they need a sign-in is therefore not known at all.
- **What the project wants, and for which plan:** the **chain** plan (fish, eels, snakes, and tails and tentacles on other plans). For each of the 44 species, at the slowest common speed (about 1 body length per second) and if given at a faster one: the body wavelength (as a share of body length), the amplitude at the head, at mid-body and at the tail tip (as shares of body length, saying whether peak-to-peak or half amplitude), the tail-beat frequency (Hz), and the body length and swimming mode of each species. The paper's own summary by swimming mode was delivered; the per-species values were not.

## 2. Missing measurements, by plan

Each request is numbered. "**Blocks a plan**" means the plan has nothing measured for this today, so the number is the engine's guess until it comes. "**Improves a plan**" means something usable was delivered and this makes it better.

The list is ordered by what unblocks the most. **Four-legged animals' body bulk comes first**: limb bones and gaits were delivered, but no girth, depth, breadth, neck or tail length was delivered for any animal, so every animal's trunk is a guess today, and the four-legged plan also carries dragons and griffins.

The species list "the quadruped set" below means: **dog (give the breed, and prefer a medium breed such as a Labrador or German shepherd, plus one small and one giant breed if available), grey wolf, domestic cat, lion, tiger, leopard, horse (give the breed), a deer (red deer or white-tailed deer), domestic cattle, domestic pig (adult), brown or black bear, African or Asian elephant, rabbit, brown rat.**

Where such numbers are usually published (hints, not claims): livestock and veterinary body-measurement studies (heart girth, withers height and body length are routine for cattle, horses, pigs, goats, camels, dogs); wildlife-management capture records (chest girth to estimate a bear's or a deer's mass); zoo and timber-camp elephant studies; museum osteological collections and zooarchaeology standards (bone measurements); comparative anatomy and biomechanics papers; forestry inventories and laser-scan studies for trees.

### 2A. Four-legged animals (the quadruped plan)

**Q1. Girths, depths and breadths of the body.** Blocks a plan.
- Quantities: **chest girth** (heart girth: the circumference just behind the forelegs, in cm); **chest depth** (withers to sternum, vertical, cm); **body breadth at the shoulders** and **at the hips** (cm); **abdominal girth** if given (cm); **withers height** (ground to the highest point of the shoulders, cm) and **body length** (point of shoulder to point of buttock, cm), which the other measures are divided by; body mass (kg).
- Species: the quadruped set.
- Needed for: to give a four-legged body its bulk. Chest girth against withers height is the single most wanted number. None was delivered for any animal.
- Form: mean and spread, n, sex, adult age; the correlation of girth with withers height or with mass if given.
- Where: livestock and equine morphometric studies; canine breed morphometrics; wildlife capture data for bears, deer and big cats; elephant body-measurement studies.

**Q2. Neck and tail lengths, and vertebra counts.** Blocks a plan.
- Quantities: neck length (first cervical vertebra to the first thoracic, or poll to withers on the live animal: say which), tail length (base of tail to tip, without the hair), both in cm and as a share of trunk length or withers height; the number of cervical, thoracic, lumbar, sacral and caudal vertebrae, with the range where it varies (dogs' and cats' tails vary).
- Species: the quadruped set. (The earlier round found only the rabbit's caudal count, 15 to 17.)
- Needed for: the neck chain (3 bones) and tail chain (4 to 8 bones) of the skeleton, and how many joints they bend at. A tail and a neck are today the engine's guess for every species.
- Form: mean and spread, n; vertebra counts as a mode and range.
- Where: veterinary anatomy; comparative osteology; morphometric breed studies (tail length is measured in dogs and cats); a "PanTHERIA"-type trait database (PanTHERIA itself has no tail field).

**Q3. Metacarpal, metatarsal and digit lengths.** Blocks a plan.
- Quantities: length of metacarpal III (and IV in artiodactyls: deer, cattle, pigs) and metatarsal III, and the lengths of the phalanges of the main digit, in mm; their share of the whole limb length.
- Species: dog, wolf, cat, pig, deer, elephant (felids, horse, cattle and rabbit were delivered as percentages).
- Needed for: the foot and lower leg, which decide whether a leg reads as a dog's, a deer's or an elephant's.
- Form: mean and spread, n, adult.
- Where: zooarchaeology osteometric tables; museum collections; comparative limb-proportion papers (Serio et al. 2024 in part 1 may not hold these).

**Q4. Skulls, muzzles, ears, horns and antlers.** Blocks a plan.
- Quantities: greatest skull length, zygomatic breadth, muzzle (snout) length (from the eye's front corner or the stop to the nose tip, on the live animal, or the facial skull length on a dry skull: say which), all in mm; ear length (notch to tip, cm); for deer, antler beam length and spread; for cattle, horn length and spread (cm).
- Species: the quadruped set (wolf and rabbit skull sizes were delivered; nothing else).
- Needed for: the head's size and shape against the body.
- Form: mean and spread, n, sex (antlers and horns differ by sex and age; give adult males and females).
- Where: craniometric studies; canine and feline breed skull studies; deer management antler scoring data.

**Q5. Body mass with spread.** Improves a plan.
- Quantities: adult body mass (kg), mean and spread, by sex.
- Species: the quadruped set. PanTHERIA gave one number a species.
- Needed for: sizing within a species (a seed varies an animal around the mean), and mass-based scaling rules.
- Where: species accounts in mammalogy journals; animal-science breed standards; wildlife surveys.

**Q6. Standing joint angles in quiet stance.** Improves a plan.
- Quantities: the angles of the shoulder, elbow, carpus, hip, stifle (knee) and hock (ankle) when the animal stands still and square, in degrees, saying how each angle is measured (between which bones, or against the vertical).
- Species: dog, cat, horse, cattle, and any of the rest available.
- Needed for: the skeleton's rest pose. Without it the legs stand at angles chosen by eye.
- Where: veterinary kinematics papers (standing posture is often reported as the start of gait analysis); equine conformation studies.

**Q7. Joint ranges of the limbs and the neck.** Improves a plan.
- Quantities: the range of motion (flexion and extension, in degrees) of the shoulder, elbow, carpus, hip, stifle and hock, and of the neck (up, down, sideways), measured with a goniometer or from video.
- Species: dog, cat, horse.
- Needed for: limits on the skeleton so a leg cannot bend backwards.
- Where: veterinary goniometry studies (dogs and cats in particular); equine biomechanics.

**Q8. Footfall timing the earlier rounds lacked.** Improves a plan.
- Quantities: for the horse, the phase of each foot's touchdown with the left hind at 0 (as a share of the stride), and the duty factor (share of the stride each foot is on the ground), with spread, for walk, trot, canter and gallop. For the domestic cat: the same, plus the speeds at which it changes gait. For the bound and the half-bound (any species): the phases and duty factors.
- Needed for: the gait tables. A dog's footfalls were delivered; the horse's were delivered with the left fore as reference only, and Hildebrand's papers (1965, *Science* 150:701; 1977, *J. Mammal.* 58:131) were paywalled.
- Where: Hildebrand's gait papers; equine gait-analysis studies; feline locomotion studies.

### 2B. Sprawling animals (lizards, crocodiles, tortoises, salamanders)

**Q9. Lizard limb proportions against snout-vent length.** Blocks a plan.
- Quantities: humerus, forearm, femur, tibia, foot and longest toe lengths (mm), and tail length, each against snout-vent length (SVL: snout tip to the cloaca), with spread.
- Species: several lizards of different builds: a monitor (*Varanus*), an iguana, an *Anolis*, a skink, a gecko, a desert iguana (*Dipsosaurus dorsalis*).
- Needed for: the sprawling plan's limbs. Only one species' ratios were delivered, and those unverified (part 4, U7).
- Where: lizard ecomorphology papers (for example Losos 1990 on *Anolis*, named by the earlier round as paywalled).

**Q10. Turtle limb and shell lengths.** Blocks a plan.
- Quantities: carapace length, width and height; plastron length; humerus, femur and neck lengths, in mm, with spread.
- Species: a tortoise (*Testudo* or *Gopherus*), a pond turtle (*Trachemys* or *Chrysemys*).
- Needed for: a tortoise's body and legs; nothing was delivered.

**Q11. Crocodile high walk from the primary text.** Improves a plan.
- Quantities: duty factor and femoral adduction angle (degrees below horizontal) in the sprawl and in the high walk; the speed of each.
- Species: American alligator, a crocodile.
- Needed for: the crocodile's walk. The delivered numbers are from a reading of Reilly and Elias 1998 that could not be confirmed (part 4, U9).

### 2C. Birds

**Q12. Leg bones and toes.** Blocks a plan.
- Quantities: femur, tibiotarsus and tarsometatarsus lengths (mm), and the toes' lengths, with spread.
- Species: as in DS2 (a pigeon, a crow, a sparrow, a gull, a hawk or eagle, an owl, a heron, a duck, a chicken; an ostrich or emu for flightless birds).
- Needed for: every bird's legs. None delivered. (DS2 may answer this.)

**Q13. Neck vertebra count and neck length.** Blocks a plan.
- Quantities: number of cervical vertebrae; neck length (mm, and as a share of body length).
- Species: as Q12, plus a swan and a flamingo for long necks.
- Needed for: the bird's long neck chain (8 to 14 bones).

**Q14. Body girth of birds.** Blocks a plan.
- Quantities: body girth or body breadth and depth at the widest point (mm), and body length without the tail (mm), against mass.
- Species: as Q12.
- Needed for: a bird's body bulk under the feathers, as Q1 for mammals.

**Q15. Downstroke fraction across species.** Blocks a plan.
- Quantities: the share of the wingbeat cycle spent in the downstroke (%), with spread, at a stated flight speed.
- Species: as many as are measured; at least a pigeon, a finch, a duck, a hummingbird, a gull.
- Needed for: the timing of the wingbeat. Only two species' phase timing was found.

**Q16. Wing folding angles.** Improves a plan.
- Quantities: elbow and wrist angles (degrees) with the wing folded at rest and fully spread.
- Species: any beyond the pigeon (only the pigeon was found).

### 2D. Bats, and the winged plans (dragons, wyverns, griffins)

**Q17. Bat metacarpal and phalanx lengths in millimetres.** Improves a plan.
- Quantities: forearm length, and the lengths of metacarpals III, IV and V and their phalanges (mm), with spread.
- Species: several bats across sizes: a fruit bat (*Pteropus*), a vampire bat, a *Myotis*, a free-tailed bat.
- Needed for: the wing's finger bones. The earlier round had digit lengths for some species and shares, but the per-bone values of Castillo-Figueroa 2020 were only size-scaled, with no millimetre table.

**Q18. Where the wing membrane attaches, by species.** Improves a plan.
- Quantities: where the main membrane (plagiopatagium) meets the hind leg (ankle, foot, toe base, or along the shin), and the calcar's length (mm).
- Species: as Q17. Only the *Myotis* examples were found.

**Q19. Launch speeds and times.** Improves a plan.
- Quantities: take-off speed (m/s), time from the start of the push to leaving the ground (s), peak force as multiples of body weight.
- Species: bats (Schutt et al. 1997 on the vampire bat was found only as an abstract); large birds; pterosaurs if any peer-reviewed table exists (none was found).

**Q20. Mass scaling for a flyer larger than any known.** Improves a plan.
- Quantity: a published physical argument for how mass, wing area and wing loading must scale for a flying animal heavier than the largest pterosaurs (above about 250 kg), and the limit it gives.
- Needed for: a dragon's size and wing area. If no paper exists, say so: the engine then marks its own scaling as its own.

**Q21. Wing membrane thickness.** Improves a plan.
- Quantity: thickness of a bat's wing membrane (µm or mm), with spread, and where on the wing.

### 2E. Chains (snakes, eels, fish; and tails, necks and tentacles)

**Q22. Girth along the body of snakes and eels.** Blocks a plan.
- Quantities: body girth or width at the head, at mid-body, at the vent and along the tail, each as a share of maximum girth, and the place of each as a share of total length.
- Species: a python or boa, a rattlesnake or viper, a colubrid (rat snake or garter snake), a sea snake, a European or American eel.
- Needed for: the section along a chain body. None delivered.

**Q23. Head and tail length against snout-vent length; vertebra counts.** Improves a plan.
- Quantities: head length and tail length (mm) against SVL; counts of body (precloacal) and tail vertebrae.
- Species: as Q22.

**Q24. Fin origins as shares of body length.** Blocks a plan.
- Quantities: where each fin begins and ends (dorsal, anal, pectoral, pelvic, caudal), as a share of standard length or total length (say which), with spread.
- Species: trout, mackerel, tuna, eel, bluegill, a cod, a shark (dogfish).
- Needed for: placing fins on a fish. No multi-species table with spread was found; FishBase gives single-picture values (and is CC BY-NC).

**Q25. Per-species wavelength and amplitude envelope.** Blocks a plan.
- See DS3. If DS3 cannot be had: the same quantities from any open paper per species.

**Q26. Lateral undulation speed of snakes in body lengths per second.** Improves a plan.
- Quantities: speed (body lengths per second, and m/s) in lateral undulation, across species, with spread.
- Where: Jayne 1986, *Copeia* (named by the earlier round, behind JSTOR).

### 2F. Arthropods (insects, spiders, crabs)

**Q27. Leg segment lengths with spread, every leg.** Blocks a plan.
- Quantities: coxa, trochanter, femur, tibia and tarsus (insects); coxa, trochanter, femur, patella, tibia, metatarsus and tarsus (spiders); each in mm, for each leg pair, with spread and n; body length.
- Species: an ant (*Cataglyphis*, *Formica* or *Camponotus*), a cockroach (*Periplaneta americana* or *Blaberus discoidalis*), a beetle (any large ground beetle or stag beetle), a spider (a tarantula and an orb-weaver or wolf spider); a crab if found.
- Needed for: the arthropod leg. Only a stick insect had a full measured table with spread; the ant was a model's values; the tarantula's tarsus cells were garbled (part 4, U1).

**Q28. A spider's duty factor against speed.** Improves a plan.
- Quantities: duty factor and stride frequency (Hz) against speed (m/s), as a fitted line or a table.
- Species: any spider; the delivered values (Biancardi et al. 2011) are unverified (part 4, U5).

### 2G. Machines

**Q29. Ackermann steering geometry and track-to-wheelbase ratios.** Blocks a plan.
- Quantities: for named cars and trucks: wheelbase, front and rear track (mm), maximum steering angle of the inner and outer wheel (degrees), turning circle (m), and the Ackermann percentage if stated.
- Needed for: wheeled vehicles that steer believably.
- Where: manufacturers' published specifications; vehicle-dynamics teaching material.

**Q30. Track pitch, sprockets and idlers of a named tracked vehicle.** Blocks a plan.
- Quantities: track pitch (link length, mm), track width, number of links, drive sprocket teeth and pitch diameter, idler and road-wheel diameters, number of road wheels, track length on the ground (mm).
- Vehicles: a named excavator (for example a 20-tonne class excavator) and a named tracked military or agricultural vehicle.
- Where: manufacturers' specification sheets; government (public domain) technical manuals.

**Q31. Suspension travel by vehicle class.** Improves a plan.
- Quantities: wheel travel (mm, bump and rebound) for a passenger car, an SUV, an off-road vehicle, a lorry, a tank.
- The earlier round found ride frequencies and damping but no travel.

**Q32. Joint ranges of a published humanoid robot and a four-legged robot, from a primary source.** Blocks a plan.
- Quantities: each joint's range (degrees), maximum speed (deg/s) and torque (N m), and the link lengths (mm).
- Robots: any humanoid with a published specification (the iCub, the NASA Valkyrie, Unitree H1, Boston Dynamics Atlas) and a four-legged robot (MIT Cheetah, ANYmal, Unitree Go1 or A1, Boston Dynamics Spot).
- Where: the maker's data sheets and user manuals; the robot's own published papers. The delivered figures rest on Wikipedia (part 3).

**Q33. Three-view dimensions of public-domain machines.** Improves a plan.
- Quantities: overall length, width, height and the main dimensions (wheel diameters, cab size, boom lengths) from drawings that are in the public domain (for example United States government technical manuals).

### 2H. People (the biped plan)

**Q34. The spine's standing curves.** Blocks a plan.
- Quantities: lumbar lordosis, thoracic kyphosis and the cervical curve, in degrees, measured standing (say the method: Cobb angle on a radiograph, or a surface method), by sex and age group.
- Needed for: the spine's shape in the standing figure (the spine has three bones in the plan).
- Form: mean and spread, n.

**Q35. The collarbone and the shoulder blade.** Blocks a plan.
- Quantities: clavicle length (mm) and its angle to the horizontal and to the frontal plane at rest (degrees); the scapula's resting place (the height of its upper angle and inferior angle against the spine's vertebrae, its distance from the spine, its tilt), by sex.
- Needed for: the shoulder girdle, which the figure does not yet have.

**Q36. Muscle volumes or cross-sections along the limbs and trunk.** Blocks a plan.
- Quantities: cross-sectional area (cm²) or volume (cm³) of the upper arm, thigh, calf and trunk muscles at stated levels (for example at 25%, 50% and 75% of the segment's length), by sex.
- Needed for: the surface on the skeleton. The delivered research has two muscles' thickness only (the calf and the front of the thigh).
- Where: MRI and CT studies of muscle volume; sports-science body-composition papers.

**Q37. A woman's trunk sections between the hip and the shoulder.** Improves a plan.
- Quantities: breadth, depth and girth (cm) at several levels between the hip and the armpit (for example at the waist, under the bust, at the bust, at the armpit), with spread.
- The delivered sections are the chest, waist and hip only.

**Q38. Children and old age.** Improves a plan.
- Quantities: segment lengths (head, trunk, arm, leg) as shares of stature, and girths, by age and sex, for ages 2 to 18 and over 65.

**Q39. Turning and climbing stairs.** Improves a plan.
- Quantities: step length, cadence and joint angles (hip, knee, ankle, in degrees) while turning in a walk and climbing and descending stairs; the timing of a turn.

### 2I. Trees and plants

**Q40. Branch angles by branch order, measured.** Blocks a plan.
- Quantities: the angle between a branch and its parent (degrees) for first-, second- and third-order branches, with spread, measured in the field or by laser scan.
- Species: oak (*Quercus robur* or *Q. rubra*), maple (*Acer*), birch (*Betula*), beech (*Fagus sylvatica*), Douglas fir, spruce (*Picea abies*), pine (*Pinus sylvestris*), willow (*Salix*).
- Needed for: the tree plan. No branch angle for any common species was verified; two beech studies were found but could not be opened (part 4, U19).

**Q41. Branch length and radius ratios per order, from field measurement.** Improves a plan.
- Quantities: a child branch's length and radius as a share of its parent's, per order.
- Where: Bentley et al. 2013 (paywalled for the earlier round); Malhi et al. 2018 (laser scans of Wytham Woods: the PDF could not be fetched); any open laser-scan study.

**Q42. Whorl spacing of conifers.** Improves a plan.
- Quantities: vertical distance between branch whorls (cm), branches a whorl, against tree height or age.
- Species: spruce, pine, fir.

**Q43. Forms of olive, coconut, baobab and acacia.** Improves a plan.
- Quantities: height against trunk diameter, crown width, for the coconut the number of fronds and frond length; with spread.
- The earlier round found no verified equation for any of these except a small desert acacia.

### 2J. Terrain and water

**Q44. Colour and roughness of rock, soil, sand and snow by type, measured.** Improves a plan.
- Quantities: reflectance or albedo in the visible band (and if possible at 450, 550 and 650 nm), dry and wet; surface roughness as measured (say which kind: optical, or aerodynamic).
- Materials: granite, limestone, sandstone, basalt, slate, clay soil, loam, peat, beach sand, desert sand, fresh snow, old snow, glacier ice.
- Prefer open spectral libraries (the United States Geological Survey's spectral library is public domain; the ECOSTRESS library is "all rights reserved").

**Q45. The steps of the erosion methods as published.** Improves a plan.
- What: the published step-by-step algorithms, with their parameters and units, for thermal erosion (Musgrave et al. 1989), hydraulic erosion (Mei et al. 2007), and stream-power erosion (Cordonnier et al. 2016). The earlier round had the constants and an outline, not the steps.
- Say for each paper whether it is free to read.

**Q46. Water colour by depth and by Jerlov water type.** Improves a plan.
- Quantities: the attenuation coefficient Kd (per metre) at several wavelengths for each Jerlov type (I, IA, IB, II, III, coastal 1 to 9); the colour of clear ocean, coastal and lake water at stated depths.
- The earlier round found these only as abstracts or secondary mentions.

**Q47. A waterfall's path.** Improves a plan.
- Quantities: the trajectory of water leaving a brink (horizontal throw against fall height and flow speed), the brink depth against critical depth, the spread of the falling sheet.

**Q48. Where eddies and standing waves form.** Improves a plan.
- Quantities: eddy size against channel width and the size of the obstruction; the Froude-number bands in which standing waves and antidunes form.

**Q49. Lakes' shores.** Improves a plan.
- Quantities: typical shore slopes by material; wave height on a lake against wind speed and fetch (the fetch-limited wave formulas).

## 3. Numbers that came from Wikipedia pages and want a primary source

Some numbers were taken from Wikipedia pages (whose text is licensed CC BY-SA 4.0, a share-alike licence). The project uses them as facts with the page named, but would rather cite the primary source each page relies on. Pages also change, so the figure on the page today may not be the figure the primary gives.

**For each row, please send back:** the primary source behind the page's figure (the paper, the maker's data sheet, the standard); whether it is free to read; its licence; and **the figure as the primary source gives it**, with its page or table. If the primary gives a different value or a spread, say so.

The rows are grouped by delivered file. "Checker" is what a second checker said about the row in 2026-11. "Page" is the Wikipedia article the file names, as read on 2026-11-17 unless said.

### 3.1 File C10: giant and extinct animals (for dragons and other giant creatures)

The whole file is read from Wikipedia text. The checker found every row as the file states it on the pages, and flagged every row as secondary. Two papers were opened by the file (Witton and Habib 2010, Sellers et al. 2013, both CC BY); the rest were not.

| # | Number, unit | What it describes | Page | Primary as the page cites it | Checker | Wanted |
|---|---|---|---|---|---|---|
| W1 | 5.2 to 25.8 m historically; 10 to 11 m in recent estimates | Wingspan of *Quetzalcoatlus northropi* | "Quetzalcoatlus" | Witton and Habib 2010, *PLoS ONE* 5: e13982, doi:10.1371/journal.pone.0013982 (opened: 10 to 11 m confirmed); Andres and Langston 2021, *J. Vert. Paleontol.* 41: 46-202, doi:10.1080/02724634.2021.1907587 (not opened) | Verified on the page; 10 to 11 m confirmed in Witton and Habib | The span as Andres and Langston 2021 give it; the source of the 5.2 and 25.8 m ends |
| W2 | 30 kg (at 5.2 m span) to 440 kg (at 15.4 m span) | Mass of *Q. northropi*, Greenewalt's range | "Quetzalcoatlus" | Greenewalt (no year or title in the file) | Verified on the page | Greenewalt's paper and its values |
| W3 | 200 to 250 kg | Mass of *Q. northropi* | "Quetzalcoatlus" | Paul 2002, *Dinosaurs of the Air*, p. 333 (a book); Witton and Habib 2010 (opened) | Checker: the page credits Paul 2002 alone; Witton and Habib 2010 state it as most reliable | Confirmed from Witton and Habib only; nothing more needed unless a newer estimate exists |
| W4 | 544 kg | Mass of *Q. northropi* by 3D slicing | "Quetzalcoatlus" | Henderson 2010, *J. Vert. Paleontol.* 30: 768-785 (not opened; confirmed as quoted in Witton and Habib 2010) | FIX applied: value added | Henderson 2010's own table: mass, assumed span and density |
| W5 | males average 5.6 m, range 5.6 to 7.6 m; females 3.8 m | Wingspan of *Pteranodon* | "Pteranodon" | Bennett 1994; Bennett 2001, *Palaeontographica A* 260: 1-112 (not opened) | Verified; range added | Bennett's values with n |
| W6 | 6.25 to 6.5 m span, body length 2.6 m, mass about 50 kg | Large *Pteranodon* specimen USNM 50130 | "Pteranodon" | Bennett 1994, Bennett 2001, Larramendi et al. 2021 (three references on one sentence) | Note: the mass may be Larramendi's, not Bennett's | Which of the three gives the 50 kg, and how it was estimated |
| W7 | 80 to 100 t | Mass of *Argentinosaurus* | "Argentinosaurus" | Paul 1994, *The Dinosaur Report* pp. 12-13 | Verified on the page | The primary, if it can be read |
| W8 | 60 to 88 t, 73 t most likely | Mass of *Argentinosaurus* | "Argentinosaurus" | Mazzetta et al. 2004 | Added by the checker | Mazzetta et al. 2004 (journal, pages) and its method |
| W9 | 83.2 t | Mass of *Argentinosaurus* by volume of a mounted reconstruction | "Argentinosaurus" | Sellers et al. 2013, *PLoS ONE* 8: e78733 (opened, CC BY) | Verified | Nothing more needed |
| W10 | 90 and 95 t | Mass of *Argentinosaurus* by limb-bone scaling | "Argentinosaurus" | Benson et al. 2014, *PLoS Biol.* 12: e1001853 (not opened), and "2018" | Verified on the page | The figure in Benson et al. 2014 (open access) and the 2018 paper's citation |
| W11 | 96.4 t | Mass from an isolated femur of uncertain referral | "Argentinosaurus" | González Riga et al. 2016 | Added by the checker | The paper and its figure |
| W12 | over 50 t; 65 to 75 t | Mass of *Argentinosaurus* | "Argentinosaurus" | Paul 2016; Paul 2019 | Added by the checker | Both primaries (2016 may be a book) |
| W13 | about 75 t | Mass of *Argentinosaurus* | "Argentinosaurus" | Campione and Evans 2020 | Added by the checker | The paper and its figure with its error |
| W14 | about 5,650 kg | Mass of the largest adult *Tyrannosaurus*, "Sue" (FMNH PR2081) | "Tyrannosaurus" | Erickson et al. 2004, *Nature* 430: 772-775 (not opened) | Verified on the page | The figure in Erickson et al. 2004 |
| W15 | 8.87 t | Mass of "Scotty", "the largest known specimen" | "Tyrannosaurus" | not named in the file | Added by the checker | The primary paper for Scotty's mass |
| W16 | historical averages 4.5 to over 7.2 t; "most modern estimates" 5.4 to 8.0 t | Spread of adult *T. rex* mass | "Tyrannosaurus" | not named in the file | Added by the checker | The papers behind the range |
| W17 | under 1,800 kg until about 14 years, then about 600 kg a year for four years | Growth of *T. rex* | "Tyrannosaurus" | Erickson et al. 2004 | Verified on the page | The growth curve as Erickson gives it |
| W18 | about 75 cm long, 2.5 kg | A hatchling *T. rex* | "Tyrannosaurus" | Longrich et al. 2026, *Biology* 15: 1090, doi:10.3390/biology15131090 (not opened) | Verified on the page | The figure in Longrich et al. (open access journal) |
| W19 | mostly about 9 m/s; as low as 4.5 to 6.8 m/s; as high as 20 m/s; over 40 km/h needs leg muscle of 40 to 86% of body mass | Top running speed of *T. rex* | "Tyrannosaurus" | Hutchinson and Garcia 2002, *Nature* 415: 1018-1021; Hutchinson 2004, *J. Morphol.* 262: 441-461 (neither opened) | Verified on the page | The values as the two papers give them, and which paper gives 20 m/s |

### 3.2 File D3: robots and machines (for humanoid robots and machines)

Every row rests on Wikipedia alone. The checker matched all fifteen to the pages and opened no maker data sheet.

| # | Number, unit | What it describes | Page | Primary as the page cites it | Wanted |
|---|---|---|---|---|---|
| W20 | Payload 3 kg, footprint 128 mm, mass 11.2 kg, reach 500 mm (2018) | Universal Robots UR3e arm | "Universal Robots", table "Collaborative robots from Universal Robots" | maker product pages (universal-robots.com) and trade press | The maker's data sheet values, plus each joint's range (degrees) and speed (deg/s) |
| W21 | 5 kg, 149 mm, 18.4 kg, 850 mm (2008) | UR5 | same | same, for example universal-robots.com/cb3/ | as W20 |
| W22 | 5 kg, 149 mm, 20.6 kg, 850 mm (2018) | UR5e | same | same | as W20 |
| W23 | 12.5 kg, 190 mm, 33.5 kg, 1,300 mm (2018) | UR10e | same | same | as W20 |
| W24 | 16 kg, 190 mm, 33.1 kg, 900 mm (2019) | UR16e | same | same | as W20 |
| W25 | 20 kg, 245 mm, 64 kg, 1,750 mm (2022) | UR20 | same | same | as W20 |
| W26 | 30 kg, 245 mm, 64 kg, 1,300 mm (2024) | UR30 | same | same | as W20 |
| W27 | 3 kg, 11 kg, 500 mm (2015) | UR3 | same | universal-robots.com/media/240787/ur3_us.pdf (named, not opened) | as W20 |
| W28 | 10 kg, 28.9 kg, 1,300 mm (2012) | UR10 | same | same kind of PDF | as W20 |
| W29 | 28 degrees of freedom in the limbs | Boston Dynamics Atlas, 2013 DARPA version | "Atlas (robot)" | "Atlas robot interactive 3D animation" (checker's correction) | A maker or DARPA primary; joint ranges if published |
| W30 | 150 cm tall, 80 kg | Atlas "The Next Generation", 2016 | "Atlas (robot)" | Boston Dynamics video, 23 Feb 2016; bostondynamics.com/atlas | Maker's figures |
| W31 | 1.5 m, 85 kg (190 lb), hydraulic, battery, 20 degrees of freedom | Atlas, 2021 | "Atlas (robot)" | Boston Dynamics video "Inside the lab: How does Atlas work?", 17 Aug 2021 | Maker's figures; also the all-electric Atlas of 2024, for which no figures were taken |
| W32 | 25 kg (55 lb) | SpotMini, 2016 | "Boston Dynamics" | Yahoo Finance, 23 June 2016 | The current Spot's specification sheet (mass, leg length, joint ranges, speed) |
| W33 | 28 mph (12.5 m/s computed); the cited headline says 28.3 mph | Cheetah robot's galloping speed, 2012 | "Boston Dynamics" | *The Verge*, 6 Sep 2012 | DARPA or Boston Dynamics primary, and which figure is right |
| W34 | 6.5 ft tall, 9 mph, jumps 4 ft | Handle robot, 2017 | "Boston Dynamics" | CNN Money and *French Tribune*, 28 Feb 2017 | Maker's figures |
| W35 | 7-degree-of-freedom arm; boxes up to 23 kg | Stretch robot, 2021 | "Boston Dynamics" | *The Verge*, 29 Mar 2021 | Maker's figures |

### 3.3 File J22: ground vehicles (for machines on wheels and tracks)

The file says Wikipedia is "used only as a pointer to its cited primary". These rows were taken from the pages and their primaries were not opened. Rows that came from primaries read directly (NHTSA's measured inertia data, OptimumG's notes, the UK Highway Code, MathWorks) are not listed.

| # | Number, unit | What it describes | Page | Primary as the page cites it | Checker | Wanted |
|---|---|---|---|---|---|---|
| W36 | kinetic friction 0.7 with a 1.5 s perception-reaction time | Accident-reconstruction baseline for braking | "Braking distance" | Taoka 1989, *ITE Journal* 59(3): 19-21; Virginia Code § 46.2-880 | The page quotes 1.5 s as NHTSA's average; it gives no 1 s average | Taoka 1989's values; the NHTSA source of 1.5 s |
| W37 | friction 0.25 or lower | Wet or frozen asphalt | "Braking distance" | Wong, *Theory of Ground Vehicles*, 2nd ed. 1993, p. 26 (a book) | | An open primary for wet and icy road friction |
| W38 | static friction 0.5 to 0.8 | Rubber on dry asphalt | "Bicycle and motorcycle dynamics" | "its listed references" | | A primary measurement |
| W39 | 0.0010 to 0.0024 (passenger rail car about 0.0020) | Rolling resistance coefficient, steel wheel on rail | "Rolling resistance", table "Rolling resistance coefficient examples" | the table credits HowStuffWorks "tire4" and per-row references | | Each row's primary |
| W40 | 0.0022 to 0.0050 | Road bicycle tyres, 120 psi, 50 km/h, on rollers | same | same | | as W39 |
| W41 | 0.0045 to 0.0080 | Lorry (semi-trailer) tyres | same | same | | as W39 |
| W42 | 0.0065 | EU tyre label class A upper limit | same | same | | The EU regulation's own figure (a public legal text) |
| W43 | 0.0062 to 0.0150 | Car tyre measurements | same | same | | as W39 |
| W44 | 0.0100 to 0.0150 | Ordinary car tyres on concrete | same | same | | as W39 |
| W45 | 0.0385 to 0.0730 | Stage coach on a dirt road; soft snow, worst case | same | same | | as W39 |
| W46 | 0.30 | Ordinary car tyres on sand | same | same | | as W39 |
| W47 | rear brake about 0.25 g at best; front braking about 0.5 g; a recumbent up to 0.8 g if friction is 0.8; centre of gravity about 60 cm behind and 120 cm above the front contact | Bicycle braking limits | "Bicycle and motorcycle dynamics" | Wilson, *Bicycling Science*, 3rd ed. 2004 (a book) | 0.5 g added by the checker | An open primary, if any |
| W48 | 10 m/s on a 10 m radius needs 45.6 degrees | Lean in a steady turn (the formula tan θ = v²/(g r) is physics) | "Bicycle and motorcycle dynamics" | Fajans 2000, *Am. J. Phys.* 68: 654 | | Low priority: the formula needs no source; confirm the worked example only if easy |
| W49 | touring 72 to 73 degrees head angle, 43 to 60 mm trail; racing 73 to 74, 28 to 45 mm; track 75, 23.5 to 37 mm | Bicycle trail and head angle | "Bicycle and motorcycle dynamics" | Whitt and Wilson, *Bicycling Science*, 2nd ed. 1982, p. 188 (a book; corrected by the checker) | Attribution corrected | An open survey of frame geometry |
| W50 | oscillations from about 1 m/s; weave speed about 5.3 m/s; capsize speed about 8 m/s | Self-stability of the benchmark bicycle | "Bicycle and motorcycle dynamics" | Meijaard, Papadopoulos, Ruina and Schwab 2007, *Proc. R. Soc. A* 463: 1955; Kooijman et al. 2011, *Science* 332: 339 | | The values in Meijaard et al. 2007 |
| W51 | weave 0 to 4 Hz; wobble 4 to 10 Hz; rear wobble up to about 6.5 Hz; wobble unstable at 43.7 m/s in one model | Motorcycle oscillation modes | "Bicycle and motorcycle dynamics" | Cossalter, *Motorcycle Dynamics* (a book); a figure caption | | An open primary |
| W52 | from 1,255 kg (range 1,255 to 1,465 kg); 4,284 / 1,789 / 1,456 mm; wheelbase 2,636 mm | VW Golf Mk8 mass and length / width / height | "Volkswagen Golf Mk8" infobox | infobox references (not named in the file) | Ranges added | Maker's data sheet; front and rear track too (for Q29) |
| W53 | 1,380 to 1,710 kg; 4,519 to 4,535 / 1,852 to 1,900 / 1,289 to 1,303 mm; wheelbase 2,450 mm | Porsche 911 (992) | "Porsche 911 (992)" infobox | infobox references | | as W52 |
| W54 | from 3,552 lb (range to 4,048 lb); width 72.8 in; wheelbase 113.2 in; single-speed 9:1 reduction | Tesla Model 3 | "Tesla Model 3" infobox | infobox references | Range added | as W52 |
| W55 | 26,000 lb (range to 30,500 lb); 41 ft long, 102 in wide, 10 to 11 ft high; wheelbase 283.75 in; 60 ft articulated 39,000 to 45,500 lb | New Flyer Xcelsior bus, 40 ft | "New Flyer Xcelsior" infobox | infobox references | Range added | Maker's data sheet |
| W56 | 63 short tons (family 60 to 73.6 short tons); hull 26.02 ft, 12 ft wide, 8 ft high; 1,500 hp; road 45 mph governed; ground clearance 0.48 m | M1A1 Abrams tank | "M1 Abrams" infobox | Hunnicutt 2015, p. 306 (a book) | Range added | A public-domain US Army technical manual's figures; also its track and road-wheel numbers (for Q30) |
| W57 | 62.3 t (family 55.2 to 66.5 t); 10.97 m gun forward, 3.75 m wide, 3 m high; 17.8 kW/t; 70 km/h | Leopard 2A6 tank | "Leopard 2" infobox | infobox references | Range added | A primary (maker or a government) |
| W58 | snowshoes 3.5 kPa; human in flat shoes 17 kPa standing; M1 Abrams 103 kPa; adult horse 170 kPa; car 205 kPa; adult elephant 240 kPa; mountain bike 245 kPa; road bicycle 620 kPa; lorry 793 kPa; stiletto heel 3,250 kPa; "a galloping horse will exert up to 3.5 MPa" | Ground pressure of people, animals and vehicles | "Ground pressure" table | mostly uncited; Thompson, *Sustainable Landscape Construction*, p. 50 for the human; "mathscinotes" for the M1 | The file marks the table indicative | A primary for each, and above all for the horse and elephant (useful for animals' footprints) |

### 3.4 File J23: aircraft (for flying machines)

| # | Number, unit | What it describes | Page | Primary as the infobox names it | Checker | Wanted |
|---|---|---|---|---|---|---|
| W59 | empty / loaded 274 / 338 kg; span 12.29 m; wing area 47 m²; 12 hp (8.9 kW); max 48 km/h; wing loading 6.4 kg/m² | Wright Flyer, 1903 | "Wright Flyer" | not named in the file | | A primary (the Smithsonian, a Wright paper) |
| W60 | 1,691 / 2,450 lb; span 36 ft 1 in; 174 sq ft; aspect ratio 7.32; NACA 2412 modified; 160 hp; cruise 122 kt; stall 47 kt; never-exceed 163 kt; climb 721 ft/min; ceiling 13,500 ft | Cessna 172 | "Cessna 172" | "Cessna, FAA type certificate" | | The FAA type certificate data sheet (a US government document) |
| W61 | 1,201 / 2,150 lb; span 30 ft; 160 sq ft; 150 hp; max 123 kt; cruise 108 kt; stall 47 kt; climb 660 ft/min; ceiling 14,300 ft | Piper PA-28-150 Cherokee | "Piper PA-28 Cherokee" | "Piper Aircraft Owner's Handbook" | | The handbook or the FAA type certificate |
| W62 | 5,065 / 6,700 lb (gross 6,622 lb); span 36 ft 10 in; 242.1 sq ft; NACA 2213 root, 2209.4 tip; 1,470 hp; max 370 mph; climb 2,600 ft/min; ceiling 36,500 ft; 27.35 lb/sq ft | Supermarine Spitfire Mk VB | "Supermarine Spitfire" | *Spitfire: The History*; *Jane's Fighting Aircraft of WWII* (books) | Gross weight added | A primary test report, if public |
| W63 | 16,865 lb empty, 25,200 lb gross; span 95 ft; 987 sq ft; aspect ratio 9.17; 2 x 1,200 hp; max 223 kt; cruise 183 kt; stall 68 kt; climb 1,140 ft/min; ceiling 23,200 ft | Douglas DC-3 | "Douglas DC-3" | *McDonnell Douglas Aircraft since 1920* (a book) | | The FAA type certificate |
| W64 | 75,800 / 155,000 lb; span 132 ft 7 in; 1,745 sq ft; 4 x 4,590 shp; max 320 kt; cruise 292 kt; climb 1,830 ft/min; ceiling 33,000 ft | Lockheed C-130 Hercules | "Lockheed C-130 Hercules" | USAF fact sheet and others | | The USAF fact sheet (public domain) |
| W65 | 41,413 / 79,016 kg; span 35.79 m with winglets (34.32 m without); 124.6 m²; 2 x 26,300 lbf; Mach 0.82; long-range cruise 452 kt at 39,000 ft; ceiling 41,000 ft | Boeing 737-800 | "Boeing 737 Next Generation" | not named in the file | Span without winglets added | Boeing's airport planning document |
| W66 | 18,900 / 42,300 lb, gross 26,500 lb; span 32 ft 8 in; 300 sq ft; NACA 64A204; 17,155 lbf dry, 29,500 lbf with afterburner; Mach 2.05; +9 g; ceiling 50,000 ft; thrust/weight 1.095 (1.24 loaded) | F-16C Block 50/52 | "General Dynamics F-16 Fighting Falcon" | USAF fact sheet; flight manual | FIX applied: span 32 ft 8 in | The USAF fact sheet |
| W67 | 465 / 850 kg; span 26.58 m; 16.7 m²; aspect ratio 38.3; stall 65 km/h; never-exceed 280 km/h; +5.3 / -2.65 g; 33.4 kg/m² | Schleicher ASW 22 glider | "Schleicher ASW 22" | *Jane's All the World's Aircraft 1988-89* (a book) | | The type certificate data sheet (EASA or LBA) |
| W68 | house sparrow 4; herring gull 10; common tern 12; albatross 20; Wright Flyer 8.3; Boeing 747 in cruise 17.7; A380 20; Concorde 4 (take-off), 12 (Mach 0.95), 7.5 (Mach 2); helicopter at 100 kt 4.5; Cessna 172 10.9; U-2 25.6; A320 16.3, 777 19.3, 747-400 15.5 (computed by the page) | Lift-to-drag (glide) ratios | "Lift-to-drag ratio", "Examples" | Filippone (aerodyn.org); Cumpsty, *Jet Propulsion* p. 4; Orlebar, *The Concorde Story* p. 116; Leishman p. 230; CIA "U2 Developments" (the bird values' source not named) | The page's prose and list differ slightly (Concorde 7 against 7.5; 747 17 against 17.7) | Above all the birds' values from a primary (useful for birds and dragons gliding) |
| W69 | 621 kg max take-off, rotor 7.67 m; 1,451 kg, 10.16 m; 9,979 kg, 16.36 m | Robinson R22, Bell 206, UH-60 Black Hawk: mass and rotor diameter | helicopter infoboxes | R22 Pilot's Operating Handbook; *Jane's All the World's Aircraft 1976-77*; *Encyclopedia of Modern Warplanes*, US Army, a Lockheed Martin brochure | All three infoboxes' disc loadings disagree with their own mass and rotor area (UH-60 by 35%) | The primaries, and the weight each disc loading is based on |
| W70 | exponent α about 1/7 (0.143) for neutral stability | Wind speed against height near the ground (power law) | "Wind profile power law" | Touma 1977, *J. Air Pollution Control Assoc.* 27: 863-866; Peterson and Hennessey 1978, *J. Appl. Meteorol.* 17: 390-394 | | The value and its range by surface roughness, from the primaries |

### 3.5 File I3: material values (for terrain and ground colours)

| # | Number, unit | What it describes | Where in the file | Source as the file states it | Wanted |
|---|---|---|---|---|---|
| W71 | albedo 0.02 (Unreal Engine docs) / 0.04 (Lagarde) | Charcoal | Section 3, row S1 | Unreal Engine 4.27 documentation; Lagarde 2011 blog post, "from Wikipedia albedo list" | The measurement behind each value |
| W72 | 0.02 / 0.04 | Fresh asphalt | Row S2 | same | as W71 |
| W73 | 0.08 / 0.12 | Worn asphalt | Row S3 | same | as W71 |
| W74 | 0.51 / 0.55 | Fresh concrete | Row S4 | same | as W71 |
| W75 | Unreal: bare soil 0.13, green grass 0.21, desert sand 0.36, ocean ice 0.56, fresh snow 0.81; Lagarde: 0.17, 0.25, 0.40, 0.5 to 0.7, 0.80 to 0.90 | Ground albedos | Row S5 | same | **Important for terrain:** the measured primary for each (field albedo studies), with spread; see also Q44 |
| W76 | refractive index: PMMA 1.4905, polycarbonate 1.5848, PET 1.575, PLA 1.4586, polypropylene 1.492, polyurethane 1.6, PVC 1.542 | Plastics' refractive index, from which reflectance at normal incidence is computed | Section 5b, rows PI1 to PI7 | physicallybased.info (CC0), whose values "cite Wikipedia / refractiveindex.info" | Low priority: a primary measurement for each |

The file itself marks one more (car paint 0.1, "cites Wikipedia") as not data; it is not asked for.

### 3.6 File C3: birds (feather counts)

The file and the checker both mark these rows "Unverified" (secondary, Wikipedia "Flight feather", read through the MediaWiki API on 2026-11-16; and "Vestigiality in flightless birds").

| # | Number | What it describes | Wanted |
|---|---|---|---|
| W77 | typically 11 primaries (6 on the metacarpus, 5 on the phalanges), the outermost often tiny; non-passerines 9 to 11; grebes, storks and flamingos 12; ostriches 16; most passerines 10, some 9 | Primary feathers by group | A primary ornithology source by group. The US Fish and Wildlife Service Feather Atlas (public domain, delivered) gives "most 9 to 10" and "most groups 10"; the gap is mostly whether the tiny outermost one is counted |
| W78 | 6 (hummingbirds) to about 40 (some albatrosses); more than 40 non-passerine families lack the fifth secondary | Secondary feathers | as W77 (the Feather Atlas gives 9 to 25) |
| W79 | always paired; most species 6 pairs; absent in grebes and some ratites; many grouse over 12 | Tail feathers (rectrices) | as W77 |
| W80 | cassowary flight feathers reduced to 5 to 6 bare quills; most ratites have no tail feathers (the ostrich does); penguins have no distinct flight feathers | Flightless birds | as W77 |

### 3.7 Smaller items in other files the library may use

| # | File and place | What | Source as stated | Wanted |
|---|---|---|---|---|
| W81 | A8, section A8-2, moved under "Unconfirmed" | 1 to 3 in (2.5 to 7.6 cm): the best diameter of a cylinder for a full grip | Wikipedia "Grip strength", an uncited sentence (the checker: UNVERIFIABLE) | A primary ergonomics study (useful for hands holding props) |
| W82 | C4, source S11 (a licence, not a number) | The licence of Castillo-Figueroa 2020, *Zoological Studies* 59: e60, doi:10.6620/ZS.2020.59-60: "Wikipedia lists the journal licence as CC BY 4.0"; the journal says copyright belongs to the journal | Wikipedia | The licence as the journal or the article states it |

### 3.8 Other delivered files with numbers from Wikipedia, not needed by the library now

These files also take numbers from Wikipedia pages. They are about effects, light, weather, buildings, weapons and space, which the library of sample assets does not use. **Do not research them in this round** unless the rest is done; they are listed so the owner has the full picture.

| File | Subject | Wikipedia pages named |
|---|---|---|
| G4 | sky and weather | "Cumulonimbus cloud", "Cloud", "Drop (liquid)", "Hail", "Snowflake", "Fog" |
| I4 | buildings | "Brick", "Roof pitch", "Storey" (marked "citation needed") |
| I5 | fabric | "Units of textile measurement", "Ease (sewing)" (an "original research" banner) |
| J11 | fireballs and flames | "Boiling liquid expanding vapor explosion", "Flame", "Adiabatic flame temperature" |
| J12 | the colour of light | "Purkinje effect", "Flame" |
| J13 | lightning and thunder | "Lightning" |
| J14 | blast | "TNT equivalent", "Overpressure", "Gurney equations" |
| J15 | fracture | "Fracture toughness" (table "Typical values"), "Granite", "Fracture mechanics" |
| J16 | light levels | "Orders of magnitude (illuminance)", "Lux", "Daylight", "Moonlight", "Sky brightness" |
| J17 | eye adaptation | "Orders of magnitude (illuminance)", "Scotopic vision", "Mesopic vision", "Adaptation (eye)", "Purkinje effect", "Moonlight" |
| J18 | fog and smoke | "Visibility" (no inline references), "Fog" |
| J19 | drag and inertia | "Drag coefficient", "List of moments of inertia", "Tennis racket theorem", "Terminal velocity", "Density of air", "Buoyancy" |
| J20 | bounce | "Coefficient of restitution" |
| J21 | destruction | "Concrete", "Compressive strength", "Ultimate tensile strength", "Brick", "Masonry", "Sledgehammer", "Progressive collapse", "Ronan Point", "Building implosion" |
| J24 | space | "Rocketdyne F-1", "RS-25", "SpaceX Merlin", "SpaceX Raptor", "RL10", "Saturn V", "Falcon 9 Full Thrust", "G-force", "Space Shuttle thermal protection system", "Extravehicular Mobility Unit", "Lunar Roving Vehicle" |
| J25 | bullets, arrows, thrown things | cartridge infoboxes (".22 Long Rifle" and others), "English longbow", "Compound bow" |
| J26 | swords | "Longsword", "Rapier" and other sword types (the file says its weapon-type dimensions rest on Wikipedia alone) |
| J27 | loudness | "Sound pressure", table "Examples of sound pressure" |
| J29 | bullet drag, gelatin | "Ballistic coefficient", "Ballistic gelatin" (its 4 °C is uncited) |
| J31 | ion engines, re-entry | "Hall-effect thruster", "Gridded ion thruster", "Soyuz (spacecraft)" |

## 4. Numbers the delivered research marked unverified, garbled or thin

These were delivered with a warning from the researcher or the checker. The library's design leans on them. For each, the table says what would settle it.

| # | File and place | Number as delivered | What it describes | Why it is doubtful | What would settle it |
|---|---|---|---|---|---|
| U1 | C7, row 1.18 | read as "10 ± 0.3 / 0.8 ± 0.3 / 0.8 ± 0.4 / 10 ± 0.2" (mm) | Tarsus length of legs I to IV of the tarantula *Eupalaestrus weijenberghi*, males, n = 12 | The table cells were garbled in the XML of the PubMed Central page | Silva-Pereyra et al. 2019, Table 1, read from the PDF (PMC6766366, open access): the four tarsus lengths with spread. The femur, patella-plus-tibia and metatarsus rows of the same table were verified |
| U2 | C7, row 1.11 | coxa 7.20 ± 0.24, femur 7.65 ± 0.20, tibia 4.18 ± 0.23, tarsus 5.39 ± 0.14 mm (front leg); hind femur 10.6 ± 0.3 mm | Cockroach *Blaberus discoidalis* leg segments | Read by another assistant from a scanned copy; the journal blocked access | Kram, Wong and Full 1997, *J. Exp. Biol.* 200: 1919, Table 1, doi:10.1242/jeb.200.13.1919 (all legs) |
| U3 | C7, row 1.7 | front coxa 1.6 ± 0.2, trochanterofemur 14.7 ± 1.3, tibia 13.7 ± 1.4 mm; body 73.2 ± 4.6 mm | Stick insect *Carausius morosus*, n = 20 | Paywalled, not opened | Cruse 1976, Table 3, doi:10.1007/BF00606541 |
| U4 | C7, row 1.14 | coxa, femur, tibia, tarsus for three legs (mm) | Fruit fly leg model | Supplement not opened | Ramdya et al. 2017, *Nat. Commun.*, Supplementary Table 2, doi:10.1038/ncomms14494 |
| U5 | C7, row 3.14 | duty factor d = -0.531 v + 0.666 (R² 0.71); stride frequency f = 14.4 v + 0.14 Hz (R² 0.92), maximum 4.55 Hz | Tarantula *Grammostola mollicoma* (about 18 g) | Not opened (journal blocked) | Biancardi et al. 2011, *J. Exp. Biol.*, doi:10.1242/jeb.057471 |
| U6 | C7, rows 2.23 and 2.24 | tripod gait 0.44 to 1.0 m/s, stride frequency up to about 27 Hz; preferred speeds and a gait change at Froude about 0.4 | Cockroach gaits | Not opened; inconsistent DOIs for Bender et al. 2011 | Full and Tu 1991, *J. Exp. Biol.* 156: 215, doi:10.1242/jeb.156.1.215; Bender et al. 2011 (PMC3102013) |
| U7 | C6, row 3.8 | femur 14.2 ± 0.1%, tibia 12.4 ± 0.1%, foot 7.9 ± 0.1%, fourth toe 12.6 ± 0.2% of hind-limb span; tail 21.5 ± 1.1 cm against SVL 12.2 cm | Desert iguana *Dipsosaurus dorsalis*, n = 4 | Only copy on a third-party host; the journal blocked | Fieler and Jayne 1998, *J. Exp. Biol.* 201: 609, p. 610, doi:10.1242/jeb.201.4.609 |
| U8 | C6, rows 1.27 to 1.29 | duty factor (%) = 73.1 - 21.0 V + 2.78 V²; stride fits; *Varanus exanthematicus* walk duty 69 ± 1.7%, run 41 ± 0.9% | Lizard gaits | Not opened | Fieler and Jayne 1998, Fig. 7; McElroy, Hickey and Reilly 2008, Table 1, doi:10.1242/jeb.015503 |
| U9 | C6, rows 2.13 and 2.14 | duty 0.74 / 0.69 (sprawl) and 0.71 / 0.70 (high walk) at 0.074 and 0.146 m/s; femoral adduction about 40 (sprawl) and 55 (high walk) degrees | American alligator | Read from an archived copy by another assistant; not confirmed | Reilly and Elias 1998, *J. Exp. Biol.* 201: 2559, Table 1 and Fig. 4, doi:10.1242/jeb.201.18.2559 |
| U10 | C6, row 4.14 | duty about 0.8, "sometimes two feet down" | Tortoise walking | Not opened | Jayes and Alexander 1980, doi:10.1111/j.1469-7998.1980.tb01464.x |
| U11 | C1, section 1.7 (cattle: Matolcsi) and 1.8 (pig: Teichert), and "Sources disagree" | Matolcsi 1970 factors: humerus 4.14, radius 4.30, femur 3.53, tibia 3.45, metacarpus 6.0, metatarsus 5.45; Teichert 1969 factors: humerus 4.0, radius 5.3, tibia 3.8, MC III 10.5, MC IV 10.3, astragalus 18.5, calcaneus 9.7 | Withers height as a multiple of a bone's length (cattle; pigs) | From search summaries only; and one cited author's own numbers (Gidney) imply a femur factor of about 3.22, not 3.53 | Matolcsi 1970, *Z. Tierzüchtung und Züchtungsbiologie* 87, doi:10.1111/j.1439-0388.1970.tb01330.x; Teichert 1969; von den Driesch and Boessneck 1974 |
| U12 | C1, foot posture rows | digitigrade, unguligrade, plantigrade by family | Which way each species stands on its foot | The category scheme is verified; each species' row is in a supplement not opened | Kubo et al. 2019, its supplementary species table |
| U13 | C1, Turopolje pig | femur 202.0 mm, humerus 194.9 mm, n = 19; spreads 7.11 and 4.31 | A pig breed's bones | The spreads appear only in a search summary | Đikić et al. 2007, *Poljoprivreda* 13(1): 172-174, full text |
| U14 | C2, rows marked "U" | cheetah and horse gait figures | Galloping footfalls | Taken from a secondary website summarising Hildebrand 1959 (paywalled) | Hildebrand 1959, *J. Mammal.* 40(4): 481-495, doi:10.2307/1376265; Hildebrand 1961 |
| U15 | C4, section 3.1 | per-family coefficients of bat wing shape against mass; "the heaviest bats are about 1.5 kg" | Bat wing allometry | Transcribed by another assistant from a copy that could not be reached | Norberg and Rayner 1987, *Phil. Trans. R. Soc. B*, Tables 1 and 4 |
| U16 | C4, Castillo-Figueroa 2020 | digit length minima (LD3, LD4) | Bat wing digits | The text gives them to *Eptesicus fuscus*, the table to *E. furinalis*; and the licence is unconfirmed (W82) | The official PDF of Table 1 and the supplement S1 |
| U17 | C3, section 8 | regression coefficients to predict mass, wing length and span from single bones; mean brachial index per order; aspect ratio of 54 taxa | Bird wing scaling | Paywalled or blocked | Nudds 2007; Nudds, Dyke and Rayner 2004, electronic appendix; Wang and Clarke 2015, Table S4 |
| U18 | C5, section 4 | per-species wavelength, amplitude and frequency from Bainbridge 1958, Videler and Hess 1984, Gillis 1998 | Fish body wave | Abstracts only (the journal blocked) | The papers' tables; or DS3 |
| U19 | E1, section 5 | beech branch angle 30.7 degrees in pure stands against 54.1 in mixed stands; coconut 8 to 18 m tall, 20 to 32 cm trunk, 24 to 35 fronds; baobab zone mean heights 13.5 to 21.2 m, crowns 14.2 to 17.0 m | Tree forms | From search summaries only | *Can. J. For. Res.* 2021 (beech); *Forest Ecosystems* 2024, doi:10.1016/j.fecs.2024.100279; Assogbadjo et al. 2006 (baobab); a coconut morphometric study |
| U20 | E1 (from the review) | one oak height | Oak height against diameter | It is a 5% quantile, not an average | The same source's mean |
| U21 | F3, section 9, item 7 | dune albedo 0.426 ± 0.043; "lithosols and rock" 0.302; volcanic rock 0.174; dry dune sand 0.266; wet soil albedo 32 to 58% below dry (mean 45%); albedo = 0.069 x Munsell value - 0.114; wet 0.14 against dry 0.305 | Ground albedo, dry and wet | Reported by another assistant; not checked against the primaries | Tsvetsinskaya et al. 2002 (MODIS); Post et al. 2000, *Soil Sci. Soc. Am. J.*; Idso 1974/1975 |
| U22 | G3, section 8 | Jerlov water types; Forel-Ule colour classes; a 79 m Secchi depth credited to "Berman et al. 1985, Weddell Sea" | Water colour and clarity | Not opened; the 79 m is reported to be Gieskes et al. 1987, and Berman 1985 to be 53 m in the eastern Mediterranean | Jerlov 1976; Solonenko and Mobley 2015; Wernand 2010; Novoa 2013; the two Secchi papers |
| U23 | G2, section 9 | Manning's roughness ranges (Chow 1959 Table 5-6, a book); brink depth 0.715 of critical depth (Rouse 1936); antidune and standing-wave Froude bands | River flow | Not opened | Barnes 1967, USGS Water-Supply Paper 1849 (public domain); Rouse 1936; Kennedy |
| U24 | J23, section J23-5 | rotor figure of merit 0.75 | How efficient a helicopter rotor is against ideal | Moved under "Unconfirmed" by the checker | Leishman, or any open rotor study, for a typical range |
| U25 | A2 and A1 (from the review) | head height as a share of stature by age (A2); segment ratios read off a redrawn figure (A1-4); segment values from a course's copy (A4-5) | People's proportions | Secondary | The primary growth studies and de Leva 1996 / Dempster 1955 originals |

## 5. What is not being asked for

Please do not spend time on these.
- **A dragon's six limbs, and where its wings join its body.** No animal has six limbs; the engine sets this itself from the nearest real animals and marks it as its own. (Bats', birds' and pterosaurs' real measurements, in part 2, are wanted.)
- **The wasting of the undead** (how much flesh is left on a skeleton). There is no measurement of this; it is the engine's own.
- **Fantasy creatures as such** (griffins, wyverns, aliens): the engine builds them from real animals' and people's numbers. Only the real numbers are asked for.
- **What it costs to draw grass and trees** on a computer: the engine measures that on its own reference machine.
- **Faces and hair**: another line of work handles them.
- **Anything already delivered and verified**: for example limb bone lengths of the wolf, lion, bears and horse; a dog's footfalls; human segment lengths and girths from the US Army's ANSUR II survey; wave spectra. Only the gaps above.
- **The Wikipedia-sourced numbers of section 3.8** (effects, light, weather, weapons, space), unless everything else is done.
- **Whole data sets, pictures, or text from books in copyright.** Report values, not files.
- **The lists of controls in commercial games' character creators.**
- **Motion-capture data**: which sets may be used was settled in an earlier round.

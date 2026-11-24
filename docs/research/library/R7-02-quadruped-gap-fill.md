# R7-02 Filling DS1 gaps: dog, elephant, rabbit and rat bones, and quadruped-set body mass

Round 7 library research. Access date for every page and file read here: **2026-11-24**. DS1 (Serio et al. 2024, Data_1.csv) has no dog, elephant, rabbit or rat, and no body-mass column. This file sources those gaps from open papers and open data, under the same return rules as the brief (section 0). No data set is copied whole.

## Sources (licence said plainly)

| ID | Source | Free to read? | Licence | What I read |
|---|---|---|---|---|
| S1 | Humphries A, Shaheen AF, Gomez Alvarez CB (2020). "Biomechanical comparison of standing posture and during trot between German shepherd and Labrador retriever dogs." *PLOS ONE* 15(10): e0239832. doi:10.1371/journal.pone.0239832. PMC7531786 | Yes | **CC BY 4.0** | Full PDF from journals.plos.org; Table S5 (file `pone.0239832.s005`, a Word document served as the article's supporting file) |
| S2 | Singh S, Kumar R (2023). "Comparative evaluation of radiographic morphometry of femur and hip joint in healthy Labrador retriever and German shepherd breeds of dog." *The Pharma Innovation Journal* SP-12(11): 241-245. https://www.thepharmajournal.com/archives/2023/vol12issue11S/PartD/S-12-10-363-735.pdf | Yes (free PDF) | PDF shows **© 2023 TPI**. No Creative Commons statement found in the PDF. Treat as all rights reserved; numbers reported as facts with this citation | Full PDF, Table 3 p. 244 |
| S3 | Thuller MAO, Jangarelli M, Couto DM, Araujo AHB (2015). "Sexual dimorphism of Labrador Retriever dogs by morphometry." *Bioscience Journal* (Uberlandia) 31(5): 1475-1487. doi:10.14393/BJ-v31n5a2015-26496. PDF: seer.ufu.br/index.php/biosciencejournal/article/download/26496/17137/126859 | Yes | **CC BY 4.0** (journal page `DC.Rights` = creativecommons.org/licenses/by/4.0; confirmed on the article page, 2026-11-24) | Full PDF, Table 2; methods |
| S4 | Larramendi A (2016). "Shoulder height, body mass, and shape of proboscideans." *Acta Palaeontologica Polonica* 61(3): 537-574. doi:10.4202/app.00136.2014. SOM: https://app.pan.pl/SOM/app61-Larramendi_SOM.pdf | Yes | **CC BY 4.0** (article: "open-access article distributed under the terms of the Creative Commons Attribution License"; HTML points to CC BY 4.0). Copyright 2016 A. Larramendi | Full PDF and SOM Table 1 |
| S5 | Gurbuz I, Demiraslan Y, Aslan K (2015). "Morphometric analysis of the skull of New Zealand rabbit (*Oryctolagus cuniculus* L.) according to gender." *ARC Journal of Animal and Veterinary Sciences* 1(2): 27-32. https://arcjournals.org/pdfs/ajavs/v1-i2/4.pdf | Yes (free PDF) | No licence statement found in the PDF. Treat as all rights reserved; numbers reported as facts with this citation | Full PDF, Table 1 and methods (L1, L2) |
| S6 | Islam MM, Farag E, Mahmoudi A, Hassan MM, Atta M, Mostafavi E, Alnager IA, Farrag HA, Eljack GEA, Bansal D, Haroun M, Abdeen R, Al-Romaihi H, Al-Zeyara AA, Almalki SA, Mkhize-Kwitshana Z (2021). "Morphometric study of *Mus musculus*, *Rattus norvegicus*, and *Rattus rattus* in Qatar." *Animals* 11(8): 2162. doi:10.3390/ani11082162. PMC8388387 | Yes | **CC BY 4.0** (MDPI licence paragraph) | Full text via Europe PMC XML; body and cranial tables |
| S7 | Ahmed A, Koichev B, Andreev R, Stepanov I, Savev P (2024). "Somatometric characteristics of the grey wolf (*Canis lupus*) in Bulgaria." *Silva Balcanica* 25(1): 101-107. doi:10.3897/silvabalcanica.25.e121340 | Yes | **CC BY 4.0** (article: "open access article distributed under the terms of the Creative Commons Attribution License (CC BY 4.0)") | Full PDF, Table 1 p. 104 |
| S8 | Jones KE, Bielby J, Cardillo M, Fritz SA, O'Dell J, Orme CDL, Safi K, Sechrest W, Boakes EH, Carbone C, Connolly C, Cutts MJ, Foster JK, Grenyer R, Habib M, Plaster CA, Price SA, Rigby EA, Rist J, Teacher A, Bininda-Emonds ORP, Gittleman JL, Mace GM, Purvis A (2009). "PanTHERIA: a species-level database of life history, ecology, and geography of extant and recently extinct mammals." *Ecology* 90: 2648. Ecological Archives E090-184. File: `PanTHERIA_1-0_WR05_Aug2008.txt` (https://esapubs.org/archive/ecol/E090/184/) | Yes | Metadata: **Copyright restrictions: None. Proprietary restrictions: None.** Free for use; cite the paper and the archive | The WR05 file, column `5-1_AdultBodyMass_g` |

Where two sources give a mass, the primary paper with n and spread is preferred; PanTHERIA is the consistent compiled figure when no such paper was opened.

---

## A. Bones DS1 does not hold (humerus, radius, femur, tibia, greatest skull length, mm)

### Dog (medium: Labrador Retriever and German Shepherd; small and giant breeds)

**Small and giant breeds.** No opened open-access source gave humerus, radius, femur, tibia and greatest skull length in mm, with n and spread, for a small breed (for example Chihuahua or Yorkshire terrier) or a giant breed (for example Great Dane, mastiff or St Bernard). See "Blocked downloads" for a paywalled German Shepherd osteology paper, and "Not found".

**Medium breeds, femur in mm (radiograph, not dry-bone greatest length).** S2, Table 3, "Femoral Length (mm)". Client-owned healthy dogs, India; hip-extended ventrodorsal radiograph under anaesthesia; dogs with Norberg angle over 105 degrees and no hip pathology. **Landmark (quote of the method):** "proximally from the femoral neck midpoint to the intercondylar fossa distally." This is shorter than osteological greatest femoral length (which includes the femoral head). n in each weight class is as printed; the paper also says 12 Labradors and 12 German Shepherds were enrolled, and "in case of body weight additional animals were added", so the weight-class n values sum to more than 12 a breed.

| Q | Breed | Body-weight class (kg) | n | Femur length (mm), mean ± SD | Notes |
|---|---|---|---:|---|---|
| DS1-gap | Labrador Retriever | <25 | 4 | 179.07 ± 6.92 | S2 Table 3 |
| DS1-gap | Labrador Retriever | 25-35 | 5 | 177.27 ± 6.83 | S2 Table 3 |
| DS1-gap | Labrador Retriever | >35 | 4 | 189.89 ± 8.81 | S2 Table 3 |
| DS1-gap | German Shepherd | <25 | 6 | 197.59 ± 10.80 | S2 Table 3 |
| DS1-gap | German Shepherd | 25-35 | 5 | 211.26 ± 13.15 | S2 Table 3 |
| DS1-gap | German Shepherd | >35 | 4 | 221.77 ± 8.32 | S2 Table 3 |

**Medium breeds, humerus, radius, femur and tibia as a share of withers height, not mm.** S1, live dogs, UK; 12 Labrador Retrievers (6 male, 6 female) and 12 German Shepherds (5 male, 7 female); spherical markers on anatomical landmarks, lengths "expressed as a percentage of the height at the withers" (Table S5 caption). The main text pools the two sides (humerus, radius, tibia only). Table S5 gives left and right separately, including femur, which the main text does not summarise. I did not find a mean withers height in millimetres in the main PDF, so I have **not** converted these shares to mm (that conversion would be computed, and it needs the withers heights, preferably per dog). The OSF data deposit is listed under blocked downloads.

| Q | Breed | Bone (S5 label) | Side | n | Mean (% of withers height) | SD (%) | Source |
|---|---|---|---|---:|---:|---:|---|
| DS1-gap | Labrador Retriever | Left humerus | L | 12 | 29.95 | 1.99 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Right humerus | R | 12 | 31.09 | 2.80 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Left radius | L | 12 | 36.58 | 2.41 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Right radius | R | 12 | 37.32 | 2.32 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Left femur | L | 12 | 34.85 | 4.20 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Right femur | R | 12 | 36.08 | 4.28 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Left tibia | L | 12 | 34.52 | 3.03 | S1 Table S5 |
| DS1-gap | Labrador Retriever | Right tibia | R | 12 | 32.60 | 2.54 | S1 Table S5 |
| DS1-gap | German Shepherd | Left humerus | L | 12 | 25.55 | 2.92 | S1 Table S5 |
| DS1-gap | German Shepherd | Right humerus | R | 12 | 26.53 | 3.10 | S1 Table S5 |
| DS1-gap | German Shepherd | Left radius | L | 12 | 34.53 | 2.04 | S1 Table S5 |
| DS1-gap | German Shepherd | Right radius | R | 12 | 34.26 | 1.40 | S1 Table S5 |
| DS1-gap | German Shepherd | Left femur | L | 12 | 31.84 | 2.58 | S1 Table S5 |
| DS1-gap | German Shepherd | Right femur | R | 12 | 32.92 | 2.64 | S1 Table S5 |
| DS1-gap | German Shepherd | Left tibia | L | 12 | 30.97 | 2.46 | S1 Table S5 |
| DS1-gap | German Shepherd | Right tibia | R | 12 | 30.48 | 2.07 | S1 Table S5 |

The main-text humerus, radius and tibia figures are the average of the two side means in S5 (**computed** check: Labrador humerus (29.95+31.09)/2 = 30.52, matching 30.52%; German Shepherd humerus (25.55+26.53)/2 = 26.04, matching 26.04%; Labrador tibia 33.56, matching 33.6%; German Shepherd tibia 30.725, matching 30.7%). The main-text SDs are not the average of the two side SDs.

**Labrador, live skull length (not a dry skull).** S3, Table 2, "Length of skull (CCB)", centimetres, standing dogs, caliper or ruler (±0.1 cm). The PDF names the trait but does not spell out the two landmarks beyond the abbreviation CCB. n = 27 adult males, 47 adult females, Brazil, commercial animals. Converted to mm below (**computed**, ×10).

| Q | Breed | Sex | n | Length of skull, CCB | In mm (**computed**) | Source |
|---|---|---|---:|---|---|---|
| DS1-gap | Labrador Retriever | Male | 27 | 24.15 ± 1.40 cm (SD) | 241.5 ± 14.0 mm | S3 Table 2 |
| DS1-gap | Labrador Retriever | Female | 47 | 22.40 ± 0.95 cm (SD) | 224.0 ± 9.5 mm | S3 Table 2 |

German Shepherd skull length in mm: **not found** in the opened sources.

### African elephant (*Loxodonta africana*)

S4 SOM Table 1 lists two named *L. africana* skeletons, with long-bone **length in mm** used to estimate mass from Christiansen (2004) equations. These are individual specimens, not a species mean. There is no radius column (ulna is given instead). I have not pooled the two animals: Jumbo is a famous large male; NMNS002990-F002715 is much smaller (estimated mass in the same table 3,465 kg against Jumbo's 7,640 kg; those masses are allometric estimates, not scale weights).

| Q | Specimen | Humerus (mm) | Radius (mm) | Femur (mm) | Tibia (mm) | Greatest skull length | Source |
|---|---|---:|---|---:|---:|---|---|
| DS1-gap | *L. africana*, Jumbo | 1091 | not given (ulna 1037 mm) | 1258 | 788 | not in this table | S4 SOM Table 1 |
| DS1-gap | *L. africana*, NMNS002990-F002715 | 846 | not given (ulna 750 mm) | 980 | 585 | not in this table | S4 SOM Table 1 |

No *Elephas maximus* row with these bone lengths was found in the same SOM table. A species mean with n and SD, radius in mm, and greatest skull length: **not found** in the opened sources. See blocked downloads for a veterinary thesis on African elephant skulls, and for Chapman et al. 2016 Dryad (mass, not bones).

### Rabbit (*Oryctolagus cuniculus*, New Zealand breed, adult dry skulls)

S5, n = 10 female and 10 male, electronic caliper, mm. **L1 skull length** is defined as acrocranion to prosthion (this is a standard greatest skull length). **L2** is condylobasal length (aboral borders of the occipital condyles, as defined in the paper). Limb bones are not in this paper.

| Q | Sample | Quantity | n | Mean ± SD (mm) | Source |
|---|---|---|---:|---|---|
| DS1-gap | New Zealand rabbit, female | Greatest skull length (L1, acrocranion-prosthion) | 10 | 94.49 ± 3.95 | S5 Table 1 |
| DS1-gap | New Zealand rabbit, male | Greatest skull length (L1) | 10 | 91.43 ± 2.29 | S5 Table 1 |
| DS1-gap | New Zealand rabbit, female | Condylobasal length (L2) | 10 | 84.74 ± 3.81 | S5 Table 1 |
| DS1-gap | New Zealand rabbit, male | Condylobasal length (L2) | 10 | 82.36 ± 2.14 | S5 Table 1 |

Humerus, radius, femur and tibia in mm: **not in the opened files.** Two likely sources were blocked (Volf 2023 in *Lynx*; Ajayi et al. 2012 in the *Italian Journal of Anatomy and Embryology*). See blocked downloads.

### Brown rat (*Rattus norvegicus*)

S6, commensal rats trapped in Qatar. Body measurements n = 120 (62 female, 58 male). Cranial measurements n = 86 (38 female, 48 male); not every carcass had a skull. Units: body weight in grams, linear measures in millimetres (table footnotes). **General cranial length** is the paper's closest trait to greatest skull length (dry or prepared skull, as a cranial morphometric series). Limb long bones are not in this paper.

| Q | Sample | Quantity | n | Mean ± SD | Source |
|---|---|---|---:|---|---|
| DS1-gap | *R. norvegicus*, Qatar, all in the cranial sample | General cranial length (mm) | 86 | 46.8 ± 4.1 | S6 cranial table |
| DS1-gap | Female | General cranial length (mm) | 38 | 46.3 ± 3.7 | S6 sex-split cranial table |
| DS1-gap | Male | General cranial length (mm) | 48 | 47.1 ± 4.3 | S6 sex-split cranial table |
| DS1-gap | All in the cranial sample | Condylobasal length (mm) | 86 | 45.2 ± 4.1 | S6 cranial table |
| DS1-gap | Female | Condylobasal length (mm) | 38 | 44.8 ± 3.6 | S6 sex-split cranial table |
| DS1-gap | Male | Condylobasal length (mm) | 48 | 45.5 ± 4.4 | S6 sex-split cranial table |

The n = 120 body sample includes whatever young animals were among the 120 brown rats (10 of the 148 rodents of all three species were classed young). Humerus, radius, femur and tibia in mm: **not found** in the opened sources.

---

## B. Body mass for the quadruped set

Primary papers with n and spread first. PanTHERIA (S8) is a compiled adult body mass in grams, with no sample size and no spread in the file. Kilograms below from PanTHERIA are **computed** (grams / 1000).

| Q | Species (and sample, if a primary paper) | n | Mass | Spread | Licence / access | Source |
|---|---|---:|---|---|---|---|
| Mass | Dog, Labrador Retriever, UK, 6 M + 6 F, live, S1 | 12 | 29.3 kg | SD 4.5 kg | CC BY 4.0 | S1 Results |
| Mass | Dog, German Shepherd, UK, 5 M + 7 F, live, S1 | 12 | 36.2 kg | SD 4.1 kg | CC BY 4.0 | S1 Results |
| Mass | Grey wolf, Bulgaria, adult males, hunted carcasses, S7 | 60 | 37.8 kg | SD 7.3 kg; range 15-57 kg | CC BY 4.0 | S7 Table 1, parameter P |
| Mass | Grey wolf, Bulgaria, adult females, S7 | 29 | 30.4 kg | SD 7 kg; range 15-45 kg | CC BY 4.0 | S7 Table 1, parameter P |
| Mass | Grey wolf, compiled adult | - | 31.76 kg (**computed** from 31,756.51 g) | none in file | no copyright or proprietary restriction | S8 `Canis lupus` |
| Mass | Domestic cat, compiled adult | - | 2.88 kg (**computed** from 2,884.80 g) | none in file | S8 | S8 `Felis catus` |
| Mass | Lion, compiled adult | - | 158.62 kg (**computed** from 158,623.93 g) | none in file | S8 | S8 `Panthera leo` |
| Mass | Tiger, compiled adult | - | 161.91 kg (**computed** from 161,914.66 g) | none in file | S8 | S8 `Panthera tigris` |
| Mass | Leopard, compiled adult | - | 52.40 kg (**computed** from 52,399.99 g) | none in file | S8 | S8 `Panthera pardus` |
| Mass | Horse, compiled adult | - | 403.60 kg (**computed** from 403,598.53 g) | none in file | S8 | S8 `Equus caballus` |
| Mass | Red deer, compiled adult | - | 240.87 kg (**computed** from 240,867.13 g) | none in file | S8 | S8 `Cervus elaphus` |
| Mass | White-tailed deer, compiled adult | - | 75.90 kg (**computed** from 75,901.25 g) | none in file | S8 | S8 `Odocoileus virginianus` |
| Mass | Cattle, compiled adult | - | 618.64 kg (**computed** from 618,642.42 g) | none in file | S8 | S8 `Bos taurus` |
| Mass | Pig / wild boar, compiled adult | - | 84.47 kg (**computed** from 84,471.54 g) | none in file | S8 | S8 `Sus scrofa` |
| Mass | Brown bear, compiled adult | - | 196.29 kg (**computed** from 196,287.50 g) | none in file | S8 | S8 `Ursus arctos` |
| Mass | American black bear, compiled adult | - | 110.50 kg (**computed** from 110,500.00 g) | none in file | S8 | S8 `Ursus americanus` |
| Mass | African elephant, compiled adult | - | 3824.54 kg (**computed** from 3,824,539.93 g) | none in file | S8 | S8 `Loxodonta africana` |
| Mass | Asian elephant, compiled adult | - | 3269.79 kg (**computed** from 3,269,794.34 g) | none in file | S8 | S8 `Elephas maximus` |
| Mass | European rabbit, compiled adult | - | 1.59 kg (**computed** from 1,590.57 g) | none in file | S8 | S8 `Oryctolagus cuniculus` |
| Mass | Brown rat, Qatar, S6 (mostly adult commensal animals; see note above on young) | 120 | 264.3 g (0.2643 kg, **computed**) | SD 87.5 g (0.0875 kg, **computed**) | CC BY 4.0 | S6 external-body table |
| Mass | Brown rat, Qatar, female / male | 62 / 58 | 260.6 / 268.2 g | SD 76.1 / 98.8 g | CC BY 4.0 | S6 sex-split body table |
| Mass | Brown rat, compiled adult | - | 0.283 kg (**computed** from 282.89 g) | none in file | S8 | S8 `Rattus norvegicus` |

PanTHERIA has no row for *Canis familiaris*. Horse, cattle and pig figures are species-level compiled adults and do not name a breed or a domestic-versus-wild split.

---

## Not found (in opened sources)

- Dog: a small breed and a giant breed, with humerus, radius, femur, tibia and greatest skull length in mm and n and spread.
- Dog: humerus, radius, tibia (and a greatest-length femur) in mm; only shares of withers height (S1) and a radiographic neck-to-intercondylar femur (S2).
- German Shepherd: skull length.
- African or Asian elephant: radius; greatest skull length; a species mean and spread for the long bones (only two named African skeletons in S4).
- Rabbit: humerus, radius, femur, tibia (two likely papers were blocked).
- Brown rat: humerus, radius, femur, tibia.
- Body mass with n and spread from an opened primary paper for: domestic cat (a typical adult, not an obese clinical sample), lion, tiger, leopard, horse (named breed), red deer, white-tailed deer, cattle (named breed), domestic pig, brown bear, black bear, elephant (scale mass, not an allometric estimate), and rabbit.
- Landmarks for Thuller's live "Length of skull (CCB)" beyond the abbreviation.

## Sources disagree

- **Wolf mass.** S7 (Bulgaria, adults, scale mass) gives males 37.8 ± 7.3 kg (n=60) and females 30.4 ± 7 kg (n=29). PanTHERIA's single compiled figure for *Canis lupus* is 31.76 kg, which sits between those two means and has no n or sex split. Prefer S7 for a measured sample; prefer PanTHERIA only as a species-level compiled number.
- **Rat mass.** S6 264.3 ± 87.5 g (n=120, Qatar, mixed sex, a few young possibly included) versus PanTHERIA 282.89 g. Prefer S6 for that population.
- **Elephant mass.** Jumbo's allometric estimate in S4 SOM (7,640 kg) is far above PanTHERIA's compiled adult *L. africana* (3,824.54 kg). They answer different questions (one huge named male, versus a species compiled mean).
- **Dog femur.** S2 radiographic neck-midpoint-to-intercondylar lengths (about 177-190 mm in Labradors, 198-222 mm in German Shepherds, by weight class) are not the same quantity as S1's live marker-to-marker femur as a share of withers height. Do not treat them as one measurement.
- **Humphries main text versus Table S5.** The main-text bone shares are the averages of the left and right means in S5 (check above). The main-text SDs are not recovered by averaging the two side SDs. Use S5 when a side is needed.

## Blocked downloads (exact URL and file, for a human)

I did not bypass a human check or a sign-in. These are worth a browser download.

1. **Humphries OSF data** (needed to turn S1 bone shares into mm: withers height, or bone length, per dog). Project: https://osf.io/pv4uj and https://doi.org/10.17605/OSF.IO/PV4UJ (title: "German shepherd and Labrador retriever dogs biomechanical datasets"). The public API from this box listed an empty storage folder, so I could not record individual file names. Please download whatever data files are on that project page.
2. **Volf J (2023).** "Morfometrika dlouhych kosti koncetin zajicovitych (Lagomorpha: Leporidae)." *Lynx*, n.s. (Praha) 54: 193-200. doi:10.37520/lynx.2023.014. Issue page: https://publikace.nm.cz/periodicke-publikace/lynx-nova-serie/54-1. I could not get the PDF (no public direct file URL from the pages I opened). This is the most likely open source for rabbit humerus, femur, tibia and a skull length together.
3. **Ajayi IE, Shawulu JC, Ahmed S, Adah BMJ (2012).** "Osteomorphometry of the bones of the thigh, crus and foot in the New Zealand white rabbit (*Oryctolagus cuniculus*)." *Italian Journal of Anatomy and Embryology*. https://oajournals.fupress.net/index.php/ijae/article/view/1131. The site returned a "Making sure you're not a bot" check. Abstract (not used as a table here) mentions femur and tibia lengths; the PDF is needed to take n, spread and licence from the full text.
4. **Belhaoues F. and colleagues (2020).** "Sexual dimorphism in limb long bones of the German Shepherd Dog." *Anatomia, Histologia, Embryologia*. doi:10.1111/ahe.12550. Wiley page; data "available from the corresponding author upon reasonable request." Likely paywalled. Would be a dry-bone humerus, radius, femur and tibia series (n=25, 12 male, 13 female) if a legal copy can be had.
5. **Chapman et al. (2016) Dryad** for Asian elephant body size, doi:10.5061/dryad.p9r4k (paper: *PLOS ONE* 11(3): e0150533, CC BY 4.0). Same Dryad "confirm you are human" or API-token rule as DS1. Not read.
6. **Schopf B (2010).** "Beitrage zur Osteologie und Osteometrie des Schadels des Afrikanischen Elefanten." Diplomarbeit, Veterinarmedizinische Universitat Wien. https://www.vetmeduni.ac.at/hochschulschriften/diplomarbeiten/AC08346112.pdf. Not downloaded here. A possible source for African elephant skull lengths; licence and whether a browser can fetch it were not confirmed.
7. **NCBI PMC PDF links** (for example https://www.ncbi.nlm.nih.gov/pmc/articles/PMC7531786/pdf/pone.0239832.pdf) returned a reCAPTCHA page to a script. This did not block the work: the same articles were read from the PLOS PDF, the PLOS S5 file, or Europe PMC full-text XML.

Europe PMC XML for S6 did not need a sign-in. PanTHERIA and the APP SOM PDF downloaded without a check.

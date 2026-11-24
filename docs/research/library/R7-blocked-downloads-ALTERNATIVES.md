# Blocked downloads: free alternatives found (round 7, R7-5)

Compiled 2026-11-24. One row per item in `blocked-downloads-ALL.md` (A 46, B 23, C 9 = 78). Routes tried for each item, in order: Unpaywall, OpenAlex, Semantic Scholar, CORE, PMC open-data bucket and Europe PMC, author and university repositories, preprints, archive.org and the Wayback availability API, HathiTrust and Google Books, Zenodo, Dryad API, figshare API, Crossref links, JEB article-pdf, OSF API. No sign-ins, captchas, pirate sites or unofficial uploads were used (a third-party copy of Mazzetta 2004 was found and deliberately not used).

Status key: **Exact** = the cited item (or its exact data) obtained free; **Substitute** = a different free source measuring the same quantity; **Abstract** = only the abstract readable; **Blocked** = no free copy obtained. Numbers are in `library/R7-5-alt-sources.md`; files are in `sources/`.

## Counts
| Status | A | B | C | Total |
|---|---|---|---|---|
| Exact | 20 | 0 | 0 | 20 |
| Substitute | 5 | 1 | 1 | 7 |
| Abstract only | 5 | 0 | 0 | 5 |
| Still blocked | 16 | 22 | 8 | 46 |
| Total | 46 | 23 | 9 | 78 |

## Group A
| # | Item | Status | Working URL (free) | Rows closed |
|---|---|---|---|---|
| A1 | Fieler & Jayne 1998 | Abstract | https://journals.biologists.com/jeb/article-pdf/201/4/609/3398846/jexbio_201_4_609.pdf (scan; script gets abstract only) | none fully (Q9, U7, U8 polynomial open) |
| A2 | Reilly & Elias 1998 | Abstract | https://journals.biologists.com/jeb/article-pdf/201/18/2559/3576080/jexbio_201_18_2559.pdf | none (Q11, U9 open) |
| A3 | Biancardi 2011 | Exact | https://journals.biologists.com/jeb/article-pdf/214/20/3433/1275821/3433.pdf | U5, Q28 |
| A4 | Kram 1997 | Abstract | https://journals.biologists.com/jeb/article-pdf/200/13/1919/3411811/jexbio_200_13_1919.pdf | none (U2, Q27 open) |
| A5 | Peng 2025 (year corrected from 2024) | Exact | https://biblio.ugent.be (published PDF, CC BY; doi:10.1016/j.fecs.2024.100279) | U19 accuracy part; the 30.7° / 54.1° means are not in this paper |
| A6 | Böhmer 2019 figshare data | Exact (same tables via PMC ESM) | https://pmc-oa-opendata.s3.amazonaws.com/PMC6549945.1/rsos181588supp1.docx | Q12, Q13 (counts by group) |
| A7 | Shine 2023 | Exact article; Dryad data blocked | https://pmc.ncbi.nlm.nih.gov/articles/PMC10716647/ (bucket copy) | Q22/Q23 sample sizes only |
| A8 | Di Santo PNAS Dataset S2 | Substitute (same study's raw data, A9) | https://zenodo.org/records/5646507 | Q25, DS3 |
| A9 | Di Santo Dryad | Exact (Zenodo mirror) | https://zenodo.org/records/5646507 | Q25, DS3 |
| A10 | Tobalske 1999; Tobalske & Dial 1996 | Abstract | https://journals.biologists.com/jeb/article-pdf/202/13/1725/3595830/jexbio_202_13_1725.pdf and https://journals.biologists.com/jeb/article-pdf/199/2/263/3389400/jexbio_199_2_263.pdf | none (Q15 open) |
| A11 | Full & Tu 1991 | Abstract (enough for U6) | https://journals.biologists.com/jeb/article-pdf/156/1/215/2579388/jexbio_156_1_215.pdf | U6 (with A12) |
| A12 | Bender 2011 | Exact | https://journals.biologists.com/jeb/article-pdf/214/12/2057/1273625/2057.pdf | U6 |
| A13 | McElroy 2008 | Exact | https://doi.org/10.1242/jeb.015503 (JEB article-pdf) | U8 (Varanus duty factor) |
| A14 | Cruse 1976 | Blocked | https://pub.uni-bielefeld.de/record/1681261 (bot check) | U3 open |
| A15 | Kubo 2019 supplement | Blocked | PMC6377445 not in the open-data bucket; figshare 403 | U12 open |
| A16 | Wang & Clarke 2015 supplement | Blocked | PMC4614784 not in the bucket | U17 open |
| A17 | Peterson & Hennessey 1978 | Blocked (free in a browser) | https://journals.ametsoc.org/downloadpdf/journals/apme/17/3/1520-0450_1978_017_0390_otuopl_2_0_co_2.pdf | W70 open |
| A18 | Touma 1977 | Blocked | Taylor & Francis 403 | W70 open |
| A19 | Idso 1975 | Blocked (free in a browser) | https://journals.ametsoc.org/downloadpdf/journals/apme/14/1/1520-0450_1975_014_0109_tdobsa_2_0_co_2.pdf | U21 open |
| A20 | Dempster 1955 | Exact | https://archive.org/details/DTIC_AD0087892 | U25 (Dempster part) |
| A21 | Meijaard 2007 | Exact (authorised Russian translation, same tables) | http://nd.ics.org.ru/upload/iblock/121/ND_2013_v9_n2_10.pdf | W50 |
| A22 | Perovich & Polashenski 2012 | Substitute: Perovich 1996 CRREL Monograph 96-1 | https://archive.org/details/DTIC_ADA310586 | W75 (albedo ranges) |
| A23 | Đikić 2007 | Blocked | hrcak.srce.hr search not reachable | U13 open |
| A24 | Willett AFM tyre data | Blocked (snapshot exists) | http://web.archive.org/web/20120313051803/http://www.biketechreview.com/tires/images/AFM_tire_testing_rev8.pdf | W40 open |
| A25 | Green Seal 2003 | Blocked (snapshot exists) | http://web.archive.org/web/20120210160650/http://www.greenseal.org/resources/reports/CGR_tire_rollingresistance.pdf | W43 open |
| A26 | Spitfire Mk VB trials | Blocked | no Wayback snapshot found | W62 open |
| A27 | Castillo-Figueroa 2020 | Exact article; supplement s001 blocked | https://zenodo.org/records/12823199 | Q17 (whole digits) |
| A28 | Baliga 2019 | Exact (Data S1-S5, SM) ; raw angles on figshare blocked | https://pmc-oa-opendata.s3.amazonaws.com/?list-type=2&prefix=PMC6810231. | Q16 wing bones only |
| A29 | Marek 2021 | Exact article; ESM S2 blocked | https://ora.ox.ac.uk (CC BY PDF) | none new |
| A30 | Gardiner & Nudds 2011 | Exact | JEB article-pdf (Crossref link) | Q19 |
| A31 | J Neurophysiol 2026 cats | Blocked | publisher 403 | Q8 open |
| A32 | Magro 2026 | Exact | https://pmc-oa-opendata.s3.amazonaws.com/PMC13023258.1/ | Q1 |
| A33 | Jaegger 2002; Jaeger 2007 | Blocked | AVMA refused | Q7 open |
| A34 | Herrel 2002 | Exact (author copy) | http://www.anthonyherrel.fr/publications/Herrel%20et%20al%202002%20Biol%20J%20Linn%20Soc.pdf | Q9 (phrynosomatids) |
| A35 | Humphries 2020 OSF | Exact | https://osf.io/pv4uj (API: https://api.osf.io/v2/nodes/3skrh/files/) and PMC7531786 S1-S5 | Q3 dogs; real-dog mocap |
| A36 | Volf 2023 | Exact | publikace.nm.cz (Lynx 54) | Q3 rabbit/hare long bones |
| A37 | Ajayi 2012 NZ rabbit | Substitute: Volf 2023 NZ rabbit group | as A36 | Q3 (long bones, not metapodials) |
| A38 | Chapman 2016 Dryad | Exact (Zenodo mirror) | https://zenodo.org/records/4979215 | Q5 |
| A39 | Schopf 2010 | Exact | https://www.vetmeduni.ac.at/hochschulschriften/diplomarbeiten/AC08346112.pdf | Q4 |
| A40 | Mei 2007 HAL | Blocked (HAL bot check) | https://inria.hal.science/inria-00402079/file/FastErosion_PG07.pdf | Q45 open |
| A41 | Cordonnier 2016 HAL | Blocked (HAL bot check) | https://inria.hal.science/hal-01262376/file/2016_cordonnier.pdf | Q45 open |
| A42 | USACE CEM Part II | Substitute: Shore Protection Manual 1984 Vol. I | https://archive.org/details/shoreprotectionm01unit | Q49 wave formulas |
| A43 | Ford Raptor release | Blocked (snapshot exists) | http://web.archive.org/web/20251206213628/https://media.ford.com/content/fordmedia/fna/ca/en/news/2021/02/03/2021-ford-f-150-raptor.html | Q31 open |
| A44 | Kuuluvainen 1988 | Exact (OCR made) | https://www.silvafennica.fi/pdf/5347 | Q40 Scots pine |
| A45 | Spruce 1992 / pine 2010 papers | Substitute for pine: Kuuluvainen 1988 | as A44 | Q40 pine; spruce open |
| A46 | Goodrich 2001 tigers | Blocked | no free copy found | Q1 tiger open |

## Group B (paywalled)
| # | Item | Status | Free lead | Rows |
|---|---|---|---|---|
| B1 | Erickson 2004 | Blocked | none | W14, W17 |
| B2 | Hutchinson & Garcia 2002 | Blocked | none | W19 |
| B3 | Hutchinson 2004 | Blocked (green copy exists, 403 to scripts) | http://researchonline.rvc.ac.uk/id/eprint/161/ | W19 |
| B4 | Hildebrand 1959 | Blocked | none | U14 |
| B5 | Hildebrand 1977 | Blocked | none | Q8 |
| B6 | Wetzel 1977 | Blocked | none | Q8 |
| B7 | Halbertsma 1983 | Blocked | none | Q8 |
| B8 | Jayes & Alexander 1980 | Blocked | none | U10 |
| B9 | Matolcsi 1970 | Blocked | none | U11 |
| B10 | Tsvetsinskaya 2002 | Blocked (AGU papers over 24 months old are free in a browser) | https://agupubs.onlinelibrary.wiley.com/doi/pdf/10.1029/2001GL014096 | U21 |
| B11 | Post 2000 | Blocked | none | U21 |
| B12 | de Leva 1996 | Substitute: Clauser 1969 (and Dempster 1955, A20) | https://archive.org/details/DTIC_AD0710622 | U25 |
| B13 | Norberg & Rayner 1987 | Blocked | none | U15 |
| B14 | Mazzetta 2004 | Blocked (third-party copy exists, not used) | none legitimate | W8 |
| B15 | Henderson 2010 | Blocked (snapshot exists) | http://web.archive.org/web/20250708064108/https://doc.rero.ch/record/32754/files/PAL_E3629.pdf | W4 |
| B16 | Campione & Evans 2020 | Blocked (accepted manuscript on Wiley, timed out) | https://onlinelibrary.wiley.com/doi/am-pdf/10.1111/brv.12638 | W13 |
| B17 | Persons 2020 | Blocked | none (the "green" hit is a Catalogue of Life record) | W15 |
| B18 | Lawson 1975 | Blocked | none | W2 |
| B19 | Taoka 1989 | Blocked | none | W36 |
| B20 | Belhaoues 2020 | Blocked (dog bones partly covered by A35 instead) | none | Q3 |
| B21 | Crowley & Hall 1994 | Blocked | none | Q21 |
| B22 | Musgrave 1989 | Blocked (ACM free-access PDF, timed out) | https://dl.acm.org/doi/pdf/10.1145/74334.74337 | Q45 |
| B23 | Solonenko & Mobley 2015 | Blocked; lead: Ocean Optics Web Book Jerlov page (CC BY, Kd tables) | https://oceanopticsbook.info/view/inherent-and-apparent-optical-properties/classification-schemes | Q46 |

## Group C (books)
C1 to C7 and C9: no open copy (archive.org copies of Chow, Wong and Hay are lending-only). C8 (Markvart & Castañer, W74/W75): **Substitute** for W75 via Perovich 1996 (A22 row); W74 open.

## Residual hand-download list (short; browser only, no login needed unless noted)
1. Wayback snapshots: Willett AFM (A24), Green Seal (A25), Henderson 2010 (B15), Ford Raptor (A43). URLs above.
2. HAL PDFs (A40, A41): URLs above.
3. Old JEB scans (A1, A2, A4, A10 x2, A11): article-pdf URLs above; they open free in a browser.
4. AMS free PDFs (A17, A19): downloadpdf URLs above.
5. AGU Tsvetsinskaya (B10), Wiley accepted manuscript Campione (B16), ACM Musgrave (B22), RVC Hutchinson (B3).
6. Dryad Shine data (A7): https://datadryad.org/dataset/doi:10.5061/dryad.t1g1jwt82 (download button; files need a browser session).
7. figshare: Baliga raw angles https://doi.org/10.6084/m9.figshare.8856161 (A28); Kubo https://figshare.com/articles/journal_contribution/24382114 (A15); Marek ESM via https://royalsocietypublishing.org/doi/suppl/10.1098/rspb.2020.3150 (A29).
8. PMC supplements: Kubo PMC6377445 (A15), Wang & Clarke PMC4614784 (A16), Castillo s001 https://pmc.ncbi.nlm.nih.gov/articles/instance/8181164/bin/zoolstud-59-060-s001.docx (A27).
9. Cruse 1976 (A14) https://pub.uni-bielefeld.de/record/1681261.

Everything else in group B and C needs library access or a purchase.

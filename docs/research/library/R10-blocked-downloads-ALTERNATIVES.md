# Round 10: blocked downloads and what stands in for them

Checked 2026-11-28. Each failed download is kept in raw/round10 as *.FAILED-not-a-pdf.html (or similar), and every attempt is logged in raw/round10/dl.log. I did not bypass any bot check or captcha.

| Item | What failed | Alternative used | Effect |
|---|---|---|---|
| Europe PMC PDFs (Nguyen 2009 Q angle, other PMC items, LLC study) | PDF render returned HTML | PMC full-text XML or a fetched PMC page | None; tables retyped from text. Image-only tables stay UNVERIFIED |
| Alexander 2011 ear (JPRAS) | Publisher PDF returned HTML | Publisher full-text page through the fetch tool | None |
| Craniovertebral angle 2024 (MDPI), HVA 2022 (BMC) | PDF link returned HTML at first try | A later direct PDF or XML route succeeded | None |
| Nguyen & Shultz 2009 (UNCG repository) | Not a PDF | PMC page; tables are images | Matrix values UNVERIFIED (D5) |
| Gonial angle studies (Padjadjaran, Hindawi archive) | Hosts would not serve the PDF | Search summaries only | UNVERIFIED, flagged in A6 |
| Tedla carrying angle (Via Medica, Folia Morphologica) | Bot-check redirect; not bypassed | Search summary only | UNVERIFIED, flagged in A2 |
| Jung 2015 nails (e-APS) | PDF and table images not served | PMC text plus search summary | Per-finger sizes UNVERIFIED (B2) |
| CDC ROM data set page, NHANES sr11_252, NIOSH fit-panel report, Greiner 1991 (DTIC) | curl 403 or HTML | Fetch tool copy (CDC, NIOSH); archive DjVu text and page images (Greiner) | NHANES text truncated after Table 14, so Tables 24-33 (child skinfolds, segment lengths) were not reached |
| Soucie 2011 ROM | Closed | Secondary SDs (Holsman) | Labelled Secondary (C1) |
| Hirschmann 2019 HKA | Closed | None | Gap (A2) |
| Roe deer 2009 (HAL) | Not a PDF | Fruzinski et al. 1982 (RCIN repository) | Older but larger sample (F5) |
| Best & Fisher 1979 von Mises sampler (JSTOR) | Not a PDF; no free copy | SciPy documentation and Wikipedia (Secondary) | Citation UNVERIFIED (MATH) |
| Mardia & Jupp 2000 (book) | No free copy | Wikipedia quoting p.36 (Secondary) | Labelled (MATH) |
| Guinness pages for the shortest woman, tallest woman ever and lightest person | Returned 404 | Other Guinness pages for the extremes that were reachable | Limb and mass-minimum extremes are ANSUR II only (C6) |

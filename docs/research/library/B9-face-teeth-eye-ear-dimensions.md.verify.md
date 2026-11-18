# Second check: B9-face-teeth-eye-ear-dimensions.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: B9-face-teeth-eye-ear-dimensions.md

**31 rows checked: 29 VERIFIED, 1 FIX, 1 UNVERIFIABLE.**

I pulled all six full texts from Europe PMC (fullTextXML) and parsed each table cell by cell. Every retyped value matches the source table, and so do all four licences as read from the article permissions blocks. That covers:
- Lim Tables 2-5 (10 dimensions by sex, the percentiles, Malay vs non-Malay, the regression, and the relayed NIOSH and Chinese columns)
- Uslu-Akcam Tables 1-3 (24 tooth rows, the sex differences, and the exact list of non-significant MD teeth)
- Rakhshan Tables 1-2 (right side, 56 cells)
- Hartmann Tables 1-2
- Alrasheed Table 1
- Cho Tables 1-6

The relayed-source references (Zhuang & Bradtmiller 2005, Du 2008, Alexander 2011, Wang 2011) match Lim refs 24 and 5 and Cho refs 3 and 6. Crossref confirms every DOI, volume and page.

FIX:
- **Row 22, Cho "the right ear is within 0.6 mm":** this isn't true on every row. Auricle distance from the inferior orbital rim (IOR) in men is 91.3 on the left against **90.0** on the right, a **1.3 mm** gap. Right-minus-left differences are also marked significant (*) for several rows. Suggested wording: "within 0.6 mm except auricle distance from the IOR in men (1.3 mm)".

UNVERIFIABLE:
- **Row 28**, "callipers in the older surveys", under Sources disagree. Lim doesn't say how the NIOSH and Chinese surveys measured, and I didn't open those papers. It's an untested reading.

Missing context:
- **Cho ear data are 3D-CT soft-tissue measurements.** They come from patients who had facial CT for medical reasons at six hospitals, not from live calliper measurements. That's relevant to the "Korean SDs are 2-3× larger" point under Sources disagree, and it should be stated in section 4.
- **Cho Table 6:** its footnote wrongly says "Data from Indian, Caucasian, and African population". The Han Chinese values are Wang 2011 (ref 6, a CT study), which the file correctly attributes.
- **Cho age trend:** men's ear length peaks at **70.2** at 60-69, not at 70+ (69.5). The "63.9 to 69.5" endpoints are right.
- **Uslu-Akcam age range:** the 10 to 26 years includes adolescents, which is worth a note when comparing.
- **No access date:** the file header has none. Sources 2-6 give DOI links only, with no Europe PMC URL. Every DOI resolves on Crossref, but the brief asks for a URL plus access date on each source.

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 22 | Right ear within 0.6 mm | FIX | IOR men differ by 1.3 mm | https://doi.org/10.3346/jkms.2025.40.e107 |
| 28 | Disagree: callipers in older surveys | UNVERIFIABLE | not in Lim | https://europepmc.org/article/PMC/PMC9459016 |

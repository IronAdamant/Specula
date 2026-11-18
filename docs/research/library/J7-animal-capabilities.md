# J7: What animals can do (top speed against body mass)

Fixed after the second check (2026-11-18):
- Licence added: the bioRxiv preprint is CC BY-NC-ND (non-commercial, no derivatives). The facts can be used; its text and figures cannot be reused.
- The "commonly cited form" of the model, v_max = a·M^b·(1 − e^(−h·M^i)), moved to "Unconfirmed": it was not read from any source.
- Added a Sources list. The checker's summary is in `J7-animal-capabilities.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Partial. Gait timing and transition speeds for mammals are in C2; bird flight numbers are in C3.

## Table J7-1. Maximum-speed scaling model (read directly, preprint)

Source: Hirt MR, Jetz W, Rall BC, Brose U, "Universal scaling of maximum speed with body mass - Why the largest animals are not the fastest", bioRxiv preprint 10.1101/095018, posted 17 Dec 2016, https://www.biorxiv.org/content/10.1101/095018v1.full. Licence: CC BY-NC-ND, as the bioRxiv API lists it (`cc_by_nc_nd`, https://api.biorxiv.org/details/biorxiv/10.1101/095018, second check 2026-11-17); facts only, the text and figures are not to be reused. Published version: *Nature Ecology & Evolution* (2017), https://www.nature.com/articles/s41559-017-0241-4 (not opened in this pass; its licence was not checked, and Europe PMC shows it as not open access).

Model form: a power law a·M^b times a limiting term between 0 and 1 that falls with mass through the exponent i. The equations are images on the preprint page and were not read (the form written down in the first pass is under "Unconfirmed" below). Speed first rises with mass as a power law, then falls for large animals because they run out of fast-available energy before they finish accelerating. Data: 617 data points for 470 species, 5.7×10⁻⁸ to 108,000 kg, running, flying and swimming; overall fit R² = 0.893.

| Mode | a | b | i | n (data points, figure 2 legend) |
|---|---|---|---|---|
| Running | 26 | 0.24 | 0.6 | 453 |
| Flying | 144 | 0.27 | 0.72 | 55 |
| Swimming | not given in the text | 0.36 | 0.56 | 109 |

The text says "flying animals are nearly six times faster than running ones (a = 144 and 26)". The remaining constant (h), the swimming a and the units are in Supplementary Table 4 (per the text, 'parameter values in Supplementary Table 4'), which was not read, so this table cannot yet be turned into speeds without that table. Do not use it for numbers until h is filled in from the supplement.

Other findings in the same text: in running and flying animals, warm-blooded species tend to be faster than cold-blooded ones of the same size; in water the reverse holds.

## Not found

- Supplementary Table 4 (h, swimming a, units) and the species-level speed table (Supplementary data). The Nature supplement and the iDiv data portal were not opened in this pass.
- Jump heights, carrying loads and endurance for animals: not reached in this pass.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives it has been opened.

- **The model's equation, v_max = a·M^b·(1 − e^(−h·M^i)), "commonly cited".** The checker could not confirm it: the paper's equations are images, and this form came from memory or a secondary source, not from any page read. The first pass already flagged it as needing a check. Read the equation from the paper (preprint or *Nature Ecology & Evolution* version) before using it.

## Sources

- Hirt MR, Jetz W, Rall BC, Brose U (2016), bioRxiv preprint 10.1101/095018, CC BY-NC-ND (facts only), https://www.biorxiv.org/content/10.1101/095018v1.full (accessed 2026-11-17); licence from the bioRxiv API, https://api.biorxiv.org/details/biorxiv/10.1101/095018 (opened by the second check, 2026-11-17).
- Hirt MR et al. (2017), *Nature Ecology & Evolution*, https://www.nature.com/articles/s41559-017-0241-4 (named, not opened; accessed 2026-11-17).

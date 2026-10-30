# Built-in catalog

Built-in recipes combine authored sound settings with study citations, methods, limitations and disclosed adaptations. Browse them without opening an audio device:

```text
auditorylab list --collection builtin
auditorylab describe builtin:ioannou-2015-binaural-10-hz
auditorylab inspect builtin:ioannou-2015-binaural-10-hz
```

`describe` shows the current recipe's research context; `inspect` validates its sound at the chosen rate. `recipe builtin:ID` prints its complete portable document. The CLI and desktop use the same embedded catalog. Catalog names contain lowercase ASCII letters, digits and hyphens; a syntactically valid name still needs a matching entry. Generator details identify each authored component ID and lane.

[Source methods](catalog-sources.md) gives the detailed paper locators and explains each recipe's adaptations. The [authoring guide](recipes.md#research-context-and-catalog-entries) defines admission and fidelity requirements. The authoritative condition data is in [catalog](../crates/core/catalog), with shared bibliography in [studies](../crates/core/studies); the application assembles standalone recipes from these embedded inputs at runtime. The curated `ENTRIES` and `STUDIES` lists in the [catalog module](../crates/core/src/model/catalog.rs) define their membership and order.

## Nominal rhythm references

The CLI and desktop derive reference-band memberships from the same pure structural classifier. These labels describe authored repetition, gating, amplitude modulation and explicit signed-offset pairs. They do not measure EEG, predict a mental state or certify a percept. Carrier frequency and endpoint fades are not classification inputs.

The [format reference](formats.md#queries-and-classification) defines the band intervals and query values.

The convention identifier is `ifcn-2017-corrected-2019`: [Kane et al. 2017, glossary](https://doi.org/10.1016/j.cnp.2017.07.002), with the [2019 beta correction](https://pure.au.dk/portal/en/publications/corrigendum-to-a-revised-glossary-of-terms-most-commonly-used-by-/). The glossary is also available as [publisher-deposited Europe PMC XML](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6123891/fullTextXML). Study-specific conventions remain in research notes.

One sound can belong to several bands. Two independent processes in one band still count as multiple rhythms. Positive rates outside these intervals remain **Other rates**. **No rhythmic rate** requires complete analysis; invalid intrinsic settings appear under **Needs review**. A known modulation rate keeps its nominal band even if the carrier cannot play at a particular sample rate. Full target validation remains necessary before playback/export.

AM contributes a rhythm only at positive depth. Raw tones remain independent steady tones; their carrier differences do not establish a binaural or monaural pair. Finite very quiet levels are retained in nominal analysis without an audibility threshold.

Explicit `binaural_pair` and `monaural_pair` components contribute the absolute signed offset as their nominal rhythm, independently of other components. A zero offset contributes no rhythm. Their component identity attributes the relationship without raw-tone inference. The same classifier serves Built-in, User and temporary recipes.

Band and method filters are correlated within one descriptor. A method cannot borrow a band's membership from an unrelated process. The classifier returns exact authored rates independently of the selected device/export rate; presentation retains sufficient decimal precision at boundaries.

## Sampled pulse widths

Catalog pulse sources and pulsed modulation retain explicit rectangular shape and duration width intent in schema 5. They realize each stated width as exactly `round(width_seconds × sample_rate)` frames. Their onsets are the first frames at or after the period onsets. The catalog’s 1 ms pulses therefore have a constant frame width at each rendering rate.

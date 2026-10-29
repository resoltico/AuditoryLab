# Catalog source methods

This reference records the primary methods, evidence limits and synthesis adaptations used by the current [built-in catalog](catalog.md). Condition-specific parameters and disclosures are stored in [catalog recipes](../crates/core/catalog); citation records are stored in [studies](../crates/core/studies).

## Primary sources and evidence limits

| Study | Primary version and source |
|---|---|
| Beauchene et al. 2016 | PLOS ONE 11(11), e0166630; [publisher article](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0166630), DOI 10.1371/journal.pone.0166630 |
| Chan et al. 2022 | PLOS ONE 17(12), e0278412; [publisher article](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0278412), DOI 10.1371/journal.pone.0278412 |
| Ioannou et al. 2015 | PLOS ONE 10(6), e0129486; [publisher article](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0129486), DOI 10.1371/journal.pone.0129486 |
| Jirakittayakorn & Wongsawat 2017 | Frontiers in Neuroscience 11:365; [publisher article](https://www.frontiersin.org/journals/neuroscience/articles/10.3389/fnins.2017.00365/full), DOI 10.3389/fnins.2017.00365; [author/publisher-deposited PMC copy](https://pmc.ncbi.nlm.nih.gov/articles/PMC5487409/) |
| Lahijanian et al. 2024 | Scientific Reports version of record / PMC version 1; [publisher](https://www.nature.com/articles/s41598-024-63727-z), DOI 10.1038/s41598-024-63727-z; [archived full XML](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11161471/fullTextXML) |
| Martorell et al. 2019 | Cell final edited publisher PDF and PMC author manuscript; [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC6774262/), DOI 10.1016/j.cell.2019.02.014; [author-lab publisher PDF with supplementary figures](https://synthneuro.org/wp-content/uploads/2021/08/19-04-martorell.pdf) |
| Orozco Perez et al. 2020 | eNeuro / PMC version 1; [publisher](https://www.eneuro.org/content/7/2/ENEURO.0232-19.2020), DOI 10.1523/ENEURO.0232-19.2020; [archived full XML](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC7082494/fullTextXML) |
| Van Eeckhoutte et al. 2018 | Trends in Hearing version of record / PMC version 1; [publisher](https://journals.sagepub.com/doi/full/10.1177/2331216518805352), DOI 10.1177/2331216518805352; [archived full XML](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6196616/fullTextXML) |

Check primary methods, relevant supplements and publisher corrections when revising a recipe. Required waveform parameters come from primary methods.

Broader-literature caution is separate from checking individual source parameters. [Ingendoh, Posny and Heine (2023), PLOS ONE 18(5):e0286023](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0286023), Abstract, eligibility criteria and Discussion, reports inconsistent EEG entrainment findings across fourteen studies and substantial methodological heterogeneity.

## 1. Beauchene 2016 — four recipes

**Locators:** Materials and Methods / Participants (`sec003`), Auditory Stimulus (`sec004`), Visuospatial Task (`sec006`); Fig 1; Results / Working Memory Task Performance (`sec015`), Fig 3; Statistical Methods; Discussion. S1 Fig is the OLS-metric correlation matrix, not an additional sound specification.

| Recipe | Source ear assignment/carrier | Source continuous duration | Recipe parameters |
| --- | --- | --- | --- |
| `beauchene-2016-binaural-5-hz` | L245/R240 Hz pure tones | 5 min | exact frequencies, routing and 300 s match |
| `beauchene-2016-binaural-10-hz` | L250/R240 Hz pure tones | 5 min | exact frequencies, routing and 300 s match |
| `beauchene-2016-binaural-15-hz` | L255/R240 Hz pure tones | 5 min | exact frequencies, routing and 300 s match |
| `beauchene-2016-pure-tone-240-hz` | L240/R240 Hz control | 5 min | `route=both`, 240 Hz and 300 s match |

No pulse width or physical amplitude modulation is prescribed; binaural difference is a percept rather than an amplitude-modulator parameter. MATLAB generation and Sony MDR-NC7 stereo headphones are stated; participants selected volume. **Audio** sample rate, starting phases, digital amplitude and endpoint ramps are unspecified. The paper's 128 Hz number is EEG acquisition, not an audio rate. All four recipes explicitly add phase-zero starts, -20 dBFS component peaks and 0.1 s fades without claiming acoustic calibration.

Twenty-eight adults performed a titrated visuospatial task; six sound/control conditions changed every five minutes in randomized order. The reported outcome is later-minus-earlier accuracy **within each condition**, not an established general cognitive benefit: only 15 Hz had positive change (about 3% in the discussion); all others were negative; reaction-time condition comparison was nonsignificant. The recipe summaries state those directions, participant count, duration and null reaction-time result. The statistical analysis bootstrapped the 28-person behavioral data; its expanded analysis sample must not be described as hundreds of participants.

## 2. Chan 2022 — one recipe

**Locators:** Sensory stimulation (`sec005`), Fig 2A/B; Evaluation of induced entrainment (`sec006`); Study design (`sec003`), Participants (`sec004`); Structural MRI (`sec021`), Fig 4, S5 Table; Cognition (`sec024`), Table 4, Fig 6; Discussion (`sec026`). [S1 Protocol](https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0278412.s014&type=supplementary) is protocol #1712179268 (3 pages); [S2](https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0278412.s015&type=supplementary) #1902706647 (5 pages); [S3](https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0278412.s016&type=supplementary) #1903763319 (8 pages). Their broader approved ranges do not replace the final article's actual 40 Hz condition.

`chan-2022-clicks-40-hz`: source 1 ms rectangular sound-control pulse per 25 ms (40 Hz, 4% duty), approximately three-minute sound-only EEG condition for cognitively normal adults. Thus rate 40, width 0.001 and duration 180 s are supported. Clinical/epilepsy EEG conditions were approximately one minute; chronic AD intervention was **combined** light/sound for one hour daily, not this standalone three-minute sound recipe. Loudspeakers/soundbar were approximately five feet away; article reports 78 dB in cognitively normal adults and 68 dB for mild AD/epilepsy, without enough information to recreate a calibrated dBFS setting. Fig 2B labels electrical on/off signals. The measured ear-canal waveform, phase, acoustic pulse polarity and headphone channel assignment are unspecified. Audio sample rate and endpoint ramps are unspecified; the 512 Hz EEG digitization is not audio. The adaptation explicitly identifies identical both-ear positive pulses, uncalibrated -20 dBFS, added 0.1 s fades and omitted light.

The recipe summary reports: pilot enrolled 15; MRI comparison active 7/control 6; ventricular difference p = .024; no significant total-brain-volume change; FNA delayed recall active 4/control 5, between-group p = .027; standard cognitive tests nonsignificant. S5 Table gives total-brain-volume p = .419 and cortical-thickness p = .547. Small sample, missing data, exploratory underpowered outcomes and combined stimulation prevent inference of a sound-alone treatment.

The publisher supplements comprise S1 Checklist; S1–S7 figures (timeline, scalp/intracranial EEG, education/confounding, actigraphy); S1–S5 tables; and the three protocol PDFs. The statistical tables and protocols support the participant/outcome distinctions above; they do not supply the unspecified acoustic phase or audio sample-rate calibration.

## 3. Ioannou 2015 — six recipes

**Locators:** Materials and Methods / Participants (`sec003`), Stimuli (`sec004`), Recording and pre-processing; Results / Power analysis, Fig 1; Discussion / Limitations and future extension (`sec020`); [S1 File](https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0129486.s001&type=supplementary), Additional Power Analysis and Figs A/B.

`ioannou-2015-binaural-10-hz`: 200 Hz left/210 Hz right, 60 s. `ioannou-2015-pure-tone-200-hz`: 200 Hz both ears, 60 s. The 2, 6, 20 and 40 Hz one-minute conditions retain 200 Hz left with 202, 206, 220 or 240 Hz right: `ioannou-2015-binaural-2-hz`, `ioannou-2015-binaural-6-hz`, `ioannou-2015-binaural-20-hz` and `ioannou-2015-binaural-40-hz`. These values follow the primary frequency-step schedule; the 2 Hz condition supplies a Delta reference without a sleep claim. All six match explicit source conditions. Blocks contain 20 s silence, then 60 s non-binaural control, then 60 s binaural sound. The recipes intentionally omit that sequence. Sixteen musicians plus sixteen non-musicians watched a silent subtitled film and ignored the sounds. Philips in-ear headphones and participant-adjusted constant-within-participant volume are stated. Absolute acoustic/digital level, audio sample rate, starting phase and ramps are not specified; EEG sampling (512 Hz, later 256 Hz) is distinct. The recipes' -20 dBFS, phase-zero and 0.1 s fade adaptations are disclosed.

The strongest robust group frequency-following findings concern the alpha beat band (9–12 Hz), not proof that the individual recipe improves wellbeing. S1 File reports that only alpha SSR remained significant across five bands and describes a delta-stimulation/alpha-response cross-frequency finding. The recipe summaries describe neural responses. Source limitations include individualized volume/possible cross-hearing, right-ear-only mismatch and no behavioral measure of the illusion. The source's per-band block-count sentence is internally inconsistent with its frequency-step schedule. The selected recipes follow the explicit frequency conditions and do not reconstruct the whole experimental schedule.

## 4. Jirakittayakorn 2017 — one recipe

**Locators:** Materials and methods (`s2`) / Binaural beat sound, Participants, EEG recording, Experimental procedures, Fig 2, Statistical analysis; Results / theta activity, Tables 1–5, emotional states Tables 6–8; Discussion; Limitation (`s5`).

`jirakittayakorn-2017-binaural-6-hz`: source 250 Hz left/256 Hz right; Sound Forge Pro 11.0; 65 dB SPL; Monster Inspiration over-ear headphones; 30 min sound between 5 min baseline and 5 min post-sound. The recipe's 250L/256R and 1800 s match. No amplitude-modulator/pulse-width parameter is required. Starting phases, endpoint ramps and audio sample rate are unspecified. The recipe's phase-zero/-20 dBFS/0.1 s fade and omitted baseline/post periods are explicit adaptations; -20 dBFS cannot recreate 65 dB SPL.

Source reports 17 experimental/11 control participants. Within-group theta increases were broad at 10 min; between-group results were limited by cortical location/time (Table 5), not uniform proof of improved attention. Mood analyses used p < .1 within groups and p < .05 between groups. Tables 6–8 distinguish decreased “worried” from increased negative items; fatigue/vigor changed in both groups. The recipe summary distinguishes EEG resemblance to meditation from demonstrated wellbeing or performance effects. Single exposure, restricted sound and timing of emotional evaluation are explicit source limitations.

**Supplement:** The author/publisher-deposited `DataSheet1.docx` is available through [Europe PMC supplementaryFiles](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC5487409/supplementaryFiles?includeInlineImage=false). It contains four standard-deviation tables and nine Word charts. Supplementary Figs 1/2 describe mean theta power by channel/time for experimental/control groups; Fig 3A–G gives baseline differences by group/time; Tables 1–4 are standard deviations (do not mistake them for the mean powers). They add no source audio phase/ramp/sample-rate prescription.

## Lahijanian, Martorell, Orozco Perez and Van Eeckhoutte recipe parameters

The following recipes use schema 4 and 0.1 s endpoint fades. All levels are uncalibrated dBFS; none represents a reproduced acoustic level. `SAM` means full-depth sinusoidal amplitude modulation, and `gate` means rectangular modulation.

| Exact recipe ID | Declared components/routing | Rate/width/depth | Duration | Peak/component |
|---|---|---|---|---|
| `lahijanian-2024-tone-pips-40-hz` | 5000 Hz tone, Both | gate 40 Hz / 1 ms | 40 s | -20 dBFS |
| `martorell-2019-tone-pips-8-hz` | 10000 Hz tone, Both | gate 8 Hz / 1 ms | 3600 s | -20 dBFS |
| `martorell-2019-tone-pips-40-hz` | 10000 Hz tone, Both | gate 40 Hz / 1 ms | 3600 s | -20 dBFS |
| `martorell-2019-tone-pips-80-hz` | 10000 Hz tone, Both | gate 80 Hz / 1 ms | 3600 s | -20 dBFS |
| `orozco-perez-2020-binaural-7-hz` | `binaural_pair`, 396.5 Hz Left / 403.5 Hz Right | beat offset +7 Hz | 480 s | -20 dBFS |
| `orozco-perez-2020-binaural-40-hz` | `binaural_pair`, 380 Hz Left / 420 Hz Right | beat offset +40 Hz | 480 s | -20 dBFS |
| `orozco-perez-2020-monaural-7-hz` | `monaural_pair`, 396.5 + 403.5 Hz, Both | beat offset +7 Hz | 480 s | -23.0103 dBFS |
| `orozco-perez-2020-monaural-40-hz` | `monaural_pair`, 380 + 420 Hz, Both | beat offset +40 Hz | 480 s | -23.0103 dBFS |
| `van-eeckhoutte-2018-am-500-hz` | 500 Hz tone, Left | SAM 39.0625 Hz / depth 1 | 307.2 s | -20 dBFS |
| `van-eeckhoutte-2018-am-1000-hz` | 1000 Hz tone, Left | SAM 39.0625 Hz / depth 1 | 307.2 s | -20 dBFS |
| `van-eeckhoutte-2018-am-2000-hz` | 2000 Hz tone, Left | SAM 39.0625 Hz / depth 1 | 307.2 s | -20 dBFS |

## 5. Lahijanian 2024 — one recipe

**Locators:** Methods XML `Sec14` (participants), `Sec18` (stimulation), Results `Sec4`/`Sec7`, limitations `Sec13`; [publisher supplement](https://media.springernature.com/full/springer-static/esm/art%3A10.1038%2Fs41598-024-63727-z/MediaObjects/41598_2024_63727_MOESM1_ESM.pdf), Table 1, PDF p. 1.

**Source facts:** A 5 kHz carrier uses 40 Hz rectangular modulation, 4% duty/1 ms; six or ten 40 s trials alternate with 20 s silence. Two speakers are 50 cm apart and 50 cm from the listener. The paper reports approximately -40 dB without naming that level's reference, then participant comfort adjustments. Audio phase, ramps and audio sample rate are unspecified. Thirty-five memory-clinic volunteers were recruited; P6/P13 were excluded, leaving 33. EEG findings concern entrainment/connectivity, with unequal responsiveness and a single session; treatment or memory improvement was not tested here.

**Recipe adaptation:** The single-trial timing and carrier/gate agree. Digital Both routing, continuous phase-zero carrier, -20 dBFS and endpoint fades are explicitly disclosed synthesis choices. The adaptation acknowledges the ambiguous -40 dB statement without relabeling it SPL or dBFS. The recipe does not reproduce a calibrated exposure or treatment.

## 6. Martorell 2019 — three recipes

**Locators:** STAR Methods, auditory tone-train protocol, publisher PDF p. 21 (printed e4); Figure 2, PDF pp. 5-6; Figure 3, p. 8; Figure S2, pp. 27-28; Figure S3, p. 29.

**Source facts:** 1 ms/10 kHz tones repeat at 8/40/80 Hz or random intervals from overhead speakers; reported level is 60 dB, with calibration reference/method and audio phase/ramps/sample rate unstated in this protocol. Figure 3 specifies 1 h/day for seven days. Its amyloid groups are no-stimulation/40 Hz n=19 each, 8 Hz n=4, 80 Hz n=7. Memory groups use n=20/20/9 for recognition and n=25/28/9 for water maze. Reported benefits concern mouse models. S3A explicitly lacks hippocampal Aβ1-40 assays for 80 Hz/random; missing assays are not negative results.

**Recipe adaptation:** All three carriers, rates, widths and per-session durations agree. Both routing, digital level, phase and fades remain adaptations; the daily schedule/random controls are not implemented. Recipe summaries give the sample counts for their respective experiments. Missing assays are not evidence of a null result.

## 7. Orozco Perez 2020 — four recipes

**Locators:** Materials and Methods XML `s3A`, `s3B` (stimuli), `s3E` (procedure), `s3F` (calibration); FFR `s3I4`; ASSR results `s4E`/`s4F`; mood results `s4G4`, Figures 2/4.

**Source facts:** Sixteen adults receive four continuous eight-minute conditions after an eight-minute baseline. Dichotic tones have equal starting phases; the lower tone goes left. Diotic controls sum/halve the two tones; both sound types are RMS normalized. Stated pairs are 396.5/403.5 and 380/420 Hz. Live 48 kHz RX6 generation drives ER3 inserts, calibrated to 70 dB SPL per ear using a 2-cc coupler/meter with slow A-weighting. Absolute starting phase and ramps are not specified. Monaural ASSRs exceed binaural responses; mood ratings do not differ. FFR text inconsistently says 397.5, and one gamma-results sentence conflicts with its subsequent monaural-higher statistics.

**Recipe adaptation:** All four parameter/routing/duration choices agree with the stimuli section. The adaptation identifies the 396.5/397.5 discrepancy and chosen 7 Hz difference. Zero absolute phase, uncalibrated levels and fades remain declared choices. The monaural -23.0103 dBFS level is a rounded plateau-RMS match, not replicated calibration. The recipe follows the explicitly stated sound frequencies and the interpretation supported by the quantitative results and figures.

## 8. Van Eeckhoutte 2018 — three recipes

**Locators:** Methods XML `sec3-2331216518805352` (participants), `sec4-2331216518805352` (stimuli/apparatus), `sec6` (behavioral balancing), `sec7` (EEG); results `sec13`, Figure 6 bottom.

**Source facts:** Full-depth SAM carriers are 500/1000/2000 Hz. EEG stimulation lasts 307.2 s at 96 kHz; nominal 40 Hz becomes 39.0625 Hz for integer periods in 1.024 s epochs. Ears alternate, separated by at least 10 s. ER3A inserts use a calibrated 2-cc coupler. EEG levels span 40-80 dB SPL in 10 dB steps, with extra 5 dB steps for 1000 Hz. Of 38 young participants labeled normal-hearing, 19 hear 500/2000 Hz and 19 hear 1000 Hz; one has a 35 dB HL threshold exception at 500 Hz. Relative/absolute carrier-modulator phase and ramps are unspecified. Across conditions, predicted-versus-behavioral balances differ by under 5 dB in 59%, under 10 dB in 85%; substantial individual errors remain.

**Recipe adaptation:** All three EEG carrier/modulator/duration values agree, including the actual 39.0625 Hz modulation rate. Selecting Left, normalized `(1+sin)/2`, zero phases, one uncalibrated digital level and fades are disclosed adaptations. The recipe summaries describe aggregate hearing-measurement accuracy and its individual variation.

## Catalog scope

Built-in recipes use stated carrier/pulse/modulation and duration parameters, while their chosen digital routing, phase, fades, rendering rates and uncalibrated digital levels remain explicit adaptations. Free-field speaker placement does not itself specify independent ear channels. Calibrated dB SPL, an unspecified dB reference and dBFS are different facts; none may stand in for another. Sources that leave required waveform parameters unstated cannot acquire invented values through descriptive text.

Chan's pilot combined light and sound. Beauchene measured within-condition task changes; Ioannou measured EEG responses; Jirakittayakorn measured theta activity and mood. Martorell's mouse findings are not human treatment evidence, and its missing assays are not negative results. Orozco's internally inconsistent sentences are identified alongside the stimuli/statistics supporting the recipe interpretation. Van Eeckhoutte's aggregate hearing-measurement accuracy is not per-carrier therapeutic performance.

The running application neither downloads these sources nor checks custom metadata against parameters. When changing a recipe, check every affected description, independent signal oracle and digest; expected values must follow an explained signal change.


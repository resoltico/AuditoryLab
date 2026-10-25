# Persistent-library verification record

This records the local implementation checked on 2026-10-25, before repository integration. The product remains 0.5.0/build 2; the one Unreleased Recipe schema remains 3. Managed-record and CLI-output formats are 1 independently. Main-branch integration is not release, real-time or physical-device qualification.

## Delivered scope and functional checks

The shared nominal classifier/query, first-class signed-offset pairs, portable identity/history/evidence/lineage, rooted persistent store and lifecycle, guarded immutable saves/recovery receipts, ordinary CLI and native Slint are implemented. The 23-entry catalog retains all 19 earlier numerical/sample oracles and adds the 2/6/20/40 Hz Ioannou conditions with disclosed adaptations. Current format/behavior sources are [recipes](recipes.md), [architecture](architecture.md), [CLI](cli.md), [desktop](desktop.md) and [catalog evidence](catalog.md).

Local Apple M4/macOS 27.0.1 checks: workspace 446 passed/0 failed/10 ignored; explicit production CLI/installed Slint parity 13 queries; native filesystem faults 8 and multiprocess tests 5; release core 119. fmt, all-target clippy, docs, dependency policy, vendor authentication and actionlint pass. The ignored count includes five physical-device tests that were not run, the separately invoked official benchmark and diagnostic, two invoked subprocess helpers, and the explicitly invoked query comparison. They are not all unexecuted or counted twice. Core 119 also passes under Rosetta; translated execution is not native Intel qualification.

The audited compact empty-filter/recovery routes, committed-filter summary, duration/sort-date rows and local-time details with selectable exact UTC have production regressions. Back restoration waits for selected-row initialization and clamps scroll; tests deliberately delay the first frame. Empty scopes/unavailable root and 800×600/26px controls/transport are covered on the production Slint offscreen renderer. Actual private macOS app checks cover duration/date rows, readable selected read-only UTC, staged-invalid versus applied filters, clear/recovery and unchanged source/library bytes. Physical compact resizing was not driven successfully through automation and is not counted as native compact qualification. Full OS/theme/font/screen-reader/device matrices remain partial.

## Timing remains failed

The unchanged criterion is maximum*100≤floor(buffer_ns)*75 for every measured callback, with 16 voices, up to 32 transition slots, 100 warmup calls and 3,000 measured calls per configuration. Rates and supported voices were not reduced. Raw results are retained in [the callback evidence directory](verification/callbacks-2026-10-25/), with SHA256SUMS identifying the gzip bytes and UNCOMPRESSED-SHA256SUMS identifying the unchanged original output, rather than proving correctness.

The original 768 kHz waveform failures were 1,882,291 ns at 256 frames (budget 333,333 ns) and 6,355,000 ns at 4096 frames (budget 5,333,333 ns). Exact-phase modulation sine reuse preserves per-voice clocks, sample arithmetic and mixing order and reduces typical work. It does not guarantee wall-clock tails.

A subsequent user-confirmed quiet native old/fixed/fixed/old comparison was run once:

| Run | Configurations passing 75% | Exit | Nominal deadline misses |
|---|---:|---:|---:|
| Old 1 |23/24|101|0|
| Fixed 1 |24/24|0|0|
| Fixed 2 |23/24|101|1|
| Old 2 |24/24|0|0|

Both original waveform cases pass both fixed runs. Fixed 2 still fails 768 kHz/256 frames/Quantities: 349,959 ns against 249,999.75 ns headroom and 333,333 ns nominal deadline. That event's exact cause is unresolved: the official benchmark sorts durations and retained no per-event iteration/CPU state. The passing fixed1/old2 do not erase this failure or prove a cure.

A diagnostic QA audit corrected an unlike-window inference: the historical enclosing CPU delta included after-call getrusage outside inner wall. Enclosing CPU>threshold is not proof of callback-only CPU>threshold; wall-minus-CPU is not exact off-CPU time. The corrected schema 2 sampler records nested raw wall/CPU boundaries with process counters outside, empty/sleep calibration, publication/poll/state witnesses and signed conditional bounds. Reported clock resolution is not an accuracy guarantee; no median subtraction or clipping qualifies a callback.

Two bounded captures measured 108,000 callbacks (36,000 Quantities) without reproducing an over-limit event. A 4-second/30-second/128 MiB-bounded native CPU profile completed below 58 MB, but running-only millisecond samples cannot establish event-level scheduling/VM attribution. Absence of reproduction is not a fix. No additional official run was made for green.

## Evidence and remaining conditions

Full local raw diagnostics, native screenshots, earlier failures, checked-input manifests and development archives remain under ignored `target/persistent-library-handoff/`; they include host metadata and large traces and are deliberately not repository content. The compact raw qualification logs above preserve the unresolved failure remotely without those captures. Local aggregate acceptance is 92 verified-local/9 partial/1 failed qualification, not 102/102. Remaining conditions include complete native platform/filesystem/unsupported-lock/reparse behavior, the full native accessibility/layout matrix, other native target/sample/distribution qualification and physical-device/dedicated-host real-time assurance.

A failed controlled callback criterion remains failed after commit or merge. No release, calibrated SPL, clinical effect, universal device/rate support or production readiness is claimed.

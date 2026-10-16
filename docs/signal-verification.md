# Signal and offline render characterization

These checks characterize the generators and sink conversions. They supplement the independent catalog formulas, pinned sample digests, partition/allocation checks, RIFF/RF64 decoders and callback state-machine tests. Numerical acceptance and offline computation time are separate properties.

## Independent numerical references and budgets

Let `u = 2^-53`, the binary64 unit roundoff. The phase characterization uses exactly representable declared frequencies 403.5 Hz (`807/2`), 39.0625 Hz (`625/16`) and 10,000 Hz, at 44.1/48/96 kHz. Its reference is the integer remainder `(n * numerator) mod (sample_rate * denominator)`, divided only after reduction. Integer arithmetic uses u128; the reduced integers fit exactly in binary64. It does not multiply a frame index by the production-rounded step.

For these normalized steps, divide rounding contributes at most `n*u*(frequency/sample_rate)` cycles. Each accumulator addition has magnitude below 1.5 and contributes at most `u` absolute cycles. Subtracting one during wrap is exact by Sterbenz's lemma: the two operands are within a factor of two. The reference's final division adds at most `u`. Therefore the circular phase allowance is:

```text
E_phase(n) = n*u*(frequency/sample_rate) + n*u + u
```

The tests also require the actual accumulator to remain in `[0,1)`. A wrap discrepancy at zero cannot be hidden by accepting an out-of-range value as the same circular angle. At one hour/96 kHz/10 kHz this conservative bound is below 4.3e-8 cycles (below 4.3e-12 seconds of carrier phase). It is a forward-roundoff bound, not an estimate fitted to observed errors or a promise of correct rounding for arbitrary oscillators. Existing validation tests separately retain extremely small/underflowing step cases; this normalized-step derivation must not be applied to subnormal division without its absolute underflow term.

Since sine is 1-Lipschitz in radians, the sample comparison allows `TAU*E_phase + 32*u`. The separate `32*u` bounded-angle allowance covers argument multiplication and the portable/native sine comparison. The pinned libm kernel sources (`src/math/k_sin.rs`, `k_cos.rs`, `sin.rs`) document polynomial approximation error at most 2^-58 on the reduced interval; floating evaluation, argument reduction and the independent system-math reference also round. This allowance is an explicit characterization assumption for that bounded path, not a formal all-platform correctly-rounded-libm theorem. Platform execution remains necessary. No large-angle native sine reduction is used by the reference.

Pulse references use declared integer rate/sample rate and decimal 1 ms widths as rational boundaries, including neighbors of onset and off edges near the one-hour and `2^36` frame limits. For `q = n*rate/sample_rate`, step division plus absolute-index multiplication contributes at most `2*u*q` cycles; decimal-width representation and duty multiplication contribute at most `2*u*duty`. The reduced reference adds `u`. Floor and fractional subtraction add no further rounding in these ranges. Classification must agree except within that explicit distance of a start/off boundary. At the tested maximum indices this uncertainty is far below one sample; adjacent-frame edge selection is allowed, arbitrary timing drift is not. Exactly representable 1/8-cycle clocks separately require the strict half-open edge, including late indices. At 44.1 kHz the actual one-second 40 Hz train must contain 40 onsets with bounded sampled widths; every sample is checked against the rational edge rule.

Fade checks use an independent `sin²(pi/2 * depth/fade)` expression against the production `0.5 - 0.5*cos(pi*depth/fade)`, including first/last frames, the transition to unity, large fades and the maximum session index. The absolute allowance is `8*EPSILON` for division/argument/evaluation roundoff. Absolute error matters near zero, where cosine subtraction can lose relative precision. Frame/fade quantization remains checked through real validated sessions.

Core-private properties read the actual stored **linear** per-ear bound, avoiding a logarithmic display round trip. The 128-case strategy covers zero to sixteen components, tone/click carriers, plain/SAM/gated modulation, both/left/right routing, 1/8k/44.1k/48k/96k/768k sample rates, very low gains, short frame/fade boundaries, exact counts, post-completion zeros and bitwise partition independence. Fade and Nyquist rejection neighbors accompany accepted cases. Existing near-full-scale properties remain in place. Negative controls alter the actual private initial phase, oscillator step and pulse duty/rate, and shift the independent fade edge; these must disagree with the same baseline oracle.

Bounded end-to-end streams additionally render a shortened actual catalog tone-pip session at three rates, and explicitly qualify its full one-hour 48 kHz stream. Memory remains one 4,096-frame block. Selected samples use independently reduced declared carrier/gate/fade formulas, all samples obey the stored linear bound and ear identity, and rendered counts and terminal silence are checked. Pulse-edge samples are handled by the separate edge characterization rather than assumed exactly representable.

## Sink conversion is a separate contract

WAV tests independently check nearest binary32 ties-to-even, both signed zeros, the normal/subnormal boundary, subnormal midpoints and underflow. The absolute rounding bound is `max(abs(x)*2^-24, 2^-150)`, including half the minimum binary32 subnormal; normal-only relative EPSILON bounds are insufficient near zero. Real encoded low-level tones demonstrate underflow, and a -20 dBFS crest demonstrates binary32 rounding slightly **above** its binary64 source value. A core generation bound is not an exact bound on the quantized sink representation.

Device tests retain supported formats and saturated endpoints. I16/I32 use their actual truncating conversion with one-LSB absolute spacing; I24 first uses saturated I32 and then discards low bits, including the resulting negative half-step behavior. Its conservative absolute spacing allowance is one I24 LSB plus one I32 LSB. These are not nearest-float rounding rules. Actual callback conversion checks first-two-channel placement and exact zero on every extra channel. Existing no-allocation, timing, I24 full-scale and independent WAV byte/decoder controls remain required.

## Run the explicit offline qualifications

Ordinary workspace tests discover the numerical/property/conversion checks. The long streams and timing report are explicitly ignored to keep routine CI bounded:

```sh
cargo test -p auditory-core --release --lib --locked hour_long_accumulation -- --ignored --nocapture
cargo test -p auditory-core --release --lib --locked the_longest_builtin_stream -- --ignored --nocapture
cargo test -p auditory-device --release --lib --locked warmed_production_callback_render_and_conversion_budget -- --ignored --nocapture
```

The timing test opens no device. It warms 16 blocks and measures 128 actual production `Callback::fill` blocks for each rate/block/component/modulation/channel/conversion combination: 44.1/48/96 kHz; 64/256/1,024 frames; one/four/sixteen voices; plain/SAM/gated tones; two/six channels; F64/F32/I32/I24/I16. This is 810 nonempty cases. All voices use valid -36 dBFS levels, and the actual stream I24 converter is reused. Empty/partial/completed workloads reject; every measured block must consume a full block of active renderer frames. Per-case output records active frames and nonzero block count, with extra-channel zeros checked outside timing. Silent tails cannot count as active throughput; legitimate gate-closed frames remain part of active synthesis.

Each case reports total/minimum/median/p95/maximum processing time and p95 processing/buffer-duration ratio. The report identifies profile, toolchain target, CPU and source inputs; set `AUDITORYLAB_BENCH_SOURCE` to the retained source-record identifier when collecting it. Preserve command output and source-file identities together. Timing is a measurement, not a wall-clock release gate. Clock-read overhead and ordinary OS load are included. No offline ratio establishes backend scheduling, device latency, acoustic timing or minimum-OS/native-platform qualification.

## Generated-signal observation

`Renderer::render` returns the exact source start, active prefix length and exclusive end as `RenderedSpan`; empty calls do not advance it, and the caller's remaining frames are silence. Device timing uses these authoritative spans. Monitoring reads the same mixed, faded stereo `f64` prefix before device conversion and channel placement. Generated samples may subsequently be silenced by a cancellation race; observation never establishes audible delivery or playback completion.

Shared core history retains at most 100 ms of indexed raw detail plus at most 2,048 source-zero-anchored coarse buckets spanning at most two seconds. Queries return no more than 1,024 bins with separate per-ear extrema and sample peaks. Detail uses retained raw samples; peak ranges recombine whole retained buckets and the actual trailing prefix, without applying coarse extrema to arbitrarily clipped intervals. Missing and partially observed intervals remain explicit; they are not silence. Feeding preallocated history allocates nothing.

The optional device path uses reviewed `rtrb` 0.4.0 safe SPSC operations with fixed 1,024-frame stereo packets. Construction allocates before playback; callback publication never allocates, locks, waits, retries, logs or wakes the UI. Full or disconnected observation drops visual data immediately. A separate native atomic cursor still reports the latest generated source end; it is independent of packet coverage and terminal arbitration. The application supplies identity, bounded polling, stalled-source freshness and resume frontiers, and rejects monitoring configurations above the whole-product 8 MiB buffer-capacity limit.

Capacity is derived from actual types and the prepared source rate, not a fixed benchmark total. The app accounts for transport slots and endpoints, working packets, raw/coarse history, snapshot/query buffers and source context. It composes that capacity with an explicit caller-owned presentation reservation and rejects the combined construction above 8 MiB. The GUI separately checks actual String capacities, bounded formatting, pinned Slint conversion and live replacement copies. Independent consumers must provide and verify their own presentation reserve. See the [observation architecture](architecture.md#generated-signal-observation), [app accounting](../crates/auditory-app/src/observation.rs) and [GUI reservation](../crates/auditory-gui/src/signal_geometry.rs). These are buffer capacities, excluding allocator bookkeeping, process RSS, native graphics allocations and arbitrary additional snapshots retained by consumers.

Additional explicit offline reports:

```sh
cargo test -p auditory-device --release --lib --locked observed_production_callback_budget -- --ignored --nocapture
cargo test -p auditory-app --release --lib --locked maximum_rate_observation_and_snapshot_budget -- --ignored --nocapture
```

The paired callback report adds maximum-rate workloads and disabled/drained/full/disconnected observation modes. The full queue is filled before timing; drained packets are consumed outside callback timing. Allocation controls cover enabled, overflowing, unconsumed, disconnected, cancelled, malformed/timing-error and tail paths. Independent oracles cover source spans, anchored gaps, one-frame pulses that a stride reducer misses, gated 10 kHz signals, polarity inversion and silent ears. Pinned PCM digests and terminal/routing regressions independently protect the audio contract.

## Collect and interpret measurements

Run measurements only after their relevant inputs are stable. Retain the complete command, source identity, compiler/target/profile, CPU/OS, workload, output and any failure. A committed source identity is preferable; a local working-tree report must identify its actual inputs without claiming immutable release qualification. Keep temporary evidence under `target/` and preserve needed results separately before cleaning it.

The callback reports measure actual production synthesis, conversion and channel placement with nonempty active spans; the maximum-rate app report measures bounded transport consumption, reduction and snapshots. Neither measures GUI drawing or native device scheduling. Preserve nonzero-workload and active-frame checks when changing the reports. Compare observation modes for otherwise identical rates, block sizes, voices, signal shapes, channel counts and converters. Report tails and individual outliers as well as medians; callbacks can exceed their buffer duration under OS load, and an average or median below that duration is not a real-time deadline guarantee.

Capacity observations must distinguish app-owned bytes from the caller's presentation reservation, use the actual source context and construction, and state the accounting exclusions. A report with a zero presentation reserve measures the app portion only. A numerical sum with a GUI reserve does not measure GUI allocations or process RSS. Keep actual-capacity and rejection controls alongside timing reports.

For GUI resource comparisons, use the same packaged executable and controlled workload in separate GPU and forced-software processes. Establish the actual renderer's first frame, identify the window size/scale and signal complexity, and distinguish idle, live, frozen/reduced-motion, unfocused, hidden, minimized and occluded states. Flat silence is not a dense waveform workload. Record CPU, wakeups, memory and available power counters with their units, sampling interval and limitations; process-energy counters are not calibrated whole-system energy measurements. Lower CPU does not establish lower total energy or a universal GPU benefit.

Use silence for native audio tests and do not change volume, routing or devices to collect evidence. Offline source checks, window captures and process measurements do not establish physical GPU identity, assistive-technology behavior, execution on minimum OS versions, acoustic timing or sound pressure. The [verification guide](verification.md) keeps those evidence requirements distinct.

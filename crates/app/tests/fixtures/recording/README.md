# Recording decoder fixtures

`source.toml` authors a ten-second stereo 1000 Hz tone with 40 Hz sinusoidal modulation at depth 0.8 and a −20 dBFS peak bound. The sound contains symmetric sidebands at 960 and 1040 Hz, with no channel-exclusive pitch difference. Expected frequencies come from this recipe, independently of the analyzer.

Run `crates/app/tests/fixtures/recording/encode.sh` from the repository root to regenerate the fixtures. It exports the source with Auditory Lab at 48 kHz, then uses FFmpeg 9.0.2 to encode small 8 kHz fixtures. MP3, AAC/M4A and native Vorbis fixtures are each below 200 kB. FFmpeg is needed only for regeneration, not tests or runtime decoding. The source WAV is disposable and removed from `target/tmp/recording-codecs` by the script.

`tone.32bit.flac` deliberately uses FFmpeg's experimental 32-bit FLAC encoding. Symphonia 0.6.1's 32-bit stereo-side decoding needs a 33-bit sample reader and panics in debug builds. The adapter refuses this bit depth using the decoder's normalized codec parameters before decoding; tests retain this case on every profile. `no-audio.mp4` contains only a synthetic 16×16 video frame, to exercise the no-audio-track refusal. `tone.opus` exercises the explicit unsupported-codec refusal. All fixture media is synthetic; no third-party recording is distributed.

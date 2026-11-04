#!/bin/sh
# Run from the repository root. Synthetic source is exported by Auditory Lab;
# ffmpeg is needed only to regenerate these checked-in codec fixtures.
set -eu
scratch="$PWD/target/tmp/recording-codecs"
fixtures="$PWD/crates/app/tests/fixtures/recording"
mkdir -p "$scratch"
trap 'rm -rf "$scratch"' EXIT
cargo run --locked -p auditorylab-cli -- export --file "$fixtures/source.toml" --sample-rate 48000 --output "$scratch/source.wav"
ffmpeg -version | head -1
ffmpeg -v error -nostdin -y -i "$scratch/source.wav" -ar 8000 -sample_fmt s32 -bits_per_raw_sample 32 -strict -2 -c:a flac "$fixtures/tone.32bit.flac"
for spec in 'flac flac' 'mp3 libmp3lame' 'm4a aac' 'aac aac' 'ogg vorbis' 'alac.m4a alac' 'opus libopus' 'aiff pcm_s16be' 'caf pcm_s16le'; do
    set -- $spec
    sample_format="s16"
    if [ "$2" = alac ]; then sample_format="s16p"; fi
    if [ "$2" = aac ] || [ "$2" = vorbis ]; then sample_format="fltp"; fi
    if [ "$2" = libopus ]; then sample_format="flt"; fi
    if [ "$2" = libmp3lame ]; then sample_format="s16p"; fi
    ffmpeg -v error -nostdin -y -i "$scratch/source.wav" -ar 8000 -sample_fmt "$sample_format" -c:a "$2" -strict -2 "$fixtures/tone.$1"
done
ffmpeg -v error -nostdin -y -f lavfi -i color=size=16x16:duration=0.1 -an -c:v mpeg4 "$fixtures/no-audio.mp4"

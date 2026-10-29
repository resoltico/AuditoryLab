//! The bytes that precede the samples of a 32-bit float WAV file.
//!
//! A session's length is known before its first sample, so the header is
//! written once, complete, and the file needs no later correction.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use auditorylab_core::{SampleRate, Signal};
use std::fmt::{Display, Formatter};

const BYTES_PER_SAMPLE: u16 = 4;
const BITS_PER_SAMPLE: u16 = 32;
const FORMAT_IEEE_FLOAT: u16 = 3;
/// A 32-bit size field whose value is in the `ds64` chunk instead.
const SIZE_IN_DS64: u32 = u32::MAX;
const DS64_BYTES: u64 = 28;

/// The largest values the core's bounds give the header's fields.
const MAX_BLOCK_ALIGN: u64 = 2 * BYTES_PER_SAMPLE as u64;
const MAX_BYTE_RATE: u64 = SampleRate::MAX_HZ as u64 * MAX_BLOCK_ALIGN;
const MAX_PAYLOAD_BYTES: u64 = Signal::MAX_FRAMES * MAX_BLOCK_ALIGN;

// Every format and length the core accepts fits its header field, and the
// file's size fits 64 bits with room for the header.
const _: () = assert!(MAX_BLOCK_ALIGN <= u16::MAX as u64);
const _: () = assert!(MAX_BYTE_RATE <= u32::MAX as u64);
const _: () = assert!(MAX_PAYLOAD_BYTES <= u64::MAX / 2);

/// The WAV container variant of an exported file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// Classic RIFF/WAVE, whose 32-bit size fields limit it to 4 GiB.
    Riff,
    /// RF64, with 64-bit sizes for longer sessions.
    Rf64,
}

impl Display for Container {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Riff => "RIFF/WAVE",
            Self::Rf64 => "RF64",
        })
    }
}

/// The container chosen for a session and its complete header.
pub(crate) struct Layout {
    pub(crate) container: Container,
    pub(crate) header: Vec<u8>,
}

/// Lay out `frames` frames of 32-bit float samples: RIFF/WAVE when every
/// size fits 32 bits, RF64 otherwise.
pub(crate) fn layout(rate: SampleRate, frames: u64) -> Layout {
    let fmt = fmt_body(rate);
    let payload = frames * MAX_BLOCK_ALIGN;
    // "WAVE", then the fmt, fact and data chunks, each after an eight-byte chunk header.
    let riff_size = 4 + (8 + fmt.len() as u64) + (8 + 4) + 8 + payload;
    match (
        u32::try_from(riff_size),
        u32::try_from(payload),
        u32::try_from(frames),
    ) {
        (Ok(riff_size), Ok(payload), Ok(frames)) => Layout {
            container: Container::Riff,
            header: riff(&fmt, riff_size, payload, frames),
        },
        _ => Layout {
            container: Container::Rf64,
            header: rf64(&fmt, payload, frames),
        },
    }
}

/// Exact byte length of the file `layout` and the samples produce.
pub(crate) fn encoded_len(rate: SampleRate, frames: u64) -> u64 {
    layout(rate, frames).header.len() as u64 + frames * MAX_BLOCK_ALIGN
}

/// Stereo IEEE float WAVEFORMATEX.
fn fmt_body(rate: SampleRate) -> Vec<u8> {
    let mut body = Vec::with_capacity(18);
    body.extend_from_slice(&FORMAT_IEEE_FLOAT.to_le_bytes());
    body.extend_from_slice(&2_u16.to_le_bytes());
    body.extend_from_slice(&rate.hz().to_le_bytes());
    body.extend_from_slice(&(rate.hz() * 8).to_le_bytes());
    body.extend_from_slice(&8_u16.to_le_bytes());
    body.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    body.extend_from_slice(&0_u16.to_le_bytes());
    body
}

fn riff(fmt: &[u8], riff_size: u32, payload: u32, frames: u32) -> Vec<u8> {
    let mut header = Vec::with_capacity(80);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&riff_size.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    chunk(&mut header, *b"fmt ", fmt);
    chunk(&mut header, *b"fact", &frames.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&payload.to_le_bytes());
    header
}

fn rf64(fmt: &[u8], payload: u64, frames: u64) -> Vec<u8> {
    // "WAVE", then the ds64, fmt and data chunks, each after an eight-byte chunk header.
    let riff_size = 4 + (8 + DS64_BYTES) + (8 + fmt.len() as u64) + 8 + payload;
    let mut ds64 = Vec::with_capacity(28);
    ds64.extend_from_slice(&riff_size.to_le_bytes());
    ds64.extend_from_slice(&payload.to_le_bytes());
    ds64.extend_from_slice(&frames.to_le_bytes());
    // No further chunk has a 64-bit size.
    ds64.extend_from_slice(&0_u32.to_le_bytes());
    let mut header = Vec::with_capacity(104);
    header.extend_from_slice(b"RF64");
    header.extend_from_slice(&SIZE_IN_DS64.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    chunk(&mut header, *b"ds64", &ds64);
    chunk(&mut header, *b"fmt ", fmt);
    header.extend_from_slice(b"data");
    header.extend_from_slice(&SIZE_IN_DS64.to_le_bytes());
    header
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the chunk bodies built in this module are at most 40 bytes"
)]
fn chunk(header: &mut Vec<u8>, id: [u8; 4], body: &[u8]) {
    header.extend_from_slice(&id);
    header.extend_from_slice(&(body.len() as u32).to_le_bytes());
    header.extend_from_slice(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate() -> SampleRate {
        SampleRate::new(8_000).unwrap()
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn u64_at(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    /// Inspect a whole file against the RIFF, RF64 and IEEE float layouts,
    /// written out independently of the code that builds them.
    fn check_file(bytes: &[u8], payload: &[u8], container: Container, channels: u16) {
        let rf64 = container == Container::Rf64;
        let frames = payload.len() / (usize::from(channels) * 4);
        assert_eq!(&bytes[..4], if rf64 { b"RF64" } else { b"RIFF" });
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(
            u32_at(bytes, 4),
            if rf64 {
                u32::MAX
            } else {
                u32::try_from(bytes.len() - 8).unwrap()
            }
        );
        let mut offset = 12;
        let mut seen = Vec::new();
        while offset < bytes.len() {
            let id = &bytes[offset..offset + 4];
            let declared = usize::try_from(u32_at(bytes, offset + 4)).unwrap();
            let body = offset + 8;
            let length = if rf64 && id == b"data" {
                assert_eq!(declared, usize::try_from(u32::MAX).unwrap());
                payload.len()
            } else {
                declared
            };
            let chunk = &bytes[body..body + length];
            match id {
                b"ds64" => {
                    assert_eq!(length, 28);
                    assert_eq!(u64_at(chunk, 0), u64::try_from(bytes.len() - 8).unwrap());
                    assert_eq!(u64_at(chunk, 8), u64::try_from(payload.len()).unwrap());
                    assert_eq!(u64_at(chunk, 16), u64::try_from(frames).unwrap());
                    assert_eq!(u32_at(chunk, 24), 0);
                }
                b"fmt " => {
                    assert_eq!(&chunk[2..4], &channels.to_le_bytes());
                    assert_eq!(u32_at(chunk, 4), 8_000);
                    assert_eq!(u32_at(chunk, 8), 8_000 * u32::from(channels) * 4);
                    assert_eq!(&chunk[12..14], &(channels * 4).to_le_bytes());
                    assert_eq!(&chunk[14..16], &[32, 0]);
                    assert_eq!(length, 18);
                    assert_eq!(&chunk[..2], &[3, 0]);
                    assert_eq!(&chunk[16..18], &[0, 0]);
                }
                b"fact" => {
                    assert!(!rf64);
                    assert_eq!(length, 4);
                    assert_eq!(u32_at(chunk, 0), u32::try_from(frames).unwrap());
                }
                b"data" => assert_eq!(chunk, payload),
                _ => panic!("unexpected chunk"),
            }
            assert!(!seen.contains(&id));
            seen.push(id);
            offset = body + length + length % 2;
        }
        assert_eq!(offset, bytes.len());
        if rf64 {
            assert_eq!(seen.first(), Some(&&b"ds64"[..]));
        }
        assert!(seen.contains(&&b"fmt "[..]) && seen.last() == Some(&&b"data"[..]));
        assert_eq!(seen.contains(&&b"ds64"[..]), rf64);
        assert_eq!(seen.contains(&&b"fact"[..]), !rf64);
    }

    fn payload(channels: u16, frames: usize) -> Vec<u8> {
        [0.0_f32, -0.0, 0.25, -0.5]
            .into_iter()
            .cycle()
            .take(usize::from(channels) * frames)
            .flat_map(f32::to_le_bytes)
            .collect()
    }

    /// A small file in either container; `layout` itself chooses RF64 only past 4 GiB.
    fn file(container: Container, channels: u16, payload: &[u8]) -> Vec<u8> {
        let frames = u64::try_from(payload.len()).unwrap() / (u64::from(channels) * 4);
        let mut bytes = match container {
            Container::Riff => {
                let layout = layout(rate(), frames);
                assert_eq!(layout.container, Container::Riff);
                layout.header
            }
            Container::Rf64 => rf64(
                &fmt_body(rate()),
                u64::try_from(payload.len()).unwrap(),
                frames,
            ),
        };
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn both_containers_carry_stereo_float_layouts() {
        for channels in [2] {
            let payload = payload(channels, 2);
            for container in [Container::Riff, Container::Rf64] {
                check_file(
                    &file(container, channels, &payload),
                    &payload,
                    container,
                    channels,
                );
            }
        }
    }

    #[test]
    fn an_independent_decoder_reads_both_containers() {
        for channels in [2] {
            let payload = payload(channels, 3);
            for container in [Container::Riff, Container::Rf64] {
                let bytes = file(container, channels, &payload);
                let mut reader = waveadapter::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
                assert_eq!(reader.sample_rate(), 8_000);
                assert_eq!(reader.channels(), usize::from(channels));
                assert_eq!(reader.frames(), 3);
                assert_eq!(reader.sample_format(), Some(waveadapter::SampleFormat::F32));
                let mut decoded = Vec::new();
                assert_eq!(reader.read_raw_interleaved(3, &mut decoded).unwrap(), 3);
                assert_eq!(decoded, payload);
            }
        }
    }

    #[test]
    fn the_layout_check_rejects_corrupted_headers_counts_and_samples() {
        let payload = payload(2, 2);
        for container in [Container::Riff, Container::Rf64] {
            let original = file(container, 2, &payload);
            // Signature, size, format and rate fields, frame count, and payload.
            let positions = if container == Container::Riff {
                vec![0, 4, 20, 24, 46, original.len() - 1]
            } else {
                vec![0, 4, 20, 28, 36, 56, 60, original.len() - 1]
            };
            for position in positions {
                let mut bytes = original.clone();
                bytes[position] ^= 1;
                assert!(
                    std::panic::catch_unwind(|| check_file(&bytes, &payload, container, 2))
                        .is_err(),
                    "corruption at {position} escaped"
                );
            }
        }
    }

    #[test]
    fn riff_is_chosen_up_to_its_capacity_and_rf64_beyond() {
        let channels = 2_u16;
        // Independent arithmetic: the RIFF size field counts all but the first eight bytes.
        let header_bytes: u64 = 58;
        let capacity = (u64::from(u32::MAX) + 8 - header_bytes) / (u64::from(channels) * 4);
        let within = layout(rate(), capacity);
        assert_eq!(within.container, Container::Riff);
        assert_eq!(u64::try_from(within.header.len()).unwrap(), header_bytes);
        let beyond = layout(rate(), capacity + 1);
        assert_eq!(beyond.container, Container::Rf64);
        assert_eq!(&beyond.header[..4], b"RF64");
    }

    #[test]
    fn the_largest_session_has_consistent_64_bit_sizes() {
        let rate = SampleRate::new(SampleRate::MAX_HZ).unwrap();
        let layout = layout(rate, Signal::MAX_FRAMES);
        assert_eq!(layout.container, Container::Rf64);
        let header = &layout.header;
        // 12-byte prologue, 36-byte ds64 chunk, 26-byte fmt chunk, 8-byte data header.
        assert_eq!(header.len(), 82);
        let payload = (1_u64 << 36) * 8;
        assert_eq!(u64_at(header, 20), 74 + payload);
        assert_eq!(u64_at(header, 28), payload);
        assert_eq!(u64_at(header, 36), 1 << 36);
        // Sample rate, byte rate and block alignment at their largest.
        assert_eq!(u32_at(header, 60), 768_000);
        assert_eq!(u32_at(header, 64), 6_144_000);
        assert_eq!(&header[68..70], &8_u16.to_le_bytes());
        // The limits the compile-time assertions rest on are these same values.
        assert_eq!(MAX_BLOCK_ALIGN, 8);
        assert_eq!(MAX_BYTE_RATE, 6_144_000);
        assert_eq!(MAX_PAYLOAD_BYTES, payload);
    }

    #[test]
    fn containers_have_plain_names() {
        assert_eq!(Container::Riff.to_string(), "RIFF/WAVE");
        assert_eq!(Container::Rf64.to_string(), "RF64");
    }
}

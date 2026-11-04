//! Packet streaming through a single admitted file handle.
use crate::job::{Cancel, Progress};
use crate::recording::{
    AnalysisOptions, AnalyzedRecording, ContainerFormat, RecordingError, RecordingFile,
    measure::Measurement,
};
use crate::storage::{
    error::{FileError, IoFailure, IoOperation},
    input_admission,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use symphonia::core::{
    codecs::audio::{
        AudioDecoder, AudioDecoderOptions,
        well_known::{
            CODEC_ID_AAC, CODEC_ID_FLAC, CODEC_ID_MP1, CODEC_ID_MP2, CODEC_ID_MP3, CODEC_ID_OPUS,
            CODEC_ID_VORBIS,
        },
    },
    formats::{FormatOptions, FormatReader, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
};

struct Decoder {
    format: Box<dyn FormatReader>,
    audio: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
    channels: usize,
    frames: Option<u64>,
    codec: String,
    lossy: bool,
    packet: u64,
    samples: Vec<f32>,
}
fn media_error(packet: u64, error: symphonia::core::errors::Error) -> RecordingError {
    match error {
        symphonia::core::errors::Error::Unsupported(message)
            if message.starts_with("aac:") || message.starts_with("adts:") =>
        {
            RecordingError::UnsupportedCodec {
                codec: "AAC stream (AAC-LC only)".into(),
            }
        }
        error => decode_error(packet, error),
    }
}
fn decode_error(packet: u64, error: impl std::fmt::Display) -> RecordingError {
    RecordingError::Decode {
        packet,
        cause: error.to_string(),
    }
}
impl Decoder {
    fn new(mut file: File) -> Result<Self, RecordingError> {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| decode_error(0, error))?;
        let mut head = [0; 3];
        file.read_exact(&mut head)
            .map_err(|error| decode_error(0, error))?;
        // Probe may skip an unsupported first ADTS header and find a later LC
        // frame. Preserve that refusal; the decoder patch checks subsequent headers.
        if head[0] == 0xFF && head[1] & 0xF6 == 0xF0 && head[2] >> 6 != 1 {
            return Err(RecordingError::UnsupportedCodec {
                codec: "AAC profile (AAC-LC only)".into(),
            });
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|error| decode_error(0, error))?;
        let format = symphonia::default::get_probe()
            .probe(
                &Hint::new(),
                MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|error| media_error(0, error))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or(RecordingError::NoAudioTrack)?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or(RecordingError::NoAudioTrack)?;
        if params.codec == CODEC_ID_OPUS {
            return Err(RecordingError::UnsupportedCodec {
                codec: "Opus".into(),
            });
        }
        if let Some(channels) = &params.channels
            && !(1..=2).contains(&channels.count())
        {
            return Err(RecordingError::UnsupportedChannels {
                channels: channels.count(),
            });
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|error| match error {
                symphonia::core::errors::Error::Unsupported(_) => {
                    RecordingError::UnsupportedCodec {
                        codec: if params.codec == CODEC_ID_AAC {
                            "AAC profile (AAC-LC only)".into()
                        } else {
                            params.codec.to_string()
                        },
                    }
                }
                error => decode_error(0, error),
            })?;
        let params = decoder.codec_params();
        // A 32-bit FLAC stereo difference channel needs 33 bits. Symphonia
        // 0.6.1 uses a 32-bit reader there (debug panic, invalid release shift).
        // Refuse this representation before entering the decoder on every profile.
        if params.codec == CODEC_ID_FLAC && params.bits_per_sample.is_some_and(|bits| bits > 24) {
            return Err(RecordingError::UnsupportedCodec {
                codec: "32-bit FLAC (convert to 16- or 24-bit PCM)".into(),
            });
        }
        let rate = params
            .sample_rate
            .ok_or_else(|| decode_error(0, "missing sample rate"))?;
        let channels = params
            .channels
            .as_ref()
            .map_or(0, symphonia::core::audio::Channels::count);
        if !(1..=2).contains(&channels) {
            return Err(RecordingError::UnsupportedChannels { channels });
        }
        if rate < 1000 {
            return Err(decode_error(0, "sample rate below 1000 Hz"));
        }
        let lossy = matches!(
            params.codec,
            CODEC_ID_AAC | CODEC_ID_MP3 | CODEC_ID_MP2 | CODEC_ID_MP1 | CODEC_ID_VORBIS
        );
        let codec = if params.codec == CODEC_ID_AAC {
            "AAC-LC".into()
        } else {
            decoder.codec_info().long_name.into()
        };
        // Native MPEG/ADTS readers may estimate num_frames from bitrate. A
        // counted pass establishes the exact timeline before choosing its 10% offset.
        let estimated = matches!(
            format.format_info().short_name,
            "aac" | "mp1" | "mp2" | "mp3"
        );
        let (id, frames) = (track.id, if estimated { None } else { track.num_frames });
        Ok(Self {
            format,
            audio: decoder,
            track: id,
            rate,
            channels,
            frames,
            codec,
            lossy,
            packet: 0,
            samples: Vec::new(),
        })
    }
    fn next(&mut self, cancel: &Cancel) -> Result<bool, RecordingError> {
        loop {
            if cancel.is_requested() {
                return Err(RecordingError::Cancelled);
            }
            let packet = self
                .format
                .next_packet()
                .map_err(|error| media_error(self.packet, error))?;
            let Some(packet) = packet else {
                return Ok(false);
            };
            self.packet += 1;
            if packet.track_id != self.track {
                continue;
            }
            let audio = self
                .audio
                .decode(&packet)
                .map_err(|error| media_error(self.packet, error))?;
            if audio.spec().rate() != self.rate || audio.spec().channels().count() != self.channels
            {
                return Err(decode_error(
                    self.packet,
                    "audio format changed within the recording",
                ));
            }
            self.samples.resize(audio.samples_interleaved(), 0.0);
            audio.copy_to_slice_interleaved(&mut self.samples);
            if self.samples.iter().any(|sample| !sample.is_finite()) {
                return Err(decode_error(self.packet, "nonfinite decoded sample"));
            }
            return Ok(true);
        }
    }
}

pub(crate) fn analyze(
    path: &Path,
    options: AnalysisOptions,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<AnalyzedRecording, RecordingError> {
    let options = options.validate()?;
    if cancel.is_requested() {
        return Err(RecordingError::Cancelled);
    }
    let failure =
        |action, error| FileError::Io(IoFailure::new(action, Some(path.to_owned()), error));
    let mut file = input_admission::open(path).map_err(|error| match error {
        input_admission::Failure::Io(error) => failure(IoOperation::Open, error),
        input_admission::Failure::NotRegular => FileError::NotRegular {
            path: path.to_owned(),
        },
    })?;
    let bytes = file
        .metadata()
        .map_err(|error| failure(IoOperation::Inspect, error))?
        .len();
    let mut head = [0; ContainerFormat::SIGNATURE_BYTES];
    let read = file
        .read(&mut head)
        .map_err(|error| failure(IoOperation::Read, error))?;
    let identity = RecordingFile {
        path: path.to_owned(),
        bytes,
        format: ContainerFormat::from_signature(&head[..read]),
    };
    let mut decoder = Decoder::new(
        file.try_clone()
            .map_err(|error| failure(IoOperation::Read, error))?,
    )?;
    // Some streams have no declared duration. Count them without retaining samples, then
    // decode the selected window from the same handle, never reopening a possibly replaced path.
    let total = if let Some(frames) = decoder.frames {
        frames
    } else {
        let mut frames = 0;
        while decoder.next(cancel)? {
            frames += (decoder.samples.len() / decoder.channels) as u64;
        }
        decoder = Decoder::new(file)?;
        frames
    };
    let start = total / 10;
    let requested = u64::from(options.seconds.unwrap_or(600)) * u64::from(decoder.rate);
    let end = start.saturating_add(requested).min(total);
    progress.set_total(end);
    let mut measurement = Measurement::new(decoder.rate, decoder.channels, end - start)?;
    let mut cursor = 0;
    while cursor < end && decoder.next(cancel)? {
        for frame in decoder.samples.chunks_exact(decoder.channels) {
            if cursor >= start && cursor < end {
                measurement.push(frame)?;
            }
            cursor += 1;
        }
        progress.set(cursor.min(end));
    }
    if cursor < end {
        return Err(decode_error(
            decoder.packet,
            "audio ended before its declared duration",
        ));
    }
    if cancel.is_requested() {
        return Err(RecordingError::Cancelled);
    }
    let mut analysis = measurement.finish(cancel)?;
    analysis.codec = decoder.codec;
    analysis.lossy = decoder.lossy;
    analysis.duration_seconds = frames_seconds(total, decoder.rate);
    let offset = frames_seconds(start, decoder.rate);
    analysis.envelope_window.start_seconds += offset;
    analysis.envelope_window.end_seconds += offset;
    analysis.window.start_seconds = offset;
    analysis.window.end_seconds = frames_seconds(end, decoder.rate);
    Ok(AnalyzedRecording {
        file: identity,
        analysis,
    })
}
#[expect(
    clippy::cast_precision_loss,
    reason = "media frame counts converted to displayed seconds"
)]
fn frames_seconds(frames: u64, rate: u32) -> f64 {
    frames as f64 / f64::from(rate)
}

#[cfg(test)]
mod tests;

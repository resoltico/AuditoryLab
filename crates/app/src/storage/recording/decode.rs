//! Packet streaming through a single admitted file handle.
use crate::job::{Cancel, Progress, ProgressPhase};
use crate::recording::{
    AnalysisOptions, AnalyzedRecording, ContainerFormat, RecordingError, RecordingFile,
};
use crate::storage::{
    error::{FileError, IoFailure, IoOperation},
    input_admission,
};
use auditorylab_core::measurement::Measurement;
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
        // MPEG/ADTS frame counts may be bitrate estimates, never trusted endpoints.
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
            self.packet = self
                .packet
                .checked_add(1)
                .filter(|p| *p <= (1 << 24))
                .ok_or(RecordingError::BudgetExceeded { resource: "packet" })?;
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
            admit_decoded_frames(audio.samples_interleaved() / self.channels)?;
            self.samples.resize(audio.samples_interleaved(), 0.0);
            audio.copy_to_slice_interleaved(&mut self.samples);
            if self.samples.iter().any(|sample| !sample.is_finite()) {
                return Err(decode_error(self.packet, "nonfinite decoded sample"));
            }
            return Ok(true);
        }
    }
}

fn admit_decoded_frames(frames: usize) -> Result<(), RecordingError> {
    if frames > (1 << 22) {
        return Err(RecordingError::BudgetExceeded {
            resource: "decoded packet",
        });
    }
    Ok(())
}

pub(crate) fn analyze(
    path: &Path,
    options: AnalysisOptions,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<AnalyzedRecording, RecordingError> {
    progress.set_phase(ProgressPhase::Preparing);
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
    let analysis = measure_decoder(Decoder::new(file)?, options, cancel, progress)?;
    Ok(AnalyzedRecording::complete(identity, analysis))
}
#[derive(Debug, PartialEq, Eq)]
struct Selection {
    start: u64,
    end: u64,
    selected: u64,
    prefix_capacity: usize,
}
impl Selection {
    fn new(
        sample_rate: u32,
        channels: usize,
        source_frames: Option<u64>,
        options: AnalysisOptions,
    ) -> Result<Self, RecordingError> {
        let options = options.validate()?;
        let rate = u64::from(sample_rate);
        let start = u64::from(options.start_seconds)
            .checked_mul(rate)
            .filter(|f| *f <= (1 << 32))
            .ok_or(RecordingError::BudgetExceeded {
                resource: "skip frames",
            })?;
        let requested = u64::from(options.seconds.unwrap_or(600))
            .checked_mul(rate)
            .filter(|f| *f <= (1 << 32))
            .ok_or(RecordingError::BudgetExceeded {
                resource: "measurement frames",
            })?;
        let requested_end = start
            .checked_add(requested)
            .ok_or(RecordingError::BudgetExceeded {
                resource: "timeline",
            })?;
        let end = source_frames.map_or(requested_end, |total| requested_end.min(total));
        let selected = end
            .checked_sub(start)
            .filter(|n| *n >= rate * 2)
            .ok_or(RecordingError::TooShort)?;
        // Prefix is bounded by the desired FFT plan, not the whole selected window.
        let prefix_frames = (1_u64 << selected.min(1 << 20).ilog2()).min(1 << 20);
        let prefix_capacity = usize::try_from(prefix_frames)
            .map_err(|_| RecordingError::BudgetExceeded { resource: "prefix" })?
            .checked_mul(channels)
            .ok_or(RecordingError::BudgetExceeded { resource: "prefix" })?;
        Ok(Self {
            start,
            end,
            selected,
            prefix_capacity,
        })
    }
}

fn measure_decoder(
    mut decoder: Decoder,
    options: AnalysisOptions,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<crate::recording::Analysis, RecordingError> {
    let options = options.validate()?;
    let Selection {
        start,
        end,
        selected,
        prefix_capacity,
    } = Selection::new(decoder.rate, decoder.channels, decoder.frames, options)?;
    let rate = u64::from(decoder.rate);
    progress.set_total(end);
    progress.set_phase(if start == 0 {
        ProgressPhase::Processing
    } else {
        ProgressPhase::Skipping
    });
    let mut prefix = Vec::with_capacity(prefix_capacity);
    let mut measurement = None;
    let mut cursor = 0_u64;
    let mut measured = 0_u64;
    while cursor < end && decoder.next(cancel)? {
        for frame in decoder.samples.chunks_exact(decoder.channels) {
            if cursor >= end {
                break;
            }
            if cursor >= start {
                if cursor == start {
                    progress.set_phase(ProgressPhase::Processing);
                }
                if let Some(accumulator) = &mut measurement {
                    Measurement::push(accumulator, frame)?;
                } else {
                    prefix.extend_from_slice(frame);
                    if prefix.len() == prefix_capacity {
                        let mut accumulator =
                            Measurement::new(decoder.rate, decoder.channels, selected)?;
                        for buffered in prefix.chunks_exact(decoder.channels) {
                            accumulator.push(buffered)?;
                        }
                        prefix.clear();
                        measurement = Some(accumulator);
                    }
                }
                measured += 1;
            }
            cursor += 1;
            if cursor.is_multiple_of(4096) && cancel.is_requested() {
                return Err(RecordingError::Cancelled);
            }
        }
        progress.set(cursor);
    }
    if cursor < end && decoder.frames.is_some() {
        return Err(decode_error(
            decoder.packet,
            "audio ended before its declared selected endpoint",
        ));
    }
    if measured < rate * 2 {
        return Err(RecordingError::TooShort);
    }
    if cancel.is_requested() {
        return Err(RecordingError::Cancelled);
    }
    let accumulator = if let Some(accumulator) = measurement {
        accumulator
    } else {
        let mut accumulator = Measurement::new(decoder.rate, decoder.channels, measured)?;
        for frame in prefix.chunks_exact(decoder.channels) {
            accumulator.push(frame)?;
        }
        accumulator
    };
    progress.set_phase(ProgressPhase::Completing);
    let samples = accumulator.finish(&|| cancel.is_requested())?;
    if cancel.is_requested() {
        return Err(RecordingError::Cancelled);
    }
    let analysis = crate::recording::Analysis::complete(
        decoder.codec,
        decoder.lossy,
        decoder.frames,
        start,
        samples,
    )?;
    Ok(analysis)
}

#[cfg(test)]
mod tests;

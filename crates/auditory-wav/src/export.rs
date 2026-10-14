// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{Container, ExportError, header};
use auditory_core::{Renderer, SampleRate, Session};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Samples rendered and written at a time.
const BLOCK_FRAMES: usize = 4_096;

/// What a successful export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exported {
    /// Number of frames in the file.
    pub frames: u64,
    /// The sample rate the file states.
    pub sample_rate: SampleRate,
    /// Container variant chosen for that length.
    pub container: Container,
}

/// Write a session to a new 32-bit float WAV file.
///
/// The destination is created only if nothing has its name, so no existing
/// file or symbolic link is replaced. The complete header is written first,
/// then the samples as they are rendered, and the file is synced before
/// success is reported. Setting `cancel` stops the export before its next
/// block. An export that fails or is cancelled removes its file; until then,
/// and if the process is killed, the destination holds an incomplete file.
///
/// Callers must leave the destination entry and its parent directory unchanged
/// until export ends. Cleanup addresses that pathname; failed removal or forced
/// termination can leave incomplete output.
///
/// ```no_run
/// fn write(session: auditory_core::Session, path: &std::path::Path, cancel: &std::sync::atomic::AtomicBool) {
///     let _ = auditory_wav::export(session, path, cancel);
/// }
/// ```
/// ```compile_fail,E0308
/// fn write(renderer: auditory_core::Renderer, path: &std::path::Path, cancel: &std::sync::atomic::AtomicBool) {
///     let _ = auditory_wav::export(renderer, path, cancel);
/// }
/// ```
///
/// # Errors
/// Returns an error for an existing or uncreatable destination, cancellation,
/// or a failed write or sync.
pub fn export(
    session: Session,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<Exported, ExportError> {
    let frames = session.frames();
    let sample_rate = session.sample_rate();
    let layout = header::layout(sample_rate, frames);
    stop_if_cancelled(cancel)?;
    let mut file = File::create_new(destination).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            ExportError::DestinationExists
        } else {
            ExportError::Io(error)
        }
    })?;
    let written = write(&mut file, &layout.header, session.into_renderer(), cancel)
        .and_then(|()| file.sync_all().map_err(ExportError::Io));
    // Windows cannot remove a file that is still open.
    drop(file);
    match written {
        Ok(()) => Ok(Exported {
            frames,
            sample_rate,
            container: layout.container,
        }),
        Err(failure) => Err(discard(destination, failure)),
    }
}

/// Remove the incomplete file of a failed export.
fn discard(destination: &Path, failure: ExportError) -> ExportError {
    match fs::remove_file(destination) {
        Ok(()) => failure,
        Err(removal) if removal.kind() == io::ErrorKind::NotFound => failure,
        Err(removal) => ExportError::IncompleteFileLeft {
            failure: Box::new(failure),
            removal,
        },
    }
}

fn stop_if_cancelled(cancel: &AtomicBool) -> Result<(), ExportError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ExportError::Cancelled)
    } else {
        Ok(())
    }
}

fn write(
    output: &mut impl Write,
    header: &[u8],
    mut renderer: Renderer,
    cancel: &AtomicBool,
) -> Result<(), ExportError> {
    output.write_all(header)?;
    let mut samples = vec![[0.0; 2]; BLOCK_FRAMES];
    let mut bytes = Vec::with_capacity(samples.len() * 8);
    while !renderer.is_complete() {
        stop_if_cancelled(cancel)?;
        let rendered = renderer.render(&mut samples);
        bytes.clear();
        for sample in samples[..rendered].iter().flatten() {
            bytes.extend_from_slice(&nearest_f32(*sample).to_le_bytes());
        }
        output.write_all(&bytes)?;
    }
    stop_if_cancelled(cancel)
}

/// Round a binary64 sample within full scale to the nearest binary32.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the file holds binary32 samples; the cast rounds to the nearest one"
)]
const fn nearest_f32(sample: f64) -> f32 {
    sample as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditory_core::{Component, Ears, Stimulus};

    fn session(frames: u64) -> Session {
        Stimulus {
            duration_seconds: f64::from(u32::try_from(frames).unwrap()) / 8_000.0,
            fade_seconds: 0.0,
            components: vec![Component::Tone {
                ears: Ears::Both,
                frequency_hz: 500.0,
                peak_dbfs: -20.0,
                modulation: None,
            }],
        }
        .session(SampleRate::new(8_000).unwrap())
        .unwrap()
    }

    /// Accepts a number of bytes, then fails.
    struct Limited {
        accepted: Vec<u8>,
        limit: usize,
    }

    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.accepted.len() + bytes.len() > self.limit {
                return Err(io::Error::other("injected write failure"));
            }
            self.accepted.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_header_then_every_sample_is_written_as_the_nearest_binary32() {
        let layout = header::layout(session(3).sample_rate(), 3);
        let mut output = Limited {
            accepted: Vec::new(),
            limit: usize::MAX,
        };
        write(
            &mut output,
            &layout.header,
            session(3).into_renderer(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut expected = [[0.0; 2]; 3];
        session(3).into_renderer().render(&mut expected);
        let mut bytes = layout.header.clone();
        for sample in expected.into_iter().flatten() {
            bytes.extend_from_slice(&nearest_f32(sample).to_le_bytes());
        }
        assert_eq!(output.accepted, bytes);
        assert_eq!(nearest_f32(0.1).to_bits(), 0.1_f32.to_bits());
        assert_eq!(nearest_f32(-1.0), -1.0);
    }

    #[test]
    fn a_write_failure_in_the_header_or_in_any_block_is_reported() {
        let layout = header::layout(session(20_000).sample_rate(), 20_000);
        // Before the header, within the first block, and within the last block.
        for limit in [0, layout.header.len(), layout.header.len() + 70_000] {
            let mut output = Limited {
                accepted: Vec::new(),
                limit,
            };
            let result = write(
                &mut output,
                &layout.header,
                session(20_000).into_renderer(),
                &AtomicBool::new(false),
            );
            assert!(
                matches!(result, Err(ExportError::Io(_))),
                "limit {limit}: {result:?}"
            );
        }
    }

    /// Requests cancellation after the first payload write.
    struct Cancelling<'flag> {
        file: File,
        cancel: &'flag AtomicBool,
        calls: usize,
    }
    impl Write for Cancelling<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.file.write_all(bytes)?;
            self.calls += 1;
            if self.calls == 2 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }
    #[test]
    fn cancellation_during_a_payload_stops_before_next_block_or_final_success_and_cleans_up() {
        for frames in [10, 5_000] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("cancelled.wav");
            let cancel = AtomicBool::new(false);
            let layout = header::layout(session(frames).sample_rate(), frames);
            let mut writer = Cancelling {
                file: File::create_new(&path).unwrap(),
                cancel: &cancel,
                calls: 0,
            };
            let result = write(
                &mut writer,
                &layout.header,
                session(frames).into_renderer(),
                &cancel,
            );
            assert!(matches!(result, Err(ExportError::Cancelled)));
            assert_eq!(writer.calls, 2);
            let expected = 58 + frames.min(4_096) * 8;
            assert_eq!(writer.file.metadata().unwrap().len(), expected);
            drop(writer);
            assert!(matches!(
                discard(&path, result.unwrap_err()),
                ExportError::Cancelled
            ));
            assert!(!path.exists());
        }
    }

    #[test]
    fn discarding_removes_the_file_and_reports_one_it_cannot_remove() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("incomplete.wav");
        fs::write(&file, b"partial").unwrap();
        assert!(matches!(
            discard(&file, ExportError::Cancelled),
            ExportError::Cancelled
        ));
        assert!(!file.exists());
        // Already gone: the original failure stands.
        assert!(matches!(
            discard(&file, ExportError::Cancelled),
            ExportError::Cancelled
        ));
        // A directory cannot be removed as a file.
        let error = discard(directory.path(), ExportError::Cancelled);
        let ExportError::IncompleteFileLeft { failure, .. } = &error else {
            panic!("removal failure was not reported: {error:?}");
        };
        assert!(matches!(**failure, ExportError::Cancelled));
        assert!(
            error
                .to_string()
                .starts_with("export cancelled before completion; ")
        );
        assert!(error.to_string().contains("could not be removed"));
        assert!(directory.path().is_dir());
    }
}

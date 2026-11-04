//! Bounded identification of a recording file: its size and leading signature.
mod decode;
use super::input_admission;
use crate::recording::{ContainerFormat, RecordingFile};
use crate::storage::error::{FileError, IoFailure, IoOperation};
pub(crate) use decode::analyze;
use std::{io::Read, path::Path};

pub(crate) fn identify(path: &Path) -> Result<RecordingFile, FileError> {
    let failure =
        |action, error| FileError::Io(IoFailure::new(action, Some(path.to_owned()), error));
    let file = input_admission::open(path).map_err(|error| match error {
        input_admission::Failure::Io(error) => failure(IoOperation::Open, error),
        input_admission::Failure::NotRegular => FileError::NotRegular {
            path: path.to_owned(),
        },
    })?;
    let bytes = file
        .metadata()
        .map_err(|error| failure(IoOperation::Inspect, error))?
        .len();
    let mut head = Vec::with_capacity(ContainerFormat::SIGNATURE_BYTES);
    file.take(ContainerFormat::SIGNATURE_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(|error| failure(IoOperation::Read, error))?;
    Ok(RecordingFile {
        path: path.to_owned(),
        bytes,
        format: ContainerFormat::from_signature(&head),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_reports_size_and_signature_and_refuses_non_files() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("tone.wav");
        std::fs::write(&wav, b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0").unwrap();
        assert_eq!(
            identify(&wav).unwrap(),
            RecordingFile {
                path: wav.clone(),
                bytes: 20,
                format: ContainerFormat::Wav,
            }
        );
        let empty = dir.path().join("empty.m4a");
        std::fs::write(&empty, b"").unwrap();
        let empty = identify(&empty).unwrap();
        assert_eq!((empty.bytes, empty.format), (0, ContainerFormat::Unknown));
        // Unix opens a directory and admission refuses it; Windows refuses the open itself.
        #[cfg(unix)]
        assert!(matches!(
            identify(dir.path()),
            Err(FileError::NotRegular { path }) if path == dir.path()
        ));
        #[cfg(windows)]
        assert!(matches!(
            identify(dir.path()),
            Err(FileError::Io(failure)) if failure.action == IoOperation::Open
        ));
        assert!(matches!(
            identify(&dir.path().join("missing.wav")),
            Err(FileError::Io(failure)) if failure.action == IoOperation::Open
        ));
    }
}

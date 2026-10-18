//! One operation thread and its progress channel.
use auditorylab::{Cancel, OutputInfo, Preset, Progress, Sound};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};
pub(crate) enum Message {
    Ready(OutputInfo, Sound),
    Imported(Result<Preset, auditorylab::Error>),
    Done(Result<String, auditorylab::Error>),
}
pub(crate) enum Work {
    Import(PathBuf),
    Play(Preset),
    Export(Sound, PathBuf),
}
pub(crate) struct Worker {
    pub(crate) cancel: Cancel,
    pub(crate) progress: Progress,
    pub(crate) messages: Receiver<Message>,
    handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub(crate) fn start(work: Work) -> Self {
        let cancel = Cancel::default();
        let progress = Progress::default();
        let c = cancel.clone();
        let p = progress.clone();
        let (tx, messages) = mpsc::channel();
        let handle = Some(thread::spawn(move || match work {
            Work::Import(path) => {
                let _ = tx.send(Message::Imported(auditorylab::load_file(&path)));
            }
            Work::Play(preset) => {
                let result = auditorylab::prepare_playback(&preset)
                    .and_then(|playback| {
                        let _ = tx.send(Message::Ready(
                            playback.output().clone(),
                            playback.sound().clone(),
                        ));
                        playback.play(&c, &p)
                    })
                    .map(|played| {
                        if played.underruns > 0 {
                            auditorylab::Notice::Underruns(played.underruns).to_string()
                        } else {
                            "Playback complete.".into()
                        }
                    });
                let _ = tx.send(Message::Done(result));
            }
            Work::Export(sound, path) => {
                let result =
                    auditorylab::export(&sound, &path, auditorylab::Replace::Allowed, &c, &p)
                        .map(|_| format!("Exported WAV to {path:?}."));
                let _ = tx.send(Message::Done(result));
            }
        }));
        Self {
            cancel,
            progress,
            messages,
            handle,
        }
    }
    pub(crate) fn join(mut self) -> Result<(), String> {
        self.handle.take().map_or(Ok(()), |h| {
            h.join().map_err(|_| "The operation thread failed.".into())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_export_joins_and_removes_the_partial_file() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base).unwrap();
        let dir = tempfile::tempdir_in(base).unwrap();
        let path = dir.path().join("cancel.wav");
        let preset = auditorylab::load_reader(
            b"schema_version=2\nduration_seconds=1000000\nfade_seconds=0\n".as_slice(),
            "silence",
        )
        .unwrap();
        assert!(preset.stimulus().components.is_empty());
        let worker = Worker::start(Work::Export(
            preset.at(auditorylab::SampleRate::DEFAULT).unwrap(),
            path.clone(),
        ));
        worker.cancel.request();
        let message = worker
            .messages
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            matches!(message, Message::Done(Err(ref e)) if e.kind() == auditorylab::ErrorKind::Cancelled)
        );
        worker.join().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

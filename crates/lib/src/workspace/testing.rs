//! Controlled launch fixtures for tests of real workspace commands and frontend bindings.
//! Enabled only for unit tests or the nondefault `test-support` feature. Workers
//! retain real threads/channels/join handles; this fixture controls when their
//! captured work runs, produces a result and exits. It never opens an audio device.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::time::Duration;

enum Step {
    Run(Sender<()>),
    Fail(Sender<()>),
    Exit,
    Panic,
}

fn acknowledged(sender: &Sender<Step>, fail: bool) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    sender
        .send(if fail { Step::Fail(tx) } else { Step::Run(tx) })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())
}

/// A file task launched by a real workspace command.
pub struct FileDriver {
    steps: Sender<Step>,
    kind: FileKind,
}
impl FileDriver {
    /// The captured destination or import source path.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        match &self.kind {
            FileKind::Import(path)
            | FileKind::Save { path, .. }
            | FileKind::Export { path, .. } => path,
        }
    }
    /// The immutable recipe captured by Save, if this is a Save.
    #[must_use]
    pub fn captured_recipe(&self) -> Option<&Recipe> {
        match &self.kind {
            FileKind::Save { content, .. } => Some(content),
            _ => None,
        }
    }
    /// Whether cancellation was requested for this export.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        matches!(&self.kind, FileKind::Export { cancel, .. } if cancel.is_requested())
    }
    /// Execute the captured filesystem operation and queue its result, retaining the worker.
    /// Returns a channel/timeout error if the worker cannot acknowledge it.
    pub fn result(&self) -> Result<(), String> {
        acknowledged(&self.steps, false)
    }
    /// Queue an injected operation failure, retaining the worker.
    /// Returns a channel/timeout error if the worker cannot acknowledge it.
    pub fn fail(&self) -> Result<(), String> {
        acknowledged(&self.steps, true)
    }
    /// Exit without another result. Returns an error if the worker already ended.
    pub fn exit(&self) -> Result<(), String> {
        self.steps.send(Step::Exit).map_err(|e| e.to_string())
    }
    /// Panic before or after a result. Returns an error if the worker already ended.
    pub fn panic(&self) -> Result<(), String> {
        self.steps.send(Step::Panic).map_err(|e| e.to_string())
    }
}

/// A simulated audio task launched through real workspace admission.
pub struct AudioDriver {
    steps: Sender<Step>,
}
impl AudioDriver {
    /// Queue successful completion or an accepted Stop, without releasing the worker.
    /// Returns a channel/timeout error if the worker cannot acknowledge it.
    pub fn result(&self) -> Result<(), String> {
        acknowledged(&self.steps, false)
    }
    /// Queue a backend failure without releasing the worker.
    /// Returns a channel/timeout error if the worker cannot acknowledge it.
    pub fn fail(&self) -> Result<(), String> {
        acknowledged(&self.steps, true)
    }
    /// Exit without another result. Returns an error if the worker already ended.
    pub fn exit(&self) -> Result<(), String> {
        self.steps.send(Step::Exit).map_err(|e| e.to_string())
    }
    /// Panic before or after a result. Returns an error if the worker already ended.
    pub fn panic(&self) -> Result<(), String> {
        self.steps.send(Step::Panic).map_err(|e| e.to_string())
    }
}

/// Test-side controls for tasks launched through the production command paths.
pub struct Execution {
    files: Receiver<FileDriver>,
    audio: Receiver<AudioDriver>,
    fail_file: Arc<AtomicBool>,
    fail_audio: Arc<AtomicBool>,
}
impl Execution {
    /// Receive the next file task; returns a timeout after five seconds if none launched.
    pub fn file(&self) -> Result<FileDriver, mpsc::RecvTimeoutError> {
        self.files.recv_timeout(Duration::from_secs(5))
    }
    /// Receive the next audio task; returns a timeout after five seconds if none launched.
    pub fn audio(&self) -> Result<AudioDriver, mpsc::RecvTimeoutError> {
        self.audio.recv_timeout(Duration::from_secs(5))
    }
    /// Make the next admitted file launch fail before any thread starts.
    pub fn fail_file_launch(&self) {
        self.fail_file.store(true, Ordering::Relaxed);
    }
    /// Make the next admitted audio launch fail before any thread starts.
    pub fn fail_audio_launch(&self) {
        self.fail_audio.store(true, Ordering::Relaxed);
    }
}

/// Construct a workspace whose real command paths launch controllable workers.
/// Playback capability is simulated as requested; no physical device is opened.
/// Returns an error if editor document identities are exhausted.
pub fn workspace(
    catalog: Vec<RecipeSnapshot>,
    capabilities: Capabilities,
) -> Result<(Workspace, Execution), Error> {
    let mut workspace = Workspace::new(catalog, capabilities)?;
    workspace.capabilities = capabilities;
    let (file_tx, files) = mpsc::channel();
    let (audio_tx, audio) = mpsc::channel();
    let fail_file = Arc::new(AtomicBool::new(false));
    let fail_audio = Arc::new(AtomicBool::new(false));
    let file_failure = Arc::clone(&fail_file);
    workspace.file_spawner = Some(Box::new(move |kind| {
        if file_failure.swap(false, Ordering::Relaxed) {
            return tasks::Worker::failed();
        }
        let (steps, script) = mpsc::channel();
        let _ = file_tx.send(FileDriver {
            steps,
            kind: kind.clone(),
        });
        tasks::Worker::spawn(move |tx| {
            for step in script {
                let (result, ack) = match step {
                    Step::Run(ack) => (kind.clone().execute(), ack),
                    Step::Fail(ack) => {
                        let error = Error::new(ErrorKind::Write, "injected file failure");
                        let outcome = match kind {
                            FileKind::Import(_) => FileOutcome::Imported(Err(error)),
                            FileKind::Save { .. } => FileOutcome::Saved(Err(error)),
                            FileKind::Export { .. } => FileOutcome::Exported(Err(error)),
                        };
                        (outcome, ack)
                    }
                    Step::Exit => break,
                    Step::Panic => panic!("injected file worker panic"),
                };
                let _ = tx.send(result);
                let _ = ack.send(());
            }
        })
    }));
    let audio_failure = Arc::clone(&fail_audio);
    workspace.audio_spawner = Some(Box::new(move |session, _kind, _draft| {
        if audio_failure.swap(false, Ordering::Relaxed) {
            return tasks::Worker::failed();
        }
        let (steps, script) = mpsc::channel();
        let _ = audio_tx.send(AudioDriver { steps });
        tasks::Worker::spawn(move |tx| {
            for step in script {
                let (result, ack) = match step {
                    Step::Run(ack) => {
                        session
                            .state()
                            .finish_drained(crate::session::Outcome::Completed);
                        let report = session.state().report().ok_or_else(|| {
                            Error::new(ErrorKind::Device, "missing test playback outcome")
                        });
                        (report, ack)
                    }
                    Step::Fail(ack) => (
                        Err(Error::new(ErrorKind::Device, "injected audio failure")),
                        ack,
                    ),
                    Step::Exit => break,
                    Step::Panic => panic!("injected audio worker panic"),
                };
                let _ = tx.send(AudioEvent::Finished(result));
                let _ = ack.send(());
            }
        })
    }));
    Ok((
        workspace,
        Execution {
            files,
            audio,
            fail_file,
            fail_audio,
        },
    ))
}

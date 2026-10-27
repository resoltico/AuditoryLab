//! The occupied audio slot, live binding and playback transitions.
use super::*;
pub(super) struct AudioState {
    pub(super) slot: Option<AudioTask>,
    pub(super) status: AudioStatus,
    pub(super) output: OutputState,
    pub(super) capabilities: Capabilities,
}
impl AudioState {
    pub(super) fn new(capabilities: Capabilities) -> Self {
        Self {
            slot: None,
            status: AudioStatus::Idle,
            output: OutputState::default(),
            capabilities,
        }
    }
    pub(super) const fn busy(&self) -> bool {
        self.slot.is_some()
    }
}

#[derive(Debug, Clone)]
pub(super) enum AudioRequest {
    Finite {
        snapshot: Box<RecipeSnapshot>,
        store: Option<crate::services::library::Library>,
    },
    Live {
        document: DocumentId,
        candidate: Arc<crate::document::editing::Candidate>,
    },
}

impl AudioState {
    pub(super) fn live_rate(&self, document_id: DocumentId) -> Option<SampleRate> {
        self.slot
            .as_ref()
            .filter(|t| matches!(t.kind, AudioKind::Live(document) if document == document_id))
            .and_then(|t| t.rate)
    }
    pub(super) fn submit_live(&mut self, editor: &mut EditorSession) {
        let Some(task) = self.slot.as_ref() else {
            return;
        };
        let (AudioKind::Live(document), Some(live), Some(rate)) =
            (&task.kind, &task.live, task.rate)
        else {
            return;
        };
        if *document != editor.document() {
            return;
        }
        let live = live.clone();
        let analysis = editor.committed_analysis(rate);
        if let Some(target) = analysis.target()
            && let Err(error) = live.update(target)
            && error != crate::audio::session::LiveUpdateError::SessionClosed
        {
            self.status = AudioStatus::LiveUpdate(error);
        }
    }
    pub(super) fn start(&mut self, id: TaskId, request: AudioRequest) -> Job {
        assert!(
            self.slot.is_none(),
            "audio slot is released only by its joined completion"
        );
        let session = Session::new();
        let control = session.control();
        let (kind, title, draft) = match request {
            AudioRequest::Finite { snapshot, store } => {
                let title = snapshot.title();
                let kind = match store {
                    Some(store) => AudioKind::Managed {
                        snapshot: *snapshot,
                        store,
                    },
                    None => AudioKind::Finite(*snapshot),
                };
                (kind, title, None)
            }
            AudioRequest::Live {
                document,
                candidate,
            } => (
                AudioKind::Live(document),
                candidate.draft().title().to_owned(),
                Some(candidate.draft().clone()),
            ),
        };
        self.output.unbind();
        self.status = AudioStatus::Idle;
        let job = Job::audio(id, session, kind.clone(), draft);
        self.slot = Some(AudioTask {
            id,
            kind,
            title,
            control,
            live: None,
            rate: None,
            protocol_failed: false,
        });

        job
    }
    pub(super) fn stop_admission(&self) -> Result<&PlaybackControl, Refusal> {
        self.slot
            .as_ref()
            .map(|task| &task.control)
            .ok_or(Refusal::NoAudio)
    }
    pub(super) fn stop(&self) -> Result<StopRequest, Refusal> {
        Ok(self.stop_admission()?.request_stop())
    }

    pub(super) fn finish(
        &mut self,
        task: &AudioTask,
        result: Result<Result<crate::audio::session::PlaybackReport, Error>, WorkerFailure>,
    ) -> Conclusion {
        let failed = task.protocol_failed || !matches!(&result, Ok(Ok(_)));
        self.status = match result {
            Ok(Ok(report)) if !task.protocol_failed => AudioStatus::Ended {
                report,
                monitored: self.output.reader().is_some(),
            },
            Ok(Err(error)) => AudioStatus::Failed(error),
            Err(error) => AudioStatus::WorkerFailed(error),
            Ok(Ok(_)) => AudioStatus::WorkerFailed(WorkerFailure::InvalidOutcome),
        };
        if failed {
            self.output.mark_failed(&task.title);
        }
        if failed {
            Conclusion::Failed
        } else {
            Conclusion::Succeeded
        }
    }
    pub(super) fn accept_prepared(
        &mut self,
        job: TaskId,
        reader: crate::audio::session::MonitorReader,
        rate: SampleRate,
        live: Option<crate::audio::session::LiveControl>,
    ) -> ResponseDisposition {
        let Some(task) = self.slot.as_mut().filter(|task| task.id == job) else {
            return ResponseDisposition::Ignored;
        };
        if task.rate.is_some()
            || task.protocol_failed
            || reader.descriptor().id != task.control.session()
            || reader.descriptor().output.sample_rate != rate
            || matches!(task.kind, AudioKind::Live(_)) != live.is_some()
            || live.as_ref().is_some_and(|control| {
                control.session() != task.control.session() || control.sample_rate() != rate
            })
        {
            task.protocol_failed = true;
            task.control.request_stop();
            self.status = AudioStatus::WorkerFailed(WorkerFailure::InvalidOutcome);
            return ResponseDisposition::Rejected;
        }
        task.rate = Some(rate);
        task.live = live;
        self.output.bind(reader);
        ResponseDisposition::Applied
    }
    pub(super) fn take_finished(&mut self, id: TaskId) -> Option<AudioTask> {
        if self.slot.as_ref().is_some_and(|task| task.id == id) {
            self.slot.take()
        } else {
            None
        }
    }
}

impl AudioState {
    pub(super) fn idle_admission(&self) -> Result<(), Refusal> {
        if self.busy() {
            Err(Refusal::AudioPlaying)
        } else {
            Ok(())
        }
    }
    pub(super) fn play_admission(
        &self,
        prompts: &Prompts,
        pending: bool,
        files: &FileState,
    ) -> Result<(), Refusal> {
        if !self.capabilities.device_playback {
            return Err(Refusal::DevicePlaybackUnavailable);
        }
        prompts.admit_start(pending)?;
        if files.exporting() {
            return Err(Refusal::Exporting);
        }
        Ok(())
    }
}

impl Workspace {
    /// The capabilities in effect.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.audio.capabilities
    }
    /// Whether an audio task occupies its slot.
    #[must_use]
    pub const fn audio_busy(&self) -> bool {
        self.audio.busy()
    }
    /// The bound live session's actual device rate, if any.
    #[must_use]
    pub fn live_rate(&self) -> Option<SampleRate> {
        self.audio.live_rate(self.editor.session.document())
    }
    /// Request Stop of the current audio session. Files are unaffected, and
    /// Stop stays available while prompts are open.
    ///
    /// # Errors
    /// Returns [`Refusal::NoAudio`] when nothing plays.
    pub fn stop(&mut self) -> Result<StopRequest, Refusal> {
        let result = self.audio.stop();
        self.changed();
        result
    }
    /// Play what the current view shows: the selected recipe to its end, or
    /// the open document live. Playing replaces current audio after it
    /// stops. Preparation is requested from fully parsed finite values; the
    /// definitive check happens at the device's actual rate before the
    /// stream starts, independently of the export rate.
    ///
    /// # Errors
    /// Returns the admission or input refusal.
    pub fn play(&mut self) -> Result<(), Refusal> {
        let result = self.play_inner();
        self.audio.status = match &result {
            Ok(()) => AudioStatus::Idle,
            Err(refusal) => AudioStatus::Refused(refusal.clone()),
        };
        self.changed();
        result
    }
    /// Invoke the persistent transport action for the current workspace.
    ///
    /// # Errors
    /// Returns the same admission refusal as Stop or Play.
    pub fn play_stop(&mut self) -> Result<(), Refusal> {
        if self.audio_busy() {
            self.stop().map(|_| ())
        } else {
            self.play()
        }
    }
    pub(super) fn play_inner(&mut self) -> Result<(), Refusal> {
        let request = self.audio_request()?;
        if self.audio.slot.is_some() {
            self.schedule(Action::StartAudio(request));
        } else {
            self.start_audio(request);
        }
        Ok(())
    }
    pub(super) fn audio_request(&mut self) -> Result<AudioRequest, Refusal> {
        let request = self.prepare_audio()?;
        if matches!(request, AudioRequest::Live { .. }) {
            self.editor.commit()?;
        }
        Ok(request)
    }
    /// Whether Play has a complete candidate and admission, before device discovery.
    pub(crate) fn play_readiness(&mut self) -> Result<(), Refusal> {
        self.prepare_audio().map(|_| ())
    }
    /// Stop control of the current audio session, for a native control that
    /// must stop exactly that session.
    #[must_use]
    pub fn audio_control(&self) -> Option<PlaybackControl> {
        self.audio.slot.as_ref().map(|task| task.control.clone())
    }
    /// Phase of the current audio session, if one is bound.
    #[must_use]
    pub fn audio_phase(&self) -> Option<PlaybackPhase> {
        self.audio.slot.as_ref().map(|t| t.control.phase())
    }
}

//! The occupied audio slot, live binding and playback transitions.
use super::{
    Arc, AudioCommand, AudioKind, AudioStatus, AudioTask, Capabilities, Conclusion, DocumentId,
    EditorState, Error, Job, MotionPreference, OutputState, PlaybackControl, RecipeSnapshot,
    Refusal, ResponseDisposition, SampleRate, Session, StopRequest, TaskId, View, WorkerFailure,
    monitor,
};
pub(super) struct AudioState {
    slot: Option<AudioTask>,
    status: AudioStatus,
    output: OutputState,
    capabilities: Capabilities,
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

    pub(super) fn live_rate(&self, document_id: DocumentId) -> Option<SampleRate> {
        self.slot
            .as_ref()
            .filter(|t| matches!(t.kind, AudioKind::Live(document) if document == document_id))
            .and_then(|t| t.rate)
    }
    pub(super) fn submit_live(&mut self, editor: &mut EditorState) {
        let Some(task) = self.slot.as_ref() else {
            return;
        };
        let (AudioKind::Live(document), Some(live), Some(rate)) =
            (&task.kind, &task.live, task.rate)
        else {
            return;
        };
        if *document != editor.session().document() {
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

    pub(super) fn idle_admission(&self) -> Result<(), Refusal> {
        if self.busy() {
            Err(Refusal::AudioPlaying)
        } else {
            Ok(())
        }
    }
    pub(super) fn play_admission(&self) -> Result<(), Refusal> {
        if !self.capabilities.device_playback {
            return Err(Refusal::DevicePlaybackUnavailable);
        }
        Ok(())
    }

    pub(super) const fn task(&self) -> Option<&AudioTask> {
        self.slot.as_ref()
    }
    pub(super) const fn status(&self) -> &AudioStatus {
        &self.status
    }
    pub(super) const fn output(&self) -> &OutputState {
        &self.output
    }
    pub(super) const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub(super) fn record_status(&mut self, status: AudioStatus) {
        self.status = status;
    }
    pub(super) fn set_motion(&mut self, preference: MotionPreference) -> bool {
        self.output.set_motion(preference)
    }
    pub(super) fn set_override(&mut self, held: Option<bool>) {
        self.output.set_override(held);
    }
    pub(super) fn set_waveform(&mut self, span_ms: u32, scale: monitor::WaveformScale) -> bool {
        self.output.set_waveform(span_ms, scale)
    }
    pub(super) fn unbind_output(&mut self) {
        self.output.unbind();
    }
    pub(super) fn refresh(
        &mut self,
        editor: &mut EditorState,
        selected: Option<&RecipeSnapshot>,
        view: View,
        rate: SampleRate,
    ) -> bool {
        self.output
            .refresh(editor, selected, view, rate, self.busy())
    }
    pub(super) fn refresh_model(
        &mut self,
        editor: &mut EditorState,
        selected: Option<&RecipeSnapshot>,
        view: View,
        live_document: bool,
        rate: SampleRate,
    ) -> bool {
        self.output
            .refresh_model(editor, selected, view, live_document, rate)
    }

    pub(super) fn apply(&mut self, command: AudioCommand) -> Result<AudioTransition, Refusal> {
        match command {
            AudioCommand::Play => Ok(AudioTransition::Play),
            AudioCommand::PlaySelected => Ok(AudioTransition::PlaySelected),
            AudioCommand::Toggle if !self.busy() => Ok(AudioTransition::Play),
            AudioCommand::Toggle | AudioCommand::Stop => {
                self.stop().map(|_| AudioTransition::Stopped)
            }
        }
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

#[cfg(test)]
impl AudioState {
    pub(super) fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
    }
}

pub(super) enum AudioTransition {
    Play,
    PlaySelected,
    Stopped,
}

impl AudioState {
    pub(super) fn unrelated_title(&self, document: DocumentId) -> Option<String> {
        self.slot
            .as_ref()
            .filter(|task| !matches!(task.kind, AudioKind::Live(owner) if owner == document))
            .map(|task| task.title.clone())
    }
}

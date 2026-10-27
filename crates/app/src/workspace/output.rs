//! The output view: a static preview of the current source, or the bound
//! session's monitored capture, and the hold policy.
//!
//! Work happens only when its inputs change. A preview is generated once per
//! source identity (snapshot, or document and committed revision), rate and
//! span; a scale change reuses its samples. A monitored capture is rebuilt
//! only for a new publication, span or scale. Invalid sources are cached as
//! their diagnosis. A held capture is immutable: later navigation, edits,
//! settings or sessions cannot relabel it.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{View, Workspace};
use crate::audio::session::MonitorReader;
use crate::audio::session::SessionId;
use crate::document::editing::EditorSession;
use crate::document::recipe::{RecipeSnapshot, SnapshotId};
use auditorylab_core::SampleRate;
use std::sync::Arc;
use {
    crate::workspace::monitor::CaptureError, crate::workspace::monitor::ModelSource,
    crate::workspace::monitor::ModelView, crate::workspace::monitor::WaveformCapture,
    crate::workspace::monitor::WaveformScale,
};

/// The platform's reduced-motion preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionPreference {
    /// Not known; live drawing is used.
    #[default]
    Unknown,
    /// Motion is allowed.
    Enabled,
    /// The platform asks to reduce motion; captures are held by default.
    Reduced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKey {
    Snapshot(SnapshotId),
    Draft(crate::document::editing::DocumentId, u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewKey {
    source: SourceKey,
    rate: SampleRate,
    span_ms: u32,
}

/// What the timing model explains: an accepted live revision, or a static
/// source at a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKey {
    Accepted(SessionId, u64),
    Source(SourceKey, SampleRate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CaptureKey {
    session: SessionId,
    sequence: u64,
    span_ms: u32,
    scale: WaveformScale,
}

/// The displayed capture and why there might be none.
#[derive(Debug, Clone)]
pub struct OutputView {
    /// The capture to draw: held, monitored or preview.
    pub capture: Option<Arc<WaveformCapture>>,
    /// Why nothing is drawn, or why a monitored window was not accepted.
    pub unavailable: Option<OutputIssue>,
    /// Whether the display is held.
    pub held: bool,
}

/// Why a selected sound cannot produce a preview at the diagnostic rate.
#[derive(Debug, Clone)]
pub enum PreviewIssue {
    /// The saved source failed rate-dependent realization.
    Snapshot(crate::document::error::RenditionError),
    /// The current draft has no valid preview target.
    Acoustic {
        /// Authored title at evaluation.
        title: String,
        /// Rate used by the preview.
        rate: SampleRate,
        /// Acoustic cause when evaluation produced one.
        error: Option<crate::core::SoundError>,
    },
}

#[derive(Default)]
pub(crate) struct OutputState {
    motion: MotionPreference,
    hold_override: Option<bool>,
    span_ms: u32,
    scale: Option<WaveformScale>,
    reader: Option<MonitorReader>,
    failed_title: Option<String>,
    monitored: Option<(CaptureKey, Arc<WaveformCapture>)>,
    capture_error: Option<CaptureError>,
    preview: Option<(PreviewKey, Result<Arc<WaveformCapture>, PreviewIssue>)>,
    scaled_preview: Option<(PreviewKey, WaveformScale, Arc<WaveformCapture>)>,
    held: Option<Arc<WaveformCapture>>,
    model: Option<(ModelKey, Option<Arc<ModelView>>)>,
    #[cfg(test)]
    pub(crate) previews: u64,
    #[cfg(test)]
    pub(crate) models: u64,
}

impl OutputState {
    pub(crate) fn span_ms(&self) -> u32 {
        if self.span_ms == 0 {
            crate::workspace::controls::DEFAULT_MONITOR_SPAN_MS
        } else {
            self.span_ms
        }
    }

    pub(crate) fn scale(&self) -> WaveformScale {
        self.scale
            .unwrap_or(crate::workspace::controls::DEFAULT_MONITOR_SCALE)
    }

    pub(crate) fn held(&self) -> bool {
        self.hold_override
            .unwrap_or(self.motion == MotionPreference::Reduced)
    }

    pub(crate) const fn hold_overridden(&self) -> Option<bool> {
        self.hold_override
    }

    pub(crate) const fn motion(&self) -> MotionPreference {
        self.motion
    }

    pub(crate) fn set_motion(&mut self, motion: MotionPreference) -> bool {
        let changed = self.motion != motion;
        self.motion = motion;
        if changed {
            self.apply_hold();
        }
        changed
    }

    pub(crate) fn set_override(&mut self, held: Option<bool>) {
        self.hold_override = held;
        self.apply_hold();
    }

    fn apply_hold(&mut self) {
        if self.held() {
            if self.held.is_none() {
                self.held = self.current();
            }
        } else {
            self.held = None;
        }
    }

    pub(crate) fn set_waveform(&mut self, span_ms: u32, scale: WaveformScale) -> bool {
        let changed = self.span_ms() != span_ms || self.scale() != scale;
        self.span_ms = span_ms;
        self.scale = Some(scale);
        changed
    }

    pub(crate) fn bind(&mut self, reader: MonitorReader) {
        self.reader = Some(reader);
        self.monitored = None;
        self.capture_error = None;
        self.failed_title = None;
    }

    pub(crate) fn unbind(&mut self) {
        self.reader = None;
        self.monitored = None;
        self.capture_error = None;
        self.failed_title = None;
    }

    pub(crate) fn mark_failed(&mut self, title: &str) {
        if self.reader.is_none() {
            self.failed_title = Some(title.to_owned());
        }
    }

    pub(crate) fn reader(&self) -> Option<&MonitorReader> {
        self.reader.as_ref()
    }

    pub(crate) fn failed_title(&self) -> Option<&str> {
        self.failed_title.as_deref()
    }

    pub(crate) fn model(&self) -> Option<&Arc<ModelView>> {
        self.model.as_ref().and_then(|(_, model)| model.as_ref())
    }

    /// Bring the timing model up to date: a bound live session's accepted
    /// revision while its document is open, otherwise the shown source at
    /// its rate. A model is built once per key; an invalid source caches its
    /// absence. Returns whether the model changed.
    pub(crate) fn refresh_model(
        &mut self,
        editor: &mut EditorSession,
        selected: Option<&RecipeSnapshot>,
        view: View,
        live_document: bool,
        rate: SampleRate,
    ) -> bool {
        let accepted = self
            .reader
            .as_ref()
            .filter(|r| live_document && view == View::Workbench && r.descriptor().frames.is_none())
            .map(|r| (r.status().session_id, r.accepted()));
        let key = match (accepted, view, selected) {
            (Some((session, (revision, _))), _, _) => ModelKey::Accepted(session, revision),
            (None, View::Workbench, _) => ModelKey::Source(
                SourceKey::Draft(editor.document(), editor.draft_revision()),
                rate,
            ),
            (None, View::Library, Some(snapshot)) => {
                ModelKey::Source(SourceKey::Snapshot(snapshot.id()), rate)
            }
            (None, View::Library, None) => {
                let changed = self.model.is_some();
                self.model = None;
                return changed;
            }
        };
        if self.model.as_ref().is_some_and(|(k, _)| *k == key) {
            return false;
        }
        #[cfg(test)]
        {
            self.models += 1;
        }
        let model = match (accepted, key) {
            (Some((_, (revision, target))), _) => Some((
                target,
                editor.draft().title().to_owned(),
                ModelSource::Accepted(revision),
            )),
            (None, ModelKey::Source(SourceKey::Draft(..), _)) => editor
                .committed_analysis(rate)
                .target()
                .map(|t| (*t, editor.draft().title().to_owned(), ModelSource::Draft)),
            (None, _) => selected.and_then(|snapshot| {
                snapshot.at(rate).ok().map(|sound| {
                    (
                        *sound.signal().target(),
                        snapshot.title(),
                        ModelSource::Recipe,
                    )
                })
            }),
        };
        self.model = Some((
            key,
            model.map(|(target, title, source)| Arc::new(ModelView::new(target, title, source))),
        ));
        true
    }

    fn current(&self) -> Option<Arc<WaveformCapture>> {
        if self.reader.is_some() {
            return self.monitored.as_ref().map(|(_, c)| Arc::clone(c));
        }
        self.scaled_preview.as_ref().map(|(_, _, c)| Arc::clone(c))
    }

    /// The view to draw.
    pub(crate) fn view(&self) -> OutputView {
        let held = self.held();
        if held && let Some(capture) = &self.held {
            return OutputView {
                capture: Some(Arc::clone(capture)),
                unavailable: None,
                held,
            };
        }
        let unavailable = if self.reader.is_some() {
            self.capture_error.map(OutputIssue::Capture)
        } else {
            match &self.preview {
                Some((_, Err(reason))) => Some(OutputIssue::Preview(reason.clone())),
                _ => None,
            }
        };
        OutputView {
            capture: self.current(),
            unavailable,
            held,
        }
    }

    /// Bring captures up to date with their inputs. Returns whether the
    /// displayed capture changed.
    pub(crate) fn refresh(
        &mut self,
        editor: &mut EditorSession,
        selected: Option<&RecipeSnapshot>,
        view: View,
        export_rate: SampleRate,
        audio: bool,
    ) -> bool {
        let span_ms = self.span_ms();
        let scale = self.scale();
        let before = self.current().map(|c| Arc::as_ptr(&c));
        if let Some(reader) = self.reader.as_mut() {
            reader.poll(span_ms);
            let key = CaptureKey {
                session: reader.status().session_id,
                sequence: reader.status().sequence,
                span_ms,
                scale,
            };
            if self.monitored.as_ref().is_none_or(|(k, _)| *k != key) {
                match WaveformCapture::monitored(reader, scale) {
                    Ok(Some(capture)) => {
                        self.monitored = Some((key, Arc::new(capture)));
                        self.capture_error = None;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.capture_error = Some(error);
                    }
                }
            }
        } else if !audio {
            self.refresh_preview(editor, selected, view, export_rate, span_ms, scale);
        }
        if !self.held() {
            self.held = None;
        } else if self.held.is_none() {
            self.held = self.current();
        }
        before != self.current().map(|c| Arc::as_ptr(&c))
    }

    fn refresh_preview(
        &mut self,
        editor: &mut EditorSession,
        selected: Option<&RecipeSnapshot>,
        view: View,
        rate: SampleRate,
        span_ms: u32,
        scale: WaveformScale,
    ) {
        let source = match view {
            View::Library => match selected {
                Some(snapshot) => SourceKey::Snapshot(snapshot.id()),
                None => {
                    self.preview = None;
                    self.scaled_preview = None;
                    return;
                }
            },
            View::Workbench => SourceKey::Draft(editor.document(), editor.draft_revision()),
        };
        let key = PreviewKey {
            source,
            rate,
            span_ms,
        };
        if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            #[cfg(test)]
            {
                self.previews += 1;
            }
            let result = match view {
                View::Library => {
                    let Some(snapshot) = selected else {
                        return;
                    };
                    snapshot
                        .at(rate)
                        .map(|sound| {
                            Arc::new(WaveformCapture::preview(
                                snapshot.title(),
                                sound.signal(),
                                span_ms,
                                WaveformScale::Fixed,
                            ))
                        })
                        .map_err(PreviewIssue::Snapshot)
                }
                View::Workbench => {
                    let analysis = editor.committed_analysis(rate);
                    match analysis.signal() {
                        Some(signal) => Ok(Arc::new(WaveformCapture::preview(
                            editor.draft().title().to_owned(),
                            signal,
                            span_ms,
                            WaveformScale::Fixed,
                        ))),
                        None => Err(PreviewIssue::Acoustic {
                            title: editor.draft().title().into(),
                            rate,
                            error: analysis.finite_error().cloned(),
                        }),
                    }
                }
            };
            self.preview = Some((key, result));
            self.scaled_preview = None;
        }
        if self
            .scaled_preview
            .as_ref()
            .is_none_or(|(k, s, _)| *k != key || *s != scale)
        {
            self.scaled_preview = match &self.preview {
                Some((_, Ok(capture))) => Some((key, scale, Arc::new(capture.with_scale(scale)))),
                _ => None,
            };
        }
    }
}

impl OutputState {
    pub(super) fn preview_admission(audio: &super::AudioState) -> Result<(), super::Refusal> {
        audio.idle_admission()
    }
}

impl Workspace {
    /// Record the platform's reduced-motion preference.
    pub fn set_motion_preference(&mut self, preference: MotionPreference) {
        if self.audio.output.set_motion(preference) {
            self.changed();
        }
    }
    /// Hold or release the displayed capture, overriding the platform
    /// preference until [`Workspace::follow_motion`].
    pub fn set_hold(&mut self, held: bool) {
        self.audio.output.set_override(Some(held));
        self.changed();
    }
    /// Follow the platform's motion preference again.
    pub fn follow_motion(&mut self) {
        self.audio.output.set_override(None);
        self.changed();
    }
    /// Choose the waveform span in milliseconds and its display scale.
    pub fn set_waveform(&mut self, span_ms: u32, scale: WaveformScale) {
        if self.audio.output.set_waveform(span_ms, scale) {
            self.changed();
        }
    }
    /// Stop showing the last session's output and preview the current source.
    pub fn preview_current(&mut self) {
        if self.preview_admission().is_ok() {
            self.audio.output.unbind();
            self.changed();
        }
    }
    /// The timing model of what the current view explains: the open
    /// document's accepted live revision, its committed content, or the
    /// selected recipe. `None` when that source is not valid.
    #[must_use]
    pub fn model_view(&self) -> Option<Arc<crate::workspace::monitor::ModelView>> {
        self.audio.output.model().cloned()
    }
}

/// Why a waveform is unavailable, retaining its source and diagnostic rate.
#[derive(Debug, Clone)]
pub enum OutputIssue {
    /// A copied sample window failed its boundary checks.
    Capture(CaptureError),
    /// The selected authored sound cannot produce a preview.
    Preview(PreviewIssue),
}

impl std::fmt::Display for PreviewIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}
impl std::error::Error for PreviewIssue {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Snapshot(cause) => Some(cause),
            Self::Acoustic {
                error: Some(cause), ..
            } => Some(cause),
            Self::Acoustic { error: None, .. } => None,
        }
    }
}
impl std::fmt::Display for OutputIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}
impl std::error::Error for OutputIssue {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Capture(cause) => cause,
            Self::Preview(cause) => cause,
        })
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    PreviewIssue {
        codes {
            Self::Acoustic { error: None, .. } => "preview.unavailable",
        }
        delegates {
            Self::Snapshot(cause) => cause,
            Self::Acoustic {
                error: Some(cause), ..
            } => cause,
        }
        params(this) {
            match this {
                Self::Acoustic { title, rate, .. } => {
                    crate::core::diagnostic_params! {"title"=>title.clone(),"diagnostic_rate_hz"=>rate.hz()}
                }
                _ => crate::core::Params::new(),
            }
        }
    }
    OutputIssue {
        codes {}
        delegates {
            Self::Capture(cause) => cause,
            Self::Preview(cause) => cause,
        }
    }
}

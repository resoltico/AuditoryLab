// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The output view: a static preview of the current source, or the bound
//! session's monitored capture, and the hold policy.
//!
//! Work happens only when its inputs change. A preview is generated once per
//! source identity (snapshot, or document and committed revision), rate and
//! span; a scale change reuses its samples. A monitored capture is rebuilt
//! only for a new publication, span or scale. Invalid sources are cached as
//! their diagnosis. A held capture is immutable: later navigation, edits,
//! settings or sessions cannot relabel it.
use super::View;
use crate::session::MonitorReader;
use crate::{EditorSession, RecipeSnapshot, SessionId, SnapshotId, WaveformCapture, WaveformScale};
use auditorylab_core::SampleRate;
use std::sync::Arc;

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
    Draft(crate::DocumentId, u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewKey {
    source: SourceKey,
    rate: SampleRate,
    span_ms: u32,
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
    pub unavailable: Option<String>,
    /// Whether the display is held.
    pub held: bool,
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
    capture_error: Option<String>,
    preview: Option<(PreviewKey, Result<Arc<WaveformCapture>, String>)>,
    scaled_preview: Option<(PreviewKey, WaveformScale, Arc<WaveformCapture>)>,
    held: Option<Arc<WaveformCapture>>,
    #[cfg(test)]
    pub(crate) previews: u64,
}

impl OutputState {
    pub(crate) fn span_ms(&self) -> u32 {
        if self.span_ms == 0 { 40 } else { self.span_ms }
    }

    pub(crate) fn scale(&self) -> WaveformScale {
        self.scale.unwrap_or(WaveformScale::Fixed)
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
            self.capture_error.clone()
        } else {
            match &self.preview {
                Some((_, Err(reason))) => Some(reason.clone()),
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
        catalog: &[RecipeSnapshot],
        selected: usize,
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
                        self.capture_error = Some(crate::presentation::capture_error_text(error));
                    }
                }
            }
        } else if !audio {
            self.refresh_preview(editor, catalog, selected, view, export_rate, span_ms, scale);
        }
        if !self.held() {
            self.held = None;
        } else if self.held.is_none() {
            self.held = self.current();
        }
        before != self.current().map(|c| Arc::as_ptr(&c))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The preview key's inputs are the editor, library selection, view, rate, span and scale; bundling them adds a type only for this call"
    )]
    fn refresh_preview(
        &mut self,
        editor: &mut EditorSession,
        catalog: &[RecipeSnapshot],
        selected: usize,
        view: View,
        rate: SampleRate,
        span_ms: u32,
        scale: WaveformScale,
    ) {
        let source = match view {
            View::Library => match catalog.get(selected) {
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
                    let snapshot = &catalog[selected];
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
                        .map_err(|e| e.to_string())
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
                        None => Err(analysis.finite_error().map_or_else(String::new, |e| {
                            crate::presentation::invalid_at_rate(editor.draft().title(), rate, e)
                        })),
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

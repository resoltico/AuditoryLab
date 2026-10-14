// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use auditory_core::Stimulus;
use std::fmt::{Display, Formatter};

/// What a listener should do before playback starts, one sentence per line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListeningAdvice {
    headphones: bool,
}

impl ListeningAdvice {
    /// The advice for playing `stimulus`: always a low starting volume, and
    /// headphones when components are routed to one ear only.
    #[must_use]
    pub fn for_stimulus(stimulus: &Stimulus) -> Self {
        Self {
            headphones: stimulus.uses_separate_ear_routing(),
        }
    }
}

impl Display for ListeningAdvice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Set the output volume low before playback: digital levels do not limit sound pressure."
        )?;
        if self.headphones {
            writeln!(f, "Use headphones to preserve the intended ear routing.")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ListeningAdvice;
    use auditory_core::{Component, Ears, Stimulus};

    fn tones(ears: &[Ears]) -> Stimulus {
        Stimulus {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: ears
                .iter()
                .map(|ears| Component::Tone {
                    ears: *ears,
                    frequency_hz: 250.0,
                    peak_dbfs: -20.0,
                    modulation: None,
                })
                .collect(),
        }
    }

    #[test]
    fn headphones_are_advised_only_for_separate_ear_routing() {
        for (ears, headphones) in [
            (&[][..], false),
            (&[Ears::Both, Ears::Both][..], false),
            (&[Ears::Left][..], true),
            (&[Ears::Both, Ears::Right][..], true),
        ] {
            let advice = ListeningAdvice::for_stimulus(&tones(ears)).to_string();
            assert!(advice.starts_with("Set the output volume low"), "{advice}");
            assert_eq!(advice.contains("headphones"), headphones, "{ears:?}");
            assert_eq!(advice.lines().count(), 1 + usize::from(headphones));
        }
    }
}

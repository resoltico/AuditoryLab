//! Listening checks of the output in use: left/right delivery and low tones.
//!
//! Software cannot observe which ear hears a channel or whether a speaker
//! reproduces a low tone. Each check plays a short tone and records what the
//! listener reports; [`Assessment`] interprets those answers.
use auditorylab_core::{ChannelRouting, ComponentForm, Finite, SoundSource, vocabulary};

/// One test tone of the device check, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceTest {
    /// A tone sent only to the left channel.
    LeftChannel,
    /// A tone sent only to the right channel.
    RightChannel,
    /// A 100 Hz tone on both channels.
    Tone100Hz,
    /// A 60 Hz tone on both channels.
    Tone60Hz,
    /// A 40 Hz tone on both channels.
    Tone40Hz,
}
vocabulary! {DeviceTest,"device_test" {
    LeftChannel => ("left_channel","Left channel","left channel"),
    RightChannel => ("right_channel","Right channel","right channel"),
    Tone100Hz => ("tone_100_hz","100 Hz tone","100 Hz tone"),
    Tone60Hz => ("tone_60_hz","60 Hz tone","60 Hz tone"),
    Tone40Hz => ("tone_40_hz","40 Hz tone","40 Hz tone"),
}}

/// What a listener reports after one test tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceAnswer {
    /// Heard on the left side only.
    Left,
    /// Heard on the right side only.
    Right,
    /// Heard on both sides.
    Both,
    /// Heard, without a side.
    Heard,
    /// Not heard.
    NotHeard,
}
vocabulary! {DeviceAnswer,"device_answer" {
    Left => ("left","Left only","left only"),
    Right => ("right","Right only","right only"),
    Both => ("both","Both sides","both sides"),
    Heard => ("heard","Heard it","heard it"),
    NotHeard => ("not_heard","Did not hear it","did not hear it"),
}}

/// Peak level of every test tone. One level lets low tones be compared with
/// each other at an unchanged device volume.
pub const TEST_PEAK_DBFS: f64 = -20.0;
/// Length of every test tone in seconds.
pub const TEST_SECONDS: f64 = 3.0;
/// Fade at each end of every test tone in seconds, so tones start and stop without a click.
pub const TEST_FADE_SECONDS: f64 = 0.1;
/// Frequency of the channel tests in hertz.
pub const CHANNEL_TONE_HZ: f64 = 440.0;

impl DeviceTest {
    /// Tone frequency in hertz.
    #[must_use]
    pub const fn frequency_hz(self) -> f64 {
        match self {
            Self::LeftChannel | Self::RightChannel => CHANNEL_TONE_HZ,
            Self::Tone100Hz => 100.0,
            Self::Tone60Hz => 60.0,
            Self::Tone40Hz => 40.0,
        }
    }
    /// Digital channels that carry the tone.
    #[must_use]
    pub const fn route(self) -> ChannelRouting {
        match self {
            Self::LeftChannel => ChannelRouting::Left,
            Self::RightChannel => ChannelRouting::Right,
            Self::Tone100Hz | Self::Tone60Hz | Self::Tone40Hz => ChannelRouting::Both,
        }
    }
    /// Whether this test asks where the tone was heard, rather than whether.
    #[must_use]
    pub const fn asks_side(self) -> bool {
        matches!(self, Self::LeftChannel | Self::RightChannel)
    }
    /// The answers this test accepts, in display order.
    #[must_use]
    pub const fn answers(self) -> &'static [DeviceAnswer] {
        if self.asks_side() {
            &[
                DeviceAnswer::Left,
                DeviceAnswer::Right,
                DeviceAnswer::Both,
                DeviceAnswer::NotHeard,
            ]
        } else {
            &[DeviceAnswer::Heard, DeviceAnswer::NotHeard]
        }
    }
    /// The single unmodulated tone this test plays.
    #[must_use]
    pub fn component(self) -> ComponentForm {
        ComponentForm {
            source: SoundSource::Tone {
                route: self.route(),
                peak_dbfs: finite(TEST_PEAK_DBFS),
                frequency_hz: finite(self.frequency_hz()),
            },
            modulation: None,
        }
    }
    /// A complete finite recipe for this test tone, titled by the test.
    ///
    /// # Errors
    /// Returns an identity or portable-contract failure.
    pub fn recipe(self) -> Result<auditorylab_core::Recipe, crate::services::OperationError> {
        let mut draft = crate::document::editing::Draft::new()?;
        draft.set_title(self.label().to_owned());
        draft
            .add_node(self.component())
            .unwrap_or_else(|_| unreachable!("one constructed tone fits an empty document"));
        draft.set_duration_seconds(finite(TEST_SECONDS));
        draft.set_fade_seconds(finite(TEST_FADE_SECONDS));
        Ok(draft.recipe()?)
    }
    /// The test tone as a captured recipe, attributed to the test.
    ///
    /// # Errors
    /// Returns an identity or portable-contract failure.
    pub fn snapshot(
        self,
    ) -> Result<crate::document::recipe::RecipeSnapshot, crate::services::OperationError> {
        Ok(crate::document::recipe::RecipeSnapshot::new(
            crate::document::recipe::Origin::Draft(self.label().to_owned()),
            self.recipe()?,
        ))
    }
    const fn index(self) -> usize {
        self as usize
    }
}

fn finite(value: f64) -> Finite {
    Finite::new(value).unwrap_or_else(|| unreachable!("finite device-check constants"))
}

/// The listener's answers, kept for the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceCheck {
    answers: [Option<DeviceAnswer>; DeviceTest::ALL.len()],
}

/// An answer the test does not offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnofferedAnswer {
    /// The test that was answered.
    pub test: DeviceTest,
    /// The refused answer.
    pub answer: DeviceAnswer,
}

impl DeviceCheck {
    /// The recorded answer for one test.
    #[must_use]
    pub const fn answer(&self, test: DeviceTest) -> Option<DeviceAnswer> {
        self.answers[test.index()]
    }
    /// Record an answer, replacing an earlier one for the same test.
    ///
    /// # Errors
    /// Refuses an answer the test does not offer.
    pub fn record(
        &mut self,
        test: DeviceTest,
        answer: DeviceAnswer,
    ) -> Result<(), UnofferedAnswer> {
        if !test.answers().contains(&answer) {
            return Err(UnofferedAnswer { test, answer });
        }
        self.answers[test.index()] = Some(answer);
        Ok(())
    }
    /// Forget every answer.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// Whether any test has an answer.
    #[must_use]
    pub fn started(&self) -> bool {
        self.answers.iter().any(Option::is_some)
    }
    /// Interpret the answers recorded so far.
    #[must_use]
    pub fn assessment(&self) -> Assessment {
        Assessment {
            channels: ChannelResult::from_answers(
                self.answer(DeviceTest::LeftChannel),
                self.answer(DeviceTest::RightChannel),
            ),
            low_tones: LowToneResult::from_answers(
                [
                    DeviceTest::Tone100Hz,
                    DeviceTest::Tone60Hz,
                    DeviceTest::Tone40Hz,
                ]
                .map(|test| (test, self.answer(test))),
            ),
        }
    }
}

/// What the answers establish about the output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Assessment {
    /// Left/right delivery.
    pub channels: ChannelResult,
    /// Low-tone reproduction.
    pub low_tones: LowToneResult,
}

/// Left/right delivery reported by the two channel tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelResult {
    /// At least one channel test has no answer.
    Incomplete,
    /// Each channel reached its own side.
    Separate,
    /// Each channel reached the opposite side.
    Swapped,
    /// Both channels were heard on both sides.
    Mixed,
    /// Neither channel was heard.
    Silent,
    /// Only one channel test was heard on its own side; the other was not heard.
    OneSide(DeviceTest),
    /// Any other combination of answers.
    Inconsistent,
}
impl ChannelResult {
    fn from_answers(left: Option<DeviceAnswer>, right: Option<DeviceAnswer>) -> Self {
        use DeviceAnswer as A;
        match (left, right) {
            (None, _) | (_, None) => Self::Incomplete,
            (Some(A::Left), Some(A::Right)) => Self::Separate,
            (Some(A::Right), Some(A::Left)) => Self::Swapped,
            (Some(A::Both), Some(A::Both)) => Self::Mixed,
            (Some(A::NotHeard), Some(A::NotHeard)) => Self::Silent,
            (Some(A::Left), Some(A::NotHeard)) => Self::OneSide(DeviceTest::LeftChannel),
            (Some(A::NotHeard), Some(A::Right)) => Self::OneSide(DeviceTest::RightChannel),
            _ => Self::Inconsistent,
        }
    }
    /// Whether explicit binaural pairs can reach separate ears on this output.
    #[must_use]
    pub const fn separates_ears(self) -> Option<bool> {
        match self {
            Self::Separate | Self::Swapped => Some(true),
            Self::Mixed | Self::Silent | Self::OneSide(_) => Some(false),
            Self::Incomplete | Self::Inconsistent => None,
        }
    }
}

/// Low-tone reproduction reported by the low-tone tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LowToneResult {
    /// At least one low-tone test has no answer.
    Incomplete,
    /// Every low tone was heard.
    AllHeard,
    /// The lowest tone heard; at least one other tone was not heard.
    LowestHeard {
        /// Lowest heard test.
        test: DeviceTest,
    },
    /// No low tone was heard.
    NoneHeard,
}
impl LowToneResult {
    fn from_answers(answers: [(DeviceTest, Option<DeviceAnswer>); 3]) -> Self {
        if answers.iter().any(|(_, answer)| answer.is_none()) {
            return Self::Incomplete;
        }
        let heard = |answer: &Option<DeviceAnswer>| *answer == Some(DeviceAnswer::Heard);
        let lowest = answers
            .iter()
            .filter(|(_, answer)| heard(answer))
            .map(|(test, _)| *test)
            .min_by(|a, b| a.frequency_hz().total_cmp(&b.frequency_hz()));
        match lowest {
            None => Self::NoneHeard,
            Some(_) if answers.iter().all(|(_, answer)| heard(answer)) => Self::AllHeard,
            Some(test) => Self::LowestHeard { test },
        }
    }
}

#[cfg(test)]
mod tests;

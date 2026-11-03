//! Device-check instructions and the meaning of the listener's answers.
use auditorylab_app::device_check::{
    Assessment, ChannelResult, DeviceTest, LowToneResult, TEST_PEAK_DBFS, TEST_SECONDS,
};

/// What the check is for and how to use it.
pub const INTRODUCTION: &str = "Check what your headphones or speakers deliver. Set the device \
    volume low, play each tone and choose what you heard. Keep the same volume for every tone.";

/// What one test plays and what it asks.
#[must_use]
pub fn test_prompt(test: DeviceTest) -> String {
    let tone = format!(
        "Plays a {} Hz tone for {} seconds at {} dBFS",
        test.frequency_hz(),
        TEST_SECONDS,
        TEST_PEAK_DBFS
    );
    match test {
        DeviceTest::LeftChannel => {
            format!("{tone} on the left channel only. Where did you hear it?")
        }
        DeviceTest::RightChannel => {
            format!("{tone} on the right channel only. Where did you hear it?")
        }
        DeviceTest::Tone100Hz | DeviceTest::Tone60Hz | DeviceTest::Tone40Hz => {
            format!("{tone} on both channels. Did you hear it?")
        }
    }
}

/// What the channel answers mean for binaural pairs.
#[must_use]
pub fn channel_result(result: ChannelResult) -> String {
    match result {
        ChannelResult::Incomplete => "Answer both channel tests to see whether left and right \
            reach separate ears."
            .into(),
        ChannelResult::Separate => "Left and right reach separate ears. Binaural pairs can work \
            on this output."
            .into(),
        ChannelResult::Swapped => "Left and right are swapped. Each ear still gets its own \
            channel, so binaural pairs can work, with their sides reversed."
            .into(),
        ChannelResult::Mixed => "Both ears hear both channels, as with a single speaker or a \
            mono audio setting. Binaural pairs cannot work on this output: each ear needs its \
            own tone."
            .into(),
        ChannelResult::Silent => "Neither channel was heard. Check the output device, its \
            connection and the volume, then play the tones again."
            .into(),
        ChannelResult::OneSide(test) => {
            let (heard, missing) = if test == DeviceTest::LeftChannel {
                ("left", "right")
            } else {
                ("right", "left")
            };
            format!(
                "Only the {heard} side made sound. Check the {missing} earpiece or speaker, the \
                connection and the balance setting. Binaural pairs cannot work until both sides do."
            )
        }
        ChannelResult::Inconsistent => "These answers do not match a known output layout. Play \
            both channel tests again."
            .into(),
    }
}

/// What the low-tone answers mean for sounds built on low tones.
#[must_use]
pub fn low_tone_result(result: LowToneResult) -> String {
    match result {
        LowToneResult::Incomplete => {
            "Answer the three low-tone tests to see how low this output reaches.".into()
        }
        LowToneResult::AllHeard => "You heard every low tone, down to 40 Hz.".into(),
        LowToneResult::LowestHeard { test } => format!(
            "The lowest tone you heard was {} Hz. Sounds built on lower tones may be faint or \
            missing on this output. Raising the volume to hear them makes everything else much \
            louder.",
            test.frequency_hz()
        ),
        LowToneResult::NoneHeard => "You heard none of the low tones. Small speakers and many \
            earbuds reproduce little at or below 100 Hz, so sounds built on such tones may be \
            faint or missing on this output."
            .into(),
    }
}

/// Both results, one per line.
#[must_use]
pub fn assessment(assessment: Assessment) -> String {
    format!(
        "{}\n{}",
        channel_result(assessment.channels),
        low_tone_result(assessment.low_tones)
    )
}

//! Authoritative semantic parameter labels, units and portable value rules.
macro_rules! parameter_schema {
    ($consumer:ident) => {
        $consumer! {
            parameters {
                component {
                    /// A tone carrier's frequency.
                    ToneFrequency => ("tone_frequency","Tone frequency","tone frequency",Hertz,
                        Some(crate::ValueRule::Positive));
                    /// A pulse source's repetition rate.
                    SourcePulseRate => ("source_pulse_rate","Source pulse rate",
                        "source pulse rate",Hertz,
                        Some(crate::ValueRule::Positive));
                    /// A pulse source's requested window duration.
                    SourcePulseDuration => ("source_pulse_duration","Source pulse duration",
                        "source pulse duration",Milliseconds,
                        Some(crate::ValueRule::Positive));
                    /// A primitive component's peak level.
                    PeakLevel => ("peak_level","Peak level","peak level",DecibelsFullScale,
                        Some(crate::ValueRule::Peak));
                    /// Sine amplitude modulation's rate.
                    ModulationRate => ("modulation_rate","Modulation rate","modulation rate",Hertz,
                        Some(crate::ValueRule::Positive));
                    /// Sine amplitude modulation's depth.
                    ModulationDepth => ("modulation_depth","Modulation depth","modulation depth",
                        Percent,
                        Some(crate::ValueRule::Depth));
                    /// Shared pulse modulation's repetition rate.
                    ModulationPulseRate => ("modulation_pulse_rate","Modulation pulse rate",
                        "modulation pulse rate",Hertz,
                        Some(crate::ValueRule::Positive));
                    /// Shared pulse modulation's requested window duration.
                    ModulationPulseDuration => ("modulation_pulse_duration",
                        "Modulation pulse duration","modulation pulse duration",Milliseconds,
                        Some(crate::ValueRule::Positive));
                    /// First/left carrier of an authored pair.
                    PairFirstFrequency => ("pair_first_frequency","First carrier","first carrier",
                        Hertz,Some(crate::ValueRule::Positive));
                    /// Signed nominal carrier offset of an authored pair.
                    PairOffset => ("pair_offset","Beat offset","beat offset",Hertz,None);
                    /// First/left peak level of an authored pair.
                    PairFirstLevel => ("pair_first_level","First peak","first peak",
                        DecibelsFullScale,Some(crate::ValueRule::Peak));
                    /// Second/right peak level of an authored pair.
                    PairSecondLevel => ("pair_second_level","Second peak","second peak",
                        DecibelsFullScale,Some(crate::ValueRule::Peak));
                    /// Source duty fraction.
                    SourceDuty => ("source_duty","Pulse duty","pulse duty",Percent,
                        Some(crate::ValueRule::Duty));
                    /// Source rise duration.
                    SourceRise => ("source_rise","Pulse rise","pulse rise",Milliseconds,
                        Some(crate::ValueRule::Positive));
                    /// Source fall duration.
                    SourceFall => ("source_fall","Pulse fall","pulse fall",Milliseconds,
                        Some(crate::ValueRule::Positive));
                    /// Shared modulation duty fraction.
                    ModulationDuty => ("modulation_duty","Modulation duty","modulation duty",
                        Percent,Some(crate::ValueRule::Duty));
                    /// Shared modulation rise duration.
                    ModulationRise => ("modulation_rise","Modulation rise","modulation rise",
                        Milliseconds,Some(crate::ValueRule::Positive));
                    /// Shared modulation fall duration.
                    ModulationFall => ("modulation_fall","Modulation fall","modulation fall",
                        Milliseconds,Some(crate::ValueRule::Positive));
                }
                envelope {
                    /// The finite length of saved and exported audio.
                    Duration => ("duration","Duration","duration",Seconds,None);
                    /// The edge fade of saved and exported audio.
                    Fade => ("fade","Fade","fade",Seconds,None);
                }
            }
        }
    };
}
pub(crate) use parameter_schema;

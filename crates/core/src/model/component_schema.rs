//! Component wire fields and the quantities they expose.
// Field declarations define decoder rejection order; kind rows define canonical
// field order. Shared fields are referenced rather than redeclared. Numeric
// meaning belongs to the parameter, not its wire name.
macro_rules! component_schema {
    ($consumer:ident)=>{$consumer! {
        parameters {
            component {
                /// A tone carrier's frequency.
                ToneFrequency => ("tone_frequency","Tone frequency","tone frequency",Hertz,
                    Some(crate::ValueRule::Positive));
                /// A click carrier's repetition rate.
                ClickRate => ("click_rate","Clicks rate","clicks rate",Hertz,
                    Some(crate::ValueRule::Positive));
                /// A click carrier's requested pulse length.
                ClickWidth => ("click_width","Clicks width","clicks width",Milliseconds,
                    Some(crate::ValueRule::Positive));
                /// A primitive component's peak level.
                PeakLevel => ("peak_level","Peak level","peak level",DecibelsFullScale,
                    Some(crate::ValueRule::Peak));
                /// Sine amplitude modulation's rate.
                AmRate => ("am_rate","Modulation rate","modulation rate",Hertz,
                    Some(crate::ValueRule::Positive));
                /// Sine amplitude modulation's depth.
                AmDepth => ("am_depth","Modulation depth","modulation depth",Percent,
                    Some(crate::ValueRule::Depth));
                /// Gating's repetition rate.
                GateRate => ("gate_rate","Gate rate","gate rate",Hertz,
                    Some(crate::ValueRule::Positive));
                /// Gating's requested open length.
                GateWidth => ("gate_width","Gate width","gate width",Milliseconds,
                    Some(crate::ValueRule::Positive));
                /// First/left carrier of an authored pair.
                PairFirstFrequency => ("pair_first_frequency","First / left carrier","first / \
                    left carrier",Hertz,Some(crate::ValueRule::Positive));
                /// Signed nominal carrier offset of an authored pair.
                PairOffset => ("pair_offset","Signed offset","signed offset",Hertz,None);
                /// First/left peak level of an authored pair.
                PairFirstLevel => ("pair_first_level","First / left peak","first / left peak",
                    DecibelsFullScale,Some(crate::ValueRule::Peak));
                /// Second/right peak level of an authored pair.
                PairSecondLevel => ("pair_second_level","Second / right peak","second / right peak",
                    DecibelsFullScale,Some(crate::ValueRule::Peak));
            }
            envelope {
                /// The finite length of saved and exported audio.
                Duration => ("duration","Duration","duration",Seconds,None);
                /// The edge fade of saved and exported audio.
                Fade => ("fade","Fade","fade",Seconds,None);
            }
        }
        fields {
            ears: crate::Ears;
            frequency_hz: crate::Finite => ToneFrequency;
            peak_dbfs: crate::Finite => PeakLevel;
            modulation: Option<crate::Modulation>;
            rate_hz: crate::Finite => ClickRate;
            width_seconds: crate::Finite => ClickWidth;
            left_hz: crate::Finite => PairFirstFrequency;
            offset_hz: crate::Finite => PairOffset;
            left_peak_dbfs: crate::Finite => PairFirstLevel;
            right_peak_dbfs: crate::Finite => PairSecondLevel;
            first_hz: crate::Finite => PairFirstFrequency;
            first_peak_dbfs: crate::Finite => PairFirstLevel;
            second_peak_dbfs: crate::Finite => PairSecondLevel;
        }
        kinds {
            Tone "tone" {
                route ears;
                quantities [frequency_hz];
                level peak_dbfs;
                modulation modulation;
            }
            Clicks "clicks" {
                route ears;
                quantities [rate_hz,width_seconds];
                level peak_dbfs;
                modulation modulation;
            }
            BinauralPair "binaural_pair" {
                quantities [left_hz,offset_hz,left_peak_dbfs,right_peak_dbfs];
            }
            MonauralPair "monaural_pair" {
                route ears;
                quantities [first_hz,offset_hz,first_peak_dbfs,second_peak_dbfs];
            }
        }
        modulation_quantities {
            Am {
                rate_hz => AmRate,
                depth => AmDepth
            }
            Gate {
                rate_hz => GateRate,
                width_seconds => GateWidth
            }
        }
    }};
}
pub(crate) use component_schema;

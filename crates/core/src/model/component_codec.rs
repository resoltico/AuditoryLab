//! Strict nested component tables with direct, located numeric decoding.
use crate::{
    ChannelRouting, Component, ComponentForm, ComponentId, Finite, Modulation, SoundSource,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// Direct field decoding retains TOML numeric spans; internally tagged decoding buffers values.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFields {
    kind: alloc::string::String,
    route: Option<ChannelRouting>,
    frequency_hz: Option<Finite>,
    peak_dbfs: Option<Finite>,
    pulse: Option<crate::PulseSpec>,
    first_hz: Option<Finite>,
    offset_hz: Option<Finite>,
    first_peak_dbfs: Option<Finite>,
    second_peak_dbfs: Option<Finite>,
}
impl<'de> Deserialize<'de> for SoundSource {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use crate::model::table_fields::{only, required};
        use serde::de::Error;
        let v = SourceFields::deserialize(d)?;
        let present = [
            ("route", v.route.is_some()),
            ("frequency_hz", v.frequency_hz.is_some()),
            ("peak_dbfs", v.peak_dbfs.is_some()),
            ("pulse", v.pulse.is_some()),
            ("first_hz", v.first_hz.is_some()),
            ("offset_hz", v.offset_hz.is_some()),
            ("first_peak_dbfs", v.first_peak_dbfs.is_some()),
            ("second_peak_dbfs", v.second_peak_dbfs.is_some()),
        ];
        Ok(match v.kind.as_str() {
            "tone" => {
                only::<D::Error>(&present, &["route", "frequency_hz", "peak_dbfs"])?;
                Self::Tone {
                    route: required(v.route, "route")?,
                    frequency_hz: required(v.frequency_hz, "frequency_hz")?,
                    peak_dbfs: required(v.peak_dbfs, "peak_dbfs")?,
                }
            }
            "pulse_train" => {
                only::<D::Error>(&present, &["route", "pulse", "peak_dbfs"])?;
                Self::PulseTrain {
                    route: required(v.route, "route")?,
                    pulse: required(v.pulse, "pulse")?,
                    peak_dbfs: required(v.peak_dbfs, "peak_dbfs")?,
                }
            }
            "binaural_pair" => {
                only::<D::Error>(
                    &present,
                    &[
                        "first_hz",
                        "offset_hz",
                        "first_peak_dbfs",
                        "second_peak_dbfs",
                    ],
                )?;
                Self::BinauralPair {
                    first_hz: required(v.first_hz, "first_hz")?,
                    offset_hz: required(v.offset_hz, "offset_hz")?,
                    first_peak_dbfs: required(v.first_peak_dbfs, "first_peak_dbfs")?,
                    second_peak_dbfs: required(v.second_peak_dbfs, "second_peak_dbfs")?,
                }
            }
            "monaural_pair" => {
                only::<D::Error>(
                    &present,
                    &[
                        "route",
                        "first_hz",
                        "offset_hz",
                        "first_peak_dbfs",
                        "second_peak_dbfs",
                    ],
                )?;
                Self::MonauralPair {
                    route: required(v.route, "route")?,
                    first_hz: required(v.first_hz, "first_hz")?,
                    offset_hz: required(v.offset_hz, "offset_hz")?,
                    first_peak_dbfs: required(v.first_peak_dbfs, "first_peak_dbfs")?,
                    second_peak_dbfs: required(v.second_peak_dbfs, "second_peak_dbfs")?,
                }
            }
            _ => {
                return Err(D::Error::unknown_variant(
                    &v.kind,
                    &["tone", "pulse_train", "binaural_pair", "monaural_pair"],
                ));
            }
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentDocument {
    id: ComponentId,
    source: SoundSource,
    #[serde(with = "crate::modulation::optional")]
    modulation: Option<Modulation>,
}
impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ComponentDocument {
            id: self.id,
            source: self.form.source,
            modulation: self.form.modulation,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Component {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let d = ComponentDocument::deserialize(deserializer)?;
        Ok(Self {
            id: d.id,
            form: ComponentForm {
                source: d.source,
                modulation: d.modulation,
            },
        })
    }
}

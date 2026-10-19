//! Editable sound components with stable identities.
use crate::{Component, Error, ErrorKind, SampleRate};
use auditorylab_core::live::Target;
/// Stable identity of a component within its draft.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ComponentId(pub(crate) u32);
/// Editable sound parameters, independent of a device rate.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub(crate) components: Vec<(ComponentId, Component)>,
}
impl Draft {
    /// An empty, silent draft.
    pub fn new() -> Self {
        Self::default()
    }
    /// Components and their stable identities, in display order.
    pub fn components(&self) -> &[(ComponentId, Component)] {
        &self.components
    }
    /// Validate a complete live target at the stream's sample rate.
    pub fn target(&self, rate: SampleRate) -> Result<Target, Error> {
        let components: Vec<_> = self
            .components
            .iter()
            .map(|(id, component)| (id.0, *component))
            .collect();
        Target::new(rate, &components)
            .map_err(|e| Error::caused(ErrorKind::InvalidSound, e.to_string(), e))
    }
}

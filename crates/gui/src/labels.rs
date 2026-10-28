//! Labels whose alternatives determine desktop control geometry.
use auditorylab_app::vocabulary::Operation;
pub(crate) const fn transport_labels() -> [&'static str; 3] {
    [
        auditorylab_presentation::workspace::operation_label(Operation::PlaySelectedSound),
        auditorylab_presentation::workspace::operation_label(Operation::PlayLive),
        auditorylab_presentation::workspace::operation_label(Operation::Stop),
    ]
}

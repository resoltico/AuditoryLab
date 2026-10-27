//! Captured I/O causes retain their original error chain and identity.
use std::{error::Error, fmt, io, sync::Arc};
/// A captured I/O cause. Clones retain the same cause identity.
#[derive(Debug, Clone)]
pub struct IoCause(Arc<io::Error>);
impl IoCause {
    pub(crate) fn new(error: io::Error) -> Self {
        Self(Arc::new(error))
    }
    /// The original I/O cause.
    pub fn error(&self) -> &io::Error {
        self.0.as_ref()
    }
}
impl PartialEq for IoCause {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for IoCause {}
impl fmt::Display for IoCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for IoCause {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

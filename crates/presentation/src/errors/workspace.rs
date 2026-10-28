//! Workspace error-chain explanations.
use crate::workspace::status::shutdown_reset_failure;
use auditorylab_app::workspace::status::LibraryStatus;
impl crate::Explain for auditorylab_app::workspace::TaskFailure {
    fn explain(&self) -> String {
        match self {
            Self::Operation(error) => error.explain(),
            Self::Worker(error) => crate::workspace::status::worker_failure_description(error),
        }
    }
}
impl crate::Explain for auditorylab_app::workspace::ShutdownError {
    fn explain(&self) -> String {
        use auditorylab_app::workspace::ShutdownIssue;
        use auditorylab_app::workspace::TaskFailure;
        self.issues
            .iter()
            .map(|issue| match issue {
                ShutdownIssue::Library(library) => {
                    crate::workspace::status::library_status_description(
                        &(LibraryStatus::Failed {
                            error: library.error.clone(),
                            publishing: library.publishing,
                            context: library.context.clone(),
                        }),
                    )
                }
                ShutdownIssue::Task { error, context } => {
                    let error = match error {
                        TaskFailure::Operation(error) => error.explain(),
                        TaskFailure::Worker(error) => {
                            crate::workspace::status::worker_failure_description(error)
                        }
                    };
                    format!(
                        "{error}{}",
                        crate::workspace::status::job_context_description(context)
                    )
                }
                ShutdownIssue::EditorReset { error, context } => shutdown_reset_failure(
                    error,
                    &crate::workspace::status::job_context_description(context),
                ),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

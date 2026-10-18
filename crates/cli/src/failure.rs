// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
use crate::args::HelpTopic;
use auditorylab::{AppError, ErrorClass, Operation, Reason};
use std::{
    fmt::{self, Display, Formatter},
    io,
};
pub(crate) const INTERRUPTED: u8 = 130;
#[derive(Debug)]
pub(crate) enum Failure {
    Usage {
        topic: HelpTopic,
        message: String,
    },
    Output(io::Error),
    Interrupts(ctrlc::Error),
    App {
        error: AppError,
        topic: Option<HelpTopic>,
    },
}
impl From<AppError> for Failure {
    fn from(error: AppError) -> Self {
        Self::App { error, topic: None }
    }
}
impl Failure {
    pub(crate) fn for_command(error: AppError, topic: HelpTopic) -> Self {
        Self::App {
            error,
            topic: Some(topic),
        }
    }
    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Self::Usage { .. } => 2,
            Self::App { error, .. } => match error.class() {
                ErrorClass::InvalidInput => 2,
                ErrorClass::Cancelled => INTERRUPTED,
                ErrorClass::Operational => 1,
            },
            _ => 1,
        }
    }
    pub(crate) fn diagnostic(&self) -> String {
        if let Self::Usage { topic, message } = self {
            let pointer = if *topic == HelpTopic::Global {
                "auditorylab --help".into()
            } else {
                format!("auditorylab {} --help", topic.verb())
            };
            let message = crate::run::inline_text(message);
            format!(
                "error: {message}\n{}\nExample: {}\nRun {pointer} for help.\n",
                crate::args::short_usage(*topic),
                crate::args::example(*topic)
            )
        } else if let Self::App { error, .. } = self
            && error.class() == ErrorClass::Cancelled
        {
            let action = match error.operation() {
                Operation::Export => "Export",
                Operation::Save => "Recipe save",
                Operation::Playback => "Playback",
                _ => "Operation",
            };
            let target = error.destination().map_or_else(
                || {
                    error
                        .origin()
                        .map_or_else(String::new, crate::report::origin)
                },
                |path| format!("{path:?}"),
            );
            format!("{action} cancelled: {target}\n")
        } else {
            format!("error: {self}\n")
        }
    }
}
impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage { message, .. } => f.write_str(message),
            Self::Output(error) => write!(f, "cannot write to standard output: {error}"),
            Self::Interrupts(error) => write!(f, "cannot handle interrupts: {error}"),
            Self::App { error, topic } => {
                write!(f, "{error}")?;
                if error.reason() == Reason::UnknownCatalog {
                    f.write_str("; run auditorylab list to find a built-in preset ID")?;
                    if topic.is_none_or(HelpTopic::accepts_file) {
                        f.write_str(", or use --file PATH for a custom recipe")?;
                    } else if let Some(topic) = topic {
                        write!(f, "; {}", crate::args::short_usage(*topic))?;
                    }
                }
                if error.reason() == Reason::DestinationExists {
                    f.write_str("; choose a different --output path")?;
                }
                Ok(())
            }
        }
    }
}

//! Command-line interface to Auditory Lab.
#![forbid(unsafe_code)]
use auditorylab_app::job::{Cancel, Progress, Replace};
use auditorylab_app::presentation::DisplayText;
mod cli_text;
mod library_cli;
use auditorylab_app::core::SampleRate;
use auditorylab_app::outcome::Outcome;

use auditorylab_app::services::playback::{EndReason, PlaybackControl, Preparation, Session};
use auditorylab_app::{self as lab};
use clap::{Args, CommandFactory, Parser, Subcommand};
use cli_text::{CommandProblem, InterfaceOperation};
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};
#[derive(Parser)]
#[command(name = "auditorylab", about = "Explore, export and play auditory recipes", version = lab::presentation::build_info::BUILD_IDENTITY.version_text())]
struct Cli {
    /// Absolute persistent User Library override (then environment, then native data).
    #[arg(long,global=true,value_name="ABSOLUTE_PATH",value_parser=absolute_path)]
    library_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Query persistent Built-in/User entries and explicit temporary inputs.
    List(library_cli::ListArgs),
    /// Persistent User Library lifecycle, diagnostics and root configuration.
    Library {
        #[command(subcommand)]
        command: library_cli::LibraryCommand,
    },
    /// Read a recipe's description and research context.
    Describe {
        #[command(flatten)]
        source: Source,
        #[arg(long, value_enum, default_value = "text")]
        format: library_cli::ReadFormat,
    },
    /// Print or save the portable TOML of a Built-in or User recipe.
    Recipe {
        selector: String,
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Validate a recipe and inspect its technical parameters at one sample rate.
    Inspect {
        #[arg(long, value_enum, default_value = "text")]
        format: library_cli::ReadFormat,
        #[command(flatten)]
        source: Source,
        #[arg(long, default_value=SampleRate::DEFAULT.hz().to_string(), value_parser=rate, value_name="HZ")]
        sample_rate: SampleRate,
    },
    /// Play on the default audio output device.
    Play(Source),
    /// Write a stereo float WAV file; never replace an existing file.
    Export {
        #[command(flatten)]
        source: Source,
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
        #[arg(long, default_value=SampleRate::DEFAULT.hz().to_string(), value_parser=rate, value_name="HZ")]
        sample_rate: SampleRate,
    },
}
#[derive(Args)]
#[group(id = "source", required = true, multiple = false)]
struct Source {
    /// Persistent builtin:<catalog-id> or user:<canonical-uuid> selector.
    #[arg(value_name = "SOURCE")]
    selector: Option<String>,
    /// Read a UTF-8 TOML recipe file.
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Read a UTF-8 TOML recipe until EOF.
    #[arg(long)]
    stdin: bool,
}
fn rate(text: &str) -> Result<SampleRate, String> {
    text.parse::<u32>()
        .map_err(|e| e.to_string())
        .and_then(|n| {
            SampleRate::new(n).map_err(|e| lab::presentation::core_errors::sample_rate_error(&e))
        })
}
fn absolute_path(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(CommandProblem::RelativeLibraryRoot.message().into())
    }
}
#[derive(Debug)]
struct Usage(String);
impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Usage {}
#[derive(Debug)]
enum CliError {
    Usage(Usage),
    Operation(Box<auditorylab_app::services::OperationError>),
    Io(io::Error),
    Interrupt(ctrlc::Error),
    Json(serde_json::Error),
    Formatting(std::fmt::Error),
}
impl CliError {
    fn outcome(&self) -> Outcome {
        match self {
            Self::Usage(_) => Outcome::InvalidArgument,
            Self::Operation(error) => error.outcome(),
            Self::Io(_) | Self::Interrupt(_) | Self::Json(_) | Self::Formatting(_) => {
                Outcome::Write
            }
        }
    }
    fn code(&self) -> String {
        match self {
            Self::Operation(error) => error.to_string(),
            Self::Usage(_) => "cli.argument".into(),
            Self::Io(_) => "cli.output".into(),
            Self::Interrupt(_) => "cli.interrupt".into(),
            Self::Json(_) => "cli.json".into(),
            Self::Formatting(_) => "cli.format".into(),
        }
    }
    fn message(&self) -> String {
        let message = match self {
            Self::Usage(error) => error.to_string(),
            Self::Operation(error) => lab::presentation::Explain::explain(error.as_ref()),
            Self::Io(error) => error.to_string(),
            Self::Interrupt(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::Formatting(error) => error.to_string(),
        };
        if self.outcome() == Outcome::UnknownRecipe {
            format!("{message}; run `auditorylab list`, or use --file PATH")
        } else {
            message
        }
    }
}
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}
impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Usage(e) => e,
            Self::Operation(e) => e.as_ref(),
            Self::Io(e) => e,
            Self::Interrupt(e) => e,
            Self::Json(e) => e,
            Self::Formatting(e) => e,
        })
    }
}
macro_rules! operation_errors {
    ($($ty:ty),* $(,)?) => { $(impl From<$ty> for CliError { fn from(error: $ty) -> Self { Self::Operation(Box::new(error.into())) } })* };
}
operation_errors! { auditorylab_app::services::OperationError, auditorylab_app::services::LoadError, auditorylab_app::services::library::LibraryError, auditorylab_app::services::SaveError, auditorylab_app::services::ExportError, auditorylab_app::services::playback::PlaybackError, auditorylab_app::document::error::EditorError, auditorylab_app::identity::IdentityError, auditorylab_app::discovery::query::QueryError, lab::core::RecipeError }
impl From<lab::document::origin::OriginError> for CliError {
    fn from(error: lab::document::origin::OriginError) -> Self {
        Self::Operation(Box::new(error.into()))
    }
}
impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<ctrlc::Error> for CliError {
    fn from(error: ctrlc::Error) -> Self {
        Self::Interrupt(error)
    }
}
impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl From<std::fmt::Error> for CliError {
    fn from(error: std::fmt::Error) -> Self {
        Self::Formatting(error)
    }
}
impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Usage(Usage(message))
    }
}
impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        Self::Usage(Usage(message.into()))
    }
}
fn usage(message: &str) -> CliError {
    CliError::Usage(Usage(message.into()))
}
fn exit_code(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::UnknownRecipe
        | Outcome::InvalidRecipe
        | Outcome::InvalidSound
        | Outcome::InvalidArgument
        | Outcome::ReadOnlyBuiltIn => 2,
        Outcome::Conflict | Outcome::HistoryConflict => 3,
        Outcome::Busy | Outcome::LockUnavailable => 4,
        Outcome::IncompleteInventory => 5,
        Outcome::Cancelled => 130,
        Outcome::Unreadable
        | Outcome::Identity
        | Outcome::Device
        | Outcome::DestinationExists
        | Outcome::Write => 1,
    }
}

fn prose(text: &str) -> String {
    let width = if io::stdout().is_terminal() {
        textwrap::termwidth().saturating_sub(2).max(1)
    } else {
        78
    };
    text.lines()
        .map(|line| textwrap::fill(&(line).display_text(), width))
        .collect::<Vec<_>>()
        .join("\n")
}
fn output(bytes: &[u8]) -> io::Result<()> {
    match io::stdout()
        .lock()
        .write_all(bytes)
        .and_then(|()| io::stdout().lock().flush())
    {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}
fn say(text: &str) -> io::Result<()> {
    output(format!("{}\n", prose(text)).as_bytes())
}
fn completed(text: &str) {
    if let Err(e) = say(text) {
        eprintln!("{}", crate::cli_text::completion_reporting_failed(e));
    }
}
/// The first interrupt runs `request`; a second one exits at once.
fn on_interrupt(request: impl Fn() + Send + 'static) -> Result<(), CliError> {
    let interrupted = std::sync::atomic::AtomicBool::new(false);
    ctrlc::set_handler(move || {
        if interrupted.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if io::stderr().is_terminal() {
                let _ = write!(io::stderr(), "\r\x1b[2K");
                let _ = io::stderr().flush();
            }
            std::process::exit(130);
        }
        request();
    })?;
    Ok(())
}
struct StatusLine {
    active: bool,
}
impl StatusLine {
    fn draw(&mut self, text: &str) {
        self.active = true;
        let _ = write!(io::stderr(), "\r\x1b[2K{}", (text).display_text());
        let _ = io::stderr().flush();
    }
}
impl Drop for StatusLine {
    fn drop(&mut self) {
        if self.active {
            let _ = write!(io::stderr(), "\r\x1b[2K");
            let _ = io::stderr().flush();
        }
    }
}
/// How a command ended when it did not fail.
enum Ended {
    Normally,
    Stopped,
    Code(u8),
}
fn play(resolved: &lab::services::Resolved) -> Result<Ended, CliError> {
    let session = Session::new();
    let control: PlaybackControl = session.control();
    let stop = control.clone();
    on_interrupt(move || {
        stop.request_stop();
    })?;
    let mut playback = match lab::services::playback::prepare_playback(session, resolved)? {
        Preparation::Ready(playback) => playback,
        Preparation::Stopped(report) => {
            eprintln!("{}", prose(&report.description()));
            return Ok(Ended::Stopped);
        }
    };
    eprintln!(
        "{}\n{}",
        prose(
            &playback
                .rendition()
                .snapshot()
                .playback_preamble(playback.output(), playback.signal())
        ),
        crate::cli_text::Caption::StopKeyboardHint.label()
    );
    let sound = playback.rendition();
    eprintln!("{}", prose(&sound.description(false)));
    let report = if io::stderr().is_terminal() {
        let mut reader = playback
            .monitor(lab::services::playback::MonitorDetail::Quantities)
            .reader()
            .ok_or_else(|| {
                std::io::Error::other(crate::cli_text::Caption::MonitorAlreadyInUse.label())
            })?;
        let done = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(
            |scope| -> Result<lab::services::playback::PlaybackReport, CliError> {
                let printer = scope.spawn(|| {
                    let mut terminal = StatusLine { active: false };
                    while !done.load(std::sync::atomic::Ordering::Acquire) {
                        reader.poll(0);
                        terminal.draw(&reader.status_text());
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                });
                // Keep the discovered device on its caller thread; only plain
                // shared status crosses to the terminal renderer.
                let result = playback.play();
                done.store(true, std::sync::atomic::Ordering::Release);
                printer.join().map_err(|_| {
                    io::Error::other(InterfaceOperation::StatusRenderer.failure(""))
                })?;
                Ok(result?)
            },
        )?
    } else {
        playback.play()?
    };
    eprintln!("{}", prose(&report.description()));
    Ok(if report.end == EndReason::Stopped {
        Ended::Stopped
    } else {
        Ended::Normally
    })
}
fn run(cli: Cli) -> Result<Ended, CliError> {
    match cli.command {
        None => output(format!("{}\n", Cli::command().render_help()).as_bytes())?,
        Some(Command::List(args)) => {
            return Ok(Ended::Code(library_cli::list(
                args,
                cli.library_dir.as_deref(),
            )?));
        }
        Some(Command::Library { command }) => {
            return Ok(Ended::Code(library_cli::library(
                command,
                cli.library_dir.as_deref(),
            )?));
        }
        Some(Command::Recipe {
            selector,
            output: path,
        }) => {
            let key: auditorylab_app::document::origin::LibraryKey = selector.parse()?;
            if matches!(
                key,
                auditorylab_app::document::origin::LibraryKey::External(_)
            ) {
                return Err(usage(CommandProblem::TemporaryRecipeSelector.message()));
            }
            let text = lab::services::portable_text(&key, cli.library_dir.as_deref())?;
            if let Some(path) = path {
                let saved = lab::services::save_recipe_text(&text, &path, Replace::Never)?;
                for warning in saved.warnings {
                    eprintln!(
                        "{}",
                        prose(&lab::presentation::publication_text::warning(&warning))
                    );
                }
                completed(&lab::presentation::recipe_exported(&path));
            } else {
                output(text.as_bytes())?;
            }
        }
        Some(Command::Describe { source, format }) => library_cli::describe(
            &library_cli::resolve(source, cli.library_dir.as_deref())?,
            format,
        )?,
        Some(Command::Inspect {
            source,
            sample_rate,
            format,
        }) => {
            return Ok(Ended::Code(library_cli::inspect(
                &library_cli::resolve(source, cli.library_dir.as_deref())?,
                sample_rate,
                format,
            )?));
        }
        Some(Command::Export {
            source,
            output: path,
            sample_rate,
        }) => {
            let resolved = library_cli::resolve(source, cli.library_dir.as_deref())?;
            let inspection = lab::services::inspect(&resolved, sample_rate);
            let sound = inspection.sound().map_err(Clone::clone)?;
            let cancel = Cancel::default();
            let request = cancel.clone();
            on_interrupt(move || request.request())?;
            eprintln!("{}", prose(&sound.export_preamble(&path)));
            let result = lab::services::export(
                resolved.snapshot(),
                sample_rate,
                &path,
                Replace::Never,
                &cancel,
                &Progress::default(),
            )?;
            for warning in &result.warnings {
                eprintln!(
                    "{}",
                    prose(&lab::presentation::publication_text::warning(warning))
                );
            }
            completed(&result.description(&path));
        }
        Some(Command::Play(source)) => {
            let resolved = library_cli::resolve(source, cli.library_dir.as_deref())?;
            return play(&resolved);
        }
    }
    Ok(Ended::Normally)
}
fn command_output(command: &Option<Command>) -> (bool, &'static str) {
    match command {
        Some(Command::List(args)) => (args.json(), "list"),
        Some(Command::Library { command }) => (command.json(), command.operation()),
        Some(Command::Describe { format, .. }) => {
            (*format == library_cli::ReadFormat::Json, "describe")
        }
        Some(Command::Inspect { format, .. }) => {
            (*format == library_cli::ReadFormat::Json, "inspect")
        }
        _ => (false, "command"),
    }
}
fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e)
            if matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return if output(e.to_string().as_bytes()).is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        Err(e) => e.exit(),
    };
    let (json, operation) = command_output(&cli.command);
    match run(cli) {
        Ok(Ended::Normally) => ExitCode::SUCCESS,
        Ok(Ended::Stopped) => ExitCode::from(130),
        Ok(Ended::Code(code)) => ExitCode::from(code),
        Err(e) => {
            let message = e.message();
            let code = exit_code(e.outcome());
            if json {
                if let Err(output_error) = library_cli::error_json(operation, &e) {
                    eprintln!(
                        "{}",
                        InterfaceOperation::StructuredFailure.failure(output_error)
                    );
                }
            } else {
                eprintln!("{}", prose(&message));
            }
            ExitCode::from(code)
        }
    }
}

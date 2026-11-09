//! Command-line interface to Auditory Lab.
#![forbid(unsafe_code)]
use auditorylab_app::job::{Cancel, Progress, Replace};

use auditorylab_presentation::Explain;
mod cli_text;
mod device_check_cli;
mod library_cli;
mod recording_cli;
use auditorylab_app::core::SampleRate;
use auditorylab_app::outcome::Outcome;
use auditorylab_app::vocabulary::Operation;

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
#[command(name = "auditorylab", about = "Browse, export and play research-inspired sounds",
    after_help = auditorylab_presentation::build_info::RESEARCH_USE,
    version = auditorylab_presentation::build_info::build_identity_version_text(
        auditorylab_app::identity::BUILD_IDENTITY
    ))]
struct Cli {
    // A plain help string: rustdoc's Markdown lint would put backticks into rendered help.
    #[arg(
        long,
        global = true,
        value_name = "ABSOLUTE_PATH",
        value_parser = library_override,
        help = "Use this User Library folder (an absolute path) instead of \
            AUDITORYLAB_LIBRARY_DIR or the default"
    )]
    library_dir: Option<lab::services::LibraryContext>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Create a complete editable recipe using the shared application defaults.
    New {
        /// Source construction; edit the emitted TOML to customize it.
        #[arg(value_parser = creation_kind, help = creation_help())]
        kind: lab::document::construction::CreationKind,
        /// File to create; existing files are never replaced.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// List built-in sounds and your own recipes, with search, filters and sorting.
    List(library_cli::ListArgs),
    /// Manage your User Library: set it up, add, rename, trash, restore and check recipes.
    Library {
        #[command(subcommand)]
        command: library_cli::LibraryCommand,
    },
    /// Read what a sound is, what it was studied for, and how it was adapted.
    Describe {
        #[command(flatten)]
        source: Source,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: library_cli::ReadFormat,
    },
    /// Print a recipe as an editable TOML file, or save it with --output.
    Recipe {
        /// The recipe, as builtin:<id> or user:<id> from list.
        selector: String,
        /// File to create; an existing file is never replaced.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Check a recipe and show its technical details at one sample rate.
    Inspect {
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: library_cli::ReadFormat,
        #[command(flatten)]
        source: Source,
        /// Sample rate in Hz.
        #[arg(long, default_value=SampleRate::DEFAULT.hz().to_string(),
            value_parser=clap::value_parser!(u32), value_name="HZ")]
        sample_rate: u32,
    },
    /// Play a sound on the default audio output; turn the volume down first.
    Play(Source),
    /// Play short test tones and report whether left, right and low tones reach you.
    DeviceCheck,
    /// Measure tones, stereo differences, pulsing and low-frequency energy in a recording.
    Analyze {
        /// Local audio file to decode and measure.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: library_cli::ReadFormat,
        /// Analyze this many seconds after the 10% offset (default: up to ten minutes).
        #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(2..))]
        seconds: Option<u32>,
    },
    /// Save a sound as a stereo WAV file (32-bit float); an existing file is never replaced.
    Export {
        #[command(flatten)]
        source: Source,
        /// WAV file to create.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
        /// Sample rate in Hz.
        #[arg(long, default_value=SampleRate::DEFAULT.hz().to_string(),
            value_parser=clap::value_parser!(u32), value_name="HZ")]
        sample_rate: u32,
    },
}
#[derive(Args)]
#[group(id = "source", required = true, multiple = false)]
struct Source {
    /// The recipe, as builtin:<id> or user:<id> from list.
    #[arg(value_name = "SOURCE")]
    selector: Option<String>,
    /// Use a recipe file instead.
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Read a recipe from standard input instead.
    #[arg(long)]
    stdin: bool,
}

fn creation_help() -> String {
    format!(
        "Construction to create: {}. Edit the emitted TOML to customize it.",
        lab::document::construction::CreationKind::ALL
            .map(|kind| format!("{} — {}", kind.key(), kind.term().label))
            .join("; ")
    )
}
fn creation_kind(text: &str) -> Result<lab::document::construction::CreationKind, String> {
    lab::document::construction::CreationKind::ALL
        .into_iter()
        .find(|kind| kind.key() == text)
        .ok_or_else(|| {
            format!(
                "expected one of: {}",
                lab::document::construction::CreationKind::ALL
                    .map(auditorylab_app::document::construction::CreationKind::key)
                    .join(", ")
            )
        })
}
fn library_override(text: &str) -> Result<lab::services::LibraryContext, String> {
    lab::services::LibraryContext::from_override(std::path::Path::new(text))
        .map_err(|error| error.explain())
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

    fn message(&self) -> String {
        let message = match self {
            Self::Usage(error) => error.to_string(),
            Self::Operation(error) => auditorylab_presentation::Explain::explain(error.as_ref()),
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
    ($($ty:ty),* $(,)?) => { $(impl From<$ty> for CliError { fn from(error: $ty) -> Self {
        Self::Operation(Box::new(error.into())) } })* };
}
operation_errors! {
    auditorylab_app::services::OperationError,
    auditorylab_app::services::LoadError,
    auditorylab_app::services::FileError,
    auditorylab_app::recording::RecordingError,
    auditorylab_app::services::library::LibraryError,
    auditorylab_app::services::SaveError,
    auditorylab_app::services::ExportError,
    auditorylab_app::services::playback::PlaybackError,
    auditorylab_app::document::error::EditorError,
    auditorylab_app::identity::IdentityError,
    auditorylab_app::discovery::query::QueryError,
    lab::core::RecipeError,
}
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

/// Text for standard output.
fn prose(text: &str) -> String {
    wrapped(text, io::stdout().is_terminal())
}
/// Text for standard error.
fn eprose(text: &str) -> String {
    wrapped(text, io::stderr().is_terminal())
}
/// Wrap for a terminal only, and never inside a word: paths, selectors and
/// tokens stay whole so they can be copied, searched and parsed.
fn wrapped(text: &str, terminal: bool) -> String {
    let width = terminal.then(|| textwrap::termwidth().saturating_sub(2).max(1));
    text.lines()
        .map(|line| {
            let line = auditorylab_presentation::display_text(line);
            match width {
                Some(width) => textwrap::fill(
                    &line,
                    textwrap::Options::new(width)
                        .break_words(false)
                        .word_splitter(textwrap::WordSplitter::NoHyphenation),
                ),
                None => line,
            }
        })
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
/// Report confirmed publication without allowing either terminal to revoke it.
fn report_publication(
    text: &str,
    warnings: &[lab::services::StorageWarning],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) {
    for warning in warnings {
        let _ = writeln!(
            stderr,
            "{}",
            eprose(&auditorylab_presentation::errors::publication::warning(
                warning
            ))
        );
    }
    write_confirmed(
        format!("{}\n", prose(text)).as_bytes(),
        crate::cli_text::completion_reporting_failed,
        stdout,
        stderr,
    );
    let _ = stderr.flush();
}
/// Keep publication truth independent of a terminal write or flush.
fn write_confirmed(
    bytes: &[u8],
    failure: impl FnOnce(io::Error) -> String,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) {
    if let Err(error) = stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        let _ = writeln!(
            stderr,
            "{}",
            auditorylab_presentation::display_text(&failure(error))
        );
        let _ = stderr.flush();
    }
}
fn completed(text: &str, warnings: &[lab::services::StorageWarning]) {
    report_publication(
        text,
        warnings,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
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
        let _ = write!(
            io::stderr(),
            "\r\x1b[2K{}",
            auditorylab_presentation::display_text(text)
        );
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
            eprintln!(
                "{}",
                eprose(&auditorylab_presentation::playback_report_description(
                    &(report)
                ))
            );
            return Ok(Ended::Stopped);
        }
    };
    eprintln!(
        "{}\n{}",
        eprose(&auditorylab_presentation::snapshot_playback_preamble(
            playback.rendition().snapshot(),
            playback.output(),
            playback.signal()
        )),
        crate::cli_text::Caption::StopKeyboardHint.label()
    );
    let sound = playback.rendition();
    eprintln!(
        "{}",
        eprose(&auditorylab_presentation::rendition_description(
            sound, false
        ))
    );
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
                        terminal.draw(&auditorylab_presentation::monitor_reader_status_text(
                            &(reader),
                        ));
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
    eprintln!(
        "{}",
        eprose(&auditorylab_presentation::playback_report_description(
            &(report)
        ))
    );
    Ok(if report.end == EndReason::Stopped {
        Ended::Stopped
    } else {
        Ended::Normally
    })
}
fn run(cli: Cli) -> Result<Ended, CliError> {
    let Some(command) = cli.command else {
        output(format!("{}\n", Cli::command().render_help()).as_bytes())?;
        return Ok(Ended::Normally);
    };
    let context = cli
        .library_dir
        .unwrap_or_else(|| lab::services::LibraryContext::resolve(None));
    match command {
        Command::New { kind, output: path } => {
            let recipe = lab::document::construction::new_recipe(kind)?;
            if let Some(path) = path {
                let saved = lab::services::save_content(&recipe, &path, Replace::Never)?;
                // New --output intentionally has no completion stdout; warnings still matter.
                report_publication(
                    "",
                    &saved.warnings,
                    &mut io::sink(),
                    &mut io::stderr().lock(),
                );
            } else {
                output(recipe.to_toml()?.as_bytes())?;
            }
        }
        Command::List(args) => {
            return Ok(Ended::Code(library_cli::list(args, &context)?));
        }
        Command::Library { command } => {
            return Ok(Ended::Code(library_cli::library(command, &context)?));
        }
        Command::Recipe {
            selector,
            output: path,
        } => {
            export_recipe(&selector, path, &context)?;
        }
        Command::Describe { source, format } => {
            library_cli::describe(&library_cli::resolve(source, &context)?, format)?;
        }
        Command::Inspect {
            source,
            sample_rate,
            format,
        } => {
            let sample_rate =
                SampleRate::new(sample_rate).map_err(lab::services::OperationError::from)?;
            return Ok(Ended::Code(library_cli::inspect(
                &library_cli::resolve(source, &context)?,
                sample_rate,
                format,
            )?));
        }
        Command::Export {
            source,
            output: path,
            sample_rate,
        } => {
            let sample_rate =
                SampleRate::new(sample_rate).map_err(lab::services::OperationError::from)?;
            let resolved = library_cli::resolve(source, &context)?;
            let inspection = lab::services::inspect(&resolved, sample_rate);
            let sound = inspection.sound().map_err(Clone::clone)?;
            let cancel = Cancel::default();
            let request = cancel.clone();
            on_interrupt(move || request.request())?;
            let result = lab::services::export_rendition(
                sound,
                &path,
                Replace::Never,
                &cancel,
                &Progress::default(),
                || {
                    let _ = writeln!(
                        io::stderr().lock(),
                        "{}",
                        eprose(&auditorylab_presentation::rendition_export_preamble(
                            sound, &path
                        ))
                    );
                },
            )?;
            completed(
                &auditorylab_presentation::workspace::exported_description(&result, &path),
                &result.warnings,
            );
        }
        Command::Play(source) => {
            let resolved = library_cli::resolve(source, &context)?;
            return play(&resolved);
        }
        Command::DeviceCheck => return device_check_cli::device_check(),
        Command::Analyze {
            file,
            format,
            seconds,
        } => recording_cli::analyze(&file, format, seconds)?,
    }
    Ok(Ended::Normally)
}
fn command_output(command: Option<&Command>) -> Option<(bool, Operation)> {
    Some(match command? {
        Command::List(args) => (args.json(), Operation::List),
        Command::Library { command } => (command.json(), command.operation()),
        Command::Describe { format, .. } => (
            *format == library_cli::ReadFormat::Json,
            Operation::Describe,
        ),
        Command::Inspect { format, .. } => {
            (*format == library_cli::ReadFormat::Json, Operation::Inspect)
        }
        Command::New { .. } => (false, Operation::NewSound),
        Command::Recipe { .. } => (false, Operation::Recipe),
        Command::Export { .. } => (false, Operation::Export),
        Command::Play(_) => (false, Operation::Play),
        Command::DeviceCheck => (false, Operation::DeviceCheck),
        Command::Analyze { format, .. } => (
            *format == library_cli::ReadFormat::Json,
            Operation::AnalyzeRecording,
        ),
    })
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
    let selected_output = command_output(cli.command.as_ref());
    match run(cli) {
        Ok(Ended::Normally) => ExitCode::SUCCESS,
        Ok(Ended::Stopped) => ExitCode::from(130),
        Ok(Ended::Code(code)) => ExitCode::from(code),
        Err(e) => {
            let message = e.message();
            let code = exit_code(e.outcome());
            if let Some((true, operation)) = selected_output {
                if let Err(output_error) = library_cli::error_json(operation, &e) {
                    eprintln!(
                        "{}",
                        InterfaceOperation::StructuredFailure.failure(output_error)
                    );
                }
            } else {
                eprintln!("{}", eprose(&message));
            }
            ExitCode::from(code)
        }
    }
}

// diagnostic-codes:begin
// The presentation reference test reads this declaration because CLI is a binary.
auditorylab_app::core::diagnostic_codes! {
    #[cfg(test)]
    pub(crate) const CODES;
    CliError {
        codes {
            Self::Usage(_)=>"cli.argument",
            Self::Io(_)=>"cli.output",
            Self::Interrupt(_)=>"cli.interrupt",
            Self::Json(_)=>"cli.json",
            Self::Formatting(_)=>"cli.format",
        }
        delegates {Self::Operation(error)=>error,}
    }
}
// diagnostic-codes:end

fn export_recipe(
    selector: &str,
    path: Option<PathBuf>,
    context: &lab::services::LibraryContext,
) -> Result<(), CliError> {
    let key: auditorylab_app::document::origin::LibraryKey = selector.parse()?;
    if matches!(
        key,
        auditorylab_app::document::origin::LibraryKey::External(_)
    ) {
        return Err(usage(CommandProblem::TemporaryRecipeSelector.message()));
    }
    let text = lab::services::portable_text(&key, context)?;
    if let Some(path) = path {
        let saved = lab::services::save_recipe_text(&text, &path, Replace::Never)?;
        completed(
            &auditorylab_presentation::recipe_exported(&path),
            &saved.warnings,
        );
    } else {
        output(text.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    #[test]
    fn every_cli_leaf_maps_to_its_own_report_operation() {
        use std::collections::BTreeSet;
        let id = "user:00000000-0000-4000-8000-000000000001";
        let cases: &[(&[&str], Operation)] = &[
            (&["list"], Operation::List),
            (&["describe", id], Operation::Describe),
            (&["inspect", id], Operation::Inspect),
            (&["recipe", id], Operation::Recipe),
            (&["play", id], Operation::Play),
            (&["export", id, "--output", "out.wav"], Operation::Export),
            (&["library", "path"], Operation::LibraryPath),
            (&["library", "init"], Operation::LibraryInit),
            (
                &["library", "add", "--file", "in.toml"],
                Operation::LibraryAdd,
            ),
            (
                &["library", "update", id, "--file", "in.toml"],
                Operation::LibraryUpdate,
            ),
            (
                &["library", "rename", id, "--title", "title"],
                Operation::LibraryRename,
            ),
            (&["library", "trash", id], Operation::LibraryTrash),
            (&["library", "restore", id], Operation::LibraryRestore),
            (&["library", "purge", id, "--yes"], Operation::LibraryPurge),
            (&["library", "list-trash"], Operation::ListTrash),
            (&["library", "check"], Operation::LibraryCheck),
            (&["new", "tone"], Operation::NewSound),
            (&["device-check"], Operation::DeviceCheck),
            (&["analyze", "in.m4a"], Operation::AnalyzeRecording),
        ];
        let mut operations = BTreeSet::new();
        let mut names = BTreeSet::new();
        for (args, expected) in cases {
            let parsed =
                Cli::try_parse_from(std::iter::once("auditorylab").chain(args.iter().copied()))
                    .unwrap();
            let (_, actual) = command_output(parsed.command.as_ref()).unwrap();
            assert_eq!(actual, *expected);
            assert!(
                operations.insert(actual),
                "two CLI leaves name the same operation"
            );
            names.insert(if args[0] == "library" {
                format!("library {}", args[1])
            } else {
                args[0].to_owned()
            });
            let report = auditorylab_presentation::report::Report::error(
                actual,
                &CliError::Usage(Usage("fixture".into())),
                "fixture",
                Outcome::InvalidArgument,
                None,
            );
            assert_eq!(
                serde_json::to_value(report).unwrap()["operation"],
                expected.key()
            );
        }
        let mut grammar = BTreeSet::new();
        for command in Cli::command()
            .get_subcommands()
            .filter(|c| c.get_name() != "help")
        {
            if command.has_subcommands() {
                for child in command.get_subcommands().filter(|c| c.get_name() != "help") {
                    grammar.insert(format!("{} {}", command.get_name(), child.get_name()));
                }
            } else {
                grammar.insert(command.get_name().to_owned());
            }
        }
        assert_eq!(
            names, grammar,
            "every offered CLI command needs an independent operation expectation"
        );
        assert!(command_output(None).is_none());
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;
    #[test]
    fn help_text_is_plain_text_without_markdown_markup() {
        fn check(command: &mut clap::Command) {
            let help = command.render_long_help().to_string();
            assert!(
                !help.contains('`'),
                "{} help contains Markdown backticks:\n{help}",
                command.get_name()
            );
            for child in command.get_subcommands_mut() {
                check(child);
            }
        }
        let mut command = Cli::command();
        command.build();
        check(&mut command);
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use auditorylab_app::core::{diagnostic::Code, diagnostic::ParamValue};
    #[test]
    fn owned_cli_codes_cover_their_variants_and_domain_errors_delegate() {
        let cases = [
            (
                CliError::Usage(Usage("invalid argument".into())),
                "cli.argument",
            ),
            (
                CliError::Io(io::Error::other("output failure")),
                "cli.output",
            ),
            (
                CliError::Interrupt(ctrlc::Error::MultipleHandlers),
                "cli.interrupt",
            ),
            (
                CliError::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
                "cli.json",
            ),
            (CliError::Formatting(std::fmt::Error), "cli.format"),
        ];
        let mut actual = std::collections::BTreeSet::new();
        for (error, expected) in cases {
            assert_eq!(error.code(), expected);
            actual.insert(error.code());
        }
        assert_eq!(actual, CODES.iter().copied().collect());
        let error = CliError::from(lab::services::OperationError::from(
            SampleRate::new(0).unwrap_err(),
        ));
        assert_eq!(error.code(), "sample_rate.range");
        assert_eq!(error.params()["hz"], ParamValue::Unsigned(0));
        assert_eq!(error.outcome(), Outcome::InvalidArgument);
    }
}

#[cfg(test)]
mod publication_reporting_tests {
    use super::*;
    struct RefusingWriter;
    impl Write for RefusingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected terminal failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }
    #[test]
    fn confirmed_publication_survives_either_or_both_terminal_failures() {
        let warnings = [lab::services::StorageWarning::ClockPredatesEdit];
        let mut stderr = Vec::new();
        report_publication("Published", &warnings, &mut RefusingWriter, &mut stderr);
        let text = String::from_utf8(stderr).unwrap();
        assert!(text.contains("injected terminal failure"));
        assert!(
            text.contains("Published") || text.contains("completed") || text.contains("Completion")
        );
        let mut stdout = Vec::new();
        report_publication("Published", &warnings, &mut stdout, &mut RefusingWriter);
        assert_eq!(stdout, b"Published\n");
        report_publication(
            "Published",
            &warnings,
            &mut RefusingWriter,
            &mut RefusingWriter,
        );
    }
}

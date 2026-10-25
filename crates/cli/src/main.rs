//! Command-line interface to Auditory Lab.
#![forbid(unsafe_code)]
mod library_cli;
use auditorylab::{
    self as lab, Cancel, EndReason, ErrorKind, PlaybackControl, Preparation, Progress,
    RecipeSnapshot, Replace, SampleRate, Session,
};
use clap::{Args, CommandFactory, Parser, Subcommand};
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};
#[derive(Parser)]
#[command(name = "auditorylab", about = "Explore, export and play auditory recipes", version = lab::version_line("auditorylab").trim_start_matches("auditorylab ").to_owned())]
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
    /// Print or save an editable built-in TOML recipe.
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
        .and_then(|n| SampleRate::new(n).map_err(|e| e.to_string()))
}
fn absolute_path(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("--library-dir must be absolute; no fallback is used".into())
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
fn usage(message: &str) -> Box<dyn std::error::Error> {
    Box::new(Usage(message.into()))
}
fn exit_code(kind: ErrorKind) -> u8 {
    kind.exit_code()
}
use auditorylab::display_text as escape;
fn prose(text: &str) -> String {
    let width = if io::stdout().is_terminal() {
        textwrap::termwidth().saturating_sub(2).max(1)
    } else {
        78
    };
    text.lines()
        .map(|line| textwrap::fill(&escape(line), width))
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
        eprintln!("warning: operation completed; cannot report completion: {e}");
    }
}
/// The first interrupt runs `request`; a second one exits at once.
fn on_interrupt(request: impl Fn() + Send + 'static) -> Result<(), Box<dyn std::error::Error>> {
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
        let _ = write!(io::stderr(), "\r\x1b[2K{}", escape(text));
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
fn play(snapshot: &RecipeSnapshot) -> Result<Ended, Box<dyn std::error::Error>> {
    let session = Session::new();
    let control: PlaybackControl = session.control();
    let stop = control.clone();
    on_interrupt(move || {
        stop.request_stop();
    })?;
    let mut playback = match lab::device::prepare_playback(session, snapshot)? {
        Preparation::Ready(playback) => playback,
        Preparation::Stopped(report) => {
            eprintln!("{}", prose(&lab::playback_result(&report)));
            return Ok(Ended::Stopped);
        }
    };
    eprintln!(
        "{}\nPress Ctrl-C to stop.",
        prose(&lab::playback_preamble(
            snapshot,
            playback.output(),
            playback.signal()
        ))
    );
    let sound = snapshot.at(playback.output().sample_rate)?;
    eprintln!("{}", prose(&lab::sound_description(&sound, false)));
    let report = if io::stderr().is_terminal() {
        let mut reader = playback.monitor(lab::MonitorDetail::Quantities).reader();
        let done = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(
            |scope| -> Result<lab::PlaybackReport, Box<dyn std::error::Error>> {
                let printer = scope.spawn(|| {
                    let mut terminal = StatusLine { active: false };
                    while !done.load(std::sync::atomic::Ordering::Acquire) {
                        reader.poll(0);
                        terminal.draw(&lab::playback_status(&reader));
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                });
                // Keep the discovered device on its caller thread; only plain
                // shared status crosses to the terminal renderer.
                let result = playback.play();
                done.store(true, std::sync::atomic::Ordering::Release);
                printer
                    .join()
                    .map_err(|_| io::Error::other("status renderer failed"))?;
                Ok(result?)
            },
        )?
    } else {
        playback.play()?
    };
    eprintln!("{}", prose(&lab::playback_result(&report)));
    Ok(if report.end == EndReason::Stopped {
        Ended::Stopped
    } else {
        Ended::Normally
    })
}
fn run(cli: Cli) -> Result<Ended, Box<dyn std::error::Error>> {
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
            let key: lab::library::LibraryKey = selector.parse()?;
            let text = match &key {
                lab::library::LibraryKey::BuiltIn(id) => lab::builtin_source(id)?.to_owned(),
                lab::library::LibraryKey::User(_) => {
                    library_cli::store(cli.library_dir.as_deref())?
                        .load(&key)?
                        .recipe_toml
                        .to_string()
                }
                lab::library::LibraryKey::External(_) => {
                    return Err(usage("temporary selectors are not reusable; use --file"));
                }
            };
            if let Some(path) = path {
                lab::save_recipe_text(&text, &path, Replace::Never)?;
                completed(&format!("Exported recipe to {path:?}."));
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
            let snapshot = library_cli::resolve(source, cli.library_dir.as_deref())?.snapshot;
            let sound = snapshot.at(sample_rate)?;
            let cancel = Cancel::default();
            let request = cancel.clone();
            on_interrupt(move || request.request())?;
            eprintln!(
                "{}",
                prose(&format!(
                    "Exporting {} to {path:?} at {} Hz. Press Ctrl-C to stop.",
                    snapshot.title(),
                    sample_rate.hz()
                ))
            );
            let result = lab::export(&sound, &path, Replace::Never, &cancel, &Progress::default())?;
            completed(&format!(
                "Exported {} frames to {path:?} ({}).",
                result.frames, result.container
            ));
        }
        Some(Command::Play(source)) => {
            let resolved = library_cli::resolve(source, cli.library_dir.as_deref())?;
            if resolved.summary.trashed_at.is_some() {
                return Err(usage(
                    "trashed recipes can be inspected/exported; restore before playback",
                ));
            }
            return play(&resolved.snapshot);
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
            let mut message = e.to_string();
            if e.downcast_ref::<lab::Error>()
                .is_some_and(|e| e.kind() == ErrorKind::UnknownRecipe)
            {
                message.push_str("; run `auditorylab list`, or use --file PATH");
            }
            let code = if e.downcast_ref::<Usage>().is_some() {
                2
            } else {
                e.downcast_ref::<lab::Error>()
                    .map_or(1, |e| exit_code(e.kind()))
            };
            if json {
                if let Err(output_error) = library_cli::error_json(operation, e.as_ref(), code) {
                    eprintln!("cannot report structured failure: {output_error}");
                }
            } else {
                eprintln!("{}", prose(&message));
            }
            ExitCode::from(code)
        }
    }
}

//! Command-line interface to Auditory Lab.
#![forbid(unsafe_code)]
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
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Find built-in recipes and their durations.
    List,
    /// Read a recipe's description and research context.
    Describe(Source),
    /// Print or save an editable built-in TOML recipe.
    Recipe {
        recipe_id: String,
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Validate a recipe and inspect its technical parameters at one sample rate.
    Inspect {
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
    /// Exact built-in recipe ID (see list).
    recipe_id: Option<String>,
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
fn read(source: Source) -> Result<RecipeSnapshot, lab::Error> {
    if let Some(id) = source.recipe_id {
        lab::builtin(&id)
    } else if let Some(path) = source.file {
        lab::load_file(&path)
    } else {
        if io::stdin().is_terminal() {
            eprintln!(
                "Paste a UTF-8 TOML recipe, then end input with Ctrl-D (Unix) or Ctrl-Z followed by Enter (Windows)."
            );
        }
        lab::load_reader(io::stdin().lock(), "standard input")
    }
}
fn exit_code(kind: ErrorKind) -> u8 {
    match kind {
        ErrorKind::UnknownRecipe | ErrorKind::InvalidRecipe | ErrorKind::InvalidSound => 2,
        ErrorKind::Cancelled => 130,
        _ => 1,
    }
}
fn escape(text: &str) -> String {
    text.chars().flat_map(|c| { if (c.is_control() && c != '\n' && c != '\t') || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') { c.escape_default().collect::<Vec<_>>() } else { vec![c] } }).collect()
}
fn prose(text: &str) -> String {
    text.lines()
        .map(|line| textwrap::fill(&escape(line), 78))
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
        Some(Command::List) => say(&lab::catalog_listing(&lab::catalog()?))?,
        Some(Command::Recipe {
            recipe_id,
            output: path,
        }) => {
            if let Some(path) = path {
                lab::save_builtin(&recipe_id, &path, Replace::Never)?;
                completed(&format!("Saved recipe to {path:?}."));
            } else {
                output(lab::builtin_source(&recipe_id)?.as_bytes())?;
            }
        }
        Some(Command::Describe(source)) => say(&lab::describe(&read(source)?))?,
        Some(Command::Inspect {
            source,
            sample_rate,
        }) => say(&lab::inspection(&read(source)?.at(sample_rate)?))?,
        Some(Command::Export {
            source,
            output: path,
            sample_rate,
        }) => {
            let snapshot = read(source)?;
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
        Some(Command::Play(source)) => return play(&read(source)?),
    }
    Ok(Ended::Normally)
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
    match run(cli) {
        Ok(Ended::Normally) => ExitCode::SUCCESS,
        Ok(Ended::Stopped) => ExitCode::from(130),
        Err(e) => {
            let mut message = e.to_string();
            if e.downcast_ref::<lab::Error>()
                .is_some_and(|e| e.kind() == ErrorKind::UnknownRecipe)
            {
                message.push_str("; run `auditorylab list`, or use --file PATH");
            }
            eprintln!("{}", prose(&message));
            ExitCode::from(
                e.downcast_ref::<lab::Error>()
                    .map_or(1, |e| exit_code(e.kind())),
            )
        }
    }
}

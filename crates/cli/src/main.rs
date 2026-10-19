//! Command-line interface to Auditory Lab.
#![forbid(unsafe_code)]
use auditorylab::{self as lab, Cancel, ErrorKind, Origin, Preset, Progress, Replace, SampleRate};
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
    /// Find built-in sounds and their durations.
    List,
    /// Read a sound's description and research context.
    Describe(Source),
    /// Print or save an editable built-in TOML recipe.
    Recipe {
        preset_id: String,
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Validate a recipe and inspect its technical parameters.
    Inspect {
        #[command(flatten)]
        source: Source,
        #[arg(long, default_value="48000", value_parser=rate, value_name="HZ")]
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
        #[arg(long, default_value="48000", value_parser=rate, value_name="HZ")]
        sample_rate: SampleRate,
    },
}
#[derive(Args)]
#[group(id = "source", required = true, multiple = false)]
struct Source {
    /// Exact built-in ID (see list).
    preset_id: Option<String>,
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
fn read(source: Source) -> Result<Preset, lab::Error> {
    if let Some(id) = source.preset_id {
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
        ErrorKind::UnknownPreset | ErrorKind::InvalidPreset | ErrorKind::InvalidSound => 2,
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
fn cancel_on_interrupt() -> Result<Cancel, Box<dyn std::error::Error>> {
    let cancel = Cancel::default();
    let handler = cancel.clone();
    ctrlc::set_handler(move || {
        if handler.is_requested() {
            if io::stderr().is_terminal() {
                let _ = write!(io::stderr(), "\r\x1b[2K");
                let _ = io::stderr().flush();
            }
            std::process::exit(130);
        }
        handler.request();
    })?;
    Ok(cancel)
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
fn describe(preset: &Preset) -> String {
    let s = preset.stimulus();
    let mut out = format!(
        "{}\nSource: {}\nDuration: {}\nFade: {} at each edge\n",
        preset.title(),
        preset.origin(),
        lab::human_duration(s.duration_seconds),
        lab::human_duration(s.fade_seconds)
    );
    for (label, value) in preset.research() {
        out.push_str(&format!("{label}: {value}\n"));
    }
    if let Ok(sound) = preset.at(SampleRate::DEFAULT) {
        out.push_str(&format!(
            "Description at {} Hz:\n{}\n",
            SampleRate::DEFAULT.hz(),
            lab::sound_description(&sound, false)
        ));
    } else {
        for (i, component) in s.components.iter().enumerate() {
            out.push_str(&format!("Component {}: {component}\n", i + 1));
        }
    }
    out
}
fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        None => {
            output(format!("{}\n", Cli::command().render_help()).as_bytes())?;
        }
        Some(Command::List) => {
            let mut text = String::new();
            for preset in lab::catalog()? {
                if let Origin::BuiltIn(id) = preset.origin() {
                    text.push_str(&format!(
                        "{id}\n  {} ({})\n",
                        preset.title(),
                        lab::human_duration(preset.stimulus().duration_seconds)
                    ));
                    if let Some(a) = preset.about().and_then(|a| a.studied_for.as_deref()) {
                        text.push_str(&format!("  {a}\n"));
                    }
                }
            }
            say(&text)?;
        }
        Some(Command::Recipe {
            preset_id,
            output: path,
        }) => {
            if let Some(path) = path {
                lab::save_builtin(&preset_id, &path, Replace::Never)?;
                completed(&format!("Saved recipe to {path:?}."));
            } else {
                output(lab::builtin_source(&preset_id)?.as_bytes())?;
            }
        }
        Some(Command::Describe(source)) => say(&describe(&read(source)?))?,
        Some(Command::Inspect {
            source,
            sample_rate,
        }) => {
            let preset = read(source)?;
            let sound = preset.at(sample_rate)?;
            let signal = sound.signal();
            say(&format!(
                "{}\nSample rate: {} Hz\nFrames: {}\nDuration: {}\nFade: {} frames ({} s)\nPeak bounds: left {} dBFS, right {} dBFS",
                describe(&preset),
                sample_rate.hz(),
                signal.frames(),
                lab::human_duration(signal.duration_seconds()),
                signal.fade_frames(),
                signal.fade_seconds(),
                signal.peak_dbfs()[0],
                signal.peak_dbfs()[1]
            ))?;
            say(&lab::sound_description(&sound, true))?;
        }
        Some(Command::Export {
            source,
            output: path,
            sample_rate,
        }) => {
            let preset = read(source)?;
            let sound = preset.at(sample_rate)?;
            let cancel = cancel_on_interrupt()?;
            eprintln!(
                "{}",
                prose(&format!(
                    "Exporting {} to {path:?} at {} Hz. Press Ctrl-C to stop.",
                    preset.title(),
                    sample_rate.hz()
                ))
            );
            let result = lab::export(&sound, &path, Replace::Never, &cancel, &Progress::default())?;
            completed(&format!(
                "Exported {} frames to {path:?} ({:?}).",
                result.frames, result.container
            ));
        }
        Some(Command::Play(source)) => {
            let preset = read(source)?;
            let cancel = cancel_on_interrupt()?;
            let mut playback = lab::prepare_playback(&preset)?;
            let info = playback.output();
            eprintln!(
                "{}",
                prose(&format!(
                    "Preparing {}\nSource: {}\nDuration: {}\nDevice: {}\n{} Hz, {} channels, {}\n{}\nPress Ctrl-C to stop.",
                    preset.title(),
                    preset.origin(),
                    lab::human_duration(playback.sound().signal().duration_seconds()),
                    info.name.as_deref().unwrap_or("default output"),
                    info.sample_rate.hz(),
                    info.channels,
                    info.format,
                    preset
                        .notices()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n")
                ))
            );
            eprintln!(
                "{}",
                prose(&lab::sound_description(playback.sound(), false))
            );
            let tty = io::stderr().is_terminal();
            let result = if tty {
                let mut reader = playback.monitor(lab::MonitorDetail::Quantities).reader();
                let progress = Progress::default();
                let done = std::sync::atomic::AtomicBool::new(false);
                std::thread::scope(|scope| -> Result<lab::Played, Box<dyn std::error::Error>> {
                    let printer = scope.spawn(|| {
                        let mut terminal = StatusLine { active: false };
                        while !done.load(std::sync::atomic::Ordering::Acquire) {
                            reader.poll(0);
                            let state = reader.status();
                            terminal.draw(&format!(
                                "{} · {:?} · {:?} · {}",
                                lab::playback_elapsed(
                                    reader.descriptor().output.sample_rate,
                                    progress.frames()
                                ),
                                state.phase,
                                reader.freshness(),
                                lab::generated_description(state.observation.as_ref())
                            ));
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                    });
                    // Keep the discovered device on its caller thread; only plain
                    // shared status crosses to the terminal renderer.
                    let result = playback.play(&cancel, &progress);
                    done.store(true, std::sync::atomic::Ordering::Release);
                    printer
                        .join()
                        .map_err(|_| io::Error::other("status renderer failed"))?;
                    Ok(result?)
                })?
            } else {
                playback.play(&cancel, &Progress::default())?
            };
            completed(&format!("Played {} frames.", result.frames));
            if result.underruns > 0 {
                eprintln!("{}", lab::Notice::Underruns(result.underruns));
            }
        }
    }
    Ok(())
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
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let mut message = e.to_string();
            if e.downcast_ref::<lab::Error>()
                .is_some_and(|e| e.kind() == ErrorKind::UnknownPreset)
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

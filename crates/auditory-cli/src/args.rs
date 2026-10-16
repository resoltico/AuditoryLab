// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::failure::Failure;
use auditory_app::SampleRate;
use lexopt::Arg;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    Global,
    List,
    Describe,
    Recipe,
    Inspect,
    Play,
    Export,
}
impl HelpTopic {
    pub(crate) fn verb(self) -> &'static str {
        metadata(self).map_or("", |command| command.name)
    }
    pub(crate) fn accepts_file(self) -> bool {
        metadata(self).is_some_and(|command| command.options.contains(&OptionName::File))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateSelection {
    Default,
    Explicit,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help(HelpTopic),
    Version,
    BuildInfo,
    List,
    Describe {
        preset: InputSource,
    },
    Recipe {
        name: OsString,
        output: Option<PathBuf>,
    },
    Inspect {
        preset: InputSource,
        sample_rate: SampleRate,
        rate_selection: RateSelection,
    },
    Play {
        preset: InputSource,
    },
    Export {
        preset: InputSource,
        output: PathBuf,
        sample_rate: SampleRate,
        rate_selection: RateSelection,
    },
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputSource {
    Catalog(OsString),
    File(PathBuf),
    Stdin,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum OptionName {
    File,
    Stdin,
    Output,
    SampleRate,
}
impl OptionName {
    fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Stdin => "stdin",
            Self::Output => "output",
            Self::SampleRate => "sample-rate",
        }
    }
    fn help(self) -> String {
        match self {
            Self::File => {
                "--file PATH         Read a UTF-8 TOML recipe file (or --file=PATH).".into()
            }
            Self::Stdin => "--stdin             Read a UTF-8 TOML recipe until EOF.".into(),
            Self::Output => {
                "--output PATH       Save to a new file (or --output=PATH); never overwrite.".into()
            }
            Self::SampleRate => format!(
                "--sample-rate HZ    Whole decimal hertz from {} to {}; default {} Hz.",
                SampleRate::MIN_HZ,
                SampleRate::MAX_HZ,
                SampleRate::DEFAULT.hz()
            ),
        }
    }
}
#[derive(Clone, Copy)]
enum Sources {
    None,
    Catalog,
    Any,
}
struct CommandMetadata {
    topic: HelpTopic,
    name: &'static str,
    purpose: &'static str,
    usage: &'static str,
    options: &'static [OptionName],
    output_required: bool,
    sources: Sources,
    example: &'static str,
}
const COMMANDS: [CommandMetadata; 6] = [
    CommandMetadata {
        topic: HelpTopic::List,
        name: "list",
        purpose: "Find built-in sounds and their durations",
        usage: "list",
        options: &[],
        output_required: false,
        sources: Sources::None,
        example: "auditorylab list",
    },
    CommandMetadata {
        topic: HelpTopic::Describe,
        name: "describe",
        purpose: "Read a sound's description and research context",
        usage: "describe SOURCE",
        options: &[OptionName::File, OptionName::Stdin],
        output_required: false,
        sources: Sources::Any,
        example: "auditorylab describe chan-2022-clicks-40-hz",
    },
    CommandMetadata {
        topic: HelpTopic::Recipe,
        name: "recipe",
        purpose: "Print or save an editable built-in TOML recipe",
        usage: "recipe PRESET_ID [--output PATH]",
        options: &[OptionName::Output],
        output_required: false,
        sources: Sources::Catalog,
        example: "auditorylab recipe chan-2022-clicks-40-hz --output clicks.toml",
    },
    CommandMetadata {
        topic: HelpTopic::Inspect,
        name: "inspect",
        purpose: "Validate a recipe and inspect its technical parameters",
        usage: "inspect SOURCE [--sample-rate HZ]",
        options: &[OptionName::File, OptionName::Stdin, OptionName::SampleRate],
        output_required: false,
        sources: Sources::Any,
        example: "auditorylab inspect --file clicks.toml --sample-rate 48000",
    },
    CommandMetadata {
        topic: HelpTopic::Play,
        name: "play",
        purpose: "Play a sound on the default audio output",
        usage: "play SOURCE",
        options: &[OptionName::File, OptionName::Stdin],
        output_required: false,
        sources: Sources::Any,
        example: "auditorylab play chan-2022-clicks-40-hz",
    },
    CommandMetadata {
        topic: HelpTopic::Export,
        name: "export",
        purpose: "Save a sound as a new WAV file",
        usage: "export SOURCE --output PATH [--sample-rate HZ]",
        options: &[
            OptionName::File,
            OptionName::Stdin,
            OptionName::Output,
            OptionName::SampleRate,
        ],
        output_required: true,
        sources: Sources::Any,
        example: "auditorylab export --file clicks.toml --output clicks.wav",
    },
];
fn metadata(topic: HelpTopic) -> Option<&'static CommandMetadata> {
    COMMANDS.iter().find(|command| command.topic == topic)
}
pub(crate) fn short_usage(topic: HelpTopic) -> String {
    format!(
        "Usage: auditorylab {}",
        metadata(topic).map_or("COMMAND [OPTIONS]", |command| command.usage)
    )
}
pub(crate) fn example(topic: HelpTopic) -> &'static str {
    metadata(topic).map_or("auditorylab list", |command| command.example)
}
pub(crate) fn help(topic: HelpTopic) -> String {
    let Some(command) = metadata(topic) else {
        let mut text = format!(
            "Auditory Lab\nPlay research sound presets or export them as WAV files.\n\n{}\n\nCommands:\n",
            short_usage(topic)
        );
        for command in &COMMANDS {
            text.push_str(&format!("  {:<10} {}\n", command.name, command.purpose));
        }
        text.push_str("\nStart here:\n  auditorylab list\n  auditorylab describe chan-2022-clicks-40-hz\n  auditorylab play chan-2022-clicks-40-hz\n\nTurn the output volume down before running play.\nUse a preset ID, --file PATH, or --stdin to choose input.\nRun auditorylab COMMAND --help for command help.\nUse auditorylab --version to show version/build qualification.\nUse auditorylab --build-info for artifact identity JSON.\n");
        #[cfg(windows)]
        text.push_str(
            "For an unpacked download in PowerShell, start with .\\auditorylab.exe list.\n",
        );
        #[cfg(not(windows))]
        text.push_str("For an unpacked download, start with ./auditorylab list.\n");
        return text;
    };
    let mut text = format!("{}\n{}\n", short_usage(topic), command.purpose);
    match command.sources {
        Sources::Any => text.push_str("\nChoose exactly one SOURCE: a built-in PRESET_ID, --file PATH, or --stdin.\nA positional value always names a built-in preset.\nThis includes ./name, tone.toml, and -.\n"),
        Sources::Catalog => text.push_str("\nPRESET_ID names a built-in recipe.\nWithout --output, print its exact UTF-8 source.\nCustom file/stdin input is not accepted.\n"),
        Sources::None => {},
    }
    if !command.options.is_empty() {
        text.push_str("\nOptions:\n");
        for option in command.options {
            text.push_str(&format!("  {}\n", option.help()));
        }
    }
    if command.output_required {
        text.push_str("--output PATH is required.\n");
    }
    text.push_str("\n-h, --help show this help.\n");
    if !matches!(command.sources, Sources::None) {
        text.push_str("Options and the preset ID may appear in either order.\n-- ends option recognition; subsequent values are built-in IDs.\n");
    } else {
        text.push_str("A final -- is allowed; list takes no operands.\n");
    }
    if command.options.contains(&OptionName::File) {
        text.push_str("Use --file=-draft.toml for a hyphen-leading input path.\n--file - reads a file named -.\n");
    }
    if command.options.contains(&OptionName::Output) {
        text.push_str("Use --output=-draft.wav for a hyphen-leading output path.\n--output - writes a file named -.\n");
    }
    match topic {
        HelpTopic::Play => text.push_str("\nTurn output volume down before running play.\nPlayback uses the default output's stream rate and at least two channels.\nIt revalidates for that rate. Headphone advice preserves separate-ear routing.\nPress Ctrl-C to stop: the first handled interrupt requests cancellation;\nthe second forces exit. Already queued sound cannot be recalled.\n"),
        HelpTopic::Export => text.push_str("\nNew files only; an existing destination is never overwritten.\n--output - names a file, not standard output.\n"),
        HelpTopic::Recipe => text.push_str("\nNew files only; an existing destination is never overwritten.\n"),
        HelpTopic::Inspect => text.push_str("\nThe validation rate is separate from playback's actual stream rate;\nplayback revalidates for its device.\n"),
        _ => {},
    }
    text.push_str(&format!("\nExample: {}\n", command.example));
    text
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, Failure> {
    let args: Vec<_> = args.into_iter().collect();
    let usage = |topic, message| Failure::Usage { topic, message };
    let Some(verb) = args.first() else {
        return Ok(Command::Help(HelpTopic::Global));
    };
    if matches!(
        verb.to_str(),
        Some("-h" | "--help" | "--version" | "--build-info")
    ) {
        if args.len() != 1 {
            return Err(usage(
                HelpTopic::Global,
                format!("{verb:?} must be used alone"),
            ));
        }
        return Ok(if verb == "--version" {
            Command::Version
        } else if verb == "--build-info" {
            Command::BuildInfo
        } else {
            Command::Help(HelpTopic::Global)
        });
    }
    let command = COMMANDS
        .iter()
        .find(|command| verb == command.name)
        .ok_or_else(|| {
            let names = COMMANDS
                .iter()
                .map(|command| command.name)
                .collect::<Vec<_>>()
                .join(", ");
            usage(
                HelpTopic::Global,
                format!("unknown command {verb:?}; expected {names}"),
            )
        })?;
    let error = |message| usage(command.topic, message);
    // Exact raw standalone tokens deliberately win before all validation. Neither
    // bundled shorts nor equals-form values count as independent help requests.
    if args[1..]
        .iter()
        .take_while(|word| *word != "--")
        .any(|word| word == "-h" || word == "--help")
    {
        return Ok(Command::Help(command.topic));
    }
    let mut parser = lexopt::Parser::from_args(args.into_iter().skip(1));
    let mut source = None;
    let mut output = None;
    let mut rate = None;
    while let Some(argument) = parser.next().map_err(|e| error(e.to_string()))? {
        match argument {
            Arg::Short(short) => {
                return Err(error(format!(
                    "unsupported short option -{short}; only standalone -h is supported"
                )));
            }
            Arg::Long(name) => {
                let option = command
                    .options
                    .iter()
                    .copied()
                    .find(|option| option.name() == name)
                    .ok_or_else(|| error(format!("{} does not accept --{name}", command.name)))?;
                if option == OptionName::Stdin {
                    if parser.optional_value().is_some() {
                        return Err(error("--stdin takes no value".into()));
                    }
                    choose_source(&mut source, InputSource::Stdin).map_err(error)?;
                    continue;
                }
                if (option == OptionName::Output && output.is_some())
                    || (option == OptionName::SampleRate && rate.is_some())
                {
                    return Err(error(format!(
                        "--{} was specified more than once",
                        option.name()
                    )));
                }
                let attached = parser.optional_value();
                let equals = attached.is_some();
                let value = attached.map_or_else(|| parser.value(), Ok).map_err(|_| {
                    error(format!(
                        "--{} requires {}",
                        option.name(),
                        if option == OptionName::SampleRate {
                            "HZ"
                        } else {
                            "a file path"
                        }
                    ))
                })?;
                if value.is_empty() {
                    return Err(error(format!(
                        "--{} requires a nonempty value",
                        option.name()
                    )));
                }
                match option {
                    OptionName::File | OptionName::Output => {
                        if !equals && leading_option(&value) {
                            return Err(error(format!(
                                "--{} requires a file path; use --{}=-draft.{} for a hyphen-leading filename",
                                option.name(),
                                option.name(),
                                if option == OptionName::File {
                                    "toml"
                                } else {
                                    "wav"
                                }
                            )));
                        }
                        let path = PathBuf::from(value);
                        if option == OptionName::File {
                            choose_source(&mut source, InputSource::File(path)).map_err(error)?;
                        } else {
                            output = Some(path);
                        }
                    }
                    OptionName::SampleRate => {
                        let hz = value
                            .to_str()
                            .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
                            .and_then(|s| s.parse::<u32>().ok())
                            .ok_or_else(|| {
                                error(format!(
                                    "--sample-rate expects whole decimal hertz, got {value:?}"
                                ))
                            })?;
                        rate = Some(SampleRate::new(hz).map_err(|e| error(e.to_string()))?);
                    }
                    OptionName::Stdin => unreachable!(),
                }
            }
            Arg::Value(value) => {
                if matches!(command.sources, Sources::None) {
                    return Err(error(format!("{} takes no operands", command.name)));
                }
                if value.is_empty() {
                    return Err(error("expected a nonempty built-in preset ID".into()));
                }
                choose_source(&mut source, InputSource::Catalog(value)).map_err(error)?;
            }
        }
    }
    if command.output_required && output.is_none() {
        return Err(error(format!("{} requires --output PATH", command.name)));
    }
    if matches!(command.sources, Sources::None) {
        return Ok(Command::List);
    }
    let preset = source.ok_or_else(|| {
        error(format!(
            "{} requires exactly one {}",
            command.name,
            if matches!(command.sources, Sources::Catalog) {
                "built-in PRESET_ID"
            } else {
                "SOURCE: PRESET_ID, --file PATH, or --stdin"
            }
        ))
    })?;
    let sample_rate = rate.unwrap_or(SampleRate::DEFAULT);
    let rate_selection = if rate.is_some() {
        RateSelection::Explicit
    } else {
        RateSelection::Default
    };
    Ok(match command.topic {
        HelpTopic::Describe => Command::Describe { preset },
        HelpTopic::Recipe => {
            let InputSource::Catalog(name) = preset else {
                unreachable!("metadata permits only catalog recipe input");
            };
            Command::Recipe { name, output }
        }
        HelpTopic::Inspect => Command::Inspect {
            preset,
            sample_rate,
            rate_selection,
        },
        HelpTopic::Play => Command::Play { preset },
        HelpTopic::Export => {
            let Some(output) = output else {
                unreachable!("command metadata required the export destination");
            };
            Command::Export {
                preset,
                output,
                sample_rate,
                rate_selection,
            }
        }
        HelpTopic::Global | HelpTopic::List => unreachable!(),
    })
}
fn choose_source(current: &mut Option<InputSource>, source: InputSource) -> Result<(), String> {
    if current.is_some() {
        return Err("choose exactly one SOURCE; preset IDs, --file, and --stdin cannot be repeated or combined".into());
    }
    *current = Some(source);
    Ok(())
}
fn leading_option(value: &OsStr) -> bool {
    value != "-" && value.as_encoded_bytes().starts_with(b"-")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn words(args: &[&str]) -> Result<Command, Failure> {
        parse(args.iter().map(OsString::from))
    }
    #[test]
    fn global_forms_and_removed_commands_are_unambiguous() {
        for args in [vec![], vec!["-h"], vec!["--help"]] {
            assert_eq!(words(&args).unwrap(), Command::Help(HelpTopic::Global));
        }
        assert_eq!(words(&["--version"]).unwrap(), Command::Version);
        assert_eq!(words(&["--build-info"]).unwrap(), Command::BuildInfo);
        for args in [
            vec!["--"],
            vec!["--help", "list"],
            vec!["--version", "extra"],
            vec!["about", "x"],
            vec!["show", "x"],
            vec!["render", "--help"],
            vec!["-hh"],
        ] {
            assert!(words(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn exact_raw_help_wins_only_before_the_first_delimiter() {
        for command in &COMMANDS {
            for flag in ["-h", "--help"] {
                assert_eq!(
                    words(&[command.name, "--unknown", "--file", flag]).unwrap(),
                    Command::Help(command.topic)
                );
                assert_eq!(
                    words(&[command.name, "-hfoo", flag]).unwrap(),
                    Command::Help(command.topic)
                );
            }
        }
        for malformed in [
            "-hh",
            "-hfoo",
            "-h=foo",
            "--help=foo",
            "--hel",
            "-f",
            "-o",
            "-x",
        ] {
            assert!(words(&["play", malformed]).is_err(), "{malformed}");
        }
        assert!(
            matches!(words(&["play","--","--help"]).unwrap(),Command::Play{preset:InputSource::Catalog(id)} if id=="--help")
        );
        assert!(
            matches!(words(&["play","--file=--help"]).unwrap(),Command::Play{preset:InputSource::File(path)} if path.as_os_str()=="--help")
        );
        assert!(
            matches!(words(&["play","--file=--"]).unwrap(),Command::Play{preset:InputSource::File(path)} if path.as_os_str()=="--")
        );
        assert_eq!(words(&["list", "--"]).unwrap(), Command::List);
        assert!(words(&["list", "--", "extra"]).is_err());
    }
    #[test]
    fn positional_spellings_never_select_files_or_stdin() {
        for id in ["tone.toml", "./tone", "-", "a/b", "a\\b"] {
            for command in ["describe", "inspect", "play"] {
                let source = match words(&[command, id]).unwrap() {
                    Command::Describe { preset }
                    | Command::Inspect { preset, .. }
                    | Command::Play { preset } => preset,
                    _ => unreachable!(),
                };
                assert_eq!(source, InputSource::Catalog(id.into()));
            }
        }
        assert!(
            matches!(words(&["play","--","-hfoo"]).unwrap(),Command::Play{preset:InputSource::Catalog(id)} if id=="-hfoo")
        );
    }
    #[test]
    fn every_source_pair_and_duplicate_rejects_for_every_source_command() {
        let selectors: &[&[&str]] = &[&["id"], &["--file", "file.toml"], &["--stdin"]];
        for command in ["describe", "inspect", "play", "export"] {
            for first in selectors {
                for second in selectors {
                    let mut invocation = vec![command];
                    invocation.extend_from_slice(first);
                    invocation.extend_from_slice(second);
                    if command == "export" {
                        invocation.extend(["--output", "out.wav"]);
                    }
                    assert!(words(&invocation).is_err(), "{invocation:?}");
                }
            }
            let invocation = if command == "export" {
                vec![command, "--output", "out.wav"]
            } else {
                vec![command]
            };
            assert!(words(&invocation).is_err());
        }
    }
    #[test]
    fn supported_options_and_output_requirements_follow_command_metadata() {
        for command in &COMMANDS {
            for option in [
                OptionName::File,
                OptionName::Stdin,
                OptionName::Output,
                OptionName::SampleRate,
            ] {
                let mut invocation = vec![command.name];
                if !matches!(command.sources, Sources::None)
                    && !matches!(option, OptionName::File | OptionName::Stdin)
                {
                    invocation.push("id");
                }
                invocation.push(match option {
                    OptionName::File => "--file=x",
                    OptionName::Stdin => "--stdin",
                    OptionName::Output => "--output=x",
                    OptionName::SampleRate => "--sample-rate=48000",
                });
                if command.output_required && option != OptionName::Output {
                    invocation.push("--output=x");
                }
                assert_eq!(
                    words(&invocation).is_ok(),
                    command.options.contains(&option),
                    "{invocation:?}"
                );
            }
            if command.output_required {
                assert!(words(&[command.name, "id"]).is_err());
            }
        }
        for command in ["recipe", "export"] {
            assert!(words(&[command, "id", "--output=a", "--output=b"]).is_err());
        }
        for command in ["inspect", "export"] {
            let mut invocation = vec![command, "id", "--sample-rate=48000", "--sample-rate=44100"];
            if command == "export" {
                invocation.push("--output=x");
            }
            assert!(words(&invocation).is_err());
        }
        for selector in ["--stdin", "--file=x"] {
            assert!(words(&["recipe", selector]).is_err());
        }
    }
    #[test]
    fn flags_missing_empty_values_and_extra_operands_reject() {
        for invocation in [
            vec!["play", "--stdin=yes"],
            vec!["play", "--stdin="],
            vec!["play", "--file"],
            vec!["play", "--file="],
            vec!["play", "--file", ""],
            vec!["play", ""],
            vec!["list", "id"],
            vec!["recipe", "a", "b"],
            vec!["inspect", "--sample-rate"],
            vec!["inspect", "id", "--sample-rate="],
            vec!["export", "id", "--output"],
            vec!["export", "id", "--output="],
            vec!["export", "id", "--output", "--file=x"],
            vec!["play", "--file", "--"],
            vec!["play", "--stdin", "extra"],
            vec!["inspect", "--stdin", "--", "--sample-rate=48000"],
        ] {
            assert!(words(&invocation).is_err(), "{invocation:?}");
        }
    }
    #[test]
    fn native_path_values_support_equals_and_preserve_hyphen_rules() {
        for (option, command) in [("file", "play"), ("output", "recipe")] {
            for value in [
                "plain",
                "./same/../spelling",
                "-",
                "-draft.toml",
                "--help",
                "--",
            ] {
                let attached = format!("--{option}={value}");
                let flag = format!("--{option}");
                let mut invocation = vec![command];
                if option == "output" {
                    invocation.push("id");
                }
                invocation.push(&attached);
                assert!(words(&invocation).is_ok(), "{invocation:?}");
                invocation.pop();
                invocation.extend([&flag, value]);
                if value == "--help" {
                    assert_eq!(
                        words(&invocation).unwrap(),
                        Command::Help(if command == "play" {
                            HelpTopic::Play
                        } else {
                            HelpTopic::Recipe
                        })
                    );
                    continue;
                }
                assert_eq!(
                    words(&invocation).is_ok(),
                    !leading_option(OsStr::new(value)),
                    "{invocation:?}"
                );
            }
        }
        assert!(
            matches!(words(&["export","--stdin","--output=-"]).unwrap(),Command::Export{preset:InputSource::Stdin,output,..} if output.as_os_str()=="-")
        );
    }
    #[test]
    fn rates_keep_provenance_and_delegate_numeric_bounds() {
        assert!(
            matches!(words(&["inspect","id"]).unwrap(),Command::Inspect{sample_rate,rate_selection:RateSelection::Default,..} if sample_rate==SampleRate::DEFAULT)
        );
        for command in ["inspect", "export"] {
            for hz in [
                SampleRate::MIN_HZ,
                44_100,
                SampleRate::DEFAULT.hz(),
                SampleRate::MAX_HZ,
            ] {
                let hz = hz.to_string();
                let attached = format!("--sample-rate={hz}");
                for rate_args in [vec![attached.as_str()], vec!["--sample-rate", hz.as_str()]] {
                    let mut invocation = vec![command, "id"];
                    invocation.extend(rate_args);
                    if command == "export" {
                        invocation.push("--output=x");
                    }
                    let (actual, provenance) = match words(&invocation).unwrap() {
                        Command::Inspect {
                            sample_rate,
                            rate_selection,
                            ..
                        }
                        | Command::Export {
                            sample_rate,
                            rate_selection,
                            ..
                        } => (sample_rate, rate_selection),
                        _ => unreachable!(),
                    };
                    assert_eq!(actual.hz().to_string(), hz);
                    assert_eq!(provenance, RateSelection::Explicit);
                }
            }
        }
        for value in [
            "0",
            "768001",
            "-1",
            "+48000",
            "48k",
            "48000.0",
            " 48000",
            "٤٨٠٠٠",
            "999999999999999999999999",
        ] {
            assert!(
                words(&["inspect", "id", "--sample-rate", value]).is_err(),
                "{value}"
            );
        }
    }
    #[test]
    fn option_order_and_delimiters_keep_values_literal() {
        let expected = words(&["export", "id", "--output=out", "--sample-rate=44100"]).unwrap();
        for invocation in [
            vec!["export", "--output", "out", "--sample-rate", "44100", "id"],
            vec!["export", "--sample-rate=44100", "--output=out", "--", "id"],
        ] {
            assert_eq!(words(&invocation).unwrap(), expected);
        }
        assert!(
            matches!(words(&["inspect","--sample-rate=48000","--file=x"]).unwrap(),Command::Inspect{preset:InputSource::File(path),rate_selection:RateSelection::Explicit,..} if path.as_os_str()=="x")
        );
    }
    #[test]
    fn help_and_short_usage_are_driven_by_the_same_metadata() {
        let global = help(HelpTopic::Global);
        for command in &COMMANDS {
            assert!(global.contains(command.name));
            let text = help(command.topic);
            assert!(text.starts_with(&short_usage(command.topic)));
            assert!(text.contains(command.purpose));
            assert!(text.contains(example(command.topic)));
            for option in command.options {
                assert!(text.contains(&option.help()));
            }
            if command.output_required {
                assert!(text.contains("--output PATH is required"));
            }
        }
        assert!(global.contains("Turn the output volume down"));
        assert!(help(HelpTopic::Play).contains("Press Ctrl-C to stop"));
        assert!(!help(HelpTopic::Play).contains("staging"));
        assert!(help(HelpTopic::Export).contains("not standard output"));
    }
    #[cfg(unix)]
    #[test]
    fn non_unicode_paths_remain_native_in_separated_and_equals_forms() {
        use std::os::unix::ffi::OsStringExt;
        for option in ["file", "output"] {
            let value = OsString::from_vec(b"./path-\xff".to_vec());
            let mut attached = OsString::from(format!("--{option}="));
            attached.push(&value);
            for path_args in [
                vec![format!("--{option}").into(), value.clone()],
                vec![attached],
            ] {
                let mut invocation = vec![if option == "file" {
                    "play".into()
                } else {
                    "recipe".into()
                }];
                if option == "output" {
                    invocation.push("id".into());
                }
                invocation.extend(path_args);
                let parsed = match parse(invocation).unwrap() {
                    Command::Play {
                        preset: InputSource::File(path),
                    }
                    | Command::Recipe {
                        output: Some(path), ..
                    } => path,
                    _ => unreachable!(),
                };
                assert_eq!(parsed.as_os_str(), value);
            }
        }
    }
    #[cfg(windows)]
    #[test]
    fn non_unicode_windows_paths_remain_native_in_separated_and_equals_forms() {
        use std::os::windows::ffi::OsStringExt;
        let value = OsString::from_wide(&[b'.' as u16, b'\\' as u16, 0xd800]);
        for option in ["file", "output"] {
            let mut attached = OsString::from(format!("--{option}="));
            attached.push(&value);
            for path_args in [
                vec![format!("--{option}").into(), value.clone()],
                vec![attached],
            ] {
                let mut invocation = vec![if option == "file" {
                    "play".into()
                } else {
                    "recipe".into()
                }];
                if option == "output" {
                    invocation.push("id".into());
                }
                invocation.extend(path_args);
                let parsed = match parse(invocation).unwrap() {
                    Command::Play {
                        preset: InputSource::File(path),
                    }
                    | Command::Recipe {
                        output: Some(path), ..
                    } => path,
                    _ => unreachable!(),
                };
                assert_eq!(parsed.as_os_str(), value);
            }
        }
    }
}

#[cfg(test)]
mod help_layout_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;
    #[test]
    fn help_prose_and_examples_fit_the_declared_terminal_width() {
        for topic in std::iter::once(HelpTopic::Global).chain(COMMANDS.iter().map(|c| c.topic)) {
            for line in help(topic).lines() {
                assert!(line.width() <= 78, "{topic:?}: {line}");
            }
        }
    }
}

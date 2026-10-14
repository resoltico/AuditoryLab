// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::failure::Failure;
use auditory_core::SampleRate;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    Global,
    Play,
    Export,
    Inspect,
    List,
    Show,
    About,
}

impl HelpTopic {
    fn verb(self) -> &'static str {
        match self {
            Self::Global => "",
            Self::Play => "play",
            Self::Export => "export",
            Self::Inspect => "inspect",
            Self::List => "list",
            Self::Show => "show",
            Self::About => "about",
        }
    }
}

pub(crate) fn help(topic: HelpTopic) -> String {
    let common = "PRESET is - for standard input, a path containing /, \\ or ., otherwise a built-in name.\nUse ./name for a dotless file and ./- for a file named -.\nOptions and PRESET may appear in any order before --; -- only ends option parsing. Prefix option-looking output paths with ./ .";
    let rate = format!(
        "--sample-rate HZ: whole hertz from {} to {}; default {} Hz.",
        SampleRate::MIN_HZ,
        SampleRate::MAX_HZ,
        SampleRate::DEFAULT.hz()
    );
    match topic {
        HelpTopic::Global => format!(
            "Auditory Lab: inspect, export and play auditory stimuli.\n\nUsage:\n  auditory-lab (-h | --help)\n  auditory-lab --version\n  auditory-lab list\n  auditory-lab about [--] PRESET\n  auditory-lab show [--output OUTPUT] [--] NAME\n  auditory-lab play [--] PRESET\n  auditory-lab export --output OUTPUT [--sample-rate HZ] [--] PRESET\n  auditory-lab inspect [--sample-rate HZ] [--] PRESET\n\n{common}\nUse auditory-lab COMMAND --help for command help."
        ),
        HelpTopic::List => {
            "Usage: auditory-lab list\nList the built-in presets with durations and titles.".into()
        }
        HelpTopic::Show => "Usage: auditory-lab show [--output OUTPUT] [--] NAME\nPrint a built-in preset as UTF-8 TOML, or save it to a new OUTPUT file, ready to edit.\nAn existing OUTPUT is never replaced.\nExample: auditory-lab show chan-2022-clicks-40-hz --output clicks.toml".into(),
        HelpTopic::Play => format!(
            "Usage: auditory-lab play [--] PRESET\n{common}\nPlays on the default output at its configured rate; it needs at least two channels.\nBefore playing, the program advises on volume and headphones for this sound.\nThe first handled interrupt requests cancellation; the second forces exit. An operation may already have completed; cancellation cannot undo it, and forced exit may leave incomplete output.\nExample: auditory-lab play orozco-perez-2020-binaural-7-hz"
        ),
        HelpTopic::Export => format!(
            "Usage: auditory-lab export --output OUTPUT [--sample-rate HZ] [--] PRESET\n{common}\n--output OUTPUT is required. OUTPUT - names a file, not standard output.\n{rate}\nAn existing OUTPUT is never replaced. Incomplete output is visible during export; cooperative cancellation removes it under exclusive path ownership.\nExample: auditory-lab export chan-2022-clicks-40-hz --output clicks.wav"
        ),
        HelpTopic::About => format!(
            "Usage: auditory-lab about [--] PRESET\n{common}\nDescribe what a preset's sound is, what its source studied and what was found.\nExample: auditory-lab about orozco-perez-2020-binaural-7-hz"
        ),
        HelpTopic::Inspect => format!(
            "Usage: auditory-lab inspect [--sample-rate HZ] [--] PRESET\n{common}\n{rate}\nExample: auditory-lab inspect orozco-perez-2020-binaural-7-hz"
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help(HelpTopic),
    Version,
    List,
    Show {
        name: OsString,
        output: Option<PathBuf>,
    },
    Play {
        preset: InputSource,
    },
    Export {
        preset: InputSource,
        output: PathBuf,
        sample_rate: SampleRate,
    },
    Inspect {
        preset: InputSource,
        sample_rate: SampleRate,
    },
    About {
        preset: InputSource,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputSource {
    Stdin,
    File(PathBuf),
    Catalog(OsString),
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, Failure> {
    let args: Vec<_> = args.into_iter().collect();
    let usage = |topic, message| Failure::Usage { topic, message };
    let Some(verb) = args.first() else {
        return Err(usage(
            HelpTopic::Global,
            "expected a command: list, about, show, play, export, or inspect".into(),
        ));
    };
    if verb == "-h" || verb == "--help" || verb == "--version" {
        if args.len() != 1 {
            return Err(usage(
                HelpTopic::Global,
                format!("{verb:?} must be used alone"),
            ));
        }
        return Ok(if verb == "--version" {
            Command::Version
        } else {
            Command::Help(HelpTopic::Global)
        });
    }
    let topic = match verb.to_str() {
        Some("play") => HelpTopic::Play,
        Some("export") => HelpTopic::Export,
        Some("inspect") => HelpTopic::Inspect,
        Some("list") => HelpTopic::List,
        Some("show") => HelpTopic::Show,
        Some("about") => HelpTopic::About,
        _ => {
            return Err(usage(
                HelpTopic::Global,
                format!(
                    "unknown command {verb:?}; expected list, about, show, play, export, or inspect"
                ),
            ));
        }
    };
    let error = |message| usage(topic, message);
    if args[1..]
        .iter()
        .take_while(|word| *word != "--")
        .any(|word| word == "-h" || word == "--help")
    {
        return Ok(Command::Help(topic));
    }
    if topic == HelpTopic::List {
        return if args.len() == 1 {
            Ok(Command::List)
        } else {
            Err(error("list takes no arguments".into()))
        };
    }
    let mut preset = None;
    let mut destination = None;
    let mut rate = None;
    let mut literal = false;
    let mut words = args[1..].iter();
    while let Some(word) = words.next() {
        if !literal && word == "--" {
            literal = true;
            continue;
        }
        if !literal && (word == "--output" || word == "--sample-rate") {
            if topic == HelpTopic::Play
                || topic == HelpTopic::About
                || (topic == HelpTopic::Inspect && word == "--output")
                || (topic == HelpTopic::Show && word == "--sample-rate")
            {
                return Err(error(format!(
                    "{} does not accept {}",
                    topic.verb(),
                    word.to_string_lossy()
                )));
            }
            let output = word == "--output";
            if (output && destination.is_some()) || (!output && rate.is_some()) {
                return Err(error(format!(
                    "{} was specified more than once",
                    word.to_string_lossy()
                )));
            }
            let missing = if output {
                "--output requires a file path"
            } else {
                "--sample-rate requires HZ"
            };
            let value = words.next().ok_or_else(|| error(missing.into()))?;
            if value.is_empty()
                || (output && option(value))
                || value.as_encoded_bytes().starts_with(b"--")
            {
                return Err(error(missing.into()));
            }
            if output {
                destination = Some(PathBuf::from(value));
            } else {
                let hz = value.to_str().and_then(|s| s.parse().ok()).ok_or_else(|| {
                    error(format!(
                        "--sample-rate expects a whole number of hertz, got {value:?}"
                    ))
                })?;
                rate = Some(SampleRate::new(hz).map_err(|e| error(e.to_string()))?);
            }
        } else {
            if !literal && option(word) {
                return Err(error(format!(
                    "unknown option {word:?} for {}",
                    topic.verb()
                )));
            }
            if word.is_empty() {
                return Err(error("expected a nonempty preset argument".into()));
            }
            if preset.is_some() {
                return Err(error(format!("unexpected extra preset argument {word:?}")));
            }
            preset = Some(word.clone());
        }
    }
    if topic == HelpTopic::Show {
        return match preset {
            Some(name) => Ok(Command::Show {
                name,
                output: destination,
            }),
            None => Err(error("show requires one built-in preset name".into())),
        };
    }
    let preset = source(&preset.ok_or_else(|| {
        error(format!(
            "{} requires a preset argument: a built-in name, file path or - for standard input",
            topic.verb()
        ))
    })?);
    let sample_rate = rate.unwrap_or(SampleRate::DEFAULT);
    Ok(match topic {
        HelpTopic::Play => Command::Play { preset },
        HelpTopic::About => Command::About { preset },
        HelpTopic::Inspect => Command::Inspect {
            preset,
            sample_rate,
        },
        HelpTopic::Export => Command::Export {
            preset,
            output: destination.ok_or_else(|| error("export requires --output OUTPUT".into()))?,
            sample_rate,
        },
        HelpTopic::Global | HelpTopic::List | HelpTopic::Show => unreachable!(),
    })
}

fn source(word: &OsStr) -> InputSource {
    if word == "-" {
        InputSource::Stdin
    } else if word
        .as_encoded_bytes()
        .iter()
        .any(|b| matches!(b, b'/' | b'\\' | b'.'))
    {
        InputSource::File(word.into())
    } else {
        InputSource::Catalog(word.into())
    }
}

fn option(word: &OsStr) -> bool {
    word != "-" && word.as_encoded_bytes().starts_with(b"-")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_words(words: &[&str]) -> Result<Command, Failure> {
        parse(words.iter().map(OsString::from))
    }
    #[test]
    fn help_precedes_execution_only_before_the_delimiter() {
        for flag in ["-h", "--help"] {
            assert_eq!(
                parse_words(&[flag]).unwrap(),
                Command::Help(HelpTopic::Global)
            );
            for (verb, topic) in [
                ("play", HelpTopic::Play),
                ("export", HelpTopic::Export),
                ("inspect", HelpTopic::Inspect),
            ] {
                assert_eq!(
                    parse_words(&[verb, "absent", "--output", flag]).unwrap(),
                    Command::Help(topic)
                );
            }
        }
        assert!(parse_words(&["render", "--help"]).is_err());
        assert!(
            matches!(parse_words(&["inspect", "--", "-h"]).unwrap(), Command::Inspect { preset: InputSource::Catalog(p), .. } if p == OsStr::new("-h"))
        );
    }
    #[test]
    fn help_uses_the_core_rate_bounds_and_default_and_describes_cancellation_requests() {
        let rate = format!(
            "whole hertz from {} to {}; default {} Hz.",
            SampleRate::MIN_HZ,
            SampleRate::MAX_HZ,
            SampleRate::DEFAULT.hz()
        );
        for topic in [HelpTopic::Inspect, HelpTopic::Export] {
            assert!(help(topic).contains(&rate));
        }
        let playback = help(HelpTopic::Play);
        assert!(playback.contains("requests cancellation"));
        assert!(playback.contains("may already have completed"));
    }

    #[test]
    fn accepts_option_order_and_literal_paths() {
        let expected =
            parse_words(&["export", "a", "--output", "b", "--sample-rate", "44100"]).unwrap();
        for words in [
            ["export", "--output", "b", "--sample-rate", "44100", "a"],
            ["export", "--sample-rate", "44100", "a", "--output", "b"],
        ] {
            assert_eq!(parse_words(&words).unwrap(), expected);
        }
        assert!(parse_words(&["export", "--output", "./--help", "--", "--preset"]).is_ok());
        assert!(parse_words(&["inspect", "--", "-"]).is_ok());
        assert!(parse_words(&["play", "./-"]).is_ok());
    }
    #[test]
    fn rejects_excluded_forms_with_role_diagnostics() {
        for (words, message) in [
            (vec![], "expected a command"),
            (vec!["--help", "extra"], "must be used alone"),
            (
                vec!["play", "a", "--sample-rate", "1"],
                "play does not accept --sample-rate",
            ),
            (
                vec!["inspect", "a", "--output", "b"],
                "inspect does not accept --output",
            ),
            (
                vec!["inspect", "a", "--rate"],
                "unknown option \"--rate\" for inspect",
            ),
            (
                vec!["export", "a", "--output", ""],
                "--output requires a file path",
            ),
            (
                vec!["export", "a", "--output", "-x"],
                "--output requires a file path",
            ),
            (
                vec!["inspect", "a", "--sample-rate", ""],
                "--sample-rate requires HZ",
            ),
            (vec!["export", "a"], "export requires --output OUTPUT"),
            (
                vec!["export", "a", "--output"],
                "--output requires a file path",
            ),
            (
                vec!["export", "a", "--output", "--rate"],
                "--output requires a file path",
            ),
            (
                vec!["inspect", "a", "--sample-rate"],
                "--sample-rate requires HZ",
            ),
            (
                vec!["inspect", "a", "--sample-rate", "--output"],
                "--sample-rate requires HZ",
            ),
            (
                vec!["inspect", "a", "--sample-rate", "1", "--sample-rate", "1"],
                "--sample-rate was specified more than once",
            ),
            (
                vec!["export", "a", "--output", "b", "--output", "b"],
                "--output was specified more than once",
            ),
            (
                vec!["inspect", "a", "b"],
                "unexpected extra preset argument \"b\"",
            ),
            (vec!["inspect"], "inspect requires a preset argument"),
            (vec!["inspect", ""], "nonempty preset"),
            (vec!["inspect", "a", "--sample-rate", "48k"], "got \"48k\""),
        ] {
            let e = parse_words(&words).unwrap_err();
            assert_eq!(e.exit_code(), 2);
            assert!(e.to_string().contains(message), "{words:?}: {e}");
        }
        for hz in ["0", "768001"] {
            assert_eq!(
                parse_words(&["inspect", "a", "--sample-rate", hz])
                    .unwrap_err()
                    .to_string(),
                SampleRate::new(hz.parse().unwrap())
                    .unwrap_err()
                    .to_string()
            );
        }
    }
    #[test]
    fn every_preset_command_names_the_shared_argument_contract() {
        for command in ["inspect", "export", "play"] {
            for operands in [vec![], vec![""], vec!["a", "b"]] {
                let mut words = vec![command];
                if command == "export" {
                    words.extend(["--output", "out.wav"]);
                }
                words.extend(operands);
                let text = parse_words(&words).unwrap_err().to_string();
                assert!(text.contains("preset argument"), "{words:?}: {text}");
                assert!(!text.contains("preset path"));
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_paths_remain_native() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'.', b'/', b'a', 255]);
        assert!(
            matches!(parse(["play".into(), path.clone()]).unwrap(), Command::Play { preset: InputSource::File(p) } if p.as_os_str() == path)
        );
    }
}

//! Ordinary CLI discovery, selectors, structured output and managed lifecycle grammar.
use super::{Source, output, prose, usage};
use crate::cli_text::CommandProblem;
use auditorylab_app::core::SampleRate;
use auditorylab_app::discovery::query::{Direction, GroupBy, Scope, SortBy};
use auditorylab_app::document::origin::{LibraryKey, VersionToken};
use auditorylab_app::job::Cancel;
use auditorylab_app::vocabulary::Operation;
use auditorylab_app::{
    self as lab,
    discovery::collection::{CatalogQuery, DateRange},
    services::library::Library,
};

use auditorylab_presentation::report::Report;
use clap::{Args, Subcommand, ValueEnum};
use lab::services::LibraryContext;
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};
type Result<T> = std::result::Result<T, super::CliError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ReadFormat {
    Text,
    Json,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ListFormat {
    Text,
    Json,
    Ids,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum CollectionArg {
    #[value(name = Scope::All.key(), help = Scope::All.label())]
    All,
    #[value(name = Scope::BuiltIn.key(), help = Scope::BuiltIn.label())]
    Builtin,
    #[value(name = Scope::User.key(), help = Scope::User.label())]
    User,
}
impl CollectionArg {
    fn scope(self) -> Scope {
        match self {
            Self::All => Scope::All,
            Self::Builtin => Scope::BuiltIn,
            Self::User => Scope::User,
        }
    }
}
#[derive(Args)]
pub(super) struct QueryArgs {
    /// Words to find in titles, IDs and what a sound was studied for; all words must match.
    #[arg(long, default_value = "")]
    search: String,
    /// Rhythm bands to include (any of them).
    #[arg(long,value_parser=parse_choice::<lab::core::rhythm::Band>)]
    band: Vec<lab::core::rhythm::Band>,
    /// How the rhythm is made (any of them); with --band, both must describe the same rhythm.
    #[arg(long,value_parser=parse_choice::<lab::core::rhythm::Method>)]
    method: Vec<lab::core::rhythm::Method>,
    /// Study DOI or source key of the sound's own study.
    #[arg(long)]
    study: Vec<String>,
    /// The sound's role in its study.
    #[arg(long,value_parser=parse_role)]
    role: Vec<lab::core::provenance::ConditionRole>,
    /// Who or what the study tested.
    #[arg(long,value_parser=parse_population)]
    population: Vec<lab::core::provenance::Population>,
    /// Whether the rhythm analysis is complete or invalid.
    #[arg(long,value_parser=parse_analysis)]
    analysis: Option<lab::core::rhythm::Completeness>,
    // A plain help string: rustdoc's Markdown lint would put backticks into rendered help.
    #[arg(
        long,
        help = "Rhythm state: none, multiple, outside_bands or needs_review",
        value_parser = parse_choice::<auditorylab_app::discovery::query::RhythmState>
    )]
    rhythms: Option<auditorylab_app::discovery::query::RhythmState>,
    /// Created at or after this time, such as 2026-10-24T00:00:00+03:00.
    #[arg(long)]
    created_from: Option<lab::core::Timestamp>,
    /// Created before this time.
    #[arg(long)]
    created_before: Option<lab::core::Timestamp>,
    /// Edited at or after this time.
    #[arg(long)]
    modified_from: Option<lab::core::Timestamp>,
    /// Edited before this time.
    #[arg(long)]
    modified_before: Option<lab::core::Timestamp>,
    /// Added to your library at or after this time.
    #[arg(long)]
    added_from: Option<lab::core::Timestamp>,
    /// Added to your library before this time.
    #[arg(long)]
    added_before: Option<lab::core::Timestamp>,
    /// Sort order; without it, user and Trash lists sort by last edit, others by rhythm rate.
    #[arg(long,value_parser=parse_sort)]
    sort: Option<SortBy>,
    /// Sort direction; unknown values always come last.
    #[arg(long,value_parser=parse_choice::<Direction>)]
    direction: Option<Direction>,
    /// Show the results in groups.
    #[arg(long, default_value = "none",value_parser=parse_choice::<GroupBy>)]
    group_by: GroupBy,
    /// Output as readable text, JSON, or bare IDs (IDs need a complete listing).
    #[arg(long, value_enum, default_value = "text")]
    format: ListFormat,
}
impl QueryArgs {
    fn query(self, scope: Scope) -> Result<(CatalogQuery, ListFormat)> {
        let query = CatalogQuery {
            scope,
            include_external: false,
            search: self.search,
            rhythm: lab::discovery::query::RhythmQuery {
                bands: self.band,
                methods: self.method,
                state: self.rhythms,
            },
            studies: self.study,
            roles: self.role,
            populations: self.population,
            analysis: self.analysis,
            created: DateRange {
                from: self.created_from,
                before: self.created_before,
            },
            modified: DateRange {
                from: self.modified_from,
                before: self.modified_before,
            },
            added: DateRange {
                from: self.added_from,
                before: self.added_before,
            },
            sort: self.sort.unwrap_or(SortBy::Automatic),
            direction: self.direction,
            group_by: self.group_by,
        };
        query.validate()?;
        if self.format == ListFormat::Ids && query.group_by != GroupBy::None {
            return Err(usage(CommandProblem::GroupedIds.message()));
        }
        Ok((query, self.format))
    }
}
#[derive(Args)]
pub(super) struct ListArgs {
    /// Which recipes to list; --file inputs need all.
    #[arg(long, value_enum, default_value = "all")]
    collection: CollectionArg,
    /// Also list these recipe files for this run, without adding them to your library.
    #[arg(long = "file", value_name = "PATH")]
    files: Vec<PathBuf>,
    #[command(flatten)]
    query: Box<QueryArgs>,
}
impl ListArgs {
    pub(super) fn json(&self) -> bool {
        self.query.format == ListFormat::Json
    }
}
#[derive(Subcommand)]
pub(super) enum LibraryCommand {
    /// Show where your User Library is and whether it is set up; changes nothing.
    Path {
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Set up your User Library folder, or finish a setup that was interrupted.
    Init {
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Add a recipe file to your library; the original file stays untouched.
    Add {
        /// Recipe file to add.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        /// Add it as a new copy with its own identity, noting the original.
        #[arg(long)]
        as_copy: bool,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Replace one of your recipes with the content of a recipe file.
    Update {
        /// The User recipe, as user:<id> from list.
        selector: LibraryKey,
        /// Recipe file with the new content.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        /// Refuse unless the recipe still has this version token (from an earlier report).
        #[arg(long)]
        expect: Option<VersionToken>,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Change the title of one of your recipes; its sound and identity stay the same.
    Rename {
        /// The User recipe, as user:<id> from list.
        selector: LibraryKey,
        /// The new title.
        #[arg(long)]
        title: String,
        /// Refuse unless the recipe still has this version token (from an earlier report).
        #[arg(long)]
        expect: Option<VersionToken>,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Move one of your recipes to Trash, where it can be restored.
    Trash {
        /// The User recipe, as user:<id> from list.
        selector: LibraryKey,
        /// Refuse unless the recipe still has this version token (from an earlier report).
        #[arg(long)]
        expect: Option<VersionToken>,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Bring a recipe back from Trash.
    Restore {
        /// The User recipe, as user:<id> from list.
        selector: LibraryKey,
        /// Refuse unless the recipe still has this version token (from an earlier report).
        #[arg(long)]
        expect: Option<VersionToken>,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Permanently delete a recipe that is in Trash; files outside the library are never touched.
    Purge {
        /// The User recipe, as user:<id> from list.
        selector: LibraryKey,
        /// Refuse unless the recipe still has this version token (from an earlier report).
        #[arg(long)]
        expect: Option<VersionToken>,
        /// Confirm the permanent deletion.
        #[arg(long)]
        yes: bool,
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// List the recipes in Trash; nothing is deleted automatically.
    ListTrash(Box<QueryArgs>),
    /// Check your library for problems without changing or repairing anything.
    Check {
        /// Output as readable text or JSON for scripts.
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
}
impl LibraryCommand {
    pub(super) fn json(&self) -> bool {
        match self {
            Self::ListTrash(q) => q.format == ListFormat::Json,
            Self::Path { format }
            | Self::Init { format }
            | Self::Add { format, .. }
            | Self::Update { format, .. }
            | Self::Rename { format, .. }
            | Self::Trash { format, .. }
            | Self::Restore { format, .. }
            | Self::Purge { format, .. }
            | Self::Check { format } => *format == ReadFormat::Json,
        }
    }
    pub(super) fn operation(&self) -> Operation {
        match self {
            Self::Path { .. } => Operation::LibraryPath,
            Self::Init { .. } => Operation::LibraryInit,
            Self::Add { .. } => Operation::LibraryAdd,
            Self::Update { .. } => Operation::LibraryUpdate,
            Self::Rename { .. } => Operation::LibraryRename,
            Self::Trash { .. } => Operation::LibraryTrash,
            Self::Restore { .. } => Operation::LibraryRestore,
            Self::Purge { .. } => Operation::LibraryPurge,
            Self::ListTrash(_) => Operation::ListTrash,
            Self::Check { .. } => Operation::LibraryCheck,
        }
    }
}
fn expected_choices<'a>(keys: impl Iterator<Item = &'a str>) -> String {
    let keys = keys.collect::<Vec<_>>();
    match keys.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("expected {} or {last}", rest.join(", ")),
        Some((last, _)) => format!("expected {last}"),
        None => "no choices are available".into(),
    }
}
fn parse_choice<T: lab::core::vocabulary::Vocabulary + std::str::FromStr>(
    text: &str,
) -> std::result::Result<T, String> {
    text.parse()
        .map_err(|_| expected_choices(T::ALL.iter().map(|value| value.key())))
}
fn parse_sort(text: &str) -> std::result::Result<SortBy, String> {
    // Automatic applies when --sort is omitted, so only the app's other sorts are offered.
    let explicit = || {
        SortBy::ALL
            .into_iter()
            .filter(|&value| value != SortBy::Automatic)
    };
    explicit()
        .find(|value| value.key() == text)
        .ok_or_else(|| expected_choices(explicit().map(SortBy::key)))
}
fn parse_role(text: &str) -> std::result::Result<lab::core::provenance::ConditionRole, String> {
    parse_choice(text)
}
fn parse_population(text: &str) -> std::result::Result<lab::core::provenance::Population, String> {
    parse_choice(text)
}
fn parse_analysis(text: &str) -> std::result::Result<lab::core::rhythm::Completeness, String> {
    parse_choice(text)
}
pub(super) fn open_library(context: &LibraryContext) -> Result<&Library> {
    Ok(context.library()?)
}
use lab::services::Resolved;
pub(super) fn resolve(source: Source, context: &LibraryContext) -> Result<Resolved> {
    let request = if let Some(selector) = source.selector {
        lab::services::Source::Selector(selector.parse()?)
    } else if let Some(file) = source.file {
        lab::services::Source::File(file)
    } else {
        if io::stdin().is_terminal() {
            eprintln!("{}", crate::cli_text::Caption::PasteRecipe.label());
        }
        return Ok(lab::services::resolve(
            lab::services::Source::Stream {
                reader: &mut io::stdin().lock(),
                label: crate::cli_text::Caption::StandardInput.label(),
            },
            context,
        )?);
    };
    Ok(lab::services::resolve(request, context)?)
}
pub(super) fn list(args: ListArgs, context: &LibraryContext) -> Result<u8> {
    let (query, format) = args.query.query(args.collection.scope())?;
    if format == ListFormat::Ids && !args.files.is_empty() {
        return Err(usage(CommandProblem::TemporaryIds.message()));
    }
    let listing = lab::services::list(
        lab::services::ListRequest {
            query,
            files: args.files,
        },
        context,
        &Cancel::default(),
    )?;
    print_list(Operation::List, &listing, format)?;
    Ok(listing.outcome().map_or(0, super::exit_code))
}
fn print_list(
    operation: Operation,
    listing: &lab::services::Listing,
    format: ListFormat,
) -> Result<()> {
    match format {
        ListFormat::Ids => {
            if listing.complete() {
                let ids = listing
                    .entries()
                    .iter()
                    .map(|entry| entry.key.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !ids.is_empty() {
                    output(format!("{ids}\n").as_bytes())?;
                }
            } else {
                for diagnostic in listing.diagnostics() {
                    eprintln!(
                        "{}",
                        prose(
                            &auditorylab_presentation::errors::library_diagnostic_message(
                                diagnostic
                            )
                        )
                    );
                }
            }
        }
        ListFormat::Json => print_json(&Report::listing(operation, listing))?,
        ListFormat::Text => {
            for entry in listing.entries() {
                output(
                    auditorylab_presentation::library::entry_summary_list_description(
                        entry,
                        listing.query().effective_sort().0,
                    )
                    .as_bytes(),
                )?;
            }
            if !listing.complete() {
                eprintln!(
                    "{}",
                    auditorylab_presentation::library::INVENTORY_INCOMPLETE
                );
            }
            for diagnostic in listing.diagnostics() {
                eprintln!(
                    "{}: {}",
                    auditorylab_presentation::display_scalar(&(diagnostic.storage_id)),
                    prose(
                        &auditorylab_presentation::errors::library_diagnostic_message(diagnostic)
                    )
                );
            }
        }
    }
    Ok(())
}
pub(super) fn describe(resolved: &Resolved, format: ReadFormat) -> Result<()> {
    let description = lab::services::describe(resolved)?;
    if format == ReadFormat::Json {
        print_json(&Report::describe(&description))?;
    } else {
        let extra = auditorylab_presentation::library::entry_summary_membership_description(
            resolved.summary(),
        );
        output(
            format!(
                "{}{}\n",
                prose(&auditorylab_presentation::recipe_description(
                    &(description)
                )),
                prose(&extra)
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn inspect(resolved: &Resolved, rate: SampleRate, format: ReadFormat) -> Result<u8> {
    let inspection = lab::services::inspect(resolved, rate);
    if format == ReadFormat::Text {
        let sound = inspection.sound().map_err(Clone::clone)?;
        output(
            format!(
                "{}\n",
                prose(&auditorylab_presentation::rendition_inspection(sound))
            )
            .as_bytes(),
        )?;
    } else {
        print_json(&Report::inspect(&inspection))?;
    }
    Ok(inspection.outcome().map_or(0, super::exit_code))
}
pub(super) fn library(command: LibraryCommand, context: &LibraryContext) -> Result<u8> {
    let operation = command.operation();
    match command {
        LibraryCommand::Path { format } => library_path(context, operation, format),
        LibraryCommand::Init { format } => {
            let initialized = open_library(context)?.initialize()?;
            if format == ReadFormat::Json {
                print_mutation_json(&Report::initialized(operation, &initialized));
            } else {
                output(
                    auditorylab_presentation::library::initialization_description(&(initialized))
                        .as_bytes(),
                )?;
            }
            Ok(0)
        }
        LibraryCommand::ListTrash(args) => {
            let (query, format) = args.query(Scope::Trash)?;
            let listing = lab::services::list(
                lab::services::ListRequest {
                    query,
                    files: Vec::new(),
                },
                context,
                &Cancel::default(),
            )?;
            print_list(operation, &listing, format)?;
            Ok(listing.outcome().map_or(0, super::exit_code))
        }
        LibraryCommand::Check { format } => check_library(context, operation, format),
        command => {
            if matches!(&command, LibraryCommand::Purge { yes: false, .. }) {
                return Err(usage(CommandProblem::PurgeConfirmation.message()));
            }
            let cancel = Cancel::default();
            let stop = cancel.clone();
            super::on_interrupt(move || stop.request())?;
            let library = open_library(context)?;
            let (outcome, format) = match command {
                LibraryCommand::Add {
                    file,
                    as_copy,
                    format,
                } => (library.add_file(&file, as_copy, &cancel)?, format),
                LibraryCommand::Update {
                    selector,
                    file,
                    expect,
                    format,
                } => (
                    library.update_file(selector, expect, &file, &cancel)?,
                    format,
                ),
                LibraryCommand::Rename {
                    selector,
                    title,
                    expect,
                    format,
                } => (library.rename(selector, expect, title, &cancel)?, format),
                LibraryCommand::Trash {
                    selector,
                    expect,
                    format,
                } => (library.trash(selector, expect, &cancel)?, format),
                LibraryCommand::Restore {
                    selector,
                    expect,
                    format,
                } => (library.restore(selector, expect, &cancel)?, format),
                LibraryCommand::Purge {
                    selector,
                    expect,
                    yes,
                    format,
                } => (library.purge(selector, expect, yes, &cancel)?, format),
                _ => unreachable!("read-only library commands handled above"),
            };
            report_mutation(
                &outcome,
                operation,
                format,
                &mut io::stdout().lock(),
                &mut io::stderr().lock(),
            )
        }
    }
}
// The service establishes publication truth before any fallible terminal output.
fn report_mutation(
    outcome: &lab::services::library::WriteOutcome,
    operation: Operation,
    format: ReadFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let mut bytes = if format == ReadFormat::Json {
        auditorylab_presentation::report::json_bytes(&Report::mutation(operation, outcome))?
    } else {
        auditorylab_presentation::display_text(
            &(auditorylab_presentation::library::write_outcome_description(outcome)),
        )
        .into_bytes()
    };
    bytes.push(b'\n');
    if let Err(error) = stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
        let _ = writeln!(
            stderr,
            "{}",
            auditorylab_presentation::display_text(
                &(auditorylab_presentation::library::write_outcome_completion_failure(
                    outcome, error
                ))
            )
        );
        let _ = stderr.flush();
    }
    Ok(0)
}

pub(super) fn print_json(value: &Report) -> Result<()> {
    let bytes = auditorylab_presentation::report::json_bytes(value)?;
    output(&bytes)?;
    output(b"\n")?;
    Ok(())
}
fn print_mutation_json(value: &Report) {
    let mut bytes = match auditorylab_presentation::report::json_bytes(value) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "{}",
                auditorylab_presentation::display_text(
                    &(auditorylab_presentation::library::report_completion_failure(
                        value.complete,
                        error
                    ))
                )
            );
            return;
        }
    };
    bytes.push(b'\n');
    if let Err(error) = io::stdout()
        .lock()
        .write_all(&bytes)
        .and_then(|()| io::stdout().lock().flush())
    {
        eprintln!(
            "{}",
            auditorylab_presentation::display_text(
                &(auditorylab_presentation::library::report_completion_failure(
                    value.complete,
                    error
                ))
            )
        );
    }
}
pub(super) fn error_json(operation: Operation, error: &super::CliError) -> Result<()> {
    print_json(&Report::error(
        operation,
        error,
        error.message(),
        error.outcome(),
    ))
}

fn library_path(context: &LibraryContext, operation: Operation, format: ReadFormat) -> Result<u8> {
    let inspection = lab::services::library::path(context)?;
    let root = inspection.location;
    let info = match inspection.result {
        Ok(info) => info,
        Err(error) => {
            let exists = inspection.exists;
            if format == ReadFormat::Json {
                print_json(&Report::unavailable_path(operation, &root, exists, &error))?;
            } else {
                output(
                    auditorylab_presentation::library::location_inspection_failure(
                        &(root),
                        exists,
                        &error,
                    )
                    .as_bytes(),
                )?;
            }
            return Ok(
                if matches!(
                    error.outcome(),
                    auditorylab_app::outcome::Outcome::Busy
                        | auditorylab_app::outcome::Outcome::LockUnavailable
                ) {
                    4
                } else {
                    5
                },
            );
        }
    };
    if format == ReadFormat::Json {
        print_json(&Report::path(operation, info.clone()))?;
    } else {
        output(auditorylab_presentation::library::root_info_description(&(info)).as_bytes())?;
    }
    Ok(0)
}

fn check_library(context: &LibraryContext, operation: Operation, format: ReadFormat) -> Result<u8> {
    let inventory = open_library(context)?.scan(&Cancel::default());
    if format == ReadFormat::Json {
        print_json(&Report::inventory(operation, &inventory))?;
    } else {
        output(auditorylab_presentation::library::inventory_description(&(inventory)).as_bytes())?;
        for diagnostic in &inventory.diagnostics {
            eprintln!(
                "{}: {}",
                auditorylab_presentation::display_scalar(&(diagnostic.storage_id)),
                prose(&auditorylab_presentation::errors::library_diagnostic_message(diagnostic))
            );
        }
        for stage in &inventory.stages {
            output(auditorylab_presentation::retained_stage(&stage.to_string_lossy()).as_bytes())?;
        }
    }
    Ok(inventory.failure_kind.map_or(0, super::exit_code))
}

#[cfg(test)]
mod query_mapping_tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Probe {
        #[command(flatten)]
        args: ListArgs,
    }
    #[test]
    fn discovery_flags_map_to_an_independent_typed_query() {
        for (collection, scope) in [
            ("all", Scope::All),
            ("builtin", Scope::BuiltIn),
            ("user", Scope::User),
        ] {
            let probe = Probe::try_parse_from([
                "probe",
                "--collection",
                collection,
                "--search",
                "actual text",
                "--band",
                "theta",
                "--method",
                "monaural",
                "--study",
                "key:source",
                "--role",
                "comparison",
                "--population",
                "mouse",
                "--analysis",
                "invalid",
                "--rhythms",
                "outside_bands",
                "--created-from",
                "2026-01-01T00:00:00Z",
                "--created-before",
                "2026-02-01T00:00:00Z",
                "--modified-from",
                "2026-03-01T00:00:00Z",
                "--modified-before",
                "2026-04-01T00:00:00Z",
                "--added-from",
                "2026-05-01T00:00:00Z",
                "--added-before",
                "2026-06-01T00:00:00Z",
                "--sort",
                "modified",
                "--direction",
                "desc",
                "--group-by",
                "study",
                "--format",
                "json",
            ])
            .unwrap();
            assert_eq!(probe.args.collection.scope(), scope);
            let (actual, format) = probe.args.query.query(scope).unwrap();
            let expected = CatalogQuery {
                scope,
                include_external: false,
                search: "actual text".into(),
                rhythm: lab::discovery::query::RhythmQuery {
                    bands: vec![lab::core::rhythm::Band::Theta],
                    methods: vec![lab::core::rhythm::Method::Monaural],
                    state: Some(auditorylab_app::discovery::query::RhythmState::OutsideBands),
                },
                studies: vec!["key:source".into()],
                roles: vec![lab::core::provenance::ConditionRole::Comparison],
                populations: vec![lab::core::provenance::Population::Mouse],
                analysis: Some(lab::core::rhythm::Completeness::Invalid),
                created: DateRange {
                    from: Some("2026-01-01T00:00:00Z".parse().unwrap()),
                    before: Some("2026-02-01T00:00:00Z".parse().unwrap()),
                },
                modified: DateRange {
                    from: Some("2026-03-01T00:00:00Z".parse().unwrap()),
                    before: Some("2026-04-01T00:00:00Z".parse().unwrap()),
                },
                added: DateRange {
                    from: Some("2026-05-01T00:00:00Z".parse().unwrap()),
                    before: Some("2026-06-01T00:00:00Z".parse().unwrap()),
                },
                sort: SortBy::Modified,
                direction: Some(Direction::Desc),
                group_by: GroupBy::Study,
            };
            assert_eq!(actual, expected);
            assert_eq!(format, ListFormat::Json);
        }
    }
}

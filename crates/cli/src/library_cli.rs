//! Ordinary CLI discovery, selectors, structured output and managed lifecycle grammar.
use super::{Source, output, prose, usage};
use crate::cli_text::CommandProblem;
use auditorylab_app::core::SampleRate;
use auditorylab_app::core::query::{Direction, GroupBy, Scope, SortBy};
use auditorylab_app::presentation::DisplayText;
use auditorylab_app::presentation::reports::Report;
use auditorylab_app::services::Cancel;
use auditorylab_app::{
    self as lab,
    discovery::collection::{CatalogQuery, DateRange},
    services::library::{Library, LibraryKey, VersionToken},
};
use clap::{Args, Subcommand, ValueEnum};
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
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
    /// Token-AND Unicode-normalized text search; numeric terms match whole values.
    #[arg(long, default_value = "")]
    search: String,
    /// Nominal reference bands, OR within this facet.
    #[arg(long,value_parser=parse_choice::<lab::core::rhythm::Band>)]
    band: Vec<lab::core::rhythm::Band>,
    /// Structural methods; band/method must match one descriptor.
    #[arg(long,value_parser=parse_choice::<lab::core::rhythm::Method>)]
    method: Vec<lab::core::rhythm::Method>,
    /// Current-condition DOI or source key, never ancestor-only evidence.
    #[arg(long)]
    study: Vec<String>,
    /// Condition role correlated within the same evidence record.
    #[arg(long,value_parser=parse_role)]
    role: Vec<lab::core::ConditionRole>,
    /// Population correlated within the same evidence record.
    #[arg(long,value_parser=parse_population)]
    population: Vec<lab::core::Population>,
    /// Nominal analysis coverage: complete or invalid.
    #[arg(long,value_parser=parse_analysis)]
    analysis: Option<lab::core::rhythm::Completeness>,
    /// Special state: none, multiple, outside-bands or needs-review.
    #[arg(long,value_parser=parse_choice::<lab::core::query::RhythmState>)]
    rhythms: Option<auditorylab_app::core::query::RhythmState>,
    /// Inclusive explicit-offset creation instant.
    #[arg(long)]
    created_from: Option<lab::core::Timestamp>,
    /// Exclusive explicit-offset creation instant.
    #[arg(long)]
    created_before: Option<lab::core::Timestamp>,
    /// Inclusive explicit-offset edit instant.
    #[arg(long)]
    modified_from: Option<lab::core::Timestamp>,
    /// Exclusive explicit-offset edit instant.
    #[arg(long)]
    modified_before: Option<lab::core::Timestamp>,
    /// Inclusive explicit-offset Added here instant.
    #[arg(long)]
    added_from: Option<lab::core::Timestamp>,
    /// Exclusive explicit-offset Added here instant.
    #[arg(long)]
    added_before: Option<lab::core::Timestamp>,
    /// Explicit primary sort (default follows scope only while automatic).
    #[arg(long,value_parser=parse_sort)]
    sort: Option<SortBy>,
    /// Primary direction; nulls always last, tie title/key always ascending.
    #[arg(long,value_parser=parse_choice::<Direction>)]
    direction: Option<Direction>,
    /// Virtual groups reference unique flat keys.
    #[arg(long, default_value = "none",value_parser=parse_choice::<GroupBy>)]
    group_by: GroupBy,
    /// Machine output has no headings/progress; IDs require complete discovery.
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
    /// Persistent collection; temporary --file inputs require all.
    #[arg(long, value_enum, default_value = "all")]
    collection: CollectionArg,
    /// Explicit temporary recipe inputs, not added to User Library.
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
    /// Report configured root/format without creating anything.
    Path {
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Initialize an empty/owned interrupted configured root; marker is published last.
    Init {
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Add one external recipe; preserve its original file and declared history.
    Add {
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        #[arg(long)]
        as_copy: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Update an existing User recipe, optionally checking its observed byte token.
    Update {
        selector: LibraryKey,
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        #[arg(long)]
        expect: Option<VersionToken>,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Rename a User title; identity, Added here and audio stay unchanged.
    Rename {
        selector: LibraryKey,
        #[arg(long)]
        title: String,
        #[arg(long)]
        expect: Option<VersionToken>,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Move a selected User record to recoverable Trash.
    Trash {
        selector: LibraryKey,
        #[arg(long)]
        expect: Option<VersionToken>,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Restore a selected trashed User record.
    Restore {
        selector: LibraryKey,
        #[arg(long)]
        expect: Option<VersionToken>,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Permanently delete one trashed User record, never external originals/ancestors.
    Purge {
        selector: LibraryKey,
        #[arg(long)]
        expect: Option<VersionToken>,
        #[arg(long)]
        yes: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Query only trashed managed records; no automatic purge.
    ListTrash(Box<QueryArgs>),
    /// Diagnose records/stages without modifying or repairing them.
    Check {
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
    pub(super) fn operation(&self) -> &'static str {
        match self {
            Self::Path { .. } => "library_path",
            Self::Init { .. } => "library_init",
            Self::Add { .. } => "library_add",
            Self::Update { .. } => "library_update",
            Self::Rename { .. } => "library_rename",
            Self::Trash { .. } => "library_trash",
            Self::Restore { .. } => "library_restore",
            Self::Purge { .. } => "library_purge",
            Self::ListTrash(_) => "list_trash",
            Self::Check { .. } => "library_check",
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
    // Automatic applies when --sort is omitted.
    const EXPLICIT: [SortBy; 7] = [
        SortBy::Rate,
        SortBy::Title,
        SortBy::Study,
        SortBy::Collection,
        SortBy::Created,
        SortBy::Modified,
        SortBy::Added,
    ];
    EXPLICIT
        .into_iter()
        .find(|value| value.key() == text)
        .ok_or_else(|| expected_choices(EXPLICIT.iter().map(|value| value.key())))
}
fn parse_role(text: &str) -> std::result::Result<lab::core::ConditionRole, String> {
    parse_choice(text)
}
fn parse_population(text: &str) -> std::result::Result<lab::core::Population, String> {
    parse_choice(text)
}
fn parse_analysis(text: &str) -> std::result::Result<lab::core::rhythm::Completeness, String> {
    parse_choice(text)
}
pub(super) fn open_library(path: Option<&Path>) -> Result<Library> {
    Ok(Library::resolve(path)?)
}
use lab::services::Resolved;
pub(super) fn resolve(source: Source, path: Option<&Path>) -> Result<Resolved> {
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
            path,
        )?);
    };
    Ok(lab::services::resolve(request, path)?)
}
pub(super) fn list(args: ListArgs, path: Option<&Path>) -> Result<u8> {
    let (query, format) = args.query.query(args.collection.scope())?;
    if format == ListFormat::Ids && !args.files.is_empty() {
        return Err(usage(CommandProblem::TemporaryIds.message()));
    }
    let listing = lab::services::list(
        lab::services::ListRequest {
            query,
            files: args.files,
        },
        path,
        &Cancel::default(),
    )?;
    print_list("list", &listing, format)?;
    Ok(listing.outcome().map_or(0, super::exit_code))
}
fn print_list(operation: &str, listing: &lab::services::Listing, format: ListFormat) -> Result<()> {
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
                    eprintln!("{}", prose(&diagnostic.message()));
                }
            }
        }
        ListFormat::Json => print_json(&Report::listing(operation, listing))?,
        ListFormat::Text => {
            for entry in listing.entries() {
                output(entry.list_description().as_bytes())?;
            }
            if !listing.complete() {
                eprintln!("{}", lab::presentation::vocabulary::INVENTORY_INCOMPLETE);
            }
            for diagnostic in listing.diagnostics() {
                eprintln!(
                    "{}: {}",
                    diagnostic.storage_id.display_scalar(),
                    prose(&diagnostic.message())
                );
            }
        }
    }
    Ok(())
}
pub(super) fn describe(resolved: &Resolved, format: ReadFormat) -> Result<()> {
    let description = lab::services::describe(resolved)?;
    if format == ReadFormat::Json {
        print_json(&Report::describe(description))?;
    } else {
        let extra = resolved.summary().membership_description();
        output(format!("{}{}\n", prose(&description.description()), prose(&extra)).as_bytes())?;
    }
    Ok(())
}
pub(super) fn inspect(resolved: &Resolved, rate: SampleRate, format: ReadFormat) -> Result<u8> {
    let inspection = lab::services::inspect(resolved, rate);
    if format == ReadFormat::Text {
        let sound = inspection.sound().map_err(Clone::clone)?;
        output(format!("{}\n", prose(&sound.inspection())).as_bytes())?;
    } else {
        print_json(&Report::inspect(&inspection))?;
    }
    Ok(inspection.outcome().map_or(0, super::exit_code))
}
pub(super) fn library(command: LibraryCommand, path: Option<&Path>) -> Result<u8> {
    let operation = command.operation();
    match command {
        LibraryCommand::Path { format } => {
            let inspection = lab::services::library::path(path)?;
            let root = inspection.location;
            let info = match inspection.result {
                Ok(info) => info,
                Err(error) => {
                    let exists = inspection.exists;
                    if format == ReadFormat::Json {
                        print_json(&Report::unavailable_path(operation, &root, exists, &error))?;
                    } else {
                        output(root.inspection_failure(exists, &error).as_bytes())?;
                    }
                    return Ok(
                        if matches!(
                            error.outcome(),
                            lab::error::Outcome::Busy | lab::error::Outcome::LockUnavailable
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
                output(info.description().as_bytes())?;
            }
            Ok(0)
        }
        LibraryCommand::Init { format } => {
            let initialized = open_library(path)?.initialize()?;
            if format == ReadFormat::Json {
                print_mutation_json(&Report::initialized(operation, &initialized))?;
            } else {
                output(initialized.description().as_bytes())?;
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
                path,
                &Cancel::default(),
            )?;
            print_list(operation, &listing, format)?;
            Ok(listing.outcome().map_or(0, super::exit_code))
        }
        LibraryCommand::Check { format } => {
            let inventory = open_library(path)?.scan(&Cancel::default());
            if format == ReadFormat::Json {
                print_json(&Report::inventory(operation, &inventory))?;
            } else {
                output(inventory.description().as_bytes())?;
                for diagnostic in &inventory.diagnostics {
                    eprintln!(
                        "{}: {}",
                        diagnostic.storage_id.display_scalar(),
                        prose(&diagnostic.message())
                    );
                }
                for stage in &inventory.stages {
                    output(lab::presentation::retained_stage(stage).as_bytes())?;
                }
            }
            Ok(inventory.failure_kind.map_or(0, super::exit_code))
        }
        command => {
            if matches!(&command, LibraryCommand::Purge { yes: false, .. }) {
                return Err(usage(CommandProblem::PurgeConfirmation.message()));
            }
            let cancel = Cancel::default();
            let stop = cancel.clone();
            super::on_interrupt(move || stop.request())?;
            let library = open_library(path)?;
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
    operation: &str,
    format: ReadFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let mut bytes = if format == ReadFormat::Json {
        json_bytes(&Report::mutation(operation, outcome))?
    } else {
        outcome.description().display_text().into_bytes()
    };
    bytes.push(b'\n');
    if let Err(error) = stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
        let _ = writeln!(
            stderr,
            "{}",
            outcome.completion_failure(error).display_text()
        );
        let _ = stderr.flush();
    }
    Ok(0)
}
fn json_bytes(value: &Report) -> Result<Vec<u8>> {
    let text = serde_json::to_string(value)?;
    let mut output = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{2028}'|'\u{2029}')
        {
            use std::fmt::Write;
            write!(&mut output, "\\u{:04x}", u32::from(c))?;
        } else {
            output.push(c);
        }
    }
    Ok(output.into_bytes())
}
pub(super) fn print_json(value: &Report) -> Result<()> {
    let bytes = json_bytes(value)?;
    output(&bytes)?;
    output(b"\n")?;
    Ok(())
}
fn print_mutation_json(value: &Report) -> Result<()> {
    let mut bytes = match json_bytes(value) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{}", value.completion_failure(error).display_text());
            return Ok(());
        }
    };
    bytes.push(b'\n');
    if let Err(error) = io::stdout()
        .lock()
        .write_all(&bytes)
        .and_then(|()| io::stdout().lock().flush())
    {
        eprintln!("{}", value.completion_failure(error).display_text());
    }
    Ok(())
}
pub(super) fn error_json(operation: &str, error: &super::CliError) -> Result<()> {
    print_json(&Report::error(operation, error.message(), error.outcome()))
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
                "outside-bands",
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
                    state: Some(auditorylab_app::core::query::RhythmState::OutsideBands),
                },
                studies: vec!["key:source".into()],
                roles: vec![lab::core::ConditionRole::Comparison],
                populations: vec![lab::core::Population::Mouse],
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

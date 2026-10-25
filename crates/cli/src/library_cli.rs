//! Ordinary CLI discovery, selectors, structured output and managed lifecycle grammar.
use super::{Source, output, prose, usage};
use auditorylab::core::SampleRate;
use auditorylab::presentation::DisplayText;
use auditorylab::recipe::RecipeSnapshot;
use auditorylab::reports::Report;
use auditorylab::vocabulary::{Caption, CommandProblem, Status};
use auditorylab::workflow::Cancel;
use auditorylab::{
    self as lab,
    collection::{CatalogQuery, DateRange, Direction, EntrySummary, GroupBy, Scope, SortBy},
    library::{LibraryKey, LibraryRoot, LibraryStore, Mutation, VersionToken},
};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::Value;
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
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
    #[value(name = Scope::All.cli_key(), help = Scope::All.label())]
    All,
    #[value(name = Scope::BuiltIn.cli_key(), help = Scope::BuiltIn.label())]
    Builtin,
    #[value(name = Scope::User.cli_key(), help = Scope::User.label())]
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
    #[arg(long)]
    band: Vec<lab::core::nominal::Band>,
    /// Structural methods; band/method must match one descriptor.
    #[arg(long)]
    method: Vec<lab::core::nominal::Method>,
    /// Current-condition DOI or source key, never ancestor-only evidence.
    #[arg(long)]
    study: Vec<String>,
    /// Condition role correlated within the same evidence record.
    #[arg(long,value_parser=parse_role)]
    role: Vec<lab::core::ConditionRole>,
    /// Population correlated within the same evidence record.
    #[arg(long,value_parser=parse_population)]
    population: Vec<lab::core::Population>,
    /// Nominal analysis coverage: complete, partial or invalid.
    #[arg(long,value_parser=parse_analysis)]
    analysis: Option<lab::core::nominal::Completeness>,
    /// Special state: none, multiple, outside-bands or needs-review.
    #[arg(long)]
    rhythms: Option<lab::query::RhythmState>,
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
    #[arg(long)]
    sort: Option<SortBy>,
    /// Primary direction; nulls always last, tie title/key always ascending.
    #[arg(long)]
    direction: Option<Direction>,
    /// Virtual groups reference unique flat keys.
    #[arg(long, default_value = "none")]
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
            rhythm: lab::query::RhythmQuery {
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
    query: QueryArgs,
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
    /// Update one User record with the exact fresh version token from describe/list JSON.
    Update {
        selector: LibraryKey,
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        #[arg(long)]
        expect: VersionToken,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Rename a User title; identity, Added here and audio stay unchanged.
    Rename {
        selector: LibraryKey,
        #[arg(long)]
        title: String,
        #[arg(long)]
        expect: VersionToken,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Move a selected User record to recoverable Trash.
    Trash {
        selector: LibraryKey,
        #[arg(long)]
        expect: VersionToken,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Restore a selected trashed User record.
    Restore {
        selector: LibraryKey,
        #[arg(long)]
        expect: VersionToken,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Permanently delete one trashed User record, never external originals/ancestors.
    Purge {
        selector: LibraryKey,
        #[arg(long)]
        expect: VersionToken,
        #[arg(long)]
        yes: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: ReadFormat,
    },
    /// Query only trashed managed records; no automatic purge.
    ListTrash(QueryArgs),
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
fn parse_role(text: &str) -> std::result::Result<lab::core::ConditionRole, String> {
    serde_json::from_value(Value::String(text.into())).map_err(|e| e.to_string())
}
fn parse_population(text: &str) -> std::result::Result<lab::core::Population, String> {
    serde_json::from_value(Value::String(text.into())).map_err(|e| e.to_string())
}
fn parse_analysis(text: &str) -> std::result::Result<lab::core::nominal::Completeness, String> {
    match text {
        "complete" => Ok(lab::core::nominal::Completeness::Complete),
        "partial" => Ok(lab::core::nominal::Completeness::Partial),
        "invalid" => Ok(lab::core::nominal::Completeness::Invalid),
        _ => Err(CommandProblem::AnalysisCoverage.message().into()),
    }
}
pub(super) fn store(path: Option<&Path>) -> Result<LibraryStore> {
    Ok(LibraryStore::new(LibraryRoot::resolve(path)?)?)
}
/// Source resolution captures immutable saved content before any export/device work.
pub(super) struct Resolved {
    pub(super) snapshot: RecipeSnapshot,
    pub(super) summary: EntrySummary,
}
pub(super) fn resolve(source: Source, path: Option<&Path>) -> Result<Resolved> {
    if let Some(selector) = source.selector {
        let key: LibraryKey = selector.parse()?;
        match &key {
            LibraryKey::BuiltIn(id) => {
                let snapshot = lab::recipe::builtin(id)?;
                let summary = EntrySummary::from_snapshot(key, &snapshot, None);
                Ok(Resolved { snapshot, summary })
            }
            LibraryKey::User(_) => {
                let record = store(path)?.load(&key)?;
                let summary = record.summary();
                Ok(Resolved {
                    snapshot: record.snapshot,
                    summary,
                })
            }
            LibraryKey::External(_) => Err(usage(CommandProblem::TemporarySelector.message())),
        }
    } else {
        let snapshot = if let Some(file) = source.file {
            lab::recipe::load_file(&file)?
        } else {
            if io::stdin().is_terminal() {
                eprintln!("{}", Caption::PasteRecipe.label());
            }
            lab::recipe::load_reader(io::stdin().lock(), Caption::StandardInput.label())?
        };
        let key = LibraryKey::External(lab::library::ExternalId::allocate()?);
        let summary = EntrySummary::from_snapshot(key, &snapshot, None);
        Ok(Resolved { snapshot, summary })
    }
}
fn builtins() -> Result<Vec<EntrySummary>> {
    lab::recipe::catalog()?
        .into_iter()
        .map(|snapshot| {
            let key = match snapshot.origin() {
                lab::recipe::Origin::BuiltIn(id) => LibraryKey::BuiltIn((*id).into()),
                _ => return Err(usage(CommandProblem::CatalogOrigin.message())),
            };
            Ok(EntrySummary::from_snapshot(key, &snapshot, None))
        })
        .collect()
}
pub(super) fn list(args: ListArgs, path: Option<&Path>) -> Result<u8> {
    let scope = args.collection.scope();
    if !args.files.is_empty() && scope != Scope::All {
        return Err(usage(CommandProblem::RestrictedTemporaryInput.message()));
    }
    let (mut query, format) = args.query.query(scope)?;
    if format == ListFormat::Ids && !args.files.is_empty() {
        return Err(usage(CommandProblem::TemporaryIds.message()));
    }
    let mut entries = if scope == Scope::User {
        Vec::new()
    } else {
        builtins()?
    };
    let mut complete = true;
    let mut diagnostics = Vec::new();
    let mut stages = Vec::new();
    let mut exit = 0;
    if scope != Scope::BuiltIn {
        let inventory = match store(path) {
            Ok(store) => store.scan(&Cancel::default()),
            Err(error) => {
                if error
                    .downcast_ref::<lab::error::Error>()
                    .is_some_and(|error| {
                        matches!(
                            error.kind(),
                            lab::error::ErrorKind::InvalidArgument
                                | lab::error::ErrorKind::InvalidRecipe
                        )
                    })
                {
                    return Err(error);
                }
                lab::library::Inventory {
                    complete: false,
                    failure_kind: Some(lab::error::ErrorKind::IncompleteInventory),
                    diagnostics: vec![lab::library::LibraryDiagnostic {
                        storage_id: Caption::ConfiguredRoot.label().into(),
                        code: "root_resolution".into(),
                        message: error.to_string(),
                    }],
                    ..lab::library::Inventory::default()
                }
            }
        };
        complete = inventory.complete;
        exit = inventory
            .failure_kind
            .map_or(0, lab::error::ErrorKind::exit_code);
        entries.extend(inventory.entries);
        diagnostics = inventory.diagnostics;
        stages = inventory.stages;
    }
    query.include_external = !args.files.is_empty();
    for path in args.files {
        let snapshot = lab::recipe::load_file(&path)?;
        entries.push(EntrySummary::from_snapshot(
            LibraryKey::External(lab::library::ExternalId::allocate()?),
            &snapshot,
            None,
        ));
    }
    print_list(
        "list",
        &query,
        &entries,
        complete,
        &diagnostics,
        &stages,
        format,
    )?;
    Ok(exit)
}
fn print_list(
    operation: &str,
    query: &CatalogQuery,
    entries: &[EntrySummary],
    complete: bool,
    diagnostics: &[lab::library::LibraryDiagnostic],
    stages: &[String],
    format: ListFormat,
) -> Result<()> {
    let result = query.execute(entries)?;
    match format {
        ListFormat::Ids => {
            if complete {
                let ids = result
                    .entries
                    .iter()
                    .map(|e| e.key.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !ids.is_empty() {
                    output(format!("{ids}\n").as_bytes())?;
                }
            } else {
                for diagnostic in diagnostics {
                    eprintln!("{}", prose(&diagnostic.message));
                }
            }
        }
        ListFormat::Json => {
            let value = Report::listing(operation, query, &result, complete, diagnostics, stages);
            print_json(&value)?;
        }
        ListFormat::Text => {
            for entry in &result.entries {
                output(entry.list_description().as_bytes())?;
            }
            if !complete {
                eprintln!("{}", Status::InventoryIncomplete.label());
            }
            for diagnostic in diagnostics {
                eprintln!(
                    "{}: {}",
                    diagnostic.storage_id.display_scalar(),
                    prose(&diagnostic.message)
                );
            }
        }
    }
    Ok(())
}
pub(super) fn describe(resolved: &Resolved, format: ReadFormat) -> Result<()> {
    if format == ReadFormat::Json {
        print_json(&Report::describe(&resolved.summary, &resolved.snapshot)?)?;
    } else {
        let extra = resolved.summary.membership_description();
        output(
            format!(
                "{}{}\n",
                prose(&resolved.snapshot.description()),
                prose(&extra)
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn inspect(resolved: &Resolved, rate: SampleRate, format: ReadFormat) -> Result<u8> {
    if format == ReadFormat::Text {
        let sound = resolved.snapshot.at(rate);
        output(format!("{}\n", prose(&sound?.inspection())).as_bytes())?;
        return Ok(0);
    }
    let (report, code) = Report::inspect(&resolved.summary, &resolved.snapshot, rate);
    print_json(&report)?;
    Ok(code)
}
pub(super) fn library(command: LibraryCommand, path: Option<&Path>) -> Result<u8> {
    let operation = command.operation();
    match command {
        LibraryCommand::Path { format } => {
            let root = LibraryRoot::resolve(path)?;
            let info = match root.inspect() {
                Ok(info) => info,
                Err(error) => {
                    let exists = root.path().try_exists().ok();
                    if format == ReadFormat::Json {
                        print_json(&Report::unavailable_path(operation, &root, exists, &error))?;
                    } else {
                        output(root.inspection_failure(exists, &error).as_bytes())?;
                    }
                    return Ok(
                        if matches!(
                            error.kind(),
                            lab::error::ErrorKind::Busy | lab::error::ErrorKind::LockUnavailable
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
            let initialized = store(path)?.root().initialize()?;
            if format == ReadFormat::Json {
                print_mutation_json(&Report::initialized(operation, &initialized))?;
            } else {
                output(initialized.description().as_bytes())?;
            }
            Ok(0)
        }
        LibraryCommand::ListTrash(args) => {
            let (query, format) = args.query(Scope::Trash)?;
            let inventory = store(path)?.scan(&Cancel::default());
            print_list(
                operation,
                &query,
                &inventory.entries,
                inventory.complete,
                &inventory.diagnostics,
                &inventory.stages,
                format,
            )?;
            Ok(inventory
                .failure_kind
                .map_or(0, lab::error::ErrorKind::exit_code))
        }
        LibraryCommand::Check { format } => {
            let inventory = store(path)?.scan(&Cancel::default());
            if format == ReadFormat::Json {
                print_json(&Report::inventory(operation, &inventory))?;
            } else {
                output(inventory.description().as_bytes())?;
                for diagnostic in &inventory.diagnostics {
                    eprintln!(
                        "{}: {}",
                        diagnostic.storage_id.display_scalar(),
                        prose(&diagnostic.message)
                    );
                }
                for stage in &inventory.stages {
                    output(Caption::retained_stage(stage).as_bytes())?;
                }
            }
            Ok(inventory
                .failure_kind
                .map_or(0, lab::error::ErrorKind::exit_code))
        }
        command => {
            let (mutation, format) = match command {
                LibraryCommand::Add {
                    file,
                    as_copy,
                    format,
                } => {
                    let snapshot = lab::recipe::load_file(&file)?;
                    let recipe = if as_copy {
                        let mut copy = lab::editing::Draft::from_snapshot(&snapshot)?;
                        copy.set_title(snapshot.title());
                        copy.recipe()
                    } else {
                        snapshot.recipe().clone()
                    };
                    (
                        if as_copy {
                            Mutation::Create(Arc::new(recipe))
                        } else {
                            Mutation::Add(Arc::new(recipe))
                        },
                        format,
                    )
                }
                LibraryCommand::Update {
                    selector,
                    file,
                    expect,
                    format,
                } => {
                    selector.require_user()?;
                    (
                        Mutation::Update {
                            key: selector,
                            expect,
                            recipe: Arc::new(lab::recipe::load_file(&file)?.recipe().clone()),
                        },
                        format,
                    )
                }
                LibraryCommand::Rename {
                    selector,
                    title,
                    expect,
                    format,
                } => (
                    Mutation::Rename {
                        key: selector,
                        expect,
                        title,
                    },
                    format,
                ),
                LibraryCommand::Trash {
                    selector,
                    expect,
                    format,
                } => (
                    Mutation::Trash {
                        key: selector,
                        expect,
                    },
                    format,
                ),
                LibraryCommand::Restore {
                    selector,
                    expect,
                    format,
                } => (
                    Mutation::Restore {
                        key: selector,
                        expect,
                    },
                    format,
                ),
                LibraryCommand::Purge {
                    selector,
                    expect,
                    yes,
                    format,
                } => {
                    if !yes {
                        return Err(usage(CommandProblem::PurgeConfirmation.message()));
                    }
                    (
                        Mutation::Purge {
                            key: selector,
                            expect,
                            confirmed: yes,
                        },
                        format,
                    )
                }
                _ => unreachable!("read-only library commands handled above"),
            };
            let cancel = Cancel::default();
            let stop = cancel.clone();
            super::on_interrupt(move || stop.request())?;
            apply_and_report_mutation(
                &store(path)?,
                mutation,
                operation,
                format,
                &cancel,
                &mut io::stdout().lock(),
                &mut io::stderr().lock(),
            )
        }
    }
}
// The command and native fault tests share the complete apply/report boundary.
// Publication truth is established before any fallible terminal output.
fn apply_and_report_mutation(
    store: &LibraryStore,
    mutation: Mutation,
    operation: &str,
    format: ReadFormat,
    cancel: &Cancel,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let outcome = store.apply(mutation, cancel)?;
    let code = if outcome.confirmed() { 0 } else { 1 };
    let mut bytes = if format == ReadFormat::Json {
        json_bytes(&Report::mutation(operation, &outcome))?
    } else {
        outcome.description(store.root().path()).into_bytes()
    };
    bytes.push(b'\n');
    if let Err(error) = stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
        // Publication truth is established before this notification can fail.
        let _ = writeln!(
            stderr,
            "{}",
            outcome.reporting_failure(store.root().path(), &error)
        );
        let _ = stderr.flush();
    }
    Ok(code)
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
    let mut bytes = json_bytes(value)?;
    bytes.push(b'\n');
    if let Err(error) = io::stdout()
        .lock()
        .write_all(&bytes)
        .and_then(|()| io::stdout().lock().flush())
    {
        eprintln!("{}", value.reporting_failure(&error));
    }
    Ok(())
}
pub(super) fn error_json(operation: &str, error: &dyn std::error::Error, code: u8) -> Result<()> {
    print_json(&Report::error(operation, error, code))
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use clap::Parser;
    use lab::library::StoreFault;

    struct InterruptedOutput;
    impl Write for InterruptedOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "closed completion pipe",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn default_text_uncertainty_receipts_compare_real_committed_and_uncommitted_destinations() {
        for committed in [false, true] {
            for broken_output in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let store =
                    LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap())
                        .unwrap();
                store.root().initialize().unwrap();
                let original = store
                    .apply(
                        Mutation::Create(Arc::new(lab::editing::Draft::new().unwrap().recipe())),
                        &Cancel::default(),
                    )
                    .unwrap()
                    .receipt
                    .record
                    .unwrap();
                let key = original.key.to_string();
                let previous = original.version_token.to_string();
                // Parse the ordinary command without --format. The same apply/
                // report function is called by the production dispatch above.
                let parsed = super::super::Cli::try_parse_from([
                    "auditorylab",
                    "library",
                    "rename",
                    &key,
                    "--title",
                    "Recovery candidate",
                    "--expect",
                    &previous,
                ])
                .unwrap();
                let Some(super::super::Command::Library {
                    command:
                        LibraryCommand::Rename {
                            selector,
                            expect,
                            title,
                            format,
                        },
                }) = parsed.command
                else {
                    panic!("rename command not parsed")
                };
                assert_eq!(format, ReadFormat::Text);
                let mutation = Mutation::Rename {
                    key: selector,
                    expect,
                    title,
                };
                let fault = if committed {
                    StoreFault::PublicationCommittedUnreadable
                } else {
                    StoreFault::PublicationUncommittedUnreadable
                };
                let failing = store.clone().with_fault(Some(fault));
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                let code = if broken_output {
                    apply_and_report_mutation(
                        &failing,
                        mutation,
                        "library_rename",
                        format,
                        &Cancel::default(),
                        &mut InterruptedOutput,
                        &mut stderr,
                    )
                } else {
                    apply_and_report_mutation(
                        &failing,
                        mutation,
                        "library_rename",
                        format,
                        &Cancel::default(),
                        &mut stdout,
                        &mut stderr,
                    )
                }
                .unwrap();
                assert_eq!(code, 1);
                let report = String::from_utf8(if broken_output {
                    stderr
                } else {
                    assert!(stderr.is_empty());
                    stdout
                })
                .unwrap();
                assert!(report.contains("Publication not confirmed"));
                assert!(!report.contains("PublicationUnconfirmed"));
                assert!(report.contains("Do not blindly retry"));
                assert!(report.contains("Absence alone does not attribute a purge"));
                assert!(
                    report.contains(&format!("Library root: {}", store.root().path().display()))
                );
                assert!(report.contains("Owned stage: "));
                let field = |label: &str| {
                    report
                        .lines()
                        .find_map(|line| line.strip_prefix(label))
                        .unwrap()
                        .to_owned()
                };
                let candidate = field("Candidate version token: ");
                let printed_previous = field("Previous version token: ");
                assert_eq!(printed_previous, previous);
                let _: VersionToken = candidate.parse().unwrap();
                let _: lab::library::RecordRevision =
                    field("Operation revision: ").parse().unwrap();
                assert_ne!(candidate, previous);
                let revision = field("Operation revision: ");
                let stage = field("Owned stage: ");
                assert_eq!(
                    stage,
                    format!(
                        ".{}.{revision}.partial",
                        original.snapshot.recipe().document.id
                    )
                );
                assert_eq!(
                    store.root().path().join("records").join(&stage).exists(),
                    !committed
                );
                let observed = store.load(&original.key).unwrap();
                assert_eq!(
                    observed.version_token.to_string(),
                    if committed {
                        candidate
                    } else {
                        printed_previous
                    }
                );
                assert_eq!(observed.snapshot.title() == "Recovery candidate", committed);
                if broken_output {
                    assert!(report.contains("Completion output failed"));
                    assert_eq!(report.matches("Recovery receipt").count(), 1);
                }
            }
        }
    }
    #[test]
    fn ordinary_confirmed_text_stays_concise_but_failed_reporting_retains_comparison_values() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
        store.root().initialize().unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            apply_and_report_mutation(
                &store,
                Mutation::Create(Arc::new(lab::editing::Draft::new().unwrap().recipe())),
                "library_add",
                ReadFormat::Text,
                &Cancel::default(),
                &mut out,
                &mut err
            )
            .unwrap(),
            0
        );
        assert!(err.is_empty());
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("Saved: user:"));
        assert_eq!(text.lines().count(), 1);
        let recipe = Arc::new(lab::editing::Draft::new().unwrap().recipe());
        let key = LibraryKey::User(recipe.document.id);
        let mut err = Vec::new();
        assert_eq!(
            apply_and_report_mutation(
                &store,
                Mutation::Create(recipe),
                "library_add",
                ReadFormat::Text,
                &Cancel::default(),
                &mut InterruptedOutput,
                &mut err
            )
            .unwrap(),
            0
        );
        let text = String::from_utf8(err).unwrap();
        let observed = store.load(&key).unwrap();
        assert!(text.contains(&format!(
            "Candidate version token: {}",
            observed.version_token
        )));
        assert!(text.contains("Previous version token: None (no previous record)"));
        assert!(text.contains("Completion output failed"));
    }
    #[test]
    fn uncertain_text_purge_retains_previous_token_without_attributing_absence() {
        for committed in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let store =
                LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap())
                    .unwrap();
            store.root().initialize().unwrap();
            let original = store
                .apply(
                    Mutation::Create(Arc::new(lab::editing::Draft::new().unwrap().recipe())),
                    &Cancel::default(),
                )
                .unwrap()
                .receipt
                .record
                .unwrap();
            let trashed = store
                .apply(
                    Mutation::Trash {
                        key: original.key.clone(),
                        expect: original.version_token,
                    },
                    &Cancel::default(),
                )
                .unwrap()
                .receipt
                .record
                .unwrap();
            let previous = trashed.version_token.to_string();
            let failing = store.clone().with_fault(Some(if committed {
                StoreFault::PurgeCommittedUnreadable
            } else {
                StoreFault::PurgeUncommittedUnreadable
            }));
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                apply_and_report_mutation(
                    &failing,
                    Mutation::Purge {
                        key: trashed.key.clone(),
                        expect: trashed.version_token,
                        confirmed: true
                    },
                    "library_purge",
                    ReadFormat::Text,
                    &Cancel::default(),
                    &mut out,
                    &mut err
                )
                .unwrap(),
                1
            );
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains(&format!("Previous version token: {previous}")));
            assert!(text.contains("Candidate version token: None (purge has no candidate record)"));
            assert!(text.contains("Absence alone does not attribute a purge"));
            assert!(err.is_empty());
            if committed {
                assert!(store.load(&trashed.key).is_err());
            } else {
                assert_eq!(
                    store.load(&trashed.key).unwrap().version_token.to_string(),
                    previous
                );
            }
        }
    }
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
                "partial",
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
                rhythm: lab::query::RhythmQuery {
                    bands: vec![lab::core::nominal::Band::Theta],
                    methods: vec![lab::core::nominal::Method::Monaural],
                    state: Some(lab::query::RhythmState::OutsideBands),
                },
                studies: vec!["key:source".into()],
                roles: vec![lab::core::ConditionRole::Comparison],
                populations: vec![lab::core::Population::Mouse],
                analysis: Some(lab::core::nominal::Completeness::Partial),
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

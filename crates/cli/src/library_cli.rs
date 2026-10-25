//! Ordinary CLI discovery, selectors, structured output and managed lifecycle grammar.
use super::{Source, escape, output, prose, usage};
use auditorylab::core::SampleRate;
use auditorylab::recipe::RecipeSnapshot;
use auditorylab::workflow::Cancel;
use auditorylab::{
    self as lab,
    collection::{CatalogQuery, DateRange, Direction, EntrySummary, GroupBy, Scope, SortBy},
    library::{
        LibraryKey, LibraryRoot, LibraryStore, Mutation, MutationKind, VersionToken, WriteOutcome,
    },
};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};
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
    All,
    Builtin,
    User,
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
            return Err(usage("--format ids cannot be combined with grouping"));
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
        _ => Err("expected complete, partial or invalid".into()),
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
            LibraryKey::External(_) => Err(usage(
                "temporary selectors are not reusable; use --file or --stdin",
            )),
        }
    } else {
        let snapshot = if let Some(file) = source.file {
            lab::recipe::load_file(&file)?
        } else {
            if io::stdin().is_terminal() {
                eprintln!(
                    "Paste a UTF-8 TOML recipe, then end input with Ctrl-D (Unix) or Ctrl-Z followed by Enter (Windows)."
                );
            }
            lab::recipe::load_reader(io::stdin().lock(), "standard input")?
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
                _ => return Err(usage("catalog returned a non-builtin origin")),
            };
            Ok(EntrySummary::from_snapshot(key, &snapshot, None))
        })
        .collect()
}
pub(super) fn list(args: ListArgs, path: Option<&Path>) -> Result<u8> {
    let scope = match args.collection {
        CollectionArg::All => Scope::All,
        CollectionArg::Builtin => Scope::BuiltIn,
        CollectionArg::User => Scope::User,
    };
    if !args.files.is_empty() && scope != Scope::All {
        return Err(usage(
            "temporary --file inputs cannot be combined with a restrictive --collection",
        ));
    }
    let (mut query, format) = args.query.query(scope)?;
    if format == ListFormat::Ids && !args.files.is_empty() {
        return Err(usage(
            "--format ids cannot emit temporary inputs; omit --file",
        ));
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
                        storage_id: "configured root".into(),
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
            let value = json!({"output_schema_version":1,"operation":operation,"complete":complete,"convention":lab::core::nominal::CONVENTION,"classifier_revision":lab::core::nominal::CLASSIFIER_REVISION,"query":query,"effective_sort":query.effective_sort(),"unique_count":result.unique_count(),"entries":result.entries.iter().map(|e|entry_json(e)).collect::<Vec<_>>(),"groups":result.groups,"diagnostics":diagnostics,"stages":stages});
            print_json(&value)?;
        }
        ListFormat::Text => {
            for entry in &result.entries {
                let dates = format!(
                    "Created {} · Edited {} · Added here {}",
                    date(entry.created_at),
                    date(entry.modified_at),
                    date(entry.added_at)
                );
                let text = format!(
                    "{}\n  {}\n  {} · {}\n  {}\n",
                    scalar(&entry.title),
                    entry.key,
                    lab::presentation::nominal_description(&entry.nominal),
                    lab::presentation::human_duration(entry.duration_seconds),
                    dates
                );
                output(text.as_bytes())?;
            }
            if !complete {
                eprintln!(
                    "Incomplete inventory: valid results above do not establish an empty/complete library."
                );
            }
            for diagnostic in diagnostics {
                eprintln!(
                    "{}: {}",
                    scalar(&diagnostic.storage_id),
                    prose(&diagnostic.message)
                );
            }
        }
    }
    Ok(())
}
pub(super) fn entry_json(entry: &EntrySummary) -> Value {
    json!({"key":entry.key,"collection":entry.collection(),"title":entry.title,"document_id":entry.document_id,"duration_seconds":entry.duration_seconds,"created_at":entry.created_at,"modified_at":entry.modified_at,"added_at":entry.added_at,"trashed_at":entry.trashed_at,"version_token":entry.version_token,"nominal":{"descriptors":entry.nominal.rhythms.iter().map(|r|json!({"owner":r.owner.get(),"related_owner":r.related.map(|id|id.get()),"method":r.method.key(),"hz":r.hz,"band":r.band,"derivation":match r.derivation {lab::core::nominal::Derivation::Explicit=>"explicit",lab::core::nominal::Derivation::NarrowInference=>"narrow_inference"}})).collect::<Vec<_>>(),"bands":entry.nominal.bands,"methods":entry.nominal.methods,"completeness":entry.nominal.completeness,"multiple_rhythms":entry.nominal.multiple(),"outside_bands":entry.nominal.outside_bands(),"no_rhythm":entry.nominal.no_rhythm(),"diagnostics":entry.nominal.diagnostics},"evidence":entry.evidence,"validation":{"status":"not_checked"}})
}
pub(super) fn describe(resolved: &Resolved, format: ReadFormat) -> Result<()> {
    if format == ReadFormat::Json {
        let mut value = entry_json(&resolved.summary);
        value["history_warnings"] =
            json!(lab::library::history_warnings(resolved.snapshot.recipe()));
        value["recipe_toml"] = Value::String(resolved.snapshot.recipe().to_toml()?);
        print_json(
            &json!({"output_schema_version":1,"operation":"describe","complete":true,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":1,"entries":[value],"groups":[],"diagnostics":[]}),
        )?;
    } else {
        let extra = if resolved.summary.added_at.is_some() {
            format!(
                "Added here: {}\nTrash: {}\nVersion token: {}\n",
                date(resolved.summary.added_at),
                date(resolved.summary.trashed_at),
                resolved
                    .summary
                    .version_token
                    .as_ref()
                    .map_or_else(|| "None".into(), ToString::to_string)
            )
        } else {
            String::new()
        };
        output(
            format!(
                "{}{}\n",
                prose(&lab::presentation::describe(&resolved.snapshot)),
                prose(&extra)
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn inspect(resolved: &Resolved, rate: SampleRate, format: ReadFormat) -> Result<u8> {
    let sound = resolved.snapshot.at(rate);
    if format == ReadFormat::Text {
        output(format!("{}\n", prose(&lab::presentation::inspection(&sound?))).as_bytes())?;
        return Ok(0);
    }
    let mut entry = entry_json(&resolved.summary);
    let mut code = 0;
    match sound {
        Ok(sound) => {
            entry["validation"] = json!({"status":"valid","sample_rate_hz":rate.hz(),"frames":sound.signal().frames(),"peak_dbfs":sound.signal().peak_dbfs(),"components":sound.signal().target().components().map(|(id,f)|json!({"owner":id.get(),"lane":format!("{:?}",id.lane()).to_lowercase(),"facts":f})).collect::<Vec<_>>()});
        }
        Err(error) => {
            code = error.kind().exit_code();
            entry["validation"] = json!({"status":"invalid","sample_rate_hz":rate.hz(),"diagnostics":[error.to_string()]});
        }
    }
    print_json(
        &json!({"output_schema_version":1,"operation":"inspect","complete":true,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":1,"entries":[entry],"groups":[],"diagnostics":[]}),
    )?;
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
                        print_json(
                            &json!({"output_schema_version":1,"operation":operation,"complete":false,"root":{"path":root.path(),"source":root.source(),"exists":exists,"library_format":null,"initialized":false},"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":0,"entries":[],"groups":[],"diagnostics":[{"code":"root_inspection","message":error.to_string()}]}),
                        )?;
                    } else {
                        output(
                            format!(
                                "{}\nExists: {:?} · Initialized: unverified · Source: {:?}\n{}\n",
                                root.path().display(),
                                exists,
                                root.source(),
                                error
                            )
                            .as_bytes(),
                        )?;
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
                print_json(
                    &json!({"output_schema_version":1,"operation":operation,"complete":true,"root":info,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":0,"entries":[],"groups":[],"diagnostics":[]}),
                )?;
            } else {
                output(
                    format!(
                        "{}\nExists: {} · Initialized: {} · Source: {:?}\n",
                        info.path.display(),
                        info.exists,
                        info.initialized,
                        info.source
                    )
                    .as_bytes(),
                )?;
            }
            Ok(0)
        }
        LibraryCommand::Init { format } => {
            let initialized = store(path)?.root().initialize()?;
            if format == ReadFormat::Json {
                print_mutation_json(
                    &json!({"output_schema_version":1,"operation":operation,"complete":true,"status":if initialized.created {"published"}else{"unchanged"},"root":initialized,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":0,"entries":[],"groups":[],"diagnostics":initialized.warnings}),
                    true,
                )?;
            } else {
                output(
                    format!(
                        "Library {}: {}\n",
                        if initialized.created {
                            "initialized"
                        } else {
                            "already initialized"
                        },
                        initialized.path.display()
                    )
                    .as_bytes(),
                )?;
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
                print_json(
                    &json!({"output_schema_version":1,"operation":operation,"complete":inventory.complete,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":inventory.entries.len(),"entries":inventory.entries.iter().map(entry_json).collect::<Vec<_>>(),"groups":[],"diagnostics":inventory.diagnostics,"stages":inventory.stages}),
                )?;
            } else {
                output(
                    format!(
                        "Library check: {} managed records · complete={}\n",
                        inventory.entries.len(),
                        inventory.complete
                    )
                    .as_bytes(),
                )?;
                for diagnostic in &inventory.diagnostics {
                    eprintln!(
                        "{}: {}",
                        scalar(&diagnostic.storage_id),
                        prose(&diagnostic.message)
                    );
                }
                for stage in &inventory.stages {
                    output(format!("Retained stage: {}\n", scalar(stage)).as_bytes())?;
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
                        return Err(usage(
                            "permanent deletion requires --yes and a fresh --expect token",
                        ));
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
        json_bytes(&outcome_json(operation, &outcome))?
    } else {
        mutation_text(&outcome, store.root().path()).into_bytes()
    };
    bytes.push(b'\n');
    if let Err(error) = stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
        // Include every comparison value even if stdout received only a prefix.
        // A lost completion pipe cannot turn a publication into NotSaved.
        let mut report = mutation_text(&outcome, store.root().path());
        if outcome.confirmed() {
            report.push('\n');
            report.push_str(&recovery_text(&outcome, store.root().path()));
        }
        let truth = if outcome.confirmed() {
            "Operation confirmed"
        } else {
            "Publication remains unconfirmed; Recheck before retry"
        };
        let _ = writeln!(
            stderr,
            "{truth}\n{report}\nCompletion output failed: {error}"
        );
        let _ = stderr.flush();
    }
    Ok(code)
}
fn mutation_text(outcome: &WriteOutcome, root: &Path) -> String {
    let mut text = format!("{:?}: {}", outcome.status, outcome.receipt.key);
    for diagnostic in &outcome.diagnostics {
        text.push('\n');
        text.push_str(&escape(diagnostic));
    }
    if !outcome.confirmed() {
        text.push('\n');
        text.push_str(&recovery_text(outcome, root));
    }
    text
}
fn recovery_text(outcome: &WriteOutcome, root: &Path) -> String {
    let receipt = &outcome.receipt;
    let previous = receipt
        .previous
        .as_ref()
        .map_or_else(|| "None (no previous record)".into(), ToString::to_string);
    let candidate = receipt.candidate_token.as_ref().map_or_else(
        || "None (purge has no candidate record)".into(),
        ToString::to_string,
    );
    let stage = receipt
        .owned_stage
        .as_ref()
        .map_or_else(|| "None".into(), |stage| scalar(stage));
    let root = scalar(&root.display().to_string());
    format!(
        "Recovery receipt\nLibrary root: {root}\nOperation revision: {}\nSelector: {}\nPrevious version token: {previous}\nCandidate version token: {candidate}\nOwned stage: {stage}\nRecheck in this same library root with: auditorylab describe {} --format json\nCompare the observed version_token with this candidate/previous token before deciding another operation. Do not blindly retry. Absence alone does not attribute a purge.",
        receipt.operation_revision, receipt.key, receipt.key
    )
}
fn outcome_json(operation: &str, outcome: &WriteOutcome) -> Value {
    let receipt = &outcome.receipt;
    let entry = receipt
        .record
        .as_ref()
        .map(|record| entry_json(&record.summary()));
    let entries = if outcome.confirmed() && receipt.kind != MutationKind::Purge {
        entry.clone().into_iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({"output_schema_version":1,"operation":operation,"complete":outcome.confirmed(),"status":outcome.status,"selector":receipt.key,"version_token":receipt.candidate_token,"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":entries.len(),"entries":entries,"groups":[],"candidate":if outcome.confirmed(){Value::Null}else{entry.unwrap_or(Value::Null)},"diagnostics":outcome.diagnostics,"receipt":{"operation_revision":receipt.operation_revision,"previous_version_token":receipt.previous,"candidate_version_token":receipt.candidate_token,"owned_stage":receipt.owned_stage}})
}
fn date(value: Option<lab::core::Timestamp>) -> String {
    value.map_or_else(|| "Unknown".into(), |date| date.to_string())
}
fn scalar(value: &str) -> String {
    escape(value).replace('\n', "\\n").replace('\t', "\\t")
}
fn json_bytes(value: &Value) -> Result<Vec<u8>> {
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
pub(super) fn print_json(value: &Value) -> Result<()> {
    let bytes = json_bytes(value)?;
    output(&bytes)?;
    output(b"\n")?;
    Ok(())
}
fn print_mutation_json(value: &Value, confirmed: bool) -> Result<()> {
    let mut bytes = json_bytes(value)?;
    bytes.push(b'\n');
    if let Err(error) = io::stdout()
        .lock()
        .write_all(&bytes)
        .and_then(|()| io::stdout().lock().flush())
    {
        eprintln!(
            "{}; completion output failed: {error}; receipt {}",
            if confirmed {
                "Operation confirmed"
            } else {
                "Publication remains unconfirmed; Recheck before retry"
            },
            value["receipt"]
        );
    }
    Ok(())
}
pub(super) fn error_json(operation: &str, error: &dyn std::error::Error, code: u8) -> Result<()> {
    print_json(
        &json!({"output_schema_version":1,"operation":operation,"complete":false,"status":match code {3=>"conflict",4=>"busy",5=>"incomplete_inventory",130=>"cancelled",2=>"rejected",_=>"failed_before_publication"},"convention":lab::core::nominal::CONVENTION,"query":null,"unique_count":0,"entries":[],"groups":[],"diagnostics":[error.to_string()]}),
    )
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
                assert!(report.contains("PublicationUnconfirmed"));
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
        assert!(text.starts_with("Published: user:"));
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

//! Managed-library operations shared by terminal and interactive jobs.
use super::Cancel;
use crate::document::origin::{LibraryKey, VersionToken};
pub use crate::storage::library::LibraryError;
pub use crate::storage::library::{
    DiagnosticDetail, Initialization, Inventory, LIBRARY_LAYOUT, LibraryDiagnostic, LoadedRecord,
    Mutation, MutationKind, MutationStatus, RootInfo, RootSource, WriteOutcome,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use {
    crate::core::Recipe, crate::document::editing::Draft, crate::document::recipe::RecipeSnapshot,
    crate::services::OperationError, crate::storage::library::LibraryRoot,
    crate::storage::library::LibraryStore,
};

/// One captured library configuration and its availability. Resolving a context
/// never initializes storage; every filesystem operation still checks its locks
/// and the current layout/content at the action boundary.
#[derive(Debug, Clone)]
pub struct LibraryContext {
    source: RootSource,
    location: Option<Location>,
    library: Result<Library, LibraryError>,
}
impl LibraryContext {
    /// Capture CLI override, environment or native default once, including failure.
    #[must_use]
    pub fn resolve(override_path: Option<&Path>) -> Self {
        let (source, selected) = LibraryRoot::select(override_path);
        let mut location = selected.as_ref().ok().map(|path| Location {
            path: path.clone(),
            source,
        });
        let library = selected
            .and_then(|path| LibraryRoot::configured(path, source))
            .and_then(Library::from_root);
        if let Ok(library) = &library {
            location = Some(Location {
                path: library.path().to_owned(),
                source,
            });
        }
        Self {
            source,
            location,
            library,
        }
    }
    /// Retain an already bound capability without resolving configuration again.
    #[must_use]
    pub fn from_library(library: Library) -> Self {
        let source = library.source();
        let location = Some(Location {
            path: library.path().to_owned(),
            source,
        });
        Self {
            source,
            location,
            library: Ok(library),
        }
    }
    /// Configuration precedence source, even when no native path is available.
    #[must_use]
    pub const fn source(&self) -> RootSource {
        self.source
    }
    /// Captured path/source, including an unusable explicit or environment path.
    #[must_use]
    pub const fn location(&self) -> Option<&Location> {
        self.location.as_ref()
    }
    /// Bound service capability, or the captured configuration failure.
    ///
    /// # Errors
    /// Returns the original configuration/catalog failure; never falls back.
    pub fn library(&self) -> Result<&Library, LibraryError> {
        self.library.as_ref().map_err(Clone::clone)
    }
    /// Captured configuration failure, distinct from later filesystem failures.
    #[must_use]
    pub fn problem(&self) -> Option<&LibraryError> {
        self.library.as_ref().err()
    }
    pub(crate) fn store(&self) -> Option<&Library> {
        self.library.as_ref().ok()
    }
}

/// An explicitly configured library service. Construction never initializes storage.
#[derive(Debug, Clone)]
pub struct Library {
    store: LibraryStore,
}
impl Library {
    /// Bind an absolute root without creating it or any recipe.
    ///
    /// # Errors
    /// Rejects relative paths and an invalid reserved catalog.
    pub fn at_path(path: PathBuf) -> Result<Self, LibraryError> {
        Self::from_root(LibraryRoot::explicit(path)?)
    }
    fn from_root(root: LibraryRoot) -> Result<Self, LibraryError> {
        Ok(Self {
            store: LibraryStore::new(root)?,
        })
    }
    /// Captured configured path, canonical when it existed at configuration time.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.store.root().path()
    }
    /// Configuration precedence source.
    #[must_use]
    pub fn source(&self) -> RootSource {
        self.store.root().source()
    }
    /// Observe layout and initialization without creating storage.
    ///
    /// # Errors
    /// Preserves locking, format and filesystem failures.
    pub fn inspect(&self) -> Result<RootInfo, LibraryError> {
        self.store.root().inspect()
    }
    /// Explicitly initialize this root, preserving existing content.
    ///
    /// # Errors
    /// Refuses unsupported or ambiguous existing state and unavailable locking.
    pub fn initialize(&self) -> Result<Initialization, LibraryError> {
        self.store.root().initialize()
    }
    /// Rebuild a bounded inventory; failure never establishes an empty library.
    #[must_use]
    pub fn scan(&self, cancel: &Cancel) -> Inventory {
        self.store.scan(cancel)
    }
    /// Capture a managed record under the adapter's read lock.
    ///
    /// # Errors
    /// Preserves identity, history and incomplete-read failures.
    pub fn load(&self, key: &LibraryKey) -> Result<LoadedRecord, LibraryError> {
        self.store.load(key)
    }
    /// Execute an immutable lifecycle request with checks against locked storage.
    ///
    /// # Errors
    /// Preserves cancellation, conflicts and truthful publication failures.
    pub fn apply(&self, mutation: Mutation, cancel: &Cancel) -> Result<WriteOutcome, LibraryError> {
        self.store.apply(mutation, cancel)
    }
    /// Add bounded external content, optionally as an independent identity with ancestry.
    ///
    /// # Errors
    /// Preserves input, identity, portable-contract and publication failures.
    pub fn add_file(
        &self,
        path: &Path,
        as_copy: bool,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, OperationError> {
        if cancel.is_requested() {
            return Err(LibraryError::Cancelled.into());
        }
        let snapshot = super::open_file(path)?;
        self.add_snapshot(&snapshot, as_copy, cancel)
    }
    /// Add captured content; copy construction retains the source's scoped ancestry.
    ///
    /// # Errors
    /// Preserves identity/contract failures before publication, then locked write failures.
    pub fn add_snapshot(
        &self,
        snapshot: &RecipeSnapshot,
        as_copy: bool,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, OperationError> {
        if cancel.is_requested() {
            return Err(LibraryError::Cancelled.into());
        }
        let mutation = if as_copy {
            let mut copy = Draft::from_snapshot(snapshot)?;
            copy.set_title(snapshot.title());
            Mutation::Create(Arc::new(copy.recipe()?))
        } else {
            Mutation::Add(Arc::new(snapshot.recipe().clone()))
        };
        Ok(self.apply(mutation, cancel)?)
    }
    /// Update a captured User key using bounded external content.
    ///
    /// # Errors
    /// Rejects non-User keys before opening input and checks expected content under lock.
    pub fn update_file(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        path: &Path,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, OperationError> {
        crate::storage::library::require_user(&key)?;
        let recipe = super::open_file(path)?.recipe().clone();
        Ok(self.update(key, expect, recipe, cancel)?)
    }
    /// Update validated portable content under its captured key and optional token.
    ///
    /// # Errors
    /// Rejects stale content, replaced history, wrong owners and Trash edits under lock.
    pub fn update(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        recipe: Recipe,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, LibraryError> {
        self.apply(
            Mutation::Update {
                key,
                expect,
                recipe: Arc::new(recipe),
            },
            cancel,
        )
    }
    /// Rename a User entry without changing its identity or creation history.
    ///
    /// # Errors
    /// Preserves content-token conflicts and publication failures.
    pub fn rename(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        title: String,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, LibraryError> {
        self.apply(Mutation::Rename { key, expect, title }, cancel)
    }
    /// Move a User entry to Trash with captured-content checks.
    ///
    /// # Errors
    /// Preserves conflicts and relocation postconditions.
    pub fn trash(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, LibraryError> {
        self.apply(Mutation::Trash { key, expect }, cancel)
    }
    /// Restore a captured Trash entry without replacing active content.
    ///
    /// # Errors
    /// Preserves conflicts and no-clobber relocation failures.
    pub fn restore(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, LibraryError> {
        self.apply(Mutation::Restore { key, expect }, cancel)
    }
    /// Permanently delete only a confirmed, captured Trash entry.
    ///
    /// # Errors
    /// Refuses missing confirmation, wrong membership and stale content under lock.
    pub fn purge(
        &self,
        key: LibraryKey,
        expect: Option<VersionToken>,
        confirmed: bool,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, LibraryError> {
        self.apply(
            Mutation::Purge {
                key,
                expect,
                confirmed,
            },
            cancel,
        )
    }
}

/// Immutable configured location, without native adapter operations.
#[derive(Debug, Clone)]
pub struct Location {
    /// Canonical configured path when it exists.
    pub path: PathBuf,
    /// Configuration precedence source.
    pub source: RootSource,
}
impl Location {
    /// Configured root path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Configuration precedence source.
    #[must_use]
    pub const fn source(&self) -> RootSource {
        self.source
    }
}
/// Inspection retains known location and existence even when layout cannot be verified.
#[derive(Debug)]
pub struct PathInspection {
    /// Resolved configuration.
    pub location: Location,
    /// Observed existence, unknown when inspection itself failed.
    pub exists: Option<bool>,
    /// Exact layout/locking observation.
    pub result: Result<RootInfo, LibraryError>,
}
/// Inspect a configured path without initializing it.
///
/// # Errors
/// Returns configuration failures before any inspection is possible.
pub fn path(context: &LibraryContext) -> Result<PathInspection, LibraryError> {
    let library = context.library()?;
    let location = Location {
        path: library.path().to_owned(),
        source: context.source(),
    };
    let result = library.inspect();
    let exists = result.as_ref().map_or_else(
        |_| library.path().try_exists().ok(),
        |info| Some(info.exists),
    );
    Ok(PathInspection {
        location,
        exists,
        result,
    })
}

/// Typed bounded inventory notices.
pub use crate::storage::library::InventoryNotice;

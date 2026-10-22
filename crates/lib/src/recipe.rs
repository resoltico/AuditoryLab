//! Immutable loaded recipes, their provenance and identity.
use crate::{Error, ErrorKind, input_admission};
use auditorylab_core::{About, Recipe, SampleRate, Signal, Stimulus};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Where a recipe snapshot came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A built-in recipe, by catalog ID.
    BuiltIn(&'static str),
    /// A recipe file, by the path it was read from.
    File(PathBuf),
    /// A labelled stream, such as standard input.
    Stream(String),
    /// An editor document, by its title when the snapshot was taken.
    Draft(String),
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(f, "{id}"),
            Self::File(path) => write!(f, "{path:?}"),
            Self::Stream(label) | Self::Draft(label) => write!(f, "{label}"),
        }
    }
}

/// The process-local identity of one snapshot. Loading the same path twice
/// gives two identities, because the file's content may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotId(u64);

static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(1);

impl SnapshotId {
    fn next() -> Self {
        Self(NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Immutable recipe content with its origin and identity. A snapshot is
/// neither necessarily saved to disk nor acoustically valid at any rate; its
/// values are checked when it is realized at a rate.
#[derive(Debug, Clone)]
pub struct RecipeSnapshot {
    id: SnapshotId,
    origin: Origin,
    recipe: Arc<Recipe>,
}

impl RecipeSnapshot {
    /// Capture content under a new identity.
    #[must_use]
    pub fn new(origin: Origin, recipe: Recipe) -> Self {
        Self {
            id: SnapshotId::next(),
            origin,
            recipe: Arc::new(recipe),
        }
    }

    /// The snapshot's identity.
    #[must_use]
    pub const fn id(&self) -> SnapshotId {
        self.id
    }

    /// Where the content came from.
    #[must_use]
    pub const fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The captured document.
    #[must_use]
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    /// Research and listening description.
    #[must_use]
    pub fn about(&self) -> Option<&About> {
        self.recipe.about.as_ref()
    }

    /// The recipe's title, or its origin when it has none.
    #[must_use]
    pub fn title(&self) -> String {
        self.about()
            .map_or_else(|| self.origin.to_string(), |a| a.title.clone())
    }

    /// Declared sound parameters.
    #[must_use]
    pub fn stimulus(&self) -> &Stimulus {
        &self.recipe.stimulus
    }

    /// Validate the recipe and realize it at a sample rate.
    ///
    /// # Errors
    /// Returns [`ErrorKind::InvalidSound`] naming the origin and the first
    /// failure at that rate.
    pub fn at(&self, rate: SampleRate) -> Result<Sound, Error> {
        let signal = self.stimulus().signal(rate).map_err(|e| {
            Error::caused(
                ErrorKind::InvalidSound,
                format!("{} at {} Hz: {e}", self.origin, rate.hz()),
                e,
            )
        })?;
        Ok(Sound {
            snapshot: self.clone(),
            signal: Arc::new(signal),
        })
    }
}

/// A snapshot realized as a validated signal at one rate.
#[derive(Debug, Clone)]
pub struct Sound {
    snapshot: RecipeSnapshot,
    signal: Arc<Signal>,
}

impl Sound {
    /// The realized snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &RecipeSnapshot {
        &self.snapshot
    }

    /// Seekable stereo signal.
    #[must_use]
    pub fn signal(&self) -> &Signal {
        &self.signal
    }

    pub(crate) const fn from_parts(snapshot: RecipeSnapshot, signal: Arc<Signal>) -> Self {
        Self { snapshot, signal }
    }

    #[cfg(feature = "device-playback")]
    pub(crate) fn shared_signal(&self) -> Arc<Signal> {
        Arc::clone(&self.signal)
    }
}

/// Load every built-in recipe in natural order.
///
/// # Errors
/// Returns an error only if a shipped recipe fails to parse.
pub fn catalog() -> Result<Vec<RecipeSnapshot>, Error> {
    auditorylab_core::catalog::ENTRIES
        .iter()
        .map(|e| builtin(e.id))
        .collect()
}

fn entry(id: &str) -> Result<&'static auditorylab_core::catalog::Entry, Error> {
    auditorylab_core::catalog::find(id).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownRecipe,
            format!("unknown built-in recipe {id:?}"),
        )
    })
}

/// Load a built-in recipe by exact ID.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`] for an unknown ID.
pub fn builtin(id: &str) -> Result<RecipeSnapshot, Error> {
    let e = entry(id)?;
    parse(e.text.as_bytes(), Origin::BuiltIn(e.id))
}

/// Return a built-in recipe's exact TOML text, comments included.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`] for an unknown ID.
pub fn builtin_source(id: &str) -> Result<&'static str, Error> {
    Ok(entry(id)?.text)
}

fn parse(bytes: &[u8], origin: Origin) -> Result<RecipeSnapshot, Error> {
    let recipe = Recipe::from_bytes(bytes)
        .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, format!("{origin}: {e}"), e))?;
    Ok(RecipeSnapshot::new(origin, recipe))
}

fn read(reader: impl Read, origin: Origin) -> Result<RecipeSnapshot, Error> {
    let mut bytes = Vec::new();
    reader
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            Error::caused(
                ErrorKind::Unreadable,
                format!("cannot read {origin}: {e}"),
                e,
            )
        })?;
    parse(&bytes, origin)
}

/// Read a bounded recipe stream.
///
/// # Errors
/// Returns [`ErrorKind::Unreadable`] or [`ErrorKind::InvalidRecipe`].
pub fn load_reader(reader: impl Read, label: &str) -> Result<RecipeSnapshot, Error> {
    read(reader, Origin::Stream(label.into()))
}

/// Open and read a regular recipe file without blocking on special files.
///
/// # Errors
/// Returns [`ErrorKind::Unreadable`] or [`ErrorKind::InvalidRecipe`].
pub fn load_file(path: &Path) -> Result<RecipeSnapshot, Error> {
    let file = input_admission::open(path).map_err(|e| match e {
        input_admission::Failure::Io(e) => Error::caused(
            ErrorKind::Unreadable,
            format!("cannot open {path:?}: {e}"),
            e,
        ),
        input_admission::Failure::NotRegular => Error::new(
            ErrorKind::Unreadable,
            format!("cannot read {path:?}: expected a regular file"),
        ),
    })?;
    read(file, Origin::File(path.to_owned()))
}

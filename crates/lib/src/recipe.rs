//! Immutable loaded recipes, their provenance and identity.
use crate::{Error, ErrorKind, input_admission};
use auditorylab_core::{About, Recipe, SampleRate, Signal, Sound};
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
    /// Captured managed identity and observed version, separate from snapshot ID.
    User {
        /// Portable User identity.
        id: auditorylab_core::DocumentUuid,
        /// Exact observed managed-record version.
        version: crate::library::VersionToken,
    },
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
            Self::User { id, .. } => write!(f, "user:{id}"),
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
    /// Process-local opaque number for native bindings; never a persistent selector.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    fn next() -> Self {
        match NEXT_SNAPSHOT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)) {
            Ok(value) => Self(value),
            Err(_) => panic!("process-local snapshot identities exhausted"),
        }
    }
}

/// Immutable recipe content with its origin and identity. A snapshot is
/// neither necessarily saved to disk nor acoustically valid at any rate; its
/// values are checked when it is realized at a rate.
#[derive(Debug, Clone)]
pub struct RecipeSnapshot {
    id: SnapshotId,
    origin: Origin,
    external: Option<crate::external::ObservedFile>,
    recipe: Arc<Recipe>,
    about: Arc<Option<About>>,
    realizations: Arc<std::sync::Mutex<Realizations>>,
    nominal: Arc<auditorylab_core::nominal::Analysis>,
}

/// Two rates per immutable snapshot cover inspection/export and device use.
/// Cache ownership follows the snapshot; evictions never retain a Rendition back-reference.
#[derive(Debug, Default)]
struct Realizations {
    values: Vec<(
        SampleRate,
        Result<Arc<Signal>, auditorylab_core::StimulusError>,
    )>,
    #[cfg(test)]
    compilations: u64,
}

impl RecipeSnapshot {
    /// Capture content under a new identity.
    #[must_use]
    pub fn new(origin: Origin, recipe: Recipe) -> Self {
        let nominal = Arc::new(auditorylab_core::nominal::analyze(
            &recipe.stimulus.components,
        ));
        let about = Arc::new(recipe.display_about());
        Self {
            nominal,
            about,
            id: SnapshotId::next(),
            origin,
            external: None,
            recipe: Arc::new(recipe),
            realizations: Arc::new(std::sync::Mutex::new(Realizations::default())),
        }
    }

    pub(crate) fn with_external(mut self, external: crate::external::ObservedFile) -> Self {
        self.external = Some(external);
        self
    }
    /// Exact external file observation captured from the same opened content.
    #[must_use]
    pub const fn external_file(&self) -> Option<&crate::external::ObservedFile> {
        self.external.as_ref()
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

    /// Cached sample-rate-independent nominal classification of this snapshot.
    #[must_use]
    pub fn nominal(&self) -> &auditorylab_core::nominal::Analysis {
        &self.nominal
    }

    /// Research and listening description.
    #[must_use]
    pub fn about(&self) -> Option<&About> {
        self.about.as_ref().as_ref()
    }

    /// The recipe's title, or its origin when it has none.
    #[must_use]
    pub fn title(&self) -> String {
        self.about().map_or_else(
            || match &self.origin {
                Origin::File(path) => path
                    .file_stem()
                    .filter(|name| !name.is_empty())
                    .map_or_else(
                        || "Untitled sound".into(),
                        |name| {
                            name.to_string_lossy()
                                .chars()
                                .flat_map(|c| {
                                    if c.is_control() {
                                        c.escape_default().collect::<Vec<_>>()
                                    } else {
                                        vec![c]
                                    }
                                })
                                .collect()
                        },
                    ),
                Origin::User { .. } => "Untitled sound".into(),
                other => other.to_string(),
            },
            |a| a.title.clone(),
        )
    }

    /// A concise safe output filename, independent of full origin diagnostics.
    #[must_use]
    pub fn suggested_output_name(&self, extension: &str) -> String {
        match self.origin() {
            Origin::BuiltIn(id) => file_name(id, extension),
            Origin::File(path) if self.about().is_none() => file_name(
                &path
                    .file_stem()
                    .map_or_else(|| "sound".into(), |name| name.to_string_lossy()),
                extension,
            ),
            _ => file_name(&self.title(), extension),
        }
    }

    /// Declared sound parameters.
    #[must_use]
    pub fn stimulus(&self) -> &Sound {
        &self.recipe.stimulus
    }

    /// Validate the recipe and realize it at a sample rate.
    ///
    /// # Errors
    /// Returns [`ErrorKind::InvalidSound`] naming the origin and the first
    /// failure at that rate.
    pub fn at(&self, rate: SampleRate) -> Result<Rendition, Error> {
        self.realize(rate).map_err(|e| {
            Error::caused(
                ErrorKind::InvalidSound,
                format!("{} at {} Hz: {e}", self.origin, rate.hz()),
                e,
            )
        })
    }

    pub(crate) fn realize(
        &self,
        rate: SampleRate,
    ) -> Result<Rendition, auditorylab_core::StimulusError> {
        // Application/device preparation only; callbacks use the captured Signal.
        let mut cache = self
            .realizations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let signal = if let Some((_, result)) = cache.values.iter().find(|(r, _)| *r == rate) {
            result.clone()?
        } else {
            #[cfg(test)]
            {
                cache.compilations += 1;
            }
            let result = self.stimulus().signal(rate).map(Arc::new);
            if cache.values.len() == 2 {
                let _ = cache.values.remove(0);
            }
            cache.values.push((rate, result.clone()));
            result?
        };
        Ok(Rendition {
            snapshot: self.clone(),
            signal,
        })
    }

    #[cfg(test)]
    pub(crate) fn realization_count(&self) -> u64 {
        self.realizations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .compilations
    }

    #[cfg(test)]
    pub(crate) fn cached_rates(&self) -> usize {
        self.realizations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values
            .len()
    }
}

/// A snapshot realized as a validated signal at one rate.
#[derive(Debug, Clone)]
pub struct Rendition {
    snapshot: RecipeSnapshot,
    signal: Arc<Signal>,
}

impl Rendition {
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
    let mut bytes = Vec::new();
    let mut file = file;
    (&mut file)
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::caused(ErrorKind::Unreadable, "cannot read external recipe", e))?;
    let snapshot = parse(&bytes, Origin::File(path.to_owned()))?;
    let observed = crate::external::ObservedFile::capture(path, file, &bytes)?;
    Ok(snapshot.with_external(observed))
}

/// A file name suggested from a title: characters that file systems reject
/// become spaces, and the name is kept short.
pub(crate) fn file_name(title: &str, extension: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut stem: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((cut, _)) = stem.char_indices().nth(80) {
        stem.truncate(cut);
    }
    let stem = stem.trim_matches(|c: char| c == '.' || c.is_whitespace());
    format!(
        "{}.{extension}",
        if stem.is_empty() { "sound" } else { stem }
    )
}

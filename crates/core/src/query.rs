//! Shared discovery choices; query execution belongs to the application.
use serde::Serialize;
/// Persistent All excludes temporary entries and Trash unless explicitly selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// Active Built-in and User entries.
    All,
    /// Curated immutable entries only.
    #[serde(rename = "builtin")]
    BuiltIn,
    /// Active managed entries only.
    User,
    /// Temporary Open files, not persisted by viewing.
    OpenFiles,
    /// Managed trashed records only.
    Trash,
}

/// Primary sort, with scope-dependent defaults only while Automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    /// Rate ascending for All/Built-in; modified descending for User.
    Automatic,
    /// Matching nominal rate.
    Rate,
    /// Normalized title.
    Title,
    /// Current declared study identity.
    Study,
    /// Storage ownership.
    Collection,
    /// Declared creation instant.
    Created,
    /// Declared edit instant.
    Modified,
    /// Local Added here instant.
    Added,
}

/// Direction applies only to the primary value; nulls and tie-breaks stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Ascending primary values.
    Asc,
    /// Descending primary values.
    Desc,
}

/// Optional repeated-key grouping; the flat result is always unique by key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    /// No groups.
    None,
    /// Reference bands/special rhythm states.
    Band,
    /// Structural methods.
    Method,
    /// Current-condition studies.
    Study,
    /// Ownership.
    Collection,
}

/// Special rhythm navigation; these properties can overlap band memberships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RhythmState {
    /// At least one positive rate outside the reference intervals.
    OutsideBands,
    /// Complete analysis with no rhythmic descriptors.
    None,
    /// At least two independent processes, even within one band.
    Multiple,
    /// Incomplete interpretation or document diagnostics.
    NeedsReview,
}

crate::vocabulary::vocabulary! { Scope,"scope" {
    All => ("all","All · Built-in + User","all · Built-in + User"),
    BuiltIn => ("builtin","Built-in · Read only","built-in · Read only"),
    User => ("user","User","user"),
    OpenFiles => ("open-files","Open files · Temporary","open files · Temporary"),
    Trash => ("trash","Trash","trash"),
}}

crate::vocabulary::vocabulary! { SortBy,"sort_by" {
    Automatic => ("automatic","Automatic order","automatic order"),
    Rate => ("rate","Nominal rate","nominal rate"),
    Title => ("title","Title","title"),
    Created => ("created","Created","created"),
    Modified => ("modified","Modified","modified"),
    Added => ("added","Added here","added here"),
    Study => ("study","Study","study"),
    Collection => ("collection","Collection","collection"),
}}

crate::vocabulary::vocabulary! { Direction,"direction" {
    Asc => ("asc","Ascending","ascending"),
    Desc => ("desc","Descending","descending"),
}}

crate::vocabulary::vocabulary! { GroupBy,"group_by" {
    None => ("none","No grouping","no grouping"),
    Band => ("band","Band groups","band groups"),
    Method => ("method","Method groups","method groups"),
    Study => ("study","Study groups","study groups"),
    Collection => ("collection","Collection groups","collection groups"),
}}

crate::vocabulary::vocabulary! { RhythmState,"rhythm_state" {
    OutsideBands => ("outside-bands","Other rates","other rates"),
    None => ("none","No rhythmic rate","no rhythmic rate"),
    Multiple => ("multiple","Multiple rhythms","multiple rhythms"),
    NeedsReview => ("needs-review","Needs review","needs review"),
}}

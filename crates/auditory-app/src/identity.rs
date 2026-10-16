// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! One source/kind/target snapshot, with product chosen separately by each frontend.
/// A checked distributable product, independent of shared dependency compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// Terminal application.
    Cli,
    /// Desktop application.
    Gui,
}
impl Product {
    /// Product key shared with release tooling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Gui => "gui",
        }
    }
    /// Cargo package identity, checked against workspace metadata by tooling.
    pub const fn package(self) -> &'static str {
        match self {
            Self::Cli => "auditory-cli",
            Self::Gui => "auditory-gui",
        }
    }
    /// Sole binary target of this product.
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Cli => "auditorylab",
            Self::Gui => "auditorylab-gui",
        }
    }
}
/// Build intent, not a claim that an artifact has been published or authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    /// Unqualified direct Cargo build; no source revision is claimed.
    Local,
    /// Controlled source build for development qualification.
    Candidate,
    /// Controlled source build for the release workflow.
    Release,
}
impl BuildKind {
    /// Stable protocol spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Candidate => "candidate",
            Self::Release => "release",
        }
    }
}
/// Immutable artifact identity. Self-reporting is not proof of build authenticity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildIdentity {
    product: Product,
    kind: BuildKind,
    source: Option<&'static str>,
}
impl BuildIdentity {
    /// Combine validated common build metadata with this caller's product.
    pub fn for_product(product: Product) -> Self {
        let kind = match env!("AUDITORY_IDENTITY_KIND") {
            "local" => BuildKind::Local,
            "candidate" => BuildKind::Candidate,
            "release" => BuildKind::Release,
            _ => unreachable!("build script validates build kind"),
        };
        let source = match env!("AUDITORY_IDENTITY_SOURCE") {
            "" => None,
            source => Some(source),
        };
        Self {
            product,
            kind,
            source,
        }
    }
    /// Version of the narrow machine identity response.
    pub const fn format_version(self) -> u32 {
        1
    }
    /// Product selected at this frontend call site.
    pub const fn product(self) -> Product {
        self.product
    }
    /// Workspace package version, which does not itself imply release status.
    pub const fn version(self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    /// Qualified or local build intent.
    pub const fn kind(self) -> BuildKind {
        self.kind
    }
    /// Full supplied source revision, absent on local unqualified builds.
    pub const fn source_revision(self) -> Option<&'static str> {
        self.source
    }
    /// Cargo's actual target triple.
    pub const fn target(self) -> &'static str {
        env!("AUDITORY_IDENTITY_TARGET")
    }
    /// Exact narrow JSON protocol for package verification, with no platform initialization.
    pub fn to_json(self) -> String {
        // All fields are fixed product strings, Cargo semver/target identifiers,
        // or a build-script-validated hexadecimal revision, so none need JSON escaping.
        let source = self
            .source
            .map_or_else(|| "null".into(), |source| format!("\"{source}\""));
        format!(
            "{{\"format_version\":{},\"product\":\"{}\",\"package\":\"{}\",\"bin\":\"{}\",\"version\":\"{}\",\"build_kind\":\"{}\",\"source_revision\":{},\"target\":\"{}\"}}",
            self.format_version(),
            self.product.as_str(),
            self.product.package(),
            self.product.binary(),
            self.version(),
            self.kind.as_str(),
            source,
            self.target()
        )
    }
    /// Human identity for version/About views, retaining local qualification status.
    pub fn summary(self) -> String {
        format!(
            "{} {} ({}; {}; source {})",
            self.product.binary(),
            self.version(),
            self.kind.as_str(),
            self.target(),
            self.source.unwrap_or("unqualified")
        )
    }
}

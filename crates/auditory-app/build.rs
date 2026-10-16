// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Validate controlled qualification inputs and record Cargo target metadata.
mod build_metadata;
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_metadata.rs");
    for name in ["AUDITORY_BUILD_KIND", "AUDITORY_SOURCE_REVISION"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let optional = |name| match std::env::var(name) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("{name} must be Unicode; unset identity inputs for local builds")
        }
    };
    let kind = optional("AUDITORY_BUILD_KIND");
    let source = optional("AUDITORY_SOURCE_REVISION");
    let (kind, source) = build_metadata::qualification(kind.as_deref(), source.as_deref())
        .unwrap_or_else(|error| panic!("{error}"));
    let target = std::env::var("TARGET")
        .unwrap_or_else(|_| panic!("Cargo TARGET is required when building auditory-app"));
    println!("cargo:rustc-env=AUDITORY_IDENTITY_KIND={kind}");
    println!(
        "cargo:rustc-env=AUDITORY_IDENTITY_SOURCE={}",
        source.unwrap_or("")
    );
    println!("cargo:rustc-env=AUDITORY_IDENTITY_TARGET={target}");
}

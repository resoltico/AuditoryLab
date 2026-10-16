// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Compile only the contained GUI with explicitly embedded resources.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_imports()?;
    // Pinned slint-build/i-slint-compiler inputs; track absent values as well.
    for name in [
        "SLINT_STYLE",
        "SLINT_FONT_SIZES",
        "SLINT_SCALE_FACTOR",
        "SLINT_ASSET_SECTION",
        "SLINT_EMBED_RESOURCES",
        "SLINT_EMBED_TEXTURES",
        "SLINT_EMIT_DEBUG_INFO",
        "SLINT_LIVE_PREVIEW",
        "SLINT_BUNDLE_TRANSLATIONS",
        "SLINT_INLINING",
        "SLINT_ENABLE_EXPERIMENTAL_FEATURES",
        "SLINT_CPP_NAMESPACE",
        "SLINT_COMPILER_DENY_WARNINGS",
        "SLINT_INCLUDE_GENERATED",
        "DEP_MCU_BOARD_SUPPORT_BOARD_CONFIG_PATH",
        "DEP_MCU_BOARD_SUPPORT_MCU_BOARD_CONFIG_PATH",
        "DEP_MCU_BOARD_SUPPORT_MCU_EMBED_TEXTURES",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    for (name, value) in std::env::vars_os() {
        let spelling = name.to_string_lossy();
        if spelling.starts_with("SLINT_") || spelling.starts_with("DEP_MCU_BOARD_SUPPORT_") {
            println!("cargo:rerun-if-env-changed={spelling}");
            if name != "SLINT_COMPILER_DENY_WARNINGS" || value != "1" {
                return Err(format!("unset unsupported GUI compiler override {name:?}; only SLINT_COMPILER_DENY_WARNINGS=1 is accepted").into());
            }
        }
    }
    let configuration = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", configuration)?;
    Ok(())
}

// These names are checked against the reviewed import-producer manifest. The
// explicit preparer verifies bytes; this check gives ordinary Cargo users an
// actionable prerequisite without searching the registry or invoking Cargo.
fn windows_imports() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET")?;
    if !matches!(
        target.as_str(),
        "aarch64-pc-windows-gnullvm" | "x86_64-pc-windows-gnullvm"
    ) {
        return Ok(());
    }
    let directory = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or("missing Cargo manifest directory")?,
    )
    .join("../..")
    .canonicalize()?
    .join("target/windows-imports")
    .join(&target);
    for name in [
        "libadvapi32.a",
        "libgdi32.a",
        "libopengl32.a",
        "libshell32.a",
        "libshlwapi.a",
    ] {
        let path = directory.join(name);
        if !path.is_file() {
            return Err(format!("missing reviewed Windows GUI import views; run python scripts/windows_runtime.py prepare --target {target}").into());
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rustc-link-search=native={}", directory.display());
    Ok(())
}

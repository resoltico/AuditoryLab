//! Generate product identity from the canonical workspace manifest.
mod identity_input;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=identity_input.rs");
    println!("cargo:rerun-if-env-changed=AUDITORYLAB_SOURCE_REVISION");
    let text = std::fs::read_to_string("../../Cargo.toml")?;
    let manifest: toml::Value = toml::from_str(&text)?;
    let metadata = manifest
        .get("workspace")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("auditorylab"))
        .ok_or("workspace.metadata.auditorylab is required")?;
    let name = metadata
        .get("product-name")
        .and_then(toml::Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("workspace.metadata.auditorylab.product-name must be a nonempty string")?;
    let copyright = metadata
        .get("copyright")
        .and_then(toml::Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("workspace.metadata.auditorylab.copyright must be a nonempty string")?;
    let build = metadata
        .get("build-number")
        .and_then(toml::Value::as_integer)
        .filter(|n| (1..=9999).contains(n))
        .ok_or("workspace.metadata.auditorylab.build-number must be an integer in 1..9999")?;
    let revision = std::env::var("AUDITORYLAB_SOURCE_REVISION")
        .ok()
        .filter(|s| !s.is_empty());
    if let Some(value) = &revision {
        identity_input::revision(value)?;
    }
    let source = format!(
        "/// Canonical identity resolved from workspace metadata.\npub(super) const BUILD: BuildIdentity = BuildIdentity {{ name: {name:?}, copyright: {copyright:?}, version: {:?}, build: {build}, revision: {:?} }};",
        std::env::var("CARGO_PKG_VERSION")?,
        revision.as_deref()
    );
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR")?).join("identity.rs"),
        source,
    )?;
    Ok(())
}

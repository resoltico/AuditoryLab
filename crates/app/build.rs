//! Generate product identity from the canonical workspace manifest.
#[expect(
    dead_code,
    reason = "Shared build readers use different parts of identity validation and \
        native resource generation"
)]
mod identity_input;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=identity_input.rs");
    println!("cargo:rerun-if-env-changed=AUDITORYLAB_SOURCE_REVISION");
    let text = std::fs::read_to_string("../../Cargo.toml")?;
    let manifest: toml::Value = toml::from_str(&text)?;
    let product = identity_input::product(&manifest)?;
    let name = product.name;
    let copyright = product.copyright;
    let revision = std::env::var("AUDITORYLAB_SOURCE_REVISION")
        .ok()
        .filter(|s| !s.is_empty());
    if let Some(value) = &revision {
        identity_input::revision(value)?;
    }
    let source = format!(
        "/// Canonical identity resolved from workspace metadata.\npub(super) \
            const BUILD: BuildIdentity = BuildIdentity {{ name: {name:?}, \
            copyright: {copyright:?}, version: {:?}, revision: {:?} }};",
        std::env::var("CARGO_PKG_VERSION")?,
        revision.as_deref()
    );
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR")?).join("identity.rs"),
        source,
    )?;
    Ok(())
}

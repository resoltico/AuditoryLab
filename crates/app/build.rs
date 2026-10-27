//! Generate product identity from the canonical workspace manifest.
mod identity_input;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=identity_input.rs");
    println!("cargo:rerun-if-env-changed=AUDITORYLAB_SOURCE_REVISION");
    let text = std::fs::read_to_string("../../Cargo.toml")?;
    let manifest: toml::Value = toml::from_str(&text)?;
    let build = manifest
        .get("workspace")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("auditorylab"))
        .and_then(|v| v.get("build-number"))
        .and_then(toml::Value::as_integer)
        .filter(|n| (1..=9999).contains(n))
        .ok_or("workspace.metadata.auditorylab.build-number must be an integer in 1..9999")?;
    let revision = std::env::var("AUDITORYLAB_SOURCE_REVISION")
        .ok()
        .filter(|s| !s.is_empty());
    if let Some(value) = &revision {
        identity_input::revision(value)?;
    }
    println!("cargo:rerun-if-changed=../../LICENSE");
    let license = std::fs::read_to_string("../../LICENSE")?;
    let mut lines = license.lines().filter(|line| !line.is_empty());
    let name = lines.next().ok_or("LICENSE requires the product name")?;
    let copyright = lines
        .next()
        .filter(|line| line.starts_with("Copyright "))
        .ok_or("LICENSE requires copyright metadata")?
        .replace("(c)", "©");
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

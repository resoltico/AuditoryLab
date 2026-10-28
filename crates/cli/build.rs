//! Embed the canonical product's native Windows executable identity.
// CLI and GUI resources share the app/packaging reader's metadata validation.
#[path = "../app/identity_input.rs"]
#[expect(
    dead_code,
    reason = "Native resources share validation with readers that also accept source revisions"
)]
mod identity_input;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../app/identity_input.rs");
    if std::env::var("CARGO_CFG_TARGET_OS")? == "windows" {
        let manifest: toml::Value = toml::from_str(&std::fs::read_to_string("../../Cargo.toml")?)?;
        let resource = identity_input::windows_resource(
            identity_input::product(&manifest)?,
            &std::env::var("CARGO_PKG_VERSION")?,
        )?;
        let path = std::path::Path::new(&std::env::var("OUT_DIR")?).join("version.rc");
        std::fs::write(&path, resource)?;
        embed_resource::compile(path, embed_resource::NONE).manifest_required()?;
    }
    Ok(())
}

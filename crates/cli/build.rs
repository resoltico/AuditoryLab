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
            identity_input::Executable::Cli,
            &std::env::var("CARGO_PKG_VERSION")?,
        )?;
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
        let manifest_path = out.join("app.manifest");
        std::fs::write(
            &manifest_path,
            identity_input::windows_manifest(identity_input::Executable::Cli),
        )?;
        let path = out.join("version.rc");
        std::fs::write(
            &path,
            resource + &identity_input::windows_manifest_resource(&manifest_path),
        )?;
        embed_resource::compile(path, embed_resource::NONE).manifest_required()?;
    }
    Ok(())
}

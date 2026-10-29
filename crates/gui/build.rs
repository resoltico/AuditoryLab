//! Compile the desktop interface.
// The desktop build script and binary share the same desktop-owned chrome inventory.
use std::fmt::Write as _;
#[path = "src/terms.rs"]
mod terms;
// Native resources and app identity validate the same workspace-owned metadata.
#[path = "../app/identity_input.rs"]
#[expect(
    dead_code,
    reason = "Native resources share validation with readers that also accept source revisions"
)]
mod identity_input;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let mut vocabulary = String::from("export global Terms {\n");
    let mut properties = std::collections::BTreeSet::new();
    for (property, label) in terms::UI_TERMS {
        append_term(&mut vocabulary, &mut properties, property, label)?;
    }
    for property in [
        "caption-product-name",
        "caption-copyright",
        "caption-about-product",
    ] {
        append_term(&mut vocabulary, &mut properties, property, "")?;
    }
    vocabulary.push_str("}\n");
    write_generated(&generated.join("vocabulary.slint"), &vocabulary)?;
    slint_build::compile_with_config(
        "ui/main.slint",
        slint_build::CompilerConfiguration::new()
            .with_include_paths(vec![generated])
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )?;
    println!("cargo:rerun-if-changed=../../LICENSE");
    println!(
        "cargo:rerun-if-changed=../../artwork/auditorylab-icon/assets/runtime/window-icon.png"
    );
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../app/identity_input.rs");
    if std::env::var("CARGO_CFG_TARGET_OS")?.as_str() == "windows" {
        println!("cargo:rerun-if-changed=app.rc");
        println!(
            "cargo:rerun-if-changed=../../artwork/auditorylab-icon/assets/windows/auditorylab.ico"
        );
        let manifest: toml::Value = toml::from_str(&std::fs::read_to_string("../../Cargo.toml")?)?;
        let resource = identity_input::windows_resource(
            identity_input::product(&manifest)?,
            identity_input::Executable::Desktop,
            &std::env::var("CARGO_PKG_VERSION")?,
        )?;
        let icon = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR")?)
            .join("../../artwork/auditorylab-icon/assets/windows/auditorylab.ico")
            .canonicalize()?;
        let source = std::fs::read_to_string("app.rc")?
            .replace("@ICON_PATH@", &icon.to_string_lossy().replace('\\', "/"));
        let path = std::path::Path::new(&std::env::var("OUT_DIR")?).join("app.rc");
        std::fs::write(&path, source + &resource)?;
        embed_resource::compile(path, embed_resource::NONE).manifest_required()?;
    }
    Ok(())
}

fn append_term(
    output: &mut String,
    properties: &mut std::collections::BTreeSet<String>,
    property: &str,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !properties.insert(property.into()) {
        return Err(format!("duplicate Slint vocabulary property {property}").into());
    }
    let _ = writeln!(
        output,
        "    in-out property <string> {property}: {label:?};"
    );
    Ok(())
}
fn write_generated(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if std::fs::read(path).ok().as_deref() != Some(contents.as_bytes()) {
        std::fs::write(path, contents)?;
    }
    Ok(())
}

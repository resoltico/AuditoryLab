//! Compile the desktop interface.
// The desktop build script and binary share the same desktop-owned chrome inventory.
#[path = "src/terms.rs"]
mod terms;
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
    if std::env::var("CARGO_CFG_TARGET_OS")?.as_str() == "windows" {
        println!("cargo:rerun-if-changed=app.rc");
        println!(
            "cargo:rerun-if-changed=../../artwork/auditorylab-icon/assets/windows/auditorylab.ico"
        );
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_required()?;
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
    output.push_str(&format!(
        "    in-out property <string> {property}: {label:?};\n"
    ));
    Ok(())
}
fn write_generated(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if std::fs::read(path).ok().as_deref() != Some(contents.as_bytes()) {
        std::fs::write(path, contents)?;
    }
    Ok(())
}

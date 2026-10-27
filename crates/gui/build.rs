//! Compile the desktop interface.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let mut vocabulary = String::from("export global Terms {\n");
    for (property, label) in auditorylab_app::presentation::vocabulary::UI_TERMS {
        // Rust debug string escaping also satisfies Slint's string grammar.
        vocabulary.push_str(&format!(
            "    in-out property <string> {property}: {label:?};\n"
        ));
    }
    vocabulary.push_str("}\n");
    let vocabulary_path = generated.join("vocabulary.slint");
    if std::fs::read(&vocabulary_path).ok().as_deref() != Some(vocabulary.as_bytes()) {
        std::fs::write(&vocabulary_path, vocabulary)?;
    }
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

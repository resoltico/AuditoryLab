//! Compile the desktop interface.
#[path = "src/terms.rs"]
#[expect(
    dead_code,
    reason = "native text helpers share the desktop chrome module with the Slint build-time inventory"
)]
mod terms;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let mut vocabulary = String::from("export global Terms {\n");
    for (property, label) in auditorylab_app::presentation::vocabulary::UI_TERMS
        .iter()
        .chain(terms::UI_TERMS)
    {
        // Rust debug string escaping also satisfies Slint's string grammar.
        vocabulary.push_str(&format!(
            "    in-out property <string> {property}: {label:?};\n"
        ));
    }
    vocabulary.push_str("}\n");
    use auditorylab_app::core::{ConditionRole, Population, rhythm::Completeness};
    use auditorylab_app::discovery::collection::{Direction, GroupBy, Scope, SortBy};
    vocabulary.push_str("export global Choices {\n");
    let mut choices = |name: &str, labels: Vec<&str>| {
        vocabulary.push_str(&format!(
            "    out property <[string]> {name}: [{}];\n",
            labels
                .iter()
                .map(|label| format!("{label:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    choices(
        "collection-labels",
        Scope::ALL.iter().map(|value| value.label()).collect(),
    );
    choices(
        "sort-labels",
        SortBy::ALL.iter().map(|value| value.label()).collect(),
    );
    choices(
        "population-labels",
        std::iter::once(terms::Facet::AnyPopulation.label())
            .chain(Population::ALL.iter().map(|value| value.label()))
            .collect(),
    );
    choices(
        "role-labels",
        std::iter::once(terms::Facet::AnyConditionRole.label())
            .chain(ConditionRole::ALL.iter().map(|value| value.label()))
            .collect(),
    );
    choices(
        "analysis-labels",
        std::iter::once(terms::Facet::AnyAnalysisCoverage.label())
            .chain(Completeness::ALL.iter().map(|value| value.label()))
            .collect(),
    );
    choices(
        "direction-labels",
        std::iter::once(terms::Caption::AutomaticDirection.label())
            .chain(Direction::ALL.iter().map(|value| value.label()))
            .collect(),
    );
    choices(
        "group-labels",
        GroupBy::ALL.iter().map(|value| value.label()).collect(),
    );
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

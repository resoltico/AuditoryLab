//! Compile the desktop interface.
#[path = "src/terms.rs"]
mod terms;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let mut vocabulary = String::from("export global Terms {\n");
    let mut properties = std::collections::BTreeSet::new();
    macro_rules! terms_from {
        ($($kind:ty),* $(,)?)=>{$(append_terms::<$kind>(&mut vocabulary,&mut properties)?;)*};
    }
    use auditorylab_core::{
        CarrierKind, ConditionRole, Ear, Ears, EvidenceScope, Measure, ModulationKind, PairKind,
        ParameterId, Population, SlotRole,
        query::{Direction, GroupBy, RhythmState, Scope, SortBy},
        rhythm::{Band, Completeness, Method},
        vocabulary::{Action, Status},
    };
    terms_from!(
        Action,
        Status,
        Scope,
        SortBy,
        Direction,
        GroupBy,
        RhythmState,
        ConditionRole,
        Population,
        Measure,
        EvidenceScope,
        Ear,
        Ears,
        PairKind,
        CarrierKind,
        ModulationKind,
        ParameterId,
        SlotRole,
        Band,
        Method,
        Completeness
    );
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
fn append_terms<T: auditorylab_core::vocabulary::Vocabulary>(
    output: &mut String,
    properties: &mut std::collections::BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .ok_or("missing vocabulary type")?;
    let mut prefix = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            prefix.push('-');
        }
        prefix.push(ch.to_ascii_lowercase());
    }
    for value in T::ALL {
        let property = format!("{prefix}-{}", value.key().replace('_', "-"));
        append_term(output, properties, &property, value.term().label)?;
    }
    Ok(())
}

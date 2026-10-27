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
        vocabulary::{Operation, Status},
    };
    terms_from!(
        Operation,
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
    write_generated(&generated.join("vocabulary.slint"), &vocabulary)?;
    let mut parameter_enum = String::from("export enum ParameterKey {\n");
    let mut to_binding = String::from(
        "pub(crate) const fn parameter_key(id: ParameterId) -> ParameterKey { match id {\n",
    );
    let mut to_core = String::from(
        "pub(crate) const fn parameter_id(key: ParameterKey) -> ParameterId { match key {\n",
    );
    for id in ParameterId::ALL {
        parameter_enum.push_str(&format!("    {},\n", id.key().replace('_', "-")));
        let binding_name = id
            .key()
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_ascii_uppercase().to_string() + chars.as_str()
                })
            })
            .collect::<String>();
        let core_name = id.variant_name();
        to_binding.push_str(&format!(
            "ParameterId::{core_name} => ParameterKey::{binding_name},\n"
        ));
        to_core.push_str(&format!(
            "ParameterKey::{binding_name} => ParameterId::{core_name},\n"
        ));
    }
    parameter_enum.push_str("}\n");
    to_binding.push_str("}}\n");
    to_core.push_str("}}\n");
    write_generated(&generated.join("parameters.slint"), &parameter_enum)?;
    std::fs::write(
        generated.join("parameter_bindings.rs"),
        to_binding + &to_core,
    )?;
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

fn write_generated(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if std::fs::read(path).ok().as_deref() != Some(contents.as_bytes()) {
        std::fs::write(path, contents)?;
    }
    Ok(())
}

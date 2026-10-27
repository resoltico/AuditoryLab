//! Recipe schema 4 preserves canonical bytes for every owned recipe.
use auditorylab_core::{Recipe, catalog};
use std::{collections::BTreeSet, fs, path::Path};
#[test]
fn catalog_and_fixture_serializations_preserve_the_schema_four_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let expected = root.join("canonical");
    let mut catalog_names = BTreeSet::new();
    for entry in catalog::ENTRIES {
        catalog_names.insert(format!("{}.toml", entry.id));
        let canonical = Recipe::parse(entry.text).unwrap().to_toml().unwrap();
        assert_eq!(
            canonical.as_bytes(),
            fs::read(expected.join("catalog").join(format!("{}.toml", entry.id))).unwrap(),
            "catalog {}",
            entry.id
        );
    }
    let expected_catalog: BTreeSet<_> = fs::read_dir(expected.join("catalog"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        catalog_names, expected_catalog,
        "catalog golden discovery must be complete"
    );
    let mut fixture_names = BTreeSet::new();
    for entry in fs::read_dir(root.join("recipes")).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(path.extension().unwrap(), "toml");
        fixture_names.insert(path.file_name().unwrap().to_owned());
        let canonical = Recipe::parse(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .to_toml()
            .unwrap();
        assert_eq!(
            canonical.as_bytes(),
            fs::read(expected.join("recipes").join(path.file_name().unwrap())).unwrap(),
            "fixture {}",
            path.display()
        );
    }
    let expected_fixtures: BTreeSet<_> = fs::read_dir(expected.join("recipes"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(!catalog_names.is_empty() && !fixture_names.is_empty());
    assert_eq!(
        fixture_names, expected_fixtures,
        "fixture golden discovery must be complete"
    );
}

//! Validation shared by identity generation and its unit tests.
pub(crate) fn revision(value: &str) -> Result<(), &'static str> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(
            "AUDITORYLAB_SOURCE_REVISION must contain a full 40- or 64-character \
                ASCII hexadecimal commit ID",
        )
    }
}

/// Product identity declared in the canonical workspace manifest.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Product<'a> {
    pub(crate) name: &'a str,
    pub(crate) copyright: &'a str,
}
pub(crate) fn product(manifest: &toml::Value) -> Result<Product<'_>, &'static str> {
    let metadata = manifest
        .get("workspace")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("auditorylab"))
        .ok_or("workspace.metadata.auditorylab is required")?;
    let name = metadata
        .get("product-name")
        .and_then(toml::Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("workspace.metadata.auditorylab.product-name must be a nonempty string")?;
    let copyright = metadata
        .get("copyright")
        .and_then(toml::Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("workspace.metadata.auditorylab.copyright must be a nonempty string")?;
    Ok(Product { name, copyright })
}
/// Native executable resource; numeric versions are separate from identity text.
pub(crate) fn windows_resource(
    product: Product<'_>,
    version: &str,
) -> Result<String, &'static str> {
    let plain = version.split(['-', '+']).next().ok_or("missing version")?;
    let values = plain
        .split('.')
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Windows version components must fit in 16 bits")?;
    let [major, minor, patch] = values.as_slice() else {
        return Err("version must contain three numeric components");
    };
    let native = format!("{major}.{minor}.{patch}.0");
    Ok(format!(
        r#"#pragma code_page(65001)
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileVersion", {native:?}
            VALUE "ProductVersion", {native:?}
            VALUE "ProductName", {name:?}
            VALUE "LegalCopyright", {copyright:?}
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
        name = product.name,
        copyright = product.copyright
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn revision_validation_rejects_partial_and_non_ascii_values() {
        for value in [
            "",
            "abc",
            "éééééééééééééééééééé",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert!(super::revision(value).is_err());
        }
        assert!(super::revision(&"a".repeat(40)).is_ok());
        assert!(super::revision(&"F".repeat(64)).is_ok());
    }
}

#[cfg(test)]
mod product_tests {
    use super::*;
    #[test]
    fn product_identity_requires_nonempty_canonical_fields() {
        let manifest: toml::Value = toml::from_str(
            "[workspace.metadata.auditorylab]\nproduct-name\
            ='Auditory Lab'\ncopyright='Copyright © 2026'\n",
        )
        .unwrap();
        let identity = product(&manifest).unwrap();
        assert_eq!(identity.name, "Auditory Lab");
        assert_eq!(identity.copyright, "Copyright © 2026");
        for source in [
            "",
            "[workspace.metadata.auditorylab]\nproduct-name=''\ncopyright='Copyright'",
            "[workspace.metadata.auditorylab]\nproduct-name='Name'",
        ] {
            assert!(product(&toml::from_str::<toml::Value>(source).unwrap()).is_err());
        }
    }
    #[test]
    fn windows_resource_contains_numeric_and_text_identity() {
        let product = Product {
            name: "Auditory Lab",
            copyright: "Copyright © 2026",
        };
        let resource = windows_resource(product, "1.2.3").unwrap();
        assert!(resource.contains("FILEVERSION 1,2,3,0"));
        assert!(resource.contains("PRODUCTVERSION 1,2,3,0"));
        assert!(resource.contains("VALUE \"ProductName\", \"Auditory Lab\""));
        assert!(resource.contains("VALUE \"LegalCopyright\", \"Copyright © 2026\""));
        assert!(windows_resource(product, "65536.0.0").is_err());
    }
}

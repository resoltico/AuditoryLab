//! The product's code registry is closed, unique and documented.
use std::{collections::BTreeSet, fs, path::Path};

#[expect(
    clippy::expect_used,
    reason = "A malformed authoritative declaration must fail this contract test \
        rather than skip a diagnostic owner"
)]
fn braced(text: &str) -> (&str, &str) {
    let start = text.find('{').expect("declaration group");
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        if ch == '"' {
            quoted = true;
        } else if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                let end = start + offset;
                return (&text[start + 1..end], &text[end + 1..]);
            }
        }
    }
    panic!("unterminated diagnostic declaration")
}
#[expect(
    clippy::expect_used,
    reason = "Missing literal codes in the diagnostic declaration are test \
        failures, not recoverable product input"
)]
fn code_literals(declaration: &str) -> Vec<String> {
    let mut rest = declaration;
    let mut codes = Vec::new();
    while let Some(position) = rest.find("codes {") {
        let (body, after) = braced(&rest[position..]);
        let body = body
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        for arm in body.split("=>").skip(1) {
            codes.push(
                arm.split('"')
                    .nth(1)
                    .expect("literal code after =>")
                    .to_owned(),
            );
        }
        rest = after;
    }
    codes
}
#[expect(
    clippy::expect_used,
    reason = "Unreadable source or an invalid vocabulary declaration must fail \
        registry completeness verification"
)]
fn source_codes(path: &Path, declared: &mut BTreeSet<String>) {
    for entry in fs::read_dir(path).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            source_codes(&path, declared);
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("UTF-8 Rust source");
        for invocation in source.split("diagnostic_codes!").skip(1) {
            let (declaration, _) = braced(invocation);
            for code in code_literals(declaration) {
                assert!(
                    declared.insert(code),
                    "duplicate source code in {}",
                    path.display()
                );
            }
        }
        // The vocabulary macro owns the failure code for each declared kind.
        for invocation in source.split("vocabulary!").skip(1) {
            let (declaration, _) = braced(invocation);
            let kind = declaration.split('"').nth(1).expect("vocabulary kind");
            assert!(
                declared.insert(format!("vocabulary.{kind}")),
                "duplicate vocabulary kind in {}",
                path.display()
            );
        }
    }
}
fn grammar(code: &str) -> bool {
    code.contains('.')
        && code.split('.').all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .bytes()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_')
        })
}
#[test]
fn every_declared_code_is_registered_once_and_matches_the_format_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut registered = BTreeSet::new();
    for owner in auditorylab_core::diagnostic::OWNERS
        .iter()
        .chain(auditorylab_app::diagnostic::OWNERS)
    {
        for code in *owner {
            assert!(grammar(code), "invalid code {code}");
            assert!(
                registered.insert((*code).to_owned()),
                "duplicate registered code {code}"
            );
        }
    }
    let cli = include_str!("../../cli/src/main.rs");
    let cli = cli
        .split_once("// diagnostic-codes:begin")
        .expect("CLI code declaration")
        .1
        .split_once("// diagnostic-codes:end")
        .expect("CLI code declaration end")
        .0;
    let cli_codes = code_literals(cli);
    assert!(
        !cli_codes.is_empty(),
        "CLI registry discovery must not be empty"
    );
    for code in cli_codes {
        assert!(grammar(&code));
        assert!(registered.insert(code));
    }
    let mut declared = BTreeSet::new();
    source_codes(&root.join("crates/core/src"), &mut declared);
    source_codes(&root.join("crates/app/src"), &mut declared);
    declared.extend(code_literals(cli));
    assert_eq!(
        registered, declared,
        "every source owner must participate in the compiled registry"
    );

    let document = include_str!("../../../docs/formats.md");
    let table = document
        .split_once("<!-- diagnostic-codes:begin -->")
        .expect("diagnostic table marker")
        .1
        .split_once("<!-- diagnostic-codes:end -->")
        .expect("diagnostic table end")
        .0;
    let mut documented = BTreeSet::new();
    for line in table.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if matches!(line, "| Code |" | "| --- |") {
            continue;
        }
        let code = line
            .strip_prefix("| `")
            .and_then(|row| row.strip_suffix("` |"))
            .expect("each reference row must contain one code");
        assert!(grammar(code), "invalid documented code {code}");
        assert!(
            documented.insert(code.to_owned()),
            "duplicate documented code {code}"
        );
    }
    assert_eq!(
        registered, documented,
        "the format reference must list exactly the owned codes"
    );
}

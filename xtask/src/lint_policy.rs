//! Validate authored lint exceptions without depending on the host's active cfg.
use crate::{Result, capture};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syn::{
    Attribute, Meta, Token,
    parse::Parser,
    punctuated::Punctuated,
    visit::{self, Visit},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    exception: Vec<Exception>,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Exception {
    id: String,
    file: String,
    scope: String,
    level: String,
    lints: BTreeSet<String>,
    rationale: String,
}
#[derive(Debug, PartialEq, Eq)]
struct Site {
    id: String,
    scope: String,
    level: String,
    lints: BTreeSet<String>,
}
fn metas(tokens: TokenStream) -> Result<Punctuated<Meta, Token![,]>> {
    Ok(Punctuated::<Meta, Token![,]>::parse_terminated.parse2(tokens)?)
}
fn name(meta: &Meta) -> String {
    meta.path().to_token_stream().to_string().replace(' ', "")
}
fn string_value(meta: &Meta) -> Result<String> {
    if let Meta::NameValue(value) = meta
        && let syn::Expr::Lit(expr) = &value.value
        && let syn::Lit::Str(text) = &expr.lit
    {
        return Ok(text.value());
    }
    Err("lint reason must be a literal string".into())
}
fn lower(meta: &Meta) -> bool {
    matches!(name(meta).as_str(), "allow" | "expect" | "warn")
}
fn inspect_meta(meta: &Meta, scope: &str, sites: &mut Vec<Site>) -> Result<()> {
    let level = name(meta);
    if level == "cfg_attr" {
        let Meta::List(list) = meta else {
            return Err("invalid cfg_attr".into());
        };
        let entries = metas(list.tokens.clone())?;
        let mut entries = entries.iter();
        let condition = entries.next().ok_or("cfg_attr condition missing")?;
        let scope = format!("{scope}/cfg_attr:{}", condition.to_token_stream());
        for entry in entries {
            inspect_meta(entry, &scope, sites)?;
        }
    } else if lower(meta) {
        let Meta::List(list) = meta else {
            return Err("invalid lint attribute".into());
        };
        let mut id = None;
        let mut lints = BTreeSet::new();
        for entry in metas(list.tokens.clone())? {
            if entry.path().is_ident("reason") {
                if id.is_some() {
                    return Err("duplicate lint reason".into());
                }
                let reason = string_value(&entry)?;
                let value = reason
                    .strip_prefix("quality:")
                    .ok_or("lint reasons must reference quality:ID in lint-exceptions.toml")?;
                if value.is_empty()
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                {
                    return Err(
                        "exception IDs must use lowercase ASCII words separated by hyphens".into(),
                    );
                }
                id = Some(value.to_owned());
            } else if let Meta::Path(_) = entry {
                let lint = name(&entry);
                if !lints.insert(lint.clone()) {
                    return Err(format!("duplicate lint {lint}").into());
                }
                if matches!(
                    lint.as_str(),
                    "warnings"
                        | "unused"
                        | "unsafe_op_in_unsafe_fn"
                        | "unfulfilled_lint_expectations"
                        | "clippy::all"
                        | "clippy::pedantic"
                        | "clippy::restriction"
                        | "clippy::correctness"
                        | "clippy::suspicious"
                        | "clippy::complexity"
                        | "clippy::perf"
                        | "clippy::style"
                        | "clippy::nursery"
                        | "clippy::cargo"
                        | "clippy::allow_attributes_without_reason"
                ) {
                    return Err(format!(
                        "broad lint group or policy guard {lint} cannot be exempted"
                    )
                    .into());
                }
            } else {
                return Err("unsupported lint attribute argument".into());
            }
        }
        if lints.is_empty() {
            return Err("empty lint exception".into());
        }
        sites.push(Site {
            id: id.ok_or("lint exception needs a quality:ID reason")?,
            scope: scope.to_owned(),
            level,
            lints,
        });
    }
    Ok(())
}

// Macro templates and opaque expressions can hide attributes from syn's normal
// AST visitor. Inspect token trees too, including inactive macro arms, but never
// interpret string/comment contents. Exceptions belong on a named Rust scope.
fn unscoped_attributes(tokens: TokenStream) -> Result<()> {
    let tokens: Vec<_> = tokens.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token, TokenTree::Punct(p) if p.as_char() == '#') {
            let mut next = index + 1;
            if matches!(tokens.get(next), Some(TokenTree::Punct(p)) if p.as_char() == '!') {
                next += 1;
            }
            if let Some(TokenTree::Group(group)) = tokens.get(next)
                && group.delimiter() == proc_macro2::Delimiter::Bracket
            {
                let meta: Meta = syn::parse2(group.stream())
                    .map_err(|e| format!("uninspectable token attribute: {e}"))?;
                let mut sites = Vec::new();
                // Parse recursively even when cfg_attr's condition is false.
                inspect_meta(&meta, "unsupported scope", &mut sites)?;
                if !sites.is_empty() {
                    return Err(
                        "put lint exceptions on a named Rust scope outside opaque tokens".into(),
                    );
                }
            }
        }
        if let TokenTree::Group(group) = token {
            unscoped_attributes(group.stream())?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Source {
    scope: Vec<String>,
    owners: BTreeMap<usize, String>,
    sites: Vec<Site>,
    errors: Vec<String>,
}
impl Source {
    fn enter(&mut self, mut label: String, attrs: &[Attribute]) {
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("cfg")) {
            label.push_str(&format!("/{}", attr.meta.to_token_stream()));
        }
        self.scope.push(label);
        self.own(attrs);
    }
    fn own(&mut self, attrs: &[Attribute]) {
        let scope = self.scope.join("/");
        for attr in attrs {
            let style = if matches!(attr.style, syn::AttrStyle::Inner(_)) {
                "inner"
            } else {
                "outer"
            };
            self.owners.insert(
                std::ptr::from_ref(attr) as usize,
                format!("{scope}/{style}"),
            );
        }
    }
    fn parse(text: &str) -> Result<Vec<Site>> {
        let file = syn::parse_file(text)?;
        let mut source = Self {
            scope: Vec::new(),
            ..Self::default()
        };
        source.enter("file".into(), &file.attrs);
        source.visit_file(&file);
        if !source.errors.is_empty() {
            return Err(source.errors.join("\n").into());
        }
        Ok(source.sites)
    }
}
impl<'ast> Visit<'ast> for Source {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let (label, attrs) = match item {
            syn::Item::Const(v) => (format!("const:{}", v.ident), &v.attrs),
            syn::Item::Enum(v) => (format!("enum:{}", v.ident), &v.attrs),
            syn::Item::Fn(v) => (format!("fn:{}", v.sig.ident), &v.attrs),
            syn::Item::Mod(v) => (format!("mod:{}", v.ident), &v.attrs),
            syn::Item::Struct(v) => (format!("struct:{}", v.ident), &v.attrs),
            syn::Item::Static(v) => (format!("static:{}", v.ident), &v.attrs),
            syn::Item::Type(v) => (format!("type:{}", v.ident), &v.attrs),
            syn::Item::Trait(v) => (format!("trait:{}", v.ident), &v.attrs),
            syn::Item::Impl(v) => (
                format!(
                    "impl:{}:{}",
                    v.trait_
                        .as_ref()
                        .map(|(p, _)| p.to_token_stream().to_string())
                        .unwrap_or_default(),
                    v.self_ty.to_token_stream()
                ),
                &v.attrs,
            ),
            _ => {
                visit::visit_item(self, item);
                return;
            }
        };
        self.enter(label, attrs);
        visit::visit_item(self, item);
        self.scope.pop();
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.enter(format!("fn:{}", item.sig.ident), &item.attrs);
        visit::visit_impl_item_fn(self, item);
        self.scope.pop();
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        self.enter(format!("let:{}", local.pat.to_token_stream()), &local.attrs);
        visit::visit_local(self, local);
        self.scope.pop();
    }
    fn visit_expr_method_call(&mut self, expr: &'ast syn::ExprMethodCall) {
        if matches!(
            expr.method.to_string().as_str(),
            "arg" | "args" | "env" | "envs"
        ) {
            for arg in &expr.args {
                if let Err(error) = flag_literals(arg.to_token_stream(), false) {
                    self.errors.push(error.to_string());
                }
            }
        }
        self.enter(format!("call:{}", expr.method), &expr.attrs);
        visit::visit_expr_method_call(self, expr);
        self.scope.pop();
    }
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        let mut sites = Vec::new();
        let scope = self.owners.get(&(std::ptr::from_ref(attr) as usize));
        if let Err(error) = inspect_meta(
            &attr.meta,
            scope.map_or("unsupported scope", String::as_str),
            &mut sites,
        ) {
            self.errors.push(error.to_string());
        } else if !sites.is_empty() && scope.is_none() {
            self.errors.push(
                "lint exceptions require a supported named item, binding, or method-call scope"
                    .into(),
            );
        } else {
            self.sites.extend(sites);
        }
        // Meta tokens are handled above, not as executable Rust expressions.
    }
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if let syn::Expr::Verbatim(tokens) = expr
            && let Err(error) = unscoped_attributes(tokens.clone())
        {
            self.errors.push(error.to_string());
        }
        visit::visit_expr(self, expr);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Err(error) = unscoped_attributes(mac.tokens.clone()) {
            self.errors.push(error.to_string());
        }
        if let Err(error) = flag_literals(mac.tokens.clone(), true) {
            self.errors.push(error.to_string());
        }
    }
}

fn compare(
    file: &str,
    sites: Vec<Site>,
    registry: &BTreeMap<&str, &Exception>,
    seen: &mut BTreeSet<String>,
) -> Result<()> {
    for site in sites {
        if !seen.insert(site.id.clone()) {
            return Err(format!("duplicate exception ID {}", site.id).into());
        }
        let entry = registry.get(site.id.as_str()).ok_or_else(|| {
            format!(
                "unregistered exception {} at {file}:{}",
                site.id, site.scope
            )
        })?;
        if entry.file != file
            || entry.scope != site.scope
            || entry.level != site.level
            || entry.lints != site.lints
        {
            return Err(format!("exception {} changed file, scope, level or lint set; observed {file}:{} ({}, {:?})", site.id, site.scope, site.level, site.lints).into());
        }
    }
    Ok(())
}
fn manifest(text: &str, workspace: bool) -> Result<()> {
    let value: toml::Value = toml::from_str(text)?;
    if workspace {
        let tables = value
            .get("workspace")
            .and_then(|v| v.get("lints"))
            .and_then(toml::Value::as_table)
            .ok_or("workspace lint policy missing")?;
        for (tool, entries) in tables {
            for (lint, rule) in entries.as_table().ok_or("lint rules must be tables")? {
                let level = rule
                    .as_str()
                    .or_else(|| rule.get("level").and_then(toml::Value::as_str));
                if !matches!(level, Some("deny" | "forbid")) {
                    return Err(format!("workspace lint {tool}::{lint} must deny or forbid").into());
                }
            }
        }
        for (tool, lint) in [
            ("rust", "warnings"),
            ("rust", "unsafe_code"),
            ("rust", "unsafe_op_in_unsafe_fn"),
            ("clippy", "all"),
            ("clippy", "allow_attributes_without_reason"),
        ] {
            if tables.get(tool).and_then(|v| v.get(lint)).is_none() {
                return Err(format!("required workspace lint {tool}::{lint} missing").into());
            }
        }
    } else if value
        .get("lints")
        .and_then(toml::Value::as_table)
        .is_none_or(|v| {
            v.len() != 1 || v.get("workspace").and_then(toml::Value::as_bool) != Some(true)
        })
    {
        return Err("authored package must inherit exactly [lints] workspace = true".into());
    }
    Ok(())
}
fn flags(text: &str) -> Result<()> {
    for token in text.split(|c: char| c.is_whitespace() || "\"'=,[]\u{1f}".contains(c)) {
        if token.starts_with("--cap-lints")
            || token.starts_with("--force-warn")
            || token.starts_with("--allow")
            || token.starts_with("--warn")
            || token.starts_with("-A")
            || token.starts_with("-W")
        {
            return Err(format!("production lint weakening flag {token}").into());
        }
    }
    Ok(())
}
fn flag_literals(tokens: TokenStream, cargo_directives_only: bool) -> Result<()> {
    for token in tokens {
        match token {
            TokenTree::Literal(literal) => {
                if let Ok(syn::Lit::Str(literal)) = syn::parse_str::<syn::Lit>(&literal.to_string())
                {
                    let text = literal.value();
                    if !cargo_directives_only
                        || text.starts_with("cargo:rustc-")
                        || text.starts_with("cargo::rustc-")
                    {
                        flags(&text)?;
                    }
                }
            }
            TokenTree::Group(group) => flag_literals(group.stream(), cargo_directives_only)?,
            _ => {}
        }
    }
    Ok(())
}
fn walk(root: &Path, at: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(at)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        if relative.components().count() == 1
            && matches!(
                entry.file_name().to_str(),
                Some("target" | ".git" | "third_party")
            )
        {
            continue;
        }
        if entry.file_type()?.is_symlink() {
            return Err(format!("authored tree contains a symlink: {}", relative.display()).into());
        }
        if path.is_dir() {
            walk(root, &path, paths)?;
        } else {
            paths.push(path);
        }
    }
    Ok(())
}
pub(super) fn check(root: &Path) -> Result<()> {
    for (key, value) in std::env::vars_os() {
        let key = key.to_string_lossy();
        if key.contains("RUSTFLAGS") || key.contains("RUSTDOCFLAGS") {
            flags(&value.to_string_lossy()).map_err(|e| format!("{key}: {e}"))?;
        }
    }
    let registry: Registry =
        toml::from_str(&fs::read_to_string(root.join("lint-exceptions.toml"))?)?;
    let mut entries = BTreeMap::new();
    for entry in &registry.exception {
        if entry.rationale.trim().is_empty() || entry.lints.is_empty() {
            return Err(format!(
                "exception {} needs a concrete rationale and exact lints",
                entry.id
            )
            .into());
        }
        if entries.insert(entry.id.as_str(), entry).is_some() {
            return Err(format!("duplicate registry ID {}", entry.id).into());
        }
    }
    let root_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    manifest(&root_manifest, true)?;
    if toml::from_str::<toml::Value>(&root_manifest)?
        .get("package")
        .is_some()
    {
        manifest(&root_manifest, false)?;
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&capture(Command::new("cargo").current_dir(root).args([
            "metadata",
            "--no-deps",
            "--locked",
            "--format-version",
            "1",
        ]))?)?;
    let package_manifests: BTreeSet<PathBuf> = metadata["packages"]
        .as_array()
        .ok_or("workspace packages missing")?
        .iter()
        .map(|v| -> Result<PathBuf> {
            let path = v["manifest_path"]
                .as_str()
                .ok_or("package manifest missing")?;
            // Match discovered files in the same namespace, including Windows
            // extended-length prefixes and symlink-resolved workspace roots.
            Ok(fs::canonicalize(path)?)
        })
        .collect::<Result<_>>()?;
    let mut paths = Vec::new();
    walk(root, root, &mut paths)?;
    paths.sort();
    let mut seen = BTreeSet::new();
    let mut sources = 0;
    for path in paths {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .ok_or("source path is not UTF-8")?
            .replace('\\', "/");
        if path.extension().is_some_and(|ext| ext == "rs") {
            let sites = Source::parse(&fs::read_to_string(&path)?)
                .map_err(|e| format!("{relative}: {e}"))?;
            compare(&relative, sites, &entries, &mut seen)?;
            sources += 1;
        }
        if path.file_name().is_some_and(|v| v == "Cargo.toml") && relative != "Cargo.toml" {
            if !package_manifests.contains(&fs::canonicalize(&path)?) {
                return Err(format!(
                    "unregistered authored package {relative}; add it to the shared workspace"
                )
                .into());
            }
            manifest(&fs::read_to_string(&path)?, false).map_err(|e| format!("{relative}: {e}"))?;
        }
        if relative.starts_with(".cargo/")
            || relative.starts_with(".github/workflows/")
            || path.file_name().is_some_and(|v| v == "Cargo.toml")
        {
            flags(&fs::read_to_string(&path)?).map_err(|e| format!("{relative}: {e}"))?;
        }
    }
    for entry in &registry.exception {
        if !seen.contains(&entry.id) {
            return Err(format!("stale exception {}", entry.id).into());
        }
    }
    println!(
        "Lint policy checked {sources} authored Rust files and {} exact exceptions",
        seen.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"#[expect(dead_code, reason = "quality:fixture")] fn helper() {}"#;
    const POLICY: &str = r#"
[workspace]
members = ["engine"]
resolver = "3"
[workspace.lints.rust]
warnings = "deny"
unsafe_code = "deny"
unsafe_op_in_unsafe_fn = "deny"
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
allow_attributes_without_reason = "deny"
"#;
    fn entry() -> Exception {
        Exception {
            id: "fixture".into(),
            file: "engine/src/lib.rs".into(),
            scope: "file/fn:helper/outer".into(),
            level: "expect".into(),
            lints: BTreeSet::from(["dead_code".into()]),
            rationale: "Fixture used to exercise exact source/registry matching".into(),
        }
    }
    fn matches(text: &str, entry: &Exception) -> Result<()> {
        compare(
            "engine/src/lib.rs",
            Source::parse(text)?,
            &BTreeMap::from([(entry.id.as_str(), entry)]),
            &mut BTreeSet::new(),
        )
    }
    #[test]
    fn exact_scope_lints_level_and_file_are_required() {
        assert!(matches(VALID, &entry()).is_ok());
        for changed in [
            VALID.replace("helper", "unreviewed"),
            VALID.replace("dead_code,", "dead_code, unused_imports,"),
            VALID.replace("expect(", "allow("),
            VALID.replace("fn helper() {}", "mod helper {}"),
            VALID.replace("quality:fixture", "quality:other"),
        ] {
            assert!(matches(&changed, &entry()).is_err(), "{changed}");
        }
        let mut wrong_file = entry();
        wrong_file.file = "another.rs".into();
        assert!(matches(VALID, &wrong_file).is_err());
        assert!(matches(&format!("{VALID} mod nested {{ {VALID} }}"), &entry()).is_err());
    }
    #[test]
    fn cfg_and_inner_attributes_do_not_escape_review() {
        let inner = r#"#![allow(dead_code, reason = "quality:fixture")] fn helper() {}"#;
        assert_eq!(Source::parse(inner).unwrap()[0].scope, "file/inner");
        assert!(matches(inner, &entry()).is_err());
        let conditional = r#"#[cfg_attr(any(), cfg_attr(all(), allow(dead_code, reason = "quality:fixture")))] fn helper() {}"#;
        let sites = Source::parse(conditional).unwrap();
        assert_eq!(sites.len(), 1);
        assert!(sites[0].scope.contains("cfg_attr:any ()/cfg_attr:all ()"));
        for text in [
            format!("#[cfg(any())] {VALID}"),
            format!("#[cfg(any())] mod nested {{ {VALID} }}"),
        ] {
            assert!(matches(&text, &entry()).is_err());
        }
    }
    #[test]
    fn macros_are_inspected_without_reading_comments_or_string_contents() {
        for text in [
            r#"macro_rules! hidden { () => { #[allow(dead_code)] fn hidden() {} }; }"#,
            r#"macro_rules! hidden { () => { #![allow(dead_code, reason = "quality:fixture")] }; }"#,
            r#"macro_rules! hidden { () => { #[cfg_attr(any(), allow(dead_code))] fn hidden() {} }; }"#,
            r#"macro_rules! hidden { ($attr:meta) => { #[$attr] fn hidden() {} }; }"#,
        ] {
            assert!(Source::parse(text).is_err(), "{text}");
        }
        assert!(
            Source::parse(
                r##"// #[allow(dead_code)]
            const TEXT: &str = "#[allow(dead_code)]";
            say!("#[allow(dead_code)]");"##
            )
            .unwrap()
            .is_empty()
        );
        assert!(Source::parse("fn invalid(").is_err());
    }
    #[test]
    fn reasonless_empty_broad_and_unsupported_exceptions_reject() {
        for text in [
            "#[allow(dead_code)] fn helper() {}",
            r#"#[allow(dead_code, reason = "because old code")] fn helper() {}"#,
            r#"#[allow(reason = "quality:fixture")] fn helper() {}"#,
            r#"#[allow(warnings, reason = "quality:fixture")] fn helper() {}"#,
            r#"#[allow(clippy::all, reason = "quality:fixture")] fn helper() {}"#,
            r#"#[allow(clippy::style, reason = "quality:fixture")] fn helper() {}"#,
            r#"#[expect(unfulfilled_lint_expectations, reason = "quality:fixture")] fn helper() {}"#,
            r#"#[allow(unsafe_op_in_unsafe_fn, reason = "quality:fixture")] fn helper() {}"#,
            r#"#[allow(dead_code, dead_code, reason = "quality:fixture")] fn helper() {}"#,
            r#"struct S { #[allow(dead_code, reason = "quality:fixture")] field: u8 }"#,
        ] {
            assert!(Source::parse(text).is_err(), "{text}");
        }
    }
    #[test]
    fn statement_expression_attributes_cannot_escape_review() {
        for text in [
            r#"fn helper() { #[allow(dead_code, reason = "quality:fixture")] become helper(); }"#,
            r#"#[cfg(any())] fn helper() { #[cfg_attr(any(), allow(dead_code, reason = "quality:fixture"))] become helper(); }"#,
            r#"fn helper() { #[allow(dead_code, reason = "quality:fixture")] return; }"#,
        ] {
            let error = Source::parse(text).unwrap_err().to_string();
            assert!(
                error.contains("named Rust scope") || error.contains("supported named item"),
                "{text}: {error}"
            );
        }
        assert!(Source::parse("fn helper() { become helper(); }").is_ok());
        assert!(
            Source::parse(r##"fn helper() { become helper("#[allow(dead_code)]"); }"##).is_ok()
        );
    }
    #[test]
    fn workspace_rules_and_package_inheritance_are_not_optional() {
        assert!(manifest(POLICY, true).is_ok());
        for text in [
            POLICY.replace("unsafe_code = \"deny\"", "unsafe_code = \"allow\""),
            POLICY.replace("warnings = \"deny\"", "warnings = \"warn\""),
            POLICY.replace("unsafe_op_in_unsafe_fn = \"deny\"", ""),
            "[workspace]".into(),
        ] {
            assert!(manifest(&text, true).is_err(), "{text}");
        }
        assert!(manifest("[lints]\nworkspace = true", false).is_ok());
        for text in [
            "",
            "[lints]\nworkspace=false",
            "[lints.rust]\nwarnings='deny'",
            "[lints]\nworkspace=true\n[lints.clippy]\nall='allow'",
        ] {
            assert!(manifest(text, false).is_err(), "{text}");
        }
    }
    #[test]
    fn production_flags_and_build_directives_cannot_silence_lints() {
        assert!(flags("rustflags = [\"-C\", \"target-feature=+crt-static\"]").is_ok());
        for text in [
            "--cap-lints=allow",
            "--force-warn warnings",
            "-Awarnings",
            "-A warnings",
            "--allow=dead_code",
            "--warn warnings",
            "-Wwarnings",
            "-Copt-level=2\u{1f}--cap-lints\u{1f}warn",
        ] {
            assert!(flags(text).is_err(), "{text}");
        }
        for text in [
            r#"fn build() { Command::new("cargo").args(["rustc", "--", "-Awarnings"]); }"#,
            r#"fn build() { Command::new("cargo").env("RUSTDOCFLAGS", "--cap-lints=allow"); }"#,
            r#"fn main() { println!("cargo::rustc-flags=-Awarnings"); }"#,
        ] {
            assert!(Source::parse(text).is_err(), "{text}");
        }
    }
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("engine/src")).unwrap();
        fs::write(dir.path().join("Cargo.toml"), POLICY).unwrap();
        fs::write(
            dir.path().join("engine/Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[lints]\nworkspace=true",
        )
        .unwrap();
        fs::write(dir.path().join("engine/src/lib.rs"), "").unwrap();
        fs::write(dir.path().join("lint-exceptions.toml"), "exception=[]").unwrap();
        dir
    }
    #[test]
    fn filesystem_scan_includes_untracked_sources_and_rejects_unregistered_packages() {
        let dir = fixture();
        assert!(check(dir.path()).is_ok());
        // No Git repository or tracked file list exists in this fixture.
        let extra = dir.path().join("engine/src/untracked.rs");
        fs::write(&extra, VALID).unwrap();
        assert!(
            check(dir.path())
                .unwrap_err()
                .to_string()
                .contains("unregistered exception")
        );
        fs::remove_file(extra).unwrap();
        fs::create_dir(dir.path().join("escape")).unwrap();
        fs::write(
            dir.path().join("escape/Cargo.toml"),
            "[package]\nname='escape'\nversion='0.1.0'",
        )
        .unwrap();
        assert!(
            check(dir.path())
                .unwrap_err()
                .to_string()
                .contains("unregistered authored package")
        );
    }
    #[test]
    fn registry_rejects_stale_duplicate_and_reasonless_records() {
        let dir = fixture();
        let record = r#"[[exception]]
id = "fixture"
file = "engine/src/lib.rs"
scope = "file/fn:helper/outer"
level = "expect"
lints = ["dead_code"]
rationale = "Uncalled fixture helper used to verify the checker"
"#;
        let registry = dir.path().join("lint-exceptions.toml");
        fs::write(&registry, record).unwrap();
        assert!(
            check(dir.path())
                .unwrap_err()
                .to_string()
                .contains("stale exception")
        );
        fs::write(dir.path().join("engine/src/lib.rs"), VALID).unwrap();
        assert!(check(dir.path()).is_ok());
        fs::write(&registry, format!("{record}\n{record}")).unwrap();
        assert!(
            check(dir.path())
                .unwrap_err()
                .to_string()
                .contains("duplicate registry")
        );
        fs::write(
            &registry,
            record.replace("Uncalled fixture helper used to verify the checker", " "),
        )
        .unwrap();
        assert!(
            check(dir.path())
                .unwrap_err()
                .to_string()
                .contains("concrete rationale")
        );
    }
}

// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Narrow generated-code/compiler/resource exceptions for the selected GUI.
use quote::ToTokens;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use syn::{Expr, Meta, Stmt};

fn system_font_config(config: &toml::Value) -> Result<(), String> {
    let expected: toml::Value =
        toml::from_str("rustc-link-lib=['dylib=freetype']").map_err(|error| error.to_string())?;
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        if config
            .get("target")
            .and_then(|targets| targets.get(target))
            .and_then(|target| target.get("freetype"))
            != Some(&expected)
        {
            return Err(format!(
                "{target} must override the FreeType build script with exactly dylib=freetype; bundled native compilation is prohibited"
            ));
        }
    }
    if config.get("include").is_some() {
        return Err(
            "root Cargo configuration must not include an alternate font build policy".into(),
        );
    }
    Ok(())
}
pub(super) fn system_font_build_policy(root: &Path) -> Result<(), String> {
    // Cargo prefers the extensionless file when both exist. Inspecting only the
    // TOML file would then certify an override that Cargo never reads.
    if root.join(".cargo/config").exists() {
        return Err(
            "extensionless .cargo/config shadows the supported system-only font policy".into(),
        );
    }
    let path = root.join(".cargo/config.toml");
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    system_font_config(&config)
}

#[test]
fn system_only_font_override_cannot_enable_native_fallback_or_be_shadowed() {
    let valid = "[target.x86_64-unknown-linux-gnu.freetype]\nrustc-link-lib=['dylib=freetype']\n[target.aarch64-unknown-linux-gnu.freetype]\nrustc-link-lib=['dylib=freetype']\n";
    system_font_config(&toml::from_str(valid).unwrap()).unwrap();
    for invalid in [
        valid.replace(
            "[target.aarch64-unknown-linux-gnu.freetype]\nrustc-link-lib=['dylib=freetype']",
            "",
        ),
        valid.replace("dylib=freetype", "static=freetype"),
        valid.replace("dylib=freetype", "freetype"),
        valid.replace(
            "rustc-link-lib=['dylib=freetype']",
            "rustc-link-lib=['dylib=freetype','static=freetype']",
        ),
        format!("include=['alternate.toml']\n{valid}"),
        format!("{valid}rustc-link-search=['/alternate']\n"),
    ] {
        assert!(
            system_font_config(&toml::from_str(&invalid).unwrap()).is_err(),
            "accepted {invalid}"
        );
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".cargo")).unwrap();
    std::fs::write(root.path().join(".cargo/config.toml"), valid).unwrap();
    system_font_build_policy(root.path()).unwrap();
    std::fs::write(root.path().join(".cargo/config"), "# ignored TOML sibling").unwrap();
    assert!(system_font_build_policy(root.path()).is_err());
}

pub(super) const COMPILER_INPUTS: [&str; 17] = [
    "SLINT_STYLE",
    "SLINT_FONT_SIZES",
    "SLINT_SCALE_FACTOR",
    "SLINT_ASSET_SECTION",
    "SLINT_EMBED_RESOURCES",
    "SLINT_EMBED_TEXTURES",
    "SLINT_EMIT_DEBUG_INFO",
    "SLINT_LIVE_PREVIEW",
    "SLINT_BUNDLE_TRANSLATIONS",
    "SLINT_INLINING",
    "SLINT_ENABLE_EXPERIMENTAL_FEATURES",
    "SLINT_CPP_NAMESPACE",
    "SLINT_COMPILER_DENY_WARNINGS",
    "SLINT_INCLUDE_GENERATED",
    "DEP_MCU_BOARD_SUPPORT_BOARD_CONFIG_PATH",
    "DEP_MCU_BOARD_SUPPORT_MCU_BOARD_CONFIG_PATH",
    "DEP_MCU_BOARD_SUPPORT_MCU_EMBED_TEXTURES",
];
fn spelling(value: impl ToTokens) -> String {
    value.into_token_stream().to_string()
}
pub(super) fn native_bridge(node: &syn::ExprMethodCall) -> bool {
    if !node.attrs.is_empty() || node.turbofish.is_some() || node.args.len() != 1 {
        return false;
    }
    let expected = match node.method.to_string().as_str() {
        "with_winit_window" => quote::quote!(|window| window.has_focus()),
        "on_winit_window_event" => quote::quote!(move |_, event| {
            if let slint::winit_030::winit::event::WindowEvent::Occluded(occluded) = event {
                reported_occluded.set(*occluded);
            }
            slint::winit_030::EventResult::Propagate
        }),
        _ => return false,
    };
    // Normalize parsed syntax, not source bytes. This deliberately narrow
    // exception has no retained native handle, arbitrary native operation,
    // event consumption, or controller/window borrow in its observer body.
    spelling(&node.args[0]) == spelling(expected)
}
pub(super) fn native_trait_import(node: &syn::ItemUse) -> bool {
    node.attrs.is_empty()
        && matches!(node.vis, syn::Visibility::Inherited)
        && spelling(&node.tree) == spelling(quote::quote!(slint::winit_030::WinitWindowAccessor))
}
fn path(expr: &Expr, expected: &str) -> bool {
    matches!(expr,Expr::Path(value) if spelling(&value.path)==expected)
}
fn literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(value) => match &value.lit {
            syn::Lit::Str(text) => Some(text.value()),
            _ => None,
        },
        _ => None,
    }
}
fn expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(value, _) => Some(value),
        _ => None,
    }
}
fn println_only(stmt: &Stmt, format: &str) -> bool {
    match stmt {
        Stmt::Macro(value) => {
            value.mac.path.is_ident("println")
                && syn::parse2::<syn::LitStr>(value.mac.tokens.clone())
                    .is_ok_and(|text| text.value() == format)
        }
        _ => false,
    }
}
fn metas(attribute: &syn::Attribute) -> Option<Vec<Meta>> {
    attribute
        .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .ok()
        .map(|items| items.into_iter().collect())
}
pub(super) fn generated_module(module: &syn::ItemMod) -> bool {
    if module.unsafety.is_some()
        || !matches!(module.vis, syn::Visibility::Inherited)
        || module.attrs.len() != 1
        || module.attrs[0]
            .path()
            .get_ident()
            .is_none_or(|id| id != "allow")
    {
        return false;
    }
    let Some(items) = metas(&module.attrs[0]) else {
        return false;
    };
    let mut allowed = HashSet::new();
    let mut reason = false;
    for item in items {
        match item {
            Meta::Path(path) => {
                allowed.insert(spelling(path));
            }
            Meta::NameValue(value) if value.path.is_ident("reason") => {
                reason = literal(&value.value).is_some_and(|text| !text.trim().is_empty());
            }
            _ => return false,
        }
    }
    if !reason
        || allowed
            != HashSet::from([
                "unsafe_code".into(),
                "unreachable_pub".into(),
                "clippy :: unwrap_used".into(),
                "clippy :: todo".into(),
            ])
    {
        return false;
    }
    let Some((_, items)) = &module.content else {
        return false;
    };
    if items.len() != 1 {
        return false;
    }
    matches!(&items[0],syn::Item::Macro(item) if item.attrs.is_empty()&&item.ident.is_none()&&item.semi_token.is_some()&&spelling(&item.mac.path)=="slint :: include_modules"&&matches!(item.mac.delimiter,syn::MacroDelimiter::Paren(_))&&item.mac.tokens.is_empty())
}
pub(super) fn lint_weakening(meta: &Meta) -> bool {
    use syn::parse::Parser;
    let nested: Vec<Meta> = if let Meta::List(list) = meta {
        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated
            .parse2(list.tokens.clone())
            .map(|items| items.into_iter().collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let lowers = matches!(meta.path().get_ident(),Some(id)if id=="allow"||id=="expect")
        && nested.iter().any(|item| {
            matches!(
                spelling(item.path()).as_str(),
                "unsafe_code"
                    | "unreachable_pub"
                    | "warnings"
                    | "clippy :: all"
                    | "clippy :: unwrap_used"
                    | "clippy :: todo"
            )
        });
    lowers || meta.path().is_ident("unsafe") || nested.iter().any(lint_weakening)
}
fn check_build_source(source: &str) -> Result<(), String> {
    let fail = || {
        "GUI build.rs must only track/reject reviewed compiler overrides and compile ui/main.slint with explicit EmbedFiles".to_string()
    };
    let tree = syn::parse_file(source).map_err(|error| error.to_string())?;
    if tree
        .attrs
        .iter()
        .any(|attribute| !attribute.path().is_ident("doc"))
    {
        return Err(fail());
    }
    if tree.items.len() != 1 {
        return Err(fail());
    }
    let syn::Item::Fn(main) = &tree.items[0] else {
        return Err(fail());
    };
    if main.sig.ident != "main"
        || !main.attrs.is_empty()
        || main.sig.asyncness.is_some()
        || main.sig.unsafety.is_some()
        || main.sig.abi.is_some()
        || !main.sig.inputs.is_empty()
        || !main.sig.generics.params.is_empty()
        || main.block.stmts.len() != 5
    {
        return Err(fail());
    }
    struct NoAttributes(bool);
    impl<'a> syn::visit::Visit<'a> for NoAttributes {
        fn visit_attribute(&mut self, _: &'a syn::Attribute) {
            self.0 = false;
        }
    }
    let mut no_attributes = NoAttributes(true);
    syn::visit::Visit::visit_block(&mut no_attributes, &main.block);
    if !no_attributes.0 {
        return Err(fail());
    }
    let statements = &main.block.stmts;
    let Some(Expr::ForLoop(track)) = expr(&statements[0]) else {
        return Err(fail());
    };
    if spelling(&track.pat) != "name"
        || track.label.is_some()
        || !track.attrs.is_empty()
        || track.body.stmts.len() != 1
        || !println_only(&track.body.stmts[0], "cargo:rerun-if-env-changed={name}")
    {
        return Err(fail());
    }
    let Expr::Array(names) = track.expr.as_ref() else {
        return Err(fail());
    };
    let names: Option<Vec<_>> = names.elems.iter().map(literal).collect();
    if names
        .as_ref()
        .map(|names| names.iter().map(String::as_str).collect::<HashSet<_>>())
        != Some(COMPILER_INPUTS.into_iter().collect())
        || names
            .as_ref()
            .is_none_or(|names| names.len() != COMPILER_INPUTS.len())
    {
        return Err(fail());
    }
    let Some(Expr::ForLoop(reject)) = expr(&statements[1]) else {
        return Err(fail());
    };
    if spelling(&reject.pat) != "(name , value)"
        || spelling(&reject.expr) != "std :: env :: vars_os ()"
        || reject.label.is_some()
        || !reject.attrs.is_empty()
        || reject.body.stmts.len() != 2
    {
        return Err(fail());
    }
    let Stmt::Local(local) = &reject.body.stmts[0] else {
        return Err(fail());
    };
    if spelling(&local.pat) != "spelling"
        || local.init.as_ref().is_none_or(|init| {
            spelling(&init.expr) != "name . to_string_lossy ()" || init.diverge.is_some()
        })
        || !local.attrs.is_empty()
    {
        return Err(fail());
    }
    let Some(Expr::If(prefix)) = expr(&reject.body.stmts[1]) else {
        return Err(fail());
    };
    if spelling(&prefix.cond)
        != "spelling . starts_with (\"SLINT_\") || spelling . starts_with (\"DEP_MCU_BOARD_SUPPORT_\")"
        || prefix.else_branch.is_some()
        || !prefix.attrs.is_empty()
        || prefix.then_branch.stmts.len() != 2
        || !println_only(
            &prefix.then_branch.stmts[0],
            "cargo:rerun-if-env-changed={spelling}",
        )
    {
        return Err(fail());
    }
    let Some(Expr::If(unsupported)) = expr(&prefix.then_branch.stmts[1]) else {
        return Err(fail());
    };
    if spelling(&unsupported.cond) != "name != \"SLINT_COMPILER_DENY_WARNINGS\" || value != \"1\""
        || unsupported.else_branch.is_some()
        || !unsupported.attrs.is_empty()
        || unsupported.then_branch.stmts.len() != 1
    {
        return Err(fail());
    }
    let Some(Expr::Return(returned)) = expr(&unsupported.then_branch.stmts[0]) else {
        return Err(fail());
    };
    let Some(Expr::Call(error)) = returned.expr.as_deref() else {
        return Err(fail());
    };
    if !path(&error.func, "Err") || error.args.len() != 1 {
        return Err(fail());
    }
    let Expr::MethodCall(convert) = &error.args[0] else {
        return Err(fail());
    };
    if convert.method != "into" || !convert.args.is_empty() {
        return Err(fail());
    }
    let Expr::Macro(message) = convert.receiver.as_ref() else {
        return Err(fail());
    };
    if !message.mac.path.is_ident("format")
        || syn::parse2::<syn::LitStr>(message.mac.tokens.clone()).is_err()
    {
        return Err(fail());
    }
    let Stmt::Local(configuration) = &statements[2] else {
        return Err(fail());
    };
    if spelling(&configuration.pat) != "configuration" || !configuration.attrs.is_empty() {
        return Err(fail());
    }
    let Some(initializer) = &configuration.init else {
        return Err(fail());
    };
    if initializer.diverge.is_some() {
        return Err(fail());
    }
    let Expr::MethodCall(embed) = initializer.expr.as_ref() else {
        return Err(fail());
    };
    if embed.method != "embed_resources"
        || embed.args.len() != 1
        || !path(
            &embed.args[0],
            "slint_build :: EmbedResourcesKind :: EmbedFiles",
        )
        || spelling(&embed.receiver) != "slint_build :: CompilerConfiguration :: new ()"
    {
        return Err(fail());
    }
    let Some(Expr::Try(compiled)) = expr(&statements[3]) else {
        return Err(fail());
    };
    let Expr::Call(call) = compiled.expr.as_ref() else {
        return Err(fail());
    };
    if !path(&call.func, "slint_build :: compile_with_config")
        || call.args.len() != 2
        || literal(&call.args[0]).as_deref() != Some("ui/main.slint")
        || !path(&call.args[1], "configuration")
    {
        return Err(fail());
    }
    if spelling(expr(&statements[4]).ok_or_else(fail)?) != "Ok (())" {
        return Err(fail());
    }
    Ok(())
}
pub(super) fn build_script(package: &serde_json::Value) -> Result<(), String> {
    let root = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let targets: Vec<_> = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| {
            target["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|kind| kind == "custom-build")
        })
        .collect();
    if targets.len() != 1 {
        return Err("auditory-gui requires exactly one reviewed build.rs target".into());
    }
    let actual = Path::new(targets[0]["src_path"].as_str().unwrap())
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if actual != root.join("build.rs") {
        return Err("GUI compiler entry must be contained build.rs".into());
    }
    check_build_source(&std::fs::read_to_string(&actual).map_err(|error| error.to_string())?)
        .map_err(|error| format!("{}: {error}", actual.display()))
}
pub(super) fn lints(workspace: &toml::Value, manifest: &toml::Value) -> Result<(), String> {
    let mut expected = workspace["workspace"]["lints"].clone();
    expected["rust"]["unsafe_code"] = toml::Value::String("deny".into());
    if manifest.get("lints") != Some(&expected) {
        return Err("GUI lint table must match workspace rules exactly, except generated-only unsafe_code deny instead of forbid".into());
    }
    Ok(())
}
fn resolved(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let actual = path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !actual.starts_with(root) {
        return Err(format!(
            "{} escapes declared GUI root {}",
            path.display(),
            root.display()
        ));
    }
    Ok(actual)
}
fn markup(
    ui: &Path,
    resources: &Path,
    file: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    use i_slint_compiler::parser::SyntaxKind;
    let actual = resolved(ui, file)?;
    if !seen.insert(actual.clone()) {
        return Ok(());
    }
    let source = std::fs::read_to_string(&actual).map_err(|error| error.to_string())?;
    let mut diagnostics = i_slint_compiler::diagnostics::BuildDiagnostics::default();
    let tree = i_slint_compiler::parser::parse(source, Some(&actual), &mut diagnostics);
    if diagnostics.has_errors() {
        return Err(format!(
            "{}: invalid or unsupported Slint source syntax",
            actual.display()
        ));
    }
    for node in tree.descendants() {
        if node.kind() == SyntaxKind::AtRustAttr {
            return Err(format!(
                "{}: handwritten Rust attributes in Slint markup are unsupported at the generated-only boundary",
                actual.display()
            ));
        }
        if !matches!(
            node.kind(),
            SyntaxKind::ImportSpecifier | SyntaxKind::ExportModule | SyntaxKind::AtImageUrl
        ) {
            continue;
        }
        let strings: Vec<_> = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .filter(|token| token.kind() == SyntaxKind::StringLiteral)
            .collect();
        if strings.len() != 1 {
            return Err(format!(
                "{}: import/resource requires one literal contained path",
                actual.display()
            ));
        }
        let reference = i_slint_compiler::literals::unescape_string(strings[0].text())
            .ok_or_else(|| "unsupported Slint path escape".to_string())?;
        let path = actual.parent().unwrap().join(reference.as_str());
        if reference == "std-widgets.slint" && node.kind() == SyntaxKind::ImportSpecifier {
            match std::fs::symlink_metadata(&path){Err(error) if error.kind()==std::io::ErrorKind::NotFound=>continue,Ok(_)=>return Err("local std-widgets.slint shadows the pinned framework library; rename the local import".into()),Err(error)=>return Err(error.to_string())}
        }
        if node.kind() != SyntaxKind::AtImageUrl
            && path
                .extension()
                .is_some_and(|extension| extension == "slint")
        {
            markup(ui, resources, &path, seen)?;
        } else {
            resolved(resources, &path)?;
        }
    }
    Ok(())
}
pub(super) fn markup_tree(root: &Path) -> Result<(), String> {
    let ui = root
        .join("ui")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !ui.starts_with(root) {
        return Err("GUI ui directory escapes package".into());
    }
    let resources = root.join("resources");
    let resources = if resources.exists() {
        let actual = resources
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !actual.starts_with(root) {
            return Err("GUI resource directory escapes package".into());
        }
        actual
    } else {
        resources
    };
    markup(&ui, &resources, &ui.join("main.slint"), &mut HashSet::new())
}
pub(super) fn compiler_record(root: &Path, record: &str) -> Result<bool, String> {
    compiler_record_with_framework(root, record, None)
}
fn compiler_record_with_framework(
    root: &Path,
    record: &str,
    framework: Option<&Path>,
) -> Result<bool, String> {
    let ui = root
        .join("ui")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let main = ui
        .join("main.slint")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let resources = root.join("resources");
    let resources = if resources.exists() {
        resources
            .canonicalize()
            .map_err(|error| error.to_string())?
    } else {
        resources
    };
    let mut saw_main = false;
    for line in record.lines() {
        let Some(path) = line
            .strip_prefix("cargo:rerun-if-changed=")
            .or_else(|| line.strip_prefix("cargo::rerun-if-changed="))
        else {
            continue;
        };
        let requested = Path::new(path);
        if requested.starts_with("builtin:/") {
            if !i_slint_compiler::fileaccess::load_file(requested)
                .is_some_and(|file| file.is_builtin())
            {
                return Err(format!(
                    "unrecognized pinned Slint builtin dependency {path}"
                ));
            }
            continue;
        }
        let requested = if requested.is_absolute() {
            requested.to_owned()
        } else {
            root.join(requested)
        };
        let actual = requested
            .canonicalize()
            .map_err(|error| format!("compiler dependency {}: {error}", requested.display()))?;
        if actual == main {
            saw_main = true;
        }
        if !actual.starts_with(&ui)
            && !actual.starts_with(&resources)
            && !framework.is_some_and(|root| actual.starts_with(root))
        {
            return Err(format!(
                "compiler-reported dependency {} escapes GUI UI/resources",
                actual.display()
            ));
        }
    }
    Ok(saw_main)
}
pub(super) fn compiler_inputs(
    root: &Path,
    target: &Path,
    framework: Option<&Path>,
) -> Result<(), String> {
    let mut profiles = vec![target.join("debug/build"), target.join("release/build")];
    if target.exists() {
        for entry in std::fs::read_dir(target).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                profiles.extend([path.join("debug/build"), path.join("release/build")]);
            }
        }
    }
    let mut witnessed = false;
    for profile in profiles {
        if !profile.exists() {
            continue;
        }
        for entry in std::fs::read_dir(profile).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with("auditory-gui-")
            {
                continue;
            }
            let output = entry.path().join("output");
            if output.is_file() {
                let record = std::fs::read_to_string(&output).map_err(|error| error.to_string())?;
                witnessed |= compiler_record_with_framework(root, &record, framework)
                    .map_err(|error| format!("{}: {error}", output.display()))?;
            }
        }
    }
    if !witnessed {
        return Err("no GUI compiler dependency witness; run cargo check -p auditory-gui, then rerun workspace tests".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn build() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../auditory-gui/build.rs"),
        )
        .unwrap()
    }
    #[test]
    fn only_reviewed_build_effects_configuration_and_environment_rejection_pass() {
        let valid = build();
        check_build_source(&valid).unwrap();
        for changed in [
            valid.replace("fn main()", "fn extra() {}\nfn main()"),
            valid.replace("let configuration =", "std::process::Command::new(\"sh\").status()?; let configuration ="),
            valid.replace("let configuration =", "std::env::set_var(\"SLINT_STYLE\", \"external\"); let configuration ="),
            valid.replace("EmbedFiles", "Nothing"),
            valid.replace("ui/main.slint", "../outside.slint"),
            valid.replace("compile_with_config(\"ui/main.slint\", configuration)?", "compile_with_config(\"ui/main.slint\", configuration)"),
            valid.replace("name != \"SLINT_COMPILER_DENY_WARNINGS\" || value != \"1\"", "false"),
            valid.replace("spelling.starts_with(\"SLINT_\") || spelling.starts_with(\"DEP_MCU_BOARD_SUPPORT_\")", "false"),
            valid.replace("\"SLINT_BUNDLE_TRANSLATIONS\",", ""),
            valid.replace("cargo:rerun-if-env-changed={name}", "ignored"),
            valid.replace("slint_build::compile_with_config(","#[cfg(any())] slint_build::compile_with_config("),
        ] {assert_ne!(valid,changed,"negative fixture must actually mutate source");assert!(check_build_source(&changed).is_err(),"accepted changed GUI compiler source");}
    }
    #[test]
    fn lint_copy_cannot_drop_or_weaken_any_workspace_rule() {
        let workspace: toml::Value = toml::from_str(
            &std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut manifest: toml::Value = toml::from_str(
            &std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../auditory-gui/Cargo.toml"),
            )
            .unwrap(),
        )
        .unwrap();
        lints(&workspace, &manifest).unwrap();
        for (group, name) in [
            ("rust", "unsafe_code"),
            ("rust", "unreachable_pub"),
            ("rust", "missing_docs"),
            ("clippy", "unwrap_used"),
            ("clippy", "all"),
            ("rustdoc", "broken_intra_doc_links"),
        ] {
            let saved = manifest["lints"][group][name].clone();
            manifest["lints"][group][name] = toml::Value::String("allow".into());
            assert!(
                lints(&workspace, &manifest).is_err(),
                "accepted weakened {group}.{name}"
            );
            manifest["lints"][group][name] = saved;
        }
        manifest["lints"]["rust"]
            .as_table_mut()
            .unwrap()
            .remove("warnings");
        assert!(lints(&workspace, &manifest).is_err());
    }
    #[test]
    fn markup_parser_checks_nested_imports_exports_assets_and_escaped_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("ui")).unwrap();
        std::fs::create_dir(root.join("resources")).unwrap();
        let main = root.join("ui/main.slint");
        std::fs::write(
            root.join("ui/child.slint"),
            "export component Child inherits Rectangle {}",
        )
        .unwrap();
        std::fs::write(&main,"import { Child } from \"child.slint\"; export component Main inherits Window { Child {} }").unwrap();
        markup_tree(&root).unwrap();
        std::fs::write(
            root.join("outside.slint"),
            "export component Child inherits Rectangle {}",
        )
        .unwrap();
        std::fs::write(root.join("outside.png"), b"asset").unwrap();
        for source in [
            "import { Child } from \"../outside.slint\"; export component Main inherits Window {}",
            "export * from \"../outside.slint\";",
            "import { Child } from \"\\u{2e}./outside.slint\"; export component Main inherits Window {}",
            "export component Main inherits Window { Image { source: @image-url(\"../outside.png\"); } }",
            "export component Main inherits Window { Image { source: @image-url(folder + \"outside.png\"); } }",
            "import { Child } from folder; export component Main inherits Window {}",
            "@rust-attr(allow(unsafe_code)) export struct Injected { field:int }",
        ] {
            std::fs::write(&main, source).unwrap();
            assert!(markup_tree(&root).is_err(), "accepted {source}");
        }
        std::fs::write(root.join("resources/asset.png"), b"asset").unwrap();
        std::fs::write(&main,"export component Main inherits Window { Image { source: @image-url(\"../resources/asset.png\"); } }").unwrap();
        markup_tree(&root).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("outside.slint"), root.join("ui/linked.slint"))
                .unwrap();
            std::fs::write(&main, "export * from \"linked.slint\";").unwrap();
            assert!(markup_tree(&root).is_err());
            std::os::unix::fs::symlink(root.join("outside.png"), root.join("resources/linked.png"))
                .unwrap();
            std::fs::write(&main,"export component Main inherits Window { Image { source: @image-url(\"../resources/linked.png\"); } }").unwrap();
            assert!(markup_tree(&root).is_err());
        }
        std::fs::write(&main,"// import \"../outside.slint\";\nexport component Main inherits Window { Text { text: \"@image-url(outside)\"; } }").unwrap();
        markup_tree(&root).unwrap();
        std::fs::write(
            root.join("ui/child.slint"),
            "export * from \"../outside.slint\";",
        )
        .unwrap();
        std::fs::write(&main, "export * from \"child.slint\";").unwrap();
        assert!(markup_tree(&root).is_err());
    }
    #[test]
    fn compiler_reported_paths_are_independent_of_literal_markup_checks() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("ui")).unwrap();
        let main = root.join("ui/main.slint");
        std::fs::write(&main, "export component Main inherits Window {}").unwrap();
        assert!(
            compiler_record(
                &root,
                &format!("cargo:rerun-if-changed={}\n", main.display())
            )
            .unwrap()
        );
        let external = root.join("translation.po");
        std::fs::write(&external, "external compiler input").unwrap();
        assert!(
            compiler_record(
                &root,
                &format!("cargo:rerun-if-changed={}\n", external.display())
            )
            .is_err()
        );
        assert!(
            compiler_record(
                &root,
                "cargo:rerun-if-changed=builtin:/common/does-not-exist.slint\n"
            )
            .is_err()
        );
        let widgets = root.join("pinned-widgets");
        std::fs::create_dir(&widgets).unwrap();
        let standard = widgets.join("std-widgets.slint");
        std::fs::write(&standard, "pinned standard library").unwrap();
        let report = format!(
            "cargo:rerun-if-changed={}\ncargo:rerun-if-changed={}\n",
            main.display(),
            standard.display()
        );
        assert!(compiler_record(&root, &report).is_err());
        assert!(compiler_record_with_framework(&root, &report, Some(&widgets)).unwrap());
        let empty = root.join("empty-target");
        std::fs::create_dir(&empty).unwrap();
        assert!(
            compiler_inputs(&root, &empty, None)
                .unwrap_err()
                .contains("no GUI compiler dependency witness")
        );
    }
    #[test]
    fn pinned_standard_widgets_cannot_be_shadowed_and_rust_attributes_are_not_treated_as_data() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("ui")).unwrap();
        let main = root.join("ui/main.slint");
        std::fs::write(&main,"import { Button } from \"std-widgets.slint\"; export component Main inherits Window { Button {} }").unwrap();
        markup_tree(&root).unwrap();
        std::fs::write(
            root.join("ui/std-widgets.slint"),
            "export component Button inherits Rectangle {}",
        )
        .unwrap();
        assert!(markup_tree(&root).unwrap_err().contains("shadows"));
        std::fs::remove_file(root.join("ui/std-widgets.slint")).unwrap();
        let source = "@rust-attr(allow(unsafe_code)) export struct Injected { field:int }";
        let mut diagnostics = i_slint_compiler::diagnostics::BuildDiagnostics::default();
        i_slint_compiler::parser::parse(source.into(), Some(&main), &mut diagnostics);
        assert!(
            !diagnostics.has_errors(),
            "negative control must be valid Slint syntax"
        );
        std::fs::write(&main, source).unwrap();
        assert!(
            markup_tree(&root)
                .unwrap_err()
                .contains("handwritten Rust attributes")
        );
    }
}

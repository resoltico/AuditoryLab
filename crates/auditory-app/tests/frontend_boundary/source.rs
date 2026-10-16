// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Canonical source admission and frontend Rust syntax restrictions.
use super::gui;
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use syn::{ext::IdentExt, visit::Visit};

#[derive(Default)]
struct SyntaxGuard {
    forbidden: Option<String>,
    gui: bool,
    allow_generated: bool,
    generated_count: usize,
}
fn is_inclusion_macro(name: &proc_macro2::Ident) -> bool {
    matches!(
        name.unraw().to_string().as_str(),
        "include" | "include_str" | "include_bytes" | "include_modules"
    )
}
fn is_native_bridge_identifier(name: &proc_macro2::Ident) -> bool {
    let name = name.unraw().to_string();
    name == "winit"
        || name.starts_with("winit_")
        || name.starts_with("with_winit_")
        || matches!(
            name.as_str(),
            "on_winit_window_event" | "has_winit_window" | "set_platform" | "WinitWindowAccessor"
        )
}
fn is_signal_implementation(name: &proc_macro2::Ident) -> bool {
    matches!(
        name.unraw().to_string().as_str(),
        "SignalHistory"
            | "Renderer"
            | "Stimulus"
            | "ObservationProducer"
            | "ObservationConsumer"
            | "observation_channel"
            | "sin"
            | "cos"
            | "sin_cos"
            | "fft"
            | "ifft"
    )
}
impl SyntaxGuard {
    // Macro bodies are token trees rather than syn item ASTs. Inspect syntax
    // tokens recursively, preserving literals/comments as opaque text.
    fn visit_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        use proc_macro2::TokenTree;
        let mut tokens = tokens.into_iter().peekable();
        while let Some(token) = tokens.next() {
            match token {
                TokenTree::Ident(ident) if self.gui && is_native_bridge_identifier(&ident) => {
                    self.forbidden = Some(
                        "native bridge in macro tokens requires explicit reviewed syntax".into(),
                    );
                }
                TokenTree::Ident(ident)
                    if self.gui
                        && (is_signal_implementation(&ident)
                            || matches!(
                                ident.unraw().to_string().as_str(),
                                "fold" | "reduce" | "step_by"
                            )) =>
                {
                    self.forbidden = Some("GUI-local signal implementation in macro tokens".into());
                }
                TokenTree::Ident(ident)
                    if self.gui
                        && matches!(ident.unraw().to_string().as_str(), "unsafe" | "extern") =>
                {
                    self.forbidden =
                        Some("handwritten unsafe/extern syntax in GUI macro tokens".into());
                }
                TokenTree::Ident(ident)
                    if self.gui
                        && ident.unraw() == "slint"
                        && matches!(tokens.peek(),Some(TokenTree::Punct(p))if p.as_char()=='!') =>
                {
                    self.forbidden = Some("inline Slint compiler in macro tokens".into());
                }
                TokenTree::Ident(ident) if is_inclusion_macro(&ident) => {
                    self.forbidden = Some("source inclusion macro in macro tokens".into());
                }
                TokenTree::Ident(ident) if ident.unraw() == "use" => {
                    // Inspect ordinary use declarations inside macro bodies too.
                    // The AST visitor does not expand these token trees.
                    let mut declaration = proc_macro2::TokenStream::from(TokenTree::Ident(ident));
                    for token in tokens.clone() {
                        let end = matches!(&token, TokenTree::Punct(p) if p.as_char() == ';');
                        declaration.extend([token]);
                        if end {
                            break;
                        }
                    }
                    if let Ok(import) = syn::parse2::<syn::ItemUse>(declaration.clone()) {
                        self.visit_item_use(&import);
                    } else if inclusion_import_tokens(declaration, self.gui) {
                        // Metavariables can prevent ItemUse parsing, but literal
                        // inclusion names in the import are still prohibited.
                        self.forbidden =
                            Some("source inclusion macro import in macro tokens".into());
                    }
                }
                TokenTree::Punct(p) if p.as_char() == '#' => {
                    let _ =
                        tokens.next_if(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '!'));
                    if let Some(TokenTree::Group(group)) = tokens.peek()
                        && group.delimiter() == proc_macro2::Delimiter::Bracket
                    {
                        let wrapped = format!("#[{}] mod token_probe;", group.stream());
                        if let Ok(file) = syn::parse_file(&wrapped) {
                            self.visit_file(&file);
                        } else {
                            self.forbidden = Some(
                                "unresolved attribute tokens can change source inclusion".into(),
                            );
                        }
                    } else {
                        self.forbidden =
                            Some("forwarded attribute tokens can change source inclusion".into());
                    }
                }
                TokenTree::Group(group) => self.visit_tokens(group.stream()),
                _ => {}
            }
        }
    }
}
fn inclusion_import_tokens(tokens: proc_macro2::TokenStream, gui: bool) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(name) => {
            is_inclusion_macro(&name) || (gui && name.unraw() == "slint")
        }
        proc_macro2::TokenTree::Group(group) => inclusion_import_tokens(group.stream(), gui),
        _ => false,
    })
}
impl<'ast> Visit<'ast> for SyntaxGuard {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if self.gui && gui::native_trait_import(node) {
            return;
        }
        syn::visit::visit_item_use(self, node);
    }
    fn visit_path(&mut self, node: &'ast syn::Path) {
        if self.gui
            && node
                .segments
                .iter()
                .any(|segment| is_native_bridge_identifier(&segment.ident))
        {
            self.forbidden = Some("unreviewed native window/backend path; only the borrowed focus/occlusion bridge is permitted".into());
        }
        syn::visit::visit_path(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.gui && is_native_bridge_identifier(&node.method) {
            if !gui::native_bridge(node) {
                self.forbidden = Some("native window calls must be the read-only borrowed focus query or always-propagating occlusion observer".into());
            }
            self.visit_expr(&node.receiver);
            return;
        }
        if self.gui
            && (is_signal_implementation(&node.method)
                || matches!(
                    node.method.unraw().to_string().as_str(),
                    "reduce" | "fold" | "step_by"
                ))
        {
            self.forbidden = Some(
                "GUI-local signal synthesis/reduction; request analyzed bins from auditory-app"
                    .into(),
            );
        }
        syn::visit::visit_expr_method_call(self, node);
    }
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if self.gui
            && node
                .path
                .segments
                .iter()
                .any(|segment| is_signal_implementation(&segment.ident))
        {
            self.forbidden =
                Some("GUI-local signal implementation or raw observation transport".into());
        }
        syn::visit::visit_expr_path(self, node);
    }
    fn visit_file(&mut self, node: &'ast syn::File) {
        let permitted = self.allow_generated;
        self.allow_generated = false;
        for attribute in &node.attrs {
            self.visit_attribute(attribute);
        }
        for item in &node.items {
            self.allow_generated =
                permitted && matches!(item,syn::Item::Mod(module) if module.ident=="generated_ui");
            self.visit_item(item);
            self.allow_generated = false;
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if self.gui && node.unsafety.is_some() {
            self.forbidden = Some("handwritten GUI unsafe module".into());
        }
        if self.gui && node.ident == "generated_ui" {
            if self.gui && self.allow_generated && gui::generated_module(node) {
                self.generated_count += 1;
                if self.generated_count > 1 {
                    self.forbidden = Some("duplicate generated UI module".into());
                }
                return;
            }
            self.forbidden=Some("generated UI inclusion requires the one reviewed generated-only module and exact lint allowances".into());
        }
        syn::visit::visit_item_mod(self, node);
    }
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        if self.gui {
            self.forbidden = Some("handwritten GUI unsafe block".into());
        }
        syn::visit::visit_expr_unsafe(self, node);
    }
    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        if self.gui && node.unsafety.is_some() {
            self.forbidden = Some("handwritten GUI unsafe function".into());
        }
        syn::visit::visit_signature(self, node);
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if self.gui && node.unsafety.is_some() {
            self.forbidden = Some("handwritten GUI unsafe impl".into());
        }
        syn::visit::visit_item_impl(self, node);
    }
    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if self.gui && node.unsafety.is_some() {
            self.forbidden = Some("handwritten GUI unsafe trait".into());
        }
        syn::visit::visit_item_trait(self, node);
    }
    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        if self.gui {
            self.forbidden = Some("handwritten GUI extern block".into());
        }
        syn::visit::visit_item_foreign_mod(self, node);
    }

    fn visit_use_tree(&mut self, node: &'ast syn::UseTree) {
        // Reserve these import source names in frontends, irrespective of the
        // namespace prefix or alias. Rejecting the import itself avoids scope
        // tracking and also closes ordinary reexport/alias chains.
        let source = match node {
            syn::UseTree::Name(name) => Some(&name.ident),
            syn::UseTree::Rename(rename) => Some(&rename.ident),
            syn::UseTree::Path(path) => Some(&path.ident),
            syn::UseTree::Group(_) | syn::UseTree::Glob(_) => None,
        };
        if self.gui && matches!(node,syn::UseTree::Name(name) if name.ident.unraw()=="slint")
            || self.gui && matches!(node,syn::UseTree::Rename(name) if name.ident.unraw()=="slint")
        {
            self.forbidden = Some("inline Slint compiler macro import".into());
        }
        if source.is_some_and(is_inclusion_macro) {
            self.forbidden = Some("source inclusion macro import".into());
        }
        if self.gui && source.is_some_and(is_native_bridge_identifier) {
            self.forbidden = Some(
                "native window imports/aliases are prohibited except the exact accessor trait"
                    .into(),
            );
        }
        if self.gui && source.is_some_and(is_signal_implementation) {
            self.forbidden =
                Some("GUI-local signal implementation import, including aliases".into());
        }
        syn::visit::visit_use_tree(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.gui
            && node
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident.unraw() == "slint")
        {
            self.forbidden =
                Some("inline Slint compiler macro outside reviewed generated boundary".into());
        }
        if node
            .path
            .segments
            .last()
            .is_some_and(|s| is_inclusion_macro(&s.ident))
        {
            self.forbidden = Some("source inclusion macro".into());
        }
        self.visit_tokens(node.tokens.clone());
        syn::visit::visit_macro(self, node);
    }
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        fn redirects_input(meta: &syn::Meta) -> bool {
            if meta
                .path()
                .get_ident()
                .is_some_and(|id| matches!(id.unraw().to_string().as_str(), "path" | "macro_use"))
            {
                return true;
            }
            if let syn::Meta::List(list) = meta {
                use syn::parse::Parser;
                match syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                {
                    Ok(items) => return items.iter().any(redirects_input),
                    Err(_)
                        if list
                            .path
                            .get_ident()
                            .is_some_and(|name| name.unraw() == "cfg_attr") =>
                    {
                        return true;
                    }
                    Err(_) => {}
                }
            }
            false
        }
        if self.gui && gui::lint_weakening(&node.meta) {
            self.forbidden =
                Some("GUI lint weakening or unsafe attribute outside generated-only module".into());
        }
        if redirects_input(&node.meta) {
            self.forbidden = Some(
                "source path/macro-import attribute (including conditional attributes)".into(),
            );
        }
        // Attribute payloads contain token trees, including doc/include macros;
        // visiting syn Meta alone does not inspect those source-producing forms.
        use quote::ToTokens;
        self.visit_tokens(node.meta.to_token_stream());
        syn::visit::visit_attribute(self, node);
    }
}

fn syntax(source: &str) -> Result<SyntaxGuard, String> {
    syntax_for(source, false, false)
}
fn syntax_for(source: &str, gui: bool, allow_generated: bool) -> Result<SyntaxGuard, String> {
    let tree = syn::parse_file(source).map_err(|e| e.to_string())?;
    let mut guard = SyntaxGuard {
        gui,
        allow_generated,
        ..SyntaxGuard::default()
    };
    guard.visit_file(&tree);
    if let Some(reason) = &guard.forbidden {
        return Err(format!(
            "{reason} prohibited; use auditory-app capabilities"
        ));
    }
    Ok(guard)
}
fn sources(root: &Path, file: &Path, seen: &mut HashSet<PathBuf>) -> Result<(), String> {
    sources_for(root, file, seen, false)
}
fn sources_for(
    root: &Path,
    file: &Path,
    seen: &mut HashSet<PathBuf>,
    is_gui: bool,
) -> Result<(), String> {
    let actual = file
        .canonicalize()
        .map_err(|e| format!("{}: {e}", file.display()))?;
    if !actual.starts_with(root) {
        return Err(format!(
            "{} escapes frontend source directory {}",
            file.display(),
            root.display()
        ));
    }
    if !seen.insert(actual.clone()) {
        return Ok(());
    }
    let source = std::fs::read_to_string(&actual).map_err(|e| e.to_string())?;
    let is_main = actual == root.join("main.rs");
    let inspected = syntax_for(&source, is_gui, is_gui && is_main)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    if is_gui && is_main && inspected.generated_count != 1 {
        return Err("GUI main.rs requires exactly one generated-only inclusion module".into());
    }
    // Inspect every Rust file under src, including cfg-disabled modules and bins.
    Ok(())
}
fn directory_for(
    root: &Path,
    path: &Path,
    seen: &mut HashSet<PathBuf>,
    directories: &mut HashSet<PathBuf>,
    is_gui: bool,
) -> Result<(), String> {
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if !actual.starts_with(root) {
        return Err(format!(
            "{} escapes frontend source directory",
            path.display()
        ));
    }
    if !directories.insert(actual) {
        return Ok(());
    }
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            directory_for(root, &path, seen, directories, is_gui)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            sources_for(root, &path, seen, is_gui)?;
        }
    }
    Ok(())
}

/// Internal libraries may use their domain APIs; their authored compiler inputs
/// still cannot escape through inclusion aliases, macro forwarding or path attrs.
fn internal_source(package: &Path, file: &Path) -> Result<(), String> {
    use quote::ToTokens;
    let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let mut tree = syn::parse_file(&source).map_err(|e| format!("{}: {e}", file.display()))?;
    let catalog = package
        .file_name()
        .is_some_and(|name| name == "auditory-core")
        && file == package.join("src/catalog.rs");
    if catalog {
        let expected: syn::ItemMacro = syn::parse_str(r#"macro_rules! entries {
            ($($name:literal),* $(,)?) => {
                &[$(Entry { name: $name, text: include_str!(concat!("../catalog/", $name, ".toml")) }),*]
            };
        }"#).map_err(|e| e.to_string())?;
        let mut count = 0;
        for item in &tree.items {
            if let syn::Item::Macro(mac) = item
                && mac.ident.as_ref().is_some_and(|name| name == "entries")
            {
                count += 1;
                if mac.to_token_stream().to_string() != expected.to_token_stream().to_string() {
                    return Err(
                        "core catalog inclusion requires the exact reviewed entries macro".into(),
                    );
                }
            }
        }
        if count != 1 {
            return Err("core catalog requires one reviewed entries macro".into());
        }
        tree.items.retain(|item| !matches!(item,syn::Item::Macro(mac) if mac.ident.as_ref().is_some_and(|name| name=="entries")));
    }
    let mut catalog_inputs = CatalogInputs {
        package,
        catalog,
        failure: None,
    };
    catalog_inputs.visit_file(&tree);
    if let Some(error) = catalog_inputs.failure {
        return Err(format!("{}: {error}", file.display()));
    }
    let mut guard = SyntaxGuard::default();
    guard.visit_file(&tree);
    if let Some(error) = guard.forbidden {
        return Err(format!("{}: {error}", file.display()));
    }
    if file == package.join("build.rs") || file == package.join("build_metadata.rs") {
        let mut modules = BuildModules {
            root_script: file == package.join("build.rs"),
            depth: 0,
            failure: false,
        };
        modules.visit_file(&tree);
        if modules.failure {
            return Err("app identity build inputs permit only the ordinary build_metadata module; no module-generating macros".into());
        }
    }
    Ok(())
}
struct CatalogInputs<'a> {
    package: &'a Path,
    catalog: bool,
    failure: Option<String>,
}
fn has_identifier(tokens: proc_macro2::TokenStream, expected: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(name) => name.unraw() == expected,
        proc_macro2::TokenTree::Group(group) => has_identifier(group.stream(), expected),
        _ => false,
    })
}
impl<'ast> Visit<'ast> for CatalogInputs<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if self.catalog {
            self.failure =
                Some("catalog inclusion privilege cannot be inherited by child modules".into());
        }
        syn::visit::visit_item_mod(self, node);
    }
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if self.catalog && node.mac.path.is_ident("macro_rules") {
            self.failure = Some(
                "catalog inclusion privilege cannot be forwarded by another macro definition"
                    .into(),
            );
        }
        syn::visit::visit_item_macro(self, node);
    }
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        use quote::ToTokens;
        if self.catalog && has_identifier(node.to_token_stream(), "entries") {
            self.failure =
                Some("catalog inclusion macro aliases/reexports are not admitted".into());
        }
        syn::visit::visit_item_use(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node
            .path
            .segments
            .last()
            .is_some_and(|name| name.ident.unraw() == "entries")
        {
            use syn::parse::Parser;
            let literals =
                syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated
                    .parse2(node.tokens.clone());
            let admitted = self.catalog
                && node.path.is_ident("entries")
                && literals.is_ok_and(|names| {
                    !names.is_empty()
                        && names.iter().all(|name| {
                            let stem = name.value();
                            if stem.is_empty()
                                || !stem.bytes().all(|byte| {
                                    byte.is_ascii_lowercase()
                                        || byte.is_ascii_digit()
                                        || byte == b'-'
                                })
                            {
                                return false;
                            }
                            let expected =
                                self.package.join("catalog").join(format!("{stem}.toml"));
                            expected.is_file()
                                && expected
                                    .canonicalize()
                                    .is_ok_and(|actual| actual == expected)
                        })
                });
            if !admitted {
                self.failure = Some(
                    "catalog entries require direct literal names of admitted catalog TOMLs".into(),
                );
            }
        } else if self.catalog && has_identifier(node.tokens.clone(), "entries") {
            self.failure = Some(
                "catalog inclusion macro cannot be forwarded through other macro tokens".into(),
            );
        }
        syn::visit::visit_macro(self, node);
    }
}
struct BuildModules {
    root_script: bool,
    depth: usize,
    failure: bool,
}
impl<'ast> Visit<'ast> for BuildModules {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if node.content.is_none()
            && !(self.root_script && self.depth == 0 && node.ident == "build_metadata")
        {
            self.failure = true;
        }
        self.depth += 1;
        syn::visit::visit_item_mod(self, node);
        self.depth -= 1;
    }
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if node.mac.path.is_ident("macro_rules") {
            self.failure = true;
        }
        syn::visit::visit_item_macro(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if has_identifier(node.tokens.clone(), "mod") {
            self.failure = true;
        }
        syn::visit::visit_macro(self, node);
    }
}

/// Keep the original workspace authority while admitting package and source roots.
/// Canonicalizing a relocated root and trusting it would erase this boundary.
pub(super) fn workspace_sources(metadata: &Value, expected_workspace: &Path) -> Result<(), String> {
    let workspace = expected_workspace
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let declared = Path::new(
        metadata["workspace_root"]
            .as_str()
            .ok_or("missing workspace root")?,
    );
    if declared.canonicalize().map_err(|e| e.to_string())? != workspace {
        return Err("Cargo workspace root differs from the admitted workspace".into());
    }
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("missing workspace members")?;
    for package in metadata["packages"]
        .as_array()
        .ok_or("missing packages")?
        .iter()
        .filter(|p| members.contains(&p["id"]))
    {
        let name = package["name"].as_str().ok_or("missing package name")?;
        let root = workspace.join("crates").join(name);
        let manifest = Path::new(
            package["manifest_path"]
                .as_str()
                .ok_or("missing manifest path")?,
        );
        if manifest.canonicalize().map_err(|e| e.to_string())? != root.join("Cargo.toml") {
            return Err(format!(
                "{name}: manifest escapes admitted workspace package root {}",
                root.display()
            ));
        }
        let src = root.join("src");
        admitted_directory(&root, &src)?;
        let internal = !matches!(name, "auditory-cli" | "auditory-gui");
        contained_tree(
            &src,
            &src,
            &mut HashSet::new(),
            internal.then_some(root.as_path()),
        )?;
        if name == "auditory-app" {
            for file in ["build.rs", "build_metadata.rs"] {
                let path = root.join(file);
                if path.canonicalize().map_err(|e| e.to_string())? != path {
                    return Err("app build input escapes its admitted package root".into());
                }
                internal_source(&root, &path)?;
            }
        }
        let resource_roots: &[&str] = if name == "auditory-core" {
            &["catalog"]
        } else if name == "auditory-gui" {
            &["ui", "resources"]
        } else {
            &[]
        };
        for relative in resource_roots {
            let path = root.join(relative);
            if path.exists() {
                admitted_directory(&root, &path)?;
                contained_tree(&path, &path, &mut HashSet::new(), None)?;
            }
        }
        for target in package["targets"].as_array().ok_or("missing targets")? {
            let path = Path::new(target["src_path"].as_str().ok_or("missing target source")?);
            let actual = path.canonicalize().map_err(|e| e.to_string())?;
            let kinds = target["kind"].as_array().ok_or("missing target kind")?;
            let allowed = kinds.iter().all(|kind| match kind.as_str() {
                Some("bin" | "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro") => {
                    actual.starts_with(&src)
                }
                Some("custom-build") => actual == root.join("build.rs"),
                Some("test") => actual.starts_with(&src) || actual.starts_with(root.join("tests")),
                Some("bench") => {
                    actual.starts_with(&src) || actual.starts_with(root.join("benches"))
                }
                Some("example") => actual.starts_with(root.join("examples")),
                _ => false,
            });
            if !allowed || kinds.is_empty() {
                return Err(format!(
                    "{name}: target source {} escapes its admitted root",
                    path.display()
                ));
            }
            // Cargo target paths are Rust compiler inputs even when their
            // filename has another extension (or none). Directory discovery is
            // supplementary; it must not be their only syntax admission path.
            if internal
                && kinds.iter().any(|kind| {
                    matches!(
                        kind.as_str(),
                        Some(
                            "lib"
                                | "rlib"
                                | "dylib"
                                | "cdylib"
                                | "staticlib"
                                | "proc-macro"
                                | "bin"
                        )
                    )
                })
            {
                internal_source(&root, &actual)?;
            }
        }
    }
    Ok(())
}
fn admitted_directory(package: &Path, path: &Path) -> Result<(), String> {
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if actual != path || !actual.starts_with(package) {
        return Err(format!(
            "{} escapes admitted package source root",
            path.display()
        ));
    }
    Ok(())
}
fn contained_tree(
    root: &Path,
    path: &Path,
    seen: &mut HashSet<PathBuf>,
    internal: Option<&Path>,
) -> Result<(), String> {
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if !actual.starts_with(root) {
        return Err(format!("{} escapes admitted source root", path.display()));
    }
    if actual.is_dir() {
        if !seen.insert(actual) {
            return Ok(());
        }
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            contained_tree(
                root,
                &entry.map_err(|e| e.to_string())?.path(),
                seen,
                internal,
            )?;
        }
    } else if actual
        .extension()
        .is_some_and(|extension| extension == "rs")
        && let Some(package) = internal
    {
        internal_source(package, &actual)?;
    }
    Ok(())
}

pub(super) fn check_sources(package: &Value) -> Result<(), String> {
    let manifest = Path::new(package["manifest_path"].as_str().unwrap());
    let package_root = manifest
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let root = package_root.join("src");
    admitted_directory(&package_root, &root)?;
    let is_gui = package["name"] == "auditory-gui";
    let mut seen = HashSet::new();
    for target in package["targets"].as_array().unwrap() {
        if target["kind"].as_array().unwrap().iter().any(|k| {
            matches!(
                k.as_str(),
                Some("bin" | "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
            )
        }) {
            sources_for(
                &root,
                Path::new(target["src_path"].as_str().unwrap()),
                &mut seen,
                is_gui,
            )?;
        }
    }
    directory_for(&root, &root, &mut seen, &mut HashSet::new(), is_gui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn gui_requests_shared_analysis_and_only_maps_supplied_ranges_to_geometry() {
        for source in [
            "fn trace(x:f64) { let _=x.sin(); }",
            "fn trace(x:f64) { let _=f64::cos(x); }",
            "use libm::sin as position;",
            "use auditory_app::SignalHistory as Geometry;",
            "use auditory_device::observation_channel as source;",
            "fn trace(x:&[f64]) { let _=x.iter().copied().reduce(f64::max); }",
            "fn trace(x:&[f64]) { let _=x.iter().fold(0.0, |a,b| f64::max(a,*b)); }",
            "fn trace(x:&[f64]) { let _=x.iter().step_by(20); }",
            "macro_rules! trace { ($x:expr) => { $x.iter().fold(0.0, |a,b| f64::max(a,*b)) }; }",
        ] {
            assert!(
                syntax_for(source, true, false).is_err(),
                "accepted {source}"
            );
        }
        // Presentation clipping and per-bin path construction are legitimate. This
        // syntax gate is not a claim to solve arbitrary Rust program semantics.
        syntax_for("fn path(bin:auditory_app::SignalBin,width:f64) { let x=width.max(1.0); let _=(bin.left().min()*x,bin.left().max()*x); }", true, false).unwrap();
    }

    #[test]
    fn only_the_borrowed_focus_query_and_propagating_occlusion_observer_are_allowed() {
        let focus = "ui.window().with_winit_window(|window| window.has_focus()).unwrap_or(false)";
        let observer = "ui.window().on_winit_window_event(move |_, event| { if let slint::winit_030::winit::event::WindowEvent::Occluded(occluded) = event { reported_occluded.set(*occluded); } slint::winit_030::EventResult::Propagate });";
        let valid = format!(
            "use slint::winit_030::WinitWindowAccessor; fn eligible() {{ let _ = {focus}; {observer} }}"
        );
        syntax_for(&valid, true, false).unwrap();
        for changed in [
            focus.replace("window.has_focus()", "window.clone()"),
            focus.replace("window.has_focus()", "window.set_visible(false)"),
            focus.replace(
                "|window| window.has_focus()",
                "|window| { save(window); window.has_focus() }",
            ),
            observer.replace("EventResult::Propagate", "EventResult::PreventDefault"),
            observer.replace(
                "reported_occluded.set(*occluded);",
                "reported_occluded.set(*occluded); controller.borrow_mut().stop();",
            ),
            "ui.window().winit_window().await;".into(),
            "backend.with_winit_event_loop_builder(builder);".into(),
            "backend.with_winit_custom_application_handler(handler);".into(),
            "backend.with_winit_window_attributes_hook(|attributes| attributes);".into(),
            "slint::platform::set_platform(platform);".into(),
            "slint::winit_030::WinitWindowAccessor::winit_window(ui.window());".into(),
        ] {
            let source = format!("fn outside() {{ {changed}; }}");
            assert!(
                syntax_for(&source, true, false).is_err(),
                "accepted {source}"
            );
        }
        for source in [
            "use slint::winit_030::winit;",
            "use slint::winit_030::winit::window::Window as Native;",
            "use slint::winit_030::WinitWindowAccessor as Native;",
            "use slint::winit_030 as native;",
            "fn retain(_:std::sync::Arc<slint::winit_030::winit::window::Window>) {}",
            "macro_rules! retain { () => { ui.window().winit_window() }; }",
        ] {
            assert!(
                syntax_for(source, true, false).is_err(),
                "accepted {source}"
            );
        }
    }
    #[test]
    fn syntax_controls_use_ast_including_cfg_disabled_code() {
        for source in [
            "include!(\"../engine.rs\");",
            "const X: &str = include_str!(\"../../x\");",
            "const X: &[u8] = include_bytes!(\"x\");",
            "#[cfg(any())] #[path=\"../../engine.rs\"] mod engine;",
            "#[cfg_attr(any(), path=\"../../x.rs\")] mod x;",
            "#[cfg_attr(any(), cfg_attr(all(), path=\"x\"))] mod x;",
        ] {
            assert!(syntax(source).is_err(), "accepted {source}");
        }
        assert!(
            syntax("// include!(\"x\");\nconst TEXT: &str = \"#[path = x] include_bytes!(x)\";")
                .is_ok()
        );
    }
    #[test]
    fn internal_production_inclusions_cannot_escape_admitted_inputs() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_source = outside.path().join("outside.rs");
        std::fs::write(&outside_source, "pub fn outside() {}\n").unwrap();
        let attacks = [
            r#"include!("../../../../outside.rs");"#,
            r#"const VALUE:&str=std::include_str!("../../../../outside");"#,
            r#"#[path="../../../../outside.rs"] mod outside;"#,
            r#"#[cfg_attr(all(),path="../../../../outside.rs")] mod outside;"#,
            r#"#[cfg_attr(all(),doc=include_str!("../../../../outside"))] fn data() {}"#,
            r#"#[doc=include_bytes!("../../../../outside")] fn data() {}"#,
            r#"use std::include as innocent; innocent!("../../../../outside.rs");"#,
            r#"macro_rules! load {()=>{include!("../../../../outside.rs");}} load!();"#,
            r#"macro_rules! load {($kind:ident)=>{$kind!("../../../../outside.rs");}} load!(include);"#,
            r#"macro_rules! load {($condition:meta)=>{#[cfg_attr($condition,path="../../../../outside.rs")] mod outside;}} load!(all());"#,
            r#"macro_rules! load {($condition:meta)=>{#[r#cfg_attr($condition,r#path="../../../../outside.rs")] mod outside;}} load!(all());"#,
            r#"macro_rules! load {($attribute:meta)=>{#[$attribute] mod outside;}} load!(path="../../../../outside.rs");"#,
            r#"#[macro_use] mod catalog;"#,
        ];
        for role in [
            "auditory-core",
            "auditory-device",
            "auditory-wav",
            "auditory-app",
        ] {
            let package = workspace.join("crates").join(role);
            std::fs::create_dir_all(package.join("src")).unwrap();
            std::fs::write(package.join("Cargo.toml"), "").unwrap();
            if role == "auditory-app" {
                std::fs::write(package.join("build.rs"), "mod build_metadata; fn main() {}")
                    .unwrap();
                std::fs::write(package.join("build_metadata.rs"), "pub fn data() {}").unwrap();
            }
            let file = package.join("src/lib.rs");
            let metadata = json!({"workspace_root":workspace,"workspace_members":[role],"packages":[{
                "id":role,"name":role,"manifest_path":package.join("Cargo.toml"),
                "targets":[{"kind":["lib"],"src_path":file}]
            }]});
            let existing_input_attacks = [
                format!("include!({outside_source:?});"),
                format!("#[path={outside_source:?}] mod outside;"),
                format!("#[doc=include_str!({outside_source:?})] fn outside() {{}}"),
            ];
            for source in attacks
                .iter()
                .copied()
                .chain(existing_input_attacks.iter().map(String::as_str))
            {
                std::fs::write(&file, source).unwrap();
                assert!(
                    workspace_sources(&metadata, &workspace).is_err(),
                    "{role} admitted {source}"
                );
            }
            // Domain code/imports and ordinary common-noun variables are not
            // frontend policy. Only source-producing forms are constrained.
            std::fs::write(&file,r#"use auditory_core::Stimulus; fn domain(){ let entries=3; let _=format!("{entries}"); let _=vec![entries]; let _=libm::sin(0.0); }"#).unwrap();
            workspace_sources(&metadata, &workspace).unwrap();
            for filename in ["entry.input", "entry"] {
                let explicit = package.join("src").join(filename);
                let mut explicit_metadata = metadata.clone();
                explicit_metadata["packages"][0]["targets"][0]["src_path"] = json!(explicit);
                std::fs::write(&explicit, "pub fn domain() {}").unwrap();
                workspace_sources(&explicit_metadata, &workspace).unwrap();
                for source in [
                    format!("include!({outside_source:?});"),
                    format!("#[path={outside_source:?}] mod outside;"),
                ] {
                    std::fs::write(&explicit, source).unwrap();
                    assert!(
                        workspace_sources(&explicit_metadata, &workspace).is_err(),
                        "{role} failed to inspect explicit {filename} target"
                    );
                }
                std::fs::remove_file(explicit).unwrap();
            }
            if role == "auditory-app" {
                for source in [
                    r#"mod outside; fn main() {}"#,
                    r#"mod parent { mod build_metadata; } fn main() {}"#,
                    r#"macro_rules! load {()=>{mod outside;}} load!(); fn main() {}"#,
                ] {
                    std::fs::write(package.join("build.rs"), source).unwrap();
                    assert!(
                        workspace_sources(&metadata, &workspace).is_err(),
                        "app build admitted {source}"
                    );
                }
            }
        }
        let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../auditory-core")
            .canonicalize()
            .unwrap();
        internal_source(&actual, &actual.join("src/catalog.rs")).unwrap();
        let catalog = workspace.join("crates/auditory-core");
        std::fs::create_dir(catalog.join("catalog")).unwrap();
        std::fs::write(catalog.join("catalog/safe.toml"), "").unwrap();
        let source = std::fs::read_to_string(actual.join("src/catalog.rs")).unwrap();
        let tree = syn::parse_file(&source).unwrap();
        use quote::ToTokens;
        let definition = tree.items.iter().find(|item| matches!(item,syn::Item::Macro(mac) if mac.ident.as_ref().is_some_and(|name| name=="entries"))).unwrap().to_token_stream().to_string();
        let file = catalog.join("src/catalog.rs");
        std::fs::write(
            &file,
            format!("{definition} const ENTRIES:&[Entry]=entries![\"safe\"];"),
        )
        .unwrap();
        internal_source(&catalog, &file).unwrap();
        for invocation in [
            r#"entries!["../outside"]"#,
            r#"entries![concat!("safe")]"#,
            r#"entries!["missing"]"#,
            r#"{ use entries as alias; alias!("../outside") }"#,
            r#"{ macro_rules! forward {($name:ident)=>{$name!("../outside")}} forward!(entries) }"#,
            r#"{ mod child; entries!["safe"] }"#,
        ] {
            std::fs::write(
                &file,
                format!("{definition} const ENTRIES:&[Entry]={invocation};"),
            )
            .unwrap();
            assert!(
                internal_source(&catalog, &file).is_err(),
                "catalog admitted {invocation}"
            );
        }
        std::fs::write(
            &file,
            format!(
                "{} const ENTRIES:&[Entry]=entries![\"safe\"];",
                definition.replace("../catalog/", "../../outside/")
            ),
        )
        .unwrap();
        assert!(internal_source(&catalog, &file).is_err());
    }
    #[test]
    fn workspace_package_and_source_roots_cannot_relocate() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().canonicalize().unwrap();
        let package = workspace.join("crates/auditory-cli");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(package.join("Cargo.toml"), "").unwrap();
        std::fs::write(package.join("src/main.rs"), "fn main() {}").unwrap();
        let metadata = json!({"workspace_root":workspace,"workspace_members":["cli"],"packages":[{
            "id":"cli","name":"auditory-cli","manifest_path":package.join("Cargo.toml"),
            "targets":[{"kind":["bin"],"src_path":package.join("src/main.rs")}]
        }]});
        workspace_sources(&metadata, &workspace).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut alternate_workspace = metadata.clone();
        alternate_workspace["workspace_root"] = json!(outside.path());
        assert!(workspace_sources(&alternate_workspace, &workspace).is_err());
        std::fs::create_dir(outside.path().join("src")).unwrap();
        std::fs::write(outside.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(outside.path().join("src/main.rs"), "fn main() {}").unwrap();
        let mut relocated = metadata.clone();
        relocated["packages"][0]["manifest_path"] = json!(outside.path().join("Cargo.toml"));
        relocated["packages"][0]["targets"][0]["src_path"] =
            json!(outside.path().join("src/main.rs"));
        assert!(workspace_sources(&relocated, &workspace).is_err());
        #[cfg(unix)]
        {
            // A whole directory relocation used to reset the check's authority.
            std::fs::remove_dir_all(package.join("src")).unwrap();
            std::os::unix::fs::symlink(outside.path().join("src"), package.join("src")).unwrap();
            assert!(workspace_sources(&metadata, &workspace).is_err());
            assert!(check_sources(&metadata["packages"][0]).is_err());
            std::fs::remove_file(package.join("src")).unwrap();
            std::fs::create_dir(package.join("src")).unwrap();
            std::fs::write(package.join("src/main.rs"), "fn main() {}").unwrap();
            let gui_root = workspace.join("crates/auditory-gui");
            std::fs::rename(&package, &gui_root).unwrap();
            let mut gui_metadata = metadata.clone();
            gui_metadata["packages"][0]["name"] = json!("auditory-gui");
            gui_metadata["packages"][0]["manifest_path"] = json!(gui_root.join("Cargo.toml"));
            gui_metadata["packages"][0]["targets"][0]["src_path"] =
                json!(gui_root.join("src/main.rs"));
            workspace_sources(&gui_metadata, &workspace).unwrap();
            for resource in ["ui", "resources"] {
                std::os::unix::fs::symlink(outside.path().join("src"), gui_root.join(resource))
                    .unwrap();
                assert!(workspace_sources(&gui_metadata, &workspace).is_err());
                std::fs::remove_file(gui_root.join(resource)).unwrap();
            }
            let core_root = workspace.join("crates/auditory-core");
            std::fs::rename(&gui_root, &core_root).unwrap();
            let mut core_metadata = metadata.clone();
            core_metadata["packages"][0]["name"] = json!("auditory-core");
            core_metadata["packages"][0]["manifest_path"] = json!(core_root.join("Cargo.toml"));
            core_metadata["packages"][0]["targets"][0]["src_path"] =
                json!(core_root.join("src/main.rs"));
            std::os::unix::fs::symlink(outside.path().join("src"), core_root.join("catalog"))
                .unwrap();
            assert!(workspace_sources(&core_metadata, &workspace).is_err());
            std::fs::remove_file(core_root.join("catalog")).unwrap();
            std::fs::create_dir(core_root.join("catalog")).unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("src/main.rs"),
                core_root.join("catalog/recipe.toml"),
            )
            .unwrap();
            assert!(workspace_sources(&core_metadata, &workspace).is_err());
            std::fs::remove_dir_all(core_root).unwrap();
            std::os::unix::fs::symlink(outside.path(), &package).unwrap();
            assert!(workspace_sources(&metadata, &workspace).is_err());
        }
    }
    #[test]
    fn canonical_source_containment_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("src");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside.rs");
        std::fs::write(&outside, "").unwrap();
        assert!(
            sources(&root.canonicalize().unwrap(), &outside, &mut HashSet::new())
                .unwrap_err()
                .contains("escapes")
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("escape.rs")).unwrap();
            assert!(
                directory_for(
                    &root.canonicalize().unwrap(),
                    &root,
                    &mut HashSet::new(),
                    &mut HashSet::new(),
                    false,
                )
                .unwrap_err()
                .contains("escapes")
            );
        }
    }

    #[test]
    fn macro_body_inclusion_and_attributes_are_rejected() {
        for source in [
            r#"macro_rules! load { () => { include!("../../outside.rs"); } } load!();"#,
            r#"cfg_if! { if #[cfg(windows)] { #[path="../../outside.rs"] mod x; } }"#,
            r#"macro_rules! text { () => { include_str!("../../outside"); } }"#,
            r#"r#include!("../../outside.rs");"#,
            r#"#[r#path="../../outside.rs"] mod x;"#,
            r#"#[cfg_attr(any(), r#path="../../outside.rs")] mod x;"#,
            r#"macro_rules! text { () => { r#include_bytes!("outside"); } }"#,
        ] {
            assert!(syntax(source).is_err(), "accepted {source}");
        }
        assert!(syntax(r#"macro_rules! text { () => { "include!(outside) #[path=x]" } }"#).is_ok());
    }
    #[test]
    fn frontend_library_target_cannot_escape_source_root() {
        let directory = tempfile::tempdir().unwrap();
        let src = directory.path().join("src");
        std::fs::create_dir(&src).unwrap();
        let main = src.join("main.rs");
        std::fs::write(&main, "fn main() {}").unwrap();
        let library = directory.path().join("outside.rs");
        std::fs::write(&library, "").unwrap();
        let package = json!({"manifest_path":directory.path().join("Cargo.toml"),"targets":[
        {"kind":["bin"],"src_path":main},{"kind":["lib"],"src_path":library}]});
        assert!(check_sources(&package).unwrap_err().contains("escapes"));
    }

    fn inclusion_import_fixtures() -> Vec<String> {
        ["include", "include_str", "include_bytes"]
        .into_iter()
        .flat_map(|builtin| {
            [
                format!("use std::{builtin} as source_file;"),
                format!("use ::std::{{io, {builtin} as source_file}};"),
                format!("use {{std::{{{builtin} as source_file}}}};"),
                format!("pub use core::r#{builtin} as r#source_file;"),
                format!("#[cfg(any())] use std::{builtin} as source_file;"),
                format!("fn local() {{ use std::{builtin} as source_file; }}"),
                format!("use std as library; use library::{builtin} as source_file;"),
                format!("use std::{builtin} as first; use first as source_file;"),
                format!("macro_rules! bind {{ ($alias:ident) => {{ use std::{builtin} as $alias; }} }} bind!(source_file);"),
                format!("macro_rules! bind {{ ($alias:ident) => {{ use ::std::{{r#{builtin} as $alias}}; }} }}"),
                format!("macro_rules! bind {{ ($($alias:ident),*) => {{ $(use std::{builtin} as $alias;)* }} }}"),
                format!("macro_rules! bind {{ ($root:ident) => {{ use $root::{builtin} as source_file; }} }}"),
                format!("macro_rules! import {{ () => {{ use std::{builtin} as source_file; }} }}"),
                format!(
                    "wrapper! {{ #[cfg(any())] pub use {{std::{{r#{builtin} as source_file}}}}; }}"
                ),
            ]
        })
        .collect()
    }
    #[test]
    fn inclusion_import_aliases_are_rejected_as_syntax() {
        for source in inclusion_import_fixtures() {
            assert!(syntax(&source).is_err(), "accepted {source}");
        }
        for source in [
            "use std::io::{self, Read as Input};",
            "// use std::include as source_file;\nconst TEXT: &str = \"use std::include_str as text\";",
            "macro_rules! text { () => { \"use std::include_bytes as bytes;\" } }",
        ] {
            assert!(syntax(source).is_ok(), "rejected harmless {source}");
        }
    }
    #[test]
    fn real_frontend_source_tree_rejects_inclusion_import_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let src = directory.path().join("src");
        std::fs::create_dir(&src).unwrap();
        let main = src.join("main.rs");
        std::fs::write(directory.path().join("outside.rs"), "fn outside() {}\n").unwrap();
        let package = json!({"manifest_path":directory.path().join("Cargo.toml"),"targets":[{"kind":["bin"],"src_path":main}]});
        for source in inclusion_import_fixtures().into_iter().chain([
        "use std::include as source_file; source_file!(\"../outside.rs\"); fn main() {}".into(),
        "use std::include_str as text; const TEXT: &str = text!(\"../outside.rs\"); fn main() {}".into(),
        "use std::include_bytes as bytes; const BYTES: &[u8] = bytes!(\"../outside.rs\"); fn main() {}".into(),
    ]) {
        std::fs::write(&main, &source).unwrap();
        let result = check_sources(&package);
        assert!(result.is_err(), "accepted {source}");
        let error = result.unwrap_err();
        assert!(error.contains("main.rs"), "missing source context: {error}");
    }
        std::fs::write(
            &main,
            "use std::io as input; fn main() {} // include as x\n",
        )
        .unwrap();
        check_sources(&package).unwrap();
    }

    fn generated_source() -> String {
        r#"#[allow(unsafe_code, unreachable_pub, clippy::unwrap_used, clippy::todo, reason="pinned generated code only")] mod generated_ui { slint::include_modules!(); }"#.into()
    }
    #[test]
    fn only_the_exact_generated_module_and_lint_boundary_is_allowed() {
        let valid = generated_source();
        let checked = syntax_for(&valid, true, true).unwrap();
        assert_eq!(checked.generated_count, 1);
        assert!(syntax_for(&valid, false, true).is_err());
        assert!(syntax_for(&valid, true, false).is_err());
        for changed in [
            valid.replace(
                "slint::include_modules!();",
                "fn handwritten() {} slint::include_modules!();",
            ),
            valid.replace("mod generated_ui", "pub mod generated_ui"),
            valid.replace("mod generated_ui", "unsafe mod generated_ui"),
            valid.replace(
                "slint::include_modules!()",
                "slint::include_modules!(\"outside\")",
            ),
            valid.replace("unsafe_code,", "unsafe_code, warnings,"),
            valid.replace("clippy::todo,", "clippy::all,"),
            valid.replace("unreachable_pub,", ""),
            format!("mod hidden {{ {valid} }}"),
            format!("{valid}\n{valid}"),
            format!("fn hidden() {{ {valid} }}"),
        ] {
            assert!(
                syntax_for(&changed, true, true).is_err(),
                "accepted {changed}"
            );
        }
        for source in [
            "slint::include_modules!();",
            "use slint::include_modules as source; source!();",
            "use slint::slint as compiler; compiler!{}",
            "slint::slint! { export component Another inherits Window {} }",
            "#[cfg(any())] unsafe fn ignored() {}",
            "fn bad(){ unsafe {} }",
            "unsafe trait Hidden {}",
            "unsafe impl Send for Thing {}",
            "unsafe extern \"C\" { fn native(); }",
            "#[unsafe(no_mangle)] fn raw() {}",
            "#[allow(unsafe_code,reason=\"elsewhere\")] fn lower() {}",
            "#[cfg_attr(any(), allow(unreachable_pub))] mod hidden {}",
            "macro_rules! hide { () => { unsafe fn raw() {} }; }",
            "wrapper! { #[cfg(any())] #[allow(unsafe_code)] fn raw() {} }",
            "macro_rules! hide { () => { slint::include_modules!(); }; }",
            "macro_rules! compiler { () => { slint::slint!{} }; }",
            "macro_rules! alias { ($root:ident) => { use $root::slint as compiler; }; }",
            "macro_rules! alias { ($root:ident) => { use $root::include_modules as compiler; }; }",
        ] {
            assert!(
                syntax_for(source, true, false).is_err(),
                "accepted {source}"
            );
        }
        assert!(syntax_for("// unsafe fn f() {}\nconst TEXT:&str=\"slint::include_modules!(); #[allow(unsafe_code)]\"; use slint::ComponentHandle;",true,false).is_ok());
    }
}

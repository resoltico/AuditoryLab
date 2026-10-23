//! Build, license and package Auditory Lab downloads.
// Exercise the exact native policy source without pulling Winit's example/dev graph.
mod archive;
#[cfg(test)]
#[path = "../../third_party/winit/src/platform_impl/linux/x11/xsettings/gtk_scaling.rs"]
mod gtk_scaling;
mod macos;
mod notices;
mod package;
mod pe_icon;
mod source;
mod targets;
use serde_json::Value;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use targets::platform;
const LEGAL_FILES: [&str; 5] = [
    "LICENSE",
    "README.txt",
    "THIRD-PARTY-NOTICES.txt",
    "COPYRIGHT-library.html",
    "SOURCE.txt",
];
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("{command:?} exited with {status}").into());
    }
    Ok(())
}
fn capture(command: &mut Command) -> Result<Vec<u8>> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{command:?} exited with {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}
fn metadata(target: Option<&str>) -> Result<Value> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--locked", "--format-version", "1"]);
    if let Some(target) = target {
        command.args(["--filter-platform", target]);
    }
    Ok(serde_json::from_slice(&capture(&mut command)?)?)
}
fn version(metadata: &Value) -> Result<&str> {
    metadata["packages"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["name"] == "auditorylab"))
        .and_then(|p| p["version"].as_str())
        .ok_or_else(|| "workspace version missing".into())
}
fn build_number(metadata: &Value) -> Result<u32> {
    metadata["metadata"]["auditorylab"]["build-number"]
        .as_u64()
        .filter(|n| (1..=9999).contains(n))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            "workspace.metadata.auditorylab.build-number must be an integer in 1..9999".into()
        })
}
fn native_version(version: &str) -> Result<()> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("macOS marketing version must contain three decimal components; prerelease versions are unsupported".into());
    }
    Ok(())
}
fn rust_version() -> Result<String> {
    let source = fs::read_to_string("rust-toolchain.toml")?;
    source
        .lines()
        .find_map(|l| l.strip_prefix("channel = "))
        .map(|v| v.trim_matches('"').to_owned())
        .ok_or_else(|| "pinned Rust version missing".into())
}
fn rust_standard_library_copyright() -> Result<Vec<u8>> {
    let compiler = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let actual = String::from_utf8(capture(Command::new(&compiler).arg("--version"))?)?;
    if actual.split_whitespace().nth(1) != Some(rust_version()?.as_str()) {
        return Err(
            "distribution compiler differs from rust-toolchain.toml; use the pinned toolchain"
                .into(),
        );
    }
    let sysroot = String::from_utf8(capture(
        Command::new(compiler).args(["--print", "sysroot"]),
    )?)?;
    let path = Path::new(sysroot.trim()).join("share/doc/rust/COPYRIGHT-library.html");
    let bytes = fs::read(&path).map_err(|error| {
        format!("cannot read Rust standard-library notices at {path:?}: {error}")
    })?;
    if bytes.is_empty() {
        return Err("Rust standard-library copyright file is empty".into());
    }
    Ok(bytes)
}
fn readme(version: &str, gui: bool, target: &str) -> String {
    include_str!("../templates/archive-readme.txt").replace("{version}",version).replace("{product}",if gui {"Desktop"} else {"CLI"}).replace("{start}",if gui {"Open Auditory Lab.app on macOS, auditorylab-gui.exe on Windows, or auditorylab-gui on Linux. Select a sound, read its summary, then use Listen or Export WAV…."} else {"Run auditorylab --help, then auditorylab list. Use auditorylab describe ID to read about a sound, and auditorylab recipe ID --output recipe.toml to edit one."}).replace("{platform_needs}",if target.contains("apple") {"macOS 15 or later. If first launch is blocked, use System Settings > Privacy & Security > Open Anyway after trying to open the app. Apple instructions: https://support.apple.com/en-us/102445"} else if target.contains("windows") {if gui { "Windows 10 or later on x86_64; Windows 11 on ARM64. Desktop drawing needs an OpenGL 2.0 or newer graphics driver." } else { "Windows 10 or later on x86_64; Windows 11 on ARM64." }} else {if gui { "Linux with glibc 2.35 or later, ALSA, X11 or Wayland, fontconfig, xkbcommon (including its X11 library on X11), fonts, Pango/GTK 3/GIO for desktop font preferences, OpenGL and an xdg-desktop-portal for file dialogs." } else { "Linux with glibc 2.35 or later and ALSA." }})
}
fn section<'a>(changelog: &'a str, version: &str) -> Result<(&'a str, &'a str)> {
    let marker = format!("## [{version}]");
    let dated = format!("{marker} - ");
    let mut tail = changelog;
    while !tail.is_empty() {
        let (heading, body) = tail.split_once('\n').unwrap_or((tail, ""));
        if heading == marker || heading.starts_with(&dated) {
            let end = if body.starts_with("## ") {
                0
            } else {
                body.find("\n## ").unwrap_or(body.len())
            };
            return Ok((heading, body[..end].trim()));
        }
        tail = body;
    }
    Err("changelog version section missing".into())
}
fn release_notes<'a>(changelog: &'a str, version: &str) -> Result<&'a str> {
    let (heading, body) = section(changelog, version)?;
    let date = heading
        .strip_prefix(&format!("## [{version}] - "))
        .ok_or("release needs a dated changelog section")?;
    let fields: Vec<_> = date.split('-').collect();
    if fields.len() != 3
        || fields[0].len() != 4
        || fields[1].len() != 2
        || fields[2].len() != 2
        || fields
            .iter()
            .any(|field| !field.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("release date must be YYYY-MM-DD".into());
    }
    let year: u32 = fields[0].parse()?;
    let month: usize = fields[1].parse()?;
    let day: u32 = fields[2].parse()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        28 + u32::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days[month - 1]
        || body.is_empty()
    {
        return Err("release needs a valid date and nonempty changelog section".into());
    }
    Ok(body)
}
// Cargo metadata merges host/target and workspace feature sets. The selected
// product's normal-edge tree supplies the enabled graph; metadata supplies IDs.
fn product_dependencies(
    metadata: &Value,
    product: &str,
    target: &str,
    edges: &str,
) -> Result<std::collections::BTreeSet<String>> {
    let tree = String::from_utf8(capture(Command::new("cargo").args([
        "tree", "--locked", "--target", target, "-p", product, "--edges", edges, "--prefix",
        "none", "--format", "{p}",
    ]))?)?;
    notices::identities(metadata, &tree)
}
fn about(product: &str, target: &str, meta: &Value) -> Result<String> {
    let manifest = format!("crates/{product}/Cargo.toml");
    let data = capture(Command::new("cargo").args([
        "about",
        "generate",
        "--format",
        "json",
        "--locked",
        "-c",
        "about.toml",
        "-m",
        &manifest,
        "--target",
        target,
    ]))?;
    let json: Value = serde_json::from_slice(&data)?;
    let dependencies = product_dependencies(
        meta,
        if product == "cli" {
            "auditorylab-cli"
        } else {
            "auditorylab-gui"
        },
        target,
        "normal,build",
    )?;
    let normal = product_dependencies(
        meta,
        if product == "cli" {
            "auditorylab-cli"
        } else {
            "auditorylab-gui"
        },
        target,
        "normal",
    )?;
    let mut text = notices::assemble(
        &json,
        meta,
        &dependencies,
        &normal,
        &rust_version()?,
        target.contains("windows"),
    )?;
    if product == "gui" {
        text.push_str(
            "\nOwner artwork terms (complete software and artwork terms are also in LICENSE)\n\n",
        );
        text.push_str(
            include_str!("../../LICENSE")
                .split("The unmodified Mozilla Public License follows.")
                .next()
                .ok_or("artwork terms missing")?,
        );
    }
    Ok(text)
}
fn version_line(binary: &str, version: &str, build: u32, revision: Option<&str>) -> Result<String> {
    let source = match revision {
        Some(rev) => {
            if !matches!(rev.len(), 40 | 64) || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("AUDITORYLAB_SOURCE_REVISION must be a full 40- or 64-character ASCII hex commit ID".into());
            }
            format!(" · {}", &rev[..12])
        }
        None => String::new(),
    };
    Ok(format!("{binary} {version} (build {build}){source}"))
}
fn smoke(cli: &Path, scratch: &Path) -> Result<()> {
    let listing = String::from_utf8(capture(Command::new(cli).arg("list"))?)?;
    for entry in fs::read_dir("crates/core/catalog")? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "toml")
            && !listing.contains(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("invalid catalog filename")?,
            )
        {
            return Err(format!("catalog ID absent from list: {path:?}").into());
        }
    }
    let path = scratch.join("one-frame.wav");
    for expected_success in [true, false] {
        let existing = b"preserve this existing destination";
        if !expected_success {
            fs::write(&path, existing)?;
        }
        let mut child = Command::new(cli)
            .args(["export", "--stdin", "--sample-rate", "8000", "--output"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("stdin missing")?
            .write_all(b"schema_version=2\nduration_seconds=0.000125\nfade_seconds=0\n")?;
        let output = child.wait_with_output()?;
        if output.status.success() != expected_success {
            return Err(format!(
                "export smoke status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let bytes = fs::read(&path)?;
        if expected_success {
            if bytes.len() != 66 || &bytes[..4] != b"RIFF" {
                return Err("one-frame export must be a 66-byte RIFF file".into());
            }
        } else if bytes != existing {
            return Err("no-clobber export modified the existing destination".into());
        }
    }
    fs::remove_file(path)?;
    println!("Smoke checks passed: catalog, one-frame RIFF, no-clobber.");
    Ok(())
}
fn source_notice(version: &str, build: u32, source: &source::Source) -> String {
    format!(
        "Auditory Lab {version} (build {build})\nSource revision: {}\nSource commit epoch: {} (Unix UTC seconds)\nCorresponding source: https://github.com/resoltico/AuditoryLab/tree/{}\nRelease source archives: https://github.com/resoltico/AuditoryLab/releases\nSoftware uses MPL-2.0; owner artwork has separate terms in LICENSE.\nRedistributors must provide corresponding MPL-covered source and tell recipients how to obtain it.\n",
        source.revision, source.epoch, source.revision
    )
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let path = entry?.path();
        let name = path.file_name().ok_or("asset name missing")?;
        if path.is_dir() {
            copy_tree(&path, &destination.join(name))?;
        } else {
            fs::copy(&path, destination.join(name))?;
        }
    }
    Ok(())
}
fn increasing_build(current: u32, previous: u32) -> Result<()> {
    if !(1..=9999).contains(&current) || !(1..=9999).contains(&previous) || current <= previous {
        return Err(format!(
            "product build {current} must exceed prior published build {previous} (range 1..9999)"
        )
        .into());
    }
    Ok(())
}
fn check_previous_build(current: u32) -> Result<()> {
    // Release checkout must fetch tags. Query committed metadata, never a separate counter.
    let tag = String::from_utf8(capture(Command::new("git").args([
        "describe",
        "--tags",
        "--abbrev=0",
        "--match",
        "v[0-9]*",
    ]))?)?;
    let text = String::from_utf8(capture(
        Command::new("git").args(["show", &format!("{}:Cargo.toml", tag.trim())]),
    )?)?;
    let manifest: toml::Value = toml::from_str(&text)?;
    let previous = manifest["workspace"]["metadata"]["auditorylab"]["build-number"]
        .as_integer()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("previous published build metadata missing")?;
    increasing_build(current, previous)
}
fn dist(target: &str, out: &Path) -> Result<()> {
    let platform = platform(target)?;
    let source = source::Source::resolve()?;
    if target.contains("apple") {
        archive::actool()?;
    }
    let rust_copyright = rust_standard_library_copyright()?;
    capture(Command::new("cargo").args(["about","--version"]))
        .map_err(|error| format!("cargo-about is required for dist; install it with its cli feature (see CONTRIBUTING.md).\n{error}"))?;
    let mut command = Command::new("cargo");
    command.args([
        "build",
        "--locked",
        "--profile",
        "dist",
        "-p",
        "auditorylab-cli",
        "-p",
        "auditorylab-gui",
        "--target",
        target,
    ]);
    if target.contains("apple") {
        command.env("MACOSX_DEPLOYMENT_TARGET", "15.0");
    }
    command
        .env("AUDITORYLAB_SOURCE_REVISION", &source.revision)
        .env("SOURCE_DATE_EPOCH", source.epoch.to_string());
    run(&mut command)?;
    let meta = metadata(Some(target))?;
    let version = version(&meta)?.to_owned();
    let build = build_number(&meta)?;
    let target_dir = PathBuf::from(
        meta["target_directory"]
            .as_str()
            .ok_or("target directory missing")?,
    );
    let scratch = target_dir.join("dist-stage");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    fs::create_dir_all(&scratch)?;
    fs::create_dir_all(out)?;
    let result: Result<()> = (|| {
        let mut executables = Vec::new();
        for (product, binary) in targets::PRODUCTS {
            let folder = scratch.join(format!("{binary}-{version}-{platform}"));
            fs::create_dir_all(&folder)?;
            let exe = if target.contains("windows") {
                format!("{binary}.exe")
            } else {
                binary.into()
            };
            let built = target_dir.join(target).join("dist").join(&exe);
            let destination = folder.join(&exe);
            fs::copy(built, &destination)?;
            let notice = about(product, target, &meta)?;
            println!("{product} notices: {} bytes", notice.len());
            fs::write(folder.join("THIRD-PARTY-NOTICES.txt"), notice)?;
            fs::write(
                folder.join("README.txt"),
                readme(&version, product == "gui", target),
            )?;
            fs::copy("LICENSE", folder.join("LICENSE"))?;
            fs::write(folder.join("COPYRIGHT-library.html"), &rust_copyright)?;
            fs::write(
                folder.join("SOURCE.txt"),
                source_notice(&version, build, &source),
            )?;
            if product == "gui" && target.contains("apple") {
                macos::bundle(&destination, &folder, &version, build)?;
                fs::remove_file(&destination)?;
            } else if target.contains("apple") {
                // Both architectures have the same declared ad-hoc signing contract.
                run(Command::new("codesign")
                    .args(["--sign", "-", "--force"])
                    .arg(&destination))?;
            }
            if product == "gui" && target.contains("linux") {
                fs::write(
                    folder.join("auditorylab-gui.desktop"),
                    "[Desktop Entry]\nType=Application\nName=Auditory Lab\nExec=auditorylab-gui\nIcon=org.resoltico.auditorylab\nTerminal=false\nCategories=Science;\n",
                )?;
                copy_tree(
                    Path::new("artwork/auditorylab-icon/assets/linux/hicolor"),
                    &folder.join("hicolor"),
                )?;
            }
            let name = folder
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("archive name missing")?;
            let format = archive::Format::for_product(product == "gui", target);
            let archive_path = out.join(format!("{name}.{}", format.extension()));
            archive::write(&folder, &archive_path, format, source.epoch)?;
            executables.push(package::inspect(
                &archive_path,
                &scratch.join("extracted"),
                target,
                &version,
                build,
                &rust_copyright,
                &source,
            )?);
            println!("Wrote {}", archive_path.display());
        }
        smoke(&executables[0], &scratch)?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(format!(
            "{error}\nFailed staging inputs retained at {}",
            scratch.display()
        )
        .into());
    }
    fs::remove_dir_all(&scratch)?;
    Ok(())
}
fn main() -> Result<()> {
    env::set_current_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("workspace root missing")?,
    )?;
    let args: Vec<_> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("dist") => { let mut target=None; let mut out=PathBuf::from("target/dist"); let mut i=1; while i<args.len() { let value=args.get(i+1).ok_or("missing option value")?; match args[i].as_str() { "--target"=>target=Some(value.as_str()),"--out"=>out=PathBuf::from(value),_=>return Err("use dist --target TARGET [--out DIR]".into()) } i+=2; } dist(target.ok_or("dist requires --target")?,&out) }
        Some("inspect-package") if args.len()==3 => { let meta=metadata(None)?; let _=package::inspect(Path::new(&args[2]), Path::new("target/package-inspection"), &args[1], version(&meta)?, build_number(&meta)?, &rust_standard_library_copyright()?, &source::Source::resolve()?)?; Ok(()) },
        Some("collect-assets") if args.len()==4 => targets::collect(&args[1], Path::new(&args[2]), Path::new(&args[3])),
        Some("notes") if args.len()==2 => { println!("{}",section(&fs::read_to_string("CHANGELOG.md")?,&args[1])?.1); Ok(()) }
        Some("check-release") if args.len()==2 => { let meta=metadata(None)?; if version(&meta)?!=args[1] { return Err("workspace version differs from requested release".into()); } let text=fs::read_to_string("CHANGELOG.md")?; release_notes(&text,&args[1])?; check_previous_build(build_number(&meta)?)?; rust_standard_library_copyright()?; Ok(()) }
        Some("notices") if args.len()==4 => { let meta=metadata(Some(&args[2]))?; fs::write(&args[3],about(&args[1],&args[2],&meta)?)?; Ok(()) }
        _=>Err("use cargo xtask dist --target TARGET [--out DIR], inspect-package TARGET ARCHIVE, collect-assets VERSION INPUT OUTPUT, notices PRODUCT TARGET OUTPUT, notes VERSION, or check-release VERSION".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_build_counter_must_increase_without_reset() {
        assert!(increasing_build(2, 1).is_ok());
        for (current, previous) in [(1, 1), (1, 2), (0, 1), (10000, 9999), (2, 0)] {
            assert!(increasing_build(current, previous).is_err());
        }
    }
    #[test]
    fn archive_names_and_readmes_follow_the_product_and_platform() {
        assert!(platform("universal-apple-darwin").is_err());
        assert_eq!(
            platform("aarch64-pc-windows-msvc").unwrap(),
            "windows-aarch64"
        );
        assert!(platform("unknown").is_err());
        let text = readme("0.4.0", true, "aarch64-unknown-linux-gnu");
        assert!(text.contains("Desktop") && text.contains("xdg-desktop-portal"));
        assert!(!text.contains("{version}"));
    }
    #[test]
    fn identity_metadata_rejects_missing_invalid_counters_and_prereleases() {
        for value in [
            serde_json::json!(null),
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(10000),
            serde_json::json!(1.5),
            serde_json::json!("1"),
        ] {
            assert!(
                build_number(
                    &serde_json::json!({"metadata":{"auditorylab":{"build-number":value}}})
                )
                .is_err()
            );
        }
        assert!(build_number(&serde_json::json!({})).is_err());
        assert_eq!(
            build_number(&serde_json::json!({"metadata":{"auditorylab":{"build-number":1}}}))
                .unwrap(),
            1
        );
        assert!(native_version("0.4.0").is_ok());
        for v in ["0.4.0-beta.1", "0.4", "", "1.a.0"] {
            assert!(native_version(v).is_err());
        }
    }
    #[test]
    fn release_checks_reject_malformed_dates_and_version_headings() {
        for date in ["2026-10-19", "2028-02-29", "2000-02-29"] {
            let text = format!("## [0.4.0] - {date}\n\nA change.\n");
            assert_eq!(release_notes(&text, "0.4.0").unwrap(), "A change.");
        }
        for date in [
            "2026-99-99",
            "2026-02-29",
            "2100-02-29",
            "2026-04-31",
            "0000-01-01",
            "2026-1-01",
            "+026-01-01",
            "2026-+1-01",
        ] {
            assert!(release_notes(&format!("## [0.4.0] - {date}\nA change."), "0.4.0").is_err());
        }
        for text in [
            "## [0.4.0]extra - 2026-10-19\nA change.",
            "prefix ## [0.4.0] - 2026-10-19\nA change.",
            "## [0.4.0] - 2026-10-19\n\n## [0.3.0]\nOlder.",
            "## [0.4.0] - 2026-10-19\n## [0.3.0]\nOlder.",
        ] {
            assert!(release_notes(text, "0.4.0").is_err());
        }
    }
    #[test]
    fn changelog_extraction_keeps_only_the_requested_version() {
        let text = "## [Unreleased]\n\n## [1.0] - 2026-10-18\n\nA change.\n\n## [0.9]\nOlder.";
        assert_eq!(section(text, "1.0").unwrap().1, "A change.");
        assert!(section(text, "1.1").is_err());
    }
}

//! Build, license and package Auditory Lab downloads.
mod archive;
mod macos;
mod notices;
mod package;
mod pe_icon;
mod source;
mod style;
mod targets;
mod vendor;
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
fn native_bundle_version(version: &str) -> Result<u32> {
    let parts = version
        .split('.')
        .map(|part| {
            if part.is_empty()
                || part.len() > 1 && part.starts_with('0')
                || !part.bytes().all(|b| b.is_ascii_digit())
            {
                return Err("native packaging requires X.Y.Z without suffixes".into());
            }
            part.parse::<u32>().map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    let [major, minor, patch] = parts.as_slice() else {
        return Err("native packaging requires X.Y.Z without suffixes".into());
    };
    if *minor > 99 || *patch > 99 {
        return Err("native packaging requires minor and patch at most 99".into());
    }
    major
        .checked_mul(10000)
        .and_then(|n| n.checked_add(minor * 100))
        .and_then(|n| n.checked_add(*patch))
        .ok_or_else(|| "native bundle version overflow".into())
}
fn version(metadata: &Value) -> Result<&str> {
    metadata["packages"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["name"] == "auditorylab-app"))
        .and_then(|p| p["version"].as_str())
        .ok_or_else(|| "workspace version missing".into())
}

fn rust_version() -> Result<String> {
    let source = fs::read_to_string("rust-toolchain.toml")?;
    source
        .lines()
        .find_map(|l| l.strip_prefix("channel = "))
        .map(|v| v.trim_matches('"').to_owned())
        .ok_or_else(|| "pinned Rust version missing".into())
}
#[expect(
    clippy::unnecessary_debug_formatting,
    reason = "Native diagnostic paths retain escaped, quoted bytes, including non-UTF-8 names"
)]
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
    let os = if target.contains("apple") {
        "macos"
    } else if target.contains("windows") {
        "windows"
    } else {
        "linux"
    };
    let start = match (gui, os) {
        (false, _) => {
            "Run auditorylab --help, then auditorylab list. Use auditorylab describe \
            builtin:ID to read about a sound, and auditorylab recipe builtin:ID \
            --output recipe.toml to edit one."
        }
        (true, "macos") => {
            "Open Auditory Lab.app. Select a sound, read its summary, then use \
            Play selected sound or Export WAV…."
        }
        (true, "windows") => {
            "Open auditorylab-gui.exe. Select a sound, read its summary, then use \
            Play selected sound or Export WAV…."
        }
        (true, _) => {
            "Open auditorylab-gui. Select a sound, read its summary, then use \
            Play selected sound or Export WAV…."
        }
    };
    let needs = match (gui, os) {
        (_, "macos") => {
            "macOS 15 or later. If first launch is blocked, use System Settings > \
            Privacy & Security > Open Anyway after trying to open the app. Apple \
            instructions: https://support.apple.com/en-us/102445"
        }
        (true, "windows") => {
            "Windows 10 or later on x86_64; Windows 11 on ARM64. Desktop drawing \
            needs an OpenGL 2.0 or newer graphics driver."
        }
        (false, "windows") => "Windows 10 or later on x86_64; Windows 11 on ARM64.",
        (true, _) => {
            "Linux with glibc 2.35 or later, ALSA, X11 or Wayland, fontconfig, \
            xkbcommon (including its X11 library on X11), fonts, GLib gsettings for \
            GNOME Wayland text scaling, OpenGL and an xdg-desktop-portal for file \
            dialogs."
        }
        (false, _) => "Linux with glibc 2.35 or later and ALSA.",
    };
    let notes = match (gui, os) {
        (false, _) => "",
        (true, "macos") => {
            "\nAll legal materials are also inside the app at Contents/Resources.\n\
            The About window provides Licenses and component notices.\n"
        }
        (true, "windows") => "\nThe About window provides Licenses and component notices.\n",
        (true, _) => {
            "\nThe About window provides Licenses and component notices.\n\n\
            For a desktop launcher, keep the extracted folder and its legal files\n\
            together and add that folder to PATH. Install auditorylab-gui.desktop under\n\
            ~/.local/share/applications/ and merge hicolor/ into \
            ~/.local/share/icons/hicolor/.\nKeep icon names unchanged.\n"
        }
    };
    include_str!("../templates/archive-readme.txt")
        .replace("{version}", version)
        .replace("{product}", if gui { "Desktop" } else { "CLI" })
        .replace("{start}", start)
        .replace("{platform_needs}", needs)
        .replace("{platform_notes}\n", notes)
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
fn version_line(binary: &str, version: &str, revision: Option<&str>) -> Result<String> {
    let source = match revision {
        Some(rev) => {
            if !matches!(rev.len(), 40 | 64) || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(
                    "AUDITORYLAB_SOURCE_REVISION must be a full 40- or 64-character ASCII \
                    hex commit ID"
                        .into(),
                );
            }
            format!(" ({rev})")
        }
        None => String::new(),
    };
    Ok(format!("{binary} {version}{source}"))
}
#[expect(
    clippy::unnecessary_debug_formatting,
    reason = "Native diagnostic paths retain escaped, quoted bytes, including non-UTF-8 names"
)]
fn smoke(cli: &Path, scratch: &Path) -> Result<()> {
    let listing = String::from_utf8(capture(Command::new(cli).args([
        "list",
        "--collection",
        "builtin",
    ]))?)?;
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
        child.stdin.take().ok_or("stdin missing")?.write_all(
            b"schema_version=5\n[timing]\nduration_seconds=0.000125\n\
                fade_seconds=0\n[sound]\ncomponent=[]\n\
                [document]\nid='00000000-0000-4000-8000-000000000001'\n",
        )?;
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
fn source_notice(version: &str, source: &source::Source) -> String {
    format!(
        "Auditory Lab {version}\nSource revision: {}\nSource commit epoch: {} \
            (Unix UTC seconds)\nCorresponding source: \
            https://github.com/resoltico/AuditoryLab/tree/{}\nRelease source \
            archives: https://github.com/resoltico/AuditoryLab/releases\nSoftware \
            uses MPL-2.0; owner artwork has separate terms in \
            LICENSE.\nRedistributors must provide corresponding MPL-covered \
            source and tell recipients how to obtain it.\n",
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

fn dist(target: &str, out: &Path, icons: Option<&Path>) -> Result<()> {
    let platform = platform(target)?;
    let meta = metadata(Some(target))?;
    let version = version(&meta)?.to_owned();
    let bundle_version = native_bundle_version(&version)?;
    let source = source::Source::resolve()?;
    let rust_copyright = distribution_tools(target, icons, &source)?;
    build_binaries(target, &source)?;
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
            fs::write(folder.join("SOURCE.txt"), source_notice(&version, &source))?;
            if product == "gui" && target.contains("apple") {
                macos::bundle(
                    &destination,
                    &folder,
                    &version,
                    bundle_version,
                    icons,
                    &source,
                )?;
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
                    "[Desktop Entry]\nType=Application\nName=Auditory \
                        Lab\nExec=auditorylab-gui\nIcon=org.resoltico.auditorylab\nTerminal=fa\
                        lse\nCategories=Science;\n",
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
                bundle_version,
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
        Some("check-style") if args.len() == 1 => style::check(),
        Some("verify-vendor") if args.len() == 1 => vendor::verify(),
        Some("dist") => run_dist(&args),
        Some("compile-icons") if args.len() == 2 => {
            macos::compile_icons(Path::new(&args[1]), &source::Source::resolve()?)
        }
        Some("inspect-package") if args.len() == 3 => {
            let meta = metadata(None)?;
            let _ = package::inspect(
                Path::new(&args[2]),
                Path::new(
                    "target/pac\
            kage-inspection",
                ),
                &args[1],
                version(&meta)?,
                native_bundle_version(version(&meta)?)?,
                &rust_standard_library_copyright()?,
                &source::Source::resolve()?,
            )?;
            Ok(())
        }
        Some("collect-assets") if args.len() == 4 => {
            targets::collect(&args[1], Path::new(&args[2]), Path::new(&args[3]))
        }
        Some("notes") if args.len() == 2 => {
            println!(
                "{}",
                section(
                    &fs::read_to_string(
                        "CHANGELOG.m\
            d"
                    )?,
                    &args[1]
                )?
                .1
            );
            Ok(())
        }
        Some("check-release") if args.len() == 2 => {
            let meta = metadata(None)?;
            if version(&meta)? != args[1] {
                return Err("workspace \
            version differs from requested release"
                    .into());
            }
            let text = fs::read_to_string(
                "CHANGELOG.\
            md",
            )?;
            release_notes(&text, &args[1])?;
            rust_standard_library_copyright()?;
            Ok(())
        }
        Some("notices") if args.len() == 4 => {
            let meta = metadata(Some(&args[2]))?;
            fs::write(&args[3], about(&args[1], &args[2], &meta)?)?;
            Ok(())
        }
        _ => Err(
            "use cargo xtask check-style, verify-vendor, dist --target TARGET [--out DIR] \
            [--icons DIR], compile-icons OUTPUT, inspect-package TARGET ARCHIVE, \
            collect-assets VERSION INPUT OUTPUT, notices PRODUCT TARGET OUTPUT, \
            notes VERSION, or check-release VERSION"
                .into(),
        ),
    }
}

fn distribution_tools(
    target: &str,
    icons: Option<&Path>,
    source: &source::Source,
) -> Result<Vec<u8>> {
    if let Some(icons) = icons {
        if !target.contains("apple") {
            return Err("--icons is supported only for macOS distributions".into());
        }
        macos::validate_icons(icons, source)?;
    } else if target.contains("apple") {
        archive::actool()?;
    }
    let rust_copyright = rust_standard_library_copyright()?;
    capture(Command::new("cargo").args(["about", "--version"])).map_err(|error| {
        format!(
            "cargo-about is required for dist; install it with its cli \
            feature (see CONTRIBUTING.md).\n{error}"
        )
    })?;
    Ok(rust_copyright)
}
fn build_binaries(target: &str, source: &source::Source) -> Result<()> {
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
    run(&mut command)
}

fn run_dist(args: &[String]) -> Result<()> {
    let mut target = None;
    let mut icons = None;
    let mut out = PathBuf::from(
        "target/dis\
            t",
    );
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1).ok_or(
            "missing \
            option value",
        )?;
        match args[i].as_str() {
            "--target" => target = Some(value.as_str()),
            "--out" => out = PathBuf::from(value),
            "--icons" => icons = Some(Path::new(value)),
            _ => {
                return Err("use dist \
            --target TARGET [--out DIR] [--icons DIR]"
                    .into());
            }
        }
        i += 2;
    }
    dist(
        target.ok_or(
            "dist \
            requires --target",
        )?,
        &out,
        icons,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_bundle_identity_is_derived_and_rejects_unsupported_versions() {
        for (version, expected) in [
            ("0.7.0", 700),
            ("0.8.0", 800),
            ("1.2.3", 10203),
            ("1.99.99", 19999),
        ] {
            assert_eq!(native_bundle_version(version).unwrap(), expected);
        }
        assert!(native_bundle_version("0.7.0").unwrap() > 4);
        for version in [
            "0.100.0",
            "0.0.100",
            "0.8.0-beta.1",
            "0.8.0+local",
            "0.8",
            "",
            "1.a.0",
            "01.2.3",
            "999999999.0.0",
        ] {
            assert!(native_bundle_version(version).is_err(), "{version}");
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
        assert!(text.contains("Install auditorylab-gui.desktop under"));
        assert!(!text.contains("{version}"));
    }
    #[test]
    fn each_readme_names_only_its_own_platform() {
        let cases = [
            (
                "aarch64-apple-darwin",
                true,
                "Open Auditory Lab.app. Select a sound",
            ),
            (
                "x86_64-pc-windows-msvc",
                true,
                "Open auditorylab-gui.exe. Select a sound",
            ),
            (
                "x86_64-unknown-linux-gnu",
                true,
                "Open auditorylab-gui. Select a sound",
            ),
            ("aarch64-pc-windows-msvc", false, "Run auditorylab --help"),
        ];
        for (target, gui, start) in cases {
            let text = readme("0.4.0", gui, target);
            assert!(text.contains(start), "{target}");
            assert!(!text.contains('{'), "{target}: unreplaced placeholder");
            assert!(text.ends_with(".\n") && !text.ends_with("\n\n"), "{target}");
            let mentions = [
                (
                    "macos",
                    ["macOS", "Auditory Lab.app", "Contents/Resources"].as_slice(),
                ),
                ("windows", ["Windows", ".exe"].as_slice()),
                ("linux", ["Linux", ".desktop", "hicolor"].as_slice()),
            ];
            for (os, words) in mentions {
                if !target.contains(if os == "macos" { "apple" } else { os }) {
                    for word in words {
                        assert!(!text.contains(word), "{target} mentions {word}");
                    }
                }
            }
            assert_eq!(text.contains("About window"), gui, "{target}");
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
    #[test]
    fn product_version_comes_from_the_renamed_application_package() {
        let metadata = serde_json::json!({"packages": [
            {"name": "auditorylab-core", "version": "9.0.0"},
            {"name": "auditorylab-app", "version": "1.2.3"}
        ]});
        assert_eq!(super::version(&metadata).unwrap(), "1.2.3");
        let retired_name =
            serde_json::json!({"packages": [{"name": "auditorylab", "version": "1.2.3"}]});
        assert!(super::version(&retired_name).is_err());
    }
}

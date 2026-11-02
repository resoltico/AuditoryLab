//! Inspect the extracted deliverable, including exact macOS architecture and identity.
use crate::{Result, capture, run};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn architecture(actual: &str, expected: &str) -> Result<()> {
    if actual.split_whitespace().collect::<Vec<_>>() != [expected] {
        return Err(format!(
            "expected exactly {expected}; extracted Mach-O architectures: {actual:?}"
        )
        .into());
    }
    Ok(())
}
// Check the file's machine, not whether the host can execute it through emulation.
fn architecture_header(bytes: &[u8], target: &str) -> Result<()> {
    let arm = target.starts_with("aarch64-");
    let valid = if target.contains("linux") {
        let machine: u16 = if arm { 183 } else { 62 };
        bytes.len() >= 64
            && bytes.get(..7) == Some(b"\x7fELF\x02\x01\x01")
            && bytes.get(18..20) == Some(machine.to_le_bytes().as_slice())
    } else if target.contains("windows") {
        let machine: u16 = if arm { 0xaa64 } else { 0x8664 };
        let header = bytes
            .get(0x3c..0x40)
            .and_then(|offset| offset.try_into().ok())
            .map(u32::from_le_bytes)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| bytes.get(offset..offset.checked_add(26)?));
        bytes.get(..2) == Some(b"MZ")
            && header.is_some_and(|header| {
                &header[..4] == b"PE\0\0"
                    && header[4..6] == machine.to_le_bytes()
                    && header[24..26] == [0x0b, 0x02]
            })
    } else {
        false
    };
    if !valid {
        return Err(format!("extracted executable header does not match {target}").into());
    }
    Ok(())
}
/// What the executable carries: no build-host paths, and a notice for every
/// embedded font.
fn executable_content(root: &Path, executable: &Path) -> Result<()> {
    let built = fs::read(executable)?;
    let mut hosts = vec![std::env::current_dir()?];
    hosts.push(std::path::absolute(crate::cargo_home()?)?);
    leaks_no_host_path(&built, &hosts)?;
    let notices = fs::read_to_string(root.join("THIRD-PARTY-NOTICES.txt"))?;
    for font in crate::fonts::embedded(&built) {
        if font.copyright.trim().is_empty() || !notices.contains(font.copyright.trim()) {
            return Err(format!("embedded font {:?} is not credited in notices", font.name).into());
        }
    }
    Ok(())
}
/// Compiled-in source locations are remapped, so no build-host directory remains.
fn leaks_no_host_path(bytes: &[u8], hosts: &[PathBuf]) -> Result<()> {
    for host in hosts {
        let host = host.to_string_lossy();
        if host.len() > 3
            && bytes
                .windows(host.len())
                .any(|window| window == host.as_bytes())
        {
            return Err(format!("executable embeds the build-host path {host}").into());
        }
    }
    Ok(())
}
/// Windows executable protections and process identity that the loader reads.
///
/// Requires ASLR with high entropy, DEP and Control Flow Guard; no dynamically
/// linked C runtime (it is linked statically); and the product's exact manifest.
fn windows_executable(bytes: &[u8], desktop: bool) -> Result<()> {
    const REQUIRED: u16 = 0x0020 | 0x0040 | 0x0100 | 0x4000;
    let characteristics = bytes
        .get(0x3c..0x40)
        .and_then(|offset| usize::try_from(u32::from_le_bytes(offset.try_into().ok()?)).ok())
        .and_then(|pe| bytes.get(pe + 24 + 70..pe + 24 + 72))
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
        .ok_or("PE optional header is truncated")?;
    if characteristics & REQUIRED != REQUIRED {
        return Err(format!(
            "Windows executable lacks ASLR, high-entropy ASLR, DEP or Control Flow Guard: \
            DllCharacteristics {characteristics:#06x}"
        )
        .into());
    }
    let lower = bytes.to_ascii_lowercase();
    for runtime in [&b"vcruntime140"[..], b"msvcp140", b"api-ms-win-crt-"] {
        if lower.windows(runtime.len()).any(|window| window == runtime) {
            return Err(format!(
                "Windows executable imports a dynamic C runtime: {}",
                String::from_utf8_lossy(runtime)
            )
            .into());
        }
    }
    let kind = if desktop {
        crate::source::identity_input::Executable::Desktop
    } else {
        crate::source::identity_input::Executable::Cli
    };
    let manifest = crate::source::identity_input::windows_manifest(kind);
    if !bytes
        .windows(manifest.len())
        .any(|window| window == manifest.as_bytes())
    {
        return Err("Windows executable lacks its exact application manifest".into());
    }
    Ok(())
}
pub(crate) fn inspect(
    archive: &Path,
    out: &Path,
    target: &str,
    version: &str,
    bundle_version: u32,
    rust_copyright: &[u8],
    source: &crate::source::Source,
) -> Result<PathBuf> {
    let platform = crate::targets::platform(target)?;
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive filename")?;
    let binary = if name.starts_with("auditorylab-gui-") {
        "auditorylab-gui"
    } else {
        "auditorylab"
    };
    let stem = format!("{binary}-{version}-{platform}");
    let format = crate::archive::Format::for_product(binary == "auditorylab-gui", target);
    let expected = format!("{stem}.{}", format.extension());
    if name != expected {
        return Err(format!("expected archive {expected}; got {name}").into());
    }
    fs::create_dir_all(out)?;
    let root = out.join(&stem);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    crate::archive::verify_metadata(archive, format, source.epoch)?;
    crate::archive::extract(archive, out, format)?;
    for file in ["LICENSE", "README.txt", "THIRD-PARTY-NOTICES.txt"] {
        if fs::read(root.join(file))?.is_empty() {
            return Err(format!("empty package member {file}").into());
        }
    }
    let actual = fs::read(root.join("COPYRIGHT-library.html"))
        .map_err(|error| format!("cannot read packaged Rust standard-library notices: {error}"))?;
    if actual != rust_copyright {
        return Err(
            "packaged Rust standard-library notices differ from the compiler's original".into(),
        );
    }
    if fs::read(root.join("LICENSE"))? != include_bytes!("../../LICENSE") {
        return Err("package LICENSE differs from complete software/artwork terms".into());
    }
    if fs::read_to_string(root.join("SOURCE.txt"))? != crate::source_notice(version, source) {
        return Err("package source availability identity differs".into());
    }
    let app = root.join("Auditory Lab.app");
    let executable = if binary == "auditorylab-gui" && target.contains("apple") {
        app.join("Contents/MacOS/auditorylab-gui")
    } else {
        root.join(format!(
            "{binary}{}",
            if target.contains("windows") {
                ".exe"
            } else {
                ""
            }
        ))
    };
    executable_content(&root, &executable)?;
    #[cfg(unix)]
    if !target.contains("windows") {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&executable)?.permissions().mode() & 0o111 == 0 {
            return Err("extracted binary has no executable permission".into());
        }
    }
    if target.contains("apple") {
        inspect_macos(&root, &executable, binary, target, version, bundle_version)?;
    } else {
        let bytes = fs::read(&executable)?;
        architecture_header(&bytes, target)?;
        if target.contains("windows") {
            windows_executable(&bytes, binary == "auditorylab-gui")?;
            inspect_windows_identity(&executable, binary, version)?;
        }
        if binary == "auditorylab-gui" && target.contains("windows") {
            crate::pe_icon::verify(
                &bytes,
                include_bytes!("../../artwork/auditorylab-icon/assets/windows/auditorylab.ico"),
            )?;
        }
        if binary == "auditorylab-gui" && target.contains("linux") {
            let source = Path::new("artwork/auditorylab-icon/assets/linux/hicolor");
            compare_tree(source, &root.join("hicolor"))?;
            let entry = fs::read_to_string(root.join(format!("{binary}.desktop")))?;
            for line in [
                "Icon=org.resoltico.auditorylab".to_owned(),
                format!("Exec={binary}"),
                format!("StartupWMClass={binary}"),
            ] {
                if !entry.lines().any(|actual| actual == line) {
                    return Err(format!("desktop entry lacks {line}").into());
                }
            }
        }
    }
    let actual = String::from_utf8(capture(Command::new(&executable).arg("--version"))?)?;
    if actual.trim() != crate::version_line(binary, version, Some(&source.revision))? {
        return Err(format!("extracted executable identity differs: {actual:?}").into());
    }
    println!("Extracted package identity/content/architecture checks passed: {name}");
    Ok(executable)
}
fn compare_tree(source: &Path, actual: &Path) -> Result<()> {
    let mut expected: Vec<_> = fs::read_dir(source)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    let mut found: Vec<_> = fs::read_dir(actual)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    expected.sort();
    found.sort();
    if expected != found {
        return Err("Linux icon tree members differ from supplied artwork".into());
    }
    for name in expected {
        let source = source.join(&name);
        let actual = actual.join(name);
        if source.is_dir() {
            compare_tree(&source, &actual)?;
        } else if fs::read(source)? != fs::read(actual)? {
            return Err("Linux icon bytes differ from supplied artwork".into());
        }
    }
    Ok(())
}

fn inspect_windows_identity(executable: &Path, binary: &str, version: &str) -> Result<()> {
    if !cfg!(target_os = "windows") {
        return Err("Windows resource inspection requires a native Windows host".into());
    }
    let manifest: toml::Value = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
    let product = crate::source::identity_input::product(&manifest)?;
    let kind = if binary == "auditorylab-gui" {
        crate::source::identity_input::Executable::Desktop
    } else {
        crate::source::identity_input::Executable::Cli
    };
    let output = capture(
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "\n[Console]::OutputEncoding = \
            [System.Text.UTF8Encoding]::new($false)\n$v = (Get-Item -LiteralPath \
            $env:AUDITORYLAB_INSPECT_EXECUTABLE).VersionInfo\n@{\n    file = \
            @($v.FileMajorPart, $v.FileMinorPart, $v.FileBuildPart, \
            $v.FilePrivatePart)\n    product = @($v.ProductMajorPart, \
            $v.ProductMinorPart, $v.ProductBuildPart, $v.ProductPrivatePart)\n    \
            fileVersion = $v.FileVersion\n    productVersion = \
            $v.ProductVersion\n    name = $v.ProductName\n    copyright = \
            $v.LegalCopyright\n    description = $v.FileDescription\n} | \
            ConvertTo-Json -Compress\n",
            ])
            .env("AUDITORYLAB_INSPECT_EXECUTABLE", executable),
    )?;
    let actual: serde_json::Value = serde_json::from_slice(&output)?;
    let mut components = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    components.push(0);
    let native = format!("{version}.0");
    let expected = serde_json::json!({"file": components, "product": components,
        "fileVersion": native, "productVersion": native,
        "name": product.name, "copyright": product.copyright,
        "description": crate::source::identity_input::file_description(product, kind)});
    if actual != expected {
        return Err(format!("Windows VERSIONINFO differs: {actual} vs {expected}").into());
    }
    Ok(())
}

fn inspect_macos(
    root: &Path,
    executable: &Path,
    binary: &str,
    target: &str,
    version: &str,
    bundle_version: u32,
) -> Result<()> {
    let app = root.join("Auditory Lab.app");

    let arches = String::from_utf8(capture(Command::new("lipo").arg("-archs").arg(executable))?)?;
    architecture(
        arches.trim(),
        if target.starts_with("aarch64") {
            "arm64"
        } else {
            "x86_64"
        },
    )?;
    run(Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(executable))?;
    if binary == "auditorylab-gui" {
        for file in crate::LEGAL_FILES {
            if fs::read(app.join("Contents/Resources").join(file))? != fs::read(root.join(file))? {
                return Err(format!("bundled legal resource differs: {file}").into());
            }
        }
        for file in crate::macos::BUNDLED_ICON_FILES {
            if fs::metadata(app.join("Contents/Resources").join(file))?.len() == 0 {
                return Err(format!("missing icon resource: {file}").into());
            }
        }
        let mut resources: Vec<_> = fs::read_dir(app.join("Contents/Resources"))?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;
        resources.sort();
        let mut expected: Vec<_> = crate::macos::BUNDLED_ICON_FILES
            .iter()
            .chain(crate::LEGAL_FILES.iter())
            .map(std::ffi::OsString::from)
            .collect();
        expected.sort();
        if resources != expected {
            return Err(format!("unexpected app resources: {resources:?}").into());
        }
        let manifest: toml::Value = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
        let product = crate::source::identity_input::product(&manifest)?;
        for (key, expected) in [
            ("NSHumanReadableCopyright", product.copyright.to_owned()),
            ("CFBundleShortVersionString", version.to_owned()),
            ("CFBundleVersion", bundle_version.to_string()),
            ("LSMinimumSystemVersion", "15.0".into()),
            ("CFBundleIconName", "AppIcon".into()),
            // OpenGL apps stay on the integrated GPU of dual-GPU Macs only with this key.
            ("NSSupportsAutomaticGraphicsSwitching", "true".into()),
        ] {
            let actual = String::from_utf8(capture(
                Command::new("/usr/libexec/PlistBuddy")
                    .args(["-c", &format!("Print :{key}")])
                    .arg(app.join("Contents/Info.plist")),
            )?)?;
            if actual.trim() != expected {
                return Err(
                    format!("extracted plist {key} differs: {actual:?} vs {expected}").into(),
                );
            }
        }
        run(Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn archives_require_the_complete_original_rust_component_notices() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("auditorylab-0.4.0-windows-x86_64");
        fs::create_dir(&folder).unwrap();
        for name in [
            "LICENSE",
            "README.txt",
            "THIRD-PARTY-NOTICES.txt",
            "auditorylab.exe",
        ] {
            fs::write(folder.join(name), "fixture").unwrap();
        }
        fs::write(folder.join("LICENSE"), include_bytes!("../../LICENSE")).unwrap();
        fs::write(
            folder.join("SOURCE.txt"),
            crate::source_notice(
                "0.4.0",
                &crate::source::Source {
                    revision: "a".repeat(40),
                    epoch: 1_709_251_199,
                },
            ),
        )
        .unwrap();
        let archive = temp.path().join("auditorylab-0.4.0-windows-x86_64.zip");
        let expected = b"complete upstream Rust copyright fixture";
        for contents in [
            None,
            Some(&b""[..]),
            Some(&b"truncated"[..]),
            Some(&expected[..]),
        ] {
            if let Some(contents) = contents {
                fs::write(folder.join("COPYRIGHT-library.html"), contents).unwrap();
            }
            crate::archive::write(
                &folder,
                &archive,
                crate::archive::Format::Zip,
                1_709_251_199,
            )
            .unwrap();
            let error = super::inspect(
                &archive,
                &temp.path().join("extracted"),
                "x86_64-pc-windows-msvc",
                "0.4.0",
                1,
                expected,
                &crate::source::Source {
                    revision: "a".repeat(40),
                    epoch: 1_709_251_199,
                },
            )
            .unwrap_err()
            .to_string();
            if contents == Some(&expected[..]) {
                assert!(error.contains("executable header"), "{error}");
            } else {
                assert!(error.contains("Rust standard-library"), "{error}");
            }
        }
    }
    #[test]
    fn elf_and_pe_headers_reject_emulated_wrong_architectures_and_truncation() {
        let x64_linux = "x86_64-unknown-linux-gnu";
        let arm_linux = "aarch64-unknown-linux-gnu";
        let mut elf = [0_u8; 64];
        elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        elf[18..20].copy_from_slice(&[0x3e, 0]);
        assert!(super::architecture_header(&elf, x64_linux).is_ok());
        assert!(super::architecture_header(&elf, arm_linux).is_err());
        elf[18..20].copy_from_slice(&[0xb7, 0]);
        assert!(super::architecture_header(&elf, arm_linux).is_ok());
        assert!(super::architecture_header(&elf, x64_linux).is_err());
        for end in 0..elf.len() {
            assert!(super::architecture_header(&elf[..end], arm_linux).is_err());
        }
        for (index, value) in [(0, 0), (4, 1), (5, 2), (6, 0)] {
            let mut invalid = elf;
            invalid[index] = value;
            assert!(super::architecture_header(&invalid, arm_linux).is_err());
        }

        let x64_windows = "x86_64-pc-windows-msvc";
        let arm_windows = "aarch64-pc-windows-msvc";
        let mut pe = [0_u8; 0x80 + 26];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&[0x64, 0x86]);
        pe[0x98..0x9a].copy_from_slice(&[0x0b, 0x02]);
        assert!(super::architecture_header(&pe, x64_windows).is_ok());
        assert!(super::architecture_header(&pe, arm_windows).is_err());
        pe[0x84..0x86].copy_from_slice(&[0x64, 0xaa]);
        assert!(super::architecture_header(&pe, arm_windows).is_ok());
        assert!(super::architecture_header(&pe, x64_windows).is_err());
        for end in 0..pe.len() {
            assert!(super::architecture_header(&pe[..end], arm_windows).is_err());
        }
        for (index, value) in [(0, 0), (0x80, 0), (0x98, 0x07), (0x99, 0x01)] {
            let mut invalid = pe;
            invalid[index] = value;
            assert!(super::architecture_header(&invalid, arm_windows).is_err());
        }
        pe[0x3c..0x40].copy_from_slice(&[0xff; 4]);
        assert!(super::architecture_header(&pe, arm_windows).is_err());
    }
    #[test]
    fn fat_and_wrong_architecture_binaries_are_rejected() {
        assert!(super::architecture("arm64", "arm64").is_ok());
        assert!(super::architecture("x86_64", "x86_64").is_ok());
        for actual in ["arm64 x86_64", "x86_64 arm64", "x86_64", "arm64 arm64", ""] {
            assert!(super::architecture(actual, "arm64").is_err());
        }
    }
    #[test]
    fn windows_executables_need_protections_a_static_runtime_and_their_manifest() {
        use crate::source::identity_input::{Executable, windows_manifest};
        let image = |characteristics: u16, extra: &[u8]| {
            let mut bytes = vec![0_u8; 0x200];
            bytes[0x3c] = 0x80;
            bytes[0x80 + 24 + 70..0x80 + 24 + 72].copy_from_slice(&characteristics.to_le_bytes());
            bytes.extend_from_slice(extra);
            bytes
        };
        let gui = windows_manifest(Executable::Desktop);
        let cli = windows_manifest(Executable::Cli);
        assert!(super::windows_executable(&image(0x4160, gui.as_bytes()), true).is_ok());
        assert!(super::windows_executable(&image(0xc160, cli.as_bytes()), false).is_ok());
        // Control Flow Guard, DEP, ASLR or high-entropy ASLR missing.
        for missing in [0x0160, 0x4060, 0x4120, 0x4140] {
            let error = super::windows_executable(&image(missing, gui.as_bytes()), true);
            assert!(
                error
                    .unwrap_err()
                    .to_string()
                    .contains("DllCharacteristics")
            );
        }
        let mut dynamic = gui.as_bytes().to_vec();
        dynamic.extend_from_slice(b"VCRUNTIME140.dll");
        assert!(super::windows_executable(&image(0x4160, &dynamic), true).is_err());
        // The desktop needs its own manifest; the CLI one lacks Common Controls 6.
        assert!(super::windows_executable(&image(0x4160, cli.as_bytes()), true).is_err());
        assert!(super::windows_executable(&image(0x4160, b""), false).is_err());
        assert!(super::windows_executable(&[0; 0x40], false).is_err());
    }
    #[test]
    fn executables_must_not_embed_build_host_paths() {
        let hosts = [std::path::PathBuf::from("/home/runner/work/repo")];
        assert!(super::leaks_no_host_path(b"panic at /auditorylab/src/x.rs", &hosts).is_ok());
        let leaked = b"panic at /home/runner/work/repo/src/x.rs";
        assert!(super::leaks_no_host_path(leaked, &hosts).is_err());
    }
}

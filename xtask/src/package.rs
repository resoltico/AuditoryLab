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
pub(crate) fn inspect(
    archive: &Path,
    out: &Path,
    target: &str,
    version: &str,
    build: u32,
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
    let expected = format!(
        "{stem}.{}",
        if target.contains("windows") {
            "zip"
        } else {
            "tar.gz"
        }
    );
    if name != expected {
        return Err(format!("expected archive {expected}; got {name}").into());
    }
    fs::create_dir_all(out)?;
    let root = out.join(&stem);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    if target.contains("windows") {
        zip::ZipArchive::new(fs::File::open(archive)?)?.extract(out)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?)).unpack(out)?;
    }
    for file in ["LICENSE", "README.txt", "THIRD-PARTY-NOTICES.txt"] {
        if fs::read(root.join(file))?.is_empty() {
            return Err(format!("empty package member {file}").into());
        }
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
    #[cfg(unix)]
    if !target.contains("windows") {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&executable)?.permissions().mode() & 0o111 == 0 {
            return Err("extracted binary has no executable permission".into());
        }
    }
    if !target.contains("apple") {
        architecture_header(&fs::read(&executable)?, target)?;
    } else {
        let arches = String::from_utf8(capture(
            Command::new("lipo").arg("-archs").arg(&executable),
        )?)?;
        architecture(
            arches.trim(),
            if target.starts_with("aarch64") {
                "arm64"
            } else {
                "x86_64"
            },
        )?;
        if binary == "auditorylab-gui" {
            for (key, expected) in [
                ("CFBundleShortVersionString", version.to_owned()),
                ("CFBundleVersion", build.to_string()),
                ("LSMinimumSystemVersion", "15.0".into()),
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
    }
    let actual = String::from_utf8(capture(Command::new(&executable).arg("--version"))?)?;
    if actual.trim() != crate::version_line(binary, version, build)? {
        return Err(format!("extracted executable identity differs: {actual:?}").into());
    }
    println!("Extracted package identity/content/architecture checks passed: {name}");
    Ok(executable)
}
#[cfg(test)]
mod tests {
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
}

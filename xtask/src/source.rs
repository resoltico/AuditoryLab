//! Frozen commit identity and deterministic archive time inputs.
use crate::{Result, capture};
use std::{env, process::Command};
// Identity generation and packaging share validation of the canonical workspace inputs.
#[path = "../../crates/app/identity_input.rs"]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Packaging and native-resource readers share the full identity validation module"
    )
)]
pub(crate) mod identity_input;

// ZIP's civil date range is the common representable range for every product.
pub(crate) fn epoch(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("SOURCE_DATE_EPOCH must be decimal Unix seconds".into());
    }
    let value: u64 = value.parse()?;
    if !(315_532_800..=4_354_819_199).contains(&value) {
        return Err(
            "SOURCE_DATE_EPOCH must be within 1980-01-01..2107-12-31 UTC (ZIP range)".into(),
        );
    }
    Ok(value)
}

pub(crate) struct Source {
    pub(crate) revision: String,
    pub(crate) epoch: u64,
}

impl Source {
    pub(crate) fn resolve() -> Result<Self> {
        Self::at(
            &env::current_dir()?,
            env::var("AUDITORYLAB_SOURCE_REVISION").ok(),
            env::var("SOURCE_DATE_EPOCH").ok(),
        )
    }
    fn at(
        directory: &std::path::Path,
        supplied_revision: Option<String>,
        supplied_epoch: Option<String>,
    ) -> Result<Self> {
        let git = if directory.join(".git").try_exists()? {
            Some(
                Command::new("git")
                    .current_dir(directory)
                    .args(["rev-parse", "--verify", "HEAD"])
                    .output()?,
            )
        } else {
            None
        };
        if let Some(git) = git {
            if !git.status.success() {
                return Err("Cannot resolve source HEAD from existing .git".into());
            }

            let revision = String::from_utf8(git.stdout)?.trim().to_owned();
            identity_input::revision(&revision)?;
            if supplied_revision
                .as_ref()
                .is_some_and(|value| !value.eq_ignore_ascii_case(&revision))
            {
                return Err("AUDITORYLAB_SOURCE_REVISION differs from checked-out HEAD".into());
            }
            let seconds =
                String::from_utf8(capture(Command::new("git").current_dir(directory).args([
                    "show",
                    "-s",
                    "--format=%ct",
                    &revision,
                ]))?)?;
            let resolved = epoch(seconds.trim())?;
            if supplied_epoch
                .as_ref()
                .map(|v| epoch(v))
                .transpose()?
                .is_some_and(|v| v != resolved)
            {
                return Err("SOURCE_DATE_EPOCH differs from the source commit timestamp".into());
            }
            Ok(Self {
                revision,
                epoch: resolved,
            })
        } else {
            let revision = supplied_revision.ok_or(
                "Source tree without Git requires \
                AUDITORYLAB_SOURCE_REVISION and SOURCE_DATE_EPOCH from the release \
                commit",
            )?;
            identity_input::revision(&revision)?;
            let seconds = supplied_epoch.ok_or(
                "Source tree without Git requires SOURCE_DATE_EPOCH from the release commit",
            )?;
            Ok(Self {
                revision,
                epoch: epoch(&seconds)?,
            })
        }
    }
}

struct TrackedSource {
    index: Vec<u8>,
    files: Vec<(std::path::PathBuf, Vec<u8>, bool)>,
}
impl TrackedSource {
    fn read(root: &std::path::Path) -> Result<Self> {
        use std::{
            fs,
            path::{Component, Path},
        };
        if !capture(Command::new("git").current_dir(root).args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
        ]))?
        .is_empty()
        {
            return Err("source archive refuses untracked files; review and add intended sources to Git first".into());
        }
        let index = capture(
            Command::new("git")
                .current_dir(root)
                .args(["ls-files", "--stage", "-z"]),
        )?;
        let mut files = Vec::new();
        for entry in index.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let entry = std::str::from_utf8(entry)?;
            let (fields, path) = entry.split_once('\t').ok_or("invalid Git index entry")?;
            let fields: Vec<_> = fields.split(' ').collect();
            if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") || fields[2] != "0" {
                return Err("source archive requires resolved stage-zero regular files".into());
            }
            let relative = Path::new(path);
            if !relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            {
                return Err("source archive has a non-relative tracked path".into());
            }
            let original = root.join(relative);
            if !fs::symlink_metadata(&original)?.is_file() {
                return Err(format!(
                    "source archive requires an ordinary tracked file: {}",
                    original.display()
                )
                .into());
            }
            files.push((
                relative.to_owned(),
                fs::read(original)?,
                fields[0] == "100755",
            ));
        }
        Ok(Self { index, files })
    }
    fn copy_to(&self, folder: &std::path::Path) -> Result<()> {
        for (relative, bytes, executable) in &self.files {
            let destination = folder.join(relative);
            std::fs::create_dir_all(destination.parent().ok_or("source parent missing")?)?;
            std::fs::write(&destination, bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    destination,
                    std::fs::Permissions::from_mode(if *executable { 0o755 } else { 0o644 }),
                )?;
            }
            #[cfg(not(unix))]
            let _ = executable;
        }
        Ok(())
    }
    fn verify(&self, root: &std::path::Path) -> Result<()> {
        let current = Self::read(root)?;
        if current.index != self.index {
            return Err(
                "Git index changed during source archive preparation; no archive was published"
                    .into(),
            );
        }
        for ((path, before, _), (_, after, _)) in self.files.iter().zip(&current.files) {
            if before != after {
                return Err(format!("Tracked source changed during archive preparation: {}; no archive was published", path.display()).into());
            }
        }
        Ok(())
    }
}

/// Package tracked working sources and Cargo's complete locked dependency graph.
/// General-purpose toolchains and platform system libraries are prerequisites.
pub(crate) fn archive(out: &std::path::Path) -> Result<()> {
    use std::fs;
    if out.exists() {
        return Err(format!("source archive output already exists: {}", out.display()).into());
    }
    let root = env::current_dir()?;
    let tracked = TrackedSource::read(&root)?;
    let meta = crate::metadata(None)?;
    let version = crate::version(&meta)?;
    let identity = Source::resolve()?;
    fs::create_dir_all("target")?;
    let scratch = tempfile::Builder::new()
        .prefix("source-archive-")
        .tempdir_in("target")?;
    let folder = scratch.path().join(format!("auditorylab-{version}-source"));
    fs::create_dir(&folder)?;
    tracked.copy_to(&folder)?;
    let vendor = folder.join("vendor");
    let generated = String::from_utf8(capture(
        Command::new("cargo")
            .args(["vendor", "--locked", "--versioned-dirs"])
            .arg(&vendor),
    )?)?;
    let original = fs::read_to_string(folder.join(".cargo/config.toml"))?;
    let mut config: toml::Value = toml::from_str(&original)?;
    let mut sources: toml::Value = toml::from_str(&generated)?;
    sources["source"]["vendored-sources"]["directory"] = toml::Value::String("vendor".into());
    if config.get("source").is_some() {
        return Err("source archive cannot replace existing Cargo source configuration".into());
    }
    config
        .as_table_mut()
        .ok_or("Cargo configuration must be a table")?
        .insert("source".into(), sources["source"].clone());
    fs::write(folder.join(".cargo/config.toml"), toml::to_string(&config)?)?;
    fs::write(
        folder.join("SOURCE.txt"),
        crate::source_notice(version, &identity),
    )?;
    // Verify the staged dependency graph can resolve without registry/network IO.
    crate::run(
        Command::new("cargo")
            .current_dir(&folder)
            .args(["metadata", "--locked", "--offline", "--format-version", "1"])
            .stdout(std::process::Stdio::null()),
    )?;
    let prepared = scratch.path().join("source.tar.gz");
    crate::archive::write(
        &folder,
        &prepared,
        crate::archive::Format::Tar,
        identity.epoch,
    )?;
    crate::archive::verify_metadata(&prepared, crate::archive::Format::Tar, identity.epoch)?;
    tracked.verify(&root)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    publish(&prepared, out)?;
    println!("Wrote corresponding source: {}", out.display());
    Ok(())
}

fn publish(prepared: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let mut input = std::fs::File::open(prepared)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|error| {
            format!(
                "cannot exclusively create source archive {}: {error}",
                out.display()
            )
        })?;
    std::io::copy(&mut input, &mut output).map_err(|error| {
        format!(
            "source archive copy failed; inspect incomplete output {}: {error}",
            out.display()
        )
    })?;
    output.sync_all()?;
    Ok(())
}

/// Convert Unix seconds to UTC civil fields; DOS ZIP stores seconds rounded down to even.
pub(crate) fn zip_time(seconds: u64) -> Result<zip::DateTime> {
    epoch(&seconds.to_string())?;
    let mut days = seconds / 86_400;
    let mut year: u16 = 1970;
    loop {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let length = if leap { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let lengths = [
        31,
        28 + u64::from(leap),
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
    let mut month: u8 = 1;
    for length in lengths {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    Ok(zip::DateTime::from_date_and_time(
        year,
        month,
        u8::try_from(days + 1)?,
        u8::try_from(seconds / 3600 % 24)?,
        u8::try_from(seconds / 60 % 60)?,
        u8::try_from(seconds % 60 / 2 * 2)?,
    )?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_publication_refuses_a_destination_created_after_preflight() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("prepared.tar.gz");
        let out = temp.path().join("published.tar.gz");
        std::fs::write(&source, "prepared source archive").unwrap();
        assert!(!out.exists());
        std::fs::write(&out, "concurrent owner bytes").unwrap();
        assert!(super::publish(&source, &out).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), b"concurrent owner bytes");
        assert_eq!(std::fs::read(&source).unwrap(), b"prepared source archive");
    }
    #[test]
    fn source_snapshot_rejects_changes_at_the_vendor_boundary() {
        use std::{fs, process::Command};
        for change in ["bytes", "index", "untracked"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            assert!(
                Command::new("git")
                    .current_dir(root)
                    .args(["init", "-q"])
                    .status()
                    .unwrap()
                    .success()
            );
            fs::write(root.join("source.rs"), "original source").unwrap();
            assert!(
                Command::new("git")
                    .current_dir(root)
                    .args(["add", "source.rs"])
                    .status()
                    .unwrap()
                    .success()
            );
            let snapshot = super::TrackedSource::read(root).unwrap();
            let staged = root.join("staged");
            // Staging is outside tracked input discovery in production (target/).
            fs::write(root.join(".git/info/exclude"), "staged/\narchive.tar.gz\n").unwrap();
            snapshot.copy_to(&staged).unwrap();
            assert_eq!(
                fs::read(staged.join("source.rs")).unwrap(),
                b"original source"
            );
            snapshot.verify(root).unwrap();
            match change {
                "bytes" => fs::write(root.join("source.rs"), "changed source").unwrap(),
                "index" => {
                    fs::write(root.join("source.rs"), "new staged source").unwrap();
                    assert!(
                        Command::new("git")
                            .current_dir(root)
                            .args(["add", "source.rs"])
                            .status()
                            .unwrap()
                            .success()
                    );
                }
                "untracked" => fs::write(root.join("new-build-input.rs"), "new source").unwrap(),
                _ => unreachable!(),
            }
            let out = root.join("archive.tar.gz");
            let result = snapshot.verify(root).and_then(|()| {
                fs::write(&out, "would publish archive")?;
                Ok(())
            });
            assert!(result.is_err(), "{change}");
            assert!(!out.exists(), "{change} must refuse before publishing");
        }
    }
    #[test]
    fn source_archives_require_both_validated_identity_inputs() {
        let temp = tempfile::tempdir().unwrap();
        assert!(super::Source::at(temp.path(), None, None).is_err());
        assert!(super::Source::at(temp.path(), Some("a".repeat(40)), None).is_err());
        assert!(
            super::Source::at(temp.path(), Some("short".into()), Some("1709251199".into()))
                .is_err()
        );
        assert!(super::Source::at(temp.path(), Some("a".repeat(40)), Some("0".into())).is_err());
        let source =
            super::Source::at(temp.path(), Some("a".repeat(40)), Some("1709251199".into()))
                .unwrap();
        assert_eq!(source.epoch, 1_709_251_199);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        assert!(super::Source::at(root, Some("0".repeat(40)), None).is_err());
        assert!(super::Source::at(root, None, Some("1709251199".into())).is_err());
    }
    #[test]
    fn epoch_and_utc_zip_dates_reject_defaults_and_preserve_leap_boundaries() {
        for value in [
            "",
            "-1",
            "0",
            "315532799",
            "4354819200",
            "1.5",
            "+315532800",
        ] {
            assert!(super::epoch(value).is_err());
        }
        let time = super::zip_time(1_709_251_199).unwrap(); // 2024-02-29 23:59:59 UTC
        assert_eq!(
            (
                time.year(),
                time.month(),
                time.day(),
                time.hour(),
                time.minute(),
                time.second()
            ),
            (2024, 2, 29, 23, 59, 58)
        );
        let time = super::zip_time(315_532_800).unwrap();
        assert_eq!((time.year(), time.month(), time.day()), (1980, 1, 1));
    }
}

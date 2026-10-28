//! Deterministic containers over the same staged bytes, not reproducible compilation.
use crate::{Result, capture, run, source};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Format {
    Tar,
    Zip,
    MacZip,
}
impl Format {
    pub(crate) fn for_product(gui: bool, target: &str) -> Self {
        if gui && target.contains("apple") {
            Self::MacZip
        } else if target.contains("windows") {
            Self::Zip
        } else {
            Self::Tar
        }
    }
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Tar => "tar.gz",
            Self::Zip | Self::MacZip => "zip",
        }
    }
}
fn entries(parent: &Path, at: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(at)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(format!("unsupported archive member: {}", at.display()).into());
    }
    out.push(at.strip_prefix(parent)?.to_owned());
    if metadata.is_dir() {
        for entry in fs::read_dir(at)? {
            entries(parent, &entry?.path(), out)?;
        }
    }
    out.sort();
    Ok(())
}
fn mode(path: &Path) -> Result<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(
            if path.is_dir() || fs::metadata(path)?.permissions().mode() & 0o111 != 0 {
                0o755
            } else {
                0o644
            },
        )
    }
    #[cfg(not(unix))]
    {
        Ok(if path.is_dir() { 0o755 } else { 0o644 })
    }
}
pub(crate) fn write(folder: &Path, out: &Path, format: Format, epoch: u64) -> Result<()> {
    let timestamp = source::zip_time(epoch)?;
    let parent = folder.parent().ok_or("archive parent missing")?;
    let mut members = Vec::new();
    entries(parent, folder, &mut members)?;
    match format {
        Format::Tar => {
            let gzip = flate2::GzBuilder::new()
                .mtime(0)
                .write(fs::File::create(out)?, flate2::Compression::default());
            let mut tar = tar::Builder::new(gzip);
            for member in members {
                let path = parent.join(&member);
                let mut header = tar::Header::new_gnu();
                header.set_uid(0);
                header.set_gid(0);
                header.set_mtime(epoch);
                header.set_mode(mode(&path)?);
                if path.is_dir() {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_cksum();
                    tar.append_data(&mut header, member, std::io::empty())?;
                } else {
                    let mut file = fs::File::open(&path)?;
                    header.set_size(file.metadata()?.len());
                    header.set_cksum();
                    tar.append_data(&mut header, member, &mut file)?;
                }
            }
            tar.into_inner()?.finish()?;
        }
        Format::Zip => {
            let mut zip = zip::ZipWriter::new(fs::File::create(out)?);
            for member in members {
                let path = parent.join(&member);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .last_modified_time(timestamp)
                    .unix_permissions(mode(&path)?);
                let name = member.to_string_lossy().replace('\\', "/");
                if path.is_dir() {
                    zip.add_directory(format!("{name}/"), options)?;
                } else {
                    zip.start_file(name, options)?;
                    std::io::copy(&mut fs::File::open(path)?, &mut zip)?;
                }
            }
            zip.finish()?;
        }
        Format::MacZip => {
            // Only filesystem metadata changes; signed file bytes remain untouched.
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch / 2 * 2);
            for member in members.iter().rev() {
                let path = parent.join(member);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(mode(&path)?))?;
                }
                fs::File::open(path)?
                    .set_times(fs::FileTimes::new().set_accessed(time).set_modified(time))?;
            }
            run(Command::new("/usr/bin/ditto")
                .env("TZ", "UTC")
                .args([
                    "-c",
                    "-k",
                    "--keepParent",
                    "--norsrc",
                    "--noextattr",
                    "--noqtn",
                    "--zlibCompressionLevel",
                    "9",
                ])
                .arg(folder)
                .arg(out))?;
            // ditto walks filesystem order. Preserve its compressed members and
            // attributes, but write the directory in canonical name order.
            let intermediate = out.with_extension("ditto.zip");
            fs::rename(out, &intermediate)?;
            let mut input = zip::ZipArchive::new(fs::File::open(&intermediate)?)?;
            let mut names = input
                .file_names()
                .map(|name| name.map(std::borrow::Cow::into_owned))
                .collect::<zip::result::ZipResult<Vec<_>>>()?;
            names.sort();
            let mut output = zip::ZipWriter::new(fs::File::create(out)?);
            for name in names {
                let file = input.by_name(&name)?;
                let permissions = file.unix_mode();
                // Strip ditto's incidental UID/GID and extended-time fields.
                output.raw_copy_file_touch(file, timestamp, permissions)?;
            }
            output.finish()?;
            drop(input);
            fs::remove_file(intermediate)?;
        }
    }
    Ok(())
}

pub(crate) fn extract(archive: &Path, out: &Path, format: Format) -> Result<()> {
    match format {
        Format::MacZip => run(Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(archive)
            .arg(out)),
        Format::Zip => {
            zip::ZipArchive::new(fs::File::open(archive)?)?.extract(out)?;
            Ok(())
        }
        Format::Tar => {
            tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?))
                .unpack(out)?;
            Ok(())
        }
    }
}

/// Selected Xcode is an explicit prerequisite; a flat fallback is not equivalent.
pub(crate) fn actool() -> Result<PathBuf> {
    let value = capture(Command::new("xcrun").args(["--sdk", "macosx", "--find", "actool"]))
        .map_err(|e| {
            format!(
                "macOS desktop packaging requires selected full Xcode with actool; \
            set DEVELOPER_DIR or xcode-select. {e}"
            )
        })?;
    Ok(PathBuf::from(String::from_utf8(value)?.trim()))
}

pub(crate) fn verify_metadata(path: &Path, format: Format, epoch: u64) -> Result<()> {
    match format {
        Format::Tar => {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(path)?));
            let mut last = PathBuf::new();
            for entry in tar.entries()? {
                let entry = entry?;
                let path = entry.path()?.into_owned();
                if path <= last
                    || entry.header().mtime()? != epoch
                    || entry.header().uid()? != 0
                    || entry.header().gid()? != 0
                {
                    return Err("tar member order/time/owner differs from source policy".into());
                }
                last = path;
            }
        }
        Format::Zip | Format::MacZip => {
            let expected = source::zip_time(epoch)?;
            let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
            let mut last = String::new();
            for i in 0..zip.len() {
                let member = zip.by_index(i)?;
                if member.name()?.as_ref() <= last.as_str() {
                    return Err("ZIP members are not sorted and unique".into());
                }
                last = member.name()?.into_owned();
                if member.last_modified() != Some(expected) {
                    return Err(
                        "ZIP member time differs from source policy (UTC, two-second precision)"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn same_stage_bytes_produce_identical_containers_and_meaningful_times() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("fixture");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("input"), "stable bytes").unwrap();
        fs::write(folder.join("z-last"), "z").unwrap();
        fs::write(folder.join("a-first"), "a").unwrap();
        let mut formats = vec![Format::Tar, Format::Zip];
        if cfg!(target_os = "macos") {
            formats.push(Format::MacZip);
        }
        for format in formats {
            let a = temp.path().join(format!("a.{}", format.extension()));
            let b = temp.path().join(format!("b.{}", format.extension()));
            write(&folder, &a, format, 1_709_251_199).unwrap();
            write(&folder, &b, format, 1_709_251_199).unwrap();
            verify_metadata(&a, format, 1_709_251_199).unwrap();
            assert!(verify_metadata(&a, format, 1_709_251_201).is_err());
            assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap(), "{format:?}");
            if matches!(format, Format::Tar) {
                let mut tar =
                    tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(a).unwrap()));
                for member in tar.entries().unwrap() {
                    assert_eq!(member.unwrap().header().mtime().unwrap(), 1_709_251_199);
                }
            } else {
                let mut zip = zip::ZipArchive::new(fs::File::open(a).unwrap()).unwrap();
                let member = zip.by_name("fixture/input").unwrap();
                assert_eq!(
                    member.last_modified(),
                    Some(source::zip_time(1_709_251_199).unwrap())
                );
            }
        }
        assert!(write(&folder, &temp.path().join("invalid.zip"), Format::Zip, 0).is_err());
    }
}

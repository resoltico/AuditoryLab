//! Fonts compiled into a built executable, found by their sfnt structure.
//!
//! Dependencies can embed fonts (Wayland window decorations draw their title with
//! one). Notices are derived from each font's own name table, so a font that a
//! dependency starts embedding is credited, or refused, without a hand-kept list.
use crate::Result;
use std::fmt::Write as _;

/// Copyright and license statements a font declares about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Font {
    pub(crate) name: String,
    pub(crate) copyright: String,
    pub(crate) license: String,
}

const OFL: &str = "SIL Open Font License, Version 1.1";

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn usize_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32_at(bytes, at)?).ok()
}

/// The table directory of an sfnt starting at `start`, when it is a complete font
/// with naming, header and character-map tables inside `bytes`.
fn tables(bytes: &[u8], start: usize) -> Option<Vec<([u8; 4], usize, usize)>> {
    let count = usize::from(u16_at(bytes, start + 4)?);
    if !(3..=64).contains(&count) {
        return None;
    }
    let mut tables = Vec::with_capacity(count);
    for index in 0..count {
        let record = start + 12 + index * 16;
        let tag: [u8; 4] = bytes.get(record..record + 4)?.try_into().ok()?;
        let offset = start.checked_add(usize_at(bytes, record + 8)?)?;
        let length = usize_at(bytes, record + 12)?;
        // Tags are printable ASCII, padded with trailing spaces such as `cvt `.
        let printable = tag.iter().all(|byte| (0x20..=0x7e).contains(byte)) && tag[0] != b' ';
        if !printable || offset.checked_add(length)? > bytes.len() {
            return None;
        }
        tables.push((tag, offset, length));
    }
    [b"name", b"head", b"cmap"]
        .iter()
        .all(|wanted| tables.iter().any(|(tag, ..)| tag == *wanted))
        .then_some(tables)
}

/// One name-table string, preferring Windows Unicode English over Macintosh Roman.
fn name(bytes: &[u8], table: usize, id: u16) -> Option<String> {
    let count = usize::from(u16_at(bytes, table + 2)?);
    let strings = table + usize::from(u16_at(bytes, table + 4)?);
    let mut fallback = None;
    for index in 0..count {
        let record = table + 6 + index * 12;
        if u16_at(bytes, record + 6)? != id {
            continue;
        }
        let (platform, encoding, language) = (
            u16_at(bytes, record)?,
            u16_at(bytes, record + 2)?,
            u16_at(bytes, record + 4)?,
        );
        let length = usize::from(u16_at(bytes, record + 8)?);
        let at = strings + usize::from(u16_at(bytes, record + 10)?);
        let raw = bytes.get(at..at.checked_add(length)?)?;
        if platform == 3 && encoding == 1 && language == 0x409 {
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            return String::from_utf16(&units).ok();
        }
        if platform == 1 && raw.is_ascii() {
            fallback = Some(String::from_utf8_lossy(raw).into_owned());
        }
    }
    fallback
}

/// Every complete font embedded in an executable, in file order.
pub(crate) fn embedded(bytes: &[u8]) -> Vec<Font> {
    let mut fonts = Vec::new();
    let mut at = 0;
    while at + 12 <= bytes.len() {
        let signature = &bytes[at..at + 4];
        let sfnt = signature == [0, 1, 0, 0] || signature == b"OTTO" || signature == b"true";
        if sfnt && let Some(tables) = tables(bytes, at) {
            let (_, table, _) = tables
                .iter()
                .find(|(tag, ..)| tag == b"name")
                .copied()
                .unwrap_or_default();
            let field = |id| name(bytes, table, id).unwrap_or_default();
            fonts.push(Font {
                name: field(4),
                copyright: field(0),
                license: field(13),
            });
            let end = tables
                .iter()
                .map(|(_, offset, length)| offset + length)
                .max();
            at = end.unwrap_or(at + 1).max(at + 1);
            continue;
        }
        at += 1;
    }
    fonts
}

/// Notice text crediting each embedded font under its declared license.
///
/// # Errors
/// A font without a copyright statement, or under a license other than the
/// SIL Open Font License 1.1, is refused rather than shipped uncredited.
pub(crate) fn notices(fonts: &[Font]) -> Result<String> {
    let mut text = String::new();
    for font in fonts {
        if font.copyright.trim().is_empty() || !font.license.contains(OFL) {
            return Err(format!(
                "embedded font {:?} needs a copyright and a supported license; it declares \
                {:?} under {:?}",
                font.name, font.copyright, font.license
            )
            .into());
        }
        let _ = write!(
            text,
            "SIL Open Font License 1.1\nUsed by: {} font, embedded in this executable\n\n\
            {}\n\n{}\n\n",
            font.name.trim(),
            font.copyright.trim(),
            include_str!("../templates/ofl-1.1.txt").trim_end()
        );
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal font with name, head and cmap tables, padded inside other bytes.
    pub(crate) fn font(copyright: &str, license: &str) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut records = Vec::new();
        for (id, value) in [(0_u16, copyright), (4, "Example Regular"), (13, license)] {
            let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_be_bytes).collect();
            records.push((id, u16::try_from(strings.len()).unwrap(), encoded.len()));
            strings.extend(encoded);
        }
        let mut name = Vec::new();
        name.extend(0_u16.to_be_bytes());
        name.extend(u16::try_from(records.len()).unwrap().to_be_bytes());
        name.extend(u16::try_from(6 + records.len() * 12).unwrap().to_be_bytes());
        for (id, offset, length) in records {
            for value in [3_u16, 1, 0x409, id, u16::try_from(length).unwrap(), offset] {
                name.extend(value.to_be_bytes());
            }
        }
        name.extend(strings);
        let directory = 12 + 4 * 16;
        let mut sfnt = vec![0, 1, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0];
        let tables: [(&[u8; 4], Vec<u8>); 4] = [
            (b"cmap", vec![0; 4]),
            (b"cvt ", vec![0; 4]),
            (b"head", vec![0; 54]),
            (b"name", name),
        ];
        let mut body: Vec<u8> = Vec::new();
        for (tag, data) in &tables {
            sfnt.extend(*tag);
            sfnt.extend([0; 4]);
            sfnt.extend(u32::try_from(directory + body.len()).unwrap().to_be_bytes());
            sfnt.extend(u32::try_from(data.len()).unwrap().to_be_bytes());
            body.extend(data);
        }
        sfnt.extend(body);
        sfnt
    }

    #[test]
    fn fonts_inside_executable_bytes_are_found_with_their_declarations() {
        let ofl = format!("This Font Software is licensed under the {OFL}.");
        let mut executable = b"\0\x01\0\0 code that only resembles a header".to_vec();
        executable.extend(font("Copyright (c) 2010, Example Foundry", &ofl));
        executable.extend(b"more code");
        let fonts = embedded(&executable);
        assert_eq!(
            fonts,
            [Font {
                name: "Example Regular".into(),
                copyright: "Copyright (c) 2010, Example Foundry".into(),
                license: ofl,
            }]
        );
        let text = notices(&fonts).unwrap();
        assert!(text.contains("Used by: Example Regular font"));
        assert!(text.contains("Copyright (c) 2010, Example Foundry"));
        assert!(text.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert_eq!(embedded(b"no fonts \0\x01\0\0\0\x03 here"), []);
        let truncated = font("C", "L");
        assert_eq!(embedded(&truncated[..truncated.len() - 1]), []);
    }

    #[test]
    fn fonts_without_a_copyright_or_a_known_license_are_refused() {
        for (copyright, license) in [
            ("", "SIL Open Font License, Version 1.1"),
            ("Copyright (c) Foundry", "All rights reserved"),
        ] {
            let fonts = embedded(&font(copyright, license));
            assert_eq!(fonts.len(), 1);
            assert!(notices(&fonts).is_err(), "{copyright:?} {license:?}");
        }
    }
}

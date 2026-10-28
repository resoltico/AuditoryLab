//! Inspect actual PE icon resources against the supplied multi-image ICO.
use crate::Result;
fn word(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or("truncated PE/ICO word")?
            .try_into()?,
    ))
}
fn dword(bytes: &[u8], offset: usize) -> Result<usize> {
    Ok(usize::try_from(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("truncated PE/ICO offset")?
            .try_into()?,
    ))?)
}
fn child(bytes: &[u8], directory: usize, id: usize) -> Result<usize> {
    let count =
        usize::from(word(bytes, directory + 12)?) + usize::from(word(bytes, directory + 14)?);
    for index in 0..count {
        let at = directory + 16 + index * 8;
        if dword(bytes, at)? == id {
            return dword(bytes, at + 4);
        }
    }
    Err(format!("PE icon resource ID {id} missing").into())
}
fn resource(bytes: &[u8], kind: usize, id: usize) -> Result<&[u8]> {
    let pe = dword(bytes, 0x3c)?;
    let sections = usize::from(word(bytes, pe + 6)?);
    let optional = pe + 24;
    let section_table = optional + usize::from(word(bytes, pe + 20)?);
    let rva = dword(bytes, optional + 112 + 16)?;
    let locate = |rva: usize| -> Result<usize> {
        for i in 0..sections {
            let at = section_table + i * 40;
            let start = dword(bytes, at + 12)?;
            let length = dword(bytes, at + 16)?;
            if let Some(relative) = rva.checked_sub(start).filter(|relative| *relative < length) {
                return Ok(dword(bytes, at + 20)? + relative);
            }
        }
        Err("PE icon RVA has no file section".into())
    };
    let base = locate(rva)?;
    let directory = |offset: usize| -> Result<usize> {
        if offset & 0x8000_0000 == 0 {
            return Err("PE icon resource directory missing".into());
        }
        Ok(base + (offset & 0x7fff_ffff))
    };
    let types = directory(child(bytes, base, kind)?)?;
    let languages = directory(child(bytes, types, id)?)?;
    let count =
        usize::from(word(bytes, languages + 12)?) + usize::from(word(bytes, languages + 14)?);
    if count != 1 {
        return Err("expected one icon resource language".into());
    }
    let data = child(bytes, languages, dword(bytes, languages + 16)?)?;
    if data & 0x8000_0000 != 0 {
        return Err("PE icon payload is a directory".into());
    }
    let offset = locate(dword(bytes, base + data)?)?;
    let size = dword(bytes, base + data + 4)?;
    bytes
        .get(offset..offset.checked_add(size).ok_or("icon size overflow")?)
        .ok_or_else(|| "truncated PE icon payload".into())
}
pub(crate) fn verify(bytes: &[u8], ico: &[u8]) -> Result<()> {
    if word(ico, 0)? != 0 || word(ico, 2)? != 1 {
        return Err("invalid supplied ICO".into());
    }
    let count = usize::from(word(ico, 4)?);
    if count == 0 {
        return Err("empty supplied ICO".into());
    }
    let group = resource(bytes, 14, 1)?;
    if group.get(..6) != ico.get(..6) {
        return Err("PE group icon differs from supplied ICO".into());
    }
    for i in 0..count {
        let at = 6 + i * 16;
        let g = 6 + i * 14;
        if ico.get(at..at + 4) != group.get(g..g + 4)
            || ico.get(at + 6..at + 12) != group.get(g + 6..g + 12)
        {
            return Err("PE icon dimensions differ from supplied ICO".into());
        }
        // RC fills an unspecified ICO plane count with the single image plane.
        // Descriptor dimensions, bit depth, size and actual image bytes remain exact.
        let planes = word(ico, at + 4)?;
        let resource_planes = word(group, g + 4)?;
        if resource_planes != planes && !(planes == 0 && resource_planes == 1) {
            return Err("PE icon plane count differs from supplied ICO".into());
        }
        let offset = dword(ico, at + 12)?;
        let size = dword(ico, at + 8)?;
        let expected = ico
            .get(offset..offset.checked_add(size).ok_or("ICO size overflow")?)
            .ok_or("truncated supplied ICO image")?;
        if resource(bytes, 3, usize::from(word(group, g + 12)?))? != expected {
            return Err("PE icon image differs from owner artwork".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn actual_resource_tree_and_image_bytes_are_required() {
        let mut pe = vec![0_u8; 1024];
        let put16 = |bytes: &mut [u8], at, value: u16| {
            bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        let put32 = |bytes: &mut [u8], at, value: u32| {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put32(&mut pe, 0x3c, 0x80);
        put16(&mut pe, 0x86, 1);
        put16(&mut pe, 0x94, 240);
        put32(&mut pe, 0x98 + 128, 0x1000); // resource RVA in PE32+ optional header
        let section = 0x98 + 240;
        put32(&mut pe, section + 12, 0x1000);
        put32(&mut pe, section + 16, 512);
        put32(&mut pe, section + 20, 512);
        // resource root: RT_ICON=3 and RT_GROUP_ICON=14
        put16(&mut pe, 512 + 14, 2);
        put32(&mut pe, 528, 3);
        put32(&mut pe, 532, 0x8000_0030);
        put32(&mut pe, 536, 14);
        put32(&mut pe, 540, 0x8000_0050);
        // one image ID 7; group ID 1
        for (directory, id, next) in [(48, 7, 80_u32 + 32), (80, 1, 144)] {
            put16(&mut pe, 512 + directory + 14, 1);
            put32(&mut pe, 512 + directory + 16, id);
            put32(&mut pe, 512 + directory + 20, 0x8000_0000 + next);
        }
        for (directory, data) in [(112, 176), (144, 192)] {
            put16(&mut pe, 512 + directory + 14, 1);
            put32(&mut pe, 512 + directory + 16, 1033);
            put32(&mut pe, 512 + directory + 20, data);
        }
        put32(&mut pe, 512 + 176, 0x1100);
        put32(&mut pe, 512 + 180, 4);
        put32(&mut pe, 512 + 192, 0x1120);
        put32(&mut pe, 512 + 196, 20);
        pe[768..772].copy_from_slice(b"icon");
        let mut ico = vec![0_u8; 26];
        put16(&mut ico, 2, 1);
        put16(&mut ico, 4, 1);
        ico[6] = 32;
        ico[7] = 32;
        put16(&mut ico, 10, 1);
        put16(&mut ico, 12, 32);
        put32(&mut ico, 14, 4);
        put32(&mut ico, 18, 22);
        ico[22..].copy_from_slice(b"icon");
        pe[800..806].copy_from_slice(&ico[..6]);
        pe[806..818].copy_from_slice(&ico[6..18]);
        put16(&mut pe, 818, 7);
        super::verify(&pe, &ico).unwrap();
        // The supplied PNG ICO uses zero planes; native RC writes one in the group.
        put16(&mut ico, 10, 0);
        super::verify(&pe, &ico).unwrap();
        put16(&mut pe, 810, 2);
        assert!(super::verify(&pe, &ico).is_err());
        put16(&mut pe, 810, 1);
        pe[806] = 48;
        assert!(super::verify(&pe, &ico).is_err());
        pe[806] = 32;
        pe[812] = 24;
        assert!(super::verify(&pe, &ico).is_err());
        pe[812] = 32;
        pe[768] = b'x';
        assert!(super::verify(&pe, &ico).is_err());
        pe[768] = b'i';
        pe[536] = 13;
        assert!(super::verify(&pe, &ico).is_err());
        for size in [0, 63, 128, 512, 800] {
            assert!(super::verify(&pe[..size], &ico).is_err());
        }
        assert!(super::verify(&pe, b"").is_err());
    }
}

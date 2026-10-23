//! Deterministic, uncompressed ZIP source archive (Python's standard zipapp format).
//! The sources remain individually editable after extraction. No timestamps, host
//! paths, compression libraries, or Python executable enter the emitted bytes.
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) fn encode(files: &BTreeMap<PathBuf, String>, limit: usize) -> Result<Vec<u8>, String> {
    if files.len() > u16::MAX as usize || !files.contains_key(&PathBuf::from("__main__.py")) {
        return Err("Python package needs a bounded file set and __main__.py".into());
    }
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (path, source) in files {
        let name = path.to_str().ok_or("non-UTF-8 Python archive path")?;
        if name.is_empty()
            || name.contains(['/', '\\', '\0'])
            || name == "."
            || name == ".."
            || name.len() > u16::MAX as usize
        {
            return Err("Python archive entries must be flat file names".into());
        }
        let next = out
            .len()
            .checked_add(directory.len())
            .and_then(|n| n.checked_add(source.len()))
            .and_then(|n| n.checked_add(76 + name.len() * 2 + 22))
            .ok_or("Python archive size overflow")?;
        if next > limit || next > u32::MAX as usize {
            return Err("generated Python archive exceeds size limit".into());
        }
        let offset = out.len() as u32;
        let size = source.len() as u32;
        let crc = crc32(source.as_bytes());
        // Stored member, UTF-8 name flag, fixed DOS date 1980-01-01.
        u32le(&mut out, 0x04034b50);
        for n in [20, 0x0800, 0, 0, 33] {
            u16le(&mut out, n);
        }
        for n in [crc, size, size] {
            u32le(&mut out, n);
        }
        u16le(&mut out, name.len() as u16);
        u16le(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(source.as_bytes());

        u32le(&mut directory, 0x02014b50);
        for n in [20, 20, 0x0800, 0, 0, 33] {
            u16le(&mut directory, n);
        }
        for n in [crc, size, size] {
            u32le(&mut directory, n);
        }
        for n in [name.len() as u16, 0, 0, 0, 0] {
            u16le(&mut directory, n);
        }
        u32le(&mut directory, 0);
        u32le(&mut directory, offset);
        directory.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    let size = directory.len() as u32;
    out.extend_from_slice(&directory);
    u32le(&mut out, 0x06054b50);
    for n in [0, 0, files.len() as u16, files.len() as u16] {
        u16le(&mut out, n);
    }
    u32le(&mut out, size);
    u32le(&mut out, start);
    u16le(&mut out, 0);
    Ok(out)
}

fn u16le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_archive_limits() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        let mut files = BTreeMap::from([(PathBuf::from("__main__.py"), "print(7)\n".into())]);
        let bytes = encode(&files, 1024).unwrap();
        assert_eq!(bytes, encode(&files, bytes.len()).unwrap());
        assert!(encode(&files, bytes.len() - 1).is_err());
        files.insert(PathBuf::from("../escape"), "".into());
        assert!(encode(&files, 1024).is_err());
    }
}

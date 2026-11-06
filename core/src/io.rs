//! Reading and writing romfs files: zstd for `.zs`, and writes that never leave half a file.

use crate::error::{Context, Result};
use std::io::Read;
use std::path::Path;

/// zstd level for written files. The game reads any level; 16 is close to the shipped sizes.
const ZSTD_LEVEL: i32 = 16;

/// A file's contents, decompressed when the name ends in `.zs`.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let raw = std::fs::read(path).path_context(path)?;
    if path.extension().is_some_and(|e| e == "zs") {
        let mut out = Vec::new();
        zstd::stream::read::Decoder::new(&raw[..]).and_then(|mut d| d.read_to_end(&mut out)).path_context(path)?;
        Ok(out)
    } else {
        Ok(raw)
    }
}

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Ok(zstd::bulk::compress(data, ZSTD_LEVEL)?)
}

/// Write through a temporary file and rename it into place, creating parent folders.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).path_context(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, data).path_context(&tmp)?;
    std::fs::rename(&tmp, path).path_context(path)
}

/// Files below `root`, as sorted '/'-separated relative paths.
pub fn files_under(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_round_trip_and_atomic_write() {
        let dir = std::env::temp_dir().join(format!("mkwtrack-io-{}", std::process::id()));
        let f = dir.join("a/b/c.bin.zs");
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        write_atomic(&f, &compress(&data).unwrap()).unwrap();
        assert_eq!(read_file(&f).unwrap(), data);
        assert_eq!(files_under(&dir), ["a/b/c.bin.zs"]);
        assert!(read_file(&dir.join("missing")).unwrap_err().to_string().contains("missing"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

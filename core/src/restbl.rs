//! The resource size table (`System/Resource/ResourceSizeTable.Product.*.rsizetable.zs`).
//!
//! The game reserves memory for a resource from this table before loading it, so a file that
//! grows past its entry needs a bigger entry. Format (RESTBL v1): a CRC32 of the path for most
//! files, plus a table of full paths for names whose CRC32 collides.

use crate::error::{Error, Result};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSizeTable {
    pub version: u32,
    pub name_len: usize,
    pub crc: BTreeMap<u32, u32>,
    pub names: BTreeMap<String, u32>,
}

impl ResourceSizeTable {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 22 || &d[..6] != b"RESTBL" {
            return Err("not a RESTBL file".into());
        }
        let u32le = |o: usize| -> Result<u32> {
            d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| Error::new("RESTBL: truncated"))
        };
        let version = u32le(6)?;
        let name_len = u32le(10)? as usize;
        let (n_crc, n_name) = (u32le(14)? as usize, u32le(18)? as usize);
        let need = n_crc
            .checked_mul(8)
            .and_then(|c| n_name.checked_mul(name_len + 4).and_then(|n| c.checked_add(n)))
            .and_then(|x| x.checked_add(22));
        if need.is_none_or(|n| n > d.len()) {
            return Err("RESTBL: truncated".into());
        }
        let mut off = 22;
        let mut crc = BTreeMap::new();
        for _ in 0..n_crc {
            crc.insert(u32le(off)?, u32le(off + 4)?);
            off += 8;
        }
        let mut names = BTreeMap::new();
        for _ in 0..n_name {
            let raw = &d[off..off + name_len];
            let z = raw.iter().position(|&c| c == 0).unwrap_or(name_len);
            names.insert(String::from_utf8_lossy(&raw[..z]).into_owned(), u32le(off + name_len)?);
            off += name_len + 4;
        }
        Ok(Self { version, name_len, crc, names })
    }

    /// Table key for a romfs path: no leading slash, no `.zs`.
    pub fn key(path: &str) -> &str {
        let p = path.trim_start_matches('/');
        p.strip_suffix(".zs").unwrap_or(p)
    }

    pub fn get(&self, path: &str) -> Option<u32> {
        let k = Self::key(path);
        self.names.get(k).copied().or_else(|| self.crc.get(&crc32fast::hash(k.as_bytes())).copied())
    }

    pub fn set(&mut self, path: &str, size: u32) {
        let k = Self::key(path);
        match self.names.get_mut(k) {
            Some(v) => *v = size,
            None => {
                self.crc.insert(crc32fast::hash(k.as_bytes()), size);
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = b"RESTBL".to_vec();
        for v in [self.version, self.name_len as u32, self.crc.len() as u32, self.names.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (h, s) in &self.crc {
            out.extend_from_slice(&h.to_le_bytes());
            out.extend_from_slice(&s.to_le_bytes());
        }
        for (n, s) in &self.names {
            let mut raw = n.as_bytes().to_vec();
            raw.resize(self.name_len, 0);
            out.extend_from_slice(&raw);
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

/// The entry Nintendo's tools give a baked collision file (`.bphsc`): its decompressed size
/// rounded up to 32, plus 2720, plus 320 for each body. Matches every shipped entry.
pub fn required_collision_size(decompressed: usize, bodies: usize) -> u32 {
    (((decompressed + 31) & !31) + 2720 + 320 * bodies) as u32
}

/// The size entry Nintendo's tools write for a Banc resource, from the pattern in the shipped
/// table (it matches all 1,311 Banc entries): `.bcett.byml` = decompressed size + 1024; plain
/// `.byml` (PathAdditionalData) = size rounded up to 32, + 288.
pub fn required_size(path: &str, decompressed: usize) -> u32 {
    if ResourceSizeTable::key(path).ends_with(".bcett.byml") {
        (decompressed + 1024) as u32
    } else {
        (((decompressed + 31) & !31) + 288) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_get_set() {
        let mut t = ResourceSizeTable { version: 1, name_len: 160, crc: BTreeMap::new(), names: BTreeMap::new() };
        t.set("Banc/A.bcett.byml.zs", 10);
        t.names.insert("Banc/Collide.byml".into(), 5);
        t.set("/Banc/Collide.byml", 6);
        let back = ResourceSizeTable::parse(&t.to_bytes()).unwrap();
        assert_eq!(back.get("Banc/A.bcett.byml"), Some(10));
        assert_eq!(back.get("Banc/Collide.byml.zs"), Some(6));
        assert_eq!(back.to_bytes(), t.to_bytes());
        assert_eq!(required_size("Banc/A.bcett.byml.zs", 4096), 5120);
        assert_eq!(required_size("Banc/PathAdditionalData/A.byml", 33), 64 + 288);
    }

    #[test]
    fn truncated_tables_are_errors() {
        let mut t = ResourceSizeTable { version: 1, name_len: 8, crc: BTreeMap::new(), names: BTreeMap::new() };
        t.set("x", 1);
        t.names.insert("y".into(), 2);
        let b = t.to_bytes();
        for n in 0..b.len() {
            assert!(ResourceSizeTable::parse(&b[..n]).is_err());
        }
        let mut huge = b.clone();
        huge[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ResourceSizeTable::parse(&huge).is_err());
    }
}

//! BYML version 7 (little endian), as used by Mario Kart World and other module-system games.
//!
//! The writer reproduces the layout of Nintendo's own writer byte for byte: header, sorted key
//! table, sorted string table, the 8-byte values (u64, s64, f64) and binary blobs in depth-first
//! order, then every container in depth-first pre-order. Course files (`.bcett.byml`) are also
//! zero-padded to the next power of two.

use crate::error::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Byml {
    Null,
    Bool(bool),
    I32(i32),
    F32(f32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Bin(Vec<u8>),
    Array(Vec<Byml>),
    Map(BTreeMap<String, Byml>),
    Hash32(BTreeMap<u32, Byml>),
    Hash64(BTreeMap<u64, Byml>),
}

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::new(format!("BYML: {}", m.into())))
}

const T_HASH32: u8 = 0x20;
const T_HASH64: u8 = 0x21;
const T_STR: u8 = 0xA0;
const T_BIN: u8 = 0xA1;
const T_ARRAY: u8 = 0xC0;
const T_DICT: u8 = 0xC1;
const T_STRTAB: u8 = 0xC2;
const T_BOOL: u8 = 0xD0;
const T_I32: u8 = 0xD1;
const T_F32: u8 = 0xD2;
const T_U32: u8 = 0xD3;
const T_I64: u8 = 0xD4;
const T_U64: u8 = 0xD5;
const T_F64: u8 = 0xD6;
const T_NULL: u8 = 0xFF;

/// Deepest nesting the reader accepts. Shipped files nest about 8 deep; the limit stops a
/// malformed file whose offsets loop from recursing forever.
const MAX_DEPTH: usize = 128;

impl Byml {
    pub fn type_code(&self) -> u8 {
        match self {
            Byml::Null => T_NULL,
            Byml::Bool(_) => T_BOOL,
            Byml::I32(_) => T_I32,
            Byml::F32(_) => T_F32,
            Byml::U32(_) => T_U32,
            Byml::I64(_) => T_I64,
            Byml::U64(_) => T_U64,
            Byml::F64(_) => T_F64,
            Byml::Str(_) => T_STR,
            Byml::Bin(_) => T_BIN,
            Byml::Array(_) => T_ARRAY,
            Byml::Map(_) => T_DICT,
            Byml::Hash32(_) => T_HASH32,
            Byml::Hash64(_) => T_HASH64,
        }
    }

    fn is_container(&self) -> bool {
        matches!(self, Byml::Array(_) | Byml::Map(_) | Byml::Hash32(_) | Byml::Hash64(_))
    }

    /// Values stored out of line (8-byte numbers and binary blobs).
    fn is_long(&self) -> bool {
        matches!(self, Byml::I64(_) | Byml::U64(_) | Byml::F64(_) | Byml::Bin(_))
    }

    // ------------------------------------------------------------ constructors

    pub fn map() -> Byml {
        Byml::Map(BTreeMap::new())
    }

    /// An array of f32 values, the form of every vector in course files.
    pub fn floats(v: &[f32]) -> Byml {
        Byml::Array(v.iter().map(|&x| Byml::F32(x)).collect())
    }

    // ------------------------------------------------------------ accessors

    pub fn get(&self, key: &str) -> Option<&Byml> {
        self.as_map()?.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Byml> {
        self.as_map_mut()?.get_mut(key)
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Byml>> {
        match self {
            Byml::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_map_mut(&mut self) -> Option<&mut BTreeMap<String, Byml>> {
        match self {
            Byml::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Byml>> {
        match self {
            Byml::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Byml>> {
        match self {
            Byml::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Byml::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Byml::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Any numeric value (or a bool) as f64.
    pub fn as_f64(&self) -> Option<f64> {
        Some(match self {
            Byml::I32(v) => *v as f64,
            Byml::F32(v) => *v as f64,
            Byml::U32(v) => *v as f64,
            Byml::I64(v) => *v as f64,
            Byml::U64(v) => *v as f64,
            Byml::F64(v) => *v,
            Byml::Bool(b) => *b as u8 as f64,
            _ => return None,
        })
    }

    /// Integer values (hashes) as u64.
    pub fn as_u64(&self) -> Option<u64> {
        Some(match self {
            Byml::U64(v) => *v,
            Byml::I64(v) => *v as u64,
            Byml::U32(v) => *v as u64,
            Byml::I32(v) => *v as u64,
            _ => return None,
        })
    }

    /// The string under `key`, or `default`.
    pub fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).and_then(|v| v.as_str()).unwrap_or(default)
    }

    /// The array under `key`, or an empty slice.
    pub fn array(&self, key: &str) -> &[Byml] {
        self.get(key).and_then(|a| a.as_array()).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The array under `key`, created (or replaced, if `key` holds something else) when needed.
    pub fn array_mut(&mut self, key: &str) -> &mut Vec<Byml> {
        if self.get(key).and_then(|a| a.as_array()).is_none() {
            self.insert(key, Byml::Array(vec![]));
        }
        self.get_mut(key).and_then(|a| a.as_array_mut()).expect("just inserted")
    }

    /// The number under `key` as u64 (hashes, link targets).
    pub fn u64_of(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_u64())
    }

    /// The 3-vector under `key` (see [`vec3`]).
    pub fn vec3(&self, key: &str, default: [f32; 3]) -> [f32; 3] {
        vec3(self.get(key), default)
    }

    pub fn insert(&mut self, key: &str, v: Byml) {
        if let Byml::Map(m) = self {
            m.insert(key.to_string(), v);
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Byml> {
        self.as_map_mut()?.remove(key)
    }
}

/// A 3-vector: a three-element array, or a single number used for all three (scales are
/// sometimes stored that way). Anything else gives `default`.
pub fn vec3(v: Option<&Byml>, default: [f32; 3]) -> [f32; 3] {
    match v {
        Some(Byml::Array(a)) if a.len() == 3 => [0, 1, 2].map(|i| a[i].as_f64().map_or(default[i], |x| x as f32)),
        Some(x) => x.as_f64().map_or(default, |s| [s as f32; 3]),
        None => default,
    }
}

// ---------------------------------------------------------------- reading

struct Reader<'a> {
    d: &'a [u8],
    keys: Vec<String>,
    strings: Vec<String>,
}

impl Reader<'_> {
    fn bytes(&self, o: usize, n: usize) -> Result<&[u8]> {
        o.checked_add(n).and_then(|end| self.d.get(o..end)).map_or_else(|| err(format!("read past the end at {o:#x}")), Ok)
    }
    fn u8(&self, o: usize) -> Result<u8> {
        Ok(self.bytes(o, 1)?[0])
    }
    fn u16(&self, o: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(o, 2)?.try_into().unwrap()))
    }
    fn u24(&self, o: usize) -> Result<u32> {
        let b = self.bytes(o, 3)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], 0]))
    }
    fn u32(&self, o: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(o, 4)?.try_into().unwrap()))
    }
    fn u64(&self, o: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(o, 8)?.try_into().unwrap()))
    }

    fn table(&self, off: usize) -> Result<Vec<String>> {
        if off == 0 {
            return Ok(vec![]);
        }
        if self.u8(off)? != T_STRTAB {
            return err(format!("expected a string table at {off:#x}"));
        }
        let n = self.u24(off + 1)? as usize;
        let offs: Vec<usize> = (0..=n).map(|i| self.u32(off + 4 + 4 * i).map(|o| off + o as usize)).collect::<Result<_>>()?;
        offs.windows(2)
            .map(|w| {
                let s = self.d.get(w[0]..w[1]).ok_or_else(|| Error::new("BYML: bad string table"))?;
                let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                Ok(String::from_utf8_lossy(&s[..end]).into_owned())
            })
            .collect()
    }

    fn value(&self, t: u8, slot: usize, depth: usize) -> Result<Byml> {
        let v = self.u32(slot)?;
        Ok(match t {
            T_STR => Byml::Str(self.strings.get(v as usize).cloned().ok_or_else(|| Error::new("BYML: bad string index"))?),
            T_ARRAY | T_DICT | T_HASH32 | T_HASH64 => self.node(t, v as usize, depth + 1)?,
            T_BOOL => Byml::Bool(v != 0),
            T_I32 => Byml::I32(v as i32),
            T_F32 => Byml::F32(f32::from_bits(v)),
            T_U32 => Byml::U32(v),
            T_I64 => Byml::I64(self.u64(v as usize)? as i64),
            T_U64 => Byml::U64(self.u64(v as usize)?),
            T_F64 => Byml::F64(f64::from_bits(self.u64(v as usize)?)),
            T_BIN => {
                let n = self.u32(v as usize)? as usize;
                Byml::Bin(self.bytes(v as usize + 4, n)?.to_vec())
            }
            T_NULL => Byml::Null,
            _ => return err(format!("unsupported value type {t:#x} at {slot:#x}")),
        })
    }

    fn node(&self, t: u8, off: usize, depth: usize) -> Result<Byml> {
        if depth > MAX_DEPTH {
            return err("nested too deeply");
        }
        let actual = self.u8(off)?;
        if actual != t {
            return err(format!("type mismatch {t:#x} vs {actual:#x} at {off:#x}"));
        }
        let n = self.u24(off + 1)? as usize;
        Ok(match t {
            T_ARRAY => {
                let vals = off + 4 + ((n + 3) & !3);
                let a = (0..n).map(|i| self.value(self.u8(off + 4 + i)?, vals + 4 * i, depth)).collect::<Result<_>>()?;
                Byml::Array(a)
            }
            T_DICT => {
                let mut m = BTreeMap::new();
                for i in 0..n {
                    let e = off + 4 + 8 * i;
                    let key = self.keys.get(self.u24(e)? as usize).cloned().ok_or_else(|| Error::new("BYML: bad key index"))?;
                    m.insert(key, self.value(self.u8(e + 3)?, e + 4, depth)?);
                }
                Byml::Map(m)
            }
            T_HASH32 => {
                let types = off + 4 + 8 * n;
                let mut m = BTreeMap::new();
                for i in 0..n {
                    let e = off + 4 + 8 * i;
                    m.insert(self.u32(e)?, self.value(self.u8(types + i)?, e + 4, depth)?);
                }
                Byml::Hash32(m)
            }
            T_HASH64 => {
                let types = off + 4 + 12 * n;
                let mut m = BTreeMap::new();
                for i in 0..n {
                    let e = off + 4 + 12 * i;
                    m.insert(self.u64(e)?, self.value(self.u8(types + i)?, e + 8, depth)?);
                }
                Byml::Hash64(m)
            }
            _ => return err(format!("unsupported container type {t:#x}")),
        })
    }
}

/// Parse BYML bytes. Returns the root and the format version.
pub fn from_bytes(d: &[u8]) -> Result<(Byml, u16)> {
    if d.len() < 16 {
        return err("file too small");
    }
    match &d[..2] {
        b"YB" => {}
        b"BY" => return err("big-endian files are not supported"),
        _ => return err("not a BYML file"),
    }
    let mut r = Reader { d, keys: vec![], strings: vec![] };
    let version = r.u16(2)?;
    r.keys = r.table(r.u32(4)? as usize)?;
    r.strings = r.table(r.u32(8)? as usize)?;
    let root = r.u32(12)? as usize;
    let node = if root == 0 { Byml::Null } else { r.node(r.u8(root)?, root, 0)? };
    Ok((node, version))
}

// ---------------------------------------------------------------- writing

fn collect(n: &Byml, keys: &mut BTreeSet<String>, strings: &mut BTreeSet<String>) {
    match n {
        Byml::Map(m) => {
            for (k, v) in m {
                keys.insert(k.clone());
                collect(v, keys, strings);
            }
        }
        Byml::Array(a) => a.iter().for_each(|v| collect(v, keys, strings)),
        Byml::Hash32(m) => m.values().for_each(|v| collect(v, keys, strings)),
        Byml::Hash64(m) => m.values().for_each(|v| collect(v, keys, strings)),
        Byml::Str(s) => {
            strings.insert(s.clone());
        }
        _ => {}
    }
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

fn string_table(strings: &[String]) -> Vec<u8> {
    let mut blob = Vec::new();
    let mut offs = Vec::with_capacity(strings.len() + 1);
    let base = 4 + 4 * (strings.len() + 1);
    for s in strings {
        offs.push((base + blob.len()) as u32);
        blob.extend_from_slice(s.as_bytes());
        blob.push(0);
    }
    offs.push((base + blob.len()) as u32);
    let mut out = (T_STRTAB as u32 | ((strings.len() as u32) << 8)).to_le_bytes().to_vec();
    for o in offs {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&blob);
    pad4(&mut out);
    out
}

/// Children of a container in the order Nintendo writes them: maps sorted by key bytes, hash
/// maps by hash (the BTreeMaps already keep both orders).
fn children(n: &Byml) -> Vec<&Byml> {
    match n {
        Byml::Array(a) => a.iter().collect(),
        Byml::Map(m) => m.values().collect(),
        Byml::Hash32(m) => m.values().collect(),
        Byml::Hash64(m) => m.values().collect(),
        _ => vec![],
    }
}

struct Writer {
    out: Vec<u8>,
    key_idx: BTreeMap<String, u32>,
    str_idx: BTreeMap<String, u32>,
    longs: Vec<u32>,
    next_long: usize,
}

impl Writer {
    /// Write every out-of-line value first, depth first, remembering their offsets.
    fn place_longs(&mut self, n: &Byml) {
        for c in children(n) {
            if c.is_container() {
                self.place_longs(c);
            } else if c.is_long() {
                self.longs.push(self.out.len() as u32);
                match c {
                    Byml::I64(v) => self.out.extend_from_slice(&v.to_le_bytes()),
                    Byml::U64(v) => self.out.extend_from_slice(&v.to_le_bytes()),
                    Byml::F64(v) => self.out.extend_from_slice(&v.to_le_bytes()),
                    Byml::Bin(b) => {
                        self.out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                        self.out.extend_from_slice(b);
                        pad4(&mut self.out);
                    }
                    _ => unreachable!("is_long"),
                }
            }
        }
    }

    fn size_of(n: &Byml) -> usize {
        match n {
            Byml::Array(a) => 4 + ((a.len() + 3) & !3) + 4 * a.len(),
            Byml::Map(m) => 4 + 8 * m.len(),
            Byml::Hash32(m) => (4 + 9 * m.len() + 3) & !3,
            Byml::Hash64(m) => (4 + 13 * m.len() + 3) & !3,
            _ => 0,
        }
    }

    fn scalar(&self, v: &Byml) -> u32 {
        match v {
            Byml::Str(s) => self.str_idx[s],
            Byml::Bool(b) => *b as u32,
            Byml::I32(i) => *i as u32,
            Byml::U32(u) => *u,
            Byml::F32(f) => f.to_bits(),
            Byml::Null => 0,
            _ => unreachable!("not an inline scalar"),
        }
    }

    /// Write a container: reserve its space, then write its children in order (pre-order).
    fn write(&mut self, n: &Byml) -> u32 {
        let start = self.out.len();
        self.out.resize(start + Self::size_of(n), 0);
        let kids = children(n);
        let mut slots = Vec::with_capacity(kids.len());
        for c in &kids {
            if c.is_container() {
                slots.push(self.write(c));
            } else if c.is_long() {
                slots.push(self.longs[self.next_long]);
                self.next_long += 1;
            } else {
                slots.push(self.scalar(c));
            }
        }
        let mut buf = (n.type_code() as u32 | ((kids.len() as u32) << 8)).to_le_bytes().to_vec();
        match n {
            Byml::Array(_) => {
                buf.extend(kids.iter().map(|c| c.type_code()));
                pad4(&mut buf);
                for s in &slots {
                    buf.extend_from_slice(&s.to_le_bytes());
                }
            }
            Byml::Map(m) => {
                for ((k, v), s) in m.iter().zip(&slots) {
                    buf.extend_from_slice(&(self.key_idx[k] | ((v.type_code() as u32) << 24)).to_le_bytes());
                    buf.extend_from_slice(&s.to_le_bytes());
                }
            }
            Byml::Hash32(m) => {
                for (k, s) in m.keys().zip(&slots) {
                    buf.extend_from_slice(&k.to_le_bytes());
                    buf.extend_from_slice(&s.to_le_bytes());
                }
                buf.extend(kids.iter().map(|c| c.type_code()));
            }
            Byml::Hash64(m) => {
                for (k, s) in m.keys().zip(&slots) {
                    buf.extend_from_slice(&k.to_le_bytes());
                    buf.extend_from_slice(&s.to_le_bytes());
                }
                buf.extend(kids.iter().map(|c| c.type_code()));
            }
            _ => unreachable!("is_container"),
        }
        pad4(&mut buf);
        self.out[start..start + buf.len()].copy_from_slice(&buf);
        start as u32
    }
}

/// Serialize. `pad_pow2` zero-pads the output to the next power of two, as the shipped
/// compressed `.bcett.byml` files are.
pub fn to_bytes(root: &Byml, version: u16, pad_pow2: bool) -> Result<Vec<u8>> {
    if !root.is_container() && *root != Byml::Null {
        return err("the root must be a container");
    }
    let mut keys = BTreeSet::new();
    let mut strings = BTreeSet::new();
    collect(root, &mut keys, &mut strings);
    let keys: Vec<String> = keys.into_iter().collect();
    let strings: Vec<String> = strings.into_iter().collect();
    let mut w = Writer {
        out: vec![0; 16],
        key_idx: keys.iter().enumerate().map(|(i, k)| (k.clone(), i as u32)).collect(),
        str_idx: strings.iter().enumerate().map(|(i, s)| (s.clone(), i as u32)).collect(),
        longs: vec![],
        next_long: 0,
    };
    let mut key_off = 0u32;
    let mut str_off = 0u32;
    if !keys.is_empty() {
        key_off = w.out.len() as u32;
        w.out.extend(string_table(&keys));
    }
    if !strings.is_empty() {
        str_off = w.out.len() as u32;
        w.out.extend(string_table(&strings));
    }
    let mut root_off = 0u32;
    if root.is_container() {
        w.place_longs(root);
        root_off = w.write(root);
    }
    w.out[0..2].copy_from_slice(b"YB");
    w.out[2..4].copy_from_slice(&version.to_le_bytes());
    w.out[4..8].copy_from_slice(&key_off.to_le_bytes());
    w.out[8..12].copy_from_slice(&str_off.to_le_bytes());
    w.out[12..16].copy_from_slice(&root_off.to_le_bytes());
    if pad_pow2 {
        let size = w.out.len().next_power_of_two();
        w.out.resize(size, 0);
    }
    Ok(w.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Byml {
        let mut m = Byml::map();
        m.insert("Name", Byml::Str("Coin".into()));
        m.insert("Hash", Byml::U64(0x1234_5678_9abc_def0));
        m.insert("Translate", Byml::floats(&[1.0, -2.5, 3.25]));
        m.insert("Big", Byml::I64(-7));
        m.insert("Ratio", Byml::F64(0.125));
        m.insert("Blob", Byml::Bin(vec![1, 2, 3]));
        m.insert("Flags", Byml::U32(9));
        m.insert("On", Byml::Bool(true));
        m.insert("Nothing", Byml::Null);
        let mut h = BTreeMap::new();
        h.insert(42u64, Byml::I32(-1));
        m.insert("ByHash", Byml::Hash64(h));
        let mut h32 = BTreeMap::new();
        h32.insert(7u32, Byml::Str("seven".into()));
        m.insert("ByHash32", Byml::Hash32(h32));
        let mut root = Byml::map();
        root.insert("Actors", Byml::Array(vec![m.clone(), m]));
        root
    }

    #[test]
    fn round_trip_every_type() {
        let root = sample();
        for pad in [false, true] {
            let bytes = to_bytes(&root, 7, pad).unwrap();
            assert!(!pad || bytes.len().is_power_of_two());
            let (back, ver) = from_bytes(&bytes).unwrap();
            assert_eq!(ver, 7);
            assert_eq!(back, root);
            assert_eq!(to_bytes(&back, 7, pad).unwrap(), bytes, "writing is deterministic");
        }
    }

    #[test]
    fn empty_root() {
        let bytes = to_bytes(&Byml::Null, 7, false).unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(from_bytes(&bytes).unwrap().0, Byml::Null);
        assert!(to_bytes(&Byml::I32(1), 7, false).is_err());
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        let good = to_bytes(&sample(), 7, false).unwrap();
        assert!(from_bytes(b"BY\0\x07").is_err());
        assert!(from_bytes(&good[..20]).is_err());
        // every truncation and every single-byte corruption must fail cleanly or parse
        for n in 0..good.len() {
            let _ = from_bytes(&good[..n]);
        }
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xFF;
            let _ = from_bytes(&bad);
        }
        // a container whose child points back at itself
        let mut cyc = to_bytes(&Byml::Array(vec![Byml::Array(vec![])]), 7, false).unwrap();
        let root = u32::from_le_bytes(cyc[12..16].try_into().unwrap()) as usize;
        let slot = root + 8;
        cyc[slot..slot + 4].copy_from_slice(&(root as u32).to_le_bytes());
        assert!(from_bytes(&cyc).is_err());
    }

    #[test]
    fn helpers() {
        let root = sample();
        let a = &root.array("Actors")[0];
        assert_eq!(a.vec3("Translate", [0.0; 3]), [1.0, -2.5, 3.25]);
        assert_eq!(a.vec3("Missing", [9.0; 3]), [9.0; 3]);
        assert_eq!(vec3(Some(&Byml::F32(2.0)), [0.0; 3]), [2.0; 3]);
        assert_eq!(a.u64_of("Hash"), Some(0x1234_5678_9abc_def0));
        assert_eq!(a.str_or("Name", ""), "Coin");
        assert!(root.array("Nope").is_empty());
        let mut m = Byml::map();
        m.array_mut("List").push(Byml::Null);
        assert_eq!(m.array("List").len(), 1);
    }
}

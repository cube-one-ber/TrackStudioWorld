//! Havok tagfiles (`TAG0`, SDK 2023), as far as the game's collision files need them.
//!
//! A tagfile holds one block of object data (`DATA`), a description of every type in it
//! (`TYPE`) and a list of the objects and arrays in the block (`INDX`/`ITEM`). Pointers in the
//! data are offsets relative to the field that holds them, so the data is used in place.
//!
//! This module reads the type descriptions, so that code can ask for a field by name instead
//! of trusting a fixed offset, and can follow every pointer of an object. It never changes the
//! type section; edits change the data and the item list.

use crate::error::{Error, Result};
use std::collections::BTreeSet;

/// One type of the `TYPE` section. Index 0 is "no type".
#[derive(Clone, Debug, Default)]
pub struct Type {
    /// Name without template arguments, such as `hkRelArray`.
    pub base: String,
    /// Full name, such as `hkRelArray<hkcdSimdTreeNamespace::Node>`.
    pub name: String,
    /// Template arguments: type indices for `t...` parameters, numbers for `v...`.
    pub args: Vec<(String, usize)>,
    pub parent: usize,
    pub size: Option<usize>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub offset: usize,
    pub ty: usize,
}

/// One entry of the item list: an object (`count` 1) or an array in the data block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub ty: usize,
    /// 0x10 for a single object, 0x20 for an array.
    pub kind: u8,
    pub offset: usize,
    pub count: usize,
}

pub const ITEM_OBJECT: u8 = 0x10;
pub const ITEM_ARRAY: u8 = 0x20;

/// A pointer field found by walking a type.
#[derive(Clone, Copy, Debug)]
struct PtrField {
    offset: usize,
    /// 8 for `hkRelPtr` and `hkRelArray` (64-bit offset), 4 for `hkRelArrayView`.
    width: usize,
    /// Whether a count follows the offset.
    array: bool,
}

#[derive(Clone, Debug)]
pub struct Tagfile {
    sdkv: Vec<u8>,
    pub data: Vec<u8>,
    type_section: Vec<u8>,
    pub types: Vec<Type>,
    pub items: Vec<Item>,
}

fn err(msg: impl Into<String>) -> Error {
    Error::new(format!("Havok tagfile: {}", msg.into()))
}

struct Section<'a> {
    tag: [u8; 4],
    body: &'a [u8],
    /// Whole section with its header.
    raw: &'a [u8],
}

/// The sections directly inside `d`.
fn sections(d: &[u8]) -> Result<Vec<Section<'_>>> {
    let mut out = vec![];
    let mut o = 0;
    while o < d.len() {
        let head = d.get(o..o + 8).ok_or_else(|| err("truncated section header"))?;
        let size = (u32::from_be_bytes(head[..4].try_into().unwrap()) & 0x3FFF_FFFF) as usize;
        if size < 8 || o + size > d.len() {
            return Err(err(format!("section at {o:#x} has a bad size")));
        }
        out.push(Section { tag: head[4..8].try_into().unwrap(), body: &d[o + 8..o + size], raw: &d[o..o + size] });
        o += size;
    }
    Ok(out)
}

fn find<'a>(secs: &'a [Section<'a>], tag: &[u8; 4]) -> Result<&'a Section<'a>> {
    secs.iter().find(|s| &s.tag == tag).ok_or_else(|| err(format!("no {} section", String::from_utf8_lossy(tag))))
}

fn section(tag: &[u8; 4], leaf: bool, body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() as u32 + 8) | if leaf { 0x4000_0000 } else { 0 }).to_be_bytes().to_vec();
    out.extend_from_slice(tag);
    out.extend_from_slice(body);
    out
}

/// Havok's variable-length integers: the leading bits of the first byte give the length.
struct Packed<'a> {
    d: &'a [u8],
    o: usize,
}

impl Packed<'_> {
    fn done(&self) -> bool {
        self.o >= self.d.len()
    }

    fn next(&mut self) -> Result<usize> {
        let b = *self.d.get(self.o).ok_or_else(|| err("truncated type data"))?;
        let (len, mask): (usize, u64) = match b >> 3 {
            0x00..=0x0F => (1, 0x7F),
            0x10..=0x17 => (2, 0x3FFF),
            0x18..=0x1B => (3, 0x1F_FFFF),
            0x1C => (4, 0x7FF_FFFF),
            0x1D => (5, 0x7_FFFF_FFFF),
            0x1E => (8, 0x7FF_FFFF_FFFF_FFFF),
            _ => return Err(err("unsupported integer encoding in the type data")),
        };
        let bytes = self.d.get(self.o..self.o + len).ok_or_else(|| err("truncated type data"))?;
        self.o += len;
        let v = bytes.iter().fold(0u64, |acc, &x| (acc << 8) | x as u64) & mask;
        usize::try_from(v).map_err(|_| err("integer out of range in the type data"))
    }
}

fn strings(d: &[u8]) -> Vec<String> {
    d.split(|&c| c == 0).map(|s| String::from_utf8_lossy(s).into_owned()).collect()
}

fn parse_types(type_body: &[u8]) -> Result<Vec<Type>> {
    let secs = sections(type_body)?;
    let tst = strings(find(&secs, b"TST1")?.body);
    let fst = strings(find(&secs, b"FST1")?.body);
    let name_of = |table: &[String], i: usize| table.get(i).cloned().ok_or_else(|| err("bad string index"));

    let mut p = Packed { d: find(&secs, b"TNA1")?.body, o: 0 };
    let n = p.next()?;
    if n == 0 || n > 1 << 16 {
        return Err(err("bad type count"));
    }
    let mut types = vec![Type::default(); n];
    for t in types.iter_mut().skip(1) {
        t.base = name_of(&tst, p.next()?)?;
        for _ in 0..p.next()? {
            let (name, value) = (name_of(&tst, p.next()?)?, p.next()?);
            t.args.push((name, value));
        }
    }

    // Either name the body section has had
    let body = secs.iter().find(|s| &s.tag == b"TBDY" || &s.tag == b"TBOD").ok_or_else(|| err("no TBDY section"))?;
    let mut p = Packed { d: body.body, o: 0 };
    while !p.done() {
        let ti = p.next()?;
        if ti == 0 {
            continue;
        }
        if ti >= n {
            return Err(err("type index out of range"));
        }
        let parent = p.next()?;
        let flags = p.next()?;
        let format = if flags & 0x01 != 0 { p.next()? } else { 0 };
        if flags & 0x02 != 0 && format & 0xF >= 6 {
            p.next()?; // element or pointee type; the template arguments give it too
        }
        if flags & 0x04 != 0 {
            p.next()?; // version
        }
        let mut size = None;
        if flags & 0x08 != 0 {
            size = Some(p.next()?);
            p.next()?; // alignment
        }
        if flags & 0x10 != 0 {
            p.next()?;
        }
        let mut members = vec![];
        if flags & 0x20 != 0 {
            for _ in 0..p.next()? & 0xFFFF {
                let name = name_of(&fst, p.next()?)?;
                if p.next()? & 0x80 != 0 {
                    p.next()?; // attributes
                }
                let (offset, ty) = (p.next()?, p.next()?);
                if ty >= n {
                    return Err(err("member type out of range"));
                }
                members.push(Member { name, offset, ty });
            }
        }
        if flags & 0x40 != 0 {
            for _ in 0..p.next()? {
                p.next()?;
                p.next()?;
            }
        }
        if flags & 0x80 != 0 {
            p.next()?;
        }
        if parent >= n {
            return Err(err("parent type out of range"));
        }
        let t = &mut types[ti];
        (t.parent, t.size, t.members) = (parent, size, members);
    }

    // Full names, after every type has its arguments
    fn full(types: &[Type], i: usize, depth: usize) -> String {
        if i == 0 || i >= types.len() || depth > 16 {
            return "null".into();
        }
        let t = &types[i];
        if t.args.is_empty() {
            return t.base.clone();
        }
        let args: Vec<String> =
            t.args.iter().map(|(n, v)| if n.starts_with('t') { full(types, *v, depth + 1) } else { v.to_string() }).collect();
        format!("{}<{}>", t.base, args.join(","))
    }
    for i in 1..n {
        types[i].name = full(&types, i, 0);
    }
    Ok(types)
}

impl Tagfile {
    /// Parse a tagfile. `d` may have padding after the `TAG0` section.
    pub fn parse(d: &[u8]) -> Result<Tagfile> {
        let top = sections(d.get(..tag0_size(d)?).ok_or_else(|| err("truncated"))?)?;
        let secs = sections(find(&top, b"TAG0")?.body)?;
        let type_sec = find(&secs, b"TYPE")?;
        let types = parse_types(type_sec.body)?;
        let data = find(&secs, b"DATA")?.body.to_vec();
        let indx = sections(find(&secs, b"INDX")?.body)?;
        if indx.iter().any(|s| &s.tag != b"ITEM") {
            return Err(err("patch sections are not supported"));
        }
        let item_body = find(&indx, b"ITEM")?.body;
        if !item_body.len().is_multiple_of(12) {
            return Err(err("bad item list"));
        }
        let mut items = vec![];
        for e in item_body.as_chunks::<12>().0 {
            let w = u32::from_le_bytes(e[..4].try_into().unwrap());
            let item = Item {
                ty: (w & 0xFF_FFFF) as usize,
                kind: (w >> 24) as u8,
                offset: u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize,
                count: u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize,
            };
            if item.ty >= types.len() || item.offset > data.len() {
                return Err(err("item out of range"));
            }
            items.push(item);
        }
        let tag = Tagfile { sdkv: find(&secs, b"SDKV")?.body.to_vec(), data, type_section: type_sec.raw.to_vec(), types, items };
        for it in tag.items.iter().skip(1) {
            let end = tag.size_of(it.ty).checked_mul(it.count).and_then(|n| n.checked_add(it.offset));
            if end.is_none_or(|e| e > tag.data.len()) {
                return Err(err("item runs past the data"));
            }
        }
        Ok(tag)
    }

    /// The `TAG0` section, without padding after it.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut items = Vec::with_capacity(self.items.len() * 12);
        for it in &self.items {
            items.extend_from_slice(&((it.ty as u32) | ((it.kind as u32) << 24)).to_le_bytes());
            items.extend_from_slice(&(it.offset as u32).to_le_bytes());
            items.extend_from_slice(&(it.count as u32).to_le_bytes());
        }
        let mut body = section(b"SDKV", true, &self.sdkv);
        body.extend(section(b"DATA", true, &self.data));
        body.extend_from_slice(&self.type_section);
        body.extend(section(b"INDX", false, &section(b"ITEM", true, &items)));
        section(b"TAG0", false, &body)
    }

    pub fn type_named(&self, name: &str) -> Option<usize> {
        self.types.iter().position(|t| t.name == name)
    }

    /// Size in bytes of a value of type `ty` (its own, or the one it inherits).
    pub fn size_of(&self, mut ty: usize) -> usize {
        for _ in 0..32 {
            let Some(t) = self.types.get(ty).filter(|_| ty != 0) else { break };
            if let Some(s) = t.size {
                return s;
            }
            ty = t.parent;
        }
        0
    }

    /// Whether `ty` is the type called `base` or inherits from it.
    pub fn is_a(&self, mut ty: usize, base: &str) -> bool {
        for _ in 0..32 {
            let Some(t) = self.types.get(ty).filter(|_| ty != 0) else { break };
            if t.base == base {
                return true;
            }
            ty = t.parent;
        }
        false
    }

    /// Offset and type of the field `name` of `ty`, looking through the types it inherits from.
    pub fn member(&self, ty: usize, name: &str) -> Result<(usize, usize)> {
        let mut t = ty;
        for _ in 0..32 {
            let Some(info) = self.types.get(t).filter(|_| t != 0) else { break };
            if let Some(m) = info.members.iter().find(|m| m.name == name) {
                return Ok((m.offset, m.ty));
            }
            t = info.parent;
        }
        Err(err(format!("{} has no field {name}", self.types.get(ty).map_or("?", |t| t.name.as_str()))))
    }

    /// Offset of a field given as a path, such as `["simdTree", "nodes"]`.
    pub fn field(&self, ty: usize, path: &[&str]) -> Result<(usize, usize)> {
        let (mut off, mut t) = (0, ty);
        for name in path {
            let (o, mt) = self.member(t, name)?;
            off += o;
            t = mt;
        }
        Ok((off, t))
    }

    /// The single object that starts at `offset`.
    pub fn object_at(&self, offset: usize) -> Option<&Item> {
        self.items.iter().skip(1).find(|i| i.kind == ITEM_OBJECT && i.offset == offset)
    }

    fn bytes(&self, o: usize, n: usize) -> Result<&[u8]> {
        o.checked_add(n).and_then(|e| self.data.get(o..e)).ok_or_else(|| err(format!("read past the data at {o:#x}")))
    }

    pub fn u8(&self, o: usize) -> Result<u8> {
        Ok(self.bytes(o, 1)?[0])
    }
    pub fn u16(&self, o: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(o, 2)?.try_into().unwrap()))
    }
    pub fn u32(&self, o: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(o, 4)?.try_into().unwrap()))
    }
    pub fn i32(&self, o: usize) -> Result<i32> {
        Ok(self.u32(o)? as i32)
    }
    pub fn u64(&self, o: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(o, 8)?.try_into().unwrap()))
    }
    pub fn f32(&self, o: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(o)?))
    }
    pub fn f32s<const N: usize>(&self, o: usize) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.f32(o + 4 * i)?;
        }
        Ok(out)
    }

    pub fn put(&mut self, o: usize, bytes: &[u8]) -> Result<()> {
        let end = o.checked_add(bytes.len()).filter(|&e| e <= self.data.len()).ok_or_else(|| err("write past the data"))?;
        self.data[o..end].copy_from_slice(bytes);
        Ok(())
    }

    /// Target of a 64-bit relative pointer at `o`; `None` when it is null.
    pub fn rel_ptr(&self, o: usize) -> Result<Option<usize>> {
        let rel = self.u64(o)? as i64;
        if rel == 0 {
            return Ok(None);
        }
        let t = (o as i64).checked_add(rel).filter(|&t| t >= 0 && (t as usize) <= self.data.len());
        t.map(|t| Some(t as usize)).ok_or_else(|| err(format!("pointer at {o:#x} leaves the data")))
    }

    pub fn set_rel_ptr(&mut self, o: usize, target: Option<usize>) -> Result<()> {
        let rel = target.map_or(0, |t| t as i64 - o as i64);
        self.put(o, &rel.to_le_bytes())
    }

    /// Start and length of a `hkRelArray` at `o` (64-bit offset, then a 32-bit count).
    pub fn rel_array(&self, o: usize) -> Result<(usize, usize)> {
        let n = self.i32(o + 8)?;
        let start = self.rel_ptr(o)?.unwrap_or(o);
        usize::try_from(n).map(|n| (start, n)).map_err(|_| err("negative array length"))
    }

    /// Start and length of a `hkRelArrayView` at `o` (32-bit offset and count).
    pub fn rel_view(&self, o: usize) -> Result<(usize, usize)> {
        let (rel, n) = (self.i32(o)?, self.i32(o + 4)?);
        let start = (o as i64 + rel as i64) as usize;
        if rel != 0 && start > self.data.len() {
            return Err(err(format!("array at {o:#x} leaves the data")));
        }
        usize::try_from(n).map(|n| (start, n)).map_err(|_| err("negative array length"))
    }

    /// Add zeroed bytes at the end of the data, aligned to 16, and return where they start.
    pub fn append(&mut self, len: usize) -> usize {
        let start = self.data.len().next_multiple_of(16);
        self.data.resize((start + len).next_multiple_of(16), 0);
        start
    }

    /// Every pointer field of a value of type `ty`, relative to the value's start.
    fn pointer_fields(&self, ty: usize, base: usize, depth: usize, out: &mut Vec<PtrField>) {
        let Some(t) = self.types.get(ty).filter(|_| ty != 0 && depth < 24) else { return };
        match t.base.as_str() {
            "hkRelPtr" => out.push(PtrField { offset: base, width: 8, array: false }),
            "hkRelArray" => out.push(PtrField { offset: base, width: 8, array: true }),
            "hkRelArrayView" => out.push(PtrField { offset: base, width: 4, array: true }),
            "T[N]" => {
                let elem = t.args.iter().find(|a| a.0.starts_with('t')).map_or(0, |a| a.1);
                let n = t.args.iter().find(|a| a.0.starts_with('v')).map_or(0, |a| a.1);
                let step = self.size_of(elem);
                let mut probe = vec![];
                self.pointer_fields(elem, 0, depth + 1, &mut probe);
                if !probe.is_empty() && step > 0 {
                    for i in 0..n.min(4096) {
                        out.extend(probe.iter().map(|p| PtrField { offset: base + i * step + p.offset, ..*p }));
                    }
                }
            }
            _ => {
                self.pointer_fields(t.parent, base, depth + 1, out);
                for m in &t.members {
                    self.pointer_fields(m.ty, base + m.offset, depth + 1, out);
                }
            }
        }
    }

    /// Where the pointers stored in `item` lead: the start of each non-empty target.
    fn targets(&self, item: &Item) -> Result<Vec<usize>> {
        let mut fields = vec![];
        self.pointer_fields(item.ty, 0, 0, &mut fields);
        let step = self.size_of(item.ty);
        let mut out = vec![];
        for i in 0..item.count {
            for f in &fields {
                let o = item.offset + i * step + f.offset;
                match (f.width, f.array) {
                    (8, false) => out.extend(self.rel_ptr(o)?),
                    (8, true) => {
                        let (start, n) = self.rel_array(o)?;
                        if n > 0 {
                            out.push(start);
                        }
                    }
                    _ => {
                        let (start, n) = self.rel_view(o)?;
                        if n > 0 {
                            out.push(start);
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Index of the item whose bytes contain `offset`.
    fn item_containing(&self, offset: usize) -> Option<usize> {
        // An exact start wins: an empty object can share its start with what follows it
        self.items.iter().skip(1).position(|i| i.offset == offset).map(|i| i + 1).or_else(|| {
            self.items
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, i)| offset >= i.offset && offset < i.offset + self.size_of(i.ty) * i.count)
                .map(|(i, _)| i)
        })
    }

    /// Indices of the items an item reaches through its pointers, itself included, in the
    /// order of the item list.
    pub fn reachable(&self, from: usize) -> Result<Vec<usize>> {
        let mut seen = BTreeSet::from([from]);
        let mut todo = vec![from];
        while let Some(i) = todo.pop() {
            let item = *self.items.get(i).ok_or_else(|| err("no such item"))?;
            for t in self.targets(&item)? {
                let j = self.item_containing(t).ok_or_else(|| err(format!("pointer to {t:#x}, which no item covers")))?;
                if seen.insert(j) {
                    todo.push(j);
                }
            }
        }
        Ok(seen.into_iter().collect())
    }

    /// Take over `src`'s type descriptions, which must cover every type this file's items use.
    /// The type section is copied whole, so it stays as consistent as it was in `src`.
    pub fn adopt_types(&mut self, src: &Tagfile) -> Result<()> {
        let mut mapped = vec![0];
        for it in self.items.iter().skip(1) {
            let name = &self.types[it.ty].name;
            let ty = src.type_named(name).ok_or_else(|| err(format!("the other file has no type {name}")))?;
            if src.size_of(ty) != self.size_of(it.ty) {
                return Err(err(format!("{name} differs between the two files")));
            }
            mapped.push(ty);
        }
        for (it, ty) in self.items.iter_mut().zip(mapped) {
            it.ty = ty;
        }
        (self.types, self.type_section) = (src.types.clone(), src.type_section.clone());
        Ok(())
    }

    /// Copy the object `from` of `src`, and everything it points to, into this file. When this
    /// file lacks a type the copy needs, it first takes over `src`'s type descriptions. The copy
    /// keeps the source's layout, so its relative pointers stay valid as they are. Returns
    /// where the object now starts. Fails when neither file describes every type of both.
    pub fn import(&mut self, src: &Tagfile, from: usize) -> Result<usize> {
        let reach = src.reachable(from)?;
        let span = |i: &usize| {
            let it = &src.items[*i];
            (it.offset, it.offset + src.size_of(it.ty) * it.count)
        };
        let lo = reach.iter().map(|i| span(i).0).min().unwrap_or(0) & !15;
        let hi = reach.iter().map(|i| span(i).1).max().unwrap_or(0);
        if reach.iter().any(|i| self.type_named(&src.types[src.items[*i].ty].name).is_none()) {
            self.adopt_types(src)?;
        }
        let mut mapped = vec![];
        for i in &reach {
            let name = &src.types[src.items[*i].ty].name;
            let ty = self.type_named(name).ok_or_else(|| err(format!("this file has no type {name}")))?;
            if self.size_of(ty) != src.size_of(src.items[*i].ty) {
                return Err(err(format!("{name} differs between the two files")));
            }
            mapped.push(ty);
        }
        let start = self.append(hi - lo);
        self.data[start..start + (hi - lo)].copy_from_slice(&src.data[lo..hi]);
        for (i, ty) in reach.iter().zip(mapped) {
            let it = src.items[*i];
            self.items.push(Item { ty, offset: it.offset - lo + start, ..it });
        }
        Ok(src.items[from].offset - lo + start)
    }
}

/// Length of the `TAG0` section that starts `d`.
pub fn tag0_size(d: &[u8]) -> Result<usize> {
    let head = d.get(..8).ok_or_else(|| err("truncated"))?;
    if &head[4..8] != b"TAG0" {
        return Err(err("not a tagfile"));
    }
    let size = (u32::from_be_bytes(head[..4].try_into().unwrap()) & 0x3FFF_FFFF) as usize;
    if size < 8 || size > d.len() { Err(err("truncated")) } else { Ok(size) }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn packed(v: usize) -> Vec<u8> {
        match v {
            0..=0x7F => vec![v as u8],
            0x80..=0x3FFF => vec![0x80 | (v >> 8) as u8, v as u8],
            _ => vec![0xC0 | (v >> 16) as u8, (v >> 8) as u8, v as u8],
        }
    }

    /// Builds a `TYPE` section from a readable description.
    #[derive(Default)]
    pub struct Types {
        names: Vec<String>,
        fields: Vec<String>,
        tna: Vec<u8>,
        body: Vec<u8>,
        count: usize,
    }

    impl Types {
        fn s(table: &mut Vec<String>, s: &str) -> usize {
            table.iter().position(|x| x == s).unwrap_or_else(|| {
                table.push(s.into());
                table.len() - 1
            })
        }

        /// Add a type; returns its index. `args` are `("tT", type index)` or `("vN", number)`.
        pub fn add(
            &mut self,
            base: &str,
            args: &[(&str, usize)],
            parent: usize,
            size: Option<usize>,
            members: &[(&str, usize, usize)],
        ) -> usize {
            self.count += 1;
            let ti = self.count;
            self.tna.extend(packed(Self::s(&mut self.names, base)));
            self.tna.extend(packed(args.len()));
            for (n, v) in args {
                self.tna.extend(packed(Self::s(&mut self.names, n)));
                self.tna.extend(packed(*v));
            }
            let flags = if size.is_some() { 0x08 } else { 0 } | if members.is_empty() { 0 } else { 0x20 };
            self.body.extend(packed(ti));
            self.body.extend(packed(parent));
            self.body.extend(packed(flags));
            if let Some(s) = size {
                self.body.extend(packed(s));
                self.body.extend(packed(8));
            }
            if !members.is_empty() {
                self.body.extend(packed(members.len()));
                for (n, off, ty) in members {
                    self.body.extend(packed(Self::s(&mut self.fields, n)));
                    self.body.extend(packed(0x20));
                    self.body.extend(packed(*off));
                    self.body.extend(packed(*ty));
                }
            }
            ti
        }

        pub fn section(&self) -> Vec<u8> {
            let join = |t: &[String]| t.iter().flat_map(|s| s.bytes().chain([0])).collect::<Vec<u8>>();
            let mut tna = packed(self.count + 1);
            tna.extend_from_slice(&self.tna);
            let mut body = section(b"TST1", true, &join(&self.names));
            body.extend(section(b"TNA1", true, &tna));
            body.extend(section(b"FST1", true, &join(&self.fields)));
            body.extend(section(b"TBDY", true, &self.body));
            section(b"TYPE", false, &body)
        }
    }

    /// A tagfile from parts, for tests.
    pub fn build(types: &Types, data: &[u8], items: &[Item]) -> Vec<u8> {
        let type_section = types.section();
        let t = Tagfile {
            sdkv: b"20230200".to_vec(),
            data: data.to_vec(),
            types: parse_types(&type_section[8..]).unwrap(),
            type_section,
            items: [Item { ty: 0, kind: 0, offset: 0, count: 0 }].iter().chain(items).copied().collect(),
        };
        t.to_bytes()
    }

    /// Two nodes: `Node { next: hkRelPtr<Node>, values: hkRelArray<int>, value: int }`.
    fn sample() -> (Types, Vec<u8>, Vec<Item>) {
        let mut t = Types::default();
        let int = t.add("int", &[], 0, Some(4), &[]);
        let long = t.add("hkInt64", &[], 0, Some(8), &[]);
        let node = 5;
        let ptr = t.add("hkRelPtr", &[("tT", node)], 0, Some(8), &[("offset", 0, long)]);
        let arr = t.add("hkRelArray", &[("tT", int)], 0, Some(16), &[("offset", 0, long), ("size", 8, int)]);
        assert_eq!(t.add("Node", &[], 0, Some(32), &[("next", 0, ptr), ("values", 8, arr), ("value", 24, int)]), node);
        let mut data = vec![0u8; 80];
        data[0..8].copy_from_slice(&32i64.to_le_bytes()); // first.next -> second at 32
        data[8..16].copy_from_slice(&56i64.to_le_bytes()); // first.values -> 64
        data[16..20].copy_from_slice(&3i32.to_le_bytes());
        data[24..28].copy_from_slice(&7i32.to_le_bytes());
        data[32 + 24..32 + 28].copy_from_slice(&9i32.to_le_bytes());
        for (i, v) in [1i32, 2, 3].iter().enumerate() {
            data[64 + 4 * i..68 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        let items = vec![
            Item { ty: node, kind: ITEM_OBJECT, offset: 0, count: 1 },
            Item { ty: node, kind: ITEM_OBJECT, offset: 32, count: 1 },
            Item { ty: int, kind: ITEM_ARRAY, offset: 64, count: 3 },
        ];
        (t, data, items)
    }

    #[test]
    fn round_trip_fields_and_pointers() {
        let (types, data, items) = sample();
        let bytes = build(&types, &data, &items);
        let t = Tagfile::parse(&bytes).unwrap();
        assert_eq!(t.to_bytes(), bytes);
        let node = t.type_named("Node").unwrap();
        assert_eq!(t.types[t.member(node, "next").unwrap().1].name, "hkRelPtr<Node>");
        assert_eq!(t.field(node, &["values", "size"]).unwrap().0, 16);
        assert!(t.member(node, "missing").is_err());
        assert_eq!(t.rel_ptr(0).unwrap(), Some(32));
        assert_eq!(t.rel_ptr(32).unwrap(), None);
        assert_eq!(t.rel_array(8).unwrap(), (64, 3));
        assert_eq!(t.reachable(1).unwrap(), [1, 2, 3]);
        assert_eq!(t.reachable(2).unwrap(), [2]);
        assert_eq!(t.object_at(32).unwrap().count, 1);
    }

    #[test]
    fn import_keeps_pointers_working() {
        let (types, data, items) = sample();
        let src = Tagfile::parse(&build(&types, &data, &items)).unwrap();
        let mut dst = Tagfile::parse(&build(&types, &[0u8; 24], &[])).unwrap();
        let at = dst.import(&src, 1).unwrap();
        assert_eq!(at % 16, 0);
        let next = dst.rel_ptr(at).unwrap().unwrap();
        assert_eq!(dst.i32(next + 24).unwrap(), 9);
        let (vals, n) = dst.rel_array(at + 8).unwrap();
        assert_eq!((dst.i32(vals + 8).unwrap(), n), (3, 3));
        assert_eq!(dst.items.len(), 4);
        let again = Tagfile::parse(&dst.to_bytes()).unwrap();
        assert_eq!(again.reachable(1).unwrap(), [1, 2, 3]);
        // a file with fewer types takes over the source's, keeping its own items valid
        let mut other = Types::default();
        let int = other.add("int", &[], 0, Some(4), &[]);
        let own = [Item { ty: int, kind: ITEM_ARRAY, offset: 0, count: 2 }];
        let mut small = Tagfile::parse(&build(&other, &[5, 0, 0, 0, 6, 0, 0, 0], &own)).unwrap();
        let at = small.import(&src, 1).unwrap();
        assert_eq!(small.types[small.items[1].ty].name, "int");
        assert_eq!(small.rel_array(at + 8).unwrap().1, 3);
        assert_eq!(Tagfile::parse(&small.to_bytes()).unwrap().items, small.items);
        // but not when it has a type the source lacks
        other.add("Other", &[], 0, Some(4), &[]);
        let odd = [Item { ty: 2, kind: ITEM_OBJECT, offset: 0, count: 1 }];
        let mut bare = Tagfile::parse(&build(&other, &[0; 4], &odd)).unwrap();
        assert!(bare.import(&src, 1).is_err());
    }

    #[test]
    fn corrupt_input_fails_cleanly() {
        let (types, data, items) = sample();
        let bytes = build(&types, &data, &items);
        for n in 0..bytes.len() {
            assert!(Tagfile::parse(&bytes[..n]).is_err(), "truncated to {n}");
        }
        for i in 0..bytes.len() {
            let mut b = bytes.clone();
            b[i] ^= 0xFF;
            if let Ok(t) = Tagfile::parse(&b) {
                for item in 1..t.items.len() {
                    let _ = t.reachable(item);
                }
            }
        }
    }
}

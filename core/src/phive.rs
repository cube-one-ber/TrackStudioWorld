//! The game's baked collision: `Phive/StaticCompoundBody/**/*.bphsc.zs`.
//!
//! Objects whose type has `IsUseStaticCompound` (arrow signs, grandstands, flags, the rescue
//! and valley areas...) get no collision of their own in game. Their shapes are baked into one
//! file per course, at the place each object had when Nintendo built the game, so a course
//! file alone cannot move, add or remove them. This module reads and rewrites those files.
//!
//! A `.bphsc` is a Phive container (a header with a table of sections). Its sections are:
//!
//! | # | Holds |
//! |---|---|
//! | 0 | Which collision layers have a body, in fixed-size tables (kept as it is) |
//! | 1 | Links: one per object that has physics, sorted by type name: the object's `Hash`, its `SRTHash`, and the range of its entries in section 4 (none for objects that are not baked) |
//! | 2, 3 | A hash table over the links: twice as many slots as links, slot = hash mod slots with linear probing; section 3 gives the link index of each slot |
//! | 4 | One entry per baked shape: its layer, its link, its index in its body, and its materials in sections 5 and 6 |
//! | 5, 6 | Material tables (kept; grown when a shape is brought in from another file) |
//! | 7 to 10 | Not used by courses' objects (kept) |
//! | 11... | One Havok tagfile per body: a `hknpCompoundShape` whose instances are the baked shapes |
//!
//! Each instance holds the object's translation, rotation (a quaternion of the course file's
//! `Rotate`) and scale, and points at a shape stored once in the same tagfile. Its `shapeTag`
//! is the index of its section 4 entry. A tree of bounding boxes over the instances
//! (`hkcdSimdTree`, four children per node) is what collision queries walk, so it is rebuilt
//! whenever an instance changes.
//!
//! All of this is from the shipped files: every one of them is reproduced byte for byte by
//! [`StaticCompound::to_bytes`] (see `docs/format.md`).

use crate::error::{Error, Result};
use crate::hktag::{ITEM_ARRAY, Item, Tagfile};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

fn err(msg: impl Into<String>) -> Error {
    Error::new(format!("collision file: {}", msg.into()))
}

/// A Phive container: a header and a table of sections.
#[derive(Clone, Debug)]
pub struct Phive {
    /// Header bytes 6 to 11: version, byte order mark, and the number of table slots.
    head: [u8; 6],
    pub sections: Vec<Vec<u8>>,
    /// Slots with an offset in the table (empty sections still have one).
    placed: Vec<bool>,
}

impl Phive {
    pub fn parse(d: &[u8]) -> Result<Phive> {
        if d.len() < 12 || &d[..6] != b"Phive\0" {
            return Err(err("not a Phive file"));
        }
        if d[8..10] != [0xFF, 0xFE] {
            return Err(err("big-endian files are not supported"));
        }
        let n = d[11] as usize;
        let table = d.get(12..12 + 8 * n).ok_or_else(|| err("truncated header"))?;
        let word = |i: usize| u32::from_le_bytes(table[4 * i..4 * i + 4].try_into().unwrap()) as usize;
        let mut sections = Vec::with_capacity(n);
        let mut placed = Vec::with_capacity(n);
        for i in 0..n {
            let (off, size) = (word(i), word(n + i));
            let body = off.checked_add(size).and_then(|e| d.get(off..e)).ok_or_else(|| err(format!("section {i} runs past the file")))?;
            sections.push(body.to_vec());
            placed.push(off != 0);
        }
        Ok(Phive { head: d[6..12].try_into().unwrap(), sections, placed })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.sections.len();
        let mut at = (12 + 8 * n).next_multiple_of(16);
        let mut out = b"Phive\0".to_vec();
        out.extend_from_slice(&self.head);
        let mut offsets = vec![];
        for (s, placed) in self.sections.iter().zip(&self.placed) {
            offsets.push(if *placed || !s.is_empty() { at as u32 } else { 0 });
            at += s.len().next_multiple_of(16);
        }
        out.extend(offsets.iter().flat_map(|o| o.to_le_bytes()));
        out.extend(self.sections.iter().flat_map(|s| (s.len() as u32).to_le_bytes()));
        for s in &self.sections {
            out.resize(out.len().next_multiple_of(16), 0);
            out.extend_from_slice(s);
        }
        out.resize(out.len().next_multiple_of(16), 0);
        out
    }
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// What the game stores for "no box": slightly inside the largest float, inverted.
    const EMPTY: Aabb = Aabb { min: Vec3::splat(f32::from_bits(0x7F7F_FFEE)), max: Vec3::splat(f32::from_bits(0xFF7F_FFEE)) };

    fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    fn union(self, o: Aabb) -> Aabb {
        if o.is_empty() {
            self
        } else if self.is_empty() {
            o
        } else {
            Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
        }
    }

    fn of(points: impl IntoIterator<Item = Vec3>) -> Aabb {
        points.into_iter().fold(Aabb::EMPTY, |a, p| a.union(Aabb { min: p, max: p }))
    }

    pub fn contains(&self, o: &Aabb, slack: f32) -> bool {
        (self.min - slack).cmple(o.min).all() && (self.max + slack).cmpge(o.max).all()
    }
}

/// `hknpShapeInstance` flag bits.
const HAS_TRANSLATION: u8 = 0x02;
const HAS_ROTATION: u8 = 0x04;
const HAS_SCALE: u8 = 0x08;

/// One baked shape: where it is, and which shape of its body it uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    /// Offset of the shape in the body's data.
    pub shape: usize,
    /// The instance as stored, for the bytes this module does not interpret (its entry index
    /// and shape pointer zeroed).
    raw: Vec<u8>,
}

impl Instance {
    /// Move the instance. Its flags follow: rotation and scale are marked when present.
    pub fn place(&mut self, translation: Vec3, rotation: Quat, scale: Vec3) {
        (self.translation, self.rotation, self.scale) = (translation, rotation.normalize(), scale);
    }

    fn flags(&self, stored: u8) -> u8 {
        // The shipped files leave the bit clear below about 0.1 degrees: w of 0.99999976 is
        // unrotated, the next smaller values are rotated
        let rotated = self.rotation.w.abs() < 1.0 - 2.5 * f32::EPSILON;
        let scaled = self.scale != Vec3::ONE;
        (stored & !(HAS_ROTATION | HAS_SCALE))
            | HAS_TRANSLATION
            | if rotated { HAS_ROTATION } else { 0 }
            | if scaled { HAS_SCALE } else { 0 }
    }
}

/// A baked shape of an object.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Index of the body (tagfile) the instance lives in.
    pub body: usize,
    /// Collision layer, as stored: layer index in the high bits, entity (0) or sensor (1) below.
    pub layer: u32,
    /// First entry and count in the material tables.
    pub materials: (u32, u32),
    pub instance: Instance,
    /// Box of the instance in the body's tree; `None` to compute it when writing.
    bounds: Option<Aabb>,
}

impl Part {
    /// Move the shape; its box in the tree is worked out again when the file is written.
    pub fn place(&mut self, translation: Vec3, rotation: Quat, scale: Vec3) {
        self.instance.place(translation, rotation, scale);
        self.bounds = None;
    }
}

/// An object the file knows: every object with physics has a link, baked or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub hash: u64,
    pub srt_hash: u32,
    /// The object's baked shapes; empty for objects that carry their own collision.
    pub parts: Vec<Part>,
    tail: [u8; 12],
}

impl Link {
    pub fn new(hash: u64, srt_hash: u32) -> Link {
        Link { hash, srt_hash, parts: vec![], tail: [0; 12] }
    }
}

/// Field offsets of the Havok types used here, looked up by name in the file's own type
/// descriptions.
#[derive(Clone, Debug)]
struct Fields {
    // hknpCompoundShape
    key_bits: usize,
    elements: usize,
    first_free: usize,
    allocated: usize,
    aabb_min: usize,
    aabb_max: usize,
    radius: usize,
    keys: usize,
    tree_data: usize,
    // hknpCompoundShapeData
    nodes: usize,
    // hknpShapeInstance
    inst_size: usize,
    i_rotation: usize,
    i_translation: usize,
    i_flags: usize,
    i_scale: usize,
    i_tag: usize,
    i_shape: usize,
    // hkcdSimdTree node
    node_size: usize,
    n_lo: [usize; 3],
    n_hi: [usize; 3],
    n_data: usize,
    n_flags: usize,
    n_depth: usize,
    n_parent: usize,
    n_leaves: usize,
}

impl Fields {
    fn of(t: &Tagfile) -> Result<Fields> {
        let named = |n: &str| t.type_named(n).ok_or_else(|| err(format!("no type {n}")));
        let (compound, inst, data, node) = (
            named("hknpCompoundShape")?,
            named("hknpShapeInstance")?,
            named("hknpCompoundShapeData")?,
            named("hkcdSimdTreeNamespace::Node")?,
        );
        let f = |ty: usize, path: &[&str]| t.field(ty, path).map(|x| x.0);
        let f = Fields {
            key_bits: f(compound, &["numShapeKeyBits"])?,
            elements: f(compound, &["instances", "elements"])?,
            first_free: f(compound, &["instances", "firstFree"])?,
            allocated: f(compound, &["instances", "numAllocated"])?,
            aabb_min: f(compound, &["aabb", "min"])?,
            aabb_max: f(compound, &["aabb", "max"])?,
            radius: f(compound, &["boundingRadius"])?,
            keys: f(compound, &["estimatedNumShapeKeys"])?,
            tree_data: f(compound, &["boundingVolumeData"])?,
            nodes: f(data, &["simdTree", "nodes"])?,
            inst_size: t.size_of(inst),
            i_rotation: f(inst, &["rotation"])?,
            i_translation: f(inst, &["translation"])?,
            i_flags: f(inst, &["flags"])?,
            i_scale: f(inst, &["scale"])?,
            i_tag: f(inst, &["shapeTag"])?,
            i_shape: f(inst, &["shape"])?,
            node_size: t.size_of(node),
            n_lo: [f(node, &["lx"])?, f(node, &["ly"])?, f(node, &["lz"])?],
            n_hi: [f(node, &["hx"])?, f(node, &["hy"])?, f(node, &["hz"])?],
            n_data: f(node, &["data"])?,
            n_flags: f(node, &["flags"])?,
            n_depth: f(node, &["maxDepth"])?,
            n_parent: f(node, &["parent"])?,
            n_leaves: f(node, &["numLeaves"])?,
        };
        // The layout this module writes: refuse anything else rather than guess
        let inst = [(f.i_rotation, 16), (f.i_translation, 12), (f.i_flags, 1), (f.i_scale, 12), (f.i_tag, 2), (f.i_shape, 8)];
        let node = [(f.n_data, 16), (f.n_flags, 1), (f.n_depth, 1), (f.n_parent, 4), (f.n_leaves, 4)];
        let boxes = f.n_lo.iter().chain(&f.n_hi).map(|&o| (o, 16));
        if f.inst_size != 64
            || f.node_size != 128
            || inst.iter().any(|(o, n)| o + n > f.inst_size)
            || node.into_iter().chain(boxes).any(|(o, n)| o + n > f.node_size)
        {
            return Err(err("unexpected Havok type sizes"));
        }
        Ok(f)
    }
}

/// A node of the bounding-box tree: four children, each a node or an instance.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    boxes: [Aabb; 4],
    /// Child node indices, or instance indices when `leaf`.
    data: [u32; 4],
    leaf: bool,
    depth: u8,
    parent: u32,
    leaves: u32,
}

const NO_INSTANCE: u32 = u32::MAX;

/// A tree over `boxes` (one per instance). Node 0 is the unused node the format keeps, node 1
/// the root; children come after their parent.
fn build_tree(boxes: &[Aabb]) -> Vec<Node> {
    let blank = |leaf: bool, parent: u32| Node {
        boxes: [Aabb::EMPTY; 4],
        data: if leaf { [NO_INSTANCE; 4] } else { [0; 4] },
        leaf,
        depth: 0,
        parent,
        leaves: 0,
    };
    let mut nodes = vec![blank(false, 0), blank(true, 0)];
    let mut todo = std::collections::VecDeque::from([(1usize, (0..boxes.len()).collect::<Vec<usize>>())]);
    // Cut a group in two across the axis its box centres spread most along
    let cut = |mut ids: Vec<usize>, first: usize| -> (Vec<usize>, Vec<usize>) {
        let centre = |i: usize| boxes[i].min + boxes[i].max;
        let spread = Aabb::of(ids.iter().map(|&i| centre(i)));
        let size = spread.max - spread.min;
        let axis = if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        };
        ids.sort_by(|&a, &b| centre(a)[axis].total_cmp(&centre(b)[axis]).then(a.cmp(&b)));
        let back = ids.split_off(first);
        (ids, back)
    };
    while let Some((at, ids)) = todo.pop_front() {
        if ids.len() <= 4 {
            nodes[at].leaf = true;
            nodes[at].data = [NO_INSTANCE; 4];
            for (slot, &i) in ids.iter().enumerate() {
                nodes[at].data[slot] = i as u32;
                nodes[at].boxes[slot] = boxes[i];
            }
            continue;
        }
        // Children that are full subtrees, but for the last: the fewest nodes, so a tree for
        // as many instances as before fits where the old one was
        let mut full = 4;
        while full * 4 < ids.len() {
            full *= 4;
        }
        let mut groups = vec![(ids, 0)];
        groups[0].1 = groups[0].0.len().div_ceil(full);
        while let Some(i) = groups.iter().position(|g| g.1 > 1) {
            let (ids, children) = groups.remove(i);
            let first = children.div_ceil(2);
            let (a, b) = cut(ids, first * full);
            groups.insert(i, (b, children - first));
            groups.insert(i, (a, first));
        }
        nodes[at].leaf = false;
        nodes[at].data = [0; 4];
        for (slot, (group, _)) in groups.into_iter().enumerate() {
            let child = nodes.len();
            nodes.push(blank(true, at as u32));
            nodes[at].data[slot] = child as u32;
            todo.push_back((child, group));
        }
    }
    // Children have larger indices than their parent: fill boxes and counts from the end
    for at in (1..nodes.len()).rev() {
        if nodes[at].leaf {
            nodes[at].depth = 1;
            nodes[at].leaves = nodes[at].data.iter().filter(|&&d| d != NO_INSTANCE).count() as u32;
        } else {
            for slot in 0..4 {
                let child = nodes[at].data[slot] as usize;
                if child != 0 {
                    nodes[at].boxes[slot] = nodes[child].boxes.iter().fold(Aabb::EMPTY, |a, b| a.union(*b));
                    nodes[at].depth = nodes[at].depth.max(nodes[child].depth + 1);
                    nodes[at].leaves += nodes[child].leaves;
                }
            }
        }
    }
    nodes
}

/// What is needed of a shape to bound an instance of it.
enum Local {
    /// Meshes and nested compounds: the game takes the box of the transformed corners.
    Box(Aabb),
    /// Convex shapes: vertices and a radius around them.
    Convex { points: Vec<Vec3>, radius: f32, kind: Convex },
}

#[derive(Clone, Copy, PartialEq)]
enum Convex {
    Box,
    /// Spheres and capsules.
    Round,
    Other,
}

/// One body: a tagfile with a compound shape.
#[derive(Clone, Debug)]
struct Body {
    tag: Tagfile,
    f: Fields,
    /// Offset of the compound shape (the root object).
    root: usize,
    /// Instances as read, with their section 4 index.
    read: Vec<(Instance, u16)>,
}

impl Body {
    fn parse(d: &[u8]) -> Result<Body> {
        let tag = Tagfile::parse(d)?;
        let f = Fields::of(&tag)?;
        let root = tag.items.get(1).filter(|i| tag.types[i.ty].name == "hknpCompoundShape").ok_or_else(|| err("no compound shape"))?.offset;
        let (start, n) = tag.rel_array(root + f.elements)?;
        if tag.i32(root + f.first_free)? != -1 || tag.u32(root + f.allocated)? as usize != n {
            return Err(err("a body has free instance slots"));
        }
        if n.checked_mul(f.inst_size).and_then(|len| len.checked_add(start)).is_none_or(|end| end > tag.data.len()) {
            return Err(err("instances run past the data"));
        }
        let mut read = Vec::with_capacity(n);
        for i in 0..n {
            let o = start + i * f.inst_size;
            let mut raw = tag.data.get(o..o + f.inst_size).ok_or_else(|| err("instances run past the data"))?.to_vec();
            // Kept apart: the entry index, and the pointer, which depends on where the instance is
            raw[f.i_tag..f.i_tag + 2].fill(0);
            raw[f.i_shape..f.i_shape + 8].fill(0);
            let shape = tag.rel_ptr(o + f.i_shape)?.ok_or_else(|| err("an instance has no shape"))?;
            let inst = Instance {
                translation: Vec3::from(tag.f32s::<3>(o + f.i_translation)?),
                rotation: Quat::from_array(tag.f32s::<4>(o + f.i_rotation)?),
                scale: Vec3::from(tag.f32s::<3>(o + f.i_scale)?),
                shape,
                raw,
            };
            read.push((inst, tag.u16(o + f.i_tag)?));
        }
        let body = Body { tag, f, root, read };
        let (_, start, n) = body.nodes()?;
        if n.checked_mul(body.f.node_size).and_then(|len| len.checked_add(start)).is_none_or(|end| end > body.tag.data.len()) {
            return Err(err("the tree runs past the data"));
        }
        Ok(body)
    }

    /// Start and count of the tree's nodes.
    fn nodes(&self) -> Result<(usize, usize, usize)> {
        let data = self.tag.rel_ptr(self.root + self.f.tree_data)?.ok_or_else(|| err("a body has no tree"))?;
        let field = data + self.f.nodes;
        let (start, n) = self.tag.rel_array(field)?;
        Ok((field, start, n))
    }

    /// The box of each instance, from the leaves of the tree.
    fn leaf_boxes(&self) -> Result<Vec<Option<Aabb>>> {
        let (_, start, n) = self.nodes()?;
        let f = &self.f;
        let mut out = vec![None; self.read.len()];
        for node in 1..n {
            let o = start + node * f.node_size;
            if self.tag.u8(o + f.n_flags)? & 1 == 0 {
                continue;
            }
            for slot in 0..4 {
                let inst = self.tag.u32(o + f.n_data + 4 * slot)?;
                if inst == NO_INSTANCE {
                    continue;
                }
                let axis = |offs: [usize; 3]| -> Result<Vec3> {
                    Ok(Vec3::new(
                        self.tag.f32(o + offs[0] + 4 * slot)?,
                        self.tag.f32(o + offs[1] + 4 * slot)?,
                        self.tag.f32(o + offs[2] + 4 * slot)?,
                    ))
                };
                *out.get_mut(inst as usize).ok_or_else(|| err("the tree names a missing instance"))? =
                    Some(Aabb { min: axis(f.n_lo)?, max: axis(f.n_hi)? });
            }
        }
        Ok(out)
    }

    fn local(&self, shape: usize) -> Result<Local> {
        let t = &self.tag;
        let ty = t.object_at(shape).ok_or_else(|| err("an instance points at no shape"))?.ty;
        if t.is_a(ty, "hknpMeshShape") {
            let (start, n) = t.rel_array(shape + t.field(ty, &["topLevelTree", "nodes"])?.0)?;
            if n < 2 {
                return Err(err("a mesh has no tree"));
            }
            let o = start + self.f.node_size;
            let mut b = Aabb::EMPTY;
            for slot in 0..4 {
                let v = |offs: [usize; 3]| -> Result<Vec3> {
                    Ok(Vec3::new(t.f32(o + offs[0] + 4 * slot)?, t.f32(o + offs[1] + 4 * slot)?, t.f32(o + offs[2] + 4 * slot)?))
                };
                b = b.union(Aabb { min: v(self.f.n_lo)?, max: v(self.f.n_hi)? });
            }
            Ok(Local::Box(b))
        } else if t.is_a(ty, "hknpCompoundShape") {
            Ok(Local::Box(Aabb {
                min: Vec3::from(t.f32s::<3>(shape + self.f.aabb_min)?),
                max: Vec3::from(t.f32s::<3>(shape + self.f.aabb_max)?),
            }))
        } else if t.is_a(ty, "hknpConvexShape") {
            let (start, n) = t.rel_view(shape + t.field(ty, &["hull", "vertices"])?.0)?;
            let points = (0..n).map(|i| t.f32s::<3>(start + 12 * i).map(Vec3::from)).collect::<Result<Vec<_>>>()?;
            if points.is_empty() {
                return Err(err("a convex shape has no vertices"));
            }
            let radius = t.f32(shape + t.member(ty, "convexRadius")?.0)?;
            let kind = if t.is_a(ty, "hknpBoxShape") {
                Convex::Box
            } else if t.is_a(ty, "hknpSphereShape") || t.is_a(ty, "hknpCapsuleShape") {
                Convex::Round
            } else {
                Convex::Other
            };
            Ok(Local::Convex { points, radius, kind })
        } else {
            Err(err(format!("unsupported shape type {}", t.types[ty].name)))
        }
    }

    /// World box of an instance, the way the shipped files have it: the box of the transformed
    /// corners for meshes, nested compounds and unscaled boxes; the vertices plus the radius for
    /// unscaled spheres and capsules; and for scaled boxes the vertices moved so that the
    /// rounded surface scales while the radius stays. Scaled spheres and capsules, and other
    /// convex shapes, get a box that is never smaller than the game's own.
    fn bounds(&self, inst: &Instance) -> Result<Aabb> {
        let place = |p: Vec3| inst.rotation * p + inst.translation;
        let corners = |b: Aabb| {
            Aabb::of((0..8).map(|i| {
                let pick = |bit: usize, axis: usize| if i >> bit & 1 == 1 { b.max[axis] } else { b.min[axis] };
                place(Vec3::new(pick(0, 0), pick(1, 1), pick(2, 2)) * inst.scale)
            }))
        };
        let grown = |b: Aabb, r: f32| Aabb { min: b.min - r, max: b.max + r };
        let (s, unscaled) = (inst.scale, inst.scale == Vec3::ONE);
        Ok(match self.local(inst.shape)? {
            Local::Box(b) => corners(b),
            Local::Convex { points, radius, kind: Convex::Round } => {
                // The game scales these by one factor; the largest covers every reading
                let u = if unscaled { 1.0 } else { s.abs().max_element() };
                grown(Aabb::of(points.into_iter().map(|p| place(p * u))), radius * u)
            }
            Local::Convex { points, radius, kind: Convex::Box } if !unscaled => {
                let all = Aabb::of(points.iter().copied());
                let centre = (all.min + all.max) * 0.5;
                let moved = |p: Vec3| {
                    let d = p - centre;
                    centre * s + d.signum() * ((d.abs() + radius) * s.abs() - radius)
                };
                grown(Aabb::of(points.into_iter().map(|p| place(moved(p)))), radius)
            }
            Local::Convex { points, radius, .. } => corners(grown(Aabb::of(points), radius)),
        })
    }

    /// Shape keys an instance of `shape` may produce: what the compound adds up as an
    /// estimate.
    fn keys(&self, shape: usize) -> Result<u32> {
        let ty = self.tag.object_at(shape).ok_or_else(|| err("an instance points at no shape"))?.ty;
        if self.tag.is_a(ty, "hknpCompoundShape") {
            Ok(self.tag.u32(shape + self.f.keys)?)
        } else {
            Ok(1u32 << self.tag.u8(shape + self.f.key_bits)?.min(31))
        }
    }

    /// Store `insts` (each with its section 4 index and, when known, its box) as the body's
    /// instances. Bytes change only where something differs from what was read.
    fn write(&mut self, insts: &[(Instance, u16, Option<Aabb>)]) -> Result<()> {
        let same_places = insts.len() == self.read.len() && insts.iter().zip(&self.read).all(|(a, b)| a.0 == b.0);
        if same_places && insts.iter().zip(&self.read).all(|(a, b)| a.1 == b.1) {
            return Ok(());
        }
        if insts.is_empty() {
            return Err(err("a body cannot be left without instances"));
        }
        let f = self.f.clone();
        let field = self.root + f.elements;
        let (old_start, old_n) = self.tag.rel_array(field)?;
        let start = if insts.len() <= old_n { old_start } else { self.tag.append(insts.len() * f.inst_size) };
        self.tag.data[old_start..old_start + old_n * f.inst_size].fill(0);
        for (i, (inst, tag, _)) in insts.iter().enumerate() {
            let o = start + i * f.inst_size;
            let mut raw = inst.raw.clone();
            raw[f.i_rotation..f.i_rotation + 16].copy_from_slice(&floats(&inst.rotation.to_array()));
            raw[f.i_translation..f.i_translation + 12].copy_from_slice(&floats(&inst.translation.to_array()));
            raw[f.i_scale..f.i_scale + 12].copy_from_slice(&floats(&inst.scale.to_array()));
            raw[f.i_flags] = inst.flags(raw[f.i_flags]);
            raw[f.i_tag..f.i_tag + 2].copy_from_slice(&tag.to_le_bytes());
            self.tag.put(o, &raw)?;
            self.tag.set_rel_ptr(o + f.i_shape, Some(inst.shape))?;
        }
        self.tag.set_rel_ptr(field, Some(start))?;
        self.tag.put(field + 8, &(insts.len() as u32).to_le_bytes())?;
        self.tag.put(self.root + f.allocated, &(insts.len() as u32).to_le_bytes())?;
        self.move_item(old_start, start, insts.len())?;
        if same_places {
            return Ok(());
        }

        let mut boxes = Vec::with_capacity(insts.len());
        let mut keys = 0u32;
        for (inst, _, known) in insts {
            boxes.push(match known {
                Some(b) => *b,
                None => self.bounds(inst)?,
            });
            keys = keys.saturating_add(self.keys(inst.shape)?);
        }
        let nodes = build_tree(&boxes);
        let (nfield, old_start, old_n) = self.nodes()?;
        let start = if nodes.len() <= old_n { old_start } else { self.tag.append(nodes.len() * f.node_size) };
        self.tag.data[old_start..old_start + old_n * f.node_size].fill(0);
        for (i, node) in nodes.iter().enumerate() {
            let o = start + i * f.node_size;
            for slot in 0..4 {
                for axis in 0..3 {
                    self.tag.put(o + f.n_lo[axis] + 4 * slot, &node.boxes[slot].min[axis].to_le_bytes())?;
                    self.tag.put(o + f.n_hi[axis] + 4 * slot, &node.boxes[slot].max[axis].to_le_bytes())?;
                }
                self.tag.put(o + f.n_data + 4 * slot, &node.data[slot].to_le_bytes())?;
            }
            self.tag.put(o + f.n_flags, &[node.leaf as u8])?;
            self.tag.put(o + f.n_depth, &[node.depth])?;
            self.tag.put(o + f.n_parent, &node.parent.to_le_bytes())?;
            self.tag.put(o + f.n_leaves, &node.leaves.to_le_bytes())?;
        }
        self.tag.set_rel_ptr(nfield, Some(start))?;
        self.tag.put(nfield + 8, &(nodes.len() as u32).to_le_bytes())?;
        self.move_item(old_start, start, nodes.len())?;

        let all = boxes.iter().fold(Aabb::EMPTY, |a, b| a.union(*b));
        self.tag.put(self.root + f.aabb_min, &floats(&all.min.to_array()))?;
        self.tag.put(self.root + f.aabb_max, &floats(&all.max.to_array()))?;
        // The game's radius is at most half the box's diagonal, which always covers it
        self.tag.put(self.root + f.radius, &((all.max - all.min).length() * 0.5).to_le_bytes())?;
        self.tag.put(self.root + f.keys, &keys.to_le_bytes())?;
        let bits = usize::BITS - insts.len().leading_zeros();
        self.tag.put(self.root + f.key_bits, &[bits as u8])
    }

    /// Point the item list's entry for the array at `old` to its new place and length.
    fn move_item(&mut self, old: usize, new: usize, count: usize) -> Result<()> {
        let item: &mut Item = self
            .tag
            .items
            .iter_mut()
            .skip(1)
            .find(|i| i.kind == ITEM_ARRAY && i.offset == old)
            .ok_or_else(|| err("an array is missing from the item list"))?;
        (item.offset, item.count) = (new, count);
        Ok(())
    }
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

const LINKS: usize = 1;
const SLOTS: usize = 2;
const SLOT_LINKS: usize = 3;
const ENTRIES: usize = 4;
const MATERIALS: [usize; 2] = [5, 6];
const FIRST_BODY: usize = 11;
const LINK_SIZE: usize = 32;
const ENTRY_SIZE: usize = 20;
const MATERIAL_SIZE: usize = 16;

/// The objects a collision file knows, without reading its bodies: each `Hash`, and whether
/// the object is baked.
pub fn links_of(d: &[u8]) -> Result<Vec<(u64, bool)>> {
    let file = Phive::parse(d)?;
    let links = file.sections.get(LINKS).filter(|_| file.sections.len() > FIRST_BODY).ok_or_else(|| err("not a static compound"))?;
    Ok(links
        .as_chunks::<LINK_SIZE>()
        .0
        .iter()
        .map(|l| (u64::from_le_bytes(l[8..16].try_into().unwrap()), i32::from_le_bytes(l[..4].try_into().unwrap()) >= 0))
        .collect())
}

/// How many bodies a collision file has. The resource size table's entry depends on it.
pub fn body_count_of(d: &[u8]) -> Result<usize> {
    let file = Phive::parse(d)?;
    Ok(file.sections.iter().skip(FIRST_BODY).filter(|s| !s.is_empty()).count())
}

/// A course's baked collision.
#[derive(Clone, Debug)]
pub struct StaticCompound {
    file: Phive,
    pub links: Vec<Link>,
    bodies: Vec<Body>,
}

impl StaticCompound {
    pub fn parse(d: &[u8]) -> Result<StaticCompound> {
        let file = Phive::parse(d)?;
        if file.sections.len() <= FIRST_BODY {
            return Err(err("not a static compound"));
        }
        if file.to_bytes() != d {
            return Err(err("unusual section layout"));
        }
        let mut bodies = vec![];
        for s in file.sections[FIRST_BODY..].iter().filter(|s| !s.is_empty()) {
            bodies.push(Body::parse(s)?);
        }
        // Section 4 entry -> (body, instance)
        let mut by_tag: BTreeMap<u16, (usize, usize)> = BTreeMap::new();
        for (b, body) in bodies.iter().enumerate() {
            for (i, (_, tag)) in body.read.iter().enumerate() {
                if by_tag.insert(*tag, (b, i)).is_some() {
                    return Err(err("two instances share an entry"));
                }
            }
        }
        let boxes = bodies.iter().map(Body::leaf_boxes).collect::<Result<Vec<_>>>()?;
        let entries = &file.sections[ENTRIES];
        let u32at = |d: &[u8], o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let mut links = vec![];
        for (li, l) in file.sections[LINKS].as_chunks::<LINK_SIZE>().0.iter().enumerate() {
            let (first, last) = (u32at(l, 0) as i32, u32at(l, 4) as i32);
            let mut link = Link {
                hash: u64::from_le_bytes(l[8..16].try_into().unwrap()),
                srt_hash: u32at(l, 16),
                parts: vec![],
                tail: l[20..32].try_into().unwrap(),
            };
            for e in (first >= 0).then_some(first..=last).into_iter().flatten() {
                let rec = entries
                    .get(e as usize * ENTRY_SIZE..(e as usize + 1) * ENTRY_SIZE)
                    .ok_or_else(|| err("a link names a missing entry"))?;
                let &(body, inst) = by_tag.get(&(e as u16)).ok_or_else(|| err("an entry has no instance"))?;
                let (index, owner, slot) = (u32at(rec, 0), u16::from_le_bytes([rec[8], rec[9]]), u16::from_le_bytes([rec[10], rec[11]]));
                if index != e as u32 || owner as usize != li || slot as usize != inst {
                    return Err(err("entries and instances disagree"));
                }
                link.parts.push(Part {
                    body,
                    layer: u32at(rec, 4),
                    materials: (u32at(rec, 12), u32at(rec, 16)),
                    instance: bodies[body].read[inst].0.clone(),
                    bounds: boxes[body][inst],
                });
            }
            links.push(link);
        }
        if links.iter().map(|l| l.parts.len()).sum::<usize>() != by_tag.len() {
            return Err(err("an instance belongs to no object"));
        }
        Ok(StaticCompound { file, links, bodies })
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// The file, with every table and tree brought up to date with [`Self::links`].
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let n = self.links.len();
        if n > u16::MAX as usize {
            return Err(err("too many objects"));
        }
        let mut file = self.file.clone();
        let mut per_body: Vec<Vec<(Instance, u16, Option<Aabb>)>> = vec![vec![]; self.bodies.len()];
        let (mut links, mut entries) = (Vec::with_capacity(n * LINK_SIZE), vec![]);
        let mut next = 0usize;
        for (li, link) in self.links.iter().enumerate() {
            let range = if link.parts.is_empty() { (-1i32, -1i32) } else { (next as i32, (next + link.parts.len() - 1) as i32) };
            links.extend_from_slice(&range.0.to_le_bytes());
            links.extend_from_slice(&range.1.to_le_bytes());
            links.extend_from_slice(&link.hash.to_le_bytes());
            links.extend_from_slice(&link.srt_hash.to_le_bytes());
            links.extend_from_slice(&link.tail);
            for p in &link.parts {
                let list = per_body.get_mut(p.body).ok_or_else(|| err("a shape names a missing body"))?;
                if next > u16::MAX as usize || list.len() > u16::MAX as usize {
                    return Err(err("too many baked shapes"));
                }
                entries.extend_from_slice(&(next as u32).to_le_bytes());
                entries.extend_from_slice(&p.layer.to_le_bytes());
                entries.extend_from_slice(&(li as u16).to_le_bytes());
                entries.extend_from_slice(&(list.len() as u16).to_le_bytes());
                entries.extend_from_slice(&p.materials.0.to_le_bytes());
                entries.extend_from_slice(&p.materials.1.to_le_bytes());
                list.push((p.instance.clone(), next as u16, p.bounds));
                next += 1;
            }
        }
        // Twice as many slots as links; a link sits at its hash modulo the slot count, or in
        // the next free slot after it
        let mut slots: Vec<Option<usize>> = vec![None; n * 2];
        for (li, link) in self.links.iter().enumerate() {
            let mut at = (link.hash % (n as u64 * 2)) as usize;
            while slots[at].is_some() {
                at = (at + 1) % slots.len();
            }
            slots[at] = Some(li);
        }
        let hashes: Vec<u8> = slots.iter().flat_map(|s| s.map_or(0, |li| self.links[li].hash).to_le_bytes()).collect();
        let mut owners: Vec<u8> = slots.iter().flat_map(|s| (s.unwrap_or(0) as u16).to_le_bytes()).collect();
        owners.resize(owners.len().next_multiple_of(16), 0);
        entries.resize(entries.len().next_multiple_of(16), 0);
        file.sections[LINKS] = links;
        file.sections[SLOTS] = hashes;
        file.sections[SLOT_LINKS] = owners;
        file.sections[ENTRIES] = entries;
        let mut body_sections = file.sections.iter_mut().skip(FIRST_BODY).filter(|s| !s.is_empty());
        for (body, insts) in self.bodies.iter().zip(&per_body) {
            let section = body_sections.next().ok_or_else(|| err("a body lost its section"))?;
            let mut body = body.clone();
            body.write(insts)?;
            let mut bytes = body.tag.to_bytes();
            bytes.resize(bytes.len().next_multiple_of(16), 0);
            *section = bytes;
        }
        Ok(file.to_bytes())
    }

    /// A new baked shape like `like`: same body, shape, layer and materials, at another place.
    pub fn part_like(like: &Part, translation: Vec3, rotation: Quat, scale: Vec3) -> Part {
        let mut p = like.clone();
        p.place(translation, rotation, scale);
        p
    }

    /// Bring a shape of another file into this one: the shape itself is copied into the body
    /// of the same layer, and its materials are added to the tables. Fails when this file has
    /// no body for that layer or lacks a Havok type the shape needs.
    pub fn import_part(&mut self, from: &StaticCompound, part: &Part) -> Result<Part> {
        let body = self
            .links
            .iter()
            .flat_map(|l| &l.parts)
            .find(|p| p.layer == part.layer)
            .map(|p| p.body)
            .ok_or_else(|| err("no body for the shape's collision layer"))?;
        let src = from.bodies.get(part.body).ok_or_else(|| err("a shape names a missing body"))?;
        let item = src
            .tag
            .items
            .iter()
            .position(|i| i.kind == crate::hktag::ITEM_OBJECT && i.offset == part.instance.shape)
            .ok_or_else(|| err("an instance points at no shape"))?;
        let (start, count) = (part.materials.0 as usize, part.materials.1 as usize);
        let first = self.file.sections[MATERIALS[0]].len() / MATERIAL_SIZE;
        let mut tables = vec![];
        for s in MATERIALS {
            let rows = from.file.sections[s]
                .get(start * MATERIAL_SIZE..(start + count) * MATERIAL_SIZE)
                .ok_or_else(|| err("a shape names missing materials"))?;
            tables.push(rows.to_vec());
        }
        let target = &mut self.bodies[body];
        let shape = target.tag.import(&src.tag, item)?;
        target.f = Fields::of(&target.tag)?;
        for (s, rows) in MATERIALS.into_iter().zip(tables) {
            self.file.sections[s].extend(rows);
        }
        let mut p = part.clone();
        (p.body, p.instance.shape, p.materials.0, p.bounds) = (body, shape, first as u32, None);
        Ok(p)
    }

    /// Check that every instance's box can be worked out and covers the one stored, and return
    /// how many came out exactly as stored. For tests against the shipped files.
    pub fn check_bounds(&self, slack: f32) -> Result<(usize, usize)> {
        let (mut exact, mut all) = (0, 0);
        for p in self.links.iter().flat_map(|l| &l.parts) {
            let stored = p.bounds.ok_or_else(|| err("an instance is missing from the tree"))?;
            let ours = self.bodies[p.body].bounds(&p.instance)?;
            if !ours.contains(&stored, slack * stored.min.abs().max(stored.max.abs()).max_element().max(1.0)) {
                return Err(err(format!("computed box {ours:?} does not cover the stored {stored:?}")));
            }
            all += 1;
            exact += (ours.min.abs_diff_eq(stored.min, 2e-3) && ours.max.abs_diff_eq(stored.max, 2e-3)) as usize;
        }
        Ok((exact, all))
    }

    /// Check the stored trees against the rules [`build_tree`] follows, and that the stored
    /// summary fields are the ones this module would write. For tests against the shipped
    /// files.
    pub fn check_trees(&self) -> Result<()> {
        for body in &self.bodies {
            let (f, t) = (&body.f, &body.tag);
            let (_, start, n) = body.nodes()?;
            let boxes = body.leaf_boxes()?;
            if boxes.iter().any(Option::is_none) {
                return Err(err("an instance is missing from the tree"));
            }
            let all = boxes.iter().flatten().fold(Aabb::EMPTY, |a, b| a.union(*b));
            let stored =
                Aabb { min: Vec3::from(t.f32s::<3>(body.root + f.aabb_min)?), max: Vec3::from(t.f32s::<3>(body.root + f.aabb_max)?) };
            if all != stored {
                return Err(err("a body's box is not the union of its leaves"));
            }
            if t.f32(body.root + f.radius)? > (all.max - all.min).length() * 0.5 * 1.000_001 {
                return Err(err("a body's radius exceeds half its diagonal"));
            }
            let mut keys = 0u32;
            for (inst, _) in &body.read {
                keys = keys.saturating_add(body.keys(inst.shape)?);
                let flags = inst.raw[f.i_flags];
                if inst.flags(flags) != flags {
                    return Err(err(format!("instance flags {flags:#x} do not follow the rule")));
                }
            }
            if keys != t.u32(body.root + f.keys)? {
                return Err(err("the shape key estimate is not the sum over the instances"));
            }
            if t.u8(body.root + f.key_bits)? as u32 != usize::BITS - body.read.len().leading_zeros() {
                return Err(err("the shape key bits do not follow the instance count"));
            }
            let mut leaves = 0u32;
            for node in 1..n {
                let o = start + node * f.node_size;
                let leaf = t.u8(o + f.n_flags)?;
                let mut count = 0u32;
                let mut depth = 0;
                for slot in 0..4 {
                    let d = t.u32(o + f.n_data + 4 * slot)?;
                    let empty = t.f32(o + f.n_lo[0] + 4 * slot)?.to_bits() == Aabb::EMPTY.min.x.to_bits()
                        && t.f32(o + f.n_hi[0] + 4 * slot)?.to_bits() == Aabb::EMPTY.max.x.to_bits();
                    match (leaf, d) {
                        (1, NO_INSTANCE) | (0, 0) if empty => {}
                        (1, _) if !empty => count += 1,
                        (0, child) if !empty && (child as usize) < n => {
                            let c = start + child as usize * f.node_size;
                            if t.u32(c + f.n_parent)? != node as u32 {
                                return Err(err("a tree node has the wrong parent"));
                            }
                            count = count.saturating_add(t.u32(c + f.n_leaves)?);
                            depth = depth.max(t.u8(c + f.n_depth)?);
                        }
                        _ => return Err(err("a tree slot breaks the rules")),
                    }
                }
                if count != t.u32(o + f.n_leaves)? || depth.checked_add(1) != Some(t.u8(o + f.n_depth)?) {
                    return Err(err("a tree node has the wrong counts"));
                }
                leaves = leaves.saturating_add(if leaf == 1 { count } else { 0 });
            }
            if leaves as usize != body.read.len() || n < 2 || t.u32(start + f.node_size + f.n_parent)? != 0 {
                return Err(err("the tree does not hold every instance once"));
            }
        }
        Ok(())
    }

    /// Whether every object and array of every body is reached from its compound shape: the
    /// check that the pointer walk [`Tagfile::import`] relies on sees every pointer.
    pub fn check_reachable(&self) -> Result<()> {
        for body in &self.bodies {
            let reach = body.tag.reachable(1)?;
            if reach.len() != body.tag.items.len() - 1 {
                return Err(err(format!("{} of {} items reached", reach.len(), body.tag.items.len() - 1)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::hktag::tests::{Types, build};
    use crate::hktag::{ITEM_OBJECT, Item};

    /// The Havok types this module looks fields up in, at the offsets the game's files use.
    fn types() -> Types {
        let mut t = Types::default();
        let float = t.add("float", &[], 0, Some(4), &[]);
        let int = t.add("int", &[], 0, Some(4), &[]);
        let long = t.add("hkInt64", &[], 0, Some(8), &[]);
        let byte = t.add("hkUint8", &[], 0, Some(1), &[]);
        let short = t.add("hkUint16", &[], 0, Some(2), &[]);
        let word = t.add("hkUint32", &[], 0, Some(4), &[]);
        let vec4 = t.add("hkVector4", &[], 0, Some(16), &[]);
        let float3 = t.add("hkFloat3", &[], 0, Some(12), &[]);
        let quat = t.add("hkQuaternion", &[], 0, Some(16), &[]);
        let shape = t.add("hknpShape", &[], 0, Some(64), &[("numShapeKeyBits", 0x1C, byte), ("convexRadius", 0x20, float)]);
        let to_shape = t.add("hkRelPtr", &[("tT", shape)], 0, Some(8), &[("offset", 0, long)]);
        let inst = t.add(
            "hknpShapeInstance",
            &[],
            0,
            Some(64),
            &[
                ("rotation", 0, quat),
                ("translation", 0x10, float3),
                ("flags", 0x1C, byte),
                ("scale", 0x20, float3),
                ("shapeTag", 0x2C, short),
                ("shape", 0x30, to_shape),
            ],
        );
        let insts = t.add("hkRelArray", &[("tT", inst)], 0, Some(16), &[("offset", 0, long), ("size", 8, int)]);
        let list =
            t.add("hkFreeListArray", &[], 0, Some(24), &[("elements", 0, insts), ("firstFree", 0x10, int), ("numAllocated", 0x14, word)]);
        let aabb = t.add("hkAabb", &[], 0, Some(32), &[("min", 0, vec4), ("max", 0x10, vec4)]);
        let words = t.add("T[N]", &[("tT", word), ("vN", 4)], 0, Some(16), &[]);
        let node = t.add(
            "hkcdSimdTreeNamespace::Node",
            &[],
            0,
            Some(128),
            &[
                ("lx", 0, vec4),
                ("hx", 0x10, vec4),
                ("ly", 0x20, vec4),
                ("hy", 0x30, vec4),
                ("lz", 0x40, vec4),
                ("hz", 0x50, vec4),
                ("data", 0x60, words),
                ("flags", 0x74, byte),
                ("maxDepth", 0x75, byte),
                ("parent", 0x78, word),
                ("numLeaves", 0x7C, word),
            ],
        );
        let nodes = t.add("hkRelArray", &[("tT", node)], 0, Some(16), &[("offset", 0, long), ("size", 8, int)]);
        let tree = t.add("hkcdSimdTree", &[], 0, Some(24), &[("nodes", 0, nodes)]);
        let data = t.add("hknpCompoundShapeData", &[], 0, Some(88), &[("simdTree", 0x38, tree)]);
        let to_data = t.add("hkRelPtr", &[("tT", data)], 0, Some(8), &[("offset", 0, long)]);
        t.add(
            "hknpCompoundShape",
            &[],
            shape,
            Some(192),
            &[
                ("instances", 0x48, list),
                ("aabb", 0x70, aabb),
                ("boundingRadius", 0x90, float),
                ("estimatedNumShapeKeys", 0x98, int),
                ("boundingVolumeData", 0xB0, to_data),
            ],
        );
        let points = t.add("hkRelArrayView", &[("tT", float3)], 0, Some(8), &[("offset", 0, int), ("size", 4, int)]);
        let hull = t.add("hknpConvexHull", &[], 0, Some(48), &[("vertices", 0, points)]);
        let convex = t.add("hknpConvexShape", &[], shape, Some(128), &[("hull", 0x40, hull)]);
        t.add("hknpBoxShape", &[], convex, Some(128), &[]);
        t
    }

    /// A body with one box of the given half size (radius included) at `at`.
    fn body(at: Vec3, half: f32) -> Vec<u8> {
        const RADIUS: f32 = 0.05;
        let t = types();
        let section = t.section();
        let probe = Tagfile::parse(&build(&t, &[], &[])).unwrap();
        let ty = |n: &str| probe.type_named(n).unwrap();
        let _ = section;
        let (insts, data, nodes, shape, verts, end) = (0xC0, 0x100, 0x160, 0x260, 0x2E0, 0x340);
        let mut d = vec![0u8; end];
        let mut put = |o: usize, b: &[u8]| d[o..o + b.len()].copy_from_slice(b);
        let bounds = Aabb { min: at - half, max: at + half };
        // the compound shape
        put(0x1C, &[1]);
        put(0x48, &((insts - 0x48) as i64).to_le_bytes());
        put(0x50, &1i32.to_le_bytes());
        put(0x58, &(-1i32).to_le_bytes());
        put(0x5C, &1u32.to_le_bytes());
        put(0x70, &floats(&bounds.min.to_array()));
        put(0x80, &floats(&bounds.max.to_array()));
        put(0x90, &((bounds.max - bounds.min).length() * 0.5).to_le_bytes());
        put(0x98, &1u32.to_le_bytes());
        put(0xB0, &((data - 0xB0) as i64).to_le_bytes());
        // its one instance
        put(insts, &floats(&[0.0, 0.0, 0.0, 1.0]));
        put(insts + 0x10, &floats(&at.to_array()));
        put(insts + 0x1C, &[0x62]);
        put(insts + 0x20, &floats(&[1.0; 3]));
        put(insts + 0x30, &((shape - insts - 0x30) as i64).to_le_bytes());
        // the tree: the unused node and a root with the instance
        put(data + 0x38, &((nodes - data - 0x38) as i64).to_le_bytes());
        put(data + 0x40, &2i32.to_le_bytes());
        for node in 0..2 {
            let o = nodes + 128 * node;
            for slot in 0..4 {
                let b = if node == 1 && slot == 0 { bounds } else { Aabb::EMPTY };
                for axis in 0..3 {
                    put(o + 0x20 * axis + 4 * slot, &b.min[axis].to_le_bytes());
                    put(o + 0x20 * axis + 0x10 + 4 * slot, &b.max[axis].to_le_bytes());
                }
            }
        }
        put(nodes + 128 + 0x60, &[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        put(nodes + 128 + 0x74, &[1, 1]);
        put(nodes + 128 + 0x7C, &1u32.to_le_bytes());
        // the box and its corners
        put(shape + 0x20, &RADIUS.to_le_bytes());
        put(shape + 0x40, &((verts - shape - 0x40) as i32).to_le_bytes());
        put(shape + 0x44, &8i32.to_le_bytes());
        for i in 0..8 {
            let c = half - RADIUS;
            let v = [if i & 1 == 0 { c } else { -c }, if i & 2 == 0 { c } else { -c }, if i & 4 == 0 { c } else { -c }];
            put(verts + 12 * i, &floats(&v));
        }
        let items = [
            Item { ty: ty("hknpCompoundShape"), kind: ITEM_OBJECT, offset: 0, count: 1 },
            Item { ty: ty("hknpShapeInstance"), kind: ITEM_ARRAY, offset: insts, count: 1 },
            Item { ty: ty("hknpCompoundShapeData"), kind: ITEM_OBJECT, offset: data, count: 1 },
            Item { ty: ty("hkcdSimdTreeNamespace::Node"), kind: ITEM_ARRAY, offset: nodes, count: 2 },
            Item { ty: ty("hknpBoxShape"), kind: ITEM_OBJECT, offset: shape, count: 1 },
            Item { ty: ty("hkFloat3"), kind: ITEM_ARRAY, offset: verts, count: 8 },
        ];
        let mut bytes = build(&t, &d, &items);
        bytes.resize(bytes.len().next_multiple_of(16), 0);
        bytes
    }

    pub const LAYER: u32 = 0x400;

    /// A collision file that bakes one box per object, in one body. `half` is the half size of
    /// the box (0.5 where it does not matter).
    pub fn sample(objects: &[(u64, [f32; 3])], half: f32) -> Vec<u8> {
        let (hash, at) = objects[0];
        let mut link = vec![0u8; 8];
        link.extend_from_slice(&hash.to_le_bytes());
        link.resize(LINK_SIZE, 0);
        let mut slots = vec![0u8; 16];
        slots[(hash % 2) as usize * 8..][..8].copy_from_slice(&hash.to_le_bytes());
        let mut entry = 0u32.to_le_bytes().to_vec();
        entry.extend_from_slice(&LAYER.to_le_bytes());
        entry.extend_from_slice(&[0; 8]);
        entry.extend_from_slice(&1u32.to_le_bytes());
        entry.resize(32, 0);
        let mut sections = vec![vec![0xAB; 16], link, slots, vec![0; 16], entry, vec![1; 16], vec![2; 16], vec![0; 16]];
        sections.extend([vec![], vec![], vec![], body(Vec3::from(at), half), vec![], vec![]]);
        let file = Phive { head: [2, 2, 0xFF, 0xFE, 2, sections.len() as u8], placed: vec![true; sections.len()], sections };
        let mut sc = StaticCompound::parse(&file.to_bytes()).unwrap();
        let like = sc.links[0].parts[0].clone();
        for &(hash, at) in &objects[1..] {
            let mut l = Link::new(hash, 0);
            l.parts.push(StaticCompound::part_like(&like, Vec3::from(at), Quat::IDENTITY, Vec3::ONE));
            sc.links.push(l);
        }
        sc.to_bytes().unwrap()
    }

    fn grid(n: usize) -> Vec<(u64, [f32; 3])> {
        (0..n).map(|i| (100 + i as u64, [(i % 7) as f32 * 10.0, 0.0, (i / 7) as f32 * 10.0])).collect()
    }

    fn checked(d: &[u8]) -> StaticCompound {
        let sc = StaticCompound::parse(d).unwrap();
        sc.check_trees().unwrap();
        sc.check_reachable().unwrap();
        let (exact, all) = sc.check_bounds(1e-5).unwrap();
        assert_eq!(exact, all);
        assert!(sc.to_bytes().unwrap() == d);
        sc
    }

    #[test]
    fn sample_files_follow_the_rules() {
        for n in [1, 2, 5, 40] {
            let d = sample(&grid(n), 0.5);
            let sc = checked(&d);
            assert_eq!((sc.links.len(), sc.body_count()), (n, 1));
            assert_eq!(links_of(&d).unwrap(), grid(n).iter().map(|o| (o.0, true)).collect::<Vec<_>>());
            assert_eq!(body_count_of(&d).unwrap(), 1);
        }
    }

    #[test]
    fn edits_rebuild_tables_and_tree() {
        let d = sample(&grid(9), 0.5);
        let mut sc = checked(&d);
        // move and turn one, scale another, remove two, add one without shapes and twenty with
        let quarter = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        sc.links[3].parts[0].place(Vec3::new(500.0, 20.0, -300.0), quarter, Vec3::ONE);
        sc.links[4].parts[0].place(Vec3::new(5.0, 0.0, 5.0), Quat::IDENTITY, Vec3::new(2.0, 3.0, 4.0));
        sc.links.retain(|l| l.hash != 100 && l.hash != 105);
        sc.links.insert(2, Link::new(7, 9));
        let like = sc.links[0].parts[0].clone();
        for i in 0..20 {
            let mut l = Link::new(1000 + i, 0);
            l.parts.push(StaticCompound::part_like(&like, Vec3::new(i as f32 * 3.0, 50.0, 0.0), Quat::IDENTITY, Vec3::ONE));
            sc.links.push(l);
        }
        let out = sc.to_bytes().unwrap();
        let back = checked(&out);
        assert_eq!(back.links.len(), 9 - 2 + 1 + 20);
        assert_eq!(back.links.iter().map(|l| l.parts.len()).sum::<usize>(), 27);
        let find = |h: u64| back.links.iter().find(|l| l.hash == h).unwrap();
        assert!(find(7).parts.is_empty() && find(7).srt_hash == 9);
        let moved = &find(103).parts[0];
        assert_eq!(moved.instance.translation, Vec3::new(500.0, 20.0, -300.0));
        assert!(moved.instance.rotation.angle_between(quarter) < 1e-6);
        // a box turned 45 degrees is wider across: the tree has the new box
        let b = moved.bounds.unwrap();
        assert!((b.max.x - b.min.x - 2f32.sqrt()).abs() < 1e-4 && (b.max.y - b.min.y - 1.0).abs() < 1e-5, "{b:?}");
        let scaled = find(104).parts[0].bounds.unwrap();
        assert!((scaled.max - scaled.min).abs_diff_eq(Vec3::new(2.0, 3.0, 4.0), 1e-4), "{scaled:?}");
        assert!(back.links.iter().flat_map(|l| &l.parts).all(|p| p.instance.shape == like.instance.shape));
        // nothing changed: nothing is rewritten
        let mut same = StaticCompound::parse(&d).unwrap();
        let (t, r, s) = (same.links[1].parts[0].instance.translation, same.links[1].parts[0].instance.rotation, Vec3::ONE);
        same.links[1].parts[0].place(t, r, s);
        assert!(same.to_bytes().unwrap() == d);
        // a body cannot lose every shape
        same.links.clear();
        assert!(same.to_bytes().is_err());
    }

    #[test]
    fn shapes_come_over_from_another_file() {
        let mut sc = StaticCompound::parse(&sample(&grid(3), 0.5)).unwrap();
        let from = StaticCompound::parse(&sample(&[(1, [0.0; 3])], 2.0)).unwrap();
        let mut part = sc.import_part(&from, &from.links[0].parts[0]).unwrap();
        assert_eq!(part.materials, (1, 1));
        part.place(Vec3::new(0.0, 100.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let mut l = Link::new(55, 0);
        l.parts.push(part);
        sc.links.push(l);
        let back = checked(&sc.to_bytes().unwrap());
        let b = back.links.iter().find(|l| l.hash == 55).unwrap().parts[0].bounds.unwrap();
        assert!((b.max - b.min).abs_diff_eq(Vec3::splat(4.0), 1e-4), "{b:?}");
        let small = back.links[0].parts[0].bounds.unwrap();
        assert!((small.max - small.min).abs_diff_eq(Vec3::splat(1.0), 1e-4));
        // a shape of a layer this file has no body for is refused
        let mut other = from.links[0].parts[0].clone();
        other.layer = 0x701;
        assert!(sc.import_part(&from, &other).is_err());
    }

    #[test]
    fn corrupt_files_fail_cleanly() {
        let d = sample(&grid(6), 0.5);
        for n in 0..d.len() {
            assert!(StaticCompound::parse(&d[..n]).is_err(), "truncated to {n}");
        }
        for i in 0..d.len() {
            let mut b = d.clone();
            b[i] ^= 0xFF;
            let _ = links_of(&b);
            let Ok(mut sc) = StaticCompound::parse(&b) else { continue };
            let _ = sc.check_trees();
            let _ = sc.check_bounds(1e-5);
            let _ = sc.check_reachable();
            let _ = sc.to_bytes();
            if let Some(p) = sc.links.iter_mut().flat_map(|l| &mut l.parts).next() {
                p.place(Vec3::ONE, Quat::IDENTITY, Vec3::splat(2.0));
            }
            let _ = sc.to_bytes();
        }
    }

    #[test]
    fn tree_holds_every_instance() {
        for n in [1usize, 3, 4, 5, 16, 17, 43, 200] {
            let boxes: Vec<Aabb> = (0..n)
                .map(|i| {
                    let p = Vec3::new((i * 37 % 101) as f32, (i % 7) as f32, (i * 11 % 53) as f32);
                    Aabb { min: p, max: p + 1.0 }
                })
                .collect();
            let nodes = build_tree(&boxes);
            assert_eq!(nodes[1].leaves as usize, n);
            assert_eq!(nodes[1].parent, 0);
            let mut seen = vec![false; n];
            for (i, node) in nodes.iter().enumerate().skip(1) {
                for slot in 0..4 {
                    match (node.leaf, node.data[slot]) {
                        (true, NO_INSTANCE) | (false, 0) => assert!(node.boxes[slot].is_empty()),
                        (true, inst) => {
                            assert!(!std::mem::replace(&mut seen[inst as usize], true));
                            assert_eq!(node.boxes[slot], boxes[inst as usize]);
                        }
                        (false, child) => {
                            let c = &nodes[child as usize];
                            assert!(child as usize > i && c.parent as usize == i);
                            let all = c.boxes.iter().fold(Aabb::EMPTY, |a, b| a.union(*b));
                            assert_eq!(node.boxes[slot], all);
                            assert!(node.depth > c.depth);
                        }
                    }
                }
            }
            assert!(seen.iter().all(|&s| s));
            // as shallow as four children per node allow
            let mut depth = 1;
            while 4usize.pow(depth) < n {
                depth += 1;
            }
            assert_eq!(nodes[1].depth as u32, depth, "{n} instances");
            // and with as few nodes: every leaf node full but the last
            assert_eq!(nodes.iter().filter(|n| n.leaf).count(), n.div_ceil(4), "{n} instances");
        }
    }

    #[test]
    fn container_round_trip() {
        let mut d = b"Phive\0\x02\x02\xFF\xFE\x02\x03".to_vec();
        for v in [0x30u32, 0x40, 0, 16, 32, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.resize(0x30, 0);
        d.extend([7u8; 16]);
        d.extend([9u8; 32]);
        let p = Phive::parse(&d).unwrap();
        assert_eq!((p.sections[0].len(), p.sections[1].len(), p.sections[2].len()), (16, 32, 0));
        assert_eq!(p.to_bytes(), d);
        for n in 0..d.len() {
            assert!(Phive::parse(&d[..n]).is_err());
        }
    }
}

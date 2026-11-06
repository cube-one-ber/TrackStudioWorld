//! Minimal binary FBX reader for the Switch Toolbox exports: static meshes with
//! normals, one UV set and a diffuse texture per material. Skinning is ignored.

use crate::error::Result;
use flate2::read::ZlibDecoder;
use glam::{Mat3, Mat4, Quat, Vec3};
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone)]
pub enum Prop {
    I16(i16),
    Bool(bool),
    I32(i32),
    F32(f32),
    F64(f64),
    I64(i64),
    Str(String),
    Raw(Vec<u8>),
    F32s(Vec<f32>),
    F64s(Vec<f64>),
    I32s(Vec<i32>),
    I64s(Vec<i64>),
    Bools(Vec<bool>),
}

impl Prop {
    fn as_i64(&self) -> Option<i64> {
        match self {
            Prop::I64(v) => Some(*v),
            Prop::I32(v) => Some(*v as i64),
            Prop::I16(v) => Some(*v as i64),
            _ => None,
        }
    }
    fn as_f64(&self) -> Option<f64> {
        match self {
            Prop::F64(v) => Some(*v),
            Prop::F32(v) => Some(*v as f64),
            Prop::I32(v) => Some(*v as f64),
            Prop::I64(v) => Some(*v as f64),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }
    fn f64s(&self) -> Vec<f64> {
        match self {
            Prop::F64s(v) => v.clone(),
            Prop::F32s(v) => v.iter().map(|&x| x as f64).collect(),
            _ => vec![],
        }
    }
    fn i32s(&self) -> Vec<i32> {
        match self {
            Prop::I32s(v) => v.clone(),
            Prop::I64s(v) => v.iter().map(|&x| x as i32).collect(),
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn child_str(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.props.first()).and_then(|p| p.as_str())
    }
}

struct Parser<'a> {
    d: &'a [u8],
    pos: usize,
    wide: bool,
}

impl<'a> Parser<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.d.get(self.pos..self.pos + n).ok_or("FBX: unexpected end of file")?;
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64v(&mut self) -> Result<u64> {
        if self.wide { Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap())) } else { Ok(self.u32()? as u64) }
    }

    fn array<T>(&mut self, size: usize, conv: impl Fn(&[u8]) -> T) -> Result<Vec<T>> {
        let len = self.u32()? as usize;
        let enc = self.u32()?;
        let clen = self.u32()? as usize;
        let raw = self.take(clen)?;
        let data = if enc == 1 {
            let mut out = Vec::with_capacity(len * size);
            ZlibDecoder::new(raw).read_to_end(&mut out).map_err(|e| format!("FBX zlib: {e}"))?;
            out
        } else {
            raw.to_vec()
        };
        if data.len() < len * size {
            return Err("FBX: short array".into());
        }
        Ok(data.chunks_exact(size).take(len).map(conv).collect())
    }

    fn prop(&mut self) -> Result<Prop> {
        let t = self.u8()?;
        Ok(match t {
            b'Y' => Prop::I16(i16::from_le_bytes(self.take(2)?.try_into().unwrap())),
            b'C' => Prop::Bool(self.u8()? != 0),
            b'I' => Prop::I32(i32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            b'F' => Prop::F32(f32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            b'D' => Prop::F64(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            b'L' => Prop::I64(i64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            b'f' => Prop::F32s(self.array(4, |b| f32::from_le_bytes(b.try_into().unwrap()))?),
            b'd' => Prop::F64s(self.array(8, |b| f64::from_le_bytes(b.try_into().unwrap()))?),
            b'i' => Prop::I32s(self.array(4, |b| i32::from_le_bytes(b.try_into().unwrap()))?),
            b'l' => Prop::I64s(self.array(8, |b| i64::from_le_bytes(b.try_into().unwrap()))?),
            b'b' | b'c' => Prop::Bools(self.array(1, |b| b[0] != 0)?),
            b'S' => {
                let n = self.u32()? as usize;
                Prop::Str(String::from_utf8_lossy(self.take(n)?).into_owned())
            }
            b'R' => {
                let n = self.u32()? as usize;
                Prop::Raw(self.take(n)?.to_vec())
            }
            _ => return Err(format!("FBX: unknown property type {:?}", t as char).into()),
        })
    }

    fn node(&mut self) -> Result<Option<Node>> {
        let end = self.u64v()? as usize;
        let nprops = self.u64v()? as usize;
        let _plen = self.u64v()?;
        let nlen = self.u8()? as usize;
        if end == 0 {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(self.take(nlen)?).into_owned();
        let mut props = Vec::with_capacity(nprops);
        for _ in 0..nprops {
            props.push(self.prop()?);
        }
        let mut children = vec![];
        while self.pos < end {
            match self.node()? {
                Some(c) => children.push(c),
                None => break,
            }
        }
        self.pos = end;
        Ok(Some(Node { name, props, children }))
    }
}

pub fn parse(d: &[u8]) -> Result<Node> {
    if d.len() < 27 || !d.starts_with(b"Kaydara FBX Binary") {
        return Err("not a binary FBX file".into());
    }
    let version = u32::from_le_bytes(d[23..27].try_into().unwrap());
    let mut p = Parser { d, pos: 27, wide: version >= 7500 };
    let mut root = Node { name: "root".into(), ..Default::default() };
    while p.pos + 13 < d.len() {
        match p.node()? {
            Some(n) => root.children.push(n),
            None => break,
        }
    }
    Ok(root)
}

// ---------------------------------------------------------------- meshes

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub material: String,
    /// texture file name as referenced by the FBX (basename only)
    pub texture: Option<String>,
    pub cutout: bool,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub parts: Vec<Part>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.indices.len() / 3).sum()
    }
}

fn props70(n: &Node) -> HashMap<String, Vec<f64>> {
    let mut out = HashMap::new();
    if let Some(p70) = n.child("Properties70") {
        for p in &p70.children {
            if let Some(name) = p.props.first().and_then(|x| x.as_str()) {
                out.insert(name.to_string(), p.props.iter().skip(4).filter_map(|x| x.as_f64()).collect());
            }
        }
    }
    out
}

fn vec3_of(m: &HashMap<String, Vec<f64>>, k: &str, d: Vec3) -> Vec3 {
    match m.get(k) {
        Some(v) if v.len() >= 3 => Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32),
        _ => d,
    }
}

fn euler_deg(v: Vec3, order: i64) -> Quat {
    let (x, y, z) = (v.x.to_radians(), v.y.to_radians(), v.z.to_radians());
    let (qx, qy, qz) = (Quat::from_rotation_x(x), Quat::from_rotation_y(y), Quat::from_rotation_z(z));
    // FBX order names list the first axis applied first
    match order {
        1 => qy * qz * qx, // XZY
        2 => qz * qx * qy, // YZX
        3 => qx * qz * qy, // YXZ
        4 => qy * qx * qz, // ZXY
        5 => qx * qy * qz, // ZYX
        _ => qz * qy * qx, // XYZ
    }
}

fn local_matrix(n: &Node) -> (Mat4, Mat4) {
    let p = props70(n);
    let order = p.get("RotationOrder").and_then(|v| v.first()).copied().unwrap_or(0.0) as i64;
    let t = vec3_of(&p, "Lcl Translation", Vec3::ZERO);
    let r = euler_deg(vec3_of(&p, "Lcl Rotation", Vec3::ZERO), order);
    let s = vec3_of(&p, "Lcl Scaling", Vec3::ONE);
    let pre = euler_deg(vec3_of(&p, "PreRotation", Vec3::ZERO), 0);
    let post = euler_deg(vec3_of(&p, "PostRotation", Vec3::ZERO), 0);
    let local = Mat4::from_translation(t) * Mat4::from_quat(pre * r * post.inverse()) * Mat4::from_scale(s);
    let geo = Mat4::from_scale_rotation_translation(
        vec3_of(&p, "GeometricScaling", Vec3::ONE),
        euler_deg(vec3_of(&p, "GeometricRotation", Vec3::ZERO), 0),
        vec3_of(&p, "GeometricTranslation", Vec3::ZERO),
    );
    (local, geo)
}

struct Layer {
    mapping: String,
    direct: Vec<f64>,
    index: Option<Vec<i32>>,
    width: usize,
}

impl Layer {
    fn from(n: Option<&Node>, data: &str, idx: &[&str], width: usize) -> Option<Layer> {
        let n = n?;
        let direct = n.child(data)?.props.first()?.f64s();
        if direct.is_empty() {
            return None;
        }
        let refkind = n.child_str("ReferenceInformationType").unwrap_or("Direct");
        let index = if refkind == "IndexToDirect" || refkind == "Index" {
            idx.iter().find_map(|k| n.child(k)).and_then(|c| c.props.first()).map(|p| p.i32s())
        } else {
            None
        };
        Some(Layer { mapping: n.child_str("MappingInformationType").unwrap_or("ByPolygonVertex").to_string(), direct, index, width })
    }

    fn get(&self, poly_vertex: usize, control_point: usize, polygon: usize) -> Option<&[f64]> {
        let i = match self.mapping.as_str() {
            "ByVertice" | "ByVertex" | "ByControlPoint" => control_point,
            "ByPolygon" => polygon,
            "AllSame" => 0,
            _ => poly_vertex,
        };
        let i = match &self.index {
            Some(ix) => *ix.get(i)? as usize,
            None => i,
        };
        self.direct.get(i * self.width..i * self.width + self.width)
    }
}

pub fn is_cutout(texture: &str, material: &str) -> bool {
    let s = format!("{texture} {material}").to_lowercase();
    ["cutout", "alpha", "leaf", "grass", "foliage"].iter().any(|k| s.contains(k))
}

/// Extract world-space meshes grouped by material.
pub fn load_model(d: &[u8]) -> Result<Model> {
    let root = parse(d)?;
    let objects = root.child("Objects").ok_or("FBX: no Objects")?;
    let mut by_id: HashMap<i64, &Node> = HashMap::new();
    for o in &objects.children {
        if let Some(id) = o.props.first().and_then(|p| p.as_i64()) {
            by_id.insert(id, o);
        }
    }
    // connections: child -> parent (OO), and texture -> material (OP)
    let mut parent: HashMap<i64, i64> = HashMap::new();
    let mut geom_of_model: HashMap<i64, i64> = HashMap::new();
    let mut mats_of_model: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut tex_of_mat: HashMap<i64, (i64, bool)> = HashMap::new();
    if let Some(conns) = root.child("Connections") {
        for c in &conns.children {
            let kind = c.props.first().and_then(|p| p.as_str()).unwrap_or("");
            let (Some(src), Some(dst)) = (c.props.get(1).and_then(|p| p.as_i64()), c.props.get(2).and_then(|p| p.as_i64())) else {
                continue;
            };
            let (Some(s), Some(t)) = (by_id.get(&src), by_id.get(&dst)) else {
                if kind == "OO" && dst == 0 {
                    parent.insert(src, 0);
                }
                continue;
            };
            match (s.name.as_str(), t.name.as_str()) {
                ("Model", "Model") => {
                    parent.insert(src, dst);
                }
                ("Geometry", "Model") => {
                    geom_of_model.insert(dst, src);
                }
                ("Material", "Model") => mats_of_model.entry(dst).or_default().push(src),
                ("Texture", "Material") => {
                    let diffuse = c
                        .props
                        .get(3)
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.contains("Diffuse") || p.contains("BaseColor") || p.contains("baseColor"));
                    let e = tex_of_mat.entry(dst).or_insert((src, diffuse));
                    if diffuse && !e.1 {
                        *e = (src, true);
                    }
                }
                _ => {}
            }
        }
    }
    let mut global_cache: HashMap<i64, Mat4> = HashMap::new();
    fn global(id: i64, by_id: &HashMap<i64, &Node>, parent: &HashMap<i64, i64>, cache: &mut HashMap<i64, Mat4>, depth: u32) -> Mat4 {
        if let Some(m) = cache.get(&id) {
            return *m;
        }
        let local = by_id.get(&id).map(|n| local_matrix(n).0).unwrap_or(Mat4::IDENTITY);
        let p = parent.get(&id).copied().unwrap_or(0);
        let m = if p != 0 && depth < 64 { global(p, by_id, parent, cache, depth + 1) * local } else { local };
        cache.insert(id, m);
        m
    }
    let name_of = |n: &Node| n.props.get(1).and_then(|p| p.as_str()).unwrap_or("").split('\0').next().unwrap_or("").to_string();
    let tex_name = |mat: i64| -> Option<String> {
        let (tid, _) = tex_of_mat.get(&mat)?;
        let t = by_id.get(tid)?;
        let f = t.child_str("RelativeFilename").filter(|s| !s.is_empty()).or_else(|| t.child_str("FileName"))?;
        Some(f.rsplit(['/', '\\']).next().unwrap_or(f).to_string())
    };

    let mut parts: HashMap<String, Part> = HashMap::new();
    let mut dedup: HashMap<String, HashMap<[u32; 8], u32>> = HashMap::new();
    let mut models: Vec<_> = geom_of_model.iter().map(|(m, g)| (*m, *g)).collect();
    models.sort();
    for (model_id, geom_id) in models {
        let Some(g) = by_id.get(&geom_id) else { continue };
        let Some(verts) = g.child("Vertices").and_then(|v| v.props.first()).map(|p| p.f64s()) else { continue };
        let Some(pvi) = g.child("PolygonVertexIndex").and_then(|v| v.props.first()).map(|p| p.i32s()) else { continue };
        let world = global(model_id, &by_id, &parent, &mut global_cache, 0)
            * by_id.get(&model_id).map(|n| local_matrix(n).1).unwrap_or(Mat4::IDENTITY);
        let nmat = Mat3::from_mat4(world).inverse().transpose();
        let normals = Layer::from(g.child("LayerElementNormal"), "Normals", &["NormalsIndex", "NormalIndex"], 3);
        let uv_node =
            g.children.iter().filter(|c| c.name == "LayerElementUV").min_by_key(|c| c.props.first().and_then(|p| p.as_i64()).unwrap_or(0));
        let uvs = Layer::from(uv_node, "UV", &["UVIndex"], 2);
        let mat_layer = g.child("LayerElementMaterial");
        let mat_map = mat_layer.and_then(|m| m.child_str("MappingInformationType")).unwrap_or("AllSame").to_string();
        let mat_idx = mat_layer.and_then(|m| m.child("Materials")).and_then(|m| m.props.first()).map(|p| p.i32s()).unwrap_or_default();
        let mats = mats_of_model.get(&model_id).cloned().unwrap_or_default();

        let mut polygon = 0usize;
        let mut poly: Vec<(usize, usize)> = vec![]; // (polygon-vertex index, control point)
        for (pv, &raw) in pvi.iter().enumerate() {
            let (cp, last) = if raw < 0 { ((-raw - 1) as usize, true) } else { (raw as usize, false) };
            poly.push((pv, cp));
            if !last {
                continue;
            }
            let slot =
                if mat_map == "AllSame" { mat_idx.first().copied().unwrap_or(0) } else { mat_idx.get(polygon).copied().unwrap_or(0) };
            let mat_id = mats.get(slot.max(0) as usize).copied();
            let mat_name = mat_id.and_then(|m| by_id.get(&m)).map(|n| name_of(n)).unwrap_or_default();
            let texture = mat_id.and_then(tex_name);
            let key = format!("{}|{}", texture.clone().unwrap_or_default(), mat_name);
            let part = parts.entry(key.clone()).or_insert_with(|| Part {
                cutout: is_cutout(texture.as_deref().unwrap_or(""), &mat_name),
                material: mat_name.clone(),
                texture: texture.clone(),
                ..Default::default()
            });
            let dd = dedup.entry(key).or_default();
            let mut ids = Vec::with_capacity(poly.len());
            for &(pv, cp) in &poly {
                let Some(p) = verts.get(cp * 3..cp * 3 + 3) else { continue };
                let pos = world.transform_point3(Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32));
                let n = normals
                    .as_ref()
                    .and_then(|l| l.get(pv, cp, polygon))
                    .map(|n| (nmat * Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32)).normalize_or_zero())
                    .unwrap_or(Vec3::Y);
                let uv = uvs.as_ref().and_then(|l| l.get(pv, cp, polygon)).map(|u| [u[0] as f32, 1.0 - u[1] as f32]).unwrap_or([0.0, 0.0]);
                let k = [
                    pos.x.to_bits(),
                    pos.y.to_bits(),
                    pos.z.to_bits(),
                    n.x.to_bits(),
                    n.y.to_bits(),
                    n.z.to_bits(),
                    uv[0].to_bits(),
                    uv[1].to_bits(),
                ];
                let id = *dd.entry(k).or_insert_with(|| {
                    part.positions.push(pos.into());
                    part.normals.push(n.into());
                    part.uvs.push(uv);
                    (part.positions.len() - 1) as u32
                });
                ids.push(id);
            }
            for i in 1..ids.len().saturating_sub(1) {
                part.indices.extend_from_slice(&[ids[0], ids[i], ids[i + 1]]);
            }
            poly.clear();
            polygon += 1;
        }
    }
    let mut parts: Vec<Part> = parts.into_values().filter(|p| !p.indices.is_empty()).collect();
    parts.sort_by(|a, b| a.material.cmp(&b.material));
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for p in &parts {
        for v in &p.positions {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
    }
    if parts.is_empty() {
        min = [0.0; 3];
        max = [0.0; 3];
    }
    Ok(Model { parts, min, max })
}

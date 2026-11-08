//! BFRES models (`Model/*.bfres.zs`, format version 10): the first model's most detailed meshes,
//! placed by their bones, one part per material with the material's albedo texture.
//!
//! Layout notes (all offsets are absolute and little endian):
//!
//! * File header: model array at 0x28, model count (u16) at 0xDC, buffer info at 0xB0 (the
//!   vertex and index data start at its offset 8).
//! * FMDL (0x78 bytes): skeleton at +0x18, vertex buffers at +0x20, shapes at +0x28,
//!   materials at +0x38; vertex buffer, shape and material counts (u16) from +0x68.
//! * FSKL: bone array at +0x10, matrix-to-bone list at +0x18, bone count at +0x38. Bones are
//!   0x58 bytes: parent (i16) at +0x22, flags at +0x2C (bit 12: Euler XYZ, else quaternion),
//!   scale at +0x30, rotation at +0x3C, translation at +0x4C.
//! * FVTX (0x58 bytes): attributes (0x10 each: name, nn::gfx format, offset, buffer) at +0x08,
//!   buffer sizes and strides (0x10 each) at +0x30 and +0x38, data offset at +0x48, attribute
//!   and buffer counts (u8) at +0x4C, vertex count at +0x50.
//! * FSHP (0x60 bytes): vertex buffer at +0x10, meshes (0x38 bytes, one per level of detail) at
//!   +0x18; material and bone index (u16) at +0x52 and +0x54, skin count (u8) at +0x5A, mesh
//!   count at +0x5B. A mesh has its index data offset, primitive type, index format, index
//!   count and base vertex as u32 from +0x20.
//! * FMAT (0xB0 bytes): shader info at +0x10 (its shader assign holds the render info and
//!   sampler names), texture names at +0x20, sampler dictionary at +0x38, render info values,
//!   counts and offsets at +0x40, +0x48 and +0x50, texture count (u8) at +0xA3.
//!
//! Shapes with no skinning follow their bone; rigid skinning (one weight) follows the bone each
//! vertex names through the matrix-to-bone list; smooth-skinned vertices are already in model
//! space, so skinned models come out in their bind pose.

use crate::error::{Context, Result};
use crate::model::{Model, Part};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3, Vec4};
use std::collections::HashMap;
use std::path::Path;

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn bytes<const N: usize>(&self, o: usize) -> Result<[u8; N]> {
        self.0.get(o..o + N).and_then(|s| s.try_into().ok()).ok_or_else(|| format!("BFRES: read past the end at {o:#x}").into())
    }
    fn u8(&self, o: usize) -> Result<u8> {
        Ok(self.bytes::<1>(o)?[0])
    }
    fn u16(&self, o: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(o)?))
    }
    fn i16(&self, o: usize) -> Result<i16> {
        Ok(i16::from_le_bytes(self.bytes(o)?))
    }
    fn u32(&self, o: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(o)?))
    }
    fn f32(&self, o: usize) -> Result<f32> {
        Ok(f32::from_le_bytes(self.bytes(o)?))
    }
    fn off(&self, o: usize) -> Result<usize> {
        Ok(u64::from_le_bytes(self.bytes(o)?) as usize)
    }
    fn vec3(&self, o: usize) -> Result<Vec3> {
        Ok(Vec3::new(self.f32(o)?, self.f32(o + 4)?, self.f32(o + 8)?))
    }
    /// A string by its offset: u16 length, then the bytes.
    fn str(&self, o: usize) -> Result<String> {
        if o == 0 {
            return Ok(String::new());
        }
        let n = self.u16(o)? as usize;
        Ok(String::from_utf8_lossy(self.0.get(o + 2..o + 2 + n).ok_or("BFRES: bad string")?).into_owned())
    }
    fn str_at(&self, o: usize) -> Result<String> {
        self.str(self.off(o)?)
    }
    /// Names of a dictionary, in order (the root node is skipped).
    fn dict(&self, o: usize) -> Result<Vec<String>> {
        if o == 0 {
            return Ok(vec![]);
        }
        let n = self.u32(o + 4)? as usize;
        (1..=n.min(1 << 16)).map(|i| self.str_at(o + 8 + 16 * i + 8)).collect()
    }
}

/// One vertex attribute value as up to four floats, from its nn::gfx attribute format.
fn attribute(r: &R, o: usize, format: u32) -> Result<Vec4> {
    let ty = format & 0xff;
    let (count, bits) = match format >> 8 {
        0x02 => (1, 8),
        0x09 => (2, 8),
        0x0b => (4, 8),
        0x0a => (1, 16),
        0x12 => (2, 16),
        0x15 => (4, 16),
        0x14 => (1, 32),
        0x17 => (2, 32),
        0x18 => (3, 32),
        0x19 => (4, 32),
        0x0e => {
            let v = r.u32(o)?;
            let c = |i: u32| {
                let x = (v >> (10 * i)) & 0x3ff;
                if ty == 2 { ((((x << 22) as i32) >> 22) as f32 / 511.0).max(-1.0) } else { x as f32 / 1023.0 }
            };
            return Ok(Vec4::new(c(0), c(1), c(2), (v >> 30) as f32 / 3.0));
        }
        _ => return Err(format!("BFRES: unsupported attribute format {format:#06x}").into()),
    };
    let mut out = [0.0f32, 0.0, 0.0, 1.0];
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let p = o + i * bits / 8;
        *slot = match (bits, ty) {
            (8, 1) => r.u8(p)? as f32 / 255.0,
            (8, 2) => (r.u8(p)? as i8 as f32 / 127.0).max(-1.0),
            (8, 4 | 9) => r.u8(p)? as i8 as f32,
            (8, _) => r.u8(p)? as f32,
            (16, 1) => r.u16(p)? as f32 / 65535.0,
            (16, 2) => (r.i16(p)? as f32 / 32767.0).max(-1.0),
            (16, 4 | 9) => r.i16(p)? as f32,
            (16, 5) => half(r.u16(p)?),
            (16, _) => r.u16(p)? as f32,
            (32, 5) => r.f32(p)?,
            (32, 4 | 9) => r.u32(p)? as i32 as f32,
            _ => r.u32(p)? as f32,
        };
    }
    Ok(Vec4::from(out))
}

/// An IEEE half-precision float.
pub fn half(h: u16) -> f32 {
    let (sign, exp, frac) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    let bits = match exp {
        0 if frac == 0 => sign << 31,
        0 => {
            // subnormal: normalise
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            (sign << 31) | (e << 23) | ((f & 0x3ff) << 13)
        }
        31 => (sign << 31) | 0x7f80_0000 | (frac << 13),
        _ => (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13),
    };
    f32::from_bits(bits)
}

/// World (model-space) matrix of every bone.
fn bone_matrices(r: &R, skel: usize) -> Result<Vec<Mat4>> {
    if skel == 0 {
        return Ok(vec![]);
    }
    let arr = r.off(skel + 0x10)?;
    let n = r.u16(skel + 0x38)? as usize;
    let mut world: Vec<Mat4> = Vec::with_capacity(n);
    for i in 0..n {
        let b = arr + i * 0x58;
        let parent = r.i16(b + 0x22)?;
        let flags = r.u32(b + 0x2c)?;
        let scale = r.vec3(b + 0x30)?;
        let rot = Vec4::new(r.f32(b + 0x3c)?, r.f32(b + 0x40)?, r.f32(b + 0x44)?, r.f32(b + 0x48)?);
        let pos = r.vec3(b + 0x4c)?;
        let q = if flags & 0x7000 == 0x1000 {
            Quat::from_euler(EulerRot::ZYX, rot.z, rot.y, rot.x)
        } else {
            Quat::from_xyzw(rot.x, rot.y, rot.z, rot.w).normalize()
        };
        let local = Mat4::from_scale_rotation_translation(scale, q, pos);
        // parents come before their children
        let m = match usize::try_from(parent).ok().and_then(|p| world.get(p)) {
            Some(p) => *p * local,
            None => local,
        };
        world.push(m);
    }
    Ok(world)
}

/// Render info values of a material that are strings, by name.
fn render_info(r: &R, mat: usize) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let assign = r.off(r.off(mat + 0x10)?)?;
    let keys = r.dict(r.off(assign + 0x18)?)?;
    let infos = r.off(assign + 0x10)?;
    let (values, counts, offsets) = (r.off(mat + 0x40)?, r.off(mat + 0x48)?, r.off(mat + 0x50)?);
    if values == 0 || counts == 0 || offsets == 0 {
        return Ok(out);
    }
    for (k, name) in keys.into_iter().enumerate() {
        let ty = r.u8(infos + 16 * k + 8)?;
        if ty != 2 || r.u16(counts + 4 * k)? == 0 {
            continue;
        }
        let v = r.str_at(values + r.u16(offsets + 2 * k)? as usize)?;
        out.insert(name, v);
    }
    Ok(out)
}

struct Material {
    name: String,
    albedo: Option<String>,
    cutout: bool,
}

fn material(r: &R, mat: usize) -> Result<Material> {
    let name = r.str_at(mat + 0x08)?;
    let ntex = r.u8(mat + 0xa3)? as usize;
    let tex_names = r.off(mat + 0x20)?;
    let textures = (0..ntex).map(|i| r.str_at(tex_names + 8 * i)).collect::<Result<Vec<_>>>()?;
    let samplers = r.dict(r.off(mat + 0x38)?)?;
    // textures and samplers are parallel; `_a0` is the albedo sampler
    let albedo = samplers
        .iter()
        .position(|s| s == "_a0")
        .and_then(|i| textures.get(i))
        .or_else(|| textures.iter().find(|t| t.ends_with("_Alb")))
        .cloned();
    let info = render_info(r, mat).unwrap_or_default();
    let cutout =
        info.get("gsys_alpha_test_enable").is_some_and(|v| v == "true") || info.get("gsys_render_state_mode").is_some_and(|v| v == "mask");
    Ok(Material { name, albedo, cutout })
}

/// The first model of a decompressed BFRES file.
pub fn load_model(d: &[u8]) -> Result<Model> {
    let r = R(d);
    if !d.starts_with(b"FRES") {
        return Err("not a BFRES file".into());
    }
    if r.u16(0xdc)? == 0 {
        return Ok(Model::default());
    }
    let buffers = r.off(r.off(0xb0)? + 8)?;
    let m = r.off(0x28)?;
    if r.bytes::<4>(m)? != *b"FMDL" {
        return Err("BFRES: no model".into());
    }
    let bones = bone_matrices(&r, r.off(m + 0x18)?)?;
    let skel = r.off(m + 0x18)?;
    let matrix_to_bone: Vec<u16> = if skel == 0 {
        vec![]
    } else {
        let n = r.u16(skel + 0x3a)? as usize + r.u16(skel + 0x3c)? as usize;
        let list = r.off(skel + 0x18)?;
        if list == 0 { vec![] } else { (0..n).map(|i| r.u16(list + 2 * i)).collect::<Result<_>>()? }
    };
    let (shapes, mats) = (r.off(m + 0x28)?, r.off(m + 0x38)?);
    let (nshape, nmat) = (r.u16(m + 0x6a)? as usize, r.u16(m + 0x6c)? as usize);
    let materials = (0..nmat).map(|i| material(&r, mats + 0xb0 * i)).collect::<Result<Vec<_>>>()?;

    let mut parts: Vec<Part> = vec![];
    let mut part_of: HashMap<usize, usize> = HashMap::new();
    for s in 0..nshape {
        let sh = shapes + 0x60 * s;
        if r.bytes::<4>(sh)? != *b"FSHP" {
            return Err("BFRES: bad shape".into());
        }
        let shape_name = r.str_at(sh + 8)?;
        let mat_index = r.u16(sh + 0x52)? as usize;
        let bone = r.u16(sh + 0x54)? as usize;
        let skin = r.u8(sh + 0x5a)?;
        if r.u8(sh + 0x5b)? == 0 {
            continue;
        }
        let vb = r.off(sh + 0x10)?;
        let nverts = r.u32(vb + 0x50)? as usize;
        let nattr = r.u8(vb + 0x4c)? as usize;
        let nbuf = r.u8(vb + 0x4d)? as usize;
        // buffer b starts after the ones before it
        let mut starts = Vec::with_capacity(nbuf);
        let mut strides = Vec::with_capacity(nbuf);
        let mut at = buffers + r.u32(vb + 0x48)? as usize;
        for b in 0..nbuf {
            starts.push(at);
            strides.push(r.u32(r.off(vb + 0x38)? + 0x10 * b)? as usize);
            at += r.u32(r.off(vb + 0x30)? + 0x10 * b)? as usize;
        }
        let attrs = r.off(vb + 8)?;
        let find = |want: &str| -> Result<Option<(usize, usize, u32)>> {
            for a in 0..nattr {
                let o = attrs + 0x10 * a;
                if r.str_at(o)? == want {
                    let buf = r.u16(o + 0xe)? as usize;
                    let (start, stride) = (*starts.get(buf).ok_or("BFRES: bad buffer index")?, strides[buf]);
                    return Ok(Some((start + r.u16(o + 0xc)? as usize, stride, r.u32(o + 8)?)));
                }
            }
            Ok(None)
        };
        let Some(pos) = find("_p0")? else { continue };
        let (nrm, uv, idx) = (find("_n0")?, find("_u0")?, find("_i0")?);
        let read = |a: (usize, usize, u32), v: usize| attribute(&r, a.0 + a.1 * v, a.2);
        let shape_bone = bones.get(bone).copied().unwrap_or(Mat4::IDENTITY);

        let mesh = r.off(sh + 0x18)?;
        let (index_at, prim, ifmt, count, base) =
            (r.u32(mesh + 0x20)?, r.u32(mesh + 0x24)?, r.u32(mesh + 0x28)?, r.u32(mesh + 0x2c)? as usize, r.u32(mesh + 0x30)?);
        if prim != 3 {
            return Err(format!("BFRES: shape {shape_name} draws primitive type {prim}").into());
        }
        let ibuf = buffers + index_at as usize;
        let index = |i: usize| -> Result<u32> {
            Ok(base
                + match ifmt {
                    0 => r.u8(ibuf + i)? as u32,
                    1 => r.u16(ibuf + 2 * i)? as u32,
                    _ => r.u32(ibuf + 4 * i)?,
                })
        };

        let mat = materials.get(mat_index);
        let pi = *part_of.entry(mat_index).or_insert_with(|| {
            parts.push(Part {
                material: mat.map(|m| m.name.clone()).unwrap_or_default(),
                texture: mat.and_then(|m| m.albedo.clone()),
                cutout: mat.is_some_and(|m| m.cutout),
                alpha_cutoff: 0.5,
                ..Default::default()
            });
            parts.len() - 1
        });
        let part = &mut parts[pi];
        let first = part.positions.len() as u32;
        part.positions.reserve(nverts);
        for v in 0..nverts {
            let world = match (skin, idx) {
                (0, _) => shape_bone,
                (1, Some(ia)) => {
                    let mi = read(ia, v)?.x as usize;
                    matrix_to_bone.get(mi).and_then(|&b| bones.get(b as usize)).copied().unwrap_or(Mat4::IDENTITY)
                }
                _ => Mat4::IDENTITY,
            };
            let p = world.transform_point3(read(pos, v)?.truncate());
            let n = match nrm {
                Some(na) => (Mat3::from_mat4(world) * read(na, v)?.truncate()).normalize_or(Vec3::Y),
                None => Vec3::Y,
            };
            let t = match uv {
                Some(ua) => {
                    let t = read(ua, v)?;
                    [t.x, t.y]
                }
                None => [0.0, 0.0],
            };
            part.positions.push(p.into());
            part.normals.push(n.into());
            part.uvs.push(t);
        }
        part.indices.reserve(count);
        for i in 0..count {
            let k = index(i)?;
            if k as usize >= nverts {
                return Err(format!("BFRES: shape {shape_name} index {k} past {nverts} vertices").into());
            }
            part.indices.push(first + k);
        }
    }
    parts.retain(|p| !p.indices.is_empty());
    parts.sort_by(|a, b| a.material.cmp(&b.material));
    let mut model = Model { parts, ..Default::default() };
    model.update_bounds();
    Ok(model)
}

/// Path of a model in the romfs from the `Fmdb` its ModelInfo names:
/// `Work/Model/CmnObj/CmnObj_ItemBox000/output/CmnObj_ItemBox000_A.fmdb` ->
/// `Model/CmnObj_ItemBox000.CmnObj_ItemBox000_A.bfres.zs`.
pub fn model_rel(fmdb: &str) -> Option<String> {
    let parts: Vec<&str> = fmdb.split('/').collect();
    if parts.len() < 5 || parts[0] != "Work" || parts[1] != "Model" {
        return None;
    }
    let last = parts.last()?;
    let model = last.rsplit_once('.').map_or(*last, |x| x.0);
    Some(format!("Model/{}.{model}.bfres.zs", parts[3]))
}

/// Read and parse a model by its path in the romfs.
pub fn load(romfs: &Path, rel: &str) -> Result<Model> {
    let path = romfs.join(rel);
    let d = crate::io::read_file(&path)?;
    load_model(&d).path_context(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_and_packed_normals() {
        assert_eq!([half(0x3c00), half(0xc000), half(0x0000), half(0x3555)], [1.0, -2.0, 0.0, 0.333_251_95]);
        assert!((half(0x0001) - 5.960_464_5e-8).abs() < 1e-12);
        // 10_10_10_2 SNORM: x = 511 (1.0), y = -511 (-1.0), z = 0
        let v: u32 = 511 | ((1024 - 511) << 10);
        let n = attribute(&R(&v.to_le_bytes()), 0, 0x0e02).unwrap();
        assert_eq!((n.x, n.y, n.z), (1.0, -1.0, 0.0));
    }

    #[test]
    fn model_paths() {
        assert_eq!(
            model_rel("Work/Model/CmnObj/CmnObj_ItemBox000/output/CmnObj_ItemBox000_A.fmdb").as_deref(),
            Some("Model/CmnObj_ItemBox000.CmnObj_ItemBox000_A.bfres.zs")
        );
        assert_eq!(model_rel("Model/x.fmdb"), None);
    }
}

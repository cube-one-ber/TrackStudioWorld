//! The open world's terrain, read from the game's terrain tiles and turned into a mesh under a
//! course.
//!
//! `TerrainArc/Game03_Proc.tscb` lists the tiles of a quadtree over the world: each with its
//! name, centre and size in units of the scene's scale (250 m). The root is 64 units (16 km)
//! centred on the origin and the finest tiles are 0.5 units. Not every tile is split: a quarter
//! with no smaller tile in the list is drawn from its parent. `TerrainArc/Game03_Proc/<name>.hght` holds a tile's heights: a TARB header,
//! then zstd, then a fixed layout whose second section is a 260 x 260 R16 surface, block linear
//! like the BNTX textures (Orin GOBs, one GOB per block). Sample `i` along X (or Z) sits at
//! `tile min + (i - 2) * size / 256`, so 2..=258 span the tile and the rest overlap its
//! neighbours. A sample is `value / 65536 * max height` metres; 0 is a hole, cut where a stage
//! model sits. `Map_GroundColor_Root_GroundColor` colours the 16 km square.

use crate::error::{Context, Result};
use crate::model::{Model, Part};
use std::path::Path;

/// The world colour map, used as the terrain's albedo texture.
pub const COLOR_TEXTURE: &str = "Map_GroundColor_Root_GroundColor";
/// Full-resolution world colour map (2048x2048), used when high-quality terrain is enabled.
pub const COLOR_TEXTURE_HQ: &str = "Map_GroundColor_Root_GroundColor@2048";
/// Side of the square the colour map covers, in metres, centred on the origin.
const COLOR_SIZE: f32 = 16000.0;
const SCENE: &str = "TerrainArc/Game03_Proc";
/// Samples on a side of a tile's height surface, and the cells between its edges.
const SAMPLES: usize = 260;
const CELLS: usize = 256;
/// First sample on a tile's edge.
const BORDER: usize = 2;
/// Rough upper bound of mesh cells in one terrain; coarser spacing beyond it.
const MAX_CELLS_STANDARD: usize = 600_000;
const MAX_CELLS_HIGH: usize = 2_400_000;

/// Mesh and texture detail level for open-world terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    #[default]
    Standard,
    High,
}

/// One tile of the quadtree, in world metres.
#[derive(Debug, Clone)]
pub struct Tile {
    pub name: String,
    pub centre: [f32; 2],
    pub size: f32,
    /// bit `qx + 2 * qz` is set when a smaller tile covers that quarter (+X, +Z halves)
    pub children: u8,
}

/// The quadtree of a terrain scene.
#[derive(Debug, Clone)]
pub struct Scene {
    pub tiles: Vec<Tile>,
    /// height of a sample value of 65536, in metres
    pub max_height: f32,
}

fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(d.get(o..o + 4).and_then(|s| s.try_into().ok()).ok_or("TSCB: unexpected end of file")?))
}
fn f32_at(d: &[u8], o: usize) -> Result<f32> {
    Ok(f32::from_bits(u32_at(d, o)?))
}
/// Offsets in TSCB and hght files count from where they are stored.
fn rel_at(d: &[u8], o: usize) -> Result<usize> {
    Ok(o + u32_at(d, o)? as usize)
}

/// Parse a `.tscb` file: scale and max height at 0x10 and 0x14, the tile count at 0x38 and
/// the tile offsets at 0x78; each tile starts with its centre X, Z and size (f32) and names
/// its file at +0x60.
pub fn parse_scene(d: &[u8]) -> Result<Scene> {
    if !d.starts_with(b"TSCB") {
        return Err("not a TSCB file".into());
    }
    let (scale, max_height) = (f32_at(d, 0x10)?, f32_at(d, 0x14)?);
    let (count, table) = (u32_at(d, 0x38)? as usize, rel_at(d, 0x78)?);
    let mut tiles = Vec::with_capacity(count);
    for k in 0..count {
        let e = rel_at(d, table + 4 * k)?;
        let n = rel_at(d, e + 0x60)?;
        let end = d.get(n..).and_then(|s| s.iter().position(|&b| b == 0)).ok_or("TSCB: bad tile name")?;
        tiles.push(Tile {
            name: String::from_utf8_lossy(&d[n..n + end]).into_owned(),
            centre: [f32_at(d, e)? * scale, f32_at(d, e + 4)? * scale],
            size: f32_at(d, e + 8)? * scale,
            children: 0,
        });
    }
    // mark each tile's quarter in its parent, found by its cell in the grid of the parent's
    // size laid from the root's corner
    let half = tiles.iter().map(|t| t.size).fold(0.0, f32::max) / 2.0;
    let cell = |c: [f32; 2], s: f32| (((c[0] + half) / s).floor() as i64, ((c[1] + half) / s).floor() as i64, s.to_bits());
    let index: std::collections::HashMap<_, usize> = tiles.iter().enumerate().map(|(i, t)| (cell(t.centre, t.size), i)).collect();
    for i in 0..tiles.len() {
        let (c, s) = (tiles[i].centre, tiles[i].size);
        if let Some(&p) = index.get(&cell(c, s * 2.0)) {
            let (qx, qz) = (((c[0] + half) / s).floor() as i64 & 1, ((c[1] + half) / s).floor() as i64 & 1);
            tiles[p].children |= 1 << (qx + 2 * qz);
        }
    }
    Ok(Scene { tiles, max_height })
}

pub fn read_scene(romfs: &Path) -> Result<Scene> {
    let path = romfs.join(format!("{SCENE}.tscb"));
    parse_scene(&crate::io::read_file(&path)?).path_context(&path)
}

/// Unwrap a TARB file: a 20-byte header (`TARB`, version, header size at 0xa, sizes) around
/// one zstd frame.
fn untarb(raw: &[u8]) -> Result<Vec<u8>> {
    if !raw.starts_with(b"TARB") || raw.len() < 12 {
        return Err("not a TARB file".into());
    }
    let start = u16::from_le_bytes([raw[10], raw[11]]) as usize;
    Ok(zstd::decode_all(raw.get(start..).ok_or("TARB: unexpected end of file")?)?)
}

/// The 260 x 260 height samples of a decompressed `.hght`, rows along +Z. Its header places
/// the height surface with an offset at 0x2c and a size at 0x30.
pub fn parse_heights(d: &[u8]) -> Result<Vec<u16>> {
    let (start, len) = (rel_at(d, 0x2c)?, u32_at(d, 0x30)? as usize);
    let surface = d.get(start..start + len).ok_or("hght: height surface outside the file")?;
    let lin = crate::bntx::untile(surface, SAMPLES * 2, SAMPLES, 1, true);
    Ok(lin.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect())
}

pub fn read_heights(romfs: &Path, tile: &str) -> Result<Vec<u16>> {
    let path = romfs.join(format!("{SCENE}/{tile}.hght"));
    let raw = std::fs::read(&path).path_context(&path)?;
    parse_heights(&untarb(&raw).path_context(&path)?).path_context(&path)
}

/// The square of ground around `points` (world positions) with `margin` metres on each side,
/// as `[min_x, min_z, max_x, max_z]`. None without points.
pub fn region_around(points: &[[f32; 3]], margin: f32) -> Option<[f32; 4]> {
    let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in points {
        (mn[0], mn[1]) = (mn[0].min(p[0]), mn[1].min(p[2]));
        (mx[0], mx[1]) = (mx[0].max(p[0]), mx[1].max(p[2]));
    }
    (mn[0] <= mx[0]).then_some([mn[0] - margin, mn[1] - margin, mx[0] + margin, mx[1] + margin])
}

/// Model cache key of the terrain of `region` at standard quality. Terrain shares the model
/// cache with the romfs models, so its key carries the region and never names a file.
pub fn model_key(region: [f32; 4]) -> String {
    model_key_with_quality(region, Quality::Standard)
}

/// Model cache key of the terrain of `region` at `quality`.
pub fn model_key_with_quality(region: [f32; 4], quality: Quality) -> String {
    let tag = match quality {
        Quality::Standard => "terrain",
        Quality::High => "terrain:hq",
    };
    format!("{tag}:{}:{}:{}:{}", region[0].floor(), region[1].floor(), region[2].ceil(), region[3].ceil())
}

/// The region of a [`model_key`], None for any other key.
pub fn parse_key(key: &str) -> Option<[f32; 4]> {
    parse_key_with_quality(key).map(|(r, _)| r)
}

/// The region and quality of a terrain model cache key, None for any other key.
pub fn parse_key_with_quality(key: &str) -> Option<([f32; 4], Quality)> {
    let (rest, quality) =
        if let Some(r) = key.strip_prefix("terrain:hq:") { (r, Quality::High) } else { (key.strip_prefix("terrain:")?, Quality::Standard) };
    let v: Vec<f32> = rest.split(':').map(|s| s.parse().ok()).collect::<Option<_>>()?;
    Some((v.try_into().ok()?, quality))
}

/// Metres between mesh vertices wanted at `d` metres from the course: finest at the course,
/// coarser further out.
fn spacing_at(d: f32, quality: Quality) -> f32 {
    match quality {
        Quality::Standard => match d {
            d if d < 100.0 => 1.0,
            d if d < 300.0 => 2.0,
            _ => 8.0,
        },
        Quality::High => match d {
            d if d < 200.0 => 0.5,
            d if d < 500.0 => 1.0,
            _ => 2.0,
        },
    }
}

/// The terrain of `region` (`[min_x, min_z, max_x, max_z]`, world metres) as a model in world
/// space, textured with [`COLOR_TEXTURE`]: one part per tile it touches that smaller tiles do
/// not wholly cover. Tiles are meshed finest near `points` (the course) and coarser away from
/// it. Cells on a hole are left out. Empty when the region misses the terrain.
pub fn load(romfs: &Path, region: [f32; 4], points: &[[f32; 3]]) -> Result<Model> {
    load_with_quality(romfs, region, points, Quality::Standard)
}

/// [`load`] at the requested [`Quality`].
pub fn load_with_quality(romfs: &Path, region: [f32; 4], points: &[[f32; 3]], quality: Quality) -> Result<Model> {
    let scene = read_scene(romfs)?;
    let tiles: Vec<&Tile> = scene
        .tiles
        .iter()
        .filter(|t| {
            let h = t.size / 2.0;
            t.children != 0b1111
                && t.centre[0] + h > region[0]
                && t.centre[0] - h < region[2]
                && t.centre[1] + h > region[1]
                && t.centre[1] - h < region[3]
        })
        .collect();
    // samples between vertices per tile, a power of two; a cell may not straddle a quarter
    // that a smaller tile covers
    let caps: Vec<usize> = tiles.iter().map(|t| if t.children == 0 { CELLS } else { CELLS / 2 }).collect();
    let mut strides: Vec<usize> = tiles
        .iter()
        .zip(&caps)
        .map(|(t, &cap)| {
            let h = t.size / 2.0;
            let d = points
                .iter()
                .map(|p| ((p[0] - t.centre[0]).abs() - h).max(0.0).hypot(((p[2] - t.centre[1]).abs() - h).max(0.0)))
                .fold(f32::MAX, f32::min);
            let want = (spacing_at(d, quality) / (t.size / CELLS as f32)).max(1.0) as usize;
            (1usize << want.ilog2()).min(cap)
        })
        .collect();
    let max_cells = match quality {
        Quality::Standard => MAX_CELLS_STANDARD,
        Quality::High => MAX_CELLS_HIGH,
    };
    let cells = |s: &[usize]| s.iter().map(|s| (CELLS / s).pow(2)).sum::<usize>();
    while cells(&strides) > max_cells && strides.iter().zip(&caps).any(|(s, c)| s < c) {
        strides.iter_mut().zip(&caps).for_each(|(s, &c)| *s = (*s * 2).min(c));
    }
    let tex = match quality {
        Quality::Standard => COLOR_TEXTURE,
        Quality::High => COLOR_TEXTURE_HQ,
    };
    let scale = scene.max_height / 65536.0;
    let mut model = Model::default();
    for (t, &step) in tiles.iter().zip(&strides) {
        let h = read_heights(romfs, &t.name)?;
        let at = |i: isize, j: isize| h[j.clamp(0, SAMPLES as isize - 1) as usize * SAMPLES + i.clamp(0, SAMPLES as isize - 1) as usize];
        let (cell, n) = (t.size / CELLS as f32, CELLS / step);
        let origin = [t.centre[0] - t.size / 2.0, t.centre[1] - t.size / 2.0];
        let mut part = Part { material: "Terrain".into(), texture: Some(tex.into()), ..Part::default() };
        for z in 0..=n {
            for x in 0..=n {
                let (i, j) = ((BORDER + x * step) as isize, (BORDER + z * step) as isize);
                let (wx, wz) = (origin[0] + (x * step) as f32 * cell, origin[1] + (z * step) as f32 * cell);
                let y = at(i, j) as f32 * scale;
                part.positions.push([wx, y, wz]);
                // central differences; a hole next door counts as level ground
                let s = step as isize;
                let nb = |a: isize, b: isize| match at(a, b) {
                    0 => y,
                    v => v as f32 * scale,
                };
                let dx = (nb(i + s, j) - nb(i - s, j)) / (2.0 * s as f32 * cell);
                let dz = (nb(i, j + s) - nb(i, j - s)) / (2.0 * s as f32 * cell);
                part.normals.push(glam::Vec3::new(-dx, 1.0, -dz).normalize().into());
                part.uvs.push([wx / COLOR_SIZE + 0.5, wz / COLOR_SIZE + 0.5]);
            }
        }
        let row = n + 1;
        let hole = |k: usize| at((BORDER + k % row * step) as isize, (BORDER + k / row * step) as isize) == 0;
        for z in 0..n {
            for x in 0..n {
                let quarter = (x * step) / (CELLS / 2) + 2 * ((z * step) / (CELLS / 2));
                if t.children & (1 << quarter) != 0 {
                    continue;
                }
                let a = z * row + x;
                let quad = [a, a + 1, a + row, a + row + 1];
                if quad.iter().any(|&k| hole(k)) {
                    continue;
                }
                part.indices.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1].map(|k| k as u32));
            }
        }
        if !part.indices.is_empty() {
            model.parts.push(part);
        }
    }
    model.update_bounds();
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        let r = [-120.0, 40.0, 900.0, 1500.0];
        assert_eq!(parse_key(&model_key(r)), Some(r));
        assert_eq!(parse_key_with_quality(&model_key_with_quality(r, Quality::Standard)), Some((r, Quality::Standard)));
        assert_eq!(parse_key_with_quality(&model_key_with_quality(r, Quality::High)), Some((r, Quality::High)));
        assert_eq!(parse_key("Model/Stage_X.bfres.zs"), None);
    }
}

//! BNTX textures (`Tex/*.bntx.zs`): the texture header, block-linear untiling and decoding to
//! RGBA8.
//!
//! The game's textures are on the Switch 2 platform (`Ounc`). Their surfaces are block linear
//! like the Switch's Tegra X1 (512-byte GOBs, 64 bytes by 8 rows, stacked in blocks of
//! 2^`layout` GOBs), but the bytes inside each GOB are laid out the way the Orin TRM describes:
//! 16-byte runs, four rows of them per 128-byte half. Switch 1 (`NX  `) files use the Tegra X1
//! order and are read too.

use crate::error::{Context, Result};
use std::path::Path;

/// One texture of a BNTX file.
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    /// Switch 2 GOB layout
    pub orin: bool,
    pub tile_mode: u16,
    pub mip_count: u16,
    /// nn::gfx image format: channel layout in the high byte, type (UNORM, SRGB...) in the low
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub array_count: u32,
    /// log2 of the first mip's block height in GOBs
    pub block_height_log2: u32,
    /// component selectors for R, G, B, A: 0 zero, 1 one, 2..5 R, G, B, A
    pub channels: [u8; 4],
    /// file offset of each mip of the first layer
    pub mip_offsets: Vec<usize>,
}

/// A decoded image, rows top to bottom.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// Whether any texel is less than opaque.
    pub fn has_alpha(&self) -> bool {
        self.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255)
    }
}

fn rd<const N: usize>(d: &[u8], o: usize) -> Result<[u8; N]> {
    d.get(o..o + N).and_then(|s| s.try_into().ok()).ok_or_else(|| "BNTX: unexpected end of file".into())
}
fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(rd(d, o)?))
}
fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(rd(d, o)?))
}
fn off_at(d: &[u8], o: usize) -> Result<usize> {
    Ok(u64::from_le_bytes(rd(d, o)?) as usize)
}

/// (block width, block height, bytes per block) of an image format's channel layout.
fn block_info(format: u32) -> Option<(u32, u32, u32)> {
    Some(match format >> 8 {
        0x02 => (1, 1, 1),                       // R8
        0x09 | 0x0a => (1, 1, 2),                // R8G8, R16
        0x0b | 0x0c => (1, 1, 4),                // R8G8B8A8, B8G8R8A8
        0x15 => (1, 1, 8),                       // R16G16B16A16
        0x1a | 0x1d => (4, 4, 8),                // BC1, BC4
        0x1b | 0x1c | 0x1e..=0x20 => (4, 4, 16), // BC2, BC3, BC5, BC6H, BC7
        0x2d => (4, 4, 16),
        0x2e => (5, 4, 16),
        0x2f => (5, 5, 16),
        0x30 => (6, 5, 16),
        0x31 => (6, 6, 16),
        0x32 => (8, 5, 16),
        0x33 => (8, 6, 16),
        0x34 => (8, 8, 16),
        0x35 => (10, 5, 16),
        0x36 => (10, 6, 16),
        0x37 => (10, 8, 16),
        0x38 => (10, 10, 16),
        0x39 => (12, 10, 16),
        0x3a => (12, 12, 16),
        _ => return None,
    })
}

pub fn parse(d: &[u8]) -> Result<Vec<Texture>> {
    if !d.starts_with(b"BNTX") {
        return Err("not a BNTX file".into());
    }
    let orin = rd::<4>(d, 0x20)? == *b"Ounc";
    let count = u32_at(d, 0x24)? as usize;
    let infos = off_at(d, 0x28)?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count.min(4096) {
        let p = off_at(d, infos + 8 * i)?;
        if rd::<4>(d, p)? != *b"BRTI" {
            return Err("BNTX: bad texture info".into());
        }
        let mip_count = u16_at(d, p + 0x16)?.max(1);
        let name_at = off_at(d, p + 0x60)?;
        let name_len = u16_at(d, name_at)? as usize;
        let name = String::from_utf8_lossy(d.get(name_at + 2..name_at + 2 + name_len).ok_or("BNTX: bad name")?).into_owned();
        let mips = off_at(d, p + 0x70)?;
        let mip_offsets = (0..mip_count as usize).map(|m| off_at(d, mips + 8 * m)).collect::<Result<Vec<_>>>()?;
        out.push(Texture {
            name,
            orin,
            tile_mode: u16_at(d, p + 0x12)?,
            mip_count,
            format: u32_at(d, p + 0x1c)?,
            width: u32_at(d, p + 0x24)?,
            height: u32_at(d, p + 0x28)?,
            array_count: u32_at(d, p + 0x30)?,
            block_height_log2: u32_at(d, p + 0x34)? & 7,
            channels: rd(d, p + 0x58)?,
            mip_offsets,
        });
    }
    Ok(out)
}

/// Offset of byte (x, y) inside a 64x8-byte GOB.
fn gob_offset(x: usize, y: usize, orin: bool) -> usize {
    if orin {
        (x / 32) * 256 + (y / 4) * 128 + ((x % 32) / 16) * 64 + (y % 4) * 16 + x % 16
    } else {
        (x / 32) * 256 + (y / 2) * 64 + ((x % 32) / 16) * 32 + (y % 2) * 16 + x % 16
    }
}

/// Block-linear surface (`rows` rows of `width_bytes`) to row-major bytes. Bytes past the end
/// of `data` read as zero.
fn untile(data: &[u8], width_bytes: usize, rows: usize, block_height: usize, orin: bool) -> Vec<u8> {
    let mut out = vec![0u8; width_bytes * rows];
    let gobs_wide = width_bytes.div_ceil(64);
    let block_bytes = 512 * block_height;
    for y in 0..rows {
        let row_base = (y / (8 * block_height)) * gobs_wide * block_bytes + ((y % (8 * block_height)) / 8) * 512;
        for x in (0..width_bytes).step_by(16) {
            let src = row_base + (x / 64) * block_bytes + gob_offset(x % 64, y % 8, orin);
            let n = 16.min(width_bytes - x);
            if let Some(s) = data.get(src..src + n) {
                out[y * width_bytes + x..y * width_bytes + x + n].copy_from_slice(s);
            }
        }
    }
    out
}

/// Decode one texture to RGBA8 at the largest mip no bigger than `max_size` on either side (the
/// smallest mip when none is), with its component selectors applied.
pub fn decode(d: &[u8], t: &Texture, max_size: u32) -> Result<Image> {
    let (bw, bh, bpb) = block_info(t.format).ok_or_else(|| format!("{}: unsupported format {:#06x}", t.name, t.format))?;
    let level = (0..t.mip_count as u32)
        .find(|&l| (t.width >> l).max(1) <= max_size && (t.height >> l).max(1) <= max_size)
        .unwrap_or(t.mip_count as u32 - 1);
    let (w, h) = ((t.width >> level).max(1), (t.height >> level).max(1));
    let (bx, by) = (w.div_ceil(bw) as usize, h.div_ceil(bh) as usize);
    let width_bytes = bx * bpb as usize;
    let start = t.mip_offsets[level as usize];
    let data = d.get(start..).ok_or("BNTX: mip outside the file")?;
    let lin = if t.tile_mode == 1 {
        let pitch = width_bytes.div_ceil(32) * 32;
        let mut v = vec![0u8; width_bytes * by];
        for (y, row) in v.chunks_exact_mut(width_bytes).enumerate() {
            if let Some(s) = data.get(y * pitch..y * pitch + width_bytes) {
                row.copy_from_slice(s);
            }
        }
        v
    } else {
        let mut block_height = 1usize << t.block_height_log2;
        while block_height > 1 && by <= (block_height / 2) * 8 {
            block_height /= 2;
        }
        untile(data, width_bytes, by, block_height, t.orin)
    };
    let (wu, hu) = (w as usize, h as usize);
    let mut px = vec![0u32; wu * hu]; // texture2ddecoder writes BGRA
    let kind = t.format >> 8;
    let ty = t.format & 0xff;
    // the block decoders come from texture2ddecoder; a panic on odd data becomes an error
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match kind {
        0x2d..=0x3a => texture2ddecoder::decode_astc(&lin, wu, hu, bw as usize, bh as usize, &mut px),
        0x1a => texture2ddecoder::decode_bc1(&lin, wu, hu, &mut px),
        0x1b => texture2ddecoder::decode_bc2(&lin, wu, hu, &mut px),
        0x1c => texture2ddecoder::decode_bc3(&lin, wu, hu, &mut px),
        0x1d => texture2ddecoder::decode_bc4(&lin, wu, hu, &mut px),
        0x1e => {
            let mut lin = lin;
            if ty == 2 {
                // signed endpoints: re-bias so the unsigned decoder gives the same ramp
                for b in lin.as_chunks_mut::<8>().0 {
                    b[0] ^= 0x80;
                    b[1] ^= 0x80;
                }
            }
            texture2ddecoder::decode_bc5(&lin, wu, hu, &mut px)
        }
        0x1f => texture2ddecoder::decode_bc6(&lin, wu, hu, &mut px, ty == 2),
        0x20 => texture2ddecoder::decode_bc7(&lin, wu, hu, &mut px),
        _ => {
            let bgra = |r: u8, g: u8, b: u8, a: u8| u32::from_le_bytes([b, g, r, a]);
            for (i, p) in px.iter_mut().enumerate() {
                *p = match kind {
                    0x02 => bgra(lin[i], 0, 0, 255),
                    0x09 => bgra(lin[2 * i], lin[2 * i + 1], 0, 255),
                    0x0a => bgra(lin[2 * i + 1], 0, 0, 255),
                    0x0b => bgra(lin[4 * i], lin[4 * i + 1], lin[4 * i + 2], lin[4 * i + 3]),
                    0x15 => {
                        let c = |k: usize| {
                            let v = u16::from_le_bytes([lin[8 * i + 2 * k], lin[8 * i + 2 * k + 1]]);
                            match ty {
                                5 => (crate::bfres::half(v).clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                                _ => (v >> 8) as u8,
                            }
                        };
                        bgra(c(0), c(1), c(2), c(3))
                    }
                    _ => bgra(lin[4 * i + 2], lin[4 * i + 1], lin[4 * i], lin[4 * i + 3]),
                };
            }
            Ok(())
        }
    }))
    .unwrap_or(Err("decoder failed"));
    res.map_err(|e| format!("{}: {e}", t.name))?;
    let mut rgba = Vec::with_capacity(wu * hu * 4);
    for p in px {
        let [b, g, r, a] = p.to_le_bytes();
        let src = [r, g, b, a];
        rgba.extend(t.channels.map(|c| match c {
            0 => 0,
            2..=5 => src[c as usize - 2],
            _ => 255,
        }));
    }
    Ok(Image { width: w, height: h, rgba })
}

/// Path of a texture in the romfs.
pub fn texture_path(romfs: &Path, name: &str) -> std::path::PathBuf {
    romfs.join("Tex").join(format!("{name}.bntx.zs"))
}

/// Read and decode the texture `Tex/<name>.bntx.zs` (its first texture) at most `max_size`
/// texels on a side.
pub fn load(romfs: &Path, name: &str, max_size: u32) -> Result<Image> {
    let path = texture_path(romfs, name);
    let d = crate::io::read_file(&path)?;
    let t = parse(&d).path_context(&path)?;
    let t = t.first().ok_or("BNTX: no texture").path_context(&path)?;
    decode(&d, t, max_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gob_layouts_are_permutations() {
        for orin in [false, true] {
            let mut seen = [false; 512];
            for y in 0..8 {
                for x in 0..64 {
                    let o = gob_offset(x, y, orin);
                    assert!(!seen[o], "{orin} {x} {y}");
                    seen[o] = true;
                }
            }
        }
        // the Orin GOB keeps four rows of a 16-byte column together
        assert_eq!((gob_offset(0, 1, true), gob_offset(16, 0, true), gob_offset(0, 4, true)), (16, 64, 128));
        assert_eq!((gob_offset(0, 1, false), gob_offset(16, 0, false), gob_offset(0, 2, false)), (16, 32, 64));
    }

    #[test]
    fn untile_one_gob_column() {
        // 16 bytes wide, 16 rows, block height 2: the first 8 rows fill GOB 0, the next GOB 1
        let mut tiled = vec![0u8; 1024];
        for y in 0..16 {
            for x in 0..16 {
                tiled[(y / 8) * 512 + gob_offset(x, y % 8, true)] = (y * 16 + x) as u8;
            }
        }
        let lin = untile(&tiled, 16, 16, 2, true);
        assert!(lin.iter().enumerate().all(|(i, &b)| b == i as u8));
    }
}

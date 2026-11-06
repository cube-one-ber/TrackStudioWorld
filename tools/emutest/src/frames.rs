//! Simple frame measurements and contact sheets.

use image::{RgbImage, imageops};

/// Fraction of the pixels in a rectangle (x, y, w, h) that pass `test`.
pub fn fraction(img: &RgbImage, r: (u32, u32, u32, u32), test: impl Fn([u8; 3]) -> bool) -> f32 {
    let (x0, y0, w, h) = r;
    let (mut n, mut hit) = (0u32, 0u32);
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            n += 1;
            hit += test(img.get_pixel(x, y).0) as u32;
        }
    }
    if n == 0 { 0.0 } else { hit as f32 / n as f32 }
}

/// Fraction of near-white pixels in a rectangle, e.g. on-screen text.
pub fn white_fraction(img: &RgbImage, r: (u32, u32, u32, u32)) -> f32 {
    fraction(img, r, |p| p.iter().all(|&c| c > 220))
}

/// Mean brightness, 0..255.
pub fn mean(img: &RgbImage) -> f32 {
    let sum: u64 = img.pixels().map(|p| p.0.iter().map(|&c| c as u64).sum::<u64>()).sum();
    sum as f32 / (img.width() * img.height() * 3).max(1) as f32
}

/// Mean absolute difference between two frames of the same size, 0..255 (on a 4x subsample).
pub fn diff(a: &RgbImage, b: &RgbImage) -> f32 {
    if a.dimensions() != b.dimensions() {
        return 255.0;
    }
    let (mut sum, mut n) = (0u64, 0u64);
    for y in (0..a.height()).step_by(4) {
        for x in (0..a.width()).step_by(4) {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            sum += (0..3).map(|i| (p[i] as i32 - q[i] as i32).unsigned_abs() as u64).sum::<u64>();
            n += 3;
        }
    }
    sum as f32 / n.max(1) as f32
}

/// Frames in a grid, `cols` wide, each scaled to `tile` pixels wide.
pub fn contact_sheet(frames: &[RgbImage], cols: u32, tile: u32) -> RgbImage {
    let Some(first) = frames.first() else { return RgbImage::new(1, 1) };
    let th = tile * first.height() / first.width().max(1);
    let rows = (frames.len() as u32).div_ceil(cols);
    let gap = 4;
    let mut out = RgbImage::from_pixel(cols * (tile + gap) + gap, rows * (th + gap) + gap, image::Rgb([24, 26, 30]));
    for (i, f) in frames.iter().enumerate() {
        let small = imageops::resize(f, tile, th, imageops::FilterType::Triangle);
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        imageops::replace(&mut out, &small, (gap + c * (tile + gap)) as i64, (gap + r * (th + gap)) as i64);
    }
    out
}

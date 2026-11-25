//! Albedo textures for the viewport: decoded from the romfs (`Tex/<name>.bntx.zs`) and cached as
//! PNG files in `~/.cache/trackstudioworld/textures/`, which Qt Quick 3D loads by URL.

use crate::config::cache_dir;
use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use std::path::{Path, PathBuf};

/// Largest side kept for the viewport by default; bigger textures use a smaller mip.
/// A texture name ending in `@<size>` (e.g. `Map_GroundColor_Root_GroundColor@2048`) overrides
/// this limit and gets its own cache entry.
const MAX_SIZE: u32 = 1024;

fn split_size(name: &str) -> (&str, u32) {
    if let Some((base, sz)) = name.rsplit_once('@')
        && let Ok(n) = sz.parse::<u32>()
        && n > 0
    {
        return (base, n);
    }
    (name, MAX_SIZE)
}

fn mod_tag(mod_romfs: &Path) -> String {
    let h = mod_romfs.to_string_lossy().as_bytes().iter().fold(0x811c9dc5u32, |h, &x| (h ^ x as u32).wrapping_mul(0x01000193));
    format!("mod-{h:08x}")
}

pub fn png_path(name: &str) -> PathBuf {
    cache_dir().join("textures").join(format!("{}.png", name.replace(['/', '\\'], "_")))
}

fn mod_png_path(name: &str, mod_romfs: &Path) -> PathBuf {
    cache_dir().join("textures").join(mod_tag(mod_romfs)).join(format!("{}.png", name.replace(['/', '\\'], "_")))
}

/// The cached PNG of a texture, preferring `mod_romfs`'s copy when present.
pub fn cached(name: &str) -> Option<PathBuf> {
    let p = png_path(name);
    p.is_file().then_some(p)
}

pub fn cached_for(name: &str, mod_romfs: Option<&Path>) -> Option<PathBuf> {
    if let Some(m) = mod_romfs {
        let p = mod_png_path(name, m);
        if p.is_file() {
            return Some(p);
        }
    }
    cached(name)
}

/// Make sure the texture's PNG is cached and not older than the source file (checking
/// `mod_romfs` first when set, then `romfs`).
pub fn ensure(romfs: &Path, mod_romfs: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    let (base, max_size) = split_size(name);
    let mod_src = mod_romfs.map(|m| mkwtrack::bntx::texture_path(m, base)).filter(|p| p.is_file());
    let (src_romfs, src, png) = match (mod_romfs, mod_src) {
        (Some(m), Some(s)) => (m, s, mod_png_path(name, m)),
        _ => (romfs, mkwtrack::bntx::texture_path(romfs, base), png_path(name)),
    };
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let (Some(a), Some(b)) = (modified(&png), modified(&src))
        && a >= b
    {
        return Ok(png);
    }
    let img = mkwtrack::bntx::load(src_romfs, base, max_size).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(img.rgba.len() / 2);
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Sub)
        .write_image(&img.rgba, img.width, img.height, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    mkwtrack::io::write_atomic(&png, &out).map_err(|e| e.to_string())?;
    Ok(png)
}

/// [`ensure`] for many textures on worker threads; returns the errors.
pub fn ensure_all(romfs: &Path, mod_romfs: Option<&Path>, names: &[String]) -> Vec<String> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).saturating_sub(1).clamp(1, 8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let errors = std::sync::Mutex::new(vec![]);
    std::thread::scope(|s| {
        for _ in 0..threads.min(names.len()) {
            s.spawn(|| {
                while let Some(name) = names.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                    if let Err(e) = ensure(romfs, mod_romfs, name) {
                        errors.lock().unwrap().push(format!("Texture {name}: {e}"));
                    }
                }
            });
        }
    });
    errors.into_inner().unwrap()
}

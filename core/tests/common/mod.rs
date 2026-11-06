//! Shared setup for the tests that read the real game files.
#![allow(dead_code)]

use mkwtrack::catalog::Catalog;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn env_dir(var: &str) -> Option<PathBuf> {
    let Some(p) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} (an absolute path) to run the tests that read the game files");
        return None;
    };
    // cargo runs tests from the crate folder, so a relative path would quietly point elsewhere
    assert!(p.is_absolute() && p.is_dir(), "{var}={} must be an absolute path to a folder", p.display());
    Some(p)
}

pub fn romfs() -> Option<PathBuf> {
    env_dir("MKW_ROMFS")
}

pub fn models() -> Option<PathBuf> {
    env_dir("MKW_MODELS")
}

/// The romfs and its catalog (cached between test runs in the temp folder).
pub fn catalog() -> Option<(PathBuf, Catalog)> {
    let (romfs, models) = (romfs()?, models()?);
    let cat = Catalog::load(&romfs, &models, &std::env::temp_dir().join("mkwtrack-test-cache"), &|m| println!("{m}")).unwrap();
    Some((romfs, cat))
}

/// Every BYML file under `Banc/`, relative to the romfs.
pub fn all_byml(romfs: &Path) -> Vec<String> {
    mkwtrack::io::files_under(&romfs.join("Banc"))
        .into_iter()
        .filter(|f| f.ends_with(".byml") || f.ends_with(".byml.zs"))
        .map(|f| format!("Banc/{f}"))
        .collect()
}

/// The ground surface of one stage model, read from the exported FBX files.
pub fn stage_surface(cat: &Catalog, stage: &str) -> mkwtrack::raycast::Bvh {
    let s = cat.data.stages.iter().find(|s| s.name == stage).unwrap_or_else(|| panic!("no stage {stage}"));
    mkwtrack::scene::stage_bvh(std::slice::from_ref(s), |rel| {
        let d = std::fs::read(cat.models.join(rel)).ok()?;
        mkwtrack::fbx::load_model(&d).ok().map(Arc::new)
    })
}

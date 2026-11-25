//! The game's models and textures, read from the romfs: every model parses, a stage's bounds
//! match the game's ActorInfo, and the albedo textures the models use decode. Needs MKW_ROMFS.

mod common;

use mkwtrack::{bfres, bntx};
use std::collections::BTreeSet;
use std::sync::Mutex;

/// Run `f` over `items` on all cores.
fn par<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = Mutex::new(0usize);
    let out = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        *n += 1;
                        *n - 1
                    };
                    let Some(item) = items.get(i) else { break };
                    let r = f(item);
                    out.lock().unwrap().push(r);
                }
            });
        }
    });
    out.into_inner().unwrap()
}

#[test]
fn stage_bounds_match_actorinfo() {
    let Some(romfs) = common::romfs() else { return };
    let t = std::time::Instant::now();
    let model = bfres::load(&romfs, "Model/Stage_FirstCircuit.Stage_FirstCircuit_A.bfres.zs").unwrap();
    println!(
        "FirstCircuit: {} parts, {} tris, {:?}..{:?} in {:?}",
        model.parts.len(),
        model.triangle_count(),
        model.min,
        model.max,
        t.elapsed()
    );
    // ActorInfo ModelAABB: [-474.46, 0, -362.95] .. [276.10, 211.06, 501.71]
    let close = |a: f32, b: f32| (a - b).abs() < 1.0;
    assert!(close(model.min[0], -474.46) && close(model.min[2], -362.95), "{:?}", model.min);
    assert!(close(model.max[0], 276.10) && close(model.max[1], 211.06) && close(model.max[2], 501.71), "{:?}", model.max);
    assert!(model.triangle_count() > 120_000);
    assert!(model.parts.iter().filter(|p| p.texture.is_some()).count() > 50);
    assert!(model.parts.iter().any(|p| p.cutout) && model.parts.iter().any(|p| !p.cutout));
    let item_box = bfres::load(&romfs, "Model/CmnObj_ItemBox000.CmnObj_ItemBox000_A.bfres.zs").unwrap();
    assert_eq!(item_box.parts[0].texture.as_deref(), Some("CmnObj_ItemBox000_Body_Symbol_Alb"));
}

#[test]
fn every_model_parses_and_its_textures_decode() {
    let Some(romfs) = common::romfs() else { return };
    let t = std::time::Instant::now();
    let files: Vec<String> = mkwtrack::io::files_under(&romfs.join("Model"))
        .into_iter()
        .filter(|f| f.ends_with(".bfres.zs"))
        .map(|f| format!("Model/{f}"))
        .collect();
    let results = par(&files, |rel| (rel.clone(), bfres::load(&romfs, rel)));
    let mut bad = vec![];
    let mut empty = vec![];
    let mut textures = BTreeSet::new();
    for (rel, r) in &results {
        match r {
            Ok(m) if m.parts.is_empty() => empty.push(rel.as_str()),
            Ok(m) => textures.extend(m.parts.iter().filter_map(|p| p.texture.clone())),
            Err(e) => bad.push(format!("{rel}: {e}")),
        }
    }
    empty.sort();
    println!("{} models, {} without geometry, {} failed, in {:?}", files.len(), empty.len(), bad.len(), t.elapsed());
    println!("without geometry, e.g. {:?}", empty.iter().step_by(empty.len().div_ceil(12).max(1)).collect::<Vec<_>>());
    assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(5)]);

    let t = std::time::Instant::now();
    let textures: Vec<String> = textures.into_iter().collect();
    let decoded = par(&textures, |name| {
        if !bntx::texture_path(&romfs, name).exists() {
            return Err(format!("{name}: missing"));
        }
        bntx::load(&romfs, name, 64).map_err(|e| format!("{name}: {e}")).map(|img| img.width > 0 && img.height > 0)
    });
    let missing = decoded.iter().filter(|r| r.as_ref().is_err_and(|e| e.ends_with("missing"))).count();
    let failed: Vec<&String> = decoded.iter().filter_map(|r| r.as_ref().err()).filter(|e| !e.ends_with("missing")).collect();
    println!("{} albedo textures, {} not in Tex/, {} failed, in {:?}", textures.len(), missing, failed.len(), t.elapsed());
    assert!(failed.is_empty(), "{:?}", &failed[..failed.len().min(5)]);
    assert!(missing * 100 < textures.len(), "{missing} textures missing");
}

#[test]
fn texture_decodes_to_the_expected_image() {
    let Some(romfs) = common::romfs() else { return };
    // ASTC 8x8 sRGB, opaque: the item box's "?" symbol is on a light background
    let img = bntx::load(&romfs, "CmnObj_ItemBox000_Body_Symbol_Alb", 4096).unwrap();
    assert!(img.width >= 64 && img.height >= 64);
    assert!(!img.has_alpha());
    let mean: u64 =
        img.rgba.as_chunks::<4>().0.iter().map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum::<u64>() / (img.rgba.len() as u64 / 4);
    println!("{}x{}, mean RGB sum {mean}", img.width, img.height);
    assert!(mean > 60, "decoded image is nearly black: is the untiling wrong?");
    // smaller mips are picked by size
    let small = bntx::load(&romfs, "CmnObj_ItemBox000_Body_Symbol_Alb", 64).unwrap();
    assert!(small.width <= 64 && small.height <= 64 && small.width * 2 > 64, "{}x{}", small.width, small.height);
}

#[test]
fn open_world_objects_sit_on_the_terrain() {
    use mkwtrack::{byml, raycast::Bvh, terrain};
    let Some(romfs) = common::romfs() else { return };
    let rel = "Banc/World_Game03/0707.bcett.byml.zs";
    let (doc, _) = byml::from_bytes(&mkwtrack::io::read_file(&romfs.join(rel)).unwrap()).unwrap();
    let pts: Vec<[f32; 3]> = doc.array("Actors").iter().map(|a| a.vec3("Translate", [0.0; 3])).collect();
    let region = terrain::region_around(&pts, 50.0).unwrap();
    let t = std::time::Instant::now();
    let model = terrain::load(&romfs, region, &pts).unwrap();
    let tris =
        model.parts.iter().flat_map(|p| p.indices.as_chunks::<3>().0.iter().map(|t| t.map(|i| p.positions[i as usize].into()))).collect();
    let ground = Bvh::new(tris);
    let near = |tol: f32| {
        pts.iter()
            .filter(|p| ground.ground_below(glam::Vec3::from(**p) + glam::Vec3::Y * 0.5).is_some_and(|y| (y - p[1]).abs() < tol))
            .count()
    };
    let (on, total) = (near(0.15), pts.len());
    println!("{rel}: {} tris in {:?}; {on} of {total} objects within 0.15 m ({} within 1 m)", ground.len(), t.elapsed(), near(1.0));
    // objects on stage models, bridges or in the air make up the rest
    assert!(on * 4 > total, "{on} of {total}");
}

#[test]
fn gridify_pieces_support_routes() {
    use mkwtrack::{byml, catalog::parse_gridify_cell, raycast::Bvh};
    let Some(romfs) = common::romfs() else { return };
    for (course, min_hits) in [
        ("Banc/InterStageEx1_AirshipFortressToLava_A_00.bcett.byml.zs", 150),
        ("Banc/InterStageEx2_CinemaToDinoBones_A_00.bcett.byml.zs", 350),
    ] {
        let (doc, _) = byml::from_bytes(&mkwtrack::io::read_file(&romfs.join(course)).unwrap()).unwrap();
        let pts: Vec<glam::Vec3> = doc
            .array("Rails")
            .iter()
            .filter(|r| r.str_or("Name", "") == "RoutePathGraph")
            .flat_map(|r| r.array("Points"))
            .map(|p| glam::Vec3::from(p.vec3("Translation", [0.0; 3])))
            .collect();
        let files: Vec<String> = mkwtrack::io::files_under(&romfs.join("Model"))
            .into_iter()
            .filter(|f| f.starts_with("Piece_Gridify_Game03_") && f.contains(".Gridify_Game03_") && f.ends_with(".bfres.zs"))
            .collect();
        let mut tris = vec![];
        for f in &files {
            let Some(stem) = f.split('.').nth(1) else { continue };
            let Some(t) = parse_gridify_cell(stem) else { continue };
            if !pts.iter().any(|p| (p.x - t[0]).abs() < 300.0 && (p.z - t[2]).abs() < 300.0) {
                continue;
            }
            let m = bfres::load(&romfs, &format!("Model/{f}")).unwrap();
            let off = glam::Vec3::from(t);
            for part in &m.parts {
                for tri in part.indices.as_chunks::<3>().0 {
                    tris.push([0, 1, 2].map(|k| glam::Vec3::from(part.positions[tri[k] as usize]) + off));
                }
            }
        }
        let bvh = Bvh::new(tris);
        let hits = pts.iter().filter(|p| bvh.ground_below(**p + glam::Vec3::Y * 2.0).is_some_and(|y| (y - p.y).abs() < 2.0)).count();
        assert!(hits >= min_hits, "{course}: expected >= {min_hits} route points on gridify pieces, got {hits}");
    }
}

//! The exported FBX models: every one parses, and a stage's bounds match the game's ActorInfo.
//! Needs MKW_MODELS (an absolute path to the Switch Toolbox export's `Models` folder).

mod common;

use mkwtrack::fbx;

#[test]
fn stage_bounds_match_actorinfo() {
    let Some(m) = common::models() else { return };
    let t = std::time::Instant::now();
    let d = std::fs::read(m.join("Stage/Stage_FirstCircuit.Stage_FirstCircuit_A/Stage_FirstCircuit_A.fbx")).unwrap();
    let model = fbx::load_model(&d).unwrap();
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
}

#[test]
fn every_exported_model_parses() {
    let Some(m) = common::models() else { return };
    let fbxs: Vec<_> = mkwtrack::io::files_under(&m).into_iter().filter(|f| f.ends_with(".fbx")).collect();
    let mut bad = vec![];
    let mut empty = 0;
    for rel in &fbxs {
        match std::fs::read(m.join(rel)).map_err(mkwtrack::Error::from).and_then(|d| fbx::load_model(&d)) {
            Ok(model) if model.parts.is_empty() => empty += 1,
            Ok(_) => {}
            Err(e) => bad.push(format!("{rel}: {e}")),
        }
    }
    println!("{} FBX files, {} without geometry, {} failed", fbxs.len(), empty, bad.len());
    assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(5)]);
}

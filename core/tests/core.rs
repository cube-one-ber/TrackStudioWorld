//! Checks against the real game files. They need your own dump: set MKW_ROMFS to an absolute
//! path. Without them each test prints why it skipped and passes.

mod common;

use common::{catalog, romfs};
use mkwtrack::banc;
use mkwtrack::byml;
use mkwtrack::catalog as cat;
use mkwtrack::io::read_file;
use mkwtrack::restbl::{ResourceSizeTable, required_size};
use mkwtrack::typedjson;

#[test]
fn byml_writer_and_typed_json_round_trip() {
    let Some(romfs) = romfs() else { return };
    let files = common::all_byml(&romfs);
    let (mut same, mut semantic) = (0, 0);
    for rel in &files {
        let data = read_file(&romfs.join(rel)).unwrap();
        let (root, ver) = byml::from_bytes(&data).unwrap();
        let pow2 = data.len().is_power_of_two();
        let out = byml::to_bytes(&root, ver, pow2).unwrap();
        // typed JSON text round trip must give the same tree
        let text = serde_json::to_string(&typedjson::to_json(&root)).unwrap();
        let back = typedjson::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(byml::to_bytes(&back, ver, pow2).unwrap(), out, "typed JSON changed {rel}");
        if out == data {
            same += 1;
        } else {
            // Banc/PathByml comes from another Nintendo writer that puts binary blobs first
            assert!(rel.contains("PathByml"), "not byte-identical: {rel}");
            assert_eq!(byml::from_bytes(&out).unwrap().0, root, "content differs: {rel}");
            semantic += 1;
        }
    }
    println!("byte-identical {same}, identical content {semantic}");
    assert_eq!(same + semantic, files.len());
}

#[test]
fn size_table_rewrites_identically_and_formula_matches() {
    let Some(romfs) = romfs() else { return };
    let dir = romfs.join("System/Resource");
    let f = std::fs::read_dir(&dir).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".rsizetable.zs")).unwrap();
    let raw = read_file(&f.path()).unwrap();
    let t = ResourceSizeTable::parse(&raw).unwrap();
    assert_eq!(t.to_bytes(), raw);
    let files = common::all_byml(&romfs);
    let hits = files.iter().filter(|rel| t.get(rel) == Some(required_size(rel, read_file(&romfs.join(rel)).unwrap().len()))).count();
    assert_eq!(hits, files.len());
}

#[test]
fn recompute_leaves_shipped_files_unchanged() {
    let Some((romfs, cat)) = catalog() else { return };
    assert!(cat.data.collects.exact.values().all(|&e| e), "{:?}", cat.data.collects.exact);
    assert_eq!(cat.data.collects.exact.len(), 5);
    let docs = cat::parse_all(&romfs, &cat::banc_files(&romfs));
    for (rel, doc) in &docs {
        let root = doc.as_ref().unwrap();
        let mut d = root.clone();
        let notes = banc::recompute(&mut d, Some(root), Some(&cat));
        let data = read_file(&romfs.join(rel)).unwrap();
        assert_eq!(byml::to_bytes(&d, 7, true).unwrap(), data, "recompute changed {rel}: {notes:?}");
        assert!(banc::validate(root).errors.is_empty(), "{rel} fails validation");
    }
    println!("recompute unchanged: {}", docs.len());
    let ib = &cat.data.actors["CmnObj_ItemBox000_A_00"];
    assert!(ib.model.is_some() && ib.banc_offset == Some([0.0, 1.5, 0.0]));
    assert_eq!(cat.data.stages.len(), 38 + 10);
}

#[test]
fn route_points_have_path_data() {
    // New route points get a copy of a neighbour's PathAdditionalData entry (Document::new_rail,
    // point inserts) because shipped courses give route points one and other points none.
    let Some(romfs) = romfs() else { return };
    let docs = cat::parse_all(&romfs, &cat::banc_files(&romfs));
    let (mut with, mut without, mut other) = (0, 0, 0);
    let mut files_without = std::collections::BTreeSet::new();
    for (rel, doc) in &docs {
        let Ok(pad) = read_file(&romfs.join(banc::pad_rel(rel))) else { continue };
        let Ok((byml::Byml::Hash64(pad), _)) = byml::from_bytes(&pad) else { continue };
        for r in doc.as_ref().unwrap().array("Rails") {
            for p in r.array("Points") {
                let has = pad.contains_key(&banc::hash_of(p));
                match (r.str_or("Name", "") == "RoutePathGraph", has) {
                    (true, true) => with += 1,
                    (true, false) => {
                        without += 1;
                        files_without.insert(rel.clone());
                    }
                    (false, true) => other += 1,
                    (false, false) => {}
                }
            }
        }
    }
    println!(
        "route points with path data {with}, without {without} (in {} files: {files_without:?}); other points with it {other}",
        files_without.len()
    );
    // 1.8.0: 227,255 with, 855 without (in 14 files), none on other path types
    assert!(with * 100 > (with + without) * 99, "most route points have path data");
    assert_eq!(other, 0, "only route points have path data");
}

#[test]
fn check_shipped_courses() {
    // The Problems panel's warnings describe what no shipped course does, so the shipped courses
    // must give none (src/check.rs).
    let Some((romfs, cat)) = catalog() else { return };
    let docs = cat::parse_all(&romfs, &cat::banc_files(&romfs));
    let mut warned = vec![];
    for (rel, doc) in &docs {
        for p in mkwtrack::check::check(doc.as_ref().unwrap(), rel, Some(&cat)) {
            assert_ne!(p.severity, mkwtrack::check::Severity::Error, "{rel}: {}", p.message);
            if p.severity == mkwtrack::check::Severity::Warning {
                warned.push(format!("{rel}: {}", p.message));
            }
        }
    }
    println!("{} courses checked; warnings:\n{}", docs.len(), warned.join("\n"));
    assert!(warned.is_empty(), "{warned:#?}");
}

#[test]
fn shipped_checkpoints_keep_their_numbers() {
    // the numbering rule (src/checkpoints.rs) reproduces every shipped CheckPoint, so renumbering
    // leaves every shipped file as it is
    let Some(romfs) = romfs() else { return };
    let (mut files, mut cps) = (0, 0);
    for rel in common::all_byml(&romfs).iter().filter(|f| f.ends_with(".bcett.byml.zs")) {
        let (mut root, _) = byml::from_bytes(&read_file(&romfs.join(rel)).unwrap()).unwrap();
        let n = mkwtrack::checkpoints::numbers(root.array("Rails")).len();
        let total = root
            .array("Rails")
            .iter()
            .filter(|r| r.str_or("Name", "") == mkwtrack::checkpoints::LAP)
            .flat_map(|r| r.array("Points"))
            .filter(|p| mkwtrack::checkpoints::is_checkpoint(p))
            .count();
        assert_eq!(n, total, "{rel}: every checkpoint has a number");
        assert_eq!(mkwtrack::checkpoints::renumber(&mut root), 0, "{rel}: renumbering changed it");
        files += (total > 0) as usize;
        cps += total;
    }
    println!("{cps} checkpoints in {files} files keep their numbers");
    assert!(cps > 5000, "only {cps} checkpoints found");
}

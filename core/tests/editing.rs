//! End-to-end editing on Mario Bros. Circuit: every operation, save as a mod, then check the
//! written files independently. Needs MKW_ROMFS and MKW_MODELS (absolute paths).

mod common;

use glam::Vec3;
use mkwtrack::banc::hash_of;
use mkwtrack::byml::{self, Byml, vec3};
use mkwtrack::catalog::Catalog;
use mkwtrack::doc::Document;
use mkwtrack::io::read_file;
use mkwtrack::math::quat_zyx;
use mkwtrack::raycast::{Bvh, Ray};
use mkwtrack::restbl::ResourceSizeTable;
use mkwtrack::scene;
use mkwtrack::workspace::{Loaded, Workspace, ns2jb_romfs};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

const FILE: &str = "Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs";

struct Env {
    romfs: PathBuf,
    cat: Catalog,
}

fn env() -> Option<Env> {
    let (romfs, cat) = common::catalog()?;
    Some(Env { romfs, cat })
}

fn stage_bvh(e: &Env) -> Bvh {
    common::stage_surface(&e.cat, "Stage_FirstCircuit_A_00")
}

fn check(ok: &mut bool, cond: bool, msg: &str) {
    println!("{} {msg}", if cond { "PASS" } else { "FAIL" });
    *ok &= cond;
}

#[test]
fn edit_save_and_verify() {
    let Some(e) = env() else { return };
    let mods = std::env::temp_dir().join(format!("mkwtrack-mods-{}", std::process::id()));
    let ws = Workspace { romfs: e.romfs.clone(), mods_root: mods.clone() };
    let mut doc = Document::new(ws.load(FILE, None).unwrap());
    let bvh = stage_bvh(&e);
    println!("stage BVH: {} triangles", bvh.len());

    // the stage surface is where the shipped objects say it is
    let ib = doc.actors().iter().find(|a| a.str_or("Name", "") == "CmnObj_ItemBox000_A_00").unwrap().clone();
    let t = Vec3::from(vec3(ib.get("Translate"), [0.0; 3]));
    let above = t.y - bvh.ground_below(t).unwrap();
    let mut ok = true;
    check(&mut ok, (above - 1.5).abs() < 0.1, &format!("shipped item box sits {above:.2} m above the stage (BancTransOffset 1.5)"));

    // a. move an item box +5 m x
    let moved = hash_of(&ib);
    let (p, r, s) = doc.transform(moved).unwrap();
    doc.push_undo();
    doc.set_transform(moved, p + Vec3::X * 5.0, r, s);
    // b. duplicate it
    let dup = doc.duplicate(&[moved], Vec3::new(3.0, 0.0, 0.0))[0];
    // c. delete one coin
    let coin = hash_of(doc.actors().iter().find(|a| a.str_or("Name", "") == "CmnObj_Coin000_A_00").unwrap());
    doc.delete(&[coin]);
    // d. add a new item box, then drop it to the ground (keeps its 1.5 m offset)
    let added = doc.add_actor(&e.cat, "CmnObj_ItemBox000_A_00", t + Vec3::new(0.0, 20.0, 8.0), 0.5).unwrap();
    let (n, total) = doc.drop_to_ground(&[added], &e.cat, &|p| bvh.ground_below(p));
    check(&mut ok, n == 1 && total == 1, "drop to ground placed the new item box");
    // e. insert a lap point after point 3 of the first lap path
    let lap = doc.rails().iter().find(|r| r.str_or("Name", "") == "LapPathGraph").unwrap().clone();
    let lap_hash = hash_of(&lap);
    let before = Document::points(&lap).len();
    let p3 = hash_of(&Document::points(&lap)[3]);
    let ins = doc.insert_point_after(p3).unwrap();
    // f. delete the first point of a route path that other points link into
    let mut incoming: HashMap<u64, usize> = HashMap::new();
    for r in doc.rails() {
        for p in Document::points(r) {
            for l in p.get("Links").and_then(|x| x.as_array()).into_iter().flatten() {
                *incoming.entry(l.get("Dst").unwrap().as_u64().unwrap()).or_default() += 1;
            }
        }
    }
    let pad = doc.pad.clone().unwrap();
    let Byml::Hash64(padm) = &pad else { panic!() };
    let route = doc
        .rails()
        .iter()
        .find(|r| {
            let pts = Document::points(r);
            r.str_or("Name", "") == "RoutePathGraph"
                && pts.len() > 3
                && incoming.contains_key(&hash_of(&pts[0]))
                && padm.contains_key(&hash_of(&pts[0]))
        })
        .unwrap()
        .clone();
    let (first, second) = (hash_of(&Document::points(&route)[0]), hash_of(&Document::points(&route)[1]));
    let n_in = incoming[&first];
    doc.delete(&[first]);
    // g. new route path, link another route's last point to it
    doc.new_rail(&e.cat, "RoutePathGraph", t + Vec3::new(10.0, 0.0, 0.0), 0.3).unwrap();
    let new_rail = doc.rails().last().unwrap().clone();
    let new_first = hash_of(&Document::points(&new_rail)[0]);
    let src_rail = doc
        .rails()
        .iter()
        .find(|r| r.str_or("Name", "") == "RoutePathGraph" && hash_of(r) != hash_of(&new_rail) && hash_of(r) != hash_of(&route))
        .unwrap()
        .clone();
    let src = hash_of(Document::points(&src_rail).last().unwrap());
    doc.add_link(src, hash_of(&new_rail), "BasicSignal").unwrap();
    // h. undo + redo the link
    let snapshot = byml::to_bytes(&doc.root, 7, false).unwrap();
    doc.undo();
    let has_link = |d: &Document| {
        d.find(src)
            .unwrap()
            .1
            .get("Links")
            .and_then(|l| l.as_array())
            .is_some_and(|ls| ls.iter().any(|l| l.get("Dst").unwrap().as_u64() == Some(new_first)))
    };
    check(&mut ok, !has_link(&doc), "undo removed the link");
    doc.redo();
    check(&mut ok, has_link(&doc) && byml::to_bytes(&doc.root, 7, false).unwrap() == snapshot, "redo restored the exact document");

    // i. save
    let res = ws.save(&doc.file, &doc.root, doc.pad.as_ref(), "E2ETest", &doc.deleted, Some(&e.cat));
    println!("save: {:?}", res);
    let pad_rel = "Banc/PathAdditionalData/Race_FirstCircuit_A_00.byml";
    check(
        &mut ok,
        res.ok && res.written.iter().any(|w| w == FILE) && res.written.iter().any(|w| w == pad_rel),
        "saved course + path additional data",
    );

    // ---- verify the written files
    let mr = mods.join("E2ETest/romfs");
    let orig = byml::from_bytes(&read_file(&e.romfs.join(FILE)).unwrap()).unwrap().0;
    let raw = read_file(&mr.join(FILE)).unwrap();
    check(&mut ok, raw.len().is_power_of_two(), "decompressed size is a power of two");
    let new = byml::from_bytes(&raw).unwrap().0;
    let nd = Document::new(Loaded { file: FILE.into(), from_mod: true, version: 7, doc: new.clone(), pad: None, pad_file: None });
    let actors: HashMap<u64, &Byml> = nd.actors().iter().map(|a| (hash_of(a), a)).collect();
    check(&mut ok, nd.actors().len() == doc_len(&orig, "Actors") + 1, "objects +dup +add -coin");
    check(&mut ok, vec3(actors[&moved].get("Translate"), [0.0; 3])[0] == p.x + 5.0, "moved item box has its new position");
    check(
        &mut ok,
        actors.contains_key(&dup) && actors.contains_key(&added) && !actors.contains_key(&coin),
        "duplicate and added present, coin gone",
    );
    let ca = new.get("CountActor").unwrap();
    check(
        &mut ok,
        ca.get("CmnObj_ItemBox000_A_00").unwrap().as_f64()
            == orig.get("CountActor").unwrap().get("CmnObj_ItemBox000_A_00").unwrap().as_f64().map(|x| x + 2.0),
        "CountActor item boxes +2",
    );
    check(&mut ok, new.get("Collects").unwrap().get("Coin").unwrap().as_f64() == Some(23.0), "Collects.Coin 24 -> 23");
    let added_a = actors[&added];
    check(
        &mut ok,
        matches!(added_a.get("Hash"), Some(Byml::U64(_)))
            && matches!(added_a.get("SRTHash"), Some(Byml::U32(_)))
            && matches!(added_a.get("IsGridify"), Some(Byml::Bool(_))),
        "new object keeps BYML types",
    );
    let at = Vec3::from(vec3(added_a.get("Translate"), [0.0; 3]));
    check(&mut ok, (at.y - bvh.ground_below(at).unwrap() - 1.5).abs() < 0.05, "dropped item box floats 1.5 m like the shipped ones");
    let lap_new = nd.find(lap_hash).unwrap().1;
    check(
        &mut ok,
        Document::points(lap_new).len() == before + 1 && hash_of(&Document::points(lap_new)[4]) == ins,
        "lap point inserted at index 4",
    );
    let insp = &Document::points(lap_new)[4];
    check(
        &mut ok,
        insp.get("Parent").unwrap().as_u64() == Some(lap_hash) && insp.get("Dynamic").and_then(|d| d.get("CheckPoint")).is_none(),
        "inserted point: Parent set, CheckPoint not copied",
    );
    check(&mut ok, nd.find(first).is_none(), "deleted route point gone");
    let into_second = nd
        .rails()
        .iter()
        .flat_map(Document::points)
        .flat_map(|p| p.get("Links").and_then(|l| l.as_array()).cloned().unwrap_or_default())
        .filter(|l| l.get("Dst").unwrap().as_u64() == Some(second))
        .count();
    check(&mut ok, into_second >= n_in, &format!("links into the deleted first point retargeted ({into_second})"));
    let nr = nd.find(hash_of(&new_rail)).unwrap().1;
    check(
        &mut ok,
        Document::points(nr).len() == 2
            && Document::points(nr).iter().all(|p| p.get("Parent").unwrap().as_u64() == Some(hash_of(&new_rail))),
        "new path: 2 points, Parent = path",
    );
    let all: Vec<u64> = nd.index.keys().copied().collect();
    check(&mut ok, all.len() == all.iter().collect::<HashSet<_>>().len(), &format!("{} hashes, all unique", all.len()));
    let oa: HashMap<u64, &Byml> = orig.get("Actors").unwrap().as_array().unwrap().iter().map(|a| (hash_of(a), a)).collect();
    let same = oa.iter().filter(|(k, a)| actors.get(k) == Some(a)).count();
    check(&mut ok, same == oa.len() - 2, &format!("{same} of {} shipped objects unchanged (1 moved, 1 deleted)", oa.len()));
    let pad_n = byml::from_bytes(&read_file(&mr.join(pad_rel)).unwrap()).unwrap().0;
    let Byml::Hash64(pn) = &pad_n else { panic!("PAD type") };
    check(&mut ok, !pn.contains_key(&first), "PAD entry of the deleted point pruned");
    // the new route's two points copy a neighbour's entry, as shipped route points have one
    let new_pts: Vec<u64> = Document::points(nr).iter().map(hash_of).collect();
    check(
        &mut ok,
        new_pts.iter().all(|h| pn.contains_key(h)) && pn.len() == padm.len() - 1 + new_pts.len(),
        "new route points have path data",
    );
    // the size table in effect (the mod's copy if it wrote one) covers both files; the course
    // file still fits its shipped entry
    let dir = e.romfs.join("System/Resource");
    let table_name = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|f| f.file_name().to_string_lossy().into_owned())
        .find(|f| f.ends_with(".rsizetable.zs"))
        .unwrap();
    let table_rel = format!("System/Resource/{table_name}");
    let shipped = ResourceSizeTable::parse(&read_file(&e.romfs.join(&table_rel)).unwrap()).unwrap();
    let in_mod = res.written.contains(&table_rel);
    let table = if in_mod { ResourceSizeTable::parse(&read_file(&mr.join(&table_rel)).unwrap()).unwrap() } else { shipped.clone() };
    let need = |rel: &str| mkwtrack::restbl::required_size(rel, read_file(&mr.join(rel)).unwrap().len());
    check(
        &mut ok,
        table.get(pad_rel) >= Some(need(pad_rel)) && table.get(FILE) >= Some(need(FILE)) && table.get(FILE) == shipped.get(FILE),
        &format!("size table covers the saved files (written to the mod: {in_mod})"),
    );

    // ---- scene + picking
    let sel: HashSet<u64> = HashSet::new();
    let sc = scene::build(&nd, &e.cat, &scene::ViewOptions { models: true, ..Default::default() }, &|_| true, &sel);
    let ray = Ray { origin: at + Vec3::Y * 50.0, dir: Vec3::NEG_Y };
    check(&mut ok, scene::pick(&sc, &ray) == Some(added), "picking from above selects the item box");
    let lp = Vec3::from(vec3(Document::points(lap_new)[2].get("Translation"), [0.0; 3]));
    let ray = Ray { origin: lp + Vec3::new(0.0, 30.0, 0.0), dir: Vec3::NEG_Y };
    check(&mut ok, scene::pick(&sc, &ray) == Some(hash_of(&Document::points(lap_new)[2])), "picking a lap point");
    check(
        &mut ok,
        sc.groups.contains_key(e.cat.data.actors["CmnObj_ItemBox000_A_00"].model.as_ref().unwrap()),
        "item boxes grouped for instancing",
    );

    std::fs::remove_dir_all(&mods).ok();
    assert!(ok);
}

fn doc_len(d: &Byml, k: &str) -> usize {
    d.get(k).and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0)
}

#[test]
fn restbl_growth_and_refusals() {
    let Some(e) = env() else { return };
    let mods = std::env::temp_dir().join(format!("mkwtrack-mods-g{}", std::process::id()));
    let ws = Workspace { romfs: e.romfs.clone(), mods_root: mods.clone() };
    let mut doc = Document::new(ws.load(FILE, None).unwrap());
    let coin = hash_of(doc.actors().iter().find(|a| a.str_or("Name", "") == "CmnObj_Coin000_A_00").unwrap());
    let mut sel = vec![coin];
    for _ in 0..11 {
        let mut more = doc.duplicate(&sel, Vec3::new(0.1, 0.0, 0.0));
        sel.append(&mut more);
    }
    let res = ws.save(&doc.file, &doc.root, doc.pad.as_ref(), "GrowTest", &doc.deleted, Some(&e.cat));
    let mut ok = true;
    check(&mut ok, res.ok, "grown file saves");
    let mr = mods.join("GrowTest/romfs");
    let size = read_file(&mr.join(FILE)).unwrap().len();
    let rt = res.written.iter().find(|w| w.starts_with("System/Resource/")).cloned();
    check(&mut ok, size == 524288 && rt.is_some(), &format!("grew to {size} bytes and wrote the size table"));
    if let Some(rt) = rt {
        let t = ResourceSizeTable::parse(&read_file(&mr.join(&rt)).unwrap()).unwrap();
        let shipped = ResourceSizeTable::parse(&read_file(&e.romfs.join(&rt)).unwrap()).unwrap();
        let diff = shipped.crc.iter().filter(|(h, v)| t.crc.get(h) != Some(v)).count();
        check(&mut ok, t.get(FILE) == Some(524288 + 1024) && diff == 1, "only this file's RESTBL entry changed, to size + 1024");
    }
    // duplicate hash refused, nothing written
    let mut bad = doc.root.clone();
    let first = bad.get("Actors").unwrap().as_array().unwrap()[0].clone();
    bad.get_mut("Actors").unwrap().as_array_mut().unwrap().push(first);
    let r2 = ws.save(FILE, &bad, None, "DupTest", &HashSet::new(), Some(&e.cat));
    check(
        &mut ok,
        !r2.ok && r2.errors.iter().any(|x| x.contains("duplicate Hash")) && !mods.join("DupTest").exists(),
        "duplicate hash refused, nothing written",
    );
    check(&mut ok, !ws.save("../../etc/passwd", &bad, None, "X", &HashSet::new(), None).ok, "non-Banc path refused");
    check(&mut ok, !ws.save(FILE, &doc.root, None, "../escape", &HashSet::new(), None).ok, "bad mod name refused");
    // reload from the mod
    let l = ws.load(FILE, Some("GrowTest")).unwrap();
    check(&mut ok, l.from_mod && doc_len(&l.doc, "Actors") == doc.actors().len(), "loading with the mod returns the edited file");
    // reverse a path twice gives the original back
    let rail = hash_of(doc.rails().iter().find(|r| r.str_or("Name", "") == "LapPathGraph").unwrap());
    let before = byml::to_bytes(doc.find(rail).unwrap().1, 7, false).unwrap();
    doc.reverse_rail(rail);
    doc.reverse_rail(rail);
    let after = doc.find(rail).unwrap().1;
    let ok_rev = Document::points(after).iter().zip(Document::points(&byml::from_bytes(&before).unwrap().0)).all(|(a, b)| {
        let (qa, qb) = (quat_zyx(a.vec3("Rotate", [0.0; 3])), quat_zyx(b.vec3("Rotate", [0.0; 3])));
        qa.dot(qb).abs() > 0.9999 && a.get("Translation") == b.get("Translation")
    });
    check(&mut ok, ok_rev, "reversing a path twice restores it");
    std::fs::remove_dir_all(&mods).ok();
    assert!(ok);
}

#[test]
fn ns2jb_export_merges_into_the_sd_layout() {
    let Some(e) = env() else { return };
    let tmp = std::env::temp_dir().join(format!("mkwtrack-ns2jb-{}", std::process::id()));
    let ws = Workspace { romfs: e.romfs.clone(), mods_root: tmp.join("mods") };
    let mut doc = Document::new(ws.load(FILE, None).unwrap());
    // grow the course past its size table entry, so the export has to carry the table too
    let coin = hash_of(doc.actors().iter().find(|a| a.str_or("Name", "") == "CmnObj_Coin000_A_00").unwrap());
    let mut sel = vec![coin];
    for _ in 0..11 {
        let mut more = doc.duplicate(&sel, Vec3::new(0.1, 0.0, 0.0));
        sel.append(&mut more);
    }
    let saved = ws.save(&doc.file, &doc.root, doc.pad.as_ref(), "Ns2jbTest", &doc.deleted, Some(&e.cat));
    let mut ok = true;
    check(&mut ok, saved.ok, "mod saves");
    let rt = saved.written.iter().find(|w| w.starts_with("System/Resource/")).cloned().unwrap();

    // another mod already on the card raised a different entry
    let sd = tmp.join("sd");
    let romfs = sd.join("ns2jb-modloader/contents/0400c3f00006e000/romfs");
    let other = "Banc/Race_Cave_A_00[Game03].bcett.byml.zs";
    let mut table = ResourceSizeTable::parse(&read_file(&e.romfs.join(&rt)).unwrap()).unwrap();
    table.set(other, 9_999_999);
    std::fs::create_dir_all(romfs.join(&rt).parent().unwrap()).unwrap();
    std::fs::write(romfs.join(&rt), mkwtrack::io::compress(&table.to_bytes()).unwrap()).unwrap();

    let res = ws.export_ns2jb("Ns2jbTest", &sd);
    check(&mut ok, res.ok && res.mod_romfs == romfs, &format!("export ok into {}", res.mod_romfs.display()));
    check(
        &mut ok,
        std::fs::read(romfs.join(FILE)).ok() == std::fs::read(tmp.join("mods/Ns2jbTest/romfs").join(FILE)).ok(),
        "course file copied unchanged",
    );
    let pad = res.written.iter().any(|w| w.starts_with("Banc/PathAdditionalData/") && romfs.join(w).is_file());
    check(&mut ok, pad, "path data copied");
    let merged = ResourceSizeTable::parse(&read_file(&romfs.join(&rt)).unwrap()).unwrap();
    check(
        &mut ok,
        merged.get(FILE) == Some(524288 + 1024) && merged.get(other) == Some(9_999_999),
        "size table keeps the other mod's entry and adds this one",
    );
    // ns2jb-modloader rejects anything but romfs/exefs (and a few known files) in the title folder
    let title: Vec<_> = std::fs::read_dir(romfs.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    check(&mut ok, title == ["romfs"], "only romfs/ in the title folder");
    // the SD root, its ns2jb-modloader folder and contents/ all resolve to the same place
    for p in [sd.clone(), sd.join("ns2jb-modloader"), sd.join("ns2jb-modloader/contents")] {
        check(&mut ok, ns2jb_romfs(&p) == romfs, &format!("{} resolves", p.display()));
    }
    check(&mut ok, !ws.export_ns2jb("NotSaved", &sd).ok, "exporting an unsaved mod is refused");
    std::fs::remove_dir_all(&tmp).ok();
    assert!(ok);
}

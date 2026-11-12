//! Checks of the baked collision files against the real game files. They need your own dump:
//! set MKW_ROMFS to an absolute path. Without it each test prints why it skipped and passes.

mod common;

use common::{catalog, romfs};
use mkwtrack::banc::hash_of;
use mkwtrack::byml::Byml;
use mkwtrack::catalog as cat;
use mkwtrack::collision::{self, Physics, Pose};
use mkwtrack::io::{files_under, read_file};
use mkwtrack::phive::{self, StaticCompound};
use mkwtrack::restbl::{ResourceSizeTable, required_collision_size};
use mkwtrack::workspace::Workspace;
use std::collections::HashSet;
use std::path::Path;

/// Every collision file of the courses, missions and stages, relative to the romfs. The open
/// world's tiles are sampled (there are 20,480 of them).
fn collision_files(romfs: &Path) -> Vec<String> {
    let all = files_under(&romfs.join("Phive/StaticCompoundBody"));
    let mut world = 0;
    all.into_iter()
        .filter(|f| f.ends_with(".bphsc.zs"))
        .filter(|f| {
            if !f.starts_with("World_") {
                return true;
            }
            world += 1;
            world % 16 == 0
        })
        .map(|f| format!("Phive/StaticCompoundBody/{f}"))
        .collect()
}

fn each(romfs: &Path, f: impl Fn(&str, &[u8]) + Sync) -> usize {
    let files = collision_files(romfs);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    std::thread::scope(|s| {
        for chunk in files.chunks(files.len().div_ceil(threads).max(1)) {
            let f = &f;
            s.spawn(move || {
                for rel in chunk {
                    f(rel, &read_file(&romfs.join(rel)).unwrap());
                }
            });
        }
    });
    files.len()
}

#[test]
fn shipped_files_rewrite_identically() {
    let Some(romfs) = romfs() else { return };
    let n = each(&romfs, |rel, data| {
        let sc = StaticCompound::parse(data).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(sc.to_bytes().unwrap() == data, "not byte-identical: {rel}");
    });
    println!("{n} collision files rewrite byte-identically");
    assert!(n > 1000);
}

#[test]
fn shipped_trees_and_boxes_follow_the_rules() {
    let Some(romfs) = romfs() else { return };
    let counts = std::sync::Mutex::new((0usize, 0usize));
    each(&romfs, |rel, data| {
        let sc = StaticCompound::parse(data).unwrap();
        sc.check_trees().unwrap_or_else(|e| panic!("{rel}: {e}"));
        sc.check_reachable().unwrap_or_else(|e| panic!("{rel}: {e}"));
        let (exact, all) = sc.check_bounds(1e-5).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let mut c = counts.lock().unwrap();
        (c.0, c.1) = (c.0 + exact, c.1 + all);
    });
    let (exact, all) = *counts.lock().unwrap();
    println!("{exact} of {all} instance boxes computed exactly; the rest cover the stored box");
    assert!(exact * 10 > all * 9, "{exact} of {all}");
}

#[test]
fn size_table_formula_matches_every_collision_file() {
    let Some(romfs) = romfs() else { return };
    let dir = romfs.join("System/Resource");
    let f = std::fs::read_dir(&dir).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".rsizetable.zs")).unwrap();
    let table = ResourceSizeTable::parse(&read_file(&f.path()).unwrap()).unwrap();
    let n = each(&romfs, |rel, data| {
        let need = required_collision_size(data.len(), phive::body_count_of(data).unwrap());
        assert_eq!(table.get(rel), Some(need), "{rel}");
    });
    println!("{n} size table entries match");
}

#[test]
fn unedited_courses_leave_their_collision_alone() {
    let Some((romfs, _)) = catalog() else { return };
    let ws = Workspace { romfs: romfs.clone(), mods_root: std::env::temp_dir().join("mkwtrack-collision-none") };
    let courses: Vec<String> = cat::banc_files(&romfs).into_iter().filter(|r| r.matches('/').count() == 1).collect();
    let (mut files, mut other_frame) = (0, 0);
    let mut scan = std::time::Duration::ZERO;
    for (rel, doc) in cat::parse_all(&romfs, &courses) {
        let doc = doc.unwrap();
        let poses = Pose::all(&doc);
        let started = std::time::Instant::now();
        let found = ws.collision_files(&rel, &doc);
        scan += started.elapsed();
        for crel in found {
            let data = read_file(&romfs.join(&crel)).unwrap();
            let mut sc = StaticCompound::parse(&data).unwrap();
            let changes =
                collision::sync(&mut sc, &poses, &poses, &|_| Physics::None, &mut |_| None).unwrap_or_else(|e| panic!("{crel}: {e}"));
            assert!(!changes.any(), "{crel}: {changes:?}");
            assert!(sc.to_bytes().unwrap() == data, "{crel}");
            files += 1;
            // files in another frame: the first baked object is not where the course has it
            let first = sc.links.iter().find(|l| !l.parts.is_empty() && poses.iter().any(|p| p.hash == l.hash));
            if let Some(l) = first {
                let p = poses.iter().find(|p| p.hash == l.hash).unwrap();
                other_frame += (l.parts[0].instance.translation.distance(p.translate.into()) > 1.0) as usize;
            }
        }
    }
    println!("{files} collision files belong to a course; {other_frame} hold it in another frame");
    println!("finding a course's collision files takes {:?} on average", scan / courses.len() as u32);
    assert!(files > 300 && other_frame > 20);
}

fn actor(doc: &mut Byml, hash: u64) -> &mut Byml {
    doc.array_mut("Actors").iter_mut().find(|a| hash_of(a) == hash).unwrap()
}

#[test]
fn saving_moves_adds_and_removes_baked_shapes() {
    let Some((romfs, cat)) = catalog() else { return };
    let mods = std::env::temp_dir().join(format!("mkwtrack-collision-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&mods);
    let ws = Workspace { romfs: romfs.clone(), mods_root: mods.clone() };
    let rel = "Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs";
    let shipped = ws.load(rel, None).unwrap().doc;
    let named =
        |doc: &Byml, name: &str| -> Vec<u64> { doc.array("Actors").iter().filter(|a| a.str_or("Name", "") == name).map(hash_of).collect() };
    let arrows = named(&shipped, "CmnObj_ArrowElectric000_L_00");
    let stands = named(&shipped, "CmnObj_AudiencePlatform000_A_00");
    let areas = named(&shipped, "SystemObj_RescueArea_00");
    let (moved, turned, still, deleted) = (arrows[0], arrows[1], arrows[2], stands[0]);

    let mut doc = shipped.clone();
    let from = actor(&mut doc, moved).vec3("Translate", [0.0; 3]);
    let to = [from[0] + 25.0, from[1] + 2.0, from[2] - 40.0];
    actor(&mut doc, moved).insert("Translate", Byml::floats(&to));
    actor(&mut doc, turned).insert("Rotate", Byml::floats(&[0.1, 1.0, -0.2]));
    actor(&mut doc, turned).insert("Scale", Byml::floats(&[2.0, 2.0, 2.0]));
    doc.array_mut("Actors").retain(|a| hash_of(a) != deleted && !areas.contains(&hash_of(a)));
    // a copy of an arrow, and a baked type this course has none of
    let mut copy = actor(&mut doc, still).clone();
    copy.insert("Hash", Byml::U64(0xC0FFEE));
    copy.insert("Translate", Byml::floats(&[from[0], from[1] + 30.0, from[2]]));
    copy.remove("SRTHash");
    doc.array_mut("Actors").push(copy.clone());
    let used: HashSet<&str> = shipped.array("Actors").iter().map(|a| a.str_or("Name", "")).collect();
    let (new_type, info) =
        cat.data.actors.iter().find(|(n, a)| a.physics == Physics::Baked && a.collision.is_some() && !used.contains(n.as_str())).unwrap();
    let mut fresh = copy.clone();
    fresh.insert("Hash", Byml::U64(0xBEEF));
    fresh.insert("Name", Byml::Str(new_type.clone()));
    fresh.insert("Gyaml", Byml::Str(info.gyaml.clone()));
    doc.array_mut("Actors").push(fresh);

    let r = ws.save(rel, &doc, None, "Mod", &HashSet::new(), Some(&cat));
    assert!(r.ok && r.warnings.is_empty(), "{r:?}");
    let written: Vec<&String> = r.written.iter().filter(|w| w.ends_with(".bphsc.zs")).collect();
    println!("{written:?}\n{:?}", r.notes);
    assert!(written.iter().any(|w| w.ends_with("/Race_FirstCircuit_A_00.Nin_Ounce_NVN2.bphsc.zs")));
    assert!(written.iter().any(|w| w.ends_with("/RaceStage_FirstCircuit_A_00.Nin_Ounce_NVN2.bphsc.zs")));
    let table_rel = r.written.iter().find(|w| w.ends_with(".rsizetable.zs")).expect("the collision files grew");
    let table = ResourceSizeTable::parse(&read_file(&r.mod_romfs.join(table_rel)).unwrap()).unwrap();

    for crel in &written {
        let before = StaticCompound::parse(&read_file(&romfs.join(crel)).unwrap()).unwrap();
        let data = read_file(&r.mod_romfs.join(crel)).unwrap();
        let after = StaticCompound::parse(&data).unwrap_or_else(|e| panic!("{crel}: {e}"));
        after.check_trees().unwrap();
        after.check_reachable().unwrap();
        after.check_bounds(1e-5).unwrap();
        assert!(after.to_bytes().unwrap() == data, "{crel} is not stable");
        assert!(table.get(crel).unwrap() >= required_collision_size(data.len(), phive::body_count_of(&data).unwrap()));
        let part = |sc: &StaticCompound, h: u64| sc.links.iter().find(|l| l.hash == h).unwrap().parts[0].clone();
        // the moved arrow keeps its distance to one that stayed, whatever frame the file is in
        let world = glam::Vec3::from(to).distance(actor(&mut doc, still).vec3("Translate", [0.0; 3]).into());
        let baked = part(&after, moved).instance.translation.distance(part(&after, still).instance.translation);
        assert!((world - baked).abs() < 0.02, "{crel}: {world} {baked}");
        assert_eq!(part(&after, still), part(&before, still));
        assert_eq!(part(&after, moved).instance.rotation, part(&before, moved).instance.rotation);
        if crel.contains("/Race_") {
            assert_eq!(part(&after, moved).instance.translation.to_array(), to);
        }
        assert_eq!(part(&after, turned).instance.scale, glam::Vec3::splat(2.0));
        assert!(part(&after, turned).instance.rotation.angle_between(part(&before, turned).instance.rotation) > 0.1);
        assert!(after.links.iter().all(|l| l.hash != deleted));
        assert_eq!(part(&after, 0xC0FFEE).instance.shape, part(&after, still).instance.shape);
        assert_eq!(after.links.iter().find(|l| l.hash == 0xBEEF).unwrap().parts.len(), {
            let (file, hash) = info.collision.clone().unwrap();
            let from = StaticCompound::parse(&read_file(&romfs.join(file)).unwrap()).unwrap();
            from.links.iter().find(|l| l.hash == hash).unwrap().parts.len()
        });
        // every rescue area went: the last keeps its link, a kilometre below where it was
        let parked: Vec<_> = after.links.iter().filter(|l| areas.contains(&l.hash)).collect();
        assert_eq!(parked.len(), 1);
        let was = before.links.iter().find(|l| l.hash == parked[0].hash).unwrap().parts[0].instance.translation;
        assert!((was.y - parked[0].parts[0].instance.translation.y - 1000.0).abs() < 0.01);
        // links stay sorted by type name, new ones included
        let names: std::collections::HashMap<u64, &str> = doc.array("Actors").iter().map(|a| (hash_of(a), a.str_or("Name", ""))).collect();
        let order: Vec<&str> = after.links.iter().filter_map(|l| names.get(&l.hash).copied()).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
    }

    // saved again as the game has it: the mod's collision files go away
    let r = ws.save(rel, &shipped, None, "Mod", &HashSet::new(), Some(&cat));
    assert!(r.ok, "{r:?}");
    assert!(written.iter().all(|w| !r.mod_romfs.join(w).exists()));
    assert!(r.notes.iter().any(|n| n.contains("removed")), "{:?}", r.notes);
    std::fs::remove_dir_all(&mods).unwrap();
}

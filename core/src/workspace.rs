//! Loading course files from the game (or a mod) and saving them as LayeredFS mods.
//!
//! A mod is a folder `<mods root>/<mod name>/romfs/` holding only the files that differ from
//! the game: the course file, its PathAdditionalData, the course's baked collision when an
//! object in it moved, came or went, and the resource size table when a file outgrew its
//! entry. Emulators (Eden and forks) read `load/<title id>/<mod name>/romfs`;
//! ns2jb-modloader on a Switch 2 reads one shared `contents/<title id>/romfs` folder.

use crate::banc::{TITLE_ID, pad_rel, recompute, safe_rel, srt_hash, validate};
use crate::byml::{self, Byml};
use crate::catalog::Catalog;
use crate::collision::{self, Physics, Pose};
use crate::error::{Context, Result};
use crate::io::{compress, files_under, read_file, write_atomic};
use crate::phive::{self, StaticCompound};
use crate::restbl::{ResourceSizeTable, required_collision_size, required_size};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the game keeps the baked collision of courses and missions.
const COLLISION_DIRS: [&str; 2] = ["Phive/StaticCompoundBody/Course", "Phive/StaticCompoundBody/Mission"];

/// The size table entry a file needs.
fn needed_size(rel: &str, data: &[u8]) -> u32 {
    match phive::body_count_of(data) {
        Ok(bodies) if ResourceSizeTable::key(rel).ends_with(".bphsc") => required_collision_size(data.len(), bodies),
        _ => required_size(rel, data.len()),
    }
}

/// BYML version of every Mario Kart World course file.
pub const BYML_VERSION: u16 = 7;

/// A course as read from disk.
pub struct Loaded {
    pub file: String,
    pub from_mod: bool,
    pub version: u16,
    pub doc: Byml,
    pub pad: Option<Byml>,
    pub pad_file: Option<String>,
}

#[derive(Default, Debug)]
pub struct SaveResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    /// romfs-relative paths written
    pub written: Vec<String>,
    /// the romfs folder they were written to
    pub mod_romfs: PathBuf,
}

impl SaveResult {
    fn fail(mut self, e: impl ToString) -> SaveResult {
        self.errors.push(e.to_string());
        self.ok = false;
        self
    }
}

/// What saving one collision file did.
#[derive(Default)]
struct CollisionSaved {
    /// The size table entry the written file needs; `None` when nothing was written.
    written: Option<u32>,
    notes: Vec<String>,
    warnings: Vec<String>,
}

/// Mod names become folder names: letters, digits, space, `_`, `.` and `-` only.
pub fn valid_mod_name(m: &str) -> bool {
    !m.trim_matches('.').is_empty() && m.chars().all(|c| c.is_ascii_alphanumeric() || " _.-".contains(c))
}

/// Where ns2jb-modloader reads this game's RomFS mods. `sd` may be the SD card root, its
/// `ns2jb-modloader` folder, or that folder's `contents`.
pub fn ns2jb_romfs(sd: &Path) -> PathBuf {
    let base = if sd.file_name().is_some_and(|n| n == "contents") && sd.parent().is_some_and(|p| p.ends_with("ns2jb-modloader")) {
        sd.parent().unwrap().to_path_buf()
    } else if sd.file_name().is_some_and(|n| n == "ns2jb-modloader") {
        sd.to_path_buf()
    } else {
        sd.join("ns2jb-modloader")
    };
    base.join("contents").join(TITLE_ID.to_lowercase()).join("romfs")
}

/// The game's romfs and the folder mods are saved in.
pub struct Workspace {
    pub romfs: PathBuf,
    pub mods_root: PathBuf,
}

impl Workspace {
    pub fn mod_romfs(&self, m: &str) -> Result<PathBuf> {
        if !valid_mod_name(m) {
            return Err("a mod name may only use letters, digits, space, _ . and -".into());
        }
        Ok(self.mods_root.join(m).join("romfs"))
    }

    /// The file to read: the mod's copy when it has one, else the game's.
    fn source(&self, rel: &str, m: Option<&str>) -> (PathBuf, bool) {
        if let Some(Ok(dir)) = m.filter(|m| !m.is_empty()).map(|m| self.mod_romfs(m)) {
            let p = dir.join(rel);
            if p.exists() {
                return (p, true);
            }
        }
        (self.romfs.join(rel), false)
    }

    /// Mods saved so far (folders under the mods root that have a `romfs`).
    pub fn mods(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.mods_root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("romfs").is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// Read a course (from mod `m` if it has a copy) and its PathAdditionalData.
    pub fn load(&self, rel: &str, m: Option<&str>) -> Result<Loaded> {
        let rel = safe_rel(rel)?;
        let (src, from_mod) = self.source(&rel, m);
        let (doc, version) = byml::from_bytes(&read_file(&src)?).path_context(&src)?;
        let prel = pad_rel(&rel);
        let (psrc, _) = self.source(&prel, m);
        let (pad, pad_file) = match read_file(&psrc).ok().and_then(|d| byml::from_bytes(&d).ok()) {
            Some((p, _)) => (Some(p), Some(prel)),
            None => (None, None),
        };
        Ok(Loaded { file: rel, from_mod, version, doc, pad, pad_file })
    }

    /// Save a course into mod `m`: rebuild its summaries, check it, fill in `SRTHash` for new
    /// objects, write the course (power-of-two padded, zstd) and its PathAdditionalData (minus
    /// entries for `deleted` points), and grow the size table if a file outgrew its entry.
    /// Nothing is written when the checks find an error.
    pub fn save(&self, rel: &str, doc: &Byml, pad: Option<&Byml>, m: &str, deleted: &HashSet<u64>, cat: Option<&Catalog>) -> SaveResult {
        let res = SaveResult::default();
        let rel = match safe_rel(rel) {
            Ok(r) => r,
            Err(e) => return res.fail(e),
        };
        let mod_romfs = match self.mod_romfs(m) {
            Ok(d) => d,
            Err(e) => return res.fail(e),
        };
        let mut res = SaveResult { mod_romfs: mod_romfs.clone(), ..res };
        let mut doc = doc.clone();
        let original = read_file(&self.romfs.join(&rel)).ok().and_then(|d| byml::from_bytes(&d).ok()).map(|x| x.0);
        res.notes = recompute(&mut doc, original.as_ref(), cat);
        let checks = validate(&doc);
        res.warnings = checks.warnings;
        if !checks.errors.is_empty() {
            res.errors = checks.errors;
            return res;
        }
        for a in doc.array_mut("Actors").iter_mut() {
            if a.get("SRTHash").is_none() {
                let h = srt_hash(a);
                a.insert("SRTHash", Byml::U32(h));
            }
        }
        let mut sizes = vec![];
        let course = match self.write_byml(&mod_romfs, &rel, &doc, true) {
            Ok(n) => n,
            Err(e) => return res.fail(e),
        };
        res.written.push(rel.clone());
        sizes.push((rel.clone(), required_size(&rel, course)));
        if let Some(pad) = pad {
            let mut pad = pad.clone();
            if let Byml::Hash64(m) = &mut pad {
                m.retain(|k, _| !deleted.contains(k));
            }
            let prel = pad_rel(&rel);
            match self.write_byml(&mod_romfs, &prel, &pad, false) {
                Ok(n) => {
                    res.written.push(prel.clone());
                    let need = required_size(&prel, n);
                    sizes.push((prel, need));
                }
                Err(e) => return res.fail(e),
            }
        }
        // The course is saved; a collision file that cannot be brought along is a warning
        if let Some(original) = &original {
            for crel in self.collision_files(&rel, original) {
                match self.sync_collision(&mod_romfs, &crel, original, &doc, cat) {
                    Ok(out) => {
                        res.notes.extend(out.notes);
                        res.warnings.extend(out.warnings);
                        if let Some(need) = out.written {
                            res.written.push(crel.clone());
                            sizes.push((crel, need));
                        }
                    }
                    Err(e) => res.warnings.push(format!("{crel} was not updated, so collision may not match the course: {e}")),
                }
            }
        }
        match self.update_restbl(&mod_romfs, &sizes) {
            Ok(Some((rrel, notes))) => {
                res.written.push(rrel);
                res.notes.extend(notes);
            }
            Ok(None) => {}
            Err(e) => return res.fail(format!("resource size table: {e}")),
        }
        res.ok = true;
        res
    }

    /// Serialize, check that it reads back, write (zstd when `course`). Returns the
    /// decompressed size, which is what the size table counts.
    fn write_byml(&self, mod_romfs: &Path, rel: &str, root: &Byml, course: bool) -> Result<usize> {
        let data = byml::to_bytes(root, BYML_VERSION, course)?;
        byml::from_bytes(&data).context(format!("{rel} does not read back"))?;
        let bytes = if course { compress(&data)? } else { data.clone() };
        write_atomic(&mod_romfs.join(rel), &bytes)?;
        Ok(data.len())
    }

    /// Copy a saved mod into ns2jb-modloader's SD layout (Switch 2):
    /// `<sd>/ns2jb-modloader/contents/<title id>/romfs/...`. Every mod for a game shares that
    /// one romfs, so files are copied over the top and the resource size table is merged into
    /// the one already there (entries only ever grow).
    pub fn export_ns2jb(&self, m: &str, sd: &Path) -> SaveResult {
        let res = SaveResult::default();
        let src = match self.mod_romfs(m) {
            Ok(p) => p,
            Err(e) => return res.fail(e),
        };
        if !src.is_dir() {
            return res.fail(format!("the mod \"{m}\" has not been saved yet"));
        }
        if !sd.is_dir() {
            return res.fail(format!("{} is not a folder (set the SD card in Settings)", sd.display()));
        }
        let dst = ns2jb_romfs(sd);
        let mut res = SaveResult { mod_romfs: dst.clone(), ..res };
        let restbl = self.restbl_rel();
        let mut sizes = vec![];
        for rel in files_under(&src) {
            if restbl.as_deref() == Some(rel.as_str()) {
                continue; // merged below, so other mods' entries survive
            }
            let (from, to) = (src.join(&rel), dst.join(&rel));
            let replaced = to.exists();
            let copied = to
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::copy(&from, &to))
                .map_err(Into::into)
                .and_then(|_| read_file(&from));
            match copied {
                Ok(data) => {
                    if replaced {
                        res.notes.push(format!("replaced {rel}"));
                    }
                    sizes.push((rel.clone(), needed_size(&rel, &data)));
                    res.written.push(rel);
                }
                Err(e) => return res.fail(format!("{rel}: {e}")),
            }
        }
        match self.update_restbl(&dst, &sizes) {
            Ok(Some((rrel, notes))) => {
                res.written.push(rrel);
                res.notes.extend(notes);
            }
            Ok(None) => {}
            Err(e) => return res.fail(format!("resource size table: {e}")),
        }
        res.ok = true;
        res
    }

    /// romfs-relative path of the game's resource size table.
    fn restbl_rel(&self) -> Option<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.romfs.join("System/Resource"))
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|f| f.starts_with("ResourceSizeTable.") && f.ends_with(".rsizetable.zs"))
            .collect();
        v.sort();
        v.first().map(|f| format!("System/Resource/{f}"))
    }

    /// The baked collision files that hold this course's objects: the course's own, and any
    /// other that bakes the same objects (`RaceStage_*` has them relative to the stage). They
    /// are found by the objects' hashes, since nothing else names the course they belong to.
    pub fn collision_files(&self, rel: &str, original: &Byml) -> Vec<String> {
        static FOUND: Mutex<BTreeMap<(PathBuf, String), Vec<String>>> = Mutex::new(BTreeMap::new());
        let key = (self.romfs.clone(), rel.to_string());
        if let Some(v) = FOUND.lock().ok().and_then(|f| f.get(&key).cloned()) {
            return v;
        }
        let hashes: HashSet<u64> = original.array("Actors").iter().map(crate::banc::hash_of).collect();
        let files: Vec<String> = COLLISION_DIRS
            .iter()
            .flat_map(|dir| files_under(&self.romfs.join(dir)).into_iter().map(move |f| format!("{dir}/{f}")))
            .filter(|f| f.ends_with(".bphsc.zs"))
            .collect();
        let ours = |f: &String| {
            let links = read_file(&self.romfs.join(f)).ok().and_then(|d| phive::links_of(&d).ok()).unwrap_or_default();
            let known = links.iter().filter(|l| hashes.contains(&l.0)).count();
            known > 0 && known * 10 >= links.len() * 9
        };
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
        let found: Vec<String> = std::thread::scope(|s| {
            let handles: Vec<_> = files
                .chunks(files.len().div_ceil(threads).max(1))
                .map(|chunk| s.spawn(move || chunk.iter().filter(|f| ours(f)).cloned().collect::<Vec<_>>()))
                .collect();
            handles.into_iter().flat_map(|h| h.join().expect("scan thread panicked")).collect()
        });
        if let Ok(mut f) = FOUND.lock() {
            f.insert(key, found.clone());
        }
        found
    }

    /// Bring one baked collision file in step with the course being saved, starting from the
    /// game's copy. Writes it into the mod when it differs from the game's, and removes an
    /// earlier copy when it no longer does.
    fn sync_collision(&self, mod_romfs: &Path, crel: &str, original: &Byml, doc: &Byml, cat: Option<&Catalog>) -> Result<CollisionSaved> {
        let mut out = CollisionSaved::default();
        let shipped = read_file(&self.romfs.join(crel))?;
        let mut sc = StaticCompound::parse(&shipped)?;
        let physics = |name: &str| cat.and_then(|c| c.actor(name)).map_or(Physics::None, |a| a.physics);
        let mut donor = |name: &str| {
            let (file, hash) = cat?.actor(name)?.collision.clone()?;
            let from = StaticCompound::parse(&read_file(&self.romfs.join(&file)).ok()?).ok()?;
            Some((from, hash))
        };
        let changes = collision::sync(&mut sc, &Pose::all(original), &Pose::all(doc), &physics, &mut donor)?;
        let mut without: BTreeMap<&str, (usize, &str)> = BTreeMap::new();
        for (name, why) in &changes.without {
            without.entry(name).or_insert((0, why)).0 += 1;
        }
        for (name, (n, why)) in without {
            out.warnings.push(format!("{n} new {name} will have no collision in game ({crel}: {why})"));
        }
        let target = mod_romfs.join(crel);
        if !changes.any() {
            if target.exists() {
                std::fs::remove_file(&target).path_context(&target)?;
                out.notes.push(format!("removed {crel} from the mod: its objects are where the game has them"));
            }
            return Ok(out);
        }
        let data = sc.to_bytes()?;
        // What was written must read back as a file that follows every rule the game's do
        let back = StaticCompound::parse(&data).context(format!("{crel} does not read back"))?;
        back.check_trees().context(format!("{crel} does not read back"))?;
        write_atomic(&target, &compress(&data)?)?;
        if changes.parked > 0 {
            out.notes.push(format!(
                "{crel}: {} deleted objects were the last of their kind, so their collision was lowered 1 km instead",
                changes.parked
            ));
        }
        out.notes.push(format!("collision {crel}: {}", changes.summary()));
        out.written = Some(needed_size(crel, &data));
        Ok(out)
    }

    /// Write the size table into `romfs` only if a file outgrew its entry. Starts from the copy
    /// already in `romfs` when there is one, else from the game's. `sizes` gives the entry each
    /// file needs.
    fn update_restbl(&self, romfs: &Path, sizes: &[(String, u32)]) -> Result<Option<(String, Vec<String>)>> {
        let Some(rrel) = self.restbl_rel() else { return Ok(None) };
        let target = romfs.join(&rrel);
        let src = if target.exists() { target.clone() } else { self.romfs.join(&rrel) };
        let mut table = ResourceSizeTable::parse(&read_file(&src)?).path_context(&src)?;
        let mut notes = vec![];
        for (rel, need) in sizes {
            let need = *need;
            let have = table.get(rel);
            if have.is_none_or(|h| h < need) {
                table.set(rel, need);
                notes.push(format!("RESTBL entry for {rel}: {} -> {need}", have.map_or("none".into(), |h| h.to_string())));
            }
        }
        if notes.is_empty() {
            return Ok(None);
        }
        write_atomic(&target, &compress(&table.to_bytes())?)?;
        Ok(Some((rrel, notes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::banc::tests::{actor, course};

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mkwtrack-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A tiny fake romfs: one course and a size table with its entry.
    fn romfs(dir: &Path) -> &'static str {
        let rel = "Banc/Test_A_00[Game03].bcett.byml.zs";
        // like a shipped file: summaries up to date and every object with an SRTHash
        let mut doc = course();
        recompute(&mut doc, None, None);
        for a in doc.array_mut("Actors") {
            let h = srt_hash(a);
            a.insert("SRTHash", Byml::U32(h));
        }
        let data = byml::to_bytes(&doc, BYML_VERSION, true).unwrap();
        write_atomic(&dir.join(rel), &compress(&data).unwrap()).unwrap();
        let mut t = ResourceSizeTable { version: 1, name_len: 160, crc: Default::default(), names: Default::default() };
        t.set(rel, required_size(rel, data.len()));
        write_atomic(&dir.join("System/Resource/ResourceSizeTable.Product.100.rsizetable.zs"), &compress(&t.to_bytes()).unwrap()).unwrap();
        rel
    }

    #[test]
    fn save_load_and_size_table() {
        let dir = temp("save");
        let rel = romfs(&dir.join("romfs"));
        let ws = Workspace { romfs: dir.join("romfs"), mods_root: dir.join("mods") };
        let l = ws.load(rel, None).unwrap();
        assert!(!l.from_mod && l.pad.is_none());
        // unchanged: the course is written, the size table is not
        let r = ws.save(rel, &l.doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok, "{r:?}");
        assert_eq!(r.written, [rel]);
        assert_eq!(read_file(&dir.join("mods/Mod/romfs").join(rel)).unwrap(), read_file(&dir.join("romfs").join(rel)).unwrap());
        // grown past the next power of two: the size table entry grows too
        let mut big = l.doc.clone();
        for i in 0..400 {
            big.array_mut("Actors").push(actor("CmnObj_Coin000_A_00", 1000 + i, [i as f32, 0.0, 0.0]));
        }
        let r = ws.save(rel, &big, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written.len() == 2, "{r:?}");
        let back = ws.load(rel, Some("Mod")).unwrap();
        assert!(back.from_mod);
        assert_eq!(back.doc.get("CountActor").unwrap().get("CmnObj_Coin000_A_00"), Some(&Byml::I32(402)));
        assert!(back.doc.array("Actors").iter().all(|a| matches!(a.get("SRTHash"), Some(Byml::U32(_)))));
        let t = ResourceSizeTable::parse(&read_file(&dir.join("mods/Mod/romfs").join(&r.written[1])).unwrap()).unwrap();
        let size = read_file(&dir.join("mods/Mod/romfs").join(rel)).unwrap().len();
        assert_eq!(t.get(rel), Some(required_size(rel, size)));
        // refusals write nothing
        let mut dup = l.doc.clone();
        let first = dup.array("Actors")[0].clone();
        dup.array_mut("Actors").push(first);
        assert!(!ws.save(rel, &dup, None, "Dup", &HashSet::new(), None).ok);
        assert!(!dir.join("mods/Dup").exists());
        assert!(!ws.save("../../etc/passwd", &l.doc, None, "X", &HashSet::new(), None).ok);
        assert!(!ws.save(rel, &l.doc, None, "../escape", &HashSet::new(), None).ok);
        assert_eq!(ws.mods(), ["Mod"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn baked_collision_follows_the_course() {
        let dir = temp("collision");
        let rel = romfs(&dir.join("romfs"));
        // the course's two coins, baked the way the game bakes static objects
        let crel = "Phive/StaticCompoundBody/Course/Test_A_00.Nin_Ounce_NVN2.bphsc.zs";
        let shipped = crate::phive::tests::sample(&[(1, [0.0; 3]), (2, [5.0, 0.0, 0.0])], 0.5);
        write_atomic(&dir.join("romfs").join(crel), &compress(&shipped).unwrap()).unwrap();
        let other = "Phive/StaticCompoundBody/Course/Other_A_00.Nin_Ounce_NVN2.bphsc.zs";
        write_atomic(&dir.join("romfs").join(other), &compress(&crate::phive::tests::sample(&[(900, [0.0; 3])], 0.5)).unwrap()).unwrap();
        let table_rel = "System/Resource/ResourceSizeTable.Product.100.rsizetable.zs";
        let mut table = ResourceSizeTable::parse(&read_file(&dir.join("romfs").join(table_rel)).unwrap()).unwrap();
        table.set(crel, needed_size(crel, &shipped));
        write_atomic(&dir.join("romfs").join(table_rel), &compress(&table.to_bytes()).unwrap()).unwrap();
        let ws = Workspace { romfs: dir.join("romfs"), mods_root: dir.join("mods") };
        let l = ws.load(rel, None).unwrap();
        assert_eq!(ws.collision_files(rel, &l.doc), [crel]);
        let in_mod = dir.join("mods/Mod/romfs");

        // unchanged: no collision file in the mod
        let r = ws.save(rel, &l.doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written == [rel] && r.warnings.is_empty(), "{r:?}");

        // a coin moved: the collision file comes along, at the same size
        let mut doc = l.doc.clone();
        doc.array_mut("Actors")[1].insert("Translate", Byml::floats(&[5.0, 3.0, -2.0]));
        let r = ws.save(rel, &doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written == [rel, crel] && r.warnings.is_empty(), "{r:?}");
        assert!(r.notes.iter().any(|n| n.contains("1 moved")), "{:?}", r.notes);
        let sc = StaticCompound::parse(&read_file(&in_mod.join(crel)).unwrap()).unwrap();
        assert_eq!(sc.links[1].parts[0].instance.translation.to_array(), [5.0, 3.0, -2.0]);
        assert_eq!(sc.links[0].parts[0].instance.translation.to_array(), [0.0; 3]);

        // more coins and one fewer: shapes like the others', and a larger size table entry
        for i in 0..30 {
            doc.array_mut("Actors").push(actor("CmnObj_Coin000_A_00", 1000 + i, [i as f32 * 3.0, 9.0, 0.0]));
        }
        doc.array_mut("Actors").remove(0);
        let r = ws.save(rel, &doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written == [rel, crel, table_rel] && r.warnings.is_empty(), "{r:?}");
        let data = read_file(&in_mod.join(crel)).unwrap();
        let sc = StaticCompound::parse(&data).unwrap();
        sc.check_trees().unwrap();
        assert_eq!(sc.links.len(), 31);
        assert!(sc.links.iter().all(|l| l.hash != 1 && l.parts.len() == 1));
        let table = ResourceSizeTable::parse(&read_file(&in_mod.join(table_rel)).unwrap()).unwrap();
        assert_eq!(table.get(crel), Some(required_collision_size(data.len(), 1)));
        assert!(data.len() > shipped.len());

        // back to the game's course: the mod's collision file goes
        let r = ws.save(rel, &l.doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written == [rel] && !in_mod.join(crel).exists(), "{r:?}");
        assert!(r.notes.iter().any(|n| n.contains("removed")), "{:?}", r.notes);

        // a collision file the editor cannot read is left alone, with a warning
        write_atomic(&dir.join("romfs").join(crel), &compress(&shipped[..shipped.len() - 40]).unwrap()).unwrap();
        let r = ws.save(rel, &doc, None, "Mod", &HashSet::new(), None);
        assert!(r.ok && r.written == [rel], "{r:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mod_names_and_ns2jb_paths() {
        assert!(valid_mod_name("My Mod_1.0-b"));
        for bad in ["", "..", ".", "a/b", "a\\b", "ü"] {
            assert!(!valid_mod_name(bad), "{bad}");
        }
        let sd = Path::new("/sd");
        let want = Path::new("/sd/ns2jb-modloader/contents/0400c3f00006e000/romfs");
        for p in [sd.to_path_buf(), sd.join("ns2jb-modloader"), sd.join("ns2jb-modloader/contents")] {
            assert_eq!(ns2jb_romfs(&p), want);
        }
    }
}

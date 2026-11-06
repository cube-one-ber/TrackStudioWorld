//! Loading course files from the game (or a mod) and saving them as LayeredFS mods.
//!
//! A mod is a folder `<mods root>/<mod name>/romfs/` holding only the files that differ from
//! the game: the course file, its PathAdditionalData, and the resource size table when a file
//! outgrew its entry. Emulators (Eden and forks) read `load/<title id>/<mod name>/romfs`;
//! ns2jb-modloader on a Switch 2 reads one shared `contents/<title id>/romfs` folder.

use crate::banc::{TITLE_ID, pad_rel, recompute, safe_rel, srt_hash, validate};
use crate::byml::{self, Byml};
use crate::catalog::Catalog;
use crate::error::{Context, Result};
use crate::io::{compress, files_under, read_file, write_atomic};
use crate::restbl::{ResourceSizeTable, required_size};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
        sizes.push((rel.clone(), course));
        if let Some(pad) = pad {
            let mut pad = pad.clone();
            if let Byml::Hash64(m) = &mut pad {
                m.retain(|k, _| !deleted.contains(k));
            }
            let prel = pad_rel(&rel);
            match self.write_byml(&mod_romfs, &prel, &pad, false) {
                Ok(n) => {
                    res.written.push(prel.clone());
                    sizes.push((prel, n));
                }
                Err(e) => return res.fail(e),
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
                    sizes.push((rel.clone(), data.len()));
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

    /// Write the size table into `romfs` only if a file outgrew its entry. Starts from the copy
    /// already in `romfs` when there is one, else from the game's.
    fn update_restbl(&self, romfs: &Path, sizes: &[(String, usize)]) -> Result<Option<(String, Vec<String>)>> {
        let Some(rrel) = self.restbl_rel() else { return Ok(None) };
        let target = romfs.join(&rrel);
        let src = if target.exists() { target.clone() } else { self.romfs.join(&rrel) };
        let mut table = ResourceSizeTable::parse(&read_file(&src)?).path_context(&src)?;
        let mut notes = vec![];
        for (rel, size) in sizes {
            let need = required_size(rel, *size);
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

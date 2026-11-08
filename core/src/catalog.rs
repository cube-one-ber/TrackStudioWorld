//! What the editor needs to know about the game, read from the romfs and cached as JSON:
//!
//! * object types: label and category (RSDB `Actor`), model (ActorInfo → ModelInfo → Fmdb),
//!   model bounds, `BancTransOffset` and parameter table (from the GymlArchive);
//! * parameter tables (RSDB `BancParam`) and path types (RSDB `GraphRailDefine`);
//! * courses (RSDB `Course`) and where the stage models are placed (`WorldResident`,
//!   `Rainbow_00_Fix`);
//! * from a survey of every shipped course: which object names feed each `Collects` counter,
//!   the extra fields a new object of each type should get, and the values each parameter
//!   takes.

use crate::byml::{self, Byml};
use crate::error::{Context, Result};
use crate::io::read_file;
use crate::typedjson;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Bump when `CatalogData` or how it is built changes, so old caches are rebuilt.
const CACHE_VERSION: u32 = 4;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct ActorInfo {
    pub gyaml: String,
    pub label: String,
    pub category: String,
    pub banc_param: String,
    /// the model's path in the romfs (`Model/<name>.bfres.zs`), when that file exists
    pub model: Option<String>,
    pub aabb: Option<[[f32; 3]; 2]>,
    /// `BancTransOffset`: shipped placements already include it (see `Document::add_actor`)
    pub banc_offset: Option<[f32; 3]>,
    pub collect: Option<String>,
    /// Typed JSON of extra keys copied from a shipped placement (PrimitiveType, Scale...).
    pub template: Option<Value>,
    /// placed in at least one shipped course
    pub placed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Param {
    pub name: String,
    pub label: String,
    /// Bool, S32, U32, F32, String, Enum or Vec3f
    pub kind: String,
    pub init: Value,
    pub init_name: Option<String>,
    pub enum_name: Option<String>,
    pub instance: bool,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct RailDef {
    pub gyaml: String,
    pub point_param: String,
    pub rail_param: String,
    pub rail_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CourseInfo {
    pub name: String,
    pub display: String,
    pub scene: String,
    pub rule: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StagePlacement {
    pub name: String,
    pub source: String,
    pub model: Option<String>,
    pub aabb: Option<[[f32; 3]; 2]>,
    pub translate: [f32; 3],
    pub rotate: [f32; 3],
    pub scale: [f32; 3],
}

/// Which object names feed each `Collects` counter.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Collects {
    /// name -> class, for every name that counts
    pub known: BTreeMap<String, String>,
    /// every name placed in a shipped course (those not in `known` count for nothing)
    pub seen: BTreeSet<String>,
    /// class -> whether the fitted membership reproduces every shipped file
    pub exact: BTreeMap<String, bool>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct CatalogData {
    pub stamp: Vec<String>,
    pub actors: BTreeMap<String, ActorInfo>,
    pub params: BTreeMap<String, Vec<Param>>,
    pub rails: BTreeMap<String, RailDef>,
    pub courses: BTreeMap<String, Vec<CourseInfo>>,
    pub stages: Vec<StagePlacement>,
    pub seen_values: BTreeMap<String, Vec<Value>>,
    pub collects: Collects,
}

pub struct Catalog {
    pub data: CatalogData,
    pub romfs: PathBuf,
}

/// `Work/Gyml/ActorGParam/CmnObj_Coin000_A_00.gear__ActorGParam.gyml` -> `CmnObj_Coin000_A_00`
pub fn gyml_name(r: &str) -> String {
    r.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string()
}

fn vec3_xyz(v: Option<&Byml>) -> Option<[f32; 3]> {
    let v = v?;
    let g = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    Some([g("X"), g("Y"), g("Z")])
}

/// Fallback `Collects` class for names that never appear in a shipped course, from their name.
/// The save report says when it was used.
pub fn collect_guess(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("CmnObj_")?;
    let three_digits = |s: &str| s.len() >= 4 && s[..3].bytes().all(|b| b.is_ascii_digit()) && s.as_bytes()[3] == b'_';
    let after = |prefixes: &[&str]| prefixes.iter().find_map(|p| rest.strip_prefix(p));
    if after(&["Coin", "WaterCoin"]).is_some_and(three_digits) {
        return Some("Coin");
    }
    if after(&["BlockBrick", "BlockEmpty", "BlockHatena", "BlockNote"]).is_some_and(|r| three_digits(r) && r[4..].starts_with("A_")) {
        return Some("Block");
    }
    if ["Mobcar", "Mobplane", "Mobship", "MobShip"].iter().any(|p| rest.starts_with(p))
        && ["_Move_", "_MoveN_", "_Ride_", "_RideLead_"].iter().any(|s| rest.contains(s))
    {
        return Some("MobVehicle");
    }
    let audience =
        ["AudienceChiko", "AudienceHeyho", "AudienceKinopio", "AudienceMegaBros", "AudienceMonte", "AudiencePatapata", "AudienceYoshi"];
    if after(&audience).is_some_and(three_digits) {
        return Some("Citizen");
    }
    if let Some(r) = rest.strip_prefix("Citizen") {
        let letters = r.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
        if letters > 0 && three_digits(&r[letters..]) {
            return Some("Citizen");
        }
    }
    let animals = ["Buffalo", "Camel", "Cow", "Elephant", "Giraffe", "Ostrich", "Penguin", "Zebra"];
    if rest.strip_prefix("Audience").and_then(|r| animals.iter().find_map(|p| r.strip_prefix(p))).is_some_and(three_digits) {
        return Some("NpcAudience");
    }
    None
}

impl Catalog {
    /// (class, verified) for an object name: verified classes come from the shipped files.
    pub fn collect_class(&self, name: &str) -> (Option<String>, bool) {
        if let Some(c) = self.data.collects.known.get(name) {
            return (Some(c.clone()), true);
        }
        if self.data.collects.seen.contains(name) {
            return (None, true);
        }
        (collect_guess(name).map(String::from), false)
    }

    /// An object type by its name or its `Gyaml` path.
    pub fn actor(&self, gyaml_or_name: &str) -> Option<&ActorInfo> {
        self.data.actors.get(&gyml_name(gyaml_or_name)).or_else(|| self.data.actors.get(gyaml_or_name))
    }

    /// Read the catalog from the cache in `cache_dir`, or build it (about 7 s) and cache it.
    pub fn load(romfs: &Path, cache_dir: &Path, progress: &dyn Fn(&str)) -> Result<Catalog> {
        std::fs::create_dir_all(cache_dir).path_context(cache_dir)?;
        let cache = cache_dir.join("catalog.json");
        let stamp = stamp(romfs);
        let cached = std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<CatalogData>(&t).ok());
        let data = match cached.filter(|d| d.stamp == stamp) {
            Some(d) => d,
            None => {
                let mut data = build(romfs, progress)?;
                data.stamp = stamp;
                crate::io::write_atomic(&cache, &serde_json::to_vec(&data)?)?;
                data
            }
        };
        Ok(Catalog { data, romfs: romfs.into() })
    }
}

/// What the cache depends on: the cache format and the source files' times.
fn stamp(romfs: &Path) -> Vec<String> {
    let mut out = vec![CACHE_VERSION.to_string()];
    let mut files = vec![romfs.join("RSDB"), romfs.join("Banc/World_Game03/WorldResident.bcett.byml.zs")];
    if let Ok(rd) = std::fs::read_dir(romfs.join("System/GymlArchive")) {
        let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        v.sort();
        files.extend(v);
    }
    for f in files {
        let t = std::fs::metadata(&f).and_then(|m| m.modified()).ok();
        out.push(format!("{}:{:?}", f.display(), t.map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())));
    }
    out
}

/// The game's GymlArchive packs: generic parameter files, resolved with their `$parent` chain.
struct GymlArchive {
    files: BTreeMap<String, Vec<u8>>,
    cache: std::cell::RefCell<BTreeMap<String, Option<Byml>>>,
}

impl GymlArchive {
    fn open(romfs: &Path) -> Result<Self> {
        let dir = romfs.join("System/GymlArchive");
        let mut packs: Vec<_> = std::fs::read_dir(&dir)
            .path_context(&dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with(".sarc.zs"))
            .collect();
        packs.sort();
        let mut files = BTreeMap::new();
        for p in packs {
            files.extend(crate::sarc::read(&read_file(&p)?).path_context(&p)?);
        }
        Ok(Self { files, cache: Default::default() })
    }

    /// Resolve a `Work/...gyml` reference, merging its `$parent` chain.
    fn get(&self, r: &str) -> Option<Byml> {
        if r.is_empty() {
            return None;
        }
        if let Some(v) = self.cache.borrow().get(r) {
            return v.clone();
        }
        let key = r.strip_prefix("Work/").unwrap_or(r);
        let key = key.strip_suffix(".gyml").map_or_else(|| key.to_string(), |stem| format!("{stem}.bgyml"));
        let node = self.files.get(&key).and_then(|d| byml::from_bytes(d).ok()).map(|(n, _)| n);
        let node = node.map(|n| match n.get("$parent").and_then(|p| p.as_str()).filter(|p| *p != r).map(|p| self.get(p)) {
            Some(Some(base @ Byml::Map(_))) => merge(&base, &n),
            _ => n,
        });
        self.cache.borrow_mut().insert(r.to_string(), node.clone());
        node
    }
}

/// `over` on top of `base`, merging nested maps.
fn merge(base: &Byml, over: &Byml) -> Byml {
    match (base, over) {
        (Byml::Map(b), Byml::Map(o)) => {
            let mut out = b.clone();
            for (k, v) in o {
                let merged = match out.get(k) {
                    Some(bv @ Byml::Map(_)) if matches!(v, Byml::Map(_)) => merge(bv, v),
                    _ => v.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Byml::Map(out)
        }
        _ => over.clone(),
    }
}

/// The rows of the newest `RSDB/<name>.Product.*.rstbl.byml.zs`.
fn rsdb(romfs: &Path, name: &str) -> Vec<Byml> {
    let prefix = format!("{name}.Product.");
    let mut hits: Vec<_> = std::fs::read_dir(romfs.join("RSDB"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().map(|f| f.to_string_lossy()).is_some_and(|f| f.starts_with(&prefix) && f.ends_with(".rstbl.byml.zs")))
        .collect();
    hits.sort();
    hits.last()
        .and_then(|p| read_file(p).ok())
        .and_then(|d| byml::from_bytes(&d).ok())
        .and_then(|(n, _)| n.as_array().cloned())
        .unwrap_or_default()
}

/// The romfs path of the model an `Fmdb` names (see [`crate::bfres::model_rel`]), if the file
/// exists.
pub fn model_rel(romfs: &Path, fmdb: &str) -> Option<String> {
    crate::bfres::model_rel(fmdb).filter(|rel| romfs.join(rel).is_file())
}

fn plain(v: Option<&Byml>) -> Value {
    v.map_or(Value::Null, typedjson::to_plain)
}

/// Every Banc course file under the romfs, as relative paths with forward slashes.
pub fn banc_files(romfs: &Path) -> Vec<String> {
    let mut v: Vec<String> = crate::io::files_under(&romfs.join("Banc"))
        .into_iter()
        .filter(|f| f.ends_with(".bcett.byml.zs"))
        .map(|f| format!("Banc/{f}"))
        .collect();
    v.sort();
    v
}

/// Parse many romfs files on all cores. Files that fail to read give `None`.
pub fn parse_all(romfs: &Path, rels: &[String]) -> Vec<(String, Option<Byml>)> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let chunk = rels.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = rels
            .chunks(chunk)
            .map(|chunk| {
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|r| (r.clone(), read_file(&romfs.join(r)).ok().and_then(|d| byml::from_bytes(&d).ok()).map(|x| x.0)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("parse thread panicked")).collect()
    })
}

fn build(romfs: &Path, progress: &dyn Fn(&str)) -> Result<CatalogData> {
    progress("Reading GymlArchive…");
    let gyml = GymlArchive::open(romfs)?;
    progress("Resolving actor models…");
    let mut actors = read_actors(romfs, &gyml);
    progress("Reading parameter tables…");
    let params = read_params(romfs);
    let rails = read_rails(romfs);
    let courses = read_courses(romfs);
    let stages = read_stages(romfs, &actors);
    progress("Surveying every shipped course…");
    let survey = survey_courses(romfs);
    progress("Fitting Collects classes…");
    let collects = fit_collects(&survey.counts);
    for (n, class) in &collects.known {
        if let Some(a) = actors.get_mut(n) {
            a.collect = Some(class.clone());
        }
    }
    for (n, t) in survey.templates {
        if let Some(a) = actors.get_mut(&n) {
            a.template = Some(t);
            a.placed = true;
        }
    }
    Ok(CatalogData { stamp: vec![], actors, params, rails, courses, stages, seen_values: survey.seen_values, collects })
}

/// Every object type: RSDB `Actor` rows plus any ActorGParam file without a row.
fn read_actors(romfs: &Path, gyml: &GymlArchive) -> BTreeMap<String, ActorInfo> {
    let mut actors: BTreeMap<String, ActorInfo> = BTreeMap::new();
    for row in rsdb(romfs, "Actor") {
        let r = row.str_or("__RowId", "").to_string();
        let info = ActorInfo {
            gyaml: r.clone(),
            label: row.str_or("Label", "").into(),
            category: row.str_or("Category", "").into(),
            ..Default::default()
        };
        actors.insert(gyml_name(&r), info);
    }
    for path in gyml.files.keys() {
        if let Some(stem) = path.strip_prefix("Gyml/ActorGParam/").and_then(|p| p.strip_suffix(".bgyml")) {
            let n = stem.split('.').next().unwrap_or(stem).to_string();
            actors.entry(n).or_insert_with(|| ActorInfo { gyaml: format!("Work/{}.gyml", &path[..path.len() - 6]), ..Default::default() });
        }
    }
    for (n, a) in actors.iter_mut() {
        let info = gyml.get(&format!("Work/Gyml/ActorInfoGParam/{n}.game__ActorInfoGParam.gyml")).unwrap_or_else(Byml::map);
        let gp = gyml.get(&a.gyaml).unwrap_or_else(Byml::map);
        let comps = gp.get("Components").cloned().unwrap_or_else(Byml::map);
        let path = |info_key: &str, comp_key: &str| {
            info.get(info_key).and_then(|v| v.as_str()).or_else(|| comps.get(comp_key).and_then(|v| v.as_str())).unwrap_or("").to_string()
        };
        a.banc_param = gyml_name(&path("BancParamPath", "BancParamRef"));
        if let Some(mi) = gyml.get(&path("ModelInfoPath", "ModelInfoRef")) {
            let res: Vec<&Byml> = mi.array("ModelResource").iter().filter(|x| x.as_map().is_some()).collect();
            let main = res.iter().find(|r| r.get("ResourceType").is_none()).or(res.first());
            a.model = main.and_then(|m| m.get("Fmdb")).and_then(|f| f.as_str()).and_then(|f| model_rel(romfs, f));
        }
        if let (Some(mn), Some(mx)) = (vec3_xyz(info.get("ModelAABBMin")), vec3_xyz(info.get("ModelAABBMax"))) {
            a.aabb = Some([mn, mx]);
        }
        a.banc_offset = vec3_xyz(gp.get("BancTransOffset"));
    }
    actors
}

/// Parameter tables (RSDB `BancParam`), by table name.
fn read_params(romfs: &Path) -> BTreeMap<String, Vec<Param>> {
    let mut params = BTreeMap::new();
    for row in rsdb(romfs, "BancParam") {
        let mut list = vec![];
        for kind in ["Bool", "S32", "U32", "F32", "String", "Enum", "Vec3f"] {
            for p in row.array(&format!("BancGParam{kind}Array")) {
                let info = p.get("ParamInfo").cloned().unwrap_or_else(Byml::map);
                let mut e = Param {
                    name: info.str_or("Name", "").into(),
                    label: info.str_or("Label", "").into(),
                    kind: kind.into(),
                    init: plain(p.get("InitVal")),
                    init_name: None,
                    enum_name: None,
                    instance: info.get("IsInstanceParam").and_then(|b| b.as_bool()).unwrap_or(true),
                };
                if kind == "Enum" {
                    // stored as the s32 index; the enum definitions (.geyml) aren't shipped
                    e.enum_name = Some(gyml_name(p.str_or("EnumName", "")));
                    e.init_name = e.init.as_str().map(String::from);
                    e.init = plain(p.get("InitVal_s32"));
                }
                list.push(e);
            }
        }
        params.insert(gyml_name(row.str_or("__RowId", "")), list);
    }
    params
}

/// Path types (RSDB `GraphRailDefine`), by name.
fn read_rails(romfs: &Path) -> BTreeMap<String, RailDef> {
    rsdb(romfs, "GraphRailDefine")
        .iter()
        .map(|row| {
            let def = RailDef {
                gyaml: row.str_or("__RowId", "").into(),
                point_param: gyml_name(row.str_or("PointParamRef", "")),
                rail_param: gyml_name(row.str_or("RailParamRef", "")),
                rail_type: row.str_or("RailType", "").into(),
            };
            (gyml_name(row.str_or("__RowId", "")), def)
        })
        .collect()
}

/// Courses (RSDB `Course`), by Banc file base name.
fn read_courses(romfs: &Path) -> BTreeMap<String, Vec<CourseInfo>> {
    let mut courses: BTreeMap<String, Vec<CourseInfo>> = BTreeMap::new();
    for row in rsdb(romfs, "Course") {
        let Some(base) = row.str_or("BancFile", "").strip_prefix("Work/Banc/").and_then(|b| b.strip_suffix(".bcett.json")) else {
            continue;
        };
        let base = base.rsplit('/').next().unwrap_or(base).to_string();
        courses.entry(base).or_default().push(CourseInfo {
            name: gyml_name(row.str_or("__RowId", "")),
            display: row.str_or("DispName", "").into(),
            scene: gyml_name(row.str_or("Scene", "")),
            rule: gyml_name(row.str_or("GameRule", "")),
        });
    }
    courses
}

/// Stage models placed in the world: WorldResident for the island, Rainbow_00_Fix for Rainbow
/// Road's sky sections. Other files that place `Stage_` actors draw them as ordinary objects.
fn read_stages(romfs: &Path, actors: &BTreeMap<String, ActorInfo>) -> Vec<StagePlacement> {
    let mut stages = vec![];
    for rel in ["Banc/World_Game03/WorldResident.bcett.byml.zs", "Banc/Rainbow_00_Fix.bcett.byml.zs"] {
        let Some((root, _)) = read_file(&romfs.join(rel)).ok().and_then(|d| byml::from_bytes(&d).ok()) else { continue };
        for a in root.array("Actors") {
            let n = a.str_or("Name", "");
            if !n.starts_with("Stage_") {
                continue;
            }
            let act = actors.get(n);
            stages.push(StagePlacement {
                name: n.into(),
                source: rel.into(),
                model: act.and_then(|x| x.model.clone()),
                aabb: act.and_then(|x| x.aabb),
                translate: a.vec3("Translate", [0.0; 3]),
                rotate: a.vec3("Rotate", [0.0; 3]),
                scale: a.vec3("Scale", [1.0; 3]),
            });
        }
    }
    stages
}

/// One course file's `CountActor` and `Collects`, as numbers by name.
pub type CountRow = (BTreeMap<String, f64>, BTreeMap<String, f64>);

/// What every shipped course shows.
struct Survey {
    /// per file: (CountActor, Collects)
    counts: Vec<CountRow>,
    /// per object name: the extra keys of its first shipped placement
    templates: BTreeMap<String, Value>,
    /// per parameter name: up to 64 distinct shipped values, sorted
    seen_values: BTreeMap<String, Vec<Value>>,
}

/// Keys copied from a shipped placement onto new objects of the same type.
const TEMPLATE_KEYS: [&str; 4] = ["Bakeable", "IsGridify", "PrimitiveType", "ScreenSetting"];

fn survey_courses(romfs: &Path) -> Survey {
    let docs = parse_all(romfs, &banc_files(romfs));
    let mut seen: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut seen_values: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut counts = vec![];
    let mut templates: BTreeMap<String, Value> = BTreeMap::new();
    let numbers = |m: Option<&Byml>| -> BTreeMap<String, f64> {
        m.and_then(|m| m.as_map()).map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect()).unwrap_or_default()
    };
    for root in docs.iter().filter_map(|(_, d)| d.as_ref()) {
        counts.push((numbers(root.get("CountActor")), numbers(root.get("Collects"))));
        for a in root.array("Actors") {
            let n = a.str_or("Name", "");
            if n.is_empty() || templates.contains_key(n) {
                continue;
            }
            let mut t = Byml::map();
            for k in TEMPLATE_KEYS {
                if let Some(v) = a.get(k) {
                    t.insert(k, v.clone());
                }
            }
            // primitive shapes (areas) are sized by their Scale
            if let (Some(_), Some(s)) = (a.get("PrimitiveType"), a.get("Scale")) {
                t.insert("Scale", s.clone());
            }
            templates.insert(n.into(), typedjson::to_json(&t));
        }
        let rails = root.array("Rails").iter().flat_map(|r| std::iter::once(r).chain(r.array("Points")));
        let items = root.array("Actors").iter().chain(rails);
        for it in items {
            for (k, v) in it.get("Dynamic").and_then(|d| d.as_map()).into_iter().flatten() {
                if !matches!(v, Byml::Str(_) | Byml::I32(_) | Byml::U32(_)) {
                    continue;
                }
                let value = typedjson::to_plain(v);
                let s = seen.entry(k.clone()).or_default();
                if s.len() < 64 && s.insert(value.to_string()) {
                    seen_values.entry(k.clone()).or_default().push(value);
                }
            }
        }
    }
    for v in seen_values.values_mut() {
        v.sort_by(|a, b| match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => a.to_string().cmp(&b.to_string()),
        });
    }
    Survey { counts, templates, seen_values }
}

/// Work out which object names feed each `Collects` counter. Each class's count is a sum of
/// per-name counts, so solve for 0/1 membership by least squares over every file, and mark the
/// class exact only if the result reproduces every file.
pub fn fit_collects(rows: &[CountRow]) -> Collects {
    use nalgebra::{DMatrix, DVector};
    let names: Vec<String> = rows.iter().flat_map(|(ca, _)| ca.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
    let idx: BTreeMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let n_rows = rows.len();
    let mut a = DMatrix::<f64>::zeros(n_rows, names.len());
    for (i, (ca, _)) in rows.iter().enumerate() {
        for (n, c) in ca {
            a[(i, idx[n.as_str()])] = *c;
        }
    }
    let classes: BTreeSet<String> = rows.iter().flat_map(|(_, co)| co.keys().cloned()).collect();
    let mut out = Collects { seen: names.iter().cloned().collect(), ..Default::default() };
    for k in classes {
        let b = DVector::from_iterator(n_rows, rows.iter().map(|(_, co)| co.get(&k).copied().unwrap_or(0.0)));
        // a name can only belong to the class if its count never exceeds the class count
        let cand: Vec<usize> = (0..names.len()).filter(|&j| (0..n_rows).all(|i| a[(i, j)] <= b[i])).collect();
        let mut sel = vec![];
        if !cand.is_empty() {
            let sub = DMatrix::from_fn(n_rows, cand.len(), |i, j| a[(i, cand[j])]);
            let svd = sub.svd(true, true);
            let eps = svd.singular_values.max() * f64::EPSILON * (n_rows.max(cand.len()) as f64);
            if let Ok(x) = svd.solve(&b, eps) {
                sel = cand.iter().zip(x.iter()).filter(|(_, v)| **v > 0.5).map(|(j, _)| *j).collect();
            }
        }
        let exact = (0..n_rows).all(|i| (sel.iter().map(|&j| a[(i, j)]).sum::<f64>() - b[i]).abs() < 1e-6);
        out.exact.insert(k.clone(), exact);
        for j in sel {
            out.known.insert(names[j].clone(), k.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_guesses() {
        assert_eq!(collect_guess("CmnObj_Coin000_A_00"), Some("Coin"));
        assert_eq!(collect_guess("CmnObj_WaterCoin001_A_00"), Some("Coin"));
        assert_eq!(collect_guess("CmnObj_BlockHatena000_A_00"), Some("Block"));
        assert_eq!(collect_guess("CmnObj_BlockHatena000_B_00"), None);
        assert_eq!(collect_guess("CmnObj_MobcarVan000_MoveN_00"), Some("MobVehicle"));
        assert_eq!(collect_guess("CmnObj_AudienceYoshi000_A_00"), Some("Citizen"));
        assert_eq!(collect_guess("CmnObj_CitizenNokonoko002_A_00"), Some("Citizen"));
        assert_eq!(collect_guess("CmnObj_AudienceCow000_A_00"), Some("NpcAudience"));
        assert_eq!(collect_guess("CmnObj_ItemBox000_A_00"), None);
        assert_eq!(collect_guess("Coin000"), None);
    }

    #[test]
    fn fits_collects_membership() {
        let row = |ca: &[(&str, f64)], co: &[(&str, f64)]| {
            (ca.iter().map(|(k, v)| (k.to_string(), *v)).collect(), co.iter().map(|(k, v)| (k.to_string(), *v)).collect())
        };
        let rows = vec![
            row(&[("Coin", 3.0), ("Box", 2.0), ("Yoshi", 1.0)], &[("Coin", 3.0), ("Citizen", 1.0)]),
            row(&[("Coin", 1.0), ("Yoshi", 4.0), ("Toad", 2.0)], &[("Coin", 1.0), ("Citizen", 6.0)]),
            row(&[("Box", 5.0), ("Toad", 1.0)], &[("Coin", 0.0), ("Citizen", 1.0)]),
        ];
        let c = fit_collects(&rows);
        assert_eq!(c.known.get("Coin").map(String::as_str), Some("Coin"));
        assert_eq!(c.known.get("Yoshi").map(String::as_str), Some("Citizen"));
        assert_eq!(c.known.get("Toad").map(String::as_str), Some("Citizen"));
        assert!(!c.known.contains_key("Box"));
        assert!(c.exact.values().all(|&e| e));
    }

    #[test]
    fn names_and_model_paths() {
        assert_eq!(gyml_name("Work/Gyml/ActorGParam/CmnObj_Coin000_A_00.gear__ActorGParam.gyml"), "CmnObj_Coin000_A_00");
        assert_eq!(model_rel(Path::new("/nonexistent"), "Work/Model/CmnObj/CmnObj_ItemBox000/output/CmnObj_ItemBox000_A.fmdb"), None);
        assert_eq!(model_rel(Path::new("/"), "Model/x.fmdb"), None);
    }
}

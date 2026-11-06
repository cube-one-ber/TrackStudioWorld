//! Editor state behind the QML bridge: open course, selection, view options, models,
//! gizmo dragging and the JSON the panels display. Plain Rust; no Qt types here.

use crate::config::{Config, cache_dir, emulator_load_dir};
use glam::{Mat4, Quat, Vec3};
use mkwtrack::banc::{course_base, hash_of};
use mkwtrack::byml::{Byml, vec3};
use mkwtrack::catalog::{ActorInfo, Catalog, Param, StagePlacement, banc_files};
use mkwtrack::doc::{Document, Loc};
use mkwtrack::math::quat_zyx;
use mkwtrack::raycast::{Bvh, Ray, ray_line_param, ray_plane};
use mkwtrack::scene::{self, Scene, ViewOptions, stages_for};
use mkwtrack::workspace::Workspace;
use mkwtrack::{fbx, typedjson};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct ModelCache {
    pub map: Mutex<HashMap<String, Arc<fbx::Model>>>,
}

impl ModelCache {
    pub fn get(&self, rel: &str) -> Option<Arc<fbx::Model>> {
        self.map.lock().unwrap().get(rel).cloned()
    }
    pub fn has(&self, rel: &str) -> bool {
        self.map.lock().unwrap().contains_key(rel)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

pub struct Drag {
    mode: GizmoMode,
    axis: usize,
    center: Vec3,
    dir: Vec3,
    start: Vec3,
    start_param: f32,
    items: Vec<(u64, Vec3, Quat, Vec3)>,
}

pub struct State {
    pub cfg: Config,
    pub cat: Option<Arc<Catalog>>,
    pub doc: Option<Document>,
    pub selection: Vec<u64>,
    pub view: ViewOptions,
    pub xray: bool,
    pub snap_ground: bool,
    pub show_stage: bool,
    pub expanded: HashSet<String>,
    pub scene: Option<Scene>,
    pub models: Arc<ModelCache>,
    pub stages: Vec<StagePlacement>,
    pub ground: Option<Arc<Bvh>>,
    pub drag: Option<Drag>,
    pub link_pick: Option<u64>,
    pub link_type: String,
    pub console: VecDeque<String>,
    pub mod_name: String,
    pub gizmo_mode: GizmoMode,
    pub gizmo_local: bool,
    pub load_gen: u64,
}

/// Console lines kept.
const CONSOLE_LINES: usize = 500;

pub fn rgba(c: [f32; 3], a: f32) -> Value {
    json!([c[0], c[1], c[2], a])
}

fn v3(v: Vec3) -> Value {
    json!([v.x, v.y, v.z])
}

/// Qt.quaternion(w, x, y, z) order.
fn q4(q: Quat) -> Value {
    json!([q.w, q.x, q.y, q.z])
}

fn deg(v: [f32; 3]) -> Value {
    json!(v.map(|x| x.to_degrees()))
}

impl State {
    pub fn new(cfg: Config) -> State {
        State {
            mod_name: "MyTrackMod".into(),
            cfg,
            cat: None,
            doc: None,
            selection: vec![],
            view: ViewOptions { models: true, ..Default::default() },
            xray: true,
            snap_ground: false,
            show_stage: true,
            expanded: HashSet::new(),
            scene: None,
            models: Arc::new(ModelCache::default()),
            stages: vec![],
            ground: None,
            drag: None,
            link_pick: None,
            link_type: "BasicSignal".into(),
            console: VecDeque::new(),
            gizmo_mode: GizmoMode::Translate,
            gizmo_local: true,
            load_gen: 0,
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        // the console and status bar never show the home folder (or the user name in it)
        let msg = crate::config::tilde(&msg.into());
        eprintln!("{msg}");
        self.console.push_back(msg);
        if self.console.len() > CONSOLE_LINES {
            self.console.pop_front();
        }
    }

    pub fn ws(&self) -> Workspace {
        Workspace { romfs: self.cfg.romfs.clone(), mods_root: self.cfg.mods.clone() }
    }

    /// The catalog cache folder, one per romfs path.
    pub fn catalog_cache(&self) -> std::path::PathBuf {
        cache_dir().join(format!("catalog-{:08x}", fnv1a(self.cfg.romfs.to_string_lossy().as_bytes())))
    }

    // ------------------------------------------------------------ courses

    pub fn course_list(&self, filter: &str) -> Value {
        let Some(cat) = &self.cat else { return json!([]) };
        let f = filter.to_lowercase();
        let ws = self.ws();
        let mods = ws.mods();
        let mut out = vec![];
        for rel in banc_files(&self.cfg.romfs) {
            let base = course_base(&rel);
            let info = cat.data.courses.get(&base).and_then(|v| v.first());
            let display = info.map(|i| i.display.clone()).unwrap_or_default();
            if !f.is_empty() && !rel.to_lowercase().contains(&f) && !display.to_lowercase().contains(&f) {
                continue;
            }
            let group =
                if rel.contains("/World_Game03") { "Free-roam grid".to_string() } else { base.split('_').next().unwrap_or("").to_string() };
            let modded: Vec<&String> = mods.iter().filter(|m| self.cfg.mods.join(m).join("romfs").join(&rel).exists()).collect();
            out.push(json!({"file": rel, "name": rel.trim_start_matches("Banc/").trim_end_matches(".bcett.byml.zs"), "display": display,
                            "rule": info.map(|i| i.rule.clone()).unwrap_or_default(), "group": group, "mods": modded}));
        }
        let order =
            ["Race", "LongRally", "InterStageEx1", "InterStageEx2", "BalloonBattle", "CoinBattle", "BombBattle", "Mission", "Parade"];
        out.sort_by_key(|v| {
            let g = v["group"].as_str().unwrap_or("").to_string();
            (order.iter().position(|o| *o == g).unwrap_or(99), g, v["file"].as_str().unwrap_or("").to_string())
        });
        json!(out)
    }

    pub fn open(&mut self, file: &str, m: Option<&str>) -> mkwtrack::Result<()> {
        let cat = self.cat.clone().ok_or("the game catalog is still loading")?;
        let loaded = self.ws().load(file, m)?;
        let doc = Document::new(loaded);
        self.stages = stages_for(&cat, &doc.root, 12).into_iter().cloned().collect();
        self.doc = Some(doc);
        self.selection.clear();
        self.expanded.clear();
        self.drag = None;
        self.link_pick = None;
        self.ground = None;
        self.load_gen += 1;
        if let Some(m) = m.filter(|m| !m.is_empty()) {
            self.mod_name = m.to_string();
        }
        self.rebuild();
        let d = self.doc.as_ref().unwrap();
        let pts: usize = d.rails().iter().map(|r| Document::points(r).len()).sum();
        self.log(format!(
            "Opened {file}{}: {} objects, {} paths, {pts} points, {} stage model(s)",
            if d.from_mod { format!(" from mod \"{}\"", self.mod_name) } else { String::new() },
            d.actors().len(),
            d.rails().len(),
            self.stages.len()
        ));
        Ok(())
    }

    /// Models the open course needs: stages first, then object models.
    pub fn wanted_models(&self) -> Vec<(String, bool)> {
        let (Some(cat), Some(doc)) = (&self.cat, &self.doc) else { return vec![] };
        let mut out: Vec<(String, bool)> = self.stages.iter().filter_map(|s| s.model.clone()).map(|m| (m, true)).collect();
        let mut seen: HashSet<String> = out.iter().map(|x| x.0.clone()).collect();
        for a in doc.actors() {
            if let Some(m) = cat.actor(a.str_or("Gyaml", "")).and_then(|i| i.model.clone())
                && seen.insert(m.clone())
            {
                out.push((m, false));
            }
        }
        out
    }

    pub fn rebuild(&mut self) {
        let (Some(cat), Some(doc)) = (&self.cat, &self.doc) else {
            self.scene = None;
            return;
        };
        let sel: HashSet<u64> = self.selection.iter().copied().collect();
        let models = self.models.clone();
        self.scene = Some(scene::build(doc, cat, &self.view, &|m| models.has(m), &sel));
    }

    // ------------------------------------------------------------ scene JSON

    pub fn stages_json(&self) -> Value {
        if !self.show_stage {
            return json!([]);
        }
        json!(self.stages.iter().filter_map(|s| {
            let m = s.model.as_ref()?;
            let model = self.models.get(m)?;
            Some(json!({"rel": m, "name": s.name, "parts": model.parts.len(), "p": s.translate, "q": q4(quat_zyx(s.rotate)), "s": s.scale}))
        }).collect::<Vec<_>>())
    }

    pub fn actor_groups_json(&self) -> Value {
        let Some(sc) = &self.scene else { return json!([]) };
        json!(sc.groups.iter().filter_map(|(rel, inst)| {
            let model = self.models.get(rel)?;
            Some(json!({"rel": rel, "parts": model.parts.len(),
                "inst": inst.iter().map(|i| json!({"h": i.hash.to_string(), "p": v3(i.pos), "q": q4(i.rot), "s": v3(i.scale)})).collect::<Vec<_>>()}))
        }).collect::<Vec<_>>())
    }

    pub fn boxes_json(&self) -> Value {
        let Some(sc) = &self.scene else { return json!([]) };
        json!(
            sc.boxes
                .iter()
                .map(|b| {
                    let (s, q, p) = b.world.to_scale_rotation_translation();
                    json!({"h": b.hash.to_string(), "shape": b.shape, "c": rgba(b.color, 0.45), "p": v3(p), "q": q4(q), "s": v3(s)})
                })
                .collect::<Vec<_>>()
        )
    }

    /// Current transforms of the given objects, for live updates while dragging.
    pub fn transforms_json(&self, hashes: &[u64]) -> Value {
        let Some(doc) = &self.doc else { return json!([]) };
        let Some(cat) = &self.cat else { return json!([]) };
        json!(
            hashes
                .iter()
                .filter_map(|h| {
                    let (loc, a) = doc.find(*h)?;
                    if !matches!(loc, Loc::Actor(_)) {
                        return None;
                    }
                    let (p, q, s) = scene::actor_world(a);
                    let (mn, mx, _) = scene::actor_box(cat, a);
                    let local = Mat4::from_scale_rotation_translation(mx - mn, Quat::IDENTITY, (mn + mx) / 2.0);
                    let (bs, bq, bp) = (Mat4::from_scale_rotation_translation(s, q, p) * local).to_scale_rotation_translation();
                    Some(json!({"h": h.to_string(), "p": v3(p), "q": q4(q), "s": v3(s), "bp": v3(bp), "bq": q4(bq), "bs": v3(bs)}))
                })
                .collect::<Vec<_>>()
        )
    }

    pub fn rail_index_of(&self, rail_hash: u64) -> Option<usize> {
        self.scene.as_ref()?.rails.iter().position(|r| r.hash == rail_hash)
    }

    /// Rails touched by moving these items (indices into the scene rails).
    pub fn rails_touched(&self, hashes: &[u64]) -> Vec<usize> {
        let Some(sc) = &self.scene else { return vec![] };
        let mut out = vec![];
        for h in hashes {
            let rh = scene::rail_of(sc, *h).unwrap_or(*h);
            if let Some(i) = self.rail_index_of(rh)
                && !out.contains(&i)
            {
                out.push(i);
            }
        }
        out
    }

    pub fn gizmo_json(&self) -> Value {
        let Some(doc) = &self.doc else { return json!({"visible": false}) };
        let items: Vec<(Vec3, Quat)> = self.selection.iter().filter_map(|h| doc.transform(*h).map(|(p, q, _)| (p, q))).collect();
        // count: how much is selected, gizmo or not (a whole path has no gizmo)
        let count = self.selection.len();
        if items.is_empty() {
            return json!({"visible": false, "count": count});
        }
        let c = items.iter().map(|x| x.0).sum::<Vec3>() / items.len() as f32;
        let q = if self.gizmo_local && items.len() == 1 { items[0].1 } else { Quat::IDENTITY };
        json!({"visible": true, "p": v3(c), "q": q4(q), "count": count})
    }

    pub fn focus_json(&self, all: bool) -> Value {
        let Some(doc) = &self.doc else { return json!(null) };
        let mut pts: Vec<Vec3> = vec![];
        if all {
            pts.extend(doc.actors().iter().map(|a| Vec3::from(a.vec3("Translate", [0.0; 3]))));
            pts.extend(doc.rails().iter().flat_map(Document::points).map(|p| Vec3::from(p.vec3("Translation", [0.0; 3]))));
        } else {
            for h in &self.selection {
                match doc.find(*h) {
                    Some((Loc::Rail(_), r)) => pts.extend(Document::points(r).iter().map(|p| Vec3::from(p.vec3("Translation", [0.0; 3])))),
                    Some(_) => pts.extend(doc.transform(*h).map(|t| t.0)),
                    None => {}
                }
            }
        }
        if pts.is_empty() {
            return json!(null);
        }
        let mn = pts.iter().fold(Vec3::splat(f32::MAX), |a, b| a.min(*b));
        let mx = pts.iter().fold(Vec3::splat(f32::MIN), |a, b| a.max(*b));
        json!({"c": v3((mn + mx) / 2.0), "r": ((mx - mn).length() / 2.0).max(6.0)})
    }

    // ------------------------------------------------------------ picking and gizmo

    pub fn pick(&mut self, r: &Ray, additive: bool) -> Option<u64> {
        let hit = self.scene.as_ref().and_then(|sc| scene::pick(sc, r));
        if let Some(src) = self.link_pick.take() {
            match hit {
                Some(dst) if dst != src => {
                    let kind = self.link_type.clone();
                    let res = self.doc.as_mut().map(|d| d.add_link(src, dst, &kind)).unwrap_or(Ok(()));
                    match res {
                        Ok(()) => self.log(format!("{kind} link added")),
                        Err(e) => self.log(format!("Link not added: {e}")),
                    }
                }
                _ => self.log("Link cancelled"),
            }
            self.rebuild();
            return hit;
        }
        match hit {
            Some(h) if additive => {
                if let Some(i) = self.selection.iter().position(|x| *x == h) {
                    self.selection.remove(i);
                } else {
                    self.selection.push(h);
                }
            }
            Some(h) => self.selection = vec![h],
            None if !additive => self.selection.clear(),
            None => {}
        }
        self.rebuild();
        hit
    }

    pub fn ground_hit(&self, r: &Ray) -> Option<Vec3> {
        let g = self.ground.as_ref()?;
        g.hit(r, 20000.0).map(|(d, _)| r.origin + r.dir * d)
    }

    pub fn placement(&self, r: &Ray) -> Vec3 {
        self.ground_hit(r).unwrap_or(r.origin + r.dir * 30.0)
    }

    pub fn begin_drag(&mut self, axis: usize, r: &Ray) -> bool {
        let Some(doc) = &mut self.doc else { return false };
        let items: Vec<(u64, Vec3, Quat, Vec3)> =
            self.selection.iter().filter_map(|h| doc.transform(*h).map(|(p, q, s)| (*h, p, q, s))).collect();
        if items.is_empty() {
            return false;
        }
        let center = items.iter().map(|x| x.1).sum::<Vec3>() / items.len() as f32;
        let frame = if self.gizmo_local && items.len() == 1 { items[0].2 } else { Quat::IDENTITY };
        let dir = match axis {
            0 => frame * Vec3::X,
            1 => frame * Vec3::Y,
            2 => frame * Vec3::Z,
            _ => -r.dir,
        };
        let mode = self.gizmo_mode;
        let (start, start_param) = match (mode, axis) {
            (GizmoMode::Rotate, _) | (_, 3) => (ray_plane(r, center, dir).unwrap_or(center), 0.0),
            _ => (center, ray_line_param(r, center, dir).unwrap_or(0.0)),
        };
        doc.push_undo();
        self.drag = Some(Drag { mode, axis, center, dir, start, start_param, items });
        true
    }

    /// Apply a drag; returns the hashes that moved.
    pub fn drag_to(&mut self, r: &Ray) -> Vec<u64> {
        let Some(d) = &self.drag else { return vec![] };
        let Some(doc) = &mut self.doc else { return vec![] };
        let cat = self.cat.clone();
        let ground = self.ground.clone();
        let mut moved = vec![];
        let snap = self.snap_ground && d.mode == GizmoMode::Translate;
        for &(h, p0, q0, s0) in &d.items {
            let (p, q, s) = match (d.mode, d.axis) {
                (GizmoMode::Translate, 3) => {
                    let Some(hit) = ray_plane(r, d.center, d.dir) else { continue };
                    (p0 + (hit - d.start), q0, s0)
                }
                (GizmoMode::Translate, _) => {
                    let Some(t) = ray_line_param(r, d.center, d.dir) else { continue };
                    (p0 + d.dir * (t - d.start_param), q0, s0)
                }
                (GizmoMode::Rotate, _) => {
                    let Some(hit) = ray_plane(r, d.center, d.dir) else { continue };
                    let (a, b) = ((d.start - d.center).normalize_or_zero(), (hit - d.center).normalize_or_zero());
                    let angle = a.cross(b).dot(d.dir).atan2(a.dot(b));
                    let rq = Quat::from_axis_angle(d.dir.normalize(), angle);
                    (d.center + rq * (p0 - d.center), rq * q0, s0)
                }
                (GizmoMode::Scale, _) => {
                    let Some(t) = ray_line_param(r, d.center, d.dir) else { continue };
                    let ratio = if d.start_param.abs() < 1e-3 { 1.0 } else { (t / d.start_param).clamp(0.01, 100.0) };
                    let mut s = s0;
                    if d.axis < 3 {
                        s[d.axis] = s0[d.axis] * ratio;
                    }
                    (p0, q0, s)
                }
            };
            let mut p = p;
            if snap && let (Some(g), Some((loc, e))) = (&ground, doc.find(h)) {
                let lift = if matches!(loc, Loc::Actor(_)) {
                    cat.as_ref().and_then(|c| c.actor(e.str_or("Gyaml", ""))).and_then(|a| a.banc_offset).map(|o| o[1]).unwrap_or(0.0)
                } else {
                    0.0
                };
                if let Some(y) = g.ground_below(p) {
                    p.y = y + lift;
                }
            }
            doc.set_transform(h, p, q, s);
            moved.push(h);
        }
        self.rebuild();
        moved
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    // ------------------------------------------------------------ outliner

    pub fn outliner_json(&self, filter: &str) -> Value {
        let (Some(doc), Some(cat)) = (&self.doc, &self.cat) else { return json!([]) };
        let f = filter.to_lowercase();
        let sel: HashSet<u64> = self.selection.iter().copied().collect();
        let mut rows = vec![];
        let mut groups: BTreeMap<String, Vec<&Byml>> = BTreeMap::new();
        for a in doc.actors() {
            groups.entry(a.str_or("Name", "").to_string()).or_default().push(a);
        }
        rows.push(json!({"kind": "section", "label": "Objects", "count": doc.actors().len(), "key": "s:objects"}));
        for (name, list) in &groups {
            let info = cat.data.actors.get(name);
            let label = info.map(|i| i.label.as_str()).unwrap_or("");
            if !f.is_empty() && !name.to_lowercase().contains(&f) && !label.to_lowercase().contains(&f) {
                continue;
            }
            let key = format!("a:{name}");
            let open = self.expanded.contains(&key);
            let category = info.map(|i| i.category.as_str()).unwrap_or("Other");
            rows.push(json!({"kind": "group", "key": key, "label": short_name(name), "name": name, "sub": label, "count": list.len(), "expanded": open, "thumb": thumb_url(info), "category": category,
                "color": scene::hex(scene::category_color(category)), "icon": if info.and_then(|i| i.model.as_ref()).is_some() { "cube" } else { "draw-cuboid" },
                "selected": list.iter().any(|a| sel.contains(&hash_of(a))), "visible": !self.view.hidden_actor_names.contains(name) && !self.view.hidden_categories.contains(category)}));
            if open {
                for (i, a) in list.iter().enumerate() {
                    let h = hash_of(a);
                    let t = a.vec3("Translate", [0.0; 3]);
                    let dynk: Vec<String> =
                        a.get("Dynamic").and_then(|d| d.as_map()).map(|m| m.keys().cloned().collect()).unwrap_or_default();
                    let at = format!("{:.1}, {:.1}, {:.1}", t[0], t[1], t[2]);
                    rows.push(json!({"kind": "item", "key": format!("h:{h}"), "hash": h.to_string(), "label": format!("#{i}"),
                        "sub": if dynk.is_empty() { at } else { format!("{at}  {}", dynk.join(", ")) }, "selected": sel.contains(&h)}));
                }
            }
        }
        rows.push(json!({"kind": "section", "label": "Paths", "count": doc.rails().len(), "key": "s:paths"}));
        let mut counters: HashMap<String, usize> = HashMap::new();
        for r in doc.rails() {
            let name = r.str_or("Name", "").to_string();
            let n = {
                let c = counters.entry(name.clone()).or_default();
                *c += 1;
                *c
            };
            if !f.is_empty() && !name.to_lowercase().contains(&f) {
                continue;
            }
            let rh = hash_of(r);
            let key = format!("r:{rh}");
            let open = self.expanded.contains(&key);
            let pts = Document::points(r);
            rows.push(json!({"kind": "group", "key": key, "hash": rh.to_string(), "label": format!("{name} {n}"), "sub": if r.get("IsClosed").and_then(|b| b.as_bool()) == Some(true) { "closed loop" } else { "" },
                "count": pts.len(), "expanded": open, "color": scene::hex(scene::rail_color(&name)), "icon": "draw-path",
                "selected": sel.contains(&rh) || pts.iter().any(|p| sel.contains(&hash_of(p))),
                "visible": !self.view.hidden_rails.contains(&rh) && !self.view.hidden_rail_types.contains(&name)}));
            if open {
                for (i, p) in pts.iter().enumerate() {
                    let h = hash_of(p);
                    let dynv: Vec<String> = p
                        .get("Dynamic")
                        .and_then(|d| d.as_map())
                        .map(|m| {
                            m.iter()
                                .map(|(k, v)| match v {
                                    Byml::Bool(b) => format!("{k}={b}"),
                                    x => format!("{k}={}", x.as_f64().map(|f| format!("{f}")).unwrap_or_default()),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let links = p.get("Links").and_then(|l| l.as_array()).map(|l| l.len()).unwrap_or(0);
                    rows.push(json!({"kind": "item", "key": format!("h:{h}"), "hash": h.to_string(),
                        "label": if links > 0 { format!("#{i}  → {links}") } else { format!("#{i}") }, "sub": dynv.join(" "), "selected": sel.contains(&h)}));
                }
            }
        }
        json!(rows)
    }

    /// Expand the outliner group holding the selection (one object or path point), so the
    /// outliner can scroll to it.
    pub fn reveal_selection(&mut self) {
        let (Some(doc), [h]) = (&self.doc, self.selection.as_slice()) else { return };
        let key = match doc.find(*h) {
            Some((Loc::Actor(_), a)) => format!("a:{}", a.str_or("Name", "")),
            Some((Loc::Point(r, _), _)) => format!("r:{}", hash_of(&doc.rails()[r])),
            _ => return,
        };
        self.expanded.insert(key);
    }

    pub fn group_hashes(&self, key: &str) -> Vec<u64> {
        let Some(doc) = &self.doc else { return vec![] };
        if let Some(name) = key.strip_prefix("a:") {
            return doc.actors().iter().filter(|a| a.str_or("Name", "") == name).map(hash_of).collect();
        }
        if let Some(h) = key.strip_prefix("r:").and_then(|h| h.parse().ok()) {
            return vec![h];
        }
        vec![]
    }

    pub fn set_group_visible(&mut self, key: &str, visible: bool) {
        if let Some(name) = key.strip_prefix("a:") {
            if visible {
                self.view.hidden_actor_names.remove(name);
            } else {
                self.view.hidden_actor_names.insert(name.into());
            }
        } else if let Some(h) = key.strip_prefix("r:").and_then(|h| h.parse::<u64>().ok()) {
            if visible {
                self.view.hidden_rails.remove(&h);
            } else {
                self.view.hidden_rails.insert(h);
            }
        }
        self.rebuild();
    }

    // ------------------------------------------------------------ properties

    fn params_json(&self, owner: &Byml, table: &str) -> Value {
        let Some(cat) = &self.cat else { return json!([]) };
        let empty = BTreeMap::new();
        let dynm = owner.get("Dynamic").and_then(|d| d.as_map()).unwrap_or(&empty);
        let mut schema: Vec<&Param> = cat.data.params.get(table).map(|v| v.iter().collect()).unwrap_or_default();
        schema.sort_by(|a, b| a.name.cmp(&b.name));
        let mut rows = vec![];
        let known: HashSet<&str> = schema.iter().map(|p| p.name.as_str()).collect();
        for p in schema {
            let cur = dynm.get(&p.name);
            rows.push(
                json!({"name": p.name, "label": p.label, "type": p.kind, "enumName": p.enum_name, "init": p.init, "initName": p.init_name,
                "set": cur.is_some(), "value": cur.map(display).unwrap_or(Value::Null),
                "suggest": cat.data.seen_values.get(&p.name).cloned().unwrap_or_default()}),
            );
        }
        for (k, v) in dynm {
            if known.contains(k.as_str()) {
                continue;
            }
            rows.push(json!({"name": k, "label": "not in the parameter table", "type": byml_kind(v), "set": true, "value": display(v), "extra": true,
                "suggest": cat.data.seen_values.get(k).cloned().unwrap_or_default()}));
        }
        json!(rows)
    }

    fn links_json(&self, owner: &Byml) -> Value {
        let Some(doc) = &self.doc else { return json!([]) };
        json!(
            owner
                .get("Links")
                .and_then(|l| l.as_array())
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, l)| {
                    let dst = l.get("Dst").and_then(|d| d.as_u64()).unwrap_or(0);
                    let desc = match doc.find(dst) {
                        Some((Loc::Point(r, p), _)) => format!("{} point #{p}", doc.rails()[r].str_or("Name", "")),
                        Some((_, e)) => e.str_or("Name", "").to_string(),
                        None => format!("{dst} (not in this file)"),
                    };
                    json!({"i": i, "type": l.str_or("Name", ""), "target": desc, "dst": dst.to_string(), "found": doc.find(dst).is_some()})
                })
                .collect::<Vec<_>>()
        )
    }

    fn raw_json(owner: &Byml, skip: &[&str]) -> String {
        let mut m = serde_json::Map::new();
        for (k, v) in owner.as_map().into_iter().flatten() {
            if !skip.contains(&k.as_str()) {
                m.insert(k.clone(), typedjson::to_json(v));
            }
        }
        serde_json::to_string_pretty(&Value::Object(m)).unwrap()
    }

    pub const ACTOR_KEEP: [&'static str; 6] = ["Translate", "Rotate", "Scale", "Dynamic", "Links", "Hash"];
    pub const POINT_KEEP: [&'static str; 10] =
        ["Translation", "Rotate", "RectSize", "RectOffset", "Control0", "Control1", "Dynamic", "Links", "Hash", "Parent"];
    pub const RAIL_KEEP: [&'static str; 4] = ["Points", "Dynamic", "Hash", "IsClosed"];

    pub fn selection_json(&self) -> Value {
        let (Some(doc), Some(cat)) = (&self.doc, &self.cat) else { return json!({"kind": "none"}) };
        if self.selection.is_empty() {
            let d = doc;
            let pts: usize = d.rails().iter().map(|r| Document::points(r).len()).sum();
            let collects = d
                .root
                .get("Collects")
                .and_then(|c| c.as_map())
                .map(|m| m.iter().map(|(k, v)| format!("{k} {}", v.as_f64().unwrap_or(0.0))).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            return json!({"kind": "course", "file": d.file, "objects": d.actors().len(), "paths": d.rails().len(), "points": pts, "collects": collects,
                "padEntries": d.pad.as_ref().map(|p| if let Byml::Hash64(m) = p { m.len() } else { 0 }).unwrap_or(0), "padFile": d.pad_file, "fromMod": d.from_mod});
        }
        if self.selection.len() > 1 {
            return json!({"kind": "multi", "count": self.selection.len()});
        }
        let h = self.selection[0];
        let Some((loc, e)) = doc.find(h) else { return json!({"kind": "none"}) };
        match loc {
            Loc::Actor(_) => {
                let info = cat.actor(e.str_or("Gyaml", ""));
                json!({"kind": "actor", "hash": h.to_string(), "name": e.str_or("Name", ""), "label": info.map(|i| i.label.clone()), "thumb": thumb_url(info),
                    "category": info.map(|i| i.category.clone()), "collect": info.and_then(|i| i.collect.clone()), "gyaml": e.str_or("Gyaml", ""),
                    "hasModel": info.and_then(|i| i.model.as_ref()).is_some(), "instanceId": e.str_or("InstanceId", ""),
                    "translate": e.vec3("Translate", [0.0; 3]), "rotate": deg(e.vec3("Rotate", [0.0; 3])), "scale": e.vec3("Scale", [1.0; 3]),
                    "paramTable": info.map(|i| i.banc_param.clone()), "params": self.params_json(e, info.map(|i| i.banc_param.as_str()).unwrap_or("")),
                    "links": self.links_json(e), "raw": Self::raw_json(e, &Self::ACTOR_KEEP)})
            }
            Loc::Point(r, i) => {
                let rail = &doc.rails()[r];
                let name = rail.str_or("Name", "");
                let def = cat.data.rails.get(name);
                let pad = match &doc.pad {
                    Some(Byml::Hash64(m)) => match m.get(&h) {
                        Some(Byml::Hash64(groups)) => groups
                            .iter()
                            .flat_map(|(g, vals)| {
                                vals.as_map()
                                    .into_iter()
                                    .flatten()
                                    .map(move |(k, v)| json!({"group": g.to_string(), "key": k, "value": display(v), "type": byml_kind(v)}))
                            })
                            .collect::<Vec<_>>(),
                        _ => vec![],
                    },
                    _ => vec![],
                };
                let opt = |k: &str| e.get(k).map(|v| json!(vec3(Some(v), [0.0; 3])));
                let two = |k: &str| {
                    e.get(k).and_then(|v| v.as_array()).map(|a| json!(a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()))
                };
                json!({"kind": "point", "hash": h.to_string(), "rail": name, "railHash": hash_of(rail).to_string(), "index": i, "count": Document::points(rail).len(),
                    "translate": e.vec3("Translation", [0.0; 3]), "rotate": deg(e.vec3("Rotate", [0.0; 3])),
                    "rectSize": two("RectSize"), "rectOffset": two("RectOffset"), "control0": opt("Control0"), "control1": opt("Control1"),
                    "params": self.params_json(e, def.map(|d| d.point_param.as_str()).unwrap_or("")), "paramTable": def.map(|d| d.point_param.clone()),
                    "pad": pad, "links": self.links_json(e), "raw": Self::raw_json(e, &Self::POINT_KEEP)})
            }
            Loc::Rail(_) => {
                let name = e.str_or("Name", "");
                let def = cat.data.rails.get(name);
                json!({"kind": "rail", "hash": h.to_string(), "name": name, "count": Document::points(e).len(),
                    "closed": e.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false), "railType": def.map(|d| d.rail_type.clone()),
                    "params": self.params_json(e, def.map(|d| d.rail_param.as_str()).unwrap_or("")), "paramTable": def.map(|d| d.rail_param.clone()),
                    "raw": Self::raw_json(e, &Self::RAIL_KEEP)})
            }
        }
    }

    // ------------------------------------------------------------ assets

    pub fn asset_json(&self, filter: &str, category: &str, placed_only: bool) -> Value {
        let Some(cat) = &self.cat else { return json!([]) };
        let f = filter.to_lowercase();
        let in_doc: HashSet<&str> =
            self.doc.as_ref().map(|d| d.actors().iter().map(|a| a.str_or("Name", "")).collect()).unwrap_or_default();
        let mut out: Vec<(bool, &String, &ActorInfo)> = cat
            .data
            .actors
            .iter()
            .filter(|(n, a)| {
                (!placed_only || a.placed)
                    && (category.is_empty() || a.category == category)
                    && (f.is_empty() || n.to_lowercase().contains(&f) || a.label.to_lowercase().contains(&f))
            })
            .map(|(n, a)| (in_doc.contains(n.as_str()), n, a))
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        json!(out.iter().take(600).map(|(here, n, a)| {
            json!({"name": n, "short": short_name(n), "label": a.label, "category": a.category, "model": a.model.is_some(), "inCourse": here,
                   "color": scene::hex(scene::category_color(&a.category)), "thumb": thumb_url(Some(a))})
        }).collect::<Vec<_>>())
    }

    /// Export the saved mod for ns2jb-modloader (see `Workspace::export_ns2jb`).
    pub fn export_ns2jb(&self) -> Value {
        let res = self.ws().export_ns2jb(&self.mod_name, &self.cfg.ns2jb);
        json!({"ok": res.ok, "errors": res.errors, "notes": res.notes, "dir": res.mod_romfs.display().to_string(),
            "files": res.written.iter().map(|w| format!("romfs/{w}")).collect::<Vec<_>>()})
    }

    pub fn categories_json(&self) -> Value {
        let Some(cat) = &self.cat else { return json!([]) };
        let mut c: Vec<&String> =
            cat.data.actors.values().map(|a| &a.category).filter(|c| !c.is_empty()).collect::<HashSet<_>>().into_iter().collect();
        c.sort();
        json!(c)
    }

    pub fn rail_types_json(&self) -> Value {
        let Some(cat) = &self.cat else { return json!([]) };
        let mut counts: HashMap<&str, usize> = HashMap::new();
        if let Some(d) = &self.doc {
            for r in d.rails() {
                *counts.entry(r.str_or("Name", "")).or_default() += 1;
            }
        }
        let mut v: Vec<Value> = cat
            .data
            .rails
            .iter()
            .map(|(n, d)| {
                json!({"name": n, "type": d.rail_type, "count": counts.get(n.as_str()).copied().unwrap_or(0),
            "color": scene::hex(scene::rail_color(n))})
            })
            .collect();
        v.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()).then(a["name"].as_str().cmp(&b["name"].as_str())));
        json!(v)
    }

    pub fn view_json(&self) -> Value {
        let (Some(doc), Some(cat)) = (&self.doc, &self.cat) else { return json!({"rails": [], "categories": []}) };
        let mut rails: Vec<&str> = doc.rails().iter().map(|r| r.str_or("Name", "")).collect::<HashSet<_>>().into_iter().collect();
        rails.sort();
        let mut cats: Vec<String> = doc
            .actors()
            .iter()
            .map(|a| cat.actor(a.str_or("Gyaml", "")).map(|i| i.category.clone()).unwrap_or("Other".into()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        json!({"rails": rails.iter().map(|r| json!({"name": r, "visible": !self.view.hidden_rail_types.contains(*r), "color": scene::hex(scene::rail_color(r))})).collect::<Vec<_>>(),
               "categories": cats.iter().map(|c| json!({"name": c, "visible": !self.view.hidden_categories.contains(c), "color": scene::hex(scene::category_color(c))})).collect::<Vec<_>>(),
               "stage": self.show_stage, "models": self.view.models, "xray": self.xray, "widths": self.view.widths, "snap": self.snap_ground})
    }

    // ------------------------------------------------------------ save

    pub fn save(&mut self) -> Value {
        let (Some(doc), Some(cat)) = (&self.doc, &self.cat) else { return json!({"ok": false, "errors": ["no course open"]}) };
        let res = self.ws().save(&doc.file, &doc.root, doc.pad.as_ref(), &self.mod_name, &doc.deleted, Some(cat));
        let emu = emulator_load_dir();
        let mod_dir = res.mod_romfs.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let install = if mod_dir.starts_with(&emu) {
            format!("Already in the emulator's mod folder: {}", crate::config::tilde(&mod_dir.display().to_string()))
        } else {
            let (emu, md) = (crate::config::shell_path(&emu), crate::config::shell_path(&mod_dir));
            format!("mkdir -p \"{emu}\" && cp -r \"{md}\" \"{emu}/\"")
        };
        // Switch 2: copy the saved mod into ns2jb-modloader's SD layout too
        let ns2jb = (res.ok && self.cfg.ns2jb_on_save && !self.cfg.ns2jb.as_os_str().is_empty()).then(|| self.export_ns2jb());
        let v = json!({"ok": res.ok, "errors": res.errors, "warnings": res.warnings, "notes": res.notes, "mod": self.mod_name, "ns2jb": ns2jb,
            "written": res.written.iter().map(|w| res.mod_romfs.join(w).display().to_string()).collect::<Vec<_>>(), "install": install,
            "files": res.written.iter().map(|w| format!("romfs/{w}")).collect::<Vec<_>>(), "modDir": mod_dir.display().to_string(),
            "installed": mod_dir.starts_with(&emu)});
        if res.ok {
            if let Some(d) = &mut self.doc {
                d.dirty = false;
            }
            self.log(format!("Saved {} file(s) to mod \"{}\" ({})", res.written.len(), self.mod_name, mod_dir.display()));
            for n in &res.notes {
                self.log(format!("  {n}"));
            }
        } else {
            self.log(format!("Not saved: {}", res.errors.join("; ")));
        }
        v
    }
}

/// file:// URL of the object's cached thumbnail, or "" when it has no model or none is built yet.
fn thumb_url(info: Option<&ActorInfo>) -> String {
    let Some(p) = info.and_then(|i| i.model.as_ref()).map(|m| crate::thumbs::thumb_path(m)) else { return String::new() };
    if p.exists() { format!("file://{}", p.display()) } else { String::new() }
}

pub fn short_name(n: &str) -> String {
    let s = n.strip_prefix("CmnObj_").or_else(|| n.strip_prefix("SystemObj_")).unwrap_or(n);
    let s = s.strip_suffix("_00").unwrap_or(s);
    s.replace("000_A", "").replace('_', " ")
}

pub fn display(v: &Byml) -> Value {
    match v {
        Byml::Array(a) => json!(a.iter().map(|x| x.as_f64().map(|f| json!(f)).unwrap_or(json!(null))).collect::<Vec<_>>()),
        other => typedjson::to_plain(other),
    }
}

pub fn byml_kind(v: &Byml) -> &'static str {
    match v {
        Byml::Bool(_) => "bool",
        Byml::I32(_) => "s32",
        Byml::U32(_) => "u32",
        Byml::F32(_) => "f32",
        Byml::F64(_) => "f64",
        Byml::I64(_) => "s64",
        Byml::U64(_) => "u64",
        Byml::Str(_) => "str",
        Byml::Array(a) if a.len() == 3 => "vec3",
        _ => "other",
    }
}

/// 32-bit FNV-1a, to name a cache folder after a path.
fn fnv1a(b: &[u8]) -> u32 {
    b.iter().fold(0x811c9dc5u32, |h, &x| (h ^ x as u32).wrapping_mul(0x01000193))
}

//! The `Editor` QObject that QML talks to. Thin wrappers over `state::State`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qvector3d.h");
        type QVector3D = cxx_qt_lib::QVector3D;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, ready)]
        #[qproperty(bool, busy)]
        #[qproperty(QString, status)]
        #[qproperty(QString, course_title)]
        #[qproperty(QString, mod_name, READ, WRITE = set_mod_name_from_qml, NOTIFY)]
        #[qproperty(bool, dirty)]
        #[qproperty(bool, can_undo)]
        #[qproperty(bool, can_redo)]
        #[qproperty(QString, gizmo_mode)]
        #[qproperty(bool, gizmo_local)]
        #[qproperty(bool, link_picking)]
        #[qproperty(bool, ground_ready)]
        #[qproperty(i32, revision)]
        #[qproperty(i32, scene_revision)]
        #[qproperty(i32, rails_revision)]
        #[qproperty(i32, stages_revision)]
        #[qproperty(i32, overlay_revision)]
        #[qproperty(i32, models_loaded)]
        #[qproperty(i32, models_wanted)]
        #[qproperty(i32, thumbs_done)]
        #[qproperty(i32, thumbs_total)]
        #[qproperty(QString, console_text)]
        #[qproperty(QString, romfs_path)]
        #[qproperty(QString, models_path)]
        #[qproperty(QString, mods_path)]
        #[qproperty(QString, ns2jb_path, cxx_name = "ns2jbPath")]
        #[qproperty(bool, ns2jb_on_save, cxx_name = "ns2jbOnSave")]
        type Editor = super::EditorRust;

        #[qsignal]
        fn transforms_moved(self: Pin<&mut Editor>, json: QString);
        #[qsignal]
        fn rails_moved(self: Pin<&mut Editor>, json: QString);

        fn set_mod_name_from_qml(self: Pin<&mut Editor>, name: QString);

        #[qinvokable]
        fn start(self: Pin<&mut Editor>);
        #[qinvokable]
        fn course_list(self: &Editor, filter: &QString) -> QString;
        #[qinvokable]
        fn mod_list(self: &Editor) -> QString;
        #[qinvokable]
        fn startup_course(self: &Editor) -> QString;
        #[qinvokable]
        fn open_course(self: Pin<&mut Editor>, file: &QString, mod_name: &QString) -> bool;
        #[qinvokable]
        fn save(self: Pin<&mut Editor>) -> QString;
        #[qinvokable]
        fn undo(self: Pin<&mut Editor>);
        #[qinvokable]
        fn redo(self: Pin<&mut Editor>);

        #[qinvokable]
        fn stages(self: &Editor) -> QString;
        #[qinvokable]
        fn actor_groups(self: &Editor) -> QString;
        #[qinvokable]
        fn boxes(self: &Editor) -> QString;
        #[qinvokable]
        fn part_count(self: &Editor, rel: &QString) -> i32;
        #[qinvokable]
        fn mesh_part(self: &Editor, rel: &QString, i: i32) -> QVariant;
        #[qinvokable]
        fn primitive(self: &Editor, shape: &QString) -> QVariant;
        #[qinvokable]
        fn rail_count(self: &Editor) -> i32;
        #[qinvokable]
        fn rail_mesh(self: &Editor, i: i32) -> QVariant;
        #[qinvokable]
        fn links_mesh(self: &Editor) -> QVariant;
        #[qinvokable]
        fn selection_mesh(self: &Editor) -> QVariant;
        #[qinvokable]
        fn gizmo(self: &Editor) -> QString;
        #[qinvokable]
        fn focus_target(self: &Editor, all: bool) -> QString;

        #[qinvokable]
        fn pick(self: Pin<&mut Editor>, origin: &QVector3D, dir: &QVector3D, additive: bool) -> bool;
        #[qinvokable]
        fn ground_at(self: &Editor, origin: &QVector3D, dir: &QVector3D) -> QString;
        #[qinvokable]
        fn begin_gizmo(self: Pin<&mut Editor>, axis: i32, origin: &QVector3D, dir: &QVector3D) -> bool;
        #[qinvokable]
        fn drag_gizmo(self: Pin<&mut Editor>, origin: &QVector3D, dir: &QVector3D);
        #[qinvokable]
        fn end_gizmo(self: Pin<&mut Editor>);

        #[qinvokable]
        fn outliner(self: &Editor, filter: &QString) -> QString;
        #[qinvokable]
        fn toggle_expand(self: Pin<&mut Editor>, key: &QString);
        #[qinvokable]
        fn set_group_visible(self: Pin<&mut Editor>, key: &QString, visible: bool);
        #[qinvokable]
        fn select_hash(self: Pin<&mut Editor>, hash: &QString, additive: bool);
        #[qinvokable]
        fn select_group(self: Pin<&mut Editor>, key: &QString);
        #[qinvokable]
        fn clear_selection(self: Pin<&mut Editor>);
        #[qinvokable]
        fn selection(self: &Editor) -> QString;

        #[qinvokable]
        fn set_vector(self: Pin<&mut Editor>, key: &QString, x: f64, y: f64, z: f64);
        #[qinvokable]
        fn set_param(self: Pin<&mut Editor>, name: &QString, kind: &QString, text: &QString) -> QString;
        #[qinvokable]
        fn remove_param(self: Pin<&mut Editor>, name: &QString);
        #[qinvokable]
        fn set_raw(self: Pin<&mut Editor>, text: &QString) -> QString;
        #[qinvokable]
        fn set_closed(self: Pin<&mut Editor>, closed: bool);
        #[qinvokable]
        fn toggle_curve(self: Pin<&mut Editor>);
        #[qinvokable]
        fn set_pad(self: Pin<&mut Editor>, group: &QString, key: &QString, kind: &QString, text: &QString) -> QString;
        #[qinvokable]
        fn remove_link(self: Pin<&mut Editor>, index: i32);
        #[qinvokable]
        fn start_link(self: Pin<&mut Editor>, kind: &QString);
        #[qinvokable]
        fn cancel_link(self: Pin<&mut Editor>);

        #[qinvokable]
        fn add_actor(self: Pin<&mut Editor>, name: &QString, origin: &QVector3D, dir: &QVector3D, yaw: f64) -> bool;
        #[qinvokable]
        fn duplicate(self: Pin<&mut Editor>, rx: f64, rz: f64);
        #[qinvokable]
        fn delete_selected(self: Pin<&mut Editor>);
        #[qinvokable]
        fn drop_to_ground(self: Pin<&mut Editor>);
        #[qinvokable]
        fn insert_point(self: Pin<&mut Editor>);
        #[qinvokable]
        fn new_rail(self: Pin<&mut Editor>, name: &QString, origin: &QVector3D, dir: &QVector3D, yaw: f64) -> bool;
        #[qinvokable]
        fn reverse_rail(self: Pin<&mut Editor>);
        #[qinvokable]
        fn change_object(self: Pin<&mut Editor>, name: &QString) -> QString;

        #[qinvokable]
        fn assets(self: &Editor, filter: &QString, category: &QString, placed_only: bool) -> QString;
        #[qinvokable]
        fn categories(self: &Editor) -> QString;
        #[qinvokable]
        fn rail_types(self: &Editor) -> QString;
        #[qinvokable]
        fn view_state(self: &Editor) -> QString;
        #[qinvokable]
        fn set_view_option(self: Pin<&mut Editor>, name: &QString, on: bool);
        #[qinvokable]
        fn set_rail_type_visible(self: Pin<&mut Editor>, name: &QString, on: bool);
        #[qinvokable]
        fn set_category_visible(self: Pin<&mut Editor>, name: &QString, on: bool);
        #[qinvokable]
        fn set_gizmo(self: Pin<&mut Editor>, mode: &QString, local: bool);
        #[qinvokable]
        fn apply_settings(self: Pin<&mut Editor>, romfs: &QString, models: &QString, mods: &QString) -> QString;
        #[qinvokable]
        fn set_export(self: Pin<&mut Editor>, ns2jb: &QString, on_save: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "exportNs2jb"]
        fn export_ns2jb(self: Pin<&mut Editor>) -> QString;
        #[qinvokable]
        fn open_folder(self: &Editor, which: &QString);
        #[qinvokable]
        fn build_thumbnails(self: Pin<&mut Editor>);
    }

    impl cxx_qt::Threading for Editor {}
}

use crate::config::{Config, cache_dir};
use crate::state::{GizmoMode, State};
use crate::thumbs;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant, QVector2D, QVector3D, QVector4D};
use glam::Vec3;
use mkwtrack::banc::hash_of;
use mkwtrack::catalog::Catalog;
use mkwtrack::doc::{Document, LINK_TYPES, Loc};
use mkwtrack::raycast::Ray;
use mkwtrack::scene::{self, Lines, Tris};
use mkwtrack::{byml::Byml, fbx, typedjson};
use serde_json::json;
use std::sync::Arc;

pub struct EditorRust {
    ready: bool,
    busy: bool,
    status: QString,
    course_title: QString,
    mod_name: QString,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    gizmo_mode: QString,
    gizmo_local: bool,
    link_picking: bool,
    ground_ready: bool,
    revision: i32,
    scene_revision: i32,
    rails_revision: i32,
    stages_revision: i32,
    overlay_revision: i32,
    models_loaded: i32,
    models_wanted: i32,
    thumbs_done: i32,
    thumbs_total: i32,
    console_text: QString,
    romfs_path: QString,
    models_path: QString,
    mods_path: QString,
    ns2jb_path: QString,
    ns2jb_on_save: bool,
    st: State,
}

impl Default for EditorRust {
    fn default() -> Self {
        let cfg = Config::load();
        EditorRust {
            ready: false,
            busy: false,
            status: QString::from("Starting…"),
            course_title: QString::default(),
            mod_name: QString::from("MyTrackMod"),
            dirty: false,
            can_undo: false,
            can_redo: false,
            gizmo_mode: QString::from("translate"),
            gizmo_local: true,
            link_picking: false,
            ground_ready: false,
            revision: 0,
            scene_revision: 0,
            rails_revision: 0,
            stages_revision: 0,
            overlay_revision: 0,
            models_loaded: 0,
            models_wanted: 0,
            thumbs_done: 0,
            thumbs_total: 0,
            console_text: QString::default(),
            romfs_path: q(crate::config::tilde(&cfg.romfs.display().to_string())),
            models_path: q(crate::config::tilde(&cfg.models.display().to_string())),
            mods_path: q(crate::config::tilde(&cfg.mods.display().to_string())),
            ns2jb_path: q(crate::config::tilde(&cfg.ns2jb.display().to_string())),
            ns2jb_on_save: cfg.ns2jb_on_save,
            st: State::new(cfg),
        }
    }
}

/// What QML has to redraw after a change: the panels (outliner, properties), the object
/// instances, the path meshes, and the overlay (gizmo, links, selection outline).
#[derive(Clone, Copy)]
struct Refresh {
    panels: bool,
    objects: bool,
    rails: bool,
    overlay: bool,
}

impl Refresh {
    const ALL: Refresh = Refresh { panels: true, objects: true, rails: true, overlay: true };
    /// selection changes recolour path markers and move the gizmo
    const SELECTION: Refresh = Refresh { panels: true, objects: false, rails: true, overlay: true };
    const GIZMO: Refresh = Refresh { panels: true, objects: false, rails: false, overlay: true };
    const PANELS: Refresh = Refresh { panels: true, objects: false, rails: false, overlay: false };
    const OBJECTS: Refresh = Refresh { panels: true, objects: true, rails: false, overlay: true };
}

fn q(s: impl AsRef<str>) -> QString {
    QString::from(s.as_ref())
}

fn ray(origin: &QVector3D, dir: &QVector3D) -> Ray {
    let v = |q: &QVector3D| Vec3::new(q.x(), q.y(), q.z());
    Ray { origin: v(origin), dir: v(dir).normalize_or(Vec3::NEG_Z) }
}

fn map(entries: Vec<(&str, QVariant)>) -> QVariant {
    let mut m = QMap::<QMapPair_QString_QVariant>::default();
    for (k, v) in entries {
        m.insert(q(k), v);
    }
    QVariant::from(&m)
}

fn vec3_list(v: &[[f32; 3]]) -> QVariant {
    let mut l = QList::<QVariant>::default();
    l.reserve(v.len() as isize);
    for p in v {
        l.append(QVariant::from(&QVector3D::new(p[0], p[1], p[2])));
    }
    QVariant::from(&l)
}

fn vec4_list(v: &[[f32; 4]]) -> QVariant {
    let mut l = QList::<QVariant>::default();
    l.reserve(v.len() as isize);
    for p in v {
        l.append(QVariant::from(&QVector4D::new(p[0], p[1], p[2], p[3])));
    }
    QVariant::from(&l)
}

fn lines_variant(l: &Lines) -> QVariant {
    map(vec![
        ("positions", vec3_list(&l.positions)),
        ("colors", vec4_list(&l.colors)),
        ("count", QVariant::from(&(l.positions.len() as i32))),
    ])
}

fn tris_variant(t: &Tris) -> QVariant {
    map(vec![
        ("positions", vec3_list(&t.positions)),
        ("colors", vec4_list(&t.colors)),
        ("count", QVariant::from(&(t.positions.len() as i32))),
    ])
}

impl qobject::Editor {
    fn st(&self) -> &State {
        &self.rust().st
    }

    fn stm(self: Pin<&mut Self>) -> &mut State {
        &mut self.rust_mut().get_mut().st
    }

    /// Bump the revision counters QML watches, then sync the flags.
    fn bump(mut self: Pin<&mut Self>, r: Refresh) {
        if r.panels {
            let v = *self.revision() + 1;
            self.as_mut().set_revision(v);
        }
        if r.objects {
            let v = *self.scene_revision() + 1;
            self.as_mut().set_scene_revision(v);
        }
        if r.rails {
            let v = *self.rails_revision() + 1;
            self.as_mut().set_rails_revision(v);
        }
        if r.overlay {
            let v = *self.overlay_revision() + 1;
            self.as_mut().set_overlay_revision(v);
        }
        self.as_mut().sync_flags();
    }

    fn sync_flags(mut self: Pin<&mut Self>) {
        let (dirty, cu, cr, link) = {
            let s = self.st();
            match &s.doc {
                Some(d) => (d.dirty, d.can_undo(), d.can_redo(), s.link_pick.is_some()),
                None => (false, false, false, false),
            }
        };
        self.as_mut().set_dirty(dirty);
        self.as_mut().set_can_undo(cu);
        self.as_mut().set_can_redo(cr);
        self.as_mut().set_link_picking(link);
        let console = self.st().console.iter().rev().take(200).rev().cloned().collect::<Vec<_>>().join("\n");
        self.as_mut().set_console_text(q(console));
        let last = self.st().console.back().cloned().unwrap_or_default();
        self.as_mut().set_status(q(last));
    }

    /// Structural change: everything that depends on the document is rebuilt.
    fn changed(mut self: Pin<&mut Self>) {
        self.as_mut().stm().rebuild();
        self.bump(Refresh::ALL);
    }

    fn log(mut self: Pin<&mut Self>, msg: impl Into<String>) {
        self.as_mut().stm().log(msg);
        self.sync_flags();
    }

    fn set_mod_name_from_qml(mut self: Pin<&mut Self>, name: QString) {
        self.as_mut().stm().mod_name = name.to_string();
        self.as_mut().rust_mut().get_mut().mod_name = name;
        self.mod_name_changed();
    }

    // ------------------------------------------------------------ startup

    fn start(mut self: Pin<&mut Self>) {
        if let Err(e) = self.st().cfg.valid() {
            self.as_mut().log(e);
            self.as_mut().set_busy(false);
            return;
        }
        self.as_mut().set_busy(true);
        self.as_mut().set_status(q("Reading the game catalog…"));
        let qt = self.qt_thread();
        let cfg = self.st().cfg.clone();
        let cache = self.st().catalog_cache();
        std::thread::spawn(move || {
            let qt2 = qt.clone();
            let progress = move |m: &str| {
                let m = m.to_string();
                let _ = qt2.queue(move |ed| ed.set_status(q(m)));
            };
            let t = std::time::Instant::now();
            let res = Catalog::load(&cfg.romfs, &cfg.models, &cache, &progress);
            let took = t.elapsed();
            let _ = qt.queue(move |mut ed| {
                ed.as_mut().set_busy(false);
                match res {
                    Ok(cat) => {
                        let n = cat.data.actors.len();
                        ed.as_mut().stm().cat = Some(Arc::new(cat));
                        ed.as_mut().log(format!("Game catalog ready: {n} object types ({:.1} s)", took.as_secs_f32()));
                        ed.as_mut().set_ready(true);
                    }
                    Err(e) => ed.as_mut().log(format!("Could not read the game files: {e}")),
                }
            });
        });
    }

    fn startup_course(&self) -> QString {
        q(self.st().cfg.open.clone().unwrap_or_default())
    }

    fn course_list(&self, filter: &QString) -> QString {
        q(self.st().course_list(&filter.to_string()).to_string())
    }

    fn mod_list(&self) -> QString {
        q(json!(self.st().ws().mods()).to_string())
    }

    fn open_course(mut self: Pin<&mut Self>, file: &QString, mod_name: &QString) -> bool {
        let m = mod_name.to_string();
        let res = self.as_mut().stm().open(&file.to_string(), Some(&m));
        if let Err(e) = res {
            self.as_mut().log(format!("Could not open {file}: {e}"));
            return false;
        }
        let title = {
            let d = self.st().doc.as_ref().unwrap();
            format!(
                "{}{}",
                d.file.trim_start_matches("Banc/"),
                if d.from_mod { format!("  [{}]", self.st().mod_name) } else { String::new() }
            )
        };
        self.as_mut().set_course_title(q(title));
        let mn = self.st().mod_name.clone();
        self.as_mut().rust_mut().get_mut().mod_name = q(&mn);
        self.as_mut().mod_name_changed();
        self.as_mut().set_ground_ready(false);
        if let Some(sel) = self.st().cfg.select.clone() {
            let d = self.st().doc.as_ref().unwrap();
            let hs: Vec<u64> = match sel.split_once('#') {
                // "LapPathGraph#3": point 3 of the first path with that name
                Some((rail, i)) => d
                    .rails()
                    .iter()
                    .find(|r| r.str_or("Name", "") == rail)
                    .and_then(|r| Document::points(r).get(i.parse::<usize>().unwrap_or(0)))
                    .map(hash_of)
                    .into_iter()
                    .collect(),
                None => d.actors().iter().filter(|a| a.str_or("Name", "") == sel).map(hash_of).take(1).collect(),
            };
            self.as_mut().stm().selection = hs;
            self.as_mut().stm().reveal_selection();
            self.as_mut().stm().rebuild();
        }
        self.as_mut().changed();
        let v = *self.stages_revision() + 1;
        self.as_mut().set_stages_revision(v);
        self.load_models();
        true
    }

    /// Parse the course's FBX models on a worker thread: stages first (each shown as it
    /// arrives, then the ground BVH), then object models.
    fn load_models(mut self: Pin<&mut Self>) {
        let wanted = self.st().wanted_models();
        let cache = self.st().models.clone();
        let models_dir = self.st().cfg.models.clone();
        let stages = self.st().stages.clone();
        let generation = self.st().load_gen;
        self.as_mut().set_models_wanted(wanted.len() as i32);
        let already = wanted.iter().filter(|(m, _)| cache.has(m)).count();
        self.as_mut().set_models_loaded(already as i32);
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let mut loaded = already;
            let mut stage_done = false;
            let finish_stages = |cache: &crate::state::ModelCache| Arc::new(scene::stage_bvh(&stages, |m| cache.get(m)));
            for (i, (rel, is_stage)) in wanted.iter().enumerate() {
                if !*is_stage && !stage_done {
                    stage_done = true;
                    let bvh = finish_stages(&cache);
                    let _ = qt.queue(move |mut ed| {
                        if ed.st().load_gen == generation {
                            let n = bvh.len();
                            ed.as_mut().stm().ground = Some(bvh);
                            ed.as_mut().set_ground_ready(true);
                            ed.as_mut().log(format!("Stage surface ready for drop-to-ground ({n} triangles)"));
                        }
                    });
                }
                if !cache.has(rel) {
                    let res = std::fs::read(models_dir.join(rel)).map_err(mkwtrack::Error::from).and_then(|d| fbx::load_model(&d));
                    match res {
                        Ok(m) => {
                            cache.map.lock().unwrap().insert(rel.clone(), Arc::new(m));
                        }
                        Err(e) => {
                            let msg = format!("Model {rel}: {e}");
                            let _ = qt.queue(move |ed| ed.log(msg));
                        }
                    }
                    loaded += 1;
                }
                let last = i + 1 == wanted.len();
                let is_stage = *is_stage;
                let n = loaded as i32;
                if is_stage || last || n % 12 == 0 {
                    let _ = qt.queue(move |mut ed| {
                        if ed.st().load_gen != generation {
                            return;
                        }
                        ed.as_mut().set_models_loaded(n);
                        if is_stage {
                            let v = *ed.stages_revision() + 1;
                            ed.as_mut().set_stages_revision(v);
                        } else {
                            ed.as_mut().stm().rebuild();
                            let v = *ed.scene_revision() + 1;
                            ed.as_mut().set_scene_revision(v);
                        }
                    });
                }
            }
            if !stage_done {
                let bvh = finish_stages(&cache);
                let _ = qt.queue(move |mut ed| {
                    if ed.st().load_gen == generation {
                        ed.as_mut().stm().ground = Some(bvh);
                        ed.as_mut().set_ground_ready(true);
                    }
                });
            }
        });
    }

    fn save(mut self: Pin<&mut Self>) -> QString {
        let v = self.as_mut().stm().save();
        self.as_mut().sync_flags();
        let v2 = *self.revision() + 1;
        self.as_mut().set_revision(v2);
        q(crate::config::tilde(&v.to_string()))
    }

    fn undo(mut self: Pin<&mut Self>) {
        let ok = self.as_mut().stm().doc.as_mut().map(|d| d.undo()).unwrap_or(false);
        if ok {
            let st = self.as_mut().stm();
            let doc = st.doc.as_ref().unwrap();
            st.selection.retain(|h| doc.index.contains_key(h));
            self.as_mut().log("Undone");
            self.changed();
        }
    }

    fn redo(mut self: Pin<&mut Self>) {
        let ok = self.as_mut().stm().doc.as_mut().map(|d| d.redo()).unwrap_or(false);
        if ok {
            let st = self.as_mut().stm();
            let doc = st.doc.as_ref().unwrap();
            st.selection.retain(|h| doc.index.contains_key(h));
            self.as_mut().log("Redone");
            self.changed();
        }
    }

    // ------------------------------------------------------------ scene data

    fn stages(&self) -> QString {
        q(self.st().stages_json().to_string())
    }

    fn actor_groups(&self) -> QString {
        q(self.st().actor_groups_json().to_string())
    }

    fn boxes(&self) -> QString {
        q(self.st().boxes_json().to_string())
    }

    fn part_count(&self, rel: &QString) -> i32 {
        self.st().models.get(&rel.to_string()).map(|m| m.parts.len() as i32).unwrap_or(0)
    }

    fn mesh_part(&self, rel: &QString, i: i32) -> QVariant {
        let rel = rel.to_string();
        let Some(m) = self.st().models.get(&rel) else { return QVariant::default() };
        let Some(p) = m.parts.get(i as usize) else { return QVariant::default() };
        let mut uvs = QList::<QVariant>::default();
        uvs.reserve(p.uvs.len() as isize);
        for v in &p.uvs {
            uvs.append(QVariant::from(&QVector2D::new(v[0], v[1])));
        }
        let mut idx = QList::<i32>::default();
        idx.reserve(p.indices.len() as isize);
        for &x in &p.indices {
            idx.append(x as i32);
        }
        let dir = self.st().cfg.models.join(&rel).parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let tex = p.texture.as_ref().and_then(|t| {
            let a = dir.join(t);
            if a.exists() {
                return Some(a);
            }
            let b = dir.join(format!("{}.png", t.rsplit_once('.').map(|x| x.0).unwrap_or(t)));
            b.exists().then_some(b)
        });
        map(vec![
            ("positions", vec3_list(&p.positions)),
            ("normals", vec3_list(&p.normals)),
            ("uvs", QVariant::from(&uvs)),
            ("indexes", QVariant::from(&idx)),
            ("texture", QVariant::from(&q(tex.map(|t| format!("file://{}", t.display())).unwrap_or_default()))),
            ("cutout", QVariant::from(&p.cutout)),
        ])
    }

    fn primitive(&self, shape: &QString) -> QVariant {
        let (p, n) = scene::primitive(&shape.to_string());
        map(vec![("positions", vec3_list(&p)), ("normals", vec3_list(&n))])
    }

    fn rail_count(&self) -> i32 {
        self.st().scene.as_ref().map(|s| s.rails.len() as i32).unwrap_or(0)
    }

    fn rail_mesh(&self, i: i32) -> QVariant {
        let Some(r) = self.st().scene.as_ref().and_then(|s| s.rails.get(i as usize)) else { return QVariant::default() };
        map(vec![
            ("lines", lines_variant(&r.lines)),
            ("markers", tris_variant(&r.markers)),
            ("hash", QVariant::from(&q(r.hash.to_string()))),
            ("name", QVariant::from(&q(&r.name))),
        ])
    }

    fn links_mesh(&self) -> QVariant {
        match &self.st().doc {
            Some(d) => lines_variant(&scene::links(d)),
            None => lines_variant(&Lines::default()),
        }
    }

    fn selection_mesh(&self) -> QVariant {
        let s = self.st();
        match (&s.doc, &s.cat) {
            (Some(d), Some(c)) => lines_variant(&scene::selection_outline(d, c, &s.selection.iter().copied().collect())),
            _ => lines_variant(&Lines::default()),
        }
    }

    fn gizmo(&self) -> QString {
        q(self.st().gizmo_json().to_string())
    }

    fn focus_target(&self, all: bool) -> QString {
        q(self.st().focus_json(all).to_string())
    }

    // ------------------------------------------------------------ picking and gizmo

    fn pick(mut self: Pin<&mut Self>, origin: &QVector3D, dir: &QVector3D, additive: bool) -> bool {
        let was_link = self.st().link_pick.is_some();
        let hit = self.as_mut().stm().pick(&ray(origin, dir), additive);
        if hit.is_some() && !was_link {
            self.as_mut().stm().reveal_selection();
        }
        if was_link {
            self.as_mut().changed();
        } else {
            self.as_mut().bump(Refresh::SELECTION);
        }
        hit.is_some()
    }

    fn ground_at(&self, origin: &QVector3D, dir: &QVector3D) -> QString {
        match self.st().ground_hit(&ray(origin, dir)) {
            Some(p) => q(format!("{:.1}, {:.1}, {:.1}", p.x, p.y, p.z)),
            None => QString::default(),
        }
    }

    fn begin_gizmo(mut self: Pin<&mut Self>, axis: i32, origin: &QVector3D, dir: &QVector3D) -> bool {
        let ok = self.as_mut().stm().begin_drag(axis.clamp(0, 3) as usize, &ray(origin, dir));
        self.sync_flags();
        ok
    }

    fn drag_gizmo(mut self: Pin<&mut Self>, origin: &QVector3D, dir: &QVector3D) {
        let moved = self.as_mut().stm().drag_to(&ray(origin, dir));
        if moved.is_empty() {
            return;
        }
        let actors = self.st().transforms_json(&moved);
        let rails = self.st().rails_touched(&moved);
        self.as_mut().transforms_moved(q(actors.to_string()));
        self.as_mut().rails_moved(q(json!(rails).to_string()));
        let v = *self.overlay_revision() + 1;
        self.as_mut().set_overlay_revision(v);
        self.as_mut().set_dirty(true);
    }

    fn end_gizmo(mut self: Pin<&mut Self>) {
        self.as_mut().stm().end_drag();
        self.bump(Refresh::GIZMO);
    }

    // ------------------------------------------------------------ outliner and selection

    fn outliner(&self, filter: &QString) -> QString {
        q(self.st().outliner_json(&filter.to_string()).to_string())
    }

    fn toggle_expand(mut self: Pin<&mut Self>, key: &QString) {
        let k = key.to_string();
        let st = self.as_mut().stm();
        if !st.expanded.remove(&k) {
            st.expanded.insert(k);
        }
        self.bump(Refresh::PANELS);
    }

    fn set_group_visible(mut self: Pin<&mut Self>, key: &QString, visible: bool) {
        self.as_mut().stm().set_group_visible(&key.to_string(), visible);
        self.bump(Refresh::ALL);
    }

    fn select_hash(mut self: Pin<&mut Self>, hash: &QString, additive: bool) {
        let Ok(h) = hash.to_string().parse::<u64>() else { return };
        let st = self.as_mut().stm();
        if additive {
            if let Some(i) = st.selection.iter().position(|x| *x == h) {
                st.selection.remove(i);
            } else {
                st.selection.push(h);
            }
        } else {
            st.selection = vec![h];
        }
        st.rebuild();
        self.bump(Refresh::SELECTION);
    }

    fn select_group(mut self: Pin<&mut Self>, key: &QString) {
        let st = self.as_mut().stm();
        st.selection = st.group_hashes(&key.to_string());
        st.rebuild();
        self.bump(Refresh::SELECTION);
    }

    fn clear_selection(mut self: Pin<&mut Self>) {
        let st = self.as_mut().stm();
        st.selection.clear();
        st.rebuild();
        self.bump(Refresh::SELECTION);
    }

    fn selection(&self) -> QString {
        q(self.st().selection_json().to_string())
    }

    // ------------------------------------------------------------ property edits

    fn current(&self) -> Option<u64> {
        let s = self.st();
        (s.selection.len() == 1).then(|| s.selection[0])
    }

    fn set_vector(mut self: Pin<&mut Self>, key: &QString, x: f64, y: f64, z: f64) {
        let Some(h) = self.current() else { return };
        let k = key.to_string();
        let st = self.as_mut().stm();
        let Some(doc) = st.doc.as_mut() else { return };
        let is_point = matches!(doc.index.get(&h), Some(Loc::Point(..)));
        let key = match (k.as_str(), is_point) {
            ("Translate", true) => "Translation",
            (other, _) => other,
        };
        let v: Vec<f32> = match key {
            "Rotate" => vec![(x as f32).to_radians(), (y as f32).to_radians(), (z as f32).to_radians()],
            "RectSize" | "RectOffset" => vec![x as f32, y as f32],
            _ => vec![x as f32, y as f32, z as f32],
        };
        doc.set_vector(h, key, &v);
        self.changed();
    }

    fn set_param(mut self: Pin<&mut Self>, name: &QString, kind: &QString, text: &QString) -> QString {
        let Some(h) = self.current() else { return q("select one item") };
        match typedjson::parse_text(&kind.to_string(), &text.to_string()) {
            Ok(v) => {
                let n = name.to_string();
                if let Some(d) = self.as_mut().stm().doc.as_mut() {
                    d.set_dynamic(h, &n, Some(v));
                }
                self.changed();
                QString::default()
            }
            Err(e) => q(e.to_string()),
        }
    }

    fn remove_param(mut self: Pin<&mut Self>, name: &QString) {
        let Some(h) = self.current() else { return };
        let n = name.to_string();
        if let Some(d) = self.as_mut().stm().doc.as_mut() {
            d.set_dynamic(h, &n, None);
        }
        self.changed();
    }

    fn set_raw(mut self: Pin<&mut Self>, text: &QString) -> QString {
        let Some(h) = self.current() else { return q("select one item") };
        let t = text.to_string();
        let st = self.as_mut().stm();
        let Some(doc) = st.doc.as_mut() else { return q("no course open") };
        let keep: &[&str] = match doc.index.get(&h) {
            Some(Loc::Actor(_)) => &State::ACTOR_KEEP,
            Some(Loc::Point(..)) => &State::POINT_KEEP,
            _ => &State::RAIL_KEEP,
        };
        match doc.replace_fields(h, &t, keep) {
            Ok(()) => {
                self.as_mut().log("Raw fields applied");
                self.changed();
                QString::default()
            }
            Err(e) => q(e.to_string()),
        }
    }

    fn set_closed(mut self: Pin<&mut Self>, closed: bool) {
        let Some(h) = self.current() else { return };
        if let Some(d) = self.as_mut().stm().doc.as_mut() {
            d.set_field(h, "IsClosed", Some(Byml::Bool(closed)));
        }
        self.changed();
    }

    fn toggle_curve(mut self: Pin<&mut Self>) {
        let Some(h) = self.current() else { return };
        if let Some(d) = self.as_mut().stm().doc.as_mut() {
            d.toggle_curve(h);
        }
        self.changed();
    }

    fn set_pad(mut self: Pin<&mut Self>, group: &QString, key: &QString, kind: &QString, text: &QString) -> QString {
        let Some(h) = self.current() else { return q("select one point") };
        let Ok(g) = group.to_string().parse::<u64>() else { return q("bad group") };
        match typedjson::parse_text(&kind.to_string(), &text.to_string()) {
            Ok(v) => {
                let k = key.to_string();
                if let Some(d) = self.as_mut().stm().doc.as_mut() {
                    d.set_pad_value(h, g, &k, v);
                }
                self.changed();
                QString::default()
            }
            Err(e) => q(e.to_string()),
        }
    }

    fn remove_link(mut self: Pin<&mut Self>, index: i32) {
        let Some(h) = self.current() else { return };
        if let Some(d) = self.as_mut().stm().doc.as_mut() {
            d.remove_link(h, index.max(0) as usize);
        }
        self.changed();
    }

    fn start_link(mut self: Pin<&mut Self>, kind: &QString) {
        let Some(h) = self.current() else {
            self.log("Select one object or point to link from");
            return;
        };
        let k = kind.to_string();
        let st = self.as_mut().stm();
        st.link_type = if LINK_TYPES.contains(&k.as_str()) { k } else { "BasicSignal".into() };
        st.link_pick = Some(h);
        let msg = format!("Click the {} target (Esc cancels). Clicking a path picks its first point.", st.link_type);
        self.log(msg);
    }

    fn cancel_link(mut self: Pin<&mut Self>) {
        self.as_mut().stm().link_pick = None;
        self.log("Link cancelled");
    }

    // ------------------------------------------------------------ tools

    fn add_actor(mut self: Pin<&mut Self>, name: &QString, origin: &QVector3D, dir: &QVector3D, yaw: f64) -> bool {
        let n = name.to_string();
        let st = self.as_mut().stm();
        let (Some(cat), true) = (st.cat.clone(), st.doc.is_some()) else { return false };
        let pos = st.placement(&ray(origin, dir));
        let h = st.doc.as_mut().unwrap().add_actor(&cat, &n, pos, yaw as f32);
        match h {
            Some(h) => {
                st.selection = vec![h];
                let extra = if cat.data.actors.get(&n).is_some_and(|a| a.template.is_none()) {
                    " (never placed in a shipped course, so no extra fields were copied)"
                } else {
                    ""
                };
                self.as_mut().log(format!("Added {n}{extra}"));
                self.changed();
                true
            }
            None => false,
        }
    }

    fn duplicate(mut self: Pin<&mut Self>, rx: f64, rz: f64) {
        let st = self.as_mut().stm();
        let sel = st.selection.clone();
        let Some(doc) = st.doc.as_mut() else { return };
        let out = doc.duplicate(&sel, Vec3::new(rx as f32, 0.0, rz as f32).normalize_or(Vec3::X) * 3.0);
        if !out.is_empty() {
            st.selection = out;
            let n = sel.len();
            self.as_mut().log(format!("Duplicated {n}"));
            self.changed();
        }
    }

    fn delete_selected(mut self: Pin<&mut Self>) {
        let st = self.as_mut().stm();
        let sel = std::mem::take(&mut st.selection);
        let Some(doc) = st.doc.as_mut() else { return };
        if sel.is_empty() {
            return;
        }
        let msg = doc.delete(&sel);
        self.as_mut().log(msg);
        self.changed();
    }

    fn drop_to_ground(mut self: Pin<&mut Self>) {
        let st = self.as_mut().stm();
        let (Some(cat), Some(ground)) = (st.cat.clone(), st.ground.clone()) else {
            self.log("The stage surface is still loading");
            return;
        };
        let sel = st.selection.clone();
        let Some(doc) = st.doc.as_mut() else { return };
        let (n, total) = doc.drop_to_ground(&sel, &cat, &|p| ground.ground_below(p));
        self.as_mut().log(format!("Dropped {n} of {total} onto the stage surface"));
        self.changed();
    }

    fn insert_point(mut self: Pin<&mut Self>) {
        let st = self.as_mut().stm();
        let Some(&h) = st.selection.last() else { return };
        let Some(doc) = st.doc.as_mut() else { return };
        match doc.insert_point_after(h) {
            Some(n) => {
                st.selection = vec![n];
                self.as_mut().log("Inserted a point");
                self.changed();
            }
            None => self.log("Select a path point first"),
        }
    }

    fn new_rail(mut self: Pin<&mut Self>, name: &QString, origin: &QVector3D, dir: &QVector3D, yaw: f64) -> bool {
        let n = name.to_string();
        let st = self.as_mut().stm();
        let Some(cat) = st.cat.clone() else { return false };
        let pos = st.placement(&ray(origin, dir));
        let Some(doc) = st.doc.as_mut() else { return false };
        match doc.new_rail(&cat, &n, pos, yaw as f32) {
            Some(last) => {
                st.selection = vec![last];
                self.as_mut().log(format!("New {n} with 2 points: select a point and press I to extend it"));
                self.changed();
                true
            }
            None => false,
        }
    }

    fn reverse_rail(mut self: Pin<&mut Self>) {
        let st = self.as_mut().stm();
        let Some(&h) = st.selection.first() else { return };
        let Some(doc) = st.doc.as_mut() else { return };
        let Some(rail) = doc.rail_hash_of(h) else { return };
        doc.reverse_rail(rail);
        self.as_mut()
            .log("Path reversed. Links into its old first point and out of its old last point were left as they were; check them.");
        self.changed();
    }

    fn change_object(mut self: Pin<&mut Self>, name: &QString) -> QString {
        let n = name.to_string();
        let st = self.as_mut().stm();
        let Some(cat) = st.cat.clone() else { return q("catalog not ready") };
        let sel = st.selection.clone();
        let Some(doc) = st.doc.as_mut() else { return q("no course open") };
        for h in &sel {
            if let Err(e) = doc.change_type(&cat, *h, &n) {
                return q(e.to_string());
            }
        }
        self.as_mut().log(format!("Changed {} object(s) to {n}", sel.len()));
        self.changed();
        QString::default()
    }

    // ------------------------------------------------------------ panels

    fn assets(&self, filter: &QString, category: &QString, placed_only: bool) -> QString {
        q(self.st().asset_json(&filter.to_string(), &category.to_string(), placed_only).to_string())
    }

    fn categories(&self) -> QString {
        q(self.st().categories_json().to_string())
    }

    fn rail_types(&self) -> QString {
        q(self.st().rail_types_json().to_string())
    }

    fn view_state(&self) -> QString {
        q(self.st().view_json().to_string())
    }

    fn set_view_option(mut self: Pin<&mut Self>, name: &QString, on: bool) {
        let st = self.as_mut().stm();
        match name.to_string().as_str() {
            "stage" => st.show_stage = on,
            "models" => st.view.models = on,
            "xray" => st.xray = on,
            "widths" => st.view.widths = on,
            "snap" => st.snap_ground = on,
            _ => {}
        }
        st.rebuild();
        let v = *self.stages_revision() + 1;
        self.as_mut().set_stages_revision(v);
        self.bump(Refresh::ALL);
    }

    fn set_rail_type_visible(mut self: Pin<&mut Self>, name: &QString, on: bool) {
        let st = self.as_mut().stm();
        let n = name.to_string();
        if on {
            st.view.hidden_rail_types.remove(&n);
        } else {
            st.view.hidden_rail_types.insert(n);
        }
        st.rebuild();
        self.bump(Refresh::SELECTION);
    }

    fn set_category_visible(mut self: Pin<&mut Self>, name: &QString, on: bool) {
        let st = self.as_mut().stm();
        let n = name.to_string();
        if on {
            st.view.hidden_categories.remove(&n);
        } else {
            st.view.hidden_categories.insert(n);
        }
        st.rebuild();
        self.bump(Refresh::OBJECTS);
    }

    fn set_gizmo(mut self: Pin<&mut Self>, mode: &QString, local: bool) {
        let m = mode.to_string();
        let st = self.as_mut().stm();
        st.gizmo_mode = match m.as_str() {
            "rotate" => GizmoMode::Rotate,
            "scale" => GizmoMode::Scale,
            _ => GizmoMode::Translate,
        };
        st.gizmo_local = local;
        self.as_mut().set_gizmo_mode(q(m));
        self.as_mut().set_gizmo_local(local);
        let v = *self.overlay_revision() + 1;
        self.as_mut().set_overlay_revision(v);
    }

    fn apply_settings(mut self: Pin<&mut Self>, romfs: &QString, models: &QString, mods: &QString) -> QString {
        let mut cfg = self.st().cfg.clone();
        cfg.romfs = crate::config::expand(&romfs.to_string());
        cfg.models = crate::config::expand(&models.to_string());
        cfg.mods = crate::config::expand(&mods.to_string());
        if let Err(e) = cfg.valid().and_then(|_| cfg.save()) {
            return q(crate::config::tilde(&e));
        }
        self.as_mut().set_romfs_path(romfs.clone());
        self.as_mut().set_models_path(models.clone());
        self.as_mut().set_mods_path(mods.clone());
        let st = self.as_mut().stm();
        st.cfg = cfg;
        st.cat = None;
        st.doc = None;
        st.scene = None;
        self.as_mut().set_ready(false);
        self.as_mut().set_course_title(QString::default());
        self.as_mut().bump(Refresh::ALL);
        self.start();
        QString::default()
    }

    /// The ns2jb-modloader export target (an SD card or a folder to copy onto one). Saved
    /// without reloading the game files.
    fn set_export(mut self: Pin<&mut Self>, ns2jb: &QString, on_save: bool) -> QString {
        let path = crate::config::expand(&ns2jb.to_string());
        if on_save && !path.is_dir() {
            return q(crate::config::tilde(&format!(
                "{} is not a folder: pick the Switch 2 SD card (or a folder to copy onto it)",
                path.display()
            )));
        }
        let st = self.as_mut().stm();
        st.cfg.ns2jb = path;
        st.cfg.ns2jb_on_save = on_save;
        if let Err(e) = st.cfg.save() {
            return q(crate::config::tilde(&format!("Settings not saved: {e}")));
        }
        self.as_mut().set_ns2jb_path(ns2jb.clone());
        self.as_mut().set_ns2jb_on_save(on_save);
        QString::default()
    }

    /// Export the saved mod for ns2jb-modloader now (File menu).
    fn export_ns2jb(mut self: Pin<&mut Self>) -> QString {
        let v = self.st().export_ns2jb();
        let msg = if v["ok"] == true {
            format!(
                "Exported {} file(s) for ns2jb-modloader to {}",
                v["files"].as_array().map(|a| a.len()).unwrap_or(0),
                v["dir"].as_str().unwrap_or("")
            )
        } else {
            format!(
                "ns2jb-modloader export failed: {}",
                v["errors"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default()
            )
        };
        self.as_mut().log(msg);
        q(crate::config::tilde(&v.to_string()))
    }

    fn open_folder(&self, which: &QString) {
        let s = self.st();
        let dir = match which.to_string().as_str() {
            "mods" => s.cfg.mods.join(&s.mod_name),
            "ns2jb" => mkwtrack::workspace::ns2jb_romfs(&s.cfg.ns2jb),
            "emulator" => crate::config::emulator_load_dir(),
            _ => cache_dir(),
        };
        std::fs::create_dir_all(&dir).ok();
        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    }

    /// Render a thumbnail for every object model into the cache (in the background).
    fn build_thumbnails(mut self: Pin<&mut Self>) {
        let Some(cat) = self.st().cat.clone() else { return };
        if *self.thumbs_total() > 0 && *self.thumbs_done() < *self.thumbs_total() {
            return;
        }
        let jobs = thumbs::jobs(&cat);
        self.as_mut().set_thumbs_total(jobs.len() as i32);
        self.as_mut().set_thumbs_done(0);
        let qt = self.qt_thread();
        let models = self.st().cfg.models.clone();
        std::thread::spawn(move || {
            let qt2 = qt.clone();
            thumbs::render_all(&models, jobs, &move |done| {
                let _ = qt2.queue(move |mut ed| ed.as_mut().set_thumbs_done(done as i32));
            });
            let _ = qt.queue(|mut ed| {
                let v = *ed.revision() + 1;
                ed.as_mut().set_revision(v);
            });
        });
    }
}

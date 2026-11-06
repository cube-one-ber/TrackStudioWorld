//! Viewport content built from the open document: what QML draws and what Rust picks.

use crate::banc::hash_of;
use crate::byml::{Byml, vec3};
use crate::catalog::{Catalog, StagePlacement};
use crate::doc::{Document, Loc};
use crate::fbx::Model;
use crate::math::{quat_zyx, world_aabb};
use crate::raycast::{Bvh, Ray, ray_obb, ray_sphere};
use glam::{Mat4, Quat, Vec3};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub fn rail_color(name: &str) -> [f32; 3] {
    let c: u32 = match name {
        "LapPathGraph" => 0xff5a36,
        "RoutePathGraph" => 0x3aa0ff,
        "GuideGraphRail" => 0xc86bff,
        "RailSlidePath" => 0xffd23a,
        "ColRailSlidePath" => 0xa8b840,
        "GlidePath" => 0x3ae0d0,
        "GravityPath" => 0xff3ad0,
        "GravityCameraPath" => 0xffa0e6,
        "CitizenPath" => 0x8bc34a,
        "PullPath" => 0xf0f0a0,
        "WaterCascadePath" | "WaterFlowPath" | "WhirlpoolPath" => 0x29b6f6,
        "ParkingPath" => 0xa1887f,
        "OffroadPath" => 0xd7a86e,
        "RidePath" => 0x9575cd,
        _ => 0x9aa0a6,
    };
    rgb(c)
}

pub fn category_color(cat: &str) -> [f32; 3] {
    rgb(match cat {
        "PropObj" => 0x4caf50,
        "StaticObj" => 0x9e9e9e,
        "SystemObj" => 0xff9800,
        "System" => 0xffb74d,
        "EffectObj" => 0xe91e63,
        "SoundObj" => 0x8d6e63,
        "Enemy" => 0xf44336,
        _ => 0x26c6da,
    })
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

pub fn hex(c: [f32; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

const LINK_COLORS: [(&str, [f32; 3]); 3] =
    [("BasicSignal", [1.0, 1.0, 1.0]), ("MessageLink", [1.0, 0.47, 0.8]), ("CreateLink", [0.47, 1.0, 0.6])];
const SELECT: [f32; 3] = [1.0, 0.85, 0.1];

#[derive(Default, Clone)]
pub struct ViewOptions {
    pub models: bool,
    pub widths: bool,
    pub hidden_rail_types: HashSet<String>,
    pub hidden_categories: HashSet<String>,
    pub hidden_actor_names: HashSet<String>,
    pub hidden_rails: HashSet<u64>,
}

#[derive(Clone, Debug)]
pub struct Instance {
    pub hash: u64,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

#[derive(Clone, Debug)]
pub struct BoxActor {
    pub hash: u64,
    /// "box", "sphere", "cone", "cylinder"
    pub shape: &'static str,
    pub color: [f32; 3],
    pub world: Mat4,
}

#[derive(Clone, Debug)]
pub struct Proxy {
    pub hash: u64,
    pub world: Mat4,
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Default)]
pub struct Lines {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
}

impl Lines {
    pub fn seg(&mut self, a: Vec3, b: Vec3, c: [f32; 3]) {
        self.positions.push(a.into());
        self.positions.push(b.into());
        self.colors.push([c[0], c[1], c[2], 1.0]);
        self.colors.push([c[0], c[1], c[2], 1.0]);
    }

    fn chevron(&mut self, at: Vec3, dir: Vec3, size: f32, c: [f32; 3]) {
        let d = dir.normalize_or_zero();
        if d == Vec3::ZERO {
            return;
        }
        let side = d.cross(Vec3::Y).normalize_or(Vec3::X) * size * 0.6;
        let back = at - d * size;
        self.seg(at, back + side, c);
        self.seg(at, back - side, c);
    }

    fn obb(&mut self, world: Mat4, min: Vec3, max: Vec3, c: [f32; 3]) {
        let p = |x: f32, y: f32, z: f32| world.transform_point3(Vec3::new(x, y, z));
        let (a, b) = (min, max);
        let corners = [
            p(a.x, a.y, a.z),
            p(b.x, a.y, a.z),
            p(b.x, b.y, a.z),
            p(a.x, b.y, a.z),
            p(a.x, a.y, b.z),
            p(b.x, a.y, b.z),
            p(b.x, b.y, b.z),
            p(a.x, b.y, b.z),
        ];
        for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
            self.seg(corners[i], corners[j], c);
        }
    }
}

/// Triangles with vertex colours (point markers).
#[derive(Default)]
pub struct Tris {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
}

impl Tris {
    /// Octahedron marker with simple baked shading.
    fn marker(&mut self, c: Vec3, r: f32, col: [f32; 3]) {
        let v = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        for (a, b, cc) in [(0, 2, 4), (4, 2, 1), (1, 2, 5), (5, 2, 0), (4, 3, 0), (1, 3, 4), (5, 3, 1), (0, 3, 5)] {
            let n = (v[a] + v[b] + v[cc]).normalize();
            let shade = 0.55 + 0.45 * (0.5 + 0.5 * n.y);
            for i in [a, b, cc] {
                self.positions.push((c + v[i] * r).into());
                self.colors.push([col[0] * shade, col[1] * shade, col[2] * shade, 1.0]);
            }
        }
    }
}

pub struct RailMesh {
    pub hash: u64,
    pub name: String,
    pub lines: Lines,
    pub markers: Tris,
}

pub struct Scene {
    pub groups: BTreeMap<String, Vec<Instance>>,
    pub boxes: Vec<BoxActor>,
    pub proxies: Vec<Proxy>,
    pub rails: Vec<RailMesh>,
    pub points: Vec<(u64, u64, Vec3, f32)>,
}

pub fn actor_world(a: &Byml) -> (Vec3, Quat, Vec3) {
    (Vec3::from(a.vec3("Translate", [0.0; 3])), quat_zyx(a.vec3("Rotate", [0.0; 3])), Vec3::from(a.vec3("Scale", [1.0; 3])))
}

/// Local box used for picking and as the placeholder shape.
pub fn actor_box(cat: &Catalog, a: &Byml) -> (Vec3, Vec3, &'static str) {
    match a.str_or("PrimitiveType", "") {
        "Cone" => return (Vec3::new(-0.9, -0.9, -1.2), Vec3::new(0.9, 0.9, 1.2), "cone"),
        p if p.starts_with("Sphere") => return (Vec3::splat(-1.0), Vec3::splat(1.0), "sphere"),
        p if p.starts_with("Cylinder") => return (Vec3::new(-1.0, -0.5, -1.0), Vec3::new(1.0, 0.5, 1.0), "cylinder"),
        "" => {}
        _ => return (Vec3::splat(-0.5), Vec3::splat(0.5), "box"),
    }
    match cat.actor(a.str_or("Gyaml", "")).and_then(|i| i.aabb) {
        Some(b) => {
            let (mn, mx) = (Vec3::from(b[0]), Vec3::from(b[1]));
            let c = (mn + mx) / 2.0;
            let h = ((mx - mn) / 2.0).max(Vec3::splat(0.25));
            (c - h, c + h, "box")
        }
        None => (Vec3::new(-0.75, 0.0, -0.75), Vec3::new(0.75, 1.5, 0.75), "box"),
    }
}

pub fn build(doc: &Document, cat: &Catalog, opts: &ViewOptions, model_ready: &dyn Fn(&str) -> bool, selection: &HashSet<u64>) -> Scene {
    let mut groups: BTreeMap<String, Vec<Instance>> = BTreeMap::new();
    let mut boxes = vec![];
    let mut proxies = vec![];
    for a in doc.actors() {
        let h = hash_of(a);
        let info = cat.actor(a.str_or("Gyaml", ""));
        let category = info.map(|i| i.category.as_str()).unwrap_or("Other");
        if opts.hidden_categories.contains(category) || opts.hidden_actor_names.contains(a.str_or("Name", "")) {
            continue;
        }
        let (pos, rot, scale) = actor_world(a);
        let world = Mat4::from_scale_rotation_translation(scale, rot, pos);
        let (mn, mx, shape) = actor_box(cat, a);
        proxies.push(Proxy { hash: h, world, min: mn, max: mx });
        let model = info.and_then(|i| i.model.as_ref()).filter(|m| opts.models && a.get("PrimitiveType").is_none() && model_ready(m));
        match model {
            Some(m) => groups.entry(m.clone()).or_default().push(Instance { hash: h, pos, rot, scale }),
            None => {
                let local = Mat4::from_scale_rotation_translation(mx - mn, Quat::IDENTITY, (mn + mx) / 2.0);
                boxes.push(BoxActor { hash: h, shape, color: category_color(category), world: world * local });
            }
        }
    }
    let mut rails = vec![];
    let mut points = vec![];
    for r in doc.rails() {
        let name = r.str_or("Name", "").to_string();
        let rh = hash_of(r);
        if opts.hidden_rail_types.contains(&name) || opts.hidden_rails.contains(&rh) {
            continue;
        }
        let col = rail_color(&name);
        let pts = Document::points(r);
        let mut lines = Lines::default();
        let mut markers = Tris::default();
        let p: Vec<Vec3> = pts.iter().map(|x| Vec3::from(x.vec3("Translation", [0.0; 3]))).collect();
        let n = pts.len();
        let closed = r.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false);
        let segs = if closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            let j = (i + 1) % n;
            let (a, b) = (p[i], p[j]);
            let c1 = pts[i].get("Control1").map(|c| Vec3::from(vec3(Some(c), [0.0; 3])));
            let c0 = pts[j].get("Control0").map(|c| Vec3::from(vec3(Some(c), [0.0; 3])));
            let (mid, dir) = if c1.is_some() || c0.is_some() {
                let (q1, q2) = (c1.unwrap_or(a), c0.unwrap_or(b));
                let bez = |t: f32| {
                    let u = 1.0 - t;
                    a * u * u * u + q1 * 3.0 * u * u * t + q2 * 3.0 * u * t * t + b * t * t * t
                };
                let mut prev = a;
                for k in 1..=16 {
                    let q = bez(k as f32 / 16.0);
                    lines.seg(prev, q, col);
                    prev = q;
                }
                (bez(0.55), bez(0.55) - bez(0.45))
            } else {
                lines.seg(a, b, col);
                (a.lerp(b, 0.55), b - a)
            };
            lines.chevron(mid, dir, 1.4, col);
        }
        let rects = name == "LapPathGraph" || name == "GlidePath" || opts.widths;
        for (i, pt) in pts.iter().enumerate() {
            let ph = hash_of(pt);
            let rot = quat_zyx(pt.vec3("Rotate", [0.0; 3]));
            if rects && let Some(size) = pt.get("RectSize").and_then(|s| s.as_array()) {
                let w = size.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let hgt = size.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let off = pt.get("RectOffset").and_then(|s| s.as_array());
                let ox = off.and_then(|o| o.first()).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let oy = off.and_then(|o| o.get(1)).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                // RectOffset.y is the gate's top edge (shipped gates bottom out 5-10 m below the point)
                let c = [[ox - w / 2.0, oy - hgt], [ox + w / 2.0, oy - hgt], [ox + w / 2.0, oy], [ox - w / 2.0, oy]]
                    .map(|[x, y]| p[i] + rot * Vec3::new(x, y, 0.0));
                let rc = if name == "LapPathGraph" { col } else { col.map(|x| x * 0.7) };
                for k in 0..4 {
                    lines.seg(c[k], c[(k + 1) % 4], rc);
                }
            }
            let sel = selection.contains(&ph) || selection.contains(&rh);
            let radius = if name == "LapPathGraph" { 1.1 } else { 0.7 } * if sel { 1.5 } else { 1.0 };
            markers.marker(p[i], radius, if sel { SELECT } else { col });
            points.push((ph, rh, p[i], radius.max(0.9)));
            if selection.contains(&ph) {
                for k in ["Control0", "Control1"] {
                    if let Some(c) = pt.get(k) {
                        let c = Vec3::from(vec3(Some(c), [0.0; 3]));
                        lines.seg(p[i], c, [1.0, 1.0, 1.0]);
                        markers.marker(c, 0.35, [1.0, 1.0, 1.0]);
                    }
                }
            }
        }
        rails.push(RailMesh { hash: rh, name, lines, markers });
    }
    Scene { groups, boxes, proxies, rails, points }
}

/// Links between objects and points, drawn as lines with an arrowhead near the target.
pub fn links(doc: &Document) -> Lines {
    let mut out = Lines::default();
    let pos = |h: u64| -> Option<Vec3> {
        let (loc, e) = doc.find(h)?;
        Some(match loc {
            Loc::Actor(_) => Vec3::from(e.vec3("Translate", [0.0; 3])),
            Loc::Point(..) => Vec3::from(e.vec3("Translation", [0.0; 3])),
            Loc::Rail(_) => Vec3::from(Document::points(e).first()?.vec3("Translation", [0.0; 3])),
        })
    };
    let mut add = |owner: &Byml| {
        for l in owner.get("Links").and_then(|x| x.as_array()).into_iter().flatten() {
            let src = l.get("Src").and_then(|x| x.as_u64()).unwrap_or(hash_of(owner));
            let (Some(a), Some(b)) = (pos(src), l.get("Dst").and_then(|x| x.as_u64()).and_then(&pos)) else { continue };
            let c = LINK_COLORS.iter().find(|(n, _)| *n == l.str_or("Name", "")).map(|x| x.1).unwrap_or([1.0; 3]);
            // dashed
            let len = a.distance(b);
            let steps = ((len / 2.5) as usize).clamp(1, 400);
            for k in (0..steps).step_by(2) {
                out.seg(a.lerp(b, k as f32 / steps as f32), a.lerp(b, ((k + 1) as f32 / steps as f32).min(1.0)), c);
            }
            if len > 3.0 {
                out.chevron(b - (b - a).normalize() * 1.0, b - a, 1.6, c);
            }
        }
    };
    for a in doc.actors() {
        add(a);
    }
    for r in doc.rails() {
        for p in Document::points(r) {
            add(p);
        }
    }
    out
}

/// Outline boxes around selected objects.
pub fn selection_outline(doc: &Document, cat: &Catalog, selection: &HashSet<u64>) -> Lines {
    let mut out = Lines::default();
    for &h in selection {
        if let Some((Loc::Actor(_), a)) = doc.find(h) {
            let (pos, rot, scale) = actor_world(a);
            let (mn, mx, _) = actor_box(cat, a);
            let pad = (mx - mn) * 0.04;
            out.obb(Mat4::from_scale_rotation_translation(scale, rot, pos), mn - pad, mx + pad, SELECT);
        }
    }
    out
}

/// Pick the nearest point or object along a ray (points win ties within 1 m).
pub fn pick(scene: &Scene, r: &Ray) -> Option<u64> {
    let mut best_pt: Option<(f32, u64)> = None;
    for &(h, _, c, rad) in &scene.points {
        let grow = (c.distance(r.origin) * 0.004).max(rad);
        if let Some(t) = ray_sphere(r, c, grow)
            && best_pt.is_none_or(|b| t < b.0)
        {
            best_pt = Some((t, h));
        }
    }
    let mut best_actor: Option<(f32, u64)> = None;
    for p in &scene.proxies {
        if let Some(t) = ray_obb(r, p.world, p.min, p.max)
            && best_actor.is_none_or(|b| t < b.0)
        {
            best_actor = Some((t, p.hash));
        }
    }
    match (best_pt, best_actor) {
        (Some(p), Some(a)) => Some(if p.0 <= a.0 + 1.0 { p.1 } else { a.1 }),
        (Some(p), None) => Some(p.1),
        (None, Some(a)) => Some(a.1),
        (None, None) => None,
    }
}

/// Rail hash for a point hash.
pub fn rail_of(scene: &Scene, point: u64) -> Option<u64> {
    scene.points.iter().find(|p| p.0 == point).map(|p| p.1)
}

/// Ground surface for drop-to-ground and placement: every triangle of the given stage models,
/// placed where the game puts them. `model` returns a parsed model by its path under the
/// models folder (stages whose model is missing are skipped).
pub fn stage_bvh(stages: &[StagePlacement], model: impl Fn(&str) -> Option<Arc<Model>>) -> Bvh {
    let mut tris = vec![];
    for s in stages {
        let Some(m) = s.model.as_deref().and_then(&model) else { continue };
        let world = Mat4::from_scale_rotation_translation(Vec3::from(s.scale), quat_zyx(s.rotate), Vec3::from(s.translate));
        for p in &m.parts {
            for t in p.indices.as_chunks::<3>().0 {
                tris.push([0, 1, 2].map(|k| world.transform_point3(Vec3::from(p.positions[t[k] as usize]))));
            }
        }
    }
    Bvh::new(tris)
}

/// Stage models near a course, nearest first (at most `max`), skipping backdrops over 5 km
/// and stages the course file places itself (those draw as objects).
pub fn stages_for<'a>(cat: &'a Catalog, doc: &Byml, max: usize) -> Vec<&'a StagePlacement> {
    let mut pts: Vec<[f32; 3]> = doc.array("Actors").iter().map(|a| a.vec3("Translate", [0.0; 3])).collect();
    for r in doc.array("Rails") {
        pts.extend(r.array("Points").iter().map(|p| p.vec3("Translation", [0.0; 3])));
    }
    if let Some(a) = doc.get("AABB") {
        pts.push(a.vec3("Min", [0.0; 3]));
        pts.push(a.vec3("Max", [0.0; 3]));
    }
    if pts.is_empty() {
        return vec![];
    }
    let mn = Vec3::from([0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f32::MAX, f32::min) - 60.0));
    let mx = Vec3::from([0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f32::MIN, f32::max) + 60.0));
    let centre = (mn + mx) / 2.0;
    let own: HashSet<&str> = doc.array("Actors").iter().map(|a| a.str_or("Name", "")).collect();
    let mut hits = vec![];
    for s in &cat.data.stages {
        let (Some(_), Some(aabb)) = (&s.model, s.aabb) else { continue };
        if own.contains(s.name.as_str()) {
            continue;
        }
        let (smn, smx) = world_aabb(aabb, s.translate, s.rotate, s.scale);
        let (smn, smx) = (Vec3::from(smn), Vec3::from(smx));
        if (smx - smn).max_element() > 5000.0 {
            continue;
        }
        if smx.cmpge(mn).all() && smn.cmple(mx).all() {
            hits.push((centre.clamp(smn, smx).distance(centre), s));
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    hits.into_iter().take(max).map(|x| x.1).collect()
}

/// Unit shapes centred on the origin, as triangle lists (positions, normals): "box" and
/// "sphere" span [-0.5, 0.5]; "cylinder" runs along Y; "cone" points at +Z; "torus" is a ring
/// of radius 0.5 around Y (the gizmo's rotation handles).
pub fn primitive(shape: &str) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    use std::f32::consts::{PI, TAU};
    let mut pos = vec![];
    let mut nrm = vec![];
    let mut tri = |a: Vec3, b: Vec3, c: Vec3, n: Option<[Vec3; 3]>| {
        let f = (b - a).cross(c - a).normalize_or_zero();
        let ns = n.unwrap_or([f, f, f]);
        for (p, n) in [(a, ns[0]), (b, ns[1]), (c, ns[2])] {
            pos.push(p.into());
            nrm.push(n.into());
        }
    };
    let seg = 20;
    match shape {
        "sphere" => {
            let rings = 10;
            let pt = |i: usize, j: usize| {
                let (th, ph) = (PI * i as f32 / rings as f32, TAU * j as f32 / seg as f32);
                Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin())
            };
            for i in 0..rings {
                for j in 0..seg {
                    let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
                    tri(a * 0.5, c * 0.5, b * 0.5, Some([a, c, b]));
                    tri(a * 0.5, d * 0.5, c * 0.5, Some([a, d, c]));
                }
            }
        }
        "torus" => {
            let (major, minor, ring, tube) = (0.5f32, 0.035f32, 48, 8);
            let pt = |i: usize, j: usize| {
                let (u, v) = (TAU * i as f32 / ring as f32, TAU * j as f32 / tube as f32);
                let c = Vec3::new(u.cos(), 0.0, u.sin());
                let n = c * v.cos() + Vec3::Y * v.sin();
                (c * major + n * minor, n)
            };
            for i in 0..ring {
                for j in 0..tube {
                    let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
                    tri(a.0, b.0, c.0, Some([a.1, b.1, c.1]));
                    tri(a.0, c.0, d.0, Some([a.1, c.1, d.1]));
                }
            }
        }
        "cylinder" => {
            let (t, b) = (Vec3::Y * 0.5, -Vec3::Y * 0.5);
            for j in 0..seg {
                let (a0, a1) = (TAU * j as f32 / seg as f32, TAU * (j + 1) as f32 / seg as f32);
                let (p0, p1) = (Vec3::new(a0.cos(), 0.0, a0.sin()) * 0.5, Vec3::new(a1.cos(), 0.0, a1.sin()) * 0.5);
                tri(p0 + b, p1 + t, p1 + b, None);
                tri(p0 + b, p0 + t, p1 + t, None);
                tri(t, p1 + t, p0 + t, None);
                tri(b, p0 + b, p1 + b, None);
            }
        }
        "cone" => {
            let (tip, base) = (Vec3::Z * 0.5, -Vec3::Z * 0.5);
            for j in 0..seg {
                let (a0, a1) = (TAU * j as f32 / seg as f32, TAU * (j + 1) as f32 / seg as f32);
                let (p0, p1) = (Vec3::new(a0.cos(), a0.sin(), 0.0) * 0.5 + base, Vec3::new(a1.cos(), a1.sin(), 0.0) * 0.5 + base);
                tri(p0, p1, tip, None);
                tri(base, p1, p0, None);
            }
        }
        _ => {
            let c = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * 0.5;
            let faces = [
                [c(1., -1., -1.), c(1., 1., -1.), c(1., 1., 1.), c(1., -1., 1.)],
                [c(-1., -1., 1.), c(-1., 1., 1.), c(-1., 1., -1.), c(-1., -1., -1.)],
                [c(-1., 1., -1.), c(-1., 1., 1.), c(1., 1., 1.), c(1., 1., -1.)],
                [c(-1., -1., 1.), c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.)],
                [c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)],
                [c(1., -1., -1.), c(-1., -1., -1.), c(-1., 1., -1.), c(1., 1., -1.)],
            ];
            for f in faces {
                tri(f[0], f[1], f[2], None);
                tri(f[0], f[2], f[3], None);
            }
        }
    }
    (pos, nrm)
}

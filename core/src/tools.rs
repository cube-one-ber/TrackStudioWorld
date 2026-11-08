//! Editing tools on top of [`Document`]: copy and paste, drawing and reshaping paths (extend,
//! branch, connect, split, join, orient, smooth, subdivide, preset shapes, change type) and
//! placing objects along a path. Every public operation is one undo step.
//!
//! Paths in shipped courses join with `BasicSignal` links from a point to the first point of
//! the next path, and that first point is the next point along the road, not a copy of the
//! linking one (data: lap paths link 27 to 95 m apart, p10 to p90, and no link spans 0 m). So a
//! branch is a new path that starts at the next point, linked from the point it leaves.

use crate::banc::hash_of;
use crate::byml::Byml;
use crate::catalog::Catalog;
use crate::doc::{Document, Loc, UNIQUE_POINT_KEYS, control, link, new_uuid, point_pos, set_link_sources, shift_controls};
use crate::error::Result;
use crate::math::{euler_from_quat, quat_zyx};
use glam::{Mat3, Quat, Vec3};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Copied objects and paths, with the path data of the copied points.
#[derive(Clone, Default, Debug)]
pub struct Clipboard {
    pub actors: Vec<Byml>,
    pub rails: Vec<Byml>,
    /// PathAdditionalData entries of the copied points, by their old hash
    pub pad: HashMap<u64, Byml>,
    /// centre of everything copied
    pub centre: Vec3,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.actors.is_empty() && self.rails.is_empty()
    }

    /// Objects plus paths.
    pub fn len(&self) -> usize {
        self.actors.len() + self.rails.len()
    }
}

/// The shape a new path starts as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailShape {
    /// one point, to draw the rest with Alt+Click or Extrude
    Point,
    /// two points 20 m apart
    Line,
    /// three points bending to the right, with Bézier handles
    Curve,
    /// a closed loop of four points with Bézier handles, 15 m across
    Circle,
}

impl RailShape {
    pub fn from_name(s: &str) -> RailShape {
        match s {
            "point" => RailShape::Point,
            "curve" => RailShape::Curve,
            "circle" => RailShape::Circle,
            _ => RailShape::Line,
        }
    }
}

/// A point of a new path's shape: its position and Bézier handles (in, out).
type ShapePoint = (Vec3, Option<(Vec3, Vec3)>);

/// The rotation whose +Z is `fwd` and whose +Y is as close to `up` as it can be.
pub fn look_rotation(fwd: Vec3, up: Vec3) -> Option<Quat> {
    let f = fwd.normalize_or_zero();
    if f == Vec3::ZERO {
        return None;
    }
    let mut x = up.cross(f);
    if x.length_squared() < 1e-10 {
        x = Vec3::X.cross(f);
        if x.length_squared() < 1e-10 {
            x = Vec3::Z.cross(f);
        }
    }
    let x = x.normalize();
    let y = f.cross(x);
    Some(Quat::from_mat3(&Mat3::from_cols(x, y, f)).normalize())
}

/// Banc Euler angles that face `fwd` with +Y up (no roll).
fn facing(fwd: Vec3) -> [f32; 3] {
    look_rotation(fwd, Vec3::Y).map_or([0.0; 3], euler_from_quat)
}

fn cubic(a: Vec3, q1: Vec3, q2: Vec3, b: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    a * u * u * u + q1 * 3.0 * u * u * t + q2 * 3.0 * u * t * t + b * t * t * t
}

/// A path's shape as a polyline: straight segments as they are, Bézier segments sampled
/// `steps` times, and the closing segment of a loop.
pub fn rail_curve(rail: &Byml, steps: usize) -> Vec<Vec3> {
    let pts = Document::points(rail);
    let Some(first) = pts.first() else { return vec![] };
    let closed = rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false) && pts.len() > 2;
    let mut out = vec![point_pos(first)];
    let segs = if closed { pts.len() } else { pts.len() - 1 };
    for i in 0..segs {
        let j = (i + 1) % pts.len();
        let (a, b) = (point_pos(&pts[i]), point_pos(&pts[j]));
        match (control(&pts[i], "Control1"), control(&pts[j], "Control0")) {
            (None, None) => out.push(b),
            (c1, c0) => {
                let (q1, q2) = (c1.unwrap_or(a), c0.unwrap_or(b));
                out.extend((1..=steps.max(1)).map(|k| cubic(a, q1, q2, b, k as f32 / steps.max(1) as f32)));
            }
        }
    }
    out
}

/// Positions along a polyline every `spacing` metres from its start, with the direction there.
pub fn sample_every(line: &[Vec3], spacing: f32) -> Vec<(Vec3, Vec3)> {
    let mut out = vec![];
    if line.len() < 2 || spacing <= 0.0 {
        return out;
    }
    let mut next = 0.0f32;
    let mut walked = 0.0f32;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len < 1e-6 {
            continue;
        }
        let dir = (b - a) / len;
        while next <= walked + len + 1e-4 {
            out.push((a + dir * (next - walked).clamp(0.0, len), dir));
            next += spacing;
            if out.len() > 10_000 {
                return out;
            }
        }
        walked += len;
    }
    out
}

impl Document {
    /// The points a selection means: selected points, and every point of a selected path, as
    /// (path index, point index) in path order, once each.
    pub fn point_targets(&self, sel: &[u64]) -> Vec<(usize, usize)> {
        let mut out = vec![];
        for h in sel {
            match self.index.get(h).copied() {
                Some(Loc::Rail(r)) => out.extend((0..Self::points(&self.rails()[r]).len()).map(|p| (r, p))),
                Some(Loc::Point(r, p)) => out.push((r, p)),
                _ => {}
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The paths a selection touches (a point means its path), once each, in selection order.
    pub fn rail_targets(&self, sel: &[u64]) -> Vec<usize> {
        let mut out = vec![];
        for h in sel {
            if let Some(Loc::Rail(r) | Loc::Point(r, _)) = self.index.get(h).copied()
                && !out.contains(&r)
            {
                out.push(r);
            }
        }
        out
    }

    /// A new point like `src` for the path `rail`: new hash and id, no links, handles or
    /// parameters that mark one specific point (start, goal, lap check, checkpoint).
    fn point_like(&mut self, src: &Byml, rail: u64, pos: Vec3, rot: [f32; 3]) -> Byml {
        let mut p = src.clone();
        p.insert("Hash", Byml::U64(self.new_hash()));
        p.insert("InstanceId", Byml::Str(new_uuid()));
        p.insert("Parent", Byml::U64(rail));
        p.insert("Translation", Byml::floats(&pos.to_array()));
        p.insert("Rotate", Byml::floats(&rot));
        for k in ["ParentPrefab", "Links", "Control0", "Control1"] {
            p.remove(k);
        }
        if let Some(d) = p.get_mut("Dynamic") {
            for k in UNIQUE_POINT_KEYS {
                d.remove(k);
            }
            if d.as_map().is_some_and(|m| m.is_empty()) {
                p.remove("Dynamic");
            }
        }
        p
    }

    /// A new path like `src` (type, parameters) with no points yet.
    fn rail_like(&mut self, src: &Byml) -> Byml {
        let mut r = src.clone();
        r.insert("Hash", Byml::U64(self.new_hash()));
        r.insert("InstanceId", Byml::Str(new_uuid()));
        r.insert("IsClosed", Byml::Bool(false));
        r.insert("Points", Byml::Array(vec![]));
        r
    }

    // ------------------------------------------------------------ copy and paste

    /// Copy objects, whole paths, and points (points of one path become a new path of their
    /// own when pasted).
    pub fn copy(&self, sel: &[u64]) -> Clipboard {
        let mut clip = Clipboard::default();
        let mut pts_by_rail: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let whole: HashSet<usize> =
            sel.iter().filter_map(|h| if let Some(Loc::Rail(r)) = self.index.get(h) { Some(*r) } else { None }).collect();
        let mut positions = vec![];
        for h in sel {
            match self.index.get(h).copied() {
                Some(Loc::Actor(i)) => {
                    let a = &self.actors()[i];
                    positions.push(Vec3::from(a.vec3("Translate", [0.0; 3])));
                    clip.actors.push(a.clone());
                }
                Some(Loc::Point(r, p)) if !whole.contains(&r) => pts_by_rail.entry(r).or_default().push(p),
                _ => {}
            }
        }
        let mut rails: Vec<Byml> = whole.iter().map(|&r| self.rails()[r].clone()).collect();
        for (r, mut idx) in pts_by_rail {
            idx.sort_unstable();
            idx.dedup();
            let src = &self.rails()[r];
            let mut rail = src.clone();
            let pts: Vec<Byml> = idx.iter().map(|&i| Self::points(src)[i].clone()).collect();
            rail.insert("Points", Byml::Array(pts));
            if idx.len() != Self::points(src).len() {
                rail.insert("IsClosed", Byml::Bool(false));
            }
            rails.push(rail);
        }
        for r in &rails {
            for p in Self::points(r) {
                positions.push(point_pos(p));
                if let Some(Byml::Hash64(m)) = &self.pad
                    && let Some(e) = m.get(&hash_of(p))
                {
                    clip.pad.insert(hash_of(p), e.clone());
                }
            }
        }
        clip.rails = rails;
        if !positions.is_empty() {
            clip.centre = positions.iter().sum::<Vec3>() / positions.len() as f32;
        }
        clip
    }

    /// Paste a copy moved by `offset`, with new hashes. Links between pasted items follow the
    /// copies; links to items outside the copy are kept when this course has them and dropped
    /// otherwise. Returns the new objects and paths.
    pub fn paste(&mut self, clip: &Clipboard, offset: Vec3) -> Vec<u64> {
        if clip.is_empty() {
            return vec![];
        }
        self.push_undo_as("Paste");
        let mut remap: HashMap<u64, u64> = HashMap::new();
        let mut actors = clip.actors.clone();
        let mut rails = clip.rails.clone();
        for a in &mut actors {
            let h = self.new_hash();
            remap.insert(hash_of(a), h);
            a.insert("Hash", Byml::U64(h));
            a.insert("InstanceId", Byml::Str(new_uuid()));
            a.remove("SRTHash");
            let t = Vec3::from(a.vec3("Translate", [0.0; 3])) + offset;
            a.insert("Translate", Byml::floats(&t.to_array()));
        }
        for r in &mut rails {
            let rh = self.new_hash();
            remap.insert(hash_of(r), rh);
            r.insert("Hash", Byml::U64(rh));
            r.insert("InstanceId", Byml::Str(new_uuid()));
            let new: Vec<u64> = (0..Self::points(r).len()).map(|_| self.new_hash()).collect();
            for (p, nh) in Self::points_mut(r).iter_mut().zip(new) {
                remap.insert(hash_of(p), nh);
                p.insert("Hash", Byml::U64(nh));
                p.insert("InstanceId", Byml::Str(new_uuid()));
                p.insert("Parent", Byml::U64(rh));
                p.insert("Translation", Byml::floats(&(point_pos(p) + offset).to_array()));
                shift_controls(p, offset);
            }
        }
        let here: HashSet<u64> = self.index.keys().copied().collect();
        let fix = |e: &mut Byml| {
            let me = hash_of(e);
            if let Some(Byml::Array(ls)) = e.get_mut("Links") {
                ls.retain_mut(|l| {
                    let d = l.u64_of("Dst").unwrap_or(0);
                    l.insert("Src", Byml::U64(me));
                    match remap.get(&d) {
                        Some(n) => {
                            l.insert("Dst", Byml::U64(*n));
                            true
                        }
                        None => here.contains(&d),
                    }
                });
                if ls.is_empty() {
                    e.remove("Links");
                }
            }
        };
        actors.iter_mut().for_each(&fix);
        for r in &mut rails {
            Self::points_mut(r).iter_mut().for_each(&fix);
        }
        if let Some(Byml::Hash64(m)) = &mut self.pad {
            for (old, e) in &clip.pad {
                if let Some(n) = remap.get(old) {
                    m.insert(*n, e.clone());
                }
            }
        }
        let out: Vec<u64> = actors.iter().chain(rails.iter()).map(hash_of).collect();
        self.actors_mut().extend(actors);
        self.rails_mut().extend(rails);
        self.reindex();
        self.renumber_checkpoints();
        out
    }

    /// Move objects and points (a path means all its points) by `delta`, as one undo step.
    pub fn move_by(&mut self, sel: &[u64], delta: Vec3) -> usize {
        let pts = self.point_targets(sel);
        let actors: Vec<usize> =
            sel.iter().filter_map(|h| if let Some(Loc::Actor(i)) = self.index.get(h) { Some(*i) } else { None }).collect();
        if pts.is_empty() && actors.is_empty() {
            return 0;
        }
        self.push_undo_as("Move");
        for &i in &actors {
            let a = &mut self.actors_mut()[i];
            let t = Vec3::from(a.vec3("Translate", [0.0; 3])) + delta;
            a.insert("Translate", Byml::floats(&t.to_array()));
        }
        for &(r, p) in &pts {
            let e = &mut Self::points_mut(&mut self.rails_mut()[r])[p];
            let t = point_pos(e) + delta;
            e.insert("Translation", Byml::floats(&t.to_array()));
            shift_controls(e, delta);
        }
        actors.len() + pts.len()
    }

    // ------------------------------------------------------------ drawing paths

    /// Add a point at `pos` joined to point `from`: after it when it ends its path, before it
    /// when it starts one (links into the old first point move to the new one), and otherwise
    /// as the first point of a new path of the same type that `from` links to (a branch).
    /// The new point faces away from `from`. Returns the new point.
    pub fn add_point_at(&mut self, from: u64, pos: Vec3) -> Option<u64> {
        let Some(Loc::Point(r, i)) = self.index.get(&from).copied() else { return None };
        let rail = self.rails()[r].clone();
        let pts = Self::points(&rail);
        let n = pts.len();
        let closed = rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false);
        let at = point_pos(&pts[i]);
        if pos.distance(at) < 1e-3 {
            return None;
        }
        let rh = hash_of(&rail);
        self.push_undo_as("Add Point");
        let nh;
        if i == n - 1 {
            let mut np = self.point_like(&pts[i], rh, pos, facing(pos - at));
            nh = hash_of(&np);
            // outgoing links stay on the path's last point
            let list = Self::points_mut(&mut self.rails_mut()[r]);
            if !closed && let Some(ls) = list[i].remove("Links") {
                np.insert("Links", ls);
                set_link_sources(&mut np, nh);
            }
            list.push(np);
        } else if i == 0 && !closed {
            let np = self.point_like(&pts[0], rh, pos, facing(at - pos));
            nh = hash_of(&np);
            Self::points_mut(&mut self.rails_mut()[r]).insert(0, np);
            self.retarget_links(from, nh);
        } else {
            let mut branch = self.rail_like(&rail);
            let bh = hash_of(&branch);
            let np = self.point_like(&pts[i], bh, pos, facing(pos - at));
            nh = hash_of(&np);
            branch.insert("Points", Byml::Array(vec![np]));
            self.rails_mut().push(branch);
            Self::points_mut(&mut self.rails_mut()[r])[i].array_mut("Links").push(link(from, nh, "BasicSignal"));
        }
        self.copy_pad(from, nh);
        self.reindex();
        self.renumber_checkpoints();
        Some(nh)
    }

    /// Point every link into `old` at `new` instead.
    fn retarget_links(&mut self, old: u64, new: u64) {
        let fix = |e: &mut Byml| {
            for l in e.array_mut("Links").iter_mut() {
                if l.u64_of("Dst") == Some(old) {
                    l.insert("Dst", Byml::U64(new));
                }
            }
            if e.array("Links").is_empty() {
                e.remove("Links");
            }
        };
        self.actors_mut().iter_mut().for_each(&fix);
        for r in self.rails_mut().iter_mut() {
            Self::points_mut(r).iter_mut().for_each(&fix);
        }
    }

    /// Where Extrude puts a new point next to `from`: one segment further along the path (a
    /// segment back from a first point), or 10 m ahead along the point's heading.
    pub fn extrude_target(&self, from: u64) -> Option<Vec3> {
        let Some(Loc::Point(r, i)) = self.index.get(&from).copied() else { return None };
        let pts = Self::points(&self.rails()[r]);
        let p = point_pos(&pts[i]);
        let heading = quat_zyx(pts[i].vec3("Rotate", [0.0; 3])) * Vec3::Z;
        let n = pts.len();
        let step = if i == n - 1 && n > 1 {
            p - point_pos(&pts[i - 1])
        } else if i == 0 && n > 1 {
            p - point_pos(&pts[1])
        } else if i + 1 < n {
            heading * point_pos(&pts[i + 1]).distance(p).max(5.0)
        } else {
            heading * 10.0
        };
        Some(p + if step.length() < 0.5 { heading * 10.0 } else { step })
    }

    /// Link point `a` to point `b`, or remove the link between them if there is one (in either
    /// direction). Returns whether they are now connected.
    pub fn toggle_connection(&mut self, a: u64, b: u64) -> Result<bool> {
        let (Some(Loc::Point(..)), Some(Loc::Point(..))) = (self.index.get(&a).copied(), self.index.get(&b).copied()) else {
            return Err("select two path points".into());
        };
        if a == b {
            return Err("select two different points".into());
        }
        let has =
            |d: &Document, s: u64, t: u64| d.find(s).is_some_and(|(_, e)| e.array("Links").iter().any(|l| l.u64_of("Dst") == Some(t)));
        let (ab, ba) = (has(self, a, b), has(self, b, a));
        if ab || ba {
            self.push_undo_as("Disconnect");
            for (s, t) in [(a, b), (b, a)] {
                let loc = self.index[&s];
                let e = self.get_mut(loc).expect("indexed point");
                e.array_mut("Links").retain(|l| l.u64_of("Dst") != Some(t));
                if e.array("Links").is_empty() {
                    e.remove("Links");
                }
            }
            self.renumber_checkpoints();
            return Ok(false);
        }
        self.push_undo_as("Connect");
        let loc = self.index[&a];
        self.get_mut(loc).expect("indexed point").array_mut("Links").push(link(a, b, "BasicSignal"));
        self.renumber_checkpoints();
        Ok(true)
    }

    /// Split a path after point `at`: the points after it become a new path that `at` links
    /// to. A closed loop is opened there instead (its next point becomes the first). Returns a
    /// description for the user.
    pub fn split_rail(&mut self, at: u64) -> Result<String> {
        let Some(Loc::Point(r, i)) = self.index.get(&at).copied() else { return Err("select a path point".into()) };
        let rail = self.rails()[r].clone();
        let n = Self::points(&rail).len();
        let closed = rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false);
        if closed {
            self.push_undo_as("Open Loop");
            let rr = &mut self.rails_mut()[r];
            Self::points_mut(rr).rotate_left(i + 1);
            rr.insert("IsClosed", Byml::Bool(false));
            self.reindex();
            self.renumber_checkpoints();
            return Ok(format!("Opened the loop after point {i}; it now starts at the next point"));
        }
        if i + 1 >= n {
            return Err("pick a point before the end of the path".into());
        }
        self.push_undo_as("Split Path");
        let mut tail = self.rail_like(&rail);
        let th = hash_of(&tail);
        let moved: Vec<Byml> = Self::points_mut(&mut self.rails_mut()[r]).split_off(i + 1);
        let first = hash_of(&moved[0]);
        let moved: Vec<Byml> = moved
            .into_iter()
            .map(|mut p| {
                p.insert("Parent", Byml::U64(th));
                p
            })
            .collect();
        tail.insert("Points", Byml::Array(moved));
        self.rails_mut().push(tail);
        Self::points_mut(&mut self.rails_mut()[r])[i].array_mut("Links").push(link(at, first, "BasicSignal"));
        self.reindex();
        self.renumber_checkpoints();
        Ok(format!("Split into {} and {} points; the first part's last point links to the new path", i + 1, n - i - 1))
    }

    /// Join two paths of the same type into one: the second's points follow the first's. When
    /// one path's last point links to the other's first point, that decides the order (and the
    /// link goes); otherwise `a` comes first.
    pub fn join_rails(&mut self, a: u64, b: u64) -> Result<String> {
        let rails = self.rail_targets(&[a, b]);
        let [ra, rb] = rails[..] else { return Err("select points (or paths) of two different paths".into()) };
        let (x, y) = (&self.rails()[ra], &self.rails()[rb]);
        if x.str_or("Name", "") != y.str_or("Name", "") {
            return Err(format!("can't join a {} to a {}", x.str_or("Name", ""), y.str_or("Name", "")).into());
        }
        let is_closed = |r: &Byml| r.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false);
        if is_closed(x) || is_closed(y) {
            return Err("closed loops can't be joined".into());
        }
        let links_to = |from: &Byml, to: &Byml| {
            let (Some(l), Some(f)) = (Self::points(from).last(), Self::points(to).first()) else { return false };
            l.array("Links").iter().any(|k| k.u64_of("Dst") == Some(hash_of(f)))
        };
        let (first, second) = if !links_to(x, y) && links_to(y, x) { (rb, ra) } else { (ra, rb) };
        self.push_undo_as("Join Paths");
        let fh = hash_of(&self.rails()[first]);
        let tail = self.rails()[second].clone();
        let head_first = Self::points(&tail).first().map(hash_of).unwrap_or(0);
        let differs = self.rails()[first].get("Dynamic") != tail.get("Dynamic");
        let list = Self::points_mut(&mut self.rails_mut()[first]);
        if let Some(last) = list.last_mut() {
            last.array_mut("Links").retain(|l| l.u64_of("Dst") != Some(head_first));
            if last.array("Links").is_empty() {
                last.remove("Links");
            }
        }
        list.extend(Self::points(&tail).iter().cloned().map(|mut p| {
            p.insert("Parent", Byml::U64(fh));
            p
        }));
        self.rails_mut().remove(second);
        self.deleted.insert(hash_of(&tail));
        self.reindex();
        self.renumber_checkpoints();
        let n = self.find(fh).map(|(_, r)| Self::points(r).len()).unwrap_or(0);
        Ok(format!("Joined into one path of {n} points{}", if differs { "; the second path's own parameters were dropped" } else { "" }))
    }

    // ------------------------------------------------------------ reshaping paths

    /// The direction a path runs at one of its points: along its handles when it has them,
    /// else from the previous point to the next.
    fn tangent(&self, r: usize, i: usize) -> Vec3 {
        let rail = &self.rails()[r];
        let pts = Self::points(rail);
        let p = point_pos(&pts[i]);
        if let Some(c1) = control(&pts[i], "Control1").filter(|c| c.distance(p) > 1e-3) {
            return c1 - p;
        }
        if let Some(c0) = control(&pts[i], "Control0").filter(|c| c.distance(p) > 1e-3) {
            return p - c0;
        }
        let closed = rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false) && pts.len() > 2;
        let n = pts.len();
        let prev = if i > 0 {
            Some(i - 1)
        } else if closed {
            Some(n - 1)
        } else {
            None
        };
        let next = if i + 1 < n {
            Some(i + 1)
        } else if closed {
            Some(0)
        } else {
            None
        };
        match (prev, next) {
            (Some(a), Some(b)) => point_pos(&pts[b]) - point_pos(&pts[a]),
            (None, Some(b)) => point_pos(&pts[b]) - p,
            (Some(a), None) => p - point_pos(&pts[a]),
            (None, None) => quat_zyx(pts[i].vec3("Rotate", [0.0; 3])) * Vec3::Z,
        }
    }

    /// Turn points to face along their path, as the shipped paths do (every shipped lap, route,
    /// glide, gravity, pull and off-road point faces its next point). `normal` gives the
    /// surface normal under a point when the stage is loaded: the point's up axis then follows
    /// the road, as shipped points do; otherwise it stays as close to vertical as the slope
    /// allows. Returns how many points turned.
    pub fn orient_points(&mut self, sel: &[u64], normal: &dyn Fn(Vec3) -> Option<Vec3>) -> usize {
        let targets = self.point_targets(sel);
        let rots: Vec<((usize, usize), [f32; 3])> = targets
            .iter()
            .filter_map(|&(r, i)| {
                let p = point_pos(&Self::points(&self.rails()[r])[i]);
                let t = self.tangent(r, i);
                let q = match normal(p) {
                    Some(n) => look_rotation(t - n * t.dot(n), n),
                    None => look_rotation(t, Vec3::Y),
                }?;
                Some(((r, i), euler_from_quat(q)))
            })
            .collect();
        if rots.is_empty() {
            return 0;
        }
        self.push_undo_as("Orient Points");
        for ((r, i), e) in &rots {
            Self::points_mut(&mut self.rails_mut()[*r])[*i].insert("Rotate", Byml::floats(e));
        }
        rots.len()
    }

    /// Give points smooth Bézier handles (Catmull-Rom tangents, each handle a third of its
    /// segment long), so the path curves through them.
    pub fn smooth_handles(&mut self, sel: &[u64]) -> usize {
        let targets = self.point_targets(sel);
        let mut out = vec![];
        for &(r, i) in &targets {
            let rail = &self.rails()[r];
            let pts = Self::points(rail);
            let n = pts.len();
            if n < 2 {
                continue;
            }
            let closed = rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false) && n > 2;
            let p = point_pos(&pts[i]);
            let prev = if i > 0 {
                Some(i - 1)
            } else if closed {
                Some(n - 1)
            } else {
                None
            }
            .map(|j| point_pos(&pts[j]));
            let next = if i + 1 < n {
                Some(i + 1)
            } else if closed {
                Some(0)
            } else {
                None
            }
            .map(|j| point_pos(&pts[j]));
            let (dir, lp, ln) = match (prev, next) {
                (Some(a), Some(b)) => (b - a, a.distance(p), b.distance(p)),
                (None, Some(b)) => (b - p, b.distance(p), b.distance(p)),
                (Some(a), None) => (p - a, a.distance(p), a.distance(p)),
                (None, None) => continue,
            };
            let d = dir.normalize_or_zero();
            if d == Vec3::ZERO {
                continue;
            }
            out.push(((r, i), p - d * lp / 3.0, p + d * ln / 3.0));
        }
        if out.is_empty() {
            return 0;
        }
        self.push_undo_as("Smooth Curve");
        for ((r, i), c0, c1) in &out {
            let e = &mut Self::points_mut(&mut self.rails_mut()[*r])[*i];
            e.insert("Control0", Byml::floats(&c0.to_array()));
            e.insert("Control1", Byml::floats(&c1.to_array()));
        }
        out.len()
    }

    /// Remove the Bézier handles of points, so the path runs straight between them.
    pub fn clear_handles(&mut self, sel: &[u64]) -> usize {
        let targets: Vec<(usize, usize)> = self
            .point_targets(sel)
            .into_iter()
            .filter(|&(r, i)| {
                let p = &Self::points(&self.rails()[r])[i];
                p.get("Control0").is_some() || p.get("Control1").is_some()
            })
            .collect();
        if targets.is_empty() {
            return 0;
        }
        self.push_undo_as("Straighten");
        for &(r, i) in &targets {
            let e = &mut Self::points_mut(&mut self.rails_mut()[r])[i];
            e.remove("Control0");
            e.remove("Control1");
        }
        targets.len()
    }

    /// Insert a point halfway along the segment after each selected point (every segment of a
    /// selected path). Curved segments are split on the curve. Returns the new points.
    pub fn subdivide(&mut self, sel: &[u64]) -> Vec<u64> {
        let hashes: Vec<u64> = self
            .point_targets(sel)
            .into_iter()
            .filter(|&(r, i)| {
                let rail = &self.rails()[r];
                let n = Self::points(rail).len();
                i + 1 < n || (rail.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false) && n > 1)
            })
            .map(|(r, i)| hash_of(&Self::points(&self.rails()[r])[i]))
            .collect();
        if hashes.is_empty() {
            return vec![];
        }
        self.push_undo_as("Subdivide");
        hashes.into_iter().filter_map(|h| self.insert_point_raw(h)).collect()
    }

    /// A new path of type `name` starting at `origin` and heading `yaw`, shaped as `shape`.
    /// Its points copy the checkpoint rectangle of a path of that type in this course and get
    /// path data like it. Returns its last point.
    pub fn new_rail_shaped(&mut self, cat: &Catalog, name: &str, origin: Vec3, yaw: f32, shape: RailShape) -> Option<u64> {
        let def = cat.data.rails.get(name)?.clone();
        let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let right = Vec3::Y.cross(fwd).normalize() * -1.0;
        // (position, handles) per point
        let (pts, closed): (Vec<ShapePoint>, bool) = match shape {
            RailShape::Point => (vec![(origin, None)], false),
            RailShape::Line => (vec![(origin, None), (origin + fwd * 20.0, None)], false),
            RailShape::Curve => {
                let mid = origin + fwd * 20.0 + right * 8.0;
                let end = origin + fwd * 30.0 + right * 24.0;
                let d1 = (end - origin).normalize() * 7.0;
                (
                    vec![
                        (origin, Some((origin - fwd * 6.0, origin + fwd * 6.0))),
                        (mid, Some((mid - d1, mid + d1))),
                        (end, Some((end - right * 5.0, end + right * 5.0))),
                    ],
                    false,
                )
            }
            RailShape::Circle => {
                // starting at `origin` heading `yaw` and turning left
                let r = 7.5;
                let centre = origin - right * r;
                let k = r * 0.552_284_8;
                let ring: Vec<(Vec3, Vec3)> = (0..4)
                    .map(|q| {
                        let a = q as f32 * std::f32::consts::FRAC_PI_2;
                        let out = right * a.cos() + fwd * a.sin();
                        let along = fwd * a.cos() - right * a.sin();
                        (centre + out * r, along)
                    })
                    .collect();
                (ring.into_iter().map(|(p, t)| (p, Some((p - t * k, p + t * k)))).collect(), true)
            }
        };
        self.push_undo_as("New Path");
        let rh = self.new_hash();
        let (size, offset) = self.rail_defaults(name);
        let mut list = vec![];
        for (i, (pos, handles)) in pts.iter().enumerate() {
            let t = match handles {
                Some((c0, c1)) => *c1 - *c0,
                None if pts.len() > 1 => pts[(i + 1).min(pts.len() - 1)].0 - pts[i.saturating_sub(1)].0,
                None => fwd,
            };
            let mut p = Byml::map();
            p.insert("Gyaml", Byml::Str(def.gyaml.clone()));
            p.insert("Hash", Byml::U64(self.new_hash()));
            p.insert("InstanceId", Byml::Str(new_uuid()));
            p.insert("Name", Byml::Str(format!("{name}.Node")));
            p.insert("Parent", Byml::U64(rh));
            p.insert("RectOffset", offset.clone());
            p.insert("RectSize", size.clone());
            p.insert("Rotate", Byml::floats(&facing(t)));
            p.insert("Translation", Byml::floats(&pos.to_array()));
            if let Some((c0, c1)) = handles {
                p.insert("Control0", Byml::floats(&c0.to_array()));
                p.insert("Control1", Byml::floats(&c1.to_array()));
            }
            list.push(p);
        }
        let last = list.last().map(hash_of);
        let mut r = Byml::map();
        r.insert("Gyaml", Byml::Str(def.gyaml));
        r.insert("Hash", Byml::U64(rh));
        r.insert("InstanceId", Byml::Str(new_uuid()));
        r.insert("IsClosed", Byml::Bool(closed));
        r.insert("Name", Byml::Str(name.into()));
        r.insert("Points", Byml::Array(list));
        // path data like the course's other paths of this type (RoutePathGraph has it)
        let template = self
            .rails()
            .iter()
            .filter(|r| r.str_or("Name", "") == name)
            .flat_map(Self::points)
            .map(hash_of)
            .find(|h| matches!(&self.pad, Some(Byml::Hash64(m)) if m.contains_key(h)));
        if let Some(t) = template {
            for h in Self::points(&r).iter().map(hash_of).collect::<Vec<_>>() {
                self.copy_pad(t, h);
            }
        }
        self.rails_mut().push(r);
        self.reindex();
        last
    }

    /// Change paths into another path type: new `Gyaml` and `Name` on the path and its points,
    /// keeping only the parameters the new type's tables have. Returns how many changed.
    pub fn change_rail_type(&mut self, cat: &Catalog, sel: &[u64], name: &str) -> Result<usize> {
        let def = cat.data.rails.get(name).ok_or("unknown path type")?.clone();
        let rails = self.rail_targets(sel);
        if rails.is_empty() {
            return Err("select a path or one of its points".into());
        }
        let keys = |table: &str| -> HashSet<String> {
            cat.data.params.get(table).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default()
        };
        let (rail_keys, point_keys) = (keys(&def.rail_param), keys(&def.point_param));
        let keep = |e: &mut Byml, allowed: &HashSet<String>| {
            if let Some(Byml::Map(m)) = e.get_mut("Dynamic") {
                m.retain(|k, _| allowed.contains(k));
                if m.is_empty() {
                    e.remove("Dynamic");
                }
            }
        };
        self.push_undo_as("Change Path Type");
        for &r in &rails {
            let rr = &mut self.rails_mut()[r];
            rr.insert("Gyaml", Byml::Str(def.gyaml.clone()));
            rr.insert("Name", Byml::Str(name.into()));
            keep(rr, &rail_keys);
            for p in Self::points_mut(rr) {
                p.insert("Gyaml", Byml::Str(def.gyaml.clone()));
                p.insert("Name", Byml::Str(format!("{name}.Node")));
                keep(p, &point_keys);
            }
        }
        self.renumber_checkpoints();
        Ok(rails.len())
    }

    /// Place objects of type `name` along a path every `spacing` metres, facing along it
    /// (`ground` gives the surface height under a position, when the stage is loaded). Returns
    /// the new objects.
    pub fn place_along(&mut self, cat: &Catalog, rail: u64, name: &str, spacing: f32, ground: &dyn Fn(Vec3) -> Option<f32>) -> Vec<u64> {
        let Some(Loc::Rail(r) | Loc::Point(r, _)) = self.index.get(&rail).copied() else { return vec![] };
        if !cat.data.actors.contains_key(name) {
            return vec![];
        }
        let line = rail_curve(&self.rails()[r], 24);
        let spots = sample_every(&line, spacing.max(0.5));
        if spots.is_empty() {
            return vec![];
        }
        self.push_undo_as("Place Along Path");
        let mut out = vec![];
        for (p, dir) in spots {
            let pos = ground(p).map_or(p, |y| Vec3::new(p.x, y, p.z));
            out.extend(self.add_actor_raw(cat, name, pos, dir.x.atan2(dir.z)));
        }
        self.reindex();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::banc::tests::course;
    use crate::workspace::Loaded;

    fn doc() -> Document {
        Document::new(Loaded {
            file: "Banc/Test.bcett.byml.zs".into(),
            from_mod: false,
            version: 7,
            doc: course(),
            pad: None,
            pad_file: None,
        })
    }

    fn pos(d: &Document, h: u64) -> Vec3 {
        d.transform(h).unwrap().0
    }

    fn rail_hashes(d: &Document, r: usize) -> Vec<u64> {
        Document::points(&d.rails()[r]).iter().map(hash_of).collect()
    }

    #[test]
    fn copy_and_paste_remap_links() {
        let mut d = doc();
        d.add_link(1, 2, "BasicSignal").unwrap();
        d.add_link(2, 3, "BasicSignal").unwrap();
        let clip = d.copy(&[1, 2]);
        assert_eq!((clip.len(), clip.centre), (2, Vec3::new(2.5, 0.0, 0.0)));
        let new = d.paste(&clip, Vec3::new(0.0, 0.0, 10.0));
        assert_eq!(new.len(), 2);
        let a = d.find(new[0]).unwrap().1;
        assert_eq!(a.vec3("Translate", [9.0; 3]), [0.0, 0.0, 10.0]);
        // 1 -> 2 became copy -> copy; 2 -> 3 still goes to 3, which this course has
        assert_eq!(a.array("Links")[0].u64_of("Dst"), Some(new[1]));
        assert_eq!(d.find(new[1]).unwrap().1.array("Links")[0].u64_of("Dst"), Some(3));
        assert!(d.undo() && d.actors().len() == 3);
        // into a course without the target, the outside link goes
        let mut other =
            Document::new(Loaded { file: "x".into(), from_mod: false, version: 7, doc: Byml::map(), pad: None, pad_file: None });
        let new = other.paste(&clip, Vec3::ZERO);
        assert!(other.find(new[1]).unwrap().1.get("Links").is_none());
    }

    #[test]
    fn copying_points_makes_a_path() {
        let mut d = doc();
        let clip = d.copy(&[12, 11]);
        assert_eq!(Document::points(&clip.rails[0]).len(), 2);
        let new = d.paste(&clip, Vec3::Y);
        assert_eq!(d.rails().len(), 2);
        let (_, r) = d.find(new[0]).unwrap();
        assert!(Document::points(r).iter().all(|p| p.u64_of("Parent") == Some(new[0])));
        assert_eq!(point_pos(&Document::points(r)[1]), Vec3::new(10.0, 1.0, 0.0));
    }

    #[test]
    fn drawing_extends_prepends_and_branches() {
        let mut d = doc();
        // last point: append, and the new point faces away from it
        let n = d.add_point_at(13, Vec3::new(30.0, 0.0, 0.0)).unwrap();
        assert_eq!(*rail_hashes(&d, 0).last().unwrap(), n);
        let fwd = d.transform(n).unwrap().1 * Vec3::Z;
        assert!(fwd.dot(Vec3::X) > 0.99, "{fwd}");
        // first point: prepend, links into it follow
        d.add_link(1, 11, "BasicSignal").unwrap();
        let f = d.add_point_at(11, Vec3::new(-10.0, 0.0, 0.0)).unwrap();
        assert_eq!(rail_hashes(&d, 0)[0], f);
        assert_eq!(d.find(1).unwrap().1.array("Links")[0].u64_of("Dst"), Some(f));
        // middle point: a new path that the point links to
        let b = d.add_point_at(12, Vec3::new(10.0, 0.0, 15.0)).unwrap();
        assert_eq!(d.rails().len(), 2);
        assert_eq!(d.find(12).unwrap().1.array("Links")[0].u64_of("Dst"), Some(b));
        assert_eq!(d.rail_hash_of(b), Some(hash_of(&d.rails()[1])));
        // extrude continues the last segment
        assert_eq!(d.extrude_target(n), Some(Vec3::new(40.0, 0.0, 0.0)));
    }

    #[test]
    fn connect_toggles() {
        let mut d = doc();
        assert!(d.toggle_connection(13, 11).unwrap());
        assert_eq!(d.find(13).unwrap().1.array("Links").len(), 1);
        assert!(!d.toggle_connection(11, 13).unwrap(), "either direction disconnects");
        assert!(d.find(13).unwrap().1.get("Links").is_none());
        assert!(d.toggle_connection(1, 11).is_err());
    }

    #[test]
    fn split_and_join_round_trip() {
        let mut d = doc();
        d.add_link(13, 1, "BasicSignal").unwrap();
        d.split_rail(11).unwrap();
        assert_eq!((rail_hashes(&d, 0), rail_hashes(&d, 1)), (vec![11], vec![12, 13]));
        assert!(d.find(12).unwrap().1.u64_of("Parent") == Some(hash_of(&d.rails()[1])));
        assert_eq!(d.find(11).unwrap().1.array("Links")[0].u64_of("Dst"), Some(12));
        assert!(d.split_rail(13).is_err(), "nothing after the last point");
        // join in either selection order: the link decides
        d.join_rails(12, 11).unwrap();
        assert_eq!((d.rails().len(), rail_hashes(&d, 0)), (1, vec![11, 12, 13]));
        assert!(d.find(11).unwrap().1.get("Links").is_none());
        assert_eq!(d.find(13).unwrap().1.array("Links")[0].u64_of("Dst"), Some(1), "the end's own links stay");
        // a closed loop opens instead
        d.set_field(10, "IsClosed", Some(Byml::Bool(true)));
        d.split_rail(12).unwrap();
        assert_eq!(rail_hashes(&d, 0), vec![13, 11, 12]);
    }

    #[test]
    fn orient_smooth_subdivide() {
        let mut d = doc();
        // points turned away from the path face along it again
        d.set_vector(12, "Rotate", &[0.0, 2.0, 0.0]);
        assert_eq!(d.orient_points(&[10], &|_| None), 3);
        for h in [11, 12, 13] {
            assert!((d.transform(h).unwrap().1 * Vec3::Z).dot(Vec3::X) > 0.999);
        }
        // up follows a tilted surface
        let n = Vec3::new(0.0, 1.0, 1.0).normalize();
        d.orient_points(&[12], &|_| Some(n));
        assert!((d.transform(12).unwrap().1 * Vec3::Y).dot(n) > 0.999);
        // smooth handles lie along the path; subdividing a curve keeps the curve
        assert_eq!(d.smooth_handles(&[10]), 3);
        let c1 = control(d.find(11).unwrap().1, "Control1").unwrap();
        assert!((c1 - Vec3::new(10.0 / 3.0, 0.0, 0.0)).length() < 1e-4, "{c1}");
        let before = rail_curve(&d.rails()[0], 64);
        let new = d.subdivide(&[11]);
        assert_eq!(new.len(), 1);
        let after = rail_curve(&d.rails()[0], 32);
        let near = |p: Vec3| before.iter().map(|q| q.distance(p)).fold(f32::MAX, f32::min);
        assert!(after.iter().all(|p| near(*p) < 0.2), "the split curve keeps its shape");
        assert_eq!(d.clear_handles(&[10]), 4);
    }

    #[test]
    fn sampling_spacing() {
        let s = sample_every(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 10.0)], 5.0);
        assert_eq!(s.len(), 5);
        assert_eq!(s[3].0, Vec3::new(10.0, 0.0, 5.0));
        assert_eq!(s[3].1, Vec3::Z);
    }

    #[test]
    fn look_rotation_axes() {
        let q = look_rotation(Vec3::new(1.0, 0.0, 1.0), Vec3::Y).unwrap();
        assert!((q * Vec3::Z - Vec3::new(1.0, 0.0, 1.0).normalize()).length() < 1e-5);
        assert!((q * Vec3::Y - Vec3::Y).length() < 1e-5);
        assert!(look_rotation(Vec3::Y, Vec3::Y).is_some(), "straight up still has a frame");
    }

    #[test]
    fn move_by_moves_handles() {
        let mut d = doc();
        d.toggle_curve(12);
        let c = control(d.find(12).unwrap().1, "Control0").unwrap();
        assert_eq!(d.move_by(&[1, 10], Vec3::Y), 4);
        assert_eq!(control(d.find(12).unwrap().1, "Control0").unwrap(), c + Vec3::Y);
        assert_eq!(pos(&d, 1), Vec3::Y);
    }
}

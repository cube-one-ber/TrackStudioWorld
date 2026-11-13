//! Ray queries: a BVH over stage triangles, plus rays against boxes and spheres.

use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

#[derive(Clone, Copy)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    fn empty() -> Self {
        Aabb { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN) }
    }
    fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    fn hit(&self, r: &Ray, inv: Vec3, tmax: f32) -> bool {
        let t0 = (self.min - r.origin) * inv;
        let t1 = (self.max - r.origin) * inv;
        let tmin = t0.min(t1).max_element().max(0.0);
        let tfar = t0.max(t1).min_element().min(tmax);
        tmin <= tfar
    }
}

enum Node {
    Leaf { bounds: Aabb, start: u32, count: u32 },
    Inner { bounds: Aabb, left: u32, right: u32 },
}

pub struct Bvh {
    tris: Vec<[Vec3; 3]>,
    nodes: Vec<Node>,
}

/// Möller–Trumbore; returns distance along the ray.
pub fn ray_triangle(r: &Ray, t: &[Vec3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = r.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = r.origin - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = r.dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d > 1e-4).then_some(d)
}

impl Bvh {
    pub fn new(tris: Vec<[Vec3; 3]>) -> Self {
        let mut b = Bvh { tris, nodes: vec![] };
        if !b.tris.is_empty() {
            let n = b.tris.len();
            b.build(0, n);
        }
        b
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    fn build(&mut self, start: usize, end: usize) -> u32 {
        let mut bounds = Aabb::empty();
        let mut cb = Aabb::empty();
        for t in &self.tris[start..end] {
            for p in t {
                bounds.grow(*p);
            }
            cb.grow((t[0] + t[1] + t[2]) / 3.0);
        }
        let idx = self.nodes.len() as u32;
        if end - start <= 8 {
            self.nodes.push(Node::Leaf { bounds, start: start as u32, count: (end - start) as u32 });
            return idx;
        }
        self.nodes.push(Node::Leaf { bounds, start: 0, count: 0 });
        let ext = cb.max - cb.min;
        let axis = if ext.x >= ext.y && ext.x >= ext.z {
            0
        } else if ext.y >= ext.z {
            1
        } else {
            2
        };
        let mid = (start + end) / 2;
        self.tris[start..end].select_nth_unstable_by(mid - start, |a, b| {
            let ca = (a[0][axis] + a[1][axis] + a[2][axis]) / 3.0;
            let cb = (b[0][axis] + b[1][axis] + b[2][axis]) / 3.0;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let left = self.build(start, mid);
        let right = self.build(mid, end);
        self.nodes[idx as usize] = Node::Inner { bounds, left, right };
        idx
    }

    /// Nearest hit: (distance, triangle normal facing the ray origin).
    pub fn hit(&self, r: &Ray, tmax: f32) -> Option<(f32, Vec3)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / r.dir;
        let mut best: Option<(f32, usize)> = None;
        let mut stack = vec![0u32];
        while let Some(n) = stack.pop() {
            let lim = best.map(|b| b.0).unwrap_or(tmax);
            match &self.nodes[n as usize] {
                Node::Leaf { bounds, start, count } => {
                    if !bounds.hit(r, inv, lim) {
                        continue;
                    }
                    let first = *start as usize;
                    for (i, tri) in self.tris[first..first + *count as usize].iter().enumerate() {
                        if let Some(d) = ray_triangle(r, tri)
                            && d < best.map(|b| b.0).unwrap_or(tmax)
                        {
                            best = Some((d, first + i));
                        }
                    }
                }
                Node::Inner { bounds, left, right } => {
                    if bounds.hit(r, inv, lim) {
                        stack.push(*left);
                        stack.push(*right);
                    }
                }
            }
        }
        best.map(|(d, i)| {
            let t = &self.tris[i];
            let mut n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            if n.dot(r.dir) > 0.0 {
                n = -n;
            }
            (d, n)
        })
    }

    /// Height of the first surface below `p` (searching from a little above it).
    pub fn ground_below(&self, p: Vec3) -> Option<f32> {
        for lift in [3.0, 40.0, 400.0] {
            let r = Ray { origin: p + Vec3::Y * lift, dir: Vec3::NEG_Y };
            if let Some((d, _)) = self.hit(&r, 5000.0) {
                return Some(r.origin.y - d);
            }
        }
        None
    }

    /// Normal (facing up) of the first surface below `p`, searched like [`ground_below`](Self::ground_below).
    pub fn normal_below(&self, p: Vec3) -> Option<Vec3> {
        for lift in [3.0, 40.0, 400.0] {
            let r = Ray { origin: p + Vec3::Y * lift, dir: Vec3::NEG_Y };
            if let Some((_, n)) = self.hit(&r, 5000.0) {
                return Some(n);
            }
        }
        None
    }
}

/// The region a rectangle on screen covers, from the rays through its four corners (in order
/// around it): a pyramid for a perspective camera, a box for an orthographic one.
pub struct Frustum {
    planes: [(Vec3, Vec3); 4],
    origin: Vec3,
    forward: Vec3,
}

impl Frustum {
    pub fn from_rays(c: &[Ray; 4]) -> Frustum {
        let origin = c.iter().map(|r| r.origin).sum::<Vec3>() / 4.0;
        let forward = c.iter().map(|r| r.dir).sum::<Vec3>().normalize_or(Vec3::NEG_Z);
        let inside = origin + forward * 50.0;
        let planes = [0, 1, 2, 3].map(|i| {
            let (a, b) = (&c[i], &c[(i + 1) % 4]);
            let (p0, p1, p2) = (a.origin, a.origin + a.dir * 100.0, b.origin + b.dir * 100.0);
            let mut n = (p1 - p0).cross(p2 - p0).normalize_or_zero();
            if n.dot(inside - p0) < 0.0 {
                n = -n;
            }
            (p0, n)
        });
        Frustum { planes, origin, forward }
    }

    pub fn contains(&self, p: Vec3) -> bool {
        self.forward.dot(p - self.origin) > 0.0 && self.planes.iter().all(|(o, n)| n.dot(p - *o) >= 0.0)
    }
}

/// Ray against an oriented box: `world` maps the unit-free local box [min, max] to world.
pub fn ray_obb(r: &Ray, world: Mat4, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = world.inverse();
    let o = inv.transform_point3(r.origin);
    let d = inv.transform_vector3(r.dir);
    let id = Vec3::ONE / d;
    let t0 = (min - o) * id;
    let t1 = (max - o) * id;
    let tmin = t0.min(t1).max_element();
    let tmax = t0.max(t1).min_element();
    if tmax < tmin.max(0.0) {
        return None;
    }
    let local_hit = o + d * tmin.max(0.0);
    Some(world.transform_point3(local_hit).distance(r.origin))
}

pub fn ray_sphere(r: &Ray, c: Vec3, radius: f32) -> Option<f32> {
    let oc = r.origin - c;
    let b = oc.dot(r.dir);
    let cc = oc.length_squared() - radius * radius;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t > 0.0).then_some(t)
}

/// Closest point parameters between a ray and an infinite line (point `p`, direction `a`).
/// Returns the position along the line, or None when they are parallel.
pub fn ray_line_param(r: &Ray, p: Vec3, a: Vec3) -> Option<f32> {
    let w0 = p - r.origin;
    let aa = a.dot(a);
    let ab = a.dot(r.dir);
    let bb = r.dir.dot(r.dir);
    let denom = aa * bb - ab * ab;
    if denom.abs() < 1e-8 {
        return None;
    }
    let ad = a.dot(w0);
    let bd = r.dir.dot(w0);
    Some((ab * bd - bb * ad) / denom)
}

/// Where a ray meets a plane.
pub fn ray_plane(r: &Ray, p: Vec3, n: Vec3) -> Option<Vec3> {
    let denom = n.dot(r.dir);
    if denom.abs() < 1e-8 {
        return None;
    }
    let t = n.dot(p - r.origin) / denom;
    (t > 0.0).then(|| r.origin + r.dir * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_from_corner_rays() {
        // a camera at the origin looking down -Z, a box over the middle of the screen
        let o = Vec3::ZERO;
        let rays =
            [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)].map(|(x, y)| Ray { origin: o, dir: Vec3::new(x, y, -4.0).normalize() });
        let f = Frustum::from_rays(&rays);
        assert!(f.contains(Vec3::new(0.0, 0.0, -10.0)) && f.contains(Vec3::new(2.0, -2.0, -10.0)));
        assert!(!f.contains(Vec3::new(3.0, 0.0, -10.0)) && !f.contains(Vec3::new(0.0, 0.0, 10.0)), "outside, and behind");
        // orthographic: parallel rays from different origins
        let rays =
            [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)].map(|(x, z)| Ray { origin: Vec3::new(x, 100.0, z), dir: Vec3::NEG_Y });
        let f = Frustum::from_rays(&rays);
        assert!(f.contains(Vec3::new(0.5, 0.0, -0.5)) && !f.contains(Vec3::new(1.5, 0.0, 0.0)));
    }

    #[test]
    fn nearest_hit_and_its_normal() {
        // a row of triangles, each at its own height and tilt, so a hit on the wrong one shows
        let tri = |i: usize| {
            let (x, y, slope) = (i as f32 * 10.0, i as f32, i as f32 * 0.1);
            [Vec3::new(x - 4.0, y, -4.0), Vec3::new(x - 4.0, y, 4.0), Vec3::new(x + 4.0, y + 8.0 * slope, 0.0)]
        };
        let bvh = Bvh::new((0..40).map(tri).collect());
        for i in 0..40 {
            let t = tri(i);
            let at = (t[0] + t[1] + t[2]) / 3.0;
            let r = Ray { origin: Vec3::new(at.x, 100.0, at.z), dir: Vec3::NEG_Y };
            let (d, n) = bvh.hit(&r, 1000.0).expect("a triangle under the ray");
            let mut want = (t[1] - t[0]).cross(t[2] - t[0]).normalize();
            if want.y < 0.0 {
                want = -want;
            }
            assert!((d - (100.0 - at.y)).abs() < 1e-3 && n.distance(want) < 1e-4, "triangle {i}: {d} {n}");
        }
        assert!(bvh.hit(&Ray { origin: Vec3::new(-50.0, 100.0, 0.0), dir: Vec3::NEG_Y }, 1000.0).is_none());
    }
}

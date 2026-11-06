//! Transform conventions of Banc files.
//!
//! `Rotate` holds Euler angles in radians, applied X first, then Y, then Z: R = Rz·Ry·Rx. This
//! comes from the game's actor loader (version 1.8.0, function 0x29b9f0): after reading the
//! three `Rotate` values it calls sinf and cosf on each and writes the matrix entries
//! m00 = cy·cz, m01 = sx·sy·cz − cx·sz, m11 = sx·sy·sz + cx·cz, m20 = −sy, which are those of
//! Rz·Ry·Rx. The shipped data agrees: over 250 tilted path points, the point's up axis is 0.6°
//! (median) from the road normal with this order and 8.7° with the reverse one.

use glam::{EulerRot, Mat3, Quat, Vec3};

/// Rotation matrix for Banc Euler angles.
pub fn euler_zyx(rot: [f32; 3]) -> Mat3 {
    Mat3::from_rotation_z(rot[2]) * Mat3::from_rotation_y(rot[1]) * Mat3::from_rotation_x(rot[0])
}

/// The same rotation as a quaternion.
pub fn quat_zyx(rot: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::ZYX, rot[2], rot[1], rot[0])
}

/// Banc Euler angles for a rotation (inverse of [`quat_zyx`]).
pub fn euler_from_quat(q: Quat) -> [f32; 3] {
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    [x, y, z]
}

/// World-space bounds of a local box [min, max] scaled, rotated and moved like an actor.
pub fn world_aabb(aabb: [[f32; 3]; 2], t: [f32; 3], r: [f32; 3], s: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let m = euler_zyx(r);
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    for cx in [aabb[0][0], aabb[1][0]] {
        for cy in [aabb[0][1], aabb[1][1]] {
            for cz in [aabb[0][2], aabb[1][2]] {
                let p = m * Vec3::new(cx * s[0], cy * s[1], cz * s[2]) + Vec3::from(t);
                mn = mn.min(p);
                mx = mx.max(p);
            }
        }
    }
    (mn.into(), mx.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_quaternion_and_inverse_agree() {
        let r = [0.3f32, -1.1, 0.7];
        let v = Vec3::new(0.2, 1.0, -0.5);
        assert!((euler_zyx(r) * v - quat_zyx(r) * v).length() < 1e-5);
        assert!((quat_zyx(euler_from_quat(quat_zyx(r))) * v - quat_zyx(r) * v).length() < 1e-5);
    }

    #[test]
    fn matches_the_game_matrix() {
        // the entries the game's loader computes (see the module docs)
        let (x, y, z) = (0.4f32, -0.9f32, 1.3f32);
        let (sx, cx, sy, cy, sz, cz) = (x.sin(), x.cos(), y.sin(), y.cos(), z.sin(), z.cos());
        let m = euler_zyx([x, y, z]);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(m.col(0).x, cy * cz));
        assert!(close(m.col(1).x, sx * sy * cz - cx * sz));
        assert!(close(m.col(1).y, sx * sy * sz + cx * cz));
        assert!(close(m.col(0).z, -sy));
        assert!(close(m.col(1).z, sx * cy));
        assert!(close(m.col(2).z, cx * cy));
    }

    #[test]
    fn world_aabb_of_rotated_box() {
        let (mn, mx) =
            world_aabb([[-1.0, 0.0, -2.0], [1.0, 1.0, 2.0]], [10.0, 0.0, 0.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0], [1.0; 3]);
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(close(mn, [8.0, 0.0, -1.0]) && close(mx, [12.0, 1.0, 1.0]), "{mn:?} {mx:?}");
    }
}

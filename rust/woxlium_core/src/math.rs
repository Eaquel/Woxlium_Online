//! Minimal linear algebra for the game (column-major matrices, like GL).

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub const ZERO: V2 = v2(0.0, 0.0);
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn len2(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
    pub fn norm(self) -> V2 {
        let l = self.len();
        if l > 1e-6 {
            v2(self.x / l, self.y / l)
        } else {
            V2::ZERO
        }
    }
    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    pub fn perp(self) -> V2 {
        v2(-self.y, self.x)
    }
    pub fn lerp(self, o: V2, t: f32) -> V2 {
        self + (o - self) * t
    }
    pub fn from_angle(a: f32) -> V2 {
        v2(a.cos(), a.sin())
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, s: f32) -> V2 {
        v2(self.x * s, self.y * s)
    }
}
impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}
impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl SubAssign for V2 {
    fn sub_assign(&mut self, o: V2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            self
        }
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}

/// Column-major 4x4 matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M4(pub [f32; 16]);

impl M4 {
    pub fn identity() -> M4 {
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        M4(m)
    }

    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> M4 {
        let f = 1.0 / (fovy * 0.5).tan();
        let mut m = [0.0; 16];
        m[0] = f / aspect;
        m[5] = f;
        m[10] = (far + near) / (near - far);
        m[11] = -1.0;
        m[14] = 2.0 * far * near / (near - far);
        M4(m)
    }

    pub fn ortho(l: f32, r: f32, b: f32, t: f32) -> M4 {
        let mut m = M4::identity().0;
        m[0] = 2.0 / (r - l);
        m[5] = 2.0 / (t - b);
        m[10] = -1.0;
        m[12] = -(r + l) / (r - l);
        m[13] = -(t + b) / (t - b);
        M4(m)
    }

    pub fn ortho_depth(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> M4 {
        let mut m = M4::ortho(l, r, b, t).0;
        m[10] = -2.0 / (f - n);
        m[14] = -(f + n) / (f - n);
        M4(m)
    }

    pub fn look_at(eye: V3, target: V3, up: V3) -> M4 {
        let f = (target - eye).norm();
        let s = f.cross(up).norm();
        let u = s.cross(f);
        let mut m = M4::identity().0;
        m[0] = s.x;
        m[4] = s.y;
        m[8] = s.z;
        m[1] = u.x;
        m[5] = u.y;
        m[9] = u.z;
        m[2] = -f.x;
        m[6] = -f.y;
        m[10] = -f.z;
        m[12] = -s.dot(eye);
        m[13] = -u.dot(eye);
        m[14] = f.dot(eye);
        M4(m)
    }

    pub fn mul(&self, o: &M4) -> M4 {
        let a = &self.0;
        let b = &o.0;
        let mut r = [0.0; 16];
        for c in 0..4 {
            for row in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += a[k * 4 + row] * b[c * 4 + k];
                }
                r[c * 4 + row] = s;
            }
        }
        M4(r)
    }

    /// Transform a point, returning clip-space (x, y, z, w).
    pub fn transform(&self, p: V3) -> [f32; 4] {
        let m = &self.0;
        [
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
            m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15],
        ]
    }
}

pub fn clamp(x: f32, a: f32, b: f32) -> f32 {
    x.max(a).min(b)
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Small, fast deterministic RNG (xorshift32).
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n.max(1)
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_at_maps_target_to_center() {
        let eye = v3(0.0, 10.0, 8.0);
        let view = M4::look_at(eye, v3(0.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
        let proj = M4::perspective(0.9, 1.6, 0.1, 100.0);
        let c = proj.mul(&view).transform(v3(0.0, 0.0, 0.0));
        assert!((c[0] / c[3]).abs() < 1e-4 && (c[1] / c[3]).abs() < 1e-4);
        assert!(c[2] / c[3] > -1.0 && c[2] / c[3] < 1.0);
    }

    #[test]
    fn rng_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let x = r.f();
            assert!((0.0..1.0).contains(&x));
        }
    }
}

// ------------------------------------------------------------------ 3D extras
impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
    pub fn xz(self) -> V2 {
        v2(self.x, self.z)
    }
}

impl std::ops::Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// Unit quaternion (w, x, y, z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat { w: 1.0, x: 0.0, y: 0.0, z: 0.0 };

    pub fn from_axis_angle(axis: V3, angle: f32) -> Quat {
        let a = axis.norm();
        let (s, c) = (angle * 0.5).sin_cos();
        Quat { w: c, x: a.x * s, y: a.y * s, z: a.z * s }
    }

    pub fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    pub fn dot(self, o: Quat) -> f32 {
        self.w * o.w + self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn normalize(self) -> Quat {
        let l = self.dot(self).sqrt().max(1e-12);
        Quat { w: self.w / l, x: self.x / l, y: self.y / l, z: self.z / l }
    }

    /// Normalised lerp along the shortest arc.
    pub fn nlerp(self, o: Quat, t: f32) -> Quat {
        let s = if self.dot(o) < 0.0 { -1.0 } else { 1.0 };
        Quat {
            w: self.w + (o.w * s - self.w) * t,
            x: self.x + (o.x * s - self.x) * t,
            y: self.y + (o.y * s - self.y) * t,
            z: self.z + (o.z * s - self.z) * t,
        }
        .normalize()
    }

    pub fn rotate(self, v: V3) -> V3 {
        let u = v3(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
}

impl M4 {
    /// Translation * rotation (no scale).
    pub fn from_tr(t: V3, q: Quat) -> M4 {
        let (w, x, y, z) = (q.w, q.x, q.y, q.z);
        let mut m = [0.0; 16];
        m[0] = 1.0 - 2.0 * (y * y + z * z);
        m[1] = 2.0 * (x * y + w * z);
        m[2] = 2.0 * (x * z - w * y);
        m[4] = 2.0 * (x * y - w * z);
        m[5] = 1.0 - 2.0 * (x * x + z * z);
        m[6] = 2.0 * (y * z + w * x);
        m[8] = 2.0 * (x * z + w * y);
        m[9] = 2.0 * (y * z - w * x);
        m[10] = 1.0 - 2.0 * (x * x + y * y);
        m[12] = t.x;
        m[13] = t.y;
        m[14] = t.z;
        m[15] = 1.0;
        M4(m)
    }

    pub fn translation(&self) -> V3 {
        v3(self.0[12], self.0[13], self.0[14])
    }

    pub fn transform_point(&self, p: V3) -> V3 {
        let c = self.transform(p);
        v3(c[0], c[1], c[2])
    }

    pub fn transform_dir(&self, d: V3) -> V3 {
        let m = &self.0;
        v3(m[0] * d.x + m[4] * d.y + m[8] * d.z, m[1] * d.x + m[5] * d.y + m[9] * d.z, m[2] * d.x + m[6] * d.y + m[10] * d.z)
    }

    pub fn scale(s: V3) -> M4 {
        let mut m = M4::identity();
        m.0[0] = s.x;
        m.0[5] = s.y;
        m.0[10] = s.z;
        m
    }

    /// Inverse of a rigid transform (rotation + translation only).
    pub fn rigid_inverse(&self) -> M4 {
        let m = &self.0;
        let mut r = M4::identity().0;
        for i in 0..3 {
            for j in 0..3 {
                r[i * 4 + j] = m[j * 4 + i];
            }
        }
        let t = self.translation();
        r[12] = -(r[0] * t.x + r[4] * t.y + r[8] * t.z);
        r[13] = -(r[1] * t.x + r[5] * t.y + r[9] * t.z);
        r[14] = -(r[2] * t.x + r[6] * t.y + r[10] * t.z);
        M4(r)
    }
}

#[cfg(test)]
mod quat_tests {
    use super::*;

    #[test]
    fn quat_matches_matrix() {
        let q = Quat::from_axis_angle(v3(0.3, 1.0, 0.2), 1.1);
        let p = v3(0.5, -1.0, 2.0);
        let a = q.rotate(p);
        let b = M4::from_tr(V3::ZERO, q).transform_point(p);
        assert!((a - b).len() < 1e-5);
        let m = M4::from_tr(v3(1.0, 2.0, 3.0), q);
        let back = m.rigid_inverse().mul(&m).transform_point(p);
        assert!((back - p).len() < 1e-4);
    }
}

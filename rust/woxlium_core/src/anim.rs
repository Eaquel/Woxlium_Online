//! Skeletal animation: clip sampling, cross-fading and skin matrices.

use crate::math::{Quat, M4, V3};
use crate::wxm::Model;

/// Local pose: root translation + per-bone local rotations.
#[derive(Clone)]
pub struct Pose {
    pub root: V3,
    pub rot: Vec<Quat>,
}

impl Pose {
    pub fn new(n: usize) -> Pose {
        Pose { root: V3::ZERO, rot: vec![Quat::IDENTITY; n] }
    }

    pub fn rest(model: &Model) -> Pose {
        Pose { root: model.bones.first().map(|b| b.rest_t).unwrap_or(V3::ZERO), rot: model.bones.iter().map(|b| b.rest_q).collect() }
    }

    /// Sample `clip` at time `t` (seconds) into self.
    pub fn sample(&mut self, model: &Model, clip: usize, t: f32) {
        let c = &model.clips[clip];
        let n = model.bones.len();
        let last = (c.frames - 1) as f32;
        let mut f = t * c.fps;
        f = if c.looping { f.rem_euclid(last.max(1.0)) } else { f.clamp(0.0, last) };
        let i0 = f.floor() as usize;
        let i1 = (i0 + 1).min(c.frames as usize - 1);
        let k = f - i0 as f32;
        self.root = c.root[i0].lerp(c.root[i1], k);
        for b in 0..n {
            self.rot[b] = c.rot[i0 * n + b].nlerp(c.rot[i1 * n + b], k);
        }
    }

    /// self = lerp(self, other, w)
    pub fn blend(&mut self, other: &Pose, w: f32) {
        if w <= 0.0 {
            return;
        }
        self.root = self.root.lerp(other.root, w);
        for (a, b) in self.rot.iter_mut().zip(&other.rot) {
            *a = a.nlerp(*b, w);
        }
    }

    /// Model-space bone matrices.
    pub fn globals(&self, model: &Model, out: &mut Vec<M4>) {
        out.clear();
        for (i, b) in model.bones.iter().enumerate() {
            let m = if b.parent < 0 {
                M4::from_tr(self.root, self.rot[i])
            } else {
                out[b.parent as usize].mul(&M4::from_tr(b.rest_t, self.rot[i]))
            };
            out.push(m);
        }
    }
}

/// Plays one clip and cross-fades from the previous one.
#[derive(Clone)]
pub struct Animator {
    pub clip: usize,
    pub time: f32,
    pub speed: f32,
    prev: usize,
    prev_time: f32,
    fade: f32,
    fade_len: f32,
}

impl Animator {
    pub fn new(clip: usize) -> Animator {
        Animator { clip, time: 0.0, speed: 1.0, prev: clip, prev_time: 0.0, fade: 0.0, fade_len: 0.0 }
    }

    /// Switch clip (no-op if already playing it, unless `restart`).
    pub fn play(&mut self, clip: usize, fade: f32, restart: bool) {
        if clip == self.clip && !restart {
            return;
        }
        self.prev = self.clip;
        self.prev_time = self.time;
        self.clip = clip;
        self.time = 0.0;
        self.fade = fade;
        self.fade_len = fade.max(1e-4);
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt * self.speed;
        self.prev_time += dt * self.speed;
        self.fade = (self.fade - dt).max(0.0);
    }

    pub fn finished(&self, model: &Model) -> bool {
        let c = &model.clips[self.clip];
        !c.looping && self.time >= c.duration()
    }

    pub fn progress(&self, model: &Model) -> f32 {
        (self.time / model.clips[self.clip].duration()).clamp(0.0, 1.0)
    }

    pub fn evaluate(&self, model: &Model, pose: &mut Pose, scratch: &mut Pose) {
        pose.sample(model, self.clip, self.time);
        if self.fade > 0.0 {
            scratch.sample(model, self.prev, self.prev_time);
            let w = self.fade / self.fade_len;
            let mut cur = std::mem::replace(pose, scratch.clone());
            // pose currently = previous clip; blend towards current
            cur.blend(pose, w);
            *pose = cur;
        }
    }
}

/// Pack skin matrices (global * inverse bind) as 3 rows of vec4 for the shader.
pub fn skin_rows(model: &Model, globals: &[M4], out: &mut Vec<[f32; 4]>) {
    out.clear();
    for (g, b) in globals.iter().zip(&model.bones) {
        let m = g.mul(&b.inv_bind).0;
        out.push([m[0], m[4], m[8], m[12]]);
        out.push([m[1], m[5], m[9], m[13]]);
        out.push([m[2], m[6], m[10], m[14]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> Option<Model> {
        let p = format!("{}/../../build/wxm/{name}.wxm", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(p).ok().map(|b| Model::parse(&b).expect("parse"))
    }

    #[test]
    fn rest_pose_reproduces_bind_pose() {
        let Some(m) = load("zombie_a") else { return };
        let pose = Pose::rest(&m);
        let mut g = Vec::new();
        pose.globals(&m, &mut g);
        for (gm, b) in g.iter().zip(&m.bones) {
            let id = gm.mul(&b.inv_bind);
            for (i, x) in id.0.iter().enumerate() {
                let e = if i % 5 == 0 { 1.0 } else { 0.0 };
                assert!((x - e).abs() < 2e-3, "bone {} not identity: {:?}", b.name, id.0);
            }
        }
    }

    #[test]
    fn clips_keep_character_upright_and_grounded() {
        let Some(m) = load("zombie_a") else { return };
        let head = m.bone("Bip01 Head").unwrap();
        let foot = m.bone("Bip01 L Toe0").unwrap();
        let mut pose = Pose::new(m.bones.len());
        let mut g = Vec::new();
        for name in ["idle", "walk", "run", "attack"] {
            let c = m.clip(name).unwrap();
            pose.sample(&m, c, 0.4);
            pose.globals(&m, &mut g);
            let hy = g[head].translation().y;
            let fy = g[foot].translation().y;
            assert!(hy > 1.3 && hy < 1.9, "{name}: head at {hy}");
            assert!(fy > -0.1 && fy < 0.5, "{name}: toe at {fy}");
        }
    }
}


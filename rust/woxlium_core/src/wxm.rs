//! Loader for `.wxm` (Woxlium mesh) files written by tools/blender/wxm_export.py.

use crate::math::{v3, Quat, M4, V3};

pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub rest_t: V3,
    pub rest_q: Quat,
    pub inv_bind: M4,
}

pub struct SubMesh {
    pub material: String,
    pub first: u32,
    pub count: u32,
}

pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub frames: u32,
    pub looping: bool,
    /// per frame: root translation
    pub root: Vec<V3>,
    /// per frame * bone: local rotation
    pub rot: Vec<Quat>,
}

impl Clip {
    pub fn duration(&self) -> f32 {
        (self.frames.max(2) - 1) as f32 / self.fps
    }
}

/// Raw vertex layout shared with the GPU (36 bytes).
pub const VERTEX_SIZE: usize = 36;

pub struct Model {
    pub bones: Vec<Bone>,
    pub subs: Vec<SubMesh>,
    pub vertex_count: u32,
    /// interleaved: pos f32x3, normal i8x4, tangent i8x4, uv f32x2, joints u8x4, weights u8x4
    pub vertices: Vec<u8>,
    /// u16 or u32 indices, little endian (see `wide`)
    pub index_bytes: Vec<u8>,
    pub wide: bool,
    pub index_count: u32,
    pub clips: Vec<Clip>,
    pub bounds_min: V3,
    pub bounds_max: V3,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.b.len() {
            return Err(format!("wxm truncated at {}", self.p));
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn v3(&mut self) -> Result<V3, String> {
        Ok(v3(self.f32()?, self.f32()?, self.f32()?))
    }
    fn name(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

impl Model {
    pub fn parse(bytes: &[u8]) -> Result<Model, String> {
        let mut r = Reader { b: bytes, p: 0 };
        let magic = r.take(4)?;
        let wide = match magic {
            b"WXM1" => false,
            b"WXM2" => true,
            _ => return Err("not a WXM file".into()),
        };
        let nb = r.u32()? as usize;
        let mut bones = Vec::with_capacity(nb);
        for _ in 0..nb {
            let name = r.name()?;
            let parent = r.i32()?;
            let rest_t = r.v3()?;
            let rest_q = Quat { w: r.f32()?, x: r.f32()?, y: r.f32()?, z: r.f32()? };
            let mut m = [0.0; 16];
            for x in &mut m {
                *x = r.f32()?;
            }
            bones.push(Bone { name, parent, rest_t, rest_q, inv_bind: M4(m) });
        }
        let ns = r.u32()? as usize;
        let mut subs = Vec::with_capacity(ns);
        for _ in 0..ns {
            subs.push(SubMesh { material: r.name()?, first: r.u32()?, count: r.u32()? });
        }
        let nv = r.u32()?;
        let vertices = r.take(nv as usize * VERTEX_SIZE)?.to_vec();
        let ni = r.u32()? as usize;
        let index_bytes = r.take(ni * if wide { 4 } else { 2 })?.to_vec();
        let nc = r.u32()? as usize;
        let mut clips = Vec::with_capacity(nc);
        for _ in 0..nc {
            let name = r.name()?;
            let fps = r.f32()?;
            let frames = r.u32()?;
            let looping = r.u8()? != 0;
            let mut root = Vec::with_capacity(frames as usize);
            let mut rot = Vec::with_capacity(frames as usize * nb);
            for _ in 0..frames {
                root.push(r.v3()?);
                for _ in 0..nb {
                    let s = 1.0 / 32767.0;
                    rot.push(Quat { w: r.i16()? as f32 * s, x: r.i16()? as f32 * s, y: r.i16()? as f32 * s, z: r.i16()? as f32 * s }.normalize());
                }
            }
            clips.push(Clip { name, fps, frames, looping, root, rot });
        }
        let mut lo = v3(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = v3(f32::MIN, f32::MIN, f32::MIN);
        for v in vertices.chunks_exact(VERTEX_SIZE) {
            let f = |o: usize| f32::from_le_bytes(v[o..o + 4].try_into().unwrap());
            let p = v3(f(0), f(4), f(8));
            lo = v3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = v3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Ok(Model { bones, subs, vertex_count: nv, vertices, index_bytes, wide, index_count: ni as u32, clips, bounds_min: lo, bounds_max: hi })
    }

    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
}

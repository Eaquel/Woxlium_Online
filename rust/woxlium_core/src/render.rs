//! OpenGL ES 3.0 renderer: GPU-skinned characters, normal-mapped props and
//! surfaces, moonlight shadow map, flashlight + street lamps, fog, sky dome,
//! first-person arms, VFX billboards and the minimap overlay.

use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;

use crate::anim::{skin_rows, Pose};
use crate::assets::AssetSource;
use crate::assets_gen::*;
use crate::game::{Game, ZState};
use crate::gl::*;
use crate::level::{Block, MAX_X, MAX_Z, MIN_X, MIN_Z, M_MEDKIT};
use crate::math::{clamp, v3, Quat, M4, V2, V3};
use crate::texture::upload_pvr;
use crate::wxm::{Model, VERTEX_SIZE};

const LIT_VS: &str = r#"#version 300 es
layout(location=0) in vec3 aPos;
layout(location=1) in vec4 aNrm;
layout(location=2) in vec4 aTan;
layout(location=3) in vec2 aUV;
layout(location=4) in vec4 aJoints;
layout(location=5) in vec4 aWeights;
uniform mat4 uVP;
uniform mat4 uModel;
uniform mat4 uShadowVP;
#ifdef SKINNED
uniform vec4 uBones[156];
mat4 bone(int i) {
  return transpose(mat4(uBones[i * 3], uBones[i * 3 + 1], uBones[i * 3 + 2], vec4(0.0, 0.0, 0.0, 1.0)));
}
#endif
out vec3 vPos; out vec2 vUV; out vec3 vN; out vec4 vT; out vec4 vShadow;
void main() {
  vec4 p = vec4(aPos, 1.0);
  vec3 n = aNrm.xyz;
  vec3 t = aTan.xyz;
#ifdef SKINNED
  mat4 s = bone(int(aJoints.x)) * aWeights.x + bone(int(aJoints.y)) * aWeights.y
         + bone(int(aJoints.z)) * aWeights.z + bone(int(aJoints.w)) * aWeights.w;
  p = s * p; n = mat3(s) * n; t = mat3(s) * t;
#endif
  vec4 w = uModel * p;
  vPos = w.xyz;
  vN = mat3(uModel) * n;
  vT = vec4(mat3(uModel) * t, aTan.w);
  vUV = aUV;
  vShadow = uShadowVP * w;
  gl_Position = uVP * w;
}"#;

const LIT_FS: &str = r#"#version 300 es
precision highp float;
precision highp sampler2DShadow;
uniform sampler2D uAlbedo;
uniform sampler2D uNormal;
uniform sampler2DShadow uShadow;
uniform vec3 uCamPos;
uniform vec3 uSunDir;
uniform vec3 uSunColor;
uniform vec3 uAmbSky;
uniform vec3 uAmbGround;
uniform vec3 uFlashPos;
uniform vec3 uFlashDir;
uniform vec3 uFlashColor;
uniform vec4 uLights[8];   // 4 x (pos.xyz, radius), (color.rgb, -)
uniform vec4 uFog;         // rgb, density
uniform vec4 uTint;        // rgb multiplier, a = emissive strength
uniform vec4 uMat;         // x roughness, y alpha test, z shadows on, w hit flash
uniform vec2 uWorldUV;     // >0: planar world mapping with this tile size (generated surfaces)
in vec3 vPos; in vec2 vUV; in vec3 vN; in vec4 vT; in vec4 vShadow;
out vec4 frag;

float shadow() {
  if (uMat.z < 0.5) return 1.0;
  vec3 c = vShadow.xyz / vShadow.w * 0.5 + 0.5;
  if (c.x < 0.0 || c.x > 1.0 || c.y < 0.0 || c.y > 1.0 || c.z > 1.0) return 1.0;
  float o = 1.0 / 1536.0, s = 0.0;
  s += texture(uShadow, vec3(c.xy + vec2(-o, -o), c.z - 0.0015));
  s += texture(uShadow, vec3(c.xy + vec2( o, -o), c.z - 0.0015));
  s += texture(uShadow, vec3(c.xy + vec2(-o,  o), c.z - 0.0015));
  s += texture(uShadow, vec3(c.xy + vec2( o,  o), c.z - 0.0015));
  return s * 0.25;
}

vec3 light(vec3 n, vec3 v, vec3 l, vec3 col, float rough) {
  float nl = max(dot(n, l), 0.0);
  vec3 h = normalize(l + v);
  float shin = mix(64.0, 6.0, rough);
  float spec = pow(max(dot(n, h), 0.0), shin) * (1.0 - rough) * 0.6;
  return col * (nl + spec * nl);
}

void main() {
  vec2 uv = vUV;
  vec3 nGeom = normalize(vN);
  if (uWorldUV.x > 0.0) {
    // top faces use XZ, walls use the dominant side axis
    vec3 an = abs(nGeom);
    uv = an.y > 0.5 ? vPos.xz : (an.x > an.z ? vPos.zy : vPos.xy);
    uv = uv / uWorldUV.x * vec2(1.0, -1.0);
  }
  vec4 alb = texture(uAlbedo, uv);
  if (alb.a < uMat.y) discard;
  vec3 base = pow(alb.rgb, vec3(2.2)) * uTint.rgb;
  vec3 n = nGeom;
  vec3 tn = texture(uNormal, uv).xyz * 2.0 - 1.0;
  vec3 t = normalize(vT.xyz - nGeom * dot(vT.xyz, nGeom));
  vec3 b = cross(nGeom, t) * (vT.w < 0.0 ? -1.0 : 1.0);
  if (uWorldUV.x > 0.0) {
    vec3 an = abs(nGeom);
    t = an.y > 0.5 ? vec3(1.0, 0.0, 0.0) : (an.x > an.z ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0));
    b = cross(nGeom, t);
  }
  n = normalize(t * tn.x + b * tn.y + nGeom * max(tn.z, 0.2));
  vec3 v = normalize(uCamPos - vPos);
  float rough = uMat.x;

  // hemispheric ambient
  vec3 col = base * mix(uAmbGround, uAmbSky, n.y * 0.5 + 0.5);
  // moonlight
  col += base * light(n, v, uSunDir, uSunColor, rough) * shadow();
  // flashlight (spot from the camera)
  vec3 fl = uFlashPos - vPos;
  float fd = length(fl);
  fl /= fd;
  float cone = smoothstep(0.72, 0.96, dot(-fl, uFlashDir));
  float fatt = cone / (1.0 + fd * fd * 0.035);
  col += base * light(n, v, fl, uFlashColor * fatt, rough);
  // street lamps / flare
  for (int i = 0; i < 4; i++) {
    vec4 lp = uLights[i * 2];
    vec3 lc = uLights[i * 2 + 1].rgb;
    vec3 d = lp.xyz - vPos;
    float dist = length(d);
    float att = clamp(1.0 - dist / lp.w, 0.0, 1.0);
    col += base * light(n, v, d / dist, lc * att * att, rough);
  }
  col += base * uTint.a * 4.0;
  col = mix(col, vec3(1.0, 0.85, 0.75), uMat.w * 0.35);
  // fog
  float dist = length(uCamPos - vPos);
  float f = 1.0 - exp(-pow(dist * uFog.w, 2.0));
  col = mix(col, uFog.rgb, f);
  // ACES-ish tone map + gamma
  col = clamp((col * (2.51 * col + 0.03)) / (col * (2.43 * col + 0.59) + 0.14), 0.0, 1.0);
  frag = vec4(pow(col, vec3(1.0 / 2.2)), 1.0);
}"#;

const SHADOW_FS: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D uAlbedo;
uniform vec4 uMat;
in vec2 vUV;
out vec4 frag;
void main() {
  if (uMat.y > 0.0 && texture(uAlbedo, vUV).a < uMat.y) discard;
  frag = vec4(1.0);
}"#;

const SKY_VS: &str = r#"#version 300 es
uniform mat4 uInvVP;
out vec3 vDir;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
  vec4 w = uInvVP * vec4(p, 1.0, 1.0);
  vDir = w.xyz / w.w;
  gl_Position = vec4(p, 1.0, 1.0);
}"#;

const SKY_FS: &str = r#"#version 300 es
precision highp float;
uniform sampler2D uSky;
uniform vec3 uCamPos;
uniform vec4 uFog;
in vec3 vDir;
out vec4 frag;
void main() {
  vec3 d = normalize(vDir - uCamPos);
  vec2 uv = vec2(atan(d.x, -d.z) / 6.2831853 + 0.5, acos(clamp(d.y, -1.0, 1.0)) / 3.1415926);
  vec3 c = texture(uSky, uv).rgb;
  // fade the horizon into the fog colour
  float h = smoothstep(-0.02, 0.35, d.y);
  frag = vec4(mix(uFog.rgb * 1.2, c, h), 1.0);
}"#;

const SPRITE_VS: &str = r#"#version 300 es
layout(location=0) in vec3 aPos;
layout(location=1) in vec2 aUV;
layout(location=2) in vec4 aCol;
uniform mat4 uVP;
out vec2 vUV; out vec4 vCol; out vec3 vPos;
void main() { vUV = aUV; vCol = aCol; vPos = aPos; gl_Position = uVP * vec4(aPos, 1.0); }"#;

const SPRITE_FS: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D uTex;
uniform vec4 uFog;
uniform vec3 uCamPos;
uniform float uWorld;
in vec2 vUV; in vec4 vCol; in vec3 vPos;
out vec4 frag;
void main() {
  vec4 t = texture(uTex, vUV);
  vec4 c = vec4(t.rgb * vCol.rgb, t.a * vCol.a);
  if (uWorld > 0.5) {
    float f = 1.0 - exp(-pow(length(uCamPos - vPos) * uFog.w, 2.0));
    c.rgb *= 1.0 - f;
  }
  frag = c;
}"#;

struct Program {
    id: GLuint,
    u: HashMap<&'static str, GLint>,
}

impl Program {
    fn new(vs: &str, fs: &str, defines: &str, uniforms: &[&'static str]) -> Program {
        let inject = |src: &str| src.replacen("#version 300 es", &format!("#version 300 es\n{defines}"), 1);
        unsafe {
            let v = compile(VERTEX_SHADER, &inject(vs));
            let f = compile(FRAGMENT_SHADER, &inject(fs));
            let id = glCreateProgram();
            glAttachShader(id, v);
            glAttachShader(id, f);
            glLinkProgram(id);
            let mut ok = 0;
            glGetProgramiv(id, LINK_STATUS, &mut ok);
            if ok == 0 {
                let mut buf = [0u8; 2048];
                let mut len = 0;
                glGetProgramInfoLog(id, 2048, &mut len, buf.as_mut_ptr() as *mut _);
                crate::log(&format!("link failed: {}", String::from_utf8_lossy(&buf[..len.max(0) as usize])));
            }
            glDeleteShader(v);
            glDeleteShader(f);
            let mut u = HashMap::new();
            for name in uniforms {
                let c = CString::new(*name).unwrap();
                u.insert(*name, glGetUniformLocation(id, c.as_ptr()));
            }
            Program { id, u }
        }
    }
    fn loc(&self, n: &str) -> GLint {
        *self.u.get(n).unwrap_or(&-1)
    }
}

unsafe fn compile(kind: GLenum, src: &str) -> GLuint {
    let s = glCreateShader(kind);
    let c = CString::new(src).unwrap();
    let p = c.as_ptr();
    glShaderSource(s, 1, &p, ptr::null());
    glCompileShader(s);
    let mut ok = 0;
    glGetShaderiv(s, COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut buf = [0u8; 2048];
        let mut len = 0;
        glGetShaderInfoLog(s, 2048, &mut len, buf.as_mut_ptr() as *mut _);
        crate::log(&format!("shader failed: {}", String::from_utf8_lossy(&buf[..len.max(0) as usize])));
    }
    s
}

const LIT_UNIFORMS: &[&str] = &[
    "uVP", "uModel", "uShadowVP", "uBones", "uAlbedo", "uNormal", "uShadow", "uCamPos", "uSunDir", "uSunColor", "uAmbSky", "uAmbGround",
    "uFlashPos", "uFlashDir", "uFlashColor", "uLights", "uFog", "uTint", "uMat", "uWorldUV",
];

#[derive(Clone, Copy)]
struct Mat {
    albedo: GLuint,
    normal: GLuint,
    flags: u32,
}

struct GpuMesh {
    vbo: GLuint,
    ibo: GLuint,
    wide: bool,
    subs: Vec<(u32, u32, Mat)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SVert {
    p: [f32; 3],
    uv: [f32; 2],
    c: [u8; 4],
}

pub struct Renderer {
    lit: Program,
    lit_skin: Program,
    shadow: Program,
    shadow_skin: Program,
    sky: Program,
    sprite: Program,
    pub models: Vec<Model>,
    meshes: Vec<GpuMesh>,
    model_index: HashMap<String, usize>,
    level_meshes: Vec<(GpuMesh, f32)>,
    sky_tex: GLuint,
    vfx_tex: GLuint,
    shadow_fbo: GLuint,
    shadow_tex: GLuint,
    shadow_size: i32,
    pub shadows: bool,
    svbo: GLuint,
    sibo: GLuint,
    verts: Vec<SVert>,
    pose: Pose,
    scratch: Pose,
    globals: Vec<M4>,
    rows: Vec<[f32; 4]>,
    pub width: i32,
    pub height: i32,
    pub minimap: [f32; 3],
    pub map_expanded: bool,
    pub zombie_models: Vec<usize>,
    arms_model: usize,
    head_bone: usize,
    time: f32,
    mats_cache: Option<HashMap<String, Mat>>,
    /// clenched-fist finger rotations (bone index, local rotation) for the view model
    fist: Vec<(usize, Quat)>,
}

const MAX_QUADS: usize = 1500;

impl Renderer {
    pub fn new(src: &dyn AssetSource) -> Result<Renderer, String> {
        let defines_skin = "#define SKINNED 1";
        let lit = Program::new(LIT_VS, LIT_FS, "", LIT_UNIFORMS);
        let lit_skin = Program::new(LIT_VS, LIT_FS, defines_skin, LIT_UNIFORMS);
        let shadow = Program::new(LIT_VS, SHADOW_FS, "", LIT_UNIFORMS);
        let shadow_skin = Program::new(LIT_VS, SHADOW_FS, defines_skin, LIT_UNIFORMS);
        let sky = Program::new(SKY_VS, SKY_FS, "", &["uInvVP", "uSky", "uCamPos", "uFog"]);
        let sprite = Program::new(SPRITE_VS, SPRITE_FS, "", &["uVP", "uTex", "uFog", "uCamPos", "uWorld"]);

        // textures
        let mut tex = Vec::with_capacity(TEXTURES.len());
        for name in TEXTURES {
            let bytes = src.read(name).ok_or_else(|| format!("missing asset {name}"))?;
            let repeat = name.starts_with("s_");
            tex.push(upload_pvr(&bytes, repeat).map_err(|e| format!("{name}: {e}"))?);
        }
        let flat = unsafe {
            let mut id = 0;
            glGenTextures(1, &mut id);
            glBindTexture(TEXTURE_2D, id);
            let px: [u8; 4] = [128, 128, 255, 255];
            glTexImage2D(TEXTURE_2D, 0, RGBA as i32, 1, 1, 0, RGBA, UNSIGNED_BYTE, px.as_ptr() as *const _);
            glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR);
            glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR);
            id
        };
        let mut mats: HashMap<&str, Mat> = HashMap::new();
        for &(name, a, n, flags) in MATERIALS {
            mats.insert(name, Mat { albedo: tex[a as usize], normal: if n >= 0 { tex[n as usize] } else { flat }, flags });
        }
        let fallback = Mat { albedo: tex[0], normal: flat, flags: 0 };
        let sky_tex = tex[TEXTURES.iter().position(|t| *t == SKY_TEXTURE).unwrap()];
        let vfx_tex = tex[TEXTURES.iter().position(|t| *t == VFX_TEXTURE).unwrap()];

        // models
        let mut models = Vec::new();
        let mut meshes = Vec::new();
        let mut model_index = HashMap::new();
        for name in MODELS {
            let bytes = src.read(name).ok_or_else(|| format!("missing asset {name}"))?;
            let m = Model::parse(&bytes)?;
            let subs = m.subs.iter().map(|s| (s.first, s.count, *mats.get(s.material.as_str()).unwrap_or(&fallback))).collect();
            meshes.push(upload_mesh(&m.vertices, &m.index_bytes, m.wide, subs));
            model_index.insert(name.to_string(), models.len());
            models.push(m);
        }
        let zombie_models: Vec<usize> = MODELS.iter().enumerate().filter(|(_, n)| n.starts_with("zombie_")).map(|(i, _)| i).collect();
        let arms_model = *model_index.get("player_arms.wxm").ok_or("no arms")?;
        let head_bone = models[arms_model].bone("Bip01 Head").unwrap_or(0);
        let nb = models[zombie_models[0]].bones.len();

        let (shadow_fbo, shadow_tex, shadow_size) = unsafe { make_shadow_target(1536) };
        let (svbo, sibo) = unsafe {
            let mut b = [0u32; 2];
            glGenBuffers(2, b.as_mut_ptr());
            glBindBuffer(ARRAY_BUFFER, b[0]);
            glBufferData(ARRAY_BUFFER, (MAX_QUADS * 4 * std::mem::size_of::<SVert>()) as isize, ptr::null(), DYNAMIC_DRAW);
            let mut idx: Vec<u16> = Vec::with_capacity(MAX_QUADS * 6);
            for i in 0..MAX_QUADS as u16 {
                let q = i * 4;
                idx.extend_from_slice(&[q, q + 1, q + 2, q, q + 2, q + 3]);
            }
            glBindBuffer(ELEMENT_ARRAY_BUFFER, b[1]);
            glBufferData(ELEMENT_ARRAY_BUFFER, (idx.len() * 2) as isize, idx.as_ptr() as *const _, STATIC_DRAW);
            (b[0], b[1])
        };
        let _ = mats.len();
        Ok(Renderer {
            lit,
            lit_skin,
            shadow,
            shadow_skin,
            sky,
            sprite,
            models,
            meshes,
            model_index,
            level_meshes: Vec::new(),
            sky_tex,
            vfx_tex,
            shadow_fbo,
            shadow_tex,
            shadow_size,
            shadows: true,
            svbo,
            sibo,
            verts: Vec::with_capacity(MAX_QUADS * 4),
            pose: Pose::new(nb),
            scratch: Pose::new(nb),
            globals: Vec::with_capacity(nb),
            rows: Vec::with_capacity(nb * 3),
            width: 1,
            height: 1,
            minimap: [0.0; 3],
            map_expanded: false,
            zombie_models,
            arms_model,
            head_bone,
            time: 0.0,
            mats_cache: None,
            fist: Vec::new(),
        })
        .map(|mut r| {
            r.mats_cache = Some(mats.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
            r.fist = fist_pose(&r.models[r.arms_model]);
            r
        })
    }

    pub fn bounds(&self, name: &str) -> (V3, V3) {
        match self.model_index.get(name) {
            Some(&i) => (self.models[i].bounds_min, self.models[i].bounds_max),
            None => crate::level::approx_bounds(name),
        }
    }

    pub fn model(&self, name: &str) -> Option<usize> {
        self.model_index.get(name).copied()
    }

    /// Generated level geometry (ground slabs, walls) grouped per surface material.
    pub fn build_level(&mut self, blocks: &[Block]) {
        for (m, _) in self.level_meshes.drain(..) {
            unsafe {
                glDeleteBuffers(1, &m.vbo);
                glDeleteBuffers(1, &m.ibo);
            }
        }
        let mats = self.mats_cache.as_ref().unwrap();
        let mut by: HashMap<&str, (Vec<u8>, Vec<u16>, f32)> = HashMap::new();
        for b in blocks.iter().filter(|b| !b.surface.is_empty()) {
            let e = by.entry(b.surface).or_insert_with(|| (Vec::new(), Vec::new(), b.tile));
            push_box(&mut e.0, &mut e.1, b);
        }
        for (surface, (v, i, tile)) in by {
            if let Some(m) = mats.get(surface) {
                self.level_meshes.push((upload_mesh(&v, bytemuck_u16(&i), false, vec![(0, i.len() as u32, *m)]), tile));
            }
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.width = w.max(1);
        self.height = h.max(1);
    }

    // ---------------------------------------------------------------- frame
    pub fn render(&mut self, game: &Game, dt: f32) {
        self.time += dt;
        let p = &game.player;
        let shake = game.shake * game.shake * 0.04;
        let st = self.time * 37.0;
        let eye = p.eye() + v3((st * 1.3).sin() * shake, (st * 1.7).cos() * shake, 0.0);
        let yaw = p.yaw + p.kick.x + (st * 0.9).sin() * shake * 0.5;
        let pitch = p.pitch + p.kick.y;
        let fwd = v3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
        let view = M4::look_at(eye, eye + fwd, v3(0.0, 1.0, 0.0));
        let aspect = self.width as f32 / self.height as f32;
        let proj = M4::perspective(1.15, aspect, 0.05, 140.0);
        let vp = proj.mul(&view);

        let sun = v3(-0.35, 0.82, 0.45).norm();
        let center = v3(p.pos.x + fwd.x * 12.0, 0.0, p.pos.y + fwd.z * 12.0);
        let light_view = M4::look_at(center + sun * 40.0, center, v3(0.0, 0.0, 1.0));
        let light_proj = M4::ortho_depth(-28.0, 28.0, -28.0, 28.0, 1.0, 90.0);
        let shadow_vp = light_proj.mul(&light_view);

        // gather lights: 3 nearest lamps + extraction flare
        let mut lamps: Vec<(f32, [f32; 8])> = game
            .level
            .lights
            .iter()
            .map(|l| {
                let d = (l.pos.xz() - p.pos).len();
                let fl = if l.flicker > 0.0 { 0.75 + 0.25 * ((self.time * (7.0 + l.flicker * 9.0)).sin() * (self.time * 13.0 + l.flicker).sin()).abs() } else { 1.0 };
                (d, [l.pos.x, l.pos.y, l.pos.z, l.radius, l.color.x * 2.2 * fl, l.color.y * 2.2 * fl, l.color.z * 2.2 * fl, 0.0])
            })
            .collect();
        lamps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut lights = [0.0f32; 32];
        for (i, l) in lamps.iter().take(4).enumerate() {
            lights[i * 8..i * 8 + 8].copy_from_slice(&l.1);
        }
        if game.extraction_active {
            let e = game.level.extraction;
            let pulse = 0.6 + 0.4 * (self.time * 6.0).sin().abs();
            lights[24..32].copy_from_slice(&[e.x, 1.0, e.y, 14.0, 3.5 * pulse, 0.5 * pulse, 0.3 * pulse, 0.0]);
        }

        let fog = [0.045f32, 0.05, 0.065, 0.038];
        unsafe {
            // ---- shadow map
            if self.shadows {
                glBindFramebuffer(FRAMEBUFFER, self.shadow_fbo);
                glViewport(0, 0, self.shadow_size, self.shadow_size);
                glClear(DEPTH_BUFFER_BIT);
                glEnable(DEPTH_TEST);
                glDepthFunc(LEQUAL);
                glDepthMask(TRUE);
                glDisable(BLEND);
                glEnable(POLYGON_OFFSET_FILL);
                glPolygonOffset(1.5, 3.0);
                self.draw_scene(game, &shadow_vp, &shadow_vp, eye, None, true);
                glDisable(POLYGON_OFFSET_FILL);
                glBindFramebuffer(FRAMEBUFFER, 0);
            }

            // ---- main pass
            glViewport(0, 0, self.width, self.height);
            glClearColor(fog[0], fog[1], fog[2], 1.0);
            glClear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
            glEnable(DEPTH_TEST);
            glDepthFunc(LEQUAL);
            glDepthMask(TRUE);
            glDisable(BLEND);
            glEnable(CULL_FACE);
            glCullFace(BACK);
            glFrontFace(CCW);
            for prog in [&self.lit, &self.lit_skin] {
                glUseProgram(prog.id);
                glUniform3f(prog.loc("uCamPos"), eye.x, eye.y, eye.z);
                glUniform3f(prog.loc("uSunDir"), sun.x, sun.y, sun.z);
                glUniform3f(prog.loc("uSunColor"), 0.20, 0.24, 0.34);
                glUniform3f(prog.loc("uAmbSky"), 0.050, 0.058, 0.080);
                glUniform3f(prog.loc("uAmbGround"), 0.020, 0.020, 0.024);
                glUniform3f(prog.loc("uFlashPos"), eye.x, eye.y - 0.1, eye.z);
                glUniform3f(prog.loc("uFlashDir"), fwd.x, fwd.y, fwd.z);
                glUniform3f(prog.loc("uFlashColor"), 1.7, 1.58, 1.4);
                glUniform4fv(prog.loc("uLights"), 8, lights.as_ptr());
                glUniform4f(prog.loc("uFog"), fog[0], fog[1], fog[2], fog[3]);
                glUniform1i(prog.loc("uAlbedo"), 0);
                glUniform1i(prog.loc("uNormal"), 1);
                glUniform1i(prog.loc("uShadow"), 2);
            }
            glActiveTexture(TEXTURE0 + 2);
            glBindTexture(TEXTURE_2D, self.shadow_tex);
            glActiveTexture(TEXTURE0);
            self.draw_scene(game, &vp, &shadow_vp, eye, Some(fwd), false);

            // ---- sky
            glDisable(CULL_FACE);
            glUseProgram(self.sky.id);
            let inv = invert(&vp);
            glUniformMatrix4fv(self.sky.loc("uInvVP"), 1, FALSE, inv.0.as_ptr());
            glUniform3f(self.sky.loc("uCamPos"), eye.x, eye.y, eye.z);
            glUniform4f(self.sky.loc("uFog"), fog[0], fog[1], fog[2], fog[3]);
            glUniform1i(self.sky.loc("uSky"), 0);
            glBindTexture(TEXTURE_2D, self.sky_tex);
            for i in 0..6 {
                glDisableVertexAttribArray(i);
            }
            glDepthMask(FALSE);
            glDrawArrays(TRIANGLES, 0, 3);

            // ---- effects
            glEnable(BLEND);
            glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
            self.draw_fx(game, &vp, eye, fwd);

            // ---- first-person arms
            glDepthMask(TRUE);
            glClear(DEPTH_BUFFER_BIT);
            glDisable(BLEND);
            glEnable(CULL_FACE);
            let vproj = M4::perspective(1.05, aspect, 0.02, 10.0);
            self.draw_arms(game, &vproj.mul(&view), eye, yaw, pitch, &shadow_vp);
            glDisable(CULL_FACE);

            // ---- 2D overlay
            glDisable(DEPTH_TEST);
            glEnable(BLEND);
            glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
            self.draw_overlay(game, yaw);
            glDepthMask(TRUE);
        }
    }

    unsafe fn draw_scene(&mut self, game: &Game, vp: &M4, shadow_vp: &M4, eye: V3, fwd: Option<V3>, depth_only: bool) {
        let (stat, skin) = if depth_only { (&self.shadow, &self.shadow_skin) } else { (&self.lit, &self.lit_skin) };
        for prog in [stat, skin] {
            glUseProgram(prog.id);
            glUniformMatrix4fv(prog.loc("uVP"), 1, FALSE, vp.0.as_ptr());
            glUniformMatrix4fv(prog.loc("uShadowVP"), 1, FALSE, shadow_vp.0.as_ptr());
            glUniform4f(prog.loc("uMat"), 0.8, 0.0, if self.shadows { 1.0 } else { 0.0 }, 0.0);
            glUniform4f(prog.loc("uTint"), 1.0, 1.0, 1.0, 0.0);
            glUniform2f(prog.loc("uWorldUV"), 0.0, 0.0);
        }
        let visible = |pos: V3, radius: f32| -> bool {
            let d = pos - eye;
            let dist = d.len();
            if dist > 60.0 + radius {
                return false;
            }
            match fwd {
                Some(f) => dist < radius + 2.0 || d.dot(f) > -radius,
                None => (pos.xz() - game.player.pos).len() < 40.0 + radius,
            }
        };
        // generated level surfaces
        glUseProgram(stat.id);
        let ident = M4::identity();
        glUniformMatrix4fv(stat.loc("uModel"), 1, FALSE, ident.0.as_ptr());
        for (m, tile) in &self.level_meshes {
            glUniform2f(stat.loc("uWorldUV"), *tile, 0.0);
            glUniform4f(stat.loc("uMat"), 0.75, 0.0, if self.shadows { 1.0 } else { 0.0 }, 0.0);
            draw_mesh(m, depth_only, stat);
        }
        glUniform2f(stat.loc("uWorldUV"), 0.0, 0.0);
        // props
        for pr in &game.level.props {
            let Some(&mi) = self.model_index.get(pr.model) else { continue };
            let m = &self.models[mi];
            let r = (m.bounds_max - m.bounds_min).len() * 0.5;
            let c = pr.pos + (m.bounds_max + m.bounds_min) * 0.5;
            if !visible(c, r) {
                continue;
            }
            let model = M4::from_tr(pr.pos, Quat::from_axis_angle(v3(0.0, 1.0, 0.0), pr.yaw));
            glUniformMatrix4fv(stat.loc("uModel"), 1, FALSE, model.0.as_ptr());
            for &(first, count, mat) in &self.meshes[mi].subs {
                let emissive = if mat.flags & MAT_EMISSIVE != 0 { 1.0 } else { 0.0 };
                glUniform4f(stat.loc("uTint"), 1.0, 1.0, 1.0, emissive);
                draw_sub(&self.meshes[mi], first, count, mat, depth_only);
            }
        }
        glUniform4f(stat.loc("uTint"), 1.0, 1.0, 1.0, 0.0);
        // medkit pickups (bob + spin, soft glow)
        if let Some(mi) = self.model(M_MEDKIT) {
            for k in &game.pickups {
                let y = 0.35 + (k.t * 2.5).sin() * 0.06;
                let model = M4::from_tr(v3(k.pos.x, y, k.pos.y), Quat::from_axis_angle(v3(0.0, 1.0, 0.0), k.t * 1.4)).mul(&M4::scale(v3(1.3, 1.3, 1.3)));
                glUniformMatrix4fv(stat.loc("uModel"), 1, FALSE, model.0.as_ptr());
                glUniform4f(stat.loc("uTint"), 1.0, 1.0, 1.0, 0.12 + 0.08 * (k.t * 4.0).sin());
                for &(first, count, mat) in &self.meshes[mi].subs {
                    draw_sub(&self.meshes[mi], first, count, mat, depth_only);
                }
            }
            glUniform4f(stat.loc("uTint"), 1.0, 1.0, 1.0, 0.0);
        }
        // zombies
        glUseProgram(skin.id);
        for z in &game.zombies {
            let mi = self.zombie_models[z.variant % self.zombie_models.len()];
            let pos = v3(z.pos.x, game.level.ground_height(z.pos) - z.sink, z.pos.y);
            if !visible(pos + v3(0.0, 0.9, 0.0), 1.2) {
                continue;
            }
            let model = &self.models[mi];
            z.anim.evaluate(model, &mut self.pose, &mut self.scratch);
            self.pose.globals(model, &mut self.globals);
            skin_rows(model, &self.globals, &mut self.rows);
            glUniform4fv(skin.loc("uBones"), self.rows.len() as i32, self.rows.as_ptr() as *const f32);
            let m = M4::from_tr(pos, Quat::from_axis_angle(v3(0.0, 1.0, 0.0), z.yaw));
            glUniformMatrix4fv(skin.loc("uModel"), 1, FALSE, m.0.as_ptr());
            let dead_dark = if let ZState::Dead { t } = z.state { 1.0 - clamp(t / 6.0, 0.0, 0.25) } else { 1.0 };
            glUniform4f(skin.loc("uTint"), dead_dark, dead_dark, dead_dark, 0.0);
            glUniform4f(skin.loc("uMat"), 0.65, 0.0, if self.shadows { 1.0 } else { 0.0 }, z.flash);
            for &(first, count, mat) in &self.meshes[mi].subs {
                let alpha = mat.flags & MAT_ALPHA != 0;
                if alpha {
                    glUniform4f(skin.loc("uMat"), 0.7, 0.5, if self.shadows { 1.0 } else { 0.0 }, z.flash);
                }
                draw_sub(&self.meshes[mi], first, count, mat, depth_only);
                if alpha {
                    glUniform4f(skin.loc("uMat"), 0.65, 0.0, if self.shadows { 1.0 } else { 0.0 }, z.flash);
                }
            }
        }
    }

    unsafe fn draw_arms(&mut self, game: &Game, vp: &M4, eye: V3, yaw: f32, pitch: f32, shadow_vp: &M4) {
        let p = &game.player;
        if p.dead() {
            return;
        }
        let mi = self.arms_model;
        let model = &self.models[mi];
        p.arms.evaluate(model, &mut self.pose, &mut self.scratch);
        // keep both hands clenched: boxing clips only close the punching hand
        for &(b, q) in &self.fist {
            self.pose.rot[b] = q;
        }
        self.pose.globals(model, &mut self.globals);
        let head = self.globals[self.head_bone].translation();
        skin_rows(model, &self.globals, &mut self.rows);
        let sway = v3((p.bob * 1.0).sin() * 0.012, (p.bob * 2.0).sin().abs() * -0.012, 0.0) * clamp(p.vel.len() / 4.0, 0.0, 1.0);
        let block = p.block_k;
        let cam = M4::from_tr(eye, Quat::from_axis_angle(v3(0.0, 1.0, 0.0), yaw).mul(Quat::from_axis_angle(v3(1.0, 0.0, 0.0), -pitch)));
        // model faces +Z like the camera; place the skeleton's head slightly behind the eye
        let offset = v3(sway.x, -0.085 + sway.y + block * 0.05, 0.25 - block * 0.03);
        let local = M4::from_tr(offset, Quat::from_axis_angle(v3(1.0, 0.0, 0.0), -0.12 - block * 0.25)).mul(&M4::from_tr(-head, Quat::IDENTITY));
        let world = cam.mul(&local);
        let prog = &self.lit_skin;
        glUseProgram(prog.id);
        glUniformMatrix4fv(prog.loc("uVP"), 1, FALSE, vp.0.as_ptr());
        glUniformMatrix4fv(prog.loc("uShadowVP"), 1, FALSE, shadow_vp.0.as_ptr());
        glUniformMatrix4fv(prog.loc("uModel"), 1, FALSE, world.0.as_ptr());
        glUniform4fv(prog.loc("uBones"), self.rows.len() as i32, self.rows.as_ptr() as *const f32);
        glUniform4f(prog.loc("uTint"), 1.0, 1.0, 1.0, 0.0);
        glUniform4f(prog.loc("uMat"), 0.7, 0.5, 0.0, 0.0);
        glUniform2f(prog.loc("uWorldUV"), 0.0, 0.0);
        // the flashlight sits on the chest, so arms catch light from below
        glUniform3f(prog.loc("uFlashPos"), eye.x, eye.y - 0.25, eye.z);
        let _ = game;
        for &(first, count, mat) in &self.meshes[mi].subs {
            draw_sub(&self.meshes[mi], first, count, mat, false);
        }
        glUniform3f(prog.loc("uFlashPos"), eye.x, eye.y - 0.1, eye.z);
    }

    unsafe fn draw_fx(&mut self, game: &Game, vp: &M4, eye: V3, fwd: V3) {
        self.verts.clear();
        let right = v3(0.0, 1.0, 0.0).cross(fwd).norm() * -1.0;
        let up = right.cross(fwd).norm() * -1.0;
        let up = if up.y < 0.0 { up * -1.0 } else { up };
        // ground decals first, then billboards far -> near
        for f in game.fx.iter().filter(|f| f.ground) {
            let k = clamp(f.t / 1.5, 0.0, 1.0);
            let fi = f.anim.0 + ((k * (f.anim.1 - 1) as f32) as usize).min(f.anim.1 - 1);
            let fade = clamp((f.dur - f.t) / 4.0, 0.0, 1.0) * f.alpha;
            let s = f.size * (0.4 + 0.6 * k);
            push_ground(&mut self.verts, fi, v3(f.pos.x, f.pos.y + 0.02, f.pos.z), s, f.rot, [f.color[0] * fade, f.color[1] * fade, f.color[2] * fade, fade]);
        }
        let mut order: Vec<(f32, usize)> = game.fx.iter().enumerate().filter(|(_, f)| !f.ground).map(|(i, f)| ((f.pos - eye).dot(fwd), i)).collect();
        order.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for (_, i) in order {
            let f = &game.fx[i];
            let k = clamp(f.t / f.dur, 0.0, 0.999);
            let fi = f.anim.0 + (k * f.anim.1 as f32) as usize;
            let a = f.alpha * (1.0 - clamp((k - 0.6) / 0.4, 0.0, 1.0));
            let col = [f.color[0] * a, f.color[1] * a, f.color[2] * a, if f.additive { 0.0 } else { a }];
            push_billboard(&mut self.verts, fi, f.pos, f.size, f.rot, right, up, col);
        }
        // extraction beacon ring
        if game.extraction_active {
            let e = game.level.extraction;
            let pulse = 0.6 + 0.4 * (self.time * 4.0).sin();
            push_ground(&mut self.verts, G_RING.0, v3(e.x, 0.05, e.y), 6.0, self.time, [1.0 * pulse, 0.35 * pulse, 0.2 * pulse, 0.0]);
        }
        if self.verts.is_empty() {
            return;
        }
        glDepthMask(FALSE);
        let prog = &self.sprite;
        glUseProgram(prog.id);
        glUniformMatrix4fv(prog.loc("uVP"), 1, FALSE, vp.0.as_ptr());
        glUniform1i(prog.loc("uTex"), 0);
        glUniform1f(prog.loc("uWorld"), 1.0);
        glUniform3f(prog.loc("uCamPos"), eye.x, eye.y, eye.z);
        glUniform4f(prog.loc("uFog"), 0.0, 0.0, 0.0, 0.038);
        glBindTexture(TEXTURE_2D, self.vfx_tex);
        self.flush_sprites();
    }

    unsafe fn flush_sprites(&mut self) {
        let n = self.upload_sprites();
        if n > 0 {
            glDrawElements(TRIANGLES, (n * 6) as i32, UNSIGNED_SHORT, ptr::null());
        }
    }

    unsafe fn upload_sprites(&mut self) -> usize {
        let n = (self.verts.len() / 4).min(MAX_QUADS);
        if n == 0 {
            return 0;
        }
        glBindBuffer(ARRAY_BUFFER, self.svbo);
        glBufferSubData(ARRAY_BUFFER, 0, (n * 4 * std::mem::size_of::<SVert>()) as isize, self.verts.as_ptr() as *const _);
        glBindBuffer(ELEMENT_ARRAY_BUFFER, self.sibo);
        let stride = std::mem::size_of::<SVert>() as i32;
        for i in 3..6 {
            glDisableVertexAttribArray(i);
        }
        glEnableVertexAttribArray(0);
        glEnableVertexAttribArray(1);
        glEnableVertexAttribArray(2);
        glVertexAttribPointer(0, 3, FLOAT, FALSE, stride, ptr::null());
        glVertexAttribPointer(1, 2, FLOAT, FALSE, stride, 12 as *const _);
        glVertexAttribPointer(2, 4, UNSIGNED_BYTE, TRUE, stride, 20 as *const _);
        n
    }

    unsafe fn draw_overlay(&mut self, game: &Game, yaw: f32) {
        let (w, h) = (self.width as f32, self.height as f32);
        self.verts.clear();
        let p = &game.player;
        // vignette, red when hurt / low health
        let low = clamp((0.35 - p.hp / p.hp_max) / 0.35, 0.0, 1.0);
        let pulse = 0.7 + 0.3 * (self.time * 5.0).sin();
        let red = clamp(p.hurt_flash * 0.8 + low * 0.5 * pulse, 0.0, 1.0);
        let a = 0.55 + red * 0.3;
        vignette(&mut self.verts, w, h, [a * red * 0.75, 0.0, 0.0, a]);
        // crosshair
        let c = (w * 0.5, h * 0.5);
        let s = (h / 140.0).max(3.0);
        sprite2d(&mut self.verts, G_DISC.0, c.0, c.1, s * 1.6, s * 1.6, 0.0, [0.9, 0.95, 0.85, 0.55]);
        let fixed = self.verts.len() / 4;

        // minimap (top-right), rotates with the view
        let [mx, my, ms] = self.minimap;
        let (cx, cy, size, range, rot, center) = if self.map_expanded {
            let s = h.min(w) * 0.84;
            (w * 0.5 - s * 0.5, h * 0.5 - s * 0.5, s, 60.0, 0.0f32, V2 { x: 0.0, y: 0.0 })
        } else {
            (mx, my, ms, 22.0, yaw, p.pos)
        };
        if size > 4.0 {
            let ppm = size / (range * 2.0);
            let (sn, cs) = rot.sin_cos();
            // world -> map: forward (+yaw dir) points up on the map
            let to_map = |q: V2| -> (f32, f32) {
                let d = q - center;
                let rx = d.x * cs - d.y * sn;
                let rz = d.x * sn + d.y * cs;
                (cx + size * 0.5 - rx * ppm, cy + size * 0.5 - rz * ppm)
            };
            rect(&mut self.verts, cx - 3.0, cy - 3.0, size + 6.0, size + 6.0, [0.45, 0.55, 0.38, 0.9]);
            rect(&mut self.verts, cx, cy, size, size, [0.05, 0.07, 0.06, 0.88]);
            let fixed2 = self.verts.len() / 4;
            // roads + walls as rotated quads
            let quad = |v: &mut Vec<SVert>, a: V2, b: V2, col: [f32; 4]| {
                let pts = [to_map(a), to_map(V2 { x: b.x, y: a.y }), to_map(b), to_map(V2 { x: a.x, y: b.y })];
                let uv = white_uv();
                let al = col[3];
                let cc = color_u8([col[0] * al, col[1] * al, col[2] * al, al]);
                for (x, y) in pts {
                    v.push(SVert { p: [x, y, 0.0], uv, c: cc });
                }
            };
            quad(&mut self.verts, V2 { x: -6.0, y: MIN_Z }, V2 { x: 6.0, y: MAX_Z }, [0.24, 0.25, 0.26, 1.0]);
            quad(&mut self.verts, V2 { x: MIN_X, y: -5.0 }, V2 { x: MAX_X, y: 5.0 }, [0.24, 0.25, 0.26, 1.0]);
            for a in &game.level.colliders {
                quad(&mut self.verts, a.min, a.max, [0.62, 0.62, 0.58, 1.0]);
            }
            let dot = (size / 55.0).max(3.5);
            for k in &game.pickups {
                let (x, y) = to_map(k.pos);
                sprite2d(&mut self.verts, G_DISC.0, x, y, dot * 1.8, dot * 1.8, 0.0, [0.3, 1.0, 0.45, 1.0]);
            }
            for z in game.zombies.iter().filter(|z| z.alive()) {
                let (x, y) = to_map(z.pos);
                sprite2d(&mut self.verts, G_DISC.0, x, y, dot * 1.8, dot * 1.8, 0.0, [0.95, 0.15, 0.1, 1.0]);
            }
            if game.extraction_active {
                let (x, y) = to_map(game.level.extraction);
                let pr = dot * (2.4 + 0.5 * (self.time * 5.0).sin());
                sprite2d(&mut self.verts, G_RING.0, x, y, pr * 3.0, pr * 3.0, 0.0, [1.0, 0.8, 0.2, 1.0]);
            }
            let (px, py) = to_map(p.pos);
            let arrow_rot = if self.map_expanded { std::f32::consts::PI - yaw } else { 0.0 };
            sprite2d(&mut self.verts, G_ARROW.0, px, py, dot * 3.4, dot * 3.4, arrow_rot, [0.6, 1.0, 0.45, 1.0]);
            let n = (self.verts.len() / 4).min(MAX_QUADS);
            let proj = M4::ortho(0.0, w, h, 0.0);
            let prog = &self.sprite;
            glUseProgram(prog.id);
            glUniformMatrix4fv(prog.loc("uVP"), 1, FALSE, proj.0.as_ptr());
            glUniform1f(prog.loc("uWorld"), 0.0);
            glBindTexture(TEXTURE_2D, self.vfx_tex);
            self.upload_sprites();
            glDrawElements(TRIANGLES, (fixed2 * 6) as i32, UNSIGNED_SHORT, ptr::null());
            // map contents clipped to the square
            glEnable(SCISSOR_TEST);
            glScissor(cx as i32, (h - cy - size) as i32, size as i32, size as i32);
            glDrawElements(TRIANGLES, ((n - fixed2) * 6) as i32, UNSIGNED_SHORT, (fixed2 * 12) as *const _);
            glDisable(SCISSOR_TEST);
            let _ = fixed;
            return;
        }
        let proj = M4::ortho(0.0, w, h, 0.0);
        glUseProgram(self.sprite.id);
        glUniformMatrix4fv(self.sprite.loc("uVP"), 1, FALSE, proj.0.as_ptr());
        glUniform1f(self.sprite.loc("uWorld"), 0.0);
        glBindTexture(TEXTURE_2D, self.vfx_tex);
        self.flush_sprites();
    }
}

/// Finger rotations at the moment of impact of the jab (left) and cross (right).
fn fist_pose(m: &Model) -> Vec<(usize, Quat)> {
    let mut out = Vec::new();
    let mut pose = Pose::new(m.bones.len());
    for (clip, side) in [("jab", "Bip01 L Finger"), ("cross", "Bip01 R Finger")] {
        let Some(c) = m.clip(clip) else { continue };
        pose.sample(m, c, m.clips[c].duration() * 0.36);
        for (i, b) in m.bones.iter().enumerate() {
            if b.name.starts_with(side) {
                out.push((i, pose.rot[i]));
            }
        }
    }
    out
}

unsafe fn make_shadow_target(size: i32) -> (GLuint, GLuint, i32) {
    let mut tex = 0;
    glGenTextures(1, &mut tex);
    glBindTexture(TEXTURE_2D, tex);
    glTexImage2D(TEXTURE_2D, 0, DEPTH_COMPONENT24, size, size, 0, DEPTH_COMPONENT, UNSIGNED_INT, ptr::null());
    glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR);
    glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR);
    glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE);
    glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE);
    glTexParameteri(TEXTURE_2D, TEXTURE_COMPARE_MODE, COMPARE_REF_TO_TEXTURE);
    glTexParameteri(TEXTURE_2D, TEXTURE_COMPARE_FUNC, LEQUAL as i32);
    let mut fbo = 0;
    glGenFramebuffers(1, &mut fbo);
    glBindFramebuffer(FRAMEBUFFER, fbo);
    glFramebufferTexture2D(FRAMEBUFFER, DEPTH_ATTACHMENT, TEXTURE_2D, tex, 0);
    let none = NONE;
    glDrawBuffers(1, &none);
    glReadBuffer(NONE);
    if glCheckFramebufferStatus(FRAMEBUFFER) != FRAMEBUFFER_COMPLETE {
        crate::log("shadow framebuffer incomplete");
    }
    glBindFramebuffer(FRAMEBUFFER, 0);
    (fbo, tex, size)
}

fn bytemuck_u16(v: &[u16]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) }
}

fn upload_mesh(indices_vertices: &[u8], index_bytes: &[u8], wide: bool, subs: Vec<(u32, u32, Mat)>) -> GpuMesh {
    let vertices = indices_vertices;
    let mut b = [0u32; 2];
    unsafe {
        glGenBuffers(2, b.as_mut_ptr());
        glBindBuffer(ARRAY_BUFFER, b[0]);
        glBufferData(ARRAY_BUFFER, vertices.len() as isize, vertices.as_ptr() as *const _, STATIC_DRAW);
        glBindBuffer(ELEMENT_ARRAY_BUFFER, b[1]);
        glBufferData(ELEMENT_ARRAY_BUFFER, index_bytes.len() as isize, index_bytes.as_ptr() as *const _, STATIC_DRAW);
    }
    GpuMesh { vbo: b[0], ibo: b[1], wide, subs }
}

unsafe fn bind_mesh(m: &GpuMesh) {
    glBindBuffer(ARRAY_BUFFER, m.vbo);
    glBindBuffer(ELEMENT_ARRAY_BUFFER, m.ibo);
    let s = VERTEX_SIZE as i32;
    for i in 0..6 {
        glEnableVertexAttribArray(i);
    }
    glVertexAttribPointer(0, 3, FLOAT, FALSE, s, ptr::null());
    glVertexAttribPointer(1, 4, BYTE, TRUE, s, 12 as *const _);
    glVertexAttribPointer(2, 4, BYTE, TRUE, s, 16 as *const _);
    glVertexAttribPointer(3, 2, FLOAT, FALSE, s, 20 as *const _);
    glVertexAttribPointer(4, 4, UNSIGNED_BYTE, FALSE, s, 28 as *const _);
    glVertexAttribPointer(5, 4, UNSIGNED_BYTE, TRUE, s, 32 as *const _);
}

unsafe fn draw_sub(m: &GpuMesh, first: u32, count: u32, mat: Mat, depth_only: bool) {
    bind_mesh(m);
    glActiveTexture(TEXTURE0);
    glBindTexture(TEXTURE_2D, mat.albedo);
    if !depth_only {
        glActiveTexture(TEXTURE0 + 1);
        glBindTexture(TEXTURE_2D, mat.normal);
        glActiveTexture(TEXTURE0);
    }
    if m.wide {
        glDrawElements(TRIANGLES, count as i32, UNSIGNED_INT, (first as usize * 4) as *const _);
    } else {
        glDrawElements(TRIANGLES, count as i32, UNSIGNED_SHORT, (first as usize * 2) as *const _);
    }
}

unsafe fn draw_mesh(m: &GpuMesh, depth_only: bool, _p: &Program) {
    for &(first, count, mat) in &m.subs {
        draw_sub(m, first, count, mat, depth_only);
    }
}

/// Box geometry (top + four sides) in the 36-byte vertex format.
fn push_box(v: &mut Vec<u8>, idx: &mut Vec<u16>, b: &Block) {
    let (x0, y0, z0, x1, y1, z1) = (b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z);
    let faces: [([V3; 4], V3, V3); 5] = [
        ([v3(x0, y1, z1), v3(x1, y1, z1), v3(x1, y1, z0), v3(x0, y1, z0)], v3(0.0, 1.0, 0.0), v3(1.0, 0.0, 0.0)),
        ([v3(x0, y0, z1), v3(x1, y0, z1), v3(x1, y1, z1), v3(x0, y1, z1)], v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 0.0)),
        ([v3(x1, y0, z0), v3(x0, y0, z0), v3(x0, y1, z0), v3(x1, y1, z0)], v3(0.0, 0.0, -1.0), v3(-1.0, 0.0, 0.0)),
        ([v3(x0, y0, z0), v3(x0, y0, z1), v3(x0, y1, z1), v3(x0, y1, z0)], v3(-1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ([v3(x1, y0, z1), v3(x1, y0, z0), v3(x1, y1, z0), v3(x1, y1, z1)], v3(1.0, 0.0, 0.0), v3(0.0, 0.0, -1.0)),
    ];
    let s8 = |x: f32| (x * 127.0).round() as i8 as u8;
    for (pts, n, t) in faces {
        if idx.len() + 6 > 60000 {
            break;
        }
        let base = (v.len() / VERTEX_SIZE) as u16;
        for p in pts {
            v.extend_from_slice(&p.x.to_le_bytes());
            v.extend_from_slice(&p.y.to_le_bytes());
            v.extend_from_slice(&p.z.to_le_bytes());
            v.extend_from_slice(&[s8(n.x), s8(n.y), s8(n.z), 0]);
            v.extend_from_slice(&[s8(t.x), s8(t.y), s8(t.z), 127]);
            v.extend_from_slice(&0f32.to_le_bytes());
            v.extend_from_slice(&0f32.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, 255, 0, 0, 0]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn color_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|x| (clamp(x, 0.0, 1.0) * 255.0 + 0.5) as u8)
}

fn frame_uv(fi: usize) -> [f32; 4] {
    let f = VFX_FRAMES[fi.min(VFX_FRAMES.len() - 1)];
    [f[0], f[1], f[2], f[3]]
}

fn white_uv() -> [f32; 2] {
    let f = frame_uv(G_WHITE.0);
    [(f[0] + f[2]) * 0.5, (f[1] + f[3]) * 0.5]
}

fn push_billboard(v: &mut Vec<SVert>, fi: usize, pos: V3, size: f32, rot: f32, right: V3, up: V3, col: [f32; 4]) {
    if v.len() / 4 >= MAX_QUADS {
        return;
    }
    let f = VFX_FRAMES[fi.min(VFX_FRAMES.len() - 1)];
    let [u0, v0, u1, v1] = frame_uv(fi);
    let (hw, hh) = (f[4] * size * 0.5, f[5] * size * 0.5);
    let (sn, cs) = rot.sin_cos();
    let c = color_u8(col);
    let corners = [(-hw, -hh, u0, v1), (hw, -hh, u1, v1), (hw, hh, u1, v0), (-hw, hh, u0, v0)];
    for (x, y, u, vv) in corners {
        let rx = x * cs - y * sn;
        let ry = x * sn + y * cs;
        let p = pos + right * rx + up * ry;
        v.push(SVert { p: [p.x, p.y, p.z], uv: [u, vv], c });
    }
}

fn push_ground(v: &mut Vec<SVert>, fi: usize, pos: V3, size: f32, rot: f32, col: [f32; 4]) {
    if v.len() / 4 >= MAX_QUADS {
        return;
    }
    let f = VFX_FRAMES[fi.min(VFX_FRAMES.len() - 1)];
    let [u0, v0, u1, v1] = frame_uv(fi);
    let (hw, hd) = (f[4] * size * 0.5, f[5] * size * 0.5);
    let (sn, cs) = rot.sin_cos();
    let c = color_u8(col);
    for (x, z, u, vv) in [(-hw, hd, u0, v1), (hw, hd, u1, v1), (hw, -hd, u1, v0), (-hw, -hd, u0, v0)] {
        let rx = x * cs - z * sn;
        let rz = x * sn + z * cs;
        v.push(SVert { p: [pos.x + rx, pos.y, pos.z + rz], uv: [u, vv], c });
    }
}

fn rect(v: &mut Vec<SVert>, x: f32, y: f32, w: f32, h: f32, col: [f32; 4]) {
    let a = col[3];
    let c = color_u8([col[0] * a, col[1] * a, col[2] * a, a]);
    let uv = white_uv();
    for (px, py) in [(x, y + h), (x + w, y + h), (x + w, y), (x, y)] {
        v.push(SVert { p: [px, py, 0.0], uv, c });
    }
}

fn sprite2d(v: &mut Vec<SVert>, fi: usize, x: f32, y: f32, w: f32, h: f32, rot: f32, col: [f32; 4]) {
    let [u0, v0, u1, v1] = frame_uv(fi);
    let o = if col[3] > 0.0 { col[3] } else { 1.0 };
    let c = color_u8([col[0] * o, col[1] * o, col[2] * o, col[3]]);
    let (sn, cs) = rot.sin_cos();
    for (dx, dy, u, vv) in [(-0.5, 0.5, u0, v1), (0.5, 0.5, u1, v1), (0.5, -0.5, u1, v0), (-0.5, -0.5, u0, v0)] {
        let (px, py) = (dx * w, dy * h);
        v.push(SVert { p: [x + px * cs - py * sn, y + px * sn + py * cs, 0.0], uv: [u, vv], c });
    }
}

fn vignette(v: &mut Vec<SVert>, w: f32, h: f32, edge: [f32; 4]) {
    let xs = [0.0, w * 0.2, w * 0.8, w];
    let ys = [0.0, h * 0.22, h * 0.78, h];
    let uv = white_uv();
    let ec = color_u8(edge);
    for j in 0..3 {
        for i in 0..3 {
            if i == 1 && j == 1 {
                continue;
            }
            for (ci, cj) in [(i, j + 1), (i + 1, j + 1), (i + 1, j), (i, j)] {
                let inner = (ci == 1 || ci == 2) && (cj == 1 || cj == 2);
                v.push(SVert { p: [xs[ci], ys[cj], 0.0], uv, c: if inner { [0; 4] } else { ec } });
            }
        }
    }
}

/// General 4x4 inverse (for the sky ray reconstruction).
fn invert(m: &M4) -> M4 {
    let a = &m.0;
    let mut inv = [0.0f32; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15] + a[9] * a[7] * a[14] + a[13] * a[6] * a[11] - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15] - a[8] * a[7] * a[14] - a[12] * a[6] * a[11] + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15] + a[8] * a[7] * a[13] + a[12] * a[5] * a[11] - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14] - a[8] * a[6] * a[13] - a[12] * a[5] * a[10] + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15] - a[9] * a[3] * a[14] - a[13] * a[2] * a[11] + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15] + a[8] * a[3] * a[14] + a[12] * a[2] * a[11] - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15] - a[8] * a[3] * a[13] - a[12] * a[1] * a[11] + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14] + a[8] * a[2] * a[13] + a[12] * a[1] * a[10] - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15] + a[5] * a[3] * a[14] + a[13] * a[2] * a[7] - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15] - a[4] * a[3] * a[14] - a[12] * a[2] * a[7] + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15] + a[4] * a[3] * a[13] + a[12] * a[1] * a[7] - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14] - a[4] * a[2] * a[13] - a[12] * a[1] * a[6] + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11] - a[5] * a[3] * a[10] - a[9] * a[2] * a[7] + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11] + a[4] * a[3] * a[10] + a[8] * a[2] * a[7] - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11] - a[4] * a[3] * a[9] - a[8] * a[1] * a[7] + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10] + a[4] * a[2] * a[9] + a[8] * a[1] * a[6] - a[8] * a[2] * a[5];
    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    let d = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };
    M4(inv.map(|x| x * d))
}


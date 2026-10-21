//! OpenGL ES 2.0 renderer: textured 3D ground + concrete ruins, depth-sorted
//! billboard characters/VFX from one atlas, and the minimap overlay.

use std::ffi::CString;
use std::ptr;

use crate::atlas_gen::*;
use crate::game::{Game, Sprite};
use crate::gl::*;
use crate::math::{clamp, v3, M4, V2, V3};
use crate::world::{Mat, Wall, SIZE, TILE, TILES};

/// GL texture names supplied by the platform (already uploaded, premultiplied).
#[derive(Clone, Copy, Debug, Default)]
pub struct Textures {
    pub sprites: GLuint,
    pub asphalt: GLuint,
    pub grass: GLuint,
    pub mud: GLuint,
    pub dirt: GLuint,
    pub concrete: [GLuint; 3],
}

pub const TEX_SLOTS: usize = 8;

impl Textures {
    pub fn from_slots(s: &[GLuint; TEX_SLOTS]) -> Textures {
        Textures { sprites: s[0], asphalt: s[1], grass: s[2], mud: s[3], dirt: s[4], concrete: [s[5], s[6], s[7]] }
    }
}

const WORLD_VS: &str = r#"
attribute vec3 aPos; attribute vec2 aUV; attribute vec3 aNrm; attribute float aW;
uniform mat4 uMVP;
varying vec2 vUV; varying vec3 vPos; varying float vShade; varying float vW; varying float vNrmY;
uniform vec3 uLight;
void main(){
  vUV = aUV; vPos = aPos; vW = aW; vNrmY = aNrm.y;
  vShade = 0.5 + 0.55 * max(dot(normalize(aNrm), uLight), 0.0);
  gl_Position = uMVP * vec4(aPos, 1.0);
}"#;

const WORLD_FS: &str = r#"
precision mediump float;
uniform sampler2D uTex; uniform vec2 uPlayer; uniform vec3 uTint; uniform float uBlend;
varying vec2 vUV; varying vec3 vPos; varying float vShade; varying float vW; varying float vNrmY;
void main(){
  vec4 t = texture2D(uTex, vUV);
  if (vNrmY > 0.9) {
    // second, rotated lower-frequency sample hides the tiling grid
    vec2 r = vec2(vUV.x * 0.8 - vUV.y * 0.6, vUV.x * 0.6 + vUV.y * 0.8) * 0.37;
    t = mix(t, texture2D(uTex, r + 0.21), 0.45);
  }
  float a = 1.0;
  if (uBlend > 0.5) {
    float lum = dot(t.rgb, vec3(0.333));
    a = smoothstep(0.25, 0.75, vW + (lum - 0.4) * 0.7);
  }
  float d = distance(vPos.xz, uPlayer);
  float lit = mix(1.12, 0.4, smoothstep(7.0, 26.0, d));
  float ao = mix(0.55, 1.0, smoothstep(0.0, 1.4, vPos.y));
  vec3 c = t.rgb * uTint * vShade * lit * ao;
  gl_FragColor = vec4(c * a, a);
}"#;

const SPRITE_VS: &str = r#"
attribute vec3 aPos; attribute vec2 aUV; attribute vec4 aCol; attribute float aFlash;
uniform mat4 uMVP;
varying vec2 vUV; varying vec4 vCol; varying float vFlash; varying vec3 vPos;
void main(){
  vUV = aUV; vCol = aCol; vFlash = aFlash; vPos = aPos;
  gl_Position = uMVP * vec4(aPos, 1.0);
}"#;

const SPRITE_FS: &str = r#"
precision mediump float;
uniform sampler2D uTex; uniform vec2 uPlayer; uniform float uLit; uniform float uSil;
varying vec2 vUV; varying vec4 vCol; varying float vFlash; varying vec3 vPos;
void main(){
  vec4 t = texture2D(uTex, vUV);
  vec3 rgb = mix(t.rgb * vCol.rgb, vec3(1.0, 0.92, 0.85) * t.a, vFlash * 0.55);
  rgb = mix(rgb, vCol.rgb * t.a, uSil);
  if (uLit > 0.5) {
    float d = distance(vPos.xz, uPlayer);
    rgb *= mix(1.15, 0.45, smoothstep(7.0, 26.0, d));
  }
  gl_FragColor = vec4(rgb, t.a * vCol.a);
}"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct WVert {
    p: [f32; 3],
    uv: [f32; 2],
    n: [f32; 3],
    w: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SVert {
    p: [f32; 3],
    uv: [f32; 2],
    c: [u8; 4],
    flash: f32,
}

struct Program {
    id: GLuint,
    mvp: GLint,
    tex: GLint,
    player: GLint,
    extra: [GLint; 3],
}

struct Mesh {
    vbo: GLuint,
    count: i32,
    tex: GLuint,
    blend: bool,
}

pub struct Renderer {
    world: Program,
    sprite: Program,
    tex: Textures,
    ground: Vec<Mesh>,
    walls: Vec<Mesh>,
    svbo: GLuint,
    sibo: GLuint,
    verts: Vec<SVert>,
    sprites: Vec<Sprite>,
    order: Vec<(f32, u32)>,
    pub width: i32,
    pub height: i32,
    cam_target: V3,
    cam_init: bool,
    pub zoom: f32,
    pub map_expanded: bool,
    /// minimap square in surface pixels: x, y (top-left origin), size
    pub minimap: [f32; 3],
    shake_t: f32,
    /// set by a new game (UI thread); consumed on the GL thread
    pub reset_pending: bool,
}

const MAX_SPRITES: usize = 2000;

impl Renderer {
    pub fn new(tex: Textures, game: &Game) -> Renderer {
        unsafe {
            for (t, repeat) in [
                (tex.sprites, false),
                (tex.asphalt, true),
                (tex.grass, true),
                (tex.mud, true),
                (tex.dirt, true),
                (tex.concrete[0], true),
                (tex.concrete[1], true),
                (tex.concrete[2], true),
            ] {
                glBindTexture(TEXTURE_2D, t);
                let wrap = if repeat { REPEAT } else { CLAMP_TO_EDGE };
                glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, wrap);
                glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, wrap);
                glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR);
                glGenerateMipmap(TEXTURE_2D);
                glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR_MIPMAP_LINEAR);
            }
        }
        let world = Program::new(WORLD_VS, WORLD_FS, &["aPos", "aUV", "aNrm", "aW"], &["uLight", "uTint", "uBlend"]);
        let sprite = Program::new(SPRITE_VS, SPRITE_FS, &["aPos", "aUV", "aCol", "aFlash"], &["uLit", "uSil", "uSil"]);
        let mut r = Renderer {
            world,
            sprite,
            tex,
            ground: Vec::new(),
            walls: Vec::new(),
            svbo: 0,
            sibo: 0,
            verts: Vec::with_capacity(MAX_SPRITES * 4),
            sprites: Vec::with_capacity(MAX_SPRITES),
            order: Vec::with_capacity(MAX_SPRITES),
            width: 1,
            height: 1,
            cam_target: v3(0.0, 0.0, 0.0),
            cam_init: false,
            zoom: 1.0,
            map_expanded: false,
            minimap: [0.0, 0.0, 0.0],
            shake_t: 0.0,
            reset_pending: false,
        };
        r.build_static(game);
        unsafe {
            let mut b = [0u32; 2];
            glGenBuffers(2, b.as_mut_ptr());
            r.svbo = b[0];
            r.sibo = b[1];
            glBindBuffer(ARRAY_BUFFER, r.svbo);
            glBufferData(ARRAY_BUFFER, (MAX_SPRITES * 4 * std::mem::size_of::<SVert>()) as isize, ptr::null(), DYNAMIC_DRAW);
            let mut idx: Vec<u16> = Vec::with_capacity(MAX_SPRITES * 6);
            for i in 0..MAX_SPRITES as u16 {
                let b = i * 4;
                idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
            glBindBuffer(ELEMENT_ARRAY_BUFFER, r.sibo);
            glBufferData(ELEMENT_ARRAY_BUFFER, (idx.len() * 2) as isize, idx.as_ptr() as *const _, STATIC_DRAW);
        }
        r
    }

    /// Rebuild level geometry (called on new game; the level layout is fixed).
    pub fn build_static(&mut self, game: &Game) {
        for m in self.ground.drain(..).chain(self.walls.drain(..)) {
            unsafe { glDeleteBuffers(1, &m.vbo) };
        }
        let w = &game.world;
        // base grass: one big quad
        let mut base = Vec::new();
        quad_flat(&mut base, 0.0, 0.0, SIZE, SIZE, 0.0, [1.0; 4]);
        self.ground.push(Mesh::upload(&base, self.tex.grass, false));
        // blended material layers
        for (mat, tex) in [(Mat::Dirt, self.tex.dirt), (Mat::Mud, self.tex.mud), (Mat::Asphalt, self.tex.asphalt)] {
            let weight = |vx: usize, vz: usize| -> f32 {
                let mut n = 0.0;
                for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (tx, tz) = (vx as i32 - dx, vz as i32 - dz);
                    if tx >= 0 && tz >= 0 && (tx as usize) < TILES && (tz as usize) < TILES && w.tiles[tz as usize][tx as usize] == mat {
                        n += 1.0;
                    }
                }
                n / 4.0
            };
            let mut v = Vec::new();
            for z in 0..TILES {
                for x in 0..TILES {
                    let ws = [weight(x, z), weight(x + 1, z), weight(x + 1, z + 1), weight(x, z + 1)];
                    if ws.iter().all(|w| *w <= 0.0) {
                        continue;
                    }
                    let ws = ws.map(|w| if w >= 0.74 { 1.0 } else { w });
                    quad_flat(&mut v, x as f32 * TILE, z as f32 * TILE, TILE, TILE, 0.004 * (mat as u8 as f32), ws);
                }
            }
            if !v.is_empty() {
                self.ground.push(Mesh::upload(&v, tex, true));
            }
        }
        for style in 0..3u8 {
            let mut v = Vec::new();
            for wall in w.walls.iter().filter(|w| w.style == style) {
                wall_box(&mut v, wall);
            }
            if !v.is_empty() {
                self.walls.push(Mesh::upload(&v, self.tex.concrete[style as usize], false));
            }
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.width = w.max(1);
        self.height = h.max(1);
    }

    fn camera(&mut self, game: &Game, dt: f32) -> (M4, V3, V3) {
        let p = game.player.pos;
        let target = v3(p.x, 0.9, p.y);
        if !self.cam_init {
            self.cam_target = target;
            self.cam_init = true;
        }
        let k = clamp(dt * 7.0, 0.0, 1.0);
        self.cam_target = self.cam_target + (target - self.cam_target) * k;
        self.shake_t += dt * 40.0;
        let s = game.shake * game.shake * 0.35;
        let shake = v3((self.shake_t * 1.3).sin() * s, (self.shake_t * 1.7).cos() * s * 0.5, (self.shake_t * 1.1).cos() * s);
        let t = self.cam_target + shake;
        let aspect = self.width as f32 / self.height as f32;
        // keep the same horizontal coverage on tall/narrow screens
        let dist = self.zoom * if aspect < 1.5 { 1.0 + (1.5 - aspect) * 0.6 } else { 1.0 };
        let eye = t + v3(0.0, 7.6 * dist, 5.6 * dist);
        let view = M4::look_at(eye, t, v3(0.0, 1.0, 0.0));
        let proj = M4::perspective(0.82, aspect, 0.5, 120.0);
        (proj.mul(&view), eye, (t - eye))
    }

    pub fn render(&mut self, game: &Game, dt: f32) {
        if self.reset_pending {
            self.reset_pending = false;
            self.build_static(game);
            self.cam_init = false;
        }
        let (mvp, eye, fwd) = self.camera(game, dt);
        let fwd = fwd.norm();
        let ppos = game.player.pos;
        unsafe {
            glViewport(0, 0, self.width, self.height);
            glClearColor(0.04, 0.05, 0.05, 1.0);
            glClear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
            glEnable(DEPTH_TEST);
            glDepthFunc(LEQUAL);
            glEnable(BLEND);
            glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
            glDisable(CULL_FACE);

            // ---- opaque-ish world
            glUseProgram(self.world.id);
            glUniformMatrix4fv(self.world.mvp, 1, FALSE, mvp.0.as_ptr());
            glUniform1i(self.world.tex, 0);
            glUniform2f(self.world.player, ppos.x, ppos.y);
            let l = v3(-0.45, 0.8, 0.4).norm();
            glUniform3f(self.world.extra[0], l.x, l.y, l.z);
            glActiveTexture(TEXTURE0);
            glDepthMask(TRUE);
            for (i, m) in self.ground.iter().enumerate() {
                let tint = if i == 0 { [0.82, 0.88, 0.78] } else { [0.9, 0.9, 0.88] };
                glUniform3f(self.world.extra[1], tint[0], tint[1], tint[2]);
                glUniform1f(self.world.extra[2], if m.blend { 1.0 } else { 0.0 });
                m.draw_world();
            }
            glUniform1f(self.world.extra[2], 0.0);
            glUniform3f(self.world.extra[1], 0.92, 0.9, 0.88);
            for m in &self.walls {
                m.draw_world();
            }
        }

        // ---- sprites
        let mut sprites = std::mem::take(&mut self.sprites);
        game.collect_sprites(&mut sprites);
        let right = v3(1.0, 0.0, 0.0);
        let cam_up = right.cross(fwd).norm();
        let up = (v3(0.0, 1.0, 0.0) + cam_up).norm();
        let up = (v3(0.0, 1.0, 0.0) + up).norm(); // mostly upright, slightly leaning to camera

        self.verts.clear();
        // ground decals in submission order
        let mut ground_quads = 0;
        for s in sprites.iter().filter(|s| s.ground.is_some()) {
            if self.verts.len() / 4 >= MAX_SPRITES {
                break;
            }
            push_ground(&mut self.verts, s);
            ground_quads += 1;
        }
        // billboards sorted far -> near
        self.order.clear();
        for (i, s) in sprites.iter().enumerate() {
            if s.ground.is_none() {
                let d = (s.pos - eye).dot(fwd);
                self.order.push((d, i as u32));
            }
        }
        self.order.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut xray_first = None;
        for &(_, i) in &self.order {
            if self.verts.len() / 4 >= MAX_SPRITES {
                break;
            }
            let s = &sprites[i as usize];
            if s.xray {
                xray_first = Some(self.verts.len() / 4);
            }
            push_billboard(&mut self.verts, s, right, up);
        }
        let total = self.verts.len() / 4;
        unsafe {
            glUseProgram(self.sprite.id);
            glUniformMatrix4fv(self.sprite.mvp, 1, FALSE, mvp.0.as_ptr());
            glUniform1i(self.sprite.tex, 0);
            glUniform2f(self.sprite.player, ppos.x, ppos.y);
            glUniform1f(self.sprite.extra[0], 1.0);
            glUniform1f(self.sprite.extra[1], 0.0);
            glBindTexture(TEXTURE_2D, self.tex.sprites);
            self.upload_sprites();
            glDepthMask(FALSE);
            glEnable(POLYGON_OFFSET_FILL);
            glPolygonOffset(-1.0, -2.0);
            draw_quads(0, ground_quads);
            glDisable(POLYGON_OFFSET_FILL);
            draw_quads(ground_quads, total - ground_quads);
            // x-ray silhouette of the player behind walls
            if let Some(q) = xray_first {
                for v in &mut self.verts[q * 4..q * 4 + 4] {
                    v.c = [40, 110, 170, 150];
                }
                self.upload_sprites();
                glDepthFunc(GREATER);
                glUniform1f(self.sprite.extra[1], 1.0);
                draw_quads(q, 1);
                glUniform1f(self.sprite.extra[1], 0.0);
                glDepthFunc(LEQUAL);
            }
        }
        self.sprites = sprites;

        // ---- overlays
        unsafe {
            glDisable(DEPTH_TEST);
        }
        self.draw_overlay(game);
        unsafe {
            glDepthMask(TRUE);
        }
    }

    fn upload_sprites(&self) {
        let n = self.verts.len().min(MAX_SPRITES * 4);
        unsafe {
            glBindBuffer(ARRAY_BUFFER, self.svbo);
            glBufferSubData(ARRAY_BUFFER, 0, (n * std::mem::size_of::<SVert>()) as isize, self.verts.as_ptr() as *const _);
            glBindBuffer(ELEMENT_ARRAY_BUFFER, self.sibo);
            let stride = std::mem::size_of::<SVert>() as i32;
            glEnableVertexAttribArray(0);
            glEnableVertexAttribArray(1);
            glEnableVertexAttribArray(2);
            glEnableVertexAttribArray(3);
            glVertexAttribPointer(0, 3, FLOAT, FALSE, stride, ptr::null());
            glVertexAttribPointer(1, 2, FLOAT, FALSE, stride, 12 as *const _);
            glVertexAttribPointer(2, 4, UNSIGNED_BYTE, TRUE, stride, 20 as *const _);
            glVertexAttribPointer(3, 1, FLOAT, FALSE, stride, 24 as *const _);
        }
    }

    // ------------------------------------------------------------ 2D overlay
    fn draw_overlay(&mut self, game: &Game) {
        let (w, h) = (self.width as f32, self.height as f32);
        self.verts.clear();
        let hp_k = clamp(game.player.hp / game.player.hp_max, 0.0, 1.0);
        // vignette (stronger and red when hurt)
        let danger = clamp((0.4 - hp_k) / 0.4, 0.0, 1.0);
        let pulse = 0.75 + 0.25 * (game.time * 5.0).sin();
        let a = 0.55 + 0.25 * danger * pulse;
        let edge = [a * danger * 0.7, 0.0, 0.0, a];
        vignette(&mut self.verts, w, h, edge);

        let [mx, my, ms] = self.minimap;
        let expanded = self.map_expanded;
        let (cx, cy, size, center, range) = if expanded {
            let size = h.min(w) * 0.86;
            (w * 0.5 - size * 0.5, h * 0.5 - size * 0.5, size, V2 { x: SIZE * 0.5, y: SIZE * 0.5 }, SIZE * 0.5)
        } else {
            (mx, my, ms, game.player.pos, 26.0)
        };
        let scissor_from = self.verts.len() / 4;
        if size > 4.0 {
            let ppm = size / (range * 2.0);
            let to_map = |p: V2| -> (f32, f32) { (cx + size * 0.5 + (p.x - center.x) * ppm, cy + size * 0.5 + (p.y - center.y) * ppm) };
            rect(&mut self.verts, cx - 3.0, cy - 3.0, size + 6.0, size + 6.0, [0.55, 0.62, 0.45, 0.9]);
            rect(&mut self.verts, cx, cy, size, size, [0.07, 0.09, 0.07, 0.86]);
            let wd = &game.world;
            for z in 0..TILES {
                for x in 0..TILES {
                    let m = wd.tiles[z][x];
                    let c = match m {
                        Mat::Asphalt => [0.27, 0.28, 0.28, 0.95],
                        Mat::Mud => [0.2, 0.16, 0.11, 0.9],
                        Mat::Dirt => [0.25, 0.21, 0.14, 0.9],
                        Mat::Grass => continue,
                    };
                    let (x0, y0) = to_map(V2 { x: x as f32 * TILE, y: z as f32 * TILE });
                    rect(&mut self.verts, x0, y0, TILE * ppm + 0.5, TILE * ppm + 0.5, c);
                }
            }
            for wall in &wd.walls {
                let (x0, y0) = to_map(wall.min);
                let (x1, y1) = to_map(wall.max);
                let c = if wall.height > 2.0 { [0.72, 0.72, 0.68, 1.0] } else { [0.48, 0.48, 0.45, 1.0] };
                rect(&mut self.verts, x0, y0, (x1 - x0).max(1.5), (y1 - y0).max(1.5), c);
            }
            let dot = (size / 60.0).max(4.0);
            if game.extraction_active {
                let (ex, ey) = to_map(wd.extraction);
                let pr = dot * (2.2 + 0.4 * (game.time * 5.0).sin());
                sprite2d(&mut self.verts, G_DISC.0, ex, ey, pr * 2.0, pr * 2.0, 0.0, [1.0, 0.85, 0.15, 0.0]);
                sprite2d(&mut self.verts, G_RING.0, ex, ey, pr * 3.0, pr * 3.0, 0.0, [1.0, 0.85, 0.2, 1.0]);
            }
            for p in &game.pickups {
                let (x, y) = to_map(p.pos);
                sprite2d(&mut self.verts, G_DISC.0, x, y, dot * 1.6, dot * 1.6, 0.0, [0.3, 1.0, 0.4, 1.0]);
            }
            for z in game.zombies.iter().filter(|z| z.alive()) {
                let (x, y) = to_map(z.pos);
                sprite2d(&mut self.verts, G_DISC.0, x, y, dot * 1.7, dot * 1.7, 0.0, [0.95, 0.15, 0.1, 1.0]);
            }
            let (px, py) = to_map(game.player.pos);
            let f = game.player.facing;
            let ang = f.x.atan2(-f.y);
            sprite2d(&mut self.verts, G_ARROW.0, px, py, dot * 3.4, dot * 3.4, ang, [0.6, 1.0, 0.45, 1.0]);
        }
        let n = (self.verts.len() / 4).min(MAX_SPRITES);
        let proj = M4::ortho(0.0, w, h, 0.0);
        unsafe {
            glUseProgram(self.sprite.id);
            glUniformMatrix4fv(self.sprite.mvp, 1, FALSE, proj.0.as_ptr());
            glUniform1f(self.sprite.extra[0], 0.0);
            glUniform1f(self.sprite.extra[1], 0.0);
            self.upload_sprites();
            draw_quads(0, scissor_from + 2);
            if n > scissor_from + 2 {
                glEnable(SCISSOR_TEST);
                glScissor(cx as i32, (h - cy - size) as i32, size as i32, size as i32);
                draw_quads(scissor_from + 2, n - scissor_from - 2);
                glDisable(SCISSOR_TEST);
            }
        }
    }
}

impl Program {
    fn new(vs: &str, fs: &str, attribs: &[&str], extra: &[&str; 3]) -> Program {
        unsafe {
            let v = compile(VERTEX_SHADER, vs);
            let f = compile(FRAGMENT_SHADER, fs);
            let id = glCreateProgram();
            glAttachShader(id, v);
            glAttachShader(id, f);
            for (i, a) in attribs.iter().enumerate() {
                let c = CString::new(*a).unwrap();
                glBindAttribLocation(id, i as u32, c.as_ptr());
            }
            glLinkProgram(id);
            let mut ok = 0;
            glGetProgramiv(id, LINK_STATUS, &mut ok);
            if ok == 0 {
                let mut buf = [0i8; 1024];
                let mut len = 0;
                glGetProgramInfoLog(id, 1024, &mut len, buf.as_mut_ptr() as *mut _);
                crate::log(&format!("program link failed: {}", cstr(&buf, len)));
            }
            glDeleteShader(v);
            glDeleteShader(f);
            let loc = |n: &str| {
                let c = CString::new(n).unwrap();
                glGetUniformLocation(id, c.as_ptr())
            };
            Program { id, mvp: loc("uMVP"), tex: loc("uTex"), player: loc("uPlayer"), extra: [loc(extra[0]), loc(extra[1]), loc(extra[2])] }
        }
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
        let mut buf = [0i8; 1024];
        let mut len = 0;
        glGetShaderInfoLog(s, 1024, &mut len, buf.as_mut_ptr() as *mut _);
        crate::log(&format!("shader compile failed: {}", cstr(&buf, len)));
    }
    s
}

fn cstr(buf: &[i8], len: i32) -> String {
    let b: Vec<u8> = buf[..len.max(0) as usize].iter().map(|c| *c as u8).collect();
    String::from_utf8_lossy(&b).into_owned()
}

impl Mesh {
    fn upload(v: &[WVert], tex: GLuint, blend: bool) -> Mesh {
        let mut vbo = 0;
        unsafe {
            glGenBuffers(1, &mut vbo);
            glBindBuffer(ARRAY_BUFFER, vbo);
            glBufferData(ARRAY_BUFFER, std::mem::size_of_val(v) as isize, v.as_ptr() as *const _, STATIC_DRAW);
        }
        Mesh { vbo, count: v.len() as i32, tex, blend }
    }

    unsafe fn draw_world(&self) {
        glBindTexture(TEXTURE_2D, self.tex);
        glBindBuffer(ARRAY_BUFFER, self.vbo);
        let stride = std::mem::size_of::<WVert>() as i32;
        for i in 0..4 {
            glEnableVertexAttribArray(i);
        }
        glVertexAttribPointer(0, 3, FLOAT, FALSE, stride, ptr::null());
        glVertexAttribPointer(1, 2, FLOAT, FALSE, stride, 12 as *const _);
        glVertexAttribPointer(2, 3, FLOAT, FALSE, stride, 20 as *const _);
        glVertexAttribPointer(3, 1, FLOAT, FALSE, stride, 32 as *const _);
        glDrawArrays(TRIANGLES, 0, self.count);
    }
}

unsafe fn draw_quads(first: usize, count: usize) {
    if count == 0 {
        return;
    }
    glDrawElements(TRIANGLES, (count * 6) as i32, UNSIGNED_SHORT, (first * 12) as *const _);
}

fn quad_flat(v: &mut Vec<WVert>, x: f32, z: f32, w: f32, d: f32, y: f32, ws: [f32; 4]) {
    let uvs = 1.0 / TILE;
    let c = [(x, z, ws[0]), (x + w, z, ws[1]), (x + w, z + d, ws[2]), (x, z + d, ws[3])];
    for i in [0, 1, 2, 0, 2, 3] {
        let (px, pz, ww) = c[i];
        v.push(WVert { p: [px, y, pz], uv: [px * uvs, pz * uvs], n: [0.0, 1.0, 0.0], w: ww });
    }
}

fn wall_box(v: &mut Vec<WVert>, w: &Wall) {
    let (x0, z0, x1, z1, h) = (w.min.x, w.min.y, w.max.x, w.max.y, w.height);
    let s = 1.0 / 3.0;
    let mut face = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], n: [f32; 3], uw: f32, vh: f32| {
        let uvs = [[0.0, vh], [uw, vh], [uw, 0.0], [0.0, 0.0]];
        let ps = [a, b, c, d];
        for i in [0, 1, 2, 0, 2, 3] {
            v.push(WVert { p: ps[i], uv: [uvs[i][0] + x0 * 0.37, uvs[i][1] + z0 * 0.21], n, w: 1.0 });
        }
    };
    let (lx, lz) = ((x1 - x0) * s, (z1 - z0) * s);
    let hv = h * s;
    face([x0, 0.0, z1], [x1, 0.0, z1], [x1, h, z1], [x0, h, z1], [0.0, 0.0, 1.0], lx, hv); // south
    face([x1, 0.0, z0], [x0, 0.0, z0], [x0, h, z0], [x1, h, z0], [0.0, 0.0, -1.0], lx, hv); // north
    face([x0, 0.0, z0], [x0, 0.0, z1], [x0, h, z1], [x0, h, z0], [-1.0, 0.0, 0.0], lz, hv); // west
    face([x1, 0.0, z1], [x1, 0.0, z0], [x1, h, z0], [x1, h, z1], [1.0, 0.0, 0.0], lz, hv); // east
    face([x0, h, z1], [x1, h, z1], [x1, h, z0], [x0, h, z0], [0.0, 0.35, 0.0], lx, lz); // top (darker)
}

fn color_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|x| (clamp(x, 0.0, 1.0) * 255.0 + 0.5) as u8)
}

fn frame_uv(fi: usize, mirror: bool) -> (f32, f32, f32, f32) {
    let f = &FRAMES[fi];
    if mirror {
        (f.u1, f.v0, f.u0, f.v1)
    } else {
        (f.u0, f.v0, f.u1, f.v1)
    }
}

fn push_billboard(v: &mut Vec<SVert>, s: &Sprite, right: V3, up: V3) {
    let (u0, v0, u1, v1) = frame_uv(s.frame as usize, s.mirror);
    let c = color_u8(s.color);
    let left = -s.ax * s.w;
    let bottom = -s.ay * s.h;
    let corner = |dx: f32, dy: f32| s.pos + right * dx + up * dy;
    let p = [
        corner(left, bottom),
        corner(left + s.w, bottom),
        corner(left + s.w, bottom + s.h),
        corner(left, bottom + s.h),
    ];
    let uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
    for i in 0..4 {
        v.push(SVert { p: [p[i].x, p[i].y, p[i].z], uv: uv[i], c, flash: s.flash });
    }
}

fn push_ground(v: &mut Vec<SVert>, s: &Sprite) {
    let (u0, v0, u1, v1) = frame_uv(s.frame as usize, s.mirror);
    let c = color_u8(s.color);
    let rot = s.ground.unwrap_or(0.0);
    let (sn, cs) = rot.sin_cos();
    let hw = s.w * 0.5;
    let hd = s.h * 0.5;
    let pts = [(-hw, hd), (hw, hd), (hw, -hd), (-hw, -hd)];
    let uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
    for i in 0..4 {
        let (x, z) = pts[i];
        let rx = x * cs - z * sn;
        let rz = x * sn + z * cs;
        v.push(SVert { p: [s.pos.x + rx, s.pos.y, s.pos.z + rz], uv: uv[i], c, flash: s.flash });
    }
}

fn white_uv() -> [f32; 2] {
    let f = &FRAMES[G_WHITE.0];
    [(f.u0 + f.u1) * 0.5, (f.v0 + f.v1) * 0.5]
}

fn rect(v: &mut Vec<SVert>, x: f32, y: f32, w: f32, h: f32, col: [f32; 4]) {
    let a = col[3];
    let c = color_u8([col[0] * a, col[1] * a, col[2] * a, a]);
    let uv = white_uv();
    for (px, py) in [(x, y + h), (x + w, y + h), (x + w, y), (x, y)] {
        v.push(SVert { p: [px, py, 0.0], uv, c, flash: 0.0 });
    }
}

/// 2D sprite centred at (x, y); col rgb straight, col[3] = opacity (0 additive).
fn sprite2d(v: &mut Vec<SVert>, fi: usize, x: f32, y: f32, w: f32, h: f32, rot: f32, col: [f32; 4]) {
    let (u0, v0, u1, v1) = frame_uv(fi, false);
    let o = if col[3] > 0.0 { col[3] } else { 1.0 };
    let c = color_u8([col[0] * o, col[1] * o, col[2] * o, col[3]]);
    let (sn, cs) = rot.sin_cos();
    let pts = [(-0.5, 0.5), (0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)];
    let uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
    for i in 0..4 {
        let (dx, dy) = (pts[i].0 * w, pts[i].1 * h);
        let rx = dx * cs - dy * sn;
        let ry = dx * sn + dy * cs;
        v.push(SVert { p: [x + rx, y + ry, 0.0], uv: uv[i], c, flash: 0.0 });
    }
}

/// Screen-edge darkening built from a 4x4 vertex grid (inner vertices clear).
fn vignette(v: &mut Vec<SVert>, w: f32, h: f32, edge: [f32; 4]) {
    let xs = [0.0, w * 0.22, w * 0.78, w];
    let ys = [0.0, h * 0.25, h * 0.75, h];
    let uv = white_uv();
    let ec = color_u8([edge[0], edge[1], edge[2], edge[3]]);
    let clear = [0u8; 4];
    for j in 0..3 {
        for i in 0..3 {
            if i == 1 && j == 1 {
                continue;
            }
            let corners = [(i, j + 1), (i + 1, j + 1), (i + 1, j), (i, j)];
            for (ci, cj) in corners {
                let inner = (ci == 1 || ci == 2) && (cj == 1 || cj == 2);
                v.push(SVert { p: [xs[ci], ys[cj], 0.0], uv, c: if inner { clear } else { ec }, flash: 0.0 });
            }
        }
    }
}

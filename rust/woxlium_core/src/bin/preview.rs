//! Desktop verification harness: renders scripted gameplay off-screen through
//! ANGLE (SwiftShader) and writes PPM screenshots + timing.
//!
//!   RUSTFLAGS="-L $ANGLE_DIR" cargo run --release --features preview --bin preview -- out_dir
//!   (run with LD_LIBRARY_PATH=$ANGLE_DIR)

use std::fs;
use std::io::Write;
use std::os::raw::{c_char, c_void};
use std::time::Instant;

use woxlium::engine::{self, lock, BTN_ATTACK, BTN_BLOCK, BTN_DODGE, BTN_KICK, ENGINE, INPUT, SETTINGS};
use woxlium::gl::*;
use woxlium::math::v2;

type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLSurface = *mut c_void;
type EGLContext = *mut c_void;

#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn eglInitialize(d: EGLDisplay, major: *mut i32, minor: *mut i32) -> u32;
    fn eglChooseConfig(d: EGLDisplay, attrs: *const i32, cfg: *mut EGLConfig, n: i32, num: *mut i32) -> u32;
    fn eglCreatePbufferSurface(d: EGLDisplay, cfg: EGLConfig, attrs: *const i32) -> EGLSurface;
    fn eglCreateContext(d: EGLDisplay, cfg: EGLConfig, share: EGLContext, attrs: *const i32) -> EGLContext;
    fn eglMakeCurrent(d: EGLDisplay, draw: EGLSurface, read: EGLSurface, ctx: EGLContext) -> u32;
    fn eglBindAPI(api: u32) -> u32;
    fn eglGetError() -> i32;
}

const W: i32 = 1280;
const H: i32 = 720;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "preview_out".into());
    let assets = std::env::args().nth(2).unwrap_or_else(|| "../../build/preview_assets".into());
    fs::create_dir_all(&out).unwrap();
    unsafe { init_egl() };

    let names = ["sprites", "tex_asphalt", "tex_grass", "tex_mud", "tex_dirt", "tex_concrete", "tex_concrete_cracked", "tex_concrete_holes"];
    let mut slots = [0u32; 8];
    for (i, n) in names.iter().enumerate() {
        slots[i] = unsafe { load_texture(&format!("{assets}/{n}.rgba")) };
    }
    engine::start_game(1, 0, 42);
    engine::surface_created(&slots);
    engine::surface_changed(W, H);
    lock(&SETTINGS).minimap = [W as f32 - 230.0, 20.0, 210.0];

    // scripted session: walk, fight, dodge, kick, block; shots along the way
    let shots = [3.0f32, 9.0, 14.0, 20.0, 27.0, 33.0, 35.5, 45.0];
    let mut restarted = false;
    let mut next_shot = 0;
    let dt = 1.0 / 60.0;
    let mut t = 0.0f32;
    let mut frame_ms = Vec::new();
    while t < 46.0 {
        {
            let mut inp = lock(&INPUT);
            let (nearest, _) = nearest_zombie();
            let mv = match nearest {
                Some(d) if d.len() > 1.3 => d.norm(),
                Some(_) => v2(0.0, 0.0),
                None => v2((t * 0.5).sin(), (t * 0.3).cos() * 0.6),
            };
            inp.mv = mv;
            let f = (t * 60.0) as i32;
            if f % 14 == 0 {
                inp.button(BTN_ATTACK, true);
            }
            if f % 150 == 75 {
                inp.button(BTN_KICK, true);
            }
            if f % 240 == 120 {
                inp.button(BTN_DODGE, true);
            }
            inp.button(BTN_BLOCK, (t % 10.0) > 8.5);
            // keep the hero alive for the demo
            drop(inp);
            if let Some(e) = lock(&ENGINE).as_mut() {
                e.game.player.hp = e.game.player.hp.max(35.0);
            }
        }
        if (t % 12.0) > 11.0 {
            lock(&SETTINGS).map_expanded = true;
        } else {
            lock(&SETTINGS).map_expanded = false;
        }
        if !restarted && t > 28.0 {
            // exercise the restart path (UI thread in the app): endless mode, leather skin
            restarted = true;
            engine::start_game(0, 1, 7);
        }
        let t0 = Instant::now();
        engine::frame(dt);
        unsafe { gl_finish_shim() };
        frame_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        t += dt;
        if next_shot < shots.len() && t >= shots[next_shot] {
            write_ppm(&format!("{out}/shot_{next_shot}.ppm"));
            next_shot += 1;
        }
    }
    frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let g = lock(&ENGINE);
    let game = &g.as_ref().unwrap().game;
    println!(
        "frames={} median_ms={:.2} p95_ms={:.2} (SwiftShader CPU raster) wave={} kills={} combo_max={} alive={}",
        frame_ms.len(),
        frame_ms[frame_ms.len() / 2],
        frame_ms[frame_ms.len() * 95 / 100],
        game.wave,
        game.kills,
        game.max_combo,
        game.alive_count()
    );
}

fn nearest_zombie() -> (Option<woxlium::math::V2>, usize) {
    let g = lock(&ENGINE);
    let Some(e) = g.as_ref() else { return (None, 0) };
    let p = e.game.player.pos;
    let best = e.game.zombies.iter().filter(|z| z.alive()).map(|z| z.pos - p).min_by(|a, b| a.len().partial_cmp(&b.len()).unwrap());
    (best, e.game.zombies.len())
}

unsafe fn gl_finish_shim() {
    let mut px = [0u8; 4];
    glReadPixels(0, 0, 1, 1, RGBA, UNSIGNED_BYTE, px.as_mut_ptr() as *mut _);
}

unsafe fn init_egl() {
    let get_platform: extern "C" fn(u32, *mut c_void, *const i32) -> EGLDisplay =
        std::mem::transmute(eglGetProcAddress(b"eglGetPlatformDisplayEXT\0".as_ptr() as *const _));
    let attrs: Vec<i32> = std::env::var("ANGLE_ATTRS").ok().map(|s| s.split(",").map(|x| i32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap()).collect()).unwrap_or(vec![0x3203, 0x3450, 0x3209, 0x3487, 0x348F, 0x31DD, 0x3038]);
    let d = get_platform(0x3202, std::ptr::null_mut(), attrs.as_ptr());
    let (mut ma, mut mi) = (0, 0);
    assert!(eglInitialize(d, &mut ma, &mut mi) != 0, "eglInitialize failed {:x}", eglGetError());
    eglBindAPI(0x30A0);
    let cfg_attrs = [0x3033, 0x0001, 0x3040, 0x0004, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3021, 8, 0x3025, 24, 0x3038];
    let mut cfg = std::ptr::null_mut();
    let mut n = 0;
    assert!(eglChooseConfig(d, cfg_attrs.as_ptr(), &mut cfg, 1, &mut n) != 0 && n > 0, "no config");
    let surf = eglCreatePbufferSurface(d, cfg, [0x3057, W, 0x3056, H, 0x3038].as_ptr());
    let ctx = eglCreateContext(d, cfg, std::ptr::null_mut(), [0x3098, 2, 0x3038].as_ptr());
    assert!(eglMakeCurrent(d, surf, surf, ctx) != 0, "make current {:x}", eglGetError());
}

unsafe fn load_texture(path: &str) -> u32 {
    let data = fs::read(path).unwrap_or_else(|_| panic!("missing {path}; run tools/build_assets.py"));
    let w = u32::from_le_bytes(data[0..4].try_into().unwrap()) as i32;
    let h = u32::from_le_bytes(data[4..8].try_into().unwrap()) as i32;
    let mut id = 0;
    glGenTextures(1, &mut id);
    glBindTexture(TEXTURE_2D, id);
    glTexImage2D(TEXTURE_2D, 0, RGBA as i32, w, h, 0, RGBA, UNSIGNED_BYTE, data[8..].as_ptr() as *const _);
    id
}

fn write_ppm(path: &str) {
    let mut px = vec![0u8; (W * H * 4) as usize];
    unsafe { glReadPixels(0, 0, W, H, RGBA, UNSIGNED_BYTE, px.as_mut_ptr() as *mut _) };
    let mut f = fs::File::create(path).unwrap();
    write!(f, "P6\n{W} {H}\n255\n").unwrap();
    let mut rgb = Vec::with_capacity((W * H * 3) as usize);
    for y in (0..H).rev() {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            rgb.extend_from_slice(&px[i..i + 3]);
        }
    }
    f.write_all(&rgb).unwrap();
}

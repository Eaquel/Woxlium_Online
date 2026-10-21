//! Desktop verification harness: scripted first-person session rendered
//! off-screen through ANGLE (SwiftShader); writes PPM screenshots.
//!
//!   RUSTFLAGS="-L $ANGLE" cargo run --release --features preview --bin preview -- out_dir assets_dir

use std::fs;
use std::io::Write;
use std::os::raw::{c_char, c_void};
use std::time::Instant;

use woxlium::assets::DirSource;
use woxlium::engine::{self, lock, BTN_ATTACK, BTN_BLOCK, ENGINE, INPUT, SETTINGS};
use woxlium::gl::*;
use woxlium::math::v2;

#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn eglInitialize(d: *mut c_void, major: *mut i32, minor: *mut i32) -> u32;
    fn eglChooseConfig(d: *mut c_void, attrs: *const i32, cfg: *mut *mut c_void, n: i32, num: *mut i32) -> u32;
    fn eglCreatePbufferSurface(d: *mut c_void, cfg: *mut c_void, attrs: *const i32) -> *mut c_void;
    fn eglCreateContext(d: *mut c_void, cfg: *mut c_void, share: *mut c_void, attrs: *const i32) -> *mut c_void;
    fn eglMakeCurrent(d: *mut c_void, draw: *mut c_void, read: *mut c_void, ctx: *mut c_void) -> u32;
    fn eglBindAPI(api: u32) -> u32;
    fn eglGetError() -> i32;
}

const W: i32 = 1280;
const H: i32 = 720;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "preview_out".into());
    let assets = std::env::args().nth(2).unwrap_or_else(|| "../../build/preview_assets".into());
    let script = std::env::args().nth(3).unwrap_or_else(|| "fight".into());
    fs::create_dir_all(&out).unwrap();
    unsafe { init_egl() };
    engine::set_source(Box::new(DirSource(assets.into())));
    engine::surface_created();
    if let Some(e) = lock(&ENGINE).error.clone() {
        panic!("init: {e}");
    }
    engine::surface_changed(W, H);
    lock(&SETTINGS).minimap = [W as f32 - 220.0, 20.0, 200.0];
    engine::start_game(if script == "story" { 1 } else { 0 }, 42);

    if script == "tour" {
        tour(&out);
        return;
    }
    let shots: Vec<f32> = vec![10.5, 13.0, 15.5, 17.0, 19.0, 22.0, 29.0, 31.0];
    let mut next = 0;
    let dt = 1.0 / 30.0;
    let mut t = 0.0f32;
    let mut ms = Vec::new();
    while t < 34.0 {
        {
            // aim at the nearest zombie, walk up and punch; block now and then
            let (target, player) = {
                let g = lock(&ENGINE);
                match g.game.as_ref() {
                    Some(game) => {
                        let p = game.player.pos;
                        let z = game.zombies.iter().filter(|z| z.alive()).map(|z| z.pos).min_by(|a, b| (*a - p).len().partial_cmp(&(*b - p).len()).unwrap());
                        (z, Some((p, game.player.yaw)))
                    }
                    None => (None, None),
                }
            };
            let mut inp = lock(&INPUT);
            if let (Some(z), Some((p, yaw))) = (target, player) {
                let d = z - p;
                let want = d.x.atan2(d.y);
                let mut dy = want - yaw;
                while dy > std::f32::consts::PI {
                    dy -= std::f32::consts::TAU;
                }
                while dy < -std::f32::consts::PI {
                    dy += std::f32::consts::TAU;
                }
                inp.look(-dy * 0.25, 0.0);
                inp.mv = if d.len() > 1.4 { v2(0.0, -1.0) } else { v2(0.0, 0.0) };
                if d.len() < 2.2 && (t * 30.0) as i32 % 9 == 0 {
                    inp.button(BTN_ATTACK, true);
                }
            } else {
                inp.mv = v2(0.0, -0.6);
                inp.look(0.01, 0.0);
            }
            inp.button(BTN_BLOCK, (t % 9.0) > 8.0);
            drop(inp);
            if let Some(g) = lock(&ENGINE).game.as_mut() {
                g.player.hp = g.player.hp.max(40.0);
            }
        }
        lock(&SETTINGS).map_expanded = (28.5..29.5).contains(&t);
        let t0 = Instant::now();
        engine::frame(dt);
        unsafe {
            let mut px = [0u8; 4];
            glReadPixels(0, 0, 1, 1, RGBA, UNSIGNED_BYTE, px.as_mut_ptr() as *mut _);
        }
        ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        t += dt;
        if next < shots.len() && t >= shots[next] {
            write_ppm(&format!("{out}/shot_{next}.ppm"));
            next += 1;
        }
    }
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let g = lock(&ENGINE);
    let game = g.game.as_ref().unwrap();
    println!("frames={} median_ms={:.1} (CPU raster) wave={} kills={} combo={} alive={}", ms.len(), ms[ms.len() / 2], game.wave, game.kills, game.max_combo, game.alive_count());
}

/// Fixed camera positions for visual review (no zombies).
fn tour(out: &str) {
    let views: [(f32, f32, f32, f32); 6] = [
        (0.0, -2.0, 0.0, 0.05),
        (-3.0, 12.0, -1.2, 0.35),
        (4.0, 20.0, 0.9, 0.25),
        (0.0, 30.0, 0.0, 0.0),
        (-12.0, 0.0, -1.57, 0.0),
        (0.0, -40.0, 3.14, 0.1),
    ];
    for _ in 0..3 {
        engine::frame(1.0 / 30.0);
    }
    for (i, (x, z, yaw, pitch)) in views.iter().enumerate() {
        if let Some(g) = lock(&ENGINE).game.as_mut() {
            g.zombies.clear();
            g.player.pos = v2(*x, *z);
            g.player.yaw = *yaw;
            g.player.pitch = *pitch;
        }
        for _ in 0..3 {
            engine::frame(1.0 / 30.0);
        }
        write_ppm(&format!("{out}/tour_{i}.ppm"));
    }
}

unsafe fn init_egl() {
    let get_platform: extern "C" fn(u32, *mut c_void, *const i32) -> *mut c_void = std::mem::transmute(eglGetProcAddress(b"eglGetPlatformDisplayEXT\0".as_ptr() as *const _));
    let attrs = [0x3203, 0x3450, 0x3209, 0x3487, 0x348F, 0x31DD, 0x3038];
    let d = get_platform(0x3202, std::ptr::null_mut(), attrs.as_ptr());
    let (mut a, mut b) = (0, 0);
    assert!(eglInitialize(d, &mut a, &mut b) != 0, "eglInitialize {:x}", eglGetError());
    eglBindAPI(0x30A0);
    let cfg_attrs = [0x3033, 0x0001, 0x3040, 0x0040, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3025, 24, 0x3038];
    let mut cfg = std::ptr::null_mut();
    let mut n = 0;
    assert!(eglChooseConfig(d, cfg_attrs.as_ptr(), &mut cfg, 1, &mut n) != 0 && n > 0, "no config");
    let surf = eglCreatePbufferSurface(d, cfg, [0x3057, W, 0x3056, H, 0x3038].as_ptr());
    let ctx = eglCreateContext(d, cfg, std::ptr::null_mut(), [0x3098, 3, 0x3038].as_ptr());
    assert!(eglMakeCurrent(d, surf, surf, ctx) != 0, "make current {:x}", eglGetError());
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

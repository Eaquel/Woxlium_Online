//! Platform-independent glue: assets, game + renderer lifetime, input latching
//! and the HUD snapshot shared with the UI thread.

use std::sync::Mutex;

use crate::assets::AssetSource;
use crate::game::{ArmClips, Clips, Game, Input, Mode, ZombieClips, ARM_CLIPS, HUD_LEN, ZOMBIE_CLIPS};
use crate::level::Level;
use crate::math::{v2, V2};
use crate::render::Renderer;

pub const BTN_ATTACK: i32 = 0;
pub const BTN_DODGE: i32 = 2;
pub const BTN_BLOCK: i32 = 4;

#[derive(Default)]
pub struct SharedInput {
    pub mv: V2,
    look: V2,
    attack: u32,
    dodge: u32,
    pub block: bool,
}

impl SharedInput {
    pub fn button(&mut self, id: i32, down: bool) {
        match id {
            BTN_BLOCK => self.block = down,
            BTN_ATTACK if down => self.attack += 1,
            BTN_DODGE if down => self.dodge += 1,
            _ => {}
        }
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        self.look = self.look + v2(dx, dy);
    }

    fn latch(&mut self) -> Input {
        let i = Input { mv: self.mv, look: self.look, attack: self.attack > 0, dodge: self.dodge > 0, block: self.block };
        self.look = V2::ZERO;
        self.attack = self.attack.saturating_sub(1).min(1);
        self.dodge = 0;
        i
    }
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub minimap: [f32; 3],
    pub map_expanded: bool,
    pub shadows: bool,
}

pub struct Engine {
    pub source: Option<Box<dyn AssetSource>>,
    pub game: Option<Game>,
    pub renderer: Option<Renderer>,
    pending: Option<(Mode, u32)>,
    pub paused: bool,
    fps_acc: f32,
    fps_frames: u32,
    pub fps: f32,
    pub error: Option<String>,
}

pub static ENGINE: Mutex<Engine> = Mutex::new(Engine {
    source: None,
    game: None,
    renderer: None,
    pending: None,
    paused: false,
    fps_acc: 0.0,
    fps_frames: 0,
    fps: 60.0,
    error: None,
});
pub static INPUT: Mutex<SharedInput> = Mutex::new(SharedInput { mv: v2(0.0, 0.0), look: v2(0.0, 0.0), attack: 0, dodge: 0, block: false });
pub static HUD: Mutex<[f32; HUD_LEN]> = Mutex::new([0.0; HUD_LEN]);
pub static SETTINGS: Mutex<Settings> = Mutex::new(Settings { minimap: [0.0; 3], map_expanded: false, shadows: true });

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_source(src: Box<dyn AssetSource>) {
    lock(&ENGINE).source = Some(src);
}

/// Request a new game (any thread). It is created on the GL thread once the
/// models are loaded.
pub fn start_game(mode: i32, seed: u32) {
    let mode = if mode == 1 { Mode::Story } else { Mode::Endless };
    let mut e = lock(&ENGINE);
    e.pending = Some((mode, seed));
    e.paused = false;
    drop(e);
    *lock(&INPUT) = SharedInput::default();
    lock(&SETTINGS).map_expanded = false;
}

pub fn surface_created() {
    let mut e = lock(&ENGINE);
    let Some(src) = e.source.as_ref() else {
        e.error = Some("no asset source".into());
        return;
    };
    match Renderer::new(src.as_ref()) {
        Ok(mut r) => {
            if let Some(g) = e.game.as_ref() {
                r.build_level(&g.level.blocks);
            }
            e.renderer = Some(r);
        }
        Err(err) => {
            crate::log(&format!("renderer init failed: {err}"));
            e.error = Some(err);
        }
    }
}

pub fn surface_changed(w: i32, h: i32) {
    if let Some(r) = lock(&ENGINE).renderer.as_mut() {
        r.resize(w, h);
    }
}

fn clip_set<const N: usize>(r: &Renderer, model: usize, names: [&str; N]) -> Clips<N> {
    let m = &r.models[model];
    let mut c = Clips { id: [0; N], dur: [1.0; N] };
    for (k, n) in names.iter().enumerate() {
        if let Some(i) = m.clip(n) {
            c.id[k] = i;
            c.dur[k] = m.clips[i].duration();
        }
    }
    c
}

fn create_game(r: &mut Renderer, mode: Mode, seed: u32) -> Game {
    let level = Level::build(&|n| r.bounds(n));
    r.build_level(&level.blocks);
    let zc: ZombieClips = clip_set(r, r.zombie_models[0], ZOMBIE_CLIPS);
    let arms = r.model("player_arms.wxm").unwrap_or(0);
    let ac: ArmClips = clip_set(r, arms, ARM_CLIPS);
    Game::new(mode, seed, level, zc, ac, r.zombie_models.len())
}

/// Advance + draw one frame on the GL thread. Returns EV_* bits.
pub fn frame(dt: f32) -> u32 {
    let input = lock(&INPUT).latch();
    let settings = *lock(&SETTINGS);
    let mut guard = lock(&ENGINE);
    let e = &mut *guard;
    let Some(r) = e.renderer.as_mut() else { return 0 };
    if let Some((mode, seed)) = e.pending.take() {
        e.game = Some(create_game(r, mode, seed));
    }
    let Some(game) = e.game.as_mut() else { return 0 };
    let dt = dt.clamp(0.0, 0.1);
    e.fps_acc += dt;
    e.fps_frames += 1;
    if e.fps_acc >= 0.5 {
        e.fps = e.fps_frames as f32 / e.fps_acc;
        e.fps_acc = 0.0;
        e.fps_frames = 0;
    }
    let mut events = 0;
    if !e.paused {
        let mut left = dt;
        let mut first = true;
        while left > 1e-4 {
            let h = left.min(1.0 / 60.0);
            let mut inp = input;
            if !first {
                inp.look = V2::ZERO;
                inp.attack = false;
                inp.dodge = false;
            }
            game.step(h, &inp);
            first = false;
            left -= h;
        }
        events = game.take_events();
    }
    r.minimap = settings.minimap;
    r.map_expanded = settings.map_expanded;
    r.shadows = settings.shadows;
    r.render(game, if e.paused { 0.0 } else { dt });
    *lock(&HUD) = game.hud(e.fps);
    events
}

pub fn set_paused(p: bool) {
    lock(&ENGINE).paused = p;
}

//! Platform-independent glue: owns the game + renderer, frame pacing, input
//! latching and the HUD snapshot shared with the UI thread.

use std::sync::Mutex;

use crate::game::{Game, Input, Mode, HUD_LEN};
use crate::math::{v2, V2};
use crate::render::{Renderer, Textures, TEX_SLOTS};

pub const BTN_ATTACK: i32 = 0;
pub const BTN_KICK: i32 = 1;
pub const BTN_DODGE: i32 = 2;
pub const BTN_HEAL: i32 = 3;
pub const BTN_BLOCK: i32 = 4;

/// Input written by the UI thread, consumed once per frame by the GL thread.
#[derive(Default)]
pub struct SharedInput {
    pub mv: V2,
    presses: [u32; 4],
    pub block: bool,
}

impl SharedInput {
    pub fn button(&mut self, id: i32, down: bool) {
        match id {
            BTN_BLOCK => self.block = down,
            0..=3 if down => self.presses[id as usize] += 1,
            _ => {}
        }
    }

    fn latch(&mut self) -> Input {
        let i = Input {
            mv: self.mv,
            attack: self.presses[0] > 0,
            kick: self.presses[1] > 0,
            dodge: self.presses[2] > 0,
            heal: self.presses[3] > 0,
            block: self.block,
        };
        // one press per frame; extra taps stay queued (max 2) for combo buffering
        for p in &mut self.presses {
            *p = p.saturating_sub(1).min(1);
        }
        i
    }
}

pub struct Engine {
    pub game: Game,
    pub renderer: Option<Renderer>,
    pub paused: bool,
    fps_acc: f32,
    fps_frames: u32,
    pub fps: f32,
}

pub static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
pub static INPUT: Mutex<SharedInput> = Mutex::new(SharedInput { mv: v2(0.0, 0.0), presses: [0; 4], block: false });
pub static HUD: Mutex<[f32; HUD_LEN]> = Mutex::new([0.0; HUD_LEN]);
pub static SETTINGS: Mutex<Settings> = Mutex::new(Settings { zoom: 1.0, minimap: [0.0; 3], map_expanded: false });

#[derive(Clone, Copy)]
pub struct Settings {
    pub zoom: f32,
    pub minimap: [f32; 3],
    pub map_expanded: bool,
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn start_game(mode: i32, skin: i32, seed: u32) {
    let mode = if mode == 1 { Mode::Story } else { Mode::Endless };
    let game = Game::new(mode, skin as u8, seed);
    let mut e = lock(&ENGINE);
    match e.as_mut() {
        Some(engine) => {
            engine.game = game;
            engine.paused = false;
            // called from the UI thread: GL work is deferred to the next frame
            if let Some(r) = engine.renderer.as_mut() {
                r.reset_pending = true;
            }
        }
        None => *e = Some(Engine { game, renderer: None, paused: false, fps_acc: 0.0, fps_frames: 0, fps: 60.0 }),
    }
    drop(e);
    *lock(&INPUT) = SharedInput::default();
    lock(&SETTINGS).map_expanded = false;
}

/// GL context (re)created: textures are fresh GL names.
pub fn surface_created(slots: &[u32; TEX_SLOTS]) {
    let mut e = lock(&ENGINE);
    if e.is_none() {
        drop(e);
        start_game(0, 0, 1);
        e = lock(&ENGINE);
    }
    let engine = e.as_mut().unwrap();
    engine.renderer = Some(Renderer::new(Textures::from_slots(slots), &engine.game));
}

pub fn surface_changed(w: i32, h: i32) {
    if let Some(r) = lock(&ENGINE).as_mut().and_then(|e| e.renderer.as_mut()) {
        r.resize(w, h);
    }
}

/// Advance + draw one frame. Returns the event bit flags for audio/haptics.
pub fn frame(dt: f32) -> u32 {
    let input = lock(&INPUT).latch();
    let settings = *lock(&SETTINGS);
    let mut guard = lock(&ENGINE);
    let Some(engine) = guard.as_mut() else { return 0 };
    let dt = dt.clamp(0.0, 0.1);
    engine.fps_acc += dt;
    engine.fps_frames += 1;
    if engine.fps_acc >= 0.5 {
        engine.fps = engine.fps_frames as f32 / engine.fps_acc;
        engine.fps_acc = 0.0;
        engine.fps_frames = 0;
    }
    let mut events = 0;
    if !engine.paused {
        // fixed sub-steps keep combat timing identical at 30/60/120 Hz
        let mut left = dt;
        while left > 1e-4 {
            let h = left.min(1.0 / 60.0);
            engine.game.step(h, &input);
            left -= h;
        }
        events = engine.game.take_events();
    }
    if let Some(r) = engine.renderer.as_mut() {
        r.zoom = settings.zoom;
        r.minimap = settings.minimap;
        r.map_expanded = settings.map_expanded;
        r.render(&engine.game, if engine.paused { 0.0 } else { dt });
    }
    *lock(&HUD) = engine.game.hud(engine.fps);
    events
}

pub fn set_paused(p: bool) {
    if let Some(e) = lock(&ENGINE).as_mut() {
        e.paused = p;
    }
}

//! Gameplay simulation: player combat, zombie AI, waves, effects.
//! Pure logic (no GL) so it is unit-testable on the host.

use crate::atlas_gen::*;
use crate::math::{clamp, v2, v3, Rng, V2, V3};
use crate::world::{Mat, World};

pub const PLAYER_HEIGHT: f32 = 1.85;
pub const PLAYER_RADIUS: f32 = 0.38;
pub const ZOMBIE_RADIUS: f32 = 0.36;
pub const STORY_WAVES: u32 = 5;
const MAX_ALIVE: usize = 36;
const MAX_FX: usize = 220;
const MAX_DECALS: usize = 70;

// ---- events (bit flags handed to the platform layer for audio / haptics)
pub const EV_SWING: u32 = 1 << 0;
pub const EV_HIT: u32 = 1 << 1;
pub const EV_KICK_HIT: u32 = 1 << 2;
pub const EV_CRIT: u32 = 1 << 3;
pub const EV_ZOMBIE_DIE: u32 = 1 << 4;
pub const EV_PLAYER_HURT: u32 = 1 << 5;
pub const EV_BLOCK: u32 = 1 << 6;
pub const EV_DODGE: u32 = 1 << 7;
pub const EV_HEAL: u32 = 1 << 8;
pub const EV_WAVE_START: u32 = 1 << 9;
pub const EV_WAVE_CLEAR: u32 = 1 << 10;
pub const EV_GROWL: u32 = 1 << 11;
pub const EV_PICKUP: u32 = 1 << 12;
pub const EV_NO_STAMINA: u32 = 1 << 13;
pub const EV_GAME_OVER: u32 = 1 << 14;
pub const EV_VICTORY: u32 = 1 << 15;
pub const EV_EXPLODE: u32 = 1 << 16;

// ---- HUD layout shared with Kotlin (NativeBridge.HUD_*)
pub const HUD_LEN: usize = 20;
pub const HUD_HP: usize = 0;
pub const HUD_HP_MAX: usize = 1;
pub const HUD_STAMINA: usize = 2;
pub const HUD_STAMINA_MAX: usize = 3;
pub const HUD_MEDKITS: usize = 4;
pub const HUD_WAVE: usize = 5;
pub const HUD_ENEMIES: usize = 6;
pub const HUD_KILLS: usize = 7;
pub const HUD_SCORE: usize = 8;
pub const HUD_COMBO: usize = 9;
pub const HUD_STATE: usize = 10;
pub const HUD_BREAK: usize = 11;
pub const HUD_OBJECTIVE: usize = 12;
pub const HUD_KICK_KILLS: usize = 13;
pub const HUD_MAX_COMBO: usize = 14;
pub const HUD_TIME: usize = 15;
pub const HUD_EXTRACT_DIST: usize = 16;
pub const HUD_STORY_WAVES: usize = 17;
pub const HUD_FPS: usize = 18;
pub const HUD_MODE: usize = 19;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Endless,
    Story,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    Break,
    GameOver,
    Victory,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub mv: V2,
    pub attack: bool,
    pub kick: bool,
    pub dodge: bool,
    pub heal: bool,
    pub block: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PState {
    Free,
    Punch { step: u8, t: f32, hit: bool },
    Kick { t: f32, hit: bool },
    Dodge { t: f32, dir: V2 },
    Hurt { t: f32 },
    Dead { t: f32 },
}

pub struct Player {
    pub pos: V2,
    pub vel: V2,
    pub facing: V2,
    pub hp: f32,
    pub hp_max: f32,
    pub stamina: f32,
    pub stamina_max: f32,
    pub medkits: u32,
    pub skin: u8,
    state: PState,
    invuln: f32,
    stamina_delay: f32,
    combo_window: f32,
    attack_buffer: f32,
    next_step: u8,
    heal_cd: f32,
    kick_cd: f32,
    step_t: f32,
    sweat_t: f32,
    walk_phase: f32,
    flash: f32,
    pub blocking: bool,
    shield_fx: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZKind {
    Walker,
    Runner,
    Lunger,
    Bloater,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ZState {
    Chase,
    Windup { t: f32 },
    Recover { t: f32 },
    LungeWind { t: f32, dir: V2 },
    Lunge { t: f32, dir: V2, hit: bool },
    Stagger { t: f32 },
    Dying { t: f32 },
    Corpse { t: f32 },
}

pub struct Zombie {
    pub kind: ZKind,
    pub pos: V2,
    vel: V2,
    pub facing: V2,
    pub hp: f32,
    pub hp_max: f32,
    speed: f32,
    damage: f32,
    state: ZState,
    anim_t: f32,
    flash: f32,
    lunge_cd: f32,
    growl_t: f32,
    show_bar: f32,
}

impl Zombie {
    pub fn alive(&self) -> bool {
        !matches!(self.state, ZState::Dying { .. } | ZState::Corpse { .. })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pickup {
    pub pos: V2,
    pub t: f32,
}

#[derive(Clone, Copy, Debug)]
struct Fx {
    anim: (usize, usize),
    pos: V3,
    vel: V3,
    t: f32,
    dur: f32,
    size: f32,
    color: [f32; 3],
    alpha: f32,
    additive: bool,
    mirror: bool,
    follow_player: bool,
}

#[derive(Clone, Copy, Debug)]
struct Decal {
    anim: (usize, usize),
    pos: V2,
    t: f32,
    grow: f32,
    life: f32,
    size: f32,
    rot: f32,
}

/// Renderable produced by the simulation; the renderer only batches these.
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub pos: V3,
    pub frame: u16,
    pub w: f32,
    pub h: f32,
    /// anchor inside the quad: (0,0) = bottom-left, (1,1) = top-right
    pub ax: f32,
    pub ay: f32,
    pub mirror: bool,
    /// premultiplied tint: rgb * opacity, a = opacity (0 for additive)
    pub color: [f32; 4],
    pub flash: f32,
    /// Some(rotation) = lies flat on the ground
    pub ground: Option<f32>,
    /// x-ray silhouette when hidden behind walls (player only)
    pub xray: bool,
}

pub struct Game {
    pub world: World,
    pub player: Player,
    pub zombies: Vec<Zombie>,
    pub pickups: Vec<Pickup>,
    fx: Vec<Fx>,
    decals: Vec<Decal>,
    pub mode: Mode,
    pub phase: Phase,
    pub wave: u32,
    pending: u32,
    spawn_t: f32,
    break_t: f32,
    pub kills: u32,
    pub kick_kills: u32,
    pub score: u32,
    pub combo: u32,
    pub max_combo: u32,
    combo_t: f32,
    pub time: f32,
    pub shake: f32,
    hitstop: f32,
    end_t: f32,
    events: u32,
    rng: Rng,
    flow_t: f32,
    pub extraction_active: bool,
}

impl Game {
    pub fn new(mode: Mode, skin: u8, seed: u32) -> Game {
        let world = World::generate(1337);
        let start = world.player_start;
        let mut g = Game {
            player: Player {
                pos: start,
                vel: V2::ZERO,
                facing: v2(0.0, 1.0),
                hp: 100.0,
                hp_max: 100.0,
                stamina: 100.0,
                stamina_max: 100.0,
                medkits: 2,
                skin,
                state: PState::Free,
                invuln: 0.0,
                stamina_delay: 0.0,
                combo_window: 0.0,
                attack_buffer: 0.0,
                next_step: 0,
                heal_cd: 0.0,
                kick_cd: 0.0,
                step_t: 0.0,
                sweat_t: 0.0,
                walk_phase: 0.0,
                flash: 0.0,
                blocking: false,
                shield_fx: 0.0,
            },
            world,
            zombies: Vec::with_capacity(64),
            pickups: Vec::new(),
            fx: Vec::with_capacity(MAX_FX),
            decals: Vec::with_capacity(MAX_DECALS),
            mode,
            phase: Phase::Break,
            wave: 0,
            pending: 0,
            spawn_t: 0.0,
            break_t: 2.5,
            kills: 0,
            kick_kills: 0,
            score: 0,
            combo: 0,
            max_combo: 0,
            combo_t: 0.0,
            time: 0.0,
            shake: 0.0,
            hitstop: 0.0,
            end_t: 0.0,
            events: 0,
            rng: Rng::new(seed ^ 0x9E37_79B9),
            flow_t: 0.0,
            extraction_active: false,
        };
        g.world.update_flow(start);
        g
    }

    pub fn take_events(&mut self) -> u32 {
        std::mem::take(&mut self.events)
    }

    pub fn alive_count(&self) -> usize {
        self.zombies.iter().filter(|z| z.alive()).count()
    }

    // ------------------------------------------------------------------ step
    pub fn step(&mut self, dt: f32, input: &Input) {
        let dt = dt.min(1.0 / 20.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        if self.hitstop > 0.0 {
            self.hitstop -= dt;
            // still accept the buffered attack so combos feel responsive
            if input.attack {
                self.player.attack_buffer = 0.3;
            }
            return;
        }
        self.time += dt;
        self.update_waves(dt);
        self.update_player(dt, input);
        self.flow_t -= dt;
        if self.flow_t <= 0.0 {
            self.flow_t = 0.2;
            self.world.update_flow(self.player.pos);
        }
        self.update_zombies(dt);
        self.update_pickups(dt);
        self.update_fx(dt);
        if self.combo_t > 0.0 {
            self.combo_t -= dt;
            if self.combo_t <= 0.0 {
                self.combo = 0;
            }
        }
        if matches!(self.phase, Phase::GameOver | Phase::Victory) {
            self.end_t += dt;
        }
    }

    fn update_waves(&mut self, dt: f32) {
        match self.phase {
            Phase::Break => {
                self.break_t -= dt;
                if self.break_t <= 0.0 {
                    self.wave += 1;
                    self.pending = 4 + self.wave * 3;
                    if self.mode == Mode::Story {
                        self.pending = [6, 10, 14, 18, 24][(self.wave as usize - 1).min(4)];
                    }
                    self.spawn_t = 0.0;
                    self.phase = Phase::Playing;
                    self.events |= EV_WAVE_START;
                }
            }
            Phase::Playing => {
                if self.pending > 0 {
                    self.spawn_t -= dt;
                    if self.spawn_t <= 0.0 && self.alive_count() < MAX_ALIVE {
                        self.spawn_t = (0.9 - self.wave as f32 * 0.05).max(0.3);
                        self.spawn_one();
                        self.pending -= 1;
                    }
                } else if self.alive_count() == 0 && !self.extraction_active {
                    self.events |= EV_WAVE_CLEAR;
                    if self.mode == Mode::Story && self.wave >= STORY_WAVES {
                        self.extraction_active = true;
                    } else {
                        self.phase = Phase::Break;
                        self.break_t = 5.0;
                        // reward: some stamina and a breather
                        self.player.hp = (self.player.hp + 15.0).min(self.player.hp_max);
                    }
                }
                if self.extraction_active && (self.player.pos - self.world.extraction).len() < 3.0 {
                    self.phase = Phase::Victory;
                    self.events |= EV_VICTORY;
                }
            }
            _ => {}
        }
    }

    fn spawn_one(&mut self) {
        let w = self.wave;
        let roll = self.rng.f();
        let kind = if w >= 4 && roll < 0.12 {
            ZKind::Bloater
        } else if w >= 3 && roll < 0.3 {
            ZKind::Lunger
        } else if w >= 2 && roll < 0.55 {
            ZKind::Runner
        } else {
            ZKind::Walker
        };
        let pos = self.world.spawn_point(&mut self.rng, self.player.pos, 16.0, 30.0);
        let scale = 1.0 + 0.08 * (w.saturating_sub(1)) as f32;
        let (hp, speed, damage) = match kind {
            ZKind::Walker => (55.0, 1.35, 9.0),
            ZKind::Runner => (40.0, 3.5, 7.0),
            ZKind::Lunger => (85.0, 1.7, 15.0),
            ZKind::Bloater => (120.0, 1.05, 12.0),
        };
        let speed = speed * (1.0 + 0.025 * w.min(12) as f32) * self.rng.range(0.9, 1.1);
        self.zombies.push(Zombie {
            kind,
            pos,
            vel: V2::ZERO,
            facing: (self.player.pos - pos).norm(),
            hp: hp * scale,
            hp_max: hp * scale,
            speed,
            damage: damage * (1.0 + 0.04 * w as f32),
            state: ZState::Chase,
            anim_t: self.rng.range(0.0, 2.0),
            flash: 0.0,
            lunge_cd: 2.0,
            growl_t: self.rng.range(2.0, 6.0),
            show_bar: 0.0,
        });
    }

    // ---------------------------------------------------------------- player
    fn update_player(&mut self, dt: f32, input: &Input) {
        let p = &mut self.player;
        p.invuln = (p.invuln - dt).max(0.0);
        p.flash = (p.flash - dt * 4.0).max(0.0);
        p.heal_cd = (p.heal_cd - dt).max(0.0);
        p.kick_cd = (p.kick_cd - dt).max(0.0);
        p.combo_window = (p.combo_window - dt).max(0.0);
        p.attack_buffer = (p.attack_buffer - dt).max(0.0);
        p.shield_fx = (p.shield_fx - dt).max(0.0);
        if input.attack {
            p.attack_buffer = 0.3;
        }

        if let PState::Dead { t } = &mut p.state {
            *t += dt;
            p.vel = p.vel * (1.0 - dt * 6.0).max(0.0);
            p.pos = self.world.collide(p.pos + p.vel * dt, PLAYER_RADIUS);
            return;
        }

        // stamina
        p.stamina_delay = (p.stamina_delay - dt).max(0.0);
        p.blocking = input.block && matches!(p.state, PState::Free) && p.stamina > 1.0;
        if p.blocking {
            p.stamina = (p.stamina - 9.0 * dt).max(0.0);
            p.stamina_delay = 0.4;
        } else if p.stamina_delay <= 0.0 {
            p.stamina = (p.stamina + 26.0 * dt).min(p.stamina_max);
        }

        let mv = if input.mv.len() > 1.0 { input.mv.norm() } else { input.mv };
        let moving = mv.len() > 0.12;

        match p.state {
            PState::Free => {
                let speed = if p.blocking { 1.8 } else { 4.7 };
                let target = mv * speed;
                p.vel = p.vel.lerp(target, clamp(dt * 14.0, 0.0, 1.0));
                if moving {
                    p.facing = p.facing.lerp(mv.norm(), clamp(dt * 16.0, 0.0, 1.0)).norm();
                }
                // actions, highest priority first
                if input.dodge {
                    self.try_dodge(mv);
                } else if input.heal {
                    self.try_heal();
                } else if input.kick {
                    self.try_kick();
                } else if self.player.attack_buffer > 0.0 {
                    self.try_punch();
                }
            }
            PState::Punch { step, t, hit } => {
                let t = t + dt;
                p.vel = p.vel.lerp(mv * 1.2, clamp(dt * 10.0, 0.0, 1.0));
                let dur = if step == 2 { 0.42 } else { 0.30 };
                let hit_at = if step == 2 { 0.16 } else { 0.11 };
                let mut hit = hit;
                if !hit && t >= hit_at {
                    hit = true;
                    let (dmg, knock, arc, range) = match step {
                        0 => (18.0, 2.5, 0.25, 1.75),
                        1 => (22.0, 3.0, 0.25, 1.75),
                        _ => (36.0, 6.5, 0.0, 1.95),
                    };
                    self.player.state = PState::Punch { step, t, hit };
                    self.resolve_hits(dmg, knock, arc, range, false, step == 2);
                }
                let p = &mut self.player;
                if let PState::Punch { .. } = p.state {
                    p.state = PState::Punch { step, t, hit };
                }
                if t >= dur {
                    p.state = PState::Free;
                    p.combo_window = 0.45;
                    p.next_step = (step + 1) % 3;
                    if p.attack_buffer > 0.0 {
                        self.try_punch();
                    }
                } else if t > dur * 0.55 && (input.dodge) {
                    // dodge-cancel the recovery
                    p.state = PState::Free;
                    self.try_dodge(mv);
                }
            }
            PState::Kick { t, hit } => {
                let t = t + dt;
                p.vel = p.vel * (1.0 - dt * 8.0).max(0.0);
                let mut hit = hit;
                if !hit && t >= 0.2 {
                    hit = true;
                    self.player.state = PState::Kick { t, hit };
                    self.resolve_hits(28.0, 9.0, -0.1, 2.1, true, false);
                    let fpos = self.player.pos + self.player.facing * 0.9;
                    self.spawn_fx(V_DUST, v3(fpos.x, 0.3, fpos.y), 1.4, 0.5, [0.85, 0.8, 0.72], 0.9, false);
                }
                let p = &mut self.player;
                p.state = if t >= 0.45 { PState::Free } else { PState::Kick { t, hit } };
            }
            PState::Dodge { t, dir } => {
                let t = t + dt;
                let k = 1.0 - t / 0.32;
                p.vel = dir * (13.0 * k.max(0.25));
                if t >= 0.32 {
                    p.state = PState::Free;
                    p.vel = dir * 3.0;
                } else {
                    p.state = PState::Dodge { t, dir };
                }
            }
            PState::Hurt { t } => {
                let t = t + dt;
                p.vel = p.vel * (1.0 - dt * 7.0).max(0.0);
                p.state = if t >= 0.22 { PState::Free } else { PState::Hurt { t } };
            }
            PState::Dead { .. } => {}
        }

        let p = &mut self.player;
        let new_pos = self.world.collide(p.pos + p.vel * dt, PLAYER_RADIUS);
        let speed = p.vel.len();
        p.pos = new_pos;
        p.walk_phase += speed * dt * 2.2;

        // footstep dust on soft ground
        if speed > 3.0 {
            p.step_t -= dt;
            if p.step_t <= 0.0 {
                p.step_t = 0.32;
                let m = self.world.material_at(p.pos);
                if m == Mat::Dirt || m == Mat::Mud || matches!(p.state, PState::Dodge { .. }) {
                    let pos = p.pos;
                    let tint = if m == Mat::Mud { [0.55, 0.47, 0.38] } else { [0.85, 0.78, 0.66] };
                    self.spawn_fx(V_DUST, v3(pos.x, 0.15, pos.y), 0.8, 0.45, tint, 0.55, false);
                }
            }
        }
        // exhaustion feedback
        let p = &mut self.player;
        if p.stamina < 28.0 {
            p.sweat_t -= dt;
            if p.sweat_t <= 0.0 {
                p.sweat_t = 0.55;
                let pos = p.pos;
                self.spawn_fx(V_SWEAT, v3(pos.x, 1.75, pos.y), 0.7, 0.5, [0.9, 0.95, 1.0], 0.85, true);
            }
        }
        if self.player.blocking && self.player.shield_fx <= 0.0 {
            self.player.shield_fx = 0.35;
            let f = self.player.pos + self.player.facing * 0.45;
            self.spawn_fx_follow(V_SHIELD, v3(f.x, 1.1, f.y), 1.25, 0.4, [0.6, 0.8, 1.0], 0.35);
        }
    }

    fn spend(&mut self, cost: f32) -> bool {
        let p = &mut self.player;
        if p.stamina < cost {
            let pos = p.pos;
            self.events |= EV_NO_STAMINA;
            self.spawn_fx(V_SMOKE, v3(pos.x, 1.3, pos.y), 1.2, 0.7, [0.5, 0.5, 0.5], 0.75, false);
            return false;
        }
        p.stamina -= cost;
        p.stamina_delay = 0.55;
        true
    }

    fn try_punch(&mut self) {
        if !self.spend(7.0) {
            self.player.attack_buffer = 0.0;
            return;
        }
        let step = if self.player.combo_window > 0.0 { self.player.next_step } else { 0 };
        self.player.attack_buffer = 0.0;
        self.auto_aim(3.2);
        self.player.state = PState::Punch { step, t: 0.0, hit: false };
        self.events |= EV_SWING;
        let f = self.player.facing;
        let pos = self.player.pos + f * 0.8;
        self.spawn_fx_dir(V_SLASH, v3(pos.x, 1.25, pos.y), 1.1, 0.22, [1.0, 1.0, 1.0], 0.55, f.x < 0.0);
    }

    fn try_kick(&mut self) {
        if self.player.kick_cd > 0.0 || !self.spend(16.0) {
            return;
        }
        self.player.kick_cd = 0.7;
        self.auto_aim(3.0);
        self.player.state = PState::Kick { t: 0.0, hit: false };
        self.events |= EV_SWING;
    }

    fn try_dodge(&mut self, mv: V2) {
        if !self.spend(22.0) {
            return;
        }
        let dir = if mv.len() > 0.2 { mv.norm() } else { -self.player.facing };
        self.player.state = PState::Dodge { t: 0.0, dir };
        self.player.invuln = 0.36;
        self.events |= EV_DODGE;
        let pos = self.player.pos;
        self.spawn_fx(V_DUST, v3(pos.x, 0.2, pos.y), 1.3, 0.5, [0.8, 0.75, 0.65], 0.8, false);
    }

    fn try_heal(&mut self) {
        let p = &mut self.player;
        if p.medkits == 0 || p.heal_cd > 0.0 || p.hp >= p.hp_max {
            return;
        }
        p.medkits -= 1;
        p.heal_cd = 1.0;
        p.hp = (p.hp + 45.0).min(p.hp_max);
        self.events |= EV_HEAL;
        let pos = p.pos;
        self.spawn_fx_follow(V_HEAL, v3(pos.x, 1.0, pos.y), 2.0, 0.9, [0.7, 1.0, 0.7], 1.0);
    }

    fn auto_aim(&mut self, radius: f32) {
        let p = &self.player;
        let mut best: Option<(f32, V2)> = None;
        for z in self.zombies.iter().filter(|z| z.alive()) {
            let d = z.pos - p.pos;
            let l = d.len();
            if l < radius {
                // prefer targets roughly in front, but allow turning around
                let score = l - d.norm().dot(p.facing) * 0.8;
                if best.map_or(true, |(s, _)| score < s) {
                    best = Some((score, d));
                }
            }
        }
        if let Some((_, d)) = best {
            let l = d.len();
            self.player.facing = d.norm();
            if l > 1.25 {
                // short magnetic step towards the target
                self.player.vel = d.norm() * ((l - 1.1) * 6.0).min(6.5);
            }
        }
    }

    fn resolve_hits(&mut self, dmg: f32, knock: f32, arc: f32, range: f32, kick: bool, finisher: bool) {
        let ppos = self.player.pos;
        let facing = self.player.facing;
        let mut any = false;
        let mut crit_any = false;
        let n = self.zombies.len();
        for i in 0..n {
            let z = &self.zombies[i];
            if !z.alive() {
                continue;
            }
            let d = z.pos - ppos;
            let l = d.len();
            if l > range + ZOMBIE_RADIUS || d.norm().dot(facing) < arc {
                continue;
            }
            let crit = self.rng.chance(0.12) || (finisher && self.combo >= 6);
            let dmg = if crit { dmg * 2.0 } else { dmg } * self.rng.range(0.9, 1.1);
            crit_any |= crit;
            any = true;
            let dir = if l > 1e-3 { d * (1.0 / l) } else { facing };
            self.damage_zombie(i, dmg, dir * knock, kick);
            let hp = ppos + dir * (l * 0.6);
            let hit_pos = v3(hp.x, 1.25, hp.y);
            self.spawn_fx(V_SPARK, hit_pos, if crit { 1.4 } else { 0.9 }, 0.28, [1.0, 0.95, 0.8], 1.0, true);
            self.spawn_fx_dir(V_BLOOD, hit_pos, 1.0, 0.4, [1.0, 1.0, 1.0], 0.95, dir.x < 0.0);
            if crit || finisher {
                self.spawn_fx(V_SHOCK, hit_pos, if crit { 2.4 } else { 1.7 }, 0.35, [1.0, 0.95, 0.75], 0.9, true);
            }
        }
        if any {
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
            self.combo_t = 2.5;
            self.hitstop = if crit_any { 0.085 } else { 0.045 };
            self.shake = self.shake.max(if crit_any || finisher { 0.45 } else { 0.22 });
            self.events |= if kick { EV_KICK_HIT } else { EV_HIT };
            if crit_any {
                self.events |= EV_CRIT;
            }
        }
    }

    fn damage_zombie(&mut self, i: usize, dmg: f32, impulse: V2, kick: bool) {
        let z = &mut self.zombies[i];
        z.hp -= dmg;
        z.flash = 1.0;
        z.show_bar = 3.0;
        z.vel = impulse;
        if z.hp <= 0.0 {
            z.state = ZState::Dying { t: 0.0 };
            z.anim_t = 0.0;
            let kind = z.kind;
            let pos = z.pos;
            self.kills += 1;
            if kick {
                self.kick_kills += 1;
            }
            self.score += 100 + self.combo.min(50) * 10 + if kick { 50 } else { 0 };
            self.events |= EV_ZOMBIE_DIE;
            self.add_decal(V_POOL, pos + v2(0.3, 0.1), 1.9);
            let drop = if self.player.medkits == 0 { 0.22 } else { 0.1 };
            if self.rng.chance(drop) {
                self.pickups.push(Pickup { pos, t: 0.0 });
            }
            if kind == ZKind::Bloater {
                self.explode(pos);
            }
        } else {
            let stagger = if kick { 0.6 } else { 0.28 };
            z.state = ZState::Stagger { t: stagger };
        }
    }

    fn explode(&mut self, pos: V2) {
        self.events |= EV_EXPLODE;
        self.shake = self.shake.max(0.6);
        self.spawn_fx(V_GAS, v3(pos.x, 0.9, pos.y), 3.6, 1.3, [0.85, 1.0, 0.6], 0.9, false);
        self.spawn_fx(V_PUKE, v3(pos.x, 1.0, pos.y), 2.2, 0.7, [0.9, 1.0, 0.7], 1.0, false);
        let d = (self.player.pos - pos).len();
        if d < 2.8 && self.player.invuln <= 0.0 {
            let dir = (self.player.pos - pos).norm();
            self.hurt_player(16.0, dir, pos);
        }
        for j in 0..self.zombies.len() {
            let z = &self.zombies[j];
            if z.alive() && (z.pos - pos).len() < 2.8 {
                let dir = (z.pos - pos).norm();
                self.damage_zombie(j, 40.0, dir * 5.0, false);
            }
        }
    }

    fn hurt_player(&mut self, dmg: f32, dir: V2, from: V2) {
        let p = &mut self.player;
        if p.invuln > 0.0 || matches!(p.state, PState::Dead { .. }) {
            return;
        }
        let toward = (from - p.pos).norm();
        if p.blocking && toward.dot(p.facing) > 0.1 {
            p.hp -= dmg * 0.2;
            p.stamina = (p.stamina - 8.0).max(0.0);
            p.vel = dir * 2.0;
            p.invuln = 0.25;
            self.events |= EV_BLOCK;
            let f = p.pos + p.facing * 0.5;
            self.spawn_fx(V_SHIELD, v3(f.x, 1.1, f.y), 1.6, 0.45, [0.7, 0.85, 1.0], 1.0, true);
        } else {
            p.hp -= dmg;
            p.vel = dir * 4.0;
            p.invuln = 0.45;
            p.flash = 1.0;
            p.state = PState::Hurt { t: 0.0 };
            self.events |= EV_PLAYER_HURT;
            self.shake = self.shake.max(0.35);
            let pos = p.pos;
            self.spawn_fx_dir(V_CLAW, v3(pos.x, 1.2, pos.y), 1.3, 0.35, [1.0, 1.0, 1.0], 1.0, dir.x > 0.0);
            self.spawn_fx_dir(V_SPRAY, v3(pos.x, 1.1, pos.y), 1.4, 0.45, [1.0, 1.0, 1.0], 0.9, dir.x < 0.0);
            if self.rng.chance(0.5) {
                self.add_decal(V_BLOOD, pos, 0.9);
            }
        }
        let p = &mut self.player;
        if p.hp <= 0.0 {
            p.hp = 0.0;
            p.state = PState::Dead { t: 0.0 };
            self.phase = Phase::GameOver;
            self.events |= EV_GAME_OVER;
        }
    }

    // ---------------------------------------------------------------- zombies
    fn update_zombies(&mut self, dt: f32) {
        let ppos = self.player.pos;
        let player_dead = matches!(self.player.state, PState::Dead { .. });
        let n = self.zombies.len();
        let mut i = 0;
        let mut pending_hits: [(f32, V2, V2); 8] = [(0.0, V2::ZERO, V2::ZERO); 8];
        let mut hit_count = 0;
        while i < n {
            // separation (O(n^2) is fine for <= 36 zombies)
            let mut push = V2::ZERO;
            if self.zombies[i].alive() {
                let pi = self.zombies[i].pos;
                for j in 0..n {
                    if i == j || !self.zombies[j].alive() {
                        continue;
                    }
                    let d = pi - self.zombies[j].pos;
                    let l2 = d.len2();
                    if l2 < 0.8 * 0.8 && l2 > 1e-6 {
                        let l = l2.sqrt();
                        push += d * ((0.8 - l) / l);
                    }
                }
            }
            let flow = self.world.flow_dir(self.zombies[i].pos);
            let los = (self.zombies[i].pos - ppos).len() < 7.0 && self.world.line_of_sight(self.zombies[i].pos, ppos);

            let z = &mut self.zombies[i];
            z.anim_t += dt;
            z.flash = (z.flash - dt * 7.0).max(0.0);
            z.show_bar = (z.show_bar - dt).max(0.0);
            z.lunge_cd = (z.lunge_cd - dt).max(0.0);
            let to_p = ppos - z.pos;
            let dist = to_p.len();
            let dir_p = to_p.norm();
            let reach = 1.15;

            match z.state {
                ZState::Chase => {
                    let dir = if player_dead {
                        // wander off slowly once the player is down
                        z.facing
                    } else if los {
                        dir_p
                    } else {
                        flow.unwrap_or(dir_p)
                    };
                    let speed = if player_dead { z.speed * 0.3 } else { z.speed };
                    let desired = dir * speed + push * 3.0;
                    z.vel = z.vel.lerp(desired, clamp(dt * 6.0, 0.0, 1.0));
                    if z.vel.len() > 0.2 {
                        z.facing = z.facing.lerp(z.vel.norm(), clamp(dt * 8.0, 0.0, 1.0)).norm();
                    }
                    if !player_dead {
                        if z.kind == ZKind::Lunger && z.lunge_cd <= 0.0 && dist > 2.5 && dist < 6.0 && los {
                            z.state = ZState::LungeWind { t: 0.0, dir: dir_p };
                            z.anim_t = 0.0;
                        } else if dist < reach {
                            z.state = ZState::Windup { t: 0.0 };
                        }
                    }
                    z.growl_t -= dt;
                    if z.growl_t <= 0.0 {
                        z.growl_t = 4.0 + (i as f32 * 1.7) % 5.0;
                        if dist < 14.0 {
                            self.events |= EV_GROWL;
                        }
                    }
                }
                ZState::Windup { t } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 10.0).max(0.0) + push;
                    z.facing = z.facing.lerp(dir_p, clamp(dt * 10.0, 0.0, 1.0)).norm();
                    let wind = if z.kind == ZKind::Runner { 0.38 } else { 0.55 };
                    if t >= wind {
                        if dist < reach + 0.45 && !player_dead && hit_count < pending_hits.len() {
                            pending_hits[hit_count] = (z.damage, dir_p, z.pos);
                            hit_count += 1;
                        }
                        z.state = ZState::Recover { t: 0.0 };
                    } else {
                        z.state = ZState::Windup { t };
                    }
                }
                ZState::Recover { t } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 8.0).max(0.0) + push;
                    z.state = if t >= 0.55 { ZState::Chase } else { ZState::Recover { t } };
                }
                ZState::LungeWind { t, dir } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 10.0).max(0.0);
                    z.facing = dir;
                    z.state = if t >= 0.42 { ZState::Lunge { t: 0.0, dir, hit: false } } else { ZState::LungeWind { t, dir } };
                }
                ZState::Lunge { t, dir, hit } => {
                    let t = t + dt;
                    z.vel = dir * 9.0 * (1.0 - t / 0.5).max(0.2);
                    let mut hit = hit;
                    if !hit && dist < 1.0 && !player_dead && hit_count < pending_hits.len() {
                        hit = true;
                        pending_hits[hit_count] = (z.damage, dir, z.pos);
                        hit_count += 1;
                    }
                    if t >= 0.5 {
                        z.state = ZState::Recover { t: -0.3 };
                        z.lunge_cd = 3.5;
                    } else {
                        z.state = ZState::Lunge { t, dir, hit };
                    }
                }
                ZState::Stagger { t } => {
                    let t = t - dt;
                    z.vel = z.vel * (1.0 - dt * 5.0).max(0.0);
                    z.state = if t <= 0.0 { ZState::Chase } else { ZState::Stagger { t } };
                }
                ZState::Dying { t } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 4.0).max(0.0);
                    z.state = if t >= 1.3 { ZState::Corpse { t: 0.0 } } else { ZState::Dying { t } };
                }
                ZState::Corpse { t } => {
                    z.vel = V2::ZERO;
                    z.state = ZState::Corpse { t: t + dt };
                }
            }
            let next = z.pos + z.vel * dt;
            z.pos = if z.alive() { self.world.collide(next, ZOMBIE_RADIUS) } else { next };
            i += 1;
        }
        for k in 0..hit_count {
            let (dmg, dir, from) = pending_hits[k];
            self.hurt_player(dmg, dir, from);
        }
        // keep the player from overlapping living zombies
        for z in self.zombies.iter().filter(|z| z.alive()) {
            let d = self.player.pos - z.pos;
            let l = d.len();
            let min = PLAYER_RADIUS + ZOMBIE_RADIUS;
            if l < min && l > 1e-4 && !matches!(self.player.state, PState::Dodge { .. }) {
                self.player.pos += d * ((min - l) / l * 0.6);
            }
        }
        self.zombies.retain(|z| !matches!(z.state, ZState::Corpse { t } if t > 14.0));
    }

    fn update_pickups(&mut self, dt: f32) {
        let ppos = self.player.pos;
        let mut got = 0;
        self.pickups.retain_mut(|p| {
            p.t += dt;
            if (p.pos - ppos).len() < 1.1 {
                got += 1;
                false
            } else {
                p.t < 30.0
            }
        });
        if got > 0 {
            self.player.medkits = (self.player.medkits + got).min(5);
            self.events |= EV_PICKUP;
        }
    }

    // ---------------------------------------------------------------- effects
    fn spawn_fx(&mut self, anim: (usize, usize), pos: V3, size: f32, dur: f32, color: [f32; 3], alpha: f32, additive: bool) {
        let mirror = self.rng.chance(0.5);
        self.push_fx(Fx { anim, pos, vel: v3(0.0, 0.0, 0.0), t: 0.0, dur, size, color, alpha, additive, mirror, follow_player: false });
    }

    fn spawn_fx_dir(&mut self, anim: (usize, usize), pos: V3, size: f32, dur: f32, color: [f32; 3], alpha: f32, mirror: bool) {
        self.push_fx(Fx { anim, pos, vel: v3(0.0, 0.0, 0.0), t: 0.0, dur, size, color, alpha, additive: false, mirror, follow_player: false });
    }

    fn spawn_fx_follow(&mut self, anim: (usize, usize), pos: V3, size: f32, dur: f32, color: [f32; 3], alpha: f32) {
        let off = v3(pos.x - self.player.pos.x, pos.y, pos.z - self.player.pos.y);
        self.push_fx(Fx { anim, pos: off, vel: v3(0.0, 0.0, 0.0), t: 0.0, dur, size, color, alpha, additive: true, mirror: false, follow_player: true });
    }

    fn push_fx(&mut self, fx: Fx) {
        if self.fx.len() >= MAX_FX {
            self.fx.remove(0);
        }
        self.fx.push(fx);
    }

    fn add_decal(&mut self, anim: (usize, usize), pos: V2, size: f32) {
        if self.decals.len() >= MAX_DECALS {
            self.decals.remove(0);
        }
        let rot = self.rng.range(0.0, std::f32::consts::TAU);
        self.decals.push(Decal { anim, pos, t: 0.0, grow: 1.6, life: 40.0, size, rot });
    }

    fn update_fx(&mut self, dt: f32) {
        for f in &mut self.fx {
            f.t += dt;
            f.pos = f.pos + f.vel * dt;
        }
        self.fx.retain(|f| f.t < f.dur);
        for d in &mut self.decals {
            d.t += dt;
        }
        self.decals.retain(|d| d.t < d.life);
    }

    // ---------------------------------------------------------------- output
    pub fn hud(&self, fps: f32) -> [f32; HUD_LEN] {
        let mut h = [0.0; HUD_LEN];
        let p = &self.player;
        h[HUD_HP] = p.hp;
        h[HUD_HP_MAX] = p.hp_max;
        h[HUD_STAMINA] = p.stamina;
        h[HUD_STAMINA_MAX] = p.stamina_max;
        h[HUD_MEDKITS] = p.medkits as f32;
        h[HUD_WAVE] = self.wave as f32;
        h[HUD_ENEMIES] = (self.alive_count() as u32 + self.pending) as f32;
        h[HUD_KILLS] = self.kills as f32;
        h[HUD_SCORE] = self.score as f32;
        h[HUD_COMBO] = self.combo as f32;
        h[HUD_STATE] = match self.phase {
            Phase::Playing => 0.0,
            Phase::Break => 1.0,
            // give the death animation time to play before the panel shows
            Phase::GameOver => if self.end_t > 2.2 { 2.0 } else { 0.0 },
            Phase::Victory => if self.end_t > 0.8 { 3.0 } else { 0.0 },
        };
        h[HUD_BREAK] = self.break_t.max(0.0);
        h[HUD_OBJECTIVE] = if self.extraction_active { 1.0 } else { 0.0 };
        h[HUD_KICK_KILLS] = self.kick_kills as f32;
        h[HUD_MAX_COMBO] = self.max_combo as f32;
        h[HUD_TIME] = self.time;
        h[HUD_EXTRACT_DIST] = (self.world.extraction - p.pos).len();
        h[HUD_STORY_WAVES] = STORY_WAVES as f32;
        h[HUD_FPS] = fps;
        h[HUD_MODE] = if self.mode == Mode::Story { 1.0 } else { 0.0 };
        h
    }

    /// Ground-lying sprites (blood, decals, pickups' shadows) and billboards.
    pub fn collect_sprites(&self, out: &mut Vec<Sprite>) {
        out.clear();
        // decals
        for d in &self.decals {
            let k = clamp(d.t / d.grow, 0.0, 1.0);
            let fi = d.anim.0 + ((k * (d.anim.1 - 1) as f32) as usize).min(d.anim.1 - 1);
            let fade = clamp((d.life - d.t) / 4.0, 0.0, 1.0) * 0.92;
            let f = &FRAMES[fi];
            let s = d.size * (0.5 + 0.5 * k);
            out.push(Sprite {
                pos: v3(d.pos.x, 0.02, d.pos.y),
                frame: fi as u16,
                w: f.w * s,
                h: f.h * s,
                ax: 0.5,
                ay: 0.5,
                mirror: false,
                color: [0.75 * fade, 0.7 * fade, 0.7 * fade, fade],
                flash: 0.0,
                ground: Some(d.rot),
                xray: false,
            });
        }
        for h in &self.world.holes {
            let f = &FRAMES[G_DISC.0];
            out.push(Sprite {
                pos: v3(h.pos.x, 0.015, h.pos.y),
                frame: G_DISC.0 as u16,
                w: f.w * h.size,
                h: f.h * h.size * 0.8,
                ax: 0.5,
                ay: 0.5,
                mirror: false,
                color: [0.0, 0.0, 0.0, 0.45],
                flash: 0.0,
                ground: Some(h.rot),
                xray: false,
            });
        }
        // extraction beacon
        if self.extraction_active {
            let e = self.world.extraction;
            let pulse = 0.7 + 0.3 * (self.time * 4.0).sin();
            out.push(Sprite {
                pos: v3(e.x, 0.05, e.y),
                frame: G_RING.0 as u16,
                w: 6.0,
                h: 6.0,
                ax: 0.5,
                ay: 0.5,
                mirror: false,
                color: [1.0 * pulse, 0.85 * pulse, 0.2 * pulse, 0.0],
                flash: 0.0,
                ground: Some(self.time),
                xray: false,
            });
        }

        // blob shadows
        let shadow = |p: V2, r: f32, out: &mut Vec<Sprite>| {
            out.push(Sprite {
                pos: v3(p.x, 0.03, p.y),
                frame: G_DISC.0 as u16,
                w: r * 2.0,
                h: r * 1.4,
                ax: 0.5,
                ay: 0.5,
                mirror: false,
                color: [0.0, 0.0, 0.0, 0.55],
                flash: 0.0,
                ground: Some(0.0),
                xray: false,
            });
        };
        shadow(self.player.pos, 0.55, out);
        for z in &self.zombies {
            if !matches!(z.state, ZState::Corpse { .. }) {
                shadow(z.pos, 0.5, out);
            }
        }

        // pickups
        for p in &self.pickups {
            let bob = (p.t * 3.0).sin() * 0.12;
            let blink = if p.t > 25.0 && (p.t * 8.0).sin() < 0.0 { 0.3 } else { 1.0 };
            let f = &FRAMES[I_MEDKIT.0];
            shadow(p.pos, 0.3, out);
            out.push(Sprite {
                pos: v3(p.pos.x, 0.35 + bob, p.pos.y),
                frame: I_MEDKIT.0 as u16,
                w: f.w * 0.75,
                h: f.h * 0.75,
                ax: 0.5,
                ay: 0.0,
                mirror: false,
                color: [blink, blink, blink, blink],
                flash: 0.15 + 0.15 * (p.t * 5.0).sin(),
                ground: None,
                xray: false,
            });
        }

        self.player_sprites(out);
        for z in &self.zombies {
            zombie_sprite(z, out);
        }

        // effects
        for fx in &self.fx {
            let k = clamp(fx.t / fx.dur, 0.0, 0.999);
            let fi = fx.anim.0 + (k * fx.anim.1 as f32) as usize;
            let f = &FRAMES[fi];
            let pos = if fx.follow_player { v3(fx.pos.x + self.player.pos.x, fx.pos.y, fx.pos.z + self.player.pos.y) } else { fx.pos };
            let a = fx.alpha * (1.0 - smooth_tail(k));
            out.push(Sprite {
                pos,
                frame: fi as u16,
                w: f.w * fx.size,
                h: f.h * fx.size,
                ax: 0.5,
                ay: 0.5,
                mirror: fx.mirror,
                color: [fx.color[0] * a, fx.color[1] * a, fx.color[2] * a, if fx.additive { 0.0 } else { a }],
                flash: 0.0,
                ground: None,
                xray: false,
            });
        }
    }

    fn player_sprites(&self, out: &mut Vec<Sprite>) {
        let p = &self.player;
        let tint = if p.skin == 1 { [0.62, 0.56, 0.52] } else { [1.0, 1.0, 1.0] };
        let facing_left = p.facing.x < -0.05;
        let (fi, bob, lean): (usize, f32, f32) = match p.state {
            PState::Punch { step, t, .. } => {
                let dur = if step == 2 { 0.42 } else { 0.30 };
                let k = clamp(t / dur, 0.0, 0.999);
                // jab = frames 1..5, cross = 6..9, finisher = 9..11 (strong reach)
                let seq: &[usize] = match step {
                    0 => &[1, 2, 3, 3, 4, 5],
                    1 => &[6, 7, 8, 8, 9],
                    _ => &[9, 10, 10, 10, 11, 5],
                };
                (P_PUNCH.0 + seq[(k * seq.len() as f32) as usize], 0.0, 0.0)
            }
            PState::Kick { t, .. } => {
                let seq = [3usize, 4, 4, 4, 5];
                let k = clamp(t / 0.45, 0.0, 0.999);
                (P_PUNCH.0 + seq[(k * seq.len() as f32) as usize], 0.0, 0.0)
            }
            PState::Dead { t } => {
                let k = clamp(t / 1.6, 0.0, 0.999);
                (P_DEATH.0 + (k * P_DEATH.1 as f32) as usize, 0.0, 0.0)
            }
            _ => {
                let fi = P_TURN.0 + turn_view(p.facing);
                let speed = p.vel.len();
                let bob = (p.walk_phase * std::f32::consts::PI).sin().abs() * 0.06 * clamp(speed / 4.0, 0.0, 1.0);
                (if p.blocking { P_PUNCH.0 } else { fi }, bob, clamp(p.vel.x * 0.02, -0.08, 0.08))
            }
        };
        let _ = lean;
        let f = &FRAMES[fi];
        let mirror = if matches!(p.state, PState::Free | PState::Hurt { .. } | PState::Dodge { .. }) && !p.blocking {
            turn_mirror(p.facing)
        } else {
            facing_left
        };
        let blink = if p.invuln > 0.0 && !matches!(p.state, PState::Dodge { .. }) && (self.time * 30.0).sin() > 0.4 { 0.55 } else { 1.0 };
        let base = Sprite {
            pos: v3(p.pos.x, bob, p.pos.y),
            frame: fi as u16,
            w: f.w * PLAYER_HEIGHT,
            h: f.h * PLAYER_HEIGHT,
            ax: if mirror { 1.0 - f.ax } else { f.ax },
            ay: 0.0,
            mirror,
            color: [tint[0] * blink, tint[1] * blink, tint[2] * blink, blink],
            flash: p.flash * 0.8,
            ground: None,
            xray: true,
        };
        // dodge afterimages
        if let PState::Dodge { t, dir } = p.state {
            for k in 1..3 {
                let back = dir * (-(k as f32) * 0.55 * (1.0 - t / 0.32).max(0.2));
                let a = 0.35 / k as f32;
                let mut s = base;
                s.pos = v3(p.pos.x + back.x, 0.0, p.pos.y + back.y);
                s.color = [0.5 * a, 0.7 * a, 0.9 * a, 0.0];
                s.xray = false;
                out.push(s);
            }
        }
        out.push(base);
    }
}

fn smooth_tail(k: f32) -> f32 {
    if k < 0.6 {
        0.0
    } else {
        (k - 0.6) / 0.4
    }
}

/// Turnaround sheet view index for a facing (0 front, 2 right profile, 4 back).
fn turn_view(f: V2) -> usize {
    let a = f.x.abs().atan2(f.y); // 0 = towards camera, PI = away
    ((a / std::f32::consts::FRAC_PI_4).round() as usize).min(4)
}

fn turn_mirror(f: V2) -> bool {
    f.x < -0.05
}

fn zombie_sprite(z: &Zombie, out: &mut Vec<Sprite>) {
    let (tint, scale) = match z.kind {
        ZKind::Walker => ([1.0, 1.0, 1.0], 1.0),
        ZKind::Runner => ([1.05, 0.9, 0.86], 0.97),
        ZKind::Lunger => ([0.82, 0.9, 0.84], 1.04),
        ZKind::Bloater => ([0.86, 1.05, 0.62], 1.18),
    };
    let height = PLAYER_HEIGHT * 1.0 * scale;
    let frame_of = |anim: (usize, usize), t: f32, fps: f32| anim.0 + ((t * fps) as usize % anim.1);
    let mut fade = 1.0;
    let fi = match z.state {
        ZState::Chase => {
            let speed = z.vel.len();
            if speed < 0.25 {
                Z_IDLE.0
            } else if z.kind == ZKind::Runner {
                frame_of(Z_RUN, z.anim_t, 16.0 * (speed / 3.5).max(0.6))
            } else {
                frame_of(Z_WALK, z.anim_t, 8.0 * (speed / 1.35).clamp(0.6, 1.6))
            }
        }
        ZState::Windup { t } => {
            // reach-and-claw pose taken from the lunge sheet
            let seq = [Z_LUNGE.0 + 10, Z_LUNGE.0 + 11, Z_LUNGE.0 + 12];
            seq[((t / 0.55) * 3.0).min(2.0) as usize]
        }
        ZState::Recover { .. } => Z_LUNGE.0 + 13,
        ZState::LungeWind { t, .. } => Z_LUNGE.0 + ((t / 0.42 * 4.0) as usize).min(3),
        ZState::Lunge { t, .. } => Z_LUNGE.0 + 4 + ((t / 0.5 * 6.0) as usize).min(5),
        ZState::Stagger { .. } => Z_DEATH.0 + 1,
        ZState::Dying { t } => Z_DEATH.0 + ((t / 1.3 * Z_DEATH.1 as f32) as usize).min(Z_DEATH.1 - 1),
        ZState::Corpse { t } => {
            fade = clamp((14.0 - t) / 2.0, 0.0, 1.0);
            Z_DEATH.0 + Z_DEATH.1 - 1
        }
    };
    let f = &FRAMES[fi];
    let mirror = z.facing.x < 0.0;
    out.push(Sprite {
        pos: v3(z.pos.x, 0.0, z.pos.y),
        frame: fi as u16,
        w: f.w * height,
        h: f.h * height,
        ax: if mirror { 1.0 - f.ax } else { f.ax },
        ay: 0.0,
        mirror,
        color: [tint[0] * fade, tint[1] * fade, tint[2] * fade, fade],
        flash: z.flash,
        ground: None,
        xray: false,
    });
    // health bar
    if z.show_bar > 0.0 && z.alive() {
        let a = clamp(z.show_bar, 0.0, 1.0);
        let k = clamp(z.hp / z.hp_max, 0.0, 1.0);
        let top = height * 1.08;
        let bar = |x0: f32, w: f32, c: [f32; 3], out: &mut Vec<Sprite>| {
            out.push(Sprite {
                pos: v3(z.pos.x + x0, top, z.pos.y),
                frame: G_WHITE.0 as u16,
                w,
                h: 0.09,
                ax: 0.0,
                ay: 0.0,
                mirror: false,
                color: [c[0] * a, c[1] * a, c[2] * a, a],
                flash: 0.0,
                ground: None,
                xray: false,
            });
        };
        bar(-0.42, 0.84, [0.08, 0.08, 0.08], out);
        bar(-0.40, 0.80 * k, [0.85, 0.12, 0.08], out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(g: &mut Game, secs: f32, input: Input) {
        let steps = (secs * 60.0) as i32;
        for _ in 0..steps {
            g.step(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn first_wave_spawns_zombies() {
        let mut g = Game::new(Mode::Endless, 0, 1);
        run(&mut g, 6.0, Input::default());
        assert_eq!(g.wave, 1);
        assert!(g.alive_count() > 0);
    }

    #[test]
    fn punching_kills_a_zombie() {
        let mut g = Game::new(Mode::Endless, 0, 2);
        g.phase = Phase::Playing;
        g.wave = 1;
        g.pending = 0;
        let pos = g.player.pos + v2(1.2, 0.0);
        g.zombies.push(Zombie {
            kind: ZKind::Walker,
            pos,
            vel: V2::ZERO,
            facing: v2(-1.0, 0.0),
            hp: 50.0,
            hp_max: 50.0,
            speed: 0.0,
            damage: 0.0,
            state: ZState::Chase,
            anim_t: 0.0,
            flash: 0.0,
            lunge_cd: 9.0,
            growl_t: 9.0,
            show_bar: 0.0,
        });
        let mut ev = 0;
        for i in 0..240 {
            let input = Input { attack: i % 12 == 0, ..Input::default() };
            g.step(1.0 / 60.0, &input);
            ev |= g.take_events();
        }
        assert_eq!(g.kills, 1, "zombie should die from a punch combo");
        assert!(ev & EV_HIT != 0 && ev & EV_ZOMBIE_DIE != 0);
        assert!(g.max_combo >= 2);
    }

    #[test]
    fn zombies_hurt_idle_player_and_game_ends() {
        let mut g = Game::new(Mode::Endless, 0, 3);
        run(&mut g, 120.0, Input::default());
        assert_eq!(g.phase, Phase::GameOver);
        assert_eq!(g.hud(60.0)[HUD_STATE], 2.0);
    }

    #[test]
    fn dodge_gives_invulnerability_and_costs_stamina() {
        let mut g = Game::new(Mode::Endless, 0, 4);
        g.step(1.0 / 60.0, &Input { dodge: true, mv: v2(1.0, 0.0), ..Input::default() });
        assert!(g.player.invuln > 0.0);
        assert!(g.player.stamina < 80.0);
    }

    #[test]
    fn heal_consumes_medkit() {
        let mut g = Game::new(Mode::Endless, 0, 5);
        g.player.hp = 30.0;
        g.step(1.0 / 60.0, &Input { heal: true, ..Input::default() });
        assert_eq!(g.player.medkits, 1);
        assert!(g.player.hp > 70.0);
    }

    #[test]
    fn sprites_reference_valid_frames() {
        let mut g = Game::new(Mode::Story, 1, 6);
        let mut out = Vec::new();
        for i in 0..1800 {
            let input = Input { attack: i % 9 == 0, kick: i % 50 == 0, mv: v2((i as f32 * 0.01).sin(), 0.3), ..Input::default() };
            g.step(1.0 / 60.0, &input);
            g.collect_sprites(&mut out);
            for s in &out {
                assert!((s.frame as usize) < FRAMES.len());
                assert!(s.w.is_finite() && s.h.is_finite());
            }
        }
    }

    #[test]
    fn simulation_step_is_fast() {
        let mut g = Game::new(Mode::Endless, 0, 7);
        g.player.hp = 1e9;
        g.player.hp_max = 1e9;
        run(&mut g, 90.0, Input { attack: true, ..Input::default() });
        let mut out = Vec::new();
        let t0 = std::time::Instant::now();
        for _ in 0..600 {
            g.step(1.0 / 60.0, &Input { attack: true, ..Input::default() });
            g.collect_sprites(&mut out);
        }
        let per = t0.elapsed().as_secs_f64() / 600.0;
        // generous bound for debug builds; release is ~20x faster
        assert!(per < 0.004, "step took {per}s");
    }
}

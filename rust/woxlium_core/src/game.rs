//! First-person gameplay simulation (no GL): player melee, zombie AI driven by
//! skeletal clips, waves, pickups and effects.

use crate::anim::Animator;
use crate::level::Level;
use crate::math::{clamp, v2, v3, Rng, V2, V3};

pub const EYE_HEIGHT: f32 = 1.62;
pub const PLAYER_RADIUS: f32 = 0.35;
pub const ZOMBIE_RADIUS: f32 = 0.33;
pub const STORY_WAVES: u32 = 5;
const MAX_ALIVE: usize = 14;

// ---- events (bit flags for audio / haptics)
pub const EV_SWING: u32 = 1 << 0;
pub const EV_HIT: u32 = 1 << 1;
pub const EV_HEAVY_HIT: u32 = 1 << 2;
pub const EV_CRIT: u32 = 1 << 3;
pub const EV_ZOMBIE_DIE: u32 = 1 << 4;
pub const EV_PLAYER_HURT: u32 = 1 << 5;
pub const EV_BLOCK: u32 = 1 << 6;
pub const EV_DODGE: u32 = 1 << 7;
pub const EV_HEAL: u32 = 1 << 8;
pub const EV_WAVE_START: u32 = 1 << 9;
pub const EV_WAVE_CLEAR: u32 = 1 << 10;
pub const EV_GROWL: u32 = 1 << 11;
pub const EV_NO_STAMINA: u32 = 1 << 13;
pub const EV_GAME_OVER: u32 = 1 << 14;
pub const EV_VICTORY: u32 = 1 << 15;
pub const EV_STEP: u32 = 1 << 16;

// ---- HUD layout shared with Kotlin (NativeBridge.HUD_*)
pub const HUD_LEN: usize = 16;
pub const HUD_HP: usize = 0;
pub const HUD_HP_MAX: usize = 1;
pub const HUD_STAMINA: usize = 2;
pub const HUD_STAMINA_MAX: usize = 3;
pub const HUD_WAVE: usize = 4;
pub const HUD_ENEMIES: usize = 5;
pub const HUD_KILLS: usize = 6;
pub const HUD_SCORE: usize = 7;
pub const HUD_COMBO: usize = 8;
pub const HUD_STATE: usize = 9;
pub const HUD_BREAK: usize = 10;
pub const HUD_OBJECTIVE: usize = 11;
pub const HUD_MAX_COMBO: usize = 12;
pub const HUD_TIME: usize = 13;
pub const HUD_EXTRACT_DIST: usize = 14;
pub const HUD_MODE: usize = 15;

/// Clip indices + durations for one skeleton family.
#[derive(Clone, Copy, Debug)]
pub struct Clips<const N: usize> {
    pub id: [usize; N],
    pub dur: [f32; N],
}

pub const Z_IDLE: usize = 0;
pub const Z_WALK: usize = 1;
pub const Z_WALK2: usize = 2;
pub const Z_RUN: usize = 3;
pub const Z_ATTACK: usize = 4;
pub const Z_ATTACK2: usize = 5;
pub const Z_HIT_CHEST: usize = 6;
pub const Z_HIT_HEAD: usize = 7;
pub const Z_DEATH: usize = 8;
pub const ZOMBIE_CLIPS: [&str; 9] = ["idle", "walk", "walk2", "run", "attack", "attack2", "hit_chest", "hit_head", "death"];
pub type ZombieClips = Clips<9>;

pub const A_GUARD: usize = 0;
pub const A_JAB: usize = 1;
pub const A_CROSS: usize = 2;
pub const A_HIT: usize = 3;
pub const ARM_CLIPS: [&str; 4] = ["guard", "jab", "cross", "hit"];
pub type ArmClips = Clips<4>;

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
    pub look: V2,
    pub attack: bool,
    pub dodge: bool,
    pub block: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PState {
    Free,
    Punch { kind: u8, t: f32, hit: bool },
    Dodge { t: f32, dir: V2 },
    Dead { t: f32 },
}

pub struct Player {
    pub pos: V2,
    pub yaw: f32,
    pub pitch: f32,
    pub vel: V2,
    pub hp: f32,
    pub hp_max: f32,
    pub stamina: f32,
    pub stamina_max: f32,
    state: PState,
    pub blocking: bool,
    pub block_k: f32,
    invuln: f32,
    stamina_delay: f32,
    attack_buffer: f32,
    combo_step: u8,
    combo_window: f32,
    pub bob: f32,
    pub hurt_flash: f32,
    pub arms: Animator,
    step_t: f32,
    /// camera roll/kick from hits and dodges
    pub kick: V2,
}

impl Player {
    pub fn forward(&self) -> V2 {
        v2(self.yaw.sin(), self.yaw.cos())
    }
    pub fn eye(&self) -> V3 {
        let bob = (self.bob * 2.0).sin().abs() * 0.035 * clamp(self.vel.len() / 4.0, 0.0, 1.0);
        let y = if let PState::Dead { t } = self.state { EYE_HEIGHT - clamp(t * 1.6, 0.0, 1.3) } else { EYE_HEIGHT };
        v3(self.pos.x, y + bob, self.pos.y)
    }
    pub fn dead(&self) -> bool {
        matches!(self.state, PState::Dead { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZState {
    Chase,
    Attack { hit: bool },
    Hit { t: f32 },
    Dead { t: f32 },
}

pub struct Zombie {
    pub variant: usize,
    pub runner: bool,
    pub pos: V2,
    pub yaw: f32,
    vel: V2,
    pub hp: f32,
    pub hp_max: f32,
    speed: f32,
    damage: f32,
    pub state: ZState,
    pub anim: Animator,
    pub flash: f32,
    growl_t: f32,
    walk_clip: usize,
    attack_cd: f32,
    pub sink: f32,
}

impl Zombie {
    pub fn alive(&self) -> bool {
        !matches!(self.state, ZState::Dead { .. })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pickup {
    pub pos: V2,
    pub t: f32,
}

/// Billboard / decal effect, sampled from the VFX atlas by the renderer.
#[derive(Clone, Copy, Debug)]
pub struct Fx {
    pub anim: (usize, usize),
    pub pos: V3,
    pub vel: V3,
    pub t: f32,
    pub dur: f32,
    pub size: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub additive: bool,
    pub ground: bool,
    pub rot: f32,
}

pub struct Game {
    pub level: Level,
    pub player: Player,
    pub zombies: Vec<Zombie>,
    pub pickups: Vec<Pickup>,
    pub fx: Vec<Fx>,
    pub zclips: ZombieClips,
    pub aclips: ArmClips,
    pub mode: Mode,
    pub phase: Phase,
    pub wave: u32,
    pending: u32,
    spawn_t: f32,
    break_t: f32,
    pub kills: u32,
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
    pub variants: usize,
}

impl Game {
    pub fn new(mode: Mode, seed: u32, level: Level, zclips: ZombieClips, aclips: ArmClips, variants: usize) -> Game {
        let start = level.player_start;
        let mut arms = Animator::new(aclips.id[A_GUARD]);
        arms.time = aclips.dur[A_GUARD];
        let mut g = Game {
            player: Player {
                pos: start,
                yaw: 0.0,
                pitch: 0.0,
                vel: V2::ZERO,
                hp: 100.0,
                hp_max: 100.0,
                stamina: 100.0,
                stamina_max: 100.0,
                state: PState::Free,
                blocking: false,
                block_k: 0.0,
                invuln: 0.0,
                stamina_delay: 0.0,
                attack_buffer: 0.0,
                combo_step: 0,
                combo_window: 0.0,
                bob: 0.0,
                hurt_flash: 0.0,
                arms,
                step_t: 0.0,
                kick: V2::ZERO,
            },
            level,
            zombies: Vec::with_capacity(MAX_ALIVE + 8),
            pickups: Vec::new(),
            fx: Vec::with_capacity(128),
            zclips,
            aclips,
            mode,
            phase: Phase::Break,
            wave: 0,
            pending: 0,
            spawn_t: 0.0,
            break_t: 3.0,
            kills: 0,
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
            variants: variants.max(1),
        };
        g.level.update_flow(start);
        for i in 0..2 {
            let s = g.level.medkit_spots[i * 2 % g.level.medkit_spots.len()];
            g.pickups.push(Pickup { pos: s, t: 0.0 });
        }
        g
    }

    pub fn take_events(&mut self) -> u32 {
        std::mem::take(&mut self.events)
    }

    pub fn alive_count(&self) -> usize {
        self.zombies.iter().filter(|z| z.alive()).count()
    }

    pub fn step(&mut self, dt: f32, input: &Input) {
        let dt = dt.min(1.0 / 20.0);
        self.shake = (self.shake - dt * 2.2).max(0.0);
        // look is applied even during hit-stop so aiming never feels sticky
        self.apply_look(input.look);
        if self.hitstop > 0.0 {
            self.hitstop -= dt;
            if input.attack {
                self.player.attack_buffer = 0.35;
            }
            return;
        }
        self.time += dt;
        self.update_waves(dt);
        self.update_player(dt, input);
        self.flow_t -= dt;
        if self.flow_t <= 0.0 {
            self.flow_t = 0.25;
            self.level.update_flow(self.player.pos);
        }
        self.update_zombies(dt);
        self.update_pickups(dt);
        for f in &mut self.fx {
            f.t += dt;
            f.pos = f.pos + f.vel * dt;
            if !f.ground {
                f.vel.y -= 2.0 * dt;
            }
        }
        self.fx.retain(|f| f.t < f.dur);
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

    fn apply_look(&mut self, look: V2) {
        let p = &mut self.player;
        if p.dead() {
            return;
        }
        p.yaw -= look.x;
        p.pitch = clamp(p.pitch - look.y, -1.1, 1.0);
    }

    fn update_waves(&mut self, dt: f32) {
        match self.phase {
            Phase::Break => {
                self.break_t -= dt;
                if self.break_t <= 0.0 {
                    self.wave += 1;
                    self.pending = if self.mode == Mode::Story { [5, 8, 11, 14, 18][(self.wave as usize - 1).min(4)] } else { 4 + self.wave * 3 };
                    self.spawn_t = 0.5;
                    self.phase = Phase::Playing;
                    self.events |= EV_WAVE_START;
                }
            }
            Phase::Playing => {
                if self.pending > 0 {
                    self.spawn_t -= dt;
                    if self.spawn_t <= 0.0 && self.alive_count() < MAX_ALIVE {
                        self.spawn_t = (1.6 - self.wave as f32 * 0.1).max(0.6);
                        self.spawn_one();
                        self.pending -= 1;
                    }
                } else if self.alive_count() == 0 && !self.extraction_active {
                    self.events |= EV_WAVE_CLEAR;
                    if self.mode == Mode::Story && self.wave >= STORY_WAVES {
                        self.extraction_active = true;
                    } else {
                        self.phase = Phase::Break;
                        self.break_t = 6.0;
                        let spot = self.level.medkit_spots[self.rng.below(self.level.medkit_spots.len() as u32) as usize];
                        if self.pickups.len() < 3 {
                            self.pickups.push(Pickup { pos: spot, t: 0.0 });
                        }
                    }
                }
                if self.extraction_active && (self.player.pos - self.level.extraction).len() < 3.0 && !self.player.dead() {
                    self.phase = Phase::Victory;
                    self.events |= EV_VICTORY;
                }
            }
            _ => {}
        }
    }

    fn spawn_one(&mut self) {
        // farthest-ish spawn from the player so zombies walk in from the dark
        let pp = self.player.pos;
        let mut cands: Vec<V2> = self.level.spawns.iter().copied().filter(|s| (*s - pp).len() > 16.0).collect();
        if cands.is_empty() {
            cands = self.level.spawns.clone();
        }
        let base = cands[self.rng.below(cands.len() as u32) as usize];
        let pos = base + v2(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0));
        let w = self.wave;
        let runner = w >= 2 && self.rng.chance(0.18 + 0.04 * w.min(8) as f32);
        let scale = 1.0 + 0.1 * w.saturating_sub(1) as f32;
        let (hp, speed, damage) = if runner { (45.0, 3.3, 9.0) } else { (65.0, self.rng.range(1.0, 1.35), 13.0) };
        let walk_clip = if self.rng.chance(0.5) { Z_WALK } else { Z_WALK2 };
        let mut anim = Animator::new(self.zclips.id[if runner { Z_RUN } else { walk_clip }]);
        anim.time = self.rng.range(0.0, 2.0);
        self.zombies.push(Zombie {
            variant: self.rng.below(self.variants as u32) as usize,
            runner,
            pos,
            yaw: (pp - pos).x.atan2((pp - pos).y),
            vel: V2::ZERO,
            hp: hp * scale,
            hp_max: hp * scale,
            speed: speed * (1.0 + 0.02 * w.min(10) as f32),
            damage: damage * (1.0 + 0.05 * w as f32),
            state: ZState::Chase,
            anim,
            flash: 0.0,
            growl_t: self.rng.range(1.0, 5.0),
            walk_clip,
            attack_cd: 0.0,
            sink: 0.0,
        });
    }

    // ---------------------------------------------------------------- player
    fn update_player(&mut self, dt: f32, input: &Input) {
        let ac = self.aclips;
        let p = &mut self.player;
        p.invuln = (p.invuln - dt).max(0.0);
        p.hurt_flash = (p.hurt_flash - dt * 1.5).max(0.0);
        p.attack_buffer = (p.attack_buffer - dt).max(0.0);
        p.combo_window = (p.combo_window - dt).max(0.0);
        p.kick = p.kick * (1.0 - dt * 8.0).max(0.0);
        p.arms.update(dt);
        if input.attack {
            p.attack_buffer = 0.35;
        }
        if let PState::Dead { t } = &mut p.state {
            *t += dt;
            return;
        }
        p.stamina_delay = (p.stamina_delay - dt).max(0.0);
        p.blocking = input.block && matches!(p.state, PState::Free) && p.stamina > 2.0;
        p.block_k = (p.block_k + if p.blocking { dt * 9.0 } else { -dt * 7.0 }).clamp(0.0, 1.0);
        if p.blocking {
            p.stamina = (p.stamina - 7.0 * dt).max(0.0);
            p.stamina_delay = 0.4;
        } else if p.stamina_delay <= 0.0 {
            p.stamina = (p.stamina + 24.0 * dt).min(p.stamina_max);
        }

        // camera-relative movement
        let mv = if input.mv.len() > 1.0 { input.mv.norm() } else { input.mv };
        let f = p.forward();
        let right = v2(-f.y, f.x); // forward x up
        let dir = right * mv.x + f * (-mv.y);
        let speed = if p.blocking { 1.6 } else if matches!(p.state, PState::Punch { .. }) { 2.0 } else { 4.2 };

        match p.state {
            PState::Free => {
                p.vel = p.vel.lerp(dir * speed, clamp(dt * 12.0, 0.0, 1.0));
                if input.dodge {
                    self.try_dodge(dir);
                } else if self.player.attack_buffer > 0.0 && !self.player.blocking {
                    self.try_punch();
                }
            }
            PState::Punch { kind, t, hit } => {
                p.vel = p.vel.lerp(dir * speed, clamp(dt * 10.0, 0.0, 1.0));
                let t = t + dt;
                let (dur, hit_at) = punch_timing(kind, &ac);
                let mut hit = hit;
                if !hit && t >= hit_at {
                    hit = true;
                    self.player.state = PState::Punch { kind, t, hit };
                    self.resolve_punch(kind);
                }
                let p = &mut self.player;
                if t >= dur {
                    p.state = PState::Free;
                    p.combo_window = 0.5;
                    p.arms.play(ac.id[A_GUARD], 0.18, false);
                    p.arms.time = ac.dur[A_GUARD];
                    if p.attack_buffer > 0.0 {
                        self.try_punch();
                    }
                } else if t > dur * 0.6 && p.attack_buffer > 0.0 {
                    // cancel the recovery into the next punch
                    p.state = PState::Free;
                    self.try_punch();
                } else if matches!(p.state, PState::Punch { .. }) {
                    p.state = PState::Punch { kind, t, hit };
                }
            }
            PState::Dodge { t, dir } => {
                let t = t + dt;
                let k = 1.0 - t / 0.3;
                p.vel = dir * (11.0 * k.max(0.2));
                p.state = if t >= 0.3 { PState::Free } else { PState::Dodge { t, dir } };
            }
            PState::Dead { .. } => {}
        }
        let p = &mut self.player;
        p.pos = self.level.collide(p.pos + p.vel * dt, PLAYER_RADIUS);
        let sp = p.vel.len();
        p.bob += sp * dt * 1.9;
        if sp > 1.0 {
            p.step_t -= dt * sp / 4.0;
            if p.step_t <= 0.0 {
                p.step_t = 0.42;
                self.events |= EV_STEP;
            }
        }
    }

    fn spend(&mut self, cost: f32) -> bool {
        let p = &mut self.player;
        if p.stamina < cost {
            self.events |= EV_NO_STAMINA;
            return false;
        }
        p.stamina -= cost;
        p.stamina_delay = 0.6;
        true
    }

    fn try_punch(&mut self) {
        let ac = self.aclips;
        if !self.spend(8.0) {
            self.player.attack_buffer = 0.0;
            return;
        }
        let p = &mut self.player;
        p.attack_buffer = 0.0;
        let step = if p.combo_window > 0.0 { p.combo_step } else { 0 };
        p.combo_step = (step + 1) % 3;
        // jab, cross, then a heavier cross
        let kind = step;
        let clip = if kind == 0 { A_JAB } else { A_CROSS };
        p.arms.play(ac.id[clip], 0.08, true);
        p.arms.speed = if kind == 0 { 1.9 } else if kind == 1 { 1.75 } else { 1.45 };
        p.state = PState::Punch { kind, t: 0.0, hit: false };
        self.events |= EV_SWING;
        // magnetism: lunge slightly towards a zombie in front
        if let Some((i, d)) = self.target_in_front(2.6, 0.6) {
            let dir = (self.zombies[i].pos - self.player.pos).norm();
            if d > 1.2 {
                self.player.vel = dir * ((d - 1.0) * 5.0).min(5.0);
            }
        }
    }

    fn try_dodge(&mut self, dir: V2) {
        if !self.spend(20.0) {
            return;
        }
        let d = if dir.len() > 0.2 { dir.norm() } else { -self.player.forward() };
        self.player.state = PState::Dodge { t: 0.0, dir: d };
        self.player.invuln = 0.32;
        self.player.kick = v2(d.x * 0.04, 0.0);
        self.events |= EV_DODGE;
    }

    /// Nearest living zombie within `range` of the player whose direction is
    /// within the view cone (cos > min_dot). Returns index and distance.
    fn target_in_front(&self, range: f32, min_dot: f32) -> Option<(usize, f32)> {
        let f = self.player.forward();
        let mut best: Option<(usize, f32)> = None;
        for (i, z) in self.zombies.iter().enumerate() {
            if !z.alive() {
                continue;
            }
            let d = z.pos - self.player.pos;
            let l = d.len() - ZOMBIE_RADIUS;
            if l < range && d.norm().dot(f) > min_dot && best.map_or(true, |(_, bl)| l < bl) {
                best = Some((i, l));
            }
        }
        best
    }

    fn resolve_punch(&mut self, kind: u8) {
        let Some((i, _)) = self.target_in_front(1.45, 0.55) else { return };
        let heavy = kind == 2;
        let crit = self.rng.chance(0.12) || (heavy && self.combo >= 5) || self.player.pitch > 0.18 && self.rng.chance(0.4);
        let base = [16.0, 24.0, 34.0][kind as usize];
        let dmg = base * if crit { 2.0 } else { 1.0 } * self.rng.range(0.9, 1.1);
        let dir = (self.zombies[i].pos - self.player.pos).norm();
        let knock = if heavy { 5.5 } else { 2.2 };
        let zp = self.zombies[i].pos;
        let hit_pos = v3(zp.x - dir.x * 0.25, if crit { 1.62 } else { 1.35 }, zp.y - dir.y * 0.25);
        self.damage_zombie(i, dmg, dir * knock, crit);
        self.combo += 1;
        self.max_combo = self.max_combo.max(self.combo);
        self.combo_t = 2.5;
        self.hitstop = if crit { 0.085 } else if heavy { 0.06 } else { 0.04 };
        self.shake = self.shake.max(if crit || heavy { 0.5 } else { 0.25 });
        self.player.kick = v2(if kind == 0 { -0.012 } else { 0.012 }, 0.02);
        self.events |= if heavy { EV_HEAVY_HIT } else { EV_HIT };
        if crit {
            self.events |= EV_CRIT;
            self.fx_burst(crate::assets_gen::V_SHOCK, hit_pos, 0.9, 0.3, [1.0, 0.95, 0.8], 0.7, true);
        }
        self.fx_burst(crate::assets_gen::V_SPARK, hit_pos, 0.45, 0.22, [1.0, 0.9, 0.75], 0.8, true);
        let side = v3(-dir.y, 0.0, dir.x) * self.rng.range(-0.6, 0.6);
        self.fx.push(Fx {
            anim: crate::assets_gen::V_SPRAY,
            pos: hit_pos,
            vel: v3(dir.x * 1.6, 0.6, dir.y * 1.6) + side,
            t: 0.0,
            dur: 0.5,
            size: 0.8,
            color: [0.8, 0.8, 0.8],
            alpha: 1.0,
            additive: false,
            ground: false,
            rot: self.rng.range(0.0, 6.28),
        });
        self.fx_burst(crate::assets_gen::V_BLOOD, hit_pos, 0.6, 0.35, [0.85, 0.85, 0.85], 1.0, false);
    }

    fn damage_zombie(&mut self, i: usize, dmg: f32, impulse: V2, head: bool) {
        let zc = self.zclips;
        let z = &mut self.zombies[i];
        z.hp -= dmg;
        z.flash = 1.0;
        z.vel = impulse;
        if z.hp <= 0.0 {
            z.state = ZState::Dead { t: 0.0 };
            z.anim.play(zc.id[Z_DEATH], 0.12, true);
            z.anim.speed = 1.0;
            let pos = z.pos;
            self.kills += 1;
            self.score += 100 + self.combo.min(50) * 10 + if head { 50 } else { 0 };
            self.events |= EV_ZOMBIE_DIE;
            let rot = self.rng.range(0.0, 6.28);
            self.fx.push(Fx { anim: crate::assets_gen::V_POOL, pos: v3(pos.x, 0.03, pos.y), vel: V3::ZERO, t: 0.0, dur: 40.0, size: 1.9, color: [0.55, 0.5, 0.5], alpha: 0.95, additive: false, ground: true, rot });
        } else {
            z.state = ZState::Hit { t: 0.0 };
            let clip = if head { Z_HIT_HEAD } else { Z_HIT_CHEST };
            z.anim.play(zc.id[clip], 0.06, true);
            z.anim.speed = 1.15;
        }
    }

    fn hurt_player(&mut self, dmg: f32, from: V2) {
        let p = &mut self.player;
        if p.invuln > 0.0 || p.dead() {
            return;
        }
        let toward = (from - p.pos).norm();
        let facing = toward.dot(p.forward()) > 0.3;
        if p.blocking && facing {
            p.hp -= dmg * 0.15;
            p.stamina = (p.stamina - 10.0).max(0.0);
            p.invuln = 0.3;
            p.vel = -toward * 2.5;
            p.kick = v2(0.0, -0.03);
            self.events |= EV_BLOCK;
            self.shake = self.shake.max(0.2);
        } else {
            p.hp -= dmg;
            p.invuln = 0.5;
            p.vel = -toward * 3.5;
            p.hurt_flash = 1.0;
            p.kick = v2(toward.x * 0.06, -0.05);
            p.arms.play(self.aclips.id[A_HIT], 0.05, true);
            p.arms.speed = 1.4;
            p.state = PState::Free;
            self.events |= EV_PLAYER_HURT;
            self.shake = self.shake.max(0.45);
        }
        let p = &mut self.player;
        if p.hp <= 0.0 {
            p.hp = 0.0;
            p.state = PState::Dead { t: 0.0 };
            self.phase = Phase::GameOver;
            self.events |= EV_GAME_OVER;
        }
    }

    fn update_zombies(&mut self, dt: f32) {
        let zc = self.zclips;
        let ppos = self.player.pos;
        let pdead = self.player.dead();
        let n = self.zombies.len();
        let mut hits: Vec<(f32, V2)> = Vec::new();
        for i in 0..n {
            let mut push = V2::ZERO;
            if self.zombies[i].alive() {
                let pi = self.zombies[i].pos;
                for j in 0..n {
                    if i != j && self.zombies[j].alive() {
                        let d = pi - self.zombies[j].pos;
                        let l2 = d.len2();
                        if l2 < 0.75 * 0.75 && l2 > 1e-6 {
                            let l = l2.sqrt();
                            push += d * ((0.75 - l) / l);
                        }
                    }
                }
            }
            let zpos = self.zombies[i].pos;
            let to_p = ppos - zpos;
            let dist = to_p.len();
            let los = dist < 8.0 && self.level.line_of_sight(zpos, ppos);
            let flow = self.level.flow_dir(zpos);
            let z = &mut self.zombies[i];
            z.anim.update(dt);
            z.flash = (z.flash - dt * 6.0).max(0.0);
            z.attack_cd = (z.attack_cd - dt).max(0.0);
            let dir_p = to_p.norm();
            match z.state {
                ZState::Chase => {
                    let dir = if pdead { -dir_p } else if los { dir_p } else { flow.unwrap_or(dir_p) };
                    let speed = if pdead { 0.4 } else { z.speed };
                    let desired = dir * speed + push * 2.5;
                    z.vel = z.vel.lerp(desired, clamp(dt * 5.0, 0.0, 1.0));
                    let sp = z.vel.len();
                    if sp > 0.15 {
                        let target = z.vel.x.atan2(z.vel.y);
                        z.yaw = turn_towards(z.yaw, target, dt * 5.0);
                    }
                    // pick the locomotion clip and sync its rate to the ground speed
                    if z.runner {
                        z.anim.play(zc.id[Z_RUN], 0.25, false);
                        z.anim.speed = (sp / 3.0).clamp(0.5, 1.3);
                    } else if sp < 0.2 {
                        z.anim.play(zc.id[Z_IDLE], 0.35, false);
                        z.anim.speed = 1.0;
                    } else {
                        z.anim.play(zc.id[z.walk_clip], 0.3, false);
                        z.anim.speed = (sp / 1.15).clamp(0.6, 1.5);
                    }
                    if !pdead && dist < 1.25 && z.attack_cd <= 0.0 {
                        z.state = ZState::Attack { hit: false };
                        let c = if (i + self.wave as usize) % 2 == 0 { Z_ATTACK } else { Z_ATTACK2 };
                        z.anim.play(zc.id[c], 0.15, true);
                        z.anim.speed = if z.runner { 1.25 } else { 1.0 };
                    }
                    z.growl_t -= dt;
                    if z.growl_t <= 0.0 {
                        z.growl_t = 3.5 + (i as f32 * 1.3) % 4.0;
                        if dist < 15.0 {
                            self.events |= EV_GROWL;
                        }
                    }
                }
                ZState::Attack { hit } => {
                    z.vel = z.vel * (1.0 - dt * 6.0).max(0.0) + push;
                    z.yaw = turn_towards(z.yaw, dir_p.x.atan2(dir_p.y), dt * 6.0);
                    let k = z.anim.progress_of(z.anim.time, zc.dur[if z.anim.clip == zc.id[Z_ATTACK] { Z_ATTACK } else { Z_ATTACK2 }]);
                    if !hit && k > 0.42 {
                        z.state = ZState::Attack { hit: true };
                        if dist < 1.7 && !pdead {
                            hits.push((z.damage, z.pos));
                        }
                    }
                    if k >= 0.95 {
                        z.state = ZState::Chase;
                        z.attack_cd = 0.6;
                    }
                }
                ZState::Hit { t } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 5.0).max(0.0);
                    let clip = if z.anim.clip == zc.id[Z_HIT_HEAD] { Z_HIT_HEAD } else { Z_HIT_CHEST };
                    if t >= zc.dur[clip] / z.anim.speed.max(0.1) * 0.9 {
                        z.state = ZState::Chase;
                    } else {
                        z.state = ZState::Hit { t };
                    }
                }
                ZState::Dead { t } => {
                    let t = t + dt;
                    z.vel = z.vel * (1.0 - dt * 4.0).max(0.0);
                    z.state = ZState::Dead { t };
                    if t > 18.0 {
                        z.sink = (t - 18.0) * 0.5;
                    }
                }
            }
            let next = z.pos + z.vel * dt;
            z.pos = if z.alive() { self.level.collide(next, ZOMBIE_RADIUS) } else { next };
        }
        for (dmg, from) in hits {
            self.hurt_player(dmg, from);
        }
        // the player cannot walk through the living
        for z in self.zombies.iter().filter(|z| z.alive()) {
            let d = self.player.pos - z.pos;
            let l = d.len();
            let min = PLAYER_RADIUS + ZOMBIE_RADIUS;
            if l < min && l > 1e-4 && !matches!(self.player.state, PState::Dodge { .. }) {
                self.player.pos += d * ((min - l) / l);
            }
        }
        self.zombies.retain(|z| z.sink < 1.2);
    }

    fn update_pickups(&mut self, dt: f32) {
        let p = &mut self.player;
        let mut healed = false;
        self.pickups.retain_mut(|k| {
            k.t += dt;
            if (k.pos - p.pos).len() < 1.0 && p.hp < p.hp_max && !p.dead() {
                p.hp = (p.hp + 40.0).min(p.hp_max);
                healed = true;
                false
            } else {
                true
            }
        });
        if healed {
            self.events |= EV_HEAL;
        }
    }

    fn fx_burst(&mut self, anim: (usize, usize), pos: V3, size: f32, dur: f32, color: [f32; 3], alpha: f32, additive: bool) {
        let rot = self.rng.range(0.0, 6.28);
        self.fx.push(Fx { anim, pos, vel: V3::ZERO, t: 0.0, dur, size, color, alpha, additive, ground: false, rot });
    }

    pub fn hud(&self, fps: f32) -> [f32; HUD_LEN] {
        let mut h = [0.0; HUD_LEN];
        let p = &self.player;
        h[HUD_HP] = p.hp;
        h[HUD_HP_MAX] = p.hp_max;
        h[HUD_STAMINA] = p.stamina;
        h[HUD_STAMINA_MAX] = p.stamina_max;
        h[HUD_WAVE] = self.wave as f32;
        h[HUD_ENEMIES] = (self.alive_count() as u32 + self.pending) as f32;
        h[HUD_KILLS] = self.kills as f32;
        h[HUD_SCORE] = self.score as f32;
        h[HUD_COMBO] = self.combo as f32;
        h[HUD_STATE] = match self.phase {
            Phase::Playing => 0.0,
            Phase::Break => 1.0,
            Phase::GameOver => if self.end_t > 2.0 { 2.0 } else { 0.0 },
            Phase::Victory => if self.end_t > 0.8 { 3.0 } else { 0.0 },
        };
        h[HUD_BREAK] = self.break_t.max(0.0);
        h[HUD_OBJECTIVE] = if self.extraction_active { 1.0 } else { 0.0 };
        h[HUD_MAX_COMBO] = self.max_combo as f32;
        h[HUD_TIME] = self.time;
        h[HUD_EXTRACT_DIST] = (self.level.extraction - p.pos).len();
        h[HUD_MODE] = if self.mode == Mode::Story { 1.0 } else { 0.0 };
        let _ = fps;
        h
    }
}

fn punch_timing(kind: u8, ac: &ArmClips) -> (f32, f32) {
    let (clip, speed, hit) = match kind {
        0 => (A_JAB, 1.9, 0.33),
        1 => (A_CROSS, 1.75, 0.36),
        _ => (A_CROSS, 1.45, 0.38),
    };
    let dur = ac.dur[clip] / speed;
    (dur * 0.92, dur * hit)
}

fn turn_towards(a: f32, b: f32, max: f32) -> f32 {
    let mut d = (b - a) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    a + d.clamp(-max, max)
}

impl Animator {
    /// Normalised progress for a clip of duration `dur`.
    pub fn progress_of(&self, time: f32, dur: f32) -> f32 {
        (time / dur.max(1e-3)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::approx_bounds;

    fn clips() -> (ZombieClips, ArmClips) {
        (
            Clips { id: [0, 1, 2, 3, 4, 5, 6, 7, 8], dur: [5.0, 2.5, 2.5, 0.7, 1.33, 1.2, 0.33, 0.43, 2.4] },
            Clips { id: [0, 1, 2, 3], dur: [0.87, 0.87, 1.0, 0.33] },
        )
    }

    fn game(mode: Mode) -> Game {
        let (z, a) = clips();
        Game::new(mode, 1, Level::build(&approx_bounds), z, a, 4)
    }

    fn run(g: &mut Game, secs: f32, input: Input) {
        for _ in 0..(secs * 60.0) as i32 {
            g.step(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn waves_spawn_and_zombies_approach() {
        let mut g = game(Mode::Endless);
        run(&mut g, 6.0, Input::default());
        assert_eq!(g.wave, 1);
        assert!(g.alive_count() > 0);
        let d0: f32 = g.zombies.iter().map(|z| (z.pos - g.player.pos).len()).sum::<f32>() / g.zombies.len() as f32;
        run(&mut g, 5.0, Input::default());
        let d1: f32 = g.zombies.iter().map(|z| (z.pos - g.player.pos).len()).sum::<f32>() / g.zombies.len() as f32;
        assert!(d1 < d0, "zombies should close in ({d0} -> {d1})");
    }

    #[test]
    fn punches_kill_a_zombie_in_front() {
        let mut g = game(Mode::Endless);
        g.phase = Phase::Playing;
        g.wave = 1;
        g.pending = 0;
        g.spawn_one();
        let p = g.player.pos;
        g.zombies[0].pos = p + v2(0.0, 1.3);
        g.zombies[0].speed = 0.0;
        g.zombies[0].damage = 0.0;
        let mut ev = 0;
        for i in 0..400 {
            g.step(1.0 / 60.0, &Input { attack: i % 15 == 0, ..Input::default() });
            ev |= g.take_events();
            g.zombies.iter_mut().for_each(|z| z.speed = 0.0);
        }
        assert_eq!(g.kills, 1);
        assert!(ev & EV_HIT != 0 && ev & EV_ZOMBIE_DIE != 0);
        assert!(g.max_combo >= 3);
    }

    #[test]
    fn idle_player_dies_and_block_reduces_damage() {
        let mut g = game(Mode::Endless);
        run(&mut g, 150.0, Input::default());
        assert_eq!(g.phase, Phase::GameOver);

        let mut g = game(Mode::Endless);
        g.hurt_player(20.0, g.player.pos + v2(0.0, 1.0));
        let open = 100.0 - g.player.hp;
        let mut g = game(Mode::Endless);
        g.player.blocking = true;
        g.hurt_player(20.0, g.player.pos + v2(0.0, 1.0));
        let blocked = 100.0 - g.player.hp;
        assert!(blocked < open * 0.3, "{blocked} vs {open}");
    }

    #[test]
    fn movement_is_camera_relative() {
        let mut g = game(Mode::Endless);
        let start = g.player.pos;
        // stick up = forward (+Z at yaw 0)
        run(&mut g, 0.5, Input { mv: v2(0.0, -1.0), ..Input::default() });
        assert!(g.player.pos.y > start.y + 0.5);
        // turn 90° left and walk forward: should move along +X
        let mut g = game(Mode::Endless);
        g.player.yaw = std::f32::consts::FRAC_PI_2;
        let start = g.player.pos;
        run(&mut g, 0.5, Input { mv: v2(0.0, -1.0), ..Input::default() });
        assert!(g.player.pos.x > start.x + 0.5, "{:?}", g.player.pos);
        // stick right = strafe right (screen right at yaw 0 is -X when looking down +Z)
        let mut g = game(Mode::Endless);
        let start = g.player.pos;
        run(&mut g, 0.5, Input { mv: v2(1.0, 0.0), ..Input::default() });
        assert!(g.player.pos.x < start.x - 0.5, "{:?}", g.player.pos);
    }

    #[test]
    fn story_mode_extraction_wins() {
        let mut g = game(Mode::Story);
        g.wave = STORY_WAVES;
        g.phase = Phase::Playing;
        g.pending = 0;
        run(&mut g, 0.1, Input::default());
        assert!(g.extraction_active);
        g.player.pos = g.level.extraction;
        run(&mut g, 0.1, Input::default());
        assert_eq!(g.phase, Phase::Victory);
    }

    #[test]
    fn pickups_heal() {
        let mut g = game(Mode::Endless);
        g.player.hp = 30.0;
        g.player.pos = g.pickups[0].pos;
        run(&mut g, 0.05, Input::default());
        assert!(g.player.hp >= 70.0);
    }
}

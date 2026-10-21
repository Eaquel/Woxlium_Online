//! Static level: ground materials, ruined concrete walls, collision and the
//! flow-field used by zombies to path around obstacles.

use crate::math::{v2, Rng, V2};

pub const TILE: f32 = 4.0;
pub const TILES: usize = 24;
pub const SIZE: f32 = TILE * TILES as f32; // 96 m
pub const NAV: usize = 96; // 1 m navigation cells
const NAV_INF: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mat {
    Grass = 0,
    Dirt = 1,
    Mud = 2,
    Asphalt = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub min: V2,
    pub max: V2,
    pub height: f32,
    /// 0 = clean concrete, 1 = cracked, 2 = bullet holes
    pub style: u8,
}

impl Wall {
    pub fn center(&self) -> V2 {
        (self.min + self.max) * 0.5
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Decor {
    pub pos: V2,
    pub size: f32,
    pub rot: f32,
}

pub struct World {
    pub tiles: [[Mat; TILES]; TILES],
    pub walls: Vec<Wall>,
    pub holes: Vec<Decor>,
    pub extraction: V2,
    pub player_start: V2,
    blocked: Vec<bool>,
    flow: Vec<u16>,
    flow_target: (usize, usize),
    queue: Vec<u32>,
}

impl World {
    pub fn generate(seed: u32) -> World {
        let mut rng = Rng::new(seed);
        let mut tiles = [[Mat::Grass; TILES]; TILES];

        // organic dirt and mud patches
        for _ in 0..14 {
            let cx = rng.below(TILES as u32) as i32;
            let cz = rng.below(TILES as u32) as i32;
            let r = rng.range(1.0, 3.2);
            let m = if rng.chance(0.5) { Mat::Dirt } else { Mat::Mud };
            for z in 0..TILES as i32 {
                for x in 0..TILES as i32 {
                    let d = (((x - cx) * (x - cx) + (z - cz) * (z - cz)) as f32).sqrt();
                    if d < r + rng.range(-0.4, 0.4) {
                        tiles[z as usize][x as usize] = m;
                    }
                }
            }
        }
        // crossing roads + a ring road
        for i in 0..TILES {
            tiles[11][i] = Mat::Asphalt;
            tiles[12][i] = Mat::Asphalt;
            tiles[i][11] = Mat::Asphalt;
            tiles[i][12] = Mat::Asphalt;
        }
        for i in 4..20 {
            tiles[4][i] = Mat::Asphalt;
            tiles[19][i] = Mat::Asphalt;
            tiles[i][4] = Mat::Asphalt;
            tiles[i][19] = Mat::Asphalt;
        }

        let mut walls = Vec::new();
        // perimeter
        let t = 1.0;
        walls.push(Wall { min: v2(0.0, 0.0), max: v2(SIZE, t), height: 3.2, style: 0 });
        walls.push(Wall { min: v2(0.0, SIZE - t), max: v2(SIZE, SIZE), height: 3.2, style: 0 });
        walls.push(Wall { min: v2(0.0, 0.0), max: v2(t, SIZE), height: 3.2, style: 0 });
        walls.push(Wall { min: v2(SIZE - t, 0.0), max: v2(SIZE, SIZE), height: 3.2, style: 0 });

        // ruined buildings in the blocks between the roads
        let blocks = [
            (v2(22.0, 22.0), v2(42.0, 42.0)),
            (v2(54.0, 22.0), v2(74.0, 42.0)),
            (v2(22.0, 54.0), v2(42.0, 74.0)),
            (v2(54.0, 54.0), v2(74.0, 74.0)),
            (v2(6.0, 6.0), v2(14.0, 14.0)),
            (v2(82.0, 82.0), v2(90.0, 90.0)),
            (v2(6.0, 82.0), v2(14.0, 90.0)),
        ];
        for (bi, (lo, hi)) in blocks.iter().enumerate() {
            let pad = if bi < 4 { 2.0 } else { 0.5 };
            let w = rng.range(0.55, 0.9) * (hi.x - lo.x - pad * 2.0);
            let h = rng.range(0.55, 0.9) * (hi.y - lo.y - pad * 2.0);
            let ox = lo.x + pad + rng.range(0.0, hi.x - lo.x - pad * 2.0 - w);
            let oz = lo.y + pad + rng.range(0.0, hi.y - lo.y - pad * 2.0 - h);
            building(&mut walls, &mut rng, v2(ox, oz), v2(ox + w, oz + h));
        }

        // concrete road barriers
        let mut holes = Vec::new();
        for _ in 0..26 {
            let along = rng.range(6.0, SIZE - 6.0);
            let (x, z) = if rng.chance(0.5) { (along, rng.range(45.0, 51.0)) } else { (rng.range(45.0, 51.0), along) };
            if (x - 48.0).abs() < 7.0 && (z - 48.0).abs() < 7.0 {
                continue; // keep the spawn crossroads clear
            }
            if rng.chance(0.45) {
                holes.push(Decor { pos: v2(x, z), size: rng.range(1.6, 2.6), rot: rng.range(0.0, 6.28) });
            } else {
                let w = rng.range(1.0, 2.2);
                let horizontal = rng.chance(0.5);
                let (sx, sz) = if horizontal { (w, 0.7) } else { (0.7, w) };
                walls.push(Wall { min: v2(x, z), max: v2(x + sx, z + sz), height: rng.range(0.8, 1.2), style: rng.below(3) as u8 });
            }
        }

        let mut world = World {
            tiles,
            walls,
            holes,
            extraction: v2(88.0, 9.0),
            player_start: v2(48.0, 48.0),
            blocked: vec![false; NAV * NAV],
            flow: vec![NAV_INF; NAV * NAV],
            flow_target: (usize::MAX, usize::MAX),
            queue: Vec::with_capacity(NAV * NAV),
        };
        world.rebuild_nav();
        world
    }

    fn rebuild_nav(&mut self) {
        let margin = 0.3;
        for z in 0..NAV {
            for x in 0..NAV {
                let c = v2(x as f32 + 0.5, z as f32 + 0.5);
                self.blocked[z * NAV + x] = self.walls.iter().any(|w| {
                    c.x > w.min.x - margin && c.x < w.max.x + margin && c.y > w.min.y - margin && c.y < w.max.y + margin
                });
            }
        }
    }

    pub fn material_at(&self, p: V2) -> Mat {
        let x = ((p.x / TILE) as i32).clamp(0, TILES as i32 - 1) as usize;
        let z = ((p.y / TILE) as i32).clamp(0, TILES as i32 - 1) as usize;
        self.tiles[z][x]
    }

    pub fn is_walkable(&self, p: V2) -> bool {
        match nav_cell(p) {
            Some((x, z)) => !self.blocked[z * NAV + x],
            None => false,
        }
    }

    /// Push a circle out of all walls. Returns the corrected position.
    pub fn collide(&self, mut p: V2, r: f32) -> V2 {
        for _ in 0..2 {
            for w in &self.walls {
                let cx = p.x.clamp(w.min.x, w.max.x);
                let cz = p.y.clamp(w.min.y, w.max.y);
                let d = v2(p.x - cx, p.y - cz);
                let l2 = d.len2();
                if l2 < r * r {
                    if l2 > 1e-8 {
                        let l = l2.sqrt();
                        p += d * ((r - l) / l);
                    } else {
                        // centre inside the box: exit through the nearest face
                        let dl = p.x - w.min.x;
                        let dr = w.max.x - p.x;
                        let dt = p.y - w.min.y;
                        let db = w.max.y - p.y;
                        let m = dl.min(dr).min(dt).min(db);
                        if m == dl {
                            p.x = w.min.x - r;
                        } else if m == dr {
                            p.x = w.max.x + r;
                        } else if m == dt {
                            p.y = w.min.y - r;
                        } else {
                            p.y = w.max.y + r;
                        }
                    }
                }
            }
        }
        p.x = p.x.clamp(1.0 + r, SIZE - 1.0 - r);
        p.y = p.y.clamp(1.0 + r, SIZE - 1.0 - r);
        p
    }

    /// True when the segment a-b does not cross a wall taller than knee height.
    pub fn line_of_sight(&self, a: V2, b: V2) -> bool {
        let d = b - a;
        let steps = (d.len() / 0.5).ceil().max(1.0) as i32;
        for i in 1..steps {
            let p = a + d * (i as f32 / steps as f32);
            if !self.is_walkable(p) {
                return false;
            }
        }
        true
    }

    /// Recompute the BFS distance field towards `target` (only when it moved cells).
    pub fn update_flow(&mut self, target: V2) {
        let Some(tc) = nav_cell(target) else { return };
        if tc == self.flow_target {
            return;
        }
        self.flow_target = tc;
        self.flow.iter_mut().for_each(|d| *d = NAV_INF);
        self.queue.clear();
        let ti = tc.1 * NAV + tc.0;
        self.flow[ti] = 0;
        self.queue.push(ti as u32);
        let mut head = 0;
        while head < self.queue.len() {
            let i = self.queue[head] as usize;
            head += 1;
            let (x, z) = ((i % NAV) as i32, (i / NAV) as i32);
            let nd = self.flow[i] + 1;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= NAV as i32 || nz >= NAV as i32 {
                    continue;
                }
                let ni = nz as usize * NAV + nx as usize;
                if self.blocked[ni] || self.flow[ni] <= nd {
                    continue;
                }
                self.flow[ni] = nd;
                self.queue.push(ni as u32);
            }
        }
    }

    /// Direction a zombie at `p` should walk to approach the flow target.
    pub fn flow_dir(&self, p: V2) -> Option<V2> {
        let (x, z) = nav_cell(p)?;
        let here = self.flow[z * NAV + x];
        let mut best = here;
        let mut dir = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x as i32 + dx, z as i32 + dz);
            if nx < 0 || nz < 0 || nx >= NAV as i32 || nz >= NAV as i32 {
                continue;
            }
            // no corner cutting through blocked cells
            if dx != 0 && dz != 0 && (self.blocked[z * NAV + nx as usize] || self.blocked[nz as usize * NAV + x]) {
                continue;
            }
            let d = self.flow[nz as usize * NAV + nx as usize];
            if d < best {
                best = d;
                dir = Some(v2(nx as f32 + 0.5 - p.x, nz as f32 + 0.5 - p.y).norm());
            }
        }
        dir
    }

    /// Random walkable spawn point between `min_d` and `max_d` from `from`.
    pub fn spawn_point(&self, rng: &mut Rng, from: V2, min_d: f32, max_d: f32) -> V2 {
        for _ in 0..64 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let d = rng.range(min_d, max_d);
            let p = from + V2::from_angle(a) * d;
            if p.x > 3.0 && p.y > 3.0 && p.x < SIZE - 3.0 && p.y < SIZE - 3.0 && self.is_walkable(p) {
                if let Some((x, z)) = nav_cell(p) {
                    if self.flow[z * NAV + x] != NAV_INF {
                        return p;
                    }
                }
            }
        }
        // fall back to map corners on the road ring
        let corners = [v2(18.0, 18.0), v2(78.0, 18.0), v2(18.0, 78.0), v2(78.0, 78.0)];
        corners[rng.below(4) as usize]
    }
}

fn nav_cell(p: V2) -> Option<(usize, usize)> {
    if p.x < 0.0 || p.y < 0.0 {
        return None;
    }
    let (x, z) = (p.x as usize, p.y as usize);
    if x >= NAV || z >= NAV {
        None
    } else {
        Some((x, z))
    }
}

/// Rectangular ruin with door gaps and broken segments.
fn building(walls: &mut Vec<Wall>, rng: &mut Rng, lo: V2, hi: V2) {
    let th = 0.5;
    let sides = [
        (lo, v2(hi.x, lo.y + th), true),
        (v2(lo.x, hi.y - th), hi, true),
        (lo, v2(lo.x + th, hi.y), false),
        (v2(hi.x - th, lo.y), hi, false),
    ];
    for (a, b, horizontal) in sides {
        let len = if horizontal { b.x - a.x } else { b.y - a.y };
        let door = rng.range(1.5, (len - 4.0).max(1.6));
        let door_w = 2.6;
        let mut s = 0.0;
        while s < len {
            let seg_end = if s < door { door.min(len) } else { len };
            let seg_len = seg_end - s;
            if seg_len > 0.4 {
                // split long segments into chunks of varying (broken) height
                let chunks = (seg_len / 3.0).ceil().max(1.0) as i32;
                let cl = seg_len / chunks as f32;
                for c in 0..chunks {
                    if rng.chance(0.12) {
                        continue; // collapsed piece
                    }
                    let s0 = s + c as f32 * cl;
                    let s1 = s0 + cl;
                    let height = if rng.chance(0.3) { rng.range(0.9, 1.6) } else { rng.range(2.2, 2.8) };
                    let style = rng.below(3) as u8;
                    let w = if horizontal {
                        Wall { min: v2(a.x + s0, a.y), max: v2(a.x + s1, b.y), height, style }
                    } else {
                        Wall { min: v2(a.x, a.y + s0), max: v2(b.x, a.y + s1), height, style }
                    };
                    walls.push(w);
                }
            }
            s = if s < door { door + door_w } else { len };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_walkable_and_flow_reaches_most_cells() {
        let mut w = World::generate(1337);
        assert!(w.is_walkable(w.player_start));
        w.update_flow(w.player_start);
        let reachable = w.flow.iter().filter(|d| **d != NAV_INF).count();
        let open = w.blocked.iter().filter(|b| !**b).count();
        // nearly every open cell must be reachable (buildings have doors)
        assert!(reachable as f32 > open as f32 * 0.9, "{reachable} of {open}");
    }

    #[test]
    fn collide_pushes_out_of_walls() {
        let w = World::generate(1337);
        let wall = w.walls[10];
        let inside = wall.center();
        let out = w.collide(inside, 0.4);
        let cx = out.x.clamp(wall.min.x, wall.max.x);
        let cz = out.y.clamp(wall.min.y, wall.max.y);
        assert!(v2(out.x - cx, out.y - cz).len() >= 0.39);
    }

    #[test]
    fn flow_dir_points_toward_target() {
        let mut w = World::generate(1337);
        let target = w.player_start;
        w.update_flow(target);
        let p = target + v2(6.0, 0.0);
        let d = w.flow_dir(p).expect("direction");
        assert!(d.x < 0.0);
    }
}

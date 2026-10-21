//! Hand-built level: a ruined downtown crossroads. Static props, generated
//! surfaces/walls, lights, collision boxes and a flow field for zombie pathing.

use crate::math::{v2, v3, Rng, V2, V3};

pub const NAV_CELL: f32 = 0.5;
pub const MIN_X: f32 = -22.0;
pub const MAX_X: f32 = 22.0;
pub const MIN_Z: f32 = -58.0;
pub const MAX_Z: f32 = 58.0;
const NX: usize = ((MAX_X - MIN_X) / NAV_CELL) as usize;
const NZ: usize = ((MAX_Z - MIN_Z) / NAV_CELL) as usize;
const INF: u16 = u16::MAX;

/// Named models the level places (must exist in assets_gen::MODELS).
pub const M_FACADE: &str = "facade_a.wxm";
pub const M_FACADE_B: &str = "facade_b.wxm";
pub const M_CAR: &str = "covered_car.wxm";
pub const M_BARRIER: &str = "concrete_road_barrier.wxm";
pub const M_LAMP: &str = "street_lamp_01.wxm";
pub const M_TYRE: &str = "old_tyre.wxm";
pub const M_BAG: &str = "trashbag.wxm";
pub const M_BIN: &str = "metal_trash_can.wxm";
pub const M_CRATE: &str = "wooden_crate_01.wxm";
pub const M_UTILITY: &str = "utility_box_01.wxm";
pub const M_HYDRANT: &str = "fire_hydrant.wxm";
pub const M_TRUNK: &str = "dead_tree_trunk.wxm";
pub const M_BARREL: &str = "Barrel_01.wxm";
pub const M_MEDKIT: &str = "medical_box.wxm";

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: V2,
    pub max: V2,
}

impl Aabb {
    pub fn contains(&self, p: V2, pad: f32) -> bool {
        p.x > self.min.x - pad && p.x < self.max.x + pad && p.y > self.min.y - pad && p.y < self.max.y + pad
    }
}

#[derive(Clone, Debug)]
pub struct Prop {
    pub model: &'static str,
    pub pos: V3,
    pub yaw: f32,
    pub collide: bool,
}

/// Generated geometry: an axis-aligned box or flat quad with a tiling surface.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub min: V3,
    pub max: V3,
    pub surface: &'static str,
    /// texture repeat size in metres
    pub tile: f32,
    pub collide: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: V3,
    pub color: V3,
    pub radius: f32,
    pub flicker: f32,
}

pub struct Level {
    pub props: Vec<Prop>,
    pub blocks: Vec<Block>,
    pub lights: Vec<Light>,
    pub colliders: Vec<Aabb>,
    pub spawns: Vec<V2>,
    pub player_start: V2,
    pub extraction: V2,
    pub medkit_spots: Vec<V2>,
    blocked: Vec<bool>,
    flow: Vec<u16>,
    flow_cell: usize,
    queue: Vec<u32>,
}

/// Local XZ bounds of a model (from the loaded mesh).
pub type BoundsFn<'a> = &'a dyn Fn(&str) -> (V3, V3);

fn rot(p: V2, yaw: f32) -> V2 {
    // rotation about +Y: local +Z -> (sin, cos)
    let (s, c) = yaw.sin_cos();
    v2(p.x * c + p.y * s, -p.x * s + p.y * c)
}

impl Level {
    pub fn build(bounds: BoundsFn) -> Level {
        let mut l = Level {
            props: Vec::new(),
            blocks: Vec::new(),
            lights: Vec::new(),
            colliders: Vec::new(),
            spawns: Vec::new(),
            player_start: v2(0.0, -2.0),
            extraction: v2(2.0, 45.5),
            medkit_spots: Vec::new(),
            blocked: vec![false; NX * NZ],
            flow: vec![INF; NX * NZ],
            flow_cell: usize::MAX,
            queue: Vec::with_capacity(NX * NZ),
        };
        let half_pi = std::f32::consts::FRAC_PI_2;

        // ---- ground
        let g = |l: &mut Level, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, s: &'static str, tile: f32| {
            l.blocks.push(Block { min: v3(x0, y - 0.3, z0), max: v3(x1, y, z1), surface: s, tile, collide: false });
        };
        g(&mut l, -6.0, MIN_Z, 6.0, MAX_Z, 0.0, "surface_road", 6.0);
        g(&mut l, -21.0, -5.0, -6.0, 5.0, 0.0, "surface_road", 6.0);
        g(&mut l, 6.0, -5.0, 21.0, 5.0, 0.0, "surface_road", 6.0);
        for (x0, x1) in [(-9.0, -6.0), (6.0, 9.0)] {
            g(&mut l, x0, 5.0, x1, MAX_Z, 0.14, "surface_sidewalk", 3.0);
            g(&mut l, x0, MIN_Z, x1, -5.0, 0.14, "surface_sidewalk", 3.0);
        }
        // alleys beyond the crossing: mud and rubble
        g(&mut l, -22.0, -5.0, -15.0, 5.0, 0.01, "surface_mud", 5.0);
        g(&mut l, 15.0, -5.0, 22.0, 5.0, 0.01, "surface_rubble", 4.0);
        g(&mut l, -22.0, MIN_Z, 22.0, MAX_Z, -0.02, "surface_mud", 6.0);

        // ---- buildings: eight 24 m facades lining the main street
        let (flo, fhi) = bounds(M_FACADE);
        for (k, s0) in [5.0f32, 29.0, -29.0, -53.0].iter().enumerate() {
            let a = if k % 2 == 0 { M_FACADE } else { M_FACADE_B };
            let b = if k % 2 == 0 { M_FACADE_B } else { M_FACADE };
            // west side faces +X (yaw +90°): world z = zc - local x
            l.props.push(Prop { model: a, pos: v3(-9.0, 0.0, s0 + fhi.x), yaw: half_pi, collide: false });
            // east side faces -X (yaw -90°): world z = zc + local x
            l.props.push(Prop { model: b, pos: v3(9.0, 0.0, s0 - flo.x), yaw: -half_pi, collide: false });
        }
        // building walls (collision + side walls of the alleys)
        let wall = |l: &mut Level, x0: f32, z0: f32, x1: f32, z1: f32, h: f32, s: &'static str| {
            l.blocks.push(Block { min: v3(x0, 0.0, z0), max: v3(x1, h, z1), surface: s, tile: 3.0, collide: true });
        };
        // building bodies behind the facades (seen through window openings)
        wall(&mut l, -16.0, 5.0, -9.35, 55.5, 12.5, "surface_wall2");
        wall(&mut l, -16.0, -55.5, -9.35, -5.0, 12.5, "surface_wall2");
        wall(&mut l, 9.35, 5.0, 16.0, 55.5, 12.5, "surface_wall2");
        wall(&mut l, 9.35, -55.5, 16.0, -5.0, 12.5, "surface_wall2");
        // buildings closing the far ends of both streets (scenery only)
        let pi = std::f32::consts::PI;
        l.props.push(Prop { model: M_FACADE_B, pos: v3(12.0, 0.0, 57.0), yaw: pi, collide: false });
        l.props.push(Prop { model: M_FACADE, pos: v3(-12.0, 0.0, -57.0), yaw: 0.0, collide: false });
        l.props.push(Prop { model: M_FACADE, pos: v3(-23.0, 0.0, 12.0), yaw: half_pi, collide: false });
        l.props.push(Prop { model: M_FACADE_B, pos: v3(23.0, 0.0, -12.0), yaw: -half_pi, collide: false });
        for (x0, z0, x1, z1) in [(-12.0, 57.35, 12.0, 64.0), (-12.0, -64.0, 12.0, -57.35), (-30.0, -12.0, -23.35, 12.0), (23.35, -12.0, 30.0, 12.0)] {
            l.blocks.push(Block { min: v3(x0, 0.0, z0), max: v3(x1, 12.5, z1), surface: "surface_wall2", tile: 3.0, collide: false });
        }
        // barricades closing the streets
        wall(&mut l, -9.0, 53.0, 9.0, 55.5, 3.2, "surface_plaster");
        wall(&mut l, -9.0, -55.5, 9.0, -53.0, 3.2, "surface_plaster");
        wall(&mut l, -22.0, -5.0, -20.0, 5.0, 3.2, "surface_wall");
        wall(&mut l, 20.0, -5.0, 22.0, 5.0, 3.2, "surface_wall");
        // alley side walls
        wall(&mut l, -22.0, 5.0, -16.0, 6.0, 6.0, "surface_wall");
        wall(&mut l, -22.0, -6.0, -16.0, -5.0, 6.0, "surface_wall");
        wall(&mut l, 16.0, 5.0, 22.0, 6.0, 6.0, "surface_wall");
        wall(&mut l, 16.0, -6.0, 22.0, -5.0, 6.0, "surface_wall");

        // ---- props
        let mut p = |model: &'static str, x: f32, y: f32, z: f32, yaw: f32, collide: bool| {
            l.props.push(Prop { model, pos: v3(x, y, z), yaw, collide });
        };
        // abandoned cars
        p(M_CAR, -3.6, 0.0, 18.0, 0.08, true);
        p(M_CAR, 3.9, 0.0, -15.0, pi + 0.12, true);
        p(M_CAR, 1.2, 0.0, 33.0, 0.7, true);
        p(M_CAR, -2.8, 0.0, -36.0, -0.35, true);
        p(M_CAR, 4.0, 0.0, 44.0, pi - 0.05, true);
        p(M_CAR, -12.0, 0.0, 1.5, half_pi + 0.2, true);
        p(M_CAR, 13.5, 0.0, -2.0, -half_pi + 0.3, true);
        p(M_CAR, -1.5, 0.0, 49.5, half_pi + 0.1, true);
        // barrier chicanes
        for (x, z, yaw) in [(-4.0, 8.0, 0.0), (-2.4, 8.3, 0.15), (2.6, 26.0, -0.1), (4.2, 26.2, 0.0), (-4.5, -24.0, 0.3), (-2.9, -24.4, 0.1),
            (2.0, -46.0, 0.0), (3.6, -45.8, -0.2), (-17.0, -3.0, half_pi), (-17.0, 3.0, half_pi), (17.5, 0.0, half_pi)] {
            p(M_BARRIER, x, 0.0, z, yaw, true);
        }
        // street lamps along both sidewalks
        let mut li = 0;
        for z in [-47.0, -34.0, -21.0, 9.0, 22.0, 35.0, 48.0] {
            for side in [-1.0, 1.0] {
                let x = side * 8.4;
                l.props.push(Prop { model: M_LAMP, pos: v3(x, 0.14, z), yaw: if side < 0.0 { 0.0 } else { pi }, collide: true });
                li += 1;
                // some lamps are dead; the rest flicker a little
                if li % 3 != 0 {
                    l.lights.push(Light { pos: v3(x - side * 0.3, 3.7, z), color: v3(1.0, 0.72, 0.42), radius: 11.0, flicker: (li % 4) as f32 * 0.25 });
                }
            }
        }
        // sidewalk clutter
        let mut rng = Rng::new(77);
        for side in [-1.0f32, 1.0] {
            for z in [-50.0f32, -41.0, -30.0, -12.0, 14.0, 30.0, 43.0] {
                let x = side * 8.3;
                let z = z + rng.range(-2.0, 2.0);
                match rng.below(5) {
                    0 => {
                        l.props.push(Prop { model: M_BIN, pos: v3(x, 0.14, z), yaw: if side < 0.0 { half_pi } else { -half_pi }, collide: true });
                        l.props.push(Prop { model: M_BAG, pos: v3(x - side * 0.6, 0.14, z + 1.0), yaw: rng.range(0.0, 6.0), collide: false });
                    }
                    1 => l.props.push(Prop { model: M_UTILITY, pos: v3(x + side * 0.3, 0.14, z), yaw: if side < 0.0 { half_pi } else { -half_pi }, collide: true }),
                    2 => l.props.push(Prop { model: M_HYDRANT, pos: v3(side * 6.5, 0.14, z), yaw: if side < 0.0 { half_pi } else { -half_pi }, collide: true }),
                    3 => {
                        for k in 0..3 {
                            l.props.push(Prop { model: M_BAG, pos: v3(x - side * rng.range(0.0, 0.9), 0.14, z + k as f32 * 0.5), yaw: rng.range(0.0, 6.0), collide: false });
                        }
                    }
                    _ => {
                        l.props.push(Prop { model: M_BARREL, pos: v3(x, 0.14, z), yaw: rng.range(0.0, 6.0), collide: true });
                        l.props.push(Prop { model: M_CRATE, pos: v3(x - side * 0.7, 0.14, z + 0.8), yaw: rng.range(0.0, 3.0), collide: false });
                    }
                }
            }
        }
        // road debris
        for (x, z, yaw) in [(-1.0, -6.0, 0.4), (2.5, 14.0, 1.2), (-2.0, 40.0, -0.6), (1.0, -30.0, 2.0), (-14.0, -1.0, 0.3)] {
            l.props.push(Prop { model: M_TRUNK, pos: v3(x, 0.0, z), yaw, collide: false });
        }
        for (x, z) in [(-0.5, 21.0), (2.0, -18.0), (-5.0, 3.0), (5.2, 31.0), (14.0, 3.0), (-3.5, -48.0)] {
            l.props.push(Prop { model: M_TYRE, pos: v3(x, 0.08, z), yaw: 0.0, collide: false });
        }

        // ---- collision from props
        for pr in &l.props {
            if !pr.collide {
                continue;
            }
            let (lo, hi) = bounds(pr.model);
            let corners = [v2(lo.x, lo.z), v2(hi.x, lo.z), v2(hi.x, hi.z), v2(lo.x, hi.z)];
            let mut mn = v2(f32::MAX, f32::MAX);
            let mut mx = v2(f32::MIN, f32::MIN);
            for c in corners {
                let w = rot(c, pr.yaw) + pr.pos.xz();
                mn = v2(mn.x.min(w.x), mn.y.min(w.y));
                mx = v2(mx.x.max(w.x), mx.y.max(w.y));
            }
            // rotated boxes are approximated by their AABB, shrunk a little
            let shrink = if (pr.yaw / half_pi).fract().abs() > 0.05 { 0.12 } else { 0.0 };
            l.colliders.push(Aabb { min: mn + v2(shrink, shrink), max: mx - v2(shrink, shrink) });
        }
        for b in &l.blocks {
            if b.collide {
                l.colliders.push(Aabb { min: v2(b.min.x, b.min.z), max: v2(b.max.x, b.max.z) });
            }
        }

        l.spawns = vec![v2(0.0, 51.5), v2(-4.5, 51.8), v2(4.5, 51.5), v2(0.0, -51.5), v2(4.0, -51.8), v2(-4.5, -51.5), v2(-19.0, 0.0), v2(19.0, 0.5), v2(-18.5, -3.0), v2(18.5, 3.0)];
        l.medkit_spots = vec![v2(-7.5, 12.0), v2(7.6, -26.0), v2(-7.4, -42.0), v2(7.5, 40.0), v2(-18.0, 2.5), v2(18.0, -3.5), v2(0.5, 47.0)];
        l.rebuild_nav();
        l
    }

    fn rebuild_nav(&mut self) {
        for z in 0..NZ {
            for x in 0..NX {
                let c = v2(MIN_X + (x as f32 + 0.5) * NAV_CELL, MIN_Z + (z as f32 + 0.5) * NAV_CELL);
                self.blocked[z * NX + x] = self.colliders.iter().any(|a| a.contains(c, 0.3));
            }
        }
    }

    pub fn ground_height(&self, p: V2) -> f32 {
        if (p.x.abs() > 6.0 && p.x.abs() < 9.0) && p.y.abs() > 5.0 {
            0.14
        } else {
            0.0
        }
    }

    pub fn walkable(&self, p: V2) -> bool {
        cell(p).map(|(x, z)| !self.blocked[z * NX + x]).unwrap_or(false)
    }

    /// Push a circle out of every collider.
    pub fn collide(&self, mut p: V2, r: f32) -> V2 {
        for _ in 0..2 {
            for a in &self.colliders {
                let cx = p.x.clamp(a.min.x, a.max.x);
                let cz = p.y.clamp(a.min.y, a.max.y);
                let d = v2(p.x - cx, p.y - cz);
                let l2 = d.len2();
                if l2 < r * r {
                    if l2 > 1e-8 {
                        let l = l2.sqrt();
                        p += d * ((r - l) / l);
                    } else {
                        let opts = [(p.x - a.min.x, v2(a.min.x - r, p.y)), (a.max.x - p.x, v2(a.max.x + r, p.y)), (p.y - a.min.y, v2(p.x, a.min.y - r)), (a.max.y - p.y, v2(p.x, a.max.y + r))];
                        p = opts.iter().min_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap().1;
                    }
                }
            }
        }
        v2(p.x.clamp(MIN_X + r, MAX_X - r), p.y.clamp(MIN_Z + r, MAX_Z - r))
    }

    pub fn line_of_sight(&self, a: V2, b: V2) -> bool {
        let d = b - a;
        let n = (d.len() / 0.4).ceil().max(1.0) as i32;
        (1..n).all(|i| self.walkable(a + d * (i as f32 / n as f32)))
    }

    pub fn update_flow(&mut self, target: V2) {
        let Some((tx, tz)) = cell(target) else { return };
        let ti = tz * NX + tx;
        if ti == self.flow_cell {
            return;
        }
        self.flow_cell = ti;
        self.flow.iter_mut().for_each(|d| *d = INF);
        self.queue.clear();
        self.flow[ti] = 0;
        self.queue.push(ti as u32);
        let mut head = 0;
        while head < self.queue.len() {
            let i = self.queue[head] as usize;
            head += 1;
            let (x, z) = ((i % NX) as i32, (i / NX) as i32);
            let nd = self.flow[i] + 1;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= NX as i32 || nz >= NZ as i32 {
                    continue;
                }
                let ni = nz as usize * NX + nx as usize;
                if !self.blocked[ni] && self.flow[ni] > nd {
                    self.flow[ni] = nd;
                    self.queue.push(ni as u32);
                }
            }
        }
    }

    pub fn flow_dir(&self, p: V2) -> Option<V2> {
        let (x, z) = cell(p)?;
        let mut best = self.flow[z * NX + x];
        let mut dir = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x as i32 + dx, z as i32 + dz);
            if nx < 0 || nz < 0 || nx >= NX as i32 || nz >= NZ as i32 {
                continue;
            }
            if dx != 0 && dz != 0 && (self.blocked[z * NX + nx as usize] || self.blocked[nz as usize * NX + x]) {
                continue;
            }
            let d = self.flow[nz as usize * NX + nx as usize];
            if d < best {
                best = d;
                let c = v2(MIN_X + (nx as f32 + 0.5) * NAV_CELL, MIN_Z + (nz as f32 + 0.5) * NAV_CELL);
                dir = Some((c - p).norm());
            }
        }
        dir
    }

    pub fn reachable(&self, p: V2) -> bool {
        cell(p).map(|(x, z)| self.flow[z * NX + x] != INF).unwrap_or(false)
    }
}

fn cell(p: V2) -> Option<(usize, usize)> {
    let x = ((p.x - MIN_X) / NAV_CELL).floor();
    let z = ((p.y - MIN_Z) / NAV_CELL).floor();
    if x < 0.0 || z < 0.0 || x >= NX as f32 || z >= NZ as f32 {
        None
    } else {
        Some((x as usize, z as usize))
    }
}

/// Bounds stub for tests / tools: centred boxes from the generated prop sizes.
pub fn approx_bounds(name: &str) -> (V3, V3) {
    let key = name.trim_end_matches(".wxm");
    let s = crate::assets_gen::PROP_SIZES.iter().find(|(n, _)| *n == key).map(|(_, s)| *s).unwrap_or([1.0, 1.0, 1.0]);
    if key.starts_with("facade_") {
        return (v3(0.0, -0.75, -0.3), v3(24.0, 12.75, 0.45));
    }
    (v3(-s[0] * 0.5, 0.0, -s[2] * 0.5), v3(s[0] * 0.5, s[1], s[2] * 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_spawns_are_connected() {
        let mut l = Level::build(&approx_bounds);
        assert!(l.walkable(l.player_start));
        l.update_flow(l.player_start);
        for s in &l.spawns {
            assert!(l.reachable(*s), "spawn {s:?} unreachable");
        }
        assert!(l.reachable(l.extraction));
        for m in &l.medkit_spots {
            assert!(l.reachable(*m), "medkit {m:?}");
        }
    }

    #[test]
    fn facades_line_the_street() {
        let l = Level::build(&approx_bounds);
        let f: Vec<_> = l.props.iter().filter(|p| p.model.starts_with("facade_")).collect();
        assert_eq!(f.len(), 12);
        // first west facade spans z 5 .. 29
        assert!((f[0].pos.z - 24.0 - 5.0).abs() < 0.01);
    }
}

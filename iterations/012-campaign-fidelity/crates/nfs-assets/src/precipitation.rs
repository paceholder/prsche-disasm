//! Rain and snow (`docs/re/weather.md`): two camera-space particle boxes of the weather
//! manager `[0x627e08]` (built by 0x458c90, filled by 0x459260, updated and drawn by
//! 0x459020 at the end of the race render 0x468ae0).
//!
//! Each system (0x456ed0) keeps up to 401 particles in the camera's frame inside a box in
//! front of the eye. Every frame (0x4567d0) the particles are turned and moved by the
//! camera's own motion and the wind velocity (so they stay fixed in the world while the box
//! follows the camera), particles leaving the box re-enter on the opposite face (0x457040),
//! and the count follows the intensity of the weather keys (0x456b00). Each particle is one
//! textured triangle hanging along the world's down axis (0x4565f0).

use crate::particles::ParticleRandom;

/// Particles per system (`0x20e8` flag bytes, 0x4b3 = 3 · 401 vertices).
pub const MAX_PARTICLES: usize = 401;

/// The generator `0x538040` (multi-word add-with-carry; `0x5561d0` = low 16 bits / 65536).
/// State as in the data section (`[0x5de910..0x5de924]`); the seeding call `0x538110` from
/// render init is not replayed (approximation A280).
#[derive(Debug, Clone, PartialEq)]
pub struct GameRandom {
    s: [u32; 6],
}

impl Default for GameRandom {
    fn default() -> Self {
        Self { s: [0xf22d_0e56, 0x8831_26e9, 0xc624_dd2f, 0x0702_c49c, 0x9e35_3f7d, 0x6fdf_3b64] }
    }
}

impl GameRandom {
    /// `0x538040`: `s5 += 1`, then each word adds the next one plus carry; returns `s0`.
    pub fn next_raw(&mut self) -> u32 {
        let [s0, s1, s2, s3, s4, s5] = self.s;
        let a = s4.wrapping_add(s5);
        let c = u32::from(a < s5 || a < s4);
        let b = a.wrapping_add(c).wrapping_add(s3);
        let c2 = u32::from(b < s3);
        let d = b.wrapping_add(c2).wrapping_add(s2);
        let c3 = u32::from(d < s2);
        let e = d.wrapping_add(c3).wrapping_add(s1);
        let c4 = u32::from(e < s1);
        let mut n = [s0.wrapping_add(e).wrapping_add(c4), e, d, b, a, s5.wrapping_add(1)];
        if n[5] == 0 {
            n[4] = n[4].wrapping_add(1);
            if n[4] == 0 {
                n[3] = n[3].wrapping_add(1);
                if n[3] == 0 {
                    n[2] = n[2].wrapping_add(1);
                    if n[2] == 0 {
                        n[1] = n[1].wrapping_add(1);
                        if n[1] == 0 {
                            n[0] = n[0].wrapping_add(1);
                        }
                    }
                }
            }
        }
        self.s = n;
        n[0]
    }

    /// `0x5561d0`: `(u16) 0x538040() · 1/65536`.
    pub fn unit(&mut self) -> f32 {
        (self.next_raw() & 0xffff) as f32 * 1.525_878_9e-5
    }
}

/// Parameters of one system (the 0x15 dwords copied to `+0x8604` by 0x456ed0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecipitationConfig {
    /// Camera-space box: x min/max/range, y min/max/range, z min/max/range.
    pub x: [f32; 3],
    pub y: [f32; 3],
    pub z: [f32; 3],
    /// `+0x862c`: particles at intensity 1 (SPECIALFX 0..3: 60, 140, 220, 300; 0x44ed10).
    pub count: f32,
    /// `+0x8630` triangle width, `+0x8634` height.
    pub width: f32,
    pub height: f32,
    /// `+0x8638`: world velocity per 64 Hz tick (original axes).
    pub velocity: [f32; 3],
    /// `+0x8644`: gust velocity per tick (original axes).
    pub gust: [f32; 3],
    /// `+0x8650`: texture in `particle.fsh` (`sno0..3`/`ran0..3` by detail, else `snow`/`rain`).
    pub texture: &'static str,
    /// `+0x8654`: snow flutter (per-slot vertical bob, 0x457040).
    pub flutter: bool,
}

/// SPECIALFX 3 (`0x6197f0` = 300).
pub const FULL_COUNT: f32 = 300.0;

/// Snow of 0x458c90: box x ±15, y −3..10, z 1..30; triangles 0.2 × 0.2; drifts 0.01/tick
/// east and falls 0.01/tick; gusts (0.02, −0.02, 0.15).
pub const SNOW: PrecipitationConfig = PrecipitationConfig {
    x: [-15.0, 15.0, 30.0],
    y: [-3.0, 10.0, 13.0],
    z: [1.0, 30.0, 29.0],
    count: FULL_COUNT,
    width: 0.2,
    height: 0.2,
    velocity: [0.01, -0.01, 0.0],
    gust: [0.02, -0.02, 0.15],
    texture: "snow",
    flutter: true,
};

/// Rain of 0x458c90: box x ±15, y −3..10, z 2.5..15; triangles 0.2 × 0.5; falls 0.8/tick;
/// gusts (0, −0.32, 0).
pub const RAIN: PrecipitationConfig = PrecipitationConfig {
    x: [-15.0, 15.0, 30.0],
    y: [-3.0, 10.0, 13.0],
    z: [2.5, 15.0, 12.5],
    count: FULL_COUNT,
    width: 0.2,
    height: 0.5,
    velocity: [0.0, -0.8, 0.0],
    gust: [0.0, -0.32, 0.0],
    texture: "rain",
    flutter: false,
};

/// Camera pose in original axes: rows of the world-to-camera rotation (right, up,
/// forward; `+0xb8`) and the eye (`+0x10c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub rows: [[f32; 3]; 3],
    pub eye: [f32; 3],
}

impl CameraPose {
    /// World vector to camera space.
    pub fn to_camera(&self, v: [f32; 3]) -> [f32; 3] {
        self.rows.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
    }
    /// Camera-space vector to world.
    pub fn to_world(&self, v: [f32; 3]) -> [f32; 3] {
        let r = &self.rows;
        [0, 1, 2].map(|k| r[0][k] * v[0] + r[1][k] * v[1] + r[2][k] * v[2])
    }
}

/// `0x5cee14`: `4096 / t²` for `t` ticks (1..10), turning the squared displacement into
/// (units per second)² at 64 Hz.
fn speed_scale(ticks: u32) -> f32 {
    let t = ticks.clamp(1, 10) as f32;
    4096.0 / (t * t)
}

#[derive(Debug, Clone)]
pub struct Precipitation {
    pub config: PrecipitationConfig,
    /// `+0`: intensity the target was computed from.
    intensity: f32,
    /// `+4` live slots, `+8` target count.
    count: usize,
    target: usize,
    /// Camera-space positions and the active flags (`+0x83a0`).
    positions: Vec<[f32; 3]>,
    active: Vec<bool>,
    /// `+0x85c4` gust level, `+0x85c8` gust target.
    gust: f32,
    gust_target: f32,
    /// Snow flutter: 8 phases (`+0x8534`), per-slot offsets (`+0x8554`), last sine (`+0x85b4`).
    phases: [i32; 8],
    offsets: [[f32; 3]; 8],
    last_sine: f32,
    sine: [f32; 128],
    /// Camera pose of the last update (`+0x85cc`), `None` before the first.
    last_pose: Option<CameraPose>,
    /// `+0x8600`: horizontal camera speed², (units/s)².
    speed_sq: f32,
}

impl Precipitation {
    /// `0x456ed0`.
    pub fn new(config: PrecipitationConfig, game_random: &mut GameRandom) -> Self {
        let mut s = Self {
            config,
            intensity: 0.0,
            count: 0,
            target: 0,
            positions: vec![[0.0; 3]; MAX_PARTICLES],
            active: vec![false; MAX_PARTICLES],
            gust: 0.0,
            gust_target: 0.0,
            phases: [0; 8],
            offsets: [[0.0; 3]; 8],
            last_sine: 0.0,
            sine: std::array::from_fn(|i| (i as f32 * 0.0078125 * std::f32::consts::TAU).sin()),
            last_pose: None,
            speed_sq: 0.0,
        };
        s.reset_flutter(game_random);
        s
    }

    fn reset_flutter(&mut self, game_random: &mut GameRandom) {
        self.active.iter_mut().for_each(|a| *a = false);
        for k in 0..8 {
            // ftol(0x5561d0() · 1000)
            self.phases[k] = (game_random.unit() * 1000.0) as i32;
            self.offsets[k] = [0.0; 3];
        }
        self.last_sine = 0.0;
    }

    /// `round(intensity · count − 0.5)`, below 3 none.
    fn set_intensity(&mut self, intensity: f32) {
        if self.intensity != intensity {
            self.intensity = intensity;
            let t = (intensity * self.config.count - 0.5).round_ties_even() as i32;
            self.target = if t < 3 { 0 } else { (t as usize).min(MAX_PARTICLES) };
        }
    }

    fn random_in(&self, random: &mut ParticleRandom, axis: [f32; 3]) -> f32 {
        random.next_raw() as f32 * axis[2] * 1.525_878_9e-5 + axis[0]
    }

    fn park(&mut self, i: usize) {
        let c = &self.config;
        self.active[i] = false;
        self.positions[i] = [c.x[0] - c.x[2], c.y[0] - c.y[2], c.z[0] - c.z[2]];
    }

    /// Full refill (the body of 0x459260, also 0x456b00 and 0x457040 on a camera jump).
    fn refill(&mut self, intensity: f32, random: &mut ParticleRandom, game_random: &mut GameRandom) {
        self.reset_flutter(game_random);
        self.set_intensity(intensity);
        self.speed_sq = 0.0;
        self.count = self.target;
        let (x, y, z) = (self.config.x, self.config.y, self.config.z);
        for i in 0..self.count {
            self.active[i] = true;
            let px = self.random_in(random, x);
            let py = self.random_in(random, y);
            let pz = self.random_in(random, z);
            self.positions[i] = [px, py, pz];
        }
    }

    /// `0x459260`: fill the box at the intensity of the start.
    pub fn fill(&mut self, intensity: f32, random: &mut ParticleRandom, game_random: &mut GameRandom) {
        self.intensity = f32::NAN;
        self.refill(intensity, random, game_random);
    }

    /// `0x456d70`: a new particle enters at the far face when the camera moves faster
    /// than 45 units/s, else at the top.
    fn spawn(&mut self, i: usize, random: &mut ParticleRandom) {
        let (x, y, z) = (self.config.x, self.config.y, self.config.z);
        self.active[i] = true;
        let px = self.random_in(random, x);
        self.positions[i] = if self.speed_sq > 2025.0 {
            let py = self.random_in(random, y);
            [px, py, z[1]]
        } else {
            let pz = self.random_in(random, z);
            [px, y[1], pz]
        };
        self.count += 1;
    }

    /// `0x456b00`: follow the target count.
    fn adjust_count(&mut self, random: &mut ParticleRandom, game_random: &mut GameRandom) {
        let (cur, tgt) = (self.count, self.target);
        if tgt <= cur * 4 || self.speed_sq <= 10000.0 {
            if cur < tgt {
                for i in cur..(cur + 20).min(tgt) {
                    self.spawn(i, random);
                }
            } else if tgt < cur {
                if tgt == 0 && self.speed_sq > 2025.0 {
                    for i in 0..cur {
                        self.park(i);
                    }
                }
                while self.target < self.count && !self.active[self.count - 1] {
                    self.count -= 1;
                }
            }
        } else {
            let intensity = self.intensity;
            self.refill(intensity, random, game_random);
        }
    }

    /// `0x4567d0(ticks, intensity)` with the camera pose of this frame.
    pub fn update(
        &mut self,
        ticks: u32,
        intensity: f32,
        pose: CameraPose,
        random: &mut ParticleRandom,
        game_random: &mut GameRandom,
    ) {
        if ticks == 0 {
            return;
        }
        let t = ticks as f32;
        // Camera displacement (previous − current, world) and the relative turn.
        let (moved, last) = match self.last_pose {
            None => ([0.0; 3], pose),
            Some(last) => ([0, 1, 2].map(|k| last.eye[k] - pose.eye[k]), last),
        };
        self.last_pose = Some(pose);
        self.speed_sq = (moved[0] * moved[0] + moved[2] * moved[2]) * speed_scale(ticks);
        self.set_intensity(intensity);
        self.adjust_count(random, game_random);
        let mut shift = [0.0; 3];
        if self.count > 0 {
            if self.gust_target <= self.gust {
                if self.gust <= self.gust_target {
                    if game_random.unit() < t * 0.0015 {
                        self.gust_target = 1.0;
                    }
                } else {
                    self.gust -= t * 0.0015;
                    if self.gust < self.gust_target {
                        self.gust = 0.0;
                        self.gust_target = 0.0;
                    }
                }
            } else {
                self.gust += t * 0.004;
                if self.gust_target < self.gust {
                    self.gust_target = 0.0;
                }
            }
            let c = &self.config;
            let calm = (1.0 - self.gust) * t;
            let mut v = c.velocity.map(|x| x * calm);
            if self.gust > 0.0 {
                for k in 0..3 {
                    v[k] += c.gust[k] * t * self.gust;
                }
            }
            let world = [0, 1, 2].map(|k| moved[k] + v[k]);
            shift = pose.to_camera(world);
        }
        self.move_particles(&last, &pose, shift, ticks, random, game_random);
    }

    /// `0x457040`.
    fn move_particles(
        &mut self,
        last: &CameraPose,
        pose: &CameraPose,
        shift: [f32; 3],
        ticks: u32,
        random: &mut ParticleRandom,
        game_random: &mut GameRandom,
    ) {
        if self.count == 0 {
            return;
        }
        let c = self.config;
        if c.flutter {
            for k in 0..8 {
                self.phases[k] = self.phases[k].wrapping_add((ticks as i32).wrapping_mul(0x80));
                let i = (self.phases[k].wrapping_mul(0x80) / 0x10000) % 128;
                let s = self.sine[i.rem_euclid(128) as usize];
                let d = s - self.last_sine;
                self.offsets[k] = [d * c.velocity[0] * self.gust * 9.0, d, d * c.velocity[2] * self.gust * 9.0];
                self.last_sine = s;
            }
        }
        for i in 0..self.count {
            if !self.active[i] && self.count < self.target {
                self.active[i] = true;
            }
            if self.active[i] {
                // Relative turn: back to the world with the last pose, into the new camera.
                let p = pose.to_camera(last.to_world(self.positions[i]));
                let mut p = [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]];
                if c.flutter {
                    let o = self.offsets[i & 7];
                    p = [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
                }
                self.positions[i] = p;
            }
        }
        let mut wrapped_total = 0usize;
        for i in 0..self.count {
            if !self.active[i] {
                continue;
            }
            let mut p = self.positions[i];
            let mut wrapped = false;
            if p[2] > c.z[1] {
                p[2] = c.z[0];
                p[0] = self.random_in(random, c.x);
                p[1] = self.random_in(random, c.y);
                wrapped = true;
            } else if p[2] < c.z[0] {
                p[2] = c.z[1];
                p[0] = self.random_in(random, c.x);
                p[1] = self.random_in(random, c.y);
                wrapped = true;
            }
            if p[0] > c.x[1] {
                p[0] = c.x[0];
                p[1] = self.random_in(random, c.y);
                p[2] = self.random_in(random, c.z);
                wrapped = true;
            } else if p[0] < c.x[0] {
                p[0] = c.x[1];
                p[1] = self.random_in(random, c.y);
                p[2] = self.random_in(random, c.z);
                wrapped = true;
            }
            if p[1] > c.y[1] {
                p[1] = c.y[0] + 0.01;
                p[0] = self.random_in(random, c.x);
                p[2] = self.random_in(random, c.z);
                wrapped = true;
            } else if p[1] < c.y[0] {
                p[1] = c.y[1] - 0.01;
                p[0] = self.random_in(random, c.x);
                p[2] = self.random_in(random, c.z);
                wrapped = true;
            }
            self.positions[i] = p;
            if wrapped {
                wrapped_total += 1;
                if self.target <= i {
                    self.park(i);
                }
            }
        }
        // A camera jump (more than half the particles wrapped): refill the box.
        if self.count > 20 && wrapped_total as f32 > self.count as f32 * 0.5 {
            let intensity = self.intensity;
            self.refill(intensity, random, game_random);
        }
    }

    pub fn live(&self) -> usize {
        self.active[..self.count].iter().filter(|a| **a).count()
    }

    /// `0x4565f0`: the triangles in camera space. `d` = world down in camera space;
    /// R = `(0, 0, −1) × d · width`, U = `(d.x · height, d.y · height, d.z)` (z unscaled as in
    /// the original); even particles `(P, P+R+U, P+U)`, odd `(P, P+R, P+R+U)`, UVs
    /// `(0,0) (1,1) (0,1)` and `(0,0) (1,0) (1,1)`.
    pub fn triangles(&self, pose: &CameraPose) -> Vec<[([f32; 3], [f32; 2]); 3]> {
        let c = &self.config;
        let d = pose.to_camera([0.0, -1.0, 0.0]);
        let r = [d[1] * c.width, -d[0] * c.width, 0.0];
        let u = [d[0] * c.height, d[1] * c.height, d[2]];
        let add = |a: [f32; 3], b: [f32; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
        let mut out = Vec::with_capacity(self.count);
        for i in 0..self.count {
            if !self.active[i] {
                continue;
            }
            let p = self.positions[i];
            out.push(if i % 2 == 0 {
                [(p, [0.0, 0.0]), (add(add(p, r), u), [1.0, 1.0]), (add(p, u), [0.0, 1.0])]
            } else {
                [(p, [0.0, 0.0]), (add(p, r), [1.0, 0.0]), (add(add(p, r), u), [1.0, 1.0])]
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(eye: [f32; 3]) -> CameraPose {
        CameraPose { rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], eye }
    }

    #[test]
    fn target_follows_intensity() {
        let mut g = GameRandom::default();
        let mut r = ParticleRandom::default();
        let mut s = Precipitation::new(RAIN, &mut g);
        s.fill(1.0, &mut r, &mut g);
        assert_eq!(s.live(), 300);
        s.set_intensity(0.005);
        assert_eq!(s.target, 0);
        s.set_intensity(0.5);
        assert_eq!(s.target, 150);
    }

    #[test]
    fn particles_stay_in_the_box_and_rain_falls() {
        let mut g = GameRandom::default();
        let mut r = ParticleRandom::default();
        let mut s = Precipitation::new(RAIN, &mut g);
        s.fill(1.0, &mut r, &mut g);
        let before = s.positions[0];
        s.update(1, 1.0, pose([0.0; 3]), &mut r, &mut g);
        let after = s.positions[0];
        // 0.8 per tick down (unless it wrapped).
        if after[1] < before[1] {
            assert!((before[1] - after[1] - 0.8).abs() < 0.2);
        }
        for (p, a) in s.positions.iter().zip(&s.active) {
            if *a {
                assert!(p[0] >= -15.0 && p[0] <= 15.0 && p[1] >= -3.0 && p[1] <= 10.0 && p[2] >= 2.5 && p[2] <= 15.0);
            }
        }
    }

    #[test]
    fn camera_motion_moves_particles_backwards() {
        let mut g = GameRandom::default();
        let mut r = ParticleRandom::default();
        let mut s = Precipitation::new(SNOW, &mut g);
        s.fill(1.0, &mut r, &mut g);
        s.update(1, 1.0, pose([0.0; 3]), &mut r, &mut g);
        let z0: Vec<f32> = s.positions.iter().map(|p| p[2]).collect();
        // Camera 0.1 forward (camera z): particles move 0.1 towards it.
        s.update(1, 1.0, pose([0.0, 0.0, 0.1]), &mut r, &mut g);
        let moved = s.positions.iter().zip(&z0).filter(|(p, z)| (*z - p[2] - 0.1).abs() < 1e-3).count();
        assert!(moved > 250, "{moved}");
    }

    #[test]
    fn triangle_hangs_down() {
        let mut g = GameRandom::default();
        let mut r = ParticleRandom::default();
        let mut s = Precipitation::new(RAIN, &mut g);
        s.fill(1.0, &mut r, &mut g);
        let tris = s.triangles(&pose([0.0; 3]));
        let [(p, _), (q, _), _] = tris[0];
        // R = (−1, 0, 0)·0.2, U = (0, −0.5, 0).
        assert!((q[0] - (p[0] - 0.2)).abs() < 1e-5 && (q[1] - (p[1] - 0.5)).abs() < 1e-5);
    }
}

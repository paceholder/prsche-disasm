//! The original generic particle system (`docs/re/particles.md`).
//!
//! `RGenericParticleSystem` (vtable 0x5b3df8): ctor 0x454250, tick 0x454c30,
//! particle init 0x454860, draw 0x454430 with the sprite expansion 0x44f6e0.
//! The manager 0x455150/0x4551e0/0x455260 keeps one system per emitter key,
//! runs at 64 Hz (0x4689c0) and drops a system that was not requested for
//! 100 ticks. Track emitters are the `ef` entries of type 3 of the track CRP
//! (0x450de0, 0x450d10); car wheel emitters follow `R3DCarParticle_Update`
//! 0x43ef80.
//!
//! Everything here is in original coordinates and original units (ticks).

use std::collections::BTreeMap;

/// Record of `particle.dat` / `carpart.dat`: a tag and 0x7c bytes. The
/// original keeps a pointer to the body after the tag, so its offsets are
/// the file offsets below minus 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleDef {
    pub tag: [u8; 4],
    /// `+0`: bit 0 bounce on the ground, bit 1 one-shot, bit 2 streaks.
    pub flags: u32,
    /// `+4`: particle count (budget before the detail scale).
    pub count: u32,
    /// `+8`: x, z position jitter; y speed along axis B.
    pub emit: [f32; 3],
    /// `+0x14`: random speed along A, B, C.
    pub rand_emit: [f32; 3],
    /// `+0x20`: horizontal velocity factor per tick.
    pub air_friction: f32,
    /// `+0x24`: texture FourCC, stored byte-swapped (`KRPS` = `SPRK`).
    pub material: [u8; 4],
    /// `+0x28`: render blend mode.
    pub blend_mode: u32,
    /// `+0x2c`: z bias.
    pub z_bias: i32,
    /// `+0x30..0x32`: frames, frame speed, colour states.
    pub num_frames: u8,
    pub frame_speed: u8,
    pub num_states: u8,
    /// `+0x3c`/`+0x40`: start and end size, a tenth of the drawn size.
    pub start_size: f32,
    pub end_size: f32,
    /// `+0x44`: lifetime in ticks.
    pub lifetime: u32,
    /// `+0x48`/`+0x4c`: emission duty cycle in ticks.
    pub period_on: u32,
    pub period_off: u32,
    /// `+0x50`: ticks run when a track system is created.
    pub prime_ticks: u32,
    /// `+0x54`/`+0x58`: gravity weight and its random part.
    pub weight: f32,
    pub rand_weight: f32,
    /// `+0x64`: random rotation, degrees per lifetime.
    pub rand_rotation: f32,
    /// `+0x68`: fraction of the far distance where emission starts to fade.
    pub fade_fraction: f32,
    /// `+0x6c`: colour states (ARGB).
    pub colors: [u32; 4],
}

impl ParticleDef {
    /// Texture name in `particle.fsh`.
    pub fn texture(&self) -> String {
        let mut m = self.material;
        m.reverse();
        String::from_utf8_lossy(&m).into_owned()
    }

    /// Particles per tick (0x454250).
    pub fn rate(&self) -> f32 {
        rate_for(self.count as f32, self)
    }

    /// Both sizes 0: the system draws no sprites (system flag `+0x2e` bit 1).
    pub fn has_sprites(&self) -> bool {
        !(self.start_size == 0.0 && self.end_size == 0.0)
    }
}

/// The count spread over the emitting ticks of one lifetime (0x454250,
/// 0x454ac0 with the budget as count).
fn rate_for(count: f32, d: &ParticleDef) -> f32 {
    let (life, on, off) = (d.lifetime, d.period_on, d.period_off);
    if off == 0 || life < on {
        count / life as f32
    } else if on + off < life {
        count / ((life as f32 / (on + off) as f32) * on as f32)
    } else {
        count / on as f32
    }
}

fn f32le(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// Parse `particle.dat` or `carpart.dat`: a u32 count and 0x80-byte records.
pub fn parse_particle_dat(data: &[u8]) -> Result<Vec<ParticleDef>, String> {
    if data.len() < 4 {
        return Err("particle dat too short".into());
    }
    let n = u32le(data, 0) as usize;
    if data.len() < 4 + n * 0x80 {
        return Err(format!("particle dat has {} bytes for {n} records", data.len()));
    }
    Ok((0..n)
        .map(|i| {
            let r = &data[4 + i * 0x80..4 + (i + 1) * 0x80];
            let v3 = |o| [f32le(r, o), f32le(r, o + 4), f32le(r, o + 8)];
            ParticleDef {
                tag: r[0..4].try_into().unwrap(),
                flags: u32le(r, 4),
                count: u32le(r, 8),
                emit: v3(0x0c),
                rand_emit: v3(0x18),
                air_friction: f32le(r, 0x24),
                material: r[0x28..0x2c].try_into().unwrap(),
                blend_mode: u32le(r, 0x2c),
                z_bias: u32le(r, 0x30) as i32,
                num_frames: r[0x34],
                frame_speed: r[0x35],
                num_states: r[0x36],
                start_size: f32le(r, 0x40),
                end_size: f32le(r, 0x44),
                lifetime: u32le(r, 0x48),
                period_on: u32le(r, 0x4c),
                period_off: u32le(r, 0x50),
                prime_ticks: u32le(r, 0x54),
                weight: f32le(r, 0x58),
                rand_weight: f32le(r, 0x5c),
                rand_rotation: f32le(r, 0x68),
                fade_fraction: f32le(r, 0x6c),
                colors: [u32le(r, 0x70), u32le(r, 0x74), u32le(r, 0x78), u32le(r, 0x7c)],
            }
        })
        .collect())
}

/// One tag map from both files, as 0x455570: `particle.dat` first (the first
/// record of a tag wins), then `carpart.dat` (replaces a record of its tag).
pub fn merge_particle_defs(particle: &[ParticleDef], carpart: &[ParticleDef]) -> Vec<ParticleDef> {
    let mut out: Vec<ParticleDef> = Vec::new();
    for d in particle {
        if !out.iter().any(|o| o.tag == d.tag) {
            out.push(*d);
        }
    }
    for d in carpart {
        match out.iter_mut().find(|o| o.tag == d.tag) {
            Some(o) => *o = *d,
            None => out.push(*d),
        }
    }
    out
}

/// The particle random generator [VA 0x454860, seed 0x44fd00]:
/// `p = state·mult`, `state = p & 0xffff`, sample `(p >> 8) & 0xffff`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleRandom {
    state: u32,
    mult: u32,
}

impl Default for ParticleRandom {
    fn default() -> Self {
        Self::seeded(0)
    }
}

impl ParticleRandom {
    /// `0x44fd00(seed)`: multiplier 123456789, state 0x3ade68b1, then
    /// `seed % 500` steps of `state = (state·mult) & 0xffff`.
    pub fn seeded(seed: u32) -> Self {
        let mult = 123_456_789u32;
        let mut state = 0x3ade_68b1u32;
        for _ in 0..seed % 500 {
            state = state.wrapping_mul(mult) & 0xffff;
        }
        Self { state, mult }
    }

    /// The raw 16-bit draw.
    pub fn next_raw(&mut self) -> u32 {
        let p = self.state.wrapping_mul(self.mult);
        self.state = p & 0xffff;
        ((p as i32 >> 8) & 0xffff) as u32
    }

    /// `r · 3.0517578e-05`, in [0, 2).
    pub fn next(&mut self) -> f32 {
        self.next_raw() as f32 * 3.051_757_8e-5
    }

    /// `base − spread + r·spread`.
    pub fn around(&mut self, base: f32, spread: f32) -> f32 {
        (base - spread) + self.next() * spread
    }

    /// `r·spread − spread`.
    fn signed(&mut self, spread: f32) -> f32 {
        self.next() * spread - spread
    }
}

/// Emitter frame (0x30 bytes): position, ground height, axes A and B.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmitterFrame {
    pub position: [f32; 3],
    /// Height for the ground bounce (flag bit 0).
    pub ground: f32,
    pub axis_a: [f32; 3],
    pub axis_b: [f32; 3],
}

impl Default for EmitterFrame {
    /// The car slots start with A = (1,0,0), B = (0,1,0) (0x43ee10).
    fn default() -> Self {
        Self { position: [0.0; 3], ground: 0.0, axis_a: [1.0, 0.0, 0.0], axis_b: [0.0, 1.0, 0.0] }
    }
}

impl EmitterFrame {
    /// The frame at `+8` of an `ef` payload.
    pub fn from_bytes(d: &[u8]) -> Option<Self> {
        if d.len() < 0x28 {
            return None;
        }
        let v3 = |o| [f32le(d, o), f32le(d, o + 4), f32le(d, o + 8)];
        Some(Self { position: v3(0), ground: f32le(d, 0xc), axis_a: v3(0x10), axis_b: v3(0x1c) })
    }
}

/// `Vec3_Cross` 0x532880.
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub position: [f32; 3],
    /// Per tick.
    pub velocity: [f32; 3],
    /// Ticks (a float in the original).
    pub age: f32,
    pub gravity: f32,
    /// Degrees over the lifetime.
    pub rotation: f32,
}

/// One drawn particle (0x454430): original coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub position: [f32; 3],
    /// Full width of the camera-facing square.
    pub size: f32,
    /// ARGB.
    pub color: u32,
    /// Degrees about the view axis.
    pub rotation: f32,
}

/// `RGenericParticleSystem`.
#[derive(Debug, Clone)]
pub struct ParticleSystem {
    pub def: ParticleDef,
    pub frame: EmitterFrame,
    /// `+0x30`: C = A × B.
    pub axis_c: [f32; 3],
    /// Live particles, oldest first (the original list is newest first).
    pub particles: Vec<Particle>,
    /// `+0x2c`.
    pub active: bool,
    /// `+0xc`: period counter.
    pub counter: u32,
    /// `+0x14` / `+0x18`.
    emitted: f32,
    accumulator: f32,
    /// `+0x40`: particles per tick.
    pub rate: f32,
    /// `+0x54`: `count·detail/256`.
    pub budget: usize,
    /// `+0x44`: emission scale (the wheel effects write the skid).
    pub scale: f32,
    /// `+0x10`: camera distance² of the owner.
    pub distance_sq: f32,
    /// `+8`: 100 when requested since the last manager tick.
    pub request: i32,
}

/// Full detail (`0x6197e0` = 0x100 at SPECIALFX 3; 0, 100, 200 below).
pub const DETAIL_FULL: u32 = 0x100;

impl ParticleSystem {
    /// Ctor 0x454250 (without the prime ticks).
    pub fn new(def: ParticleDef, frame: EmitterFrame, detail: u32, distance_sq: f32) -> Self {
        Self {
            def,
            frame,
            axis_c: cross(frame.axis_a, frame.axis_b),
            particles: Vec::new(),
            active: def.flags & 2 == 0,
            counter: 0,
            emitted: 0.0,
            accumulator: 0.0,
            rate: def.rate(),
            budget: ((def.count * detail) >> 8) as usize,
            scale: 1.0,
            distance_sq,
            request: 100,
        }
    }

    /// Static track system (`RGenericParticleCreator` 0x455000): created with
    /// its `primeTicks` run at once.
    pub fn primed(
        def: ParticleDef,
        frame: EmitterFrame,
        detail: u32,
        distance_sq: f32,
        far: f32,
        random: &mut ParticleRandom,
    ) -> Self {
        let mut s = Self::new(def, frame, detail, distance_sq);
        for _ in 0..def.prime_ticks {
            s.tick(far, random);
        }
        s
    }

    /// 0x454ac0: switch to another record (car road dirt by surface);
    /// the budget and rate follow, live particles are dropped when the
    /// count changes.
    pub fn retag(&mut self, def: ParticleDef, detail: u32) {
        if def.tag == self.def.tag {
            return;
        }
        let budget = (def.count * detail) >> 8;
        if def.count != self.def.count {
            self.particles.clear();
        }
        self.def = def;
        self.budget = budget as usize;
        self.rate = rate_for(budget as f32, &def);
    }

    /// Start 0x454420: active, period counter 0.
    pub fn start(&mut self) {
        self.active = true;
        self.counter = 0;
    }

    /// Set the frame and recompute C.
    pub fn set_frame(&mut self, frame: EmitterFrame) {
        self.frame = frame;
        self.axis_c = cross(frame.axis_a, frame.axis_b);
    }

    /// Emission fade by the camera distance (0x454c30): 1 within `k·far`,
    /// then linear to 0 at `far`.
    pub fn distance_fade(&self, far: f32) -> f32 {
        let k = self.def.fade_fraction;
        let near = far * k;
        if near * near < self.distance_sq {
            // The original's square root: (bits >> 1) + 0x1fc00000.
            let d = f32::from_bits(((self.distance_sq.to_bits() as i32 >> 1) + 0x1fc0_0000) as u32);
            (far - d) / ((1.0 - k) * far)
        } else {
            1.0
        }
    }

    /// One tick (0x454c30). `far` is the far view distance (`0x6197cc`).
    pub fn tick(&mut self, far: f32, random: &mut ParticleRandom) {
        if self.particles.is_empty() && !self.active {
            return;
        }
        let d = self.def;
        let (on, off) = (d.period_on, d.period_off);
        let mut advance = true;
        if !self.active || (off != 0 && on < self.counter) {
            if on.wrapping_add(off) < self.counter {
                self.counter = self.counter - on - off;
                if d.flags & 2 != 0 {
                    self.active = false;
                }
                advance = false;
            }
        } else {
            let f = self.distance_fade(far);
            self.accumulator += f * self.scale * self.rate;
            if self.emitted + 1.0 < self.accumulator {
                loop {
                    if self.particles.len() < self.budget {
                        let p = self.spawn(random);
                        self.particles.push(p);
                    }
                    self.emitted += 1.0;
                    if !(self.emitted + 1.0 < self.accumulator) {
                        break;
                    }
                }
            }
        }
        if advance {
            self.counter = self.counter.wrapping_add(1);
        }
        let (ground, bounce) = (self.frame.ground, d.flags & 1 != 0);
        let life = d.lifetime as f32;
        let af = d.air_friction;
        self.particles.retain_mut(|p| {
            p.position[0] += p.velocity[0];
            let y = p.position[1];
            p.position[1] = p.velocity[1] + y;
            p.position[2] += p.velocity[2];
            if bounce && p.velocity[1] + y < ground {
                p.position[1] = ground;
                p.velocity[1] *= -0.5;
            }
            p.velocity[1] -= p.gravity;
            if af < 1.0 {
                p.velocity[0] *= af;
            }
            p.velocity[2] *= af;
            p.age += 1.0;
            p.age <= life
        });
    }

    /// Particle init 0x454860.
    fn spawn(&mut self, r: &mut ParticleRandom) -> Particle {
        let d = self.def;
        let mut position = self.frame.position;
        position[0] += r.signed(d.emit[0]);
        position[2] += r.signed(d.emit[2]);
        let main = r.around(d.emit[1], d.rand_emit[1]);
        let a = r.signed(d.rand_emit[0]);
        let c = r.signed(d.rand_emit[2]);
        let (fa, fb, fc) = (self.frame.axis_a, self.frame.axis_b, self.axis_c);
        let velocity: [f32; 3] = std::array::from_fn(|k| fb[k] * main + fa[k] * a + fc[k] * c);
        let gravity = r.around(d.weight, d.rand_weight) * 0.005;
        let rotation = r.signed(d.rand_rotation);
        Particle { position, velocity, age: 0.0, gravity, rotation }
    }

    /// Colour of a particle at `t = age/life` (0x454430): ARGB bytes plus the
    /// truncated delta to the next state.
    pub fn color_at(&self, t: f32) -> u32 {
        let d = &self.def;
        let lerp = |a: u32, b: u32, f: f32| -> u32 {
            let mut out = 0u32;
            for shift in [24, 16, 8, 0] {
                let (x, y) = ((a >> shift) & 255, (b >> shift) & 255);
                let delta = y as f32 - x as f32;
                let byte = (x as u8).wrapping_add((delta * f) as i32 as u8);
                out |= (byte as u32) << shift;
            }
            out
        };
        match d.num_states {
            2 => lerp(d.colors[0], d.colors[1], t),
            3 | 4 => {
                let t = if t >= 1.0 { f32::from_bits(0x3f7f_f972) } else { t };
                let x = (d.num_states - 1) as f32 * t;
                let i = ((x - 0.5).round_ties_even() as i32).clamp(0, d.num_states as i32 - 2) as usize;
                lerp(d.colors[i], d.colors[i + 1], x - i as f32)
            }
            _ => d.colors[0],
        }
    }

    /// Drawn particles, newest first as the original list (0x454430).
    pub fn sprites(&self) -> impl Iterator<Item = Sprite> + '_ {
        let d = self.def;
        let life = d.lifetime.max(1) as f32;
        self.particles.iter().rev().map(move |p| {
            let t = p.age / life;
            Sprite {
                position: p.position,
                size: ((d.end_size - d.start_size) * t + d.start_size) * 10.0,
                color: self.color_at(t),
                rotation: t * p.rotation,
            }
        })
    }
}

/// A track emitter: `ef` type 3 on a static article.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackEmitter {
    pub tag: [u8; 4],
    pub frame: EmitterFrame,
    pub article_index: usize,
}

/// Parse an `ef` payload; `Some` for type 3 (particle emitter) [VA 0x450d10].
pub fn parse_ef_emitter(payload: &[u8], article_index: usize) -> Option<TrackEmitter> {
    if payload.len() < 0x34 || u32le(payload, 0) != 3 {
        return None;
    }
    Some(TrackEmitter {
        tag: payload[0x30..0x34].try_into().unwrap(),
        frame: EmitterFrame::from_bytes(&payload[8..0x30])?,
        article_index,
    })
}

/// The particle manager (`0x627df0`): systems by key.
#[derive(Debug, Clone)]
pub struct ParticleManager {
    pub systems: BTreeMap<usize, ParticleSystem>,
    pub random: ParticleRandom,
    pub detail: u32,
    /// Far view distance (`0x6197cc`).
    pub far: f32,
}

impl Default for ParticleManager {
    fn default() -> Self {
        Self { systems: BTreeMap::new(), random: ParticleRandom::default(), detail: DETAIL_FULL, far: 1500.0 }
    }
}

/// Request counter of a system requested since the last tick.
pub const REQUESTED: i32 = 100;

impl ParticleManager {
    /// `Particles_Request` 0x455150 with the static creator 0x455000.
    pub fn request(&mut self, key: usize, def: &ParticleDef, frame: EmitterFrame, distance_sq: f32) {
        if self.detail == 0 {
            return;
        }
        let (detail, far) = (self.detail, self.far);
        let random = &mut self.random;
        let s = self
            .systems
            .entry(key)
            .or_insert_with(|| ParticleSystem::primed(*def, frame, detail, distance_sq, far, random));
        s.request = REQUESTED;
        s.distance_sq = distance_sq;
    }

    /// `Particles_Tick` 0x4551e0.
    pub fn tick(&mut self) {
        if self.detail == 0 {
            return;
        }
        let far = self.far;
        let random = &mut self.random;
        self.systems.retain(|_, s| {
            s.request -= 1;
            if s.request < 1 {
                return false;
            }
            s.tick(far, random);
            true
        });
    }

    /// `Particles_Draw` 0x455260: systems requested since the last tick.
    pub fn drawn(&self) -> impl Iterator<Item = &ParticleSystem> {
        self.systems.values().filter(|s| s.request == REQUESTED)
    }
}

/// The 64 Hz particle clock of the race frame 0x4689c0 on the 128 Hz game
/// timer: one tick per 2 timer ticks; after 13 or more timer ticks only one.
#[derive(Debug, Clone, Default)]
pub struct ParticleClock {
    timer: f64,
    last: u32,
}

impl ParticleClock {
    /// Advance by `dt` seconds; the number of particle ticks to run.
    pub fn advance(&mut self, dt: f32) -> u32 {
        self.timer += dt.max(0.0) as f64 * 128.0;
        let now = self.timer as u32;
        let mut elapsed = now.wrapping_sub(self.last);
        if elapsed >= 13 {
            self.last = now.wrapping_sub(2);
            elapsed = 2;
        } else if elapsed < 2 {
            return 0;
        }
        let n = elapsed >> 1;
        self.last = self.last.wrapping_add(2 * n);
        n
    }
}

/// Inputs of `R3DCarParticle_Update` 0x43ef80 for one wheel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelParticleInput {
    /// `w+0x40` (`car+0x7f8`): contact point.
    pub contact: [f32; 3],
    /// `w+0x8c & 0xf` (`car+0x844`).
    pub surface: u32,
    /// `w+0xb8` (`car+0x870`): skid 0..1.
    pub skid: f32,
}

/// Car inputs of 0x43ef80.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarParticleInput {
    pub wheels: [WheelParticleInput; 4],
    /// `+0x33c`: velocity (m/s).
    pub velocity: [f32; 3],
    /// `+0x37c`: forward axis F.
    pub forward: [f32; 3],
    /// `+0xd60`: forward speed in the ground frame.
    pub speed: f32,
    /// `FUN_0045d000() > 0.1`.
    pub raining: bool,
    /// `0x6197aa`: rain spray option.
    pub rain_spray: bool,
    /// Camera distance² of the car.
    pub distance_sq: f32,
}

/// Surfaces where smoke and rain spray appear (`0x5cd964[s] == 1`).
pub const SURFACE_CLASS: [u32; 16] = [0, 1, 2, 2, 1, 2, 0, 1, 0, 2, 1, 1, 1, 2, 0, 2];
/// Road dirt tag per surface (`0x5b3750`).
pub const DIRT_TAGS: [Option<&[u8; 4]>; 16] = [
    None, None, Some(b"RDDT"), Some(b"RDGR"), None, Some(b"RDDT"), Some(b"WTSP"), None,
    None, Some(b"RDSN"), None, None, None, Some(b"RDDT"), None, Some(b"RDSN"),
];
/// Rain spray speed threshold `0x5cd9b0`.
pub const SPRAY_SPEED: f32 = 7.5;

/// `R3DCarParticle` (0x43ee10): 11 systems `SMRT×4 CSPK WTSP×2 RDDT×4`.
#[derive(Debug, Clone)]
pub struct CarParticles {
    pub systems: Vec<ParticleSystem>,
    defs: Vec<ParticleDef>,
    pub detail: u32,
}

impl CarParticles {
    const TAGS: [&'static [u8; 4]; 11] =
        [b"SMRT", b"SMRT", b"SMRT", b"SMRT", b"CSPK", b"WTSP", b"WTSP", b"RDDT", b"RDDT", b"RDDT", b"RDDT"];

    /// `None` when a record is missing.
    pub fn new(defs: &[ParticleDef], detail: u32) -> Option<Self> {
        let find = |t: &[u8; 4]| defs.iter().find(|d| &d.tag == t).copied();
        let mut systems = Vec::new();
        for t in Self::TAGS {
            let mut s = ParticleSystem::new(find(t)?, EmitterFrame::default(), detail, 0.0);
            s.active = false;
            systems.push(s);
        }
        Some(Self { systems, defs: defs.to_vec(), detail })
    }

    fn def(&self, tag: &[u8; 4]) -> Option<ParticleDef> {
        self.defs.iter().find(|d| &d.tag == tag).copied()
    }

    /// `R3DCarParticle_Update` 0x43ef80 for a drawn car.
    pub fn update(&mut self, car: &CarParticleInput) {
        if self.detail == 0 {
            return;
        }
        let v = car.velocity;
        let f = car.forward;
        for (i, w) in car.wheels.iter().enumerate() {
            let paved = SURFACE_CLASS[(w.surface & 15) as usize] == 1;
            // Skid smoke.
            let s = &mut self.systems[i];
            if car.raining || !paved || w.skid < 0.5 {
                s.active = false;
            } else {
                s.frame.position = w.contact;
                s.frame.axis_b = [v[0], v[1] + 0.8, v[2]];
                s.set_frame(s.frame);
                s.scale = w.skid;
                s.active = true;
                s.request = REQUESTED;
                s.distance_sq = car.distance_sq;
            }
            // Road dirt by surface.
            let tag = DIRT_TAGS[(w.surface & 15) as usize];
            let def = tag.and_then(|t| self.def(t));
            let detail = self.detail;
            let s = &mut self.systems[7 + i];
            match def {
                Some(def) if w.skid >= 0.3 && car.speed > 0.0 => {
                    s.scale = w.skid;
                    s.retag(def, detail);
                    s.active = true;
                    s.request = REQUESTED;
                    s.distance_sq = car.distance_sq;
                    let mut p = w.contact;
                    let lift = if &def.tag == b"RDDT" {
                        0.8
                    } else {
                        p[0] -= f[0];
                        p[2] -= f[2];
                        6.32
                    };
                    let mut frame = s.frame;
                    frame.position = p;
                    frame.axis_b = [v[0] - f[0] * 3.66, v[1] + lift, v[2] - f[2] * 3.66];
                    s.set_frame(frame);
                }
                _ => s.active = false,
            }
            // Rain spray, rear wheels.
            if i >= 2 {
                let s = &mut self.systems[3 + i];
                if !car.raining || !paved || car.speed <= SPRAY_SPEED || !car.rain_spray {
                    s.active = false;
                } else {
                    let over = (car.speed - SPRAY_SPEED).min(SPRAY_SPEED * 2.0);
                    let mut frame = s.frame;
                    frame.position = [w.contact[0] - f[0] * 1.14, w.contact[1], w.contact[2] - f[2] * 1.14];
                    frame.axis_b = [v[0] - 2.0 * f[0], v[1] + 4.35 + car.speed * 0.05, v[2] - 2.0 * f[2]];
                    s.set_frame(frame);
                    s.active = true;
                    s.scale = over / (SPRAY_SPEED * 2.0);
                    s.request = REQUESTED;
                    s.distance_sq = car.distance_sq;
                }
            }
        }
    }

    /// `R3DCarParticle_TickAll` 0x43f660 (32 Hz car tick); the spark slot 4 is
    /// left to the spark emitter.
    pub fn tick(&mut self, far: f32, random: &mut ParticleRandom) {
        for (i, s) in self.systems.iter_mut().enumerate() {
            if i != 4 {
                s.tick(far, random);
            }
        }
    }

    /// Systems with sprites to draw.
    pub fn drawn(&self) -> impl Iterator<Item = &ParticleSystem> {
        self.systems.iter().enumerate().filter(|(i, s)| *i != 4 && !s.particles.is_empty()).map(|(_, s)| s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WF06 CanyonWaterfalBottom` of particle.dat.
    pub(crate) fn wf06() -> ParticleDef {
        ParticleDef {
            tag: *b"WF06",
            flags: 0,
            count: 40,
            emit: [5.9, 0.25, 10.0],
            rand_emit: [0.0, 0.0125, 0.0],
            air_friction: 1.0,
            material: *b"ktss",
            blend_mode: 0,
            z_bias: 100,
            num_frames: 1,
            frame_speed: 10,
            num_states: 2,
            start_size: 1.0,
            end_size: 1.0,
            lifetime: 220,
            period_on: 100,
            period_off: 0,
            prime_ticks: 0,
            weight: 0.23,
            rand_weight: 0.0,
            rand_rotation: 389.0,
            fade_fraction: 1.0,
            colors: [0xffcebbd9, 0x00ffffff, 0, 0],
        }
    }

    fn up_frame() -> EmitterFrame {
        EmitterFrame { position: [10.0, 20.0, 30.0], ground: 1.0, axis_a: [1.0, 0.0, 0.0], axis_b: [0.0, 1.0, 0.0] }
    }

    #[test]
    fn local_particle_dat_has_the_waterfall_systems() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Render");
        let (Ok(p), Ok(c)) = (std::fs::read(dir.join("particle.dat")), std::fs::read(dir.join("carpart.dat"))) else {
            return;
        };
        let p = parse_particle_dat(&p).unwrap();
        assert_eq!(p.len(), 62);
        let wf = p.iter().find(|d| &d.tag == b"WF06").unwrap();
        assert_eq!(*wf, wf06());
        assert_eq!(wf.texture(), "sstk");
        let defs = merge_particle_defs(&p, &parse_particle_dat(&c).unwrap());
        assert!(defs.iter().any(|d| &d.tag == b"CSPK"));
        assert_eq!(defs.iter().filter(|d| &d.tag == b"SMOK").count(), 1);
    }

    #[test]
    fn random_follows_the_original_lcg() {
        let mut r = ParticleRandom::seeded(0);
        let p = 0x3ade_68b1u32.wrapping_mul(123_456_789);
        assert_eq!(r.next_raw(), ((p as i32 >> 8) & 0xffff) as u32);
        assert_eq!(r.state, p & 0xffff);
        let r1 = ParticleRandom::seeded(1);
        assert_eq!(r1.state, 0x3ade_68b1u32.wrapping_mul(123_456_789) & 0xffff);
        assert_eq!(ParticleRandom::seeded(501), r1);
        let mut r = ParticleRandom::seeded(7);
        for _ in 0..1000 {
            let x = r.next();
            assert!((0.0..2.0).contains(&x));
        }
    }

    #[test]
    fn rate_spreads_the_count_over_the_emitting_ticks() {
        let mut d = wf06();
        assert!((d.rate() - 40.0 / 220.0).abs() < 1e-6);
        d.period_on = 60;
        d.period_off = 40;
        d.lifetime = 174;
        assert!((d.rate() - 40.0 / ((174.0 / 100.0) * 60.0)).abs() < 1e-5);
        d.lifetime = 80;
        assert!((d.rate() - 40.0 / 60.0).abs() < 1e-6, "on + off >= life: count / on");
        d.lifetime = 50;
        assert!((d.rate() - 40.0 / 50.0).abs() < 1e-6, "life < on: count / life");
    }

    #[test]
    fn budget_follows_the_detail() {
        let s = ParticleSystem::new(wf06(), up_frame(), 200, 0.0);
        assert_eq!(s.budget, 31);
        let mut s = ParticleSystem::new(wf06(), up_frame(), DETAIL_FULL, 0.0);
        let mut r = ParticleRandom::default();
        let mut max = 0;
        for _ in 0..1000 {
            s.tick(1500.0, &mut r);
            max = max.max(s.particles.len());
        }
        assert!(max <= 40 && max >= 38, "{max}");
    }

    #[test]
    fn mist_starts_jittered_and_rises() {
        let mut s = ParticleSystem::new(wf06(), up_frame(), DETAIL_FULL, 0.0);
        let mut r = ParticleRandom::default();
        for _ in 0..50 {
            s.tick(1500.0, &mut r);
        }
        assert!(!s.particles.is_empty());
        for p in &s.particles {
            assert!(p.velocity[1] > 0.0 || p.age > 100.0);
            assert!(p.rotation.abs() <= 389.0);
        }
        // A fresh particle: within the jitter box (one tick of motion).
        let mut fresh = ParticleSystem::new(wf06(), up_frame(), DETAIL_FULL, 0.0);
        let p = fresh.spawn(&mut r);
        assert!((p.position[0] - 10.0).abs() <= 5.9 && (p.position[2] - 30.0).abs() <= 10.0);
        assert_eq!(p.position[1], 20.0);
        assert!((p.velocity[1] - 0.25).abs() <= 0.0125 + 1e-6);
        assert!((p.gravity - 0.00115).abs() < 1e-7);
    }

    #[test]
    fn particles_fall_and_die_after_their_lifetime() {
        let mut s = ParticleSystem::new(wf06(), up_frame(), DETAIL_FULL, 0.0);
        let mut r = ParticleRandom::default();
        for _ in 0..10 {
            s.tick(1500.0, &mut r);
        }
        s.active = false;
        let first = s.particles[0];
        for _ in 0..221 {
            s.tick(1500.0, &mut r);
        }
        assert!(s.particles.is_empty());
        // Motion of one particle: y(t) = y0 + Σ(vy − k·g).
        let mut p = first;
        let mut y = p.position[1];
        for _ in 0..10 {
            y += p.velocity[1];
            p.velocity[1] -= p.gravity;
        }
        assert!(y > first.position[1]);
    }

    #[test]
    fn duty_cycle_and_one_shot() {
        let mut d = wf06();
        d.period_on = 5;
        d.period_off = 50;
        d.flags = 2;
        d.lifetime = 127;
        d.count = 14;
        let mut s = ParticleSystem::new(d, up_frame(), DETAIL_FULL, 0.0);
        assert!(!s.active, "one-shot systems start inactive");
        s.start();
        let mut r = ParticleRandom::default();
        for _ in 0..60 {
            s.tick(1500.0, &mut r);
        }
        assert!(!s.active);
        let n = s.particles.len();
        assert!(n > 0);
        for _ in 0..60 {
            s.tick(1500.0, &mut r);
        }
        assert!(s.particles.len() <= n);
    }

    #[test]
    fn far_systems_fade_out() {
        let mut d = wf06();
        d.fade_fraction = 0.5;
        let s = ParticleSystem::new(d, up_frame(), DETAIL_FULL, 400.0 * 400.0);
        assert_eq!(s.distance_fade(1000.0), 1.0);
        let s = ParticleSystem::new(d, up_frame(), DETAIL_FULL, 750.0 * 750.0);
        // The original square root is the bit trick: √562500 ≈ 786.66, not 750.
        assert!((s.distance_fade(1000.0) - (1000.0 - 786.6582) / 500.0).abs() < 1e-4);
    }

    #[test]
    fn colour_states_and_sizes() {
        let s = ParticleSystem::new(wf06(), up_frame(), DETAIL_FULL, 0.0);
        assert_eq!(s.color_at(0.0), 0xffcebbd9);
        assert_eq!(s.color_at(1.0) >> 24, 0);
        let mid = s.color_at(0.5);
        assert!((mid >> 24) > 120 && (mid >> 24) < 135);
        let mut d = wf06();
        d.num_states = 3;
        d.colors = [0x00000000, 0xff000000, 0x00ff0000, 0];
        let s3 = ParticleSystem::new(d, up_frame(), DETAIL_FULL, 0.0);
        assert_eq!(s3.color_at(0.5) >> 24, 0xff);
        assert_eq!(s3.color_at(0.25) >> 24, 0x7f);
        // t is clamped to 0.9999 and the delta truncated: alpha 1, not 0.
        assert_eq!(s3.color_at(1.0) >> 24, 1);
        let mut d = wf06();
        d.start_size = 0.875;
        let mut s = ParticleSystem::new(d, up_frame(), DETAIL_FULL, 0.0);
        s.particles.push(Particle { position: [0.0; 3], velocity: [0.0; 3], age: 110.0, gravity: 0.0, rotation: 100.0 });
        let sp = s.sprites().next().unwrap();
        assert!((sp.size - 9.375).abs() < 1e-4);
        assert!((sp.rotation - 50.0).abs() < 1e-4);
    }

    #[test]
    fn manager_drops_unrequested_systems() {
        let mut m = ParticleManager::default();
        let mut d = wf06();
        d.prime_ticks = 10;
        m.request(7, &d, up_frame(), 0.0);
        assert_eq!(m.systems[&7].particles.len(), 1, "10 prime ticks at 40/220 per tick");
        assert_eq!(m.drawn().count(), 1);
        m.tick();
        assert_eq!(m.drawn().count(), 0, "drawn only when requested since the last tick");
        for _ in 0..98 {
            m.tick();
        }
        assert!(m.systems.contains_key(&7));
        m.tick();
        assert!(m.systems.is_empty(), "dropped after 100 ticks without a request");
    }

    #[test]
    fn clock_ticks_at_64_hz_with_a_catch_up_cap() {
        let mut c = ParticleClock::default();
        let mut n = 0;
        for _ in 0..60 {
            n += c.advance(1.0 / 60.0);
        }
        assert!((63..=64).contains(&n), "{n}");
        assert_eq!(c.advance(1.0), 1, "a long gap runs one tick");
    }

    #[test]
    fn ef_type_3_payload() {
        let mut p = vec![0u8; 0x38];
        p[0] = 3;
        p[8..12].copy_from_slice(&(-3243.0f32).to_le_bytes());
        p[0x30..0x34].copy_from_slice(b"WF06");
        let e = parse_ef_emitter(&p, 492).unwrap();
        assert_eq!(&e.tag, b"WF06");
        assert_eq!(e.frame.position[0], -3243.0);
        p[0] = 2;
        assert!(parse_ef_emitter(&p, 0).is_none());
    }

    fn car_defs() -> Vec<ParticleDef> {
        let mut out = Vec::new();
        for t in [b"SMRT", b"CSPK", b"WTSP", b"RDDT", b"RDGR", b"RDSN"] {
            let mut d = wf06();
            d.tag = *t;
            d.count = if t == b"RDGR" { 43 } else { 35 };
            d.lifetime = 21;
            out.push(d);
        }
        out
    }

    #[test]
    fn wheel_effects_follow_skid_and_surface() {
        let mut c = CarParticles::new(&car_defs(), DETAIL_FULL).unwrap();
        let mut input = CarParticleInput { velocity: [0.0, 0.0, 20.0], forward: [0.0, 0.0, 1.0], speed: 20.0, ..Default::default() };
        input.wheels[2] = WheelParticleInput { contact: [1.0, 0.0, 2.0], surface: 1, skid: 0.6 };
        input.wheels[3] = WheelParticleInput { contact: [-1.0, 0.0, 2.0], surface: 3, skid: 0.4 };
        c.update(&input);
        assert!(c.systems[2].active, "smoke on paved surface 1 with skid ≥ 0.5");
        assert!(!c.systems[3].active, "no smoke on grass");
        assert_eq!(c.systems[2].frame.axis_b, [0.0, 0.8, 20.0]);
        assert!(!c.systems[9].active, "no dirt on paved");
        assert!(c.systems[10].active);
        assert_eq!(&c.systems[10].def.tag, b"RDGR");
        assert_eq!(c.systems[10].frame.position, [-1.0, 0.0, 1.0], "moved back by F");
        assert!((c.systems[10].frame.axis_b[1] - 6.32).abs() < 1e-6);
        assert!(!c.systems[5].active && !c.systems[6].active, "no rain");
        input.raining = true;
        input.rain_spray = true;
        c.update(&input);
        assert!(!c.systems[2].active, "no smoke in rain");
        assert!(c.systems[5].active, "spray behind the left rear wheel");
        assert!((c.systems[5].scale - 12.5 / 15.0).abs() < 1e-6);
    }
}

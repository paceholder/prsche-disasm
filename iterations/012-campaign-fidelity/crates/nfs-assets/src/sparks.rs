//! Collision sparks (`docs/re/contact-effects.md`).
//!
//! `Car_ContactEffects` 0x40f740 bursts the car's spark emitter
//! (`R3DCarParticle` slot 4, system `CSPK` of `Render/carpart.dat`) through
//! `R3DCarParticle_EmitSparks` 0x43f4c0 when the contact strength `+0x430` is
//! above 1.5 and the contact is a barrier edge (`0x4xxxx`, 0x494000) or a scene
//! polygon (`0x3xxxx`, 0x495f30); car–car contacts (`0x5xxxx`) only make a
//! sound. The generic particle system emits and moves the sparks per tick
//! (0x454c30, particle init 0x454860).

/// Contact strength above which a contact bursts sparks (0x40f740, `1.5 < +0x430`).
pub const SPARK_MIN_IMPACT: f32 = 1.5;
/// Particle tick rate. Approximation A70: assumed to be the 32 Hz effects tick.
pub const PARTICLE_HZ: f32 = 32.0;

/// One particle system record of `carpart.dat` / `particle.dat` (0x80 bytes;
/// the generic system reads it from `+4`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleDef {
    pub tag: [u8; 4],
    /// `+4`: bit 0 bounce on the ground, bit 1 one-shot (stop after a period).
    pub flags: u32,
    pub count: u32,
    pub emit: [f32; 3],
    pub rand_emit: [f32; 3],
    pub air_friction: f32,
    /// Texture FourCC, stored reversed (`KRPS` = `SPRK`).
    pub material: [u8; 4],
    pub blend_mode: u32,
    pub lifetime: u32,
    pub period_on: u32,
    pub period_off: u32,
    pub weight: f32,
    pub rand_weight: f32,
    /// Colour states (ARGB) over the lifetime.
    pub colors: [u32; 4],
}

impl ParticleDef {
    /// Texture name in `particle.fsh`.
    pub fn texture(&self) -> String {
        let mut m = self.material;
        m.reverse();
        String::from_utf8_lossy(&m).into_owned()
    }

    /// Particles emitted per tick while emitting (`FUN_00454250`): the count
    /// spread over the emitting ticks of one lifetime.
    pub fn rate(&self) -> f32 {
        let (count, life, on, off) = (
            self.count as f32,
            self.lifetime as f32,
            self.period_on as f32,
            self.period_off,
        );
        if off == 0 || self.lifetime < self.period_on {
            count / life
        } else if self.period_on + off < self.lifetime {
            count / ((life / (self.period_on + off) as f32) * on)
        } else {
            count / on
        }
    }
}

fn f32le(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// Parse `carpart.dat`: a u32 count and 0x80-byte records.
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
                lifetime: u32le(r, 0x48),
                period_on: u32le(r, 0x4c),
                period_off: u32le(r, 0x50),
                weight: f32le(r, 0x58),
                rand_weight: f32le(r, 0x5c),
                colors: [u32le(r, 0x70), u32le(r, 0x74), u32le(r, 0x78), u32le(r, 0x7c)],
            }
        })
        .collect())
}

/// The particle random generator (`0x5ceae0 = (0x5ceae0·0x5ceae4) & 0xffff`,
/// sample `(x >> 8 & 0xffff) / 32768`). Approximation A70: seed and multiplier
/// of the original globals are not read; a 16-bit LCG of the same shape.
#[derive(Debug, Clone)]
pub struct ParticleRandom(u32);

impl Default for ParticleRandom {
    fn default() -> Self {
        Self(0x1234)
    }
}

impl ParticleRandom {
    /// A value in [0, 2) like the original's `r · 3.0517578e-05`.
    fn next(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(25173).wrapping_add(13849)) & 0xffff_ffff;
        ((self.0 >> 8) & 0xffff) as f32 * 3.051_757_8e-5
    }
    /// `base − spread + r·spread`, as the original's jitter terms.
    fn around(&mut self, base: f32, spread: f32) -> f32 {
        base - spread + self.next() * spread
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spark {
    /// Position and velocity per tick, original world space.
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub gravity: f32,
    pub age: u32,
}

/// One car's spark emitter: `R3DCarParticle` slot 4 with its generic system.
#[derive(Debug, Clone)]
pub struct SparkEmitter {
    pub def: ParticleDef,
    pub sparks: Vec<Spark>,
    active: bool,
    counter: u32,
    emitted: f32,
    accumulator: f32,
    origin: [f32; 3],
    axes: [[f32; 3]; 3],
    random: ParticleRandom,
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 { v.map(|x| x / l) } else { v }
}

impl SparkEmitter {
    pub fn new(def: ParticleDef) -> Self {
        Self {
            def,
            sparks: Vec::new(),
            active: false,
            counter: 0,
            emitted: 0.0,
            accumulator: 0.0,
            origin: [0.0; 3],
            // Created with (1,0,0), (0,1,0); the third axis is their cross (0x454250).
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            random: ParticleRandom::default(),
        }
    }

    /// `R3DCarParticle_EmitSparks` 0x43f4c0: burst at the contact point,
    /// streaming against the car velocity `velocity` (m/s).
    pub fn burst(&mut self, contact: [f32; 3], velocity: [f32; 3]) {
        self.origin = contact;
        // Main axis −v/10; approximation A70: the side axis is taken as
        // perpendicular to v and to (up × v) (0x5328a0 not read).
        let main = velocity.map(|x| x * -0.1);
        let mut v = velocity;
        if v[0].abs() < 0.01 && v[2].abs() < 0.01 {
            v[0] = 0.015;
        }
        let side = unit(cross(cross([0.0, 1.0, 0.0], v), v));
        self.axes[0] = side;
        self.axes[1] = main;
        self.active = true;
        self.counter = 0;
    }

    /// One particle tick (0x454c30): emission while active, then motion.
    pub fn tick(&mut self) {
        let d = self.def;
        if self.active {
            let emitting = !(d.period_off != 0 && d.period_on < self.counter);
            if emitting {
                self.accumulator += d.rate();
                while self.emitted + 1.0 < self.accumulator {
                    if self.sparks.len() < d.count as usize {
                        let spark = self.spawn();
                        self.sparks.push(spark);
                    }
                    self.emitted += 1.0;
                }
                self.counter += 1;
            } else if d.period_on + d.period_off < self.counter {
                self.counter -= d.period_on + d.period_off;
                if d.flags & 2 != 0 {
                    self.active = false;
                }
            } else {
                self.counter += 1;
            }
        }
        let friction = d.air_friction;
        self.sparks.retain_mut(|s| {
            for k in 0..3 {
                s.position[k] += s.velocity[k];
            }
            s.velocity[1] -= s.gravity;
            if friction < 1.0 {
                s.velocity[0] *= friction;
            }
            s.velocity[2] *= friction;
            s.age += 1;
            s.age <= d.lifetime
        });
    }

    /// Particle init 0x454860.
    fn spawn(&mut self) -> Spark {
        let d = self.def;
        let r = &mut self.random;
        let mut position = self.origin;
        position[0] += r.around(0.0, d.emit[0]);
        position[2] += r.around(0.0, d.emit[2]);
        let main = r.around(d.emit[1], d.rand_emit[1]);
        let a = r.around(0.0, d.rand_emit[0]);
        let c = r.around(0.0, d.rand_emit[2]);
        let [ax, bx, cx] = self.axes;
        let velocity: [f32; 3] =
            std::array::from_fn(|k| bx[k] * main + ax[k] * a + cx[k] * c);
        let gravity = r.around(d.weight, d.rand_weight) * 0.005;
        Spark {
            position,
            velocity,
            gravity,
            age: 0,
        }
    }

    /// Colour of a spark (ARGB): the colour states interpolated over its life.
    /// Approximation A70: linear interpolation between the states.
    pub fn color(&self, spark: &Spark) -> [f32; 4] {
        let states = 3usize;
        let t = (spark.age as f32 / self.def.lifetime.max(1) as f32).clamp(0.0, 1.0)
            * (states - 1) as f32;
        let i = (t.floor() as usize).min(states - 2);
        let f = t - i as f32;
        let c = |argb: u32| {
            [
                ((argb >> 16) & 255) as f32 / 255.0,
                ((argb >> 8) & 255) as f32 / 255.0,
                (argb & 255) as f32 / 255.0,
                (argb >> 24) as f32 / 255.0,
            ]
        };
        let (a, b) = (c(self.def.colors[i]), c(self.def.colors[i + 1]));
        std::array::from_fn(|k| a[k] + (b[k] - a[k]) * f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spark_def() -> ParticleDef {
        ParticleDef {
            tag: *b"CSPK",
            flags: 6,
            count: 40,
            emit: [0.2, 0.1925, 0.2],
            rand_emit: [0.0625; 3],
            air_friction: 1.0,
            material: *b"KRPS",
            blend_mode: 1,
            lifetime: 28,
            period_on: 1,
            period_off: 2,
            weight: 2.75,
            rand_weight: 0.0,
            colors: [0x83fff140, 0xc3ff8a20, 0x00d60101, 0x03000000],
        }
    }

    #[test]
    fn local_carpart_dat_has_the_spark_system() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Render/carpart.dat");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let defs = parse_particle_dat(&data).unwrap();
        assert_eq!(defs.len(), 9);
        let sparks = defs.iter().find(|d| &d.tag == b"CSPK").unwrap();
        assert_eq!(*sparks, spark_def());
        assert_eq!(sparks.texture(), "SPRK");
    }

    #[test]
    fn a_burst_emits_eight_sparks_over_two_ticks() {
        let def = spark_def();
        assert!((def.rate() - 40.0 / (28.0 / 3.0)).abs() < 1e-5);
        let mut e = SparkEmitter::new(def);
        e.burst([0.0, 1.0, 0.0], [0.0, 0.0, 20.0]);
        e.tick();
        assert_eq!(e.sparks.len(), 4);
        e.tick();
        assert_eq!(e.sparks.len(), 8);
        for _ in 0..4 {
            e.tick();
        }
        assert_eq!(e.sparks.len(), 8, "one-shot: no more after the period");
        assert!(!e.active);
    }

    #[test]
    fn sparks_fly_against_the_car_fall_and_die() {
        let mut e = SparkEmitter::new(spark_def());
        e.burst([0.0, 1.0, 0.0], [0.0, 0.0, 20.0]);
        e.tick();
        let s = e.sparks[0];
        assert!(s.velocity[2] < -0.2, "streams backwards: {:?}", s.velocity);
        assert!((s.gravity - 0.01375).abs() < 1e-6);
        for _ in 0..40 {
            e.tick();
        }
        assert!(e.sparks.is_empty(), "lifetime 28 ticks");
    }

    #[test]
    fn colour_fades_from_yellow_to_transparent_red() {
        let e = SparkEmitter::new(spark_def());
        let mut s = Spark { position: [0.0; 3], velocity: [0.0; 3], gravity: 0.0, age: 0 };
        let c0 = e.color(&s);
        assert!(c0[0] == 1.0 && c0[1] > 0.9 && c0[2] < 0.3);
        s.age = 28;
        let c1 = e.color(&s);
        assert_eq!(c1[3], 0.0);
        assert!(c1[0] > 0.8 && c1[1] < 0.05);
    }
}

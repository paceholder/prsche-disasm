//! Collision sparks (`docs/re/contact-effects.md`).
//!
//! `Car_ContactEffects` 0x40f740 bursts the car's spark emitter
//! (`R3DCarParticle` slot 4, system `CSPK` of `Render/carpart.dat`) through
//! `R3DCarParticle_EmitSparks` 0x43f4c0 when the contact strength `+0x430` is
//! above 1.5 and the contact is a barrier edge (`0x4xxxx`, 0x494000) or a scene
//! polygon (`0x3xxxx`, 0x495f30); car–car contacts (`0x5xxxx`) only make a
//! sound. The generic particle system (`crate::particles`, 0x454c30, 0x454860)
//! emits and moves the sparks.

pub use crate::particles::{parse_particle_dat, ParticleDef};
use crate::particles::{EmitterFrame, Particle, ParticleRandom, ParticleSystem, DETAIL_FULL};

/// Contact strength above which a contact bursts sparks (0x40f740, `1.5 < +0x430`).
pub const SPARK_MIN_IMPACT: f32 = 1.5;
/// Car particle tick: `R3DCarParticle_TickAll` 0x43f660 runs on the odd
/// scheduler steps (call 0x4150f2), 32 Hz.
pub const PARTICLE_HZ: f32 = 32.0;

/// One car's spark emitter: `R3DCarParticle` slot 4 with its generic system.
#[derive(Debug, Clone)]
pub struct SparkEmitter {
    pub system: ParticleSystem,
    random: ParticleRandom,
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 { v.map(|x| x / l) } else { v }
}

impl SparkEmitter {
    pub fn new(def: ParticleDef) -> Self {
        // Created with (1,0,0), (0,1,0) and inactive (0x43ee10).
        let mut system = ParticleSystem::new(def, EmitterFrame::default(), DETAIL_FULL, 0.0);
        system.active = false;
        Self { system, random: ParticleRandom::default() }
    }

    /// Live sparks, oldest first.
    pub fn sparks(&self) -> &[Particle] {
        &self.system.particles
    }

    pub fn active(&self) -> bool {
        self.system.active
    }

    /// `R3DCarParticle_EmitSparks` 0x43f4c0: burst at the contact point,
    /// streaming against the car velocity `velocity` (m/s).
    pub fn burst(&mut self, contact: [f32; 3], velocity: [f32; 3]) {
        // Main axis −v/10; approximation A70: the side axis is taken as
        // perpendicular to v and to (up × v) (0x5328a0 not read).
        let main = velocity.map(|x| x * -0.1);
        let mut v = velocity;
        if v[0].abs() < 0.01 && v[2].abs() < 0.01 {
            v[0] = 0.015;
        }
        let side = unit(crate::particles::cross(crate::particles::cross([0.0, 1.0, 0.0], v), v));
        let frame = EmitterFrame { position: contact, ground: self.system.frame.ground, axis_a: side, axis_b: main };
        self.system.set_frame(frame);
        self.system.start();
    }

    /// One particle tick (0x454c30).
    pub fn tick(&mut self) {
        self.system.tick(f32::INFINITY, &mut self.random);
    }

    /// Colour of a spark as RGBA 0..1 (0x454430 colour states).
    pub fn color(&self, spark: &Particle) -> [f32; 4] {
        let t = (spark.age / self.system.def.lifetime.max(1) as f32).clamp(0.0, 1.0);
        let argb = self.system.color_at(t);
        [
            ((argb >> 16) & 255) as f32 / 255.0,
            ((argb >> 8) & 255) as f32 / 255.0,
            (argb & 255) as f32 / 255.0,
            (argb >> 24) as f32 / 255.0,
        ]
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
            z_bias: -40000,
            num_frames: 1,
            frame_speed: 10,
            num_states: 3,
            start_size: 0.0,
            end_size: 0.0,
            lifetime: 28,
            period_on: 1,
            period_off: 2,
            prime_ticks: 0,
            weight: 2.75,
            rand_weight: 0.0,
            rand_rotation: 0.0,
            fade_fraction: 1.0,
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
        assert_eq!(e.sparks().len(), 4);
        e.tick();
        assert_eq!(e.sparks().len(), 8);
        for _ in 0..4 {
            e.tick();
        }
        assert_eq!(e.sparks().len(), 8, "one-shot: no more after the period");
        assert!(!e.active());
    }

    #[test]
    fn sparks_fly_against_the_car_fall_and_die() {
        let mut e = SparkEmitter::new(spark_def());
        e.burst([0.0, 1.0, 0.0], [0.0, 0.0, 20.0]);
        e.tick();
        let s = e.sparks()[0];
        assert!(s.velocity[2] < -0.2, "streams backwards: {:?}", s.velocity);
        assert!((s.gravity - 0.01375).abs() < 1e-6);
        for _ in 0..40 {
            e.tick();
        }
        assert!(e.sparks().is_empty(), "lifetime 28 ticks");
    }

    #[test]
    fn colour_fades_from_yellow_to_transparent_red() {
        let e = SparkEmitter::new(spark_def());
        let mut s = Particle { position: [0.0; 3], velocity: [0.0; 3], gravity: 0.0, age: 0.0, rotation: 0.0 };
        let c0 = e.color(&s);
        assert!(c0[0] == 1.0 && c0[1] > 0.9 && c0[2] < 0.3);
        s.age = 28.0;
        let c1 = e.color(&s);
        assert!(c1[3] <= 1.0 / 255.0 + 1e-6);
        assert!(c1[0] > 0.8 && c1[1] < 0.05);
    }
}

//! Original collision step, in the order of `Car_CollideTick` `0x498ff0`.
//!
//! Collision runs on every second integration step (32 Hz). Per car, in list
//! order: the barrier query (`Car_CollideTrackEdges` 0x495020), then a pair
//! test against every car processed earlier in the same pass, with the
//! bounding-radius broadphase `|dx|,|dy|,|dz| < rA + rB` [VA 0x499077..0x499101]
//! and the pair driver `CarPair_Collide` 0x4980d0. Angular velocity is stored
//! in turns/s and converted to rad/s around each pair call, then both cars
//! are clamped [VA 0x499103..0x499158]. See docs/re/collision.md.
//!
//! Not ported (documented): the track-object pass 0x498de0, the object
//! cooldown `+0x43c` and the pinned-car damage after exhausted retries
//! (`Car_ApplyImpact(100, zone 8)` when speed > 15) — damage is out of scope.

use std::f32::consts::TAU;

use glam::{Quat, Vec3};

use crate::physics::original_adapter::{
    from_original, inertia_from_mass, speed_estimate, to_original,
};
use crate::physics::original_contact::PreparedContactState;
use crate::physics::original_pair::{
    apply_impulse, clamp_angular_velocity, separate_overlap, PairCar, PairGlobals,
};
use crate::race::AiOpponent;

/// Which collision response the engine runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionModel {
    /// Upstream's generic response plus approximations A2/A3. Still used by
    /// arcade mode, which has no 6 DOF body; a debug switch otherwise.
    Prototype,
    /// Recovered original response (this module), the default.
    #[default]
    Original,
}

/// Collision rate: half the 64 Hz integration rate [VA 0x498ff0, 0x411130].
pub const COLLISION_HZ: f32 = 32.0;
/// Upper bound of passes per rendered frame, so a stalled frame can't spiral.
pub const MAX_PASSES_PER_FRAME: u32 = 4;
/// Extra separation/impulse rounds after the first in `0x4980d0`.
pub const PAIR_RETRIES: u32 = 4;

/// Fixed-rate collision clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionClock {
    pending: f32,
}

impl CollisionClock {
    /// Advance by `dt` seconds; returns how many collision passes are due.
    pub fn advance(&mut self, dt: f32) -> u32 {
        let step = 1.0 / COLLISION_HZ;
        self.pending += dt.max(0.0);
        let mut passes = 0;
        while self.pending >= step && passes < MAX_PASSES_PER_FRAME {
            self.pending -= step;
            passes += 1;
        }
        if passes == MAX_PASSES_PER_FRAME {
            self.pending = self.pending.min(step);
        }
        passes
    }
}

/// Barrier state of a pair car, for `RoadSurface::original_barrier_response`.
/// Wheel points and wheel fields are not part of `PairCar`; the response
/// moves the wheel points, which the engine recomputes anyway.
pub fn barrier_state(car: &PairCar) -> PreparedContactState {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let v = car.velocity_33c;
    PreparedContactState {
        position_330: car.position_330,
        velocity_33c: v,
        speed_35c: car.speed_35c,
        basis_364: car.axis_r_364,
        basis_370: car.axis_u_370,
        basis_37c: car.axis_f_37c,
        field_38c: car.angular_388[1],
        field_434: 0.0,
        // Ground frame on level ground [VA 0x49ba30]: right, forward.
        basis_490: car.axis_r_364,
        basis_4a8: car.axis_f_37c,
        local_velocity_d58: [
            dot(v, car.axis_r_364),
            dot(v, car.axis_u_370),
            dot(v, car.axis_f_37c),
        ],
        wheel_points_7f8: [car.position_330; 4],
        wheel_fields_870: [0.0; 4],
        field_db8: 0.0,
        field_dc0: 0.0,
    }
}

/// Copy a barrier response back into the pair car.
pub fn apply_barrier_state(car: &mut PairCar, state: &PreparedContactState) {
    car.position_330 = state.position_330;
    car.velocity_33c = state.velocity_33c;
    car.speed_35c = state.speed_35c;
    car.angular_388[1] = state.field_38c;
}

fn scale_angular(car: &mut PairCar, factor: f32) {
    car.angular_388 = car.angular_388.map(|w| w * factor);
}

/// `CarPair_Collide` 0x4980d0 for `a` (current car) and `b` (earlier car).
/// `barrier(index, car)` runs the barrier pass for one car. Returns whether
/// the cars were in contact.
fn pair_collide(
    cars: &mut [PairCar],
    ia: usize,
    ib: usize,
    globals: &mut PairGlobals,
    barrier: &mut impl FnMut(usize, &mut PairCar),
) -> bool {
    debug_assert!(ib < ia);
    let (head, tail) = cars.split_at_mut(ia);
    let (b, a) = (&mut head[ib], &mut tail[0]);
    let Some(contact) = separate_overlap(a, b) else {
        return false;
    };
    apply_impulse(a, b, contact, globals);
    barrier(ia, a);
    barrier(ib, b);
    let mut retries = PAIR_RETRIES;
    // `while Separate(...) && retries > 0`: separation runs once more after
    // the last retry, before the loop exits.
    while let Some(contact) = separate_overlap(a, b) {
        if retries == 0 {
            break;
        }
        retries -= 1;
        if !apply_impulse(a, b, contact, globals) {
            return true;
        }
        barrier(ia, a);
        barrier(ib, b);
    }
    true
}

/// One collision pass over all cars, in list order. Angular velocity in the
/// cars is in turns/s. Returns the number of pairs that passed the
/// broadphase and were handed to the pair driver.
pub fn collide_pass(
    cars: &mut [PairCar],
    globals: &mut PairGlobals,
    mut barrier: impl FnMut(usize, &mut PairCar),
) -> usize {
    let mut pairs = 0;
    for i in 0..cars.len() {
        barrier(i, &mut cars[i]);
        for j in 0..i {
            let (a, b) = (&cars[i], &cars[j]);
            let r = a.radius_3b4 + b.radius_3b4;
            if (0..3).any(|k| (a.position_330[k] - b.position_330[k]).abs() >= r) {
                continue;
            }
            pairs += 1;
            scale_angular(&mut cars[i], TAU);
            scale_angular(&mut cars[j], TAU);
            pair_collide(cars, i, j, globals, &mut barrier);
            scale_angular(&mut cars[i], 1.0 / TAU);
            scale_angular(&mut cars[j], 1.0 / TAU);
            clamp_angular_velocity(&mut cars[i]);
            clamp_angular_velocity(&mut cars[j]);
        }
    }
    pairs
}

/// Mass used for kinematic AI cars (tonnes); they carry no SIM mass.
/// Approximation: see docs/approximations.md.
pub const AI_MASS: f32 = 1.25;

/// Pair-collision state of a kinematic AI car. The car is drawn with
/// `translate(position) * body_rotation` (yaw plus road tilt, see
/// `AiOpponent::body_rotation`), so the box follows that frame.
/// Approximation: AI cars are kinematic; tilt follows the road surface, not the
/// original vehicle model.
pub fn pair_car_from_ai(ai: &AiOpponent, box_center_body: Vec3, half_extents: [f32; 3]) -> PairCar {
    let rotation = Quat::from_mat3(&ai.body_rotation());
    let center = Vec3::from(ai.position) + rotation * box_center_body;
    let velocity = to_original(Vec3::from(ai.velocity));
    let inertia = inertia_from_mass(AI_MASS);
    PairCar {
        position_330: to_original(center),
        velocity_33c: velocity,
        mass_354: AI_MASS,
        inv_mass_358: 1.0 / AI_MASS,
        speed_35c: speed_estimate(velocity),
        axis_r_364: to_original(rotation * Vec3::X),
        axis_u_370: to_original(rotation * Vec3::Y),
        axis_f_37c: to_original(rotation * Vec3::NEG_Z),
        inertia_394: inertia,
        inv_inertia_398: 1.0 / inertia,
        extents_3a8: half_extents,
        radius_3b4: Vec3::from(half_extents).length(),
        active_7c: true,
        ..PairCar::default()
    }
}

/// Apply a pair-collision result to a kinematic AI car: position change, new
/// velocity, and the yaw part of the spin change (roll/pitch are dropped).
pub fn apply_pair_car_to_ai(ai: &mut AiOpponent, before: &PairCar, after: &PairCar) {
    let delta = from_original(after.position_330) - from_original(before.position_330);
    ai.position = (Vec3::from(ai.position) + delta).to_array();
    if after.velocity_33c != before.velocity_33c {
        let velocity = Vec3::from(ai.velocity) + from_original(after.velocity_33c)
            - from_original(before.velocity_33c);
        ai.apply_collision_velocity(velocity.to_array());
    }
    if after.angular_388 != before.angular_388 {
        let change = crate::physics::original_adapter::angular_from_original(after.angular_388)
            - crate::physics::original_adapter::angular_from_original(before.angular_388);
        ai.apply_collision_spin(change.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(pos: [f32; 3], vel: [f32; 3]) -> PairCar {
        let ext = [0.9, 0.6, 2.0];
        PairCar {
            position_330: pos,
            velocity_33c: vel,
            mass_354: 1.25,
            inv_mass_358: 0.8,
            axis_r_364: [1.0, 0.0, 0.0],
            axis_u_370: [0.0, 1.0, 0.0],
            axis_f_37c: [0.0, 0.0, 1.0],
            inertia_394: 7.5,
            inv_inertia_398: 1.0 / 7.5,
            extents_3a8: ext,
            radius_3b4: (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt(),
            active_7c: true,
            ..PairCar::default()
        }
    }

    #[test]
    fn cadence_is_32_passes_per_64_integration_steps() {
        let mut clock = CollisionClock::default();
        let passes: u32 = (0..64).map(|_| clock.advance(1.0 / 64.0)).sum();
        assert_eq!(passes, 32);
    }

    #[test]
    fn stalled_frame_is_bounded() {
        let mut clock = CollisionClock::default();
        assert_eq!(clock.advance(5.0), MAX_PASSES_PER_FRAME);
        assert!(clock.advance(0.0) <= 1);
    }

    #[test]
    fn each_pair_once() {
        // Three mutually overlapping cars.
        let mut cars = [
            car([0.0, 0.6, 0.0], [0.0, 0.0, 10.0]),
            car([0.5, 0.6, 3.5], [0.0, 0.0, 0.0]),
            car([-0.5, 0.6, 1.8], [0.0, 0.0, 5.0]),
        ];
        let pairs = collide_pass(&mut cars, &mut PairGlobals::default(), |_, _| {});
        assert_eq!(pairs, 3);
    }

    #[test]
    fn far_cars_skip_the_pair_test() {
        let mut cars = [
            car([0.0, 0.6, 0.0], [0.0; 3]),
            car([30.0, 0.6, 0.0], [0.0; 3]),
        ];
        assert_eq!(
            collide_pass(&mut cars, &mut PairGlobals::default(), |_, _| {}),
            0
        );
    }

    #[test]
    fn car_pushed_into_a_wall_is_pushed_back() {
        // Wall at x = 2.0 (cars may not have x + 0.9 > 2.0). Car 1 is pushed
        // toward the wall by car 0 coming from -x.
        let wall = |_: usize, c: &mut PairCar| {
            let limit = 2.0 - c.extents_3a8[0];
            if c.position_330[0] > limit {
                c.position_330[0] = limit;
                c.velocity_33c[0] = c.velocity_33c[0].min(0.0);
            }
        };
        let mut cars = [
            car([1.1 - 1.75, 0.6, 0.0], [8.0, 0.0, 0.0]),
            car([1.1, 0.6, 0.0], [0.0, 0.0, 0.0]),
        ];
        collide_pass(&mut cars, &mut PairGlobals::default(), wall);
        assert!(
            cars[1].position_330[0] <= 2.0 - 0.9 + 1e-5,
            "{:?}",
            cars[1].position_330
        );
        assert!(cars[0].position_330[0] < cars[1].position_330[0]);
    }

    #[test]
    fn angular_velocity_returns_in_turns_and_is_clamped() {
        let mut a = car([-1.75, 0.6, 0.0], [3.0, 0.0, -20.0]);
        a.angular_388 = [0.0, 0.5, 0.0];
        let mut cars = [car([0.0, 0.6, 0.0], [0.0, 0.0, -20.0]), a];
        collide_pass(&mut cars, &mut PairGlobals::default(), |_, _| {});
        for c in &cars {
            assert!(
                c.angular_388.iter().all(|w| w.abs() <= 0.75 + 1e-6),
                "{:?}",
                c.angular_388
            );
        }
    }

    /// Move cars by their velocity for one collision step, then collide.
    fn step(cars: &mut [PairCar]) {
        for c in cars.iter_mut() {
            for k in 0..3 {
                c.position_330[k] += c.velocity_33c[k] / COLLISION_HZ;
            }
        }
        collide_pass(cars, &mut PairGlobals::default(), |_, _| {});
    }

    fn with_extents(mut c: PairCar, ext: [f32; 3]) -> PairCar {
        c.extents_3a8 = ext;
        c.radius_3b4 = (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt();
        c
    }

    #[test]
    fn cars_touch_without_a_gap() {
        // Rear car creeps into a parked car at 1 m/s (356a level-5 boxes).
        let ext = [0.843, 0.604, 1.880];
        let mut cars = [
            with_extents(car([0.0, 0.6, -4.5], [0.0, 0.0, 1.0]), ext),
            with_extents(car([0.0, 0.6, 0.0], [0.0; 3]), ext),
        ];
        for _ in 0..64 {
            step(&mut cars);
            cars[0].velocity_33c = [0.0, 0.0, 1.0];
        }
        let gap = cars[1].position_330[2] - cars[0].position_330[2] - 2.0 * ext[2];
        // Separation stops at the first non-overlapping 1/32 step: no visible gap.
        assert!((-0.01..=1.0 / 32.0 + 0.01).contains(&gap), "gap {gap}");
    }

    #[test]
    fn different_models_use_their_own_boxes() {
        let (short, long) = ([0.84, 0.60, 1.88], [0.99, 0.60, 2.16]);
        let mut cars = [
            with_extents(car([0.0, 0.6, -5.0], [0.0, 0.0, 1.0]), short),
            with_extents(car([0.0, 0.6, 0.0], [0.0; 3]), long),
        ];
        for _ in 0..64 {
            step(&mut cars);
            cars[0].velocity_33c = [0.0, 0.0, 1.0];
        }
        let gap = cars[1].position_330[2] - cars[0].position_330[2] - (short[2] + long[2]);
        assert!((-0.01..=1.0 / 32.0 + 0.01).contains(&gap), "gap {gap}");
    }

    #[test]
    fn pushing_a_slower_car() {
        let mut cars = [
            car([0.0, 0.6, -4.1], [0.0, 0.0, 20.0]),
            car([0.0, 0.6, 0.0], [0.0, 0.0, 15.0]),
        ];
        let start = [cars[0].position_330[2], cars[1].position_330[2]];
        for _ in 0..32 {
            step(&mut cars);
        }
        assert!(
            cars[0].position_330[2] - start[0] >= 15.0,
            "{:?}",
            cars[0].position_330
        );
        assert!(
            cars[1].position_330[2] - start[1] >= 15.0,
            "{:?}",
            cars[1].position_330
        );
        assert!(
            cars[0].position_330[2] < cars[1].position_330[2],
            "cars crossed"
        );
    }

    #[test]
    fn separating_after_a_scrape_does_not_regain_speed() {
        let mut cars = [
            car([-1.75, 0.6, 0.0], [1.0, 0.0, 20.0]),
            car([0.0, 0.6, 0.0], [0.0, 0.0, 20.0]),
        ];
        let mut max_during = 0.0_f32;
        for i in 0..64 {
            if i == 16 {
                cars[0].velocity_33c[0] = -2.0; // steer away
            }
            step(&mut cars);
            let v = cars[0].velocity_33c;
            let speed = (v[0] * v[0] + v[2] * v[2]).sqrt();
            if i < 16 {
                max_during = max_during.max(speed);
            } else {
                assert!(
                    speed <= max_during + 0.1,
                    "regained speed: {speed} > {max_during}"
                );
            }
        }
    }

    #[test]
    fn barrier_state_round_trip_keeps_pair_fields() {
        let mut c = car([1.0, 0.6, 2.0], [3.0, 0.0, -4.0]);
        c.angular_388 = [0.1, 0.2, 0.3];
        c.speed_35c = 5.0;
        let before = c.clone();
        apply_barrier_state(&mut c, &barrier_state(&before));
        assert_eq!(c, before);
    }
}

//! Adapter between the engine's 6 DOF body and the original nfs5.exe car
//! fields used by the recovered collision kernels.
//!
//! The kernels (`original_contact`, `original_scene_contact`, `original_pair`)
//! work on original car state. The engine drives the car with a different
//! (placeholder) vehicle model, so this module converts in both directions.
//! Mapping decisions, see docs/re/collision.md in the workspace root:
//! - Units: the racing scene and the original world are both metres, per
//!   second; the collision scale is 1.
//! - Axes: original world = scene with Z mirrored (`[x, y, -z]`, as in
//!   `surface.rs`). Angular velocity is a pseudovector, so under that mirror it
//!   maps to `[-x, -y, z]`. The original stores it in turns/s [VA 0x499103].
//! - Car axes: `+0x364/+0x370/+0x37c` = mirrored body right/up/forward.
//!   Ground frame `+0x490/+0x4a8` = right / forward: on level ground
//!   `Car_BuildGroundFrame` 0x49ba30 copies them from `+0x364/+0x37c`.
//! - Mass: original `+0x354` is `carSpecs+0x40 / 128`; hypothesis: tonnes.
//!   Only the special-vehicle thresholds depend on the unit.
//!
//! Approximation boundary (docs/approximations.md A5): fields the engine
//! model doesn't have (wheel fields `+0x870`, `+0xdb8`, `+0xdc0`, the
//! `+0x434` contact field) are filled with neutral values and not written
//! back; wheel contact points are recomputed by the engine's suspension.

use std::f32::consts::TAU;

use glam::Vec3;

use super::original_contact::PreparedContactState;
use super::original_pair::PairCar;
use super::vehicle::VehicleSimulation;

/// Scene vector to original world vector.
pub fn to_original(v: Vec3) -> [f32; 3] {
    [v.x, v.y, -v.z]
}

/// Original world vector to scene vector.
pub fn from_original(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], -v[2])
}

/// World-frame scene angular velocity (rad/s) to original turns/s.
pub fn angular_to_original(world_rad_s: Vec3) -> [f32; 3] {
    [
        -world_rad_s.x / TAU,
        -world_rad_s.y / TAU,
        world_rad_s.z / TAU,
    ]
}

/// Original angular velocity (turns/s) to world-frame scene rad/s.
pub fn angular_from_original(turns: [f32; 3]) -> Vec3 {
    Vec3::new(-turns[0] * TAU, -turns[1] * TAU, turns[2] * TAU)
}

/// Original mass unit (hypothesis: tonnes) from the engine's kilograms.
pub fn mass_to_original(kg: f32) -> f32 {
    kg / 1000.0
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Original `+0x35c` speed estimate [VA 0x493e40].
pub fn speed_estimate(velocity: [f32; 3]) -> f32 {
    let (x, z) = (velocity[0].abs(), velocity[2].abs());
    x.max(z) + x.min(z) * 0.25
}

/// World-frame angular velocity of the engine body (it stores body frame).
fn world_angular(sim: &VehicleSimulation) -> Vec3 {
    sim.body.orientation * sim.body.angular_velocity
}

/// Original barrier-response state for the engine body.
pub fn prepared_state(sim: &VehicleSimulation) -> PreparedContactState {
    let body = &sim.body;
    let velocity = to_original(body.linear_velocity);
    let right = to_original(body.right());
    let up = to_original(body.up());
    let forward = to_original(body.forward());
    let wheel_points = std::array::from_fn(|i| {
        sim.suspension
            .wheels
            .get(i)
            .map(|w| to_original(w.contact_point_world))
            .unwrap_or_else(|| to_original(body.position))
    });
    PreparedContactState {
        position_330: to_original(body.position),
        velocity_33c: velocity,
        speed_35c: speed_estimate(velocity),
        basis_364: right,
        basis_370: up,
        basis_37c: forward,
        field_38c: angular_to_original(world_angular(sim))[1],
        field_434: 0.0,
        // Ground frame on level ground [VA 0x49ba30].
        basis_490: right,
        basis_4a8: forward,
        local_velocity_d58: [
            dot(velocity, right),
            dot(velocity, up),
            dot(velocity, forward),
        ],
        wheel_points_7f8: wheel_points,
        wheel_fields_870: [0.0; 4],
        field_db8: 0.0,
        field_dc0: 0.0,
    }
}

/// Write the response back: position change, velocity, and the yaw-rate field
/// if the response changed it. Other fields are not mapped (see module docs).
pub fn apply_prepared_state(
    sim: &mut VehicleSimulation,
    before: &PreparedContactState,
    after: &PreparedContactState,
) {
    let delta = from_original(after.position_330) - from_original(before.position_330);
    sim.body.position += delta;
    if after.velocity_33c != before.velocity_33c {
        sim.body.linear_velocity = from_original(after.velocity_33c);
    }
    if after.field_38c != before.field_38c {
        let mut turns = angular_to_original(world_angular(sim));
        turns[1] = after.field_38c;
        let world = angular_from_original(turns);
        sim.body.angular_velocity = sim.body.orientation.inverse() * world;
    }
}

/// Original inertia `I = 6 m` [VA 0x49b510].
pub fn inertia_from_mass(mass: f32) -> f32 {
    6.0 * mass
}

/// Pair-collision state of the engine body. `box_center_body` and
/// `half_extents` are the car's collision box (`collision_box`) in body space.
/// Angular velocity is in turns/s, as the original stores it between pair calls.
pub fn pair_car_from_sim(
    sim: &VehicleSimulation,
    box_center_body: Vec3,
    half_extents: [f32; 3],
) -> PairCar {
    let body = &sim.body;
    let center = body.to_world_pos(box_center_body);
    let velocity = to_original(body.point_velocity(center));
    let mass = mass_to_original(body.mass);
    let inertia = inertia_from_mass(mass);
    PairCar {
        position_330: to_original(center),
        velocity_33c: velocity,
        mass_354: mass,
        inv_mass_358: 1.0 / mass,
        speed_35c: speed_estimate(velocity),
        axis_r_364: to_original(body.right()),
        axis_u_370: to_original(body.up()),
        axis_f_37c: to_original(body.forward()),
        angular_388: angular_to_original(world_angular(sim)),
        inertia_394: inertia,
        inv_inertia_398: 1.0 / inertia,
        extents_3a8: half_extents,
        // Car_ResetBody 0x49b510 recomputes the radius as |extents|.
        radius_3b4: Vec3::from(half_extents).length(),
        active_7c: true,
        ..PairCar::default()
    }
}

/// Camera view of the engine body (`nfs_assets::camera`): the pair-collision car (box centre,
/// mirrored axes, half extents), ground-frame speeds `+0xd58`/`+0xd60` = centre velocity
/// along R/F. No track nodes: node 0, travel direction +1 (the slope lookup must be flat).
pub fn camera_car_from_sim(
    sim: &VehicleSimulation,
    box_center_body: Vec3,
    half_extents: [f32; 3],
) -> crate::camera::CarView {
    let car = pair_car_from_sim(sim, box_center_body, half_extents);
    crate::camera::CarView {
        node_08: 0,
        pos_330: car.position_330,
        axes_364: [car.axis_r_364, car.axis_u_370, car.axis_f_37c],
        half_extents_3a8: car.extents_3a8,
        direction_d30: 1,
        lateral_speed_d58: dot(car.velocity_33c, car.axis_r_364),
        forward_speed_d60: dot(car.velocity_33c, car.axis_f_37c),
    }
}

/// Write a pair-collision result back as changes of position, velocity and
/// world angular velocity, so engine state the original doesn't model is kept.
pub fn apply_pair_car_to_sim(sim: &mut VehicleSimulation, before: &PairCar, after: &PairCar) {
    let delta = |a: [f32; 3], b: [f32; 3]| from_original(a) - from_original(b);
    sim.body.position += delta(after.position_330, before.position_330);
    sim.body.linear_velocity += delta(after.velocity_33c, before.velocity_33c);
    if after.angular_388 != before.angular_388 {
        let world = world_angular(sim) + angular_from_original(after.angular_388)
            - angular_from_original(before.angular_388);
        sim.body.angular_velocity = sim.body.orientation.inverse() * world;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::vehicle::tests::test_boxster_sim;

    fn car() -> VehicleSimulation {
        let mut sim = VehicleSimulation::from_sim(&test_boxster_sim());
        sim.reset(Vec3::new(10.0, 0.4, -20.0), 0.7);
        sim.body.linear_velocity = Vec3::new(3.0, -0.5, -18.0);
        sim.body.angular_velocity = Vec3::new(0.1, 0.8, -0.05);
        sim
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn vectors_round_trip_through_the_mirror() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(to_original(v), [1.0, 2.0, -3.0]);
        assert_eq!(from_original(to_original(v)), v);
        let w = Vec3::new(0.3, -1.2, 2.0);
        assert!(close(angular_from_original(angular_to_original(w)), w));
    }

    #[test]
    fn angular_velocity_is_a_pseudovector_under_the_mirror() {
        // Rotating with w moves a point p by w x p. Mirroring both the point's
        // motion and the point must agree with the mirrored angular velocity.
        let w = Vec3::new(0.4, 1.1, -0.7);
        let p = Vec3::new(1.5, -0.2, 2.5);
        let moved = from_original(to_original(w.cross(p)));
        let w_orig = Vec3::from(angular_to_original(w)) * TAU;
        let p_orig = Vec3::from(to_original(p));
        let moved_orig = from_original(w_orig.cross(p_orig).to_array());
        assert!(close(moved, moved_orig), "{moved:?} vs {moved_orig:?}");
    }

    #[test]
    fn untouched_state_round_trip_is_identity() {
        let mut sim = car();
        let (pos, vel, ang) = (
            sim.body.position,
            sim.body.linear_velocity,
            sim.body.angular_velocity,
        );
        let state = prepared_state(&sim);
        apply_prepared_state(&mut sim, &state, &state.clone());
        assert_eq!(sim.body.position, pos);
        assert_eq!(sim.body.linear_velocity, vel);
        assert_eq!(sim.body.angular_velocity, ang);
    }

    #[test]
    fn state_uses_mirrored_axes_and_local_velocity() {
        let sim = car();
        let s = prepared_state(&sim);
        assert_eq!(s.position_330, to_original(sim.body.position));
        assert!(close(
            Vec3::from(s.basis_37c),
            Vec3::from(to_original(sim.body.forward()))
        ));
        let forward_speed = sim.body.linear_velocity.dot(sim.body.forward());
        assert!((s.local_velocity_d58[2] - forward_speed).abs() < 1e-5);
        assert_eq!(s.speed_35c, speed_estimate(s.velocity_33c));
    }

    #[test]
    fn yaw_field_change_writes_world_yaw_only() {
        let mut sim = car();
        let before = prepared_state(&sim);
        let mut after = before.clone();
        after.field_38c = 0.25; // turns/s in the original frame
        let world_before = world_angular(&sim);
        apply_prepared_state(&mut sim, &before, &after);
        let world_after = sim.body.orientation * sim.body.angular_velocity;
        assert!((world_after.y - (-0.25 * TAU)).abs() < 1e-5);
        assert!((world_after.x - world_before.x).abs() < 1e-5);
        assert!((world_after.z - world_before.z).abs() < 1e-5);
    }

    #[test]
    fn pair_car_round_trip_is_identity() {
        let mut sim = car();
        let (pos, vel, ang) = (
            sim.body.position,
            sim.body.linear_velocity,
            sim.body.angular_velocity,
        );
        let p = pair_car_from_sim(&sim, Vec3::new(0.0, 0.5, 0.1), [0.9, 0.6, 2.0]);
        assert_eq!(p.inertia_394, 6.0 * p.mass_354);
        apply_pair_car_to_sim(&mut sim, &p, &p.clone());
        assert_eq!(sim.body.position, pos);
        assert_eq!(sim.body.linear_velocity, vel);
        assert_eq!(sim.body.angular_velocity, ang);
    }

    #[test]
    fn pair_result_moves_and_turns_the_body() {
        let mut sim = car();
        let before = pair_car_from_sim(&sim, Vec3::ZERO, [0.9, 0.6, 2.0]);
        let mut after = before.clone();
        after.position_330[2] += 0.25; // original +z = scene -z
        after.velocity_33c[0] += 2.0;
        after.angular_388[1] += 0.1; // turns/s
        let (pos, vel) = (sim.body.position, sim.body.linear_velocity);
        let yaw = (sim.body.orientation * sim.body.angular_velocity).y;
        apply_pair_car_to_sim(&mut sim, &before, &after);
        assert!(close(sim.body.position, pos + Vec3::new(0.0, 0.0, -0.25)));
        assert!(close(
            sim.body.linear_velocity,
            vel + Vec3::new(2.0, 0.0, 0.0)
        ));
        let yaw_after = (sim.body.orientation * sim.body.angular_velocity).y;
        assert!((yaw_after - (yaw - 0.1 * TAU)).abs() < 1e-4);
    }

    #[test]
    fn ground_frame_is_right_and_forward_on_level_ground() {
        let s = prepared_state(&car());
        assert_eq!(s.basis_490, s.basis_364);
        assert_eq!(s.basis_4a8, s.basis_37c);
    }

    #[test]
    fn camera_car_uses_box_centre_axes_and_ground_speeds() {
        let sim = car();
        let center = Vec3::new(0.0, 0.5, 0.1);
        let c = camera_car_from_sim(&sim, center, [0.9, 0.6, 2.0]);
        assert_eq!(c.pos_330, to_original(sim.body.to_world_pos(center)));
        assert_eq!(c.axes_364[2], to_original(sim.body.forward()));
        assert_eq!(c.half_extents_3a8[2], 2.0);
        let v = sim.body.point_velocity(sim.body.to_world_pos(center));
        assert!((c.forward_speed_d60 - v.dot(sim.body.forward())).abs() < 1e-4);
        assert!((c.lateral_speed_d58 - v.dot(sim.body.right())).abs() < 1e-4);
    }

    #[test]
    fn mass_unit_is_tonnes() {
        assert_eq!(mass_to_original(1250.0), 1.25);
    }
}

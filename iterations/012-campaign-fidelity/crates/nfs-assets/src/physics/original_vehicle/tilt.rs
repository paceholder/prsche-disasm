//! The tilted-car contact of `Car_GroundContactTick` `0x49bd40`: when the car
//! is not upright on its ground (`+0x3a0 < 0.939`) and touches it
//! (`+0x424 < 0.001`), the tick does not snap the body onto the wheel contact
//! plane. It calls `0x496610(car, n, p, k)` with the plane of the body support
//! polygon (`n` = the normal of the body owner `car+8`, `0x474420`; `p` = the
//! alternate-plane point `+0x400`; `k` = 0.1 when `+0x3a0 > 0.707`, else the
//! first value of the surface pair `[0x5d0b94 + 8·(car+0x2c & 0xf)]`).
//!
//! Source: the x87 instructions of `0x49bf77..0x49c000`, `0x496610` and the
//! impulse at a point `0x495f30`:
//! - `0x496610`: the body points `+0x1dc` (count `+0x32c`) with
//!   `-(point - p)·n > -0.01` are counted, their mean `m` and the deepest
//!   penetration kept; `ω *= 2π`, `0x495f30(car, m, ω×(m−pos)+v, n, k)`,
//!   `pos += n·deepest` (also the four wheel records `+0x7f8`), `ω /= 2π`,
//!   `Car_ClampAngularVelocity` `0x4964d0`, `+0x430 = 4|n·v|`,
//!   `Car_ImpactByDirection(car, n, 0)`.
//! - `0x495f30`: normal impulse `j = 0.3 · −(v·n + ω·(r×n)) / (2·|r×n|/I·½ + ¼/m)`
//!   (r = m − pos), a friction impulse `k·j` against the tangential point
//!   velocity, limited to stop it; for `j < 0.001` (the car moves away from
//!   the plane) it only damps `v` and `ω` by 0.9; else `v += (n·j + t)·½/m`,
//!   `ω += r×(n·j + t)·2/I`.
//!
//! The kernels are the ones of the track-object box response
//! (`original_track_objects::body_contact`, verified there); this module
//! runs them on the vehicle model's car state. Verified in the ground tick
//! against the original on the Auvergne border hill
//! (`scripts/research/replay-car-launch.py`, test
//! `tests/original_road_support.rs` "border_hill_drives_match_the_original").
//! The contact bookkeeping `+0x430..+0x448` is computed but not kept (no
//! consumer for the player yet).

use super::car::OriginalCar;
use super::driver::pair_car;
use crate::physics::original_pair::PairGlobals;
use crate::physics::original_track_objects::{body_contact, ObjectCar};

/// Friction of the tilted contact per surface: the first value of the pair at
/// `[0x5d0b94 + 8·s]` (`nfs5.exe` .data; the second value is the landing
/// bounce, `ground.rs` `BOUNCE`).
pub const FRICTION: [u32; 16] = [
    0x3f00_0000,
    0x3ecc_cccd,
    0x3f19_999a,
    0x3f19_999a,
    0x3ecc_cccd,
    0x3f19_999a,
    0x3e80_0000,
    0x3ecc_cccd,
    0x3dcc_cccd,
    0x3e4c_cccd,
    0x3f0c_cccd,
    0x3ecc_cccd,
    0x3f80_0000,
    0x3f80_0000,
    0x3f80_0000,
    0x3f80_0000,
];

/// `0x496610(car, n, p, k)` on the vehicle model's car: contact of the body
/// points `+0x1dc` with the plane through `p` with normal `n` (see the module
/// docs). Writes back position, velocity, angular velocity, the wheel
/// records `+0x7f8` and the zone damage of `Car_ImpactByDirection` mode 0
/// (gated by `[0x657a2c]`).
pub fn body_point_contact(car: &mut OriginalCar, n: [f32; 3], p: [f32; 3], k: f32) {
    let mut o = ObjectCar {
        pair: pair_car(car, car.half_extents_3a8),
        wheel_records_7f8: car.wheels.map(|w| w.point_40),
        body_points_1dc: car.world_points_1dc.clone(),
        local_velocity_d58: car.vel_ground_d58,
        field_3a0: car.ground_up_3a0,
        field_428: car.height_428,
        code_464: car.surface_464 as u32,
        ground_right_490: car.ground_490[0],
        ground_forward_4a8: car.ground_490[2],
    };
    let mut globals = PairGlobals {
        damage_on_657a2c: car.globals.flip_events_657a2c != 0,
        ..PairGlobals::default()
    };
    body_contact(&mut o, n, p, k, &mut globals);
    car.position_330 = o.pair.position_330;
    car.velocity_33c = o.pair.velocity_33c;
    car.angular_388 = o.pair.angular_388;
    for (w, r) in car.wheels.iter_mut().zip(o.wheel_records_7f8) {
        w.point_40 = r;
    }
    car.damage_4e4[..9].copy_from_slice(&o.pair.damage_4e4);
}

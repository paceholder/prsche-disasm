//! Original car-versus-track-object kernels from `nfs5.exe` (code bytes match
//! `Porsche.exe` v3.5): the solid responses for cylinder and box objects
//! (trees, pillars, crates).
//!
//! Source: `docs/re/track-objects.md` in the workspace root and the x87
//! instructions of
//! - `Car_ObjectNearTest` `0x498280` ([`near_test`]),
//! - the cylinder response `0x498aa0` ([`cylinder_response`]) with the block
//!   response `0x498380` (positional push, velocity kick, `0x493f10`,
//!   `0x493e40`, `Car_ApplyImpact` mode 1),
//! - the box response `0x498690` ([`box_response`]) with the vtable segment
//!   test `0x486d70`, the XZ segment intersection `0x489cc0`, the body-point
//!   contact `0x496610`, the impulse at a point `0x495f30` and
//!   `Car_ImpactByDirection` `0x494df0` (mode 0).
//!
//! Not ported: the `+0x52c & 4` branch of `0x493e40` (`0x493dd0`, wheel fields
//! the fixtures never touch). Not covered by the recorded fixtures (ported from
//! the instructions): the flag-0x80 branch of `0x495f30` and the `0x464` code.
//!
//! Arithmetic follows the original operation order: f64 stands in for the x87
//! extended accumulator and every original f32 store is rounded with `as f32`.
//! Verified against outputs recorded by executing the original bytes
//! (`scripts/research/replay-track-objects.py`, test
//! `tests/original_track_objects.rs`).

use super::original_contact::{damp_velocity, yaw_kick};
use super::original_pair::{
    PairCar, PairGlobals, add, apply_impact, clamp_angular_velocity, cross, dot_ext,
    impact_by_direction, length_ext, normalize, scale, sub,
};

/// Car fields read or written by the track-object kernels. Names keep the
/// original offsets; the pair kernels' fields are embedded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectCar {
    /// Shared fields (`+0x330`, `+0x33c`, axes, damage, ...).
    pub pair: PairCar,
    /// The four wheel records' position (`+0x7f8 + i * 0xc4`).
    pub wheel_records_7f8: [[f32; 3]; 4],
    /// Body points `+0x1dc` (12 bytes each); their count is `+0x32c`.
    pub body_points_1dc: Vec<[f32; 3]>,
    /// Velocity in the car frame (`+0xd58`, `+0xd5c`, `+0xd60`).
    pub local_velocity_d58: [f32; 3],
    /// Read by the box damping of `0x495f30` (`< 0.2`).
    pub field_3a0: f32,
    /// Read by the box damping of `0x495f30` (`< 0.005`).
    pub field_428: f32,
    /// Code seed `+0x464` of `0x495f30` (overwritten by the callers).
    pub code_464: u32,
    /// Ground frame right/forward axes (`+0x490`, `+0x4a8`) used by `0x493f10`.
    pub ground_right_490: [f32; 3],
    pub ground_forward_4a8: [f32; 3],
}

/// `WTreeElem_CylinderObject` (vtable `0x5b490c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CylinderObject {
    /// Bounds min/max at `obj+8` and `obj+0x14`.
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// `obj+0x34`.
    pub radius: f32,
    /// `obj+0x20`.
    pub id: u32,
}

/// `WTreeElem_BoxObject` (vtable `0x5b4938`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxObject {
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Footprint rectangle corners at `obj+0x34`, in edge order.
    pub corners: [[f32; 3]; 4],
    /// `obj+0x20`.
    pub id: u32,
}

/// Vtable slot `+4` `0x486490`: `(max + min) * 0.5` per component.
fn bounds_centre(min: [f32; 3], max: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| ((max[i] as f64 + min[i] as f64) * 0.5) as f32)
}

impl CylinderObject {
    pub fn centre(&self) -> [f32; 3] {
        bounds_centre(self.bounds_min, self.bounds_max)
    }
}

impl BoxObject {
    pub fn centre(&self) -> [f32; 3] {
        bounds_centre(self.bounds_min, self.bounds_max)
    }
}

/// `Car_ObjectNearTest` `0x498280`: with `d = (c.x - car.x, 0, c.z - car.z)`,
/// the object is near unless `|d·R| > r + hw` or `|d·F| > r + hl`.
pub fn near_test(car: &ObjectCar, centre: [f32; 3], radius: f32) -> bool {
    let p = &car.pair;
    let d = [
        (centre[0] as f64 - p.position_330[0] as f64) as f32,
        0.0,
        (centre[2] as f64 - p.position_330[2] as f64) as f32,
    ];
    if radius as f64 + (p.extents_3a8[0] as f64) < dot_ext(d, p.axis_r_364).abs() {
        return false;
    }
    radius as f64 + (p.extents_3a8[2] as f64) >= dot_ext(d, p.axis_f_37c).abs()
}

const NEG_0_95: f32 = f32::from_bits(0xbf73_3333);
const POS_0_95: f32 = f32::from_bits(0x3f73_3333);

/// Cylinder response `Car_CollideCylinder` `0x498aa0`: the car's box reach
/// along the centre line against the cylinder circle, then the block
/// response.
pub fn cylinder_response(car: &mut ObjectCar, obj: &CylinderObject, globals: &mut PairGlobals) {
    let p = &car.pair;
    // The car centre and the cylinder centre are flattened to y = 0.
    let c = {
        let mut c = obj.centre();
        c[1] = 0.0;
        c
    };
    let pos = [p.position_330[0], 0.0, p.position_330[2]];
    let v = sub(pos, c);
    if length_ext(v) <= f32::from_bits(0x3c23_d70a) as f64 {
        return;
    }
    let mut n = normalize(v);
    let distance = length_ext(v) as f32;
    let r = obj.radius;
    let reach_r = dot_ext(n, p.axis_r_364).abs();
    let reach_sa: f32 = if reach_r <= f32::from_bits(0x3dcc_cccd) as f64 {
        10.0
    } else {
        (p.extents_3a8[0] as f64 / reach_r) as f32
    };
    let reach_f = dot_ext(n, p.axis_f_37c).abs();
    let reach_sf = if reach_f <= f32::from_bits(0x3dcc_cccd) as f64 {
        10.0
    } else {
        p.extents_3a8[2] as f64 / reach_f
    };
    let s = if reach_sa as f64 <= reach_sf {
        reach_sa
    } else {
        reach_sf as f32
    };
    if (distance as f64) >= s as f64 + r as f64 {
        return;
    }
    if p.speed_35c > 5.0 {
        let heading = normalize(p.velocity_33c);
        if dot_ext(n, heading) > 0.5 && distance < 0.75 {
            n = scale(n, -1.0);
        }
    }
    let surface = add(c, scale(n, r));
    let body = add(pos, scale(n, -s));
    block_response(car, n, surface, body, globals);
    car.pair.code_438 = obj.id | 0x50000;
    car.pair.other_434 = None;
    car.pair.contact_pos_440 = surface;
}

/// Block response `0x498380(car, n, A, B)`: push the car out by `n·((A-B)·n)`
/// (body and wheel records), turn the push into velocity (x32), cap the
/// speed, then yaw kick, velocity damping and `Car_ApplyImpact` mode 1.
fn block_response(
    car: &mut ObjectCar,
    n: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    globals: &mut PairGlobals,
) {
    let g = dot_ext(n, sub(a, b)) as f32;
    let push = scale(n, g);
    let p = &mut car.pair;
    p.position_330 = add(push, p.position_330);
    for record in &mut car.wheel_records_7f8 {
        for axis in [0, 2] {
            record[axis] = (push[axis] as f64 + record[axis] as f64) as f32;
        }
    }
    let along = dot_ext(p.velocity_33c, n);
    let strength = (-along) as f32;
    p.impact_430 = ((-along * 3.0).abs()) as f32;
    let vx = (push[0] as f64 * 32.0 + p.velocity_33c[0] as f64) as f32;
    p.velocity_33c[0] = vx;
    p.velocity_33c[1] = (push[1] as f64 * 32.0 + p.velocity_33c[1] as f64) as f32;
    // The z sum stays extended for the speed estimate below (`fst` at 0x4984fc).
    let vz = push[2] as f64 * 32.0 + p.velocity_33c[2] as f64;
    p.velocity_33c[2] = vz as f32;
    let (ax, az) = ((vx as f64).abs(), vz.abs());
    let estimate = if ax <= az {
        az + ax * 0.25
    } else {
        ax + az * 0.25
    };
    let cap_ext = p.speed_35c as f64 / estimate;
    let cap = cap_ext as f32;
    if cap_ext < 1.0 {
        p.velocity_33c = scale(p.velocity_33c, cap);
        p.impact_430 = (cap as f64 * p.impact_430 as f64) as f32;
    }

    // 0x493f10: clears +0x434 and sets the yaw field unless the gate holds.
    p.other_434 = None;
    if let Some(yaw) = yaw_kick(
        strength,
        car.local_velocity_d58[2],
        n,
        car.ground_forward_4a8,
        car.ground_right_490,
    ) {
        p.angular_388[1] = yaw;
    }
    // 0x493e40: velocity x0.998, local velocity, speed estimate.
    debug_assert!(p.flags_52c & 4 == 0, "0x493dd0 (+0x52c & 4) is not ported");
    (car.local_velocity_d58, p.speed_35c) = damp_velocity(
        &mut p.velocity_33c,
        [p.axis_r_364, p.axis_u_370, p.axis_f_37c],
    );

    if p.impact_430 > 10.0 {
        let strength = p.impact_430;
        let right = dot_ext(n, p.axis_r_364) as f32;
        let forward_ext = dot_ext(n, p.axis_f_37c);
        let forward = forward_ext as f32;
        let zone = if forward_ext < NEG_0_95 as f64 {
            1
        } else if forward > POS_0_95 {
            5
        } else if right < NEG_0_95 {
            3
        } else if right > POS_0_95 {
            7
        } else if forward >= 0.0 {
            if right >= 0.0 { 4 } else { 6 }
        } else if right >= 0.0 {
            2
        } else {
            0
        };
        apply_impact(p, strength, zone, 1, globals);
    }
}

/// `0x489cc0`: intersection of two segments in XZ. Returns the point on `p1`
/// (`p1[0] + (p1[1] - p1[0]) * t`, all three components). The first segment's
/// directions and the first parameter are stored as f32; the second segment's
/// directions stay extended (same arithmetic as `edge_intersects_segment` in
/// `original_scene_objects.rs`, which only returns the boolean).
fn intersect_xz(p1: [[f32; 3]; 2], p2: [[f32; 3]; 2]) -> Option<[f32; 3]> {
    let qx = (p1[1][0] as f64 - p1[0][0] as f64) as f32;
    let qz = (p1[1][2] as f64 - p1[0][2] as f64) as f32;
    let ex = p2[1][0] as f64 - p2[0][0] as f64;
    let ez = p2[1][2] as f64 - p2[0][2] as f64;
    let det_ext = qx as f64 * ez - ex * qz as f64;
    if det_ext == 0.0 {
        return None;
    }
    let det = det_ext as f32 as f64;
    let az = (p1[0][2] as f64 - p2[0][2] as f64) as f32;
    let ax = (p1[0][0] as f64 - p2[0][0] as f64) as f32;
    let t = ((az as f64 * ex - ax as f64 * ez) / det) as f32;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let u = (az as f64 * qx as f64 - ax as f64 * qz as f64) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let d = sub(p1[1], p1[0]);
    Some([0, 1, 2].map(|i| (d[i] as f64 * t as f64 + p1[0][i] as f64) as f32))
}

/// Box vtable slot `+0x18` `0x486d70`: test a car edge `seg` (XZ) against the
/// four footprint edges. Returns the number of crossings; `normal` and
/// `point` are outputs (`point` keeps its old value when nothing crosses).
/// The first crossing gives the wall normal `(-dz, 0, dx)` and the crossing
/// point; later crossings replace the normal by the rotated segment direction
/// `(sz, 0, -sx)` and the point by the crossing of that edge with the first.
fn box_segment_test(
    obj: &BoxObject,
    seg: [[f32; 3]; 2],
    normal: &mut [f32; 3],
    point: &mut [f32; 3],
) -> u32 {
    let mut count = 0;
    *normal = [0.0; 3];
    let mut first = [[0.0; 3]; 2];
    for i in 0..4 {
        let edge = [obj.corners[i], obj.corners[(i + 1) & 3]];
        let Some(crossing) = intersect_xz(edge, seg) else {
            continue;
        };
        *point = crossing;
        count += 1;
        let mut d = sub(edge[1], edge[0]);
        d[1] = 0.0;
        let d = normalize(d);
        let wall = [-d[2], 0.0, d[0]];
        if *normal == [0.0; 3] {
            first = edge;
            *normal = wall;
        } else {
            if let Some(corner) = intersect_xz(edge, first) {
                *point = corner;
            }
            let s = normalize(sub(seg[1], seg[0]));
            *normal = [s[2], 0.0, -s[0]];
        }
    }
    count
}

/// Table `0x5d0b6c`: corner index pairs of the four car edges.
const CAR_EDGES: [(usize, usize); 4] = [(2, 0), (0, 1), (1, 3), (3, 2)];

/// Box response `Car_CollideBox` `0x498690`: the car's four corner edges
/// (XZ) against the footprint rectangle; an edge crossing twice is used at
/// once, one crossing is accepted by the body point that penetrates the wall
/// plane most (depth below the speed-scaled limit). The contact is applied by
/// `0x496610`.
pub fn box_response(car: &mut ObjectCar, obj: &BoxObject, globals: &mut PairGlobals) {
    let p = &car.pair;
    let pos = p.position_330;
    let forward = scale(p.axis_f_37c, p.extents_3a8[2]);
    let right = scale(p.axis_r_364, p.extents_3a8[0]);
    let down = scale(p.axis_u_370, -p.extents_3a8[1]);
    // The original builds corners 1 and 2 from `pos - down` (the top).
    let corners = [
        sub(add(add(pos, down), forward), right),
        add(add(sub(pos, down), forward), right),
        sub(sub(sub(pos, down), forward), right),
        add(sub(add(pos, down), forward), right),
    ];
    let mut chosen: Option<([f32; 3], [f32; 3])> = None;
    let mut best_so_far = 999.0_f32;
    let mut normal = [0.0; 3];
    let mut point = [0.0; 3];
    for (i, j) in CAR_EDGES {
        let seg = [
            [corners[i][0], 0.0, corners[i][2]],
            [corners[j][0], 0.0, corners[j][2]],
        ];
        let crossings = box_segment_test(obj, seg, &mut normal, &mut point);
        if crossings > 1 {
            chosen = Some((normal, point));
            break;
        }
        if crossings == 1 {
            let product = p.speed_35c as f64 * 0.125;
            let limit = if f32::from_bits(0x3dcc_cccd) as f64 <= product {
                product as f32
            } else {
                f32::from_bits(0x3dcc_cccd)
            };
            let floor = f32::from_bits(0xbc23_d70a) as f64;
            let mut deepest = -99.0_f32;
            for body_point in &car.body_points_1dc {
                let depth = -dot_ext(sub(*body_point, point), normal);
                if depth > floor && depth > deepest as f64 {
                    deepest = depth as f32;
                }
            }
            if deepest < best_so_far && deepest as f64 > floor && deepest < limit {
                chosen = Some((normal, point));
                best_so_far = deepest;
            }
        }
    }
    if let Some((normal, point)) = chosen {
        body_contact(car, normal, point, f32::from_bits(0x3dcc_cccd), globals);
        let p = &mut car.pair;
        p.code_438 = obj.id | 0x50000;
        p.contact_pos_440 = p.position_330;
    }
}

/// `0x496610(car, n, p, k)`: contact of the body points behind the wall plane
/// through `p` with normal `n`: impulse at their mean point, push out by the
/// deepest, angular clamp, impact strength `4|n·v|` and `0x494df0` (mode 0).
pub(crate) fn body_contact(
    car: &mut ObjectCar,
    n: [f32; 3],
    plane_point: [f32; 3],
    k: f32,
    globals: &mut PairGlobals,
) {
    let mut deepest = -1.0_f32;
    let mut count = 0.0_f32;
    let mut sum = [0.0_f32; 3];
    let floor = f32::from_bits(0xbc23_d70a) as f64;
    for body_point in &car.body_points_1dc {
        let depth = -dot_ext(sub(*body_point, plane_point), n);
        if depth > floor {
            count = (count as f64 + 1.0) as f32;
            if depth > deepest as f64 {
                deepest = depth as f32;
            }
            for axis in 0..3 {
                sum[axis] = (sum[axis] as f64 + body_point[axis] as f64) as f32;
            }
        }
    }
    if count <= 0.0 {
        return;
    }
    if deepest < 0.0 {
        deepest = 0.0;
    }
    let mean = scale(sum, (1.0 / count as f64) as f32);
    let p = &mut car.pair;
    p.angular_388 = scale(p.angular_388, f32::from_bits(0x40c9_0fd0));
    let offset = sub(mean, p.position_330);
    let point_velocity = add(cross(p.angular_388, offset), p.velocity_33c);
    impulse_at_point(car, mean, point_velocity, n, k);
    let p = &mut car.pair;
    let push = scale(n, deepest);
    p.position_330 = add(p.position_330, push);
    for record in &mut car.wheel_records_7f8 {
        *record = add(*record, push);
    }
    p.angular_388 = scale(p.angular_388, f32::from_bits(0x3e22_f987));
    clamp_angular_velocity(p);
    p.impact_430 = (dot_ext(n, p.velocity_33c).abs() * 4.0) as f32;
    impact_by_direction(p, n, 0, globals);
}

/// `0x495f30(car, m, vp, n, k)`: impulse at point `m` (point velocity `vp`,
/// normal `n`), zero restitution, plus a friction impulse `k` of it along the
/// tangential velocity.
fn impulse_at_point(car: &mut ObjectCar, m: [f32; 3], vp: [f32; 3], n: [f32; 3], k: f32) {
    let code_464 = car.code_464;
    let (field_3a0, field_428) = (car.field_3a0, car.field_428);
    let p = &mut car.pair;
    let mut tangent = vp;
    let normal_speed = dot_ext(n, p.velocity_33c) as f32;
    let r = sub(m, p.position_330);
    let c = cross(r, n);
    let numerator = (dot_ext(p.angular_388, c) + normal_speed as f64) as f32;
    let quarter = (p.inv_mass_358 as f64 * 0.5 * 0.5) as f32;
    let length = length_ext(c);
    let denominator =
        ((p.inv_inertia_398 as f64 + p.inv_inertia_398 as f64) * length) * 0.5 + quarter as f64;
    let j0 = -(numerator as f64 / denominator);
    let [hw, hh, hl] = p.extents_3a8.map(|e| e as f64);
    let normal_impulse = if p.flags_52c & 0x80 == 0 {
        (j0 * f32::from_bits(0x3e99_999a) as f64) as f32
    } else {
        let mut largest = if hw > hh { hw } else { hh };
        if largest <= hl {
            largest = hl;
        }
        if largest > f32::from_bits(0x3fc0_0000) as f64 {
            ((j0 * f32::from_bits(0x3f19_999a) as f64) / largest) as f32
        } else {
            (j0 * f32::from_bits(0x3ecc_cccd) as f64) as f32
        }
    };
    p.other_434 = None;
    p.impact_430 = normal_speed.abs();
    p.code_438 = code_464 | 0x30000;
    p.contact_pos_440 = m;

    if k > 0.0 {
        let mut tangent_length = 0.0_f32;
        if tangent != [0.0; 3] {
            let d = dot_ext(tangent, n);
            tangent = [0, 1, 2].map(|i| (tangent[i] as f64 - n[i] as f64 * d) as f32);
            let squared = dot_ext(tangent, tangent) as f32;
            tangent_length = (squared as f64).sqrt() as f32;
            let factor = if tangent_length as f64 <= f32::from_bits(0x3c23_d70a) as f64 {
                -1.0
            } else {
                (-(0.5 / (tangent_length as f64 * 0.5))) as f32
            };
            tangent = scale(tangent, factor);
        }
        let mut friction = (normal_impulse as f64 * k as f64) as f32;
        let mut linear = scale(
            tangent,
            ((friction as f64 * p.inv_mass_358 as f64) * 0.5) as f32,
        );
        let spin_factor = (friction as f64 * p.inv_inertia_398 as f64 * 2.0) as f32;
        let spin = scale(cross(cross(tangent, r), r), spin_factor);
        linear = add(linear, spin);
        let along = dot_ext(linear, tangent);
        if (tangent_length as f64) < along {
            friction = ((tangent_length as f64 / along) * friction as f64) as f32;
        }
        tangent = scale(tangent, friction);
    }
    if (normal_impulse as f64) < f32::from_bits(0x3a83_126f) as f64 {
        let damp = f32::from_bits(0x3f66_6666);
        p.angular_388 = scale(p.angular_388, damp);
        p.velocity_33c = scale(p.velocity_33c, damp);
        return;
    }
    let total = add(scale(n, normal_impulse), tangent);
    p.velocity_33c = add(
        p.velocity_33c,
        scale(total, (p.inv_mass_358 as f64 * 0.5) as f32),
    );
    let turn = scale(
        cross(r, total),
        (p.inv_inertia_398 as f64 + p.inv_inertia_398 as f64) as f32,
    );
    p.angular_388 = add(p.angular_388, turn);
    let light = p.flags_52c & 0x80 != 0 || field_3a0 < f32::from_bits(0x3e4c_cccd);
    if light
        && field_428 < f32::from_bits(0x3ba3_d70a)
        && normal_impulse < 10.0
        && p.angular_388.iter().all(|w| w.abs() < 1.0)
    {
        p.angular_388 = scale(p.angular_388, 0.75);
        p.velocity_33c = scale(p.velocity_33c, f32::from_bits(0x3f73_3333));
    }
}

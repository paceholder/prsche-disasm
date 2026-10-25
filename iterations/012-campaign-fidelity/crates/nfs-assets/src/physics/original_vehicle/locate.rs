//! `Car_TrackLocate` `0x49af70` (32 Hz queue, prio 6): the car's place on
//! the track. Workspace notes: `docs/re/support.md` section 4.
//!
//! For an active car (`+0x7f`, not frozen `+0x7d`, ground on `+0x7c`):
//! 1. `0x417190`: `+0x7e = 0`; `+0x78` = 0 for a race participant
//!    (`+0x52c & 1`), else the distance to the nearest race car
//!    (`max + 0.25·min` of |dx|, |dz|, at most 1000).
//! 2. `TrackLocator_Update` `0x474440` on `car+8` (the node graph).
//! 3. `0x474060(car+0x330, reuse)` on the same `car+8`, which is also the
//!    body support owner; its "polygon changed" byte `+0x09` (`car+0x11`)
//!    overwrites the locator's "node changed".
//! 4. If the polygon changed, `0x499520`: the alternate plane (normal
//!    `+0x3e8`, mean of the polygon's four vertices `+0x400` — a triangle's
//!    vertex getter wraps, so its vertex 0 counts twice — edge direction
//!    `+0x3f4`, `+0x3dc = +0x3e8 × +0x3f4`) and the node frame copy
//!    `+0x3b8`/`+0x3c4`/`+0x3d0`.
//! 5. Surface `+0x460` = the flags word `car+0x2c`, `+0x464` = its low
//!    nibble (1 and 10 become 2 in RALLY mode).
//! 6. Heading `+0x414 = asin(F·node lateral)`, mirrored to `±π − h` when
//!    `F·node forward < 0`.
//!
//! Verified against `scripts/research/replay-road-support.py` (closed loops
//! on real tracks, `tests/original_road_support.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::car::OriginalCar;
use super::step::{cross, dot_ext, x};
use crate::physics::original_support::SupportPolygon;
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::physics::original_support_tree::SupportTree;
use crate::race::original_ai::query::TrackLocator;
use crate::race::original_ai::track::TrackNodes;

/// `car+8`: the track locator and the body support owner, one
/// struct in the original. The default is [`BodyLocator::at_node`]`(0)`.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyLocator {
    pub track: TrackLocator,
    pub owner: OriginalSupportOwner,
}

impl Default for BodyLocator {
    fn default() -> Self {
        Self::at_node(0)
    }
}

impl BodyLocator {
    /// A locator at `node` with no section and no polygon (as after
    /// `Car_ResetDriveState` `0x49e220`).
    pub fn at_node(node: i32) -> Self {
        Self {
            track: TrackLocator {
                node_00: node as i16,
                section_04: -1,
                ..Default::default()
            },
            owner: OriginalSupportOwner::default(),
        }
    }

    /// `car+0x12`: no support polygon under the car.
    pub fn missing_12(&self) -> bool {
        self.owner.missing_0a
    }
}

/// The support arena (`[0x628960]+0x24` and its polygons).
pub type SupportArena<'a> = (&'a SupportTree, &'a [SupportPolygon]);

const PI_HI: u32 = 0x4049_0fd9; // 0x5b4a14, 3.1415925
const PI_LO: u32 = 0xc049_0fd9; // 0x5b4a10, -3.1415925
const K_DIST_MAX: u32 = 0x447a_0000; // 1000.0

/// `0x417190`: `+0x7e` and `+0x78`. `racers` are the positions of the
/// cars in `[0x5e8e2c]` (count `[0x5e8ae8]`, race participants).
fn nearest_racer_417190(car: &mut OriginalCar, racers: &[[f32; 3]]) {
    car.track.flag_7e = 0;
    if car.flags_52c & 1 != 0 {
        car.track.dist_78 = 0.0;
        return;
    }
    let mut d78 = f32::from_bits(K_DIST_MAX);
    let p = car.position_330;
    for o in racers {
        let dx = ((x(p[0]) - x(o[0])) as f32).abs();
        let dz = ((x(p[2]) - x(o[2])) as f32).abs();
        let d = if dx <= dz {
            x(dx) * 0.25 + x(dz)
        } else {
            x(dz) * 0.25 + x(dx)
        };
        let d = if x(d78) < d { x(d78) } else { d };
        d78 = d as f32;
    }
    car.track.dist_78 = d78;
}

/// `0x499520`: the alternate plane and the node frame from the body owner.
pub fn alternate_plane_499520(
    car: &mut OriginalCar,
    locator: &mut BodyLocator,
    nodes: &TrackNodes,
    polygons: &[SupportPolygon],
) {
    if car.active_7f == 0 {
        return;
    }
    let old = car.alt_base_400;
    let mut acc = [0.0f32; 3];
    if !locator.owner.missing_0a {
        let mut last_x = 0.0f64;
        for i in 0..4 {
            let v = locator.owner.vertex(polygons, i);
            last_x = x(v[0]) + x(acc[0]);
            acc[0] = last_x as f32;
            acc[1] = (x(v[1]) + x(acc[1])) as f32;
            acc[2] = (x(v[2]) + x(acc[2])) as f32;
        }
        // x from the sum still on the x87 stack, y and z from the stores.
        acc = [
            (last_x * 0.25) as f32,
            (x(acc[1]) * 0.25) as f32,
            (x(acc[2]) * 0.25) as f32,
        ];
    }
    car.alt_base_400 = acc;
    // fcomp/C3: NaN counts as zero.
    let zero = |c: f32| c == 0.0 || c.is_nan();
    if zero(acc[0]) && zero(acc[1]) && zero(acc[2]) {
        car.alt_base_400 = old;
        return;
    }
    if locator.owner.changed_09 {
        let n = locator.owner.normal(polygons);
        let t = locator.owner.tangent(polygons);
        car.alt_normal_3e8 = n;
        car.track.alt_tangent_3f4 = t;
        car.track.alt_side_3dc = cross(n, t);
        if let Some(node) = nodes.nodes.get(locator.track.node_00 as usize) {
            car.track.node_lat_3b8 = node.lateral_28;
            car.track.node_up_3c4 = node.up_10;
            car.track.node_fwd_3d0 = node.forward_1c;
        }
    }
}

/// `Car_TrackLocate` `0x49af70`. `support` is `None` on the legacy triangle
/// grid (no original support): the owner then keeps no polygon.
pub fn track_locate(
    car: &mut OriginalCar,
    locator: &mut BodyLocator,
    nodes: &TrackNodes,
    support: Option<SupportArena<'_>>,
    racers: &[[f32; 3]],
) {
    if car.active_7f == 0 || car.frozen_7d != 0 || car.ground_on_7c == 0 {
        return;
    }
    nearest_racer_417190(car, racers);
    locator.track.update(nodes, car.position_330);
    match support {
        Some((tree, polygons)) => {
            let _ = locator.owner.locate(tree, polygons, car.position_330, true);
        }
        None => locator.owner.changed_09 = false,
    }
    locator.track.changed_09 = locator.owner.changed_09;
    if locator.owner.changed_09 {
        let polygons = support.map_or(&[][..], |(_, p)| p);
        alternate_plane_499520(car, locator, nodes, polygons);
    }
    let flags = locator.owner.flags_24;
    car.body_surface_2c = flags;
    car.track.surface_460 = flags as i32;
    let surface = (flags & 0xf) as i32;
    car.surface_464 = if car.globals.handling_mode_6573f8 != 0 && (surface == 1 || surface == 10) {
        2
    } else {
        surface
    };
    let Some(node) = nodes.nodes.get(locator.track.node_00 as usize) else {
        return;
    };
    let f = car.axes_364[2];
    car.track.heading_414 = dot_ext(f, node.lateral_28).asin() as f32;
    if dot_ext(f, node.forward_1c) < 0.0 {
        let h = x(car.track.heading_414);
        let k = if !(car.track.heading_414 <= 0.0) {
            f32::from_bits(PI_HI)
        } else {
            f32::from_bits(PI_LO)
        };
        car.track.heading_414 = (x(k) - h) as f32;
    }
}

/// `Car_ResetNodeFree` `0x40f480`: `0x474060(node centre, reuse)` on
/// `car+8` finds a polygon with a surface nibble, and no other active racer
/// (`others`: their locator nodes) is on `node`.
pub fn reset_node_free(
    locator: &mut BodyLocator,
    nodes: &TrackNodes,
    support: Option<SupportArena<'_>>,
    node: i32,
    others: &[i16],
) -> bool {
    let Some(n) = nodes.nodes.get(node as usize) else {
        return false;
    };
    if let Some((tree, polygons)) = support {
        let _ = locator.owner.locate(tree, polygons, n.pos_04, true);
    }
    locator.owner.flags_24 & 0xf != 0 && !others.contains(&(node as i16))
}

/// The node choice of `Car_ResetToTrack` `0x40f540`: from the locator node,
/// the first free node, stepping `TrackNodes_Step(node, ±1, −1)` (−1 with
/// REVERSE). Bounded by the node count (the original loops until free).
pub fn reset_node(
    locator: &mut BodyLocator,
    nodes: &TrackNodes,
    support: Option<SupportArena<'_>>,
    others: &[i16],
    reverse: bool,
) -> i32 {
    let mut node = locator.track.node_00 as i32;
    for _ in 0..nodes.nodes.len() {
        if reset_node_free(locator, nodes, support, node, others) {
            break;
        }
        node = nodes.step(node, if reverse { -1 } else { 1 }, -1);
    }
    node
}

/// `0x417260`/`0x417310`: the node frame `+0x3b8..` into the body axes and
/// the ground frame, lateral and forward negated when `negate`.
fn copy_frame(car: &mut OriginalCar, negate: bool) {
    let t = car.track;
    let s = |v: [f32; 3]| if negate { v.map(|c| -c) } else { v };
    let axes = [s(t.node_lat_3b8), t.node_up_3c4, s(t.node_fwd_3d0)];
    car.axes_364 = axes;
    car.ground_490 = axes;
}

/// `Car_PlaceOnNode` `0x49b090` for a car without the traffic offset, up
/// to its ground queries (the engine's probe placement follows, A39).
/// `0x473210` resets the locator to `max(node, 1)` (it also runs inside
/// the vertex loop, so `+0x400` sums four zero vertices), the plane comes
/// from the stale owner normal, the position is the node centre, and the
/// final frame is the one `Car_TrackLocate` copies from the located node.
/// `reverse` is `+0xeac != 1`.
pub fn place_on_node(
    car: &mut OriginalCar,
    locator: &mut BodyLocator,
    nodes: &TrackNodes,
    support: Option<SupportArena<'_>>,
    node: i32,
    reverse: bool,
    racers: &[[f32; 3]],
) {
    let k = node.max(1);
    let (Some(n), Some(centre)) = (nodes.nodes.get(k as usize), nodes.nodes.get(node as usize))
    else {
        return;
    };
    let reset = |l: &mut BodyLocator| {
        // 0x473210: keeps the normal +0x0c and the edge direction +0x18.
        l.track = TrackLocator {
            node_00: k as i16,
            section_04: -1,
            ..Default::default()
        };
        let o = &mut l.owner;
        (o.polygon, o.node, o.flags_24) = (None, None, 0);
        (o.normal_valid_02, o.changed_09, o.missing_0a) = (false, false, false);
    };
    reset(locator);
    let polygons = support.map_or(&[][..], |(_, p)| p);
    if let Some((tree, polygons)) = support {
        let _ = locator.owner.locate(tree, polygons, n.pos_04, true);
    }
    reset(locator);
    car.alt_base_400 = [0.0; 3];
    car.track.node_fwd_3d0 = nodes.nodes[locator.track.node_00 as usize].forward_1c;
    car.track.node_up_3c4 = locator.owner.normal(polygons);
    let lat = super::step::normalize(cross(car.track.node_up_3c4, car.track.node_fwd_3d0));
    car.track.node_lat_3b8 = lat;
    car.track.node_fwd_3d0 = super::step::normalize(cross(lat, car.track.node_up_3c4));
    copy_frame(car, reverse);
    car.alt_normal_3e8 = locator.owner.normal(polygons);
    car.track.alt_tangent_3f4 = locator.owner.tangent(polygons);
    car.track.alt_side_3dc = cross(car.alt_normal_3e8, car.track.alt_tangent_3f4);
    car.position_330 = centre.pos_04.map(|c| (x(c) + 0.0) as f32);
    car.speed_35c = 0.0;
    car.track.flag_7e = 0;
    track_locate(car, locator, nodes, support, racers);
    copy_frame(car, reverse);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn racers_have_no_distance() {
        let mut car = OriginalCar::new(
            crate::physics::original_vehicle::sim::CarSpecs::from_bytes(&[0u8; 0x1c4]),
        );
        car.flags_52c = 1;
        nearest_racer_417190(&mut car, &[[100.0, 0.0, 0.0]]);
        assert_eq!(car.track.dist_78, 0.0);
        car.flags_52c = 0;
        nearest_racer_417190(&mut car, &[[3.0, 0.0, 4.0]]);
        assert_eq!(car.track.dist_78, 4.75);
        nearest_racer_417190(&mut car, &[]);
        assert_eq!(car.track.dist_78, 1000.0);
    }
}

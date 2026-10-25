//! Road barrier and track boundary collision resolution.

use crate::TopologyEdge;

/// Configuration for vehicle-to-barrier collisions.
#[derive(Debug, Clone, PartialEq)]
pub struct BarrierCollisionConfig {
    pub vehicle_radius: f32,
    pub height_tolerance: f32,
    pub restitution: f32,
    pub wall_friction: f32,
}

impl Default for BarrierCollisionConfig {
    fn default() -> Self {
        Self {
            vehicle_radius: 1.25,
            height_tolerance: 1.6,
            restitution: 0.32,
            wall_friction: 0.28,
        }
    }
}

/// Result of a barrier collision test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarrierHit {
    pub contact_point: [f32; 3],
    pub normal: [f32; 3],
    pub penetration: f32,
    pub edge_flags: u8,
}

/// Resolves collisions against track boundary edges.
pub struct BarrierCollider;

impl BarrierCollider {
    /// Detects contact against a list of topology edges.
    pub fn detect_contact(
        pos: [f32; 3],
        edges: &[TopologyEdge],
        config: &BarrierCollisionConfig,
    ) -> Option<BarrierHit> {
        let px = pos[0];
        let py = pos[1];
        let pz = pos[2];

        let mut closest_hit: Option<BarrierHit> = None;
        let mut min_dist_sq = config.vehicle_radius * config.vehicle_radius;

        for edge in edges {
            let x1 = edge.p1[0];
            let z1 = edge.p1[2];
            let x2 = edge.p2[0];
            let z2 = edge.p2[2];

            let dx = x2 - x1;
            let dz = z2 - z1;
            let len_sq = dx * dx + dz * dz;
            if len_sq < 1e-4 {
                continue;
            }

            let t = (((px - x1) * dx + (pz - z1) * dz) / len_sq).clamp(0.0, 1.0);
            let closest_x = x1 + t * dx;
            let closest_z = z1 + t * dz;
            let closest_y = edge.p1[1] + t * (edge.p2[1] - edge.p1[1]);

            // Vertical tolerance check (layering on bridges/tunnels)
            if (py - closest_y).abs() > config.height_tolerance {
                continue;
            }

            let dist_x = px - closest_x;
            let dist_z = pz - closest_z;
            let dist_sq = dist_x * dist_x + dist_z * dist_z;

            if dist_sq < min_dist_sq && dist_sq > 1e-7 {
                min_dist_sq = dist_sq;
                let dist = dist_sq.sqrt();
                let penetration = config.vehicle_radius - dist;
                let nx = dist_x / dist;
                let nz = dist_z / dist;

                closest_hit = Some(BarrierHit {
                    contact_point: [closest_x, closest_y, closest_z],
                    normal: [nx, 0.0, nz],
                    penetration,
                    edge_flags: edge.flags,
                });
            }
        }

        closest_hit
    }

    /// Resolves barrier collision for a 3D position and velocity vector.
    /// Modifies `pos` and `vel` in place and returns `true` if a collision occurred.
    pub fn resolve_collision(
        pos: &mut [f32; 3],
        vel: &mut [f32; 3],
        edges: &[TopologyEdge],
        config: &BarrierCollisionConfig,
    ) -> bool {
        if let Some(hit) = Self::detect_contact(*pos, edges, config) {
            // Push out along normal
            pos[0] += hit.normal[0] * hit.penetration;
            pos[2] += hit.normal[2] * hit.penetration;

            // Velocity response
            let vn = vel[0] * hit.normal[0] + vel[2] * hit.normal[2];
            if vn < 0.0 {
                // Decompose into normal and tangential components
                let vt_x = vel[0] - vn * hit.normal[0];
                let vt_z = vel[2] - vn * hit.normal[2];

                // Bounce normal with restitution
                let new_vn = -config.restitution * vn;

                // Coulomb wall friction: the tangential loss is bounded by the
                // normal impulse, so a glancing scrape keeps most of its speed
                // instead of losing a fixed share on every contact frame.
                let vt = (vt_x * vt_x + vt_z * vt_z).sqrt();
                let friction_loss = config.wall_friction * (new_vn - vn);
                let friction_factor = if vt > 1e-6 {
                    (1.0 - friction_loss / vt).max(0.0)
                } else {
                    0.0
                };
                vel[0] = vt_x * friction_factor + new_vn * hit.normal[0];
                vel[2] = vt_z * friction_factor + new_vn * hit.normal[2];
            }

            true
        } else {
            false
        }
    }
}

/// Upright vehicle body used for a generic, horizontal yaw-only contact.
/// `center` and `previous_center` are the world-space centers of the source
/// model bounds, not the vehicle origin. Dimensions are half lengths in XYZ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleContactProxy {
    pub previous_center: [f32; 3],
    pub center: [f32; 3],
    pub velocity: [f32; 3],
    pub yaw: f32,
    pub half_extents: [f32; 3],
    pub inverse_mass: f32,
}

fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn horizontal_axes(yaw: f32) -> [[f32; 2]; 2] {
    [[yaw.cos(), -yaw.sin()], [yaw.sin(), yaw.cos()]]
}

fn radius_on_axis(body: &VehicleContactProxy, axis: [f32; 2]) -> f32 {
    let body_axes = horizontal_axes(body.yaw);
    body.half_extents[0] * dot2(body_axes[0], axis).abs()
        + body.half_extents[2] * dot2(body_axes[1], axis).abs()
}

/// Resolve one pair of moving vehicle boxes, including crossings during a frame.
/// Both positions and velocities change in inverse-mass proportion. This is a
/// conservative gameplay response, not a recovered original-game collision law.
pub fn resolve_vehicle_contact(a: &mut VehicleContactProxy, b: &mut VehicleContactProxy) -> bool {
    if a.half_extents.iter().any(|&v| !v.is_finite() || v <= 0.0)
        || b.half_extents.iter().any(|&v| !v.is_finite() || v <= 0.0)
        || !a.inverse_mass.is_finite()
        || !b.inverse_mass.is_finite()
        || a.inverse_mass < 0.0
        || b.inverse_mass < 0.0
        || a.inverse_mass + b.inverse_mass <= 0.0
    {
        return false;
    }

    let a_axes = horizontal_axes(a.yaw);
    let b_axes = horizontal_axes(b.yaw);
    let axes = [a_axes[0], a_axes[1], b_axes[0], b_axes[1]];
    let old_delta = [
        b.previous_center[0] - a.previous_center[0],
        b.previous_center[2] - a.previous_center[2],
    ];
    let new_delta = [b.center[0] - a.center[0], b.center[2] - a.center[2]];
    let mut entry = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut entry_normal = [1.0_f32, 0.0];
    let mut current_overlap = true;
    let mut least_penetration = f32::INFINITY;
    let mut current_normal = [1.0_f32, 0.0];

    for axis in axes {
        let radius = radius_on_axis(a, axis) + radius_on_axis(b, axis);
        let start = dot2(old_delta, axis);
        let end = dot2(new_delta, axis);
        let penetration = radius - end.abs();
        if penetration < 0.0 {
            current_overlap = false;
        } else if penetration < least_penetration {
            least_penetration = penetration;
            current_normal = if end >= 0.0 {
                axis
            } else {
                [-axis[0], -axis[1]]
            };
        }
        let travel = end - start;
        if travel.abs() < 1e-7 {
            if start.abs() > radius {
                return false;
            }
        } else {
            let t0 = (-radius - start) / travel;
            let t1 = (radius - start) / travel;
            let axis_entry = t0.min(t1);
            let axis_exit = t0.max(t1);
            if axis_entry > entry {
                entry = axis_entry;
                let separation_at_entry = start + travel * axis_entry;
                entry_normal = if separation_at_entry >= 0.0 {
                    axis
                } else {
                    [-axis[0], -axis[1]]
                };
            }
            exit = exit.min(axis_exit);
            if entry > exit {
                return false;
            }
        }
    }
    let horizontal_entry = entry;

    // Vertical bounds are swept as well, so cars on different road layers
    // cannot collide merely because their plan-view paths cross.
    let vertical_radius = a.half_extents[1] + b.half_extents[1];
    let start_y = b.previous_center[1] - a.previous_center[1];
    let end_y = b.center[1] - a.center[1];
    let travel_y = end_y - start_y;
    if travel_y.abs() < 1e-7 {
        if start_y.abs() >= vertical_radius {
            return false;
        }
    } else {
        let t0 = (-vertical_radius - start_y) / travel_y;
        let t1 = (vertical_radius - start_y) / travel_y;
        entry = entry.max(t0.min(t1));
        exit = exit.min(t0.max(t1));
        if entry > exit {
            return false;
        }
    }
    if end_y.abs() >= vertical_radius {
        current_overlap = false;
    }
    if !current_overlap && (entry <= 0.0 || entry > 1.0 || exit < 0.0) {
        return false;
    }

    // Frame displacement left unexecuted after rewinding to the impact time.
    let mut remainder = None;
    let (normal, correction) = if entry > 0.0 {
        // A car can cross the other one's center yet finish the frame still
        // overlapping. Resolve at first impact even in that case.
        let ends = [a.center, b.center];
        for body in [&mut *a, &mut *b] {
            for i in 0..3 {
                body.center[i] =
                    body.previous_center[i] + (body.center[i] - body.previous_center[i]) * entry;
            }
        }
        if entry > horizontal_entry + 1e-5 {
            // Vertical overlap began later than horizontal overlap. Choose a
            // horizontal contact normal at the actual time of impact.
            let impact_delta = [b.center[0] - a.center[0], b.center[2] - a.center[2]];
            let mut smallest_overlap = f32::INFINITY;
            for axis in axes {
                let overlap = radius_on_axis(a, axis) + radius_on_axis(b, axis)
                    - dot2(impact_delta, axis).abs();
                if overlap < smallest_overlap {
                    smallest_overlap = overlap;
                    entry_normal = if dot2(impact_delta, axis) >= 0.0 {
                        axis
                    } else {
                        [-axis[0], -axis[1]]
                    };
                }
            }
        }
        remainder = Some([
            [
                ends[0][0] - a.center[0],
                ends[0][1] - a.center[1],
                ends[0][2] - a.center[2],
            ],
            [
                ends[1][0] - b.center[0],
                ends[1][1] - b.center[1],
                ends[1][2] - b.center[2],
            ],
        ]);
        (entry_normal, 0.001)
    } else {
        (current_normal, least_penetration + 0.001)
    };
    let total_inv_mass = a.inverse_mass + b.inverse_mass;
    let a_share = a.inverse_mass / total_inv_mass;
    let b_share = b.inverse_mass / total_inv_mass;
    a.center[0] -= normal[0] * correction * a_share;
    a.center[2] -= normal[1] * correction * a_share;
    b.center[0] += normal[0] * correction * b_share;
    b.center[2] += normal[1] * correction * b_share;

    let relative_normal_speed =
        (b.velocity[0] - a.velocity[0]) * normal[0] + (b.velocity[2] - a.velocity[2]) * normal[1];
    if relative_normal_speed < 0.0 {
        const RESTITUTION: f32 = 0.08;
        let impulse = -(1.0 + RESTITUTION) * relative_normal_speed / total_inv_mass;
        a.velocity[0] -= impulse * a.inverse_mass * normal[0];
        a.velocity[2] -= impulse * a.inverse_mass * normal[1];
        b.velocity[0] += impulse * b.inverse_mass * normal[0];
        b.velocity[2] += impulse * b.inverse_mass * normal[1];
    }

    // Finish the frame after the impact. Without this, cars in sustained
    // contact were rewound to the impact point every frame: they froze while
    // touching and their retained velocity reappeared on separation.
    // Approach along the normal is removed by mass share (no restitution);
    // sliding and shared pushing motion are kept.
    if let Some([mut rem_a, mut rem_b]) = remainder {
        let approach = (rem_b[0] - rem_a[0]) * normal[0] + (rem_b[2] - rem_a[2]) * normal[1];
        if approach < 0.0 {
            rem_a[0] += approach * a_share * normal[0];
            rem_a[2] += approach * a_share * normal[1];
            rem_b[0] -= approach * b_share * normal[0];
            rem_b[2] -= approach * b_share * normal[1];
        }
        for i in 0..3 {
            a.center[i] += rem_a[i];
            b.center[i] += rem_b[i];
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(x: f32, z: f32, yaw: f32) -> VehicleContactProxy {
        VehicleContactProxy {
            previous_center: [x, 1.0, z],
            center: [x, 1.0, z],
            velocity: [0.0; 3],
            yaw,
            half_extents: [1.0, 1.0, 2.0],
            inverse_mass: 1.0,
        }
    }

    /// Integrate both cars for one frame, then resolve, as the renderer does.
    fn step_pair(a: &mut VehicleContactProxy, b: &mut VehicleContactProxy, dt: f32) {
        for body in [&mut *a, &mut *b] {
            body.previous_center = body.center;
            for i in 0..3 {
                body.center[i] += body.velocity[i] * dt;
            }
        }
        resolve_vehicle_contact(a, b);
    }

    #[test]
    fn pushing_from_behind_keeps_both_cars_moving() {
        // a (behind, 20 m/s) pushes b (15 m/s); both travel toward -Z.
        let mut a = car(0.0, 4.0, 0.0);
        let mut b = car(0.0, 0.0, 0.0);
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            a.velocity[2] = -20.0;
            b.velocity[2] = -15.0;
            step_pair(&mut a, &mut b, dt);
        }
        // In contact the pair moves at >= 15 m/s; rewinding to impact froze it.
        assert!(b.center[2] < -14.0, "front car frozen: {:?}", b.center);
        assert!(a.center[2] < -10.0, "rear car frozen: {:?}", a.center);
        assert!(a.center[2] > b.center[2], "cars crossed");
    }

    #[test]
    fn side_by_side_scrape_keeps_forward_motion() {
        // Two cars touching side by side, a drifting into b while both drive.
        let mut a = car(-2.0, 0.0, 0.0);
        let mut b = car(0.0, 0.0, 0.0);
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            a.velocity = [1.0, 0.0, -20.0];
            b.velocity = [0.0, 0.0, -20.0];
            step_pair(&mut a, &mut b, dt);
        }
        assert!(a.center[2] < -18.0, "scraping car frozen: {:?}", a.center);
        assert!(b.center[2] < -18.0, "scraped car frozen: {:?}", b.center);
    }

    #[test]
    fn vehicle_head_on_contact_changes_both_velocities() {
        let mut a = car(0.0, 1.5, 0.0);
        let mut b = car(0.0, -1.5, 0.0);
        a.velocity[2] = -10.0;
        b.velocity[2] = 10.0;
        assert!(resolve_vehicle_contact(&mut a, &mut b));
        assert!(a.center[2] > 1.5 && b.center[2] < -1.5);
        assert!(a.velocity[2] > -1.0 && b.velocity[2] < 1.0);
    }

    #[test]
    fn vehicle_side_contact_transfers_lateral_momentum() {
        let mut a = car(-0.8, 0.0, 0.0);
        let mut b = car(0.8, 0.0, 0.0);
        a.velocity[0] = 6.0;
        assert!(resolve_vehicle_contact(&mut a, &mut b));
        assert!(a.velocity[0] < 6.0 && b.velocity[0] > 0.0);
        assert!(a.center[0] < -0.8 && b.center[0] > 0.8);
    }

    #[test]
    fn separated_vehicle_boxes_do_not_collide() {
        let mut a = car(0.0, 0.0, 0.4);
        let mut b = car(12.0, 0.0, -0.2);
        assert!(!resolve_vehicle_contact(&mut a, &mut b));
    }

    #[test]
    fn vertically_separated_vehicle_boxes_do_not_collide() {
        let mut a = car(0.0, 0.0, 0.0);
        let mut b = car(0.0, 0.0, 0.0);
        b.previous_center[1] = 6.0;
        b.center[1] = 6.0;
        assert!(!resolve_vehicle_contact(&mut a, &mut b));
    }

    #[test]
    fn swept_vehicle_boxes_stop_before_tunneling() {
        let mut a = car(10.0, 0.0, 0.0);
        let mut b = car(0.0, 0.0, 0.0);
        a.previous_center[0] = -10.0;
        a.velocity[0] = 200.0;
        assert!(resolve_vehicle_contact(&mut a, &mut b));
        assert!(a.center[0] < b.center[0]);
        // Touching after impact, and the struck car is pushed for the rest of the frame.
        assert!((b.center[0] - a.center[0] - 2.0).abs() < 0.01);
        assert!(b.center[0] > 0.0);
        assert!(a.velocity[0] < 200.0 && b.velocity[0] > 0.0);
    }

    #[test]
    fn swept_contact_uses_entry_side_when_centers_cross_inside_a_frame() {
        let mut a = car(0.0, -1.0, 0.0);
        let mut b = car(0.0, 0.0, 0.0);
        a.previous_center[2] = 5.0;
        a.velocity[2] = -60.0;
        assert!(resolve_vehicle_contact(&mut a, &mut b));
        assert!(a.center[2] > b.center[2], "cars crossed through each other");
        assert!((a.center[2] - b.center[2] - 4.0).abs() < 0.01);
        assert!(b.center[2] < 0.0);
        assert!(a.velocity[2] > -60.0 && b.velocity[2] < 0.0);
    }

    #[test]
    fn test_barrier_detection_and_resolution() {
        let edge = TopologyEdge {
            flags: 0x10, // barrier flag
            p1: [-10.0, 0.0, 0.0],
            p2: [10.0, 0.0, 0.0],
        };

        let config = BarrierCollisionConfig::default();

        // Vehicle heading into barrier from +Z towards -Z
        let mut pos = [0.0, 0.0, 0.8]; // radius is 1.25m, so penetration = 0.45m
        let mut vel = [0.0, 0.0, -10.0];

        let hit = BarrierCollider::detect_contact(pos, std::slice::from_ref(&edge), &config);
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert!((h.penetration - 0.45).abs() < 1e-3);
        assert!((h.normal[2] - 1.0).abs() < 1e-3); // Normal points toward +Z

        // Resolve
        let collided = BarrierCollider::resolve_collision(&mut pos, &mut vel, &[edge], &config);
        assert!(collided);
        assert!(pos[2] >= 1.25 - 1e-4); // Pushed out beyond radius
        assert!(vel[2] > 0.0); // Bounced back with positive velocity
        assert!(vel[2] < 10.0); // Lost energy via restitution
    }

    #[test]
    fn glancing_scrape_keeps_most_tangential_speed() {
        let edge = TopologyEdge {
            flags: 0x10,
            p1: [-100.0, 0.0, 0.0],
            p2: [100.0, 0.0, 0.0],
        };
        let config = BarrierCollisionConfig::default();
        let mut pos = [0.0, 0.0, 1.2];
        let mut vel = [30.0, 0.0, -0.12];
        // 60 contact frames, as when steering into a wall for one second.
        for _ in 0..60 {
            pos[2] = 1.2;
            vel[2] = -0.12; // ~0.7 g of tire side force per frame
            BarrierCollider::resolve_collision(
                &mut pos,
                &mut vel,
                std::slice::from_ref(&edge),
                &config,
            );
        }
        // The old fixed 28%-per-frame scrub left ~0 m/s here.
        assert!(vel[0] > 25.0, "scrape lost too much speed: {vel:?}");
    }

    #[test]
    fn head_on_hit_loses_tangential_speed_by_impulse() {
        let edge = TopologyEdge {
            flags: 0x10,
            p1: [-100.0, 0.0, 0.0],
            p2: [100.0, 0.0, 0.0],
        };
        let config = BarrierCollisionConfig::default();
        let mut pos = [0.0, 0.0, 1.0];
        let mut vel = [10.0, 0.0, -20.0];
        BarrierCollider::resolve_collision(
            &mut pos,
            &mut vel,
            std::slice::from_ref(&edge),
            &config,
        );
        // Friction is capped: it stops sliding along the wall, never reverses it.
        assert!(vel[0] >= 0.0 && vel[0] < 10.0, "{vel:?}");
    }

    #[test]
    fn test_barrier_height_tolerance_separation() {
        let edge = TopologyEdge {
            flags: 0,
            p1: [-10.0, 10.0, 0.0], // elevated bridge at Y = 10
            p2: [10.0, 10.0, 0.0],
        };

        let config = BarrierCollisionConfig::default();

        // Vehicle on lower road at Y = 0
        let pos = [0.0, 0.0, 0.5];
        let hit = BarrierCollider::detect_contact(pos, &[edge], &config);
        assert!(hit.is_none(), "Must not collide with bridge overhead");
    }
}

//! AI opponent controller, starting grid formation, and path pursuit.

use crate::physics::VehicleControls;
use crate::race::course::{CourseProgressTracker, TrackCourse};
use glam::{Mat3, Vec3};
use nfs_formats::AisCar;

/// AI driver behavior and personality parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct AiProfile {
    pub name: String,
    pub skill: f32, // 0.0 (novice) .. 1.0 (ace)
    pub aggression: f32,
    pub max_lateral_g: f32,
    pub reaction_time: f32,
}

impl Default for AiProfile {
    fn default() -> Self {
        Self {
            name: "Racer".into(),
            skill: 0.85,
            aggression: 0.50,
            max_lateral_g: 0.88,
            reaction_time: 0.12,
        }
    }
}

impl AiProfile {
    /// Constructs an AI profile from an EA `.ais` configuration.
    pub fn from_ais(ais: &AisCar) -> Self {
        // Average cornering limits from profile
        let avg_cornering = ais.cornering_profile[0..10].iter().copied().sum::<f32>() / 10.0;
        let skill = (avg_cornering / 60.0).clamp(0.4, 1.0);

        Self {
            name: ais.name.clone(),
            skill,
            aggression: 0.6,
            max_lateral_g: (skill * 0.95).clamp(0.65, 1.10),
            reaction_time: (0.20 - skill * 0.10).clamp(0.05, 0.25),
        }
    }

    pub fn pro() -> Self {
        Self {
            name: "Pro Racer".into(),
            skill: 0.95,
            aggression: 0.75,
            max_lateral_g: 0.98,
            reaction_time: 0.08,
        }
    }

    pub fn veteran() -> Self {
        Self {
            name: "Veteran".into(),
            skill: 0.85,
            aggression: 0.60,
            max_lateral_g: 0.88,
            reaction_time: 0.12,
        }
    }

    pub fn novice() -> Self {
        Self {
            name: "Rookie".into(),
            skill: 0.70,
            aggression: 0.45,
            max_lateral_g: 0.78,
            reaction_time: 0.18,
        }
    }
}

/// Deceleration of impact motion through tyre friction, m/s² (about 0.8 g).
const IMPACT_FRICTION_DECEL: f32 = 7.8;
/// Gentle braking used by the debug speed cap, m/s².
const CAP_BRAKE_DECEL: f32 = 1.5;
/// How long the AI driver lets an impact play out, s.
const IMPACT_RECOVERY_S: f32 = 1.0;
/// Velocity change that counts as an impact, m/s.
const IMPACT_THRESHOLD: f32 = 0.5;
/// Steering authority while recovering from an impact.
const IMPACT_STEER_SCALE: f32 = 0.3;
/// Decay rate of impact yaw, 1/s.
const IMPACT_YAW_DECAY: f32 = 3.0;

/// Drivetrain efficiency applied to the engine's peak power.
const DRIVETRAIN_EFFICIENCY: f32 = 0.85;
/// Fallback top speed (m/s) when the SIM carries none (about 234 km/h).
const DEFAULT_TOP_SPEED_MPS: f32 = 65.0;
/// Lowest speed (m/s) used when converting power to force, so a standing
/// start is grip-limited instead of infinite.
const MIN_POWER_SPEED_MPS: f32 = 5.0;

/// Straight-line performance of an AI car, derived from its SIM file.
///
/// Approximation: AI cars stay kinematic, so instead of the original gearbox
/// and tyre model they get a power-limited, grip-limited acceleration curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiPerformance {
    pub mass_kg: f32,
    /// Peak engine power at the wheels, W.
    pub peak_power_w: f32,
    /// Traction-limited acceleration, m/s^2.
    pub grip_accel: f32,
    pub top_speed_mps: f32,
}

impl AiPerformance {
    /// Peak power (W) of a torque curve in Nm sampled every 500 rpm.
    pub fn peak_power_from_curve(torque_nm: &[f32]) -> f32 {
        torque_nm
            .iter()
            .enumerate()
            .map(|(i, tau)| tau.max(0.0) * (i as f32 * 500.0) * std::f32::consts::TAU / 60.0)
            .fold(0.0, f32::max)
            * DRIVETRAIN_EFFICIENCY
    }

    /// Builds the profile from a parsed SIM. Returns `None` for unusable data.
    pub fn from_sim(sim: &nfs_formats::sim::SimCar) -> Option<Self> {
        let f32_at = |o: usize| f32::from_le_bytes(sim.raw[o..o + 4].try_into().unwrap());
        let peak_power_w = Self::peak_power_from_curve(&sim.torque_curve);
        if !sim.mass_kg.is_finite() || sim.mass_kg <= 0.0 || peak_power_w <= 0.0 {
            return None;
        }
        // Tyre mu lives at 0xf4 (the parser exposes it as `drag_coeff`).
        let mu = f32_at(0xf4);
        let grip_accel = if mu.is_finite() && mu > 0.0 {
            mu * 9.81 * 0.5
        } else {
            8.5
        };
        // Top speed at 0x90 is mph in the file.
        let top_mph = f32_at(0x90);
        let top_speed_mps = if top_mph.is_finite() && top_mph > 10.0 {
            top_mph * 0.447_04
        } else {
            DEFAULT_TOP_SPEED_MPS
        };
        Some(Self {
            mass_kg: sim.mass_kg,
            peak_power_w,
            grip_accel,
            top_speed_mps,
        })
    }

    /// Net acceleration (m/s^2) at `speed` under `throttle` (0..1). Drag is
    /// sized so that full power balances it exactly at the top speed.
    pub fn acceleration(&self, speed: f32, throttle: f32) -> f32 {
        let drive = self
            .grip_accel
            .min(self.peak_power_w / (self.mass_kg * speed.max(MIN_POWER_SPEED_MPS)));
        let drag_k = self.peak_power_w / (self.mass_kg * self.top_speed_mps.powi(3));
        drive * throttle - drag_k * speed * speed
    }
}

/// An active AI vehicle in the race session.
#[derive(Debug, Clone)]
pub struct AiOpponent {
    pub id: usize,
    pub name: String,
    pub car_model: String,
    pub profile: AiProfile,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub forward: [f32; 3],
    pub yaw: f32,
    /// Smoothed road-surface normal under the car (unit, default +Y).
    pub up: [f32; 3],
    pub current_speed: f32,
    pub wheel_distance: f32,
    /// Velocity from a vehicle impact that the path controller must not erase.
    collision_velocity: [f32; 3],
    pub tracker: CourseProgressTracker,
    pub distance_along_course: f32,
    pub laps_completed: u32,
    pub current_controls: VehicleControls,
    pub lookahead_wp_idx: usize,
    /// Debug speed cap in m/s (`Some(0.0)` parks the car on the course).
    pub speed_limit: Option<f32>,
    /// Seconds left in which the driver lets an impact play out (no throttle
    /// or brake, reduced steering). Approximation: AI cars are kinematic.
    pub impact_timer: f32,
    /// Yaw rate (rad/s) from impacts, fading through tyre friction.
    pub impact_yaw_rate: f32,
    /// Car performance; `None` keeps the generic constants.
    pub performance: Option<AiPerformance>,
    /// Body rotation set by an original-model AI car (`original_ai`), used
    /// instead of `yaw`/`up` while it drives.
    pub rotation_override: Option<Mat3>,
}

impl AiOpponent {
    /// Creates an AI opponent on the starting grid.
    pub fn new(
        id: usize,
        name: impl Into<String>,
        car_model: impl Into<String>,
        profile: AiProfile,
        grid_slot: usize,
        course: &TrackCourse,
    ) -> Self {
        let (pos, forward, yaw) = calculate_grid_slot(grid_slot, course);

        Self {
            id,
            name: name.into(),
            car_model: car_model.into(),
            profile,
            position: pos,
            velocity: [0.0; 3],
            forward,
            yaw,
            up: [0.0, 1.0, 0.0],
            current_speed: 0.0,
            wheel_distance: 0.0,
            collision_velocity: [0.0; 3],
            tracker: CourseProgressTracker::new(pos),
            distance_along_course: 0.0,
            laps_completed: 0,
            current_controls: VehicleControls::default(),
            lookahead_wp_idx: 1,
            speed_limit: None,
            impact_timer: 0.0,
            impact_yaw_rate: 0.0,
            performance: None,
            rotation_override: None,
        }
    }

    /// Computes steering and throttle/brake inputs to follow the course.
    pub fn update_controls(&mut self, course: &TrackCourse, dt: f32) -> VehicleControls {
        if course.waypoints.is_empty() {
            return VehicleControls::default();
        }

        let speed = self.current_speed.abs();

        // 1. Dynamic lookahead distance (e.g. 8m at low speed, up to 35m at 200 km/h)
        let lookahead_dist = (speed * 0.65).clamp(8.0, 36.0);

        // 2. Find target waypoint ahead along course
        let (curr_wp_idx, _) = course.find_closest_waypoint(self.position);
        let n = course.waypoints.len();

        let mut target_idx = curr_wp_idx;
        let mut accum_dist = 0.0f32;

        for step in 1..n {
            let next_idx = if course.is_circuit {
                (curr_wp_idx + step) % n
            } else {
                (curr_wp_idx + step).min(n - 1)
            };
            if next_idx == target_idx {
                break;
            }
            let d = distance_sq(
                course.waypoints[target_idx].position,
                course.waypoints[next_idx].position,
            )
            .sqrt();
            accum_dist += d;
            target_idx = next_idx;
            if accum_dist >= lookahead_dist {
                break;
            }
        }
        self.lookahead_wp_idx = target_idx;
        let target_wp = &course.waypoints[target_idx];

        // 3. Pure Pursuit steering calculation
        let dx = target_wp.position[0] - self.position[0];
        let dz = target_wp.position[2] - self.position[2];
        let target_angle = (-dx).atan2(-dz); // Yaw heading in NFS coordinates (0 = -Z)
        let mut angle_diff = target_angle - self.yaw;

        while angle_diff > std::f32::consts::PI {
            angle_diff -= std::f32::consts::TAU;
        }
        while angle_diff < -std::f32::consts::PI {
            angle_diff += std::f32::consts::TAU;
        }

        let steer = (angle_diff * 1.8).clamp(-1.0, 1.0);

        // 4. Target speed from waypoint curvature and AI skill
        let target_speed = target_wp.target_speed * self.profile.skill;

        // 5. Throttle and brake commands
        let (throttle, brake) = if speed < target_speed - 1.0 {
            let p = ((target_speed - speed) / 8.0).clamp(0.3, 1.0);
            (p, 0.0)
        } else if speed > target_speed + 1.5 {
            let b = ((speed - target_speed) / 10.0).clamp(0.2, 1.0);
            (0.0, b)
        } else {
            (0.35, 0.0)
        };

        let controls = VehicleControls {
            throttle,
            brake,
            steer,
            handbrake: false,
            ..Default::default()
        };

        self.current_controls = controls;
        self.distance_along_course = course.distance_along_course(self.position);

        // Checkpoint / lap update
        if let Some(true) = self.tracker.update(self.position, self.forward, course, dt) {
            self.laps_completed += 1;
        }

        controls
    }

    /// Advances the AI vehicle using a simplified kinematic bicycle model.
    pub fn step_kinematics(
        &mut self,
        dt: f32,
        course: &TrackCourse,
        elevation_fn: impl Fn(f32, f32) -> f32,
    ) {
        let mut controls = self.update_controls(course, dt);
        // After an impact the driver lets the car move with it for a moment
        // instead of correcting speed and line at once.
        if self.impact_timer > 0.0 {
            controls.throttle = 0.0;
            controls.brake = 0.0;
            controls.steer *= IMPACT_STEER_SCALE;
            self.impact_timer = (self.impact_timer - dt).max(0.0);
        }
        // Debug cap: at or above the limit the car coasts instead of driving,
        // so impacts can still push a parked or slowed car.
        if let Some(limit) = self.speed_limit {
            if self.current_speed >= limit {
                controls.throttle = 0.0;
                controls.brake = 0.0;
            }
        }

        // Impact motion persists across controller steps and slides out
        // through tyre friction (about 0.8 g). Approximation: AI cars are
        // kinematic, not physics bodies.
        let speed =
            (self.collision_velocity[0].powi(2) + self.collision_velocity[2].powi(2)).sqrt();
        if speed > 0.0 {
            let scale = (speed - IMPACT_FRICTION_DECEL * dt.max(0.0)).max(0.0) / speed;
            self.collision_velocity[0] *= scale;
            self.collision_velocity[2] *= scale;
        }

        // Acceleration and braking
        let accel_rate = 8.5; // m/s^2
        let brake_rate = 18.0;
        let drag = 0.0012 * self.current_speed * self.current_speed;

        if controls.brake > 0.05 {
            self.current_speed = (self.current_speed - brake_rate * controls.brake * dt).max(0.0);
        } else if controls.throttle > 0.05 {
            let (accel, top) = match &self.performance {
                Some(p) => (
                    p.acceleration(self.current_speed, controls.throttle),
                    p.top_speed_mps,
                ),
                None => (accel_rate * controls.throttle - drag, 65.0), // ~234 km/h
            };
            self.current_speed = (self.current_speed + accel * dt).min(top);
        } else {
            self.current_speed = (self.current_speed - (2.5 + drag) * dt).max(0.0);
        }
        // Debug cap: slow down gently when clearly above the limit, but not
        // while an impact plays out (the cap would otherwise act as a brake
        // that drags a pushing car down to the cap speed).
        if let Some(limit) = self.speed_limit.filter(|_| self.impact_timer <= 0.0) {
            if self.current_speed > limit + 3.0 {
                self.current_speed = (self.current_speed - CAP_BRAKE_DECEL * dt).max(limit);
            }
        }

        // Heading update
        let max_steer = 30.0_f32.to_radians();
        let wheel_angle = controls.steer * max_steer;
        let wheelbase = 2.45;
        let angular_vel = (self.current_speed / wheelbase) * wheel_angle.tan();
        // The controller's positive heading error requires increasing yaw.
        self.yaw += (angular_vel + self.impact_yaw_rate) * dt;
        self.impact_yaw_rate *= (-IMPACT_YAW_DECAY * dt.max(0.0)).exp();

        // Position update
        let fwd_x = -self.yaw.sin();
        let fwd_z = -self.yaw.cos();
        self.forward = [fwd_x, 0.0, fwd_z];

        self.velocity = [
            fwd_x * self.current_speed + self.collision_velocity[0],
            0.0,
            fwd_z * self.current_speed + self.collision_velocity[2],
        ];
        self.position[0] += self.velocity[0] * dt;
        self.position[2] += self.velocity[2] * dt;
        self.wheel_distance += (self.velocity[0] * fwd_x + self.velocity[2] * fwd_z) * dt;
        self.position[1] = elevation_fn(self.position[0], self.position[2]);
    }

    /// Smooths the road normal under the car into `up`. `None` (no surface
    /// hit) keeps the previous normal.
    ///
    /// Approximation: AI cars are kinematic; tilt follows the road surface,
    /// not the original vehicle model.
    pub fn update_road_up(&mut self, dt: f32, surface_normal: Option<[f32; 3]>) {
        if let Some(n) = surface_normal {
            self.up = smooth_up(self.up, n, dt, AI_UP_TIME_CONSTANT).to_array();
        }
    }

    /// Body-to-world rotation: heading from `yaw`, up axis along `up`.
    pub fn body_rotation(&self) -> Mat3 {
        if let Some(r) = self.rotation_override {
            return r;
        }
        tilted_basis(self.yaw, Vec3::from(self.up))
    }

    /// Applies the world-space velocity returned by vehicle contact resolution.
    pub fn apply_collision_velocity(&mut self, velocity: [f32; 3]) {
        let change = ((velocity[0] - self.velocity[0]).powi(2)
            + (velocity[2] - self.velocity[2]).powi(2))
        .sqrt();
        if change > IMPACT_THRESHOLD {
            self.impact_timer = IMPACT_RECOVERY_S;
        }
        let forward_speed = velocity[0] * self.forward[0] + velocity[2] * self.forward[2];
        self.current_speed = forward_speed.max(0.0);
        self.collision_velocity = [
            velocity[0] - self.forward[0] * self.current_speed,
            0.0,
            velocity[2] - self.forward[2] * self.current_speed,
        ];
        self.velocity = [velocity[0], 0.0, velocity[2]];
    }

    /// Adds an impact yaw rate (rad/s, about world +Y in scene coordinates).
    pub fn apply_collision_spin(&mut self, yaw_rate: f32) {
        if yaw_rate.is_finite() {
            self.impact_yaw_rate += yaw_rate;
        }
    }
}

/// Time constant (s) of the exponential smoothing of the AI road normal.
pub const AI_UP_TIME_CONSTANT: f32 = 0.1;

/// Exponentially moves unit vector `current` toward `target` (renormalized).
/// Degenerate inputs leave `current` unchanged.
pub fn smooth_up(current: [f32; 3], target: [f32; 3], dt: f32, tau: f32) -> Vec3 {
    let current = Vec3::from(current);
    let target = Vec3::from(target).normalize_or_zero();
    if target == Vec3::ZERO {
        return current;
    }
    let alpha = 1.0 - (-dt.max(0.0) / tau.max(1e-4)).exp();
    let blended = current.lerp(target, alpha).normalize_or_zero();
    if blended == Vec3::ZERO {
        target
    } else {
        blended
    }
}

/// Body-to-world rotation (body: +X right, +Y up, -Z forward) that keeps the
/// heading `yaw` but aligns the up axis with `up`. Flat ground (`up` = +Y)
/// gives `Mat3::from_rotation_y(yaw)`.
pub fn tilted_basis(yaw: f32, up: Vec3) -> Mat3 {
    let up = up.normalize_or_zero();
    if up == Vec3::ZERO || up.y <= 0.05 {
        return Mat3::from_rotation_y(yaw);
    }
    let heading = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    // Lift the heading vertically until it lies in the road plane, so its
    // horizontal direction stays exactly the heading.
    let forward = (heading + Vec3::Y * (-heading.dot(up) / up.y)).normalize_or_zero();
    if forward == Vec3::ZERO {
        return Mat3::from_rotation_y(yaw);
    }
    Mat3::from_cols(forward.cross(up), up, -forward)
}

/// Computes the initial spawn position and forward heading for a grid slot (0..=7).
pub fn calculate_grid_slot(slot: usize, course: &TrackCourse) -> ([f32; 3], [f32; 3], f32) {
    if course.checkpoints.is_empty() {
        return ([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], 0.0);
    }

    let gate0 = &course.checkpoints[0];
    let fwd = gate0.forward;
    let right = gate0.right;

    // Slot spacing: 7.5 meters backward per row, 2 cars per row
    let row = slot / 2;
    let col = slot % 2; // 0 = left, 1 = right

    let back_offset = 6.0 + (row as f32) * 8.5; // meters behind start line
    let side_offset = if col == 0 { -2.8 } else { 2.8 }; // meters left/right of centerline

    let spawn_pos = [
        gate0.center[0] - fwd[0] * back_offset + right[0] * side_offset,
        gate0.center[1],
        gate0.center[2] - fwd[2] * back_offset + right[2] * side_offset,
    ];

    let yaw = (-fwd[0]).atan2(-fwd[2]);

    (spawn_pos, fwd, yaw)
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_formats::SplinePoint;

    #[test]
    fn flat_ground_basis_is_yaw_only() {
        for yaw in [0.0_f32, 0.7, -2.0] {
            let m = tilted_basis(yaw, Vec3::Y);
            assert!(m.abs_diff_eq(Mat3::from_rotation_y(yaw), 1e-6));
        }
    }

    #[test]
    fn slope_tilts_up_and_keeps_heading() {
        let yaw = 0.6_f32;
        let a = 10.0_f32.to_radians();
        // Slope rising along -Z (the yaw = 0 heading): normal leans toward +Z.
        let up = Vec3::new(0.0, a.cos(), a.sin());
        let m = tilted_basis(yaw, up);
        let tilt = m.y_axis.angle_between(Vec3::Y);
        assert!((tilt - a).abs() < 1e-4, "{tilt}");
        assert!(m.y_axis.abs_diff_eq(up, 1e-5));
        let fwd = -m.z_axis;
        let heading = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let h = Vec3::new(fwd.x, 0.0, fwd.z).normalize();
        assert!(h.abs_diff_eq(heading, 1e-5));
        assert!(fwd.dot(up).abs() < 1e-5);
        assert!((m.determinant() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn up_smoothing_converges_and_holds_without_hit() {
        let mut ai = AiOpponent::new(
            0,
            "a",
            "m",
            AiProfile::pro(),
            0,
            &TrackCourse::from_spline_points(
                &(0..12)
                    .map(|i| SplinePoint {
                        x: 0.0,
                        z: i as f32 * 10.0,
                    })
                    .collect::<Vec<_>>(),
                false,
                |_, _| 0.0,
            )
            .unwrap(),
        );
        let n = Vec3::new(0.0, 0.9, 0.3).normalize().to_array();
        ai.update_road_up(0.016, Some(n));
        let first = Vec3::from(ai.up);
        assert!(first.angle_between(Vec3::Y) > 0.0);
        assert!(first.angle_between(Vec3::Y) < Vec3::from(n).angle_between(Vec3::Y) * 0.3);
        let held = ai.up;
        ai.update_road_up(0.016, None);
        assert_eq!(ai.up, held);
        for _ in 0..120 {
            ai.update_road_up(0.016, Some(n));
        }
        assert!(Vec3::from(ai.up).angle_between(Vec3::from(n)) < 1e-3);
    }

    #[test]
    fn capped_car_keeps_a_rear_push() {
        let points: Vec<_> = (0..40)
            .map(|i| SplinePoint {
                x: 0.0,
                z: -(i as f32) * 10.0,
            })
            .collect();
        let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
        let mut ai = AiOpponent::new(0, "Rival", "boxster", AiProfile::default(), 0, &course);
        let dt = 1.0 / 60.0;
        ai.speed_limit = Some(30.0 / 3.6);
        for _ in 0..600 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        // A rear hit at 80 km/h leaves both cars near 55 km/h.
        let push = 55.0 / 3.6;
        ai.apply_collision_velocity([ai.forward[0] * push, 0.0, ai.forward[2] * push]);
        for _ in 0..30 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        // Half a second later it has only coasted (about 2.5 m/s^2).
        assert!(
            ai.current_speed > push - 2.0,
            "cap braked the push: {}",
            ai.current_speed * 3.6
        );
    }

    #[test]
    fn impact_is_not_corrected_at_once() {
        let points: Vec<_> = (0..40)
            .map(|i| SplinePoint {
                x: 0.0,
                z: -(i as f32) * 10.0,
            })
            .collect();
        let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
        let mut ai = AiOpponent::new(0, "Rival", "boxster", AiProfile::default(), 0, &course);
        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        let cruise = ai.current_speed;
        // A hit from behind adds 8 m/s along the heading.
        let v = [
            ai.velocity[0] + ai.forward[0] * 8.0,
            0.0,
            ai.velocity[2] + ai.forward[2] * 8.0,
        ];
        ai.apply_collision_velocity(v);
        ai.apply_collision_spin(1.0);
        let yaw0 = ai.yaw;
        for _ in 0..30 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        // Half a second later most of the push is still there and the car turned.
        assert!(
            ai.current_speed > cruise + 5.0,
            "push corrected away: {} vs {}",
            ai.current_speed,
            cruise
        );
        assert!((ai.yaw - yaw0).abs() > 0.1, "no impact yaw");
    }

    #[test]
    fn speed_limit_brakes_down_and_parks() {
        let points: Vec<_> = (0..40)
            .map(|i| SplinePoint {
                x: 0.0,
                z: i as f32 * 10.0,
            })
            .collect();
        let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
        let mut ai = AiOpponent::new(0, "Rival", "boxster", AiProfile::default(), 0, &course);
        ai.current_speed = 40.0;
        ai.speed_limit = Some(30.0 / 3.6);
        let dt = 1.0 / 60.0;
        ai.step_kinematics(dt, &course, |_, _| 0.0);
        assert!(ai.current_speed > 30.0 / 3.6, "cap must brake, not jump");
        for _ in 0..60 * 12 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        // At the cap the car coasts below it and re-accelerates, overshooting
        // by at most one step's acceleration.
        assert!(ai.current_speed <= 30.0 / 3.6 + 0.2, "{}", ai.current_speed);
        ai.speed_limit = Some(0.0);
        for _ in 0..60 * 10 {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
        }
        assert_eq!(ai.current_speed, 0.0);

        // A parked car keeps a push instead of stopping it at once.
        ai.apply_collision_velocity([ai.forward[0] * 8.0, 0.0, ai.forward[2] * 8.0]);
        ai.step_kinematics(dt, &course, |_, _| 0.0);
        assert!(
            ai.current_speed > 7.5,
            "push braked away: {}",
            ai.current_speed
        );
    }

    #[test]
    fn collision_velocity_survives_next_kinematic_step() {
        let points: Vec<_> = (0..12)
            .map(|i| SplinePoint {
                x: 0.0,
                z: i as f32 * 10.0,
            })
            .collect();
        let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
        let mut ai = AiOpponent::new(0, "Rival", "boxster", AiProfile::default(), 0, &course);
        ai.forward = [0.0, 0.0, -1.0];
        ai.yaw = 0.0;
        ai.apply_collision_velocity([8.0, 0.0, -5.0]);
        assert!((ai.current_speed - 5.0).abs() < 1e-5);
        ai.step_kinematics(1.0 / 60.0, &course, |_, _| 0.0);
        assert!(ai.velocity[0] > 7.0, "side impact was erased by steering");
        // Friction slides the impact out within about a second, not instantly.
        for _ in 0..30 {
            ai.step_kinematics(1.0 / 60.0, &course, |_, _| 0.0);
        }
        assert!(
            ai.velocity[0] > 3.0 && ai.velocity[0] < 7.0,
            "{:?}",
            ai.velocity
        );
    }

    #[test]
    fn test_grid_slot_generation() {
        // Create circle course
        let radius = 100.0f32;
        let mut spline_pts = Vec::new();
        for i in 0..40 {
            let angle = (i as f32 / 40.0) * std::f32::consts::TAU;
            spline_pts.push(SplinePoint {
                x: radius * angle.cos(),
                z: radius * angle.sin(),
            });
        }
        let course = TrackCourse::from_spline_points(&spline_pts, true, |_, _| 0.0).unwrap();

        // Check slot 0 (Pole position) and slot 1 (P2)
        let (pos0, _, _) = calculate_grid_slot(0, &course);
        let (pos1, _, _) = calculate_grid_slot(1, &course);

        // Distance between slot 0 and slot 1 should be around track width (~5.6m)
        let d = distance_sq(pos0, pos1).sqrt();
        assert!(d > 4.0 && d < 7.0, "Lateral distance between row pair: {d}");
    }

    fn straight_course() -> TrackCourse {
        let points: Vec<_> = (0..300)
            .map(|i| SplinePoint {
                x: 0.0,
                z: -(i as f32) * 10.0,
            })
            .collect();
        TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap()
    }

    /// Seconds for a standing start to reach 100 km/h, or `None` within 60 s.
    fn zero_to_100(perf: Option<AiPerformance>) -> Option<f32> {
        let course = straight_course();
        let mut ai = AiOpponent::new(0, "Rival", "car", AiProfile::pro(), 0, &course);
        ai.performance = perf;
        let dt = 1.0 / 60.0;
        for n in 1..=(60 * 60) {
            ai.step_kinematics(dt, &course, |_, _| 0.0);
            if ai.current_speed >= 100.0 / 3.6 {
                return Some(n as f32 * dt);
            }
        }
        None
    }

    fn installed_perf(name: &str) -> Option<AiPerformance> {
        let root = std::env::var("NFS5_GAME_DIR")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../local/game").into());
        let path = std::path::Path::new(&root)
            .join("GameData/Simulation/CarData")
            .join(format!("{name}.sim"));
        let bytes = std::fs::read(path).ok()?;
        AiPerformance::from_sim(&nfs_formats::parse_sim(&bytes).ok()?)
    }

    #[test]
    fn peak_power_uses_torque_times_rpm() {
        // 100 Nm at 3000 rpm is 31.4 kW; 80 Nm at 6000 rpm is 50.3 kW.
        let mut curve = [0.0f32; 21];
        curve[6] = 100.0;
        let p = AiPerformance::peak_power_from_curve(&curve);
        assert!((p - 100.0 * 3000.0 * std::f32::consts::TAU / 60.0 * 0.85).abs() < 1.0);
        curve[12] = 80.0;
        let p = AiPerformance::peak_power_from_curve(&curve);
        assert!((p - 80.0 * 6000.0 * std::f32::consts::TAU / 60.0 * 0.85).abs() < 1.0);
    }

    #[test]
    fn default_acceleration_unchanged_without_performance() {
        let t = zero_to_100(None).unwrap();
        assert!(t < 5.0, "default model 0-100 {t}");
    }

    #[test]
    fn sim_performance_scales_acceleration() {
        let mut checked = false;
        let mut time = |names: &[&str]| {
            names.iter().find_map(|n| {
                let p = installed_perf(n)?;
                let t = zero_to_100(Some(p))?;
                eprintln!("{n}: 0-100 {t:.1} s, perf {p:?}");
                checked = true;
                Some(t)
            })
        };
        if let Some(t) = time(&["356Acoupe13", "356coupe11"]) {
            assert!(t >= 10.0, "356 {t}");
        }
        if let Some(t) = time(&["boxster25"]) {
            assert!((5.0..=9.0).contains(&t), "boxster {t}");
        }
        if let Some(t) = time(&["GT1race", "935race"]) {
            assert!(t < 5.0, "GT {t}");
        }
        if !checked {
            eprintln!("no installed SIM files, skipped");
        }
    }

    #[test]
    fn test_ai_opponent_drives_course() {
        let radius = 120.0f32;
        let mut spline_pts = Vec::new();
        for i in 0..60 {
            let angle = (i as f32 / 60.0) * std::f32::consts::TAU;
            spline_pts.push(SplinePoint {
                x: radius * angle.cos(),
                z: radius * angle.sin(),
            });
        }
        let course = TrackCourse::from_spline_points(&spline_pts, true, |_, _| 0.0).unwrap();

        let mut ai = AiOpponent::new(1, "Rival", "boxster", AiProfile::default(), 0, &course);

        assert_eq!(ai.current_speed, 0.0);

        // Simulate 5 seconds of driving
        for _ in 0..250 {
            ai.step_kinematics(0.02, &course, |_, _| 0.0);
        }

        assert!(
            ai.current_speed > 10.0,
            "AI should accelerate, got {}",
            ai.current_speed
        );
        assert!(
            ai.distance_along_course > 20.0,
            "AI should make progress along course"
        );
    }

    #[test]
    fn ai_converges_to_course_on_either_side() {
        for side in [-1.0, 1.0] {
            let course_x = side * 8.0;
            let points: Vec<_> = (0..25)
                .map(|i| SplinePoint {
                    x: course_x,
                    z: i as f32 * 10.0,
                })
                .collect();
            let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
            let mut ai = AiOpponent::new(1, "Rival", "boxster", AiProfile::default(), 0, &course);
            ai.position = [0.0; 3];
            ai.forward = [0.0, 0.0, -1.0];
            ai.yaw = 0.0;
            ai.current_speed = 8.0;
            for _ in 0..180 {
                ai.step_kinematics(1.0 / 60.0, &course, |_, _| 0.0);
            }
            let lateral_error = (ai.position[0] - course_x).abs();
            assert!(
                lateral_error < 2.0,
                "AI did not converge to course x={course_x}: position={:?}, error={lateral_error}",
                ai.position
            );
            assert!(ai.position[2] < -20.0, "AI must progress while converging");
        }
    }

    #[test]
    fn sprint_end_lookahead_keeps_driving_towards_endpoint() {
        let points: Vec<_> = (0..25)
            .map(|i| SplinePoint {
                x: 0.0,
                z: i as f32 * 10.0,
            })
            .collect();
        let course = TrackCourse::from_spline_points(&points, false, |_, _| 0.0).unwrap();
        let mut ai = AiOpponent::new(1, "Rival", "boxster", AiProfile::default(), 0, &course);
        ai.position = [0.0, 0.0, -232.0];
        ai.forward = [0.0, 0.0, -1.0];
        ai.yaw = 0.0;
        ai.current_speed = 20.0;
        for _ in 0..10 {
            ai.step_kinematics(1.0 / 60.0, &course, |_, _| 0.0);
        }
        assert!(
            ai.position[2] < -235.0,
            "AI must approach the sprint endpoint"
        );
        assert!(
            ai.position[0].abs() < 0.2 && ai.forward[2] < -0.95,
            "AI turned back towards start before finish: position={:?}, heading={:?}",
            ai.position,
            ai.forward
        );
    }
}

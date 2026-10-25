//! Prototype arcade car physics and chase camera for NFS track driving.
//!
//! The handling constants below are deliberately tunable prototype behaviour;
//! they are not a claim about the original game's vehicle physics.

use glam::{Mat4, Vec3};
use nfs_assets::TopologyEdge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveViewMode {
    Chase,
    Bumper,
    Free,
}

#[derive(Debug, Clone)]
pub struct ArcadeCar {
    pub pos: Vec3,
    pub yaw: f32,         // radians, 0 = -Z
    pub pitch: f32,       // road grade tilt (radians)
    pub roll: f32,        // road banking tilt (radians)
    pub speed: f32,       // m/s (1 m/s = 3.6 km/h)
    pub steer_angle: f32, // current wheel turn angle in radians
    pub is_braking: bool,
    pub is_handbraking: bool,
    pub is_reversing: bool,
    pub gear: i32, // -1 (R), 0 (N), 1..5
    pub rpm: f32,  // 0.0 .. 1.0
    pub view_mode: DriveViewMode,
    pub camera_eye: Vec3,
    pub camera_target: Vec3,
    pub paint_color_idx: usize,
    pub spawn_pos: Vec3,
    pub spawn_yaw: f32,
    pedal_command: f32, // filtered -1.0 (brake/reverse) .. +1.0 (gas)
}

const WHEELBASE: f32 = 2.45;
const MAX_FORWARD_SPEED: f32 = 66.6667; // 240 km/h prototype top speed
const MAX_REVERSE_SPEED: f32 = -9.0; // ~32 km/h
const ACCEL_RATE: f32 = 9.5; // m/s^2 (~0-100 in ~4s, responsive and controllable)
const BRAKE_RATE: f32 = 22.0; // m/s^2
const HANDBRAKE_RATE: f32 = 30.0; // m/s^2
const COLLISION_RADIUS: f32 = 1.30; // meters
const COLLISION_HEIGHT_TOLERANCE: f32 = 1.5; // meters from the nearest edge point
const MAX_UPDATE_DT: f32 = 0.1;
const MAX_SIMULATION_STEP: f32 = 1.0 / 120.0;
const AERODYNAMIC_DRAG_COEFF: f32 = 0.001;
const LOW_SPEED_STEER_ANGLE: f32 = 32.0_f32.to_radians();
// Prototype handling budgets, not values recovered from the original game.
// They keep the kinematic bicycle approximation controllable at road speed.
const MAX_LATERAL_ACCEL: f32 = 8.0; // m/s^2, normal grip envelope
const MAX_HANDBRAKE_LATERAL_ACCEL: f32 = 10.0; // m/s^2, capped handbrake slip envelope
const STEER_TURN_IN_RESPONSE: f32 = 7.0; // 1/s
const STEER_RETURN_RESPONSE: f32 = 11.0; // 1/s
const PEDAL_APPLY_RESPONSE: f32 = 5.0; // 1/s
const PEDAL_RELEASE_RESPONSE: f32 = 18.0; // 1/s

impl ArcadeCar {
    pub fn new(spawn_pos: Vec3, spawn_yaw: f32) -> Self {
        let forward = Vec3::new(-spawn_yaw.sin(), 0.0, -spawn_yaw.cos());
        let eye = spawn_pos + Vec3::new(0.0, 2.0, 0.0) - forward * 5.8;
        let target = spawn_pos + Vec3::new(0.0, 0.9, 0.0) + forward * 1.5;

        Self {
            pos: spawn_pos,
            yaw: spawn_yaw,
            pitch: 0.0,
            roll: 0.0,
            speed: 0.0,
            steer_angle: 0.0,
            is_braking: false,
            is_handbraking: false,
            is_reversing: false,
            gear: 1,
            rpm: 0.15,
            view_mode: DriveViewMode::Chase,
            camera_eye: eye,
            camera_target: target,
            paint_color_idx: 0,
            spawn_pos,
            spawn_yaw,
            pedal_command: 0.0,
        }
    }

    pub fn cycle_view_mode(&mut self) -> DriveViewMode {
        // The original cycle has no free camera; Free stays reachable in code
        // for debugging but is not part of the C-key cycle.
        self.view_mode = match self.view_mode {
            DriveViewMode::Chase => DriveViewMode::Bumper,
            DriveViewMode::Bumper | DriveViewMode::Free => DriveViewMode::Chase,
        };
        self.view_mode
    }

    pub fn cycle_paint(&mut self) -> usize {
        self.paint_color_idx = (self.paint_color_idx + 1) % 6;
        self.paint_color_idx
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos()).normalize_or_zero()
    }

    pub fn right(&self) -> Vec3 {
        let fwd = self.forward();
        fwd.cross(Vec3::Y).normalize_or_zero()
    }

    pub fn speed_kmh(&self) -> f32 {
        self.speed * 3.6
    }

    pub fn model_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.pos)
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_x(self.pitch)
            * Mat4::from_rotation_z(self.roll)
    }

    /// Reset car to spawn position and clear velocity
    pub fn reset_to_road(&mut self, _edges: &[TopologyEdge]) {
        self.pos = self.spawn_pos;
        self.yaw = self.spawn_yaw;
        self.pitch = 0.0;
        self.roll = 0.0;
        self.speed = 0.0;
        self.steer_angle = 0.0;
        self.is_braking = false;
        self.is_handbraking = false;
        self.is_reversing = false;
        self.gear = 1;
        self.rpm = 0.15;
        self.pedal_command = 0.0;

        let fwd = self.forward();
        self.camera_eye = self.pos + Vec3::new(0.0, 2.0, 0.0) - fwd * 5.8;
        self.camera_target = self.pos + Vec3::new(0.0, 0.9, 0.0) + fwd * 1.5;
    }

    /// Set vehicle world pose directly and update camera targets immediately.
    pub fn set_pose(&mut self, pos: Vec3, yaw: f32) {
        self.pos = pos;
        self.yaw = yaw;
        self.pitch = 0.0;
        self.roll = 0.0;
        self.speed = 0.0;
        self.steer_angle = 0.0;
        self.is_braking = false;
        self.is_handbraking = false;
        self.is_reversing = false;
        self.gear = 1;
        self.rpm = 0.15;
        self.pedal_command = 0.0;

        let fwd = self.forward();
        self.camera_eye = self.pos + Vec3::new(0.0, 2.0, 0.0) - fwd * 5.8;
        self.camera_target = self.pos + Vec3::new(0.0, 0.9, 0.0) + fwd * 1.5;
    }

    fn max_steer_angle_for_speed(speed: f32) -> f32 {
        let speed_sq = speed.abs().max(0.1).powi(2);
        let grip_limited_angle = (MAX_LATERAL_ACCEL * WHEELBASE / speed_sq).atan();
        LOW_SPEED_STEER_ANGLE.min(grip_limited_angle)
    }

    fn max_yaw_rate_for_lateral_accel(speed: f32, lateral_accel: f32) -> f32 {
        lateral_accel / speed.abs().max(0.05)
    }

    fn update_pedal_command(&mut self, requested: f32, dt: f32) -> f32 {
        let requested = requested.clamp(-1.0, 1.0);
        let braking_against_motion =
            (requested < -0.05 && self.speed > 0.5) || (requested > 0.05 && self.speed < -0.5);

        // Braking against the current direction bypasses smoothing so a gas-to-
        // brake transition never delays stopping. Other releases settle quickly,
        // while new propulsion ramps in over a short, repeatable time constant.
        if braking_against_motion {
            self.pedal_command = requested;
        } else {
            let is_releasing = requested.abs() < self.pedal_command.abs()
                || (requested * self.pedal_command).is_sign_negative();
            let response = if is_releasing {
                PEDAL_RELEASE_RESPONSE
            } else {
                PEDAL_APPLY_RESPONSE
            };
            let blend = 1.0 - (-response * dt).exp();
            self.pedal_command += (requested - self.pedal_command) * blend;
        }

        self.pedal_command
    }

    /// Advance the prototype arcade vehicle physics by `dt` seconds with inputs.
    pub fn update(
        &mut self,
        dt: f32,
        throttle: f32, // -1.0 (brake/reverse) .. +1.0 (gas)
        steer: f32,    // -1.0 (left) .. +1.0 (right)
        handbrake: bool,
        edges: &[TopologyEdge],
        sample_elevation: impl Fn(f32, f32) -> Option<f32>,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        let throttle = if throttle.is_finite() { throttle } else { 0.0 };
        let steer = if steer.is_finite() { steer } else { 0.0 };
        let total_dt = dt.min(MAX_UPDATE_DT);
        let substeps = (total_dt / MAX_SIMULATION_STEP).ceil() as u32;
        let step_dt = total_dt / substeps as f32;
        for _ in 0..substeps {
            self.update_step(
                step_dt,
                throttle,
                steer,
                handbrake,
                edges,
                &sample_elevation,
            );
        }
    }

    fn update_step(
        &mut self,
        dt: f32,
        throttle: f32,
        steer: f32,
        handbrake: bool,
        edges: &[TopologyEdge],
        sample_elevation: &impl Fn(f32, f32) -> Option<f32>,
    ) {
        // 1. Steering dynamics.  The target angle narrows with speed, while
        // exponential response keeps turn-in, centering, and reversal smooth
        // across varying render frame times.
        let max_steer = Self::max_steer_angle_for_speed(self.speed);
        let target_steer = steer.clamp(-1.0, 1.0) * max_steer;
        let is_returning_or_reversing = target_steer.abs() < 1e-4
            || (self.steer_angle.abs() >= 1e-4
                && (self.steer_angle * target_steer).is_sign_negative());
        let steer_response = if is_returning_or_reversing {
            STEER_RETURN_RESPONSE
        } else {
            STEER_TURN_IN_RESPONSE
        };
        let steer_blend = 1.0 - (-steer_response * dt).exp();
        self.steer_angle += (target_steer - self.steer_angle) * steer_blend;

        // 2. Throttle, braking, and reverse. Pedal smoothing avoids an abrupt
        // engine step, but braking against travel always retains priority.
        let throttle = self.update_pedal_command(throttle, dt);
        self.is_handbraking = handbrake;
        self.is_braking = false;
        self.is_reversing = false;

        if handbrake {
            // Handbrake: rapid deceleration
            let decel = HANDBRAKE_RATE * dt;
            if self.speed > 0.0 {
                self.speed = (self.speed - decel).max(0.0);
            } else {
                self.speed = (self.speed + decel).min(0.0);
            }
        } else if throttle > 0.05 {
            if self.speed < -0.5 {
                // Moving backward: gas acts as brake
                self.speed += BRAKE_RATE * dt;
                self.is_braking = true;
            } else {
                // Forward acceleration with aerodynamic drag and power tapering
                let speed_ratio = (self.speed / MAX_FORWARD_SPEED).clamp(0.0, 1.0);
                let power_taper = 1.0 - speed_ratio * 0.40;
                let drag = AERODYNAMIC_DRAG_COEFF * self.speed * self.speed;
                let accel = (ACCEL_RATE * throttle * power_taper - drag).max(0.0);
                self.speed = (self.speed + accel * dt).min(MAX_FORWARD_SPEED);
            }
        } else if throttle < -0.05 {
            if self.speed > 0.5 {
                // Moving forward: brake
                self.speed = (self.speed - BRAKE_RATE * (-throttle) * dt).max(0.0);
                self.is_braking = true;
            } else {
                // Reverse gear
                self.is_reversing = true;
                let accel = ACCEL_RATE * 0.55 * (-throttle);
                self.speed = (self.speed - accel * dt).max(MAX_REVERSE_SPEED);
            }
        } else {
            // Coasting: natural drag & rolling friction
            let friction = 3.2 * dt;
            let drag = AERODYNAMIC_DRAG_COEFF * self.speed * self.speed * dt;
            if self.speed > 0.0 {
                self.speed = (self.speed - friction - drag).max(0.0);
            } else {
                self.speed = (self.speed + friction + drag).min(0.0);
            }
        }

        // 3. Angular velocity / heading update
        if self.speed.abs() > 0.05 {
            let mut angular_vel = (self.speed / WHEELBASE) * self.steer_angle.tan();
            if handbrake && self.speed.abs() > 3.0 {
                // The drift response remains bounded by its own prototype
                // lateral-acceleration budget.
                angular_vel *= 1.85;
            }
            let yaw_limit = Self::max_yaw_rate_for_lateral_accel(
                self.speed,
                if handbrake {
                    MAX_HANDBRAKE_LATERAL_ACCEL
                } else {
                    MAX_LATERAL_ACCEL
                },
            );
            angular_vel = angular_vel.clamp(-yaw_limit, yaw_limit);
            // Invert sign: turning right (steer > 0) rotates yaw negatively towards +X
            self.yaw -= angular_vel * dt;
        }

        // 4. Update horizontal position
        let fwd = self.forward();
        let delta_pos = fwd * self.speed * dt;
        self.pos.x += delta_pos.x;
        self.pos.z += delta_pos.z;

        // 5. Road boundary collision & bounce
        self.handle_edge_collisions(edges);

        // 6. Surface elevation, road grade pitch & roll
        if let Some(y_center) = sample_elevation(self.pos.x, self.pos.z) {
            self.pos.y = y_center;

            let fwd_h = self.forward();
            let right_h = self.right();

            let front = self.pos + fwd_h * 1.3;
            let rear = self.pos - fwd_h * 1.3;
            let left = self.pos - right_h * 0.8;
            let right = self.pos + right_h * 0.8;

            let y_front = sample_elevation(front.x, front.z).unwrap_or(y_center);
            let y_rear = sample_elevation(rear.x, rear.z).unwrap_or(y_center);
            let y_left = sample_elevation(left.x, left.z).unwrap_or(y_center);
            let y_right = sample_elevation(right.x, right.z).unwrap_or(y_center);

            let target_pitch = ((y_front - y_rear) / 2.6).atan();
            let target_roll = -((y_right - y_left) / 1.6).atan();

            self.pitch += (target_pitch - self.pitch) * (18.0 * dt).min(1.0);
            self.roll += (target_roll - self.roll) * (18.0 * dt).min(1.0);
        }

        // 7. Calculate gear and RPM for speedometer
        self.update_gear_and_rpm();

        // 8. Update chase camera
        self.update_camera(dt);
    }

    fn handle_edge_collisions(&mut self, edges: &[TopologyEdge]) {
        if edges.is_empty() {
            return;
        }

        let px = self.pos.x;
        let pz = self.pos.z;

        for edge in edges {
            let x1 = edge.p1[0];
            let z1 = edge.p1[2];
            let x2 = edge.p2[0];
            let z2 = edge.p2[2];

            let dx = x2 - x1;
            let dz = z2 - z1;
            let seg_len_sq = dx * dx + dz * dz;
            if seg_len_sq < 1e-4 {
                continue;
            }

            let t = (((px - x1) * dx + (pz - z1) * dz) / seg_len_sq).clamp(0.0, 1.0);
            let closest_x = x1 + t * dx;
            let closest_z = z1 + t * dz;
            let closest_y = edge.p1[1] + t * (edge.p2[1] - edge.p1[1]);

            // Edges may overlap in plan view at bridges or tunnels. Only the
            // nearest point on the same vertical layer can block this car.
            if (self.pos.y - closest_y).abs() > COLLISION_HEIGHT_TOLERANCE {
                continue;
            }

            let dist_x = px - closest_x;
            let dist_z = pz - closest_z;
            let dist_sq = dist_x * dist_x + dist_z * dist_z;

            if dist_sq < COLLISION_RADIUS * COLLISION_RADIUS && dist_sq > 1e-6 {
                let dist = dist_sq.sqrt();
                let penetration = COLLISION_RADIUS - dist;
                let nx = dist_x / dist;
                let nz = dist_z / dist;

                // Push car away from boundary
                self.pos.x += nx * penetration;
                self.pos.z += nz * penetration;

                // Bounce / scrub speed
                self.speed *= 0.84;

                // Slight yaw deflection
                let fwd = self.forward();
                let dot = fwd.x * nx + fwd.z * nz;
                if dot < 0.0 {
                    self.yaw += dot * 0.15;
                }
            }
        }
    }

    fn update_gear_and_rpm(&mut self) {
        let kmh = self.speed.abs() * 3.6;
        if self.speed < -0.2 {
            self.gear = -1; // Reverse
            self.rpm = (kmh / 45.0).clamp(0.2, 0.95);
        } else if kmh < 0.5 {
            self.gear = 1;
            self.rpm = 0.15;
        } else if kmh < 45.0 {
            self.gear = 1;
            self.rpm = 0.20 + (kmh / 45.0) * 0.75;
        } else if kmh < 82.0 {
            self.gear = 2;
            self.rpm = 0.28 + ((kmh - 45.0) / 37.0) * 0.68;
        } else if kmh < 124.0 {
            self.gear = 3;
            self.rpm = 0.32 + ((kmh - 82.0) / 42.0) * 0.65;
        } else if kmh < 165.0 {
            self.gear = 4;
            self.rpm = 0.35 + ((kmh - 124.0) / 41.0) * 0.62;
        } else {
            self.gear = 5;
            self.rpm = 0.40 + ((kmh - 165.0) / 50.0).clamp(0.0, 1.0) * 0.58;
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let fwd = self.forward();
        match self.view_mode {
            DriveViewMode::Chase => {
                let ideal_target = self.pos + Vec3::new(0.0, 0.95, 0.0) + fwd * 1.8;
                let ideal_eye = self.pos + Vec3::new(0.0, 2.05, 0.0) - fwd * 5.9;

                // Smooth exponential spring damping for camera tracking
                let eye_rate = (8.5 * dt).min(1.0);
                let target_rate = (14.0 * dt).min(1.0);

                self.camera_eye += (ideal_eye - self.camera_eye) * eye_rate;
                self.camera_target += (ideal_target - self.camera_target) * target_rate;
            }
            DriveViewMode::Bumper => {
                self.camera_eye = self.pos + Vec3::new(0.0, 0.75, 0.0) + fwd * 1.6;
                self.camera_target = self.camera_eye + fwd * 12.0;
            }
            DriveViewMode::Free => {
                // Free camera doesn't lock to car orientation, orbits center
                self.camera_target = self.pos + Vec3::new(0.0, 0.9, 0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulate_drive_path(dt: f32) -> ArcadeCar {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        for _ in 0..(3.0 / dt).round() as usize {
            car.update(dt, 1.0, 0.45, false, &[], |_, _| Some(0.0));
        }
        car
    }

    fn steady_turn_peaks(speed: f32, handbrake: bool) -> (f32, f32) {
        let dt = 1.0 / 120.0;
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        let mut peak_yaw_rate: f32 = 0.0;
        let mut peak_lateral_accel: f32 = 0.0;

        for _ in 0..360 {
            car.speed = speed;
            let previous_yaw = car.yaw;
            car.update(dt, 0.0, 1.0, handbrake, &[], |_, _| Some(0.0));
            let yaw_rate = ((car.yaw - previous_yaw) / dt).abs();
            peak_yaw_rate = peak_yaw_rate.max(yaw_rate);
            peak_lateral_accel = peak_lateral_accel.max(car.speed.abs() * yaw_rate);
        }

        (peak_yaw_rate, peak_lateral_accel)
    }

    fn edge_across_road(z: f32, y: f32) -> TopologyEdge {
        TopologyEdge {
            flags: 0,
            p1: [-2.0, y, z],
            p2: [2.0, y, z],
        }
    }

    #[test]
    fn test_car_acceleration_and_gears() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        assert_eq!(car.speed, 0.0);
        assert_eq!(car.gear, 1);

        // Accelerate for 2 seconds
        for _ in 0..100 {
            car.update(0.02, 1.0, 0.0, false, &[], |_, _| Some(0.0));
        }

        assert!(car.speed > 10.0, "Car should accelerate forward");
        assert!(car.speed_kmh() > 36.0);
        assert!(car.rpm > 0.0 && car.rpm <= 1.0);
        assert!(car.gear >= 1);
    }

    #[test]
    fn test_full_throttle_reaches_prototype_top_speed() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        for _ in 0..(30 * 60) {
            car.update(1.0 / 60.0, 1.0, 0.0, false, &[], |_, _| Some(0.0));
        }

        assert!(car.speed > 65.0, "full throttle should reach near 240 km/h");
        assert!(car.speed <= MAX_FORWARD_SPEED);
    }

    #[test]
    fn test_high_speed_substeps_hit_a_thin_same_level_edge() {
        let mut car = ArcadeCar::new(Vec3::new(0.25, 0.0, 0.0), 0.0);
        car.speed = MAX_FORWARD_SPEED;
        car.pedal_command = 1.0;
        let edge = edge_across_road(-3.0, 0.0);

        car.update(0.1, 1.0, 0.0, false, &[edge], |_, _| Some(0.0));

        assert!(
            car.speed < MAX_FORWARD_SPEED * 0.9,
            "1/120 s substeps must not tunnel through a thin same-level edge"
        );
    }

    #[test]
    fn test_vertically_separated_edge_does_not_collide() {
        let mut car = ArcadeCar::new(Vec3::new(0.25, 0.0, 0.0), 0.0);
        car.speed = MAX_FORWARD_SPEED;
        car.pedal_command = 1.0;
        let upper_edge = edge_across_road(-3.0, 6.0);

        car.update(0.1, 1.0, 0.0, false, &[upper_edge], |_, _| Some(0.0));

        assert_eq!(car.speed, MAX_FORWARD_SPEED);
        assert!(car.pos.z < -6.0, "car should pass beneath the upper edge");
    }

    #[test]
    fn test_zero_and_nonfinite_dt_do_not_change_car_state() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        let original = car.clone();
        for dt in [0.0, -0.1, f32::NAN, f32::INFINITY] {
            car.update(dt, 1.0, 1.0, false, &[], |_, _| Some(0.0));
        }

        assert_eq!(car.pos, original.pos);
        assert_eq!(car.speed, original.speed);
        assert_eq!(car.steer_angle, original.steer_angle);
        assert_eq!(car.pedal_command, original.pedal_command);
    }

    #[test]
    fn test_car_braking_and_reverse() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);

        // Reverse from standstill
        for _ in 0..50 {
            car.update(0.02, -1.0, 0.0, false, &[], |_, _| Some(0.0));
        }

        assert!(car.speed < 0.0, "Car should move backwards");
        assert_eq!(car.gear, -1, "Gear should be reverse (R)");
        assert!(car.is_reversing);
    }

    #[test]
    fn test_car_handbrake() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        car.speed = 30.0; // 108 km/h

        for _ in 0..20 {
            car.update(0.02, 0.0, 0.0, true, &[], |_, _| Some(0.0));
        }

        assert!(car.speed < 20.0, "Handbrake should decelerate quickly");
    }

    #[test]
    fn test_car_view_and_paint_cycling() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        assert_eq!(car.view_mode, DriveViewMode::Chase);

        assert_eq!(car.cycle_view_mode(), DriveViewMode::Bumper);
        assert_eq!(car.cycle_view_mode(), DriveViewMode::Chase);

        assert_eq!(car.paint_color_idx, 0);
        assert_eq!(car.cycle_paint(), 1);
        assert_eq!(car.cycle_paint(), 2);
    }

    #[test]
    fn test_car_reset() {
        let mut car = ArcadeCar::new(Vec3::new(10.0, 5.0, 10.0), 1.57);
        car.speed = 25.0;
        car.pos = Vec3::new(50.0, 0.0, 50.0);

        car.reset_to_road(&[]);
        assert_eq!(car.speed, 0.0);
        assert_eq!(car.pos, Vec3::new(10.0, 5.0, 10.0));
        assert_eq!(car.yaw, 1.57);
    }

    #[test]
    fn test_car_steering_direction() {
        let mut car_right = ArcadeCar::new(Vec3::ZERO, 0.0);
        car_right.speed = 10.0;
        for _ in 0..20 {
            car_right.update(0.02, 0.0, 1.0, false, &[], |_, _| Some(0.0));
        }
        assert!(
            car_right.forward().x > 0.0,
            "Steering right must orient car forward vector towards +X (right)"
        );

        let mut car_left = ArcadeCar::new(Vec3::ZERO, 0.0);
        car_left.speed = 10.0;
        for _ in 0..20 {
            car_left.update(0.02, 0.0, -1.0, false, &[], |_, _| Some(0.0));
        }
        assert!(
            car_left.forward().x < 0.0,
            "Steering left must orient car forward vector towards -X (left)"
        );
    }

    #[test]
    fn steering_direction_forward_reverse_at_rotated_headings() {
        for yaw in [0.0, 0.7, -1.8, std::f32::consts::PI] {
            for speed in [-8.0_f32, 10.0] {
                for steer in [-1.0_f32, 1.0] {
                    let mut car = ArcadeCar::new(Vec3::ZERO, yaw);
                    let initial_right = car.right();
                    car.speed = speed;
                    for _ in 0..20 {
                        car.update(0.02, 0.0, steer, false, &[], |_, _| Some(0.0));
                    }
                    let heading_side = car.forward().dot(initial_right);
                    assert!(
                        heading_side * steer * speed.signum() > 0.01,
                        "yaw={yaw} speed={speed} steer={steer}: {heading_side}"
                    );
                    assert!(
                        car.pos.dot(initial_right) * steer > 0.01,
                        "rearward steering reverses heading change, not path curvature"
                    );
                }
            }
        }
    }

    #[test]
    fn test_pedal_smooths_gas_but_braking_against_motion_has_priority() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        car.update(0.02, 1.0, 0.0, false, &[], |_, _| Some(0.0));
        assert!(car.pedal_command > 0.0 && car.pedal_command < 0.1);

        car.speed = 25.0;
        car.pedal_command = 1.0;
        car.update(0.02, -1.0, 0.0, false, &[], |_, _| Some(0.0));
        assert_eq!(car.pedal_command, -1.0);
        assert!(car.is_braking);
        assert!(car.speed <= 25.0 - BRAKE_RATE * 0.02);
    }

    #[test]
    fn test_steering_turn_in_is_gradual_and_speed_sensitive() {
        let mut slow_car = ArcadeCar::new(Vec3::ZERO, 0.0);
        let mut fast_car = ArcadeCar::new(Vec3::ZERO, 0.0);
        fast_car.speed = MAX_FORWARD_SPEED;

        slow_car.update(0.02, 0.0, 1.0, false, &[], |_, _| Some(0.0));
        fast_car.update(0.02, 0.0, 1.0, false, &[], |_, _| Some(0.0));

        assert!(slow_car.steer_angle > 0.0);
        assert!(
            slow_car.steer_angle < LOW_SPEED_STEER_ANGLE * 0.2,
            "a single input frame must not snap to full lock"
        );
        assert!(
            slow_car.steer_angle > fast_car.steer_angle * 2.0,
            "high speed must substantially reduce the available steering lock"
        );
    }

    #[test]
    fn test_steering_returns_and_reverses_without_a_sign_jump() {
        let mut car = ArcadeCar::new(Vec3::ZERO, 0.0);
        for _ in 0..20 {
            car.update(0.02, 0.0, 1.0, false, &[], |_, _| Some(0.0));
        }
        let held_angle = car.steer_angle;

        car.update(0.02, 0.0, 0.0, false, &[], |_, _| Some(0.0));
        assert!(car.steer_angle > 0.0 && car.steer_angle < held_angle);

        car.update(0.02, 0.0, -1.0, false, &[], |_, _| Some(0.0));
        assert!(
            car.steer_angle > 0.0,
            "one reverse-input frame should first unwind the existing lock"
        );

        for _ in 0..30 {
            car.update(0.02, 0.0, -1.0, false, &[], |_, _| Some(0.0));
        }
        assert!(
            car.steer_angle < 0.0,
            "reversed input must eventually take effect"
        );
    }

    #[test]
    fn test_driving_path_is_close_across_fixed_timesteps() {
        let fine = simulate_drive_path(0.01);
        let coarse = simulate_drive_path(0.02);

        assert!(
            fine.pos.distance(coarse.pos) < 0.5,
            "fixed input should keep the 10 ms and 20 ms paths close"
        );
        assert!(
            (fine.yaw - coarse.yaw).abs() < 0.04,
            "fixed input should keep the headings close"
        );
        assert!((fine.speed - coarse.speed).abs() < 0.1);
    }

    #[test]
    fn test_prototype_driving_report_is_stable_across_30_60_120_hz() {
        let reports = [
            (1.0 / 30.0, simulate_drive_path(1.0 / 30.0)),
            (1.0 / 60.0, simulate_drive_path(1.0 / 60.0)),
            (1.0 / 120.0, simulate_drive_path(1.0 / 120.0)),
        ];
        let reference_pos = reports[2].1.pos;
        let reference_yaw = reports[2].1.yaw;
        let reference_speed = reports[2].1.speed;

        for (dt, car) in reports {
            let path_error = car.pos.distance(reference_pos);
            let yaw_error = (car.yaw - reference_yaw).abs();
            let speed_error = (car.speed - reference_speed).abs();
            println!(
                "dt={dt:.5}s pos=({:.3}, {:.3}) yaw={:.4} speed={:.3} \\
                 path_error={path_error:.3} yaw_error={yaw_error:.4} speed_error={speed_error:.3}",
                car.pos.x, car.pos.z, car.yaw, car.speed,
            );
            assert!(path_error < 1.5, "30/60 Hz path must stay close to 120 Hz");
            assert!(yaw_error < 0.10, "30/60 Hz yaw must stay close to 120 Hz");
            assert!(
                speed_error < 0.25,
                "30/60 Hz speed must stay close to 120 Hz"
            );
        }
    }

    #[test]
    fn test_turning_report_respects_lateral_acceleration_budgets() {
        for speed in [10.0, 25.0, 38.0] {
            let (yaw_rate, lateral_accel) = steady_turn_peaks(speed, false);
            let (handbrake_yaw_rate, handbrake_lateral_accel) = steady_turn_peaks(speed, true);
            println!(
                "speed={speed:.0}m/s normal: peak_yaw={yaw_rate:.3}rad/s lateral={lateral_accel:.3}m/s^2; \\
                 handbrake: peak_yaw={handbrake_yaw_rate:.3}rad/s lateral={handbrake_lateral_accel:.3}m/s^2"
            );
            assert!(lateral_accel <= MAX_LATERAL_ACCEL + 0.05);
            assert!(handbrake_lateral_accel <= MAX_HANDBRAKE_LATERAL_ACCEL + 0.05);
        }
    }
}

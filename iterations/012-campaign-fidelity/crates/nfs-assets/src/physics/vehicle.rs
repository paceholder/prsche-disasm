//! Full 6 DOF Vehicle Simulation Engine.
//!
//! Orchestrates `RigidBody`, `Powertrain`, `SuspensionSystem`, and 4-wheel tire friction
//! models with aerodynamic drag and ground surface collision.

use glam::{Mat4, Quat, Vec3};
use nfs_formats::SimCar;

use super::chassis::{ChassisBounds, resolve_chassis_ground};
use super::powertrain::Powertrain;
use super::rigid_body::RigidBody;
use super::suspension::{SuspensionSystem, WHEEL_FL, WHEEL_FR, WHEEL_RL, WHEEL_RR};
use crate::surface::RoadSurface;

const AIR_DENSITY: f32 = 1.225; // kg / m^3
const MAX_SUB_STEP_DT: f32 = 1.0 / 240.0; // 240 Hz max sub-step
const GRAVITY: f32 = 9.80665;
/// Centre-of-mass height above the road at rest (A107). Not a `.sim` value:
/// `SimCar::cg_offset_m` holds `.sim` words 0x120/0x124/0x128, which the original
/// loader (`original_vehicle/sim.rs`, `load_scaling`) treats as ride height,
/// suspension travel and steering toe.
pub const PROTOTYPE_COM_HEIGHT_M: f32 = 0.45;
/// Time after an automatic upshift before the next one (approximation A80).
pub const UPSHIFT_HOLD_S: f32 = 0.5;

/// Driver input controls for vehicle simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleControls {
    /// Throttle pedal input [0.0, 1.0].
    pub throttle: f32,
    /// Brake pedal input [0.0, 1.0].
    pub brake: f32,
    /// Steering command [-1.0, 1.0], positive is right steer.
    pub steer: f32,
    /// Emergency / handbrake engaged.
    pub handbrake: bool,
    /// Manual gear selection request:
    /// - `Some(0)`: Neutral
    /// - `Some(-1)`: Reverse
    /// - `Some(1..=8)`: Forward gears
    /// - `None`: Keep automatic shifting
    pub manual_gear: Option<i32>,
    /// Enable automatic gear transmission logic.
    pub auto_gear: bool,
}

impl Default for VehicleControls {
    fn default() -> Self {
        Self {
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            handbrake: false,
            manual_gear: None,
            auto_gear: true,
        }
    }
}

/// Instantaneous simulation state and telemetry output.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleTelemetry {
    /// Vehicle forward ground speed (m/s).
    pub speed_mps: f32,
    /// Vehicle forward speed (km/h).
    pub speed_kmh: f32,
    /// Engine rotational speed in RPM.
    pub engine_rpm: f32,
    /// Current transmission gear (-1: R, 0: N, 1..=6: Forward).
    pub current_gear: i32,
    /// Throttle input applied.
    pub throttle: f32,
    /// Brake input applied.
    pub brake: f32,
    /// Steering angle of front road wheels (radians).
    pub steer_angle_rad: f32,
    /// Lateral acceleration in units of g (9.81 m/s^2).
    pub lateral_accel_g: f32,
    /// Longitudinal acceleration in units of g.
    pub longitudinal_accel_g: f32,
    /// Yaw rate in rad/s.
    pub yaw_rate_rad_s: f32,
    /// Angular rotation speed of each wheel (rad/s) [FL, FR, RL, RR].
    pub wheel_omegas: [f32; 4],
    /// Suspension compression (m) [FL, FR, RL, RR].
    pub wheel_compressions: [f32; 4],
    /// Longitudinal slip ratio of tires [FL, FR, RL, RR].
    pub wheel_slip_ratios: [f32; 4],
    /// Lateral slip angle of tires in radians [FL, FR, RL, RR].
    pub wheel_slip_angles: [f32; 4],
    /// Ground contact flag for each wheel [FL, FR, RL, RR].
    pub wheels_in_contact: [bool; 4],
    /// 4x4 chassis transformation matrix in world coordinates.
    pub chassis_transform: Mat4,
    /// 4x4 transformation matrices for each of the 4 wheels [FL, FR, RL, RR].
    pub wheel_transforms: [Mat4; 4],
}

/// Complete vehicle dynamics simulation instance.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleSimulation {
    /// 6 DOF rigid chassis body.
    pub body: RigidBody,
    /// Reconstructed collision envelope in body coordinates.
    pub chassis_bounds: ChassisBounds,
    /// Engine, clutch, and transmission.
    pub powertrain: Powertrain,
    /// 4-wheel independent suspension system.
    pub suspension: SuspensionSystem,
    /// Front/rear brake force distribution factor (e.g. 0.60 = 60% front).
    pub brake_bias: f32,
    /// Total peak braking torque (N*m).
    pub max_brake_torque: f32,
    /// Aerodynamic drag coefficient (Cd).
    pub drag_coeff: f32,
    /// Frontal cross-section area in m^2 (typically 1.8 - 2.1 m^2).
    pub frontal_area: f32,
    /// Aerodynamic downforce factor (Cl).
    pub downforce_coeff: f32,
    /// Maximum front wheel steer angle (radians, ~32 degrees = 0.558 rad).
    pub max_steer_angle: f32,
    /// Current smoothed steer angle (radians).
    pub current_steer_angle: f32,
    /// Steering response smoothing rate (rad/s).
    pub steer_speed: f32,
    /// Visual accumulated rotation angle of each wheel for rendering [FL, FR, RL, RR].
    pub wheel_visual_angles: [f32; 4],
    /// Previous linear velocity (used to compute instantaneous g-forces).
    pub prev_linear_velocity: Vec3,
    /// Filtered lateral acceleration (m/s^2).
    pub lateral_accel: f32,
    /// Filtered longitudinal acceleration (m/s^2).
    pub longitudinal_accel: f32,
    /// Stored vehicle specification.
    pub spec_name: String,
    /// Seconds left before the automatic box may upshift again
    /// (approximation A80).
    pub shift_hold: f32,
}

impl VehicleSimulation {
    /// Override reconstructed chassis contacts with selected model body bounds.
    /// Bounds must already be expressed in the simulation body frame.
    pub fn apply_model_chassis_bounds(&mut self, bounds: [[f32; 3]; 2]) -> Result<(), String> {
        self.chassis_bounds = ChassisBounds::from_model(bounds)?;
        Ok(())
    }

    /// Use selected model wheel pivots for axle geometry. The existing vertical
    /// suspension reference remains an adapter, not a recovered original solver.
    pub fn apply_model_wheels(&mut self, wheels: &[crate::CarWheel]) -> Result<(), String> {
        if wheels.len() != 4
            || wheels.iter().enumerate().any(|(i, w)| {
                w.wheel_index != i
                    || !w.center.iter().all(|v| v.is_finite())
                    || !(0.1..0.8).contains(&w.radius)
            })
        {
            return Err("Car model needs four valid ordered wheel pivots".into());
        }
        for (wheel, source) in self.suspension.wheels.iter_mut().zip(wheels) {
            wheel.hardpoint_body.x = source.center[0];
            wheel.hardpoint_body.z = source.center[2];
            wheel.tire.radius = source.radius;
        }
        Ok(())
    }

    /// Construct a vehicle simulation from reverse-engineered NFS 5 `SimCar` specs.
    pub fn from_sim(sim: &SimCar) -> Self {
        let mass = sim.mass_kg.clamp(400.0, 3000.0);
        let width = (sim.front_track_m.max(sim.rear_track_m) + 0.35).clamp(1.4, 2.3);
        let length = (sim.wheelbase_m + 1.2).clamp(3.0, 5.5);
        let height = 1.30;

        let mut body = RigidBody::new(mass, width, height, length, Vec3::ZERO);
        // Start resting upright slightly above ground
        body.position = Vec3::new(0.0, 0.40, 0.0);

        let powertrain = Powertrain::from_sim(sim);
        let suspension = SuspensionSystem::from_sim(sim);

        let drag_coeff = if sim.drag_coeff > 0.1 && sim.drag_coeff < 1.0 {
            sim.drag_coeff
        } else {
            0.32
        };

        let brake_bias = if sim.brake_bias > 0.2 && sim.brake_bias < 0.9 {
            sim.brake_bias
        } else {
            0.62
        };

        let mut vehicle = Self {
            body,
            chassis_bounds: ChassisBounds::reconstructed(width, length),
            powertrain,
            suspension,
            brake_bias,
            max_brake_torque: mass * 12.0, // Enough to generate ~1.2g deceleration
            drag_coeff,
            frontal_area: 1.95,
            downforce_coeff: 0.15,
            max_steer_angle: 32.0f32.to_radians(),
            current_steer_angle: 0.0,
            steer_speed: 6.0, // Fast steering response
            wheel_visual_angles: [0.0; 4],
            prev_linear_velocity: Vec3::ZERO,
            lateral_accel: 0.0,
            longitudinal_accel: 0.0,
            spec_name: sim.name.clone(),
            shift_hold: 0.0,
        };
        // Approximation A107: the centre of mass sits `PROTOTYPE_COM_HEIGHT_M`
        // above the road at static ride height, on the body origin in plan.
        vehicle.body.center_of_mass.y = PROTOTYPE_COM_HEIGHT_M - vehicle.resting_height();
        vehicle
    }

    /// Reset vehicle position, orientation, and zero out all velocities.
    pub fn reset(&mut self, position: Vec3, yaw: f32) {
        self.body.position = position;
        self.body.orientation = Quat::from_rotation_y(yaw);
        self.body.linear_velocity = Vec3::ZERO;
        self.body.angular_velocity = Vec3::ZERO;
        self.current_steer_angle = 0.0;
        self.shift_hold = 0.0;
        self.powertrain.current_rpm = self.powertrain.idle_rpm;
        self.powertrain.current_gear = 1;
        self.prev_linear_velocity = Vec3::ZERO;
        for w in &mut self.suspension.wheels {
            w.support_owner = Default::default();
            w.tire.omega = 0.0;
            w.compression = 0.0;
        }
        self.wheel_visual_angles = [0.0; 4];
    }

    /// Match wheel spin to the body's ground speed after an external velocity
    /// change (collision response). Without this the wheels keep their old
    /// speed and re-accelerate the car once the contact ends.
    /// Approximation: the original contact response is not recovered.
    pub fn sync_wheel_spin_to_body(&mut self) {
        let forward = self.body.forward();
        let ground_speed =
            self.body.linear_velocity.x * forward.x + self.body.linear_velocity.z * forward.z;
        for wheel in &mut self.suspension.wheels {
            if wheel.tire.radius > 0.0 {
                wheel.tire.omega = ground_speed / wheel.tire.radius;
            }
        }
    }

    /// Approximate upright body height over a flat road under static spring load.
    pub fn resting_height(&self) -> f32 {
        let spring_sum: f32 = self.suspension.wheels.iter().map(|w| w.spring_k).sum();
        let compression = self.body.mass * GRAVITY / spring_sum;
        self.suspension
            .wheels
            .iter()
            .map(|w| {
                w.rest_length + w.tire.radius
                    - w.hardpoint_body.y
                    - compression.min(w.max_compression)
            })
            .sum::<f32>()
            / 4.0
    }

    /// Keyboard/pad two-pedal convention: S brakes first, then drives in reverse.
    /// Direction changes are allowed only after braking has stopped ground motion.
    pub fn controls_from_signed_pedal(
        &self,
        pedal: f32,
        steer: f32,
        handbrake: bool,
    ) -> VehicleControls {
        let pedal = if pedal.is_finite() {
            pedal.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let mut controls = VehicleControls {
            steer,
            handbrake,
            ..VehicleControls::default()
        };
        if pedal == 0.0 {
            return controls;
        }
        let desired = if pedal < 0.0 { -1 } else { 1 };
        let current = self.powertrain.current_gear.signum();
        // Suspension settling on a pitched chassis is not ground travel. Using
        // the full 3D forward speed here can keep braking after XZ has stopped.
        let forward = self.body.forward();
        let speed =
            self.body.linear_velocity.x * forward.x + self.body.linear_velocity.z * forward.z;
        let ground_speed = self
            .body
            .linear_velocity
            .x
            .hypot(self.body.linear_velocity.z);
        if speed * (desired as f32) < -0.001 || (desired != current && ground_speed > 0.001) {
            controls.brake = pedal.abs();
        } else {
            controls.throttle = pedal.abs();
            if desired != current {
                controls.manual_gear = Some(desired);
            }
        }
        controls
    }

    /// Advance dynamics by `dt` seconds with explicit pedal and gear controls.
    pub fn step(
        &mut self,
        surface: Option<&RoadSurface>,
        controls: &VehicleControls,
        dt: f32,
    ) -> VehicleTelemetry {
        let safe_dt = dt.clamp(1e-4, 0.1);

        // Determine number of sub-steps for numerical stability of springs/tire friction
        let num_sub_steps = (safe_dt / MAX_SUB_STEP_DT).ceil().max(1.0) as usize;
        let sub_dt = safe_dt / (num_sub_steps as f32);

        // Process gear shift requests
        if let Some(target_gear) = controls.manual_gear {
            self.powertrain.shift_to(target_gear);
        } else if controls.auto_gear {
            self.update_automatic_transmission(safe_dt);
        }

        // Speed-sensitive steering angle limit to prevent violent spinouts at high velocity
        let speed_mps = self.body.forward_speed();
        let speed_factor = (1.0 / (1.0 + speed_mps.abs() * 0.025)).clamp(0.20, 1.0);
        // Approximation A115: full lock may not ask for more lateral
        // acceleration than the front tyres hold. The old limit
        // (1 / (1 + 0.025 v)) left 18 degrees at 29 m/s, which is 2.8 g of
        // kinematic turn demand: every keyboard turn at speed was a slide.
        // The grip-limited angle is mu·g·L / v² (the original gives about
        // 1.5 degrees at 36 m/s on full input, `Car_PlanarDynamics` step 6).
        let grip_limited = {
            let w: Vec<_> = self.suspension.wheels.iter().collect();
            let front_z = w.iter().filter(|w| w.is_steered).map(|w| w.hardpoint_body.z).sum::<f32>()
                / w.iter().filter(|w| w.is_steered).count().max(1) as f32;
            let rear_z = w.iter().filter(|w| !w.is_steered).map(|w| w.hardpoint_body.z).sum::<f32>()
                / w.iter().filter(|w| !w.is_steered).count().max(1) as f32;
            let base = w.iter().filter(|w| w.is_steered).map(|w| w.tire.base_friction_coeff * w.tire.grip_multiplier).sum::<f32>()
                / w.iter().filter(|w| w.is_steered).count().max(1) as f32;
            let wheelbase = (front_z - rear_z).abs().max(1.0);
            (2.0 * base * GRAVITY * wheelbase / (speed_mps * speed_mps).max(1.0)).max(0.02)
        };
        let max_angle = (self.max_steer_angle * speed_factor).min(grip_limited);
        let target_steer = controls.steer.clamp(-1.0, 1.0) * max_angle;

        for _ in 0..num_sub_steps {
            // Smooth steer input towards target
            let steer_delta = target_steer - self.current_steer_angle;
            let max_change = self.steer_speed * sub_dt;
            self.current_steer_angle += steer_delta.clamp(-max_change, max_change);
            self.suspension.set_steer_angle(self.current_steer_angle);

            self.sub_step(surface, controls, sub_dt);
        }

        // Compute instantaneous accelerations in body coordinates
        let accel_world = (self.body.linear_velocity - self.prev_linear_velocity) / safe_dt;
        self.prev_linear_velocity = self.body.linear_velocity;

        let right_dir = self.body.right();
        let forward_dir = self.body.forward();

        let raw_lat = accel_world.dot(right_dir) / GRAVITY;
        let raw_lon = accel_world.dot(forward_dir) / GRAVITY;

        // Low-pass filter for smooth telemetry readouts
        self.lateral_accel = self.lateral_accel * 0.85 + raw_lat * 0.15;
        self.longitudinal_accel = self.longitudinal_accel * 0.85 + raw_lon * 0.15;

        // Build telemetry and rendering matrices
        self.build_telemetry(controls)
    }

    /// Single fixed-delta physics sub-step.
    fn sub_step(&mut self, surface: Option<&RoadSurface>, controls: &VehicleControls, dt: f32) {
        let previous_position = self.body.position;
        let previous_orientation = self.body.orientation;
        // Clear force and torque accumulators
        self.body.clear_accumulators();

        // 1. Suspension contact raycast and spring/damper forces
        self.suspension
            .update_surface_contact(&mut self.body, surface, dt);

        // 2. Powertrain torque delivery
        let throttle = controls.throttle.clamp(0.0, 1.0);
        let avg_wheel_omega = {
            let mut sum = 0.0;
            let mut count = 0.0;
            for w in &self.suspension.wheels {
                if w.is_driven {
                    sum += w.tire.omega;
                    count += 1.0;
                }
            }
            if count > 0.0 { sum / count } else { 0.0 }
        };
        let drive_torque = self.powertrain.step(throttle, avg_wheel_omega, dt);

        // Driven wheels count
        let driven_count = self
            .suspension
            .wheels
            .iter()
            .filter(|w| w.is_driven)
            .count()
            .max(1) as f32;
        // Approximation A114: the drive torque is shared between the driven
        // wheels in proportion to their normal load, as the original splits
        // the drive demand by the wheels' grip limits (`Car_PlanarDynamics`
        // 0x49f160, `w+0xa8 = share·w+0xac·F/sum`). An equal split let the
        // unloaded inside wheel of a corner spin up (open differential), and
        // the box then read the wheel overspeed as wheelspin.
        let driven_load: f32 = self
            .suspension
            .wheels
            .iter()
            .filter(|w| w.is_driven)
            .map(|w| w.tire.normal_load.max(0.0))
            .sum();

        // 3. Braking distribution
        let brake_in = controls.brake.clamp(0.0, 1.0);
        let front_brake_torque = brake_in * self.max_brake_torque * self.brake_bias * 0.5;
        let rear_brake_torque = brake_in * self.max_brake_torque * (1.0 - self.brake_bias) * 0.5;

        // 4. Update each tire contact and forces
        for (idx, wheel) in self.suspension.wheels.iter_mut().enumerate() {
            let is_front = idx == WHEEL_FL || idx == WHEEL_FR;

            let wheel_drive = if wheel.is_driven {
                if driven_load > 1.0 {
                    drive_torque * wheel.tire.normal_load.max(0.0) / driven_load
                } else {
                    drive_torque / driven_count
                }
            } else {
                0.0
            };

            let mut wheel_brake = if is_front {
                front_brake_torque
            } else {
                rear_brake_torque
            };

            // Handbrake locks rear wheels
            if controls.handbrake && !is_front {
                wheel_brake += self.max_brake_torque * 0.6;
            }

            if wheel.in_contact {
                // Calculate wheel contact velocity in world space
                let contact_vel = self.body.point_velocity(wheel.contact_point_world);

                // Compute wheel orientation heading (steered direction)
                // Local forward is -Z: a right turn needs negative rotation around +Y.
                let wheel_rot = Quat::from_axis_angle(self.body.up(), -wheel.steer_angle);
                // Tire friction acts in the road tangent plane, never along a
                // tilted chassis axis that could inject a vertical launch force.
                let heading = wheel_rot * self.body.forward();
                let normal = wheel.ground_normal_world;
                let wheel_fwd = (heading - normal * heading.dot(normal)).normalize_or_zero();
                let wheel_right = wheel_fwd.cross(normal).normalize_or_zero();

                let forward_vel = contact_vel.dot(wheel_fwd);
                let lateral_vel = contact_vel.dot(wheel_right);

                // Normal force on this wheel
                let normal_load = wheel.tire.normal_load;

                // Step tire friction
                let forces = wheel.tire.step(
                    normal_load,
                    forward_vel,
                    lateral_vel,
                    wheel_drive,
                    wheel_brake,
                    dt,
                );
                let f_long = forces.x;
                let f_lat = forces.y;

                // Combine friction forces into world vector
                let tire_force_world = wheel_fwd * f_long + wheel_right * f_lat;

                // Apply tire force to rigid body at contact point
                self.body
                    .apply_force_at_world_pos(tire_force_world, wheel.contact_point_world);
            } else {
                // Wheel is airborne: spin down freely or accelerate from drive torque
                wheel.tire.step(0.0, 0.0, 0.0, wheel_drive, wheel_brake, dt);
            }

            // Visual wheel rotation integration
            self.wheel_visual_angles[idx] += wheel.tire.omega * dt;
        }

        // 5. Aerodynamic drag and downforce
        let v_world = self.body.linear_velocity;
        let speed_sq = v_world.length_squared();
        if speed_sq > 0.01 {
            let drag_mag = 0.5 * AIR_DENSITY * self.drag_coeff * self.frontal_area * speed_sq;
            let drag_force = -v_world.normalize() * drag_mag;
            self.body.apply_central_force(drag_force);

            // Aerodynamic downforce acting through chassis
            let downforce_mag =
                0.5 * AIR_DENSITY * self.downforce_coeff * self.frontal_area * speed_sq;
            let downforce = -self.body.up() * downforce_mag;
            self.body.apply_central_force(downforce);
        }

        // 6. Integrate 6 DOF equations of motion
        self.body.integrate(dt, Vec3::new(0.0, -GRAVITY, 0.0));
        resolve_chassis_ground(
            &mut self.body,
            self.chassis_bounds,
            surface,
            previous_position,
            previous_orientation,
            dt,
        );

        // When vehicle is near standstill and brakes are applied, halt motion to prevent creep
        if controls.brake > 0.3
            && controls.throttle < 0.05
            && self.suspension.wheels.iter().any(|wheel| wheel.in_contact)
            && self
                .body
                .linear_velocity
                .x
                .hypot(self.body.linear_velocity.z)
                < 0.4
        {
            // A brake can stop ground motion, not suspend the chassis in mid-air
            // or suppress the suspension's vertical settling and body roll.
            self.body.linear_velocity.x = 0.0;
            self.body.linear_velocity.z = 0.0;
            self.body.angular_velocity.y = 0.0;
            for w in &mut self.suspension.wheels {
                w.tire.omega = 0.0;
            }
        }
    }

    /// Automatic transmission shift schedule logic (approximation A80, the
    /// prototype's own box, not the original `Car_AutoGearbox` 0x49daf0).
    ///
    /// Shift points come from the road rpm (body forward speed over the
    /// driven wheel radius), not from the driven wheels' spin alone: a
    /// spinning or locked axle in a slide must not move the box. Upshift one
    /// gear above 92 % redline (engine rpm while it is below 1.2 × road rpm,
    /// else road rpm), then hold [`UPSHIFT_HOLD_S`]; downshift to the lowest
    /// gear in which the next lower one would stay under 75 % redline (road
    /// rpm). Only with a driven wheel on the ground.
    fn update_automatic_transmission(&mut self, dt: f32) {
        self.shift_hold = (self.shift_hold - dt).max(0.0);
        if self.powertrain.current_gear <= 0 {
            return;
        }
        let driven = || self.suspension.wheels.iter().filter(|w| w.is_driven);
        let count = driven().count();
        if count == 0 || !driven().any(|w| w.in_contact) {
            return;
        }
        let radius = driven().map(|w| w.tire.radius).sum::<f32>() / count as f32;
        if radius <= 0.0 {
            return;
        }
        let pt = &self.powertrain;
        let wheel_rpm =
            self.body.forward_speed().max(0.0) / radius * 60.0 / (2.0 * std::f32::consts::PI);
        let road_rpm = |gear: i32| wheel_rpm * pt.gear_ratios[(gear - 1) as usize] * pt.final_drive;
        let gear = pt.current_gear;
        let top = pt.forward_gears as i32;
        // As the original box (V3): engine rpm while it is below 1.2 × road
        // rpm, so ordinary tyre slip does not delay the upshift; road rpm
        // once a driven wheel spins.
        let shift_rpm = if pt.current_rpm < 1.2 * road_rpm(gear) {
            pt.current_rpm
        } else {
            road_rpm(gear)
        };
        if gear < top && shift_rpm > pt.redline_rpm * 0.92 {
            if self.shift_hold == 0.0 {
                self.powertrain.shift_up();
                self.shift_hold = UPSHIFT_HOLD_S;
            }
            return;
        }
        let mut target = gear.min(top);
        while target > 1 && road_rpm(target - 1) < pt.redline_rpm * 0.75 {
            target -= 1;
        }
        if target != gear {
            self.powertrain.set_gear(target);
        }
    }

    /// Generate complete telemetry package and visual transform matrices.
    pub fn telemetry(&self, controls: &VehicleControls) -> VehicleTelemetry {
        self.build_telemetry(controls)
    }

    /// Generate complete telemetry package and visual transform matrices.
    fn build_telemetry(&self, controls: &VehicleControls) -> VehicleTelemetry {
        let speed_mps = self.body.forward_speed();
        let speed_kmh = speed_mps * 3.6;

        let chassis_transform = self.body.transform_matrix();

        // Compute 4 individual wheel transform matrices
        let mut wheel_transforms = [Mat4::IDENTITY; 4];
        for (i, wheel) in self.suspension.wheels.iter().enumerate() {
            // Wheel position relative to body: hardpoint + (uncompressed_len - compression) along -Y
            // The suspension ray reaches the contact patch; rendered geometry
            // is centered one tire radius above that patch along the strut.
            let current_len = wheel.rest_length - wheel.compression;
            let rel_wheel_pos = wheel.hardpoint_body - Vec3::Y * current_len;

            // Wheel rotation: steer angle (Y) * visual roll angle (X)
            let steer_quat = if wheel.is_steered {
                Quat::from_rotation_y(-wheel.steer_angle)
            } else {
                Quat::IDENTITY
            };
            // Forward is -Z: at the bottom of the tire, rolling must oppose
            // vehicle travel so the contact patch stays stationary on the road.
            let roll_quat = Quat::from_rotation_x(-self.wheel_visual_angles[i]);

            let local_wheel_mat =
                Mat4::from_translation(rel_wheel_pos) * Mat4::from_quat(steer_quat * roll_quat);

            wheel_transforms[i] = chassis_transform * local_wheel_mat;
        }

        VehicleTelemetry {
            speed_mps,
            speed_kmh,
            engine_rpm: self.powertrain.current_rpm,
            current_gear: self.powertrain.current_gear,
            throttle: controls.throttle,
            brake: controls.brake,
            steer_angle_rad: self.current_steer_angle,
            lateral_accel_g: self.lateral_accel,
            longitudinal_accel_g: self.longitudinal_accel,
            yaw_rate_rad_s: self.body.angular_velocity.y,
            wheel_omegas: [
                self.suspension.wheels[WHEEL_FL].tire.omega,
                self.suspension.wheels[WHEEL_FR].tire.omega,
                self.suspension.wheels[WHEEL_RL].tire.omega,
                self.suspension.wheels[WHEEL_RR].tire.omega,
            ],
            wheel_compressions: [
                self.suspension.wheels[WHEEL_FL].compression,
                self.suspension.wheels[WHEEL_FR].compression,
                self.suspension.wheels[WHEEL_RL].compression,
                self.suspension.wheels[WHEEL_RR].compression,
            ],
            wheel_slip_ratios: [
                self.suspension.wheels[WHEEL_FL].tire.slip_ratio,
                self.suspension.wheels[WHEEL_FR].tire.slip_ratio,
                self.suspension.wheels[WHEEL_RL].tire.slip_ratio,
                self.suspension.wheels[WHEEL_RR].tire.slip_ratio,
            ],
            wheel_slip_angles: [
                self.suspension.wheels[WHEEL_FL].tire.slip_angle,
                self.suspension.wheels[WHEEL_FR].tire.slip_angle,
                self.suspension.wheels[WHEEL_RL].tire.slip_angle,
                self.suspension.wheels[WHEEL_RR].tire.slip_angle,
            ],
            wheels_in_contact: [
                self.suspension.wheels[WHEEL_FL].in_contact,
                self.suspension.wheels[WHEEL_FR].in_contact,
                self.suspension.wheels[WHEEL_RL].in_contact,
                self.suspension.wheels[WHEEL_RR].in_contact,
            ],
            chassis_transform,
            wheel_transforms,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::surface::{RoadTriangle, RoadTriangleIdentity};

    pub(crate) fn test_boxster_sim() -> SimCar {
        SimCar {
            name: "Test Boxster".to_string(),
            mass_kg: 1252.0,
            wheelbase_m: 2.415,
            gear_count: 5,
            drive_flags: 2,
            reverse_gear: -3.44,
            forward_gears: vec![3.50, 2.12, 1.43, 1.03, 0.79],
            final_drive: 3.89,
            redline_rpm: 6700.0,
            idle_or_step_rpm: 800.0,
            torque_curve: [
                98.0, 102.0, 130.0, 138.0, 157.0, 157.0, 169.0, 169.0, 173.0, 181.0, 181.0, 173.0,
                146.0, 110.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ],
            brake_bias: 0.62,
            drag_coeff: 0.31,
            swaybar_stiffness: 12000.0,
            suspension_stiffness: 28.0,
            front_track_m: 1.465,
            rear_track_m: 1.500,
            damping_compression: 2.5,
            damping_rebound: 3.2,
            tire_grip: 100.0,
            cg_offset_m: [0.0, 0.35, -0.1],
            raw: [0u8; 328],
        }
    }

    /// 935-class car: grip 1.5 and a ".sim cg" word of 0.06 (really suspension
    /// travel), wheel pivots 0.8 m off the centreline.
    fn grippy_car() -> VehicleSimulation {
        let mut sim = test_boxster_sim();
        sim.mass_kg = 1030.0;
        sim.wheelbase_m = 2.272;
        sim.tire_grip = 1.5;
        sim.cg_offset_m = [0.0, 0.06, 0.001];
        let mut v = VehicleSimulation::from_sim(&sim);
        for (i, w) in v.suspension.wheels.iter_mut().enumerate() {
            w.hardpoint_body.x = if i % 2 == 0 { -0.8 } else { 0.8 };
        }
        v
    }

    #[test]
    fn centre_of_mass_height_ignores_the_misread_sim_words() {
        // A107: `.sim` 0x120/0x124/0x128 are ride height, suspension travel and
        // toe, not a CG offset; the CoM is a fixed height above the road.
        for cg in [[0.0, 0.05, 0.001], [0.0, 0.14, 0.005], [0.0, 0.35, -0.1]] {
            let mut sim = test_boxster_sim();
            sim.cg_offset_m = cg;
            let v = VehicleSimulation::from_sim(&sim);
            let height = v.resting_height() + v.body.center_of_mass.y;
            assert!((height - PROTOTYPE_COM_HEIGHT_M).abs() < 1e-5, "{cg:?}: {height}");
            assert_eq!(v.body.center_of_mass.x, 0.0);
            assert_eq!(v.body.center_of_mass.z, 0.0);
        }
    }

    #[test]
    fn full_lock_at_speed_slides_instead_of_rolling_over() {
        // Playtest: cars rolled over when steered. Before A107 this 935-like car
        // (grip 1.5) put its centre of mass 0.56 m up and tipped at 100 km/h.
        let mut v = grippy_car();
        let h = v.resting_height();
        v.reset(Vec3::new(0.0, h, 0.0), 0.0);
        let speed = 100.0 / 3.6;
        v.body.linear_velocity = v.body.forward() * speed;
        v.sync_wheel_spin_to_body();
        let mut min_up = 1.0f32;
        for step in 0..360 {
            let controls = VehicleControls {
                throttle: if v.body.forward_speed() < speed { 1.0 } else { 0.0 },
                steer: if step >= 30 { 1.0 } else { 0.0 },
                ..Default::default()
            };
            v.step(None, &controls, 1.0 / 60.0);
            min_up = min_up.min(v.body.up().y);
        }
        assert!(min_up > 0.9, "body tipped over: min up.y = {min_up}");
    }

    #[test]
    fn vehicle_initialization_from_sim() {
        let sim = test_boxster_sim();
        let vehicle = VehicleSimulation::from_sim(&sim);

        assert_eq!(vehicle.body.mass, 1252.0);
        assert_eq!(vehicle.powertrain.forward_gears, 5);
        assert_eq!(vehicle.suspension.wheels.len(), 4);
    }

    /// Boxster test car on flat ground at `speed` m/s forward in `gear`,
    /// driven wheels on the ground.
    fn rolling(speed: f32, gear: i32) -> VehicleSimulation {
        let mut v = VehicleSimulation::from_sim(&test_boxster_sim());
        v.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);
        v.body.linear_velocity = v.body.forward() * speed;
        v.sync_wheel_spin_to_body();
        for w in &mut v.suspension.wheels {
            w.in_contact = true;
        }
        v.powertrain.set_gear(gear);
        v
    }

    /// Engine rpm at the road speed in `gear` (no wheel slip).
    fn road_rpm(v: &VehicleSimulation, gear: i32) -> f32 {
        let r = v.suspension.wheels[2].tire.radius;
        let pt = &v.powertrain;
        v.body.forward_speed().max(0.0) / r
            * pt.gear_ratios[(gear - 1) as usize]
            * pt.final_drive
            * 60.0
            / (2.0 * std::f32::consts::PI)
    }

    #[test]
    fn auto_box_ignores_a_spinning_or_free_revving_engine() {
        // Rear wheels spinning (or locked with the engine free-revving): the
        // engine is at redline while the road speed is low. The old box
        // upshifted on engine rpm every frame and ran up to top gear.
        let mut v = rolling(12.0, 2);
        assert!(road_rpm(&v, 2) < 0.6 * v.powertrain.redline_rpm);
        for _ in 0..60 {
            v.powertrain.current_rpm = v.powertrain.redline_rpm;
            v.update_automatic_transmission(1.0 / 60.0);
        }
        assert_eq!(v.powertrain.current_gear, 2);
    }

    #[test]
    fn auto_box_upshifts_one_gear_per_hold() {
        let mut v = rolling(25.0, 1);
        let r = v.powertrain.redline_rpm;
        assert!(road_rpm(&v, 2) > 0.92 * r && road_rpm(&v, 3) < 0.92 * r);
        v.powertrain.current_rpm = r;
        v.update_automatic_transmission(1.0 / 60.0);
        assert_eq!(v.powertrain.current_gear, 2);
        for _ in 0..(UPSHIFT_HOLD_S * 60.0) as usize - 2 {
            v.powertrain.current_rpm = r;
            v.update_automatic_transmission(1.0 / 60.0);
        }
        assert_eq!(v.powertrain.current_gear, 2, "held after the upshift");
        for _ in 0..30 {
            v.powertrain.current_rpm = r;
            v.update_automatic_transmission(1.0 / 60.0);
        }
        assert_eq!(v.powertrain.current_gear, 3);
    }

    #[test]
    fn auto_box_drops_out_of_top_gear_when_slow() {
        // After a spin or a wall the car rolls slowly in a tall gear: the
        // box must pick a gear that pulls (the old one only went down below
        // 2400 rpm of driven-wheel spin, one gear per frame).
        let mut v = rolling(6.0, 5);
        v.powertrain.current_rpm = v.powertrain.redline_rpm;
        v.update_automatic_transmission(1.0 / 60.0);
        let g = v.powertrain.current_gear;
        assert!(g <= 2, "gear {g}");
        assert!(g == 1 || road_rpm(&v, g - 1) >= 0.75 * v.powertrain.redline_rpm);
        // Rolling backwards in a forward gear: 1st.
        let mut v = rolling(-4.0, 4);
        v.update_automatic_transmission(1.0 / 60.0);
        assert_eq!(v.powertrain.current_gear, 1);
    }

    #[test]
    fn auto_box_holds_its_gear_through_power_oversteer() {
        // Full throttle, then full lock at full throttle (inside rear wheel
        // spinning): every upshift must come from the road speed. The rule
        // (engine rpm while below 1.2 x road rpm) allows an upshift from
        // road rpm 0.92 / 1.2 = 0.767 of redline upward.
        let mut v = VehicleSimulation::from_sim(&test_boxster_sim());
        v.reset(Vec3::Y * v.resting_height(), 0.0);
        let r = v.powertrain.redline_rpm;
        let mut spun = false;
        for frame in 0..(9 * 60) {
            let steer = if frame < 4 * 60 { 0.0 } else { 1.0 };
            let before = v.powertrain.current_gear;
            let rpm = road_rpm(&v, before.max(1));
            let c = v.controls_from_signed_pedal(1.0, steer, false);
            let t = v.step(None, &c, 1.0 / 60.0);
            spun |= t.wheel_omegas[2].max(t.wheel_omegas[3]) > 1.5 * t.wheel_omegas[0];
            let after = v.powertrain.current_gear;
            if after > before && before >= 1 {
                assert!(
                    rpm > 0.76 * r,
                    "frame {frame}: upshift {before}->{after} at road rpm {rpm}"
                );
            }
        }
        assert!(spun, "the scenario must spin a rear wheel");
    }

    #[test]
    fn vehicle_accelerates_on_throttle() {
        let sim = test_boxster_sim();
        let mut vehicle = VehicleSimulation::from_sim(&sim);

        // Put vehicle on flat ground
        vehicle.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);

        let controls = VehicleControls {
            throttle: 1.0,
            brake: 0.0,
            steer: 0.0,
            handbrake: false,
            manual_gear: Some(1),
            auto_gear: false,
        };

        // Simulate 1 second of full throttle in 1st gear
        for _ in 0..60 {
            vehicle.step(None, &controls, 1.0 / 60.0);
        }

        // Vehicle should have accelerated forward (negative Z is forward)
        assert!(
            vehicle.body.linear_velocity.z < -1.0,
            "Car should move forward (-Z)"
        );
        assert!(
            vehicle.body.forward_speed() > 1.0,
            "Forward speed should be positive"
        );
    }

    #[test]
    fn wheel_spin_follows_body_after_collision() {
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);
        for w in &mut vehicle.suspension.wheels {
            w.tire.omega = 30.0 / w.tire.radius;
        }
        // A wall stopped the body; the wheels must not keep 30 m/s of spin.
        vehicle.body.linear_velocity = Vec3::new(0.0, 0.0, -5.0);
        vehicle.sync_wheel_spin_to_body();
        for w in &vehicle.suspension.wheels {
            assert!((w.tire.omega * w.tire.radius - 5.0).abs() < 1e-3);
        }
    }

    #[test]
    fn vehicle_brakes_and_decelerates() {
        let sim = test_boxster_sim();
        let mut vehicle = VehicleSimulation::from_sim(&sim);

        vehicle.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);
        // Give vehicle initial forward speed of 20 m/s (~72 km/h)
        vehicle.body.linear_velocity = Vec3::new(0.0, 0.0, -20.0);
        for w in &mut vehicle.suspension.wheels {
            w.tire.omega = 20.0 / w.tire.radius;
        }

        let brake_controls = VehicleControls {
            throttle: 0.0,
            brake: 1.0,
            steer: 0.0,
            handbrake: false,
            manual_gear: None,
            auto_gear: true,
        };

        // Simulate 0.5s of hard braking
        for _ in 0..30 {
            vehicle.step(None, &brake_controls, 1.0 / 60.0);
        }

        assert!(
            vehicle.body.forward_speed() < 18.0,
            "Speed should drop significantly under braking"
        );
    }

    fn flat_test_surface() -> RoadSurface {
        let corners = [
            [-40.0, 0.0, -40.0],
            [40.0, 0.0, -40.0],
            [40.0, 0.0, 40.0],
            [-40.0, 0.0, 40.0],
        ];
        RoadSurface::from_triangles([[0, 1, 2], [0, 2, 3]].into_iter().map(|indices| {
            RoadTriangle {
                identity: RoadTriangleIdentity {
                    article_name: "flat test road".into(),
                    article_index: 0,
                    mesh_name: "flat test mesh".into(),
                    primitive_index: 0,
                    triangle_index: indices[0],
                },
                positions: indices.map(|index| corners[index]),
            }
        }))
    }

    #[test]
    fn first_contact_and_recontact_do_not_create_damper_impulses() {
        let surface = flat_test_surface();
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);
        for dt in [1.0 / 30.0, 1.0 / 240.0] {
            vehicle.body.clear_accumulators();
            for w in &mut vehicle.suspension.wheels {
                w.compression = 0.0;
                w.in_contact = false;
            }
            vehicle
                .suspension
                .update_surface_contact(&mut vehicle.body, Some(&surface), dt);
            assert!(
                vehicle
                    .suspension
                    .wheels
                    .iter()
                    .all(|w| w.in_contact && w.compression_velocity == 0.0)
            );
            assert!(
                vehicle
                    .suspension
                    .wheels
                    .iter()
                    .all(|w| w.tire.normal_load <= w.spring_k * w.max_compression + 0.01)
            );
        }
        let controls = VehicleControls::default();
        vehicle.step(Some(&surface), &controls, 1.0 / 240.0);
        assert!(
            vehicle.body.linear_velocity.y < 0.15,
            "stationary contact launched chassis: {:?}",
            vehicle.body.linear_velocity
        );
    }

    #[test]
    fn missing_surface_does_not_invent_ground_at_zero() {
        let surface = RoadSurface::from_triangles(std::iter::empty());
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.reset(Vec3::new(0.0, 0.35, 0.0), 0.0);
        let telemetry = vehicle.step(Some(&surface), &VehicleControls::default(), 1.0 / 60.0);
        assert!(telemetry.wheels_in_contact.iter().all(|&contact| !contact));
        assert!(vehicle.body.linear_velocity.y < 0.0);
    }

    #[test]
    fn same_pedal_brakes_to_stop_then_reverses_without_release() {
        let surface = flat_test_surface();
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.reset(Vec3::new(0.0, vehicle.resting_height(), 0.0), 0.0);
        vehicle.body.linear_velocity = Vec3::new(0.0, 0.0, -12.0);
        for w in &mut vehicle.suspension.wheels {
            w.tire.omega = 12.0 / w.tire.radius;
        }
        let mut reversed = false;
        for _ in 0..600 {
            let controls = vehicle.controls_from_signed_pedal(-1.0, 0.0, false);
            if controls.manual_gear == Some(-1) {
                assert!(
                    vehicle
                        .body
                        .linear_velocity
                        .x
                        .hypot(vehicle.body.linear_velocity.z)
                        <= 0.001
                );
                reversed = true;
            }
            if !reversed {
                assert_eq!(controls.throttle, 0.0);
                assert_eq!(controls.brake, 1.0);
            }
            vehicle.step(Some(&surface), &controls, 1.0 / 60.0);
            if vehicle.body.forward_speed() < -2.0 {
                break;
            }
        }
        assert!(reversed, "held S never selected reverse");
        assert!(
            vehicle.body.forward_speed() < -2.0,
            "held S must drive backward after stopping"
        );
        let forward = vehicle.controls_from_signed_pedal(1.0, 0.0, false);
        assert_eq!(forward.throttle, 0.0);
        assert_eq!(forward.brake, 1.0, "W must first stop reverse motion");
        assert_eq!(forward.manual_gear, None);
    }

    #[test]
    fn original_boxster_held_pedals_reverse_then_forward() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Simulation/CarData/boxster25.sim");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("SKIP optional original SIM regression: {}", path.display());
                return;
            }
            Err(error) => panic!("cannot read {}: {error}", path.display()),
        };
        let spec = nfs_formats::parse_sim(&bytes).expect("installed Boxster SIM must parse");
        eprintln!("Original SIM regression: {}", path.display());
        let surface = flat_test_surface();
        for hz in [30, 60, 120] {
            let mut vehicle = VehicleSimulation::from_sim(&spec);
            vehicle.reset(Vec3::Y * vehicle.resting_height(), 0.0);
            for _ in 0..hz * 12 {
                let controls = vehicle.controls_from_signed_pedal(-1.0, 0.0, false);
                vehicle.step(Some(&surface), &controls, 1.0 / hz as f32);
                if vehicle.body.forward_speed() < -1.0 {
                    break;
                }
            }
            assert_eq!(vehicle.powertrain.current_gear, -1);
            assert!(
                vehicle.body.forward_speed() < -1.0,
                "S must establish reverse"
            );
            for _ in 0..hz * 12 {
                let controls = vehicle.controls_from_signed_pedal(1.0, 0.0, false);
                if controls.manual_gear == Some(1) {
                    assert!(
                        vehicle
                            .body
                            .linear_velocity
                            .x
                            .hypot(vehicle.body.linear_velocity.z)
                            <= 0.001,
                        "held W must stop ground motion before selecting forward"
                    );
                }
                vehicle.step(Some(&surface), &controls, 1.0 / hz as f32);
                if vehicle.body.forward_speed() > 1.0 {
                    break;
                }
            }
            assert!(
                vehicle.powertrain.current_gear > 0 && vehicle.body.forward_speed() > 1.0,
                "held W failed at {hz} Hz: gear={} velocity={:?}, forward={:?}, wheels={:?}",
                vehicle.powertrain.current_gear,
                vehicle.body.linear_velocity,
                vehicle.body.forward(),
                vehicle
                    .suspension
                    .wheels
                    .iter()
                    .map(|w| (w.in_contact, w.tire.omega))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn direction_change_ignores_vertical_settling_after_ground_stop() {
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.powertrain.current_gear = -1;
        vehicle.body.orientation = Quat::from_rotation_x(0.1);
        vehicle.body.linear_velocity = Vec3::new(0.0, -0.25, 0.0);
        assert!(vehicle.body.forward_speed() < -0.001);
        let controls = vehicle.controls_from_signed_pedal(1.0, 0.0, false);
        assert_eq!(
            controls.manual_gear,
            Some(1),
            "ground motion has already stopped"
        );
        assert_eq!(controls.throttle, 1.0);
        vehicle.body.linear_velocity.z = 1.0;
        let moving = vehicle.controls_from_signed_pedal(1.0, 0.0, false);
        assert_eq!(
            moving.manual_gear, None,
            "must still brake reverse ground motion"
        );
        assert_eq!(moving.brake, 1.0);
    }

    #[test]
    fn resting_car_settles_without_hopping_at_multiple_frame_rates() {
        let surface = flat_test_surface();
        for hz in [30, 60, 120] {
            let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
            vehicle.reset(Vec3::new(0.0, vehicle.resting_height(), 0.0), 0.0);
            for _ in 0..hz * 4 {
                vehicle.step(Some(&surface), &VehicleControls::default(), 1.0 / hz as f32);
                assert!(
                    vehicle.body.position.y > 0.2 && vehicle.body.position.y < 0.8,
                    "resting chassis jumped or sank at {hz} Hz: {:?}",
                    vehicle.body.position
                );
                assert!(vehicle.body.up().y > 0.95, "stationary chassis rolled over");
            }
        }
    }

    #[test]
    fn brake_does_not_freeze_airborne_vertical_motion() {
        let surface = RoadSurface::from_triangles(std::iter::empty());
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        vehicle.reset(Vec3::new(0.0, 2.0, 0.0), 0.0);
        vehicle.step(
            Some(&surface),
            &VehicleControls {
                brake: 1.0,
                ..VehicleControls::default()
            },
            1.0 / 60.0,
        );
        assert!(vehicle.body.linear_velocity.y < -0.1);
    }

    #[test]
    fn steering_direction_in_forward_and_reverse_from_two_headings() {
        let surface = flat_test_surface();
        assert_eq!(surface.triangle_count(), 2);
        for initial_yaw in [0.0, 0.7] {
            for (speed, gear) in [(12.0, 1), (-8.0, -1)] {
                for steer in [-0.6, 0.6] {
                    let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
                    vehicle.reset(Vec3::new(0.0, 0.35, 0.0), initial_yaw);
                    vehicle.body.linear_velocity = vehicle.body.forward() * speed;
                    for wheel in &mut vehicle.suspension.wheels {
                        wheel.tire.omega = speed / wheel.tire.radius;
                    }
                    let controls = VehicleControls {
                        steer,
                        manual_gear: Some(gear),
                        auto_gear: false,
                        ..VehicleControls::default()
                    };
                    let mut telemetry = vehicle.step(Some(&surface), &controls, 1.0 / 60.0);
                    for _ in 1..24 {
                        telemetry = vehicle.step(Some(&surface), &controls, 1.0 / 60.0);
                    }
                    let local_forward =
                        Quat::from_rotation_y(-initial_yaw) * vehicle.body.forward();
                    let expected = steer.signum() * speed.signum();
                    assert!(
                        local_forward.x * expected > 0.005,
                        "steer={steer}, speed={speed}, yaw={initial_yaw}: heading={local_forward:?}"
                    );
                    // Wheel local X is its axle; spin around X does not affect this direction.
                    let visual_axle =
                        telemetry.wheel_transforms[WHEEL_FL].transform_vector3(Vec3::X);
                    assert!(
                        -visual_axle.dot(vehicle.body.forward()) * steer.signum() > 0.1,
                        "front wheel visual disagrees with steer={steer}, yaw={initial_yaw}"
                    );
                }
            }
        }
    }

    #[test]
    fn visual_tire_spin_opposes_travel_at_the_contact_patch() {
        let mut vehicle = VehicleSimulation::from_sim(&test_boxster_sim());
        let controls = VehicleControls::default();
        let radius = vehicle.suspension.wheels[0].tire.radius;
        let bottom = -Vec3::Y * radius;
        let before =
            vehicle.build_telemetry(&controls).wheel_transforms[0].transform_point3(bottom);
        for direction in [-1.0, 1.0] {
            vehicle.wheel_visual_angles[0] = direction * 0.05;
            let after =
                vehicle.build_telemetry(&controls).wheel_transforms[0].transform_point3(bottom);
            assert!((after - before).dot(vehicle.body.forward()) * direction < 0.0);
        }
    }
}

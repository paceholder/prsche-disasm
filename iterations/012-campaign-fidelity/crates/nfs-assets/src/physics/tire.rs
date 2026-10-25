//! Tire friction and slip dynamics.
//!
//! Computes longitudinal and lateral forces via friction ellipse with grip factors from `.sim`.

use glam::Vec2;

/// Longitudinal force per unit slip ratio in units of the peak friction force
/// (approximation A113): the force saturates at 1/12 slip ratio.
pub const LONG_SLIP_STIFFNESS: f32 = 12.0;

/// State and forces for a single tire.
#[derive(Debug, Clone, PartialEq)]
pub struct Tire {
    /// Rolling radius (m, typically ~0.30 - 0.32m).
    pub radius: f32,
    /// Rotational inertia of wheel and tire assembly (kg*m^2).
    pub inertia: f32,
    /// Current wheel angular velocity (rad/s).
    pub omega: f32,
    /// Base peak friction coefficient (mu, typically ~1.0 - 1.1 on clean asphalt).
    pub base_friction_coeff: f32,
    /// Grip multiplier from `.sim` (normalized around 1.0, e.g. 100.0 / 100.0).
    pub grip_multiplier: f32,
    /// Cornering stiffness (1/rad, typically ~10.0 - 15.0).
    pub cornering_stiffness: f32,

    /// Calculated longitudinal slip ratio (-1.0 to +1.0).
    pub slip_ratio: f32,
    /// Calculated lateral slip angle in radians.
    pub slip_angle: f32,
    /// Vertical normal force on tire (N).
    pub normal_load: f32,
    /// Generated longitudinal force along wheel forward direction (N).
    pub longitudinal_force: f32,
    /// Generated lateral force perpendicular to wheel heading (N).
    pub lateral_force: f32,
}

impl Tire {
    /// Create a new tire with given radius and grip multiplier.
    pub fn new(radius: f32, grip_multiplier: f32) -> Self {
        Self {
            radius: radius.max(0.1),
            inertia: 1.2, // ~15 kg wheel with 0.31m radius ≈ 0.5 * 15 * 0.31^2 ≈ 0.72 - 1.2 kg*m^2
            omega: 0.0,
            base_friction_coeff: 1.0,
            grip_multiplier: grip_multiplier.max(0.1),
            cornering_stiffness: 12.0,
            slip_ratio: 0.0,
            slip_angle: 0.0,
            normal_load: 0.0,
            longitudinal_force: 0.0,
            lateral_force: 0.0,
        }
    }

    /// Linear ground speed of wheel surface due to rotation (m/s).
    pub fn surface_speed(&self) -> f32 {
        self.omega * self.radius
    }

    fn integrate_rotation(&mut self, external_torque: f32, brake_torque: f32, dt: f32) {
        let unbraked_omega = self.omega + external_torque * dt / self.inertia;
        let brake_delta = brake_torque.max(0.0) * dt / self.inertia;
        // A brake can dissipate angular momentum up to standstill, never drive
        // the wheel through zero into opposite rotation during this step.
        self.omega = unbraked_omega.signum() * (unbraked_omega.abs() - brake_delta).max(0.0);
    }

    /// Compute contact forces and update wheel rotational velocity.
    ///
    /// - `normal_load`: vertical force from suspension (N)
    /// - `wheel_forward_speed`: velocity component along wheel heading (m/s)
    /// - `wheel_lateral_speed`: velocity component perpendicular to wheel heading (m/s)
    /// - `drive_torque`: motor/drivetrain torque applied to wheel (Nm)
    /// - `brake_torque`: braking torque opposing rotation (Nm, always >= 0)
    /// - `dt`: simulation step time (s)
    pub fn step(
        &mut self,
        normal_load: f32,
        wheel_forward_speed: f32,
        wheel_lateral_speed: f32,
        drive_torque: f32,
        brake_torque: f32,
        dt: f32,
    ) -> Vec2 {
        self.normal_load = normal_load.max(0.0);

        if self.normal_load <= 1e-3 {
            // Tire is airborne: zero contact forces
            self.slip_ratio = 0.0;
            self.slip_angle = 0.0;
            self.longitudinal_force = 0.0;
            self.lateral_force = 0.0;

            // Free wheel rotation under drive/brake torque
            self.integrate_rotation(drive_torque, brake_torque, dt);
            return Vec2::ZERO;
        }

        // 1. Longitudinal slip ratio kappa = (omega * r - v_x) / max(|v_x|, 1.0)
        let v_long = wheel_forward_speed;
        let denom = v_long.abs().max(1.0);

        // 2. Lateral slip angle alpha = -atan2(v_lat, |v_long| + 0.1)
        self.slip_angle = -wheel_lateral_speed.atan2(v_long.abs().max(0.5));

        // 3. Peak friction budget (Coulomb friction limit)
        let mu_peak = self.base_friction_coeff * self.grip_multiplier;
        let max_traction = mu_peak * self.normal_load;

        // 4. Longitudinal force proportional to slip ratio with stiffness
        // `LONG_SLIP_STIFFNESS` (peak at about 8 % slip). The stiffness is so
        // high that an explicit wheel update is unstable (k·dt·r²/I ≫ 2), so
        // the wheel speed used for the slip is the implicit solution of
        // I·dω/dt = T_drive − r·a·(ω·r − v), a = stiffness·F_max/denom.
        // Without the implicit step the stiffness had to stay at 2 (peak at
        // 50 % slip), which made every ordinary acceleration run at 20-40 %
        // wheel overspeed (approximation A113).
        let a = LONG_SLIP_STIFFNESS * max_traction / denom;
        let r = self.radius;
        let omega_implicit = (self.omega + dt / self.inertia * (drive_torque + r * a * v_long))
            / (1.0 + dt * r * r * a / self.inertia);
        self.slip_ratio = ((omega_implicit * r - v_long) / denom).clamp(-1.0, 1.0);
        let f_long_req = self.slip_ratio * max_traction * LONG_SLIP_STIFFNESS;

        // Lateral cornering force proportional to slip angle:
        let f_lat_req = (self.slip_angle * self.cornering_stiffness * self.normal_load)
            .clamp(-max_traction, max_traction);

        // 5. Combined friction ellipse:
        // (F_long / max_traction)^2 + (F_lat / max_traction)^2 <= 1
        let norm_long = f_long_req / max_traction.max(1.0);
        let norm_lat = f_lat_req / max_traction.max(1.0);
        let combined_demand = (norm_long * norm_long + norm_lat * norm_lat).sqrt();

        if combined_demand > 1.0 {
            self.longitudinal_force = f_long_req / combined_demand;
            self.lateral_force = f_lat_req / combined_demand;
        } else {
            self.longitudinal_force = f_long_req;
            self.lateral_force = f_lat_req;
        }

        // 6. Update wheel rotational speed:
        // I * domega/dt = DriveTorque - BrakeTorque*sign(omega) - F_long * Radius
        let tire_torque = self.longitudinal_force * self.radius;
        let ext_torque = drive_torque - tire_torque;

        self.integrate_rotation(ext_torque, brake_torque, dt);

        // Prevent micro-oscillations near zero speed
        if v_long.abs() < 0.2 && drive_torque.abs() < 1.0 && brake_torque > 10.0 {
            self.omega = 0.0;
            self.longitudinal_force = -v_long * self.normal_load * 2.0;
        }

        Vec2::new(self.longitudinal_force, self.lateral_force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airborne_tire_has_zero_forces() {
        let mut tire = Tire::new(0.31, 1.0);
        let forces = tire.step(0.0, 20.0, 2.0, 500.0, 0.0, 0.01);
        assert_eq!(forces, Vec2::ZERO);
        assert_eq!(tire.longitudinal_force, 0.0);
        assert_eq!(tire.lateral_force, 0.0);
    }

    #[test]
    fn strong_braking_stops_grounded_wheel_without_reversing_it() {
        for initial_omega in [-2.0, 2.0] {
            for dt in [1.0 / 240.0, 1.0 / 480.0, 1.0 / 960.0] {
                let mut tire = Tire::new(0.31, 1.0);
                tire.omega = initial_omega;
                // Start with zero slip, isolating the brake's angular impulse.
                tire.step(4000.0, initial_omega * tire.radius, 0.0, 0.0, 10000.0, dt);
                assert_eq!(
                    tire.omega, 0.0,
                    "braking alone must stop, not reverse a wheel: start={initial_omega}, dt={dt}"
                );
            }
        }
    }

    #[test]
    fn airborne_braking_stops_wheel_without_drive_torque() {
        for initial_omega in [-2.0, 2.0] {
            let mut tire = Tire::new(0.31, 1.0);
            tire.omega = initial_omega;
            assert_eq!(
                tire.step(0.0, 0.0, 0.0, 0.0, 10000.0, 1.0 / 240.0),
                Vec2::ZERO
            );
            assert_eq!(
                tire.omega, 0.0,
                "brakes must act even without road contact or drive torque"
            );
        }
    }

    #[test]
    fn modest_brake_slows_rotation_without_instant_lock() {
        for load in [0.0, 4000.0] {
            for direction in [-1.0, 1.0] {
                let mut tire = Tire::new(0.31, 1.0);
                tire.omega = direction * 20.0;
                tire.step(load, tire.surface_speed(), 0.0, 0.0, 120.0, 1.0 / 240.0);
                assert!(
                    tire.omega * direction > 19.0 && tire.omega * direction < 20.0,
                    "modest brake must dissipate energy without an instant stop: {}",
                    tire.omega
                );
            }
        }
    }

    #[test]
    fn external_torque_can_overpower_brake_from_rest() {
        for load in [0.0, 4000.0] {
            for direction in [-1.0, 1.0] {
                let mut tire = Tire::new(0.31, 1.0);
                // The brake plus, on the ground, the road's traction torque at
                // the friction limit hold the wheel; drive must beat both.
                let drive = 1200.0 + load * tire.radius + 1000.0;
                tire.step(load, 0.0, 0.0, direction * drive, 1200.0, 1.0 / 240.0);
                assert!(
                    tire.omega * direction > 0.0,
                    "drive torque exceeding brake must release stationary wheel"
                );
                assert!(
                    tire.omega.abs() < drive / tire.inertia / 240.0,
                    "brake must still oppose the applied torque"
                );
            }
        }
    }

    #[test]
    fn traction_under_forward_acceleration() {
        let mut tire = Tire::new(0.31, 1.0);
        let normal_force = 4000.0; // ~400 kg wheel load
        tire.omega = 100.0; // spinning faster than vehicle ground speed
        let forces = tire.step(normal_force, 20.0, 0.0, 1000.0, 0.0, 0.01);

        // Positive forward drive force
        assert!(forces.x > 0.0);
        assert!(forces.x <= normal_force * 1.08 * 1.05); // capped by friction
        assert_eq!(forces.y, 0.0); // no lateral slip
    }

    #[test]
    fn moderate_drive_holds_with_small_slip() {
        // 40 % of the friction limit on a loaded wheel rolling at 20 m/s: the
        // wheel overspeed settles under 8 % (it was 20 % and more with the
        // soft longitudinal stiffness of 2).
        let mut tire = Tire::new(0.31, 1.0);
        let n = 4000.0;
        tire.omega = 20.0 / 0.31;
        let drive = 0.4 * n * 0.31;
        for _ in 0..480 {
            tire.step(n, 20.0, 0.0, drive, 0.0, 1.0 / 240.0);
        }
        assert!(tire.slip_ratio > 0.0 && tire.slip_ratio < 0.08, "{}", tire.slip_ratio);
        assert!((tire.longitudinal_force - 0.4 * n).abs() < 0.03 * n, "{}", tire.longitudinal_force);
    }

    #[test]
    fn friction_ellipse_limits_combined_forces() {
        let mut tire = Tire::new(0.31, 1.0);
        let normal_force = 3000.0;
        let max_traction = normal_force * 1.08;

        tire.omega = 150.0; // extreme slip
        let forces = tire.step(normal_force, 20.0, 15.0, 5000.0, 0.0, 0.01);

        let combined = (forces.x * forces.x + forces.y * forces.y).sqrt();
        assert!(combined <= max_traction * 1.02); // within friction budget
    }
}

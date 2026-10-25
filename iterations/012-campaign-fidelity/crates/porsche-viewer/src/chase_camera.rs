//! Spring-damped chase camera for the 6 DOF car.
//!
//! Prototype behaviour, not the original TailCam0: the original parameters are
//! compiled into the EXE (camera name table at 0x5d6470) and are not yet
//! recovered. The camera keeps a fixed distance behind the car but lags its
//! heading, leans toward the direction of travel during slides, filters
//! suspension motion out of its height and never rolls with the body.

use glam::Vec3;

const DISTANCE: f32 = 5.8; // m behind the car
const HEIGHT: f32 = 2.0; // m above the body origin
const TARGET_HEIGHT: f32 = 0.9;
const TARGET_LEAD: f32 = 1.5; // m ahead of the body origin
const HEADING_LAG_S: f32 = 0.30; // time constant for yaw following
const HEIGHT_LAG_S: f32 = 0.15; // time constant for vertical following
const VELOCITY_BLEND_MAX: f32 = 0.5; // share of travel direction at speed
const VELOCITY_BLEND_FROM_MPS: f32 = 3.0;
const VELOCITY_BLEND_FULL_MPS: f32 = 13.0;
const SNAP_DISTANCE: f32 = 15.0; // a larger jump is a reset/teleport

#[derive(Debug, Clone, Default)]
pub struct ChaseCamera {
    state: Option<State>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    body_pos: Vec3,
    heading: Vec3,
    eye_y: f32,
}

/// Camera placement for one frame.
#[derive(Debug, Clone, Copy)]
pub struct ChaseView {
    pub eye: Vec3,
    pub target: Vec3,
}

fn horizontal(v: Vec3) -> Option<Vec3> {
    let h = Vec3::new(v.x, 0.0, v.z);
    (h.length_squared() > 1e-6).then(|| h.normalize())
}

fn smoothing(dt: f32, lag: f32) -> f32 {
    1.0 - (-dt.max(0.0) / lag).exp()
}

impl ChaseCamera {
    /// Forget the smoothed state; the next update snaps behind the car.
    pub fn reset(&mut self) {
        self.state = None;
    }

    pub fn update(
        &mut self,
        dt: f32,
        body_pos: Vec3,
        body_forward: Vec3,
        velocity: Vec3,
    ) -> ChaseView {
        let forward = horizontal(body_forward).unwrap_or(Vec3::NEG_Z);
        let desired = Self::desired_heading(forward, velocity);
        let desired_eye_y = body_pos.y + HEIGHT;

        let state = match self.state {
            Some(s) if s.body_pos.distance(body_pos) <= SNAP_DISTANCE => {
                let k = smoothing(dt, HEADING_LAG_S);
                let heading = horizontal(s.heading + (desired - s.heading) * k).unwrap_or(desired);
                let eye_y = s.eye_y + (desired_eye_y - s.eye_y) * smoothing(dt, HEIGHT_LAG_S);
                State {
                    body_pos,
                    heading,
                    eye_y,
                }
            }
            _ => State {
                body_pos,
                heading: desired,
                eye_y: desired_eye_y,
            },
        };
        self.state = Some(state);

        let mut eye = body_pos - state.heading * DISTANCE;
        eye.y = state.eye_y;
        ChaseView {
            eye,
            target: body_pos + Vec3::Y * TARGET_HEIGHT + state.heading * TARGET_LEAD,
        }
    }

    /// Blend the body heading toward the travel direction so slides stay visible.
    fn desired_heading(forward: Vec3, velocity: Vec3) -> Vec3 {
        let Some(travel) = horizontal(velocity) else {
            return forward;
        };
        // Reversing or spinning backwards: stay behind the body.
        if travel.dot(forward) <= 0.0 {
            return forward;
        }
        let speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
        let ramp = ((speed - VELOCITY_BLEND_FROM_MPS)
            / (VELOCITY_BLEND_FULL_MPS - VELOCITY_BLEND_FROM_MPS))
            .clamp(0.0, 1.0);
        horizontal(forward.lerp(travel, VELOCITY_BLEND_MAX * ramp)).unwrap_or(forward)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn first_update_places_camera_directly_behind() {
        let mut cam = ChaseCamera::default();
        let view = cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, Vec3::ZERO);
        assert!((view.eye - Vec3::new(0.0, HEIGHT, DISTANCE)).length() < 1e-4);
    }

    #[test]
    fn heading_lags_then_converges_after_a_turn() {
        let mut cam = ChaseCamera::default();
        cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, Vec3::ZERO);
        // Body snaps to face +X; the camera must not follow in one frame.
        let first = cam.update(DT, Vec3::ZERO, Vec3::X, Vec3::ZERO);
        assert!(
            first.eye.z > 5.0,
            "camera followed the yaw instantly: {first:?}"
        );
        let mut view = first;
        for _ in 0..180 {
            view = cam.update(DT, Vec3::ZERO, Vec3::X, Vec3::ZERO);
        }
        assert!(
            (view.eye - Vec3::new(-DISTANCE, HEIGHT, 0.0)).length() < 0.05,
            "{view:?}"
        );
    }

    #[test]
    fn slide_keeps_camera_between_body_and_travel_direction() {
        let mut cam = ChaseCamera::default();
        // Body faces -Z while the car travels toward -Z/+X at 20 m/s.
        let velocity = Vec3::new(1.0, 0.0, -1.0).normalize() * 20.0;
        let mut view = cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, velocity);
        for _ in 0..180 {
            view = cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, velocity);
        }
        // Behind the travel blend means the eye is offset toward -X.
        assert!(view.eye.x < -1.0, "{view:?}");
        assert!((view.eye - Vec3::new(0.0, view.eye.y, 0.0)).length() > DISTANCE - 0.01);
    }

    #[test]
    fn height_filters_suspension_bumps() {
        let mut cam = ChaseCamera::default();
        cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, Vec3::ZERO);
        let view = cam.update(DT, Vec3::new(0.0, 0.2, 0.0), Vec3::NEG_Z, Vec3::ZERO);
        assert!(view.eye.y < HEIGHT + 0.05, "{view:?}");
    }

    #[test]
    fn large_jump_snaps_instead_of_swinging() {
        let mut cam = ChaseCamera::default();
        cam.update(DT, Vec3::ZERO, Vec3::NEG_Z, Vec3::ZERO);
        let view = cam.update(DT, Vec3::new(100.0, 5.0, 0.0), Vec3::X, Vec3::ZERO);
        assert!((view.eye - Vec3::new(100.0 - DISTANCE, 5.0 + HEIGHT, 0.0)).length() < 1e-4);
    }
}

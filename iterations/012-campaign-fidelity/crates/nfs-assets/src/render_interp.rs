//! Render interpolation between the fixed 64 Hz states (approximation A106).
//!
//! The original drew the latest 64 Hz state on every frame. At a display rate that is not a
//! multiple of 64 Hz the number of steps per frame varies (0, 1, 2) and the car and camera
//! judder. The simulation and the camera stay bit-exact; only the drawn pose is the blend of
//! the last two states by `alpha = leftover clock / step`.

use crate::camera::{CameraView, CarView};

/// Poses further apart than this (original units, metres) are a teleport (reset, camera
/// change) and are not blended.
pub const SNAP_DISTANCE: f32 = 8.0;

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        a.map(|x| x / l)
    } else {
        a
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot(d, d).sqrt()
}

/// Normalised lerp of three orthonormal axes (rows), re-orthonormalised against the third
/// row (forward) then the second, which keeps the handedness of the input.
pub fn blend_axes(a: [[f32; 3]; 3], b: [[f32; 3]; 3], t: f32) -> [[f32; 3]; 3] {
    let f = normalize(lerp3(a[2], b[2], t));
    let mut u = lerp3(a[1], b[1], t);
    let k = dot(u, f);
    u = normalize([u[0] - f[0] * k, u[1] - f[1] * k, u[2] - f[2] * k]);
    let mut r = lerp3(a[0], b[0], t);
    let (kf, ku) = (dot(r, f), dot(r, u));
    for i in 0..3 {
        r[i] -= f[i] * kf + u[i] * ku;
    }
    [normalize(r), u, f]
}

/// Position and axes (R, U, F rows) of the box centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarPose {
    pub position: [f32; 3],
    pub axes: [[f32; 3]; 3],
}

/// Blend of two car poses; `b` when they are a teleport apart.
pub fn blend_pose(a: &CarPose, b: &CarPose, t: f32) -> CarPose {
    if distance(a.position, b.position) > SNAP_DISTANCE {
        return *b;
    }
    CarPose {
        position: lerp3(a.position, b.position, t),
        axes: blend_axes(a.axes, b.axes, t),
    }
}

/// Car as the camera sees it, a fraction `t` between two frames' states (the car moves per
/// frame; the 64 Hz camera steps fall in between).
pub fn blend_car_view(a: &CarView, b: &CarView, t: f32) -> CarView {
    if distance(a.pos_330, b.pos_330) > SNAP_DISTANCE {
        return *b;
    }
    let l = |x: f32, y: f32| x + (y - x) * t;
    CarView {
        pos_330: lerp3(a.pos_330, b.pos_330, t),
        axes_364: blend_axes(a.axes_364, b.axes_364, t),
        lateral_speed_d58: l(a.lateral_speed_d58, b.lateral_speed_d58),
        forward_speed_d60: l(a.forward_speed_d60, b.forward_speed_d60),
        ..*b
    }
}

/// Blend of two camera views (eye, orientation rows, target); FOV and near from `b`.
pub fn blend_view(a: &CameraView, b: &CameraView, t: f32) -> CameraView {
    if distance(a.eye, b.eye) > SNAP_DISTANCE {
        return *b;
    }
    let matrix = blend_axes(a.matrix, b.matrix, t);
    let eye = lerp3(a.eye, b.eye, t);
    CameraView {
        eye,
        matrix,
        target: [eye[0] + matrix[2][0], eye[1] + matrix[2][1], eye[2] + matrix[2][2]],
        fov_deg: b.fov_deg,
        near: b.near,
    }
}

/// The previous and current 64 Hz states of the drawn player car and camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnStates {
    pub car: [CarPose; 2],
    pub view: [CameraView; 2],
    /// Fraction of the next step already elapsed, 0..1.
    pub alpha: f32,
}

impl DrawnStates {
    pub fn new(car: CarPose, view: CameraView) -> Self {
        Self {
            car: [car; 2],
            view: [view; 2],
            alpha: 0.0,
        }
    }

    /// A 64 Hz step ran: the current state becomes the previous one.
    pub fn push(&mut self, car: CarPose, view: CameraView) {
        self.car = [self.car[1], car];
        self.view = [self.view[1], view];
    }

    /// Drop the previous states (camera cycle, restart).
    pub fn snap(&mut self) {
        self.car[0] = self.car[1];
        self.view[0] = self.view[1];
    }

    pub fn car_pose(&self) -> CarPose {
        blend_pose(&self.car[0], &self.car[1], self.alpha)
    }

    pub fn camera_view(&self) -> CameraView {
        blend_view(&self.view[0], &self.view[1], self.alpha)
    }
}

/// Previous and current 64 Hz camera views alone (the prototype car integrates per frame,
/// only the camera steps at 64 Hz).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewStates {
    pub view: [CameraView; 2],
    pub alpha: f32,
}

impl ViewStates {
    pub fn new(view: CameraView) -> Self {
        Self {
            view: [view; 2],
            alpha: 0.0,
        }
    }

    pub fn push(&mut self, view: CameraView) {
        self.view = [self.view[1], view];
    }

    pub fn camera_view(&self) -> CameraView {
        blend_view(&self.view[0], &self.view[1], self.alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::original_vehicle::StepClock;

    const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    fn pose(x: f32) -> CarPose {
        CarPose {
            position: [x, 0.0, 0.0],
            axes: IDENTITY,
        }
    }

    fn view(x: f32) -> CameraView {
        CameraView {
            eye: [x, 1.0, 0.0],
            matrix: IDENTITY,
            target: [x, 1.0, 1.0],
            fov_deg: 75.0,
            near: 0.5,
        }
    }

    #[test]
    fn axes_blend_stays_orthonormal() {
        let (s, c) = 0.3f32.sin_cos();
        let b = [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]];
        let m = blend_axes(IDENTITY, b, 0.5);
        for i in 0..3 {
            assert!((dot(m[i], m[i]) - 1.0).abs() < 1e-5);
            for j in 0..i {
                assert!(dot(m[i], m[j]).abs() < 1e-5);
            }
        }
        assert!((m[2][0] - 0.15f32.sin()).abs() < 1e-3);
    }

    #[test]
    fn teleports_are_not_blended() {
        assert_eq!(blend_pose(&pose(0.0), &pose(100.0), 0.5), pose(100.0));
    }

    /// The drawn position advances almost uniformly per frame at any display rate while the
    /// simulation (here 16 m/s, 0.25 m per step) steps 0, 1 or 2 times per frame.
    #[test]
    fn drawn_position_is_continuous_across_frame_rates() {
        let step_m = 16.0 / 64.0;
        for fps in [60u32, 90, 120] {
            let mut clock = StepClock::default();
            let mut states = DrawnStates::new(pose(0.0), view(0.0));
            let (mut sim_x, mut steps_total) = (0.0f32, 0u32);
            let mut drawn = Vec::new();
            let (mut raw, mut raw_prev) = (Vec::new(), 0.0f32);
            for _ in 0..fps * 4 {
                for _ in 0..clock.advance(1.0 / fps as f32) {
                    sim_x += step_m;
                    steps_total += 1;
                    states.push(pose(sim_x), view(sim_x));
                }
                states.alpha = clock.fraction();
                drawn.push(states.car_pose().position[0]);
                raw.push(sim_x - raw_prev);
                raw_prev = sim_x;
                assert!((states.camera_view().eye[0] - drawn.last().unwrap()).abs() < 1e-4);
            }
            assert!(steps_total > 0);
            // Skip the first frames (one step of latency while the buffer fills).
            let d: Vec<f32> = drawn.windows(2).skip(3).map(|w| w[1] - w[0]).collect();
            let expect = 16.0 / fps as f32;
            let worst = d.iter().map(|x| (x - expect).abs()).fold(0.0, f32::max);
            assert!(worst < 0.01, "fps {fps}: worst deviation {worst} m");
            // The undrawn latest state steps in 0 or 0.5 m jumps at 60 and 120 Hz.
            let raw_spread = raw.iter().skip(3).cloned().fold(0.0, f32::max)
                - raw.iter().skip(3).cloned().fold(f32::MAX, f32::min);
            assert!(raw_spread >= step_m * 0.99, "fps {fps}: raw spread {raw_spread}");
        }
    }
}

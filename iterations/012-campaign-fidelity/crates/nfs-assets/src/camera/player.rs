//! Player camera driver: the 64 Hz clock, the per-step dispatch of `Camera_UpdateAll 0x463c30`
//! (one player; no crash camera, re-init, pause orbit or replay cameras), `Camera_Set
//! 0x465790`, `Camera_Cycle 0x465a60` (C key) with the snap `0x4618b0`, and the per-frame view
//! build (`Camera_BuildView 0x464f80`, view part).
//!
//! Notes: `docs/re/camera.md` of the research workspace.

use super::original::{
    add, build_view, clamped_height, transform, update_bumper, update_bumper2, update_heli,
    update_tail, CameraGlobals, CameraParam, CameraRecord, CameraView, CameraWorld, CarView,
    DriverCamera, LookKeys, CAMERA_PARAMS, UPDATE_HZ,
};

/// Player list at `0x658294` (`res+0x208`) as the front end writes it from Options → Kameras
/// `Camera.1..4` (`FE_WriteCarConfig` 0x4eeda0) with the defaults 0, 1, 2, 3
/// (`FEdaCameras_Init` 0x4e4d40): BumperCam, BumperCam2, TailCam0, TailCam1. The `fe.txt`
/// list (BumperCam2, BumperCam, TailCam0, HeliCam2) is overwritten by it. `Camera_Cycle`
/// picks `list[index & 3]`.
pub const CYCLE_LIST: [i16; 4] = [0, 1, 2, 3];
/// Camera id of the AnimCam race intro (`Camera_RaceInit 0x4640b0` starts in it).
pub const ANIM_CAM: i16 = 14;
/// Steps after a camera change with the fixed 0.1 lag (`+0xc1 = 0x32`).
pub const SETTLE_STEPS: i8 = 0x32;
/// Longest frame the clock catches up on (a stalled tab must not replay seconds of steps).
const MAX_FRAME_S: f64 = 0.25;

/// Name of a camera id (`g_CameraNameTable 0x5d6470`).
pub fn camera_name(id: i16) -> &'static str {
    match id {
        0 => "BumperCam",
        1 => "BumperCam2",
        2 => "TailCam0",
        3 => "TailCam1",
        4 => "TailCam2",
        5 => "HeliCam0",
        6 => "HeliCam1",
        7 => "HeliCam2",
        8 => "ShotgunCam2",
        9 => "ShotgunCam3",
        10 => "CircleCam",
        11 => "SplineCam",
        12 => "TVCam",
        13 => "BlimpCam",
        14 => "AnimCam",
        15 => "ReverseCam1",
        _ => "Camera",
    }
}

/// A track without node heights or ceilings: the TailCam/HeliCam slope term is 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatWorld;

impl CameraWorld for FlatWorld {
    fn node_step(&self, node: i32, delta: i32) -> i32 {
        node + delta
    }
    fn node_y(&self, _node: i32) -> f32 {
        0.0
    }
}

/// Camera record, parameter table and clock of the player.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerCamera {
    pub record: CameraRecord,
    /// `g_CameraParams` with row 1 = the car's `drivercampos` (race init).
    pub params: [CameraParam; 20],
    /// `+0xbe` cycle index.
    pub cycle_index: i16,
    pub globals: CameraGlobals,
    pub keys: LookKeys,
    /// Seconds not yet stepped.
    clock: f64,
    started: bool,
}

impl PlayerCamera {
    /// `Camera_RaceInit 0x4640b0`: row 1 and `+0x4c` from the car's cockpit camera, id AnimCam.
    /// The first [`advance`](Self::advance) sets `CYCLE_LIST[cycle_index & 3]`.
    pub fn new(driver: DriverCamera, cycle_index: i16) -> Self {
        let mut params = CAMERA_PARAMS;
        params[1].offset = driver.position;
        let record = CameraRecord {
            cockpit_rot_4c: driver.rotation_deg,
            id_bc: ANIM_CAM,
            ..CameraRecord::default()
        };
        Self {
            record,
            params,
            cycle_index,
            globals: CameraGlobals::default(),
            keys: LookKeys::default(),
            clock: 0.0,
            started: false,
        }
    }

    pub fn id(&self) -> i16 {
        self.record.id_bc
    }

    pub fn name(&self) -> &'static str {
        camera_name(self.record.id_bc)
    }

    /// The in-car views (`+0xc3` bit 3: id < 2).
    pub fn is_in_car(&self) -> bool {
        self.record.id_bc < 2
    }

    /// Restart from the race-init state (the next advance sets the camera again).
    pub fn restart(&mut self) {
        self.record.id_bc = ANIM_CAM;
        self.clock = 0.0;
        self.started = false;
    }

    /// Copies table flag bits 0..5 to record flag bits 1..6 (bits 0 and 7 are kept).
    fn load_flags(&mut self) {
        let table = (self.params[self.record.id_bc as usize].flags << 1) as u8;
        self.record.flags_c0 = (self.record.flags_c0 & 0x81) | (table & 0x7e);
    }

    fn leaves_chase(&self) -> bool {
        !(2..=7).contains(&self.record.id_bc)
    }

    /// `Camera_ResetChaseOffset 0x465cf0`: offset = -4 * F.
    fn reset_chase_offset(&mut self, car: &CarView) {
        let f = car.axes_364[2];
        self.record.offset_08 = [f[0] * -4.0, f[1] * -4.0, f[2] * -4.0];
    }

    /// `0x4618b0`: rigid offset `R * row` (height clamped under ceilings), eye, and the matrix
    /// car axes with R and F negated. Runs on a change to an id outside 2..7.
    fn snap(&mut self, car: &CarView, world: &mut impl CameraWorld) {
        let row = self.params[self.record.id_bc as usize].offset;
        let mut t = transform(row, &car.axes_364);
        t[1] = clamped_height(world, car.pos_330, self.record.eye_24, t[1]);
        self.record.eye_24 = add(t, car.pos_330);
        self.record.offset_08 = t;
        let [r, u, f] = car.axes_364;
        self.record.matrix_7c = [[-r[0], -r[1], -r[2]], u, [-f[0], -f[1], -f[2]]];
    }

    /// `0x4618b0(player)` called by `Car_ResetToTrack 0x40f540` after it moves the player car:
    /// the camera snaps to the car at the current id. Nothing to do before the intro ends.
    pub fn snap_to_car(&mut self, car: &CarView, world: &mut impl CameraWorld) {
        if self.started && self.record.id_bc != ANIM_CAM {
            self.snap(car, world);
        }
    }

    /// Common tail of `Camera_Set`/`Camera_Cycle` after the id is chosen.
    fn changed(&mut self, car: &CarView, world: &mut impl CameraWorld) {
        if self.record.id_bc > 20 {
            self.record.id_bc = CYCLE_LIST[0];
        }
        self.record.settle_c1 = SETTLE_STEPS;
        self.record.look_angle_c6 = 0;
        self.load_flags();
        if self.leaves_chase() {
            self.snap(car, world);
        }
    }

    /// `Camera_Set 0x465790(player, id)` outside replays.
    pub fn set(&mut self, id: i16, car: &CarView, world: &mut impl CameraWorld) {
        if self.leaves_chase() {
            self.reset_chase_offset(car);
        }
        self.record.id_bc = id;
        self.record.half_fov_58 = 37.5;
        self.record.near_5c = 0.5;
        self.changed(car, world);
    }

    /// `Camera_Cycle 0x465a60(player)` (C key), single screen: next id of [`CYCLE_LIST`].
    /// Ignored during the AnimCam intro or while a re-init is pending.
    pub fn cycle(&mut self, car: &CarView, world: &mut impl CameraWorld) {
        if self.record.id_bc == ANIM_CAM || self.record.flags_c0 & 1 != 0 {
            return;
        }
        if self.leaves_chase() {
            self.reset_chase_offset(car);
        }
        self.cycle_index = self.cycle_index.wrapping_add(1);
        self.record.id_bc = CYCLE_LIST[(self.cycle_index as u16 & 3) as usize];
        self.changed(car, world);
    }

    /// One 64 Hz step of `Camera_UpdateAll 0x463c30` for the player.
    pub fn step(&mut self, car: &CarView, world: &mut impl CameraWorld) {
        let (rec, g, keys, params) = (&mut self.record, &self.globals, &self.keys, &self.params);
        match rec.id_bc {
            0 => update_bumper(rec, car, g, keys, params),
            1 => update_bumper2(rec, car, g, keys, params),
            id @ 2..=4 => update_tail(rec, car, world, g, keys, params, (id - 2) as i32),
            id @ 5..=7 => update_heli(rec, car, world, g, keys, params, (id - 5) as i32),
            _ => {}
        }
    }

    /// Runs the steps due after `dt` seconds (the car state is that of the end of the frame,
    /// after the car update, as the camera runs after the car callbacks). The first call sets
    /// the camera of the cycle index (end of the race intro). Returns the number of steps.
    pub fn advance(&mut self, dt: f32, car: &CarView, world: &mut impl CameraWorld) -> u32 {
        if !self.started {
            self.started = true;
            self.set(
                CYCLE_LIST[(self.cycle_index as u16 & 3) as usize],
                car,
                world,
            );
        }
        let step = 1.0 / UPDATE_HZ as f64;
        self.clock += (dt as f64).clamp(0.0, MAX_FRAME_S);
        let mut steps = 0;
        while self.clock >= step {
            self.clock -= step;
            self.step(car, world);
            steps += 1;
        }
        steps
    }

    /// Per-frame view from the latest step (no interpolation), looking at `target` (the
    /// looked-at car's `+0x330`).
    pub fn view(&mut self, target: [f32; 3]) -> CameraView {
        build_view(&mut self.record, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(pos: [f32; 3]) -> CarView {
        CarView {
            node_08: 0,
            pos_330: pos,
            axes_364: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            half_extents_3a8: [0.9, 0.6, 2.1],
            direction_d30: 1,
            lateral_speed_d58: 0.0,
            forward_speed_d60: 20.0,
        }
    }

    #[test]
    fn steps_at_64_hz_independent_of_frame_rate() {
        let c = car([0.0; 3]);
        for (fps, frames) in [(60u32, 60u32), (30, 30), (144, 144), (64, 64)] {
            let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 2);
            let total: u32 = (0..frames)
                .map(|_| cam.advance(1.0 / fps as f32, &c, &mut FlatWorld))
                .sum();
            assert!((63..=64).contains(&total), "{fps} fps: {total} steps");
        }
        // A stalled frame catches up on at most 0.25 s.
        let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 2);
        assert_eq!(cam.advance(5.0, &c, &mut FlatWorld), 16);
    }

    #[test]
    fn first_advance_sets_the_camera_then_steps() {
        let c = car([0.0, 0.5, 0.0]);
        let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 2);
        assert_eq!(cam.id(), ANIM_CAM);
        // Less than one step: Camera_Set ran (offset -4 F from AnimCam), no update yet.
        assert_eq!(cam.advance(0.01, &c, &mut FlatWorld), 0);
        assert_eq!(cam.name(), "TailCam0");
        assert_eq!(cam.record.offset_08, [0.0, 0.0, -4.0]);
        assert_eq!(cam.record.settle_c1, SETTLE_STEPS);
        // The next step is exactly one Camera_UpdateTail on the same record.
        let mut direct = cam.record;
        update_tail(
            &mut direct,
            &c,
            &mut FlatWorld,
            &CameraGlobals::default(),
            &LookKeys::default(),
            &cam.params,
            0,
        );
        assert_eq!(cam.advance(0.01, &c, &mut FlatWorld), 1);
        assert_eq!(cam.record, direct);
        // The view reads the latest state without interpolation.
        let eye = cam.record.eye_24;
        assert_eq!(cam.advance(0.001, &c, &mut FlatWorld), 0);
        let view = cam.view(c.pos_330);
        assert_eq!(view.eye, eye);
        assert_eq!((view.fov_deg, view.near), (75.0, 0.5));
    }

    #[test]
    fn cycle_wraps_after_four_and_settles() {
        let c = car([0.0; 3]);
        let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 2);
        // No cycling during the intro.
        cam.cycle(&c, &mut FlatWorld);
        assert_eq!(cam.id(), ANIM_CAM);
        let settle = |cam: &mut PlayerCamera| {
            for _ in 0..4 {
                cam.advance(0.25, &c, &mut FlatWorld);
            }
        };
        settle(&mut cam);
        assert_eq!(cam.record.settle_c1, 0);
        let mut names = Vec::new();
        for _ in 0..4 {
            cam.cycle(&c, &mut FlatWorld);
            assert_eq!(cam.record.settle_c1, SETTLE_STEPS);
            assert_eq!(cam.record.look_angle_c6, 0);
            assert_eq!(
                cam.record.flags_c0 & 0x7e,
                (cam.params[cam.id() as usize].flags << 1) as u8 & 0x7e
            );
            names.push(cam.name());
            settle(&mut cam);
        }
        assert_eq!(names, ["TailCam1", "BumperCam", "BumperCam2", "TailCam0"]);
    }

    #[test]
    fn leaving_the_bumper_resets_the_chase_offset() {
        let c = car([0.0; 3]);
        let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 0);
        cam.advance(0.5, &c, &mut FlatWorld);
        assert_eq!(cam.name(), "BumperCam");
        // index 1 BumperCam2, 2 TailCam0: leaving BumperCam2 resets the offset to -4 F.
        cam.cycle(&c, &mut FlatWorld);
        cam.cycle(&c, &mut FlatWorld);
        assert_eq!(cam.name(), "TailCam0");
        assert_eq!(cam.record.offset_08, [0.0, 0.0, -4.0]);
    }
}

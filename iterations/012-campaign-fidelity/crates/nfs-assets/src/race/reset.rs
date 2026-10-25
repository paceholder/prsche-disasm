//! Car reset to the track (`docs/re/reset.md` in the RE workspace).
//!
//! - `Car_ResetWatch 0x40f740` (from `Car_ContactEffectsTick 0x410280`, 32 Hz): the reset key
//!   (event 5, `C1_RESET`) and the flipped/off-road timer `+0x1100`. Ported as [`ResetWatch`].
//! - `Car_ResetToTrack 0x40f540`: the car's track node `car+8`, else the next free node
//!   (`Car_ResetNodeFree 0x40f480`), then `Car_ResetDriveState 0x49e220` and
//!   `Car_PlaceOnNode 0x49b090`. The node choice is [`reset_node`], the pose [`reset_pose`].
//!
//! Lap and race state are not touched by the original reset, and no time penalty is applied.

use glam::{Mat3, Vec3};

use super::course::TrackCourse;

/// `+0xb4c = 0xa0` after a manual reset: 160 ticks (5 s at 32 Hz) before the next one.
pub const MANUAL_COOLDOWN_B4C: i32 = 0xa0;
/// A manual reset needs `+0x35c < 27.6` m/s.
pub const MANUAL_MAX_SPEED: f32 = 27.6;
/// A manual reset needs `g_SchedStep [0x606ac4] > 0x300`.
pub const MANUAL_MIN_SCHED_STEP: i32 = 0x300;
/// `+0x1100 = 0xc0`: the flipped/off-road timer, in 32 Hz ticks.
pub const WATCH_ARM_1100: i32 = 0xc0;

/// Which reset `0x40f540` runs (its second argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// 0: from the `+0x1100` timer; skipped when the car turns out to be fine.
    Conditional,
    /// 2: the reset key.
    Manual,
}

/// Car state read by one tick of [`ResetWatch`]. Field names carry the original offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatchInputs {
    /// Reset event this tick (`d8b == 5`, or `d89`).
    pub request: bool,
    /// `g_SchedStep [0x606ac4]`.
    pub sched_step: i32,
    /// `+0xb50 != 0`: finished or out of the race.
    pub out_b50: bool,
    /// `+0x35c`.
    pub speed_35c: f32,
    /// Node up `+0x3c4` · car up `+0x370`.
    pub node_up_dot: f32,
    /// Ground normal · car up `+0x3a0`.
    pub ground_up_3a0: f32,
    /// `+0xdb0 != 0`: controls not cut by a roll.
    pub controls_db0: bool,
    /// Track surface `+0x464` (0: none).
    pub surface_464: i32,
    /// Angular velocity `+0x388` (turns/s, x and z are compared signed).
    pub angular_388: [f32; 3],
    /// `+0x418`.
    pub wheels_off_418: u16,
    /// Height above the ground `+0x424`.
    pub height_424: f32,
    /// Locator byte `car+0x12` (`car+8` + 0xa); a reset needs it 0 or another failed check.
    pub locator_12: bool,
}

/// Timers of `0x40f740`: the manual cooldown `+0xb4c` and the watch timer `+0x1100`.
/// The stuck counter `+0x540` is left out: no writer sets it for a car (only `0x438440`
/// writes `+0x540`, of another object), so it never starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResetWatch {
    pub cooldown_b4c: i32,
    pub timer_1100: i32,
}

impl ResetWatch {
    /// Arms the watch timer as a hard hit does (`+0x1100 = 0xc0` when it is not running).
    pub fn arm(&mut self) {
        if self.timer_1100 < 1 {
            self.timer_1100 = WATCH_ARM_1100;
        }
    }

    /// One 32 Hz tick of `0x40f740` [VA 0x40f7db..0x40fbac]. Returns the reset to run now; the
    /// caller then moves the car (`0x40f540` also clears `+0x1100`, done here).
    pub fn tick(&mut self, i: &WatchInputs) -> Option<ResetMode> {
        if i.request
            && !i.out_b50
            && i.sched_step > MANUAL_MIN_SCHED_STEP
            && self.cooldown_b4c == 0
            && i.speed_35c < MANUAL_MAX_SPEED
        {
            self.cooldown_b4c = MANUAL_COOLDOWN_B4C - 1;
            self.timer_1100 = 0;
            return Some(ResetMode::Manual);
        }
        if self.cooldown_b4c > 0 {
            self.cooldown_b4c -= 1;
        }
        let t = self.timer_1100;
        if t > 0 {
            // −1 per tick, −9 without a track surface.
            self.timer_1100 = if i.surface_464 == 0 { t - 9 } else { t - 1 };
            if i.ground_up_3a0 <= 0.75 || !i.controls_db0 {
                if self.timer_1100 < 1 {
                    self.timer_1100 = 0;
                    if !Self::car_is_fine(i) {
                        return Some(ResetMode::Conditional);
                    }
                }
            } else {
                self.timer_1100 = 0;
            }
        }
        if self.timer_1100 < 1 && (i.node_up_dot < 0.1 || i.surface_464 == 0) {
            self.timer_1100 = WATCH_ARM_1100;
        }
        None
    }

    /// The early exit of `0x40f540` mode 0 [VA 0x40f5cd..0x40f63f]: upright on the node,
    /// not spinning, on a surface, low.
    fn car_is_fine(i: &WatchInputs) -> bool {
        i.node_up_dot > 0.75
            && i.angular_388[0] < 1.0
            && i.angular_388[2] < 1.0
            && i.surface_464 != 0
            && i.locator_12
            && i.wheels_off_418 < 6
            && i.height_424 < 1.0
    }
}

/// `TrackNodes_Step(node, +1, -1)` on the course: the next node, wrapping on a circuit and
/// stopping at the end of a point-to-point course.
pub fn next_node(course: &TrackCourse, node: usize) -> usize {
    let n = course.waypoints.len();
    if n == 0 {
        return 0;
    }
    if node + 1 < n {
        node + 1
    } else if course.is_circuit {
        0
    } else {
        n - 1
    }
}

/// `0x40f540` node choice: from `start`, the first node `free` accepts, stepping forward
/// (`0x657430` REVERSE steps backward; not supported here). `free` is `0x40f480`: a road
/// surface at the node and no other car on it. Gives up after one pass and returns `start`.
pub fn reset_node(
    course: &TrackCourse,
    start: usize,
    mut free: impl FnMut(usize) -> bool,
) -> usize {
    let mut node = start;
    for _ in 0..course.waypoints.len() {
        if free(node) {
            return node;
        }
        let next = next_node(course, node);
        if next == node {
            break;
        }
        node = next;
    }
    start
}

/// Pose of `Car_PlaceOnNode 0x49b090` for a player car: the node point, the node forward
/// made orthogonal to the support plane normal `up` (`lateral = up × forward`,
/// `forward = lateral × up`), speed 0. World axes: the returned matrix maps body space
/// (+X right, +Y up, −Z forward) to world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResetPose {
    pub node: usize,
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    /// Heading as `atan2(−f.x, −f.z)` (the grid's yaw convention).
    pub yaw: f32,
}

impl ResetPose {
    pub fn basis(&self) -> Mat3 {
        Mat3::from_cols(self.forward.cross(self.up), self.up, -self.forward)
    }
}

/// [`ResetPose`] at `node`, with the support normal `up` there (`+Y` if unknown) and the
/// ground height `height` (the node's own height if `None`).
pub fn reset_pose(
    course: &TrackCourse,
    node: usize,
    up: Option<Vec3>,
    height: Option<f32>,
) -> ResetPose {
    let wp = &course.waypoints[node.min(course.waypoints.len() - 1)];
    let mut position = Vec3::from(wp.position);
    if let Some(h) = height {
        position.y = h;
    }
    let up = up
        .map(|u| u.normalize_or_zero())
        .filter(|u| u.y > 0.05)
        .unwrap_or(Vec3::Y);
    let f = Vec3::from(wp.forward);
    let right = f.cross(up).normalize_or_zero();
    let forward = if right == Vec3::ZERO {
        Vec3::NEG_Z
    } else {
        up.cross(right).normalize()
    };
    ResetPose {
        node,
        position,
        forward,
        up,
        yaw: (-forward.x).atan2(-forward.z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_formats::SplinePoint;

    fn ring() -> TrackCourse {
        let pts: Vec<_> = (0..40)
            .map(|i| {
                let a = i as f32 / 40.0 * std::f32::consts::TAU;
                SplinePoint {
                    x: 100.0 * a.cos(),
                    z: 100.0 * a.sin(),
                }
            })
            .collect();
        TrackCourse::from_spline_points(&pts, true, |_, _| 0.0).unwrap()
    }

    fn upright() -> WatchInputs {
        WatchInputs {
            request: false,
            sched_step: 0x1000,
            out_b50: false,
            speed_35c: 0.0,
            node_up_dot: 1.0,
            ground_up_3a0: 1.0,
            controls_db0: true,
            surface_464: 1,
            angular_388: [0.0; 3],
            wheels_off_418: 0,
            height_424: 0.0,
            locator_12: true,
        }
    }

    #[test]
    fn manual_reset_gates_and_cooldown() {
        let mut w = ResetWatch::default();
        let req = WatchInputs {
            request: true,
            ..upright()
        };
        assert_eq!(w.tick(&req), Some(ResetMode::Manual));
        // 160 ticks of cooldown, counting the reset tick.
        for _ in 0..159 {
            assert_eq!(w.tick(&req), None);
        }
        assert_eq!(w.tick(&req), Some(ResetMode::Manual));
        let mut w = ResetWatch::default();
        for bad in [
            WatchInputs {
                speed_35c: 27.6,
                ..req
            },
            WatchInputs {
                sched_step: 0x300,
                ..req
            },
            WatchInputs {
                out_b50: true,
                ..req
            },
        ] {
            assert_eq!(w.tick(&bad), None);
        }
    }

    #[test]
    fn flipped_car_resets_after_192_ticks() {
        let mut w = ResetWatch::default();
        let roof = WatchInputs {
            node_up_dot: -1.0,
            ground_up_3a0: -1.0,
            controls_db0: false,
            ..upright()
        };
        // Arms on the first tick, then counts 192 ticks down.
        assert_eq!(w.tick(&roof), None);
        for _ in 0..191 {
            assert_eq!(w.tick(&roof), None);
        }
        assert_eq!(w.tick(&roof), Some(ResetMode::Conditional));
        // Righting the car before the end cancels it.
        let mut w = ResetWatch::default();
        w.tick(&roof);
        w.tick(&upright());
        assert_eq!(w.timer_1100, 0);
    }

    #[test]
    fn off_surface_tilted_counts_nine_per_tick_and_upright_never_fires() {
        let mut w = ResetWatch::default();
        let off = WatchInputs {
            surface_464: 0,
            ground_up_3a0: 0.5,
            ..upright()
        };
        let mut ticks = 0;
        while w.tick(&off).is_none() {
            ticks += 1;
            assert!(ticks < 100);
        }
        assert_eq!(ticks, 192 / 9 + 1);
        let mut w = ResetWatch::default();
        let flat_off = WatchInputs {
            surface_464: 0,
            ..upright()
        };
        for _ in 0..1000 {
            assert_eq!(w.tick(&flat_off), None);
        }
    }

    #[test]
    fn reset_node_skips_occupied_nodes_and_wraps() {
        let course = ring();
        let n = course.waypoints.len();
        assert_eq!(reset_node(&course, 5, |k| k != 5 && k != 6), 7);
        assert_eq!(reset_node(&course, n - 1, |k| k != n - 1), 0);
        assert_eq!(reset_node(&course, 3, |_| false), 3);
    }

    #[test]
    fn pose_faces_along_the_track_on_the_road_plane() {
        let course = ring();
        let up = Vec3::new(0.1, 1.0, 0.0).normalize();
        let p = reset_pose(&course, 10, Some(up), Some(2.0));
        assert_eq!(p.position.y, 2.0);
        assert!(p.forward.dot(up).abs() < 1e-5);
        assert!(p.forward.dot(Vec3::from(course.waypoints[10].forward)) > 0.99);
        let b = p.basis();
        assert!((b.y_axis - up).length() < 1e-5);
        assert!((b.z_axis + p.forward).length() < 1e-5);
    }
}

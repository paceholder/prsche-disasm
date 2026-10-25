//! Mid-race track reset (`Car_ResetToTrack 0x40f540`) with the original vehicle model: the
//! car drives part of a lap, R resets it, and the lap timer, the checkpoint progress and the
//! race clock carry on. The renderer composes the same pieces (`Renderer::reset_car_to_track`),
//! and like it, this test leaves the race session and the progress tracker alone.
//!
//! A flat ring course and alps, with the Boxster 2.5 `.sim`. Needs `local/game` (skipped
//! otherwise).

use std::path::{Path, PathBuf};

use glam::Vec3;
use nfs_assets::physics::original_adapter::{from_original, to_original};
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP;
use nfs_assets::physics::original_vehicle::ground::LEGACY_SURFACE;
use nfs_assets::physics::original_vehicle::{
    input_word, scheduler_step, FlatGround, GroundAdapter, GroundQuery, OriginalCar,
    OriginalVehicle, RoadGround, StepQueue,
};
use nfs_assets::race::reset::{reset_node, reset_pose, ResetMode, ResetWatch, WatchInputs};
use nfs_assets::{
    load_track, AssetFiles, CourseProgressTracker, RacePhase, RaceSession, RoadSurface, TrackCourse,
};
use nfs_formats::SplinePoint;

const HALF: [f32; 3] = [0.9, 0.6, 2.1];
const WHEELS: [[f32; 3]; 4] = [
    [-0.75, -0.6, 1.2],
    [0.75, -0.6, 1.2],
    [-0.75, -0.6, -1.2],
    [0.75, -0.6, -1.2],
];

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn sim_bytes() -> Option<Vec<u8>> {
    std::fs::read(game().join("Simulation/CarData/boxster25.sim")).ok()
}

fn vehicle() -> OriginalVehicle {
    OriginalVehicle::from_sim(&sim_bytes().unwrap(), "boxster", HALF, WHEELS).unwrap()
}

enum World<'a> {
    Flat,
    Road(&'a RoadSurface),
}

struct Queue<'a> {
    world: &'a World<'a>,
    ground: GroundAdapter,
    owners: [OriginalSupportOwner; 4],
}

impl Queue<'_> {
    fn with<R>(&mut self, f: impl FnOnce(&mut GroundAdapter, &mut dyn GroundQueryDyn) -> R) -> R {
        match self.world {
            World::Flat => f(
                &mut self.ground,
                &mut FlatGround {
                    height: 0.0,
                    surface: LEGACY_SURFACE,
                },
            ),
            World::Road(surface) => f(
                &mut self.ground,
                &mut RoadGround {
                    surface,
                    owners: &mut self.owners,
                },
            ),
        }
    }
}

/// Object-safe shim so one queue serves both grounds.
trait GroundQueryDyn {
    fn step(&mut self, adapter: &mut GroundAdapter, car: &mut OriginalCar, lcg: &mut OriginalLcg);
    fn place(&mut self, v: &mut OriginalVehicle, pos: [f32; 3], axes: [[f32; 3]; 3]);
}

impl<G: GroundQuery> GroundQueryDyn for G {
    fn step(&mut self, adapter: &mut GroundAdapter, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        adapter.ground_step(car, self, lcg);
    }
    fn place(&mut self, v: &mut OriginalVehicle, pos: [f32; 3], axes: [[f32; 3]; 3]) {
        v.set_pose(pos, axes, [0.0; 3], [0.0; 3], self);
    }
}

impl StepQueue for Queue<'_> {
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        self.with(|adapter, g| g.step(adapter, car, lcg));
    }
}

fn ring() -> TrackCourse {
    let pts: Vec<_> = (0..120)
        .map(|i| {
            let a = i as f32 / 120.0 * std::f32::consts::TAU;
            SplinePoint {
                x: 150.0 * a.cos(),
                z: 150.0 * a.sin(),
            }
        })
        .collect();
    TrackCourse::from_spline_points(&pts, true, |_, _| 0.0).unwrap()
}

fn track(name: &str) -> Option<nfs_assets::Scene> {
    let dir = game().join("Track");
    let mut files = AssetFiles::new();
    for ext in ["crp", "fsh", "edg", "jnc", "map"] {
        if let Ok(d) = std::fs::read(dir.join(format!("{name}.{ext}"))) {
            files.insert(format!("{name}.{ext}"), d);
        }
    }
    files.insert(
        "animdefs.txt".into(),
        std::fs::read(dir.join("animdefs.txt")).ok()?,
    );
    for lsp in [format!("racer/work/{name}0.lsp"), format!("{name}0.lsp")] {
        if let Ok(d) = std::fs::read(dir.join(&lsp)) {
            files.insert(format!("{name}0.lsp"), d);
            break;
        }
    }
    load_track(&files, name).ok()
}

/// Axes R, U, F (original space) for a scene forward and up.
fn axes(forward: Vec3, up: Vec3) -> [[f32; 3]; 3] {
    let right = forward.cross(up).normalize();
    let f = up.cross(right).normalize();
    [to_original(right), to_original(up), to_original(f)]
}

fn ground_at(world: &World, p: [f32; 3]) -> Option<(f32, Vec3)> {
    match world {
        World::Flat => Some((0.0, Vec3::Y)),
        World::Road(s) => s
            .query(p[0], p[2], p[1], 25.0, 50.0)
            .map(|h| (h.height, Vec3::from(h.normal))),
    }
}

fn drive_reset_and_continue(name: &str, course: &TrackCourse, world: &World) {
    let mut v = vehicle();
    let mut q = Queue {
        world,
        ground: GroundAdapter::default(),
        owners: Default::default(),
    };
    // Start a little past gate 0 so the start line is behind.
    let start_node = 3;
    let (h, up) = ground_at(world, course.waypoints[start_node].position).unwrap();
    let start = reset_pose(course, start_node, Some(up), Some(h));
    q.with(|_, g| {
        g.place(
            &mut v,
            to_original(start.position + start.up * 1.0),
            axes(start.forward, start.up),
        )
    });
    q.ground = v.ground.clone();

    let mut session = RaceSession::new(name, 3, !course.is_circuit);
    session.add_participant(0, "Player", true, "Boxster");
    session.phase = RacePhase::Racing { elapsed_secs: 0.0 };
    let mut tracker = CourseProgressTracker::new(start.position.to_array());
    let mut watch = ResetWatch::default();
    let dt = 1.0 / 64.0;

    let step = |v: &mut OriginalVehicle,
                q: &mut Queue,
                session: &mut RaceSession,
                tracker: &mut CourseProgressTracker,
                gas: f32| {
        let p = from_original(v.car.position_330);
        let (node, _) = course.find_closest_waypoint(p.to_array());
        let ahead = course.waypoints[(node + 3) % course.waypoints.len()].position;
        let to = (Vec3::from(ahead) - p).normalize_or_zero();
        let steer = (to.dot(from_original(v.car.axes_364[0])) * 3.0).clamp(-1.0, 1.0);
        let (gas, brake) = if gas > 0.0 && v.car.vel_fwd_d60() < 35.0 {
            (gas, 0.0)
        } else if gas > 0.0 || v.car.vel_fwd_d60() < 15.0 {
            (0.0, 0.0)
        } else {
            (0.0, 1.0)
        };
        scheduler_step(
            &mut v.car,
            input_word(gas, brake, steer, false),
            &mut v.lcg,
            q,
        );
        session.step(dt);
        let p = from_original(v.car.position_330);
        let f = from_original(v.car.axes_364[2]);
        if tracker.update(p.to_array(), f.to_array(), course, dt) == Some(true) {
            session.on_player_lap_completed();
        }
    };

    // 40 s at the wheel (the alps gates are about 780 m apart), then 3 s braking to below
    // the 27.6 m/s gate.
    for _ in 0..64 * 40 {
        step(&mut v, &mut q, &mut session, &mut tracker, 1.0);
    }
    for _ in 0..64 * 3 {
        step(&mut v, &mut q, &mut session, &mut tracker, 0.0);
    }
    let before_pos = from_original(v.car.position_330);
    let before_lap_time = session.lap_tracker.current_lap_time;
    let before_lap = session.lap_tracker.current_lap;
    let before_checkpoint = tracker.next_checkpoint;
    let before_visited = tracker.checkpoints_visited_this_lap;
    let before_race_time = session.total_race_time;
    assert!(
        before_race_time > 42.0,
        "{name}: race clock {before_race_time}"
    );
    assert!(before_visited > 0, "{name}: no checkpoint passed in 43 s");

    // R: one 32 Hz tick of the watch with the event.
    let (_, up) = ground_at(world, before_pos.to_array()).unwrap();
    let inputs = WatchInputs {
        request: true,
        sched_step: START_SCHED_STEP + 64 * 43,
        out_b50: false,
        speed_35c: v.car.speed_35c,
        node_up_dot: up.dot(from_original(v.car.axes_364[1])),
        ground_up_3a0: v.car.ground_up_3a0,
        controls_db0: v.car.start_db0 != 0,
        surface_464: 1,
        angular_388: v.car.angular_388,
        wheels_off_418: v.car.wheels_off_418,
        height_424: v.car.height_424,
        locator_12: true,
    };
    assert_eq!(
        watch.tick(&inputs),
        Some(ResetMode::Manual),
        "{name}: speed {}",
        v.car.speed_35c
    );
    let (here, _) = course.find_closest_waypoint(before_pos.to_array());
    let node = reset_node(course, here, |k| {
        ground_at(world, course.waypoints[k].position).is_some()
    });
    let (h, up) = ground_at(world, course.waypoints[node].position).unwrap();
    let pose = reset_pose(course, node, Some(up), Some(h));
    v.reset_drive_state();
    let sched = v.car.globals.sched_step_606ac4;
    q.with(|_, g| {
        g.place(
            &mut v,
            to_original(pose.position + pose.up * 1.0),
            axes(pose.forward, pose.up),
        )
    });
    q.ground = v.ground.clone();
    q.owners = Default::default();

    // On the road near the old position, at rest, facing along the track.
    let p = from_original(v.car.position_330);
    assert!(
        p.distance(before_pos) < 15.0,
        "{name}: moved {}",
        p.distance(before_pos)
    );
    assert!(v.car.velocity_33c.iter().all(|x| *x == 0.0));
    assert!(v.car.speed_35c == 0.0);
    assert!(
        v.car.height_424.abs() < 0.05,
        "{name}: height {}",
        v.car.height_424
    );
    let f = from_original(v.car.axes_364[2]);
    assert!(f.dot(Vec3::from(course.waypoints[node].forward)) > 0.95);
    assert_eq!(
        v.car.globals.sched_step_606ac4, sched,
        "the scheduler clock goes on"
    );
    // A reset mid-race keeps the race: lap, timer and checkpoints are untouched.
    assert_eq!(session.lap_tracker.current_lap, before_lap);
    assert_eq!(session.lap_tracker.current_lap_time, before_lap_time);
    assert_eq!(session.total_race_time, before_race_time);
    assert_eq!(tracker.next_checkpoint, before_checkpoint);
    assert_eq!(tracker.checkpoints_visited_this_lap, before_visited);

    // The race goes on from here: the timer keeps counting and progress is not lost.
    for _ in 0..64 * 4 {
        step(&mut v, &mut q, &mut session, &mut tracker, 1.0);
    }
    assert!((session.total_race_time - before_race_time - 4.0).abs() < 0.01);
    if session.lap_tracker.current_lap == before_lap {
        assert!(session.lap_tracker.current_lap_time > before_lap_time + 3.9);
        assert!(tracker.checkpoints_visited_this_lap >= before_visited);
    } else {
        assert_eq!(session.lap_tracker.current_lap, before_lap + 1, "{name}");
    }
    let travelled = from_original(v.car.position_330).distance(p);
    eprintln!(
        "{name}: lap {before_lap} at {before_lap_time:.1} s, gate {before_checkpoint}; reset at node {node} ({:.1} m from the car), then {travelled:.0} m in 4 s",
        p.distance(before_pos)
    );
    assert!(travelled > 20.0, "{name}: travelled {travelled}");
}

#[test]
fn reset_mid_race_keeps_progress_on_a_ring() {
    if sim_bytes().is_none() {
        eprintln!("skipped: no local/game");
        return;
    }
    drive_reset_and_continue("ring", &ring(), &World::Flat);
}

#[test]
fn reset_mid_race_keeps_progress_on_alps() {
    if sim_bytes().is_none() {
        eprintln!("skipped: no local/game");
        return;
    }
    let Some(scene) = track("alps") else {
        eprintln!("skipped: no alps");
        return;
    };
    let surface = scene.road_surface.as_ref().unwrap();
    let course = scene.course.as_ref().unwrap();
    drive_reset_and_continue("alps", course, &World::Road(surface));
}

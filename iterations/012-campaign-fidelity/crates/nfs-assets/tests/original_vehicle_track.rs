//! Task 6.2 smoke test on real tracks: the original vehicle model driven by
//! the fixed-step scheduler with the ground-contact adapter on the original
//! support tree. A Boxster 2.5 starts on grid slot 0, drives at full throttle
//! with a simple steer toward the course waypoints, and must stay on the
//! road: finite state, wheels on the ground, the body at the road height.
//! The 32 Hz queue runs `Car_TrackLocate` 0x49af70 on the track node graph,
//! as the renderer does: without it the alternate plane `+0x3e8/+0x400` keeps
//! the spawn placement's plane, and a tilted contact (`0x496610`, which pushes
//! the car out of that plane) after a hard wall hit lifted the car 4.9 m on
//! monaco1.
//!
//! Needs `local/game` (skipped otherwise).

use std::path::{Path, PathBuf};

use glam::Vec3;
use nfs_assets::physics::original_adapter::{from_original, to_original};
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_vehicle::driver::collide_player;
use nfs_assets::physics::original_vehicle::{
    input_word, scheduler_step, track_locate, BodyLocator, OriginalCar, OriginalVehicle,
    RoadGround, StepQueue,
};
use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes};
use nfs_assets::{calculate_grid_slot, load_track, AssetFiles, RoadSurface};

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn track(name: &str) -> Option<nfs_assets::Scene> {
    let dir = game().join("Track");
    let mut files = AssetFiles::new();
    for ext in ["crp", "fsh", "edg", "jnc", "map"] {
        let p = dir.join(format!("{name}.{ext}"));
        if let Ok(d) = std::fs::read(&p) {
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

/// The track node graph (`TrackNodes_Build` on the CRP's SimD/SimT).
fn nodes(name: &str) -> Option<TrackNodes> {
    let raw = std::fs::read(game().join("Track").join(format!("{name}.crp"))).ok()?;
    let unpacked = nfs_formats::decompress(&raw).ok()?;
    let (nodes, sections) = sim_chunks(&unpacked)?;
    Some(TrackNodes::build(nodes, &sections, None))
}

const HALF: [f32; 3] = [0.9, 0.6, 2.1];
/// FL, FR, RL, RR (the quadrant order of `0x49abc0`).
const WHEELS: [[f32; 3]; 4] = [
    [-0.75, -0.6, 1.2],
    [0.75, -0.6, 1.2],
    [-0.75, -0.6, -1.2],
    [0.75, -0.6, -1.2],
];

struct Queue<'a> {
    surface: &'a RoadSurface,
    nodes: &'a TrackNodes,
    /// `car+8`: track locator and body support owner (also the barrier pass).
    locator: BodyLocator,
    ground: nfs_assets::physics::original_vehicle::GroundAdapter,
    owners: [OriginalSupportOwner; 4],
    globals: nfs_assets::physics::original_pair::PairGlobals,
    walls: usize,
}

impl StepQueue for Queue<'_> {
    fn track_locate(&mut self, car: &mut OriginalCar) {
        let me = car.position_330;
        track_locate(car, &mut self.locator, self.nodes, self.surface.original_arena(), &[me]);
    }
    fn ground(
        &mut self,
        car: &mut OriginalCar,
        lcg: &mut nfs_assets::physics::original_lcg::OriginalLcg,
    ) {
        self.ground.ground_step(
            car,
            &mut RoadGround {
                surface: self.surface,
                owners: &mut self.owners,
            },
            lcg,
        );
    }
    fn collide(
        &mut self,
        car: &mut OriginalCar,
        lcg: &mut nfs_assets::physics::original_lcg::OriginalLcg,
    ) {
        let (surface, owner) = (self.surface, &mut self.locator.owner);
        let mut hit = false;
        collide_player(car, HALF, &mut [], &mut self.globals, lcg, |state| {
            let h = surface
                .original_barrier_contact(state, HALF, owner)
                .ok()
                .flatten()
                .map(Into::into);
            hit |= h.is_some();
            h
        });
        self.walls += hit as usize;
    }
}

#[test]
fn boxster_drives_on_real_tracks() {
    let Ok(sim) = std::fs::read(game().join("Simulation/CarData/boxster25.sim")) else {
        eprintln!("skipped: no local/game");
        return;
    };
    for name in ["alps", "autobahn", "monaco1", "canyon"] {
        let Some(scene) = track(name) else {
            eprintln!("skipped {name}");
            continue;
        };
        let surface = scene.road_surface.as_ref().unwrap();
        let Some(track_nodes) = nodes(name) else {
            eprintln!("skipped {name}: no node graph");
            continue;
        };
        let course = scene.course.as_ref().unwrap();
        // As `Renderer::reset_car`: the grid slot, else 5 m past gate 0, else gate 0.
        let (mut pos, fwd, _) = calculate_grid_slot(0, course);
        let gate = &course.checkpoints[0];
        let ahead = [
            gate.center[0] + gate.forward[0] * 5.0,
            gate.center[1],
            gate.center[2] + gate.forward[2] * 5.0,
        ];
        for p in [pos, ahead, gate.center] {
            if let Some(h) = surface.query(p[0], p[2], p[1], 50.0, 100.0) {
                pos = [p[0], h.height, p[2]];
                break;
            }
        }
        let f = Vec3::new(fwd[0], 0.0, fwd[2]).normalize();
        let axes = {
            let f = to_original(f);
            let u = [0.0, 1.0, 0.0];
            let r = Vec3::from(u).cross(Vec3::from(f)).to_array();
            [r, u, f]
        };
        let mut v = OriginalVehicle::from_sim(&sim, "boxster", HALF, WHEELS).unwrap();
        let mut owners = Default::default();
        let start = [pos[0], pos[1] + 1.0, -pos[2]];
        v.set_pose(
            start,
            axes,
            [0.0; 3],
            [0.0; 3],
            &mut RoadGround {
                surface,
                owners: &mut owners,
            },
        );
        let mut q = Queue {
            surface,
            ground: v.ground.clone(),
            owners,
            nodes: &track_nodes,
            locator: Default::default(),
            globals: Default::default(),
            walls: 0,
        };
        let start_scene = from_original(v.car.position_330);
        assert_eq!(
            v.car.ground_up_3a0,
            v.car.ground_up_3a0.clamp(0.99, 1.0),
            "{name}"
        );
        let mut next = 1;
        let mut travelled = 0.0;
        let mut last = start_scene;
        let mut off_steps = 0;
        // The 7 s start: the original gating holds the car on the grid.
        let grid = v.car.position_330;
        for _ in 0..nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP {
            scheduler_step(&mut v.car, input_word(1.0, 0.0, 0.0, false), &mut v.lcg, &mut q);
        }
        let held = (0..3).map(|k| (v.car.position_330[k] - grid[k]).powi(2)).sum::<f32>().sqrt();
        assert!(held < 0.05, "{name}: moved {held} m before the green light");
        let steps = 64 * 25;
        for n in 0..steps {
            let c = &v.car;
            let p = from_original(c.position_330);
            // Next waypoint at least 15 m ahead.
            while next < course.waypoints.len()
                && Vec3::from(course.waypoints[next].position).distance(p) < 15.0
            {
                next += 1;
            }
            let target = course.waypoints[next.min(course.waypoints.len() - 1)].position;
            let to = (Vec3::from(target) - p).normalize_or_zero();
            let right = from_original(c.axes_364[0]);
            let steer = (to.dot(right) * 3.0).clamp(-1.0, 1.0);
            let gas = if c.vel_fwd_d60() > 35.0 { 0.0 } else { 1.0 };
            let word = input_word(gas, 0.0, steer, false);
            scheduler_step(&mut v.car, word, &mut v.lcg, &mut q);
            let c = &v.car;
            for x in c
                .position_330
                .iter()
                .chain(&c.velocity_33c)
                .chain(&c.angular_388)
            {
                assert!(x.is_finite(), "{name} step {n}: {:?}", c.position_330);
            }
            if c.wheels.iter().filter(|w| w.airborne_84 != 0).count() > 1 {
                off_steps += 1;
            }
            if n % 64 == 0 {
                let p = from_original(c.position_330);
                travelled += p.distance(last);
                last = p;
                let hit = surface.query(p.x, p.z, p.y, 3.0, 3.0).map(|h| h.height);
                assert!(
                    hit.is_some_and(|h| (p.y - h - 0.6).abs() < 0.5),
                    "{name} step {n}: body {p:?} road {:?}",
                    hit
                );
            }
        }
        eprintln!(
            "{name}: {travelled:.0} m, speed {:.1} m/s, gear {}, airborne steps {off_steps}, wall contacts {}",
            v.car.speed_35c, v.car.gear_d82, q.walls
        );
        assert!(travelled > 200.0, "{name}: travelled {travelled}");
        assert!(off_steps < steps / 10, "{name}: {off_steps} steps airborne");
    }
}

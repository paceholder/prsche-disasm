//! Regression: the prototype (6 DOF) car must not spin its rear wheels in an
//! ordinary turn on tarmac. The user report: turning at speed made the car
//! skid at once, and the automatic box then stayed at the rev limiter
//! because the wheels ran far above road speed (engine rpm > 1.2 x road rpm
//! makes the box read road rpm, so it never upshifts).
//!
//! Two causes, both fixed in `physics/tire.rs` and `physics/vehicle.rs`
//! (approximations A113, A114): the tyre needed 20-40 % slip for ordinary
//! drive force, and the drive torque was split equally so the unloaded inside
//! wheel of a corner spun up.
//!
//! Needs `local/game` (skipped otherwise).

use std::path::{Path, PathBuf};

use glam::Vec3;
use nfs_assets::physics::VehicleSimulation;
use nfs_assets::{calculate_grid_slot, load_track, AssetFiles};

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn track(name: &str) -> Option<nfs_assets::Scene> {
    let dir = game().join("Track");
    let mut files = AssetFiles::new();
    for ext in ["crp", "fsh", "edg", "jnc", "map"] {
        if let Ok(d) = std::fs::read(dir.join(format!("{name}.{ext}"))) {
            files.insert(format!("{name}.{ext}"), d);
        }
    }
    files.insert("animdefs.txt".into(), std::fs::read(dir.join("animdefs.txt")).ok()?);
    for lsp in [format!("racer/work/{name}0.lsp"), format!("{name}0.lsp")] {
        if let Ok(d) = std::fs::read(dir.join(&lsp)) {
            files.insert(format!("{name}0.lsp"), d);
            break;
        }
    }
    load_track(&files, name).ok()
}

/// First waypoint of the straightest 320 m of the course (heading change under
/// 4 degrees, road under the middle waypoint), so a stand-start run reaches
/// 4th gear before the first bend.
fn straightest(course: &nfs_assets::TrackCourse, surface: &nfs_assets::RoadSurface) -> usize {
    let wps = &course.waypoints;
    for i in 0..wps.len() - 1 {
        let a = Vec3::from(wps[i].position);
        let d0 = (Vec3::from(wps[i + 1].position) - a).normalize_or_zero();
        let h0 = match surface.query(a.x, a.z, a.y, 5.0, 5.0) { Some(h) => h.height, None => continue };
        // Road under every 4 m of the next 320 m along the start heading,
        // within 0.6 m of the start height, and lane-wide (+-1.5 m).
        let right = d0.cross(Vec3::Y);
        let ok = (0..=80).all(|k| {
            [-1.5f32, 0.0, 1.5].iter().all(|off| {
                let q = a + d0 * (k as f32 * 4.0) + right * *off;
                surface.query(q.x, q.z, h0, 1.0, 1.0).is_some_and(|h| (h.height - h0).abs() < 0.6)
            })
        });
        if ok {
            return i;
        }
    }
    panic!("no straight stretch");
}

/// Engine rpm over the rpm the driven wheels would have at road speed.
fn rpm_ratio(v: &VehicleSimulation, speed: f32) -> f32 {
    let pt = &v.powertrain;
    let r = v.suspension.wheels[2].tire.radius;
    let road = speed.max(0.0) / r * 60.0 / std::f32::consts::TAU
        * pt.gear_ratios[(pt.current_gear.max(1) - 1) as usize]
        * pt.final_drive;
    pt.current_rpm / road.max(1.0)
}

/// Full throttle, waypoint steering to `turn_gear`, then `steer` held for
/// `turn_s` seconds. Returns (max rpm ratio during the turn, gear at the end
/// of the turn, speed at the turn start), or None without game files.
fn run(simname: &str, steer: f32, turn_gear: i32, turn_s: f32) -> Option<(f32, i32, f32)> {
    let bytes = std::fs::read(game().join(format!("Simulation/CarData/{simname}.sim"))).ok()?;
    let sim = nfs_formats::sim::parse_sim(&bytes).unwrap();
    let scene = track("autobahn")?;
    let surface = scene.road_surface.as_ref().unwrap();
    let course = scene.course.as_ref().unwrap();
    // Start on the long straight in the second half of the lap (waypoints
    // around the point (-170, -17) heading +x), standing.
    let start = straightest(course, surface);
    let w0 = Vec3::from(course.waypoints[start].position);
    let w1 = Vec3::from(course.waypoints[start + 1].position);
    let h = surface.query(w0.x, w0.z, w0.y, 50.0, 100.0)?.height;
    let dir = (w1 - w0).normalize();
    let mut v = VehicleSimulation::from_sim(&sim);
    v.reset(
        Vec3::new(w0.x, h, w0.z) + Vec3::Y * v.resting_height(),
        (-dir.x).atan2(-dir.z),
    );
    let dt = 1.0 / 60.0;
    let (mut turn_start, mut worst, mut start_speed) = (None, 0.0f32, 0.0);
    for n in 0..60 * 40 {
        // Hold the start heading (the stretch is straight).
        let auto = (dir.dot(v.body.forward().cross(Vec3::Y)) * 6.0).clamp(-1.0, 1.0);
        if turn_start.is_none()
            && v.powertrain.current_gear >= turn_gear
            && v.body.forward_speed() > 15.0
            && n > 60 * 6
        {
            turn_start = Some(n);
            start_speed = v.body.forward_speed();
        }
        let s = match turn_start {
            Some(t) if n < t + (turn_s * 60.0) as usize => steer,
            _ => auto,
        };
        let controls = v.controls_from_signed_pedal(1.0, s, false);
        let tel = v.step(Some(surface), &controls, dt);
        if let Some(t) = turn_start {
            if n >= t + 30 && n < t + (turn_s * 60.0) as usize && tel.wheels_in_contact.iter().all(|c| *c) {
                worst = worst.max(rpm_ratio(&v, tel.speed_mps));
            }
        }
        if let Some(t) = turn_start {
            if n == t + (turn_s * 60.0) as usize {
                return Some((worst, tel.current_gear, start_speed));
            }
        }
    }
    Some((worst, v.powertrain.current_gear, start_speed))
}

#[test]
fn moderate_turn_on_tarmac_does_not_spin_the_rear() {
    for simname in ["boxster25", "993coupe436"] {
        // 4th gear is reached on the straight (the box is not held back by
        // wheel overspeed) ...
        let Some((straight_ratio, gear, speed)) = run(simname, 0.0, 4, 0.5) else {
            eprintln!("skipped: no local/game");
            return;
        };
        assert!(gear >= 4 && speed > 15.0, "{simname}: only gear {gear} at {speed} m/s");
        assert!(straight_ratio < 1.2, "{simname}: straight ratio {straight_ratio}");
        // ... and a moderate turn in 3rd and 4th keeps the engine below 1.2 x road rpm.
        // (0.3 and the full keyboard lock 1.0).
        for (gear, steer) in [(3, 0.3), (4, 0.3), (3, 1.0), (4, 1.0)] {
            let (ratio, _, _) = run(simname, steer, gear, 1.5).unwrap();
            eprintln!("{simname} turn in gear {gear}: max engine/road rpm {ratio:.2}");
            assert!(ratio < 1.2, "{simname}: rear spin in a moderate turn, ratio {ratio} (gear {gear}, steer {steer})");
        }
    }
}

/// Coast-down log: full gas to 27 m/s, then release. Prints gear, engine rpm,
/// road rpm and rear load; run with `COAST=1 --nocapture`.
#[test]
fn coast_log() {
    if std::env::var("COAST").is_err() { return; }
    let Ok(bytes) = std::fs::read(game().join("Simulation/CarData/993coupe436.sim")) else { return };
    let sim = nfs_formats::sim::parse_sim(&bytes).unwrap();
    let scene = track("autobahn").unwrap();
    let surface = scene.road_surface.as_ref().unwrap();
    let course = scene.course.as_ref().unwrap();
    let (mut pos, fwd, _) = calculate_grid_slot(0, course);
    let gate = &course.checkpoints[0];
    let ahead = [gate.center[0] + gate.forward[0] * 5.0, gate.center[1], gate.center[2] + gate.forward[2] * 5.0];
    for p in [pos, ahead, gate.center] {
        if let Some(h) = surface.query(p[0], p[2], p[1], 50.0, 100.0) { pos = [p[0], h.height, p[2]]; break; }
    }
    let mut v = VehicleSimulation::from_sim(&sim);
    v.reset(Vec3::from(pos) + Vec3::Y * v.resting_height(), (-fwd[0]).atan2(-fwd[2]));
    let dt = 1.0 / 60.0;
    let (mut next, mut lift) = (1, None);
    for n in 0..60 * 20 {
        let p = v.body.position;
        while next < course.waypoints.len() && Vec3::from(course.waypoints[next].position).distance(p) < 15.0 { next += 1; }
        let target = course.waypoints[next.min(course.waypoints.len() - 1)].position;
        let to = (Vec3::from(target) - p).normalize_or_zero();
        let auto = (to.dot(v.body.forward().cross(Vec3::Y)) * 3.0).clamp(-1.0, 1.0);
        if lift.is_none() && v.body.forward_speed() > 27.0 { lift = Some(n); }
        let pedal = if lift.is_some() { 0.0 } else { 1.0 };
        let controls = v.controls_from_signed_pedal(pedal, auto, false);
        let tel = v.step(Some(surface), &controls, dt);
        if n % 15 == 0 {
            let r = v.suspension.wheels[2].tire.radius;
            let pt = &v.powertrain;
            let g = pt.current_gear.max(1) as usize - 1;
            let road = tel.speed_mps.max(0.0) / r * 60.0 / std::f32::consts::TAU * pt.gear_ratios[g] * pt.final_drive;
            let wheel = tel.wheel_omegas[2].abs() * 60.0 / std::f32::consts::TAU * pt.gear_ratios[g] * pt.final_drive;
            eprintln!("COAST t={:.2} gas={} v={:.1} g={} rpm={:.0} road_rpm={:.0} rearwheel_rpm={:.0} rear N=[{:.0} {:.0}]", n as f32 * dt, pedal, tel.speed_mps, tel.current_gear, tel.engine_rpm, road, wheel, v.suspension.wheels[2].tire.normal_load, v.suspension.wheels[3].tire.normal_load);
        }
    }
}

/// Launch log: standstill, partial gas, straight. `LAUNCH=0.3 --nocapture`.
#[test]
fn launch_log() {
    let Ok(g) = std::env::var("LAUNCH") else { return };
    let gas: f32 = g.parse().unwrap();
    let Ok(bytes) = std::fs::read(game().join("Simulation/CarData/boxster25.sim")) else { return };
    let sim = nfs_formats::sim::parse_sim(&bytes).unwrap();
    let scene = track("autobahn").unwrap();
    let surface = scene.road_surface.as_ref().unwrap();
    let course = scene.course.as_ref().unwrap();
    let (mut pos, fwd, _) = calculate_grid_slot(0, course);
    let gate = &course.checkpoints[0];
    let ahead = [gate.center[0] + gate.forward[0] * 5.0, gate.center[1], gate.center[2] + gate.forward[2] * 5.0];
    for p in [pos, ahead, gate.center] {
        if let Some(h) = surface.query(p[0], p[2], p[1], 50.0, 100.0) { pos = [p[0], h.height, p[2]]; break; }
    }
    let mut v = VehicleSimulation::from_sim(&sim);
    v.reset(Vec3::from(pos) + Vec3::Y * v.resting_height(), (-fwd[0]).atan2(-fwd[2]));
    let dt = 1.0 / 60.0;
    for n in 0..120 {
        let controls = v.controls_from_signed_pedal(gas, 0.0, false);
        let tel = v.step(Some(surface), &controls, dt);
        if n % 8 == 0 {
            let r = v.suspension.wheels[2].tire.radius;
            let t2 = &v.suspension.wheels[2].tire;
            let t3 = &v.suspension.wheels[3].tire;
            eprintln!("LAUNCH t={:.2} v={:.2} g={} rpm={:.0} rearwheel/road(surface speed)={:.2} slipR={:.2} Fx={:.0}/{:.0} N={:.0}/{:.0} maxT={:.0}/{:.0} wheel_spd={:.2}", n as f32 * dt, tel.speed_mps, tel.current_gear, tel.engine_rpm,
                t2.omega * r / tel.speed_mps.max(0.3), t2.slip_ratio, t2.longitudinal_force, t3.longitudinal_force, t2.normal_load, t3.normal_load,
                t2.normal_load * t2.base_friction_coeff * t2.grip_multiplier, t3.normal_load * t3.base_friction_coeff * t3.grip_multiplier, t2.omega * r);
        }
    }
}

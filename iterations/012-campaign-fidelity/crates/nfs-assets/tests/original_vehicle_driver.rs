//! Task 6.2: the fixed-step driver of the original vehicle model on a flat
//! plane, with the collision pass in the 32 Hz queue (`collide_player`) and
//! a camera hook, in the scheduler order of `Sched_Step` `0x414ff0`.
//!
//! Cars: the carSpecs of the V5 fixtures (`vehicle-step-cars.tsv`).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

#[path = "support/vehicle_step.rs"]
mod fixture;

use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_pair::{PairCar, PairGlobals};
use nfs_assets::physics::original_vehicle::driver::collide_player;
use nfs_assets::physics::original_vehicle::step::Phase;
use nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP;
use nfs_assets::physics::original_vehicle::{
    input_word, FlatGround, OriginalCar, OriginalVehicle, StepQueue,
};

const HALF: [f32; 3] = [0.9, 0.6, 2.1];
/// Wheel ground points relative to the box centre (R, U, F): FL, FR, RL, RR
/// (the quadrant order of `0x49abc0`).
const WHEELS: [[f32; 3]; 4] = [
    [-0.75, -0.6, 1.2],
    [0.75, -0.6, 1.2],
    [-0.75, -0.6, -1.2],
    [0.75, -0.6, -1.2],
];
/// The 7 s start (0x1c0 steps) plus a little.
const START_S: f32 = 7.5;
const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn boxster() -> OriginalVehicle {
    let specs = fixture::cars().remove("Boxster").unwrap();
    let mut v = OriginalVehicle::from_specs(specs, HALF, WHEELS);
    v.set_pose([0.0, 3.0, 0.0], IDENTITY, [0.0; 3], [0.0; 3], &mut flat());
    v
}

fn flat() -> FlatGround {
    FlatGround {
        height: 0.0,
        surface: 1,
    }
}

/// Ground, a parked car to collide with, and a phase log.
struct Queue {
    ground: nfs_assets::physics::original_vehicle::GroundAdapter,
    parked: Vec<PairCar>,
    globals: PairGlobals,
    log: Vec<(i32, Phase)>,
    contacts: usize,
    camera_steps: u32,
}

impl Queue {
    fn new(v: &OriginalVehicle, parked: Vec<PairCar>) -> Self {
        Self {
            ground: v.ground.clone(),
            parked,
            globals: PairGlobals::default(),
            log: Vec::new(),
            contacts: 0,
            camera_steps: 0,
        }
    }
}

impl StepQueue for Queue {
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        self.ground.ground_step(car, &mut flat(), lcg);
    }
    fn collide(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        let before = car.velocity_33c;
        collide_player(car, HALF, &mut self.parked, &mut self.globals, lcg, |_| {
            false
        });
        if car.velocity_33c != before {
            self.contacts += 1;
        }
    }
    fn camera(&mut self, _car: &OriginalCar) {
        self.camera_steps += 1;
    }
    fn trace(&mut self, phase: Phase, car: &OriginalCar) {
        self.log.push((car.globals.sched_step_606ac4, phase));
    }
}

fn drive(v: &mut OriginalVehicle, q: &mut Queue, seconds: f32, word: u32) {
    let frames = (seconds * 60.0) as usize;
    for _ in 0..frames {
        v.advance(1.0 / 60.0, word, q);
    }
}

#[test]
fn spawns_on_the_road_and_selects_first_at_the_start() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    assert!(
        (v.car.position_330[1] - 0.6).abs() < 1e-6,
        "{:?}",
        v.car.position_330
    );
    assert_eq!(v.car.gear_d82, 1, "neutral before the start");
    let mut q = Queue::new(&v, vec![]);
    drive(&mut v, &mut q, START_S, 0);
    assert_eq!(
        v.car.gear_d82, 2,
        "the automatic box takes 1st at the start"
    );
    assert_eq!(v.hud_gear(), 1);
    assert!(v.car.wheels.iter().all(|w| w.airborne_84 == 0));
}

#[test]
fn full_throttle_accelerates_on_the_ground() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    let mut q = Queue::new(&v, vec![]);
    drive(&mut v, &mut q, START_S, 0);
    drive(&mut v, &mut q, 6.0, input_word(1.0, 0.0, 0.0, false));
    let c = &v.car;
    assert!(c.vel_fwd_d60() > 20.0, "d60 {}", c.vel_fwd_d60());
    assert!(c.gear_d82 >= 3, "gear {}", c.gear_d82);
    // On the road; the acceleration squat (V6 pitch) moves the box centre
    // by a few millimetres.
    assert!(
        (c.position_330[1] - 0.6).abs() < 0.02,
        "{:?}",
        c.position_330
    );
    assert!(
        c.axes_364[2][1] > 0.0,
        "nose up under power: {:?}",
        c.axes_364[2]
    );
    assert!(c.position_330[0].abs() < 1e-3, "{:?}", c.position_330);
    assert!(c.position_330[2] > 30.0);
    // 7.5 s start + 6 s at 64 Hz, the camera on every step.
    assert!((863..=864).contains(&q.camera_steps), "{}", q.camera_steps);
}

#[test]
fn steering_right_turns_toward_r() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    let mut q = Queue::new(&v, vec![]);
    drive(&mut v, &mut q, START_S, 0);
    drive(&mut v, &mut q, 3.0, input_word(1.0, 0.0, 0.0, false));
    drive(&mut v, &mut q, 1.0, input_word(0.3, 0.0, 1.0, false));
    assert!(v.car.angular_388[1] > 0.0, "yaw {:?}", v.car.angular_388);
    assert!(v.car.axes_364[2][0] > 0.05, "F {:?}", v.car.axes_364[2]);
}

#[test]
fn collision_runs_in_the_32_hz_queue_after_the_ground() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    let parked = OriginalVehicle::from_specs(v.car.specs.clone(), HALF, WHEELS);
    let mut wall = nfs_assets::physics::original_vehicle::driver::pair_car(&parked.car, HALF);
    wall.position_330 = [0.0, 0.6, 20.0];
    let mut q = Queue::new(&v, vec![wall]);
    drive(&mut v, &mut q, START_S, 0);
    drive(&mut v, &mut q, 4.0, input_word(1.0, 0.0, 0.0, false));
    assert!(q.contacts > 0, "no contact");
    // The player never passes through the parked car (which is not
    // integrated here, so the boxes may overlap a little).
    let gap = q.parked[0].position_330[2] - v.car.position_330[2];
    assert!(gap > HALF[2], "gap {gap}");
    // Order: collision follows the ground step and precedes the
    // integrations of the same scheduler step, on even steps only.
    for (i, &(step, phase)) in q.log.iter().enumerate() {
        if phase == Phase::Collide {
            assert_eq!(step % 2, 0);
            assert_eq!(q.log[i - 1], (step, Phase::Ground));
            assert_eq!(q.log[i + 1], (step, Phase::IntegratePosition));
        }
    }
}

/// Wheel drawing (`0x43e670`/`0x43c830`): in a steady turn the body rolls,
/// and each drawn wheel moves along the body up by its compression `w+0x68`,
/// so the drawn tyre bottom (the body's wheel ground point plus that offset)
/// stays on the road for the compressed outside wheels and the drooping
/// inside wheels alike.
#[test]
fn drawn_wheels_follow_the_road_in_a_roll() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    let mut q = Queue::new(&v, vec![]);
    drive(&mut v, &mut q, START_S, 0);
    drive(&mut v, &mut q, 4.0, input_word(1.0, 0.0, 0.0, false));
    drive(&mut v, &mut q, 1.5, input_word(0.6, 0.0, 1.0, false));
    let c = &v.car;
    let roll = c.axes_364[1][0];
    assert!(roll.abs() > 0.03, "body roll {:?}", c.axes_364[1]);
    let up = c.axes_364[1];
    for i in 0..4 {
        let (offset, _) = v.draw_wheel_pose(i);
        let bottom = c.world_points_1dc[i][1] + offset * up[1];
        assert!(
            bottom.abs() < 0.005,
            "wheel {i}: tyre bottom {bottom}, offset {offset}, roll {roll}"
        );
    }
    // Outside of a right turn is the left (wheels 0, 2): compressed.
    assert!(v.draw_wheel_pose(2).0 > 0.0 && v.draw_wheel_pose(3).0 < 0.0);
}

/// 10 % downhill along +z.
struct Slope;
impl nfs_assets::physics::original_vehicle::GroundQuery for Slope {
    fn wheel_ground(
        &mut self,
        _wheel: usize,
        p: [f32; 3],
    ) -> Option<nfs_assets::physics::original_vehicle::GroundHit> {
        let n = glam::Vec3::new(0.0, 1.0, 0.1).normalize().to_array();
        Some(nfs_assets::physics::original_vehicle::GroundHit::at(
            p,
            -0.1 * p[2],
            n,
            1,
        ))
    }
}

struct SlopeQueue(nfs_assets::physics::original_vehicle::GroundAdapter);
impl StepQueue for SlopeQueue {
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        self.0.ground_step(car, &mut Slope, lcg);
    }
}

/// Race start on a slope: through the 7 s start the original gating
/// (`+0xdb0` from 1, brake 0xff until step 0x1c0) holds the car on the grid,
/// also with the gas held; after the green light it rolls.
#[test]
fn holds_on_a_slope_until_the_green_light() {
    skip_without_local_fixtures!(fixture::CARS);
    let specs = fixture::cars().remove("Boxster").unwrap();
    let mut v = OriginalVehicle::from_specs(specs, HALF, WHEELS);
    let n = glam::Vec3::new(0.0, 1.0, 0.1).normalize();
    let f = glam::Vec3::X.cross(n).normalize();
    let axes = [glam::Vec3::X.to_array(), n.to_array(), f.to_array()];
    v.set_pose([0.0, 1.0, 0.0], axes, [0.0; 3], [0.0; 3], &mut Slope);
    let mut q = SlopeQueue(v.ground.clone());
    let start = v.car.position_330;
    let mut steps = 0;
    while v.car.globals.sched_step_606ac4 < START_SCHED_STEP {
        let word = if steps < 200 { 0 } else { input_word(1.0, 0.0, 0.0, false) };
        v.advance(1.0 / 64.0, word, &mut q);
        steps += 1;
    }
    let moved = glam::Vec3::from(v.car.position_330).distance(glam::Vec3::from(start));
    assert!(moved < 0.05, "moved {moved} m during the start: {:?}", v.car.position_330);
    assert_eq!(v.car.gear_d82, 1, "neutral until the green light");
    for _ in 0..64 * 3 {
        v.advance(1.0 / 64.0, 0, &mut q);
    }
    let rolled = glam::Vec3::from(v.car.position_330).distance(glam::Vec3::from(start));
    assert!(rolled > 0.5, "rolled {rolled} m after the green light");
}

/// Switching the original model on mid-countdown puts the car on the race
/// clock in neutral (the reset gear); after the green light the gear is kept.
#[test]
fn race_clock_puts_the_car_in_neutral_before_the_green_light() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    v.set_gear(2, 1500.0);
    v.set_race_clock(START_SCHED_STEP - 64 * 3);
    assert_eq!(v.car.gear_d82, 1);
    assert_eq!(v.car.globals.sched_step_606ac4, START_SCHED_STEP - 192);
    assert_eq!(v.car.start_db0, 1 + (START_SCHED_STEP - 192) / 2);
    v.set_gear(3, 3000.0);
    v.set_race_clock(START_SCHED_STEP + 640);
    assert_eq!(v.car.gear_d82, 3);
}

/// Car setup screen: the manual box and the damage option (`[0x657a2c]`) survive a restart.
#[test]
fn car_setup_reaches_the_car_globals() {
    use nfs_assets::physics::original_vehicle::CarSetup;
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    v.restart();
    assert_eq!(v.car.globals.auto_box_658098, 1);
    assert_eq!(v.car.globals.flip_events_657a2c, 0);
    v.set_setup(CarSetup { manual_box: true, damage: true });
    v.restart();
    assert_eq!(v.car.globals.auto_box_658098, 0);
    assert_eq!(v.car.globals.flip_events_657a2c, 1);
}

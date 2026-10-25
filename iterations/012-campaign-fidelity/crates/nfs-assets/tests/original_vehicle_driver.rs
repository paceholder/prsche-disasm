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
    drive(&mut v, &mut q, 0.5, 0);
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
    drive(&mut v, &mut q, 0.5, 0);
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
    // 6.5 s at 64 Hz, the camera on every step.
    assert!((415..=416).contains(&q.camera_steps), "{}", q.camera_steps);
}

#[test]
fn steering_right_turns_toward_r() {
    skip_without_local_fixtures!(fixture::CARS);
    let mut v = boxster();
    let mut q = Queue::new(&v, vec![]);
    drive(&mut v, &mut q, 0.5, 0);
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
    drive(&mut v, &mut q, 0.5, 0);
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

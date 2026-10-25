//! Launch log of the original model on flat ground: partial gas from a
//! standstill. `LAUNCH=0.3 cargo test ... original_launch -- --nocapture`.
use std::path::{Path, PathBuf};

use nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP;
use nfs_assets::physics::original_vehicle::ground::LEGACY_SURFACE;
use nfs_assets::physics::original_vehicle::{
    input_word, scheduler_step, FlatGround, GroundAdapter, OriginalCar, OriginalVehicle, StepQueue,
};

const HALF: [f32; 3] = [0.9, 0.6, 2.1];
const WHEELS: [[f32; 3]; 4] = [[-0.75, -0.6, 1.2], [0.75, -0.6, 1.2], [-0.75, -0.6, -1.2], [0.75, -0.6, -1.2]];

struct Q(GroundAdapter);
impl StepQueue for Q {
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut nfs_assets::physics::original_lcg::OriginalLcg) {
        self.0.ground_step(car, &mut FlatGround { height: 0.0, surface: LEGACY_SURFACE }, lcg);
    }
}

#[test]
fn original_launch() {
    let Ok(g) = std::env::var("LAUNCH") else { return };
    let gas: f32 = g.parse().unwrap();
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Simulation/CarData/boxster25.sim");
    let Ok(sim) = std::fs::read(p) else { return };
    let mut v = OriginalVehicle::from_sim(&sim, "boxster", HALF, WHEELS).unwrap();
    v.car.globals.sched_step_606ac4 = START_SCHED_STEP;
    v.car.start_db0 = 0x40;
    let mut fl = FlatGround { height: 0.0, surface: LEGACY_SURFACE };
    v.set_pose([0.0, 0.6, 0.0], [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3], [0.0; 3], &mut fl);
    let mut q = Q(v.ground.clone());
    for n in 0..64 * 4 {
        scheduler_step(&mut v.car, input_word(gas, 0.0, 0.0, false), &mut v.lcg, &mut q);
        if n % 16 == 0 {
            let c = &v.car;
            eprintln!("OLAUNCH t={:.2} v={:.2} g={} rpm={:.0} db8={} rear(dem,grip,lat,long)=[{:.2} {:.2} {:.2} {:.2}] [{:.2} {:.2} {:.2} {:.2}] surf={}", n as f32 / 64.0, c.vel_fwd_d60(), c.gear_d82, c.rpm_db4,
                c.spin_rear_db8, c.wheels[2].tyre.demand_a8, c.wheels[2].tyre.grip_ac, c.wheels[2].tyre.lat_90, c.wheels[2].tyre.long_98,
                c.wheels[3].tyre.demand_a8, c.wheels[3].tyre.grip_ac, c.wheels[3].tyre.lat_90, c.wheels[3].tyre.long_98, c.wheels[2].tyre.surface_8c);
        }
    }
}

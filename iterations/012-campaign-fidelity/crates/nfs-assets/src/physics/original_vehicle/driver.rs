//! Engine integration of the original vehicle model (task 6.2): the
//! `vehicle_model` switch, the fixed 64 Hz step clock, the keyboard-to-input
//! word mapping, the collision view of [`OriginalCar`], and
//! [`OriginalVehicle`], the player car runtime the renderer drives.
//!
//! The step order itself is [`scheduler_step`]; the renderer supplies the
//! ground (`Car_GroundContactTick` through the [`GroundAdapter`] on its road
//! query), the collision pass and the camera through [`StepQueue`].

use super::car::{Axes, OriginalCar};
use super::ground::{GroundAdapter, GroundQuery};
use super::sim::{derive, CarSpecs, Tuning, SIM_FILE_SIZE};
use super::step::{scheduler_step, update_world_points, StepQueue};
use crate::physics::original_contact::PreparedContactState;
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_pair::{PairCar, PairGlobals};
use crate::physics::original_track_objects::ObjectCar;
use crate::physics::original_track_pass::{SmashEvent, TrackObjectWorld};
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::race::original_collision::collide_pass_with;

/// Which vehicle model drives the player car.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VehicleModel {
    /// Upstream's 6 DOF placeholder model (`physics::vehicle`).
    #[default]
    Prototype,
    /// The recovered original model ([`OriginalCar`]).
    Original,
}

/// Scheduler rate (`Sched_Step` `0x414ff0`).
pub const STEP_HZ: f64 = 64.0;
/// Longest frame the clock catches up on (as the camera clock).
pub const MAX_FRAME_S: f64 = 0.25;

/// Fixed-step clock: frame time to whole 64 Hz scheduler steps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepClock {
    pending: f64,
}

impl StepClock {
    /// Advance by `dt` seconds (clamped to [`MAX_FRAME_S`]); returns the
    /// number of steps due.
    pub fn advance(&mut self, dt: f32) -> u32 {
        let step = 1.0 / STEP_HZ;
        self.pending += (dt as f64).clamp(0.0, MAX_FRAME_S);
        let mut steps = 0;
        while self.pending >= step {
            self.pending -= step;
            steps += 1;
        }
        steps
    }

    /// Fraction of the next step already elapsed (0..1), the render interpolation alpha.
    pub fn fraction(&self) -> f32 {
        (self.pending * STEP_HZ).clamp(0.0, 1.0) as f32
    }
}

/// Input word `[0x5e9664]` from the engine's controls (`Input_PollControllers`
/// `0x412e70` packing, `docs/re/vehicle.md` section 2): byte 0 bit 0
/// handbrake, byte 1 steer `(right − left)/4` signed (positive = right),
/// byte 2 gas/2 and byte 3 brake/2 (both clamped to 0x7e). Pedals and steer
/// are clamped to 0..1 and −1..1 and scaled to the 0..255 axis a keyboard
/// key reports (device type 3 "Key", getter `0x411c20`: 0xff while the
/// scancode byte `0x5de028[sc]` is set), so a held key gives steer byte 63
/// (`d90` = ±126), not 127, and gas/brake 126. Truncation is the C division.
/// No horn bit, no gear events (the box is automatic). Verified against the
/// original chain from key state to wheel angle (`original_vehicle_keyboard.rs`).
pub fn input_word(gas: f32, brake: f32, steer: f32, handbrake: bool) -> u32 {
    let axis = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as i32;
    let pedal = |v: f32| ((axis(v) / 2).min(0x7e)) as u32;
    let steer = (((steer.clamp(-1.0, 1.0) * 255.0).round() as i32) / 4) as i8 as u8 as u32;
    handbrake as u32 | steer << 8 | pedal(gas) << 16 | pedal(brake) << 24
}

/// Collision view of the car (`Car_CollideTick` `0x498ff0` works on the car
/// struct itself; [`PairCar`] holds the fields it reads). The box centre is
/// `+0x330`, the mass `sim+0x40 / 128` [VA 0x410730], `I = 6m` [VA 0x49b510].
/// Angular velocity stays in turns/s.
pub fn pair_car(car: &OriginalCar, half_extents: [f32; 3]) -> PairCar {
    let mass = car.specs.mass_40 / 128.0;
    let inertia = 6.0 * mass;
    let [r, u, f] = car.axes_364;
    let mut damage = [0.0; 9];
    damage.copy_from_slice(&car.damage_4e4[..9]);
    PairCar {
        position_330: car.position_330,
        velocity_33c: car.velocity_33c,
        mass_354: mass,
        inv_mass_358: 1.0 / mass,
        speed_35c: car.speed_35c,
        axis_r_364: r,
        axis_u_370: u,
        axis_f_37c: f,
        angular_388: car.angular_388,
        inertia_394: inertia,
        inv_inertia_398: 1.0 / inertia,
        extents_3a8: half_extents,
        radius_3b4: (half_extents[0] * half_extents[0]
            + half_extents[1] * half_extents[1]
            + half_extents[2] * half_extents[2])
            .sqrt(),
        active_7c: true,
        field_424: car.height_424,
        flags_52c: car.flags_52c as u32,
        resource_518: car.res_518 != 0,
        field_b50: car.field_b50,
        local_angular_d64: car.angular_ground_d64,
        field_dac: car.remote_dac,
        field_db0: car.start_db0,
        contacts_418: car.wheels_off_418 as i16,
        damage_4e4: damage,
        ..PairCar::default()
    }
}

/// Writes a collision result back into the car (the fields the pair and
/// barrier responses change). `+0x418` (the pair contact counter, set to 1 by
/// a hard hit of `Car_ApplyImpact`, read by the ground tick) and `+0xdb0`
/// (cleared by a hard hit above 120) are the same fields as the car's
/// `wheels_off_418` and `start_db0`.
pub fn store_pair_car(car: &mut OriginalCar, pair: &PairCar) {
    car.position_330 = pair.position_330;
    car.velocity_33c = pair.velocity_33c;
    car.speed_35c = pair.speed_35c;
    car.angular_388 = pair.angular_388;
    car.angular_ground_d64 = pair.local_angular_d64;
    car.wheels_off_418 = pair.contacts_418 as u16;
    car.start_db0 = pair.field_db0;
}

/// Barrier-response state (`Car_CollideTrackEdges` `0x495020`) of the car
/// while its collision view is `pair`: the ground frame, wheel points
/// `w+0x40`, skid `w+0xb8` (`+0x870`), `+0xdb8`, `+0xdc0` come from the car
/// (no neutral values as in the 6 DOF adapter A5). `+0x434` is not modelled
/// (0).
pub fn barrier_state(car: &OriginalCar, pair: &PairCar) -> PreparedContactState {
    PreparedContactState {
        position_330: pair.position_330,
        velocity_33c: pair.velocity_33c,
        speed_35c: pair.speed_35c,
        basis_364: pair.axis_r_364,
        basis_370: pair.axis_u_370,
        basis_37c: pair.axis_f_37c,
        field_38c: pair.angular_388[1],
        field_434: 0.0,
        basis_490: car.ground_490[0],
        basis_4a8: car.ground_490[2],
        local_velocity_d58: car.vel_ground_d58,
        wheel_points_7f8: car.wheels.map(|w| w.point_40),
        wheel_fields_870: car.wheels.map(|w| w.tyre.skid_b8),
        field_db8: car.spin_rear_db8 as f32,
        field_dc0: car.slip_ratio_dc0,
    }
}

/// Writes a barrier response back: into the collision view (position,
/// velocity, speed, yaw rate) and into the car (local velocity, wheel points
/// and the wheel fields it may clear).
pub fn store_barrier_state(car: &mut OriginalCar, pair: &mut PairCar, s: &PreparedContactState) {
    pair.position_330 = s.position_330;
    pair.velocity_33c = s.velocity_33c;
    pair.speed_35c = s.speed_35c;
    pair.angular_388[1] = s.field_38c;
    car.vel_ground_d58 = s.local_velocity_d58;
    for (w, (p, skid)) in car
        .wheels
        .iter_mut()
        .zip(s.wheel_points_7f8.iter().zip(s.wheel_fields_870))
    {
        w.point_40 = *p;
        w.tyre.skid_b8 = skid;
    }
    car.spin_rear_db8 = s.field_db8 as i32;
    car.slip_ratio_dc0 = s.field_dc0;
}

/// Track-object view of the car while its collision view is `pair`
/// (`Car_CollideTrackObjects` `0x498de0`): body points `+0x1dc`, the wheel
/// records' positions (`w+0x40`), the car-frame velocity and the ground frame
/// come from the car.
pub fn object_car(car: &OriginalCar, pair: &PairCar) -> ObjectCar {
    ObjectCar {
        pair: pair.clone(),
        wheel_records_7f8: car.wheels.map(|w| w.point_40),
        body_points_1dc: car.world_points_1dc.clone(),
        local_velocity_d58: car.vel_ground_d58,
        field_3a0: car.ground_up_3a0,
        field_428: car.height_428,
        code_464: 0,
        ground_right_490: car.ground_490[0],
        ground_forward_4a8: car.ground_490[2],
    }
}

/// Writes a track-object response back: the collision view and the car fields
/// the response changes (local velocity, wheel points, damage zones).
pub fn store_object_car(car: &mut OriginalCar, pair: &mut PairCar, o: &ObjectCar) {
    *pair = o.pair.clone();
    car.vel_ground_d58 = o.local_velocity_d58;
    for (w, p) in car.wheels.iter_mut().zip(o.wheel_records_7f8) {
        w.point_40 = p;
    }
    car.damage_4e4[..9].copy_from_slice(&o.pair.damage_4e4);
}

/// A barrier response of `Car_CollideTrackEdges` 0x495020: the magnitude
/// `0x494000` returned and the wall normal it used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarrierHit {
    pub magnitude: f32,
    pub normal: [f32; 3],
}

impl From<crate::surface::BarrierContact> for BarrierHit {
    fn from(c: crate::surface::BarrierContact) -> Self {
        Self {
            magnitude: c.magnitude,
            normal: c.normal,
        }
    }
}

/// The impact of a barrier response (`0x495020` after `0x494000`): above a
/// magnitude of 20 it becomes the impact strength `+0x430` and goes to
/// `Car_ImpactByDirection(car, wall normal, 1)`, i.e. `Car_ApplyImpact` mode 1
/// (zone damage when `[0x657a2c]`, the zone spin, and from 80 the hard-hit
/// kick `vel.y += s/24` with the 0.1 lift; ends with the `0x49bba0` clamp)
/// [VA 0x49548f..0x4954d8].
pub fn barrier_impact(pair: &mut PairCar, hit: BarrierHit, globals: &mut PairGlobals) {
    if 20.0 < hit.magnitude {
        pair.impact_430 = hit.magnitude;
        crate::physics::original_pair::impact_by_direction(pair, hit.normal, 1, globals);
    }
}

/// One `Car_CollideTick` pass with the player first (`collide_pass_with`): the
/// player's barrier response through `barrier` (`None` for no contact), the
/// other cars (`others`, kinematic AI, A6) without one. The pair response
/// draws from the shared random generator.
pub fn collide_player(
    car: &mut OriginalCar,
    half_extents: [f32; 3],
    others: &mut [PairCar],
    globals: &mut PairGlobals,
    lcg: &mut OriginalLcg,
    barrier: impl FnMut(&mut PreparedContactState) -> Option<BarrierHit>,
) -> usize {
    collide_player_objects(car, half_extents, others, globals, lcg, None, barrier)
}

/// [`collide_player`] with the track-object pass (`Car_CollideTrackObjects`)
/// after the player's edge pass; smashes that start go into the event list.
pub fn collide_player_objects(
    car: &mut OriginalCar,
    half_extents: [f32; 3],
    others: &mut [PairCar],
    globals: &mut PairGlobals,
    lcg: &mut OriginalLcg,
    mut objects: Option<(&mut TrackObjectWorld, &mut Vec<SmashEvent>)>,
    mut barrier: impl FnMut(&mut PreparedContactState) -> Option<BarrierHit>,
) -> usize {
    let mut cars = Vec::with_capacity(others.len() + 1);
    cars.push(pair_car(car, half_extents));
    cars.extend_from_slice(others);
    globals.rand_state_5d1028 = lcg.state_5d1028;
    globals.rand_mul_5d102c = lcg.multiplier_5d102c;
    let pairs = collide_pass_with(&mut cars, globals, |index, pair, globals| {
        if index != 0 {
            return;
        }
        let mut state = barrier_state(car, pair);
        if let Some(hit) = barrier(&mut state) {
            store_barrier_state(car, pair, &state);
            barrier_impact(pair, hit, globals);
        }
        // `Car_CollideTrackObjects` follows the edge pass at every call site
        // [VA 0x498ff0, 0x4980d0]. The player only (approximation A173).
        if let Some((world, events)) = objects.as_mut() {
            let mut o = object_car(car, pair);
            world.pass(&mut o, globals, events);
            store_object_car(car, pair, &o);
        }
    });
    lcg.state_5d1028 = globals.rand_state_5d1028;
    store_pair_car(car, &cars[0]);
    others.clone_from_slice(&cars[1..]);
    pairs
}

/// Scheduler step of the green light: the race-start gating of
/// `Car_ApplyControls` (`docs/re/vehicle.md` V4 results) releases the brake
/// after it, and the automatic box requests 1st on steps 0x1c1..0x1c3.
pub const START_SCHED_STEP: i32 = 0x1c0;
/// Scheduler step when the car is placed on the grid: the 7 s start
/// (0x1c0 steps at 64 Hz) runs from 0 to [`START_SCHED_STEP`]
/// (approximation A29: the writer of `g_SchedStep` is not read).
pub const COUNTDOWN_SCHED_STEP: i32 = 0;
/// `+0xdb0` set by the car reset `0x410730`. `Car_ApplyControls` counts it
/// up every 32 Hz step; below 0x40 it forces gas 0 and brake 0xff, then
/// until [`START_SCHED_STEP`] brake 0xff only (the engine revs in neutral),
/// so the car holds on a slope through the countdown.
pub const START_DB0: i32 = 1;

/// The player car on the original model: car state, random generator,
/// ground adapter state, the per-wheel support owners and the step clock.
#[derive(Clone, Debug)]
pub struct OriginalVehicle {
    pub car: OriginalCar,
    pub lcg: OriginalLcg,
    pub ground: GroundAdapter,
    pub owners: [OriginalSupportOwner; 4],
    /// `car+8`: track locator and body support owner (`Car_TrackLocate`).
    pub locator: super::locate::BodyLocator,
    pub clock: StepClock,
    /// Collision box half extents (`+0x3a8`).
    pub half_extents: [f32; 3],
    /// Front-end choices that reach the car (transmission, damage).
    pub setup: CarSetup,
}

/// The front-end settings the car consumes in a Quick Race. `FE_WriteCarConfig 0x4eeda0` writes
/// `.TRANSMISSION` (`res+0xc`, 1 = automatic) from the settings screen; ABS (`res+0x24`),
/// traction control (`res+0x20`) and the roof key are forced off for game type 0, so they are
/// not part of this. `damage` is the option DAMAGE `[0x657a2c]` (`Game.Damage`, key 0x20) that
/// gates zone damage in `Car_ApplyImpact` and the flip event of `0x49bd40`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarSetup {
    pub manual_box: bool,
    pub damage: bool,
}

impl Default for CarSetup {
    /// No front end (recordings, tests): automatic box, damage gate off. A Quick Race passes
    /// the screen's values (defaults: automatic, `Game.Damage` 1 [VA 0x4e5520]).
    fn default() -> Self {
        Self { manual_box: false, damage: false }
    }
}

/// `Car_SetupBodyPoints` `0x49abc0` (from `Car_ResetBody` `0x49b510`): the
/// body point list `+0x8c` (count `+0x32c`) is the render object's point list
/// (`R3DCar+0xfc`, the CRP `BndP` points), bubble-sorted by y (ascending,
/// swaps only on strictly lower); then the four lowest are put in quadrant
/// order (−x,+z), (+x,+z), (−x,−z), (+x,−z) (a quadrant with no point gets
/// the zero vector). Points 0..3 are the wheel ground points; the rest are
/// used by the height of a tilted car (`0x49a900`) and its body contact
/// (`0x496610`). Fewer than four points are returned unchanged.
pub fn setup_body_points(points: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut p = points.to_vec();
    let n = p.len();
    if n < 4 {
        return p;
    }
    for _ in 0..n {
        for i in 0..n - 1 {
            if p[i + 1][1] < p[i][1] {
                p.swap(i, i + 1);
            }
        }
    }
    let mut q = [[0.0f32; 3]; 4];
    for v in p.iter().take(4) {
        let (x, z) = (v[0], v[2]);
        if !(0.0 <= x) && !(z <= 0.0) {
            q[0] = *v;
        } else if !(x <= 0.0) && !(z <= 0.0) {
            q[1] = *v;
        } else if !(0.0 <= x) && !(0.0 <= z) {
            q[2] = *v;
        } else if 0.0 < x && z < 0.0 {
            q[3] = *v;
        }
    }
    p[..4].copy_from_slice(&q);
    p
}

/// Model name (`res+0x2a8`, the AICarData `.ais` stem) for a `.sim` file
/// stem, as the V1 recorder chose it (longest prefix first; hypothesis).
pub fn model_for_sim(stem: &str) -> &'static str {
    const PREFIXES: [(&str, &str); 23] = [
        ("356A", "356A"),
        ("356B", "356B"),
        ("356", "356"),
        ("550", "550a"),
        ("901", "901"),
        ("914", "914"),
        ("928", "928"),
        ("930T", "930t"),
        ("935", "935"),
        ("944", "944"),
        ("959", "959"),
        ("964", "964"),
        ("965T", "965t"),
        ("9936", "993"),
        ("993T", "993t"),
        ("993", "993"),
        ("996T", "996t"),
        ("996", "996"),
        ("GT1", "Gt1"),
        ("GT2", "Gt2"),
        ("GT3", "Gt3"),
        ("boxster", "boxster"),
        ("default", "default"),
    ];
    let lower = stem.to_ascii_lowercase();
    PREFIXES
        .iter()
        .find(|(p, _)| lower.starts_with(&p.to_ascii_lowercase()))
        .map_or("default", |&(_, m)| m)
}

impl OriginalVehicle {
    /// Car from a `.sim` file (V1 with default tuning for `model`, no parts),
    /// its collision box half extents and its four wheel ground points
    /// relative to the box centre in the car's axes (R, U, F), wheel order
    /// FL, FR, RL, RR (the quadrant order of `0x49abc0`: `−R +F`, `+R +F`,
    /// `−R −F`, `+R −F`). Resource flags: `res+0x14 = 1` (steering rate limit,
    /// as in the V5 recordings), others 0; automatic box.
    pub fn from_sim(
        raw_sim: &[u8],
        model: &str,
        half_extents: [f32; 3],
        wheel_points: [[f32; 3]; 4],
    ) -> Result<Self, String> {
        Self::from_sim_with(raw_sim, model, half_extents, wheel_points, CarSetup::default())
    }

    /// [`Self::from_sim`] with the front-end settings: a manual box derives with `res+0xc = 0`
    /// (drive factors x1.05, `Sim_DeriveFields 0x49cb50`).
    pub fn from_sim_with(
        raw_sim: &[u8],
        model: &str,
        half_extents: [f32; 3],
        wheel_points: [[f32; 3]; 4],
        setup: CarSetup,
    ) -> Result<Self, String> {
        if raw_sim.len() < SIM_FILE_SIZE {
            return Err(format!(
                "SIM is {} bytes, need {SIM_FILE_SIZE}",
                raw_sim.len()
            ));
        }
        let mut tuning = Tuning::quick_race(model);
        tuning.flag_c = i32::from(!setup.manual_box);
        let specs = derive(&raw_sim[..SIM_FILE_SIZE], &tuning);
        if !specs_usable(&specs) {
            return Err("SIM is not usable for the original model".into());
        }
        let mut v = Self::from_specs(specs, half_extents, wheel_points);
        v.set_setup(setup);
        Ok(v)
    }

    /// Applies the settings to the car globals (kept by resets, set again by [`Self::restart`]).
    pub fn set_setup(&mut self, setup: CarSetup) {
        self.setup = setup;
        self.apply_setup();
    }

    fn apply_setup(&mut self) {
        let g = &mut self.car.globals;
        g.auto_box_658098 = i32::from(!self.setup.manual_box);
        g.flip_events_657a2c = i32::from(self.setup.damage);
    }

    /// Car from derived carSpecs (see [`Self::from_sim`]).
    pub fn from_specs(
        specs: CarSpecs,
        half_extents: [f32; 3],
        wheel_points: [[f32; 3]; 4],
    ) -> Self {
        let mut v = Self {
            car: OriginalCar::new(specs),
            lcg: OriginalLcg::default(),
            ground: GroundAdapter::default(),
            owners: Default::default(),
            locator: Default::default(),
            clock: StepClock::default(),
            half_extents,
            setup: CarSetup::default(),
        };
        v.car.body_points_8c = wheel_points.to_vec();
        v.restart();
        v
    }

    /// Back to the start state (the race start): a fresh car with the same
    /// specs and body points, at rest in neutral, the scheduler at
    /// [`COUNTDOWN_SCHED_STEP`] with the start counter at [`START_DB0`], so
    /// the original gating holds the brake until the green light, and the
    /// random generator seeded as the race init `0x414eb0` seeds it. The pose
    /// is kept.
    pub fn restart(&mut self) {
        let old = &self.car;
        let mut car = OriginalCar::new(old.specs.clone());
        car.body_points_8c = old.body_points_8c.clone();
        car.half_extents_3a8 = self.half_extents;
        car.position_330 = old.position_330;
        car.axes_364 = old.axes_364;
        car.res_14 = 1;
        car.start_db0 = START_DB0;
        car.globals.sched_step_606ac4 = COUNTDOWN_SCHED_STEP;
        // A race participant: `0x40f160` sets `+0x52c` bit 0 for the human and
        // the AI racers (res+8 bits 0/1), so `Car_BuildGroundFrame` builds the
        // full frame and `0x417190` gives `+0x78 = 0`.
        car.flags_52c = 1;
        update_world_points(&mut car);
        self.car = car;
        self.apply_setup();
        // The race init `0x414eb0` seeds the generator (`Rand_Seed 0x4a4780`).
        self.lcg = OriginalLcg::race_start();
        self.ground = GroundAdapter::default();
        self.locator = Default::default();
        self.owners = Default::default();
        self.clock = StepClock::default();
    }

    /// Mid-race reset of the drive state, before a track reset moves the car
    /// (`Car_ResetToTrack 0x40f540`): `Car_ResetDriveState 0x49e220` (inputs,
    /// gear N, wheel records, rpm, force sums, `+0x540`) and `0x40f400`
    /// (angular velocity, `+0x418`, `+0x420`, `+0x40c`, speed). The velocity
    /// is cleared by `0x40f540` itself. Unlike [`Self::restart`], the
    /// scheduler step, the random generator and the race fields (`+0xb50`,
    /// damage, flags, light switches) stay. `+0xdb0` becomes 0x3c (a short
    /// brake hold) or, if a roll had cut the controls, 1 (a full one).
    /// Gear: `0x49e220` selects 1st when `res+0xc == 1`, else N. `res+0xc`
    /// is not ported; it is taken as the automatic box flag (approximation
    /// A98), so the automatic car gets 1st (in N it would wait for the
    /// race-start request, which comes only before step 0x1c4).
    pub fn reset_drive_state(&mut self) {
        // `0x49e220` clears the locator section `+0xc`.
        self.locator.track.section_04 = -1;
        let old = &self.car;
        let mut car = OriginalCar::new(old.specs.clone());
        car.res_14 = old.res_14;
        car.res_20 = old.res_20;
        car.res_24 = old.res_24;
        car.res_518 = old.res_518;
        car.damage = old.damage;
        car.body_surface_2c = old.body_surface_2c;
        car.remote_steer_38 = old.remote_steer_38;
        car.remote_brake_3a = old.remote_brake_3a;
        car.remote_bits_3b = old.remote_bits_3b;
        car.ground_on_7c = old.ground_on_7c;
        car.frozen_7d = old.frozen_7d;
        car.active_7f = old.active_7f;
        car.reortho_80 = old.reortho_80;
        car.body_points_8c = old.body_points_8c.clone();
        car.position_330 = old.position_330;
        car.axes_364 = old.axes_364;
        car.half_extents_3a8 = old.half_extents_3a8;
        car.alt_normal_3e8 = old.alt_normal_3e8;
        car.alt_base_400 = old.alt_base_400;
        car.track = old.track;
        car.damage_4e4 = old.damage_4e4;
        car.player_520 = old.player_520;
        car.flags_52c = old.flags_52c;
        car.flags_52d = old.flags_52d;
        car.field_544 = old.field_544;
        car.field_b50 = old.field_b50;
        car.toggles_d87 = old.toggles_d87;
        car.ai_da8 = old.ai_da8;
        car.remote_dac = old.remote_dac;
        car.events_1110 = old.events_1110.clone();
        car.lights_1230 = old.lights_1230;
        car.lights_1232 = old.lights_1232;
        car.globals = old.globals;
        car.start_db0 = if old.start_db0 != 0 { 0x3c } else { 1 };
        if car.globals.auto_box_658098 == 1 {
            car.gear_d82 = 2;
            car.gear_req_d7e = 2;
        }
        update_world_points(&mut car);
        self.car = car;
        self.ground = GroundAdapter::default();
        self.owners = Default::default();
    }

    /// Puts the car on the race clock: `g_SchedStep` at `sched` and the start
    /// counter `+0xdb0` as `Car_ApplyControls` would have counted it from
    /// the reset (1 at step [`COUNTDOWN_SCHED_STEP`], +1 per 32 Hz step).
    /// Before the green light the car is in its reset gear, neutral
    /// (`0x49e220`; the gear logic of `Car_ApplyControls` only runs after
    /// [`START_SCHED_STEP`]), so the engine revs without driving. Used when
    /// the model is switched on during a race.
    pub fn set_race_clock(&mut self, sched: i32) {
        let sched = sched.max(COUNTDOWN_SCHED_STEP);
        self.car.globals.sched_step_606ac4 = sched;
        self.car.start_db0 = START_DB0 + (sched - COUNTDOWN_SCHED_STEP) / 2;
        if sched < START_SCHED_STEP {
            let rpm = self.car.rpm_db4;
            self.set_gear(1, rpm);
        }
    }

    /// Moves the car to a pose (box centre, axes R/U/F, velocity, angular
    /// velocity in turns/s), keeping the model state, and places it on the
    /// ground there. Used at spawn and when switching from the prototype;
    /// the caller resets the support owners ([`Self::owners`]) for a jump.
    pub fn set_pose(
        &mut self,
        position: [f32; 3],
        axes: Axes,
        velocity: [f32; 3],
        angular: [f32; 3],
        ground: &mut impl GroundQuery,
    ) {
        let car = &mut self.car;
        car.position_330 = position;
        car.axes_364 = axes;
        car.velocity_33c = velocity;
        car.angular_388 = angular;
        self.ground = GroundAdapter::default();
        self.ground.place(car, ground, &mut self.lcg);
        // Ground-frame velocities for the first step's consumers.
        let [r, n, f] = car.ground_490;
        let d = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        car.vel_ground_d58 = [d(velocity, r), d(velocity, n), d(velocity, f)];
    }

    /// Replaces the body points with the car model's point list in the car
    /// frame (relative to the box centre), ordered by [`setup_body_points`]
    /// as `Car_ResetBody` does. Without at least four points the current
    /// (wheel) points stay.
    pub fn set_body_points(&mut self, points: &[[f32; 3]]) {
        if points.len() < 4 {
            return;
        }
        self.car.body_points_8c = setup_body_points(points);
        update_world_points(&mut self.car);
    }

    /// Selects a gear (`+0xd82`, 0 = R, 1 = N, 2 = 1st..) with no shift in
    /// progress, and the engine rpm (model switch).
    pub fn set_gear(&mut self, gear: i8, rpm: f32) {
        let top = (self.car.specs.gear_count_50 - 1).clamp(1, 7) as i8;
        let g = gear.clamp(0, top);
        let c = &mut self.car;
        c.gear_d82 = g;
        c.gear_req_d7e = g;
        c.prev_gear_d81 = g;
        c.shift_timer_d84 = 0;
        c.downshift_d83 = 0;
        c.rpm_db4 = rpm.max(0.0);
    }

    /// Runs the scheduler steps due after `dt` seconds with the same input
    /// word (read on even steps). Returns the number of steps.
    pub fn advance(&mut self, dt: f32, input_word: u32, queue: &mut impl StepQueue) -> u32 {
        let steps = self.clock.advance(dt);
        for _ in 0..steps {
            scheduler_step(&mut self.car, input_word, &mut self.lcg, queue);
        }
        steps
    }

    /// Gear for the HUD: −1 R, 0 N, 1.. forward.
    pub fn hud_gear(&self) -> i32 {
        self.car.gear_d82 as i32 - 1
    }

    /// Suspension pose of drawn wheel `i` (FL, FR, RL, RR) as the original
    /// car draw applies it: `(offset, camber)`.
    /// - `offset`: metres along the body up. `0x43e670` (R3DCarLOD, from
    ///   `0x439f90`) sets the wheel pivot `R3DCar+0x6d0+12·i` y to the
    ///   compression `w+0x68` of physics wheel `[0x5b36f0 + 4·i]` = i
    ///   (`[0x5b3700]` = 1, 0, 3, 2 in mirror mode, not supported), and
    ///   `0x43c830` translates the wheel part by that pivot: up when
    ///   compressed, down when drooping, 0 at rest.
    /// - `camber`: turns; `0x438440` puts the arm angle `w+0x70` into the
    ///   wheel channel's z (`car+0x828 + 0xc4·i`), which `0x43c830` turns
    ///   by `ch.z·360°` about the part's forward axis.
    pub fn draw_wheel_pose(&self, wheel: usize) -> (f32, f32) {
        self.car
            .wheels
            .get(wheel)
            .map_or((0.0, 0.0), |w| (w.compression_68, w.angle_70))
    }

    /// Front wheel steer angle for drawing, in turns: `sim+0xfc·steer/17`
    /// 1/256 turn (`Wheel_TyreForce` `0x4a20d0`) of the smoothed copy
    /// `+0xac8/+0xacc`. Positive turns toward `R`.
    pub fn draw_steer_turns(&self, wheel: usize) -> f32 {
        let s = self.car.steer_smooth_ac8.get(wheel).copied().unwrap_or(0.0);
        self.car.specs.steer_lock_fc * s / 17.0 / 256.0
    }
}

/// Camera view of the car (`Camera_UpdateAll` reads `+0x330`, the axes, the
/// half extents and the ground-frame speeds `+0xd58`/`+0xd60`). No track
/// nodes: node 0, travel direction +1 (approximation A12).
pub fn camera_car(car: &OriginalCar, half_extents: [f32; 3]) -> crate::camera::CarView {
    crate::camera::CarView {
        node_08: 0,
        pos_330: car.position_330,
        axes_364: car.axes_364,
        half_extents_3a8: half_extents,
        direction_d30: 1,
        lateral_speed_d58: car.vel_ground_d58[0],
        forward_speed_d60: car.vel_ground_d58[2],
    }
}

/// Whether `spec` holds a usable SIM (finite mass, gears) for the original
/// model.
pub fn specs_usable(specs: &CarSpecs) -> bool {
    specs.mass_40.is_finite()
        && specs.mass_40 > 0.0
        && (3..=8).contains(&specs.gear_count_50)
        && specs.tyre_circumference_148.is_finite()
        && specs.tyre_circumference_148 > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::original_vehicle::step::Phase;
    use crate::physics::original_vehicle::FlatGround;

    #[test]
    fn clock_runs_64_steps_per_second() {
        for fps in [30u32, 60, 64, 144] {
            let mut clock = StepClock::default();
            let n: u32 = (0..fps).map(|_| clock.advance(1.0 / fps as f32)).sum();
            assert!((63..=64).contains(&n), "{fps} fps: {n}");
        }
        assert_eq!(StepClock::default().advance(5.0), 16);
    }

    #[test]
    fn input_word_packs_full_scale_keys() {
        // A key reports 255: steer 255/4 = 63, pedals 255/2 clamped to 0x7e.
        assert_eq!(input_word(1.0, 0.0, 0.0, false), 126 << 16);
        assert_eq!(input_word(0.0, 1.0, 0.0, true), 126 << 24 | 1);
        assert_eq!(input_word(0.0, 0.0, 1.0, false), 63 << 8);
        assert_eq!(input_word(0.0, 0.0, -1.0, false), 0xc1 << 8);
    }

    #[test]
    fn body_points_put_the_four_lowest_in_quadrant_order() {
        let pts = [
            [0.8, 1.0, 1.0],
            [0.7, 0.0, -1.1],
            [-0.7, 0.01, 0.9],
            [-0.8, 0.5, -1.0],
            [0.7, 0.0, 0.9],
            [-0.7, 0.0, -1.1],
        ];
        let p = setup_body_points(&pts);
        assert_eq!(&p[..4], &[[-0.7, 0.01, 0.9], [0.7, 0.0, 0.9], [-0.7, 0.0, -1.1], [0.7, 0.0, -1.1]]);
        assert_eq!(&p[4..], &[[-0.8, 0.5, -1.0], [0.8, 1.0, 1.0]]);
    }

    #[test]
    fn model_names_follow_the_recorder() {
        assert_eq!(model_for_sim("GT2race"), "Gt2");
        assert_eq!(model_for_sim("356Acoupe16"), "356A");
        assert_eq!(model_for_sim("356coupe11"), "356");
        assert_eq!(model_for_sim("boxster25"), "boxster");
        assert_eq!(model_for_sim("unknown"), "default");
    }

    #[derive(Default)]
    struct Recorder {
        log: Vec<(i32, Phase)>,
        ground: GroundAdapter,
    }

    impl StepQueue for Recorder {
        fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
            self.ground.ground_step(
                car,
                &mut FlatGround {
                    height: 0.0,
                    surface: 1,
                },
                lcg,
            );
        }
        fn trace(&mut self, phase: Phase, car: &OriginalCar) {
            self.log.push((car.globals.sched_step_606ac4, phase));
        }
    }

    /// Step order of `Sched_Step` for the player car: on even steps the
    /// 32 Hz queue (track locate 6, inputs 0x15, dynamics and ground 0x1e,
    /// collision 0x28),
    /// then every step the integrations (0x1e) and the camera (100).
    #[test]
    fn scheduler_runs_the_original_order() {
        use Phase::*;
        let mut car = OriginalCar::new(CarSpecs::from_bytes(&[0u8; 0x1c4]));
        car.globals.sched_step_606ac4 = 10;
        let mut q = Recorder::default();
        let mut lcg = OriginalLcg::default();
        for _ in 0..4 {
            scheduler_step(&mut car, 0, &mut lcg, &mut q);
        }
        let even = [
            TrackLocate,
            ReadInputs,
            Dynamics,
            Ground,
            Collide,
            IntegratePosition,
            IntegrateOrientation,
            Camera,
        ];
        let odd = [IntegratePosition, IntegrateOrientation, Camera];
        let mut expected = Vec::new();
        for step in 10..14 {
            let phases: &[Phase] = if step % 2 == 0 { &even } else { &odd };
            expected.extend(phases.iter().map(|&p| (step, p)));
        }
        assert_eq!(q.log, expected);
        assert_eq!(car.globals.sched_step_606ac4, 14);
    }
}

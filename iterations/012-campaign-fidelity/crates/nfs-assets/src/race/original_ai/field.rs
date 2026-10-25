//! The AI cars of a race on the original model: their track data, the
//! cars, and the scheduler step that runs the player and the AI cars in the
//! original order (`Sched_Step` 0x414ff0: each priority class for all cars,
//! cars in insertion order, player first; `docs/re/vehicle.md` section 1 in
//! the workspace root).
//!
//! Approximations (`docs/approximations.md`): A40 opponents use the
//! player's car; A41 race order and progress from the node graph and an own
//! lap counter; A42 the countdown runs outside the original scheduler; A43
//! the player's progress for the catch-up; A44 grid placement from the
//! engine course.

use super::ais::{load_ais, AisData, TrackTables};
use super::driver::{read_inputs_ai, track_locate, AiState, AiWorld};
use super::planner::{AiOther, OrderSlot};
use super::query::TrackLocator;
use super::racing_line::RacingLine;
use super::track::{sim_chunks, TrackNodes};
use crate::physics::original_contact::PreparedContactState;
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_pair::PairGlobals;
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::physics::original_vehicle::car::Axes;
use crate::physics::original_vehicle::controls::read_inputs;
use crate::physics::original_vehicle::driver::{
    barrier_state, pair_car, store_barrier_state, store_pair_car,
};
use crate::physics::original_vehicle::ground::{GroundAdapter, GroundQuery};
use crate::physics::original_vehicle::step::{
    dynamics_tick, integrate_orientation, integrate_position,
};
use crate::physics::original_vehicle::{OriginalCar, OriginalVehicle};
use crate::race::original_collision::collide_pass;

/// Track data the AI needs: the node graph and the racing line.
#[derive(Clone, Debug, PartialEq)]
pub struct OriginalAiTrack {
    /// Track name as `[0x657444]` holds it (`alps`, `monaco1`, ...).
    pub name: String,
    pub reverse: bool,
    pub nodes: TrackNodes,
    pub line: RacingLine,
}

impl OriginalAiTrack {
    /// From the track CRP (RefPack or decompressed), `.jnc` and the
    /// direction's `.lsp`.
    pub fn from_files(
        name: &str,
        reverse: bool,
        crp: &[u8],
        jnc: Option<&[u8]>,
        lsp: &[u8],
    ) -> Option<Self> {
        let crp = nfs_formats::decompress(crp).ok()?;
        let (nodes, sections) = sim_chunks(&crp)?;
        let nodes = TrackNodes::build(nodes, &sections, jnc);
        if nodes.nodes.is_empty() {
            return None;
        }
        Some(Self {
            name: name.to_ascii_lowercase(),
            reverse,
            nodes,
            line: RacingLine::from_bytes(lsp),
        })
    }
}

/// One AI car: the original car runtime and its AI fields.
#[derive(Clone, Debug)]
pub struct AiDriverCar {
    pub vehicle: OriginalVehicle,
    pub ai: AiState,
    /// Body support owner for the barrier response.
    pub body_owner: OriginalSupportOwner,
    /// Laps counted from the node fraction (A41).
    pub laps: i32,
    last_fraction: f32,
}

/// The ground and barrier queries of the renderer.
pub trait FieldQueue {
    /// `Car_GroundContactTick` 0x49bd40 for car `index` (0 = player).
    fn ground(
        &mut self,
        index: usize,
        car: &mut OriginalCar,
        adapter: &mut GroundAdapter,
        owners: &mut [OriginalSupportOwner; 4],
        lcg: &mut OriginalLcg,
    );
    /// Barrier response (`Car_CollideTrackEdges` 0x495020) of car `index`
    /// with its body support owner (`None` for the player: the queue's own);
    /// false without contact.
    fn barrier(
        &mut self,
        index: usize,
        state: &mut PreparedContactState,
        half: [f32; 3],
        owner: Option<&mut OriginalSupportOwner>,
    ) -> bool;
    /// `Camera_UpdateAll` (prio 100) after the integrations.
    fn camera(&mut self, _player: &OriginalCar) {}
}

/// AI options: SKILL, CATCHUP mode, the res skill values (fe.txt defaults).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiOptions {
    pub skill_6573f4: i32,
    pub mode_6573e8: i32,
    pub res_skill_154: [f32; 8],
}

impl Default for AiOptions {
    /// fe.txt: `skill = 0`, every car's skill values 1.0 and ranges 0.
    fn default() -> Self {
        Self {
            skill_6573f4: 0,
            mode_6573e8: 0,
            res_skill_154: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        }
    }
}

/// The AI cars of a race.
#[derive(Clone, Debug)]
pub struct OriginalAiField {
    pub track: OriginalAiTrack,
    pub ais: AisData,
    pub tables: TrackTables,
    pub cars: Vec<AiDriverCar>,
    pub options: AiOptions,
    /// The player's locator and lap count (race order and catch-up).
    pub player_locator: TrackLocator,
    pub player_laps: i32,
    player_fraction: f32,
    /// `[0x606ac0]`: the race runs (AI cars are stepped only then).
    pub running: bool,
}

impl OriginalAiField {
    /// AI data for the player's car (`Ais_Load` 0x49ca30 for `model` on this
    /// track; the grip ratio `car+0x10d8` is 1 without parts).
    pub fn new(track: OriginalAiTrack, ais_file: &[u8], model: &str, sim_140: &mut f32) -> Self {
        let (ais, tables) = load_ais(ais_file, model, &track.name, track.reverse, 1.0, sim_140);
        Self {
            track,
            ais,
            tables,
            cars: Vec::new(),
            options: AiOptions::default(),
            player_locator: TrackLocator {
                section_04: -1,
                ..Default::default()
            },
            player_laps: 0,
            player_fraction: 0.0,
            running: false,
        }
    }

    /// Adds an AI car built like the player's (`vehicle`, whose `.sim`
    /// carSpecs must have `sim+0x140` scaled as [`Self::new`] scaled it),
    /// placed at `position` (box centre) with `axes`, on the ground.
    pub fn spawn(
        &mut self,
        mut vehicle: OriginalVehicle,
        position: [f32; 3],
        axes: Axes,
        ground: &mut impl GroundQuery,
    ) {
        vehicle.restart();
        vehicle.car.flags_52c = 1;
        vehicle.car.ai_da8 = 1;
        vehicle.set_pose(position, axes, [0.0; 3], [0.0; 3], ground);
        // The start gate holds the brake (lights on) until the race runs.
        vehicle.car.brake_eff_d80 = 0xff;
        let t = &self.track.nodes;
        let node = t.locate(position, 0);
        let ai = AiState::at_node(t, node);
        self.cars.push(AiDriverCar {
            vehicle,
            ai,
            body_owner: OriginalSupportOwner::default(),
            laps: 0,
            last_fraction: 0.0,
        });
    }

    /// Grid slot `slot` ahead of `start` (approximation A44: the original
    /// start grid, `<track>_fstart.scn`, is not read): rows of two, the
    /// first row 3 nodes ahead and one row every 2 nodes, 2.5 m left and
    /// right of the node centre along the node's lateral axis; box centre
    /// 0.5 m above the node, axes from the node forward (travel direction).
    pub fn grid_slot(&self, start: i32, slot: usize) -> ([f32; 3], Axes) {
        let t = &self.track.nodes;
        let ahead = 3 + 2 * (slot / 2) as i32;
        let delta = if self.track.reverse { -ahead } else { ahead };
        let node = t.step(start, delta, -1);
        let n = &t.nodes[node as usize];
        let side = if slot.is_multiple_of(2) { -2.5 } else { 2.5 };
        let pos = [0, 1, 2].map(|k| n.pos_04[k] + n.lateral_28[k] * side + n.up_10[k] * 0.5);
        let sign = if self.track.reverse { -1.0 } else { 1.0 };
        let (fx, fz) = (sign * n.forward_1c[0], sign * n.forward_1c[2]);
        let l = (fx * fx + fz * fz).sqrt().max(1e-6);
        let f = [fx / l, 0.0, fz / l];
        (pos, [[f[2], 0.0, -f[0]], [0.0, 1.0, 0.0], f])
    }

    /// Lap fraction of a node (the `0x481b70` interpolation of
    /// `Car_TrackProgressTick` 0x4169b0).
    fn fraction(&self, node: i32) -> f32 {
        let (start, end, len, first) = self.track.nodes.progress_terms(node);
        let f = (end - start) * ((node - first) as f32 / len as f32) + start;
        if self.track.reverse {
            1.0 - f
        } else {
            f
        }
    }

    fn count_lap(laps: &mut i32, last: &mut f32, f: f32) -> f32 {
        if f < *last - 0.5 {
            *laps += 1;
        } else if f > *last + 0.5 {
            *laps -= 1;
        }
        *last = f;
        *laps as f32 + f
    }

    /// One 64 Hz scheduler step for the player and the AI cars.
    pub fn step(
        &mut self,
        player: &mut OriginalVehicle,
        input_word: u32,
        globals: &mut PairGlobals,
        queue: &mut impl FieldQueue,
    ) {
        let step = player.car.globals.sched_step_606ac4;
        let run_ai = self.running;
        for c in &mut self.cars {
            c.vehicle.car.globals.sched_step_606ac4 = step;
        }
        let lcg = &mut player.lcg;
        if step & 1 == 0 {
            // Prio 6: Car_TrackLocate.
            let t = &self.track.nodes;
            self.player_locator.update(t, player.car.position_330);
            let pf = self.fraction(self.player_locator.node_00 as i32);
            let player_af8 = Self::count_lap(&mut self.player_laps, &mut self.player_fraction, pf);
            if run_ai {
                for i in 0..self.cars.len() {
                    let c = &mut self.cars[i];
                    track_locate(&c.vehicle.car, &mut c.ai, &self.track.nodes);
                    let car = &mut c.vehicle.car;
                    car.surface_464 = (car.body_surface_2c & 0xf) as i32;
                    if car.globals.handling_mode_6573f8 != 0
                        && (car.surface_464 == 1 || car.surface_464 == 10)
                    {
                        car.surface_464 = 2;
                    }
                    let f = self.fraction(self.cars[i].ai.locator_08.node_00 as i32);
                    let c = &mut self.cars[i];
                    c.ai.progress_af8 = Self::count_lap(&mut c.laps, &mut c.last_fraction, f);
                }
            }
            // Race order (0x417710, by progress): index 0 = player.
            let mut order: Vec<(usize, f32)> = vec![(0, player_af8)];
            order.extend(self.cars.iter().enumerate().map(|(i, c)| (i + 1, c.ai.progress_af8)));
            order.sort_by(|a, b| a.1.total_cmp(&b.1));
            let others: Vec<AiOther> = std::iter::once(AiOther {
                pos_330: player.car.position_330,
                half_w_3a8: player.car.half_extents_3a8[0],
                fwd_d60: player.car.vel_ground_d58[2],
                node_08: self.player_locator.node_00,
            })
            .chain(self.cars.iter().map(|c| AiOther {
                pos_330: c.vehicle.car.position_330,
                half_w_3a8: c.vehicle.car.half_extents_3a8[0],
                fwd_d60: c.vehicle.car.vel_ground_d58[2],
                node_08: c.ai.locator_08.node_00,
            }))
            .collect();

            // Prio 0x15: Car_ReadInputs.
            {
                let specs = player.car.specs.clone();
                let (mut cc, ce) = player.car.controls_view(input_word);
                read_inputs(&mut cc, &specs, &ce);
                player.car.store_controls(&cc, &ce);
            }
            if run_ai {
                let world = AiWorld {
                    track: &self.track.nodes,
                    line: &self.track.line,
                    ais: &self.ais,
                    tables: &self.tables,
                    res_skill_154: self.options.res_skill_154,
                    skill_6573f4: self.options.skill_6573f4,
                    reverse_657430: self.track.reverse,
                    mode_6573e8: self.options.mode_6573e8,
                    running_606ac0: 1,
                    player_af8,
                    player_b00: 0.0,
                };
                for i in 0..self.cars.len() {
                    let me = i + 1;
                    let slots: Vec<OrderSlot> = order
                        .iter()
                        .map(|&(k, _)| {
                            if k == me {
                                OrderSlot::Me
                            } else {
                                OrderSlot::Other(others[k])
                            }
                        })
                        .collect();
                    let c = &mut self.cars[i];
                    c.ai.order_528 = order.iter().position(|&(k, _)| k == me).unwrap() as i32;
                    read_inputs_ai(&mut c.vehicle.car, &mut c.ai, &world, &slots, input_word, lcg);
                }
            }

            // Prio 0x1e: dynamics, then ground, per car.
            dynamics_tick(&mut player.car, lcg);
            queue.ground(0, &mut player.car, &mut player.ground, &mut player.owners, lcg);
            if run_ai {
                for (i, c) in self.cars.iter_mut().enumerate() {
                    let v = &mut c.vehicle;
                    dynamics_tick(&mut v.car, lcg);
                    queue.ground(i + 1, &mut v.car, &mut v.ground, &mut v.owners, lcg);
                }
            }

            // Prio 0x28: Car_CollideTick for all cars (the AI cars only once
            // they run: before that their dynamics and integration don't).
            self.collide(player, globals, queue, run_ai);
        }
        // Every step, prio 0x1e: the integrations.
        integrate_position(&mut player.car);
        integrate_orientation(&mut player.car);
        if run_ai {
            for c in &mut self.cars {
                integrate_position(&mut c.vehicle.car);
                integrate_orientation(&mut c.vehicle.car);
            }
        }
        queue.camera(&player.car);
        player.car.globals.sched_step_606ac4 = step.wrapping_add(1);
        for c in &mut self.cars {
            c.vehicle.car.globals.sched_step_606ac4 = step.wrapping_add(1);
        }
    }

    /// `Car_CollideTick` 0x498ff0 for the player and the AI cars: one
    /// `collide_pass` with each car's barrier response.
    fn collide(
        &mut self,
        player: &mut OriginalVehicle,
        globals: &mut PairGlobals,
        queue: &mut impl FieldQueue,
        with_ai: bool,
    ) {
        let lcg = &mut player.lcg;
        let mut pairs = vec![pair_car(&player.car, player.half_extents)];
        if with_ai {
            pairs.extend(self.cars.iter().map(|c| pair_car(&c.vehicle.car, c.vehicle.half_extents)));
        }
        globals.rand_state_5d1028 = lcg.state_5d1028;
        globals.rand_mul_5d102c = lcg.multiplier_5d102c;
        let cars = &mut self.cars;
        let pcar = &mut player.car;
        let phalf = player.half_extents;
        collide_pass(&mut pairs, globals, |index, pair| {
            let (car, half, owner): (&mut OriginalCar, [f32; 3], Option<&mut OriginalSupportOwner>) =
                if index == 0 {
                    (&mut *pcar, phalf, None)
                } else {
                    let c = &mut cars[index - 1];
                    (&mut c.vehicle.car, c.vehicle.half_extents, Some(&mut c.body_owner))
                };
            let mut state = barrier_state(car, pair);
            if queue.barrier(index, &mut state, half, owner) {
                store_barrier_state(car, pair, &state);
            }
        });
        lcg.state_5d1028 = globals.rand_state_5d1028;
        store_pair_car(&mut player.car, &pairs[0]);
        for (c, p) in self.cars.iter_mut().zip(&pairs[1..]) {
            store_pair_car(&mut c.vehicle.car, p);
        }
    }
}

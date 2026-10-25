//! The AI on an [`OriginalCar`] (`docs/re/ai.md` section 1): the AI fields
//! the car model does not hold ([`AiState`]), the track locate step the AI
//! reads (prio 6) and the AI branch of `Car_ReadInputs` 0x4991e0 (prio
//! 0x15).

use super::ais::{AisData, TrackTables};
use super::planner::{drive, target_speed, AiCar, AiEnv, OrderSlot};
use super::query::TrackLocator;
use super::racing_line::RacingLine;
use super::track::TrackNodes;
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_vehicle::controls::read_inputs;
use crate::physics::original_vehicle::OriginalCar;

/// AI fields of a car outside [`OriginalCar`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiState {
    /// `car+0x8` locator (node, section `+0xc`, changed flags `+0x10/+0x11`).
    pub locator_08: TrackLocator,
    pub junction_e94: Option<usize>,
    pub branch_e98: i32,
    pub class_e9c: i32,
    pub skill_ea0: f32,
    pub target_eb8: f32,
    pub stuck_ea4: i32,
    pub pull_1078: f32,
    pub order_528: i32,
    pub progress_af8: f32,
    pub hit_1100: i32,
    /// Node frame copied by `0x499520` when the node changes: lateral
    /// `+0x3b8`, up `+0x3c4`, forward `+0x3d0`.
    pub node_lat_3b8: [f32; 3],
    pub node_up_3c4: [f32; 3],
    pub node_fwd_3d0: [f32; 3],
}

impl AiState {
    /// A fresh AI car at `node` (`0x406d50` resets: skill factor 1, the
    /// rest 0; section −1), with the node frame of `node`.
    pub fn at_node(track: &TrackNodes, node: i32) -> Self {
        let mut s = Self {
            skill_ea0: 1.0,
            locator_08: TrackLocator {
                node_00: node as i16,
                section_04: -1,
                ..Default::default()
            },
            ..Default::default()
        };
        s.copy_node_frame(track);
        s
    }

    fn copy_node_frame(&mut self, track: &TrackNodes) {
        let n = &track.nodes[self.locator_08.node_00 as usize];
        self.node_lat_3b8 = n.lateral_28;
        self.node_up_3c4 = n.up_10;
        self.node_fwd_3d0 = n.forward_1c;
    }
}

/// The world the AI reads: track graph, racing line, its car data and the
/// race options.
pub struct AiWorld<'a> {
    pub track: &'a TrackNodes,
    pub line: &'a RacingLine,
    pub ais: &'a AisData,
    pub tables: &'a TrackTables,
    pub res_skill_154: [f32; 8],
    pub skill_6573f4: i32,
    pub reverse_657430: bool,
    pub mode_6573e8: i32,
    pub running_606ac0: i32,
    pub player_af8: f32,
    pub player_b00: f32,
}

/// The part of `Car_TrackLocate` 0x49af70 the AI reads: the locator update
/// `0x474440` (its section is the AI's branch section `+0xc`) and the node
/// frame copy of `0x499520` when the node changed.
pub fn track_locate(car: &OriginalCar, s: &mut AiState, track: &TrackNodes) {
    s.locator_08.update(track, car.position_330);
    if s.locator_08.changed_09 {
        s.copy_node_frame(track);
    }
}

fn view(car: &OriginalCar, s: &AiState) -> AiCar {
    AiCar {
        node_08: s.locator_08.node_00,
        section_0c: s.locator_08.section_04,
        junction_e94: s.junction_e94,
        branch_e98: s.branch_e98,
        class_e9c: s.class_e9c,
        skill_ea0: s.skill_ea0,
        target_eb8: s.target_eb8,
        stuck_ea4: s.stuck_ea4,
        pull_1078: s.pull_1078,
        speed_35c: car.speed_35c,
        fwd_d60: car.vel_ground_d58[2],
        gear_d82: car.gear_d82,
        gear_req_d7e: car.gear_req_d7e,
        shift_d84: car.shift_timer_d84,
        pos_330: car.position_330,
        r_364: car.axes_364[0],
        f_37c: car.axes_364[2],
        half_w_3a8: car.half_extents_3a8[0],
        node_fwd_3d0: s.node_fwd_3d0,
        surface_464: car.surface_464,
        dmg_4e4: car.damage_4e4[0],
        dmg_4ec: car.damage_4e4[2],
        order_528: s.order_528,
        flags_52c: car.flags_52c as u32,
        progress_af8: s.progress_af8,
        state_b50: car.field_b50,
        start_db0: car.start_db0,
        spin_db8: car.spin_rear_db8,
        hit_1100: s.hit_1100,
        event_d8b: car.event_d8b,
        gas_d9c: car.ai_gas_d9c,
        brake_d9d: car.ai_brake_d9d,
        steer_da0: car.ai_steer_da0,
    }
}

fn store(car: &mut OriginalCar, s: &mut AiState, a: &AiCar) {
    s.locator_08.section_04 = a.section_0c;
    s.junction_e94 = a.junction_e94;
    s.branch_e98 = a.branch_e98;
    s.class_e9c = a.class_e9c;
    s.skill_ea0 = a.skill_ea0;
    s.target_eb8 = a.target_eb8;
    s.stuck_ea4 = a.stuck_ea4;
    s.pull_1078 = a.pull_1078;
    car.gear_d82 = a.gear_d82;
    car.gear_req_d7e = a.gear_req_d7e;
    car.shift_timer_d84 = a.shift_d84;
    car.event_d8b = a.event_d8b;
    car.ai_gas_d9c = a.gas_d9c;
    car.ai_brake_d9d = a.brake_d9d;
    car.ai_steer_da0 = a.steer_da0;
}

/// `Car_ReadInputs` 0x4991e0 for an AI car: `AI_TargetSpeed` 0x40b650,
/// `AI_Drive` 0x40bfe0, then the common input path (copies `d9c/d9d/da0`,
/// clears the horn, reads the event bits of `input_word`).
pub fn read_inputs_ai(
    car: &mut OriginalCar,
    s: &mut AiState,
    world: &AiWorld,
    order: &[OrderSlot],
    input_word: u32,
    lcg: &mut OriginalLcg,
) {
    car.ai_da8 = 1;
    let env = AiEnv {
        track: world.track,
        line: world.line,
        specs: &car.specs,
        ais: world.ais,
        tables: world.tables,
        res_skill_154: world.res_skill_154,
        skill_6573f4: world.skill_6573f4,
        reverse_657430: world.reverse_657430,
        mode_6573e8: world.mode_6573e8,
        step_606ac4: car.globals.sched_step_606ac4,
        running_606ac0: world.running_606ac0,
        autobahn_606ae0: car.globals.autobahn_606ae0 != 0,
        autobahn_alt_473330: car.globals.track_query_473330 == 0,
        player_af8: world.player_af8,
        player_b00: world.player_b00,
        order,
    };
    let mut a = view(car, s);
    target_speed(&mut a, &env, lcg);
    drive(&mut a, &env, lcg);
    store(car, s, &a);
    let specs = car.specs.clone();
    let (mut cc, ce) = car.controls_view(input_word);
    read_inputs(&mut cc, &specs, &ce);
    car.store_controls(&cc, &ce);
}

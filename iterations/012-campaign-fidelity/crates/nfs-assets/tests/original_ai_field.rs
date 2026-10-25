//! AI 6.1 smoke test: three AI cars and a parked player on the original
//! model race for 50 s on alps, autobahn and monaco1. Ground: the plane of
//! the nearest track node (a stand-in for the road surface). Every AI car
//! must make progress and stay near the road. Needs `local/game`.

use std::path::{Path, PathBuf};

use nfs_assets::physics::original_contact::PreparedContactState;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_pair::PairGlobals;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_vehicle::ground::{GroundAdapter, GroundHit, GroundQuery};
use nfs_assets::physics::original_vehicle::{OriginalCar, OriginalVehicle};
use nfs_assets::race::original_ai::field::{FieldQueue, OriginalAiField, OriginalAiTrack};
use nfs_assets::race::original_ai::track::TrackNodes;

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

struct NodeGround<'a> {
    t: &'a TrackNodes,
    hint: i32,
}

impl NodeGround<'_> {
    /// Height at `p` of node `k`'s plane.
    fn plane(&self, k: i32, p: [f32; 3]) -> f32 {
        let n = &self.t.nodes[k as usize];
        let u = n.up_10;
        n.pos_04[1] - (u[0] * (p[0] - n.pos_04[0]) + u[2] * (p[2] - n.pos_04[2])) / u[1]
    }
}

impl GroundQuery for NodeGround<'_> {
    /// The node planes blended along the track (no steps between nodes).
    fn wheel_ground(&mut self, _wheel: usize, p: [f32; 3]) -> Option<GroundHit> {
        self.hint = self.t.locate(p, self.hint);
        let last = self.t.nodes.len() as i32 - 1;
        let k = self.hint;
        let n = &self.t.nodes[k as usize];
        let f = n.forward_1c;
        let along = f[0] * (p[0] - n.pos_04[0]) + f[2] * (p[2] - n.pos_04[2]);
        let _ = last;
        // Graph neighbours, not index neighbours (sections are not stored in
        // track order).
        let (a, b) = if along >= 0.0 { (k, self.t.step(k, 1, -1)) } else { (self.t.step(k, -1, -1), k) };
        let pa = self.t.nodes[a as usize].pos_04;
        let pb = self.t.nodes[b as usize].pos_04;
        let seg = ((pb[0] - pa[0]).powi(2) + (pb[2] - pa[2]).powi(2)).sqrt().max(0.01);
        let fa = self.t.nodes[a as usize].forward_1c;
        let t = ((fa[0] * (p[0] - pa[0]) + fa[2] * (p[2] - pa[2])) / seg).clamp(0.0, 1.0);
        let h = self.plane(a, p) * (1.0 - t) + self.plane(b, p) * t;
        let ua = self.t.nodes[a as usize].up_10;
        Some(GroundHit::at(p, h, ua, 1))
    }
}

struct Queue<'a> {
    t: &'a TrackNodes,
    hints: Vec<i32>,
}

impl FieldQueue for Queue<'_> {
    fn ground(
        &mut self,
        index: usize,
        car: &mut OriginalCar,
        adapter: &mut GroundAdapter,
        _owners: &mut [OriginalSupportOwner; 4],
        lcg: &mut OriginalLcg,
    ) {
        if self.hints.len() <= index {
            self.hints.resize(index + 1, 0);
        }
        let mut g = NodeGround { t: self.t, hint: self.hints[index] };
        adapter.ground_step(car, &mut g, lcg);
        self.hints[index] = g.hint;
    }
    fn barrier(
        &mut self,
        _index: usize,
        _state: &mut PreparedContactState,
        _half: [f32; 3],
        _owner: Option<&mut OriginalSupportOwner>,
    ) -> Option<nfs_assets::physics::original_vehicle::driver::BarrierHit> {
        None
    }
}

fn axes_at(t: &TrackNodes, node: i32) -> [[f32; 3]; 3] {
    let f = t.nodes[node as usize].forward_1c;
    let l = (f[0] * f[0] + f[2] * f[2]).sqrt();
    let f = [f[0] / l, 0.0, f[2] / l];
    [[f[2], 0.0, -f[0]], [0.0, 1.0, 0.0], f]
}

#[test]
fn ai_cars_race_on_real_tracks() {
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let g = game();
    for track in ["alps", "autobahn", "monaco1"] {
        let crp = std::fs::read(g.join(format!("Track/{track}.crp"))).unwrap();
        let jnc = std::fs::read(g.join(format!("Track/{track}.jnc"))).ok();
        let lsp = std::fs::read(g.join(format!("Track/racer/work/{track}0.lsp"))).unwrap();
        let t = OriginalAiTrack::from_files(track, false, &crp, jnc.as_deref(), &lsp).unwrap();
        let sim = std::fs::read(g.join("Simulation/CarData/356Acoupe16.sim")).unwrap();
        let ais = std::fs::read(g.join("Simulation/AICarData/356A.ais")).unwrap();
        let half = [0.84, 0.6, 1.88];
        let wheels = [[-0.7, -0.6, 1.1], [0.7, -0.6, 1.1], [-0.7, -0.6, -1.0], [0.7, -0.6, -1.0]];
        let mut player = OriginalVehicle::from_sim(&sim, "356A", half, wheels).unwrap();
        let mut s140 = player.car.specs.field_140;
        let mut field = OriginalAiField::new(t, &ais, "356A", &mut s140);
        player.car.specs.field_140 = s140;
        let nodes = field.track.nodes.clone();
        let mut ground = NodeGround { t: &nodes, hint: 0 };
        // Player parked on the line ahead of the grid (AI cars must pass it).
        let p0 = nodes.nodes[40].pos_04;
        player.set_pose([p0[0], p0[1] + 1.0, p0[2]], axes_at(&nodes, 40), [0.0; 3], [0.0; 3], &mut ground);
        for (i, node) in [12, 18, 24].into_iter().enumerate() {
            let n = &nodes.nodes[node as usize];
            let lat = (i as f32 - 1.0) * 2.5;
            let p = [0, 1, 2].map(|k| n.pos_04[k] + n.lateral_28[k] * lat);
            field.spawn(player.clone(), [p[0], p[1] + 1.0, p[2]], axes_at(&nodes, node), &mut ground);
        }
        field.running = true;
        let mut globals = PairGlobals::default();
        let mut queue = Queue { t: &nodes, hints: Vec::new() };
        let start: Vec<i32> = field.cars.iter().map(|c| c.ai.locator_08.node_00 as i32).collect();
        let mut max_off = 0.0f32;
        let mut flipped = false;
        for st in 0..64 * 50 {
            field.step(&mut player, 0, &mut globals, &mut queue);
            if std::env::var("AI_TRACE").is_ok() && st % 64 == 0 {
                for (i, c) in field.cars.iter().enumerate() {
                    let k = &c.vehicle.car;
                    eprintln!("t {} car {i} node {} v {:.1} gas {} brk {} steer {} gear {} d84 {} up {:.2} h {:.2} y {:.1} tgt {:.1} d8b {} b50 {}", st / 64, c.ai.locator_08.node_00, k.vel_ground_d58[2], k.ai_gas_d9c, k.ai_brake_d9d, k.ai_steer_da0, k.gear_d82, k.shift_timer_d84, k.ground_up_3a0, k.height_424, k.position_330[1], c.ai.target_eb8, k.event_d8b, k.field_b50);
                }
            }
            for c in &field.cars {
                let n = &nodes.nodes[c.ai.locator_08.node_00 as usize];
                let p = c.vehicle.car.position_330;
                let d = ((p[0] - n.pos_04[0]).powi(2) + (p[2] - n.pos_04[2]).powi(2)).sqrt();
                if d > 25.0 && max_off <= 25.0 {
                    eprintln!("{track} step {st}: node {} section {} off {d:.1} pos {:?} nodepos {:?} junction {:?} branch {}", c.ai.locator_08.node_00, c.ai.locator_08.section_04, p, n.pos_04, c.ai.junction_e94, c.ai.branch_e98);
                }
                max_off = max_off.max(d);
                if c.vehicle.car.ground_up_3a0 < 0.3 && !flipped {
                    flipped = true;
                    eprintln!("{track} step {st}: car flipped at node {} (pos {:?}, player {:?})", c.ai.locator_08.node_00, p, player.car.position_330);
                }
            }
        }
        for (i, c) in field.cars.iter().enumerate() {
            let moved = c.laps * nodes.lap_nodes_04 + c.ai.locator_08.node_00 as i32 - start[i];
            eprintln!(
                "{track} car {i}: node {} (+{moved}), laps {}, speed {:.1} m/s, gear {}",
                c.ai.locator_08.node_00, c.laps, c.vehicle.car.speed_35c, c.vehicle.car.gear_d82
            );
            assert!(moved > 100, "{track} car {i} moved only {moved} nodes");
        }
        eprintln!("{track}: max distance from the node centre {max_off:.1} m");
        assert!(!flipped, "{track}: an AI car rolled over");
        assert!(max_off < 25.0, "{track}: an AI car left the road ({max_off} m)");
    }
}

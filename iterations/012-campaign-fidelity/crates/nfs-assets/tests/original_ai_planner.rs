//! AI 4.2/4.4: `AI_TargetSpeed` 0x40b650 and `AI_Drive` 0x40bfe0 on sampled
//! car states on real tracks, recorded by executing nfs5.exe in Unicorn
//! (scripts/research/replay-ai-planner.py), replayed through
//! `race::original_ai::planner`. Needs `local/game` (skipped otherwise).
//!
//! Spec: ai-drivers "AI target speed follows the corner ahead" and "AI gas,
//! brake and steering follow the original".

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::sim::{derive, CarSpecs, Tuning};
use nfs_assets::race::original_ai::ais::{load_ais, AisData, TrackTables};
use nfs_assets::race::original_ai::planner::{drive, target_speed, AiCar, AiEnv, AiOther, OrderSlot};
use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes};
use nfs_assets::race::original_ai::RacingLine;

static FIXTURE: Fixture = Fixture::new("ai-planner.tsv");

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn sim_for(model: &str) -> &'static str {
    match model {
        "356A" => "356Acoupe16",
        "993" => "993coupe436",
        "boxster" => "boxster25",
        "Gt2" => "GT2race",
        "356" => "356coupe11",
        "959" => "959coupe29",
        m => panic!("no sim for {m}"),
    }
}

struct Config {
    track: TrackNodes,
    line: RacingLine,
    specs: CarSpecs,
    ais: AisData,
    tables: TrackTables,
}

fn config(track: &str, reverse: bool, model: &str) -> Config {
    let g = game();
    let crp = std::fs::read(g.join(format!("Track/{track}.crp"))).unwrap();
    let crp = nfs_formats::decompress(&crp).unwrap();
    let (nodes, sections) = sim_chunks(&crp).unwrap();
    let jnc = std::fs::read(g.join(format!("Track/{track}.jnc"))).ok();
    let t = TrackNodes::build(nodes, &sections, jnc.as_deref());
    let line = RacingLine::from_bytes(
        &std::fs::read(g.join("Track/racer/work").join(RacingLine::file_name(track, reverse)))
            .unwrap(),
    );
    let raw = std::fs::read(g.join(format!("Simulation/CarData/{}.sim", sim_for(model)))).unwrap();
    let mut specs = derive(&raw, &Tuning::for_model(model));
    let file = std::fs::read(g.join(format!("Simulation/AICarData/{model}.ais"))).unwrap();
    let mut s140 = specs.field_140;
    let (ais, tables) = load_ais(&file, model, track, reverse, 1.0, &mut s140);
    specs.field_140 = s140;
    Config { track: t, line, specs, ais, tables }
}

struct Row<'a> {
    h: &'a HashMap<&'a str, usize>,
    c: Vec<&'a str>,
}

impl Row<'_> {
    fn s(&self, k: &str) -> &str {
        self.c[*self.h.get(k).unwrap_or_else(|| panic!("no column {k}"))]
    }
    fn i(&self, k: &str) -> i32 {
        self.s(k).parse().unwrap()
    }
    fn f(&self, k: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(self.s(k), 16).unwrap())
    }
    fn v(&self, k: &str) -> [f32; 3] {
        ["x", "y", "z"].map(|a| self.f(&format!("{k}_{a}")))
    }
    fn car(&self, p: &str) -> AiCar {
        let j = self.i(&format!("{p}junction_e94"));
        AiCar {
            node_08: self.i(&format!("{p}node_08")) as i16,
            section_0c: self.i(&format!("{p}section_0c")),
            junction_e94: (j >= 0).then_some(j as usize),
            branch_e98: self.i(&format!("{p}branch_e98")),
            class_e9c: self.i(&format!("{p}class_e9c")),
            skill_ea0: self.f(&format!("{p}skill_ea0")),
            target_eb8: self.f(&format!("{p}target_eb8")),
            stuck_ea4: self.i(&format!("{p}stuck_ea4")),
            pull_1078: self.f(&format!("{p}pull_1078")),
            speed_35c: self.f(&format!("{p}speed_35c")),
            fwd_d60: self.f(&format!("{p}fwd_d60")),
            gear_d82: self.i(&format!("{p}gear_d82")) as u8 as i8,
            gear_req_d7e: self.i(&format!("{p}gear_req_d7e")) as u8 as i8,
            shift_d84: self.i(&format!("{p}shift_d84")) as u8 as i8,
            pos_330: self.v(&format!("{p}pos_330")),
            r_364: self.v(&format!("{p}r_364")),
            f_37c: self.v(&format!("{p}f_37c")),
            half_w_3a8: self.f(&format!("{p}half_w_3a8")),
            node_fwd_3d0: self.v(&format!("{p}node_fwd_3d0")),
            surface_464: self.i(&format!("{p}surface_464")),
            dmg_4e4: self.f(&format!("{p}dmg_4e4")),
            dmg_4ec: self.f(&format!("{p}dmg_4ec")),
            order_528: self.i(&format!("{p}order_528")),
            flags_52c: self.i(&format!("{p}flags_52c")) as u32,
            progress_af8: self.f(&format!("{p}progress_af8")),
            state_b50: self.i(&format!("{p}state_b50")),
            start_db0: self.i(&format!("{p}start_db0")),
            spin_db8: self.i(&format!("{p}spin_db8")),
            hit_1100: self.i(&format!("{p}hit_1100")),
            event_d8b: self.i(&format!("{p}event_d8b")) as u8,
            gas_d9c: self.i(&format!("{p}gas_d9c")) as u8,
            brake_d9d: self.i(&format!("{p}brake_d9d")) as u8,
            steer_da0: self.i(&format!("{p}steer_da0")),
        }
    }
}

#[test]
fn planners_match_the_original() {
    skip_without_local_fixtures!(FIXTURE);
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let mut lines = FIXTURE.lines().filter(|l| !l.starts_with('#'));
    let head: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let h: HashMap<&str, usize> = head.iter().enumerate().map(|(i, &k)| (k, i)).collect();
    let mut configs: HashMap<String, Config> = HashMap::new();
    let (mut n, mut bad_t, mut bad_o) = (0, Vec::new(), Vec::new());
    for line in lines {
        let r = Row { h: &h, c: line.split('\t').collect() };
        let (track, reverse, model) = (r.s("track"), r.s("reverse") == "1", r.s("model"));
        let cfg = configs
            .entry(format!("{track}/{reverse}/{model}"))
            .or_insert_with(|| config(track, reverse, model));
        let mut car = r.car("");
        let rs = ["154", "158", "15c", "160", "164", "168", "16c", "170"].map(|k| r.f(&format!("res_{k}")));
        let mut others = r.s("others").split(' ').filter(|s| !s.is_empty()).map(|o| {
            let p: Vec<&str> = o.split(':').collect();
            let f = |i: usize| f32::from_bits(u32::from_str_radix(p[i], 16).unwrap());
            AiOther { pos_330: [f(0), f(1), f(2)], half_w_3a8: f(3), fwd_d60: f(4), node_08: p[5].parse().unwrap() }
        });
        let order: Vec<OrderSlot> = (0..4)
            .map(|i| if i == car.order_528 { OrderSlot::Me } else { OrderSlot::Other(others.next().unwrap()) })
            .collect();
        let env = AiEnv {
            track: &cfg.track,
            line: &cfg.line,
            specs: &cfg.specs,
            ais: &cfg.ais,
            tables: &cfg.tables,
            res_skill_154: rs,
            skill_6573f4: r.i("skill"),
            reverse_657430: reverse,
            mode_6573e8: r.i("mode"),
            step_606ac4: r.i("step"),
            running_606ac0: r.i("running"),
            autobahn_606ae0: false,
            autobahn_alt_473330: false,
            player_af8: r.f("player_af8"),
            player_b00: r.f("player_b00"),
            order: &order,
        };
        let mut lcg = OriginalLcg {
            state_5d1028: r.s("rand_state").parse().unwrap(),
            multiplier_5d102c: r.s("rand_mul").parse().unwrap(),
            product_655a0c: 0,
        };
        n += 1;
        target_speed(&mut car, &env, &mut lcg);
        let mid = (car.junction_e94.map_or(-1, |j| j as i32), car.branch_e98, car.section_0c,
                   car.class_e9c, car.skill_ea0.to_bits(), car.target_eb8.to_bits(), lcg.state_5d1028);
        let want_mid = (r.i("t_junction_e94"), r.i("t_branch_e98"), r.i("t_section_0c"), r.i("t_class_e9c"),
                        r.f("t_skill_ea0").to_bits(), r.f("t_target_eb8").to_bits(),
                        r.s("t_rand_state").parse::<u32>().unwrap());
        if mid != want_mid {
            bad_t.push(format!("row {n} {track} {model}: got {mid:?} want {want_mid:?}"));
            continue;
        }
        drive(&mut car, &env, &mut lcg);
        let want = r.car("o_");
        let want_state: u32 = r.s("o_rand_state").parse().unwrap();
        let mut got = car;
        // Compare float fields by bits (NaN-safe).
        let key = |c: &AiCar| format!("{:?} {:?}", (c.node_08, c.section_0c, c.junction_e94, c.branch_e98,
            c.class_e9c, c.skill_ea0.to_bits(), c.target_eb8.to_bits(), c.stuck_ea4), (c.pull_1078.to_bits(),
            c.gear_d82, c.gear_req_d7e, c.shift_d84, c.event_d8b, c.gas_d9c, c.brake_d9d, c.steer_da0));
        got.speed_35c = want.speed_35c;
        if key(&got) != key(&want) || lcg.state_5d1028 != want_state {
            bad_o.push(format!("row {n} {track} {model}:\n got  {}\n want {}", key(&got), key(&want)));
        }
    }
    let total = bad_t.len() + bad_o.len();
    assert!(
        total == 0,
        "{n} rows: {} target-speed and {} drive mismatches\n{}\n{}",
        bad_t.len(),
        bad_o.len(),
        bad_t.iter().take(8).cloned().collect::<Vec<_>>().join("\n"),
        bad_o.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}

//! Race progress, finish and race end recorded by executing nfs5.exe `Car_TrackProgressTick`
//! 0x4169b0 (`0x416310`, `0x417440`, `0x481b70`), `Race_OrderClear` 0x416140,
//! `Race_EndCheck` 0x417010 (`0x416ac0`) and `Race_Places` 0x415fc0 in Unicorn
//! (scripts/research/replay-race-finish.py), replayed through `race::finish`. Every row
//! must match bit for bit.
//!
//! Spec: race-finish "The race ends at the finish line".

use std::collections::HashMap;

use nfs_assets::race::finish::{LapTerms, RaceCar, RaceGlobals, RaceTable};

const STEPS: &str = include_str!("fixtures/race-finish-steps.tsv");

struct Row<'a>(HashMap<&'a str, &'a str>);

impl Row<'_> {
    fn s(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("no column {k}"))
    }
    fn i(&self, k: &str) -> i32 {
        self.s(k).parse().unwrap()
    }
    fn f(&self, k: &str) -> f32 {
        self.s(k).parse().unwrap()
    }
    fn ints(&self, k: &str) -> Vec<i32> {
        self.s(k).split(',').map(|v| v.parse().unwrap()).collect()
    }
    fn floats(&self, k: &str) -> Vec<f32> {
        self.s(k).split(',').map(|v| v.parse().unwrap()).collect()
    }
}

fn rows(text: &str) -> Vec<Row<'_>> {
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), header.len());
            Row(header.iter().copied().zip(cols).collect())
        })
        .collect()
}

/// One section from node 0 to `last`, fractions 0..1 (the recorder's node object).
struct OneSection(i32);

impl LapTerms for OneSection {
    fn terms(&self, _node: i32) -> (f32, f32, i32, i32) {
        (0.0, 1.0, self.0 + 1, 0)
    }
}

struct Case {
    cars: usize,
    laps: i32,
    linear: bool,
    nodes: i32,
    b00: f32,
    b04: f32,
    speech_busy: bool,
}

fn case(name: &str) -> Case {
    match name {
        "linear" => Case {
            cars: 4,
            laps: 1,
            linear: true,
            nodes: 200,
            b00: 0.05,
            b04: 0.95,
            speech_busy: false,
        },
        "circuit" => Case {
            cars: 3,
            laps: 3,
            linear: false,
            nodes: 100,
            b00: 0.0,
            b04: 1.0,
            speech_busy: false,
        },
        "linear_speech" => Case {
            cars: 2,
            laps: 1,
            linear: true,
            nodes: 200,
            b00: 0.05,
            b04: 0.95,
            speech_busy: true,
        },
        _ => panic!("unknown case {name}"),
    }
}

#[test]
fn race_progress_finish_and_end_match_the_original() {
    let all = rows(STEPS);
    let mut checked = 0;
    let mut i = 0;
    while i < all.len() {
        let name = all[i].s("case");
        let c = case(name);
        let terms = OneSection(c.nodes - 1);
        let cars = (0..c.cars)
            .map(|k| {
                let mut car = RaceCar::new(k as i32, k == 0, c.linear, c.b00, c.b04);
                if c.linear {
                    // The recorder writes the f32 constants 0.05, 0.95 and 0.045.
                    car.behind_b08 = 0.045;
                }
                car
            })
            .collect();
        let mut table = RaceTable::new(
            RaceGlobals {
                laps_6573f0: c.laps,
                linear_657494: c.linear,
                nodes_per_lap: c.nodes,
                ..Default::default()
            },
            cars,
        );
        // The recorder's initial order table (grid order, progress 0).
        for k in 0..c.cars {
            table.order[k].id = k as i32;
            table.order[k].progress = 0.0;
            table.order[k].step = 0;
            table.order[k].human = 0;
        }
        while i < all.len() && all[i].s("case") == name {
            let step = all[i].i("step");
            let group = &all[i..i + c.cars];
            table.globals.step_606ac4 = step;
            let mut events = vec![Vec::new(); c.cars];
            let mut end = Vec::new();
            if step % 2 == 0 {
                table.clear_order();
                for (k, r) in group.iter().enumerate() {
                    table.cars[k].node_08 = r.i("node");
                    table.cars[k].speed_d60 = r.f("speed_d60");
                    let ev = table.progress_tick(k, &terms);
                    if ev.reset {
                        events[k].push("reset");
                    }
                    if ev.lap_speech {
                        events[k].push("lap_speech");
                    }
                }
            } else {
                if table.end_check(!c.speech_busy) {
                    // `0x493cf0` (part wear) runs once per human.
                    end.push("wear");
                }
                table.places();
            }
            for (k, r) in group.iter().enumerate() {
                let car = &table.cars[k];
                let at = format!("{name} step {step} car {k}");
                assert_eq!(car.progress_af4.to_bits(), r.f("af4").to_bits(), "af4 {at}");
                assert_eq!(car.progress_af8.to_bits(), r.f("af8").to_bits(), "af8 {at}");
                assert_eq!(car.laps_530, r.i("laps_530"), "530 {at}");
                assert_eq!(car.back_534, r.i("back_534"), "534 {at}");
                assert_eq!(car.nodes_b10, r.i("b10"), "b10 {at}");
                assert_eq!(car.step_b14, r.i("b14"), "b14 {at}");
                assert_eq!(car.node_b18, r.i("b18"), "b18 {at}");
                assert_eq!(car.laps_b20, r.i("b20"), "b20 {at}");
                assert_eq!(car.time_b24, r.i("b24"), "b24 {at}");
                assert_eq!(car.lap_b28.to_vec(), r.ints("b28"), "b28 {at}");
                let top: Vec<u32> = r.floats("top_ad4").iter().map(|v| v.to_bits()).collect();
                assert_eq!(car.top_ad4.map(f32::to_bits).to_vec(), top, "ad4 {at}");
                assert_eq!(car.state_b50, r.i("state_b50"), "b50 {at}");
                assert_eq!(car.result_cd0, r.i("cd0"), "cd0 {at}");
                assert_eq!(car.place_cd4, r.i("place_cd4"), "cd4 {at}");
                assert_eq!(car.time_cd8, r.i("time_cd8"), "cd8 {at}");
                assert_eq!(car.best_cfc, r.i("best_cfc"), "cfc {at}");
                let o = &table.order[k];
                assert_eq!(o.id, r.i("order_id"), "order id {at}");
                assert_eq!(
                    o.progress.to_bits(),
                    r.f("order_af8").to_bits(),
                    "order af8 {at}"
                );
                assert_eq!(o.step, r.i("order_b14"), "order b14 {at}");
                assert_eq!(o.human as i32, r.i("order_human"), "order human {at}");
                assert_eq!(
                    table.globals.camera_606850,
                    r.i("camera_606850"),
                    "camera {at}"
                );
                assert_eq!(
                    table.globals.leave_606a8c as i32,
                    r.i("leave_606a8c"),
                    "leave {at}"
                );
                let calls = if events[k].is_empty() {
                    "-".to_string()
                } else {
                    events[k].join(" ")
                };
                assert_eq!(calls, r.s("calls"), "calls {at}");
                let end_calls = if end.is_empty() {
                    "-".to_string()
                } else {
                    end.join(" ")
                };
                assert_eq!(end_calls, r.s("end_calls"), "end calls {at}");
                checked += 1;
            }
            i += c.cars;
        }
    }
    assert!(checked > 5000, "{checked} rows");
}

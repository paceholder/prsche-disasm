//! Evolution progression replayed against the original nfs5.exe code
//! (`scripts/research/replay-career-progress.py`): Race_ApplyResults 0x4edf00 and the Tourn_*
//! functions on scripted careers. After every operation the career globals, the logged side
//! effects and the hash of the `Tournament` save payload (the whole table) must match.

#[macro_use]
mod support;

use nfs_game::career::{Career, CareerEvent, CarResult, RaceResults};
use support::career_fixture::{fnv64, int, parse};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-progress-synthetic.tsv");
static LOCAL_TRN: Fixture = Fixture::new("career-progress-trn.tsv");

fn event_name(e: &CareerEvent) -> &'static str {
    match e {
        CareerEvent::TrackRecords { .. } => "records",
        CareerEvent::RebuildCarLists => "rebuild",
        CareerEvent::RaceListRefresh => "races",
        CareerEvent::DebugPrint => "print",
        e => panic!("unexpected event {e:?}"),
    }
}

/// Replays every op; returns (ops, races, era changes, completions).
fn replay(text: &str) -> (usize, usize, usize, usize) {
    let fx = parse(text);
    let mut c = Career::new(&fx.table);
    let (mut odo, mut wins) = (0i32, 0i32);
    let (mut races, mut era_changes, mut completions) = (0, 0, 0);
    for row in &fx.rows {
        let args: Vec<&str> = if row["args"] == "-" { vec![] } else { row["args"].split(',').collect() };
        let a = |k: usize| -> i32 { args[k].parse().unwrap() };
        let ret: String = match row["op"].as_str() {
            "init" => {
                c = Career::new(&fx.table);
                odo = 0;
                wins = 0;
                "0".into()
            }
            "select" => {
                c.select_list(a(0));
                "0".into()
            }
            "allowed" => (c.car_allowed(a(0), a(1)) as i32).to_string(),
            "partly" => (c.other_tournament_partly_played() as i32).to_string(),
            "abandon" => {
                c.reset_abandoned();
                "0".into()
            }
            "launch" => match c.pay_entry_and_start() {
                Ok(()) => "1".into(),
                Err(e) => e.string_id().to_string(),
            },
            "addcash" => {
                c.add_cash(a(0));
                "0".into()
            }
            "race" => {
                let cars = args[1..]
                    .iter()
                    .map(|s| {
                        let v: Vec<i32> = s.split(':').map(|x| x.parse().unwrap()).collect();
                        CarResult { place: v[0], time: v[1], bonus_blocked: v[2] != 0 }
                    })
                    .collect();
                let mut r = RaceResults::tournament(cars);
                match args[0] {
                    "abort" => r.aborted = true,
                    "cheat_tourn" => r.debug_complete_tournament = true,
                    "cheat_era" => r.debug_complete_era = true,
                    _ => {}
                }
                let out = c.apply_race_results(&r);
                odo += out.odometer;
                wins += out.wins;
                races += 1;
                "0".into()
            }
            "standing" => c.standing(a(0), a(1)).to_string(),
            "eraquery" => format!(
                "{}:{}:{}",
                c.era_is_complete(a(0)) as i32,
                c.highest_completed_era().unwrap_or(0),
                c.era_bonus_record(a(0))
            ),
            "setyear" => {
                c.set_year(a(0));
                "0".into()
            }
            op => panic!("unknown op {op}"),
        };
        let calls: Vec<&str> = c.drain_events().iter().map(event_name).collect();
        let calls = if calls.is_empty() { "-".to_string() } else { calls.join(" ") };
        let ctx = format!("step {} {} {}", row["step"], row["op"], row["args"]);
        assert_eq!(ret, row["ret"], "ret, {ctx}");
        assert_eq!(
            [c.year, c.era, c.evolution_complete, c.cur_tourn, c.next_race, c.running_race, c.running_tourn],
            ["year", "era", "complete", "cur", "next", "run_race", "run_tourn"].map(|k| int(row, k)),
            "globals, {ctx}"
        );
        assert_eq!(c.era_changed as i32, int(row, "era_changed"), "era changed, {ctx}");
        assert_eq!([c.cash, odo, wins], ["cash", "odometer", "wins"].map(|k| int(row, k)), "cash/car, {ctx}");
        assert_eq!(calls, row["calls"], "side effects, {ctx}");
        assert_eq!(format!("{:016x}", fnv64(&c.save_tournament_section())), row["payload_fnv"], "table, {ctx}");
        era_changes += (row["era_changed"] == "1" && row["op"] == "race") as usize;
        completions += (row["complete"] == "1") as usize;
    }
    (fx.rows.len(), races, era_changes, completions)
}

#[test]
fn synthetic_career_matches_the_original() {
    let (ops, races, era_changes, completions) = replay(SYNTHETIC);
    assert!(ops > 1000 && races > 300, "{ops} ops, {races} races");
    assert!(era_changes >= 8 && completions > 0);
}

#[test]
fn local_career_matches_the_original() {
    skip_without_local_fixtures!(LOCAL_TRN);
    let (ops, races, _, completions) = replay(&LOCAL_TRN);
    assert!(ops > 500 && races > 150 && completions > 0, "{ops} ops, {races} races");
}

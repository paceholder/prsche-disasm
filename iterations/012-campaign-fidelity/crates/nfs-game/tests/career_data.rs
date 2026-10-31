//! `nfs5.trn` table and the `Tournament` save section against the original
//! `Tourn_SaveSection` 0x502ae0 (recorder `scripts/research/replay-career-progress.py`):
//! a new career's payload must hash like the original's.

#[macro_use]
mod support;

use nfs_game::career::{trn, Career};
use support::career_fixture::{fnv64, int, parse};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-progress-synthetic.tsv");
static LOCAL_TRN: Fixture = Fixture::new("career-progress-trn.tsv");

fn check_new_career(text: &str) -> usize {
    let fx = parse(text);
    let mut n = 0;
    for row in fx.rows.iter().filter(|r| r["op"] == "init") {
        let c = Career::new(&fx.table);
        assert_eq!(format!("{:016x}", fnv64(&c.save_tournament_section())), row["payload_fnv"]);
        assert_eq!((c.year, c.era, c.cur_tourn, c.next_race), (int(row, "year"), int(row, "era"), int(row, "cur"), int(row, "next")));
        assert_eq!((c.running_race, c.running_tourn, c.cash), (int(row, "run_race"), int(row, "run_tourn"), int(row, "cash")));
        n += 1;
    }
    assert!(n > 0);
    n
}

#[test]
fn synthetic_new_career_saves_like_the_original() {
    assert_eq!(check_new_career(SYNTHETIC), 4);
}

#[test]
fn local_new_career_saves_like_the_original() {
    skip_without_local_fixtures!(LOCAL_TRN);
    check_new_career(&LOCAL_TRN);
}

#[test]
fn local_trn_matches_the_documented_content() {
    skip_without_local_fixtures!(LOCAL_TRN);
    let fx = parse(&LOCAL_TRN);
    let c = Career::new(&fx.table);
    assert_eq!(c.table.len(), 35);
    // docs/re/career.md "Content"
    assert_eq!(c.list_ids(trn::ERA_CLASSIC), &[1, 9, 13, 20, 23]);
    assert_eq!(c.list_ids(trn::ERA_GOLDEN), &[2, 4, 24, 25, 26, 27]);
    assert_eq!(c.list_ids(trn::ERA_MODERN), &[15, 16, 18, 28, 29, 30]);
    let bonus: Vec<_> = (0..35).filter(|&i| c.table.kind(i) == 2).map(|i| (c.table.id(i), c.table.record(i).prize_car)).collect();
    assert_eq!(bonus, [(31, 168), (32, 169), (33, 170)]);
    let challenge = c.table.record(0);
    assert_eq!((challenge.id, challenge.entry_fee, challenge.races.len()), (1, 0, 2));
    assert_eq!((challenge.races[0].track, challenge.races[1].track), (14, 8));
    assert_eq!(challenge.overall_prize[0], 5000);
    assert_eq!(c.list_ids(trn::CLUB_CLASSIC).len() + c.list_ids(trn::CLUB_GOLDEN).len() + c.list_ids(trn::CLUB_MODERN).len(), 15);
}

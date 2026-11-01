//! Dealer prices, car lists, purchases and prize cars replayed against the original nfs5.exe
//! code (`scripts/research/replay-career-garage.py`): Car_DealerPrice 0x4ee220,
//! Career_RebuildCarLists 0x4f3e40, Garage_BuyDealerCar 0x501cf0, Garage_AddPrizeCar 0x502270.
//! Owned cars compare as their full 0x760-byte object image and part lists.

#[macro_use]
mod support;

use nfs_game::career::{dealer_price, CarCatalogue, Career, CareerEvent, OwnedCar, PartsCatalogue};
use support::career_fixture::{fnv64, int, parse_tables};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-garage-synthetic.tsv");
static LOCAL: Fixture = Fixture::new("career-garage-car.tsv");

fn car_dump(c: &OwnedCar) -> String {
    let ids = |l: &[nfs_game::career::PartRecord]| {
        if l.is_empty() { "-".to_string() } else { l.iter().map(|p| p.id().to_string()).collect::<Vec<_>>().join("/") }
    };
    let inst: Vec<u8> = c.installed.iter().flat_map(|p| p.bytes().to_vec()).collect();
    format!("{}:{:016x}:{}:{}:{:016x}", c.car_id(), fnv64(&c.image), ids(&c.installed), ids(&c.stock), fnv64(&inst))
}

fn garage_dump(c: &Career) -> String {
    let d: Vec<String> = c.garage.cars.iter().map(car_dump).collect();
    if d.is_empty() { "-".into() } else { d.join(";") }
}

fn lists_dump(c: &Career) -> String {
    let j = |l: &[usize]| l.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("/");
    let l = &c.car_lists;
    format!(
        "all={} model={} year={} f1={} f2={} dealer={} avail={}",
        j(&l.all), j(&l.model), j(&l.year), j(&l.flag1), j(&l.flag2), j(&l.dealer), j(&c.garage.available)
    )
}

fn replay(text: &str) -> (usize, usize) {
    let fx = parse_tables(text);
    let (cars, parts, present, rows) = (fx.cars, fx.parts, fx.present, fx.rows);
    let cat = CarCatalogue::from_bytes(&cars);
    let parts = PartsCatalogue::from_bytes(&parts);
    let mut c = Career::new(&[]);
    let mut bought = 0;
    for row in &rows {
        let args: Vec<&str> = if row["args"] == "-" { vec![] } else { row["args"].split(',').collect() };
        let a = |k: usize| -> i32 { args[k].parse::<i64>().unwrap() as i32 };
        let colours = || -> [i32; 6] { std::array::from_fn(|k| args[k + 1].parse::<u32>().unwrap() as i32) };
        let ctx = format!("step {} {} {}", row["step"], row["op"], row["args"]);
        let ret = match row["op"].as_str() {
            "price" => dealer_price(&cat.records[a(0) as usize], a(1)),
            "rebuild" => {
                c.year = a(0);
                c.rebuild_car_lists(&cat, 4, a(1) != 0, |m| present.contains(m));
                assert_eq!(lists_dump(&c), row["lists"], "lists, {ctx}");
                0
            }
            "buy" => match c.buy_dealer_car(&cat, &parts, a(0) as usize, colours(), 4) {
                Ok(()) => {
                    bought += 1;
                    0
                }
                Err(e) => e.string_id(),
            },
            "prize" => {
                c.add_prize_car(&cat, &parts, a(0), colours(), 4);
                0
            }
            "addcash" => {
                c.add_cash(a(0));
                0
            }
            "final" => {
                assert_eq!(garage_dump(&c), row["garage"], "{ctx}");
                0
            }
            op => panic!("unknown op {op}"),
        };
        assert_eq!(ret, int(row, "ret"), "ret, {ctx}");
        assert_eq!(c.cash, int(row, "cash"), "cash, {ctx}");
        if matches!(row["op"].as_str(), "buy" | "prize" | "rebuild") {
            let d = garage_dump(&c);
            assert_eq!(format!("{}:{:016x}", c.garage.cars.len(), fnv64(d.as_bytes())), row["garage"], "garage, {ctx}\n{d}");
        }
        let calls: Vec<String> = c
            .drain_events()
            .iter()
            .map(|e| match e {
                CareerEvent::RebuildTrackList => "tracks".to_string(),
                CareerEvent::Sound(_) => "sound".to_string(),
                e => panic!("unexpected {e:?}"),
            })
            .collect();
        assert_eq!(if calls.is_empty() { "-".to_string() } else { calls.join(" ") }, row["calls"], "calls, {ctx}");
    }
    (rows.len(), bought)
}

#[test]
fn synthetic_dealer_and_garage_match_the_original() {
    let (ops, bought) = replay(SYNTHETIC);
    assert!(ops > 300 && bought >= 78, "{ops} ops, {bought} bought");
}

#[test]
fn local_dealer_and_garage_match_the_original() {
    skip_without_local_fixtures!(LOCAL);
    let (ops, bought) = replay(&LOCAL);
    assert!(ops > 1000 && bought >= 78, "{ops} ops, {bought} bought");
}

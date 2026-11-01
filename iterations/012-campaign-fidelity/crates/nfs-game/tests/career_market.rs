//! The used-car market replayed against the original nfs5.exe code
//! (`scripts/research/replay-career-market.py`): UsedMarket_Refresh 0x4f7db0 with the
//! evaluation, generation and used-car creation, car values, listing, selling, buying.

#[macro_use]
mod support;

use nfs_game::career::used::{market_value, CrtRandom, GameRandom, MarketRandom, MarketSlot};
use nfs_game::career::{CarCatalogue, Career, OwnedCar, PartsCatalogue};
use support::career_fixture::{fnv64, int, parse_tables};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-market-synthetic.tsv");
static LOCAL: Fixture = Fixture::new("career-market-car.tsv");

fn car_dump(c: &OwnedCar) -> String {
    let inst: Vec<String> = c.installed.iter().map(|p| format!("{}={:?}", p.id(), p.condition() as f64)).collect();
    let stock: Vec<String> = c.stock.iter().map(|p| p.id().to_string()).collect();
    let j = |v: Vec<String>| if v.is_empty() { "-".to_string() } else { v.join("/") };
    format!("{}:{:016x}:{}:{}", c.car_id(), fnv64(&c.image), j(inst), j(stock))
}

fn summary<'a>(cars: impl Iterator<Item = &'a OwnedCar>) -> String {
    let d: Vec<String> = cars.map(car_dump).collect();
    let n = d.len();
    let d = if d.is_empty() { "-".to_string() } else { d.join(";") };
    format!("{}:{:016x}", n, fnv64(d.as_bytes()))
}

fn market_cars(c: &Career) -> Vec<&OwnedCar> {
    c.used_market
        .slots
        .iter()
        .map(|s| match s {
            MarketSlot::Market(car) => car,
            MarketSlot::Listed(h) => c.garage.cars.iter().find(|g| g.handle == *h).expect("listed car"),
        })
        .collect()
}

fn replay(text: &str) -> (usize, usize, usize) {
    let fx = parse_tables(text);
    let cat = CarCatalogue::from_bytes(&fx.cars);
    let parts = PartsCatalogue::from_bytes(&fx.parts);
    let mut c = Career::new(&[]);
    let mut game = GameRandom::default();
    let mut crt = CrtRandom::default();
    let (mut sold, mut bought) = (0, 0);
    for row in &fx.rows {
        let args: Vec<&str> = if row["args"] == "-" { vec![] } else { row["args"].split(',').collect() };
        let a = |k: usize| -> i64 { args[k].parse().unwrap() };
        let ctx = format!("step {} {} {}", row["step"], row["op"], row["args"]);
        let mut last: Option<OwnedCar> = None;
        let mut ret = 0i64;
        let mut r = MarketRandom { game: &mut game, crt: &mut crt, decal_count: 7 };
        match row["op"].as_str() {
            "crt" => r.crt.state = a(0) as u32,
            "cash" => c.cash = a(0) as i32,
            "rebuild" => {
                c.year = a(0) as i32;
                c.rebuild_car_lists(&cat, 4, false, |m| fx.present.contains(m));
            }
            "refresh" => c.refresh_used_market(&cat, &parts, a(0) as i32, a(1) as i32, &mut r),
            "value" => {
                let car = &c.garage.cars[a(0) as usize];
                ret = market_value(car, c.used_market.year) as i64;
                last = Some(car.clone());
            }
            "buydealer" => {
                ret = match c.buy_dealer_car(&cat, &parts, a(0) as usize, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66], 4) {
                    Ok(()) => 0,
                    Err(e) => e.string_id() as i64,
                }
            }
            "list" => {
                c.list_car_for_sale(a(0) as usize, a(1) as i32);
                last = Some(c.garage.cars[a(0) as usize].clone());
            }
            "unlist" => {
                c.unlist_car(a(0) as usize);
                last = Some(c.garage.cars[a(0) as usize].clone());
            }
            "decline" => {
                c.decline_offer(a(0) as usize);
                last = Some(c.garage.cars[a(0) as usize].clone());
            }
            "sell" => {
                c.sell_car(a(0) as usize, 4);
                sold += 1;
            }
            "buyused" => {
                let k = a(0) as usize;
                let h = market_cars(&c)[k].handle;
                ret = match c.buy_used_car(k, 4) {
                    Ok(()) => 0,
                    Err(e) => e.string_id() as i64,
                };
                if ret == 0 {
                    bought += 1;
                }
                last = c
                    .garage
                    .cars
                    .iter()
                    .chain(market_cars(&c).into_iter())
                    .find(|x| x.handle == h)
                    .cloned();
            }
            op => panic!("unknown op {op}"),
        }
        c.drain_events();
        assert_eq!(ret, row["ret"].parse::<i64>().unwrap(), "ret, {ctx}");
        assert_eq!(c.cash, int(row, "cash"), "cash, {ctx}");
        let rng: Vec<String> = game.s.iter().map(|w| format!("{w:08x}")).chain([format!("{:08x}", crt.state)]).collect();
        assert_eq!(rng.join(","), row["rng"], "rng, {ctx}");
        let m = market_cars(&c);
        assert_eq!(summary(m.iter().copied()), row["market"], "market, {ctx}\n{}", m.iter().map(|x| car_dump(x)).collect::<Vec<_>>().join("\n"));
        assert_eq!(summary(c.garage.cars.iter()), row["garage"], "garage, {ctx}");
        let l = last.as_ref().map_or("-".to_string(), car_dump);
        assert_eq!(l, row["last"], "last, {ctx}");
    }
    (fx.rows.len(), sold, bought)
}

#[test]
fn synthetic_used_market_matches_the_original() {
    let (ops, sold, bought) = replay(SYNTHETIC);
    assert!(ops > 150 && sold > 3 && bought > 5, "{ops} ops, {sold} sold, {bought} bought");
}

#[test]
fn local_used_market_matches_the_original() {
    skip_without_local_fixtures!(LOCAL);
    let (ops, sold, bought) = replay(&LOCAL);
    assert!(ops > 150 && sold > 3 && bought > 5, "{ops} ops, {sold} sold, {bought} bought");
}

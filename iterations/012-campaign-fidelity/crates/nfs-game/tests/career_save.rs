//! The career save sections PlayerInfo, UsedCarMarket and PartsMarket against the original
//! writers and readers (`scripts/research/replay-career-save.py`): the Rust readers load the
//! bytes the original wrote, the loaded state must match the original's, and saving it again
//! must give the original's second save.

#[macro_use]
mod support;

use nfs_game::career::used::MarketSlot;
use nfs_game::career::{Career, OwnedCar, PartsCatalogue};
use std::collections::HashMap;
use support::career_fixture::{fnv64, hex};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-save-synthetic.tsv");
static LOCAL: Fixture = Fixture::new("career-save-car.tsv");

fn car_dump(c: &OwnedCar) -> String {
    let inst: Vec<String> = c.installed.iter().map(|p| format!("{}={:?}", p.id(), p.condition() as f64)).collect();
    let stock: Vec<String> = c.stock.iter().map(|p| p.id().to_string()).collect();
    let j = |v: Vec<String>| if v.is_empty() { "-".to_string() } else { v.join("/") };
    format!("{}:{:016x}:{}:{}", c.car_id(), fnv64(&c.image), j(inst), j(stock))
}

fn joined(v: Vec<String>, sep: &str) -> String {
    if v.is_empty() { "-".into() } else { v.join(sep) }
}

fn replay(text: &str) -> usize {
    let mut parts = Vec::new();
    let (mut saved, mut loaded, mut resaved) = (HashMap::new(), HashMap::new(), HashMap::new());
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.splitn(3, '\t').collect();
        match f[0] {
            "part" => parts.extend(hex(f[2])),
            "saved" => {
                saved.insert(f[1].to_string(), hex(f[2]));
            }
            "loaded" => {
                loaded.insert(f[1].to_string(), f[2].to_string());
            }
            "resaved" => {
                resaved.insert(f[1].to_string(), f[2].to_string());
            }
            _ => {}
        }
    }
    let parts = PartsCatalogue::from_bytes(&parts);
    let mut c = Career::new(&[]);
    let p = &saved["PlayerInfo"];
    assert_eq!(c.load_player_section(p, &parts, 4), Ok(p.len()));
    let m = &saved["UsedCarMarket"];
    assert_eq!(c.load_used_market_section(m, &parts), Ok(m.len()));
    let s = &saved["PartsMarket"];
    assert_eq!(c.load_parts_section(s), Ok(s.len()));
    c.relist_cars(4);

    let pl = &c.player;
    assert_eq!(c.cash.to_string(), loaded["cash"]);
    assert_eq!(pl.driver.to_string(), loaded["driver"]);
    assert_eq!(String::from_utf8_lossy(&pl.name), loaded["name"]);
    assert_eq!(String::from_utf8_lossy(&pl.name2), loaded["name2"]);
    assert_eq!(format!("{},{},{}", pl.career_started, pl.flag_d94, pl.flag_df5), loaded["flags"]);
    assert_eq!(pl.ints.map(|v| v.to_string()).join(","), loaded["ints"]);
    assert_eq!(c.used_market.year.to_string(), loaded["market_year"]);
    assert_eq!(joined(c.garage.cars.iter().map(car_dump).collect(), ";"), loaded["garage"]);
    let market: Vec<String> = c
        .used_market
        .slots
        .iter()
        .map(|s| match s {
            MarketSlot::Market(car) => car_dump(car),
            MarketSlot::Listed(h) => car_dump(c.garage.cars.iter().find(|g| g.handle == *h).unwrap()),
        })
        .collect();
    assert_eq!(joined(market, ";"), loaded["market"]);
    let spares: Vec<String> = c.spare_parts.iter().map(|q| format!("{}:{:?}", q.id(), q.condition() as f64)).collect();
    assert_eq!(joined(spares, "/"), loaded["spares"]);

    for (name, bytes) in [
        ("PlayerInfo", c.save_player_section()),
        ("UsedCarMarket", c.save_used_market_section()),
        ("PartsMarket", c.save_parts_section()),
    ] {
        assert_eq!(format!("{}:{:016x}", bytes.len(), fnv64(&bytes)), resaved[name], "{name}");
    }
    c.garage.cars.len()
}

#[test]
fn synthetic_save_sections_match_the_original() {
    assert!(replay(SYNTHETIC) >= 8);
}

#[test]
fn local_save_sections_match_the_original() {
    skip_without_local_fixtures!(LOCAL);
    assert!(replay(&LOCAL) >= 8);
}

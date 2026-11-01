//! `Record_LoadZoneDamage` 0x4edd40 (career race damage back into the owned car: zone words,
//! part wear, the "Body" part) replayed against the original nfs5.exe code
//! (`scripts/research/replay-career-damage.py`, synthetic data).

#[macro_use]
mod support;

use nfs_game::career::{CarCatalogue, Career, OwnedCar, PartRecord, PartsCatalogue};
use support::career_fixture::{fnv64, parse_tables};

const SYNTHETIC: &str = include_str!("fixtures/career-damage-synthetic.tsv");

fn part_str(p: &PartRecord) -> String {
    format!("{}:{:08x}", p.id(), p.condition().to_bits())
}

fn car_str(o: &OwnedCar) -> String {
    let parts: Vec<String> = o.installed.iter().map(part_str).collect();
    let parts = if parts.is_empty() { "-".into() } else { parts.join("/") };
    let zones: Vec<String> = o.damage_zones().iter().map(|z| format!("{:08x}", z.to_bits())).collect();
    format!("{parts} {}", zones.join(","))
}

#[test]
fn race_damage_matches_the_original() {
    let fx = parse_tables(SYNTHETIC);
    let cat = CarCatalogue::from_bytes(&fx.cars);
    let parts = PartsCatalogue::from_bytes(&fx.parts);
    let mut c = Career::new(&[]);
    let mut applied = 0;
    let mut body_changed = 0;
    for row in &fx.rows {
        let args: Vec<&str> = row["args"].split(',').collect();
        let u = |k: usize| -> usize { args[k].parse().unwrap() };
        let f = |k: usize| -> f32 { args[k].parse::<f64>().unwrap() as f32 };
        let car = match row["op"].as_str() {
            "newcar" => {
                let alive = !c.garage.cars.is_empty();
                c.garage.cars.push(OwnedCar::new(&cat.records[u(0)], [1, 2, 3, 4, 5, 6], &parts, alive));
                c.garage.cars.len() - 1
            }
            "wear" => {
                c.garage.cars[u(0)].installed[u(1)].set_condition(f(2));
                u(0)
            }
            "racedamage" => {
                let zones: [f32; 10] = std::array::from_fn(|z| f(2 + z));
                let ids: [i32; 20] = std::array::from_fn(|k| args[12 + k].parse().unwrap());
                let wear: [f32; 20] = std::array::from_fn(|k| f(32 + k));
                let o = &mut c.garage.cars[u(0)];
                let before = o.installed.iter().find(|p| p.category() == 10).map(|p| p.condition());
                o.apply_race_damage(&zones, &ids, &wear);
                let after = o.installed.iter().find(|p| p.category() == 10).map(|p| p.condition());
                body_changed += usize::from(before != after);
                assert_eq!(o.race_damage_zones(4), zones, "Evolution carries the zones");
                assert_eq!(o.race_damage_zones(0), [0.0; 10], "Quick Race starts clean");
                applied += 1;
                u(0)
            }
            op => panic!("unknown op {op}"),
        };
        let cs = car_str(&c.garage.cars[car]);
        assert_eq!(format!("{:016x}", fnv64(cs.as_bytes())), row["car"], "step {} {}: {cs}", row["step"], row["op"]);
    }
    assert!(applied > 150 && body_changed > 100, "{applied} ops, body changed {body_changed}");
}

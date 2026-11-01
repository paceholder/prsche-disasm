//! Part prices, the parts market, spare parts, installing, fix/replace and the repair screen
//! replayed against the original nfs5.exe code (`scripts/research/replay-career-parts.py`).

#[macro_use]
mod support;

use nfs_game::career::parts::{inflated_price, part_value, repair_cost, replace_cost};
use nfs_game::career::{CarCatalogue, Career, OwnedCar, PartRecord, PartsCatalogue};
use support::career_fixture::{fnv64, int, parse_tables};
use support::local_fixtures::Fixture;

const SYNTHETIC: &str = include_str!("fixtures/career-parts-synthetic.tsv");
static LOCAL: Fixture = Fixture::new("career-parts-car.tsv");

fn part_str(p: &PartRecord) -> String {
    format!("{}:{:?}", p.id(), p.condition() as f64)
}

fn join(v: impl Iterator<Item = String>) -> String {
    let v: Vec<String> = v.collect();
    if v.is_empty() { "-".into() } else { v.join("/") }
}

fn car_str(c: &Career, car: Option<usize>) -> String {
    let Some(k) = car else { return "-".into() };
    let o = &c.garage.cars[k];
    let zones: Vec<String> = (0..10).map(|z| format!("{:?}", o.f32(0x6a8 + 4 * z) as f64)).collect();
    format!("{} {}", join(o.installed.iter().map(part_str)), zones.join(","))
}

fn replay(text: &str) -> usize {
    let fx = parse_tables(text);
    let cat = CarCatalogue::from_bytes(&fx.cars);
    let parts = PartsCatalogue::from_bytes(&fx.parts);
    let mut c = Career::new(&[]);
    c.cash = 5_000_000;
    let mut fixes = 0;
    for row in &fx.rows {
        let args: Vec<&str> = if row["args"] == "-" { vec![] } else { row["args"].split(',').collect() };
        let a = |k: usize| -> i32 { args[k].parse().unwrap() };
        let u = |k: usize| -> usize { args[k].parse().unwrap() };
        let f = |k: usize| -> f32 { args[k].parse::<f64>().unwrap() as f32 };
        let ctx = format!("step {} {} {}", row["step"], row["op"], row["args"]);
        let mut car = None;
        let ret: String = match row["op"].as_str() {
            "price" => {
                let mut p = parts.records[u(0)].clone();
                p.set_condition(f(2));
                let y = a(1);
                format!("{}:{}:{}:{}", inflated_price(&p, y), part_value(&p, y), repair_cost(&p, y), replace_cost(&p, y))
            }
            "newcar" => {
                let alive = !c.garage.cars.is_empty();
                c.garage.cars.push(OwnedCar::new(&cat.records[u(0)], [1, 2, 3, 4, 5, 6], &parts, alive));
                car = Some(c.garage.cars.len() - 1);
                c.garage.cars.len().to_string()
            }
            "market" => join(c.parts_market(&c.garage.cars[u(0)], &parts, a(1)).iter().map(|k| k.to_string())),
            "fitting" => join(c.fitting_spares(&c.garage.cars[u(0)]).iter().map(|k| k.to_string())),
            "listed" => join(c.listed_spares().iter().map(|k| k.to_string())),
            op => {
                match op {
                    "buy" => {
                        c.year = a(1);
                        c.buy_part(&parts.records[u(0)], a(1));
                    }
                    "sell" => {
                        c.year = a(1);
                        c.sell_spare_part(u(0), a(1));
                    }
                    "install" => {
                        c.install_part(u(0), u(1));
                        car = Some(u(0));
                    }
                    "uninstall" => {
                        c.uninstall_part(u(0), u(1));
                        car = Some(u(0));
                    }
                    "wear" => {
                        c.garage.cars[u(0)].installed[u(1)].set_condition(f(2));
                        car = Some(u(0));
                    }
                    "zones" => {
                        for z in 0..10 {
                            c.garage.cars[u(0)].set_f32(0x6a8 + 4 * z, f(z + 1));
                        }
                        car = Some(u(0));
                    }
                    "fix" | "replace" => {
                        c.year = a(2);
                        if op == "fix" {
                            c.fix_installed_part(u(0), u(1), a(2));
                        } else {
                            c.replace_installed_part(u(0), u(1), a(2));
                        }
                        fixes += 1;
                        car = Some(u(0));
                    }
                    "cost" => {
                        c.year = a(2);
                        car = Some(u(0));
                        let r = c.repair_group_cost(u(0), u(1));
                        assert_eq!(r, int(row, "ret"), "cost, {ctx}");
                    }
                    "repair" => {
                        c.year = a(2);
                        c.repair_group(u(0), u(1));
                        car = Some(u(0));
                    }
                    op => panic!("unknown op {op}"),
                }
                row["ret"].clone()
            }
        };
        assert_eq!(ret, row["ret"], "ret, {ctx}");
        assert_eq!(c.cash, int(row, "cash"), "cash, {ctx}");
        let spares = join(c.spare_parts.iter().map(part_str));
        assert_eq!(format!("{:016x}", fnv64(spares.as_bytes())), row["spares"], "spares, {ctx}: {spares}");
        let cs = car_str(&c, car);
        assert_eq!(format!("{:016x}", fnv64(cs.as_bytes())), row["car"], "car, {ctx}: {cs}");
    }
    fixes
}

#[test]
fn synthetic_parts_match_the_original() {
    assert!(replay(SYNTHETIC) > 5);
}

#[test]
fn local_parts_match_the_original() {
    skip_without_local_fixtures!(LOCAL);
    assert!(replay(&LOCAL) > 5);
}

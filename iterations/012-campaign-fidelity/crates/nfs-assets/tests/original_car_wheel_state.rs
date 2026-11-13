//! The Direct3D 7 state of the car wheel draws (the `CarWheel` page: tyre and rim, the 4444
//! wheel shadow quads), recorded by `scripts/research/replay-car-wheel-draw.py`: nfs5.exe and
//! Drivers/dx7/dx7z.dll in Unicorn.
//!
//! - `fixtures/thrash-z-alpha.tsv` (synthetic, committed): `THRASH_setstate@8` 0x60003ca0 of the
//!   driver for the z and alpha-test ids (4, 0x28, 0x39, 0x24, 0x25) and the material ids the
//!   wheels use, on a fresh driver with and without the device caps; `render_state::Dx7Driver`
//!   replays every sequence (D3D calls and return values) exactly.
//! - `local/fixtures/car-wheel-draw.tsv` (game data, `record-local-fixtures.sh`): the race view's
//!   state prefix (0x537700, 0x537890, 0x468ae0's 0x537870 / 0x537830), the real LODs
//!   (0x43adb0 + 0x43c1b0) and the exterior passes 0x444e00 -> `R3DCarLOD_DrawParts` 0x43c830 ->
//!   0x44a250 + 0x53f1b0 of the 356A and the 993, with the render state in effect at every
//!   DrawIndexedPrimitive. Checked: every wheel draw has the state `render_state::car_part_states`
//!   builds from the car method block, and that is what the viewer draws the loader's wheel
//!   materials with (`viewer_car_states`: `BlendTest`, cutoff 10/255, single-sided); the draw
//!   order of the wheel parts (part order, `pr` order in a part, wheels after the body).
//!
//! Spec: original-car-materials (wheel page render state).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use local_fixtures::Fixture;
use nfs_assets::render_state::{
    car_part_states, viewer_car_states, D3dCall, D3dStates, DriverCaps, Dx7Driver, CAR_METHOD_BLOCK,
    CAR_PART_STATES,
};
use nfs_assets::AssetFiles;

const MAP: &str = include_str!("fixtures/thrash-z-alpha.tsv");
static DRAWS: Fixture = Fixture::new("car-wheel-draw.tsv");

struct Row<'a> {
    kind: &'a str,
    case: &'a str,
    key: &'a str,
    out: &'a str,
}

fn rows(text: &str) -> Vec<Row<'_>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("kind\t"))
        .map(|l| {
            let c: Vec<&str> = l.splitn(4, '\t').collect();
            Row { kind: c[0], case: c[1], key: c[2], out: c[3] }
        })
        .collect()
}

fn fields(s: &str) -> HashMap<&str, &str> {
    s.split(';').filter_map(|p| p.split_once('=')).collect()
}

/// `rs:a=b|tss:s,t=v|...` as the recorder writes D3D calls.
fn calls(s: &str) -> Vec<D3dCall> {
    if s == "-" {
        return vec![];
    }
    s.split('|')
        .map(|c| {
            let (kind, rest) = c.split_once(':').unwrap();
            let (lhs, v) = rest.split_once('=').unwrap();
            let v: u32 = v.parse().unwrap();
            match kind {
                "rs" => D3dCall::RenderState(lhs.parse().unwrap(), v),
                "tss" => {
                    let (a, b) = lhs.split_once(',').unwrap();
                    D3dCall::TextureStage(a.parse().unwrap(), b.parse().unwrap(), v)
                }
                _ => panic!("{c}"),
            }
        })
        .collect()
}

fn hal() -> DriverCaps {
    DriverCaps::HAL
}

#[test]
fn thrash_z_and_alpha_states_match_the_recorded_driver() {
    let mut checked = 0;
    for r in rows(MAP) {
        let caps = match r.kind {
            "map1" => hal(),
            "map0" => DriverCaps::default(),
            _ => continue,
        };
        let mut driver = Dx7Driver::new(caps);
        let results: Vec<String> = r
            .case
            .split(',')
            .map(|step| {
                let (id, v) = step.split_once('=').unwrap();
                driver.set_state(u32::from_str_radix(id, 16).unwrap(), v.parse().unwrap()).to_string()
            })
            .collect();
        assert_eq!(results.join(","), r.key, "{} {}", r.kind, r.case);
        assert_eq!(driver.d3d, calls(r.out), "{} {}", r.kind, r.case);
        checked += 1;
    }
    assert!(checked >= 45, "{checked}");
}

#[test]
fn car_part_state_is_z_tested_z_written_alpha_tested_and_blended() {
    let s = car_part_states(&CAR_METHOD_BLOCK, hal());
    assert_eq!(s, viewer_car_states(nfs_assets::AlphaMode::BlendTest, 10.0 / 255.0, false));
    // Plain `Blend` (no depth write, no alpha test) is not the original.
    assert_ne!(s, viewer_car_states(nfs_assets::AlphaMode::Blend, 0.0, false));
}

/// `2=0,7=1,...` of a `draw` row, limited to the car part states.
fn recorded_states(s: &str) -> D3dStates {
    s.split(',')
        .map(|kv| kv.split_once('=').unwrap())
        .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
        .filter(|(k, _)| CAR_PART_STATES.contains(k))
        .collect()
}

fn car_files(car: &str) -> AssetFiles {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/CarModel");
    let mut files = AssetFiles::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        if name == format!("{car}.crp") || name == format!("{car}.tpg") || name.ends_with(".fsh") {
            files.insert(name, std::fs::read(path).unwrap());
        }
    }
    files
}

#[test]
fn local_wheel_draws_match_the_recorded_state_and_order() {
    skip_without_local_fixtures!(DRAWS);
    let all = rows(&DRAWS);
    // The loader's wheel materials, by car and `mt` index.
    let mut viewer: HashMap<String, BTreeMap<String, D3dStates>> = HashMap::new();
    let mut prefix_checked = 0;
    let (mut draws, mut wheel_draws, mut shadow_draws) = (0, 0, 0);
    let mut binds: HashMap<&str, Vec<HashMap<&str, &str>>> = HashMap::new();
    let expected = car_part_states(&CAR_METHOD_BLOCK, hal());
    for r in &all {
        let car = r.case.split('/').next().unwrap();
        match r.kind {
            "prefix" => {
                // The view's prefix as the port sends it (the z ids only).
                let mut driver = Dx7Driver::new(hal());
                for (id, v) in [(0x28, 3), (4, 1), (0x39, 1), (0x39, 1), (0x28, 3)] {
                    driver.set_state(id, v);
                }
                let got: Vec<D3dCall> = calls(r.out)
                    .into_iter()
                    .filter(|c| matches!(c, D3dCall::RenderState(s, _) if CAR_PART_STATES.contains(s)))
                    .collect();
                assert_eq!(got, driver.d3d, "{}", r.case);
                prefix_checked += 1;
            }
            "bind" => binds.entry(r.case).or_default().push(fields(r.out)),
            "draw" => {
                let x = fields(r.out);
                let sources: Vec<&str> = x["src"].split('|').collect();
                draws += 1;
                if !sources.iter().any(|s| s.contains(":CarWheel:")) {
                    continue;
                }
                // A batch holds draws of one state: the wheel page is never batched with others.
                assert!(sources.iter().all(|s| s.contains(":CarWheel:")), "{} {}", r.case, x["src"]);
                let recorded = recorded_states(x["rs"]);
                assert_eq!(recorded, expected, "{} {}", r.case, r.key);
                let materials = viewer.entry(car.to_string()).or_insert_with(|| {
                    let scene = nfs_assets::load_car(&car_files(car), car).unwrap();
                    scene
                        .materials
                        .iter()
                        .filter(|m| m.name.ends_with(":CarWheel"))
                        .map(|m| {
                            (m.name.clone(), viewer_car_states(m.alpha_mode, m.alpha_cutoff, m.double_sided))
                        })
                        .collect()
                });
                for s in &sources {
                    // tag = mask:part:article:pr:material:method:pass
                    let t: Vec<&str> = s.split(':').collect();
                    let name = format!("mt{}:CarWheel", t[4]);
                    assert_eq!(materials[&name], recorded, "{} {} {name}", r.case, r.key);
                    if t[2].starts_with("WheelShadow") {
                        shadow_draws += 1;
                    } else {
                        wheel_draws += 1;
                    }
                }
                // Every wheel vertex is opaque (the light layer's alpha 255).
                assert!(x["diffuse"].split(',').all(|w| w.starts_with("ff")), "{}", r.case);
            }
            _ => {}
        }
    }
    assert_eq!(prefix_checked, 12);
    assert!(draws > 1000 && wheel_draws >= 60 && shadow_draws >= 8, "{draws} {wheel_draws} {shadow_draws}");
    // Draw order: the body pass (mask 0x50) before the wheels (0x20); the wheel parts in LOD
    // part order, each part's `pr` groups in index order (tyre and rim are one part, no sort).
    for (case, b) in &binds {
        let wheels: Vec<&HashMap<&str, &str>> = b.iter().filter(|x| x["mask"] == "20").collect();
        assert!(!wheels.is_empty(), "{case}");
        let first_wheel = b.iter().position(|x| x["mask"] == "20").unwrap();
        assert!(b[..first_wheel].iter().any(|x| x["mask"] == "50"), "{case}");
        assert!(b[first_wheel..].iter().all(|x| x["mask"] == "20"), "{case}");
        let key = |x: &HashMap<&str, &str>| -> (u32, u32) { (x["part"].parse().unwrap(), x["pr"].parse().unwrap()) };
        assert!(wheels.windows(2).all(|w| key(w[0]) < key(w[1])), "{case}");
        assert!(wheels.iter().all(|x| x["method"] == "CarWheel" && u32::from_str_radix(x["mt_flags"], 16).unwrap() & 0x20 != 0));
    }
}

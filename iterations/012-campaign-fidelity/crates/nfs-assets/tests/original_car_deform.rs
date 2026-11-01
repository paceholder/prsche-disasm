//! `R3DCar_UpdateDamage` 0x438230 / `R3DCarLOD_ApplyDamageMorph` 0x43e390 / `R3DCar_LightCap`
//! 0x4382e0 against recordings of the original (`scripts/research/replay-car-deform.py`):
//! a synthetic CRP (committed) and shipped cars (local fixture). Every step compares the zone
//! cache, the heavy-damage bytes, whether the texture update ran and the FNV-1a 64 of every
//! part's vertex array, bit for bit.
#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
use local_fixtures::Fixture;

use nfs_assets::car_damage::{CarDamageModel, DamageView, MorphPart};
use nfs_formats::{parse_crp, Crp};

static CARS: Fixture = Fixture::new("car-deform-cars.tsv");

fn fnv64(parts: &MorphPart) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in &parts.vertices {
        for x in v {
            for b in x.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        }
    }
    h
}

fn f(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

/// Base level table (`Base+0x08..+0x0c` range of 12-byte entries at `+0x64`, u16 level at +8),
/// as R3DCarLOD init 0x43c1b0 reads it.
fn levels(article: &nfs_formats::Entry) -> Vec<u16> {
    let d = &article.find("Base", 0).unwrap().data;
    let w = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize;
    (w(8)..w(12))
        .map(|k| u16::from_le_bytes(d[0x64 + 12 * k + 8..0x64 + 12 * k + 10].try_into().unwrap()))
        .collect()
}

/// The recorder's part list: per level (ascending), the articles listing it that have `vt`.
fn model(crp: &Crp) -> (CarDamageModel, Vec<(usize, u16, usize)>) {
    let mut all: Vec<u16> = crp.articles.iter().flat_map(levels).collect();
    all.sort();
    all.dedup();
    let mut parts = Vec::new();
    let mut ids = Vec::new();
    for level in all {
        for (ai, article) in crp.articles.iter().enumerate() {
            if !levels(article).contains(&level) {
                continue;
            }
            if let Some(part) = MorphPart::from_article(article, level).unwrap() {
                ids.push((ai, level, part.base.len()));
                parts.push(part);
            }
        }
    }
    (CarDamageModel::new(parts), ids)
}

fn replay<'a>(
    label: &str,
    crp: Crp,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) {
    let (mut m, ids) = model(&crp);
    let mut k = 0;
    while let Some(line) = lines.next_if(|l| l.starts_with("part\t")) {
        let c: Vec<&str> = line.split('\t').collect();
        let want = (
            c[2].parse().unwrap(),
            c[3].parse().unwrap(),
            c[4].parse().unwrap(),
        );
        assert_eq!(ids[k], want, "{label}: part {k}");
        k += 1;
    }
    assert_eq!(k, ids.len(), "{label}: part count");
    let mut expected: Vec<u64> = vec![0; ids.len()];
    let mut steps = 0;
    while let Some(line) = lines.next_if(|l| l.starts_with("step\t")) {
        let c: Vec<&str> = line.split('\t').collect();
        let n: usize = c[1].parse().unwrap();
        let (car, viewed, camera, flags, detail): (u32, u32, i16, u8, i32) = (
            c[2].parse().unwrap(),
            c[3].parse().unwrap(),
            c[4].parse().unwrap(),
            c[5].parse().unwrap(),
            c[6].parse().unwrap(),
        );
        let zones: [f32; 10] = std::array::from_fn(|i| f(c[7 + i]));
        let cache: [u32; 10] = std::array::from_fn(|i| f(c[17 + i]).to_bits());
        let view = DamageView {
            viewed: car == viewed,
            camera_id: camera,
            in_car: flags & 8 != 0,
        };
        let ran = m.update(&zones, view, detail, false).is_some();
        assert_eq!(m.cache.map(f32::to_bits), cache, "{label} step {n}: cache");
        let heavy: String = m
            .parts
            .iter()
            .map(|p| if p.heavy { '1' } else { '0' })
            .collect();
        assert_eq!(heavy, c[27], "{label} step {n}: heavy bytes");
        assert_eq!(ran, c[28] == "1", "{label} step {n}: texture update");
        while let Some(b) = lines.next_if(|l| l.starts_with("buf\t")) {
            let c: Vec<&str> = b.split('\t').collect();
            expected[c[2].parse::<usize>().unwrap()] = u64::from_str_radix(c[3], 16).unwrap();
        }
        for (p, part) in m.parts.iter().enumerate() {
            assert_eq!(
                fnv64(part),
                expected[p],
                "{label} step {n}: part {p} {:?}",
                ids[p]
            );
        }
        steps += 1;
    }
    assert!(steps > 40, "{label}: {steps} steps");
}

#[test]
fn synthetic_crp_matches_the_original() {
    let text = include_str!("fixtures/car-deform-synthetic.tsv");
    let mut lines = text.lines().filter(|l| !l.starts_with('#')).peekable();
    let hex = lines.next().unwrap().strip_prefix("crp\t").unwrap();
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    replay("synthetic", parse_crp(&bytes).unwrap(), &mut lines);
    assert!(lines.next().is_none());
}

#[test]
fn shipped_cars_match_the_original() {
    skip_without_local_fixtures!(CARS);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/CarModel");
    let mut lines = CARS.lines().filter(|l| !l.starts_with('#')).peekable();
    let mut cars = 0;
    while let Some(line) = lines.next() {
        let car = line.strip_prefix("car\t").expect("car line");
        let crp = parse_crp(&std::fs::read(dir.join(format!("{car}.crp"))).unwrap()).unwrap();
        replay(car, crp, &mut lines);
        cars += 1;
    }
    assert_eq!(cars, 3);
}

/// The scene loader's damage parts rebuild the drawn mesh positions exactly from the base
/// arrays, and full damage moves them (356a, installed game).
#[test]
fn loaded_car_maps_morph_vertices_to_meshes() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/CarModel");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("SKIPPED: game not installed");
        return;
    };
    let mut files = nfs_assets::AssetFiles::new();
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        if name == "356a.crp" || name == "356a.tpg" || name.ends_with(".fsh") {
            files.insert(name, std::fs::read(path).unwrap());
        }
    }
    let scene = nfs_assets::load_car(&files, "356a").unwrap();
    assert!(
        scene.car_damage.len() > 20,
        "{} damage parts",
        scene.car_damage.len()
    );
    let mut moved = 0;
    for d in &scene.car_damage {
        let mut full = d.part.clone();
        let mut model = CarDamageModel::new(vec![full.clone()]);
        model.update(&[100.0; 10], DamageView::default(), 2, false);
        full.vertices = model.parts[0].vertices.clone();
        for (k, (mesh, _)) in d.meshes.iter().enumerate() {
            let base = d.mesh_positions(k, &d.part.base);
            let drawn: Vec<[f32; 3]> = scene.meshes[*mesh]
                .vertices
                .iter()
                .map(|v| v.position)
                .collect();
            assert_eq!(base, drawn, "{} mesh {k}", d.name);
            moved += usize::from(d.mesh_positions(k, &full.vertices) != base);
        }
    }
    assert!(moved > 20, "{moved} meshes moved at full damage");
    // The in-car parts (BumperCam2) carry their morph too.
    let mut in_car = 0;
    for part in &scene.car_in_car {
        let Some(d) = &part.damage else { continue };
        for (k, mesh) in part.meshes.iter().enumerate() {
            let drawn: Vec<[f32; 3]> = mesh.mesh.vertices.iter().map(|v| v.position).collect();
            assert_eq!(d.mesh_positions(k, &d.part.base), drawn, "{} in-car mesh {k}", part.name);
            in_car += 1;
        }
    }
    assert!(in_car > 0, "no in-car damage meshes");
}

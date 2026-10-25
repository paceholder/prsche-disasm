//! Raw CRP bounding box per model level (no /5 scale, no Z flip).
//! Usage: crp_level_bounds <game-dir> <car>...
//! Reads GameData/CarModel/<car>.crp. Per level: article count, bbox of the
//! vertices referenced by that level's `pr` parts (plus `tr` translation), and
//! the bbox of every vertex in the level's `vt` arrays (plus `tr`).
use nfs_assets::collision_box::level_bounds;
use nfs_formats::{f32le, parse_crp, Entry};
use std::{collections::BTreeMap, env, fs};

type Bounds = [[f32; 3]; 2];
const EMPTY: Bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];

fn grow(b: &mut Bounds, p: [f32; 3]) {
    for k in 0..3 {
        b[0][k] = b[0][k].min(p[k]);
        b[1][k] = b[1][k].max(p[k]);
    }
}

fn name(a: &Entry) -> String {
    a.find("Name", 0)
        .map(|e| {
            let end = e.data.iter().position(|&c| c == 0).unwrap_or(e.data.len());
            String::from_utf8_lossy(&e.data[..end]).into_owned()
        })
        .unwrap_or_else(|| "?".into())
}

fn show(label: &str, b: &Bounds) {
    if b[0][0] > b[1][0] {
        println!("  {label}: empty");
        return;
    }
    let half: Vec<f32> = (0..3).map(|k| 0.5 * (b[1][k] - b[0][k])).collect();
    println!(
        "  {label}: min {:?} max {:?}\n    half raw {:.3?}  half/5 {:.3?}  full raw {:.3?}",
        b[0],
        b[1],
        half,
        half.iter().map(|h| h / 5.0).collect::<Vec<_>>(),
        half.iter().map(|h| h * 2.0).collect::<Vec<_>>()
    );
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let dir = args
        .first()
        .ok_or("usage: crp_level_bounds <game-dir> <car>...")?;
    for car in &args[1..] {
        let path = format!("{dir}/GameData/CarModel/{car}.crp");
        let crp = parse_crp(&fs::read(&path).map_err(|e| format!("{path}: {e}"))?)
            .map_err(|e| format!("{path}: {e}"))?;
        println!("== {car}: {} articles", crp.articles.len());
        let referenced = level_bounds(&crp)?;
        let mut whole: BTreeMap<u16, Bounds> = BTreeMap::new();
        let mut names: BTreeMap<u16, Vec<String>> = BTreeMap::new();
        let mut raw_idx: BTreeMap<u16, std::collections::BTreeSet<u16>> = BTreeMap::new();
        for a in &crp.articles {
            let n = name(a);
            for vt in a.children.iter().filter(|e| e.tag == "vt") {
                // Only plain level indices (undamaged, type 0), as the viewer loader
                // selects them; higher bits are variant/damage selectors.
                if vt.index > 0xf {
                    continue;
                }
                let lvl = vt.index;
                let t = a.find("tr", vt.index);
                let tr = match t {
                    Some(t) => [
                        f32le(&t.data, 48)?,
                        f32le(&t.data, 52)?,
                        f32le(&t.data, 56)?,
                    ],
                    None => [0.; 3],
                };
                raw_idx.entry(lvl).or_default().insert(vt.index);
                let list = names.entry(lvl).or_default();
                if !list.contains(&n) {
                    list.push(n.clone());
                }
                let w = whole.entry(lvl).or_insert(EMPTY);
                for v in vt.data.as_chunks::<16>().0 {
                    let p = [f32le(v, 0)?, f32le(v, 4)?, f32le(v, 8)?];
                    grow(w, [p[0] + tr[0], p[1] + tr[1], p[2] + tr[2]]);
                }
            }
        }
        for (lvl, list) in &names {
            println!(
                "level {lvl}: {} articles (vt index {:?})",
                list.len(),
                raw_idx[lvl]
            );
            println!("  articles: {}", list.join(", "));
            show("pr-referenced", referenced.get(lvl).unwrap_or(&EMPTY));
            show("all vt", &whole[lvl]);
        }
    }
    Ok(())
}

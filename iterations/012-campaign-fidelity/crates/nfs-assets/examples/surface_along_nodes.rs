//! Surface nibbles (`g_GripTable` rows) across the road at every track node, with the
//! snow and rain keys of the sky config there (`docs/re/weather.md`).
//!
//! Usage: `cargo run -p nfs-assets --example surface_along_nodes -- <GameData/Track> <track>`
//! Prints one line per run of nodes with the same set of nibbles.

use std::{collections::BTreeMap, env, fs, path::Path};

use nfs_assets::physics::original_adapter::{from_original, to_original};
use nfs_assets::{load_track, AssetFiles};

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let [root, track] = args.as_slice() else {
        return Err("usage: surface_along_nodes <GameData/Track> <track>".into());
    };
    let root = Path::new(root);
    let mut files = AssetFiles::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.to_ascii_lowercase().starts_with(&track.to_ascii_lowercase()) && entry.path().is_file() {
            files.insert(name, fs::read(entry.path()).map_err(|e| e.to_string())?);
        }
    }
    for shared in ["animdefs.txt".to_string(), format!("racer/work/{track}0.lsp")] {
        if let Ok(data) = fs::read(root.join(&shared)) {
            files.insert(shared.rsplit('/').next().unwrap_or(&shared).to_string(), data);
        }
    }
    for entry in fs::read_dir(root.join("Sky")).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
        if stem == track.to_ascii_lowercase() {
            files.insert(format!("Sky/{name}"), fs::read(entry.path()).map_err(|e| e.to_string())?);
        }
    }
    let scene = load_track(&files, track)?;
    let surface = scene.road_surface.as_ref().ok_or("no road surface")?;
    let nodes = &scene.ai_track.as_ref().ok_or("no track nodes")?.nodes;
    let weather = scene.sky_weather.as_ref();
    let n = nodes.nodes.len();
    let mut runs: Vec<(usize, usize, String)> = Vec::new();
    let mut totals: BTreeMap<u16, usize> = BTreeMap::new();
    // Per nibble and snow flag: samples and the nearest lateral offset from the centre line.
    let mut near: BTreeMap<(u16, bool), (usize, f32)> = BTreeMap::new();
    for (i, node) in nodes.nodes.iter().enumerate() {
        let mut seen = BTreeMap::new();
        for k in -8..=8 {
            let side = k as f32 * 0.5;
            let p = [0, 1, 2].map(|a| node.pos_04[a] + node.lateral_28[a] * side);
            let s = from_original(p);
            let Some(hit) = surface.query(s.x, s.z, s.y, 2.0, 4.0) else { continue };
            let Some(flags) = hit.flags_word_a else { continue };
            *seen.entry(flags & 0xf).or_insert(0usize) += 1;
            *totals.entry(flags & 0xf).or_insert(0) += 1;
            let snowy = weather.is_some_and(|w| w.at(i as i32, n as i32).snow > 0.1);
            let e = near.entry((flags & 0xf, snowy)).or_insert((0, f32::MAX));
            e.0 += 1;
            e.1 = e.1.min(side.abs());
            let _ = to_original(s);
        }
        let w = weather.map(|w| w.at(i as i32, n as i32)).unwrap_or_default();
        let key = format!(
            "{:?}{}",
            seen.keys().collect::<Vec<_>>(),
            if w.snow > 0.1 { " snow" } else if w.rain > 0.1 { " rain" } else { "" }
        );
        match runs.last_mut() {
            Some((_, end, k)) if *k == key && *end + 1 == i => *end = i,
            _ => runs.push((i, i, key)),
        }
    }
    if env::var("RUNS").is_ok() {
        for (a, b, k) in runs {
            println!("nodes {a:5}..{b:5}  nibbles {k}");
        }
    }
    for ((nib, snowy), (count, d)) in &near {
        println!("nibble {nib:2} {:7} samples {count:6}, nearest {d:.1} m from the centre line", if *snowy { "snow" } else { "no snow" });
    }
    println!("samples per nibble (8 m either side of the centre line): {totals:?}");
    Ok(())
}

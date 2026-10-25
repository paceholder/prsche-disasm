//! List the selected parts of a car scene: mesh name, material, size, bounds.
//! Usage: car_parts <CarModel dir> <car id>
use nfs_assets::{load_car, AssetFiles};
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let root = Path::new(args.get(1).ok_or("expected CarModel directory")?);
    let car = args.get(2).ok_or("expected car id")?;
    let mut files = AssetFiles::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        if name == format!("{car}.crp") || name == format!("{car}.tpg") || name.ends_with(".fsh") {
            files.insert(name, fs::read(path)?);
        }
    }
    if args.iter().any(|a| a == "--all") {
        let crp = nfs_formats::parse_crp(&files[&format!("{car}.crp")])?;
        let dump_filter = args.iter().skip_while(|a| a.as_str() != "--dump").nth(1).cloned().unwrap_or_else(|| "\u{0}".into());
        for (i, art) in crp.articles.iter().enumerate() {
            let name = art
                .children
                .iter()
                .find(|e| e.tag == "Name")
                .map(|e| String::from_utf8_lossy(e.data.split(|b| *b == 0).next().unwrap()).into_owned())
                .unwrap_or_default();
            let vts: Vec<String> = art
                .children
                .iter()
                .filter(|e| e.tag == "vt")
                .map(|e| format!("{:04x}", e.index))
                .collect();
            let prs: Vec<String> = art
                .children
                .iter()
                .filter(|e| e.tag == "pr")
                .map(|e| format!("{:04x}", e.index))
                .collect();
            println!("{i:3} {name:24} vt {vts:?} pr {prs:?}");
            if args.iter().any(|a| a == "--dump") && (vts.is_empty() || name.contains(dump_filter.as_str())) {
                for e in &art.children {
                    if e.tag == "vt" || e.tag == "nm" || e.tag == "uv" || e.tag == "pr" { continue; }
                    let hex: Vec<String> = e.data.iter().take(160).map(|b| format!("{b:02x}")).collect();
                    println!("      {} #{} n{} len {}: {}", e.tag, e.index, e.count, e.data.len(), hex.join(""));
                }
            }
        }
        for e in &crp.misc {
            let hex: Vec<String> = e.data.iter().take(48).map(|b| format!("{b:02x}")).collect();
            println!("misc {} #{} n{} len {}: {}", e.tag, e.index, e.count, e.data.len(), hex.join(""));
        }
        return Ok(());
    }
    let scene = load_car(&files, car)?;
    for g in &scene.car_glares {
        println!(
            "glare {:30} group {:2} kind {:2} colors {:08x}/{:08x} beam {} pivot {} pos {:?} |A| {:.3} n {:?}",
            g.name, g.group, g.kind, g.colors[0], g.colors[1], g.beam, g.pivot,
            g.position.map(|x| (x * 100.0).round() / 100.0),
            g.axis_a.iter().map(|x| x * x).sum::<f32>().sqrt(),
            g.normal.map(|x| (x * 100.0).round() / 100.0)
        );
    }
    if args.iter().any(|a| a == "--glares") {
        return Ok(());
    }
    for (i, mesh) in scene.meshes.iter().enumerate() {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &mesh.vertices {
            for a in 0..3 {
                lo[a] = lo[a].min(v.position[a]);
                hi[a] = hi[a].max(v.position[a]);
            }
        }
        let m = &scene.materials[mesh.material];
        println!(
            "{i:3} {:24} mat {:3} {:28} {:?} tris {:5} lo {:?} hi {:?}",
            mesh.name,
            mesh.material,
            m.name,
            m.alpha_mode,
            mesh.indices.len() / 3,
            lo.map(|x| (x * 5.0 * 100.0).round() / 100.0),
            hi.map(|x| (x * 5.0 * 100.0).round() / 100.0)
        );
    }
    Ok(())
}

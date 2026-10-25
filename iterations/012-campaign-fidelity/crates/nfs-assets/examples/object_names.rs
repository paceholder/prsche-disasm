//! Collision objects with article names and whether their tag matched an animdefs entry.
//! Usage: object_names <track.crp> <animdefs.txt> [name-substring]
use nfs_assets::physics::original_track_object_build::{ObjectShape, build_initial_track_objects};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let objs = build_initial_track_objects(&crp, &defs)?;
    let mut g: std::collections::BTreeMap<(String, bool, bool, bool), (u32, Vec<String>)> = Default::default();
    for o in &objs {
        let name = crp.articles[o.article_index].find("Name", 0).map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string()).unwrap_or_default();
        let matched = defs.definitions.iter().any(|d| d.tag == o.tag);
        let tag = String::from_utf8_lossy(&o.tag.to_le_bytes()).trim_end_matches('\0').to_string();
        let e = g.entry((tag, matches!(o.shape, ObjectShape::Box { .. }), matched, o.drive_through)).or_default();
        e.0 += 1;
        if e.1.len() < 4 { e.1.push(format!("{name} {:.1}x{:.1}x{:.1}", o.bounds_max[0]-o.bounds_min[0], o.bounds_max[1]-o.bounds_min[1], o.bounds_max[2]-o.bounds_min[2])); }
    }
    for ((tag, boxk, matched, thru), (n, ex)) in &g { println!("{tag:5} {} box={boxk} matched={matched} x{n}  e.g. {ex:?}", if *thru { "DRIVE-THROUGH" } else { "solid" }); }
    Ok(())
}

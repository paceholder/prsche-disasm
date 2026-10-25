//! Collision objects built from a track CRP. Usage: object_census <track.crp> <animdefs.txt> [-v]
use nfs_assets::physics::original_track_object_build::{ObjectShape, build_initial_track_objects};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let objects = build_initial_track_objects(&crp, &defs)?;
    let mut counts = std::collections::BTreeMap::new();
    for o in &objects {
        let t = String::from_utf8_lossy(&o.tag.to_le_bytes()).to_string();
        let k = (
            t,
            matches!(o.shape, ObjectShape::Box { .. }),
            o.drive_through,
        );
        *counts.entry(k).or_insert(0usize) += 1;
        if a.get(3).is_some_and(|s| s == "-v") {
            println!(
                "{:?} art {} id {} min {:?} max {:?} {:?}",
                String::from_utf8_lossy(&o.tag.to_le_bytes()),
                o.article_index,
                o.id,
                o.bounds_min,
                o.bounds_max,
                o.shape
            );
        }
    }
    println!("{} objects", objects.len());
    for (k, v) in counts {
        println!("{k:?} x{v}");
    }
    Ok(())
}

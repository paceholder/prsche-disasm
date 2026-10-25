//! Smackable records of a track CRP. Usage: smack_census <track.crp> <animdefs.txt> [-v]
use nfs_assets::physics::original_smackable::build_initial_smack_records;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let records = build_initial_smack_records(&crp, &defs)?;
    let mut counts = std::collections::BTreeMap::new();
    for r in &records {
        *counts.entry(String::from_utf8_lossy(&r.tag.to_le_bytes()).to_string()).or_insert(0) += 1;
        if a.get(3).is_some_and(|s| s == "-v") {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &r.hull {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            println!("{} art {} pos {:?} axes {:?} hull {} box {:?}..{:?} mass {} type {:#x}", String::from_utf8_lossy(&r.tag.to_le_bytes()), r.article_index, r.position, r.axes, r.hull.len(), lo, hi, r.mass, r.type_44c);
        }
    }
    println!("{} records {counts:?}", records.len());
    Ok(())
}

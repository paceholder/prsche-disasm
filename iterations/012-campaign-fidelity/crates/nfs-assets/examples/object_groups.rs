//! Collision objects grouped by tag and article name (digits stripped) with size ranges.
//! Usage: object_groups <track.crp> <animdefs.txt>
use nfs_assets::physics::original_track_object_build::build_initial_track_objects;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let objs = build_initial_track_objects(&crp, &defs)?;
    let mut g: std::collections::BTreeMap<(String, String), (u32, [f32; 3], [f32; 3])> = Default::default();
    for o in &objs {
        let name = crp.articles[o.article_index].find("Name", 0).map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string()).unwrap_or_default();
        // "CNK0320R (7 OAKCAGE_COLLIDE02)" -> "OAKCAGE_COLLIDE"
        let base = name.rsplit_once(' ').map_or(name.as_str(), |x| x.1).trim_end_matches(')').trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
        let tag = String::from_utf8_lossy(&o.tag.to_le_bytes()).trim_end_matches('\0').to_string();
        let size = [0, 1, 2].map(|i| o.bounds_max[i] - o.bounds_min[i]);
        let e = g.entry((tag, base)).or_insert((0, [f32::MAX; 3], [0.0; 3]));
        e.0 += 1;
        for i in 0..3 { e.1[i] = e.1[i].min(size[i]); e.2[i] = e.2[i].max(size[i]); }
    }
    for ((tag, name), (n, lo, hi)) in &g {
        println!("{tag:5} {name:28} x{n:<4} size {:.1}x{:.1}x{:.1} .. {:.1}x{:.1}x{:.1}{}", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], "");
    }
    Ok(())
}

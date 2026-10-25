//! Articles whose name contains a substring: tag, Base word 0, mesh kinds, bounds size.
//! Usage: name_search <track.crp> <substring>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let pat = a[2].to_ascii_lowercase();
    let mut groups: std::collections::BTreeMap<String, (u32, String, f32)> = Default::default();
    for art in &crp.articles {
        let n = art.find("Name", 0).map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string()).unwrap_or_default();
        if !n.to_ascii_lowercase().contains(&pat) { continue; }
        let base = art.find("Base", 0).unwrap();
        let w0 = u32::from_le_bytes(base.data[0..4].try_into().unwrap());
        let tag = String::from_utf8_lossy(&base.data[0x44..0x48]).trim_end_matches('\0').to_string();
        let f = |o: usize| f32::from_le_bytes(base.data[o..o + 4].try_into().unwrap());
        let key = n.rsplit_once(' ').map_or(n.clone(), |x| x.1.trim_end_matches(')').trim_end_matches(|c: char| c.is_ascii_digit()).to_string());
        let e = groups.entry(key).or_insert((0, format!("tag {tag:?} w0 {w0:#x} kids {:?}", art.children.iter().map(|c| format!("{}{}", c.tag, c.index)).collect::<Vec<_>>()), f(0x20) - f(0x14)));
        e.0 += 1;
    }
    for (k, (n, d, w)) in groups { println!("{k:30} x{n:<4} width {w:.1} {d}"); }
    Ok(())
}

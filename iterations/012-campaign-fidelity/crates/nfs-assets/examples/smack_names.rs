fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let recs = nfs_assets::physics::original_smackable::build_initial_smack_records(&crp, &defs)?;
    let mut names = std::collections::BTreeMap::new();
    for r in &recs {
        let n = crp.articles[r.article_index].find("Name", 0).map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string()).unwrap_or_default();
        *names.entry(n).or_insert(0) += 1;
    }
    println!("{names:?}");
    // All articles by tag whose tag looks sign-like but are not records
    let mut other = std::collections::BTreeMap::new();
    for (i, art) in crp.articles.iter().enumerate() {
        let n = art.find("Name", 0).map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string()).unwrap_or_default();
        let l = n.to_ascii_lowercase();
        if (l.contains("sign") || l.contains("sgn")) && !recs.iter().any(|r| r.article_index == i) {
            let base = art.find("Base", 0).unwrap();
            let w0 = u32::from_le_bytes(base.data[0..4].try_into().unwrap());
            let tag = String::from_utf8_lossy(&base.data[0x44..0x48]).to_string();
            *other.entry((n, format!("{w0:#x}"), tag)).or_insert(0) += 1;
        }
    }
    println!("non-record sign-like: {other:?}");
    Ok(())
}

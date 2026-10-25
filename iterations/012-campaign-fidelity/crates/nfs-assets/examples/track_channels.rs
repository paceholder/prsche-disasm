//! Inventory of track CRP article entries and `pr` channel kinds.
//! Usage: track_channels <track.crp> [article-substring]
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&args[1])?)?;
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<u16, usize> = BTreeMap::new();
    let mut shown = 0;
    for a in &crp.articles {
        for c in &a.children {
            *tags.entry(c.tag.clone()).or_default() += 1;
            if c.tag == "pr" && c.data.len() >= 48 {
                let ni = u32::from_le_bytes(c.data[40..44].try_into()?) as usize;
                for i in 0..ni.min(64) {
                    let o = 48 + i * 16;
                    if o + 16 <= c.data.len() {
                        let k = u16::from_le_bytes(c.data[o + 10..o + 12].try_into()?);
                        *kinds.entry(k).or_default() += 1;
                    }
                }
            }
        }
        if shown < 2 && a.children.iter().any(|c| c.tag == "pr") {
            shown += 1;
            for c in &a.children {
                let hex: String = c.data.iter().take(96).map(|b| format!("{b:02x}")).collect();
                println!("  {} #{} n{} len {} {}", c.tag, c.index, c.count, c.data.len(), hex);
            }
        }
    }
    let mut hist = [[0usize; 8]; 4];
    let mut sum = [0f64; 4];
    let mut n = 0f64;
    for a in &crp.articles {
        for c in a.children.iter().filter(|c| c.tag == "df") {
            for px in c.data.chunks_exact(4) {
                for k in 0..4 {
                    hist[k][(px[k] >> 5) as usize] += 1;
                    sum[k] += px[k] as f64;
                }
                n += 1.0;
            }
        }
    }
    let (mut blue, mut warm) = (0usize, 0usize);
    for a in &crp.articles {
        for c in a.children.iter().filter(|c| c.tag == "df") {
            for px in c.data.chunks_exact(4) {
                let (b, r) = (px[0] as i32, px[2] as i32);
                if b - r > 30 { blue += 1 }
                if r - b > 30 { warm += 1 }
            }
        }
    }
    println!("df byte0-byte2 > 30: {:.0}%, byte2-byte0 > 30: {:.0}%", 100.0 * blue as f64 / n, 100.0 * warm as f64 / n);
    println!("df byte means {:?} (bytes 0..3)", sum.map(|s| (s / n).round()));
    println!("df byte histograms (/32) {hist:?}");
    println!("tags {tags:?}");
    println!("pr channel kinds {kinds:?}");
    println!("misc: {:?}", crp.misc.iter().map(|m| format!("{}#{} n{} len{}", m.tag, m.index, m.count, m.data.len())).collect::<Vec<_>>());
    Ok(())
}

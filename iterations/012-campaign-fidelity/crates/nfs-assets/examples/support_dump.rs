//! Dumps the original type-1 support polygons of tracks (the retained
//! flag-1 assembly of `0x4750b0`, in insertion order) for the Unicorn
//! recorder `scripts/research/replay-road-support.py`.
//!
//! ```text
//! cargo run --release -p nfs-assets --example support_dump -- \
//!     <game dir> <out dir> autobahn alps ...
//! ```
//!
//! Writes `<out>/<track>-polygons.tsv`: index, vertex count, 12 vertex
//! coordinates as f32 hex bits (original axes; zero for the fourth vertex of
//! a triangle) and the polygon flags word (`+0xa`).

use std::path::PathBuf;

use nfs_assets::physics::original_support::SupportVertices;
use nfs_assets::physics::original_support_loader::assemble_initial_track_support;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [game, out, tracks @ ..] = args.as_slice() else {
        eprintln!("usage: support_dump <game dir> <out dir> <track>...");
        std::process::exit(2);
    };
    let dir = PathBuf::from(game).join("GameData/Track");
    let anim = nfs_formats::animdefs::parse_animdefs(
        &std::fs::read_to_string(dir.join("animdefs.txt")).expect("animdefs.txt"),
    )
    .expect("animdefs");
    std::fs::create_dir_all(out).expect("out dir");
    for track in tracks {
        let raw = std::fs::read(dir.join(format!("{track}.crp"))).expect("crp");
        let crp = nfs_formats::track::parse_track_crp(&raw).expect("parse crp");
        let load = assemble_initial_track_support(&crp, &anim).expect("support");
        let mut text = String::from("index\tcount\tvertices\tflags\n");
        for (i, p) in load.polygons.iter().enumerate() {
            let (n, v): (usize, Vec<[f32; 3]>) = match &p.polygon.vertices {
                SupportVertices::Triangle(v) => (3, v.to_vec()),
                SupportVertices::Quad(v) => (4, v.to_vec()),
            };
            let mut bits = Vec::with_capacity(12);
            for k in 0..4 {
                for c in 0..3 {
                    bits.push(format!("{:08x}", v.get(k).map_or(0, |p| p[c].to_bits())));
                }
            }
            text += &format!("{i}\t{n}\t{}\t{}\n", bits.join(","), p.polygon.flags_word_a);
        }
        let path = PathBuf::from(out).join(format!("{track}-polygons.tsv"));
        std::fs::write(&path, text).expect("write");
        println!(
            "{track}: {} polygons -> {}",
            load.polygons.len(),
            path.display()
        );
    }
}

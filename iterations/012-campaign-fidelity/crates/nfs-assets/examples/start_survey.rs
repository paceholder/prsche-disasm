//! Start placement of every track and direction (`docs/re/reset.md` "Race start").
//! Usage: start_survey <GameData/Track dir> [racers]
use nfs_assets::race::original_ai::{start, track::{sim_chunks, TrackNodes}};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&args[1]);
    let racers: i32 = args.get(2).map_or(8, |s| s.parse().unwrap());
    let tracks = ["alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills",
        "forest", "industrial", "monaco1", "monaco2", "monaco3", "monaco4", "monaco5", "skidpad"];
    for t in tracks {
        let crp = nfs_formats::decompress(&std::fs::read(dir.join(format!("{t}.crp"))).unwrap()).unwrap();
        let (nb, sec) = sim_chunks(&crp).unwrap();
        let jnc = std::fs::read(dir.join(format!("{t}.jnc"))).ok();
        let nodes = TrackNodes::build(nb, &sec, jnc.as_deref());
        let lsp = std::fs::read(dir.join(format!("racer/work/{t}0.lsp"))).unwrap();
        let pts: Vec<(f32, f32)> = lsp[4..].chunks_exact(8).map(|c| (
            f32::from_le_bytes(c[0..4].try_into().unwrap()), f32::from_le_bytes(c[4..8].try_into().unwrap()))).collect();
        let (f, l) = (pts[0], pts[pts.len() - 1]);
        let gap = ((f.0 - l.0).powi(2) + (f.1 - l.1).powi(2)).sqrt();
        let linear = gap >= 60.0;
        println!("== {t}: {} nodes, lsp {} pts first ({:.0},{:.0}) last ({:.0},{:.0}) linear {linear}", nodes.nodes.len(), pts.len(), f.0, f.1, l.0, l.1);
        for (tag, rev) in [("F", false), ("B", true)] {
            let scn = std::fs::read_to_string(dir.join(format!("{t}_{}start.scn", tag.to_lowercase()))).ok();
            let Some(trig) = scn.as_deref().and_then(start::start_trigger) else {
                let first = start::circuit_first(&nodes, rev);
                match start::lap_route(&nodes, first, rev) {
                    Some(r) => println!("  {tag}: no start trigger; lap from node {first}: {} nodes", r.len()),
                    None => println!("  {tag}: no start trigger; NO LAP from node {first}"),
                }
                continue;
            };
            if let Some(text) = scn.as_deref() {
                if let Ok(f) = nfs_formats::parse_scn(text) {
                    for tr in f.triggers.iter().filter(|t| t.trigger_type == Some(1)) {
                        println!("  {tag}: finish trigger node {} of sections {:?}", nodes.locate(tr.position, 1), nodes.sections.iter().map(|s| (s.first(), s.last())).collect::<Vec<_>>());
                    }
                }
            }
            let sn = start::start_node(&nodes, trig);
            if let Some(f) = scn.as_deref().and_then(start::finish_trigger) {
                let fnode = start::start_node(&nodes, f);
                match start::route(&nodes, sn, fnode, rev) {
                    Some(r) => {
                        let len: f32 = r.windows(2).map(|w| { let (a, b) = (nodes.nodes[w[0] as usize].pos_04, nodes.nodes[w[1] as usize].pos_04); ((a[0]-b[0]).powi(2) + (a[2]-b[2]).powi(2)).sqrt() }).sum();
                        println!("  {tag}: route {} nodes {:.0} m start {sn} finish {fnode}", r.len(), len);
                    }
                    None => println!("  {tag}: NO ROUTE start {sn} finish {fnode}"),
                }
            }
            let n = &nodes.nodes[sn as usize];
            let p = start::slot(&nodes, sn, rev, linear, racers, racers);
            let o = start::slot(&nodes, sn, rev, linear, racers, 1);
            println!("  {tag}: trigger ({:.0},{:.0},{:.0}) node {sn} at ({:.0},{:.0},{:.0}) | player node {} ({:.0},{:.0},{:.0}) fwd ({:.2},{:.2}) | pole node {} ({:.0},{:.0},{:.0})",
                trig[0], trig[1], trig[2], n.pos_04[0], n.pos_04[1], n.pos_04[2],
                p.node, p.position[0], p.position[1], p.position[2], p.forward[0], p.forward[2],
                o.node, o.position[0], o.position[1], o.position[2]);
        }
    }
}

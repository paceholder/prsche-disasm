//! HUD track map outline `HTrackMap_BuildOutline` 0x432230 (spec original-race-hud
//! "Track map from the node graph"): one polyline per section, connectors to the
//! junction centres, no segment across two sections.
//!
//! The synthetic tests need no game data; the local-data test needs `local/game`
//! (skipped otherwise).

use std::path::{Path, PathBuf};

use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes, NODE_SIZE};
use nfs_assets::race::track_map::{TrackMap, MAX_POINTS};

fn node_bytes(pts: &[(f32, f32)]) -> Vec<u8> {
    let mut b = vec![0u8; pts.len() * NODE_SIZE];
    for (i, (x, z)) in pts.iter().enumerate() {
        let r = &mut b[i * NODE_SIZE..(i + 1) * NODE_SIZE];
        r[4..8].copy_from_slice(&x.to_le_bytes());
        r[0xc..0x10].copy_from_slice(&z.to_le_bytes());
        // forward +z so the node frame is sane
        r[0x24..0x28].copy_from_slice(&1.0f32.to_le_bytes());
    }
    b
}

/// A trunk along +x, then two branches that leave the trunk end (a road split).
fn split() -> (TrackNodes, usize) {
    let mut pts = Vec::new();
    for i in 0..40 {
        pts.push((i as f32 * 10.0, 0.0));
    }
    let trunk = pts.len();
    // branch A bends left, branch B bends right; both start at the trunk end
    for i in 0..30 {
        let a = i as f32;
        pts.push((400.0 + a * 10.0, a * a * 0.5));
    }
    for i in 0..30 {
        let a = i as f32;
        pts.push((400.0 + a * 10.0, -a * a * 0.5));
    }
    let t = TrackNodes::build(&node_bytes(&pts), &[trunk as i32, 30, 30], None);
    (t, trunk)
}

#[test]
fn road_split_is_not_connected_across_sections() {
    let (t, _) = split();
    assert!(t.sections.len() >= 3);
    let m = TrackMap::build(&t);
    assert!(m.points.len() >= 6 && m.points.len() <= MAX_POINTS);
    // every drawn polyline segment stays inside one section
    let polyline_segments = m.points.windows(2).filter(|w| w[0].section == w[1].section).count();
    let all = m.segments();
    assert_eq!(all.len(), polyline_segments + m.connectors.len());
    // straight sections are one long segment (no turning); joining the branch ends
    // would be over 600 m
    let split_jump = m
        .points
        .windows(2)
        .filter(|w| w[0].section != w[1].section)
        .count();
    assert!(split_jump >= 2, "sections must be separate polylines");
    for s in all.iter().take(polyline_segments) {
        let d = ((s[2] - s[0]).powi(2) + (s[3] - s[1]).powi(2)).sqrt();
        assert!(d < 500.0, "segment {s:?} spans {d} m");
    }
}

#[test]
fn connectors_run_from_section_end_nodes_to_the_junction_centre() {
    let (t, _) = split();
    let m = TrackMap::build(&t);
    assert!(!m.connectors.is_empty(), "section ends have junctions");
    for c in &m.connectors {
        // starts on a node, ends at some junction centre
        assert!(t.nodes.iter().any(|n| n.pos_04[0] == c[0] && n.pos_04[2] == c[1]));
        assert!(t.junctions.iter().any(|j| j.centre_2c[0] == c[2] && j.centre_2c[2] == c[3]));
    }
}

#[test]
fn single_section_circuit_is_resampled_to_the_turning_budget() {
    let n = 600;
    let pts: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (300.0 * a.cos(), 300.0 * a.sin())
        })
        .collect();
    let t = TrackNodes::build(&node_bytes(&pts), &[], None);
    let m = TrackMap::build(&t);
    // 0.5 % of the total turn per point: about 200 points for one full turn
    assert!(m.points.len() > 150 && m.points.len() <= MAX_POINTS, "{}", m.points.len());
    assert!(m.points.iter().all(|p| p.section == m.points[0].section));
    // bounding box centre near the circle centre, extent the diameter
    assert!(m.centre[0].abs() <= 2 && m.centre[1].abs() <= 2, "{:?}", m.centre);
    assert!((m.extent - 600.0).abs() < 4.0, "{}", m.extent);
}

#[test]
fn empty_graph_gives_an_empty_map() {
    let t = TrackNodes::default();
    let m = TrackMap::build(&t);
    assert!(m.points.is_empty() && m.segments().is_empty());
}

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Track")
}

fn load(name: &str) -> Option<TrackNodes> {
    let dir = game();
    let crp = std::fs::read(dir.join(format!("{name}.crp"))).ok()?;
    let crp = nfs_formats::decompress(&crp).ok()?;
    let (nodes, sections) = sim_chunks(&crp)?;
    let jnc = std::fs::read(dir.join(format!("{name}.jnc"))).ok();
    Some(TrackNodes::build(nodes, &sections, jnc.as_deref()))
}

#[test]
fn shipped_tracks_never_join_sections() {
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    for name in ["alps", "autobahn", "monaco1"] {
        let Some(t) = load(name) else {
            eprintln!("skipped {name}: no data");
            continue;
        };
        let m = TrackMap::build(&t);
        assert!(m.points.len() > 20 && m.points.len() <= MAX_POINTS, "{name} {}", m.points.len());
        let poly: Vec<_> = m.points.windows(2).filter(|w| w[0].section == w[1].section).collect();
        for w in &poly {
            let d = ((w[1].x - w[0].x).powi(2) + (w[1].z - w[0].z).powi(2)).sqrt();
            // resampled points of one section: well below the track length
            assert!(d < 1500.0, "{name}: {d} m between {w:?}");
        }
        eprintln!(
            "{name}: {} nodes, {} sections, {} points, {} connectors, centre {:?}, extent {}",
            t.nodes.len(), t.sections.len(), m.points.len(), m.connectors.len(), m.centre, m.extent
        );
    }
}

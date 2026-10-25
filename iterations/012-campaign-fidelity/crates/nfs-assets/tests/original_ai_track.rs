//! AI 2.2: the track node graph of every track, recorded by executing the
//! original builder `TrackNodes_Build` 0x480140 and the `.jnc` loader
//! 0x481080 in Unicorn (scripts/research/replay-ai-track.py), rebuilt by
//! `race::original_ai::track`. Needs `local/game` (skipped otherwise).
//!
//! Spec: ai-drivers "The track node graph is built as the original builds
//! it" (scenario "Every track").

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::path::{Path, PathBuf};

use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes};

static GRAPH: Fixture = Fixture::new("ai-track-graph.tsv");

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Track")
}

pub fn load_track(name: &str) -> Option<TrackNodes> {
    let dir = game();
    let crp = std::fs::read(dir.join(format!("{name}.crp"))).ok()?;
    let crp = nfs_formats::decompress(&crp).ok()?;
    let (nodes, sections) = sim_chunks(&crp)?;
    let jnc = std::fs::read(dir.join(format!("{name}.jnc"))).ok();
    Some(TrackNodes::build(nodes, &sections, jnc.as_deref()))
}

fn bits(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}

fn fb(v: f32) -> u32 {
    v.to_bits()
}

#[test]
fn every_track_graph_matches_the_original() {
    skip_without_local_fixtures!(GRAPH);
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let mut t: Option<TrackNodes> = None;
    let mut name = String::new();
    let (mut checked, mut mism) = (0, Vec::new());
    for line in GRAPH.lines().filter(|l| !l.starts_with('#')) {
        let c: Vec<&str> = line.split('\t').collect();
        if c[0] == "T" {
            name = c[1].to_string();
            let g = load_track(&name).expect("track data");
            assert_eq!(g.nodes.len(), c[2].parse::<usize>().unwrap(), "{name} nodes");
            if g.lap_nodes_04 != c[3].parse::<i32>().unwrap() {
                mism.push(format!("{name} lap_nodes {} vs {}", g.lap_nodes_04, c[3]));
            }
            if g.lap_end_08 != c[4].parse::<i32>().unwrap() {
                mism.push(format!("{name} lap_end {} vs {}", g.lap_end_08, c[4]));
            }
            t = Some(g);
            continue;
        }
        let g = t.as_ref().unwrap();
        checked += 1;
        let ok = match c[0] {
            "S" => {
                let i: usize = c[1].parse().unwrap();
                g.sections.get(i).is_some_and(|s| {
                    let mut cols = Vec::new();
                    for e in &s.ends {
                        cols.push(e.node.to_string());
                        cols.push(e.section.to_string());
                        cols.push(format!("{:08x}", fb(e.weight)));
                        cols.push(e.end.to_string());
                    }
                    let j = |j: Option<usize>| j.map_or(-1, |j| j as i64).to_string();
                    cols.push(j(s.junction_start_20));
                    cols.push(j(s.junction_end_24));
                    cols == c[2..]
                }) && c.len() == 12
                    && i < g.sections.len()
            }
            "J" => {
                let i: usize = c[1].parse().unwrap();
                g.junctions.get(i).is_some_and(|j| {
                    let ends: Vec<String> = j.ends.iter().map(|(s, e)| format!("{s}.{e}")).collect();
                    j.distance_24 == c[2].parse::<i32>().unwrap()
                        && fb(j.progress_28) == bits(c[3])
                        && (0..3).all(|k| fb(j.centre_2c[k]) == bits(c[4 + k]))
                        && ends == c[7..]
                })
            }
            "JS" => {
                let i: usize = c[1].parse().unwrap();
                g.junctions.get(i).is_some_and(|j| {
                    (0..8).all(|k| {
                        let want = match c[2 + k] {
                            "x" | "-" => None,
                            v => {
                                let (s, e) = v.split_once('.').unwrap();
                                Some((s.parse().unwrap(), e.parse().unwrap()))
                            }
                        };
                        j.slots[k] == want
                    })
                })
            }
            "B10" => hex(&g.near_junction_10) == c[1],
            "B14" => hex(&g.end_junction_14) == c[1],
            "N" => {
                let i: usize = c[1].parse().unwrap();
                g.near_20.get(i).is_some_and(|l| {
                    let want: Vec<(i32, u32)> = c[2..]
                        .iter()
                        .map(|p| {
                            let (n, d) = p.split_once(':').unwrap();
                            (n.parse().unwrap(), bits(d))
                        })
                        .collect();
                    l.iter().map(|&(n, d)| (n, fb(d))).collect::<Vec<_>>() == want
                })
            }
            other => panic!("unknown row {other}"),
        };
        if !ok {
            mism.push(format!("{name}: {}", &line[..line.len().min(160)]));
        }
    }
    assert!(mism.is_empty(), "{} of {checked} rows differ:\n{}", mism.len(), mism.join("\n"));
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

static QUERY: Fixture = Fixture::new("ai-track-query.tsv");

/// AI 2.3: node queries recorded from the original (replay-ai-query.py).
/// Spec: ai-drivers "Node queries follow the original".
#[test]
fn node_queries_match_the_original() {
    skip_without_local_fixtures!(QUERY);
    use nfs_assets::race::original_ai::TrackLocator;
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let mut graphs = std::collections::HashMap::new();
    let (mut checked, mut mism) = (0, Vec::new());
    for line in QUERY.lines().filter(|l| !l.starts_with('#')).skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let g = graphs
            .entry(c[0].to_string())
            .or_insert_with(|| load_track(c[0]).unwrap());
        let (a, b, cc): (i32, i32, i32) = (c[2].parse().unwrap(), c[3].parse().unwrap(), c[4].parse().unwrap());
        let pos = [0, 1, 2].map(|k| f32::from_bits(bits(c[5 + k])));
        let got = match c[1] {
            "step" => g.step(a, b, cc).to_string(),
            "ahead" => g.junction_ahead(a, b).map_or(-1, |j| j as i64).to_string(),
            "locate" => g.locate(pos, a).to_string(),
            "walk" => g.walk_section(a as usize, pos, b).to_string(),
            "locator" => {
                let mut l = TrackLocator { node_00: a as i16, section_04: b, ..Default::default() };
                l.update(g, pos);
                format!("{} {} {}", l.node_00, l.changed_08 as u8, l.changed_09 as u8)
            }
            "progress" => {
                let (s, e, len, first) = g.progress_terms(a);
                format!("{:08x} {:08x} {len} {first}", fb(s), fb(e))
            }
            q => panic!("unknown query {q}"),
        };
        checked += 1;
        if got != c[8] {
            mism.push(format!("{line} -> {got}"));
        }
    }
    assert!(mism.is_empty(), "{} of {checked} differ:\n{}", mism.len(), mism[..mism.len().min(20)].join("\n"));
}

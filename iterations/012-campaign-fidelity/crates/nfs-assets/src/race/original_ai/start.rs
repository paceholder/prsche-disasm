//! Race start placement of the original (`docs/re/reset.md` "Race start"): `FUN_00410730`
//! with `FUN_00410580` and `Car_PlaceOnNode` 0x49b090.
//!
//! The start scene `<track>_FStart.scn` / `_BStart.scn` (`FUN_0046d600`, chosen by REVERSE
//! `[0x657430]`) holds a trigger of type 2 (the start line) and one of type 1 (the finish).
//! The start node is the node nearest to the start trigger. The racers stand on it one by
//! one: racer with grid value `g` (key 0x247, the player has the highest, `racers`; the last
//! opponent 1) is `(racers - g) * 3 + 1` nodes ahead of it (1 when there are only two
//! racers), left or right of the node centre by `g & 1`, 0.5 m above it, facing the travel
//! direction (against the node forward with REVERSE).

use super::track::TrackNodes;

/// Position and heading of one grid slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartSlot {
    pub node: i32,
    /// Box-centre-independent road point: node centre plus lateral offset, 0.5 m up.
    pub position: [f32; 3],
    /// Unit travel direction (node forward, negated with REVERSE).
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

/// Body axes `[lateral, up, forward]` of a car placed on a node (`Car_PlaceOnNode` 0x49b090):
/// up is the road normal (flat when it is not upright), lateral = up x forward, forward =
/// lateral x up, so a car at the start of a slope is level with the road.
pub fn frame_axes(up: [f32; 3], forward: [f32; 3]) -> [[f32; 3]; 3] {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    };
    let up = if up[1] > 0.05 { norm(up) } else { [0.0, 1.0, 0.0] };
    let lat = norm(cross(up, forward));
    [lat, up, norm(cross(lat, up))]
}

/// The start line: position of the first type-2 trigger of a start scene.
pub fn start_trigger(scn: &str) -> Option<[f32; 3]> {
    trigger_of_type(scn, 2)
}

/// The finish line: position of the first type-1 trigger of a start scene.
pub fn finish_trigger(scn: &str) -> Option<[f32; 3]> {
    trigger_of_type(scn, 1)
}

fn trigger_of_type(scn: &str, kind: u32) -> Option<[f32; 3]> {
    let file = nfs_formats::parse_scn(scn).ok()?;
    file.triggers
        .iter()
        .find(|t| t.trigger_type == Some(kind))
        .map(|t| t.position)
}

/// The start node of a track without start triggers (`FUN_00410730`: node 1; a backward race on
/// a circuit takes the mirrored node `Step(0, -1)`).
pub fn circuit_first(nodes: &TrackNodes, reverse: bool) -> i32 {
    if reverse {
        nodes.step(0, -1, -1)
    } else {
        1
    }
}

/// One lap of a circuit from `first` along the road in the race direction; `None` unless the
/// walk comes back to `first`.
pub fn lap_route(nodes: &TrackNodes, first: i32, reverse: bool) -> Option<Vec<i32>> {
    let step = if reverse { -1 } else { 1 };
    let mut out = vec![first];
    let mut node = first;
    loop {
        let next = nodes.step(node, step, -1);
        if next == first {
            return Some(out);
        }
        if next == node || out.len() > nodes.nodes.len() {
            return None;
        }
        out.push(next);
        node = next;
    }
}

/// The road from the start node to the finish node, one node per step along the graph in
/// the race direction (`TrackNodes_Step` with the junction rule of the AI walk). The node
/// numbering is not the road order: sections are stored in file order, so the `.lsp` point
/// order is not the road either. `None` when the walk does not reach the finish.
pub fn route(nodes: &TrackNodes, start: i32, finish: i32, reverse: bool) -> Option<Vec<i32>> {
    let step = if reverse { -1 } else { 1 };
    let mut out = vec![start];
    let mut node = start;
    while node != finish {
        let next = nodes.step(node, step, -1);
        if next == node || out.len() > nodes.nodes.len() {
            return None;
        }
        out.push(next);
        node = next;
    }
    Some(out)
}

/// Start node (`FUN_00474440` from the car's locator at node 1): nearest node to the trigger.
pub fn start_node(nodes: &TrackNodes, trigger: [f32; 3]) -> i32 {
    nodes.locate(trigger, 1)
}

/// `FUN_00410580` + `Car_PlaceOnNode` for grid value `grid` of `racers` cars. `linear` is
/// LINEAR_TRACK `[0x657494]` (point to point).
pub fn slot(
    nodes: &TrackNodes,
    start: i32,
    reverse: bool,
    linear: bool,
    racers: i32,
    grid: i32,
) -> StartSlot {
    let dir = if reverse { -1 } else { 1 };
    // On a circuit the node numbering of a backward start is mirrored: `Step(0, -node)`.
    let first = if reverse && !linear {
        nodes.step(0, -start, -1)
    } else {
        start
    };
    let ahead = if racers > 2 { (racers - grid) * 3 + 1 } else { 1 };
    let node = nodes.step(first, ahead * dir, -1);
    let n = &nodes.nodes[node as usize];
    let mut right = grid & 1 == 1;
    if reverse {
        right = !right;
    }
    // Lane widths of the two sides (`+0x48/+0x4c`); `FUN_0040e650` selects the lane-count
    // variant when node 1 has no lanes.
    let lateral = if nodes.nodes.get(1).is_some_and(|m| m.lanes_38[0] == 0) {
        let (l, r) = (n.lanes_38[0] as f32, n.lanes_38[1] as f32);
        let (wl, wr) = (n.lane_width_48[0], n.lane_width_48[1]);
        let total = wr * r + wl * l;
        let centre = total * 0.5 - wl * l;
        let lane = total / (r + l);
        if right { centre + lane } else { centre - lane }
    } else if right {
        n.lane_width_48[1] * 0.5
    } else {
        n.lane_width_48[0] * -0.5
    };
    // `Car_PlaceOnNode` negates the frame's lateral and forward with REVERSE.
    let side = lateral * dir as f32;
    let position = [0, 1, 2].map(|k| n.pos_04[k] + n.lateral_28[k] * side + n.up_10[k] * 0.5);
    let forward = n.forward_1c.map(|v| v * dir as f32);
    StartSlot {
        node,
        position,
        forward,
        up: n.up_10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race::original_ai::track::sim_chunks;

    #[test]
    fn frame_axes_follow_the_slope() {
        // Flat road: the old flat frame.
        let a = frame_axes([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        assert_eq!(a, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        // A 10 degree climb along +z: up leans back, forward rises, the frame stays orthonormal.
        let t = 10f32.to_radians();
        let a = frame_axes([0.0, t.cos(), -t.sin()], [0.0, t.sin(), t.cos()]);
        assert!((a[1][1] - t.cos()).abs() < 1e-5 && (a[2][1] - t.sin()).abs() < 1e-5);
        let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
        assert!(dot(a[1], a[2]).abs() < 1e-5 && dot(a[0], a[1]).abs() < 1e-5);
    }

    const TRACKS: [&str; 9] = [
        "alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest",
        "industrial",
    ];

    /// Every point-to-point track, both directions (local game data; skipped without it): the
    /// start scene has a start and a finish trigger, the road leads from one to the other, and
    /// the grid stands on that road facing along it, the player last.
    #[test]
    fn local_start_grid_follows_the_road_in_both_directions() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        if !dir.join("alps.crp").exists() {
            eprintln!("skipped: no local game data");
            return;
        }
        for track in TRACKS {
            let crp = nfs_formats::decompress(&std::fs::read(dir.join(format!("{track}.crp"))).unwrap())
                .unwrap();
            let (nb, sec) = sim_chunks(&crp).unwrap();
            let jnc = std::fs::read(dir.join(format!("{track}.jnc"))).ok();
            let nodes = TrackNodes::build(nb, &sec, jnc.as_deref());
            for reverse in [false, true] {
                let name = format!("{track}_{}start.scn", if reverse { 'b' } else { 'f' });
                let text = std::fs::read_to_string(dir.join(&name)).unwrap();
                let (s, f) = (start_trigger(&text).unwrap(), finish_trigger(&text).unwrap());
                let (sn, fnode) = (start_node(&nodes, s), start_node(&nodes, f));
                let road = route(&nodes, sn, fnode, reverse)
                    .unwrap_or_else(|| panic!("{name}: no road from node {sn} to {fnode}"));
                assert!(road.len() > 500, "{name}: {} nodes", road.len());
                let racers = 8;
                let mut last = -1;
                for grid in (1..=racers).rev() {
                    let slot = slot(&nodes, sn, reverse, true, racers, grid);
                    let i = road.iter().position(|&n| n == slot.node);
                    let i = i.unwrap_or_else(|| panic!("{name}: grid {grid} node {} off the road", slot.node)) as i32;
                    // Player (grid 8) first, then one row of three nodes further for each place.
                    assert_eq!(i, 1 + 3 * (racers - grid), "{name} grid {grid}");
                    assert!(i > last);
                    last = i;
                    // Facing the way the road leads.
                    let next = nodes.nodes[road[i as usize + 1] as usize].pos_04;
                    let here = nodes.nodes[slot.node as usize].pos_04;
                    let d = [next[0] - here[0], next[2] - here[2]];
                    let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    let dot = (d[0] * slot.forward[0] + d[1] * slot.forward[2]) / l;
                    assert!(dot > 0.8, "{name} grid {grid}: facing dot {dot}");
                }
            }
        }
    }
}

//! The HUD track map outline, `HTrackMap_BuildOutline` 0x432230 (`docs/re/hud.md`
//! "Track map" in the workspace root): the track node graph `[0x628bc0]` walked
//! node by node, resampled by accumulated heading change, one polyline per
//! section, plus a connector from every section-end node to its junction centre.
//!
//! `HTrackMap_Draw3D` 0x433690 draws a segment between two consecutive outline
//! points only when both belong to the same section, so the sections of a road
//! split are never joined to each other.

use super::original_ai::track::TrackNodes;

/// Outline point cap (`+0x104 < 400`).
pub const MAX_POINTS: usize = 400;
/// Heading change per kept point: the total turn times this.
const STEP_FRACTION: f64 = 0.005;

/// One outline point (`0x60e4a0`, 16 bytes: x, 1.0, z, section).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPoint {
    pub x: f32,
    pub z: f32,
    pub section: i32,
}

/// The outline arrays and the bounding box of `HTrackMap_BuildOutline`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackMap {
    /// `0x60e4a0`, at most [`MAX_POINTS`].
    pub points: Vec<MapPoint>,
    /// `0x60cb70`: node position to junction centre, `[x0, z0, x1, z1]`.
    pub connectors: Vec<[f32; 4]>,
    /// Bounding box centre `+0xe8/+0xec` (truncated to whole metres).
    pub centre: [i32; 2],
    /// `+0x14c`: larger bounding box extent (whole metres).
    pub extent: f32,
}

fn turns(dz: f32, dx: f32) -> f64 {
    (dz as f64).atan2(dx as f64) * (0.5 / std::f64::consts::PI)
}

/// Heading change of one step in turns, `|d|`, folded to at most 0.5.
fn fold(h: f64, prev: f64) -> f64 {
    let d = (h - prev).abs();
    (if d > 0.5 { 1.0 - d } else { d }).abs()
}

impl TrackMap {
    /// Outline segments `[x0, z0, x1, z1]` as `HTrackMap_Draw3D` draws them:
    /// consecutive points of the same section, then the junction connectors.
    pub fn segments(&self) -> Vec<[f32; 4]> {
        let mut out = Vec::new();
        for w in self.points.windows(2) {
            if w[0].section == w[1].section {
                out.push([w[0].x, w[0].z, w[1].x, w[1].z]);
            }
        }
        out.extend_from_slice(&self.connectors);
        out
    }

    /// `HTrackMap_BuildOutline` 0x432230.
    pub fn build(t: &TrackNodes) -> Self {
        let n = t.nodes.len();
        let sec = |i: usize| -> i32 {
            if i >= n {
                -1
            } else {
                t.section_of(i as i32).map_or(-1, |s| s as i32)
            }
        };
        // Section per node once, instead of a search per query.
        let secs: Vec<i32> = (0..n).map(|i| sec(i)).collect();
        let sec = |i: usize| -> i32 { secs.get(i).copied().unwrap_or(-1) };
        let pos = |i: usize| (t.nodes[i].pos_04[0], t.nodes[i].pos_04[2]);
        // Heading of node `i` towards the next node (`+0x54/+0x5c`: the next record;
        // the last node reads past the array, its value is never used).
        let head0 = |i: usize| -> f64 {
            let (x, z) = pos(i);
            let (nx, nz) = if i + 1 < n { pos(i + 1) } else { (x, z) };
            turns(nz - z, nx - x)
        };

        // Total turn along every section.
        let mut total = 0.0f32;
        let mut i = 0;
        'sum: while i < n {
            let (mut px, mut pz) = pos(i);
            let mut h = head0(i);
            let s = sec(i);
            i += 1;
            if s == sec(i) {
                loop {
                    let prev = h;
                    if i >= n {
                        break 'sum;
                    }
                    let (qx, qz) = pos(i);
                    h = turns(qz - pz, qx - px);
                    total = (fold(h, prev) + total as f64) as f32;
                    i += 1;
                    px = qx;
                    pz = qz;
                    if s != sec(i) {
                        break;
                    }
                }
            }
        }
        let step = (total as f64 * STEP_FRACTION) as f32;

        let mut points: Vec<MapPoint> = Vec::new();
        let mut connectors: Vec<[f32; 4]> = Vec::new();
        let junction = |i: usize| -> Option<[f32; 2]> {
            match t.end_junction_14.get(i).copied() {
                Some(j) if j != super::original_ai::track::NO_JUNCTION && (j as usize) < t.junctions.len() => {
                    let c = t.junctions[j as usize].centre_2c;
                    Some([c[0], c[2]])
                }
                _ => None,
            }
        };
        let connect = |connectors: &mut Vec<[f32; 4]>, i: usize, x: f32, z: f32| {
            if let Some(c) = junction(i) {
                connectors.push([x, z, c[0], c[1]]);
            }
        };

        let mut i = 0;
        if n > 0 {
            'outer: while points.len() < MAX_POINTS {
                let (mut px, mut pz) = pos(i);
                let mut s = sec(i);
                let mut h = head0(i);
                points.push(MapPoint { x: px, z: pz, section: s });
                connect(&mut connectors, i, px, pz);
                i += 1;
                let mut acc = 0.0f32;
                if s == sec(i) {
                    loop {
                        let prev = h;
                        if i >= n {
                            break 'outer;
                        }
                        if points.len() >= MAX_POINTS {
                            break;
                        }
                        let (qx, qz) = pos(i);
                        h = turns(qz - pz, qx - px);
                        acc = (fold(h, prev) + acc as f64) as f32;
                        if step <= acc {
                            points.push(MapPoint { x: px, z: pz, section: s });
                            acc -= step;
                        }
                        s = sec(i);
                        connect(&mut connectors, i, qx, qz);
                        i += 1;
                        px = qx;
                        pz = qz;
                        if s != sec(i) {
                            break;
                        }
                    }
                }
                if i >= n {
                    break;
                }
                if points.len() < MAX_POINTS {
                    points.push(MapPoint { x: px, z: pz, section: s });
                }
            }
        }

        // Bounding box: the running extrema start at 0, so the origin is inside.
        let (mut x1, mut z1, mut x0, mut z0) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for p in &points {
            x1 = x1.max(p.x);
            z1 = z1.max(p.z);
            x0 = x0.min(p.x);
            z0 = z0.min(p.z);
        }
        let ex = (x1 - x0) as i32;
        let ez = (z1 - z0) as i32;
        Self {
            points,
            connectors,
            centre: [((x1 - x0) * 0.5 + x0) as i32, ((z1 - z0) * 0.5 + z0) as i32],
            extent: ex.max(ez) as f32,
        }
    }
}

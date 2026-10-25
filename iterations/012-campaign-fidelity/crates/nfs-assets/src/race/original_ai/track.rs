//! The original track node graph `[0x628bc0]` (`docs/re/ai.md` section 2 in
//! the workspace root): nodes from the track CRP chunk `SimD`, sections from
//! `SimT`, junctions, end weights (`.jnc`), the per-node junction tables and
//! the per-junction near-node lists, built as `TrackNodes_Build` 0x480140
//! builds them.
//!
//! Floating point follows the x87 code at 53-bit precision: intermediate
//! values in `f64`, rounded to `f32` where the original stores them.

/// Size of a `SimD` node record.
pub const NODE_SIZE: usize = 0x50;
/// Squared junction gather radius of `0x480900`/`0x480b40` (16 m).
const BRANCH_RADIUS2: f32 = 256.0;
/// `[0x628bb8] = 90²` (CRT initialiser 0x47fcf0, `0x5b47bc`).
pub const NEAR_RADIUS2: f32 = 90.0 * 90.0;
/// `[0x628bbc] = 18²` (CRT initialiser 0x47fd10, `0x5b47c0`), junctions
/// with fewer than 3 ends.
pub const NEAR_RADIUS2_SMALL: f32 = 18.0 * 18.0;
/// No junction in the byte tables `+0x10/+0x14`.
pub const NO_JUNCTION: u8 = 0xff;
/// At most 8 ends per junction (`0x480b40`).
pub const MAX_ENDS: usize = 8;

fn x(v: f32) -> f64 {
    v as f64
}

/// One `SimD` node record (0x50 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrackNode {
    /// `+0x00`.
    pub field_00: i32,
    /// `+0x04`: centre.
    pub pos_04: [f32; 3],
    /// `+0x10`: up.
    pub up_10: [f32; 3],
    /// `+0x1c`: forward.
    pub forward_1c: [f32; 3],
    /// `+0x28`: lateral axis (up × forward).
    pub lateral_28: [f32; 3],
    /// `+0x34`: flags.
    pub flags_34: u32,
    /// `+0x38/+0x3c`: lane counts of the two sides.
    pub lanes_38: [i32; 2],
    /// `+0x40/+0x44`: road half widths of the two sides.
    pub half_width_40: [f32; 2],
    /// `+0x48/+0x4c`: lane widths of the two sides.
    pub lane_width_48: [f32; 2],
}

impl TrackNode {
    pub fn from_bytes(b: &[u8]) -> Self {
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let v = |o: usize| [f(o), f(o + 4), f(o + 8)];
        Self {
            field_00: i(0),
            pos_04: v(4),
            up_10: v(0x10),
            forward_1c: v(0x1c),
            lateral_28: v(0x28),
            flags_34: i(0x34) as u32,
            lanes_38: [i(0x38), i(0x3c)],
            half_width_40: [f(0x40), f(0x44)],
            lane_width_48: [f(0x48), f(0x4c)],
        }
    }
}

/// One end record of a section (0x10 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SectionEnd {
    /// `+0x0`: node.
    pub node: i32,
    /// `+0x4`: section index.
    pub section: i32,
    /// `+0x8`: branch weight (`1/n` per direction, or `.jnc`).
    pub weight: f32,
    /// `+0xc`: 0 at the section start, 1 at its end.
    pub end: i32,
}

/// One section (0x28 bytes): `ends[0]` start, `ends[1]` end, and the
/// junctions there (`+0x20`, `+0x24`; `None` = null).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Section {
    pub ends: [SectionEnd; 2],
    pub junction_start_20: Option<usize>,
    pub junction_end_24: Option<usize>,
}

impl Section {
    pub fn first(&self) -> i32 {
        self.ends[0].node
    }
    pub fn last(&self) -> i32 {
        self.ends[1].node
    }
}

/// A reference to a section end: `(section, 0 start | 1 end)` (the original
/// stores a pointer into the section table).
pub type EndRef = (usize, usize);

/// One junction (0x38 bytes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Junction {
    /// `+0x00..+0x1c`, count `+0x20`.
    pub ends: Vec<EndRef>,
    /// All 8 end slots: past the count they hold the ends of earlier
    /// junctions (the builder's local array is reused, `0x480b40`); `None`
    /// where the original holds builder stack data. `TrackNodes_Step` can
    /// read them with a branch index past the count.
    pub slots: [Option<EndRef>; MAX_ENDS],
    /// `+0x24`: longest node distance from junction 0 (`0x4819e0`).
    pub distance_24: i32,
    /// `+0x28`: `+0x24` over the lap length (lap fraction).
    pub progress_28: f32,
    /// `+0x2c`: mean position of the ends' nodes.
    pub centre_2c: [f32; 3],
}

/// The node graph object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackNodes {
    /// `+0xc` (count `+0x0`).
    pub nodes: Vec<TrackNode>,
    /// `+0x4`: lap length in nodes.
    pub lap_nodes_04: i32,
    /// `+0x8`: last node of the lap.
    pub lap_end_08: i32,
    /// `+0x10`: nearest junction per node ([`NO_JUNCTION`] none).
    pub near_junction_10: Vec<u8>,
    /// `+0x14`: junction at a section end per node.
    pub end_junction_14: Vec<u8>,
    /// `+0x18`.
    pub sections: Vec<Section>,
    /// `+0x1c`.
    pub junctions: Vec<Junction>,
    /// `+0x20`: per junction, `(node, distance²)` by distance.
    pub near_20: Vec<Vec<(i32, f32)>>,
}

/// `SimD` node bytes and `SimT` section lengths from a decompressed track
/// CRP (`karT`): table entries `tag, flags, count, offset`, the offset
/// relative to the entry when `flags & 2` (`TrackNodes_LoadSim` 0x480080).
pub fn sim_chunks(crp: &[u8]) -> Option<(&[u8], Vec<i32>)> {
    let entry = |tag: &[u8; 4]| -> Option<(usize, usize)> {
        let fourcc: Vec<u8> = tag.iter().rev().copied().collect();
        let mut i = 0;
        while i + 16 <= crp.len() {
            if crp[i..i + 4] == fourcc[..] {
                let w = |o: usize| u32::from_le_bytes(crp[o..o + 4].try_into().unwrap()) as usize;
                let (flags, count, mut off) = (w(i + 4), w(i + 8), w(i + 12));
                if flags & 2 != 0 {
                    off += i;
                }
                return Some((count, off));
            }
            i += 4;
        }
        None
    };
    let (n, off) = entry(b"SimD")?;
    let nodes = crp.get(off..off + n * NODE_SIZE)?;
    let sections = match entry(b"SimT") {
        Some((ns, soff)) => (0..ns)
            .map(|k| {
                crp.get(soff + 4 * k..soff + 4 * k + 4)
                    .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
            })
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    Some((nodes, sections))
}

impl TrackNodes {
    /// `TrackNodes_Build` 0x480140 on `SimD` node bytes and `SimT` section
    /// lengths, then the `.jnc` weights (`TrackNodes_LoadJunctionWeights`
    /// 0x481080) when given.
    pub fn build(node_bytes: &[u8], section_lengths: &[i32], jnc: Option<&[u8]>) -> Self {
        let nodes: Vec<TrackNode> = node_bytes
            .as_chunks::<NODE_SIZE>()
            .0
            .iter()
            .map(|c| TrackNode::from_bytes(c))
            .collect();
        let n = nodes.len() as i32;
        let mut t = TrackNodes {
            near_junction_10: vec![NO_JUNCTION; nodes.len()],
            end_junction_14: vec![NO_JUNCTION; nodes.len()],
            nodes,
            ..Default::default()
        };
        let section = |first: i32, last: i32| Section {
            ends: [
                SectionEnd { node: first, section: -1, weight: 0.0, end: 0 },
                SectionEnd { node: last, section: -1, weight: 0.0, end: 1 },
            ],
            junction_start_20: None,
            junction_end_24: None,
        };
        if section_lengths.len() < 2 {
            t.sections.push(section(0, n - 1));
        } else {
            let mut start = 0;
            for &len in section_lengths {
                let last = len - 1 + start;
                t.sections.push(section(start, last));
                start = last - start + 1 + start;
            }
        }
        t.split_sections();
        t.build_junctions();
        t.link_sections();
        t.build_near_lists();
        t.lap_distances();
        for s in 0..t.sections.len() {
            let (_, end, len, first) = t.progress_terms(t.sections[s].last());
            if end as f64 >= 0.999 {
                t.lap_end_08 = len - 1 + first;
            }
        }
        if let Some(jnc) = jnc {
            t.load_junction_weights(jnc);
        }
        t
    }

    fn pos(&self, node: i32) -> [f32; 3] {
        self.nodes[node as usize].pos_04
    }

    /// `TrackNodes_SplitSections` 0x480900: splits a section where an end
    /// of another section lies within 16 m of one of its nodes, more than 5
    /// nodes from both of its ends; then numbers the sections.
    fn split_sections(&mut self) {
        let count = self.sections.len();
        // The best node survives from one candidate section to the next
        // (`local_48`); it only matters together with a distance below 16 m.
        let mut best_node = 0;
        for a in 0..count {
            for e in 0..2 {
                let p = self.pos(self.sections[a].ends[e].node);
                let mut b = 0;
                while b < self.sections.len() {
                    if b != a {
                        let (first, last) = (self.sections[b].first(), self.sections[b].last());
                        let mut best = 1e9f32;
                        for k in first..last {
                            let q = self.pos(k);
                            let dz = x(p[2]) - x(q[2]);
                            let dx = x(p[0]) - x(q[0]);
                            let d = dx * dx + dz * dz;
                            if d < x(best) {
                                best = d as f32;
                                best_node = k;
                            }
                        }
                        if best < BRANCH_RADIUS2
                            && best_node - self.sections[b].first() > 5
                            && self.sections[b].last() - best_node > 5
                        {
                            let mut copy = self.sections[b];
                            self.sections[b].ends[1].node = best_node;
                            copy.ends[0].node = best_node + 1;
                            self.sections.push(copy);
                        }
                    }
                    b += 1;
                }
            }
        }
        for (i, s) in self.sections.iter_mut().enumerate() {
            s.ends[0].section = i as i32;
            s.ends[1].section = i as i32;
        }
    }

    fn junction_of_end(&self, end: EndRef) -> Option<usize> {
        self.junctions.iter().position(|j| j.ends.contains(&end))
    }

    fn end(&self, (s, e): EndRef) -> &SectionEnd {
        &self.sections[s].ends[e]
    }

    /// `TrackNodes_BuildJunctions` 0x480b40: one junction per section end
    /// not yet in one, with every other end within 16 m (at most 8 ends).
    fn build_junctions(&mut self) {
        let mut slots: [Option<EndRef>; MAX_ENDS] = [None; MAX_ENDS];
        for a in 0..self.sections.len() {
            for e in 0..2 {
                if self.junction_of_end((a, e)).is_some() {
                    continue;
                }
                let mut ends: Vec<EndRef> = vec![(a, e)];
                let p = self.pos(self.end((a, e)).node);
                for b in 0..self.sections.len() {
                    for f in 0..2 {
                        let q = self.pos(self.end((b, f)).node);
                        let dz = x(p[2]) - x(q[2]);
                        let dx = x(p[0]) - x(q[0]);
                        if dz * dz + dx * dx < x(BRANCH_RADIUS2) {
                            if self.junction_of_end((b, f)).is_some() {
                                continue;
                            }
                            if (b != a || f != e) && ends.len() < MAX_ENDS {
                                ends.push((b, f));
                            }
                        }
                    }
                }
                let mut sum = [0.0f64; 3];
                for &r in &ends {
                    let q = self.pos(self.end(r).node);
                    for k in 0..3 {
                        sum[k] += x(q[k]);
                    }
                }
                let cnt = ends.len() as f32;
                let centre = sum.map(|s| (s / x(cnt)) as f32);
                let at_end = ends.iter().filter(|&&r| self.end(r).end == 1).count() as i32;
                for &(s, f) in &ends {
                    let m = if self.sections[s].ends[f].end == 1 {
                        at_end
                    } else {
                        ends.len() as i32 - at_end
                    };
                    self.sections[s].ends[f].weight = (1.0 / x(m as f32)) as f32;
                }
                for (k, &r) in ends.iter().enumerate() {
                    slots[k] = Some(r);
                }
                self.junctions.push(Junction {
                    slots,
                    ends,
                    distance_24: 0,
                    progress_28: 0.0,
                    centre_2c: centre,
                });
            }
        }
    }

    /// Junction with an end at `node` (first in table order).
    fn junction_at_node(&self, node: i32) -> Option<usize> {
        self.junctions
            .iter()
            .position(|j| j.ends.iter().any(|&r| self.end(r).node == node))
    }

    /// 0x480140 after the junctions: links section ends to junctions (by
    /// node) and marks end nodes in `+0x14`.
    fn link_sections(&mut self) {
        for s in 0..self.sections.len() {
            let js = self.junction_at_node(self.sections[s].first());
            let je = self.junction_at_node(self.sections[s].last());
            self.sections[s].junction_start_20 = js;
            self.sections[s].junction_end_24 = je;
        }
        for j in 0..self.junctions.len() {
            for k in 0..self.junctions[j].ends.len() {
                let node = self.end(self.junctions[j].ends[k]).node;
                self.end_junction_14[node as usize] = j as u8;
            }
        }
    }

    /// 0x480140: per junction, every node of its ends' sections within the
    /// near radius of the centre, by distance; `+0x10` is the nearest
    /// junction per node.
    fn build_near_lists(&mut self) {
        for j in 0..self.junctions.len() {
            let c = self.junctions[j].centre_2c;
            let r2 = if self.junctions[j].ends.len() < 3 {
                NEAR_RADIUS2_SMALL
            } else {
                NEAR_RADIUS2
            };
            let mut list: Vec<(i32, f32)> = Vec::new();
            for k in 0..self.junctions[j].ends.len() {
                let sec = self.sections[self.end(self.junctions[j].ends[k]).section as usize];
                for node in sec.first()..=sec.last() {
                    let p = self.pos(node);
                    let dz = x(c[2]) - x(p[2]);
                    let dx = x(c[0]) - x(p[0]);
                    let d = (dz * dz + dx * dx) as f32;
                    if d < r2 {
                        list.push((node, d));
                        let cur = self.near_junction_10[node as usize];
                        if cur == NO_JUNCTION {
                            self.near_junction_10[node as usize] = j as u8;
                        } else {
                            let o = self.junctions[cur as usize].centre_2c;
                            let dz = x(o[2]) - x(p[2]);
                            let dx = x(o[0]) - x(p[0]);
                            if x(d) < dz * dz + dx * dx {
                                self.near_junction_10[node as usize] = j as u8;
                            }
                        }
                    }
                }
            }
            sort_by_distance(&mut list);
            self.near_20.push(list);
        }
    }

    /// `0x481910` with the walk `0x4819e0`: the longest node distance of
    /// each junction from junction 0, the lap length `+0x4` and the
    /// junction progress `+0x28`.
    fn lap_distances(&mut self) {
        let mut max = 0u32;
        if !self.junctions.is_empty() {
            self.walk(0, &mut max);
        }
        self.lap_nodes_04 = max as i32;
        if self.junctions.len() == 1 {
            self.lap_nodes_04 = self.nodes.len() as i32;
        }
        for j in &mut self.junctions {
            j.progress_28 = if max == 0 {
                0.0
            } else {
                (x(j.distance_24 as f32) / (max as f64) as f32 as f64) as f32
            };
        }
    }

    /// `0x4819e0`: for each start end of junction `j` (the first 8 slots),
    /// follow its section to the junction at its last node and extend that
    /// junction's distance if the path is longer.
    fn walk(&mut self, j: usize, max: &mut u32) {
        for k in 0..MAX_ENDS {
            let Some(&r) = self.junctions[j].ends.get(k) else {
                continue;
            };
            if self.end(r).end != 0 {
                continue;
            }
            let node = self.end(r).node;
            let Some(sec) = self.section_of(node) else {
                continue;
            };
            let last = self.sections[sec].last();
            let Some(j2) = self.junction_at_node(last) else {
                continue;
            };
            if j2 == j {
                continue;
            }
            let d = self.junctions[j].distance_24.wrapping_add(last - node).wrapping_add(1);
            if self.junctions[j2].distance_24 < d {
                if *max < d as u32 {
                    *max = d as u32;
                }
                if j2 != 0 {
                    self.junctions[j2].distance_24 = d;
                    self.walk(j2, max);
                }
            }
        }
    }

    /// `0x481080`: if the file holds one 36-byte record per junction, record
    /// `j`'s floats (after its index) replace junction `j`'s end weights.
    pub fn load_junction_weights(&mut self, jnc: &[u8]) {
        if jnc.len() / 36 != self.junctions.len() {
            return;
        }
        for j in 0..self.junctions.len() {
            for k in 0..self.junctions[j].ends.len() {
                let o = j * 36 + 4 + 4 * k;
                let w = f32::from_le_bytes(jnc[o..o + 4].try_into().unwrap());
                let (s, e) = self.junctions[j].ends[k];
                self.sections[s].ends[e].weight = w;
            }
        }
    }

    /// Section index containing `node` (first match; `0x481c80`).
    pub fn section_of(&self, node: i32) -> Option<usize> {
        self.sections
            .iter()
            .position(|s| s.first() <= node && node <= s.last())
    }

    /// `TrackNodes_Progress` 0x481b70: (start junction `+0x28`, end
    /// junction `+0x28` (1.0 if ≤ 0.01), section length, first node) of the
    /// section containing `node`; `(0, 0, 1, _)` outside every section.
    pub fn progress_terms(&self, node: i32) -> (f32, f32, i32, i32) {
        let Some(s) = self.section_of(node) else {
            return (0.0, 0.0, 1, 0);
        };
        let sec = &self.sections[s];
        let p = |j: Option<usize>| j.map_or(0.0, |j| self.junctions[j].progress_28);
        let start = p(sec.junction_start_20);
        let mut end = p(sec.junction_end_24);
        if end <= 0.01 {
            end = 1.0;
        }
        (start, end, sec.last() - sec.first() + 1, sec.first())
    }
}

/// Order of the near lists: by distance. The original sorts with the MSVC
/// `std::sort` (introsort, insertion sort `0x482290` for up to 16 pairs);
/// a stable insertion sort gives the same order on every shipped track
/// (no equal distances in one list).
pub fn sort_by_distance(v: &mut [(i32, f32)]) {
    insertion_sort(v);
}

fn insertion_sort(v: &mut [(i32, f32)]) {
    for i in 1..v.len() {
        let item = v[i];
        if item.1 < v[0].1 {
            v.copy_within(0..i, 1);
            v[0] = item;
        } else {
            let mut j = i;
            while item.1 < v[j - 1].1 {
                v[j] = v[j - 1];
                j -= 1;
            }
            v[j] = item;
        }
    }
}

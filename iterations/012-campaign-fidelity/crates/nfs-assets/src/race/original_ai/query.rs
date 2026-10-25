//! Node queries on [`TrackNodes`] (`docs/re/ai.md` section 2): stepping
//! across junctions, the junction ahead, nearest-node walks and the car's
//! track locator. All are thiscall methods on `[0x628bc0]` in the original.

#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::track::{TrackNodes, NEAR_RADIUS2, NO_JUNCTION};

fn x(v: f32) -> f64 {
    v as f64
}

impl TrackNodes {
    /// Squared x/z distance in extended precision, `node − pos` order.
    fn dist2(&self, node: i32, pos: [f32; 3]) -> f64 {
        let p = self.nodes[node as usize].pos_04;
        let dz = x(p[2]) - x(pos[2]);
        let dx = x(p[0]) - x(pos[0]);
        dz * dz + dx * dx
    }

    /// `TrackNodes_Step` 0x4817d0: the node `delta` nodes from `node` along
    /// the graph. Inside a section the result is clamped to the node range;
    /// past a section end the walk continues through the junction there
    /// into end slot `branch` (for `branch < 0` the last end whose
    /// direction is `delta < 1`), entering at `end node + overshoot − 1`.
    /// Returns `node` when there is no way on, and the boundary node at a
    /// section end without a junction.
    pub fn step(&self, node: i32, delta: i32, branch: i32) -> i32 {
        let Some(mut sec) = self.section_of(node) else {
            return node;
        };
        let mut target = delta.wrapping_add(node);
        let mut branch = branch;
        loop {
            let s = &self.sections[sec];
            if s.first() <= target && target <= s.last() {
                let n = self.nodes.len() as i32;
                if n <= target {
                    target = n - 1;
                }
                return target.max(0);
            }
            let boundary = if s.last() < target { s.last() } else { s.first() };
            let j = self.end_junction_14[boundary as usize] as i8;
            if j < 0 {
                return boundary;
            }
            let jn = &self.junctions[j as usize];
            if branch < 0 {
                let dir = (delta < 1) as i32;
                for (k, &r) in jn.ends.iter().enumerate() {
                    if self.sections[r.0].ends[r.1].end == dir {
                        branch = k as i32;
                    }
                }
                if branch < 0 {
                    return node;
                }
            }
            // A branch past the count reads a stale slot (or builder stack
            // data, not modelled: stop there).
            let Some(r) = jn.slots.get(branch as usize).copied().flatten() else {
                return node;
            };
            let end = self.sections[r.0].ends[r.1];
            target = end.node.wrapping_add(target - boundary).wrapping_sub(1);
            if end.section < 0 {
                return node;
            }
            sec = end.section as usize;
            branch = -1;
        }
    }

    /// `TrackNodes_JunctionAhead` 0x4816f0: the junction at the end of
    /// `node`'s section that `node + delta` passes, if any.
    pub fn junction_ahead(&self, node: i32, delta: i32) -> Option<usize> {
        let s = &self.sections[self.section_of(node)?];
        let target = delta.wrapping_add(node);
        if target < s.first() || s.last() < target {
            let b = if s.last() < target { s.last() } else { s.first() };
            let j = self.end_junction_14[b as usize];
            if j != NO_JUNCTION && (j as i8) >= 0 {
                return Some(j as usize);
            }
        }
        None
    }

    /// `0x4812c0` (via `0x4816c0` for a section): nearest node of the node
    /// range `first..=last` by a walk from `hint` (from the nearer range
    /// end when the hint is outside).
    pub fn walk_range(&self, first: i32, last: i32, pos: [f32; 3], hint: i32) -> i32 {
        let mut cur = hint;
        if hint < first || last < hint {
            cur = if self.dist2(last, pos) <= self.dist2(first, pos) {
                last
            } else {
                first
            };
        }
        if cur == -1 {
            return -1;
        }
        loop {
            let d = self.dist2(cur, pos);
            if cur < last && self.dist2(cur + 1, pos) < d {
                cur += 1;
                continue;
            }
            if cur <= first {
                return cur;
            }
            if self.dist2(cur - 1, pos) < d {
                cur -= 1;
            } else {
                return cur;
            }
        }
    }

    /// `0x4816c0`: [`Self::walk_range`] over section `sec`.
    pub fn walk_section(&self, sec: usize, pos: [f32; 3], hint: i32) -> i32 {
        let s = &self.sections[sec];
        self.walk_range(s.first(), s.last(), pos, hint)
    }

    /// `0x481190`: nearest node to `pos` in junction `j`'s near list,
    /// searched in the distance band `|d(centre) ± (90² − 84²)|`.
    pub fn near_junction_node(&self, pos: [f32; 3], j: usize) -> i32 {
        let list = &self.near_20[j];
        let c = self.junctions[j].centre_2c;
        let dz = x(c[2]) - x(pos[2]);
        let dx = x(c[0]) - x(pos[0]);
        let d = dz * dz + dx * dx;
        let k = x(NEAR_RADIUS2) - 7056.0;
        let hi = (k + d) as f32;
        let mut lo = (d - k) as f32;
        if lo < 0.0 {
            lo = 0.0;
        }
        let mut i = 0;
        if list[0].1 < lo {
            while i < list.len() {
                let next = list.get(i + 1).map_or(f32::INFINITY, |e| e.1);
                i += 1;
                if !(next < lo) {
                    break;
                }
            }
        }
        if i == list.len() {
            return list[i - 1].0;
        }
        let mut best = f32::from_bits(0x7e96_7699);
        let mut result = 0;
        while i < list.len() {
            if !(list[i].1 < hi) {
                return result;
            }
            let d2 = self.dist2(list[i].0, pos);
            if d2 < x(best) {
                best = d2 as f32;
                result = list[i].0;
            }
            i += 1;
        }
        result
    }

    /// `TrackNodes_Locate` 0x481400: nearest node to `pos` from `hint`.
    /// Far hints (> 128 m) restart from the nearest of every 8th node; a
    /// node near a junction restarts from the junction's near list; then a
    /// walk to the local minimum (wrapping at the node count, at most 1000
    /// steps).
    pub fn locate(&self, pos: [f32; 3], hint: i32) -> i32 {
        let n = self.nodes.len() as i32;
        let mut h = hint;
        if 16384.0 < self.dist2(h, pos) {
            let mut best = 1.9e38f32;
            h = -1;
            let mut i = 0;
            while i < n {
                let d = self.dist2(i, pos);
                if d < x(best) {
                    best = d as f32;
                    h = i;
                }
                i += 8;
            }
        }
        let j = self.near_junction_10[h as usize];
        if j != NO_JUNCTION {
            h = self.near_junction_node(pos, j as i8 as i32 as usize);
        }
        let f = |node: i32| self.dist2(node, pos) as f32;
        let mut prev = -1;
        let mut cur = h;
        let mut iters = 0;
        loop {
            if prev == cur {
                return cur;
            }
            iters += 1;
            let d = if cur < n - 1 { f(cur) } else { f(cur % n) };
            let mut next;
            let forward = if cur < n - 2 {
                next = cur + 1;
                f(cur + 1) < d
            } else {
                next = (cur + 1) % n;
                f(next) < d
            };
            if !forward {
                if cur < 1 {
                    let c2 = cur + n - 1;
                    let v = if f(c2 % n) < d { c2 } else { cur + n };
                    next = (n + v) % n;
                } else {
                    next = if f(cur - 1) < d { cur - 1 } else { cur };
                }
            }
            if 999 < iters {
                return next;
            }
            prev = cur;
            cur = next;
        }
    }
}

/// The car's track locator at `car+0x8` (`TrackLocator_Update` 0x474440):
/// node `+0x0`, section `+0x4` (the AI's chosen branch section, `car+0xc`),
/// changed flags `+0x8`/`+0x9` (`car+0x10`/`+0x11`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackLocator {
    pub node_00: i16,
    pub section_04: i32,
    pub changed_08: bool,
    pub changed_09: bool,
}

impl TrackLocator {
    /// `0x474440`: with more than one section, the nearer of the walks in
    /// the node's own section and in `section_04` (if ≥ 0); otherwise
    /// [`TrackNodes::locate`].
    pub fn update(&mut self, t: &TrackNodes, pos: [f32; 3]) {
        let old = self.node_00 as i32;
        let new = match t.section_of(old) {
            Some(own) if self.section_04 >= 0 && t.sections.len() > 1 => {
                let a = t.walk_section(own, pos, old);
                let b = t.walk_section(self.section_04 as usize, pos, old);
                let d = |n: i32| {
                    let p = t.nodes[n as usize].pos_04;
                    let dz = x(pos[2]) - x(p[2]);
                    let dx = x(pos[0]) - x(p[0]);
                    dz * dz + dx * dx
                };
                if d(a) < d(b) {
                    a
                } else {
                    b
                }
            }
            _ => t.locate(pos, old),
        };
        self.node_00 = new as i16;
        let changed = self.node_00 as i32 != old;
        self.changed_08 = changed;
        self.changed_09 = changed;
    }
}

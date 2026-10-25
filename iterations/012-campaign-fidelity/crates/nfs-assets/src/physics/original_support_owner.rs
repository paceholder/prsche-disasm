//! The support owner of nfs5.exe: the 0x30-byte object through which a wheel
//! (`w+4`) or the car body (`car+8`) asks the track for the polygon under a
//! point. Workspace notes: `docs/re/support.md` section 3.
//!
//! Layout (original offsets): `+0x02` normal valid, `+0x09` polygon changed,
//! `+0x0a` no polygon, `+0x0c` normal, `+0x18` in-plane edge direction,
//! `+0x24` polygon flags word, `+0x28` polygon, `+0x2c` spatial node cache.
//! The body owner shares `+0x00..+0x08` with the track locator
//! (`TrackLocator_Update` 0x474440 writes the node there and the changed
//! bytes `+0x08`/`+0x09`; `0x474060` then overwrites `+0x09`).
//!
//! Methods (thiscall on the owner):
//! - `0x474060 (point, reuse)` picks the polygon ([`OriginalSupportOwner::locate`]);
//! - `0x474420` the normal and `0x474430` the edge direction, both computed
//!   once per polygon by `0x474320` ([`OriginalSupportOwner::normal`],
//!   [`OriginalSupportOwner::tangent`]);
//! - `0x4745f0 (i)` vertex `i` through the polygon's vertex getter (quad
//!   `0x485b10`, triangle `0x486030`), or the zero vector `0x5bb2b0` with no
//!   polygon ([`OriginalSupportOwner::vertex`]).
//!
//! The selection itself is `0x473240` ([`SupportTree::query`]); the polygon
//! normal is vtable +0x14 (quad `0x485b50`, triangle `0x485f90`).
//! Verified on real tracks against `scripts/research/replay-road-support.py`
//! (`tests/original_road_support.rs`), and on synthetic cases by upstream
//! Run 018 (`tests/original_support_owner.rs`).

use super::original_support::{SupportPolygon, SupportVertices};
use super::original_support_tree::SupportTree;
use super::original_vehicle::step::{cross, normalize};
use nfs_formats::Result;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SupportPlaneHit {
    pub polygon: usize,
    /// Polygon slot +0x18, index zero; first source vertex in original axes.
    pub point: [f32; 3],
    /// The owner's cached +0x0c normal after 0x474320.
    pub normal: [f32; 3],
    pub height: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OriginalSupportOwner {
    /// Original owner+0x28. This is distinct from the spatial node cache.
    pub polygon: Option<usize>,
    /// Original owner+0x2c.
    pub node: Option<usize>,
    /// `+0x02`: `+0x0c`/`+0x18` hold the current polygon's plane.
    pub normal_valid_02: bool,
    /// `+0x09`: the last `0x474060` changed the polygon.
    pub changed_09: bool,
    /// `+0x0a`: the last tree query found no polygon.
    pub missing_0a: bool,
    /// `+0x0c`.
    pub normal_0c: [f32; 3],
    /// `+0x18`: unit in-plane direction of the edge vertex 0 → vertex 1.
    pub tangent_18: [f32; 3],
    /// `+0x24`: the polygon's flags word (`polygon+0xa`; surface in the low
    /// nibble), 0 without a polygon.
    pub flags_24: u16,
}

/// Polygon vtable +0x14 (quad `0x485b50`, triangle `0x485f90`).
fn normal(polygon: &SupportPolygon) -> [f32; 3] {
    let subtract =
        |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| (a[i] as f64 - b[i] as f64) as f32);
    let (a, b): ([f32; 3], [f32; 3]) = match &polygon.vertices {
        SupportVertices::Triangle(v) => (subtract(v[1], v[0]), subtract(v[0], v[2])),
        SupportVertices::Quad(v) => {
            // 0x485b50 averages opposite edge differences, storing each f32.
            let a = std::array::from_fn(|i| {
                (((v[2][i] as f64 - v[3][i] as f64) + (v[1][i] as f64 - v[0][i] as f64)) * 0.5)
                    as f32
            });
            let b = std::array::from_fn(|i| {
                (((v[0][i] as f64 - v[3][i] as f64) + (v[1][i] as f64 - v[2][i] as f64)) * 0.5)
                    as f32
            });
            (a, b)
        }
    };
    let (a, b) = if polygon.winding_8 { (a, b) } else { (b, a) };
    let mut cross = [
        (a[1] as f64 * b[2] as f64 - a[2] as f64 * b[1] as f64) as f32,
        (a[2] as f64 * b[0] as f64 - a[0] as f64 * b[2] as f64) as f32,
        (a[0] as f64 * b[1] as f64 - a[1] as f64 * b[0] as f64) as f32,
    ];
    let length = ((cross[0] as f64 * cross[0] as f64)
        + (cross[1] as f64 * cross[1] as f64)
        + (cross[2] as f64 * cross[2] as f64))
        .sqrt();
    if length >= 0.01 {
        for component in &mut cross {
            *component = (*component as f64 / length) as f32;
        }
        if cross[1] < f32::from_bits(0x3f7ff972) {
            return cross;
        }
    }
    cross[1] = f32::from_bits(0x3f7ff972);
    cross
}

/// Polygon vtable +0x18: quad `0x485b10` (`i > 3` wraps by `i & 3` with the
/// sign kept, as `cdq`-free `and 0x80000003`), triangle `0x486030`
/// (`i > 2` → `i % 3`).
pub fn polygon_vertex(polygon: &SupportPolygon, i: i32) -> [f32; 3] {
    match &polygon.vertices {
        SupportVertices::Quad(v) => {
            let k = if i > 3 {
                let r = i & (0x8000_0003u32 as i32);
                if r < 0 {
                    ((r - 1) | -4) + 1
                } else {
                    r
                }
            } else {
                i
            };
            v[k as usize]
        }
        SupportVertices::Triangle(v) => v[if i > 2 { i % 3 } else { i } as usize],
    }
}

impl OriginalSupportOwner {
    /// `0x474060 (point, reuse)`: with a cached polygon whose XZ
    /// containment accepts `point` and `reuse` set, keep it (`+0x09` = 0).
    /// Otherwise query the tree (`0x473240`, node cache `+0x2c`), store the
    /// polygon and its flags word, clear `+0x02`, and set `+0x09` when the
    /// polygon changed and `+0x0a` when there is none.
    pub fn locate(
        &mut self,
        tree: &SupportTree,
        polygons: &[SupportPolygon],
        point: [f32; 3],
        reuse: bool,
    ) -> Result<()> {
        if point.iter().any(|value| !value.is_finite()) {
            return Err("nonfinite support owner query outside proven contract".into());
        }
        let old = self.polygon;
        if let Some(index) = old {
            let polygon = polygons
                .get(index)
                .ok_or("cached polygon outside support arena")?;
            if polygon.contains_xz(point) && reuse {
                self.changed_09 = false;
                return Ok(());
            }
        }
        let (index, node) = tree.query(polygons, point, self.node)?;
        self.node = Some(node);
        self.polygon = index;
        self.normal_valid_02 = false;
        match index {
            Some(i) => {
                self.missing_0a = false;
                self.flags_24 = polygons[i].flags_word_a;
            }
            None => {
                self.missing_0a = true;
                self.flags_24 = 0;
            }
        }
        self.changed_09 = index != old;
        Ok(())
    }

    /// `0x474320`: once per polygon, the normal `+0x0c` (vtable +0x14) and
    /// `+0x18 = normalize((e × n) × n)` with `e = v0 − v1`, left unchanged
    /// when that vector is exactly zero.
    fn update_plane(&mut self, polygons: &[SupportPolygon]) {
        let Some(polygon) = self.polygon.and_then(|i| polygons.get(i)) else {
            return;
        };
        if self.normal_valid_02 {
            return;
        }
        let n = normal(polygon);
        self.normal_0c = n;
        let v0 = polygon_vertex(polygon, 0);
        let v1 = polygon_vertex(polygon, 1);
        let e = std::array::from_fn(|i| (v0[i] as f64 - v1[i] as f64) as f32);
        let t = cross(cross(e, n), n);
        // fcomp/C3: NaN counts as zero, like an exact zero.
        if t.iter().any(|c| !(*c == 0.0 || c.is_nan())) {
            self.tangent_18 = normalize(t);
        }
        self.normal_valid_02 = true;
    }

    /// `0x474420`: the normal `+0x0c` (unchanged without a polygon).
    pub fn normal(&mut self, polygons: &[SupportPolygon]) -> [f32; 3] {
        self.update_plane(polygons);
        self.normal_0c
    }

    /// `0x474430`: the edge direction `+0x18` (unchanged without a polygon).
    pub fn tangent(&mut self, polygons: &[SupportPolygon]) -> [f32; 3] {
        self.update_plane(polygons);
        self.tangent_18
    }

    /// `0x4745f0 (i)`: vertex `i` of the polygon, zero without one.
    pub fn vertex(&self, polygons: &[SupportPolygon], i: i32) -> [f32; 3] {
        self.polygon
            .and_then(|k| polygons.get(k))
            .map_or([0.0; 3], |p| polygon_vertex(p, i))
    }

    /// `0x474060`, `0x474420`, `0x4745f0(0)` and the plane height at
    /// `point` (`base.y − ((p − base)·n)_xz / n.y`, as the callers compute
    /// it). `reuse=true` is the 0x474060 caller flag. A zero/nonfinite
    /// normal Y cannot supply a finite height in the bounded portable API;
    /// the original caller divides by that value without a guard.
    pub fn select(
        &mut self,
        tree: &SupportTree,
        polygons: &[SupportPolygon],
        point: [f32; 3],
        reuse: bool,
    ) -> Result<Option<SupportPlaneHit>> {
        if self.polygon.is_some_and(|index| index >= polygons.len()) {
            return Err("cached polygon outside support arena".into());
        }
        self.locate(tree, polygons, point, reuse)?;
        let Some(index) = self.polygon else {
            return Ok(None);
        };
        let base = self.vertex(polygons, 0);
        let normal = self.normal(polygons);
        if !normal[1].is_finite() || normal[1] == 0.0 {
            return Ok(None);
        }
        let height = (base[1] as f64
            - ((point[0] as f64 - base[0] as f64) * normal[0] as f64
                + (point[2] as f64 - base[2] as f64) * normal[2] as f64)
                / normal[1] as f64) as f32;
        if !height.is_finite() {
            return Ok(None);
        }
        Ok(Some(SupportPlaneHit {
            polygon: index,
            point: base,
            normal,
            height,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_support_and_cached_polygon() {
        let vertices =
            |y| SupportVertices::Triangle([[0.0, y, 0.0], [0.0, y, 10.0], [10.0, y, 0.0]]);
        let polygons = vec![
            SupportPolygon::new(vertices(0.0), 1),
            SupportPolygon::new(vertices(10.0), 1),
        ];
        let mut tree = SupportTree::original_scene();
        tree.insert(&polygons, 0).unwrap();
        tree.insert(&polygons, 1).unwrap();
        let mut owner = OriginalSupportOwner::default();
        let first = owner
            .select(&tree, &polygons, [1.0, 1.0, 1.0], true)
            .unwrap()
            .unwrap();
        assert_eq!(first.polygon, 0);
        assert_eq!(first.height, 0.0);
        assert!(owner.changed_09 && !owner.missing_0a && owner.flags_24 == 1);
        let retained = owner
            .select(&tree, &polygons, [1.0, 9.0, 1.0], true)
            .unwrap()
            .unwrap();
        assert_eq!(retained.polygon, 0);
        assert!(!owner.changed_09);
        let fresh = owner
            .select(&tree, &polygons, [1.0, 9.0, 1.0], false)
            .unwrap()
            .unwrap();
        assert_eq!(fresh.polygon, 1);
        assert_eq!(fresh.height, 10.0);
        assert!(owner.changed_09);
    }

    #[test]
    fn vertex_getters_wrap() {
        let t = SupportPolygon::new(
            SupportVertices::Triangle([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]),
            1,
        );
        assert_eq!(polygon_vertex(&t, 3), [0.0, 0.0, 0.0]);
        assert_eq!(polygon_vertex(&t, 4), [1.0, 0.0, 0.0]);
        let q = SupportPolygon::new(
            SupportVertices::Quad([
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ]),
            1,
        );
        assert_eq!(polygon_vertex(&q, 3), [0.0, 0.0, 1.0]);
        assert_eq!(polygon_vertex(&q, 5), [1.0, 0.0, 0.0]);
    }
}

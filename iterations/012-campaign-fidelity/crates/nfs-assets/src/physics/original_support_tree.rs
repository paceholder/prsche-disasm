//! Original polygon/EDG tree insertion, splitting and bounded scene queries.
//! SHA256 ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39.
//! Insert 0x484ae0, split 0x483df0, predicate 0x484320, child 0x483bd0.
//! Win32 locks and allocator layout are platform infrastructure; Vec keeps
//! original record order. See Run 017 original instruction replay.

use super::original_scene_objects::{edge_intersects_segment, SceneObject};
use super::original_spatial::{SpatialGrid, SpatialNode};
use super::original_support::{
    select_support_polygon, SupportLeafRecord, SupportPolygon, SupportVertices,
};
use super::original_support_bounds;
use nfs_formats::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneRef {
    Polygon(usize),
    Object(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneEdgeHit {
    pub object: usize,
    pub endpoints: [[f32; 3]; 2],
    pub word: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SupportTreeNode {
    pub cell: SpatialNode,
    /// None mirrors a null vector pointer, distinct from an allocated empty vector.
    pub records: Option<Vec<SceneRef>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SupportTree {
    pub grid: SpatialGrid,
    pub nodes: Vec<SupportTreeNode>,
}

fn bounds(polygon: &SupportPolygon) -> ([f32; 3], [f32; 3]) {
    let points: &[[f32; 3]] = match &polygon.vertices {
        SupportVertices::Triangle(points) => points,
        SupportVertices::Quad(points) => points,
    };
    original_support_bounds::bounds(points, [0.0; 3], [0.0; 3])
}

impl SupportTree {
    pub fn new(grid: SpatialGrid, root: u32) -> Self {
        assert!(
            root >> 28 <= 12,
            "root beyond original split stopping level"
        );
        Self {
            grid,
            nodes: vec![SupportTreeNode {
                cell: SpatialNode {
                    packed: root,
                    children: [None; 4],
                    records_present: false,
                },
                records: None,
            }],
        }
    }

    /// Original scene setup at 0x487876: half extent 8192, depth 15, root zero.
    pub fn original_scene() -> Self {
        Self::new(SpatialGrid::new(8192.0, 15), 0)
    }

    /// Type-1 polygons with finite geometry only. Invalid caller references
    /// or unsupported float inputs are rejected before mutating the tree.
    pub fn insert(&mut self, polygons: &[SupportPolygon], index: usize) -> Result<()> {
        if self
            .nodes
            .iter()
            .filter_map(|node| node.records.as_ref())
            .flatten()
            .any(|record| matches!(record, SceneRef::Object(_)))
        {
            return Err("mixed tree insertion requires both arenas; use insert_scene".into());
        }
        let polygon = polygons
            .get(index)
            .ok_or("support polygon index outside arena")?;
        let points: &[[f32; 3]] = match &polygon.vertices {
            SupportVertices::Triangle(points) => points,
            SupportVertices::Quad(points) => points,
        };
        if points.iter().flatten().any(|v| !v.is_finite())
            || polygon.center.iter().any(|v| !v.is_finite())
        {
            return Err("nonfinite support geometry outside proven tree contract".into());
        }
        self.insert_at(0, polygons, &[], SceneRef::Polygon(index));
        Ok(())
    }

    /// Original typed record insertion. All record types count toward the
    /// 48-entry split and are redistributed in their original insertion order.
    /// Both immutable arenas are required even when inserting a polygon.
    pub fn insert_scene(
        &mut self,
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) -> Result<()> {
        let valid = match record {
            SceneRef::Polygon(index) => polygons.get(index).is_some_and(|p| {
                p.center.iter().all(|v| v.is_finite())
                    && match &p.vertices {
                        SupportVertices::Triangle(v) => v.iter().flatten().all(|v| v.is_finite()),
                        SupportVertices::Quad(v) => v.iter().flatten().all(|v| v.is_finite()),
                    }
            }),
            SceneRef::Object(index) => objects
                .get(index)
                .is_some_and(|object| matches!(object, SceneObject::Edge(_)) && object.is_finite()),
        };
        if !valid {
            return Err("invalid or unproven mixed scene object".into());
        }
        self.insert_at(0, polygons, objects, record);
        Ok(())
    }

    /// Original scene → cached node → leaf records → selected type-1 polygon.
    /// Returns (polygon index, updated cached node). Polygons must be the same
    /// immutable arena used for insertion; game-unit conversion is not inferred.
    pub fn query(
        &self,
        polygons: &[SupportPolygon],
        point: [f32; 3],
        cached: Option<usize>,
    ) -> Result<(Option<usize>, usize)> {
        if point.iter().any(|value| !value.is_finite()) {
            return Err("nonfinite support query outside proven tree contract".into());
        }
        let retained = if let Some(cached) = cached {
            let node = self
                .nodes
                .get(cached)
                .ok_or("cached support node outside arena")?;
            node.cell.records_present && self.grid.contains(node.cell.packed, point)
        } else {
            false
        };
        let node = if retained {
            cached.unwrap()
        } else {
            self.grid.query_nodes(|i| &self.nodes[i].cell, point)
        };
        let Some(indices) = &self.nodes[node].records else {
            return Ok((None, node));
        };
        let polygon_indices: Vec<usize> = indices
            .iter()
            .filter_map(|record| match record {
                SceneRef::Polygon(index) => Some(*index),
                SceneRef::Object(_) => None,
            })
            .collect();
        let records = polygon_indices
            .iter()
            .map(|index| {
                polygons
                    .get(*index)
                    .map(|polygon| SupportLeafRecord {
                        record_type: 1,
                        polygon,
                    })
                    .ok_or_else(|| "tree polygon reference outside arena".to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((
            select_support_polygon(&records, point).map(|record| polygon_indices[record]),
            node,
        ))
    }

    /// `0x473330 (point, out)` on a locator: the support polygon more than 3.5 above `point`
    /// (the nearest one; centre callback height minus `point.y`, x87 difference, best stored
    /// as f32) among the type-1 records of the point's leaf that contain it in XZ, any
    /// surface. Returns its flags word `+0xa` (0 without one: `R3DCar_UpdateLighting` 0x43a560
    /// reads `& 0xc000` as "covered road") and the cell cache `+0x2c` it leaves. The cache is
    /// kept when it contains the point, whether or not it has records (unlike 0x473240).
    /// Recorded: `scripts/research/replay-car-env-map.py` (`update` rows, Coastal tunnel).
    pub fn overhead_flags(
        &self,
        polygons: &[SupportPolygon],
        point: [f32; 3],
        cached: Option<usize>,
    ) -> (u16, usize) {
        let node = match cached.filter(|&c| {
            self.nodes.get(c).is_some_and(|n| self.grid.contains(n.cell.packed, point))
        }) {
            Some(c) => c,
            None => self.grid.query_nodes(|i| &self.nodes[i].cell, point),
        };
        let Some(records) = &self.nodes[node].records else {
            return (0, node);
        };
        let mut best = f32::from_bits(0x7e96_7699);
        let mut flags = None;
        for record in records {
            let SceneRef::Polygon(index) = *record else {
                continue;
            };
            let Some(polygon) = polygons.get(index) else {
                continue;
            };
            if !polygon.contains_xz(point) {
                continue;
            }
            let d = polygon.center[1] as f64 - point[1] as f64;
            // `fcom 3.5; test ah, 0x41`: skipped when d <= 3.5 or unordered.
            if !(d > 3.5) {
                continue;
            }
            if d < best as f64 {
                best = d as f32;
                flags = Some(polygon.flags_word_a);
            }
        }
        (flags.unwrap_or(0), node)
    }

    /// 0x4740d0 requires a nonnull owner polygon before traversing type-2
    /// records. 0x484630 walks leaves using the first endpoint and the source
    /// radius abs(dx)+abs(dz). It does not use a segment's tight AABB.
    pub fn query_scene_edges(
        &self,
        objects: &[SceneObject],
        segment: [[f32; 3]; 2],
        owner_has_support: bool,
    ) -> Result<Option<SceneEdgeHit>> {
        if !owner_has_support {
            return Ok(None);
        }
        if segment.iter().flatten().any(|v| !v.is_finite()) {
            return Err("nonfinite edge query".into());
        }
        let radius = ((segment[1][0] as f64 - segment[0][0] as f64).abs()
            + (segment[1][2] as f64 - segment[0][2] as f64).abs()) as f32;
        let mut leaves = vec![];
        self.segment_leaves(0, segment[0], radius, &mut leaves);
        let query_y = (segment[0][1] as f64 + segment[1][1] as f64) * 0.5;
        let mut best_distance = f32::from_bits(0x7e967699);
        let mut best = None;
        for node in leaves {
            let records = self.nodes[node]
                .records
                .as_ref()
                .ok_or("original edge traversal reached null leaf vector")?;
            for record in records {
                let SceneRef::Object(index) = *record else {
                    continue;
                };
                let object = objects.get(index).ok_or("edge reference outside arena")?;
                let SceneObject::Edge(edge) = object else {
                    continue;
                };
                if !edge_intersects_segment(edge, segment) {
                    continue;
                }
                let distance = ((edge.endpoints[0][1] as f64 + edge.endpoints[1][1] as f64) * 0.5
                    - query_y)
                    .abs();
                if distance < best_distance as f64 {
                    best_distance = distance as f32;
                    best = Some(SceneEdgeHit {
                        object: index,
                        endpoints: edge.endpoints,
                        word: edge.word,
                    });
                }
            }
        }
        Ok(best)
    }

    fn segment_leaves(&self, node: usize, point: [f32; 3], radius: f32, output: &mut Vec<usize>) {
        let children = self.nodes[node].cell.children;
        if children.iter().all(Option::is_none) {
            output.push(node);
            return;
        }
        let packed = self.nodes[node].cell.packed;
        let width = self.grid.widths[(packed >> 28) as usize] as f64;
        let mid_x =
            (((packed & 0x3fff) as f64 + 0.5) * width - self.grid.half_extent as f64) as f32;
        let mid_z = ((((packed >> 14) & 0x3fff) as f64 + 0.5) * width
            - self.grid.half_extent as f64) as f32;
        let r = radius as f64;
        let negative_x = point[0] as f64 - r < mid_x as f64;
        let positive_x = point[0] as f64 + r >= mid_x as f64;
        let negative_z = point[2] as f64 - r < mid_z as f64;
        let positive_z = point[2] as f64 + r >= mid_z as f64;
        for (slot, accepts) in [
            (2, negative_x && negative_z),
            (0, negative_x && positive_z),
            (3, positive_x && negative_z),
            (1, positive_x && positive_z),
        ] {
            if accepts {
                if let Some(child) = children[slot] {
                    self.segment_leaves(child, point, radius, output);
                }
            }
        }
    }

    fn extent(&self, packed: u32) -> (f32, f32, f32) {
        let width = self.grid.widths[(packed >> 28) as usize];
        let x = ((packed & 0x3fff) as f64 * width as f64 - self.grid.half_extent as f64) as f32;
        let z =
            (((packed >> 14) & 0x3fff) as f64 * width as f64 - self.grid.half_extent as f64) as f32;
        (x, z, width)
    }

    /// 0x484320: inclusive center, then half-open AABB min / inclusive max.
    fn geometry(
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) -> ([f32; 3], [f32; 3], [f32; 3]) {
        match record {
            SceneRef::Polygon(index) => {
                let p = &polygons[index];
                let (lo, hi) = bounds(p);
                (p.center, lo, hi)
            }
            SceneRef::Object(index) => {
                let o = &objects[index];
                let (lo, hi) = o.bounds([0.; 3], [0.; 3]);
                (o.center(), lo, hi)
            }
        }
    }

    fn accepts_node(
        &self,
        node: usize,
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) -> bool {
        let (x, z, width) = self.extent(self.nodes[node].cell.packed);
        let right = (x as f64 + width as f64) as f32;
        let far = (z as f64 + width as f64) as f32;
        let (center, lo, hi) = Self::geometry(polygons, objects, record);
        if center[0] >= x && center[0] <= right && center[2] >= z && center[2] <= far {
            return true;
        }
        lo[0] < right && lo[2] < far && hi[0] >= x && hi[2] >= z
    }

    /// Child filter in 0x484ae0 / 0x483df0 is inclusive at upper bounds.
    /// Allocation precedes the child's own 0x484320 check: an empty child
    /// can therefore remain allocated and must not be pruned from the arena.
    fn accepts_child(
        &self,
        node: usize,
        slot: usize,
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) -> bool {
        let (x, z, width) = self.extent(self.nodes[node].cell.packed);
        let half = (width as f64 * 0.5) as f32;
        let mid_x = (x as f64 + half as f64) as f32;
        let mid_z = (z as f64 + half as f64) as f32;
        let right = slot == 1 || slot == 3;
        let positive_z = slot == 0 || slot == 1;
        let lower_x = if right { mid_x } else { x } as f64;
        let lower_z = if positive_z { mid_z } else { z } as f64;
        let upper_x = if right {
            mid_x as f64 + half as f64
        } else {
            mid_x as f64
        };
        let upper_z = if positive_z {
            mid_z as f64 + half as f64
        } else if slot == 2 {
            z as f64 + half as f64
        } else {
            mid_z as f64
        };
        let (center, lo, hi) = Self::geometry(polygons, objects, record);
        let [cx, _, cz] = center.map(f64::from);
        if lower_x <= cx && cx <= upper_x && lower_z <= cz && cz <= upper_z {
            return true;
        }
        lo[0] as f64 <= upper_x
            && lo[2] as f64 <= upper_z
            && hi[0] as f64 >= lower_x
            && hi[2] as f64 >= lower_z
    }

    fn child(&mut self, node: usize, slot: usize) -> usize {
        if let Some(child) = self.nodes[node].cell.children[slot] {
            return child;
        }
        let child = self.nodes.len();
        self.nodes.push(SupportTreeNode {
            cell: SpatialNode::child(self.nodes[node].cell.packed, slot as u32),
            records: None,
        });
        self.nodes[node].cell.children[slot] = Some(child);
        child
    }

    fn distribute(
        &mut self,
        node: usize,
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) {
        // Source calls children in this order, not pointer-slot order.
        for slot in [2, 0, 1, 3] {
            if self.accepts_child(node, slot, polygons, objects, record) {
                let child = self.child(node, slot);
                self.insert_at(child, polygons, objects, record);
            }
        }
    }

    fn insert_at(
        &mut self,
        node: usize,
        polygons: &[SupportPolygon],
        objects: &[SceneObject],
        record: SceneRef,
    ) {
        if !self.accepts_node(node, polygons, objects, record) {
            return;
        }
        if self.nodes[node].cell.children.iter().any(Option::is_some) {
            self.distribute(node, polygons, objects, record);
            return;
        }
        let records = self.nodes[node]
            .records
            .get_or_insert_with(|| Vec::with_capacity(50));
        records.push(record);
        let split = records.len() > 48 && self.nodes[node].cell.packed >> 28 != 12;
        self.nodes[node].cell.records_present = true;
        if split {
            // Parent vector stays live during original redistribution, and
            // is released only after all records have been processed in order.
            let records = self.nodes[node].records.as_ref().unwrap().clone();
            for record in records {
                self.distribute(node, polygons, objects, record);
            }
            self.nodes[node].records = None;
            self.nodes[node].cell.records_present = false;
        }
    }
}

//! CPU support queries. Real tracks use source-loaded type-1 polygons and
//! original selection/cache/plane callbacks. The legacy triangle/ray adapter
//! remains available for explicit CPU fixtures, without a real-track fallback.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::physics::original_scene_objects::SceneObject;
use crate::physics::original_support::{SupportPolygon, SupportVertices};
use crate::physics::original_support_loader::SupportLoad;
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::physics::original_support_tree::SupportTree;
use crate::physics::original_support_tree::{SceneEdgeHit, SceneRef};

const CELL_SIZE: f32 = 16.0;
const MAX_ABS_COORDINATE: f32 = 1_000_000.0;
const MAX_CELLS_PER_TRIANGLE: i64 = 4096;
const MIN_UP_NORMAL: f32 = 0.01;
// Query coordinates are f32. Permit eight ULPs at the local coordinate scale,
// measured as XZ distance from each projected edge rather than a barycentric ratio.
const EDGE_ULPS: f64 = 8.0;

/// Original CRP identity retained when a triangle is copied before material batching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoadTriangleIdentity {
    pub article_name: String,
    pub article_index: usize,
    pub mesh_name: String,
    pub primitive_index: u16,
    pub triangle_index: usize,
}

/// One candidate support triangle in scene coordinates `[x, y, -z]`.
#[derive(Debug, Clone)]
pub struct RoadTriangle {
    pub identity: RoadTriangleIdentity,
    pub positions: [[f32; 3]; 3],
}

/// A barrier contact: response magnitude (the `0x494000` return that
/// `0x495020` keeps as impact strength `+0x430`) and contact point (`+0x440`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarrierContact {
    pub magnitude: f32,
    pub point: [f32; 3],
}

/// A valid support contact. `identity` can be used by an audit UI or log.
#[derive(Debug, Clone)]
pub struct SurfaceHit {
    pub height: f32,
    pub normal: [f32; 3],
    pub identity: RoadTriangleIdentity,
    /// Original support polygon flags word (polygon+0xa) for the original
    /// backend; the wheel copies it as its surface `w+0x8c` [VA 0x499a70].
    /// `None` for the legacy triangle grid.
    pub flags_word_a: Option<u16>,
}

/// Counts accepted geometry and every input class deliberately rejected by the index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoadSurfaceReport {
    pub accepted: usize,
    pub rejected_degenerate: usize,
    pub rejected_vertical: usize,
    pub rejected_non_finite: usize,
    pub rejected_out_of_bounds: usize,
    pub rejected_giant: usize,
}

#[derive(Debug, Clone)]
struct IndexedTriangle {
    source: RoadTriangle,
    normal: [f32; 3],
}

#[derive(Debug)]
struct OriginalSupportSurface {
    polygons: Vec<SupportPolygon>,
    identities: Vec<RoadTriangleIdentity>,
    tree: SupportTree,
    objects: Vec<SceneObject>,
}

/// Original type-1 support backend for tracks, or an explicit bounded triangle
/// grid for CPU fixtures. Scene clones share the immutable original arena.
#[derive(Debug, Clone, Default)]
pub struct RoadSurface {
    triangles: Vec<IndexedTriangle>,
    grid: BTreeMap<(i32, i32), Vec<usize>>,
    report: RoadSurfaceReport,
    original: Option<Arc<OriginalSupportSurface>>,
}

impl RoadSurface {
    /// Source polygons remain in original XYZ coordinates. Only the scene API
    /// reflects Z, so the original winding/signed-zero tests run unchanged.
    pub fn from_original_support(
        load: SupportLoad,
        article_names: &[String],
    ) -> Result<Self, String> {
        Self::from_original_scene_support(load, article_names, vec![])
    }

    /// Retained scene insertion order 0x4883bc then 0x4883f5: original
    /// flag-1 polygons first, constructed EDG objects second. Special objects
    /// and the no-EDG temporary-tree fallback remain outside this loader.
    pub fn from_original_scene_support(
        load: SupportLoad,
        article_names: &[String],
        objects: Vec<SceneObject>,
    ) -> Result<Self, String> {
        let mut surface = Self::default();
        let mut polygons = Vec::with_capacity(load.polygons.len());
        let mut identities = Vec::with_capacity(load.polygons.len());
        for assembled in load.polygons {
            let identity = RoadTriangleIdentity {
                article_name: article_names
                    .get(assembled.article_index)
                    .ok_or("support article identity outside archive")?
                    .clone(),
                article_index: assembled.article_index,
                mesh_name: format!("original/{}", assembled.primitive_index),
                primitive_index: assembled.primitive_index,
                triangle_index: assembled.polygon_index as usize,
            };
            // The triangulated copy is for existing read-only geometry audits.
            // Live selection uses the original tri/quad arena below.
            let pieces = match &assembled.polygon.vertices {
                SupportVertices::Triangle(p) => vec![*p],
                SupportVertices::Quad(p) => vec![[p[0], p[1], p[2]], [p[0], p[2], p[3]]],
            };
            for positions in pieces {
                let positions = positions.map(crate::track_loader::to_scene_coordinates);
                surface.triangles.push(IndexedTriangle {
                    normal: triangle_normal(positions),
                    source: RoadTriangle {
                        identity: identity.clone(),
                        positions,
                    },
                });
            }
            identities.push(identity);
            polygons.push(assembled.polygon);
        }
        let mut tree = SupportTree::original_scene();
        for index in 0..polygons.len() {
            tree.insert_scene(&polygons, &objects, SceneRef::Polygon(index))?;
        }
        for index in 0..objects.len() {
            tree.insert_scene(&polygons, &objects, SceneRef::Object(index))?;
        }
        surface.report.accepted = polygons.len();
        surface.report.rejected_degenerate = load.rejected_degenerate;
        surface.original = Some(Arc::new(OriginalSupportSurface {
            polygons,
            identities,
            tree,
            objects,
        }));
        Ok(surface)
    }

    pub fn uses_original_support(&self) -> bool {
        self.original.is_some()
    }

    pub fn original_scene_object_count(&self) -> Option<usize> {
        self.original.as_ref().map(|source| source.objects.len())
    }

    /// Expose the restored original edge query in scene coordinates. Callers
    /// must supply the original owner gate; body response binding is separate.
    pub fn original_edge_query(
        &self,
        segment: [[f32; 3]; 2],
        owner_has_support: bool,
    ) -> Result<Option<SceneEdgeHit>, String> {
        let source = self.original.as_ref().ok_or("no original scene backend")?;
        let segment = segment.map(crate::track_loader::to_scene_coordinates);
        let hit = source
            .tree
            .query_scene_edges(&source.objects, segment, owner_has_support)?;
        Ok(hit.map(|mut hit| {
            hit.endpoints = hit.endpoints.map(crate::track_loader::to_scene_coordinates);
            hit
        }))
    }

    /// Endpoints of the original scene's barrier edges, in original world
    /// coordinates (empty without the original backend).
    pub fn original_barrier_edges(&self) -> Vec<[[f32; 3]; 2]> {
        use crate::physics::original_scene_objects::SceneObject;
        self.original
            .as_ref()
            .map(|source| {
                source
                    .objects
                    .iter()
                    .filter_map(|object| match object {
                        SceneObject::Edge(edge) => Some(edge.endpoints),
                        SceneObject::Special(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Original barrier pass for one car, in original world coordinates:
    /// `Car_CollideTrackEdges` 0x495020 selects the first body point crossing a
    /// scene edge, then 0x494000 responds (angular flag 1, as the original
    /// passes at 0x495020+). `body_owner` is the car's persistent support
    /// owner (car+8); it is refreshed at the car position first.
    /// Hypothesis: the original refreshes car+8 during body integration.
    /// Returns the response magnitude, or `None` without a contact.
    pub fn original_barrier_response(
        &self,
        state: &mut crate::physics::original_contact::PreparedContactState,
        extents: [f32; 3],
        body_owner: &mut OriginalSupportOwner,
    ) -> Result<Option<f32>, String> {
        Ok(self
            .original_barrier_contact(state, extents, body_owner)?
            .map(|hit| hit.magnitude))
    }

    /// As [`Self::original_barrier_response`], also returning the contact point
    /// that `Car_CollideTrackEdges` 0x495020 stores at `+0x440` (original space).
    pub fn original_barrier_contact(
        &self,
        state: &mut crate::physics::original_contact::PreparedContactState,
        extents: [f32; 3],
        body_owner: &mut OriginalSupportOwner,
    ) -> Result<Option<BarrierContact>, String> {
        use crate::physics::original_body_contact::{first_body_contact, BodyContactInput};
        use crate::physics::original_scene_contact::{respond_scene_contact, SceneContactRequest};
        let source = self.original.as_ref().ok_or("no original scene backend")?;
        body_owner.select(&source.tree, &source.polygons, state.position_330, true)?;
        let input = BodyContactInput {
            position_330: state.position_330,
            velocity_33c: state.velocity_33c,
            basis_364: state.basis_364,
            basis_370: state.basis_370,
            basis_37c: state.basis_37c,
            extents_3a8: extents,
        };
        let Some(contact) = first_body_contact(
            input,
            body_owner,
            &source.tree,
            &source.polygons,
            &source.objects,
        )?
        else {
            return Ok(None);
        };
        let result = respond_scene_contact(
            state,
            body_owner,
            &source.tree,
            &source.polygons,
            SceneContactRequest {
                normal: contact.normal,
                point: contact.point,
                displacement: contact.displacement,
                alternate_52c_mask_04: false,
                angular_callback: true,
            },
        )?;
        Ok(Some(BarrierContact {
            magnitude: result.magnitude,
            point: contact.point,
        }))
    }

    /// Original support polygon `index` (original axes), for audits.
    pub fn original_polygon(&self, index: usize) -> Option<&SupportPolygon> {
        self.original.as_ref()?.polygons.get(index)
    }

    /// The retained support tree and its polygons (original axes), for the
    /// owner methods of [`OriginalSupportOwner`].
    pub fn original_arena(&self) -> Option<(&SupportTree, &[SupportPolygon])> {
        let source = self.original.as_ref()?;
        Some((&source.tree, &source.polygons))
    }

    pub fn original_support_counts(&self) -> Option<(usize, usize)> {
        self.original
            .as_ref()
            .map(|source| (source.polygons.len(), source.tree.nodes.len()))
    }

    /// Original wheel call 0x499b64 uses per-wheel owner state and permits
    /// polygon reuse. No legacy RD grid fallback is used when this backend exists.
    pub fn original_support(
        &self,
        point: [f32; 3],
        owner: &mut OriginalSupportOwner,
    ) -> Option<SurfaceHit> {
        let original = self.original.as_ref()?;
        let point = crate::track_loader::to_scene_coordinates(point);
        let hit = owner
            .select(&original.tree, &original.polygons, point, true)
            .ok()??;
        Some(SurfaceHit {
            height: hit.height,
            normal: crate::track_loader::to_scene_coordinates(hit.normal),
            identity: original.identities[hit.polygon].clone(),
            flags_word_a: Some(original.polygons[hit.polygon].flags_word_a),
        })
    }

    /// Builds a grid while rejecting malformed and impractically large triangles.
    pub fn from_triangles(triangles: impl IntoIterator<Item = RoadTriangle>) -> Self {
        let mut surface = Self::default();
        for triangle in triangles {
            surface.insert(triangle);
        }
        surface
    }

    pub fn report(&self) -> RoadSurfaceReport {
        self.report
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Accepted source triangles for a read-only audit. The iterator exposes no grid internals.
    pub fn triangles(
        &self,
    ) -> impl Iterator<Item = (&RoadTriangleIdentity, &[[f32; 3]; 3], [f32; 3])> + '_ {
        self.triangles.iter().map(|triangle| {
            (
                &triangle.source.identity,
                &triangle.source.positions,
                triangle.normal,
            )
        })
    }

    /// Returns the nearest permitted support to `reference_y`, rather than the
    /// globally highest deck. `max_step_up` and `max_drop` are non-negative distances.
    pub fn query(
        &self,
        x: f32,
        z: f32,
        reference_y: f32,
        max_step_up: f32,
        max_drop: f32,
    ) -> Option<SurfaceHit> {
        if ![x, z, reference_y, max_step_up, max_drop]
            .iter()
            .all(|value| value.is_finite())
            || max_step_up < 0.0
            || max_drop < 0.0
        {
            return None;
        }
        if self.original.is_some() {
            let hit =
                self.original_support([x, reference_y, z], &mut OriginalSupportOwner::default())?;
            return (hit.height <= reference_y + max_step_up
                && hit.height >= reference_y - max_drop)
                .then_some(hit);
        }
        let cell = cell_for(x, z)?;
        let mut best: Option<(f32, &IndexedTriangle, f32)> = None;
        for &index in self.grid.get(&cell)? {
            let triangle = &self.triangles[index];
            let Some(height) = height_at(&triangle.source.positions, x, z) else {
                continue;
            };
            if height > reference_y + max_step_up || height < reference_y - max_drop {
                continue;
            }
            let distance = (height - reference_y).abs();
            if match best.as_ref() {
                Some((best_distance, _, _)) => distance < *best_distance,
                None => true,
            } {
                best = Some((distance, triangle, height));
            }
        }
        best.map(|(_, triangle, height)| SurfaceHit {
            height,
            normal: triangle.normal,
            identity: triangle.source.identity.clone(),
            flags_word_a: None,
        })
    }

    /// Legacy fixture adapter: cast along local down against finite triangles.
    /// The signed distance permits a small existing ground penetration to recover.
    /// Upward/back-face rays cannot attach inverted wheels to the road.
    pub fn suspension_ray(
        &self,
        origin: glam::Vec3,
        up: glam::Vec3,
        reach: f32,
        penetration: f32,
    ) -> Option<(f32, SurfaceHit)> {
        if !origin.is_finite()
            || !up.is_finite()
            || !reach.is_finite()
            || !penetration.is_finite()
            || reach < 0.0
            || penetration < 0.0
            || (up.length_squared() - 1.0).abs() > 0.001
        {
            return None;
        }
        let a = origin + up * penetration;
        let b = origin - up * reach;
        let min = a.min(b);
        let max = a.max(b);
        let (min_x, min_z) = cell_for(min.x, min.z)?;
        let (max_x, max_z) = cell_for(max.x, max.z)?;
        let cells =
            (i64::from(max_x) - i64::from(min_x) + 1) * (i64::from(max_z) - i64::from(min_z) + 1);
        if cells > MAX_CELLS_PER_TRIANGLE {
            return None;
        }
        let mut best: Option<(f32, &IndexedTriangle, f32)> = None;
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let Some(indices) = self.grid.get(&(x, z)) else {
                    continue;
                };
                for &index in indices {
                    let triangle = &self.triangles[index];
                    let normal = glam::Vec3::from_array(triangle.normal);
                    let denominator = up.dot(normal);
                    if denominator <= 0.01 {
                        continue;
                    }
                    let distance = (origin - glam::Vec3::from_array(triangle.source.positions[0]))
                        .dot(normal)
                        / denominator;
                    if distance < -penetration || distance > reach {
                        continue;
                    }
                    let point = origin - up * distance;
                    let Some(height) = height_at(&triangle.source.positions, point.x, point.z)
                    else {
                        continue;
                    };
                    if best
                        .as_ref()
                        .is_none_or(|(old, _, _)| distance.abs() < old.abs())
                    {
                        best = Some((distance, triangle, height));
                    }
                }
            }
        }
        best.map(|(distance, triangle, height)| {
            (
                distance,
                SurfaceHit {
                    height,
                    normal: triangle.normal,
                    identity: triangle.source.identity.clone(),
                    flags_word_a: None,
                },
            )
        })
    }

    fn insert(&mut self, triangle: RoadTriangle) {
        if triangle
            .positions
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            self.report.rejected_non_finite += 1;
            return;
        }
        if triangle
            .positions
            .iter()
            .flatten()
            .any(|value| value.abs() > MAX_ABS_COORDINATE)
        {
            self.report.rejected_out_of_bounds += 1;
            return;
        }
        let normal = triangle_normal(triangle.positions);
        let length_sq = dot(normal, normal);
        if length_sq <= 1e-12 {
            self.report.rejected_degenerate += 1;
            return;
        }
        let mut normal = normal.map(|value| value / length_sq.sqrt());
        if normal[1].abs() < MIN_UP_NORMAL {
            self.report.rejected_vertical += 1;
            return;
        }
        if normal[1] < 0.0 {
            normal = normal.map(|value| -value);
        }
        let min_x = triangle
            .positions
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = triangle
            .positions
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_z = triangle
            .positions
            .iter()
            .map(|point| point[2])
            .fold(f32::INFINITY, f32::min);
        let max_z = triangle
            .positions
            .iter()
            .map(|point| point[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let Some((min_cell_x, min_cell_z)) = cell_for(min_x, min_z) else {
            self.report.rejected_out_of_bounds += 1;
            return;
        };
        let Some((max_cell_x, max_cell_z)) = cell_for(max_x, max_z) else {
            self.report.rejected_out_of_bounds += 1;
            return;
        };
        let cells = i64::from(max_cell_x - min_cell_x + 1) * i64::from(max_cell_z - min_cell_z + 1);
        if cells > MAX_CELLS_PER_TRIANGLE {
            self.report.rejected_giant += 1;
            return;
        }
        let index = self.triangles.len();
        self.triangles.push(IndexedTriangle {
            source: triangle,
            normal,
        });
        for cell_x in min_cell_x..=max_cell_x {
            for cell_z in min_cell_z..=max_cell_z {
                self.grid.entry((cell_x, cell_z)).or_default().push(index);
            }
        }
        self.report.accepted += 1;
    }
}

fn cell_for(x: f32, z: f32) -> Option<(i32, i32)> {
    if !x.is_finite()
        || !z.is_finite()
        || x.abs() > MAX_ABS_COORDINATE
        || z.abs() > MAX_ABS_COORDINATE
    {
        return None;
    }
    Some((
        (x / CELL_SIZE).floor() as i32,
        (z / CELL_SIZE).floor() as i32,
    ))
}

fn triangle_normal(points: [[f32; 3]; 3]) -> [f32; 3] {
    let a = subtract(points[1], points[0]);
    let b = subtract(points[2], points[0]);
    cross(a, b)
}

fn height_at(points: &[[f32; 3]; 3], x: f32, z: f32) -> Option<f32> {
    let a = points[0].map(f64::from);
    let b = points[1].map(f64::from);
    let c = points[2].map(f64::from);
    let x = f64::from(x);
    let z = f64::from(z);
    let denominator = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let u = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / denominator;
    let v = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / denominator;
    let w = 1.0 - u - v;
    let coordinate_scale = a
        .into_iter()
        .chain(b)
        .chain(c)
        .chain([x, z])
        .map(f64::abs)
        .fold(1.0, f64::max);
    let edge_tolerance = EDGE_ULPS * f64::from(f32::EPSILON) * coordinate_scale;
    let denominator = denominator.abs();
    let u_tolerance = edge_tolerance * distance_xz(b, c) / denominator;
    let v_tolerance = edge_tolerance * distance_xz(c, a) / denominator;
    let w_tolerance = edge_tolerance * distance_xz(a, b) / denominator;
    if u < -u_tolerance || v < -v_tolerance || w < -w_tolerance {
        return None;
    }
    Some((u * a[1] + v * b[1] + w * c[1]) as f32)
}

fn distance_xz(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(name: &str, points: [[f32; 3]; 3]) -> RoadTriangle {
        RoadTriangle {
            identity: RoadTriangleIdentity {
                article_name: name.into(),
                article_index: 7,
                mesh_name: format!("{name}#7/0"),
                primitive_index: 0,
                triangle_index: 3,
            },
            positions: points,
        }
    }

    #[test]
    fn returns_incline_height_upward_normal_and_identity() {
        let surface = RoadSurface::from_triangles([triangle(
            "RD0001",
            [[0.0, 0.0, 0.0], [8.0, 4.0, 0.0], [0.0, 0.0, 8.0]],
        )]);
        let hit = surface.query(2.0, 2.0, 0.8, 1.0, 1.0).unwrap();
        assert!((hit.height - 1.0).abs() < 1e-5);
        assert!(hit.normal[1] > 0.89);
        assert_eq!(hit.identity.article_name, "RD0001");
    }

    #[test]
    fn accepts_triangle_edges_and_misses_outside() {
        let surface = RoadSurface::from_triangles([triangle(
            "RD_EDGE",
            [[0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [0.0, 0.0, 8.0]],
        )]);
        assert!(surface.query(4.0, 0.0, 0.0, 0.0, 0.0).is_some());
        assert!(surface.query(6.0, 6.0, 0.0, 1.0, 1.0).is_none());
    }

    #[test]
    fn queries_a_real_world_thin_triangle_at_its_f32_centroid() {
        // monaco3 CRP audit: RD0144C (1 PRIMARY)#408, primitive 0, triangle 3.
        // The projected triangle is thin enough that f32 centroid rounding changes its
        // raw barycentric coordinates substantially; the point is still micrometres
        // from the projected edge at this coordinate scale.
        let points = [
            [-72.82958, -7.735953, 64.55752],
            [-72.45224, -7.582618, 60.22616],
            [-72.9732, -7.794313, 66.20606],
        ];
        let point: [f32; 3] =
            std::array::from_fn(|axis| (points[0][axis] + points[1][axis] + points[2][axis]) / 3.0);
        let surface = RoadSurface::from_triangles([triangle("RD0144C (1 PRIMARY)", points)]);
        let hit = surface
            .query(point[0], point[2], point[1], 0.01, 0.01)
            .unwrap();
        assert!((hit.height - point[1]).abs() <= 0.01);
    }

    #[test]
    fn chooses_nearest_allowed_stacked_deck() {
        let lower = triangle(
            "RD_LOW",
            [[0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [0.0, 0.0, 8.0]],
        );
        let upper = triangle(
            "RD_HIGH",
            [[0.0, 10.0, 0.0], [8.0, 10.0, 0.0], [0.0, 10.0, 8.0]],
        );
        let surface = RoadSurface::from_triangles([lower, upper]);
        assert_eq!(
            surface
                .query(1.0, 1.0, 1.0, 2.0, 2.0)
                .unwrap()
                .identity
                .article_name,
            "RD_LOW"
        );
        assert_eq!(
            surface
                .query(1.0, 1.0, 9.0, 2.0, 2.0)
                .unwrap()
                .identity
                .article_name,
            "RD_HIGH"
        );
        assert!(surface.query(1.0, 1.0, 5.0, 1.0, 1.0).is_none());
    }

    #[test]
    fn rejects_invalid_vertical_and_unbounded_triangles() {
        let degenerate = triangle("RD_DEG", [[0.0, 0.0, 0.0]; 3]);
        let vertical = triangle(
            "RD_WALL",
            [[0.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
        );
        let non_finite = triangle(
            "RD_NAN",
            [[f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        );
        let giant = triangle(
            "RD_GIANT",
            [
                [-900_000.0, 0.0, -900_000.0],
                [900_000.0, 0.0, -900_000.0],
                [-900_000.0, 0.0, 900_000.0],
            ],
        );
        let surface = RoadSurface::from_triangles([degenerate, vertical, non_finite, giant]);
        assert_eq!(surface.triangle_count(), 0);
        assert_eq!(
            surface.report(),
            RoadSurfaceReport {
                accepted: 0,
                rejected_degenerate: 1,
                rejected_vertical: 1,
                rejected_non_finite: 1,
                rejected_out_of_bounds: 0,
                rejected_giant: 1,
            }
        );
    }

    #[test]
    fn suspension_ray_crosses_cells_and_uses_finite_sloped_triangles() {
        let up = glam::Vec3::new(-0.6, 0.8, 0.0);
        let flat = RoadSurface::from_triangles([triangle(
            "pad",
            [[16.0, 0.0, -1.0], [18.0, 0.0, -1.0], [16.0, 0.0, 1.0]],
        )]);
        let origin = glam::Vec3::new(15.8, 0.4, 0.0);
        assert!(flat.query(origin.x, origin.z, origin.y, 0.0, 1.0).is_none());
        let (distance, hit) = flat
            .suspension_ray(origin, up, 0.6, 0.49)
            .expect("tilted ray enters the neighboring grid cell and pad triangle");
        assert!((distance - 0.5).abs() < 1e-5);
        assert_eq!(hit.identity.article_name, "pad");
        assert!(flat.suspension_ray(origin, up, 0.4, 0.49).is_none());
        assert!(flat.suspension_ray(origin, -up, 0.6, 0.49).is_none());
        assert!(flat
            .suspension_ray(origin + glam::Vec3::Z * 2.0, up, 0.6, 0.49)
            .is_none());
        let slope = RoadSurface::from_triangles([triangle(
            "ramp",
            [[16.0, 0.0, -1.0], [18.0, 2.0, -1.0], [16.0, 0.0, 1.0]],
        )]);
        let origin = glam::Vec3::new(16.3, 1.0, 0.0);
        let (distance, hit) = slope.suspension_ray(origin, up, 0.6, 0.49).unwrap();
        assert!((distance - 0.5).abs() < 1e-5);
        assert!((hit.height - 0.6).abs() < 1e-5);
        let (_, recovered) = flat
            .suspension_ray(glam::Vec3::new(16.2, -0.05, 0.0), glam::Vec3::Y, 0.6, 0.1)
            .unwrap();
        assert_eq!(recovered.height, 0.0);
    }

    #[test]
    fn invalid_query_input_is_a_miss() {
        let surface = RoadSurface::default();
        assert!(surface.query(f32::NAN, 0.0, 0.0, 1.0, 1.0).is_none());
        assert!(surface.query(0.0, 0.0, 0.0, -1.0, 1.0).is_none());
    }
}

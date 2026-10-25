//! Collision objects of a track, built the way the loader `0x4750b0` does
//! (flag 1 build, `docs/re/track-objects.md` section 2).
//!
//! An article whose `animdefs.txt` `collideType` is `kCylinder` (1) or `kBox`
//! (2) and whose Base word 0 has bit 0x8000 clear becomes a
//! `WTreeElem_CylinderObject` (ctor `0x486690`) or `WTreeElem_BoxObject`
//! (ctor `0x486c20`) instead of support polygons [VA 0x4751b7..0x475238].
//!
//! Approximation A170: the eight corners the constructors take from the scene
//! article provider (`0x486340`, provider vtable slot +8) are taken as the
//! `Base+0x14` bounding box of the article, moved by its `tr` matrix (row
//! vector times 4x4); the provider was not recovered. The box footprint (local
//! `vt` vertices, then moved by `tr`, as the matrix call at 0x4752a... suggests)
//! follows `0x474cb0` (smallest axis-aligned rectangle over
//! a 0.1 degree rotation sweep of the vertices in XZ) without the convex-hull
//! step `0x5a0480`, which cannot change the extents.

use nfs_formats::{Entry, Result, TrackCrp, animdefs::AnimDefs};

use super::original_support_assembly::SelectedBaseMask;

/// Radius of every cylinder object: the literal 0.25 pushed at 0x4751d8 and
/// clamped to at least 0.25 by `0x486690` [VA 0x4866b3..0x4866d1].
pub const CYLINDER_RADIUS: f32 = 0.25;

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectShape {
    /// `WTreeElem_CylinderObject`: circle of [`CYLINDER_RADIUS`] at the centre
    /// of the bounds.
    Cylinder,
    /// `WTreeElem_BoxObject`: footprint rectangle corners (y = 0) and the
    /// bounding radius `+0x64` = 1.05 x distance (centre to max corner).
    Box { corners: [[f32; 3]; 4], radius: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackObject {
    pub article_index: usize,
    /// The article's `Name` (debug reports).
    pub name: String,
    /// FourCC at Base+0x44 (little-endian `u32` of the four bytes).
    pub tag: u32,
    /// `obj+0x20`: the animdefs `radius` truncated to an integer
    /// [VA 0x4751d5..0x4751dd]; low word of the contact code `0x5xxxx`.
    pub id: u32,
    /// `obj+0x24`: the animdefs `drivethru` byte is non-zero.
    pub drive_through: bool,
    /// `obj+8..+0x13` and `obj+0x14..+0x1f`.
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub shape: ObjectShape,
}

impl TrackObject {
    /// Vtable slot +4: the centre of the bounds [VA 0x486490].
    pub fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|i| {
            ((self.bounds_max[i] as f64 + self.bounds_min[i] as f64) * 0.5) as f32
        })
    }

    /// Vtable slot +0x14: the broad radius [VA 0x4866e0, 0x486d10].
    pub fn radius(&self) -> f32 {
        match self.shape {
            ObjectShape::Cylinder => CYLINDER_RADIUS,
            ObjectShape::Box { radius, .. } => radius,
        }
    }

    /// `0x486510(point, r)`: whether the XZ bounds grown by `r` contain the point.
    pub fn near_point(&self, point: [f32; 3], r: f32) -> bool {
        let (px, pz) = (point[0] as f64, point[2] as f64);
        let r = r as f64;
        px + r >= self.bounds_min[0] as f64
            && px - r <= self.bounds_max[0] as f64
            && pz + r >= self.bounds_min[2] as f64
            && pz - r <= self.bounds_max[2] as f64
    }
}

fn word(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("object resource index outside payload")?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

/// `tr` entry: a 4x4 matrix, row vectors, translation in row 3.
fn article_matrix(article: &Entry) -> [f32; 16] {
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let Some(tr) = article
        .children
        .iter()
        .find(|e| e.tag == "tr" && e.index == 0)
    else {
        return identity;
    };
    if tr.data.len() < 64 {
        return identity;
    }
    std::array::from_fn(|i| f32::from_le_bytes(tr.data[i * 4..i * 4 + 4].try_into().unwrap()))
}

fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| {
        (p[0] as f64 * m[c] as f64
            + p[1] as f64 * m[4 + c] as f64
            + p[2] as f64 * m[8 + c] as f64
            + m[12 + c] as f64) as f32
    })
}

fn selected_vt(article: &Entry, resource: u16) -> Option<&Entry> {
    if resource == u16::MAX {
        article
            .children
            .iter()
            .find(|entry| entry.tag == "vt" && entry.index == 0)
    } else {
        article.find("vt", resource)
    }
}

/// The smallest axis-aligned rectangle (by area) around `points` (x, z) over
/// rotations of 0, 0.1, 0.2 ... < 90 degrees, as `0x474cb0` does; returns the
/// four rectangle corners in the original order (min,min), (min,max),
/// (max,max), (max,min) in the original frame, y = 0.
pub fn footprint_rectangle(points: &[[f32; 3]]) -> Option<[[f32; 3]; 4]> {
    if points.is_empty() {
        return None;
    }
    // Pivot: first point that attains the largest squared XZ distance to any
    // point [VA 0x474cb0 loops].
    let mut best_d = 0.0_f32;
    let mut pivot = 0usize;
    for (i, a) in points.iter().enumerate() {
        for b in points {
            let d = (a[2] - b[2]) * (a[2] - b[2]) + (a[0] - b[0]) * (a[0] - b[0]);
            if best_d < d {
                best_d = d;
                pivot = i;
            }
        }
    }
    let (cx, cz) = (points[pivot][0], points[pivot][2]);
    let local: Vec<[f32; 2]> = points.iter().map(|p| [p[0] - cx, p[2] - cz]).collect();
    let rotate = |p: [f32; 2], degrees: f32| -> [f32; 2] {
        let r = (degrees as f64).to_radians();
        let (s, c) = (r.sin(), r.cos());
        // Row vector times the 0x539bd0 matrix: x' = x c + z s, z' = -x s + z c.
        [
            (p[0] as f64 * c + p[1] as f64 * s) as f32,
            (-(p[0] as f64) * s + p[1] as f64 * c) as f32,
        ]
    };
    let mut best_area = 1e10_f32;
    let mut best_angle = 0.0_f32;
    let mut best = [0.0_f32; 4]; // min x, min z, max x, max z
    let mut angle = 0.0_f32;
    while angle < 90.0 {
        let mut e = [1e9_f32, 1e9, -1e9, -1e9];
        for p in &local {
            let q = rotate(*p, angle);
            e[0] = e[0].min(q[0]);
            e[1] = e[1].min(q[1]);
            e[2] = e[2].max(q[0]);
            e[3] = e[3].max(q[1]);
        }
        let area = (e[2] - e[0]) * (e[3] - e[1]);
        if area < best_area {
            best_area = area;
            best_angle = angle;
            best = e;
        }
        angle = (angle as f64 + 0.1_f32 as f64) as f32;
    }
    let corners = [
        [best[0], best[1]],
        [best[0], best[3]],
        [best[2], best[3]],
        [best[2], best[1]],
    ];
    Some(corners.map(|c| {
        let q = rotate(c, -best_angle);
        [q[0] + cx, 0.0, q[1] + cz]
    }))
}

/// Build the objects of a track CRP. `ordinals` is the scene's current Base+4
/// per article (`assemble_initial_track_support` supplies the stored values).
pub fn build_track_objects(
    archive: &TrackCrp,
    anim: &AnimDefs,
    ordinals: &[u32],
) -> Result<Vec<TrackObject>> {
    if archive.articles.len() != ordinals.len() {
        return Err("current Base ordinal missing for one or more articles".into());
    }
    let mut objects = Vec::new();
    for (index, (article, ordinal)) in archive.articles.iter().zip(ordinals).enumerate() {
        let base = article
            .children
            .iter()
            .find(|entry| entry.tag == "Base")
            .ok_or("object article lacks Base")?;
        if word(&base.data, 0)? & 0x8000 != 0 {
            continue; // library prop template
        }
        let mut selected = base.data.clone();
        selected
            .get_mut(4..8)
            .ok_or("Base lacks current ordinal")?
            .copy_from_slice(&ordinal.to_le_bytes());
        let Some(mask) = SelectedBaseMask::from_payload(&selected)? else {
            continue;
        };
        let tag = word(&base.data, 0x44)?;
        let Some(def) = anim.find_tag_or_first(tag) else {
            continue;
        };
        if !matches!(def.collide_type, 1 | 2) {
            continue;
        }
        let vt = selected_vt(article, mask.vertex_and_color_resource)
            .ok_or("selected object vt missing")?;
        if vt.data.len() < 16 || vt.data.len() % 16 != 0 {
            continue;
        }
        let points: Vec<[f32; 3]> = vt
            .data
            .chunks_exact(16)
            .map(|c| {
                [0, 4, 8]
                    .map(|o| f32::from_bits(u32::from_le_bytes(c[o..o + 4].try_into().unwrap())))
            })
            .collect();
        if points.iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        // Base+0x14..+0x2b: object-space bounding box; its 8 corners moved by `tr`.
        let b = |o: usize| -> Result<f32> { Ok(f32::from_bits(word(&base.data, o)?)) };
        let (lo, hi) = (
            [b(0x14)?, b(0x18)?, b(0x1c)?],
            [b(0x20)?, b(0x24)?, b(0x28)?],
        );
        let matrix = article_matrix(article);
        let mut bounds_min = [f32::MAX; 3];
        let mut bounds_max = [f32::MIN; 3];
        for corner in 0..8 {
            let p = transform_point(
                &matrix,
                [
                    if corner & 1 == 0 { lo[0] } else { hi[0] },
                    if corner & 2 == 0 { lo[1] } else { hi[1] },
                    if corner & 4 == 0 { lo[2] } else { hi[2] },
                ],
            );
            for i in 0..3 {
                bounds_min[i] = bounds_min[i].min(p[i]);
                bounds_max[i] = bounds_max[i].max(p[i]);
            }
        }
        let shape = if def.collide_type == 1 {
            ObjectShape::Cylinder
        } else {
            let Some(local_corners) = footprint_rectangle(&points) else {
                continue;
            };
            let corners = local_corners.map(|c| {
                let mut w = transform_point(&matrix, c);
                w[1] = 0.0;
                w
            });
            // 0x486c47..0x486c85: half-extent vector of the bounds in XZ, x
            // measured from the max corner, z from the centre.
            let cz = (bounds_min[2] as f64 + bounds_max[2] as f64) as f32 * 0.5;
            let dx = bounds_max[0] - (bounds_min[0] + bounds_max[0]) * 0.5;
            let dz = bounds_max[2] - cz;
            let radius = ((dx as f64).hypot(dz as f64) * 1.05) as f32;
            ObjectShape::Box { corners, radius }
        };
        objects.push(TrackObject {
            article_index: index,
            name: article
                .find("Name", 0)
                .map(|e| String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string())
                .unwrap_or_default(),
            tag,
            id: def.radius as i32 as u32,
            drive_through: def.drive_through,
            bounds_min,
            bounds_max,
            shape,
        });
    }
    Ok(objects)
}

/// `build_track_objects` with the stored Base+4 ordinals.
pub fn build_initial_track_objects(
    archive: &TrackCrp,
    anim: &AnimDefs,
) -> Result<Vec<TrackObject>> {
    let ordinals = archive
        .articles
        .iter()
        .map(|article| {
            let base = article
                .children
                .iter()
                .find(|entry| entry.tag == "Base")
                .ok_or("object article lacks Base")?;
            word(&base.data, 4)
        })
        .collect::<Result<Vec<_>>>()?;
    build_track_objects(archive, anim, &ordinals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_of_axis_aligned_points_is_the_bounds() {
        let pts = [[0., 5., 0.], [4., 1., 0.], [4., 2., 2.], [0., 3., 2.]];
        let c = footprint_rectangle(&pts).unwrap();
        let xs: Vec<f32> = c.iter().map(|p| p[0]).collect();
        let zs: Vec<f32> = c.iter().map(|p| p[2]).collect();
        assert!((xs.iter().cloned().fold(f32::MAX, f32::min) - 0.0).abs() < 1e-4);
        assert!((xs.iter().cloned().fold(f32::MIN, f32::max) - 4.0).abs() < 1e-4);
        assert!((zs.iter().cloned().fold(f32::MAX, f32::min) - 0.0).abs() < 1e-4);
        assert!((zs.iter().cloned().fold(f32::MIN, f32::max) - 2.0).abs() < 1e-4);
        assert!(c.iter().all(|p| p[1] == 0.0));
    }

    #[test]
    fn rotated_rectangle_is_recovered_with_its_own_area() {
        // 3 x 1 rectangle turned by 30 degrees (a multiple of the 0.1 step).
        let (s, c) = (30f32.to_radians().sin(), 30f32.to_radians().cos());
        let pts: Vec<[f32; 3]> = [[0., 0.], [3., 0.], [3., 1.], [0., 1.]]
            .iter()
            .map(|p| [p[0] * c + p[1] * s + 10., 0., -p[0] * s + p[1] * c + 20.])
            .collect();
        let r = footprint_rectangle(&pts).unwrap();
        let side =
            |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let (a, b) = (side(r[0], r[1]), side(r[1], r[2]));
        assert!(((a * b) - 3.0).abs() < 0.02, "area {}", a * b);
        for p in &pts {
            // Every input point lies inside the rectangle (cross products).
            let inside = (0..4).all(|i| {
                let (p0, p1) = (r[i], r[(i + 1) % 4]);
                let cr = (p1[0] - p0[0]) * (p[2] - p0[2]) - (p1[2] - p0[2]) * (p[0] - p0[0]);
                let sign = (r[1][0] - r[0][0]) * (r[2][2] - r[0][2])
                    - (r[1][2] - r[0][2]) * (r[2][0] - r[0][0]);
                cr * sign >= -0.01
            });
            assert!(inside);
        }
    }

    #[test]
    fn near_point_grows_bounds_in_xz() {
        let o = TrackObject {
            article_index: 0,
            name: String::new(),
            tag: 0,
            id: 0,
            drive_through: false,
            bounds_min: [0., 0., 0.],
            bounds_max: [1., 5., 1.],
            shape: ObjectShape::Cylinder,
        };
        assert!(o.near_point([5.5, 100., 0.5], 5.0));
        assert!(!o.near_point([6.5, 0., 0.5], 5.0));
        assert_eq!(o.centre(), [0.5, 2.5, 0.5]);
    }
}

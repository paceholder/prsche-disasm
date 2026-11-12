//! First-contact selection from Porsche.exe 0x495020..0x4954ff.
//! Original SHA256 ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39.
//! Inputs retain source field names. Extent producers, repeated responses and
//! later impact/effect callbacks must be restored before gameplay binding.
use super::{
    original_scene_objects::SceneObject,
    original_support::SupportPolygon,
    original_support_owner::{OriginalSupportOwner, SupportPlaneHit},
    original_support_tree::SupportTree,
};
use nfs_formats::Result;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyContactInput {
    pub position_330: [f32; 3],
    pub velocity_33c: [f32; 3],
    pub basis_364: [f32; 3],
    pub basis_370: [f32; 3],
    pub basis_37c: [f32; 3],
    pub extents_3a8: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectedBodyContact {
    pub candidate: usize,
    pub point: [f32; 3],
    pub segment: [[f32; 3]; 2],
    pub edge: usize,
    pub word: u16,
    pub normal: [f32; 3],
    pub displacement: [f32; 3],
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (a[i] as f64 + b[i] as f64) as f32)
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (a[i] as f64 - b[i] as f64) as f32)
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|v| (v as f64 * s as f64) as f32)
}

pub fn body_candidates(input: BodyContactInput) -> [[f32; 3]; 8] {
    let w = scale(input.basis_364, input.extents_3a8[0]);
    let h = scale(input.basis_370, input.extents_3a8[1]);
    let l = scale(input.basis_37c, input.extents_3a8[2]);
    let lower = sub(input.position_330, h);
    let upper = add(input.position_330, h);
    let half_l = scale(l, 0.5);
    [
        sub(add(lower, l), w),
        add(add(lower, l), w),
        sub(sub(lower, l), w),
        add(sub(lower, l), w),
        sub(add(upper, half_l), w),
        add(add(upper, half_l), w),
        sub(sub(upper, half_l), w),
        add(sub(upper, half_l), w),
    ]
}

fn near_plane(hit: SupportPlaneHit, point: [f32; 3], reference: f32) -> bool {
    // No f32 height store occurs before the original +/-4 comparison.
    let height = hit.point[1] as f64
        - ((point[0] as f64 - hit.point[0] as f64) * hit.normal[0] as f64
            + (point[2] as f64 - hit.point[2] as f64) * hit.normal[2] as f64)
            / hit.normal[1] as f64;
    (height - reference as f64).abs() <= 4.0
}

/// Select the first original accepted candidate; never mutates car state or
/// its persistent owner. A full 0x495020 call continues after applying each
/// response and can produce more contacts. This bounded API stops before it.
pub fn first_body_contact(
    input: BodyContactInput,
    persistent_owner: &OriginalSupportOwner,
    tree: &SupportTree,
    polygons: &[SupportPolygon],
    objects: &[SceneObject],
) -> Result<Option<SelectedBodyContact>> {
    let mut owner = persistent_owner.clone();
    let velocity_step = scale(input.velocity_33c, 0.03125);
    for (candidate, point) in body_candidates(input).into_iter().enumerate() {
        if owner.select(tree, polygons, point, true)?.is_some_and(|h| {
            polygons[h.polygon].flags_word_a & 15 != 0
                && near_plane(h, point, input.position_330[1])
        }) {
            continue;
        }
        let mut from = input.position_330;
        if !owner.select(tree, polygons, from, true)?.is_some_and(|h| {
            polygons[h.polygon].flags_word_a & 15 != 0 && near_plane(h, from, input.position_330[1])
        }) {
            from = sub(input.position_330, velocity_step);
            owner.select(tree, polygons, from, true)?;
        }
        let segment = [from, point];
        let hit = tree.query_scene_edges(objects, segment, owner.polygon.is_some())?;
        // Original 0x4953f7 retries the same query after a miss. It cannot
        // change immutable tree or owner state, so its result is identical.
        let Some(hit) = hit else { continue };
        let [a, b] = hit.endpoints;
        let delta = sub(b, a);
        let mut normal = [-delta[2], 0., delta[0]];
        let length =
            (normal[0] as f64 * normal[0] as f64 + normal[2] as f64 * normal[2] as f64).sqrt();
        if length != 0. {
            normal = normal.map(|v| (v as f64 / length) as f32);
        }
        // 0x489f10 projects onto the infinite XZ line. dx remains in x87;
        // dz is stored as f32 (`fst`, 0x489f38) for the numerator and the
        // projection, but the denominator squares the still-live x87 dz
        // (0x489f5a `fld st(1); fmul st(2)`), and the projection parameter is
        // an explicit f32 store.
        let dx = b[0] as f64 - a[0] as f64;
        let dz_x87 = b[2] as f64 - a[2] as f64;
        let dz = dz_x87 as f32;
        let t = (((point[0] as f64 - a[0] as f64) * dx
            + (point[2] as f64 - a[2] as f64) * dz as f64)
            / (dx * dx + dz_x87 * dz_x87)) as f32;
        let projection = [
            (a[0] as f64 + dx * t as f64) as f32,
            0.,
            (a[2] as f64 + dz as f64 * t as f64) as f32,
        ];
        let mut displacement = scale(sub(projection, point), f32::from_bits(0x3f8147ae));
        displacement[1] = 0.;
        return Ok(Some(SelectedBodyContact {
            candidate,
            point,
            segment,
            edge: hit.object,
            word: hit.word,
            normal,
            displacement,
        }));
    }
    Ok(None)
}

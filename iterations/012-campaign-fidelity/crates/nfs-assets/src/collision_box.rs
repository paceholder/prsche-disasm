//! Car collision box from the CRP model, as the original sets it.
//!
//! `0x4425b0` builds one model per CRP level (`i = 1..count`), and
//! `CarModel_SetCollisionExtents` `0x445090` takes half the bounding box of the
//! 5th (list entry `[4]`, level 5) as the box half extents, without padding.
//! Raw CRP units are metres. See docs/re/collision.md in the workspace root.
//!
//! Hypothesis: the original model's bounds cover every level-5 article; style
//! selection (TPG) and damage variants are not applied here.

use std::collections::BTreeMap;

use nfs_formats::{bytes, f32le, u16le, u32le, Crp, Entry};

/// Axis-aligned bounds `[min, max]` in raw CRP model coordinates.
pub type Bounds = [[f32; 3]; 2];

/// CRP level the original uses for the collision box [VA 0x445090].
pub const COLLISION_LEVEL: u16 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarCollisionBox {
    /// Box centre in raw CRP model coordinates.
    pub center: [f32; 3],
    /// Half extents (width, height, length) in metres.
    pub half_extents: [f32; 3],
}

impl CarCollisionBox {
    pub fn from_bounds(b: Bounds) -> Self {
        Self {
            center: std::array::from_fn(|k| 0.5 * (b[0][k] + b[1][k])),
            half_extents: std::array::from_fn(|k| 0.5 * (b[1][k] - b[0][k])),
        }
    }
}

fn grow(b: &mut Bounds, p: [f32; 3]) {
    for k in 0..3 {
        b[0][k] = b[0][k].min(p[k]);
        b[1][k] = b[1][k].max(p[k]);
    }
}

/// Vertices of `vt` referenced by one `pr` part (channel 0 only).
fn part_vertices(pr: &Entry, vt: &Entry, out: &mut Vec<[f32; 3]>) -> Result<(), String> {
    let d = &pr.data;
    let ni = u32le(d, 40)? as usize;
    let nd = u32le(d, 44)? as usize;
    if ni > 64 || nd > 16 {
        return Err("unreasonable part rows".into());
    }
    let ib = 48 + ni * 16;
    let data = ib + nd * 8;
    let raw = bytes(d, data, nd * pr.count)?;
    for i in 0..ni {
        let o = 48 + i * 16;
        if u16le(d, o + 10)? != 0 {
            continue;
        }
        let adjust = u32le(d, o + 4)? as usize / 16;
        let row = u16le(d, o + 14)?;
        let off = if row == u16::MAX {
            None
        } else {
            Some(u32le(d, ib + row as usize * 8 + 4)? as usize)
        };
        for n in 0..pr.count {
            let ix = off.map_or(n, |off| raw[off + n] as usize) + adjust;
            let mut p = [0.; 3];
            for (k, v) in p.iter_mut().enumerate() {
                *v = f32le(&vt.data, ix * 16 + k * 4)?;
            }
            out.push(p);
        }
    }
    Ok(())
}

/// Raw bounds per CRP level of the vertices referenced by each level's `pr`
/// parts, with the part's `tr` translation applied. Only plain level indices
/// (undamaged, type 0) count.
pub fn level_bounds(crp: &Crp) -> Result<BTreeMap<u16, Bounds>, String> {
    let mut levels: BTreeMap<u16, Bounds> = BTreeMap::new();
    for article in &crp.articles {
        for vt in article.children.iter().filter(|e| e.tag == "vt") {
            if vt.index > 0xf {
                continue;
            }
            let tr = match article.find("tr", vt.index) {
                Some(t) => [
                    f32le(&t.data, 48)?,
                    f32le(&t.data, 52)?,
                    f32le(&t.data, 56)?,
                ],
                None => [0.; 3],
            };
            let mut points = Vec::new();
            for pr in article
                .children
                .iter()
                .filter(|e| e.tag == "pr" && e.index >> 12 == vt.index)
            {
                part_vertices(pr, vt, &mut points)?;
            }
            let bounds = levels
                .entry(vt.index)
                .or_insert([[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]]);
            for p in points {
                grow(bounds, [p[0] + tr[0], p[1] + tr[1], p[2] + tr[2]]);
            }
        }
    }
    levels.retain(|_, b| b[0][0] <= b[1][0]);
    Ok(levels)
}

/// The original collision box of a car model (level 5).
pub fn car_collision_box(crp: &Crp) -> Result<CarCollisionBox, String> {
    let levels = level_bounds(crp)?;
    let bounds = levels
        .get(&COLLISION_LEVEL)
        .ok_or_else(|| format!("car CRP has no level {COLLISION_LEVEL}"))?;
    Ok(CarCollisionBox::from_bounds(*bounds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn installed(car: &str) -> Option<Crp> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/CarModel")
            .join(format!("{car}.crp"));
        let data = std::fs::read(path).ok()?;
        Some(nfs_formats::parse_crp(&data).expect("installed car CRP parses"))
    }

    #[test]
    fn bounds_split_into_centre_and_half_extents() {
        let b = CarCollisionBox::from_bounds([[-1.0, 0.0, -2.0], [1.0, 1.2, 2.5]]);
        assert_eq!(b.center, [0.0, 0.6, 0.25]);
        assert_eq!(b.half_extents, [1.0, 0.6, 2.25]);
    }

    #[test]
    fn level_five_boxes_of_installed_cars() {
        // Measured values recorded in docs/re/collision.md.
        for (car, expected) in [
            ("356a", [0.843, 0.604, 1.880]),
            ("993", [0.937, 0.631, 2.104]),
            ("boxster", [0.959, 0.605, 2.154]),
        ] {
            let Some(crp) = installed(car) else {
                eprintln!("{car}.crp not installed; skipped");
                continue;
            };
            let b = car_collision_box(&crp).unwrap();
            for (k, (&got, &want)) in b.half_extents.iter().zip(&expected).enumerate() {
                assert!((got - want).abs() < 0.001, "{car} {k}: {b:?}");
            }
        }
    }
}

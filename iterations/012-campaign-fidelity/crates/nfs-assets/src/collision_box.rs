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

/// The car's body points: the `BndP` entries of the car CRP as
/// `Car_ReadTpgCameraKeys` `0x441a10` collects them into the render object
/// (`R3DCar+0xf8` count, `+0xfc` points), in raw CRP model coordinates.
/// Each entry is `(part, value, count × (x, y, z, w))`; the original keeps an
/// entry when the car's part selection `car+0x56c[part]` equals `value`.
/// The selection array is not ported: entries of part 0 and entries with
/// value 0 are kept (approximation A32). The original subtracts the half
/// height `+0x3ac` from y and, in light mode (`res+0x518`), scales by 0.4;
/// callers convert to the car frame.
pub fn body_points(crp: &Crp) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for e in crp.misc.iter().filter(|e| e.tag == "BndP") {
        let d = &e.data;
        let (Ok(part), Ok(value)) = (u32le(d, 0), u32le(d, 4)) else {
            continue;
        };
        if part != 0 && value != 0 {
            continue;
        }
        for k in 0..e.count {
            let p: Result<Vec<f32>, String> =
                (0..3).map(|c| f32le(d, 8 + 16 * k + 4 * c)).collect();
            match p {
                Ok(p) => out.push([p[0], p[1], p[2]]),
                Err(_) => break,
            }
        }
    }
    out
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
    fn body_points_of_the_installed_356a() {
        let Some(crp) = installed("356a") else {
            eprintln!("356a.crp not installed; skipped");
            return;
        };
        let p = body_points(&crp);
        // Rear hull 8, front hull 8, roof 4, wheel contacts 4.
        assert_eq!(p.len(), 24);
        let low = p.iter().filter(|q| q[1].abs() < 0.01).count();
        assert_eq!(low, 4, "four wheel contact points on the ground");
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

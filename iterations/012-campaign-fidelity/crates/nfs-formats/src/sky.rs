//! Sun of the sky config `Sky/<track>.bin` (`docs/re/lighting.md`).
//!
//! The file is loaded raw as the sky config object (`SkyConfig_Load` 0x459660);
//! version 9 files need no upgrade (`SkyConfig_Upgrade` 0x459700 fills defaults
//! for older versions). `Sky_SunPosition` 0x461090 reads six fields.

use crate::{f32le, u32le, Result};

/// Size of a version-9 sky config.
pub const SKY_CONFIG_SIZE: usize = 0x71dc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkySun {
    /// `+0x3c`, turns.
    pub rotation: f32,
    /// `+0x44`, curvature of the sun height.
    pub curvature: f32,
    /// `+0x48`.
    pub height: f32,
    /// `+0xcdc`, `+0xce0`.
    pub offset: [f32; 2],
    /// `+0xce4`.
    pub scale: f32,
}

/// Read the sun fields. Versions below 5 have no offset (`+0xcdc/+0xce0 = 0`)
/// and versions below 6 use scale 0.1, as the upgrade does.
pub fn parse_sky_sun(data: &[u8]) -> Result<SkySun> {
    let version = u32le(data, 0)?;
    if version > 9 {
        return Err(format!("sky config version {version} is newer than 9"));
    }
    let (rotation, curvature, height) = if version < 1 {
        (0.0, 0.0004, 75.0)
    } else {
        (f32le(data, 0x3c)?, f32le(data, 0x44)?, f32le(data, 0x48)?)
    };
    let offset = if version < 5 {
        [0.0; 2]
    } else {
        [f32le(data, 0xcdc)?, f32le(data, 0xce0)?]
    };
    let scale = if version < 6 { 0.1 } else { f32le(data, 0xce4)? };
    Ok(SkySun {
        rotation,
        curvature,
        height,
        offset,
        scale,
    })
}

impl SkySun {
    /// `Sky_SunPosition` 0x461090 in original world coordinates:
    /// `((sin θ·b + cos θ·a)·s, (h − (a²+b²)·k)·s, (cos θ·b − sin θ·a)·s)`.
    pub fn position(&self) -> [f32; 3] {
        let turn = f64::from(self.rotation) * std::f64::consts::TAU;
        let (sin, cos) = (turn.sin(), turn.cos());
        let [a, b] = self.offset.map(f64::from);
        let s = f64::from(self.scale);
        [
            ((sin * b + cos * a) * s) as f32,
            ((f64::from(self.height) - (a * a + b * b) * f64::from(self.curvature)) * s) as f32,
            ((cos * b - sin * a) * s) as f32,
        ]
    }
}

/// Radius of the horizon ring, `SkyHorizon_Build` 0x459d30 (`400.0`).
pub const HORIZON_RADIUS: f32 = 400.0;
/// Segments of the horizon ring (the build loop runs 16 times, 1/16 turn each).
pub const HORIZON_SEGMENTS: usize = 16;

/// Horizon panorama (`horz`) placement of the sky config (`SkyHorizon_Build` 0x459d30,
/// draw `SkyHorizon_Draw` 0x45a1a0; `docs/re/sky.md`). The panorama is a ring of 16 flat quads at radius
/// 400 around the camera (translation zeroed, rotation kept), between the heights
/// `base` and `base + height`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyHorizon {
    /// `+0x04`, turns: azimuth of ring vertex 0 (`x = sin, z = cos`, original axes).
    pub rotation: f32,
    /// `+0x08`: first column of the `hzcl` colour strip.
    pub start: u32,
    /// `+0x0c`, `+0x10`: tile grid of `horz`.
    pub columns: u32,
    pub rows: u32,
    /// `+0x14`: height of the lower edge at radius 400.
    pub base: f32,
    /// `+0x18`: height of the band; the upper edge is `base + height`.
    pub height: f32,
    /// `+0x1c`: per segment (tile index, mirrored).
    pub tiles: [(u8, bool); HORIZON_SEGMENTS],
}

/// Read the horizon fields; version 0 files have the `SkyConfig_Upgrade` defaults.
pub fn parse_sky_horizon(data: &[u8]) -> Result<SkyHorizon> {
    let version = u32le(data, 0)?;
    if version > 9 {
        return Err(format!("sky config version {version} is newer than 9"));
    }
    if version < 1 {
        return Ok(SkyHorizon {
            rotation: 0.0,
            start: 0,
            columns: 2,
            rows: 2,
            base: -100.0,
            height: 250.0,
            tiles: [(0, false); HORIZON_SEGMENTS],
        });
    }
    let mut tiles = [(0u8, false); HORIZON_SEGMENTS];
    for (i, t) in tiles.iter_mut().enumerate() {
        let o = 0x1c + 2 * i;
        *t = (*data.get(o).ok_or("sky tiles truncated")?, *data.get(o + 1).ok_or("sky tiles truncated")? != 0);
    }
    let columns = u32le(data, 0x0c)?;
    let rows = u32le(data, 0x10)?;
    if columns == 0 || rows == 0 {
        return Err("sky horizon tile grid is empty".into());
    }
    Ok(SkyHorizon {
        rotation: f32le(data, 4)?,
        start: u32le(data, 8)?,
        columns,
        rows,
        base: f32le(data, 0x14)?,
        height: f32le(data, 0x18)?,
        tiles,
    })
}

/// Where a view direction hits the horizon ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorizonHit {
    pub segment: usize,
    /// Position along the segment chord, 0 at ring vertex `segment`, 1 at the next.
    pub along: f32,
    /// 0 at the upper edge, 1 at the lower edge (unclamped).
    pub down: f32,
    /// Texture coordinates in `horz` (tile row inset 0.01 / 0.98 as in the build).
    pub uv: [f32; 2],
}

impl SkyHorizon {
    /// Height of the upper edge.
    pub fn top(&self) -> f32 {
        self.base + self.height
    }

    /// Elevation angles (radians) of the upper and lower edges at the ring vertices.
    pub fn edge_elevations(&self) -> (f32, f32) {
        ((self.top() / HORIZON_RADIUS).atan(), (self.base / HORIZON_RADIUS).atan())
    }

    /// Intersect the ray from the camera in direction `dir` (original axes: `x = sin`,
    /// `z = cos` of the azimuth, `y` up) with the ring quads.
    pub fn hit(&self, dir: [f32; 3]) -> Option<HorizonHit> {
        let horizontal = dir[0].hypot(dir[2]);
        if horizontal < 1e-6 {
            return None;
        }
        let step = std::f32::consts::TAU / HORIZON_SEGMENTS as f32;
        let azimuth = dir[0].atan2(dir[2]) / std::f32::consts::TAU;
        let t = (azimuth - self.rotation).rem_euclid(1.0) * HORIZON_SEGMENTS as f32;
        let segment = (t.floor() as usize).min(HORIZON_SEGMENTS - 1);
        // Angle from the segment's middle; the chord is a straight line at the apothem.
        let off = (t - segment as f32 - 0.5) * step;
        let half = step / 2.0;
        let along = 0.5 + off.tan() / (2.0 * half.tan());
        let distance = HORIZON_RADIUS * half.cos() / off.cos();
        let y = dir[1] / horizontal * distance;
        let down = (self.top() - y) / self.height;
        let (tile, mirrored) = self.tiles[segment];
        let (tile, columns) = (u32::from(tile), self.columns);
        let across = if mirrored { 1.0 - along } else { along };
        let u = ((tile % columns) as f32 + across) / columns as f32;
        let v = ((tile / columns) as f32 + 0.01 + down.clamp(0.0, 1.0) * 0.97) / self.rows as f32;
        Some(HorizonHit { segment, along, down, uv: [u, v] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(version: u32, fields: &[(usize, f32)]) -> Vec<u8> {
        let mut d = vec![0u8; SKY_CONFIG_SIZE];
        d[..4].copy_from_slice(&version.to_le_bytes());
        for &(o, v) in fields {
            d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        d
    }

    #[test]
    fn overhead_sun_without_offset() {
        let sun = parse_sky_sun(&config(9, &[(0x48, 125.0), (0x44, 0.0002), (0xce4, 0.1)])).unwrap();
        let p = sun.position();
        assert_eq!(p[0], 0.0);
        assert!((p[1] - 12.5).abs() < 1e-5);
        assert_eq!(p[2], 0.0);
    }

    #[test]
    fn rotation_turns_the_offset() {
        // θ = 0.25 turn: x = b·s, z = −a·s.
        let sun = parse_sky_sun(&config(
            9,
            &[(0x3c, 0.25), (0x48, 10.0), (0xcdc, 2.0), (0xce0, 3.0), (0xce4, 1.0)],
        ))
        .unwrap();
        let p = sun.position();
        assert!((p[0] - 3.0).abs() < 1e-5 && (p[2] + 2.0).abs() < 1e-5, "{p:?}");
        assert!((p[1] - 10.0).abs() < 1e-5);
    }

    #[test]
    fn old_versions_use_the_upgrade_defaults() {
        let sun = parse_sky_sun(&config(4, &[(0x48, 50.0), (0xcdc, 9.0), (0xce4, 7.0)])).unwrap();
        assert_eq!(sun.offset, [0.0; 2]);
        assert_eq!(sun.scale, 0.1);
        assert!(parse_sky_sun(&config(10, &[])).is_err());
    }

    #[test]
    fn local_alps_sun_is_about_44_degrees_high() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track/Sky/alps.bin");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let p = parse_sky_sun(&data).unwrap().position();
        let elevation = p[1].atan2(p[0].hypot(p[2])).to_degrees();
        assert!((elevation - 44.0).abs() < 1.0, "elevation {elevation}");
    }

    fn horizon_config() -> Vec<u8> {
        let mut d = config(9, &[(4, 0.25), (0x14, -100.0), (0x18, 300.0)]);
        d[8..12].copy_from_slice(&3u32.to_le_bytes());
        d[0x0c..0x10].copy_from_slice(&4u32.to_le_bytes());
        d[0x10..0x14].copy_from_slice(&2u32.to_le_bytes());
        for i in 0..16 {
            d[0x1c + 2 * i] = (i % 8) as u8;
        }
        d[0x1c + 2 * 5 + 1] = 1;
        d
    }

    #[test]
    fn horizon_fields_and_edge_angles() {
        let h = parse_sky_horizon(&horizon_config()).unwrap();
        assert_eq!((h.columns, h.rows, h.start), (4, 2, 3));
        assert_eq!(h.top(), 200.0);
        let (top, bottom) = h.edge_elevations();
        assert!((top.to_degrees() - 26.565).abs() < 1e-2);
        assert!((bottom.to_degrees() + 14.036).abs() < 1e-2);
        let default = parse_sky_horizon(&config(0, &[])).unwrap();
        assert_eq!((default.base, default.height, default.columns), (-100.0, 250.0, 2));
        assert!(parse_sky_horizon(&config(10, &[])).is_err());
    }

    #[test]
    fn ring_hit_matches_vertices() {
        let h = parse_sky_horizon(&horizon_config()).unwrap();
        // Ring vertex 0 is at azimuth 0.25 turn: x = 400, z = 0 (top edge at y = 200).
        let a = HORIZON_RADIUS;
        let hit = h.hit([a, 200.0, -1e-3]).unwrap();
        assert_eq!(hit.segment, 0);
        assert!(hit.along < 1e-3 && hit.down.abs() < 1e-3, "{hit:?}");
        // Straight ahead of vertex 1 (azimuth 0.25 + 1/16) is the lower edge corner.
        let az = (0.25f32 + 1.0 / 16.0) * std::f32::consts::TAU;
        let hit = h.hit([a * az.sin(), -100.0, a * az.cos()]).unwrap();
        assert_eq!(hit.segment, 1);
        assert!(hit.along < 1e-2 && (hit.down - 1.0).abs() < 1e-3, "{hit:?}");
        // The horizon (y = 0) is 2/3 of the way down this band.
        let hit = h.hit([1.0, 0.0, 0.0]).unwrap();
        assert!((hit.down - 200.0 / 300.0).abs() < 1e-4);
        // Segment 5 is mirrored: u runs backwards.
        let az = (0.25f32 + 5.0 / 16.0 + 0.5 / 16.0) * std::f32::consts::TAU;
        let hit = h.hit([az.sin(), 0.0, az.cos()]).unwrap();
        assert_eq!(hit.segment, 5);
        assert!((hit.along - 0.5).abs() < 1e-3);
        assert!((hit.uv[0] - (1.0 + 0.5) / 4.0).abs() < 1e-3, "{:?}", hit.uv);
    }

    #[test]
    fn local_autobahn_horizon_values() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../../game/app/GameData/Track/Sky/Autobahn.bin");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let h = parse_sky_horizon(&data).unwrap();
        assert!((h.rotation - 0.2069).abs() < 1e-3 && h.columns == 4 && h.rows == 2);
        let (top, bottom) = h.edge_elevations();
        assert!((top.to_degrees() - 7.8).abs() < 0.1 && (bottom.to_degrees() + 24.55).abs() < 0.1);
        assert_eq!(h.tiles[9], (1, false));
    }
}

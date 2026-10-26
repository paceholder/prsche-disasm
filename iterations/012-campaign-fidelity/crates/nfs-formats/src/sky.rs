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

/// Dome of the sky-map layers (`FUN_00460f40`, `docs/re/sky.md` "Sky-map dome"): a 16x16 vertex
/// grid, a paraboloid `y = (height - (a^2 + b^2) * curvature) * scale` over `a, b` in
/// `-size/2..size/2`, turned by `angle` turns about the vertical axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyDome {
    /// `+0x3c`, turns.
    pub angle: f32,
    /// `+0x40`: edge length of the square grid.
    pub size: f32,
    /// `+0x44`: paraboloid curvature.
    pub curvature: f32,
    /// `+0x48`: apex height before scaling.
    pub height: f32,
    /// `+0xce4`: uniform scale of the dome.
    pub scale: f32,
}

/// Vertices per side minus one (`FUN_0045b5a0` calls the grid builder with 15).
pub const SKY_DOME_CELLS: f32 = 15.0;

impl SkyDome {
    /// Grid position `(column, row)` (0..15 over the dome) hit by a view direction in original
    /// axes (x = sin, z = cos, y up), or `None` outside the square grid; the shader
    /// (`sky.wgsl`) does the same per pixel. Intersection of the ray `t * d` with
    /// `y = scale * height - curvature * (x^2 + z^2) / scale` (`FUN_00460f40`).
    pub fn grid_position(&self, d: [f32; 3]) -> Option<(f32, f32)> {
        let h2 = d[0] * d[0] + d[2] * d[2];
        let denom = (d[1] + (d[1] * d[1] + 4.0 * self.curvature * h2 * self.height).sqrt()).max(1e-6);
        let t = 2.0 * self.scale * self.height / denom;
        let (x, z) = (t * d[0], t * d[2]);
        let turn = self.angle * std::f32::consts::TAU;
        let (sn, cs) = turn.sin_cos();
        let a = (x * cs - z * sn) / self.scale;
        let b = (x * sn + z * cs) / self.scale;
        let half = self.size * 0.5;
        if a.abs() > half || b.abs() > half {
            return None;
        }
        let cell = self.size / SKY_DOME_CELLS;
        Some(((half - a) / cell, (b + half) / cell))
    }
}

pub fn parse_sky_dome(data: &[u8]) -> Result<SkyDome> {
    let dome = SkyDome {
        angle: f32le(data, 0x3c)?,
        size: f32le(data, 0x40)?,
        curvature: f32le(data, 0x44)?,
        height: f32le(data, 0x48)?,
        scale: f32le(data, 0xce4)?,
    };
    if !(dome.size > 0.0 && dome.scale > 0.0 && dome.height > 0.0) {
        return Err("sky dome has no size".into());
    }
    Ok(dome)
}

/// One `BEGIN..END` block of `Sky/<track>.txt`: the layers of one computer capability.
/// Region A only (region B is the second weather set).
#[derive(Debug, Clone, PartialEq)]
pub struct SkyMapBlock {
    /// `HIGH`, `MEDIUM`, ...
    pub capability: String,
    /// `MULTINORMAL`, `BASIC`, ...
    pub kind: String,
    /// Three sky map names; `none` for an unused layer.
    pub maps: [String; 3],
    /// Per layer: `MULTI*` maps advance `1 / scale` per grid cell (`FUN_0045f1b0`); `BASIC` has
    /// one pair, the repeats over the dome.
    pub x_scale: [f32; 3],
    pub y_scale: [f32; 3],
    pub move_ratio: f32,
    pub wind_speed: [f32; 3],
    /// Degrees, north 0, east 90.
    pub wind_direction: [f32; 3],
    /// Weather region B of the same block (`MULTI*` only): the maps and scales the layer
    /// blends to by the sky-map keys of `Sky/<track>.bin` (`FUN_0045b5a0` reads both regions,
    /// `FUN_0045e160` mixes the texels, `FUN_0045f0a0` the scales; `docs/re/weather.md`).
    pub region_b: Option<Box<SkyMapBlock>>,
}

/// Parse the commented script of `Sky/<track>.txt` (`#` starts a comment line).
pub fn parse_sky_txt(text: &str) -> Vec<SkyMapBlock> {
    let mut blocks = Vec::new();
    let mut tokens: Option<Vec<&str>> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        match (t.eq_ignore_ascii_case("BEGIN"), t.eq_ignore_ascii_case("END")) {
            (true, _) => tokens = Some(Vec::new()),
            (_, true) => {
                if let Some(tok) = tokens.take() {
                    if let Some(b) = sky_block(&tok) {
                        blocks.push(b);
                    }
                }
            }
            _ => {
                if let Some(tok) = tokens.as_mut() {
                    tok.push(t.split_whitespace().next().unwrap_or(""));
                }
            }
        }
    }
    blocks
}

fn sky_block(t: &[&str]) -> Option<SkyMapBlock> {
    // capability, type, blocks-the-sun, then region A.
    if t.len() < 3 + 3 {
        return None;
    }
    let maps = [t[3].to_string(), t[4].to_string(), t[5].to_string()];
    let (capability, kind) = (t[0].to_ascii_uppercase(), t[1].to_ascii_uppercase());
    let n = |i: usize| t.get(3 + i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    if kind == "BASIC" {
        // `SkyBasicLayer` (0x45bf80): 3 maps, then x scale, y scale, 5 more numbers (cloud
        // sprite parameters, not read); one scale pair for the single sky map.
        return Some(SkyMapBlock {
            capability,
            kind,
            maps,
            x_scale: [n(3), 0.0, 0.0],
            y_scale: [n(4), 0.0, 0.0],
            move_ratio: 0.0,
            wind_speed: [0.0; 3],
            wind_direction: [0.0; 3],
            region_b: None,
        });
    }
    // MULTI*: 3 maps, 3 x, 3 y, move, 3 speeds, 3 directions, warp speed, warp amount,
    // for region A (tokens 3..21) and region B (21..39).
    let mut a = multi_region(t, 3, &capability, &kind)?;
    a.region_b = multi_region(t, 3 + 18, &capability, &kind).map(Box::new);
    Some(a)
}

fn multi_region(t: &[&str], base: usize, capability: &str, kind: &str) -> Option<SkyMapBlock> {
    if t.len() < base + 18 {
        return None;
    }
    let n = |i: usize| t.get(base + i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    Some(SkyMapBlock {
        capability: capability.to_string(),
        kind: kind.to_string(),
        maps: [t[base].to_string(), t[base + 1].to_string(), t[base + 2].to_string()],
        x_scale: [n(3), n(4), n(5)],
        y_scale: [n(6), n(7), n(8)],
        move_ratio: n(9),
        wind_speed: [n(10), n(11), n(12)],
        wind_direction: [n(13), n(14), n(15)],
        region_b: None,
    })
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

    #[test]
    fn sky_txt_blocks_are_parsed() {
        let text = "# c\nBEGIN\n\tHIGH\n\tMULTINORMAL\nNO\n# maps\n\tst1a\n\tsc1a\n\tnone\n1\n2\n0\n1\n3\n0\n400\n0\n2\n0\n90\n40\n0\n0\n0\n\tst1b\n\tsc1b\n\tnone\n1\n2\n0\n1\n2\n0\n400\n0\n2\n0\n90\n40\n0\n0\n0\n\tflat\n\tcc1a\n\tnone\nEND\nBEGIN\nMEDIUM\nBASIC\nNO\nEND\n";
        let blocks = parse_sky_txt(text);
        assert_eq!(blocks.len(), 1, "the truncated block is dropped");
        let b = &blocks[0];
        assert_eq!((b.capability.as_str(), b.kind.as_str()), ("HIGH", "MULTINORMAL"));
        assert_eq!(b.maps, ["st1a", "sc1a", "none"]);
        assert_eq!((b.x_scale, b.y_scale), ([1.0, 2.0, 0.0], [1.0, 3.0, 0.0]));
        assert_eq!((b.move_ratio, b.wind_speed, b.wind_direction), (400.0, [0.0, 2.0, 0.0], [90.0, 40.0, 0.0]));
        // Weather region B follows region A.
        let rb = b.region_b.as_deref().expect("region B");
        assert_eq!(rb.maps, ["st1b", "sc1b", "none"]);
        assert_eq!((rb.x_scale, rb.y_scale), ([1.0, 2.0, 0.0], [1.0, 2.0, 0.0]));
    }

    #[test]
    fn local_sky_dome_and_txt() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../../game/app/GameData/Track/Sky");
        let (Ok(bin), Ok(txt)) = (std::fs::read(dir.join("Coastal.bin")), std::fs::read_to_string(dir.join("Coastal.txt"))) else {
            return;
        };
        let d = parse_sky_dome(&bin).unwrap();
        assert!((d.size - 1500.0).abs() < 1e-3 && (d.height - 44.3878).abs() < 1e-3 && (d.scale - 0.2357).abs() < 1e-3);
        let blocks = parse_sky_txt(&txt);
        let high = blocks.iter().find(|b| b.capability == "HIGH").unwrap();
        assert_eq!(high.maps[0], "sm1b");
        assert_eq!((high.x_scale[0], high.y_scale[0]), (2.0, 6.0));
    }

    #[test]
    fn dome_apex_is_the_grid_centre_and_the_edge_is_the_border() {
        let d = SkyDome { angle: 0.0, size: 1500.0, curvature: 0.0002, height: 44.0, scale: 0.25 };
        let (col, row) = d.grid_position([0.0, 1.0, 0.0]).unwrap();
        assert!((col - 7.5).abs() < 1e-3 && (row - 7.5).abs() < 1e-3);
        // Slightly above the horizon the ray meets the dome far out, below it nothing is left
        // of the grid on the sea side for a steeply descending ray.
        let (col, _) = d.grid_position([1.0, 0.05, 0.0]).unwrap_or((-1.0, 0.0));
        assert!(col < 7.5, "x > 0 moves towards column 0, got {col}");
        assert!(d.grid_position([1.0, -0.5, 0.0]).is_none());
        // The rotation turns the grid: a quarter turn swaps the axes.
        let r = SkyDome { angle: 0.25, ..d };
        let (c, rw) = r.grid_position([0.3, 1.0, 0.0]).unwrap();
        let (c0, rw0) = d.grid_position([0.3, 1.0, 0.0]).unwrap();
        assert!((c - 7.5).abs() < (c0 - 7.5).abs() && (rw - 7.5).abs() > (rw0 - 7.5).abs());
    }
}

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

/// Vertex colour of the sky dome at grid position `(column, row)` (0..15) for a `MULTI*` sky
/// map: the 16x16 ARGB map is not a texture but one colour per dome vertex. `FUN_0045b5a0`
/// hands the texel buffer of the layer's map object (`FUN_0045dd90` copy, region A/B mix
/// `FUN_0045e160`, `+0x14`) to `FUN_00468630` → `FUN_00468440` as mesh channel 6 (4 bytes per
/// vertex) of the 16x16 grid whose vertex `row * 16 + column` sits at `FUN_00460f40`'s grid
/// point; texel `(x, y)` colours vertex `(column x, row y)`. Gouraud shading across each quad
/// `[i, i+1, i+17, i+16]` (`FUN_0045e8e0`) split as `(0, 1, 2)`, `(0, 2, 3)`. Recorded
/// (`replay-sky-layer.py`): the colour channel bytes per vertex, the grid quad order, and the
/// driver's quad split (nfs5 0x57e000 -> `THRASH_drawquad` -> dx7z.dll TRIANGLELIST
/// `0,1,2, 0,2,3`, Gouraud shade mode); the path from the mesh draw `FUN_00468410` to 0x57e000
/// is not run. The interpolation itself is the rasteriser's. Returns RGBA in 0..1 as stored
/// (gamma-space bytes / 255).
pub fn sky_vertex_colour(rgba: &[u8], column: f32, row: f32) -> [f32; 4] {
    const N: usize = 16;
    if rgba.len() < N * N * 4 {
        return [1.0; 4];
    }
    let max = (N - 1) as f32;
    let (c, r) = (column.clamp(0.0, max), row.clamp(0.0, max));
    let (c0, r0) = ((c.floor() as usize).min(N - 2), (r.floor() as usize).min(N - 2));
    let (fx, fy) = (c - c0 as f32, r - r0 as f32);
    let texel = |x: usize, y: usize| -> [f32; 4] {
        let i = (y * N + x) * 4;
        [0, 1, 2, 3].map(|k| f32::from(rgba[i + k]) / 255.0)
    };
    let (v0, v1, v2, v3) = (texel(c0, r0), texel(c0 + 1, r0), texel(c0 + 1, r0 + 1), texel(c0, r0 + 1));
    [0, 1, 2, 3].map(|k| {
        if fx >= fy {
            v0[k] + fx * (v1[k] - v0[k]) + fy * (v2[k] - v1[k])
        } else {
            v0[k] + fx * (v2[k] - v3[k]) + fy * (v3[k] - v0[k])
        }
    })
}

/// f64 multiplier of `SinTurns` 0x5327e0 / `CosTurns` 0x5327f0 (`[0x5bb2c0]`, 2π rounded to
/// f32): `fsin` / `fcos` of the f32 argument times it.
const TWO_PI_TURNS: u64 = 0x4019_21fb_2000_0000;

fn sin_cos_turns(t: f32) -> (f64, f64) {
    let a = f64::from(t) * f64::from_bits(TWO_PI_TURNS);
    (a.sin(), a.cos())
}

fn x(v: f32) -> f64 {
    f64::from(v)
}

impl SkyDome {
    /// The 16 x 16 dome vertices of `FUN_00460f40` (`FUN_00460ec0(15, ...)` from
    /// `SkyMapLayer_Ctor` 0x45b5a0), vertex `row * 16 + column`, original axes (x = sin,
    /// z = cos, y up). The x87 code runs at 53-bit precision: `step = size / 15` is kept as f32,
    /// the grid coordinates `a` (from `+7.5 step`, minus `step` per column) and `b` (from
    /// `-size / 2`, plus `step` per row) are f32 running sums, `b^2` and `cos(angle)` are
    /// stored as f32. Recorded bit for bit (`replay-sky-layer.py`, `dome` rows).
    pub fn vertices(&self) -> Vec<[f32; 3]> {
        let n = 15.0f64;
        let step64 = x(self.size) / n;
        let step = step64 as f32;
        let (sin, cos) = sin_cos_turns(self.angle);
        let cos = cos as f32;
        let mut b = (step64 * n * -0.5) as f32;
        let mut out = Vec::with_capacity(256);
        for _row in 0..16 {
            let mut a = (n * x(step) * 0.5) as f32;
            for _col in 0..16 {
                let b2 = (x(b) * x(b)) as f32;
                let px = ((x(cos) * x(a) + sin * x(b)) * x(self.scale)) as f32;
                let py = ((x(self.height) - (x(a) * x(a) + x(b2)) * x(self.curvature)) * x(self.scale)) as f32;
                let pz = ((x(cos) * x(b) - sin * x(a)) * x(self.scale)) as f32;
                out.push([px, py, pz]);
                a = (x(a) - x(step)) as f32;
            }
            b = (x(b) + x(step)) as f32;
        }
        out
    }
}

/// `1 / scale` of a layer (`FUN_0045f1b0`: `fld 1.0; fdiv scale`, stored as f32).
pub fn sky_inv_scale(scale: f32) -> f32 {
    (1.0 / x(scale)) as f32
}

/// Wind step of a layer per tick (`FUN_0045f1b0`): `t = direction * -1/360` (f32), then
/// `u = f32(f32(SinTurns(t) * speed) * 0.00025)`, `v = f32(CosTurns(t) * speed * 0.00025)`.
/// The layer moves (`+0x70 + i`) only when `speed > 0.25`. Recorded bit for bit.
pub fn sky_wind_step(speed: f32, direction: f32) -> ([f32; 2], bool) {
    const NEG_INV_360: u32 = 0xbb36_0b61; // 0x5b3f3c
    const STEP: u32 = 0x3983_126f; // 0x5b3f38, 0.00025
    let t = (x(direction) * x(f32::from_bits(NEG_INV_360))) as f32;
    let (sin, cos) = sin_cos_turns(t);
    let k = x(f32::from_bits(STEP));
    let u = (x((sin * x(speed)) as f32) * k) as f32;
    let v = (cos * x(speed) * k) as f32;
    ([u, v], speed > 0.25)
}

/// Region A -> B blend of a layer parameter (`FUN_0045f180` / `FUN_0045f160`:
/// `(b - a) * t + a`, stored as f32).
pub fn sky_blend(a: f32, b: f32, t: f32) -> f32 {
    ((x(b) - x(a)) * x(t) + x(a)) as f32
}

/// Drift of a moving layer (`FUN_0045ed40`, without the camera term of flag `+0x73`): per
/// axis `offset = f32(ticks * step + offset)`, then minus 1 when above 1, plus 1 when below
/// 0 (one correction each). Recorded bit for bit.
pub fn sky_drift_advance(offset: [f32; 2], step: [f32; 2], ticks: i32) -> [f32; 2] {
    [0, 1].map(|k| {
        let mut o = (f64::from(ticks) * x(step[k]) + x(offset[k])) as f32;
        if o > 1.0 {
            o = (x(o) - 1.0) as f32;
        }
        if o < 0.0 {
            o = (x(o) + 1.0) as f32;
        }
        o
    })
}

/// Texture coordinates of grid vertex `(column, row)` of a `MULTI*` layer (`FUN_0045ec40`):
/// `u = f32(column * inv_x + warp_u + f32(drift_u + camera_u))`, rows likewise; `inv` is
/// `1 / scale` (A/B blended), `warp` the per-vertex warp offset (`FUN_0045f3c0` /
/// `FUN_0045ee80`, zero without a warp amount), `drift` the wind offset
/// (`sky_drift_advance`), `camera` the camera-position offset of `FUN_0045ed40` (flag
/// `+0x73`, move ratio > 1). Recorded bit for bit. So the cloud texture repeats every
/// `scale` grid cells (`index / scale`, no spread over the 15 cells).
pub fn sky_layer_uv(column: u32, row: u32, inv: [f32; 2], warp: [f32; 2], drift: [f32; 2], camera: [f32; 2]) -> [f32; 2] {
    let index = [column, row];
    [0, 1].map(|k| {
        let offset = (x(drift[k]) + x(camera[k])) as f32;
        (f64::from(index[k]) * x(inv[k]) + x(warp[k]) + x(offset)) as f32
    })
}

/// Texels of a layer map object between its region A and B maps (`FUN_0045e160`, the sky-map
/// blend of the weather keys): `ia = round((1 - t) * 65536)`, `ib = round(t * 65536)` (f32,
/// then `fistp`, ties to even), per byte `(b * ib + a * ia) >> 16`. Same bytes in, same bytes
/// out (the original skips the mix when A and B are one image). Recorded bit for bit.
pub fn sky_map_mix(a: &[u8], b: &[u8], t: f32) -> Vec<u8> {
    let ia = ((1.0 - x(t)) * 65536.0) as f32;
    let ib = (x(t) * 65536.0) as f32;
    let (ia, ib) = (ia.round_ties_even() as u32, ib.round_ties_even() as u32);
    a.iter().zip(b).map(|(&a, &b)| ((u32::from(b) * ib + u32::from(a) * ia) >> 16) as u8).collect()
}

/// Whether a layer map object is opaque (`FUN_0045e110` -> `FUN_0045e5e0`: every alpha byte
/// 255): one image in both regions, or each region's image absent or opaque. `rgba` texels.
pub fn sky_map_opaque(a: Option<&[u8]>, b: Option<&[u8]>, same: bool) -> bool {
    let opaque = |m: Option<&[u8]>| m.is_none_or(|m| m.chunks_exact(4).all(|p| p[3] == 255));
    if same {
        return opaque(a);
    }
    opaque(a) && opaque(b)
}

/// Does the material of a `MULTI*` layer blend (`FUN_0045b5a0`; the block `{1, 2, mode, 0,
/// 0, -1}` of 0x5379f0 with field 1 set to 0 = blending off): off for `flat`; off when the map
/// is opaque and the cloud is `none` or an image without an alpha channel (`FUN_00460d40` ->
/// `0x556690` class 3, 4 or 5: formats 0x7e/0x42/3, 0x78/0x21/0x2a, 0x7f/0x43/4); on otherwise.
/// With blending the layer is drawn SRCALPHA / INVSRCALPHA for `MULTINORMAL` (recorded
/// through 0x540660 and dx7z.dll). Recorded.
pub fn sky_layer_blends(cloud: &str, cloud_format: Option<u32>, map_opaque: bool) -> bool {
    if cloud.eq_ignore_ascii_case("flat") {
        return false;
    }
    let no_alpha = cloud_format.is_some_and(|f| {
        matches!(f & 255, 3 | 0x42 | 0x7e | 0x21 | 0x2a | 0x78 | 4 | 0x43 | 0x7f)
    });
    let none = cloud.eq_ignore_ascii_case("none") || cloud_format.is_none();
    !((none || no_alpha) && map_opaque)
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
    /// `MULTI*`: the three cloud textures after region B (`flat`, `cc1a`, `none`; region A
    /// block only). Layer `i` draws map `i` as vertex colours times cloud texture `i` at
    /// `UV = grid index / scale` (`FUN_0045b5a0`: `flat` is the pack's blank texture
    /// `*[0x627e4c]`, an image name becomes a `SKY ` texture of that pack image, `none` no
    /// texture; `drawn_layers`; `FUN_0045ec40`). Recorded.
    pub clouds: [String; 3],
    /// Weather region B of the same block (`MULTI*` only): the maps and scales the layer
    /// blends to by the sky-map keys of `Sky/<track>.bin` (`FUN_0045b5a0` reads both regions,
    /// `FUN_0045e160` mixes the texels, `FUN_0045f0a0` the scales; `docs/re/weather.md`).
    pub region_b: Option<Box<SkyMapBlock>>,
}

impl SkyMapBlock {
    /// Number of `MULTI*` layers the original draws (`FUN_0045b5a0` `+0x80`): the cloud
    /// textures that are not `none`, counted over all three slots; layer `i` then uses map
    /// slot `i` and cloud slot `i` (so a `none` before a later cloud leaves an untextured layer
    /// and drops the last one). Recorded (`replay-sky-layer.py`, `cloud-gap`).
    pub fn drawn_layers(&self) -> usize {
        self.clouds.iter().filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("none")).count()
    }
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
            clouds: Default::default(),
            region_b: None,
        });
    }
    // MULTI*: 3 maps, 3 x, 3 y, move, 3 speeds, 3 directions, warp speed, warp amount,
    // for region A (tokens 3..21) and region B (21..39).
    let mut a = multi_region(t, 3, &capability, &kind)?;
    a.region_b = multi_region(t, 3 + 18, &capability, &kind).map(Box::new);
    // Cloud textures follow region B (tokens 39..42).
    if let Some(c) = t.get(3 + 36..3 + 39) {
        a.clouds = [c[0].to_string(), c[1].to_string(), c[2].to_string()];
    }
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
        clouds: Default::default(),
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
        // The cloud textures close the block.
        assert_eq!(b.clouds, ["flat", "cc1a", "none"]);
    }

    #[test]
    fn sky_vertex_colour_is_gouraud_over_the_grid() {
        // Texel (x, y) = (x * 16, y * 16, 0, 255): exact at vertices, linear inside a quad.
        let mut m = vec![0u8; 16 * 16 * 4];
        for y in 0..16 {
            for x in 0..16 {
                let i = (y * 16 + x) * 4;
                m[i] = (x * 16) as u8;
                m[i + 1] = (y * 16) as u8;
                m[i + 3] = 255;
            }
        }
        let c = sky_vertex_colour(&m, 3.0, 7.0);
        assert!((c[0] - 48.0 / 255.0).abs() < 1e-6 && (c[1] - 112.0 / 255.0).abs() < 1e-6);
        let c = sky_vertex_colour(&m, 3.25, 7.5);
        assert!((c[0] - 52.0 / 255.0).abs() < 1e-6 && (c[1] - 120.0 / 255.0).abs() < 1e-6);
        // Clamped at the border; the last row/column is a vertex too.
        let c = sky_vertex_colour(&m, 15.0, 20.0);
        assert!((c[0] - 240.0 / 255.0).abs() < 1e-6 && (c[1] - 240.0 / 255.0).abs() < 1e-6);
        // The quad split (0,1,2)/(0,2,3), not bilinear (which gives 0.1875 off the diagonal).
        let mut q = vec![0u8; 16 * 16 * 4];
        q[(16 + 1) * 4] = 255; // vertex (1, 1) only
        assert!((sky_vertex_colour(&q, 0.5, 0.5)[0] - 0.5).abs() < 1e-6);
        assert!((sky_vertex_colour(&q, 0.75, 0.25)[0] - 0.25).abs() < 1e-6);
        assert!((sky_vertex_colour(&q, 0.25, 0.75)[0] - 0.25).abs() < 1e-6);
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
        assert_eq!(high.clouds, ["flat", "none", "none"]);
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

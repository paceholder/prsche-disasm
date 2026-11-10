//! How the original lights a car: pass 1 of every car render method (workspace
//! `docs/re/lighting.md`, "Car vertex lighting" and "Car draw recorded"). GPU-independent; the
//! viewer (`porsche-viewer` `car.wgsl`, `renderer.rs`) draws it.
//!
//! Recorded in Unicorn, bit for bit (`scripts/research/replay-car-light.py`,
//! `scripts/research/replay-car-draw.py`; tests `tests/original_car_light.rs`):
//! - [`road_colour`]: `Road_SampleColour` 0x4746a0 -> 0x474700, the road colour `S` under the
//!   car from the support polygon's RGB555 vertex colours;
//! - [`light_state`]: the lighting part of `R3DCar_UpdateLighting` 0x43a560 (asm
//!   0x43a8a6..0x43ad59): ambient and diffuse slots from `S` and the `.map [header]`, light
//!   direction from the sun or straight down on covered road, specular flags;
//! - [`VertexLight`]: `R3DLightLayer::Apply` 0x446530, the per-vertex colour (ARGB) that the
//!   pass-1 draw sends to Direct3D and that the later passes of the same method (sphere map,
//!   planar sky map) reuse as their diffuse (the shared vertex buffer of the draw context);
//! - [`unclipped`] / [`sky_uv_path`]: when the planar sky map (0x445b10) takes its swapped
//!   operands: the draw-context flag 0x100 of the group (all vertices of the position array
//!   inside the view, 0x540bc0, and no polygon of the group culled by the back-face test of
//!   `LayerTransformIndexCoords` 0x449390 when the method has the plane channel `$n`).

/// The track lighting header of `TrackLighting_Load` 0x4409d0 (`<track>.map [header]`, each
/// `a, r, g, b`): object `+0` tint, `+0x10` ambient, `+0x20` interior, `+0x30`
/// interiordiffuse, `+0x40` lightdecal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackLightHeader {
    pub tint: [f32; 4],
    pub ambient: [f32; 4],
    pub interior: [f32; 4],
    pub interior_diffuse: [f32; 4],
    pub light_decal: [f32; 4],
}

impl TrackLightHeader {
    /// From the object words `+0..+0x50` (as `TrackLighting_Load` stores them).
    pub fn from_words(w: &[f32; 20]) -> Self {
        let q = |k: usize| [w[4 * k], w[4 * k + 1], w[4 * k + 2], w[4 * k + 3]];
        Self { tint: q(0), ambient: q(1), interior: q(2), interior_diffuse: q(3), light_decal: q(4) }
    }

    /// From the parsed `.map [header]` (missing keys: the loader's defaults, tint 1, ambient
    /// `1, 0.3, 0.3, 0.3`, the others 1).
    pub fn from_map(h: &nfs_formats::MapHeader) -> Self {
        Self {
            tint: h.tint,
            ambient: h.ambient,
            interior: h.interior,
            interior_diffuse: h.interiordiffuse,
            light_decal: h.lightdecal,
        }
    }
}

/// Inputs of the lighting part of 0x43a560 besides the header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightInputs {
    /// `S` = [`road_colour`] at the car (`1, r, g, b`).
    pub road: [f32; 4],
    /// `0x473330(car+0x330)` flags & 0xc000 (covered road: tunnels, roofs).
    pub covered: bool,
    /// The sun of the sky object `[0x627e5c]+0x11c` (`Sky_SunPosition` 0x461090).
    pub sun: [f32; 4],
    /// `[0x657424] >= 2 || [0x657d64] > 0` (the `SPECULAR` option, graphics detail).
    pub specular_option: bool,
    /// The camera of type 1 (the mirror view): light state byte `+0x190`.
    pub mirror_camera: bool,
}

/// The car light state `R3DCar+0x2dc` words `+0x18..+0xd0` and `+0x190` after 0x43a560.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarLightState {
    /// `+0x18 + 0x10·slot` (`a, r, g, b`): slot 0 `S·ambient`, 1 `·interior`, 2 `·lightdecal`.
    pub ambient: [[f32; 4]; 3],
    /// `+0x48`: the light direction (`x, y, z, 0`): `−sun/|sun|`, or `(0, −1, 0)` covered.
    pub light_dir: [f32; 4],
    /// `+0x58 + 0x10·slot`: slot 0 `S·tint`, 1 `S·interiordiffuse`, 2 slot 0 `·lightdecal`.
    pub diffuse: [[f32; 4]; 3],
    /// Byte `+0x89`: specular on (only on open road with the option).
    pub specular: bool,
    /// `+0x8c`: light colour (the head of 0x43a560 stores `1, 1, 1, 1`).
    pub light_colour: [f32; 4],
    /// `+0x9c..`: specular colours, `+0xcc` power (5 from the head, 10 on open road).
    pub specular_colours: [[f32; 4]; 3],
    pub power: f32,
    pub mirror: bool,
}

const POINT_2: f32 = f32::from_bits(0x3e4c_cccd);
const POINT_8: f32 = f32::from_bits(0x3f4c_cccd);

impl CarLightState {
    /// The words `+0x18..+0xd0` (46) in memory order (the byte flags `+0x88` = 1, `+0x89`).
    pub fn words(&self) -> [u32; 46] {
        let mut w = [0u32; 46];
        let mut put = |k: usize, q: [f32; 4]| {
            for i in 0..4 {
                w[k + i] = q[i].to_bits();
            }
        };
        for s in 0..3 {
            put(4 * s, self.ambient[s]);
            put(16 + 4 * s, self.diffuse[s]);
            put(33 + 4 * s, self.specular_colours[s]);
        }
        put(12, self.light_dir);
        put(29, self.light_colour);
        w[28] = 1 | (u32::from(self.specular) << 8);
        w[45] = self.power.to_bits();
        w
    }

    /// From the words `+0x18..+0xd0` ([`Self::words`] order) and the byte `+0x190`.
    pub fn from_words(w: &[u32; 46], mirror: bool) -> Self {
        let q = |k: usize| [0, 1, 2, 3].map(|i| f32::from_bits(w[k + i]));
        Self {
            ambient: [q(0), q(4), q(8)],
            light_dir: q(12),
            diffuse: [q(16), q(20), q(24)],
            specular: w[28] & 0xff00 != 0,
            light_colour: q(29),
            specular_colours: [q(33), q(37), q(41)],
            power: f32::from_bits(w[45]),
            mirror,
        }
    }
}

fn mul4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [0, 1, 2, 3].map(|i| (a[i] as f64 * b[i] as f64) as f32)
}

/// The lighting part of `R3DCar_UpdateLighting` 0x43a560 (asm 0x43a8a6..0x43ad59; every
/// product an x87 multiply stored f32).
pub fn light_state(h: &TrackLightHeader, i: &LightInputs) -> CarLightState {
    let s = i.road;
    let mut amb0 = s;
    for c in 1..4 {
        amb0[c] = (h.ambient[c] as f64 * s[c] as f64) as f32;
    }
    amb0[0] = 1.0;
    let amb1 = mul4(h.interior, amb0);
    let amb2 = mul4(h.light_decal, amb0);
    let mut dif0 = mul4(h.tint, s);
    dif0[0] = 1.0;
    let mut dif1 = mul4(h.interior_diffuse, s);
    dif1[0] = 1.0;
    let dif2 = mul4(h.light_decal, dif0);
    let head_spec = [[1.0, POINT_2, POINT_2, POINT_2], [1.0, POINT_8, POINT_8, POINT_8], [1.0; 4]];
    let (light_dir, specular, power) = if i.covered {
        ([0.0, -1.0, 0.0, 0.0], false, 5.0)
    } else {
        // asm 0x43ac8d..0x43ad36: -x stays in the x87 register, -y / -z are stored f32.
        let (x, y, z) = (-(i.sun[0] as f64), -i.sun[1], -i.sun[2]);
        let len = ((z as f64 * z as f64 + y as f64 * y as f64) + x * x).sqrt();
        let inv = 1.0 / len;
        let inv32 = inv as f32;
        let lx = (inv * x) as f32;
        let ly = (inv32 as f64 * y as f64) as f32;
        let lz = (inv32 as f64 * z as f64) as f32;
        ([lx, ly, lz, 0.0], i.specular_option, 10.0)
    };
    CarLightState {
        ambient: [amb0, amb1, amb2],
        light_dir,
        diffuse: [dif0, dif1, dif2],
        specular,
        light_colour: [1.0; 4],
        specular_colours: head_spec,
        power,
        mirror: i.mirror_camera,
    }
}

/// The layer parameter block of an `R3DLightLayer` (`mt` layer type 5, 12 bytes): `kA`, `kD`
/// (f32) and the slot (an integer word, asm 0x4465d8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightParams {
    pub ka: f32,
    pub kd: f32,
    pub slot: u32,
}

impl LightParams {
    /// From the 12-byte block (`mt` layer type 5).
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let w = |k: usize| b.get(4 * k..4 * k + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        Some(Self { ka: f32::from_bits(w(0)?), kd: f32::from_bits(w(1)?), slot: w(2)? })
    }
}

/// The light-layer parameter block of a car `mt` record: its one 12-byte layer entry (the
/// `mt` layer table after the textures: `{type, offset, size, _}` headers, then the blob). The
/// type is the method's attribute of the light layer (5 in `CarExtEnv` / `CarWindow`, 3 in
/// `CarExt`, 2 in `CarInt` / `CarWheel`); every shipped `mt` has exactly one (checked on the
/// recorded draws of the 356A and the 993: tests/original_car_light.rs).
pub fn material_light_params(mt: &[u8]) -> Option<LightParams> {
    let u16le = |o: usize| mt.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32le = |o: usize| mt.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let (textures, layers) = (u16le(8)?, u16le(10)?);
    let table = 0x20 + textures * 0x2c;
    let blob = table + layers * 16;
    (0..layers).find_map(|k| {
        let h = table + 16 * k;
        (u32le(h + 8)? == 12).then_some(())?;
        let o = blob + u32le(h + 4)?;
        LightParams::from_bytes(mt.get(o..o + 12)?)
    })
}

/// The `mt+0xc` method flags that shape the car layers (0x44bf70 definitions, recorded for all
/// cars: replay-car-env-map.py `method` rows): `0x20` adds the plane channel `$n` (back-face
/// test of 0x449390), `0x10000` the byte index channel (the clipped indexed loops).
pub fn material_plane_test(mt_flags: u32) -> bool {
    mt_flags & 0x20 != 0
}

pub fn material_indexed(mt_flags: u32) -> bool {
    mt_flags & 0x1_0000 != 0
}

/// `x + 1.5·2^23` stored f32, the low 22 mantissa bits, clamped to 255 (negative sums clamp to
/// 255 too: the mantissa then holds `2^22 + x`).
fn byte(x: f64) -> u32 {
    let v = (x + 12_582_912.0) as f32;
    (v.to_bits() & 0x3f_ffff).min(255)
}

/// `R3DLightLayer::Apply` 0x446530 prepared for one draw: the light in model space and the
/// colour terms (asm 0x4465d5..0x44689b).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VertexLight {
    /// The word of an unlit vertex: `kA·255·ambient.rgb`, alpha `255·ambient.a` (no `kA`).
    pub ambient_word: u32,
    /// `kD <= 0`: every vertex gets [`Self::ambient_word`].
    pub lit: bool,
    /// `−255·(M_rows · L)`: the light in model space (rows 0..2 of the model matrix).
    pub light: [f32; 3],
    /// `kD · light colour · diffuse[slot]` per channel (r, g, b).
    pub diffuse: [f32; 3],
    /// `kA · 255 · ambient[slot]` per channel.
    pub ambient: [f32; 3],
}

impl VertexLight {
    /// `model` row-major (rows right, up, forward, translation) as the render context's model
    /// matrix `[[ctx]+0x5c]+4`.
    pub fn new(state: &CarLightState, p: LightParams, model: &[f32; 16]) -> Option<Self> {
        let slot = usize::try_from(p.slot).ok().filter(|&s| s < 3)?;
        let a = state.ambient[slot];
        let ka255 = p.ka as f64 * 255.0;
        let amb = [1, 2, 3].map(|c| (a[c] as f64 * ka255) as f32);
        let alpha = (a[0] as f64 * 255.0) as f32;
        let word = byte(alpha as f64) << 24 | byte(amb[0] as f64) << 16 | byte(amb[1] as f64) << 8 | byte(amb[2] as f64);
        let l = state.light_dir;
        let m = model.map(f64::from);
        let (lx, ly, lz) = (l[0] as f64, l[1] as f64, l[2] as f64);
        let k = f32::from_bits(0xc37f_0000) as f64; // -255 [0x5b38e8]
        let light = [
            (((m[2] * lz + m[1] * ly) + lx * m[0]) * k) as f32,
            (((m[6] * lz + m[5] * ly) + m[4] * lx) * k) as f32,
            (((m[10] * lz + m[9] * ly) + m[8] * lx) * k) as f32,
        ];
        let d = state.diffuse[slot];
        let c = state.light_colour;
        let kd = p.kd as f64;
        let r = ((d[1] as f64 * c[1] as f64) * kd) as f32;
        let g = (((d[2] as f64 * c[2] as f64) as f32) as f64 * kd) as f32;
        let b = (((d[3] as f64 * c[3] as f64) as f32) as f64 * kd) as f32;
        Some(Self { ambient_word: word, lit: p.kd > 0.0, light, diffuse: [r, g, b], ambient: amb })
    }

    /// The colour word of a vertex with model-space normal `n` (asm 0x446925..0x446a17).
    pub fn colour(&self, n: [f32; 3]) -> u32 {
        if !self.lit {
            return self.ambient_word;
        }
        let l = self.light.map(f64::from);
        let d = ((l[2] * n[2] as f64 + l[1] * n[1] as f64) + l[0] * n[0] as f64) as f32;
        if d.is_sign_negative() {
            return self.ambient_word;
        }
        let c = |k: usize| byte(self.diffuse[k] as f64 * d as f64 + self.ambient[k] as f64);
        0xff00_0000 | c(0) << 16 | c(1) << 8 | c(2)
    }
}

/// The road colour `S` (`1, r, g, b`) of `Road_SampleColour` 0x474700 on the polygon under
/// the car (`vertices` in original axes, `colours` RGB555 as the polygon stores them at
/// `+0x1c`); `None` = no polygon: `(1, 1, 1, 1)`. A quad uses the triangle (0, 1, 2) when the
/// point is on its inner side of all three edges, else (0, 2, 3).
pub fn road_colour(polygon: Option<(&[[f32; 3]], &[u16])>, point: [f32; 3]) -> [f32; 4] {
    let Some((v, c)) = polygon else {
        return [1.0; 4];
    };
    if v.len() < 3 || c.len() < v.len() {
        return [1.0; 4];
    }
    let edge = |a: [f32; 3], b: [f32; 3]| -> bool {
        let x = (point[2] as f64 - a[2] as f64) * (b[0] as f64 - a[0] as f64)
            - (point[0] as f64 - a[0] as f64) * (b[2] as f64 - a[2] as f64);
        (x as f32).to_bits() as i32 >= 1
    };
    let (tri, col) = if v.len() == 4 && (edge(v[1], v[0]) || edge(v[2], v[1]) || edge(v[0], v[2])) {
        ([v[0], v[2], v[3]], [c[0], c[2], c[3]])
    } else {
        ([v[0], v[1], v[2]], [c[0], c[1], c[2]])
    };
    let k = f32::from_bits(0x3b80_8081) as f64; // 1/255 [0x5b3884]
    let comp = |w: u16, ch: usize| -> f32 {
        let w = u32::from(w);
        let q = match ch {
            0 => (w >> 7) & 0xf8,
            1 => (w >> 2) & 0xf8,
            _ => (w & 0x1f) << 3,
        };
        (q as f64 * k) as f32
    };
    let mut out = [1.0f32; 4];
    for ch in 0..3 {
        let p: [[f32; 3]; 3] = std::array::from_fn(|j| [tri[j][0], comp(col[j], ch), tri[j][2]]);
        let n = plane_normal(p);
        let h = plane_height(n, p[0], point);
        out[ch + 1] = if 1.0 <= h { 1.0 } else { h as f32 };
    }
    out
}

/// 0x474610: `normalize(cross(p1 − p0, p0 − p2))` (0x532880, 0x5328d0), `y` capped below
/// 0.9999 (to 0x3f7ff972).
fn plane_normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let sub = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| (a[i] as f64 - b[i] as f64) as f32);
    let a = sub(p[1], p[0]).map(f64::from);
    let b = sub(p[0], p[2]).map(f64::from);
    let c = [
        (a[1] * b[2] - a[2] * b[1]) as f32,
        (a[2] * b[0] - a[0] * b[2]) as f32,
        (a[0] * b[1] - a[1] * b[0]) as f32,
    ]
    .map(f64::from);
    let inv = 1.0 / ((c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]).sqrt();
    let mut n = c.map(|v| (inv * v) as f32);
    // fcomp [0x5b3e0c]; jne when below (or unordered).
    if n[1] >= CAP {
        n[1] = CAP;
    }
    n
}

/// 0.9999 as f32 ([0x5b3e0c] = 0x3f7ff972): the compare constant and the cap of 0x474610.
const CAP: f32 = f32::from_bits(0x3f7f_f972);

/// 0x48a060: the height of plane `n` through `p` at `q` (x87, not rounded).
fn plane_height(n: [f32; 3], p: [f32; 3], q: [f32; 3]) -> f64 {
    let n = n.map(f64::from);
    let p = p.map(f64::from);
    let q = q.map(f64::from);
    let d = (p[1] * n[1] + p[2] * n[2]) + n[0] * p[0];
    -((-d + q[2] * n[2]) + n[0] * q[0]) / n[1]
}

/// Draw-context flag 0x100 of a group after `LayerTransformIndexCoords` 0x449390: set by the
/// transform 0x540bc0 when no vertex of the position array is outside the view (clip codes
/// `& 0x3f` all zero), cleared again when the method has the plane channel (`$n`, `mt`
/// flags 0x20) and the back-face test culled any polygon of the group (`plane·eye < d`).
pub fn unclipped(all_inside: bool, plane_test: bool, any_back_face: bool) -> bool {
    all_inside && !(plane_test && any_back_face)
}

/// Whether the back-face test of 0x449390 culls any triangle of a group, in original model
/// axes (CRP `vt`, before the model matrix): the plane `(p2 − p0) × (p1 − p0)` through `p0`
/// against the eye in model space, culled when `n·(eye − p0) < 0`. A geometric stand-in for
/// the `$n` plane channel: it agrees with the recorded plane test on every recorded group
/// (tests/original_car_light.rs).
pub fn any_back_face<'a>(triangles: impl IntoIterator<Item = &'a [[f32; 3]; 3]>, eye: [f32; 3]) -> bool {
    triangles.into_iter().any(|t| {
        let p = t.map(|v| v.map(f64::from));
        let e1 = [0, 1, 2].map(|i| p[1][i] - p[0][i]);
        let e2 = [0, 1, 2].map(|i| p[2][i] - p[0][i]);
        // (p2 − p0) × (p1 − p0): the outward side of the CRP winding.
        let n = [e2[1] * e1[2] - e2[2] * e1[1], e2[2] * e1[0] - e2[0] * e1[2], e2[0] * e1[1] - e2[1] * e1[0]];
        let d = [0, 1, 2].map(|i| f64::from(eye[i]) - p[0][i]);
        n[0] * d[0] + n[1] * d[1] + n[2] * d[2] < 0.0
    })
}

/// Which loop of 0x445b10 writes the sky-map coordinates: the swapped operands
/// `k·position + normal` for a group with the byte index channel and flag 0x100 clear.
pub fn sky_uv_path(indexed: bool, unclipped: bool) -> crate::car_env::UvPath {
    if indexed && !unclipped {
        crate::car_env::UvPath::Swapped
    } else {
        crate::car_env::UvPath::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_rounds_and_clamps() {
        assert_eq!(byte(76.5), 76); // nearest even
        assert_eq!(byte(77.5), 78);
        assert_eq!(byte(300.0), 255);
        assert_eq!(byte(-3.0), 255);
        assert_eq!(byte(0.0), 0);
    }

    #[test]
    fn no_polygon_is_white() {
        assert_eq!(road_colour(None, [0.0; 3]), [1.0; 4]);
    }
}

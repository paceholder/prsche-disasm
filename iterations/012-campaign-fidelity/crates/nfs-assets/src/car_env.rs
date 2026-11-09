//! The car's planar sky-map layer (`R3DSkyMapTextureLayer` 0x445b10): the track's `.env`
//! tree-shade and tunnel textures that the car's `.map` section puts on the body
//! (workspace `docs/re/lighting.md`, "Sky map on the car"). GPU-independent; the viewer
//! (`porsche-viewer` `env_map.rs`, `car.wgsl`) draws it.
//!
//! Recorded in Unicorn (`scripts/research/replay-car-env-map.py`, tests
//! `tests/original_car_env_map.rs`), bit for bit:
//! - [`load_sections`]: `TrackLighting_Load` 0x4409d0 with the INI reader 0x523c40 /
//!   0x523e50 / 0x524030 and the CRT `atoi` / `atof`;
//! - [`select_section`], [`section_texture`], [`section_size`], [`section_aspect`]:
//!   0x441140, 0x441180, 0x4411c0, 0x4411e0;
//! - [`update_light`]: the sky-map part of `R3DCar_UpdateLighting` 0x43a560
//!   (0x43a6c9..0x43a8a6), with [`roof_flags`]-style input `covered` (`0x473330` over the car,
//!   `SupportTree::overhead_flags`);
//! - [`layer_uv`]: the per-vertex texture coordinates of 0x445b10, both its correct paths and
//!   the swapped operands of its clipped indexed path ([`UvPath`]);
//! - [`slot_for_param`], [`LayerDraw`]: the texture slot and the early return of 0x445b10.

/// FourCC `"ENV0"`: 0x4409d0's default for a missing `texture{k}.{open,covered}` key. No
/// `.env` pack has it, so the slot stays empty.
pub const ENV0: u32 = 0x3056_4e45;
/// Light state `+0` (`R3DCarLight` ctor 0x440780 from 0x5b37c0; nothing else writes it): the
/// layer projects `position + 3.33·normal`.
pub const NORMAL_PUSH: f32 = f32::from_bits(0x4055_1eb8);
/// `1/6` at 0x5b2420: the node offsets `o1`, `o2` are distances divided by 6.
const SIXTH: f32 = f32::from_bits(0x3e2a_aaab);

/// Sampling of the `.env` textures: 0x4408c0 sets the slot texture's address states (state
/// block words 5, 6) to 1 and the bind 0x536240 sends THRASH 0xd = 1 → D3DTSS_ADDRESS WRAP;
/// the texture's own filter word 1 (0x5379d0) → MAG / MIN LINEAR, MIP NONE (dx7z.dll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvSampler {
    pub wrap: bool,
    pub linear: bool,
    pub mipmaps: bool,
}

pub const ENV_SAMPLER: EnvSampler = EnvSampler { wrap: true, linear: true, mipmaps: false };

/// One section record of the track lighting vector `+0x54` (0x48 bytes): slice range,
/// `texture{1,2,3}.open` and `.covered` FourCCs, `texturesize`, `aspect`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvSection {
    pub start: u32,
    pub end: u32,
    pub open: [u32; 3],
    pub covered: [u32; 3],
    pub size: f32,
    pub aspect: f32,
}

/// The INI text as 0x523c40 splits it: per section the trimmed name (None when the `]` is
/// missing) and the byte offset of the section text.
struct Ini<'a> {
    text: &'a [u8],
    sections: Vec<(Option<&'a [u8]>, Option<usize>)>,
}

fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// 0x523dc0: strip spaces and tabs at both ends.
fn trim(s: &[u8]) -> &[u8] {
    let mut a = 0;
    while a < s.len() && is_space(s[a]) {
        a += 1;
    }
    let mut b = s.len();
    while b > a + 1 && is_space(s[b - 1]) {
        b -= 1;
    }
    if b <= a {
        return &s[a..a];
    }
    &s[a..b]
}

/// 0x523d70: the start of the next non-blank line after `i` (None at the end).
fn next_line(text: &[u8], mut i: usize) -> Option<usize> {
    let end = text.len();
    while i < end && text[i] != b'\n' && text[i] != b'\r' {
        i += 1;
    }
    if i >= end {
        return None;
    }
    while i < end && matches!(text[i], b'\n' | b'\r' | b' ' | b'\t') {
        i += 1;
    }
    (i < end).then_some(i)
}

impl<'a> Ini<'a> {
    /// 0x523c40: at most 700 sections; a `[` line starts one, other lines are skipped.
    fn parse(text: &'a [u8]) -> Self {
        // The parser works on a NUL-terminated copy: a NUL ends the text.
        let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
        let mut sections = Vec::new();
        let mut i = Some(0);
        while let Some(p) = i {
            if sections.len() >= 700 || p >= text.len() {
                break;
            }
            if text[p] == b'[' {
                let s = p + 1;
                let mut e = s;
                while e < text.len() && !matches!(text[e], b']' | b'\n' | b'\r') {
                    e += 1;
                }
                // The section text starts at the next line (0x523d70 from the `]`).
                let body = next_line(text, e);
                let name = (e < text.len() && text[e] == b']').then(|| trim(&text[s..e]));
                sections.push((name, body));
                i = body;
            } else {
                i = next_line(text, p);
            }
        }
        Self { text, sections }
    }

    /// 0x523fb0 + 0x523e50: the keys of the first section named `name` (at most 100;
    /// the first match wins in 0x524030).
    fn keys(&self, name: &[u8]) -> Option<Vec<(&'a [u8], &'a [u8])>> {
        let (_, start) = *self.sections.iter().find(|(n, _)| *n == Some(name))?;
        let text = self.text;
        let end = text.len();
        let mut out = Vec::new();
        let mut p = start;
        while let Some(s) = p {
            if text[s] == b'[' || out.len() > 99 {
                break;
            }
            let mut e = s;
            while e < end && !matches!(text[e], b'=' | b'\n' | b'\r' | 0) {
                e += 1;
            }
            if e >= end || text[e] != b'=' {
                // A line without `=`: 0x523e50 does not advance past it (never in shipped
                // data); stop here.
                break;
            }
            let key = trim(&text[s..e]);
            let v = e + 1;
            let mut ve = v;
            while ve < end && !matches!(text[ve], b'\n' | b'\r' | b';' | 0) {
                ve += 1;
            }
            out.push((key, trim(&text[v..ve])));
            p = next_line(text, ve);
        }
        Some(out)
    }
}

fn lookup<'a>(keys: &[(&'a [u8], &'a [u8])], name: &str) -> Option<&'a [u8]> {
    keys.iter().find(|(k, _)| *k == name.as_bytes()).map(|(_, v)| *v)
}

/// MSVC `atoi` (0x5a1f4a): optional blanks and sign, decimal digits, wrapping i32.
pub fn crt_atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let neg = i < s.len() && s[i] == b'-';
    if i < s.len() && matches!(s[i], b'-' | b'+') {
        i += 1;
    }
    let mut v: i32 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add(i32::from(s[i] - b'0'));
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// MSVC `atof` (0x5a21b8) as the loader stores it (f32): the longest decimal prefix
/// (blanks, sign, digits, `.`, exponent), 0 when there is none.
pub fn crt_atof(s: &[u8]) -> f32 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let start = i;
    if i < s.len() && matches!(s[i], b'-' | b'+') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let a = *i;
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - a
    };
    let mut n = digits(&mut i);
    if i < s.len() && s[i] == b'.' {
        i += 1;
        n += digits(&mut i);
    }
    if n == 0 {
        return 0.0;
    }
    let mantissa_end = i;
    if i < s.len() && matches!(s[i], b'e' | b'E') {
        let mut j = i + 1;
        if j < s.len() && matches!(s[j], b'-' | b'+') {
            j += 1;
        }
        if digits(&mut j) > 0 {
            i = j;
        }
    }
    let text = std::str::from_utf8(&s[start..i]).unwrap_or("0");
    let text = if text.ends_with('.') { &text[..text.len() - 1] } else { text };
    let _ = mantissa_end;
    text.parse::<f64>().map_or(0.0, |v| v as f32)
}

/// `TrackLighting_Load` 0x4409d0: `[section0..count-1]` of the `.map` text (missing sections
/// skipped), keys `startslice`, `endslice`, `texture{1,2,3}.{open,covered}` (FourCCs as
/// integers, default [`ENV0`]), `texturesize` (default 6) and `aspect` (default 1).
pub fn load_sections(map: &[u8]) -> Vec<EnvSection> {
    let ini = Ini::parse(map);
    let Some(header) = ini.keys(b"header") else {
        return Vec::new();
    };
    let count = lookup(&header, "count").map_or(0, crt_atoi) as u32;
    let mut out = Vec::new();
    for d in 0..count {
        let Some(keys) = ini.keys(format!("section{d}").as_bytes()) else {
            continue;
        };
        let int = |k: &str, default: u32| lookup(&keys, k).map_or(default, |v| crt_atoi(v) as u32);
        let float = |k: &str, default: f32| lookup(&keys, k).map_or(default, crt_atof);
        out.push(EnvSection {
            start: int("startslice", 0),
            end: int("endslice", 0),
            open: [1, 2, 3].map(|k| int(&format!("texture{k}.open"), ENV0)),
            covered: [1, 2, 3].map(|k| int(&format!("texture{k}.covered"), ENV0)),
            size: float("texturesize", 6.0),
            aspect: float("aspect", 1.0),
        });
    }
    out
}

/// `TrackLighting_SelectSection` 0x441140: the first section with `start <= node <= end`,
/// the node compared as an unsigned word (`movsx` of the locator node, so -1 is 0xffffffff).
pub fn select_section(sections: &[EnvSection], node: i32) -> Option<usize> {
    let n = node as u32;
    sections.iter().position(|s| s.start <= n && n <= s.end)
}

/// 0x441180 (covered, k): the FourCC of slot `k` (`.covered` on covered road), none without a
/// section.
pub fn section_texture(sections: &[EnvSection], sel: Option<usize>, covered: bool, k: usize) -> Option<u32> {
    let s = sections.get(sel?)?;
    Some(if covered { s.covered[k] } else { s.open[k] })
}

/// 0x4411c0: `texturesize` of the selected section, 6 without one.
pub fn section_size(sections: &[EnvSection], sel: Option<usize>) -> f32 {
    sel.and_then(|i| sections.get(i)).map_or(6.0, |s| s.size)
}

/// 0x4411e0: `aspect` of the selected section, 1 without one.
pub fn section_aspect(sections: &[EnvSection], sel: Option<usize>) -> f32 {
    sel.and_then(|i| sections.get(i)).map_or(1.0, |s| s.aspect)
}

/// The car's inputs to the sky-map part of 0x43a560, original coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvCar {
    /// `car+0x330`.
    pub position: [f32; 3],
    /// Locator node `car+8` (i16).
    pub node: i16,
    /// `0x473330(car+0x330)` flags & 0xc000 != 0 (covered road).
    pub covered: bool,
    /// Node centre `+4`, lateral `+0x28` and forward `+0x1c` of node `node` (0x43a7c4).
    pub node_pos: [f32; 3],
    pub node_lat: [f32; 3],
    pub node_fwd: [f32; 3],
    /// The car's node frame `car+0x3b8` (lateral), `+0x3c4` (up), `+0x3d0` (forward).
    pub frame: [[f32; 3]; 3],
}

/// The sky-map fields of the car light state (`R3DCar+0x2dc`) after 0x43a560.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvLight {
    /// `+0xd0..`: FourCCs of the slot textures (None: no section / CARCHROME below 2).
    pub slots: [Option<u32>; 3],
    /// `+0`: the normal push.
    pub k: f32,
    /// `+4 = 1/texturesize`.
    pub inv_size: f32,
    /// `+8 = aspect`.
    pub aspect: f32,
    /// `+0xe8 = car+0x330`.
    pub position: [f32; 3],
    /// `+0x104`: the transpose of the node frame (row-major, rows of 4; translation zero).
    pub frame_t: [f32; 16],
    /// `+0x144`, `+0x148`: node offsets along the node lateral and forward axes / 6.
    pub offsets: [f32; 2],
}

/// The x87 sum `(a0·b0 + a1·b1) + a2·b2` of `0x532910` (53-bit precision, f32 operands).
fn dot_x87(a: [f32; 3], b: [f32; 3]) -> f64 {
    (a[0] as f64 * b[0] as f64 + a[1] as f64 * b[1] as f64) + a[2] as f64 * b[2] as f64
}

/// The sky-map part of `R3DCar_UpdateLighting` 0x43a560 (asm 0x43a6c9..0x43a8a6) with the
/// `CARCHROME` 2 flags (0x6197ca / 0x6197cb) set; `chrome = false` leaves the slots empty
/// (CARCHROME 0 or 1).
pub fn update_light(sections: &[EnvSection], car: &EnvCar, chrome: bool) -> EnvLight {
    let sel = select_section(sections, i32::from(car.node));
    let slots = [0, 1, 2].map(|k| if chrome { section_texture(sections, sel, car.covered, k) } else { None });
    let inv_size = (1.0f64 / section_size(sections, sel) as f64) as f32;
    let d = [0, 1, 2].map(|i| (car.position[i] as f64 - car.node_pos[i] as f64) as f32);
    let o1 = (dot_x87(car.node_lat, d) * SIXTH as f64) as f32;
    let o2 = (dot_x87(car.node_fwd, d) * SIXTH as f64) as f32;
    let [l, u, f] = car.frame;
    let frame_t = [
        l[0], u[0], f[0], 0.0, //
        l[1], u[1], f[1], 0.0, //
        l[2], u[2], f[2], 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    EnvLight {
        slots,
        k: NORMAL_PUSH,
        inv_size,
        aspect: section_aspect(sections, sel),
        position: car.position,
        frame_t,
        offsets: [o1, o2],
    }
}

/// 0x445b10: the texture slot of the layer parameter `**(param+4)`: `< 0.05` slot 0,
/// `< 0.95` slot 1, else 2 (x87 compares with the f32 constants 0x5b38b0 / 0x5b38b4).
pub fn slot_for_param(param: f32) -> usize {
    let p = param as f64;
    if !(p >= f32::from_bits(0x3d4c_cccd) as f64) {
        0
    } else if !(p >= f32::from_bits(0x3f73_3333) as f64) {
        1
    } else {
        2
    }
}

/// What 0x445b10 does for a draw: `None` slot texture or the light state's disable byte
/// `+0x190` (the camera of type 1, the mirror view, 0x43ad4e) → no coordinates; with the
/// layer type 0xe (every car `RMth` `mC` entry) or a 32-byte stride it then returns the layer
/// after the pass's draw layer (0x53f460(0, 0)), i.e. the pass is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerDraw {
    /// Texture coordinates are written; the pass draws with this FourCC.
    Draw(u32),
    /// Nothing written and the pass's draw layer is skipped.
    SkipPass,
    /// Nothing written; the next layer runs (another type with another stride).
    Next,
}

pub fn layer_draw(light: &EnvLight, param: f32, disabled: bool, layer_type: u32, stride: u32) -> LayerDraw {
    let tex = if disabled { None } else { light.slots[slot_for_param(param)] };
    match tex {
        Some(t) => LayerDraw::Draw(t),
        None if stride == 0x20 || layer_type == 0xe => LayerDraw::SkipPass,
        None => LayerDraw::Next,
    }
}

/// Which vertex loop of 0x445b10 writes the coordinates. All of them project
/// `M·(k·a + b)` with `M` = model-to-world · translate(−car) · node frame transposed, but the
/// clipped indexed loop (a polygon list with an index channel and some vertex of the draw
/// outside the view: draw-context flag `+0x117e4 & 0x100` clear, 0x540bc0) swaps the
/// operands: `k·position + normal` instead of `position + k·normal` [VA 0x445dca..0x445e4e].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvPath {
    /// `position + k·normal` (unclipped draws, and lists without an index channel).
    Normal,
    /// `k·position + normal` (clipped indexed draws).
    Swapped,
}

/// The row-major product `a·b` of 0x5398b0: each element an x87 sum in the order of the
/// code (53-bit precision, so f64 here), stored as f32.
#[rustfmt::skip]
fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let a = a.map(f64::from);
    let b = b.map(f64::from);
    [
        ((((a[0] * b[0] + b[8] * a[2]) + a[1] * b[4]) + a[3] * b[12])) as f32,
        ((((b[9] * a[2] + a[0] * b[1]) + a[1] * b[5]) + b[13] * a[3])) as f32,
        ((((a[0] * b[2] + b[10] * a[2]) + a[1] * b[6]) + b[14] * a[3])) as f32,
        ((((a[3] * b[15] + a[1] * b[7]) + b[3] * a[0]) + b[11] * a[2])) as f32,
        ((((a[5] * b[4] + a[7] * b[12]) + b[8] * a[6]) + a[4] * b[0])) as f32,
        ((((b[5] * a[5] + a[4] * b[1]) + b[13] * a[7]) + b[9] * a[6])) as f32,
        ((((a[5] * b[6] + b[14] * a[7]) + b[10] * a[6]) + a[4] * b[2])) as f32,
        ((((a[4] * b[3] + a[7] * b[15]) + a[6] * b[11]) + a[5] * b[7])) as f32,
        ((((a[9] * b[4] + a[11] * b[12]) + b[8] * a[10]) + a[8] * b[0])) as f32,
        ((((b[5] * a[9] + a[8] * b[1]) + b[13] * a[11]) + b[9] * a[10])) as f32,
        ((((a[9] * b[6] + b[14] * a[11]) + b[10] * a[10]) + a[8] * b[2])) as f32,
        ((((a[8] * b[3] + a[11] * b[15]) + a[10] * b[11]) + a[9] * b[7])) as f32,
        ((((a[13] * b[4] + a[15] * b[12]) + b[8] * a[14]) + a[12] * b[0])) as f32,
        ((((b[5] * a[13] + a[12] * b[1]) + b[13] * a[15]) + b[9] * a[14])) as f32,
        ((((a[13] * b[6] + b[14] * a[15]) + b[10] * a[14]) + a[12] * b[2])) as f32,
        ((((a[12] * b[3] + a[15] * b[15]) + a[14] * b[11]) + a[13] * b[7])) as f32,
    ]
}

/// The layer matrix of 0x445b10 (asm 0x445c56..0x445ceb): `model · T(−car) · frame_t`.
pub fn layer_matrix(light: &EnvLight, model: &[f32; 16]) -> [f32; 16] {
    let p = light.position;
    let neg = [-p[0], -p[1], -p[2]];
    let translate = [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        neg[0], neg[1], neg[2], 1.0,
    ];
    mat_mul(&mat_mul(model, &translate), &light.frame_t)
}

/// Texture coordinates 0x445b10 writes for a vertex (model-space position and normal):
/// `u = (M·q).x · inv_size + (0.1·o1 + 0.5)`, `v = (M·q).z · aspect·inv_size + (o2 + 0.5)`,
/// `q = position + k·normal` ([`UvPath::Normal`]) or `k·position + normal`
/// ([`UvPath::Swapped`]).
pub fn layer_uv(light: &EnvLight, m: &[f32; 16], position: [f32; 3], normal: [f32; 3], path: UvPath) -> [f32; 2] {
    let k = light.k as f64;
    let (a, b) = match path {
        UvPath::Normal => (normal, position),
        UvPath::Swapped => (position, normal),
    };
    let q = [0, 1, 2].map(|i| k * a[i] as f64 + b[i] as f64);
    let x = |v: f32| v as f64;
    let c_u = (x(light.offsets[0]) * x(f32::from_bits(0x3dcc_cccd)) + 0.5) as f32;
    let c_v = (x(light.offsets[1]) + 0.5) as f32;
    let scale_v = (x(light.aspect) * x(light.inv_size)) as f32;
    // asm 0x445ef0..0x445f2a (the other loops sum in the same order).
    let u = (((q[2] * x(m[8]) + q[1] * x(m[4])) + q[0] * x(m[0])) + x(m[12])) * x(light.inv_size) + x(c_u);
    let v = (((x(m[10]) * q[2] + x(m[6]) * q[1]) + x(m[2]) * q[0]) + x(m[14])) * x(scale_v) + x(c_v);
    [u as f32, v as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_number_parsing() {
        assert_eq!(crt_atoi(b"538976305"), 538976305);
        assert_eq!(crt_atoi(b"-4"), -4);
        assert_eq!(crt_atoi(b"4294967295") as u32, 0xffff_ffff);
        assert_eq!(crt_atoi(b" 540103764"), 540103764);
        assert_eq!(crt_atof(b"6.406338"), 6.406338f32);
        assert_eq!(crt_atof(b"abc"), 0.0);
        assert_eq!(crt_atof(b""), 0.0);
        assert_eq!(crt_atof(b"1e3"), 1000.0);
        assert_eq!(crt_atof(b"-107374176.000000").to_bits(), 0xcccc_cccc);
    }

    #[test]
    fn sections_default_and_select() {
        let text = b"[header]\r\ncount=3\r\n[section0]\r\nstartslice=5\r\nendslice=9\r\n[section2]\r\nstartslice=8\r\nendslice=20\r\ntexture1.open=540103764\r\ntexturesize=20\r\n";
        let s = load_sections(text);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].open, [ENV0; 3]);
        assert_eq!((s[0].size, s[0].aspect), (6.0, 1.0));
        assert_eq!(select_section(&s, 8), Some(0));
        assert_eq!(select_section(&s, 10), Some(1));
        assert_eq!(select_section(&s, -1), None);
        assert_eq!(section_texture(&s, Some(1), false, 0), Some(0x2031_5454));
    }

    #[test]
    fn slot_thresholds() {
        assert_eq!(slot_for_param(0.0), 0);
        assert_eq!(slot_for_param(0.0499), 0);
        assert_eq!(slot_for_param(0.05), 1);
        assert_eq!(slot_for_param(0.9499), 1);
        assert_eq!(slot_for_param(0.95), 2);
        assert_eq!(slot_for_param(f32::NAN), 0);
    }
}

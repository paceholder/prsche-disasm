//! Port of the original player camera update (64 Hz) and view build.
//!
//! Sources (raw x86 in `nfs5.exe`; notes in `docs/re/camera.md` of the research workspace):
//! `Camera_UpdateTail 0x461e80` (TailCam0/1/2), `Camera_UpdateHeli 0x4623d0` (HeliCam0/1/2),
//! `Camera_UpdateBumper 0x461d50` (BumperCam), `Camera_UpdateCockpit 0x461a30` (BumperCam2),
//! `Camera_ApplyLookAngle 0x4611c0`, `Camera_LookAt 0x464bf0`, the view part of
//! `Camera_BuildView 0x464f80`, and the parameter table `g_CameraParams 0x5cf610`.
//!
//! Arithmetic follows the original x87 code: the game runs with control word 0x27f (53-bit
//! precision), so a chain of x87 operations is an `f64` chain rounded to `f32` where the
//! original stores. The vector transform `Vec_TransformByMatrix 0x532a40` dispatches to the
//! PACODE kernel 0x5b10b8, which switches to 24-bit precision: it is plain `f32` arithmetic.
//! Fixtures: `scripts/research/replay-camera.py` (`local/fixtures/camera-{tail,bumper}.tsv`, gitignored).
//!
//! Not ported here: the crash camera (0x461590), the pause orbit (0x4617c0), replay/TV
//! cameras, the replay-mode shake and up vector in the look-at (`[0x5e99f4] == 2`), the view
//! corrections of the view build (push out of objects/walls 0x4649e0/0x464500, keep above
//! ground 0x464330), head bob (flag 0x04), the mirror flag and the 1.2x FOV flag. The ceiling
//! clamp 0x461370 and the track-node lookup 0x4817d0 are inputs ([`CameraWorld`]).

/// World or car-local vector.
pub type Vec3 = [f32; 3];
/// 3x3 matrix as rows (right, up, forward).
pub type Mat3 = [Vec3; 3];

/// The camera update runs in the every-step scheduler queue (`0x414db0`, priority 100).
pub const UPDATE_HZ: u32 = 64;
/// Address of `g_CameraParams` in `nfs5.exe`.
pub const CAMERA_PARAMS_VA: u32 = 0x5cf610;

/// One `g_CameraParams` row: offset in car axes and flags (bit n -> record flag bit n+1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraParam {
    pub offset: Vec3,
    pub flags: u32,
}

const fn p(x: u32, y: u32, z: u32, flags: u32) -> CameraParam {
    CameraParam {
        offset: [f32::from_bits(x), f32::from_bits(y), f32::from_bits(z)],
        flags,
    }
}

/// `g_CameraParams 0x5cf610`, ids 0..19, copied from the exe bytes. Row 1 is replaced by the
/// player car's `.tpg` `drivercampos` at race init (`Camera_RaceInit 0x4640b0`).
pub const CAMERA_PARAMS: [CameraParam; 20] = [
    p(0x00000000, 0x3ecccccd, 0x3f666666, 0x02), // 0 BumperCam
    p(0x00000000, 0x3ecccccd, 0x3f666666, 0x00), // 1 BumperCam2
    p(0x00000000, 0x3f933333, 0xc0533333, 0x2d), // 2 TailCam0
    p(0x00000000, 0x3faccccd, 0xc0900000, 0x2d), // 3 TailCam1
    p(0x00000000, 0x3f933333, 0xc099999a, 0x2d), // 4 TailCam2
    p(0x00000000, 0x40000000, 0xc0c00000, 0x2d), // 5 HeliCam0
    p(0x00000000, 0x40000000, 0xc0c00000, 0x2d), // 6 HeliCam1
    p(0x00000000, 0x409ccccd, 0xc0d66666, 0x2d), // 7 HeliCam2
    p(0xbfe00000, 0x3e800000, 0xc0700000, 0x3c), // 8 ShotgunCam2
    p(0x00000000, 0x3fc3d70a, 0x40833333, 0x38), // 9 ShotgunCam3
    p(0x00000000, 0x00000000, 0x00000000, 0x3c), // 10 CircleCam
    p(0x00000000, 0x00000000, 0x00000000, 0x14), // 11 SplineCam
    p(0x00000000, 0x00000000, 0x00000000, 0x14), // 12 TVCam
    p(0x00000000, 0x41700000, 0xc0000000, 0x14), // 13 BlimpCam
    p(0x00000000, 0x00000000, 0x00000000, 0x10), // 14 AnimCam
    p(0x00000000, 0x3f800000, 0xbeff7cee, 0x12), // 15 ReverseCam1
    p(0x00000000, 0x00000000, 0x00000000, 0x18), // 16
    p(0x00000000, 0x00000000, 0x00000000, 0x18), // 17
    p(0x00000000, 0x00000000, 0x00000000, 0x14), // 18
    p(0x00000000, 0x00000000, 0x00000000, 0x14), // 19
];

/// Cockpit camera of a car model (`Car_ReadTpgCameraKeys 0x441a10`: `.tpg` `[header]`
/// `drivercampos` -> car `+0x10e8`, `drivercamrot` -> `+0x10f4`), in original car axes
/// (x right, y up, z forward; rotation in degrees). Copied into [`CAMERA_PARAMS`] row 1 and
/// the record's `+0x4c` at race init (`Camera_RaceInit 0x4640b0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriverCamera {
    pub position: Vec3,
    pub rotation_deg: Vec3,
}

impl DriverCamera {
    /// Defaults of `0x441a10` when `drivercampos`/`drivercamrot` are missing.
    pub const DEFAULT: Self = Self {
        position: [
            f32::from_bits(0xbeb851ec),
            f32::from_bits(0x3ecccccd),
            f32::from_bits(0xbe800000),
        ],
        rotation_deg: [0.0; 3],
    };
}

impl Default for DriverCamera {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The fields of the 0x10c-byte camera record (`g_Camera 0x627e80`) the ported updates use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraRecord {
    /// +0x08 smoothed eye offset from the car centre, world space.
    pub offset_08: Vec3,
    /// +0x14 aux point (car + rigid offset; look-behind target).
    pub aux_14: Vec3,
    /// +0x24 eye position.
    pub eye_24: Vec3,
    /// +0x4c cockpit rotation (`drivercamrot`, degrees).
    pub cockpit_rot_4c: Vec3,
    /// +0x58 half horizontal FOV in degrees.
    pub half_fov_58: f32,
    /// +0x5c near plane.
    pub near_5c: f32,
    /// +0x7c camera matrix, rows right/up/forward.
    pub matrix_7c: Mat3,
    /// +0xbc camera id (index into [`CAMERA_PARAMS`]).
    pub id_bc: i16,
    /// +0xc0 flags byte (table flags << 1).
    pub flags_c0: u8,
    /// +0xc1 settle counter (50 after a camera change).
    pub settle_c1: i8,
    /// +0xc6 look angle in degrees (cockpit cams).
    pub look_angle_c6: i16,
}

impl Default for CameraRecord {
    /// `Camera_Set 0x465790` defaults: FOV 37.5 (half), near 0.5.
    fn default() -> Self {
        Self {
            offset_08: [0.0; 3],
            aux_14: [0.0; 3],
            eye_24: [0.0; 3],
            cockpit_rot_4c: [0.0; 3],
            half_fov_58: 37.5,
            near_5c: 0.5,
            matrix_7c: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            id_bc: 2,
            flags_c0: 0x5a,
            settle_c1: 0,
            look_angle_c6: 0,
        }
    }
}

/// The car fields the camera reads (offsets in the car struct).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarView {
    /// +0x08 track node index.
    pub node_08: i16,
    /// +0x330 position.
    pub pos_330: Vec3,
    /// +0x364/+0x370/+0x37c body axes R, U, F.
    pub axes_364: Mat3,
    /// +0x3a8..+0x3b0 collision half extents (z = half length).
    pub half_extents_3a8: Vec3,
    /// +0xd30 sign of the travel direction along the node list (only `> 0` is tested).
    pub direction_d30: i32,
    /// +0xd58 ground-frame lateral speed (m/s).
    pub lateral_speed_d58: f32,
    /// +0xd60 ground-frame forward speed (m/s).
    pub forward_speed_d60: f32,
}

/// Globals read by the updates (all 0 in a normal single-player race).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraGlobals {
    /// Update gates: skip when `[0x606a98] != 0 && [0x606870] == 0`, or `[0x606ad4] != 0`.
    pub gate_606a98: i32,
    pub gate_606870: i32,
    pub gate_606ad4: i32,
    /// Game mode; with `[0x657408]` mode 3 keeps the TailCam1/2 lag factor at 0.1.
    pub game_mode_6573e8: i32,
    pub flag_657408: i32,
    /// Replay mode: `>= 2` sets the HeliCam lag factor to 0.01.
    pub replay_mode_5e99f4: i32,
    /// Flips the track-node step direction (xor with `car+0xd30 > 0`).
    pub direction_flag_657430: i32,
}

/// Held look keys (`g_LookForward/Right/Behind/Left`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LookKeys {
    pub forward_5e99a8: bool,
    pub right_5e99ac: bool,
    pub behind_5e99b0: bool,
    pub left_5e99b4: bool,
}

/// Track data the chase cameras query.
pub trait CameraWorld {
    /// `TrackNodes_Step 0x4817d0(node, delta, -1)`: node index `delta` steps along the track.
    fn node_step(&self, node: i32, delta: i32) -> i32;
    /// Node height (`[[0x628bc0]+0xc] + i*0x50 + 8`).
    fn node_y(&self, node: i32) -> f32;
    /// `Camera_ClampUnderCeiling 0x461370(player, pos, &dy)`: may lower `dy` (eye height
    /// above `car_pos`) under an overhead object near `pos`; returns whether it did.
    fn clamp_under_ceiling(&mut self, _car_pos: Vec3, _pos: Vec3, _dy: &mut f32) -> bool {
        false
    }
}

/// A track with one node section `0..y.len()` and no ceilings: `TrackNodes_Step` clamps to the
/// section ends (no connections), then to the node count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeStrip {
    pub y: Vec<f32>,
}

impl CameraWorld for NodeStrip {
    fn node_step(&self, node: i32, delta: i32) -> i32 {
        (node + delta).clamp(0, self.y.len() as i32 - 1)
    }
    fn node_y(&self, node: i32) -> f32 {
        self.y[node as usize]
    }
}

/// View built from the camera state (`Camera_BuildView 0x464f80`, view part).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    pub eye: Vec3,
    /// Rows right, up, forward (world space).
    pub matrix: Mat3,
    /// `eye + forward`.
    pub target: Vec3,
    /// Horizontal field of view in degrees (2 x `+0x58`).
    pub fov_deg: f32,
    /// Near plane (`+0x5c`, minus 0.35 for BumperCam2).
    pub near: f32,
}

// ---------------------------------------------------------------------------
// Constants (f32 bit patterns from .rdata).
const K_01: f32 = f32::from_bits(0x3dcccccd); // 0.1
const K_005: f32 = f32::from_bits(0x3d4ccccd); // 0.05
const K_002: f32 = f32::from_bits(0x3ca3d70a); // 0.02
const K_001: f32 = f32::from_bits(0x3c23d70a); // 0.01
const SPEED_SCALE: f32 = f32::from_bits(0x3a91a2b4); // 1/900
const THIRD: f32 = f32::from_bits(0x3eaaaaab);
const TAIL_MIN_DIST: f32 = f32::from_bits(0x409e147b); // 4.94
const HELI_MIN_DIST: f32 = f32::from_bits(0x40ca3d71); // 6.32
const LIFT_TAIL: f32 = f32::from_bits(0x3ea66666); // 0.325
const LIFT_OTHER: f32 = f32::from_bits(0x3ecccccd); // 0.4
const UP_LIMIT: f32 = f32::from_bits(0x3f7d70a4); // 0.99
const PER_DEGREE: f32 = f32::from_bits(0x3b360b61); // 1/360 turn
const K_02: f32 = f32::from_bits(0x3e4ccccd); // 0.2
/// `[0x5bb2c0]`, the double 2*pi used by `0x5327e0/0x5327f0`.
const TWO_PI: f64 = f64::from_bits(0x401921fb20000000);

#[inline]
fn d(x: f32) -> f64 {
    x as f64
}

/// `Vec_TransformByMatrix 0x532a40` -> PACODE 0x5b10b8 (24-bit precision, + zero vector).
pub(crate) fn transform(v: Vec3, m: &Mat3) -> Vec3 {
    let mut out = [0.0; 3];
    for (i, o) in out.iter_mut().enumerate() {
        *o = ((v[0] * m[0][i] + v[1] * m[1][i]) + v[2] * m[2][i]) + 0.0;
    }
    out
}

/// `0x532300` / `0x56e660`.
pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `0x532b20` (left on the x87 stack, not rounded).
fn length(v: Vec3) -> f64 {
    ((d(v[0]) * d(v[0]) + d(v[1]) * d(v[1])) + d(v[2]) * d(v[2])).sqrt()
}

/// `0x532910` -> `0x56e8d1`.
fn dot(a: Vec3, b: Vec3) -> f64 {
    (d(a[0]) * d(b[0]) + d(a[1]) * d(b[1])) + d(a[2]) * d(b[2])
}

/// `0x532880` -> `0x56e8f5`: a x b.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        (d(a[1]) * d(b[2]) - d(a[2]) * d(b[1])) as f32,
        (d(a[2]) * d(b[0]) - d(a[0]) * d(b[2])) as f32,
        (d(a[0]) * d(b[1]) - d(a[1]) * d(b[0])) as f32,
    ]
}

/// `0x5328d0`.
fn normalize(v: Vec3) -> Vec3 {
    let inv = 1.0 / length(v);
    [
        (d(v[0]) * inv) as f32,
        (d(v[1]) * inv) as f32,
        (d(v[2]) * inv) as f32,
    ]
}

/// `0x532800`: (sin, cos) of `turns` (f32 turns x double 2*pi).
fn sin_cos(turns: f32) -> (f32, f32) {
    let a = d(turns) * TWO_PI;
    (a.sin() as f32, a.cos() as f32)
}

fn flat(m: &Mat3) -> [f64; 9] {
    let mut o = [0.0; 9];
    for (i, v) in m.iter().flatten().enumerate() {
        o[i] = d(*v);
    }
    o
}

fn unflat(v: [f64; 9]) -> Mat3 {
    [
        [v[0] as f32, v[1] as f32, v[2] as f32],
        [v[3] as f32, v[4] as f32, v[5] as f32],
        [v[6] as f32, v[7] as f32, v[8] as f32],
    ]
}

/// `0x532470(a, b, out)`: out = a * b (rows), with the original summation order.
fn mat_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let (a, b) = (flat(a), flat(b));
    unflat([
        (b[3] * a[1] + b[6] * a[2]) + b[0] * a[0],
        (a[2] * b[7] + b[4] * a[1]) + b[1] * a[0],
        (a[2] * b[8] + a[1] * b[5]) + b[2] * a[0],
        (b[6] * a[5] + a[4] * b[3]) + a[3] * b[0],
        (b[1] * a[3] + b[4] * a[4]) + b[7] * a[5],
        (b[8] * a[5] + a[4] * b[5]) + b[2] * a[3],
        (b[6] * a[8] + b[3] * a[7]) + a[6] * b[0],
        (a[8] * b[7] + b[4] * a[7]) + a[6] * b[1],
        (b[8] * a[8] + a[6] * b[2]) + b[5] * a[7],
    ])
}

/// `0x532600`: rotation about y by `turns`.
fn rot_y_matrix(turns: f32) -> Mat3 {
    let (s, c) = sin_cos(turns);
    [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
}

/// `0x5563d0(in, turns, out)`: rotate rows up/forward about the right axis.
fn rotate_x(m: &Mat3, turns: f32) -> Mat3 {
    let (s, c) = sin_cos(turns);
    let (s, c, i) = (d(s), d(c), flat(m));
    let mut o = flat(m);
    o[3] = i[3] * c + i[6] * s;
    o[4] = i[7] * s + i[4] * c;
    o[5] = i[8] * s + i[5] * c;
    o[6] = i[6] * c - i[3] * s;
    o[7] = i[7] * c - i[4] * s;
    o[8] = i[8] * c - i[5] * s;
    unflat(o)
}

/// `0x5564a0(in, turns, out)`: rotate rows right/forward about the up axis.
fn rotate_y(m: &Mat3, turns: f32) -> Mat3 {
    let (s, c) = sin_cos(turns);
    let (s, c, i) = (d(s), d(c), flat(m));
    let mut o = flat(m);
    o[0] = c * i[0] - i[6] * s;
    o[1] = i[1] * c - i[7] * s;
    o[2] = i[2] * c - i[8] * s;
    o[6] = i[6] * c + s * i[0];
    o[7] = i[1] * s + i[7] * c;
    o[8] = i[2] * s + i[8] * c;
    unflat(o)
}

/// `0x556560(in, turns, out)`: rotate rows right/up about the forward axis.
fn rotate_z(m: &Mat3, turns: f32) -> Mat3 {
    let (s, c) = sin_cos(turns);
    let (s, c, i) = (d(s), d(c), flat(m));
    let mut o = flat(m);
    o[0] = i[3] * s + c * i[0];
    o[1] = i[4] * s + i[1] * c;
    o[2] = i[5] * s + i[2] * c;
    o[3] = i[3] * c - s * i[0];
    o[4] = i[4] * c - i[1] * s;
    o[5] = i[5] * c - i[2] * s;
    unflat(o)
}

fn gated(g: &CameraGlobals) -> bool {
    (g.gate_606a98 != 0 && g.gate_606870 == 0) || g.gate_606ad4 != 0
}

/// `max(|vl|, |vf|) + 0.25 * min(|vl|, |vf|)` as the x87 code computes it.
fn ground_speed(car: &CarView) -> f64 {
    let vf = d(car.forward_speed_d60).abs();
    let vl = d(car.lateral_speed_d58).abs();
    if vl <= vf {
        vf + vl * 0.25
    } else {
        vf * 0.25 + vl
    }
}

/// Speed-scaled lag factor of TailCam1/2 and HeliCam1/2.
fn speed_factor(car: &CarView, kmax: f32) -> f32 {
    let v = ground_speed(car) * d(SPEED_SCALE);
    let v = if d(kmax) > v { v } else { d(kmax) };
    if d(K_002) > v {
        K_002
    } else {
        v as f32
    }
}

/// Slope clamp: `upper <= s` -> upper, `lower > s` -> lower, else s through an f32 slot.
fn clamp_slope(s: f64, lower: f32, upper: f32) -> f64 {
    if d(upper) <= s {
        d(upper)
    } else if d(lower) > s {
        d(lower)
    } else {
        d(s as f32)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chase {
    Tail,
    Heli,
}

/// `Camera_UpdateTail 0x461e80(player, variant)`: TailCam0/1/2 (ids 2/3/4, variant 0/1/2).
pub fn update_tail(
    rec: &mut CameraRecord,
    car: &CarView,
    world: &mut impl CameraWorld,
    g: &CameraGlobals,
    keys: &LookKeys,
    params: &[CameraParam],
    variant: i32,
) {
    chase(rec, car, world, g, keys, params, variant, Chase::Tail);
}

/// `Camera_UpdateHeli 0x4623d0(player, variant)`: HeliCam0/1/2 (ids 5/6/7, variant 0/1/2).
pub fn update_heli(
    rec: &mut CameraRecord,
    car: &CarView,
    world: &mut impl CameraWorld,
    g: &CameraGlobals,
    keys: &LookKeys,
    params: &[CameraParam],
    variant: i32,
) {
    chase(rec, car, world, g, keys, params, variant, Chase::Heli);
}

#[allow(clippy::too_many_arguments)]
fn chase(
    rec: &mut CameraRecord,
    car: &CarView,
    world: &mut impl CameraWorld,
    g: &CameraGlobals,
    keys: &LookKeys,
    params: &[CameraParam],
    variant: i32,
    kind: Chase,
) {
    let row = params[rec.id_bc as usize].offset;
    let step = if ((car.direction_d30 > 0) as i32 ^ g.direction_flag_657430) != 0 {
        3
    } else {
        -3
    };
    if gated(g) {
        return;
    }
    let mut local = row;
    if kind == Chase::Tail {
        local[2] = row[2] - car.half_extents_3a8[2];
    }
    // k: horizontal lag factor; ky: vertical.
    let ky = K_005;
    let mut k = K_01;
    if variant != 0 {
        let kmax = if variant == 2 { K_005 } else { K_01 };
        let fixed = kind == Chase::Tail && g.game_mode_6573e8 == 3 && g.flag_657408 != 0;
        if !fixed {
            k = speed_factor(car, kmax);
        }
    }
    if kind == Chase::Heli && g.replay_mode_5e99f4 >= 2 {
        k = K_001;
    }
    if keys.behind_5e99b0 {
        rec.offset_08 = transform(local, &car.axes_364);
    }
    if kind == Chase::Heli {
        let pull = ground_speed(car) * d(K_005);
        let pull = if pull <= 2.0 { pull } else { 2.0 };
        local[2] = (d(row[2]) - pull) as f32;
    }
    // Slope from the track nodes `step` ahead and back.
    let n1 = world.node_step(car.node_08 as i32, step);
    let y1 = world.node_y(n1);
    let n2 = world.node_step(n1, -step);
    let y2 = world.node_y(n2);
    let s = match kind {
        Chase::Tail => (d(y1) - d(y2)) * d(THIRD),
        Chase::Heli => (d(y1) - d(y2)) * 0.5,
    };
    let s = match (kind, variant) {
        (_, 0) => 0.0,
        (Chase::Tail, 1) => clamp_slope(s, 0.25, 1.0),
        (Chase::Tail, 2) => clamp_slope(s, -0.25, 2.0),
        (Chase::Heli, 1) => clamp_slope(s, 0.25, 1.25),
        (Chase::Heli, 2) => clamp_slope(s, -0.75, 3.0),
        _ => s,
    };
    local[1] = (d(local[1]) + s) as f32;
    let pos = car.pos_330;
    if keys.behind_5e99b0 {
        // Rigid: aux = car + offset, eye at the mirrored front point.
        rec.aux_14 = add(pos, transform(local, &car.axes_364));
        local[2] = -local[2];
        let t = transform(local, &car.axes_364);
        let y = clamped_height(world, pos, rec.eye_24, t[1]);
        rec.eye_24 = [t[0] + pos[0], y + pos[1], t[2] + pos[2]];
        return;
    }
    let mut t = transform(local, &car.axes_364);
    t[1] = clamped_height(world, pos, rec.eye_24, t[1]);
    let o = rec.offset_08;
    let (k, ky) = if rec.settle_c1 > 0 {
        rec.settle_c1 -= 1;
        (K_01, K_01)
    } else {
        (k, ky)
    };
    let mut o = [
        ((d(t[0]) - d(o[0])) * d(k) + d(o[0])) as f32,
        ((d(t[1]) - d(o[1])) * d(ky) + d(o[1])) as f32,
        ((d(t[2]) - d(o[2])) * d(k) + d(o[2])) as f32,
    ];
    if rec.settle_c1 == 0 {
        let min = match kind {
            Chase::Tail => TAIL_MIN_DIST,
            Chase::Heli => HELI_MIN_DIST,
        };
        let len = length(o);
        if len < d(min) {
            let scale = (d(min) / len) as f32;
            o = [o[0] * scale, o[1] * scale, o[2] * scale];
        }
    }
    rec.offset_08 = o;
    rec.eye_24 = add(pos, o);
}

/// Ceiling clamp at the car and at the current eye; the smaller height wins.
pub(crate) fn clamped_height(world: &mut impl CameraWorld, pos: Vec3, eye: Vec3, dy: f32) -> f32 {
    let (mut at_car, mut at_eye) = (dy, dy);
    world.clamp_under_ceiling(pos, pos, &mut at_car);
    world.clamp_under_ceiling(pos, eye, &mut at_eye);
    if at_car < at_eye {
        at_car
    } else {
        at_eye
    }
}

/// `Camera_ApplyLookAngle 0x4611c0`: moves `+0xc6` a quarter of the way to the look target
/// (the short way round) and rotates the matrix by it about y. Returns early, without the
/// rotation, when the angle already equals the target.
fn apply_look_angle(rec: &mut CameraRecord, keys: &LookKeys) {
    let mut target: i32 = 0;
    if keys.left_5e99b4 {
        target = -90;
    }
    if keys.right_5e99ac {
        target = 90;
    }
    if keys.behind_5e99b0 {
        target = 180 - (target as i16 as i32) / 2;
    }
    if keys.forward_5e99a8 {
        target = (target as i16 as i32) / 2;
    } else if !keys.right_5e99ac && !keys.left_5e99b4 && !keys.behind_5e99b0 {
        target = 0;
    }
    let cur = rec.look_angle_c6;
    if target as i16 == cur {
        return;
    }
    if (target as i16) < 0 {
        target += 360;
    }
    let si = target as i16;
    let mut bp = si.wrapping_add(180);
    if bp >= 360 {
        bp -= 360;
    }
    let quarter = |x: i32| (x / 4).abs();
    let diff = si as i32 - cur as i32;
    let step = quarter(diff);
    let store = if si < 180 {
        if cur > si {
            if cur <= bp {
                Some(cur as i32 - step)
            } else {
                Some(cur as i32 + quarter(diff + 360))
            }
        } else if cur < si {
            Some(cur as i32 + step)
        } else {
            None
        }
    } else if cur > si {
        Some(cur as i32 - step)
    } else if cur < si {
        if cur > bp {
            Some(cur as i32 + step)
        } else {
            Some(cur as i32 - quarter(diff - 360))
        }
    } else {
        None
    };
    if let Some(v) = store {
        rec.look_angle_c6 = v as i16;
    }
    let a = rec.look_angle_c6;
    if a > 360 {
        rec.look_angle_c6 = a - 360;
    } else if a < 0 {
        rec.look_angle_c6 = a + 360;
    }
    let turns = (d(rec.look_angle_c6 as f32) * d(PER_DEGREE)) as f32;
    rec.matrix_7c = mat_mul(&rot_y_matrix(turns), &rec.matrix_7c);
}

/// `Camera_UpdateBumper 0x461d50`: BumperCam, rigid (matrix = car axes, look angle applied).
pub fn update_bumper(
    rec: &mut CameraRecord,
    car: &CarView,
    g: &CameraGlobals,
    keys: &LookKeys,
    params: &[CameraParam],
) {
    let row = params[rec.id_bc as usize].offset;
    if gated(g) {
        return;
    }
    rec.matrix_7c = car.axes_364;
    rec.aux_14 = add(car.pos_330, transform(row, &car.axes_364));
    apply_look_angle(rec, keys);
    rec.eye_24 = add(transform(row, &car.axes_364), car.pos_330);
}

/// `Camera_UpdateCockpit 0x461a30`: BumperCam2 (offset = table row 1 = `.tpg drivercampos`,
/// matrix rotated by `+0x4c drivercamrot`, offset lerp 0.2/0.2/0.5; rigid while looking
/// left or right; look-behind zeroes the x offset).
pub fn update_bumper2(
    rec: &mut CameraRecord,
    car: &CarView,
    g: &CameraGlobals,
    keys: &LookKeys,
    params: &[CameraParam],
) {
    let mut row = params[rec.id_bc as usize].offset;
    if gated(g) {
        return;
    }
    let pos = car.pos_330;
    rec.matrix_7c = car.axes_364;
    rec.aux_14 = add(pos, transform(row, &car.axes_364));
    apply_look_angle(rec, keys);
    if keys.left_5e99b4 || keys.right_5e99ac {
        if keys.behind_5e99b0 {
            row[0] = 0.0;
        }
        rec.eye_24 = add(transform(row, &car.axes_364), pos);
        return;
    }
    let rot = rec.cockpit_rot_4c;
    if rot != [0.0; 3] {
        let turns = |deg: f32| (d(deg) * d(PER_DEGREE)) as f32;
        rec.matrix_7c = rotate_x(&rec.matrix_7c, turns(rot[0]));
        rec.matrix_7c = rotate_y(&rec.matrix_7c, turns(rot[1]));
        rec.matrix_7c = rotate_z(&rec.matrix_7c, turns(rot[2]));
    }
    if keys.behind_5e99b0 {
        row[0] = 0.0;
    }
    let t = transform(row, &car.axes_364);
    let o = rec.offset_08;
    let lerp = |t: f32, o: f32, k: f32| ((d(t) - d(o)) * d(k) + d(o)) as f32;
    rec.offset_08 = [
        lerp(t[0], o[0], K_02),
        lerp(t[1], o[1], K_02),
        lerp(t[2], o[2], 0.5),
    ];
    rec.eye_24 = add(pos, rec.offset_08);
}

/// `Camera_LookAt 0x464bf0(player, NULL, eye, matrix)` outside replay mode: aim at `target`
/// (the looked-at car's position) + lift (flag 0x02: 1.3 m for ids 2..4, 1.6 m otherwise),
/// world up as reference (the previous up row if `|up . fwd| > 0.99`). Writes `+0x7c`.
pub fn look_at(rec: &mut CameraRecord, target: Vec3, eye: Vec3) {
    let mut fwd = [
        ((d(target[0]) - d(eye[0])) * 0.25) as f32,
        ((d(target[1]) - d(eye[1])) * 0.25) as f32,
        ((d(target[2]) - d(eye[2])) * 0.25) as f32,
    ];
    if rec.flags_c0 & 0x02 != 0 {
        let lift = if (2..=4).contains(&rec.id_bc) {
            LIFT_TAIL
        } else {
            LIFT_OTHER
        };
        fwd[1] = (d(lift) + d(fwd[1])) as f32;
    }
    let fwd = normalize(fwd);
    let mut up = [0.0, 1.0, 0.0];
    if dot(up, fwd).abs() > d(UP_LIMIT) {
        up = rec.matrix_7c[1];
    }
    let right = normalize(cross(up, fwd));
    let up = normalize(cross(fwd, right));
    rec.matrix_7c = [right, up, fwd];
}

/// View part of `Camera_BuildView 0x464f80` for the main view: eye = `+0x24`, look-at when
/// flag 0x08 is set (writes the record matrix like the original), FOV = 2 x `+0x58`, near =
/// `+0x5c` (minus 0.35 for BumperCam2). `target` is the looked-at car's position.
pub fn build_view(rec: &mut CameraRecord, target: Vec3) -> CameraView {
    let eye = rec.eye_24;
    if rec.flags_c0 & 0x08 != 0 {
        look_at(rec, target, eye);
    }
    let near = if rec.id_bc == 1 {
        rec.near_5c - 0.35
    } else {
        rec.near_5c
    };
    let f = rec.matrix_7c[2];
    CameraView {
        eye,
        matrix: rec.matrix_7c,
        target: add(eye, f),
        fov_deg: rec.half_fov_58 * 2.0,
        near,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_match_the_documented_table() {
        assert_eq!(CAMERA_PARAMS[2].offset, [0.0, 1.15, -3.3]);
        assert_eq!(CAMERA_PARAMS[7].offset, [0.0, 4.9, -6.7]);
        assert_eq!(CAMERA_PARAMS[0].flags, 0x02);
        assert_eq!(CAMERA_PARAMS[4].flags << 1, 0x5a);
    }

    #[test]
    fn build_view_projection() {
        let mut rec = CameraRecord {
            eye_24: [0.0, 1.65, -5.4],
            ..CameraRecord::default()
        };
        let view = build_view(&mut rec, [0.0, 0.5, 0.0]);
        assert_eq!((view.fov_deg, view.near), (75.0, 0.5));
        assert_eq!(view.matrix, rec.matrix_7c);
        assert!(view.matrix[2][2] > 0.9 && view.matrix[0][1] == 0.0);
        let mut cockpit = CameraRecord {
            id_bc: 1,
            flags_c0: 0,
            ..CameraRecord::default()
        };
        let view = build_view(&mut cockpit, [0.0; 3]);
        assert!((view.near - 0.15).abs() < 1e-6);
        assert_eq!(view.matrix, CameraRecord::default().matrix_7c);
    }

    #[test]
    fn look_angle_converges_short_of_the_target() {
        let mut rec = CameraRecord::default();
        let keys = LookKeys {
            right_5e99ac: true,
            ..LookKeys::default()
        };
        for _ in 0..100 {
            apply_look_angle(&mut rec, &keys);
        }
        // Quarter steps truncate: the angle stops 3 degrees short and keeps rotating.
        assert_eq!(rec.look_angle_c6, 87);
    }
}

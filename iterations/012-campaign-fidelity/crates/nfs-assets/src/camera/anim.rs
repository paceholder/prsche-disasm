//! Race-intro camera: the AnimCam (camera id 14) fly-around of the player car.
//!
//! Sources (raw x86 in `nfs5.exe`; notes in `docs/re/race-intro.md` of the research workspace):
//! - Data: every track CRP holds five `CmAn` entries (`camera00`..`camera04`), loaded in table
//!   order by `CamAnimList_LoadTrack 0x471850` into the list `[0x628938]` (created by
//!   `CamAnimList_Init 0x471700(&g_SchedStep, 1/64)` from the track loader 0x4876d0).
//!   Entry: name (0x24 bytes), f32 +0x24 (0.1, unused by the player), u32 +0x28 key count
//!   (121), keys from +0x2c, 0x34 bytes each: quaternion `x, y, z, w`, position, f32 (1.0),
//!   scale, f32, f32.
//! - `CamAnimList_NewPlayer 0x471a50(0)`: animation `index = (index + 1) % count`, the index
//!   `[0x62893c]` is static (never reset), so successive races cycle through the five.
//!   The player samples at 28 keys/s (`0x3d124925` = 1/28 s per key) on the 64 Hz
//!   `g_SchedStep` clock from the step it was created.
//! - `CamAnim_Sample 0x470e90` / `CamAnim_Pose 0x470f70`: key `trunc(t / (64/28))`, linear
//!   blend of the quaternions (not normalised; 0x48a150) and positions, quaternion matrix
//!   0x48a1e0, scale 0x5329a0. Finished (no loop flag) once the key reaches `count - 2`:
//!   273 steps (4.27 s) for 121 keys.
//! - `Camera_UpdateAnim 0x462f30`: eye = car `+0x330` + car axes x key position; matrix =
//!   90 degrees about x (`0x539bd0`) x key rotation x car axes, orthonormalised
//!   (`Mat3_Orthonormalize 0x4a4810`). When finished: `Camera_Set(player, list[0])` and one
//!   more `Camera_UpdateAll` in the same step.
//!
//! Arithmetic follows the x87 code at 53-bit precision (`f64` chains rounded to `f32` where
//! the original stores); the vector transform is the 24-bit PACODE kernel (`transform`).
//! Fixture: `scripts/research/replay-camera-anim.py` (`local/fixtures/camera-anim.tsv`).

use super::original::{add, d, mat_mul, transform, CameraRecord, CarView, Mat3, Vec3};

/// `CmAn` table tag of the camera animations in a track CRP.
pub const CMAN_TAG: &str = "CmAn";
/// Seconds per key used by the player (`0x471a50`: `0x3d124925`, 1/28 s).
pub const KEY_SECONDS: f32 = f32::from_bits(0x3d124925);
/// The list's clock step (`0x471700(&g_SchedStep, 1/64)`).
pub const STEP_SECONDS: f32 = f32::from_bits(0x3c800000);
/// Size of one key record.
pub const KEY_SIZE: usize = 0x34;
const HEADER_SIZE: usize = 0x2c;

/// One key of a `CmAn` animation (car-local camera pose).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimKey {
    /// +0x00 rotation quaternion `x, y, z, w`.
    pub quat: [f32; 4],
    /// +0x10 position in car axes (x right, y up, z forward).
    pub pos: Vec3,
    /// +0x20 scale of the rotation rows.
    pub scale: Vec3,
}

/// A `CmAn` camera animation of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraAnimation {
    pub name: String,
    /// +0x24, 0.1 in the shipped data; the player ignores it ([`KEY_SECONDS`]).
    pub key_time: f32,
    pub keys: Vec<AnimKey>,
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl CameraAnimation {
    /// Parses the data of one `CmAn` table entry.
    pub fn parse(chunk: &[u8]) -> Result<Self, String> {
        if chunk.len() < HEADER_SIZE {
            return Err(format!("CmAn entry too short: {} bytes", chunk.len()));
        }
        let name_end = chunk[..0x24].iter().position(|&c| c == 0).unwrap_or(0x24);
        let name = String::from_utf8_lossy(&chunk[..name_end]).into_owned();
        let key_time = f32_at(chunk, 0x24);
        let count =
            u32::from_le_bytes([chunk[0x28], chunk[0x29], chunk[0x2a], chunk[0x2b]]) as usize;
        if count < 2 || HEADER_SIZE + count * KEY_SIZE > chunk.len() {
            return Err(format!(
                "CmAn {name}: {count} keys do not fit {} bytes",
                chunk.len()
            ));
        }
        let keys = (0..count)
            .map(|k| {
                let o = HEADER_SIZE + k * KEY_SIZE;
                AnimKey {
                    quat: [0, 4, 8, 12].map(|i| f32_at(chunk, o + i)),
                    pos: [0x10, 0x14, 0x18].map(|i| f32_at(chunk, o + i)),
                    scale: [0x20, 0x24, 0x28].map(|i| f32_at(chunk, o + i)),
                }
            })
            .collect();
        Ok(Self {
            name,
            key_time,
            keys,
        })
    }

    /// Steps (64 Hz) the animation plays: the first elapsed step count it is finished at.
    pub fn duration_steps(&self) -> i32 {
        (0..i32::MAX)
            .find(|&t| key_at(self.keys.len(), t).is_none())
            .unwrap_or(i32::MAX)
    }

    /// `CamAnim_Sample 0x470e90` (no loop flag): rotation rows and position of the camera in
    /// car axes, `elapsed` steps after the player was created; `None` once finished.
    pub fn sample(&self, elapsed: i32) -> Option<(Mat3, Vec3)> {
        let (i, frac) = key_at(self.keys.len(), elapsed)?;
        Some(pose(&self.keys[i], &self.keys[i + 1], frac))
    }
}

/// `CamAnimList_NewPlayer 0x471a50(0)`: the next index after `last` (`[0x62893c]`).
pub fn next_index(last: u32, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        ((last as usize).wrapping_add(1)) % count
    }
}

/// Key index and blend factor at `elapsed` steps (`0x470e90`), `None` when finished.
fn key_at(count: usize, elapsed: i32) -> Option<(usize, f32)> {
    let steps_per_key = (d(KEY_SECONDS) / d(STEP_SECONDS)) as f32;
    let t = elapsed.max(0) as f64;
    let f = t / d(steps_per_key);
    let idx = f.trunc();
    let frac = (f - idx) as f32;
    let idx = idx as usize;
    if count < 2 || idx >= count - 2 {
        return None;
    }
    Some((idx, frac))
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    ((d(b) - d(a)) * d(t) + d(a)) as f32
}

/// `0x48a1e0`: quaternion `x, y, z, w` to rotation rows.
fn quat_matrix(q: [f32; 4]) -> Mat3 {
    let [x, y, z, w] = q.map(d);
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let r = |v: f64| v as f32 as f64;
    let wx = r(x2 * w);
    let wy = r(y2 * w);
    let wz = r(z2 * w);
    let xx = x2 * x;
    let xy = y2 * x;
    let xz = r(z2 * x);
    let yy = r(y2 * y);
    let yz = r(z2 * y);
    let zz = r(z2 * z);
    let one_xx = 1.0 - xx;
    [
        [((1.0 - yy) - zz) as f32, (xy + wz) as f32, (xz - wy) as f32],
        [(xy - wz) as f32, (one_xx - zz) as f32, (yz + wx) as f32],
        [(xz + wy) as f32, (yz - wx) as f32, (one_xx - yy) as f32],
    ]
}

/// `CamAnim_Pose 0x470f70` without the flag-8/16 spins (the intro player's flags are 0).
fn pose(a: &AnimKey, b: &AnimKey, frac: f32) -> (Mat3, Vec3) {
    let q = [0, 1, 2, 3].map(|k| lerp(a.quat[k], b.quat[k], frac));
    let p = [0, 1, 2].map(|k| lerp(a.pos[k], b.pos[k], frac));
    let r = quat_matrix(q);
    let keep = 1.0 - d(frac);
    let s = [0, 1, 2].map(|k| (keep * d(a.scale[k]) + d(frac) * d(b.scale[k])) as f32);
    let m = [0, 1, 2].map(|row| r[row].map(|v| (d(v) * d(s[row])) as f32));
    // The zero drift vector of the player (+0x14) is added to the translation.
    (m, p.map(|v| v + 0.0))
}

/// Rows of `0x539bd0(90 degrees, (1, 0, 0))`.
const C90: f32 = f32::from_bits(0x345110b4);
const ROT_X_90: Mat3 = [[1.0, 0.0, 0.0], [0.0, C90, 1.0], [0.0, -1.0, C90]];
/// `[0x5d1040]` series coefficients of `Mat3_Orthonormalize 0x4a4810`.
const ORTHO_COEFFS: [f32; 3] = [-0.125, 0.09375, -0.078125];
const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn transpose(m: &Mat3) -> Mat3 {
    [0, 1, 2].map(|r| [m[0][r], m[1][r], m[2][r]])
}

/// `Mat3_Orthonormalize 0x4a4810`: four passes of `M = M * S`, `S` a series in
/// `E = M^T M - I`.
pub fn orthonormalize(m: &Mat3) -> Mat3 {
    let mut m = *m;
    for _ in 0..4 {
        let p = mat_mul(&transpose(&m), &m);
        let e = [0, 1, 2].map(|r| [0, 1, 2].map(|c| (d(p[r][c]) - d(IDENTITY[r][c])) as f32));
        let mut x = IDENTITY;
        let mut s = IDENTITY;
        for c in ORTHO_COEFFS {
            x = mat_mul(&x, &e);
            for k in 0..9 {
                let (r, col) = (k / 3, k % 3);
                let prod = d(x[r][col]) * d(c);
                let prod = if k == 8 { prod } else { prod as f32 as f64 };
                s[r][col] = (d(s[r][col]) + prod) as f32;
            }
        }
        m = mat_mul(&m, &s);
    }
    m
}

/// `Camera_UpdateAnim 0x462f30` while the animation plays: eye `+0x24` and matrix `+0x7c`
/// from the key pose in the car frame. Returns `false` (record untouched) once finished.
pub fn update_anim(
    rec: &mut CameraRecord,
    car: &CarView,
    anim: &CameraAnimation,
    elapsed: i32,
) -> bool {
    let Some((rot, pos)) = anim.sample(elapsed) else {
        return false;
    };
    rec.eye_24 = add(car.pos_330, transform(pos, &car.axes_364));
    let m = mat_mul(&rot, &car.axes_364);
    rec.matrix_7c = orthonormalize(&mat_mul(&ROT_X_90, &m));
    true
}

/// A `CmAn` entry for tests: a half turn about y around the car, radius 3 m, 0.5 m up.
#[cfg(test)]
pub(crate) fn synthetic_chunk(count: usize) -> Vec<u8> {
    let mut b = vec![0u8; HEADER_SIZE + count * KEY_SIZE + 4];
    b[..8].copy_from_slice(b"camera00");
    b[0x24..0x28].copy_from_slice(&0.1f32.to_le_bytes());
    b[0x28..0x2c].copy_from_slice(&(count as u32).to_le_bytes());
    for k in 0..count {
        let o = HEADER_SIZE + k * KEY_SIZE;
        // Half turn about y over the animation, radius 3 m, 0.5 m up.
        let a = std::f32::consts::PI * k as f32 / (count - 1) as f32;
        let (s, c) = (a * 0.5).sin_cos();
        for (i, v) in [
            0.0,
            s,
            0.0,
            c,
            3.0 * a.sin(),
            0.5,
            3.0 * a.cos(),
            1.0,
            1.0,
            1.0,
            1.0,
        ]
        .into_iter()
        .enumerate()
        {
            b[o + 4 * i..o + 4 * i + 4].copy_from_slice(&f32::to_le_bytes(v));
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(count: usize) -> Vec<u8> {
        synthetic_chunk(count)
    }

    #[test]
    fn parses_and_times_like_the_original() {
        let anim = CameraAnimation::parse(&synthetic(121)).unwrap();
        assert_eq!(anim.name, "camera00");
        assert_eq!(anim.keys.len(), 121);
        // 28 keys/s on the 64 Hz clock: key 119 (= count - 2) is reached at step 273.
        assert_eq!(anim.duration_steps(), 273);
        assert!(anim.sample(272).is_some());
        assert!(anim.sample(273).is_none());
        // Key positions are hit exactly at multiples of 64/28 steps (step 0 = key 0).
        let (_, p0) = anim.sample(0).unwrap();
        assert_eq!(p0, anim.keys[0].pos);
        assert!(CameraAnimation::parse(&synthetic(121)[..100]).is_err());
    }

    #[test]
    fn index_cycles_through_the_list() {
        assert_eq!(next_index(0, 5), 1);
        assert_eq!(next_index(4, 5), 0);
        assert_eq!(next_index(7, 0), 0);
    }

    #[test]
    fn eye_follows_the_car_frame_and_matrix_is_orthonormal() {
        let anim = CameraAnimation::parse(&synthetic(121)).unwrap();
        let yaw: f32 = 0.6;
        let (s, c) = yaw.sin_cos();
        let car = CarView {
            node_08: 0,
            pos_330: [10.0, 2.0, -5.0],
            axes_364: [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
            half_extents_3a8: [0.9, 0.6, 2.1],
            direction_d30: 1,
            lateral_speed_d58: 0.0,
            forward_speed_d60: 0.0,
        };
        let mut rec = CameraRecord::default();
        for t in [0, 50, 136, 272] {
            assert!(update_anim(&mut rec, &car, &anim, t));
            let (_, p) = anim.sample(t).unwrap();
            let rel: Vec<f32> = (0..3).map(|k| rec.eye_24[k] - car.pos_330[k]).collect();
            let local = [0, 1, 2].map(|a| (0..3).map(|k| rel[k] * car.axes_364[a][k]).sum::<f32>());
            for k in 0..3 {
                assert!((local[k] - p[k]).abs() < 1e-4, "t {t}: {local:?} vs {p:?}");
            }
            for a in 0..3 {
                for b in 0..3 {
                    let dot: f32 = (0..3)
                        .map(|k| rec.matrix_7c[a][k] * rec.matrix_7c[b][k])
                        .sum();
                    let want = if a == b { 1.0 } else { 0.0 };
                    // The blended quaternion is not normalised and the four series passes
                    // of 0x4a4810 leave a small residual (as in the original).
                    assert!((dot - want).abs() < 1e-4, "t {t} rows {a} {b}: {dot}");
                }
            }
        }
        assert!(!update_anim(&mut rec, &car, &anim, 273));
    }
}

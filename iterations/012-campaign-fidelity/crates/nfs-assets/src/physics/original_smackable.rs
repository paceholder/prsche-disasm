//! Smackable objects (`kSmackable`: signs, cones, posts, bins, trucks): the
//! original runs each of them as a flag-0x80 "object car" in the car list
//! (`docs/re/track-objects.md` section 7). They sleep (`+0x7d`) until a real
//! car is within 10 m, take part in the car-pair collision when awake, are
//! woken for physics by the first hit (`+0x7c`) and then slide and tumble on
//! the support under the car callbacks `Car_ObjectDynamicsTick` 0x4a1030,
//! `Car_GroundContactTick` 0x49bd40 (object branch with 0x4998c0),
//! `Car_IntegratePosition` and `Car_IntegrateOrientation`.
//!
//! Ported and replayed against the original: `Car_ObjectDynamicsTick`
//! ([`SmackBody::dynamics_tick`], `tests/original_track_objects.rs`). Ported
//! from pseudo-C only: the object branch of the ground tick. Approximations
//! A174-A176 in `docs/approximations.md`.

use nfs_formats::{Entry, Result, TrackCrp, animdefs::AnimDefs};

use super::original_pair::{PairCar, PairGlobals, add, clamp_angular_velocity, dot_ext, sub};
use super::original_support_assembly::SelectedBaseMask;
use super::original_track_objects::{ObjectCar, body_contact};
use super::original_vehicle::car::Axes;
use super::original_vehicle::ground::GroundHit;
use super::original_vehicle::step::{mat_mul, rot_x, rot_y, rot_z, transpose};

/// A real car within this octagonal XZ distance wakes a sleeping object
/// [VA 0x4a0f80].
pub const WAKE_DISTANCE: f32 = 10.0;
/// `g_GripTable`-like surface table at 0x5d0b94: (contact friction k, bounce).
pub const SURFACE_K: [(f32, f32); 16] = [
    (0.5, 0.5),
    (0.4, 0.7),
    (0.6, 0.5),
    (0.6, 0.5),
    (0.4, 0.7),
    (0.6, 0.5),
    (0.25, 0.5),
    (0.4, 0.4),
    (0.1, 0.7),
    (0.2, 0.5),
    (0.55, 0.6),
    (0.4, 0.5),
    (1.0, 1.0),
    (1.0, 1.0),
    (1.0, 1.0),
    (1.0, 1.0),
];

/// `Car_InitObjectShape` 0x4a1280: the contact code `+0x44c` of an
/// `AudioTypes` index.
pub fn type_code(audio_type: u32) -> u32 {
    match audio_type {
        1 => 0x12,
        2 => 0x11,
        3 => 0x14,
        4 => 0x10,
        5 => 0xe,
        6 => 0xf,
        7 => 3,
        8 => 9,
        9 => 10,
        10 => 0xb,
        11 => 0xc,
        12 => 0xd,
        _ => 0x13,
    }
}

/// One entry of the original's smackable list (`[0x628960]+0x34`): placement,
/// hull points, mass and type. Approximation A174: the list builder was not
/// found; entries are the placed `kSmackable` articles.
#[derive(Clone, Debug, PartialEq)]
pub struct SmackRecord {
    pub article_index: usize,
    pub tag: u32,
    /// Record `+0x28`: the start position (centre of the article's bounds).
    pub position: [f32; 3],
    /// Record `+0x34`: right, up, forward axes (the `tr` rows, normalised).
    pub axes: Axes,
    /// Record `+0xc`: hull points relative to the centre in the axes' frame.
    pub hull: Vec<[f32; 3]>,
    /// Car mass: the animdefs `mass` / 128 [VA 0x4a1280].
    pub mass: f32,
    /// Record `+0x18` mapped through [`type_code`] (`+0x44c`).
    pub type_44c: u32,
}

fn word(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("smackable resource index outside payload")?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] as f64 * v[0] as f64 + v[1] as f64 * v[1] as f64 + v[2] as f64 * v[2] as f64)
        .sqrt();
    if l < 1e-9 {
        v
    } else {
        v.map(|c| (c as f64 / l) as f32)
    }
}

fn selected_vt(article: &Entry, resource: u16) -> Option<&Entry> {
    if resource == u16::MAX {
        article
            .children
            .iter()
            .find(|entry| entry.tag == "vt" && entry.index == 0)
    } else {
        article.find("vt", resource)
    }
}

/// Height of the part of a tall, flat object that the pair test sees
/// (approximation A177).
pub const FLAT_HULL_HEIGHT: f32 = 0.9;

/// Approximation A177: a street sign's hull is a flat disc plus pole, about
/// 3.3 m tall and 0.02 m thick. In the 28-point box test of `0x496fd0` only
/// the sign's lowest face centre and corners can lie inside the car's box (the
/// rest is above its roof) and at ground level that is a coin flip on the
/// ride height: 3 of 4 signs were missed by a car on the road. The original's
/// hull for these objects is not recovered (A174), but they are kicked
/// reliably there, so the hull of a tall, flat object is cut to its lowest
/// [`FLAT_HULL_HEIGHT`] m (points above are clamped) and the box re-centred.
fn clip_flat_tall_hull(position: &mut [f32; 3], axes: &Axes, hull: &mut [[f32; 3]]) {
    if hull.is_empty() {
        return;
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in hull.iter() {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let thin = (0..3).map(|i| (hi[i] - lo[i]) * 0.5).fold(f32::MAX, f32::min) < 0.1;
    if !thin || hi[1] - lo[1] <= 1.2 {
        return;
    }
    let top = lo[1] + FLAT_HULL_HEIGHT;
    let shift = (lo[1] + top) * 0.5;
    for p in hull.iter_mut() {
        p[1] = p[1].min(top) - shift;
    }
    for i in 0..3 {
        position[i] += axes[1][i] * shift;
    }
}

/// Maximum hull points kept per object (the contact response is linear in it).
const MAX_HULL: usize = 64;

/// Smackable records of a track: non-library articles whose animdefs
/// `collideType` is `kSmackable` (3).
pub fn build_smack_records(
    archive: &TrackCrp,
    anim: &AnimDefs,
    ordinals: &[u32],
) -> Result<Vec<SmackRecord>> {
    let mut out = Vec::new();
    for (index, (article, ordinal)) in archive.articles.iter().zip(ordinals).enumerate() {
        let Some(base) = article.children.iter().find(|e| e.tag == "Base") else {
            continue;
        };
        if word(&base.data, 0)? & 0x8000 != 0 {
            continue;
        }
        let mut selected = base.data.clone();
        selected
            .get_mut(4..8)
            .ok_or("Base lacks current ordinal")?
            .copy_from_slice(&ordinal.to_le_bytes());
        let Some(mask) = SelectedBaseMask::from_payload(&selected)? else {
            continue;
        };
        let tag = word(&base.data, 0x44)?;
        let Some(def) = anim.find_tag_or_first(tag) else {
            continue;
        };
        if def.collide_type != 3 {
            continue;
        }
        let Some(vt) = selected_vt(article, mask.vertex_and_color_resource) else {
            continue;
        };
        let tr = article
            .children
            .iter()
            .find(|e| e.tag == "tr" && e.index == 0 && e.data.len() >= 64);
        let m: [f32; 16] = std::array::from_fn(|i| {
            tr.map_or(if i % 5 == 0 { 1.0 } else { 0.0 }, |t| {
                f32::from_le_bytes(t.data[i * 4..i * 4 + 4].try_into().unwrap())
            })
        });
        let world = |p: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|c| {
                (p[0] as f64 * m[c] as f64
                    + p[1] as f64 * m[4 + c] as f64
                    + p[2] as f64 * m[8 + c] as f64
                    + m[12 + c] as f64) as f32
            })
        };
        let pts: Vec<[f32; 3]> = vt
            .data
            .chunks_exact(16)
            .map(|c| {
                world([0, 4, 8].map(|o| f32::from_bits(u32::from_le_bytes(c[o..o + 4].try_into().unwrap()))))
            })
            .collect();
        if pts.is_empty() || pts.iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &pts {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let position = [0, 1, 2].map(|i| ((lo[i] as f64 + hi[i] as f64) * 0.5) as f32);
        let axes: Axes = [
            norm([m[0], m[1], m[2]]),
            norm([m[4], m[5], m[6]]),
            norm([m[8], m[9], m[10]]),
        ];
        let mut position = position;
        let mut hull: Vec<[f32; 3]> = pts
            .iter()
            .map(|p| {
                let d = sub(*p, position);
                [
                    dot_ext(d, axes[0]) as f32,
                    dot_ext(d, axes[1]) as f32,
                    dot_ext(d, axes[2]) as f32,
                ]
            })
            .collect();
        clip_flat_tall_hull(&mut position, &axes, &mut hull);
        hull.dedup();
        hull.sort_by(|a, b| a.partial_cmp(b).unwrap());
        hull.dedup();
        while hull.len() > MAX_HULL {
            // Keep every other point: only the extents and the contact points matter.
            hull = hull.into_iter().step_by(2).collect();
        }
        out.push(SmackRecord {
            article_index: index,
            tag,
            position,
            axes,
            hull,
            mass: def.mass / 128.0,
            type_44c: type_code(def.audio_type),
        });
    }
    Ok(out)
}

/// A smackable as a flag-0x80 car.
#[derive(Clone, Debug, PartialEq)]
pub struct SmackBody {
    pub record: usize,
    pub car: ObjectCar,
    /// Local hull points (`+0x8c`).
    pub hull: Vec<[f32; 3]>,
    /// `+0x7d`: asleep.
    pub asleep: bool,
    /// `+0x41c`: ground height at the previous ground tick.
    pub ground_prev: f32,
    /// `+0x80`: re-orthonormalisation counter of the orientation integrator.
    pub reortho: i32,
    /// Orientation changed since the reset (the renderer's transform).
    pub moved: bool,
}

impl SmackBody {
    /// The car after `0x4a1280`, `0x49b510` and `0x4a1510`: at rest on its
    /// record, asleep, not yet hit.
    pub fn from_record(index: usize, r: &SmackRecord) -> Self {
        let mut ext = [0.0_f32; 3];
        let (mut lo, mut hi) = ([0.0_f32; 3], [0.0_f32; 3]);
        for p in &r.hull {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        for i in 0..3 {
            ext[i] = ((hi[i] as f64 - lo[i] as f64) * 0.5) as f32;
        }
        let mass = r.mass.max(1e-4);
        let inertia = mass * 6.0;
        let pair = PairCar {
            position_330: r.position,
            mass_354: mass,
            inv_mass_358: 1.0 / mass,
            axis_r_364: r.axes[0],
            axis_u_370: r.axes[1],
            axis_f_37c: r.axes[2],
            inertia_394: inertia,
            inv_inertia_398: 1.0 / inertia,
            extents_3a8: ext,
            radius_3b4: (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt(),
            flags_52c: 0x80,
            type_44c: r.type_44c,
            active_7c: false,
            ..PairCar::default()
        };
        let mut body = Self {
            record: index,
            car: ObjectCar {
                pair,
                field_3a0: 1.0,
                ground_right_490: r.axes[0],
                ground_forward_4a8: r.axes[2],
                ..ObjectCar::default()
            },
            hull: r.hull.clone(),
            asleep: true,
            ground_prev: 0.0,
            reortho: 0x20,
            moved: false,
        };
        body.update_world_points();
        body
    }

    /// `Car_UpdateWorldPoints` 0x49ae30.
    pub fn update_world_points(&mut self) {
        let t = transpose([
            self.car.pair.axis_r_364,
            self.car.pair.axis_u_370,
            self.car.pair.axis_f_37c,
        ]);
        let pos = self.car.pair.position_330;
        self.car.body_points_1dc = self
            .hull
            .iter()
            .map(|&p| [0, 1, 2].map(|i| (dot_ext(p, t[i]) + pos[i] as f64) as f32))
            .collect();
    }

    fn axes(&self) -> Axes {
        [
            self.car.pair.axis_r_364,
            self.car.pair.axis_u_370,
            self.car.pair.axis_f_37c,
        ]
    }

    /// `Car_ObjectDynamicsTick` 0x4a1030 (32 Hz). `real_car_near`: a real
    /// car is within [`WAKE_DISTANCE`] [VA 0x4a0f80].
    pub fn dynamics_tick(&mut self, real_car_near: bool) {
        let p = &mut self.car.pair;
        if self.asleep {
            if p.active_7c {
                p.velocity_33c = [0.0; 3];
                p.angular_388 = [0.0; 3];
            }
            if real_car_near {
                self.asleep = false;
            }
            return;
        }
        let mul = |v: &mut f32, c: f32| *v = (*v as f64 * c as f64) as f32;
        let c995 = 0.995_f32;
        let h424 = p.field_424;
        let mut skip_z = false;
        if !(h424 < 0.005) {
            if p.angular_388[0].abs() > 4.0 {
                mul(&mut p.angular_388[0], c995);
            }
            if p.angular_388[1].abs() > 4.0 {
                mul(&mut p.angular_388[1], c995);
            }
            if !(p.angular_388[2].abs() > 4.0) {
                skip_z = true;
            }
        } else {
            for v in &mut p.velocity_33c {
                mul(v, f32::from_bits(0x3f6c_cccd));
            }
            mul(&mut p.angular_388[0], c995);
            mul(&mut p.angular_388[1], 0.98);
        }
        if !skip_z {
            mul(&mut p.angular_388[2], c995);
        }
        let v = p.velocity_33c.map(|c| c.abs() as f64);
        if v[2] + v[1] + v[0] < 0.1 {
            let a = p.angular_388.map(|c| c.abs() as f64);
            if a[2] + a[1] + a[0] < 0.05 && self.car.field_428 < 0.01 && !real_car_near {
                self.asleep = true;
            }
        }
    }

    /// `Car_IntegratePosition` 0x49b7a0 (64 Hz) for a flag-0x80 car.
    pub fn integrate_position(&mut self) {
        if self.asleep {
            return;
        }
        let p = &mut self.car.pair;
        let d = p.velocity_33c.map(|v| (v as f64 * 0.015625) as f32);
        p.position_330 = [0, 1, 2].map(|i| (d[i] as f64 + p.position_330[i] as f64) as f32);
    }

    /// `Car_IntegrateOrientation` 0x49b8e0 (64 Hz), without the
    /// re-orthonormalisation (kept simple: the axes are renormalised every 32
    /// calls).
    pub fn integrate_orientation(&mut self) {
        if self.asleep {
            return;
        }
        let w = self.car.pair.angular_388;
        if w == [0.0; 3] {
            self.update_world_points();
            return;
        }
        let a = w.map(|c| (c as f64 * 0.015625) as f32);
        let t = mat_mul(mat_mul(rot_x(a[0]), rot_y(a[1])), rot_z(a[2]));
        let axes = mat_mul(self.axes(), t);
        self.car.pair.axis_r_364 = axes[0];
        self.car.pair.axis_u_370 = axes[1];
        self.car.pair.axis_f_37c = axes[2];
        self.moved = true;
        self.reortho -= 1;
        if self.reortho == 0 {
            let mut m = self.axes();
            super::original_vehicle::step::reorthonormalise(&mut m);
            self.car.pair.axis_r_364 = m[0];
            self.car.pair.axis_u_370 = m[1];
            self.car.pair.axis_f_37c = m[2];
            self.reortho = 0x20;
        }
        self.update_world_points();
    }

    /// The object branch of `Car_GroundContactTick` 0x49bd40 with the support
    /// query `0x4998c0` (32 Hz; only for a hit object that is awake).
    /// `ground`: the support plane under the centre.
    pub fn ground_tick(&mut self, ground: Option<GroundHit>, globals: &mut PairGlobals) {
        if self.asleep || !self.car.pair.active_7c {
            return;
        }
        let Some(hit) = ground else { return };
        let n = hit.normal;
        if n[1] <= 0.1 {
            return;
        }
        let [hw, hh, hl] = self.car.pair.extents_3a8;
        let pos = self.car.pair.position_330;
        let axes = self.axes();
        // 0x4998c0: ground height under the centre, wheel records.
        let ground_y = (-((pos[0] - hit.base[0]) * n[0]) - (pos[2] - hit.base[2]) * n[2]) / n[1]
            + hit.base[1];
        let corner = |sr: f32, sf: f32| -> [f32; 3] {
            [0, 1, 2].map(|i| {
                pos[i] - axes[1][i] * hh + sr * axes[0][i] * hw * 0.9 + sf * axes[2][i] * hl * 0.7
            })
        };
        self.car.wheel_records_7f8 = [
            corner(-1.0, 1.0),
            corner(1.0, 1.0),
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
        ];
        // 0x49a900: height of the body points above the plane (the mean of the
        // four lowest, averaged with the lowest).
        let mut low = [1000.0_f32; 4];
        for q in &self.car.body_points_1dc {
            let d = dot_ext(n, sub(*q, hit.base)) as f32;
            if low[0] < d {
                if low[1] < d {
                    if low[2] < d {
                        if d <= low[3] {
                            low[3] = d;
                        }
                    } else {
                        low[3] = low[2];
                        low[2] = d;
                    }
                } else {
                    low[3] = low[2];
                    low[2] = low[1];
                    low[1] = d;
                }
            } else {
                low[3] = low[2];
                low[2] = low[1];
                low[1] = low[0];
                low[0] = d;
            }
        }
        let height = (((low[3] + low[2] + low[1] + low[0]) * 0.25 + low[0]) * 0.5).min(1000.0);
        self.car.pair.field_424 = height;
        self.car.field_428 = height;
        let rate = (ground_y - self.ground_prev) * 32.0;
        self.ground_prev = ground_y;
        let up = dot_ext(axes[1], n) as f32;
        self.car.field_3a0 = up;
        let p = &mut self.car.pair;
        let gravity = if height > 0.005 { 0.30625_f32 } else { 0.0 };
        let vy = (p.velocity_33c[1] as f64 - gravity as f64) as f32;
        p.velocity_33c[1] = vy;
        let (ax, az) = (p.velocity_33c[0].abs(), p.velocity_33c[2].abs());
        p.speed_35c = if ax <= az { ax * 0.25 + az } else { az * 0.25 + ax };
        let closing = rate - vy;
        let (k, bounce) = SURFACE_K[(hit.surface & 0xf) as usize];
        if height >= 0.001 {
            if closing <= 6.0 {
                if height < 0.005 {
                    p.field_424 = 0.0;
                    p.position_330[1] -= height;
                    p.velocity_33c[1] = if vy <= 0.0 || rate <= 0.0 { rate } else { vy.max(rate) };
                }
            } else if height < -0.05 {
                p.field_424 = -0.05;
                p.position_330[1] += -0.05 - height;
                p.velocity_33c[1] = bounce * 0.4 * closing + rate;
                clamp_angular_velocity(p);
            }
        } else {
            let body = self.car.clone();
            let mut o = body;
            body_contact(&mut o, n, hit.base, k, globals);
            self.car = o;
            let p = &mut self.car.pair;
            if up < 0.707 && vy < -2.0 {
                p.velocity_33c[1] -= bounce * vy;
                p.velocity_33c[0] *= 0.95;
                p.velocity_33c[2] *= 0.95;
            }
        }
        self.update_world_points();
        let _ = add;
    }
}

/// Octagonal XZ distance `max + 0.25 min` of `0x4a0f80`.
pub fn octagon_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = (a[0] - b[0]).abs();
    let dz = (a[2] - b[2]).abs();
    if dx <= dz { dx * 0.25 + dz } else { dz * 0.25 + dx }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> SmackRecord {
        SmackRecord {
            article_index: 0,
            tag: 0,
            position: [0.0, 1.0, 0.0],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            hull: vec![
                [-0.1, -1.0, -0.1],
                [0.1, -1.0, -0.1],
                [-0.1, 1.0, 0.1],
                [0.1, 1.0, 0.1],
            ],
            mass: 0.625,
            type_44c: 0x12,
        }
    }

    #[test]
    fn sleeps_until_a_car_is_near_then_settles_when_it_leaves() {
        let mut b = SmackBody::from_record(0, &record());
        b.dynamics_tick(false);
        assert!(b.asleep);
        b.dynamics_tick(true);
        assert!(!b.asleep);
        b.dynamics_tick(false);
        assert!(b.asleep, "at rest and nobody near: asleep again");
    }

    #[test]
    fn flat_tall_sign_hull_is_cut_so_a_car_on_the_road_hits_it() {
        use crate::physics::original_pair::{PairGlobals, PairCar};
        use crate::race::original_collision::collide_pass_with;
        // Disc + pole: 1.5 x 3.3 x 0.024 around the centre.
        let mut pos = [0.0, 1.66, 0.0];
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut hull = vec![
            [-0.757, -1.66, -0.012],
            [0.757, -1.66, 0.012],
            [-0.757, 1.66, -0.012],
            [0.757, 1.66, 0.012],
        ];
        clip_flat_tall_hull(&mut pos, &axes, &mut hull);
        let top = hull.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let bottom = hull.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((top - bottom - FLAT_HULL_HEIGHT).abs() < 1e-4);
        assert!((pos[1] - (0.45)).abs() < 1e-3, "{pos:?}");
        let record = SmackRecord {
            article_index: 0,
            tag: 0,
            position: pos,
            axes,
            hull,
            mass: 0.625,
            type_44c: 0x12,
        };
        let mut hits = 0;
        for ride in [0.0_f32, 0.02, 0.05, 0.1, 0.2] {
            let mut body = SmackBody::from_record(0, &record);
            body.asleep = false;
            let ext = [0.9, 0.65, 2.1];
            let car = PairCar {
                position_330: [0.0, 0.65 + ride, -12.0],
                velocity_33c: [0.0, 0.0, 30.0],
                mass_354: 1.25,
                inv_mass_358: 0.8,
                speed_35c: 30.0,
                axis_r_364: [1.0, 0.0, 0.0],
                axis_u_370: [0.0, 1.0, 0.0],
                axis_f_37c: [0.0, 0.0, 1.0],
                inertia_394: 7.5,
                inv_inertia_398: 1.0 / 7.5,
                extents_3a8: ext,
                radius_3b4: 2.4,
                active_7c: true,
                ..PairCar::default()
            };
            let mut cars = vec![car, body.car.pair.clone()];
            for _ in 0..60 {
                cars[0].position_330[2] += 30.0 / 32.0;
                collide_pass_with(&mut cars, &mut PairGlobals::default(), |_, _, _| {});
                if cars[1].active_7c {
                    hits += 1;
                    break;
                }
            }
        }
        assert_eq!(hits, 5);
    }

    #[test]
    fn extents_are_half_the_hull_box_and_mass_is_in_128ths() {
        let b = SmackBody::from_record(0, &record());
        assert_eq!(b.car.pair.extents_3a8, [0.1, 1.0, 0.1]);
        assert_eq!(b.car.pair.mass_354, 0.625);
        assert_eq!(b.car.pair.flags_52c, 0x80);
    }

    #[test]
    fn a_hit_object_falls_to_the_ground_and_stops() {
        let mut b = SmackBody::from_record(0, &record());
        b.asleep = false;
        b.car.pair.active_7c = true;
        b.car.pair.position_330[1] = 3.0;
        b.car.pair.velocity_33c = [2.0, 0.0, 0.0];
        b.update_world_points();
        let plane = GroundHit::at([0.0, 0.0, 0.0], 0.0, [0.0, 1.0, 0.0], 1);
        let mut globals = PairGlobals::default();
        for step in 0..2000 {
            if step % 2 == 0 {
                b.dynamics_tick(true);
                b.ground_tick(Some(plane), &mut globals);
            }
            b.integrate_position();
            b.integrate_orientation();
        }
        let low = b
            .car
            .body_points_1dc
            .iter()
            .map(|p| p[1])
            .fold(f32::MAX, f32::min);
        assert!(low > -0.2 && low < 0.3, "lowest point {low}");
        assert!(b.car.pair.velocity_33c.iter().all(|v| v.abs() < 0.5));
    }
}

/// All smackable cars of a track and their 64 Hz clock. The 32 Hz callbacks
/// run on every second step [VA 0x411130: queue 0x606ad0 even steps].
#[derive(Clone, Debug, Default)]
pub struct SmackWorld {
    pub bodies: Vec<SmackBody>,
    pending: f64,
    step: u64,
}

impl SmackWorld {
    pub fn new(records: &[SmackRecord]) -> Self {
        Self {
            bodies: records
                .iter()
                .enumerate()
                .map(|(i, r)| SmackBody::from_record(i, r))
                .collect(),
            pending: 0.0,
            step: 0,
        }
    }

    /// Race restart: every object back on its post, asleep.
    pub fn reset(&mut self, records: &[SmackRecord]) {
        *self = Self::new(records);
    }

    /// Advance by `dt` seconds. `real_cars`: positions of the real cars
    /// (`+0x7f` set); `ground`: the support plane under a point.
    /// Approximation A176: the world runs on its own 64 Hz clock, not in the
    /// player's scheduler step.
    pub fn advance(
        &mut self,
        dt: f32,
        real_cars: &[[f32; 3]],
        ground: &mut dyn FnMut([f32; 3]) -> Option<GroundHit>,
        globals: &mut PairGlobals,
    ) {
        self.pending += (dt as f64).clamp(0.0, 0.25);
        while self.pending >= 1.0 / 64.0 {
            self.pending -= 1.0 / 64.0;
            if self.step % 2 == 0 {
                for b in &mut self.bodies {
                    let pos = b.car.pair.position_330;
                    let near = real_cars
                        .iter()
                        .any(|c| octagon_distance(pos, *c) < WAKE_DISTANCE);
                    b.dynamics_tick(near);
                    if !b.asleep && b.car.pair.active_7c {
                        let hit = ground(b.car.pair.position_330);
                        b.ground_tick(hit, globals);
                    }
                }
            }
            for b in &mut self.bodies {
                b.integrate_position();
                b.integrate_orientation();
            }
            self.step += 1;
        }
    }

    /// Indices of the cars that take part in the pair collision (awake).
    pub fn awake(&self) -> Vec<usize> {
        (0..self.bodies.len())
            .filter(|&i| !self.bodies[i].asleep)
            .collect()
    }
}

/// `build_smack_records` with the stored Base+4 ordinals.
pub fn build_initial_smack_records(archive: &TrackCrp, anim: &AnimDefs) -> Result<Vec<SmackRecord>> {
    let ordinals = archive
        .articles
        .iter()
        .map(|article| {
            let base = article
                .children
                .iter()
                .find(|entry| entry.tag == "Base")
                .ok_or("smackable article lacks Base")?;
            word(&base.data, 4)
        })
        .collect::<Result<Vec<_>>>()?;
    build_smack_records(archive, anim, &ordinals)
}

//! Original car-to-car collision kernels from `nfs5.exe` (code bytes match
//! `Porsche.exe` v3.5).
//!
//! Source: `docs/re/collision.md` "Car-to-car (original)" in the workspace root
//! and the x87 instructions of
//! - `CarPair_SeparateOverlap` `0x497b10` (positional separation, 1/32 steps),
//! - `CarPair_FindContact` `0x496fd0` (28-point box test) with the point tests
//!   `0x495600` (B face centre in A), `0x495890` (A face centre in B),
//!   `0x495b30` (B corner in A) and `0x495d30` (A corner in B),
//! - `CarPair_ApplyImpulse` `0x496860` with its effects
//!   `Car_ImpactByDirection` `0x494df0` and `Car_ApplyImpact` `0x494500`
//!   (zone damage, hard-hit kick and lift, random and zone spin),
//! - `Car_ClampAngularVelocity` `0x4964d0`.
//!
//! Not ported: the `+0x434 == 0` branch of `0x494500` (height change from
//! `+0xd6c`; unreachable from the pair call, which sets `+0x434` first) and the
//! `0x49bba0` callback (only for sources other than 2, car contact). The
//! flag-0x80 (track-object vehicle), `+0x518`, `[0x657408]`/`[0x6573e8]`,
//! damage boost `[0x657400]` and zone-8 (`dir·U < -0.5`) branches are ported
//! from the instructions but not covered by the recorded fixtures.
//!
//! Arithmetic follows the original operation order: f64 stands in for the x87
//! extended accumulator and every original f32 store is rounded with `as f32`.
//! Verified against outputs recorded by executing the original bytes
//! (`scripts/research/replay-car-pair.py`, tests `tests/original_pair.rs`).
//!
//! Units are the original ones: position `+0x330` in world units (hypothesis:
//! metres), mass `+0x354` (hypothesis: tonnes). Angular velocity `+0x388` is in
//! rad/s here; the original caller `0x499103` converts from turns/s around the
//! pair call.

/// Car fields read or written by the pair kernels. Names keep the original
/// offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PairCar {
    pub position_330: [f32; 3],
    pub velocity_33c: [f32; 3],
    pub mass_354: f32,
    pub inv_mass_358: f32,
    /// Speed estimate; read only by the retry loop `0x4980d0` (not ported here).
    pub speed_35c: f32,
    /// Box axes right/up/forward (`+0x364`, `+0x370`, `+0x37c`).
    pub axis_r_364: [f32; 3],
    pub axis_u_370: [f32; 3],
    pub axis_f_37c: [f32; 3],
    /// Angular velocity, rad/s inside the pair call.
    pub angular_388: [f32; 3],
    pub inertia_394: f32,
    pub inv_inertia_398: f32,
    /// Half extents width/height/length (`+0x3a8`, `+0x3ac`, `+0x3b0`).
    pub extents_3a8: [f32; 3],
    /// Bounding radius for the broadphase `0x499077` (not used by the kernels).
    pub radius_3b4: f32,
    /// Byte `+0x7c`, physically active; set on the first car hit of a
    /// flag-0x80 car.
    pub active_7c: bool,
    /// Contact counter (short); a hard hit in `0x494500` resets it to 1.
    pub contacts_418: i16,
    /// Read by the hard-hit lift in `0x494500` (`< 0.1` lifts `+0x334` by 0.1).
    pub field_424: f32,
    pub hit_flag_42c: i32,
    /// Impact strength of the last contact.
    pub impact_430: f32,
    /// Other car of the last contact (a car pointer in the original).
    pub other_434: Option<PairRole>,
    /// Contact code: `0x50003`, or `0x50000 | other+0x44c` for flag-0x80 cars.
    pub code_438: u32,
    /// Contact position for effects: midpoint of the two car centres.
    pub contact_pos_440: [f32; 3],
    /// Object/vehicle type code of flag-0x80 cars (0x10 special).
    pub type_44c: u32,
    pub field_450: i32,
    /// Damage per zone (written only with the damage gate `[0x657a2c]`).
    pub damage_4e4: [f32; 9],
    /// Flags; 0x80 marks a track-object vehicle.
    pub flags_52c: u32,
    /// Car resource field `+0x558 -> +0x518` (shrunk box / light mode).
    pub resource_518: bool,
    /// Blocks damage when non-zero.
    pub field_b50: i32,
    /// Local angular velocity written by `0x494500` (`ω·R`, `ω·U`, `ω·F`).
    pub local_angular_d64: [f32; 3],
    /// With `[0x657408]`, scales the impulse by 0.2.
    pub field_dac: i32,
    pub field_db0: i32,
    pub field_1100: i32,
    pub field_1104: i32,
    /// Damage events queued through `0x40f020` (count `+0x110c`, at most 7);
    /// only the event's strength argument is kept.
    pub damage_events_110c: Vec<f32>,
}

/// Role of a car in the pair call `(A, B)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairRole {
    A,
    B,
}

/// Globals read by the pair impulse and `Car_ApplyImpact` `0x494500`.
#[derive(Clone, Debug, PartialEq)]
pub struct PairGlobals {
    /// `[0x657408]`: with `+0xdac`, impulse x0.2; with mode 3, impact x0.5.
    pub pair_damp_657408: bool,
    /// `[0x657a2c]`: damage gate.
    pub damage_on_657a2c: bool,
    /// `[0x657400]`: damage x1.5 instead of x0.5.
    pub damage_boost_657400: bool,
    /// `[0x6573e8]`: mode selector.
    pub mode_6573e8: u32,
    /// LCG state `[0x5d1028]`.
    pub rand_state_5d1028: u32,
    /// LCG multiplier `[0x5d102c]` (1 in the shipped `.data`).
    pub rand_mul_5d102c: u32,
}

impl Default for PairGlobals {
    /// Values of the shipped `nfs5.exe` image (`.data` and zeroed `.bss`).
    fn default() -> Self {
        Self {
            pair_damp_657408: false,
            damage_on_657a2c: false,
            damage_boost_657400: false,
            mode_6573e8: 0,
            rand_state_5d1028: 0xf874af01,
            rand_mul_5d102c: 1,
        }
    }
}

impl PairGlobals {
    /// LCG step at `0x494977`/`0x494a83`: `r = mul*state`, `state = r & 0xffff`,
    /// result `(r >> 8) & 0xffff`.
    fn next_random(&mut self) -> u32 {
        let r = self.rand_mul_5d102c.wrapping_mul(self.rand_state_5d1028);
        self.rand_state_5d1028 = r & 0xffff;
        (r >> 8) & 0xffff
    }
}

impl PairCar {
    fn axes(&self) -> [[f32; 3]; 3] {
        [self.axis_r_364, self.axis_u_370, self.axis_f_37c]
    }

    fn object_vehicle(&self) -> bool {
        self.flags_52c & 0x80 != 0
    }
}

/// Contact returned by the separation: point and normal (pointing from B to A)
/// of the last overlapping iteration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairContact {
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

/// `Vec3_Dot` `0x532910` (`0x56e8d1`): `(x*x' + y*y') + z*z'` left on the x87
/// stack.
fn dot_ext(a: [f32; 3], b: [f32; 3]) -> f64 {
    (a[0] as f64 * b[0] as f64 + a[1] as f64 * b[1] as f64) + a[2] as f64 * b[2] as f64
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    dot_ext(a, b) as f32
}

/// `Vec3_Sub` `0x532330`: `a - b`, stored f32.
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| (a[i] as f64 - b[i] as f64) as f32)
}

/// `Vec3_Add` `0x532300`: `a + b`, stored f32.
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| (a[i] as f64 + b[i] as f64) as f32)
}

/// `Vec3_Cross` `0x532880` (`0x56e8f5`): `a × b`, stored f32.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let [ax, ay, az] = a.map(|c| c as f64);
    let [bx, by, bz] = b.map(|c| c as f64);
    [
        (ay * bz - az * by) as f32,
        (az * bx - ax * bz) as f32,
        (ax * by - ay * bx) as f32,
    ]
}

/// `Vec3_Scale` `0x532360`: `v * s`, stored f32.
fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    v.map(|c| (s as f64 * c as f64) as f32)
}

/// `Vec3_Length` `0x532b20`: `sqrt((z² + y²) + x²)`, extended.
fn length_ext(v: [f32; 3]) -> f64 {
    let [x, y, z] = v.map(|c| c as f64);
    ((z * z + y * y) + x * x).sqrt()
}

/// `Vec3_Normalize` `0x5328d0`: `inv = 1/length` (no zero check), stored f32.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let inv = 1.0 / length_ext(v);
    v.map(|c| (inv * c as f64) as f32)
}

/// Precomputed axis products of `0x497b10` (globals `0x628f08..0x628cac`).
struct PairFrame {
    /// `0x628f2c..`: `(A_i·B_j) * extB_j`, indexed `[i][j]`.
    b_in_a: [[f32; 3]; 3],
    /// `0x628c8c..`: `(A_i·B_j) * extA_i`, indexed `[i][j]`.
    a_in_b: [[f32; 3]; 3],
    /// `0x628cc4..`: `A_i·d` with `d = B.pos - A.pos`.
    a_offset: [f32; 3],
    /// `0x628cd0..`: `B_j·d`.
    b_offset: [f32; 3],
}

impl PairFrame {
    fn new(a: &PairCar, b: &PairCar) -> Self {
        let (aa, ba) = (a.axes(), b.axes());
        let d: [[f32; 3]; 3] = [0, 1, 2].map(|i| [0, 1, 2].map(|j| dot(aa[i], ba[j])));
        Self {
            b_in_a: [0, 1, 2]
                .map(|i| [0, 1, 2].map(|j| (d[i][j] as f64 * b.extents_3a8[j] as f64) as f32)),
            a_in_b: [0, 1, 2]
                .map(|i| [0, 1, 2].map(|j| (d[i][j] as f64 * a.extents_3a8[i] as f64) as f32)),
            a_offset: [0.0; 3],
            b_offset: [0.0; 3],
        }
    }

    fn set_offset(&mut self, a: &PairCar, b: &PairCar) {
        let d = sub(b.position_330, a.position_330);
        self.a_offset = a.axes().map(|axis| dot(axis, d));
        self.b_offset = b.axes().map(|axis| dot(axis, d));
    }

    /// Local coordinates in A of B's unit point `p` (`0x495600`/`0x495b30`).
    fn b_point_in_a(&self, p: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| {
            let t = self.b_in_a[i];
            (((t[0] as f64 * p[0] as f64 + t[1] as f64 * p[1] as f64) + t[2] as f64 * p[2] as f64)
                + self.a_offset[i] as f64) as f32
        })
    }

    /// Local coordinates in B of A's unit point `p` (`0x495890`/`0x495d30`).
    fn a_point_in_b(&self, p: [f32; 3]) -> [f32; 3] {
        let t = self.a_in_b;
        [0, 1, 2].map(|j| {
            (((t[0][j] as f64 * p[0] as f64 + t[1][j] as f64 * p[1] as f64)
                + t[2][j] as f64 * p[2] as f64)
                - self.b_offset[j] as f64) as f32
        })
    }
}

/// Inclusive inside test of the point tests: `-ext <= local <= ext` per axis.
fn inside(local: [f32; 3], ext: [f32; 3]) -> bool {
    (0..3).all(|i| local[i] <= ext[i] && -ext[i] <= local[i])
}

/// World point of `owner`'s unit point `p`: `((eR·p0 + eU·p1) + eF·p2) + pos`.
fn world_point(owner: &PairCar, p: [f32; 3]) -> [f32; 3] {
    let axes = owner.axes();
    let e = owner.extents_3a8;
    [0, 1, 2].map(|k| {
        let term = |n: usize| e[n] as f64 * axes[n][k] as f64 * p[n] as f64;
        (((term(0) + term(1)) + term(2)) + owner.position_330[k] as f64) as f32
    })
}

/// Face normal `(p0·R + p1·U) + p2·F` of `owner`'s axes.
fn face_axis(owner: &PairCar, p: [f32; 3]) -> [f32; 3] {
    let axes = owner.axes();
    [0, 1, 2].map(|k| {
        let term = |n: usize| p[n] as f64 * axes[n][k] as f64;
        ((term(0) + term(1)) + term(2)) as f32
    })
}

const FACES: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, -1.0],
];

const CORNERS: [[f32; 3]; 8] = [
    [1.0, 1.0, 1.0],
    [1.0, 1.0, -1.0],
    [1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [-1.0, -1.0, -1.0],
];

/// Normal for a corner hit, chosen in `0x496fd0`. `owner` owns the corner,
/// `other` is the box it is inside, `local` the corner in `other`'s frame.
/// `corner_of_a` selects the A-corner variant (normal keeps `other`'s axis
/// sign) versus the B-corner variant (normal points against it).
fn corner_normal(
    a: &PairCar,
    b: &PairCar,
    owner: &PairCar,
    other: &PairCar,
    local: [f32; 3],
    corner_of_a: bool,
) -> [f32; 3] {
    // Max |vB - vA| component; the first one stays extended on the x87 stack.
    let dx = (b.velocity_33c[0] as f64 - a.velocity_33c[0] as f64).abs();
    let dy = (b.velocity_33c[1] as f64 - a.velocity_33c[1] as f64).abs() as f32;
    let dz = (b.velocity_33c[2] as f64 - a.velocity_33c[2] as f64).abs() as f32;
    let mut max = dx;
    if dy as f64 > dx as f32 as f64 {
        max = dy as f64;
    }
    if dz as f64 > max {
        max = dz as f64;
    }
    let axes = other.axes();
    if max > 15.0 {
        // g_PairFastContact: axis of `other` most parallel to the owner's
        // velocity, signed by the corner's side.
        let flip = |value: f32| {
            if corner_of_a {
                value < 0.0
            } else {
                value > 0.0
            }
        };
        let signed = [0, 1, 2].map(|n| {
            if flip(local[n]) {
                axes[n].map(|c| -c)
            } else {
                axes[n]
            }
        });
        let v = owner.velocity_33c;
        let d0 = dot(signed[0], v).abs() as f64;
        let d1 = dot(signed[1], v).abs() as f64;
        let d2 = dot_ext(signed[2], v).abs();
        return if d1 < d0 && d2 < d0 {
            signed[0]
        } else if d2 < d1 {
            signed[1]
        } else {
            signed[2]
        };
    }
    // Least penetration `ext - |local|`; the first stays f32, the others
    // extended.
    let ext = other.extents_3a8;
    let depth = |n: usize| {
        if local[n] < 0.0 {
            local[n] as f64 + ext[n] as f64
        } else {
            ext[n] as f64 - local[n] as f64
        }
    };
    let f0 = depth(0) as f32 as f64;
    let (f1, f2) = (depth(1), depth(2));
    let n = if f0 < f1 && f0 < f2 {
        0
    } else if f1 < f2 {
        1
    } else {
        2
    };
    let keep = if corner_of_a {
        local[n] >= 0.0
    } else {
        local[n] <= 0.0
    };
    if keep {
        axes[n]
    } else {
        axes[n].map(|c| -c)
    }
}

fn find_contact_in(frame: &PairFrame, a: &PairCar, b: &PairCar) -> Option<PairContact> {
    for p in FACES {
        if inside(frame.b_point_in_a(p), a.extents_3a8) {
            return Some(PairContact {
                point: world_point(b, p),
                normal: face_axis(b, p),
            });
        }
    }
    for p in FACES {
        if inside(frame.a_point_in_b(p), b.extents_3a8) {
            return Some(PairContact {
                point: world_point(a, p),
                normal: face_axis(a, p).map(|c| -c),
            });
        }
    }
    for p in CORNERS {
        let local = frame.b_point_in_a(p);
        if inside(local, a.extents_3a8) {
            return Some(PairContact {
                point: world_point(b, p),
                normal: corner_normal(a, b, b, a, local, false),
            });
        }
    }
    for p in CORNERS {
        let local = frame.a_point_in_b(p);
        if inside(local, b.extents_3a8) {
            return Some(PairContact {
                point: world_point(a, p),
                normal: corner_normal(a, b, a, b, local, true),
            });
        }
    }
    None
}

/// `CarPair_FindContact` `0x496fd0`: tests B's 6 face centres in A, A's face
/// centres in B, B's 8 corners in A, then A's corners in B; the first point
/// inside (inclusive) wins. Edge-against-edge overlaps without a sampled point
/// inside return `None`, as in the original.
pub fn find_contact(a: &PairCar, b: &PairCar) -> Option<PairContact> {
    let mut frame = PairFrame::new(a, b);
    frame.set_offset(a, b);
    find_contact_in(&frame, a, b)
}

/// `CarPair_SeparateOverlap` `0x497b10`: while a contact is found (at most 32
/// iterations), move both positions by a step chosen on the first hit. The
/// step changes the relative separation by 1/32 unit: along the centre line
/// split by mass, or, for relative speed above 60 approaching head-on
/// (`|d.xz|² < 0.05` or direction match above 0.85), back along each car's
/// own velocity. Returns the contact of the last overlapping iteration.
pub fn separate_overlap(a: &mut PairCar, b: &mut PairCar) -> Option<PairContact> {
    let mut frame = PairFrame::new(a, b);
    let mut last = None;
    let mut steps: Option<([f32; 3], [f32; 3])> = None;
    for _ in 0..32 {
        frame.set_offset(a, b);
        let Some(contact) = find_contact_in(&frame, a, b) else {
            return last;
        };
        last = Some(contact);
        let (step_a, step_b) = *steps.get_or_insert_with(|| separation_steps(a, b));
        a.position_330 = add(a.position_330, step_a);
        b.position_330 = add(b.position_330, step_b);
    }
    last
}

/// Step selection of `0x497e0b..0x498079`.
fn separation_steps(a: &PairCar, b: &PairCar) -> ([f32; 3], [f32; 3]) {
    const STEP: f64 = 0.03125; // 0x5b24b8
    let d = sub(b.position_330, a.position_330);
    let vr = sub(a.velocity_33c, b.velocity_33c);
    let vr_xz = vr[2] as f64 * vr[2] as f64 + vr[0] as f64 * vr[0] as f64;
    // 3600 [0x5b49cc], 0.05 [0x5b26a0], 0.85 [0x5b25e0].
    let rewind = vr_xz > 3600.0
        && ((d[0] as f64 * d[0] as f64 + d[2] as f64 * d[2] as f64)
            < f32::from_bits(0x3d4ccccd) as f64
            || dot_ext(normalize(vr), normalize(d)) > f32::from_bits(0x3f59999a) as f64);
    if rewind {
        let half = length_ext(vr) * 0.5;
        let k = if half == 0.0 { 1.0 } else { 0.5 / half };
        let k = (-k) as f32;
        let back = |v: [f32; 3]| v.map(|c| ((c as f64 * k as f64) as f32 as f64 * STEP) as f32);
        return (back(a.velocity_33c), back(b.velocity_33c));
    }
    let u = normalize(d);
    let total = b.mass_354 as f64 + a.mass_354 as f64;
    let share_a = (a.mass_354 as f64 / total) as f32;
    let share_b = b.mass_354 as f64 / total;
    (
        u.map(|c| (c as f64 * share_b * -STEP) as f32),
        u.map(|c| (c as f64 * share_a as f64 * STEP) as f32),
    )
}

/// Impact strength `+0x430` of `car` (`0x496af3..0x496c3e`). `mass_cap` is the
/// mass in the flag-0x80 cap `(4 - m)·4|vA - vB|`: A's mass for both cars, a
/// quirk of the original (`0x496bcc`). `other_mass` gates the fixed 9.0.
fn impact_strength(car: &PairCar, momentum: f32, cap: f32, mass_cap: f32, other_mass: f32) -> f32 {
    let value = (momentum as f64 * car.inv_mass_358 as f64).abs();
    // The flag-0x80 cap is compared while still extended (0x496b2f).
    let limit = if car.object_vehicle() && car.mass_354 < 3.0 {
        (4.0 - mass_cap as f64) * cap as f64
    } else if other_mass <= f32::from_bits(0x3f266666) {
        return 9.0;
    } else {
        cap as f64
    };
    if limit > value {
        value as f32
    } else {
        limit as f32
    }
}

/// Spin factor of `0x496ddb..0x496e2d`.
fn spin_factor(car: &PairCar) -> f32 {
    if car.object_vehicle() && car.mass_354 < 0.75 {
        (1.5 / car.mass_354 as f64) as f32
    } else if car.impact_430 > 80.0 {
        3.0
    } else {
        1.5
    }
}

/// `CarPair_ApplyImpulse` `0x496860`: zero-restitution impulse at `contact`
/// (normal from B to A), `j = vrel / K` with `K = 1/mA + 1/mB + |rA×n|²/IA +
/// |rB×n|²/IB`. Returns `false` (no change) when the cars separate (`j < 0`).
/// Linear change x1.25 for a car with impact strength >= 80, angular change
/// x1.5 (x3 above 80). Then runs `Car_ImpactByDirection` for both cars.
pub fn apply_impulse(
    a: &mut PairCar,
    b: &mut PairCar,
    contact: PairContact,
    globals: &mut PairGlobals,
) -> bool {
    let n = contact.normal;
    let ra = sub(contact.point, a.position_330);
    let rb = sub(contact.point, b.position_330);
    let ca = cross(ra, n);
    let cb = cross(rb, n);
    let vb_n = dot(b.velocity_33c, n);
    let rel = (vb_n as f64 - dot_ext(a.velocity_33c, n)) as f32;
    let rel = (rel as f64 - dot_ext(a.angular_388, ca)) as f32;
    let rel = (dot_ext(b.angular_388, cb) + rel as f64) as f32;
    let term_a = (dot_ext(ca, ca) * a.inv_inertia_398 as f64 * 0.5) as f32;
    let half_k = ((dot_ext(cb, cb) * b.inv_inertia_398 as f64 * 0.5 + term_a as f64)
        + b.inv_mass_358 as f64 * 0.5)
        + a.inv_mass_358 as f64 * 0.5;
    let mut twice_j = rel as f64 / half_k;
    for car in [&mut *a, &mut *b] {
        if car.object_vehicle() && !car.active_7c {
            twice_j = twice_j.abs();
            car.active_7c = true;
        }
    }
    if twice_j < 0.0 {
        return false;
    }
    for car in [&mut *a, &mut *b] {
        if car.object_vehicle() && car.type_44c == 0x10 {
            car.hit_flag_42c = 1;
        }
    }
    let mut j = (twice_j * 0.5) as f32;
    if globals.pair_damp_657408 && (a.field_dac != 0 || b.field_dac != 0) {
        j = (j as f64 * f32::from_bits(0x3e4ccccd) as f64) as f32;
    }
    let impulse = scale(n, j);

    let va_n = dot(a.velocity_33c, n);
    let momentum = ((b.mass_354 as f64 + a.mass_354 as f64)
        * (va_n as f64 - dot_ext(b.velocity_33c, n))) as f32;
    let cap = (length_ext(sub(a.velocity_33c, b.velocity_33c)) * 4.0) as f32;
    a.impact_430 = impact_strength(a, momentum, cap, a.mass_354, b.mass_354);
    b.impact_430 = impact_strength(b, momentum, cap, a.mass_354, a.mass_354);

    let linear = |car: &PairCar| {
        if car.impact_430 < 80.0 {
            car.inv_mass_358
        } else {
            (car.inv_mass_358 as f64 * 1.25) as f32
        }
    };
    a.velocity_33c = add(a.velocity_33c, scale(impulse, linear(a)));
    a.code_438 = if b.object_vehicle() {
        b.type_44c | 0x50000
    } else {
        0x50003
    };
    a.other_434 = Some(PairRole::B);
    let mut direction = sub(a.position_330, b.position_330);
    if n == [0.0; 3] {
        direction = [0.0, 1.0, 0.0];
    } else {
        direction = normalize(direction);
    }
    b.velocity_33c = sub(b.velocity_33c, scale(impulse, linear(b)));
    b.code_438 = if a.object_vehicle() {
        a.type_44c | 0x50000
    } else {
        0x50003
    };
    b.other_434 = Some(PairRole::A);

    let k = (spin_factor(a) as f64 * a.inv_inertia_398 as f64) as f32;
    a.angular_388 = add(a.angular_388, scale(cross(ra, impulse), k));
    let k = (-(spin_factor(b) as f64 * b.inv_inertia_398 as f64)) as f32;
    b.angular_388 = add(b.angular_388, scale(cross(rb, impulse), k));

    a.contacts_418 = a.contacts_418.wrapping_add(1);
    b.contacts_418 = b.contacts_418.wrapping_add(1);
    let mid =
        [0, 1, 2].map(|i| ((b.position_330[i] as f64 + a.position_330[i] as f64) * 0.5) as f32);
    a.contact_pos_440 = mid;
    b.contact_pos_440 = mid;
    impact_by_direction(a, direction, globals);
    impact_by_direction(b, direction.map(|c| -c), globals);
    true
}

/// `Car_ImpactByDirection` `0x494df0` with source 2 (car contact): for impact
/// strength above 10, picks the damage zone from the direction's R/F
/// components (±0.1) and calls `Car_ApplyImpact`; zone 8 (`dir·U < -0.5`,
/// non-flag-0x80 cars) is applied first and then ends the call.
fn impact_by_direction(car: &mut PairCar, direction: [f32; 3], globals: &mut PairGlobals) {
    let r = dot(direction, car.axis_r_364);
    let u = dot(direction, car.axis_u_370);
    let f = dot(direction, car.axis_f_37c);
    let strength = car.impact_430;
    if strength <= 10.0 {
        return;
    }
    if u < -0.5 && !car.object_vehicle() {
        apply_impact(car, strength, 8, globals);
    }
    let tenth = f32::from_bits(0x3dcccccd);
    let zone = if r.abs() < tenth && f < -tenth {
        1
    } else if r.abs() < tenth && f > tenth {
        5
    } else if f.abs() < tenth && r > tenth {
        7
    } else if f.abs() < tenth && r < -tenth {
        3
    } else if f < -tenth && r > tenth {
        0
    } else if f < -tenth && r < -tenth {
        2
    } else if f > tenth && r > tenth {
        6
    } else if f > tenth && r < -tenth {
        4
    } else {
        // 0x495002: the zone is still 8 or 9 here, so nothing more happens.
        return;
    };
    apply_impact(car, strength, zone, globals);
}

/// `Car_ApplyImpact` `0x494500` for source 2 (car contact), as reached from
/// the pair impulse (`+0x434` already set).
fn apply_impact(car: &mut PairCar, strength: f32, zone: usize, globals: &mut PairGlobals) {
    if globals.damage_on_657a2c && car.field_b50 == 0 {
        apply_zone_damage(car, strength, zone, globals.damage_boost_657400);
    }

    let mut strength = strength;
    if globals.mode_6573e8 == 3 && globals.pair_damp_657408 {
        strength = (strength as f64 * 0.5) as f32;
    } else if car.resource_518 {
        strength = (strength as f64 + strength as f64) as f32;
    }
    let mut factor = 1.0_f32;
    let spin;
    if strength > 80.0 {
        let object = car.object_vehicle();
        if !object {
            car.hit_flag_42c = 1;
        }
        if !car.resource_518 && strength > 120.0 {
            car.field_db0 = 0;
        }
        let hit = (strength as f64 * 0.125) as f32;
        car.contacts_418 = 1;
        car.field_450 = 0;
        car.field_1100 = 0xc0;
        car.field_1104 = 1;
        let tenth = f32::from_bits(0x3dcccccd);
        if car.field_424 < tenth {
            car.position_330[1] = (car.position_330[1] as f64 + tenth as f64) as f32;
        }
        let value = hit as f64 * 0.03125;
        spin = if 2.0 <= value { 2.0 } else { value as f32 };
        factor = if car.resource_518 {
            3.0
        } else {
            f32::from_bits(0x3f99999a)
        };
        let kick = if object && car.mass_354 < 1.0 {
            let x = globals.next_random() as f64;
            let t = (x * f32::from_bits(0x3d4ccccd) as f64 * f32::from_bits(0x37800000) as f64
                + f32::from_bits(0x3dcccccd) as f64) as f32;
            let m = if car.mass_354 > t { car.mass_354 } else { t };
            m as f64 * hit as f64 * f32::from_bits(0x3ea8f5c3) as f64
        } else {
            hit as f64 * f32::from_bits(0x3eaaaaab) as f64
        };
        car.velocity_33c[1] = (kick + car.velocity_33c[1] as f64) as f32;
        if car.angular_388[0].abs() < 0.5 && car.angular_388[2].abs() < 0.5 {
            random_spin(car, globals);
        }
    } else {
        let value = strength as f64 * 0.03125;
        let limit = f32::from_bits(0x3ea8f5c3);
        spin = if limit as f64 <= value {
            limit
        } else {
            value as f32
        };
    }

    // Zone spin in the car frame (0x494c45..0x494dc8).
    let w = car.angular_388;
    let mut local = [dot(w, car.axis_r_364), dot(w, car.axis_u_370), 0.0];
    let local_z = dot_ext(w, car.axis_f_37c);
    let q = factor as f64 * spin as f64 * 0.5;
    local[0] = if zone <= 2 {
        (local[0] as f64 - q) as f32
    } else {
        (local[0] as f64 + q) as f32
    };
    if (2..=4).contains(&zone) {
        local[1] = (local[1] as f64 + q) as f32;
        local[2] = (q + local_z) as f32;
    } else {
        local[1] = (local[1] as f64 - q) as f32;
        local[2] = (local_z - q) as f32;
    }
    car.local_angular_d64 = local;
    let axes = car.axes();
    car.angular_388 = [0, 1, 2].map(|k| dot(local, [axes[0][k], axes[1][k], axes[2][k]]));
}

/// Random spin of `0x494a62..0x494c14` (hard car hit with small roll/pitch).
fn random_spin(car: &mut PairCar, globals: &mut PairGlobals) {
    let unit = f32::from_bits(0x37800000) as f64; // 1/65536
    let mut draw =
        |scale: f64, offset: f64| (globals.next_random() as f64 * scale * unit - offset) as f32;
    car.angular_388 = if !car.object_vehicle() && !car.resource_518 {
        let x = draw(5.0, 2.5);
        let y = draw(2.0, 1.0);
        [x, y, draw(5.0, 2.5)]
    } else {
        let x = draw(20.0, 10.0);
        let y = draw(10.0, 5.0);
        [x, y, draw(20.0, 10.0)]
    };
}

/// Zone damage of `0x494521..0x494813` (damage gate on).
fn apply_zone_damage(car: &mut PairCar, strength: f32, zone: usize, boost: bool) {
    let factor = if boost { 1.5 } else { 0.5 };
    let value = strength as f64 * factor;
    let mut d = if value > 100.0 { 100.0 } else { value as f32 };
    let dmg = &mut car.damage_4e4;
    if zone.is_multiple_of(2) && d > 30.0 && dmg[zone] < 30.0 && car.damage_events_110c.len() < 7 {
        car.damage_events_110c.push(d);
    }
    if dmg[zone] > d {
        d = dmg[zone];
    }
    dmg[zone] = d;
    let spread = |dmg: &mut [f32; 9], target: usize, x: f32, y: f32| {
        let mean = (x as f64 + y as f64) * 0.5;
        if dmg[target] as f64 <= mean {
            dmg[target] = mean as f32;
        }
    };
    match zone {
        0 => {
            spread(dmg, 1, dmg[2], dmg[0]);
            spread(dmg, 7, dmg[6], dmg[0]);
        }
        1 => {
            spread(dmg, 0, dmg[7], dmg[1]);
            spread(dmg, 2, dmg[3], dmg[1]);
        }
        6 => {
            spread(dmg, 5, dmg[4], dmg[6]);
            spread(dmg, 7, dmg[0], dmg[6]);
        }
        7 => {
            spread(dmg, 0, dmg[1], dmg[7]);
            spread(dmg, 6, dmg[5], dmg[7]);
        }
        2..=5 => {
            spread(dmg, zone + 1, d, dmg[zone + 2]);
            spread(dmg, zone - 1, dmg[zone - 2], dmg[zone]);
        }
        _ => {}
    }
}

/// `Car_ClampAngularVelocity` `0x4964d0`: each component of `+0x388` to
/// ±0.75 (±2.25 with resource `+0x518`, ±6.75 for flag-0x80 cars lighter than
/// 0.75). The original caller applies it in turns/s.
pub fn clamp_angular_velocity(car: &mut PairCar) {
    let limit = if car.resource_518 {
        2.25
    } else if car.object_vehicle() && car.mass_354 < 0.75 {
        6.75
    } else {
        0.75
    };
    car.angular_388 = car.angular_388.map(|w| w.clamp(-limit, limit));
}

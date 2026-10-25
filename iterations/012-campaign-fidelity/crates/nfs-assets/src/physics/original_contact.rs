//! Prepared vehicle contact response from `Porsche.exe` SHA256
//! `ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.
//!
//! Source: `research/original-collision/response.md` and the instruction exports
//! for `0x4940ae..0x49439f`, `0x493f10..0x493ff3`, `0x493e40..0x493f00`,
//! and `0x493dd0..0x493e3f`. The caller must supply an already accepted normal
//! and prepared correction. Scene acceptance, surface effects and orientation
//! integration remain outside this kernel. See angular-field.md: +0x38c drives
//! Y rotation by field/64 turns per orientation update; wall-clock cadence is open.

/// Car fields touched by the proven response slice. Names retain original
/// offsets where the gameplay meaning has not been established.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedContactState {
    pub position_330: [f32; 3],
    pub velocity_33c: [f32; 3],
    pub speed_35c: f32,
    pub basis_364: [f32; 3],
    pub basis_370: [f32; 3],
    pub basis_37c: [f32; 3],
    /// Y angular state; preserve original units (not radians/second).
    pub field_38c: f32,
    pub field_434: f32,
    pub basis_490: [f32; 3],
    pub basis_4a8: [f32; 3],
    pub local_velocity_d58: [f32; 3],
    pub wheel_points_7f8: [[f32; 3]; 4],
    /// The four wheel fields at +0x870, +0x934, +0x9f8, +0xabc.
    pub wheel_fields_870: [f32; 4],
    pub field_db8: f32,
    pub field_dc0: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedContact {
    pub normal: [f32; 3],
    pub correction: [f32; 3],
    /// Whether car+0x52c has mask 0x04 set.
    pub alternate_52c_mask_04: bool,
    /// Whether the caller requests the 0x493f10 angular callback.
    pub angular_callback: bool,
}

fn dot_extended(a: [f32; 3], b: [f32; 3]) -> f64 {
    a[0] as f64 * b[0] as f64 + a[1] as f64 * b[1] as f64 + a[2] as f64 * b[2] as f64
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    dot_extended(a, b) as f32
}

fn speed_xz(v: [f32; 3]) -> f32 {
    let x = v[0].abs() as f64;
    let z = v[2].abs() as f64;
    (x.max(z) + x.min(z) * 0.25) as f32
}

fn angular_response(state: &mut PreparedContactState, normal: [f32; 3], strength: f32) {
    // 0x493f1f clears this field even when the following gate preserves +0x38c.
    state.field_434 = 0.0;
    // The test uses the OLD +0xd60 projection: refresh at 0x493e40 is later.
    if let Some(yaw) = yaw_kick(
        strength,
        state.local_velocity_d58[2],
        normal,
        state.basis_4a8,
        state.basis_490,
    ) {
        state.field_38c = yaw;
    }
}

/// The `+0x38c` write of `0x493f10`: `None` when the gate (`strength >= 15`
/// and the old `+0xd60 >= 20`) keeps the field.
pub(crate) fn yaw_kick(
    strength: f32,
    local_forward_d60: f32,
    normal: [f32; 3],
    basis_4a8: [f32; 3],
    basis_490: [f32; 3],
) -> Option<f32> {
    if strength >= 15.0 && local_forward_d60 >= 20.0 {
        return None;
    }
    let a = dot(normal, basis_4a8);
    let b = dot(normal, basis_490);
    let mut value = (a.abs().min(b.abs()) as f64 * 0.25) as f32;
    if a < 0.0 {
        if b < 0.0 {
            value = -value;
        }
    } else {
        if b > 0.0 {
            value = -value;
        }
        value = (value as f64 * 0.5) as f32;
    }
    Some(value)
}

/// `0x493e40` without the `+0x52c & 4` branch: velocity x0.998, the local
/// velocity `+0xd58..` and the speed estimate `+0x35c`.
pub(crate) fn damp_velocity(velocity: &mut [f32; 3], basis: [[f32; 3]; 3]) -> ([f32; 3], f32) {
    for component in velocity.iter_mut() {
        *component = (*component as f64 * f32::from_bits(0x3f7f7cee) as f64) as f32;
    }
    let local = basis.map(|axis| dot(*velocity, axis));
    (local, speed_xz(*velocity))
}

/// Apply only the response after the original caller's geometry preparation.
/// Returns the original scalar magnitude. Positive-branch arithmetic uses f64
/// intermediates for x87 operations and rounds at the observed f32 stores.
pub fn respond_prepared_contact(state: &mut PreparedContactState, contact: PreparedContact) -> f32 {
    // 0x4940bd stores a rounded copy, but 0x4940c1 compares the still-live
    // x87 value. f64 is an approximation of the x87 extended accumulator.
    let unrounded_strength = -dot_extended(state.velocity_33c, contact.normal);
    let strength = unrounded_strength as f32;
    if unrounded_strength <= 0.0 {
        return 0.0;
    }
    let old_speed = state.speed_35c;
    let reported = if old_speed as f64 * 0.125 > strength as f64 {
        (old_speed as f64 * f32::from_bits(0x3daaaaab) as f64) as f32
    } else {
        strength
    };
    let impulse = (strength as f64 * f32::from_bits(0x3f8ccccd) as f64) as f32;
    for axis in 0..3 {
        // 0x532360 writes each component of the scaled normal to a float
        // stack vector before 0x49420b adds it to the stored velocity.
        let delta = (contact.normal[axis] as f64 * impulse as f64) as f32;
        state.velocity_33c[axis] = (state.velocity_33c[axis] as f64 + delta as f64) as f32;
        state.position_330[axis] =
            (state.position_330[axis] as f64 + contact.correction[axis] as f64) as f32;
    }
    for point in &mut state.wheel_points_7f8 {
        for &axis in &[0, 2] {
            point[axis] = (point[axis] as f64 + contact.correction[axis] as f64) as f32;
        }
    }
    if contact.angular_callback {
        angular_response(state, contact.normal, strength);
    }
    (state.local_velocity_d58, state.speed_35c) = damp_velocity(
        &mut state.velocity_33c,
        [state.basis_364, state.basis_370, state.basis_37c],
    );
    if contact.alternate_52c_mask_04 {
        state.local_velocity_d58[0] = dot(state.velocity_33c, state.basis_490);
        state.local_velocity_d58[2] = dot(state.velocity_33c, state.basis_4a8);
        state.wheel_fields_870 = [0.0; 4];
        state.field_db8 = 0.0;
        state.field_dc0 = 0.0;
    }
    (reported as f64 * 3.0).abs() as f32
}

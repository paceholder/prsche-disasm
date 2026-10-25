//! Bounded original wheel height branch and material displacement state.
//!
//! Porsche.exe SHA256 ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39.
//! 0x499bb8 calls 0x494480; 0x499c7f calls 0x499730. This module does not
//! implement the remaining 0x499a70 force/body response loop.

/// 0x494480's result selecting the height computation in 0x499a70.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HeightBranch {
    PolygonPlane = 0,
    AlternateNoMaterial = 1,
    AlternateFar = 2,
}

/// The source compares the polygon plane against car+0x334. A material low
/// nibble of zero returns 1 immediately; a nonzero nibble returns 0 or 2.
/// `plane_height` is the height from the owner polygon point/normal at the
/// current query XZ. The caller owns polygon selection and plane arithmetic.
pub fn height_branch(
    material_flags: u32,
    reference_height: f32,
    plane_height: f32,
) -> HeightBranch {
    if material_flags & 15 == 0 {
        return HeightBranch::AlternateNoMaterial;
    }
    if ((plane_height as f64 - reference_height as f64).abs()) > 4.0 {
        HeightBranch::AlternateFar
    } else {
        HeightBranch::PolygonPlane
    }
}

/// The material height state at car+0x82c/+0x830/+0x834, one 0xc4-byte
/// record per wheel. `rng_seed` models the original process-global +0x5d1028.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialHeightState {
    pub previous: f32,
    pub current: f32,
    pub phase: f32,
}

/// Process state written by the original random step. +0x655a0c records the
/// full product even when material amplitude is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialRandomState {
    pub seed_5d1028: u32,
    pub multiplier_5d102c: u32,
    pub product_655a0c: u32,
}

/// Execute the bounded 0x499730 transition with explicit process-global RNG
/// fields. The original uses x87 intermediates and observed f32 field stores.
pub fn advance_material_height(
    state: &mut MaterialHeightState,
    car_gate: f32,
    car_speed: f32,
    material_flags: u32,
    rng: &mut MaterialRandomState,
) -> f32 {
    advance_material_height_unrounded(state, car_gate, car_speed, material_flags, rng) as f32
}

// 0x499730 returns an x87 value. A caller may add it before storing to f32.
pub(super) fn advance_material_height_unrounded(
    state: &mut MaterialHeightState,
    car_gate: f32,
    car_speed: f32,
    material_flags: u32,
    rng: &mut MaterialRandomState,
) -> f64 {
    if car_gate < f32::from_bits(0x3f5c28f6) {
        return 0.0;
    }
    let (period, amplitude): (f32, f32) = match material_flags & 15 {
        3 | 9 | 5 => (2.5, f32::from_bits(0x3dcccccd)),
        2 | 7 => (0.25, f32::from_bits(0x3cf5c28f)),
        4 | 10 => (0.5, f32::from_bits(0x3cf5c28f)),
        _ => (0.5, 0.0),
    };
    let unrounded_phase = car_speed as f64 * 0.03125 + state.phase as f64;
    state.phase = unrounded_phase as f32;
    // 0x4997de stores a rounded phase but compares the still-live x87 value.
    // Equality at the period does not advance the RNG; strict > does.
    if unrounded_phase > period as f64 || amplitude < f32::from_bits(0x3c23d70a) {
        state.phase = 0.0;
        state.previous = state.current;
        let product = super::original_lcg::step(&mut rng.seed_5d1028, rng.multiplier_5d102c);
        rng.product_655a0c = product;
        let random_word = super::original_lcg::word(product);
        state.current = (amplitude as f64 * f32::from_bits(0x3e2aaaab) as f64
            - random_word as f64 * amplitude as f64 * (1.0 / 65536.0))
            as f32;
    }
    (state.current as f64 - state.previous as f64) * (state.phase as f64 / period as f64)
        + state.previous as f64
}

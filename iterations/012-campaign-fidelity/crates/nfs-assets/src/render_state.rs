//! Original material render state: the `sR` layer block `{shade, blend enable, blend mode,
//! cull, z bias, -1}` through `Render_ApplyLayerState` 0x540660 and its setters, then the
//! THRASH render driver (`Drivers/dx7/dx7z.dll` `THRASH_setstate@8` 0x60003ca0,
//! `THRASH_getstate@4` 0x60001d80) down to the Direct3D 7 calls.
//!
//! Recorded: `scripts/research/replay-render-state.py` executes both binaries in Unicorn with a
//! logging D3D device; `tests/original_render_state.rs` replays every row bit for bit (THRASH
//! calls, D3D calls, cache, driver z offset). Only the THRASH ids and values listed here are
//! ported; anything else is `unimplemented` (the recorded domain).

/// THRASH ids of the six `sR` fields (table 0x5ded30, stride 8; setters 0x5ded60).
pub const LAYER_STATE_IDS: [u32; 6] = [6, 0xa, 0x38, 2, 0x18, 0];

/// Initial cache word the recorder writes so that every field is applied.
pub const CACHE_SENTINEL: i32 = 0x7fff_ffff;

/// `D3DRENDERSTATETYPE` values the driver sets.
pub const D3DRS_ZENABLE: u32 = 7;
pub const D3DRS_SHADEMODE: u32 = 9;
pub const D3DRS_ZWRITEENABLE: u32 = 14;
pub const D3DRS_ALPHATESTENABLE: u32 = 15;
pub const D3DRS_ZFUNC: u32 = 23;
pub const D3DRS_ALPHAREF: u32 = 24;
pub const D3DRS_ALPHAFUNC: u32 = 25;
pub const D3DRS_SRCBLEND: u32 = 19;
pub const D3DRS_DESTBLEND: u32 = 20;
pub const D3DRS_CULLMODE: u32 = 22;
pub const D3DRS_ALPHABLENDENABLE: u32 = 27;
pub const D3DRS_SPECULARENABLE: u32 = 29;
/// `D3DCULL`.
pub const D3DCULL_NONE: u32 = 1;
/// `D3DCMPFUNC`.
pub const D3DCMP_GREATER: u32 = 5;
pub const D3DCMP_LESSEQUAL: u32 = 4;
pub const D3DCMP_ALWAYS: u32 = 8;
/// `D3DBLEND`.
pub const D3DBLEND_SRCALPHA: u32 = 5;
pub const D3DBLEND_INVSRCALPHA: u32 = 6;
/// `D3DTEXTURESTAGESTATETYPE` / `D3DTEXTUREOP` / `D3DTA`.
pub const D3DTSS_COLOROP: u32 = 1;
pub const D3DTSS_COLORARG1: u32 = 2;
pub const D3DTSS_COLORARG2: u32 = 3;
pub const D3DTSS_ALPHAOP: u32 = 4;
pub const D3DTOP_MODULATE: u32 = 4;
pub const D3DTA_DIFFUSE: u32 = 0;
pub const D3DTA_TEXTURE: u32 = 2;

/// THRASH 0x18 z bias scale of the driver: offset added to every screen z (`sz` of the
/// transformed vertex, 0x6000706e) = value · 2^-15 [dx7z.dll 0x60005544, data 0x60015444].
pub const DRIVER_Z_SCALE: f32 = 1.0 / 32768.0;
/// Scale of the deferred z bias `[0x69e0d4]+0x1180c` (0x540529, data 0x5b3e10).
pub const DEFERRED_Z_SCALE: f32 = 1.0 / 32768.0;

/// `D3DBLEND` of THRASH 0x194 (source) / 0x195 (destination) values 0..9 [dx7z.dll].
const BLEND_FACTOR: [u32; 10] = [2, 1, 5, 6, 7, 8, 3, 9, 4, 10];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D3dCall {
    /// `IDirect3DDevice7::SetRenderState(state, value)`.
    RenderState(u32, u32),
    /// `IDirect3DDevice7::SetTextureStageState(stage, type, value)`.
    TextureStage(u32, u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrashCall {
    Set(u32, i32),
    Get(u32),
}

/// Device capability words of the driver that the recorded paths read (zero until
/// `THRASH_init` enumerates the device).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriverCaps {
    /// `[0x6005ab2c]`: THRASH 0x29 = 8 modulates the alpha too (else SELECTARG1 texture).
    pub modulate_alpha: bool,
    /// `[0x60058d78]` 1 or 3: blend mode 1 is ONE, ONE (else SRCALPHA, ONE).
    pub additive_one_one: bool,
    /// `[0x6005ab44]`: shade mode 2 (Gouraud + specular) is accepted.
    pub gouraud_specular: bool,
    /// `[0x6005a960]` / `[0x6005a990]`: the per-stage word THRASH 0x29 = 8 returns.
    pub stage_ready: bool,
    /// `[0x6005aaf4]`: the device has a z buffer (THRASH 4 accepted).
    pub z_buffer: bool,
    /// `[0x6005aba0]`: alpha compare (THRASH 0x24 accepted).
    pub alpha_compare: bool,
    /// `[0x6005abe0 + 4·f]`, f = 0..7: every z compare function (THRASH 0x28 accepted).
    pub compare_functions: bool,
}

impl DriverCaps {
    /// A Direct3D 7 HAL device with a z buffer, alpha compare and every compare function: what
    /// `THRASH_init` reports on any card the game runs on (the recorded car draws use it).
    pub const HAL: DriverCaps = DriverCaps {
        modulate_alpha: false,
        additive_one_one: false,
        gouraud_specular: false,
        stage_ready: false,
        z_buffer: true,
        alpha_compare: true,
        compare_functions: true,
    };
}

/// The driver's state as far as the ported THRASH ids reach.
#[derive(Clone, Debug, Default)]
pub struct Dx7Driver {
    pub caps: DriverCaps,
    /// `THRASH_getstate` table (0x6007b8a0), written only by successful sets.
    states: std::collections::BTreeMap<u32, i32>,
    /// `[0x6001885c]`.
    pub z_offset: f32,
    /// `[0x600170d4]`: the `D3DCMPFUNC` THRASH 4 = 1 sends as z function (THRASH 0x28 sets it;
    /// `None` = the file's value, `LESSEQUAL`).
    z_func: Option<u32>,
    pub thrash: Vec<ThrashCall>,
    pub d3d: Vec<D3dCall>,
}

impl Dx7Driver {
    pub fn new(caps: DriverCaps) -> Self {
        Self {
            caps,
            ..Self::default()
        }
    }

    /// `THRASH_getstate@4` (logged).
    pub fn get_state(&mut self, id: u32) -> i32 {
        self.thrash.push(ThrashCall::Get(id));
        self.state(id)
    }

    /// The getstate table entry without a call.
    pub fn state(&self, id: u32) -> i32 {
        self.states.get(&id).copied().unwrap_or(0)
    }

    /// `THRASH_setstate@8`: returns the driver's result (0 = refused, state unchanged).
    pub fn set_state(&mut self, id: u32, value: i32) -> u32 {
        self.thrash.push(ThrashCall::Set(id, value));
        let stage = id >> 16;
        let rs = |d: &mut Self, s: u32, v: u32| d.d3d.push(D3dCall::RenderState(s, v));
        let ok = match (id & 0xffff, value) {
            (2, 0) => {
                rs(self, D3DRS_CULLMODE, D3DCULL_NONE);
                1
            }
            (2, 1) => {
                rs(self, D3DRS_CULLMODE, 3);
                1
            }
            (2, 2) => {
                rs(self, D3DRS_CULLMODE, 2);
                1
            }
            (6, 0 | 1) => {
                rs(self, D3DRS_SHADEMODE, value as u32 + 1);
                rs(self, D3DRS_SPECULARENABLE, 0);
                1
            }
            (6, 2) if self.caps.gouraud_specular => {
                rs(self, D3DRS_SHADEMODE, 2);
                rs(self, D3DRS_SPECULARENABLE, 1);
                1
            }
            (0xa, 0) => {
                rs(self, D3DRS_ALPHABLENDENABLE, 0);
                1
            }
            (0xa, 2) => {
                rs(self, D3DRS_ALPHABLENDENABLE, 1);
                1
            }
            (0x18, _) => {
                // fild value; fcomp with the stored offset; store value·2^-15 when they differ.
                if f64::from(self.z_offset) != f64::from(value) {
                    self.z_offset = (f64::from(value) * f64::from(DRIVER_Z_SCALE)) as f32;
                }
                1
            }
            (0x38, 0..=3) => {
                let (src, dst) = match value {
                    0 => (D3DBLEND_SRCALPHA, D3DBLEND_INVSRCALPHA),
                    1 if self.caps.additive_one_one => (2, 2),
                    1 => (D3DBLEND_SRCALPHA, 2),
                    2 => (1, D3DBLEND_INVSRCALPHA),
                    _ => (9, 1),
                };
                rs(self, D3DRS_SRCBLEND, src);
                rs(self, D3DRS_DESTBLEND, dst);
                1
            }
            (0x194 | 0x195, _) => {
                if let Some(&f) = usize::try_from(value)
                    .ok()
                    .and_then(|v| BLEND_FACTOR.get(v))
                {
                    let s = if id & 0xffff == 0x194 {
                        D3DRS_SRCBLEND
                    } else {
                        D3DRS_DESTBLEND
                    };
                    rs(self, s, f);
                }
                1
            }
            (0x29, 0 | 8) => self.combine(stage, value),
            // Alpha test: 0 = off (function ALWAYS), else on with the reference = value and the
            // function `[0x600170d8]` (GREATER in the file; THRASH 0x25 is refused) [0x60004188].
            (0x24, 0) if self.caps.alpha_compare => {
                rs(self, D3DRS_ALPHATESTENABLE, 0);
                rs(self, D3DRS_ALPHAFUNC, D3DCMP_ALWAYS);
                1
            }
            (0x24, 1..=255) if self.caps.alpha_compare => {
                rs(self, D3DRS_ALPHATESTENABLE, 1);
                rs(self, D3DRS_ALPHAFUNC, D3DCMP_GREATER);
                rs(self, D3DRS_ALPHAREF, value as u32);
                1
            }
            (0x24, 0..=255) => 0,
            (0x25, 0..=8) => 0,
            // z function 0..7 = D3DCMP_NEVER..ALWAYS, kept for THRASH 4 [0x60004f30].
            (0x28, 0..=7) if self.caps.compare_functions => {
                self.z_func = Some(value as u32 + 1);
                rs(self, D3DRS_ZFUNC, value as u32 + 1);
                1
            }
            (0x28, 0..=8) => 0,
            // z buffer: 0 = off (z write off, function ALWAYS), 1 = on with the kept function;
            // 2 (w buffer) sends the same as 1 and fails without its cap [0x60005092].
            (4, 0) if self.caps.z_buffer => {
                rs(self, D3DRS_ZWRITEENABLE, 0);
                rs(self, D3DRS_ZENABLE, 0);
                rs(self, D3DRS_ZFUNC, D3DCMP_ALWAYS);
                1
            }
            (4, 1 | 2) if self.caps.z_buffer => {
                let func = self.z_func.unwrap_or(D3DCMP_LESSEQUAL);
                rs(self, D3DRS_ZWRITEENABLE, 1);
                rs(self, D3DRS_ZENABLE, 1);
                rs(self, D3DRS_ZFUNC, func);
                u32::from(value == 1)
            }
            (4, 0..=2) => 0,
            // z write [0x6000505b] (no cap).
            (0x39, 0 | 1) => {
                rs(self, D3DRS_ZWRITEENABLE, value as u32);
                1
            }
            (2 | 6 | 0xa | 0x38, _) => 0,
            _ => unimplemented!("THRASH state {id:#x} = {value} is outside the recorded domain"),
        };
        if ok != 0 {
            self.states.insert(id, value);
        }
        ok
    }

    /// THRASH 0x29 (texture combine) values 0 and 8 on stages 0 and 1.
    fn combine(&mut self, stage: u32, value: i32) -> u32 {
        let calls: &[(u32, u32)] = match (stage, value) {
            (0, _) if self.caps.modulate_alpha => &[(1, 4), (2, 2), (3, 0), (4, 4), (5, 2), (6, 0)],
            (0, _) => &[(1, 4), (2, 2), (3, 0), (4, 2), (5, 2)],
            (_, 8) => &[(2, 2), (3, 1), (5, 2), (6, 1), (1, 13), (4, 1)],
            _ => &[(1, 1), (2, 2), (3, 1), (4, 1), (5, 2), (6, 1)],
        };
        self.d3d.extend(
            calls
                .iter()
                .map(|&(t, v)| D3dCall::TextureStage(stage, t, v)),
        );
        if value == 0 || self.caps.stage_ready {
            1
        } else {
            0
        }
    }

    /// Last value sent for a render state, if any.
    pub fn render_state(&self, state: u32) -> Option<u32> {
        self.d3d.iter().rev().find_map(|c| match *c {
            D3dCall::RenderState(s, v) if s == state => Some(v),
            _ => None,
        })
    }
}

/// The view's part of the state: cull mode `[[[0x69ed0c]]+4]+0x10` (THRASH value, 2 or 1 by
/// mirror, set per view by 0x537890), z base `+0x30`, and `[0x69e0d4]+0x117e4 & 0x20000`
/// (z bias deferred into `+0x1180c` instead of THRASH 0x18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub cull_mode: i32,
    pub z_base: i32,
    pub defer_z: bool,
}

/// The THRASH cull of the unmirrored race view.
pub const VIEW: View = View {
    cull_mode: 2,
    z_base: 0,
    defer_z: false,
};

/// `Render_ApplyLayerState` 0x540660: applies the fields that differ from `cache`
/// ([0x5ded34], stride 8). Returns the deferred z (`+0x1180c`) when field 4 went there.
pub fn apply_layer_state(
    cache: &mut [i32; 6],
    block: &[i32; 6],
    view: &View,
    driver: &mut Dx7Driver,
) -> Option<f32> {
    let mut deferred = None;
    for k in 0..6 {
        let v = block[k];
        if v == cache[k] {
            continue;
        }
        cache[k] = v;
        let id = LAYER_STATE_IDS[k];
        match k {
            // 0x5405c0: keep the old mode when the driver refuses the new one.
            0 => {
                let old = driver.get_state(id);
                if driver.set_state(id, v) == 0 {
                    driver.set_state(id, old);
                    cache[0] = old;
                }
            }
            1 => {
                driver.set_state(id, v);
            }
            // 0x540550: -1 = DESTCOLOR, SRCCOLOR through the raw factor states.
            2 if v == -1 => {
                driver.set_state(0x194, 7);
                driver.set_state(0x195, 6);
            }
            2 => {
                driver.set_state(id, v);
            }
            // 0x540590: 1 = the view's cull mode, anything else none.
            3 => {
                driver.set_state(id, if v == 1 { view.cull_mode } else { 0 });
            }
            // 0x540500: z bias plus the view's base.
            4 => {
                let z = v.wrapping_add(view.z_base);
                if view.defer_z {
                    deferred = Some((f64::from(z) * f64::from(DEFERRED_Z_SCALE)) as f32);
                } else {
                    driver.set_state(id, z);
                }
            }
            // 0x516950: no-op.
            _ => {}
        }
    }
    deferred
}

/// What the engine draws a material with, from its `sR` block applied on the race view with
/// the default caps: the D3D states the original ends up with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawState {
    /// `D3DRS_CULLMODE` is `D3DCULL_NONE`.
    pub double_sided: bool,
    /// `D3DRS_ALPHABLENDENABLE` with its (`SRCBLEND`, `DESTBLEND`).
    pub blend: Option<(u32, u32)>,
    /// The THRASH 0x18 value (field 4 + the view's base 0); the driver adds `· 2^-15` to the
    /// screen z.
    pub z_bias: i32,
}

impl DrawState {
    /// Standard alpha blending (`SRCALPHA`, `INVSRCALPHA`).
    pub fn alpha_blended(&self) -> bool {
        self.blend == Some((D3DBLEND_SRCALPHA, D3DBLEND_INVSRCALPHA))
    }
}

pub fn draw_state(block: &[i32; 6]) -> DrawState {
    let mut driver = Dx7Driver::new(DriverCaps::default());
    let mut cache = [CACHE_SENTINEL; 6];
    apply_layer_state(&mut cache, block, &VIEW, &mut driver);
    let blend = (driver.render_state(D3DRS_ALPHABLENDENABLE) == Some(1)).then(|| {
        (
            driver.render_state(D3DRS_SRCBLEND).unwrap_or(1),
            driver.render_state(D3DRS_DESTBLEND).unwrap_or(1),
        )
    });
    DrawState {
        double_sided: driver.render_state(D3DRS_CULLMODE) == Some(D3DCULL_NONE),
        blend,
        z_bias: block[4].wrapping_add(VIEW.z_base),
    }
}

/// The single-texture combine of the draws (0x540ce0, 0x5405f0: THRASH 0x29 = 8 on stage 0,
/// 0x10029 = 0 on stage 1): stage 0 `COLOROP MODULATE (TEXTURE, DIFFUSE)`, stage 1 disabled.
pub fn single_texture_combine(caps: DriverCaps) -> Vec<D3dCall> {
    let mut driver = Dx7Driver::new(caps);
    driver.set_state(0x29, 8);
    driver.set_state(0x10029, 0);
    driver.d3d
}

/// The render states a draw ends up with, by `D3DRENDERSTATETYPE` (last value sent).
pub type D3dStates = std::collections::BTreeMap<u32, u32>;

/// The states that decide how a car part covers what is behind it: z, alpha test, blend, cull,
/// shading.
pub const CAR_PART_STATES: [u32; 11] = [
    D3DRS_ZENABLE,
    D3DRS_SHADEMODE,
    D3DRS_ZWRITEENABLE,
    D3DRS_ALPHATESTENABLE,
    D3DRS_SRCBLEND,
    D3DRS_DESTBLEND,
    D3DRS_CULLMODE,
    D3DRS_ZFUNC,
    D3DRS_ALPHAREF,
    D3DRS_ALPHAFUNC,
    D3DRS_ALPHABLENDENABLE,
];

/// The `sR` block of every car render method (`RMth` data): Gouraud, blend enable 2, blend mode
/// 0 (SRCALPHA, INVSRCALPHA), cull 1 (the view's mode), z bias 0.
pub const CAR_METHOD_BLOCK: [i32; 6] = [1, 2, 0, 1, 0, -1];

/// The THRASH state a car part is drawn with in the race view: the view's render state object
/// (constructor 0x537700: 0x28 = 3, 4 = 1, ..., 0x39 = 1), 0x468ae0 before the scene
/// (0x537870(1): 0x39 = 1, 0x537830(3): 0x28 = 3), `R3DCarLOD_DrawParts` 0x43c830 (0x24 = 10),
/// then the part's `sR` block (0x540660). Every state another scene draw changes in between
/// (0x39 = 0 of the car shadow 0x43b820, rain 0x459020, ...) is restored by it.
pub fn car_part_states(block: &[i32; 6], caps: DriverCaps) -> D3dStates {
    let mut driver = Dx7Driver::new(caps);
    for (id, value) in [(0x28, 3), (4, 1), (0x39, 1), (0x39, 1), (0x28, 3), (0x24, 10)] {
        driver.set_state(id, value);
    }
    let mut cache = [CACHE_SENTINEL; 6];
    apply_layer_state(&mut cache, block, &VIEW, &mut driver);
    let mut out = D3dStates::new();
    for call in &driver.d3d {
        if let D3dCall::RenderState(s, v) = *call {
            if CAR_PART_STATES.contains(&s) {
                out.insert(s, v);
            }
        }
    }
    out
}

/// The viewer's car mesh pipeline in the same terms (`porsche-viewer` `renderer.rs`: depth
/// test `LessEqual` always, depth write unless `Blend`, `ALPHA_BLENDING` for `Blend` /
/// `BlendTest`, `car.wgsl` discards `alpha <= cutoff` for `Mask` / `BlendTest`, back faces
/// culled unless double-sided; Gouraud vertex colours).
pub fn viewer_car_states(mode: crate::AlphaMode, cutoff: f32, double_sided: bool) -> D3dStates {
    use crate::AlphaMode::*;
    let blend = matches!(mode, Blend | BlendTest);
    let test = matches!(mode, Mask | BlendTest);
    let mut out = D3dStates::from([
        (D3DRS_ZENABLE, 1),
        (D3DRS_SHADEMODE, 2),
        (D3DRS_ZWRITEENABLE, u32::from(mode != Blend)),
        (D3DRS_ZFUNC, D3DCMP_LESSEQUAL),
        (D3DRS_ALPHATESTENABLE, u32::from(test)),
        (D3DRS_ALPHABLENDENABLE, u32::from(blend)),
        (D3DRS_CULLMODE, if double_sided { D3DCULL_NONE } else { 2 }),
    ]);
    if test {
        // discard `a <= cutoff` = pass `round(255 a) > round(255 cutoff)` on 8-bit alpha.
        out.insert(D3DRS_ALPHAFUNC, D3DCMP_GREATER);
        out.insert(D3DRS_ALPHAREF, (cutoff * 255.0).round() as u32);
    } else {
        out.insert(D3DRS_ALPHAFUNC, D3DCMP_ALWAYS);
    }
    if blend {
        out.insert(D3DRS_SRCBLEND, D3DBLEND_SRCALPHA);
        out.insert(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_combine_is_texture_times_diffuse() {
        let calls = single_texture_combine(DriverCaps::default());
        assert_eq!(
            &calls[..3],
            &[
                D3dCall::TextureStage(0, D3DTSS_COLOROP, D3DTOP_MODULATE),
                D3dCall::TextureStage(0, D3DTSS_COLORARG1, D3DTA_TEXTURE),
                D3dCall::TextureStage(0, D3DTSS_COLORARG2, D3DTA_DIFFUSE),
            ]
        );
        assert!(calls.contains(&D3dCall::TextureStage(1, D3DTSS_COLOROP, 1)));
        assert!(calls.contains(&D3dCall::TextureStage(0, D3DTSS_ALPHAOP, 2)));
    }

    #[test]
    fn cull_1_takes_the_view_mode_and_others_none() {
        assert!(!draw_state(&[1, 0, 0, 1, 0, -1]).double_sided);
        assert!(draw_state(&[1, 0, 0, 0, 0, -1]).double_sided);
        assert!(draw_state(&[1, 0, 0, 2, 0, -1]).double_sided);
        assert!(draw_state(&[1, 2, 0, 1, -3, -1]).alpha_blended());
        assert_eq!(draw_state(&[1, 0, 0, 1, 0, -1]).blend, None);
    }
}

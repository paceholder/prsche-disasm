//! Engine telemetry to `EngineAudio_PushState 0x4ac780` arguments, as `CarSound_Update
//! 0x4aa050` builds them for the player car with the listener at the car.
//!
//! Exact parts: RPM over redline in 16.16 (`ftol`), throttle smoothing, the gear-change
//! throttle cut and volume blend, the volume-argument scale. Approximated parts (registered
//! in `docs/approximations.md`): the update rate [`UPDATE_HZ`], the view-mode mapping from our
//! three cameras, the gear-change phase ([`GEAR_CHANGE_STEPS`] updates per upshift), the
//! distance/speed volume [`LISTENER_VOLUME`], and the engine-off / full-load loads, which are
//! never produced.
use super::engine_mixer::{PushStateArgs, ThrottleSmoother};

/// Mixer updates per second (approximation: the original audio-library tick rate is unknown).
pub const UPDATE_HZ: f32 = 100.0;

/// Gear-change phase length in updates; stands in for the per-car `spec+0x54` that the volume
/// blend divides by (approximation).
pub const GEAR_CHANGE_STEPS: i8 = 5;

/// `local_8` of `0x4aa050` for a listener at the car at low speed: distance attenuation 1024
/// (`0x4a6160(0x8000000, 0x20000) >> 16`) × 127 >> 7. The speed term is ignored (approximation).
pub const LISTENER_VOLUME: i32 = 1016;

/// Camera of the browser game (`porsche-viewer` `arcade::DriveViewMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineView {
    Chase,
    Bumper,
    Free,
}

impl EngineView {
    /// Original view mode (`0x4aa050` camera switch). Bumper is the in-car view: mode 0, the
    /// only mode that uses the cockpit (`.cct`/`.clt`) tables. Chase follows the car like
    /// `TailCam0..2`: mode 2. Free is any other exterior camera: mode 3.
    pub fn original_mode(self) -> i32 {
        match self {
            EngineView::Bumper => 0,
            EngineView::Chase => 2,
            EngineView::Free => 3,
        }
    }
}

/// Per-frame engine telemetry of the player car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineTelemetry {
    pub rpm: f32,
    /// SIM redline (`spec+0x94`, `SimCar::redline_rpm`).
    pub redline_rpm: f32,
    /// Throttle pedal 0..1.
    pub throttle: f32,
    /// Current gear (−1 R, 0 N, 1.. forward).
    pub gear: i32,
    pub view: EngineView,
}

/// `ftol(rpm / redline × 65536)`: RPM over redline, 16.16, truncated toward zero.
pub fn rpm16(rpm: f32, redline_rpm: f32) -> i32 {
    if redline_rpm <= 0.0 {
        return 0;
    }
    (rpm as f64 / redline_rpm as f64 * 65536.0) as i32
}

/// Throttle pedal 0..1 as the car's throttle byte (`car+0xD7F`), 0..255.
pub fn throttle_byte(throttle: f32) -> u8 {
    (throttle.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Gear-change throttle cut of `0x4aa050` (applied while shifting up and the smoothed throttle
/// is nonzero): phase 5 → 3/4, 4 → 1/2, 3 → 1/4, below 3 → 0, above 5 unchanged. The result
/// replaces the stored smoothed throttle.
pub fn gear_change_cut(throttle: i32, phase: i8) -> i32 {
    match phase {
        5 => throttle - (throttle >> 2),
        4 => throttle >> 1,
        3 => throttle >> 2,
        p if p < 3 => 0,
        _ => throttle,
    }
}

/// Volume blend of `0x4aa050` during a gear change: `((phase × v) / steps + v) >> 1`.
pub fn gear_change_volume(volume: i32, phase: i8, steps: i32) -> i32 {
    ((phase as i32).wrapping_mul(volume) / steps).wrapping_add(volume) >> 1
}

/// Volume argument of `0x4ac780`: `((127 × 20 × 0x55 >> 7) × v) >> 14`
/// (`g_SfxVolumeCap 0x5d1748` × `0x5d14c8`).
pub fn volume_argument(volume: i32) -> i32 {
    const SCALE: i32 = (127 * 20 * 0x55) >> 7;
    SCALE.wrapping_mul(volume) >> 14
}

/// Turns telemetry into producer arguments, one call per mixer update.
#[derive(Debug, Clone, Default)]
pub struct EngineSoundInputs {
    throttle: ThrottleSmoother,
    /// Gear-change phase (`car+0xD84`): [`GEAR_CHANGE_STEPS`] down to 1 during an upshift.
    gear_phase: i8,
    last_gear: Option<i32>,
}

impl EngineSoundInputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Smoothed throttle 0..255 as stored by the original.
    pub fn smoothed_throttle(&self) -> i32 {
        self.throttle.value
    }

    /// Current gear-change phase (0 = not shifting).
    pub fn gear_phase(&self) -> i8 {
        self.gear_phase
    }

    pub fn step(&mut self, t: &EngineTelemetry) -> PushStateArgs {
        // Upshift from a forward gear starts a phase; the original skips shifts out of
        // gear 1 (`car+0xD81 != 1`, hypothesis: neutral), here neutral is 0.
        if let Some(last) = self.last_gear {
            if t.gear > last && last >= 1 {
                self.gear_phase = GEAR_CHANGE_STEPS;
            }
        }
        self.last_gear = Some(t.gear);

        let mut throttle = self.throttle.step(throttle_byte(t.throttle));
        let mut volume = LISTENER_VOLUME;
        if self.gear_phase != 0 && throttle != 0 {
            volume = gear_change_volume(volume, self.gear_phase, GEAR_CHANGE_STEPS as i32);
            throttle = gear_change_cut(throttle, self.gear_phase);
            self.throttle.value = throttle;
        }
        if self.gear_phase > 0 {
            self.gear_phase -= 1;
        }

        PushStateArgs {
            volume: volume_argument(volume),
            rpm16: rpm16(t.rpm, t.redline_rpm),
            // Engine off (−1) and full load (127) are never produced.
            load: throttle >> 1,
            mode: t.view.original_mode(),
            doppler: 0x10000,
            pan: 0,
            front_back: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn telemetry(throttle: f32, gear: i32, view: EngineView) -> EngineTelemetry {
        EngineTelemetry {
            rpm: 3000.0,
            redline_rpm: 6000.0,
            throttle,
            gear,
            view,
        }
    }

    #[test]
    fn view_modes_follow_original_cameras() {
        assert_eq!(EngineView::Bumper.original_mode(), 0);
        assert_eq!(EngineView::Chase.original_mode(), 2);
        assert_eq!(EngineView::Free.original_mode(), 3);
        let mut inputs = EngineSoundInputs::new();
        for (view, mode) in [
            (EngineView::Bumper, 0),
            (EngineView::Chase, 2),
            (EngineView::Free, 3),
        ] {
            assert_eq!(inputs.step(&telemetry(0.0, 1, view)).mode, mode);
        }
    }

    #[test]
    fn only_bumper_uses_cockpit_tables() {
        // Mode 0 selects voices 16..31 (.cct/.clt) in EngineAudio_Mix 0x4ac290.
        for view in [EngineView::Chase, EngineView::Free] {
            assert_ne!(view.original_mode(), 0);
        }
    }

    #[test]
    fn rpm_and_throttle_scaling() {
        assert_eq!(rpm16(6000.0, 6000.0), 0x10000);
        assert_eq!(rpm16(3000.0, 6000.0), 0x8000);
        assert_eq!(rpm16(1000.0, 0.0), 0);
        assert_eq!(
            (throttle_byte(0.0), throttle_byte(1.0), throttle_byte(2.0)),
            (0, 255, 255)
        );
        assert_eq!(volume_argument(LISTENER_VOLUME), 104);
    }

    #[test]
    fn gear_change_cuts_throttle_and_blends_volume() {
        assert_eq!(
            [6, 5, 4, 3, 2, 0].map(|p| gear_change_cut(200, p)),
            [200, 150, 100, 50, 0, 0]
        );
        assert_eq!(gear_change_volume(1016, 5, 5), 1016);
        assert_eq!(gear_change_volume(1016, 1, 5), (203 + 1016) >> 1);

        let mut inputs = EngineSoundInputs::new();
        for _ in 0..20 {
            inputs.step(&telemetry(1.0, 2, EngineView::Chase));
        }
        assert_eq!(inputs.smoothed_throttle(), 255);
        let loads: Vec<i32> = (0..6)
            .map(|_| inputs.step(&telemetry(1.0, 3, EngineView::Chase)).load)
            .collect();
        // Phase 5..1 cut the freshly smoothed value to 3/4, 1/2, 1/4, 0, 0 (stored, so the
        // smoothing restarts from the cut value): 255→192, 223→111, 183→45, 150→0, 127→0.
        assert_eq!(loads, vec![96, 55, 22, 0, 0, 63]);
        assert_eq!(inputs.gear_phase(), 0);
    }

    #[test]
    fn no_cut_from_neutral_or_on_downshift() {
        let mut inputs = EngineSoundInputs::new();
        inputs.step(&telemetry(1.0, 0, EngineView::Chase));
        inputs.step(&telemetry(1.0, 1, EngineView::Chase));
        assert_eq!(inputs.gear_phase(), 0);
        inputs.step(&telemetry(1.0, 3, EngineView::Chase));
        assert_eq!(inputs.gear_phase(), GEAR_CHANGE_STEPS - 1);
        let mut inputs = EngineSoundInputs::new();
        inputs.step(&telemetry(1.0, 3, EngineView::Chase));
        inputs.step(&telemetry(1.0, 2, EngineView::Chase));
        assert_eq!(inputs.gear_phase(), 0);
    }
}

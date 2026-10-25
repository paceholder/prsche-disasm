//! In-car animation channels of `R3DCar_SetAnimChannels` 0x438440 (`docs/re/cockpit.md`):
//! the steering wheel (transform channel 5), the rev and speed needles (channels 0xb, 0xc) and
//! the hand/arm frames (frame channels 5 and 4).

/// Transform channel of the steering wheel (`Base+0x4b`).
pub const CHANNEL_STEERING_WHEEL: u8 = 5;
/// Transform channel of the rev counter needle.
pub const CHANNEL_TACH: u8 = 0xb;
/// Transform channel of the speedometer needle.
pub const CHANNEL_SPEEDO: u8 = 0xc;
/// Frame channels (`Base+0x4d`) of the right and left arm/hand.
pub const FRAME_RIGHT_HAND: u8 = 4;
pub const FRAME_LEFT_HAND: u8 = 5;

/// Steer units per turn of the drawn wheel (`k = 1/216`, 0x438440).
const STEER_PER_TURN: f32 = 216.0;
/// Steering wheel limit in turns (`0.19565217` = 4.5/23).
const WHEEL_LIMIT: f32 = 0.195_652_17;

/// Gauge calibration, `nfs5.car` record `+0x2f0..+0x2fc` (`res+0x524..0x530`), in turns.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Gauges {
    /// `.TACH0RPM`: rev needle at 0 rpm.
    pub tach_0: f32,
    /// `.TACH1000RPM`: per 1000 rpm.
    pub tach_per_1000: f32,
    /// `.SPEEDO0KM`: speed needle at 0.
    pub speedo_0: f32,
    /// `.SPEEDO100KM`: per unit of `0.144·v` (v in m/s).
    pub speedo_per_unit: f32,
}

impl Gauges {
    pub fn from_record(values: [f32; 4]) -> Self {
        let [tach_0, tach_per_1000, speedo_0, speedo_per_unit] =
            values.map(|v| if v.is_finite() { v } else { 0.0 });
        Self {
            tach_0,
            tach_per_1000,
            speedo_0,
            speedo_per_unit,
        }
    }
}

/// Car state the channels read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CockpitInputs {
    /// Front wheel steer `w+0xb4` (or the smoothed copies `+0xac8/+0xacc`), steer units.
    pub steer: [f32; 2],
    /// Engine rpm `+0xdb4`.
    pub rpm: f32,
    /// `+0xec4` (see [`speed_ec4`]), m/s.
    pub speed: f32,
    /// Mirror mode `[0x65742c]`: steer negated.
    pub mirror: bool,
}

/// Channel values: rotations in turns about the part's local z, frames as floats.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CockpitChannels {
    pub steering_wheel: f32,
    pub tach: f32,
    pub speedo: f32,
    pub left_hand_frame: f32,
    pub right_hand_frame: f32,
}

impl CockpitChannels {
    /// Rotation (turns) of a transform channel; 0 for channels not driven here (doors closed).
    pub fn rotation(&self, channel: u8) -> f32 {
        match channel {
            CHANNEL_STEERING_WHEEL => self.steering_wheel,
            CHANNEL_TACH => self.tach,
            CHANNEL_SPEEDO => self.speedo,
            _ => 0.0,
        }
    }

    /// Frame of a frame channel, clamped to `[0, frames-1]` like `R3DCar_SetFrame` 0x4380d0.
    pub fn frame(&self, channel: u8, frames: u8) -> f32 {
        let f = match channel {
            FRAME_LEFT_HAND => self.left_hand_frame,
            FRAME_RIGHT_HAND => self.right_hand_frame,
            _ => 0.0,
        };
        f.clamp(0.0, f32::from(frames.max(1) - 1))
    }
}

/// `+0xec4` (`Car_UpdateSpeedEc4` 0x40eec0): `max + 0.25·min` of `|v.x|`, `|v.z|`.
pub fn speed_ec4(velocity: [f32; 3]) -> f32 {
    let (x, z) = (velocity[0].abs(), velocity[2].abs());
    if z < x {
        z * 0.25 + x
    } else {
        x * 0.25 + z
    }
}

/// `R3DCar_SetAnimChannels` 0x438440, the in-car channels. The right hand plays the steering
/// frame like the left; its gear-change sequences and the frame-19 hold are not ported
/// (approximation A60).
pub fn cockpit_channels(inputs: &CockpitInputs, gauges: &Gauges) -> CockpitChannels {
    let k = if inputs.mirror { -1.0 } else { 1.0 } / STEER_PER_TURN;
    let wheel = (k * inputs.steer[0] + k * inputs.steer[1]).clamp(-WHEEL_LIMIT, WHEEL_LIMIT);
    let frame = wheel * 23.0 + 4.5;
    CockpitChannels {
        steering_wheel: wheel,
        tach: gauges.tach_per_1000 * inputs.rpm * 0.001 + gauges.tach_0,
        speedo: gauges.speedo_per_unit * inputs.speed * 0.144 + gauges.speedo_0,
        left_hand_frame: frame,
        right_hand_frame: frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G356A: Gauges = Gauges {
        tach_0: -0.07,
        tach_per_1000: 0.14,
        speedo_0: -0.01,
        speedo_per_unit: 0.17,
    };

    #[test]
    fn full_lock_turns_the_wheel_70_degrees_and_shows_frame_9() {
        let c = cockpit_channels(
            &CockpitInputs {
                steer: [21.13, 21.13],
                ..Default::default()
            },
            &G356A,
        );
        assert!((c.steering_wheel - WHEEL_LIMIT).abs() < 1e-4);
        assert!((c.frame(FRAME_LEFT_HAND, 10) - 9.0).abs() < 1e-3);
        // Beyond the lock the wheel is clamped; the other way and mirrored it is negative.
        let c = cockpit_channels(
            &CockpitInputs {
                steer: [40.0, 40.0],
                ..Default::default()
            },
            &G356A,
        );
        assert_eq!(c.steering_wheel, WHEEL_LIMIT);
        let c = cockpit_channels(
            &CockpitInputs {
                steer: [10.0, 10.0],
                mirror: true,
                ..Default::default()
            },
            &G356A,
        );
        assert!((c.steering_wheel + 20.0 / 216.0).abs() < 1e-6);
        assert_eq!(c.frame(FRAME_LEFT_HAND, 10), 20.0 / 216.0 * -23.0 + 4.5);
    }

    #[test]
    fn straight_ahead_is_the_middle_frame() {
        let c = cockpit_channels(&CockpitInputs::default(), &G356A);
        assert_eq!(c.steering_wheel, 0.0);
        assert_eq!(c.frame(FRAME_LEFT_HAND, 10), 4.5);
        assert_eq!(c.frame(FRAME_RIGHT_HAND, 20), 4.5);
        assert_eq!(c.frame(1, 6), 0.0);
    }

    #[test]
    fn needles_at_idle_and_speed() {
        let c = cockpit_channels(
            &CockpitInputs {
                rpm: 1000.0,
                ..Default::default()
            },
            &G356A,
        );
        assert!((c.tach - 0.07).abs() < 1e-6);
        assert!((c.speedo + 0.01).abs() < 1e-6);
        // 100 km/h straight ahead: v = 27.78 m/s, 0.144·v = 4.0 units.
        let v = speed_ec4([0.0, 0.0, 100.0 / 3.6]);
        let c = cockpit_channels(
            &CockpitInputs {
                speed: v,
                ..Default::default()
            },
            &G356A,
        );
        assert!((c.speedo - (0.17 * 4.0 - 0.01)).abs() < 1e-5);
        assert_eq!(c.rotation(CHANNEL_SPEEDO), c.speedo);
        assert_eq!(c.rotation(CHANNEL_TACH), c.tach);
        assert_eq!(c.rotation(8), 0.0);
    }

    #[test]
    fn speed_ec4_is_max_plus_quarter_min() {
        assert_eq!(speed_ec4([3.0, 9.0, -4.0]), 4.0 + 0.75);
        assert_eq!(speed_ec4([-8.0, 0.0, 2.0]), 8.5);
        assert_eq!(Gauges::from_record([f32::NAN, 1.0, 2.0, 3.0]).tach_0, 0.0);
    }
}

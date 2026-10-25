//! Sun of the sky config `Sky/<track>.bin` (`docs/re/lighting.md`).
//!
//! The file is loaded raw as the sky config object (`SkyConfig_Load` 0x459660);
//! version 9 files need no upgrade (`SkyConfig_Upgrade` 0x459700 fills defaults
//! for older versions). `Sky_SunPosition` 0x461090 reads six fields.

use crate::{f32le, u32le, Result};

/// Size of a version-9 sky config.
pub const SKY_CONFIG_SIZE: usize = 0x71dc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkySun {
    /// `+0x3c`, turns.
    pub rotation: f32,
    /// `+0x44`, curvature of the sun height.
    pub curvature: f32,
    /// `+0x48`.
    pub height: f32,
    /// `+0xcdc`, `+0xce0`.
    pub offset: [f32; 2],
    /// `+0xce4`.
    pub scale: f32,
}

/// Read the sun fields. Versions below 5 have no offset (`+0xcdc/+0xce0 = 0`)
/// and versions below 6 use scale 0.1, as the upgrade does.
pub fn parse_sky_sun(data: &[u8]) -> Result<SkySun> {
    let version = u32le(data, 0)?;
    if version > 9 {
        return Err(format!("sky config version {version} is newer than 9"));
    }
    let (rotation, curvature, height) = if version < 1 {
        (0.0, 0.0004, 75.0)
    } else {
        (f32le(data, 0x3c)?, f32le(data, 0x44)?, f32le(data, 0x48)?)
    };
    let offset = if version < 5 {
        [0.0; 2]
    } else {
        [f32le(data, 0xcdc)?, f32le(data, 0xce0)?]
    };
    let scale = if version < 6 { 0.1 } else { f32le(data, 0xce4)? };
    Ok(SkySun {
        rotation,
        curvature,
        height,
        offset,
        scale,
    })
}

impl SkySun {
    /// `Sky_SunPosition` 0x461090 in original world coordinates:
    /// `((sin θ·b + cos θ·a)·s, (h − (a²+b²)·k)·s, (cos θ·b − sin θ·a)·s)`.
    pub fn position(&self) -> [f32; 3] {
        let turn = f64::from(self.rotation) * std::f64::consts::TAU;
        let (sin, cos) = (turn.sin(), turn.cos());
        let [a, b] = self.offset.map(f64::from);
        let s = f64::from(self.scale);
        [
            ((sin * b + cos * a) * s) as f32,
            ((f64::from(self.height) - (a * a + b * b) * f64::from(self.curvature)) * s) as f32,
            ((cos * b - sin * a) * s) as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(version: u32, fields: &[(usize, f32)]) -> Vec<u8> {
        let mut d = vec![0u8; SKY_CONFIG_SIZE];
        d[..4].copy_from_slice(&version.to_le_bytes());
        for &(o, v) in fields {
            d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        d
    }

    #[test]
    fn overhead_sun_without_offset() {
        let sun = parse_sky_sun(&config(9, &[(0x48, 125.0), (0x44, 0.0002), (0xce4, 0.1)])).unwrap();
        let p = sun.position();
        assert_eq!(p[0], 0.0);
        assert!((p[1] - 12.5).abs() < 1e-5);
        assert_eq!(p[2], 0.0);
    }

    #[test]
    fn rotation_turns_the_offset() {
        // θ = 0.25 turn: x = b·s, z = −a·s.
        let sun = parse_sky_sun(&config(
            9,
            &[(0x3c, 0.25), (0x48, 10.0), (0xcdc, 2.0), (0xce0, 3.0), (0xce4, 1.0)],
        ))
        .unwrap();
        let p = sun.position();
        assert!((p[0] - 3.0).abs() < 1e-5 && (p[2] + 2.0).abs() < 1e-5, "{p:?}");
        assert!((p[1] - 10.0).abs() < 1e-5);
    }

    #[test]
    fn old_versions_use_the_upgrade_defaults() {
        let sun = parse_sky_sun(&config(4, &[(0x48, 50.0), (0xcdc, 9.0), (0xce4, 7.0)])).unwrap();
        assert_eq!(sun.offset, [0.0; 2]);
        assert_eq!(sun.scale, 0.1);
        assert!(parse_sky_sun(&config(10, &[])).is_err());
    }

    #[test]
    fn local_alps_sun_is_about_44_degrees_high() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track/Sky/alps.bin");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let p = parse_sky_sun(&data).unwrap().position();
        let elevation = p[1].atan2(p[0].hypot(p[2])).to_degrees();
        assert!((elevation - 44.0).abs() < 1.0, "elevation {elevation}");
    }
}

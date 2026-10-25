//! NFS5 vehicle physics (.sim) and AI profile (.ais) parsers.
use crate::{bytes, f32le, Result};

pub const SIM_FILE_SIZE: usize = 328;
pub const AIS_FILE_SIZE: usize = 304;

/// Vehicle simulation parameters parsed from a 328-byte `.sim` file.
///
/// Field meanings follow the original consumers (`docs/re/vehicle.md`
/// section 5 in the workspace root; `Sim_Load` `0x49c6c0`,
/// `Sim_DeriveFields` `0x49cb50`). Several public field names predate that
/// mapping and are kept so callers don't break; their doc comments give the
/// original meaning and mark the name as misleading ("Naming note"). The
/// original-model port reads the raw bytes instead
/// (`nfs_assets::physics::original_vehicle::sim`).
#[derive(Debug, Clone, PartialEq)]
pub struct SimCar {
    /// Car display name (ASCII string up to 64 bytes).
    pub name: String,
    /// Vehicle curb mass in kilograms.
    pub mass_kg: f32,
    /// Wheelbase in meters (`0x44`, stored in mm, scaled by 0.001 at load).
    pub wheelbase_m: f32,
    /// Number of forward gears (`0x50`; the original adds 2 for R and N at load).
    pub gear_count: i32,
    /// Shift time in 32 Hz steps (`0x54`; 8 / 6, GT1 4) [VA 0x49dd20, 0x49e440].
    ///
    /// Naming note: not drivetrain flags. The front torque share is the f32
    /// at `0x5c`, the engine type (0 NA, 1 turbo, 2 supercharged) `0x4c`.
    pub drive_flags: i32,
    /// Reverse gear ratio.
    pub reverse_gear: f32,
    /// Forward gear ratios (1st, 2nd, ...).
    pub forward_gears: Vec<f32>,
    /// Final drive differential ratio.
    pub final_drive: f32,
    /// Engine redline RPM (`0x94`).
    pub redline_rpm: f32,
    /// Engine-braking fraction in percent (`0x98`, x0.01 at load) [VA 0x49e440].
    ///
    /// Naming note: not an idle rpm or curve step; the torque curve is
    /// sampled every 500 rpm.
    pub idle_or_step_rpm: f32,
    /// Torque curve sample points (21 entries).
    pub torque_curve: [f32; 21],
    /// Front weight share in percent (`0xf0`, x0.01 at load) [VA 0x4a1900].
    ///
    /// Naming note: not the brake bias; the front brake share is `0x108`
    /// (field `suspension_stiffness`).
    pub brake_bias: f32,
    /// Tyre friction coefficient mu (`0xf4`; 1.15..2.75) [VA 0x4a1900, 0x4a20d0].
    ///
    /// Naming note: not a drag coefficient; the original derives its cubic
    /// drag from the top speed `0x90` and the torque curve.
    pub drag_coeff: f32,
    /// Centre-of-gravity height for load transfer (`0xf8`, x0.01 at load) [VA 0x4a1900].
    ///
    /// Naming note: not a swaybar stiffness.
    pub swaybar_stiffness: f32,
    /// Front brake share in percent (`0x108`, x0.01 at load) [VA 0x49f160].
    ///
    /// Naming note: not the suspension stiffness, which is `0x11c` (field
    /// `tire_grip`).
    pub suspension_stiffness: f32,
    /// Front downforce per m/s (`0x10c`, x0.001 at load, times the fe.txt
    /// `frontdownforce` factor): each front wheel's grip gains
    /// `0x10c * speed` above 5 m/s [VA 0x4a1be7].
    ///
    /// Naming note: not the front track width, despite the name and the
    /// 0.001 scale.
    pub front_track_m: f32,
    /// Rear downforce per m/s (`0x110`, x0.001 at load, times the fe.txt
    /// `reardownforce` factor) [VA 0x4a1c0d].
    ///
    /// Naming note: not the rear track width, despite the name and the
    /// 0.001 scale.
    pub rear_track_m: f32,
    /// Pitch rate retention per step (`0x114`, x0.01 at load) [VA 0x499a70 tail].
    ///
    /// Naming note: not a compression damping coefficient.
    pub damping_compression: f32,
    /// Roll rate retention per step (`0x118`, x0.01 at load) [VA 0x499a70 tail].
    ///
    /// Naming note: not a rebound damping coefficient.
    pub damping_rebound: f32,
    /// Suspension stiffness (`0x11c`; divides the load transfer) [VA 0x4a1900].
    ///
    /// Naming note: not the tyre grip, which is mu at `0xf4` (field
    /// `drag_coeff`).
    pub tire_grip: f32,
    /// `0x120`, `0x124`, `0x128`, each x0.001 at load: ride height (mm; the
    /// original replaces it with the fe.txt `rideheight`), suspension travel
    /// (mm) and pitch gain from acceleration [VA 0x499a70, 0x49cb50].
    ///
    /// Naming note: not a centre-of-gravity offset.
    pub cg_offset_m: [f32; 3],
    /// Raw unscaled 328-byte data.
    pub raw: [u8; SIM_FILE_SIZE],
}

/// Parse a 328-byte `.sim` file into structured vehicle simulation parameters.
pub fn parse_sim(data: &[u8]) -> Result<SimCar> {
    if data.len() != SIM_FILE_SIZE {
        return Err(format!(
            "invalid .sim file size: expected {SIM_FILE_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let name_bytes = bytes(data, 0, 64)?;
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(64);
    let name = String::from_utf8_lossy(&name_bytes[..name_end])
        .trim()
        .to_string();

    let mass_kg = f32le(data, 0x40)?;
    let wheelbase_m = f32le(data, 0x44)? * 0.001;

    let gear_count = i32::from_le_bytes(bytes(data, 0x50, 4)?.try_into().unwrap());
    let drive_flags = i32::from_le_bytes(bytes(data, 0x54, 4)?.try_into().unwrap());

    let reverse_gear = f32le(data, 0x60)?;

    // Forward gear ratios at 0x68, 0x6c, 0x70, 0x74, 0x78, 0x7c
    let num_gears = (gear_count.clamp(1, 6)) as usize;
    let mut forward_gears = Vec::with_capacity(num_gears);
    for i in 0..num_gears {
        forward_gears.push(f32le(data, 0x68 + i * 4)?);
    }

    let final_drive = f32le(data, 0x80)?;
    let redline_rpm = f32le(data, 0x94)?;
    let idle_or_step_rpm = f32le(data, 0x98)?;

    let mut torque_curve = [0.0f32; 21];
    for (i, item) in torque_curve.iter_mut().enumerate() {
        *item = f32le(data, 0x9c + i * 4)?;
    }

    let brake_bias = f32le(data, 0xf0)?;
    let drag_coeff = f32le(data, 0xf4)?;
    let swaybar_stiffness = f32le(data, 0xf8)?;
    let suspension_stiffness = f32le(data, 0x108)?;

    // 0x10c/0x110: front/rear downforce per m/s (VA 0x4a1be7, 0x4a1c0d), not track width.
    let front_track_m = f32le(data, 0x10c)? * 0.001;
    let rear_track_m = f32le(data, 0x110)? * 0.001;

    let damping_compression = f32le(data, 0x114)?;
    let damping_rebound = f32le(data, 0x118)?;
    let tire_grip = f32le(data, 0x11c)?;

    let cg_offset_m = [
        f32le(data, 0x120)? * 0.001,
        f32le(data, 0x124)? * 0.001,
        f32le(data, 0x128)? * 0.001,
    ];

    let mut raw = [0u8; SIM_FILE_SIZE];
    raw.copy_from_slice(data);

    Ok(SimCar {
        name,
        mass_kg,
        wheelbase_m,
        gear_count,
        drive_flags,
        reverse_gear,
        forward_gears,
        final_drive,
        redline_rpm,
        idle_or_step_rpm,
        torque_curve,
        brake_bias,
        drag_coeff,
        swaybar_stiffness,
        suspension_stiffness,
        front_track_m,
        rear_track_m,
        damping_compression,
        damping_rebound,
        tire_grip,
        cg_offset_m,
        raw,
    })
}

/// AI profile parsed from a 304-byte `.ais` file.
#[derive(Debug, Clone, PartialEq)]
pub struct AisCar {
    /// Family / profile name (ASCII string up to 32 bytes).
    pub name: String,
    /// Acceleration & velocity profile (24 points).
    pub accel_profile: [f32; 24],
    /// Cornering & braking speed profile (40 points).
    pub cornering_profile: [f32; 40],
    /// Limit attributes (4 points).
    pub limits: [f32; 4],
    /// Raw unscaled 304-byte data.
    pub raw: [u8; AIS_FILE_SIZE],
}

/// Parse a 304-byte `.ais` file into structured AI profile parameters.
pub fn parse_ais(data: &[u8]) -> Result<AisCar> {
    if data.len() != AIS_FILE_SIZE {
        return Err(format!(
            "invalid .ais file size: expected {AIS_FILE_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let name_bytes = bytes(data, 0, 32)?;
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
    let name = String::from_utf8_lossy(&name_bytes[..name_end])
        .trim()
        .to_string();

    let mut accel_profile = [0.0f32; 24];
    for (i, item) in accel_profile.iter_mut().enumerate() {
        *item = f32le(data, 0x20 + i * 4)?;
    }

    let mut cornering_profile = [0.0f32; 40];
    for (i, item) in cornering_profile.iter_mut().enumerate() {
        *item = f32le(data, 0x80 + i * 4)?;
    }

    let mut limits = [0.0f32; 4];
    for (i, item) in limits.iter_mut().enumerate() {
        *item = f32le(data, 0x120 + i * 4)?;
    }

    let mut raw = [0u8; AIS_FILE_SIZE];
    raw.copy_from_slice(data);

    Ok(AisCar {
        name,
        accel_profile,
        cornering_profile,
        limits,
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_sim_size_rejected() {
        assert!(parse_sim(&[0u8; 100]).is_err());
        assert!(parse_sim(&[0u8; 329]).is_err());
    }

    #[test]
    fn invalid_ais_size_rejected() {
        assert!(parse_ais(&[0u8; 100]).is_err());
        assert!(parse_ais(&[0u8; 305]).is_err());
    }

    #[test]
    fn test_all_local_sim_files_parse() {
        let sim_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/Simulation/CarData");
        if !sim_dir.exists() {
            return;
        }

        let mut count = 0;
        for entry in std::fs::read_dir(&sim_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("sim") {
                let data = std::fs::read(&path).unwrap();
                let car = parse_sim(&data)
                    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
                assert!(!car.name.is_empty(), "car name empty: {}", path.display());
                assert!(
                    car.mass_kg > 400.0 && car.mass_kg < 3000.0,
                    "unreasonable mass: {}",
                    path.display()
                );
                assert!(
                    car.gear_count >= 4 && car.gear_count <= 6,
                    "unreasonable gears: {}",
                    path.display()
                );
                assert!(
                    car.redline_rpm >= 4000.0 && car.redline_rpm <= 10000.0,
                    "unreasonable RPM: {}",
                    path.display()
                );
                count += 1;
            }
        }
        assert!(
            count >= 80,
            "expected at least 80 .sim files, found {count}"
        );
    }

    #[test]
    fn test_all_local_ais_files_parse() {
        let ais_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/Simulation/AICarData");
        if !ais_dir.exists() {
            return;
        }

        let mut count = 0;
        for entry in std::fs::read_dir(&ais_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("ais") {
                let data = std::fs::read(&path).unwrap();
                let ais = parse_ais(&data)
                    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
                assert!(!ais.name.is_empty(), "ais name empty: {}", path.display());
                count += 1;
            }
        }
        assert!(
            count >= 20,
            "expected at least 20 .ais files, found {count}"
        );
    }
}

//! Weather keys of `Sky/<track>.bin` (`docs/re/weather.md`): fog, rain, snow and the
//! sky-map blend per track node, read the way `SkyManager_Draw` 0x45d4f0 and the key
//! readers 0x45a350/0x45a560.. do. Needs `local/game` (skipped otherwise).
//!
//! Spec: weather "Weather follows the camera's track node".

use std::path::{Path, PathBuf};

fn sky(name: &str) -> Option<Vec<u8>> {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/Track/Sky")
        .join(name);
    std::fs::read(p).ok()
}

#[test]
fn alps_snows_in_the_mountains_and_is_foggy() {
    let Some(data) = sky("alps.bin") else {
        eprintln!("skipped: no local/game");
        return;
    };
    let w = nfs_formats::parse_sky_weather(&data).unwrap();
    // alps: 2105 nodes (ai.md).
    let start = w.at(0, 2105);
    assert_eq!(start.snow, 0.0);
    assert_eq!(start.rain, 0.0);
    // Sky and world fog from node 0: 0.1467³, 0.1003³.
    assert!((start.sky_fog_density - 0.1467f32.powi(3)).abs() < 2e-5);
    assert!((start.world_fog_density - 0.1003f32.powi(3)).abs() < 2e-5);
    assert_eq!(start.world_fog_color, 0xff8b_6757);
    // Snow from node 648 to 1120.
    let pass = w.at(800, 2105);
    assert_eq!(pass.snow, 1.0);
    assert!(pass.world_fog_density > 0.01);
    // Sky maps blend to region B (st1b, sc1b) from node 563 on.
    assert_eq!(start.region_b, 0.0);
    assert!((w.at(604, 2105).region_b - 1.0).abs() < 1e-6);
}

#[test]
fn autobahn_rains_from_the_start() {
    let Some(data) = sky("Autobahn.bin") else {
        eprintln!("skipped: no local/game");
        return;
    };
    let w = nfs_formats::parse_sky_weather(&data).unwrap();
    let n = 3165;
    let start = w.at(100, n);
    assert_eq!(start.rain, 1.0);
    assert!(start.raining());
    assert_eq!(start.snow, 0.0);
    // Dry stretch between keys 172 and 179.
    assert!(!w.at(175, n).raining());
    // Lightning only near the start (0.135 at node 0, 0 at node 171).
    assert!(w.at(0, n).lightning > 0.1);
    assert_eq!(w.at(600, n).lightning, 0.0);
    // Clear colour: the sky fog colour (record flag +4).
    assert!(w.clear_record(3).use_fog_color);
}

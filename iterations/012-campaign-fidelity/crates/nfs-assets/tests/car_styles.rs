//! TPG style selection from the catalogue record (`FECarAccessor` slot 0x98 = record i16 `+0x1c0`
//! -> `R3DCar_ApplyStyle` 0x437b00 -> `[style{N+1}]`), checked on the installed game.
use nfs_assets::{load_car_style, AssetFiles};
use std::path::PathBuf;

fn car_files(car: &str) -> Option<AssetFiles> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/CarModel");
    let mut files = AssetFiles::new();
    for entry in std::fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name == format!("{car}.crp") || name == format!("{car}.tpg") || name.ends_with(".fsh") {
            files.insert(name, std::fs::read(path).ok()?);
        }
    }
    Some(files)
}

fn parts(files: &AssetFiles, car: &str, style: u32) -> (String, Vec<String>) {
    let scene = load_car_style(files, car, style).unwrap();
    let mut names: Vec<String> = scene.meshes.iter().map(|m| m.name.clone()).collect();
    names.sort();
    (scene.diagnostics[0].clone(), names)
}

#[test]
fn style_index_selects_the_tpg_section_and_body() {
    let Some(f993) = car_files("993") else {
        eprintln!("skip: game not installed");
        return;
    };
    // 993 Turbo (record 56, style 8) is [style9] "turbocoupe", not the Carrera coupe [style1].
    let (turbo_diag, turbo) = parts(&f993, "993", 8);
    let (coupe_diag, coupe) = parts(&f993, "993", 0);
    assert!(turbo_diag.contains("style9 (turbocoupe)"), "{turbo_diag}");
    assert!(coupe_diag.contains("style1 (Carreracoupe)"), "{coupe_diag}");
    assert_ne!(turbo, coupe, "the Turbo body differs from the Carrera coupe");
    // Carrera Targa (record 57, style 1) has a different roof from the coupe.
    let (targa_diag, targa) = parts(&f993, "993", 1);
    assert!(targa_diag.contains("style2 (Carreratarga)"), "{targa_diag}");
    assert_ne!(targa, coupe);
    // 964: Carrera 4 coupe (47, style 4) vs cabriolet (48, style 3).
    let f964 = car_files("964").unwrap();
    let (c4, c4_parts) = parts(&f964, "964", 4);
    let (cab, cab_parts) = parts(&f964, "964", 3);
    assert!(c4.contains("style5"), "{c4}");
    assert!(cab.contains("style4"), "{cab}");
    assert_ne!(c4_parts, cab_parts, "coupe and cabriolet differ");
    // A style beyond the TPG falls back to type 0 for every part.
    let (none, _) = parts(&f993, "993", 99);
    assert!(none.contains("implicit type 0"), "{none}");
}

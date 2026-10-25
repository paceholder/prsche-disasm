//! Barrier scenarios of the vehicle-collision spec on the original response
//! (0x494000 kernel and the 0x495020 edge pass on an installed track).

use std::path::Path;

use nfs_assets::physics::original_contact::{
    respond_prepared_contact, PreparedContact, PreparedContactState,
};
use nfs_assets::physics::original_pair::PairCar;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::race::original_collision::barrier_state;
use nfs_assets::AssetFiles;

fn state(velocity: [f32; 3]) -> PreparedContactState {
    PreparedContactState {
        position_330: [0.0, 0.6, 0.0],
        velocity_33c: velocity,
        speed_35c: 0.0,
        basis_364: [1.0, 0.0, 0.0],
        basis_370: [0.0, 1.0, 0.0],
        basis_37c: [0.0, 0.0, 1.0],
        field_38c: 0.0,
        field_434: 0.0,
        basis_490: [0.0, 0.0, -1.0],
        basis_4a8: [1.0, 0.0, 0.0],
        local_velocity_d58: velocity,
        wheel_points_7f8: [[0.0; 3]; 4],
        wheel_fields_870: [0.0; 4],
        field_db8: 0.0,
        field_dc0: 0.0,
    }
}

fn contact() -> PreparedContact {
    PreparedContact {
        normal: [1.0, 0.0, 0.0],
        correction: [0.1, 0.0, 0.0],
        alternate_52c_mask_04: false,
        angular_callback: true,
    }
}

#[test]
fn head_on_barrier_hit() {
    let mut s = state([-20.0, 0.0, 0.0]);
    respond_prepared_contact(&mut s, contact());
    // (-20 + 20 * 1.1) * 0.998
    assert!(
        (s.velocity_33c[0] - 1.996).abs() < 1e-4,
        "{:?}",
        s.velocity_33c
    );
}

#[test]
fn moving_away_from_a_barrier() {
    let mut s = state([5.0, 0.0, 3.0]);
    let before = s.clone();
    respond_prepared_contact(&mut s, contact());
    assert_eq!(s.velocity_33c, before.velocity_33c);
    assert_eq!(s.position_330, before.position_330);
}

#[test]
fn scraping_along_a_fence() {
    let mut s = state([-0.12, 0.0, 30.0]);
    respond_prepared_contact(&mut s, contact());
    assert!(
        (s.velocity_33c[2] - 30.0 * 0.998).abs() < 1e-4,
        "{:?}",
        s.velocity_33c
    );
}

fn skidpad() -> Option<AssetFiles> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Track");
    let mut files = AssetFiles::new();
    for name in [
        "skidpad.crp",
        "skidpad.fsh",
        "skidpad.edg",
        "skidpad.jnc",
        "animdefs.txt",
    ] {
        files.insert(name.into(), std::fs::read(dir.join(name)).ok()?);
    }
    Some(files)
}

#[test]
fn penetration_is_resolved_on_a_real_track() {
    let Some(files) = skidpad() else {
        eprintln!("skidpad not installed; skipped");
        return;
    };
    let scene = nfs_assets::track_loader::load(&files, "skidpad").unwrap();
    let surface = scene.road_surface.expect("skidpad support surface");
    let edges = surface.original_barrier_edges();
    assert!(!edges.is_empty());
    let mut responses = 0;
    for [a, b] in edges.iter().copied().take(60) {
        let d = [b[0] - a[0], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len < 6.0 {
            continue;
        }
        let along = [d[0] / len, 0.0, d[1] / len];
        let side = [-along[2], 0.0, along[0]];
        for sign in [1.0_f32, -1.0] {
            // Car parallel to the edge, centre 0.6 m off it, its far side across it,
            // driving into the edge at 10 m/s.
            let mid = [
                (a[0] + b[0]) * 0.5,
                (a[1] + b[1]) * 0.5 + 0.6,
                (a[2] + b[2]) * 0.5,
            ];
            let n = side.map(|v| v * sign);
            let car = PairCar {
                position_330: [mid[0] + n[0] * 0.6, mid[1], mid[2] + n[2] * 0.6],
                velocity_33c: n.map(|v| -v * 10.0),
                axis_r_364: side,
                axis_u_370: [0.0, 1.0, 0.0],
                axis_f_37c: along,
                extents_3a8: [0.9, 0.6, 2.0],
                ..PairCar::default()
            };
            let mut s = barrier_state(&car);
            let mut owner = OriginalSupportOwner::default();
            let Some(_) = surface
                .original_barrier_response(&mut s, car.extents_3a8, &mut owner)
                .unwrap()
            else {
                continue;
            };
            responses += 1;
            // The approach along the original normal is reversed (e > 0).
            let approach = |v: [f32; 3]| v[0] * n[0] + v[2] * n[2];
            assert!(approach(s.velocity_33c) > approach(car.velocity_33c));
            // The position correction moves the body out of the barrier.
            let again = surface
                .original_barrier_response(&mut s.clone(), car.extents_3a8, &mut owner)
                .unwrap();
            assert!(
                again.is_none(),
                "still overlapping after the response: {again:?}"
            );
        }
    }
    assert!(responses > 0, "no barrier response on any skidpad edge");
}

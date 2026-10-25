//! Drives a box car through every smackable record of a track from four directions and
//! reports, per article tag, how often the object is hit (`+0x7c` set).
//! Usage: smack_hit_census <track.crp> <animdefs.txt> [ride height above the pole base, m]
//! The car stands on the ground at the object's lowest point.
use nfs_assets::physics::original_pair::{PairCar, PairGlobals};
use nfs_assets::physics::original_smackable::{SmackBody, build_initial_smack_records};
use nfs_assets::race::original_collision::collide_pass_with;

fn car(pos: [f32; 3], dir: [f32; 3], speed: f32) -> PairCar {
    // Forward axis = dir (horizontal), right = dir x up.
    let f = dir;
    let r = [f[2], 0.0, -f[0]];
    let ext = [0.9, 0.65, 2.1];
    PairCar {
        position_330: pos,
        velocity_33c: [f[0] * speed, 0.0, f[2] * speed],
        mass_354: 1.25,
        inv_mass_358: 0.8,
        speed_35c: speed,
        axis_r_364: r,
        axis_u_370: [0.0, 1.0, 0.0],
        axis_f_37c: f,
        inertia_394: 7.5,
        inv_inertia_398: 1.0 / 7.5,
        extents_3a8: ext,
        radius_3b4: (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt(),
        active_7c: true,
        ..PairCar::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let ride: f32 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let crp = nfs_formats::parse_track_crp(&std::fs::read(&a[1])?)?;
    let defs = nfs_formats::animdefs::parse_animdefs(&std::fs::read_to_string(&a[2])?)?;
    let records = build_initial_smack_records(&crp, &defs)?;
    let mut stats: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    let mut missed = Vec::new();
    for (i, r) in records.iter().enumerate() {
        let name = String::from_utf8_lossy(&r.tag.to_le_bytes()).trim_end_matches('\0').to_string();
        for dir in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
            let mut body = SmackBody::from_record(i, r);
            body.asleep = false;
            // Start 12 m before the object centre, at the car's centre height.
            let start = [
                r.position[0] - dir[0] * 12.0,
                r.position[1] - body.car.pair.extents_3a8[1] + 0.65 + ride,
                r.position[2] - dir[2] * 12.0,
            ];
            let mut cars = vec![car(start, dir, 30.0), body.car.pair.clone()];
            let mut globals = PairGlobals::default();
            let mut hit = false;
            for _ in 0..60 {
                for c in cars.iter_mut().take(1) {
                    for k in 0..3 {
                        c.position_330[k] += c.velocity_33c[k] / 32.0;
                    }
                }
                collide_pass_with(&mut cars, &mut globals, |_, _, _| {});
                if cars[1].active_7c {
                    hit = true;
                    break;
                }
            }
            let e = stats.entry(name.clone()).or_default();
            e.0 += 1;
            if !hit {
                e.1 += 1;
                missed.push((i, name.clone(), dir, body.car.pair.extents_3a8));
            }
        }
    }
    for (name, (n, miss)) in &stats {
        println!("{name}: {n} runs, {miss} missed");
    }
    for m in missed.iter().take(12) {
        println!("missed art {} {} dir {:?} extents {:?}", records[m.0].article_index, m.1, m.2, m.3);
    }
    Ok(())
}

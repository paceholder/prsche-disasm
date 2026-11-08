//! The AI-field queue must hand `Car_TrackLocate` the support arena (the
//! viewer's `FieldQueue::support` override). With `support() == None` no car
//! gets a body support polygon, `0x499520` never refreshes the alternate
//! plane `+0x3e8/+0x400` (it stays at the grid placement) and
//! `body_surface_2c` stays at its initial value; the 2026-11-08 coastal
//! launch came from that. This queue mirrors the viewer's `Queue`
//! (`original_ai_view.rs`): road ground per car, support arena from the
//! road surface. Needs `local/game` (skipped otherwise).

use std::path::{Path, PathBuf};

use nfs_assets::physics::original_contact::PreparedContactState;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_pair::PairGlobals;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_vehicle::ground::GroundAdapter;
use nfs_assets::physics::original_vehicle::locate::SupportArena;
use nfs_assets::physics::original_vehicle::{OriginalCar, OriginalVehicle, RoadGround};
use nfs_assets::race::original_ai::field::{FieldQueue, OriginalAiField, OriginalAiTrack};
use nfs_assets::race::original_ai::track::TrackNodes;
use nfs_assets::{load_track, AssetFiles, RoadSurface};

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn scene(name: &str) -> Option<nfs_assets::Scene> {
    let dir = game().join("Track");
    let mut files = AssetFiles::new();
    for ext in ["crp", "fsh", "edg", "jnc", "map"] {
        let p = dir.join(format!("{name}.{ext}"));
        if let Ok(d) = std::fs::read(&p) {
            files.insert(format!("{name}.{ext}"), d);
        }
    }
    files.insert(
        "animdefs.txt".into(),
        std::fs::read(dir.join("animdefs.txt")).ok()?,
    );
    for lsp in [format!("racer/work/{name}0.lsp"), format!("{name}0.lsp")] {
        if let Ok(d) = std::fs::read(dir.join(&lsp)) {
            files.insert(format!("{name}0.lsp"), d);
            break;
        }
    }
    load_track(&files, name).ok()
}

struct Queue<'a> {
    surface: &'a RoadSurface,
    /// The viewer passes the arena; the test can switch it off to show the
    /// failure the override fixes.
    with_support: bool,
}

impl FieldQueue for Queue<'_> {
    fn ground(
        &mut self,
        _index: usize,
        car: &mut OriginalCar,
        adapter: &mut GroundAdapter,
        owners: &mut [OriginalSupportOwner; 4],
        lcg: &mut OriginalLcg,
    ) {
        adapter.ground_step(car, &mut RoadGround { surface: self.surface, owners }, lcg);
    }
    fn support(&self) -> Option<SupportArena<'_>> {
        self.with_support.then(|| self.surface.original_arena()).flatten()
    }
    fn barrier(
        &mut self,
        _index: usize,
        _state: &mut PreparedContactState,
        _half: [f32; 3],
        _owner: Option<&mut OriginalSupportOwner>,
    ) -> Option<nfs_assets::physics::original_vehicle::driver::BarrierHit> {
        None
    }
}

fn axes_at(t: &TrackNodes, node: i32) -> [[f32; 3]; 3] {
    let f = t.nodes[node as usize].forward_1c;
    let l = (f[0] * f[0] + f[2] * f[2]).sqrt();
    let f = [f[0] / l, 0.0, f[2] / l];
    [[f[2], 0.0, -f[0]], [0.0, 1.0, 0.0], f]
}

/// Runs the field for `steps` and returns (player owner polygon, AI owner
/// polygons, AI (`body_surface_2c`, owner flags), player alternate plane y, player y).
fn run(name: &str, with_support: bool, steps: usize) -> Option<(bool, Vec<bool>, Vec<(u16, u16)>, f32, f32)> {
    let g = game();
    let sim = std::fs::read(g.join("Simulation/CarData/356Acoupe16.sim")).ok()?;
    let ais = std::fs::read(g.join("Simulation/AICarData/356A.ais")).ok()?;
    let scene = scene(name)?;
    let surface = scene.road_surface.as_ref()?;
    let crp = std::fs::read(g.join(format!("Track/{name}.crp"))).ok()?;
    let jnc = std::fs::read(g.join(format!("Track/{name}.jnc"))).ok();
    let lsp = std::fs::read(g.join(format!("Track/racer/work/{name}0.lsp"))).ok()?;
    let t = OriginalAiTrack::from_files(name, false, &crp, jnc.as_deref(), &lsp)?;
    let half = [0.84, 0.6, 1.88];
    let wheels = [[-0.7, -0.6, 1.1], [0.7, -0.6, 1.1], [-0.7, -0.6, -1.0], [0.7, -0.6, -1.0]];
    let mut player = OriginalVehicle::from_sim(&sim, "356A", half, wheels).ok()?;
    let mut s140 = player.car.specs.field_140;
    let mut field = OriginalAiField::new(t, &ais, "356A", &mut s140);
    player.car.specs.field_140 = s140;
    let nodes = field.track.nodes.clone();
    let on_road = |node: i32, lat: f32| {
        let n = &nodes.nodes[node as usize];
        let p = [0, 1, 2].map(|k| n.pos_04[k] + n.lateral_28[k] * lat);
        // Scene z is the negated original z.
        let h = surface.query(p[0], -p[2], p[1], 50.0, 100.0).expect("road under the node").height;
        [p[0], h + 0.7, p[2]]
    };
    let mut owners = Default::default();
    let mut ground = RoadGround { surface, owners: &mut owners };
    player.set_pose(on_road(40, 0.0), axes_at(&nodes, 40), [0.0; 3], [0.0; 3], &mut ground);
    for (i, node) in [12, 18, 24].into_iter().enumerate() {
        let mut owners = Default::default();
        let mut ground = RoadGround { surface, owners: &mut owners };
        let lat = (i as f32 - 1.0) * 2.5;
        field.spawn(player.clone(), on_road(node, lat), axes_at(&nodes, node), &mut ground);
    }
    field.running = true;
    let mut globals = PairGlobals::default();
    let mut queue = Queue { surface, with_support };
    for _ in 0..steps {
        field.step(&mut player, 0, &mut globals, &mut queue);
    }
    Some((
        player.locator.owner.polygon.is_some(),
        field.cars.iter().map(|c| c.vehicle.locator.owner.polygon.is_some()).collect(),
        field.cars.iter().map(|c| (c.vehicle.car.body_surface_2c, c.vehicle.locator.owner.flags_24)).collect(),
        player.car.alt_base_400[1],
        player.car.position_330[1],
    ))
}

#[test]
fn field_hands_track_locate_the_support_arena() {
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    for name in ["coastal", "alps"] {
        let Some((player_poly, ai_poly, ai_2c, alt_y, y)) = run(name, true, 64 * 3) else {
            eprintln!("skipped {name}");
            continue;
        };
        assert!(player_poly, "{name}: player has no body support polygon beside the AI");
        assert!(ai_poly.iter().all(|&p| p), "{name}: AI polygons {ai_poly:?}");
        // body_surface_2c is the polygon's flags word (set by Car_TrackLocate).
        assert!(ai_2c.iter().all(|&(c, f)| c == f && f != 0), "{name}: {ai_2c:?}");
        // The alternate plane follows the car (road height, not the placement).
        assert!((alt_y - y).abs() < 3.0, "{name}: alternate plane y {alt_y} vs car y {y}");
        // Without the arena (the old default) no car gets a polygon.
        let (p0, a0, _, _, _) = run(name, false, 64 * 3).unwrap();
        assert!(!p0 && a0.iter().all(|&p| !p), "{name}: legacy path has polygons");
    }
}

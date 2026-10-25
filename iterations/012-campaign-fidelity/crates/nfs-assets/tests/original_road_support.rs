//! The original road support on real tracks, recorded by executing nfs5.exe
//! in Unicorn (`scripts/research/replay-road-support.py`; workspace notes
//! `docs/re/support.md`).
//!
//! The recorder builds the track's type-1 support polygons with the
//! original constructors (from this crate's assembly, `examples/support_dump.rs`),
//! inserts them with the original tree code and builds the node graph with
//! `TrackNodes_Build`. Then:
//! - owner streams: `0x474060` / `0x474420` / `0x474430` / `0x4745f0` at
//!   random interior points, exact vertices, edge midpoints, points in the
//!   track box and walks along the centre line (reuse on and off);
//! - closed loops (993 C4, V6 car geometry): on even steps
//!   `Car_TrackLocate` 0x49af70, `Car_ReadInputs`, `Car_DynamicsTick`,
//!   `Car_GroundContactTick` with the real wheel owners; the 64 Hz
//!   integration on every step. The Autobahn run goes down the ramp the
//!   user reported.
//!
//! The port builds the same polygons (polygon-only tree, as the recorder)
//! and must match every value bit for bit. Needs `local/game` and the local fixtures (skipped
//! otherwise).
//!
//! Spec: track-support "The support owner picks the polygon as the
//! original", "Wheels stand on their own support owners", "Track locate runs
//! first in the 32 Hz queue", "The Autobahn ramp bumps as in the original",
//! "Reset on the node graph".

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[path = "support/vehicle_step.rs"]
mod fixture;
#[path = "support/vehicle_ground.rs"]
mod ground_fixture;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use local_fixtures::Fixture;

use fixture::{body_values, cars, dyn_values, rows, same, Row, V};
use ground_fixture::*;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_support::SupportPolygon;
use nfs_assets::physics::original_support_loader::assemble_initial_track_support;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_support_tree::SupportTree;
use nfs_assets::physics::original_vehicle::{
    ground_contact_tick, scheduler_step, track_locate, BodyLocator, OriginalCar, RoadGround,
    StepQueue,
};
use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes};
use nfs_assets::RoadSurface;

static QUERY: Fixture = Fixture::new("support-query.tsv");
static LOOP_RUNS: Fixture = Fixture::new("support-loop-runs.tsv");
static LOOP_BODY: Fixture = Fixture::new("support-loop-body.tsv");
static LOOP_DYN: Fixture = Fixture::new("support-loop-dyn.tsv");

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Track")
}

struct Track {
    surface: RoadSurface,
    nodes: TrackNodes,
}

impl Track {
    fn load(name: &str) -> Option<Self> {
        let dir = game();
        let raw = std::fs::read(dir.join(format!("{name}.crp"))).ok()?;
        let crp = nfs_formats::track::parse_track_crp(&raw).ok()?;
        let anim = nfs_formats::animdefs::parse_animdefs(
            &std::fs::read_to_string(dir.join("animdefs.txt")).ok()?,
        )
        .ok()?;
        let load = assemble_initial_track_support(&crp, &anim).ok()?;
        let names = vec![String::new(); crp.articles.len()];
        let surface = RoadSurface::from_original_support(load, &names).ok()?;
        let unpacked = nfs_formats::decompress(&raw).ok()?;
        let (nodes, sections) = sim_chunks(&unpacked)?;
        Some(Self {
            surface,
            nodes: TrackNodes::build(nodes, &sections, None),
        })
    }

    fn arena(&self) -> (&SupportTree, &[SupportPolygon]) {
        self.surface.original_arena().unwrap()
    }
}

fn tracks(names: impl IntoIterator<Item = String>) -> Option<HashMap<String, Track>> {
    let mut out = HashMap::new();
    for n in names {
        if !out.contains_key(&n) {
            out.insert(n.clone(), Track::load(&n)?);
        }
    }
    Some(out)
}

fn f(v: f32) -> V {
    V::F(v.to_bits())
}

fn i(v: i64) -> V {
    V::I(v)
}

fn poly(p: Option<usize>) -> V {
    i(p.map_or(-1, |k| k as i64))
}

fn node(tree: &SupportTree, n: Option<usize>) -> V {
    i(n.map_or(-1, |k| tree.nodes[k].cell.packed as i64))
}

fn vec3(out: &mut Vec<(String, V)>, name: &str, v: [f32; 3]) {
    for (k, c) in v.iter().enumerate() {
        out.push((format!("{name}{k}"), f(*c)));
    }
}

fn owner_values(tree: &SupportTree, o: &OriginalSupportOwner, prefix: &str) -> Vec<(String, V)> {
    let mut out = vec![
        (format!("{prefix}poly"), poly(o.polygon)),
        (format!("{prefix}node"), node(tree, o.node)),
        (format!("{prefix}b02"), i(o.normal_valid_02 as i64)),
        (format!("{prefix}b09"), i(o.changed_09 as i64)),
        (format!("{prefix}b0a"), i(o.missing_0a as i64)),
        (format!("{prefix}flags"), i(o.flags_24 as i64)),
    ];
    vec3(&mut out, &format!("{prefix}n"), o.normal_0c);
    vec3(&mut out, &format!("{prefix}t"), o.tangent_18);
    out
}

fn check(what: &str, got: Vec<(String, V)>, r: &Row) {
    for (k, v) in got {
        assert!(same(&v, r, &k), "{what}: {k} = {v:?}, original {}", r.s(&k));
    }
}

#[test]
fn owner_queries_match_the_original_on_real_tracks() {
    skip_without_local_fixtures!(QUERY);
    let q = rows(&QUERY);
    let Some(tracks) = tracks(q.iter().map(|r| r.s("track").to_string())) else {
        eprintln!("skipped: no local/game");
        return;
    };
    let mut owner = OriginalSupportOwner::default();
    let mut stream = String::new();
    for r in &q {
        let t = &tracks[r.s("track")];
        let (tree, polygons) = t.arena();
        let key = format!("{}/{}", r.s("track"), r.s("stream"));
        if key != stream {
            owner = OriginalSupportOwner::default();
            stream = key.clone();
        }
        let what = format!("{key} #{}", r.s("seq"));
        let p = [r.f("px"), r.f("py"), r.f("pz")];
        owner.locate(tree, polygons, p, r.i("reuse") == 1).unwrap();
        check(&what, owner_values(tree, &owner, "sel_"), r);
        let mut out = Vec::new();
        vec3(&mut out, "normal", owner.normal(polygons));
        vec3(&mut out, "tangent", owner.tangent(polygons));
        for j in 0..4 {
            vec3(&mut out, &format!("v{j}_"), owner.vertex(polygons, j));
        }
        out.extend(owner_values(tree, &owner, "out_"));
        check(&what, out, r);
    }
    eprintln!("{} owner calls matched", q.len());
}

/// The locator columns of `SupportRecorder.locate_row`.
fn locate_values(
    car: &OriginalCar,
    loc: &BodyLocator,
    owners: &[OriginalSupportOwner; 4],
    tree: &SupportTree,
) -> Vec<(String, V)> {
    let o = &loc.owner;
    let t = &car.track;
    let mut out = vec![
        ("loc_node_08".into(), i(loc.track.node_00 as i64)),
        ("loc_section_0c".into(), i(loc.track.section_04 as i64)),
        ("loc_b10".into(), i(loc.track.changed_08 as i64)),
        ("loc_b11".into(), i(loc.track.changed_09 as i64)),
        ("loc_b12".into(), i(o.missing_0a as i64)),
        ("loc_b0a".into(), i(o.normal_valid_02 as i64)),
        ("body_flags_2c".into(), i(car.body_surface_2c as i64)),
        ("dist_78".into(), f(t.dist_78)),
        ("b7e".into(), i(t.flag_7e as i64)),
        ("heading_414".into(), f(t.heading_414)),
        ("surface_460".into(), i(t.surface_460 as i64)),
        ("surface_464".into(), i(car.surface_464 as i64)),
        ("loc_poly".into(), poly(o.polygon)),
        ("loc_tree".into(), node(tree, o.node)),
    ];
    for (name, v) in [
        ("loc_n", o.normal_0c),
        ("loc_t", o.tangent_18),
        ("node_lat_3b8", t.node_lat_3b8),
        ("node_up_3c4", t.node_up_3c4),
        ("node_fwd_3d0", t.node_fwd_3d0),
        ("alt_side_3dc", t.alt_side_3dc),
        ("alt_n_3e8", car.alt_normal_3e8),
        ("alt_t_3f4", t.alt_tangent_3f4),
        ("alt_b_400", car.alt_base_400),
    ] {
        vec3(&mut out, &format!("{name}_"), v);
    }
    for (k, w) in owners.iter().enumerate() {
        out.push((format!("w{k}_poly"), poly(w.polygon)));
        out.push((format!("w{k}_tree"), node(tree, w.node)));
        out.push((format!("w{k}_b09"), i(w.changed_09 as i64)));
        out.push((format!("w{k}_b0a"), i(w.missing_0a as i64)));
        out.push((format!("w{k}_flags"), i(w.flags_24 as i64)));
    }
    out
}

struct Queue<'a> {
    track: &'a Track,
    locator: BodyLocator,
    owners: [OriginalSupportOwner; 4],
}

impl StepQueue for Queue<'_> {
    fn track_locate(&mut self, car: &mut OriginalCar) {
        let me = car.position_330;
        track_locate(
            car,
            &mut self.locator,
            &self.track.nodes,
            self.track.surface.original_arena(),
            &[me],
        );
    }
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        ground_contact_tick(
            car,
            &mut RoadGround {
                surface: &self.track.surface,
                owners: &mut self.owners,
            },
            lcg,
        );
    }
}

/// The recorder's initial state (`SupportRecorder.setup_drive`).
fn initial(run: &Row) -> (OriginalCar, OriginalLcg, BodyLocator) {
    let specs = &cars()[run.s("car")];
    let mut car = OriginalCar::new(specs.clone());
    car.res_14 = 1;
    car.body_points_8c = BODY_POINTS.to_vec();
    set_ground(&mut car, run, "init_");
    set_inputs(&mut car, run, "init_");
    let gear = run.i("gear0") as i8;
    car.gear_d82 = gear;
    car.gear_req_d7e = gear;
    car.prev_gear_d81 = gear;
    car.rpm_db4 = run.f("rpm0");
    car.globals.sched_step_606ac4 = run.i("sched0") as i32;
    car.globals.auto_box_658098 = 1;
    let lcg = OriginalLcg {
        state_5d1028: run.i("rand_state0") as u32,
        multiplier_5d102c: 1,
        product_655a0c: 0,
    };
    (car, lcg, BodyLocator::at_node(run.i("node0") as i32))
}

#[test]
fn closed_loops_on_real_tracks_match_the_original() {
    skip_without_local_fixtures!(LOOP_RUNS, LOOP_BODY, LOOP_DYN, fixture::CARS);
    let runs = rows(&LOOP_RUNS);
    let body = rows(&LOOP_BODY);
    let dyn_rows = rows(&LOOP_DYN);
    let Some(tracks) = tracks(runs.iter().map(|r| r.s("track").to_string())) else {
        eprintln!("skipped: no local/game");
        return;
    };
    for run in &runs {
        let name = run.s("run");
        let track = &tracks[run.s("track")];
        let (tree, _) = track.arena();
        let b: Vec<&Row> = body.iter().filter(|r| r.s("run") == name).collect();
        let d: Vec<&Row> = dyn_rows.iter().filter(|r| r.s("run") == name).collect();
        let (mut car, mut lcg, locator) = initial(run);
        let mut q = Queue {
            track,
            locator,
            owners: Default::default(),
        };
        let steps = run.i("steps") as usize;
        assert_eq!(b.len(), steps, "{name}");
        for n in 0..steps {
            let even = car.globals.sched_step_606ac4 & 1 == 0;
            let word = if even {
                d[n / 2].i("input_word") as u32
            } else {
                0
            };
            if even {
                // The 32 Hz rows are taken right after the ground tick.
                struct Until<'a, 'b>(&'a mut Queue<'b>, Option<OriginalCar>);
                impl StepQueue for Until<'_, '_> {
                    fn track_locate(&mut self, car: &mut OriginalCar) {
                        self.0.track_locate(car);
                    }
                    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
                        self.0.ground(car, lcg);
                        self.1 = Some(car.clone());
                    }
                }
                let mut c = car.clone();
                let mut l = lcg;
                let saved = (q.locator.clone(), q.owners.clone());
                let mut u = Until(&mut q, None);
                scheduler_step(&mut c, word, &mut l, &mut u);
                let after = u.1.unwrap();
                let r = d[n / 2];
                let what = format!("{name} 32 Hz step {}", n / 2);
                check(&what, dyn_values(&after), r);
                check(&what, ground_values(&after, false), r);
                check(&what, locate_values(&after, &q.locator, &q.owners, tree), r);
                assert_eq!(l.state_5d1028 as i64, r.i("rand_state"), "{what}");
                (q.locator, q.owners) = saved;
            }
            scheduler_step(&mut car, word, &mut lcg, &mut q);
            for (k, v) in body_values(&car) {
                assert_eq!(
                    b[n].f(&k).to_bits(),
                    v.to_bits(),
                    "{name} step {n}: {k} = {v:?}, original {}",
                    b[n].s(&k)
                );
            }
        }
        eprintln!("{name}: {steps} steps matched");
    }
}

/// The ramp the user reported ("the car jumps on every triangle edge"),
/// driven down at 33–35 m/s by the original code on the real support. Per
/// 32 Hz step the log gives the height `+0x424`, the vertical velocity and
/// the wheels whose polygon changed (`NFS5_RAMP_LOG=1` prints it). The
/// original lifts the body off the faceted ramp after polygon changes:
/// these steps are original behaviour (the port reproduces them bit for bit
/// in `closed_loops_on_real_tracks_match_the_original`), not a query fault.
#[test]
fn autobahn_ramp_bumps_follow_the_support_polygons() {
    skip_without_local_fixtures!(LOOP_DYN);
    let rows_ = rows(&LOOP_DYN);
    let ramp: Vec<&Row> = rows_
        .iter()
        .filter(|r| r.s("run") == "autobahn/ramp_down")
        .collect();
    assert!(ramp.len() > 200);
    let log = std::env::var_os("NFS5_RAMP_LOG").is_some();
    let (mut lifts, mut steps_at_changes, mut big_steps) = (0, 0, 0);
    let mut max_h = 0.0f32;
    for w in ramp.windows(2) {
        let (a, b) = (w[0], w[1]);
        let h = b.f("height_424");
        let dvy = b.f("vel_33c_1") - a.f("vel_33c_1");
        let changed: Vec<usize> = (0..4)
            .filter(|k| a.i(&format!("w{k}_poly")) != b.i(&format!("w{k}_poly")))
            .collect();
        if log {
            println!(
                "{}\th424 {h:.4}\tvy {:.3}\tdvy {dvy:+.3}\tchanged {changed:?}",
                b.s("step"),
                b.f("vel_33c_1")
            );
        }
        max_h = max_h.max(h);
        lifts += (h > 0.005) as usize;
        if dvy.abs() > 0.5 {
            big_steps += 1;
        }
        // A wheel polygon change in this step.
        if dvy.abs() > 0.5 && !changed.is_empty() {
            steps_at_changes += 1;
        }
        assert_eq!(b.i("loc_b12"), 0, "the ramp has support under the body");
        assert_eq!(
            b.i("surface_464"),
            1,
            "asphalt (surface 1) on the whole ramp"
        );
    }
    eprintln!(
        "original on the ramp: max lift {max_h:.3} m, {lifts} steps above 5 mm, \
         {big_steps} vertical velocity steps > 0.5 m/s ({steps_at_changes} at polygon changes)"
    );
    assert!(
        max_h > 0.05 && big_steps > 10,
        "the original bumps on the facets"
    );
    assert!(max_h < 0.2, "but never leaves the ramp");
}

/// Reset on the node graph (`Car_ResetNodeFree` 0x40f480, `Car_PlaceOnNode`
/// 0x49b090, ported from pseudo-C): from a node, the car ends at a node
/// centre with support under it, surface 1 and the node's frame.
#[test]
fn reset_places_the_car_on_a_supported_node() {
    skip_without_local_fixtures!(fixture::CARS);
    use nfs_assets::physics::original_vehicle::locate::{place_on_node, reset_node};
    let Some(track) = Track::load("autobahn") else {
        eprintln!("skipped: no local/game");
        return;
    };
    let specs = cars()["993C4"].clone();
    for start in [40, 400, 900] {
        let mut car = OriginalCar::new(specs.clone());
        car.flags_52c = 1;
        let mut loc = BodyLocator::at_node(start);
        let support = track.surface.original_arena();
        let node = reset_node(&mut loc, &track.nodes, support, &[start as i16 + 1], false);
        place_on_node(&mut car, &mut loc, &track.nodes, support, node, false, &[]);
        let n = &track.nodes.nodes[node as usize];
        assert_eq!(car.position_330, n.pos_04, "start {start}");
        assert!(
            !loc.missing_12() && car.surface_464 == 1,
            "start {start}: on asphalt"
        );
        assert_eq!(car.axes_364[2], car.track.node_fwd_3d0);
        assert_eq!(
            car.track.node_up_3c4,
            track.nodes.nodes[loc.track.node_00 as usize].up_10
        );
        assert_ne!(
            car.alt_base_400, [0.0; 3],
            "alternate plane from the body polygon"
        );
    }
}

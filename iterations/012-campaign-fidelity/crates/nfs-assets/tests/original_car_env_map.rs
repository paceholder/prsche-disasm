//! The original car sky-map layer recorded in Unicorn (scripts/research/replay-car-env-map.py),
//! compared bit for bit with `nfs_assets::car_env`:
//! - `TrackLighting_Load` 0x4409d0 (real INI reader, CRT `atoi` / `atof`): the section records
//!   of synthetic `.map` texts (defaults, overlaps, gaps, duplicate keys, odd numbers; committed
//!   fixtures/car-env-map.tsv) and of every shipped track (local car-env-map-tracks.tsv) ==
//!   `load_sections`;
//! - for every node (and -2, -1, 0xffff, 0xffffffff in the synthetic cases)
//!   `TrackLighting_SelectSection` 0x441140, the slot lookups 0x441180 (covered 0 / 1) and
//!   `texturesize` / `aspect` 0x4411c0 / 0x4411e0 == `select_section`, `section_texture`,
//!   `section_size`, `section_aspect`;
//! - the sky-map part of `R3DCar_UpdateLighting` 0x43a560 (entry to 0x43a8a6) for the 993 C4
//!   placed on Forest, Farmland and Coastal nodes (tunnel 190..198, a node outside every
//!   section) with the real support tree, node graph and `Car_TrackLocate` == `update_light`,
//!   and the overhead query 0x473330 run unmodified on the locator copy ==
//!   `SupportTree::overhead_flags` (covered road `& 0xc000`);
//! - `R3DSkyMapTextureLayer` 0x445b10: every texture coordinate it writes (plain, skipped
//!   polygons, polygon size 1, the unclipped flag 0x100, the byte index channel with and
//!   without 0x100, UV set 0 / 1, other strides), the texture of the layer parameter (slot
//!   thresholds 0.05 / 0.95), the disable byte and an empty slot (the pass's draw layer
//!   skipped) == `layer_matrix` + `layer_uv` + `layer_draw`;
//! - the texture binding (0x4408c0 then 0x536240 -> THRASH -> dx7z.dll -> D3D7): every `.env`
//!   texture is sampled with ADDRESS = WRAP, MAG / MIN = LINEAR, MIP = NONE ==
//!   `car_env::ENV_SAMPLER`.
//!
//! Spec: original-lighting (car sky map).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use local_fixtures::Fixture;
use nfs_assets::car_env::{
    layer_draw, layer_matrix, layer_uv, load_sections, section_aspect, section_size, section_texture,
    select_section, update_light, EnvCar, EnvLight, EnvSection, LayerDraw, UvPath, ENV_SAMPLER,
};

const SYNTHETIC: &str = include_str!("fixtures/car-env-map.tsv");
static TRACKS: Fixture = Fixture::new("car-env-map-tracks.tsv");

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Track")
}

struct Row<'a> {
    kind: &'a str,
    case: &'a str,
    key: &'a str,
    out: &'a str,
}

fn rows(text: &str) -> Vec<Row<'_>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("kind\t"))
        .map(|l| {
            let c: Vec<&str> = l.splitn(4, '\t').collect();
            Row { kind: c[0], case: c[1], key: c[2], out: c[3] }
        })
        .collect()
}

fn fields(s: &str) -> HashMap<&str, &str> {
    s.split(';').filter_map(|p| p.split_once('=')).collect()
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}

fn hexes(s: &str) -> Vec<u32> {
    s.split(',').map(hex).collect()
}

fn f(s: &str) -> f32 {
    f32::from_bits(hex(s))
}

fn v3(s: &str) -> [f32; 3] {
    let v = hexes(s);
    [0, 1, 2].map(|i| f32::from_bits(v[i]))
}

fn name(t: Option<u32>) -> String {
    t.map_or("0".into(), |v| format!("{v:08x}"))
}

fn unescape(s: &str) -> Vec<u8> {
    s.replace("\\r", "\r").replace("\\n", "\n").into_bytes()
}

fn check_sections(case: &str, sections: &[EnvSection], rows: &[&Row]) {
    let recorded: Vec<&&Row> = rows.iter().filter(|r| r.kind == "section" && r.case == case).collect();
    assert_eq!(sections.len(), recorded.len(), "{case}: section count");
    for (k, r) in recorded.iter().enumerate() {
        let x = fields(r.out);
        let s = &sections[k];
        let open = hexes(x["open"]);
        let covered = hexes(x["covered"]);
        assert_eq!(
            (s.start, s.end, s.open.to_vec(), s.covered.to_vec(), s.size.to_bits(), s.aspect.to_bits()),
            (
                x["start"].parse::<u32>().unwrap(),
                x["end"].parse::<u32>().unwrap(),
                open,
                covered,
                hex(x["size"]),
                hex(x["aspect"])
            ),
            "{case} section {k}"
        );
    }
}

/// `select` rows: a node range (`a..b`) with one recorded result.
fn check_select(case: &str, sections: &[EnvSection], rows: &[&Row]) -> usize {
    let mut n = 0;
    for r in rows.iter().filter(|r| r.kind == "select" && r.case == case) {
        let (a, b) = r.key.split_once("..").unwrap();
        let (a, b): (i64, i64) = (a.parse().unwrap(), b.parse().unwrap());
        let x = fields(r.out);
        let want_sel: i64 = x["sel"].parse().unwrap();
        let want_tex: Vec<&str> = x["tex"].split(',').collect();
        let (want_size, want_aspect) = (hex(x["size"]), hex(x["aspect"]));
        // 0x4411c0 / 0x4411e0 run after 0x441180: the row's ST0 values are those of the
        // range's first node; a section change moves them, so only the first node of a run
        // carries a fresh value (the recorder splits runs on any change).
        for node in a..=b {
            let node32 = node as u32 as i32;
            let sel = select_section(sections, node32);
            assert_eq!(sel.map_or(-1, |s| s as i64), want_sel, "{case} node {node}");
            let tex: Vec<String> = [(false, 0), (false, 1), (false, 2), (true, 0), (true, 1), (true, 2)]
                .iter()
                .map(|&(c, k)| name(section_texture(sections, sel, c, k)))
                .collect();
            assert_eq!(tex, want_tex, "{case} node {node} textures");
            assert_eq!(section_size(sections, sel).to_bits(), want_size, "{case} node {node} size");
            assert_eq!(section_aspect(sections, sel).to_bits(), want_aspect, "{case} node {node} aspect");
            n += 1;
        }
    }
    n
}

fn light_from(x: &HashMap<&str, &str>) -> EnvLight {
    let slots: Vec<Option<u32>> = x["slots"].split(',').map(|s| (s != "0").then(|| hex(s))).collect();
    let frame = hexes(x["frame"]);
    let o = hexes(x["o"]);
    EnvLight {
        slots: [slots[0], slots[1], slots[2]],
        k: f(x["k"]),
        inv_size: f(x["inv_size"]),
        aspect: f(x["aspect"]),
        position: v3(x["pos"]),
        frame_t: std::array::from_fn(|i| f32::from_bits(frame[i])),
        offsets: [f32::from_bits(o[0]), f32::from_bits(o[1])],
    }
}

/// One recorded 0x445b10 call against the port.
fn check_draw(what: &str, light: &EnvLight, disabled: bool, out: &str) -> usize {
    let x = fields(out);
    let param = f(x["param"]);
    let layer_type = hex(x["type"]);
    let stride = hex(x["stride"]);
    let poly: usize = x["poly"].parse().unwrap();
    let flags = hex(x["flags"]);
    let skip: Vec<usize> = if x["skip"] == "-" { vec![] } else { x["skip"].split(',').map(|v| v.parse().unwrap()).collect() };
    let index: Option<Vec<usize>> =
        (x["index"] != "-").then(|| x["index"].split(',').map(|v| v.parse().unwrap()).collect());
    let model: [f32; 16] = {
        let m = hexes(x["model"]);
        std::array::from_fn(|i| f32::from_bits(m[i]))
    };
    let verts: Vec<([f32; 3], [f32; 3])> = x["verts"]
        .split('|')
        .map(|v| {
            let c = hexes(v);
            (
                [0, 1, 2].map(|i| f32::from_bits(c[i])),
                [3, 4, 5].map(|i| f32::from_bits(c[i])),
            )
        })
        .collect();
    let recorded: Vec<Option<[u32; 2]>> = x["uv"]
        .split('|')
        .map(|v| (v != "-").then(|| {
            let (a, b) = v.split_once(',').unwrap();
            [hex(a), hex(b)]
        }))
        .collect();
    let draw = layer_draw(light, param, disabled, layer_type, stride);
    let ret = x["ret"];
    let texture = x["texture"];
    match draw {
        LayerDraw::Draw(t) => {
            assert_eq!(ret, "next", "{what}");
            assert_eq!(texture, format!("{t:08x}"), "{what}: texture");
            let bit = if layer_type == 0xf { 0x80 } else { 0x40 };
            assert_eq!(hex(x["flags_out"]), flags | bit, "{what}: flags");
        }
        LayerDraw::SkipPass => {
            assert_eq!(ret, "after_draw", "{what}");
            assert_eq!(texture, "0", "{what}");
            assert!(recorded.iter().all(Option::is_none), "{what}: no coordinates");
            return 0;
        }
        LayerDraw::Next => {
            assert_eq!(ret, "next", "{what}");
            assert!(recorded.iter().all(Option::is_none), "{what}: no coordinates");
            return 0;
        }
    }
    let m = layer_matrix(light, &model);
    // 0x445d53: polygon size 1 or the unclipped flag 0x100 → every vertex; otherwise per
    // polygon of `poly` vertices with the skip words, and the swapped operands on the indexed
    // loop.
    let all = flags & 0x100 != 0 || poly == 1;
    let path = if index.is_some() && !all { UvPath::Swapped } else { UvPath::Normal };
    let order: Vec<usize> = index.clone().unwrap_or_else(|| (0..verts.len()).collect());
    let mut n = 0;
    for (slot, &vi) in order.iter().enumerate() {
        let skipped = !all && skip.contains(&(slot / poly));
        let got = (!skipped).then(|| {
            let uv = layer_uv(light, &m, verts[vi].0, verts[vi].1, path);
            [uv[0].to_bits(), uv[1].to_bits()]
        });
        assert_eq!(got, recorded[slot], "{what}: output {slot} (vertex {vi})");
        n += got.is_some() as usize;
    }
    n
}

#[test]
fn synthetic_maps_sections_and_selection_match_the_original() {
    let all = rows(SYNTHETIC);
    let refs: Vec<&Row> = all.iter().collect();
    let mut nodes = 0;
    for r in all.iter().filter(|r| r.kind == "map") {
        let sections = load_sections(&unescape(r.out));
        check_sections(r.case, &sections, &refs);
        nodes += check_select(r.case, &sections, &refs);
    }
    assert!(nodes > 200, "{nodes} nodes checked");
}

#[test]
fn synthetic_layer_draws_match_the_original() {
    let all = rows(SYNTHETIC);
    let mut light = None;
    let mut uvs = 0;
    let mut kinds = std::collections::BTreeSet::new();
    for r in &all {
        match r.kind {
            "light" => light = Some(light_from(&fields(r.out))),
            "draw" => {
                let l = light.as_ref().unwrap();
                let x = fields(r.out);
                let disabled = x["off"] == "1";
                uvs += check_draw(&format!("{} draw {}", r.case, r.key), l, disabled, r.out);
                kinds.insert(x["ret"].to_string());
            }
            _ => {}
        }
    }
    assert!(uvs > 500, "{uvs} coordinates");
    assert!(kinds.contains("after_draw"), "the skipped pass is covered");
}

#[test]
fn env_textures_bind_with_wrap_and_linear_filtering_like_the_original() {
    // `full` rows: every stage-0 state of the `.env` texture after 0x4408c0 sent to D3D7.
    let all = rows(SYNTHETIC);
    let full: Vec<&Row> = all.iter().filter(|r| r.kind == "bind" && r.case == "full").collect();
    assert_eq!(full.len(), 3);
    for r in full {
        let x = fields(r.out);
        let calls: Vec<(&str, Vec<u32>)> = x["d3d"]
            .split('|')
            .map(|c| {
                let (n, a) = c.split_once(':').unwrap();
                (n, a.split(',').map(|v| v.parse().unwrap()).collect())
            })
            .collect();
        let tss = |state: u32| {
            calls
                .iter()
                .filter(|(n, a)| *n == "SetTextureStageState" && a[0] == 0 && a[1] == state)
                .map(|(_, a)| a[2])
                .last()
        };
        // D3DTSS_ADDRESS 12, MAGFILTER 16, MINFILTER 17, MIPFILTER 18.
        assert_eq!(tss(12), Some(if ENV_SAMPLER.wrap { 1 } else { 3 }), "address: D3DTADDRESS_WRAP");
        assert_eq!(tss(16), Some(if ENV_SAMPLER.linear { 2 } else { 1 }), "mag filter");
        assert_eq!(tss(17), Some(if ENV_SAMPLER.linear { 2 } else { 1 }), "min filter");
        assert_eq!(tss(18), Some(if ENV_SAMPLER.mipmaps { 2 } else { 1 }), "mip filter: none");
    }
    // 0x4408c0 forces the address states (texture state block +0x38 words 5, 6) to 1.
    for r in all.iter().filter(|r| r.kind == "bind" && r.case == "tex") {
        let x = fields(r.out);
        assert_eq!(x["state_after"], "0,1,0,0,0,1,1", "slot {}", r.key);
    }
}

#[test]
fn shipped_maps_sections_and_selection_match_the_original() {
    skip_without_local_fixtures!(TRACKS);
    let all = rows(&TRACKS);
    let refs: Vec<&Row> = all.iter().collect();
    let mut tracks = 0;
    let mut nodes = 0;
    for r in all.iter().filter(|r| r.kind == "nodes") {
        let path = game().join(format!("{}.map", r.case));
        let Ok(text) = std::fs::read(&path) else {
            panic!("{}: local game missing", path.display());
        };
        let sections = load_sections(&text);
        check_sections(r.case, &sections, &refs);
        nodes += check_select(r.case, &sections, &refs);
        tracks += 1;
    }
    assert_eq!(tracks, 15);
    assert!(nodes > 20_000, "{nodes}");
}

mod support {
    use nfs_assets::physics::original_support::SupportPolygon;
    use nfs_assets::physics::original_support_loader::assemble_initial_track_support;
    use nfs_assets::physics::original_support_tree::SupportTree;
    use nfs_assets::RoadSurface;

    pub struct Track {
        pub surface: RoadSurface,
    }

    impl Track {
        pub fn load(dir: &std::path::Path, name: &str) -> Self {
            let raw = std::fs::read(dir.join(format!("{name}.crp"))).unwrap();
            let crp = nfs_formats::track::parse_track_crp(&raw).unwrap();
            let anim_text = std::fs::read_to_string(dir.join("animdefs.txt")).unwrap();
            let anim = nfs_formats::animdefs::parse_animdefs(&anim_text).unwrap();
            let load = assemble_initial_track_support(&crp, &anim).unwrap();
            let names = vec![String::new(); crp.articles.len()];
            Self { surface: RoadSurface::from_original_scene_support(load, &names, vec![]).unwrap() }
        }

        pub fn arena(&self) -> (&SupportTree, &[SupportPolygon]) {
            self.surface.original_arena().unwrap()
        }
    }
}

#[test]
fn light_update_and_layer_on_real_tracks_match_the_original() {
    skip_without_local_fixtures!(TRACKS);
    let all = rows(&TRACKS);
    let mut tracks: HashMap<String, (Vec<EnvSection>, support::Track)> = HashMap::new();
    let mut updates = 0;
    let mut covered = 0;
    let mut uvs = 0;
    let mut light = None;
    for r in &all {
        match r.kind {
            "update" => {
                let track = r.case.split('/').next().unwrap().to_string();
                let (sections, t) = tracks.entry(track.clone()).or_insert_with(|| {
                    let text = std::fs::read(game().join(format!("{track}.map"))).unwrap();
                    (load_sections(&text), support::Track::load(&game(), &track))
                });
                let x = fields(r.out);
                let frame = hexes(x["car_frame"]);
                let fr = |k: usize| [0, 1, 2].map(|i| f32::from_bits(frame[3 * k + i]));
                let position = v3(x["pos"]);
                // 0x473330 on the locator copy (cell cache `+0x2c` as the locate left it).
                let (tree, polygons) = t.arena();
                let cache: i64 = x["cache"].parse().unwrap();
                let cached = (cache >= 0)
                    .then(|| tree.nodes.iter().position(|n| i64::from(n.cell.packed) == cache))
                    .flatten();
                let (flags, _) = tree.overhead_flags(polygons, position, cached);
                assert_eq!(format!("{flags:04x}"), x["roof"], "{}: 0x473330 flags", r.case);
                let car = EnvCar {
                    position,
                    node: x["node"].parse().unwrap(),
                    covered: flags & 0xc000 != 0,
                    node_pos: v3(x["node_pos"]),
                    node_lat: v3(x["node_lat"]),
                    node_fwd: v3(x["node_fwd"]),
                    frame: [fr(0), fr(1), fr(2)],
                };
                covered += car.covered as usize;
                let got = update_light(sections, &car, true);
                let want = light_from(&x);
                assert_eq!(got.slots, want.slots, "{}: slots", r.case);
                assert_eq!(got.k.to_bits(), want.k.to_bits(), "{}", r.case);
                assert_eq!(got.inv_size.to_bits(), want.inv_size.to_bits(), "{}: 1/texturesize", r.case);
                assert_eq!(got.aspect.to_bits(), want.aspect.to_bits(), "{}: aspect", r.case);
                assert_eq!(got.position.map(f32::to_bits), want.position.map(f32::to_bits), "{}", r.case);
                assert_eq!(got.frame_t.map(f32::to_bits), want.frame_t.map(f32::to_bits), "{}: frame", r.case);
                assert_eq!(got.offsets.map(f32::to_bits), want.offsets.map(f32::to_bits), "{}: offsets", r.case);
                assert_eq!(x["off"], "0");
                light = Some(got);
                updates += 1;
            }
            "draw" => {
                uvs += check_draw(&format!("{} draw {}", r.case, r.key), light.as_ref().unwrap(), false, r.out);
            }
            _ => {}
        }
    }
    assert!(updates >= 20, "{updates} placements");
    assert!(covered >= 3, "{covered} placements on covered road");
    assert!(uvs > 1000, "{uvs} coordinates");
}

/// The render methods of every shipped car as 0x44bf70 builds them from the CRP `RMth` chunks
/// (per `mt` record flags `+0xc`, capability words 1 = CARCHROME 0 and 3 = CARCHROME 1 / 2):
/// a material carries the sky-map layer (`R3DSkyMapTextureLayer`, vtable 0x5b398c) exactly when
/// the loader gives it an env slot (`CarExtEnv`, `CarWindow`; never `CarExt`, `CarInt`,
/// `CarWheel`); the layer's dependencies are the normal channel (attribute 1 / 6), the
/// position channel (1 / 7) and the byte index channel 0x8009 when the record has one, and its
/// pass ends with a draw layer (type 0) whose UV source is the sky-map layer.
#[test]
fn car_render_methods_carry_the_sky_map_like_the_original() {
    skip_without_local_fixtures!(TRACKS);
    let all = rows(&TRACKS);
    let car_dir = game().join("../CarModel");
    let mut files = nfs_assets::AssetFiles::new();
    for e in std::fs::read_dir(&car_dir).unwrap().flatten() {
        files.insert(e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap());
    }
    let mut methods: HashMap<(String, String), &str> = HashMap::new();
    let mut mts: HashMap<String, Vec<(u16, String, u32)>> = HashMap::new();
    for r in &all {
        match r.kind {
            "method" => {
                methods.insert((r.case.to_string(), r.key.to_string()), r.out);
            }
            "mt" => {
                let x = fields(r.out);
                mts.entry(r.case.to_string()).or_default().push((
                    r.key.parse().unwrap(),
                    x["method"].to_string(),
                    hex(x["flags"]),
                ));
            }
            _ => {}
        }
    }
    assert!(mts.len() >= 30, "{} cars", mts.len());
    let (mut with, mut indexed, mut checked) = (0, 0, 0);
    for (car, list) in &mts {
        let Some(file) = files.keys().find(|k| k.to_ascii_lowercase() == format!("{car}.crp")).cloned() else {
            continue;
        };
        let stem = file.trim_end_matches(".crp").to_string();
        let Ok(scene) = nfs_assets::load_car(&files, &stem) else {
            continue;
        };
        for (index, method, flags) in list {
            let layers = |caps: u32| -> Vec<(String, u32, u32, Vec<i64>)> {
                methods[&(car.clone(), format!("{method}/{flags:x}/{caps}"))]
                    .split('|')
                    .map(|l| {
                        let c: Vec<&str> = l.split(':').collect();
                        let deps = if c[3].is_empty() { vec![] } else { c[3].split(',').map(|d| d.parse().unwrap()).collect() };
                        (c[0].to_string(), hex(c[1]), hex(c[2]), deps)
                    })
                    .collect()
            };
            let full = layers(3);
            let sky = full.iter().position(|l| l.0 == "skymap");
            let Some(material) = scene.materials.iter().find(|m| m.name == format!("mt{index}:{method}")) else {
                continue;
            };
            assert_eq!(sky.is_some(), material.env_slot.is_some(), "{car} {}", material.name);
            checked += 1;
            let Some(k) = sky else {
                continue;
            };
            with += 1;
            let deps = &full[k].3;
            let attr = |d: i64| full.get(d as usize).map(|l| (l.1, l.2));
            assert_eq!(attr(deps[0]), Some((1, 6)), "{car} {method}: normals");
            assert_eq!(attr(deps[1]), Some((1, 7)), "{car} {method}: positions");
            match deps[2] {
                -1 => {}
                d => {
                    assert_eq!(attr(d), Some((0x8009, 0)), "{car} {method}: index channel");
                    indexed += 1;
                }
            }
            let draw = full[k + 1..].iter().position(|l| l.0 == "draw").map(|d| d + k + 1).unwrap();
            assert_eq!(full[draw].3[3], k as i64, "{car} {method}: the pass draws the sky-map UVs");
            // CARCHROME 0 (capability word 1): only the windows keep the pass (their slots stay
            // empty without 0x6197cb, so 0x445b10 skips it).
            let low = layers(1);
            assert_eq!(low.iter().any(|l| l.0 == "skymap"), method == "CarWindow", "{car} {method} caps 1");
        }
    }
    eprintln!("{checked} materials, {with} with the sky map, {indexed} of them with the index channel");
    assert!(checked > 1000 && with > 500);
}

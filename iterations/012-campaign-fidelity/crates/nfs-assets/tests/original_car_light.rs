//! The original car lighting recorded in Unicorn, compared bit for bit with
//! `nfs_assets::car_light` (and `car_env` for the sky-map coordinates of the full draw):
//! - `Road_SampleColour` 0x4746a0 -> 0x474700 on synthetic polygons with random RGB555
//!   colours (committed fixtures/car-light.tsv, replay-car-light.py --synthetic) and on the real
//!   support polygon under the car (local car-light-tracks.tsv) == `road_colour`;
//! - the lighting part of `R3DCar_UpdateLighting` 0x43a560 (0x43a8a6..0x43ad59 on synthetic
//!   headers, suns, road colours, covered / open, specular options, camera types; the whole
//!   function on Forest node 700 (tree shade), Coastal nodes 190..198 (tunnel), nodes outside
//!   every section) == `light_state`;
//! - `R3DLightLayer::Apply` 0x446530 on synthetic draw contexts (slots 0..2, kA / kD, kD <= 0,
//!   skipped polygons, flags 0x100 / 0x200, polygon size 1, the indexed cache path, other
//!   strides) == `VertexLight`;
//! - the full method draw of the 356A and the 993 (local car-draw-cars.tsv, replay-car-draw.py:
//!   scene loader 0x44a770, binder 0x44a250, method 0x53f1b0 down to dx7z.dll and D3D7):
//!   every pass-1 vertex colour == `VertexLight::colour` of the slot's model normal; every
//!   sky-map coordinate == `car_env::layer_uv` on the path `sky_uv_path(indexed, flag 0x100)`;
//!   the sphere-map and sky-map passes send the pass-1 diffuse unchanged (only u, v differ);
//!   with the whole car in view the group flag 0x100 is clear exactly when the plane test
//!   culled a polygon of the group (`unclipped`).
//!
//! Spec: original-lighting (car vertex lighting, car sky map).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;

use std::collections::HashMap;

use local_fixtures::Fixture;
use nfs_assets::car_env::{layer_matrix, layer_uv, EnvLight};
use nfs_assets::car_light::{
    any_back_face, light_state, material_indexed, material_light_params, material_plane_test, road_colour, sky_uv_path, unclipped, CarLightState, LightInputs, LightParams,
    TrackLightHeader, VertexLight,
};

const SYNTHETIC: &str = include_str!("fixtures/car-light.tsv");
static TRACKS: Fixture = Fixture::new("car-light-tracks.tsv");
static DRAW: Fixture = Fixture::new("car-draw-cars.tsv");

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

fn floats(s: &str) -> Vec<f32> {
    s.split(',').map(|w| f32::from_bits(hex(w))).collect()
}

fn v3(s: &str) -> [f32; 3] {
    let v = floats(s);
    [v[0], v[1], v[2]]
}

fn q4(s: &str) -> [f32; 4] {
    let v = floats(s);
    [v[0], v[1], v[2], v[3]]
}

fn state_of(x: &HashMap<&str, &str>) -> CarLightState {
    let w = hexes(x["state"]);
    CarLightState::from_words(&w.try_into().unwrap(), x["mirror"] == "1")
}

fn header_of(s: &str) -> TrackLightHeader {
    TrackLightHeader::from_words(&floats(s).try_into().unwrap())
}

fn polygon(verts: &str, colours: &str) -> (Vec<[f32; 3]>, Vec<u16>) {
    (verts.split('|').map(v3).collect(), colours.split(',').map(|c| hex(c) as u16).collect())
}

fn words4(s: &str) -> [u32; 4] {
    hexes(s).try_into().unwrap()
}

#[test]
fn road_colour_matches_recorded_0x474700() {
    let mut polys = Vec::new();
    let mut checked = 0;
    for r in rows(SYNTHETIC) {
        let x = fields(r.out);
        match r.kind {
            "polygon" => polys.push(polygon(x["vertices"], x["colours"])),
            "colour" => {
                let k: i32 = x["poly"].parse().unwrap();
                let p = (k >= 0).then(|| &polys[k as usize]);
                let got = road_colour(p.map(|(v, c)| (v.as_slice(), c.as_slice())), v3(x["point"]));
                assert_eq!(got.map(f32::to_bits), words4(x["out"]), "point {}", r.key);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 60, "{checked}");
}

#[test]
fn light_state_matches_recorded_0x43a560() {
    let mut checked = 0;
    for r in rows(SYNTHETIC).iter().filter(|r| r.kind == "light" && r.key != "-") {
        let x = fields(r.out);
        let k: Vec<u32> = r.key.split(',').map(|v| v.parse().unwrap()).collect();
        let inputs = LightInputs {
            road: q4(x["S"]),
            covered: k[3] == 1,
            sun: q4(x["sun"]),
            specular_option: k[0] >= 2 || k[1] as i32 > 0,
            mirror_camera: k[2] == 1,
        };
        let got = light_state(&header_of(x["header"]), &inputs);
        assert_eq!(got.words().to_vec(), hexes(x["state"]), "{} {}", r.case, r.key);
        assert_eq!(got.mirror, x["mirror"] == "1");
        checked += 1;
    }
    assert_eq!(checked, 36);
}

/// The slots 0x446530 writes: `-` untouched, else the colour of the slot's normal.
fn expected_slots(light: &VertexLight, x: &HashMap<&str, &str>) -> Vec<Option<u32>> {
    let normals: Vec<[f32; 3]> = x["normals"].split('|').map(v3).collect();
    let index: Option<Vec<usize>> = (x["index"] != "-").then(|| x["index"].split(',').map(|v| v.parse().unwrap()).collect());
    let count = index.as_ref().map_or(normals.len(), |i| i.len());
    let flags = hex(x["flags"]);
    let poly: usize = x["poly"].parse().unwrap();
    let skip: Vec<usize> = if x["skip"] == "-" { vec![] } else { x["skip"].split(',').map(|v| v.parse().unwrap()).collect() };
    if flags & 0x200 != 0 {
        return vec![None; count];
    }
    let normal = |i: usize| normals[index.as_ref().map_or(i, |ix| ix[i])];
    (0..count)
        .map(|i| {
            if !light.lit {
                return Some(light.ambient_word);
            }
            let culled = flags & 0x100 == 0 && poly != 1 && skip.contains(&(i / poly));
            (!culled).then(|| light.colour(normal(i)))
        })
        .collect()
}

#[test]
fn vertex_colours_match_recorded_0x446530() {
    let mut state = None;
    let mut checked = 0;
    for r in rows(SYNTHETIC) {
        let x = fields(r.out);
        match r.kind {
            "light" => state = Some(state_of(&x)),
            "vertex" => {
                let p: Vec<&str> = x["param"].split(',').collect();
                let params = LightParams { ka: f32::from_bits(hex(p[0])), kd: f32::from_bits(hex(p[1])), slot: p[2].parse().unwrap() };
                let model: [f32; 16] = floats(x["model"]).try_into().unwrap();
                let light = VertexLight::new(state.as_ref().unwrap(), params, &model).unwrap();
                let want: Vec<Option<u32>> = x["colour"].split('|').map(|c| (c != "-").then(|| hex(c))).collect();
                assert_eq!(expected_slots(&light, &x), want, "{} {}", r.case, r.key);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 100, "{checked}");
}

#[test]
fn local_track_lighting_matches_recorded_0x43a560() {
    skip_without_local_fixtures!(TRACKS);
    let mut header = None;
    let mut sun = [0.0; 4];
    let mut checked = 0;
    for r in rows(&TRACKS) {
        let x = fields(r.out);
        match r.kind {
            "track" => {
                header = Some(header_of(x["header"]));
                sun = q4(x["sun"]);
            }
            "place" => {
                let s = if x["poly"] == "-1" {
                    road_colour(None, v3(x["pos"]))
                } else {
                    let (v, c) = polygon(x["poly_vertices"], x["poly_colours"]);
                    road_colour(Some((&v, &c)), v3(x["pos"]))
                };
                assert_eq!(s.map(f32::to_bits), words4(x["S"]), "{} S", r.case);
                let inputs = LightInputs { road: s, covered: x["covered"] == "1", sun, specular_option: false, mirror_camera: false };
                let got = light_state(header.as_ref().unwrap(), &inputs);
                assert_eq!(got.words().to_vec(), hexes(x["state"]), "{}", r.case);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 12, "{checked}");
}

fn load_crp(car: &str) -> nfs_formats::Crp {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/CarModel");
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(&format!("{car}.crp"))))
        .unwrap();
    nfs_formats::parse_crp(&std::fs::read(path).unwrap()).unwrap()
}

struct Group<'a> {
    case: &'a str,
    x: HashMap<&'a str, &'a str>,
    light_param: &'a str,
    sky_indexed: bool,
    sources: HashMap<&'a str, &'a str>,
    passes: Vec<HashMap<&'a str, &'a str>>,
    dips: Vec<&'a str>,
}

fn env_of(x: &HashMap<&str, &str>) -> EnvLight {
    let p = v3(x["env_pos"]);
    let o = floats(x["o"]);
    EnvLight {
        slots: [None; 3],
        k: floats(x["k"])[0],
        inv_size: floats(x["inv_size"])[0],
        aspect: floats(x["aspect"])[0],
        position: p,
        frame_t: floats(x["frame"]).try_into().unwrap(),
        offsets: [o[0], o[1]],
    }
}

#[test]
fn local_car_draw_matches_recorded_method_passes() {
    skip_without_local_fixtures!(DRAW);
    let all = rows(&DRAW);
    let mut cases = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for r in &all {
        match r.kind {
            "case" => {
                cases.insert(r.case, fields(r.out));
            }
            "group" => groups.push(Group {
                case: r.case,
                x: fields(r.out),
                light_param: "-",
                sky_indexed: false,
                sources: HashMap::new(),
                passes: vec![],
                dips: vec![],
            }),
            "param" if r.key == "light" => groups.last_mut().unwrap().light_param = r.out,
            "param" if r.key == "sky_indexed" => groups.last_mut().unwrap().sky_indexed = r.out == "1",
            "source" => {
                groups.last_mut().unwrap().sources.insert(r.key, r.out);
            }
            "pass" => groups.last_mut().unwrap().passes.push(fields(r.out)),
            "d3d" if r.out.starts_with("dip:") => groups.last_mut().unwrap().dips.push(r.out),
            _ => {}
        }
    }
    let mut crps: HashMap<&str, nfs_formats::Crp> = HashMap::new();
    let (mut colours, mut uvs, mut swapped, mut flag_checks) = (0, 0, 0, 0);
    let (mut geometric, mut geometric_total) = (0, 0);
    for g in &groups {
        let cid = g.case.splitn(5, '/').take(4).collect::<Vec<_>>().join("/");
        let c = &cases[cid.as_str()];
        let state = CarLightState::from_words(&hexes(c["light"]).try_into().unwrap(), c["mirror"] == "1");
        let model: [f32; 16] = hexes(c["model"]).iter().map(|&w| f32::from_bits(w)).collect::<Vec<_>>().try_into().unwrap();
        let poly: usize = g.x["poly"].parse().unwrap();
        // Pass 1: the light layer's colour of every drawn slot.
        if g.light_param != "-" && !g.passes.is_empty() {
            let b: Vec<u8> = (0..12).map(|i| u8::from_str_radix(&g.light_param[2 * i..2 * i + 2], 16).unwrap()).collect();
            let params = LightParams::from_bytes(&b).unwrap();
            // The loader's reading of the material: its 12-byte layer entry, its method flags.
            let car = g.case.split('/').next().unwrap();
            let crp = crps.entry(car).or_insert_with(|| load_crp(car));
            let mt = crp.misc.iter().find(|e| e.tag == "mt" && e.index.to_string() == g.x["material"]).unwrap();
            assert_eq!(material_light_params(&mt.data), Some(params), "{}", g.case);
            let flags = hex(g.x["mt_flags"]);
            assert_eq!(material_plane_test(flags), g.x["layers"].starts_with("channel:1:8000"), "{}", g.case);
            if g.sources.contains_key("sky") {
                assert_eq!(material_indexed(flags), g.sky_indexed, "{}", g.case);
            }
            let light = VertexLight::new(&state, params, &model).unwrap();
            let p = &g.passes[0];
            let skip = p["skip"].as_bytes();
            for (i, (slot, src)) in p["slots"].split('|').zip(g.sources["light"].split('|')).enumerate() {
                if skip.get(i / poly) == Some(&b'1') {
                    continue;
                }
                let w = floats(src);
                let diffuse = u32::from_str_radix(&slot[32..40], 16).unwrap().swap_bytes();
                assert_eq!(light.colour([w[0], w[1], w[2]]), diffuse, "{} slot {i}", g.case);
                colours += 1;
            }
        }
        // Pass 6: the sky-map coordinates on the recorded path.
        let sky_pass = g.x["layers"].split('|').filter(|l| l.starts_with("draw:")).count();
        if let (Some(src), true) = (g.sources.get("sky"), sky_pass >= 3) {
            let p = &g.passes[2];
            let flags = hex(p["flags"]);
            let path = sky_uv_path(g.sky_indexed, flags & 0x100 != 0);
            if path == nfs_assets::car_env::UvPath::Swapped {
                swapped += 1;
            }
            let env = env_of(c);
            let m = layer_matrix(&env, &model);
            let skip = p["skip"].as_bytes();
            for (i, (slot, s)) in p["slots"].split('|').zip(src.split('|')).enumerate() {
                if skip.get(i / poly) == Some(&b'1') {
                    continue;
                }
                let w = floats(s);
                let got = layer_uv(&env, &m, [w[0], w[1], w[2]], [w[3], w[4], w[5]], path);
                let u = u32::from_str_radix(&slot[48..56], 16).unwrap().swap_bytes();
                let v = u32::from_str_radix(&slot[56..64], 16).unwrap().swap_bytes();
                assert_eq!(got.map(f32::to_bits), [u, v], "{} slot {i}", g.case);
                uvs += 1;
            }
        }
        // The later passes send the pass-1 vertices with other coordinates only.
        if g.dips.len() > 1 {
            let vb = |d: &str| d.split("vb=").nth(1).unwrap().to_string();
            let first = vb(g.dips[0]);
            for d in &g.dips[1..] {
                let other = vb(d);
                if other.len() != first.len() {
                    continue;
                }
                for k in 0..first.len() / 64 {
                    assert_eq!(first[64 * k..64 * k + 48], other[64 * k..64 * k + 48], "{} vertex {k}", g.case);
                }
            }
        }
        // The tail camera sees the whole car: flag 0x100 <=> no back face culled (plane test),
        // and the geometric test of the group's triangles finds the same.
        if g.case.split('/').nth(3) == Some("0") && g.x["layers"].starts_with("channel:1:8000") && !g.passes.is_empty() {
            let flag = hex(g.passes[0]["flags"]) & 0x100 != 0;
            let culled = g.passes[0]["skip"].contains('1');
            assert_eq!(flag, unclipped(true, true, culled), "{}", g.case);
            if let Some(src) = g.sources.get("sky") {
                let eye = v3(c["eye"]);
                let t = [model[12], model[13], model[14]];
                let d = [0, 1, 2].map(|i| eye[i] as f64 - t[i] as f64);
                let eye_model = [0, 1, 2].map(|r| (0..3).map(|i| d[i] * model[4 * r + i] as f64).sum::<f64>() as f32);
                let verts: Vec<[f32; 3]> = src.split('|').map(v3).collect();
                let tris: Vec<[[f32; 3]; 3]> = verts.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                if any_back_face(&tris, eye_model) == culled {
                    geometric += 1;
                }
                geometric_total += 1;
            }
            flag_checks += 1;
        }
    }
    assert!(colours > 10_000 && uvs > 10_000 && swapped > 100 && flag_checks > 100, "{colours} {uvs} {swapped} {flag_checks}");
    // The geometric back-face test the viewer uses agrees with the recorded plane test.
    assert_eq!(geometric, geometric_total, "geometric back-face test: {geometric} of {geometric_total}");
}

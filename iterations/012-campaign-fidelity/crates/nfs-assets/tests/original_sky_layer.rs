//! Sky-map layers of the original recorded by executing nfs5.exe `SkyMapLayer_Ctor` 0x45b5a0
//! (with every callee: txt tokens, pack lookup, map objects 0x45dd90 / 0x45e160, dome
//! 0x460f40, UV grid 0x45e8e0 -> 0x45f1b0 / 0x45ec40, mesh channels 0x468630 -> 0x468440),
//! the UV update 0x45f0a0 (0x45ed40 drift, 0x45f180 A/B blend) and the quad draw path
//! (0x540660 material state, 0x57e000 -> THRASH_drawquad of Drivers/dx7/dx7z.dll down to a
//! logging Direct3D 7 device) in Unicorn on synthetic txt blocks, maps and packs
//! (scripts/research/replay-sky-layer.py). No game data: the fixture is committed.
//!
//! Compared bit for bit with the engine's port (`nfs_formats::sky`): layer count, cloud
//! choice, material blending, vertex colours (`sky_vertex_colour` on the FSH decode of the
//! synthetic pack), UVs (`sky_layer_uv`), wind step / drift / A-B blend, dome vertices
//! (`SkyDome::vertices`, and `SkyDome::grid_position` inverts them), the grid quad order, the
//! map mix (`sky_map_mix`), and the driver's quad split and render states
//! (`render_state::apply_layer_state`).
//!
//! Spec: original-lighting tasks 9.15 / 9.16 (sky dome MULTI* layers).

use std::collections::HashMap;

use nfs_assets::precipitation::GameRandom;
use nfs_assets::render_state::{apply_layer_state, D3dCall, DriverCaps, Dx7Driver, View, CACHE_SENTINEL};
use nfs_formats::{
    parse_fsh_entries, parse_sky_txt, sky_blend, sky_drift_advance, sky_inv_scale, sky_layer_blends, sky_layer_uv,
    sky_map_mix, sky_map_opaque, sky_vertex_colour, sky_wind_step, SkyDome, SkyGridAnim, SkyMapBlock,
};

const RECORDING: &str = include_str!("fixtures/sky-layer.tsv");

struct Row {
    kind: &'static str,
    case: &'static str,
    key: &'static str,
    out: &'static str,
}

fn rows() -> Vec<Row> {
    let mut lines = RECORDING.lines().filter(|l| !l.starts_with('#'));
    assert_eq!(lines.next(), Some("kind\tcase\tkey\tout"));
    lines
        .map(|l| {
            let c: Vec<&str> = l.splitn(4, '\t').collect();
            assert_eq!(c.len(), 4, "{l}");
            Row { kind: c[0], case: c[1], key: c[2], out: c[3] }
        })
        .collect()
}

fn f(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// `k=v;k=v` fields after an optional leading value.
fn fields(s: &str) -> HashMap<&str, &str> {
    s.split(';').filter_map(|p| p.split_once('=')).collect()
}

fn pair(s: &str) -> [f32; 2] {
    let (a, b) = s.split_once(',').unwrap();
    [f(a), f(b)]
}

struct Image {
    format: u32,
    rgba: Vec<u8>,
}

struct Case {
    block: SkyMapBlock,
    mode: i32,
    dome: SkyDome,
    alt: Option<SkyDome>,
}

fn kind_name(flags: u32) -> &'static str {
    match flags & 0x3c {
        4 => "MULTINORMAL",
        8 => "MULTIADDITIVE",
        0x10 => "MULTIATTENUATE",
        _ => "MULTIMODULATE",
    }
}

fn parse_case(out: &str) -> Case {
    let mut parts = out.split(';');
    let tokens = parts.next().unwrap();
    let rest: HashMap<&str, &str> = parts.filter_map(|p| p.split_once('=')).collect();
    let flags = u32::from_str_radix(rest["flags"], 16).unwrap();
    let txt = format!(
        "# synthetic\nBEGIN\n\tHIGH\n\t{}\n{}\nEND\n",
        kind_name(flags),
        tokens.split(' ').map(|t| format!("\t{t}")).collect::<Vec<_>>().join("\n")
    );
    let block = parse_sky_txt(&txt).into_iter().next().expect("block");
    let d: Vec<f32> = rest["dome"].split(',').map(f).collect();
    let dome = SkyDome { angle: d[0], size: d[1], curvature: d[2], height: d[3], scale: d[4] };
    let alt = rest.get("alt").map(|a| {
        let d: Vec<f32> = a.split(',').map(f).collect();
        SkyDome { angle: d[0], size: d[1], curvature: d[2], height: d[3], scale: 1.0 }
    });
    Case { block, mode: rest["mode"].parse().unwrap(), dome, alt }
}

/// Recorded texel bytes are the FSH 0x7d memory order B, G, R, A (= the D3D ARGB dword).
fn engine_vertex_bgra(rgba: &[u8], column: u32, row: u32) -> [u8; 4] {
    let c = sky_vertex_colour(rgba, column as f32, row as f32).map(|v| (v * 255.0).round() as u8);
    [c[2], c[1], c[0], c[3]]
}

fn bgra(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect()
}

/// The warp case's grid as the build leaves it: the generator in its .data state (the
/// recorder starts every case from the pristine images), the map objects' 200 sprites x 2
/// numbers per map (0x45dd90), then `SkyGridAnim::new` (0x45e8e0).
fn warp_grid(b: &SkyMapBlock) -> (SkyGridAnim, GameRandom, Option<String>) {
    let mut rng = GameRandom::default();
    for _ in 0..b.maps.iter().filter(|m| m.as_str() != "none").count() * 400 {
        rng.unit();
    }
    let anim = SkyGridAnim::new(b, b.drawn_layers(), &mut || rng.unit());
    (anim, rng, None)
}

#[test]
fn sky_layer_build_matches_the_original_bit_for_bit() {
    let rows = rows();
    let shpi = rows.iter().find(|r| r.kind == "shpi").expect("shpi row");
    let images: HashMap<String, Image> = parse_fsh_entries(&hex_bytes(shpi.out))
        .unwrap()
        .into_iter()
        .map(|e| (e.name.clone(), Image { format: e.format, rgba: e.image.unwrap().rgba }))
        .collect();
    let mut cases: HashMap<&str, Case> = HashMap::new();
    let mut names: HashMap<&str, &str> = HashMap::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut warped = 0;
    let mut warp_anim: Option<(SkyGridAnim, GameRandom, Option<String>)> = None;
    // Drift state per (case, layer): offset.
    let mut offsets: HashMap<(String, usize), [f32; 2]> = HashMap::new();
    for r in &rows {
        *counts.entry(r.kind).or_default() += 1;
        if r.kind == "case" {
            cases.insert(r.case, parse_case(r.out));
            names.insert(r.case, r.key);
            continue;
        }
        let Some(case) = cases.get(r.case) else { continue };
        let name = names[r.case];
        let warp = name == "warp-move";
        let b = &case.block;
        let rb = b.region_b.as_deref().unwrap();
        let map_rgba = |slot: usize, region_b: bool| {
            let n = if region_b { &rb.maps[slot] } else { &b.maps[slot] };
            images.get(n.as_str()).map(|i| i.rgba.as_slice())
        };
        let label = format!("{} case {} ({name}) {}", r.kind, r.case, r.key);
        match r.kind {
            "layer" => {
                let fl = fields(r.out);
                assert_eq!(fl["layers"].parse::<usize>().unwrap(), b.drawn_layers(), "{label}: layer count");
            }
            "cloud" => {
                let i: usize = r.key.parse().unwrap();
                let (what, rest) = r.out.split_once(';').unwrap();
                let fl = fields(rest);
                let cloud = b.clouds[i].as_str();
                let expect = match cloud {
                    "none" => "none".to_string(),
                    "flat" => "flat:pack0".to_string(),
                    n => format!("image:{n}:tag=534b5920"),
                };
                assert_eq!(what, expect, "{label}: cloud texture of slot {i}");
                if i < b.drawn_layers() {
                    let m: Vec<i32> = fl["material"].split(',').map(|v| v.parse().unwrap()).collect();
                    let same = b.maps[i] == rb.maps[i];
                    let opaque = sky_map_opaque(map_rgba(i, false), map_rgba(i, true), same);
                    let format = images.get(cloud).map(|i| i.format);
                    let blends = sky_layer_blends(cloud, format, opaque);
                    assert_eq!(m, [1, if blends { 2 } else { 0 }, case.mode, 0, 0, -1], "{label}: material block");
                }
            }
            "step" => {
                let i: usize = r.key.parse().unwrap();
                let fl = fields(r.out);
                let (step, moves_a) = sky_wind_step(b.wind_speed[i], b.wind_direction[i]);
                let (_, moves_b) = sky_wind_step(rb.wind_speed[i], rb.wind_direction[i]);
                assert_eq!(pair(fl["step"]).map(f32::to_bits), step.map(f32::to_bits), "{label}: wind step");
                assert_eq!(fl["move"] == "1", moves_a || moves_b, "{label}: move flag");
                let inv = [sky_inv_scale(b.x_scale[i]), sky_inv_scale(b.y_scale[i])];
                assert_eq!(pair(fl["inv"]).map(f32::to_bits), inv.map(f32::to_bits), "{label}: 1/scale");
            }
            "vertex" => {
                let (i, v) = r.key.split_once('.').unwrap();
                let (i, v): (usize, u32) = (i.parse().unwrap(), v.parse().unwrap());
                let (col, row) = (v % 16, v / 16);
                let mut p = r.out.split(';');
                let colour = hex_bytes(p.next().unwrap());
                let rgba = map_rgba(i, false).expect("map");
                assert_eq!(colour, engine_vertex_bgra(rgba, col, row), "{label}: vertex colour");
                let uv = [f(p.next().unwrap()), f(p.next().unwrap())];
                if warp {
                    // The build's warp start (0x45e8e0 -> 0x45f3c0 on the game generator).
                    let (anim, _, _) = warp_anim.get_or_insert_with(|| warp_grid(b));
                    assert_eq!(uv.map(f32::to_bits), anim.uv(i, col, row).map(f32::to_bits), "{label}: warped uv");
                    warped += 1;
                    continue;
                }
                let inv = [sky_inv_scale(b.x_scale[i]), sky_inv_scale(b.y_scale[i])];
                let ours = sky_layer_uv(col, row, inv, [0.0; 2], [0.0; 2], [0.0; 2]);
                assert_eq!(uv.map(f32::to_bits), ours.map(f32::to_bits), "{label}: uv");
            }
            "dome" => {
                let v: usize = r.key.parse().unwrap();
                let d: Vec<f32> = r.out.split(',').map(f).collect();
                let dome = case.alt.unwrap_or(case.dome);
                let ours = dome.vertices()[v];
                assert_eq!(
                    [d[0], d[1], d[2]].map(f32::to_bits),
                    ours.map(f32::to_bits),
                    "{label}: dome vertex {v}"
                );
                assert_eq!(d[3], 1.0, "{label}: w");
                // The shader's ray / paraboloid inverse lands on the grid point of the vertex.
                // (Border vertices may fall a rounding error outside the square.)
                let border = matches!(v % 16, 0 | 15) || matches!(v / 16, 0 | 15);
                if let Some((col, row)) = dome.grid_position([d[0], d[1], d[2]]) {
                    assert!(
                        (col - (v % 16) as f32).abs() < 2e-3 && (row - (v / 16) as f32).abs() < 2e-3,
                        "{label}: grid_position {col} {row}"
                    );
                } else {
                    assert!(border, "{label}: interior vertex off the dome");
                }
            }
            "quads" => {
                let q = hex_bytes(r.out);
                assert_eq!(q.len(), 225 * 4, "{label}");
                let mut seen = vec![false; 225];
                for quad in q.chunks_exact(4) {
                    let i = quad[0] as usize;
                    let (c, rw) = (i % 16, i / 16);
                    assert!(c < 15 && rw < 15, "{label}: quad {quad:?}");
                    // Quad (c, r) = vertices (c, r), (c+1, r), (c+1, r+1), (c, r+1): the order
                    // `sky_vertex_colour` splits as (0, 1, 2), (0, 2, 3).
                    assert_eq!(quad, [i as u8, (i + 1) as u8, (i + 17) as u8, (i + 16) as u8], "{label}");
                    seen[rw * 15 + c] = true;
                }
                assert!(seen.iter().all(|&s| s), "{label}: every cell once");
            }
            "drift" => {
                let (_, layer) = r.key.split_once('.').unwrap();
                let i: usize = layer.parse().unwrap();
                let fl = fields(r.out);
                let t = f(fl["blend"]);
                let ticks: i32 = fl["ticks"].parse().unwrap();
                let (sa, ma) = sky_wind_step(b.wind_speed[i], b.wind_direction[i]);
                let (sb, mb) = sky_wind_step(rb.wind_speed[i], rb.wind_direction[i]);
                let step = [sky_blend(sa[0], sb[0], t), sky_blend(sa[1], sb[1], t)];
                let key = (r.case.to_string(), i);
                let prev = offsets.get(&key).copied().unwrap_or([0.0; 2]);
                let offset = if ma || mb { sky_drift_advance(prev, step, ticks) } else { prev };
                offsets.insert(key, offset);
                assert_eq!(pair(fl["offset"]).map(f32::to_bits), offset.map(f32::to_bits), "{label}: drift offset");
                let camera = pair(fl["camoff"]);
                if warp {
                    // Camera offset and warp: one update per call (`SkyGridAnim`), the camera
                    // argument is the recorder's position (x, z).
                    let (call, _) = r.key.split_once('.').unwrap();
                    let (anim, rng, last) = warp_anim.get_or_insert_with(|| warp_grid(b));
                    if last.as_deref() != Some(call) {
                        let cam: Vec<f32> = fl["cam"].split(',').map(f).collect();
                        anim.update([cam[0], cam[2]], t, ticks, &mut || rng.unit());
                        *last = Some(call.to_string());
                    }
                    assert_eq!(camera.map(f32::to_bits), anim.camera[i].map(f32::to_bits), "{label}: camera offset");
                    for (uv, v) in fl["uv"].split('|').zip([0u32, 1, 17, 255]) {
                        assert_eq!(pair(uv).map(f32::to_bits), anim.uv(i, v % 16, v / 16).map(f32::to_bits), "{label}: uv of vertex {v}");
                    }
                    warped += 1;
                    continue;
                }
                assert_eq!(camera.map(|c| c == 0.0), [true; 2], "{label}: no camera offset");
                let inv_a = [sky_inv_scale(b.x_scale[i]), sky_inv_scale(b.y_scale[i])];
                let inv_b = [sky_inv_scale(rb.x_scale[i]), sky_inv_scale(rb.y_scale[i])];
                // The UVs are rewritten only when a layer moves (flags of 0x45f0a0).
                let any_moves = (0..b.drawn_layers()).any(|k| {
                    sky_wind_step(b.wind_speed[k], b.wind_direction[k]).1
                        || sky_wind_step(rb.wind_speed[k], rb.wind_direction[k]).1
                });
                let (inv, offset) = if any_moves {
                    ([sky_blend(inv_a[0], inv_b[0], t), sky_blend(inv_a[1], inv_b[1], t)], offset)
                } else {
                    (inv_a, [0.0; 2])
                };
                for (uv, v) in fl["uv"].split('|').zip([0u32, 1, 17, 255]) {
                    let ours = sky_layer_uv(v % 16, v / 16, inv, [0.0; 2], offset, camera);
                    assert_eq!(pair(uv).map(f32::to_bits), ours.map(f32::to_bits), "{label}: uv of vertex {v}");
                }
            }
            "mix" => {
                let (i, t) = r.key.split_once('@').unwrap();
                let (i, t): (usize, f32) = (i.parse().unwrap(), f(t));
                let a = bgra(map_rgba(i, false).unwrap());
                let bb = bgra(map_rgba(i, true).unwrap());
                let ours = if b.maps[i] == rb.maps[i] { a.clone() } else { sky_map_mix(&a, &bb, t) };
                assert_eq!(hex_bytes(r.out), ours, "{label}: mixed texels");
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    assert_eq!(counts.get("case"), Some(&19));
    assert_eq!(counts.get("dome"), Some(&(19 * 256)));
    assert!(counts.get("vertex").copied().unwrap_or(0) >= 30 * 256);
    assert!(counts.get("drift").copied().unwrap_or(0) >= 100);
    assert!(counts.get("mix").copied().unwrap_or(0) >= 30);
    // Warp (0x45f3c0 / 0x45ee80) and the camera offset: `SkyGridAnim` (replay-sky-update.py).
    assert_eq!(warped, 2 * 256 + 6);
}

#[test]
fn sky_quads_are_split_0_1_2_and_0_2_3_with_the_layer_states() {
    let colours = [
        0xff102030u32, 0x80405060, 0x00a0b0c0, 0xffff0000, 0x7f00ff00, 0x010000ff, 0xffffffff, 0x00000000, 0x12345678,
    ];
    let quads = [[0usize, 1, 4, 3], [1, 2, 5, 4], [3, 4, 7, 6], [4, 5, 8, 7]];
    let mut n = 0;
    for r in rows().iter().filter(|r| r.kind == "quad") {
        n += 1;
        let block: [i32; 6] = r.key.split(',').map(|v| v.parse().unwrap()).collect::<Vec<_>>().try_into().unwrap();
        let fl = fields(r.out.strip_prefix("dip:").unwrap());
        // TRIANGLELIST (D3DPT 4), two triangles per quad in the order the quad was given.
        assert_eq!(fl["type"], "4", "{block:?}");
        let idx: Vec<u16> = fl["idx"].split(',').map(|v| v.parse().unwrap()).collect();
        let expect: Vec<u16> = (0..4u16).flat_map(|q| [0, 1, 2, 0, 2, 3].map(|k| 4 * q + k)).collect();
        assert_eq!(idx, expect, "{block:?}: quad split");
        // Gouraud with the vertex colours (shade 1); shade 0 = flat and white.
        let diffuse: Vec<u32> = fl["diffuse"].split(',').map(|v| u32::from_str_radix(v, 16).unwrap()).collect();
        let want: Vec<u32> = quads
            .iter()
            .flat_map(|q| q.map(|v| if block[0] == 1 { colours[v] } else { 0xffff_ffff }))
            .collect();
        assert_eq!(diffuse, want, "{block:?}: vertex colours in quad order");
        // The render states of the material block: the engine's port of 0x540660 + dx7z.
        let recorded: Vec<&str> = r.out.split(';').filter(|p| p.starts_with("rs")).collect();
        let mut cache = [CACHE_SENTINEL; 6];
        let mut driver = Dx7Driver::new(DriverCaps::default());
        apply_layer_state(&mut cache, &block, &View { cull_mode: 2, z_base: 0, defer_z: false }, &mut driver);
        let ours: Vec<String> = driver
            .d3d
            .iter()
            .filter_map(|c| match *c {
                D3dCall::RenderState(s, v) => Some(format!("rs{s}={v}")),
                _ => None,
            })
            .collect();
        assert_eq!(recorded, ours, "{block:?}: render states");
        // MULTINORMAL (mode 0) with blending: SRCALPHA, INVSRCALPHA, i.e. layer over the
        // previous by its alpha (the sky shader's `mix(g, c.rgb, c.a)`).
        if block[1..3] == [2, 0] {
            assert!(ours.contains(&"rs27=1".to_string()) && ours.contains(&"rs19=5".to_string()) && ours.contains(&"rs20=6".to_string()));
        }
        if block[1] == 0 {
            assert!(ours.contains(&"rs27=0".to_string()), "{block:?}: blending off");
        }
    }
    assert_eq!(n, 6);
}

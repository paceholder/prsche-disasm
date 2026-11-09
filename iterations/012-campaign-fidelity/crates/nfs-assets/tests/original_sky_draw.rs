//! The original MULTI* sky-layer draw recorded down to Direct3D 7: nfs5.exe
//! `SkyMapLayer_Draw` 0x45bd40 (wedge cull 0x45abb0, mesh draw 0x468410 -> 0x53f1b0, channel
//! descriptors: material 0x540660, textures 0x536240 -> 0x555440 -> THRASH_settexture,
//! indexed colour / UV 0x4499e0 / 0x449910, transform 0x540bc0 -> 0x5385b0, clip / emit
//! 0x545ad0) -> THRASH_drawtrimesh -> Drivers/dx7/dx7z.dll -> a logging D3D7 device, with the
//! camera made and activated by the game's own code and the sky pass's near / far 0.15 / 700
//! (scripts/research/replay-sky-draw.py). Synthetic layers (fixtures/sky-draw.tsv, layers in
//! fixtures/sky-layer.tsv) and, when recorded, the HIGH blocks of every MULTI* track
//! (local/fixtures/sky-draw-tracks.tsv).
//!
//! Compared with the engine (`nfs_formats::sky`):
//! - every TL vertex that is a dome vertex carries exactly the layer's recorded colour bytes
//!   (diffuse) and UV floats, at the projection of `SkyDome::vertices` with the camera rotation
//!   only (z = far/(far-near)(1-near/w), rhw = 1/w);
//! - every unclipped triangle is a `SkyDome::triangles` triangle in that vertex order, and
//!   every triangle of a drawn cell that lies on screen inside the near/far range is drawn;
//! - clipping only adds vertices on the far plane (w = 700, z = 1) or on the screen edges,
//!   and a clipped triangle stays inside one `triangles` triangle;
//! - the wedge cull of each cell (0x45bd40 gating, planes 0x45aaf0, test 0x45abb0) is
//!   `sky_cell_tested` / `sky_cull_planes` / `sky_cell_visible` bit for bit; the cull tests
//!   only the cell's corners and edge midpoints against the horizontal field of view, so
//!   dropped cells can still be on screen (top corners when pitched up): the original shows
//!   no layer there;
//! - flat clouds bind no texture (SetTexture(stage, NULL) on both stages), image clouds their
//!   texture; ALPHABLENDENABLE follows the layer's material;
//! - `SkyDome::hit` (the shader's per-pixel dome hit) at sample pixels of drawn triangles
//!   equals the perspective-correct interpolation of the recorded TL vertices (grid position,
//!   view depth).
//!
//! Spec: original-lighting 9.17 (sky draw path).

#[path = "support/local_fixtures.rs"]
#[macro_use]
mod local_fixtures;

use std::collections::{HashMap, HashSet};

use nfs_formats::{sky_cell_tested, sky_cell_visible, sky_cull_planes, SkyDome, SKY_FAR, SKY_NEAR};

const DRAW: &str = include_str!("fixtures/sky-draw.tsv");
const LAYER: &str = include_str!("fixtures/sky-layer.tsv");

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

fn f(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn fields(s: &str) -> HashMap<&str, &str> {
    s.split(';').filter_map(|p| p.split_once('=')).collect()
}

/// A built layer of the recording: dome vertices, per layer the vertex colour dwords and UV
/// bits, the cloud of each layer and whether its material blends.
#[derive(Default)]
struct Layer {
    dome: Vec<[f32; 3]>,
    colour: HashMap<(usize, usize), u32>,
    uv: HashMap<(usize, usize), [u32; 2]>,
    clouds: Vec<String>,
    blends: Vec<bool>,
}

fn layers(rows: &[Row]) -> HashMap<String, Layer> {
    let mut out: HashMap<String, Layer> = HashMap::new();
    for r in rows {
        match r.kind {
            "dome" => {
                let d: Vec<f32> = r.out.split(',').map(f).collect();
                let layer = out.entry(r.case.to_string()).or_default();
                let v: usize = r.key.parse().unwrap();
                if layer.dome.len() <= v {
                    layer.dome.resize(v + 1, [0.0; 3]);
                }
                layer.dome[v] = [d[0], d[1], d[2]];
            }
            "vertex" => {
                let (i, v) = r.key.split_once('.').unwrap();
                let (i, v): (usize, usize) = (i.parse().unwrap(), v.parse().unwrap());
                let mut p = r.out.split(';');
                let c = hex_bytes(p.next().unwrap());
                let layer = out.entry(r.case.to_string()).or_default();
                layer.colour.insert((i, v), u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                let u = u32::from_str_radix(p.next().unwrap(), 16).unwrap();
                let w = u32::from_str_radix(p.next().unwrap(), 16).unwrap();
                layer.uv.insert((i, v), [u, w]);
            }
            "cloud" => {
                let (what, rest) = r.out.split_once(';').unwrap();
                let layer = out.entry(r.case.to_string()).or_default();
                let material = fields(rest)["material"];
                layer.clouds.push(what.to_string());
                layer.blends.push(material != "-" && material.split(',').nth(1) != Some("0"));
            }
            _ => {}
        }
    }
    out
}

struct Tl {
    x: f32,
    y: f32,
    z: f32,
    rhw: f32,
    diffuse: u32,
    uv: [u32; 2],
}

fn tl(vb: &[u8], i: usize) -> Tl {
    let w = |k: usize| u32::from_le_bytes(vb[32 * i + 4 * k..32 * i + 4 * k + 4].try_into().unwrap());
    Tl {
        x: f32::from_bits(w(0)),
        y: f32::from_bits(w(1)),
        z: f32::from_bits(w(2)),
        rhw: f32::from_bits(w(3)),
        diffuse: w(4),
        uv: [w(6), w(7)],
    }
}

/// Screen of the recording: 640 x 480, the camera's focal length 320 (fov 90), pixel centres
/// at -0.5 (`0x538da0` viewport).
const W: f64 = 640.0;
const H: f64 = 480.0;
const FOCAL: f64 = 320.0;
const CX: f64 = 319.5;
const CY: f64 = 239.5;

fn view(rot: &[f32; 9], p: [f32; 3]) -> [f64; 3] {
    let r = rot.map(f64::from);
    let p = p.map(f64::from);
    [0, 1, 2].map(|i| r[3 * i] * p[0] + r[3 * i + 1] * p[1] + r[3 * i + 2] * p[2])
}

fn project(v: [f64; 3]) -> [f64; 3] {
    [CX + FOCAL * v[0] / v[2], CY - FOCAL * v[1] / v[2], v[2]]
}

fn on_edge(t: &Tl) -> bool {
    let e = |a: f32, b: f64| (f64::from(a) - b).abs() < 0.02;
    e(t.x, -0.49) || e(t.x, W - 0.49) || e(t.y, -0.49) || e(t.y, H - 0.49)
}

#[derive(Default, Debug)]
struct Counts {
    vertices: usize,
    clipped_far: usize,
    clipped_edge: usize,
    triangles: usize,
    complete: usize,
    culled: usize,
    culled_on_screen: usize,
    cull_cells: usize,
    flat_binds: usize,
    image_binds: usize,
    samples: usize,
}

fn check(draw: &[Row], layer_rows: &[Row], label: &str) -> Counts {
    let layers = layers(layer_rows);
    let mut n = Counts::default();
    let mut cases: HashMap<&str, &str> = HashMap::new();
    // (case, camera) -> (rotation, cull string, events)
    let mut cams: Vec<(&str, &str, [f32; 9], f32, String, Vec<&str>)> = Vec::new();
    for r in draw {
        match r.kind {
            "case" => {
                cases.insert(r.case, r.key);
            }
            "camera" => {
                let fl = fields(r.out);
                let rot: [f32; 9] = fl["rot"].split(',').map(f).collect::<Vec<_>>().try_into().unwrap();
                assert_eq!(f(fl["near"]), SKY_NEAR, "{label}: near");
                assert_eq!(f(fl["far"]), SKY_FAR, "{label}: far");
                cams.push((r.case, r.key, rot, f(fl["half_fov"]), fl["cull"].to_string(), Vec::new()));
            }
            "draw" => {
                let last = cams.last_mut().unwrap();
                assert_eq!((last.0, r.key.split('.').next().unwrap()), (r.case, last.1));
                last.5.push(r.out);
            }
            _ => {}
        }
    }
    let (near, far) = (f64::from(SKY_NEAR), f64::from(SKY_FAR));
    let mut states: HashMap<&str, (Option<u32>, Option<u32>, Option<bool>)> = HashMap::new();
    for (case, k, rot, half_fov, cull, events) in &cams {
        let name = format!("{label} {} camera {k}", cases[case]);
        let layer = &layers[*case];
        let dome = &layer.dome;
        assert_eq!(dome.len(), 256, "{name}");
        let views: Vec<[f64; 3]> = dome.iter().map(|&p| view(rot, p)).collect();
        let screen: Vec<[f64; 3]> = views.iter().map(|&v| project(v)).collect();
        let tris: HashSet<[usize; 3]> = SkyDome::triangles().collect();
        let mut drawn: HashSet<[usize; 3]> = HashSet::new();
        // The D3D7 device starts with no texture on any stage; the driver only calls SetTexture
        // (and SetRenderState) when its cached value changes, also across cameras of a case.
        let state = states.entry(*case).or_insert((Some(0u32), Some(0u32), None::<bool>));
        let (mut tex0, mut tex1, mut blend) = *state;
        for e in events {
            if let Some(v) = e.strip_prefix("tex:0=") {
                tex0 = Some(u32::from_str_radix(v, 16).unwrap());
                continue;
            }
            if let Some(v) = e.strip_prefix("tex:1=") {
                tex1 = Some(u32::from_str_radix(v, 16).unwrap());
                continue;
            }
            if let Some(v) = e.strip_prefix("rs:27=") {
                blend = Some(v == "1");
                continue;
            }
            let Some(d) = e.strip_prefix("dip:") else { continue };
            let fl = fields(d);
            assert_eq!(fl["type"], "4", "{name}: TRIANGLELIST");
            assert_eq!(fl["fvf"], "1c4", "{name}: XYZRHW | DIFFUSE | SPECULAR | TEX1");
            let vb = hex_bytes(fl["vb"]);
            let idx: Vec<usize> = fl["idx"].split(',').map(|v| v.parse().unwrap()).collect();
            // Layers whose colours and UVs match every identified vertex of this draw.
            let mut candidates: HashSet<usize> = (0..layer.clouds.len()).collect();
            let mut first_vertex = true;
            for t in idx.chunks_exact(3) {
                let mut ids = [None; 3];
                for (slot, &i) in t.iter().enumerate() {
                    let v = tl(&vb, i);
                    let w = 1.0 / f64::from(v.rhw);
                    if (w - far).abs() < 1e-2 {
                        assert_eq!(v.z, 1.0, "{name}: far-plane vertex z");
                        n.clipped_far += 1;
                        continue;
                    }
                    if on_edge(&v) {
                        n.clipped_edge += 1;
                        continue;
                    }
                    // A dome vertex: find it by its screen position and depth.
                    let hit = (0..256).find(|&j| {
                        let s = screen[j];
                        s[2] > near
                            && (s[0] - f64::from(v.x)).abs() < 0.02
                            && (s[1] - f64::from(v.y)).abs() < 0.02
                            && (s[2] - w).abs() < 1e-4 * s[2]
                    });
                    let j = hit.unwrap_or_else(|| panic!("{name}: vertex ({}, {}, w {w}) is no dome vertex", v.x, v.y));
                    let zexp = far / (far - near) * (1.0 - near / views[j][2]);
                    assert!((f64::from(v.z) - zexp).abs() < 2e-6, "{name}: z of vertex {j}: {} vs {zexp}", v.z);
                    let mine: HashSet<usize> = (0..layer.clouds.len())
                        .filter(|&l| layer.colour.get(&(l, j)) == Some(&v.diffuse) && layer.uv.get(&(l, j)) == Some(&v.uv))
                        .collect();
                    assert!(!mine.is_empty(), "{name}: vertex {j} diffuse {:08x} uv {:?} is no layer's", v.diffuse, v.uv);
                    // A draw call may hold several layers when the driver batches equal states.
                    if first_vertex {
                        candidates = mine;
                        first_vertex = false;
                    } else {
                        candidates.extend(mine);
                    }
                    ids[slot] = Some(j);
                    n.vertices += 1;
                }
                match ids {
                    [Some(a), Some(b), Some(c)] => {
                        let tri = [a, b, c];
                        assert!(tris.contains(&tri), "{name}: triangle {tri:?} is not (i, i+1, i+17) / (i, i+17, i+16)");
                        drawn.insert(tri);
                        n.triangles += 1;
                        // The shader's dome hit at points of this triangle = D3D's
                        // perspective-correct interpolation of the TL vertices.
                        let tv: Vec<Tl> = t.iter().map(|&i| tl(&vb, i)).collect();
                        for bary in [[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.6, 0.3, 0.1], [0.1, 0.15, 0.75]] {
                            let sx: f64 = (0..3).map(|q| bary[q] * f64::from(tv[q].x)).sum();
                            let sy: f64 = (0..3).map(|q| bary[q] * f64::from(tv[q].y)).sum();
                            let wsum: f64 = (0..3).map(|q| bary[q] * f64::from(tv[q].rhw)).sum();
                            let pc = |a: [f64; 3]| (0..3).map(|q| bary[q] * f64::from(tv[q].rhw) * a[q]).sum::<f64>() / wsum;
                            let col = pc(tri.map(|v| (v % 16) as f64));
                            let row = pc(tri.map(|v| (v / 16) as f64));
                            let depth = 1.0 / wsum;
                            // World direction of the pixel (rotation transposed).
                            let dv = [(sx - CX) / FOCAL, -(sy - CY) / FOCAL, 1.0];
                            let r = rot.map(f64::from);
                            let dw = [0, 1, 2].map(|j| r[j] * dv[0] + r[3 + j] * dv[1] + r[6 + j] * dv[2]);
                            let domeobj = dome_of(dome);
                            let h = domeobj.hit(dw.map(|v| v as f32)).unwrap_or_else(|| panic!("{name}: no hit in {tri:?}"));
                            assert!(
                                (f64::from(h.column) - col).abs() < 2e-3 && (f64::from(h.row) - row).abs() < 2e-3,
                                "{name}: hit ({}, {}) vs interpolated ({col}, {row}) in {tri:?}",
                                h.column,
                                h.row
                            );
                            assert!((f64::from(h.distance) - depth).abs() < 1e-3 * depth, "{name}: depth {} vs {depth}", h.distance);
                            n.samples += 1;
                        }
                    }
                    ids => {
                        // A clipped triangle stays inside one split triangle.
                        let known: Vec<usize> = ids.iter().flatten().copied().collect();
                        assert!(
                            tris.iter().any(|t| known.iter().all(|v| t.contains(v))),
                            "{name}: clipped triangle {known:?} crosses the split"
                        );
                    }
                }
            }
            // Texture and blending of the layer drawn.
            let ok = candidates.iter().any(|&l| {
                let cloud = &layer.clouds[l];
                let tex_ok = if cloud.starts_with("flat") {
                    // The pack's blank texture object (0x535ed0) has handle 0: THRASH_settexture(0)
                    // -> dx7z SetTexture(stage, NULL) on both stages.
                    tex0 == Some(0) && tex1 == Some(0)
                } else if cloud == "none" {
                    // A layer before a later cloud slot (`drawn_layers`) has no texture object.
                    tex0 == Some(0)
                } else {
                    tex0.is_some_and(|t| t & 0xffff_ff00 == 0x7e00_0000)
                };
                tex_ok && blend == Some(layer.blends[l])
            });
            assert!(ok, "{name}: texture {tex0:x?}/{tex1:x?} blend {blend:?} fit no layer of {candidates:?}");
            for &l in &candidates {
                if layer.clouds[l].starts_with("flat") {
                    n.flat_binds += 1;
                } else {
                    n.image_binds += 1;
                }
            }
        }
        states.insert(*case, (tex0, tex1, blend));
        // The wedge cull of every cell: `sky_cell_tested` / `sky_cull_planes` /
        // `sky_cell_visible`, bit for bit.
        let forward = [rot[6], rot[7], rot[8]];
        let planes = sky_cull_planes(forward, *half_fov);
        for cell in 0..9 {
            let ours = if !sky_cell_tested(cell, forward[1]) {
                b'n'
            } else if sky_cell_visible(planes, SkyDome::cell_corners(dome, cell)) {
                b'd'
            } else {
                b'c'
            };
            assert_eq!(ours as char, cull.as_bytes()[cell] as char, "{name}: cull of cell {cell} ({cull})");
            n.cull_cells += 1;
        }
        // Completeness and the wedge cull: per cell (5 x 5 quads of the 15 x 15 grid).
        let cell_of = |t: &[usize; 3]| {
            let (c, r) = (t[0] % 16, t[0] / 16);
            (r / 5) * 3 + c / 5
        };
        let inside = |p: [f64; 3]| p[2] > near && p[2] < far - 1e-2 && (0.5..W - 1.5).contains(&p[0]) && (0.5..H - 1.5).contains(&p[1]);
        for t in SkyDome::triangles() {
            let cell = cell_of(&t);
            let c = cull.as_bytes()[cell];
            if c == b'c' {
                // Dropped by the wedge: the original shows no layer where such a triangle is on
                // screen (counted; `sky_cell_visible` reproduces the decision below).
                n.culled += 1;
                let r = rot.map(f64::from);
                'px: for py in (0..480).step_by(16) {
                    for px in (0..640).step_by(16) {
                        let dv = [(f64::from(px) - CX) / FOCAL, -(f64::from(py) - CY) / FOCAL, 1.0];
                        let dw = [0, 1, 2].map(|j| (r[j] * dv[0] + r[3 + j] * dv[1] + r[6 + j] * dv[2]) as f32);
                        if let Some((tt, _, _)) = ray_tri(dw, dome[t[0]], dome[t[1]], dome[t[2]]) {
                            if (near..far).contains(&f64::from(tt)) {
                                n.culled_on_screen += 1;
                                break 'px;
                            }
                        }
                    }
                }
            } else if t.iter().all(|&v| inside(screen[v])) {
                assert!(drawn.contains(&t), "{name}: on-screen triangle {t:?} of drawn cell {cell} missing");
                n.complete += 1;
            }
        }
    }
    n
}

fn dome_of(dome: &[[f32; 3]]) -> DomeVerts<'_> {
    DomeVerts(dome)
}

/// The recorded vertices as a dome for `SkyDome::hit`: the parameters are recovered from
/// them (scale and height from the apex row, size from the spacing) and the engine's own
/// `SkyDome::vertices` must reproduce them, so the hit runs on the engine's dome.
struct DomeVerts<'a>(&'a [[f32; 3]]);

impl DomeVerts<'_> {
    fn hit(&self, d: [f32; 3]) -> Option<nfs_formats::SkyDomeHit> {
        let dome = recover(self.0);
        dome.hit(d)
    }
}

fn recover(v: &[[f32; 3]]) -> SkyDome {
    for dome in CANDIDATES.with(|c| c.borrow().clone()) {
        if dome.vertices().iter().zip(v).all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits)) {
            return dome;
        }
    }
    panic!("no known dome reproduces the recorded vertices");
}

thread_local! {
    static CANDIDATES: std::cell::RefCell<Vec<SkyDome>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn ray_tri(d: [f32; 3], p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> Option<(f32, f32, f32)> {
    let v = |p: [f32; 3]| p.map(f64::from);
    let (d, p0, p1, p2) = (v(d), v(p0), v(p1), v(p2));
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (e1, e2) = (sub(p1, p0), sub(p2, p0));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = sub([0.0; 3], p0);
    let b1 = dot(s, p) / det;
    let q = cross(s, e1);
    let b2 = dot(d, q) / det;
    let t = dot(e2, q) / det;
    (b1 > 1e-6 && b2 > 1e-6 && b1 + b2 < 1.0 - 1e-6 && t > 0.0).then_some((t as f32, b1 as f32, b2 as f32))
}

fn dome_params(case_out: &str) -> Option<SkyDome> {
    let d = fields(case_out.split_once(';').map(|x| x.1).unwrap_or(""))
        .get("dome")?
        .split(',')
        .map(f)
        .collect::<Vec<_>>();
    Some(SkyDome { angle: d[0], size: d[1], curvature: d[2], height: d[3], scale: d[4] })
}

#[test]
fn synthetic_sky_draw_matches_the_engine() {
    let draw = rows(DRAW);
    let layer = rows(LAYER);
    let domes: Vec<SkyDome> = draw.iter().filter(|r| r.kind == "case").filter_map(|r| dome_params(r.out)).collect();
    CANDIDATES.with(|c| *c.borrow_mut() = domes);
    let n = check(&draw, &layer, "synthetic");
    eprintln!("{n:?}");
    assert!(n.vertices > 9_000 && n.triangles > 3_000 && n.complete > 1_000, "{}/{}/{}", n.vertices, n.triangles, n.complete);
    assert!(n.clipped_far > 100, "the Alps-like dome reaches past the far plane: {}", n.clipped_far);
    assert!(n.clipped_edge > 100 && n.culled > 1_000 && n.flat_binds > 10 && n.image_binds > 10);
    assert!(n.samples > 9_000);
}

#[test]
fn track_sky_draw_matches_the_engine() {
    skip_without_local_files!("sky-draw-tracks.tsv");
    let text = std::fs::read_to_string(local_fixtures::dir().join("sky-draw-tracks.tsv")).unwrap();
    let draw = rows(&text);
    // The track domes come from their Sky/<track>.bin (game data next to the fixture).
    let game = std::env::var_os("NFS_GAME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game"));
    let sky = game.join("GameData/Track/Sky");
    let mut domes = Vec::new();
    if let Ok(dir) = std::fs::read_dir(&sky) {
        for e in dir.flatten() {
            if e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("bin")) {
                if let Ok(d) = nfs_formats::parse_sky_dome(&std::fs::read(e.path()).unwrap()) {
                    domes.push(d);
                }
            }
        }
    }
    if domes.is_empty() {
        eprintln!("SKIPPED: no Sky/*.bin in {}", sky.display());
        return;
    }
    CANDIDATES.with(|c| *c.borrow_mut() = domes);
    let n = check(&draw, &draw, "tracks");
    eprintln!("{n:?}");
    assert!(n.vertices > 20_000 && n.triangles > 8_000 && n.complete > 3_000);
    assert!(n.clipped_far > 100 && n.samples > 20_000);
}

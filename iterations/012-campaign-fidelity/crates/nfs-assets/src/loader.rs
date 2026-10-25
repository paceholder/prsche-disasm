use super::*;
use crate::camera::DriverCamera;
use nfs_formats::{bytes, f32le, u16le, u32le, Entry, Ini};

fn find<'a>(files: &'a AssetFiles, name: &str) -> Result<&'a [u8], String> {
    let name = name.to_ascii_lowercase();
    let suffix = format!("/{name}");
    let mut matches = files.iter().filter(|(k, _)| {
        let k = k.replace('\\', "/").to_ascii_lowercase();
        k == name || k.ends_with(&suffix)
    });
    let (_, v) = matches
        .next()
        .ok_or_else(|| format!("missing resource {name}"))?;
    if matches.next().is_some() {
        return Err(format!("ambiguous resource {name}"));
    }
    Ok(v)
}
fn val<'a>(ini: &'a Ini, s: &str, k: &str) -> Option<&'a str> {
    ini.get(s).and_then(|s| s.get(k)).map(String::as_str)
}
fn num(ini: &Ini, s: &str, k: &str, default: u32) -> Result<u32, String> {
    match val(ini, s, k) {
        Some(v) => v
            .parse()
            .map_err(|_| format!("TPG [{s}] {k}: invalid integer {v}")),
        None => Ok(default),
    }
}
fn c_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data.split(|byte| *byte == 0).next().unwrap_or(&[])).into_owned()
}
fn text(e: &Entry) -> String {
    c_string(&e.data).trim().to_string()
}

#[derive(Default)]
struct StyleSelection {
    geometry: BTreeMap<u32, u32>,
    texture: BTreeMap<u32, u32>,
    name: String,
}

/// `R3DCar_ApplyStyle` 0x437b00: style index `N` (car `+0x568`, the catalogue record's i16 at
/// `+0x1c0`) reads the TPG section `[style{N+1}]`; a missing section leaves every type 0.
fn default_style(ini: &Ini, index: u32) -> Result<StyleSelection, String> {
    let section = format!("style{}", index + 1);
    let section = section.as_str();
    // A declared style is a coherent set of overrides on type zero. Special
    // cars such as f901 do not contain a complete all-zero geometry variant.
    let mut style = StyleSelection {
        name: "implicit type 0".into(),
        ..Default::default()
    };
    if !ini.contains_key(section) {
        return Ok(style);
    }
    style.name = format!(
        "{section} ({})",
        val(ini, section, "name").unwrap_or("unnamed")
    );
    let count = num(ini, section, "count", 0)?;
    if count > 512 {
        return Err("too many style overrides".into());
    }
    for index in 1..=count {
        let variant = num(ini, section, &format!("type{index}"), 0)?;
        for (domain, map) in [
            ("geometry", &mut style.geometry),
            ("texture", &mut style.texture),
        ] {
            let key = format!("{domain}{index}");
            if val(ini, section, &key).is_some() {
                map.insert(num(ini, section, &key, 0)?, variant);
            }
        }
    }
    Ok(style)
}

fn tile_selected(ini: &Ini, style: &StyleSelection, filter: &str) -> Result<bool, String> {
    let count = num(ini, filter, "count", 0)?;
    if count > 1024 {
        return Err(format!("[{filter}]: unreasonable selection count"));
    }
    if count == 0 {
        return Ok(true);
    }
    // Evidence: 928.tpg/944.tpg [style1] select texture 13 type 1 and their
    // [file3.whf1] tiles repeat texture1=13,type1=1. Hypothesis: tile TypeIndex
    // uses the same domain-specific TPG selection as CRP Base geometry.
    for index in 1..=count {
        let type_key = format!("type{index}");
        if val(ini, filter, &type_key).is_none() {
            return Err(format!("[{filter}]: selection {index} has no {type_key}"));
        }
        let variant = num(ini, filter, &type_key, u32::MAX)?;
        let geometry_key = format!("geometry{index}");
        let texture_key = format!("texture{index}");
        let geometry = val(ini, filter, &geometry_key)
            .map(|_| num(ini, filter, &geometry_key, 0))
            .transpose()?;
        let texture = val(ini, filter, &texture_key)
            .map(|_| num(ini, filter, &texture_key, 0))
            .transpose()?;
        if geometry.is_none() && texture.is_none() {
            return Err(format!(
                "[{filter}]: selection {index} has no geometry{index} or texture{index}"
            ));
        }
        if geometry.is_some_and(|key| variant == style.geometry.get(&key).copied().unwrap_or(0))
            || texture.is_some_and(|key| variant == style.texture.get(&key).copied().unwrap_or(0))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Choose the parked-car representation from CRP metadata, not article names.
///
/// A level names both the geometry arrays and the high nibble of `pr`.
/// Select one level for the entire scene: mixing FE1 with racing-only tyre
/// tread (levels 4/5) creates overlapping surfaces with unrelated UVs.
/// `Base+0x44` geometry index of the cabrio top at rest (`CabrioUp*`, `CabrioDown*`).
const ROOF_REST_GEOMETRY: u8 = 41;

fn parked_levels(article: &Entry, style: &StyleSelection) -> Result<Vec<u16>, String> {
    if article.children.iter().any(|entry| entry.tag == "ef") {
        return Ok(vec![]);
    }
    let base = article.find("Base", 0).ok_or("article has no Base")?;
    let info = bytes(&base.data, 68, 16)?;
    // TypeIndex is selected independently for each GeomIndex by the TPG style.
    // The high nibble of LevelIndex is a BIL_* bitset; BIL_DRIVER is 0x8.
    let wanted = style
        .geometry
        .get(&u32::from(info[0]))
        .copied()
        .unwrap_or(0);
    // Geometry 41 holds the top at rest in pairs: type 2k-1 up, 2k down; the style lists one of
    // them and both are loaded (`CabrioUp*` / `CabrioDown*`), the front end shows one by the
    // state of the top.
    let type_ok = if info[0] == ROOF_REST_GEOMETRY {
        u32::from(info[1]).div_ceil(2) == wanted.div_ceil(2) && wanted != 0
    } else {
        u32::from(info[1]) == wanted
    };
    if !type_ok || info[10] & 0x80 != 0 {
        return Ok(vec![]);
    }
    let level_count = u32le(&base.data, 12)? as usize;
    if level_count == 0 {
        return Err("Base has no level masks".into());
    }
    bytes(
        &base.data,
        100,
        level_count
            .checked_mul(12)
            .ok_or("Base level count overflow")?,
    )?;
    let mut levels = Vec::with_capacity(level_count);
    for index in 0..level_count {
        let lod = u16le(&base.data, 108 + index * 12)?;
        if lod > 0x0f {
            return Err(format!("Base level {lod} cannot fit CRP entry index"));
        }
        levels.push(lod);
    }
    Ok(levels)
}

/// Glare and shadow textures of `Render/common.fsh` (`R3DCar_DrawGlare` 0x43cdd0 and
/// `R3DCar_DrawShadow` 0x43b820 look them up by FSH name). Absent file: no effects.
fn effect_textures(files: &AssetFiles) -> Result<Vec<Texture>, String> {
    let mut out = Vec::new();
    // particle.fsh holds the spark texture SPRK (carpart.dat `CSPK`, matid KRPS);
    // carcmn.fsh the specular maps SPC1..3 (`Car_Load` 0x441a10).
    for name in ["common.fsh", "particle.fsh", "carcmn.fsh"] {
        if let Ok(data) = find(files, name) {
            out.extend(fsh_textures(name, data)?);
        }
    }
    Ok(out)
}

fn fsh_textures(name: &str, data: &[u8]) -> Result<Vec<Texture>, String> {
    let images = nfs_formats::parse_fsh(data).map_err(|e| format!("{name}: {e}"))?;
    // parse_fsh lowercases names, but common.fsh has both `spik` and `SPIK`;
    // the original looks textures up by exact FourCC, so keep the raw names.
    let raw = if data.starts_with(&[0x10, 0xfb]) {
        nfs_formats::decompress(data).map_err(|e| format!("{name}: {e}"))?
    } else {
        data.to_vec()
    };
    Ok(images
        .into_iter()
        .enumerate()
        .map(|(i, image)| Texture {
            name: raw
                .get(16 + i * 8..20 + i * 8)
                .map(|n| String::from_utf8_lossy(n).trim_end().to_string())
                .unwrap_or(image.name),
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        })
        .collect())
}

/// `ef` record type of a glare (`*piVar8 == 5` in `R3DCar_DrawGlares` 0x43d800).
const EF_GLARE: u32 = 5;
/// `ef+0x50` bit: also draw a copy mirrored in x (0x43d800, `& 8`).
const EF_MIRRORED: u32 = 8;

/// Glare articles selected by the TPG style, with mirrored copies expanded.
///
/// Glare `Base` records (112 bytes) carry the same part info at 0x44 as geometry
/// articles: GeomIndex, TypeIndex, pivot at +7, light group at +0xc (0x43d8c4).
/// Each glare appears once per detail level (Base level masks, as geometry);
/// the scene level is used, like the parked-car geometry.
fn car_glares(
    crp: &nfs_formats::Crp,
    style: &StyleSelection,
    scene_lod: u16,
) -> Result<Vec<CarGlare>, String> {
    let vec3 = |d: &[u8], o: usize| -> Result<[f32; 3], String> {
        Ok([f32le(d, o)?, f32le(d, o + 4)?, f32le(d, o + 8)?])
    };
    // Raw CRP model space -> driving body space ([-x, y, z], as CarWheel).
    let body = |v: [f32; 3]| [-v[0], v[1], v[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let unit = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 { v.map(|x| x / l) } else { v }
    };
    let mut out: Vec<CarGlare> = Vec::new();
    for a in &crp.articles {
        let Some(ef) = a.children.iter().find(|e| e.tag == "ef") else {
            continue;
        };
        if ef.data.len() < 0x58 || u32le(&ef.data, 0)? != EF_GLARE {
            continue;
        }
        let Some(base) = a.find("Base", 0) else {
            continue;
        };
        let info = bytes(&base.data, 0x44, 16)?;
        if u32::from(info[1]) != style.geometry.get(&u32::from(info[0])).copied().unwrap_or(0) {
            continue;
        }
        let level_count = (u32le(&base.data, 12)? as usize).min(15);
        let levels = (0..level_count)
            .map(|i| u16le(&base.data, 108 + i * 12))
            .collect::<Result<Vec<_>, _>>()?;
        if !levels.contains(&scene_lod) && !levels.contains(&0) {
            continue;
        }
        let name = a.find("Name", 0).map(text).unwrap_or_default();
        let mut position = vec3(&ef.data, 0x08)?;
        if let Some(tr) = a.children.iter().find(|e| e.tag == "tr" && e.data.len() >= 64) {
            let t = vec3(&tr.data, 48)?;
            for k in 0..3 {
                position[k] += t[k];
            }
        }
        let (axis_a, axis_b, axis_c) = (vec3(&ef.data, 0x18)?, vec3(&ef.data, 0x28)?, vec3(&ef.data, 0x38)?);
        let colors = [u32le(&ef.data, 0x48)?, u32le(&ef.data, 0x4c)?];
        let flags = u32le(&ef.data, 0x50)?;
        let (kind, mirror_group, beam) = (ef.data[0x54], ef.data[0x56], ef.data[0x57] != 0);
        let glare = |name: String, p: [f32; 3], a: [f32; 3], b: [f32; 3], group: u8| CarGlare {
            name,
            position: body(p),
            axis_a: body(a),
            axis_b: body(b),
            normal: body(unit(cross(b, a))),
            colors,
            kind,
            group,
            beam,
            pivot: info[7],
        };
        let _ = axis_c; // ef+0x38 (mirrored with x negated) has no use in the sprite.
        let mut found = vec![glare(name.clone(), position, axis_a, axis_b, info[0xc])];
        if flags & EF_MIRRORED != 0 {
            // 0x43dae8..: pos.x, A (all), B.x, C.x negated.
            found.push(glare(
                format!("{name} (mirrored)"),
                [-position[0], position[1], position[2]],
                axis_a.map(|x| -x),
                [-axis_b[0], axis_b[1], axis_b[2]],
                mirror_group,
            ));
        }
        out.extend(found);
    }
    Ok(out)
}

/// Column-major 4×4 `tr` matrix of an article at `level` (raw CRP units), identity if absent.
fn part_frame(article: &Entry, level: u16) -> Result<[f32; 16], String> {
    match article.find("tr", level).filter(|t| t.data.len() >= 64) {
        Some(t) => {
            let mut m = [0.0; 16];
            for (i, v) in m.iter_mut().enumerate() {
                *v = f32le(&t.data, i * 4)?;
            }
            Ok(m)
        }
        None => Ok([
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ]),
    }
}

fn frame_point(m: &[f32; 16], p: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r] * w)
}

/// Parts of the in-car view (`docs/re/cockpit.md`). `R3DObject_DrawPasses` 0x444e00 draws, for
/// the viewed car in BumperCam2, mask 0x10 at level 2, mask 0x40 at level 1 and mask 0x80 at
/// the car-detail LOD; `R3DCarLOD_DrawParts` 0x43c830 keeps parts with `Base+0x44` byte 0xd
/// set and draws them in their full `tr` frame (rotation and scale). Part style as the parked
/// car; glare (`ef`) articles are drawn by the glare pass instead.
/// Driver index (`res+0x150`, config key `.DRIVER` 0x23d). The front end picks `rand() & 7`
/// for a quick race (career: `[0x5db748]`) [VA 0x4eeda0]; the engine uses driver 0
/// (approximation A63).
const DRIVER_INDEX: u32 = 0;
/// Driver body type by driver index (`0x5cd5c4`).
const DRIVER_BODY_TYPE: [u32; 8] = [2, 3, 4, 5, 10, 8, 9, 7];

/// Driver geometry types set after the TPG style (`R3DCar_SelectStyle` 0x43ae50, outside the
/// showroom and not for cop cars): body (GeomIndex 9) from `0x5cd5c4`, arms (43)
/// `(d/4)·2 + 2`, hands (56) `d/4 + 1`; drivers ≥ 8 use type 1. The Factory Driver mode
/// (`[0x6573e8] == 9`) and the `.GEOMCONFIG` overrides (`res+0x32c`) are not applied.
fn with_driver(style: &StyleSelection, driver: u32) -> StyleSelection {
    let mut out = StyleSelection {
        geometry: style.geometry.clone(),
        texture: style.texture.clone(),
        name: style.name.clone(),
    };
    let (body, arms, hands) = match DRIVER_BODY_TYPE.get(driver as usize) {
        Some(&body) => (body, driver / 4 * 2 + 2, driver / 4 + 1),
        None => (1, 1, 1),
    };
    out.geometry.insert(9, body);
    out.geometry.insert(43, arms);
    out.geometry.insert(56, hands);
    out
}

fn in_car_parts(
    crp: &nfs_formats::Crp,
    style: &StyleSelection,
    matmap: &BTreeMap<u16, usize>,
) -> Result<Vec<InCarPart>, String> {
    let style = &with_driver(style, DRIVER_INDEX);
    let mut parts = Vec::new();
    for (ai, a) in crp.articles.iter().enumerate() {
        if a.children.iter().any(|e| e.tag == "ef") {
            continue;
        }
        let Some(base) = a.find("Base", 0) else {
            continue;
        };
        let info = bytes(&base.data, 0x44, 16)?;
        let selected = u32::from(info[1])
            == style
                .geometry
                .get(&u32::from(info[0]))
                .copied()
                .unwrap_or(0);
        if !selected || info[0xd] == 0 {
            continue;
        }
        let level_count = (u32le(&base.data, 12)? as usize).min(15);
        let levels = (0..level_count)
            .map(|i| u16le(&base.data, 108 + i * 12))
            .collect::<Result<Vec<_>, _>>()?;
        let name = a
            .find("Name", 0)
            .map(text)
            .unwrap_or_else(|| format!("article{ai}"));
        for pass in InCarPass::ALL {
            if info[10] & pass.mask() == 0 {
                continue;
            }
            let level = pass.level();
            let lod = if levels.contains(&level) {
                level
            } else if levels.contains(&0) {
                0
            } else {
                continue;
            };
            let Some(vt) = a.find("vt", lod) else {
                continue;
            };
            let m = part_frame(a, lod)?;
            let to_scene_point = |p: [f32; 3]| to_scene_coordinates(frame_point(&m, p, 1.0));
            let to_scene_normal =
                |n: [f32; 3]| to_scene_coordinates(normalize(frame_point(&m, n, 0.0)));
            let frame_count = if info[8] > 1 { info[8] } else { 0 };
            let mut meshes = Vec::new();
            for pr in a
                .children
                .iter()
                .filter(|e| e.tag == "pr" && e.index >> 12 == lod)
            {
                let label = format!("{name}#{ai}/{} (in-car)", pr.index & 4095);
                let local = |frame: u16| -> Result<Mesh, String> {
                    let index = lod | (frame << 4);
                    let vt = a.find("vt", index).unwrap_or(vt);
                    let nm = a.find("nm", index).or_else(|| a.find("nm", lod));
                    mesh(
                        pr,
                        vt,
                        nm,
                        a.find("uv", lod),
                        [0.0; 3],
                        matmap,
                        label.clone(),
                    )
                    .map_err(|e| format!("{name}, CRP offset 0x{:x}: {e}", pr.offset))
                };
                // mesh() returns raw/5: the frame's translation is divided by five too.
                let place = |mesh: &Mesh| -> Vec<([f32; 3], [f32; 3])> {
                    mesh.vertices
                        .iter()
                        .map(|v| {
                            let mut m5 = m;
                            for t in &mut m5[12..15] {
                                *t /= 5.0;
                            }
                            (
                                to_scene_coordinates(frame_point(&m5, v.position, 1.0)),
                                to_scene_normal(v.normal),
                            )
                        })
                        .collect()
                };
                let mut base_mesh = local(0)?;
                if base_mesh.indices.is_empty() {
                    continue;
                }
                let placed = place(&base_mesh);
                for (v, (p, n)) in base_mesh.vertices.iter_mut().zip(&placed) {
                    v.position = *p;
                    v.normal = *n;
                }
                for tri in base_mesh.vertices.as_chunks_mut::<3>().0 {
                    if tri.iter().all(|vertex| vertex.normal == [0.; 3]) {
                        let normal =
                            triangle_normal([tri[0].position, tri[1].position, tri[2].position]);
                        for vertex in tri {
                            vertex.normal = normal;
                        }
                    }
                }
                let mut frames = Vec::new();
                if frame_count > 0 {
                    for f in 0..u16::from(frame_count) {
                        let index = lod | (f << 4);
                        if a.find("vt", index).is_none() {
                            break;
                        }
                        let mut fm = place(&local(f)?);
                        for tri in fm.chunks_mut(3) {
                            if tri.len() == 3 && tri.iter().all(|(_, n)| *n == [0.; 3]) {
                                let n = triangle_normal([tri[0].0, tri[1].0, tri[2].0]);
                                tri.iter_mut().for_each(|v| v.1 = n);
                            }
                        }
                        frames.push(fm);
                    }
                }
                meshes.push(InCarMesh {
                    mesh: base_mesh,
                    frames,
                });
            }
            if meshes.is_empty() {
                continue;
            }
            parts.push(InCarPart {
                name: name.clone(),
                pass,
                channel: info[7],
                pivot_origin: to_scene_point([0.0, 0.0, 0.0]).map(|x| x / 5.0),
                pivot_axis: to_scene_normal([0.0, 0.0, 1.0]),
                frame_channel: info[9],
                frame_count: frames_or_zero(frame_count, &meshes),
                meshes,
            });
        }
    }
    Ok(parts)
}

/// Frame count of the part: the frames actually present in every mesh.
fn frames_or_zero(declared: u8, meshes: &[InCarMesh]) -> u8 {
    let present = meshes.iter().map(|m| m.frames.len()).min().unwrap_or(0);
    declared.min(present as u8)
}

/// Wheel articles of the base geometry: domain 13 front / 14 rear with article class 10. The
/// article id differs per body variant (77/78 stock, 106/107 `WheelFront1`/`WheelRear1`, 132
/// `WheelRear2`: wide-body styles such as the 993 turbo, 901 RSR); the shadow articles of the same
/// domains are class 11. Only the style-selected article reaches here, so pivot and meshes are unique.
fn is_wheel_article(geometry: u8, class: u8, _id: u8) -> bool {
    matches!(geometry, 13 | 14) && class == 10
}

fn to_scene_coordinates(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[1], -v[2]]
}

fn alpha_mode(render: &str, tpg_bpp: u32, rgba: &[u8]) -> (AlphaMode, f32) {
    // Exterior intermediate alpha encodes paint, while zero cuts out details.
    if matches!(render, "CarExt" | "CarExtEnv") {
        return (AlphaMode::Mask, 0.0);
    }
    // Interior glass (`intglass.fsh`, a 32-bit page) is translucent: seen from the in-car
    // view it must not hide the road. Other interior pages stay opaque.
    if render == "CarInt" && tpg_bpp != 32 {
        return (AlphaMode::Opaque, 0.0);
    }
    let mut alpha = rgba.iter().skip(3).step_by(4);
    if alpha.clone().all(|&value| value == 255) {
        return (AlphaMode::Opaque, 0.0);
    }
    // TPG 1555 is binary coverage; use alpha=0 as the cutout sentinel.
    if tpg_bpp == 1555 {
        return (AlphaMode::Mask, 0.0);
    }
    // 4444 encodes fractional coverage (for example shadow.fsh).
    if tpg_bpp == 4444 {
        return (AlphaMode::Blend, 0.0);
    }
    if alpha.all(|&value| matches!(value, 0 | 255)) {
        (AlphaMode::Mask, 0.0)
    } else {
        (AlphaMode::Blend, 0.0)
    }
}

fn texture_source<'a>(
    files: &'a AssetFiles,
    crp: &'a nfs_formats::Crp,
    ini: &Ini,
    file: u32,
) -> Result<&'a [u8], String> {
    if let Some(e) = crp
        .misc
        .iter()
        .find(|e| e.tag == "sf" && e.index as u32 == file - 1)
    {
        return Ok(e.data.as_slice());
    }
    let name = val(ini, "header", &format!("file{file}"))
        .ok_or_else(|| format!("missing [header] file{file} mapping"))?;
    find(files, name)
}

/// Return external FSH filenames the selected parked-car scene requires.
///
/// The CRP embeds most packs.  Its `sf` index is zero-based while TPG files
/// are one-based; anything without a matching embedded `sf` is looked up by
/// the corresponding `[header] fileN` value.  Driver suit and head packs are
/// intentionally outside this scene.
fn required_external_fsh(
    crp: &nfs_formats::Crp,
    ini: &Ini,
    file_count: u32,
) -> Result<Vec<String>, String> {
    let mut required = Vec::new();
    for file in 1..=file_count {
        let section = format!("file{file}.details");
        if matches!(num(ini, &section, "global", 0)?, 3 | 4) {
            continue;
        }
        if crp
            .misc
            .iter()
            .any(|e| e.tag == "sf" && e.index as u32 == file - 1)
        {
            continue;
        }
        let name = val(ini, "header", &format!("file{file}"))
            .ok_or_else(|| format!("missing [header] file{file} mapping"))?;
        required.push(name.to_ascii_lowercase());
    }
    Ok(required)
}

/// NFS5 FSH packs use both `0xffff` and the 12-bit `0x0fff` spelling for a
/// one-pixel negative origin. Treat these observed border cases as a signed
/// origin and clip to the fixed TPG page (see docs/crp-format.md for evidence).
fn atlas_coordinate(raw: u32) -> i64 {
    match raw {
        0xffff | 0x0fff => -1,
        value => i64::from(value),
    }
}

/// Rasterise one FSH image into its TPG page, retaining only the intersection
/// with the page.  A tile wholly outside the page remains invalid input.
fn place_tile(
    atlas: &mut Texture,
    image: &nfs_formats::Image,
    offset: [u32; 2],
    rotate: bool,
) -> Result<(), String> {
    let expected_bytes = image
        .width
        .checked_mul(image.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("FSH tile byte count overflow")? as usize;
    if image.rgba.len() != expected_bytes {
        return Err("FSH tile pixels do not match its dimensions".into());
    }
    let x = atlas_coordinate(image.x) + i64::from(offset[0]);
    let y = atlas_coordinate(image.y) + i64::from(offset[1]);
    let (placed_width, placed_height) = if rotate {
        (image.height, image.width)
    } else {
        (image.width, image.height)
    };
    let right = x + i64::from(placed_width);
    let bottom = y + i64::from(placed_height);
    if right <= 0 || bottom <= 0 || x >= i64::from(atlas.width) || y >= i64::from(atlas.height) {
        return Err(format!(
            "tile does not intersect atlas at {x},{y}, size {placed_width},{placed_height}"
        ));
    }
    for sy in 0..image.height {
        for sx in 0..image.width {
            let (dx, dy) = if rotate {
                (image.height - 1 - sy, sx)
            } else {
                (sx, sy)
            };
            let dst_x = x + i64::from(dx);
            let dst_y = y + i64::from(dy);
            if dst_x < 0
                || dst_y < 0
                || dst_x >= i64::from(atlas.width)
                || dst_y >= i64::from(atlas.height)
            {
                continue;
            }
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((dst_y as u32 * atlas.width + dst_x as u32) * 4) as usize;
            atlas.rgba[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    Ok(())
}

/// `sscanf("%f,%f,%f")` into `default` (`0x523b50`): components are replaced until the
/// first one that does not parse.
fn tpg_vec3(ini: &Ini, key: &str, default: [f32; 3]) -> [f32; 3] {
    let mut out = default;
    if let Some(v) = val(ini, "header", key) {
        for (slot, part) in out.iter_mut().zip(v.split(',')) {
            match part.trim().parse::<f32>() {
                Ok(x) => *slot = x,
                Err(_) => break,
            }
        }
    }
    out
}

/// Float key of the TPG `[header]` (`FUN_00523b20` with a default) [VA 0x441a10].
fn tpg_f32(ini: &Ini, key: &str, default: f32) -> f32 {
    val(ini, "header", key)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(default)
}

/// `Car_ReadTpgCameraKeys 0x441a10`: `drivercampos`/`drivercamrot` (defaults
/// (-0.36, 0.4, -0.25) / 0), then `drivercamposstyleN`/`drivercamrotstyleN` override them.
fn driver_camera(ini: &Ini, style: u32) -> DriverCamera {
    let base = DriverCamera::DEFAULT;
    let pos = tpg_vec3(ini, "drivercampos", base.position);
    let rot = tpg_vec3(ini, "drivercamrot", base.rotation_deg);
    DriverCamera {
        position: tpg_vec3(ini, &format!("drivercamposstyle{style}"), pos),
        rotation_deg: tpg_vec3(ini, &format!("drivercamrotstyle{style}"), rot),
    }
}

/// `style` is the TPG style index N (car `+0x568`); it selects `[style{N+1}]` and the
/// `drivercamposstyleN` keys.
pub fn load(files: &AssetFiles, car: &str, style_index: u32) -> Result<Scene, String> {
    if car.is_empty() || !car.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("car identifier must be alphanumeric".into());
    }
    let crp = nfs_formats::parse_crp(find(files, &format!("{car}.crp"))?)
        .map_err(|e| format!("{car}.crp: {e}"))?;
    let ini = nfs_formats::parse_ini(find(files, &format!("{car}.tpg"))?)?;
    let style = default_style(&ini, style_index)?;
    let article_levels = crp
        .articles
        .iter()
        .map(|a| parked_levels(a, &style))
        .collect::<Result<Vec<_>, _>>()?;
    let scene_lod = article_levels
        .iter()
        .flatten()
        .copied()
        .filter(|&level| level != 0)
        .min()
        .unwrap_or(0);
    let mut scene = Scene {
        meshes: Vec::new(),
        car_collision_box: crate::collision_box::car_collision_box(&crp).ok(),
        car_driver_camera: Some(driver_camera(&ini, style_index)),
        car_wheels: Vec::new(),
        car_glares: car_glares(&crp, &style, scene_lod)?,
        car_in_car: Vec::new(),
        car_anim_parts: Vec::new(),
        car_roof_up: Vec::new(),
        car_roof_down: Vec::new(),
        effect_textures: effect_textures(files)?,
        particle_defs: match find(files, "carpart.dat") {
            Ok(data) => crate::sparks::parse_particle_dat(data)?,
            Err(_) => Vec::new(),
        },
        textures: Vec::new(),
        materials: Vec::new(),
        bounds: [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
        diagnostics: vec![format!(
            "{} CRP articles, {} misc entries, {} decoded bytes; {}, scene level {scene_lod}, undamaged, no driver",
            crp.articles.len(),
            crp.misc.len(),
            crp.decoded_size,
            style.name
        )],
        prop_articles: Vec::new(),
        prop_instances: Vec::new(),
        track_objects: Vec::new(),
        smashables: Vec::new(),
        smack_props: Vec::new(),
        scenario_start: None,
        scenario_end: None,
        scenario_name: None,
        scenario_gates: Vec::new(),
        sky_texture: None,
        sky_layers: [None, None, None],
        sky_horizon: None,
        sun_position: None,
        topology: None,
        road_surface: None,
        course: None,
        ai_track: None,
    };
    let mut skipped = Vec::new();
    let mut selected = Vec::new();
    let pages = num(&ini, "header", "numtpages", 0)?;
    if pages == 0 || pages > 256 {
        return Err("invalid TPG page count".into());
    }
    let mut atlas_bytes = 0usize;
    for p in 1..=pages {
        let section = format!("tpage{p}.details");
        let width = num(&ini, &section, "width", 0)?;
        let height = num(&ini, &section, "height", 0)?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(format!("[{section}]: invalid dimensions"));
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| format!("[{section}]: texture byte count overflow"))?;
        atlas_bytes = atlas_bytes
            .checked_add(bytes)
            .ok_or("TPG texture allocation overflow")?;
        if atlas_bytes > 128 * 1024 * 1024 {
            return Err("TPG texture atlases exceed 128 MiB limit".into());
        }
        scene.textures.push(Texture {
            name: format!("page{p}"),
            width,
            height,
            rgba: vec![0; bytes],
        });
    }
    let filecount = num(&ini, "header", "numfiles", 0)?;
    if filecount > 256 {
        return Err("invalid TPG file count".into());
    }
    let mut missing = Vec::new();
    for name in required_external_fsh(&crp, &ini, filecount)? {
        match find(files, &name) {
            Ok(_) => {}
            Err(error) if error.starts_with("missing resource ") => missing.push(name),
            Err(error) => return Err(error),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "missing required shared FSH resource(s): {}",
            missing.join(", ")
        ));
    }
    // Tile filters see the driver types set after the style (`R3DCar_SelectStyle` 0x43ae50), so
    // styles that list no GeomIndex 9 still select a suit/helmet tile.
    let tile_style = with_driver(&style, DRIVER_INDEX);
    for f in 1..=filecount {
        let section = format!("file{f}.details");
        let page = num(&ini, &section, "tpage", 0)?;
        if page == 0 || page > pages {
            return Err(format!("[{section}]: invalid page {page}"));
        }
        // Driver packs (suit, head) aren't part of the parked-car view and aren't required;
        // the in-car hands use the suit pack, so it is placed when present.
        let source = if matches!(num(&ini, &section, "global", 0)?, 3 | 4) {
            match texture_source(files, &crp, &ini, f) {
                Ok(source) => source,
                Err(_) => continue,
            }
        } else {
            texture_source(files, &crp, &ini, f)?
        };
        let images = nfs_formats::parse_fsh(source).map_err(|e| format!("TPG file{f}: {e}"))?;
        let mut selected = 0;
        for image in images {
            let filter = format!("file{f}.{}", image.name);
            if num(&ini, &filter, "frontend", 1)? == 0 || num(&ini, &filter, "racedecal", 0)? != 0 {
                continue;
            }
            if !tile_selected(&ini, &tile_style, &filter)? {
                continue;
            }
            let rotate = num(&ini, &filter, "rotate", 0)? != 0;
            let atlas = &mut scene.textures[(page - 1) as usize];
            place_tile(
                atlas,
                &image,
                [
                    num(&ini, &section, "offsetx", 0)?,
                    num(&ini, &section, "offsety", 0)?,
                ],
                rotate,
            )
            .map_err(|error| format!("{filter}: {error}"))?;
            selected += 1;
        }
        scene.diagnostics.push(format!(
            "file{f} -> page{page}: {selected} selected texture tiles"
        ));
    }
    for p in 1..=pages {
        let source = num(&ini, &format!("tpage{p}.details"), "sourcetpage", 0)?;
        if source > 0 {
            if source > pages || source == p {
                return Err("invalid source texture page".into());
            }
            // TPG mirror page uses the same atlas placement; UVs encode the opposite side.
            let image = scene.textures[(source - 1) as usize].clone();
            let target = &mut scene.textures[(p - 1) as usize];
            if target.width != image.width || target.height != image.height {
                return Err("source texture page dimensions differ".into());
            }
            target.rgba = image.rgba;
        }
    }
    let mut matmap = BTreeMap::new();
    let mut material_entries: Vec<_> = crp.misc.iter().filter(|e| e.tag == "mt").collect();
    material_entries.sort_by_key(|entry| entry.index);
    for e in material_entries {
        let page = u32le(&e.data, 40)? as usize;
        if page >= scene.textures.len() {
            return Err(format!("material {}: invalid texture page {page}", e.index));
        }
        let render = c_string(bytes(&e.data, 16, 16)?);
        let paintable = matches!(render.as_str(), "CarExt" | "CarExtEnv");
        let tpg_bpp = num(&ini, &format!("tpage{}.details", page + 1), "bpp", 0)?;
        let (alpha_mode, alpha_cutoff) = alpha_mode(&render, tpg_bpp, &scene.textures[page].rgba);
        let double_sided = u32le(&e.data, 12)? & 0x20 == 0;
        matmap.insert(e.index, scene.materials.len());
        scene.materials.push(Material {
            name: format!("mt{}:{render}", e.index),
            texture: Some(page),
            base_color: [1.; 4],
            paintable,
            alpha_mode,
            alpha_cutoff,
            double_sided,
            depth_bias: u32le(&e.data, 0x114)? as i32,
            prelit: false,
        });
    }
    scene.car_in_car = in_car_parts(&crp, &style, &matmap)?;
    for (ai, a) in crp.articles.iter().enumerate() {
        let article_name = a
            .find("Name", 0)
            .map(text)
            .unwrap_or_else(|| format!("article{ai}"));
        let base = a.find("Base", 0).ok_or("article has no Base")?;
        let info = bytes(&base.data, 68, 16)?;
        let base_summary = format!(
            "{article_name} [geom={}, type={}, flags=0x{:02x}]",
            info[0], info[1], info[10]
        );
        let levels = &article_levels[ai];
        let lod = if levels.contains(&scene_lod) {
            scene_lod
        } else if levels.contains(&0) {
            0
        } else {
            skipped.push(format!(
                "{base_summary}: excluded by Base/style or absent at scene level {scene_lod}"
            ));
            continue;
        };
        let name = article_name;
        if a.find("vt", lod).is_none() {
            skipped.push(format!("{name}: no vt for Base level {lod}"));
            continue;
        }
        selected.push(base_summary.clone());
        let translation = if let Some(t) = a.find("tr", lod) {
            [
                f32le(&t.data, 48)? / 5.0,
                f32le(&t.data, 52)? / 5.0,
                f32le(&t.data, 56)? / 5.0,
            ]
        } else {
            [0.0; 3]
        };
        let vt = a.find("vt", lod).unwrap();
        let uv = a.find("uv", lod);
        let nm = a.find("nm", lod);
        if vt.data.len() % 16 != 0 {
            return Err(format!("{name}: unaligned vertices"));
        }
        let first_mesh = scene.meshes.len();
        for pr in a
            .children
            .iter()
            .filter(|e| e.tag == "pr" && e.index >> 12 == lod)
        {
            let result = mesh(
                pr,
                vt,
                nm,
                uv,
                translation,
                &matmap,
                format!("{name}#{ai}/{}", pr.index & 4095),
            );
            let mut m = result.map_err(|e| format!("{name}, CRP offset 0x{:x}: {e}", pr.offset))?;
            if !m.indices.is_empty() {
                // NFS5 stores model coordinates five times larger than the
                // viewer scale. mesh() applies that scale once and adds the
                // part translation; only the handedness conversion remains.
                for v in &mut m.vertices {
                    v.position = to_scene_coordinates(v.position);
                    v.normal = to_scene_coordinates(v.normal);
                    for k in 0..3 {
                        scene.bounds[0][k] = scene.bounds[0][k].min(v.position[k]);
                        scene.bounds[1][k] = scene.bounds[1][k].max(v.position[k]);
                    }
                }
                // A raw CRP cross product would have the opposite direction
                // after the Z reflection above. Generate missing normals only
                // once coordinates are in the Scene contract.
                for tri in m.vertices.as_chunks_mut::<3>().0 {
                    if tri.iter().all(|vertex| vertex.normal == [0.; 3]) {
                        let normal =
                            triangle_normal([tri[0].position, tri[1].position, tri[2].position]);
                        for vertex in tri {
                            vertex.normal = normal;
                        }
                    }
                }
                scene.meshes.push(m);
            }
        }
        // Top at rest: the closed (odd type) mesh stays in `meshes`, the open one is kept apart.
        if info[0] == ROOF_REST_GEOMETRY && first_mesh < scene.meshes.len() {
            if info[1] % 2 == 1 {
                scene.car_roof_up.extend(first_mesh..scene.meshes.len());
            } else {
                scene.car_roof_down.extend(scene.meshes.drain(first_mesh..));
            }
        }
        // Cabrio top (frame channel 1) and hood / trunk lid (transform channels 8, 9).
        let is_top = info[9] == 1 && info[8] > 1;
        if (is_top || matches!(info[7], 6..=9)) && first_mesh < scene.meshes.len() {
            let mut frames: Vec<Vec<Mesh>> = Vec::new();
            if is_top {
                for f in 1..u16::from(info[8]) {
                    let index = lod | (f << 4);
                    let Some(vt_f) = a.find("vt", index) else {
                        break;
                    };
                    let nm_f = a.find("nm", index).or(nm);
                    let mut set = Vec::new();
                    for pr in a
                        .children
                        .iter()
                        .filter(|e| e.tag == "pr" && e.index >> 12 == lod)
                    {
                        let mut m = mesh(
                            pr,
                            vt_f,
                            nm_f,
                            uv,
                            translation,
                            &matmap,
                            format!("{name}#{ai}/{} frame {f}", pr.index & 4095),
                        )
                        .map_err(|e| format!("{name} frame {f}: {e}"))?;
                        if !m.indices.is_empty() {
                            finish_scene_mesh(&mut m);
                            set.push(m);
                        }
                    }
                    frames.push(set);
                }
            }
            let pivot = [
                -translation[0] * 5.0,
                translation[1] * 5.0,
                translation[2] * 5.0,
            ];
            // Centroid of the closed part relative to the hinge, in body space [-x, y, z].
            let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
            for m in &scene.meshes[first_mesh..] {
                for v in &m.vertices {
                    sum[0] += -v.position[0] * 5.0;
                    sum[1] += v.position[1] * 5.0;
                    sum[2] += -v.position[2] * 5.0;
                    n += 1.0;
                }
            }
            let cz = if n > 0.0 { sum[2] / n - pivot[2] } else { 0.0 };
            // Rotation by +t about x lifts a point at z < 0 of the hinge (y' = y cos t - z sin t).
            scene.car_anim_parts.push(CarAnimPart {
                channel: if is_top { 1 } else { info[7] },
                pivot,
                axis: u8::from(matches!(info[7], 6 | 7) && !is_top),
                open_sign: if cz > 0.0 { -1.0 } else { 1.0 },
                extent: match info[7] {
                    _ if is_top => 1.0,
                    6 | 7 => 0.15,
                    8 => tpg_f32(&ini, "hoodextent", 0.15).abs(),
                    _ => tpg_f32(&ini, "trunkextent", -0.15).abs(),
                },
                base_meshes: (first_mesh..scene.meshes.len()).collect(),
                frames,
            });
        }
        // Base geometry domains 13/14 are the selected front/rear wheel articles.
        // Keep their source pivots and mesh membership instead of guessing axle
        // widths from unrelated SIM scalars or mesh-name substrings.
        if is_wheel_article(info[0], info[2], info[3]) && first_mesh < scene.meshes.len()
        {
            let center = [
                -translation[0] * 5.0,
                translation[1] * 5.0,
                translation[2] * 5.0,
            ];
            let radius = vt
                .data
                .as_chunks::<16>()
                .0
                .iter()
                .map(|v| {
                    let y = f32::from_le_bytes(v[4..8].try_into().unwrap());
                    let z = f32::from_le_bytes(v[8..12].try_into().unwrap());
                    // Frontend wheels also contain square billboard vertices;
                    // their diagonal is not the rolling radius.
                    y.abs().max(z.abs())
                })
                .fold(0.0f32, f32::max);
            let wheel_index = if info[0] == 13 { 0 } else { 2 } + usize::from(center[0] > 0.0);
            scene.car_wheels.push(CarWheel {
                wheel_index,
                center,
                radius,
                mesh_indices: (first_mesh..scene.meshes.len()).collect(),
            });
        }
    }
    fix_door_signs(&mut scene);
    if scene.meshes.is_empty() {
        return Err("no drawable meshes".into());
    }
    scene.car_wheels.sort_by_key(|wheel| wheel.wheel_index);
    if scene.car_wheels.len() != 4
        || scene.car_wheels.iter().enumerate().any(|(i, w)| {
            w.wheel_index != i
                || !w.center.iter().all(|v| v.is_finite())
                || !(0.1..0.8).contains(&w.radius)
        })
    {
        scene.car_wheels.clear();
        scene
            .diagnostics
            .push("No complete selected CRP wheel geometry; articulation unavailable".into());
    }
    scene.diagnostics.push(format!(
        "Skipped articles ({}): {}",
        skipped.len(),
        skipped.join(" | ")
    ));
    scene.diagnostics.push(format!(
        "Selected articles ({}): {}",
        selected.len(),
        selected.join(", ")
    ));
    scene.diagnostics.push("Exterior alpha0 is cutout, intermediate alpha selects test paint, alpha255 preserves artwork; full CLR decoding remains unverified. Signed mt+0x114 supplies material depth bias. No geometry placeholders.".into());
    Ok(scene)
}

/// Raw-scaled mesh (`mesh()`) to scene coordinates; missing normals from the triangles.
fn finish_scene_mesh(m: &mut Mesh) {
    for v in &mut m.vertices {
        v.position = to_scene_coordinates(v.position);
        v.normal = to_scene_coordinates(v.normal);
    }
    for tri in m.vertices.as_chunks_mut::<3>().0 {
        if tri.iter().all(|vertex| vertex.normal == [0.; 3]) {
            let normal = triangle_normal([tri[0].position, tri[1].position, tri[2].position]);
            for vertex in tri {
                vertex.normal = normal;
            }
        }
    }
}

/// Doors swing outward: the sign of the rotation about the body y axis (`Ry(a)`: x' = x cos a +
/// z sin a) follows the side of the door and the position of its mass relative to the hinge,
/// taken from the largest part of the channel and used for all its parts (approximation A232).
fn fix_door_signs(scene: &mut Scene) {
    for channel in [6u8, 7] {
        let main = scene
            .car_anim_parts
            .iter()
            .filter(|p| p.channel == channel)
            .max_by_key(|p| p.base_meshes.iter().map(|&i| scene.meshes[i].vertices.len()).sum::<usize>());
        let Some(main) = main else { continue };
        let (mut sx, mut sz, mut n) = (0.0f32, 0.0f32, 0.0f32);
        for &i in &main.base_meshes {
            for v in &scene.meshes[i].vertices {
                sx += -v.position[0] * 5.0;
                sz += -v.position[2] * 5.0;
                n += 1.0;
            }
        }
        if n == 0.0 {
            continue;
        }
        let side = if sx / n >= 0.0 { 1.0 } else { -1.0 };
        let toward_rear = sz / n - main.pivot[2];
        let sign = if toward_rear >= 0.0 { side } else { -side };
        for p in scene.car_anim_parts.iter_mut().filter(|p| p.channel == channel) {
            p.open_sign = sign;
        }
    }
}

fn mesh(
    pr: &Entry,
    vt: &Entry,
    nm: Option<&Entry>,
    uv: Option<&Entry>,
    translation: [f32; 3],
    materials: &BTreeMap<u16, usize>,
    name: String,
) -> Result<Mesh, String> {
    let d = &pr.data;
    let material = *materials
        .get(&u16le(d, 4)?)
        .ok_or("missing referenced material")?;
    let ni = u32le(d, 40)? as usize;
    let nd = u32le(d, 44)? as usize;
    if ni > 64 || nd > 16 {
        return Err("unreasonable part rows".into());
    }
    let ib = 48 + ni * 16;
    let data = ib + nd * 8;
    let raw = bytes(
        d,
        data,
        nd.checked_mul(pr.count).ok_or("index size overflow")?,
    )?;
    if !pr.count.is_multiple_of(3) {
        return Err("triangle index count not divisible by three".into());
    }
    let mut index_rows = Vec::new();
    for row in 0..nd {
        let offset = u32le(d, ib + row * 8 + 4)? as usize;
        bytes(raw, offset, pr.count)?;
        index_rows.push(offset);
    }
    let mut channels = BTreeMap::new();
    for i in 0..ni {
        let o = 48 + i * 16;
        let kind = u16le(d, o + 10)?;
        if kind <= 2 {
            let adjust = u32le(d, o + 4)? as usize;
            // -1 means an expanded, sequential stream. It is legal for
            // vertices/normals to be sequential while UVs remain indexed.
            let row = u16le(d, o + 14)?;
            let offset = if row == u16::MAX {
                None
            } else {
                Some(
                    *index_rows
                        .get(row as usize)
                        .ok_or("index row reference out of bounds")?,
                )
            };
            let stride = if kind == 2 { 8 } else { 16 };
            if !adjust.is_multiple_of(stride) {
                return Err("unaligned index base".into());
            }
            channels.insert(kind, (offset, adjust / stride));
        }
    }
    let (vo, va) = *channels.get(&0).ok_or("missing vertex index stream")?;
    let mut vertices = Vec::with_capacity(pr.count);
    let mut indices = Vec::with_capacity(pr.count);
    for i in 0..pr.count {
        let ix = vo.map_or(i, |off| raw[off + i] as usize) + va;
        let mut p = [0.; 3];
        for (k, v) in p.iter_mut().enumerate() {
            *v = f32le(&vt.data, ix * 16 + k * 4)?;
        }
        let mut normal = [0.; 3];
        if let (Some(n), Some(&(off, adj))) = (nm, channels.get(&1)) {
            let ix = off.map_or(i, |off| raw[off + i] as usize) + adj;
            for (k, v) in normal.iter_mut().enumerate() {
                *v = f32le(&n.data, ix * 16 + k * 4)?;
            }
        }
        let mut tex = [0.; 2];
        if let (Some(u), Some(&(off, adj))) = (uv, channels.get(&2)) {
            let ix = off.map_or(i, |off| raw[off + i] as usize) + adj;
            for (k, v) in tex.iter_mut().enumerate() {
                *v = f32le(&u.data, ix * 8 + k * 4)?;
            }
        }
        let position = mesh_position(p, translation);
        let normal = normalize(normal);
        indices.push(vertices.len() as u32);
        vertices.push(Vertex {
            position,
            normal,
            uv: tex,
            color: [1.0; 4],
        });
    }
    Ok(Mesh {
        name,
        vertices,
        indices,
        material,
    })
}
fn mesh_position(p: [f32; 3], translation: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| p[i] / 5.0 + translation[i])
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if len > 1e-8 {
        v.map(|x| x / len)
    } else {
        [0.; 3]
    }
}
fn triangle_normal(triangle: [[f32; 3]; 3]) -> [f32; 3] {
    let a: [f32; 3] = std::array::from_fn(|i| triangle[1][i] - triangle[0][i]);
    let b: [f32; 3] = std::array::from_fn(|i| triangle[2][i] - triangle[0][i]);
    normalize([
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(text: &str) -> Ini {
        nfs_formats::parse_ini(text.as_bytes()).unwrap()
    }

    #[test]
    fn driver_camera_prefers_style_keys() {
        // 550.tpg [header].
        let tpg = ini(
            "[header]\nnumstyles=1\ndrivercampos=-0.234375,0.375564,-0.196614\n\
            drivercamposstyle0=-0.239375,0.425564,-0.121614\n\
            drivercamrotstyle0=9.600019,1.799999,0.000000\n",
        );
        let cam = driver_camera(&tpg, 0);
        assert_eq!(cam.position, [-0.239375, 0.425564, -0.121614]);
        assert_eq!(cam.rotation_deg, [9.600019, 1.799999, 0.0]);
        // Another style falls back to the plain keys.
        assert_eq!(
            driver_camera(&tpg, 1).position,
            [-0.234375, 0.375564, -0.196614]
        );
        assert_eq!(driver_camera(&tpg, 1).rotation_deg, [0.0; 3]);
    }
    #[test]
    fn driver_camera_without_style_keys() {
        // 993.tpg [header].
        let tpg = ini("[header]\ndrivercampos=-0.382813,0.397512,-0.155002\n\
            drivercamrot=3.649998,-0.050000,0.000000\n");
        let cam = driver_camera(&tpg, 0);
        assert_eq!(cam.position, [-0.382813, 0.397512, -0.155002]);
        assert_eq!(cam.rotation_deg, [3.649998, -0.05, 0.0]);
        // No keys: the 0x441a10 defaults; a bad component stops the scan.
        assert_eq!(driver_camera(&ini("[header]\n"), 0), DriverCamera::DEFAULT);
        let partial = ini("[header]\ndrivercampos=1.5,x,2\n");
        assert_eq!(driver_camera(&partial, 0).position, [1.5, 0.4, -0.25]);
    }
    #[test]
    fn local_tpg_driver_cameras() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/CarModel");
        if !root.join("550.tpg").exists() {
            return;
        }
        let read = |car: &str| {
            let tpg = parse_ini_file(&root.join(format!("{car}.tpg")));
            driver_camera(&tpg, 0)
        };
        let c550 = read("550");
        assert_eq!(c550.position, [-0.239375, 0.425564, -0.121614]);
        assert_eq!(c550.rotation_deg, [9.600019, 1.799999, 0.0]);
        assert_eq!(read("993").position, [-0.382813, 0.397512, -0.155002]);
    }
    fn parse_ini_file(path: &std::path::Path) -> Ini {
        nfs_formats::parse_ini(&std::fs::read(path).unwrap()).unwrap()
    }
    #[test]
    fn tile_selection_defaults_each_domain_to_type_zero() {
        let ini = ini("[file1.tile]\ncount=2\ngeometry1=4\ntype1=1\ntexture2=9\ntype2=0\n");
        assert!(tile_selected(&ini, &StyleSelection::default(), "file1.tile").unwrap());
    }

    #[test]
    fn tile_selection_uses_style_override_for_texture() {
        let ini = ini(
            "[style1]\ncount=1\ntexture1=13\ntype1=1\n[file3.whf1]\ncount=1\ntexture1=13\ntype1=1\n",
        );
        let style = default_style(&ini, 0).unwrap();
        assert!(tile_selected(&ini, &style, "file3.whf1").unwrap());
        assert!(!tile_selected(&ini, &StyleSelection::default(), "file3.whf1").unwrap());
    }

    #[test]
    fn tile_selection_keeps_geometry_and_texture_domains_separate() {
        let ini = ini(
            "[style1]\ncount=1\ntexture1=7\ntype1=1\n[file1.geometry]\ncount=1\ngeometry1=7\ntype1=1\n[file1.texture]\ncount=1\ntexture1=7\ntype1=1\n",
        );
        let style = default_style(&ini, 0).unwrap();
        assert!(!tile_selected(&ini, &style, "file1.geometry").unwrap());
        assert!(tile_selected(&ini, &style, "file1.texture").unwrap());
    }

    #[test]
    fn tile_selection_treats_pairs_as_alternatives() {
        let ini = ini(
            "[style1]\ncount=1\ntexture1=9\ntype1=2\n[file1.tile]\ncount=2\ngeometry1=4\ntype1=1\ntexture2=9\ntype2=2\n",
        );
        assert!(tile_selected(&ini, &default_style(&ini, 0).unwrap(), "file1.tile").unwrap());
    }

    #[test]
    fn tile_selection_rejects_incomplete_or_invalid_pairs() {
        let missing_type = ini("[file1.tile]\ncount=1\ntexture1=9\n");
        assert!(tile_selected(&missing_type, &StyleSelection::default(), "file1.tile").is_err());
        let missing_domain = ini("[file1.tile]\ncount=1\ntype1=0\n");
        assert!(tile_selected(&missing_domain, &StyleSelection::default(), "file1.tile").is_err());
        let invalid_type = ini("[file1.tile]\ncount=1\ntexture1=9\ntype1=no\n");
        assert!(tile_selected(&invalid_type, &StyleSelection::default(), "file1.tile").is_err());
    }

    #[test]
    fn sequential_vertices_support_indexed_or_sequential_uvs() {
        let entry = |tag: &str, count, data| Entry {
            tag: tag.into(),
            index: 0,
            count,
            offset: 0,
            data,
            children: vec![],
        };
        let mut positions = vec![0; 16]; // A nonzero channel base.
        for p in [[1f32, 0., 0., 1.], [0., 1., 0., 1.], [0., 0., 1., 1.]] {
            positions.extend(p.into_iter().flat_map(f32::to_le_bytes));
        }
        let vt = entry("vt", 4, positions);
        let mut texcoords = vec![0; 8];
        for uv in [[0f32, 0.], [1., 0.], [0., 1.]] {
            texcoords.extend(uv.into_iter().flat_map(f32::to_le_bytes));
        }
        let uv = entry("uv", 4, texcoords);
        for indexed_uv in [false, true] {
            let rows = usize::from(indexed_uv);
            let mut data = vec![0; 48 + 2 * 16 + rows * 8 + rows * 3];
            data[40..44].copy_from_slice(&2u32.to_le_bytes());
            data[44..48].copy_from_slice(&(rows as u32).to_le_bytes());
            for (channel, (kind, stride, row)) in [
                (0u16, 16u32, u16::MAX),
                (2, 8, if indexed_uv { 0 } else { u16::MAX }),
            ]
            .into_iter()
            .enumerate()
            {
                let offset = 48 + channel * 16;
                data[offset + 4..offset + 8].copy_from_slice(&stride.to_le_bytes());
                data[offset + 10..offset + 12].copy_from_slice(&kind.to_le_bytes());
                data[offset + 14..offset + 16].copy_from_slice(&row.to_le_bytes());
            }
            if indexed_uv {
                data[82..84].copy_from_slice(&0x4975u16.to_le_bytes());
                data[88..91].copy_from_slice(&[2, 0, 1]);
            }
            let pr = entry("pr", 3, data);
            let result = mesh(
                &pr,
                &vt,
                None,
                Some(&uv),
                [0.; 3],
                &BTreeMap::from([(0, 0)]),
                "mixed".into(),
            )
            .unwrap();
            assert_eq!(
                result
                    .vertices
                    .iter()
                    .map(|v| v.position)
                    .collect::<Vec<_>>(),
                [[0.2, 0., 0.], [0., 0.2, 0.], [0., 0., 0.2]]
            );
            assert_eq!(
                result.vertices[0].uv,
                if indexed_uv { [0., 1.] } else { [0., 0.] }
            );
            let mut invalid = pr.clone();
            invalid.data[62..64].copy_from_slice(&9u16.to_le_bytes());
            assert!(mesh(
                &invalid,
                &vt,
                None,
                Some(&uv),
                [0.; 3],
                &BTreeMap::from([(0, 0)]),
                "bad".into()
            )
            .is_err());
        }
    }

    fn atlas(width: u32, height: u32) -> Texture {
        Texture {
            name: "test".into(),
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    fn image(width: u32, height: u32, x: u32, y: u32) -> nfs_formats::Image {
        let mut rgba = Vec::new();
        for value in 1..=width * height {
            rgba.extend_from_slice(&[value as u8, 0, 0, 255]);
        }
        nfs_formats::Image {
            name: "test".into(),
            width,
            height,
            x,
            y,
            rgba,
        }
    }

    fn red(texture: &Texture, x: u32, y: u32) -> u8 {
        texture.rgba[((y * texture.width + x) * 4) as usize]
    }

    #[test]
    fn tile_blit_clips_a_one_pixel_right_edge() {
        let mut page = atlas(4, 1);
        place_tile(&mut page, &image(2, 1, 3, 0), [0, 0], false).unwrap();
        assert_eq!(red(&page, 3, 0), 1);
    }

    #[test]
    fn tile_blit_accepts_nfs5_negative_one_coordinate_spellings() {
        let mut page = atlas(2, 2);
        place_tile(&mut page, &image(2, 1, 0x0fff, 1), [0, 0], false).unwrap();
        assert_eq!(red(&page, 0, 1), 2);

        place_tile(&mut page, &image(1, 2, 1, 0xffff), [0, 0], false).unwrap();
        assert_eq!(red(&page, 1, 0), 2);
    }

    #[test]
    fn tile_blit_rotates_before_clipping() {
        let mut page = atlas(2, 2);
        place_tile(&mut page, &image(2, 3, 0x0fff, 0), [0, 0], true).unwrap();
        assert_eq!([red(&page, 0, 0), red(&page, 1, 0)], [3, 1]);
        assert_eq!([red(&page, 0, 1), red(&page, 1, 1)], [4, 2]);
    }

    #[test]
    fn tile_blit_rejects_nonintersecting_or_malformed_input() {
        let mut page = atlas(2, 2);
        assert!(place_tile(&mut page, &image(1, 1, 2, 0), [0, 0], false).is_err());
        let mut malformed = image(1, 1, 0, 0);
        malformed.rgba.pop();
        assert!(place_tile(&mut page, &malformed, [0, 0], false).is_err());
    }

    #[test]
    fn reject_missing_and_ambiguous() {
        assert!(load(&AssetFiles::new(), "356a", 0)
            .unwrap_err()
            .contains("missing resource"));
        let mut f = AssetFiles::new();
        f.insert("a/356a.crp".into(), vec![]);
        f.insert("b/356a.crp".into(), vec![]);
        assert!(load(&f, "356a", 0).unwrap_err().contains("ambiguous"));
    }
    #[test]
    fn path_identifiers_rejected() {
        assert!(load(&AssetFiles::new(), "../356a", 0).is_err());
    }
    #[test]
    fn mesh_placement_scales_then_adds_translation_before_scene_flip() {
        assert_eq!(
            to_scene_coordinates(mesh_position([10., 15., 20.], [1., 2., 3.])),
            [3., 5., -7.]
        );
    }
    #[test]
    fn fallback_normal_is_calculated_after_scene_z_flip() {
        let raw = [[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]];
        let scene = raw.map(to_scene_coordinates);
        assert_eq!(triangle_normal(scene), [0., 1., 0.]);
        assert_eq!(triangle_normal(raw), [0., -1., 0.]);
    }

    #[test]
    fn parked_levels_preserve_racing_only_membership() {
        let mut base = vec![0; 112];
        base[12..16].copy_from_slice(&1u32.to_le_bytes());
        base[108..110].copy_from_slice(&4u16.to_le_bytes());
        let article = Entry {
            tag: "Arti".into(),
            index: 0,
            count: 0,
            offset: 0,
            data: vec![],
            children: vec![Entry {
                tag: "Base".into(),
                index: 0,
                count: 0,
                offset: 0,
                data: base,
                children: vec![],
            }],
        };
        let levels = parked_levels(&article, &StyleSelection::default()).unwrap();
        assert_eq!(levels, vec![4]);
        assert!(!levels.contains(&1));
    }
    #[test]
    fn parked_lod_rejects_empty_or_unencodable_base_levels() {
        let article = |level_count: u32, lod: u16| {
            let mut base = vec![0; 112];
            base[12..16].copy_from_slice(&level_count.to_le_bytes());
            base[108..110].copy_from_slice(&lod.to_le_bytes());
            Entry {
                tag: "Arti".into(),
                index: 0,
                count: 0,
                offset: 0,
                data: vec![],
                children: vec![Entry {
                    tag: "Base".into(),
                    index: 0,
                    count: 0,
                    offset: 0,
                    data: base,
                    children: vec![],
                }],
            }
        };
        assert!(parked_levels(&article(0, 0), &StyleSelection::default()).is_err());
        assert!(parked_levels(&article(1, 16), &StyleSelection::default()).is_err());
    }
    #[test]
    fn handedness_flips_translation_once_with_the_mesh() {
        let mesh_position = [2., 3., 4.];
        let raw_translation = [5., 6., 7.];
        assert_eq!(
            to_scene_coordinates(std::array::from_fn(
                |i| mesh_position[i] + raw_translation[i]
            )),
            [7., 9., -11.]
        );
    }
    #[test]
    fn material_render_name_stops_at_first_nul() {
        assert_eq!(c_string(b"CarExt\0leftover"), "CarExt");
    }
    #[test]
    fn alpha_mode_uses_render_method_and_tpg_coverage() {
        assert_eq!(
            alpha_mode("CarExt", 1555, &[0, 0, 0, 0]),
            (AlphaMode::Mask, 0.0)
        );
        assert_eq!(
            alpha_mode("CarWheel", 1555, &[0, 0, 0, 0, 0, 0, 0, 255]),
            (AlphaMode::Mask, 0.0)
        );
        assert_eq!(
            alpha_mode("CarWheel", 4444, &[0, 0, 0, 0, 0, 0, 0, 199]),
            (AlphaMode::Blend, 0.0)
        );
    }
    #[test]
    fn required_shared_fsh_are_named_together() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        let crp_path = root.join("356a.crp");
        if !crp_path.exists() {
            return;
        }
        let crp = nfs_formats::parse_crp(&std::fs::read(crp_path).unwrap()).unwrap();
        let ini = nfs_formats::parse_ini(&std::fs::read(root.join("356a.tpg")).unwrap()).unwrap();
        assert_eq!(
            required_external_fsh(&crp, &ini, 10).unwrap(),
            ["intglass.fsh", "shadow.fsh", "cabrio.fsh"]
        );
    }
    #[test]
    fn wheel_article_rule_covers_wide_body_variants() {
        assert!(is_wheel_article(13, 10, 77) && is_wheel_article(14, 10, 78));
        assert!(is_wheel_article(13, 10, 106) && is_wheel_article(14, 10, 107));
        assert!(is_wheel_article(14, 10, 132));
        // shadows (class 11), wheel wells (domain 12) and other domains are not wheels
        assert!(!is_wheel_article(13, 11, 186) && !is_wheel_article(12, 255, 23));
        assert!(!is_wheel_article(15, 10, 77));
    }

    #[test]
    fn local_993_turbo_style_has_wheel_articulation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/CarModel");
        if !root.join("993.crp").exists() {
            return;
        }
        let mut files = AssetFiles::new();
        for name in ["993.crp", "993.tpg", "INTGLASS.FSH", "Shadow.fsh", "Cabrio.fsh"] {
            if let Ok(bytes) = std::fs::read(root.join(name)) {
                files.insert(name.into(), bytes);
            }
        }
        // Quick Race '95 911 Turbo 3.6 uses TPG style 8 (turbocoupe) with WheelFront1/WheelRear1.
        for style in [0, 8] {
            let scene = load(&files, "993", style).unwrap();
            assert_eq!(scene.car_wheels.len(), 4, "style {style}: {:?}", scene.diagnostics);
            assert!(scene.car_wheels.iter().all(|w| !w.mesh_indices.is_empty()));
        }
    }

    #[test]
    fn local_every_car_style_has_wheel_articulation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/CarModel");
        let Ok(dir) = std::fs::read_dir(&root) else {
            return;
        };
        let mut bad = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("crp") {
                continue;
            }
            let car = path.file_stem().unwrap().to_string_lossy().to_string();
            let Ok(tpg) = std::fs::read(path.with_extension("tpg")) else {
                continue;
            };
            let mut files = AssetFiles::new();
            files.insert(format!("{car}.crp"), std::fs::read(&path).unwrap());
            files.insert(format!("{car}.tpg"), tpg.clone());
            for name in ["INTGLASS.FSH", "Shadow.fsh", "Cabrio.fsh"] {
                if let Ok(bytes) = std::fs::read(root.join(name)) {
                    files.insert(name.into(), bytes);
                }
            }
            let styles = nfs_formats::parse_ini(&tpg)
                .ok()
                .and_then(|ini| ini.get("header").and_then(|h| h.get("numstyles")).and_then(|v| v.parse::<u32>().ok()))
                .unwrap_or(1);
            for style in 0..styles.max(1) {
                match load(&files, &car, style) {
                    Ok(scene) if scene.car_wheels.len() == 4 => {}
                    Ok(_) => bad.push(format!("{car} style {style}")),
                    Err(_) => {}
                }
            }
        }
        assert!(bad.is_empty(), "no wheel articulation: {bad:?}");
    }

    #[test]
    fn local_boxster_wheel_pivots_drive_geometry_and_visual_centers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData");
        if !root.join("CarModel/boxster.crp").exists() {
            return;
        }
        let mut files = AssetFiles::new();
        for name in [
            "boxster.crp",
            "boxster.tpg",
            "INTGLASS.FSH",
            "Shadow.fsh",
            "Cabrio.fsh",
        ] {
            files.insert(
                name.into(),
                std::fs::read(root.join("CarModel").join(name)).unwrap(),
            );
        }
        let scene = load(&files, "boxster", 0).unwrap();
        assert_eq!(scene.car_wheels.len(), 4, "{:?}", scene.diagnostics);
        let w = &scene.car_wheels;
        assert!((w[1].center[0] - w[0].center[0] - 1.3825873).abs() < 1e-5);
        assert!((w[3].center[0] - w[2].center[0] - 1.4817582).abs() < 1e-5);
        assert!((w[2].center[2] - w[0].center[2] - 2.4233546).abs() < 1e-5);
        for wheel in w {
            assert!((0.30..0.34).contains(&wheel.radius));
            assert!(!wheel.mesh_indices.is_empty());
            assert!(wheel
                .mesh_indices
                .iter()
                .all(|&i| scene.meshes[i].name.starts_with("Wheel")));
        }
        let sim_bytes = std::fs::read(root.join("Simulation/CarData/boxster25.sim")).unwrap();
        let spec = nfs_formats::parse_sim(&sim_bytes).unwrap();
        let mut sim = VehicleSimulation::from_sim(&spec);
        sim.apply_model_wheels(w).unwrap();
        for (physical, source) in sim.suspension.wheels.iter().zip(w) {
            assert_eq!(physical.hardpoint_body.x, source.center[0]);
            assert_eq!(physical.hardpoint_body.z, source.center[2]);
            assert_eq!(physical.tire.radius, source.radius);
        }
        for _ in 0..120 {
            sim.step(None, &VehicleControls::default(), 1.0 / 60.0);
        }
        let telemetry = sim.step(None, &VehicleControls::default(), 1.0 / 60.0);
        for (i, wheel) in sim.suspension.wheels.iter().enumerate() {
            assert!(wheel.in_contact);
            let center = telemetry.wheel_transforms[i].transform_point3(glam::Vec3::ZERO);
            assert!(
                (center.y - wheel.tire.radius).abs() < 0.002,
                "render wheel center must be one tire radius above road: {center:?}"
            );
        }
        // Source dimensions must support a sustained moderate-speed turn,
        // the operation that rolled over with the false 1 m SIM axle widths.
        sim.body.linear_velocity = sim.body.forward() * 14.0;
        for wheel in &mut sim.suspension.wheels {
            wheel.tire.omega = 14.0 / wheel.tire.radius;
        }
        let controls = VehicleControls {
            steer: -0.8,
            throttle: 0.25,
            ..VehicleControls::default()
        };
        for _ in 0..180 {
            sim.step(None, &controls, 1.0 / 60.0);
            assert!(
                sim.body.up().y > 0.7,
                "model-based Boxster turned over: {:?}",
                sim.body.up()
            );
        }
    }

    #[test]
    fn local_356a_glares_follow_the_tpg_style() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        if !root.join("356a.crp").exists() {
            return;
        }
        let mut files = AssetFiles::new();
        for name in ["356a.crp", "356a.tpg", "INTGLASS.FSH", "Shadow.fsh", "Cabrio.fsh"] {
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        let scene = load(&files, "356a", 0).unwrap();
        let find = |name: &str| scene.car_glares.iter().find(|g| g.name == name);
        // 356a.tpg [style1] "13Cabrio": geometry7 type3 selects the cabrio variants.
        let brake = find("GlareBrakeCabrio").expect("cabrio brake light");
        assert!(find("GlareBrakeCoupe").is_none() && find("GlareBrake").is_none());
        assert_eq!((brake.group, brake.kind), (2, 1));
        assert_eq!(brake.colors, [0xffff0000; 2], "red, ARGB");
        assert!(brake.normal[2] > 0.5, "faces backwards (+z): {:?}", brake.normal);
        let mirrored = find("GlareBrakeCabrio (mirrored)").unwrap();
        assert_eq!(mirrored.group, 3);
        assert_eq!(mirrored.position[0], -brake.position[0]);
        let head = find("GlareHeadLight").unwrap();
        assert_eq!((head.group, head.kind, head.beam), (0, 0, true));
        assert!(head.normal[2] < -0.9, "faces forwards (-z)");
        assert_eq!(find("GlareMirror1").unwrap().pivot, 6, "follows the left door");
        // Each glare appears once per detail level; only the scene level is used.
        assert_eq!(scene.car_glares.iter().filter(|g| g.name == "GlareHeadLight").count(), 1);
    }

    #[test]
    fn local_356a_hood_trunk_and_top_animation_parts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        if !root.join("356a.crp").exists() {
            return;
        }
        let mut files = AssetFiles::new();
        for name in [
            "356a.crp",
            "356a.tpg",
            "INTGLASS.FSH",
            "Shadow.fsh",
            "Cabrio.fsh",
            "Suit.fsh",
            "Head.fsh",
        ] {
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        // Style 4 ('59 convertible) has a cabrio top: six morph frames, five beyond the base.
        let scene = load(&files, "356a", 4).unwrap();
        let top = scene.car_anim_parts.iter().find(|p| p.channel == 1).expect("cabrio top");
        assert_eq!(top.frames.len(), 5);
        assert!(top.frames.iter().all(|f| f.len() == top.base_meshes.len()));
        let hood = scene.car_anim_parts.iter().find(|p| p.channel == 8).expect("hood");
        assert!(scene.car_anim_parts.iter().any(|p| p.channel == 9), "trunk lid");
        assert!(hood.open_sign.abs() == 1.0 && hood.frames.is_empty());
        // Doors: channel 6 (left) and 7 (right) swing about y, to opposite sides of the car.
        let door = |c: u8| scene.car_anim_parts.iter().find(|p| p.channel == c && p.axis == 1).expect("door");
        let (l, r) = (door(6), door(7));
        assert!(l.pivot[0] * r.pivot[0] < 0.0, "hinges on opposite sides: {:?} {:?}", l.pivot, r.pivot);
        assert!(l.open_sign * r.open_sign < 0.0, "mirrored swing");
        assert_eq!(l.extent, 0.15);
        // The top moves: the last frame differs from the closed shape.
        let m0 = &scene.meshes[top.base_meshes[0]];
        let m5 = &top.frames[4][0];
        assert_eq!(m0.vertices.len(), m5.vertices.len());
        assert!(m0.vertices.iter().zip(&m5.vertices).any(|(a, b)| a.position != b.position));
    }

    #[test]
    fn local_356a_in_car_parts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        if !root.join("356a.crp").exists() {
            return;
        }
        let mut files = AssetFiles::new();
        for name in [
            "356a.crp",
            "356a.tpg",
            "INTGLASS.FSH",
            "Shadow.fsh",
            "Cabrio.fsh",
            "Suit.fsh",
            "Head.fsh",
        ] {
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        let scene = load(&files, "356a", 0).unwrap();
        let parts = &scene.car_in_car;
        let find = |name: &str| parts.iter().find(|p| p.name == name);
        // [style1] selects the type-1 variants; level 2 for the body pass.
        let dash = find("Dash1").expect("dashboard");
        assert_eq!(dash.pass, InCarPass::Body);
        assert!(find("Dash").is_none());
        let steer = find("Steer1").expect("steering wheel");
        assert_eq!((steer.pass, steer.channel), (InCarPass::SteeringWheel, 5));
        // The column tilts: local z is (0, 0.284, 0.959) raw, z flipped in the scene.
        assert!((steer.pivot_axis[1] - 0.284).abs() < 0.01 && steer.pivot_axis[2] < -0.9);
        assert_eq!(find("Needle1a").unwrap().channel, 0xc);
        assert_eq!(find("Needle2a").unwrap().channel, 0xb);
        // Hands with their frames; no driver body, arms, legs, wheels, bottom or engine.
        // Driver 0: type-1 hands (0x43ae50 overrides the [style1] type 2).
        assert!(find("LeftHand2").is_none());
        let left = find("LeftHand1").unwrap();
        assert_eq!(
            (left.pass, left.frame_channel, left.frame_count),
            (InCarPass::Driver, 5, 10)
        );
        let right = find("RightHand1").unwrap();
        assert_eq!((right.frame_channel, right.frame_count), (4, 20));
        assert!(right
            .meshes
            .iter()
            .all(|m| m.frames.len() == 20
                && m.frames.iter().all(|f| f.len() == m.mesh.vertices.len())));
        for hidden in [
            "DriverBody1",
            "LeftArm1",
            "Legs1",
            "WheelFront",
            "Bottom",
            "Engine",
            "HoodIn",
        ] {
            assert!(find(hidden).is_none(), "{hidden}");
        }
        assert!(find("Body").is_some() && find("SideMirrorLeft").is_some());
    }

    #[test]
    fn local_cars_have_a_cockpit() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        let Ok(dir) = std::fs::read_dir(&root) else {
            return;
        };
        let mut files = AssetFiles::new();
        let mut cars = Vec::new();
        for entry in dir {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".crp") {
                cars.push(lower.trim_end_matches(".crp").to_string());
            }
            if lower.ends_with(".crp") || lower.ends_with(".tpg") || lower.ends_with(".fsh") {
                files.insert(name, std::fs::read(&path).unwrap());
            }
        }
        let mut without = Vec::new();
        for car in &cars {
            let scene = load(&files, car, 0).unwrap_or_else(|e| panic!("{car}: {e}"));
            let has = |channel: u8| scene.car_in_car.iter().any(|p| p.channel == channel);
            if !(has(5) && has(0xb)) {
                without.push(car.clone());
            } else {
                // Every car with a cockpit has the driver's hands (the 993 style lists none).
                let hands = scene.car_in_car.iter().filter(|p| p.pass == InCarPass::Driver);
                assert_eq!(hands.count(), 2, "{car}: hands");
            }
            // 935 has no speedometer needle; gt1's exists at level 3 only.
            if has(5) && has(0xb) && !has(0xc) {
                assert!(
                    ["935", "gt1"].contains(&car.as_str()),
                    "{car}: no speed needle"
                );
            }
        }
        // Traffic models and the post ship have no modelled interior.
        assert!(
            without
                .iter()
                .all(|c| c.starts_with("eas") || c.starts_with("eden") || c == "postship"),
            "cars without steering wheel or rev needle: {without:?}"
        );
    }

    #[test]
    fn local_render_common_has_the_effect_textures() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Render/common.fsh");
        if !path.exists() {
            return;
        }
        let mut files = AssetFiles::new();
        files.insert("gamedata/render/common.fsh".into(), std::fs::read(path).unwrap());
        let textures = effect_textures(&files).unwrap();
        for name in ["CARG", "SPIK", "GLNT", "SHAD", "FEGL"] {
            let t = textures.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!((t.width, t.height), (64, 64), "{name}");
        }
        let shad = textures.iter().find(|t| t.name == "SHAD").unwrap();
        let max_alpha = shad.rgba.chunks_exact(4).map(|p| p[3]).max().unwrap();
        assert_eq!(max_alpha, 136, "shadow is about half dark");
        assert!(effect_textures(&AssetFiles::new()).unwrap().is_empty());
    }

    #[test]
    fn local_356_variants_load_to_complete_scenes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        if !root.join("356a.crp").exists() {
            return;
        }
        for car in ["356a", "356b"] {
            let mut files = AssetFiles::new();
            for name in [
                format!("{car}.crp"),
                format!("{car}.tpg"),
                "INTGLASS.FSH".into(),
                "Shadow.fsh".into(),
                "Cabrio.fsh".into(),
            ] {
                files.insert(
                    format!("GameData/CarModel/{name}"),
                    std::fs::read(root.join(&name)).unwrap(),
                );
            }
            let scene = load(&files, car, 0).unwrap_or_else(|error| panic!("{car}: {error}"));
            assert_eq!(scene.car_wheels.len(), 4, "{car}: selected wheel metadata");
            assert_eq!(scene.textures.len(), 11);
            for (name, expected) in [("mt4:", 0), ("mt12:", -3)] {
                assert_eq!(
                    scene
                        .materials
                        .iter()
                        .find(|m| m.name.starts_with(name))
                        .unwrap()
                        .depth_bias,
                    expected
                );
            }
            assert!(
                !scene.materials.is_empty() && !scene.meshes.is_empty(),
                "{car}: empty scene"
            );
            assert!(scene.meshes.iter().all(|mesh| {
                !mesh.vertices.is_empty()
                    && mesh.indices.len() % 3 == 0
                    && mesh
                        .indices
                        .iter()
                        .all(|&index| (index as usize) < mesh.vertices.len())
            }));
            println!("{car} bounds: {:?}", scene.bounds);
            for m in scene.meshes.iter().take(15) {
                let z =
                    m.vertices.iter().map(|v| v.position[2]).sum::<f32>() / m.vertices.len() as f32;
                println!("mesh {} center z: {:.3}", m.name, z);
            }
            assert!(scene.bounds.into_iter().flatten().all(f32::is_finite));
        }
    }

    #[test]
    fn driver_tiles_are_selected_even_when_the_style_lists_no_driver_geometry() {
        let ini = nfs_formats::parse_ini(
            b"[f.cas1]\ncount=1\ngeometry1=9\ntype1=2\n[f.cas2]\ncount=1\ngeometry1=9\ntype1=3\n",
        )
        .unwrap();
        // A style such as the 993 "turbocoupe" lists no GeomIndex 9: the raw style selects none.
        let style = StyleSelection::default();
        assert!(!tile_selected(&ini, &style, "f.cas1").unwrap());
        // With the driver types of `R3DCar_SelectStyle` (driver 0: body type 2) the tile is found.
        let driven = with_driver(&style, DRIVER_INDEX);
        assert!(tile_selected(&ini, &driven, "f.cas1").unwrap());
        assert!(!tile_selected(&ini, &driven, "f.cas2").unwrap());
    }

    #[test]
    fn local_driver_hands_are_textured_in_every_style() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/CarModel");
        let Ok(dir) = std::fs::read_dir(&root) else {
            return;
        };
        let mut files = AssetFiles::new();
        for entry in dir {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            files.insert(name, std::fs::read(&path).unwrap());
        }
        for (car, styles) in [("993", 10u32), ("356a", 3)] {
            for style in 0..styles {
                let Ok(scene) = load(&files, car, style) else {
                    continue;
                };
                for part in scene.car_in_car.iter().filter(|p| p.pass == InCarPass::Driver) {
                    let mesh = &part.meshes[0].mesh;
                    let page = &scene.textures[scene.materials[mesh.material].texture.unwrap()];
                    let opaque = mesh
                        .vertices
                        .iter()
                        .filter(|v| {
                            let x = ((v.uv[0] * page.width as f32) as usize).min(page.width as usize - 1);
                            let y = ((v.uv[1] * page.height as f32) as usize).min(page.height as usize - 1);
                            page.rgba[(y * page.width as usize + x) * 4 + 3] != 0
                        })
                        .count();
                    assert!(opaque > 0, "{car} style {style}: {} is untextured", part.name);
                }
            }
        }
    }
}

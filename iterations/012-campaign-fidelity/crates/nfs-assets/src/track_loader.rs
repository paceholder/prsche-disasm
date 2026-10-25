use std::collections::BTreeMap;

use nfs_formats::{
    bytes, f32le, parse_edg, parse_fsh, parse_jnc, parse_lsp, parse_map, parse_scn,
    parse_track_crp, u16le, u32le, Entry,
};

use super::*;

fn edge_color(flags: u8) -> [f32; 4] {
    if flags == 0 {
        [0.0, 0.8, 1.0, 1.0] // Cyan for normal road boundaries
    } else if flags & 0x10 != 0 {
        [1.0, 0.6, 0.0, 1.0] // Orange for barriers/walls
    } else if flags & 0x20 != 0 {
        [0.2, 1.0, 0.2, 1.0] // Bright green for curbs/grass
    } else {
        [1.0, 0.2, 0.8, 1.0] // Magenta for other flags
    }
}

fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn assemble_boundary_lines(edges: &[TopologyEdge]) -> Vec<TopologyLine> {
    let mut lines = Vec::new();
    let mut current_points: Vec<[f32; 3]> = Vec::new();
    let mut current_color = [0.0; 4];

    for edge in edges {
        let color = edge_color(edge.flags);
        if current_points.is_empty() {
            current_points.push(edge.p1);
            current_points.push(edge.p2);
            current_color = color;
        } else if color == current_color && dist_sq(*current_points.last().unwrap(), edge.p1) < 1e-4
        {
            current_points.push(edge.p2);
        } else {
            lines.push(TopologyLine {
                points: std::mem::take(&mut current_points),
                color: current_color,
            });
            current_points.push(edge.p1);
            current_points.push(edge.p2);
            current_color = color;
        }
    }

    if !current_points.is_empty() {
        lines.push(TopologyLine {
            points: current_points,
            color: current_color,
        });
    }

    lines
}

fn find<'a>(files: &'a AssetFiles, name: &str) -> Result<&'a [u8], String> {
    let name = name.to_ascii_lowercase();
    let suffix = format!("/{name}");
    let matches: Vec<_> = files
        .iter()
        .filter(|(k, _)| {
            let k = k.replace('\\', "/").to_ascii_lowercase();
            k == name || k.ends_with(&suffix)
        })
        .collect();

    if matches.is_empty() {
        return Err(format!("missing resource {name}"));
    }
    if matches.len() == 1 {
        return Ok(matches[0].1);
    }
    let preferred: Vec<_> = matches
        .iter()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !k.contains("sky") && !k.contains("trackart") && !k.contains("fedata")
        })
        .copied()
        .collect();
    if preferred.len() == 1 {
        return Ok(preferred[0].1);
    }
    Err(format!("ambiguous resource {name}"))
}

fn c_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data.split(|byte| *byte == 0).next().unwrap_or(&[])).into_owned()
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

pub fn to_scene_coordinates(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[1], -v[2]]
}

pub fn load(files: &AssetFiles, track: &str) -> Result<Scene, String> {
    if track.is_empty()
        || !track
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("track identifier must be alphanumeric".into());
    }
    let crp_data = find(files, &format!("{track}.crp"))?;
    let crp = parse_track_crp(crp_data).map_err(|e| format!("{track}.crp: {e}"))?;

    let fsh_name = crp
        .misc
        .iter()
        .find(|e| e.tag == "sn" && e.index == 0)
        .map(|e| c_string(&e.data))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{track}.fsh"));

    let fsh_data = find(files, &fsh_name)?;
    let images = parse_fsh(fsh_data).map_err(|e| format!("{fsh_name}: {e}"))?;

    let mut scene = Scene {
        meshes: Vec::new(),
        car_collision_box: None,
        car_driver_camera: None,
        car_wheels: Vec::new(),
        car_glares: Vec::new(),
        car_in_car: Vec::new(),
        effect_textures: Vec::new(),
        particle_defs: Vec::new(),
        textures: Vec::new(),
        materials: Vec::new(),
        bounds: [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
        diagnostics: vec![format!(
            "Track {track}: {} articles, {} misc entries, {} decoded bytes; {} textures from {fsh_name}",
            crp.articles.len(),
            crp.misc.len(),
            crp.decoded_size,
            images.len(),
        )],
        prop_articles: Vec::new(),
        prop_instances: Vec::new(),
        scenario_start: None,
        scenario_end: None,
        scenario_name: None,
        scenario_gates: Vec::new(),
        sky_texture: None,
        sky_layers: [None, None, None],
        sun_position: None,
        topology: None,
        road_surface: None,
        course: None,
        ai_track: None,
    };

    let mut tex_map = BTreeMap::new();
    for image in images {
        let idx = scene.textures.len();
        let name_key = image.name.to_ascii_lowercase();
        scene.textures.push(Texture {
            name: image.name,
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
        tex_map.insert(name_key, idx);
    }

    let mut matmap = BTreeMap::new();
    let mut material_entries: Vec<_> = crp.misc.iter().filter(|e| e.tag == "mt").collect();
    material_entries.sort_by_key(|e| e.index);

    for e in material_entries {
        let render_method = if e.data.len() >= 32 {
            c_string(bytes(&e.data, 16, 16).map_err(|err| format!("mt{}: {err}", e.index))?)
        } else {
            String::from("Opaque")
        };

        let tex_name = if e.data.len() >= 0x2c {
            let name_bytes =
                bytes(&e.data, 0x28, 4).map_err(|err| format!("mt{}: {err}", e.index))?;
            c_string(name_bytes).trim().to_ascii_lowercase()
        } else {
            String::new()
        };

        let texture = tex_map.get(&tex_name).copied();
        let alpha_mode = if let Some(t_idx) = texture {
            let has_transparency = scene.textures[t_idx]
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] < 255);
            if has_transparency {
                if render_method == "Transparency" {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Mask
                }
            } else {
                AlphaMode::Opaque
            }
        } else {
            AlphaMode::Opaque
        };

        let double_sided = render_method != "Opaque";
        let depth_bias = if e.data.len() >= 0x118 {
            u32le(&e.data, 0x114).unwrap_or(0) as i32
        } else {
            0
        };

        matmap.insert(e.index, scene.materials.len());
        scene.materials.push(Material {
            name: format!("mt{}:{render_method}", e.index),
            texture,
            base_color: [1.0; 4],
            paintable: false,
            alpha_mode,
            alpha_cutoff: 0.5,
            double_sided,
            depth_bias,
            // Track geometry carries prelit `df` vertex colours (approximation A22).
            prelit: true,
        });
    }

    let mut raw_meshes = Vec::new();
    // Rendering and source collision resources are assembled separately.
    let mut prop_templates: Vec<(u32, Vec<Mesh>)> = Vec::new();
    let mut total_tris = 0;
    for (ai, a) in crp.articles.iter().enumerate() {
        let article_name = a
            .children
            .iter()
            .find(|c| c.tag == "Name" && c.index == 0)
            .map(|c| c_string(&c.data))
            .unwrap_or_else(|| format!("article{ai}"));

        // Articles with Base flag 0x8000 are dynamic/instanced library props (e.g. smackable cones,
        // vehicles, spectator figures placed by .scn files), not static track world geometry.
        let base_entry = a.children.iter().find(|c| c.tag == "Base" && c.index == 0);
        let base_word = base_entry
            .and_then(|base| u32le(&base.data, 0).ok())
            .unwrap_or(0);
        let is_library_prop = base_word & 0x8000 != 0;
        if is_library_prop {
            // Extract FourCC from Base:0 at offset 0x44 (68 bytes).
            let fourcc = base_entry
                .and_then(|base| u32le(&base.data, 0x44).ok())
                .unwrap_or(0);
            if fourcc != 0 {
                let mut prop_meshes = Vec::new();
                // Build prop meshes (centered at origin, no world transform)
                if let Some(vt) = a.children.iter().find(|c| c.tag == "vt" && c.index == 0) {
                    if vt.data.len() % 16 == 0 {
                        let uv = a.children.iter().find(|c| c.tag == "uv" && c.index == 0);
                        let df = a.children.iter().find(|c| c.tag == "df" && c.index == 0);
                        let n_verts = vt.data.len() / 16;
                        let mut raw_positions = Vec::with_capacity(n_verts);
                        for i in 0..n_verts {
                            let x = f32le(&vt.data, i * 16).unwrap_or(0.0);
                            let y = f32le(&vt.data, i * 16 + 4).unwrap_or(0.0);
                            let z = f32le(&vt.data, i * 16 + 8).unwrap_or(0.0);
                            raw_positions.push(to_scene_coordinates([x, y, z]));
                        }
                        for pr in a
                            .children
                            .iter()
                            .filter(|c| c.tag == "pr" && (c.index >> 12) == 0)
                        {
                            if let Ok(m) = parse_mesh(
                                pr,
                                df,
                                &raw_positions,
                                uv,
                                &matmap,
                                format!("prop_{fourcc:#x}#{ai}/{}", pr.index),
                            ) {
                                if !m.indices.is_empty() {
                                    prop_meshes.push(m);
                                }
                            }
                        }
                    }
                }
                if !prop_meshes.is_empty() {
                    prop_templates.push((fourcc, prop_meshes));
                }
            }
            continue;
        }

        let Some(vt) = a.children.iter().find(|c| c.tag == "vt" && c.index == 0) else {
            continue;
        };
        let uv = a.children.iter().find(|c| c.tag == "uv" && c.index == 0);
        let df = a.children.iter().find(|c| c.tag == "df" && c.index == 0);
        let tr = a.children.iter().find(|c| c.tag == "tr" && c.index == 0);

        if vt.data.len() % 16 != 0 {
            return Err(format!("{article_name}: unaligned vertices in vt:0"));
        }
        let n_verts = vt.data.len() / 16;

        let matrix = if let Some(t) = tr {
            if t.data.len() >= 64 {
                let mut m = [0.0f32; 16];
                for (k, val) in m.iter_mut().enumerate() {
                    *val = f32le(&t.data, k * 4).map_err(|err| format!("{article_name}: {err}"))?;
                }
                Some(m)
            } else {
                None
            }
        } else {
            None
        };

        let mut raw_positions = Vec::with_capacity(n_verts);
        for i in 0..n_verts {
            let x = f32le(&vt.data, i * 16)?;
            let y = f32le(&vt.data, i * 16 + 4)?;
            let z = f32le(&vt.data, i * 16 + 8)?;
            let (tx, ty, tz) = if let Some(m) = matrix {
                (
                    x * m[0] + y * m[4] + z * m[8] + m[12],
                    x * m[1] + y * m[5] + z * m[9] + m[13],
                    x * m[2] + y * m[6] + z * m[10] + m[14],
                )
            } else {
                (x, y, z)
            };
            raw_positions.push(to_scene_coordinates([tx, ty, tz]));
        }

        for pr in a
            .children
            .iter()
            .filter(|c| c.tag == "pr" && (c.index >> 12) == 0)
        {
            let m = parse_mesh(
                pr,
                df,
                &raw_positions,
                uv,
                &matmap,
                format!("{article_name}#{ai}/{}", pr.index),
            )?;
            if !m.indices.is_empty() {
                total_tris += m.indices.len() / 3;
                for v in &m.vertices {
                    for k in 0..3 {
                        scene.bounds[0][k] = scene.bounds[0][k].min(v.position[k]);
                        scene.bounds[1][k] = scene.bounds[1][k].max(v.position[k]);
                    }
                }
                raw_meshes.push(m);
            }
        }
    }

    let anim = nfs_formats::animdefs::parse_animdefs(&String::from_utf8_lossy(find(
        files,
        "animdefs.txt",
    )?))?;
    let support = physics::original_support_loader::assemble_initial_track_support(&crp, &anim)?;
    let skipped_special = support.skipped_special_articles;
    let skipped_disabled = support.skipped_disabled_articles;
    let skipped_library = support.skipped_library_articles;
    let names = crp
        .articles
        .iter()
        .enumerate()
        .map(|(index, article)| {
            article
                .find("Name", 0)
                .map(|entry| c_string(&entry.data))
                .unwrap_or_else(|| format!("article{index}"))
        })
        .collect::<Vec<_>>();
    let original_edges = match find(files, &format!("{track}.edg")) {
        Ok(bytes) => physics::original_scene_objects::load_raw_edg_objects(bytes)?,
        Err(_) => {
            scene.diagnostics.push(
                "Original EDG absent: temporary-tree boundary generation 488590 remains incomplete"
                    .into(),
            );
            vec![]
        }
    };
    let edge_count = original_edges.len();
    let road_surface = RoadSurface::from_original_scene_support(support, &names, original_edges)?;
    let (polygons, nodes) = road_surface.original_support_counts().unwrap();
    scene.diagnostics.push(format!(
        "Original scene support: {polygons} polygons, {edge_count} EDG objects, {nodes} nodes; special={skipped_special}, disabled={skipped_disabled}, library={skipped_library}, degenerate={}. Porsche.exe 4750b0/48839f + 487e80/485100 -> 4883bc/4883f5 -> 484ae0 -> 474060/499a70. Special objects and original force/body response binding remain incomplete.",
        road_surface.report().rejected_degenerate,
    ));
    scene.road_surface = Some(road_surface);

    // Combine meshes sharing the same material to drastically reduce GPU buffer count and draw calls
    let mut combined: BTreeMap<usize, Mesh> = BTreeMap::new();
    for m in raw_meshes {
        let entry = combined.entry(m.material).or_insert_with(|| Mesh {
            name: format!("mat_{}", m.material),
            vertices: Vec::new(),
            indices: Vec::new(),
            material: m.material,
        });
        let base_idx = entry.vertices.len() as u32;
        entry.vertices.extend(m.vertices);
        entry
            .indices
            .extend(m.indices.into_iter().map(|idx| idx + base_idx));
    }
    scene.meshes = combined.into_values().collect();

    if scene.meshes.is_empty() {
        return Err("no drawable meshes in track".into());
    }

    scene.diagnostics.push(format!(
        "Loaded {} meshes, {} triangles, bounds min=[{:.2}, {:.2}, {:.2}] max=[{:.2}, {:.2}, {:.2}]",
        scene.meshes.len(),
        total_tris,
        scene.bounds[0][0], scene.bounds[0][1], scene.bounds[0][2],
        scene.bounds[1][0], scene.bounds[1][1], scene.bounds[1][2],
    ));

    // Append prop template meshes to scene.meshes after static geometry
    for (fourcc, meshes) in prop_templates {
        let prop_mesh_start = scene.meshes.len();
        scene.meshes.extend(meshes);
        let prop_mesh_end = scene.meshes.len();
        if prop_mesh_end > prop_mesh_start {
            scene.prop_articles.push(PropArticle {
                fourcc,
                mesh_range: prop_mesh_start..prop_mesh_end,
            });
        }
    }

    // --- Prop instancing from .scn files ---
    // Find scenario files: prefer <track>_st1.scn, fall back to any <track>_st*.scn
    let mut scn_keys: Vec<&String> = files
        .keys()
        .filter(|k| {
            let k_low = k.to_ascii_lowercase();
            k_low.ends_with(".scn")
                && !k_low.contains("audio")
                && !k_low.contains("camera")
                && !k_low.contains("start")
        })
        .filter(|k| {
            let stem = k
                .rsplit('/')
                .next()
                .unwrap_or(k)
                .strip_suffix(".scn")
                .or_else(|| k.rsplit('/').next().unwrap_or(k).strip_suffix(".SCN"))
                .unwrap_or("")
                .to_ascii_lowercase();
            stem.starts_with(&track.to_ascii_lowercase())
        })
        .collect();
    scn_keys.sort();

    // Prefer _st1 if available
    let scn_key = scn_keys
        .iter()
        .find(|k| {
            let low = k.to_ascii_lowercase();
            low.contains("_st1.scn") && !low.contains("_st1")
                || low.ends_with(&format!("{}_st1.scn", track.to_ascii_lowercase()))
        })
        .or_else(|| scn_keys.first())
        .cloned()
        .cloned();

    if let Some(scn_key) = scn_key {
        if let Some(scn_data) = files.get(&scn_key) {
            let scn_text = String::from_utf8_lossy(scn_data);
            match parse_scn(&scn_text) {
                Ok(scn) => {
                    let definitions = find(files, "animdefs.txt").and_then(|data| {
                        nfs_formats::animdefs::parse_animdefs(&String::from_utf8_lossy(data))
                    });
                    if let Err(error) = &definitions {
                        scene
                            .diagnostics
                            .push(format!("Scenario object metadata unavailable: {error}"));
                    }
                    let linked_prop = |trigger: &nfs_formats::scn::ScenarioTrigger| {
                        scn.linked_geom(trigger)
                            .filter(|geom| geom.flags as i16 == 1)
                            .map(|geom| crate::scenario::PropTarget {
                                fourcc: geom.fourcc,
                                position: to_scene_coordinates(geom.position),
                            })
                    };
                    scene.scenario_name = Some(
                        scn_key
                            .rsplit('/')
                            .next()
                            .unwrap_or(&scn_key)
                            .to_ascii_lowercase(),
                    );
                    if let Some(start) = scn.triggers.iter().find(|t| t.trigger_type == Some(2)) {
                        let forward = to_scene_coordinates(start.direction);
                        if forward[0].hypot(forward[2]) > 0.5 {
                            scene.scenario_start = Some(crate::ScenarioStart {
                                position: to_scene_coordinates(start.position),
                                forward,
                                linked_prop: linked_prop(start),
                            });
                        }
                    }
                    scene.scenario_end = scn
                        .triggers
                        .iter()
                        .find(|t| t.trigger_type == Some(1))
                        .map(|t| to_scene_coordinates(t.position));
                    if scene.scenario_start.is_some() {
                        let mut triggers: Vec<_> = scn
                            .triggers
                            .iter()
                            .filter(|t| t.sequence > 0 && matches!(t.trigger_type, Some(0 | 1)))
                            .collect();
                        triggers.sort_by_key(|t| t.sequence);
                        for trigger in triggers {
                            let center = to_scene_coordinates(trigger.position);
                            if let (Some(shape), Some(speed_range)) =
                                (trigger.shape, trigger.speed_range)
                            {
                                scene.scenario_gates.push(crate::ScenarioGate {
                                    sequence: trigger.sequence as u32,
                                    center,
                                    segment: trigger.segment.map(to_scene_coordinates),
                                    width: trigger.width,
                                    forward: to_scene_coordinates(trigger.direction),
                                    velocity_direction: to_scene_coordinates(
                                        trigger.velocity_direction,
                                    ),
                                    shape,
                                    speed_range,
                                    is_end: trigger.trigger_type == Some(1),
                                    linked_prop: linked_prop(trigger),
                                });
                            }
                        }
                    }
                    let mut matched = 0;
                    let mut unmatched = 0;
                    for ge in &scn.geom_elements {
                        if let Some(idx) = scene
                            .prop_articles
                            .iter()
                            .position(|pa| pa.fourcc == ge.fourcc)
                        {
                            scene.prop_instances.push(PropInstance {
                                article_index: idx,
                                triggerable: definitions
                                    .as_ref()
                                    .ok()
                                    .and_then(|defs| defs.find_tag_or_first(ge.fourcc))
                                    .is_some_and(|definition| definition.triggerable),
                                position: to_scene_coordinates(ge.position),
                                // GEOM v4 consumer multiplies the basis by the
                                // signed short percentage (EXE 0x46a644..0x46a7e9).
                                rotation: ge.rotation.map(|row| {
                                    row.map(|value| {
                                        value
                                            * ge.uniform_scale_percent
                                                .map_or(1.0, |p| p as f32 * 0.01)
                                    })
                                }),
                            });
                            matched += 1;
                        } else {
                            unmatched += 1;
                        }
                    }
                    scene.diagnostics.push(format!(
                        "SCN {scn_key}: {} geom elements, {matched} matched, {unmatched} unmatched",
                        scn.geom_elements.len(),
                    ));
                }
                Err(e) => {
                    scene
                        .diagnostics
                        .push(format!("SCN {scn_key}: parse error: {e}"));
                }
            }
        }
    }

    // --- Sun from the sky config Sky/<track>.bin ---
    let sky_bin = files.iter().find(|(k, _)| {
        let low = k.replace('\\', "/").to_ascii_lowercase();
        (low.contains("/sky/") || low.starts_with("sky/"))
            && !low.contains("sky new")
            && low.rsplit('/').next() == Some(&format!("{}.bin", track.to_ascii_lowercase()))
    });
    if let Some((key, data)) = sky_bin {
        match nfs_formats::parse_sky_sun(data) {
            Ok(sun) => scene.sun_position = Some(to_scene_coordinates(sun.position())),
            Err(e) => scene.diagnostics.push(format!("Sky {key}: {e}")),
        }
    }

    // --- Sky texture loading ---
    let sky_fsh_key = files
        .keys()
        .find(|k| {
            let low = k.to_ascii_lowercase();
            (low.contains("/sky/") || low.starts_with("sky/"))
                && !low.contains("sky new")
                && low.ends_with(".fsh")
                && {
                    let stem = low
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .strip_suffix(".fsh")
                        .unwrap_or("");
                    stem.eq_ignore_ascii_case(track)
                }
        })
        .cloned();

    if let Some(sky_key) = sky_fsh_key {
        if let Some(sky_data) = files.get(&sky_key) {
            match parse_fsh(sky_data) {
                Ok(sky_images) => {
                    // Region A sky map and cloud map (`st1a`, `sc1a` in Sky/<track>.txt)
                    // and the horizon haze strip.
                    for (slot, prefix) in ["st", "sc", "hzcl"].iter().enumerate() {
                        scene.sky_layers[slot] = sky_images
                            .iter()
                            .find(|img| img.name.starts_with(prefix))
                            .map(|img| SkyTexture {
                                width: img.width,
                                height: img.height,
                                rgba: img.rgba.clone(),
                            });
                    }
                    // Look for the horizon panorama texture ("horz")
                    if let Some(horz) = sky_images.iter().find(|img| img.name == "horz") {
                        scene.sky_texture = Some(SkyTexture {
                            width: horz.width,
                            height: horz.height,
                            rgba: horz.rgba.clone(),
                        });
                        scene.diagnostics.push(format!(
                            "Sky {sky_key}: loaded horizon {}x{}",
                            horz.width, horz.height,
                        ));
                    } else {
                        scene.diagnostics.push(format!(
                            "Sky {sky_key}: {} images but no 'horz' found",
                            sky_images.len(),
                        ));
                    }
                }
                Err(e) => {
                    scene
                        .diagnostics
                        .push(format!("Sky {sky_key}: parse error: {e}"));
                }
            }
        }
    }

    // --- Track topology loading (.edg, .jnc, .map, .lsp) ---
    let edg_key = format!("{track}.edg");
    let jnc_key = format!("{track}.jnc");
    let map_key = format!("{track}.map");
    let lsp0_key = format!("{track}0.lsp");
    let lsp_key = format!("{track}.lsp");

    let edg_data = find(files, &edg_key).ok();
    let jnc_data = find(files, &jnc_key).ok();
    let map_data = find(files, &map_key).ok();
    let lsp_data = find(files, &lsp0_key)
        .or_else(|_| find(files, &lsp_key))
        .ok();

    if edg_data.is_some() || jnc_data.is_some() || map_data.is_some() || lsp_data.is_some() {
        let mut topology = TrackTopology::default();

        if let Some(data) = edg_data {
            match parse_edg(data) {
                Ok(edg) => {
                    for seg in &edg.segments {
                        topology.edges.push(TopologyEdge {
                            flags: seg.flags,
                            p1: to_scene_coordinates(seg.p1),
                            p2: to_scene_coordinates(seg.p2),
                        });
                    }
                    topology.boundary_lines = assemble_boundary_lines(&topology.edges);
                    scene.diagnostics.push(format!(
                        "EDG {edg_key}: {} segments, {} boundary lines",
                        topology.edges.len(),
                        topology.boundary_lines.len()
                    ));
                }
                Err(e) => {
                    scene
                        .diagnostics
                        .push(format!("EDG {edg_key}: parse error: {e}"));
                }
            }
        }

        if let Some(data) = jnc_data {
            match parse_jnc(data) {
                Ok(jnc) => {
                    topology.junctions = jnc.records;
                    scene.diagnostics.push(format!(
                        "JNC {jnc_key}: {} junction records",
                        topology.junctions.len()
                    ));
                }
                Err(e) => {
                    scene
                        .diagnostics
                        .push(format!("JNC {jnc_key}: parse error: {e}"));
                }
            }
        }

        if let Some(data) = map_data {
            match parse_map(data) {
                Ok(map) => {
                    let sec_count = map.sections.len();
                    topology.map = Some(map);
                    scene
                        .diagnostics
                        .push(format!("MAP {map_key}: {sec_count} sections"));
                }
                Err(e) => {
                    scene
                        .diagnostics
                        .push(format!("MAP {map_key}: parse error: {e}"));
                }
            }
        }

        if let Some(data) = lsp_data {
            match parse_lsp(data) {
                Ok(lsp) => {
                    let pts_count = lsp.points.len();
                    if pts_count >= 3 {
                        let p_first = &lsp.points[0];
                        let p_last = &lsp.points[pts_count - 1];
                        let gap = ((p_first.x - p_last.x).powi(2) + (p_first.z - p_last.z).powi(2))
                            .sqrt();
                        let is_circuit = gap < 60.0;
                        if let Ok(course) =
                            TrackCourse::from_spline_points(&lsp.points, is_circuit, |x, z| {
                                scene
                                    .road_surface
                                    .as_ref()
                                    .and_then(|s| s.query(x, z, 0.0, 500.0, 500.0))
                                    .map(|h| h.height)
                                    .unwrap_or(0.0)
                            })
                        {
                            scene.diagnostics.push(format!(
                                "Course: {} waypoints, {} checkpoints, len {:.1}m ({})",
                                course.waypoints.len(),
                                course.checkpoints.len(),
                                course.total_length,
                                if is_circuit { "Circuit" } else { "Sprint" }
                            ));
                            scene.course = Some(course);
                        }
                    }
                    topology.spline = Some(lsp);
                    scene
                        .diagnostics
                        .push(format!("LSP: {pts_count} waypoints"));
                }
                Err(e) => {
                    scene.diagnostics.push(format!("LSP parse error: {e}"));
                }
            }
        }

        scene.topology = Some(topology);
        // The original AI: node graph from the CRP and the forward racing line.
        if let Ok(lsp0) = find(files, &lsp0_key) {
            scene.ai_track = crate::race::original_ai::field::OriginalAiTrack::from_files(
                track, false, crp_data, jnc_data, lsp0,
            );
        }
    }

    scene.diagnostics.push(format!(
        "Props: {} articles, {} instances; Sky: {}; Topology: {}",
        scene.prop_articles.len(),
        scene.prop_instances.len(),
        if scene.sky_texture.is_some() {
            "loaded"
        } else {
            "none"
        },
        if let Some(top) = &scene.topology {
            format!(
                "{} edges, {} lines",
                top.edges.len(),
                top.boundary_lines.len()
            )
        } else {
            "none".to_string()
        }
    ));

    Ok(scene)
}

/// A `df` colour (D3D ARGB word, bytes B, G, R, A) as RGBA 0..1.
///
/// Evidence for the byte order: the original packs byte +2 into the high five
/// bits of the RGB555 road colour (0x475574..0x47559d), which `Road_SampleColour`
/// 0x474700 returns as red; `LayerIndexedColour::Apply` 0x4499e0 copies the words
/// unchanged into the vertex colour, the same slot that gets the glare colours,
/// where the brake light word 0xffff0000 is red.
fn df_rgba(px: &[u8]) -> [f32; 4] {
    [px[2], px[1], px[0], px[3]].map(|b| f32::from(b) / 255.0)
}

fn parse_mesh(
    pr: &Entry,
    df: Option<&Entry>,
    positions: &[[f32; 3]],
    uv: Option<&Entry>,
    materials: &BTreeMap<u16, usize>,
    name: String,
) -> Result<Mesh, String> {
    let d = &pr.data;
    if d.len() < 48 {
        return Err("pr entry data too short".into());
    }
    let prim_type = u32le(d, 0)? & 0xffff;
    let mat_id = u16le(d, 4)?;
    let material = *materials
        .get(&mat_id)
        .ok_or_else(|| format!("missing referenced material {mat_id}"))?;
    let ni = u32le(d, 40)? as usize;
    let nd = u32le(d, 44)? as usize;
    if ni > 64 || nd > 16 {
        return Err("unreasonable part rows in pr".into());
    }
    let ib = 48 + ni * 16;
    let data_offset = ib + nd * 8;
    let raw = bytes(
        d,
        data_offset,
        nd.checked_mul(pr.count).ok_or("index size overflow")?,
    )?;

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
        // Kinds 0/1/2/3 index vt/nm/uv/df (df: 4-byte ARGB colours).
        if kind <= 3 {
            let adjust = u32le(d, o + 4)? as usize;
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
            let stride = match kind {
                2 => 8,
                3 => 4,
                _ => 16,
            };
            if !adjust.is_multiple_of(stride) {
                return Err("unaligned index base".into());
            }
            channels.insert(kind, (offset, adjust / stride));
        }
    }

    let (vo, va) = *channels.get(&0).ok_or("missing vertex index stream")?;
    let mut vertices = Vec::with_capacity(pr.count);
    let mut raw_v_indices = Vec::with_capacity(pr.count);

    for i in 0..pr.count {
        let ix = vo.map_or(i, |off| raw[off + i] as usize) + va;
        let position = *positions
            .get(ix)
            .ok_or_else(|| format!("vertex index {ix} out of bounds ({})", positions.len()))?;

        let mut tex = [0.0; 2];
        if let (Some(u), Some(&(off, adj))) = (uv, channels.get(&2)) {
            let u_ix = off.map_or(i, |off| raw[off + i] as usize) + adj;
            if u.data.len() >= (u_ix + 1) * 8 {
                tex[0] = f32le(&u.data, u_ix * 8)?;
                tex[1] = f32le(&u.data, u_ix * 8 + 4)?;
            }
        }

        // `df` colours are D3D ARGB words (bytes B, G, R, A).
        let mut color = [1.0; 4];
        if let (Some(df), Some(&(off, adj))) = (df, channels.get(&3)) {
            let c_ix = off.map_or(i, |off| raw[off + i] as usize) + adj;
            if let Some(px) = df.data.get(c_ix * 4..c_ix * 4 + 4) {
                color = df_rgba(px);
            }
        }

        raw_v_indices.push(ix);
        vertices.push(Vertex {
            position,
            normal: [0.0, 0.0, 0.0],
            uv: tex,
            color,
        });
    }

    let mut indices = Vec::new();
    match prim_type {
        3 => {
            if !pr.count.is_multiple_of(3) {
                return Err("triangle index count not divisible by three".into());
            }
            indices.reserve(pr.count);
            for i in 0..pr.count as u32 {
                indices.push(i);
            }
        }
        4 => {
            if !pr.count.is_multiple_of(4) {
                return Err("quad index count not divisible by four".into());
            }
            let n_quads = pr.count / 4;
            indices.reserve(n_quads * 6);
            for q in 0..n_quads as u32 {
                let base = q * 4;
                indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        1 => {
            if pr.count >= 3 {
                for i in 0..pr.count - 2 {
                    let (a, b, c) = if i % 2 == 0 {
                        (i, i + 1, i + 2)
                    } else {
                        (i + 1, i, i + 2)
                    };
                    let va = raw_v_indices[a];
                    let vb = raw_v_indices[b];
                    let vc = raw_v_indices[c];
                    if va != vb && vb != vc && va != vc {
                        indices.extend([a as u32, b as u32, c as u32]);
                    }
                }
            }
        }
        other => return Err(format!("unsupported primitive type {other:#x}")),
    }

    // Accumulate normals across triangles
    for tri in indices.as_chunks::<3>().0 {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let n = triangle_normal([
            vertices[i0].position,
            vertices[i1].position,
            vertices[i2].position,
        ]);
        for &idx in &[i0, i1, i2] {
            vertices[idx].normal[0] += n[0];
            vertices[idx].normal[1] += n[1];
            vertices[idx].normal[2] += n[2];
        }
    }
    for v in &mut vertices {
        let norm = normalize(v.normal);
        v.normal = if norm == [0.0; 3] {
            [0.0, 1.0, 0.0]
        } else {
            norm
        };
    }

    Ok(Mesh {
        name,
        vertices,
        indices,
        material,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn df_words_are_argb() {
        // Alps road RD1136C: bytes 99 6b 4b ff = ARGB 0xff4b6b99.
        let c = df_rgba(&[0x99, 0x6b, 0x4b, 0xff]);
        assert_eq!(c, [0x4b as f32 / 255.0, 0x6b as f32 / 255.0, 0x99 as f32 / 255.0, 1.0]);
        // Brake glare colour word 0xffff0000 (bytes 00 00 ff ff) is red.
        assert_eq!(df_rgba(&[0, 0, 0xff, 0xff]), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn local_skidpad_loads_to_valid_scene() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let crp_path = root.join("skidpad.crp");
        let fsh_path = root.join("skidpad.fsh");
        if !crp_path.exists() || !fsh_path.exists() {
            return;
        }

        let mut files = AssetFiles::new();
        files.insert("skidpad.crp".into(), std::fs::read(&crp_path).unwrap());
        files.insert("skidpad.fsh".into(), std::fs::read(&fsh_path).unwrap());
        files.insert(
            "animdefs.txt".into(),
            std::fs::read(root.join("animdefs.txt")).unwrap(),
        );

        let sky = std::fs::read(root.join("Sky/Skidpad.bin")).ok();
        if let Some(sky) = &sky {
            files.insert("GameData/Track/Sky/Skidpad.bin".into(), sky.clone());
        }
        let scene = load(&files, "skidpad").unwrap();
        // Skidpad.bin: a = -122.1, theta = 0.65 turn, h = 115.5, s = 0.537.
        assert_eq!(scene.sun_position.is_some(), sky.is_some());
        if let Some(sun) = scene.sun_position {
            let elevation = sun[1].atan2(sun[0].hypot(sun[2])).to_degrees();
            assert!((40.0..45.0).contains(&elevation), "sun elevation {elevation}");
        }
        // Prelit `df` colours reach the vertices (pr channel kind 3). Skidpad's
        // colours are almost all white; only 90 entries are shaded.
        let colors: Vec<[f32; 4]> = scene
            .meshes
            .iter()
            .flat_map(|m| m.vertices.iter().map(|v| v.color))
            .collect();
        let shaded = colors.iter().filter(|c| c[..3] != [1.0; 3]).count();
        assert!((60..1000).contains(&shaded), "{shaded} of {} shaded", colors.len());
        assert!(scene.materials.iter().all(|m| m.prelit));
        assert_eq!(scene.meshes.len(), 180);
        assert_eq!(scene.prop_articles.len(), 70);
        assert_eq!(scene.textures.len(), 98);
        assert_eq!(scene.materials.len(), 107);
        let static_end = scene
            .prop_articles
            .first()
            .map(|p| p.mesh_range.start)
            .unwrap_or(scene.meshes.len());
        assert_eq!(static_end, 46);
        let static_triangles: usize = scene.meshes[..static_end]
            .iter()
            .map(|m| m.indices.len() / 3)
            .sum();
        assert_eq!(static_triangles, 6848);
        assert!(scene.bounds[0][0] < -200.0);
        assert!(scene.bounds[1][0] > 200.0);
        let surface = scene.road_surface.as_ref().unwrap();
        assert!(surface.triangle_count() > 0, "no original support polygons");
        assert!(surface.uses_original_support());
        assert_eq!(
            surface.report().accepted,
            surface.original_support_counts().unwrap().0
        );
    }

    #[test]
    fn local_skidpad_loads_with_scenario_and_sky() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let crp_path = root.join("skidpad.crp");
        let fsh_path = root.join("skidpad.fsh");
        let scn_path = root.join("skidpad_st1.scn");
        let sky_path = root.join("Sky/Skidpad.fsh");
        if !crp_path.exists() || !fsh_path.exists() || !scn_path.exists() || !sky_path.exists() {
            return;
        }

        let mut files = AssetFiles::new();
        files.insert("skidpad.crp".into(), std::fs::read(&crp_path).unwrap());
        files.insert("skidpad.fsh".into(), std::fs::read(&fsh_path).unwrap());
        files.insert("skidpad_st1.scn".into(), std::fs::read(&scn_path).unwrap());
        files.insert(
            "animdefs.txt".into(),
            std::fs::read(root.join("animdefs.txt")).unwrap(),
        );
        files.insert("sky/skidpad.fsh".into(), std::fs::read(&sky_path).unwrap());

        let scene = load(&files, "skidpad").unwrap();
        assert_eq!(scene.prop_articles.len(), 70);
        assert_eq!(scene.prop_instances.len(), 11);
        assert_eq!(
            scene
                .prop_instances
                .iter()
                .filter(|p| p.triggerable)
                .count(),
            5
        );
        // Original v4 arrows use percentages; the final L A stays at 100%.
        // Check the full basis and translation against the source SCN so scale
        // cannot accidentally affect placement or only one rotation axis.
        let source =
            nfs_formats::scn::parse_scn(&std::fs::read_to_string(&scn_path).unwrap()).unwrap();
        let arrows: Vec<_> = source
            .geom_elements
            .iter()
            .filter(|g| g.flags == 1)
            .collect();
        assert_eq!(
            arrows
                .iter()
                .map(|g| g.uniform_scale_percent.unwrap())
                .collect::<Vec<_>>(),
            [41, 49, 55, 42, 100]
        );
        for arrow in arrows {
            let position = to_scene_coordinates(arrow.position);
            let instance = scene
                .prop_instances
                .iter()
                .find(|p| p.position == position)
                .unwrap();
            let scale = arrow.uniform_scale_percent.unwrap() as f32 * 0.01;
            for row in 0..3 {
                for col in 0..3 {
                    assert!(
                        (instance.rotation[row][col] - arrow.rotation[row][col] * scale).abs()
                            < 1e-6
                    );
                }
            }
        }
        let start = scene.scenario_start.expect("Skidpad st1 Start trigger");
        assert_eq!(
            start.linked_prop.unwrap().position,
            [-3.916403, 4.161158, -110.107_32]
        );
        assert_eq!(
            scene
                .scenario_gates
                .iter()
                .map(|gate| gate.linked_prop.map(|p| p.position[2]))
                .collect::<Vec<_>>(),
            [
                Some(-59.33183),
                Some(-4.083225),
                Some(79.31392),
                None,
                Some(-150.21165),
                None
            ]
        );
        assert_eq!(start.position, [-6.844955, 0.0, -147.240_66]);
        assert_eq!(start.forward, [0.0, 0.0, 1.0]);
        assert_eq!(scene.scenario_end, Some([-6.805335, 0.0, -144.493_32]));
        assert_eq!(scene.scenario_name.as_deref(), Some("skidpad_st1.scn"));
        assert_eq!(
            scene
                .scenario_gates
                .iter()
                .map(|g| g.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(scene.scenario_gates.last().unwrap().is_end);
        assert_eq!(scene.scenario_gates.last().unwrap().width, 8.5);
        let mut route =
            crate::scenario::ScenarioProgress::new(scene.scenario_gates.clone(), start.position)
                .expect("0M01 has an ordered source route ending at End");
        route.observe(&crate::scenario::TriggerVehicle {
            position: glam::Vec3::from(scene.scenario_end.unwrap()),
            forward: glam::Vec3::Z,
            right: glam::Vec3::X,
            up: glam::Vec3::Y,
            half_extents: glam::Vec3::new(0.9, 0.6, 2.2),
            velocity: glam::Vec3::ZERO,
            expanded: false,
        });
        assert!(
            !route.complete(),
            "End reached before waypoints must not pass"
        );
        let support = scene
            .road_surface
            .as_ref()
            .unwrap()
            .query(start.position[0], start.position[2], 0.0, 25.0, 50.0)
            .expect("SCN Start has static MESH04 support");
        assert_eq!(support.identity.article_name, "MESH04");
        assert_eq!(support.height, 0.0);
        // Continuous support along every source gate center, including both
        // sides of sector boundaries. The old MESH04-only selection fell through
        // immediately after gate 1 in a real keyboard playthrough.
        let surface = scene.road_surface.as_ref().unwrap();
        let mut previous = start.position;
        for gate in &scene.scenario_gates {
            for step in 0..=100 {
                let t = step as f32 / 100.0;
                let x = previous[0] + (gate.center[0] - previous[0]) * t;
                let z = previous[2] + (gate.center[2] - previous[2]) * t;
                for dx in [-1.0, 0.0, 1.0] {
                    let hit = surface.query(x + dx, z, 0.0, 0.1, 0.1).unwrap_or_else(|| {
                        panic!("missing pad before gate {} at {x}, {z}", gate.sequence)
                    });
                    assert!(hit.height.abs() < 0.001);
                }
            }
            previous = gate.center;
        }
        assert_eq!(
            surface
                .query(1.45, -102.99, 0.0, 0.1, 0.1)
                .expect("attempt-1 fall location must have source support")
                .identity
                .article_name,
            "MESH18"
        );
        assert!(
            !surface
                .triangles()
                .any(|(id, _, _)| id.article_name == "MESH04" && id.article_index == 52),
            "same-named building must not become road support"
        );
        assert!(
            surface.query(180.0, 180.0, 0.0, 0.1, 0.1).is_none(),
            "pad bounds must not become a rectangular phantom floor"
        );
        assert!(scene.sky_texture.is_some());
        let sky = scene.sky_texture.as_ref().unwrap();
        assert_eq!(sky.width, 256);
        assert_eq!(sky.height, 256);
    }

    #[test]
    fn missing_track_file_reported() {
        let files = AssetFiles::new();
        let err = load(&files, "skidpad").unwrap_err();
        assert!(err.contains("missing resource skidpad.crp"));
    }

    #[test]
    fn audit_all_tracks_magenta() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        if !root.exists() {
            return;
        }
        for track_name in &["farmland", "skidpad"] {
            let path = root.join(format!("{track_name}.crp"));
            let fsh_path = root.join(format!("{track_name}.fsh"));
            let mut files = AssetFiles::new();
            files.insert(
                "animdefs.txt".into(),
                std::fs::read(root.join("animdefs.txt")).unwrap(),
            );
            files.insert(format!("{track_name}.crp"), std::fs::read(&path).unwrap());
            files.insert(
                format!("{track_name}.fsh"),
                std::fs::read(&fsh_path).unwrap(),
            );
            let scene = match load(&files, track_name) {
                Ok(s) => s,
                Err(e) => {
                    println!("TRACK FAIL: {track_name}: {e}");
                    continue;
                }
            };
            println!(
                "=== Track {track_name} (textures: {}) ===",
                scene.textures.len()
            );
            let fsh_bytes = files.get(&format!("{track_name}.fsh")).unwrap();
            let fsh = nfs_formats::parse_fsh(fsh_bytes).unwrap();
            for img in &fsh {
                if ["fi03", "eur1", "wall", "hs09"].contains(&img.name.as_str()) {
                    let _n = img.width as usize * img.height as usize;
                    let p0 = &img.rgba[..16];
                    println!(
                        "  FSH image '{}' ({}x{}): sample={:?}",
                        img.name, img.width, img.height, p0
                    );
                }
            }
        }
    }

    #[test]
    fn local_skidpad_loads_with_topology() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let crp_path = root.join("skidpad.crp");
        let fsh_path = root.join("skidpad.fsh");
        let edg_path = root.join("skidpad.edg");
        let jnc_path = root.join("skidpad.jnc");
        let map_path = root.join("skidpad.map");
        let lsp_path = root.join("racer/work/skidpad0.lsp");

        if !crp_path.exists() || !fsh_path.exists() || !edg_path.exists() {
            return;
        }

        let mut files = AssetFiles::new();
        files.insert("skidpad.crp".into(), std::fs::read(&crp_path).unwrap());
        files.insert("skidpad.fsh".into(), std::fs::read(&fsh_path).unwrap());
        files.insert("skidpad.edg".into(), std::fs::read(&edg_path).unwrap());
        files.insert(
            "animdefs.txt".into(),
            std::fs::read(root.join("animdefs.txt")).unwrap(),
        );
        files.insert("skidpad.jnc".into(), std::fs::read(&jnc_path).unwrap());
        files.insert("skidpad.map".into(), std::fs::read(&map_path).unwrap());
        if lsp_path.exists() {
            files.insert("skidpad0.lsp".into(), std::fs::read(&lsp_path).unwrap());
        }

        let scene = load(&files, "skidpad").unwrap();
        assert!(scene.topology.is_some());
        let top = scene.topology.as_ref().unwrap();
        assert_eq!(top.edges.len(), 167);
        assert!(!top.boundary_lines.is_empty());
        assert_eq!(top.junctions.len(), 1);
        assert!(top.map.is_some());
        if lsp_path.exists() {
            assert!(top.spline.is_some());
            assert!(scene.course.is_some());
            let course = scene.course.as_ref().unwrap();
            assert_eq!(course.waypoints.len(), 234);
            assert!(course.is_circuit);
            assert!(!course.checkpoints.is_empty());
        }
    }
}

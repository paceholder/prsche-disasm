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

/// `ef` entries of type 3 of the static (non-object) articles.
fn particle_emitters(
    crp: &nfs_formats::TrackCrp,
    object_articles: &std::collections::BTreeSet<usize>,
) -> Vec<crate::particles::TrackEmitter> {
    let mut out = Vec::new();
    for (ai, a) in crp.articles.iter().enumerate() {
        // Library articles (Base bit 0x8000) are templates, not placed.
        let library = a
            .children
            .iter()
            .find(|c| c.tag == "Base")
            .and_then(|b| b.data.get(0..4))
            .is_some_and(|w| u32::from_le_bytes(w.try_into().unwrap()) & 0x8000 != 0);
        if library || object_articles.contains(&ai) {
            continue;
        }
        for c in a.children.iter().filter(|c| c.tag == "ef") {
            if let Some(e) = crate::particles::parse_ef_emitter(&c.data, ai) {
                out.push(e);
            }
        }
    }
    out
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

/// `Anqt` frames of a library article: 52 bytes each, quaternion (x, y, z, w),
/// position, four floats and two words (`docs/re/track-objects.md` section 6).
fn anqt_frames(article: &nfs_formats::Entry) -> Vec<PropFrame> {
    let Some(anqt) = article.children.iter().find(|c| c.tag == "Anqt") else {
        return Vec::new();
    };
    anqt.data
        .chunks_exact(52)
        .map(|f| {
            let g = |i: usize| f32::from_le_bytes(f[i * 4..i * 4 + 4].try_into().unwrap());
            PropFrame {
                rotation: [g(0), g(1), g(2), g(3)],
                position: [g(4), g(5), g(6)],
            }
        })
        .collect()
}

/// Scroll speed of a `UVAnimate` material: the first layer of the `mt` record whose parameters
/// have mode 0 (`LayerUVAnimate::Apply` 0x448cc0). The record is `0x20` bytes of header, then
/// `ntex` texture entries of `0x2c`, then `nlayers` headers `{type, offset, size, _}` of 16
/// bytes, then the parameter blob (loader 0x44a770). Modes 1 (zero steps in every shipped
/// track) and 2 (sway, shadows) are not animated (A191).
fn uv_animate_speed(mt: &[u8]) -> Option<i32> {
    let counts = u32le(mt, 8).ok()?;
    let (ntex, nlayers) = ((counts & 0xffff) as usize, (counts >> 16) as usize);
    let headers = 0x20 + ntex * 0x2c;
    let blob = headers + nlayers * 16;
    for i in 0..nlayers {
        let h = headers + i * 16;
        let (ty, off, size) = (u32le(mt, h).ok()?, u32le(mt, h + 4).ok()? as usize, u32le(mt, h + 8).ok()?);
        if !(4..=6).contains(&ty) || size < 8 {
            continue;
        }
        let p = blob + off;
        if u32le(mt, p).ok()? == 0 {
            return Some(u32le(mt, p + 4).ok()? as i32).filter(|s| *s != 0);
        }
    }
    None
}

/// Constant vertex colour of a track material whose render method keeps a `LayerAmbient`
/// layer (`RCustomLayers::LayerAmbient`, entry type 0x8007, apply 0x448c70: writes its
/// parameter word into every vertex colour). The scene loader 0x44a770 builds a material's
/// layers from the `RMth` chunk named at `mt +0x10`; `0x44bf70` keeps an entry when its mask
/// is 0 or shares a bit with the `mt +0x0c` flags. Track methods keep either this layer
/// (flag 0x10: objects, signs) or the `df` data layer (entry type 6, flag 8: road, cliffs);
/// the draw then copies the `df` words unchanged (0x540ce0). `RMth` layout: name (16),
/// group, entry and parameter counts, 16-byte groups, 16-byte entries
/// `{u16 pass, tag, type, arg, mask}`, 16-byte parameter descriptors `{u16 pass, tag,
/// offset, size, _}`, then the default values. Parameter overrides in the `mt` record are
/// not read (shipped tracks override only the render state).
fn ambient_layer_colour(rmth: &[u8], flags: u32) -> Option<[f32; 4]> {
    let (groups, entries, params) = (
        u32le(rmth, 0x10).ok()? as usize,
        u32le(rmth, 0x14).ok()? as usize,
        u32le(rmth, 0x18).ok()? as usize,
    );
    let entry_base = 0x1c + groups.checked_mul(16)?;
    let param_base = entry_base + entries.checked_mul(16)?;
    let blob = param_base + params.checked_mul(16)?;
    // `LayerFullAmbient` (type 0x8005, `AF`; apply 0x448c30): fills the vertex colour table with
    // 0xffffffff once, and the `FullAmbient` methods have no `fd` data layer, so their vertices
    // draw white (texture unmodulated, alpha from the texture). Read, not recorded.
    let full_ambient = (0..entries).any(|i| {
        let e = entry_base + i * 16;
        u32le(rmth, e + 4).ok() == Some(0x8005)
            && u32le(rmth, e + 12).ok().is_some_and(|mask| mask == 0 || flags & mask != 0)
    });
    if full_ambient {
        return Some([1.0; 4]);
    }
    let tag = (0..entries).find_map(|i| {
        let e = entry_base + i * 16;
        let (ty, mask) = (u32le(rmth, e + 4).ok()?, u32le(rmth, e + 12).ok()?);
        (ty == 0x8007 && (mask == 0 || flags & mask != 0)).then(|| bytes(rmth, e, 4).ok())?
    })?;
    (0..params).find_map(|i| {
        let p = param_base + i * 16;
        if bytes(rmth, p, 4).ok()? != tag || u32le(rmth, p + 8).ok()? != 4 {
            return None;
        }
        let off = blob + u32le(rmth, p + 4).ok()? as usize;
        Some(df_rgba(bytes(rmth, off, 4).ok()?))
    })
}

/// Tag word of the `sR` render-state layer in `RMth` layer definitions and its key in the
/// sorted key table (type 8, vtable 0x5b69bc → `Render_ApplyLayerState` 0x540660).
const RENDER_STATE_KEY: u32 = 0x5273_0001;

/// A track `RMth` record as `RenderMethod` construction 0x44bf70 reads it: the sorted, unique
/// tag words of its layer definitions (an `mt` layer index points into this table, 0x44a220)
/// and the method's default `sR` block `{shade, blend enable, blend mode, cull, z bias, -1}`.
struct RenderMethod<'a> {
    /// The whole record (also read by `ambient_layer_colour`).
    data: &'a [u8],
    keys: Vec<u32>,
    state: Option<[i32; 6]>,
}

/// `RMth` layout after the 16-byte name: pass count, layer definition count, default count;
/// pass records (16 bytes) from +0x1c, then layer definitions `{tag, type, param, mask}`, then
/// defaults `{key, offset, size, _}`, then the default data blob [VA 0x44bf70].
fn render_methods(misc: &[Entry]) -> BTreeMap<String, RenderMethod<'_>> {
    let mut out = BTreeMap::new();
    for e in misc.iter().filter(|e| e.tag == "RMth") {
        let d: &[u8] = &e.data;
        let parse = || -> Option<(String, RenderMethod<'_>)> {
            let name = c_string(d.get(..16)?);
            let (passes, defs, defaults) =
                (u32le(d, 0x10).ok()? as usize, u32le(d, 0x14).ok()? as usize, u32le(d, 0x18).ok()? as usize);
            let def_base = 0x1c + passes * 16;
            let mut keys = (0..defs)
                .map(|i| u32le(d, def_base + i * 16).ok())
                .collect::<Option<Vec<_>>>()?;
            keys.sort_unstable();
            keys.dedup();
            let default_base = def_base + defs * 16;
            let blob = default_base + defaults * 16;
            let mut state = None;
            for i in 0..defaults {
                let h = default_base + i * 16;
                let (key, off, size) =
                    (u32le(d, h).ok()?, u32le(d, h + 4).ok()? as usize, u32le(d, h + 8).ok()?);
                if key == RENDER_STATE_KEY && size == 24 {
                    state = Some(state_block(d, blob + off)?);
                }
            }
            Some((name, RenderMethod { data: d, keys, state }))
        };
        if let Some((name, method)) = parse() {
            out.insert(name, method);
        }
    }
    out
}

fn state_block(d: &[u8], at: usize) -> Option<[i32; 6]> {
    let mut s = [0; 6];
    for (k, v) in s.iter_mut().enumerate() {
        *v = u32le(d, at + k * 4).ok()? as i32;
    }
    Some(s)
}

/// The `sR` block a material draws with: an `mt` layer whose index names the method's `sR`
/// key replaces the method default (material loader 0x44a770; layer headers as in
/// `uv_animate_speed`).
fn material_render_state(mt: &[u8], method: Option<&RenderMethod<'_>>) -> Option<[i32; 6]> {
    let method = method?;
    let mut state = method.state;
    let counts = u32le(mt, 8).ok()?;
    let (ntex, nlayers) = ((counts & 0xffff) as usize, (counts >> 16) as usize);
    let headers = 0x20 + ntex * 0x2c;
    let blob = headers + nlayers * 16;
    for i in 0..nlayers {
        let h = headers + i * 16;
        let (index, off, size) =
            (u32le(mt, h).ok()? as usize, u32le(mt, h + 4).ok()? as usize, u32le(mt, h + 8).ok()?);
        if method.keys.get(index) == Some(&RENDER_STATE_KEY) && size == 24 {
            state = Some(state_block(mt, blob + off)?);
        }
    }
    state
}

/// Whether a track material is drawn without back-face culling: its `sR` block through
/// `Render_ApplyLayerState` 0x540660 and the THRASH driver ends with `D3DRS_CULLMODE =
/// D3DCULL_NONE` (cull field 1 selects the view's mode, `[[0x69ed0c]+4]+0x10`, any other
/// value none [VA 0x540590]); recorded, `render_state::draw_state`. Every track method
/// default has 1; the `MatState` materials (posts, bushes, wires) override it with 0. Track
/// masks never set 0x20, so the per-primitive plane test of 0x449390 (its `$n` plane layer,
/// tag 0x8000) never runs on track geometry.
fn track_material_double_sided(state: Option<[i32; 6]>, render_method: &str) -> bool {
    match state {
        Some(s) => crate::render_state::draw_state(&s).double_sided,
        None => render_method != "Opaque",
    }
}

/// Preserve the pre-PR26 render-method classification. Alpha distribution alone does
/// not prove the original method registry's blend state (`FUN_0044a770`, A190).
/// `S_UVAnimate` is the shadow method of the tracks (`sh80`, sway layer mode 2,
/// `docs/re/water.md`): its `sR` block enables blending (read from the `RMth` data, not
/// recorded), so the texture alpha is a soft mask, not a cut-out. Behind
/// `experimental-original-port` (evidence gate).
fn track_shadow_method(render_method: &str) -> bool {
    cfg!(feature = "experimental-original-port") && render_method == "S_UVAnimate"
}

/// A texture whose alpha is a soft sheet (sea `wtr3`/`wtr4`, shore foam `seaa`/`seaf..h`,
/// rivers, waterfalls, some windows): at least half of its texels partly transparent (alpha
/// 9..246) and at most 15 % clear (0..8). Foliage and decals (mostly 0/255 or many clear
/// texels) are not.
fn translucent_sheet(rgba: &[u8]) -> bool {
    let n = rgba.len() / 4;
    let (mut mid, mut clear) = (0usize, 0usize);
    for p in rgba.as_chunks::<4>().0 {
        match p[3] {
            0..=8 => clear += 1,
            9..=246 => mid += 1,
            _ => {}
        }
    }
    n > 0 && mid * 2 >= n && clear * 100 <= n * 15
}

/// The lighthouse beam methods `FullAmbient` / `FullAmbMatState` (`lthl`, `lths`): their `sR`
/// block enables SRCALPHA/INVSRCALPHA blending, so the texture alpha (12..119 on `lths`) is a
/// soft ramp, not a cut-out; as a mask it vanished completely. Read from the `RMth` data, not
/// recorded; behind `experimental-original-port` (A315).
fn track_blend_method(render_method: &str, state: Option<[i32; 6]>) -> bool {
    cfg!(feature = "experimental-original-port")
        && render_method.starts_with("FullAmb")
        && state.is_some_and(|s| {
            crate::render_state::draw_state(&s).blend.is_some_and(|(src, dst)| (src, dst) == (5, 6))
        })
}

/// Alpha mode of a track material. `state` is the material's `sR` block (method default or
/// the `mt` override). With `experimental-original-port`: when that block resolves to
/// `SRCALPHA`/`INVSRCALPHA` blending through `Render_ApplyLayerState` 0x540660 and the dx7
/// driver (recorded, `render_state::draw_state`; every `Normal`, `UVAnimate`, `FakeSort`,
/// `FullAmbient`, `FA_UVAnim` and `S_*` default `{1, 2, 0, 1, z, -1}`), a soft-alpha sheet
/// ([`translucent_sheet`]) is drawn blended. Other textures of those methods stay cut out:
/// for 0/255 alpha the blend gives the same pixels, and our blend path (sorted, no depth
/// write) is not the original's draw order (A190). Without the feature the method name alone
/// decides.
fn track_alpha_mode(render_method: &str, state: Option<[i32; 6]>, rgba: &[u8]) -> AlphaMode {
    let blended_sheet = || {
        cfg!(feature = "experimental-original-port")
            && state.is_some_and(|s| crate::render_state::draw_state(&s).alpha_blended())
            && translucent_sheet(rgba)
    };
    if !rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255) {
        AlphaMode::Opaque
    } else if render_method == "Transparency" || track_shadow_method(render_method)
        || track_blend_method(render_method, state)
        || blended_sheet()
    {
        AlphaMode::Blend
    } else {
        AlphaMode::Mask
    }
}

pub fn load(files: &AssetFiles, track: &str) -> Result<Scene, String> {
    load_direction(files, track, false)
}

/// Loads a track for a race direction. `reverse` is the original's `REVERSE` option
/// `[0x657430]`: the AI graph uses `<track>1.lsp` (`<dir><track><REVERSE>.lsp`, `docs/re/ai.md`),
/// with the node graph walked backwards, and the course (start, laps, finish) runs against the
/// racing-line point order.
pub fn load_direction(files: &AssetFiles, track: &str, reverse: bool) -> Result<Scene, String> {
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
        car_body_points: Vec::new(),
        car_driver_camera: None,
        car_headlight_extent: 0.0,
        car_wheels: Vec::new(),
        car_glares: Vec::new(),
        car_in_car: Vec::new(),
        car_driver_exterior: Vec::new(),
        car_anim_parts: Vec::new(),
        car_roof_up: Vec::new(),
        car_roof_down: Vec::new(),
        effect_textures: Vec::new(),
        particle_defs: Vec::new(),
        particle_emitters: Vec::new(),
        textures: Vec::new(),
        car_texture_tint: Vec::new(),
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
        track_objects: Vec::new(),
        smashables: Vec::new(),
        smack_props: Vec::new(),
        animated_articles: Vec::new(),
        scenario_start: None,
        scenario_end: None,
        scenario_name: None,
        scenario_gates: Vec::new(),
        sky_texture: None,
        sky_layers: [None, None, None],
        sky_horizon: None,
        sky_dome: None,
        sky_map: None,
        sky_map_b: None,
        sky_layers_b: [None, None],
        sky_clouds: [None, None],
        sky_layer_blends: [false, true],
        sky_weather: None,
        sun_position: None,
        flare: None,
        env_textures: Vec::new(),
        env_sections: Vec::new(),
        topology: None,
        road_surface: None,
        course: None,
        ai_track: None,
        start_trigger: None,
        intro_cameras: crp
            .misc
            .iter()
            .filter(|e| e.tag == crate::camera::anim::CMAN_TAG)
            .filter_map(|e| crate::camera::anim::CameraAnimation::parse(&e.data).ok())
            .collect(),
        finish_trigger: None,
        car_damage: Vec::new(),
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
    // Material index -> constant vertex colour of its `LayerAmbient` layer.
    let mut ambient_colour: BTreeMap<usize, [f32; 4]> = BTreeMap::new();
    let mut material_entries: Vec<_> = crp.misc.iter().filter(|e| e.tag == "mt").collect();
    material_entries.sort_by_key(|e| e.index);

    let methods = render_methods(&crp.misc);
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
        let state = material_render_state(&e.data, methods.get(&render_method));
        let alpha_mode = texture.map_or(AlphaMode::Opaque, |t_idx| {
            track_alpha_mode(&render_method, state, &scene.textures[t_idx].rgba)
        });

        let double_sided = track_material_double_sided(state, &render_method);
        let mut depth_bias = if e.data.len() >= 0x118 {
            u32le(&e.data, 0x114).unwrap_or(0) as i32
        } else {
            0
        };
        // The shadow sheet's z bias is the `sR` field 4 of its method (-3 for `S_UVAnimate`),
        // not `mt+0x114` (absent from these records). Read from the data (0x540660 block),
        // behind `experimental-original-port`; unit A72.
        if track_shadow_method(&render_method) {
            depth_bias = state.map_or(depth_bias, |s| s[4]);
        }

        // `LayerAmbient` materials take the layer's constant instead of `df`. Which layers a
        // material keeps (mask rule of `RenderMethod` construction 0x44bf70, layer factory
        // 0x53f230) and the apply 0x448c70 are read, not recorded: behind
        // `experimental-original-port` (evidence gate; the default keeps `df` everywhere, A22).
        let flags = u32le(&e.data, 0x0c).unwrap_or(0);
        if let Some(c) = methods
            .get(&render_method)
            .filter(|_| cfg!(feature = "experimental-original-port"))
            .and_then(|m| ambient_layer_colour(m.data, flags))
        {
            ambient_colour.insert(scene.materials.len(), c);
        }
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
            // Track geometry: texture × vertex colour (`df` or the `LayerAmbient` constant).
            prelit: true,
            uv_scroll: uv_animate_speed(&e.data),
            env_slot: None,
            // The shadow decal texture has alpha 0 on its whole border and the sheet's UVs
            // run far outside 0..1 (-5..15 on canyon): clamped addressing fades each decal out
            // at its border, repeat shows the wrong tile as a hard cut along road polygons.
            clamp_uv: track_shadow_method(&render_method),
        });
    }

    let anim = nfs_formats::animdefs::parse_animdefs(&String::from_utf8_lossy(find(
        files,
        "animdefs.txt",
    )?))?;
    // Collision objects (cylinders, boxes) with their smash definitions. Articles of
    // a smashable object (a definition with pieces) keep their meshes out of the
    // static scene so that the renderer can hide them.
    let track_objects = physics::original_track_object_build::build_initial_track_objects(&crp, &anim)?;
    let smash_article: BTreeMap<usize, usize> = track_objects
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            anim.find_tag_or_first(o.tag)
                .is_some_and(|d| d.tiers.iter().any(|t| !t.pieces.is_empty()))
        })
        .map(|(i, o)| (o.article_index, i))
        .collect();
    let mut smash_meshes: BTreeMap<usize, Vec<Mesh>> = BTreeMap::new();
    // Smackable articles (signs, cones, posts) run as object cars in the original.
    let smack_records = physics::original_smackable::build_initial_smack_records(&crp, &anim)?;
    let smack_article: BTreeMap<usize, usize> = smack_records
        .iter()
        .enumerate()
        .map(|(i, r)| (r.article_index, i))
        .collect();
    let mut smack_meshes: BTreeMap<usize, Vec<Mesh>> = BTreeMap::new();
    // Particle emitters of static articles (0x450de0 → 0x450d10). Objects use
    // the object path 0x450980 (not ported).
    let object_articles: std::collections::BTreeSet<usize> = track_objects
        .iter()
        .map(|o| o.article_index)
        .chain(smack_records.iter().map(|r| r.article_index))
        .collect();
    scene.particle_emitters = particle_emitters(&crp, &object_articles);
    // Animated scenery (`WAnimBundle` 0x4777f0): the scene build 0x4879d0 offers the articles
    // to WSceneBundle 0x482970 and WInstanceBundle 0x4781a0 first, which take every article with
    // a `tr` entry (`0x59f220(0)` = tag 'tr' index 0); WAnimBundle then takes the rest that
    // carry an `Anqt` entry and plays it with a looping `CamAnim` sampler (0.1 s per key)
    // started at the build, i.e. race step 0. Recorded (`replay-lighthouse-anim.py`,
    // `tests/original_scenery_anim.rs`): 0x4777f0 takes every offered `Anqt` article and the
    // player pose for a given `g_SchedStep - start`, bit for bit. Still read: which articles
    // reach the bundle (the `tr` claim of the earlier bundles), the start step and the draw;
    // so behind `experimental-original-port`; without it they stay static at their local
    // coordinates.
    let scenery_articles: BTreeMap<usize, Vec<crate::camera::anim::AnimKey>> = crp
        .articles
        .iter()
        .enumerate()
        .filter(|_| cfg!(feature = "experimental-original-port"))
        .filter(|(_, a)| {
            let base = a.children.iter().find(|c| c.tag == "Base" && c.index == 0);
            let library = base.and_then(|b| u32le(&b.data, 0).ok()).unwrap_or(0) & 0x8000 != 0;
            !library && !a.children.iter().any(|c| c.tag == "tr" && c.index == 0)
        })
        .filter_map(|(i, a)| {
            let anqt = a.children.iter().find(|c| c.tag == "Anqt")?;
            let keys = crate::camera::anim::parse_anqt_keys(&anqt.data);
            (keys.len() >= 3).then_some((i, keys))
        })
        .collect();
    let mut scenery_meshes: BTreeMap<usize, Vec<Mesh>> = BTreeMap::new();
    let mut raw_meshes = Vec::new();
    // Rendering and source collision resources are assembled separately.
    let mut prop_templates: Vec<(u32, Vec<Mesh>, Vec<PropFrame>)> = Vec::new();
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
                                &ambient_colour,
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
                    prop_templates.push((fourcc, prop_meshes, anqt_frames(a)));
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
                &ambient_colour,
                format!("{article_name}#{ai}/{}", pr.index),
            )?;
            if !m.indices.is_empty() && scenery_articles.contains_key(&ai) {
                scenery_meshes.entry(ai).or_default().push(m);
            } else if !m.indices.is_empty() {
                total_tris += m.indices.len() / 3;
                for v in &m.vertices {
                    for k in 0..3 {
                        scene.bounds[0][k] = scene.bounds[0][k].min(v.position[k]);
                        scene.bounds[1][k] = scene.bounds[1][k].max(v.position[k]);
                    }
                }
                if smash_article.contains_key(&ai) {
                    smash_meshes.entry(ai).or_default().push(m);
                } else if smack_article.contains_key(&ai) {
                    smack_meshes.entry(ai).or_default().push(m);
                } else {
                    raw_meshes.push(m);
                }
            }
        }
    }
    scene.diagnostics.push(format!(
        "{} of {} track materials culled by their sR state (0x540590)",
        scene.materials.iter().filter(|m| !m.double_sided).count(),
        scene.materials.len()
    ));

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
    for (fourcc, meshes, frames) in prop_templates {
        let prop_mesh_start = scene.meshes.len();
        scene.meshes.extend(meshes);
        let prop_mesh_end = scene.meshes.len();
        if prop_mesh_end > prop_mesh_start {
            scene.prop_articles.push(PropArticle {
                fourcc,
                mesh_range: prop_mesh_start..prop_mesh_end,
                frames,
            });
        }
    }
    // Smashable crates: intact meshes after the prop templates (the renderer draws
    // them as hideable instances).
    for (&article_index, &object_index) in &smash_article {
        let Some(meshes) = smash_meshes.remove(&article_index) else {
            continue;
        };
        let start = scene.meshes.len();
        scene.meshes.extend(meshes);
        let object = &track_objects[object_index];
        let Some(definition) = anim.find_tag_or_first(object.tag).cloned() else {
            continue;
        };
        let tr = crp.articles[article_index]
            .children
            .iter()
            .find(|c| c.tag == "tr" && c.index == 0)
            .filter(|t| t.data.len() >= 64);
        let mut placement = [0.0f32; 16];
        for (k, v) in placement.iter_mut().enumerate() {
            *v = tr
                .and_then(|t| f32le(&t.data, k * 4).ok())
                .unwrap_or(if k % 5 == 0 { 1.0 } else { 0.0 });
        }
        scene.smashables.push(Smashable {
            object_index,
            definition,
            intact_meshes: start..scene.meshes.len(),
            placement,
        });
    }
    scene.track_objects = track_objects;
    for (&article_index, &record_index) in &smack_article {
        let Some(meshes) = smack_meshes.remove(&article_index) else {
            continue;
        };
        let start = scene.meshes.len();
        scene.meshes.extend(meshes);
        scene.smack_props.push(SmackProp {
            record: smack_records[record_index].clone(),
            meshes: start..scene.meshes.len(),
        });
    }

    for (article_index, keys) in scenery_articles {
        let Some(meshes) = scenery_meshes.remove(&article_index) else {
            continue;
        };
        let start = scene.meshes.len();
        scene.meshes.extend(meshes);
        scene.animated_articles.push(AnimatedArticle {
            name: names[article_index].clone(),
            meshes: start..scene.meshes.len(),
            keys,
        });
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
        match nfs_formats::parse_sky_dome(data) {
            Ok(dome) => scene.sky_dome = Some(dome),
            Err(e) => scene.diagnostics.push(format!("Sky {key}: dome {e}")),
        }
        match nfs_formats::parse_sky_horizon(data) {
            Ok(horizon) => scene.sky_horizon = Some(horizon),
            Err(e) => scene.diagnostics.push(format!("Sky {key}: horizon {e}")),
        }
        match nfs_formats::parse_sky_weather(data) {
            Ok(weather) => scene.sky_weather = Some(weather),
            Err(e) => scene.diagnostics.push(format!("Sky {key}: weather {e}")),
        }
    }

    // --- Sky layer table Sky/<track>.txt (HIGH block, region A) ---
    let sky_txt = files.iter().find(|(k, _)| {
        let low = k.replace('\\', "/").to_ascii_lowercase();
        (low.contains("/sky/") || low.starts_with("sky/"))
            && !low.contains("sky new")
            && low.rsplit('/').next() == Some(&format!("{}.txt", track.to_ascii_lowercase()))
    });
    if let Some((_, data)) = sky_txt {
        let blocks = nfs_formats::parse_sky_txt(&String::from_utf8_lossy(data));
        scene.sky_map = blocks
            .iter()
            .find(|b| b.capability == "HIGH")
            .or(blocks.first())
            .cloned();
        scene.sky_map_b = scene.sky_map.as_ref().and_then(|b| b.region_b.as_deref().cloned());
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
                    // Layer maps are named by the txt (Coastal: `sm1b`); without it the
                    // first `st`/`sc` images stand in (approximation A4).
                    let by_name = |name: &str| {
                        sky_images
                            .iter()
                            .find(|img| img.name.eq_ignore_ascii_case(name))
                    };
                    let pick = |slot: usize, prefix: &str| {
                        match scene.sky_map.as_ref().map(|b| b.maps[slot].as_str()) {
                            Some(name) if name.eq_ignore_ascii_case("none") => None,
                            Some(name) => by_name(name),
                            None => sky_images.iter().find(|img| img.name.starts_with(prefix)),
                        }
                    };
                    let tex = |img: &nfs_formats::Image| SkyTexture {
                        width: img.width,
                        height: img.height,
                        rgba: img.rgba.clone(),
                    };
                    scene.sky_layers[0] = pick(0, "st").map(tex);
                    scene.sky_layers[1] = pick(1, "sc").map(tex);
                    scene.sky_layers[2] = by_name("hzcl").map(tex);
                    // Region B maps (`FUN_0045b5a0` loads both regions per layer).
                    for slot in 0..2 {
                        scene.sky_layers_b[slot] = scene
                            .sky_map_b
                            .as_ref()
                            .map(|b| b.maps[slot].as_str())
                            .filter(|n| !n.eq_ignore_ascii_case("none"))
                            .and_then(by_name)
                            .map(tex);
                    }
                    // Cloud textures of the MULTI* layers (`FUN_0045b5a0`: `flat` is the blank
                    // texture of the pack, `none` no layer).
                    for slot in 0..2 {
                        scene.sky_clouds[slot] = scene
                            .sky_map
                            .as_ref()
                            .map(|b| b.clouds[slot].as_str())
                            .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("none") && !n.eq_ignore_ascii_case("flat"))
                            .and_then(by_name)
                            .map(tex);
                    }
                    // Material blending per layer (`FUN_0045b5a0` -> 0x5379f0 / 0x537a40,
                    // recorded): needs the cloud's FSH format and the maps' alpha.
                    if let Some(b) = scene.sky_map.as_ref() {
                        let formats: std::collections::HashMap<String, u32> = nfs_formats::parse_fsh_entries(sky_data)
                            .map(|e| e.into_iter().map(|e| (e.name, e.format)).collect())
                            .unwrap_or_default();
                        for slot in 0..2 {
                            let same = scene
                                .sky_map_b
                                .as_ref()
                                .is_none_or(|rb| rb.maps[slot].eq_ignore_ascii_case(&b.maps[slot]));
                            let a = scene.sky_layers[slot].as_ref().map(|t| t.rgba.as_slice());
                            let bm = scene.sky_layers_b[slot].as_ref().map(|t| t.rgba.as_slice());
                            let opaque = nfs_formats::sky_map_opaque(a, bm, same);
                            let cloud = b.clouds[slot].as_str();
                            let format = formats.get(&cloud.to_ascii_lowercase()).copied();
                            scene.sky_layer_blends[slot] = nfs_formats::sky_layer_blends(cloud, format, opaque);
                        }
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

    // --- Car environment textures (`<track>.env`, TrackLighting_Load 0x4409d0) ---
    // Read from the decompile, not recorded (A274): loaded only with experimental-original-port.
    let env_key = format!("{track}.env");
    if let Some(data) = cfg!(feature = "experimental-original-port").then(|| find(files, &env_key).ok()).flatten() {
        match crate::loader::fsh_textures(&env_key, data) {
            Ok(textures) => scene.env_textures = textures,
            Err(e) => scene.diagnostics.push(format!("ENV {env_key}: {e}")),
        }
    }

    // --- Track topology loading (.edg, .jnc, .map, .lsp) ---
    let edg_key = format!("{track}.edg");
    let jnc_key = format!("{track}.jnc");
    let map_key = format!("{track}.map");
    let lsp0_key = format!("{track}0.lsp");
    let lsp_dir_key = format!("{track}{}.lsp", reverse as u8);
    let lsp_key = format!("{track}.lsp");

    let edg_data = find(files, &edg_key).ok();
    let jnc_data = find(files, &jnc_key).ok();
    let map_data = find(files, &map_key).ok();
    let lsp_data = find(files, &lsp_dir_key)
        .or_else(|_| find(files, &lsp0_key))
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
            // The car sky-map sections (0x4409d0, recorded: tests/original_car_env_map.rs).
            scene.env_sections = crate::car_env::load_sections(data);
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
                        // A backward race runs the racing line against its point order.
                        let ordered: Vec<_> = if reverse {
                            lsp.points.iter().rev().cloned().collect()
                        } else {
                            lsp.points.clone()
                        };
                        if let Ok(course) =
                            TrackCourse::from_spline_points(&ordered, is_circuit, |x, z| {
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
        if let Ok(lsp) = find(files, &lsp_dir_key).or_else(|_| find(files, &lsp0_key)) {
            scene.ai_track = crate::race::original_ai::field::OriginalAiTrack::from_files(
                track, reverse, crp_data, jnc_data, lsp,
            );
        }
    }

    // The start scene of the direction (`FUN_0046d600`): the start line trigger.
    let start_key = format!("{track}_{}start.scn", if reverse { 'b' } else { 'f' });
    if let Ok(bytes) = find(files, &start_key) {
        use crate::race::original_ai::start;
        let text = String::from_utf8_lossy(bytes);
        scene.start_trigger = start::start_trigger(&text);
        scene.finish_trigger = start::finish_trigger(&text);
        // The finish line: the start scene's GEOM elements place the track's `FNSH`
        // library article (animdefs `collideType=0`) at the finish trigger; the scenario
        // reader 0x46c220 places them like the mission scenes' objects.
        if let Ok(scn) = parse_scn(&text) {
            let mut placed = 0;
            for ge in &scn.geom_elements {
                if let Some(idx) = scene.prop_articles.iter().position(|pa| pa.fourcc == ge.fourcc) {
                    scene.prop_instances.push(PropInstance {
                        article_index: idx,
                        triggerable: false,
                        position: to_scene_coordinates(ge.position),
                        rotation: ge.rotation.map(|row| {
                            row.map(|v| v * ge.uniform_scale_percent.map_or(1.0, |p| p as f32 * 0.01))
                        }),
                    });
                    placed += 1;
                }
            }
            scene.diagnostics.push(format!(
                "Start scene {start_key}: {placed} of {} GEOM elements placed (finish line)",
                scn.geom_elements.len()
            ));
        }
        // A point-to-point race runs from the start line to the finish line along the road:
        // the course follows the node graph between the two triggers (the `.lsp` order is
        // the file order of the sections, not the road, and runs on past the finish).
        if let (Some(ai), Some(s), Some(f)) = (&scene.ai_track, scene.start_trigger, start::finish_trigger(&text)) {
            let nodes = &ai.nodes;
            let (sn, fnode) = (start::start_node(nodes, s), start::start_node(nodes, f));
            if let Some(route) = start::route(nodes, sn, fnode, reverse).filter(|r| r.len() >= 3) {
                let points: Vec<_> = route
                    .iter()
                    .map(|&n| {
                        let p = nodes.nodes[n as usize].pos_04;
                        nfs_formats::SplinePoint { x: p[0], z: p[2] }
                    })
                    .collect();
                let surface = scene.road_surface.as_ref();
                if let Ok(course) = TrackCourse::from_spline_points(&points, false, |x, z| {
                    surface
                        .and_then(|s| s.query(x, z, 0.0, 500.0, 500.0))
                        .map(|h| h.height)
                        .unwrap_or(0.0)
                }) {
                    scene.diagnostics.push(format!(
                        "Course from the start line to the finish along the nodes: {} nodes, {:.1} m",
                        route.len(),
                        course.total_length
                    ));
                    scene.course = Some(course);
                }
            }
        }
    }

    // No start triggers (circuits): the course is one lap along the node graph from the start
    // node, in the race direction (the `.lsp` order of the sections is not the road).
    if scene.start_trigger.is_none() {
        use crate::race::original_ai::start;
        if let Some(ai) = &scene.ai_track {
            let nodes = &ai.nodes;
            let first = start::circuit_first(nodes, reverse);
            if let Some(route) = start::lap_route(nodes, first, reverse).filter(|r| r.len() >= 100) {
                let points: Vec<_> = route
                    .iter()
                    .map(|&n| {
                        let p = nodes.nodes[n as usize].pos_04;
                        nfs_formats::SplinePoint { x: p[0], z: p[2] }
                    })
                    .collect();
                let surface = scene.road_surface.as_ref();
                if let Ok(course) = TrackCourse::from_spline_points(&points, true, |x, z| {
                    surface
                        .and_then(|s| s.query(x, z, 0.0, 500.0, 500.0))
                        .map(|h| h.height)
                        .unwrap_or(0.0)
                }) {
                    scene.diagnostics.push(format!(
                        "Course: one lap along the nodes, {} nodes, {:.1} m",
                        route.len(),
                        course.total_length
                    ));
                    scene.course = Some(course);
                }
            }
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
    ambient_colour: &BTreeMap<usize, [f32; 4]>,
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

        // `df` colours are D3D ARGB words (bytes B, G, R, A); a `LayerAmbient` material
        // has its constant instead (`ambient_layer_colour`).
        let mut color = [1.0; 4];
        if let Some(&c) = ambient_colour.get(&material) {
            color = c;
        } else if let (Some(df), Some(&(off, adj))) = (df, channels.get(&3)) {
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

    /// `mt` record of one texture and the given layers `(type, params)`.
    fn mt_record(layers: &[(u32, Vec<u32>)]) -> Vec<u8> {
        let mut w = vec![0u32; 8];
        w[2] = 1 | ((layers.len() as u32) << 16);
        w.extend(std::iter::repeat_n(0, 11)); // one 0x2c texture entry
        let mut off = 0u32;
        for (ty, p) in layers {
            w.extend([*ty, off, p.len() as u32 * 4, 0]);
            off += p.len() as u32 * 4;
        }
        for (_, p) in layers {
            w.extend(p);
        }
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn uv_animate_speed_reads_the_mode_0_layer() {
        // Plain UVAnimate: one layer {mode 0, speed 70}.
        let plain = mt_record(&[(5, vec![0, 70, 0, 0, 0])]);
        assert_eq!(uv_animate_speed(&plain), Some(70));
        // Negative speed scrolls the other way.
        let rev = mt_record(&[(5, vec![0, (-110i32) as u32, 0, 0, 0])]);
        assert_eq!(uv_animate_speed(&rev), Some(-110));
        // FA_UVAnim: mode 0 layer first, colour layer second.
        let fa = mt_record(&[(5, vec![0, 70, 0, 0, 0]), (1, vec![0xff00_46ff])]);
        assert_eq!(uv_animate_speed(&fa), Some(70));
        // Mode 1 / 2 and non-UV layers are not scrolled.
        assert_eq!(uv_animate_speed(&mt_record(&[(5, vec![1, 20, 4, 0, 0])])), None);
        assert_eq!(uv_animate_speed(&mt_record(&[(4, vec![2, 10, 0, 0, 0])])), None);
        assert_eq!(uv_animate_speed(&mt_record(&[(1, vec![0xffff_ffff])])), None);
        assert_eq!(uv_animate_speed(&[0; 16]), None);
    }

    #[test]
    fn local_finish_line_objects_at_the_finish_trigger_of_every_open_track() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let Ok(anim) = std::fs::read(root.join("animdefs.txt")) else {
            eprintln!("SKIPPED local_finish_line_objects_at_the_finish_trigger_of_every_open_track");
            return;
        };
        let fnsh = u32::from_le_bytes(*b"FNSH");
        for track in [
            "alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest",
            "industrial",
        ] {
            for (reverse, key) in [(false, "fstart"), (true, "bstart")] {
                let mut files = AssetFiles::new();
                files.insert("animdefs.txt".into(), anim.clone());
                for ext in ["crp", "fsh"] {
                    let name = format!("{track}.{ext}");
                    let Ok(b) = std::fs::read(root.join(&name)) else { return };
                    files.insert(name, b);
                }
                let name = format!("{track}_{key}.scn");
                let Ok(b) = std::fs::read(root.join(&name)) else { return };
                files.insert(name, b);
                let scene = load_direction(&files, track, reverse).unwrap();
                let finish = to_scene_coordinates(scene.finish_trigger.expect("finish trigger"));
                let lines: Vec<_> = scene
                    .prop_instances
                    .iter()
                    .filter(|p| scene.prop_articles[p.article_index].fourcc == fnsh)
                    .collect();
                assert!(lines.len() >= 2, "{track} {key}: {} FNSH, {:?}", lines.len(), scene.diagnostics);
                for p in lines {
                    let d = (0..3).map(|k| (p.position[k] - finish[k]).powi(2)).sum::<f32>().sqrt();
                    assert!(d < 25.0, "{track} {key}: FNSH {d} m from the finish trigger");
                }
            }
        }
    }

    /// `RMth` chunk: one group, the given entries `(tag, type, mask)` and one parameter
    /// (tag `mA`, 4 bytes) with the default value `word`.
    fn rmth_chunk(entries: &[(&[u8; 2], u32, u32)], word: u32) -> Vec<u8> {
        let mut b = b"Test\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        for v in [1u32, entries.len() as u32, 1, 0, entries.len() as u32, 0, 1] {
            b.extend(v.to_le_bytes());
        }
        for (tag, ty, mask) in entries {
            b.extend(1u16.to_le_bytes());
            b.extend(*tag);
            for v in [*ty, 1, *mask] {
                b.extend(v.to_le_bytes());
            }
        }
        b.extend(1u16.to_le_bytes());
        b.extend(b"mA");
        for v in [0u32, 4, 0, word] {
            b.extend(v.to_le_bytes());
        }
        b
    }

    /// Every `sR` block the shipped tracks draw with lies inside the domain recorded by
    /// `replay-render-state.py` (shade 0..3, blend enable 0..3, blend mode -1..3, cull 0..2,
    /// z bias -3..11, field 5 -1), so `render_state::draw_state` is proven for all of them.
    #[test]
    fn local_track_render_states_are_in_the_recorded_domain() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let Ok(dir) = std::fs::read_dir(&root) else {
            eprintln!("SKIPPED: no local game data at {}", root.display());
            return;
        };
        let mut seen = std::collections::BTreeMap::new();
        for path in dir.flatten().map(|e| e.path()) {
            if path.extension().is_none_or(|x| x != "crp") {
                continue;
            }
            let crp = parse_track_crp(&std::fs::read(&path).unwrap()).unwrap();
            let methods = render_methods(&crp.misc);
            for e in crp.misc.iter().filter(|e| e.tag == "mt" && e.data.len() >= 32) {
                let name = c_string(bytes(&e.data, 16, 16).unwrap());
                if let Some(s) = material_render_state(&e.data, methods.get(&name)) {
                    *seen.entry((s, name)).or_insert(0) += 1;
                }
            }
        }
        assert!(!seen.is_empty());
        for ((s, name), n) in &seen {
            eprintln!("{s:?} {name} x{n}");
            assert!((0..=3).contains(&s[0]) && (0..=3).contains(&s[1]), "{s:?} {name}");
            assert!((-1..=3).contains(&s[2]) && (0..=2).contains(&s[3]), "{s:?} {name}");
            assert!((-3..=11).contains(&s[4]) && s[5] == -1, "{s:?} {name}");
        }
    }

    #[test]
    fn ambient_layer_colour_follows_the_material_flags() {
        // Entries as in the shipped `Opaque` method: `df` data layer (mask 0x40008) and
        // `LayerAmbient` (mask 0x10); the `mt +0x0c` flags pick one (0x44bf70).
        let rmth = rmth_chunk(&[(b"fd", 6, 0x40008), (b"mA", 0x8007, 0x10)], 0xff80_4020);
        let c = ambient_layer_colour(&rmth, 0x30010).unwrap();
        assert_eq!(c, df_rgba(&0xff80_4020u32.to_le_bytes()));
        assert_eq!(ambient_layer_colour(&rmth, 0x30008), None);
        // Mask 0: always kept.
        let always = rmth_chunk(&[(b"mA", 0x8007, 0)], 0xffff_ffff);
        assert_eq!(ambient_layer_colour(&always, 0), Some([1.0; 4]));
        assert_eq!(ambient_layer_colour(&[0; 8], 0x10), None);
        // `FullAmbient` (`AF` 0x8005, mask 0): white whatever the flags.
        let full = rmth_chunk(&[(b"FA", 0x8005, 0)], 0);
        assert_eq!(ambient_layer_colour(&full, 0x0b), Some([1.0; 4]));
    }

    #[test]
    fn local_canyon_road_keeps_df_and_objects_take_the_ambient_constant() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("canyon.crp")),
            std::fs::read(root.join("canyon.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("canyon.crp".into(), crp);
        files.insert("canyon.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "canyon").unwrap();
        let colours = |tex: &str| -> Vec<[f32; 4]> {
            scene
                .meshes
                .iter()
                .filter(|m| {
                    scene.materials[m.material]
                        .texture
                        .is_some_and(|t| scene.textures[t].name == tex)
                })
                .flat_map(|m| m.vertices.iter().map(|v| v.color))
                .collect()
        };
        // Road (`Opaque`, flags 0x30008): `df` words, shaded.
        let road = colours("rd74");
        assert!(!road.is_empty());
        assert!(road.iter().any(|c| c[..3] != [1.0; 3]));
        // Cone (`Opaque`, flags 0x30010): `LayerAmbient` 0xffffffff (feature only; the default
        // build keeps its `df` words).
        let cone = colours("cone");
        assert!(!cone.is_empty());
        if cfg!(feature = "experimental-original-port") {
            assert!(cone.iter().all(|c| *c == [1.0; 4]));
        }
    }

    #[test]
    fn local_canyon_waterfalls_scroll() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("canyon.crp")),
            std::fs::read(root.join("canyon.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("canyon.crp".into(), crp);
        files.insert("canyon.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "canyon").unwrap();
        let speed = |tex: &str| {
            scene
                .materials
                .iter()
                .find(|m| m.texture.is_some_and(|t| scene.textures[t].name == tex))
                .and_then(|m| m.uv_scroll)
        };
        assert_eq!(speed("wfw2"), Some(70));
        assert_eq!(speed("wfw4"), Some(110));
        assert_eq!(speed("ch07"), None);
    }

    #[test]
    fn local_canyon_shadow_material_is_blended() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("canyon.crp")),
            std::fs::read(root.join("canyon.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("canyon.crp".into(), crp);
        files.insert("canyon.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "canyon").unwrap();
        let sheet: Vec<_> = scene
            .materials
            .iter()
            .filter(|m| m.texture.is_some_and(|t| scene.textures[t].name == "sh80"))
            .collect();
        assert_eq!(sheet.len(), 1);
        // Evidence gate: read-not-recorded behaviour only with `experimental-original-port`.
        let on = cfg!(feature = "experimental-original-port");
        let m = sheet[0];
        assert_eq!(m.alpha_mode, if on { AlphaMode::Blend } else { AlphaMode::Mask });
        // Method default `sR` {1, 2, 0, 1, -3, -1}: z bias -3, clamped decal addressing.
        assert_eq!(m.depth_bias, if on { -3 } else { 0 });
        assert_eq!(m.clamp_uv, on);
        // The decal's border has alpha 0, so clamped addressing hides what lies outside 0..1.
        let t = &scene.textures[m.texture.unwrap()];
        let (w, h) = (t.width as usize, t.height as usize);
        let a = |x: usize, y: usize| t.rgba[(y * w + x) * 4 + 3];
        assert!((0..w).all(|x| a(x, 0) == 0 && a(x, h - 1) == 0));
        assert!((0..h).all(|y| a(0, y) == 0 && a(w - 1, y) == 0));
    }

    #[test]
    fn local_castle_and_forest_waterfalls_scroll_with_source_render_method() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        for (track, expected) in [
            ("castle", vec![("wfw1", -110), ("wfw2", 70), ("wfw2", -70), ("wfw4", 110)]),
            ("forest", vec![("wfw2", 60), ("wfw4", 100)]),
        ] {
            let (Ok(crp), Ok(fsh), Ok(anim)) = (
                std::fs::read(root.join(format!("{track}.crp"))),
                std::fs::read(root.join(format!("{track}.fsh"))),
                std::fs::read(root.join("animdefs.txt")),
            ) else {
                return;
            };
            let mut files = AssetFiles::new();
            files.insert(format!("{track}.crp"), crp);
            files.insert(format!("{track}.fsh"), fsh);
            files.insert("animdefs.txt".into(), anim);
            let scene = load(&files, track).unwrap();
            for (tex, speed) in expected {
                let m = scene
                    .materials
                    .iter()
                    .find(|m| {
                        m.uv_scroll == Some(speed)
                            && m.texture.is_some_and(|t| scene.textures[t].name == tex)
                    })
                    .unwrap_or_else(|| panic!("{track} {tex} {speed}"));
                let sheet = cfg!(feature = "experimental-original-port")
                    && translucent_sheet(&scene.textures[m.texture.unwrap()].rgba);
                let expected_alpha = if m.name.ends_with(":Transparency") || sheet {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Mask
                };
                assert_eq!(m.alpha_mode, expected_alpha, "{track} {tex} {}", m.name);
            }
        }
    }

    #[test]
    fn local_coastal_sea_surfaces_keep_source_render_method() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("coastal.crp")),
            std::fs::read(root.join("coastal.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("coastal.crp".into(), crp);
        files.insert("coastal.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "coastal").unwrap();
        for tex in ["wtr3", "wtr4", "seaf", "seag", "seah"] {
            let mat = scene
                .materials
                .iter()
                .find(|m| m.texture.is_some_and(|t| scene.textures[t].name == tex))
                .unwrap_or_else(|| panic!("no material for {tex}"));
            // Every sea and shore sheet is a soft sheet of a blended method (`Normal`,
            // `UVAnimate`: recorded `sR` blend).
            let expected_alpha = if mat.name.ends_with(":Transparency")
                || cfg!(feature = "experimental-original-port")
            {
                AlphaMode::Blend
            } else {
                AlphaMode::Mask
            };
            assert_eq!(mat.alpha_mode, expected_alpha, "{tex} {}", mat.name);
        }
        // Cut-out foliage stays masked.
        let fol = scene
            .materials
            .iter()
            .find(|m| m.texture.is_some_and(|t| scene.textures[t].name == "ch07"))
            .unwrap();
        assert_eq!(fol.alpha_mode, AlphaMode::Mask);
    }

    #[cfg(feature = "experimental-original-port")]
    #[test]
    fn local_coastal_lighthouse_beam_is_animated_scenery() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("coastal.crp")),
            std::fs::read(root.join("coastal.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("coastal.crp".into(), crp);
        files.insert("coastal.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "coastal").unwrap();
        let names: Vec<_> = scene.animated_articles.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["BOX01", "OBJECT01"]);
        for a in &scene.animated_articles {
            assert_eq!(a.keys.len(), 401);
            assert!(!a.meshes.is_empty());
            // `FullAmbient` beam materials draw white (0x448c30), not the dark `df` words.
            for m in &scene.meshes[a.meshes.clone()] {
                // `lths` (alpha 12..119) must blend, not mask away.
                if scene.materials[m.material].texture.is_some_and(|t| scene.textures[t].name == "lths") {
                    assert_eq!(scene.materials[m.material].alpha_mode, AlphaMode::Blend);
                }
                assert!(m.vertices.iter().all(|v| v.color == [1.0; 4]), "{} {}", a.name, m.name);
            }
            // Pivot at the lantern; one full turn about y over 400 keys.
            let (r0, p0) = crate::camera::anim::sample_scenery(&a.keys, 0).unwrap();
            assert!((p0[0] - 3247.5).abs() < 0.1 && (p0[1] - 75.73).abs() < 0.1, "{p0:?}");
            // 100 keys = 640 steps (10 s): a quarter turn.
            let (r1, _) = crate::camera::anim::sample_scenery(&a.keys, 640).unwrap();
            let dot: f32 = (0..3).map(|k| r0[0][k] * r1[0][k]).sum();
            assert!(dot.abs() < 0.01, "{}: x rows {:?} {:?}", a.name, r0[0], r1[0]);
        }
    }

    #[test]
    fn partial_alpha_does_not_invent_a_blend_render_method() {
        let sea = [200, 170, 180, 95].repeat(64);
        assert_eq!(track_alpha_mode("Opaque", None, &sea), AlphaMode::Mask);
        assert_eq!(track_alpha_mode("Water", None, &sea), AlphaMode::Mask);
        assert_eq!(track_alpha_mode("Transparency", None, &sea), AlphaMode::Blend);
        let opaque = [10, 90, 20, 255].repeat(64);
        assert_eq!(track_alpha_mode("Transparency", None, &opaque), AlphaMode::Opaque);
        assert_eq!(track_alpha_mode("Opaque", None, &[]), AlphaMode::Opaque);
        assert_eq!(
            track_alpha_mode("S_UVAnimate", None, &sea),
            if cfg!(feature = "experimental-original-port") { AlphaMode::Blend } else { AlphaMode::Mask }
        );
    }

    /// The recorded `sR` blend (`{1, 2, 0, 1, 0, -1}`: `SRCALPHA`/`INVSRCALPHA`) blends a soft
    /// sheet with the feature; cut-out textures and unblended states stay masked.
    #[test]
    fn recorded_blend_state_blends_soft_sheets_with_the_feature() {
        const NORMAL: [i32; 6] = [1, 2, 0, 1, 0, -1];
        const OPAQUE: [i32; 6] = [1, 0, 0, 1, 0, -1];
        let on = cfg!(feature = "experimental-original-port");
        let sea = [200, 170, 180, 95].repeat(64);
        let blend = if on { AlphaMode::Blend } else { AlphaMode::Mask };
        assert_eq!(track_alpha_mode("Normal", Some(NORMAL), &sea), blend);
        assert_eq!(track_alpha_mode("UVAnimate", Some(NORMAL), &sea), blend);
        assert_eq!(track_alpha_mode("Opaque", Some(OPAQUE), &sea), AlphaMode::Mask);
        assert_eq!(track_alpha_mode("Normal", None, &sea), AlphaMode::Mask);
        // Foliage: 0/255 alpha.
        let mut leaf = [30, 90, 20, 255].repeat(32);
        leaf.extend([0, 0, 0, 0].repeat(32));
        assert_eq!(track_alpha_mode("Normal", Some(NORMAL), &leaf), AlphaMode::Mask);
        assert!(translucent_sheet(&sea) && !translucent_sheet(&leaf) && !translucent_sheet(&[]));
    }

    /// `RMth` record: name, one pass, the layer definition tags, one `sR` default.
    fn rmth_entry(name: &str, tags: &[u32], state: [i32; 6]) -> Entry {
        let mut d = name.as_bytes().to_vec();
        d.resize(16, 0);
        let mut w = vec![1u32, tags.len() as u32, 1];
        w.extend([0, tags.len() as u32, 0, 1]); // pass record
        for &t in tags {
            w.extend([t, 0, 0, 0]);
        }
        w.extend([RENDER_STATE_KEY, 0, 24, 0]);
        w.extend(state.map(|v| v as u32));
        d.extend(w.iter().flat_map(|x| x.to_le_bytes()));
        Entry { tag: "RMth".into(), index: 0, count: 1, offset: 0, data: d, children: Vec::new() }
    }

    #[test]
    fn material_state_overrides_the_method_cull() {
        // Tags as alps `Normal`: the sorted keys are 1, '$n', 'Rs', 'vt' (Rs = index 2).
        let tags = [0x246e_0001, 0x7674_0001, RENDER_STATE_KEY, 1, 0x7674_0001];
        let entries = [rmth_entry("Normal", &tags, [1, 2, 0, 1, 0, -1])];
        let methods = render_methods(&entries);
        let method = methods.get("Normal");
        assert_eq!(method.unwrap().keys, vec![1, 0x246e_0001, RENDER_STATE_KEY, 0x7674_0001]);
        // No material layer: method default, cull 1 = the view's cull mode.
        let plain = material_render_state(&mt_record(&[]), method);
        assert_eq!(plain, Some([1, 2, 0, 1, 0, -1]));
        assert!(!track_material_double_sided(plain, "Normal"));
        // `MatState` style override with cull 0 (alps mt198, sign post).
        let post = material_render_state(&mt_record(&[(2, vec![1, 2, 0, 0, 0, u32::MAX])]), method);
        assert!(track_material_double_sided(post, "Normal"));
        // A 24-byte layer for another key is not a render state.
        let other = material_render_state(&mt_record(&[(3, vec![1, 2, 0, 0, 0, u32::MAX])]), method);
        assert_eq!(other, plain);
        // Unknown method: the old classification.
        assert!(track_material_double_sided(material_render_state(&mt_record(&[]), None), "Normal"));
    }

    #[test]
    fn local_alps_sign_plates_are_culled_and_posts_are_not() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let (Ok(crp), Ok(fsh), Ok(anim)) = (
            std::fs::read(root.join("alps.crp")),
            std::fs::read(root.join("alps.fsh")),
            std::fs::read(root.join("animdefs.txt")),
        ) else {
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("alps.crp".into(), crp);
        files.insert("alps.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        let scene = load(&files, "alps").unwrap();
        let mat = |name: &str| scene.materials.iter().find(|m| m.name == name).unwrap();
        // Article 1626 `30k_Rigid1_SGN`: back plate mt123, face plate mt126 (Normal, method
        // default cull 1), post mt198 (NormMatState, material state cull 0).
        assert!(!mat("mt123:Normal").double_sided);
        assert!(!mat("mt126:Normal").double_sided);
        assert!(mat("mt198:NormMatState").double_sided);
        // Bushes (FakeSort with a cull-0 state) stay visible from both sides; FakeSort
        // trees without one are culled.
        assert!(mat("mt161:FakeSort").double_sided);
        assert!(!mat("mt7:FakeSort").double_sided);
        assert_eq!(scene.materials.iter().filter(|m| m.double_sided).count(), 51);
    }

    #[test]
    fn df_words_are_argb() {
        // Alps road RD1136C: bytes 99 6b 4b ff = ARGB 0xff4b6b99.
        let c = df_rgba(&[0x99, 0x6b, 0x4b, 0xff]);
        assert_eq!(c, [0x4b as f32 / 255.0, 0x6b as f32 / 255.0, 0x99 as f32 / 255.0, 1.0]);
        // Brake glare colour word 0xffff0000 (bytes 00 00 ff ff) is red.
        assert_eq!(df_rgba(&[0, 0, 0xff, 0xff]), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn local_canyon_waterfall_emitters() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track/canyon.crp");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let crp = parse_track_crp(&data).unwrap();
        let e = particle_emitters(&crp, &Default::default());
        let tags: Vec<&[u8; 4]> = e.iter().map(|e| &e.tag).collect();
        assert_eq!(tags.iter().filter(|t| t.starts_with(b"WF0")).count(), 8, "{tags:?}");
        let base = e.iter().find(|e| &e.tag == b"WF06").unwrap();
        assert!((base.frame.position[0] + 3243.372).abs() < 0.01);
        assert!(base.frame.axis_b[1] > 0.9, "the base mist rises");
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
        // Vertex colours: `df` words (pr channel kind 3) or the `LayerAmbient` constant.
        // Skidpad's `df` words are white where they are drawn; its 90 shaded entries
        // belong to `LayerAmbient` materials (flag 0x10), which take 0xffffffff with
        // `experimental-original-port` (the default keeps `df`).
        let colors: Vec<[f32; 4]> = scene
            .meshes
            .iter()
            .flat_map(|m| m.vertices.iter().map(|v| v.color))
            .collect();
        let shaded = colors.iter().filter(|c| c[..3] != [1.0; 3]).count();
        let expected = if cfg!(feature = "experimental-original-port") { 0 } else { 90 };
        assert_eq!(shaded, expected, "{shaded} of {} shaded", colors.len());
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
    fn local_backward_race_uses_lsp1_and_reversed_course() {
        // Local game data (skipped without it): alps, a point-to-point track.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Track");
        let read = |n: &str| std::fs::read(root.join(n)).ok();
        let (Some(crp), Some(fsh), Some(anim), Some(l0), Some(l1)) = (
            read("alps.crp"),
            read("alps.fsh"),
            read("animdefs.txt"),
            read("racer/work/alps0.lsp"),
            read("racer/work/alps1.lsp"),
        ) else {
            eprintln!("skipped: no local game data");
            return;
        };
        let mut files = AssetFiles::new();
        files.insert("alps.crp".into(), crp);
        files.insert("alps.fsh".into(), fsh);
        files.insert("animdefs.txt".into(), anim);
        files.insert("alps0.lsp".into(), l0);
        files.insert("alps1.lsp".into(), l1);
        if let Some(j) = read("alps.jnc") {
            files.insert("alps.jnc".into(), j);
        }
        let fwd = load_direction(&files, "alps", false).unwrap();
        let bwd = load_direction(&files, "alps", true).unwrap();
        let (cf, cb) = (fwd.course.unwrap(), bwd.course.unwrap());
        assert!(!cf.is_circuit);
        // The backward course starts where the forward one ends and runs the other way.
        let last = cf.waypoints.last().unwrap().position;
        let first = cb.waypoints[0].position;
        assert!((last[0] - first[0]).abs() < 1e-3 && (last[2] - first[2]).abs() < 1e-3);
        assert!(cf.waypoints[0].forward[2] * cb.waypoints[0].forward[2] < 0.0);
        // The AI graph: the direction's racing line and REVERSE.
        let (af, ab) = (fwd.ai_track.unwrap(), bwd.ai_track.unwrap());
        assert!(!af.reverse && ab.reverse);
        assert_ne!(af.line, ab.line, "alps1.lsp differs from alps0.lsp");
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


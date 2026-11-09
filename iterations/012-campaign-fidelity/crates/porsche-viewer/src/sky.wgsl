// Horizon ring of the original sky (`SkyHorizon_Build` 0x459d30, docs/re/sky.md): 16 flat quads at
// radius 400 around the eye, from height `base` to `base + height`; tiles of `horz` per segment.
struct SkyCamera {
    view_projection: mat4x4<f32>,
    horizon: vec4<f32>,       // rotation (turns), base, height, tile rows
    grid: vec4<f32>,          // tile columns, first hzcl column
    tiles: array<vec4<f32>, 16>,   // tile index, mirrored
    top_colors: array<vec4<f32>, 16>,
    bottom_colors: array<vec4<f32>, 16>,
    dome: vec4<f32>,    // angle (turns), size, curvature, height
    dome2: vec4<f32>,   // scale, dome present, layer 0 1/xscale, 1/yscale
    dome3: vec4<f32>,   // layer 1 1/xscale, 1/yscale, MULTI* layers (1/0), layer blending bits
    drift: vec4<f32>,   // layer texture offsets u0 v0 u1 v1
    fog: vec4<f32>,     // sky fog (0x45d2e0): sRGB colour, EXP density (0 = off)
    forward: vec4<f32>, // camera forward, scene axes
    clear: vec4<f32>,   // clear colour (0x459a70), sRGB; w = 1 when known
    dome_b: vec4<f32>,  // region B layer 0 1/xscale, 1/yscale, layer 1 1/xscale, 1/yscale
    blend: vec4<f32>,   // sky-map blend A -> B (0x45a350), animated MULTI* grid (1/0), 0, 0
    cull: vec4<f32>,    // wedge planes of the cell cull (0x45aaf0): n1x, n1z, n2x, n2z
    cull2: vec4<f32>,   // cull other cells (1/0), cull the centre cell (1/0), 0, 0
    uv_inv: vec4<f32>,  // MULTI* animated grid: layer 0 / 1 1/scale (blend.y = 1)
    warp: array<vec4<f32>, 128>, // warp offsets of the 256 grid vertices, two per entry
};
@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
@group(1) @binding(0) var sky_texture: texture_2d<f32>;
@group(1) @binding(1) var sky_sampler: sampler;
// Sky map (layer 0) and cloud map (layer 1) on the dome of the sky config; hzcl colours the
// horizon ring on the CPU. No warp (A4 remainder).
@group(1) @binding(2) var sky_map: texture_2d<f32>;
@group(1) @binding(3) var cloud_map: texture_2d<f32>;
@group(1) @binding(4) var haze_map: texture_2d<f32>; // hzcl: used on the CPU for the vertex colours
@group(1) @binding(5) var layer_sampler: sampler;
// Region B maps (`st1b`, `sc1b` in Alps): `FUN_0045e160` mixes the texels of A and B by the
// blend, `FUN_0045f0a0` the `1/scale` factors.
@group(1) @binding(6) var sky_map_b: texture_2d<f32>;
@group(1) @binding(7) var cloud_map_b: texture_2d<f32>;
// Cloud textures of the MULTI* layers (`cc1a`; white for `flat`): `FUN_0045b5a0` draws layer i
// as map i in the dome vertex colours times cloud texture i at UV = grid index / scale.
@group(1) @binding(8) var cloud_tex0: texture_2d<f32>;
@group(1) @binding(9) var cloud_tex1: texture_2d<f32>;
struct SkyOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) direction: vec3<f32>,
};
// BASIC layers (`0x45bf80`, not read): the map as a texture spread over the 15 cells times
// `1 / scale` (approximation A195).
fn sky_scale(inv: vec2<f32>) -> vec2<f32> {
    return inv / 15.0;
}
// MULTI* map texel = colour of dome vertex (column, row) (`FUN_0045b5a0` passes the 16x16
// texels to `FUN_00468630` as the per-vertex colour channel of the 16x16 grid), Gouraud over
// the quad split (0,1,2), (0,2,3) (recorded: grid quads (i, i+1, i+17, i+16) of `FUN_0045e8e0`,
// dx7z.dll draws a quad as triangles (0,1,2), (0,2,3); `nfs_formats::sky_vertex_colour`).
// Returns the stored (gamma-space) value.
fn map_texel(map: texture_2d<f32>, x: u32, y: u32) -> vec4<f32> {
    let t = textureLoad(map, vec2<u32>(x, y), 0);
    return vec4<f32>(linear_to_srgb(t.rgb), t.a);
}
fn vertex_colour(map: texture_2d<f32>, grid: vec2<f32>) -> vec4<f32> {
    if textureDimensions(map).x < 16u {
        return textureLoad(map, vec2<u32>(0u, 0u), 0);
    }
    let g = clamp(grid, vec2<f32>(0.0), vec2<f32>(15.0));
    let base = min(vec2<u32>(floor(g)), vec2<u32>(14u));
    let f = g - vec2<f32>(base);
    let v0 = map_texel(map, base.x, base.y);
    let v1 = map_texel(map, base.x + 1u, base.y);
    let v2 = map_texel(map, base.x + 1u, base.y + 1u);
    let v3 = map_texel(map, base.x, base.y + 1u);
    if f.x >= f.y {
        return v0 + f.x * (v1 - v0) + f.y * (v2 - v1);
    }
    return v0 + f.x * (v2 - v3) + f.y * (v3 - v0);
}
// Dome vertex at grid (column, row) (`FUN_00460f40`, `nfs_formats::SkyDome::vertices`),
// original axes.
fn dome_vertex(c: f32, r: f32) -> vec3<f32> {
    let step = sky_camera.dome.y / 15.0;
    let a = (7.5 - c) * step;
    let b = (r - 7.5) * step;
    let turn = 6.28318530718 * sky_camera.dome.x;
    let cs = cos(turn);
    let sn = sin(turn);
    let s = sky_camera.dome2.x;
    return vec3<f32>((cs * a + sn * b) * s, (sky_camera.dome.w - (a * a + b * b) * sky_camera.dome.z) * s, (cs * b - sn * a) * s);
}
// Ray t * d against triangle (p0, p1, p2): (t, b1, b2, hit).
fn ray_triangle(d: vec3<f32>, p0: vec3<f32>, p1: vec3<f32>, p2: vec3<f32>) -> vec4<f32> {
    let e1 = p1 - p0;
    let e2 = p2 - p0;
    let p = cross(d, e2);
    let det = dot(e1, p);
    if abs(det) < 1e-12 {
        return vec4<f32>(0.0);
    }
    let s = -p0;
    let b1 = dot(s, p) / det;
    let q = cross(s, e1);
    let b2 = dot(d, q) / det;
    let t = dot(e2, q) / det;
    let ok = b1 >= -1e-5 && b2 >= -1e-5 && b1 + b2 <= 1.0 + 1e-5 && t > 0.0;
    return vec4<f32>(t, b1, b2, select(0.0, 1.0, ok));
}
// The dome is drawn as the flat triangles (i, i+1, i+17), (i, i+17, i+16) of the 16x16 grid
// (recorded down to D3D7: `SkyDome::triangles`, `SkyDome::hit`): the first triangle the ray
// meets around the paraboloid estimate `g`, with the grid position interpolated over it
// (the perspective-correct mix of the TL vertices). Returns (column, row, distance, hit).
fn dome_hit(d: vec3<f32>, g: vec2<f32>) -> vec4<f32> {
    let base = vec2<i32>(clamp(floor(g), vec2<f32>(0.0), vec2<f32>(14.0)));
    var best = vec4<f32>(0.0, 0.0, 1e30, 0.0);
    for (var dr = -1; dr <= 1; dr++) {
        for (var dc = -1; dc <= 1; dc++) {
            let c = base.x + dc;
            let r = base.y + dr;
            if c < 0 || c > 14 || r < 0 || r > 14 {
                continue;
            }
            let fc = f32(c);
            let fr = f32(r);
            let v0 = dome_vertex(fc, fr);
            let v1 = dome_vertex(fc + 1.0, fr);
            let v2 = dome_vertex(fc + 1.0, fr + 1.0);
            let v3 = dome_vertex(fc, fr + 1.0);
            let h0 = ray_triangle(d, v0, v1, v2);
            if h0.w > 0.5 && h0.x < best.z {
                best = vec4<f32>(fc + h0.y + h0.z, fr + h0.z, h0.x, 1.0);
            }
            let h1 = ray_triangle(d, v0, v2, v3);
            if h1.w > 0.5 && h1.x < best.z {
                best = vec4<f32>(fc + h1.y, fr + h1.y + h1.z, h1.x, 1.0);
            }
        }
    }
    return best;
}
// The layer draw's wedge cull (0x45bd40 / `FUN_0045abb0`, `nfs_formats::sky_cell_visible`):
// a cell of 5 x 5 quads is not drawn when its four corners and four edge midpoints all lie
// outside the horizontal field of view (only while the camera is nearly level).
fn cell_culled(grid: vec2<f32>) -> bool {
    let cc = min(u32(max(grid.x, 0.0) / 5.0), 2u);
    let cr = min(u32(max(grid.y, 0.0) / 5.0), 2u);
    let centre = cc == 1u && cr == 1u;
    if (centre && sky_camera.cull2.y < 0.5) || (!centre && sky_camera.cull2.x < 0.5) {
        return false;
    }
    let c = f32(cc * 5u);
    let r = f32(cr * 5u);
    let p0 = dome_vertex(c, r).xz;
    let p1 = dome_vertex(c + 5.0, r).xz;
    let p2 = dome_vertex(c, r + 5.0).xz;
    let p3 = dome_vertex(c + 5.0, r + 5.0).xz;
    let pts = array<vec2<f32>, 8>(p0, p1, p2, p3, (p1 + p0) * 0.5, (p3 + p1) * 0.5, (p3 + p2) * 0.5, (p2 + p0) * 0.5);
    let n1 = sky_camera.cull.xy;
    let n2 = sky_camera.cull.zw;
    for (var i = 0; i < 8; i++) {
        if dot(n1, pts[i]) < 0.0 && dot(n2, pts[i]) < 0.0 {
            return false;
        }
    }
    return true;
}
// Warp offset of grid vertex (column, row) (`SkyGridAnim::warp`, shared by the layers).
fn warp_vertex(c: u32, r: u32) -> vec2<f32> {
    let k = r * 16u + c;
    let e = sky_camera.warp[k / 2u];
    return select(e.xy, e.zw, (k & 1u) == 1u);
}
// The warp offsets interpolated over the dome triangle like the vertex UVs
// (`FUN_0045ec40` adds them per vertex).
fn warp_at(grid: vec2<f32>) -> vec2<f32> {
    let g = clamp(grid, vec2<f32>(0.0), vec2<f32>(15.0));
    let base = min(vec2<u32>(floor(g)), vec2<u32>(14u));
    let f = g - vec2<f32>(base);
    let v0 = warp_vertex(base.x, base.y);
    let v1 = warp_vertex(base.x + 1u, base.y);
    let v2 = warp_vertex(base.x + 1u, base.y + 1u);
    let v3 = warp_vertex(base.x, base.y + 1u);
    if f.x >= f.y {
        return v0 + f.x * (v1 - v0) + f.y * (v2 - v1);
    }
    return v0 + f.x * (v2 - v3) + f.y * (v3 - v0);
}
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), c * 12.92, c <= vec3<f32>(0.0031308));
}
// Sky fog of the weather keys: EXP table fog over the view depth of the sky geometry
// (drawn around the eye at the origin, `SkyManager_Draw` 0x45d4f0), mixed in gamma space.
fn sky_fog(c: vec3<f32>, depth: f32) -> vec3<f32> {
    if sky_camera.fog.w <= 0.0 {
        return c;
    }
    let f = exp(-sky_camera.fog.w * max(depth, 0.0));
    return srgb_to_linear(mix(sky_camera.fog.rgb, linear_to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0))), f));
}
@vertex fn vs_main(@location(0) position: vec3<f32>) -> SkyOutput {
    var out: SkyOutput;
    // Force depth to max (1.0) so sky is always behind everything
    let clip = sky_camera.view_projection * vec4<f32>(position, 1.0);
    out.clip = vec4<f32>(clip.xy, clip.w * 0.99999, clip.w);
    out.direction = position;
    return out;
}
@fragment fn fs_main(in: SkyOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.direction);
    let tau = 6.28318530718;
    let radius = 400.0;
    let segments = 16.0;
    // Original axes: scene z = -original z; azimuth in turns, x = sin, z = cos.
    let azimuth = atan2(dir.x, -dir.z) / tau;
    let t = fract(azimuth - sky_camera.horizon.x) * segments;
    let k = min(u32(floor(t)), 15u);
    let step_angle = tau / segments;
    let off = (t - f32(k) - 0.5) * step_angle;
    let half_angle = step_angle * 0.5;
    let along = 0.5 + tan(off) / (2.0 * tan(half_angle));
    let distance = radius * cos(half_angle) / cos(off);
    let horizontal = max(length(dir.xz), 1e-6);
    let facing = dot(dir, sky_camera.forward.xyz);
    let ring_depth = distance / horizontal * facing;
    let y = dir.y / horizontal * distance;
    let base = sky_camera.horizon.y;
    let band = sky_camera.horizon.z;
    let down = (base + band - y) / band;
    let down_c = clamp(down, 0.0, 1.0);

    let tile = sky_camera.tiles[k];
    let columns = sky_camera.grid.x;
    let rows = sky_camera.horizon.w;
    let across = select(along, 1.0 - along, tile.y > 0.5);
    let column = tile.x - floor(tile.x / columns) * columns;
    let row = floor(tile.x / columns);
    // Keep the bilinear footprint inside the tile: the panorama is an atlas, so sampling exactly on a
    // tile edge (the segment seams of the ring) blends in the neighbouring tile (thin bright line).
    let atlas = vec2<f32>(textureDimensions(sky_texture));
    let tile_texels = atlas / vec2<f32>(columns, rows);
    let inset = vec2<f32>(0.5) / tile_texels;
    let tile_uv = clamp(vec2<f32>(across, 0.01 + down_c * 0.97), inset, vec2<f32>(1.0) - inset);
    let u = (column + tile_uv.x) / columns;
    let v = (row + tile_uv.y) / rows;
    let panorama = textureSampleLevel(sky_texture, sky_sampler, vec2<f32>(u, v), 0.0);
    // Vertex colours from the hzcl strip, Gouraud across the quad.
    let next = (k + 1u) & 15u;
    let top_col = mix(sky_camera.top_colors[k], sky_camera.top_colors[next], along);
    let bottom_col = mix(sky_camera.bottom_colors[k], sky_camera.bottom_colors[next], along);
    let tint = mix(top_col, bottom_col, down_c).rgb;

    // Sky-map dome behind the panorama (`FUN_00460f40`, `FUN_00460ec0`, `FUN_0045ec40`,
    // docs/re/sky.md): a paraboloid grid over the eye, UV = grid cell / scale factor.
    // What shows where no dome covers the sky (and below a blending layer 0): the clear colour
    // (0x459a70: the sky fog colour or the record colours of `Sky/<track>.bin +0xce8`; feature
    // only, A317), else the top row of the panorama (approximation A193).
    var background = vec3<f32>(0.0);
    if sky_camera.clear.w > 0.5 {
        background = srgb_to_linear(sky_camera.clear.rgb);
    } else {
        let top = textureSampleLevel(sky_texture, sky_sampler, vec2<f32>(u, (row + 0.01) / rows), 0.0);
        background = top.rgb * top_col.rgb;
    }
    var sky = vec3<f32>(0.0);
    var covered = false;
    var dome_depth = 0.0;
    if sky_camera.dome2.y > 0.5 {
        // Original axes: scene z = -original z.
        let d = vec3<f32>(dir.x, dir.y, -dir.z);
        let scale = sky_camera.dome2.x;
        let curvature = sky_camera.dome.z;
        let apex = sky_camera.dome.w;
        let h2 = d.x * d.x + d.z * d.z;
        let denom = max(d.y + sqrt(d.y * d.y + 4.0 * curvature * h2 * apex), 1e-6);
        let t = 2.0 * scale * apex / denom;
        let px = t * d.x;
        let pz = t * d.z;
        let turn = tau * sky_camera.dome.x;
        let cs = cos(turn);
        let sn = sin(turn);
        let a_g = (px * cs - pz * sn) / scale;
        let b_g = (px * sn + pz * cs) / scale;
        let half_size = sky_camera.dome.y * 0.5;
        let cell = sky_camera.dome.y / 15.0;
        let col_p = (half_size - a_g) / cell;
        let row_p = (b_g + half_size) / cell;
        let multi = sky_camera.dome3.z > 0.5;
        // MULTI*: the flat triangles of the draw; BASIC (A195): the paraboloid.
        let hit = dome_hit(d, vec2<f32>(col_p, row_p));
        let col = select(col_p, hit.x, multi);
        let row = select(row_p, hit.y, multi);
        // Region A -> B blend (weather keys): the texture scale and the maps mix by `blend.x`.
        let b = sky_camera.blend.x;
        let has_b0 = sky_camera.dome_b.x > 0.0;
        let has_b1 = sky_camera.dome_b.z > 0.0;
        let inv0 = select(sky_camera.dome2.zw, mix(sky_camera.dome2.zw, sky_camera.dome_b.xy, b), has_b0);
        let inv1 = select(sky_camera.dome3.xy, mix(sky_camera.dome3.xy, sky_camera.dome_b.zw, b), has_b1);
        let uv0 = vec2<f32>(col, row) * sky_scale(inv0) + sky_camera.drift.xy;
        let uv1 = vec2<f32>(col, row) * sky_scale(inv1) + sky_camera.drift.zw;
        // Mip level from the screen derivatives (the maps are mipmapped); the derivatives and
        // all samples are taken before any branch (uniform control flow).
        let g0x = dpdx(uv0);
        let g0y = dpdy(uv0);
        let g1x = dpdx(uv1);
        let g1y = dpdy(uv1);
        let a0 = textureSampleGrad(sky_map, layer_sampler, uv0, g0x, g0y);
        let b0 = textureSampleGrad(sky_map_b, layer_sampler, uv0, g0x, g0y);
        let a1 = textureSampleGrad(cloud_map, layer_sampler, uv1, g1x, g1y);
        let b1 = textureSampleGrad(cloud_map_b, layer_sampler, uv1, g1x, g1y);
        // MULTI*: cloud textures at UV `index / scale` + warp + (drift + camera offset)
        // (`FUN_0045ec40`); with the animated grid (`SkyAnim`, recorded) the 1/scale is the
        // grid's blend and the warp its per-vertex offsets.
        let animated = sky_camera.blend.y > 0.5;
        let warp = select(vec2<f32>(0.0), warp_at(vec2<f32>(col, row)), animated);
        let tinv0 = select(inv0, sky_camera.uv_inv.xy, animated);
        let tinv1 = select(inv1, sky_camera.uv_inv.zw, animated);
        let tuv0 = vec2<f32>(col, row) * tinv0 + warp + sky_camera.drift.xy;
        let tuv1 = vec2<f32>(col, row) * tinv1 + warp + sky_camera.drift.zw;
        let t0 = textureSampleGrad(cloud_tex0, layer_sampler, tuv0, dpdx(tuv0), dpdy(tuv0));
        let t1 = textureSampleGrad(cloud_tex1, layer_sampler, tuv1, dpdx(tuv1), dpdy(tuv1));
        // Far plane 700 of the sky pass (0x45d4f0 -> 0x467120; the clip code cuts the dome
        // triangles at view depth 700, recorded) and the wedge cull of the cells.
        let multi_depth = hit.z * facing;
        if multi && hit.w > 0.5 && multi_depth <= 700.0 && !cell_culled(vec2<f32>(col, row)) {
            covered = true;
            dome_depth = multi_depth;
            // Layer i = vertex colour of map i (texels A/B mixed, `FUN_0045e160`) times cloud
            // texture i (MODULATE, alpha too), in the gamma frame buffer; layer 1 blends over
            // layer 0 by its alpha (`sR` blend 2: SRCALPHA, INVSRCALPHA).
            // A layer whose material blends (`dome3.w` bits, `sky_layer_blends`: SRCALPHA,
            // INVSRCALPHA) goes over what is below by its alpha, else it replaces it; below
            // layer 0 is the clear colour (or the A193 stand-in).
            let grid = vec2<f32>(col, row);
            let blend_bits = u32(sky_camera.dome3.w + 0.5);
            var g = linear_to_srgb(clamp(background, vec3<f32>(0.0), vec3<f32>(1.0)));
            if sky_camera.dome2.z > 0.0 {
                let va = vertex_colour(sky_map, grid);
                let v = select(va, mix(va, vertex_colour(sky_map_b, grid), b), has_b0);
                let c = v * vec4<f32>(linear_to_srgb(t0.rgb), t0.a);
                g = select(c.rgb, mix(g, c.rgb, c.a), (blend_bits & 1u) != 0u);
            }
            if sky_camera.dome3.x > 0.0 {
                let va = vertex_colour(cloud_map, grid);
                let v = select(va, mix(va, vertex_colour(cloud_map_b, grid), b), has_b1);
                let c = v * vec4<f32>(linear_to_srgb(t1.rgb), t1.a);
                g = select(c.rgb, mix(g, c.rgb, c.a), (blend_bits & 2u) != 0u);
            }
            sky = srgb_to_linear(clamp(g, vec3<f32>(0.0), vec3<f32>(1.0)));
        } else if !multi && abs(a_g) <= half_size && abs(b_g) <= half_size {
            covered = true;
            dome_depth = t * facing;
            if sky_camera.dome2.z > 0.0 {
                sky = select(a0, mix(a0, b0, b), has_b0).rgb;
            }
            if sky_camera.dome3.x > 0.0 {
                let cloud = select(a1, mix(a1, b1, b), has_b1);
                sky = mix(sky, cloud.rgb, cloud.a);
            }
        }
    }
    if covered {
        sky = sky_fog(sky, dome_depth);
    } else {
        sky = background;
    }
    // Above the band only the sky shows.
    let cover = select(panorama.a, 0.0, down < 0.0);
    var color = mix(sky, sky_fog(panorama.rgb * tint, ring_depth), cover);
    // Below the band nothing is drawn originally (the terrain covers it); keep a flat ground colour.
    let ground_color = vec3<f32>(0.647, 0.518, 0.388);
    color = select(color, ground_color, down > 1.0);
    return vec4<f32>(color, 1.0);
}

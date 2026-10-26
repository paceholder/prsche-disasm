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
    dome3: vec4<f32>,   // layer 1 1/xscale, 1/yscale
    drift: vec4<f32>,   // layer texture offsets u0 v0 u1 v1
    fog: vec4<f32>,     // sky fog (0x45d2e0): sRGB colour, EXP density (0 = off)
    forward: vec4<f32>, // camera forward, scene axes
    clear: vec4<f32>,   // clear colour (0x459a70), sRGB; w = 1 when known
    dome_b: vec4<f32>,  // region B layer 0 1/xscale, 1/yscale, layer 1 1/xscale, 1/yscale
    blend: vec4<f32>,   // sky-map blend A -> B (0x45a350), 0, 0, 0
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
struct SkyOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) direction: vec3<f32>,
};
// Texture coordinates over the dome: the 16x16 grid index over 15 cells times `1 / scale`
// (approximation A195: `FUN_0045ec40` takes `index / scale` per cell without the 1/15).
fn sky_scale(inv: vec2<f32>) -> vec2<f32> {
    return inv / 15.0;
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
    let u = (column + across) / columns;
    let v = (row + 0.01 + down_c * 0.97) / rows;
    let panorama = textureSampleLevel(sky_texture, sky_sampler, vec2<f32>(u, v), 0.0);
    // Vertex colours from the hzcl strip, Gouraud across the quad.
    let next = (k + 1u) & 15u;
    let top_col = mix(sky_camera.top_colors[k], sky_camera.top_colors[next], along);
    let bottom_col = mix(sky_camera.bottom_colors[k], sky_camera.bottom_colors[next], along);
    let tint = mix(top_col, bottom_col, down_c).rgb;

    // Sky-map dome behind the panorama (`FUN_00460f40`, `FUN_00460ec0`, `FUN_0045ec40`,
    // docs/re/sky.md): a paraboloid grid over the eye, UV = grid cell / scale factor.
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
        let col = (half_size - a_g) / cell;
        let row = (b_g + half_size) / cell;
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
        if abs(a_g) <= half_size && abs(b_g) <= half_size {
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
    } else if sky_camera.clear.w > 0.5 {
        // Outside the dome the clear colour shows (0x459a70: the sky fog colour or the
        // record colours of `Sky/<track>.bin +0xce8`).
        sky = srgb_to_linear(sky_camera.clear.rgb);
    } else {
        // Without weather data the top row of the panorama stands in (approximation A193).
        let top = textureSampleLevel(sky_texture, sky_sampler, vec2<f32>(u, (row + 0.01) / rows), 0.0);
        sky = top.rgb * top_col.rgb;
    }
    // Above the band only the sky shows.
    let cover = select(panorama.a, 0.0, down < 0.0);
    var color = mix(sky, sky_fog(panorama.rgb * tint, ring_depth), cover);
    // Below the band nothing is drawn originally (the terrain covers it); keep a flat ground colour.
    let ground_color = vec3<f32>(0.647, 0.518, 0.388);
    color = select(color, ground_color, down > 1.0);
    return vec4<f32>(color, 1.0);
}

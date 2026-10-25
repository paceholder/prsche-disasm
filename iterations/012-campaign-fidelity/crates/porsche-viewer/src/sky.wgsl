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
struct SkyOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) direction: vec3<f32>,
};
// Texture coordinates over the dome: the 16x16 grid index over 15 cells times `1 / scale`
// (approximation A195: `FUN_0045ec40` takes `index / scale` per cell without the 1/15).
fn sky_scale(inv: vec2<f32>) -> vec2<f32> {
    return inv / 15.0;
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
        if abs(a_g) <= half_size && abs(b_g) <= half_size {
            covered = true;
            let col = (half_size - a_g) / cell;
            let row = (b_g + half_size) / cell;
            if sky_camera.dome2.z > 0.0 {
                let uv0 = vec2<f32>(col, row) * sky_scale(sky_camera.dome2.zw) + sky_camera.drift.xy;
                sky = textureSampleLevel(sky_map, layer_sampler, uv0, 0.0).rgb;
            }
            if sky_camera.dome3.x > 0.0 {
                let uv1 = vec2<f32>(col, row) * sky_scale(sky_camera.dome3.xy) + sky_camera.drift.zw;
                let cloud = textureSampleLevel(cloud_map, layer_sampler, uv1, 0.0);
                sky = mix(sky, cloud.rgb, cloud.a);
            }
        }
    }
    if !covered {
        // Outside the dome the original shows its clear colour; the top row of the panorama
        // is the nearest known colour (approximation A193).
        let top = textureSampleLevel(sky_texture, sky_sampler, vec2<f32>(u, (row + 0.01) / rows), 0.0);
        sky = top.rgb * top_col.rgb;
    }
    // Above the band only the sky shows.
    let cover = select(panorama.a, 0.0, down < 0.0);
    var color = mix(sky, panorama.rgb * tint, cover);
    // Below the band nothing is drawn originally (the terrain covers it); keep a flat ground colour.
    let ground_color = vec3<f32>(0.647, 0.518, 0.388);
    color = select(color, ground_color, down > 1.0);
    return vec4<f32>(color, 1.0);
}

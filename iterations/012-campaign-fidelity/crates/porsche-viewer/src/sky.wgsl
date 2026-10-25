// Horizon ring of the original sky (`SkyHorizon_Build` 0x459d30, docs/re/sky.md): 16 flat quads at
// radius 400 around the eye, from height `base` to `base + height`; tiles of `horz` per segment.
struct SkyCamera {
    view_projection: mat4x4<f32>,
    horizon: vec4<f32>,       // rotation (turns), base, height, tile rows
    grid: vec4<f32>,          // tile columns, first hzcl column
    tiles: array<vec4<f32>, 16>,   // tile index, mirrored
    top_colors: array<vec4<f32>, 16>,
    bottom_colors: array<vec4<f32>, 16>,
};
@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
@group(1) @binding(0) var sky_texture: texture_2d<f32>;
@group(1) @binding(1) var sky_sampler: sampler;
// Approximation A4: sky map and cloud map projected on a flat sky plane behind the
// horizon panorama, horizon haze from `hzcl`; no scrolling, wind or warp.
@group(1) @binding(2) var sky_map: texture_2d<f32>;
@group(1) @binding(3) var cloud_map: texture_2d<f32>;
@group(1) @binding(4) var haze_map: texture_2d<f32>; // hzcl: used on the CPU for the vertex colours
@group(1) @binding(5) var layer_sampler: sampler;
struct SkyOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) direction: vec3<f32>,
};
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

    // Sky behind the panorama (approximation A4): the horz tiles are transparent above the skyline.
    let plane = dir.xz / max(dir.y, 0.05);
    var sky = textureSampleLevel(sky_map, layer_sampler, plane * 0.08, 0.0).rgb;
    let cloud = textureSampleLevel(cloud_map, layer_sampler, plane * 0.15 + vec2<f32>(0.3, 0.7), 0.0);
    sky = mix(sky, cloud.rgb, cloud.a);
    // Above the band only the sky shows.
    let cover = select(panorama.a, 0.0, down < 0.0);
    var color = mix(sky, panorama.rgb * tint, cover);
    // Below the band nothing is drawn originally (the terrain covers it); keep a flat ground colour.
    let ground_color = vec3<f32>(0.647, 0.518, 0.388);
    color = select(color, ground_color, down > 1.0);
    return vec4<f32>(color, 1.0);
}

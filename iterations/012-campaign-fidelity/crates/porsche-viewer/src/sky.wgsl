struct SkyCamera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
@group(1) @binding(0) var sky_texture: texture_2d<f32>;
@group(1) @binding(1) var sky_sampler: sampler;
// Approximation A4: sky map and cloud map projected on a flat sky plane behind the
// horizon panorama, horizon haze from `hzcl`; no scrolling, wind or warp.
@group(1) @binding(2) var sky_map: texture_2d<f32>;
@group(1) @binding(3) var cloud_map: texture_2d<f32>;
@group(1) @binding(4) var haze_map: texture_2d<f32>;
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
    // Azimuth angle in [0, 1) wrapping 360 degrees
    let angle = atan2(dir.x, dir.z);
    let u360 = fract(angle / (2.0 * 3.14159265) + 0.5);

    // The 256x256 horz texture contains two stacked 256x128 tiles:
    // Top tile: [0, 180 deg) -> U in [0, 1), V in [0, 0.5]
    // Bottom tile: [180, 360 deg) -> U in [0, 1), V in [0.5, 1.0]
    let is_bottom_tile = u360 >= 0.5;
    let tile_u = fract(u360 * 2.0);
    let v_base = select(0.0, 0.5, is_bottom_tile);

    // Elevation range: map [-0.15, 0.35] rad (-8.5 deg to +20 deg) to local V [1.0, 0.0]
    let elev = asin(clamp(dir.y, -1.0, 1.0));
    let elev_min = -0.15;
    let elev_max = 0.35;
    let v_local = 1.0 - clamp((elev - elev_min) / (elev_max - elev_min), 0.0, 1.0);

    // Inset slightly to prevent bleeding across tile boundary
    let u = (tile_u * 255.0 + 0.5) / 256.0;
    let v = v_base + (v_local * 126.0 + 0.5) / 256.0;

    let panorama = textureSample(sky_texture, sky_sampler, vec2<f32>(u, v));
    // Sky behind the panorama: the horz tiles are transparent above the skyline.
    let plane = dir.xz / max(dir.y, 0.05);
    var sky = textureSample(sky_map, layer_sampler, plane * 0.08).rgb;
    let cloud = textureSample(cloud_map, layer_sampler, plane * 0.15 + vec2<f32>(0.3, 0.7));
    sky = mix(sky, cloud.rgb, cloud.a);
    let haze = textureSample(haze_map, layer_sampler, vec2<f32>(u360, 0.25));
    sky = mix(sky, haze.rgb, haze.a * (1.0 - smoothstep(0.0, 0.3, elev)));
    // Above the panorama (elev > elev_max) only the sky shows.
    let above = elev > elev_max;
    let cover = select(panorama.a, 0.0, above);
    let sample_color = mix(sky, panorama.rgb, cover);
    // Fade to uniform ground color below horizon to avoid vertical streaking into the abyss
    let ground_color = vec3<f32>(0.647, 0.518, 0.388);
    let ground_fade = clamp((elev_min - elev) / 0.1, 0.0, 1.0);
    let color = mix(sample_color, ground_color, ground_fade);

    return vec4<f32>(color, 1.0);
}

// Approximation A22: gamma-space factor for prelit colours (2 = MODULATE2X) and
// how much of their hue is kept (0 = brightness only, 1 = full colour).
const PRELIT_SCALE: f32 = 1.6;
const PRELIT_SATURATION: f32 = 0.3;
struct Camera {
    view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    model: mat4x4<f32>,
    // Original car lighting (R3DLightLayer_Apply 0x446530): sun.w = 1 enables it.
    sun: vec4<f32>,
    ambient: array<vec4<f32>, 2>,
    diffuse: array<vec4<f32>, 2>,
    view_right: vec4<f32>,
    view_up: vec4<f32>,
    specular: vec4<f32>,
};
@group(0) @binding(1) var specular_map: texture_2d<f32>;
@group(0) @binding(2) var specular_sampler: sampler;
struct Material { color: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
@group(1) @binding(2) var<uniform> material: Material;
struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    // Per-vertex specular term of R3DSpecularLayer (Gouraud, as the original).
    @location(4) specular: f32,
};
// sRGB-encoded vertex colour to linear: modulating in linear space by the
// linearised colour equals the original's modulation of gamma-space values.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color: vec4<f32>) -> Output {
    var out: Output;
    let world_pos = camera.model * vec4<f32>(position, 1.0);
    out.clip = camera.view_projection * world_pos;
    out.position = world_pos.xyz;
    let normal_mat = mat3x3<f32>(camera.model[0].xyz, camera.model[1].xyz, camera.model[2].xyz);
    out.normal = normalize(normal_mat * normal);
    out.uv = uv;
    out.color = color;
    // R3DSpecularLayer 0x446e80: (reflect(sun, n) · view)^8 per vertex.
    let to_eye = normalize(camera.eye.xyz - world_pos.xyz);
    let r = 2.0 * dot(out.normal, camera.sun.xyz) * out.normal - camera.sun.xyz;
    out.specular = pow(max(dot(r, to_eye), 0.0), 8.0);
    return out;
}
@fragment fn fs_main(in: Output, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let texel = textureSample(color_texture, color_sampler, in.uv);
    // Exterior alpha encodes paint regions, not opacity. Alpha 255 is fixed
    // artwork (lamps/chrome), 0 is unpainted background; intermediate values
    // select paint regions. Full CLR palette decoding is separate work.
    let painted = material.params.z > 0.5 && texel.a > 0.5 / 255.0 && texel.a < 254.5 / 255.0;
    let tint = select(vec3<f32>(1.0), material.color.rgb, material.params.z < 0.5 || painted);
    let base = vec4<f32>(texel.rgb * tint, texel.a * material.color.a);
    let alpha_mode = material.params.x;
    let alpha_cutoff = material.params.y;
    if alpha_mode > 0.5 && alpha_mode < 1.5 && base.a <= alpha_cutoff { discard; }
    let out_alpha = select(1.0, base.a, alpha_mode > 1.5);
    if material.params.w > 0.5 && material.params.w < 1.5 {
        // Prelit track geometry: texture times its `df` vertex colour
        // (approximation A22: the original combine mode is not recovered).
        // The scale applies to the gamma-space colour (like D3D MODULATE2X).
        let grey = dot(in.color.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let tinted = mix(vec3<f32>(grey), in.color.rgb, PRELIT_SATURATION);
        let shade = srgb_to_linear(min(tinted * PRELIT_SCALE, vec3<f32>(2.0)));
        return vec4<f32>(min(base.rgb * shade, vec3<f32>(1.0)), out_alpha);
    }
    let normal = normalize(select(-in.normal, in.normal, front));
    if camera.sun.w > 0.5 {
        // Vertex colour min(1, ambient + diffuse·max(0, n·sun)) modulating the
        // texture in gamma space (approximation A23: per pixel, S = 1).
        let slot = select(0, 1, material.params.w > 1.5 && material.params.w < 2.5);
        let lit = camera.ambient[slot].rgb + camera.diffuse[slot].rgb * max(dot(normal, camera.sun.xyz), 0.0);
        var rgb = base.rgb * srgb_to_linear(min(lit, vec3<f32>(1.0)));
        if material.params.w > 2.5 && camera.specular.w > 0.5 {
            // R3DSpecularLayer 0x446e80: per-vertex term times the slot colour, added.
            let s = in.specular;
            // R3DSpecularTextureLayer 0x445520: sphere map of the view-space normal,
            // uv = 0.5 + n.xy · 0.5 / sqrt(1 + |n.z|); added by its alpha (A71).
            let nx = dot(normal, camera.view_right.xyz);
            let ny = dot(normal, camera.view_up.xyz);
            let nz = dot(normal, normalize(cross(camera.view_up.xyz, camera.view_right.xyz)));
            let k = 0.5 / sqrt(1.0 + abs(nz));
            let spc = textureSample(specular_map, specular_sampler, vec2<f32>(0.5 + nx * k, 0.5 - ny * k));
            rgb = rgb + srgb_to_linear(min(camera.specular.rgb * s, vec3<f32>(1.0))) + spc.rgb * spc.a;
        }
        return vec4<f32>(rgb, out_alpha);
    }
    let key = normalize(vec3<f32>(-0.4, 0.8, 0.6));
    let fill = normalize(vec3<f32>(0.8, 0.3, -0.6));
    let lighting = 0.38 + 0.50 * max(dot(normal, key), 0.0) + 0.20 * max(dot(normal, fill), 0.0);
    let eye = normalize(camera.eye.xyz - in.position);
    let specular = pow(max(dot(normal, normalize(key + eye)), 0.0), 48.0) * 0.12;
    return vec4<f32>(base.rgb * lighting + vec3<f32>(specular), out_alpha);
}

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
    // World fog (0x45d3b0, docs/re/weather.md): sRGB colour, EXP density (0 = off).
    fog: vec4<f32>,
    forward: vec4<f32>,
    // Planar sky-map layer (R3DSkyMapTextureLayer 0x445b10): texture-array layer of
    // slots 0..2 (-1 none), w = 1 enables; 1/texturesize, aspect/texturesize, u and v
    // offsets; centre of the projection (car position).
    env_layers: vec4<f32>,
    env_proj: vec4<f32>,
    env_origin: vec4<f32>,
    // Node frame of the car (lateral, forward), scene axes.
    env_axis_u: vec4<f32>,
    env_axis_v: vec4<f32>,
};
@group(0) @binding(1) var specular_map: texture_2d<f32>;
@group(0) @binding(2) var specular_sampler: sampler;
@group(0) @binding(3) var env_maps: texture_2d_array<f32>;
@group(0) @binding(4) var env_sampler: sampler;
struct Material { color: vec4<f32>, params: vec4<f32>, anim: vec4<f32> };
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
    // Sphere-map coordinates of R3DSpecularTextureLayer 0x445520, per vertex as the
    // original writes them into the vertex UVs (interpolated across the polygon).
    @location(4) sphere: vec2<f32>,
    // Planar sky-map coordinates (per vertex, as the layer writes them).
    @location(5) planar: vec2<f32>,
};
// sRGB-encoded vertex colour to linear: modulating in linear space by the
// linearised colour equals the original's modulation of gamma-space values.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), c * 12.92, c <= vec3<f32>(0.0031308));
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
    // R3DSpecularTextureLayer 0x445520: view-space normal (y up, as the original
    // projection 0x5391b0 flips y for the screen), uv = 0.5 + n.xy · 0.5 / sqrt(1 + |n.z|),
    // texture v down, so normals facing up read the lower half of SPC2/SPC3.
    let nx = dot(out.normal, camera.view_right.xyz);
    let ny = dot(out.normal, camera.view_up.xyz);
    let nz = dot(out.normal, normalize(cross(camera.view_up.xyz, camera.view_right.xyz)));
    let k = 0.5 / sqrt(1.0 + abs(nz));
    out.sphere = vec2<f32>(0.5 + nx * k, 0.5 + ny * k);
    // 0x445b10: the body point relative to the car in the car's node frame (lateral,
    // forward; 0x43a862 inverts the frame car+0x3b8), u = lateral/texturesize + 0.1·o1
    // + 0.5, v = forward·aspect/texturesize + o2 + 0.5: road-aligned, so the pattern turns
    // against the body when the car turns relative to the road.
    // The point is pushed out along the normal by the light state's `+0` = 3.33
    // (`R3DCarLight` ctor 0x440780 from 0x5b37c0), so the pattern follows the curvature.
    let rel = world_pos.xyz + out.normal * camera.env_origin.w - camera.env_origin.xyz;
    out.planar = vec2<f32>(
        dot(rel, camera.env_axis_u.xyz) * camera.env_proj.x + camera.env_proj.z,
        dot(rel, camera.env_axis_v.xyz) * camera.env_proj.y + camera.env_proj.w,
    );
    return out;
}
// D3D table fog EXP (driver dx7z.dll: THRASH state 0x15 = 4 -> FOGTABLEMODE EXP): the
// framebuffer (gamma) colour moves to the fog colour by 1 - exp(-density * depth); depth is
// the view-space z (Hypothesis: the driver's w-based fog).
fn apply_fog(c: vec4<f32>, position: vec3<f32>) -> vec4<f32> {
    if camera.fog.w <= 0.0 {
        return c;
    }
    let depth = max(dot(position - camera.eye.xyz, camera.forward.xyz), 0.0);
    let f = exp(-camera.fog.w * depth);
    let rgb = mix(camera.fog.rgb, linear_to_srgb(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0))), f);
    return vec4<f32>(srgb_to_linear(rgb), c.a);
}
@fragment fn fs_main(in: Output, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return apply_fog(shade(in, front), in.position);
}
fn shade(in: Output, front: bool) -> vec4<f32> {
    // anim.xy: UV offset of LayerUVAnimate mode 0 (0x448cc0).
    let texel = textureSample(color_texture, color_sampler, in.uv + material.anim.xy);
    // Exterior alpha encodes paint regions, not opacity. Alpha 255 is fixed
    // artwork (lamps/chrome), 0 is unpainted background; intermediate values
    // select paint regions. Full CLR palette decoding is separate work.
    // Blended paintable overlays (alpha_mode 2, `material.params.z` > 0.5: the rear PORSCHE
    // strip, script, plate) classify paint by the nearest texel, so the bilinear ramp at a
    // glyph edge stays coverage (blended) and is not mistaken for a paint region.
    var coverage = texel.a;
    if material.params.z > 0.5 && material.params.x > 1.5 {
        let dims = vec2<i32>(textureDimensions(color_texture));
        let at = vec2<i32>(floor((in.uv + material.anim.xy) * vec2<f32>(dims)));
        coverage = textureLoad(color_texture, ((at % dims) + dims) % dims, 0).a;
    }
    let painted = material.params.z > 0.5 && coverage > 0.5 / 255.0 && coverage < 254.5 / 255.0;
    let tint = select(vec3<f32>(1.0), material.color.rgb, material.params.z < 0.5 || painted);
    let base = vec4<f32>(texel.rgb * tint, texel.a * material.color.a);
    let alpha_mode = material.params.x;
    let alpha_cutoff = material.params.y;
    if alpha_mode > 0.5 && (alpha_mode < 1.5 || alpha_mode > 2.5) && base.a <= alpha_cutoff { discard; }
    let out_alpha = select(1.0, select(base.a, 1.0, painted), alpha_mode > 1.5);
    if material.params.w > 0.5 && material.params.w < 1.5 {
        // Track geometry: texture times its 8-bit vertex colour (the `df` word, or the
        // `LayerAmbient` constant), D3D COLOROP MODULATE (THRASH 0x29 = 8, set by every
        // single-texture draw 0x540ce0/0x543ae0/0x545ad0/0x5483c0/0x5492f0) on gamma values:
        // never brighter than the texture. No scale, full hue (re/lighting.md "Track lighting").
        return vec4<f32>(base.rgb * srgb_to_linear(in.color.rgb), out_alpha);
    }
    let normal = normalize(select(-in.normal, in.normal, front));
    if camera.sun.w > 0.5 {
        // Vertex colour min(1, ambient + diffuse·max(0, n·sun)) modulating the
        // texture in gamma space (approximation A23: per pixel, S = 1).
        let slot = select(0, 1, material.params.w > 1.5 && material.params.w < 2.5);
        let lit = min(camera.ambient[slot].rgb + camera.diffuse[slot].rgb * max(dot(normal, camera.sun.xyz), 0.0), vec3<f32>(1.0));
        var rgb = base.rgb * srgb_to_linear(lit);
        if material.params.w > 2.5 && (camera.specular.w > 0.5 || camera.env_layers.w > 0.5) {
            // Texture slot k = params.w − 3 (layer parameter of the material). Render state
            // blend field 0 = THRASH 0x38 SRCALPHA/INVSRCALPHA in every driver: the map is
            // alpha-blended over the paint on gamma-encoded values (frame buffer). The pass
            // draws with THRASH 0x29 = 8 (MODULATE texture × diffuse, 0x540ce0) and the
            // diffuse is the vertex colour pass 1's R3DLightLayer (0x446530) left in the
            // shared buffer, i.e. `lit`: colour `SPC.rgb · lit`, alpha `SPC.a` (vertex alpha
            // 255 · ambient.a = 1 on every track). camera.specular.x scales the alpha
            // (A71: user-preference strength, not from the original).
            let slot = clamp(round(material.params.w - 3.0), 0.0, 2.0);
            let texel = 0.5 / f32(textureDimensions(specular_map).y);
            let u = (slot + clamp(in.sphere.x, texel, 1.0 - texel)) / 3.0;
            let spc = textureSample(specular_map, specular_sampler, vec2<f32>(u, in.sphere.y));
            if camera.specular.w > 0.5 {
                let a = spc.a * camera.specular.x;
                rgb = srgb_to_linear(mix(linear_to_srgb(rgb), linear_to_srgb(spc.rgb) * lit, a));
            }
            // Pass 6: the planar sky map of the track section (tree-shade `TT*`, lamp
            // stars, black `1` outdoors), alpha-blended like the sphere map (A71).
            if camera.env_layers.w > 0.5 {
                let layer = camera.env_layers[u32(slot)];
                if layer >= 0.0 {
                    let env = textureSample(env_maps, env_sampler, in.planar, i32(layer));
                    // env_axis_u.w: debug alpha multiplier (1 = original).
                    let a = clamp(env.a * camera.env_axis_u.w, 0.0, 1.0);
                    rgb = srgb_to_linear(mix(linear_to_srgb(rgb), linear_to_srgb(env.rgb), a));
                }
            }
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

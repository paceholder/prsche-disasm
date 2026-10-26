//! Car glares and shadows (`docs/re/lighting.md`).
//!
//! CPU builders port the geometry of `R3DCar_DrawGlare` 0x43cdd0 and
//! `R3DCar_DrawShadow` 0x43b820 / `R3DCar_ShadowGrid` 0x43b5b0. The GPU side
//! draws them with blend and depth states that are not recovered
//! (approximations A20, A21).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use nfs_assets::lights::LightGroups;
use nfs_assets::{CarGlare, Texture};
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;

/// Glare types of the original table 0x5cd73c: directional flag and the two
/// `common.fsh` textures drawn per glare.
const GLARE_TYPES: [(bool, [&str; 2]); 11] = [
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (true, ["CARG", "GLNT"]),
    (true, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["CARG", "SPIK"]),
    (false, ["FEGL", ""]),
];
/// Glares at or below this intensity are not drawn (`0.1 < fVar9 * param_4`).
pub const GLARE_MIN_INTENSITY: f32 = 0.1;
/// Approximation A20: the original pulls glares towards the camera with a
/// z bias of `-20000·I/d²`; we move the sprite this far towards the eye.
const GLARE_EYE_OFFSET: f32 = 0.3;
/// Shadow texture in `common.fsh`.
pub const SHADOW_TEXTURE: &str = "SHAD";
/// Approximation A21: lift above the road instead of the original z bias.
const SHADOW_LIFT: f32 = 0.03;
/// The original drops shadow vertices from `car.y + 1.75` onto the road.
pub const SHADOW_PROBE_HEIGHT: f32 = 1.75;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct EffectVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Camera basis in world space.
#[derive(Clone, Copy, Debug)]
pub struct CameraFrame {
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

impl CameraFrame {
    pub fn look(eye: Vec3, target: Vec3) -> Self {
        let forward = (target - eye).normalize_or(Vec3::NEG_Z);
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        Self { eye, forward, right, up }
    }
}

/// One textured triangle list per effect texture.
#[derive(Default)]
pub struct EffectBatches {
    pub batches: BTreeMap<String, Vec<EffectVertex>>,
}

impl EffectBatches {
    fn push_quad(&mut self, texture: &str, corners: [Vec3; 4], uvs: [[f32; 2]; 4], color: [f32; 4]) {
        let out = self.batches.entry(texture.to_string()).or_default();
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(EffectVertex {
                position: corners[i].to_array(),
                uv: uvs[i],
                color,
            });
        }
    }
}

/// sRGB byte to linear (the target and textures are sRGB).
fn linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// ARGB colour of a glare pass as linear RGB.
pub fn argb_rgb(argb: u32) -> [f32; 3] {
    [linear((argb >> 16) as u8), linear((argb >> 8) as u8), linear(argb as u8)]
}

/// Intensity `I = f · level` of 0x43cdd0: `f` is the facing cosine between the
/// glare normal and the view direction, clamped to 1; directional types use
/// `(f − 0.95)·20`.
pub fn glare_intensity(kind: u8, normal_world: Vec3, camera_forward: Vec3, level: f32) -> f32 {
    let directional = GLARE_TYPES.get(kind as usize).is_some_and(|t| t.0);
    let mut f = (-camera_forward.dot(normal_world)).min(1.0);
    if directional {
        f = (f - 0.95) * 20.0;
    }
    f * level
}

/// Add the sprites of every lit glare of one car (model = car body to world).
pub fn push_car_glares(
    out: &mut EffectBatches,
    glares: &[CarGlare],
    lights: &LightGroups,
    model: Mat4,
    camera: &CameraFrame,
) {
    for glare in glares {
        if !lights.is_lit(glare.group) {
            continue;
        }
        let normal = model.transform_vector3(Vec3::from(glare.normal)).normalize_or_zero();
        let intensity = glare_intensity(glare.kind, normal, camera.forward, lights.level(glare.group));
        if intensity <= GLARE_MIN_INTENSITY {
            continue;
        }
        let center = model.transform_point3(Vec3::from(glare.position));
        let center = center + (camera.eye - center).normalize_or_zero() * GLARE_EYE_OFFSET;
        // Approximation A20: square sprites; the original stretches along the
        // projected B axis by |B|/|A| (about 1 for car lights).
        let half = Vec3::from(glare.axis_a).length() * intensity;
        let (r, u) = (camera.right * half, camera.up * half);
        let corners = [center - r - u, center + r - u, center + r + u, center - r + u];
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let textures = GLARE_TYPES.get(glare.kind as usize).map_or(["CARG", "SPIK"], |t| t.1);
        for (texture, argb) in textures.iter().zip(glare.colors) {
            if texture.is_empty() {
                continue;
            }
            let [cr, cg, cb] = argb_rgb(argb);
            out.push_quad(texture, corners, uvs, [cr, cg, cb, intensity.min(1.0)]);
        }
    }
}

/// Spark texture in `particle.fsh` (`CSPK` matid `KRPS`).
pub const SPARK_TEXTURE: &str = "SPRK";
/// Approximation A70: sparks have size 0 in `carpart.dat`; they are drawn as
/// streaks this wide, `SPARK_STREAK_TICKS` ticks of motion long, pulled
/// `GLARE_EYE_OFFSET` towards the eye for the original's z bias −40000.
const SPARK_WIDTH: f32 = 0.08;
const SPARK_STREAK_TICKS: f32 = 3.0;

/// Add a camera-facing streak per live spark (additive, like glares).
pub fn push_sparks(
    out: &mut EffectBatches,
    emitter: &nfs_assets::sparks::SparkEmitter,
    to_scene: impl Fn([f32; 3]) -> Vec3,
    camera: &CameraFrame,
) {
    for spark in emitter.sparks() {
        let back: [f32; 3] =
            std::array::from_fn(|k| spark.position[k] - spark.velocity[k] * SPARK_STREAK_TICKS);
        let pull = |p: Vec3| p + (camera.eye - p).normalize_or_zero() * GLARE_EYE_OFFSET;
        let head = pull(to_scene(spark.position));
        let tail = pull(to_scene(back));
        let along = head - tail;
        let view = (camera.eye - head).normalize_or_zero();
        let side = along.cross(view).normalize_or(camera.right) * (SPARK_WIDTH * 0.5);
        let [r, g, b, a] = emitter.color(spark);
        if a <= 0.0 {
            continue;
        }
        out.push_quad(
            SPARK_TEXTURE,
            [tail - side, head - side, head + side, tail + side],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [linear_f(r), linear_f(g), linear_f(b), a],
        );
    }
}

/// Camera-facing squares of a particle system (0x454430 with the sprite
/// expansion 0x44f6e0): `pos ± (right ± up)·size/2`, the camera axes rotated
/// by the particle rotation about the view axis. Blend mode 0 goes to the
/// alpha-blended batches, 1 and 2 to the additive ones (approximation A252:
/// the blend enum of the driver layer is not read; no z bias).
pub fn push_particles(
    alpha: &mut EffectBatches,
    additive: &mut EffectBatches,
    system: &nfs_assets::particles::ParticleSystem,
    to_scene: impl Fn([f32; 3]) -> Vec3,
    camera: &CameraFrame,
) {
    let def = &system.def;
    if !def.has_sprites() {
        return;
    }
    let texture = def.texture();
    let out = if def.blend_mode == 0 { alpha } else { additive };
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    for sprite in system.sprites() {
        let a = (sprite.color >> 24) as f32 / 255.0;
        if a <= 0.0 || sprite.size <= 0.0 {
            continue;
        }
        let center = to_scene(sprite.position);
        let (sin, cos) = sprite.rotation.to_radians().sin_cos();
        let half = sprite.size * 0.5;
        let r = (camera.right * cos + camera.up * sin) * half;
        let u = (camera.up * cos - camera.right * sin) * half;
        let [cr, cg, cb] = argb_rgb(sprite.color);
        out.push_quad(&texture, [center - r - u, center + r - u, center + r + u, center - r + u], uvs, [cr, cg, cb, a]);
    }
}

/// Snow and rain triangles (0x4565f0, 0x4564e0): camera-space vertices placed in the
/// world through the camera frame; white, blend mode 1 (additive batch, A252).
pub fn push_precipitation(
    out: &mut EffectBatches,
    system: &nfs_assets::precipitation::Precipitation,
    pose: &nfs_assets::precipitation::CameraPose,
    camera: &CameraFrame,
) {
    let list = out.batches.entry(system.config.texture.to_string()).or_default();
    for tri in system.triangles(pose) {
        for (p, uv) in tri {
            let position = camera.eye + camera.right * p[0] + camera.up * p[1] + camera.forward * p[2];
            list.push(EffectVertex { position: position.to_array(), uv, color: [1.0; 4] });
        }
    }
}

fn linear_f(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Shadow grid size of 0x43b820: `n = 6 − min(6, round(d²/100))`, at least 1.
pub fn shadow_subdivisions(camera_distance: f32) -> u32 {
    let steps = (camera_distance * camera_distance * 0.01).round().clamp(0.0, 6.0) as u32;
    (6 - steps).max(1)
}

/// Footprint corners of 0x43b220: `(±w, 0, ±l)` around the box centre in car
/// axes, in the original order (−x−z, +x−z, +x+z, −x+z).
pub fn shadow_corners(model: Mat4, box_center: Vec3, half_width: f32, half_length: f32) -> [Vec3; 4] {
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sz)| {
        model.transform_point3(box_center + Vec3::new(sx * half_width, 0.0, sz * half_length))
    })
}

/// `R3DCar_ShadowGrid` 0x43b5b0: an `(n+1)²` grid over corner0→corner1 (u)
/// and corner0→corner3 (v), each vertex placed on the road by `height(x, z)`.
pub fn push_car_shadow(
    out: &mut EffectBatches,
    corners: [Vec3; 4],
    n: u32,
    height: impl Fn(f32, f32) -> Option<f32>,
    fallback_y: f32,
) {
    let n = n.max(1);
    let du = corners[1] - corners[0];
    let dv = corners[3] - corners[0];
    let point = |i: u32, j: u32| {
        let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
        let p = corners[0] + du * u + dv * v;
        let y = height(p.x, p.z).unwrap_or(fallback_y) + SHADOW_LIFT;
        (Vec3::new(p.x, y, p.z), [u, v])
    };
    for j in 0..n {
        for i in 0..n {
            let quad = [point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)];
            out.push_quad(
                SHADOW_TEXTURE,
                quad.map(|q| q.0),
                quad.map(|q| q.1),
                [1.0; 4],
            );
        }
    }
}

pub(crate) const SHADER: &str = r#"
struct Uniforms { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs_main(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var out: VOut;
    out.clip = u.view_projection * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.color = color;
    return out;
}
// Additive glow: texture colour times glare colour, weighted by texture and glare alpha.
@fragment fn fs_glare(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    return vec4<f32>(t.rgb * in.color.rgb * t.a * in.color.a, 0.0);
}
@fragment fn fs_shadow(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    return vec4<f32>(t.rgb * in.color.rgb, t.a * in.color.a);
}
"#;

pub struct Effects {
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: BTreeMap<String, wgpu::BindGroup>,
    glare_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    vertex_buffer: Option<(wgpu::Buffer, usize)>,
}

impl Effects {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("effects uniform"),
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effects uniform"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effects sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("effects shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effects"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<EffectVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        Self {
            glare_pipeline: pipeline("fs_glare", additive),
            shadow_pipeline: pipeline("fs_shadow", wgpu::BlendState::ALPHA_BLENDING),
            uniform,
            uniform_group,
            texture_layout,
            sampler,
            textures: BTreeMap::new(),
            vertex_buffer: None,
        }
    }

    /// Layout of the per-texture bind groups (texture, sampler; fragment stage).
    pub(crate) fn texture_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_layout
    }

    /// Bind group of an effect texture by its exact FSH name.
    pub(crate) fn texture_group(&self, name: &str) -> Option<&wgpu::BindGroup> {
        self.textures.get(name)
    }

    /// Upload the effect textures of a car scene (`Scene::effect_textures`).
    pub fn set_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &[Texture]) {
        self.textures.clear();
        for texture in textures {
            if texture.width == 0
                || texture.height == 0
                || texture.rgba.len() as u64 != u64::from(texture.width) * u64::from(texture.height) * 4
            {
                continue;
            }
            let view = crate::renderer::upload_texture(device, queue, texture.width, texture.height, &texture.rgba);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&texture.name),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.textures.insert(texture.name.clone(), group);
        }
    }

    /// Draw shadows (alpha blended) and glares (additive) into `target`,
    /// depth-tested against the scene without writing depth.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view_projection: Mat4,
        shadows: &EffectBatches,
        glares: &EffectBatches,
    ) {
        let mut vertices: Vec<EffectVertex> = Vec::new();
        let mut draws = Vec::new();
        for (is_glare, batches) in [(false, shadows), (true, glares)] {
            for (name, list) in &batches.batches {
                if list.is_empty() || !self.textures.contains_key(name) {
                    continue;
                }
                let start = vertices.len() as u32;
                vertices.extend_from_slice(list);
                draws.push((is_glare, name.clone(), start..vertices.len() as u32));
            }
        }
        if draws.is_empty() {
            return;
        }
        let needed = vertices.len();
        if self.vertex_buffer.as_ref().is_none_or(|(_, capacity)| *capacity < needed) {
            let capacity = needed.next_power_of_two().max(1024);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("effects vertices"),
                size: (capacity * std::mem::size_of::<EffectVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_buffer = Some((buffer, capacity));
        }
        let (buffer, _) = self.vertex_buffer.as_ref().unwrap();
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&view_projection.to_cols_array()));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("car effects"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (is_glare, name, range) in draws {
                pass.set_pipeline(if is_glare { &self.glare_pipeline } else { &self.shadow_pipeline });
                pass.set_bind_group(1, &self.textures[&name], &[]);
                pass.draw(range, 0..1);
            }
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_assets::lights::LightInputs;

    fn brake_glare() -> CarGlare {
        CarGlare {
            name: "GlareBrake".into(),
            position: [-0.5, 0.5, 1.7],
            axis_a: [0.0, 0.2, 0.0],
            axis_b: [0.2, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            colors: [0xffff0000; 2],
            kind: 1,
            group: 2,
            beam: false,
            pivot: 0,
        }
    }

    fn braking() -> LightGroups {
        let mut lights = LightGroups::default();
        lights.update(&LightInputs { brake: 255, ..Default::default() }, 0, false);
        lights
    }

    #[test]
    fn subdivision_follows_camera_distance() {
        assert_eq!(shadow_subdivisions(3.0), 6);
        assert_eq!(shadow_subdivisions(15.0), 4);
        assert_eq!(shadow_subdivisions(30.0), 1);
        assert_eq!(shadow_subdivisions(0.0), 6);
    }

    #[test]
    fn corners_span_the_box_footprint() {
        let c = shadow_corners(Mat4::from_translation(Vec3::new(10.0, 2.0, 0.0)), Vec3::ZERO, 0.9, 2.0);
        assert_eq!(c[0], Vec3::new(9.1, 2.0, -2.0));
        assert_eq!(c[2], Vec3::new(10.9, 2.0, 2.0));
    }

    #[test]
    fn shadow_grid_lies_on_the_road() {
        let mut out = EffectBatches::default();
        let corners = shadow_corners(Mat4::IDENTITY, Vec3::ZERO, 1.0, 2.0);
        push_car_shadow(&mut out, corners, 4, |x, _| Some(0.1 * x), 0.0);
        let v = &out.batches[SHADOW_TEXTURE];
        assert_eq!(v.len(), 4 * 4 * 6);
        for p in v {
            assert!((p.position[1] - (0.1 * p.position[0] + SHADOW_LIFT)).abs() < 1e-6);
        }
    }

    #[test]
    fn brake_glare_shows_from_behind_only() {
        let lights = braking();
        let glare = brake_glare();
        // Camera behind the car (+z), looking forward (-z).
        let behind = CameraFrame::look(Vec3::new(0.0, 1.5, 8.0), Vec3::ZERO);
        let mut out = EffectBatches::default();
        push_car_glares(&mut out, &[glare.clone()], &lights, Mat4::IDENTITY, &behind);
        assert_eq!(out.batches["CARG"].len(), 6);
        assert_eq!(out.batches["SPIK"].len(), 6);
        let front = CameraFrame::look(Vec3::new(0.0, 1.5, -8.0), Vec3::ZERO);
        let mut out = EffectBatches::default();
        push_car_glares(&mut out, &[glare], &lights, Mat4::IDENTITY, &front);
        assert!(out.batches.is_empty());
    }

    #[test]
    fn unlit_group_draws_nothing() {
        let behind = CameraFrame::look(Vec3::new(0.0, 0.5, 8.0), Vec3::new(0.0, 0.5, 0.0));
        let mut out = EffectBatches::default();
        push_car_glares(&mut out, &[brake_glare()], &LightGroups::default(), Mat4::IDENTITY, &behind);
        assert!(out.batches.is_empty());
    }

    #[test]
    fn intensity_and_directional_types() {
        let n = Vec3::Z;
        let f = Vec3::NEG_Z;
        assert_eq!(glare_intensity(1, n, f, 0.6), 0.6);
        // Directional (type 4): only within about 18 degrees of the normal.
        assert!((glare_intensity(4, n, f, 1.0) - 1.0).abs() < 1e-6);
        let off = Vec3::new(0.5, 0.0, -1.0).normalize();
        assert!(glare_intensity(4, n, off, 1.0) < 0.0);
        assert!(glare_intensity(1, n, off, 1.0) > 0.8);
    }

    #[test]
    fn sprite_size_scales_with_intensity() {
        let lights = braking();
        let behind = CameraFrame::look(Vec3::new(-0.5, 0.5, 9.0), Vec3::new(-0.5, 0.5, 1.7));
        let mut out = EffectBatches::default();
        push_car_glares(&mut out, &[brake_glare()], &lights, Mat4::IDENTITY, &behind);
        let v = &out.batches["CARG"];
        let width = Vec3::from(v[1].position).distance(Vec3::from(v[0].position));
        assert!((width - 0.4).abs() < 1e-3, "2·|A|·I = 0.4, got {width}");
        assert_eq!(v[0].color[3], 1.0);
        assert!(v[0].color[0] > 0.99 && v[0].color[1] == 0.0);
    }

    #[test]
    fn particle_sprites_face_the_camera_with_their_size() {
        use nfs_assets::particles::*;
        let def = ParticleDef {
            tag: *b"WF06", flags: 0, count: 40, emit: [0.0; 3], rand_emit: [0.0; 3], air_friction: 1.0,
            material: *b"ktss", blend_mode: 0, z_bias: 0, num_frames: 1, frame_speed: 10, num_states: 2,
            start_size: 1.0, end_size: 1.0, lifetime: 220, period_on: 100, period_off: 0, prime_ticks: 0,
            weight: 0.0, rand_weight: 0.0, rand_rotation: 0.0, fade_fraction: 1.0, colors: [0xffffffff, 0x00ffffff, 0, 0],
        };
        let mut s = ParticleSystem::new(def, EmitterFrame::default(), DETAIL_FULL, 0.0);
        s.particles.push(Particle { position: [0.0; 3], velocity: [0.0; 3], age: 0.0, gravity: 0.0, rotation: 0.0 });
        let camera = CameraFrame::look(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO);
        let (mut alpha, mut add) = (EffectBatches::default(), EffectBatches::default());
        push_particles(&mut alpha, &mut add, &s, |p| Vec3::from(p), &camera);
        assert!(add.batches.is_empty());
        let v = &alpha.batches["sstk"];
        assert_eq!(v.len(), 6);
        let width = Vec3::from(v[1].position).distance(Vec3::from(v[0].position));
        assert!((width - 10.0).abs() < 1e-4, "size 1.0 in the .dat is 10 units");
    }
}

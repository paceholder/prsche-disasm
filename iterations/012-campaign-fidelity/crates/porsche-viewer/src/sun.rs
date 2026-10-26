//! Sun sprite and lens flare (`docs/re/lighting.md`, "Sun and lens flare").
//!
//! CPU part: the screen geometry of `Flare_DrawSprite` 0x451710 for the records of
//! `Render/flare.dat`, the sun's screen position (`Sun_Draw` 0x460290) and the flare
//! layout (`Flare_Draw` 0x460460). The GPU part is in `effects.rs`.

use glam::{Mat4, Vec3, Vec4};
use nfs_formats::flare::{FlareData, FlareRecord, FLARE_FLAG_ROTATE};

/// Tag of the sun record (`SunFlare_Init` 0x460780 string table, first entry).
pub const SUN_TAG: &str = "suna";
/// Flare sprite `i` (1..13) uses record `sun` + letter `b`..`n`.
pub const FLARE_SPRITES: usize = 13;
/// `Flare_DrawSprite` skips everything below this intensity.
pub const MIN_INTENSITY: f32 = 0.05;
/// `SunFlare_Init` 0x460780: the sun counts as overhead (no sprite, no flare) when both
/// `Sky/<track>.bin` offsets `+0xcdc`, `+0xce0` are within ±0.02. Then the sun is at
/// x = z = 0; the shipped tracks with a sun have offsets of 26 or more, so the
/// horizontal distance of the sun point decides it here.
pub const OVERHEAD_HORIZONTAL: f32 = 1e-3;
/// Far plane of the sky camera (`SkyManager_Draw` 0x45d4f0: near/far 0.15/700);
/// `Sun_Draw` needs the projected sun in front of it (`z < 1`).
pub const SKY_FAR: f32 = 700.0;

/// One textured screen quad in pixels (y down), corners in the order of the original
/// vertex list with uv (0,0), (1,0), (1,1), (0,1).
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSprite {
    pub texture: String,
    pub corners: [[f32; 2]; 4],
    /// Linear RGB and alpha of the record colour (alpha already scaled by the intensity
    /// when it is below 1, as the original does).
    pub color: [f32; 4],
}

/// Approximation A272: records `sunb`, `sunc`, `sune`, `suni` name a texture `lens`
/// that is in no shipped pack (the flare textures are looked up first in the track's sky
/// FSH, `FlareDb_ResolveTextures` 0x452120 → 0x460b10, then in the loaded packs by
/// FourCC, 0x44d320); the original then drops those records. The ring `FLR1` of
/// `common.fsh`, used by no record, stands in, so the flare shows its ring ghosts.
pub const MISSING_LENS_STAND_IN: (&str, &str) = ("lens", "FLR1");

pub const SPRITE_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// `Flare_DrawSprite` 0x451710: for each texture pass of `record`, a quad of half
/// extent `size·0.1` pixels of a 640×480 screen (scaled to `width × height`) around
/// `center`, turned by `(x + y)/1440` turns when the record has flag 4; the colour's
/// alpha is multiplied by `intensity` when that is below 1; nothing below 0.05.
/// Blink timing (`+0x50..+0x58`) is not applied (the sun records have none).
pub fn record_sprites(
    record: &FlareRecord,
    center: [f32; 2],
    intensity: f32,
    width: f32,
    height: f32,
) -> Vec<ScreenSprite> {
    if !(intensity >= MIN_INTENSITY) {
        return Vec::new();
    }
    let turns = if record.flags & FLARE_FLAG_ROTATE != 0 { (center[0] + center[1]) / 1440.0 } else { 0.0 };
    let (sin, cos) = (turns * std::f32::consts::TAU).sin_cos();
    let mut out = Vec::new();
    for pass in 0..2 {
        let Some(texture) = &record.textures[pass] else {
            continue;
        };
        let texture = if texture == MISSING_LENS_STAND_IN.0 { MISSING_LENS_STAND_IN.1 } else { texture.as_str() };
        let argb = record.colors[pass];
        let mut alpha = (argb >> 24) as f32 / 255.0;
        if intensity < 1.0 {
            alpha = ((argb >> 24) as f32 * intensity).trunc() / 255.0;
        }
        let size = record.sizes[pass] * 0.1;
        let (hx, hy) = (width * size / 640.0, height * size / 480.0);
        let corners = [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]].map(|[x, y]| {
            [x * cos + y * sin + center[0], y * cos - x * sin + center[1]]
        });
        out.push(ScreenSprite {
            texture: texture.to_string(),
            corners,
            color: [
                srgb_to_linear((argb >> 16) as u8),
                srgb_to_linear((argb >> 8) as u8),
                srgb_to_linear(argb as u8),
                alpha,
            ],
        });
    }
    out
}

/// Whether the track has a sun to draw (`SunFlare_Init` 0x460780 `+0x140`).
pub fn sun_shown(sun: Vec3) -> bool {
    Vec3::new(sun.x, 0.0, sun.z).length() > OVERHEAD_HORIZONTAL
}

/// `Sun_Draw` 0x460290: the sun point (sky camera at the origin, rotation only) on
/// screen in pixels, if it projects inside `[0, w) × [0, h)` in front of the camera.
pub fn sun_screen(sun: Vec3, sky_view_projection: Mat4, width: f32, height: f32) -> Option<[f32; 2]> {
    if sun.length() >= SKY_FAR {
        return None;
    }
    let clip = sky_view_projection * Vec4::new(sun.x, sun.y, sun.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let x = (clip.x / clip.w * 0.5 + 0.5) * width;
    let y = (0.5 - clip.y / clip.w * 0.5) * height;
    ((0.0..width).contains(&x) && (0.0..height).contains(&y)).then_some([x, y])
}

/// The sun sprite (record `suna` at the sun, intensity 1).
pub fn sun_sprites(flare: &FlareData, sun_px: [f32; 2], width: f32, height: f32) -> Vec<ScreenSprite> {
    flare
        .find(SUN_TAG)
        .map(|r| record_sprites(r, sun_px, 1.0, width, height))
        .unwrap_or_default()
}

/// `Flare_Draw` 0x460460: sprites `sunb`..`sunn` at `centre + (sun − centre)·t[i]`,
/// drawn at full intensity here; the visibility (matching pixels / 16) scales their
/// alpha on the GPU. Records or textures that are missing are skipped.
pub fn flare_sprites(flare: &FlareData, sun_px: [f32; 2], width: f32, height: f32) -> Vec<ScreenSprite> {
    let (cx, cy) = (width * 0.5, height * 0.5);
    let mut out = Vec::new();
    for i in 1..=FLARE_SPRITES {
        let tag = format!("sun{}", (b'a' + i as u8) as char);
        let Some(record) = flare.find(&tag) else {
            continue;
        };
        let t = flare.positions[i];
        let at = [(sun_px[0] - cx) * t + cx, (sun_px[1] - cy) * t + cy];
        out.extend(record_sprites(record, at, 1.0, width, height));
    }
    out
}

// --- GPU ---

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub(crate) const SHADER: &str = r#"
// size = render target in pixels, sun = sun position in pixels (y down).
struct Screen { size: vec2<f32>, sun: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
fn to_clip(p: vec2<f32>, z: f32) -> vec4<f32> {
    return vec4<f32>(p.x / screen.size.x * 2.0 - 1.0, 1.0 - p.y / screen.size.y * 2.0, z, 1.0);
}
// The sun is part of the sky: drawn at the far plane right after the sky dome.
@vertex fn vs_sun(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var out: VOut;
    out.clip = to_clip(pos, 1.0);
    out.uv = uv;
    out.color = color;
    return out;
}
// Additive (render state blend 1, as the car glares; approximation A20): texture
// times record colour, weighted by texture and record alpha, added in linear space
// (the original adds gamma-space values; approximation A270).
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    return vec4<f32>(t.rgb * in.color.rgb * t.a * in.color.a, 0.0);
}
fn to_gamma(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), c * 12.92, c <= vec3<f32>(0.0031308));
}
// The flare draws into the non-sRGB view of the target, so the add happens on
// gamma-encoded values as in the original (texture and colour back to gamma).
@fragment fn fs_gamma(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    return vec4<f32>(to_gamma(t.rgb * in.color.rgb) * t.a * in.color.a, 0.0);
}"#;

pub(crate) const FLARE_SHADER: &str = r#"
@group(0) @binding(1) var depth: texture_depth_2d;
// Flare_Draw 0x460460: the share of the 4×4 pixels at the sun that still show the
// sun (here: nothing drawn over the sky there, depth at the far plane) is the
// intensity; Flare_DrawSprite scales alpha below 1 and skips below 0.05.
@vertex fn vs_flare(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    let dims = vec2<i32>(textureDimensions(depth));
    let base = vec2<i32>(round(screen.sun)) - vec2<i32>(2, 2);
    var count = 0.0;
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let p = base + vec2<i32>(i, j);
            if all(p >= vec2<i32>(0, 0)) && all(p < dims) && textureLoad(depth, p, 0) >= 1.0 {
                count += 1.0;
            }
        }
    }
    let intensity = count / 16.0;
    var out: VOut;
    out.clip = to_clip(pos, 0.0);
    if intensity < 0.05 {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    out.uv = uv;
    out.color = vec4<f32>(color.rgb, color.a * min(intensity, 1.0));
    return out;
}
"#;

/// Sun sprite and lens flare on the GPU. The sun is drawn inside the main pass
/// after the sky (`draw_sun`), the flare afterwards over the finished frame
/// (`draw_flare`), reading the depth buffer for its visibility.
pub struct SunFlareGpu {
    uniform: wgpu::Buffer,
    sun_group: wgpu::BindGroup,
    flare_layout: wgpu::BindGroupLayout,
    sun_pipeline: wgpu::RenderPipeline,
    flare_pipeline: wgpu::RenderPipeline,
    vertex_buffer: Option<(wgpu::Buffer, usize)>,
    /// Texture and vertex range of each sprite, sun first.
    sun_draws: Vec<(String, std::ops::Range<u32>)>,
    flare_draws: Vec<(String, std::ops::Range<u32>)>,
}

impl SunFlareGpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, texture_layout: &wgpu::BindGroupLayout) -> Self {
        use wgpu::util::DeviceExt;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sun uniform"),
            contents: bytemuck::cast_slice(&[0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sun_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun"),
            entries: &[uniform_entry],
        });
        let flare_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens flare"),
            entries: &[
                uniform_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let sun_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun"),
            layout: &sun_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let sun_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sun shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let flare_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lens flare shader"),
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{FLARE_SHADER}").into()),
        });
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
        let pipeline = |label: &str,
                        group0: &wgpu::BindGroupLayout,
                        shader: &wgpu::ShaderModule,
                        vs: &str,
                        fs: &str,
                        target: wgpu::TextureFormat,
                        depth: Option<wgpu::DepthStencilState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(group0), Some(texture_layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<ScreenVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: Some(additive),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let sun_pipeline = pipeline(
            "sun",
            &sun_layout,
            &sun_shader,
            "vs_sun",
            "fs_main",
            format,
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        );
        let flare_pipeline = pipeline("lens flare", &flare_layout, &flare_shader, "vs_flare", "fs_gamma", format.remove_srgb_suffix(), None);
        Self {
            uniform,
            sun_group,
            flare_layout,
            sun_pipeline,
            flare_pipeline,
            vertex_buffer: None,
            sun_draws: Vec::new(),
            flare_draws: Vec::new(),
        }
    }

    /// Upload this frame's sprites; `sun_px` is the sun on screen. Empty lists draw nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        sun_px: [f32; 2],
        sun: &[ScreenSprite],
        flare: &[ScreenSprite],
    ) {
        self.sun_draws.clear();
        self.flare_draws.clear();
        let mut vertices: Vec<ScreenVertex> = Vec::new();
        for (sprites, draws) in [(sun, &mut self.sun_draws), (flare, &mut self.flare_draws)] {
            for sprite in sprites {
                let start = vertices.len() as u32;
                for i in [0, 1, 2, 0, 2, 3] {
                    vertices.push(ScreenVertex { position: sprite.corners[i], uv: SPRITE_UVS[i], color: sprite.color });
                }
                draws.push((sprite.texture.clone(), start..vertices.len() as u32));
            }
        }
        if vertices.is_empty() {
            return;
        }
        if self.vertex_buffer.as_ref().is_none_or(|(_, capacity)| *capacity < vertices.len()) {
            let capacity = vertices.len().next_power_of_two().max(64);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sun vertices"),
                size: (capacity * std::mem::size_of::<ScreenVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_buffer = Some((buffer, capacity));
        }
        let (buffer, _) = self.vertex_buffer.as_ref().unwrap();
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[width, height, sun_px[0], sun_px[1]]));
    }

    /// Draw the sun sprite into the main pass (right after the sky).
    pub fn draw_sun(&self, pass: &mut wgpu::RenderPass<'_>, effects: &crate::effects::Effects) {
        let Some((buffer, _)) = &self.vertex_buffer else {
            return;
        };
        if self.sun_draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.sun_pipeline);
        pass.set_bind_group(0, &self.sun_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (texture, range) in &self.sun_draws {
            if let Some(group) = effects.texture_group(texture) {
                pass.set_bind_group(1, group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
    }

    /// Draw the lens flare over the frame; `depth` is the scene depth (sampled).
    pub fn draw_flare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        effects: &crate::effects::Effects,
    ) {
        let Some((buffer, _)) = &self.vertex_buffer else {
            return;
        };
        if self.flare_draws.is_empty() {
            return;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lens flare"),
            layout: &self.flare_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        });
        // Non-sRGB view of the same texture (targets allow it as a view format).
        let texture = target.texture();
        let raw = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(texture.format().remove_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lens flare"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &raw,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.flare_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (texture, range) in &self.flare_draws {
                if let Some(tex) = effects.texture_group(texture) {
                    pass.set_bind_group(1, tex, &[]);
                    pass.draw(range.clone(), 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_formats::flare::DEFAULT_FLARE_POSITIONS;

    fn rec(tag: &str, textures: [Option<&str>; 2], colors: [u32; 2], sizes: [f32; 2]) -> FlareRecord {
        FlareRecord {
            tag: tag.into(),
            textures: textures.map(|t| t.map(String::from)),
            colors,
            sizes,
            flags: FLARE_FLAG_ROTATE,
            blink: [0; 3],
        }
    }

    #[test]
    fn sprite_size_scales_with_the_screen() {
        let r = rec("suna", [Some("SUNA"), None], [0xffff_ffff, 0], [200.0, 0.0]);
        // At the origin there is no turn: half extent 20 px on 640×480, 40×20 on 1280×240.
        let s = record_sprites(&r, [0.0, 0.0], 1.0, 640.0, 480.0);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].corners[0], [-20.0, -20.0]);
        assert_eq!(s[0].corners[2], [20.0, 20.0]);
        let s = record_sprites(&r, [0.0, 0.0], 1.0, 1280.0, 240.0);
        assert_eq!(s[0].corners[2], [40.0, 10.0]);
    }

    #[test]
    fn sprite_turns_with_the_screen_position() {
        let r = rec("suna", [Some("SUNA"), None], [0xffff_ffff, 0], [100.0, 0.0]);
        // (x + y)/1440 = 0.25 turns at (180, 180): the corner (−10, −10) turns a quarter.
        let s = record_sprites(&r, [180.0, 180.0], 1.0, 640.0, 480.0);
        let [x, y] = s[0].corners[0];
        assert!((x - (180.0 - 10.0)).abs() < 1e-3 && (y - (180.0 + 10.0)).abs() < 1e-3, "{x} {y}");
    }

    #[test]
    fn intensity_scales_alpha_below_one_and_hides_below_005() {
        let r = rec("sund", [Some("FLR0"), None], [0x68a9_ffff, 0], [33.0, 0.0]);
        let full = record_sprites(&r, [0.0, 0.0], 1.0, 640.0, 480.0);
        assert!((full[0].color[3] - 0x68 as f32 / 255.0).abs() < 1e-6);
        let half = record_sprites(&r, [0.0, 0.0], 0.5, 640.0, 480.0);
        assert!((half[0].color[3] - 52.0 / 255.0).abs() < 1e-6);
        assert!(record_sprites(&r, [0.0, 0.0], 0.04, 640.0, 480.0).is_empty());
        // Colour channels: ARGB a9/ff/ff as linear rgb.
        assert!((half[0].color[0] - srgb_to_linear(0xa9)).abs() < 1e-6);
    }

    #[test]
    fn flare_sprites_lie_on_the_line_through_the_centre() {
        let flare = FlareData {
            records: vec![
                rec("sund", [Some("FLR0"), None], [0xffff_ffff, 0], [10.0, 0.0]),
                rec("suni", [Some("lens"), None], [0xffff_ffff, 0], [10.0, 0.0]),
            ],
            positions: DEFAULT_FLARE_POSITIONS,
        };
        // Sun 100 px right of and 80 px above the centre of 640×480.
        let s = flare_sprites(&flare, [420.0, 160.0], 640.0, 480.0);
        let centre = |q: &ScreenSprite| [
            q.corners.iter().map(|c| c[0]).sum::<f32>() / 4.0,
            q.corners.iter().map(|c| c[1]).sum::<f32>() / 4.0,
        ];
        assert_eq!(s.len(), 2);
        // sund: t = 0.5; suni: t = −1 (mirror point), its `lens` drawn with FLR1 (A272).
        assert_eq!(s[1].texture, "FLR1");
        let a = centre(&s[0]);
        let b = centre(&s[1]);
        assert!((a[0] - 370.0).abs() < 1e-3 && (a[1] - 200.0).abs() < 1e-3, "{a:?}");
        assert!((b[0] - 220.0).abs() < 1e-3 && (b[1] - 320.0).abs() < 1e-3, "{b:?}");
    }

    #[test]
    fn sun_projects_only_in_front_and_on_screen() {
        let vp = Mat4::perspective_rh(1.0, 4.0 / 3.0, 0.15, 700.0) * Mat4::IDENTITY;
        // Straight ahead (−z): screen centre.
        let p = sun_screen(Vec3::new(0.0, 0.0, -100.0), vp, 640.0, 480.0).unwrap();
        assert!((p[0] - 320.0).abs() < 1e-3 && (p[1] - 240.0).abs() < 1e-3);
        // Above the view axis: upper half (y down).
        assert!(sun_screen(Vec3::new(0.0, 20.0, -100.0), vp, 640.0, 480.0).unwrap()[1] < 240.0);
        assert!(sun_screen(Vec3::new(0.0, 0.0, 100.0), vp, 640.0, 480.0).is_none());
        assert!(sun_screen(Vec3::new(500.0, 0.0, -10.0), vp, 640.0, 480.0).is_none());
        assert!(!sun_shown(Vec3::new(0.0, 44.0, 0.0)));
        assert!(sun_shown(Vec3::new(134.6, 96.3, -280.6)));
    }
}

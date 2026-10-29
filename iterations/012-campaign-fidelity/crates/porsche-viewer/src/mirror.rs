//! Rear-view mirror (`HRearview`, HUD slot S10): a second 3D view of the scene from the player
//! car looking back, drawn into the mirror box of the HUD. The original renders it into its own
//! viewport with the view `0x628144` (`0x4691d0` from `HRearview` 0x42c510, built per frame by
//! `0x468ca0`) and the opposite cull mode of the main view (`0x468ba0` vs `0x468ca0`), i.e. a
//! mirrored image. Here the scene is rendered into an offscreen texture of the box size and copied
//! into the frame with a left-right flip (`docs/re/hud.md` "Rear-view mirror").

/// Blit shader: one full-viewport triangle sampling the mirror texture, u flipped.
pub(crate) const SHADER: &str = r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var out: VsOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(1.0 - x, y);
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(image, image_sampler, input.uv).rgb, 1.0);
}
"#;

/// Offscreen colour and depth targets of the mirror view and the blit that copies it.
pub(crate) struct MirrorTarget {
    pub width: u32,
    pub height: u32,
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

pub(crate) struct MirrorBlit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl MirrorBlit {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mirror blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror blit"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mirror blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mirror blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirror blit"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self { pipeline, layout, sampler }
    }

    /// Offscreen targets of `width` x `height` pixels in the frame's colour format.
    pub fn target(&self, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> MirrorTarget {
        let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
        let color = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("mirror colour"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("mirror depth"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirror blit"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        MirrorTarget { width, height, color, depth, bind_group }
    }

    /// Copies the mirror image into `target` at the pixel box `rect` = [x, y, w, h].
    pub fn draw(&self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView, mirror: &MirrorTarget, rect: [u32; 4], frame: (u32, u32)) {
        let [x, y, w, h] = rect;
        let (fw, fh) = frame;
        if x >= fw || y >= fh {
            return;
        }
        let w = w.min(fw - x);
        let h = h.min(fh - y);
        if w == 0 || h == 0 {
            return;
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirror blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            // WebGPU keeps the viewport inside the target: a clipped box is squeezed, not cut.
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &mirror.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

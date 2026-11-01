//! Player car in BumperCam2: the in-car parts of the CRP (`Scene::car_in_car`) with the
//! steering wheel, needles and hand frames driven by `nfs_assets::cockpit` (`docs/re/cockpit.md`).
use crate::renderer::{GpuMesh, GpuVertex};
use glam::{Mat4, Vec3};
use nfs_assets::cockpit::CockpitChannels;
use nfs_assets::{InCarPart, InCarPass};
use std::cell::Cell;
use wgpu::util::DeviceExt;

/// Scene → car mesh space, as `Renderer::set_car_model` does for the parked meshes.
const CAR_SCALE: f32 = 5.0;
fn body(p: [f32; 3]) -> [f32; 3] {
    [-p[0] * CAR_SCALE, p[1] * CAR_SCALE, -p[2] * CAR_SCALE]
}
fn body_dir(n: [f32; 3]) -> [f32; 3] {
    [-n[0], n[1], -n[2]]
}

/// Sense of the channel rotation in car mesh space. Raw CRP → original world is a proper
/// rotation (`R3DObject_Draw` 0x4432b0 scales by (−1, 1, −1)); car mesh space mirrors x
/// only, which reverses the rotation sense. Checked on screenshots: the rev needle rests on
/// the gauge zero and turns clockwise, the steering wheel turns with the road wheels.
const ROTATION_SENSE: f32 = -1.0;

pub(crate) struct InCarPartRender {
    pub(crate) pass: InCarPass,
    channel: u8,
    pivot_origin: Vec3,
    pivot_axis: Vec3,
    frame_channel: u8,
    frame_count: u8,
    pub(crate) meshes: Vec<GpuMesh>,
    /// Per mesh, per frame: vertices in car mesh space.
    frames: Vec<Vec<Vec<GpuVertex>>>,
    shown_frame: Cell<f32>,
    /// Damage morph (`R3DCarLOD_ApplyDamageMorph` 0x43e390) and the drawn vertices per mesh.
    damage: Option<(nfs_assets::car_damage::CarDamageModel, nfs_assets::InCarDamage, Vec<Vec<GpuVertex>>)>,
}

impl InCarPartRender {
    /// Mesh transform of the part (rotation about its pivot by the channel, in turns).
    pub(crate) fn matrix(&self, channels: &CockpitChannels) -> Mat4 {
        let turns = channels.rotation(self.channel);
        if turns == 0.0 || self.pivot_axis.length_squared() < 0.5 {
            return Mat4::IDENTITY;
        }
        Mat4::from_translation(self.pivot_origin)
            * Mat4::from_axis_angle(
                self.pivot_axis,
                ROTATION_SENSE * turns * std::f32::consts::TAU,
            )
            * Mat4::from_translation(-self.pivot_origin)
    }

    /// 0x438230 for this part on the car's zones: rewrites its meshes when they moved. Called
    /// every frame with the zones of the car, so its cache follows the exterior model's.
    pub(crate) fn update_damage(
        &mut self,
        queue: &wgpu::Queue,
        zones: &[f32; 10],
        view: nfs_assets::car_damage::DamageView,
    ) {
        let Some((model, data, drawn)) = &mut self.damage else {
            return;
        };
        if !model.update(zones, view, 2, false).is_some_and(|w| !w.is_empty()) {
            return;
        }
        let vertices = &model.parts[0].vertices;
        for (k, (mesh, verts)) in self.meshes.iter().zip(drawn.iter_mut()).enumerate() {
            for (v, p) in verts.iter_mut().zip(data.mesh_positions(k, vertices)) {
                v.position = body(p);
            }
            queue.write_buffer(&mesh.vertex, 0, bytemuck::cast_slice(verts));
        }
    }

    /// Uploads the blend of the two frames around the channel frame (approximation A61:
    /// linear blend assumed) when it changed.
    pub(crate) fn update_frames(&self, queue: &wgpu::Queue, channels: &CockpitChannels) {
        if self.frame_count < 2 {
            return;
        }
        let f = channels.frame(self.frame_channel, self.frame_count);
        if f == self.shown_frame.get() {
            return;
        }
        self.shown_frame.set(f);
        let a = (f.floor() as usize).min(self.frame_count as usize - 1);
        let b = (a + 1).min(self.frame_count as usize - 1);
        let t = f - a as f32;
        for (mesh, frames) in self.meshes.iter().zip(&self.frames) {
            let (Some(fa), Some(fb)) = (frames.get(a), frames.get(b)) else {
                continue;
            };
            let blended: Vec<GpuVertex> = fa
                .iter()
                .zip(fb)
                .map(|(va, vb)| {
                    let lerp = |x: [f32; 3], y: [f32; 3]| -> [f32; 3] {
                        std::array::from_fn(|i| x[i] + (y[i] - x[i]) * t)
                    };
                    let n = Vec3::from(lerp(va.normal, vb.normal)).normalize_or_zero();
                    GpuVertex {
                        position: lerp(va.position, vb.position),
                        normal: n.into(),
                        uv: va.uv,
                        color: va.color,
                    }
                })
                .collect();
            queue.write_buffer(&mesh.vertex, 0, bytemuck::cast_slice(&blended));
        }
    }
}

pub(crate) fn build(device: &wgpu::Device, parts: &[InCarPart]) -> Vec<InCarPartRender> {
    parts
        .iter()
        .map(|part| {
            let mut meshes = Vec::new();
            let mut frames = Vec::new();
            let mut drawn = Vec::new();
            for m in &part.meshes {
                let vertices: Vec<GpuVertex> = m
                    .mesh
                    .vertices
                    .iter()
                    .map(|v| GpuVertex {
                        position: body(v.position),
                        normal: body_dir(v.normal),
                        uv: v.uv,
                        color: v.color,
                    })
                    .collect();
                let per_frame: Vec<Vec<GpuVertex>> = m
                    .frames
                    .iter()
                    .map(|f| {
                        f.iter()
                            .zip(&vertices)
                            .map(|((p, n), v)| GpuVertex {
                                position: body(*p),
                                normal: body_dir(*n),
                                ..*v
                            })
                            .collect()
                    })
                    .collect();
                let center = vertices
                    .iter()
                    .map(|v| Vec3::from(v.position))
                    .sum::<Vec3>()
                    / vertices.len().max(1) as f32;
                meshes.push(GpuMesh {
                    vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&m.mesh.name),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    }),
                    index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&m.mesh.name),
                        contents: bytemuck::cast_slice(&m.mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: m.mesh.indices.len() as u32,
                    material: m.mesh.material,
                    center,
                });
                frames.push(per_frame);
                drawn.push(vertices);
            }
            InCarPartRender {
                pass: part.pass,
                channel: part.channel,
                pivot_origin: Vec3::from(body(part.pivot_origin)),
                pivot_axis: Vec3::from(body_dir(part.pivot_axis)).normalize_or_zero(),
                frame_channel: part.frame_channel,
                frame_count: part.frame_count,
                meshes,
                frames,
                shown_frame: Cell::new(f32::NAN),
                damage: part.damage.clone().map(|d| {
                    (nfs_assets::car_damage::CarDamageModel::new(vec![d.part.clone()]), d, drawn)
                }),
            }
        })
        .collect()
}

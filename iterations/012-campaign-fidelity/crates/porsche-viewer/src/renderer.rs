use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use nfs_assets::{AlphaMode, Scene};
use std::collections::{BTreeMap, BTreeSet};
use wgpu::util::DeviceExt;

#[path = "original_ai_view.rs"]
mod original_ai_view;

pub fn summarize(scene: &Scene) -> String {
    let vertices: usize = scene.meshes.iter().map(|m| m.vertices.len()).sum();
    let triangles: usize = scene.meshes.iter().map(|m| m.indices.len() / 3).sum();
    format!(
        "{} parts · {vertices} vertices · {triangles} triangles · {} textures · {} materials\nBounds: {:?}\n{}",
        scene.meshes.len(),
        scene.textures.len(),
        scene.materials.len(),
        scene.bounds,
        scene.diagnostics.join("\n")
    )
}

pub fn instance() -> wgpu::Instance {
    let backends = if cfg!(target_arch = "wasm32") {
        wgpu::Backends::BROWSER_WEBGPU
    } else if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::VULKAN
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = backends;
    wgpu::Instance::new(descriptor)
}

pub async fn request_gpu(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|e| format!("Graphics adapter unavailable: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Porsche viewer"),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("Graphics device unavailable: {e}"))?;
    Ok((adapter, device, queue))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    Orbit,
    Drive,
}

pub struct Camera {
    pub mode: CameraMode,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub center: Vec3,
    pub initial_center: Vec3,
    pub initial_distance: f32,
    pub radius: f32,
    pub drive_pos: Vec3,
    pub drive_target: Vec3,
    pub drive_height: f32,
    /// Vertical FOV in degrees (perspective_rh convention), unless `drive_hfov` is set.
    pub drive_fov: f32,
    /// Horizontal FOV in degrees; the vertical FOV then follows the viewport aspect.
    pub drive_hfov: Option<f32>,
    pub drive_near: f32,
    /// View up reference (world up, or the camera matrix up row of the original cameras).
    pub drive_up: Vec3,
}

/// Drive camera from an original view (original world = scene with z mirrored): eye, eye +
/// forward, the matrix up row as up reference (world up for the chase cams; the car's for
/// the bumper cams), horizontal FOV and near plane.
/// Debug side chase view angle (Shift+V), degrees.
const SIDE_VIEW_DEG: f32 = 25.0;

/// A camera view (original world space) rotated about the world up axis through `pivot`
/// by `deg` degrees (debug side chase view).
fn swing_view(
    view: &nfs_assets::camera::CameraView,
    pivot: [f32; 3],
    deg: f32,
) -> nfs_assets::camera::CameraView {
    let rot = glam::Mat3::from_rotation_y(deg.to_radians());
    let p = Vec3::from(pivot);
    let r = |v: [f32; 3]| (rot * Vec3::from(v)).to_array();
    let eye = (p + rot * (Vec3::from(view.eye) - p)).to_array();
    let matrix = [r(view.matrix[0]), r(view.matrix[1]), r(view.matrix[2])];
    nfs_assets::camera::CameraView {
        eye,
        matrix,
        target: (Vec3::from(eye) + Vec3::from(matrix[2])).to_array(),
        ..*view
    }
}

pub fn apply_original_view(camera: &mut Camera, view: &nfs_assets::camera::CameraView) {
    use nfs_assets::physics::original_adapter::from_original;
    camera.drive_pos = from_original(view.eye);
    camera.drive_target = from_original(view.target);
    camera.drive_up = from_original(view.matrix[1]);
    camera.drive_hfov = Some(view.fov_deg);
    camera.drive_near = view.near;
}

/// Vertical FOV (radians) of a horizontal FOV (degrees) at `aspect` (width / height):
/// the original projection scales x by `tan(fov / 2)` and y by `tan(fov / 2) / aspect`.
pub fn vertical_fov(horizontal_deg: f32, aspect: f32) -> f32 {
    2.0 * ((horizontal_deg.to_radians() * 0.5).tan() / aspect.max(1e-3)).atan()
}

/// The original ran at 4:3 only. Wider screens keep the 4:3 vertical FOV and see more at
/// the sides ("Hor+", as widescreen patches do); narrower ones keep the horizontal FOV.
pub const ORIGINAL_ASPECT: f32 = 4.0 / 3.0;

/// Vertical FOV (radians) for the original horizontal FOV on a screen of `aspect`.
pub fn hor_plus_vertical_fov(horizontal_deg: f32, aspect: f32) -> f32 {
    vertical_fov(horizontal_deg, aspect.min(ORIGINAL_ASPECT))
}

impl Camera {
    pub fn new(bounds: [[f32; 3]; 2]) -> Self {
        let min = Vec3::from(bounds[0]);
        let max = Vec3::from(bounds[1]);
        let radius = ((max - min).length() * 0.5).max(0.1);
        let center = (max + min) * 0.5;
        let distance = radius * 2.8;
        let mut camera = Self {
            mode: CameraMode::Orbit,
            center,
            initial_center: center,
            initial_distance: distance,
            radius,
            yaw: 0.0,
            pitch: 0.0,
            distance: 0.0,
            drive_pos: center,
            drive_target: center + Vec3::new(0.0, 0.0, -10.0),
            drive_height: 1.6,
            drive_fov: 58.0,
            drive_hfov: None,
            drive_near: 0.1,
            drive_up: Vec3::Y,
        };
        camera.reset();
        camera
    }
    pub fn reset(&mut self) {
        self.mode = CameraMode::Orbit;
        self.center = self.initial_center;
        self.yaw = 0.75;
        self.pitch = 0.27;
        self.distance = self.initial_distance;
        self.drive_pos = self.initial_center;
        self.drive_target = self.initial_center + Vec3::new(0.0, 0.0, -10.0);
        self.drive_height = 1.6;
        self.drive_fov = 58.0;
        self.use_world_view();
    }
    /// Arcade/free framing: vertical `drive_fov`, near 0.1, world up.
    pub fn use_world_view(&mut self) {
        self.drive_hfov = None;
        self.drive_near = 0.1;
        self.drive_up = Vec3::Y;
    }
    fn drive_fov_near(&self, aspect: f32) -> (f32, f32) {
        let fov = match self.drive_hfov {
            Some(h) => hor_plus_vertical_fov(h, aspect),
            None => self.drive_fov.to_radians(),
        };
        (fov, self.drive_near)
    }
    fn up(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => Vec3::Y,
            CameraMode::Drive => self.drive_up,
        }
    }
    pub fn orbit(&mut self, x: f32, y: f32) {
        match self.mode {
            CameraMode::Orbit => {
                self.yaw -= x * 0.006;
                self.pitch = (self.pitch + y * 0.006).clamp(-1.45, 1.45);
            }
            CameraMode::Drive => {
                self.yaw -= x * 0.004;
                self.pitch = (self.pitch - y * 0.004).clamp(-1.35, 1.35);
            }
        }
    }
    pub fn pan(&mut self, x: f32, y: f32) {
        match self.mode {
            CameraMode::Orbit => {
                let eye = self.eye();
                let forward = (self.center - eye).normalize_or_zero();
                let right = forward.cross(Vec3::Y).normalize_or_zero();
                let up = right.cross(forward).normalize_or_zero();
                let speed = (self.distance * 0.0015).max(0.01);
                self.center += (-right * x + up * y) * speed;
            }
            CameraMode::Drive => {
                let forward_h =
                    Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos()).normalize_or_zero();
                let right_h = forward_h.cross(Vec3::Y).normalize_or_zero();
                self.drive_pos += -right_h * x * 0.05 + forward_h * y * 0.05;
            }
        }
    }
    pub fn move_ground(&mut self, forward_amount: f32, right_amount: f32) {
        match self.mode {
            CameraMode::Orbit => {
                let eye = self.eye();
                let forward = (self.center - eye).normalize_or_zero();
                let mut forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
                if forward_h.length_squared() < 0.001 {
                    forward_h = Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
                }
                let right_h = forward_h.cross(Vec3::Y).normalize_or_zero();
                let speed = (self.distance * 0.05).clamp(0.5, 50.0);
                self.center += (forward_h * forward_amount + right_h * right_amount) * speed;
            }
            CameraMode::Drive => {
                let forward_h =
                    Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos()).normalize_or_zero();
                let right_h = forward_h.cross(Vec3::Y).normalize_or_zero();
                let speed = 25.0;
                self.drive_pos += (forward_h * forward_amount + right_h * right_amount) * speed;
            }
        }
    }
    pub fn zoom(&mut self, delta: f32) {
        match self.mode {
            CameraMode::Orbit => {
                let min_distance = (self.radius * 0.001).clamp(0.2, 1.0);
                let max_distance = (self.radius * 20.0).max(5000.0);
                self.distance =
                    (self.distance * (delta * 0.001).exp()).clamp(min_distance, max_distance);
            }
            CameraMode::Drive => {
                self.drive_height = (self.drive_height + delta * 0.005).clamp(0.5, 30.0);
            }
        }
    }
    pub fn eye(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => {
                self.center
                    + self.distance
                        * Vec3::new(
                            self.yaw.sin() * self.pitch.cos(),
                            self.pitch.sin(),
                            self.yaw.cos() * self.pitch.cos(),
                        )
            }
            CameraMode::Drive => self.drive_pos,
        }
    }
    pub fn target(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => self.center,
            CameraMode::Drive => self.drive_target,
        }
    }
    fn fov_near(&self, aspect: f32) -> (f32, f32) {
        match self.mode {
            CameraMode::Orbit => (
                45.0_f32.to_radians(),
                (self.distance * 0.05).clamp(0.05, 1.0),
            ),
            CameraMode::Drive => self.drive_fov_near(aspect),
        }
    }
    fn view_projection(&self, width: u32, height: u32) -> Mat4 {
        let aspect = width as f32 / height.max(1) as f32;
        let (fov, near) = self.fov_near(aspect);
        let far = (self.radius * 20.0).max(10_000.0);
        let projection = Mat4::perspective_rh(fov, aspect, near, far);
        projection * Mat4::look_at_rh(self.eye(), self.target(), self.up())
    }
    /// View matrix with the translation removed, so the sky dome follows the
    /// camera's orientation only. Uses `eye()`/`target()` so it is correct in
    /// both Orbit and Drive modes (`center` is stale in Drive mode).
    fn sky_view(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target(), self.up());
        Mat4::from_cols(
            view.col(0),
            view.col(1),
            view.col(2),
            glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }
    fn sky_view_projection(&self, width: u32, height: u32) -> Mat4 {
        let aspect = width as f32 / height.max(1) as f32;
        let (fov, near) = self.fov_near(aspect);
        let far = (self.radius * 20.0).max(10_000.0);
        let projection = Mat4::perspective_rh(fov, aspect, near, far);
        projection * self.sky_view()
    }
    fn uniform(&self, width: u32, height: u32) -> CameraUniform {
        let eye = self.eye();
        CameraUniform {
            matrix: self.view_projection(width, height).to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            model: Mat4::IDENTITY.to_cols_array_2d(),
            sun: [0.0; 4],
            ambient: [[0.0; 4]; 2],
            diffuse: [[0.0; 4]; 2],
            view_right: {
                let r = (self.target() - eye).cross(Vec3::Y).normalize_or(Vec3::X);
                [r.x, r.y, r.z, 0.0]
            },
            view_up: {
                let f = (self.target() - eye).normalize_or(Vec3::NEG_Z);
                let u = f.cross(Vec3::Y).normalize_or(Vec3::X).cross(f);
                [u.x, u.y, u.z, 0.0]
            },
            specular: [0.0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    matrix: [[f32; 4]; 4],
    eye: [f32; 4],
    model: [[f32; 4]; 4],
    /// Unit vector towards the sun; `w = 1` enables the original car lighting.
    sun: [f32; 4],
    /// Light slots 0 (body) and 1 (interior): ambient and diffuse colours.
    ambient: [[f32; 4]; 2],
    diffuse: [[f32; 4]; 2],
    /// Camera right/up in world space (sphere map of the specular texture layer).
    view_right: [f32; 4],
    view_up: [f32; 4],
    /// Specular highlight colour of slot 1 (`+0x388`: 0.8) and `w = 1` to enable the
    /// specular layers (`R3DSpecularLayer`, `R3DSpecularTextureLayer`).
    specular: [f32; 4],
}

/// Car light state of `R3DCar_UpdateLighting` 0x43a560 for one track: slot 0
/// `ambient`/`tint`, slot 1 `ambient·interior`/`interiordiffuse` from
/// `<track>.map [header]`, light from the sun of `Sky/<track>.bin`.
/// Approximation A23: road colour sample `S = 1`, layer factors 1, no tunnels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CarLighting {
    pub sun: Vec3,
    pub ambient: [[f32; 3]; 2],
    pub diffuse: [[f32; 3]; 2],
}

impl CarLighting {
    pub(crate) fn from_track(scene: &Scene) -> Option<Self> {
        let header = &scene.topology.as_ref()?.map.as_ref()?.header;
        let sun = Vec3::from(scene.sun_position?).normalize_or_zero();
        if sun == Vec3::ZERO {
            return None;
        }
        let rgb = |c: [f32; 4]| [c[1], c[2], c[3]];
        let mul = |a: [f32; 3], b: [f32; 3]| [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
        let ambient = rgb(header.ambient);
        Some(Self {
            sun,
            ambient: [ambient, mul(ambient, rgb(header.interior))],
            diffuse: [rgb(header.tint), rgb(header.interiordiffuse)],
        })
    }

    /// Fill the uniform; `road` is the road colour `S` under the car (both
    /// slot colours are multiplied by it, 0x43a560).
    fn apply(&self, uniform: &mut CameraUniform, road: [f32; 3]) {
        uniform.sun = [self.sun.x, self.sun.y, self.sun.z, 1.0];
        // 0x43a560: slot 1 specular colour (1, .8, .8, .8); light colour (1, 1, 1).
        uniform.specular = [0.8, 0.8, 0.8, 1.0];
        for slot in 0..2 {
            let [r, g, b] = self.ambient[slot];
            uniform.ambient[slot] = [r * road[0], g * road[1], b * road[2], 1.0];
            let [r, g, b] = self.diffuse[slot];
            uniform.diffuse[slot] = [r * road[0], g * road[1], b * road[2], 1.0];
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuVertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) uv: [f32; 2],
    /// sRGB-encoded vertex colour (track prelit colours, white for cars).
    pub(crate) color: [f32; 4],
}
pub(crate) struct GpuMesh {
    pub(crate) vertex: wgpu::Buffer,
    pub(crate) index: wgpu::Buffer,
    pub(crate) count: u32,
    pub(crate) material: usize,
    pub(crate) center: Vec3,
}
struct GpuMaterial {
    group: wgpu::BindGroup,
    color_buffer: wgpu::Buffer,
    /// `LayerUVAnimate` mode 0 speed (`nfs_assets::uv_scroll_offset`).
    uv_scroll: Option<i32>,
    paintable: bool,
    /// Last value of the material uniform (prelit flag / light slot), kept when repainting.
    tail: f32,
    alpha_mode: AlphaMode,
    alpha_cutoff: f32,
    double_sided: bool,
    depth_bias: i32,
}

/// A prop instance draw call: model matrix + range of mesh indices to draw.
struct PropDraw {
    model: Mat4,
    mesh_range: std::ops::Range<usize>,
}

/// Sky dome rendering state.
struct SkyState {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    /// Uniform with the horizon placement; only `view_projection` changes per frame.
    uniform: SkyUniform,
    /// Drift of the two layers, UV per second (`sky_drift_velocity`).
    drift: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineVertex {
    position: [f32; 3],
    color: [f32; 4],
}

pub struct TopologyRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

/// Dynamic wireframe overlay for the collision boxes (debug).
struct BoxOverlay {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
}

const BOX_OVERLAY_MAX_CARS: usize = 16;
/// Start camera: index 2 of the original cycle list = TailCam0. The original starts in
/// `list[cycle index]` after the intro; the index the front end passes is not read (A12).
const START_CYCLE_INDEX: i16 = 2;
/// Camera half extents of a car without a CRP collision box.
const FALLBACK_HALF_EXTENTS: [f32; 3] = [0.9, 0.65, 2.1];
const BOX_OVERLAY_MAX_VERTICES: usize = BOX_OVERLAY_MAX_CARS * 12 * 2;

/// Build the pair cars (player first, then AI) exactly as the original
/// collision pass sees them. Shared by the collision pass and the debug overlay.
fn build_collision_pair_cars(
    sim: &nfs_assets::physics::VehicleSimulation,
    ai_opponents: &[nfs_assets::AiOpponent],
    center: Vec3,
    adapter: Vec3,
    half: [f32; 3],
) -> Vec<nfs_assets::physics::original_pair::PairCar> {
    use nfs_assets::physics::original_adapter::pair_car_from_sim;
    use nfs_assets::race::original_collision::pair_car_from_ai;
    let mut cars = vec![pair_car_from_sim(sim, center + adapter, half)];
    // AI cars are drawn with the player's model, so they use its box.
    cars.extend(
        ai_opponents
            .iter()
            .map(|ai| pair_car_from_ai(ai, center, half)),
    );
    cars
}

/// Box centre (scene) and the 8 corners (scene) of a pair car.
fn pair_car_box_corners(car: &nfs_assets::physics::original_pair::PairCar) -> (Vec3, [Vec3; 8]) {
    use nfs_assets::physics::original_adapter::from_original;
    let c = Vec3::from(car.position_330);
    let r = Vec3::from(car.axis_r_364) * car.extents_3a8[0];
    let u = Vec3::from(car.axis_u_370) * car.extents_3a8[1];
    let f = Vec3::from(car.axis_f_37c) * car.extents_3a8[2];
    let mut out = [Vec3::ZERO; 8];
    for (i, o) in out.iter_mut().enumerate() {
        let sr = if i & 1 == 0 { -1.0 } else { 1.0 };
        let su = if i & 2 == 0 { -1.0 } else { 1.0 };
        let sf = if i & 4 == 0 { -1.0 } else { 1.0 };
        *o = from_original((c + r * sr + u * su + f * sf).to_array());
    }
    (from_original(car.position_330), out)
}

/// Lift of the car in the front-end room: `FE3DObject_SetPos` 0x4d1700 sets the car's y to
/// the room y plus `+0x3ac`, the half height of the collision box (CRP level 5,
/// `CarModel_SetCollisionExtents` 0x445090), so the box bottom stands at the room origin.
/// Taking `+0x330` as the box centre is approximation A16.
fn fe_room_car_lift(box_center: Vec3, half: [f32; 3]) -> f32 {
    half[1] - box_center.y
}

/// Second car of the compare screen: its meshes and collision box (for the lift).
struct CompareCar {
    render: CarRenderState,
    collision_box: Option<(Vec3, [f32; 3])>,
}

impl CarRenderState {
    /// Meshes a plain draw shows.
    fn visible(&self) -> Vec<usize> {
        (0..self.meshes.len()).filter(|i| !self.hidden.contains(i)).collect()
    }
}

/// Moving part of the room car (`nfs_assets::CarAnimPart` on the GPU).
struct CarAnimRender {
    channel: u8,
    axis: u8,
    pivot: Vec3,
    open_sign: f32,
    extent: f32,
    /// Indices into `CarRenderState::meshes` of the closed part (frame 0).
    base: Vec<usize>,
    /// Cabrio top: vertex frames 0.. per base mesh and the meshes the blend is written to.
    morph: Option<CarMorph>,
}

/// Morph frames of the cabrio top (`[frame][mesh]` body-space vertices) and one dynamic mesh per
/// base mesh; the drawn shape blends two neighbouring frames by the fractional channel value
/// (`R3DCar_SetFrame` 0x4380d0 keeps floor and fraction).
struct CarMorph {
    frames: Vec<Vec<Vec<GpuVertex>>>,
    dynamic: Vec<GpuMesh>,
}

/// Body-space vertices of a scene mesh (like the `set_car_model` loop).
fn body_vertices(mesh: &nfs_assets::Mesh) -> Vec<GpuVertex> {
    const CAR_SCALE: f32 = 5.0;
    mesh.vertices
        .iter()
        .map(|v| GpuVertex {
            position: [
                -v.position[0] * CAR_SCALE,
                v.position[1] * CAR_SCALE,
                -v.position[2] * CAR_SCALE,
            ],
            normal: [-v.normal[0], v.normal[1], -v.normal[2]],
            uv: v.uv,
            color: v.color,
        })
        .collect()
}

/// GPU mesh whose vertex buffer can be rewritten each frame.
fn dynamic_car_mesh(device: &wgpu::Device, mesh: &nfs_assets::Mesh) -> GpuMesh {
    let vertices = body_vertices(mesh);
    let center = vertices.iter().map(|v| Vec3::from(v.position)).sum::<Vec3>()
        / vertices.len().max(1) as f32;
    GpuMesh {
        vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&mesh.name),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }),
        index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&mesh.name),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: mesh.indices.len() as u32,
        material: mesh.material,
        center,
    }
}

struct CarRenderState {
    anim_parts: Vec<CarAnimRender>,
    /// Closed top at rest (`CabrioUp*`, in `meshes`) and the open one (`CabrioDown*`).
    roof_up: Vec<usize>,
    roof_down: Vec<GpuMesh>,
    /// Meshes no plain draw shows: the top's morph frame 0 when a rest mesh replaces it.
    hidden: BTreeSet<usize>,
    meshes: Vec<GpuMesh>,
    materials: Vec<GpuMaterial>,
    chassis_mesh_indices: Vec<usize>,
    wheels: Vec<CarWheelRender>,
    /// Source body bounds in driving coordinates, before the physics adapter.
    body_bounds: Option<[Vec3; 2]>,
    /// In-car parts drawn instead of the car in BumperCam2 (`docs/re/cockpit.md`).
    in_car: Vec<crate::in_car::InCarPartRender>,
}

#[derive(Clone)]
struct CarWheelRender {
    wheel_index: usize,
    source_center: Vec3,
    radius: f32,
    mesh_indices: Vec<usize>,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    pipelines: BTreeMap<(bool, bool, i32), wgpu::RenderPipeline>,
    meshes: Vec<GpuMesh>,
    static_mesh_count: usize,
    materials: Vec<GpuMaterial>,
    paint_color: usize,
    depth: wgpu::TextureView,
    prop_draws: Vec<PropDraw>,
    sky: Option<SkyState>,
    pub topology: Option<TopologyRenderer>,
    pub show_topology: bool,
    pub show_collision_boxes: bool,
    box_overlay: Option<BoxOverlay>,
    pub road_edges: Vec<nfs_assets::TopologyEdge>,
    pub road_surface: Option<nfs_assets::RoadSurface>,
    pub car: Option<crate::arcade::ArcadeCar>,
    pub sim_car: Option<nfs_assets::physics::VehicleSimulation>,
    pub sim_telemetry: Option<nfs_assets::physics::VehicleTelemetry>,
    /// Original player camera (6 DOF driving), `nfs_assets::camera`.
    pub player_camera: nfs_assets::camera::PlayerCamera,
    /// Debug free camera (V): the eye stays, the view follows the car.
    pub free_camera: bool,
    /// Shift+V (debug, not original): the drive view swung about the car by this many degrees
    /// (0 off, +/-`SIDE_VIEW_DEG`) to see the wheels.
    pub side_view_deg: f32,
    pub vehicle_contact_count: u32,
    pub sim_mode: bool,
    /// Collision response: upstream prototype or the recovered original.
    pub collision_model: nfs_assets::race::original_collision::CollisionModel,
    /// Vehicle model of the player car (O): upstream's 6 DOF prototype or the
    /// original (`nfs_assets::physics::original_vehicle`).
    pub vehicle_model: nfs_assets::physics::original_vehicle::VehicleModel,
    /// The player car on the original model, built from the car's `.sim`.
    /// While it drives, the 6 DOF body mirrors its pose.
    pub original_vehicle: Option<nfs_assets::physics::original_vehicle::OriginalVehicle>,
    /// Previous and current 64 Hz states of the drawn original car and camera, blended by the
    /// step clock's leftover (approximation A106).
    drawn_states: Option<nfs_assets::render_interp::DrawnStates>,
    /// Same for the camera alone while the prototype car drives (A106).
    proto_view: Option<nfs_assets::render_interp::ViewStates>,
    proto_prev_car: Option<nfs_assets::camera::CarView>,
    /// Previous and current 64 Hz poses of the original AI cars (A106).
    drawn_ai: Vec<[nfs_assets::render_interp::CarPose; 2]>,
    /// The loaded `.sim` bytes and model name (`res+0x2a8`), to rebuild the
    /// original car when the car model changes.
    original_sim: Option<(Vec<u8>, String)>,
    /// The original AI's track graph and racing line (from the scene), the player
    /// car's `.ais`, and the AI cars on the original model (`original_ai_view`).
    original_ai_track: Option<nfs_assets::race::original_ai::field::OriginalAiTrack>,
    original_ais: Option<Vec<u8>>,
    pub original_ai: Option<nfs_assets::race::original_ai::field::OriginalAiField>,
    collision_clock: nfs_assets::race::original_collision::CollisionClock,
    /// Track collision objects (trees, pillars, crates) and their smash state.
    track_world: Option<nfs_assets::physics::original_track_pass::TrackObjectWorld>,
    smash_events: Vec<nfs_assets::physics::original_track_pass::SmashEvent>,
    crate_smash: crate::crate_smash::CrateSmash,
    smashables: Vec<nfs_assets::Smashable>,
    smash_props: Vec<nfs_assets::PropArticle>,
    smash_clock: f32,
    /// Smackable objects (signs, cones, posts) as flag-0x80 object cars.
    smack_world: nfs_assets::physics::original_smackable::SmackWorld,
    smack_props: Vec<nfs_assets::SmackProp>,
    smack_owners: [nfs_assets::physics::original_support_owner::OriginalSupportOwner; 4],
    pair_globals: nfs_assets::physics::original_pair::PairGlobals,
    /// Car setup screen choices (transmission, damage) for the original vehicle.
    car_setup: nfs_assets::physics::original_vehicle::CarSetup,
    /// Pending gear event (SHIFTUP 2 / SHIFTDOWN 3) and the frames it is still held: the input
    /// word is read on even steps only; the request is idempotent until the gear changes.
    pending_shift: (u8, u8),
    /// Persistent body support owner (original car+8) for the barrier pass.
    body_support_owner: nfs_assets::physics::original_support_owner::OriginalSupportOwner,
    /// Original collision box of the loaded car: centre in race body space
    /// (before the chassis adapter offset) and half extents.
    car_collision_box: Option<(Vec3, [f32; 3])>,
    /// Body points of the loaded car (CRP `BndP`) in race body space (`(-x, y, z)`).
    car_body_points: Vec<Vec3>,
    pub race_session: Option<nfs_assets::RaceSession>,
    pub track_course: Option<nfs_assets::TrackCourse>,
    pub course_tracker: Option<nfs_assets::CourseProgressTracker>,
    pub ai_opponents: Vec<nfs_assets::AiOpponent>,
    /// Performance of the player's SIM car, shared with the AI cars.
    /// Approximation: opponents run the player's car kinematically.
    ai_performance: Option<nfs_assets::AiPerformance>,
    /// Debug cap for AI speed (m/s), cycled with J: none, 30 km/h, parked.
    ai_speed_limit: Option<f32>,
    /// Quick Race choices that reach the race (`docs/re/frontend.md` "Race start parameters").
    pub race_options: RaceOptions,
    /// Start line trigger of the loaded race direction (`Scene::start_trigger`).
    start_trigger: Option<[f32; 3]>,
    /// LINEAR_TRACK `[0x657494]` from the track record (`None`: from the course).
    linear_track: Option<bool>,
    pub barrier_config: nfs_assets::BarrierCollisionConfig,
    pub is_wrong_way: bool,
    /// In-race pause (`DAT_00606a90`): `update_car` runs no step. `re/pause.md`.
    pub paused: bool,
    /// Track reset (R, `Car_ResetWatch 0x40f740`): its timers, the pending key event, the
    /// 32 Hz tick clock, and the number of resets done (debug).
    reset_watch: nfs_assets::race::reset::ResetWatch,
    reset_request: bool,
    reset_clock: f32,
    pub car_resets: u32,
    car_render: Option<CarRenderState>,
    /// Front-end 3D room shown behind the menus (`crate::fe_room`); the scene is its model.
    pub fe_room: Option<crate::fe_room::RoomView>,
    /// Draw the loaded car in the front-end room (layouts with a 3D car control).
    pub fe_room_car: bool,
    /// Body paint of the room car (the colour button's list entry); `None` keeps the model's.
    pub fe_room_paint: Option<[f32; 4]>,
    /// Car compare screen (`carcompare.lay`): the room car stands at spot 1 and the stashed
    /// second car at spot 2 (`FECar3DControl` position types 1 and 2, A230).
    pub fe_compare: bool,
    /// Opening of the room car's top, hood and trunk lid (front-end clicks, A232).
    pub fe_car_anim: crate::fe_room::CarAnim,
    /// Wall clock of the tweens, set by the browser each frame.
    pub fe_anim_ms: f64,
    fe_compare2: Option<CompareCar>,
    pub fe_compare_paint2: Option<[f32; 4]>,
    /// Front-end screen with an empty room (`ID_NONE`): draw nothing but the clear colour.
    pub fe_room_hidden: bool,
    /// Rendered source-model bounds adapter; original selected bounds getter
    /// and body/model origin equivalence still require comparison.
    trigger_half_extents: Option<Vec3>,
    scenario_props: Vec<nfs_assets::scenario::ScenarioProp>,
    pub prop_hit: Vec<bool>,
    scenario_start: Option<nfs_assets::ScenarioStart>,
    scenario_name: Option<String>,
    scenario_gates: Vec<nfs_assets::ScenarioGate>,
    mission_progress: Option<nfs_assets::scenario::ScenarioProgress>,
    mission_timed_out: bool,
    pub is_mission_mode: bool,
    pub mission_time_limit: f32,
    pub mission_elapsed_time: f32,
    pub stunt_detector: Option<nfs_game::factory_driver::StuntDetector>,
    /// Car glares and shadows (`docs/re/lighting.md`).
    effects: crate::effects::Effects,
    car_glares: Vec<nfs_assets::CarGlare>,
    /// Needle calibration of the car's `nfs5.car` record (`docs/re/cockpit.md`).
    cockpit_gauges: nfs_assets::cockpit::Gauges,
    player_lights: nfs_assets::lights::LightGroups,
    ai_lights: Vec<nfs_assets::lights::LightGroups>,
    /// Player's light switch (original `car+0xd87` bit 0, `Controller.Lights`).
    pub light_switch: bool,
    /// Tick clock for the light groups (headlight fade, indicator blink).
    light_clock_s: f64,
    /// Original car lighting of the loaded track, if it has `.map` and sky data.
    car_lighting: Option<CarLighting>,
    /// Player's spark emitter (`R3DCarParticle` slot 4, `CSPK`), if carpart.dat loaded.
    spark_emitter: Option<nfs_assets::sparks::SparkEmitter>,
    /// Barrier contacts of the collide ticks since the last effects update.
    spark_contacts: Vec<SparkContact>,
    spark_clock: f32,
    /// Debug: bursts so far and the strongest barrier contact seen.
    spark_bursts: u32,
    max_barrier_impact: f32,
    /// Builds mesh pipelines for car materials added after `new` (per-material z bias).
    pipeline_factory: MeshPipelineFactory,
    /// Camera group layout and sampler, to rebind the specular map of a car.
    camera_layout: wgpu::BindGroupLayout,
    specular_sampler: wgpu::Sampler,
    /// Road colour lookup for the car lighting (`S`).
    shade_grid: Option<crate::shade_sample::ShadeGrid>,
}

fn scenario_start_pose(start: nfs_assets::ScenarioStart) -> (Vec3, f32) {
    let forward = Vec3::from(start.forward).normalize_or_zero();
    let yaw = (-forward.x).atan2(-forward.z);
    (Vec3::from(start.position), yaw)
}

/// Race options of the Quick Race front end, written like the original's `FE_WriteCarConfig`
/// 0x4eb470/0x4ecf00 writes them as config keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceOptions {
    /// `SKILL` `[0x6573f4]` = `Game.Difficutly` (0 beginner, 1 advanced, 2 expert).
    pub difficulty: i32,
    /// `MIRROR` `[0x65742c]`: the picture is mirrored and the steering swapped.
    pub mirror: bool,
    /// Skill values of each opponent (`res+0x154`, from the driver records), by grid slot.
    pub opponent_skills: Vec<[f32; 8]>,
    /// Body paint of the player's car and of each opponent (linear RGBA), by grid slot; the
    /// front end picks them (`docs/re/frontend.md` "Opponent cars and colours").
    pub player_paint: Option<[f32; 4]>,
    pub opponent_paints: Vec<[f32; 4]>,
}

fn quick_race_grid(
    course: &nfs_assets::TrackCourse,
    laps: u32,
    opponents: u32,
) -> (nfs_assets::RaceSession, Vec<nfs_assets::AiOpponent>) {
    let effective_laps = if course.is_circuit { laps } else { 1 };
    let mut session =
        nfs_assets::RaceSession::new("quick_race", effective_laps, !course.is_circuit);
    original_start_countdown(&mut session);
    session.add_participant(0, "Player", true, "Porsche 911 Carrera");
    let mut ai = Vec::with_capacity(opponents as usize);
    let profiles = [
        nfs_assets::AiProfile::pro(),
        nfs_assets::AiProfile::veteran(),
        nfs_assets::AiProfile::novice(),
    ];
    for i in 0..opponents as usize {
        let profile = profiles[i % profiles.len()].clone();
        let slot = i + 1;
        let opponent = nfs_assets::AiOpponent::new(
            slot,
            profile.name.clone(),
            "Porsche 911 Carrera",
            profile,
            slot,
            course,
        );
        session.add_participant(slot, opponent.name.clone(), false, "Porsche 911 Carrera");
        ai.push(opponent);
    }
    (session, ai)
}

fn car_chassis_adapter_body(
    sim: &nfs_assets::physics::VehicleSimulation,
    car_render: &CarRenderState,
) -> Vec3 {
    chassis_adapter_from_wheels(
        sim,
        car_render
            .wheels
            .iter()
            .map(|w| (w.wheel_index, w.source_center)),
    )
}

/// `car_chassis_adapter_body` from the model wheel pivots `(wheel index, source centre)`.
fn chassis_adapter_from_wheels(
    sim: &nfs_assets::physics::VehicleSimulation,
    wheels: impl Iterator<Item = (usize, Vec3)>,
) -> Vec3 {
    let mut sum = Vec3::ZERO;
    let mut count = 0.0;
    for (wheel_index, source_center) in wheels {
        let Some(wheel) = sim.suspension.wheels.get(wheel_index) else {
            continue;
        };
        let physics_rest_center = wheel.hardpoint_body - Vec3::Y * wheel.rest_length;
        sum += physics_rest_center - source_center;
        count += 1.0;
    }
    if count == 0.0 {
        return Vec3::ZERO;
    }

    let adapter = sum / count;
    Vec3::new(
        adapter.x.clamp(-0.25, 0.25),
        adapter.y.clamp(-1.0, 1.0),
        adapter.z.clamp(-0.25, 0.25),
    )
}

/// World matrix of a drawn wheel of the prototype 6 DOF car, origin at the wheel centre. As in the
/// original (`0x43e670`, `0x43c830`) the wheel stays at its own CRP part pivot `source_center`
/// (x and z never change); only y moves, by the suspension compression, and the wheel turns by
/// steer and spin. The pivot is in the chassis mesh frame, so it carries the chassis adapter
/// (`car_chassis_adapter_body`). Built from the current `sim.body`, not from the step's
/// telemetry: a body moved after the step (barrier push-out, contact response) takes its wheels
/// with it. Approximation A2/A37: the compression is the prototype's `wheel.compression`, not
/// the original `w+0x68`.
fn prototype_wheel_world_matrix(
    sim: &nfs_assets::physics::VehicleSimulation,
    wheel_index: usize,
    source_center: Vec3,
    adapter: Vec3,
) -> Option<Mat4> {
    let wheel = sim.suspension.wheels.get(wheel_index)?;
    let steer_quat = if wheel.is_steered {
        Quat::from_rotation_y(-wheel.steer_angle)
    } else {
        Quat::IDENTITY
    };
    let roll_quat = Quat::from_rotation_x(-sim.wheel_visual_angles[wheel_index]);
    Some(
        sim.body.transform_matrix()
            * Mat4::from_translation(adapter + source_center + Vec3::Y * wheel.compression)
            * Mat4::from_quat(steer_quat * roll_quat),
    )
}

/// Original pose of a 6 DOF body whose collision box centre is `box_body` (body space): box
/// centre `+0x330`, axes R/U/F, centre velocity, angular velocity (turns/s), all in original
/// axes (`original_adapter`).
fn original_pose_from_body(
    body: &nfs_assets::physics::RigidBody,
    box_body: Vec3,
) -> ([f32; 3], [[f32; 3]; 3], [f32; 3], [f32; 3]) {
    use nfs_assets::physics::original_adapter::{angular_to_original, to_original};
    let centre = body.to_world_pos(box_body);
    (
        to_original(centre),
        [
            to_original(body.right()),
            to_original(body.up()),
            to_original(body.forward()),
        ],
        to_original(body.point_velocity(centre)),
        angular_to_original(body.orientation * body.angular_velocity),
    )
}

/// The inverse of `original_pose_from_body`: puts the body at the original car's pose.
fn set_body_from_original(
    body: &mut nfs_assets::physics::RigidBody,
    car: &nfs_assets::physics::original_vehicle::OriginalCar,
    box_body: Vec3,
) {
    use nfs_assets::physics::original_adapter::{angular_from_original, from_original};
    let [r, u, f] = car.axes_364.map(from_original);
    let q = Quat::from_mat3(&glam::Mat3::from_cols(r, u, -f)).normalize();
    body.orientation = q;
    body.position = from_original(car.position_330) - q * box_body;
    let omega = angular_from_original(car.angular_388);
    body.angular_velocity = q.inverse() * omega;
    // The original velocity is the box centre's.
    let r = from_original(car.position_330) - body.world_com();
    body.linear_velocity = from_original(car.velocity_33c) - omega.cross(r);
}

/// Model matrix of the original car: scene axes from the original R, U, F (`+0x364`), at
/// the box centre `+0x330` (Z mirrored, `original_adapter`). Body −z is forward.
fn original_body_matrix(car: &nfs_assets::physics::original_vehicle::OriginalCar) -> Mat4 {
    pose_matrix(&car_pose(car))
}

/// Box-centre pose of the original car (the interpolated state).
fn car_pose(
    car: &nfs_assets::physics::original_vehicle::OriginalCar,
) -> nfs_assets::render_interp::CarPose {
    nfs_assets::render_interp::CarPose {
        position: car.position_330,
        axes: car.axes_364,
    }
}

/// `original_body_matrix` of a (possibly interpolated) pose.
fn pose_matrix(pose: &nfs_assets::render_interp::CarPose) -> Mat4 {
    use nfs_assets::physics::original_adapter::from_original;
    let [r, u, f] = pose.axes.map(from_original);
    Mat4::from_cols(
        r.extend(0.0),
        u.extend(0.0),
        (-f).extend(0.0),
        from_original(pose.position).extend(1.0),
    )
}

/// Original wheel record of a drawn wheel at mesh-space offset `p` from the box centre
/// (forward is mesh −z, right is +x): the quadrant order of `0x49abc0`, FL, FR, RL, RR.
fn original_wheel_index(p: Vec3) -> usize {
    let front = if p.z < 0.0 { 0 } else { 2 };
    front + usize::from(p.x >= 0.0)
}

/// Wheel mesh transform in mesh space for the original car, as `0x43c830` draws a wheel part:
/// translated by the pivot `(0, w+0x68, 0)` (the compression along the body up, both ways,
/// `draw_wheel_pose`), then steer of the front wheels from the smoothed steer
/// `+0xac8/+0xacc`, the camber `w+0x70` turns about the forward axis, and the spin `w+0x88`
/// turns about the axle. The rotation order and the camber sense in the mirrored mesh space
/// are not read from the original (approximation A37).
fn original_wheel_matrix(
    v: &nfs_assets::physics::original_vehicle::OriginalVehicle,
    wheel: &CarWheelRender,
    center: Vec3,
) -> Mat4 {
    use std::f32::consts::TAU;
    let index = original_wheel_index(wheel.source_center - center);
    let steer = if index < 2 {
        v.draw_steer_turns(index) * TAU
    } else {
        0.0
    };
    let spin = v.car.wheels[index].spin_88 * TAU;
    let (offset, camber) = v.draw_wheel_pose(index);
    // Mesh space is the original model rotated 180° about up (x and z negated), so a turn
    // about the model's forward axis is the opposite turn about mesh z: a compressed wheel's
    // top leans toward the car's centre line.
    Mat4::from_translation(wheel.source_center + Vec3::Y * offset)
        * Mat4::from_rotation_y(-steer)
        * Mat4::from_rotation_z(-camber * TAU)
        * Mat4::from_rotation_x(-spin)
        * Mat4::from_translation(-wheel.source_center)
}

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        scene: &Scene,
    ) -> Result<Self, String> {
        if scene.meshes.is_empty() || scene.materials.is_empty() {
            return Err("Scene has no geometry or materials".into());
        }
        let mut camera = Camera::new(scene.bounds);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::bytes_of(&camera.uniform(width, height)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Car specular sphere map (`R3DSpecularTextureLayer`, carcmn.fsh SPCn).
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let specular_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("specular map sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let no_specular = upload_texture(&device, &queue, 1, 1, &[0, 0, 0, 0]);
        let camera_group =
            camera_bind_group(&device, &camera_layout, &camera_buffer, &no_specular, &specular_sampler);
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&mipmapped_sampler("texture sampler"));
        let mut texture_views = Vec::new();
        for texture in &scene.textures {
            if texture.width == 0
                || texture.height == 0
                || texture.width > device.limits().max_texture_dimension_2d
                || texture.height > device.limits().max_texture_dimension_2d
                || u64::from(texture.width) * u64::from(texture.height) * 4
                    != texture.rgba.len() as u64
            {
                return Err(format!("Invalid texture dimensions/data: {}", texture.name));
            }
            texture_views.push(upload_texture_mipmapped(
                &device,
                &queue,
                texture.width,
                texture.height,
                &texture.rgba,
            ));
        }
        let white = upload_texture(&device, &queue, 1, 1, &[255; 4]);
        let mut materials = Vec::new();
        for material in &scene.materials {
            let view = match material.texture {
                Some(i) => texture_views
                    .get(i)
                    .ok_or("Invalid material texture index")?,
                None => &white,
            };
            let material_uniform = [
                material.base_color[0],
                material.base_color[1],
                material.base_color[2],
                material.base_color[3],
                alpha_mode_code(material.alpha_mode),
                material.alpha_cutoff,
                if material.paintable { 1.0 } else { 0.0 },
                if material.prelit { 1.0 } else { 0.0 },
                0.0, 0.0, 0.0, 0.0, // anim: uv offset
            ];
            let color = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&material.name),
                contents: bytemuck::cast_slice(&material_uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&material.name),
                layout: &material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: color.as_entire_binding(),
                    },
                ],
            });
            materials.push(GpuMaterial {
                group,
                color_buffer: color,
                uv_scroll: material.uv_scroll,
                paintable: material.paintable,
                tail: if material.prelit { 1.0 } else { 0.0 },
                alpha_mode: material.alpha_mode,
                alpha_cutoff: material.alpha_cutoff,
                double_sided: material.double_sided,
                depth_bias: material.depth_bias,
            });
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("car shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("car.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("car"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let mut pipelines = BTreeMap::new();
        let pipeline_factory = MeshPipelineFactory { shader, layout, format };
        let make_pipeline = |blend: bool, double_sided: bool, depth_bias: i32| {
            pipeline_factory.create(&device, blend, double_sided, depth_bias)
        };
        for material in &scene.materials {
            let blend = material.alpha_mode == AlphaMode::Blend;
            let double_sided = material.double_sided;
            let key = (blend, double_sided, material.depth_bias);
            pipelines
                .entry(key)
                .or_insert_with(|| make_pipeline(blend, double_sided, material.depth_bias));
        }
        pipelines
            .entry((false, false, 0))
            .or_insert_with(|| make_pipeline(false, false, 0));
        pipelines
            .entry((true, false, 0))
            .or_insert_with(|| make_pipeline(true, false, 0));
        let mut meshes = Vec::new();
        for mesh in &scene.meshes {
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            if mesh.material >= materials.len()
                || mesh
                    .indices
                    .iter()
                    .any(|&i| i as usize >= mesh.vertices.len())
            {
                return Err(format!("Invalid mesh indices: {}", mesh.name));
            }
            let vertices: Vec<GpuVertex> = mesh
                .vertices
                .iter()
                .map(|v| GpuVertex {
                    position: v.position,
                    normal: v.normal,
                    uv: v.uv,
                    color: v.color,
                })
                .collect();
            let center = mesh
                .vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .sum::<Vec3>()
                / mesh.vertices.len() as f32;
            meshes.push(GpuMesh {
                vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&mesh.name),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&mesh.name),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: mesh.indices.len() as u32,
                material: mesh.material,
                center,
            });
        }
        let static_mesh_count = scene
            .prop_articles
            .first()
            .map(|p| p.mesh_range.start)
            .unwrap_or(scene.meshes.len());

        // Build prop instance draw calls
        let mut prop_draws: Vec<PropDraw> = scene
            .prop_instances
            .iter()
            .map(|inst| {
                let r = &inst.rotation;
                // Build 4x4 model matrix from 3x3 rotation + translation.
                // The .scn rotation is row-vector (D3D, like the CRP `tr`): its rows are
                // the object's axes in world space. Scene coordinates negate Z, so the
                // model matrix is S·Rᵀ·S with S = diag(1, 1, −1).
                let model = Mat4::from_cols_array(&[
                    r[0][0],
                    r[0][1],
                    -r[0][2],
                    0.0,
                    r[1][0],
                    r[1][1],
                    -r[1][2],
                    0.0,
                    -r[2][0],
                    -r[2][1],
                    r[2][2],
                    0.0,
                    inst.position[0],
                    inst.position[1],
                    inst.position[2],
                    1.0,
                ]);
                PropDraw {
                    model,
                    mesh_range: scene.prop_articles[inst.article_index].mesh_range.clone(),
                }
            })
            .collect();

        let mut scenario_props: Vec<_> = scene
            .prop_instances
            .iter()
            .map(|inst| nfs_assets::scenario::ScenarioProp {
                target: nfs_assets::scenario::PropTarget {
                    position: inst.position,
                    fourcc: scene.prop_articles[inst.article_index].fourcc,
                },
                triggerable: inst.triggerable,
            })
            .collect();
        nfs_assets::scenario::hide_triggerable_props(&mut scenario_props);
        for (draw, prop) in prop_draws.iter_mut().zip(&scenario_props) {
            draw.model.w_axis.y = prop.target.position[1];
        }

        // Build sky dome
        let sky = scene.sky_texture.as_ref().map(|sky_tex| {
            create_sky_state(
                &device,
                &queue,
                format,
                &camera,
                width,
                height,
                sky_tex.width,
                sky_tex.height,
                &sky_tex.rgba,
                &scene.sky_layers,
                &scene.sky_horizon.unwrap_or_else(default_sky_horizon),
                scene.sky_dome.as_ref(),
                scene.sky_map.as_ref(),
            )
        });

        // Build topology line overlay
        let mut topology_renderer = None;
        let mut box_overlay_init = None;
        if let Some(top) = &scene.topology {
            let mut line_vertices = Vec::new();
            for line in &top.boundary_lines {
                if line.points.len() >= 2 {
                    for i in 0..line.points.len() - 1 {
                        line_vertices.push(LineVertex {
                            position: line.points[i],
                            color: line.color,
                        });
                        line_vertices.push(LineVertex {
                            position: line.points[i + 1],
                            color: line.color,
                        });
                    }
                }
            }
            if !line_vertices.is_empty() {
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("topology line vertices"),
                    contents: bytemuck::cast_slice(&line_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("topology line shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        r#"
struct Camera {
    matrix: mat4x4<f32>,
    eye: vec4<f32>,
    model: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    var world_pos = camera.model * vec4<f32>(in.position, 1.0);
    var clip_pos = camera.matrix * world_pos;
    clip_pos.z = clip_pos.z - 0.0004 * clip_pos.w;
    out.clip_position = clip_pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#
                        .into(),
                    ),
                });
                let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("topology line layout"),
                    bind_group_layouts: &[Some(&camera_layout)],
                    immediate_size: 0,
                });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("topology line pipeline"),
                    layout: Some(&line_layout),
                    vertex: wgpu::VertexState {
                        module: &line_shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<LineVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                        })],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &line_shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::LineList,
                        ..Default::default()
                    },
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
                });
                box_overlay_init = Some(BoxOverlay {
                    pipeline: pipeline.clone(),
                    vertex_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("collision box vertices"),
                        size: (BOX_OVERLAY_MAX_VERTICES * std::mem::size_of::<LineVertex>()) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                });
                topology_renderer = Some(TopologyRenderer {
                    pipeline,
                    vertex_buffer,
                    vertex_count: line_vertices.len() as u32,
                });
            }
        }

        // Arcade & 6 DOF Simulation car spawn and procedural GPU geometry (for track scenes)
        let mut car = None;
        let mut sim_car = None;
        let mut car_render = None;
        let road_edges: Vec<nfs_assets::TopologyEdge> = scene
            .topology
            .as_ref()
            .map(|t| t.edges.clone())
            .unwrap_or_default();

        if !road_edges.is_empty() {
            let mut spawn_pos = camera.center;
            let mut spawn_yaw = 0.0_f32;

            if let Some(first_edge) = road_edges.first() {
                let p1 = Vec3::from(first_edge.p1);
                let p2 = Vec3::from(first_edge.p2);
                let mid = (p1 + p2) * 0.5;
                let dir = (p2 - p1).normalize_or_zero();
                let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();
                let to_center = camera.center - mid;
                let inward = if perp.dot(to_center) >= 0.0 {
                    perp
                } else {
                    -perp
                };
                spawn_pos = mid + inward * 6.5;
                spawn_pos.y = p1.y;
                if dir.length_squared() > 0.01 {
                    spawn_yaw = (-dir.x).atan2(-dir.z);
                }
            }

            if let Some(course) = &scene.course {
                let (grid_pos, _, grid_yaw) = nfs_assets::calculate_grid_slot(0, course);
                spawn_pos = Vec3::from(grid_pos);
                spawn_yaw = grid_yaw;
                if let Some(surface) = scene.road_surface.as_ref().filter(|s| {
                    s.query(spawn_pos.x, spawn_pos.z, spawn_pos.y, 25.0, 50.0)
                        .is_none()
                }) {
                    let gate0 = &course.checkpoints[0];
                    let fwd_pos = Vec3::new(
                        gate0.center[0] + gate0.forward[0] * 5.0,
                        gate0.center[1],
                        gate0.center[2] + gate0.forward[2] * 5.0,
                    );
                    if let Some(hit) = surface.query(fwd_pos.x, fwd_pos.z, fwd_pos.y, 50.0, 100.0) {
                        spawn_pos = Vec3::new(fwd_pos.x, hit.height, fwd_pos.z);
                    }
                }
            }

            let mut arcade_car = crate::arcade::ArcadeCar::new(spawn_pos, spawn_yaw);
            arcade_car.reset_to_road(&road_edges);
            if let Some(hit) = scene.road_surface.as_ref().and_then(|surface| {
                surface.query(
                    arcade_car.pos.x,
                    arcade_car.pos.z,
                    arcade_car.pos.y,
                    25.0,
                    50.0,
                )
            }) {
                arcade_car.pos.y = hit.height;
            }
            spawn_pos = arcade_car.pos;
            arcade_car.set_pose(arcade_car.pos, spawn_yaw);
            camera.drive_pos = arcade_car.camera_eye;
            camera.drive_target = arcade_car.camera_target;
            car = Some(arcade_car);

            // Realistic 6 DOF Vehicle Simulation specification
            let sim_spec = nfs_formats::sim::SimCar {
                name: "1997 Boxster 2.5L".to_string(),
                mass_kg: 1252.0,
                wheelbase_m: 2.415,
                gear_count: 5,
                drive_flags: 2,
                reverse_gear: -3.44,
                forward_gears: vec![3.50, 2.12, 1.43, 1.03, 0.79],
                final_drive: 3.89,
                redline_rpm: 6700.0,
                idle_or_step_rpm: 800.0,
                torque_curve: [
                    98.0, 102.0, 130.0, 138.0, 157.0, 157.0, 169.0, 169.0, 173.0, 181.0, 181.0,
                    173.0, 146.0, 110.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                ],
                brake_bias: 0.62,
                drag_coeff: 0.31,
                swaybar_stiffness: 12000.0,
                suspension_stiffness: 28.0,
                front_track_m: 1.465,
                rear_track_m: 1.500,
                damping_compression: 2.5,
                damping_rebound: 3.2,
                tire_grip: 0.90,
                cg_offset_m: [0.0, 0.35, -0.1],
                raw: [0u8; 328],
            };
            let mut vehicle_sim = nfs_assets::physics::VehicleSimulation::from_sim(&sim_spec);
            vehicle_sim.reset(
                spawn_pos + Vec3::Y * vehicle_sim.resting_height(),
                spawn_yaw,
            );
            sim_car = Some(vehicle_sim);

            let geom = crate::car_mesh::generate_procedural_car();
            let mut car_materials = Vec::new();
            for part in &geom.parts {
                let color_idx = part.material;
                let base_color = match color_idx {
                    crate::car_mesh::MAT_BODY => crate::car_mesh::CAR_COLORS[0],
                    crate::car_mesh::MAT_GLASS => [0.10, 0.14, 0.20, 0.90],
                    crate::car_mesh::MAT_BLACK => [0.08, 0.08, 0.09, 1.0],
                    crate::car_mesh::MAT_RIMS => [0.82, 0.84, 0.86, 1.0],
                    crate::car_mesh::MAT_LIGHTS_FRONT => [1.0, 0.98, 0.85, 1.0],
                    crate::car_mesh::MAT_LIGHTS_REAR => [0.95, 0.06, 0.06, 1.0],
                    _ => [0.8, 0.8, 0.8, 1.0],
                };
                let alpha_mode = if color_idx == crate::car_mesh::MAT_GLASS {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                };
                let material_uniform = [
                    base_color[0],
                    base_color[1],
                    base_color[2],
                    base_color[3],
                    alpha_mode_code(alpha_mode),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ];
                let color_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("procedural car color"),
                    contents: bytemuck::cast_slice(&material_uniform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("procedural car material"),
                    layout: &material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&white),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: color_buf.as_entire_binding(),
                        },
                    ],
                });
                car_materials.push(GpuMaterial {
                    group,
                    color_buffer: color_buf,
                    uv_scroll: None,
                    paintable: color_idx == crate::car_mesh::MAT_BODY,
                    tail: 0.0,
                    alpha_mode,
                    alpha_cutoff: 0.0,
                    double_sided: false,
                    depth_bias: 0,
                });
            }

            let mut car_meshes = Vec::new();
            for (idx, part) in geom.parts.iter().enumerate() {
                let vertices: Vec<GpuVertex> = part
                    .vertices
                    .iter()
                    .map(|v| GpuVertex {
                        position: v.pos,
                        normal: v.normal,
                        uv: v.uv,
                        color: [1.0; 4],
                    })
                    .collect();
                let center = vertices
                    .iter()
                    .map(|v| Vec3::from(v.position))
                    .sum::<Vec3>()
                    / vertices.len() as f32;
                car_meshes.push(GpuMesh {
                    vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("procedural car vertex"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("procedural car index"),
                        contents: bytemuck::cast_slice(&part.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: part.indices.len() as u32,
                    material: idx,
                    center,
                });
            }
            car_render = Some(CarRenderState {
                meshes: car_meshes,
                materials: car_materials,
                chassis_mesh_indices: (0..geom.parts.len()).collect(),
                wheels: Vec::new(),
                body_bounds: None,
                in_car: Vec::new(),
                anim_parts: Vec::new(),
                roof_up: Vec::new(),
                roof_down: Vec::new(),
                hidden: BTreeSet::new(),
            });
        }

        let depth = depth_view(&device, width, height);

        let track_course = scene.course.clone();
        let mut race_session = None;
        let mut course_tracker = None;
        let mut ai_opponents = Vec::new();

        if let Some(course) = &track_course {
            let total_laps = if course.is_circuit { 3 } else { 1 };
            let mut session =
                nfs_assets::RaceSession::new("track0", total_laps, !course.is_circuit);
            original_start_countdown(&mut session);
            session.add_participant(0, "Player", true, "Porsche 911 Carrera");

            let (p0_pos, _, p0_yaw) = nfs_assets::calculate_grid_slot(0, course);
            let mut car_pos = glam::Vec3::from(p0_pos);
            let hit = scene
                .road_surface
                .as_ref()
                .and_then(|s| s.query(car_pos.x, car_pos.z, car_pos.y, 25.0, 50.0))
                .or_else(|| {
                    let gate0 = &course.checkpoints[0];
                    let fwd_pos = [
                        gate0.center[0] + gate0.forward[0] * 5.0,
                        gate0.center[1],
                        gate0.center[2] + gate0.forward[2] * 5.0,
                    ];
                    if let Some(h) = scene
                        .road_surface
                        .as_ref()
                        .and_then(|s| s.query(fwd_pos[0], fwd_pos[2], fwd_pos[1], 50.0, 100.0))
                    {
                        car_pos.x = fwd_pos[0];
                        car_pos.z = fwd_pos[2];
                        return Some(h);
                    }
                    let h = scene.road_surface.as_ref().and_then(|s| {
                        s.query(
                            gate0.center[0],
                            gate0.center[2],
                            gate0.center[1],
                            50.0,
                            100.0,
                        )
                    });
                    if h.is_some() {
                        car_pos.x = gate0.center[0];
                        car_pos.z = gate0.center[2];
                    }
                    h
                });
            if let Some(hit) = hit {
                car_pos.y = hit.height;
            }

            if let Some(car) = &mut car {
                car.set_pose(car_pos, p0_yaw);
                camera.drive_pos = car.camera_eye;
                camera.drive_target = car.camera_target;
            }
            if let Some(sim) = &mut sim_car {
                sim.reset(car_pos + Vec3::Y * sim.resting_height(), p0_yaw);
            }
            course_tracker = Some(nfs_assets::CourseProgressTracker::new(car_pos.to_array()));

            let ai_profiles = [
                nfs_assets::AiProfile::pro(),
                nfs_assets::AiProfile::veteran(),
                nfs_assets::AiProfile::novice(),
            ];
            for (i, profile) in ai_profiles.into_iter().enumerate() {
                let slot = i + 1;
                let ai = nfs_assets::AiOpponent::new(
                    slot,
                    profile.name.clone(),
                    "Porsche 911 Carrera",
                    profile,
                    slot,
                    course,
                );
                session.add_participant(slot, ai.name.clone(), false, "Porsche 911 Carrera");
                ai_opponents.push(ai);
            }
            race_session = Some(session);
        }

        let effects = crate::effects::Effects::new(&device, format);
        let mut smash_definitions = vec![None; scene.track_objects.len()];
        for s in &scene.smashables {
            smash_definitions[s.object_index] = Some(s.definition.clone());
        }
        let track_world = (!scene.track_objects.is_empty()).then(|| {
            nfs_assets::physics::original_track_pass::TrackObjectWorld::with_definitions(
                scene.track_objects.clone(),
                smash_definitions,
            )
        });
        Ok(Self {
            track_world,
            smash_events: Vec::new(),
            crate_smash: crate::crate_smash::CrateSmash::new(scene.smashables.len()),
            smashables: scene.smashables.clone(),
            smash_props: scene.prop_articles.clone(),
            smash_clock: 0.0,
            smack_world: nfs_assets::physics::original_smackable::SmackWorld::new(
                &scene
                    .smack_props
                    .iter()
                    .map(|p| p.record.clone())
                    .collect::<Vec<_>>(),
            ),
            smack_props: scene.smack_props.clone(),
            smack_owners: Default::default(),
            device,
            queue,
            camera,
            width,
            height,
            camera_buffer,
            camera_group,
            pipelines,
            meshes,
            static_mesh_count,
            materials,
            paint_color: 0,
            depth,
            prop_draws,
            sky,
            topology: topology_renderer,
            show_topology: false,
            show_collision_boxes: false,
            box_overlay: box_overlay_init,
            road_surface: scene.road_surface.clone(),
            road_edges,
            car,
            sim_car,
            sim_telemetry: None,
            player_camera: nfs_assets::camera::PlayerCamera::new(
                nfs_assets::camera::DriverCamera::DEFAULT,
                START_CYCLE_INDEX,
            ),
            free_camera: false,
            side_view_deg: 0.0,
            vehicle_contact_count: 0,
            sim_mode: true,
            collision_model: Default::default(),
            vehicle_model: Default::default(),
            original_vehicle: None,
            drawn_states: None,
            proto_view: None,
            proto_prev_car: None,
            drawn_ai: Vec::new(),
            original_sim: None,
            original_ai_track: scene.ai_track.clone(),
            original_ais: None,
            original_ai: None,
            collision_clock: Default::default(),
            pair_globals: Default::default(),
            car_setup: Default::default(),
            pending_shift: (0, 0),
            body_support_owner: Default::default(),
            car_collision_box: None,
            car_body_points: Vec::new(),
            race_session,
            track_course,
            course_tracker,
            ai_opponents,
            ai_performance: None,
            ai_speed_limit: None,
            race_options: RaceOptions::default(),
            start_trigger: scene.start_trigger,
            linear_track: None,
            barrier_config: nfs_assets::BarrierCollisionConfig::default(),
            is_wrong_way: false,
            paused: false,
            reset_watch: Default::default(),
            reset_request: false,
            reset_clock: 0.0,
            car_resets: 0,
            car_render,
            fe_room: None,
            fe_room_car: false,
            fe_room_paint: None,
            fe_compare: false,
            fe_car_anim: crate::fe_room::CarAnim::CLOSED,
            fe_anim_ms: 0.0,
            fe_compare2: None,
            fe_compare_paint2: None,
            fe_room_hidden: false,
            trigger_half_extents: None,
            scenario_props,
            prop_hit: vec![false; scene.prop_instances.len()],
            scenario_start: scene.scenario_start,
            scenario_name: scene.scenario_name.clone(),
            scenario_gates: scene.scenario_gates.clone(),
            mission_progress: None,
            mission_timed_out: false,
            is_mission_mode: false,
            mission_time_limit: 0.0,
            mission_elapsed_time: 0.0,
            stunt_detector: None,
            effects,
            car_glares: Vec::new(),
            cockpit_gauges: Default::default(),
            player_lights: Default::default(),
            ai_lights: Vec::new(),
            light_switch: false,
            light_clock_s: 0.0,
            car_lighting: CarLighting::from_track(scene),
            spark_emitter: None,
            camera_layout,
            specular_sampler,
            pipeline_factory,
            spark_contacts: Vec::new(),
            spark_clock: 0.0,
            spark_bursts: 0,
            max_barrier_impact: 0.0,
            shade_grid: CarLighting::from_track(scene)
                .and_then(|_| crate::shade_sample::ShadeGrid::from_scene(scene)),
        })
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        if width > 0 && height > 0 {
            self.depth = depth_view(&self.device, width, height);
        }
    }
    /// Writes the body paint of the car meshes (the `CarExt` materials).
    fn paint_car(&self, car_render: &CarRenderState, color: [f32; 4]) {
        for material in car_render.materials.iter().filter(|m| m.paintable) {
            self.queue.write_buffer(
                &material.color_buffer,
                0,
                bytemuck::cast_slice(&[
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                    alpha_mode_code(material.alpha_mode),
                    material.alpha_cutoff,
                    1.0,
                    material.tail,
                ]),
            );
        }
    }

    fn draw_car_meshes(
        &self,
        target: &wgpu::TextureView,
        label: &'static str,
        car_render: &CarRenderState,
        model_matrix: Mat4,
        mesh_indices: &[usize],
    ) {
        self.draw_gpu_meshes(
            target,
            label,
            &car_render.meshes,
            &car_render.materials,
            model_matrix,
            mesh_indices,
        );
    }

    fn draw_gpu_meshes(
        &self,
        target: &wgpu::TextureView,
        label: &'static str,
        meshes: &[GpuMesh],
        materials: &[GpuMaterial],
        model_matrix: Mat4,
        mesh_indices: &[usize],
    ) {
        if mesh_indices.is_empty() {
            return;
        }

        let uniform = self.car_uniform(model_matrix);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
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
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.camera_group, &[]);
            // Opaque and masked parts first, then the blended ones (glass) far to near,
            // so the interior behind a window is not drawn over the window.
            let eye = self.camera.eye();
            let blended = |i: &usize| {
                meshes.get(*i).is_some_and(|m| {
                    materials.get(m.material).is_some_and(|mat| mat.alpha_mode == AlphaMode::Blend)
                })
            };
            let mut order: Vec<usize> = mesh_indices.iter().copied().filter(|i| !blended(i)).collect();
            let mut glass: Vec<usize> = mesh_indices.iter().copied().filter(blended).collect();
            let distance = |i: &usize| model_matrix.transform_point3(meshes[*i].center).distance_squared(eye);
            glass.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
            order.extend(glass);
            for &mesh_index in &order {
                let Some(mesh) = meshes.get(mesh_index) else {
                    continue;
                };
                if mesh.material >= materials.len() {
                    continue;
                }
                let material = &materials[mesh.material];
                let pipeline_key = (
                    material.alpha_mode == AlphaMode::Blend,
                    material.double_sided,
                    material.depth_bias,
                );
                let pipeline = self
                    .pipelines
                    .get(&pipeline_key)
                    .or_else(|| self.pipelines.get(&(pipeline_key.0, false, 0)))
                    .or_else(|| self.pipelines.get(&(false, false, 0)));
                if let Some(pipeline) = pipeline {
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(1, &material.group, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                    pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Drawn box centre and forward (scene axes) of the original car, for telemetry.
    pub fn drawn_original_pose(&self) -> Option<(Vec3, Vec3)> {
        let d = self.drawn_states.as_ref().filter(|_| self.original_active())?;
        let m = pose_matrix(&d.car_pose());
        Some((m.w_axis.truncate(), -m.z_axis.truncate()))
    }

    /// Body matrix of the original car for drawing: the blend of its last two 64 Hz states.
    fn drawn_body_matrix(&self, car: &nfs_assets::physics::original_vehicle::OriginalCar) -> Mat4 {
        match &self.drawn_states {
            Some(d) => pose_matrix(&d.car_pose()),
            None => original_body_matrix(car),
        }
    }

    /// Car-mesh-to-world matrix of the player car body (original model or 6 DOF prototype).
    fn player_body_matrix(&self, car_render: &CarRenderState) -> Option<Mat4> {
        if let Some(v) = self.original_vehicle.as_ref().filter(|_| self.original_active()) {
            let (center, _) = self.collision_box_or_fallback();
            return Some(self.drawn_body_matrix(&v.car) * Mat4::from_translation(-center));
        }
        let sim = self.sim_car.as_ref().filter(|_| self.sim_mode)?;
        Some(sim.body.transform_matrix() * Mat4::from_translation(car_chassis_adapter_body(sim, car_render)))
    }

    /// Inputs of `R3DCar_SetAnimChannels` 0x438440 from `OriginalCar` when the original vehicle
    /// model drives, otherwise from the prototype (approximation A62).
    fn cockpit_inputs(&self) -> nfs_assets::cockpit::CockpitInputs {
        use nfs_assets::cockpit::{speed_ec4, CockpitInputs};
        if let Some(v) = self.original_vehicle.as_ref().filter(|_| self.original_active()) {
            // `[0x657424] >= 2`: the smoothed copies `+0xac8/+0xacc`.
            return CockpitInputs {
                steer: v.car.steer_smooth_ac8,
                rpm: v.car.rpm_db4,
                speed: speed_ec4(v.car.velocity_33c),
                mirror: false,
            };
        }
        let Some(sim) = self.sim_car.as_ref() else {
            return CockpitInputs::default();
        };
        // Prototype wheel angle (rad) as steer units: the original draws w+0xb4/216 turns.
        let units = |i: usize| {
            sim.suspension.wheels.get(i).map_or(0.0, |w| w.steer_angle) / std::f32::consts::TAU * 216.0
        };
        let v = sim.body.linear_velocity;
        CockpitInputs {
            steer: [units(0), units(1)],
            rpm: self.sim_telemetry.as_ref().map_or(0.0, |t| t.engine_rpm),
            speed: speed_ec4([v.x, v.y, v.z]),
            mirror: false,
        }
    }

    /// BumperCam2: the in-car parts of the player car (`R3DObject_DrawPasses` 0x444e00) in pass
    /// order, the steering wheel and needles rotated about their pivots, the hands at their frame;
    /// the driver pass is skipped while a look key is held.
    fn draw_in_car(&self, target: &wgpu::TextureView, car_render: &CarRenderState) {
        let Some(body) = self.player_body_matrix(car_render) else {
            return;
        };
        let channels =
            nfs_assets::cockpit::cockpit_channels(&self.cockpit_inputs(), &self.cockpit_gauges);
        let keys = &self.player_camera.keys;
        let looking = keys.left_5e99b4 || keys.right_5e99ac || keys.behind_5e99b0;
        for pass in nfs_assets::InCarPass::ALL {
            if pass == nfs_assets::InCarPass::Driver && looking {
                continue;
            }
            for part in car_render.in_car.iter().filter(|p| p.pass == pass) {
                part.update_frames(&self.queue, &channels);
                let indices: Vec<usize> = (0..part.meshes.len()).collect();
                self.draw_gpu_meshes(
                    target,
                    "in-car part",
                    &part.meshes,
                    &car_render.materials,
                    body * part.matrix(&channels),
                    &indices,
                );
            }
        }
    }

    /// Needle calibration (`nfs5.car` record `+0x2f0..+0x2fc`) of the player car.
    pub fn set_car_gauges(&mut self, values: [f32; 4]) {
        self.cockpit_gauges = nfs_assets::cockpit::Gauges::from_record(values);
    }

    pub fn cycle_paint_color(&mut self) {
        const COLORS: [[f32; 4]; 6] = [
            [1.0, 1.0, 1.0, 1.0],
            [0.72, 0.05, 0.04, 1.0],
            [0.03, 0.16, 0.72, 1.0],
            [0.04, 0.48, 0.16, 1.0],
            [0.82, 0.72, 0.05, 1.0],
            [0.08, 0.08, 0.09, 1.0],
        ];
        self.paint_color = (self.paint_color + 1) % COLORS.len();
        for material in &self.materials {
            if material.paintable {
                self.queue.write_buffer(
                    &material.color_buffer,
                    0,
                    bytemuck::cast_slice(&[
                        COLORS[self.paint_color][0],
                        COLORS[self.paint_color][1],
                        COLORS[self.paint_color][2],
                        COLORS[self.paint_color][3],
                        alpha_mode_code(material.alpha_mode),
                        material.alpha_cutoff,
                        1.0,
                        0.0,
                    ]),
                );
            }
        }
    }
    pub fn render(&mut self, target: &wgpu::TextureView) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        // `LayerUVAnimate` mode 0 (waterfalls, rivers, sea): V offset from the clock [VA 0x448cc0].
        let seconds = animation_seconds();
        for material in &self.materials {
            if let Some(speed) = material.uv_scroll {
                let v = nfs_assets::uv_scroll_offset(speed, seconds);
                self.queue.write_buffer(&material.color_buffer, 32, bytemuck::cast_slice(&[0.0f32, v, 0.0, 0.0]));
            }
        }
        // Front-end room: fixed original camera, the room model turns [VA 0x4e9ff0].
        let mut static_model = Mat4::IDENTITY;
        if let Some(room) = &self.fe_room {
            self.camera.mode = CameraMode::Drive;
            self.camera.drive_pos = room.eye();
            self.camera.drive_target = room.target();
            self.camera.drive_up = Vec3::Y;
            self.camera.drive_hfov = Some(if self.fe_compare {
                crate::fe_room::COMPARE_HFOV_DEG
            } else {
                room.room.horizontal_fov_deg
            });
            self.camera.drive_near = 0.1;
            static_model = room.room_matrix();
        }
        // Update camera uniform with the static model matrix (identity outside rooms)
        let mut static_uniform = self.scene_uniform();
        static_uniform.model = static_model.to_cols_array_2d();
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&static_uniform));

        // Update sky camera if present
        if let Some(sky) = &self.sky {
            let sky_vp = self.camera.sky_view_projection(self.width, self.height);
            let mut sky_uniform = sky.uniform;
            sky_uniform.view_projection = sky_vp.to_cols_array_2d();
            let t = animation_seconds();
            sky_uniform.drift = sky.drift.map(|v| ((f64::from(v) * t).rem_euclid(1.0)) as f32);
            self.queue
                .write_buffer(&sky.camera_buffer, 0, bytemuck::bytes_of(&sky_uniform));
        }

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.fe_compare {
                            wgpu::Color::TRANSPARENT
                        } else {
                            wgpu::Color {
                                r: 0.022,
                                g: 0.030,
                                b: 0.044,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // 1. Draw sky dome first (depth at far plane, always behind geometry)
            if let Some(sky) = &self.sky {
                pass.set_pipeline(&sky.pipeline);
                pass.set_bind_group(0, &sky.camera_group, &[]);
                pass.set_bind_group(1, &sky.texture_group, &[]);
                pass.set_vertex_buffer(0, sky.vertex_buffer.slice(..));
                pass.set_index_buffer(sky.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..sky.index_count, 0, 0..1);
            }

            // 2. Draw static geometry (identity model matrix)
            pass.set_bind_group(0, &self.camera_group, &[]);
            let mut order: Vec<usize> = if self.fe_room_hidden {
                Vec::new()
            } else {
                (0..self.static_mesh_count).collect()
            };
            let eye = self.camera.eye();
            order.sort_by(|&a, &b| {
                let a = &self.meshes[a];
                let b = &self.meshes[b];
                let ta = self.materials[a.material].alpha_mode == AlphaMode::Blend;
                let tb = self.materials[b.material].alpha_mode == AlphaMode::Blend;
                ta.cmp(&tb).then_with(|| {
                    if ta {
                        b.center
                            .distance_squared(eye)
                            .total_cmp(&a.center.distance_squared(eye))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
            });
            for index in order {
                let mesh = &self.meshes[index];
                let material = &self.materials[mesh.material];
                pass.set_pipeline(
                    &self.pipelines[&(
                        material.alpha_mode == AlphaMode::Blend,
                        material.double_sided,
                        material.depth_bias,
                    )],
                );
                pass.set_bind_group(1, &material.group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);

        // 3. Draw prop instances (one encoder per instance to update model matrix)
        let crate_draws: Vec<PropDraw> = self
            .crate_smash
            .draws(self.smash_clock, &self.smashables, &self.smash_props)
            .into_iter()
            .map(|d| PropDraw {
                model: d.model,
                mesh_range: d.mesh_range,
            })
            .chain(self.smack_draws())
            .collect();
        if !self.prop_draws.is_empty() || !crate_draws.is_empty() {
            for prop_draw in self.prop_draws.iter().chain(crate_draws.iter()) {
                let mut uniform = self.scene_uniform();
                uniform.model = prop_draw.model.to_cols_array_2d();
                self.queue
                    .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
                let mut encoder = self.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("prop"),
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
                            view: &self.depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    pass.set_bind_group(0, &self.camera_group, &[]);
                    for mesh_idx in prop_draw.mesh_range.clone() {
                        if mesh_idx >= self.meshes.len() {
                            continue;
                        }
                        let mesh = &self.meshes[mesh_idx];
                        let material = &self.materials[mesh.material];
                        pass.set_pipeline(
                            &self.pipelines[&(
                                material.alpha_mode == AlphaMode::Blend,
                                material.double_sided,
                                material.depth_bias,
                            )],
                        );
                        pass.set_bind_group(1, &material.group, &[]);
                        pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                        pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..mesh.count, 0, 0..1);
                    }
                }
                self.queue.submit([encoder.finish()]);
            }
            // Restore identity model for next frame
            self.queue.write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::bytes_of(&self.scene_uniform()),
            );
        }

        // 3.4. Car in the front-end room: same position and yaw as the room.
        if let (Some(room), Some(car_render), true) = (
            &self.fe_room,
            &self.car_render,
            self.fe_room_car && !self.fe_room_hidden,
        ) {
            if let Some(paint) = self.fe_room_paint {
                self.paint_car(car_render, paint);
            }
            let moving: BTreeSet<usize> = car_render
                .anim_parts
                .iter()
                .flat_map(|p| p.base.iter().copied())
                .collect();
            let roof_open = self.fe_car_anim.roof.value(self.fe_anim_ms);
            let fixed: Vec<usize> = (0..car_render.meshes.len())
                .filter(|i| {
                    !moving.contains(i)
                        && !car_render.hidden.contains(i)
                        && !(car_render.roof_up.contains(i) && roof_open > 0.0)
                })
                .collect();
            let car_matrix = room.car_matrix({
                let (center, half) = self.collision_box_or_fallback();
                fe_room_car_lift(center, half)
            });
            self.draw_car_meshes(target, "front-end room car", car_render, car_matrix, &fixed);
            for part in &car_render.anim_parts {
                let open = self
                    .fe_car_anim
                    .tween(part.channel)
                    .map_or(0.0, |t| t.value(self.fe_anim_ms));
                if let (1, Some(morph)) = (part.channel, &part.morph) {
                    // Cabrio top: the frame channel runs 0..max over the opening; the shape blends
                    // frames floor and floor + 1 by the fraction.
                    let max = (morph.frames.len() - 1) as f32;
                    let pos = open.clamp(0.0, 1.0) * max;
                    if pos <= 0.0 {
                        // Closed: the rest mesh is in `fixed`; without one frame 0 is drawn.
                        if car_render.roof_up.is_empty() {
                            self.draw_car_meshes(target, "room car top", car_render, car_matrix, &part.base);
                        }
                    } else if pos >= max && !car_render.roof_down.is_empty() {
                        let all: Vec<usize> = (0..car_render.roof_down.len()).collect();
                        self.draw_gpu_meshes(
                            target,
                            "room car top down",
                            &car_render.roof_down,
                            &car_render.materials,
                            car_matrix,
                            &all,
                        );
                    } else {
                        let f0 = (pos.floor() as usize).min(morph.frames.len() - 1);
                        let f1 = (f0 + 1).min(morph.frames.len() - 1);
                        let t = pos - f0 as f32;
                        for (mi, dynamic) in morph.dynamic.iter().enumerate() {
                            let (a, b) = (&morph.frames[f0][mi], &morph.frames[f1][mi]);
                            let blended: Vec<GpuVertex> = a
                                .iter()
                                .zip(b)
                                .map(|(p, q)| GpuVertex {
                                    position: Vec3::from(p.position)
                                        .lerp(Vec3::from(q.position), t)
                                        .to_array(),
                                    normal: Vec3::from(p.normal)
                                        .lerp(Vec3::from(q.normal), t)
                                        .to_array(),
                                    uv: p.uv,
                                    color: p.color,
                                })
                                .collect();
                            self.queue.write_buffer(&dynamic.vertex, 0, bytemuck::cast_slice(&blended));
                        }
                        let all: Vec<usize> = (0..morph.dynamic.len()).collect();
                        self.draw_gpu_meshes(
                            target,
                            "room car top blend",
                            &morph.dynamic,
                            &car_render.materials,
                            car_matrix,
                            &all,
                        );
                    }
                } else {
                    // Hood / trunk lid (about the body x axis) and doors (about y) through the hinge.
                    let angle = part.open_sign * part.extent * open * std::f32::consts::TAU;
                    let turn = if part.axis == 1 {
                        Mat4::from_rotation_y(angle)
                    } else {
                        Mat4::from_rotation_x(angle)
                    };
                    let hinge = Mat4::from_translation(part.pivot) * turn * Mat4::from_translation(-part.pivot);
                    self.draw_car_meshes(target, "room car lid", car_render, car_matrix * hinge, &part.base);
                }
            }
        }

        // 3.4b. Car compare screen: two cars at the fixed compare spots, no room.
        if let (Some(room), Some(car1), Some(car2), true) = (
            &self.fe_room,
            &self.car_render,
            &self.fe_compare2,
            self.fe_compare,
        ) {
            let lift = |b: Option<(Vec3, [f32; 3])>| {
                let (center, half) = b.unwrap_or((Vec3::ZERO, FALLBACK_HALF_EXTENTS));
                fe_room_car_lift(center, half)
            };
            let spots = [
                (crate::fe_room::COMPARE_SPOT_1, car1, self.car_collision_box, self.fe_room_paint),
                (
                    crate::fe_room::COMPARE_SPOT_2,
                    &car2.render,
                    car2.collision_box,
                    self.fe_compare_paint2,
                ),
            ];
            for (spot, render, collision_box, paint) in spots {
                if let Some(paint) = paint {
                    self.paint_car(render, paint);
                }
                let all_meshes: Vec<usize> = render.visible();
                self.draw_car_meshes(
                    target,
                    "compare car",
                    render,
                    room.compare_car_matrix(spot, lift(collision_box)),
                    &all_meshes,
                );
            }
        }

        // 3.5. Draw car on track (if present)
        if let (Some(car), Some(car_render), None) = (&self.car, &self.car_render, &self.fe_room) {
            if let Some(paint) = self.race_options.player_paint {
                self.paint_car(car_render, paint);
            }
            // The bumper view looks out from inside the body: like the
            // original BumperCam, the player's own car isn't drawn.
            let bumper_view = self.camera.mode == CameraMode::Drive
                && if self.sim_mode {
                    !self.free_camera && self.player_camera.is_in_car()
                } else {
                    car.view_mode == crate::arcade::DriveViewMode::Bumper
                };
            if bumper_view && self.sim_mode {
                // BumperCam: the viewed car isn't drawn (`R3DCar_SelectLOD` 0x439f90 returns 0);
                // BumperCam2: its in-car passes (0x444e00).
                if self.player_camera.id() == 1 {
                    self.draw_in_car(target, car_render);
                }
            } else if let Some(v) = self
                .original_vehicle
                .as_ref()
                .filter(|_| self.original_active())
            {
                let (center, _) = self.collision_box_or_fallback();
                let body_matrix = self.drawn_body_matrix(&v.car) * Mat4::from_translation(-center);
                self.draw_car_meshes(
                    target,
                    "original car chassis",
                    car_render,
                    body_matrix,
                    &car_render.chassis_mesh_indices,
                );
                for wheel in &car_render.wheels {
                    let matrix = body_matrix * original_wheel_matrix(v, wheel, center);
                    self.draw_car_meshes(
                        target,
                        "original car wheel",
                        car_render,
                        matrix,
                        &wheel.mesh_indices,
                    );
                }
            } else if let (Some(sim), true) = (&self.sim_car, self.sim_mode) {
                let body_matrix = sim.body.transform_matrix()
                    * Mat4::from_translation(car_chassis_adapter_body(sim, car_render));
                self.draw_car_meshes(
                    target,
                    "sim car chassis",
                    car_render,
                    body_matrix,
                    &car_render.chassis_mesh_indices,
                );
                for wheel in &car_render.wheels {
                    let Some(wheel_matrix) = prototype_wheel_world_matrix(
                        sim,
                        wheel.wheel_index,
                        wheel.source_center,
                        car_chassis_adapter_body(sim, car_render),
                    ) else {
                        continue;
                    };
                    self.draw_car_meshes(
                        target,
                        "sim car wheel",
                        car_render,
                        wheel_matrix * Mat4::from_translation(-wheel.source_center),
                        &wheel.mesh_indices,
                    );
                }
            } else {
                let all_meshes: Vec<usize> = car_render.visible();
                self.draw_car_meshes(
                    target,
                    "arcade car",
                    car_render,
                    car.model_matrix(),
                    &all_meshes,
                );
            }

            // Draw AI opponent vehicles on track with the same visible model.
            for (slot, ai) in self.ai_opponents.iter().enumerate() {
                // Each opponent's own paint; the player's goes back after the loop.
                if let Some(paint) = self.race_options.opponent_paints.get(slot) {
                    self.paint_car(car_render, *paint);
                }
                // Approximation: AI cars are kinematic; tilt follows the road
                // surface, not the original vehicle model.
                let model_matrix = Mat4::from_translation(Vec3::from(ai.position))
                    * Mat4::from_mat3(ai.body_rotation());
                self.draw_car_meshes(
                    target,
                    "ai car",
                    car_render,
                    model_matrix,
                    &car_render.chassis_mesh_indices,
                );
                for wheel in &car_render.wheels {
                    let steer = if wheel.wheel_index < 2 {
                        ai.current_controls.steer * 30.0_f32.to_radians()
                    } else {
                        0.0
                    };
                    let angle =
                        (ai.wheel_distance / wheel.radius).rem_euclid(std::f32::consts::TAU);
                    let matrix = model_matrix
                        * Mat4::from_translation(wheel.source_center)
                        * Mat4::from_rotation_y(steer)
                        * Mat4::from_rotation_x(-angle)
                        * Mat4::from_translation(-wheel.source_center);
                    self.draw_car_meshes(
                        target,
                        "ai wheel",
                        car_render,
                        matrix,
                        &wheel.mesh_indices,
                    );
                }
            }

            if let Some(paint) = self.race_options.player_paint {
                self.paint_car(car_render, paint);
            }

            // Restore identity model for remaining passes
            self.queue.write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::bytes_of(&self.scene_uniform()),
            );
        }

        // 3.6 Car shadows and light glares.
        self.draw_car_effects(target);

        // 3b. Collision box wireframes (debug)
        self.draw_collision_boxes(target);

        // 4. Draw topology lines (if enabled and present)
        if let Some(top) = self
            .topology
            .as_ref()
            .filter(|t| self.show_topology && t.vertex_count > 0)
        {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("topology lines"),
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
                        view: &self.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(&top.pipeline);
                pass.set_bind_group(0, &self.camera_group, &[]);
                pass.set_vertex_buffer(0, top.vertex_buffer.slice(..));
                pass.draw(0..top.vertex_count, 0..1);
            }
            self.queue.submit([encoder.finish()]);
        }
    }

    /// Car shadows (`R3DCar_DrawShadow` 0x43b820) and light glares
    /// (`R3DCar_DrawGlares` 0x43d800) of the player and AI cars.
    fn draw_car_effects(&mut self, target: &wgpu::TextureView) {
        use crate::effects::*;
        let (Some(car), Some(car_render)) = (&self.car, &self.car_render) else {
            return;
        };
        let in_car_view = self.camera.mode == CameraMode::Drive
            && if self.sim_mode {
                !self.free_camera && self.player_camera.is_in_car()
            } else {
                car.view_mode == crate::arcade::DriveViewMode::Bumper
            };
        let mut cars: Vec<(Mat4, &nfs_assets::lights::LightGroups)> = Vec::new();
        if !in_car_view {
            let original = self.original_vehicle.as_ref().filter(|_| self.original_active());
            let model = match (&self.sim_car, self.sim_mode) {
                // Same matrix as the original car's chassis draw.
                _ if original.is_some() => {
                    let (center, _) = self.collision_box_or_fallback();
                    self.drawn_body_matrix(&original.unwrap().car) * Mat4::from_translation(-center)
                }
                (Some(sim), true) => {
                    sim.body.transform_matrix()
                        * Mat4::from_translation(car_chassis_adapter_body(sim, car_render))
                }
                _ => car.model_matrix(),
            };
            cars.push((model, &self.player_lights));
        }
        let unlit = nfs_assets::lights::LightGroups::default();
        for (index, ai) in self.ai_opponents.iter().enumerate() {
            let model = Mat4::from_translation(Vec3::from(ai.position))
                * Mat4::from_mat3(ai.body_rotation());
            cars.push((model, self.ai_lights.get(index).unwrap_or(&unlit)));
        }
        let camera = CameraFrame::look(self.camera.eye(), self.camera.target());
        let (box_center, half) = self
            .car_collision_box
            .unwrap_or((Vec3::ZERO, FALLBACK_HALF_EXTENTS));
        let mut shadows = EffectBatches::default();
        let mut glares = EffectBatches::default();
        for (model, lights) in cars {
            let car_y = model.w_axis.y;
            let corners = shadow_corners(model, box_center, half[0], half[2]);
            let n = shadow_subdivisions(camera.eye.distance(model.w_axis.truncate()));
            let height = |x: f32, z: f32| {
                self.road_surface
                    .as_ref()
                    .and_then(|s| s.query(x, z, car_y, SHADOW_PROBE_HEIGHT, 4.0))
                    .map(|hit| hit.height)
                    .or_else(|| sample_road_elevation_from_edges(&self.road_edges, x, z))
            };
            push_car_shadow(&mut shadows, corners, n, height, car_y - half[1]);
            push_car_glares(&mut glares, &self.car_glares, lights, model, &camera);
        }
        if let Some(emitter) = &self.spark_emitter {
            push_sparks(
                &mut glares,
                emitter,
                |p| nfs_assets::physics::original_adapter::from_original(p),
                &camera,
            );
        }
        let view_projection = self.camera.view_projection(self.width, self.height);
        self.effects.draw(
            &self.device,
            &self.queue,
            target,
            &self.depth,
            view_projection,
            &shadows,
            &glares,
        );
    }

    pub fn toggle_collision_boxes(&mut self) -> bool {
        self.show_collision_boxes = !self.show_collision_boxes;
        self.show_collision_boxes
    }

    /// Current pair cars (player first) and the chassis box, if a sim car is loaded.
    fn current_pair_cars(&self) -> Option<Vec<nfs_assets::physics::original_pair::PairCar>> {
        let sim = self.sim_car.as_ref()?;
        let (center, half) = self.car_collision_box?;
        let adapter = self
            .car_render
            .as_ref()
            .map(|r| car_chassis_adapter_body(sim, r))
            .unwrap_or(Vec3::ZERO);
        Some(build_collision_pair_cars(
            sim,
            &self.ai_opponents,
            center,
            adapter,
            half,
        ))
    }

    /// Text report of the player's and nearest AI car's collision boxes.
    pub fn collision_box_report(&self) -> String {
        let Some(cars) = self.current_pair_cars() else {
            return "no sim car".to_string();
        };
        use nfs_assets::physics::original_adapter::from_original;
        let fmt = |v: Vec3| format!("({:.3}, {:.3}, {:.3})", v.x, v.y, v.z);
        let p = &cars[0];
        let mut out = format!(
            "player centre {} half {:?}",
            fmt(from_original(p.position_330)),
            p.extents_3a8
        );
        let pc = Vec3::from(p.position_330);
        let nearest = cars[1..].iter().min_by(|a, b| {
            let da = (Vec3::from(a.position_330) - pc).length_squared();
            let db = (Vec3::from(b.position_330) - pc).length_squared();
            da.total_cmp(&db)
        });
        let Some(a) = nearest else {
            out.push_str("\nno AI cars");
            return out;
        };
        let ac = Vec3::from(a.position_330);
        let d = ac - pc;
        let dist = d.length();
        out.push_str(&format!(
            "\nnearest AI centre {} half {:?}",
            fmt(from_original(a.position_330)),
            a.extents_3a8
        ));
        if dist > 1e-6 {
            let dir = d / dist;
            // Support extent of an oriented box along dir.
            let support = |c: &nfs_assets::physics::original_pair::PairCar| {
                (dir.dot(Vec3::from(c.axis_r_364)) * c.extents_3a8[0]).abs()
                    + (dir.dot(Vec3::from(c.axis_u_370)) * c.extents_3a8[1]).abs()
                    + (dir.dot(Vec3::from(c.axis_f_37c)) * c.extents_3a8[2]).abs()
            };
            let (sp, sa) = (support(p), support(a));
            out.push_str(&format!(
                "\ncentre distance {dist:.3} m, extents along line {sp:.3} + {sa:.3}, box gap {:.3} m",
                dist - sp - sa
            ));
        }
        out
    }

    fn draw_collision_boxes(&mut self, target: &wgpu::TextureView) {
        if !self.show_collision_boxes || self.box_overlay.is_none() {
            return;
        }
        let Some(cars) = self.current_pair_cars() else {
            return;
        };
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let mut verts: Vec<LineVertex> = Vec::new();
        for (i, car) in cars.iter().take(BOX_OVERLAY_MAX_CARS).enumerate() {
            let color = if i == 0 {
                [1.0, 0.9, 0.1, 1.0]
            } else {
                [0.1, 0.9, 1.0, 1.0]
            };
            let (_, corners) = pair_car_box_corners(car);
            for (a, b) in EDGES {
                verts.push(LineVertex {
                    position: corners[a].to_array(),
                    color,
                });
                verts.push(LineVertex {
                    position: corners[b].to_array(),
                    color,
                });
            }
        }
        let Some(overlay) = self.box_overlay.as_ref() else {
            return;
        };
        self.queue
            .write_buffer(&overlay.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("collision boxes"),
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
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&overlay.pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_vertex_buffer(0, overlay.vertex_buffer.slice(..));
            pass.draw(0..verts.len() as u32, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }

    pub fn set_show_topology(&mut self, show: bool) {
        self.show_topology = show;
    }

    pub fn toggle_topology(&mut self) -> bool {
        self.show_topology = !self.show_topology;
        self.show_topology
    }

    pub fn sample_road_elevation(&self, x: f32, z: f32) -> Option<f32> {
        sample_road_elevation_from_edges(&self.road_edges, x, z)
    }

    pub fn move_ground(&mut self, forward_amount: f32, right_amount: f32) {
        if self.camera.mode == CameraMode::Drive && self.car.is_some() {
            self.update_car(0.016, forward_amount * 0.05, right_amount * 0.05, false);
            return;
        }
        self.camera.move_ground(forward_amount, right_amount);
        if let Some(road_y) = (self.camera.mode == CameraMode::Drive)
            .then(|| self.sample_road_elevation(self.camera.drive_pos.x, self.camera.drive_pos.z))
            .flatten()
        {
            let target_y = road_y + self.camera.drive_height;
            self.camera.drive_pos.y += (target_y - self.camera.drive_pos.y) * 0.35;
        }
    }

    pub fn toggle_camera_mode(&mut self) -> bool {
        match self.camera.mode {
            CameraMode::Orbit => {
                self.camera.mode = CameraMode::Drive;
                if let Some(car) = &mut self.car {
                    if car.pos.length_squared() < 0.01 {
                        car.reset_to_road(&self.road_edges);
                    }
                    let fwd = car.forward();
                    car.camera_eye = car.pos + Vec3::new(0.0, 2.0, 0.0) - fwd * 5.8;
                    car.camera_target = car.pos + Vec3::new(0.0, 0.9, 0.0) + fwd * 1.5;
                    self.camera.drive_pos = car.camera_eye;
                    self.camera.drive_target = car.camera_target;
                    self.camera.drive_fov = 58.0;
                } else {
                    let mut start_pos = self.camera.center;
                    if let Some(road_y) = self.sample_road_elevation(start_pos.x, start_pos.z) {
                        start_pos.y = road_y + self.camera.drive_height;
                    } else if let Some(first_edge) = self.road_edges.first() {
                        start_pos = Vec3::new(
                            first_edge.p1[0],
                            first_edge.p1[1] + self.camera.drive_height,
                            first_edge.p1[2],
                        );
                        let dir = (Vec3::from(first_edge.p2) - Vec3::from(first_edge.p1))
                            .normalize_or_zero();
                        if dir.length_squared() > 0.01 {
                            self.camera.yaw = (-dir.x).atan2(-dir.z);
                        }
                    }
                    self.camera.drive_pos = start_pos;
                    self.camera.drive_target = start_pos + Vec3::new(0.0, 0.0, -10.0);
                }
                self.camera.pitch = 0.0;
                true
            }
            CameraMode::Drive => {
                self.camera.mode = CameraMode::Orbit;
                if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
                    self.camera.center = sim.body.position;
                } else if let Some(car) = &self.car {
                    self.camera.center = car.pos;
                } else {
                    let forward = Vec3::new(-self.camera.yaw.sin(), 0.0, -self.camera.yaw.cos());
                    self.camera.center = self.camera.drive_pos + forward * 20.0;
                }
                self.camera.distance = 40.0;
                self.camera.pitch = 0.35;
                false
            }
        }
    }

    pub fn is_drive_mode(&self) -> bool {
        self.camera.mode == CameraMode::Drive
    }

    fn vehicle_contact_proxies(&self) -> Vec<nfs_assets::race::collision::VehicleContactProxy> {
        use nfs_assets::race::collision::VehicleContactProxy;
        let bounds = self
            .car_render
            .as_ref()
            .and_then(|r| r.body_bounds)
            .unwrap_or([Vec3::new(-0.85, 0.15, -2.1), Vec3::new(0.85, 1.3, 2.1)]);
        let center = (bounds[0] + bounds[1]) * 0.5;
        let half = (bounds[1] - bounds[0]) * 0.5;
        let mut proxies = Vec::with_capacity(1 + self.ai_opponents.len());
        let inverse_mass = self
            .sim_car
            .as_ref()
            .map(|s| s.body.inv_mass)
            .unwrap_or(1.0 / 1200.0);
        if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            let adapter = self
                .car_render
                .as_ref()
                .map(|r| car_chassis_adapter_body(sim, r))
                .unwrap_or(Vec3::ZERO);
            let c = sim.body.to_world_pos(center + adapter);
            let forward = sim.body.forward();
            let yaw = (-forward.x).atan2(-forward.z);
            // A yaw box enclosing the rolled/pitched body, including its height.
            let tilt = Quat::from_rotation_y(-yaw) * sim.body.orientation;
            let h = (tilt * Vec3::X).abs() * half.x
                + (tilt * Vec3::Y).abs() * half.y
                + (tilt * Vec3::Z).abs() * half.z;
            proxies.push(VehicleContactProxy {
                previous_center: c.to_array(),
                center: c.to_array(),
                velocity: sim.body.linear_velocity.to_array(),
                yaw,
                half_extents: h.to_array(),
                inverse_mass,
            });
        } else if let Some(car) = &self.car {
            let c = car.pos + Quat::from_rotation_y(car.yaw) * center;
            proxies.push(VehicleContactProxy {
                previous_center: c.to_array(),
                center: c.to_array(),
                velocity: (car.forward() * car.speed).to_array(),
                yaw: car.yaw,
                half_extents: half.to_array(),
                inverse_mass,
            });
        } else {
            return proxies;
        }
        for ai in &self.ai_opponents {
            let c = Vec3::from(ai.position) + Quat::from_rotation_y(ai.yaw) * center;
            proxies.push(VehicleContactProxy {
                previous_center: c.to_array(),
                center: c.to_array(),
                velocity: ai.velocity,
                yaw: ai.yaw,
                half_extents: half.to_array(),
                inverse_mass,
            });
        }
        proxies
    }

    fn resolve_vehicle_contacts(
        &mut self,
        previous: &[nfs_assets::race::collision::VehicleContactProxy],
    ) {
        let mut current = self.vehicle_contact_proxies();
        if current.len() < 2 || current.len() != previous.len() {
            return;
        }
        let before: Vec<[f32; 3]> = current.iter().map(|p| p.center).collect();
        for (p, old) in current.iter_mut().zip(previous) {
            p.previous_center = old.center;
        }
        let mut contacts = 0;
        for i in 0..current.len() {
            let (left, right) = current.split_at_mut(i + 1);
            for b in right {
                if nfs_assets::race::collision::resolve_vehicle_contact(&mut left[i], b) {
                    contacts += 1;
                }
            }
        }
        if contacts == 0 {
            return;
        }
        self.vehicle_contact_count += contacts;
        let delta = Vec3::from(current[0].center) - Vec3::from(before[0]);
        if let (true, Some(sim)) = (self.sim_mode, &mut self.sim_car) {
            sim.body.position += delta;
            sim.body.linear_velocity = Vec3::from(current[0].velocity);
            sim.sync_wheel_spin_to_body();
            self.sim_telemetry =
                Some(sim.telemetry(&nfs_assets::physics::VehicleControls::default()));
            self.camera.drive_pos += delta;
            self.camera.drive_target += delta;
        } else if let Some(car) = &mut self.car {
            car.pos += delta;
            car.speed = Vec3::from(current[0].velocity).dot(car.forward());
        }
        for (i, ai) in self.ai_opponents.iter_mut().enumerate() {
            ai.position = (Vec3::from(ai.position) + Vec3::from(current[i + 1].center)
                - Vec3::from(before[i + 1]))
            .to_array();
            ai.apply_collision_velocity(current[i + 1].velocity);
        }
    }

    /// Pauses or resumes the race: while paused no scheduler step runs (the original gates
    /// `Sched_Step` on `DAT_00606a90`, `re/pause.md` section 3). The session phase follows.
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        if let Some(session) = self.race_session.as_mut() {
            if paused {
                session.pause();
            } else {
                session.resume();
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn update_car(&mut self, dt: f32, mut throttle: f32, steer: f32, handbrake: bool) {
        if self.paused {
            return;
        }
        // MIRROR `[0x65742c]` mirrors the steering input (`docs/re/vehicle.md`); the page
        // flips the picture.
        let steer = if self.race_options.mirror { -steer } else { steer };
        let previous_contacts = self.vehicle_contact_proxies();
        let original = self.original_active();
        let edges = &self.road_edges;

        // Check race session state and restrict inputs during countdown
        if self.is_mission_mode && (self.mission_timed_out || self.get_mission_goal_reached()) {
            return;
        }
        if let Some(session) = self.race_session.as_mut().filter(|_| !self.is_mission_mode) {
            // The original model holds itself at the start (`Car_ApplyControls` gating:
            // brake 0xff until the green light, gas from `+0xdb0 = 0x40`, so the engine
            // revs in neutral); otherwise no gas while driving is not allowed.
            let held_by_gating =
                original && matches!(session.phase, nfs_assets::RacePhase::Countdown { .. });
            if !session.phase.allows_driving_input() && !held_by_gating {
                throttle = 0.0;
            }
            session.step(dt);
        }

        let mut player_pos = [0.0; 3];
        let mut player_fwd = [0.0, 0.0, -1.0];
        // Approximation A19: the original light inputs come from the vehicle
        // (`car+0xd80` brake, `car+0xd82` gear, `car+0xd87` switches).
        let mut player_light_inputs = nfs_assets::lights::LightInputs {
            light_switch: self.light_switch,
            ..Default::default()
        };

        if original {
            // Stepped below with the collision pass (scheduler order).
        } else if let (true, Some(sim)) = (self.sim_mode, &mut self.sim_car) {
            let controls = sim.controls_from_signed_pedal(throttle, steer, handbrake);
            let tel = sim.step(self.road_surface.as_ref(), &controls, dt);
            player_light_inputs.brake = brake_byte(controls.brake);
            player_light_inputs.reverse_gear = sim.powertrain.current_gear < 0;

            // Barrier collision resolution for 6 DOF simulation
            let mut pos = sim.body.position.to_array();
            let mut vel = sim.body.linear_velocity.to_array();
            if self.collision_model
                == nfs_assets::race::original_collision::CollisionModel::Prototype
                && nfs_assets::BarrierCollider::resolve_collision(
                    &mut pos,
                    &mut vel,
                    edges,
                    &self.barrier_config,
                )
            {
                sim.body.position = Vec3::from(pos);
                sim.body.linear_velocity = Vec3::from(vel);
                sim.sync_wheel_spin_to_body();
            }

            player_pos = sim.body.position.to_array();
            player_fwd = sim.body.forward().to_array();

            self.sim_telemetry = Some(tel);
        } else if let Some(car) = &mut self.car {
            let reference_y = car.pos.y;
            let surface = self.road_surface.as_ref();
            car.update(dt, throttle, steer, handbrake, edges, |x, z| {
                match surface {
                    // Prototype support window, not recovered physics. On a miss,
                    // do not snap to another deck through the old edge estimate.
                    Some(surface) => surface
                        .query(x, z, reference_y, 2.0, 4.0)
                        .map(|hit| hit.height),
                    None => sample_road_elevation_from_edges(edges, x, z),
                }
            });
            player_pos = car.pos.to_array();
            player_fwd = car.forward().to_array();
            // Arcade prototype: a negative pedal brakes or reverses.
            player_light_inputs.brake = brake_byte(-throttle);

            if self.camera.mode == CameraMode::Drive {
                self.camera.use_world_view();
                self.camera.drive_pos = car.camera_eye;
                self.camera.drive_target = car.camera_target;
                self.camera.drive_fov = match car.view_mode {
                    crate::arcade::DriveViewMode::Chase => {
                        58.0 + (car.speed.abs() / 50.0).clamp(0.0, 1.0) * 10.0
                    }
                    crate::arcade::DriveViewMode::Bumper => {
                        68.0 + (car.speed.abs() / 50.0).clamp(0.0, 1.0) * 12.0
                    }
                    crate::arcade::DriveViewMode::Free => 55.0,
                };
            }
        }

        self.update_lights(dt, player_light_inputs);
        self.update_sparks(dt);

        // Advance opponents before resolving all vehicle pairs. Collision response
        // is applied back to their controller, rather than discarded next frame.
        if let (Some(course), Some(session)) = (&self.track_course, &self.race_session)
            && session.phase.is_racing()
            && !(original && self.original_ai.is_some())
        {
            let surface = self.road_surface.as_ref();
            for ai in &mut self.ai_opponents {
                let reference_y = ai.position[1];
                let normal = std::cell::Cell::new(None);
                ai.step_kinematics(dt, course, |x, z| {
                    let hit = surface.and_then(|s| s.query(x, z, reference_y, 2.0, 4.0));
                    normal.set(hit.as_ref().map(|h| h.normal));
                    hit.map(|h| h.height).unwrap_or(reference_y)
                });
                ai.update_road_up(dt, normal.get());
            }
        }
        if original {
            // The original model always runs the original collision pass, in its queue.
            self.run_original_vehicle(dt, throttle, steer, handbrake);
        } else if self.collision_model
            == nfs_assets::race::original_collision::CollisionModel::Original
            && self.sim_mode
        {
            self.run_original_collision(dt);
        } else {
            self.resolve_vehicle_contacts(&previous_contacts);
        }
        self.smash_clock += dt;
        self.process_smash_events();
        self.advance_smackables(dt);
        // Track-object contacts are `0x5xxxx` codes: the original's contact effects
        // make no sparks for them (`docs/re/contact-effects.md`), only a sound
        // (not wired); the hit list is dropped.
        if let Some(world) = self.track_world.as_mut() {
            world.hits.clear();
        }
        if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            player_pos = sim.body.position.to_array();
            player_fwd = sim.body.forward().to_array();
        } else if let Some(car) = &self.car {
            player_pos = car.pos.to_array();
        }
        // The camera runs after the car callbacks (scheduler priority 100).
        self.update_original_camera(dt);

        // `Car_ResetWatch 0x40f740` runs in `Car_ContactEffectsTick` (32 Hz, prio 0x32).
        if self.tick_car_reset(dt) {
            if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
                player_pos = sim.body.position.to_array();
                player_fwd = sim.body.forward().to_array();
            } else if let Some(car) = &self.car {
                player_pos = car.pos.to_array();
                player_fwd = car.forward().to_array();
            }
        }

        // Progress tracking along course
        if let (Some(course), Some(tracker), Some(session)) = (
            &self.track_course,
            &mut self.course_tracker,
            self.race_session.as_mut().filter(|_| !self.is_mission_mode),
        ) {
            let lap_res = tracker.update(player_pos, player_fwd, course, dt);
            self.is_wrong_way = tracker.is_wrong_way;

            if let Some(true) = lap_res {
                session.on_player_lap_completed();
            }

            let dist = course.distance_along_course(player_pos);
            if let Some(p) = session.participants.iter_mut().find(|p| p.is_player) {
                p.distance_along_track = dist;
            }

            // Update AI opponents
            if session.phase.is_racing() {
                for ai in &self.ai_opponents {
                    if let Some(p) = session.participants.iter_mut().find(|p| p.id == ai.id) {
                        p.distance_along_track = ai.distance_along_course;
                        p.laps_completed = ai.laps_completed;
                    }
                }
            }

            session.update_standings();
        }

        // Mission mode tracking and prop cone collisions
        if self.is_mission_mode {
            if !self.mission_timed_out && !self.get_mission_goal_reached() {
                self.mission_elapsed_time += dt;
                if self.mission_time_limit > 0.0
                    && self.mission_elapsed_time > self.mission_time_limit
                {
                    self.mission_timed_out = true;
                } else if let Some(vehicle) = self.trigger_vehicle()
                    && let Some(progress) = &mut self.mission_progress
                {
                    let previous = progress.passed_gates();
                    progress.observe(&vehicle);
                    if progress.passed_gates() != previous
                        && let Some(target) = progress.last_linked_prop()
                    {
                        self.activate_scenario_prop(target);
                    }
                }
            }
            if !self.mission_timed_out {
                // Check prop (cone) collision detection
                for i in 0..self.scenario_props.len() {
                    if !self.prop_hit[i] {
                        let p = self.scenario_props[i].target.position;
                        let dx = player_pos[0] - p[0];
                        let dy = player_pos[1] - p[1];
                        let dz = player_pos[2] - p[2];
                        if dx * dx + dy * dy + dz * dz < 2.2 * 2.2 {
                            self.prop_hit[i] = true;
                            if let Some(detector) = &mut self.stunt_detector {
                                detector.cone_hits += 1;
                            }
                        }
                    }
                }

                // Update stunt detector
                if let Some(detector) = &mut self.stunt_detector {
                    let speed_mps = if self.sim_mode {
                        self.sim_telemetry
                            .as_ref()
                            .map(|t| t.speed_mps)
                            .unwrap_or(0.0)
                    } else {
                        self.car.as_ref().map(|c| c.speed).unwrap_or(0.0)
                    };
                    let heading = if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
                        let fwd = sim.body.forward();
                        (-fwd.x).atan2(-fwd.z)
                    } else if let Some(car) = &self.car {
                        car.yaw
                    } else {
                        0.0
                    };
                    let is_rev = if let (true, Some(tel)) = (self.sim_mode, &self.sim_telemetry) {
                        tel.current_gear < 0
                    } else if let Some(car) = &self.car {
                        car.is_reversing
                    } else {
                        false
                    };
                    detector.update(dt, speed_mps, heading, is_rev, handbrake, 0.0);
                }
            }
        }
    }

    /// Camera uniform with the track's car lighting.
    fn scene_uniform(&self) -> CameraUniform {
        let mut uniform = self.camera.uniform(self.width, self.height);
        if let Some(lighting) = &self.car_lighting {
            lighting.apply(&mut uniform, [1.0; 3]);
        }
        uniform
    }

    /// Camera uniform for a car at `model`, lit with the road colour under it.
    fn car_uniform(&self, model: Mat4) -> CameraUniform {
        let mut uniform = self.camera.uniform(self.width, self.height);
        if let Some(lighting) = &self.car_lighting {
            let road = self
                .shade_grid
                .as_ref()
                .and_then(|grid| grid.sample(model.w_axis.truncate()))
                .map(crate::shade_sample::desaturate)
                .unwrap_or([1.0; 3]);
            lighting.apply(&mut uniform, road);
        }
        uniform.model = model.to_cols_array_2d();
        uniform
    }

    /// Light groups of the player and AI cars (`R3DCar_UpdateLights` 0x439360).
    fn update_lights(&mut self, dt: f32, player: nfs_assets::lights::LightInputs) {
        use nfs_assets::lights::{blink_phase, LightInputs};
        self.light_clock_s += f64::from(dt.max(0.0));
        let now_ms = (self.light_clock_s * 1000.0) as u32;
        let blink = blink_phase(self.light_clock_s);
        let mut player = player;
        if let Some(v) = self.original_vehicle.as_ref().filter(|_| self.original_active()) {
            // The original vehicle has the fields R3DCar_UpdateLights 0x439360 reads.
            let car = &v.car;
            player.brake = if car.reverse_d98 != 0 { car.gas_eff_d7f } else { car.brake_eff_d80 };
            player.reverse_gear = car.gear_d82 == 0;
        }
        self.player_lights.update(&player, now_ms, blink);
        self.ai_lights.resize(self.ai_opponents.len(), Default::default());
        for (lights, ai) in self.ai_lights.iter_mut().zip(&self.ai_opponents) {
            // Approximation A19: AI cars drive with their lights off.
            let inputs = LightInputs {
                brake: brake_byte(ai.current_controls.brake),
                ..Default::default()
            };
            lights.update(&inputs, now_ms, blink);
        }
    }

    /// `Car_ContactEffects` 0x40f740 sparks: each collide tick whose strongest
    /// barrier contact exceeds 1.5 bursts the emitter; particles tick at 32 Hz.
    fn update_sparks(&mut self, dt: f32) {
        let contacts = std::mem::take(&mut self.spark_contacts);
        for c in &contacts {
            self.max_barrier_impact = self.max_barrier_impact.max(c.impact);
        }
        let Some(emitter) = &mut self.spark_emitter else {
            return;
        };
        if let Some(hit) = contacts
            .iter()
            .filter(|c| c.impact > nfs_assets::sparks::SPARK_MIN_IMPACT)
            .max_by(|a, b| a.impact.total_cmp(&b.impact))
        {
            emitter.burst(hit.point, hit.velocity);
            self.spark_bursts += 1;
        }
        self.spark_clock += dt.max(0.0);
        let step = 1.0 / nfs_assets::sparks::PARTICLE_HZ;
        while self.spark_clock >= step {
            self.spark_clock -= step;
            emitter.tick();
        }
    }

    /// Debug: live sparks, bursts so far, strongest barrier contact strength.
    pub fn spark_stats(&self) -> [f32; 6] {
        let first = self
            .spark_emitter
            .as_ref()
            .and_then(|e| e.sparks.first())
            .map(|s| nfs_assets::physics::original_adapter::from_original(s.position))
            .unwrap_or(Vec3::ZERO);
        [
            self.spark_emitter.as_ref().map_or(0, |e| e.sparks.len()) as f32,
            self.spark_bursts as f32,
            self.max_barrier_impact,
            first.x,
            first.y,
            first.z,
        ]
    }

    /// Toggle the player's light switch (original `Controller.Lights`).
    pub fn toggle_lights(&mut self) -> bool {
        self.light_switch = !self.light_switch;
        self.light_switch
    }

    /// Debug: the last solid track object the player hit ("name [tag] size at x,y,z").
    pub fn object_hit_report(&self) -> String {
        let Some(world) = &self.track_world else {
            return String::new();
        };
        let Some(o) = world.last_hit.and_then(|i| world.objects().get(i)) else {
            return String::new();
        };
        let size = [0, 1, 2].map(|i| o.bounds_max[i] - o.bounds_min[i]);
        let c = o.centre();
        format!(
            "{} [{}] {:.1}x{:.1}x{:.1} at {:.0},{:.0},{:.0}",
            o.name,
            String::from_utf8_lossy(&o.tag.to_le_bytes()).trim_end_matches('\0'),
            size[0], size[1], size[2], c[0], c[1], c[2]
        )
    }

    /// Debug (headless checks): smackable objects as `[count, awake, hit, smashed crates]`.
    pub fn smack_stats(&self) -> [f32; 4] {
        let b = &self.smack_world.bodies;
        [
            b.len() as f32,
            b.iter().filter(|x| !x.asleep).count() as f32,
            b.iter().filter(|x| x.car.pair.active_7c).count() as f32,
            self.track_world.as_ref().map_or(0.0, |w| {
                (0..w.objects().len())
                    .filter(|&i| w.state(i) == nfs_assets::physics::original_track_smash::SmashState::Smashed)
                    .count() as f32
            }),
        ]
    }

    /// Debug: place the car at `pos` heading `yaw` (radians) and restart the original model.
    pub fn debug_start(&mut self, pos: [f32; 3], yaw: f32) -> f32 {
        let mut car_pos = Vec3::from(pos);
        let hit = self
            .road_surface
            .as_ref()
            .and_then(|s| s.query(car_pos.x, car_pos.z, car_pos.y, 200.0, 400.0));
        if let Some(hit) = &hit {
            car_pos.y = hit.height;
        }
        if let Some(car) = &mut self.car {
            car.set_pose(car_pos, yaw);
        }
        if let Some(sim) = &mut self.sim_car {
            sim.reset(car_pos + Vec3::Y * sim.resting_height(), yaw);
        }
        self.restart_original_vehicle();
        hit.map_or(-1000.0, |h| h.height)
    }

    pub fn reset_car(&mut self) {
        self.player_camera.restart();
        self.proto_view = None;
        self.body_support_owner = Default::default();
        self.collision_clock = Default::default();
        self.vehicle_contact_count = 0;
        // Smashed objects come back [VA 0x476170].
        if let Some(world) = &mut self.track_world {
            world.reset();
        }
        self.crate_smash.reset();
        let records: Vec<_> = self.smack_props.iter().map(|p| p.record.clone()).collect();
        self.smack_world.reset(&records);
        self.reset_scenario_props();
        if let Some(course) = &self.track_course {
            let (pos, _, yaw) = nfs_assets::calculate_grid_slot(0, course);
            let mut car_pos = Vec3::from(pos);
            let mut car_yaw = yaw;
            // The original start: the player's grid slot on the node graph at the start line.
            if !self.is_mission_mode {
                let racers = 1 + self.ai_opponents.len() as i32;
                if let Some(slot) = self.start_slot(racers, racers) {
                    let p = nfs_assets::physics::original_adapter::from_original(slot.position);
                    let f = nfs_assets::physics::original_adapter::from_original(slot.forward);
                    car_pos = p;
                    car_yaw = (-f.x).atan2(-f.z);
                }
            }
            let scenario_start = self.scenario_start.filter(|_| self.is_mission_mode);
            if let Some(start) = scenario_start {
                let (position, heading) = scenario_start_pose(start);
                car_pos = position;
                car_yaw = heading;
            }
            let hit = self
                .road_surface
                .as_ref()
                .and_then(|surface| surface.query(car_pos.x, car_pos.z, car_pos.y, 25.0, 50.0))
                .or_else(|| {
                    if scenario_start.is_some() {
                        return None;
                    }
                    let gate0 = &course.checkpoints[0];
                    let fwd_pos = [
                        gate0.center[0] + gate0.forward[0] * 5.0,
                        gate0.center[1],
                        gate0.center[2] + gate0.forward[2] * 5.0,
                    ];
                    if let Some(h) = self
                        .road_surface
                        .as_ref()
                        .and_then(|s| s.query(fwd_pos[0], fwd_pos[2], fwd_pos[1], 50.0, 100.0))
                    {
                        car_pos.x = fwd_pos[0];
                        car_pos.z = fwd_pos[2];
                        return Some(h);
                    }
                    let h = self.road_surface.as_ref().and_then(|s| {
                        s.query(
                            gate0.center[0],
                            gate0.center[2],
                            gate0.center[1],
                            50.0,
                            100.0,
                        )
                    });
                    if h.is_some() {
                        car_pos.x = gate0.center[0];
                        car_pos.z = gate0.center[2];
                    }
                    h
                });
            let road_up = hit.as_ref().map(|h| Vec3::from(h.normal));
            if let Some(hit) = hit {
                car_pos.y = hit.height;
            }
            if let Some(car) = &mut self.car {
                car.set_pose(car_pos, car_yaw);
                self.camera.drive_pos = car.camera_eye;
                self.camera.drive_target = car.camera_target;
            }
            if let Some(sim) = &mut self.sim_car {
                // `Car_PlaceOnNode` 0x49b090: the frame is the road's (up = the support plane
                // normal, forward along the heading), so a start on a slope is level with it.
                let up = road_up.filter(|u| u.y > 0.05).unwrap_or(Vec3::Y);
                sim.reset(car_pos + up * sim.resting_height(), car_yaw);
                sim.body.orientation =
                    Quat::from_mat3(&nfs_assets::race::ai::tilted_basis(car_yaw, up)).normalize();
            }
            if let Some(progress) = &mut self.mission_progress {
                progress.reset(car_pos.to_array());
            }
            self.mission_timed_out = false;
            self.course_tracker = Some(nfs_assets::CourseProgressTracker::new(car_pos.to_array()));
            if let Some(session) = &mut self.race_session {
                session.restart(nfs_assets::audio::START_SECS);
            }
            for (i, ai) in self.ai_opponents.iter_mut().enumerate() {
                let slot = i + 1;
                let (ai_pos, ai_fwd, ai_yaw) = nfs_assets::calculate_grid_slot(slot, course);
                let mut p = ai_pos;
                if let Some(hit) = self
                    .road_surface
                    .as_ref()
                    .and_then(|surface| surface.query(p[0], p[2], p[1], 25.0, 50.0))
                {
                    p[1] = hit.height;
                }
                ai.position = p;
                ai.forward = ai_fwd;
                ai.yaw = ai_yaw;
                ai.apply_collision_velocity([0.0; 3]);
                ai.wheel_distance = 0.0;
                ai.current_controls = Default::default();
                ai.lookahead_wp_idx = 1;
                ai.tracker = nfs_assets::CourseProgressTracker::new(p);
                ai.distance_along_course = 0.0;
                ai.laps_completed = 0;
            }
            self.restart_original_vehicle();
            return;
        }

        if let Some(car) = &mut self.car {
            car.reset_to_road(&self.road_edges);
            if let Some(hit) = self
                .road_surface
                .as_ref()
                .and_then(|surface| surface.query(car.pos.x, car.pos.z, car.pos.y, 2.0, 4.0))
            {
                car.pos.y = hit.height;
            }
            if let Some(sim) = &mut self.sim_car {
                sim.reset(car.pos + Vec3::Y * sim.resting_height(), car.yaw);
            }
            if let Some(progress) = &mut self.mission_progress {
                progress.reset(car.pos.to_array());
            }
            self.mission_timed_out = false;
            if self.camera.mode == CameraMode::Drive {
                self.camera.drive_pos = car.camera_eye;
                self.camera.drive_target = car.camera_target;
            }
        }
        self.restart_original_vehicle();
    }

    /// R: the reset key (`C1_RESET`, event 5 of the input word). The next 32 Hz reset tick
    /// takes it and drops it when the original gates refuse (cooldown, speed, race start,
    /// finished), as the original drops the event.
    pub fn request_car_reset(&mut self) {
        self.reset_request = true;
    }

    /// Player position and up axis, from the model that drives.
    fn player_pose(&self) -> Option<(Vec3, Vec3)> {
        if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            return Some((sim.body.position, sim.body.up()));
        }
        self.car.as_ref().map(|c| (c.pos, Vec3::Y))
    }

    /// `g_SchedStep [0x606ac4]` and `+0xb50 != 0` for the reset gates. The scheduler step
    /// counts from the race start: [`START_SCHED_STEP`] at the green light (approximation
    /// A97). Free driving has no race gate.
    ///
    /// [`START_SCHED_STEP`]: nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP
    fn reset_race_clock(&self) -> (i32, bool) {
        use nfs_assets::RacePhase;
        use nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP;
        let Some(session) = self.race_session.as_ref().filter(|_| !self.is_mission_mode) else {
            return (i32::MAX, false);
        };
        match &session.phase {
            RacePhase::Racing { elapsed_secs } => {
                (START_SCHED_STEP + (elapsed_secs * 64.0) as i32, false)
            }
            RacePhase::Finished { .. } | RacePhase::Results => (i32::MAX, true),
            _ => (0, false),
        }
    }

    /// `g_SchedStep` of the original car for the current race phase: counting from 0 at the
    /// start of the countdown to `START_SCHED_STEP` at the green light (approximation A29),
    /// then on with the race time; outside a race, past the start.
    fn original_race_sched(&self) -> i32 {
        use nfs_assets::RacePhase;
        use nfs_assets::physics::original_vehicle::driver::START_SCHED_STEP;
        let Some(session) = self.race_session.as_ref().filter(|_| !self.is_mission_mode) else {
            return START_SCHED_STEP;
        };
        match &session.phase {
            RacePhase::Countdown { remaining_secs, .. } => {
                START_SCHED_STEP - (remaining_secs.max(0.0) * 64.0) as i32
            }
            RacePhase::Racing { elapsed_secs } => {
                START_SCHED_STEP + (elapsed_secs * 64.0) as i32
            }
            _ => START_SCHED_STEP,
        }
    }

    /// Car state for one tick of the reset watch. The original model gives its own fields;
    /// the 6 DOF prototype and the arcade car give equivalents (approximation A96).
    fn reset_watch_inputs(
        &self,
        course: &nfs_assets::TrackCourse,
    ) -> Option<nfs_assets::race::reset::WatchInputs> {
        use nfs_assets::physics::original_adapter::{
            angular_to_original, from_original, speed_estimate, to_original,
        };
        let (pos, up) = self.player_pose()?;
        let (sched_step, out_b50) = self.reset_race_clock();
        let surface = self.road_surface.as_ref();
        let (node, _) = course.find_closest_waypoint(pos.to_array());
        let wp = course.waypoints[node].position;
        let node_up = surface
            .and_then(|s| s.query(wp[0], wp[2], wp[1], 25.0, 50.0))
            .map_or(Vec3::Y, |h| Vec3::from(h.normal));
        let ground = surface.and_then(|s| s.query(pos.x, pos.z, pos.y, 2.0, 6.0));
        // `+0x464 = +0x2c & 0xf` of the support polygon under the car; the legacy grid
        // has no flags (surface 1).
        let surface_464 = match (&ground, surface) {
            (Some(h), _) => h.flags_word_a.map_or(1, |f| (f & 0xf) as i32),
            (None, Some(_)) => 0,
            (None, None) => 1,
        };
        let mut inputs = nfs_assets::race::reset::WatchInputs {
            request: self.reset_request,
            sched_step,
            out_b50,
            speed_35c: 0.0,
            node_up_dot: node_up.dot(up),
            ground_up_3a0: ground
                .as_ref()
                .map_or(up.y, |h| Vec3::from(h.normal).dot(up)),
            controls_db0: true,
            surface_464,
            angular_388: [0.0; 3],
            wheels_off_418: 0,
            height_424: 0.0,
            locator_12: true,
        };
        if let (true, Some(v)) = (self.original_active(), &self.original_vehicle) {
            let car = &v.car;
            inputs.speed_35c = car.speed_35c;
            inputs.node_up_dot = node_up.dot(from_original(car.axes_364[1]));
            inputs.ground_up_3a0 = car.ground_up_3a0;
            inputs.controls_db0 = car.start_db0 != 0;
            inputs.angular_388 = car.angular_388;
            inputs.wheels_off_418 = car.wheels_off_418;
            inputs.height_424 = car.height_424;
            if self.original_ai_track.is_some() {
                // `Car_TrackLocate` runs (prio 6): node up `+0x3c4`, surface `+0x464`
                // and the body owner's "no polygon" byte `car+0x12`.
                inputs.node_up_dot =
                    Vec3::from(car.track.node_up_3c4).dot(Vec3::from(car.axes_364[1]));
                inputs.surface_464 = car.surface_464;
                inputs.locator_12 = v.locator.missing_12();
            }
        } else if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            let body = &sim.body;
            inputs.speed_35c = speed_estimate(to_original(body.linear_velocity));
            inputs.angular_388 = angular_to_original(body.orientation * body.angular_velocity);
            inputs.height_424 = ground
                .as_ref()
                .map_or(2.0, |h| (pos.y - sim.resting_height() - h.height).max(0.0));
        } else if let Some(car) = &self.car {
            inputs.speed_35c = car.speed.abs();
        }
        Some(inputs)
    }

    /// Runs the 32 Hz reset ticks due after `dt`; true if the car was moved.
    fn tick_car_reset(&mut self, dt: f32) -> bool {
        const TICK: f32 = 1.0 / 32.0;
        let Some(course) = self.track_course.take() else {
            self.reset_request = false;
            return false;
        };
        let mut moved = false;
        if !course.waypoints.is_empty() {
            self.reset_clock += dt.clamp(0.0, 0.25);
            while self.reset_clock >= TICK {
                self.reset_clock -= TICK;
                let Some(inputs) = self.reset_watch_inputs(&course) else {
                    break;
                };
                self.reset_request = false;
                if self.reset_watch.tick(&inputs).is_some() {
                    moved |= self.reset_car_to_track(&course);
                }
            }
        }
        self.track_course = Some(course);
        moved
    }

    /// `Car_ResetToTrack 0x40f540` for the player: the car's track node (the nearest course
    /// waypoint), else the next free one, on the road there facing along the track, at rest.
    /// Lap, checkpoint and race state stay. Works for both vehicle models.
    /// `Car_ResetToTrack` 0x40f540 for the original model on the node graph: the locator
    /// node `car+8`, the first free node (`0x40f480`: support polygon at the node centre,
    /// no original AI car on it), `0x49e220` + `0x40f400`, `Car_PlaceOnNode` 0x49b090, then
    /// the probe placement (A39). False without the original model or the graph.
    fn reset_original_on_nodes(&mut self) -> bool {
        use nfs_assets::physics::original_vehicle::RoadGround;
        use nfs_assets::physics::original_vehicle::locate::{place_on_node, reset_node};
        if !self.original_active() {
            return false;
        }
        let (Some(track), Some(surface)) =
            (self.original_ai_track.as_ref(), self.road_surface.as_ref())
        else {
            return false;
        };
        let Some(support) = surface.original_arena() else {
            return false;
        };
        let others: Vec<i16> = self
            .original_ai
            .as_ref()
            .map(|f| f.cars.iter().map(|c| c.ai.locator_08.node_00).collect())
            .unwrap_or_default();
        let racers: Vec<[f32; 3]> = self
            .original_ai
            .as_ref()
            .map(|f| f.cars.iter().map(|c| c.vehicle.car.position_330).collect())
            .unwrap_or_default();
        let Some(v) = self.original_vehicle.as_mut() else {
            return false;
        };
        let node = reset_node(
            &mut v.locator,
            &track.nodes,
            Some(support),
            &others,
            track.reverse,
        );
        v.reset_drive_state();
        let mut racers = racers;
        racers.push(v.car.position_330);
        place_on_node(
            &mut v.car,
            &mut v.locator,
            &track.nodes,
            Some(support),
            node,
            track.reverse,
            &racers,
        );
        let (pos, axes) = (v.car.position_330, v.car.axes_364);
        let mut owners = v.owners.clone();
        v.set_pose(
            pos,
            axes,
            [0.0; 3],
            [0.0; 3],
            &mut RoadGround {
                surface,
                owners: &mut owners,
            },
        );
        v.owners = owners;
        self.body_support_owner = Default::default();
        self.sync_sim_from_original(false);
        true
    }

    fn reset_car_to_track(&mut self, course: &nfs_assets::TrackCourse) -> bool {
        use nfs_assets::race::reset::{reset_node, reset_pose};
        if self.reset_original_on_nodes() {
            if let Some(view) = self.camera_car() {
                self.player_camera
                    .snap_to_car(&view, &mut nfs_assets::camera::FlatWorld);
            }
            self.car_resets += 1;
            return true;
        }
        let Some((pos, _)) = self.player_pose() else {
            return false;
        };
        let surface = self.road_surface.as_ref();
        let query = |p: [f32; 3]| surface.and_then(|s| s.query(p[0], p[2], p[1], 25.0, 50.0));
        // `0x40f480`: a road surface at the node and no other active car on it.
        let ai_nodes: Vec<usize> = self
            .ai_opponents
            .iter()
            .map(|a| course.find_closest_waypoint(a.position).0)
            .collect();
        let (start, _) = course.find_closest_waypoint(pos.to_array());
        let node = reset_node(course, start, |k| {
            !ai_nodes.contains(&k)
                && (surface.is_none()
                    || query(course.waypoints[k].position)
                        .is_some_and(|h| h.flags_word_a.is_none_or(|f| f & 0xf != 0)))
        });
        let hit = query(course.waypoints[node].position);
        let pose = reset_pose(
            course,
            node,
            hit.as_ref().map(|h| Vec3::from(h.normal)),
            hit.as_ref().map(|h| h.height),
        );
        if let Some(car) = &mut self.car {
            car.set_pose(pose.position, pose.yaw);
            if self.camera.mode == CameraMode::Drive && !self.sim_mode {
                self.camera.drive_pos = car.camera_eye;
                self.camera.drive_target = car.camera_target;
            }
        }
        if let Some(sim) = &mut self.sim_car {
            let lift = sim.resting_height();
            sim.reset(pose.position + pose.up * lift, pose.yaw);
            sim.body.orientation = Quat::from_mat3(&pose.basis()).normalize();
        }
        self.body_support_owner = Default::default();
        if self.original_active() {
            // `0x49e220` + `0x40f400`, then `Car_PlaceOnNode 0x49b090`: the probe placement of
            // approximation A39 lowers the car onto the road.
            if let Some(v) = &mut self.original_vehicle {
                v.reset_drive_state();
            }
            self.sync_original_from_sim(false);
            self.sync_sim_from_original(false);
        }
        // `0x4618b0(0)`: the player camera snaps to the car.
        if let Some(view) = self.camera_car() {
            self.player_camera
                .snap_to_car(&view, &mut nfs_assets::camera::FlatWorld);
        }
        self.car_resets += 1;
        true
    }

    pub fn get_race_phase(&self) -> u32 {
        if self.paused {
            return 3;
        }
        if self.is_mission_mode {
            return if self.mission_timed_out || self.get_mission_goal_reached() {
                4
            } else {
                2
            };
        }
        match self.race_session.as_ref().map(|s| &s.phase) {
            None | Some(nfs_assets::RacePhase::Loading) => 0,
            Some(nfs_assets::RacePhase::Countdown { .. }) => 1,
            Some(nfs_assets::RacePhase::Racing { .. }) => 2,
            Some(nfs_assets::RacePhase::Paused { .. }) => 3,
            Some(nfs_assets::RacePhase::Finished { .. }) => 4,
            Some(nfs_assets::RacePhase::Results) => 5,
        }
    }

    pub fn get_countdown_remaining(&self) -> f32 {
        if self.is_mission_mode {
            return 0.0;
        }
        match self.race_session.as_ref().map(|s| &s.phase) {
            Some(nfs_assets::RacePhase::Countdown { remaining_secs, .. }) => *remaining_secs,
            _ => 0.0,
        }
    }

    pub fn get_player_position(&self) -> usize {
        self.race_session
            .as_ref()
            .map(|s| s.player_position())
            .unwrap_or(1)
    }

    pub fn get_total_participants(&self) -> usize {
        self.race_session
            .as_ref()
            .map(|s| s.participants.len())
            .unwrap_or(1)
    }

    pub fn get_current_lap(&self) -> u32 {
        self.race_session
            .as_ref()
            .map(|s| s.lap_tracker.current_lap)
            .unwrap_or(1)
    }

    pub fn get_total_laps(&self) -> u32 {
        self.race_session
            .as_ref()
            .map(|s| s.lap_tracker.total_laps)
            .unwrap_or(1)
    }

    pub fn get_current_lap_time(&self) -> f32 {
        if self.is_mission_mode {
            return self.mission_elapsed_time;
        }
        self.race_session
            .as_ref()
            .map(|s| s.lap_tracker.current_lap_time)
            .unwrap_or(0.0)
    }

    pub fn get_best_lap_time(&self) -> f32 {
        self.race_session
            .as_ref()
            .and_then(|s| s.lap_tracker.best_lap_time)
            .unwrap_or(0.0)
    }

    pub fn is_wrong_way(&self) -> bool {
        self.is_wrong_way
    }

    pub fn restart_race(&mut self) {
        self.set_paused(false);
        self.reset_car();
        if self.is_mission_mode {
            self.mission_elapsed_time = 0.0;
            self.prop_hit.fill(false);
            if let Some(detector) = &mut self.stunt_detector {
                let initial_yaw = if let Some(c) = &self.car { c.yaw } else { 0.0 };
                detector.reset(initial_yaw);
            }
        }
    }

    /// Start a standalone race on the loaded course with the selected grid size.
    pub fn configure_quick_race(&mut self, laps: u32, opponents: u32) -> Result<(), String> {
        if !(1..=8).contains(&laps) {
            return Err("Quick Race laps must be 1 to 8".into());
        }
        if opponents > 7 {
            return Err("Quick Race opponents must be 0 to 7".into());
        }
        let course = self
            .track_course
            .as_ref()
            .ok_or("Quick Race requires a track course")?;
        if course.checkpoints.is_empty() {
            return Err("Quick Race track course has no starting grid".into());
        }
        let (session, ai) = quick_race_grid(course, laps, opponents);
        self.paused = false;
        self.race_session = Some(session);
        self.ai_opponents = ai;
        for ai in &mut self.ai_opponents {
            ai.speed_limit = self.ai_speed_limit;
            ai.performance = self.ai_performance;
        }
        self.is_mission_mode = false;
        self.mission_progress = None;
        self.mission_timed_out = false;
        self.mission_time_limit = 0.0;
        self.mission_elapsed_time = 0.0;
        self.stunt_detector = None;
        self.prop_hit.fill(false);
        self.is_wrong_way = false;
        self.reset_car();
        Ok(())
    }

    pub fn configure_mission(&mut self, is_factory: bool, time_limit: f32, has_opponent: bool) {
        self.is_mission_mode = is_factory;
        self.mission_progress = None;
        self.mission_timed_out = false;
        self.mission_time_limit = time_limit;
        self.mission_elapsed_time = 0.0;
        self.prop_hit.fill(false);
        if is_factory {
            let mut det = nfs_game::factory_driver::StuntDetector::new();
            let initial_yaw = if let Some(c) = &self.car { c.yaw } else { 0.0 };
            det.reset(initial_yaw);
            self.stunt_detector = Some(det);
            if !has_opponent {
                self.ai_opponents.clear();
                if let Some(session) = &mut self.race_session {
                    session.participants.retain(|p| p.is_player);
                    session.lap_tracker.total_laps = 1;
                }
            } else {
                self.ai_opponents.truncate(1);
                if let Some(session) = &mut self.race_session {
                    session.participants.truncate(2);
                    session.lap_tracker.total_laps = 1;
                }
            }
        } else {
            self.stunt_detector = None;
        }
    }

    /// Selects the only mission route whose SCN trigger order has been audited.
    /// Other mission codes remain unsupported and cannot win via generic laps.
    pub fn configure_mission_code(&mut self, code: &str) {
        self.mission_progress = None;
        self.mission_timed_out = false;
        if self.is_mission_mode
            && code.eq_ignore_ascii_case("0M01")
            && self.trigger_half_extents.is_some()
            && self.scenario_name.as_deref() == Some("skidpad_st1.scn")
        {
            let start = self
                .car
                .as_ref()
                .map(|c| c.pos.to_array())
                .unwrap_or([0.0; 3]);
            self.mission_progress =
                nfs_assets::scenario::ScenarioProgress::new(self.scenario_gates.clone(), start);
        }
        self.reset_scenario_props();
    }

    fn activate_scenario_prop(&mut self, target: nfs_assets::scenario::PropTarget) {
        nfs_assets::scenario::select_triggerable_prop(&mut self.scenario_props, target);
        self.sync_scenario_prop_transforms();
    }

    fn trigger_vehicle(&self) -> Option<nfs_assets::scenario::TriggerVehicle> {
        let half_extents = self.trigger_half_extents?;
        let (position, forward, right, up, velocity) = if self.sim_mode {
            let body = &self.sim_car.as_ref()?.body;
            (
                body.position,
                body.forward(),
                body.right(),
                body.up(),
                body.linear_velocity,
            )
        } else {
            let car = self.car.as_ref()?;
            (
                car.pos + Vec3::Y * half_extents.y,
                car.forward(),
                car.right(),
                Vec3::Y,
                car.forward() * car.speed,
            )
        };
        Some(nfs_assets::scenario::TriggerVehicle {
            position,
            forward,
            right,
            up,
            velocity,
            half_extents,
            expanded: false,
        })
    }

    fn sync_scenario_prop_transforms(&mut self) {
        for (draw, prop) in self.prop_draws.iter_mut().zip(&self.scenario_props) {
            draw.model.w_axis.y = prop.target.position[1];
        }
    }

    fn reset_scenario_props(&mut self) {
        nfs_assets::scenario::hide_triggerable_props(&mut self.scenario_props);
        if self.is_mission_mode
            && self.mission_progress.is_some()
            && let Some(target) = self.scenario_start.and_then(|start| start.linked_prop)
        {
            nfs_assets::scenario::select_triggerable_prop(&mut self.scenario_props, target);
        }
        self.sync_scenario_prop_transforms();
    }

    pub fn get_mission_goal_supported(&self) -> bool {
        self.mission_progress.is_some()
    }

    pub fn get_mission_passed_gates(&self) -> u32 {
        self.mission_progress
            .as_ref()
            .map_or(0, |p| p.passed_gates() as u32)
    }

    pub fn get_scenario_prop_positions(&self) -> Vec<f32> {
        self.scenario_props
            .iter()
            .filter(|p| p.triggerable)
            .flat_map(|p| p.target.position)
            .collect()
    }

    pub fn get_mission_goal_reached(&self) -> bool {
        !self.mission_timed_out && self.mission_progress.as_ref().is_some_and(|p| p.complete())
    }

    pub fn get_mission_timed_out(&self) -> bool {
        self.mission_timed_out
    }

    pub fn get_mission_elapsed_time(&self) -> f32 {
        self.mission_elapsed_time
    }

    pub fn get_cone_hits(&self) -> u32 {
        self.stunt_detector
            .as_ref()
            .map(|d| d.cone_hits)
            .unwrap_or(0)
    }

    pub fn get_stunt_180(&self) -> bool {
        self.stunt_detector
            .as_ref()
            .map(|d| d.has_done_180)
            .unwrap_or(false)
    }

    pub fn get_stunt_360(&self) -> bool {
        self.stunt_detector
            .as_ref()
            .map(|d| d.has_done_360)
            .unwrap_or(false)
    }

    pub fn get_stunt_jturn(&self) -> bool {
        self.stunt_detector
            .as_ref()
            .map(|d| d.has_done_jturn)
            .unwrap_or(false)
    }

    pub fn is_mission_mode(&self) -> bool {
        self.is_mission_mode
    }

    /// Camera input of the 6 DOF player car: the collision-box car of the original
    /// collisions (`camera_car_from_sim`).
    fn camera_car(&self) -> Option<nfs_assets::camera::CarView> {
        use nfs_assets::physics::original_adapter::camera_car_from_sim;
        if let (true, Some(v)) = (self.original_active(), &self.original_vehicle) {
            return Some(nfs_assets::physics::original_vehicle::driver::camera_car(
                &v.car,
                v.half_extents,
            ));
        }
        let (true, Some(sim)) = (self.sim_mode, self.sim_car.as_ref()) else {
            return None;
        };
        let (center, half) = self
            .car_collision_box
            .unwrap_or((Vec3::ZERO, FALLBACK_HALF_EXTENTS));
        let adapter = self
            .car_render
            .as_ref()
            .map(|r| car_chassis_adapter_body(sim, r))
            .unwrap_or(Vec3::ZERO);
        Some(camera_car_from_sim(sim, center + adapter, half))
    }

    /// Steps the original camera at 64 Hz and builds this frame's view from it.
    fn update_original_camera(&mut self, dt: f32) {
        use nfs_assets::physics::original_adapter::from_original;
        let Some(car) = self.camera_car() else {
            return;
        };
        // No track nodes or ceilings yet: flat slope, no clamp (approximation). The
        // original vehicle model steps the camera in its scheduler.
        if !self.original_active() {
            // The prototype car moves per frame; the 64 Hz camera steps see it where it was at
            // the step's time (A106).
            let prev = self.proto_prev_car.replace(car).unwrap_or(car);
            let mut states = self.proto_view.take();
            let mut steps = Vec::new();
            self.player_camera.advance_sampled(
                dt,
                |f| nfs_assets::render_interp::blend_car_view(&prev, &car, f),
                &mut nfs_assets::camera::FlatWorld,
                |c, car| steps.push(c.peek_view(car.pos_330)),
            );
            let current = self.player_camera.peek_view(car.pos_330);
            let st = states.get_or_insert_with(|| nfs_assets::render_interp::ViewStates::new(current));
            for v in steps {
                st.push(v);
            }
            st.alpha = self.player_camera.fraction();
            self.proto_view = states;
        } else {
            self.proto_view = None;
            self.proto_prev_car = None;
        }
        if self.camera.mode != CameraMode::Drive {
            return;
        }
        if self.free_camera {
            self.camera.use_world_view();
            self.camera.drive_fov = 55.0;
            let target = match (self.original_active(), &self.drawn_states) {
                (true, Some(d)) => d.car_pose().position,
                _ => car.pos_330,
            };
            self.camera.drive_target = from_original(target);
            return;
        }
        let mut view = self.player_camera.view(car.pos_330);
        if let (true, Some(d)) = (self.original_active(), &self.drawn_states) {
            view = d.camera_view();
        } else if let Some(d) = &self.proto_view {
            view = d.camera_view();
        }
        if self.side_view_deg != 0.0 {
            let pivot = match (self.original_active(), &self.drawn_states) {
                (true, Some(d)) => d.car_pose().position,
                _ => car.pos_330,
            };
            view = swing_view(&view, pivot, self.side_view_deg);
        }
        apply_original_view(&mut self.camera, &view);
    }

    /// Shift+V (debug): drive view off, swung `SIDE_VIEW_DEG` one way, then the other. Returns the angle.
    pub fn cycle_side_view(&mut self) -> f32 {
        self.side_view_deg = match self.side_view_deg {
            a if a == 0.0 => SIDE_VIEW_DEG,
            a if a > 0.0 => -SIDE_VIEW_DEG,
            _ => 0.0,
        };
        self.side_view_deg
    }

    /// Name of the drive camera shown in the HUD.
    pub fn camera_name(&self) -> String {
        if self.sim_mode && self.sim_car.is_some() {
            if self.free_camera {
                return "Free".into();
            }
            return self.player_camera.name().into();
        }
        match self.car.as_ref().map(|c| c.view_mode) {
            Some(crate::arcade::DriveViewMode::Bumper) => "Bumper".into(),
            Some(crate::arcade::DriveViewMode::Free) => "Free".into(),
            _ => "Chase".into(),
        }
    }

    /// C key: in 6 DOF driving the original cycle (`Camera_Cycle` over the Options →
    /// Kameras defaults BumperCam, BumperCam2, TailCam0, TailCam1, with the settle period), otherwise the arcade views. Leaves the
    /// free camera. Returns the new camera name.
    pub fn cycle_car_view(&mut self) -> String {
        if let Some(car) = self.camera_car() {
            self.free_camera = false;
            self.player_camera
                .cycle(&car, &mut nfs_assets::camera::FlatWorld);
            self.drawn_states = None;
            self.proto_view = None;
            if self.camera.mode == CameraMode::Drive {
                let view = self.player_camera.view(car.pos_330);
                apply_original_view(&mut self.camera, &view);
            }
        } else if let Some(car) = &mut self.car {
            car.cycle_view_mode();
            if self.camera.mode == CameraMode::Drive {
                self.camera.drive_pos = car.camera_eye;
                self.camera.drive_target = car.camera_target;
            }
        }
        self.camera_name()
    }

    /// V key (debug): free camera on/off. The eye stays where it is and keeps looking at the
    /// car; the original camera keeps stepping underneath.
    pub fn toggle_free_camera(&mut self) -> bool {
        self.free_camera = !self.free_camera;
        self.free_camera
    }

    pub fn cycle_car_paint(&mut self) -> usize {
        if let (Some(car), Some(car_render)) = (&mut self.car, &mut self.car_render) {
            let color_idx = car.cycle_paint();
            let color = crate::car_mesh::CAR_COLORS[color_idx];
            let material_uniform = [
                color[0],
                color[1],
                color[2],
                color[3],
                alpha_mode_code(AlphaMode::Opaque),
                0.0,
                0.0,
                0.0,
            ];
            self.queue.write_buffer(
                &car_render.materials[crate::car_mesh::MAT_BODY].color_buffer,
                0,
                bytemuck::cast_slice(&material_uniform),
            );
            color_idx
        } else {
            0
        }
    }

    pub fn get_car_speed_kmh(&self) -> f32 {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            return Self::original_speed_kmh(&v.car);
        }
        if let (true, Some(tel)) = (self.sim_mode, &self.sim_telemetry) {
            return tel.speed_kmh;
        }
        self.car.as_ref().map(|c| c.speed_kmh()).unwrap_or(0.0)
    }

    pub fn get_car_gear(&self) -> i32 {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            return v.hud_gear();
        }
        if let (true, Some(tel)) = (self.sim_mode, &self.sim_telemetry) {
            return tel.current_gear;
        }
        self.car.as_ref().map(|c| c.gear).unwrap_or(0)
    }

    pub fn get_car_rpm(&self) -> f32 {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            return (v.car.rpm_db4 / 7000.0).clamp(0.0, 1.0);
        }
        if let (true, Some(tel)) = (self.sim_mode, &self.sim_telemetry) {
            return (tel.engine_rpm / 7000.0).clamp(0.0, 1.0);
        }
        self.car.as_ref().map(|c| c.rpm).unwrap_or(0.0)
    }

    /// Engine RPM (not normalised). Without the SIM model the arcade RPM fraction is scaled by 7000.
    pub fn get_car_engine_rpm(&self) -> f32 {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            return v.car.rpm_db4.max(0.0);
        }
        if let (true, Some(tel)) = (self.sim_mode, &self.sim_telemetry) {
            return tel.engine_rpm.max(0.0);
        }
        self.car.as_ref().map(|c| c.rpm * 7000.0).unwrap_or(0.0)
    }

    /// Original race HUD telemetry: `[speed_ec4 m/s, engine_rpm, gear_raw, redline_rpm,
    /// damage x9]`. `speed_ec4` is the original car+0xec4 (VA 0x40eec0):
    /// `max(|vx|,|vz|) + 0.25*min(|vx|,|vz|)` of the world velocity. `gear_raw` is the
    /// car+0xd82 encoding (0 R, 1 N, 2.. 1st..). Damage is car+0x4e4..0x504, 9 zones (VA 0x422470)
    /// for the original model, zeros otherwise.
    pub fn get_hud_telemetry(&self) -> Vec<f32> {
        let mut damage = [0.0f32; 9];
        let (vx, vz, gear_raw);
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            vx = v.car.velocity_33c[0];
            vz = v.car.velocity_33c[2];
            gear_raw = v.car.gear_d82 as f32;
            damage.copy_from_slice(&v.car.damage_4e4[..9]);
        } else if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            vx = sim.body.linear_velocity.x;
            vz = sim.body.linear_velocity.z;
            gear_raw = (self.get_car_gear() + 1).max(0) as f32;
        } else {
            let fwd = self.car.as_ref().map(|c| c.forward());
            let ms = self.get_car_speed_kmh() / 3.6;
            vx = fwd.map(|f| f.x * ms).unwrap_or(0.0);
            vz = fwd.map(|f| f.z * ms).unwrap_or(0.0);
            gear_raw = (self.get_car_gear() + 1).max(0) as f32;
        }
        let (a, b) = (vx.abs(), vz.abs());
        let speed = a.max(b) + 0.25 * a.min(b);
        let mut out = vec![
            speed,
            self.get_car_engine_rpm(),
            gear_raw,
            self.get_car_redline_rpm(),
        ];
        out.extend_from_slice(&damage);
        out
    }

    /// Race standings as a JSON array, one object per participant:
    /// `{"name","player","rank","laps","progress","finished"}` (`finished` is the total
    /// time in seconds or null). Empty array without a race.
    pub fn get_race_standings_json(&self) -> String {
        let Some(session) = &self.race_session else {
            return "[]".to_string();
        };
        let items: Vec<String> = session
            .participants
            .iter()
            .map(|p| {
                let name: String = p
                    .name
                    .chars()
                    .flat_map(|c| match c {
                        '"' => "\\\"".chars().collect::<Vec<_>>(),
                        '\\' => "\\\\".chars().collect(),
                        c if (c as u32) < 0x20 => " ".chars().collect(),
                        c => vec![c],
                    })
                    .collect();
                let fin = match p.total_time_secs {
                    Some(t) if t.is_finite() => format!("{t}"),
                    _ => "null".to_string(),
                };
                let progress = if p.distance_along_track.is_finite() {
                    p.distance_along_track
                } else {
                    0.0
                };
                format!(
                    "{{\"name\":\"{}\",\"player\":{},\"rank\":{},\"laps\":{},\"progress\":{},\"finished\":{}}}",
                    name, p.is_player, p.current_position, p.laps_completed, progress, fin
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// HUD track map: 4 header floats (bounding box centre x, z, extent, 1 for a
    /// circuit) then line segments `x0, z0, x1, z1` in engine world units. Built from the
    /// node graph as `HTrackMap_BuildOutline` 0x432230 builds it (one polyline per
    /// section plus junction connectors); without a node graph (A87) the `.lsp`
    /// racing line of the course, one polyline.
    pub fn get_course_map(&self) -> Vec<f32> {
        use nfs_assets::race::track_map::TrackMap;
        if let Some(t) = self.original_ai_track.as_ref() {
            let n = &t.nodes.nodes;
            if n.len() >= 2 {
                let m = TrackMap::build(&t.nodes);
                let (a, b) = (n[0].pos_04, n[n.len() - 1].pos_04);
                let closed = ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() < 100.0;
                // Node coordinates are the original's; the engine world has z negated
                // (`from_original`), like the car markers.
                let mut out = vec![m.centre[0] as f32, -(m.centre[1] as f32), m.extent, closed as u8 as f32];
                out.extend(m.segments().iter().flat_map(|s| [s[0], -s[1], s[2], -s[3]]));
                return out;
            }
        }
        let Some(c) = self.track_course.as_ref() else {
            return Vec::new();
        };
        let pts: Vec<[f32; 2]> = c.waypoints.iter().map(|w| [w.position[0], w.position[2]]).collect();
        if pts.len() < 3 {
            return Vec::new();
        }
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in &pts {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            z0 = z0.min(p[1]);
            z1 = z1.max(p[1]);
        }
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let closed = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 100.0;
        let mut out = vec![(x0 + x1) / 2.0, (z0 + z1) / 2.0, (x1 - x0).max(z1 - z0), closed as u8 as f32];
        for w in pts.windows(2) {
            out.extend([w[0][0], w[0][1], w[1][0], w[1][1]]);
        }
        out
    }

    /// Car markers, 8 floats per car, player first then AI:
    /// `x, z, forward_x, forward_z, half_width, half_length, is_player, paint index or -1`.
    /// Half extents come from the original car `+0x3a8` when the original model drives,
    /// else 0.9 / 2.1.
    pub fn get_car_markers(&self) -> Vec<f32> {
        let mut out = Vec::new();
        let (mut hw, mut hl) = (0.9f32, 2.1f32);
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            hw = v.car.half_extents_3a8[0];
            hl = v.car.half_extents_3a8[2];
        }
        let paint = self
            .car
            .as_ref()
            .map(|c| c.paint_color_idx as f32)
            .unwrap_or(-1.0);
        let pose = if self.sim_mode {
            self.sim_car
                .as_ref()
                .map(|s| (s.body.position, s.body.forward()))
        } else {
            self.car.as_ref().map(|c| (c.pos, c.forward()))
        };
        if let Some((p, f)) = pose {
            out.extend_from_slice(&[p.x, p.z, f.x, f.z, hw, hl, 1.0, paint]);
        }
        for ai in &self.ai_opponents {
            out.extend_from_slice(&[
                ai.position[0],
                ai.position[2],
                ai.forward[0],
                ai.forward[2],
                0.9,
                2.1,
                0.0,
                -1.0,
            ]);
        }
        out
    }

    /// SIM redline RPM of the loaded car (7000 when the arcade car is driven).
    /// Car state for the tyre sounds as 16 floats (`tyre_audio::inputs_from_slice`): skids of
    /// the front and rear pairs, `+0x78`, height, ground normal, surface, `+0x540`, tyre type,
    /// `+0x52c` bit 4, forward speed, rpm, redline, wheels off, `res+0xc == 1`. The original
    /// model gives its own fields; the 6 DOF prototype gives equivalents (approximations A108,
    /// A112); the arcade car makes no tyre sounds.
    pub fn get_tyre_sound_inputs(&self) -> Vec<f32> {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            let c = &v.car;
            let skid = |i: usize| c.wheels[i].tyre.skid_b8;
            return vec![
                skid(0),
                skid(1),
                skid(2),
                skid(3),
                c.track.dist_78,
                c.height_424,
                c.ground_up_3a0,
                (c.body_surface_2c & 0xf) as f32,
                (c.field_540 != 0) as u8 as f32,
                c.specs.tyre_type_1c0 as f32,
                (c.flags_52c & 4 != 0) as u8 as f32,
                c.vel_fwd_d60(),
                c.rpm_db4,
                c.specs.redline_94,
                (c.wheels_off_418 > 5) as u8 as f32,
                1.0,
            ];
        }
        let (true, Some(tel), Some(sim)) = (self.sim_mode, &self.sim_telemetry, &self.sim_car)
        else {
            return Vec::new();
        };
        let moving = tel.speed_mps.abs() > 2.0;
        let skid = |i: usize| -> f32 {
            if !moving || !tel.wheels_in_contact[i] {
                return 0.0;
            }
            let angle = ((tel.wheel_slip_angles[i].abs() - 0.14) / 0.20).clamp(0.0, 1.0);
            let ratio = ((tel.wheel_slip_ratios[i].abs() - 0.25) / 0.5).clamp(0.0, 1.0);
            angle.max(ratio)
        };
        let surface = match (self.player_pose(), self.road_surface.as_ref()) {
            (Some((pos, _)), Some(road)) => road
                .query(pos.x, pos.z, pos.y, 2.0, 6.0)
                .map_or(0, |h| h.flags_word_a.map_or(1, |f| (f & 0xf) as i32)),
            _ => 1,
        };
        let airborne = !tel.wheels_in_contact.iter().any(|&c| c);
        vec![
            skid(0),
            skid(1),
            skid(2),
            skid(3),
            0.0,
            if airborne { 1.0 } else { 0.0 },
            1.0,
            surface as f32,
            0.0,
            0.0,
            0.0,
            tel.speed_mps,
            tel.engine_rpm,
            sim.powertrain.redline_rpm,
            0.0,
            1.0,
        ]
    }

    pub fn get_car_redline_rpm(&self) -> f32 {
        if let Some(v) = self
            .original_vehicle
            .as_ref()
            .filter(|_| self.original_active())
        {
            return v.car.specs.redline_94;
        }
        if let (true, Some(sim)) = (self.sim_mode, &self.sim_car) {
            return sim.powertrain.redline_rpm;
        }
        7000.0
    }

    /// Current drive camera for the engine sound: 0 Chase, 1 Bumper (in-car), 2 Free (other
    /// exterior views). The original cameras map TailCam to Chase, BumperCam/BumperCam2 to
    /// Bumper and HeliCam to Free.
    pub fn get_car_view(&self) -> u32 {
        if self.sim_mode && self.sim_car.is_some() {
            return match self.player_camera.id() {
                _ if self.free_camera => 2,
                0 | 1 => 1,
                2..=4 => 0,
                _ => 2,
            };
        }
        match self.car.as_ref().map(|c| c.view_mode) {
            Some(crate::arcade::DriveViewMode::Bumper) => 1,
            Some(crate::arcade::DriveViewMode::Free) => 2,
            _ => 0,
        }
    }

    pub fn get_car_wheel_render_state(&self) -> Vec<f32> {
        let (Some(sim), Some(car_render)) = (&self.sim_car, &self.car_render) else {
            return Vec::new();
        };
        let adapter = car_chassis_adapter_body(sim, car_render);
        let mut state = Vec::with_capacity(3 + car_render.wheels.len() * 9);
        state.extend_from_slice(&adapter.to_array());
        for wheel in &car_render.wheels {
            let world_center = prototype_wheel_world_matrix(
                sim,
                wheel.wheel_index,
                wheel.source_center,
                adapter,
            )
            .map(|matrix| matrix.transform_point3(Vec3::ZERO))
            .unwrap_or(Vec3::ZERO);
            state.extend_from_slice(&[
                wheel.wheel_index as f32,
                wheel.source_center.x,
                wheel.source_center.y,
                wheel.source_center.z,
                wheel.radius,
                world_center.x,
                world_center.y,
                world_center.z,
                wheel.mesh_indices.len() as f32,
            ]);
        }
        state
    }

    pub fn get_car_wheel_visuals(&self) -> Vec<f32> {
        let (Some(sim), Some(car_render)) = (&self.sim_car, &self.car_render) else {
            return Vec::new();
        };
        if car_render.wheels.len() != 4 {
            return Vec::new();
        }
        let adapter = car_chassis_adapter_body(sim, car_render);
        let mut visuals = Vec::with_capacity(4 * 16);
        for wheel_index in 0..4 {
            let Some(wheel) = car_render
                .wheels
                .iter()
                .find(|wheel| wheel.wheel_index == wheel_index)
            else {
                return Vec::new();
            };
            let Some(wheel_matrix) = prototype_wheel_world_matrix(
                sim,
                wheel.wheel_index,
                wheel.source_center,
                adapter,
            ) else {
                return Vec::new();
            };
            let render_matrix = wheel_matrix * Mat4::from_translation(-wheel.source_center);
            visuals.extend_from_slice(&render_matrix.to_cols_array());
        }
        visuals
    }

    pub fn get_car_wheel_mesh_count(&self) -> u32 {
        self.car_render
            .as_ref()
            .map(|car_render| {
                car_render
                    .wheels
                    .iter()
                    .map(|wheel| wheel.mesh_indices.len() as u32)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Original collision passes due this frame (32 Hz), on the player car
    /// and the AI cars. See nfs_assets::race::original_collision.
    /// Starts the crate animations of the smashes the last collision passes found.
    fn process_smash_events(&mut self) {
        for event in std::mem::take(&mut self.smash_events) {
            if let Some(i) = self
                .smashables
                .iter()
                .position(|s| s.object_index == event.object_index)
            {
                self.crate_smash.smash(
                    i,
                    self.smash_clock,
                    event.speed,
                    &self.smashables,
                    &self.smash_props,
                );
            }
        }
    }

    fn run_original_collision(&mut self, dt: f32) {
        use nfs_assets::physics::original_adapter::apply_pair_car_to_sim;
        use nfs_assets::race::original_collision::{
            apply_barrier_state, apply_pair_car_to_ai, barrier_state,
        };
        let passes = self.collision_clock.advance(dt);
        let wheel_points_for_objects = self.original_wheel_points();
        let (Some(sim), Some((center, half))) = (self.sim_car.as_mut(), self.car_collision_box)
        else {
            return;
        };
        let adapter = self
            .car_render
            .as_ref()
            .map(|r| car_chassis_adapter_body(sim, r))
            .unwrap_or(Vec3::ZERO);
        let surface = self
            .road_surface
            .as_ref()
            .filter(|s| s.uses_original_support());
        let wheel_points = wheel_points_for_objects;
        for _ in 0..passes {
            let mut cars =
                build_collision_pair_cars(sim, &self.ai_opponents, center, adapter, half);
            let n_cars = cars.len();
            let awake = self.smack_world.awake();
            cars.extend(
                awake
                    .iter()
                    .map(|&i| self.smack_world.bodies[i].car.pair.clone()),
            );
            let before = cars.clone();
            let owner = &mut self.body_support_owner;
            let contacts = &mut self.spark_contacts;
            let mut world = self.track_world.as_mut();
            let events = &mut self.smash_events;
            nfs_assets::race::original_collision::collide_pass_with(&mut cars, &mut self.pair_globals, |index, car, globals| {
                // AI cars are kinematic and follow the course: no barrier or
                // object pass (approximations A6, A173). The object pass
                // follows the edge pass [VA 0x498ff0].
                if index != 0 {
                    return;
                }
                if let Some(surface) = surface {
                    let mut state = barrier_state(car);
                    if let Ok(Some(hit)) =
                        surface.original_barrier_contact(&mut state, car.extents_3a8, owner)
                    {
                        apply_barrier_state(car, &state);
                        contacts.push(SparkContact {
                            impact: hit.magnitude,
                            point: hit.point,
                            velocity: state.velocity_33c,
                        });
                    }
                }
                if let Some(world) = world.as_deref_mut() {
                    let mut o = nfs_assets::physics::original_track_pass::object_car_from_pair(
                        car,
                        &wheel_points,
                    );
                    world.pass(&mut o, globals, events);
                    *car = o.pair;
                }
            });
            if cars[0] != before[0] {
                apply_pair_car_to_sim(sim, &before[0], &cars[0]);
                // The original pair/barrier response doesn't touch wheels; the
                // 6 DOF body needs them matched (approximation A2).
                sim.sync_wheel_spin_to_body();
            }
            for (i, ai) in self.ai_opponents.iter_mut().enumerate() {
                apply_pair_car_to_ai(ai, &before[i + 1], &cars[i + 1]);
            }
            for (k, &i) in awake.iter().enumerate() {
                let body = &mut self.smack_world.bodies[i];
                body.car.pair = cars[n_cars + k].clone();
                body.update_world_points();
            }
        }
    }

    /// Runs the smackable objects' own steps and returns nothing; the
    /// pair collisions with them happen in the collision passes.
    fn advance_smackables(&mut self, dt: f32) {
        use nfs_assets::physics::original_adapter::to_original;
        if self.smack_world.bodies.is_empty() {
            return;
        }
        let mut cars: Vec<[f32; 3]> = Vec::new();
        if let Some(v) = self.original_vehicle.as_ref().filter(|_| self.original_active()) {
            cars.push(v.car.position_330);
        } else if let Some(sim) = &self.sim_car {
            cars.push(to_original(sim.body.position));
        }
        cars.extend(
            self.ai_opponents
                .iter()
                .map(|a| to_original(Vec3::from(a.position))),
        );
        let surface = self.road_surface.as_ref();
        let owners = &mut self.smack_owners;
        let mut ground = |p: [f32; 3]| {
            surface.and_then(|surface| {
                use nfs_assets::physics::original_vehicle::ground::{GroundQuery, RoadGround};
                RoadGround { surface, owners }.wheel_ground(0, p)
            })
        };
        self.smack_world
            .advance(dt, &cars, &mut ground, &mut self.pair_globals);
    }

    /// Draw calls of the smackable objects at their poses.
    fn smack_draws(&self) -> Vec<PropDraw> {
        use glam::{Mat3, Mat4};
        let flip = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        self.smack_props
            .iter()
            .zip(&self.smack_world.bodies)
            .map(|(prop, body)| {
                let cols = |a: [[f32; 3]; 3]| {
                    Mat3::from_cols(Vec3::from(a[0]), Vec3::from(a[1]), Vec3::from(a[2]))
                };
                let home = cols(prop.record.axes);
                let pair = &body.car.pair;
                let now = cols([pair.axis_r_364, pair.axis_u_370, pair.axis_f_37c]);
                let model = if body.moved || pair.position_330 != prop.record.position {
                    let rot = Mat4::from_mat3(now * home.transpose());
                    flip * Mat4::from_translation(Vec3::from(pair.position_330))
                        * rot
                        * Mat4::from_translation(-Vec3::from(prop.record.position))
                        * flip
                } else {
                    Mat4::IDENTITY
                };
                PropDraw {
                    model,
                    mesh_range: prop.meshes.clone(),
                }
            })
            .collect()
    }

    /// Whether the original vehicle model drives the player car now (6 DOF driving, `O`
    /// switched on, and an original car built from the car's `.sim`).
    pub fn original_active(&self) -> bool {
        self.sim_mode
            && self.vehicle_model == nfs_assets::physics::original_vehicle::VehicleModel::Original
            && self.original_vehicle.is_some()
            && self.sim_car.is_some()
    }

    fn collision_box_or_fallback(&self) -> (Vec3, [f32; 3]) {
        self.car_collision_box
            .unwrap_or((Vec3::ZERO, FALLBACK_HALF_EXTENTS))
    }

    /// Ground points of the drawn car's wheels (wheel centre minus radius) relative to the
    /// collision box centre, in the original car axes (R, U, F) and the body point order of
    /// `0x49abc0`: FL, FR, RL, RR (picked by position, `original_wheel_index`). Without wheel
    /// meshes: points inside the box at its bottom.
    fn original_wheel_points(&self) -> [[f32; 3]; 4] {
        let (center, half) = self.collision_box_or_fallback();
        let (x, z) = (half[0] - 0.1, half[2] * 0.6);
        let mut points = [
            [-x, -half[1], z],
            [x, -half[1], z],
            [-x, -half[1], -z],
            [x, -half[1], -z],
        ];
        if let Some(render) = &self.car_render {
            for w in &render.wheels {
                let p = w.source_center - Vec3::Y * w.radius - center;
                points[original_wheel_index(p)] = [p.x, p.y, -p.z];
            }
        }
        points
    }

    /// The car model's body points (CRP `BndP`, `Car_ReadTpgCameraKeys` 0x441a10) relative
    /// to the collision box centre in the original car axes (R, U, F), with the same
    /// mapping as [`Self::original_wheel_points`]. Empty without a model.
    fn original_body_points(&self) -> Vec<[f32; 3]> {
        let (center, _) = self.collision_box_or_fallback();
        self.car_body_points
            .iter()
            .map(|&q| {
                let p = q - center;
                [p.x, p.y, -p.z]
            })
            .collect()
    }

    /// Rebuilds the original car from the loaded `.sim` and the drawn car (box, wheels).
    fn rebuild_original_vehicle(&mut self) {
        use nfs_assets::physics::original_vehicle::OriginalVehicle;
        let Some((bytes, model)) = &self.original_sim else {
            self.original_vehicle = None;
            return;
        };
        let (_, half) = self.collision_box_or_fallback();
        self.original_vehicle = OriginalVehicle::from_sim_with(
            bytes,
            model,
            half,
            self.original_wheel_points(),
            self.car_setup,
        )
        .ok();
        let body = self.original_body_points();
        if let Some(v) = &mut self.original_vehicle {
            v.set_body_points(&body);
        }
        self.sync_original_from_sim(false);
    }

    /// Chassis adapter offset of the 6 DOF body (see `car_chassis_adapter_body`).
    fn sim_adapter(&self) -> Vec3 {
        match (&self.sim_car, &self.car_render) {
            (Some(sim), Some(render)) => car_chassis_adapter_body(sim, render),
            _ => Vec3::ZERO,
        }
    }

    /// Puts the original car where the 6 DOF body is (box centre, axes, velocity, angular
    /// velocity; with `with_gear` also gear and rpm) and on the ground there.
    fn sync_original_from_sim(&mut self, with_gear: bool) {
        use nfs_assets::physics::original_vehicle::{FlatGround, RoadGround};
        let (center, _) = self.collision_box_or_fallback();
        let adapter = self.sim_adapter();
        let (Some(sim), Some(v)) = (self.sim_car.as_ref(), self.original_vehicle.as_mut()) else {
            return;
        };
        let (centre, axes, velocity, angular) =
            original_pose_from_body(&sim.body, center + adapter);
        let mut owners = Default::default();
        match self.road_surface.as_ref() {
            Some(surface) => v.set_pose(
                centre,
                axes,
                velocity,
                angular,
                &mut RoadGround {
                    surface,
                    owners: &mut owners,
                },
            ),
            None => v.set_pose(
                centre,
                axes,
                velocity,
                angular,
                &mut FlatGround {
                    height: 0.0,
                    surface: nfs_assets::physics::original_vehicle::ground::LEGACY_SURFACE,
                },
            ),
        }
        v.owners = owners;
        if with_gear {
            v.set_gear(
                (sim.powertrain.current_gear + 1).clamp(0, i8::MAX as i32) as i8,
                sim.powertrain.current_rpm,
            );
        }
    }

    /// Mirrors the original car into the 6 DOF body (pose, velocity, angular velocity, gear,
    /// rpm), so the code reading the body (camera fallback, triggers, course progress, pose
    /// telemetry) follows it and the prototype continues from there after a switch. HUD
    /// telemetry comes from the original car.
    fn sync_sim_from_original(&mut self, switching: bool) {
        use nfs_assets::physics::original_adapter::angular_from_original;
        let (center, _) = self.collision_box_or_fallback();
        let adapter = self.sim_adapter();
        let (Some(sim), Some(v)) = (self.sim_car.as_mut(), self.original_vehicle.as_ref()) else {
            return;
        };
        let car = &v.car;
        set_body_from_original(&mut sim.body, car, center + adapter);
        sim.powertrain.current_gear = v.hud_gear();
        sim.powertrain.current_rpm = car.rpm_db4.max(sim.powertrain.idle_rpm);
        if switching {
            sim.prev_linear_velocity = sim.body.linear_velocity;
            sim.sync_wheel_spin_to_body();
        }
        let mut tel = sim.telemetry(&nfs_assets::physics::VehicleControls::default());
        tel.speed_mps = car.vel_fwd_d60();
        tel.speed_kmh = Self::original_speed_kmh(car);
        tel.engine_rpm = car.rpm_db4;
        tel.current_gear = v.hud_gear();
        tel.yaw_rate_rad_s = angular_from_original(car.angular_388).y;
        self.sim_telemetry = Some(tel);
    }

    /// HUD speed of the original car: `+0x35c` (the original's speed field; the
    /// speedometer's source is not located).
    fn original_speed_kmh(car: &nfs_assets::physics::original_vehicle::OriginalCar) -> f32 {
        car.speed_35c * 3.6
    }

    /// Car setup screen: manual box (`.TRANSMISSION` 0) and the damage option (`Game.Damage`,
    /// `[0x657a2c]`, which also gates the zone damage of collisions). Applied to the original
    /// car at once; the manual box changes the derived drive factors, so the car is rebuilt.
    /// Difficulty, mirror and the opponents' driver skills of the next race (call before
    /// `configure_quick_race`). `skills` is 8 values per opponent.
    pub fn set_race_options(
        &mut self,
        difficulty: i32,
        mirror: bool,
        skills: &[f32],
        player_paint: &[f32],
        opponent_paints: &[f32],
    ) {
        let rgba = |c: &[f32]| -> [f32; 4] { c.try_into().unwrap() };
        self.race_options = RaceOptions {
            difficulty: difficulty.clamp(0, 2),
            mirror,
            opponent_skills: skills
                .chunks_exact(8)
                .map(|c| c.try_into().unwrap())
                .collect(),
            player_paint: (player_paint.len() == 4).then(|| rgba(player_paint)),
            opponent_paints: opponent_paints.chunks_exact(4).map(rgba).collect(),
        };
        self.rebuild_original_ai();
    }

    /// Whether the track record marks the track as point to point (LINEAR_TRACK).
    pub fn set_linear_track(&mut self, linear: bool) {
        self.linear_track = Some(linear);
    }

    /// Original start placement of grid value `grid` (the player has `racers`, the last
    /// opponent 1) on the node graph (`race::original_ai::start`); `None` without a node graph.
    pub fn start_slot(
        &self,
        grid: i32,
        racers: i32,
    ) -> Option<nfs_assets::race::original_ai::start::StartSlot> {
        use nfs_assets::race::original_ai::start;
        let track = self.original_ai_track.as_ref()?;
        let nodes = &track.nodes;
        if nodes.nodes.len() < 2 {
            return None;
        }
        let linear = self
            .linear_track
            .unwrap_or_else(|| !self.track_course.as_ref().is_some_and(|c| c.is_circuit));
        let node = match self.start_trigger {
            Some(t) => start::start_node(nodes, t),
            // `FUN_00410730`: no start trigger: node 1, 50 on a long point-to-point track.
            None if linear && nodes.lap_nodes_04 > 99 => 50,
            None => 1,
        };
        Some(start::slot(nodes, node, track.reverse, linear, racers, grid))
    }

    pub fn set_car_setup(&mut self, manual_box: bool, damage: bool) {
        use nfs_assets::physics::original_vehicle::CarSetup;
        let setup = CarSetup { manual_box, damage };
        let rebuild = setup.manual_box != self.car_setup.manual_box;
        self.car_setup = setup;
        self.pair_globals.damage_on_657a2c = damage;
        if rebuild && self.original_sim.is_some() {
            self.rebuild_original_vehicle();
        } else if let Some(v) = &mut self.original_vehicle {
            v.set_setup(setup);
        }
    }

    /// Gear event for the manual box: +1 SHIFTUP, -1 SHIFTDOWN (`Input_GearRequest 0x413890`).
    pub fn shift_gear(&mut self, direction: i32) {
        use nfs_assets::physics::original_vehicle::controls::{EVENT_SHIFT_DOWN, EVENT_SHIFT_UP};
        if self.car_setup.manual_box && direction != 0 {
            let code = if direction > 0 { EVENT_SHIFT_UP } else { EVENT_SHIFT_DOWN };
            self.pending_shift = (code, 2);
        }
    }

    /// Race (re)start of the original car at the 6 DOF body's spawn pose.
    fn restart_original_vehicle(&mut self) {
        self.drawn_states = None;
        if let Some(v) = &mut self.original_vehicle {
            v.restart();
            self.sync_original_from_sim(false);
        }
        self.rebuild_original_ai();
    }

    /// O key: switch the player car between the prototype and the original vehicle model.
    /// The car continues from its current pose and speed. Without an original car (no
    /// `.sim` loaded) the prototype stays.
    pub fn toggle_vehicle_model(&mut self) -> nfs_assets::physics::original_vehicle::VehicleModel {
        use nfs_assets::physics::original_vehicle::VehicleModel;
        match self.vehicle_model {
            VehicleModel::Prototype if self.original_vehicle.is_some() => {
                self.drawn_states = None;
                self.sync_original_from_sim(true);
                let sched = self.original_race_sched();
                if let Some(v) = &mut self.original_vehicle {
                    v.set_race_clock(sched);
                }
                self.vehicle_model = VehicleModel::Original;
                self.rebuild_original_ai();
            }
            VehicleModel::Prototype => {}
            VehicleModel::Original => {
                self.sync_sim_from_original(true);
                self.sim_telemetry = None;
                self.vehicle_model = VehicleModel::Prototype;
                self.rebuild_original_ai();
            }
        }
        self.vehicle_model
    }

    /// One frame of the original model: the 64 Hz scheduler steps due after `dt`, with the
    /// ground adapter, the original collision pass (player barrier and AI pairs) in the 32 Hz
    /// queue and the player camera on every step. Signed `throttle`: gas forward, brake
    /// backward (the automatic box reverses when stopped).
    fn run_original_vehicle(&mut self, dt: f32, throttle: f32, steer: f32, handbrake: bool) {
        use nfs_assets::physics::original_lcg::OriginalLcg;
        use nfs_assets::physics::original_vehicle::{
            OriginalCar, OriginalVehicle, RoadGround, StepQueue,
            driver::camera_car,
            input_word, scheduler_step,
        };
        use nfs_assets::race::original_collision::{apply_pair_car_to_ai, pair_car_from_ai_original};
        let (center, half) = self.collision_box_or_fallback();
        let mut word = input_word(throttle.max(0.0), (-throttle).max(0.0), steer, handbrake);
        if self.pending_shift.1 > 0 {
            word |= u32::from(self.pending_shift.0) << 3;
            self.pending_shift.1 -= 1;
        }
        if self.original_ai.is_some() {
            // Player and original AI cars in the original order (original_ai_view).
            self.run_original_field(dt, word);
            self.sync_sim_from_original(false);
            return;
        }
        let drawn = self.drawn_states.take();
        let Some(v) = self.original_vehicle.as_mut() else {
            self.drawn_states = drawn;
            return;
        };
        let OriginalVehicle {
            car,
            lcg,
            ground,
            owners,
            locator,
            clock,
            half_extents,
            ..
        } = v;

        struct Queue<'a> {
            surface: Option<&'a nfs_assets::RoadSurface>,
            ground: &'a mut nfs_assets::physics::original_vehicle::GroundAdapter,
            owners: &'a mut [nfs_assets::physics::original_support_owner::OriginalSupportOwner; 4],
            /// `car+8`: track locator and body support owner (also the barrier pass).
            locator: &'a mut nfs_assets::physics::original_vehicle::BodyLocator,
            nodes: Option<&'a nfs_assets::race::original_ai::track::TrackNodes>,
            pair_globals: &'a mut nfs_assets::physics::original_pair::PairGlobals,
            ai: &'a mut [nfs_assets::AiOpponent],
            ai_box: (Vec3, [f32; 3]),
            half: [f32; 3],
            camera: &'a mut nfs_assets::camera::PlayerCamera,
            /// Barrier contacts of the player per collide tick (sparks).
            contacts: &'a mut Vec<SparkContact>,
            /// Track objects and the smashes that start (`Car_CollideTrackObjects`).
            world: Option<&'a mut nfs_assets::physics::original_track_pass::TrackObjectWorld>,
            smack: &'a mut nfs_assets::physics::original_smackable::SmackWorld,
            events: &'a mut Vec<nfs_assets::physics::original_track_pass::SmashEvent>,
        }
        impl StepQueue for Queue<'_> {
            fn track_locate(&mut self, car: &mut OriginalCar) {
                if let Some(nodes) = self.nodes {
                    let me = car.position_330;
                    nfs_assets::physics::original_vehicle::track_locate(
                        car,
                        self.locator,
                        nodes,
                        self.surface.and_then(|s| s.original_arena()),
                        &[me],
                    );
                }
            }
            fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
                match self.surface {
                    Some(surface) => self.ground.ground_step(
                        car,
                        &mut RoadGround {
                            surface,
                            owners: self.owners,
                        },
                        lcg,
                    ),
                    None => self.ground.ground_step(
                        car,
                        &mut nfs_assets::physics::original_vehicle::FlatGround {
                            height: 0.0,
                            surface: nfs_assets::physics::original_vehicle::ground::LEGACY_SURFACE,
                        },
                        lcg,
                    ),
                }
            }
            fn collide(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
                // AI cars are kinematic (approximation A6): no barrier pass, the player's box.
                let mut ai: Vec<_> = self
                    .ai
                    .iter()
                    .map(|a| pair_car_from_ai_original(a, self.ai_box.0, self.ai_box.1))
                    .collect();
                // Awake smackable objects are cars in the pair test.
                let n_ai = ai.len();
                let awake = self.smack.awake();
                ai.extend(awake.iter().map(|&i| self.smack.bodies[i].car.pair.clone()));
                let before = ai.clone();
                let barrier = self.surface.filter(|s| s.uses_original_support());
                let (half, owner) = (self.half, &mut self.locator.owner);
                let contacts = &mut *self.contacts;
                let objects = self.world.as_deref_mut().map(|w| (w, &mut *self.events));
                nfs_assets::physics::original_vehicle::driver::collide_player_objects(car, half, &mut ai, self.pair_globals, lcg, objects, |state| {
                    let s = barrier?;
                    match s.original_barrier_contact(state, half, owner) {
                        Ok(Some(hit)) => {
                            contacts.push(SparkContact {
                                impact: hit.magnitude,
                                point: hit.point,
                                velocity: state.velocity_33c,
                            });
                            Some(hit.into())
                        }
                        _ => None,
                    }
                });
                for (a, (b, c)) in self.ai.iter_mut().zip(before.iter().zip(&ai)) {
                    apply_pair_car_to_ai(a, b, c);
                }
                for (k, &i) in awake.iter().enumerate() {
                    let body = &mut self.smack.bodies[i];
                    body.car.pair = ai[n_ai + k].clone();
                    body.update_world_points();
                }
            }
            fn camera(&mut self, car: &OriginalCar) {
                self.camera.step(
                    &camera_car(car, self.half),
                    &mut nfs_assets::camera::FlatWorld,
                );
            }
        }

        let mut queue = Queue {
            surface: self.road_surface.as_ref(),
            ground,
            owners,
            locator,
            nodes: self.original_ai_track.as_ref().map(|t| &t.nodes),
            pair_globals: &mut self.pair_globals,
            ai: &mut self.ai_opponents,
            ai_box: (center, half),
            half: *half_extents,
            camera: &mut self.player_camera,
            contacts: &mut self.spark_contacts,
            world: self.track_world.as_mut(),
            smack: &mut self.smack_world,
            events: &mut self.smash_events,
        };
        // Starts the camera (end of the race intro) on the first call; no steps.
        queue.camera.advance(
            0.0,
            &camera_car(car, *half_extents),
            &mut nfs_assets::camera::FlatWorld,
        );
        let sample = |car: &OriginalCar, camera: &nfs_assets::camera::PlayerCamera| {
            (car_pose(car), camera.peek_view(car.position_330))
        };
        let mut states = drawn.unwrap_or_else(|| {
            let (pose, view) = sample(car, queue.camera);
            nfs_assets::render_interp::DrawnStates::new(pose, view)
        });
        for _ in 0..clock.advance(dt) {
            scheduler_step(car, word, lcg, &mut queue);
            let (pose, view) = sample(car, queue.camera);
            states.push(pose, view);
        }
        states.alpha = clock.fraction();
        self.drawn_states = Some(states);
        self.sync_sim_from_original(false);
    }

    /// Cycle the debug AI speed cap: none -> 30 km/h -> parked -> none.
    /// Returns the cap in km/h, or a negative value for none.
    pub fn cycle_ai_speed_limit(&mut self) -> f32 {
        self.ai_speed_limit = match self.ai_speed_limit {
            None => Some(30.0 / 3.6),
            Some(limit) if limit > 0.0 => Some(0.0),
            Some(_) => None,
        };
        for ai in &mut self.ai_opponents {
            ai.speed_limit = self.ai_speed_limit;
        }
        self.ai_speed_limit.map_or(-1.0, |limit| limit * 3.6)
    }

    pub fn toggle_collision_model(
        &mut self,
    ) -> nfs_assets::race::original_collision::CollisionModel {
        use nfs_assets::race::original_collision::CollisionModel;
        self.collision_model = match self.collision_model {
            CollisionModel::Prototype => CollisionModel::Original,
            CollisionModel::Original => CollisionModel::Prototype,
        };
        self.collision_clock = Default::default();
        self.collision_model
    }

    /// Debug panel: the original vehicle model is selected.
    pub fn vehicle_model_is_original(&self) -> bool {
        self.vehicle_model == nfs_assets::physics::original_vehicle::VehicleModel::Original
    }

    /// Debug panel: the original collision response is selected.
    pub fn collision_model_is_original(&self) -> bool {
        self.collision_model == nfs_assets::race::original_collision::CollisionModel::Original
    }

    /// Debug panel: AI speed cap in km/h, negative for none.
    pub fn ai_speed_limit_kmh(&self) -> f32 {
        self.ai_speed_limit.map_or(-1.0, |limit| limit * 3.6)
    }

    /// Debug panel: the light switch.
    pub fn lights_on(&self) -> bool {
        self.light_switch
    }

    /// Debug panel: the free camera is on.
    pub fn is_free_camera(&self) -> bool {
        self.free_camera
    }

    /// Debug panel: side view angle in degrees (0 = off).
    pub fn side_view_degrees(&self) -> f32 {
        self.side_view_deg
    }

    pub fn toggle_sim_mode(&mut self) -> bool {
        self.sim_mode = !self.sim_mode;
        self.sim_mode
    }

    pub fn is_sim_mode(&self) -> bool {
        self.sim_mode
    }

    pub fn has_car(&self) -> bool {
        self.car.is_some()
    }

    /// Replace the active physics profile with a parsed original .sim file.
    /// Parse and validate first so malformed input cannot discard the current car.
    /// `sim_name` (the `.sim` file stem) selects the model name `res+0x2a8` of the original
    /// model's SIM derivation (`model_for_sim`); without it the defaults apply.
    pub fn set_car_sim(&mut self, bytes: &[u8], sim_name: Option<&str>) -> Result<(), String> {
        let car = self.car.as_ref().ok_or("No car on the current track")?;
        let mut sim = simulation_from_bytes_at_pose(bytes, car.pos, car.yaw)?;
        self.apply_car_render_wheels_to_sim(&mut sim)?;
        self.sim_car = Some(sim);
        self.sim_telemetry = None;
        let stem = sim_name
            .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n))
            .map(|n| {
                n.strip_suffix(".sim")
                    .or(n.strip_suffix(".SIM"))
                    .unwrap_or(n)
            })
            .unwrap_or("default");
        let model = nfs_assets::physics::original_vehicle::driver::model_for_sim(stem);
        self.original_sim = Some((bytes.to_vec(), model.to_string()));
        self.rebuild_original_vehicle();
        // The original model is the default whenever a `.sim` is loaded (O switches to the
        // prototype for comparison).
        if self.original_vehicle.is_some() {
            self.vehicle_model = nfs_assets::physics::original_vehicle::VehicleModel::Prototype;
            self.toggle_vehicle_model();
        }
        self.ai_performance = nfs_formats::parse_sim(bytes)
            .ok()
            .and_then(|spec| nfs_assets::AiPerformance::from_sim(&spec));
        for ai in &mut self.ai_opponents {
            ai.performance = self.ai_performance;
        }
        Ok(())
    }

    fn apply_car_render_wheels_to_sim(
        &self,
        sim: &mut nfs_assets::physics::VehicleSimulation,
    ) -> Result<(), String> {
        let Some(car_render) = &self.car_render else {
            return Ok(());
        };
        if car_render.wheels.is_empty() {
            return Ok(());
        }
        let wheels: Vec<nfs_assets::CarWheel> = car_render
            .wheels
            .iter()
            .map(|wheel| nfs_assets::CarWheel {
                wheel_index: wheel.wheel_index,
                center: wheel.source_center.to_array(),
                radius: wheel.radius,
                mesh_indices: wheel.mesh_indices.clone(),
            })
            .collect();
        sim.apply_model_wheels(&wheels)?;
        if let Some(bounds) = car_render.body_bounds {
            let offset = car_chassis_adapter_body(sim, car_render);
            sim.apply_model_chassis_bounds(bounds.map(|p| (p + offset).to_array()))?;
        }
        Ok(())
    }

    /// Load genuine Porsche car model geometry, textures, and simulation profile onto the track.
    /// The loaded car has a moving part of this channel (1 top, 8 hood, 9 trunk lid).
    pub fn car_render_has_part(&self, channel: u8) -> Option<bool> {
        self.car_render
            .as_ref()
            .map(|c| c.anim_parts.iter().any(|p| p.channel == channel))
    }

    /// Moves the car loaded last (`set_car_model`) to the compare screen's second spot.
    pub fn stash_compare_car(&mut self) {
        self.fe_compare2 = self.car_render.take().map(|render| CompareCar {
            render,
            collision_box: self.car_collision_box,
        });
    }

    pub fn set_car_model(&mut self, scene: &Scene, car_name: &str) -> Result<(), String> {
        // The car loader divides raw geometry by five; driving rendering below
        // restores it. This adapter uses that same visible body's dimensions.
        let extents = (Vec3::from(scene.bounds[1]) - Vec3::from(scene.bounds[0])) * 2.5;
        self.trigger_half_extents =
            (extents.is_finite() && extents.min_element() > 0.0).then_some(extents);
        if scene.meshes.is_empty() {
            return Err("Car scene has no geometry".into());
        }

        let sampler = self.device.create_sampler(&mipmapped_sampler("car texture sampler"));

        let material_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("car material layout"),
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let white = upload_texture(&self.device, &self.queue, 1, 1, &[255; 4]);
        let mut texture_views = Vec::new();
        for texture in &scene.textures {
            texture_views.push(upload_texture_mipmapped(
                &self.device,
                &self.queue,
                texture.width,
                texture.height,
                &texture.rgba,
            ));
        }

        let mut car_materials = Vec::new();
        for (i, material) in scene.materials.iter().enumerate() {
            let view = match material.texture {
                Some(idx) => texture_views.get(idx).unwrap_or(&white),
                None => &white,
            };
            let material_uniform = [
                material.base_color[0],
                material.base_color[1],
                material.base_color[2],
                material.base_color[3],
                alpha_mode_code(material.alpha_mode),
                material.alpha_cutoff,
                if material.paintable { 1.0 } else { 0.0 },
                // Light slot 1 for the interior (approximation A23: by render method).
                // 3 = exterior with the env/specular layers (render method `CarExtEnv`).
                if material.name.ends_with(":CarInt") {
                    2.0
                } else if material.name.ends_with(":CarExtEnv") {
                    3.0
                } else {
                    0.0
                },
                0.0,
                0.0,
                0.0,
                0.0,
            ];
            let color = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("car mat {i}")),
                    contents: bytemuck::cast_slice(&material_uniform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("car mat group {i}")),
                layout: &material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: color.as_entire_binding(),
                    },
                ],
            });
            car_materials.push(GpuMaterial {
                group,
                color_buffer: color,
                uv_scroll: material.uv_scroll,
                paintable: material.paintable,
                tail: material_uniform[7],
                alpha_mode: material.alpha_mode,
                alpha_cutoff: material.alpha_cutoff,
                double_sided: material.double_sided,
                depth_bias: material.depth_bias,
            });
        }

        let mut car_meshes = Vec::new();
        let mut mesh_index_map = vec![None; scene.meshes.len()];
        let wheel_source_meshes: BTreeSet<usize> = scene
            .car_wheels
            .iter()
            .flat_map(|w| w.mesh_indices.iter().copied())
            .collect();
        let mut body_min = Vec3::splat(f32::INFINITY);
        let mut body_max = Vec3::splat(f32::NEG_INFINITY);
        for (scene_mesh_index, mesh) in scene.meshes.iter().enumerate() {
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            const CAR_SCALE: f32 = 5.0;
            let vertices: Vec<GpuVertex> = mesh
                .vertices
                .iter()
                .map(|v| GpuVertex {
                    position: [
                        -v.position[0] * CAR_SCALE,
                        v.position[1] * CAR_SCALE,
                        -v.position[2] * CAR_SCALE,
                    ],
                    normal: [-v.normal[0], v.normal[1], -v.normal[2]],
                    uv: v.uv,
                    color: v.color,
                })
                .collect();
            let material_name = scene
                .materials
                .get(mesh.material)
                .map(|m| m.name.as_str())
                .unwrap_or("");
            if !wheel_source_meshes.contains(&scene_mesh_index)
                && !mesh.name.to_ascii_lowercase().contains("shadow")
                && (material_name.contains("CarExt") || material_name.contains("CarInt"))
            {
                for vertex in &vertices {
                    let p = Vec3::from(vertex.position);
                    body_min = body_min.min(p);
                    body_max = body_max.max(p);
                }
            }
            let center = mesh
                .vertices
                .iter()
                .map(|v| {
                    Vec3::new(
                        -v.position[0] * CAR_SCALE,
                        v.position[1] * CAR_SCALE,
                        -v.position[2] * CAR_SCALE,
                    )
                })
                .sum::<Vec3>()
                / mesh.vertices.len() as f32;
            mesh_index_map[scene_mesh_index] = Some(car_meshes.len());
            car_meshes.push(GpuMesh {
                vertex: self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&mesh.name),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                index: self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&mesh.name),
                        contents: bytemuck::cast_slice(&mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                count: mesh.indices.len() as u32,
                material: mesh.material,
                center,
            });
        }

        let wheels: Vec<CarWheelRender> = scene
            .car_wheels
            .iter()
            .filter_map(|wheel| {
                let mesh_indices: Vec<usize> = wheel
                    .mesh_indices
                    .iter()
                    .filter_map(|&mesh_index| mesh_index_map.get(mesh_index).and_then(|v| *v))
                    .collect();
                (!mesh_indices.is_empty()).then(|| CarWheelRender {
                    wheel_index: wheel.wheel_index,
                    source_center: Vec3::from(wheel.center),
                    radius: wheel.radius,
                    mesh_indices,
                })
            })
            .collect();
        let wheel_mesh_indices: BTreeSet<usize> = wheels
            .iter()
            .flat_map(|wheel| wheel.mesh_indices.iter().copied())
            .collect();
        let chassis_mesh_indices: Vec<usize> = (0..car_meshes.len())
            .filter(|mesh_index| !wheel_mesh_indices.contains(mesh_index))
            .collect();

        let anim_parts: Vec<CarAnimRender> = scene
            .car_anim_parts
            .iter()
            .map(|part| CarAnimRender {
                channel: part.channel,
                axis: part.axis,
                pivot: Vec3::from(part.pivot),
                open_sign: part.open_sign,
                extent: part.extent,
                base: part
                    .base_meshes
                    .iter()
                    .filter_map(|&i| mesh_index_map.get(i).and_then(|v| *v))
                    .collect(),
                morph: (!part.frames.is_empty()).then(|| {
                    let base: Vec<Vec<GpuVertex>> = part
                        .base_meshes
                        .iter()
                        .map(|&i| body_vertices(&scene.meshes[i]))
                        .collect();
                    let mut frames = vec![base];
                    frames.extend(
                        part.frames
                            .iter()
                            .map(|set| set.iter().map(body_vertices).collect::<Vec<_>>()),
                    );
                    CarMorph {
                        dynamic: part
                            .base_meshes
                            .iter()
                            .map(|&i| dynamic_car_mesh(&self.device, &scene.meshes[i]))
                            .collect(),
                        frames,
                    }
                }),
            })
            .collect();
        let roof_up: Vec<usize> = scene
            .car_roof_up
            .iter()
            .filter_map(|&i| mesh_index_map.get(i).and_then(|v| *v))
            .collect();
        let roof_down: Vec<GpuMesh> = scene
            .car_roof_down
            .iter()
            .map(|m| dynamic_car_mesh(&self.device, m))
            .collect();
        // With a closed top at rest the morph's frame 0 would be a second copy of it.
        let hidden: BTreeSet<usize> = if roof_up.is_empty() {
            BTreeSet::new()
        } else {
            anim_parts
                .iter()
                .filter(|p| p.channel == 1)
                .flat_map(|p| p.base.iter().copied())
                .collect()
        };
        let chassis_mesh_indices: Vec<usize> = chassis_mesh_indices
            .into_iter()
            .filter(|i| !hidden.contains(i))
            .collect();
        // Race meshes map raw CRP (x, y, z) to body (-x, y, z), see CAR_SCALE above.
        self.player_camera = nfs_assets::camera::PlayerCamera::new(
            scene.car_driver_camera.unwrap_or_default(),
            self.player_camera.cycle_index,
        );
        self.car_body_points = scene
            .car_body_points
            .iter()
            .map(|p| Vec3::new(-p[0], p[1], p[2]))
            .collect();
        self.car_collision_box = scene.car_collision_box.map(|b| {
            (
                Vec3::new(-b.center[0], b.center[1], b.center[2]),
                b.half_extents,
            )
        });
        // Pipelines for the car's (blend, cull, z bias) combinations; without them the
        // draw fell back to bias 0 and the decals z-fought with the body.
        for m in &car_materials {
            let key = (m.alpha_mode == AlphaMode::Blend, m.double_sided, m.depth_bias);
            if !self.pipelines.contains_key(&key) {
                let pipeline = self.pipeline_factory.create(&self.device, key.0, key.1, key.2);
                self.pipelines.insert(key, pipeline);
            }
        }
        self.car_render = Some(CarRenderState {
            meshes: car_meshes,
            materials: car_materials,
            chassis_mesh_indices,
            wheels,
            body_bounds: (body_min.is_finite()
                && body_max.is_finite()
                && (body_max - body_min).min_element() > 0.0)
                .then_some([body_min, body_max]),
            in_car: crate::in_car::build(&self.device, &scene.car_in_car),
            anim_parts,
            roof_up,
            roof_down,
            hidden,
        });

        self.car_glares = scene.car_glares.clone();
        // Specular map slot 1 (SPC2) of `Car_Load` 0x441a10; approximation A71.
        if let Some(t) = scene
            .effect_textures
            .iter()
            .find(|t| t.name == SPECULAR_MAP)
            .filter(|t| t.rgba.len() as u64 == u64::from(t.width) * u64::from(t.height) * 4)
        {
            let view = upload_texture(&self.device, &self.queue, t.width, t.height, &t.rgba);
            self.camera_group = camera_bind_group(
                &self.device,
                &self.camera_layout,
                &self.camera_buffer,
                &view,
                &self.specular_sampler,
            );
        }
        self.spark_emitter = scene
            .particle_defs
            .iter()
            .find(|d| &d.tag == b"CSPK")
            .map(|d| nfs_assets::sparks::SparkEmitter::new(*d));
        self.effects
            .set_textures(&self.device, &self.queue, &scene.effect_textures);

        self.configure_sim_for_model(car_name);
        if let Some(mut sim) = self.sim_car.take() {
            self.apply_car_render_wheels_to_sim(&mut sim)?;
            self.sim_car = Some(sim);
            self.sim_telemetry = None;
        }
        // A new car: its `.sim` follows (`set_car_sim`).
        self.original_sim = None;
        self.original_vehicle = None;
        Ok(())
    }

    /// Configure realistic vehicle simulation parameters for specified model.
    pub fn configure_sim_for_model(&mut self, model_name: &str) {
        let name_lower = model_name.to_lowercase();
        let (name, mass, gears, ratios, final_drive, redline, torque_max, cg_offset) =
            if name_lower.contains("356") {
                (
                    "1950 Porsche 356 Ferdinand",
                    810.0,
                    4,
                    vec![3.18, 1.76, 1.13, 0.81],
                    4.375,
                    4500.0,
                    80.0,
                    [0.0, 0.28, -0.2],
                )
            } else if name_lower.contains("911")
                || name_lower.contains("901")
                || name_lower.contains("rs")
            {
                (
                    "1973 Porsche 911 Carrera RS 2.7",
                    960.0,
                    5,
                    vec![3.18, 1.83, 1.26, 0.96, 0.76],
                    3.875,
                    7300.0,
                    255.0,
                    [0.0, 0.32, -0.3],
                )
            } else if name_lower.contains("930") || name_lower.contains("turbo") {
                (
                    "1978 Porsche 911 Turbo 3.3",
                    1300.0,
                    4,
                    vec![2.25, 1.30, 0.89, 0.65],
                    4.22,
                    6800.0,
                    412.0,
                    [0.0, 0.34, -0.35],
                )
            } else if name_lower.contains("996") || name_lower.contains("gt3") {
                (
                    "1999 Porsche 911 GT3 (996)",
                    1350.0,
                    6,
                    vec![3.82, 2.15, 1.56, 1.21, 0.97, 0.83],
                    3.44,
                    7900.0,
                    370.0,
                    [0.0, 0.33, -0.25],
                )
            } else if name_lower.contains("993") {
                (
                    "1995 Porsche 911 Carrera (993)",
                    1370.0,
                    6,
                    vec![3.82, 2.05, 1.41, 1.12, 0.92, 0.77],
                    3.44,
                    6800.0,
                    330.0,
                    [0.0, 0.33, -0.28],
                )
            } else {
                (
                    "1997 Porsche Boxster 2.5L",
                    1252.0,
                    5,
                    vec![3.50, 2.12, 1.43, 1.03, 0.79],
                    3.89,
                    6700.0,
                    245.0,
                    [0.0, 0.35, -0.1],
                )
            };

        let mut torque_curve = [0.0f32; 21];
        for (i, val) in torque_curve.iter_mut().enumerate().take(14) {
            let t = i as f32 / 13.0;
            *val = torque_max * (0.6 + 0.4 * (1.0 - (t - 0.7).powi(2) * 2.0).clamp(0.0, 1.0));
        }

        let sim_spec = nfs_formats::sim::SimCar {
            name: name.to_string(),
            mass_kg: mass,
            wheelbase_m: 2.40,
            gear_count: gears,
            drive_flags: 2,
            reverse_gear: -3.5,
            forward_gears: ratios,
            final_drive,
            redline_rpm: redline,
            idle_or_step_rpm: 800.0,
            torque_curve,
            brake_bias: 0.62,
            drag_coeff: 0.31,
            swaybar_stiffness: 14000.0,
            suspension_stiffness: 30.0,
            front_track_m: 1.45,
            rear_track_m: 1.48,
            damping_compression: 2.5,
            damping_rebound: 3.2,
            tire_grip: 0.95,
            cg_offset_m: cg_offset,
            raw: [0u8; 328],
        };

        let spawn_pos = self.car.as_ref().map(|c| c.pos).unwrap_or(glam::Vec3::ZERO);
        let spawn_yaw = self.car.as_ref().map(|c| c.yaw).unwrap_or(0.0);
        let mut sim = nfs_assets::physics::VehicleSimulation::from_sim(&sim_spec);
        sim.reset(spawn_pos + Vec3::Y * sim.resting_height(), spawn_yaw);
        self.sim_car = Some(sim);
    }
}

fn simulation_from_bytes_at_pose(
    bytes: &[u8],
    road_position: Vec3,
    yaw: f32,
) -> Result<nfs_assets::physics::VehicleSimulation, String> {
    let spec = nfs_formats::parse_sim(bytes).map_err(|e| format!("Invalid car SIM: {e}"))?;
    // The format parser checks structure and length. Guard the fields used by
    // physics as well: a correctly sized damaged file can contain NaN or zeroes.
    if !spec.mass_kg.is_finite()
        || spec.mass_kg <= 0.0
        || !spec.wheelbase_m.is_finite()
        || spec.wheelbase_m <= 0.0
        || !(1..=6).contains(&spec.gear_count)
        || spec
            .forward_gears
            .iter()
            .any(|ratio| !ratio.is_finite() || *ratio <= 0.0)
        || !spec.reverse_gear.is_finite()
        || !spec.final_drive.is_finite()
        || spec.final_drive <= 0.0
        || !spec.redline_rpm.is_finite()
        || spec.redline_rpm <= 0.0
        || !spec.idle_or_step_rpm.is_finite()
        || !spec.torque_curve.iter().all(|torque| torque.is_finite())
        || !spec.brake_bias.is_finite()
        || !spec.drag_coeff.is_finite()
        || !spec.swaybar_stiffness.is_finite()
        || !spec.suspension_stiffness.is_finite()
        || !spec.front_track_m.is_finite()
        || spec.front_track_m <= 0.0
        || !spec.rear_track_m.is_finite()
        || spec.rear_track_m <= 0.0
        || !spec.damping_compression.is_finite()
        || !spec.damping_rebound.is_finite()
        || !spec.tire_grip.is_finite()
        || !spec.cg_offset_m.iter().all(|offset| offset.is_finite())
    {
        return Err("Invalid car SIM: nonphysical or nonfinite parameters".into());
    }
    let mut sim = nfs_assets::physics::VehicleSimulation::from_sim(&spec);
    sim.reset(road_position + Vec3::Y * sim.resting_height(), yaw);
    Ok(sim)
}

pub fn sample_road_elevation_from_edges(
    road_edges: &[nfs_assets::TopologyEdge],
    x: f32,
    z: f32,
) -> Option<f32> {
    if road_edges.is_empty() {
        return None;
    }
    let mut best_dist_sq = 1e9_f32;
    let mut best_y = 0.0_f32;

    for edge in road_edges {
        let x1 = edge.p1[0];
        let z1 = edge.p1[2];
        let x2 = edge.p2[0];
        let z2 = edge.p2[2];

        if (x - x1).abs() > 45.0 && (x - x2).abs() > 45.0 {
            continue;
        }
        if (z - z1).abs() > 45.0 && (z - z2).abs() > 45.0 {
            continue;
        }

        let dx = x2 - x1;
        let dz = z2 - z1;
        let seg_len_sq = dx * dx + dz * dz;
        let t = if seg_len_sq < 1e-6 {
            0.0
        } else {
            (((x - x1) * dx + (z - z1) * dz) / seg_len_sq).clamp(0.0, 1.0)
        };

        let proj_x = x1 + t * dx;
        let proj_z = z1 + t * dz;
        let dist_sq = (x - proj_x) * (x - proj_x) + (z - proj_z) * (z - proj_z);
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            let y1 = edge.p1[1];
            let y2 = edge.p2[1];
            best_y = y1 + t * (y2 - y1);
        }
    }

    if best_dist_sq < 625.0 {
        return Some(best_y);
    }

    if best_dist_sq > 1e8 {
        for edge in road_edges {
            let x1 = edge.p1[0];
            let z1 = edge.p1[2];
            let x2 = edge.p2[0];
            let z2 = edge.p2[2];

            let dx = x2 - x1;
            let dz = z2 - z1;
            let seg_len_sq = dx * dx + dz * dz;
            let t = if seg_len_sq < 1e-6 {
                0.0
            } else {
                (((x - x1) * dx + (z - z1) * dz) / seg_len_sq).clamp(0.0, 1.0)
            };

            let proj_x = x1 + t * dx;
            let proj_z = z1 + t * dz;
            let dist_sq = (x - proj_x) * (x - proj_x) + (z - proj_z) * (z - proj_z);
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                let y1 = edge.p1[1];
                let y2 = edge.p2[1];
                best_y = y1 + t * (y2 - y1);
            }
        }
    }

    if best_dist_sq < 6400.0 {
        Some(best_y)
    } else {
        None
    }
}

/// Wall clock for the texture animations (the original uses its 128 Hz game timer, which does
/// not stop in menus either).
fn animation_seconds() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

fn alpha_mode_code(mode: AlphaMode) -> f32 {
    match mode {
        AlphaMode::Opaque => 0.0,
        AlphaMode::Mask => 1.0,
        AlphaMode::Blend => 2.0,
    }
}

/// A barrier contact of one collide pass: strength (`+0x430`), point (`+0x440`)
/// and car velocity (`+0x33c`), original space.
#[derive(Clone, Copy, Debug)]
struct SparkContact {
    impact: f32,
    point: [f32; 3],
    velocity: [f32; 3],
}

/// Shader, layout and target format of the mesh pipelines (`car.wgsl`).
struct MeshPipelineFactory {
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
}

impl MeshPipelineFactory {
    fn create(
        &self,
        device: &wgpu::Device,
        blend: bool,
        double_sided: bool,
        depth_bias: i32,
    ) -> wgpu::RenderPipeline {
        let (shader, layout, format) = (&self.shader, &self.layout, self.format);
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("car"), layout: Some(layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<GpuVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4] })] },
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: if blend { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None }, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { cull_mode: if double_sided { None } else { Some(wgpu::Face::Back) }, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(!blend), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: depth_bias_state(depth_bias) }),
            multisample: Default::default(), multiview_mask: None, cache: None,
        })
    }
}

/// Per-material z bias `mt+0x114` as a GPU depth bias. Approximation A72: one unit of the
/// original bias is taken as one LSB of a 16-bit z buffer (D3D7 `ZBIAS`), i.e. 256 ulp of
/// the 24-bit float mantissa near the far end of the depth range; negative = closer.
fn depth_bias_state(bias: i32) -> wgpu::DepthBiasState {
    wgpu::DepthBiasState {
        constant: bias.saturating_mul(Z_BIAS_UNIT),
        ..Default::default()
    }
}
const Z_BIAS_UNIT: i32 = 256;

/// Car specular sphere map: `carcmn.fsh` SPC2, slot 1 of 0x441a10 (A71).
const SPECULAR_MAP: &str = "SPC2";

fn camera_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    specular: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(specular),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Brake pedal 0..1 as the original's brake byte (`car+0xd80`, 0..255).
fn brake_byte(pedal: f32) -> u8 {
    (pedal.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Repeat sampler with trilinear and anisotropic filtering for mipmapped textures.
/// Approximation A74: the original's `MIPMAPPING` option filters minified textures
/// (driver filter modes not read); without mips distant/grazing faces shimmer and smear.
fn mipmapped_sampler(label: &str) -> wgpu::SamplerDescriptor<'_> {
    wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    }
}

/// Box-filtered mip chain (RGBA8, sRGB bytes averaged).
fn mip_chain(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(width, height, rgba.to_vec())];
    while let Some(&(w, h, ref src)) = levels.last() {
        if w == 1 && h == 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        sum += u32::from(src[((sy * w + sx) * 4 + c) as usize]);
                    }
                    dst[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        levels.push((nw, nh, dst));
    }
    levels
}

pub(crate) fn upload_texture_mipmapped(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let levels = mip_chain(width, height, rgba);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mipmapped texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (w, h, data)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(*h),
            },
            wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
        );
    }
    texture.create_view(&Default::default())
}

pub(crate) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("car texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    texture.create_view(&Default::default())
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

// --- Sky dome ---

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    view_projection: [[f32; 4]; 4],
    /// rotation (turns), base height, band height, tile rows.
    horizon: [f32; 4],
    /// tile columns, first `hzcl` column, 0, 0.
    grid: [f32; 4],
    /// per ring segment: tile index, mirrored.
    tiles: [[f32; 4]; 16],
    /// `hzcl` colours of the ring vertices, upper and lower edge, indexed by segment.
    top_colors: [[f32; 4]; 16],
    bottom_colors: [[f32; 4]; 16],
    /// Sky-map dome (`FUN_00460f40`): angle (turns), size, curvature, height.
    dome: [f32; 4],
    /// Dome scale, dome present (1/0), layer 0 `1/x scale`, `1/y scale` (0 = no layer).
    dome2: [f32; 4],
    /// Layer 1 `1/x scale`, `1/y scale` (0 = no layer), 0, 0.
    dome3: [f32; 4],
    /// Texture offsets of the two layers (u0, v0, u1, v1), wind drift (A194).
    drift: [f32; 4],
}

/// Wind drift of the sky-map layers in UV per second: `FUN_0045f1b0` stores
/// `(sin, cos)(-direction / 360 turns) * speed * 0.00025`; applying it once per 128 Hz tick is
/// a Hypothesis (A194).
fn sky_drift_velocity(map: Option<&nfs_formats::SkyMapBlock>) -> [f32; 4] {
    let mut v = [0.0; 4];
    if let Some(m) = map {
        for k in 0..2 {
            let turns = -m.wind_direction[k] / 360.0 * std::f32::consts::TAU;
            let step = m.wind_speed[k] * 0.00025 * 128.0;
            v[2 * k] = turns.sin() * step;
            v[2 * k + 1] = turns.cos() * step;
        }
    }
    v
}

impl SkyUniform {
    /// Horizon ring of `Sky/<track>.bin` (`SkyHorizon_Build` 0x459d30); the vertex colours are
    /// the 16x2 `hzcl` strip, column `(start + k) & 15` for ring vertex `k`.
    fn new(
        view_projection: Mat4,
        horizon: &nfs_formats::SkyHorizon,
        hzcl: Option<&nfs_assets::SkyTexture>,
        dome: Option<&nfs_formats::SkyDome>,
        map: Option<&nfs_formats::SkyMapBlock>,
        layers: [bool; 2],
    ) -> Self {
        let pixel = |column: usize, row: usize| -> [f32; 4] {
            let Some(t) = hzcl.filter(|t| t.width == 16 && t.height >= 2 && t.rgba.len() >= 16 * 2 * 4)
            else {
                return [1.0; 4];
            };
            let i = (row * 16 + (column & 15)) * 4;
            [0, 1, 2, 3].map(|c| f32::from(t.rgba[i + c]) / 255.0)
        };
        let mut u = SkyUniform::zeroed();
        u.view_projection = view_projection.to_cols_array_2d();
        u.horizon = [horizon.rotation, horizon.base, horizon.height, horizon.rows as f32];
        u.grid = [horizon.columns as f32, horizon.start as f32, 0.0, 0.0];
        for k in 0..16 {
            u.tiles[k] = [f32::from(horizon.tiles[k].0), f32::from(u8::from(horizon.tiles[k].1)), 0.0, 0.0];
            // Vertex k carries column start + k; the shader interpolates to vertex k + 1.
            u.top_colors[k] = pixel(horizon.start as usize + k, 0);
            u.bottom_colors[k] = pixel(horizon.start as usize + k, 1);
        }
        if let Some(d) = dome {
            // Without the txt the layers repeat once per cell.
            let inv = |scale: f32| if scale > 0.0 { 1.0 / scale } else { 0.0 };
            let (x, y) = map.map_or(([1.0; 3], [1.0; 3]), |m| (m.x_scale, m.y_scale));
            let on = |k: usize| if layers[k] { (inv(x[k]), inv(y[k])) } else { (0.0, 0.0) };
            let (l0, l1) = (on(0), on(1));
            u.dome = [d.angle, d.size, d.curvature, d.height];
            u.dome2 = [d.scale, 1.0, l0.0, l0.1];
            u.dome3 = [l1.0, l1.1, 0.0, 0.0];
        }
        u
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyVertex {
    position: [f32; 3],
}

fn generate_sky_sphere() -> (Vec<SkyVertex>, Vec<u16>) {
    let stacks = 16u16;
    let slices = 32u16;
    let radius = 1.0f32;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=stacks {
        let phi = std::f32::consts::PI * i as f32 / stacks as f32;
        for j in 0..=slices {
            let theta = 2.0 * std::f32::consts::PI * j as f32 / slices as f32;
            let x = radius * phi.sin() * theta.cos();
            let y = radius * phi.cos();
            let z = radius * phi.sin() * theta.sin();
            vertices.push(SkyVertex {
                position: [x, y, z],
            });
        }
    }

    for i in 0..stacks {
        for j in 0..slices {
            let first = i * (slices + 1) + j;
            let second = first + slices + 1;
            // Inverted winding (CW) so faces point inward
            indices.extend_from_slice(&[first, second, first + 1]);
            indices.extend_from_slice(&[first + 1, second, second + 1]);
        }
    }
    (vertices, indices)
}

#[allow(clippy::too_many_arguments)]
fn sky_layer_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_sky_state(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    camera: &Camera,
    width: u32,
    height: u32,
    tex_width: u32,
    tex_height: u32,
    tex_rgba: &[u8],
    layers: &[Option<nfs_assets::SkyTexture>; 3],
    horizon: &nfs_formats::SkyHorizon,
    dome: Option<&nfs_formats::SkyDome>,
    map: Option<&nfs_formats::SkyMapBlock>,
) -> SkyState {
    let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
    });

    let sky_camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky camera"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let sky_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky texture"),
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
            sky_layer_entry(2),
            sky_layer_entry(3),
            sky_layer_entry(4),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky"),
        bind_group_layouts: &[Some(&sky_camera_layout), Some(&sky_texture_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: Some(&sky_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &sky_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SkyVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &sky_shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
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
    });

    let vp = camera.view_projection(width, height);
    let sky_uniform = SkyUniform::new(
        vp,
        horizon,
        layers[2].as_ref(),
        dome,
        map,
        [layers[0].is_some(), layers[1].is_some()],
    );
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky camera"),
        contents: bytemuck::bytes_of(&sky_uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky camera"),
        layout: &sky_camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let sky_tex_view = upload_texture(device, queue, tex_width, tex_height, tex_rgba);
    let sky_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // Layers missing from the pack: plain blue sky map, no clouds, no haze.
    let fallbacks: [[u8; 4]; 3] = [[140, 170, 210, 255], [0; 4], [0; 4]];
    let layer_views: Vec<wgpu::TextureView> = layers
        .iter()
        .zip(fallbacks)
        .map(|(layer, fallback)| match layer {
            Some(t) if t.rgba.len() as u64 == u64::from(t.width) * u64::from(t.height) * 4 => {
                upload_texture(device, queue, t.width, t.height, &t.rgba)
            }
            _ => upload_texture(device, queue, 1, 1, &fallback),
        })
        .collect();
    let layer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky layer sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let texture_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky texture"),
        layout: &sky_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&sky_tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sky_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&layer_views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&layer_views[1]),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&layer_views[2]),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&layer_sampler),
            },
        ],
    });

    let (sphere_vertices, sphere_indices) = generate_sky_sphere();
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky vertices"),
        contents: bytemuck::cast_slice(&sphere_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky indices"),
        contents: bytemuck::cast_slice(&sphere_indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let index_count = sphere_indices.len() as u32;

    SkyState {
        pipeline,
        vertex_buffer,
        index_buffer,
        index_count,
        camera_buffer,
        camera_group,
        texture_group,
        uniform: sky_uniform,
        drift: sky_drift_velocity(map),
    }
}

/// `SkyConfig_Upgrade` 0x459700 defaults of a config without horizon fields.
fn default_sky_horizon() -> nfs_formats::SkyHorizon {
    nfs_formats::SkyHorizon {
        rotation: 0.0,
        start: 0,
        columns: 2,
        rows: 2,
        base: -100.0,
        height: 250.0,
        tiles: [(0, false); 16],
    }
}

/// The original start lasts 0x1c0 scheduler steps at 64 Hz (7 s): the announcer says "3" at
/// 4 s and "Go" at 7 s when the car is released (`Sched_Step` 0x414ff0, `docs/re/audio.md`).
fn original_start_countdown(session: &mut nfs_assets::RaceSession) {
    session.phase = nfs_assets::RacePhase::Countdown {
        remaining_secs: nfs_assets::audio::START_SECS,
        initial_secs: nfs_assets::audio::START_SECS,
    };
}

#[cfg(test)]
mod sim_loading_tests {
    use super::*;

    fn put_f32(data: &mut [u8], offset: usize, value: f32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn original_sim_preserves_road_pose_and_rejects_damage() {
        let mut data = [0u8; nfs_formats::SIM_FILE_SIZE];
        data[..4].copy_from_slice(b"Test");
        put_f32(&mut data, 0x40, 1100.0);
        put_f32(&mut data, 0x44, 2400.0);
        data[0x50..0x54].copy_from_slice(&1i32.to_le_bytes());
        put_f32(&mut data, 0x68, 3.2);
        put_f32(&mut data, 0x80, 3.9);
        put_f32(&mut data, 0x94, 6000.0);
        put_f32(&mut data, 0x10c, 1450.0);
        put_f32(&mut data, 0x110, 1450.0);

        let position = Vec3::new(30.0, 4.0, -12.0);
        let sim = simulation_from_bytes_at_pose(&data, position, 1.2).unwrap();
        assert_eq!(sim.body.mass, 1100.0);
        assert_eq!(sim.body.position.x, position.x);
        assert_eq!(sim.body.position.z, position.z);
        assert!((sim.body.position.y - position.y - sim.resting_height()).abs() < 1e-5);
        assert_eq!(sim.body.orientation, glam::Quat::from_rotation_y(1.2));
        assert!(simulation_from_bytes_at_pose(&data[..100], position, 1.2).is_err());
        put_f32(&mut data, 0x40, f32::NAN);
        assert!(simulation_from_bytes_at_pose(&data, position, 1.2).is_err());
    }

    #[test]
    fn quick_race_grid_respects_laps_sprint_and_opponents() {
        let mut course = nfs_assets::TrackCourse {
            waypoints: Vec::new(),
            checkpoints: Vec::new(),
            total_length: 0.0,
            is_circuit: true,
        };
        let (session, ai) = quick_race_grid(&course, 5, 3);
        assert_eq!(session.lap_tracker.total_laps, 5);
        assert_eq!(session.participants.len(), 4);
        assert_eq!(ai.len(), 3);
        let (session, ai) = quick_race_grid(&course, 3, 0);
        assert_eq!(session.lap_tracker.total_laps, 3);
        assert_eq!(session.participants.len(), 1);
        assert!(ai.is_empty());
        let (session, ai) = quick_race_grid(&course, 8, 7);
        assert_eq!(session.lap_tracker.total_laps, 8);
        assert_eq!((session.participants.len(), ai.len()), (8, 7));
        course.is_circuit = false;
        let (session, _) = quick_race_grid(&course, 5, 0);
        assert_eq!(session.lap_tracker.total_laps, 1);
    }
}

#[cfg(test)]
mod sky_tests {
    use super::*;

    fn drive_camera(target: Vec3) -> Camera {
        let mut cam = Camera::new([[-1.0; 3], [1.0; 3]]);
        cam.mode = CameraMode::Drive;
        cam.drive_pos = Vec3::new(500.0, 3.0, -200.0);
        cam.drive_target = target;
        // Stale orbit center must not influence the sky.
        cam.center = Vec3::new(0.0, 0.0, 0.0);
        cam
    }

    #[test]
    fn sky_view_ignores_translation() {
        let a = drive_camera(Vec3::new(500.0, 3.0, -210.0)).sky_view();
        assert_eq!(a.col(3), glam::Vec4::new(0.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn sky_view_rotates_with_drive_yaw() {
        // Looking along -Z, the world +X direction appears on the right of the screen.
        let fwd = drive_camera(Vec3::new(500.0, 3.0, -210.0)).sky_view();
        let east = fwd.transform_vector3(Vec3::X);
        assert!(east.x > 0.99);
        // After turning to look along +X, world +X is straight ahead (-Z in view space).
        let turned = drive_camera(Vec3::new(510.0, 3.0, -200.0)).sky_view();
        let east = turned.transform_vector3(Vec3::X);
        assert!(east.z < -0.99, "{east:?}");
    }
}

#[cfg(test)]
mod original_camera_tests {
    use super::*;
    use nfs_assets::camera::{DriverCamera, FlatWorld, PlayerCamera};
    use nfs_assets::physics::original_adapter::camera_car_from_sim;

    fn sim(yaw: f32) -> nfs_assets::physics::VehicleSimulation {
        let mut data = [0u8; nfs_formats::SIM_FILE_SIZE];
        data[..4].copy_from_slice(b"Test");
        for (offset, value) in [
            (0x40, 1100.0_f32),
            (0x44, 2400.0),
            (0x68, 3.2),
            (0x80, 3.9),
            (0x94, 6000.0),
            (0x10c, 1450.0),
            (0x110, 1450.0),
        ] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[0x50..0x54].copy_from_slice(&1i32.to_le_bytes());
        simulation_from_bytes_at_pose(&data, Vec3::new(20.0, 0.0, -50.0), yaw).unwrap()
    }

    #[test]
    fn original_pose_round_trips_through_the_6dof_body() {
        let mut sim = sim(0.7);
        sim.body.linear_velocity = Vec3::new(3.0, -0.5, -18.0);
        sim.body.angular_velocity = Vec3::new(0.1, 0.8, -0.05);
        let box_body = Vec3::new(0.02, 0.45, 0.1);
        let (pos, axes, vel, ang) = original_pose_from_body(&sim.body, box_body);
        let mut car = nfs_assets::physics::original_vehicle::OriginalCar::new(
            nfs_assets::physics::original_vehicle::CarSpecs::from_bytes(&[0u8; 0x1c4]),
        );
        car.position_330 = pos;
        car.axes_364 = axes;
        car.velocity_33c = vel;
        car.angular_388 = ang;
        let mut body = sim.body.clone();
        body.position = Vec3::ZERO;
        body.orientation = Quat::IDENTITY;
        set_body_from_original(&mut body, &car, box_body);
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-4;
        assert!(close(body.position, sim.body.position));
        assert!(close(body.forward(), sim.body.forward()));
        assert!(close(body.up(), sim.body.up()));
        assert!(close(body.linear_velocity, sim.body.linear_velocity));
        assert!(close(body.angular_velocity, sim.body.angular_velocity));
        // The drawn original car uses the same frame as the body.
        let m = original_body_matrix(&car) * Mat4::from_translation(-box_body);
        let b = sim.body.transform_matrix();
        for p in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::NEG_Z] {
            assert!(close(m.transform_point3(p), b.transform_point3(p)), "{p:?}");
        }
    }

    #[test]
    fn widescreen_keeps_the_4_3_vertical_fov() {
        let at_4_3 = hor_plus_vertical_fov(75.0, 4.0 / 3.0).to_degrees();
        assert!((at_4_3 - 59.84).abs() < 0.01, "{at_4_3}");
        for wide in [16.0 / 9.0, 21.0 / 9.0] {
            let v = hor_plus_vertical_fov(75.0, wide).to_degrees();
            assert!((v - at_4_3).abs() < 1e-4, "{wide}: {v}");
        }
        // Narrower than 4:3 keeps the horizontal 75 degrees instead.
        let narrow = hor_plus_vertical_fov(75.0, 1.0).to_degrees();
        assert!(narrow > at_4_3, "{narrow}");
    }

    #[test]
    fn horizontal_fov_75_is_59_8_vertical_at_4_3() {
        let v = vertical_fov(75.0, 640.0 / 480.0).to_degrees();
        assert!((v - 59.84).abs() < 0.01, "{v}");
        // perspective_rh x scale = 1 / tan(37.5 deg) at any aspect.
        for aspect in [4.0 / 3.0, 16.0 / 9.0, 0.5] {
            let p = Mat4::perspective_rh(vertical_fov(75.0, aspect), aspect, 0.5, 100.0);
            let expected = 1.0 / 37.5_f32.to_radians().tan();
            assert!((p.col(0).x - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn tail_cam_sits_behind_and_above_the_6dof_body() {
        let half = [0.9, 0.65, 2.1];
        for yaw in [0.0_f32, 1.1, -2.5] {
            let sim = sim(yaw);
            let mut player = PlayerCamera::new(DriverCamera::DEFAULT, START_CYCLE_INDEX);
            let car = camera_car_from_sim(&sim, Vec3::ZERO, half);
            for _ in 0..200 {
                player.advance(1.0 / 60.0, &car, &mut FlatWorld);
            }
            assert_eq!(player.name(), "TailCam0");
            let mut cam = Camera::new([[-1.0; 3], [1.0; 3]]);
            cam.mode = CameraMode::Drive;
            apply_original_view(&mut cam, &player.view(car.pos_330));
            let rel = cam.drive_pos - sim.body.position;
            // 3.30 m behind the rear bumper, 1.15 m up, along the scene body axes.
            let back = -rel.dot(sim.body.forward());
            assert!((back - (3.3 + half[2])).abs() < 0.01, "yaw {yaw}: {back}");
            assert!((rel.y - 1.15).abs() < 0.01, "yaw {yaw}: {rel:?}");
            assert!(rel.dot(sim.body.right()).abs() < 0.01);
            // Looks toward the car, horizon level, 75 deg horizontal, near 0.5.
            let fwd = (cam.drive_target - cam.drive_pos).normalize();
            assert!(fwd.dot(sim.body.forward()) > 0.9);
            assert!(cam.drive_up.y > 0.95);
            let right = fwd.cross(cam.drive_up);
            assert!(right.y.abs() < 1e-5);
            assert_eq!((cam.drive_hfov, cam.drive_near), (Some(75.0), 0.5));
        }
    }

    #[test]
    fn bumper_cam_is_rigid_in_scene_body_axes() {
        let sim = sim(0.7);
        let mut player = PlayerCamera::new(DriverCamera::DEFAULT, 0);
        let car = camera_car_from_sim(&sim, Vec3::ZERO, [0.9, 0.65, 2.1]);
        player.advance(0.1, &car, &mut FlatWorld);
        assert_eq!(player.name(), "BumperCam");
        let mut cam = Camera::new([[-1.0; 3], [1.0; 3]]);
        apply_original_view(&mut cam, &player.view(car.pos_330));
        let rel = cam.drive_pos - sim.body.position;
        assert!((rel - (sim.body.up() * 0.4 + sim.body.forward() * 0.9)).length() < 1e-4);
        let fwd = (cam.drive_target - cam.drive_pos).normalize();
        assert!((fwd - sim.body.forward()).length() < 1e-4);
        assert!((cam.drive_up - sim.body.up()).length() < 1e-4);
    }
}


#[cfg(test)]
mod wheel_placement_tests {
    use super::*;

    /// A car's SIM on its CRP model wheel pivots, or `None` without local game data.
    fn car_with_model_wheels(
        car: &str,
        sim_file: &str,
    ) -> Option<(
        nfs_assets::physics::VehicleSimulation,
        Vec<nfs_assets::CarWheel>,
    )> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData");
        let models = root.join("CarModel");
        if !models.join(format!("{car}.crp")).exists() {
            return None;
        }
        let mut files = nfs_assets::AssetFiles::new();
        for entry in std::fs::read_dir(&models).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            // The car's own files and the shared (non-car) FSH packs.
            if lower.starts_with(&car.to_ascii_lowercase())
                || (lower.ends_with(".fsh") && !lower.ends_with("d.fsh") && !lower.ends_with("l.fsh"))
            {
                files.insert(name, std::fs::read(&path).unwrap());
            }
        }
        let scene = nfs_assets::load_car(&files, car).unwrap();
        let bytes = std::fs::read(root.join("Simulation/CarData").join(sim_file)).unwrap();
        let mut sim = simulation_from_bytes_at_pose(&bytes, Vec3::ZERO, 0.0).unwrap();
        sim.apply_model_wheels(&scene.car_wheels).unwrap();
        Some((sim, scene.car_wheels))
    }

    /// The drawn wheel centre, in the drawn chassis frame (`body * adapter`), is the CRP wheel
    /// part's pivot with only y moved by the compression, at rest, in a turn, and after the
    /// body was moved by something other than the step (barrier push-out, contact response).
    #[test]
    fn prototype_wheels_stay_at_the_model_pivots() {
        for (car, sim_file) in [
            ("964", "964coupe36.sim"),
            ("993", "993cabrio36.sim"),
            ("356a", "356Acoupe16.sim"),
        ] {
            let Some((mut sim, wheels)) = car_with_model_wheels(car, sim_file) else {
                continue;
            };
            let adapter = chassis_adapter_from_wheels(
                &sim,
                wheels.iter().map(|w| (w.wheel_index, Vec3::from(w.center))),
            );
            let controls = nfs_assets::physics::VehicleControls::default();
            let turn = nfs_assets::physics::VehicleControls {
                steer: 0.6,
                throttle: 0.3,
                ..Default::default()
            };
            for phase in 0..3 {
                for _ in 0..120 {
                    sim.step(None, if phase == 0 { &controls } else { &turn }, 1.0 / 60.0);
                }
                if phase == 2 {
                    sim.body.position += Vec3::new(0.7, 0.2, -0.4);
                    sim.body.orientation = Quat::from_rotation_y(0.3) * sim.body.orientation;
                }
                let chassis = sim.body.transform_matrix() * Mat4::from_translation(adapter);
                for w in &wheels {
                    let source = Vec3::from(w.center);
                    let m = prototype_wheel_world_matrix(&sim, w.wheel_index, source, adapter)
                        .unwrap();
                    let local = chassis.inverse().transform_point3(m.transform_point3(Vec3::ZERO));
                    let compression = sim.suspension.wheels[w.wheel_index].compression;
                    let at = format!("{car} wheel {} phase {phase}: {local:?} vs {source:?}", w.wheel_index);
                    assert!((local.x - source.x).abs() < 1e-4, "x {at}");
                    assert!((local.z - source.z).abs() < 1e-4, "z {at}");
                    assert!((local.y - source.y - compression).abs() < 1e-4, "y {at}");
                }
            }
        }
    }
}

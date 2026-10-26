use std::collections::BTreeMap;

use js_sys::{Array, Uint8Array};
use nfs_assets::{AssetFiles, Scene, load_car, load_car_style, load_track};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::{Renderer, instance, request_gpu, summarize};

#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct BrowserViewer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Option<Renderer>,
    /// `Date.now()` when the front-end room appeared; the room yaw follows wall time.
    room_started_ms: f64,
    /// The canvas can be composited with premultiplied alpha (the compare screen's cars).
    premultiplied_ok: bool,
    default_alpha: wgpu::CompositeAlphaMode,
}

#[wasm_bindgen]
pub async fn create_viewer(canvas: HtmlCanvasElement) -> Result<BrowserViewer, JsValue> {
    if !has_webgpu() {
        return Err(JsValue::from_str(
            "WebGPU недоступен. Откройте страницу в браузере с поддержкой WebGPU.",
        ));
    }
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    let instance = instance();
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| JsValue::from_str(&format!("Не удалось создать поверхность WebGPU: {e}")))?;
    let (adapter, device, queue) = request_gpu(&instance, Some(&surface))
        .await
        .map_err(|e| JsValue::from_str(&format!("Не удалось открыть WebGPU: {e}")))?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| caps.formats.first().copied())
        .ok_or_else(|| JsValue::from_str("WebGPU не вернул формат canvas"))?;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: caps.alpha_modes[0],
        // WebGPU canvases offer only non-sRGB formats (bgra8unorm); render through an
        // sRGB view so the shaders' linear output is encoded as on native targets.
        view_formats: if format.is_srgb() { vec![] } else { vec![format.add_srgb_suffix()] },
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);
    // WebGPU canvases always accept `premultiplied`, although the wgpu capability list only
    // names the default mode.
    let premultiplied_ok = true;
    let default_alpha = config.alpha_mode;

    Ok(BrowserViewer {
        surface,
        config,
        device,
        queue,
        renderer: None,
        room_started_ms: 0.0,
        premultiplied_ok,
        default_alpha,
    })
}

#[wasm_bindgen]
impl BrowserViewer {
    pub fn load(&mut self, names: Array, bytes: Array, car: String) -> Result<String, JsValue> {
        let scene = files_from_js(names, bytes)
            .and_then(|files| load_car(&files, &car))
            .map_err(|e| JsValue::from_str(&e))?;
        self.set_scene(scene)
    }

    pub fn load_track(
        &mut self,
        names: Array,
        bytes: Array,
        track: String,
    ) -> Result<String, JsValue> {
        let scene = files_from_js(names, bytes)
            .and_then(|files| load_track(&files, &track))
            .map_err(|e| JsValue::from_str(&e))?;
        self.set_scene(scene)
    }

    /// `load_track` for a race direction: `reverse` runs the track backwards (`<track>1.lsp`, the
    /// AI node graph walked backwards, the course from the other end).
    pub fn load_track_direction(
        &mut self,
        names: Array,
        bytes: Array,
        track: String,
        reverse: bool,
    ) -> Result<String, JsValue> {
        let scene = files_from_js(names, bytes)
            .and_then(|files| nfs_assets::load_track_direction(&files, &track, reverse))
            .map_err(|e| JsValue::from_str(&e))?;
        self.set_scene(scene)
    }

    pub fn set_car_model(
        &mut self,
        names: Array,
        bytes: Array,
        car_name: String,
        style: Option<u32>,
    ) -> Result<String, JsValue> {
        let scene = files_from_js(names, bytes)
            .and_then(|files| load_car_style(&files, &car_name, style.unwrap_or(0)))
            .map_err(|e| JsValue::from_str(&e))?;
        if let Some(renderer) = &mut self.renderer {
            renderer
                .set_car_model(&scene, &car_name)
                .map_err(|e| JsValue::from_str(&e))?;
            Ok(format!("Car model '{car_name}' loaded successfully"))
        } else {
            Err(JsValue::from_str("Renderer not initialized"))
        }
    }

    /// Needle calibration of the race car: `nfs5.car` record `+0x2f0..+0x2fc` (rev needle at
    /// 0 rpm, per 1000 rpm, speed needle at 0, per unit), `docs/re/cockpit.md`.
    pub fn set_car_gauges(&mut self, tach_0: f32, tach_per_1000: f32, speedo_0: f32, speedo_per_unit: f32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_car_gauges([tach_0, tach_per_1000, speedo_0, speedo_per_unit]);
        }
    }

    /// Front-end 3D room (`room=` of a layout, e.g. `ID_GARAGE`) from its `FEData/Models`
    /// files; replaces the scene. Returns false for rooms that are not drawn yet.
    pub fn load_room(&mut self, names: Array, bytes: Array, room: String) -> Result<bool, JsValue> {
        let Some(spec) = crate::fe_room::spinning_room(&room) else {
            return Ok(false);
        };
        let mut scene = files_from_js(names, bytes)
            .and_then(|files| load_car(&files, spec.model))
            .map_err(|e| JsValue::from_str(&e))?;
        crate::fe_room::prepare_room_scene(&mut scene);
        self.set_scene(scene)?;
        if let Some(renderer) = &mut self.renderer {
            renderer.fe_room = Some(crate::fe_room::RoomView::new(spec));
        }
        self.room_started_ms = js_sys::Date::now();
        Ok(true)
    }

    /// Car shown in the room (layouts with a `3DCAR` control); `set_car_model` files.
    pub fn set_room_car(
        &mut self,
        names: Array,
        bytes: Array,
        car: String,
        style: Option<u32>,
    ) -> Result<(), JsValue> {
        self.set_car_model(names, bytes, car, style)?;
        if let Some(renderer) = &mut self.renderer {
            renderer.fe_room_car = true;
            renderer.fe_car_anim = crate::fe_room::CarAnim::CLOSED;
        }
        Ok(())
    }

    /// Moves a part of the room car to `target` (0 closed .. 1 open) over `duration_ms`:
    /// channel 1 cabrio top, 8 hood, 9 trunk lid.
    pub fn set_room_car_anim(&mut self, channel: u32, target: f32, duration_ms: f64) {
        if let Some(renderer) = &mut self.renderer {
            let now = js_sys::Date::now();
            if let Some(t) = renderer.fe_car_anim.tween_mut(channel as u8) {
                t.retarget(now, target.clamp(0.0, 1.0), duration_ms);
            }
        }
    }

    /// Target of a part's tween (see `set_room_car_anim`).
    pub fn room_car_anim_target(&self, channel: u32) -> f32 {
        self.renderer
            .as_ref()
            .and_then(|r| r.fe_car_anim.tween(channel as u8).map(|t| t.target()))
            .unwrap_or(0.0)
    }

    /// The room car has this moving part (1 cabrio top, 8 hood, 9 trunk lid).
    pub fn room_car_has_part(&self, channel: u32) -> bool {
        self.renderer
            .as_ref()
            .and_then(|r| r.car_render_has_part(channel as u8))
            .unwrap_or(false)
    }

    /// Room yaw in turns (0..1), for the click zones of the car.
    pub fn room_yaw_turns(&self) -> f32 {
        self.renderer
            .as_ref()
            .and_then(|r| r.fe_room.as_ref().map(|room| room.yaw_turns()))
            .unwrap_or(0.0)
    }

    /// Body paint of the room car, RGBA (4 floats); an empty array keeps the model's paint.
    pub fn set_room_paint(&mut self, rgba: &[f32]) {
        if let Some(renderer) = &mut self.renderer {
            renderer.fe_room_paint = <[f32; 4]>::try_from(rgba).ok();
        }
    }

    /// Compare screen: keep the car loaded last as the second car (spot 2).
    pub fn stash_compare_car(&mut self, paint: &[f32]) {
        if let Some(renderer) = &mut self.renderer {
            renderer.stash_compare_car();
            renderer.fe_compare_paint2 = <[f32; 4]>::try_from(paint).ok();
        }
    }

    /// Draws the room car (spot 1) and the stashed car (spot 2) on the compare screen.
    pub fn show_compare_cars(&mut self, show: bool) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.fe_compare = show && renderer.fe_room.is_some() && self.premultiplied_ok;
        // The cars are composited over the page's 2D backdrop: transparent canvas while shown.
        let alpha = if renderer.fe_compare {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            self.default_alpha
        };
        if self.config.alpha_mode != alpha {
            self.config.alpha_mode = alpha;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn premultiplied_alpha(&self) -> bool {
        self.premultiplied_ok
    }

    pub fn show_room_car(&mut self, show: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.fe_room_car = show && renderer.fe_room.is_some();
        }
    }

    /// Screens whose layout names an empty room (`ID_NONE`) show no 3D scene.
    pub fn hide_room(&mut self, hidden: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.fe_room_hidden = hidden && renderer.fe_room.is_some();
        }
    }

    pub fn has_room(&self) -> bool {
        self.renderer.as_ref().is_some_and(|r| r.fe_room.is_some())
    }

    /// Install the selected car's original 328-byte physics profile. `sim_name` (the `.sim`
    /// file name) selects the original model's model name; optional.
    pub fn set_car_sim(
        &mut self,
        bytes: Uint8Array,
        sim_name: Option<String>,
    ) -> Result<(), JsValue> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer not initialized"))?;
        renderer
            .set_car_sim(&bytes.to_vec(), sim_name.as_deref())
            .map_err(|e| JsValue::from_str(&e))
    }

    /// `.ais` data for the original AI (`GameData/Simulation/AICarData/<model>.ais`,
    /// model from `car_ais_model`); the AI opponents drive the player's car.
    pub fn set_car_ais(&mut self, bytes: Uint8Array) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_car_ais(&bytes.to_vec());
        }
    }

    /// The original AI cars as JSON (`null` without them).
    pub fn original_ai_state(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.original_ai_json())
            .unwrap_or_else(|| "null".into())
    }

    /// Model name (`res+0x2a8`) whose `.ais` the original AI needs.
    pub fn car_ais_model(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.car_ais_model())
            .unwrap_or_else(|| "default".into())
    }

    pub fn get_car_sim_mass(&self) -> f32 {
        self.renderer
            .as_ref()
            .and_then(|renderer| renderer.sim_car.as_ref())
            .map(|sim| sim.body.mass)
            .unwrap_or(0.0)
    }

    pub fn get_car_sim_name(&self) -> String {
        self.renderer
            .as_ref()
            .and_then(|renderer| renderer.sim_car.as_ref())
            .map(|sim| sim.spec_name.clone())
            .unwrap_or_default()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height);
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.camera.orbit(dx, dy);
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.camera.pan(dx, dy);
        }
    }

    pub fn move_ground(&mut self, forward: f32, right: f32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.move_ground(forward, right);
        }
    }

    pub fn toggle_camera_mode(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_camera_mode())
            .unwrap_or(false)
    }

    pub fn is_drive_mode(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.is_drive_mode())
            .unwrap_or(false)
    }

    pub fn zoom(&mut self, delta: f32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.camera.zoom(delta);
        }
    }

    pub fn reset_camera(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.camera.reset();
        }
    }

    pub fn cycle_paint_color(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.cycle_paint_color();
        }
    }

    pub fn set_show_topology(&mut self, show: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_show_topology(show);
        }
    }

    pub fn toggle_topology(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_topology())
            .unwrap_or(false)
    }

    pub fn has_topology(&self) -> bool {
        self.renderer
            .as_ref()
            .and_then(|r| r.topology.as_ref())
            .map(|t| t.vertex_count > 0)
            .unwrap_or(false)
    }

    pub fn update_car(&mut self, dt: f32, throttle: f32, steer: f32, handbrake: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.update_car(dt, throttle, steer, handbrake);
        }
    }

    /// Pause menu: freezes or resumes the race clock and scheduler.
    pub fn set_paused(&mut self, paused: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_paused(paused);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.renderer.as_ref().map(|r| r.is_paused()).unwrap_or(false)
    }

    /// Debug panel: the last solid object hit ("name [tag] size at x,y,z"), empty before a hit.
    pub fn object_hit_report(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.object_hit_report())
            .unwrap_or_default()
    }

    /// Debug (tools/browser-shot.mjs): `[smackables, awake, hit, smashed crates]`.
    pub fn smack_stats(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.smack_stats().to_vec())
            .unwrap_or_default()
    }

    /// Debug (tools/browser-shot.mjs): places the car here.
    pub fn debug_start(&mut self, x: f32, y: f32, z: f32, yaw: f32) -> f32 {
        match &mut self.renderer {
            Some(renderer) => renderer.debug_start([x, y, z], yaw),
            None => -1000.0,
        }
    }

    pub fn reset_car(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.reset_car();
        }
    }

    /// R key: the original track reset (the car back on the road at its track node, the race
    /// goes on). Taken by the next 32 Hz reset tick, which may refuse it as the original does.
    pub fn request_car_reset(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.request_car_reset();
        }
    }

    /// Number of track resets done since the page loaded (debug).
    pub fn get_car_resets(&self) -> u32 {
        self.renderer.as_ref().map_or(0, |r| r.car_resets)
    }

    /// C key: next drive camera; returns its name (original camera names in 6 DOF driving).
    pub fn cycle_car_view(&mut self) -> String {
        self.renderer
            .as_mut()
            .map(|r| r.cycle_car_view())
            .unwrap_or_default()
    }

    /// Debug: [live sparks, bursts, strongest barrier contact].
    pub fn get_spark_stats(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.spark_stats().to_vec())
            .unwrap_or_default()
    }

    /// L key: light switch (original `Controller.Lights`); returns whether it is on.
    pub fn toggle_lights(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_lights())
            .unwrap_or(false)
    }

    /// Shift+V (debug): side chase view off / swung 25 degrees one way / the other way.
    pub fn cycle_side_view(&mut self) -> f32 {
        self.renderer
            .as_mut()
            .map(|r| r.cycle_side_view())
            .unwrap_or(0.0)
    }

    /// Debug: look from `eye` at `target` (scene axes); 6 zeros clear it.
    pub fn debug_look(&mut self, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32) {
        if let Some(r) = &mut self.renderer {
            let (eye, target) = (glam::Vec3::new(ex, ey, ez), glam::Vec3::new(tx, ty, tz));
            r.debug_look = (eye != target).then_some((eye, target));
        }
    }

    /// Debug: drawn track particle systems, their particles, emitters, car particles.
    pub fn get_particle_stats(&self) -> Vec<f32> {
        self.renderer.as_ref().map(|r| r.particle_stats().to_vec()).unwrap_or_default()
    }

    /// Debug: camera node, rain, snow, sky fog, world fog, sky blend, lightning, wheel surfaces.
    pub fn get_weather_stats(&self) -> Vec<f32> {
        self.renderer.as_ref().map(|r| r.weather_stats().to_vec()).unwrap_or_default()
    }

    /// Debug: live snow and rain particles.
    pub fn get_precipitation_stats(&self) -> Vec<f32> {
        self.renderer.as_ref().map(|r| r.precipitation_stats().to_vec()).unwrap_or_default()
    }

    /// Debug: centre and forward of a track node (scene axes), for `debug_look`.
    pub fn debug_node_pose(&self, node: u32) -> Vec<f32> {
        self.renderer.as_ref().map(|r| r.node_pose(node as usize)).unwrap_or_default()
    }

    /// V key (debug): toggles the free camera; returns whether it is on.
    pub fn toggle_free_camera(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_free_camera())
            .unwrap_or(false)
    }

    /// Name of the current drive camera (HUD).
    pub fn get_camera_name(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.camera_name())
            .unwrap_or_default()
    }

    pub fn cycle_car_paint(&mut self) -> usize {
        if let Some(renderer) = &mut self.renderer {
            renderer.cycle_car_paint()
        } else {
            0
        }
    }

    pub fn get_car_speed(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_speed_kmh())
            .unwrap_or(0.0)
    }

    /// Read-only world-space telemetry: position xyz followed by forward xyz.
    pub fn get_car_pose(&self) -> Vec<f32> {
        let Some(renderer) = &self.renderer else {
            return Vec::new();
        };
        let pose = if renderer.sim_mode {
            renderer
                .sim_car
                .as_ref()
                .map(|sim| (sim.body.position, sim.body.forward()))
        } else {
            renderer.car.as_ref().map(|car| (car.pos, car.forward()))
        };
        pose.map(|(position, forward)| [position.to_array(), forward.to_array()].concat())
            .unwrap_or_default()
    }

    /// Drawn pose of the original car (the 64 Hz states blended for display): box centre in
    /// scene axes, then forward. Empty when the original model isn't driving.
    pub fn get_drawn_pose(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .and_then(|r| r.drawn_original_pose())
            .map(|(p, f)| [p.to_array(), f.to_array()].concat())
            .unwrap_or_default()
    }

    /// Eye of the drawn camera (scene axes), for smoothness checks.
    pub fn get_camera_eye(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.camera.eye().to_array().to_vec())
            .unwrap_or_default()
    }

    pub fn get_car_gear(&self) -> i32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_gear())
            .unwrap_or(0)
    }

    /// Read-only diagnostics: linear xyz, angular xyz, then for each wheel
    /// contact (0/1), angular speed, compression, normal load, longitudinal force;
    /// followed by body up xyz and actual steering angle.
    pub fn get_car_physics_state(&self) -> Vec<f32> {
        let Some(sim) = self.renderer.as_ref().and_then(|r| r.sim_car.as_ref()) else {
            return Vec::new();
        };
        let mut state = Vec::with_capacity(30);
        state.extend_from_slice(&sim.body.linear_velocity.to_array());
        state.extend_from_slice(&sim.body.angular_velocity.to_array());
        for wheel in &sim.suspension.wheels {
            state.extend_from_slice(&[
                if wheel.in_contact { 1.0 } else { 0.0 },
                wheel.tire.omega,
                wheel.compression,
                wheel.tire.normal_load,
                wheel.tire.longitudinal_force,
            ]);
        }
        state.extend_from_slice(&sim.body.up().to_array());
        state.push(sim.current_steer_angle);
        state
    }

    /// Read-only acceptance evidence: original backend, polygon/node counts,
    /// then one cached support polygon index per live simulation wheel (-1=miss).
    pub fn get_original_support_state(&self) -> Vec<f64> {
        let Some(renderer) = self.renderer.as_ref() else {
            return vec![];
        };
        let Some((polygons, nodes)) = renderer
            .road_surface
            .as_ref()
            .and_then(nfs_assets::RoadSurface::original_support_counts)
        else {
            return vec![];
        };
        let mut state = vec![1.0, polygons as f64, nodes as f64];
        if let Some(sim) = renderer.sim_car.as_ref() {
            state.extend(sim.suspension.wheels.iter().map(|wheel| {
                wheel
                    .support_owner
                    .polygon
                    .map_or(-1.0, |index| index as f64)
            }));
        }
        state
    }

    /// Constructed EDG entries share the live original support tree. This is
    /// geometry evidence, not a claim that body response is already restored.
    pub fn get_original_scene_object_count(&self) -> u32 {
        self.renderer
            .as_ref()
            .and_then(|r| r.road_surface.as_ref())
            .and_then(nfs_assets::RoadSurface::original_scene_object_count)
            .unwrap_or(0) as u32
    }

    /// Read-only render diagnostics: chassis adapter xyz, then per wheel:
    /// index, source center xyz, radius, world center xyz, mesh count.
    pub fn get_car_wheel_render_state(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.get_car_wheel_render_state())
            .unwrap_or_default()
    }

    /// Final wheel render matrices in FL, FR, RL, RR order as 4 column-major
    /// mat4 values (64 floats) when four wheel groups are bound.
    pub fn get_car_wheel_visuals(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.get_car_wheel_visuals())
            .unwrap_or_default()
    }

    pub fn get_car_wheel_mesh_count(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.get_car_wheel_mesh_count())
            .unwrap_or(0)
    }

    pub fn get_vehicle_contact_count(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.vehicle_contact_count)
            .unwrap_or(0)
    }

    pub fn get_car_rpm(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_rpm())
            .unwrap_or(0.0)
    }

    /// Car state for the tyre sounds, 16 floats (`tyre_audio::inputs_from_slice`).
    pub fn get_tyre_sound_inputs(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.get_tyre_sound_inputs())
            .unwrap_or_default()
    }

    /// Real engine RPM (`get_car_rpm` is rpm / 7000, clamped).
    pub fn get_car_engine_rpm(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_engine_rpm())
            .unwrap_or(0.0)
    }

    /// Original race HUD telemetry `[speed_ec4 m/s, rpm, gear_raw, redline, damage x9]`
    /// (car+0xec4 VA 0x40eec0, car+0xd82, car+0xee0, car+0x4e4 VA 0x422470).
    pub fn get_hud_telemetry(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.get_hud_telemetry())
            .unwrap_or_default()
    }

    /// Race standings JSON array (`name, player, rank, laps, progress, finished`).
    pub fn get_race_standings_json(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.get_race_standings_json())
            .unwrap_or_else(|| "[]".to_string())
    }

    /// HUD track map (`HTrackMap_BuildOutline` 0x432230): 4 header floats
    /// (bounding box centre x, z, extent, 1 if the course is a circuit) then line
    /// segments `x0, z0, x1, z1`.
    pub fn get_course_map(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.get_course_map())
            .unwrap_or_default()
    }

    /// Car markers, 8 floats per car (player first): x, z, fwd_x, fwd_z, half_width,
    /// half_length, is_player, paint index or -1.
    pub fn get_car_markers(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_markers())
            .unwrap_or_default()
    }

    /// SIM redline RPM of the loaded car.
    pub fn get_car_redline_rpm(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_redline_rpm())
            .unwrap_or(7000.0)
    }

    /// Current drive camera for the engine sound: 0 Chase, 1 Bumper (in-car), 2 Free.
    pub fn get_car_view(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_car_view())
            .unwrap_or(0)
    }

    pub fn has_car(&self) -> bool {
        self.renderer.as_ref().map(|r| r.has_car()).unwrap_or(false)
    }

    /// Debug: cycle the AI speed cap (none, 30 km/h, parked). Returns km/h,
    /// negative for none.
    pub fn cycle_ai_speed_limit(&mut self) -> f32 {
        self.renderer
            .as_mut()
            .map(|r| r.cycle_ai_speed_limit())
            .unwrap_or(-1.0)
    }

    /// Switch between the prototype and the original collision response.
    /// Returns true when the original response is active.
    pub fn toggle_collision_model(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| {
                r.toggle_collision_model()
                    == nfs_assets::race::original_collision::CollisionModel::Original
            })
            .unwrap_or(false)
    }

    /// Switch the player car between the prototype and the original vehicle model (O).
    /// Returns true when the original model is selected (it drives while 6 DOF physics is on).
    pub fn toggle_vehicle_model(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| {
                r.toggle_vehicle_model()
                    == nfs_assets::physics::original_vehicle::VehicleModel::Original
            })
            .unwrap_or(false)
    }

    /// Whether the original vehicle model can be selected (an original car was built from
    /// the loaded `.sim`).
    pub fn has_original_vehicle(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.original_vehicle.is_some())
    }

    /// Show or hide the collision-box wireframes. Returns the new state.
    pub fn toggle_collision_boxes(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_collision_boxes())
            .unwrap_or(false)
    }

    /// Player and nearest-AI collision box report (centres, extents, box gap).
    pub fn collision_box_report(&self) -> String {
        self.renderer
            .as_ref()
            .map(|r| r.collision_box_report())
            .unwrap_or_else(|| "no renderer".to_string())
    }

    /// Debug panel: original vehicle model selected (O).
    pub fn is_original_vehicle(&self) -> bool {
        self.renderer.as_ref().map(|r| r.vehicle_model_is_original()).unwrap_or(false)
    }

    /// Debug panel: original collision response selected (K).
    pub fn is_original_collision(&self) -> bool {
        self.renderer.as_ref().map(|r| r.collision_model_is_original()).unwrap_or(false)
    }

    /// Debug panel: AI speed cap in km/h, negative for none (J).
    pub fn get_ai_speed_limit(&self) -> f32 {
        self.renderer.as_ref().map(|r| r.ai_speed_limit_kmh()).unwrap_or(-1.0)
    }

    /// Debug panel: light switch (L).
    pub fn is_lights_on(&self) -> bool {
        self.renderer.as_ref().map(|r| r.lights_on()).unwrap_or(false)
    }

    /// Debug panel: free camera on (V).
    pub fn is_free_camera(&self) -> bool {
        self.renderer.as_ref().map(|r| r.is_free_camera()).unwrap_or(false)
    }

    /// Debug panel: side view angle in degrees (Shift+V).
    pub fn get_side_view(&self) -> f32 {
        self.renderer.as_ref().map(|r| r.side_view_degrees()).unwrap_or(0.0)
    }

    /// Debug panel: collision boxes shown (B).
    pub fn is_collision_boxes(&self) -> bool {
        self.renderer.as_ref().map(|r| r.show_collision_boxes).unwrap_or(false)
    }

    pub fn toggle_sim_mode(&mut self) -> bool {
        self.renderer
            .as_mut()
            .map(|r| r.toggle_sim_mode())
            .unwrap_or(false)
    }

    pub fn is_sim_mode(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.is_sim_mode())
            .unwrap_or(false)
    }

    pub fn get_race_phase(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_race_phase())
            .unwrap_or(0)
    }

    pub fn get_countdown_remaining(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_countdown_remaining())
            .unwrap_or(0.0)
    }

    pub fn get_player_position(&self) -> usize {
        self.renderer
            .as_ref()
            .map(|r| r.get_player_position())
            .unwrap_or(1)
    }

    pub fn get_total_participants(&self) -> usize {
        self.renderer
            .as_ref()
            .map(|r| r.get_total_participants())
            .unwrap_or(1)
    }

    pub fn get_current_lap(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_current_lap())
            .unwrap_or(1)
    }

    pub fn get_total_laps(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_total_laps())
            .unwrap_or(1)
    }

    pub fn get_current_lap_time(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_current_lap_time())
            .unwrap_or(0.0)
    }

    pub fn get_best_lap_time(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_best_lap_time())
            .unwrap_or(0.0)
    }

    pub fn is_wrong_way(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.is_wrong_way())
            .unwrap_or(false)
    }

    pub fn restart_race(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.restart_race();
        }
    }

    pub fn configure_quick_race(&mut self, laps: u32, opponents: u32) -> Result<(), JsValue> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer not initialized"))?;
        renderer
            .configure_quick_race(laps, opponents)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// LINEAR_TRACK of the track record (`nfs5.trk +0x6e != 1`): point to point; call before
    /// `configure_quick_race`.
    pub fn set_linear_track(&mut self, linear: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_linear_track(linear);
        }
    }

    /// Quick Race difficulty (`SKILL`), mirror, the opponents' skill values (8 per opponent,
    /// from `fe_pick_opponents_json`) and the body paints (RGBA; empty player paint = unchanged,
    /// 4 per opponent); call before `configure_quick_race`.
    pub fn set_race_options(
        &mut self,
        difficulty: i32,
        mirror: bool,
        skills: Vec<f32>,
        player_paint: Vec<f32>,
        opponent_paints: Vec<f32>,
    ) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_race_options(difficulty, mirror, &skills, &player_paint, &opponent_paints);
        }
    }

    /// Car setup screen choices: manual transmission and damage on (see `Renderer::set_car_setup`).
    pub fn set_car_setup(&mut self, manual_box: bool, damage: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_car_setup(manual_box, damage);
        }
    }

    /// Manual box gear event: +1 up, -1 down.
    pub fn shift_gear(&mut self, direction: i32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.shift_gear(direction);
        }
    }

    pub fn configure_mission(&mut self, is_factory: bool, time_limit: f32, has_opponent: bool) {
        if let Some(renderer) = &mut self.renderer {
            renderer.configure_mission(is_factory, time_limit, has_opponent);
        }
    }

    pub fn configure_mission_code(&mut self, code: String) {
        if let Some(renderer) = &mut self.renderer {
            renderer.configure_mission_code(&code);
        }
    }

    pub fn get_mission_goal_supported(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.get_mission_goal_supported())
    }

    pub fn get_mission_passed_gates(&self) -> u32 {
        self.renderer
            .as_ref()
            .map_or(0, |r| r.get_mission_passed_gates())
    }

    /// Read-only positions of source-triggerable props, in SCN placement order.
    pub fn get_scenario_prop_positions(&self) -> Vec<f32> {
        self.renderer
            .as_ref()
            .map(|r| r.get_scenario_prop_positions())
            .unwrap_or_default()
    }

    pub fn get_mission_goal_reached(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.get_mission_goal_reached())
    }

    pub fn get_mission_timed_out(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.get_mission_timed_out())
    }

    pub fn get_mission_elapsed_time(&self) -> f32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_mission_elapsed_time())
            .unwrap_or(0.0)
    }

    pub fn get_cone_hits(&self) -> u32 {
        self.renderer
            .as_ref()
            .map(|r| r.get_cone_hits())
            .unwrap_or(0)
    }

    pub fn get_stunt_180(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.get_stunt_180())
            .unwrap_or(false)
    }

    pub fn get_stunt_360(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.get_stunt_360())
            .unwrap_or(false)
    }

    pub fn get_stunt_jturn(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.get_stunt_jturn())
            .unwrap_or(false)
    }

    pub fn is_mission_mode(&self) -> bool {
        self.renderer
            .as_ref()
            .map(|r| r.is_mission_mode())
            .unwrap_or(false)
    }

    pub fn render(&mut self) -> Result<(), JsValue> {
        let view_format = self.view_format();
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        renderer.fe_anim_ms = js_sys::Date::now();
        if let Some(room) = &mut renderer.fe_room {
            room.seconds = (js_sys::Date::now() - self.room_started_ms).max(0.0) / 1000.0;
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&renderer.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(JsValue::from_str("Ошибка проверки поверхности WebGPU"));
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(view_format),
            ..Default::default()
        });
        renderer.render(&view);
        self.queue.present(frame);
        Ok(())
    }
}

impl BrowserViewer {
    /// Format the renderer draws in: the sRGB view of the canvas.
    fn view_format(&self) -> wgpu::TextureFormat {
        self.config.view_formats.first().copied().unwrap_or(self.config.format)
    }

    fn set_scene(&mut self, scene: Scene) -> Result<String, JsValue> {
        let summary = summarize(&scene);
        let renderer = Renderer::new(
            self.device.clone(),
            self.queue.clone(),
            self.view_format(),
            self.config.width,
            self.config.height,
            &scene,
        )
        .map_err(|e| JsValue::from_str(&e))?;
        self.renderer = Some(renderer);
        Ok(summary)
    }
}

/// Engine bank name for a SIM name from the bytes of `FEData/Data/nfs5.car` (first record wins).
#[wasm_bindgen]
pub fn engine_bank_for_sim(car_file: Uint8Array, sim_name: String) -> Result<String, JsValue> {
    crate::engine_audio::bank_for_sim(&car_file.to_vec(), &sim_name)
        .map_err(|e| JsValue::from_str(&e))
}

/// Original engine sound: decoded bank slots plus the mixer, stepped at a fixed 100 Hz.
/// One-shot (start/stop sample) finish state is tracked inside, in mixer-step time, so JS
/// only executes the returned commands.
#[wasm_bindgen]
pub struct EngineAudio {
    core: crate::engine_audio::EngineAudioCore,
}

#[wasm_bindgen]
impl EngineAudio {
    /// `names`/`bytes`: files including `<sim>.spl` and `<bank>.viv` (paths are matched by
    /// base name, case-insensitively).
    pub fn load(
        names: Array,
        bytes: Array,
        sim_name: String,
        bank: String,
    ) -> Result<EngineAudio, JsValue> {
        let files = files_from_js(names, bytes).map_err(|e| JsValue::from_str(&e))?;
        let core = crate::engine_audio::EngineAudioCore::load(&files, &sim_name, &bank)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(EngineAudio { core })
    }

    pub fn slot_count(&self) -> usize {
        self.core.slots().len()
    }
    /// Bank slot id of the `index`-th decoded slot.
    pub fn slot_id(&self, index: usize) -> u32 {
        self.core.slots()[index].id as u32
    }
    pub fn slot_sample_rate(&self, index: usize) -> u32 {
        self.core.slots()[index].sample_rate
    }
    pub fn slot_channels(&self, index: usize) -> u32 {
        self.core.slots()[index].channels.len() as u32
    }
    /// `(60 - root) * 100 + fine` cents; playbackRate = pitch/4096 * 2^(detune/1200).
    pub fn slot_detune_cents(&self, index: usize) -> i32 {
        self.core.slots()[index].detune_cents
    }
    /// Loop start in sample frames, or -1 when the slot doesn't loop.
    /// `f64`, not `i64`: wasm-bindgen turns `i64` into a JS BigInt, which can't
    /// be mixed with Numbers (it broke `loopStart / rate` in web/main.js).
    pub fn slot_loop_start(&self, index: usize) -> f64 {
        self.core.slots()[index]
            .loop_range
            .map_or(-1.0, |r| r.0 as f64)
    }
    /// Loop end in sample frames, or -1.
    pub fn slot_loop_end(&self, index: usize) -> f64 {
        self.core.slots()[index]
            .loop_range
            .map_or(-1.0, |r| r.1 as f64)
    }
    /// One channel of the slot's PCM, -1..1.
    pub fn slot_pcm(&self, index: usize, channel: usize) -> Vec<f32> {
        self.core.slots()[index].channels[channel].clone()
    }

    /// Steps the mixer in 1/100 s units for `dt` seconds. Returns commands, 7 ints each:
    /// `[kind, voice, slot, volume, pitch, pan, looped]` with kind 0 Start, 1 SetVolume,
    /// 2 SetPan, 3 SetPitch, 4 Stop. `view`: 0 Chase, 1 Bumper, 2 Free.
    pub fn update(
        &mut self,
        dt: f32,
        rpm: f32,
        redline: f32,
        throttle: f32,
        view: u8,
        gear: i32,
    ) -> Vec<i32> {
        self.core.update(dt, rpm, redline, throttle, view, gear)
    }

    pub fn set_running(&mut self, running: bool) {
        self.core.set_running(running);
    }
}

/// Original tyre squeal and surface rumble: decoded `SND_coll.bnk` slots plus `TyreSounds`
/// stepped at 32 Hz (`docs/re/audio.md`, "Tyre and surface sounds"). Commands as in
/// `EngineAudio::update`, with the fade ticks of a Stop in the pan field.
#[wasm_bindgen]
pub struct TyreAudio {
    core: crate::tyre_audio::TyreAudioCore,
}

#[wasm_bindgen]
impl TyreAudio {
    /// `names`/`bytes`: files including `SND_coll.bnk` (matched by base name).
    pub fn load(names: Array, bytes: Array) -> Result<TyreAudio, JsValue> {
        let files = files_from_js(names, bytes).map_err(|e| JsValue::from_str(&e))?;
        let core = crate::tyre_audio::TyreAudioCore::load(&files)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(TyreAudio { core })
    }
    pub fn slot_count(&self) -> usize {
        self.core.slots().len()
    }
    pub fn slot_id(&self, index: usize) -> u32 {
        self.core.slots()[index].id as u32
    }
    pub fn slot_sample_rate(&self, index: usize) -> u32 {
        self.core.slots()[index].sample_rate
    }
    pub fn slot_channels(&self, index: usize) -> u32 {
        self.core.slots()[index].channels.len() as u32
    }
    pub fn slot_detune_cents(&self, index: usize) -> i32 {
        self.core.slots()[index].detune_cents
    }
    pub fn slot_loop_start(&self, index: usize) -> f64 {
        self.core.slots()[index].loop_range.map_or(-1.0, |r| r.0 as f64)
    }
    pub fn slot_loop_end(&self, index: usize) -> f64 {
        self.core.slots()[index].loop_range.map_or(-1.0, |r| r.1 as f64)
    }
    pub fn slot_pcm(&self, index: usize, channel: usize) -> Vec<f32> {
        self.core.slots()[index].channels[channel].clone()
    }
    /// `inputs`: `Viewer::get_tyre_sound_inputs`; `view`: 0 Chase, 1 Bumper, 2 Free. Returns
    /// 7 ints per command: `[kind, voice, slot, volume, pitch, fade_ticks, looped]`, kind 0
    /// Start, 1 SetVolume, 3 SetPitch, 4 Stop.
    pub fn update(&mut self, dt: f32, inputs: Vec<f32>, view: u8) -> Vec<i32> {
        self.core.update(dt, &inputs, view)
    }
    /// Stop commands for every voice.
    pub fn silence(&mut self) -> Vec<i32> {
        self.core.silence()
    }
    /// Playing voices `[channel, slot, volume, control, pitch]*`, then `[ticks, starts]`.
    pub fn state(&self) -> Vec<i32> {
        self.core.state()
    }
}

/// One original music stream (SCHl / EA-XA R1), decoded in chunks (`docs/re/music.md`).
#[wasm_bindgen]
pub struct MusicTrack {
    core: crate::game_audio::MusicTrackCore,
}

#[wasm_bindgen]
impl MusicTrack {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: Uint8Array) -> Result<MusicTrack, JsValue> {
        crate::game_audio::MusicTrackCore::new(bytes.to_vec())
            .map(|core| MusicTrack { core })
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn sample_rate(&self) -> u32 {
        self.core.sample_rate()
    }
    pub fn channels(&self) -> u32 {
        self.core.channels() as u32
    }
    pub fn total_frames(&self) -> f64 {
        self.core.total_frames() as f64
    }
    pub fn done(&self) -> bool {
        self.core.done()
    }
    /// At least `min_frames` frames (fewer at the end), planar: channel 0 then channel 1.
    pub fn decode(&mut self, min_frames: u32) -> Result<Vec<f32>, JsValue> {
        self.core
            .decode(min_frames as usize)
            .map_err(|e| JsValue::from_str(&e))
    }
}

/// The original music player (`nfs_assets::audio::MusicPlayer`). Every call queues stream
/// commands; `take_commands` returns them as `[kind, a, b]` triples: 0 Start(file),
/// 1 SetVolume(v), 2 Fade(ms, v), 3 Stop.
#[wasm_bindgen]
pub struct MusicDirector {
    core: crate::game_audio::MusicDirectorCore,
}

#[wasm_bindgen]
impl MusicDirector {
    /// `names`: archive file names in TOC order; `level`: music level 0..127.
    #[wasm_bindgen(constructor)]
    pub fn new(names: Array, level: i32, seed: u32) -> MusicDirector {
        let files = names.iter().filter_map(|v| v.as_string()).collect();
        MusicDirector {
            core: crate::game_audio::MusicDirectorCore::new(files, level, seed),
        }
    }
    pub fn set_playlist(&mut self, ids: Vec<i32>) {
        self.core.player.set_playlist(&ids);
    }
    pub fn set_front_end(&mut self, on: bool) {
        self.core.player.front_end = on;
    }
    pub fn set_random_playlist(&mut self, on: bool) {
        self.core.player.random_playlist = on;
    }
    /// `Music_Play` 0x4ae4d0; `pattern` empty = keep the playlist.
    pub fn play(&mut self, pattern: String, mode: u8) -> u32 {
        let p = (!pattern.is_empty()).then_some(pattern.as_str());
        self.core.play(p, mode) as u32
    }
    pub fn stop(&mut self, ms: i32) {
        self.core.player.stop(ms);
    }
    pub fn set_level(&mut self, level: i32) {
        self.core.set_level(level);
    }
    pub fn level(&self) -> i32 {
        self.core.player.level()
    }
    pub fn toggle(&mut self) {
        self.core.toggle();
    }
    pub fn set_status(&mut self, playing: bool, volume: i32) {
        self.core
            .player
            .set_status(nfs_assets::audio::StreamStatus { playing, volume });
    }
    /// `Music_Tick` 0x4ad9c0 with the stream state.
    pub fn tick(&mut self, playing: bool, volume: i32) {
        self.core.tick(playing, volume);
    }
    pub fn take_commands(&mut self) -> Vec<i32> {
        self.core.take_commands()
    }
    pub fn index(&self) -> i32 {
        self.core.player.index()
    }
    /// File of the open stream, or −1.
    pub fn current_file(&self) -> i32 {
        self.core.player.current_file().map_or(-1, |f| f as i32)
    }
    pub fn playlist(&self) -> Vec<u8> {
        self.core.player.playlist().to_vec()
    }
    /// Switch state: 0 idle, 1 pending, 2 started, −1 stopping.
    pub fn state(&self) -> i32 {
        use nfs_assets::audio::SwitchState::*;
        match self.core.player.state() {
            Idle => 0,
            Pending => 1,
            Started => 2,
            Stopping => -1,
        }
    }
}

/// Race announcer: phrases of `zzzxeng.viv` and the original calls (`docs/re/audio.md`).
#[wasm_bindgen]
pub struct RaceAnnouncer {
    core: crate::game_audio::AnnouncerCore,
}

#[wasm_bindgen]
impl RaceAnnouncer {
    pub fn load(viv: Uint8Array, speech_level: i32, seed: u32) -> Result<RaceAnnouncer, JsValue> {
        crate::game_audio::AnnouncerCore::load(&viv.to_vec(), speech_level, seed)
            .map(|core| RaceAnnouncer { core })
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn phrase_count(&self) -> u32 {
        self.core.phrases.len() as u32
    }
    pub fn phrase_name(&self, i: u32) -> String {
        self.core.phrases[i as usize].name.clone()
    }
    pub fn phrase_rate(&self, i: u32) -> u32 {
        self.core.phrases[i as usize].sample_rate
    }
    pub fn phrase_pcm(&self, i: u32) -> Vec<f32> {
        self.core.phrases[i as usize].pcm.clone()
    }
    /// Phrase volume 0..127.
    pub fn volume(&self) -> i32 {
        self.core.volume()
    }
    pub fn set_speech_level(&mut self, level: i32) {
        self.core.speech_level = level.clamp(0, 127);
    }
    pub fn reset(&mut self) {
        self.core.reset();
    }
    /// Countdown at scheduler step `step` (64 Hz); returns the queued phrase or −1.
    pub fn countdown(&mut self, step: i32) -> i32 {
        self.core.countdown(step)
    }
    pub fn lap_completed(&mut self, laps_done: u32, total: u32, cars: u32, place: u32, best: bool) {
        self.core.lap_completed(laps_done, total, cars, place, best);
    }
    /// Queues the result call; returns the result music pattern or "".
    pub fn race_finished(&mut self, place: u32, cars: u32) -> String {
        self.core.race_finished(place, cars).to_string()
    }
    /// Next phrase to start or −1; `remaining_cs` < 0 when nothing plays.
    pub fn update(&mut self, remaining_cs: i32) -> i32 {
        self.core.update(remaining_cs)
    }
}

fn files_from_js(names: Array, bytes: Array) -> Result<AssetFiles, String> {
    if names.length() != bytes.length() {
        return Err("internal file list mismatch".into());
    }
    let mut files = BTreeMap::new();
    for index in 0..names.length() {
        let name = names
            .get(index)
            .as_string()
            .ok_or("file name is not a string")?
            .replace('\\', "/")
            .to_ascii_lowercase();
        let data = Uint8Array::new(&bytes.get(index)).to_vec();
        files.insert(name, data);
    }
    Ok(files)
}

fn has_webgpu() -> bool {
    js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("GPU")).unwrap_or(false)
        || web_sys::window()
            .and_then(|window| js_sys::Reflect::get(&window, &JsValue::from_str("navigator")).ok())
            .and_then(|navigator| js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu")).ok())
            .is_some()
}

#[wasm_bindgen]
pub fn shell_profile_new(name: String) -> String {
    let mut profile = nfs_game::profile::PlayerProfile::new(&name);
    let _ = profile.buy_initial_356();
    profile.to_json_string()
}

#[wasm_bindgen]
pub fn shell_profile_buy_356(profile_json: String) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;
    profile
        .buy_initial_356()
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_apply_event_result(
    profile_json: String,
    event_id: String,
    player_time_sec: f32,
    position: usize,
) -> Result<String, JsValue> {
    let profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;

    let event = if event_id == "0M01" {
        nfs_game::events::get_factory_driver_first_event()
    } else {
        let car_model = profile
            .garage
            .get(profile.selected_car_index)
            .map(|c| c.model_name.as_str())
            .unwrap_or("356_1");
        let car_sim = profile
            .garage
            .get(profile.selected_car_index)
            .map(|c| c.sim_name.as_str())
            .unwrap_or("356coupe11");
        nfs_game::events::get_evolution_first_event(car_model, car_sim)
    };

    let mut shell = nfs_game::shell::GameShell::with_profile(profile);
    shell.screen = nfs_game::shell::Screen::Racing {
        event,
        paused: false,
    };
    shell.finish_race(player_time_sec, position);

    let updated_profile = shell
        .profile
        .ok_or_else(|| JsValue::from_str("Missing profile"))?;
    Ok(updated_profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_get_first_evolution_event() -> String {
    let event = nfs_game::events::get_evolution_first_event("356_1", "356coupe11");
    format!(
        r#"{{"id":"{}","title":"{}","description":"{}","track_name":"{}","laps":{},"opponents":{},"entry_fee":{},"first_prize":{}}}"#,
        event.id,
        event.title,
        event.description,
        event.track_name,
        event.laps,
        event.opponents_count,
        event.entry_fee,
        event.first_prize
    )
}

/// Parses `FEData/Data/nfs5.car` into a JSON array of
/// `{index, display_name, model, sim_name, bank}` (all records, catalogue order).
#[wasm_bindgen]
pub fn parse_car_catalogue_json(bytes: Uint8Array) -> Result<String, JsValue> {
    let entries =
        nfs_formats::parse_car_catalogue(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    Ok(nfs_formats::catalogue_json(&entries))
}

#[wasm_bindgen]
pub fn shell_get_dealership_catalog() -> String {
    let cars = nfs_game::economy::get_dealership_catalog();
    let json_items: Vec<String> = cars
        .iter()
        .map(|c| {
            format!(
                r#"{{"id":"{}","model_name":"{}","sim_name":"{}","display_name":"{}","year":{},"era":"{:?}","price":{},"base_price":{},"condition":{:.2}}}"#,
                c.id,
                c.model_name,
                c.sim_name,
                c.display_name,
                c.year,
                c.era,
                c.price,
                c.base_price,
                c.overall_condition
            )
        })
        .collect();
    format!("[{}]", json_items.join(","))
}

#[wasm_bindgen]
pub fn shell_get_used_cars() -> String {
    let used_cars =
        nfs_game::economy::generate_used_car_market(nfs_game::tournament::TournamentEra::Modern);
    let json_items: Vec<String> = used_cars
        .iter()
        .map(|c| {
            format!(
                r#"{{"id":"{}","model_name":"{}","sim_name":"{}","display_name":"{}","year":{},"era":"{:?}","price":{},"base_price":{},"mileage_km":{},"condition":{:.2}}}"#,
                c.id,
                c.model_name,
                c.sim_name,
                c.display_name,
                c.year,
                c.era,
                c.price,
                c.base_price,
                c.mileage_km,
                c.overall_condition
            )
        })
        .collect();
    format!("[{}]", json_items.join(","))
}

#[wasm_bindgen]
pub fn shell_get_parts_catalog(car_model: String) -> String {
    let all_parts = nfs_game::economy::get_standard_parts_shop();
    let parts: Vec<_> = all_parts
        .into_iter()
        .filter(|p| {
            p.compatible_car_model.is_empty()
                || car_model
                    .to_lowercase()
                    .contains(&p.compatible_car_model.to_lowercase())
                || p.compatible_car_model
                    .to_lowercase()
                    .contains(&car_model.to_lowercase())
        })
        .collect();
    let json_items: Vec<String> = parts
        .iter()
        .map(|p| {
            format!(
                r#"{{"part_id":{},"name":"{}","category":"{:?}","category_name":"{}","price":{},"modifier_value":{:.2}}}"#,
                p.part_id,
                p.name,
                p.category,
                p.category.display_name(),
                p.price,
                p.modifier_value
            )
        })
        .collect();
    format!("[{}]", json_items.join(","))
}

#[wasm_bindgen]
pub fn shell_get_tournament_cups(era_name: String) -> String {
    let era = match era_name.to_lowercase().as_str() {
        "golden" => nfs_game::tournament::TournamentEra::Golden,
        "modern" => nfs_game::tournament::TournamentEra::Modern,
        _ => nfs_game::tournament::TournamentEra::Classic,
    };
    let cups: Vec<_> = nfs_game::tournament::get_tournaments_catalog()
        .into_iter()
        .filter(|c| c.era == era)
        .collect();
    let json_items: Vec<String> = cups
        .iter()
        .map(|c| {
            let total_prize_purse: u32 = c
                .stages
                .iter()
                .map(|s| s.prize_first + s.prize_second + s.prize_third)
                .sum();
            let legs_json: Vec<String> = c
                .stages
                .iter()
                .map(|s| {
                    format!(
                        r#"{{"leg_index":{},"track_id":"{}","track_display_name":"{}","laps":{}}}"#,
                        s.stage_index, s.track_id, s.track_display_name, s.laps
                    )
                })
                .collect();
            format!(
                r#"{{"id":"{}","title":"{}","description":"{}","entry_fee":{},"total_prize_purse":{},"legs":[{}]}}"#,
                c.id,
                c.title,
                c.description,
                c.entry_fee,
                total_prize_purse,
                legs_json.join(",")
            )
        })
        .collect();
    format!("[{}]", json_items.join(","))
}

#[wasm_bindgen]
pub fn shell_buy_car(
    profile_json: String,
    car_id: String,
    is_used: bool,
) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;

    let market_car = if is_used {
        let used = nfs_game::economy::generate_used_car_market(
            nfs_game::tournament::TournamentEra::Modern,
        );
        used.into_iter()
            .find(|c| c.id == car_id)
            .ok_or_else(|| JsValue::from_str("Car not found in used market"))?
    } else {
        let catalog = nfs_game::economy::get_dealership_catalog();
        catalog
            .into_iter()
            .find(|c| c.id == car_id)
            .ok_or_else(|| JsValue::from_str("Car not found in dealership"))?
    };

    profile
        .buy_car(&market_car)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_sell_car(profile_json: String, car_index: usize) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;
    profile
        .sell_car(car_index)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_buy_part(
    profile_json: String,
    car_index: usize,
    part_id: u32,
) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;
    let _car_model = profile
        .garage
        .get(car_index)
        .map(|c| c.model_name.clone())
        .ok_or_else(|| JsValue::from_str("Car index out of bounds"))?;
    let catalog = nfs_game::economy::get_standard_parts_shop();
    let shop_part = catalog
        .into_iter()
        .find(|p| p.part_id == part_id)
        .ok_or_else(|| JsValue::from_str("Part not found in shop"))?;

    let installed_part = nfs_game::parts::InstalledPart::new(
        shop_part.part_id,
        &shop_part.name,
        shop_part.category,
        shop_part.price,
        shop_part.modifier_value,
    );

    profile
        .buy_and_install_part(car_index, installed_part)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_repair_car(profile_json: String, car_index: usize) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;
    profile
        .repair_car(car_index)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(profile.to_json_string())
}

#[wasm_bindgen]
pub fn shell_get_first_factory_event() -> String {
    let event = nfs_game::events::get_factory_driver_first_event();
    let max_sec = match event.goal {
        nfs_game::events::EventGoal::TimeLimit { max_seconds } => max_seconds,
        _ => 32.0,
    };
    format!(
        r#"{{"id":"{}","title":"{}","description":"{}","track_name":"{}","laps":{},"time_limit":{:.1},"pass_message":"{}","fail_message":"{}"}}"#,
        event.id,
        event.title,
        event.description,
        event.track_name,
        event.laps,
        max_sec,
        event.pass_message,
        event.fail_message
    )
}

#[wasm_bindgen]
pub fn shell_get_factory_missions() -> String {
    let missions = nfs_game::factory_driver::get_factory_missions_catalog();
    let json_items: Vec<String> = missions
        .iter()
        .map(|m| {
            let (mtype, limit) = match &m.mission_type {
                nfs_game::factory_driver::MissionType::SlalomCourse {
                    time_limit_sec, ..
                } => ("slalom", *time_limit_sec),
                nfs_game::factory_driver::MissionType::Spin360Exercise {
                    time_limit_sec,
                    ..
                } => ("spin360", *time_limit_sec),
                nfs_game::factory_driver::MissionType::Slide180Exercise {
                    time_limit_sec,
                    ..
                } => ("slide180", *time_limit_sec),
                nfs_game::factory_driver::MissionType::PorscheCommercial { time_limit_sec } => {
                    ("commercial", *time_limit_sec)
                }
                nfs_game::factory_driver::MissionType::CarDelivery {
                    time_limit_sec, ..
                } => ("delivery", *time_limit_sec),
                nfs_game::factory_driver::MissionType::DuelRace { .. } => ("duel", 0.0),
                nfs_game::factory_driver::MissionType::CaptureTheFlag {
                    time_limit_sec, ..
                } => ("flag", *time_limit_sec),
            };
            format!(
                r#"{{"index":{},"code":"{}","tier":{},"title":"{}","briefing":"{}","tip":"{}","car_model":"{}","car_sim":"{}","track_id":"{}","track_name":"{}","type":"{}","time_limit":{:.1},"pass_message":"{}","fail_message":"{}"}}"#,
                m.mission_index,
                m.code,
                m.tier,
                m.title.replace('"', "\\\""),
                m.briefing.replace('"', "\\\""),
                m.tip.replace('"', "\\\""),
                m.car_model,
                m.car_sim,
                m.track_id,
                m.track_name.replace('"', "\\\""),
                mtype,
                limit,
                m.pass_message.replace('"', "\\\""),
                m.fail_message.replace('"', "\\\"")
            )
        })
        .collect();
    format!("[{}]", json_items.join(","))
}

#[wasm_bindgen]
pub fn shell_complete_factory_mission(
    profile_json: String,
    mission_idx: usize,
    time_taken: f32,
    has_180: bool,
    has_360: bool,
    has_jturn: bool,
    cone_hits: u32,
    damage: f32,
    is_winner: bool,
) -> Result<String, JsValue> {
    let mut profile = nfs_game::profile::PlayerProfile::from_json_string(&profile_json)
        .map_err(|e| JsValue::from_str(&e))?;
    let mut stunts = nfs_game::factory_driver::StuntDetector::new();
    stunts.has_done_180 = has_180;
    stunts.has_done_360 = has_360;
    stunts.has_done_jturn = has_jturn;
    stunts.cone_hits = cone_hits;
    stunts.current_damage = damage;

    let eval = nfs_game::factory_driver::apply_mission_completion(
        &mut profile,
        mission_idx,
        time_taken,
        &stunts,
        is_winner,
    )
    .map_err(|e| JsValue::from_str(&e))?;

    let rank_str = eval.promotion_rank.map(|r| r.display_name()).unwrap_or("");
    let car_name = eval
        .awarded_car
        .as_ref()
        .map(|c| c.display_name.as_str())
        .unwrap_or("");

    Ok(format!(
        r#"{{"passed":{},"total_time":{:.2},"penalty":{:.1},"feedback":"{}","rank":"{}","reward_car":"{}","profile":{}}}"#,
        eval.passed,
        eval.total_time_seconds,
        eval.penalty_seconds,
        eval.feedback_message.replace('"', "\\\""),
        rank_str,
        car_name,
        profile.to_json_string()
    ))
}

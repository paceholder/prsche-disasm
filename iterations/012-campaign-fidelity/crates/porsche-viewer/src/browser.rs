use std::collections::BTreeMap;

use js_sys::{Array, Uint8Array};
use nfs_assets::{AssetFiles, Scene, load_car, load_track};
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
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    Ok(BrowserViewer {
        surface,
        config,
        device,
        queue,
        renderer: None,
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

    pub fn set_car_model(
        &mut self,
        names: Array,
        bytes: Array,
        car_name: String,
    ) -> Result<String, JsValue> {
        let scene = files_from_js(names, bytes)
            .and_then(|files| load_car(&files, &car_name))
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

    /// Install the selected car's original 328-byte physics profile.
    pub fn set_car_sim(&mut self, bytes: Uint8Array) -> Result<(), JsValue> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer not initialized"))?;
        renderer
            .set_car_sim(&bytes.to_vec())
            .map_err(|e| JsValue::from_str(&e))
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

    pub fn reset_car(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            renderer.reset_car();
        }
    }

    pub fn cycle_car_view(&mut self) -> u32 {
        if let Some(renderer) = &mut self.renderer {
            match renderer.cycle_car_view() {
                crate::arcade::DriveViewMode::Chase => 0,
                crate::arcade::DriveViewMode::Bumper => 1,
                crate::arcade::DriveViewMode::Free => 2,
            }
        } else {
            0
        }
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
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
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
        let view = frame.texture.create_view(&Default::default());
        renderer.render(&view);
        self.queue.present(frame);
        Ok(())
    }
}

impl BrowserViewer {
    fn set_scene(&mut self, scene: Scene) -> Result<String, JsValue> {
        let summary = summarize(&scene);
        let renderer = Renderer::new(
            self.device.clone(),
            self.queue.clone(),
            self.config.format,
            self.config.width,
            self.config.height,
            &scene,
        )
        .map_err(|e| JsValue::from_str(&e))?;
        self.renderer = Some(renderer);
        Ok(summary)
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

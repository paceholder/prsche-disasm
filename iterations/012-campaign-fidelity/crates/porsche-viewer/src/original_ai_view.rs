//! The original AI in the renderer (change `original-ai`): with the original
//! vehicle model (O) the AI opponents are `OriginalCar`s driven by the
//! original AI (`nfs_assets::race::original_ai`), stepped with the player in
//! the original scheduler order. The `AiOpponent`s mirror them for drawing,
//! lights and race progress. The prototype keeps the kinematic AI.
//!
//! Child module of `renderer` (private renderer fields).

use super::*;
use nfs_assets::physics::original_contact::PreparedContactState;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_support_owner::OriginalSupportOwner;
use nfs_assets::physics::original_vehicle::ground::{FlatGround, GroundAdapter, LEGACY_SURFACE};
use nfs_assets::physics::original_vehicle::{OriginalCar, RoadGround};
use nfs_assets::race::original_ai::field::{FieldQueue, OriginalAiField};

struct Queue<'a> {
    surface: Option<&'a nfs_assets::RoadSurface>,
    body_owner: &'a mut OriginalSupportOwner,
    camera: &'a mut nfs_assets::camera::PlayerCamera,
    half: [f32; 3],
}

impl FieldQueue for Queue<'_> {
    fn ground(
        &mut self,
        _index: usize,
        car: &mut OriginalCar,
        adapter: &mut GroundAdapter,
        owners: &mut [OriginalSupportOwner; 4],
        lcg: &mut OriginalLcg,
    ) {
        match self.surface {
            Some(surface) => adapter.ground_step(car, &mut RoadGround { surface, owners }, lcg),
            None => adapter.ground_step(
                car,
                &mut FlatGround {
                    height: 0.0,
                    surface: LEGACY_SURFACE,
                },
                lcg,
            ),
        }
    }

    fn barrier(
        &mut self,
        _index: usize,
        state: &mut PreparedContactState,
        half: [f32; 3],
        owner: Option<&mut OriginalSupportOwner>,
    ) -> bool {
        let Some(surface) = self.surface.filter(|s| s.uses_original_support()) else {
            return false;
        };
        let owner = match owner {
            Some(o) => o,
            None => &mut *self.body_owner,
        };
        matches!(surface.original_barrier_response(state, half, owner), Ok(Some(_)))
    }

    fn camera(&mut self, player: &OriginalCar) {
        self.camera.step(
            &nfs_assets::physics::original_vehicle::driver::camera_car(player, self.half),
            &mut nfs_assets::camera::FlatWorld,
        );
    }
}

impl Renderer {
    /// `.ais` data of the player's car (`GameData/Simulation/AICarData/<model>.ais`,
    /// model from [`Self::car_ais_model`]); the AI opponents use the player's car
    /// (approximation A40).
    pub fn set_car_ais(&mut self, bytes: &[u8]) {
        self.original_ais = Some(bytes.to_vec());
        self.rebuild_original_ai();
    }

    /// Model name (`res+0x2a8`) whose `.ais` the original AI needs.
    pub fn car_ais_model(&self) -> String {
        self.original_sim
            .as_ref()
            .map(|(_, m)| m.clone())
            .unwrap_or_else(|| "default".into())
    }

    /// Builds the original AI cars at the opponents' current poses, if the original
    /// model drives and the track graph, the `.ais` and the player's original car exist;
    /// otherwise drops them (the kinematic AI takes over).
    pub(super) fn rebuild_original_ai(&mut self) {
        let had = self.original_ai.take().is_some();
        if had {
            for ai in &mut self.ai_opponents {
                ai.rotation_override = None;
                ai.apply_collision_velocity([0.0; 3]);
            }
        }
        if !self.original_active() || self.ai_opponents.is_empty() {
            return;
        }
        let (Some(track), Some(ais), Some(player), Some((_, model))) = (
            self.original_ai_track.as_ref(),
            self.original_ais.as_ref(),
            self.original_vehicle.as_ref(),
            self.original_sim.as_ref(),
        ) else {
            return;
        };
        let mut template = player.clone();
        let mut s140 = template.car.specs.field_140;
        let mut field = OriginalAiField::new(track.clone(), ais, model, &mut s140);
        template.car.specs.field_140 = s140;
        // Grid on the node graph ahead of the player (A44): upstream's grid behind the
        // player has no road on point-to-point tracks (alps starts at node 0).
        let start = field.track.nodes.locate(player.car.position_330, 0);
        for slot in 0..self.ai_opponents.len() {
            let (centre, axes) = field.grid_slot(start, slot);
            let mut owners: [OriginalSupportOwner; 4] = Default::default();
            match self.road_surface.as_ref() {
                Some(surface) => field.spawn(
                    template.clone(),
                    centre,
                    axes,
                    &mut RoadGround {
                        surface,
                        owners: &mut owners,
                    },
                ),
                None => field.spawn(
                    template.clone(),
                    centre,
                    axes,
                    &mut FlatGround {
                        height: 0.0,
                        surface: LEGACY_SURFACE,
                    },
                ),
            }
            if let Some(c) = field.cars.last_mut() {
                c.vehicle.owners = owners;
            }
        }
        self.original_ai = Some(field);
        self.drawn_ai.clear();
        self.mirror_original_ai(0.0, None);
    }

    /// One frame of the player and the original AI cars (instead of the player-only
    /// scheduler step): the 64 Hz steps due after `dt`.
    pub(super) fn run_original_field(&mut self, dt: f32, word: u32) {
        let running = self.race_session.as_ref().is_none_or(|s| s.phase.is_racing());
        let (Some(player), Some(field)) = (self.original_vehicle.as_mut(), self.original_ai.as_mut())
        else {
            return;
        };
        field.running = running;
        let mut queue = Queue {
            surface: self.road_surface.as_ref(),
            body_owner: &mut self.body_support_owner,
            camera: &mut self.player_camera,
            half: player.half_extents,
        };
        queue.camera.advance(
            0.0,
            &nfs_assets::physics::original_vehicle::driver::camera_car(&player.car, player.half_extents),
            &mut nfs_assets::camera::FlatWorld,
        );
        // Previous and current 64 Hz states for drawing (A106).
        let sample = |player: &nfs_assets::physics::original_vehicle::OriginalVehicle,
                      camera: &nfs_assets::camera::PlayerCamera| {
            (car_pose(&player.car), camera.peek_view(player.car.position_330))
        };
        let mut states = self.drawn_states.take().unwrap_or_else(|| {
            let (pose, view) = sample(player, queue.camera);
            nfs_assets::render_interp::DrawnStates::new(pose, view)
        });
        let ai_poses = |field: &OriginalAiField| -> Vec<_> {
            field.cars.iter().map(|c| car_pose(&c.vehicle.car)).collect()
        };
        if self.drawn_ai.len() != field.cars.len() {
            self.drawn_ai = ai_poses(field).into_iter().map(|p| [p, p]).collect();
        }
        for _ in 0..player.clock.advance(dt) {
            field.step(player, word, &mut self.pair_globals, &mut queue);
            let (pose, view) = sample(player, queue.camera);
            states.push(pose, view);
            for (d, p) in self.drawn_ai.iter_mut().zip(ai_poses(field)) {
                *d = [d[1], p];
            }
        }
        states.alpha = player.clock.fraction();
        let alpha = states.alpha;
        self.drawn_states = Some(states);
        let blended: Vec<_> = self
            .drawn_ai
            .iter()
            .map(|d| nfs_assets::render_interp::blend_pose(&d[0], &d[1], alpha))
            .collect();
        self.mirror_original_ai(dt, Some(&blended));
    }

    /// JSON of the original AI cars (debugging / headless checks): node, laps,
    /// position, up · ground normal, speed, gear, gas, brake, steer.
    pub fn original_ai_json(&self) -> String {
        let Some(field) = &self.original_ai else {
            return "null".into();
        };
        let cars: Vec<String> = field
            .cars
            .iter()
            .map(|c| {
                let k = &c.vehicle.car;
                format!(
                    r#"{{"node":{},"laps":{},"pos":[{:.2},{:.2},{:.2}],"up":{:.2},"h":{:.2},"v":{:.2},"gear":{},"gas":{},"brake":{},"steer":{},"target":{:.1},"db0":{}}}"#,
                    c.ai.locator_08.node_00, c.laps, k.position_330[0], k.position_330[1],
                    k.position_330[2], k.ground_up_3a0, k.height_424, k.vel_ground_d58[2],
                    k.gear_d82, k.gas_eff_d7f, k.brake_eff_d80, k.steer_d94, c.ai.target_eb8,
                    k.start_db0
                )
            })
            .collect();
        format!(r#"{{"running":{},"cars":[{}]}}"#, field.running, cars.join(","))
    }

    /// Copies each AI car's pose, speed and controls into its `AiOpponent`.
    fn mirror_original_ai(&mut self, dt: f32, drawn: Option<&[nfs_assets::render_interp::CarPose]>) {
        use nfs_assets::physics::original_adapter::from_original;
        let (center, _) = self.collision_box_or_fallback();
        let Some(field) = &self.original_ai else {
            return;
        };
        for (i, (ai, c)) in self.ai_opponents.iter_mut().zip(&field.cars).enumerate() {
            let car = &c.vehicle.car;
            // Drawn pose: the blend of the last two 64 Hz states (A106).
            let pose = drawn.and_then(|d| d.get(i)).copied().unwrap_or_else(|| car_pose(car));
            let [r, u, f] = pose.axes.map(from_original);
            let q = Quat::from_mat3(&glam::Mat3::from_cols(r, u, -f)).normalize();
            ai.position = (from_original(pose.position) - q * center).to_array();
            ai.rotation_override = Some(glam::Mat3::from_quat(q));
            ai.velocity = from_original(car.velocity_33c).to_array();
            let fwd = -(q * Vec3::Z);
            ai.forward = fwd.to_array();
            ai.yaw = (-fwd.x).atan2(-fwd.z);
            ai.current_speed = car.vel_ground_d58[2];
            ai.wheel_distance += car.vel_ground_d58[2] * dt;
            ai.current_controls.throttle = car.gas_eff_d7f as f32 / 255.0;
            ai.current_controls.brake = car.brake_eff_d80 as f32 / 255.0;
            ai.current_controls.steer = (car.steer_d94 as f32 / 127.0).clamp(-1.0, 1.0);
            if let Some(course) = &self.track_course {
                ai.distance_along_course = course.distance_along_course(ai.position);
            }
            ai.laps_completed = c.laps.max(0) as u32;
        }
    }
}

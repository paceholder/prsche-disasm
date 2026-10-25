//! Front-end 3D rooms behind the menus (`room=` of a layout), see `docs/re/frontend.md`.
//!
//! The original draws the room model and the car with a fixed camera at the origin looking
//! along +z (original frame). Room and car share one position and one yaw that grows by one
//! turn every `spin` seconds, so the whole garage turns with the car [VA 0x4e9ff0].
use glam::{Mat4, Vec3};

/// Constants of one spinning room (`FESpinning3DRoom` fields +0x270..0x290).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpinningRoom {
    /// Model in `FEData/Models` (`<model>.crp` + `.tpg`).
    pub model: &'static str,
    /// Room and car position in the original frame (+0x278..0x280).
    pub position: [f32; 3],
    /// Seconds per turn (+0x270, "Spin").
    pub spin_seconds: f32,
    /// Horizontal FOV in degrees: twice the "V.A." half-angle at +0x290 [VA 0x467120].
    pub horizontal_fov_deg: f32,
}

/// `ID_GARAGE` [VA 0x4e9b30]: model "garage", (0, -1.1, 4.8), 30 s per turn, V.A. 37.5.
pub const GARAGE: SpinningRoom = SpinningRoom {
    model: "garage",
    position: [0.0, -1.1, 4.8],
    spin_seconds: 30.0,
    horizontal_fov_deg: 75.0,
};

/// `ID_FACTORY` [VA 0x4e9880]: same class with model "factory".
pub const FACTORY: SpinningRoom = SpinningRoom {
    model: "factory",
    position: [0.0, -1.1, 4.8],
    spin_seconds: 30.0,
    horizontal_fov_deg: 75.0,
};

/// Room by layout id (`room=`); `None` for rooms that are not spinning rooms (yet).
pub fn spinning_room(id: &str) -> Option<SpinningRoom> {
    match id.to_ascii_uppercase().as_str() {
        "ID_GARAGE" => Some(GARAGE),
        "ID_FACTORY" => Some(FACTORY),
        _ => None,
    }
}

/// The light shafts (`Lightcast01-04`, `light.fsh`) and the light pool (`GarageSpot`,
/// `spotlite.fsh`) carry 4444 alpha textures; the car loader makes every `CarInt` material
/// opaque. Give those meshes a blended copy of their material (approximation A15).
pub fn prepare_room_scene(scene: &mut nfs_assets::Scene) {
    for mesh in &mut scene.meshes {
        let name = mesh.name.to_ascii_lowercase();
        if !(name.starts_with("lightcast") || name.starts_with("garagespot")) {
            continue;
        }
        let Some(material) = scene.materials.get(mesh.material) else {
            continue;
        };
        let mut blended = material.clone();
        blended.alpha_mode = nfs_assets::AlphaMode::Blend;
        blended.double_sided = true;
        scene.materials.push(blended);
        mesh.material = scene.materials.len() - 1;
    }
}

/// Car-loader scale: `nfs_assets::load_car` divides raw CRP coordinates by five.
const LOADER_SCALE: f32 = 5.0;

/// Room state while a menu shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomView {
    pub room: SpinningRoom,
    /// Seconds since the room appeared (the original uses its 128 Hz tick counter).
    pub seconds: f64,
}

impl RoomView {
    pub fn new(room: SpinningRoom) -> Self {
        Self { room, seconds: 0.0 }
    }

    /// Yaw in turns, `ticks / (128 Hz * Spin)` [VA 0x4e9ff0], wrapped to 0..1.
    pub fn yaw_turns(&self) -> f32 {
        (self.seconds / self.room.spin_seconds as f64).fract() as f32
    }

    /// Rotation of the original yaw in the engine frame (original z mirrored).
    /// **Hypothesis:** 0x532600 builds the D3D-style row-vector Y rotation, which becomes a
    /// rotation by -angle once z is mirrored.
    fn rotation(&self) -> Mat4 {
        Mat4::from_rotation_y(-self.yaw_turns() * std::f32::consts::TAU)
    }

    fn position(&self) -> Vec3 {
        let p = self.room.position;
        Vec3::new(p[0], p[1], -p[2])
    }

    /// Room model: car-loader scene space (raw / 5, z mirrored) to engine world.
    pub fn room_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.position())
            * self.rotation()
            * Mat4::from_scale(Vec3::splat(LOADER_SCALE))
    }

    /// Car: race body space (raw x mirrored, see `Renderer::set_car_model`) to engine world.
    /// `lift` is the car's +0x3ac offset added by 0x4d1700 (`fe_room_car_lift`, A16).
    pub fn car_matrix(&self, lift: f32) -> Mat4 {
        Mat4::from_translation(self.position() + Vec3::Y * lift)
            * self.rotation()
            * Mat4::from_rotation_y(std::f32::consts::PI)
    }

    /// Camera at the origin looking along original +z (engine -z) [VA 0x4d3420 view setup].
    pub fn eye(&self) -> Vec3 {
        Vec3::ZERO
    }
    pub fn target(&self) -> Vec3 {
        Vec3::NEG_Z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_turn_per_thirty_seconds() {
        let mut view = RoomView::new(GARAGE);
        view.seconds = 7.5;
        assert!((view.yaw_turns() - 0.25).abs() < 1e-6);
        view.seconds = 37.5;
        assert!((view.yaw_turns() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn room_origin_sits_in_front_of_the_camera() {
        let view = RoomView::new(GARAGE);
        let origin = view.room_matrix().transform_point3(Vec3::ZERO);
        assert!((origin - Vec3::new(0.0, -1.1, -4.8)).length() < 1e-6);
        // A loader-space point 1 unit along +x is 5 m away from the room origin.
        let x = view.room_matrix().transform_point3(Vec3::X);
        assert!(((x - origin).length() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn car_and_room_turn_together() {
        let mut view = RoomView::new(GARAGE);
        view.seconds = 3.0;
        // Raw CRP point r: room scene space = (r.x, r.y, -r.z) / 5; car body = (-r.x, r.y, r.z).
        let r = Vec3::new(1.0, 0.5, 2.0);
        let room_space = Vec3::new(r.x, r.y, -r.z) / 5.0;
        let body = Vec3::new(-r.x, r.y, r.z);
        let a = view.room_matrix().transform_point3(room_space);
        let b = view.car_matrix(0.0).transform_point3(body);
        assert!((a - b).length() < 1e-5, "{a} vs {b}");
    }

    #[test]
    fn rooms_by_layout_id() {
        assert_eq!(spinning_room("ID_GARAGE"), Some(GARAGE));
        assert_eq!(spinning_room("id_factory").unwrap().model, "factory");
        assert_eq!(spinning_room("ID_NONE"), None);
    }
}

//! Vehicle physics simulation engine for NFS 5 Porsche Unleashed.
//!
//! Features:
//! - 6 DOF rigid body dynamics with quaternion integration
//! - Powertrain engine with 21-point torque curves interpolated on 500-RPM grid
//! - 4-wheel independent suspension with bump/rebound damping and anti-roll bars
//! - Friction ellipse tire slip model with Coulomb friction budget and `.sim` grip multipliers
//! - Road surface heightfield queries via `surface::RoadSurface`

pub mod chassis;
pub mod original_adapter;
pub mod original_body_contact;
pub mod original_contact;
pub mod original_pair;
pub mod original_scene_contact;
pub mod original_scene_objects;
pub mod original_spatial;
pub mod original_support;
pub mod original_support_assembly;
pub mod original_support_bounds;
pub mod original_support_loader;
pub mod original_support_material;
pub mod original_support_owner;
pub mod original_support_tree;
pub mod original_wheel_loop;
pub mod original_wheel_state;
pub mod powertrain;
pub mod rigid_body;
pub mod suspension;
pub mod tire;
pub mod vehicle;

pub use chassis::ChassisBounds;
pub use powertrain::Powertrain;
pub use rigid_body::RigidBody;
pub use suspension::{SuspensionSystem, SuspensionWheel, WHEEL_FL, WHEEL_FR, WHEEL_RL, WHEEL_RR};
pub use tire::Tire;
pub use vehicle::{VehicleControls, VehicleSimulation, VehicleTelemetry};

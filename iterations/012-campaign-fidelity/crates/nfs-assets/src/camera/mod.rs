//! Original NFS5 player camera without a renderer.
//!
//! Research notes: `docs/re/camera.md` in the research workspace.
pub mod original;
pub mod player;

pub use original::{
    build_view, look_at, update_bumper, update_bumper2, update_heli, update_tail, CameraGlobals,
    CameraParam, CameraRecord, CameraView, CameraWorld, CarView, DriverCamera, LookKeys, NodeStrip,
    CAMERA_PARAMS, CAMERA_PARAMS_VA, UPDATE_HZ,
};
pub use player::{camera_name, FlatWorld, PlayerCamera, CYCLE_LIST};

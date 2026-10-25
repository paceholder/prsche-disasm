//! Original NFS5 vehicle model from `nfs5.exe`, ported piece by piece.
//!
//! Source: `docs/re/vehicle.md` in the workspace root (call graph, SIM field
//! consumers, recommended split V1..V6). Each piece is verified against
//! outputs recorded by executing the original bytes in Unicorn
//! (`scripts/research/vehicle_harness.py` and one recorder per piece).
//!
//! Arithmetic follows the original x87 operation order: f64 stands in for
//! the x87 register stack and every original f32 store is rounded with
//! `as f32`. The recorders run with the control word the CRT start-up sets
//! (`_fpinit` `0x5a3dba`: 53-bit precision), where f64 reproduces the x87
//! results bit for bit.
//!
//! - [`sim`]: V1, SIM load scaling and derived fields (`0x49c6c0` tail,
//!   `0x49cb50`).
//! - [`tyre`]: V2, grip and tyre force incl. the handbrake slide
//!   (`0x4a1900`, `0x4a20d0`, `0x4a1c70`, `0x4a1dc0`).
//! - [`engine`]: V3, engine rpm, drive force, drag and the automatic
//!   gearbox (`0x49e440`, `0x49daf0`, `0x49d9b0` with no other cars).
//! - [`controls`]: V4, input word, steering rate, gear requests,
//!   auto-reverse and race-start gating (`0x4991e0`, `0x413890`,
//!   `0x49dd20`).
//! - [`car`]: [`OriginalCar`], the original car state (offset-named
//!   fields, wheel records, globals) the V5 step runs on.
//! - [`step`]: V5, the planar 32 Hz step and the 64 Hz integration
//!   (`0x49fd30`, `0x49f160`, `0x49f0c0`, `0x49ba30`, `0x49b7a0`,
//!   `0x49b8e0`) in the scheduler order ([`scheduler_step`], [`step_64hz`]).
//! - [`driver`]: the engine integration (model switch, 64 Hz clock, input
//!   word, collision view, [`OriginalVehicle`]).
//! - [`ground`]: the ground-contact adapter standing in for
//!   `Car_GroundContactTick` `0x49bd40` until V6 (approximation).

pub mod car;
pub mod controls;
pub mod driver;
pub mod engine;
pub mod ground;
pub mod sim;
pub mod step;
pub mod tyre;

pub use car::{OriginalCar, OriginalGlobals, OriginalWheel};
pub use controls::{apply_controls, gear_request, read_inputs, ControlsCar, ControlsEnv};
pub use driver::{input_word, OriginalVehicle, StepClock, VehicleModel};
pub use engine::{auto_gearbox, engine_drive_force, EngineCar, EngineEnv};
pub use ground::{FlatGround, GroundAdapter, GroundHit, GroundQuery, RoadGround};
pub use sim::{derive, CarSpecs, Tuning};
pub use step::{scheduler_step, step_64hz, GroundContact, Inputs, Phase, StepQueue};
pub use tyre::{grip_limits, tyre_force, TyreCar, TyreEnv, WheelRecord};

//! Race state machine, course tracking, checkpoints, barrier collisions, and AI.

pub mod ai;
pub mod collision;
pub mod course;
pub mod original_collision;
pub mod state;

pub use ai::{calculate_grid_slot, AiOpponent, AiPerformance, AiProfile};
pub use collision::{BarrierCollider, BarrierCollisionConfig, BarrierHit};
pub use course::{CheckpointGate, CourseProgressTracker, CourseWaypoint, TrackCourse};
pub use state::{CountdownCue, LapTracker, RaceParticipant, RacePhase, RaceSession};

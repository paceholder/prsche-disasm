//! The original AI drivers (change `original-ai`, `docs/re/ai.md` in the
//! workspace root): the track node graph `[0x628bc0]`, the racing line
//! `[0x5e671c]`, the AI car data and the planners `AI_TargetSpeed` 0x40b650
//! and `AI_Drive` 0x40bfe0.

pub mod ais;
pub mod query;
pub mod driver;
pub mod field;
pub mod planner;
pub mod racing_line;
pub mod track;

pub use query::TrackLocator;
pub use racing_line::RacingLine;
pub use track::{Junction, Section, SectionEnd, TrackNode, TrackNodes};

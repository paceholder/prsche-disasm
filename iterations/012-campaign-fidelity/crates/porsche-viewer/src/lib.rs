pub mod arcade;
pub mod car_mesh;
mod effects;
mod in_car;
mod shade_sample;
pub mod engine_audio;
pub mod tyre_audio;
pub mod game_audio;
pub mod fe_room;
#[cfg(not(target_arch = "wasm32"))]
pub mod native_catalog;
mod renderer;
pub use renderer::{Camera, Renderer, instance, request_gpu, summarize};

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
mod browser_fe;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

pub mod arcade;
pub mod car_mesh;
pub mod engine_audio;
#[cfg(not(target_arch = "wasm32"))]
pub mod native_catalog;
mod renderer;
pub use renderer::{Camera, Renderer, instance, request_gpu, summarize};

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

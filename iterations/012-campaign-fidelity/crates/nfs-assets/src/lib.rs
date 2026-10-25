//! GPU-independent scene contract shared by native and browser frontends.
use std::collections::BTreeMap;

pub mod audio;
pub mod camera;
pub mod collision_box;
pub mod lights;
pub mod physics;
pub mod race;
pub mod surface;
pub use physics::{VehicleControls, VehicleSimulation, VehicleTelemetry};
pub use race::{
    calculate_grid_slot, AiOpponent, AiPerformance, AiProfile, BarrierCollider, BarrierCollisionConfig,
    BarrierHit, CheckpointGate, CountdownCue, CourseProgressTracker, CourseWaypoint, LapTracker,
    RaceParticipant, RacePhase, RaceSession, TrackCourse,
};
pub use surface::{RoadSurface, RoadSurfaceReport, RoadTriangle, RoadTriangleIdentity, SurfaceHit};

#[derive(Debug, Clone)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Vertex colour, sRGB-encoded RGBA (track `df` prelit colours; white elsewhere).
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: usize,
}

/// A car light glare: a `Glare*` CRP article with an `ef` record of type 5
/// (`R3DCar_DrawGlares` 0x43d800, `docs/re/lighting.md`). Mirrored copies
/// (`ef+0x50 & 8`) are separate entries in their own group.
/// Coordinates are driving body meters like [`CarWheel`]: [-raw X, raw Y, raw Z].
#[derive(Debug, Clone, PartialEq)]
pub struct CarGlare {
    pub name: String,
    /// `ef+0x08`, plus the article's `tr` translation if it has one.
    pub position: [f32; 3],
    /// Sprite size axis (`ef+0x18`): the half size is `|A|·intensity`.
    pub axis_a: [f32; 3],
    /// Stretch axis (`ef+0x28`).
    pub axis_b: [f32; 3],
    /// Unit facing direction, the original's `B x A` converted to body space.
    pub normal: [f32; 3],
    /// ARGB colours of the two sprite passes (`ef+0x48`, `ef+0x4c`).
    pub colors: [u32; 2],
    /// Glare type (`ef+0x54`), index into the original type table 0x5cd73c.
    pub kind: u8,
    /// Light group (`Base+0x50`, or `ef+0x56` for the mirrored copy).
    pub group: u8,
    /// Headlight beam / lens flare flag (`ef+0x57`).
    pub beam: bool,
    /// Animated part the glare follows (`Base+0x4b`: 6/7 doors, 8 hood), 0 = body.
    pub pivot: u8,
}

/// Selected CRP wheel article, shared by vehicle geometry and articulation.
/// Coordinates are driving body meters: [-raw X, raw Y, raw Z], front faces -Z.
#[derive(Debug, Clone)]
pub struct CarWheel {
    /// FL, FR, RL, RR = 0, 1, 2, 3.
    pub wheel_index: usize,
    pub center: [f32; 3],
    pub radius: f32,
    pub mesh_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub texture: Option<usize>,
    pub base_color: [f32; 4],
    pub paintable: bool,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// Signed material depth offset; separates coplanar detail surfaces.
    pub depth_bias: i32,
    /// Shaded by its vertex colours only (prelit track geometry), not by the car lights.
    pub prelit: bool,
}

/// A library prop template extracted from the track CRP (articles with `Base & 0x8000`).
#[derive(Debug, Clone)]
pub struct PropArticle {
    /// 4-byte FourCC identifier from `Base:0` at offset 0x44 (e.g. `CONE`, `ARW1`, `SAW1`).
    pub fourcc: u32,
    /// Range of mesh indices in `Scene::meshes` belonging to this prop.
    pub mesh_range: std::ops::Range<usize>,
}

/// A placed prop instance from a `.scn` scenario file.
#[derive(Debug, Clone)]
pub struct PropInstance {
    /// Index into `Scene::prop_articles`.
    pub article_index: usize,
    /// World position in scene coordinates (already transformed to `[x, y, -z]`).
    pub position: [f32; 3],
    /// 3×3 orientation matrix (row-major) from the `.scn` file.
    pub rotation: [[f32; 3]; 3],
    /// animdefs.txt property; separate from the SCN geometry category.
    pub triggerable: bool,
}

/// Sky panorama texture loaded from `Sky/<track>.fsh`.
#[derive(Debug, Clone)]
pub struct SkyTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A 3D edge segment in scene coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TopologyEdge {
    pub flags: u8,
    pub p1: [f32; 3],
    pub p2: [f32; 3],
}

/// A 3D polyline representing road boundaries or splines in scene coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TopologyLine {
    pub points: Vec<[f32; 3]>,
    pub color: [f32; 4],
}

/// Track topology representation including road boundaries, junctions, and slice mapping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackTopology {
    /// 3D edge segments in scene coordinates `[x, y, -z]`.
    pub edges: Vec<TopologyEdge>,
    /// Continuous boundary polylines assembled from connected edges.
    pub boundary_lines: Vec<TopologyLine>,
    /// Junction connection nodes (from `.jnc`).
    pub junctions: Vec<nfs_formats::JunctionRecord>,
    /// Track map metadata (from `.map`, if present).
    pub map: Option<nfs_formats::TrackMap>,
    /// Line spline path waypoints (from `.lsp`, if present).
    pub spline: Option<nfs_formats::LineSplinePath>,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub meshes: Vec<Mesh>,
    /// Original collision box (CRP level 5) of a car scene; `None` for tracks.
    pub car_collision_box: Option<collision_box::CarCollisionBox>,
    /// Cockpit camera from the car `.tpg` (`drivercampos[styleN]`/`drivercamrot[styleN]`,
    /// defaults applied); `None` for tracks and synthetic scenes.
    pub car_driver_camera: Option<camera::DriverCamera>,
    pub car_wheels: Vec<CarWheel>,
    /// Light glares of a car scene (empty for tracks).
    pub car_glares: Vec<CarGlare>,
    /// Car effect textures from `GameData/Render/common.fsh`, named by their
    /// FSH entry (`CARG`, `SPIK`, `GLNT`, `SHAD`, ...); empty if the file is absent.
    pub effect_textures: Vec<Texture>,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub bounds: [[f32; 3]; 2],
    pub diagnostics: Vec<String>,
    /// Library prop templates (mesh geometry centered at origin).
    pub prop_articles: Vec<PropArticle>,
    /// Placed prop instances from `.scn` scenario files.
    pub prop_instances: Vec<PropInstance>,
    /// Scenario Start trigger center and direction, when a selected `.scn` supplies one.
    /// The original vehicle staging offset has not been recovered.
    pub scenario_start: Option<ScenarioStart>,
    /// Scenario End trigger center, when supplied. This alone is not a mission win rule.
    pub scenario_end: Option<[f32; 3]>,
    /// Selected SCN filename, for mission-specific interpretation.
    pub scenario_name: Option<String>,
    /// Ordered trigger footprints from the selected SCN.
    pub scenario_gates: Vec<ScenarioGate>,
    /// Sky horizon panorama texture, if available.
    pub sky_texture: Option<SkyTexture>,
    /// Sky layers of the sky `.fsh` behind the horizon: sky map (`st1a`), cloud map
    /// (`sc1a`) and horizon haze (`hzcl`), each if present (approximation A4).
    pub sky_layers: [Option<SkyTexture>; 3],
    /// Sun position of `Sky/<track>.bin` in scene coordinates (`Sky_SunPosition`
    /// 0x461090); its direction lights the cars.
    pub sun_position: Option<[f32; 3]>,
    /// Track topology (boundaries, junctions, map), if available.
    pub topology: Option<TrackTopology>,
    /// CPU-only support grid constructed from static road candidate geometry.
    ///
    /// This is a geometric road-surface hypothesis, not a proven original collision contract.
    pub road_surface: Option<RoadSurface>,
    /// Track course (checkpoints, waypoints, racing line), if available.
    pub course: Option<TrackCourse>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            car_collision_box: None,
            car_driver_camera: None,
            meshes: Vec::new(),
            car_wheels: Vec::new(),
            car_glares: Vec::new(),
            effect_textures: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            bounds: [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
            diagnostics: Vec::new(),
            prop_articles: Vec::new(),
            prop_instances: Vec::new(),
            scenario_start: None,
            scenario_end: None,
            scenario_name: None,
            scenario_gates: Vec::new(),
            sky_texture: None,
            sky_layers: [None, None, None],
            sun_position: None,
            topology: None,
            road_surface: None,
            course: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenarioStart {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub linked_prop: Option<scenario::PropTarget>,
}

/// Source trigger conditions. Shape-1 geometry/constraints follow the EXE;
/// the vehicle input adapter and complete mission result flow remain under audit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenarioGate {
    pub sequence: u32,
    pub center: [f32; 3],
    pub segment: [[f32; 3]; 2],
    pub width: f32,
    pub forward: [f32; 3],
    pub velocity_direction: [f32; 3],
    pub shape: u32,
    pub speed_range: [f32; 2],
    pub is_end: bool,
    pub linked_prop: Option<scenario::PropTarget>,
}

pub mod scenario;

/// Keys are case-insensitive slash-separated paths or bare resource filenames.
pub type AssetFiles = BTreeMap<String, Vec<u8>>;

/// Load a car from user-selected bytes. Native and WASM call the same implementation.
pub fn load_car(files: &AssetFiles, car: &str) -> Result<Scene, String> {
    loader::load(files, car).map_err(|e| format!("{car}: {e}"))
}

/// Load a track from user-selected bytes. Native and WASM call the same implementation.
pub fn load_track(files: &AssetFiles, track: &str) -> Result<Scene, String> {
    track_loader::load(files, track).map_err(|e| format!("{track}: {e}"))
}

mod loader;
pub mod track_loader;

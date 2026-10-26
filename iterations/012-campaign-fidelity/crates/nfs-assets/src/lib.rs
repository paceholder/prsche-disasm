//! GPU-independent scene contract shared by native and browser frontends.
use std::collections::BTreeMap;

pub mod audio;
pub mod camera;
pub mod cockpit;
pub mod collision_box;
pub mod lights;
pub mod sparks;
pub mod physics;
pub mod race;
pub mod render_interp;
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

/// Pass of the in-car view (`R3DObject_DrawPasses` 0x444e00): part mask and CRP level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InCarPass {
    /// Mask 0x10 (body and interior) at level 2.
    Body,
    /// Mask 0x40 (steering wheel) at level 1.
    SteeringWheel,
    /// Mask 0x80 (driver; only the hands have the in-car flag) at level 1, skipped while a
    /// look key is held.
    Driver,
}

impl InCarPass {
    pub const ALL: [InCarPass; 3] = [InCarPass::Body, InCarPass::SteeringWheel, InCarPass::Driver];
    pub fn mask(self) -> u8 {
        match self {
            InCarPass::Body => 0x10,
            InCarPass::SteeringWheel => 0x40,
            InCarPass::Driver => 0x80,
        }
    }
    /// CRP level: LOD index + 1 (`R3DObject_BuildLODs` 0x4425b0). Driver: car detail 2
    /// (`[0x5cdb1c]` = {1, 0, 0}, LOD 0).
    pub fn level(self) -> u16 {
        match self {
            InCarPass::Body => 2,
            InCarPass::SteeringWheel | InCarPass::Driver => 1,
        }
    }
}

/// One drawn piece (`pr`) of an in-car part. Coordinates are scene coordinates like
/// [`Mesh`]; the part's `tr` frame is already applied.
#[derive(Debug, Clone)]
pub struct InCarMesh {
    pub mesh: Mesh,
    /// Positions and normals per animation frame (`vt`/`nm` index `(frame << 4) | level`),
    /// in the vertex order of `mesh`; empty for parts without frames.
    pub frames: Vec<Vec<([f32; 3], [f32; 3])>>,
}

/// A CRP part drawn in the in-car view (`Base+0x44` byte 0xd set; `R3DCarLOD_DrawParts`
/// 0x43c830). See `docs/re/cockpit.md`.
#[derive(Debug, Clone)]
pub struct InCarPart {
    pub name: String,
    pub pass: InCarPass,
    /// Transform channel `Base+0x4b` (5 steering wheel, 0xb rev needle, 0xc speed needle,
    /// 6..9 doors/hood/trunk, 0 none).
    pub channel: u8,
    /// Pivot of the channel rotation: the `tr` origin and its local z axis (unit), in scene
    /// coordinates. The rotation is about local z (0x43c830 order x, y, z; only z is driven).
    pub pivot_origin: [f32; 3],
    pub pivot_axis: [f32; 3],
    /// Frame channel `Base+0x4d` and frame count `Base+0x4c`.
    pub frame_channel: u8,
    pub frame_count: u8,
    pub meshes: Vec<InCarMesh>,
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
    /// `LayerUVAnimate` mode 0: the texture V scrolls by `frac(t * speed * 0.0001)`, `t` in
    /// 128 Hz ticks, `speed` signed (`FUN_00448cc0`, `docs/re/water.md` section 4).
    pub uv_scroll: Option<i32>,
}

/// Game timer rate of `TIMER_init(0x80)` [VA 0x4a5410, 0x565030].
pub const GAME_TIMER_HZ: f64 = 128.0;

/// V offset of a `LayerUVAnimate` mode-0 material after `seconds` [VA 0x448cc0].
pub fn uv_scroll_offset(speed: i32, seconds: f64) -> f32 {
    let x = seconds * GAME_TIMER_HZ * f64::from(speed) * 0.0001;
    (x - x.floor()) as f32
}

/// A library prop template extracted from the track CRP (articles with `Base & 0x8000`).
#[derive(Debug, Clone)]
pub struct PropArticle {
    /// 4-byte FourCC identifier from `Base:0` at offset 0x44 (e.g. `CONE`, `ARW1`, `SAW1`).
    pub fourcc: u32,
    /// Range of mesh indices in `Scene::meshes` belonging to this prop.
    pub mesh_range: std::ops::Range<usize>,
    /// Baked keyframes of the article's `Anqt` entry (original coordinates),
    /// empty when the article has none. Used by smashed crates.
    pub frames: Vec<PropFrame>,
}

/// One 52-byte `Anqt` frame: quaternion (x, y, z, w) and position in the
/// original coordinates (`docs/re/track-objects.md` section 6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropFrame {
    pub rotation: [f32; 4],
    pub position: [f32; 3],
}

/// A smackable article (sign, cone, post, bin): a flag-0x80 object car in the
/// original (`physics::original_smackable`). Its meshes are kept out of the
/// static scene; the renderer draws them at the car's pose.
#[derive(Debug, Clone)]
pub struct SmackProp {
    pub record: physics::original_smackable::SmackRecord,
    /// Meshes in `Scene::meshes` (vertices in scene coordinates at the start pose).
    pub meshes: std::ops::Range<usize>,
}

/// A placed article that a car can smash (a definition with pieces, such as
/// `BBOX`): its intact meshes are kept out of the static scene.
#[derive(Debug, Clone)]
pub struct Smashable {
    /// Index into `Scene::track_objects` (collision object of the same article).
    pub object_index: usize,
    pub definition: nfs_formats::animdefs::AnimDefinition,
    /// Intact meshes in `Scene::meshes` (vertices already in scene coordinates).
    pub intact_meshes: std::ops::Range<usize>,
    /// The article's `tr` matrix (row vectors, original coordinates).
    pub placement: [f32; 16],
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

/// A part of the exterior that the front end moves: the cabrio top (frame channel 1, morph
/// frames `vt` index `lod | frame << 4`), the hood (transform channel 8) or the trunk lid (9)
/// [VA 0x438440, 0x43c830; `docs/re/frontend.md` "Clicking the car"].
#[derive(Debug, Clone)]
pub struct CarAnimPart {
    /// 1 = cabrio top frame channel, 6/7 = left/right door, 8 = hood, 9 = trunk lid (the `Base+0x44` bytes 9 / 7).
    pub channel: u8,
    /// Hinge: the article's `tr` translation in driving body coordinates ([-x, y, z] raw).
    pub pivot: [f32; 3],
    /// Rotation axis of the opening: 0 = body x (hood, trunk lid), 1 = body y (doors).
    pub axis: u8,
    /// +1 / -1: the sign of the rotation that lifts the lid / swings the door outward
    /// (approximation A232: the original hinge direction is not read).
    pub open_sign: f32,
    /// Opening angle in turns: TPG `hoodextent` (default 0.15) / `trunkextent` (default -0.15),
    /// magnitude; 0.15 for the doors (`0x4c0520`: level 255 * 0.000588); 1.0 for the cabrio top
    /// (whole frame range).
    pub extent: f32,
    /// Indices into `Scene::meshes` of frame 0 / the closed part.
    pub base_meshes: Vec<usize>,
    /// Frames 1.. of a morphing part (cabrio top), one mesh per base mesh, scene coordinates.
    pub frames: Vec<Vec<Mesh>>,
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
    /// Parts of the in-car view (BumperCam2) of a car scene; empty for tracks.
    pub car_in_car: Vec<InCarPart>,
    /// Hood, trunk lid and cabrio top of a car scene (front-end clicks).
    pub car_anim_parts: Vec<CarAnimPart>,
    /// Cabrio top at rest, closed: indices of the `CabrioUp*` meshes (geometry 41, odd types) in
    /// `meshes`; the matching `CabrioDown*` meshes (even types) are kept out of `meshes`.
    pub car_roof_up: Vec<usize>,
    pub car_roof_down: Vec<Mesh>,
    /// Car effect textures from `GameData/Render/common.fsh`, named by their
    /// FSH entry (`CARG`, `SPIK`, `GLNT`, `SHAD`, ...); empty if the file is absent.
    pub effect_textures: Vec<Texture>,
    /// Car particle systems of `GameData/Render/carpart.dat` (sparks, smoke, dirt).
    pub particle_defs: Vec<sparks::ParticleDef>,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub bounds: [[f32; 3]; 2],
    pub diagnostics: Vec<String>,
    /// Library prop templates (mesh geometry centered at origin).
    pub prop_articles: Vec<PropArticle>,
    /// Placed prop instances from `.scn` scenario files.
    pub prop_instances: Vec<PropInstance>,
    /// Collision objects (cylinders and boxes) of the track, original coordinates.
    pub track_objects: Vec<physics::original_track_object_build::TrackObject>,
    /// Smashable crates, `Smashable::object_index` into `track_objects`.
    pub smashables: Vec<Smashable>,
    /// Smackable objects (signs, cones, posts).
    pub smack_props: Vec<SmackProp>,
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
    /// Placement of the horizon panorama from `Sky/<track>.bin` (`SkyHorizon_Build` 0x459d30).
    pub sky_horizon: Option<nfs_formats::SkyHorizon>,
    /// Dome of the sky-map layers (`Sky/<track>.bin`, `docs/re/sky.md`).
    pub sky_dome: Option<nfs_formats::SkyDome>,
    /// Layer table of the `HIGH` block of `Sky/<track>.txt` (maps are `sky_layers[0..2]`).
    pub sky_map: Option<nfs_formats::SkyMapBlock>,
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
    /// The original AI's track node graph and racing line (`SimD`/`SimT`,
    /// `.jnc`, `<track>0.lsp`), if available.
    pub ai_track: Option<race::original_ai::field::OriginalAiTrack>,
    /// Start line of the race direction: the type-2 trigger of `<track>_FStart.scn` /
    /// `_BStart.scn` (`race::original_ai::start`), original coordinates.
    pub start_trigger: Option<[f32; 3]>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            car_collision_box: None,
            car_driver_camera: None,
            meshes: Vec::new(),
            car_wheels: Vec::new(),
            car_glares: Vec::new(),
            car_in_car: Vec::new(),
            car_anim_parts: Vec::new(),
            car_roof_up: Vec::new(),
            car_roof_down: Vec::new(),
            effect_textures: Vec::new(),
            particle_defs: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            bounds: [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
            diagnostics: Vec::new(),
            prop_articles: Vec::new(),
            prop_instances: Vec::new(),
            track_objects: Vec::new(),
            smashables: Vec::new(),
            smack_props: Vec::new(),
            scenario_start: None,
            scenario_end: None,
            scenario_name: None,
            scenario_gates: Vec::new(),
            sky_texture: None,
            sky_layers: [None, None, None],
            sky_horizon: None,
            sky_dome: None,
            sky_map: None,
            sun_position: None,
            topology: None,
            road_surface: None,
            course: None,
            ai_track: None,
            start_trigger: None,
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
    load_car_style(files, car, 0)
}

/// Car with TPG style `style` (catalogue record i16 at `+0x1c0`, `docs/formats/car-catalogue.md`).
pub fn load_car_style(files: &AssetFiles, car: &str, style: u32) -> Result<Scene, String> {
    loader::load(files, car, style).map_err(|e| format!("{car}: {e}"))
}

/// Load a track from user-selected bytes. Native and WASM call the same implementation.
pub fn load_track(files: &AssetFiles, track: &str) -> Result<Scene, String> {
    track_loader::load(files, track).map_err(|e| format!("{track}: {e}"))
}

/// `load_track` for a race direction (`reverse`: the backward race, `docs/re/frontend.md`).
pub fn load_track_direction(files: &AssetFiles, track: &str, reverse: bool) -> Result<Scene, String> {
    track_loader::load_direction(files, track, reverse).map_err(|e| format!("{track}: {e}"))
}

mod loader;
pub mod track_loader;

#[cfg(test)]
mod uv_tests {
    use super::*;

    #[test]
    fn uv_scroll_wraps_and_follows_the_sign() {
        // 128 ticks/s * 70 * 0.0001 = 0.896 per second.
        assert!((uv_scroll_offset(70, 1.0) - 0.896).abs() < 1e-6);
        assert!((uv_scroll_offset(70, 2.0) - 0.792).abs() < 1e-6);
        // Negative speed wraps into 0..1 the other way.
        assert!((uv_scroll_offset(-70, 1.0) - 0.104).abs() < 1e-6);
        assert_eq!(uv_scroll_offset(0, 5.0), 0.0);
    }
}

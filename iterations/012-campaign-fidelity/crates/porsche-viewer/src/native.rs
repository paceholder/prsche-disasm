#![allow(clippy::items_after_test_module)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::BufWriter,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use nfs_assets::{AssetFiles, Scene, load_car, load_track};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    Renderer, instance,
    native_catalog::{NativeCatalog, NativeTargetKind},
    request_gpu, summarize,
};

pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut command = None;
    let mut game_dir = None;
    let mut car = None;
    let mut track = None;
    let mut screenshot = None;
    let mut yaw = None;
    let mut pitch = None;
    let mut distance = None;
    let mut center_x = None;
    let mut center_y = None;
    let mut center_z = None;
    let mut paint_index = 0;
    let mut width = 1280;
    let mut height = 720;
    let mut race = RaceShot::default();

    let mut iter = args.into_iter();
    let mut saw_args = false;
    while let Some(arg) = iter.next() {
        saw_args = true;
        match arg.as_str() {
            "inspect" | "view" | "catalog" => command = Some(arg),
            "--game-dir" => game_dir = Some(PathBuf::from(next_value(&mut iter, "--game-dir")?)),
            "--car" => car = Some(next_value(&mut iter, "--car")?),
            "--track" => track = Some(next_value(&mut iter, "--track")?),
            "--screenshot" => {
                screenshot = Some(PathBuf::from(next_value(&mut iter, "--screenshot")?))
            }
            "--yaw" => yaw = Some(parse_f32(&mut iter, "--yaw")?),
            "--pitch" => pitch = Some(parse_f32(&mut iter, "--pitch")?),
            "--distance" => distance = Some(parse_f32(&mut iter, "--distance")?),
            "--center-x" => center_x = Some(parse_f32(&mut iter, "--center-x")?),
            "--center-y" => center_y = Some(parse_f32(&mut iter, "--center-y")?),
            "--center-z" => center_z = Some(parse_f32(&mut iter, "--center-z")?),
            "--paint-index" => {
                paint_index = next_value(&mut iter, "--paint-index")?
                    .parse::<u32>()
                    .map_err(|_| "--paint-index requires an integer 0..5")?;
                if paint_index > 5 {
                    return Err("--paint-index must be 0..5".into());
                }
            }
            "--race-car" => race.car = Some(next_value(&mut iter, "--race-car")?),
            "--pedal" => race.pedal = parse_f32(&mut iter, "--pedal")?,
            "--steer" => race.steer = parse_f32(&mut iter, "--steer")?,
            "--seconds" => race.seconds = parse_f32(&mut iter, "--seconds")?,
            "--lights" => race.lights = true,
            "--orbit-yaw" => race.orbit_yaw = Some(parse_f32(&mut iter, "--orbit-yaw")?),
            "--orbit-pitch" => race.orbit_pitch = Some(parse_f32(&mut iter, "--orbit-pitch")?),
            "--camera-cycles" => race.camera_cycles = parse_u32(&mut iter, "--camera-cycles")?,
            "--brake-last" => race.brake_last = parse_f32(&mut iter, "--brake-last")?,
            "--original-model" => race.original_model = true,
            "--opponents" => race.opponents = parse_u32(&mut iter, "--opponents")?,
            "--width" => width = parse_u32(&mut iter, "--width")?,
            "--height" => height = parse_u32(&mut iter, "--height")?,
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(());
            }
            other => return Err(format!("Unknown argument: {other}\n\n{}", usage())),
        }
    }

    if !saw_args {
        return run_catalog_window(NativeCatalog::discover_default()?);
    }

    let command = command.ok_or_else(usage)?;
    if command == "catalog" {
        if car.is_some() || track.is_some() || screenshot.is_some() {
            return Err("catalog accepts --game-dir; use view for a fixed resource".into());
        }
        let catalog = match game_dir {
            Some(path) => NativeCatalog::from_game_dir(path)?,
            None => NativeCatalog::discover_default()?,
        };
        return run_catalog_window(catalog);
    }
    if car.is_some() && track.is_some() {
        return Err("Cannot specify both --car and --track".into());
    }
    if paint_index != 0 && (command != "view" || screenshot.is_none() || track.is_some()) {
        return Err("--paint-index requires car view --screenshot".into());
    }
    let game_dir = game_dir.ok_or_else(|| format!("Missing --game-dir\n\n{}", usage()))?;
    let (scene, title) = match (car, track) {
        (Some(c), None) => {
            let files = read_game_files(&game_dir, &c)?;
            (load_car(&files, &c)?, c)
        }
        (None, Some(t)) => {
            let files = read_track_files(&game_dir, &t)?;
            (load_track(&files, &t)?, format!("Track {t}"))
        }
        (None, None) => {
            let default_car = String::from("356a");
            let files = read_game_files(&game_dir, &default_car)?;
            (load_car(&files, &default_car)?, default_car)
        }
        _ => unreachable!(),
    };

    match command.as_str() {
        "inspect" => {
            println!("{}", summarize(&scene));
            Ok(())
        }
        "view" => {
            if let (Some(path), Some(_)) = (&screenshot, &race.car) {
                return pollster::block_on(render_race_screenshot(
                    &scene, &game_dir, &race, width, height, path,
                ));
            }
            if let Some(path) = screenshot {
                let center = match (center_x, center_y, center_z) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                };
                pollster::block_on(render_screenshot(
                    &scene,
                    width,
                    height,
                    yaw,
                    pitch,
                    distance,
                    center,
                    paint_index,
                    &path,
                ))
            } else {
                let center = match (center_x, center_y, center_z) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                };
                run_window(scene, &title, yaw, pitch, distance, center)
            }
        }
        _ => unreachable!(),
    }
}

/// `--race-car`: a track screenshot from the drive camera with that car on the grid.
#[derive(Default)]
struct RaceShot {
    car: Option<String>,
    /// Signed pedal held during the run (negative brakes).
    pedal: f32,
    /// Steering held during the run (−1..1).
    steer: f32,
    /// Simulated time before the shot.
    seconds: f32,
    /// Light switch on.
    lights: bool,
    /// Brake fully for the last seconds of the run.
    brake_last: f32,
    /// Press the camera key this many times (C) before the run.
    camera_cycles: u32,
    /// After the run, look at the car from an orbit camera at this yaw (radians).
    orbit_yaw: Option<f32>,
    /// Orbit camera pitch (radians, default 0.45; negative looks up past the car).
    orbit_pitch: Option<f32>,
    opponents: u32,
    /// Drive with the original vehicle model (key O).
    original_model: bool,
}

async fn render_race_screenshot(
    track: &Scene,
    game_dir: &Path,
    race: &RaceShot,
    width: u32,
    height: u32,
    output: &Path,
) -> Result<(), String> {
    let car = race.car.as_deref().unwrap_or("356a");
    let files = read_game_files(game_dir, car)?;
    let car_scene = load_car(&files, car)?;
    let instance = instance();
    let (_, device, queue) = request_gpu(&instance, None).await?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(device, queue, format, width, height, track)?;
    renderer.set_car_model(&car_scene, car)?;
    if let Some(gauges) = car_gauges(game_dir, car) {
        renderer.set_car_gauges(gauges);
    }
    if race.original_model {
        renderer.toggle_vehicle_model();
    }
    if race.opponents > 0 {
        renderer.configure_quick_race(1, race.opponents)?;
    }
    if !renderer.is_drive_mode() {
        renderer.toggle_camera_mode();
    }
    renderer.restart_race();
    renderer.light_switch = race.lights;
    let dt = 1.0 / 60.0;
    let steps = (race.seconds / dt).round().max(1.0) as u32;
    let brake_steps = (race.brake_last / dt).round() as u32;
    for step in 0..steps {
        let pedal = if step + brake_steps >= steps { -1.0 } else { race.pedal };
        renderer.update_car(dt, pedal, race.steer, false);
        // The C key is ignored during the race intro (AnimCam): cycle once it has ended.
        if step == 0 {
            for _ in 0..race.camera_cycles {
                renderer.cycle_car_view();
            }
        }
    }
    if let Some(yaw) = race.orbit_yaw {
        renderer.toggle_camera_mode();
        renderer.camera.yaw = yaw;
        renderer.camera.pitch = race.orbit_pitch.unwrap_or(0.45);
        renderer.camera.distance = 7.0;
    }
    let pixels = capture_pixels(&mut renderer, width, height)?;
    let file = fs::File::create(output).map_err(|e| format!("{}: {e}", output.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| format!("PNG header failed: {e}"))?
        .write_image_data(&pixels)
        .map_err(|e| format!("PNG write failed: {e}"))?;
    Ok(())
}

fn usage() -> String {
    "Usage:\n  porsche-viewer\n  porsche-viewer inspect --game-dir <path> (--car <name> | --track <name>)\n  porsche-viewer view --game-dir <path> (--car <name> | --track <name>) [--screenshot out.png] [--paint-index 0..5 (car screenshot only)] [--yaw radians] [--pitch radians] [--width px] [--height px]\n  porsche-viewer view --game-dir <path> --track <name> --race-car <car> --screenshot out.png [--pedal -1..1] [--seconds s] [--brake-last s] [--lights] [--opponents 0|3] [--camera-cycles n] [--orbit-yaw radians] [--orbit-pitch radians] [--steer -1..1] [--original-model]".into()
}

fn next_value(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn compact_title_text(text: &str, max_chars: usize) -> String {
    let mut compact = text.replace(['\r', '\n', '\t'], " ");
    if compact.chars().count() > max_chars {
        compact = compact.chars().take(max_chars.saturating_sub(3)).collect();
        compact.push_str("...");
    }
    compact
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DriveKey {
    Forward,
    Reverse,
    Left,
    Right,
    Handbrake,
}

#[derive(Debug, Clone, Default)]
struct DriveInputState {
    pressed: BTreeSet<DriveKey>,
}

impl DriveInputState {
    fn set(&mut self, key: DriveKey, pressed: bool) {
        if pressed {
            self.pressed.insert(key);
        } else {
            self.pressed.remove(&key);
        }
    }

    fn clear(&mut self) {
        self.pressed.clear();
    }

    fn controls(&self, throttle_multiplier: f32) -> (f32, f32, bool) {
        let mut throttle = 0.0;
        if self.pressed.contains(&DriveKey::Forward) {
            throttle += throttle_multiplier;
        }
        if self.pressed.contains(&DriveKey::Reverse) {
            throttle -= throttle_multiplier;
        }

        let mut steering = 0.0;
        if self.pressed.contains(&DriveKey::Right) {
            steering += 1.0;
        }
        if self.pressed.contains(&DriveKey::Left) {
            steering -= 1.0;
        }

        (
            throttle,
            steering,
            self.pressed.contains(&DriveKey::Handbrake),
        )
    }
}

fn drive_key(code: KeyCode) -> Option<DriveKey> {
    match code {
        KeyCode::KeyW | KeyCode::ArrowUp => Some(DriveKey::Forward),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(DriveKey::Reverse),
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(DriveKey::Left),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(DriveKey::Right),
        KeyCode::Space => Some(DriveKey::Handbrake),
        _ => None,
    }
}

#[cfg(test)]
mod native_input_tests {
    use super::*;

    #[test]
    fn physical_key_mapping_preserves_drive_directions_and_release() {
        for (code, expected) in [
            (KeyCode::KeyA, (0.0, -1.0, false)),
            (KeyCode::ArrowLeft, (0.0, -1.0, false)),
            (KeyCode::KeyD, (0.0, 1.0, false)),
            (KeyCode::ArrowRight, (0.0, 1.0, false)),
            (KeyCode::KeyW, (1.0, 0.0, false)),
            (KeyCode::ArrowUp, (1.0, 0.0, false)),
            (KeyCode::KeyS, (-1.0, 0.0, false)),
            (KeyCode::ArrowDown, (-1.0, 0.0, false)),
            (KeyCode::Space, (0.0, 0.0, true)),
        ] {
            let mut input = DriveInputState::default();
            let mapped = drive_key(code).unwrap();
            input.set(mapped, true);
            assert_eq!(input.controls(1.0), expected, "{code:?}");
            input.set(mapped, false);
            assert_eq!(input.controls(1.0), (0.0, 0.0, false));
        }
    }

    #[test]
    fn simultaneous_drive_keys_combine_axes() {
        let mut input = DriveInputState::default();
        input.set(DriveKey::Forward, true);
        input.set(DriveKey::Left, true);
        let (throttle, steering, handbrake) = input.controls(3.0);
        assert_eq!(throttle, 3.0);
        assert_eq!(steering, -1.0);
        assert!(!handbrake);
    }

    #[test]
    fn opposing_drive_keys_cancel_and_release() {
        let mut input = DriveInputState::default();
        input.set(DriveKey::Forward, true);
        input.set(DriveKey::Reverse, true);
        input.set(DriveKey::Left, true);
        input.set(DriveKey::Right, true);
        assert_eq!(input.controls(1.0), (0.0, 0.0, false));

        input.set(DriveKey::Reverse, false);
        input.set(DriveKey::Right, false);
        assert_eq!(input.controls(1.0), (1.0, -1.0, false));
    }

    #[test]
    fn clear_drops_held_drive_keys() {
        let mut input = DriveInputState::default();
        input.set(DriveKey::Forward, true);
        input.set(DriveKey::Handbrake, true);
        input.clear();
        assert_eq!(input.controls(1.0), (0.0, 0.0, false));
    }
}

#[cfg(test)]
mod gpu_diagnostics {
    use super::*;

    #[test]
    #[ignore = "requires a Vulkan adapter; no game resources"]
    fn paint_mask_gpu() {
        use nfs_assets::{AlphaMode, Material, Mesh, Texture, Vertex};
        let vertices = [
            ([-1., -1., 0.], [0., 1.]),
            ([1., -1., 0.], [1., 1.]),
            ([1., 1., 0.], [1., 0.]),
            ([-1., 1., 0.], [0., 0.]),
        ]
        .map(|(position, uv)| Vertex {
            position,
            normal: [0., 0., 1.],
            uv,
            color: [1.; 4],
        })
        .to_vec();
        let scene = Scene {
            car_collision_box: None,
            car_body_points: Vec::new(),
            car_driver_camera: None,
            meshes: vec![Mesh {
                name: "paint regression".into(),
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                material: 0,
            }],
            textures: vec![Texture {
                name: "lamp / paint / cutout".into(),
                width: 12,
                height: 1,
                rgba: (0..12)
                    .flat_map(|x| {
                        [
                            220,
                            220,
                            220,
                            if x < 4 {
                                255
                            } else if x < 8 {
                                204
                            } else {
                                0
                            },
                        ]
                    })
                    .collect(),
            }],
            materials: vec![Material {
                name: "exterior".into(),
                texture: Some(0),
                base_color: [1.; 4],
                paintable: true,
                alpha_mode: AlphaMode::Mask,
                alpha_cutoff: 0.,
                double_sided: true,
                depth_bias: 0,
                prelit: false,
                uv_scroll: None,
                env_slot: None,
            }],
            bounds: [[-1., -1., 0.], [1., 1., 0.]],
            diagnostics: vec![],
            ..Default::default()
        };
        let white = pollster::block_on(render_pixels(
            &scene,
            256,
            256,
            Some(0.),
            Some(0.),
            None,
            None,
            0,
        ))
        .unwrap();
        let red = pollster::block_on(render_pixels(
            &scene,
            256,
            256,
            Some(0.),
            Some(0.),
            None,
            None,
            1,
        ))
        .unwrap();
        let pixel = |data: &[u8], x: usize, y: usize| -> [u8; 4] {
            data[(y * 256 + x) * 4..(y * 256 + x + 1) * 4]
                .try_into()
                .unwrap()
        };
        assert_eq!(
            pixel(&white, 75, 128),
            pixel(&red, 75, 128),
            "lamp must not change"
        );
        assert_ne!(
            pixel(&white, 75, 128),
            pixel(&white, 0, 0),
            "lamp must be visible"
        );
        assert!(
            pixel(&red, 128, 128)[1] < pixel(&white, 128, 128)[1],
            "body must change"
        );
        assert_eq!(
            pixel(&red, 180, 128),
            pixel(&red, 0, 0),
            "alpha zero must cut out"
        );
        assert_eq!(
            pixel(&red, 128, 128)[3],
            255,
            "paint alpha must not become opacity"
        );
    }

    #[test]
    #[ignore = "requires a Vulkan adapter; no game resources"]
    fn coplanar_material_bias_gpu() {
        use nfs_assets::{AlphaMode, Material, Mesh, Texture, Vertex};
        let vertices = [
            ([-1., -1., 0.], [0., 1.]),
            ([1., -1., 0.], [1., 1.]),
            ([1., 1., 0.], [1., 0.]),
            ([-1., 1., 0.], [0., 0.]),
        ]
        .map(|(position, uv)| Vertex {
            position,
            normal: [0., 0., 1.],
            uv,
            color: [1.; 4],
        })
        .to_vec();
        let mesh = |material| Mesh {
            name: "coplanar".into(),
            vertices: vertices.clone(),
            indices: vec![0, 1, 2, 0, 2, 3],
            material,
        };
        let scene = Scene {
            car_collision_box: None,
            car_body_points: Vec::new(),
            car_driver_camera: None,
            // Deliberately put the detail first: article order must not win.
            meshes: vec![mesh(1), mesh(0)],
            textures: vec![
                Texture {
                    name: "paint".into(),
                    width: 1,
                    height: 1,
                    rgba: vec![220, 220, 220, 204],
                },
                Texture {
                    name: "lamp".into(),
                    width: 1,
                    height: 1,
                    rgba: vec![220, 220, 220, 255],
                },
            ],
            materials: (0..2)
                .map(|i| Material {
                    name: format!("mt{i}"),
                    texture: Some(i),
                    base_color: [1.; 4],
                    paintable: true,
                    alpha_mode: AlphaMode::Mask,
                    alpha_cutoff: 0.,
                    double_sided: true,
                    depth_bias: if i == 1 { -3 } else { 0 },
                    prelit: false,
                    uv_scroll: None,
                    env_slot: None,
                })
                .collect(),
            bounds: [[-1., -1., 0.], [1., 1., 0.]],
            diagnostics: vec![],
            ..Default::default()
        };
        let actual = pollster::block_on(render_pixels(
            &scene,
            64,
            64,
            Some(0.),
            Some(0.),
            None,
            None,
            1,
        ))
        .unwrap();
        let mut lamp = scene.clone();
        lamp.meshes.retain(|mesh| mesh.material == 1);
        let expected = pollster::block_on(render_pixels(
            &lamp,
            64,
            64,
            Some(0.),
            Some(0.),
            None,
            None,
            1,
        ))
        .unwrap();
        assert_eq!(
            actual, expected,
            "detail must cover coplanar body despite article order"
        );
    }

    #[test]
    #[ignore = "requires local game and Vulkan; writes layer diagnostics under local/builds"]
    fn headlight_layers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let scene = load_car(
            &read_game_files(&root.join("local/game"), "356b").unwrap(),
            "356b",
        )
        .unwrap();
        let output = root.join("local/builds/001-car-viewer/validation/layers");
        fs::create_dir_all(&output).unwrap();
        for (label, include_body) in [
            ("lights", false),
            ("lights-body", true),
            ("all-material-order", true),
        ] {
            let mut layer = scene.clone();
            if label == "all-material-order" {
                layer.meshes.sort_by_key(|mesh| mesh.material);
            } else {
                layer.meshes.retain(|mesh| {
                    mesh.name.starts_with("Light1#")
                        || (include_body && mesh.name.starts_with("Body#"))
                });
            }
            pollster::block_on(render_screenshot(
                &layer,
                960,
                600,
                None,
                None,
                None,
                None,
                1,
                &output.join(format!("{label}.png")),
            ))
            .unwrap();
        }
    }
}

fn parse_f32(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<f32, String> {
    next_value(iter, name)?
        .parse()
        .map_err(|_| format!("{name} requires a number"))
}

fn parse_u32(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<u32, String> {
    let value: u32 = next_value(iter, name)?
        .parse()
        .map_err(|_| format!("{name} requires a positive integer"))?;
    NonZeroU32::new(value)
        .map(NonZeroU32::get)
        .ok_or_else(|| format!("{name} must be greater than zero"))
}

/// Needle calibration of the first `FEData/Data/nfs5.car` record with this model
/// (`docs/re/cockpit.md`).
fn car_gauges(game_dir: &Path, model: &str) -> Option<[f32; 4]> {
    let bytes = fs::read(game_dir.join("FEData/Data/nfs5.car")).ok()?;
    let entries = nfs_formats::parse_car_catalogue(&bytes).ok()?;
    let entry = entries.iter().find(|e| e.model.eq_ignore_ascii_case(model))?;
    Some(entry.gauges)
}

fn read_game_files(root: &Path, car: &str) -> Result<AssetFiles, String> {
    if !root.is_dir() {
        return Err(format!("Game directory not found: {}", root.display()));
    }
    let mut files = BTreeMap::new();
    read_dir_recursive(root, root, car, &mut files)?;
    Ok(files)
}

fn read_dir_recursive(
    root: &Path,
    dir: &Path,
    car: &str,
    files: &mut AssetFiles,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            read_dir_recursive(root, &path, car, files)?;
        } else if kind.is_file() {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let selected = extension.as_deref() == Some("fsh")
                || (matches!(extension.as_deref(), Some("crp") | Some("tpg"))
                    && stem.eq_ignore_ascii_case(car))
                // Particle systems: Render/carpart.dat and Render/particle.dat (0x455570); sun and
                // lens flare: Render/flare.dat, flarepos.dat.
                || (extension.as_deref() == Some("dat")
                    && ["carpart", "particle", "flare", "flarepos"].iter().any(|s| stem.eq_ignore_ascii_case(s)));
            if !selected {
                continue;
            }
            let key = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.insert(key, data);
        }
    }
    Ok(())
}

fn read_track_files(root: &Path, track: &str) -> Result<AssetFiles, String> {
    if !root.is_dir() {
        return Err(format!("Game directory not found: {}", root.display()));
    }
    let mut files = BTreeMap::new();
    read_track_dir_recursive(root, root, track, &mut files)?;
    Ok(files)
}

fn read_track_dir_recursive(
    root: &Path,
    dir: &Path,
    track: &str,
    files: &mut AssetFiles,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            // Include Sky/ directory for sky textures
            read_track_dir_recursive(root, &path, track, files)?;
        } else if kind.is_file() {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();

            // Check if file is inside a Sky/ directory
            let in_sky_dir = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case("sky"))
                .unwrap_or(false);

            let selected = if stem.eq_ignore_ascii_case("animdefs")
                && extension.as_deref() == Some("txt")
            {
                true
            } else if in_sky_dir {
                // Inside Sky/: the track's sky textures (.fsh) and sky config (.bin, sun)
                matches!(extension.as_deref(), Some("fsh") | Some("bin"))
                    && stem.eq_ignore_ascii_case(track)
            } else if extension.as_deref() == Some("scn") {
                // .scn files use <track>_st<N>.scn naming — match by prefix
                stem.to_ascii_lowercase()
                    .starts_with(&track.to_ascii_lowercase())
            } else if extension.as_deref() == Some("lsp") {
                stem.eq_ignore_ascii_case(track) || stem.eq_ignore_ascii_case(&format!("{track}0"))
            } else {
                // Original: crp, fsh, env, edg, map, jnc with exact stem match
                matches!(
                    extension.as_deref(),
                    Some("crp")
                        | Some("fsh")
                        | Some("env")
                        | Some("edg")
                        | Some("map")
                        | Some("jnc")
                ) && stem.eq_ignore_ascii_case(track)
            };
            if !selected {
                continue;
            }
            let key = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.insert(key, data);
        }
    }
    Ok(())
}

fn run_window(
    scene: Scene,
    title: &str,
    yaw: Option<f32>,
    pitch: Option<f32>,
    distance: Option<f32>,
    center: Option<[f32; 3]>,
) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("Event loop unavailable: {e}"))?;
    let mut app = App::new(scene, title, yaw, pitch, distance, center);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Window loop failed: {e}"))
}

fn run_catalog_window(catalog: NativeCatalog) -> Result<(), String> {
    let (scene, title) = load_catalog_scene(&catalog)?;
    let event_loop = EventLoop::new().map_err(|e| format!("Event loop unavailable: {e}"))?;
    let mut app = App::new(scene, &title, None, None, None, None);
    app.catalog = Some(catalog);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Window loop failed: {e}"))
}

fn load_catalog_scene(catalog: &NativeCatalog) -> Result<(Scene, String), String> {
    let selected = catalog.selected();
    match selected.kind {
        NativeTargetKind::Car => {
            let files = read_game_files(&catalog.game_dir, &selected.name)?;
            Ok((load_car(&files, &selected.name)?, selected.name))
        }
        NativeTargetKind::Track => {
            let files = read_track_files(&catalog.game_dir, &selected.name)?;
            Ok((
                load_track(&files, &selected.name)?,
                format!("Track {}", selected.name),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RaceDriveTelemetry {
    phase: u32,
    pos: usize,
    total_pos: usize,
    lap: u32,
    total_laps: u32,
    lap_time: f32,
    wrong_way: bool,
    countdown: f32,
}

#[cfg(target_os = "windows")]
mod native_audio {
    unsafe extern "system" {
        fn Beep(dwFreq: u32, dwDuration: u32) -> i32;
    }

    pub fn play_cue_async(freq: u32, duration_ms: u32) {
        std::thread::spawn(move || unsafe {
            let _ = Beep(freq, duration_ms);
        });
    }

    pub fn cue_pass() {
        std::thread::spawn(move || unsafe {
            let _ = Beep(880, 100);
            let _ = Beep(1175, 150);
            let _ = Beep(1760, 200);
        });
    }

    pub fn cue_fail() {
        std::thread::spawn(move || unsafe {
            let _ = Beep(440, 150);
            let _ = Beep(330, 250);
        });
    }
}

#[cfg(not(target_os = "windows"))]
mod native_audio {
    pub fn play_cue_async(_freq: u32, _duration_ms: u32) {}
    pub fn cue_pass() {}
    pub fn cue_fail() {}
}

struct App {
    scene: Option<Scene>,
    title: String,
    catalog: Option<NativeCatalog>,
    load_error: Option<String>,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<Renderer>,
    dragging_orbit: bool,
    dragging_pan: bool,
    cursor: Option<(f64, f64)>,
    shift_pressed: bool,
    focused: bool,
    drive_input: DriveInputState,
    last_frame: Option<Instant>,
    initial_yaw: Option<f32>,
    initial_pitch: Option<f32>,
    initial_distance: Option<f32>,
    initial_center: Option<[f32; 3]>,
    shell: nfs_game::shell::GameShell,
    profile_path: PathBuf,
    race_finished_handled: bool,
}

impl App {
    fn new(
        scene: Scene,
        title: &str,
        yaw: Option<f32>,
        pitch: Option<f32>,
        distance: Option<f32>,
        center: Option<[f32; 3]>,
    ) -> Self {
        let profile_dir = PathBuf::from("local/profiles");
        let profile_path = profile_dir.join("default.json");
        let profile = match nfs_game::profile::PlayerProfile::load_from_path(&profile_path) {
            Ok(p) => p,
            Err(_) => {
                let mut p = nfs_game::profile::PlayerProfile::new("Player");
                let _ = p.buy_initial_356();
                let _ = p.save_atomic(&profile_path);
                p
            }
        };
        let shell = nfs_game::shell::GameShell::with_profile(profile);

        Self {
            scene: Some(scene),
            title: title.to_string(),
            catalog: None,
            load_error: None,
            window: None,
            surface: None,
            config: None,
            renderer: None,
            dragging_orbit: false,
            dragging_pan: false,
            cursor: None,
            shift_pressed: false,
            focused: true,
            drive_input: DriveInputState::default(),
            last_frame: None,
            initial_yaw: yaw,
            initial_pitch: pitch,
            initial_distance: distance,
            initial_center: center,
            shell,
            profile_path,
            race_finished_handled: false,
        }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(self.window_title())
                        .with_inner_size(PhysicalSize::new(1280, 720)),
                )
                .map_err(|e| format!("Window unavailable: {e}"))?,
        );
        let size = window.inner_size();
        let instance = instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Surface unavailable: {e}"))?;
        let (adapter, device, queue) = pollster::block_on(request_gpu(&instance, Some(&surface)))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or("Surface has no color formats")?;
        let present_mode = caps
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .unwrap_or(caps.present_modes[0]);
        let alpha_mode = caps.alpha_modes[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            // Non-sRGB view for the lens flare's gamma-space add (sun.rs).
            view_formats: vec![format.remove_srgb_suffix()],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(
            device,
            queue,
            format,
            config.width,
            config.height,
            self.scene.as_ref().ok_or("Scene missing")?,
        )?;
        if let Some(yaw) = self.initial_yaw {
            renderer.camera.yaw = yaw;
        }
        if let Some(pitch) = self.initial_pitch {
            renderer.camera.pitch = pitch.clamp(-1.45, 1.45);
        }
        if let Some(distance) = self.initial_distance {
            renderer.camera.distance = distance;
            renderer.camera.initial_distance = distance;
        }
        if let Some(center) = self.initial_center {
            renderer.camera.center = glam::Vec3::from(center);
            renderer.camera.initial_center = renderer.camera.center;
        }

        self.window = Some(window);
        self.surface = Some(surface);
        self.config = Some(config);
        self.renderer = Some(renderer);
        self.update_window_title();
        Ok(())
    }

    fn switch_catalog_kind(&mut self) {
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        catalog.switch_kind();
        self.reload_catalog_scene();
    }

    fn advance_catalog(&mut self, delta: isize) {
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        catalog.advance(delta);
        self.reload_catalog_scene();
    }

    fn reload_catalog_scene(&mut self) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        match load_catalog_scene(catalog) {
            Ok((scene, title)) => {
                self.title = title;
                if let (Some(renderer), Some(config)) = (&self.renderer, &self.config) {
                    let mut next = match Renderer::new(
                        renderer.device.clone(),
                        renderer.queue.clone(),
                        config.format,
                        config.width.max(1),
                        config.height.max(1),
                        &scene,
                    ) {
                        Ok(renderer) => renderer,
                        Err(error) => {
                            self.load_error = Some(error.clone());
                            eprintln!("Catalog render setup failed: {error}");
                            self.update_window_title();
                            return;
                        }
                    };
                    next.set_show_topology(renderer.show_topology);
                    self.renderer = Some(next);
                }
                self.scene = Some(scene);
                self.load_error = None;
            }
            Err(error) => {
                self.load_error = Some(error.clone());
                eprintln!("Catalog load failed: {error}");
            }
        }
        self.reset_input_state();
        self.update_window_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_title(&self) -> String {
        let prof_info = self
            .shell
            .profile
            .as_ref()
            .map(|p| {
                format!(
                    " | Профиль: {} ({} CR | Турнир 356: {} | Фабрика: {})",
                    p.name,
                    p.credits,
                    if p.evolution.completed_tournaments.contains(&0) {
                        "Пройден"
                    } else {
                        "Открыт"
                    },
                    p.factory_driver.current_mission_code
                )
            })
            .unwrap_or_default();
        let mut title = if let Some(catalog) = &self.catalog {
            format!(
                "Porsche Viewer | {} {}/{}: {}{} | Tab группа, [/] выбор, F заезд, F2 Evolution, F3 Factory, T топология",
                catalog.current_kind_label(),
                catalog.current_index() + 1,
                catalog.current_len(),
                self.title,
                prof_info
            )
        } else {
            format!("{}{}", self.title, prof_info)
        };
        if let Some(error) = &self.load_error {
            title.push_str(" | Load error: ");
            title.push_str(&compact_title_text(error, 110));
        }
        title
    }

    fn update_window_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&self.window_title());
        }
    }

    fn reset_input_state(&mut self) {
        self.dragging_orbit = false;
        self.dragging_pan = false;
        self.cursor = None;
        self.shift_pressed = false;
        self.drive_input.clear();
        self.last_frame = Some(Instant::now());
    }

    fn drive_is_animating(&self) -> bool {
        self.focused
            && self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.is_drive_mode() && renderer.has_car())
    }

    fn frame_dt(&mut self) -> f32 {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|last_frame| now.duration_since(last_frame).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.0, 0.05);
        self.last_frame = Some(now);
        dt
    }

    fn update_drive(&mut self, dt: f32) {
        let throttle_multiplier = if self.shift_pressed { 3.0 } else { 1.0 };
        let (throttle, steering, handbrake) = self.drive_input.controls(throttle_multiplier);
        let mut status = None;
        if let Some(renderer) = &mut self.renderer
            && renderer.is_drive_mode()
            && renderer.has_car()
        {
            renderer.update_car(dt, throttle, steering, handbrake);
            let race_info = if renderer.race_session.is_some() {
                Some(RaceDriveTelemetry {
                    phase: renderer.get_race_phase(),
                    pos: renderer.get_player_position(),
                    total_pos: renderer.get_total_participants(),
                    lap: renderer.get_current_lap(),
                    total_laps: renderer.get_total_laps(),
                    lap_time: renderer.get_current_lap_time(),
                    wrong_way: renderer.is_wrong_way(),
                    countdown: renderer.get_countdown_remaining(),
                })
            } else {
                None
            };
            status = Some((
                renderer.get_car_speed_kmh(),
                renderer.get_car_gear(),
                race_info,
            ));
        }
        if let Some((speed, gear, race_info)) = status {
            if let Some(r) = race_info
                && (r.phase == 4 || r.phase == 5)
                && !self.race_finished_handled
            {
                self.race_finished_handled = true;
                self.shell.finish_race(r.lap_time, r.pos);
                if let Some(prof) = &self.shell.profile {
                    let _ = prof.save_atomic(&self.profile_path);
                }
                if r.pos == 1 {
                    native_audio::cue_pass();
                } else {
                    native_audio::cue_fail();
                }
                self.update_window_title();
            }
            self.update_drive_title(speed, gear, race_info);
        }
    }

    fn update_drive_title(&self, speed: f32, gear: i32, race_info: Option<RaceDriveTelemetry>) {
        let Some(window) = &self.window else {
            return;
        };
        let gear_str = if gear < 0 {
            "R".into()
        } else if gear == 0 {
            "N".into()
        } else {
            format!("{gear}")
        };

        let mut race_str = String::new();
        if let Some(r) = race_info {
            if r.wrong_way {
                race_str.push_str(" | ⚠️ WRONG WAY!");
            }
            match r.phase {
                1 => {
                    race_str.push_str(&format!(" | СТАРТ ЧЕРЕЗ {:.1}c", r.countdown));
                }
                2 => {
                    let mins = (r.lap_time / 60.0).floor() as u32;
                    let secs = r.lap_time % 60.0;
                    race_str.push_str(&format!(
                        " | P{}/{} | Круг {}/{} | {:02}:{:04.1}",
                        r.pos, r.total_pos, r.lap, r.total_laps, mins, secs
                    ));
                }
                4 | 5 => {
                    race_str.push_str(&format!(" | ФИНИШ! P{}/{}", r.pos, r.total_pos));
                }
                _ => {}
            }
        }

        window.set_title(&format!(
            "{} | {:.0} км/ч [{}]{}",
            self.window_title(),
            speed,
            gear_str,
            race_str
        ));
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            if let Some(renderer) = &mut self.renderer {
                renderer.resize(0, 0);
            }
            return;
        }
        let (Some(surface), Some(config), Some(renderer)) =
            (&self.surface, &mut self.config, &mut self.renderer)
        else {
            return;
        };
        config.width = size.width;
        config.height = size.height;
        surface.configure(&renderer.device, config);
        renderer.resize(size.width, size.height);
    }

    fn draw(&mut self) -> Result<(), String> {
        let dt = self.frame_dt();
        if self.focused {
            self.update_drive(dt);
        }
        let (Some(surface), Some(renderer)) = (&self.surface, &mut self.renderer) else {
            return Ok(());
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let (Some(config), Some(renderer)) = (&self.config, &self.renderer) {
                    surface.configure(&renderer.device, config);
                }
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface frame validation failed".into());
            }
        };
        let view = frame.texture.create_view(&Default::default());
        renderer.render(&view);
        renderer.queue.present(frame);
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let res = self.initialize(event_loop);
            if let Err(error) = res {
                eprintln!("{error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    self.resize(window.inner_size());
                }
            }
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.reset_input_state();
            }
            WindowEvent::Focused(true) => {
                self.focused = true;
                self.last_frame = Some(Instant::now());
                if self.drive_is_animating()
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw() {
                    eprintln!("{error}");
                    event_loop.exit();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_pressed = modifiers.state().shift_key();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        if self.shift_pressed {
                            self.dragging_pan = pressed;
                            self.dragging_orbit = false;
                        } else {
                            self.dragging_orbit = pressed;
                            self.dragging_pan = false;
                        }
                    }
                    MouseButton::Right | MouseButton::Middle => {
                        self.dragging_pan = pressed;
                        self.dragging_orbit = false;
                    }
                    _ => {}
                }
                if !self.dragging_orbit && !self.dragging_pan {
                    self.cursor = None;
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if !self.dragging_orbit && !self.dragging_pan {
                    self.cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some((x, y)), Some(renderer)) = (self.cursor, &mut self.renderer) {
                    let dx = (position.x - x) as f32;
                    let dy = (position.y - y) as f32;
                    if self.dragging_orbit {
                        renderer.camera.orbit(dx, dy);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    } else if self.dragging_pan {
                        renderer.camera.pan(dx, dy);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(renderer) = &mut self.renderer {
                    let amount = match delta {
                        MouseScrollDelta::LineDelta(_, y) => -y * 120.0,
                        MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
                    };
                    renderer.camera.zoom(amount);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if let Some(key) = drive_key(code) {
                    if !pressed {
                        self.drive_input.set(key, false);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        return;
                    }
                    if self.drive_is_animating() {
                        self.drive_input.set(key, true);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        return;
                    }
                }

                if !pressed {
                    return;
                }
                let speed_mult = if self.shift_pressed { 3.0 } else { 1.0 };
                match code {
                    KeyCode::Tab if !event.repeat => {
                        self.switch_catalog_kind();
                    }
                    KeyCode::BracketLeft | KeyCode::PageUp if !event.repeat => {
                        self.advance_catalog(-1);
                    }
                    KeyCode::BracketRight | KeyCode::PageDown if !event.repeat => {
                        self.advance_catalog(1);
                    }
                    KeyCode::F2 if !event.repeat => {
                        if let Some(catalog) = &mut self.catalog
                            && catalog.select_track_by_name("canyon")
                        {
                            self.reload_catalog_scene();
                            if let Some(renderer) = &mut self.renderer {
                                if !renderer.is_drive_mode() {
                                    renderer.toggle_camera_mode();
                                }
                                renderer.restart_race();
                            }
                            self.race_finished_handled = false;
                            native_audio::play_cue_async(880, 80);
                        }
                    }
                    KeyCode::F3 if !event.repeat => {
                        if let Some(catalog) = &mut self.catalog
                            && catalog.select_track_by_name("skidpad")
                        {
                            self.reload_catalog_scene();
                            if let Some(renderer) = &mut self.renderer {
                                if !renderer.is_drive_mode() {
                                    renderer.toggle_camera_mode();
                                }
                                renderer.restart_race();
                            }
                            self.race_finished_handled = false;
                            native_audio::play_cue_async(990, 80);
                        }
                    }
                    KeyCode::KeyR if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            if renderer.is_drive_mode() && renderer.has_car() {
                                // R: original track reset; Shift+R: restart the race.
                                if self.shift_pressed {
                                    self.race_finished_handled = false;
                                    renderer.restart_race();
                                } else {
                                    renderer.request_car_reset();
                                }
                            } else {
                                renderer.camera.reset();
                            }
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyC if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            if renderer.is_drive_mode() && renderer.has_car() {
                                println!("Camera: {}", renderer.cycle_car_view());
                            } else {
                                renderer.cycle_paint_color();
                            }
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyL if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            let on = renderer.toggle_lights();
                            println!("Lights: {}", if on { "on" } else { "off" });
                        }
                    }
                    KeyCode::KeyV if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer
                            && renderer.is_drive_mode()
                            && renderer.has_car()
                        {
                            let free = renderer.toggle_free_camera();
                            println!("Free camera: {free} ({})", renderer.camera_name());
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyP if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            if renderer.has_car() {
                                renderer.cycle_car_paint();
                            } else {
                                renderer.cycle_paint_color();
                            }
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyM if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            let sim = renderer.toggle_sim_mode();
                            println!(
                                "Physics mode: {}",
                                if sim {
                                    "6 DOF Realistic Simulation (.sim curves, 4-wheel independent suspension)"
                                } else {
                                    "Prototype Arcade Car"
                                }
                            );
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyT if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            let on = renderer.toggle_topology();
                            println!("Topology overlay: {}", if on { "on" } else { "off" });
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyF if !event.repeat => {
                        if let Some(renderer) = &mut self.renderer {
                            let drive = renderer.toggle_camera_mode();
                            self.drive_input.clear();
                            self.last_frame = Some(Instant::now());
                            println!(
                                "Mode: {}",
                                if drive {
                                    if renderer.has_car() {
                                        "Arcade Car Drive"
                                    } else {
                                        "Drive (fly-through)"
                                    }
                                } else {
                                    "Orbit"
                                }
                            );
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyW | KeyCode::ArrowUp => {
                        if let Some(renderer) = &mut self.renderer
                            && !(renderer.is_drive_mode() && renderer.has_car())
                        {
                            renderer.move_ground(speed_mult, 0.0);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyS | KeyCode::ArrowDown => {
                        if let Some(renderer) = &mut self.renderer
                            && !(renderer.is_drive_mode() && renderer.has_car())
                        {
                            renderer.move_ground(-speed_mult, 0.0);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyA | KeyCode::ArrowLeft => {
                        if let Some(renderer) = &mut self.renderer
                            && !(renderer.is_drive_mode() && renderer.has_car())
                        {
                            renderer.move_ground(0.0, -speed_mult);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyD | KeyCode::ArrowRight => {
                        if let Some(renderer) = &mut self.renderer
                            && !(renderer.is_drive_mode() && renderer.has_car())
                        {
                            renderer.move_ground(0.0, speed_mult);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyQ | KeyCode::Minus | KeyCode::NumpadSubtract => {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.camera.zoom(120.0 * speed_mult);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    KeyCode::KeyE | KeyCode::Equal | KeyCode::NumpadAdd => {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.camera.zoom(-120.0 * speed_mult);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if self.drive_is_animating()
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn render_screenshot(
    scene: &Scene,
    width: u32,
    height: u32,
    yaw: Option<f32>,
    pitch: Option<f32>,
    distance: Option<f32>,
    center: Option<[f32; 3]>,
    paint_index: u32,
    output: &Path,
) -> Result<(), String> {
    let pixels = render_pixels(
        scene,
        width,
        height,
        yaw,
        pitch,
        distance,
        center,
        paint_index,
    )
    .await?;
    let file = fs::File::create(output).map_err(|e| format!("{}: {e}", output.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| format!("PNG header failed: {e}"))?
        .write_image_data(&pixels)
        .map_err(|e| format!("PNG write failed: {e}"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn render_pixels(
    scene: &Scene,
    width: u32,
    height: u32,
    yaw: Option<f32>,
    pitch: Option<f32>,
    distance: Option<f32>,
    center: Option<[f32; 3]>,
    paint_index: u32,
) -> Result<Vec<u8>, String> {
    let instance = instance();
    let (_, device, queue) = request_gpu(&instance, None).await?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(device, queue, format, width, height, scene)?;
    for _ in 0..paint_index {
        renderer.cycle_paint_color();
    }
    if let Some(yaw) = yaw {
        renderer.camera.yaw = yaw;
    }
    if let Some(pitch) = pitch {
        renderer.camera.pitch = pitch.clamp(-1.45, 1.45);
    }
    if let Some(distance) = distance {
        renderer.camera.distance = distance;
    }
    if let Some(center) = center {
        renderer.camera.center = glam::Vec3::from(center);
        renderer.camera.radius = distance.unwrap_or(50.0);
    }
    capture_pixels(&mut renderer, width, height)
}

/// Render one frame of `renderer` into an offscreen target and read it back (RGBA8).
fn capture_pixels(renderer: &mut Renderer, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless screenshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        // Non-sRGB view for the lens flare's gamma-space add (sun.rs).
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });
    let target_view = target.create_view(&Default::default());
    renderer.render(&target_view);

    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = u64::from(padded_bytes_per_row) * u64::from(height);
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("GPU readback poll failed: {e}"))?;
    receiver
        .recv()
        .map_err(|e| format!("GPU readback failed: {e}"))?
        .map_err(|e| format!("GPU readback map failed: {e}"))?;

    let mapped = slice
        .get_mapped_range()
        .map_err(|e| format!("GPU readback map range failed: {e}"))?;
    let mut pixels = vec![0; (width * height * bytes_per_pixel) as usize];
    for row in 0..height as usize {
        let src = row * padded_bytes_per_row as usize;
        let dst = row * unpadded_bytes_per_row as usize;
        pixels[dst..dst + unpadded_bytes_per_row as usize]
            .copy_from_slice(&mapped[src..src + unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    buffer.unmap();

    Ok(pixels)
}

//! Road colour under a car, the `S` of `R3DCar_UpdateLighting` 0x43a560.
//!
//! The original interpolates the RGB555 vertex colours of the road polygon
//! under the car (`Road_SampleColour` 0x474700). Approximation A23: we take the
//! prelit `df` colour of the nearest track vertex near the car's height.

use glam::Vec3;
use nfs_assets::Scene;
use std::collections::HashMap;

const CELL: f32 = 4.0;
/// Only vertices within this height of the car count (bridges, tunnels).
const MAX_DY: f32 = 3.0;

/// Share of the prelit hue that is kept (approximation A22/A23, as
/// `PRELIT_SATURATION` in `car.wgsl`): the data is strongly blue on Alps,
/// Forest and Autobahn.
pub const PRELIT_SATURATION: f32 = 0.3;

/// Pull a prelit colour towards its grey (Rec. 601 luma) by `PRELIT_SATURATION`.
pub fn desaturate(c: [f32; 3]) -> [f32; 3] {
    let grey = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    c.map(|v| grey + (v - grey) * PRELIT_SATURATION)
}

pub struct ShadeGrid {
    cells: HashMap<(i32, i32), Vec<([f32; 3], [f32; 3])>>,
}

impl ShadeGrid {
    /// Index the prelit vertices of a track scene; `None` if it has none.
    pub fn from_scene(scene: &Scene) -> Option<Self> {
        let mut cells: HashMap<(i32, i32), Vec<([f32; 3], [f32; 3])>> = HashMap::new();
        for mesh in &scene.meshes {
            if !scene.materials.get(mesh.material).is_some_and(|m| m.prelit) {
                continue;
            }
            for v in &mesh.vertices {
                let key = Self::key(v.position[0], v.position[2]);
                cells
                    .entry(key)
                    .or_default()
                    .push((v.position, [v.color[0], v.color[1], v.color[2]]));
            }
        }
        (!cells.is_empty()).then_some(Self { cells })
    }

    fn key(x: f32, z: f32) -> (i32, i32) {
        ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
    }

    /// Colour of the nearest indexed vertex to `p` (3×3 cells, |dy| ≤ 3 m).
    pub fn sample(&self, p: Vec3) -> Option<[f32; 3]> {
        let (cx, cz) = Self::key(p.x, p.z);
        let mut best: Option<(f32, [f32; 3])> = None;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(list) = self.cells.get(&(cx + dx, cz + dz)) else {
                    continue;
                };
                for (pos, color) in list {
                    let d = Vec3::from(*pos) - p;
                    if d.y.abs() > MAX_DY {
                        continue;
                    }
                    let dist = d.x * d.x + d.z * d.z;
                    if best.is_none_or(|(b, _)| dist < b) {
                        best = Some((dist, *color));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_assets::{AlphaMode, Material, Mesh, Vertex};

    fn scene(points: &[([f32; 3], [f32; 4])]) -> Scene {
        Scene {
            meshes: vec![Mesh {
                name: "road".into(),
                vertices: points
                    .iter()
                    .map(|&(position, color)| Vertex {
                        position,
                        normal: [0.0, 1.0, 0.0],
                        uv: [0.0; 2],
                        color,
                    })
                    .collect(),
                indices: vec![],
                material: 0,
            }],
            materials: vec![Material {
                name: "mt0".into(),
                texture: None,
                base_color: [1.0; 4],
                paintable: false,
                alpha_mode: AlphaMode::Opaque,
                alpha_cutoff: 0.0,
                double_sided: false,
                depth_bias: 0,
                prelit: true,
                uv_scroll: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn desaturate_keeps_grey_and_luma() {
        assert_eq!(desaturate([0.5; 3]), [0.5; 3]);
        let c = desaturate([0.29, 0.42, 0.6]);
        let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        assert!((luma(c) - luma([0.29, 0.42, 0.6])).abs() < 1e-6);
        assert!(c[2] - c[0] < 0.1, "blue cast reduced: {c:?}");
    }

    #[test]
    fn nearest_vertex_at_the_car_height() {
        let grid = ShadeGrid::from_scene(&scene(&[
            ([0.0, 0.0, 0.0], [0.2, 0.2, 0.2, 1.0]),
            ([3.0, 0.0, 0.0], [0.8, 0.8, 0.8, 1.0]),
            // A bridge deck above: ignored.
            ([2.0, 10.0, 0.0], [1.0, 0.0, 0.0, 1.0]),
        ]))
        .unwrap();
        assert_eq!(grid.sample(Vec3::new(2.5, 0.5, 0.0)), Some([0.8; 3]));
        assert_eq!(grid.sample(Vec3::new(0.5, 0.5, 0.0)), Some([0.2; 3]));
        assert_eq!(grid.sample(Vec3::new(100.0, 0.0, 0.0)), None);
    }
}

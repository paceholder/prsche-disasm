//! The car's planar sky-map layer on the GPU (`R3DSkyMapTextureLayer` 0x445b10): the light
//! state of [`nfs_assets::car_env`] (recorded bit for bit) as the uniforms of `car.wgsl`
//! (`docs/re/lighting.md`, "Sky map on the car").
//!
//! The shader writes per vertex what 0x445b10 writes into the vertex UVs:
//! `u = dot(p + k·n − car, lat)·inv_size + (0.1·o1 + 0.5)`,
//! `v = dot(p + k·n − car, fwd)·aspect·inv_size + (o2 + 0.5)`, with `p`, `n` the world
//! position and normal of the vertex, `car` the car position, `lat` / `fwd` rows 0 and 2 of
//! the car's node frame `car+0x3b8` (the transposed frame of light state `+0x104`).

use glam::Vec3;
use nfs_assets::car_env::{EnvCar, EnvLight};
use nfs_assets::physics::original_adapter::from_original;

/// Uniform values of the layer: slot layers, `inv_size`, `aspect·inv_size`, `0.1·o1 + 0.5`,
/// `o2 + 0.5` (all f32 as the original stores them), the axes in scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvUniform {
    pub proj: [f32; 4],
    pub axis_u: Vec3,
    pub axis_v: Vec3,
    pub push: f32,
}

pub fn uniform(light: &EnvLight) -> EnvUniform {
    let x = |v: f32| v as f64;
    let c_u = (x(light.offsets[0]) * x(f32::from_bits(0x3dcc_cccd)) + 0.5) as f32;
    let c_v = (x(light.offsets[1]) + 0.5) as f32;
    let scale_v = (x(light.aspect) * x(light.inv_size)) as f32;
    let f = &light.frame_t;
    EnvUniform {
        proj: [light.inv_size, scale_v, c_u, c_v],
        // Columns 0 and 2 of the transposed frame: the frame's lateral and forward rows.
        axis_u: from_original([f[0], f[4], f[8]]),
        axis_v: from_original([f[2], f[6], f[10]]),
        push: light.k,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
/// CPU mirror of `car.wgsl` `vs_main` (f32 arithmetic, scene coordinates): the coordinates of
/// a vertex at world position `p` with world normal `n` for a car at `origin`.
pub fn shader_uv(u: &EnvUniform, origin: Vec3, p: Vec3, n: Vec3) -> [f32; 2] {
    let rel = p + n * u.push - origin;
    [rel.dot(u.axis_u) * u.proj[0] + u.proj[2], rel.dot(u.axis_v) * u.proj[1] + u.proj[3]]
}

/// The car's inputs from the original vehicle runtime: position, locator node, the node frame
/// `car+0x3b8`, and covered road from 0x473330 on the locator (`covered`).
pub fn env_car(
    car: &nfs_assets::physics::original_vehicle::OriginalCar,
    node: i16,
    covered: bool,
    nodes: &nfs_assets::race::original_ai::track::TrackNodes,
) -> Option<EnvCar> {
    let n = nodes.nodes.get(usize::try_from(node).ok()?)?;
    Some(EnvCar {
        position: car.position_330,
        node,
        covered,
        node_pos: n.pos_04,
        node_lat: n.lateral_28,
        node_fwd: n.forward_1c,
        frame: [car.track.node_lat_3b8, car.track.node_up_3c4, car.track.node_fwd_3d0],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_assets::car_env::{layer_matrix, layer_uv, UvPath};
    use nfs_assets::physics::original_adapter::to_original;

    const FIXTURE: &str = include_str!("../../nfs-assets/tests/fixtures/car-env-map.tsv");

    fn fields(s: &str) -> std::collections::HashMap<&str, &str> {
        s.split(';').filter_map(|p| p.split_once('=')).collect()
    }

    fn words(s: &str) -> Vec<f32> {
        s.split(',').map(|v| f32::from_bits(u32::from_str_radix(v, 16).unwrap())).collect()
    }

    /// The shader formula (f32, scene axes) against the recorded 0x445b10 coordinates of the
    /// unclipped draws: within 2e-5 of the texture size (the original sums in 53-bit x87).
    #[test]
    fn shader_formula_matches_the_recorded_layer() {
        let mut light: Option<EnvLight> = None;
        let mut checked = 0;
        for line in FIXTURE.lines().filter(|l| !l.starts_with('#')) {
            let c: Vec<&str> = line.splitn(4, '\t').collect();
            if c.len() < 4 {
                continue;
            }
            let x = fields(c[3]);
            match c[0] {
                "light" => {
                    let fr = words(x["frame"]);
                    let o = words(x["o"]);
                    let p = words(x["pos"]);
                    light = Some(EnvLight {
                        slots: [None; 3],
                        k: words(x["k"])[0],
                        inv_size: words(x["inv_size"])[0],
                        aspect: words(x["aspect"])[0],
                        position: [p[0], p[1], p[2]],
                        frame_t: std::array::from_fn(|i| fr[i]),
                        offsets: [o[0], o[1]],
                    });
                }
                "draw" if x["index"] == "-" && x["off"] == "0" && x["uv"] != "" => {
                    let l = light.as_ref().unwrap();
                    let m = words(x["model"]);
                    let model: [f32; 16] = std::array::from_fn(|i| m[i]);
                    let u = uniform(l);
                    let origin = from_original(l.position);
                    let mat = layer_matrix(l, &model);
                    for (vi, (v, rec)) in x["verts"].split('|').zip(x["uv"].split('|')).enumerate() {
                        if rec == "-" {
                            continue;
                        }
                        let c = words(v);
                        let (pm, nm) = ([c[0], c[1], c[2]], [c[3], c[4], c[5]]);
                        // Model to world (row vectors, original axes), then to scene axes.
                        let world = |q: [f32; 3], w: f32| {
                            [0, 1, 2].map(|j| q[0] * model[j] + q[1] * model[4 + j] + q[2] * model[8 + j] + w * model[12 + j])
                        };
                        let p = from_original(world(pm, 1.0));
                        let n = from_original(world(nm, 0.0));
                        let got = shader_uv(&u, origin, p, n);
                        let want = words(rec);
                        let exact = layer_uv(l, &mat, pm, nm, UvPath::Normal);
                        assert_eq!(exact.map(f32::to_bits), [want[0].to_bits(), want[1].to_bits()]);
                        for k in 0..2 {
                            let tol = 2e-5 * (1.0 + want[k].abs());
                            assert!((got[k] - want[k]).abs() <= tol, "vertex {vi}: {got:?} vs {want:?}");
                        }
                        let _ = to_original(p);
                        checked += 1;
                    }
                }
                _ => {}
            }
        }
        assert!(checked > 300, "{checked}");
    }
}

//! Planar environment map on the car body: `R3DSkyMapTextureLayer` 0x445b10 with the
//! `.map` sections and `.env` textures that `R3DCar_UpdateLighting` 0x43a560 selects
//! (`docs/re/lighting.md`, "Sky map on the car").
//!
//! Per frame the car's track node picks the `.map` section whose slice range holds it
//! (`TrackLighting_SelectSection` 0x441140; none → no layer). The projected point is the
//! vertex pushed out 3.33 units along its normal (light state `+0`, 0x440780). Texture slot k of the
//! layer is the section's `texture{k+1}.open` (`.covered` on covered road, 0x441180),
//! `1/texturesize` and `aspect` come from the section (0x4411c0/0x4411e0, defaults 6
//! and 1). The layer projects the body onto the car's node frame (lateral, forward;
//! the matrix ends with the inverse of `car+0x3b8`, 0x43a862) around the car:
//! `u = lateral/texturesize + 0.1·o1 + 0.5`, `v = forward·aspect/texturesize + o2 + 0.5`, where
//! `o1`, `o2` are the car's offsets from its node centre along the node's lateral and
//! forward axes divided by 6 (0x43a7c4..0x43a830).

use glam::Vec3;
use nfs_formats::topology::TrackMap;

/// One `.map` section: slice range, texture names of slots 0..2 (open road),
/// `texturesize`, `aspect`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSection {
    pub start: u32,
    pub end: u32,
    pub textures: [Option<String>; 3],
    pub texture_size: f32,
    pub aspect: f32,
}

/// `texture{k}.open=<int>`: a FourCC stored little-endian (`538976305` = `"1   "`).
fn fourcc_name(value: &str) -> Option<String> {
    let v: i64 = value.trim().parse().ok()?;
    let bytes = (v as u32).to_le_bytes();
    let name: String = bytes.iter().map(|&b| b as char).collect();
    let name = name.trim_end().to_string();
    (!name.is_empty()).then_some(name)
}

pub fn sections(map: &TrackMap) -> Vec<EnvSection> {
    map.sections
        .iter()
        .map(|s| EnvSection {
            start: s.startslice,
            end: s.endslice,
            textures: [1, 2, 3].map(|k| {
                s.properties.get(&format!("texture{k}.open")).and_then(|v| fourcc_name(v))
            }),
            texture_size: s.texturesize,
            aspect: s.aspect,
        })
        .collect()
}

/// `TrackLighting_SelectSection` 0x441140: the first section with `start <= node <= end`.
pub fn select(sections: &[EnvSection], node: i32) -> Option<&EnvSection> {
    let node = u32::try_from(node).ok()?;
    sections.iter().find(|s| s.start <= node && node <= s.end)
}

/// Offsets `o1` (lateral) and `o2` (forward) of 0x43a7c4: the car position minus the
/// node centre, dotted with the node's lateral (`+0x28`) and forward (`+0x1c`) axes,
/// divided by 6 (original coordinates).
pub fn node_offsets(car: Vec3, centre: Vec3, forward: Vec3, lateral: Vec3) -> [f32; 2] {
    let d = car - centre;
    [d.dot(lateral) / 6.0, d.dot(forward) / 6.0]
}

/// Texture coordinate of a body point relative to the car (original x/z) for the
/// section (`R3DSkyMapTextureLayer` 0x445b10). CPU reference of `car.wgsl` `vs_main`.
#[allow(dead_code)]
pub fn planar_uv(rel_x: f32, rel_z: f32, section: &EnvSection, offsets: [f32; 2]) -> [f32; 2] {
    [
        rel_x / section.texture_size + 0.1 * offsets[0] + 0.5,
        rel_z * section.aspect / section.texture_size + offsets[1] + 0.5,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(start: u32, end: u32, names: [&str; 3]) -> EnvSection {
        EnvSection {
            start,
            end,
            textures: names.map(|n| Some(n.to_string())),
            texture_size: 20.0,
            aspect: 1.0,
        }
    }

    #[test]
    fn fourcc_values_name_env_textures() {
        assert_eq!(fourcc_name("538976305").as_deref(), Some("1"));
        assert_eq!(fourcc_name("540103764").as_deref(), Some("TT1"));
        assert_eq!(fourcc_name("827085908").as_deref(), Some("TTL1"));
        assert_eq!(fourcc_name("542722899").as_deref(), Some("SKY"));
    }

    #[test]
    fn node_outside_every_section_has_no_layer() {
        let s = vec![section(0, 0, ["1", "1", "1"]), section(56, 105, ["TT1", "TT1", "TT2"])];
        assert_eq!(select(&s, 0).unwrap().start, 0);
        assert_eq!(select(&s, 80).unwrap().textures[2].as_deref(), Some("TT2"));
        assert!(select(&s, 106).is_none());
        assert!(select(&s, -1).is_none());
    }

    #[test]
    fn offsets_and_projection() {
        let o = node_offsets(Vec3::new(3.0, 0.0, 12.0), Vec3::ZERO, Vec3::Z, Vec3::X);
        assert_eq!(o, [0.5, 2.0]);
        let s = section(0, 10, ["TT1"; 3]);
        assert_eq!(planar_uv(10.0, -5.0, &s, o), [10.0 / 20.0 + 0.05 + 0.5, -0.25 + 2.0 + 0.5]);
    }
}
